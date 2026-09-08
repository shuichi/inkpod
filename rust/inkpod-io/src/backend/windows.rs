use super::{FileIdentity, FileStamp};
use crate::{IoError, IoResult};
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::Path;

#[repr(C)]
#[derive(Default)]
struct HandleInformation {
    attributes: u32,
    creation_time: [u32; 2],
    access_time: [u32; 2],
    write_time: [u32; 2],
    volume: u32,
    size_high: u32,
    size_low: u32,
    links: u32,
    index_high: u32,
    index_low: u32,
}

#[repr(C)]
#[derive(Default)]
struct BasicInformation {
    creation_time: i64,
    access_time: i64,
    write_time: i64,
    change_time: i64,
    attributes: u32,
}

#[repr(C)]
#[derive(Default)]
struct IdentifierInformation {
    volume: u64,
    file: [u8; 16],
}

#[repr(C)]
struct DispositionInformation {
    delete_file: i32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn FindFirstChangeNotificationW(
        path: *const u16,
        watch_subtree: i32,
        notify_filter: u32,
    ) -> *mut c_void;
    fn FindCloseChangeNotification(handle: *mut c_void) -> i32;
    fn GetFileInformationByHandle(handle: *mut c_void, information: *mut HandleInformation) -> i32;
    fn GetFileInformationByHandleEx(
        handle: *mut c_void,
        class: u32,
        information: *mut c_void,
        bytes: u32,
    ) -> i32;
    fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    fn SetFileInformationByHandle(
        handle: *mut c_void,
        class: u32,
        information: *const c_void,
        bytes: u32,
    ) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
}

pub(super) struct DirectoryChangeObserver {
    handle: isize,
}

impl DirectoryChangeObserver {
    pub(super) fn new(path: &Path) -> IoResult<Self> {
        const FILE_NOTIFY_CHANGE_FILE_NAME: u32 = 0x0000_0001;
        const FILE_NOTIFY_CHANGE_DIR_NAME: u32 = 0x0000_0002;
        let path = wide_path(path)?;
        // SAFETY: `path` is NUL-terminated and remains live during this
        // synchronous call. The returned notification handle is owned by the
        // observer and closed exactly once in Drop.
        let handle = unsafe {
            FindFirstChangeNotificationW(
                path.as_ptr(),
                0,
                FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME,
            )
        };
        if handle as isize == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self {
            handle: handle as isize,
        })
    }

    pub(super) fn unchanged(&self) -> IoResult<bool> {
        const WAIT_OBJECT_0: u32 = 0x0000_0000;
        const WAIT_TIMEOUT: u32 = 0x0000_0102;
        const WAIT_FAILED: u32 = 0xffff_ffff;
        // SAFETY: Drop is the sole closer and cannot run while this shared
        // reference is live. A zero timeout only queries the signal state.
        match unsafe { WaitForSingleObject(self.handle as *mut c_void, 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            WAIT_FAILED => Err(std::io::Error::last_os_error().into()),
            _ => Err(IoError::InvalidInput(
                "directory change observer returned an unknown wait state",
            )),
        }
    }
}

impl Drop for DirectoryChangeObserver {
    fn drop(&mut self) {
        // SAFETY: `handle` is a live change-notification handle created by
        // FindFirstChangeNotificationW and this Drop is its unique owner.
        let _ = unsafe { FindCloseChangeNotification(self.handle as *mut c_void) };
    }
}

pub(super) fn stamp(file: &File) -> IoResult<FileStamp> {
    let (information, basic, identifier) = query_stamp(file)?;
    if information.attributes & 0x10 != 0 {
        return Err(IoError::InvalidInput("image input is not a regular file"));
    }
    Ok(FileStamp {
        identity: FileIdentity {
            volume: identifier.volume,
            file: u128::from_le_bytes(identifier.file),
        },
        length: (u64::from(information.size_high) << 32) | u64::from(information.size_low),
        modified: i128::from(basic.write_time),
        changed: i128::from(basic.change_time),
        readonly: information.attributes & 1 != 0,
    })
}

