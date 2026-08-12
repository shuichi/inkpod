//! Editable Text and instruction-annotation document objects.

use super::*;
use crate::primitive::CanonicalInvocation;

/// Maximum UTF-8 byte length of one annotation text payload.
pub const MAX_ANNOTATION_TEXT_BYTES: usize = 65_536;
/// Maximum UTF-8 byte length of one font-family hint.
pub const MAX_ANNOTATION_FONT_FAMILY_BYTES: usize = 1_024;
/// Maximum number of points in one stroke or leader object.
pub const MAX_ANNOTATION_POINTS: usize = 65_536;
/// Maximum number of persistent annotation objects in one document.
pub const MAX_ANNOTATION_OBJECTS: usize = 16_384;
/// Maximum number of edits committed by one annotation transaction.
pub const MAX_ANNOTATION_BATCH_EDITS: usize = 4_096;
/// Bold font-style flag.
pub const ANNOTATION_STYLE_BOLD: u32 = 1 << 0;
/// Italic font-style flag.
pub const ANNOTATION_STYLE_ITALIC: u32 = 1 << 1;
/// Underline font-style flag.
pub const ANNOTATION_STYLE_UNDERLINE: u32 = 1 << 2;
const ANNOTATION_STYLE_MASK: u32 =
    ANNOTATION_STYLE_BOLD | ANNOTATION_STYLE_ITALIC | ANNOTATION_STYLE_UNDERLINE;

/// Semantic kind of one editable annotation object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationKind {
    /// Re-editable UTF-8 text within a persisted logical layout rectangle.
    Text,
    /// A freehand document-coordinate polyline.
    Stroke,
    /// A two-point instruction leader.
    Leader,
    /// A text value with a two-point leader.
    Value,
}

/// Whether an annotation participates in ordinary flattened output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationOutput {
    /// Included in Canvas, thumbnails, and ordinary flattened output.
    Normal,
    /// Visible while editing but excluded from ordinary flattened output.
    Instruction,
}

/// One point in thousandths of a document pixel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnnotationPoint {
    /// Horizontal document coordinate in milli-pixels.
    pub x_milli: i32,
    /// Vertical document coordinate in milli-pixels.
    pub y_milli: i32,
}

/// Caller-owned value used to create or replace one annotation object.
///
/// Text and Value objects require non-empty bounded UTF-8 text and positive
/// logical bounds. Stroke objects require at least two points; Leader and Value
/// require exactly two points. Every coordinate must remain within the document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnotationObjectInput {
    /// Stable target layer ID. The layer kind must be Text or Annotation.
    pub layer_id: u64,
    /// Semantic object kind.
    pub kind: AnnotationKind,
    /// Ordinary-output participation.
    pub output: AnnotationOutput,
    /// Persisted logical text/layout or geometry bounds in document pixels.
    pub bounds: RectI32,
    /// Preferred font family. Empty requests the platform system UI font.
    pub font_family_hint: String,
    /// Font size in thousandths of a document pixel; zero for non-text objects.
    pub font_size_milli: u32,
    /// Bitwise combination of `ANNOTATION_STYLE_*` flags.
    pub style_flags: u32,
    /// Straight-alpha sRGB RGBA8 or RGBA16 color.
    pub color: PixelValue,
    /// UTF-8 text for Text and Value; empty for Stroke and Leader.
    pub text: String,
    /// Bounded document-coordinate geometry for Stroke, Leader, and Value.
    pub points: Vec<AnnotationPoint>,
    /// Geometry width in milli-pixels; zero for Text.
    pub stroke_width_milli: u32,
}

