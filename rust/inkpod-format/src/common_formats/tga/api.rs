use super::decode::{apply_color_correction, decode_document};
use super::encode::encode_document;
use super::model::{TgaAlphaType, TgaDocument, TgaEncodeOptions};
use crate::{CommonRaster, CommonRasterInfo, FormatError};

pub(crate) fn encode_tga(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    let raster = CommonRaster::new(
        info.width,
        info.height,
        info.pixel_format,
        info.dpi_x_milli,
        info.dpi_y_milli,
        pixels.to_vec(),
    )?;
    encode_tga_with_options(&raster, &TgaEncodeOptions::default())
}

pub(crate) fn decode_tga(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    let document = decode_tga_document(bytes)?;
    let mut raster = document
        .raster
        .ok_or(FormatError::Unsupported("TGA contains no image data"))?;
    if let Some(extension) = &document.options.metadata.extension {
        if let Some(table) = &extension.color_correction_table {
            apply_color_correction(&mut raster.pixels, table)?;
        }
        if !matches!(
            extension.alpha_type,
            TgaAlphaType::Straight | TgaAlphaType::Premultiplied
        ) {
            for pixel in raster.pixels.chunks_exact_mut(4) {
                pixel[3] = u8::MAX;
            }
        }
    }
    Ok(raster)
}

/// Encodes a canonical straight-alpha RGBA8 raster using explicit TGA storage options.
///
/// Alpha or channel precision loss is rejected unless its corresponding option is explicit.
pub fn encode_tga_with_options(
    raster: &CommonRaster,
    options: &TgaEncodeOptions,
) -> Result<Vec<u8>, FormatError> {
    encode_tga_document(&TgaDocument {
        raster: Some(raster.clone()),
        options: options.clone(),
    })
}

/// Encodes a typed TGA document. Only `TgaImageFormat::None` permits no raster.
pub fn encode_tga_document(document: &TgaDocument) -> Result<Vec<u8>, FormatError> {
    encode_document(document)
}

/// Decodes all standard Truevision TGA 2.0 image types to RGBA8 and typed metadata.
pub fn decode_tga_document(bytes: &[u8]) -> Result<TgaDocument, FormatError> {
    decode_document(bytes)
}
