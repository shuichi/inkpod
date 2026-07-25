//! Destructive document transforms.

use super::*;
use crate::document::bounded_document_pixels;
use crate::selection::paste_value;

impl Core {
    pub fn mirror_document(&mut self, axis: MirrorAxis) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        for plane in after
            .layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
        {
            plane.raster = mirror_raster(&plane.raster, axis, revision)?;
        }
        after.selection = mirror_raster(&after.selection, axis, revision)?;
        mirror_frame_metadata(&mut after.frames, after.width, after.height, axis)?;
        for guide in &mut after.guides {
            match (axis, guide.axis) {
                (MirrorAxis::Horizontal, GuideAxis::Vertical) => {
                    guide.position = i32::try_from(after.width).map_err(|_| {
                        CoreError::InvalidState("document width exceeds guide range")
                    })? - guide.position;
                }
                (MirrorAxis::Vertical, GuideAxis::Horizontal) => {
                    guide.position = i32::try_from(after.height).map_err(|_| {
                        CoreError::InvalidState("document height exceeds guide range")
                    })? - guide.position;
                }
                _ => {}
            }
        }
        let width_milli = checked_dimension_milli(after.width)?;
        let height_milli = checked_dimension_milli(after.height)?;
        after.vector.transform_coordinates(
            |point| {
                Ok(match axis {
                    MirrorAxis::Horizontal => VectorFixedPoint {
                        x_milli: width_milli.checked_sub(point.x_milli).ok_or(
                            CoreError::InvalidArgument("mirrored vector point overflowed"),
                        )?,
                        y_milli: point.y_milli,
                    },
                    MirrorAxis::Vertical => VectorFixedPoint {
                        x_milli: point.x_milli,
                        y_milli: height_milli.checked_sub(point.y_milli).ok_or(
                            CoreError::InvalidArgument("mirrored vector point overflowed"),
                        )?,
                    },
                })
            },
            1.0,
        )?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn rotate_document(
        &mut self,
        direction: RotateDirection,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        for plane in after
            .layers
            .iter_mut()
            .flat_map(|layer| layer.planes.iter_mut())
        {
            plane.raster = rotate_raster(&plane.raster, direction, revision)?;
        }
        after.selection = rotate_raster(&after.selection, direction, revision)?;
        rotate_frame_metadata(&mut after.frames, before.width, before.height, direction)?;
        rotate_guides(&mut after.guides, before.width, before.height, direction)?;
        let old_grid = after.grid;
        after.grid.origin_x = match direction {
            RotateDirection::Left90 => old_grid.origin_y,
            RotateDirection::Right90 => {
                i32::try_from(before.height)
                    .map_err(|_| CoreError::InvalidState("document height exceeds grid range"))?
                    - old_grid.origin_y
            }
        };
        after.grid.origin_y = match direction {
            RotateDirection::Left90 => {
                i32::try_from(before.width)
                    .map_err(|_| CoreError::InvalidState("document width exceeds grid range"))?
                    - old_grid.origin_x
            }
            RotateDirection::Right90 => old_grid.origin_x,
        };
        after.grid.spacing_x = old_grid.spacing_y;
        after.grid.spacing_y = old_grid.spacing_x;
        let old_width_milli = checked_dimension_milli(before.width)?;
        let old_height_milli = checked_dimension_milli(before.height)?;
        after.vector.transform_coordinates(
            |point| {
                Ok(match direction {
                    RotateDirection::Left90 => VectorFixedPoint {
                        x_milli: point.y_milli,
                        y_milli: old_width_milli.checked_sub(point.x_milli).ok_or(
                            CoreError::InvalidArgument("rotated vector point overflowed"),
                        )?,
                    },
                    RotateDirection::Right90 => VectorFixedPoint {
                        x_milli: old_height_milli.checked_sub(point.y_milli).ok_or(
                            CoreError::InvalidArgument("rotated vector point overflowed"),
                        )?,
                        y_milli: point.x_milli,
                    },
                })
            },
            1.0,
        )?;
        after.width = before.height;
        after.height = before.width;
        after.dpi_x_milli = before.dpi_y_milli;
        after.dpi_y_milli = before.dpi_x_milli;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn resize_document(
        &mut self,
        resize: DocumentResize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        bounded_document_pixels(resize.width, resize.height)?;
        if resize.width == 0
            || resize.height == 0
            || resize.dpi_x_milli == 0
            || resize.dpi_y_milli == 0
        {
            return Err(CoreError::InvalidArgument(
                "document dimensions and DPI must be nonzero",
            ));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.width == resize.width
            && before.height == resize.height
            && before.dpi_x_milli == resize.dpi_x_milli
            && before.dpi_y_milli == resize.dpi_y_milli
        {
            return Ok(self.noop_outcome());
        }
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        if resize.resample {
            for plane in after
                .layers
                .iter_mut()
                .flat_map(|layer| layer.planes.iter_mut())
            {
                plane.raster =
                    resample_raster_nearest(&plane.raster, resize.width, resize.height, revision)?;
            }
            after.selection =
                resample_raster_nearest(&after.selection, resize.width, resize.height, revision)?;
            let scale_x = f64::from(resize.width) / f64::from(before.width);
            let scale_y = f64::from(resize.height) / f64::from(before.height);
            scale_frame_metadata(&mut after.frames, scale_x, scale_y)?;
            for guide in &mut after.guides {
                guide.position = checked_scaled_i32(
                    guide.position,
                    if guide.axis == GuideAxis::Vertical {
                        scale_x
                    } else {
                        scale_y
                    },
                )?;
            }
            after.grid.origin_x = checked_scaled_i32(after.grid.origin_x, scale_x)?;
            after.grid.origin_y = checked_scaled_i32(after.grid.origin_y, scale_y)?;
            after.grid.spacing_x = checked_scaled_spacing(after.grid.spacing_x, scale_x)?;
            after.grid.spacing_y = checked_scaled_spacing(after.grid.spacing_y, scale_y)?;
            after.vector.transform_coordinates(
                |point| {
                    Ok(VectorFixedPoint {
                        x_milli: checked_scaled_i32(point.x_milli, scale_x)?,
                        y_milli: checked_scaled_i32(point.y_milli, scale_y)?,
                    })
                },
                (scale_x.abs() + scale_y.abs()) / 2.0,
            )?;
        } else {
            let (offset_x, offset_y) = resize_anchor_offset(
                before.width,
                before.height,
                resize.width,
                resize.height,
                resize.anchor,
            )?;
            for plane in after
                .layers
                .iter_mut()
                .flat_map(|layer| layer.planes.iter_mut())
            {
                plane.raster = place_raster(
                    &plane.raster,
                    resize.width,
                    resize.height,
                    offset_x,
                    offset_y,
                    revision,
                )?;
            }
            after.selection = place_raster(
                &after.selection,
                resize.width,
                resize.height,
                offset_x,
                offset_y,
                revision,
            )?;
            translate_frame_metadata(&mut after.frames, offset_x, offset_y)?;
            for guide in &mut after.guides {
                guide.position = guide
                    .position
                    .checked_add(if guide.axis == GuideAxis::Vertical {
                        offset_x
                    } else {
                        offset_y
                    })
                    .ok_or(CoreError::InvalidArgument("translated guide overflowed"))?;
            }
            after.grid.origin_x =
                after
                    .grid
                    .origin_x
                    .checked_add(offset_x)
                    .ok_or(CoreError::InvalidArgument(
                        "translated grid origin overflowed",
                    ))?;
            after.grid.origin_y =
                after
                    .grid
                    .origin_y
                    .checked_add(offset_y)
                    .ok_or(CoreError::InvalidArgument(
                        "translated grid origin overflowed",
                    ))?;
            let offset_x_milli = offset_x
                .checked_mul(1_000)
                .ok_or(CoreError::InvalidArgument("vector translation overflowed"))?;
            let offset_y_milli = offset_y
                .checked_mul(1_000)
                .ok_or(CoreError::InvalidArgument("vector translation overflowed"))?;
            after.vector.transform_coordinates(
                |point| {
                    Ok(VectorFixedPoint {
                        x_milli: point.x_milli.checked_add(offset_x_milli).ok_or(
                            CoreError::InvalidArgument("translated vector point overflowed"),
                        )?,
                        y_milli: point.y_milli.checked_add(offset_y_milli).ok_or(
                            CoreError::InvalidArgument("translated vector point overflowed"),
                        )?,
                    })
                },
                1.0,
            )?;
        }
        after.guides.retain(|guide| {
            let limit = if guide.axis == GuideAxis::Vertical {
                resize.width
            } else {
                resize.height
            };
            guide.position >= 0
                && u32::try_from(guide.position).is_ok_and(|position| position <= limit)
        });
        clamp_margins(&mut after.frames.margins, resize.width, resize.height);
        after.width = resize.width;
        after.height = resize.height;
        after.dpi_x_milli = resize.dpi_x_milli;
        after.dpi_y_milli = resize.dpi_y_milli;
        self.commit_document_edit_with_revision(before, after, revision)
    }
}

// Shared implementation helpers for this responsibility.

pub(super) fn convert_main_line_raster(
    source: &TileRaster,
    grayscale: bool,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(
        source.width(),
        source.height(),
        if grayscale {
            PixelFormat::Grayscale8
        } else {
            PixelFormat::BinaryMask8
        },
    )?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = match source.pixel(x, y)? {
                PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                PixelValue::Grayscale16(value) => ((u32::from(value) + 128) / 257) as u8,
                _ => return Err(CoreError::InvalidState("main-line plane format is invalid")),
            };
            let value = if grayscale {
                PixelValue::Grayscale8(value)
            } else {
                PixelValue::Binary(if value >= 128 { 255 } else { 0 })
            };
            destination.set_pixel(x, y, value, revision)?;
        }
    }
    Ok(destination)
}

