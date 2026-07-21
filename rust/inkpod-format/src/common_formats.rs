use super::FormatError;
use inkpod_image::{MAX_RASTER_DIMENSION, PixelFormat};
use std::io::Cursor;

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

fn encode_png(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, info.width, info.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(match info.pixel_format {
            PixelFormat::StraightRgba8 => png::BitDepth::Eight,
            PixelFormat::StraightRgba16 => png::BitDepth::Sixteen,
            _ => return Err(FormatError::Invalid("PNG requires RGBA pixels")),
        });
        if let (Some(x), Some(y)) = (info.dpi_x_milli, info.dpi_y_milli) {
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: dpi_milli_to_pixels_per_meter(x),
                yppu: dpi_milli_to_pixels_per_meter(y),
                unit: png::Unit::Meter,
            }));
        }
        let mut writer = encoder
            .write_header()
            .map_err(|_| FormatError::Invalid("PNG header encoding failed"))?;
        if info.pixel_format == PixelFormat::StraightRgba16 {
            let mut big_endian = pixels.to_vec();
            for channel in big_endian.chunks_exact_mut(2) {
                channel.swap(0, 1);
            }
            writer
                .write_image_data(&big_endian)
                .map_err(|_| FormatError::Invalid("PNG pixel encoding failed"))?;
        } else {
            writer
                .write_image_data(pixels)
                .map_err(|_| FormatError::Invalid("PNG pixel encoding failed"))?;
        }
    }
    Ok(output)
}

fn decode_png(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|_| FormatError::Invalid("PNG header is invalid"))?;
    validate_dimensions(reader.info().width, reader.info().height)?;
    if reader.output_buffer_size() > MAX_COMMON_RASTER_BYTES {
        return Err(FormatError::Invalid(
            "PNG decoded pixel buffer exceeds its bound",
        ));
    }
    let mut source = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut source)
        .map_err(|_| FormatError::Invalid("PNG pixel data is invalid"))?;
    source.truncate(frame.buffer_size());
    let format = match frame.bit_depth {
        png::BitDepth::Eight => PixelFormat::StraightRgba8,
        png::BitDepth::Sixteen => PixelFormat::StraightRgba16,
        _ => return Err(FormatError::Unsupported("PNG bit depth is unsupported")),
    };
    let channel_bytes = if format == PixelFormat::StraightRgba8 {
        1
    } else {
        2
    };
    let pixel_count = usize::try_from(frame.width)
        .ok()
        .and_then(|width| width.checked_mul(frame.height as usize))
        .ok_or(FormatError::Invalid("PNG dimensions overflow"))?;
    let mut pixels = Vec::with_capacity(expected_bytes(
        frame.width,
        frame.height,
        4 * channel_bytes,
    )?);
    match frame.color_type {
        png::ColorType::Rgba => pixels.extend_from_slice(&source),
        png::ColorType::Rgb => {
            let stride = 3 * channel_bytes;
            for pixel in source.chunks_exact(stride) {
                pixels.extend_from_slice(pixel);
                if channel_bytes == 1 {
                    pixels.push(u8::MAX);
                } else {
                    pixels.extend_from_slice(&u16::MAX.to_be_bytes());
                }
            }
        }
        png::ColorType::Grayscale => {
            for sample in source.chunks_exact(channel_bytes) {
                pixels.extend_from_slice(sample);
                pixels.extend_from_slice(sample);
                pixels.extend_from_slice(sample);
                if channel_bytes == 1 {
                    pixels.push(u8::MAX);
                } else {
                    pixels.extend_from_slice(&u16::MAX.to_be_bytes());
                }
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for sample in source.chunks_exact(2 * channel_bytes) {
                let gray = &sample[..channel_bytes];
                pixels.extend_from_slice(gray);
                pixels.extend_from_slice(gray);
                pixels.extend_from_slice(gray);
                pixels.extend_from_slice(&sample[channel_bytes..]);
            }
        }
        png::ColorType::Indexed => {
            return Err(FormatError::Unsupported(
                "indexed PNG requires palette expansion before import",
            ));
        }
    }
    if pixels.len() != pixel_count * 4 * channel_bytes {
        return Err(FormatError::Invalid("PNG decoded pixel length is invalid"));
    }
    if format == PixelFormat::StraightRgba16 {
        for channel in pixels.chunks_exact_mut(2) {
            channel.swap(0, 1);
        }
    }
    let pixel_dims = reader.info().pixel_dims;
    let (dpi_x_milli, dpi_y_milli) = pixel_dims
        .filter(|dimensions| dimensions.unit == png::Unit::Meter)
        .map_or((None, None), |dimensions| {
            (
                Some(pixels_per_meter_to_dpi_milli(dimensions.xppu)),
                Some(pixels_per_meter_to_dpi_milli(dimensions.yppu)),
            )
        });
    CommonRaster::new(
        frame.width,
        frame.height,
        format,
        dpi_x_milli,
        dpi_y_milli,
        pixels,
    )
}

