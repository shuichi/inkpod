#![deny(unsafe_op_in_unsafe_fn)]

use inkpod_core::{Command, Core, RenderSnapshot};
use std::cell::RefCell;
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::slice;
use std::thread::{self, ThreadId};

pub const INKPOD_ABI_VERSION: u32 = 1;
pub const INKPOD_FEATURE_NONE: u64 = 0;

pub const INKPOD_STATUS_OK: u32 = 0;
pub const INKPOD_STATUS_INVALID_ARGUMENT: u32 = 1;
pub const INKPOD_STATUS_INCOMPATIBLE_ABI: u32 = 2;
pub const INKPOD_STATUS_BUFFER_TOO_SMALL: u32 = 3;
pub const INKPOD_STATUS_UNSUPPORTED: u32 = 4;
pub const INKPOD_STATUS_PANIC: u32 = 5;
pub const INKPOD_STATUS_WRONG_THREAD: u32 = 6;

pub const INKPOD_COMMAND_NO_OP: u32 = 0;
const MAX_COMMAND_COUNT: u64 = 65_536;
const ERROR_CAPACITY: usize = 512;

#[repr(C)]
pub struct InkpodCoreConfig {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
}

#[repr(C)]
pub struct InkpodCommand {
    pub struct_size: u32,
    pub kind: u32,
    pub flags: u64,
}

#[repr(C)]
pub struct InkpodCommandBatch {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub commands: *const InkpodCommand,
    pub command_count: u64,
}

#[repr(C)]
pub struct InkpodDispatchResult {
    pub struct_size: u32,
    pub reserved: u32,
    pub revision: u64,
    pub accepted_command_count: u64,
}

#[repr(C)]
pub struct InkpodSnapshotOptions {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
}

#[repr(C)]
pub struct InkpodSnapshotTile {
    pub struct_size: u32,
    pub pixel_format: u32,
    pub tile_id: u64,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub reserved: u32,
    pub pixels: *const u8,
    pub pixel_bytes: u64,
    pub tile_revision: u64,
}

#[repr(C)]
pub struct InkpodSnapshotView {
    pub struct_size: u32,
    pub abi_version: u32,
    pub feature_flags: u64,
    pub revision: u64,
    pub tiles: *const InkpodSnapshotTile,
    pub tile_count: u64,
}

pub struct InkpodCore {
    owner_thread: ThreadId,
    core: Core,
}

pub struct InkpodSnapshot {
    snapshot: RenderSnapshot,
}

struct ErrorSlot {
    bytes: [u8; ERROR_CAPACITY],
    len: usize,
}

impl ErrorSlot {
    const fn new() -> Self {
        Self {
            bytes: [0; ERROR_CAPACITY],
            len: 0,
        }
    }

    fn set(&mut self, message: &str) {
        let length = message.len().min(ERROR_CAPACITY - 1);
        self.bytes[..length].copy_from_slice(&message.as_bytes()[..length]);
        self.len = length;
        self.bytes[length] = 0;
    }

    fn clear(&mut self) {
        self.len = 0;
        self.bytes[0] = 0;
    }
}

thread_local! {
    static LAST_ERROR: RefCell<ErrorSlot> = const { RefCell::new(ErrorSlot::new()) };
}

fn clear_last_error() {
    LAST_ERROR.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            slot.clear();
        }
    });
}

fn fail(status: u32, message: &str) -> u32 {
    LAST_ERROR.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            slot.set(message);
        }
    });
    status
}

fn ffi_boundary(operation: impl FnOnce() -> u32) -> u32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(status) => status,
        Err(_) => fail(
            INKPOD_STATUS_PANIC,
            "a panic was contained at the inkpod C ABI boundary",
        ),
    }
}

fn is_aligned<T>(pointer: *const T) -> bool {
    (pointer as usize) % align_of::<T>() == 0
}

