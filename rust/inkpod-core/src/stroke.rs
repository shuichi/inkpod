//! Interactive and one-shot stroke adapters for the canonical primitive kernel.

use super::*;
use crate::document::ensure_editable_plane;
use crate::primitive::{
    RasterStrokePreview, begin_stroke_preview, canonicalize_exact_stroke, canonicalize_stroke,
    validate_stroke_request,
};
use crate::view::{device_to_document, stroke_coordinate_is_supported};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DocumentStrokeSample {
    pub(super) point: DocumentPointF32,
    pub(super) pressure: f32,
}

impl Core {
    /// Applies a complete stroke through the canonical primitive executor.
    ///
    /// Validation, sample ownership, fixed-point normalization, no-op detection,
    /// state/procedure allocation, history, revision, and cache publication share
    /// the exact same owner used by canonical replay.
    pub fn apply_stroke(&mut self, stroke: &Stroke) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_stroke_request(stroke)?;
        let (active_layer_id, active_plane_id) = self.active_editor_target_ids()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let target_plane_id = document
            .plane_for_paint_role(stroke.plane, Some(active_layer_id), Some(active_plane_id))?
            .id
            .get();
        let expected_revision = self.document_revision.get();
        self.execute_primitive(PrimitiveRequest::ApplyRasterStroke {
            expected_revision,
            target_plane_id,
            stroke: stroke.clone(),
        })
        .map(|outcome| outcome.dispatch())
    }

    /// Begins an isolated canonical stroke preview transaction.
    ///
    /// The request's variable samples are converted immediately to an owned,
    /// bounded Q16 payload. The live document, persistent IDs, revision, history,
    /// dirty state, and render cache remain unchanged until [`Core::end_stroke`].
    pub fn begin_stroke(&mut self, stroke: &Stroke) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_stroke_request(stroke)?;
        let (active_layer_id, active_plane_id) = self.active_editor_target_ids()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let target_plane_id = document
            .plane_for_paint_role(stroke.plane, Some(active_layer_id), Some(active_plane_id))?
            .id;
        let arguments = canonicalize_stroke(
            stroke,
            &self.view,
            document.width,
            document.height,
            target_plane_id.get(),
        )?;
        self.begin_canonical_stroke_preview(stroke.clone(), arguments)
    }

    /// Begins a raster stroke using exact Core-owned EditorState values.
    ///
    /// The selected raster tool (or active tool when none is specified), its
    /// exact-depth Core-owned color/Q16.16 diameter, and stable target IDs are
    /// copied before the preview begins. Appends and commit use only that
    /// captured style, so later EditorState updates cannot change an already-
    /// started procedure. A tool selector chooses another Core-owned style; it
    /// never supplies color or diameter from the caller.
    pub fn begin_editor_stroke(&mut self, input: &EditorStrokeInput) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        let (tool, color, diameter_q16, layer_id, target_plane_id) =
            {
                let state = &self
                    .editor_session
                    .as_ref()
                    .ok_or(CoreError::NoDocument)?
                    .state;
                let editor_tool = input.tool.unwrap_or(state.active_tool);
                let tool = raster_tool_from_editor(editor_tool)?;
                let style = state
                    .tool_style(editor_tool)
                    .ok_or(CoreError::InvalidState(
                        "editor raster tool style is missing",
                    ))?;
                let color = style.color.or_else(|| state.current_color()).ok_or(
                    CoreError::InvalidState("editor raster tool has no captured color"),
                )?;
                let diameter_q16 = style.diameter_q16;
                let target = state
                    .target
                    .ok_or(CoreError::InvalidState("editor state has no active target"))?;
                (
                    tool,
                    color,
                    diameter_q16,
                    LayerId::from_raw(target.layer_id),
                    PlaneId::from_raw(target.plane_id),
                )
            };
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let target = document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .and_then(|layer| {
                layer
                    .planes
                    .iter()
                    .find(|plane| plane.id == target_plane_id)
            })
            .ok_or(CoreError::InvalidState(
                "editor stroke target no longer exists",
            ))?;
        let plane = if target.kind == PlaneType::MainLine {
            ActivePlane::MainLine
        } else {
            ActivePlane::Color
        };
        let stroke = Stroke {
            tool,
            plane,
            // Exact color and diameter travel beside this legacy-compatible
            // request shell and never round-trip through these presentation fields.
            color: [0; 4],
            diameter: (diameter_q16 as f64 / 65_536.0) as f32,
            auto_erase: input.auto_erase,
            pressure_size: input.pressure_size,
            coordinate_space: input.coordinate_space,
            samples: input.samples.clone(),
        };
        let arguments = canonicalize_exact_stroke(
            &stroke,
            color,
            diameter_q16,
            &self.view,
            document.width,
            document.height,
            target_plane_id.get(),
        )?;
        self.begin_canonical_stroke_preview(stroke, arguments)
    }

    fn begin_canonical_stroke_preview(
        &mut self,
        mut settings: Stroke,
        arguments: crate::primitive::CanonicalStrokeArguments,
    ) -> Result<(), CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let target_plane_id = PlaneId::from_raw(arguments.target_plane_id);
        ensure_editable_plane(document, target_plane_id)?;
        let base_document = document.clone();
        let mut preview_document = base_document.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let preview =
            begin_stroke_preview(&mut preview_document, &arguments, preview_revision.get())?;
        let captured_color = arguments.color;
        let captured_diameter_q16 = arguments.diameter_q16;
        settings.samples.clear();
        self.active_stroke = Some(StrokeSession {
            settings,
            captured_color,
            captured_diameter_q16,
            preview,
            base_revision: self.document_revision.get(),
            base_document,
            preview_document,
            preview_revision,
        });
        Ok(())
    }

    /// Appends samples to the active canonical stroke preview.
    ///
    /// Each batch is resolved to document Q16 coordinates before it is appended.
    /// Only the bridge from the preceding sample and the new batch are staged
    /// into the private preview. The final closed payload remains batching
    /// independent. Failure discards the session.
    pub fn append_stroke(&mut self, samples: &[StrokeSample]) -> Result<(), CoreError> {
        if samples.is_empty() {
            return Err(CoreError::InvalidArgument(
                "stroke append contains no samples",
            ));
        }
        let mut session = self.active_stroke.take().ok_or(CoreError::InvalidState(
            "there is no active stroke transaction",
        ))?;
        let mut batch = session.settings.clone();
        batch.samples = samples.to_vec();
        let appended = canonicalize_exact_stroke(
            &batch,
            session.captured_color,
            session.captured_diameter_q16,
            &self.view,
            session.preview_document.width,
            session.preview_document.height,
            session.preview.target_plane_id(),
        )?;
        let preview_revision = self.allocate_preview_revision()?;
        session.preview.append(
            &session.base_document,
            &mut session.preview_document,
            &appended,
            preview_revision.get(),
        )?;
        session.preview_revision = preview_revision;
        self.active_stroke = Some(session);
        Ok(())
    }

    /// Commits the active preview through the canonical primitive executor.
    ///
    /// A zero-pixel result is a no-op. A stale base, bounded-work overflow, or
    /// any validation failure leaves every live persistent value unchanged.
    pub fn end_stroke(&mut self) -> Result<DispatchOutcome, CoreError> {
        let session = self.active_stroke.take().ok_or(CoreError::InvalidState(
            "there is no active stroke transaction",
        ))?;
        let arguments = session.preview.into_arguments()?;
        self.execute_canonical_stroke(session.base_revision, arguments)
            .map(|outcome| outcome.dispatch())
    }

    /// Discards the active stroke preview without changing live document state.
    pub fn cancel_stroke(&mut self) {
        self.active_stroke = None;
    }

    /// Reports whether a stroke preview transaction is active.
    #[must_use]
    pub const fn stroke_is_active(&self) -> bool {
        self.active_stroke.is_some()
    }
}

