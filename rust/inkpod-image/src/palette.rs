use crate::{PixelValue, RasterError};

pub const MAX_PALETTE_COLORS: usize = 4_096;
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Palette {
    colors: Vec<PixelValue>,
}

impl Palette {
    #[must_use]
    pub fn colors(&self) -> &[PixelValue] {
        &self.colors
    }

    pub fn push(&mut self, color: PixelValue) -> Result<(), RasterError> {
        if color.rgba16().is_none() {
            return Err(RasterError::PixelFormatMismatch);
        }
        if self.colors.len() >= MAX_PALETTE_COLORS {
            return Err(RasterError::InvalidTile);
        }
        self.colors.push(color);
        Ok(())
    }
}
