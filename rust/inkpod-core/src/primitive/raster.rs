//! Canonical fixed-point raster-stroke normalization and execution.

use super::CanonicalStrokeArguments;
use crate::document::ensure_editable_plane;
use crate::view::{device_to_document, stroke_coordinate_is_supported};
use crate::{
    CellDocument, CoordinateSpace, CoreError, DevicePointF64, DocumentSizeU32, MAX_BRUSH_DIAMETER,
    MAX_STROKE_COORDINATE, MAX_STROKE_SAMPLES, MAX_STROKE_WORK, PaintTool, PixelChange,
    PixelFormat, PixelValue, PlaneId, PlaneType, Stroke, StrokeSample, TILE_SIZE, TileCoord,
    ViewState,
};
use std::cmp::Ordering;
use std::collections::BTreeSet;

const Q16_ONE: i64 = 1_i64 << 16;
const MAX_Q16_COORDINATE: i64 = 16_777_216_i64 * Q16_ONE;
const PRESSURE_MAX: u16 = u16::MAX;
const PRESSURE_MINIMUM_SIZE: u16 = 655;
const CANONICAL_SAMPLE_BYTES: usize = 24;
const CANONICAL_PAYLOAD_HEADER_BYTES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalStrokeSample {
    pub(crate) x_q16: i64,
    pub(crate) y_q16: i64,
    pub(crate) pressure: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CanonicalRasterStroke {
    pub(super) tool: PaintTool,
    pub(super) target_plane_id: PlaneId,
    pub(super) color_rgba8: [u8; 4],
    pub(super) diameter_q16: i64,
    pub(super) auto_erase: bool,
    pub(super) pressure_size: bool,
    pub(super) samples: Vec<CanonicalStrokeSample>,
}

/// Private incremental state for an interactive raster-stroke preview.
///
/// The final primitive still owns one closed-schema payload; this state keeps
/// decoded samples only while the preview is active so appends never decode or
/// re-encode the complete payload. Pixel staging delegates to the same segment
/// walker used by committed execution.
#[derive(Clone, Debug)]
pub(crate) struct RasterStrokePreview {
    stroke: CanonicalRasterStroke,
    desired: PixelValue,
    maximum_radius: i64,
    work: u64,
}

/// Resolves public stroke scalars and samples to canonical procedure arguments.
///
/// Device coordinates are converted through the captured view before the
/// resulting binary64 document coordinates are quantized. The exact stable
/// target plane is supplied by the caller rather than inferred from the public
/// role hint. The returned payload owns all variable input.
pub(crate) fn canonicalize(
    stroke: &Stroke,
    view: &ViewState,
    width: u32,
    height: u32,
    target_plane_id: u64,
) -> Result<CanonicalStrokeArguments, CoreError> {
    validate_public_stroke(stroke)?;
    if target_plane_id == 0 {
        return Err(CoreError::InvalidArgument(
            "stroke target plane ID must be nonzero",
        ));
    }

    let samples = canonicalize_stroke_samples(
        *view,
        stroke.coordinate_space,
        &stroke.samples,
        width,
        height,
    )?;
    let diameter_q16 = canonical_q16_from_f32(stroke.diameter)?;
    let diameter_q16 = diameter_q16.max(1);
    if diameter_q16 > i64::from(256) * Q16_ONE {
        return Err(CoreError::InvalidArgument(
            "stroke diameter is outside bounds",
        ));
    }

    Ok(CanonicalStrokeArguments {
        target_plane_id,
        tool_code: tool_code(stroke.tool),
        color: stroke.color,
        diameter_q16,
        auto_erase: stroke.auto_erase,
        pressure_size: stroke.pressure_size,
        payload: encode_payload(&samples)?,
    })
}

/// Decodes and validates the closed schema-1 inline stroke payload.
pub(crate) fn decode_payload(payload: &[u8]) -> Result<Vec<CanonicalStrokeSample>, CoreError> {
    if payload.len() < CANONICAL_PAYLOAD_HEADER_BYTES {
        return Err(CoreError::InvalidArgument(
            "canonical stroke payload is shorter than its count",
        ));
    }
    let count = u64::from_le_bytes(
        payload[..CANONICAL_PAYLOAD_HEADER_BYTES]
            .try_into()
            .map_err(|_| CoreError::InvalidArgument("canonical stroke count is malformed"))?,
    );
    let count = usize::try_from(count).map_err(|_| {
        CoreError::InvalidArgument("canonical stroke sample count is not representable")
    })?;
    if count == 0 || count > MAX_STROKE_SAMPLES {
        return Err(CoreError::InvalidArgument(
            "canonical stroke sample count is outside bounds",
        ));
    }
    let expected_length = count
        .checked_mul(CANONICAL_SAMPLE_BYTES)
        .and_then(|bytes| bytes.checked_add(CANONICAL_PAYLOAD_HEADER_BYTES))
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke payload length overflows",
        ))?;
    if payload.len() != expected_length {
        return Err(CoreError::InvalidArgument(
            "canonical stroke payload length does not match its count",
        ));
    }

    let mut samples = Vec::with_capacity(count);
    for bytes in payload[CANONICAL_PAYLOAD_HEADER_BYTES..].chunks_exact(CANONICAL_SAMPLE_BYTES) {
        let x_q16 = i64::from_le_bytes(bytes[0..8].try_into().map_err(|_| {
            CoreError::InvalidArgument("canonical stroke x coordinate is malformed")
        })?);
        let y_q16 = i64::from_le_bytes(bytes[8..16].try_into().map_err(|_| {
            CoreError::InvalidArgument("canonical stroke y coordinate is malformed")
        })?);
        let pressure =
            u16::from_le_bytes(bytes[16..18].try_into().map_err(|_| {
                CoreError::InvalidArgument("canonical stroke pressure is malformed")
            })?);
        if x_q16.unsigned_abs() > MAX_Q16_COORDINATE as u64
            || y_q16.unsigned_abs() > MAX_Q16_COORDINATE as u64
        {
            return Err(CoreError::InvalidArgument(
                "canonical stroke coordinate is outside bounds",
            ));
        }
        if bytes[18..CANONICAL_SAMPLE_BYTES] != [0; 6] {
            return Err(CoreError::InvalidArgument(
                "canonical stroke sample reserved bytes must be zero",
            ));
        }
        samples.push(CanonicalStrokeSample {
            x_q16,
            y_q16,
            pressure,
        });
    }
    Ok(samples)
}

