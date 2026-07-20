#![deny(unsafe_op_in_unsafe_fn)]

use inkpod_core::{
    ActivePlane, Command, CoordinateSpace, Core, CoreError, DocumentInfo, PaintTool,
    RenderSnapshot, Stroke, StrokeSample, ViewCommand,
};
use std::cell::RefCell;
use std::mem::{align_of, size_of};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
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
pub const INKPOD_STATUS_IO_ERROR: u32 = 7;
pub const INKPOD_STATUS_INVALID_STATE: u32 = 8;
pub const INKPOD_STATUS_NO_DOCUMENT: u32 = 9;

pub const INKPOD_COMMAND_NO_OP: u32 = 0;
pub const INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8: u32 = 1;
pub const INKPOD_PLANE_MAIN_LINE: u32 = 1;
pub const INKPOD_PLANE_COLOR: u32 = 2;
pub const INKPOD_TOOL_PENCIL: u32 = 1;
pub const INKPOD_TOOL_BRUSH: u32 = 2;
pub const INKPOD_TOOL_ERASER: u32 = 3;
pub const INKPOD_COORDINATE_SPACE_DOCUMENT: u32 = 1;
pub const INKPOD_COORDINATE_SPACE_DEVICE: u32 = 2;
pub const INKPOD_STROKE_FLAG_AUTO_ERASE: u64 = 1 << 0;
pub const INKPOD_STROKE_FLAG_PRESSURE_SIZE: u64 = 1 << 1;
pub const INKPOD_DOCUMENT_FLAG_DIRTY: u32 = 1 << 0;
pub const INKPOD_DOCUMENT_FLAG_CAN_UNDO: u32 = 1 << 1;
pub const INKPOD_DOCUMENT_FLAG_CAN_REDO: u32 = 1 << 2;
pub const INKPOD_VIEW_PAN_BY: u32 = 1;
pub const INKPOD_VIEW_ZOOM_AT: u32 = 2;
pub const INKPOD_VIEW_FIT: u32 = 3;
pub const INKPOD_VIEW_ONE_TO_ONE: u32 = 4;
pub const INKPOD_VIEW_VIEWPORT_RESIZED: u32 = 5;
const MAX_COMMAND_COUNT: u64 = 65_536;
const MAX_STROKE_SAMPLE_COUNT: u64 = 1_048_576;
const MAX_PATH_BYTES: u64 = 32_768;
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
    pub command_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodDispatchResult {
    pub struct_size: u32,
    pub reserved: u32,
    pub revision: u64,
    pub accepted_command_count: u64,
}

#[repr(C)]
pub struct InkpodCellCreateOptions {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InkpodFrameRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodDocumentInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub document_revision: u64,
    pub view_revision: u64,
    pub document_id: u64,
    pub document_uuid_high: u64,
    pub document_uuid_low: u64,
    pub layer_id: u64,
    pub main_plane_id: u64,
    pub color_plane_id: u64,
    pub width: u32,
    pub height: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub hundred_frame: InkpodFrameRect,
    pub reference_frame: InkpodFrameRect,
    pub drawing_frame: InkpodFrameRect,
    pub safe_frame: InkpodFrameRect,
    pub margin_left: u32,
    pub margin_top: u32,
    pub margin_right: u32,
    pub margin_bottom: u32,
    pub active_plane: u32,
    pub reserved: u32,
    pub main_plane_checksum: u64,
    pub color_plane_checksum: u64,
}

#[repr(C)]
pub struct InkpodStrokeSample {
    pub struct_size: u32,
    pub flags: u32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub reserved: u32,
}

