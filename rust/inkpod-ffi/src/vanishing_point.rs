use super::*;

unsafe fn parse_input(input: *const InkpodVanishingPointInput) -> Result<VanishingPointInput, u32> {
    // SAFETY: The caller contract requires one complete readable input record.
    unsafe { validate_struct(input, "InkpodVanishingPointInput") }?;
    // SAFETY: Complete aligned record was validated above.
    let input = unsafe { &*input };
    if input.feature_flags != INKPOD_FEATURE_NONE || input.reserved != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "vanishing-point input feature flags are unsupported",
        ));
    }
    if input.visible > 1 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "vanishing-point visibility is invalid",
        ));
    }
    // SAFETY: The nested fixed-size color record lies inside the validated input.
    let color = unsafe { parse_color_value(&raw const input.color) }?;
    Ok(VanishingPointInput {
        layer_id: input.layer_id,
        x_milli: input.x_milli,
        y_milli: input.y_milli,
        interval_milli_degrees: input.interval_milli_degrees,
        angle_milli_degrees: input.angle_milli_degrees,
        color,
        opacity_milli: input.opacity_milli,
        visible: input.visible != 0,
    })
}

pub(crate) fn vanishing_point_info_record(info: VanishingPointInfo) -> InkpodVanishingPointInfo {
    InkpodVanishingPointInfo {
        struct_size: size_of::<InkpodVanishingPointInfo>() as u32,
        visible: u32::from(info.visible),
        feature_flags: INKPOD_FEATURE_NONE,
        point_id: info.id,
        layer_id: info.layer_id,
        x_milli: info.x_milli,
        y_milli: info.y_milli,
        interval_milli_degrees: info.interval_milli_degrees,
        angle_milli_degrees: info.angle_milli_degrees,
        opacity_milli: info.opacity_milli,
        reserved: 0,
        color: color_value_record(info.color)
            .expect("validated vanishing-point color must be RGBA"),
    }
}

/// Copies persistent vanishing points into caller-owned strided records.
///
/// `out_count` always receives the required count. A null output is valid only
/// with zero capacity. No Core state or caller record is changed on failure.
///
/// # Safety
/// Core/count and any supplied output span must be complete, aligned, live
/// owner-thread storage. Records are written, never retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vanishing_points_copy(
    core: *mut InkpodCore,
    output: *mut InkpodVanishingPointInfo,
    capacity: u64,
    stride_bytes: u64,
    out_count: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_count.is_null() || !is_aligned(out_count) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vanishing-point query pointer is null or misaligned",
            );
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        let points = match core.core.vanishing_points() {
            Ok(points) => points,
            Err(error) => return map_core_error(error),
        };
        unsafe { out_count.write(points.len() as u64) };
        if capacity < points.len() as u64 {
            return fail(
                INKPOD_STATUS_BUFFER_TOO_SMALL,
                "vanishing-point output capacity is too small",
            );
        }
        if points.is_empty() {
            return INKPOD_STATUS_OK;
        }
        let stride = match usize::try_from(stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodVanishingPointInfo>()
                    && stride % align_of::<InkpodVanishingPointInfo>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "vanishing-point output stride is invalid",
                );
            }
        };
        if output.is_null() || !is_aligned(output) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vanishing-point output is null or misaligned",
            );
        }
        let Some(last_offset) = points
            .len()
            .checked_sub(1)
            .and_then(|index| index.checked_mul(stride))
        else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vanishing-point output span overflows",
            );
        };
        if last_offset
            .checked_add(size_of::<InkpodVanishingPointInfo>())
            .is_none()
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vanishing-point output span overflows",
            );
        }
        for (index, point) in points.into_iter().enumerate() {
            // SAFETY: The complete span and aligned stride were validated above.
            unsafe {
                output
                    .cast::<u8>()
                    .add(index * stride)
                    .cast::<InkpodVanishingPointInfo>()
                    .write(vanishing_point_info_record(point));
            }
        }
        INKPOD_STATUS_OK
    })
}