fn query_stamp(
    file: &File,
) -> IoResult<(HandleInformation, BasicInformation, IdentifierInformation)> {
    let mut information = HandleInformation::default();
    let mut basic = BasicInformation::default();
    let mut identifier = IdentifierInformation::default();
    // SAFETY: File owns a live handle throughout the synchronous calls. All
    // output records match SDK C layouts and expose their writable exact sizes.
    let valid = unsafe {
        GetFileInformationByHandle(file.as_raw_handle(), &mut information) != 0
            && GetFileInformationByHandleEx(
                file.as_raw_handle(),
                0, // FileBasicInfo
                (&raw mut basic).cast(),
                size_of::<BasicInformation>() as u32,
            ) != 0
            && GetFileInformationByHandleEx(
                file.as_raw_handle(),
                18, // FileIdInfo: full 64-bit volume and 128-bit object identity.
                (&raw mut identifier).cast(),
                size_of::<IdentifierInformation>() as u32,
            ) != 0
    };
    if !valid {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((information, basic, identifier))
}

pub(super) fn replace(source: &Path, destination: &Path, overwrite: bool) -> IoResult<()> {
    let source = wide_path(source)?;
    let destination = wide_path(destination)?;
    let flags = 0x8 | u32::from(overwrite); // WRITE_THROUGH | optional REPLACE_EXISTING
    // SAFETY: Both path buffers are NUL-terminated, contain no interior NUL, and
    // remain live through this synchronous Win32 call. No raw pointer escapes.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

pub(super) fn object_identity(file: &File) -> IoResult<FileIdentity> {
    let (information, _, identifier) = query_stamp(file)?;
    if information.attributes & 0x400 != 0 {
        return Err(IoError::InvalidInput("reparse authority is unsupported"));
    }
    Ok(FileIdentity {
        volume: identifier.volume,
        file: u128::from_le_bytes(identifier.file),
    })
}

pub(super) fn path_object_identity(path: &Path) -> IoResult<FileIdentity> {
    let file = OpenOptions::new()
        .access_mode(0x80)
        .share_mode(7)
        .custom_flags(0x0220_0000)
        .open(path)?;
    object_identity(&file)
}

pub(super) fn open_authority_directory(path: &Path, write: bool) -> IoResult<File> {
    // No FILE_SHARE_DELETE: the parent cannot be moved/replaced while the
    // relative rename handle is retained. OPEN_REPARSE_POINT rejects redirects.
    let file = OpenOptions::new()
        .access_mode(if write { 0x0010_00a7 } else { 0x0010_00a1 })
        .share_mode(3)
        .custom_flags(0x0220_0000)
        .open(path)?;
    let (information, _, _) = query_stamp(&file)?;
    if information.attributes & 0x10 == 0 || information.attributes & 0x400 != 0 {
        return Err(IoError::InvalidInput(
            "publication parent is not a plain directory",
        ));
    }
    Ok(file)
}

pub(super) fn open_authority_source(path: &Path, overwrite: bool) -> IoResult<File> {
    // Deny WRITE sharing through the final digest check and publication. Only
    // overwrite allows DELETE sharing, which the POSIX rename needs while this
    // source handle remains open. It also permits foreign name changes; the
    // caller rechecks the path, but cannot exclude changes after that check.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(if overwrite { 5 } else { 1 })
        .custom_flags(0x0020_0000)
        .open(path)?;
    object_identity(&file)?;
    stamp(&file)?;
    Ok(file)
}

pub(super) fn open_authority_temporary(parent: &File, path: &Path) -> IoResult<File> {
    let file = open_authority_child(parent, path, 0x0011_0080, 5, 1, 0x0020_0060, 0)?;
    object_identity(&file)?;
    stamp(&file)?;
    Ok(file)
}

pub(super) fn remove_authority_temporary(
    parent: &File,
    path: &Path,
    expected: FileIdentity,
) -> IoResult<()> {
    let file = open_authority_child(parent, path, 0x0011_0080, 0, 1, 0x0020_0060, 0)?;
    if object_identity(&file)? != expected {
        return Err(IoError::ConfirmationRequired);
    }
    set_disposition(&file, true)
}

#[repr(C)]
struct RenameInformation {
    flags: u32,
    root_directory: *mut c_void,
    file_name_length: u32,
    file_name: [u16; 1],
}
#[repr(C)]
struct IoStatusBlock {
    status: isize,
    information: usize,
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetInformationFile(
        file: *mut c_void,
        status: *mut IoStatusBlock,
        information: *const c_void,
        length: u32,
        class: u32,
    ) -> i32;
    fn NtCreateFile(
        handle: *mut *mut c_void,
        access: u32,
        attributes: *const ObjectAttributes,
        status: *mut IoStatusBlock,
        allocation: *const i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *const c_void,
        ea_length: u32,
    ) -> i32;
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root: *mut c_void,
    name: *const UnicodeString,
    attributes: u32,
    security: *const c_void,
    quality: *const c_void,
}

pub(super) fn create_authority_child(
    parent: &File,
    path: &Path,
    directory: bool,
) -> IoResult<File> {
    open_authority_child(
        parent,
        path,
        if directory { 0x0010_00a1 } else { 0x0012_019f },
        if directory { 3 } else { 5 },
        2,
        0x0020_0020 | if directory { 1 } else { 0x40 },
        if directory { 0x10 } else { 0x80 },
    )
}

fn open_authority_child(
    parent: &File,
    path: &Path,
    access: u32,
    sharing: u32,
    disposition: u32,
    options: u32,
    file_attributes: u32,
) -> IoResult<File> {
    let name: Vec<u16> = path
        .file_name()
        .ok_or(IoError::InvalidInput("child name is missing"))?
        .encode_wide()
        .collect();
    let length = u16::try_from(name.len() * 2)
        .map_err(|_| IoError::LimitExceeded("child name is too long"))?;
    let name = UnicodeString {
        length,
        maximum_length: length,
        buffer: name.as_ptr(),
    };
    let attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root: parent.as_raw_handle(),
        name: &name,
        attributes: 0x40,
        security: std::ptr::null(),
        quality: std::ptr::null(),
    };
    let mut handle = std::ptr::null_mut();
    let mut status = IoStatusBlock {
        status: 0,
        information: 0,
    };
    // SAFETY: the parent handle and aligned fixed-layout records, including the
    // UTF-16 buffer owned above, live through the synchronous call. Success
    // transfers the newly allocated kernel handle once into File ownership.
    let result = unsafe {
        NtCreateFile(
            &raw mut handle,
            access,
            &attributes,
            &raw mut status,
            std::ptr::null(),
            file_attributes,
            sharing,
            disposition,
            options,
            std::ptr::null(),
            0,
        )
    };
    if result as u32 == 0xc0000035 {
        return Err(IoError::Io(std::io::ErrorKind::AlreadyExists.into()));
    }
    if result < 0 {
        return Err(IoError::Io(std::io::Error::other(
            "handle-relative child creation failed",
        )));
    }
    // SAFETY: NtCreateFile succeeded and returns a uniquely owned valid handle.
    Ok(unsafe { File::from_raw_handle(handle) })
}

pub(super) fn rename_with_authority(
    file: &File,
    parent: &File,
    destination: &Path,
    overwrite: bool,
) -> IoResult<()> {
    let name: Vec<u16> = destination
        .file_name()
        .ok_or(IoError::InvalidInput("destination has no name"))?
        .encode_wide()
        .collect();
    let offset = std::mem::offset_of!(RenameInformation, file_name);
    let bytes = offset
        .checked_add(
            name.len()
                .checked_mul(2)
                .ok_or(IoError::LimitExceeded("rename name too long"))?,
        )
        .ok_or(IoError::LimitExceeded("rename name too long"))?;
    let size = u32::try_from(bytes).map_err(|_| IoError::LimitExceeded("rename name too long"))?;
    // usize storage supplies the alignment required by the SDK pointer fields.
    let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
    let record = storage.as_mut_ptr().cast::<RenameInformation>();
    let mut status = IoStatusBlock {
        status: 0,
        information: 0,
    };
    // SAFETY: aligned storage has the exact header plus UTF-16 filename extent;
    // both owned File handles and all buffers outlive this synchronous NT call.
    let result = unsafe {
        // Request POSIX replacement while retaining the source WRITE exclusion.
        // A sharing conflict fails closed; never release the guard and retry.
        (*record).flags = if overwrite { 3 } else { 0 };
        (*record).root_directory = parent.as_raw_handle();
        (*record).file_name_length = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            name.len(),
        );
        NtSetInformationFile(
            file.as_raw_handle(),
            &raw mut status,
            record.cast(),
            size,
            if overwrite { 65 } else { 10 },
        )
    };
    if result < 0 {
        // FileRenameInformationEx capability failures are distinct from a
        // supported rename refused by this file's access/sharing permissions.
        // Keep either failure closed; never drop the guard or retry a weaker rename.
        const STATUS_NOT_IMPLEMENTED: u32 = 0xc0000002;
        const STATUS_INVALID_INFO_CLASS: u32 = 0xc0000003;
        const STATUS_NOT_SUPPORTED: u32 = 0xc00000bb;
        if overwrite
            && matches!(
                result as u32,
                STATUS_NOT_IMPLEMENTED | STATUS_INVALID_INFO_CLASS | STATUS_NOT_SUPPORTED
            )
        {
            return Err(IoError::UnsupportedAtomicPublication);
        }
        return Err(IoError::Io(std::io::Error::other(format!(
            "atomic handle rename failed ({:08x})",
            result as u32
        ))));
    }
    Ok(())
}