#[repr(C)]
pub struct InkpodStrokeInput {
    pub struct_size: u32,
    pub tool: u32,
    pub plane: u32,
    pub coordinate_space: u32,
    pub flags: u64,
    pub color_rgba: u32,
    pub diameter: f32,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodStrokeSampleSpan {
    pub struct_size: u32,
    pub reserved: u32,
    pub feature_flags: u64,
    pub samples: *const InkpodStrokeSample,
    pub sample_count: u64,
    pub sample_stride_bytes: u64,
}

#[repr(C)]
pub struct InkpodViewInput {
    pub struct_size: u32,
    pub kind: u32,
    pub flags: u64,
    pub value1: f64,
    pub value2: f64,
    pub value3: f64,
    pub value4: f64,
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
    pub tile_stride_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
pub struct InkpodSnapshotTransform {
    pub struct_size: u32,
    pub reserved: u32,
    pub view_revision: u64,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub document_width: u32,
    pub document_height: u32,
}

pub struct InkpodCore {
    owner_thread: ThreadId,
    core: Core,
}

pub struct InkpodSnapshot {
    snapshot: RenderSnapshot,
    tiles: Box<[InkpodSnapshotTile]>,
}

// SAFETY: Every raw pointer in `tiles` borrows an immutable pixel allocation
// owned by `snapshot`. Both fields remain immovable inside the Box returned over
// the ABI, and callers externally synchronize view/release as documented.
unsafe impl Send for InkpodSnapshot {}
// SAFETY: The same immutable ownership invariant permits concurrent reads; no
// function mutates a published snapshot.
unsafe impl Sync for InkpodSnapshot {}

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

// SAFETY: `pointer` must expose a readable u32 size prefix. When that prefix
// advertises `size_of::<T>()` or more, the caller must provide that full range.
unsafe fn validate_struct<T>(pointer: *const T, type_name: &str) -> Result<u32, u32> {
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

fn assert_snapshot_thread_contract() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<InkpodSnapshot>();
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

fn map_core_error(error: CoreError) -> u32 {
    let status = match error {
        CoreError::NoDocument => INKPOD_STATUS_NO_DOCUMENT,
        CoreError::InvalidArgument(_) | CoreError::Raster(_) => INKPOD_STATUS_INVALID_ARGUMENT,
        CoreError::InvalidState(_) => INKPOD_STATUS_INVALID_STATE,
        CoreError::Format(_) => INKPOD_STATUS_IO_ERROR,
    };
    fail(status, &error.to_string())
}

fn frame_rect(rect: inkpod_core::RectI32) -> InkpodFrameRect {
    InkpodFrameRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn write_document_info(output: &mut InkpodDocumentInfo, info: DocumentInfo) {
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

fn write_dispatch_result(result: &mut InkpodDispatchResult, outcome: inkpod_core::DispatchOutcome) {
    result.reserved = 0;
    result.revision = outcome.revision();
    result.accepted_command_count = outcome.accepted_commands();
}

fn parse_plane(value: u32) -> Result<ActivePlane, u32> {
    match value {
        INKPOD_PLANE_MAIN_LINE => Ok(ActivePlane::MainLine),
        INKPOD_PLANE_COLOR => Ok(ActivePlane::Color),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "plane is not a defined M1 plane",
        )),
    }
}

fn parse_tool(value: u32) -> Result<PaintTool, u32> {
    match value {
        INKPOD_TOOL_PENCIL => Ok(PaintTool::Pencil),
        INKPOD_TOOL_BRUSH => Ok(PaintTool::Brush),
        INKPOD_TOOL_ERASER => Ok(PaintTool::Eraser),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "tool is not a defined M1 paint tool",
        )),
    }
}

fn parse_coordinate_space(value: u32) -> Result<CoordinateSpace, u32> {
    match value {
        INKPOD_COORDINATE_SPACE_DOCUMENT => Ok(CoordinateSpace::Document),
        INKPOD_COORDINATE_SPACE_DEVICE => Ok(CoordinateSpace::Device),
        _ => Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "coordinate_space is not defined",
        )),
    }
}

// SAFETY: The caller must provide a readable, aligned strided span for every
// advertised record. Each record must expose at least its size prefix.
unsafe fn parse_stroke_samples(
    samples: *const InkpodStrokeSample,
    sample_count: u64,
    sample_stride_bytes: u64,
) -> Result<Vec<StrokeSample>, u32> {
    if sample_count == 0 || sample_count > MAX_STROKE_SAMPLE_COUNT {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample_count is outside bounds",
        ));
    }
    if samples.is_null() || !is_aligned(samples) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke samples are null or misaligned",
        ));
    }
    let sample_count = usize::try_from(sample_count).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample_count is not representable",
        )
    })?;
    let stride = match usize::try_from(sample_stride_bytes) {
        Ok(stride)
            if stride >= size_of::<InkpodStrokeSample>()
                && stride % align_of::<InkpodStrokeSample>() == 0 =>
        {
            stride
        }
        _ => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "sample_stride_bytes is too small, misaligned, or not representable",
            ));
        }
    };
    let storage_bytes = sample_count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<InkpodStrokeSample>()));
    if storage_bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "stroke sample storage size overflows",
        ));
    }

    let mut parsed = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        // SAFETY: The checked count/stride span is readable by contract.
        let sample_pointer = unsafe {
            samples
                .cast::<u8>()
                .add(index * stride)
                .cast::<InkpodStrokeSample>()
        };
        // SAFETY: Each record exposes its readable size prefix.
        let sample_size = unsafe { validate_struct(sample_pointer, "InkpodStrokeSample") }?;
        if u64::from(sample_size) > sample_stride_bytes {
            return Err(fail(
                INKPOD_STATUS_INCOMPATIBLE_ABI,
                "InkpodStrokeSample.struct_size exceeds sample_stride_bytes",
            ));
        }
        // SAFETY: The complete known record prefix is aligned and readable.
        let sample = unsafe { &*sample_pointer };
        if sample.flags != 0 || sample.reserved != 0 {
            return Err(fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stroke sample contains unsupported flags or reserved values",
            ));
        }
        parsed.push(StrokeSample {
            x: sample.x,
            y: sample.y,
            pressure: sample.pressure,
        });
    }
    Ok(parsed)
}

