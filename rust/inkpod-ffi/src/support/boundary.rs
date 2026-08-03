use super::*;

pub(crate) fn snapshot_handle(snapshot: RenderSnapshot) -> Box<InkpodSnapshot> {
    let tiles: Box<[InkpodSnapshotTile]> = snapshot
        .tiles()
        .iter()
        .map(|tile| InkpodSnapshotTile {
            struct_size: size_of::<InkpodSnapshotTile>() as u32,
            pixel_format: INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8,
            tile_id: tile.tile_id(),
            origin_x: tile.origin_x(),
            origin_y: tile.origin_y(),
            width: tile.width(),
            height: tile.height(),
            stride_bytes: tile.stride_bytes(),
            reserved: 0,
            pixels: tile.pixels().as_ptr(),
            pixel_bytes: tile.pixels().len() as u64,
            tile_revision: tile.tile_revision(),
        })
        .collect();
    let guides = snapshot
        .guides()
        .iter()
        .map(|guide| InkpodSnapshotGuide {
            struct_size: size_of::<InkpodSnapshotGuide>() as u32,
            axis: match guide.axis {
                GuideAxis::Horizontal => INKPOD_GUIDE_HORIZONTAL,
                GuideAxis::Vertical => INKPOD_GUIDE_VERTICAL,
            },
            position: guide.position,
            reserved: 0,
            id: guide.id,
        })
        .collect();
    let vector_segments = snapshot
        .vector_segments()
        .iter()
        .map(|segment| InkpodSnapshotVectorSegment {
            struct_size: size_of::<InkpodSnapshotVectorSegment>() as u32,
            flags: (if segment.closed {
                INKPOD_SNAPSHOT_VECTOR_CLOSED
            } else {
                0
            }) | if segment.stroke_visible {
                INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE
            } else {
                0
            },
            path_id: segment.path_id,
            plane_id: segment.plane_id,
            z_order: segment.z_order,
            segment_index: segment.segment_index,
            segment_count: segment.segment_count,
            color_rgba: pack_rgba(segment.color_rgba),
            p0: vector_point(segment.cubic.p0),
            p1: vector_point(segment.cubic.p1),
            p2: vector_point(segment.cubic.p2),
            p3: vector_point(segment.cubic.p3),
            width_start: segment.cubic.width_start,
            width_end: segment.cubic.width_end,
        })
        .collect();
    let vector_boundary_path_ids: Box<[u64]> = snapshot
        .vector_fills()
        .iter()
        .flat_map(|fill| fill.boundary_path_ids.iter().copied())
        .collect();
    let mut first_boundary_path = 0_u64;
    let vector_fills = snapshot
        .vector_fills()
        .iter()
        .map(|fill| {
            let output = InkpodSnapshotVectorFill {
                struct_size: size_of::<InkpodSnapshotVectorFill>() as u32,
                reserved: 0,
                fill_id: fill.fill_id,
                plane_id: fill.plane_id,
                z_order: fill.z_order,
                color_rgba: pack_rgba(fill.color_rgba),
                first_boundary_path,
                boundary_path_count: fill.boundary_path_ids.len() as u64,
            };
            first_boundary_path += fill.boundary_path_ids.len() as u64;
            output
        })
        .collect();
    Box::new(InkpodSnapshot {
        snapshot,
        tiles,
        guides,
        vector_segments,
        vector_fills,
        vector_boundary_path_ids,
    })
}

// SAFETY: Every raw pointer in `tiles` borrows an immutable pixel allocation
// owned by `snapshot`. Both fields remain immovable inside the Box returned over
// the ABI, and callers externally synchronize view/release as documented.
unsafe impl Send for InkpodSnapshot {}
// SAFETY: The same immutable ownership invariant permits concurrent reads; no
// function mutates a published snapshot.
unsafe impl Sync for InkpodSnapshot {}

pub(crate) struct ErrorSlot {
    pub(crate) bytes: [u8; ERROR_CAPACITY],
    pub(crate) len: usize,
}

impl ErrorSlot {
    const fn new() -> Self {
        Self {
            bytes: [0; ERROR_CAPACITY],
            len: 0,
        }
    }

    fn set(&mut self, message: &str) {
        let mut length = message.len().min(ERROR_CAPACITY - 1);
        while !message.is_char_boundary(length) {
            length -= 1;
        }
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
    pub(crate) static LAST_ERROR: RefCell<ErrorSlot> = const { RefCell::new(ErrorSlot::new()) };
}

pub(crate) fn clear_last_error() {
    LAST_ERROR.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            slot.clear();
        }
    });
}

pub(crate) fn fail(status: u32, message: &str) -> u32 {
    LAST_ERROR.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            slot.set(message);
        }
    });
    status
}

pub(crate) fn ffi_boundary(operation: impl FnOnce() -> u32) -> u32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(status) => status,
        Err(_) => fail(
            INKPOD_STATUS_PANIC,
            "a panic was contained at the inkpod C ABI boundary",
        ),
    }
}

pub(crate) fn is_aligned<T>(pointer: *const T) -> bool {
    (pointer as usize) % align_of::<T>() == 0
}

