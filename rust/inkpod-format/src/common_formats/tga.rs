mod api;
mod decode;
mod encode;
mod model;

pub(super) use api::{decode_tga, encode_tga};
pub use api::{decode_tga_document, encode_tga_document, encode_tga_with_options};
pub use model::{
    TgaAlphaLoss, TgaAlphaType, TgaColorMap, TgaCompression, TgaDeveloperField, TgaDocument,
    TgaDuration, TgaEncodeOptions, TgaExtension, TgaGrayscaleConversion, TgaImageFormat,
    TgaMetadata, TgaOrigin, TgaRatio, TgaTimestamp,
};
