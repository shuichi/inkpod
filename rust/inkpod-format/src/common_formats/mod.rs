mod bmp;
#[path = "png.rs"]
mod png_codec;
mod tga;
mod tiff;

use super::FormatError;
use bmp::{decode_bmp, encode_bmp};
use inkpod_image::{MAX_RASTER_DIMENSION, PixelFormat};
use png_codec::{decode_png, encode_png};
use tga::{decode_tga, encode_tga};
use tiff::{decode_tiff, encode_tiff};

pub const MAX_COMMON_RASTER_BYTES: usize = 1 << 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonRasterFormat {
    Png,
    Tiff,
    Tga,
    Bmp,
}

impl CommonRasterFormat {
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => Some(Self::Png),
            "tif" | "tiff" => Some(Self::Tiff),
            "tga" => Some(Self::Tga),
            "bmp" => Some(Self::Bmp),
            _ => None,
        }
    }

    #[must_use]
    pub const fn supports_16_bit(self) -> bool {
        matches!(self, Self::Png | Self::Tiff)
    }

    #[must_use]
    pub const fn supports_dpi(self) -> bool {
        !matches!(self, Self::Tga)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommonRasterInfo {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub dpi_x_milli: Option<u32>,
    pub dpi_y_milli: Option<u32>,
    pub has_alpha: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommonRaster {
    pub info: CommonRasterInfo,
    /// Row-major straight-alpha RGBA. RGBA8 uses four bytes per pixel;
    /// RGBA16 uses native little-endian u16 channels in this DTO only.
    pub pixels: Vec<u8>,
}

impl CommonRaster {
    pub fn new(
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
        dpi_x_milli: Option<u32>,
        dpi_y_milli: Option<u32>,
        pixels: Vec<u8>,
    ) -> Result<Self, FormatError> {
        let raster = Self {
            info: CommonRasterInfo {
                width,
                height,
                pixel_format,
                dpi_x_milli,
                dpi_y_milli,
                has_alpha: true,
            },
            pixels,
        };
        raster.validate()?;
        Ok(raster)
    }

    pub fn validate(&self) -> Result<(), FormatError> {
        validate_dimensions(self.info.width, self.info.height)?;
        if !matches!(
            self.info.pixel_format,
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        ) {
            return Err(FormatError::Invalid(
                "common raster must use straight RGBA8 or RGBA16",
            ));
        }
        if self.info.dpi_x_milli == Some(0) || self.info.dpi_y_milli == Some(0) {
            return Err(FormatError::Invalid("common raster DPI must be nonzero"));
        }
        if self.info.dpi_x_milli.is_some() != self.info.dpi_y_milli.is_some() {
            return Err(FormatError::Invalid(
                "common raster DPI axes must both be present or absent",
            ));
        }
        let expected = expected_bytes(
            self.info.width,
            self.info.height,
            self.info.pixel_format.bytes_per_pixel(),
        )?;
        if self.pixels.len() != expected {
            return Err(FormatError::Invalid(
                "common raster pixel length does not match its metadata",
            ));
        }
        Ok(())
    }

    fn prepared_pixels(&self, composite_white: bool) -> Result<Vec<u8>, FormatError> {
        self.validate()?;
        if !composite_white {
            return Ok(self.pixels.clone());
        }
        let mut pixels = self.pixels.clone();
        match self.info.pixel_format {
            PixelFormat::StraightRgba8 => {
                for pixel in pixels.chunks_exact_mut(4) {
                    let alpha = u32::from(pixel[3]);
                    for channel in &mut pixel[..3] {
                        *channel = ((u32::from(*channel) * alpha + 255_u32 * (255 - alpha) + 127)
                            / 255) as u8;
                    }
                    pixel[3] = u8::MAX;
                }
            }
            PixelFormat::StraightRgba16 => {
                for pixel in pixels.chunks_exact_mut(8) {
                    let alpha = u32::from(u16::from_le_bytes([pixel[6], pixel[7]]));
                    for channel in 0..3 {
                        let offset = channel * 2;
                        let value =
                            u32::from(u16::from_le_bytes([pixel[offset], pixel[offset + 1]]));
                        let composite = (value * alpha
                            + u32::from(u16::MAX) * (u32::from(u16::MAX) - alpha)
                            + u32::from(u16::MAX) / 2)
                            / u32::from(u16::MAX);
                        pixel[offset..offset + 2]
                            .copy_from_slice(&(composite as u16).to_le_bytes());
                    }
                    pixel[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
                }
            }
            _ => unreachable!("validated common-raster format"),
        }
        Ok(pixels)
    }
}

pub fn encode_common_raster(
    format: CommonRasterFormat,
    raster: &CommonRaster,
    composite_white: bool,
) -> Result<Vec<u8>, FormatError> {
    raster.validate()?;
    if raster.info.pixel_format == PixelFormat::StraightRgba16 && !format.supports_16_bit() {
        return Err(FormatError::Unsupported(
            "the selected common format cannot represent 16-bit RGBA",
        ));
    }
    let pixels = raster.prepared_pixels(composite_white)?;
    match format {
        CommonRasterFormat::Png => encode_png(raster.info, &pixels),
        CommonRasterFormat::Tiff => encode_tiff(raster.info, &pixels),
        CommonRasterFormat::Tga => encode_tga(raster.info, &pixels),
        CommonRasterFormat::Bmp => encode_bmp(raster.info, &pixels),
    }
}

pub fn decode_common_raster(
    format: CommonRasterFormat,
    bytes: &[u8],
) -> Result<CommonRaster, FormatError> {
    if bytes.len() > MAX_COMMON_RASTER_BYTES {
        return Err(FormatError::Invalid("common raster file exceeds its bound"));
    }
    match format {
        CommonRasterFormat::Png => decode_png(bytes),
        CommonRasterFormat::Tiff => decode_tiff(bytes),
        CommonRasterFormat::Tga => decode_tga(bytes),
        CommonRasterFormat::Bmp => decode_bmp(bytes),
    }
}

fn expected_bytes(width: u32, height: u32, bytes_per_pixel: usize) -> Result<usize, FormatError> {
    validate_dimensions(width, height)?;
    usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(height as usize))
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .filter(|bytes| *bytes <= MAX_COMMON_RASTER_BYTES)
        .ok_or(FormatError::Invalid("common raster byte length overflows"))
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), FormatError> {
    if width == 0 || height == 0 || width > MAX_RASTER_DIMENSION || height > MAX_RASTER_DIMENSION {
        Err(FormatError::Invalid(
            "common raster dimensions are outside bounds",
        ))
    } else {
        Ok(())
    }
}

fn dpi_milli_to_pixels_per_meter(dpi_milli: u32) -> u32 {
    ((u64::from(dpi_milli) * 10 + 127) / 254).min(u64::from(u32::MAX)) as u32
}

fn pixels_per_meter_to_dpi_milli(pixels_per_meter: u32) -> u32 {
    ((u64::from(pixels_per_meter) * 254 + 5) / 10).min(u64::from(u32::MAX)) as u32
}

#[cfg(test)]
#[path = "../../tests/unit/common_formats.rs"]
mod tests;