fn encode_tiff(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    const ENTRY_COUNT: u16 = 14;
    let ifd_offset = 8_u32;
    let ifd_bytes = 2_u32 + u32::from(ENTRY_COUNT) * 12 + 4;
    let bits_offset = ifd_offset + ifd_bytes;
    let x_resolution_offset = bits_offset + 8;
    let y_resolution_offset = x_resolution_offset + 8;
    let pixel_offset = y_resolution_offset + 8;
    let pixel_length = u32::try_from(pixels.len())
        .map_err(|_| FormatError::Invalid("TIFF pixel payload is too large"))?;
    let mut output = Vec::with_capacity(pixel_offset as usize + pixels.len());
    output.extend_from_slice(b"II");
    output.extend_from_slice(&42_u16.to_le_bytes());
    output.extend_from_slice(&ifd_offset.to_le_bytes());
    output.extend_from_slice(&ENTRY_COUNT.to_le_bytes());
    let bits = if info.pixel_format == PixelFormat::StraightRgba8 {
        8
    } else {
        16
    };
    let dpi_x = info.dpi_x_milli.unwrap_or(96_000);
    let dpi_y = info.dpi_y_milli.unwrap_or(96_000);
    for (tag, kind, count, value) in [
        (256_u16, 4_u16, 1_u32, info.width),
        (257, 4, 1, info.height),
        (258, 3, 4, bits_offset),
        (259, 3, 1, 1),
        (262, 3, 1, 2),
        (273, 4, 1, pixel_offset),
        (277, 3, 1, 4),
        (278, 4, 1, info.height),
        (279, 4, 1, pixel_length),
        (282, 5, 1, x_resolution_offset),
        (283, 5, 1, y_resolution_offset),
        (284, 3, 1, 1),
        (296, 3, 1, 2),
        (338, 3, 1, 2), // one unassociated (straight) alpha sample
    ] {
        tiff_entry(&mut output, tag, kind, count, value);
    }
    output.extend_from_slice(&0_u32.to_le_bytes());
    for _ in 0..4 {
        output.extend_from_slice(&(bits as u16).to_le_bytes());
    }
    output.extend_from_slice(&dpi_x.to_le_bytes());
    output.extend_from_slice(&1_000_u32.to_le_bytes());
    output.extend_from_slice(&dpi_y.to_le_bytes());
    output.extend_from_slice(&1_000_u32.to_le_bytes());
    output.extend_from_slice(pixels);
    Ok(output)
}

