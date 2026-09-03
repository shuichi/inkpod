use super::*;

fn parse_anchor(value: u32) -> Result<ShootingFrameAnchor, u32> {
    match value {
        INKPOD_SHOOTING_FRAME_ANCHOR_TOP_LEFT => Ok(ShootingFrameAnchor::TopLeft),
        INKPOD_SHOOTING_FRAME_ANCHOR_TOP_RIGHT => Ok(ShootingFrameAnchor::TopRight),
        INKPOD_SHOOTING_FRAME_ANCHOR_CENTER => Ok(ShootingFrameAnchor::Center),
        INKPOD_SHOOTING_FRAME_ANCHOR_BOTTOM_LEFT => Ok(ShootingFrameAnchor::BottomLeft),
        INKPOD_SHOOTING_FRAME_ANCHOR_BOTTOM_RIGHT => Ok(ShootingFrameAnchor::BottomRight),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shooting-frame anchor is unknown",
        )),
    }
}

const fn anchor_code(value: ShootingFrameAnchor) -> u32 {
    match value {
        ShootingFrameAnchor::TopLeft => INKPOD_SHOOTING_FRAME_ANCHOR_TOP_LEFT,
        ShootingFrameAnchor::TopRight => INKPOD_SHOOTING_FRAME_ANCHOR_TOP_RIGHT,
        ShootingFrameAnchor::Center => INKPOD_SHOOTING_FRAME_ANCHOR_CENTER,
        ShootingFrameAnchor::BottomLeft => INKPOD_SHOOTING_FRAME_ANCHOR_BOTTOM_LEFT,
        ShootingFrameAnchor::BottomRight => INKPOD_SHOOTING_FRAME_ANCHOR_BOTTOM_RIGHT,
    }
}

unsafe fn parse_input(input: *const InkpodShootingFrameInput) -> Result<ShootingFrameInput, u32> {
    // SAFETY: The caller contract requires a complete readable input record.
    unsafe { validate_struct(input, "InkpodShootingFrameInput") }?;
    // SAFETY: Complete aligned record was validated above.
    let input = unsafe { &*input };
    if input.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "shooting-frame input feature flags are unsupported",
        ));
    }
    if input.visible > 1 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shooting-frame Boolean field is invalid",
        ));
    }
    let values = [
        input.center_x,
        input.center_y,
        input.width,
        input.height,
        input.rotation_degrees,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shooting-frame numeric input must be finite",
        ));
    }
    let center_x_milli = fixed_milli(input.center_x)?;
    let center_y_milli = fixed_milli(input.center_y)?;
    let width_milli = positive_fixed_milli(input.width)?;
    let height_milli = positive_fixed_milli(input.height)?;
    let normalized = input.rotation_degrees.rem_euclid(360.0);
    let turns = (normalized * 4_294_967_296.0 / 360.0).round_ties_even();
    let rotation_turns = if turns >= 4_294_967_296.0 {
        0
    } else {
        turns as u32
    };
    Ok(ShootingFrameInput {
        center_x_milli,
        center_y_milli,
        width_milli,
        height_milli,
        rotation_turns,
        anchor: parse_anchor(input.anchor)?,
        visible: input.visible != 0,
    })
}

fn fixed_milli(value: f64) -> Result<i64, u32> {
    let scaled = (value * 1_000.0).round_ties_even();
    if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
        Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shooting-frame coordinate is outside fixed-point range",
        ))
    } else {
        Ok(scaled as i64)
    }
}

fn positive_fixed_milli(value: f64) -> Result<u64, u32> {
    let scaled = (value * 1_000.0).round_ties_even();
    if !scaled.is_finite() || scaled <= 0.0 || scaled > u64::MAX as f64 {
        Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "shooting-frame size is outside fixed-point range",
        ))
    } else {
        Ok(scaled as u64)
    }
}

pub(crate) fn shooting_frame_info_record(
    info: ShootingFrameInfo,
) -> Result<InkpodShootingFrameInfo, CoreError> {
    let corners = info.corners()?;
    Ok(InkpodShootingFrameInfo {
        struct_size: size_of::<InkpodShootingFrameInfo>() as u32,
        anchor: anchor_code(info.anchor),
        feature_flags: INKPOD_FEATURE_NONE,
        frame_id: info.id,
        center_x_milli: info.center_x_milli,
        center_y_milli: info.center_y_milli,
        width_milli: info.width_milli,
        height_milli: info.height_milli,
        rotation_turns: info.rotation_turns,
        visible: u32::from(info.visible),
        reserved: 0,
        corners: corners.map(|point| InkpodShootingFramePoint {
            x_milli: point.x_milli,
            y_milli: point.y_milli,
        }),
    })
}

