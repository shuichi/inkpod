//! Persistent vanishing points and viewport-bounded radial guide overlays.

use super::*;
use crate::geometry::sin_cos_turns;
use crate::primitive::CanonicalInvocation;
use crate::view::device_to_document;

/// Maximum persistent vanishing points in one document.
pub const MAX_VANISHING_POINTS: usize = 64;
/// Maximum edits accepted by one atomic vanishing-point request.
pub const MAX_VANISHING_POINT_EDITS: usize = 1_024;
/// Maximum absolute point coordinate in document milli-pixels.
pub const MAX_VANISHING_POINT_COORDINATE_MILLI: i64 = 67_108_864_000;
/// Smallest supported radial interval, in milli-degrees.
pub const MIN_VANISHING_POINT_INTERVAL_MILLI_DEGREES: u32 = 1_000;
/// Largest supported radial interval, in milli-degrees.
pub const MAX_VANISHING_POINT_INTERVAL_MILLI_DEGREES: u32 = 180_000;
/// Maximum derived radial segments in one immutable snapshot.
pub const MAX_SNAPSHOT_RADIAL_GUIDES: usize = 16_384;

/// Complete caller-owned persistent vanishing-point value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VanishingPointInput {
    /// Owning `LayerKind::VanishingPoint` stable layer ID.
    pub layer_id: u64,
    /// Horizontal document coordinate in milli-pixels; Canvas-exterior values are valid.
    pub x_milli: i64,
    /// Vertical document coordinate in milli-pixels; Canvas-exterior values are valid.
    pub y_milli: i64,
    /// Angular interval in milli-degrees, from 1 through 180 degrees.
    pub interval_milli_degrees: u32,
    /// Radial-family phase in milli-degrees; values wrap modulo 180 degrees.
    pub angle_milli_degrees: u32,
    /// Exact straight-alpha sRGB RGBA8 or RGBA16 display color.
    pub color: PixelValue,
    /// Object opacity in thousandths.
    pub opacity_milli: u32,
    /// Whether the point and radial guides participate in Canvas display and snapping.
    pub visible: bool,
}

/// One atomic vanishing-point edit operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VanishingPointEdit {
    /// Creates a new stable object at commit time.
    Create(VanishingPointInput),
    /// Replaces an existing object while preserving its stable ID.
    Update {
        /// Existing object ID.
        point_id: u64,
        /// Complete replacement value.
        input: VanishingPointInput,
    },
    /// Deletes one existing object.
    Delete {
        /// Existing object ID.
        point_id: u64,
    },
    /// Deletes every vanishing point as one atomic edit.
    DeleteAll,
}

/// Target captured by a long-lived dialog/handle preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VanishingPointPreviewTarget {
    /// Previews creation without consuming an ID.
    Create,
    /// Previews replacement of one existing object.
    Update(u64),
}

/// Immutable public information for one persistent or preview object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VanishingPointInfo {
    /// Stable ID, or zero only for an uncommitted create preview.
    pub id: u64,
    /// Owning layer stable ID.
    pub layer_id: u64,
    /// Horizontal document coordinate in milli-pixels.
    pub x_milli: i64,
    /// Vertical document coordinate in milli-pixels.
    pub y_milli: i64,
    /// Radial interval in milli-degrees.
    pub interval_milli_degrees: u32,
    /// Normalized radial-family phase in `[0, 180000)`.
    pub angle_milli_degrees: u32,
    /// Exact stored display color.
    pub color: PixelValue,
    /// Object opacity in thousandths.
    pub opacity_milli: u32,
    /// Canvas visibility and snap participation.
    pub visible: bool,
}

