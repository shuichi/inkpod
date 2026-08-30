//! Bounded raster layer thumbnails.

use crate::document::{CellDocument, LayerNode};
use crate::identity::LayerId;
use crate::{Core, CoreError, LayerThumbnail, PixelFormat, PixelValue, PlaneType};
use inkpod_image::source_over_rgba8;

impl Core {
    /// Builds a small, aspect-preserving straight RGBA8 preview of exactly one
    /// raster layer. Hidden layers are still previewed, while per-plane
    /// visibility and both layer/plane opacity values remain part of the result.
    pub fn layer_thumbnail(
        &self,
        layer_id: u64,
        maximum_width: u32,
        maximum_height: u32,
    ) -> Result<LayerThumbnail, CoreError> {
        let layer_id = LayerId::from_raw(layer_id);
        const MAXIMUM_THUMBNAIL_EDGE: u32 = 256;
        if maximum_width == 0
            || maximum_height == 0
            || maximum_width > MAXIMUM_THUMBNAIL_EDGE
            || maximum_height > MAXIMUM_THUMBNAIL_EDGE
        {
            return Err(CoreError::InvalidArgument(
                "layer thumbnail dimensions are outside bounds",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let layer = document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        let (width, height) = thumbnail_dimensions(
            document.width,
            document.height,
            maximum_width,
            maximum_height,
        )?;
        let stride_bytes = width
            .checked_mul(4)
            .ok_or(CoreError::InvalidState("layer thumbnail stride overflows"))?;
        let byte_count = stride_bytes
            .checked_mul(height)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(CoreError::InvalidState("layer thumbnail bytes overflow"))?;
        let mut pixels = vec![0_u8; byte_count];
        for output_y in 0..height {
            for output_x in 0..width {
                let rgba = sample_layer_thumbnail_pixel(
                    document, layer, output_x, output_y, width, height,
                )?;
                let offset = output_y as usize * stride_bytes as usize + output_x as usize * 4;
                pixels[offset..offset + 4].copy_from_slice(&rgba);
            }
        }
        Ok(LayerThumbnail {
            revision: self.document_revision.get(),
            layer_id: layer_id.get(),
            width,
            height,
            stride_bytes,
            pixels,
        })
    }
}

fn thumbnail_dimensions(
    source_width: u32,
    source_height: u32,
    maximum_width: u32,
    maximum_height: u32,
) -> Result<(u32, u32), CoreError> {
    if source_width == 0 || source_height == 0 {
        return Err(CoreError::InvalidState(
            "layer thumbnail source dimensions are empty",
        ));
    }
    let width_limit = maximum_width.min(source_width);
    let height_limit = maximum_height.min(source_height);
    let (width, height) = if u64::from(source_width) * u64::from(height_limit)
        > u64::from(source_height) * u64::from(width_limit)
    {
        let height = (u64::from(source_height) * u64::from(width_limit)
            + u64::from(source_width) / 2)
            / u64::from(source_width);
        (width_limit, u32::try_from(height).unwrap_or(1).max(1))
    } else {
        let width = (u64::from(source_width) * u64::from(height_limit)
            + u64::from(source_height) / 2)
            / u64::from(source_height);
        (u32::try_from(width).unwrap_or(1).max(1), height_limit)
    };
    Ok((width, height))
}

fn sample_layer_thumbnail_pixel(
    document: &CellDocument,
    layer: &LayerNode,
    output_x: u32,
    output_y: u32,
    output_width: u32,
    output_height: u32,
) -> Result<[u8; 4], CoreError> {
    const SAMPLE_OFFSETS: [f64; 4] = [0.125, 0.375, 0.625, 0.875];
    let mut premultiplied = [0_u64; 3];
    let mut alpha = 0_u64;
    for offset_y in SAMPLE_OFFSETS {
        for offset_x in SAMPLE_OFFSETS {
            let document_x = (((f64::from(output_x) + offset_x) * f64::from(document.width)
                / f64::from(output_width))
            .floor() as u32)
                .min(document.width - 1);
            let document_y = (((f64::from(output_y) + offset_y) * f64::from(document.height)
                / f64::from(output_height))
            .floor() as u32)
                .min(document.height - 1);
            let value = sample_layer_raster(document, layer, document_x, document_y)?;
            alpha += u64::from(value[3]);
            for channel in 0..3 {
                premultiplied[channel] += u64::from(value[channel]) * u64::from(value[3]);
            }
        }
    }
    let sample_count = (SAMPLE_OFFSETS.len() * SAMPLE_OFFSETS.len()) as u64;
    let mut output = [0_u8; 4];
    for channel in 0..3 {
        output[channel] = (premultiplied[channel] + alpha / 2)
            .checked_div(alpha)
            .unwrap_or(0) as u8;
    }
    output[3] = ((alpha + sample_count / 2) / sample_count) as u8;
    Ok(output)
}

fn sample_layer_raster(
    document: &CellDocument,
    layer: &LayerNode,
    x: u32,
    y: u32,
) -> Result<[u8; 4], CoreError> {
    let mut composite = [0_u8; 4];
    for plane in layer.planes.iter().rev().filter(|plane| plane.visible) {
        let mut rgba = match plane.kind {
            PlaneType::MainLine
                if matches!(
                    plane.raster.format(),
                    PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
                ) =>
            {
                rgba8(plane.raster.pixel(x, y)?)
            }
            PlaneType::MainLine => {
                let coverage = match plane.raster.pixel(x, y)? {
                    PixelValue::Binary(value) | PixelValue::Grayscale8(value) => value,
                    PixelValue::Grayscale16(value) => ((u32::from(value) + 128) / 257) as u8,
                    _ => {
                        return Err(CoreError::InvalidState(
                            "main-line thumbnail source is not grayscale",
                        ));
                    }
                };
                let mut line = rgba8(document.main_line_color);
                line[3] = ((u32::from(line[3]) * u32::from(coverage) + 127) / 255) as u8;
                line
            }
            PlaneType::Color | PlaneType::Raster => rgba8(plane.raster.pixel(x, y)?),
        };
        rgba[3] = ((u32::from(rgba[3]) * plane.opacity_milli + 500) / 1_000) as u8;
        composite = source_over_rgba8(composite, rgba);
    }
    composite[3] = ((u32::from(composite[3]) * layer.opacity_milli + 500) / 1_000) as u8;
    Ok(composite)
}

fn rgba8(color: PixelValue) -> [u8; 4] {
    match color {
        PixelValue::Rgba(value) => value,
        PixelValue::Rgba16(value) => value.map(|channel| ((u32::from(channel) + 128) / 257) as u8),
        _ => [0, 0, 0, 0],
    }
}