/// Applies canonical procedure arguments to a caller-owned working document.
///
/// The executor is expected to publish the working document only after its
/// remaining state/history/procedure allocation checks have also succeeded.
pub(crate) fn apply(
    document: &mut CellDocument,
    arguments: &CanonicalStrokeArguments,
    revision: u64,
) -> Result<Vec<PixelChange>, CoreError> {
    let stroke = stroke_from_arguments(arguments)?;
    apply_canonical_raster_stroke(document, &stroke, revision)
}

/// Starts an incremental preview by applying the initial canonical batch once.
pub(crate) fn begin_preview(
    document: &mut CellDocument,
    arguments: &CanonicalStrokeArguments,
    revision: u64,
) -> Result<RasterStrokePreview, CoreError> {
    let stroke = stroke_from_arguments(arguments)?;
    let (desired, maximum_radius) = validated_stroke_context(document, &stroke)?;
    let (changes, work) = stage_canonical_raster_stroke(document, &stroke)?;
    apply_staged_changes(document, stroke.target_plane_id, &changes, revision)?;
    Ok(RasterStrokePreview {
        stroke,
        desired,
        maximum_radius,
        work,
    })
}

impl RasterStrokePreview {
    pub(crate) const fn target_plane_id(&self) -> u64 {
        self.stroke.target_plane_id.get()
    }

    /// Applies only the new bridge and batch-internal segments to the preview.
    ///
    /// Aggregate formula-3 work remains identical to a one-shot execution. If
    /// pressure raises the global clipping radius, the preview is rebuilt from
    /// its private base; radius has at most 129 discrete values for the bounded
    /// supported bounded diameter, so ordinary appends remain strictly incremental.
    pub(crate) fn append(
        &mut self,
        base_document: &CellDocument,
        document: &mut CellDocument,
        arguments: &CanonicalStrokeArguments,
        revision: u64,
    ) -> Result<(), CoreError> {
        let appended = stroke_from_arguments(arguments)?;
        if !same_stroke_settings(&self.stroke, &appended) {
            return Err(CoreError::InvalidArgument(
                "stroke append settings do not match the active transaction",
            ));
        }
        let old_len = self.stroke.samples.len();
        let next_len =
            old_len
                .checked_add(appended.samples.len())
                .ok_or(CoreError::InvalidArgument(
                    "stroke sample count is outside bounds",
                ))?;
        if next_len > MAX_STROKE_SAMPLES {
            return Err(CoreError::InvalidArgument(
                "stroke sample count is outside bounds",
            ));
        }
        self.stroke
            .samples
            .try_reserve(appended.samples.len())
            .map_err(|_| CoreError::InvalidState("stroke preview allocation failed"))?;
        self.stroke.samples.extend_from_slice(&appended.samples);

        let next_maximum_radius =
            appended
                .samples
                .iter()
                .try_fold(self.maximum_radius, |maximum, sample| {
                    dab_radius(&self.stroke, sample.pressure).map(|radius| maximum.max(radius))
                })?;
        if next_maximum_radius > self.maximum_radius {
            // The closed primitive clips every segment with the stroke-global
            // maximum radius. A later pressure increase can therefore shift the
            // Bresenham phase of an earlier off-canvas segment. Rebuild only on
            // one of the bounded radius transitions so preview pixels remain
            // exactly batching independent without returning to per-append O(N²).
            let (changes, work) = stage_canonical_raster_stroke(base_document, &self.stroke)?;
            let mut rebuilt = base_document.clone();
            apply_staged_changes(
                &mut rebuilt,
                self.stroke.target_plane_id,
                &changes,
                revision,
            )?;
            *document = rebuilt;
            self.maximum_radius = next_maximum_radius;
            self.work = work;
            return Ok(());
        }

        let mut staged = BTreeSet::new();
        let mut next_work = self
            .work
            .checked_add(u64::try_from(appended.samples.len()).map_err(|_| {
                CoreError::InvalidArgument("canonical stroke sample count is not representable")
            })?)
            .ok_or(CoreError::InvalidArgument(
                "stroke rasterization work overflows",
            ))?;
        stage_sample_windows(
            document,
            &self.stroke,
            &self.stroke.samples[old_len - 1..],
            next_maximum_radius,
            &mut staged,
            &mut next_work,
        )?;

        let changes =
            changes_from_staged(document, self.stroke.target_plane_id, self.desired, staged)?;
        apply_staged_changes(document, self.stroke.target_plane_id, &changes, revision)?;
        self.maximum_radius = next_maximum_radius;
        self.work = next_work;
        Ok(())
    }

    /// Closes the owned samples into the exact schema-1 inline payload.
    pub(crate) fn into_arguments(self) -> Result<CanonicalStrokeArguments, CoreError> {
        Ok(CanonicalStrokeArguments {
            target_plane_id: self.stroke.target_plane_id.get(),
            tool_code: tool_code(self.stroke.tool),
            color: self.stroke.color_rgba8,
            diameter_q16: self.stroke.diameter_q16,
            auto_erase: self.stroke.auto_erase,
            pressure_size: self.stroke.pressure_size,
            payload: encode_payload(&self.stroke.samples)?,
        })
    }

    pub(crate) fn canonical_payload_bytes(&self) -> u64 {
        let bytes = self
            .stroke
            .samples
            .len()
            .saturating_mul(CANONICAL_SAMPLE_BYTES)
            .saturating_add(CANONICAL_PAYLOAD_HEADER_BYTES);
        u64::try_from(bytes).unwrap_or(u64::MAX)
    }
}

fn stroke_from_arguments(
    arguments: &CanonicalStrokeArguments,
) -> Result<CanonicalRasterStroke, CoreError> {
    Ok(CanonicalRasterStroke {
        tool: tool_from_code(arguments.tool_code)?,
        target_plane_id: checked_plane_id(arguments.target_plane_id)?,
        color_rgba8: arguments.color,
        diameter_q16: arguments.diameter_q16,
        auto_erase: arguments.auto_erase,
        pressure_size: arguments.pressure_size,
        samples: decode_payload(&arguments.payload)?,
    })
}