pub(super) fn document_samples_for_view(
    view: ViewState,
    coordinate_space: CoordinateSpace,
    samples: &[StrokeSample],
    width: u32,
    height: u32,
) -> Result<Vec<DocumentStrokeSample>, CoreError> {
    validate_effect_samples(samples)?;
    match coordinate_space {
        CoordinateSpace::Document => samples
            .iter()
            .map(|sample| {
                Ok(DocumentStrokeSample {
                    point: DocumentPointF32::new(sample.x, sample.y)?,
                    pressure: sample.pressure,
                })
            })
            .collect(),
        CoordinateSpace::Device => {
            let document_size = DocumentSizeU32::new(width, height);
            samples
                .iter()
                .map(|sample| {
                    let device_point =
                        DevicePointF64::new(f64::from(sample.x), f64::from(sample.y))?;
                    let point = device_to_document(view, document_size, device_point);
                    if !stroke_coordinate_is_supported(point.x)
                        || !stroke_coordinate_is_supported(point.y)
                    {
                        return Err(CoreError::InvalidArgument(
                            "device-to-document stroke coordinate is outside bounds",
                        ));
                    }
                    Ok(DocumentStrokeSample {
                        point: DocumentPointF32::new(point.x as f32, point.y as f32)?,
                        pressure: sample.pressure,
                    })
                })
                .collect()
        }
    }
}

fn validate_effect_samples(samples: &[StrokeSample]) -> Result<(), CoreError> {
    if samples.is_empty() || samples.len() > MAX_STROKE_SAMPLES {
        return Err(CoreError::InvalidArgument(
            "stroke sample count is outside bounds",
        ));
    }
    if samples.iter().any(|sample| {
        !sample.x.is_finite()
            || !sample.y.is_finite()
            || sample.x.abs() > MAX_STROKE_COORDINATE
            || sample.y.abs() > MAX_STROKE_COORDINATE
            || !sample.pressure.is_finite()
            || !(0.0..=1.0).contains(&sample.pressure)
    }) {
        return Err(CoreError::InvalidArgument(
            "stroke sample contains invalid values",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(super) struct StrokeSession {
    settings: Stroke,
    captured_color: PixelValue,
    captured_diameter_q16: i64,
    preview: RasterStrokePreview,
    base_revision: u64,
    base_document: CellDocument,
    pub(super) preview_document: CellDocument,
    pub(super) preview_revision: PreviewRevision,
}

fn raster_tool_from_editor(tool: EditorTool) -> Result<PaintTool, CoreError> {
    match tool {
        EditorTool::Pencil => Ok(PaintTool::Pencil),
        EditorTool::Brush => Ok(PaintTool::Brush),
        EditorTool::Eraser => Ok(PaintTool::Eraser),
        _ => Err(CoreError::InvalidState(
            "active editor tool is not a raster stroke tool",
        )),
    }
}

impl StrokeSession {
    pub(super) fn canonical_payload_bytes(&self) -> u64 {
        self.preview.canonical_payload_bytes()
    }
}