fn tiff_entry(output: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32) {
    output.extend_from_slice(&tag.to_le_bytes());
    output.extend_from_slice(&kind.to_le_bytes());
    output.extend_from_slice(&count.to_le_bytes());
    output.extend_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

fn decode_tiff(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    let endian = match bytes.get(..2) {
        Some(b"II") => Endian::Little,
        Some(b"MM") => Endian::Big,
        _ => return Err(FormatError::Invalid("TIFF byte order is invalid")),
    };
    if read_u16(bytes, 2, endian)? != 42 {
        return Err(FormatError::Invalid("TIFF magic is invalid"));
    }
    let ifd = read_u32(bytes, 4, endian)? as usize;
    let count = read_u16(bytes, ifd, endian)? as usize;
    if count > 256 {
        return Err(FormatError::Invalid(
            "TIFF IFD entry count exceeds its bound",
        ));
    }
    let mut width = None;
    let mut height = None;
    let mut bits_offset = None;
    let mut compression = 1_u32;
    let mut photometric = None;
    let mut strip_offset = None;
    let mut samples = None;
    let mut rows_per_strip = None;
    let mut strip_bytes = None;
    let mut x_resolution = None;
    let mut y_resolution = None;
    let mut planar = 1_u32;
    let mut resolution_unit = 2_u32;
    let mut extra_samples = None;
    for index in 0..count {
        let offset = ifd
            .checked_add(2 + index * 12)
            .ok_or(FormatError::Invalid("TIFF IFD offset overflows"))?;
        let tag = read_u16(bytes, offset, endian)?;
        let kind = read_u16(bytes, offset + 2, endian)?;
        let item_count = read_u32(bytes, offset + 4, endian)?;
        let value = read_u32(bytes, offset + 8, endian)?;
        let scalar = || tiff_scalar(bytes, offset + 8, endian, kind, item_count, value);
        match tag {
            256 => width = Some(scalar()?),
            257 => height = Some(scalar()?),
            258 => bits_offset = Some((kind, item_count, value)),
            259 => compression = scalar()?,
            262 => photometric = Some(scalar()?),
            273 => strip_offset = Some(scalar()?),
            277 => samples = Some(scalar()?),
            278 => rows_per_strip = Some(scalar()?),
            279 => strip_bytes = Some(scalar()?),
            282 => x_resolution = Some(read_rational(bytes, value as usize, endian)?),
            283 => y_resolution = Some(read_rational(bytes, value as usize, endian)?),
            284 => planar = scalar()?,
            296 => resolution_unit = scalar()?,
            338 => extra_samples = Some(scalar()?),
            _ => {}
        }
    }
    let width = width.ok_or(FormatError::Invalid("TIFF width is missing"))?;
    let height = height.ok_or(FormatError::Invalid("TIFF height is missing"))?;
    let samples = samples.ok_or(FormatError::Invalid("TIFF samples are missing"))?;
    if compression != 1
        || photometric != Some(2)
        || planar != 1
        || rows_per_strip != Some(height)
        || !matches!(samples, 3 | 4)
        || (samples == 4 && extra_samples != Some(2))
        || (samples == 3 && extra_samples.is_some())
    {
        return Err(FormatError::Unsupported(
            "TIFF must be one uncompressed chunky RGB(A) strip with straight alpha",
        ));
    }
    let (bits_kind, bits_count, bits_value) =
        bits_offset.ok_or(FormatError::Invalid("TIFF bits-per-sample is missing"))?;
    if bits_kind != 3 || bits_count != samples {
        return Err(FormatError::Unsupported(
            "TIFF channel depths are unsupported",
        ));
    }
    let bits_position = bits_value as usize;
    let mut bit_depth = None;
    for channel in 0..samples as usize {
        let depth = read_u16(bytes, bits_position + channel * 2, endian)?;
        if bit_depth
            .replace(depth)
            .is_some_and(|previous| previous != depth)
        {
            return Err(FormatError::Unsupported("TIFF channel depths differ"));
        }
    }
    let bit_depth = bit_depth.unwrap_or(0);
    let format = match bit_depth {
        8 => PixelFormat::StraightRgba8,
        16 => PixelFormat::StraightRgba16,
        _ => return Err(FormatError::Unsupported("TIFF bit depth is unsupported")),
    };
    let channel_bytes = usize::from(bit_depth / 8);
    let source_len = expected_bytes(width, height, samples as usize * channel_bytes)?;
    if strip_bytes != Some(source_len as u32) {
        return Err(FormatError::Invalid(
            "TIFF strip byte count is inconsistent",
        ));
    }
    let start = strip_offset.ok_or(FormatError::Invalid("TIFF strip offset is missing"))? as usize;
    let source = bytes
        .get(start..start + source_len)
        .ok_or(FormatError::Invalid("TIFF pixel strip is truncated"))?;
    let mut pixels = Vec::with_capacity(expected_bytes(width, height, 4 * channel_bytes)?);
    for pixel in source.chunks_exact(samples as usize * channel_bytes) {
        for channel in 0..3 {
            let start = channel * channel_bytes;
            if channel_bytes == 1 || matches!(endian, Endian::Little) {
                pixels.extend_from_slice(&pixel[start..start + channel_bytes]);
            } else {
                pixels.extend(pixel[start..start + channel_bytes].iter().rev());
            }
        }
        if samples == 4 {
            let start = 3 * channel_bytes;
            if channel_bytes == 1 || matches!(endian, Endian::Little) {
                pixels.extend_from_slice(&pixel[start..start + channel_bytes]);
            } else {
                pixels.extend(pixel[start..start + channel_bytes].iter().rev());
            }
        } else if channel_bytes == 1 {
            pixels.push(u8::MAX);
        } else {
            pixels.extend_from_slice(&u16::MAX.to_le_bytes());
        }
    }
    let dpi = |resolution: Option<(u32, u32)>| -> Option<u32> {
        let (numerator, denominator) = resolution?;
        if denominator == 0 {
            return None;
        }
        let scale = match resolution_unit {
            2 => 1_000_u64,
            3 => 2_540_u64,
            _ => return None,
        };
        u32::try_from(
            (u64::from(numerator) * scale + u64::from(denominator) / 2) / u64::from(denominator),
        )
        .ok()
    };
    CommonRaster::new(
        width,
        height,
        format,
        dpi(x_resolution),
        dpi(y_resolution),
        pixels,
    )
}

fn tiff_scalar(
    bytes: &[u8],
    value_offset: usize,
    endian: Endian,
    kind: u16,
    count: u32,
    value: u32,
) -> Result<u32, FormatError> {
    if count != 1 {
        return Err(FormatError::Unsupported("TIFF array tag is unsupported"));
    }
    match kind {
        3 => Ok(u32::from(read_u16(bytes, value_offset, endian)?)),
        4 => Ok(value),
        _ => Err(FormatError::Unsupported("TIFF scalar type is unsupported")),
    }
}

fn read_rational(bytes: &[u8], offset: usize, endian: Endian) -> Result<(u32, u32), FormatError> {
    Ok((
        read_u32(bytes, offset, endian)?,
        read_u32(bytes, offset + 4, endian)?,
    ))
}

fn read_u16(bytes: &[u8], offset: usize, endian: Endian) -> Result<u16, FormatError> {
    let value: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or(FormatError::Invalid("common raster field is truncated"))?
        .try_into()
        .map_err(|_| FormatError::Invalid("common raster field is truncated"))?;
    Ok(match endian {
        Endian::Little => u16::from_le_bytes(value),
        Endian::Big => u16::from_be_bytes(value),
    })
}

fn read_u32(bytes: &[u8], offset: usize, endian: Endian) -> Result<u32, FormatError> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(FormatError::Invalid("common raster field is truncated"))?
        .try_into()
        .map_err(|_| FormatError::Invalid("common raster field is truncated"))?;
    Ok(match endian {
        Endian::Little => u32::from_le_bytes(value),
        Endian::Big => u32::from_be_bytes(value),
    })
}