fn same_stroke_settings(left: &CanonicalRasterStroke, right: &CanonicalRasterStroke) -> bool {
    left.tool == right.tool
        && left.target_plane_id == right.target_plane_id
        && left.color_rgba8 == right.color_rgba8
        && left.diameter_q16 == right.diameter_q16
        && left.auto_erase == right.auto_erase
        && left.pressure_size == right.pressure_size
}

pub(crate) fn encode_payload(samples: &[CanonicalStrokeSample]) -> Result<Vec<u8>, CoreError> {
    let count = u64::try_from(samples.len()).map_err(|_| {
        CoreError::InvalidArgument("canonical stroke sample count is not representable")
    })?;
    let capacity = samples
        .len()
        .checked_mul(CANONICAL_SAMPLE_BYTES)
        .and_then(|bytes| bytes.checked_add(CANONICAL_PAYLOAD_HEADER_BYTES))
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke payload length overflows",
        ))?;
    let mut payload = Vec::with_capacity(capacity);
    payload.extend_from_slice(&count.to_le_bytes());
    for sample in samples {
        payload.extend_from_slice(&sample.x_q16.to_le_bytes());
        payload.extend_from_slice(&sample.y_q16.to_le_bytes());
        payload.extend_from_slice(&sample.pressure.to_le_bytes());
        payload.extend_from_slice(&[0; 6]);
    }
    Ok(payload)
}

const fn tool_code(tool: PaintTool) -> u32 {
    match tool {
        PaintTool::Pencil => 1,
        PaintTool::Brush => 2,
        PaintTool::Eraser => 3,
    }
}

fn tool_from_code(code: u32) -> Result<PaintTool, CoreError> {
    match code {
        1 => Ok(PaintTool::Pencil),
        2 => Ok(PaintTool::Brush),
        3 => Ok(PaintTool::Eraser),
        _ => Err(CoreError::InvalidArgument(
            "canonical stroke tool code is unknown",
        )),
    }
}

fn checked_plane_id(raw: u64) -> Result<PlaneId, CoreError> {
    if raw == 0 {
        return Err(CoreError::InvalidArgument(
            "stroke target plane ID must be nonzero",
        ));
    }
    Ok(PlaneId::from_raw(raw))
}

#[cfg(test)]
fn canonical_stroke_from_public(
    stroke: &Stroke,
    view: &ViewState,
    document: &CellDocument,
) -> Result<CanonicalRasterStroke, CoreError> {
    let target_plane_id = document.plane_for_paint_role(stroke.plane)?.id;
    let arguments = canonicalize(
        stroke,
        view,
        document.width,
        document.height,
        target_plane_id.get(),
    )?;
    Ok(CanonicalRasterStroke {
        tool: tool_from_code(arguments.tool_code)?,
        target_plane_id,
        color_rgba8: arguments.color,
        diameter_q16: arguments.diameter_q16,
        auto_erase: arguments.auto_erase,
        pressure_size: arguments.pressure_size,
        samples: decode_payload(&arguments.payload)?,
    })
}

/// Canonicalizes a batch of public samples for an interactive stroke session.
fn canonicalize_stroke_samples(
    view: ViewState,
    coordinate_space: CoordinateSpace,
    samples: &[StrokeSample],
    width: u32,
    height: u32,
) -> Result<Vec<CanonicalStrokeSample>, CoreError> {
    if samples.len() > MAX_STROKE_SAMPLES {
        return Err(CoreError::InvalidArgument(
            "stroke sample count is outside bounds",
        ));
    }

    samples
        .iter()
        .map(|sample| {
            validate_public_sample(sample)?;
            let (x_q16, y_q16) = match coordinate_space {
                CoordinateSpace::Document => (
                    canonical_q16_from_f32(sample.x)?,
                    canonical_q16_from_f32(sample.y)?,
                ),
                CoordinateSpace::Device => {
                    let point = device_to_document(
                        view,
                        DocumentSizeU32::new(width, height),
                        DevicePointF64::new(f64::from(sample.x), f64::from(sample.y))?,
                    );
                    if !stroke_coordinate_is_supported(point.x)
                        || !stroke_coordinate_is_supported(point.y)
                    {
                        return Err(CoreError::InvalidArgument(
                            "device-to-document stroke coordinate is outside bounds",
                        ));
                    }
                    (
                        canonical_q16_from_f64(point.x)?,
                        canonical_q16_from_f64(point.y)?,
                    )
                }
            };
            Ok(CanonicalStrokeSample {
                x_q16,
                y_q16,
                pressure: canonical_pressure_from_f32(sample.pressure)?,
            })
        })
        .collect()
}

/// Stages the exact pixel delta without changing the supplied document.
fn stage_canonical_raster_stroke(
    document: &CellDocument,
    stroke: &CanonicalRasterStroke,
) -> Result<(Vec<PixelChange>, u64), CoreError> {
    let (desired, maximum_radius) = validated_stroke_context(document, stroke)?;
    let mut staged = BTreeSet::new();
    let mut work = u64::try_from(stroke.samples.len()).map_err(|_| {
        CoreError::InvalidArgument("canonical stroke sample count is not representable")
    })?;
    stage_segment(
        document,
        stroke,
        stroke.samples[0],
        stroke.samples[0],
        maximum_radius,
        &mut staged,
        &mut work,
    )?;
    stage_sample_windows(
        document,
        stroke,
        &stroke.samples,
        maximum_radius,
        &mut staged,
        &mut work,
    )?;
    let changes = changes_from_staged(document, stroke.target_plane_id, desired, staged)?;
    Ok((changes, work))
}

fn validated_stroke_context(
    document: &CellDocument,
    stroke: &CanonicalRasterStroke,
) -> Result<(PixelValue, i64), CoreError> {
    if stroke.samples.is_empty() || stroke.samples.len() > MAX_STROKE_SAMPLES {
        return Err(CoreError::InvalidArgument(
            "stroke sample count is outside bounds",
        ));
    }
    if !(1..=i64::from(256) * Q16_ONE).contains(&stroke.diameter_q16) {
        return Err(CoreError::InvalidArgument(
            "canonical stroke diameter is outside bounds",
        ));
    }
    ensure_editable_plane(document, stroke.target_plane_id)?;
    let plane = document
        .plane_by_id(stroke.target_plane_id)
        .ok_or(CoreError::InvalidState(
            "stroke target plane no longer exists",
        ))?;
    let (draw_value, erase_value) =
        target_values(plane.kind, plane.raster.format(), stroke.color_rgba8)?;
    let desired = desired_value(document, stroke, draw_value, erase_value)?;
    let maximum_radius = stroke.samples.iter().try_fold(0_i64, |maximum, sample| {
        dab_radius(stroke, sample.pressure).map(|radius| maximum.max(radius))
    })?;
    Ok((desired, maximum_radius))
}

