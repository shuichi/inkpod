//! Editable angled shooting-frame instruction overlay.

use super::*;
use crate::geometry::sin_cos_turns;
use crate::primitive::CanonicalInvocation;
use inkpod_image::div_round_ties_even_i128;

const Q30_ONE: i128 = 1_i128 << 30;
/// Maximum absolute shooting-frame coordinate accepted by Core, in milli-pixels.
pub const MAX_SHOOTING_FRAME_COORDINATE_MILLI: i64 = 67_108_864_000;
/// Maximum shooting-frame edge length accepted by Core, in milli-pixels.
pub const MAX_SHOOTING_FRAME_SIZE_MILLI: u64 = 67_108_864_000;

/// Persistent manipulation anchor for an angled shooting frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShootingFrameAnchor {
    /// Rotated top-left corner.
    TopLeft,
    /// Rotated top-right corner.
    TopRight,
    /// Geometric center.
    Center,
    /// Rotated bottom-left corner.
    BottomLeft,
    /// Rotated bottom-right corner.
    BottomRight,
}

/// One fixed-point point in document milli-pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShootingFramePoint {
    /// Horizontal document coordinate.
    pub x_milli: i64,
    /// Vertical document coordinate.
    pub y_milli: i64,
}

/// Complete caller-owned value for an angled shooting-frame object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShootingFrameInput {
    /// Geometric center X in document milli-pixels.
    pub center_x_milli: i64,
    /// Geometric center Y in document milli-pixels.
    pub center_y_milli: i64,
    /// Positive edge length along the local horizontal axis, in milli-pixels.
    pub width_milli: u64,
    /// Positive edge length along the local vertical axis, in milli-pixels.
    pub height_milli: u64,
    /// Clockwise binary turns; every `u32` value is canonical.
    pub rotation_turns: u32,
    /// Persistent manipulation anchor.
    pub anchor: ShootingFrameAnchor,
    /// Whether the object is visible on the editing Canvas.
    pub visible: bool,
}

/// One synchronous create, complete replacement, or delete operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShootingFrameEdit {
    /// Creates the document's sole angled shooting frame.
    Create(ShootingFrameInput),
    /// Replaces the complete value while retaining its stable ID.
    Update {
        /// Existing stable frame ID.
        frame_id: u64,
        /// Complete replacement value.
        input: ShootingFrameInput,
    },
    /// Deletes the existing angled shooting frame.
    Delete {
        /// Existing stable frame ID.
        frame_id: u64,
    },
}

/// Target captured when a long-lived shooting-frame preview begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShootingFramePreviewTarget {
    /// Preview creation without consuming an ID until OK.
    Create,
    /// Preview replacement of an existing stable object.
    Update(u64),
}

/// Immutable public information for the persistent or preview object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShootingFrameInfo {
    /// Stable ID, or zero only for an uncommitted create preview.
    pub id: u64,
    /// Geometric center X in document milli-pixels.
    pub center_x_milli: i64,
    /// Geometric center Y in document milli-pixels.
    pub center_y_milli: i64,
    /// Positive local width in milli-pixels.
    pub width_milli: u64,
    /// Positive local height in milli-pixels.
    pub height_milli: u64,
    /// Clockwise binary turns.
    pub rotation_turns: u32,
    /// Persistent manipulation anchor.
    pub anchor: ShootingFrameAnchor,
    /// Canvas visibility.
    pub visible: bool,
}

impl ShootingFrameInfo {
    /// Returns the complete replacement input without the stable ID.
    #[must_use]
    pub const fn input(self) -> ShootingFrameInput {
        ShootingFrameInput {
            center_x_milli: self.center_x_milli,
            center_y_milli: self.center_y_milli,
            width_milli: self.width_milli,
            height_milli: self.height_milli,
            rotation_turns: self.rotation_turns,
            anchor: self.anchor,
            visible: self.visible,
        }
    }

    /// Returns one of the four rotated corners or the geometric center.
    pub fn anchor_point(
        self,
        anchor: ShootingFrameAnchor,
    ) -> Result<ShootingFramePoint, CoreError> {
        anchor_point(self.input(), anchor)
    }

    /// Returns the four rotated corners in top-left, top-right, bottom-right,
    /// bottom-left order.
    pub fn corners(self) -> Result<[ShootingFramePoint; 4], CoreError> {
        corners(self.input())
    }