// SAFETY: `input` must be a validated, complete public structure whose sample
// span satisfies the exported function contract.
unsafe fn parse_stroke_input(input: &InkpodStrokeInput) -> Result<Stroke, u32> {
    if input.flags & !(INKPOD_STROKE_FLAG_AUTO_ERASE | INKPOD_STROKE_FLAG_PRESSURE_SIZE) != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "stroke input contains unsupported flags",
        ));
    }
    // SAFETY: Forwarded from this helper's caller contract.
    let samples = unsafe {
        parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
    }?;
    let tool = parse_tool(input.tool)?;
    let plane = parse_plane(input.plane)?;
    let coordinate_space = parse_coordinate_space(input.coordinate_space)?;
    Ok(Stroke {
        tool,
        plane,
        color: [
            (input.color_rgba >> 24) as u8,
            (input.color_rgba >> 16) as u8,
            (input.color_rgba >> 8) as u8,
            input.color_rgba as u8,
        ],
        diameter: input.diameter,
        auto_erase: input.flags & INKPOD_STROKE_FLAG_AUTO_ERASE != 0,
        pressure_size: input.flags & INKPOD_STROKE_FLAG_PRESSURE_SIZE != 0,
        coordinate_space,
        samples,
    })
}

// SAFETY: `pointer` must identify `length` readable bytes for this call.
unsafe fn path_from_utf8<'a>(pointer: *const u8, length: u64) -> Result<&'a Path, u32> {
    if pointer.is_null() || length == 0 || length > MAX_PATH_BYTES {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "UTF-8 path is null, empty, or exceeds the bounded length",
        ));
    }
    let length = usize::try_from(length).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "UTF-8 path length is not representable",
        )
    })?;
    // SAFETY: The exported-function contract requires this readable range.
    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| fail(INKPOD_STATUS_INVALID_ARGUMENT, "path is not valid UTF-8"))?;
    if text.as_bytes().contains(&0) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "path contains an embedded NUL",
        ));
    }
    Ok(Path::new(text))
}

#[unsafe(no_mangle)]
pub extern "C" fn inkpod_abi_version() -> u32 {
    INKPOD_ABI_VERSION
}

/// Creates a single-writer core handle.
///
/// # Safety
/// `config` must expose a readable size prefix and the byte range it advertises.
/// `out_core` must point to writable storage for one non-overlapping handle
/// pointer.
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

        // SAFETY: The exported API requires a readable public-structure prefix.
        if let Err(status) = unsafe { validate_struct(config, "InkpodCoreConfig") } {
            return status;
        }
        // SAFETY: The size prefix was validated and the caller contract makes
        // the complete configuration readable for this call.
        let config = unsafe { &*config };
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
/// All pointers must follow the non-overlapping sizes, strides, lifetimes, and
/// ownership rules declared in `include/inkpod/core_ffi.h`.
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
        // SAFETY: The exported API requires readable/writable structure prefixes.
        if let Err(status) = unsafe { validate_struct(batch, "InkpodCommandBatch") } {
            return status;
        }
        // SAFETY: The result prefix is readable before the validated output is written.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }

        // SAFETY: Complete structures were size-checked, and valid live,
        // non-overlapping objects and output storage are required by contract.
        let core = unsafe { &mut *core };
        let batch = unsafe { &*batch };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
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
        if batch.commands.is_null() && batch.command_count != 0 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "commands is null for a non-empty batch",
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
        let command_stride = match usize::try_from(batch.command_stride_bytes) {
            Ok(stride)
                if stride >= size_of::<InkpodCommand>()
                    && stride % align_of::<InkpodCommand>() == 0 =>
            {
                stride
            }
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "command_stride_bytes is too small, misaligned, or not representable",
                );
            }
        };
        if !batch.commands.is_null() && !is_aligned(batch.commands) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "commands is misaligned");
        }
        let command_storage_bytes = command_count
            .saturating_sub(1)
            .checked_mul(command_stride)
            .and_then(|last_offset| last_offset.checked_add(size_of::<InkpodCommand>()));
        if command_storage_bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "batch command storage size overflows",
            );
        }

        let mut domain_commands = Vec::with_capacity(command_count);
        for index in 0..command_count {
            let offset = index * command_stride;
            // SAFETY: The checked count/stride range fits isize, and the caller
            // promises readable command storage for that complete range.
            let command_pointer = unsafe {
                batch
                    .commands
                    .cast::<u8>()
                    .add(offset)
                    .cast::<InkpodCommand>()
            };
            // SAFETY: The containing range and element alignment were checked,
            // and the caller promises readable records for the complete span.
            let command_struct_size =
                match unsafe { validate_struct(command_pointer, "InkpodCommand") } {
                    Ok(struct_size) => struct_size,
                    Err(status) => return status,
                };
            if u64::from(command_struct_size) > batch.command_stride_bytes {
                return fail(
                    INKPOD_STATUS_INCOMPATIBLE_ABI,
                    "InkpodCommand.struct_size exceeds command_stride_bytes",
                );
            }
            // SAFETY: The element size, alignment, and containing storage were
            // validated before constructing the known ABI prefix reference.
            let command = unsafe { &*command_pointer };
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

/// Creates a new M1 two-plane cell document.
///
/// # Safety
/// Pointers must reference live, non-overlapping objects with readable/writable
/// ranges described by their size prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_new_cell(
    core: *mut InkpodCore,
    options: *const InkpodCellCreateOptions,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(options, "InkpodCellCreateOptions") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects and writable output are required by contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "cell options contain unsupported flags or reserved values",
            );
        }
        let document_uuid =
            (u128::from(options.document_uuid_high) << 64) | u128::from(options.document_uuid_low);
        match core.core.new_cell_with_uuid(
            options.width,
            options.height,
            options.dpi_x_milli,
            options.dpi_y_milli,
            document_uuid,
        ) {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Copies current document metadata and checksums.
///
/// # Safety
/// `core` must be live on its owner thread and `out_info` must expose its
/// complete writable advertised range without overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_document_info(
    core: *mut InkpodCore,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Live core and writable output are required by contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.document_info() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Switches the editable plane without mutating document pixels or revision.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_set_active_plane(core: *mut InkpodCore, plane: u32) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A live core is required by the caller contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let plane = match parse_plane(plane) {
            Ok(plane) => plane,
            Err(status) => return status,
        };
        match core.core.set_active_plane(plane) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies all samples from pointer-down through pointer-up as one transaction.
///
/// # Safety
/// The input, sample span, output, and Core must be live, aligned,
/// non-overlapping ranges for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_stroke(
    core: *mut InkpodCore,
    input: *const InkpodStrokeInput,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structure pointers expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodStrokeInput") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects and writable output are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input and its borrowed sample span were validated above.
        let stroke = match unsafe { parse_stroke_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.apply_stroke(&stroke) {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Starts one Core-owned transient stroke transaction and stages the supplied
/// first sample batch without changing document revision, history, or dirty.
///
/// # Safety
/// Core/input/sample storage must satisfy the same contract as
/// `inkpod_core_apply_stroke` and remain live for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_begin(
    core: *mut InkpodCore,
    input: *const InkpodStrokeInput,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodStrokeInput") } {
            return status;
        }
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        // SAFETY: The complete input and borrowed span were validated above.
        let stroke = match unsafe { parse_stroke_input(input) } {
            Ok(stroke) => stroke,
            Err(status) => return status,
        };
        match core.core.begin_stroke(&stroke) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Appends one borrowed, strided sample batch to the active transient stroke.
/// Failure discards the Core-owned preview and commits no document state.
///
/// # Safety
/// Core/span/sample storage must be complete, live, aligned, non-overlapping
/// owner-thread objects for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_append(
    core: *mut InkpodCore,
    span: *const InkpodStrokeSampleSpan,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        let span_status = unsafe { validate_struct(span, "InkpodStrokeSampleSpan") };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if let Err(status) = span_status {
            core.core.cancel_stroke();
            return status;
        }
        let span = unsafe { &*span };
        if span.reserved != 0 || span.feature_flags != INKPOD_FEATURE_NONE {
            core.core.cancel_stroke();
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "stroke sample span contains unsupported values",
            );
        }
        // SAFETY: The borrowed span is validated by the exported contract.
        let samples = match unsafe {
            parse_stroke_samples(span.samples, span.sample_count, span.sample_stride_bytes)
        } {
            Ok(samples) => samples,
            Err(status) => {
                core.core.cancel_stroke();
                return status;
            }
        };
        match core.core.append_stroke(&samples) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(error) => map_core_error(error),
        }
    })
}