fn stage_sample_windows(
    document: &CellDocument,
    stroke: &CanonicalRasterStroke,
    samples: &[CanonicalStrokeSample],
    maximum_radius: i64,
    staged: &mut BTreeSet<(u32, u32)>,
    work: &mut u64,
) -> Result<(), CoreError> {
    for samples in samples.windows(2) {
        stage_segment(
            document,
            stroke,
            samples[0],
            samples[1],
            maximum_radius,
            staged,
            work,
        )?;
    }
    Ok(())
}

fn changes_from_staged(
    document: &CellDocument,
    target_plane_id: PlaneId,
    desired: PixelValue,
    staged: BTreeSet<(u32, u32)>,
) -> Result<Vec<PixelChange>, CoreError> {
    let plane = document
        .plane_by_id(target_plane_id)
        .ok_or(CoreError::InvalidState(
            "stroke target plane no longer exists",
        ))?;
    let raster = &plane.raster;
    let mut changes = Vec::with_capacity(staged.len());
    for (x, y) in staged {
        let before = raster.pixel(x, y)?;
        if before != desired {
            changes.push(PixelChange {
                x,
                y,
                before,
                after: desired,
            });
        }
    }
    Ok(changes)
}

/// Applies a fully staged canonical stroke to a private working document.
///
/// Callers must publish this working document only after all transaction checks
/// and history/procedure allocation have also succeeded.
fn apply_canonical_raster_stroke(
    document: &mut CellDocument,
    stroke: &CanonicalRasterStroke,
    revision: u64,
) -> Result<Vec<PixelChange>, CoreError> {
    let (changes, _) = stage_canonical_raster_stroke(document, stroke)?;
    apply_staged_changes(document, stroke.target_plane_id, &changes, revision)?;
    Ok(changes)
}

fn apply_staged_changes(
    document: &mut CellDocument,
    target_plane_id: PlaneId,
    changes: &[PixelChange],
    revision: u64,
) -> Result<(), CoreError> {
    if changes.is_empty() {
        return Ok(());
    }
    let raster = &mut document
        .plane_by_id_mut(target_plane_id)
        .ok_or(CoreError::InvalidState(
            "stroke target plane no longer exists",
        ))?
        .raster;
    let mut touched_tiles = BTreeSet::new();
    for change in changes {
        raster.set_pixel(change.x, change.y, change.after, revision)?;
        touched_tiles.insert(TileCoord {
            x: change.x / TILE_SIZE,
            y: change.y / TILE_SIZE,
        });
    }
    for coordinate in touched_tiles {
        raster.remove_tile_if_empty(coordinate);
    }
    document.active_plane_id = target_plane_id;
    Ok(())
}

pub(crate) fn validate_public_stroke(stroke: &Stroke) -> Result<(), CoreError> {
    if stroke.samples.is_empty() || stroke.samples.len() > MAX_STROKE_SAMPLES {
        return Err(CoreError::InvalidArgument(
            "stroke sample count is outside bounds",
        ));
    }
    if !stroke.diameter.is_finite()
        || stroke.diameter <= 0.0
        || stroke.diameter > MAX_BRUSH_DIAMETER
    {
        return Err(CoreError::InvalidArgument(
            "stroke diameter is outside bounds",
        ));
    }
    for sample in &stroke.samples {
        validate_public_sample(sample)?;
    }
    Ok(())
}

fn validate_public_sample(sample: &StrokeSample) -> Result<(), CoreError> {
    if !sample.x.is_finite()
        || !sample.y.is_finite()
        || sample.x.abs() > MAX_STROKE_COORDINATE
        || sample.y.abs() > MAX_STROKE_COORDINATE
        || !sample.pressure.is_finite()
        || !(0.0..=1.0).contains(&sample.pressure)
    {
        return Err(CoreError::InvalidArgument(
            "stroke sample contains invalid values",
        ));
    }
    Ok(())
}

fn target_values(
    kind: PlaneType,
    format: PixelFormat,
    color: [u8; 4],
) -> Result<(PixelValue, PixelValue), CoreError> {
    match (kind, format) {
        (PlaneType::MainLine, PixelFormat::BinaryMask8) => {
            Ok((PixelValue::Binary(u8::MAX), PixelValue::Binary(0)))
        }
        (PlaneType::MainLine, PixelFormat::Grayscale8) => {
            Ok((PixelValue::Grayscale8(u8::MAX), PixelValue::Grayscale8(0)))
        }
        (PlaneType::MainLine, PixelFormat::Grayscale16) => Ok((
            PixelValue::Grayscale16(u16::MAX),
            PixelValue::Grayscale16(0),
        )),
        (PlaneType::Color | PlaneType::Raster, PixelFormat::StraightRgba8) => {
            Ok((PixelValue::Rgba(color), PixelValue::Rgba([0; 4])))
        }
        (PlaneType::Color | PlaneType::Raster, PixelFormat::StraightRgba16) => Ok((
            PixelValue::Rgba16(color.map(|channel| u16::from(channel) * 257)),
            PixelValue::Rgba16([0; 4]),
        )),
        _ => Err(CoreError::InvalidState(
            "stroke target plane does not support raster painting",
        )),
    }
}

fn desired_value(
    document: &CellDocument,
    stroke: &CanonicalRasterStroke,
    draw_value: PixelValue,
    erase_value: PixelValue,
) -> Result<PixelValue, CoreError> {
    if stroke.tool == PaintTool::Eraser {
        return Ok(erase_value);
    }
    if stroke.tool != PaintTool::Pencil || !stroke.auto_erase {
        return Ok(draw_value);
    }

    let first = stroke.samples[0];
    let x = q16_floor(first.x_q16);
    let y = q16_floor(first.y_q16);
    if x >= 0 && y >= 0 && x < i64::from(document.width) && y < i64::from(document.height) {
        let raster = &document
            .plane_by_id(stroke.target_plane_id)
            .ok_or(CoreError::InvalidState(
                "stroke target plane no longer exists",
            ))?
            .raster;
        if raster.pixel(x as u32, y as u32)? == draw_value {
            return Ok(erase_value);
        }
    }
    Ok(draw_value)
}