impl VanishingPointInfo {
    /// Returns the complete replacement input without the stable ID.
    #[must_use]
    pub const fn input(self) -> VanishingPointInput {
        VanishingPointInput {
            layer_id: self.layer_id,
            x_milli: self.x_milli,
            y_milli: self.y_milli,
            interval_milli_degrees: self.interval_milli_degrees,
            angle_milli_degrees: self.angle_milli_degrees,
            color: self.color,
            opacity_milli: self.opacity_milli,
            visible: self.visible,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VanishingPointObject {
    pub(crate) id: VanishingPointId,
    pub(crate) input: VanishingPointInput,
}

impl VanishingPointObject {
    pub(crate) const fn info(self) -> VanishingPointInfo {
        VanishingPointInfo {
            id: self.id.get(),
            layer_id: self.input.layer_id,
            x_milli: self.input.x_milli,
            y_milli: self.input.y_milli,
            interval_milli_degrees: self.input.interval_milli_degrees,
            angle_milli_degrees: self.input.angle_milli_degrees,
            color: self.input.color,
            opacity_milli: self.input.opacity_milli,
            visible: self.input.visible,
        }
    }
}

/// One viewport-clipped radial guide segment in document milli-pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderRadialGuide {
    /// Owning vanishing point ID, zero only for a create preview.
    pub point_id: u64,
    /// Line angle in milli-degrees in `[0, 180000)`.
    pub angle_milli_degrees: u32,
    /// First clipped endpoint X.
    pub start_x_milli: i64,
    /// First clipped endpoint Y.
    pub start_y_milli: i64,
    /// Second clipped endpoint X.
    pub end_x_milli: i64,
    /// Second clipped endpoint Y.
    pub end_y_milli: i64,
    /// Exact stored straight-alpha display color.
    pub color: PixelValue,
    /// Layer/object-combined opacity in thousandths.
    pub opacity_milli: u32,
}

/// Result of one atomic vanishing-point edit request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VanishingPointEditOutcome {
    revision: u64,
    point_ids: Vec<u64>,
}

impl VanishingPointEditOutcome {
    /// Returns the resulting document revision; a no-op keeps the previous value.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Borrows created or updated IDs in request order.
    #[must_use]
    pub fn point_ids(&self) -> &[u64] {
        &self.point_ids
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VanishingPointPreviewSession {
    pub(crate) base_revision: DocumentRevision,
    pub(crate) base_document: CellDocument,
    pub(crate) preview_document: CellDocument,
    pub(crate) target: VanishingPointPreviewTarget,
    pub(crate) preview_revision: PreviewRevision,
}

impl Core {
    /// Borrows persistent vanishing points in stable-ID order.
    pub fn vanishing_points(&self) -> Result<Vec<VanishingPointInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .vanishing_points
            .iter()
            .copied()
            .map(VanishingPointObject::info)
            .collect())
    }

    /// Applies a bounded edit batch as one canonical transaction and Undo unit.
    pub fn edit_vanishing_points(
        &mut self,
        expected_revision: u64,
        edits: &[VanishingPointEdit],
    ) -> Result<VanishingPointEditOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_revision.get() != expected_revision {
            return Err(CoreError::InvalidState(
                "vanishing-point edit base revision is stale",
            ));
        }
        if edits.is_empty() || edits.len() > MAX_VANISHING_POINT_EDITS {
            return Err(CoreError::InvalidArgument(
                "vanishing-point edit count is outside bounds",
            ));
        }
        let edits = edits
            .iter()
            .copied()
            .map(normalize_edit)
            .collect::<Vec<_>>();
        if !self.canonical_invocation_is_active() {
            let result = self
                .execute_canonical_invocation(CanonicalInvocation::EditVanishingPoints { edits })?;
            return Ok(VanishingPointEditOutcome {
                revision: result.dispatch.revision(),
                point_ids: result.output_ids,
            });
        }
        self.apply_vanishing_point_edits(&edits)
    }