fn validate_core_thread(core: &InkpodCore) -> u32 {
    if core.owner_thread == thread::current().id() {
        INKPOD_STATUS_OK
    } else {
        fail(
            INKPOD_STATUS_WRONG_THREAD,
            "InkpodCore must be used and destroyed on its creating thread",
        )
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn inkpod_abi_version() -> u32 {
    INKPOD_ABI_VERSION
}

/// Creates a single-writer core handle.
///
/// # Safety
/// `config` must point to a readable `InkpodCoreConfig`, and `out_core` must
/// point to writable storage for one handle pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_create(
    config: *const InkpodCoreConfig,
    out_core: *mut *mut InkpodCore,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_core.is_null() || !is_aligned(out_core) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_core is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires writable storage at out_core.
        unsafe { out_core.write(ptr::null_mut()) };

        if config.is_null() || !is_aligned(config) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "config is null or misaligned",
            );
        }
        // SAFETY: The pointer was checked for null/alignment and is readable by contract.
        let config = unsafe { &*config };
        if config.struct_size < size_of::<InkpodCoreConfig>() as u32 {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodCoreConfig.struct_size is too small",
            );
        }
        if config.abi_version != INKPOD_ABI_VERSION {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodCoreConfig.abi_version is unsupported",
            );
        }
        if config.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "InkpodCoreConfig contains unsupported feature flags",
            );
        }

        let handle = Box::new(InkpodCore {
            owner_thread: thread::current().id(),
            core: Core::new(),
        });
        // SAFETY: out_core is writable by contract and now receives Box ownership.
        unsafe { out_core.write(Box::into_raw(handle)) };
        INKPOD_STATUS_OK
    })
}

/// Destroys a core and nulls the caller's pointer. Repeating the call with the
/// same pointer variable is a safe no-op.
///
/// # Safety
/// `core` must point to writable storage that contains either null or a handle
/// returned by `inkpod_core_create` and not already destroyed through an alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_destroy(core: *mut *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "core owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires readable/writable pointer storage.
        let handle = unsafe { core.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core handle is misaligned");
        }
        // SAFETY: The caller contract guarantees a live handle from core_create.
        let core_ref = unsafe { &*handle };
        let thread_status = validate_core_thread(core_ref);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // Null first so a repeated call using the same owner variable is harmless.
        // SAFETY: The outer pointer is writable by contract.
        unsafe { core.write(ptr::null_mut()) };
        // SAFETY: Ownership came from Box::into_raw and is consumed exactly once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Dispatches a validated command batch on the creating thread.
///
/// # Safety
/// All pointers must follow the sizes, lifetimes, and ownership rules declared
/// in `include/inkpod/core_ffi.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_dispatch_batch(
    core: *mut InkpodCore,
    batch: *const InkpodCommandBatch,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if batch.is_null() || !is_aligned(batch) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch is null or misaligned",
            );
        }
        if result.is_null() || !is_aligned(result) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "result is null or misaligned",
            );
        }

        // SAFETY: Valid live objects and output storage are required by contract.
        let core = unsafe { &mut *core };
        let batch = unsafe { &*batch };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if batch.struct_size < size_of::<InkpodCommandBatch>() as u32
            || result.struct_size < size_of::<InkpodDispatchResult>() as u32
        {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "batch or result structure is too small",
            );
        }
        if batch.reserved != 0 || batch.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "batch contains unsupported flags or reserved values",
            );
        }
        if batch.command_count > MAX_COMMAND_COUNT {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch command_count exceeds the bounded M0 limit",
            );
        }
        if batch.command_count != 0 && (batch.commands.is_null() || !is_aligned(batch.commands)) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "commands is null or misaligned for a non-empty batch",
            );
        }

        let command_count = match usize::try_from(batch.command_count) {
            Ok(count) => count,
            Err(_) => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch command_count cannot be represented on this platform",
                );
            }
        };
        let byte_count = match command_count.checked_mul(size_of::<InkpodCommand>()) {
            Some(bytes) if bytes <= isize::MAX as usize => bytes,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "batch command storage size overflows",
                );
            }
        };
        let _ = byte_count;

        let commands = if command_count == 0 {
            &[]
        } else {
            // SAFETY: Count is bounded, byte size is checked, and the caller promises
            // a readable array for the duration of this call.
            unsafe { slice::from_raw_parts(batch.commands, command_count) }
        };
        let mut domain_commands = Vec::with_capacity(command_count);
        for command in commands {
            if command.struct_size < size_of::<InkpodCommand>() as u32 {
                return fail(
                    INKPOD_STATUS_INCOMPATIBLE_ABI,
                    "InkpodCommand.struct_size is too small",
                );
            }
            if command.flags != 0 {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "InkpodCommand.flags contains unsupported bits",
                );
            }
            if command.kind != INKPOD_COMMAND_NO_OP {
                return fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "InkpodCommand.kind is not defined by M0",
                );
            }
            domain_commands.push(Command::NoOp);
        }

        let outcome = core.core.dispatch(&domain_commands);
        result.reserved = 0;
        result.revision = outcome.revision();
        result.accepted_command_count = outcome.accepted_commands();
        INKPOD_STATUS_OK
    })
}

