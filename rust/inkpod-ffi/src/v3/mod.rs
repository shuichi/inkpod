//! ABI-v3 generation-scoped object registry and value-only primitive control plane.

use super::*;
use std::collections::BTreeMap;

const MAX_OBJECT_COUNT: usize = 4_096;
const MAX_OBJECT_BYTES: u64 = 768 * 1_024 * 1_024;
const MAX_RASTER_OBJECT_BYTES: u64 = 512 * 1_024 * 1_024;

static NEXT_CORE_GENERATION: AtomicU64 = AtomicU64::new(1);

pub(crate) enum ObjectValue {
    Colors(Vec<PixelValue>),
    Samples(Vec<StrokeSample>),
    Raster(RasterAssetInput),
    Snapshot(Box<InkpodSnapshot>),
    Thumbnail {
        width: u32,
        height: u32,
        stride: u64,
        revision: u64,
        bytes: Box<[u8]>,
    },
    Export(Box<[u8]>),
    Task(InkpodTask),
}

impl ObjectValue {
    const fn object_type(&self) -> u32 {
        match self {
            Self::Colors(_) => INKPOD_OBJECT_COLOR_ARRAY,
            Self::Samples(_) => INKPOD_OBJECT_SAMPLE_STREAM,
            Self::Raster(_) => INKPOD_OBJECT_ASSET,
            Self::Snapshot(_) => INKPOD_OBJECT_SNAPSHOT,
            Self::Thumbnail { .. } => INKPOD_OBJECT_THUMBNAIL,
            Self::Export(_) => INKPOD_OBJECT_EXPORT,
            Self::Task(_) => INKPOD_OBJECT_TASK,
        }
    }

    fn logical_bytes(&self) -> u64 {
        match self {
            Self::Colors(values) => (values.len() as u64).saturating_mul(16),
            Self::Samples(values) => {
                (values.len() as u64).saturating_mul(size_of::<StrokeSample>() as u64)
            }
            Self::Raster(value) => value.pixels.len() as u64,
            Self::Snapshot(value) => value
                .snapshot
                .tiles()
                .iter()
                .map(|tile| tile.pixels().len() as u64)
                .fold(0_u64, u64::saturating_add),
            Self::Thumbnail { bytes, .. } | Self::Export(bytes) => bytes.len() as u64,
            Self::Task(_) => 0,
        }
    }
}

pub(crate) struct ObjectRegistry {
    generation: u64,
    next_value: u64,
    logical_bytes: u64,
    objects: BTreeMap<u64, ObjectValue>,
}

impl ObjectRegistry {
    pub(crate) fn new() -> Option<Self> {
        let generation = NEXT_CORE_GENERATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1).filter(|next| *next != 0)
            })
            .ok()?;
        if generation == 0 {
            return None;
        }
        Some(Self {
            generation,
            next_value: 2,
            logical_bytes: 0,
            objects: BTreeMap::new(),
        })
    }

    fn core_id(&self) -> InkpodObjectId {
        object_id(INKPOD_OBJECT_CORE, self.generation, 1)
    }

    fn insert(&mut self, value: ObjectValue) -> Result<InkpodObjectId, u32> {
        if self.objects.len() >= MAX_OBJECT_COUNT {
            return Err(fail(
                INKPOD_STATUS_INVALID_STATE,
                "ABI-v3 object registry count limit exceeded",
            ));
        }
        let bytes = value.logical_bytes();
        let next_bytes = self.logical_bytes.checked_add(bytes).ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_STATE,
                "ABI-v3 object registry byte count overflows",
            )
        })?;
        if next_bytes > MAX_OBJECT_BYTES {
            return Err(fail(
                INKPOD_STATUS_INVALID_STATE,
                "ABI-v3 object registry byte limit exceeded",
            ));
        }
        let slot = self.next_value;
        self.next_value = self
            .next_value
            .checked_add(1)
            .filter(|value| *value != 0)
            .ok_or_else(|| {
                fail(
                    INKPOD_STATUS_INVALID_STATE,
                    "ABI-v3 object slot space is exhausted",
                )
            })?;
        let object_type = value.object_type();
        if self.objects.insert(slot, value).is_some() {
            return Err(fail(
                INKPOD_STATUS_INVALID_STATE,
                "ABI-v3 object slot was unexpectedly reused",
            ));
        }
        self.logical_bytes = next_bytes;
        Ok(object_id(object_type, self.generation, slot))
    }

    fn resolve(&self, id: &InkpodObjectId, expected_type: u32) -> Result<&ObjectValue, u32> {
        validate_live_id(self, id, expected_type)?;
        self.objects.get(&id.value).ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_STATE,
                "ABI-v3 object ID is stale or already released",
            )
        })
    }

    fn remove(&mut self, id: &InkpodObjectId) -> Result<(), u32> {
        validate_live_id(self, id, id.object_type)?;
        if id.object_type == INKPOD_OBJECT_CORE || id.object_type == INKPOD_OBJECT_NONE {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "ABI-v3 object type is not independently releasable",
            ));
        }
        let value = self.objects.remove(&id.value).ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_STATE,
                "ABI-v3 object ID is stale or already released",
            )
        })?;
        self.logical_bytes = self.logical_bytes.saturating_sub(value.logical_bytes());
        Ok(())
    }
}