#[allow(clippy::too_many_arguments)]
fn stage_segment(
    document: &CellDocument,
    stroke: &CanonicalRasterStroke,
    start: CanonicalStrokeSample,
    end: CanonicalStrokeSample,
    maximum_radius: i64,
    staged: &mut BTreeSet<(u32, u32)>,
    work: &mut u64,
) -> Result<(), CoreError> {
    let Some((start, end)) = clip_segment(document, start, end, maximum_radius)? else {
        return Ok(());
    };
    let mut x = q16_floor(start.x_q16);
    let mut y = q16_floor(start.y_q16);
    let end_x = q16_floor(end.x_q16);
    let end_y = q16_floor(end.y_q16);
    let dx = end_x
        .checked_sub(x)
        .and_then(i64::checked_abs)
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke x distance overflows",
        ))?;
    let sx = if x < end_x { 1 } else { -1 };
    let dy = end_y
        .checked_sub(y)
        .and_then(i64::checked_abs)
        .and_then(i64::checked_neg)
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke y distance overflows",
        ))?;
    let sy = if y < end_y { 1 } else { -1 };
    let mut error = dx.checked_add(dy).ok_or(CoreError::InvalidArgument(
        "canonical stroke recurrence overflows",
    ))?;
    let steps = dx.max(-dy).max(1);
    let mut step = 0_i64;

    loop {
        let pressure_numerator = i128::from(start.pressure)
            .checked_mul(i128::from(steps - step))
            .and_then(|value| {
                i128::from(end.pressure)
                    .checked_mul(i128::from(step))
                    .and_then(|end_value| value.checked_add(end_value))
            })
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke pressure interpolation overflows",
            ))?;
        let pressure = divide_round_ties_even(pressure_numerator, i128::from(steps))?;
        let pressure = u16::try_from(pressure).map_err(|_| {
            CoreError::InvalidArgument("canonical stroke pressure is outside bounds")
        })?;
        stage_dab(document, stroke, x, y, pressure, staged, work)?;
        if x == end_x && y == end_y {
            break;
        }
        let double_error = error.checked_mul(2).ok_or(CoreError::InvalidArgument(
            "canonical stroke recurrence overflows",
        ))?;
        if double_error >= dy {
            error = error.checked_add(dy).ok_or(CoreError::InvalidArgument(
                "canonical stroke recurrence overflows",
            ))?;
            x = x.checked_add(sx).ok_or(CoreError::InvalidArgument(
                "canonical stroke x coordinate overflows",
            ))?;
        }
        if double_error <= dx {
            error = error.checked_add(dx).ok_or(CoreError::InvalidArgument(
                "canonical stroke recurrence overflows",
            ))?;
            y = y.checked_add(sy).ok_or(CoreError::InvalidArgument(
                "canonical stroke y coordinate overflows",
            ))?;
        }
        step = step.checked_add(1).ok_or(CoreError::InvalidArgument(
            "canonical stroke step count overflows",
        ))?;
    }
    Ok(())
}

fn stage_dab(
    document: &CellDocument,
    stroke: &CanonicalRasterStroke,
    center_x: i64,
    center_y: i64,
    pressure: u16,
    staged: &mut BTreeSet<(u32, u32)>,
    work: &mut u64,
) -> Result<(), CoreError> {
    let radius = dab_radius(stroke, pressure)?;
    let edge = i128::from(radius)
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke radius is not representable",
        ))?;
    let dab_work = edge
        .checked_mul(edge)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(CoreError::InvalidArgument(
            "stroke rasterization work overflows",
        ))?;
    let next_work = work
        .checked_add(dab_work)
        .ok_or(CoreError::InvalidArgument(
            "stroke rasterization work overflows",
        ))?;
    if next_work > MAX_STROKE_WORK {
        return Err(CoreError::InvalidArgument(
            "stroke rasterization work exceeds the bounded limit",
        ));
    }
    *work = next_work;

    let radius_squared =
        i128::from(radius)
            .checked_mul(i128::from(radius))
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke radius square overflows",
            ))?;
    for offset_y in -radius..=radius {
        for offset_x in -radius..=radius {
            let distance_squared = i128::from(offset_x)
                .checked_mul(i128::from(offset_x))
                .and_then(|x| {
                    i128::from(offset_y)
                        .checked_mul(i128::from(offset_y))
                        .and_then(|y| x.checked_add(y))
                })
                .ok_or(CoreError::InvalidArgument(
                    "canonical stroke dab distance overflows",
                ))?;
            if distance_squared > radius_squared {
                continue;
            }
            let x = center_x
                .checked_add(offset_x)
                .ok_or(CoreError::InvalidArgument(
                    "canonical stroke x coordinate overflows",
                ))?;
            let y = center_y
                .checked_add(offset_y)
                .ok_or(CoreError::InvalidArgument(
                    "canonical stroke y coordinate overflows",
                ))?;
            if x >= 0 && y >= 0 && x < i64::from(document.width) && y < i64::from(document.height) {
                staged.insert((x as u32, y as u32));
            }
        }
    }
    Ok(())
}

fn dab_radius(stroke: &CanonicalRasterStroke, pressure: u16) -> Result<i64, CoreError> {
    if stroke.tool == PaintTool::Pencil {
        return Ok(0);
    }
    let pressure = if stroke.pressure_size {
        pressure.max(PRESSURE_MINIMUM_SIZE)
    } else {
        PRESSURE_MAX
    };
    let scaled = i128::from(stroke.diameter_q16)
        .checked_mul(i128::from(pressure))
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke diameter scaling overflows",
        ))?;
    let scaled = divide_round_ties_even(scaled, i128::from(PRESSURE_MAX))?;
    if scaled <= i128::from(Q16_ONE) {
        return Ok(0);
    }
    let numerator = scaled - i128::from(Q16_ONE);
    let denominator = i128::from(Q16_ONE) * 2;
    let radius = numerator
        .checked_add(denominator - 1)
        .and_then(|value| value.checked_div(denominator))
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke radius overflows",
        ))?;
    i64::try_from(radius)
        .map_err(|_| CoreError::InvalidArgument("canonical stroke radius is not representable"))
}