/// Builds one immutable snapshot owned by Rust.
///
/// # Safety
/// `core` and `options` must be readable live objects and `out_snapshot` must
/// point to writable handle storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_build_snapshot(
    core: *mut InkpodCore,
    options: *const InkpodSnapshotOptions,
    out_snapshot: *mut *mut InkpodSnapshot,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if out_snapshot.is_null() || !is_aligned(out_snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_snapshot is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires writable output pointer storage.
        unsafe { out_snapshot.write(ptr::null_mut()) };
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        if options.is_null() || !is_aligned(options) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot options are null or misaligned",
            );
        }

        // SAFETY: Live/readable objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.struct_size < size_of::<InkpodSnapshotOptions>() as u32 {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodSnapshotOptions.struct_size is too small",
            );
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "snapshot options contain unsupported values",
            );
        }

        let snapshot = Box::new(InkpodSnapshot {
            snapshot: core.core.build_snapshot(),
        });
        // SAFETY: The output is writable and receives Box ownership.
        unsafe { out_snapshot.write(Box::into_raw(snapshot)) };
        INKPOD_STATUS_OK
    })
}

/// Copies the immutable, batched view descriptor for a live snapshot.
///
/// # Safety
/// `snapshot` must be live and readable; `out_view` must be writable and have
/// its `struct_size` initialized by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_view(
    snapshot: *const InkpodSnapshot,
    out_view: *mut InkpodSnapshotView,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        if out_view.is_null() || !is_aligned(out_view) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_view is null or misaligned",
            );
        }
        // SAFETY: Live snapshot and writable view are required by contract.
        let snapshot = unsafe { &*snapshot };
        let out_view = unsafe { &mut *out_view };
        if out_view.struct_size < size_of::<InkpodSnapshotView>() as u32 {
            return fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodSnapshotView.struct_size is too small",
            );
        }

        out_view.abi_version = INKPOD_ABI_VERSION;
        out_view.feature_flags = INKPOD_FEATURE_NONE;
        out_view.revision = snapshot.snapshot.revision();
        out_view.tiles = ptr::null();
        out_view.tile_count = snapshot.snapshot.tile_count() as u64;
        INKPOD_STATUS_OK
    })
}

/// Releases a snapshot and nulls the caller's pointer. Snapshots may be viewed
/// and released from a renderer thread after publication.
///
/// # Safety
/// `snapshot` must point to writable storage containing null or a handle
/// returned by `inkpod_core_build_snapshot` and not released through an alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_release(snapshot: *mut *mut InkpodSnapshot) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot owner pointer is null or misaligned",
            );
        }
        // SAFETY: The caller contract requires readable/writable pointer storage.
        let handle = unsafe { snapshot.read() };
        if handle.is_null() {
            return INKPOD_STATUS_OK;
        }
        if !is_aligned(handle) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot handle is misaligned",
            );
        }
        // SAFETY: The outer pointer is writable; nulling precedes the ownership drop.
        unsafe { snapshot.write(ptr::null_mut()) };
        // SAFETY: Ownership came from Box::into_raw and is consumed exactly once.
        drop(unsafe { Box::from_raw(handle) });
        INKPOD_STATUS_OK
    })
}

/// Returns the required UTF-8 error buffer size, including its trailing NUL.
///
/// # Safety
/// `out_required_bytes` must point to writable `u64` storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_error_message_size(out_required_bytes: *mut u64) -> u32 {
    ffi_boundary(|| {
        if out_required_bytes.is_null() || !is_aligned(out_required_bytes) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_required_bytes is null or misaligned",
            );
        }
        let required = LAST_ERROR.with(|slot| {
            slot.try_borrow()
                .map_or(1, |slot| u64::try_from(slot.len + 1).unwrap_or(1))
        });
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_required_bytes.write(required) };
        INKPOD_STATUS_OK
    })
}