fn object_id(object_type: u32, generation: u64, value: u64) -> InkpodObjectId {
    InkpodObjectId {
        struct_size: size_of::<InkpodObjectId>() as u32,
        object_type,
        feature_flags: INKPOD_FEATURE_NONE,
        generation,
        value,
    }
}

fn is_null_id(id: &InkpodObjectId) -> bool {
    id.object_type == INKPOD_OBJECT_NONE
        && id.feature_flags == INKPOD_FEATURE_NONE
        && id.generation == 0
        && id.value == 0
}

fn validate_live_id(
    registry: &ObjectRegistry,
    id: &InkpodObjectId,
    expected_type: u32,
) -> Result<(), u32> {
    if id.feature_flags != INKPOD_FEATURE_NONE {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "ABI-v3 object ID contains unsupported feature flags",
        ));
    }
    if id.object_type != expected_type || expected_type == INKPOD_OBJECT_NONE {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 object ID has the wrong type",
        ));
    }
    if id.generation != registry.generation {
        return Err(fail(
            INKPOD_STATUS_INVALID_STATE,
            "ABI-v3 object ID belongs to a different Core generation",
        ));
    }
    if id.value == 0 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 object ID value is zero",
        ));
    }
    Ok(())
}

fn validate_core(core: *mut InkpodCore) -> Result<&'static mut InkpodCore, u32> {
    if core.is_null() || !is_aligned(core) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "core is null or misaligned",
        ));
    }
    // SAFETY: The exported-function contracts require a live unique owner-thread Core.
    let core = unsafe { &mut *core };
    let status = validate_core_thread(core);
    if status != INKPOD_STATUS_OK {
        return Err(status);
    }
    // SAFETY: FFI entry points do not retain this reference beyond their call.
    Ok(unsafe { &mut *(core as *mut InkpodCore) })
}

fn validate_empty_output_id(
    output: *mut InkpodObjectId,
) -> Result<&'static mut InkpodObjectId, u32> {
    // SAFETY: The exported contracts require a readable structure prefix.
    unsafe { validate_struct(output.cast_const(), "InkpodObjectId")? };
    // SAFETY: The complete output record is writable and non-overlapping by contract.
    let output = unsafe { &mut *output };
    if output.feature_flags != 0
        || output.object_type != INKPOD_OBJECT_NONE
        || output.generation != 0
        || output.value != 0
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 output ID must not already own an object",
        ));
    }
    // SAFETY: No reference is retained beyond the exported call.
    Ok(unsafe { &mut *(output as *mut InkpodObjectId) })
}

fn validate_id(id: *const InkpodObjectId) -> Result<&'static InkpodObjectId, u32> {
    // SAFETY: The exported contracts require a readable structure prefix.
    unsafe { validate_struct(id, "InkpodObjectId")? };
    // SAFETY: The complete input record is live for this call.
    Ok(unsafe { &*id })
}

fn parse_raster_input(input: &InkpodRasterAssetInputV3) -> Result<RasterAssetInput, u32> {
    if input.feature_flags != 0 || input.reserved != 0 || input.reserved_2 != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "ABI-v3 raster asset contains unsupported flags or reserved values",
        ));
    }
    let format = parse_storage_format(input.pixel_format)?;
    if input.width == 0
        || input.height == 0
        || input.width > inkpod_core::MAX_RASTER_DIMENSION
        || input.height > inkpod_core::MAX_RASTER_DIMENSION
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 raster dimensions are outside bounds",
        ));
    }
    let bytes_per_pixel = format.bytes_per_pixel() as u64;
    let row_bytes = u64::from(input.width)
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| fail(INKPOD_STATUS_INVALID_ARGUMENT, "raster row size overflows"))?;
    if input.row_stride_bytes < row_bytes {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 raster stride is smaller than one logical row",
        ));
    }
    let advertised = input
        .row_stride_bytes
        .checked_mul(u64::from(input.height.saturating_sub(1)))
        .and_then(|bytes| bytes.checked_add(row_bytes))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "raster byte range overflows",
            )
        })?;
    let logical_bytes = row_bytes
        .checked_mul(u64::from(input.height))
        .ok_or_else(|| {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "raster payload size overflows",
            )
        })?;
    if input.pixels.is_null()
        || input.pixel_bytes != advertised
        || logical_bytes > MAX_RASTER_OBJECT_BYTES
        || advertised > isize::MAX as u64
    {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 raster byte span is null, inconsistent, or too large",
        ));
    }
    let logical_capacity = usize::try_from(logical_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 raster payload is not addressable",
        )
    })?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(logical_capacity).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_STATE,
            "ABI-v3 raster payload allocation failed",
        )
    })?;
    let row_bytes = usize::try_from(row_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 raster row is not addressable",
        )
    })?;
    let stride = usize::try_from(input.row_stride_bytes).map_err(|_| {
        fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 raster stride is not addressable",
        )
    })?;
    for row in 0..input.height as usize {
        // SAFETY: The exact checked advertised span is readable for this call.
        let source = unsafe { slice::from_raw_parts(input.pixels.add(row * stride), row_bytes) };
        pixels.extend_from_slice(source);
    }
    if format == PixelFormat::BinaryMask8 && pixels.iter().any(|value| !matches!(*value, 0 | 255)) {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "binary asset pixels must be exactly zero or 255",
        ));
    }
    let (color_space, alpha_semantics) = match format {
        PixelFormat::BinaryMask8 => (None, AssetAlphaSemantics::CoverageMask),
        PixelFormat::Grayscale8 | PixelFormat::Grayscale16 => (None, AssetAlphaSemantics::Opaque),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16 => {
            (Some(AssetColorSpace::Srgb), AssetAlphaSemantics::Straight)
        }
        PixelFormat::PremultipliedBgra8 => {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "display-only pixel format cannot be registered as an asset",
            ));
        }
    };
    Ok(RasterAssetInput {
        width: input.width,
        height: input.height,
        pixel_format: format,
        color_space,
        alpha_semantics,
        canonical_stride: row_bytes as u64,
        pixels,
        expected_id: None,
    })
}