fn encode_tga(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    let width = u16::try_from(info.width)
        .map_err(|_| FormatError::Unsupported("TGA dimensions exceed 16-bit fields"))?;
    let height = u16::try_from(info.height)
        .map_err(|_| FormatError::Unsupported("TGA dimensions exceed 16-bit fields"))?;
    let mut output = vec![0_u8; 18];
    output[2] = 2;
    output[12..14].copy_from_slice(&width.to_le_bytes());
    output[14..16].copy_from_slice(&height.to_le_bytes());
    output[16] = 32;
    output[17] = 0x28; // top-left origin, eight alpha bits
    output.reserve(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        output.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(output)
}

fn decode_tga(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    if bytes.len() < 18 || bytes[1] != 0 || bytes[2] != 2 {
        return Err(FormatError::Unsupported(
            "TGA must be an uncompressed true-color image",
        ));
    }
    let id_length = bytes[0] as usize;
    let width = u32::from(u16::from_le_bytes([bytes[12], bytes[13]]));
    let height = u32::from(u16::from_le_bytes([bytes[14], bytes[15]]));
    let depth = bytes[16];
    if !matches!(depth, 24 | 32) {
        return Err(FormatError::Unsupported("TGA pixel depth is unsupported"));
    }
    let alpha_bits = bytes[17] & 0x0f;
    if (depth == 24 && alpha_bits != 0) || (depth == 32 && !matches!(alpha_bits, 0 | 8)) {
        return Err(FormatError::Unsupported(
            "TGA alpha attribute depth is unsupported",
        ));
    }
    let source_bpp = usize::from(depth / 8);
    let source_len = expected_bytes(width, height, source_bpp)?;
    let start = 18_usize
        .checked_add(id_length)
        .ok_or(FormatError::Invalid("TGA ID length overflows"))?;
    let source = bytes
        .get(start..start + source_len)
        .ok_or(FormatError::Invalid("TGA pixel data is truncated"))?;
    let top_origin = bytes[17] & 0x20 != 0;
    let right_origin = bytes[17] & 0x10 != 0;
    let mut pixels = vec![0_u8; expected_bytes(width, height, 4)?];
    for source_y in 0..height as usize {
        let destination_y = if top_origin {
            source_y
        } else {
            height as usize - 1 - source_y
        };
        for source_x in 0..width as usize {
            let destination_x = if right_origin {
                width as usize - 1 - source_x
            } else {
                source_x
            };
            let source_offset = (source_y * width as usize + source_x) * source_bpp;
            let destination_offset = (destination_y * width as usize + destination_x) * 4;
            pixels[destination_offset..destination_offset + 4].copy_from_slice(&[
                source[source_offset + 2],
                source[source_offset + 1],
                source[source_offset],
                if source_bpp == 4 && alpha_bits == 8 {
                    source[source_offset + 3]
                } else {
                    u8::MAX
                },
            ]);
        }
    }
    CommonRaster::new(
        width,
        height,
        PixelFormat::StraightRgba8,
        None,
        None,
        pixels,
    )
}

fn encode_bmp(info: CommonRasterInfo, pixels: &[u8]) -> Result<Vec<u8>, FormatError> {
    const DIB_BYTES: u32 = 124;
    const PIXEL_OFFSET: u32 = 14 + DIB_BYTES;
    let pixel_length = u32::try_from(pixels.len())
        .map_err(|_| FormatError::Invalid("BMP pixel payload is too large"))?;
    let file_size = PIXEL_OFFSET
        .checked_add(pixel_length)
        .ok_or(FormatError::Invalid("BMP file length overflows"))?;
    let mut output = Vec::with_capacity(file_size as usize);
    output.extend_from_slice(b"BM");
    output.extend_from_slice(&file_size.to_le_bytes());
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&PIXEL_OFFSET.to_le_bytes());
    output.extend_from_slice(&DIB_BYTES.to_le_bytes());
    output.extend_from_slice(&(info.width as i32).to_le_bytes());
    output.extend_from_slice(&(-(info.height as i32)).to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&32_u16.to_le_bytes());
    output.extend_from_slice(&3_u32.to_le_bytes()); // BI_BITFIELDS
    output.extend_from_slice(&pixel_length.to_le_bytes());
    output.extend_from_slice(
        &(info.dpi_x_milli.map_or(0, dpi_milli_to_pixels_per_meter) as i32).to_le_bytes(),
    );
    output.extend_from_slice(
        &(info.dpi_y_milli.map_or(0, dpi_milli_to_pixels_per_meter) as i32).to_le_bytes(),
    );
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&0x00ff_0000_u32.to_le_bytes());
    output.extend_from_slice(&0x0000_ff00_u32.to_le_bytes());
    output.extend_from_slice(&0x0000_00ff_u32.to_le_bytes());
    output.extend_from_slice(&0xff00_0000_u32.to_le_bytes());
    output.extend_from_slice(b"sRGB");
    output.resize(PIXEL_OFFSET as usize, 0);
    for pixel in pixels.chunks_exact(4) {
        output.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(output)
}