    pub(crate) fn apply_vanishing_point_edits(
        &mut self,
        edits: &[VanishingPointEdit],
    ) -> Result<VanishingPointEditOutcome, CoreError> {
        if edits.is_empty() || edits.len() > MAX_VANISHING_POINT_EDITS {
            return Err(CoreError::InvalidArgument(
                "vanishing-point edit count is outside bounds",
            ));
        }
        if edits
            .iter()
            .any(|edit| matches!(edit, VanishingPointEdit::DeleteAll))
            && edits.len() != 1
        {
            return Err(CoreError::InvalidArgument(
                "delete-all cannot be combined with other vanishing-point edits",
            ));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut working = before.clone();
        let mut next_id = self.next_id;
        let mut output_ids = Vec::new();
        for edit in edits {
            match *edit {
                VanishingPointEdit::Create(input) => {
                    if working.vanishing_points.len() >= MAX_VANISHING_POINTS {
                        return Err(CoreError::InvalidState("vanishing-point limit reached"));
                    }
                    validate_vanishing_point_input(&working, input)?;
                    let id = next_id.take_vanishing_point();
                    working
                        .vanishing_points
                        .push(VanishingPointObject { id, input });
                    output_ids.push(id.get());
                }
                VanishingPointEdit::Update { point_id, input } => {
                    validate_vanishing_point_id(point_id)?;
                    validate_vanishing_point_input(&working, input)?;
                    let index = working
                        .vanishing_points
                        .iter()
                        .position(|object| object.id.get() == point_id)
                        .ok_or(CoreError::InvalidArgument(
                            "vanishing-point ID does not exist",
                        ))?;
                    ensure_layer_editable(
                        &working.layers,
                        working.vanishing_points[index].input.layer_id,
                    )?;
                    working.vanishing_points[index].input = input;
                    output_ids.push(point_id);
                }
                VanishingPointEdit::Delete { point_id } => {
                    validate_vanishing_point_id(point_id)?;
                    let index = working
                        .vanishing_points
                        .iter()
                        .position(|object| object.id.get() == point_id)
                        .ok_or(CoreError::InvalidArgument(
                            "vanishing-point ID does not exist",
                        ))?;
                    ensure_layer_editable(
                        &working.layers,
                        working.vanishing_points[index].input.layer_id,
                    )?;
                    working.vanishing_points.remove(index);
                }
                VanishingPointEdit::DeleteAll => {
                    if working.vanishing_points.is_empty() {
                        continue;
                    }
                    for object in &working.vanishing_points {
                        ensure_layer_editable(&working.layers, object.input.layer_id)?;
                    }
                    working.vanishing_points.clear();
                }
            }
        }
        working
            .vanishing_points
            .sort_by_key(|object| object.id.get());
        if working == before {
            return Ok(VanishingPointEditOutcome {
                revision: self.document_revision.get(),
                point_ids: output_ids,
            });
        }
        let dispatch = self.commit_deferred_document_edit_current(before, working)?;
        self.next_id = next_id;
        Ok(VanishingPointEditOutcome {
            revision: dispatch.revision(),
            point_ids: output_ids,
        })
    }

    /// Deletes every vanishing point as one atomic canonical edit.
    pub fn delete_all_vanishing_points(
        &mut self,
        expected_revision: u64,
    ) -> Result<VanishingPointEditOutcome, CoreError> {
        self.edit_vanishing_points(expected_revision, &[VanishingPointEdit::DeleteAll])
    }

    /// Begins a create or update preview without changing persistent state.
    pub fn begin_vanishing_point_preview(
        &mut self,
        expected_revision: u64,
        target: VanishingPointPreviewTarget,
        input: VanishingPointInput,
    ) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_revision.get() != expected_revision {
            return Err(CoreError::InvalidState(
                "vanishing-point preview base revision is stale",
            ));
        }
        let input = normalize_input(input);
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        validate_vanishing_point_input(&base_document, input)?;
        let mut preview_document = base_document.clone();
        set_preview_object(&mut preview_document, target, input)?;
        let preview_revision = self.allocate_preview_revision()?;
        self.vanishing_point_preview = Some(VanishingPointPreviewSession {
            base_revision: self.document_revision,
            base_document,
            preview_document,
            target,
            preview_revision,
        });
        Ok(())
    }

    /// Replaces the current preview value from the same immutable base document.
    pub fn update_vanishing_point_preview(
        &mut self,
        input: VanishingPointInput,
    ) -> Result<(), CoreError> {
        let input = normalize_input(input);
        let preview_revision = self.allocate_preview_revision()?;
        let session = self
            .vanishing_point_preview
            .as_mut()
            .ok_or(CoreError::InvalidState(
                "no vanishing-point preview is active",
            ))?;
        validate_vanishing_point_input(&session.base_document, input)?;
        session.preview_document = session.base_document.clone();
        set_preview_object(&mut session.preview_document, session.target, input)?;
        session.preview_revision = preview_revision;
        Ok(())
    }