fn clip_segment(
    document: &CellDocument,
    start: CanonicalStrokeSample,
    end: CanonicalStrokeSample,
    radius: i64,
) -> Result<Option<(CanonicalStrokeSample, CanonicalStrokeSample)>, CoreError> {
    let minimum_x =
        i128::from(radius)
            .checked_mul(-i128::from(Q16_ONE))
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke clip bound overflows",
            ))?;
    let minimum_y = minimum_x;
    let maximum_x = i128::from(document.width)
        .checked_add(i128::from(radius))
        .and_then(|value| value.checked_mul(i128::from(Q16_ONE)))
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke clip bound overflows",
        ))?;
    let maximum_y = i128::from(document.height)
        .checked_add(i128::from(radius))
        .and_then(|value| value.checked_mul(i128::from(Q16_ONE)))
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke clip bound overflows",
        ))?;
    let start_x = i128::from(start.x_q16);
    let start_y = i128::from(start.y_q16);
    let delta_x = i128::from(end.x_q16)
        .checked_sub(start_x)
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke clip delta overflows",
        ))?;
    let delta_y = i128::from(end.y_q16)
        .checked_sub(start_y)
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke clip delta overflows",
        ))?;
    let negative_delta_x = delta_x.checked_neg().ok_or(CoreError::InvalidArgument(
        "canonical stroke clip delta overflows",
    ))?;
    let negative_delta_y = delta_y.checked_neg().ok_or(CoreError::InvalidArgument(
        "canonical stroke clip delta overflows",
    ))?;
    let distance_from_minimum_x =
        start_x
            .checked_sub(minimum_x)
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke clip distance overflows",
            ))?;
    let distance_from_maximum_x =
        maximum_x
            .checked_sub(start_x)
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke clip distance overflows",
            ))?;
    let distance_from_minimum_y =
        start_y
            .checked_sub(minimum_y)
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke clip distance overflows",
            ))?;
    let distance_from_maximum_y =
        maximum_y
            .checked_sub(start_y)
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke clip distance overflows",
            ))?;
    let mut lower = Rational::ZERO;
    let mut upper = Rational::ONE;
    for (coefficient, distance) in [
        (negative_delta_x, distance_from_minimum_x),
        (delta_x, distance_from_maximum_x),
        (negative_delta_y, distance_from_minimum_y),
        (delta_y, distance_from_maximum_y),
    ] {
        if coefficient == 0 {
            if distance < 0 {
                return Ok(None);
            }
            continue;
        }
        let ratio = Rational::new(distance, coefficient)?;
        if coefficient < 0 {
            if ratio.compare(upper)? == Ordering::Greater {
                return Ok(None);
            }
            if ratio.compare(lower)? == Ordering::Greater {
                lower = ratio;
            }
        } else {
            if ratio.compare(lower)? == Ordering::Less {
                return Ok(None);
            }
            if ratio.compare(upper)? == Ordering::Less {
                upper = ratio;
            }
        }
    }

    Ok(Some((
        interpolate_sample(start, end, lower)?,
        interpolate_sample(start, end, upper)?,
    )))
}

fn interpolate_sample(
    start: CanonicalStrokeSample,
    end: CanonicalStrokeSample,
    ratio: Rational,
) -> Result<CanonicalStrokeSample, CoreError> {
    Ok(CanonicalStrokeSample {
        x_q16: interpolate_i64(start.x_q16, end.x_q16, ratio)?,
        y_q16: interpolate_i64(start.y_q16, end.y_q16, ratio)?,
        pressure: u16::try_from(interpolate_i64(
            i64::from(start.pressure),
            i64::from(end.pressure),
            ratio,
        )?)
        .map_err(|_| CoreError::InvalidArgument("canonical stroke pressure is outside bounds"))?,
    })
}

fn interpolate_i64(start: i64, end: i64, ratio: Rational) -> Result<i64, CoreError> {
    let delta =
        i128::from(end)
            .checked_sub(i128::from(start))
            .ok_or(CoreError::InvalidArgument(
                "canonical stroke interpolation delta overflows",
            ))?;
    let numerator = i128::from(start)
        .checked_mul(ratio.denominator)
        .and_then(|value| {
            delta
                .checked_mul(ratio.numerator)
                .and_then(|scaled| value.checked_add(scaled))
        })
        .ok_or(CoreError::InvalidArgument(
            "canonical stroke interpolation overflows",
        ))?;
    let value = divide_round_ties_even(numerator, ratio.denominator)?;
    i64::try_from(value).map_err(|_| {
        CoreError::InvalidArgument("canonical stroke interpolation is not representable")
    })
}

#[derive(Clone, Copy, Debug)]
struct Rational {
    numerator: i128,
    denominator: i128,
}

