use crate::{PixelFormat, PixelValue};

/// Foreground/background interpretation, retaining native 8/16-bit precision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LineBackground {
    /// Core resolves this to white+transparent on MainLine, transparent elsewhere.
    /// A standalone image operation treats it as transparent only.
    #[default]
    PlaneDefault,
    /// Scalar zero or RGBA alpha zero. Unused RGB at alpha zero is ignored.
    Transparent,
    /// Transparent plus one exact native-depth straight-RGBA background color.
    TransparentOrColor([u16; 4]),
}

impl LineBackground {
    pub(crate) fn validate(self, format: PixelFormat) -> Result<(), crate::RasterError> {
        if let Self::TransparentOrColor(color) = self {
            if format == PixelFormat::StraightRgba8
                && color.iter().any(|channel| channel % 257 != 0)
            {
                return Err(crate::RasterError::PixelFormatMismatch);
            }
        }
        Ok(())
    }

    /// Returns whether the source pixel belongs to the configured background.
    #[must_use]
    pub fn contains(self, pixel: PixelValue) -> bool {
        pixel.is_transparent()
            || matches!(self, Self::TransparentOrColor(color) if pixel.rgba16() == Some(color))
    }

    pub(crate) fn coverage(self, pixel: PixelValue) -> u16 {
        if self.contains(pixel) {
            return 0;
        }
        match pixel {
            PixelValue::Binary(v) | PixelValue::Grayscale8(v) => u16::from(v) * 257,
            PixelValue::Grayscale16(v) => v,
            PixelValue::Rgba(_) | PixelValue::Rgba16(_) => {
                let rgba = pixel.rgba16().expect("RGBA variant");
                if let Self::TransparentOrColor(color) = self {
                    let distance = (0..4)
                        .map(|c| rgba[c].abs_diff(color[c]))
                        .max()
                        .unwrap_or(0);
                    // Exact non-background native values must remain foreground,
                    // even when the contrast/alpha ranking rounds below one.
                    (((u32::from(distance) * u32::from(rgba[3]) + 32767) / 65535) as u16).max(1)
                } else {
                    rgba[3]
                }
            }
        }
    }

    pub(crate) fn empty(self, format: PixelFormat) -> PixelValue {
        match format {
            PixelFormat::BinaryMask8 => PixelValue::Binary(0),
            PixelFormat::Grayscale8 => PixelValue::Grayscale8(0),
            PixelFormat::Grayscale16 => PixelValue::Grayscale16(0),
            _ => {
                let color = match self {
                    Self::TransparentOrColor(c) => c,
                    _ => [0; 4],
                };
                if format == PixelFormat::StraightRgba16 {
                    PixelValue::Rgba16(color)
                } else {
                    PixelValue::Rgba(color.map(|c| ((u32::from(c) + 128) / 257) as u8))
                }
            }
        }
    }

    pub(crate) fn normalized_background(self, value: PixelValue) -> PixelValue {
        if !value.is_transparent() {
            return value;
        }
        match value {
            PixelValue::Rgba(_) => PixelValue::Rgba([0; 4]),
            PixelValue::Rgba16(_) => PixelValue::Rgba16([0; 4]),
            _ => value,
        }
    }
}