/// One operation in an atomic multi-object annotation edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnnotationEdit {
    /// Creates an object and allocates its stable ID at commit time.
    Create(AnnotationObjectInput),
    /// Replaces one existing object without changing its stable ID.
    Update {
        /// Stable object ID.
        object_id: u64,
        /// Complete replacement value.
        input: AnnotationObjectInput,
    },
    /// Moves bounds and geometry by an integral document-pixel offset.
    Move {
        /// Stable object ID.
        object_id: u64,
        /// Horizontal offset in document pixels.
        delta_x: i32,
        /// Vertical offset in document pixels.
        delta_y: i32,
    },
    /// Deletes one existing object.
    Delete {
        /// Stable object ID.
        object_id: u64,
    },
}

/// Immutable public information for one persistent annotation object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnotationObjectInfo {
    /// Stable object ID, unique in the document namespace and never reused.
    pub id: u64,
    /// Stable owning layer ID.
    pub layer_id: u64,
    /// Semantic object kind.
    pub kind: AnnotationKind,
    /// Ordinary-output participation.
    pub output: AnnotationOutput,
    /// Persisted logical bounds in document pixels.
    pub bounds: RectI32,
    /// Preferred font family hint.
    pub font_family_hint: String,
    /// Font size in milli-pixels.
    pub font_size_milli: u32,
    /// Bitwise combination of `ANNOTATION_STYLE_*` flags.
    pub style_flags: u32,
    /// Straight-alpha sRGB color.
    pub color: PixelValue,
    /// UTF-8 text payload.
    pub text: String,
    /// Document-coordinate geometry.
    pub points: Vec<AnnotationPoint>,
    /// Geometry width in milli-pixels.
    pub stroke_width_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnnotationObject {
    pub(crate) id: AnnotationObjectId,
    pub(crate) input: AnnotationObjectInput,
}

impl AnnotationObject {
    pub(crate) fn info(&self) -> AnnotationObjectInfo {
        AnnotationObjectInfo {
            id: self.id.get(),
            layer_id: self.input.layer_id,
            kind: self.input.kind,
            output: self.input.output,
            bounds: self.input.bounds,
            font_family_hint: self.input.font_family_hint.clone(),
            font_size_milli: self.input.font_size_milli,
            style_flags: self.input.style_flags,
            color: self.input.color,
            text: self.input.text.clone(),
            points: self.input.points.clone(),
            stroke_width_milli: self.input.stroke_width_milli,
        }
    }
}

/// Result of one synchronous annotation edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnotationEditOutcome {
    revision: u64,
    created_object_ids: Vec<u64>,
}

