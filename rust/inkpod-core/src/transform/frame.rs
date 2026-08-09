use super::numeric::*;
use crate::*;

pub(super) fn clamp_margins(margins: &mut Margins, width: u32, height: u32) {
    margins.left = margins.left.min(width);
    margins.right = margins.right.min(width.saturating_sub(margins.left));
    margins.top = margins.top.min(height);
    margins.bottom = margins.bottom.min(height.saturating_sub(margins.top));
}

fn translate_document_rect(
    rect: DocumentRectI32,
    offset: DocumentOffsetI32,
) -> Result<DocumentRectI32, CoreError> {
    Ok(DocumentRectI32 {
        origin: DocumentPointI32 {
            x: rect
                .origin
                .x
                .checked_add(offset.x)
                .ok_or(CoreError::InvalidArgument("translated frame overflowed"))?,
            y: rect
                .origin
                .y
                .checked_add(offset.y)
                .ok_or(CoreError::InvalidArgument("translated frame overflowed"))?,
        },
        ..rect
    })
}

fn scale_document_rect(
    rect: DocumentRectI32,
    scale: DocumentScaleF64,
) -> Result<DocumentRectI32, CoreError> {
    Ok(DocumentRectI32 {
        origin: DocumentPointI32 {
            x: checked_scaled_i32(rect.origin.x, scale.x)?,
            y: checked_scaled_i32(rect.origin.y, scale.y)?,
        },
        width: checked_scaled_i32(rect.width, scale.x)?.max(1),
        height: checked_scaled_i32(rect.height, scale.y)?.max(1),
    })
}

fn rotate_document_rect(
    rect: DocumentRectI32,
    document_width: i32,
    document_height: i32,
    direction: RotateDirection,
) -> DocumentRectI32 {
    match direction {
        RotateDirection::Left90 => DocumentRectI32 {
            origin: DocumentPointI32 {
                x: rect.origin.y,
                y: document_width - rect.origin.x - rect.width,
            },
            width: rect.height,
            height: rect.width,
        },
        RotateDirection::Right90 => DocumentRectI32 {
            origin: DocumentPointI32 {
                x: document_height - rect.origin.y - rect.height,
                y: rect.origin.x,
            },
            width: rect.height,
            height: rect.width,
        },
    }
}

fn mirror_document_rect(
    rect: DocumentRectI32,
    document_width: i32,
    document_height: i32,
    axis: MirrorAxis,
) -> DocumentRectI32 {
    match axis {
        MirrorAxis::Horizontal => DocumentRectI32 {
            origin: DocumentPointI32 {
                x: document_width - rect.origin.x - rect.width,
                y: rect.origin.y,
            },
            ..rect
        },
        MirrorAxis::Vertical => DocumentRectI32 {
            origin: DocumentPointI32 {
                x: rect.origin.x,
                y: document_height - rect.origin.y - rect.height,
            },
            ..rect
        },
    }
}

pub(super) fn translate_frame_metadata(
    frames: &mut FrameMetadata,
    offset: DocumentOffsetI32,
) -> Result<(), CoreError> {
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
        &mut frames.shooting_frame,
        &mut frames.maximum_close_frame,
    ] {
        let rect = DocumentRectI32::from_public(*frame);
        *frame = translate_document_rect(rect, offset)?.into_public();
    }
    Ok(())
}

pub(super) fn scale_frame_metadata(
    frames: &mut FrameMetadata,
    scale: DocumentScaleF64,
) -> Result<(), CoreError> {
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
        &mut frames.shooting_frame,
        &mut frames.maximum_close_frame,
    ] {
        let rect = DocumentRectI32::from_public(*frame);
        *frame = scale_document_rect(rect, scale)?.into_public();
    }
    frames.margins.left = checked_scaled_u32(frames.margins.left, scale.x)?;
    frames.margins.right = checked_scaled_u32(frames.margins.right, scale.x)?;
    frames.margins.top = checked_scaled_u32(frames.margins.top, scale.y)?;
    frames.margins.bottom = checked_scaled_u32(frames.margins.bottom, scale.y)?;
    Ok(())
}

pub(super) fn rotate_frame_metadata(
    frames: &mut FrameMetadata,
    document_size: DocumentSizeU32,
    direction: RotateDirection,
) -> Result<(), CoreError> {
    let width = i32::try_from(document_size.width)
        .map_err(|_| CoreError::InvalidState("document width exceeds frame range"))?;
    let height = i32::try_from(document_size.height)
        .map_err(|_| CoreError::InvalidState("document height exceeds frame range"))?;
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
        &mut frames.shooting_frame,
        &mut frames.maximum_close_frame,
    ] {
        let previous = DocumentRectI32::from_public(*frame);
        *frame = rotate_document_rect(previous, width, height, direction).into_public();
    }
    let margins = frames.margins;
    frames.margins = match direction {
        RotateDirection::Left90 => Margins {
            left: margins.top,
            top: margins.right,
            right: margins.bottom,
            bottom: margins.left,
        },
        RotateDirection::Right90 => Margins {
            left: margins.bottom,
            top: margins.left,
            right: margins.top,
            bottom: margins.right,
        },
    };
    Ok(())
}