fn primitive_schema(opcode: u32) -> Option<u32> {
    match opcode {
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR | INKPOD_PRIMITIVE_REPLACE_PALETTE => Some(1),
        INKPOD_PRIMITIVE_APPLY_RASTER_STROKE => Some(2),
        INKPOD_PRIMITIVE_IMPORT_RASTER_ASSET => Some(1),
        _ => None,
    }
}

fn parse_primitive_request(
    core: &InkpodCore,
    request: &InkpodPrimitiveRequestV3,
) -> Result<PrimitiveRequest, u32> {
    if request.reserved != 0
        || request.reserved_2 != 0
        || request.reserved_3 != 0
        || request.feature_flags != 0
    {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "ABI-v3 primitive request contains unsupported flags or reserved values",
        ));
    }
    let Some(schema) = primitive_schema(request.opcode) else {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 primitive opcode is not in the current catalog",
        ));
    };
    if request.schema_version != schema {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "ABI-v3 primitive schema version is unsupported",
        ));
    }
    match request.opcode {
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR => {
            if !is_null_id(&request.payload_id)
                || request.target_id != 0
                || request.tool != 0
                || request.plane != 0
                || request.coordinate_space != 0
                || request.stroke_flags != 0
                || request.diameter != 0.0
            {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "main-line primitive contains fields outside its schema",
                ));
            }
            // SAFETY: The nested complete color record was validated by the caller.
            let color = unsafe { parse_color_value(&raw const request.color) }?;
            Ok(PrimitiveRequest::SetMainLineColor {
                expected_revision: request.base_revision,
                color,
            })
        }
        INKPOD_PRIMITIVE_REPLACE_PALETTE => {
            let colors = match core
                .objects
                .resolve(&request.payload_id, INKPOD_OBJECT_COLOR_ARRAY)?
            {
                ObjectValue::Colors(colors) => colors.clone(),
                _ => unreachable!("object type validation fixes the variant"),
            };
            Ok(PrimitiveRequest::ReplacePalette {
                expected_revision: request.base_revision,
                colors,
            })
        }
        INKPOD_PRIMITIVE_APPLY_RASTER_STROKE => {
            if request.target_id == 0 {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "raster-stroke target ID is zero",
                ));
            }
            let samples = match core
                .objects
                .resolve(&request.payload_id, INKPOD_OBJECT_SAMPLE_STREAM)?
            {
                ObjectValue::Samples(samples) => samples.clone(),
                _ => unreachable!("object type validation fixes the variant"),
            };
            let tool = parse_tool(request.tool)?;
            let plane = parse_plane(request.plane)?;
            let coordinate_space = parse_coordinate_space(request.coordinate_space)?;
            if request.stroke_flags
                & !(INKPOD_STROKE_FLAG_AUTO_ERASE | INKPOD_STROKE_FLAG_PRESSURE_SIZE)
                != 0
            {
                return Err(fail(
                    INKPOD_STATUS_UNSUPPORTED,
                    "raster-stroke flags are unsupported",
                ));
            }
            // SAFETY: The nested complete color record was validated by the caller.
            let color = unsafe { parse_color_value(&raw const request.color) }?;
            let PixelValue::Rgba(color) = color else {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "ABI-v3 raster stroke currently requires exact RGBA8",
                ));
            };
            Ok(PrimitiveRequest::ApplyRasterStroke {
                expected_revision: request.base_revision,
                target_plane_id: request.target_id,
                stroke: Stroke {
                    tool,
                    plane,
                    color,
                    diameter: request.diameter,
                    auto_erase: request.stroke_flags & INKPOD_STROKE_FLAG_AUTO_ERASE != 0,
                    pressure_size: request.stroke_flags & INKPOD_STROKE_FLAG_PRESSURE_SIZE != 0,
                    coordinate_space,
                    samples,
                },
            })
        }
        INKPOD_PRIMITIVE_IMPORT_RASTER_ASSET => {
            if request.target_id == 0 {
                return Err(fail(
                    INKPOD_STATUS_INVALID_ARGUMENT,
                    "raster-import target ID is zero",
                ));
            }
            let raster = match core
                .objects
                .resolve(&request.payload_id, INKPOD_OBJECT_ASSET)?
            {
                ObjectValue::Raster(raster) => raster.clone(),
                _ => unreachable!("object type validation fixes the variant"),
            };
            Ok(PrimitiveRequest::ImportRasterAsset {
                expected_revision: request.base_revision,
                target_plane_id: request.target_id,
                raster,
            })
        }
        _ => unreachable!("catalog validation covers every opcode"),
    }
}

