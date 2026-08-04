//! Interactive and one-shot stroke adapters for the canonical primitive kernel.

use super::*;
use crate::document::ensure_editable_plane;
use crate::primitive::{
    RasterStrokePreview, begin_stroke_preview, canonicalize_stroke, validate_stroke_request,
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
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let target_plane_id = document.plane_for_paint_role(stroke.plane)?.id.get();
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
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let target_plane_id = document.plane_for_paint_role(stroke.plane)?.id;
        ensure_editable_plane(document, target_plane_id)?;
        let arguments = canonicalize_stroke(
            stroke,
            &self.view,
            document.width,
            document.height,
            target_plane_id.get(),
        )?;
        let base_document = document.clone();
        let mut preview_document = base_document.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let preview =
            begin_stroke_preview(&mut preview_document, &arguments, preview_revision.get())?;
        let mut settings = stroke.clone();
        settings.samples.clear();
        self.active_stroke = Some(StrokeSession {
            settings,
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
        let appended = canonicalize_stroke(
            &batch,
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
    preview: RasterStrokePreview,
    base_revision: u64,
    base_document: CellDocument,
    pub(super) preview_document: CellDocument,
    pub(super) preview_revision: PreviewRevision,
}

impl StrokeSession {
    pub(super) fn canonical_payload_bytes(&self) -> u64 {
        self.preview.canonical_payload_bytes()
    }
}
