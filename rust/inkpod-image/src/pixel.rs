#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    BinaryMask8,
    Grayscale8,
    Grayscale16,
    StraightRgba8,
    StraightRgba16,
    PremultipliedBgra8,
}

impl PixelFormat {
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::BinaryMask8 | Self::Grayscale8 => 1,
            Self::Grayscale16 => 2,
            Self::StraightRgba8 | Self::PremultipliedBgra8 => 4,
            Self::StraightRgba16 => 8,
        }
    }

    #[must_use]
    pub const fn is_color(self) -> bool {
        matches!(
            self,
            Self::StraightRgba8 | Self::StraightRgba16 | Self::PremultipliedBgra8
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelValue {
    Binary(u8),
    Grayscale8(u8),
    Grayscale16(u16),
    Rgba([u8; 4]),
    Rgba16([u16; 4]),
}

impl PixelValue {
    #[must_use]
    pub const fn is_zero(self) -> bool {
        match self {
            Self::Binary(value) => value == 0,
            Self::Grayscale8(value) => value == 0,
            Self::Grayscale16(value) => value == 0,
            Self::Rgba(value) => value[0] == 0 && value[1] == 0 && value[2] == 0 && value[3] == 0,
            Self::Rgba16(value) => value[0] == 0 && value[1] == 0 && value[2] == 0 && value[3] == 0,
        }
    }

    #[must_use]
    pub const fn is_transparent(self) -> bool {
        match self {
            Self::Rgba(value) => value[3] == 0,
            Self::Rgba16(value) => value[3] == 0,
            Self::Binary(value) | Self::Grayscale8(value) => value == 0,
            Self::Grayscale16(value) => value == 0,
        }
    }

    #[must_use]
    pub const fn is_exact_white(self) -> bool {
        match self {
            Self::Rgba(value) => value[0] == u8::MAX && value[1] == u8::MAX && value[2] == u8::MAX,
            Self::Rgba16(value) => {
                value[0] == u16::MAX && value[1] == u16::MAX && value[2] == u16::MAX
            }
            _ => false,
        }
    }

    #[must_use]
    pub const fn rgba16(self) -> Option<[u16; 4]> {
        match self {
            Self::Rgba(value) => Some([
                value[0] as u16 * 257,
                value[1] as u16 * 257,
                value[2] as u16 * 257,
                value[3] as u16 * 257,
            ]),
            Self::Rgba16(value) => Some(value),
            _ => None,
        }
    }
}