    /// Tests whether a point is within `tolerance_milli` of the frame outline.
    pub fn hit_test_outline(
        self,
        point: ShootingFramePoint,
        tolerance_milli: u64,
    ) -> Result<bool, CoreError> {
        if tolerance_milli > MAX_SHOOTING_FRAME_SIZE_MILLI {
            return Err(CoreError::InvalidArgument(
                "shooting-frame hit tolerance exceeds its bound",
            ));
        }
        let local = inverse_local(self.input(), point)?;
        let half_width = i128::from(self.width_milli) / 2;
        let half_height = i128::from(self.height_milli) / 2;
        let tolerance = i128::from(tolerance_milli);
        let x = i128::from(local.x_milli).abs();
        let y = i128::from(local.y_milli).abs();
        Ok(
            (x - half_width).abs() <= tolerance && y <= half_height + tolerance
                || (y - half_height).abs() <= tolerance && x <= half_width + tolerance,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShootingFrameObject {
    pub(crate) id: ShootingFrameId,
    pub(crate) input: ShootingFrameInput,
}

impl ShootingFrameObject {
    pub(crate) const fn info(self) -> ShootingFrameInfo {
        ShootingFrameInfo {
            id: self.id.get(),
            center_x_milli: self.input.center_x_milli,
            center_y_milli: self.input.center_y_milli,
            width_milli: self.input.width_milli,
            height_milli: self.input.height_milli,
            rotation_turns: self.input.rotation_turns,
            anchor: self.input.anchor,
            visible: self.input.visible,
        }
    }
}

/// Result of one synchronous shooting-frame edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShootingFrameEditOutcome {
    revision: u64,
    frame_id: Option<u64>,
}

impl ShootingFrameEditOutcome {
    /// Returns the document revision after the edit; a no-op keeps the old value.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Returns the created or updated stable ID, and `None` after delete.
    #[must_use]
    pub const fn frame_id(self) -> Option<u64> {
        self.frame_id
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShootingFramePreviewSession {
    pub(crate) base_revision: DocumentRevision,
    pub(crate) base_document: CellDocument,
    pub(crate) preview_document: CellDocument,
    pub(crate) target: ShootingFramePreviewTarget,
    pub(crate) preview_revision: PreviewRevision,
}

impl Core {
    /// Returns the sole persistent angled shooting frame, if present.
    pub fn shooting_frame(&self) -> Result<Option<ShootingFrameInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .shooting_frame
            .map(ShootingFrameObject::info))
    }

    /// Applies one typed shooting-frame edit as one atomic history item.
    pub fn edit_shooting_frame(
        &mut self,
        expected_revision: u64,
        edit: ShootingFrameEdit,
    ) -> Result<ShootingFrameEditOutcome, CoreError> {
        if !self.canonical_invocation_active {
            self.ensure_no_active_stroke()?;
            if self.document_revision.get() != expected_revision {
                return Err(CoreError::InvalidState(
                    "shooting-frame edit base revision is stale",
                ));
            }
            let result =
                self.execute_canonical_invocation(CanonicalInvocation::EditShootingFrame { edit })?;
            return Ok(ShootingFrameEditOutcome {
                revision: result.dispatch.revision(),
                frame_id: result.output_ids.first().copied().filter(|id| *id != 0),
            });
        }
        self.apply_shooting_frame_edit(edit)
    }

    pub(crate) fn apply_shooting_frame_edit(
        &mut self,
        edit: ShootingFrameEdit,
    ) -> Result<ShootingFrameEditOutcome, CoreError> {
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut working = before.clone();
        let mut next_id = self.next_id;
        let (changed, frame_id) = match edit {
            ShootingFrameEdit::Create(input) => {
                if working.shooting_frame.is_some() {
                    return Err(CoreError::InvalidState(
                        "document already has an angled shooting frame",
                    ));
                }
                validate_shooting_frame_input(input)?;
                let id = next_id.take_shooting_frame();
                working.shooting_frame = Some(ShootingFrameObject { id, input });
                (true, Some(id.get()))
            }
            ShootingFrameEdit::Update { frame_id, input } => {
                validate_frame_id(frame_id)?;
                validate_shooting_frame_input(input)?;
                let object = working
                    .shooting_frame
                    .as_mut()
                    .filter(|object| object.id.get() == frame_id)
                    .ok_or(CoreError::InvalidArgument(
                        "shooting-frame ID does not exist",
                    ))?;
                let changed = object.input != input;
                object.input = input;
                (changed, Some(frame_id))
            }
            ShootingFrameEdit::Delete { frame_id } => {
                validate_frame_id(frame_id)?;
                if working
                    .shooting_frame
                    .is_none_or(|object| object.id.get() != frame_id)
                {
                    return Err(CoreError::InvalidArgument(
                        "shooting-frame ID does not exist",
                    ));
                }
                working.shooting_frame = None;
                (true, None)
            }
        };
        if !changed {
            return Ok(ShootingFrameEditOutcome {
                revision: self.document_revision.get(),
                frame_id,
            });
        }
        let dispatch = self.commit_deferred_document_edit_current(before, working)?;
        self.next_id = next_id;
        Ok(ShootingFrameEditOutcome {
            revision: dispatch.revision(),
            frame_id,
        })
    }

    /// Begins a create or update preview without changing persistent state.
    pub fn begin_shooting_frame_preview(
        &mut self,
        expected_revision: u64,
        target: ShootingFramePreviewTarget,
        input: ShootingFrameInput,
    ) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_revision.get() != expected_revision {
            return Err(CoreError::InvalidState(
                "shooting-frame preview base revision is stale",
            ));
        }
        validate_shooting_frame_input(input)?;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut preview_document = base_document.clone();
        set_preview_object(&mut preview_document, target, input)?;
        let preview_revision = self.allocate_preview_revision()?;
        self.shooting_frame_preview = Some(ShootingFramePreviewSession {
            base_revision: self.document_revision,
            base_document,
            preview_document,
            target,
            preview_revision,
        });
        Ok(())
    }

    /// Replaces the working value of the active preview.
    pub fn update_shooting_frame_preview(
        &mut self,
        input: ShootingFrameInput,
    ) -> Result<(), CoreError> {
        validate_shooting_frame_input(input)?;
        let preview_revision = self.allocate_preview_revision()?;
        let session = self
            .shooting_frame_preview
            .as_mut()
            .ok_or(CoreError::InvalidState(
                "no shooting-frame preview is active",
            ))?;
        set_preview_object(&mut session.preview_document, session.target, input)?;
        session.preview_revision = preview_revision;
        Ok(())
    }

    /// Cancels the active preview without changing persistent state.
    pub fn cancel_shooting_frame_preview(&mut self) -> Result<(), CoreError> {
        self.shooting_frame_preview
            .take()
            .ok_or(CoreError::InvalidState(
                "no shooting-frame preview is active",
            ))?;
        Ok(())
    }

    /// Commits the active preview as one canonical history item.
    pub fn apply_shooting_frame_preview(&mut self) -> Result<ShootingFrameEditOutcome, CoreError> {
        let session = self
            .shooting_frame_preview
            .take()
            .ok_or(CoreError::InvalidState(
                "no shooting-frame preview is active",
            ))?;
        if self.document_revision != session.base_revision
            || self.document.as_ref() != Some(&session.base_document)
        {
            return Err(CoreError::InvalidState(
                "shooting-frame preview base revision is stale",
            ));
        }
        let object = session
            .preview_document
            .shooting_frame
            .ok_or(CoreError::InvalidState(
                "shooting-frame preview lost its working object",
            ))?;
        let edit = match session.target {
            ShootingFramePreviewTarget::Create => ShootingFrameEdit::Create(object.input),
            ShootingFramePreviewTarget::Update(frame_id) => ShootingFrameEdit::Update {
                frame_id,
                input: object.input,
            },
        };
        self.edit_shooting_frame(session.base_revision.get(), edit)
    }
}

fn set_preview_object(
    document: &mut CellDocument,
    target: ShootingFramePreviewTarget,
    input: ShootingFrameInput,
) -> Result<(), CoreError> {
    match target {
        ShootingFramePreviewTarget::Create => {
            if document
                .shooting_frame
                .is_some_and(|object| object.id.get() != 0)
            {
                return Err(CoreError::InvalidState(
                    "document already has an angled shooting frame",
                ));
            }
            document.shooting_frame = Some(ShootingFrameObject {
                id: ShootingFrameId::from_raw(0),
                input,
            });
        }
        ShootingFramePreviewTarget::Update(frame_id) => {
            validate_frame_id(frame_id)?;
            let object = document
                .shooting_frame
                .as_mut()
                .filter(|object| object.id.get() == frame_id)
                .ok_or(CoreError::InvalidArgument(
                    "shooting-frame ID does not exist",
                ))?;
            object.input = input;
        }
    }
    Ok(())
}

pub(crate) fn validate_shooting_frame_input(input: ShootingFrameInput) -> Result<(), CoreError> {
    if input.center_x_milli.unsigned_abs() > MAX_SHOOTING_FRAME_COORDINATE_MILLI as u64
        || input.center_y_milli.unsigned_abs() > MAX_SHOOTING_FRAME_COORDINATE_MILLI as u64
        || input.width_milli == 0
        || input.height_milli == 0
        || input.width_milli > MAX_SHOOTING_FRAME_SIZE_MILLI
        || input.height_milli > MAX_SHOOTING_FRAME_SIZE_MILLI
    {
        return Err(CoreError::InvalidArgument(
            "shooting-frame geometry is outside bounds",
        ));
    }
    for point in corners(input)? {
        if point.x_milli.unsigned_abs() > MAX_SHOOTING_FRAME_COORDINATE_MILLI as u64
            || point.y_milli.unsigned_abs() > MAX_SHOOTING_FRAME_COORDINATE_MILLI as u64
        {
            return Err(CoreError::InvalidArgument(
                "shooting-frame corner is outside bounds",
            ));
        }
    }
    Ok(())
}

pub(crate) fn corners(input: ShootingFrameInput) -> Result<[ShootingFramePoint; 4], CoreError> {
    Ok([
        rotated_local(
            input,
            -i128::from(input.width_milli),
            -i128::from(input.height_milli),
        )?,
        rotated_local(
            input,
            i128::from(input.width_milli),
            -i128::from(input.height_milli),
        )?,
        rotated_local(
            input,
            i128::from(input.width_milli),
            i128::from(input.height_milli),
        )?,
        rotated_local(
            input,
            -i128::from(input.width_milli),
            i128::from(input.height_milli),
        )?,
    ])
}

fn anchor_point(
    input: ShootingFrameInput,
    anchor: ShootingFrameAnchor,
) -> Result<ShootingFramePoint, CoreError> {
    match anchor {
        ShootingFrameAnchor::Center => Ok(ShootingFramePoint {
            x_milli: input.center_x_milli,
            y_milli: input.center_y_milli,
        }),
        ShootingFrameAnchor::TopLeft => rotated_local(
            input,
            -i128::from(input.width_milli),
            -i128::from(input.height_milli),
        ),
        ShootingFrameAnchor::TopRight => rotated_local(
            input,
            i128::from(input.width_milli),
            -i128::from(input.height_milli),
        ),
        ShootingFrameAnchor::BottomLeft => rotated_local(
            input,
            -i128::from(input.width_milli),
            i128::from(input.height_milli),
        ),
        ShootingFrameAnchor::BottomRight => rotated_local(
            input,
            i128::from(input.width_milli),
            i128::from(input.height_milli),
        ),
    }
}

fn rotated_local(
    input: ShootingFrameInput,
    doubled_local_x: i128,
    doubled_local_y: i128,
) -> Result<ShootingFramePoint, CoreError> {
    let (cosine, sine) = sin_cos_turns(input.rotation_turns);
    let denominator = 2 * Q30_ONE;
    let x = div_round_ties_even_i128(
        doubled_local_x * i128::from(cosine) - doubled_local_y * i128::from(sine),
        denominator,
    )
    .and_then(|offset| i128::from(input.center_x_milli).checked_add(offset))
    .and_then(|value| i64::try_from(value).ok())
    .ok_or(CoreError::InvalidArgument(
        "shooting-frame corner calculation overflows",
    ))?;
    let y = div_round_ties_even_i128(
        doubled_local_x * i128::from(sine) + doubled_local_y * i128::from(cosine),
        denominator,
    )
    .and_then(|offset| i128::from(input.center_y_milli).checked_add(offset))
    .and_then(|value| i64::try_from(value).ok())
    .ok_or(CoreError::InvalidArgument(
        "shooting-frame corner calculation overflows",
    ))?;
    Ok(ShootingFramePoint {
        x_milli: x,
        y_milli: y,
    })
}

fn inverse_local(
    input: ShootingFrameInput,
    point: ShootingFramePoint,
) -> Result<ShootingFramePoint, CoreError> {
    let dx = i128::from(point.x_milli) - i128::from(input.center_x_milli);
    let dy = i128::from(point.y_milli) - i128::from(input.center_y_milli);
    let (cosine, sine) = sin_cos_turns(input.rotation_turns);
    let x = div_round_ties_even_i128(dx * i128::from(cosine) + dy * i128::from(sine), Q30_ONE)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(CoreError::InvalidArgument(
            "shooting-frame hit calculation overflows",
        ))?;
    let y = div_round_ties_even_i128(-dx * i128::from(sine) + dy * i128::from(cosine), Q30_ONE)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(CoreError::InvalidArgument(
            "shooting-frame hit calculation overflows",
        ))?;
    Ok(ShootingFramePoint {
        x_milli: x,
        y_milli: y,
    })
}

fn validate_frame_id(frame_id: u64) -> Result<(), CoreError> {
    if frame_id == 0 || frame_id > MAX_PERSISTENT_NUMERIC_ID {
        Err(CoreError::InvalidArgument("shooting-frame ID is invalid"))
    } else {
        Ok(())
    }
}

pub(crate) fn mirror_shooting_frame(
    frame: &mut ShootingFrameObject,
    document_size: DocumentSizeU32,
    axis: MirrorAxis,
) -> Result<(), CoreError> {
    let width_milli =
        i64::from(document_size.width)
            .checked_mul(1_000)
            .ok_or(CoreError::InvalidArgument(
                "shooting-frame mirror dimensions overflow",
            ))?;
    let height_milli =
        i64::from(document_size.height)
            .checked_mul(1_000)
            .ok_or(CoreError::InvalidArgument(
                "shooting-frame mirror dimensions overflow",
            ))?;
    match axis {
        MirrorAxis::Horizontal => {
            frame.input.center_x_milli =
                width_milli.checked_sub(frame.input.center_x_milli).ok_or(
                    CoreError::InvalidArgument("shooting-frame mirror coordinate overflows"),
                )?;
            frame.input.anchor = swap_horizontal_anchor(frame.input.anchor);
        }
        MirrorAxis::Vertical => {
            frame.input.center_y_milli =
                height_milli.checked_sub(frame.input.center_y_milli).ok_or(
                    CoreError::InvalidArgument("shooting-frame mirror coordinate overflows"),
                )?;
            frame.input.anchor = swap_vertical_anchor(frame.input.anchor);
        }
    }
    frame.input.rotation_turns = frame.input.rotation_turns.wrapping_neg();
    validate_shooting_frame_input(frame.input)
}

pub(crate) fn rotate_shooting_frame(
    frame: &mut ShootingFrameObject,
    before_size: DocumentSizeU32,
    direction: RotateDirection,
) -> Result<(), CoreError> {
    let old_x = frame.input.center_x_milli;
    let old_y = frame.input.center_y_milli;
    let width_milli = i64::from(before_size.width) * 1_000;
    let height_milli = i64::from(before_size.height) * 1_000;
    match direction {
        RotateDirection::Left90 => {
            frame.input.center_x_milli = old_y;
            frame.input.center_y_milli =
                width_milli
                    .checked_sub(old_x)
                    .ok_or(CoreError::InvalidArgument(
                        "shooting-frame rotation coordinate overflows",
                    ))?;
            frame.input.rotation_turns = frame.input.rotation_turns.wrapping_sub(1 << 30);
        }
        RotateDirection::Right90 => {
            frame.input.center_x_milli =
                height_milli
                    .checked_sub(old_y)
                    .ok_or(CoreError::InvalidArgument(
                        "shooting-frame rotation coordinate overflows",
                    ))?;
            frame.input.center_y_milli = old_x;
            frame.input.rotation_turns = frame.input.rotation_turns.wrapping_add(1 << 30);
        }
    }
    validate_shooting_frame_input(frame.input)
}

pub(crate) fn validate_resample_shooting_frame(
    frame: Option<ShootingFrameObject>,
    before_size: DocumentSizeU32,
    after_size: DocumentSizeU32,
) -> Result<(), CoreError> {
    let Some(frame) = frame else {
        return Ok(());
    };
    let uniform = u128::from(after_size.width) * u128::from(before_size.height)
        == u128::from(after_size.height) * u128::from(before_size.width);
    let quarter_aligned = frame.input.rotation_turns & 0x3fff_ffff == 0;
    if uniform || quarter_aligned {
        Ok(())
    } else {
        Err(CoreError::InvalidArgument(
            "non-uniform resampling would shear the angled shooting frame",
        ))
    }
}

pub(crate) fn resample_shooting_frame(
    frame: &mut ShootingFrameObject,
    before_size: DocumentSizeU32,
    after_size: DocumentSizeU32,
) -> Result<(), CoreError> {
    validate_resample_shooting_frame(Some(*frame), before_size, after_size)?;
    frame.input.center_x_milli = scale_i64_ratio(
        frame.input.center_x_milli,
        after_size.width,
        before_size.width,
    )?;
    frame.input.center_y_milli = scale_i64_ratio(
        frame.input.center_y_milli,
        after_size.height,
        before_size.height,
    )?;
    let uniform = u128::from(after_size.width) * u128::from(before_size.height)
        == u128::from(after_size.height) * u128::from(before_size.width);
    if uniform || frame.input.rotation_turns >> 30 & 1 == 0 {
        frame.input.width_milli =
            scale_u64_ratio(frame.input.width_milli, after_size.width, before_size.width)?;
        frame.input.height_milli = scale_u64_ratio(
            frame.input.height_milli,
            after_size.height,
            before_size.height,
        )?;
    } else {
        frame.input.width_milli = scale_u64_ratio(
            frame.input.width_milli,
            after_size.height,
            before_size.height,
        )?;
        frame.input.height_milli = scale_u64_ratio(
            frame.input.height_milli,
            after_size.width,
            before_size.width,
        )?;
    }
    validate_shooting_frame_input(frame.input)
}

pub(crate) fn translate_shooting_frame(
    frame: &mut ShootingFrameObject,
    offset: DocumentOffsetI32,
) -> Result<(), CoreError> {
    frame.input.center_x_milli = frame
        .input
        .center_x_milli
        .checked_add(i64::from(offset.x) * 1_000)
        .ok_or(CoreError::InvalidArgument(
            "shooting-frame translation overflows",
        ))?;
    frame.input.center_y_milli = frame
        .input
        .center_y_milli
        .checked_add(i64::from(offset.y) * 1_000)
        .ok_or(CoreError::InvalidArgument(
            "shooting-frame translation overflows",
        ))?;
    validate_shooting_frame_input(frame.input)
}

fn scale_i64_ratio(value: i64, numerator: u32, denominator: u32) -> Result<i64, CoreError> {
    div_round_ties_even_i128(
        i128::from(value) * i128::from(numerator),
        i128::from(denominator),
    )
    .and_then(|result| i64::try_from(result).ok())
    .ok_or(CoreError::InvalidArgument(
        "shooting-frame scale coordinate overflows",
    ))
}

fn scale_u64_ratio(value: u64, numerator: u32, denominator: u32) -> Result<u64, CoreError> {
    div_round_ties_even_i128(
        i128::from(value) * i128::from(numerator),
        i128::from(denominator),
    )
    .and_then(|result| u64::try_from(result).ok())
    .ok_or(CoreError::InvalidArgument(
        "shooting-frame scale size overflows",
    ))
}

const fn swap_horizontal_anchor(anchor: ShootingFrameAnchor) -> ShootingFrameAnchor {
    match anchor {
        ShootingFrameAnchor::TopLeft => ShootingFrameAnchor::TopRight,
        ShootingFrameAnchor::TopRight => ShootingFrameAnchor::TopLeft,
        ShootingFrameAnchor::Center => ShootingFrameAnchor::Center,
        ShootingFrameAnchor::BottomLeft => ShootingFrameAnchor::BottomRight,
        ShootingFrameAnchor::BottomRight => ShootingFrameAnchor::BottomLeft,
    }
}

const fn swap_vertical_anchor(anchor: ShootingFrameAnchor) -> ShootingFrameAnchor {
    match anchor {
        ShootingFrameAnchor::TopLeft => ShootingFrameAnchor::BottomLeft,
        ShootingFrameAnchor::TopRight => ShootingFrameAnchor::BottomRight,
        ShootingFrameAnchor::Center => ShootingFrameAnchor::Center,
        ShootingFrameAnchor::BottomLeft => ShootingFrameAnchor::TopLeft,
        ShootingFrameAnchor::BottomRight => ShootingFrameAnchor::TopRight,
    }
}