/// Queries the persistent angled shooting frame without transferring ownership.
///
/// # Safety
/// Core and output records must be complete, aligned, live owner-thread storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shooting_frame_get(
    core: *mut InkpodCore,
    out_present: *mut u32,
    out_frame: *mut InkpodShootingFrameInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) || out_present.is_null() || !is_aligned(out_present)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shooting-frame query pointer is null or misaligned",
            );
        }
        if let Err(status) =
            unsafe { validate_struct(out_frame.cast_const(), "InkpodShootingFrameInfo") }
        {
            return status;
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.shooting_frame() {
            Ok(Some(info)) => match shooting_frame_info_record(info) {
                Ok(record) => {
                    unsafe {
                        out_present.write(1);
                        out_frame.write(record);
                    }
                    INKPOD_STATUS_OK
                }
                Err(error) => map_core_error(error),
            },
            Ok(None) => {
                unsafe {
                    out_present.write(0);
                    out_frame.write(InkpodShootingFrameInfo {
                        struct_size: size_of::<InkpodShootingFrameInfo>() as u32,
                        ..InkpodShootingFrameInfo::default()
                    });
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies one create/update/delete as one canonical Core transaction.
///
/// # Safety
/// Core and output scalars must be aligned live owner-thread storage. Input is
/// required for create/update and must be null for delete; it is never retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shooting_frame_edit(
    core: *mut InkpodCore,
    expected_revision: u64,
    kind: u32,
    frame_id: u64,
    input: *const InkpodShootingFrameInput,
    out_revision: *mut u64,
    out_frame_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_revision.is_null()
            || !is_aligned(out_revision)
            || out_frame_id.is_null()
            || !is_aligned(out_frame_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shooting-frame edit pointer is null or misaligned",
            );
        }
        let edit = match kind {
            INKPOD_SHOOTING_FRAME_EDIT_CREATE if frame_id == 0 => {
                match unsafe { parse_input(input) } {
                    Ok(input) => ShootingFrameEdit::Create(input),
                    Err(status) => return status,
                }
            }
            INKPOD_SHOOTING_FRAME_EDIT_UPDATE if frame_id != 0 => {
                match unsafe { parse_input(input) } {
                    Ok(input) => ShootingFrameEdit::Update { frame_id, input },
                    Err(status) => return status,
                }
            }
            INKPOD_SHOOTING_FRAME_EDIT_DELETE if frame_id != 0 && input.is_null() => {
                ShootingFrameEdit::Delete { frame_id }
            }
            INKPOD_SHOOTING_FRAME_EDIT_CREATE
            | INKPOD_SHOOTING_FRAME_EDIT_UPDATE
            | INKPOD_SHOOTING_FRAME_EDIT_DELETE => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shooting-frame edit control fields are inconsistent",
                );
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shooting-frame edit kind is unknown",
                );
            }
        };
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.edit_shooting_frame(expected_revision, edit) {
            Ok(outcome) => {
                unsafe {
                    out_revision.write(outcome.revision());
                    out_frame_id.write(outcome.frame_id().unwrap_or(0));
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Begins a create/update preview without committing document state.
///
/// # Safety
/// Core/input must be complete aligned live owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shooting_frame_preview_begin(
    core: *mut InkpodCore,
    expected_revision: u64,
    kind: u32,
    frame_id: u64,
    input: *const InkpodShootingFrameInput,
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
        let target = match (kind, frame_id) {
            (INKPOD_SHOOTING_FRAME_EDIT_CREATE, 0) => ShootingFramePreviewTarget::Create,
            (INKPOD_SHOOTING_FRAME_EDIT_UPDATE, id) if id != 0 => {
                ShootingFramePreviewTarget::Update(id)
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "shooting-frame preview target is invalid",
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
            .begin_shooting_frame_preview(expected_revision, target, input)
        {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Replaces the active shooting-frame preview value.
///
/// # Safety
/// Core/input must be complete aligned live owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shooting_frame_preview_update(
    core: *mut InkpodCore,
    input: *const InkpodShootingFrameInput,
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
        match core.core.update_shooting_frame_preview(input) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Cancels the active shooting-frame preview.
///
/// # Safety
/// Core must be a live aligned owner-thread object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shooting_frame_preview_cancel(core: *mut InkpodCore) -> u32 {
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
        match core.core.cancel_shooting_frame_preview() {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies the active shooting-frame preview as one history item.
///
/// # Safety
/// Core and scalar outputs must be aligned live owner-thread storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_shooting_frame_preview_apply(
    core: *mut InkpodCore,
    out_revision: *mut u64,
    out_frame_id: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null()
            || !is_aligned(core)
            || out_revision.is_null()
            || !is_aligned(out_revision)
            || out_frame_id.is_null()
            || !is_aligned(out_frame_id)
        {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "shooting-frame apply pointer is null or misaligned",
            );
        }
        let core = unsafe { &mut *core };
        let status = validate_core_thread(core);
        if status != INKPOD_STATUS_OK {
            return status;
        }
        match core.core.apply_shooting_frame_preview() {
            Ok(outcome) => {
                unsafe {
                    out_revision.write(outcome.revision());
                    out_frame_id.write(outcome.frame_id().unwrap_or(0));
                }
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Borrows the optional immutable shooting-frame snapshot span.
///
/// # Safety
/// Snapshot/output must be complete aligned objects; the borrowed span expires
/// when the snapshot is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_shooting_frames(
    snapshot: *const InkpodSnapshot,
    out_view: *mut InkpodSnapshotShootingFrameView,
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
            unsafe { validate_struct(out_view.cast_const(), "InkpodSnapshotShootingFrameView") }
        {
            return status;
        }
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_view };
        output.abi_version = INKPOD_ABI_VERSION;
        output.feature_flags = INKPOD_FEATURE_NONE;
        output.frames = if snapshot.shooting_frames.is_empty() {
            ptr::null()
        } else {
            snapshot.shooting_frames.as_ptr()
        };
        output.frame_count = snapshot.shooting_frames.len() as u64;
        output.frame_stride_bytes = size_of::<InkpodShootingFrameInfo>() as u64;
        INKPOD_STATUS_OK
    })
}