    /// Cancels the active preview without changing persistent state.
    pub fn cancel_vanishing_point_preview(&mut self) -> Result<(), CoreError> {
        self.vanishing_point_preview
            .take()
            .ok_or(CoreError::InvalidState(
                "no vanishing-point preview is active",
            ))?;
        Ok(())
    }

    /// Applies the active preview as one canonical edit and Undo unit.
    pub fn apply_vanishing_point_preview(
        &mut self,
    ) -> Result<VanishingPointEditOutcome, CoreError> {
        let session = self
            .vanishing_point_preview
            .take()
            .ok_or(CoreError::InvalidState(
                "no vanishing-point preview is active",
            ))?;
        if self.document_revision != session.base_revision
            || self.document.as_ref() != Some(&session.base_document)
        {
            return Err(CoreError::InvalidState(
                "vanishing-point preview base revision is stale",
            ));
        }
        let preview = match session.target {
            VanishingPointPreviewTarget::Create => session
                .preview_document
                .vanishing_points
                .iter()
                .find(|object| object.id.get() == 0)
                .copied()
                .ok_or(CoreError::InvalidState(
                    "vanishing-point create preview lost its object",
                ))?,
            VanishingPointPreviewTarget::Update(point_id) => session
                .preview_document
                .vanishing_points
                .iter()
                .find(|object| object.id.get() == point_id)
                .copied()
                .ok_or(CoreError::InvalidState(
                    "vanishing-point update preview lost its object",
                ))?,
        };
        let edit = match session.target {
            VanishingPointPreviewTarget::Create => VanishingPointEdit::Create(preview.input),
            VanishingPointPreviewTarget::Update(point_id) => VanishingPointEdit::Update {
                point_id,
                input: preview.input,
            },
        };
        self.edit_vanishing_points(session.base_revision.get(), &[edit])
    }
}

fn set_preview_object(
    document: &mut CellDocument,
    target: VanishingPointPreviewTarget,
    input: VanishingPointInput,
) -> Result<(), CoreError> {
    match target {
        VanishingPointPreviewTarget::Create => {
            if document.vanishing_points.len() >= MAX_VANISHING_POINTS {
                return Err(CoreError::InvalidState("vanishing-point limit reached"));
            }
            document.vanishing_points.push(VanishingPointObject {
                id: VanishingPointId::from_raw(0),
                input,
            });
        }
        VanishingPointPreviewTarget::Update(point_id) => {
            validate_vanishing_point_id(point_id)?;
            let object = document
                .vanishing_points
                .iter_mut()
                .find(|object| object.id.get() == point_id)
                .ok_or(CoreError::InvalidArgument(
                    "vanishing-point ID does not exist",
                ))?;
            object.input = input;
        }
    }
    Ok(())
}

pub(crate) fn validate_vanishing_point_input(
    document: &CellDocument,
    input: VanishingPointInput,
) -> Result<(), CoreError> {
    if input.layer_id == 0
        || input.x_milli.unsigned_abs() > MAX_VANISHING_POINT_COORDINATE_MILLI as u64
        || input.y_milli.unsigned_abs() > MAX_VANISHING_POINT_COORDINATE_MILLI as u64
        || !(MIN_VANISHING_POINT_INTERVAL_MILLI_DEGREES
            ..=MAX_VANISHING_POINT_INTERVAL_MILLI_DEGREES)
            .contains(&input.interval_milli_degrees)
        || input.opacity_milli > 1_000
        || input.color.rgba16().is_none()
    {
        return Err(CoreError::InvalidArgument(
            "vanishing-point input is outside bounds",
        ));
    }
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.id.get() == input.layer_id)
        .ok_or(CoreError::InvalidArgument(
            "vanishing-point layer does not exist",
        ))?;
    if layer.kind != LayerKind::VanishingPoint {
        return Err(CoreError::InvalidArgument(
            "vanishing-point object requires a VanishingPoint layer",
        ));
    }
    if !layer.editable {
        return Err(CoreError::InvalidState(
            "vanishing-point layer is not editable",
        ));
    }
    Ok(())
}