pub(super) fn merge_raster(
    destination: &mut TileRaster,
    source: &TileRaster,
    revision: u64,
) -> Result<(), CoreError> {
    if destination.width() != source.width()
        || destination.height() != source.height()
        || destination.format() != source.format()
    {
        return Err(CoreError::InvalidArgument("merge raster formats differ"));
    }
    bounded_document_pixels(source.width(), source.height())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let source_value = source.pixel(x, y)?;
            if source_value.is_zero() {
                continue;
            }
            let before = destination.pixel(x, y)?;
            let after = paste_value(
                before,
                source_value,
                match source.format() {
                    PixelFormat::BinaryMask8
                    | PixelFormat::Grayscale8
                    | PixelFormat::Grayscale16 => PlaneType::MainLine,
                    _ => PlaneType::Raster,
                },
            )?;
            destination.set_pixel(x, y, after, revision)?;
        }
    }
    Ok(())
}

pub(super) fn mirror_raster(
    source: &TileRaster,
    axis: MirrorAxis,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(source.width(), source.height(), source.format())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = source.pixel(x, y)?;
            if value.is_zero() {
                continue;
            }
            let (destination_x, destination_y) = match axis {
                MirrorAxis::Horizontal => (source.width() - 1 - x, y),
                MirrorAxis::Vertical => (x, source.height() - 1 - y),
            };
            destination.set_pixel(destination_x, destination_y, value, revision)?;
        }
    }
    Ok(destination)
}