fn write_primitive_result(output: &mut InkpodPrimitiveResultV3, outcome: PrimitiveOutcome) {
    let dispatch = outcome.dispatch();
    output.flags = 0;
    output.revision = dispatch.revision();
    output.accepted_command_count = dispatch.accepted_commands();
    output.procedure_id = 0;
    output.committed_state_id = 0;
    output.opcode = 0;
    output.schema_version = 0;
    if let Some(procedure) = outcome.procedure() {
        output.flags = INKPOD_PRIMITIVE_RESULT_COMMITTED;
        output.procedure_id = procedure.procedure_id().get();
        output.committed_state_id = procedure.committed_state_id().get();
        output.opcode = procedure.primitive_id().get();
        output.schema_version = u32::from(procedure.primitive_schema_version());
    }
}

fn validate_copy(copy: *mut InkpodBufferCopyV3) -> Result<&'static mut InkpodBufferCopyV3, u32> {
    // SAFETY: The exported contracts require a readable structure prefix.
    unsafe { validate_struct(copy.cast_const(), "InkpodBufferCopyV3")? };
    // SAFETY: The complete record is writable for this call.
    let copy = unsafe { &mut *copy };
    if copy.reserved != 0 || copy.feature_flags != 0 {
        return Err(fail(
            INKPOD_STATUS_UNSUPPORTED,
            "ABI-v3 byte-copy request contains unsupported values",
        ));
    }
    if copy.byte_capacity == 0 {
        if !copy.bytes.is_null() {
            return Err(fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "zero-capacity ABI-v3 byte copy must use a null pointer",
            ));
        }
    } else if copy.bytes.is_null() || copy.byte_capacity > isize::MAX as u64 {
        return Err(fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 byte-copy storage is invalid",
        ));
    }
    Ok(unsafe { &mut *(copy as *mut InkpodBufferCopyV3) })
}

fn copy_bytes(source: &[u8], copy: &mut InkpodBufferCopyV3) -> u32 {
    copy.total_bytes = source.len() as u64;
    copy.written_bytes = 0;
    if copy.offset > copy.total_bytes {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 byte-copy offset is past the object",
        );
    }
    let remaining = copy.total_bytes - copy.offset;
    if copy.byte_capacity == 0 {
        return INKPOD_STATUS_OK;
    }
    let copied = remaining.min(copy.byte_capacity);
    let offset = copy.offset as usize;
    let copied = copied as usize;
    // SAFETY: The caller advertises `byte_capacity` writable bytes and the source range is checked.
    unsafe { ptr::copy_nonoverlapping(source.as_ptr().add(offset), copy.bytes, copied) };
    copy.written_bytes = copied as u64;
    INKPOD_STATUS_OK
}

unsafe fn copy_records<T: Copy>(
    source: &[T],
    first: u64,
    output: *mut T,
    capacity: u64,
    stride_bytes: u64,
    out_copied: *mut u64,
) -> u32 {
    if out_copied.is_null() || !is_aligned(out_copied) {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 record-copy count output is null or misaligned",
        );
    }
    // SAFETY: Writable count output is required by contract.
    unsafe { out_copied.write(0) };
    if first > source.len() as u64 {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 record-copy start is past the source",
        );
    }
    if capacity == 0 {
        return if output.is_null() && stride_bytes == 0 {
            INKPOD_STATUS_OK
        } else {
            fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "zero-capacity ABI-v3 record copy must use null and zero stride",
            )
        };
    }
    if output.is_null() || !is_aligned(output) {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 record-copy output is null or misaligned",
        );
    }
    let stride = match usize::try_from(stride_bytes) {
        Ok(stride) if stride >= size_of::<T>() && stride % align_of::<T>() == 0 => stride,
        _ => {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "ABI-v3 record-copy stride is invalid",
            );
        }
    };
    let count = capacity.min(source.len() as u64 - first);
    let count = match usize::try_from(count) {
        Ok(count) => count,
        Err(_) => {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "ABI-v3 record-copy count is not representable",
            );
        }
    };
    let storage = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(size_of::<T>()));
    if storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return fail(
            INKPOD_STATUS_INVALID_ARGUMENT,
            "ABI-v3 record-copy storage overflows",
        );
    }
    let first = first as usize;
    for index in 0..count {
        // SAFETY: The checked caller-owned strided output range is writable.
        unsafe {
            output
                .cast::<u8>()
                .add(index * stride)
                .cast::<T>()
                .write(source[first + index]);
        }
    }
    // SAFETY: Writable count output is required by contract.
    unsafe { out_copied.write(count as u64) };
    INKPOD_STATUS_OK
}