pub(super) fn remove_exact_pair(
    native: &Path,
    expected_native: FileStamp,
    sidecar: &Path,
    expected_sidecar: FileStamp,
) -> IoResult<()> {
    remove_exact_pair_inner(native, expected_native, sidecar, expected_sidecar, false)
}

pub(super) fn remove_exact(path: &Path, expected: FileStamp) -> IoResult<()> {
    let file = open_delete_exclusive(path)?;
    if stamp(&file)? != expected {
        return Err(IoError::ChangedDuringRead);
    }
    set_disposition(&file, true)
}

fn open_delete_exclusive(path: &Path) -> IoResult<File> {
    const DELETE: u32 = 0x0001_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        // Exclusive sharing fences writers, renames, and replacement between
        // the handle stamp and handle-bound disposition.
        .share_mode(0)
        .open(path)
        .map_err(IoError::from)
}

fn set_disposition(file: &File, delete: bool) -> IoResult<()> {
    let disposition = DispositionInformation {
        delete_file: i32::from(delete),
    };
    // SAFETY: The exclusive live handle and exact C-layout input remain valid
    // for this synchronous call. FileDispositionInfo marks this handle's
    // object, not a later file installed at the same path.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            4, // FileDispositionInfo
            (&raw const disposition).cast(),
            size_of::<DispositionInformation>() as u32,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

fn remove_exact_pair_inner(
    native: &Path,
    expected_native: FileStamp,
    sidecar: &Path,
    expected_sidecar: FileStamp,
    fail_second_mark: bool,
) -> IoResult<()> {
    let native_file = open_delete_exclusive(native)?;
    let sidecar_file = open_delete_exclusive(sidecar)?;
    if stamp(&native_file)? != expected_native || stamp(&sidecar_file)? != expected_sidecar {
        return Err(IoError::ChangedDuringRead);
    }
    // Native is marked first. If marking the sidecar fails, cancel the first
    // disposition while both exclusive handles remain live, then revalidate
    // both objects before returning the original failure. Thus a failed pair
    // discard never reports failure after deleting only its sidecar.
    set_disposition(&native_file, true)?;
    let second = if fail_second_mark {
        Err(IoError::InvalidInput(
            "injected second exact-pair disposition failure",
        ))
    } else {
        set_disposition(&sidecar_file, true)
    };
    if let Err(error) = second {
        set_disposition(&native_file, false)?;
        if stamp(&native_file)? != expected_native || stamp(&sidecar_file)? != expected_sidecar {
            return Err(IoError::ChangedDuringRead);
        }
        return Err(error);
    }
    Ok(())
}

fn wide_path(path: &Path) -> IoResult<Vec<u16>> {
    let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) || value.len() > 32_767 {
        return Err(IoError::InvalidInput(
            "file path contains NUL or is too long",
        ));
    }
    value.push(0);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{remove_exact, remove_exact_pair, remove_exact_pair_inner, stamp};
    use crate::IoError;
    use std::fs::File;
    use std::os::windows::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn no_replace_handle_rename_and_child_creation_preserve_existing_objects() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!(
            "inkpod-handle-collision-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let parent = super::open_authority_directory(&root, true).unwrap();
        let source = root.join("temporary.tmp");
        let destination = root.join("destination.inkpod");
        let mut writer = super::create_authority_child(&parent, &source, false).unwrap();
        writer.write_all(b"completed temporary").unwrap();
        writer.sync_all().unwrap();
        drop(writer);
        assert!(
            matches!(super::create_authority_child(&parent,&source,false),Err(IoError::Io(error)) if error.kind()==std::io::ErrorKind::AlreadyExists)
        );
        std::fs::write(&destination, b"existing output").unwrap();
        let temporary = super::open_authority_temporary(&parent, &source).unwrap();
        let identity = super::object_identity(&temporary).unwrap();
        assert!(super::rename_with_authority(&temporary, &parent, &destination, false).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"completed temporary");
        assert_eq!(std::fs::read(&destination).unwrap(), b"existing output");
        assert_eq!(super::object_identity(&temporary).unwrap(), identity);
        drop(temporary);
        let redirected = root.join("nonexistent-parent").join("temporary.tmp");
        let relative = super::open_authority_temporary(&parent, &redirected).unwrap();
        assert_eq!(super::object_identity(&relative).unwrap(), identity);
        drop(relative);
        super::remove_authority_temporary(&parent, &redirected, identity).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"existing output");
        drop(parent);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn junction(path: &std::path::Path, target: &std::path::Path) {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn DeviceIoControl(
                handle: *mut std::ffi::c_void,
                code: u32,
                input: *const std::ffi::c_void,
                input_bytes: u32,
                output: *mut std::ffi::c_void,
                output_bytes: u32,
                returned: *mut u32,
                overlapped: *mut std::ffi::c_void,
            ) -> i32;
        }
        std::fs::create_dir(path).unwrap();
        let target = std::fs::canonicalize(target).unwrap();
        let printable = target.as_os_str().to_string_lossy();
        let printable = printable.strip_prefix(r"\\?\").unwrap_or(&printable);
        let substitute: Vec<u16> = std::ffi::OsStr::new(&format!(r"\??\{printable}"))
            .encode_wide()
            .collect();
        let print: Vec<u16> = std::ffi::OsStr::new(printable).encode_wide().collect();
        let data_bytes = 8 + (substitute.len() + 1 + print.len() + 1) * 2;
        let mut buffer = vec![0u8; data_bytes + 8];
        buffer[..4].copy_from_slice(&0xa0000003u32.to_le_bytes());
        buffer[4..6].copy_from_slice(&(data_bytes as u16).to_le_bytes());
        buffer[10..12].copy_from_slice(&((substitute.len() * 2) as u16).to_le_bytes());
        buffer[12..14].copy_from_slice(&(((substitute.len() + 1) * 2) as u16).to_le_bytes());
        buffer[14..16].copy_from_slice(&((print.len() * 2) as u16).to_le_bytes());
        for (index, unit) in substitute
            .iter()
            .copied()
            .chain([0])
            .chain(print.iter().copied())
            .chain([0])
            .enumerate()
        {
            buffer[16 + index * 2..18 + index * 2].copy_from_slice(&unit.to_le_bytes());
        }
        let handle = std::fs::OpenOptions::new()
            .access_mode(0x40000000)
            .share_mode(7)
            .custom_flags(0x02200000)
            .open(path)
            .unwrap();
        let mut returned = 0;
        // SAFETY: the bounded buffer uses the mount-point reparse wire layout;
        // its bytes, output count, and directory handle live for this call.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    handle.as_raw_handle(),
                    0x900a4,
                    buffer.as_ptr().cast(),
                    buffer.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    &raw mut returned,
                    std::ptr::null_mut(),
                )
            },
            0,
            "{}",
            std::io::Error::last_os_error()
        );
    }

    #[test]
    fn approved_junction_retarget_is_rejected_and_reads_stay_in_pinned_directory() {
        use crate::{IoConfig, IoManager, JobContext};
        let root = std::env::temp_dir().join(format!(
            "inkpod-junction-authority-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let a = root.join("a");
        let b = root.join("b");
        let alias = root.join("alias");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::fs::write(a.join("approved.txt"), b"approved").unwrap();
        std::fs::write(b.join("unapproved.txt"), b"unapproved").unwrap();
        junction(&alias, &a);
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let context = JobContext::new();
        let authority = manager.observe_path_authority(&alias, &context).unwrap();
        let mut seen = Vec::new();
        let result = manager.with_directory_authority(&alias, &authority, &context, || {
            std::fs::remove_dir(&alias).unwrap();
            junction(&alias, &b);
            seen = manager
                .list_directory(&authority.path, &context)?
                .regular_files;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].file_name().unwrap(), "approved.txt");
        let mut ran = false;
        assert!(
            manager
                .with_directory_authority(&alias, &authority, &context, || {
                    ran = true;
                    Ok(())
                })
                .is_err()
        );
        assert!(!ran);
        std::fs::remove_dir(&alias).unwrap();
        junction(&alias, &a);
        let destination = manager
            .observe_path_authority(&a.join("result.inkpod"), &context)
            .unwrap();
        let mut polls = 0;
        let bytes = vec![3; 131_072];
        let result = manager.publish_guarded_with_ancestor(
            &destination,
            None,
            &bytes,
            &context,
            &mut || {
                polls += 1;
                if polls == 3 {
                    std::fs::remove_dir(&alias).unwrap();
                    junction(&alias, &b);
                }
                false
            },
            Some((&alias, &authority)),
        );
        assert!(matches!(result, Err(IoError::ConfirmationRequired)));
        assert_eq!(polls, 3);
        assert!(!a.join("result.inkpod").exists());
        assert!(!b.join("result.inkpod").exists());
        assert_eq!(std::fs::read_dir(&a).unwrap().count(), 1);
        std::fs::remove_dir(&alias).unwrap();
        junction(&alias, &a);
        assert_eq!(
            manager
                .publish_guarded_with_ancestor(
                    &destination,
                    None,
                    &bytes,
                    &context,
                    &mut || false,
                    Some((&alias, &authority))
                )
                .unwrap(),
            crate::GuardedPublishOutcome::Installed
        );
        assert_eq!(std::fs::read(a.join("result.inkpod")).unwrap(), bytes);
        std::fs::remove_dir(&alias).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn guarded_publication_excludes_external_writers() {
        use crate::{IoConfig, IoManager, JobContext, PublishSource};
        let manager = IoManager::new(IoConfig::default()).unwrap();
        let context = JobContext::new();
        let root = std::env::temp_dir().join(format!(
            "inkpod-guarded-writer-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("source.inkpod");
        std::fs::write(&path, b"original").unwrap();
        let authority = manager.observe_path_authority(&path, &context).unwrap();
        let loaded = manager.read_bytes(&path, 100, &context).unwrap();
        let proof = PublishSource {
            path: path.clone(),
            stamp: loaded.stamp(),
            digest: *blake3::hash(loaded.bytes()).as_bytes(),
        };
        let writer = std::fs::OpenOptions::new()
            .write(true)
            .share_mode(7)
            .open(&path)
            .unwrap();
        assert!(
            manager
                .publish_guarded(&authority, Some(proof), b"lost", &context, &mut || false)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        drop(writer);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_pair_delete_reopens_and_rejects_a_replaced_member() {
        let directory = std::env::temp_dir().join(format!(
            "inkpod-exact-delete-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let native = directory.join("attempt.inkpod");
        let sidecar = directory.join("attempt.inkpod.metadata");
        std::fs::write(&native, b"native").unwrap();
        std::fs::write(&sidecar, b"metadata").unwrap();
        let expected_native = stamp(&File::open(&native).unwrap()).unwrap();
        let expected_sidecar = stamp(&File::open(&sidecar).unwrap()).unwrap();

        std::fs::write(&native, b"externally changed native").unwrap();
        assert!(matches!(
            remove_exact_pair(&native, expected_native, &sidecar, expected_sidecar),
            Err(IoError::ChangedDuringRead)
        ));
        assert!(native.exists() && sidecar.exists());

        let current_native = stamp(&File::open(&native).unwrap()).unwrap();
        let current_sidecar = stamp(&File::open(&sidecar).unwrap()).unwrap();
        remove_exact_pair(&native, current_native, &sidecar, current_sidecar).unwrap();
        assert!(!native.exists() && !sidecar.exists());
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn exact_pair_delete_cancels_the_first_disposition_when_the_second_fails() {
        let directory = std::env::temp_dir().join(format!(
            "inkpod-exact-delete-rollback-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let native = directory.join("attempt.inkpod");
        let sidecar = directory.join("attempt.inkpod.metadata");
        std::fs::write(&native, b"native").unwrap();
        std::fs::write(&sidecar, b"metadata").unwrap();
        let expected_native = stamp(&File::open(&native).unwrap()).unwrap();
        let expected_sidecar = stamp(&File::open(&sidecar).unwrap()).unwrap();

        assert!(matches!(
            remove_exact_pair_inner(&native, expected_native, &sidecar, expected_sidecar, true,),
            Err(IoError::InvalidInput(_))
        ));
        assert_eq!(std::fs::read(&native).unwrap(), b"native");
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"metadata");

        std::fs::remove_file(native).unwrap();
        std::fs::remove_file(sidecar).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn exact_delete_rejects_external_sharing_and_keeps_the_expected_file() {
        let directory = std::env::temp_dir().join(format!(
            "inkpod-exact-delete-sharing-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("member.bin");
        std::fs::write(&path, b"member").unwrap();
        let expected = stamp(&File::open(&path).unwrap()).unwrap();
        let exclusive_reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();

        assert!(remove_exact(&path, expected).is_err());
        drop(exclusive_reader);
        assert_eq!(std::fs::read(&path).unwrap(), b"member");
        remove_exact(&path, expected).unwrap();
        assert!(!path.exists());
        std::fs::remove_dir(directory).unwrap();
    }
}
