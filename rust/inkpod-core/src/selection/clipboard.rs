use super::*;

pub(crate) fn selection_from_rect(
    width: u32,
    height: u32,
    rect: RectI32,
    is_cancelled: &mut (impl FnMut() -> bool + ?Sized),
) -> Result<TileRaster, CoreError> {
    if rect.width <= 0 || rect.height <= 0 || rect.x < 0 || rect.y < 0 {
        return Err(CoreError::InvalidArgument(
            "selection rectangle must have a nonnegative origin and positive size",
        ));
    }
    let right = u32::try_from(rect.x)
        .ok()
        .and_then(|x| x.checked_add(rect.width as u32))
        .ok_or(CoreError::InvalidArgument("selection rectangle overflows"))?;
    let bottom = u32::try_from(rect.y)
        .ok()
        .and_then(|y| y.checked_add(rect.height as u32))
        .ok_or(CoreError::InvalidArgument("selection rectangle overflows"))?;
    if right > width || bottom > height {
        return Err(CoreError::InvalidArgument(
            "selection rectangle is outside the document",
        ));
    }
    let mut selection = TileRaster::new(width, height, PixelFormat::BinaryMask8)?;
    let mut work = 0_u64;
    for y in rect.y as u32..bottom {
        for x in rect.x as u32..right {
            work = work
                .checked_add(1)
                .ok_or(CoreError::InvalidArgument("selection work size overflows"))?;
            if work % 1_024 == 0 && is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            selection.set_pixel(x, y, PixelValue::Binary(255), 0)?;
        }
    }
    if is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(selection)
}

pub(crate) fn paste_value(
    destination: PixelValue,
    source: PixelValue,
    kind: PlaneType,
) -> Result<PixelValue, CoreError> {
    match (kind, destination, source) {
        (PlaneType::MainLine, PixelValue::Binary(left), PixelValue::Binary(right)) => {
            Ok(PixelValue::Binary(left.max(right)))
        }
        (PlaneType::MainLine, PixelValue::Grayscale8(left), PixelValue::Grayscale8(right)) => {
            Ok(PixelValue::Grayscale8(left.max(right)))
        }
        (PlaneType::MainLine, PixelValue::Grayscale16(left), PixelValue::Grayscale16(right)) => {
            Ok(PixelValue::Grayscale16(left.max(right)))
        }
        (_, PixelValue::Rgba(left), PixelValue::Rgba(right)) => {
            Ok(PixelValue::Rgba(blend_rgba_over(left, right)))
        }
        (_, PixelValue::Rgba16(left), PixelValue::Rgba16(right)) => {
            Ok(PixelValue::Rgba16(blend_rgba16_over(left, right)))
        }
        (_, left, right) if std::mem::discriminant(&left) == std::mem::discriminant(&right) => {
            Ok(if right.is_transparent() { left } else { right })
        }
        _ => Err(CoreError::InvalidArgument(
            "clipboard pixel type does not match destination",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba16_paste_uses_source_over() {
        assert_eq!(
            paste_value(
                PixelValue::Rgba16([u16::MAX, 0, 0, u16::MAX]),
                PixelValue::Rgba16([0, 0, u16::MAX, 32_768]),
                PlaneType::Raster,
            )
            .unwrap(),
            PixelValue::Rgba16([32_767, 0, 32_768, u16::MAX])
        );
    }
}
