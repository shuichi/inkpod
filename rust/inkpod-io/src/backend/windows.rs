use super::{FileIdentity, FileStamp};
use crate::{IoError, IoResult};
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
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