fn decode_bmp(bytes: &[u8]) -> Result<CommonRaster, FormatError> {
    if bytes.get(..2) != Some(b"BM") {
        return Err(FormatError::Invalid("BMP magic is invalid"));
    }
    let pixel_offset = read_u32(bytes, 10, Endian::Little)? as usize;
    let dib_bytes = read_u32(bytes, 14, Endian::Little)?;
    if dib_bytes < 40 {
        return Err(FormatError::Unsupported("BMP DIB header is unsupported"));
    }
    let width = read_u32(bytes, 18, Endian::Little)? as i32;
    let height = read_u32(bytes, 22, Endian::Little)? as i32;
    let planes = read_u16(bytes, 26, Endian::Little)?;
    let depth = read_u16(bytes, 28, Endian::Little)?;
    let compression = read_u32(bytes, 30, Endian::Little)?;
    if width <= 0
        || height == 0
        || planes != 1
        || !matches!((depth, compression), (24, 0) | (32, 0) | (32, 3))
    {
        return Err(FormatError::Unsupported(
            "BMP must be a 24-bit RGB or 32-bit RGB/bitfield image",
        ));
    }
    if compression == 3
        && (dib_bytes < 56
            || read_u32(bytes, 54, Endian::Little)? != 0x00ff_0000
            || read_u32(bytes, 58, Endian::Little)? != 0x0000_ff00
            || read_u32(bytes, 62, Endian::Little)? != 0x0000_00ff
            || read_u32(bytes, 66, Endian::Little)? != 0xff00_0000)
    {
        return Err(FormatError::Unsupported(
            "BMP bitfield masks are unsupported",
        ));
    }
    let width = width as u32;
    let absolute_height = height.unsigned_abs();
    validate_dimensions(width, absolute_height)?;
    let source_bpp = usize::from(depth / 8);
    let packed_row = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(source_bpp))
        .ok_or(FormatError::Invalid("BMP row length overflows"))?;
    let source_stride = packed_row
        .checked_add(3)
        .map(|row| row & !3)
        .ok_or(FormatError::Invalid("BMP row stride overflows"))?;
    let source_len = source_stride
        .checked_mul(absolute_height as usize)
        .filter(|bytes| *bytes <= MAX_COMMON_RASTER_BYTES)
        .ok_or(FormatError::Invalid("BMP pixel byte length overflows"))?;
    let source = bytes
        .get(pixel_offset..pixel_offset + source_len)
        .ok_or(FormatError::Invalid("BMP pixel data is truncated"))?;
    let mut pixels = vec![0_u8; expected_bytes(width, absolute_height, 4)?];
    for source_y in 0..absolute_height as usize {
        let destination_y = if height < 0 {
            source_y
        } else {
            absolute_height as usize - 1 - source_y
        };
        for x in 0..width as usize {
            let source_offset = source_y * source_stride + x * source_bpp;
            let destination_offset = (destination_y * width as usize + x) * 4;
            pixels[destination_offset..destination_offset + 4].copy_from_slice(&[
                source[source_offset + 2],
                source[source_offset + 1],
                source[source_offset],
                if source_bpp == 4 && compression == 3 {
                    source[source_offset + 3]
                } else {
                    u8::MAX
                },
            ]);
        }
    }
    let xppm = read_u32(bytes, 38, Endian::Little)? as i32;
    let yppm = read_u32(bytes, 42, Endian::Little)? as i32;
    let dpi_x = (xppm > 0).then(|| pixels_per_meter_to_dpi_milli(xppm as u32));
    let dpi_y = (yppm > 0).then(|| pixels_per_meter_to_dpi_milli(yppm as u32));
    CommonRaster::new(
        width,
        absolute_height,
        PixelFormat::StraightRgba8,
        dpi_x,
        dpi_y,
        pixels,
    )
}