/// Returns the generation-tagged identity of a live Core.
///
/// # Safety
/// `core` must be live on its owner thread and `out_id` must be aligned, writable, and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_get_id_v3(
    core: *mut InkpodCore,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        *output = core.objects.core_id();
        INKPOD_STATUS_OK
    })
}

/// Deep-copies an exact-depth color span into one Rust-owned runtime object.
///
/// # Safety
/// `core` must be live on its owner thread; `input` and its advertised span must be readable, and `out_id` must be writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_register_color_array_v3(
    core: *mut InkpodCore,
    input: *const InkpodColorArray,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(input, "InkpodColorArray") } {
            return status;
        }
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        // SAFETY: The complete input and its advertised span are live for this call.
        let colors = match unsafe { parse_color_array(&*input) } {
            Ok(colors) => colors,
            Err(status) => return status,
        };
        match core.objects.insert(ObjectValue::Colors(colors)) {
            Ok(id) => {
                *output = id;
                INKPOD_STATUS_OK
            }
            Err(status) => status,
        }
    })
}

/// Deep-copies a strided sample span into one Rust-owned runtime object.
///
/// # Safety
/// `core` must be live on its owner thread; `input` and its advertised span must be readable, and `out_id` must be writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_register_sample_stream_v3(
    core: *mut InkpodCore,
    input: *const InkpodStrokeSampleSpan,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(input, "InkpodStrokeSampleSpan") } {
            return status;
        }
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        // SAFETY: The complete span record is live for this call.
        let input = unsafe { &*input };
        if input.reserved != 0 || input.feature_flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "ABI-v3 sample stream contains unsupported values",
            );
        }
        // SAFETY: The advertised strided span is readable for this bounded call.
        let samples = match unsafe {
            parse_stroke_samples(input.samples, input.sample_count, input.sample_stride_bytes)
        } {
            Ok(samples) => samples,
            Err(status) => return status,
        };
        match core.objects.insert(ObjectValue::Samples(samples)) {
            Ok(id) => {
                *output = id;
                INKPOD_STATUS_OK
            }
            Err(status) => status,
        }
    })
}

/// Deep-copies a bounded raster span into one Rust-owned runtime object.
///
/// # Safety
/// `core` must be live on its owner thread; `input` and its advertised bytes must be readable, and `out_id` must be writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_register_raster_asset_v3(
    core: *mut InkpodCore,
    input: *const InkpodRasterAssetInputV3,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(input, "InkpodRasterAssetInputV3") } {
            return status;
        }
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        // SAFETY: The complete input and exact advertised byte span are live for this call.
        let raster = match parse_raster_input(unsafe { &*input }) {
            Ok(raster) => raster,
            Err(status) => return status,
        };
        match core.objects.insert(ObjectValue::Raster(raster)) {
            Ok(id) => {
                *output = id;
                INKPOD_STATUS_OK
            }
            Err(status) => status,
        }
    })
}

/// Executes one pointer-free primitive request through the canonical Core executor.
///
/// # Safety
/// `core` must be live on its owner thread; `request` must be readable and `result` writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_primitive_execute_v3(
    core: *mut InkpodCore,
    request: *const InkpodPrimitiveRequestV3,
    result: *mut InkpodPrimitiveResultV3,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(request, "InkpodPrimitiveRequestV3") } {
            return status;
        }
        if let Err(status) =
            unsafe { validate_struct(result.cast_const(), "InkpodPrimitiveResultV3") }
        {
            return status;
        }
        // SAFETY: The complete request is readable for this call.
        let request = unsafe { &*request };
        if let Err(status) =
            unsafe { validate_struct(&raw const request.payload_id, "InkpodObjectId") }
        {
            return status;
        }
        if matches!(
            request.opcode,
            INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR | INKPOD_PRIMITIVE_APPLY_RASTER_STROKE
        ) {
            if let Err(status) =
                unsafe { validate_struct(&raw const request.color, "InkpodColorValue") }
            {
                return status;
            }
        }
        let request = match parse_primitive_request(core, request) {
            Ok(request) => request,
            Err(status) => return status,
        };
        match core.core.execute_primitive(request) {
            Ok(outcome) => {
                // SAFETY: The complete non-overlapping output is writable by contract.
                write_primitive_result(unsafe { &mut *result }, outcome);
                INKPOD_STATUS_OK
            }
            Err(error) => map_core_error(error),
        }
    })
}