pub(super) fn convert_plane_raster(
    source: &TileRaster,
    destination_format: PixelFormat,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.width(), source.height())?;
    let mut destination = TileRaster::new(source.width(), source.height(), destination_format)?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = convert_plane_pixel(source.pixel(x, y)?, destination_format)?;
            if !value.is_zero() {
                destination.set_pixel(x, y, value, revision)?;
            }
        }
    }
    Ok(destination)
}

pub(super) fn convert_plane_pixel(
    source: PixelValue,
    destination_format: PixelFormat,
) -> Result<PixelValue, CoreError> {
    let coverage16 = match source {
        PixelValue::Binary(value) | PixelValue::Grayscale8(value) => u16::from(value) * 257,
        PixelValue::Grayscale16(value) => value,
        PixelValue::Rgba(value) => u16::from(value[3]) * 257,
        PixelValue::Rgba16(value) => value[3],
    };
    let rgba16 = match source {
        PixelValue::Rgba(value) => [
            u16::from(value[0]) * 257,
            u16::from(value[1]) * 257,
            u16::from(value[2]) * 257,
            u16::from(value[3]) * 257,
        ],
        PixelValue::Rgba16(value) => value,
        _ => [0, 0, 0, coverage16],
    };
    match destination_format {
        PixelFormat::BinaryMask8 => Ok(PixelValue::Binary(if coverage16 == 0 { 0 } else { 255 })),
        PixelFormat::Grayscale8 => Ok(PixelValue::Grayscale8((coverage16 / 257) as u8)),
        PixelFormat::Grayscale16 => Ok(PixelValue::Grayscale16(coverage16)),
        PixelFormat::StraightRgba8 => Ok(PixelValue::Rgba([
            (rgba16[0] / 257) as u8,
            (rgba16[1] / 257) as u8,
            (rgba16[2] / 257) as u8,
            (rgba16[3] / 257) as u8,
        ])),
        PixelFormat::StraightRgba16 => Ok(PixelValue::Rgba16(rgba16)),
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidArgument(
            "premultiplied display format cannot be stored in a document plane",
        )),
    }
}

