use super::sequence::*;
use super::*;

pub(super) fn validate_reference_frame(frame: RectI32) -> Result<(), CoreError> {
    if frame.width <= 0 || frame.height <= 0 {
        Err(CoreError::InvalidArgument(
            "reference frame dimensions must be positive",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_sequence_cell(cell: &SequenceCellSource) -> Result<(), CoreError> {
    validate_node_name(&cell.name)?;
    if cell.document_uuid == 0
        || cell.dpi_x_milli == 0
        || cell.dpi_y_milli == 0
        || parse_cell_number(&cell.name) != Some(cell.cell_number)
    {
        return Err(CoreError::InvalidArgument(
            "sequence cell identity or DPI is invalid",
        ));
    }
    validate_frame_metadata(cell.raster.width(), cell.raster.height(), cell.frames)
}

pub(super) fn flatten_document(
    document: &CellDocument,
    assets: &asset::AssetStore,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    bounded_document_pixels(document.width, document.height)?;
    let mut raster = TileRaster::new(document.width, document.height, PixelFormat::StraightRgba8)?;
    let base_asset = match document.base_surface {
        BaseSurface::SolidWhite => None,
        BaseSurface::Asset(id) => {
            let record = assets
                .get(id)
                .ok_or(CoreError::InvalidState("Genesis base asset is missing"))?;
            let source = record.raster().ok_or(CoreError::InvalidState(
                "Genesis base asset is not a raster",
            ))?;
            if source.width() != document.width || source.height() != document.height {
                return Err(CoreError::InvalidState(
                    "Genesis base asset dimensions do not match the paper",
                ));
            }
            Some(record)
        }
    };
    let vector_rasters = document
        .layers
        .iter()
        .map(|layer| {
            (layer.kind == LayerKind::VectorColoring)
                .then(|| {
                    crate::vector::rasterize_vector_layer_content(
                        document,
                        layer,
                        document.width,
                        document.height,
                        document.width.saturating_mul(4),
                        true,
                    )
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;
    for y in 0..document.height {
        for x in 0..document.width {
            let mut composite = match &base_asset {
                None => [u8::MAX; 4],
                Some(record) => base_raster_pixel(
                    record.raster().ok_or(CoreError::InvalidState(
                        "Genesis base asset stopped being a raster",
                    ))?,
                    x,
                    y,
                )?,
            };
            for (layer, vector_raster) in document
                .layers
                .iter()
                .zip(&vector_rasters)
                .rev()
                .filter(|(layer, _)| layer.visible)
            {
                if layer.kind == LayerKind::Adjustment {
                    let adjustment =
                        document
                            .adjustments
                            .get(&layer.id)
                            .ok_or(CoreError::InvalidState(
                                "adjustment layer metadata is missing",
                            ))?;
                    let adjusted =
                        inkpod_image::apply_adjustment(PixelValue::Rgba(composite), adjustment)?
                            .rgba16()
                            .ok_or(CoreError::InvalidState(
                                "adjustment output is not displayable",
                            ))?
                            .map(|channel| ((u32::from(channel) + 128) / 257) as u8);
                    composite = std::array::from_fn(|channel| {
                        ((u32::from(composite[channel]) * (1_000 - layer.opacity_milli)
                            + u32::from(adjusted[channel]) * layer.opacity_milli
                            + 500)
                            / 1_000) as u8
                    });
                    continue;
                }
                if let Some(vector_raster) = vector_raster {
                    let offset = y as usize * vector_raster.stride_bytes as usize + x as usize * 4;
                    composite = blend_rgba_over(
                        composite,
                        vector_raster.pixels[offset..offset + 4]
                            .try_into()
                            .map_err(|_| CoreError::InvalidState("vector raster is truncated"))?,
                    );
                    continue;
                }
                let mut layer_pixel = [0_u8; 4];
                for plane in layer.planes.iter().rev().filter(|plane| plane.visible) {
                    let value = plane.raster.pixel(x, y)?;
                    let mut rgba = match plane.kind {
                        PlaneType::MainLine => {
                            let coverage = match value {
                                PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                                PixelValue::Grayscale16(value) => {
                                    ((u32::from(value) + 128) / 257) as u8
                                }
                                _ => {
                                    return Err(CoreError::InvalidState(
                                        "main-line source is invalid",
                                    ));
                                }
                            };
                            let mut line = rgba8_for_display(document.main_line_color)
                                .ok_or(CoreError::InvalidState("main-line color is not RGBA"))?;
                            line[3] =
                                ((u32::from(line[3]) * u32::from(coverage) + 127) / 255) as u8;
                            line
                        }
                        PlaneType::Color | PlaneType::Raster => rgba8_for_display(value)
                            .ok_or(CoreError::InvalidState("flatten source is not RGBA"))?,
                        PlaneType::Selection
                        | PlaneType::VectorMainLine
                        | PlaneType::ColorTrace
                        | PlaneType::VectorFill => continue,
                    };
                    rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
                    layer_pixel = blend_rgba_over(layer_pixel, rgba);
                }
                layer_pixel[3] =
                    ((u32::from(layer_pixel[3]) * layer.opacity_milli + 500) / 1_000) as u8;
                composite = blend_rgba_over(composite, layer_pixel);
            }
            if composite != [0; 4] {
                raster.set_pixel(x, y, PixelValue::Rgba(composite), revision)?;
            }
        }
    }
    Ok(raster)
}

pub(crate) fn base_raster_pixel(raster: &TileRaster, x: u32, y: u32) -> Result<[u8; 4], CoreError> {
    match raster.pixel(x, y)? {
        PixelValue::Binary(coverage) => Ok([0, 0, 0, coverage]),
        PixelValue::Grayscale8(value) => Ok([value, value, value, u8::MAX]),
        PixelValue::Grayscale16(value) => {
            let value = ((u32::from(value) + 128) / 257) as u8;
            Ok([value, value, value, u8::MAX])
        }
        value @ (PixelValue::Rgba(_) | PixelValue::Rgba16(_)) => rgba8_for_display(value).ok_or(
            CoreError::InvalidState("Genesis base raster is not displayable"),
        ),
    }
}

pub(super) fn validate_frames(
    document: &CellDocument,
    frames: FrameMetadata,
) -> Result<(), CoreError> {
    validate_frame_metadata(document.width, document.height, frames)
}

pub(super) fn validate_frame_metadata(
    width: u32,
    height: u32,
    frames: FrameMetadata,
) -> Result<(), CoreError> {
    for frame in [
        frames.hundred_frame,
        frames.reference_frame,
        frames.drawing_frame,
        frames.safe_frame,
    ] {
        validate_reference_frame(frame)?;
    }
    if frames
        .margins
        .left
        .checked_add(frames.margins.right)
        .is_none_or(|value| value > width)
        || frames
            .margins
            .top
            .checked_add(frames.margins.bottom)
            .is_none_or(|value| value > height)
    {
        return Err(CoreError::InvalidArgument(
            "paper margins exceed document dimensions",
        ));
    }
    Ok(())
}

pub(super) fn common_to_tile_raster(
    raster: &CommonRaster,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    raster.validate()?;
    let mut result = TileRaster::new(
        raster.info.width,
        raster.info.height,
        raster.info.pixel_format,
    )?;
    let bytes_per_pixel = raster.info.pixel_format.bytes_per_pixel();
    for y in 0..raster.info.height {
        for x in 0..raster.info.width {
            let offset = (y as usize * raster.info.width as usize + x as usize) * bytes_per_pixel;
            let value = match raster.info.pixel_format {
                PixelFormat::StraightRgba8 => PixelValue::Rgba(
                    raster.pixels[offset..offset + 4]
                        .try_into()
                        .map_err(|_| CoreError::InvalidState("RGBA8 pixel is truncated"))?,
                ),
                PixelFormat::StraightRgba16 => {
                    let mut channels = [0_u16; 4];
                    for (index, channel) in channels.iter_mut().enumerate() {
                        let start = offset + index * 2;
                        *channel =
                            u16::from_le_bytes([raster.pixels[start], raster.pixels[start + 1]]);
                    }
                    PixelValue::Rgba16(channels)
                }
                _ => {
                    return Err(CoreError::InvalidArgument(
                        "common raster must be straight RGBA",
                    ));
                }
            };
            if !value.is_zero() {
                result.set_pixel(x, y, value, revision)?;
            }
        }
    }
    Ok(result)
}

pub(super) fn tile_to_common(
    raster: &TileRaster,
    dpi_x_milli: Option<u32>,
    dpi_y_milli: Option<u32>,
) -> Result<CommonRaster, CoreError> {
    let mut pixels = Vec::with_capacity(
        raster.width() as usize * raster.height() as usize * raster.format().bytes_per_pixel(),
    );
    for y in 0..raster.height() {
        for x in 0..raster.width() {
            match raster.pixel(x, y)? {
                PixelValue::Rgba(value) => pixels.extend_from_slice(&value),
                PixelValue::Rgba16(value) => {
                    for channel in value {
                        pixels.extend_from_slice(&channel.to_le_bytes());
                    }
                }
                _ => {
                    return Err(CoreError::InvalidState(
                        "sequence raster is not straight RGBA",
                    ));
                }
            }
        }
    }
    Ok(CommonRaster::new(
        raster.width(),
        raster.height(),
        raster.format(),
        dpi_x_milli,
        dpi_y_milli,
        pixels,
    )?)
}

pub(super) fn thumbnail_for_raster(raster: &TileRaster) -> Result<Thumbnail, CoreError> {
    let scale = (f64::from(raster.width()) / f64::from(THUMBNAIL_MAX_DIMENSION))
        .max(f64::from(raster.height()) / f64::from(THUMBNAIL_MAX_DIMENSION))
        .max(1.0);
    let width = (f64::from(raster.width()) / scale).round().max(1.0) as u32;
    let height = (f64::from(raster.height()) / scale).round().max(1.0) as u32;
    let mut rgba8 = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let source_x = ((f64::from(x) + 0.5) * scale)
                .floor()
                .min(f64::from(raster.width() - 1)) as u32;
            let source_y = ((f64::from(y) + 0.5) * scale)
                .floor()
                .min(f64::from(raster.height() - 1)) as u32;
            let pixel = rgba8_for_display(raster.pixel(source_x, source_y)?)
                .ok_or(CoreError::InvalidState("thumbnail source is not RGBA"))?;
            rgba8.extend_from_slice(&pixel);
        }
    }
    let checksum = inkpod_image::fnv_bytes(inkpod_image::FNV_OFFSET, &rgba8);
    Ok(Thumbnail {
        width,
        height,
        rgba8,
        checksum,
    })
}