/// Builds an immutable render snapshot and returns its generation-tagged ID.
///
/// # Safety
/// `core` must be live on its owner thread; `options` must be readable and `out_id` writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_build_snapshot_id_v3(
    core: *mut InkpodCore,
    options: *const InkpodSnapshotOptions,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(options, "InkpodSnapshotOptions") } {
            return status;
        }
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        // SAFETY: The complete options record is readable for this call.
        let options = unsafe { &*options };
        if options.reserved != 0 || options.feature_flags != 0 {
            return fail(
                INKPOD_STATUS_UNSUPPORTED,
                "ABI-v3 snapshot options contain unsupported values",
            );
        }
        let snapshot = *snapshot_handle(core.core.build_snapshot());
        match core
            .objects
            .insert(ObjectValue::Snapshot(Box::new(snapshot)))
        {
            Ok(id) => {
                *output = id;
                INKPOD_STATUS_OK
            }
            Err(status) => status,
        }
    })
}

/// Copies pointer-free metadata for one live snapshot ID.
///
/// # Safety
/// `core` must be live on its owner thread; `id` must be readable and `out_info` writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_snapshot_get_info_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    out_info: *mut InkpodSnapshotInfoV3,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        if let Err(status) =
            unsafe { validate_struct(out_info.cast_const(), "InkpodSnapshotInfoV3") }
        {
            return status;
        }
        let snapshot = match core.objects.resolve(id, INKPOD_OBJECT_SNAPSHOT) {
            Ok(ObjectValue::Snapshot(snapshot)) => snapshot,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        let view = snapshot.snapshot.view();
        // SAFETY: Complete writable output is required by contract.
        let output = unsafe { &mut *out_info };
        output.transform_flags = (if view.flip_horizontal() {
            INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL
        } else {
            0
        }) | if view.flip_vertical() {
            INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL
        } else {
            0
        };
        output.feature_flags = snapshot.snapshot.feature_flags();
        output.revision = snapshot.snapshot.revision();
        output.view_revision = view.revision();
        output.tile_count = snapshot.tiles.len() as u64;
        output.guide_count = snapshot.guides.len() as u64;
        output.vector_segment_count = snapshot.vector_segments.len() as u64;
        output.vector_fill_count = snapshot.vector_fills.len() as u64;
        output.vector_boundary_path_count = snapshot.vector_boundary_path_ids.len() as u64;
        output.zoom = view.zoom();
        output.pan_x = view.pan_x();
        output.pan_y = view.pan_y();
        output.document_width = snapshot.snapshot.document_width();
        output.document_height = snapshot.snapshot.document_height();
        INKPOD_STATUS_OK
    })
}

/// Copies a bounded batch of pointer-free snapshot tile descriptors.
///
/// # Safety
/// `core` must be live on its owner thread, `id` readable, `out_copied` writable, and the advertised output span writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_snapshot_tiles_copy_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    first: u64,
    output: *mut InkpodSnapshotTileInfoV3,
    capacity: u64,
    stride_bytes: u64,
    out_copied: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let snapshot = match core.objects.resolve(id, INKPOD_OBJECT_SNAPSHOT) {
            Ok(ObjectValue::Snapshot(snapshot)) => snapshot,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        let records = snapshot
            .tiles
            .iter()
            .map(|tile| InkpodSnapshotTileInfoV3 {
                struct_size: size_of::<InkpodSnapshotTileInfoV3>() as u32,
                pixel_format: tile.pixel_format,
                tile_id: tile.tile_id,
                origin_x: tile.origin_x,
                origin_y: tile.origin_y,
                width: tile.width,
                height: tile.height,
                stride_bytes: tile.stride_bytes,
                reserved: 0,
                pixel_bytes: tile.pixel_bytes,
                tile_revision: tile.tile_revision,
            })
            .collect::<Vec<_>>();
        // SAFETY: The helper validates the complete caller-owned strided output.
        unsafe { copy_records(&records, first, output, capacity, stride_bytes, out_copied) }
    })
}

/// Copies a bounded byte range from one snapshot tile.
///
/// # Safety
/// `core` must be live on its owner thread, `id` readable, and `copy` plus its advertised storage writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_snapshot_tile_pixels_copy_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    tile_index: u64,
    copy: *mut InkpodBufferCopyV3,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let copy = match validate_copy(copy) {
            Ok(copy) => copy,
            Err(status) => return status,
        };
        let snapshot = match core.objects.resolve(id, INKPOD_OBJECT_SNAPSHOT) {
            Ok(ObjectValue::Snapshot(snapshot)) => snapshot,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        let Some(tile) = snapshot.snapshot.tiles().get(tile_index as usize) else {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "ABI-v3 snapshot tile index is outside bounds",
            );
        };
        copy_bytes(tile.pixels(), copy)
    })
}

/// Copies a bounded batch of snapshot guide records.
///
/// # Safety
/// `core` must be live on its owner thread, `id` readable, `out_copied` writable, and the advertised output span writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_snapshot_guides_copy_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    first: u64,
    output: *mut InkpodSnapshotGuide,
    capacity: u64,
    stride_bytes: u64,
    out_copied: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let snapshot = match core.objects.resolve(id, INKPOD_OBJECT_SNAPSHOT) {
            Ok(ObjectValue::Snapshot(snapshot)) => snapshot,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        // SAFETY: The helper validates the complete caller-owned strided output.
        unsafe {
            copy_records(
                &snapshot.guides,
                first,
                output,
                capacity,
                stride_bytes,
                out_copied,
            )
        }
    })
}