fn dpi_milli_to_pixels_per_meter(dpi_milli: u32) -> u32 {
    ((u64::from(dpi_milli) * 10 + 127) / 254).min(u64::from(u32::MAX)) as u32
}

fn pixels_per_meter_to_dpi_milli(pixels_per_meter: u32) -> u32 {
    ((u64::from(pixels_per_meter) * 254 + 5) / 10).min(u64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba8() -> CommonRaster {
        CommonRaster::new(
            3,
            2,
            PixelFormat::StraightRgba8,
            Some(96_000),
            Some(120_000),
            vec![
                1, 2, 3, 4, 5, 6, 7, 128, 8, 9, 10, 255, 11, 12, 13, 0, 14, 15, 16, 200, 17, 18,
                19, 255,
            ],
        )
        .unwrap()
    }

    fn rgba16() -> CommonRaster {
        let channels = [
            0_u16, 1, 257, 65_535, 2, 3, 4, 32_768, 5, 6, 7, 0, 8, 9, 10, 100,
        ];
        let pixels = channels
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        CommonRaster::new(
            2,
            2,
            PixelFormat::StraightRgba16,
            Some(72_000),
            Some(144_000),
            pixels,
        )
        .unwrap()
    }

    fn assert_dpi_close(actual: Option<u32>, expected: Option<u32>) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => assert!(actual.abs_diff(expected) <= 20),
            (actual, expected) => assert_eq!(actual, expected),
        }
    }

    #[test]
    fn m4_common_formats_round_trip_depth_alpha_dimensions_and_dpi() {
        for format in [
            CommonRasterFormat::Png,
            CommonRasterFormat::Tiff,
            CommonRasterFormat::Tga,
            CommonRasterFormat::Bmp,
        ] {
            let source = rgba8();
            let encoded = encode_common_raster(format, &source, false).unwrap();
            let decoded = decode_common_raster(format, &encoded).unwrap();
            assert_eq!(decoded.info.width, source.info.width, "{format:?}");
            assert_eq!(decoded.info.height, source.info.height, "{format:?}");
            assert_eq!(
                decoded.info.pixel_format, source.info.pixel_format,
                "{format:?}"
            );
            assert_eq!(decoded.pixels, source.pixels, "{format:?}");
            if format.supports_dpi() {
                assert_dpi_close(decoded.info.dpi_x_milli, source.info.dpi_x_milli);
                assert_dpi_close(decoded.info.dpi_y_milli, source.info.dpi_y_milli);
            } else {
                assert_eq!(decoded.info.dpi_x_milli, None);
                assert_eq!(decoded.info.dpi_y_milli, None);
            }
        }
        for format in [CommonRasterFormat::Png, CommonRasterFormat::Tiff] {
            let source = rgba16();
            let encoded = encode_common_raster(format, &source, false).unwrap();
            let decoded = decode_common_raster(format, &encoded).unwrap();
            assert_eq!(decoded.info.width, source.info.width, "{format:?}");
            assert_eq!(decoded.info.height, source.info.height, "{format:?}");
            assert_eq!(
                decoded.info.pixel_format, source.info.pixel_format,
                "{format:?}"
            );
            assert_dpi_close(decoded.info.dpi_x_milli, source.info.dpi_x_milli);
            assert_dpi_close(decoded.info.dpi_y_milli, source.info.dpi_y_milli);
            assert_eq!(decoded.pixels, source.pixels, "{format:?}");
        }
        assert!(matches!(
            encode_common_raster(CommonRasterFormat::Tga, &rgba16(), false),
            Err(FormatError::Unsupported(_))
        ));
    }

    #[test]
    fn m4_white_background_export_is_explicit_and_alpha_safe() {
        let source = rgba8();
        let encoded = encode_common_raster(CommonRasterFormat::Png, &source, true).unwrap();
        let decoded = decode_common_raster(CommonRasterFormat::Png, &encoded).unwrap();
        assert!(decoded.pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
        assert_eq!(&decoded.pixels[12..16], &[255, 255, 255, 255]);
        assert_eq!(source.pixels[15], 0);
    }

    #[test]
    fn m4_tiff_declares_straight_alpha_and_rejects_associated_alpha() {
        let mut encoded = encode_common_raster(CommonRasterFormat::Tiff, &rgba8(), false).unwrap();
        let entry = encoded
            .windows(2)
            .enumerate()
            .find_map(|(index, bytes)| (bytes == 338_u16.to_le_bytes()).then_some(index))
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(encoded[entry + 8..entry + 12].try_into().unwrap()),
            2
        );
        encoded[entry + 8..entry + 12].copy_from_slice(&1_u32.to_le_bytes());
        assert!(matches!(
            decode_common_raster(CommonRasterFormat::Tiff, &encoded),
            Err(FormatError::Unsupported(_))
        ));
    }

    #[test]
    fn m4_common_raster_revalidates_public_metadata_before_allocation() {
        let mut mutated = rgba8();
        mutated.info.width = MAX_RASTER_DIMENSION + 1;
        assert!(matches!(
            encode_common_raster(CommonRasterFormat::Png, &mutated, false),
            Err(FormatError::Invalid(
                "common raster dimensions are outside bounds"
            ))
        ));

        let mut bmp = vec![0_u8; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[10..14].copy_from_slice(&54_u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&(MAX_RASTER_DIMENSION + 1).to_le_bytes());
        bmp[22..26].copy_from_slice(&1_i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&32_u16.to_le_bytes());
        assert!(matches!(
            decode_common_raster(CommonRasterFormat::Bmp, &bmp),
            Err(FormatError::Invalid(
                "common raster dimensions are outside bounds"
            ))
        ));
    }

    #[test]
    fn m4_png_expands_indexed_palette_and_transparency() {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, 2, 1);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![10, 20, 30, 40, 50, 60]);
            encoder.set_trns(vec![0, 128]);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[0, 1]).unwrap();
        }

        let decoded = decode_common_raster(CommonRasterFormat::Png, &encoded).unwrap();
        assert_eq!(decoded.info.pixel_format, PixelFormat::StraightRgba8);
        assert_eq!(decoded.pixels, [10, 20, 30, 0, 40, 50, 60, 128]);
    }

    #[test]
    fn m4_tga_honors_right_origin_and_alpha_attribute_bits() {
        let mut encoded = vec![0_u8; 18];
        encoded[2] = 2;
        encoded[12..14].copy_from_slice(&2_u16.to_le_bytes());
        encoded[14..16].copy_from_slice(&1_u16.to_le_bytes());
        encoded[16] = 32;
        encoded[17] = 0x38; // top-right origin, eight alpha bits
        encoded.extend_from_slice(&[3, 2, 1, 4, 7, 6, 5, 8]);

        let decoded = decode_common_raster(CommonRasterFormat::Tga, &encoded).unwrap();
        assert_eq!(decoded.pixels, [5, 6, 7, 8, 1, 2, 3, 4]);

        encoded[17] = 0x30; // top-right origin, no declared alpha channel
        let decoded = decode_common_raster(CommonRasterFormat::Tga, &encoded).unwrap();
        assert_eq!(decoded.pixels, [5, 6, 7, 255, 1, 2, 3, 255]);
    }

    #[test]
    fn m4_bmp_accepts_padded_24_bit_rows_and_validates_bitfield_masks() {
        let mut encoded = vec![0_u8; 54];
        encoded[..2].copy_from_slice(b"BM");
        encoded[2..6].copy_from_slice(&62_u32.to_le_bytes());
        encoded[10..14].copy_from_slice(&54_u32.to_le_bytes());
        encoded[14..18].copy_from_slice(&40_u32.to_le_bytes());
        encoded[18..22].copy_from_slice(&2_i32.to_le_bytes());
        encoded[22..26].copy_from_slice(&(-1_i32).to_le_bytes());
        encoded[26..28].copy_from_slice(&1_u16.to_le_bytes());
        encoded[28..30].copy_from_slice(&24_u16.to_le_bytes());
        encoded[34..38].copy_from_slice(&8_u32.to_le_bytes());
        encoded.extend_from_slice(&[3, 2, 1, 6, 5, 4, 0, 0]);

        let decoded = decode_common_raster(CommonRasterFormat::Bmp, &encoded).unwrap();
        assert_eq!(decoded.pixels, [1, 2, 3, 255, 4, 5, 6, 255]);

        let mut invalid_masks =
            encode_common_raster(CommonRasterFormat::Bmp, &rgba8(), false).unwrap();
        invalid_masks[54..58].copy_from_slice(&0x0000_00ff_u32.to_le_bytes());
        assert!(matches!(
            decode_common_raster(CommonRasterFormat::Bmp, &invalid_masks),
            Err(FormatError::Unsupported(
                "BMP bitfield masks are unsupported"
            ))
        ));
    }
}
