//! Selection, clipboard, and floating-selection operations.

use super::*;
use crate::document::{
    bounded_document_pixels, ensure_editable_plane, unique_layer_name, validate_node_name,
};
use crate::snapshot::{blend_rgba_over, blend_rgba16_over};
use crate::transform::{convert_plane_pixel, zero_pixel};

mod clipboard;
mod geometry;
mod mask;
mod operations;

pub(super) use clipboard::*;
pub(super) use geometry::*;
pub(super) use mask::*;