fn ensure_layer_editable(layers: &[LayerNode], layer_id: u64) -> Result<(), CoreError> {
    let layer = layers
        .iter()
        .find(|layer| layer.id.get() == layer_id)
        .ok_or(CoreError::InvalidState(
            "vanishing-point owner layer is missing",
        ))?;
    if layer.kind != LayerKind::VanishingPoint || !layer.editable {
        Err(CoreError::InvalidState(
            "vanishing-point owner layer is not editable",
        ))
    } else {
        Ok(())
    }
}

fn validate_vanishing_point_id(point_id: u64) -> Result<(), CoreError> {
    if point_id == 0 || point_id > MAX_PERSISTENT_NUMERIC_ID {
        Err(CoreError::InvalidArgument(
            "vanishing-point ID is outside bounds",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn snap_to_radial_guides(
    document: &CellDocument,
    point: DocumentPointF64,
) -> Option<DocumentPointF64> {
    let mut best: Option<(f64, u64, u32, DocumentPointF64)> = None;
    for object in visible_objects(document) {
        let origin_x = object.input.x_milli as f64 / 1_000.0;
        let origin_y = object.input.y_milli as f64 / 1_000.0;
        for angle in radial_angles(
            object.input.interval_milli_degrees,
            object.input.angle_milli_degrees,
        ) {
            let (dx, dy) = radial_direction(angle);
            let along = (point.x - origin_x) * dx + (point.y - origin_y) * dy;
            let projected = DocumentPointF64 {
                x: origin_x + along * dx,
                y: origin_y + along * dy,
            };
            let distance = (projected.x - point.x).mul_add(
                projected.x - point.x,
                (projected.y - point.y) * (projected.y - point.y),
            );
            if distance <= 16.0
                && best.as_ref().is_none_or(|current| {
                    distance < current.0
                        || (distance == current.0
                            && (object.id.get(), angle) >= (current.1, current.2))
                })
            {
                best = Some((distance, object.id.get(), angle, projected));
            }
        }
    }
    best.map(|entry| entry.3)
}

pub(crate) fn build_radial_guides(
    document: &CellDocument,
    view: ViewState,
) -> Vec<RenderRadialGuide> {
    let rect = viewport_document_rect(document, view);
    let mut result = Vec::new();
    for object in visible_objects(document) {
        let layer = document
            .layers
            .iter()
            .find(|layer| layer.id.get() == object.input.layer_id)
            .expect("visible vanishing-point owner was validated");
        let opacity_milli = object.input.opacity_milli * layer.opacity_milli / 1_000;
        if opacity_milli == 0 {
            continue;
        }
        let origin = (
            object.input.x_milli as f64 / 1_000.0,
            object.input.y_milli as f64 / 1_000.0,
        );
        for angle in radial_angles(
            object.input.interval_milli_degrees,
            object.input.angle_milli_degrees,
        ) {
            if result.len() >= MAX_SNAPSHOT_RADIAL_GUIDES {
                return result;
            }
            let direction = radial_direction(angle);
            if let Some((start, end)) = clip_infinite_line(origin, direction, rect) {
                result.push(RenderRadialGuide {
                    point_id: object.id.get(),
                    angle_milli_degrees: angle,
                    start_x_milli: round_milli(start.0),
                    start_y_milli: round_milli(start.1),
                    end_x_milli: round_milli(end.0),
                    end_y_milli: round_milli(end.1),
                    color: object.input.color,
                    opacity_milli,
                });
            }
        }
    }
    result
}

fn visible_objects(document: &CellDocument) -> impl Iterator<Item = &VanishingPointObject> {
    document.vanishing_points.iter().filter(|object| {
        object.input.visible
            && document.layers.iter().any(|layer| {
                layer.id.get() == object.input.layer_id
                    && layer.kind == LayerKind::VanishingPoint
                    && layer.visible
            })
    })
}

pub(crate) fn visible_vanishing_point_infos(document: &CellDocument) -> Vec<VanishingPointInfo> {
    visible_objects(document)
        .copied()
        .map(VanishingPointObject::info)
        .collect()
}

fn radial_angles(interval: u32, phase: u32) -> impl Iterator<Item = u32> {
    let count = 180_000_u32.div_ceil(interval);
    let mut angles = (0..count)
        .map(|index| (phase + index * interval) % 180_000)
        .collect::<Vec<_>>();
    angles.sort_unstable();
    angles.dedup();
    angles.into_iter()
}

const fn normalize_input(mut input: VanishingPointInput) -> VanishingPointInput {
    input.angle_milli_degrees %= 180_000;
    input
}

const fn normalize_edit(edit: VanishingPointEdit) -> VanishingPointEdit {
    match edit {
        VanishingPointEdit::Create(input) => VanishingPointEdit::Create(normalize_input(input)),
        VanishingPointEdit::Update { point_id, input } => VanishingPointEdit::Update {
            point_id,
            input: normalize_input(input),
        },
        VanishingPointEdit::Delete { point_id } => VanishingPointEdit::Delete { point_id },
        VanishingPointEdit::DeleteAll => VanishingPointEdit::DeleteAll,
    }
}

fn milli_degrees_to_turns(value: u32) -> u32 {
    ((u64::from(value) * (u64::from(u32::MAX) + 1) + 180_000) / 360_000) as u32
}

fn radial_direction(angle: u32) -> (f64, f64) {
    match angle {
        0 => (1.0, 0.0),
        90_000 => (0.0, 1.0),
        _ => {
            let (cosine, sine) = sin_cos_turns(milli_degrees_to_turns(angle));
            (
                cosine as f64 / f64::from(1_i32 << 30),
                sine as f64 / f64::from(1_i32 << 30),
            )
        }
    }
}

pub(crate) fn mirror_vanishing_points(
    points: &mut [VanishingPointObject],
    document_size: DocumentSizeU32,
    axis: MirrorAxis,
) -> Result<(), CoreError> {
    let width = i64::from(document_size.width) * 1_000;
    let height = i64::from(document_size.height) * 1_000;
    for point in points {
        match axis {
            MirrorAxis::Horizontal => {
                point.input.x_milli =
                    width
                        .checked_sub(point.input.x_milli)
                        .ok_or(CoreError::InvalidArgument(
                            "mirrored vanishing point overflowed",
                        ))?;
            }
            MirrorAxis::Vertical => {
                point.input.y_milli =
                    height
                        .checked_sub(point.input.y_milli)
                        .ok_or(CoreError::InvalidArgument(
                            "mirrored vanishing point overflowed",
                        ))?;
            }
        }
        point.input.angle_milli_degrees = (180_000 - point.input.angle_milli_degrees) % 180_000;
        validate_transformed_point(point)?;
    }
    Ok(())
}

pub(crate) fn rotate_vanishing_points(
    points: &mut [VanishingPointObject],
    document_size: DocumentSizeU32,
    direction: RotateDirection,
) -> Result<(), CoreError> {
    let width = i64::from(document_size.width) * 1_000;
    let height = i64::from(document_size.height) * 1_000;
    for point in points {
        let (x, y) = (point.input.x_milli, point.input.y_milli);
        (point.input.x_milli, point.input.y_milli) = match direction {
            RotateDirection::Left90 => (
                y,
                width.checked_sub(x).ok_or(CoreError::InvalidArgument(
                    "rotated vanishing point overflowed",
                ))?,
            ),
            RotateDirection::Right90 => (
                height.checked_sub(y).ok_or(CoreError::InvalidArgument(
                    "rotated vanishing point overflowed",
                ))?,
                x,
            ),
        };
        point.input.angle_milli_degrees = (point.input.angle_milli_degrees + 90_000) % 180_000;
        validate_transformed_point(point)?;
    }
    Ok(())
}

pub(crate) fn resample_vanishing_points(
    points: &mut [VanishingPointObject],
    before: DocumentSizeU32,
    after: DocumentSizeU32,
) -> Result<(), CoreError> {
    if points.is_empty() {
        return Ok(());
    }
    if u64::from(after.width) * u64::from(before.height)
        != u64::from(after.height) * u64::from(before.width)
    {
        return Err(CoreError::InvalidArgument(
            "nonuniform resampling cannot preserve radial-guide angles",
        ));
    }
    let scale = f64::from(after.width) / f64::from(before.width);
    for point in points {
        point.input.x_milli = scaled_coordinate(point.input.x_milli, scale)?;
        point.input.y_milli = scaled_coordinate(point.input.y_milli, scale)?;
        validate_transformed_point(point)?;
    }
    Ok(())
}

pub(crate) fn translate_vanishing_points(
    points: &mut [VanishingPointObject],
    offset: DocumentOffsetI32,
) -> Result<(), CoreError> {
    let x = i64::from(offset.x) * 1_000;
    let y = i64::from(offset.y) * 1_000;
    for point in points {
        point.input.x_milli =
            point
                .input
                .x_milli
                .checked_add(x)
                .ok_or(CoreError::InvalidArgument(
                    "translated vanishing point overflowed",
                ))?;
        point.input.y_milli =
            point
                .input
                .y_milli
                .checked_add(y)
                .ok_or(CoreError::InvalidArgument(
                    "translated vanishing point overflowed",
                ))?;
        validate_transformed_point(point)?;
    }
    Ok(())
}

fn validate_transformed_point(point: &VanishingPointObject) -> Result<(), CoreError> {
    if point.input.x_milli.unsigned_abs() > MAX_VANISHING_POINT_COORDINATE_MILLI as u64
        || point.input.y_milli.unsigned_abs() > MAX_VANISHING_POINT_COORDINATE_MILLI as u64
    {
        return Err(CoreError::InvalidArgument(
            "transformed vanishing point exceeds coordinate bounds",
        ));
    }
    Ok(())
}

fn scaled_coordinate(value: i64, scale: f64) -> Result<i64, CoreError> {
    let scaled = (value as f64 * scale).round_ties_even();
    if !scaled.is_finite()
        || scaled < i64::MIN as f64
        || scaled > i64::MAX as f64
        || scaled.abs() > MAX_VANISHING_POINT_COORDINATE_MILLI as f64
    {
        Err(CoreError::InvalidArgument(
            "scaled vanishing point overflowed",
        ))
    } else {
        Ok(scaled as i64)
    }
}

fn viewport_document_rect(document: &CellDocument, view: ViewState) -> (f64, f64, f64, f64) {
    let size = DocumentSizeU32::new(document.width, document.height);
    let corners = [
        DevicePointF64 { x: 0.0, y: 0.0 },
        DevicePointF64 {
            x: view.viewport_width(),
            y: 0.0,
        },
        DevicePointF64 {
            x: 0.0,
            y: view.viewport_height(),
        },
        DevicePointF64 {
            x: view.viewport_width(),
            y: view.viewport_height(),
        },
    ]
    .map(|point| device_to_document(view, size, point));
    let min_x = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    (min_x, min_y, max_x, max_y)
}

fn clip_infinite_line(
    origin: (f64, f64),
    direction: (f64, f64),
    rect: (f64, f64, f64, f64),
) -> Option<((f64, f64), (f64, f64))> {
    let mut points = Vec::with_capacity(4);
    if direction.0.abs() > f64::EPSILON {
        for x in [rect.0, rect.2] {
            let t = (x - origin.0) / direction.0;
            let y = origin.1 + t * direction.1;
            if (rect.1..=rect.3).contains(&y) {
                points.push((x, y));
            }
        }
    }
    if direction.1.abs() > f64::EPSILON {
        for y in [rect.1, rect.3] {
            let t = (y - origin.1) / direction.1;
            let x = origin.0 + t * direction.0;
            if (rect.0..=rect.2).contains(&x) {
                points.push((x, y));
            }
        }
    }
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
    (points.len() >= 2).then(|| (points[0], points[1]))
}

fn round_milli(value: f64) -> i64 {
    (value * 1_000.0).round_ties_even() as i64
}
