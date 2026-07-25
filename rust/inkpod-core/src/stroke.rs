//! Interactive and one-shot stroke operations.

use super::*;
use crate::document::ensure_editable_role;
use crate::view::{device_to_document, stroke_coordinate_is_supported};

impl Core {
    pub fn apply_stroke(&mut self, stroke: &Stroke) -> Result<DispatchOutcome, CoreError> {
        self.begin_stroke(stroke)?;
        match self.end_stroke() {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.cancel_stroke();
                Err(error)
            }
        }
    }

    pub fn begin_stroke(&mut self, stroke: &Stroke) -> Result<(), CoreError> {
        if self.active_stroke.is_some() {
            return Err(CoreError::InvalidState(
                "a stroke transaction is already active",
            ));
        }
        validate_stroke(stroke)?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        ensure_editable_role(&document, stroke.plane)?;
        let samples = document_samples_for_view(
            self.view,
            stroke.coordinate_space,
            &stroke.samples,
            document.width,
            document.height,
        )?;
        let desired = stroke_value(stroke, &document, &samples)?;
        let preview_revision = self.allocate_preview_revision()?;
        let mut settings = stroke.clone();
        settings.samples.clear();
        let mut session = StrokeSession {
            stroke: settings,
            desired,
            preview_document: document,
            changes: BTreeMap::new(),
            last_sample: None,
            sample_count: 0,
            work: 0,
            preview_revision,
        };
        session.append_document_samples(&samples, preview_revision)?;
        self.active_stroke = Some(session);
        Ok(())
    }

    pub fn append_stroke(&mut self, samples: &[StrokeSample]) -> Result<(), CoreError> {
        if samples.is_empty() {
            return Err(CoreError::InvalidArgument(
                "stroke append contains no samples",
            ));
        }
        let mut session = self.active_stroke.take().ok_or(CoreError::InvalidState(
            "there is no active stroke transaction",
        ))?;
        let samples = document_samples_for_view(
            self.view,
            session.stroke.coordinate_space,
            samples,
            session.preview_document.width,
            session.preview_document.height,
        )?;
        let preview_revision = self.allocate_preview_revision()?;
        match session.append_document_samples(&samples, preview_revision) {
            Ok(()) => {
                self.active_stroke = Some(session);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub fn end_stroke(&mut self) -> Result<DispatchOutcome, CoreError> {
        let session = self.active_stroke.take().ok_or(CoreError::InvalidState(
            "there is no active stroke transaction",
        ))?;
        if session.changes.is_empty() {
            return Ok(DispatchOutcome {
                revision: self.document_revision,
                accepted_commands: 1,
            });
        }

        let after_state = self.allocate_state()?;
        let revision = self.next_document_revision()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let plane_id = document.plane_for_role(session.stroke.plane)?.id;
        document.active_plane_id = plane_id;
        let raster = document.raster_mut(session.stroke.plane);
        let mut changes = Vec::with_capacity(session.changes.len());
        let mut touched_tiles = BTreeSet::new();
        for ((x, y), change) in session.changes {
            raster.set_pixel(x, y, change.after, revision)?;
            touched_tiles.insert(TileCoord {
                x: x / TILE_SIZE,
                y: y / TILE_SIZE,
            });
            changes.push(change);
        }
        for coord in touched_tiles {
            raster.remove_tile_if_empty(coord);
        }
        self.document_revision = revision;
        self.commit_pixel_history(plane_id, changes, after_state);
        Ok(DispatchOutcome {
            revision,
            accepted_commands: 1,
        })
    }

    pub fn cancel_stroke(&mut self) {
        self.active_stroke = None;
    }

    #[must_use]
    pub const fn stroke_is_active(&self) -> bool {
        self.active_stroke.is_some()
    }
}

// Shared implementation helpers for this responsibility.

impl StrokeSession {
    fn append_document_samples(
        &mut self,
        samples: &[StrokeSample],
        preview_revision: u64,
    ) -> Result<(), CoreError> {
        let next_count = self
            .sample_count
            .checked_add(samples.len())
            .ok_or(CoreError::InvalidArgument("stroke sample count overflows"))?;
        if next_count > MAX_STROKE_SAMPLES {
            return Err(CoreError::InvalidArgument(
                "stroke sample count is outside bounds",
            ));
        }
        validate_stroke_samples(samples)?;

        let mut raster_samples =
            Vec::with_capacity(samples.len() + usize::from(self.last_sample.is_some()));
        if let Some(last) = self.last_sample {
            raster_samples.push(last);
        }
        raster_samples.extend_from_slice(samples);
        let mut incremental = self.stroke.clone();
        incremental.samples = raster_samples;
        let (staged, work) = stage_stroke_pixels_with_work(
            &self.preview_document,
            &incremental,
            &incremental.samples,
            self.desired,
            self.work,
        )?;

        let raster = self.preview_document.raster_mut(self.stroke.plane);
        let mut touched_tiles = BTreeSet::new();
        for ((x, y), after) in staged {
            let current = raster.pixel(x, y)?;
            if current == after {
                continue;
            }
            let before = self
                .changes
                .get(&(x, y))
                .map_or(current, |change| change.before);
            raster.set_pixel(x, y, after, preview_revision)?;
            let coord = TileCoord {
                x: x / TILE_SIZE,
                y: y / TILE_SIZE,
            };
            touched_tiles.insert(coord);
            if before == after {
                self.changes.remove(&(x, y));
            } else {
                self.changes.insert(
                    (x, y),
                    PixelChange {
                        x,
                        y,
                        before,
                        after,
                    },
                );
            }
        }
        for coord in touched_tiles {
            raster.remove_tile_if_empty(coord);
        }
        self.last_sample = samples.last().copied().or(self.last_sample);
        self.sample_count = next_count;
        self.work = work;
        self.preview_revision = preview_revision;
        Ok(())
    }
}

pub(super) fn document_samples_for_view(
    view: ViewState,
    coordinate_space: CoordinateSpace,
    samples: &[StrokeSample],
    width: u32,
    height: u32,
) -> Result<Vec<StrokeSample>, CoreError> {
    validate_stroke_samples(samples)?;
    match coordinate_space {
        CoordinateSpace::Document => Ok(samples.to_vec()),
        CoordinateSpace::Device => {
            if view.zoom <= 0.0 {
                return Err(CoreError::InvalidState("view zoom is invalid"));
            }
            samples
                .iter()
                .map(|sample| {
                    let (x, y) = device_to_document(
                        view,
                        width,
                        height,
                        f64::from(sample.x),
                        f64::from(sample.y),
                    )?;
                    if !stroke_coordinate_is_supported(x) || !stroke_coordinate_is_supported(y) {
                        return Err(CoreError::InvalidArgument(
                            "device-to-document stroke coordinate is outside bounds",
                        ));
                    }
                    Ok(StrokeSample {
                        x: x as f32,
                        y: y as f32,
                        pressure: sample.pressure,
                    })
                })
                .collect()
        }
    }
}

pub(super) fn validate_stroke(stroke: &Stroke) -> Result<(), CoreError> {
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
    validate_stroke_samples(&stroke.samples)
}

pub(super) fn validate_stroke_samples(samples: &[StrokeSample]) -> Result<(), CoreError> {
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

pub(super) fn stroke_value(
    stroke: &Stroke,
    document: &CellDocument,
    samples: &[StrokeSample],
) -> Result<PixelValue, CoreError> {
    let format = document.raster(stroke.plane).format();
    let (draw_value, erase_value) = match (stroke.plane, format) {
        (ActivePlane::MainLine, PixelFormat::BinaryMask8) => {
            (PixelValue::Binary(255), PixelValue::Binary(0))
        }
        (ActivePlane::MainLine, PixelFormat::Grayscale8) => {
            (PixelValue::Grayscale8(u8::MAX), PixelValue::Grayscale8(0))
        }
        (ActivePlane::MainLine, PixelFormat::Grayscale16) => (
            PixelValue::Grayscale16(u16::MAX),
            PixelValue::Grayscale16(0),
        ),
        (ActivePlane::Color, PixelFormat::StraightRgba8) => {
            (PixelValue::Rgba(stroke.color), PixelValue::Rgba([0; 4]))
        }
        (ActivePlane::Color, PixelFormat::StraightRgba16) => (
            PixelValue::Rgba16(stroke.color.map(|channel| u16::from(channel) * 257)),
            PixelValue::Rgba16([0; 4]),
        ),
        _ => {
            return Err(CoreError::InvalidState(
                "active plane pixel format does not support painting",
            ));
        }
    };
    if stroke.tool == PaintTool::Eraser {
        return Ok(erase_value);
    }
    if stroke.tool == PaintTool::Pencil && stroke.auto_erase {
        let first = samples[0];
        let x = first.x.round() as i64;
        let y = first.y.round() as i64;
        if x >= 0
            && y >= 0
            && x < i64::from(document.width)
            && y < i64::from(document.height)
            && document.raster(stroke.plane).pixel(x as u32, y as u32)? == draw_value
        {
            return Ok(erase_value);
        }
    }
    Ok(draw_value)
}

pub(super) fn stage_stroke_pixels_with_work(
    document: &CellDocument,
    stroke: &Stroke,
    samples: &[StrokeSample],
    value: PixelValue,
    initial_work: u64,
) -> Result<(StagedPixels, u64), CoreError> {
    let mut stager = StrokeStager {
        document,
        stroke,
        value,
        maximum_radius: stroke_maximum_radius(stroke),
        work: initial_work,
        staged: BTreeMap::new(),
    };
    let mut previous = samples[0];
    stager.stage_segment(previous, previous)?;
    for sample in &samples[1..] {
        stager.stage_segment(previous, *sample)?;
        previous = *sample;
    }
    Ok((stager.staged, stager.work))
}

struct StrokeStager<'a> {
    document: &'a CellDocument,
    stroke: &'a Stroke,
    value: PixelValue,
    maximum_radius: i64,
    work: u64,
    staged: BTreeMap<(u32, u32), PixelValue>,
}

impl StrokeStager<'_> {
    fn stage_segment(&mut self, start: StrokeSample, end: StrokeSample) -> Result<(), CoreError> {
        let Some((start, end)) =
            clip_segment_to_document(self.document, start, end, self.maximum_radius)
        else {
            return Ok(());
        };
        let mut x0 = start.x.round() as i64;
        let mut y0 = start.y.round() as i64;
        let x1 = end.x.round() as i64;
        let y1 = end.y.round() as i64;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        let steps = dx.max(-dy).max(1);
        let mut step = 0_i64;
        loop {
            let interpolation = step as f32 / steps as f32;
            let pressure = start.pressure + (end.pressure - start.pressure) * interpolation;
            self.stage_dab(x0, y0, pressure)?;
            if x0 == x1 && y0 == y1 {
                break;
            }
            let double_error = error * 2;
            if double_error >= dy {
                error += dy;
                x0 += sx;
            }
            if double_error <= dx {
                error += dx;
                y0 += sy;
            }
            step += 1;
        }
        Ok(())
    }

    fn stage_dab(&mut self, center_x: i64, center_y: i64, pressure: f32) -> Result<(), CoreError> {
        let radius = if self.stroke.tool == PaintTool::Pencil {
            0
        } else {
            let scale = if self.stroke.pressure_size {
                pressure.max(0.01)
            } else {
                1.0
            };
            ((self.stroke.diameter * scale - 1.0) / 2.0).ceil().max(0.0) as i64
        };
        let diameter = u64::try_from(radius * 2 + 1)
            .map_err(|_| CoreError::InvalidArgument("stroke radius is not representable"))?;
        let dab_work = diameter
            .checked_mul(diameter)
            .ok_or(CoreError::InvalidArgument(
                "stroke rasterization work overflows",
            ))?;
        self.work = self
            .work
            .checked_add(dab_work)
            .ok_or(CoreError::InvalidArgument(
                "stroke rasterization work overflows",
            ))?;
        if self.work > MAX_STROKE_WORK {
            return Err(CoreError::InvalidArgument(
                "stroke rasterization work exceeds the bounded limit",
            ));
        }
        let radius_squared = radius * radius;
        for offset_y in -radius..=radius {
            for offset_x in -radius..=radius {
                if offset_x * offset_x + offset_y * offset_y > radius_squared {
                    continue;
                }
                let x = center_x + offset_x;
                let y = center_y + offset_y;
                if x >= 0
                    && y >= 0
                    && x < i64::from(self.document.width)
                    && y < i64::from(self.document.height)
                {
                    self.staged.insert((x as u32, y as u32), self.value);
                }
            }
        }
        Ok(())
    }
}

pub(super) fn stroke_maximum_radius(stroke: &Stroke) -> i64 {
    if stroke.tool == PaintTool::Pencil {
        return 0;
    }
    let pressure = if stroke.pressure_size {
        stroke
            .samples
            .iter()
            .map(|sample| sample.pressure)
            .fold(0.01_f32, f32::max)
    } else {
        1.0
    };
    ((stroke.diameter * pressure - 1.0) / 2.0).ceil().max(0.0) as i64
}

pub(super) fn clip_segment_to_document(
    document: &CellDocument,
    start: StrokeSample,
    end: StrokeSample,
    radius: i64,
) -> Option<(StrokeSample, StrokeSample)> {
    let start_x = f64::from(start.x);
    let start_y = f64::from(start.y);
    let delta_x = f64::from(end.x) - start_x;
    let delta_y = f64::from(end.y) - start_y;
    let radius = radius as f64;
    let minimum_x = -radius;
    let minimum_y = -radius;
    let maximum_x = f64::from(document.width - 1) + radius;
    let maximum_y = f64::from(document.height - 1) + radius;
    let mut lower = 0.0_f64;
    let mut upper = 1.0_f64;

    for (coefficient, distance) in [
        (-delta_x, start_x - minimum_x),
        (delta_x, maximum_x - start_x),
        (-delta_y, start_y - minimum_y),
        (delta_y, maximum_y - start_y),
    ] {
        if coefficient == 0.0 {
            if distance < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = distance / coefficient;
        if coefficient < 0.0 {
            if ratio > upper {
                return None;
            }
            lower = lower.max(ratio);
        } else {
            if ratio < lower {
                return None;
            }
            upper = upper.min(ratio);
        }
    }

    let interpolate = |ratio: f64| StrokeSample {
        x: (start_x + delta_x * ratio) as f32,
        y: (start_y + delta_y * ratio) as f32,
        pressure: start.pressure + (end.pressure - start.pressure) * ratio as f32,
    };
    Some((interpolate(lower), interpolate(upper)))
}

#[derive(Clone, Debug)]
pub(super) struct StrokeSession {
    pub(super) stroke: Stroke,
    pub(super) desired: PixelValue,
    pub(super) preview_document: CellDocument,
    pub(super) changes: BTreeMap<(u32, u32), PixelChange>,
    pub(super) last_sample: Option<StrokeSample>,
    pub(super) sample_count: usize,
    pub(super) work: u64,
    pub(super) preview_revision: u64,
}