pub(super) fn zero_pixel(format: PixelFormat) -> Result<PixelValue, CoreError> {
    match format {
        PixelFormat::BinaryMask8 => Ok(PixelValue::Binary(0)),
        PixelFormat::Grayscale8 => Ok(PixelValue::Grayscale8(0)),
        PixelFormat::Grayscale16 => Ok(PixelValue::Grayscale16(0)),
        PixelFormat::StraightRgba8 => Ok(PixelValue::Rgba([0; 4])),
        PixelFormat::StraightRgba16 => Ok(PixelValue::Rgba16([0; 4])),
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidArgument(
            "premultiplied display format cannot be stored in a document plane",
        )),
    }
}

pub(super) fn rotate_raster(
    source: &TileRaster,
    direction: RotateDirection,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(source.height(), source.width())?;
    let mut destination = TileRaster::new(source.height(), source.width(), source.format())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let value = source.pixel(x, y)?;
            if value.is_zero() {
                continue;
            }
            let (destination_x, destination_y) = match direction {
                RotateDirection::Left90 => (y, source.width() - 1 - x),
                RotateDirection::Right90 => (source.height() - 1 - y, x),
            };
            destination.set_pixel(destination_x, destination_y, value, revision)?;
        }
    }
    Ok(destination)
}

pub(super) fn place_raster(
    source: &TileRaster,
    width: u32,
    height: u32,
    offset_x: i32,
    offset_y: i32,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(width, height)?;
    let mut destination = TileRaster::new(width, height, source.format())?;
    for y in 0..source.height() {
        for x in 0..source.width() {
            let destination_x = i64::from(x) + i64::from(offset_x);
            let destination_y = i64::from(y) + i64::from(offset_y);
            if destination_x < 0
                || destination_y < 0
                || destination_x >= i64::from(width)
                || destination_y >= i64::from(height)
            {
                continue;
            }
            let value = source.pixel(x, y)?;
            if !value.is_zero() {
                destination.set_pixel(
                    destination_x as u32,
                    destination_y as u32,
                    value,
                    revision,
                )?;
            }
        }
    }
    Ok(destination)
}

pub(super) fn resample_raster_nearest(
    source: &TileRaster,
    width: u32,
    height: u32,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(width, height)?;
    let mut destination = TileRaster::new(width, height, source.format())?;
    for y in 0..height {
        let source_y = ((u64::from(y) * u64::from(source.height())) / u64::from(height))
            .min(u64::from(source.height() - 1)) as u32;
        for x in 0..width {
            let source_x = ((u64::from(x) * u64::from(source.width())) / u64::from(width))
                .min(u64::from(source.width() - 1)) as u32;
            let value = source.pixel(source_x, source_y)?;
            if !value.is_zero() {
                destination.set_pixel(x, y, value, revision)?;
            }
        }
    }
    Ok(destination)
}

pub(super) fn resize_anchor_offset(
    old_width: u32,
    old_height: u32,
    new_width: u32,
    new_height: u32,
    anchor: ResizeAnchor,
) -> Result<(i32, i32), CoreError> {
    let difference_x = i64::from(new_width) - i64::from(old_width);
    let difference_y = i64::from(new_height) - i64::from(old_height);
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
    Ok((
        i32::try_from(offset_x)
            .map_err(|_| CoreError::InvalidArgument("horizontal anchor offset overflowed"))?,
        i32::try_from(offset_y)
            .map_err(|_| CoreError::InvalidArgument("vertical anchor offset overflowed"))?,
    ))
}

pub(super) fn checked_dimension_milli(dimension: u32) -> Result<i32, CoreError> {
    i32::try_from(dimension)
        .ok()
        .and_then(|value| value.checked_mul(1_000))
        .ok_or(CoreError::InvalidArgument(
            "document dimension exceeds vector coordinate range",
        ))
}

pub(super) fn checked_scaled_i32(value: i32, scale: f64) -> Result<i32, CoreError> {
    let scaled = f64::from(value) * scale;
    if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        return Err(CoreError::InvalidArgument("scaled coordinate overflowed"));
    }
    Ok(scaled.round() as i32)
}

