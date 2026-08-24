use super::super::{CommonRaster, FormatError};
use inkpod_image::PixelFormat;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TgaCompression {
    Uncompressed,
    RunLengthEncoded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TgaOrigin {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

impl TgaOrigin {
    pub(super) const fn descriptor_bits(self) -> u8 {
        match self {
            Self::BottomLeft => 0,
            Self::BottomRight => 0x10,
            Self::TopLeft => 0x20,
            Self::TopRight => 0x30,
        }
    }

    pub(super) const fn top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }

    pub(super) const fn right(self) -> bool {
        matches!(self, Self::BottomRight | Self::TopRight)
    }

    pub(super) const fn from_descriptor(descriptor: u8) -> Self {
        match descriptor & 0x30 {
            0x00 => Self::BottomLeft,
            0x10 => Self::BottomRight,
            0x20 => Self::TopLeft,
            _ => Self::TopRight,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TgaImageFormat {
    None,
    ColorMapped {
        index_depth: u8,
        entry_depth: u8,
        first_index: u16,
    },
    TrueColor {
        depth: u8,
    },
    Grayscale {
        depth: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TgaAlphaType {
    None,
    UndefinedIgnore,
    UndefinedRetain,
    Straight,
    Premultiplied,
}

impl TgaAlphaType {
    pub(super) const fn code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::UndefinedIgnore => 1,
            Self::UndefinedRetain => 2,
            Self::Straight => 3,
            Self::Premultiplied => 4,
        }
    }

    pub(super) fn from_code(code: u8) -> Result<Self, FormatError> {
        match code {
            0 => Ok(Self::None),
            1 => Ok(Self::UndefinedIgnore),
            2 => Ok(Self::UndefinedRetain),
            3 => Ok(Self::Straight),
            4 => Ok(Self::Premultiplied),
            _ => Err(FormatError::Unsupported(
                "TGA extension alpha attribute type is reserved",
            )),
        }
    }

    pub(super) const fn retains_attribute(self) -> bool {
        matches!(
            self,
            Self::UndefinedRetain | Self::Straight | Self::Premultiplied
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TgaAlphaLoss {
    Reject,
    Discard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TgaGrayscaleConversion {
    RequireExact,
    Bt709,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TgaRatio {
    pub numerator: u16,
    pub denominator: u16,
}

impl TgaRatio {
    pub(super) fn validate(self) -> Result<(), FormatError> {
        if self.denominator == 0 {
            Err(FormatError::Invalid(
                "TGA extension ratio denominator is zero",
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TgaTimestamp {
    pub month: u16,
    pub day: u16,
    pub year: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

impl TgaTimestamp {
    pub(super) fn validate(self) -> Result<(), FormatError> {
        if !(1..=12).contains(&self.month)
            || !(1..=31).contains(&self.day)
            || self.hour > 23
            || self.minute > 59
            || self.second > 59
        {
            return Err(FormatError::Invalid(
                "TGA extension timestamp is outside bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TgaDuration {
    pub hours: u16,
    pub minutes: u16,
    pub seconds: u16,
}

impl TgaDuration {
    pub(super) fn validate(self) -> Result<(), FormatError> {
        if self.minutes > 59 || self.seconds > 59 {
            return Err(FormatError::Invalid(
                "TGA extension job duration is outside bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TgaColorMap {
    pub first_index: u16,
    pub entry_depth: u8,
    /// Straight-alpha RGBA entries in file index order.
    pub entries: Vec<[u8; 4]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TgaDeveloperField {
    pub tag: u16,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TgaExtension {
    pub author_name: String,
    pub author_comments: [String; 4],
    pub timestamp: Option<TgaTimestamp>,
    pub job_name: String,
    pub job_duration: Option<TgaDuration>,
    pub software_id: String,
    pub software_version: u16,
    pub software_version_letter: Option<u8>,
    /// Straight RGBA; the on-disk extension field is A,R,G,B.
    pub key_color: [u8; 4],
    pub pixel_aspect_ratio: Option<TgaRatio>,
    pub gamma: Option<TgaRatio>,
    /// Exactly 256 straight RGBA entries with 16-bit channels.
    pub color_correction_table: Option<Vec<[u16; 4]>>,
    pub postage_stamp: Option<CommonRaster>,
    pub scan_line_table: bool,
    pub alpha_type: TgaAlphaType,
    /// Future extension bytes after the understood 495-byte version 2.0 area.
    pub extra: Vec<u8>,
}

impl Default for TgaExtension {
    fn default() -> Self {
        Self {
            author_name: String::new(),
            author_comments: std::array::from_fn(|_| String::new()),
            timestamp: None,
            job_name: String::new(),
            job_duration: None,
            software_id: String::new(),
            software_version: 0,
            software_version_letter: None,
            key_color: [0; 4],
            pixel_aspect_ratio: None,
            gamma: None,
            color_correction_table: None,
            postage_stamp: None,
            scan_line_table: false,
            alpha_type: TgaAlphaType::None,
            extra: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct TgaMetadata {
    pub image_id: Vec<u8>,
    pub x_origin: u16,
    pub y_origin: u16,
    pub extension: Option<TgaExtension>,
    pub developer_fields: Vec<TgaDeveloperField>,
    pub write_footer: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TgaEncodeOptions {
    pub image_format: TgaImageFormat,
    pub compression: TgaCompression,
    pub origin: TgaOrigin,
    pub color_map: Option<TgaColorMap>,
    pub metadata: TgaMetadata,
    pub alpha_loss: TgaAlphaLoss,
    pub grayscale_conversion: TgaGrayscaleConversion,
    pub allow_color_precision_loss: bool,
    pub allow_alpha_precision_loss: bool,
}

impl Default for TgaEncodeOptions {
    fn default() -> Self {
        Self {
            image_format: TgaImageFormat::TrueColor { depth: 32 },
            compression: TgaCompression::Uncompressed,
            origin: TgaOrigin::TopLeft,
            color_map: None,
            metadata: TgaMetadata::default(),
            alpha_loss: TgaAlphaLoss::Reject,
            grayscale_conversion: TgaGrayscaleConversion::RequireExact,
            allow_color_precision_loss: false,
            allow_alpha_precision_loss: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TgaDocument {
    pub raster: Option<CommonRaster>,
    pub options: TgaEncodeOptions,
}

pub(super) fn validate_document(document: &TgaDocument) -> Result<(), FormatError> {
    match (document.options.image_format, document.raster.as_ref()) {
        (TgaImageFormat::None, None) => {}
        (TgaImageFormat::None, Some(_)) => {
            return Err(FormatError::Invalid(
                "TGA no-image format cannot contain a raster",
            ));
        }
        (_, None) => return Err(FormatError::Invalid("TGA image format requires a raster")),
        (_, Some(raster)) => {
            raster.validate()?;
            if raster.info.pixel_format != PixelFormat::StraightRgba8 {
                return Err(FormatError::Unsupported(
                    "TGA encoder requires straight RGBA8",
                ));
            }
            if raster.info.width > u32::from(u16::MAX) || raster.info.height > u32::from(u16::MAX) {
                return Err(FormatError::Unsupported(
                    "TGA dimensions exceed 16-bit fields",
                ));
            }
        }
    }
    if document.options.image_format == TgaImageFormat::None
        && document.options.compression != TgaCompression::Uncompressed
    {
        return Err(FormatError::Invalid("TGA no-image format cannot use RLE"));
    }
    if document.options.color_map.is_some()
        && !matches!(
            document.options.image_format,
            TgaImageFormat::None | TgaImageFormat::ColorMapped { .. }
        )
    {
        return Err(FormatError::Invalid(
            "TGA writer only permits a color map for no-image or color-mapped types",
        ));
    }
    if document.options.image_format == TgaImageFormat::None
        && document
            .options
            .metadata
            .extension
            .as_ref()
            .is_some_and(|extension| extension.postage_stamp.is_some())
    {
        return Err(FormatError::Invalid(
            "TGA no-image format cannot encode a postage stamp",
        ));
    }
    if document.options.metadata.image_id.len() > usize::from(u8::MAX) {
        return Err(FormatError::Invalid("TGA Image ID exceeds 255 bytes"));
    }
    if document.options.metadata.developer_fields.len() > usize::from(u16::MAX) {
        return Err(FormatError::Invalid("TGA has too many developer fields"));
    }
    if let Some(map) = &document.options.color_map {
        validate_color_map(map)?;
    }
    if let Some(extension) = &document.options.metadata.extension {
        validate_extension(extension)?;
    }
    Ok(())
}

pub(super) fn validate_color_map(map: &TgaColorMap) -> Result<(), FormatError> {
    if !matches!(map.entry_depth, 15 | 16 | 24 | 32) {
        return Err(FormatError::Unsupported(
            "TGA color-map entry depth is unsupported",
        ));
    }
    if map.entries.is_empty() || map.entries.len() > usize::from(u16::MAX) {
        return Err(FormatError::Invalid(
            "TGA color-map entry count is outside bounds",
        ));
    }
    let last = usize::from(map.first_index)
        .checked_add(map.entries.len() - 1)
        .ok_or(FormatError::Invalid("TGA color-map index range overflows"))?;
    if last > usize::from(u16::MAX) {
        return Err(FormatError::Invalid(
            "TGA color-map index range exceeds 16-bit values",
        ));
    }
    Ok(())
}

pub(super) fn validate_extension(extension: &TgaExtension) -> Result<(), FormatError> {
    validate_ascii(&extension.author_name, 40)?;
    for comment in &extension.author_comments {
        validate_ascii(comment, 80)?;
    }
    validate_ascii(&extension.job_name, 40)?;
    validate_ascii(&extension.software_id, 40)?;
    if let Some(timestamp) = extension.timestamp {
        timestamp.validate()?;
    }
    if let Some(duration) = extension.job_duration {
        duration.validate()?;
    }
    if let Some(letter) = extension.software_version_letter
        && !letter.is_ascii_graphic()
    {
        return Err(FormatError::Invalid(
            "TGA software version letter is not printable ASCII",
        ));
    }
    if let Some(ratio) = extension.pixel_aspect_ratio {
        ratio.validate()?;
    }
    if let Some(ratio) = extension.gamma {
        ratio.validate()?;
    }
    if let Some(table) = &extension.color_correction_table
        && table.len() != 256
    {
        return Err(FormatError::Invalid(
            "TGA color-correction table must contain 256 entries",
        ));
    }
    if let Some(postage) = &extension.postage_stamp {
        postage.validate()?;
        if postage.info.pixel_format != PixelFormat::StraightRgba8
            || postage.info.width == 0
            || postage.info.height == 0
            || postage.info.width > u32::from(u8::MAX)
            || postage.info.height > u32::from(u8::MAX)
        {
            return Err(FormatError::Invalid(
                "TGA postage stamp must be a nonempty RGBA8 image at most 255 by 255",
            ));
        }
    }
    if 495_usize
        .checked_add(extension.extra.len())
        .is_none_or(|size| size > usize::from(u16::MAX))
    {
        return Err(FormatError::Invalid("TGA extension area is too large"));
    }
    Ok(())
}

fn validate_ascii(value: &str, maximum: usize) -> Result<(), FormatError> {
    if value.len() > maximum || !value.is_ascii() || value.as_bytes().contains(&0) {
        Err(FormatError::Invalid(
            "TGA extension text is not bounded ASCII",
        ))
    } else {
        Ok(())
    }
}
