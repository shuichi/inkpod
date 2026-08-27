use super::{FileIdentity, FileStamp};
use crate::{IoError, IoResult};
use std::ffi::c_void;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
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

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandle(handle: *mut c_void, information: *mut HandleInformation) -> i32;
    fn GetFileInformationByHandleEx(
        handle: *mut c_void,
        class: u32,
        information: *mut c_void,
        bytes: u32,
    ) -> i32;
    fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
}

pub(super) fn stamp(file: &File) -> IoResult<FileStamp> {
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