pub(super) fn checked_scaled_spacing(value: u32, scale: f64) -> Result<u32, CoreError> {
    let scaled = (f64::from(value) * scale).round().max(1.0);
    if !scaled.is_finite() || scaled > f64::from(u32::MAX) {
        return Err(CoreError::InvalidArgument("scaled spacing overflowed"));
    }
    Ok(scaled as u32)
}

pub(super) fn checked_scaled_u32(value: u32, scale: f64) -> Result<u32, CoreError> {
    let scaled = (f64::from(value) * scale).round().max(0.0);
    if !scaled.is_finite() || scaled > f64::from(u32::MAX) {
        return Err(CoreError::InvalidArgument(
            "scaled unsigned value overflowed",
        ));
    }
    Ok(scaled as u32)
}

pub(super) fn clamp_margins(margins: &mut Margins, width: u32, height: u32) {
    margins.left = margins.left.min(width);
    margins.right = margins.right.min(width.saturating_sub(margins.left));
    margins.top = margins.top.min(height);
    margins.bottom = margins.bottom.min(height.saturating_sub(margins.top));
}

pub(super) fn translate_frame_metadata(
    frames: &mut FrameMetadata,
    offset_x: i32,
    offset_y: i32,
) -> Result<(), CoreError> {
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
    ] {
        frame.x = frame
            .x
            .checked_add(offset_x)
            .ok_or(CoreError::InvalidArgument("translated frame overflowed"))?;
        frame.y = frame
            .y
            .checked_add(offset_y)
            .ok_or(CoreError::InvalidArgument("translated frame overflowed"))?;
    }
    Ok(())
}

pub(super) fn scale_frame_metadata(
    frames: &mut FrameMetadata,
    scale_x: f64,
    scale_y: f64,
) -> Result<(), CoreError> {
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
    ] {
        frame.x = checked_scaled_i32(frame.x, scale_x)?;
        frame.y = checked_scaled_i32(frame.y, scale_y)?;
        frame.width = checked_scaled_i32(frame.width, scale_x)?.max(1);
        frame.height = checked_scaled_i32(frame.height, scale_y)?.max(1);
    }
    frames.margins.left = checked_scaled_u32(frames.margins.left, scale_x)?;
    frames.margins.right = checked_scaled_u32(frames.margins.right, scale_x)?;
    frames.margins.top = checked_scaled_u32(frames.margins.top, scale_y)?;
    frames.margins.bottom = checked_scaled_u32(frames.margins.bottom, scale_y)?;
    Ok(())
}

pub(super) fn rotate_frame_metadata(
    frames: &mut FrameMetadata,
    width: u32,
    height: u32,
    direction: RotateDirection,
) -> Result<(), CoreError> {
    let width = i32::try_from(width)
        .map_err(|_| CoreError::InvalidState("document width exceeds frame range"))?;
    let height = i32::try_from(height)
        .map_err(|_| CoreError::InvalidState("document height exceeds frame range"))?;
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
    ] {
        let previous = *frame;
        *frame = match direction {
            RotateDirection::Left90 => RectI32 {
                x: previous.y,
                y: width - previous.x - previous.width,
                width: previous.height,
                height: previous.width,
            },
            RotateDirection::Right90 => RectI32 {
                x: height - previous.y - previous.height,
                y: previous.x,
                width: previous.height,
                height: previous.width,
            },
        };
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
    width: u32,
    height: u32,
    direction: RotateDirection,
) -> Result<(), CoreError> {
    let width = i32::try_from(width)
        .map_err(|_| CoreError::InvalidState("document width exceeds guide range"))?;
    let height = i32::try_from(height)
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
    width: u32,
    height: u32,
    axis: MirrorAxis,
) -> Result<(), CoreError> {
    let width = i32::try_from(width)
        .map_err(|_| CoreError::InvalidState("document width exceeds frame range"))?;
    let height = i32::try_from(height)
        .map_err(|_| CoreError::InvalidState("document height exceeds frame range"))?;
    for frame in [
        &mut frames.hundred_frame,
        &mut frames.reference_frame,
        &mut frames.drawing_frame,
        &mut frames.safe_frame,
    ] {
        match axis {
            MirrorAxis::Horizontal => frame.x = width - frame.x - frame.width,
            MirrorAxis::Vertical => frame.y = height - frame.y - frame.height,
        }
    }
    Ok(())
}
