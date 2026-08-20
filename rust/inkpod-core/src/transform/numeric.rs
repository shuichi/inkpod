use crate::*;

pub(super) fn resize_anchor_offset(
    source_size: DocumentSizeU32,
    destination_size: DocumentSizeU32,
    anchor: ResizeAnchor,
) -> Result<DocumentOffsetI32, CoreError> {
    let difference_x = i64::from(destination_size.width) - i64::from(source_size.width);
    let difference_y = i64::from(destination_size.height) - i64::from(source_size.height);
    let offset_x = match anchor {
        ResizeAnchor::TopLeft | ResizeAnchor::BottomLeft => 0,
        ResizeAnchor::Center => difference_x / 2,
        ResizeAnchor::TopRight | ResizeAnchor::BottomRight => difference_x,
    };
    let offset_y = match anchor {
        ResizeAnchor::TopLeft | ResizeAnchor::TopRight => 0,
        ResizeAnchor::Center => difference_y / 2,
        ResizeAnchor::BottomLeft | ResizeAnchor::BottomRight => difference_y,
    };
    Ok(DocumentOffsetI32 {
        x: i32::try_from(offset_x)
            .map_err(|_| CoreError::InvalidArgument("horizontal anchor offset overflowed"))?,
        y: i32::try_from(offset_y)
            .map_err(|_| CoreError::InvalidArgument("vertical anchor offset overflowed"))?,
    })
}

pub(super) fn checked_scaled_i32(value: i32, scale: f64) -> Result<i32, CoreError> {
    let scaled = f64::from(value) * scale;
    if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        return Err(CoreError::InvalidArgument("scaled coordinate overflowed"));
    }
    Ok(scaled.round() as i32)
}

pub(super) fn checked_scaled_spacing(value: u32, scale: f64) -> Result<u32, CoreError> {
    let scaled = f64::from(value) * scale;
    if !scaled.is_finite() || scaled > f64::from(u32::MAX) {
        return Err(CoreError::InvalidArgument("scaled spacing overflowed"));
    }
    Ok(scaled.round().max(1.0) as u32)
}

pub(super) fn checked_scaled_u32(value: u32, scale: f64) -> Result<u32, CoreError> {
    let scaled = f64::from(value) * scale;
    if !scaled.is_finite() || scaled > f64::from(u32::MAX) {
        return Err(CoreError::InvalidArgument(
            "scaled unsigned value overflowed",
        ));
    }
    Ok(scaled.round().max(0.0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_offsets_and_checked_scaling_cover_growth_shrink_and_overflow() {
        let source = DocumentSizeU32::new(10, 8);
        assert_eq!(
            resize_anchor_offset(source, DocumentSizeU32::new(16, 12), ResizeAnchor::TopLeft)
                .unwrap(),
            DocumentOffsetI32 { x: 0, y: 0 }
        );
        assert_eq!(
            resize_anchor_offset(source, DocumentSizeU32::new(16, 12), ResizeAnchor::Center)
                .unwrap(),
            DocumentOffsetI32 { x: 3, y: 2 }
        );
        assert_eq!(
            resize_anchor_offset(source, DocumentSizeU32::new(7, 5), ResizeAnchor::Center).unwrap(),
            DocumentOffsetI32 { x: -1, y: -1 }
        );
        assert_eq!(
            resize_anchor_offset(
                source,
                DocumentSizeU32::new(16, 12),
                ResizeAnchor::BottomRight,
            )
            .unwrap(),
            DocumentOffsetI32 { x: 6, y: 4 }
        );
        assert_eq!(checked_scaled_i32(-3, 0.5).unwrap(), -2);
        assert_eq!(checked_scaled_spacing(1, 0.0).unwrap(), 1);
        assert_eq!(checked_scaled_u32(5, -1.0).unwrap(), 0);
        assert!(checked_scaled_i32(i32::MAX, 2.0).is_err());
        assert!(checked_scaled_spacing(u32::MAX, 2.0).is_err());
        assert!(checked_scaled_spacing(1, f64::NAN).is_err());
        assert!(checked_scaled_u32(1, f64::NAN).is_err());
    }
}
