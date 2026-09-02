use super::*;

/// Opaque shared service; release after all owner-thread jobs are finalized.
pub struct InkpodIoManager {
    pub(crate) manager: IoManager,
    pub(crate) validated_targets: ValidatedTargetCache,
}

/// Opaque request. Its mutex protects detached state, never a live Core.
pub struct InkpodIoJob {
    pub(crate) job: Mutex<FileIoJob>,
    pub(crate) owner_thread: ThreadId,
}

pub(crate) fn io_boundary(operation: impl FnOnce() -> Result<u32, u32>) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        match operation() {
            Ok(status) | Err(status) => status,
        }
    })
}

// SAFETY: Caller supplies a live, externally synchronized opaque handle.
pub(crate) unsafe fn manager_ref<'a>(
    pointer: *const InkpodIoManager,
) -> Result<&'a IoManager, u32> {
    // SAFETY: This preserves the same live-handle contract and narrows the borrow.
    Ok(&unsafe { manager_handle_ref(pointer)? }.manager)
}

// SAFETY: Caller supplies a live, externally synchronized opaque handle.
pub(crate) unsafe fn manager_handle_ref<'a>(
    pointer: *const InkpodIoManager,
) -> Result<&'a InkpodIoManager, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "I/O manager is null or misaligned",
        ));
    }
    // SAFETY: Checked address shape; validity/lifetime are the caller contract.
    Ok(unsafe { &*pointer })
}

// SAFETY: Caller supplies a live handle and does not race release.
pub(crate) unsafe fn job_lock<'a>(
    pointer: *const InkpodIoJob,
) -> Result<MutexGuard<'a, FileIoJob>, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "I/O job is null or misaligned",
        ));
    }
    // SAFETY: Validity is supplied by the ABI caller; mutation is mutex protected.
    unsafe { &*pointer }
        .job
        .lock()
        .map_err(|_| fail(INKPOD_STATUS_PANIC, "I/O job synchronization was poisoned"))
}

// SAFETY: Caller supplies a live Core exclusively on its creating thread.
pub(crate) unsafe fn owner_core<'a>(pointer: *mut InkpodCore) -> Result<&'a mut InkpodCore, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "Core is null or misaligned",
        ));
    }
    // SAFETY: Address shape checked; allocation lifetime/exclusivity are contractual.
    let core = unsafe { &mut *pointer };
    let status = validate_core_thread(core);
    if status != INKPOD_STATUS_OK {
        return Err(status);
    }
    Ok(core)
}

// SAFETY: Pointer exposes readable/writable owner storage for one handle pointer.
pub(crate) unsafe fn empty_owner<T>(pointer: *mut *mut T) -> Result<(), u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "output owner is null or misaligned",
        ));
    }
    // SAFETY: The caller promises readable owner storage.
    if !unsafe { pointer.read() }.is_null() {
        return Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "output already owns a handle",
        ));
    }
    Ok(())
}