/// Copies a bounded batch of snapshot vector-segment records.
///
/// # Safety
/// `core` must be live on its owner thread, `id` readable, `out_copied` writable, and the advertised output span writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_snapshot_vector_segments_copy_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    first: u64,
    output: *mut InkpodSnapshotVectorSegment,
    capacity: u64,
    stride_bytes: u64,
    out_copied: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let snapshot = match core.objects.resolve(id, INKPOD_OBJECT_SNAPSHOT) {
            Ok(ObjectValue::Snapshot(snapshot)) => snapshot,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        // SAFETY: The helper validates the complete caller-owned strided output.
        unsafe {
            copy_records(
                &snapshot.vector_segments,
                first,
                output,
                capacity,
                stride_bytes,
                out_copied,
            )
        }
    })
}

/// Copies a bounded batch of snapshot vector-fill records.
///
/// # Safety
/// `core` must be live on its owner thread, `id` readable, `out_copied` writable, and the advertised output span writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_snapshot_vector_fills_copy_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    first: u64,
    output: *mut InkpodSnapshotVectorFill,
    capacity: u64,
    stride_bytes: u64,
    out_copied: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let snapshot = match core.objects.resolve(id, INKPOD_OBJECT_SNAPSHOT) {
            Ok(ObjectValue::Snapshot(snapshot)) => snapshot,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        // SAFETY: The helper validates the complete caller-owned strided output.
        unsafe {
            copy_records(
                &snapshot.vector_fills,
                first,
                output,
                capacity,
                stride_bytes,
                out_copied,
            )
        }
    })
}

/// Copies a bounded batch of packed snapshot boundary-path IDs.
///
/// # Safety
/// `core` must be live on its owner thread, `id` readable, `out_copied` writable, and the advertised output span writable and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_snapshot_vector_boundary_ids_copy_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    first: u64,
    output: *mut u64,
    capacity: u64,
    stride_bytes: u64,
    out_copied: *mut u64,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let snapshot = match core.objects.resolve(id, INKPOD_OBJECT_SNAPSHOT) {
            Ok(ObjectValue::Snapshot(snapshot)) => snapshot,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        // SAFETY: The helper validates the complete caller-owned strided output.
        unsafe {
            copy_records(
                &snapshot.vector_boundary_path_ids,
                first,
                output,
                capacity,
                stride_bytes,
                out_copied,
            )
        }
    })
}

/// Builds a bounded layer thumbnail and returns its Rust-owned runtime ID.
///
/// # Safety
/// `core` must be live on its owner thread and `out_id` must be writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_layer_thumbnail_id_v3(
    core: *mut InkpodCore,
    layer_id: u64,
    maximum_width: u32,
    maximum_height: u32,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        let thumbnail = match core
            .core
            .layer_thumbnail(layer_id, maximum_width, maximum_height)
        {
            Ok(thumbnail) => thumbnail,
            Err(error) => return map_core_error(error),
        };
        match core.objects.insert(ObjectValue::Thumbnail {
            width: thumbnail.width,
            height: thumbnail.height,
            stride: u64::from(thumbnail.stride_bytes),
            revision: thumbnail.revision,
            bytes: thumbnail.pixels.into_boxed_slice(),
        }) {
            Ok(id) => {
                *output = id;
                INKPOD_STATUS_OK
            }
            Err(status) => status,
        }
    })
}

/// Encodes a common-raster export into one Rust-owned runtime object.
///
/// # Safety
/// `core` must be live on its owner thread and `out_id` must be writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_export_common_raster_id_v3(
    core: *mut InkpodCore,
    format: u32,
    composite_white: u32,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        if composite_white > 1 {
            return fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "ABI-v3 export white-composite flag must be zero or one",
            );
        }
        let format = match parse_common_raster_format(format) {
            Ok(format) => format,
            Err(status) => return status,
        };
        let bytes = match core.core.export_common_raster(format, composite_white != 0) {
            Ok(bytes) => bytes,
            Err(error) => return map_core_error(error),
        };
        match core
            .objects
            .insert(ObjectValue::Export(bytes.into_boxed_slice()))
        {
            Ok(id) => {
                *output = id;
                INKPOD_STATUS_OK
            }
            Err(status) => status,
        }
    })
}

