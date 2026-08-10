//! Canonical line, curve, shape, and click-polyline construction.

use super::*;
use crate::document::ensure_editable_plane;
use crate::effects::{FilterPreview, PreviewProcedure};
use crate::primitive::CanonicalInvocation;
use crate::vector::{geometry_fill_plane_for_stroke, stage_geometry_fill, stage_geometry_path};
use inkpod_image::{canonical_q16_from_f32, div_round_ties_even_i128};

/// Maximum number of caller points accepted by one geometry request.
pub const MAX_GEOMETRY_POINTS: usize = 256;
const MAX_POLYGON_SIDES: u16 = 64;
const Q16_ONE: i64 = 1 << 16;
const Q30_ONE: i64 = 1 << 30;
const MIN_VECTOR_WIDTH_Q16: i64 = 66;
const CUBIC_FLATTEN_STEPS: i64 = 32;
const TAN_22_5_Q16: i64 = 27_146;
const KAPPA_Q30: i64 = 593_011_235;
const CORDIC_GAIN_Q30: i64 = 652_032_874;
const CORDIC_ATAN_TURNS: [i64; 30] = [
    0x2000_0000,
    0x12e4_051d,
    0x09fb_385b,
    0x0511_11d4,
    0x028b_0d43,
    0x0145_d7e1,
    0x00a2_f61e,
    0x0051_7c55,
    0x0028_be53,
    0x0014_5f2f,
    0x000a_2f98,
    0x0005_17cc,
    0x0002_8be6,
    0x0001_45f3,
    0x0000_a2fa,
    0x0000_517d,
    0x0000_28be,
    0x0000_145f,
    0x0000_0a30,
    0x0000_0518,
    0x0000_028c,
    0x0000_0146,
    0x0000_00a3,
    0x0000_0051,
    0x0000_0029,
    0x0000_0014,
    0x0000_000a,
    0x0000_0005,
    0x0000_0003,
    0x0000_0001,
];

/// Closed set of geometry primitives exposed to every frontend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryPrimitive {
    /// One segment from start to end.
    Line,
    /// One quadratic curve represented by start, end, and one control point.
    Curve,
    /// A rectangle resolved from an anchor and current point.
    Rectangle,
    /// A cubic approximation of an ellipse resolved from an anchor and current point.
    Ellipse,
    /// A regular polygon resolved from center and radius points.
    Polygon,
    /// An ordered click-style polyline.
    Polyline,
}

/// Brush footprint and open-vector cap shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryCrossSection {
    /// Circular raster footprint and round vector cap.
    Round,
    /// Square raster footprint and square vector cap.
    Square,
}

/// Typed construction and appearance options shared by raster and vector targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryOptions {
    /// Draws an outline on the target stroke plane.
    pub outline: bool,
    /// Draws a fill; open primitives reject this option.
    pub fill: bool,
    /// Connects the final polyline vertex to the first.
    pub close_path: bool,
    /// Converts polyline spans to deterministic Catmull-Rom-derived cubics.
    pub bezier_segments: bool,
    /// Constrains line-like input to the nearest multiple of 45 degrees.
    pub constrain_45_degrees: bool,
    /// Builds rectangle and ellipse extents around the first point.
    pub from_center: bool,
    /// Tapers an open outline from the minimum native vector width.
    pub taper_start: bool,
    /// Tapers an open outline to the minimum native vector width.
    pub taper_end: bool,
    /// Raster footprint and open-vector cap shape.
    pub cross_section: GeometryCrossSection,
    /// Optional positive width/height ratio in unsigned Q16.16; zero disables it.
    pub aspect_ratio_q16: u32,
    /// Regular-polygon edge count in `3..=64`.
    pub polygon_sides: u16,
    /// Additional clockwise rotation in one-turn unsigned fixed point.
    pub rotation_turns: u32,
}

/// Owned, bounded document-space geometry request.
#[derive(Clone, Debug, PartialEq)]
pub struct GeometryRequest {
    /// Stable raster or vector stroke plane target.
    pub plane_id: u64,
    /// Requested primitive.
    pub primitive: GeometryPrimitive,
    /// Primitive-specific document-space points.
    pub points: Vec<PointF32>,
    /// Exact native-depth outline color.
    pub outline_color: PixelValue,
    /// Exact native-depth fill color.
    pub fill_color: PixelValue,
    /// Positive outline width in document pixels.
    pub outline_width: f32,
    /// Construction and appearance options.
    pub options: GeometryOptions,
}

/// Non-persistent information for an active geometry preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryPreviewInfo {
    /// Stable target plane captured when preview began.
    pub plane_id: u64,
    /// Committed document revision captured when preview began.
    pub base_revision: u64,
    /// Transient render-only revision for the current preview.
    pub preview_revision: u64,
}