impl Rational {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    fn new(mut numerator: i128, mut denominator: i128) -> Result<Self, CoreError> {
        if denominator == 0 {
            return Err(CoreError::InvalidArgument(
                "canonical stroke clip ratio has zero denominator",
            ));
        }
        if denominator < 0 {
            numerator = numerator.checked_neg().ok_or(CoreError::InvalidArgument(
                "canonical stroke clip ratio overflows",
            ))?;
            denominator = denominator.checked_neg().ok_or(CoreError::InvalidArgument(
                "canonical stroke clip ratio overflows",
            ))?;
        }
        let divisor = greatest_common_divisor(numerator.unsigned_abs(), denominator as u128);
        let divisor = i128::try_from(divisor).map_err(|_| {
            CoreError::InvalidArgument("canonical stroke clip ratio is not representable")
        })?;
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    fn compare(self, other: Self) -> Result<Ordering, CoreError> {
        let left =
            self.numerator
                .checked_mul(other.denominator)
                .ok_or(CoreError::InvalidArgument(
                    "canonical stroke ratio comparison overflows",
                ))?;
        let right =
            other
                .numerator
                .checked_mul(self.denominator)
                .ok_or(CoreError::InvalidArgument(
                    "canonical stroke ratio comparison overflows",
                ))?;
        Ok(left.cmp(&right))
    }
}

fn greatest_common_divisor(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

fn canonical_q16_from_f32(value: f32) -> Result<i64, CoreError> {
    let (negative, significand, exponent) = decompose_f32(value)?;
    let scaled = round_binary_integer(negative, significand, exponent + 16)?;
    let scaled = i64::try_from(scaled).map_err(|_| {
        CoreError::InvalidArgument("canonical document coordinate is not representable")
    })?;
    if scaled.abs() > MAX_Q16_COORDINATE {
        return Err(CoreError::InvalidArgument(
            "canonical document coordinate is outside bounds",
        ));
    }
    Ok(scaled)
}

fn canonical_q16_from_f64(value: f64) -> Result<i64, CoreError> {
    let (negative, significand, exponent) = decompose_f64(value)?;
    let scaled = round_binary_integer(negative, significand, exponent + 16)?;
    let scaled = i64::try_from(scaled).map_err(|_| {
        CoreError::InvalidArgument("canonical document coordinate is not representable")
    })?;
    if scaled.abs() > MAX_Q16_COORDINATE {
        return Err(CoreError::InvalidArgument(
            "canonical document coordinate is outside bounds",
        ));
    }
    Ok(scaled)
}

fn canonical_pressure_from_f32(value: f32) -> Result<u16, CoreError> {
    let (negative, significand, exponent) = decompose_f32(value)?;
    if negative && significand != 0 {
        return Err(CoreError::InvalidArgument(
            "stroke pressure is outside bounds",
        ));
    }
    let numerator =
        significand
            .checked_mul(u128::from(PRESSURE_MAX))
            .ok_or(CoreError::InvalidArgument(
                "canonical pressure conversion overflows",
            ))?;
    let result = round_unsigned_binary_integer(numerator, exponent)?;
    u16::try_from(result)
        .map_err(|_| CoreError::InvalidArgument("stroke pressure is outside bounds"))
}

fn decompose_f32(value: f32) -> Result<(bool, u128, i32), CoreError> {
    let bits = value.to_bits();
    let negative = bits >> 31 != 0;
    let exponent = (bits >> 23) & 0xff;
    let fraction = bits & 0x7f_ffff;
    if exponent == 0xff {
        return Err(CoreError::InvalidArgument(
            "canonical binary32 input is not finite",
        ));
    }
    if exponent == 0 {
        Ok((negative && fraction != 0, u128::from(fraction), -149))
    } else {
        Ok((
            negative,
            u128::from((1_u32 << 23) | fraction),
            exponent as i32 - 127 - 23,
        ))
    }
}

fn decompose_f64(value: f64) -> Result<(bool, u128, i32), CoreError> {
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent = (bits >> 52) & 0x7ff;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    if exponent == 0x7ff {
        return Err(CoreError::InvalidArgument(
            "canonical binary64 input is not finite",
        ));
    }
    if exponent == 0 {
        Ok((negative && fraction != 0, u128::from(fraction), -1074))
    } else {
        Ok((
            negative,
            u128::from((1_u64 << 52) | fraction),
            exponent as i32 - 1023 - 52,
        ))
    }
}

fn round_binary_integer(
    negative: bool,
    significand: u128,
    exponent: i32,
) -> Result<i128, CoreError> {
    let magnitude = round_unsigned_binary_integer(significand, exponent)?;
    let magnitude = i128::try_from(magnitude).map_err(|_| {
        CoreError::InvalidArgument("canonical binary conversion is not representable")
    })?;
    if negative && magnitude != 0 {
        magnitude.checked_neg().ok_or(CoreError::InvalidArgument(
            "canonical binary conversion overflows",
        ))
    } else {
        Ok(magnitude)
    }
}

fn round_unsigned_binary_integer(significand: u128, exponent: i32) -> Result<u128, CoreError> {
    if significand == 0 {
        return Ok(0);
    }
    if exponent >= 0 {
        return significand
            .checked_shl(exponent as u32)
            .ok_or(CoreError::InvalidArgument(
                "canonical binary conversion overflows",
            ));
    }
    let shift = exponent.unsigned_abs();
    if shift >= 128 {
        return Ok(0);
    }
    let quotient = significand >> shift;
    let mask = (1_u128 << shift) - 1;
    let remainder = significand & mask;
    let half = 1_u128 << (shift - 1);
    if remainder > half || (remainder == half && quotient & 1 != 0) {
        quotient.checked_add(1).ok_or(CoreError::InvalidArgument(
            "canonical binary rounding overflows",
        ))
    } else {
        Ok(quotient)
    }
}

fn divide_round_ties_even(numerator: i128, denominator: i128) -> Result<i128, CoreError> {
    if denominator <= 0 {
        return Err(CoreError::InvalidArgument(
            "canonical division denominator is not positive",
        ));
    }
    let quotient = numerator.div_euclid(denominator);
    let remainder = numerator.rem_euclid(denominator);
    let doubled = remainder.checked_mul(2).ok_or(CoreError::InvalidArgument(
        "canonical division remainder overflows",
    ))?;
    if doubled > denominator || (doubled == denominator && quotient & 1 != 0) {
        quotient.checked_add(1).ok_or(CoreError::InvalidArgument(
            "canonical division rounding overflows",
        ))
    } else {
        Ok(quotient)
    }
}

const fn q16_floor(value: i64) -> i64 {
    value.div_euclid(Q16_ONE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivePlane, Core, DEFAULT_DPI_MILLI};

    fn pencil(samples: Vec<StrokeSample>) -> Stroke {
        Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::MainLine,
            color: [0, 0, 0, 255],
            diameter: 1.0,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples,
        }
    }

    #[test]
    fn binary_scalars_use_exact_ties_to_even() {
        assert_eq!(canonical_q16_from_f32(0.5 / 65_536.0).unwrap(), 0);
        assert_eq!(canonical_q16_from_f32(1.5 / 65_536.0).unwrap(), 2);
        assert_eq!(canonical_q16_from_f32(-1.5 / 65_536.0).unwrap(), -2);
        assert_eq!(canonical_q16_from_f32(-0.0).unwrap(), 0);
        assert_eq!(canonical_q16_from_f64(0.5 / 65_536.0).unwrap(), 0);
        assert_eq!(canonical_q16_from_f64(1.5 / 65_536.0).unwrap(), 2);
        assert_eq!(canonical_pressure_from_f32(-0.0).unwrap(), 0);
        assert_eq!(canonical_pressure_from_f32(0.5).unwrap(), 32_768);
        assert_eq!(canonical_pressure_from_f32(1.0).unwrap(), 65_535);
        assert!(canonical_q16_from_f32(f32::INFINITY).is_err());
        assert!(canonical_pressure_from_f32(f32::NAN).is_err());
    }

    #[test]
    fn canonical_payload_is_fixed_width_and_rejects_noncanonical_bytes() {
        let samples = vec![
            CanonicalStrokeSample {
                x_q16: -MAX_Q16_COORDINATE,
                y_q16: 0,
                pressure: 0,
            },
            CanonicalStrokeSample {
                x_q16: MAX_Q16_COORDINATE,
                y_q16: -1,
                pressure: u16::MAX,
            },
        ];
        let payload = encode_payload(&samples).unwrap();
        assert_eq!(payload.len(), 8 + samples.len() * 24);
        assert_eq!(decode_payload(&payload).unwrap(), samples);

        let mut nonzero_reserved = payload.clone();
        nonzero_reserved[8 + 18] = 1;
        assert!(decode_payload(&nonzero_reserved).is_err());

        let mut trailing = payload;
        trailing.push(0);
        assert!(decode_payload(&trailing).is_err());
        assert!(decode_payload(&0_u64.to_le_bytes()).is_err());
    }

    #[test]
    fn integer_dab_radius_obeys_pressure_and_ceil_contract() {
        let mut stroke = CanonicalRasterStroke {
            tool: PaintTool::Brush,
            target_plane_id: PlaneId::from_raw(1),
            color_rgba8: [0; 4],
            diameter_q16: 3 * Q16_ONE,
            auto_erase: false,
            pressure_size: true,
            samples: Vec::new(),
        };
        assert_eq!(dab_radius(&stroke, 0).unwrap(), 0);
        assert_eq!(dab_radius(&stroke, u16::MAX).unwrap(), 1);

        stroke.diameter_q16 = 256 * Q16_ONE;
        stroke.pressure_size = false;
        assert_eq!(dab_radius(&stroke, 0).unwrap(), 128);

        stroke.tool = PaintTool::Pencil;
        assert_eq!(dab_radius(&stroke, u16::MAX).unwrap(), 0);
    }

    #[test]
    fn clipped_integer_stroke_reaches_both_document_edges() {
        let mut core = Core::new();
        core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let stroke = pencil(vec![
            StrokeSample {
                x: -10_000_000.0,
                y: 32.0,
                pressure: 1.0,
            },
            StrokeSample {
                x: 10_000_000.0,
                y: 32.0,
                pressure: 1.0,
            },
        ]);
        let canonical =
            canonical_stroke_from_public(&stroke, &core.view, core.document.as_ref().unwrap())
                .unwrap();
        let mut working = core.document.as_ref().unwrap().clone();
        let result = apply_canonical_raster_stroke(&mut working, &canonical, 2).unwrap();
        assert!(!result.is_empty());
        assert_eq!(
            working.raster(ActivePlane::MainLine).pixel(0, 32).unwrap(),
            PixelValue::Binary(255)
        );
        assert_eq!(
            working.raster(ActivePlane::MainLine).pixel(63, 32).unwrap(),
            PixelValue::Binary(255)
        );
    }

    #[test]
    fn formula_three_counts_samples_clipping_and_repeated_endpoints() {
        let mut core = Core::new();
        core.new_cell(64, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let document = core.document.as_ref().unwrap();

        let single = canonical_stroke_from_public(
            &pencil(vec![StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }]),
            &core.view,
            document,
        )
        .unwrap();
        let (changes, work) = stage_canonical_raster_stroke(document, &single).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(work, 2, "one sample plus its one pencil dab");

        let adjacent = canonical_stroke_from_public(
            &pencil(vec![
                StrokeSample {
                    x: 1.0,
                    y: 1.0,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 2.0,
                    y: 1.0,
                    pressure: 1.0,
                },
            ]),
            &core.view,
            document,
        )
        .unwrap();
        let (changes, work) = stage_canonical_raster_stroke(document, &adjacent).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(
            work, 5,
            "two samples plus the initial dab and both segment endpoints"
        );

        let clipped = canonical_stroke_from_public(
            &pencil(vec![
                StrokeSample {
                    x: -10_000_000.0,
                    y: 32.0,
                    pressure: 1.0,
                },
                StrokeSample {
                    x: 10_000_000.0,
                    y: 32.0,
                    pressure: 1.0,
                },
            ]),
            &core.view,
            document,
        )
        .unwrap();
        let (changes, work) = stage_canonical_raster_stroke(document, &clipped).unwrap();
        assert_eq!(changes.len(), 64);
        assert_eq!(
            work, 67,
            "two samples plus 65 clipped candidates, including x=width"
        );
    }

    #[test]
    fn formula_three_live_limit_accepts_exact_boundary_and_rejects_next_dab() {
        let mut core = Core::new();
        core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let document = core.document.as_ref().unwrap();
        let stroke = canonical_stroke_from_public(
            &pencil(vec![StrokeSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }]),
            &core.view,
            document,
        )
        .unwrap();
        let mut staged = BTreeSet::new();
        let mut work = MAX_STROKE_WORK - 1;

        stage_dab(document, &stroke, 1, 1, u16::MAX, &mut staged, &mut work).unwrap();
        assert_eq!(work, MAX_STROKE_WORK);
        let staged_at_limit = staged.clone();

        assert!(stage_dab(document, &stroke, 1, 1, u16::MAX, &mut staged, &mut work,).is_err());
        assert_eq!(work, MAX_STROKE_WORK);
        assert_eq!(staged, staged_at_limit);
    }

    #[test]
    fn canonical_auto_erase_samples_the_first_cell_before_staging() {
        let mut core = Core::new();
        core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let stroke = pencil(vec![StrokeSample {
            x: 3.0,
            y: 4.0,
            pressure: 1.0,
        }]);
        let canonical =
            canonical_stroke_from_public(&stroke, &core.view, core.document.as_ref().unwrap())
                .unwrap();
        let view = core.view;
        let document = core.document.as_mut().unwrap();
        apply_canonical_raster_stroke(document, &canonical, 2).unwrap();

        let mut erase = stroke;
        erase.auto_erase = true;
        let canonical = canonical_stroke_from_public(&erase, &view, document).unwrap();
        apply_canonical_raster_stroke(document, &canonical, 3).unwrap();
        assert_eq!(
            document.raster(ActivePlane::MainLine).pixel(3, 4).unwrap(),
            PixelValue::Binary(0)
        );
    }
}