/// Copies pointer-free metadata for one live runtime object ID.
///
/// # Safety
/// `core` must be live on its owner thread; `id` must be readable and `out_info` writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_object_get_info_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    out_info: *mut InkpodObjectInfoV3,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodObjectInfoV3") }
        {
            return status;
        }
        let value = match core.objects.resolve(id, id.object_type) {
            Ok(value) => value,
            Err(status) => return status,
        };
        // SAFETY: Complete writable output is required by contract.
        let output = unsafe { &mut *out_info };
        output.object_type = id.object_type;
        output.feature_flags = 0;
        output.generation = id.generation;
        output.value = id.value;
        output.element_count = 0;
        output.byte_count = value.logical_bytes();
        output.width = 0;
        output.height = 0;
        output.stride_bytes = 0;
        output.revision = 0;
        match value {
            ObjectValue::Colors(values) => output.element_count = values.len() as u64,
            ObjectValue::Samples(values) => output.element_count = values.len() as u64,
            ObjectValue::Raster(value) => {
                output.element_count = u64::from(value.width) * u64::from(value.height);
                output.width = value.width;
                output.height = value.height;
                output.stride_bytes = value.canonical_stride;
            }
            ObjectValue::Snapshot(value) => {
                output.element_count = value.tiles.len() as u64;
                output.width = value.snapshot.document_width();
                output.height = value.snapshot.document_height();
                output.revision = value.snapshot.revision();
            }
            ObjectValue::Thumbnail {
                width,
                height,
                stride,
                revision,
                ..
            } => {
                output.element_count = u64::from(*width) * u64::from(*height);
                output.width = *width;
                output.height = *height;
                output.stride_bytes = *stride;
                output.revision = *revision;
            }
            ObjectValue::Export(_) | ObjectValue::Task(_) => {}
        }
        INKPOD_STATUS_OK
    })
}

/// Copies a bounded byte range from a thumbnail or export runtime object.
///
/// # Safety
/// `core` must be live on its owner thread, `id` readable, and `copy` plus its advertised storage writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_object_bytes_copy_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    copy: *mut InkpodBufferCopyV3,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let copy = match validate_copy(copy) {
            Ok(copy) => copy,
            Err(status) => return status,
        };
        let value = match core.objects.resolve(id, id.object_type) {
            Ok(value) => value,
            Err(status) => return status,
        };
        match value {
            ObjectValue::Thumbnail { bytes, .. } | ObjectValue::Export(bytes) => {
                copy_bytes(bytes, copy)
            }
            _ => fail(
                INKPOD_STATUS_INVALID_ARGUMENT,
                "ABI-v3 object does not expose a generic byte-copy view",
            ),
        }
    })
}

/// Creates a cancellable task runtime object owned by the Core registry.
///
/// # Safety
/// `core` must be live on its owner thread and `out_id` must be writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_task_create_v3(
    core: *mut InkpodCore,
    out_id: *mut InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let output = match validate_empty_output_id(out_id) {
            Ok(output) => output,
            Err(status) => return status,
        };
        match core.objects.insert(ObjectValue::Task(InkpodTask::new())) {
            Ok(id) => {
                *output = id;
                INKPOD_STATUS_OK
            }
            Err(status) => status,
        }
    })
}

/// Copies task progress and state from one live task ID.
///
/// # Safety
/// `core` must be live on its owner thread; `id` must be readable and `out_info` writable, aligned, and non-overlapping for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_task_query_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
    out_info: *mut InkpodTaskInfo,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        if let Err(status) = unsafe { validate_struct(out_info.cast_const(), "InkpodTaskInfo") } {
            return status;
        }
        let task = match core.objects.resolve(id, INKPOD_OBJECT_TASK) {
            Ok(ObjectValue::Task(task)) => task,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        // SAFETY: Complete writable output is required by contract.
        let output = unsafe { &mut *out_info };
        output.state = task.state.load(Ordering::Acquire);
        output.reserved = 0;
        output.completed_work = task.completed_work.load(Ordering::Acquire);
        output.total_work = task.total_work.load(Ordering::Acquire);
        INKPOD_STATUS_OK
    })
}

/// Idempotently requests cancellation of one live task object.
///
/// # Safety
/// `core` must be live on its owner thread and `id` must point to a readable, aligned record for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_task_cancel_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        let task = match core.objects.resolve(id, INKPOD_OBJECT_TASK) {
            Ok(ObjectValue::Task(task)) => task,
            Ok(_) => unreachable!("object type validation fixes the variant"),
            Err(status) => return status,
        };
        task.cancelled.store(true, Ordering::Release);
        task.state.store(INKPOD_TASK_CANCELLED, Ordering::Release);
        INKPOD_STATUS_OK
    })
}

/// Releases one non-Core runtime object exactly once.
///
/// # Safety
/// `core` must be live on its owner thread and `id` must point to a readable, aligned record for this call; no concurrent query may use the same object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inkpod_core_object_release_v3(
    core: *mut InkpodCore,
    id: *const InkpodObjectId,
) -> u32 {
    ffi_boundary(|| {
        clear_last_error();
        let core = match validate_core(core) {
            Ok(core) => core,
            Err(status) => return status,
        };
        let id = match validate_id(id) {
            Ok(id) => id,
            Err(status) => return status,
        };
        match core.objects.remove(id) {
            Ok(()) => INKPOD_STATUS_OK,
            Err(status) => status,
        }
    })
}

#[cfg(test)]
#[path = "../../tests/unit/v3.rs"]
mod tests;
