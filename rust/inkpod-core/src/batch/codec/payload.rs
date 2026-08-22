use super::super::*;

#[derive(Default)]
pub(super) struct PayloadWriter {
    pub(super) bytes: Vec<u8>,
}

impl PayloadWriter {
    pub(super) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn pixel(&mut self, value: PixelValue) {
        match value {
            PixelValue::Binary(value) => {
                self.u32(1);
                self.u32(u32::from(value));
            }
            PixelValue::Grayscale8(value) => {
                self.u32(2);
                self.u32(u32::from(value));
            }
            PixelValue::Grayscale16(value) => {
                self.u32(3);
                self.u32(u32::from(value));
            }
            PixelValue::Rgba(value) => {
                self.u32(4);
                for component in value {
                    self.u32(u32::from(component));
                }
            }
            PixelValue::Rgba16(value) => {
                self.u32(5);
                for component in value {
                    self.u32(u32::from(component));
                }
            }
        }
    }
}

pub(super) struct PayloadReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> PayloadReader<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub(super) fn u32(&mut self) -> Result<u32, CoreError> {
        let end = self
            .cursor
            .checked_add(4)
            .ok_or(CoreError::InvalidArgument(
                "batch operation payload offset overflows",
            ))?;
        let bytes: [u8; 4] = self
            .bytes
            .get(self.cursor..end)
            .ok_or(CoreError::InvalidArgument(
                "batch operation payload is truncated",
            ))?
            .try_into()
            .map_err(|_| CoreError::InvalidArgument("batch u32 payload is truncated"))?;
        self.cursor = end;
        Ok(u32::from_le_bytes(bytes))
    }

    pub(super) fn u16(&mut self) -> Result<u16, CoreError> {
        u16::try_from(self.u32()?)
            .map_err(|_| CoreError::InvalidArgument("batch u16 payload is invalid"))
    }

    pub(super) fn boolean(&mut self) -> Result<bool, CoreError> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CoreError::InvalidArgument(
                "batch boolean payload is invalid",
            )),
        }
    }

    pub(super) fn count(&mut self, maximum: usize) -> Result<usize, CoreError> {
        let count = self.u32()? as usize;
        if count > maximum {
            return Err(CoreError::InvalidArgument(
                "batch payload count exceeds the bounded limit",
            ));
        }
        Ok(count)
    }

    pub(super) fn pixel(&mut self) -> Result<PixelValue, CoreError> {
        match self.u32()? {
            1 => Ok(PixelValue::Binary(u8::try_from(self.u32()?).map_err(
                |_| CoreError::InvalidArgument("batch binary color is invalid"),
            )?)),
            2 => Ok(PixelValue::Grayscale8(u8::try_from(self.u32()?).map_err(
                |_| CoreError::InvalidArgument("batch grayscale color is invalid"),
            )?)),
            3 => Ok(PixelValue::Grayscale16(self.u16()?)),
            4 => {
                let mut value = [0_u8; 4];
                for component in &mut value {
                    *component = u8::try_from(self.u32()?)
                        .map_err(|_| CoreError::InvalidArgument("batch RGBA8 color is invalid"))?;
                }
                Ok(PixelValue::Rgba(value))
            }
            5 => {
                let mut value = [0_u16; 4];
                for component in &mut value {
                    *component = self.u16()?;
                }
                Ok(PixelValue::Rgba16(value))
            }
            _ => Err(CoreError::InvalidArgument(
                "batch pixel payload kind is unknown",
            )),
        }
    }

    pub(super) fn finish(&self) -> Result<(), CoreError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(CoreError::InvalidArgument(
                "batch operation payload has trailing bytes",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_rejects_truncated_oversized_and_trailing_payloads() {
        assert!(PayloadReader::new(&[0, 0, 0]).u32().is_err());

        let oversized_count = 2_u32.to_le_bytes();
        let mut oversized = PayloadReader::new(&oversized_count);
        assert!(oversized.count(1).is_err());

        let mut trailing = PayloadReader::new(&[1, 0, 0, 0, 9]);
        assert_eq!(trailing.u32(), Ok(1));
        assert!(trailing.finish().is_err());
    }
}