/// Applies one create/update/delete/delete-all canonical edit.
///
/// # Safety
/// Core/output scalars must be aligned live owner-thread storage. Input is
/// required only for create/update and is never retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vanishing_point_edit(
    core: *mut InkpodCore,
    expected_revision: u64,
    kind: u32,
    point_id: u64,
    input: *const InkpodVanishingPointInput,
    out_revision: *mut u64,
    out_point_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_revision.is_null()
            || !is_aligned(out_revision)
            || out_point_id.is_null()
            || !is_aligned(out_point_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vanishing-point edit pointer is null or misaligned",
            );
        }
        let edit = match (kind, point_id, input.is_null()) {
            (INKPOD_VANISHING_POINT_EDIT_CREATE, 0, false) => match unsafe { parse_input(input) } {
                Ok(input) => VanishingPointEdit::Create(input),
                Err(status) => return status,
            },
            (INKPOD_VANISHING_POINT_EDIT_UPDATE, id, false) if id != 0 => {
                match unsafe { parse_input(input) } {
                    Ok(input) => VanishingPointEdit::Update {
                        point_id: id,
                        input,
                    },
                    Err(status) => return status,
                }
            }
            (INKPOD_VANISHING_POINT_EDIT_DELETE, id, true) if id != 0 => {
                VanishingPointEdit::Delete { point_id: id }
            }
            (INKPOD_VANISHING_POINT_EDIT_DELETE_ALL, 0, true) => VanishingPointEdit::DeleteAll,
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "vanishing-point edit control fields are inconsistent",
                );
            }
        };
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.edit_vanishing_points(expected_revision, &[edit]) {
            Ok(outcome) => {
                unsafe {
                    out_revision.write(outcome.revision());
                    out_point_id.write(outcome.point_ids().first().copied().unwrap_or(0));
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Begins a create/update dialog or Canvas-handle preview.
///
/// # Safety
/// Core/input must be complete aligned live owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vanishing_point_preview_begin(
    core: *mut InkpodCore,
    expected_revision: u64,
    kind: u32,
    point_id: u64,
    input: *const InkpodVanishingPointInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        let input = match unsafe { parse_input(input) } {
            Ok(input) => input,
            Err(status) => return status,
        };
        let target = match (kind, point_id) {
            (INKPOD_VANISHING_POINT_EDIT_CREATE, 0) => VanishingPointPreviewTarget::Create,
            (INKPOD_VANISHING_POINT_EDIT_UPDATE, id) if id != 0 => {
                VanishingPointPreviewTarget::Update(id)
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "vanishing-point preview target is invalid",
                );
            }
        };
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core
            .core
            .begin_vanishing_point_preview(expected_revision, target, input)
        {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the active preview value from the immutable preview base.
///
/// # Safety
/// Core/input must be complete aligned live owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vanishing_point_preview_update(
    core: *mut InkpodCore,
    input: *const InkpodVanishingPointInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        let input = match unsafe { parse_input(input) } {
            Ok(input) => input,
            Err(status) => return status,
        };
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.update_vanishing_point_preview(input) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Cancels the active preview without changing persistent state.
///
/// # Safety
/// Core must be a live aligned owner-thread object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vanishing_point_preview_cancel(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.cancel_vanishing_point_preview() {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the active preview as one Undo unit.
///
/// # Safety
/// Core/output scalars must be aligned live owner-thread storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_vanishing_point_preview_apply(
    core: *mut InkpodCore,
    out_revision: *mut u64,
    out_point_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_revision.is_null()
            || !is_aligned(out_revision)
            || out_point_id.is_null()
            || !is_aligned(out_point_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "vanishing-point apply pointer is null or misaligned",
            );
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.apply_vanishing_point_preview() {
            Ok(outcome) => {
                unsafe {
                    out_revision.write(outcome.revision());
                    out_point_id.write(outcome.point_ids().first().copied().unwrap_or(0));
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Borrows vanishing-point and radial-guide snapshot spans until release.
///
/// # Safety
/// Snapshot/output must be complete aligned live objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_vanishing_points(
    snapshot: *const InkpodSnapshot,
    out_view: *mut InkpodSnapshotVanishingPointView,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_view.cast_const(), "InkpodSnapshotVanishingPointView") }
        {
            return status;
        }
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_view };
        output.abi_version = INKPOD_ABI_VERSION;
        output.feature_flags = INKPOD_FEATURE_NONE;
        output.points = if snapshot.vanishing_points.is_empty() {
            ptr::null()
        } else {
            snapshot.vanishing_points.as_ptr()
        };
        output.point_count = snapshot.vanishing_points.len() as u64;
        output.point_stride_bytes = size_of::<InkpodVanishingPointInfo>() as u64;
        output.radial_guides = if snapshot.radial_guides.is_empty() {
            ptr::null()
        } else {
            snapshot.radial_guides.as_ptr()
        };
        output.radial_guide_count = snapshot.radial_guides.len() as u64;
        output.radial_guide_stride_bytes = size_of::<InkpodSnapshotRadialGuide>() as u64;
        INKPOD_STATUS_OK
    })
}