// SAFETY: `pointer` must expose a readable u32 size prefix. When that prefix
// advertises `size_of::<T>()` or more, the caller must provide that full range.
pub(crate) unsafe fn validate_struct<T>(pointer: *const T, type_name: &str) -> Result<u32, u32> {
    if pointer.is_null() || !is_aligned(pointer) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            &format!("{type_name} is null or misaligned"),
        ));
    }

    // SAFETY: Exported-function contracts require every public structure
    // pointer to expose at least its readable u32 size prefix. Reading only the
    // prefix avoids creating a full T reference before its size is validated.
    let struct_size = unsafe { pointer.cast::<u32>().read() };
    if struct_size < size_of::<T>() as u32 {
        return Err(fail(
            INKPOD_STATUS_INCOMPATIBLE_ABI,
            &format!("{type_name}.struct_size is too small"),
        ));
    }
    Ok(struct_size)
}

pub(crate) fn assert_snapshot_thread_contract() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<InkpodSnapshot>();
}

pub(crate) fn validate_core_thread(core: &InkpodCore) -> u32 {
    if core.owner_thread == thread::current().id() {
        INKPOD_STATUS_OK
    } else {
        fail(
            INKPOD_STATUS_WRONG_THREAD,
            "InkpodCore must be used and destroyed on its creating thread",
        )
    }
}

pub(crate) fn map_core_error(error: CoreError) -> u32 {
    let status = match error {
        CoreError::NoDocument => INKPOD_STATUS_NO_DOCUMENT,
        CoreError::InvalidArgument(_) | CoreError::Raster(_) | CoreError::Fill(_) => {
            INKPOD_STATUS_INVALID_ARGUMENT
        }
        CoreError::FillOverflow { .. } => INKPOD_STATUS_FILL_OVERFLOW,
        CoreError::Cancelled => INKPOD_STATUS_CANCELLED,
        CoreError::UnsavedChanges => INKPOD_STATUS_UNSAVED_CHANGES,
        CoreError::InvalidState(_) => INKPOD_STATUS_INVALID_STATE,
        CoreError::Format(_) => INKPOD_STATUS_IO_ERROR,
    };
    fail(status, &error.to_string())
}

pub(crate) fn frame_rect(rect: inkpod_core::RectI32) -> InkpodFrameRect {
    InkpodFrameRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

pub(crate) const fn pack_rgba(color: [u8; 4]) -> u32 {
    ((color[0] as u32) << 24)
        | ((color[1] as u32) << 16)
        | ((color[2] as u32) << 8)
        | color[3] as u32
}

pub(crate) const fn vector_point(point: PointF32) -> InkpodVectorPoint {
    InkpodVectorPoint {
        x: point.x,
        y: point.y,
    }
}

pub(crate) fn write_document_info(output: &mut InkpodDocumentInfo, info: DocumentInfo) {
    output.flags = (if info.dirty {
        INKPOD_DOCUMENT_FLAG_DIRTY
    } else {
        0
    }) | (if info.can_undo {
        INKPOD_DOCUMENT_FLAG_CAN_UNDO
    } else {
        0
    }) | (if info.can_redo {
        INKPOD_DOCUMENT_FLAG_CAN_REDO
    } else {
        0
    }) | (if info.recovered {
        INKPOD_DOCUMENT_FLAG_RECOVERED
    } else {
        0
    });
    output.document_revision = info.document_revision;
    output.view_revision = info.view_revision;
    output.document_id = info.document_id;
    output.document_uuid_high = (info.document_uuid >> 64) as u64;
    output.document_uuid_low = info.document_uuid as u64;
    output.layer_id = info.layer_id;
    output.main_plane_id = info.main_plane_id;
    output.color_plane_id = info.color_plane_id;
    output.width = info.width;
    output.height = info.height;
    output.dpi_x_milli = info.dpi_x_milli;
    output.dpi_y_milli = info.dpi_y_milli;
    output.hundred_frame = frame_rect(info.frames.hundred_frame);
    output.reference_frame = frame_rect(info.frames.reference_frame);
    output.drawing_frame = frame_rect(info.frames.drawing_frame);
    output.safe_frame = frame_rect(info.frames.safe_frame);
    output.margin_left = info.frames.margins.left;
    output.margin_top = info.frames.margins.top;
    output.margin_right = info.frames.margins.right;
    output.margin_bottom = info.frames.margins.bottom;
    output.active_plane = match info.active_plane {
        ActivePlane::MainLine => INKPOD_PLANE_MAIN_LINE,
        ActivePlane::Color => INKPOD_PLANE_COLOR,
    };
    output.reserved = 0;
    output.main_plane_checksum = info.main_plane_checksum;
    output.color_plane_checksum = info.color_plane_checksum;
}

pub(crate) fn write_resource_usage(output: &mut InkpodResourceUsage, usage: ResourceUsage) {
    output.reserved = 0;
    output.feature_flags = INKPOD_FEATURE_NONE;
    output.document_tile_bytes = usage.document_tile_bytes;
    output.document_tile_count = usage.document_tile_count;
    output.history_bytes = usage.history_bytes;
    output.history_entry_count = usage.history_entry_count;
    output.render_cache_bytes = usage.render_cache_bytes;
    output.render_cache_tile_count = usage.render_cache_tile_count;
    output.cpu_staging_bytes = usage.cpu_staging_bytes;
    output.reference_light_table_bytes = usage.reference_light_table_bytes;
    output.reference_light_table_tile_count = usage.reference_light_table_tile_count;
    output.sequence_source_bytes = usage.sequence_source_bytes;
    output.sequence_source_tile_count = usage.sequence_source_tile_count;
    output.thumbnail_cache_bytes = usage.thumbnail_cache_bytes;
}

pub(crate) fn write_dispatch_result(
    result: &mut InkpodDispatchResult,
    outcome: inkpod_core::DispatchOutcome,
) {
    result.reserved = 0;
    result.revision = outcome.revision();
    result.accepted_command_count = outcome.accepted_commands();
}