pub(super) fn rotate_guides(
    guides: &mut [Guide],
    document_size: DocumentSizeU32,
    direction: RotateDirection,
) -> Result<(), CoreError> {
    let width = i32::try_from(document_size.width)
        .map_err(|_| CoreError::InvalidState("document width exceeds guide range"))?;
    let height = i32::try_from(document_size.height)
        .map_err(|_| CoreError::InvalidState("document height exceeds guide range"))?;
    for guide in guides {
        let previous = *guide;
        match (direction, previous.axis) {
            (RotateDirection::Left90, GuideAxis::Vertical) => {
                guide.axis = GuideAxis::Horizontal;
                guide.position = width - previous.position;
            }
            (RotateDirection::Left90, GuideAxis::Horizontal) => {
                guide.axis = GuideAxis::Vertical;
                guide.position = previous.position;
            }
            (RotateDirection::Right90, GuideAxis::Vertical) => {
                guide.axis = GuideAxis::Horizontal;
                guide.position = previous.position;
            }
            (RotateDirection::Right90, GuideAxis::Horizontal) => {
                guide.axis = GuideAxis::Vertical;
                guide.position = height - previous.position;
            }
        }
    }
    Ok(())
}

pub(super) fn mirror_frame_metadata(
    frames: &mut FrameMetadata,
    document_size: DocumentSizeU32,
    axis: MirrorAxis,
) -> Result<(), CoreError> {
    let width = i32::try_from(document_size.width)
        .map_err(|_| CoreError::InvalidState("document width exceeds frame range"))?;
    let height = i32::try_from(document_size.height)
        .map_err(|_| CoreError::InvalidState("document height exceeds frame range"))?;
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
        &mut frames.shooting_frame,
        &mut frames.maximum_close_frame,
    ] {
        let rect = DocumentRectI32::from_public(*frame);
        *frame = mirror_document_rect(rect, width, height, axis).into_public();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> FrameMetadata {
        FrameMetadata {
            hundred_frame: RectI32 {
                x: 1,
                y: 2,
                width: 6,
                height: 3,
            },
            reference_frame: RectI32 {
                x: 2,
                y: 1,
                width: 5,
                height: 4,
            },
            drawing_frame: RectI32 {
                x: 0,
                y: 0,
                width: 10,
                height: 6,
            },
            safe_frame: RectI32 {
                x: 3,
                y: 2,
                width: 4,
                height: 2,
            },
            shooting_frame: RectI32 {
                x: 1,
                y: 1,
                width: 8,
                height: 4,
            },
            maximum_close_frame: RectI32 {
                x: 2,
                y: 2,
                width: 5,
                height: 2,
            },
            margins: Margins {
                left: 1,
                top: 2,
                right: 3,
                bottom: 4,
            },
        }
    }
    #[test]
    fn frame_and_guide_rotation_round_trip_preserves_geometry_and_margins() {
        let original_frames = frames();
        let mut rotated_frames = original_frames;
        rotate_frame_metadata(
            &mut rotated_frames,
            DocumentSizeU32::new(10, 6),
            RotateDirection::Left90,
        )
        .unwrap();
        rotate_frame_metadata(
            &mut rotated_frames,
            DocumentSizeU32::new(6, 10),
            RotateDirection::Right90,
        )
        .unwrap();
        assert_eq!(rotated_frames, original_frames);

        let original_guides = vec![
            Guide {
                id: 1,
                axis: GuideAxis::Vertical,
                position: 2,
            },
            Guide {
                id: 2,
                axis: GuideAxis::Horizontal,
                position: 3,
            },
        ];
        let mut rotated_guides = original_guides.clone();
        rotate_guides(
            &mut rotated_guides,
            DocumentSizeU32::new(10, 6),
            RotateDirection::Left90,
        )
        .unwrap();
        rotate_guides(
            &mut rotated_guides,
            DocumentSizeU32::new(6, 10),
            RotateDirection::Right90,
        )
        .unwrap();
        assert_eq!(rotated_guides, original_guides);

        let mut mirrored = original_frames;
        let document_size = DocumentSizeU32::new(10, 6);
        mirror_frame_metadata(&mut mirrored, document_size, MirrorAxis::Horizontal).unwrap();
        mirror_frame_metadata(&mut mirrored, document_size, MirrorAxis::Horizontal).unwrap();
        mirror_frame_metadata(&mut mirrored, document_size, MirrorAxis::Vertical).unwrap();
        mirror_frame_metadata(&mut mirrored, document_size, MirrorAxis::Vertical).unwrap();
        assert_eq!(mirrored, original_frames);
    }
}