impl AnnotationEditOutcome {
    /// Returns the document revision after the edit. A no-op keeps the old value.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns stable IDs allocated by Create edits, in request order.
    #[must_use]
    pub fn created_object_ids(&self) -> &[u64] {
        &self.created_object_ids
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AnnotationStrokeSession {
    base_revision: u64,
    layer_id: u64,
    output: AnnotationOutput,
    color: PixelValue,
    width_milli: u32,
    points: Vec<AnnotationPoint>,
}

impl AnnotationStrokeSession {
    pub(crate) fn preview(&self) -> AnnotationObjectInfo {
        AnnotationObjectInfo {
            id: 0,
            layer_id: self.layer_id,
            kind: AnnotationKind::Stroke,
            output: self.output,
            bounds: bounds_for_points(&self.points, self.width_milli).unwrap_or_default(),
            font_family_hint: String::new(),
            font_size_milli: 0,
            style_flags: 0,
            color: self.color,
            text: String::new(),
            points: self.points.clone(),
            stroke_width_milli: self.width_milli,
        }
    }
}

impl Core {
    /// Returns all persistent annotation objects in ascending stable-ID order.
    ///
    /// This query does not expose mutable Core storage and changes no revision,
    /// history, dirty, savepoint, or renderer state.
    pub fn annotation_objects(&self) -> Result<Vec<AnnotationObjectInfo>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        Ok(document
            .annotations
            .iter()
            .map(AnnotationObject::info)
            .collect())
    }

    /// Applies a bounded list of annotation edits as one atomic history item.
    ///
    /// `expected_revision` detects stale UI commands. Invalid, stale, overflow,
    /// and semantic no-op requests consume no stable ID and publish no partial
    /// document, history, journal, dirty, savepoint, or cache state.
    pub fn edit_annotations(
        &mut self,
        expected_revision: u64,
        edits: &[AnnotationEdit],
    ) -> Result<AnnotationEditOutcome, CoreError> {
        if !self.canonical_invocation_active {
            self.ensure_no_active_stroke()?;
            if self.document_revision.get() != expected_revision {
                return Err(CoreError::InvalidState(
                    "annotation edit base revision is stale",
                ));
            }
            let result =
                self.execute_canonical_invocation(CanonicalInvocation::EditAnnotations {
                    edits: edits.to_vec(),
                })?;
            return Ok(AnnotationEditOutcome {
                revision: result.dispatch.revision(),
                created_object_ids: result.output_ids,
            });
        }
        self.apply_annotation_edits(edits)
    }

    pub(crate) fn apply_annotation_edits(
        &mut self,
        edits: &[AnnotationEdit],
    ) -> Result<AnnotationEditOutcome, CoreError> {
        if edits.is_empty() || edits.len() > MAX_ANNOTATION_BATCH_EDITS {
            return Err(CoreError::InvalidArgument(
                "annotation edit count is outside bounds",
            ));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut working = before.clone();
        let mut created = Vec::new();
        let mut changed = false;
        for edit in edits {
            match edit {
                AnnotationEdit::Create(input) => {
                    validate_annotation_input(&working, input)?;
                    if working.annotations.len() >= MAX_ANNOTATION_OBJECTS {
                        return Err(CoreError::InvalidArgument(
                            "annotation object count exceeds its bound",
                        ));
                    }
                    let id = self.next_id.take_annotation();
                    working.annotations.push(AnnotationObject {
                        id,
                        input: input.clone(),
                    });
                    created.push(id.get());
                    changed = true;
                }
                AnnotationEdit::Update { object_id, input } => {
                    validate_annotation_id(*object_id, "annotation object ID")?;
                    validate_annotation_input(&working, input)?;
                    let object = working
                        .annotations
                        .iter_mut()
                        .find(|object| object.id.get() == *object_id)
                        .ok_or(CoreError::InvalidArgument(
                            "annotation object ID does not exist",
                        ))?;
                    if object.input != *input {
                        object.input = input.clone();
                        changed = true;
                    }
                }
                AnnotationEdit::Move {
                    object_id,
                    delta_x,
                    delta_y,
                } => {
                    validate_annotation_id(*object_id, "annotation object ID")?;
                    if *delta_x == 0 && *delta_y == 0 {
                        continue;
                    }
                    let index = working
                        .annotations
                        .iter()
                        .position(|object| object.id.get() == *object_id)
                        .ok_or(CoreError::InvalidArgument(
                            "annotation object ID does not exist",
                        ))?;
                    let mut input = working.annotations[index].input.clone();
                    input.bounds.x = input
                        .bounds
                        .x
                        .checked_add(*delta_x)
                        .ok_or(CoreError::InvalidArgument("annotation move overflows"))?;
                    input.bounds.y = input
                        .bounds
                        .y
                        .checked_add(*delta_y)
                        .ok_or(CoreError::InvalidArgument("annotation move overflows"))?;
                    let dx = delta_x
                        .checked_mul(1_000)
                        .ok_or(CoreError::InvalidArgument("annotation move overflows"))?;
                    let dy = delta_y
                        .checked_mul(1_000)
                        .ok_or(CoreError::InvalidArgument("annotation move overflows"))?;
                    for point in &mut input.points {
                        point.x_milli = point
                            .x_milli
                            .checked_add(dx)
                            .ok_or(CoreError::InvalidArgument("annotation move overflows"))?;
                        point.y_milli = point
                            .y_milli
                            .checked_add(dy)
                            .ok_or(CoreError::InvalidArgument("annotation move overflows"))?;
                    }
                    validate_annotation_input(&working, &input)?;
                    working.annotations[index].input = input;
                    changed = true;
                }
                AnnotationEdit::Delete { object_id } => {
                    validate_annotation_id(*object_id, "annotation object ID")?;
                    let index = working
                        .annotations
                        .iter()
                        .position(|object| object.id.get() == *object_id)
                        .ok_or(CoreError::InvalidArgument(
                            "annotation object ID does not exist",
                        ))?;
                    working.annotations.remove(index);
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(AnnotationEditOutcome {
                revision: self.document_revision.get(),
                created_object_ids: Vec::new(),
            });
        }
        working.annotations.sort_unstable_by_key(|object| object.id);
        let dispatch = self.commit_deferred_document_edit_current(before, working)?;
        Ok(AnnotationEditOutcome {
            revision: dispatch.revision(),
            created_object_ids: created,
        })
    }

    /// Begins one transient freehand instruction stroke.
    pub fn begin_annotation_stroke(
        &mut self,
        expected_revision: u64,
        layer_id: u64,
        output: AnnotationOutput,
        color: PixelValue,
        width_milli: u32,
        start: AnnotationPoint,
    ) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_revision.get() != expected_revision {
            return Err(CoreError::InvalidState(
                "annotation stroke base revision is stale",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        validate_annotation_layer(document, layer_id)?;
        validate_annotation_color(color)?;
        validate_point(document, start)?;
        if width_milli == 0 || width_milli > 1_000_000 {
            return Err(CoreError::InvalidArgument(
                "annotation stroke width is outside bounds",
            ));
        }
        self.annotation_stroke = Some(AnnotationStrokeSession {
            base_revision: expected_revision,
            layer_id,
            output,
            color,
            width_milli,
            points: vec![start],
        });
        Ok(())
    }

    /// Appends a non-empty bounded batch to the active annotation stroke.
    pub fn append_annotation_stroke(
        &mut self,
        points: &[AnnotationPoint],
    ) -> Result<(), CoreError> {
        if points.is_empty() {
            return Err(CoreError::InvalidArgument(
                "annotation stroke sample batch is empty",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if points
            .iter()
            .any(|point| validate_point(document, *point).is_err())
        {
            return Err(CoreError::InvalidArgument(
                "annotation stroke point is outside the document",
            ));
        }
        let session = self
            .annotation_stroke
            .as_mut()
            .ok_or(CoreError::InvalidState(
                "no annotation stroke transaction is active",
            ))?;
        if session.points.len().saturating_add(points.len()) > MAX_ANNOTATION_POINTS {
            return Err(CoreError::InvalidArgument(
                "annotation stroke point count exceeds its bound",
            ));
        }
        session.points.extend_from_slice(points);
        Ok(())
    }

    /// Cancels the active annotation stroke without changing document state.
    pub fn cancel_annotation_stroke(&mut self) -> Result<(), CoreError> {
        self.annotation_stroke
            .take()
            .ok_or(CoreError::InvalidState(
                "no annotation stroke transaction is active",
            ))?;
        Ok(())
    }

    /// Commits the active annotation stroke as one history item.
    pub fn end_annotation_stroke(&mut self) -> Result<AnnotationEditOutcome, CoreError> {
        let session = self
            .annotation_stroke
            .take()
            .ok_or(CoreError::InvalidState(
                "no annotation stroke transaction is active",
            ))?;
        let input = AnnotationObjectInput {
            layer_id: session.layer_id,
            kind: AnnotationKind::Stroke,
            output: session.output,
            bounds: bounds_for_points(&session.points, session.width_milli)?,
            font_family_hint: String::new(),
            font_size_milli: 0,
            style_flags: 0,
            color: session.color,
            text: String::new(),
            points: session.points,
            stroke_width_milli: session.width_milli,
        };
        self.edit_annotations(session.base_revision, &[AnnotationEdit::Create(input)])
    }
}

pub(crate) fn validate_annotation_input(
    document: &CellDocument,
    input: &AnnotationObjectInput,
) -> Result<(), CoreError> {
    validate_annotation_layer(document, input.layer_id)?;
    validate_annotation_color(input.color)?;
    validate_rect(document, input.bounds)?;
    if input.font_family_hint.len() > MAX_ANNOTATION_FONT_FAMILY_BYTES
        || input.font_family_hint.chars().any(char::is_control)
    {
        return Err(CoreError::InvalidArgument(
            "annotation font-family hint is invalid",
        ));
    }
    if input.text.len() > MAX_ANNOTATION_TEXT_BYTES {
        return Err(CoreError::InvalidArgument(
            "annotation text exceeds its byte bound",
        ));
    }
    if input.style_flags & !ANNOTATION_STYLE_MASK != 0 {
        return Err(CoreError::InvalidArgument(
            "annotation style flags are unsupported",
        ));
    }
    if input.points.len() > MAX_ANNOTATION_POINTS {
        return Err(CoreError::InvalidArgument(
            "annotation point count exceeds its bound",
        ));
    }
    for point in &input.points {
        validate_point(document, *point)?;
    }
    match input.kind {
        AnnotationKind::Text => {
            if input.text.is_empty()
                || !input.points.is_empty()
                || input.font_size_milli == 0
                || input.font_size_milli > 1_000_000
                || input.stroke_width_milli != 0
            {
                return Err(CoreError::InvalidArgument(
                    "text annotation fields are inconsistent",
                ));
            }
        }
        AnnotationKind::Stroke => {
            if !input.text.is_empty()
                || !input.font_family_hint.is_empty()
                || input.font_size_milli != 0
                || input.style_flags != 0
                || input.points.len() < 2
                || input.stroke_width_milli == 0
                || input.stroke_width_milli > 1_000_000
            {
                return Err(CoreError::InvalidArgument(
                    "stroke annotation fields are inconsistent",
                ));
            }
        }
        AnnotationKind::Leader => {
            if !input.text.is_empty()
                || !input.font_family_hint.is_empty()
                || input.font_size_milli != 0
                || input.style_flags != 0
                || input.points.len() != 2
                || input.stroke_width_milli == 0
                || input.stroke_width_milli > 1_000_000
            {
                return Err(CoreError::InvalidArgument(
                    "leader annotation fields are inconsistent",
                ));
            }
        }
        AnnotationKind::Value => {
            if input.text.is_empty()
                || input.font_size_milli == 0
                || input.font_size_milli > 1_000_000
                || input.points.len() != 2
                || input.stroke_width_milli == 0
                || input.stroke_width_milli > 1_000_000
            {
                return Err(CoreError::InvalidArgument(
                    "value annotation fields are inconsistent",
                ));
            }
        }
    }
    Ok(())
}

fn validate_annotation_layer(document: &CellDocument, layer_id: u64) -> Result<(), CoreError> {
    validate_annotation_id(layer_id, "annotation layer ID")?;
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.id.get() == layer_id)
        .ok_or(CoreError::InvalidArgument(
            "annotation layer ID does not exist",
        ))?;
    if !matches!(layer.kind, LayerKind::Text | LayerKind::Annotation) {
        return Err(CoreError::InvalidArgument(
            "annotation object requires a Text or Annotation layer",
        ));
    }
    if !layer.editable {
        return Err(CoreError::InvalidState("annotation layer is not editable"));
    }
    Ok(())
}

fn validate_annotation_color(color: PixelValue) -> Result<(), CoreError> {
    if matches!(color, PixelValue::Rgba(_) | PixelValue::Rgba16(_)) {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "annotation color must be straight RGBA",
        ))
    }
}

fn validate_rect(document: &CellDocument, bounds: RectI32) -> Result<(), CoreError> {
    let right = bounds
        .x
        .checked_add(bounds.width)
        .ok_or(CoreError::InvalidArgument("annotation bounds overflow"))?;
    let bottom = bounds
        .y
        .checked_add(bounds.height)
        .ok_or(CoreError::InvalidArgument("annotation bounds overflow"))?;
    let width = i32::try_from(document.width)
        .map_err(|_| CoreError::InvalidArgument("document width is not representable"))?;
    let height = i32::try_from(document.height)
        .map_err(|_| CoreError::InvalidArgument("document height is not representable"))?;
    if bounds.x < 0
        || bounds.y < 0
        || bounds.width <= 0
        || bounds.height <= 0
        || right > width
        || bottom > height
    {
        return Err(CoreError::InvalidArgument(
            "annotation bounds are outside the document",
        ));
    }
    Ok(())
}

fn validate_point(document: &CellDocument, point: AnnotationPoint) -> Result<(), CoreError> {
    let maximum_x = i32::try_from(document.width)
        .ok()
        .and_then(|value| value.checked_mul(1_000))
        .ok_or(CoreError::InvalidArgument(
            "document width exceeds annotation coordinate range",
        ))?;
    let maximum_y = i32::try_from(document.height)
        .ok()
        .and_then(|value| value.checked_mul(1_000))
        .ok_or(CoreError::InvalidArgument(
            "document height exceeds annotation coordinate range",
        ))?;
    if !(0..=maximum_x).contains(&point.x_milli) || !(0..=maximum_y).contains(&point.y_milli) {
        return Err(CoreError::InvalidArgument(
            "annotation point is outside the document",
        ));
    }
    Ok(())
}

fn bounds_for_points(points: &[AnnotationPoint], width_milli: u32) -> Result<RectI32, CoreError> {
    let first = points
        .first()
        .ok_or(CoreError::InvalidArgument("annotation point list is empty"))?;
    let mut min_x = first.x_milli;
    let mut max_x = first.x_milli;
    let mut min_y = first.y_milli;
    let mut max_y = first.y_milli;
    for point in points.iter().skip(1) {
        min_x = min_x.min(point.x_milli);
        max_x = max_x.max(point.x_milli);
        min_y = min_y.min(point.y_milli);
        max_y = max_y.max(point.y_milli);
    }
    let half = i32::try_from(width_milli.div_ceil(2))
        .map_err(|_| CoreError::InvalidArgument("annotation width is not representable"))?;
    let left = min_x.saturating_sub(half).max(0) / 1_000;
    let top = min_y.saturating_sub(half).max(0) / 1_000;
    let right_milli = max_x
        .checked_add(half)
        .ok_or(CoreError::InvalidArgument("annotation bounds overflow"))?;
    let right = right_milli
        .checked_add(999)
        .ok_or(CoreError::InvalidArgument("annotation bounds overflow"))?
        / 1_000;
    let bottom_milli = max_y
        .checked_add(half)
        .ok_or(CoreError::InvalidArgument("annotation bounds overflow"))?;
    let bottom = bottom_milli
        .checked_add(999)
        .ok_or(CoreError::InvalidArgument("annotation bounds overflow"))?
        / 1_000;
    Ok(RectI32 {
        x: left,
        y: top,
        width: (right - left).max(1),
        height: (bottom - top).max(1),
    })
}

fn validate_annotation_id(value: u64, name: &'static str) -> Result<(), CoreError> {
    if value == 0 || value > MAX_PERSISTENT_NUMERIC_ID {
        Err(CoreError::InvalidArgument(name))
    } else {
        Ok(())
    }
}

pub(crate) fn rasterize_annotation_layer(
    document: &CellDocument,
    layer_id: LayerId,
    width: u32,
    height: u32,
    include_instruction: bool,
) -> Result<Vec<[u8; 4]>, CoreError> {
    rasterize_annotation_layer_filtered(
        document,
        layer_id,
        width,
        height,
        include_instruction,
        false,
    )
}

pub(crate) fn rasterize_instruction_annotation_layer(
    document: &CellDocument,
    layer_id: LayerId,
    width: u32,
    height: u32,
) -> Result<Vec<[u8; 4]>, CoreError> {
    rasterize_annotation_layer_filtered(document, layer_id, width, height, true, true)
}

fn rasterize_annotation_layer_filtered(
    document: &CellDocument,
    layer_id: LayerId,
    width: u32,
    height: u32,
    include_instruction: bool,
    instruction_only: bool,
) -> Result<Vec<[u8; 4]>, CoreError> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .ok_or(CoreError::InvalidArgument(
            "annotation layer ID does not exist",
        ))?;
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| CoreError::InvalidState("annotation raster size is not representable"))?;
    let mut pixels = vec![[0_u8; 4]; pixel_count];
    for object in document.annotations.iter().filter(|object| {
        object.input.layer_id == layer_id.get()
            && (include_instruction || object.input.output == AnnotationOutput::Normal)
            && (!instruction_only || object.input.output == AnnotationOutput::Instruction)
    }) {
        let color = annotation_rgba8(object.input.color)?;
        match object.input.kind {
            AnnotationKind::Text => {
                draw_text_object(&mut pixels, width, height, document, &object.input, color)
            }
            AnnotationKind::Stroke | AnnotationKind::Leader => {
                draw_geometry_object(&mut pixels, width, height, document, &object.input, color)
            }
            AnnotationKind::Value => {
                draw_geometry_object(&mut pixels, width, height, document, &object.input, color);
                draw_text_object(&mut pixels, width, height, document, &object.input, color);
            }
        }
    }
    if layer.opacity_milli != 1_000 {
        for pixel in &mut pixels {
            pixel[3] = ((u32::from(pixel[3]) * layer.opacity_milli + 500) / 1_000) as u8;
        }
    }
    Ok(pixels)
}

fn draw_text_object(
    pixels: &mut [[u8; 4]],
    width: u32,
    height: u32,
    document: &CellDocument,
    input: &AnnotationObjectInput,
    color: [u8; 4],
) {
    let left = scale_pixel(input.bounds.x, width, document.width);
    let top = scale_pixel(input.bounds.y, height, document.height);
    let right = scale_pixel(
        input.bounds.x.saturating_add(input.bounds.width),
        width,
        document.width,
    )
    .max(left + 1)
    .min(width as i32);
    let bottom = scale_pixel(
        input.bounds.y.saturating_add(input.bounds.height),
        height,
        document.height,
    )
    .max(top + 1)
    .min(height as i32);
    let logical_height = i32::try_from(input.font_size_milli / 1_000)
        .unwrap_or(1)
        .max(1);
    let glyph_height = scale_pixel(logical_height, height, document.height).max(1);
    let glyph_width = (glyph_height * 3 / 5).max(1);
    let advance = (glyph_width + (glyph_width / 4).max(1)).max(1);
    let mut pen_x = left;
    let mut pen_y = top;
    for scalar in input.text.chars() {
        if scalar == '\n' || pen_x.saturating_add(glyph_width) > right {
            pen_x = left;
            pen_y = pen_y.saturating_add(glyph_height + 1);
            if scalar == '\n' {
                continue;
            }
        }
        if pen_y >= bottom {
            break;
        }
        if !scalar.is_whitespace() {
            let seed = scalar as u32;
            for y in 0..glyph_height.min(bottom - pen_y) {
                for x in 0..glyph_width.min(right - pen_x) {
                    let edge = x == 0 || y == 0 || x + 1 == glyph_width || y + 1 == glyph_height;
                    let interior =
                        ((seed.rotate_left((y as u32) & 15) >> ((x as u32) & 15)) & 1) != 0;
                    if edge || interior {
                        blend_pixel(pixels, width, height, pen_x + x, pen_y + y, color);
                    }
                }
            }
            if input.style_flags & ANNOTATION_STYLE_UNDERLINE != 0 {
                let underline_y = (pen_y + glyph_height - 1).min(bottom - 1);
                for x in 0..glyph_width.min(right - pen_x) {
                    blend_pixel(pixels, width, height, pen_x + x, underline_y, color);
                }
            }
        }
        pen_x = pen_x.saturating_add(advance);
    }
}

fn draw_geometry_object(
    pixels: &mut [[u8; 4]],
    width: u32,
    height: u32,
    document: &CellDocument,
    input: &AnnotationObjectInput,
    color: [u8; 4],
) {
    let radius = ((u64::from(input.stroke_width_milli)
        .saturating_mul(u64::from(width.max(height)))
        + u64::from(document.width.max(document.height)).saturating_mul(1_000))
        / (u64::from(document.width.max(document.height)).saturating_mul(2_000)))
    .max(1) as i32;
    for pair in input.points.windows(2) {
        let x0 = scale_milli(pair[0].x_milli, width, document.width);
        let y0 = scale_milli(pair[0].y_milli, height, document.height);
        let x1 = scale_milli(pair[1].x_milli, width, document.width);
        let y1 = scale_milli(pair[1].y_milli, height, document.height);
        draw_line(pixels, width, height, (x0, y0), (x1, y1), radius, color);
    }
}

fn draw_line(
    pixels: &mut [[u8; 4]],
    width: u32,
    height: u32,
    start: (i32, i32),
    end: (i32, i32),
    radius: i32,
    color: [u8; 4],
) {
    let (mut x0, mut y0) = start;
    let (x1, y1) = end;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        for offset_y in -radius..=radius {
            for offset_x in -radius..=radius {
                if offset_x * offset_x + offset_y * offset_y <= radius * radius {
                    blend_pixel(pixels, width, height, x0 + offset_x, y0 + offset_y, color);
                }
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let doubled = error.saturating_mul(2);
        if doubled >= dy {
            error += dy;
            x0 += sx;
        }
        if doubled <= dx {
            error += dx;
            y0 += sy;
        }
    }
}

fn blend_pixel(pixels: &mut [[u8; 4]], width: u32, height: u32, x: i32, y: i32, source: [u8; 4]) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    let index = y as usize * width as usize + x as usize;
    let destination = pixels[index];
    let source_alpha = u32::from(source[3]);
    let inverse = 255 - source_alpha;
    let destination_alpha = u32::from(destination[3]);
    let output_alpha = source_alpha + (destination_alpha * inverse + 127) / 255;
    let mut output = [0_u8; 4];
    if output_alpha != 0 {
        for channel in 0..3 {
            let premultiplied = u32::from(source[channel]) * source_alpha
                + (u32::from(destination[channel]) * destination_alpha * inverse + 127) / 255;
            output[channel] = (premultiplied + output_alpha / 2)
                .checked_div(output_alpha)
                .unwrap_or(0) as u8;
        }
    }
    output[3] = output_alpha as u8;
    pixels[index] = output;
}

fn scale_pixel(value: i32, output: u32, document: u32) -> i32 {
    ((i64::from(value) * i64::from(output)) / i64::from(document)) as i32
}

fn scale_milli(value: i32, output: u32, document: u32) -> i32 {
    ((i64::from(value) * i64::from(output)) / (i64::from(document) * 1_000)) as i32
}

fn annotation_rgba8(color: PixelValue) -> Result<[u8; 4], CoreError> {
    match color {
        PixelValue::Rgba(channels) => Ok(channels),
        PixelValue::Rgba16(channels) => {
            Ok(channels.map(|channel| ((u32::from(channel) + 128) / 257) as u8))
        }
        _ => Err(CoreError::InvalidState(
            "validated annotation color is not RGBA",
        )),
    }
}