/// Commits the active transient stroke as one document/history transaction.
///
/// # Safety
/// Core/result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_end(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The result exposes a readable size prefix before writing.
        let result_status = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") };
        // SAFETY: Complete live objects are required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if let Err(status) = result_status {
            core.core.cancel_stroke();
            return status;
        }
        let result = unsafe { &mut *result };
        match core.core.end_stroke() {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Discards any active transient stroke. Calling with no active stroke is a
/// successful no-op.
///
/// # Safety
/// `core` must be a live owner-thread handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_stroke_cancel(core: *mut InkpodCore) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: A complete live Core is required by contract.
        let core = unsafe { &mut *core };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        core.core.cancel_stroke();
        INKPOD_STATUS_OK
    })
}

/// Undoes one committed transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_undo(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { history_operation(core, result, false) }
}

/// Redoes one committed transaction.
///
/// # Safety
/// Core and result must be live, aligned, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_redo(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { history_operation(core, result, true) }
}

unsafe fn history_operation(
    core: *mut InkpodCore,
    result: *mut InkpodDispatchResult,
    redo: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(result.cast_const(), "InkpodDispatchResult") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let result = unsafe { &mut *result };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if redo {
            core.core.redo()
        } else {
            core.core.undo()
        };
        match operation {
            Ok(outcome) => {
                write_dispatch_result(result, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Saves to a UTF-8 path using same-directory temporary-file replacement.
///
/// # Safety
/// Path bytes must remain readable, and all object/output pointers must remain
/// live and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_save(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { file_operation(core, path_utf8, path_bytes, out_info, false) }
}

/// Opens a versioned `.inkpod` file from a UTF-8 path.
///
/// # Safety
/// Path bytes must remain readable, and all object/output pointers must remain
/// live and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_open(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    // SAFETY: This exported function forwards the identical caller contract.
    unsafe { file_operation(core, path_utf8, path_bytes, out_info, true) }
}

unsafe fn file_operation(
    core: *mut InkpodCore,
    path_utf8: *const u8,
    path_bytes: u64,
    out_info: *mut InkpodDocumentInfo,
    open: bool,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: The path range is readable for this call by contract.
        let path = match unsafe { path_from_utf8(path_utf8, path_bytes) } {
            Ok(path) => path,
            Err(status) => return status,
        };
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        let operation = if open {
            core.core.open(path)
        } else {
            core.core.save(path)
        };
        match operation {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Reopens the last normal-save path and discards unsaved changes.
///
/// # Safety
/// Core/output must be live owner-thread objects with non-overlapping storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_revert(
    core: *mut InkpodCore,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        match core.core.revert() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Applies a logical view command without changing document revision/history.
///
/// # Safety
/// Core/input/output must be complete, live, non-overlapping owner-thread objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_apply_view(
    core: *mut InkpodCore,
    input: *const InkpodViewInput,
    out_info: *mut InkpodDocumentInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        if core.is_null() || !is_aligned(core) {
            return fail(INKPOD_STATUS_INVALID_ARGUMENT, "core is null or misaligned");
        }
        // SAFETY: Public structures expose readable size prefixes.
        if let Err(status) = unsafe { validate_struct(input, "InkpodViewInput") } {
            return status;
        }
        // SAFETY: The output prefix is readable before the validated write.
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodDocumentInfo") }
        {
            return status;
        }
        // SAFETY: Complete live objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let input = unsafe { &*input };
        let out_info = unsafe { &mut *out_info };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if input.flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "view input contains unsupported flags",
            );
        }
        let command = match input.kind {
            INKPOD_VIEW_PAN_BY => ViewCommand::PanBy {
                device_dx: input.value1,
                device_dy: input.value2,
            },
            INKPOD_VIEW_ZOOM_AT => ViewCommand::ZoomAt {
                factor: input.value1,
                device_x: input.value2,
                device_y: input.value3,
            },
            INKPOD_VIEW_FIT => ViewCommand::Fit {
                viewport_width: input.value1,
                viewport_height: input.value2,
            },
            INKPOD_VIEW_ONE_TO_ONE => ViewCommand::OneToOne {
                viewport_width: input.value1,
                viewport_height: input.value2,
            },
            INKPOD_VIEW_VIEWPORT_RESIZED => ViewCommand::ViewportResized {
                viewport_width: input.value1,
                viewport_height: input.value2,
            },
            _ => {
                return fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "view command kind is not defined",
                );
            }
        };
        if let Err(error) = core.core.apply_view(command) {
            return map_core_error(error);
        }
        match core.core.document_info() {
            Ok(info) => {
                write_document_info(out_info, info);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Builds one immutable snapshot owned by Rust.
///
/// # Safety
/// `core` must be live, `options` must expose its advertised readable byte
/// range, and `out_snapshot` must point to non-overlapping handle storage.
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
        // SAFETY: The exported API requires a readable public-structure prefix.
        if let Err(status) = unsafe { validate_struct(options, "InkpodSnapshotOptions") } {
            return status;
        }

        // SAFETY: Live/readable objects are required by the caller contract.
        let core = unsafe { &mut *core };
        let options = unsafe { &*options };
        let thread_status = validate_core_thread(core);
        if thread_status != INKPOD_STATUS_OK {
            return thread_status;
        }
        if options.reserved != 0 || options.feature_flags != INKPOD_FEATURE_NONE {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "snapshot options contain unsupported values",
            );
        }

        let snapshot = core.core.build_snapshot();
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
        let snapshot = Box::new(InkpodSnapshot { snapshot, tiles });
        // SAFETY: The output is writable and receives Box ownership.
        unsafe { out_snapshot.write(Box::into_raw(snapshot)) };
        INKPOD_STATUS_OK
    })
}

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
        out_view.feature_flags = INKPOD_FEATURE_NONE;
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
        output.reserved = 0;
        output.view_revision = view.revision();
        output.zoom = view.zoom();
        output.pan_x = view.pan_x();
        output.pan_y = view.pan_y();
        output.document_width = snapshot.snapshot.document_width();
        output.document_height = snapshot.snapshot.document_height();
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
        // SAFETY: Writable output storage is required by the caller contract.
        unsafe { out_written_bytes.write(0) };
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
            tile_stride_bytes: 0,
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
        assert_eq!(
            view.tile_stride_bytes,
            size_of::<InkpodSnapshotTile>() as u64
        );

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
            command_stride_bytes: size_of::<InkpodCommand>() as u64,
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

        #[repr(C)]
        struct ExtendedCommand {
            command: InkpodCommand,
            extension: u64,
        }
        let extended_commands = [
            ExtendedCommand {
                command: InkpodCommand {
                    struct_size: size_of::<ExtendedCommand>() as u32,
                    kind: INKPOD_COMMAND_NO_OP,
                    flags: 0,
                },
                extension: 1,
            },
            ExtendedCommand {
                command: InkpodCommand {
                    struct_size: size_of::<ExtendedCommand>() as u32,
                    kind: INKPOD_COMMAND_NO_OP,
                    flags: 0,
                },
                extension: 2,
            },
        ];
        let extended_batch = InkpodCommandBatch {
            commands: &extended_commands[0].command,
            command_count: extended_commands.len() as u64,
            command_stride_bytes: size_of::<ExtendedCommand>() as u64,
            ..batch
        };
        // SAFETY: The explicit stride describes both extended records.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &extended_batch, &mut result) },
            INKPOD_STATUS_OK
        );
        assert_eq!(result.accepted_command_count, 2);

        let invalid_stride_batch = InkpodCommandBatch {
            command_stride_bytes: (size_of::<InkpodCommand>() - 1) as u64,
            ..batch
        };
        // SAFETY: Storage is valid; the record stride is intentionally short.
        assert_eq!(
            unsafe { inkpod_core_dispatch_batch(core, &invalid_stride_batch, &mut result) },
            INKPOD_STATUS_INVALID_ARGUMENT
        );

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
        #[repr(C, align(8))]
        struct StructSizePrefix {
            struct_size: u32,
        }

        let mut core = ptr::null_mut();
        // SAFETY: Null input is intentionally tested; output is writable.
        assert_eq!(
            unsafe { inkpod_core_create(ptr::null(), &mut core) },
            INKPOD_STATUS_INVALID_ARGUMENT
        );
        assert!(core.is_null());

        let short = StructSizePrefix { struct_size: 4 };
        // SAFETY: The deliberately short allocation contains the required size
        // prefix and is sufficiently aligned; no complete config is advertised.
        assert_eq!(
            unsafe { inkpod_core_create((&raw const short).cast::<InkpodCoreConfig>(), &mut core) },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(core.is_null());

        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let mut result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        // SAFETY: The short batch exposes only its aligned size prefix.
        assert_eq!(
            unsafe {
                inkpod_core_dispatch_batch(
                    core,
                    (&raw const short).cast::<InkpodCommandBatch>(),
                    &mut result,
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        let empty_batch = InkpodCommandBatch {
            struct_size: size_of::<InkpodCommandBatch>() as u32,
            reserved: 0,
            feature_flags: 0,
            commands: ptr::null(),
            command_count: 0,
            command_stride_bytes: size_of::<InkpodCommand>() as u64,
        };
        let mut short_output = StructSizePrefix { struct_size: 4 };
        // SAFETY: The short result exposes only its writable size prefix.
        assert_eq!(
            unsafe {
                inkpod_core_dispatch_batch(
                    core,
                    &empty_batch,
                    (&raw mut short_output).cast::<InkpodDispatchResult>(),
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        let mut snapshot = ptr::null_mut();
        // SAFETY: The short options expose only their aligned size prefix.
        assert_eq!(
            unsafe {
                inkpod_core_build_snapshot(
                    core,
                    (&raw const short).cast::<InkpodSnapshotOptions>(),
                    &mut snapshot,
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert!(snapshot.is_null());

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        // SAFETY: Inputs and output reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );
        // SAFETY: The short view exposes only its writable size prefix.
        assert_eq!(
            unsafe {
                inkpod_snapshot_get_view(
                    snapshot,
                    (&raw mut short_output).cast::<InkpodSnapshotView>(),
                )
            },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );

        // SAFETY: Owner variables contain live handles.
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn abi_001_contains_panics_and_preserves_a_diagnostic() {
        clear_last_error();
        let status = ffi_boundary(|| panic!("intentional ABI containment test"));
        assert_eq!(status, INKPOD_STATUS_PANIC);

        let mut required = 0;
        // SAFETY: required is writable local storage.
        assert_eq!(
            unsafe { inkpod_error_message_size(&mut required) },
            INKPOD_STATUS_OK
        );
        assert!(required > 1);

        fail(INKPOD_STATUS_INVALID_ARGUMENT, &"界".repeat(ERROR_CAPACITY));
        // SAFETY: required and the subsequently sized buffer are writable.
        assert_eq!(
            unsafe { inkpod_error_message_size(&mut required) },
            INKPOD_STATUS_OK
        );
        let mut message = vec![0_u8; required as usize];
        let mut written = 0;
        // SAFETY: The buffer uses the exact queried capacity.
        assert_eq!(
            unsafe { inkpod_error_message_copy(message.as_mut_ptr(), required, &mut written) },
            INKPOD_STATUS_OK
        );
        assert!(std::str::from_utf8(&message[..written as usize]).is_ok());
    }

    #[test]
    fn m1_batched_stroke_snapshot_view_history_and_round_trip() {
        static PATH_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

        let mut core = ptr::null_mut();
        // SAFETY: All pointers reference initialized local storage.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let create = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 0x1234_5678_9abc_def0,
            document_uuid_low: 0x1032_5476_98ba_dcfe,
            width: 1920,
            height: 1080,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        // SAFETY: Core, options, and output are valid and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_new_cell(core, &create, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!((info.width, info.height), (1920, 1080));
        let ids = (
            info.document_id,
            info.layer_id,
            info.main_plane_id,
            info.color_plane_id,
        );

        #[repr(C)]
        struct ExtendedStrokeSample {
            sample: InkpodStrokeSample,
            extension: u64,
        }
        let samples: Vec<_> = (0..256)
            .map(|index| ExtendedStrokeSample {
                sample: InkpodStrokeSample {
                    struct_size: size_of::<ExtendedStrokeSample>() as u32,
                    flags: 0,
                    x: 10.0 + index as f32,
                    y: 20.0,
                    pressure: 0.5,
                    reserved: 0,
                },
                extension: index,
            })
            .collect();
        let mut stroke = InkpodStrokeInput {
            struct_size: size_of::<InkpodStrokeInput>() as u32,
            tool: INKPOD_TOOL_PENCIL,
            plane: INKPOD_PLANE_MAIN_LINE,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            flags: 0,
            color_rgba: 0x0000_00ff,
            diameter: 1.0,
            samples: &samples[0].sample,
            sample_count: samples.len() as u64,
            sample_stride_bytes: size_of::<ExtendedStrokeSample>() as u64,
        };
        let mut dispatch = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        // SAFETY: One call borrows the complete 256-record sample span.
        assert_eq!(
            unsafe { inkpod_core_apply_stroke(core, &stroke, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        assert_eq!(dispatch.accepted_command_count, 1);
        // SAFETY: Core and output are live owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        let line_checksum = info.main_plane_checksum;

        stroke.plane = INKPOD_PLANE_COLOR;
        stroke.color_rgba = 0xdc_28_1e_ff;
        // SAFETY: The same complete sample span remains live.
        assert_eq!(
            unsafe { inkpod_core_apply_stroke(core, &stroke, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        // SAFETY: Core and output are live owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.main_plane_checksum, line_checksum);
        assert_ne!(info.color_plane_checksum, 0);
        let color_checksum = info.color_plane_checksum;
        // SAFETY: History result storage is live and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_undo(core, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_core_redo(core, &mut dispatch) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.color_plane_checksum, color_checksum);

        let revision_before_view = info.document_revision;
        let view_input = InkpodViewInput {
            struct_size: size_of::<InkpodViewInput>() as u32,
            kind: INKPOD_VIEW_ZOOM_AT,
            flags: 0,
            value1: 2.0,
            value2: 320.0,
            value3: 240.0,
            value4: 0.0,
        };
        // SAFETY: Input/output/Core are complete owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_apply_view(core, &view_input, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, revision_before_view);

        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        let mut snapshot = ptr::null_mut();
        // SAFETY: Core/options/output are valid and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );
        let mut view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: 0,
            revision: 0,
            tiles: ptr::null(),
            tile_count: 0,
            tile_stride_bytes: 0,
        };
        let mut transform = InkpodSnapshotTransform {
            struct_size: size_of::<InkpodSnapshotTransform>() as u32,
            ..InkpodSnapshotTransform::default()
        };
        // SAFETY: Snapshot and outputs remain live for both view calls.
        assert_eq!(
            unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_snapshot_get_transform(snapshot, &mut transform) },
            INKPOD_STATUS_OK
        );
        assert!(view.tile_count > 0 && !view.tiles.is_null());
        assert_eq!(transform.zoom, 2.0);
        assert_eq!(
            (transform.document_width, transform.document_height),
            (1920, 1080)
        );
        // SAFETY: Owner variable contains the live snapshot handle.
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );

        let path = std::env::temp_dir().join(format!(
            "inkpod-ffi-m1-{}-{}.inkpod",
            std::process::id(),
            PATH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let path = path.to_str().unwrap().as_bytes().to_vec();
        // SAFETY: UTF-8 path bytes and output remain live for this call.
        assert_eq!(
            unsafe { inkpod_core_save(core, path.as_ptr(), path.len() as u64, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.flags & INKPOD_DOCUMENT_FLAG_DIRTY, 0);
        // Discard the in-memory document, then reopen from the saved container.
        assert_eq!(
            unsafe { inkpod_core_new_cell(core, &create, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_ne!(info.document_id, ids.0);
        assert_eq!(
            unsafe { inkpod_core_open(core, path.as_ptr(), path.len() as u64, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            (
                info.document_id,
                info.layer_id,
                info.main_plane_id,
                info.color_plane_id
            ),
            ids
        );
        assert_eq!(info.main_plane_checksum, line_checksum);
        assert_eq!(info.color_plane_checksum, color_checksum);
        std::fs::remove_file(std::str::from_utf8(&path).unwrap()).unwrap();
        // SAFETY: Owner variable contains the live Core handle.
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }

    #[test]
    fn live_stroke_abi_previews_then_commits_once_and_cancel_is_safe() {
        let mut core = ptr::null_mut();
        // SAFETY: Config and output storage are complete and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_create(&config(), &mut core) },
            INKPOD_STATUS_OK
        );
        let create = InkpodCellCreateOptions {
            struct_size: size_of::<InkpodCellCreateOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
            document_uuid_high: 7,
            document_uuid_low: 11,
            width: 64,
            height: 64,
            dpi_x_milli: 96_000,
            dpi_y_milli: 96_000,
        };
        let mut info = InkpodDocumentInfo {
            struct_size: size_of::<InkpodDocumentInfo>() as u32,
            ..InkpodDocumentInfo::default()
        };
        // SAFETY: Core/options/output are live and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_new_cell(core, &create, &mut info) },
            INKPOD_STATUS_OK
        );
        let initial_revision = info.document_revision;
        let initial_checksum = info.main_plane_checksum;
        let begin_sample = InkpodStrokeSample {
            struct_size: size_of::<InkpodStrokeSample>() as u32,
            flags: 0,
            x: 4.0,
            y: 4.0,
            pressure: 1.0,
            reserved: 0,
        };
        let begin = InkpodStrokeInput {
            struct_size: size_of::<InkpodStrokeInput>() as u32,
            tool: INKPOD_TOOL_PENCIL,
            plane: INKPOD_PLANE_MAIN_LINE,
            coordinate_space: INKPOD_COORDINATE_SPACE_DOCUMENT,
            flags: 0,
            color_rgba: 0x0000_00ff,
            diameter: 1.0,
            samples: &begin_sample,
            sample_count: 1,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        // SAFETY: The first sample and Core remain live for the call.
        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, initial_revision);
        assert_eq!(info.main_plane_checksum, initial_checksum);

        let appended = [
            InkpodStrokeSample {
                x: 12.0,
                ..begin_sample
            },
            InkpodStrokeSample {
                x: 20.0,
                ..begin_sample
            },
        ];
        let span = InkpodStrokeSampleSpan {
            struct_size: size_of::<InkpodStrokeSampleSpan>() as u32,
            reserved: 0,
            feature_flags: 0,
            samples: appended.as_ptr(),
            sample_count: appended.len() as u64,
            sample_stride_bytes: size_of::<InkpodStrokeSample>() as u64,
        };
        // SAFETY: The strided sample span is complete and borrowed for this call.
        assert_eq!(
            unsafe { inkpod_core_stroke_append(core, &span) },
            INKPOD_STATUS_OK
        );
        let options = InkpodSnapshotOptions {
            struct_size: size_of::<InkpodSnapshotOptions>() as u32,
            reserved: 0,
            feature_flags: 0,
        };
        let mut snapshot = ptr::null_mut();
        // SAFETY: Core/options/output are live and non-overlapping.
        assert_eq!(
            unsafe { inkpod_core_build_snapshot(core, &options, &mut snapshot) },
            INKPOD_STATUS_OK
        );
        let mut view = InkpodSnapshotView {
            struct_size: size_of::<InkpodSnapshotView>() as u32,
            abi_version: 0,
            feature_flags: 0,
            revision: 0,
            tiles: ptr::null(),
            tile_count: 0,
            tile_stride_bytes: 0,
        };
        assert_eq!(
            unsafe { inkpod_snapshot_get_view(snapshot, &mut view) },
            INKPOD_STATUS_OK
        );
        assert!(view.revision >= 1_u64 << 63 && view.tile_count == 1);
        assert_eq!(
            unsafe { inkpod_snapshot_release(&mut snapshot) },
            INKPOD_STATUS_OK
        );

        let mut result = InkpodDispatchResult {
            struct_size: size_of::<InkpodDispatchResult>() as u32,
            reserved: 0,
            revision: 0,
            accepted_command_count: 0,
        };
        // SAFETY: Core and result are live owner-thread objects.
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut result) },
            INKPOD_STATUS_OK
        );
        assert_eq!(result.revision, initial_revision + 1);
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_ne!(info.main_plane_checksum, initial_checksum);
        assert_ne!(info.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO, 0);

        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        assert_eq!(unsafe { inkpod_core_stroke_cancel(core) }, INKPOD_STATUS_OK);
        assert_eq!(unsafe { inkpod_core_stroke_cancel(core) }, INKPOD_STATUS_OK);

        let committed_revision = info.document_revision;
        let committed_checksum = info.main_plane_checksum;
        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        let short_span = InkpodStrokeSampleSpan {
            struct_size: size_of::<u32>() as u32,
            ..span
        };
        assert_eq!(
            unsafe { inkpod_core_stroke_append(core, &short_span) },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut result) },
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(
            unsafe { inkpod_core_get_document_info(core, &mut info) },
            INKPOD_STATUS_OK
        );
        assert_eq!(info.document_revision, committed_revision);
        assert_eq!(info.main_plane_checksum, committed_checksum);

        assert_eq!(
            unsafe { inkpod_core_stroke_begin(core, &begin) },
            INKPOD_STATUS_OK
        );
        let mut short_result = InkpodDispatchResult {
            struct_size: size_of::<u32>() as u32,
            ..result
        };
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut short_result) },
            INKPOD_STATUS_INCOMPATIBLE_ABI
        );
        assert_eq!(
            unsafe { inkpod_core_stroke_end(core, &mut result) },
            INKPOD_STATUS_INVALID_STATE
        );
        assert_eq!(unsafe { inkpod_core_destroy(&mut core) }, INKPOD_STATUS_OK);
    }
}
