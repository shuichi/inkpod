use super::*;
use crate::selection::{combine_selection_masks, paste_value};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn line_stroke(samples: Vec<StrokeSample>) -> Stroke {
    Stroke {
        tool: PaintTool::Pencil,
        plane: ActivePlane::MainLine,
        color: [0, 0, 0, 255],
        diameter: 1.0,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples,
    }
}

fn color_stroke(tool: PaintTool, diameter: f32, sample: StrokeSample) -> Stroke {
    Stroke {
        tool,
        plane: ActivePlane::Color,
        color: [12, 34, 56, 255],
        diameter,
        auto_erase: false,
        pressure_size: false,
        coordinate_space: CoordinateSpace::Document,
        samples: vec![sample],
    }
}

fn fill_request(seed_x: u32, seed_y: u32, color: [u8; 4]) -> FillRequest {
    FillRequest {
        operation: FillOperation::Seed,
        seed_x,
        seed_y,
        color: PixelValue::Rgba(color),
        selection: None,
        use_document_selection: false,
        tolerance: 0,
        detached_regions: false,
        overflow_abort: false,
        gap_close: 0,
        transparent_only: false,
        inclusion_mode: InclusionMode::None,
        inclusion_colors: Vec::new(),
        extension_distance: 0,
    }
}

mod animation;
mod document;
mod foundation;
mod history_stroke;
mod vector;