/// Copies the current thread's UTF-8 error text and a trailing NUL.
///
/// # Safety
/// `buffer` must reference `buffer_capacity` writable bytes and `out_written_bytes`
/// must point to writable `u64` storage. The two regions must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_error_message_copy(
    buffer: *mut u8,
    buffer_capacity: u64,
    out_written_bytes: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        if out_written_bytes.is_null() || !is_aligned(out_written_bytes) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "out_written_bytes is null or misaligned",
            );
        }
        let capacity = match usize::try_from(buffer_capacity) {
            Ok(capacity) => capacity,
            Err(_) => return INKPOD_STATUS_BUFFER_TOO_SMALL,
        };
        let required = LAST_ERROR.with(|slot| slot.try_borrow().map_or(1, |slot| slot.len + 1));
        if capacity < required || buffer.is_null() {
            return INKPOD_STATUS_BUFFER_TOO_SMALL;
        }

        let copied = LAST_ERROR.with(|slot| {
            let Ok(slot) = slot.try_borrow() else {
                return 0;
            };
            // SAFETY: The caller supplies at least len + 1 writable bytes. The
            // thread-local source cannot overlap caller memory.
            unsafe {
                ptr::copy_nonoverlapping(slot.bytes.as_ptr(), buffer, slot.len + 1);
            }
            slot.len
        });
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_written_bytes.write(copied as u64) };
        INKPOD_STATUS_OK
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> InkpodCoreConfig {
        InkpodCoreConfig {
            struct_size: size_of::<InkpodCoreConfig>() as u32,
            abi_version: INKPOD_ABI_VERSION,
            feature_flags: INKPOD_FEATURE_NONE,
        }
    }

    #[test]
    fn abi_001_lifecycle_and_double_release_are_safe() {
        let mut core = ptr::null_mut();
        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        assert!(!core.is_null());

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: INKPOD_FEATURE_NONE,
        };
        let mut snapshot = ptr::null_mut();
        // SAFETY: The core is live and outputs point to local storage.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );

        let mut view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: u64::MAX,
            revision: u64::MAX,
            tiles: ptr::null(),
            tile_count: u64::MAX,
        };
        // SAFETY: Snapshot and output view are live for this call.
        assert_eq!(
            unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
            INKPOD_STATUS_OK
        );
        assert_eq!(view.abi_version, INKPOD_ABI_VERSION);
        assert_eq!(view.revision, 0);
        assert!(view.tiles.is_null());
        assert_eq!(view.tile_count, 0);

        // SAFETY: Owner variables contain live handles, then null after first calls.
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn abi_001_dispatch_validates_commands_before_applying() {
        let mut core = ptr::null_mut();
        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let command = InkpodCommand {
            struct_size: size_of::<InkpodCommand>() as u32,
            kind: INKPOD_COMMAND_NO_OP,
            flags: 0,
        };
        let batch = InkpodCommandBatch {
            struct_size: size_of::<InkpodCommandBatch>() as u32,
            reserved: 0,
            feature_flags: 0,
            commands: &command,
            command_count: 1,
        };
        let mut result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: u32::MAX,
            revision: u64::MAX,
            accepted_command_count: 0,
        };
        // SAFETY: The core and all batch/result storage are live for the call.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &batch, &mut result) },
            INKPOD_STATUS_OK
        );
        assert_eq!(result.revision, 0);
        assert_eq!(result.accepted_command_count, 1);

        let invalid_command = InkpodCommand {
            kind: 99,
            ..command
        };
        let invalid_batch = InkpodCommandBatch {
            commands: &invalid_command,
            ..batch
        };
        // SAFETY: Storage is valid; the enum value is intentionally unsupported.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &invalid_batch, &mut result) },
            INKPOD_STATUS_UNSUPPORTED
        );

        let mut required = 0;
        // SAFETY: required is writable local storage.
        assert_eq!(
            unsafe { inkpod_error_message_size(&mut required) },
            INKPOD_STATUS_OK
        );
        assert!(required > 1);
        let mut message = vec![0_u8; required as usize];
        let mut written = 0;
        // SAFETY: message has the queried capacity and written is writable.
        assert_eq!(
            unsafe {
                inkpod_error_message_copy(message.as_mut_ptr(), message.len() as u64, &mut written)
            },
            INKPOD_STATUS_OK
        );
        assert!(written > 0);
        assert_eq!(message[written as usize], 0);

        // SAFETY: The owner variable contains the live handle.
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn abi_001_rejects_null_and_short_structures() {
        let mut core = ptr::null_mut();
        // SAFETY: Null input is intentionally tested; output is writable.
        assert_eq!(
            unsafe { inkpod_core_create(ptr::null(), &mut core) },
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(core.is_null());

        let short = InkpodCoreConfig {
            struct_size: 1,
            ..config()
        };
        // SAFETY: short is readable and output is writable.
        assert_eq!(
            unsafe { inkpod_core_create(&short, &mut core) },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(core.is_null());
    }
}