/// Result of one committed geometry procedure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryCommit {
    /// Shared revision/result contract.
    pub dispatch: DispatchOutcome,
    /// Created vector path ID, or zero for raster/no-op work.
    pub path_id: u64,
    /// Created vector fill ID, or zero when no vector fill was created.
    pub fill_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalGeometryPoint {
    pub(crate) x_q16: i64,
    pub(crate) y_q16: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalGeometrySegment {
    pub(crate) p0: CanonicalGeometryPoint,
    pub(crate) p1: CanonicalGeometryPoint,
    pub(crate) p2: CanonicalGeometryPoint,
    pub(crate) p3: CanonicalGeometryPoint,
    pub(crate) width_start_q16: i64,
    pub(crate) width_end_q16: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalGeometry {
    pub(crate) plane_id: u64,
    pub(crate) primitive: GeometryPrimitive,
    pub(crate) segments: Vec<CanonicalGeometrySegment>,
    pub(crate) fill_boundary: Vec<CanonicalGeometryPoint>,
    pub(crate) outline_color: PixelValue,
    pub(crate) fill_color: PixelValue,
    pub(crate) outline_width_q16: i64,
    pub(crate) cross_section: GeometryCrossSection,
    pub(crate) outline: bool,
    pub(crate) fill: bool,
    pub(crate) closed: bool,
}

impl Core {
    /// Applies one complete geometry request through the canonical executor.
    ///
    /// Success is one document revision and Undo unit. Degenerate geometry is a
    /// semantic no-op. Invalid target, geometry, bounds, stale session, overflow,
    /// and allocation failure publish no document, history, journal, dirty, ID,
    /// snapshot, or savepoint change.
    pub fn apply_geometry(
        &mut self,
        request: &GeometryRequest,
    ) -> Result<GeometryCommit, CoreError> {
        self.ensure_no_active_stroke()?;
        let canonical = canonicalize_geometry_request(request)?;
        let result = self.execute_canonical_invocation(CanonicalInvocation::ApplyGeometry {
            geometry: canonical,
        })?;
        Ok(geometry_commit(result.dispatch, &result.output_ids))
    }

    /// Begins an isolated geometry preview from the committed base document.
    pub fn begin_geometry_preview(
        &mut self,
        expected_revision: u64,
        request: &GeometryRequest,
    ) -> Result<GeometryPreviewInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if expected_revision != self.document_revision.get() {
            return Err(CoreError::InvalidState(
                "geometry preview base revision is stale",
            ));
        }
        let canonical = canonicalize_geometry_request(request)?;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        validate_geometry_target(&base_document, &canonical)?;
        let mut preview_document = base_document.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let mut preview_ids = self.next_id;
        stage_canonical_geometry(
            &mut preview_document,
            &canonical,
            preview_revision.get(),
            &mut preview_ids,
        )?;
        self.filter_preview = Some(FilterPreview {
            plane_id: PlaneId::from_raw(canonical.plane_id),
            base_revision: self.document_revision,
            base_document,
            preview_document,
            procedure: PreviewProcedure::Geometry(canonical),
            preview_revision,
        });
        self.render_cache.clear();
        Ok(GeometryPreviewInfo {
            plane_id: request.plane_id,
            base_revision: expected_revision,
            preview_revision: preview_revision.get(),
        })
    }

    /// Replaces the active preview from its immutable committed base.
    pub fn update_geometry_preview(
        &mut self,
        expected_revision: u64,
        request: &GeometryRequest,
    ) -> Result<GeometryPreviewInfo, CoreError> {
        let prior = self
            .filter_preview
            .as_ref()
            .filter(|preview| matches!(&preview.procedure, PreviewProcedure::Geometry(_)))
            .cloned()
            .ok_or(CoreError::InvalidState(
                "there is no active geometry preview",
            ))?;
        let prior_geometry = match &prior.procedure {
            PreviewProcedure::Geometry(geometry) => geometry,
            _ => unreachable!("filtered geometry preview"),
        };
        let canonical = match canonicalize_geometry_request(request) {
            Ok(canonical)
                if canonical.plane_id == prior_geometry.plane_id
                    && canonical.primitive == prior_geometry.primitive =>
            {
                canonical
            }
            Ok(_) => {
                return Err(CoreError::InvalidArgument(
                    "geometry preview target or primitive changed",
                ));
            }
            Err(error) => return Err(error),
        };
        if expected_revision != prior.base_revision.get()
            || self.document_revision != prior.base_revision
        {
            return Err(CoreError::InvalidState(
                "geometry preview base revision is stale",
            ));
        }
        validate_geometry_target(&prior.base_document, &canonical)?;
        let mut preview_document = prior.base_document.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let mut preview_ids = self.next_id;
        stage_canonical_geometry(
            &mut preview_document,
            &canonical,
            preview_revision.get(),
            &mut preview_ids,
        )?;
        let plane_id = canonical.plane_id;
        self.filter_preview = Some(FilterPreview {
            plane_id: PlaneId::from_raw(canonical.plane_id),
            base_revision: prior.base_revision,
            base_document: prior.base_document,
            preview_document,
            procedure: PreviewProcedure::Geometry(canonical),
            preview_revision,
        });
        self.render_cache.clear();
        Ok(GeometryPreviewInfo {
            plane_id,
            base_revision: prior.base_revision.get(),
            preview_revision: preview_revision.get(),
        })
    }

    /// Commits the active preview through the same canonical geometry executor.
    pub fn commit_geometry_preview(&mut self) -> Result<GeometryCommit, CoreError> {
        if self
            .filter_preview
            .as_ref()
            .is_none_or(|preview| !matches!(&preview.procedure, PreviewProcedure::Geometry(_)))
        {
            return Err(CoreError::InvalidState(
                "there is no active geometry preview",
            ));
        }
        let session = self.filter_preview.take().expect("geometry preview exists");
        if self.document_revision != session.base_revision {
            self.filter_preview = Some(session);
            return Err(CoreError::InvalidState(
                "geometry preview base revision is stale",
            ));
        }
        let PreviewProcedure::Geometry(canonical) = &session.procedure else {
            unreachable!("validated geometry preview")
        };
        match self.execute_canonical_invocation(CanonicalInvocation::ApplyGeometry {
            geometry: canonical.clone(),
        }) {
            Ok(result) => Ok(geometry_commit(result.dispatch, &result.output_ids)),
            Err(error) => {
                self.filter_preview = Some(session);
                Err(error)
            }
        }
    }

    /// Discards the active preview without changing committed state.
    pub fn cancel_geometry_preview(&mut self) -> Result<(), CoreError> {
        if self
            .filter_preview
            .as_ref()
            .is_none_or(|preview| !matches!(&preview.procedure, PreviewProcedure::Geometry(_)))
        {
            return Err(CoreError::InvalidState(
                "there is no active geometry preview",
            ));
        }
        self.filter_preview = None;
        self.render_cache.clear();
        Ok(())
    }

    pub(crate) fn apply_canonical_geometry(
        &mut self,
        geometry: &CanonicalGeometry,
    ) -> Result<GeometryCommit, CoreError> {
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        validate_geometry_target(edit.documents().0, geometry)?;
        let revision = edit.revision().get();
        let mut next_id = self.next_id;
        let output_ids =
            stage_canonical_geometry(edit.working_mut(), geometry, revision, &mut next_id)?;
        if !target_is_vector(edit.documents().0, PlaneId::from_raw(geometry.plane_id))? {
            edit.preserve_render_cache_by_raster_revision();
        }
        let dispatch = edit.commit(self)?;
        if dispatch.revision != self.document_revision.get() {
            return Err(CoreError::InvalidState(
                "geometry commit revision was not published",
            ));
        }
        if dispatch.revision != 0 && output_ids != [0, 0] {
            self.next_id = next_id;
        }
        Ok(GeometryCommit {
            dispatch,
            path_id: output_ids[0],
            fill_id: output_ids[1],
        })
    }
}

fn geometry_commit(dispatch: DispatchOutcome, output_ids: &[u64]) -> GeometryCommit {
    GeometryCommit {
        dispatch,
        path_id: output_ids.first().copied().unwrap_or(0),
        fill_id: output_ids.get(1).copied().unwrap_or(0),
    }
}

pub(crate) fn canonicalize_geometry_request(
    request: &GeometryRequest,
) -> Result<CanonicalGeometry, CoreError> {
    if request.plane_id == 0
        || request.points.is_empty()
        || request.points.len() > MAX_GEOMETRY_POINTS
        || request.outline_color.rgba16().is_none()
        || request.fill_color.rgba16().is_none()
    {
        return Err(CoreError::InvalidArgument(
            "geometry request metadata is invalid",
        ));
    }
    if !request.options.outline && !request.options.fill {
        return Err(CoreError::InvalidArgument(
            "geometry request has no output style",
        ));
    }
    let width_q16 = canonical_q16_from_f32(request.outline_width).ok_or(
        CoreError::InvalidArgument("geometry outline width is invalid"),
    )?;
    if !(1..=i64::from(4_096) * Q16_ONE).contains(&width_q16) {
        return Err(CoreError::InvalidArgument(
            "geometry outline width is outside bounds",
        ));
    }
    if request.options.aspect_ratio_q16 != 0
        && !(Q16_ONE as u32 / 64..=Q16_ONE as u32 * 64).contains(&request.options.aspect_ratio_q16)
    {
        return Err(CoreError::InvalidArgument(
            "geometry aspect ratio is outside bounds",
        ));
    }
    if request.primitive == GeometryPrimitive::Polygon
        && !(3..=MAX_POLYGON_SIDES).contains(&request.options.polygon_sides)
    {
        return Err(CoreError::InvalidArgument(
            "geometry polygon side count is outside bounds",
        ));
    }
    let points = request
        .points
        .iter()
        .map(|point| {
            let x_q16 = canonical_q16_from_f32(point.x)
                .ok_or(CoreError::InvalidArgument("geometry point is invalid"))?;
            let y_q16 = canonical_q16_from_f32(point.y)
                .ok_or(CoreError::InvalidArgument("geometry point is invalid"))?;
            if x_q16.unsigned_abs() > (16_777_216_i64 * Q16_ONE) as u64
                || y_q16.unsigned_abs() > (16_777_216_i64 * Q16_ONE) as u64
            {
                return Err(CoreError::InvalidArgument(
                    "geometry point is outside bounds",
                ));
            }
            Ok(CanonicalGeometryPoint { x_q16, y_q16 })
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    let (mut segments, closed) = resolve_segments(request.primitive, &points, request.options)?;
    if request.options.fill && !closed {
        return Err(CoreError::InvalidArgument(
            "geometry fill requires a closed primitive",
        ));
    }
    if closed && (request.options.taper_start || request.options.taper_end) {
        return Err(CoreError::InvalidArgument(
            "closed geometry cannot use endpoint taper",
        ));
    }
    apply_segment_widths(&mut segments, width_q16, request.options)?;
    let fill_boundary = if request.options.fill {
        flattened_boundary(&segments, closed)?
    } else {
        Vec::new()
    };
    Ok(CanonicalGeometry {
        plane_id: request.plane_id,
        primitive: request.primitive,
        segments,
        fill_boundary,
        outline_color: request.outline_color,
        fill_color: request.fill_color,
        outline_width_q16: width_q16,
        cross_section: request.options.cross_section,
        outline: request.options.outline,
        fill: request.options.fill,
        closed,
    })
}

fn resolve_segments(
    primitive: GeometryPrimitive,
    points: &[CanonicalGeometryPoint],
    options: GeometryOptions,
) -> Result<(Vec<CanonicalGeometrySegment>, bool), CoreError> {
    match primitive {
        GeometryPrimitive::Line => {
            require_point_count(points, 2)?;
            let end = constrained_endpoint(points[0], points[1], options.constrain_45_degrees);
            Ok((
                line_if_distinct(points[0], end).into_iter().collect(),
                false,
            ))
        }
        GeometryPrimitive::Curve => {
            require_point_count(points, 3)?;
            let end = constrained_endpoint(points[0], points[1], options.constrain_45_degrees);
            if points[0] == end {
                return Ok((Vec::new(), false));
            }
            let p1 = lerp_point(points[0], points[2], 2, 3)?;
            let p2 = lerp_point(end, points[2], 2, 3)?;
            Ok((vec![segment(points[0], p1, p2, end)], false))
        }
        GeometryPrimitive::Rectangle => {
            require_point_count(points, 2)?;
            let (center, radius_x, radius_y) = box_geometry(points[0], points[1], options)?;
            if radius_x == 0 || radius_y == 0 {
                return Ok((Vec::new(), true));
            }
            let corners = [
                rotate_local(center, -radius_x, -radius_y, options.rotation_turns)?,
                rotate_local(center, radius_x, -radius_y, options.rotation_turns)?,
                rotate_local(center, radius_x, radius_y, options.rotation_turns)?,
                rotate_local(center, -radius_x, radius_y, options.rotation_turns)?,
            ];
            Ok((closed_lines(&corners), true))
        }
        GeometryPrimitive::Ellipse => {
            require_point_count(points, 2)?;
            let (center, radius_x, radius_y) = box_geometry(points[0], points[1], options)?;
            if radius_x == 0 || radius_y == 0 {
                return Ok((Vec::new(), true));
            }
            Ok((
                ellipse_segments(center, radius_x, radius_y, options.rotation_turns)?,
                true,
            ))
        }
        GeometryPrimitive::Polygon => {
            require_point_count(points, 2)?;
            let radius = CanonicalGeometryPoint {
                x_q16: points[1].x_q16 - points[0].x_q16,
                y_q16: points[1].y_q16 - points[0].y_q16,
            };
            if radius.x_q16 == 0 && radius.y_q16 == 0 {
                return Ok((Vec::new(), true));
            }
            let sides = options.polygon_sides as u32;
            let vertices = (0..sides)
                .map(|index| {
                    let turns = options
                        .rotation_turns
                        .wrapping_add(((u64::from(index) << 32) / u64::from(sides)) as u32);
                    let rotated = rotate_vector(radius, turns)?;
                    Ok(CanonicalGeometryPoint {
                        x_q16: points[0].x_q16.checked_add(rotated.x_q16).ok_or(
                            CoreError::InvalidArgument("geometry polygon coordinate overflows"),
                        )?,
                        y_q16: points[0].y_q16.checked_add(rotated.y_q16).ok_or(
                            CoreError::InvalidArgument("geometry polygon coordinate overflows"),
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, CoreError>>()?;
            Ok((closed_lines(&vertices), true))
        }
        GeometryPrimitive::Polyline => {
            if points.len() < 2 {
                return Err(CoreError::InvalidArgument(
                    "geometry polyline requires at least two points",
                ));
            }
            let mut constrained = Vec::with_capacity(points.len());
            constrained.push(points[0]);
            for point in &points[1..] {
                constrained.push(constrained_endpoint(
                    *constrained.last().expect("one point exists"),
                    *point,
                    options.constrain_45_degrees,
                ));
            }
            constrained.dedup();
            if constrained.len() < 2 {
                return Ok((Vec::new(), options.close_path));
            }
            let segments = if options.bezier_segments {
                smooth_polyline(&constrained, options.close_path)?
            } else if options.close_path {
                closed_lines(&constrained)
            } else {
                constrained
                    .windows(2)
                    .filter_map(|pair| line_if_distinct(pair[0], pair[1]))
                    .collect()
            };
            Ok((segments, options.close_path))
        }
    }
}

fn require_point_count(
    points: &[CanonicalGeometryPoint],
    expected: usize,
) -> Result<(), CoreError> {
    if points.len() == expected {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "geometry primitive point count is invalid",
        ))
    }
}

fn segment(
    p0: CanonicalGeometryPoint,
    p1: CanonicalGeometryPoint,
    p2: CanonicalGeometryPoint,
    p3: CanonicalGeometryPoint,
) -> CanonicalGeometrySegment {
    CanonicalGeometrySegment {
        p0,
        p1,
        p2,
        p3,
        width_start_q16: Q16_ONE,
        width_end_q16: Q16_ONE,
    }
}

fn line_if_distinct(
    start: CanonicalGeometryPoint,
    end: CanonicalGeometryPoint,
) -> Option<CanonicalGeometrySegment> {
    (start != end).then(|| {
        segment(
            start,
            lerp_point(start, end, 1, 3).expect("bounded line interpolation"),
            lerp_point(start, end, 2, 3).expect("bounded line interpolation"),
            end,
        )
    })
}

fn closed_lines(points: &[CanonicalGeometryPoint]) -> Vec<CanonicalGeometrySegment> {
    (0..points.len())
        .filter_map(|index| line_if_distinct(points[index], points[(index + 1) % points.len()]))
        .collect()
}

fn lerp_point(
    start: CanonicalGeometryPoint,
    end: CanonicalGeometryPoint,
    numerator: i128,
    denominator: i128,
) -> Result<CanonicalGeometryPoint, CoreError> {
    let component = |left: i64, right: i64| {
        let delta = i128::from(right) - i128::from(left);
        let offset = div_round_ties_even_i128(delta * numerator, denominator).ok_or(
            CoreError::InvalidArgument("geometry interpolation overflows"),
        )?;
        i64::try_from(i128::from(left) + offset)
            .map_err(|_| CoreError::InvalidArgument("geometry interpolation overflows"))
    };
    Ok(CanonicalGeometryPoint {
        x_q16: component(start.x_q16, end.x_q16)?,
        y_q16: component(start.y_q16, end.y_q16)?,
    })
}

fn constrained_endpoint(
    start: CanonicalGeometryPoint,
    end: CanonicalGeometryPoint,
    enabled: bool,
) -> CanonicalGeometryPoint {
    if !enabled {
        return end;
    }
    let dx = end.x_q16 - start.x_q16;
    let dy = end.y_q16 - start.y_q16;
    let ax = dx.unsigned_abs() as i64;
    let ay = dy.unsigned_abs() as i64;
    let maximum = ax.max(ay);
    let minimum = ax.min(ay);
    if maximum == 0 {
        return start;
    }
    let axis =
        i128::from(minimum) * i128::from(Q16_ONE) < i128::from(maximum) * i128::from(TAN_22_5_Q16);
    let (resolved_x, resolved_y) = if axis {
        if ax >= ay { (dx, 0) } else { (0, dy) }
    } else {
        (
            if dx < 0 { -maximum } else { maximum },
            if dy < 0 { -maximum } else { maximum },
        )
    };
    CanonicalGeometryPoint {
        x_q16: start.x_q16.saturating_add(resolved_x),
        y_q16: start.y_q16.saturating_add(resolved_y),
    }
}

fn box_geometry(
    anchor: CanonicalGeometryPoint,
    current: CanonicalGeometryPoint,
    options: GeometryOptions,
) -> Result<(CanonicalGeometryPoint, i64, i64), CoreError> {
    let dx = current.x_q16 - anchor.x_q16;
    let dy = current.y_q16 - anchor.y_q16;
    let center = if options.from_center {
        anchor
    } else {
        lerp_point(anchor, current, 1, 2)?
    };
    let divisor = if options.from_center { 1 } else { 2 };
    let mut radius_x = dx.unsigned_abs() as i64 / divisor;
    let mut radius_y = dy.unsigned_abs() as i64 / divisor;
    if options.aspect_ratio_q16 != 0 && radius_x != 0 && radius_y != 0 {
        let desired_x = div_round_ties_even_i128(
            i128::from(radius_y) * i128::from(options.aspect_ratio_q16),
            i128::from(Q16_ONE),
        )
        .ok_or(CoreError::InvalidArgument(
            "geometry aspect calculation overflows",
        ))?;
        if desired_x >= i128::from(radius_x) {
            radius_x = i64::try_from(desired_x)
                .map_err(|_| CoreError::InvalidArgument("geometry aspect overflows"))?;
        } else {
            radius_y = i64::try_from(
                div_round_ties_even_i128(
                    i128::from(radius_x) * i128::from(Q16_ONE),
                    i128::from(options.aspect_ratio_q16),
                )
                .ok_or(CoreError::InvalidArgument(
                    "geometry aspect calculation overflows",
                ))?,
            )
            .map_err(|_| CoreError::InvalidArgument("geometry aspect overflows"))?;
        }
    }
    Ok((center, radius_x, radius_y))
}

fn ellipse_segments(
    center: CanonicalGeometryPoint,
    radius_x: i64,
    radius_y: i64,
    rotation_turns: u32,
) -> Result<Vec<CanonicalGeometrySegment>, CoreError> {
    let kx = mul_shift(radius_x, KAPPA_Q30, 30)?;
    let ky = mul_shift(radius_y, KAPPA_Q30, 30)?;
    let local = [
        ((radius_x, 0), (radius_x, ky), (kx, radius_y), (0, radius_y)),
        (
            (0, radius_y),
            (-kx, radius_y),
            (-radius_x, ky),
            (-radius_x, 0),
        ),
        (
            (-radius_x, 0),
            (-radius_x, -ky),
            (-kx, -radius_y),
            (0, -radius_y),
        ),
        (
            (0, -radius_y),
            (kx, -radius_y),
            (radius_x, -ky),
            (radius_x, 0),
        ),
    ];
    local
        .into_iter()
        .map(|(p0, p1, p2, p3)| {
            Ok(segment(
                rotate_local(center, p0.0, p0.1, rotation_turns)?,
                rotate_local(center, p1.0, p1.1, rotation_turns)?,
                rotate_local(center, p2.0, p2.1, rotation_turns)?,
                rotate_local(center, p3.0, p3.1, rotation_turns)?,
            ))
        })
        .collect()
}

fn smooth_polyline(
    points: &[CanonicalGeometryPoint],
    closed: bool,
) -> Result<Vec<CanonicalGeometrySegment>, CoreError> {
    let span_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let at = |index: isize| -> CanonicalGeometryPoint {
        if closed {
            points[index.rem_euclid(points.len() as isize) as usize]
        } else {
            points[index.clamp(0, points.len() as isize - 1) as usize]
        }
    };
    (0..span_count)
        .map(|index| {
            let index = index as isize;
            let p0 = at(index);
            let p3 = at(index + 1);
            let tangent_start = point_delta(at(index - 1), at(index + 1));
            let tangent_end = point_delta(at(index), at(index + 2));
            let p1 = add_divided_delta(p0, tangent_start, 6)?;
            let p2 = add_divided_delta(p3, negate_delta(tangent_end), 6)?;
            Ok(segment(p0, p1, p2, p3))
        })
        .collect()
}

fn point_delta(
    start: CanonicalGeometryPoint,
    end: CanonicalGeometryPoint,
) -> CanonicalGeometryPoint {
    CanonicalGeometryPoint {
        x_q16: end.x_q16 - start.x_q16,
        y_q16: end.y_q16 - start.y_q16,
    }
}

fn negate_delta(delta: CanonicalGeometryPoint) -> CanonicalGeometryPoint {
    CanonicalGeometryPoint {
        x_q16: -delta.x_q16,
        y_q16: -delta.y_q16,
    }
}

fn add_divided_delta(
    point: CanonicalGeometryPoint,
    delta: CanonicalGeometryPoint,
    divisor: i128,
) -> Result<CanonicalGeometryPoint, CoreError> {
    let add = |value: i64, change: i64| {
        let part = div_round_ties_even_i128(i128::from(change), divisor).ok_or(
            CoreError::InvalidArgument("geometry curve calculation overflows"),
        )?;
        i64::try_from(i128::from(value) + part)
            .map_err(|_| CoreError::InvalidArgument("geometry curve calculation overflows"))
    };
    Ok(CanonicalGeometryPoint {
        x_q16: add(point.x_q16, delta.x_q16)?,
        y_q16: add(point.y_q16, delta.y_q16)?,
    })
}

fn apply_segment_widths(
    segments: &mut Vec<CanonicalGeometrySegment>,
    width_q16: i64,
    options: GeometryOptions,
) -> Result<(), CoreError> {
    if segments.len() == 1 && options.taper_start && options.taper_end {
        let source = segments[0];
        let (left, right) = split_cubic_half(source)?;
        *segments = vec![left, right];
    }
    for segment in segments.iter_mut() {
        segment.width_start_q16 = width_q16;
        segment.width_end_q16 = width_q16;
    }
    if let Some(first) = segments.first_mut().filter(|_| options.taper_start) {
        first.width_start_q16 = MIN_VECTOR_WIDTH_Q16.min(width_q16);
    }
    if let Some(last) = segments.last_mut().filter(|_| options.taper_end) {
        last.width_end_q16 = MIN_VECTOR_WIDTH_Q16.min(width_q16);
    }
    Ok(())
}

fn split_cubic_half(
    source: CanonicalGeometrySegment,
) -> Result<(CanonicalGeometrySegment, CanonicalGeometrySegment), CoreError> {
    let a = lerp_point(source.p0, source.p1, 1, 2)?;
    let b = lerp_point(source.p1, source.p2, 1, 2)?;
    let c = lerp_point(source.p2, source.p3, 1, 2)?;
    let d = lerp_point(a, b, 1, 2)?;
    let e = lerp_point(b, c, 1, 2)?;
    let middle = lerp_point(d, e, 1, 2)?;
    Ok((
        segment(source.p0, a, d, middle),
        segment(middle, e, c, source.p3),
    ))
}

fn flattened_boundary(
    segments: &[CanonicalGeometrySegment],
    closed: bool,
) -> Result<Vec<CanonicalGeometryPoint>, CoreError> {
    if segments.is_empty() || !closed {
        return Ok(Vec::new());
    }
    let mut points = Vec::with_capacity(segments.len() * CUBIC_FLATTEN_STEPS as usize);
    for (segment_index, segment) in segments.iter().enumerate() {
        for step in 0..=CUBIC_FLATTEN_STEPS {
            if segment_index != 0 && step == 0 {
                continue;
            }
            points.push(cubic_point(*segment, step, CUBIC_FLATTEN_STEPS)?);
        }
    }
    if points.last() == points.first() {
        points.pop();
    }
    points.dedup();
    if points.len() < 3 {
        Ok(Vec::new())
    } else {
        Ok(points)
    }
}

fn cubic_point(
    segment: CanonicalGeometrySegment,
    numerator: i64,
    denominator: i64,
) -> Result<CanonicalGeometryPoint, CoreError> {
    let inverse = denominator - numerator;
    let weights = [
        i128::from(inverse).pow(3),
        3 * i128::from(inverse).pow(2) * i128::from(numerator),
        3 * i128::from(inverse) * i128::from(numerator).pow(2),
        i128::from(numerator).pow(3),
    ];
    let divisor = i128::from(denominator).pow(3);
    let component =
        |values: [i64; 4]| {
            let sum = values
                .into_iter()
                .zip(weights)
                .try_fold(0_i128, |sum, (value, weight)| {
                    sum.checked_add(i128::from(value) * weight)
                })
                .ok_or(CoreError::InvalidArgument(
                    "geometry cubic calculation overflows",
                ))?;
            i64::try_from(div_round_ties_even_i128(sum, divisor).ok_or(
                CoreError::InvalidArgument("geometry cubic calculation overflows"),
            )?)
            .map_err(|_| CoreError::InvalidArgument("geometry cubic calculation overflows"))
        };
    Ok(CanonicalGeometryPoint {
        x_q16: component([
            segment.p0.x_q16,
            segment.p1.x_q16,
            segment.p2.x_q16,
            segment.p3.x_q16,
        ])?,
        y_q16: component([
            segment.p0.y_q16,
            segment.p1.y_q16,
            segment.p2.y_q16,
            segment.p3.y_q16,
        ])?,
    })
}

fn rotate_local(
    center: CanonicalGeometryPoint,
    x_q16: i64,
    y_q16: i64,
    turns: u32,
) -> Result<CanonicalGeometryPoint, CoreError> {
    let rotated = rotate_vector(CanonicalGeometryPoint { x_q16, y_q16 }, turns)?;
    Ok(CanonicalGeometryPoint {
        x_q16: center
            .x_q16
            .checked_add(rotated.x_q16)
            .ok_or(CoreError::InvalidArgument("geometry rotation overflows"))?,
        y_q16: center
            .y_q16
            .checked_add(rotated.y_q16)
            .ok_or(CoreError::InvalidArgument("geometry rotation overflows"))?,
    })
}

fn rotate_vector(
    vector: CanonicalGeometryPoint,
    turns: u32,
) -> Result<CanonicalGeometryPoint, CoreError> {
    let (cosine, sine) = sin_cos_turns(turns);
    let x = div_round_ties_even_i128(
        i128::from(vector.x_q16) * i128::from(cosine) - i128::from(vector.y_q16) * i128::from(sine),
        i128::from(Q30_ONE),
    )
    .ok_or(CoreError::InvalidArgument("geometry rotation overflows"))?;
    let y = div_round_ties_even_i128(
        i128::from(vector.x_q16) * i128::from(sine) + i128::from(vector.y_q16) * i128::from(cosine),
        i128::from(Q30_ONE),
    )
    .ok_or(CoreError::InvalidArgument("geometry rotation overflows"))?;
    Ok(CanonicalGeometryPoint {
        x_q16: i64::try_from(x)
            .map_err(|_| CoreError::InvalidArgument("geometry rotation overflows"))?,
        y_q16: i64::try_from(y)
            .map_err(|_| CoreError::InvalidArgument("geometry rotation overflows"))?,
    })
}

fn sin_cos_turns(turns: u32) -> (i64, i64) {
    let quadrant = turns >> 30;
    let mut z = i64::from(turns & 0x3fff_ffff);
    let mut x = CORDIC_GAIN_Q30;
    let mut y = 0_i64;
    for (index, angle) in CORDIC_ATAN_TURNS.into_iter().enumerate() {
        let shift = index as u32;
        if z >= 0 {
            let next_x = x - (y >> shift);
            y += x >> shift;
            x = next_x;
            z -= angle;
        } else {
            let next_x = x + (y >> shift);
            y -= x >> shift;
            x = next_x;
            z += angle;
        }
    }
    match quadrant {
        0 => (x, y),
        1 => (-y, x),
        2 => (-x, -y),
        _ => (y, -x),
    }
}

fn mul_shift(value: i64, factor: i64, shift: u32) -> Result<i64, CoreError> {
    i64::try_from(
        div_round_ties_even_i128(
            i128::from(value) * i128::from(factor),
            i128::from(1_i64 << shift),
        )
        .ok_or(CoreError::InvalidArgument(
            "geometry fixed calculation overflows",
        ))?,
    )
    .map_err(|_| CoreError::InvalidArgument("geometry fixed calculation overflows"))
}

fn validate_geometry_target(
    document: &CellDocument,
    geometry: &CanonicalGeometry,
) -> Result<(), CoreError> {
    let plane_id = PlaneId::from_raw(geometry.plane_id);
    let plane = document
        .plane_by_id(plane_id)
        .ok_or(CoreError::InvalidArgument(
            "geometry target plane does not exist",
        ))?;
    ensure_editable_plane(document, plane_id)?;
    match plane.kind {
        PlaneType::VectorMainLine | PlaneType::ColorTrace => {
            crate::vector::ensure_vector_stroke_plane(document, plane_id, true)?;
            if geometry.fill {
                geometry_fill_plane_for_stroke(document, plane_id)?;
            }
        }
        PlaneType::MainLine => {
            if geometry.fill {
                return Err(CoreError::InvalidArgument(
                    "raster main-line geometry cannot be filled",
                ));
            }
        }
        PlaneType::Color | PlaneType::Raster => {}
        _ => {
            return Err(CoreError::InvalidArgument(
                "geometry target plane is not drawable",
            ));
        }
    }
    Ok(())
}

fn validate_resolved_geometry(geometry: &CanonicalGeometry) -> Result<(), CoreError> {
    if geometry.plane_id == 0
        || (!geometry.outline && !geometry.fill)
        || geometry.outline_color.rgba16().is_none()
        || geometry.fill_color.rgba16().is_none()
        || !(1..=i64::from(4_096) * Q16_ONE).contains(&geometry.outline_width_q16)
        || geometry.segments.len() > MAX_GEOMETRY_POINTS * 2
    {
        return Err(CoreError::InvalidArgument(
            "canonical geometry metadata is invalid",
        ));
    }
    let expected_segments = match geometry.primitive {
        GeometryPrimitive::Line | GeometryPrimitive::Curve => 1..=2,
        GeometryPrimitive::Rectangle | GeometryPrimitive::Ellipse => 4..=4,
        GeometryPrimitive::Polygon => 3..=usize::from(MAX_POLYGON_SIDES),
        GeometryPrimitive::Polyline => 1..=MAX_GEOMETRY_POINTS * 2,
    };
    if !geometry.segments.is_empty() && !expected_segments.contains(&geometry.segments.len()) {
        return Err(CoreError::InvalidArgument(
            "canonical geometry segment count is invalid",
        ));
    }
    for (index, segment) in geometry.segments.iter().enumerate() {
        if !(1..=geometry.outline_width_q16).contains(&segment.width_start_q16)
            || !(1..=geometry.outline_width_q16).contains(&segment.width_end_q16)
            || (index != 0 && geometry.segments[index - 1].p3 != segment.p0)
            || (geometry.closed
                && (segment.width_start_q16 != geometry.outline_width_q16
                    || segment.width_end_q16 != geometry.outline_width_q16))
        {
            return Err(CoreError::InvalidArgument(
                "canonical geometry segment is invalid",
            ));
        }
    }
    if geometry.closed
        && geometry
            .segments
            .last()
            .is_some_and(|segment| segment.p3 != geometry.segments[0].p0)
    {
        return Err(CoreError::InvalidArgument(
            "canonical geometry is not closed",
        ));
    }
    if geometry.fill && !geometry.closed {
        return Err(CoreError::InvalidArgument(
            "canonical geometry fill requires a closed path",
        ));
    }
    let expected_boundary = if geometry.fill {
        flattened_boundary(&geometry.segments, geometry.closed)?
    } else {
        Vec::new()
    };
    if geometry.fill_boundary != expected_boundary {
        return Err(CoreError::InvalidArgument(
            "canonical geometry fill boundary is invalid",
        ));
    }
    Ok(())
}

fn target_is_vector(document: &CellDocument, plane_id: PlaneId) -> Result<bool, CoreError> {
    let kind = document
        .plane_by_id(plane_id)
        .ok_or(CoreError::InvalidArgument(
            "geometry target plane does not exist",
        ))?
        .kind;
    Ok(matches!(
        kind,
        PlaneType::VectorMainLine | PlaneType::ColorTrace
    ))
}

fn stage_canonical_geometry(
    document: &mut CellDocument,
    geometry: &CanonicalGeometry,
    revision: u64,
    next_id: &mut StableIdCursor,
) -> Result<[u64; 2], CoreError> {
    validate_resolved_geometry(geometry)?;
    validate_geometry_target(document, geometry)?;
    if geometry.segments.is_empty() {
        return Ok([0, 0]);
    }
    let plane_id = PlaneId::from_raw(geometry.plane_id);
    if target_is_vector(document, plane_id)? {
        stage_vector_geometry(document, geometry, next_id)
    } else {
        stage_raster_geometry(document, geometry, revision)?;
        Ok([0, 0])
    }
}

fn stage_vector_geometry(
    document: &mut CellDocument,
    geometry: &CanonicalGeometry,
    next_id: &mut StableIdCursor,
) -> Result<[u64; 2], CoreError> {
    let path_id = next_id.take_vector_path();
    let path_color = if geometry.outline {
        geometry.outline_color
    } else {
        transparent_color(geometry.outline_color)?
    };
    let input = VectorPathInput {
        segments: geometry
            .segments
            .iter()
            .map(|segment| VectorCubicSegment {
                p0: public_point(segment.p0),
                p1: public_point(segment.p1),
                p2: public_point(segment.p2),
                p3: public_point(segment.p3),
                width_start: q16_f32(segment.width_start_q16),
                width_end: q16_f32(segment.width_end_q16),
            })
            .collect(),
        color: path_color,
        closed: geometry.closed,
    };
    stage_geometry_path(
        document,
        path_id,
        PlaneId::from_raw(geometry.plane_id),
        input,
        geometry.cross_section == GeometryCrossSection::Square,
    )?;
    let fill_id = if geometry.fill {
        let fill_id = next_id.take_vector_fill();
        let fill_plane =
            geometry_fill_plane_for_stroke(document, PlaneId::from_raw(geometry.plane_id))?;
        stage_geometry_fill(document, fill_id, fill_plane, path_id, geometry.fill_color)?;
        fill_id.get()
    } else {
        0
    };
    Ok([path_id.get(), fill_id])
}

fn stage_raster_geometry(
    document: &mut CellDocument,
    geometry: &CanonicalGeometry,
    revision: u64,
) -> Result<(), CoreError> {
    if geometry.fill && !geometry.fill_boundary.is_empty() {
        fill_raster_polygon(document, geometry, revision)?;
    }
    if geometry.outline {
        let samples = flatten_outline_samples(geometry)?;
        if !samples.is_empty() {
            let stroke = Stroke {
                tool: PaintTool::Brush,
                plane: ActivePlane::Color,
                color: [0; 4],
                diameter: q16_f32(geometry.outline_width_q16),
                shape: match geometry.cross_section {
                    GeometryCrossSection::Round => BrushShape::Round,
                    GeometryCrossSection::Square => BrushShape::Square,
                },
                smoothing: 0,
                start_color: StartColorPredicate::Any,
                auto_erase: false,
                pressure_size: true,
                coordinate_space: CoordinateSpace::Document,
                samples,
            };
            let arguments = crate::primitive::canonicalize_exact_stroke(
                &stroke,
                geometry.outline_color,
                geometry.outline_width_q16,
                &ViewState::default(),
                document.width,
                document.height,
                geometry.plane_id,
            )?;
            crate::primitive::apply_raster_stroke(document, &arguments, revision)?;
        }
    }
    Ok(())
}

fn flatten_outline_samples(geometry: &CanonicalGeometry) -> Result<Vec<StrokeSample>, CoreError> {
    let mut samples =
        Vec::with_capacity(geometry.segments.len() * CUBIC_FLATTEN_STEPS as usize + 1);
    for (segment_index, segment) in geometry.segments.iter().enumerate() {
        for step in 0..=CUBIC_FLATTEN_STEPS {
            if segment_index != 0 && step == 0 {
                continue;
            }
            let point = cubic_point(*segment, step, CUBIC_FLATTEN_STEPS)?;
            let width_q16 = i64::try_from(
                div_round_ties_even_i128(
                    i128::from(segment.width_start_q16) * i128::from(CUBIC_FLATTEN_STEPS - step)
                        + i128::from(segment.width_end_q16) * i128::from(step),
                    i128::from(CUBIC_FLATTEN_STEPS),
                )
                .ok_or(CoreError::InvalidArgument(
                    "geometry width interpolation overflows",
                ))?,
            )
            .map_err(|_| CoreError::InvalidArgument("geometry width interpolation overflows"))?;
            samples.push(StrokeSample {
                x: q16_f32(point.x_q16),
                y: q16_f32(point.y_q16),
                pressure: (width_q16 as f64 / geometry.outline_width_q16 as f64).clamp(0.0, 1.0)
                    as f32,
            });
        }
    }
    Ok(samples)
}

fn fill_raster_polygon(
    document: &mut CellDocument,
    geometry: &CanonicalGeometry,
    revision: u64,
) -> Result<(), CoreError> {
    let plane_id = PlaneId::from_raw(geometry.plane_id);
    let (kind, format) = document
        .plane_by_id(plane_id)
        .map(|plane| (plane.kind, plane.raster.format()))
        .ok_or(CoreError::InvalidArgument(
            "geometry target plane does not exist",
        ))?;
    if !matches!(kind, PlaneType::Color | PlaneType::Raster) {
        return Err(CoreError::InvalidArgument(
            "geometry fill requires a color raster plane",
        ));
    }
    let desired = raster_color(format, geometry.fill_color)?;
    let min_x = geometry
        .fill_boundary
        .iter()
        .map(|point| point.x_q16)
        .min()
        .unwrap_or(0);
    let max_x = geometry
        .fill_boundary
        .iter()
        .map(|point| point.x_q16)
        .max()
        .unwrap_or(0);
    let min_y = geometry
        .fill_boundary
        .iter()
        .map(|point| point.y_q16)
        .min()
        .unwrap_or(0);
    let max_y = geometry
        .fill_boundary
        .iter()
        .map(|point| point.y_q16)
        .max()
        .unwrap_or(0);
    let first_x = q16_floor(min_x).clamp(0, i64::from(document.width)) as u32;
    let last_x = q16_ceil(max_x).clamp(0, i64::from(document.width)) as u32;
    let first_y = q16_floor(min_y).clamp(0, i64::from(document.height)) as u32;
    let last_y = q16_ceil(max_y).clamp(0, i64::from(document.height)) as u32;
    let work = u64::from(last_x - first_x)
        .checked_mul(u64::from(last_y - first_y))
        .ok_or(CoreError::InvalidArgument("geometry fill work overflows"))?;
    if work > MAX_IMAGE_EDIT_PIXELS {
        return Err(CoreError::InvalidArgument(
            "geometry fill work exceeds its bound",
        ));
    }
    let selection_active = document.selection.allocated_tile_count() != 0;
    let mut changes = Vec::new();
    for y in first_y..last_y {
        for x in first_x..last_x {
            if selection_active && document.selection.pixel(x, y)? == PixelValue::Binary(0) {
                continue;
            }
            let sample = CanonicalGeometryPoint {
                x_q16: i64::from(x) * Q16_ONE + Q16_ONE / 2,
                y_q16: i64::from(y) * Q16_ONE + Q16_ONE / 2,
            };
            if point_in_polygon(sample, &geometry.fill_boundary)
                && document
                    .plane_by_id(plane_id)
                    .expect("validated raster plane exists")
                    .raster
                    .pixel(x, y)?
                    != desired
            {
                changes.push((x, y));
            }
        }
    }
    let raster = &mut document
        .plane_by_id_mut(plane_id)
        .expect("validated raster plane exists")
        .raster;
    for (x, y) in changes {
        raster.set_pixel(x, y, desired, revision)?;
    }
    Ok(())
}

fn point_in_polygon(point: CanonicalGeometryPoint, polygon: &[CanonicalGeometryPoint]) -> bool {
    let mut inside = false;
    for index in 0..polygon.len() {
        let mut first = polygon[index];
        let mut second = polygon[(index + 1) % polygon.len()];
        if first.y_q16 > second.y_q16 {
            std::mem::swap(&mut first, &mut second);
        }
        if point.y_q16 < first.y_q16 || point.y_q16 >= second.y_q16 {
            continue;
        }
        let dy = i128::from(second.y_q16) - i128::from(first.y_q16);
        let intersection = i128::from(first.x_q16) * dy
            + (i128::from(point.y_q16) - i128::from(first.y_q16))
                * (i128::from(second.x_q16) - i128::from(first.x_q16));
        if intersection > i128::from(point.x_q16) * dy {
            inside = !inside;
        }
    }
    inside
}

fn raster_color(format: PixelFormat, color: PixelValue) -> Result<PixelValue, CoreError> {
    let rgba = color.rgba16().ok_or(CoreError::InvalidArgument(
        "geometry raster color must be RGBA",
    ))?;
    match format {
        PixelFormat::StraightRgba8 => {
            Ok(PixelValue::Rgba(rgba.map(|component| {
                ((u32::from(component) + 128) / 257) as u8
            })))
        }
        PixelFormat::StraightRgba16 => Ok(PixelValue::Rgba16(rgba)),
        _ => Err(CoreError::InvalidState(
            "geometry color target has an incompatible pixel format",
        )),
    }
}

fn transparent_color(color: PixelValue) -> Result<PixelValue, CoreError> {
    match color {
        PixelValue::Rgba(mut rgba) => {
            rgba[3] = 0;
            Ok(PixelValue::Rgba(rgba))
        }
        PixelValue::Rgba16(mut rgba) => {
            rgba[3] = 0;
            Ok(PixelValue::Rgba16(rgba))
        }
        _ => Err(CoreError::InvalidArgument("geometry color must be RGBA")),
    }
}

fn public_point(point: CanonicalGeometryPoint) -> PointF32 {
    PointF32 {
        x: q16_f32(point.x_q16),
        y: q16_f32(point.y_q16),
    }
}

fn q16_f32(value: i64) -> f32 {
    (value as f64 / Q16_ONE as f64) as f32
}

fn q16_floor(value: i64) -> i64 {
    value.div_euclid(Q16_ONE)
}

fn q16_ceil(value: i64) -> i64 {
    -(-value).div_euclid(Q16_ONE)
}
