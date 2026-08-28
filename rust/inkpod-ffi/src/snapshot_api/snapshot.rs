use super::*;

/// Copies the immutable, batched view descriptor for a live snapshot.
///
/// # Safety
/// `snapshot` must be live and externally synchronized; `out_view` must expose
/// its advertised writable byte range without overlapping the snapshot.
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
        // SAFETY: The output prefix is readable before the validated view is written.
        if let Err(status) = unsafe { validate_struct(out_view.cast_const(), "InkpodSnapshotView") }
        {
            return status;
        }
        // SAFETY: A live snapshot and complete, writable, non-overlapping view
        // are required by contract; the view size was checked above.
        let snapshot = unsafe { &*snapshot };
        let out_view = unsafe { &mut *out_view };

        out_view.abi_version = INKPOD_ABI_VERSION;
        out_view.feature_flags = snapshot.snapshot.feature_flags();
        out_view.revision = snapshot.snapshot.revision();
        out_view.tiles = if snapshot.tiles.is_empty() {
            ptr::null()
        } else {
            snapshot.tiles.as_ptr()
        };
        out_view.tile_count = snapshot.tiles.len() as u64;
        out_view.tile_stride_bytes = size_of::<InkpodSnapshotTile>() as u64;
        INKPOD_STATUS_OK
    })
}

/// Copies the immutable document-to-device transform carried by a snapshot.
///
/// # Safety
/// `snapshot` must be live and externally synchronized; `out_transform` must
/// expose its complete writable advertised range without overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_transform(
    snapshot: *const InkpodSnapshot,
    out_transform: *mut InkpodSnapshotTransform,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_transform.cast_const(), "InkpodSnapshotTransform") }
        {
            return status;
        }
        // SAFETY: Live snapshot and writable output are required by contract.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_transform };
        let view = snapshot.snapshot.view();
        output.flags = (if view.flip_horizontal() {
            INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL
        } else {
            0
        }) | if view.flip_vertical() {
            INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL
        } else {
            0
        };
        output.view_revision = view.revision();
        output.zoom = view.zoom();
        output.pan_x = view.pan_x();
        output.pan_y = view.pan_y();
        output.document_width = snapshot.snapshot.document_width();
        output.document_height = snapshot.snapshot.document_height();
        INKPOD_STATUS_OK
    })
}

/// Copies the runtime source namespace fixed when this snapshot was built.
///
/// This query does not scan pixels or infer provenance from the current Core.
/// Non-pristine snapshots return zero flags and zero identity fields.
///
/// # Safety
/// `snapshot` must remain live and may not be released concurrently. `output`
/// must be a complete, aligned, writable record not overlapping the snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_source_identity(
    snapshot: *const InkpodSnapshot,
    output: *mut InkpodSnapshotSourceIdentity,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        // SAFETY: Validate the readable size prefix before accessing the record.
        if let Err(status) =
            unsafe { validate_struct(output.cast_const(), "InkpodSnapshotSourceIdentity") }
        {
            return status;
        }
        // SAFETY: Both objects satisfy the live/non-overlapping caller contract;
        // the complete output size and alignment were validated above.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *output };
        let source = snapshot.snapshot.sequence_render_source();
        output.flags = source.map_or(0, |_| INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE);
        output.document_uuid_high = source.map_or(0, |source| (source.document_uuid >> 64) as u64);
        output.document_uuid_low = source.map_or(0, |source| source.document_uuid as u64);
        output.source_generation = source.map_or(0, |source| source.source_generation);
        output.owner_generation = source.map_or(0, |source| source.owner_generation);
        INKPOD_STATUS_OK
    })
}

/// Copies immutable ruler, guide, grid, snap, and transparent-view overlay data.
///
/// # Safety
/// `snapshot` must be live and externally synchronized; `out_overlay` must
/// expose its complete writable advertised range without overlap. The guide
/// span remains borrowed from `snapshot` until that snapshot is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_overlay(
    snapshot: *const InkpodSnapshot,
    out_overlay: *mut InkpodSnapshotOverlay,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if snapshot.is_null() || !is_aligned(snapshot) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "snapshot is null or misaligned",
            );
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) =
            unsafe { validate_struct(out_overlay.cast_const(), "InkpodSnapshotOverlay") }
        {
            return status;
        }
        // SAFETY: Live snapshot and writable output are required by contract.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_overlay };
        let view = snapshot.snapshot.view();
        let grid = snapshot.snapshot.grid();
        output.flags = (if view.ruler_visible() {
            INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE
        } else {
            0
        }) | (if view.guides_visible() {
            INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE
        } else {
            0
        }) | (if view.grid_visible() {
            INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE
        } else {
            0
        }) | (if view.snap_enabled() {
            INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED
        } else {
            0
        }) | (if view.transparent_view() {
            INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW
        } else {
            0
        }) | if view.alpha_view() {
            INKPOD_SNAPSHOT_OVERLAY_ALPHA_VIEW
        } else {
            0
        };
        output.grid_origin_x = grid.origin_x;
        output.grid_origin_y = grid.origin_y;
        output.grid_spacing_x = grid.spacing_x;
        output.grid_spacing_y = grid.spacing_y;
        output.grid_subdivisions = grid.subdivisions;
        output.reserved = 0;
        output.guides = if snapshot.guides.is_empty() {
            ptr::null()
        } else {
            snapshot.guides.as_ptr()
        };
        output.guide_count = snapshot.guides.len() as u64;
        output.guide_stride_bytes = size_of::<InkpodSnapshotGuide>() as u64;
        INKPOD_STATUS_OK
    })
}

/// Copies the immutable ordered render plan. All returned spans borrow storage
/// owned by `snapshot` and remain valid only until that snapshot is released.
///
/// # Safety
/// Snapshot/output must be complete, aligned, live, externally synchronized,
/// writable/non-overlapping objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_snapshot_get_render_plan(
    snapshot: *const InkpodSnapshot,
    out_plan: *mut InkpodSnapshotRenderPlan,
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
            unsafe { validate_struct(out_plan.cast_const(), "InkpodSnapshotRenderPlan") }
        {
            return status;
        }
        // SAFETY: Live snapshot and writable output are required by contract.
        let snapshot = unsafe { &*snapshot };
        let output = unsafe { &mut *out_plan };
        output.abi_version = INKPOD_ABI_VERSION;
        output.feature_flags = INKPOD_FEATURE_NONE;
        output.passes = if snapshot.render_passes.is_empty() {
            ptr::null()
        } else {
            snapshot.render_passes.as_ptr()
        };
        output.pass_count = snapshot.render_passes.len() as u64;
        output.pass_stride_bytes = size_of::<InkpodSnapshotRenderPass>() as u64;
        output.adjustment_luts_rgb8 = if snapshot.adjustment_luts_rgb8.is_empty() {
            ptr::null()
        } else {
            snapshot.adjustment_luts_rgb8.as_ptr()
        };
        output.adjustment_lut_count = (snapshot.adjustment_luts_rgb8.len() / (3 * 256)) as u64;
        output.adjustment_lut_stride_bytes = (3 * 256) as u64;
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
        assert_snapshot_thread_contract();
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
