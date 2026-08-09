//! Deterministic, bounded blank-cell creation planning.

use super::*;
use inkpod_image::div_round_ties_even_i128;

/// Maximum number of cells accepted by one creation plan.
pub const MAX_CELL_CREATION_COUNT: u32 = 64;
/// Maximum supported resolution in thousandths of a DPI.
pub const MAX_CELL_CREATION_DPI_MILLI: u32 = 2_400_000;
const RATIO_SCALE: i128 = 1_000;
const MICROMETRES_PER_INCH_MILLI: i128 = 25_400_000;

/// Determines whether entered dimensions describe the final image or the 100% frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellSizing {
    /// Exact final raster dimensions in pixels.
    ImagePixels {
        /// Final image width.
        width: u32,
        /// Final image height.
        height: u32,
    },
    /// Physical 100% frame dimensions in micrometres.
    FrameMicrometres {
        /// Physical frame width.
        width: u32,
        /// Physical frame height.
        height: u32,
    },
}

/// One of the five supported reference and maximum-close alignment anchors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameAnchor {
    /// Upper-left anchor.
    TopLeft,
    /// Upper-right anchor.
    TopRight,
    /// Centre anchor.
    Center,
    /// Lower-left anchor.
    BottomLeft,
    /// Lower-right anchor.
    BottomRight,
}

/// Complete typed input for one bounded blank-cell batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellCreationOptions {
    /// Interpretation and dimensions of the entered size.
    pub sizing: CellSizing,
    /// Horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
    /// Per-edge margin as thousandths of the 100% frame dimension.
    pub margin_milli: u32,
    /// Safe-frame size as thousandths of the 100% frame size.
    pub safe_frame_ratio_milli: u32,
    /// Maximum-close size as thousandths of the 100% frame size.
    pub maximum_close_ratio_milli: u32,
    /// Reference and maximum-close alignment anchor.
    pub anchor: FrameAnchor,
    /// Requested initial layer kind.
    pub initial_layer_kind: LayerKind,
    /// Color storage depth for the initial coloring topology.
    pub pixel_format: PixelFormat,
    /// Number of independent cells to stage.
    pub count: u32,
}

/// One validated immutable cell specification in a creation plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellCreationPlanItem {
    width: u32,
    height: u32,
    dpi_x_milli: u32,
    dpi_y_milli: u32,
    frames: FrameMetadata,
    initial_layer_kind: LayerKind,
    pixel_format: PixelFormat,
}

impl CellCreationPlanItem {
    /// Returns the final raster width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }
    /// Returns the final raster height.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
    /// Returns horizontal resolution in thousandths of a DPI.
    #[must_use]
    pub const fn dpi_x_milli(self) -> u32 {
        self.dpi_x_milli
    }
    /// Returns vertical resolution in thousandths of a DPI.
    #[must_use]
    pub const fn dpi_y_milli(self) -> u32 {
        self.dpi_y_milli
    }
    /// Returns the canonical frame geometry.
    #[must_use]
    pub const fn frames(self) -> FrameMetadata {
        self.frames
    }
    /// Returns the initial layer kind.
    #[must_use]
    pub const fn initial_layer_kind(self) -> LayerKind {
        self.initial_layer_kind
    }
    /// Returns the selected color storage format.
    #[must_use]
    pub const fn pixel_format(self) -> PixelFormat {
        self.pixel_format
    }
}

/// Owned immutable plan shared by preview and commit routes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellCreationPlan {
    items: Vec<CellCreationPlanItem>,
}

impl CellCreationPlan {
    /// Returns the number of independently staged cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }
    /// Returns whether the plan has no items. Valid plans are never empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    /// Returns an item by zero-based sequence index.
    #[must_use]
    pub fn item(&self, index: usize) -> Option<&CellCreationPlanItem> {
        self.items.get(index)
    }
    /// Iterates over the immutable plan items.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &CellCreationPlanItem> {
        self.items.iter()
    }
}

/// Validates all creation inputs and returns the canonical immutable preview/commit plan.
///
/// This function owns no Core state and consumes no stable IDs. Invalid dimensions,
/// ratios, formats, counts, and arithmetic results return an error without side effects.
pub fn plan_cell_creation(options: &CellCreationOptions) -> Result<CellCreationPlan, CoreError> {
    if options.count == 0 || options.count > MAX_CELL_CREATION_COUNT {
        return Err(CoreError::InvalidArgument(
            "cell creation count is out of range",
        ));
    }
    if options.dpi_x_milli == 0
        || options.dpi_y_milli == 0
        || options.dpi_x_milli > MAX_CELL_CREATION_DPI_MILLI
        || options.dpi_y_milli > MAX_CELL_CREATION_DPI_MILLI
    {
        return Err(CoreError::InvalidArgument(
            "cell creation DPI is out of range",
        ));
    }
    if options.margin_milli > 1_000
        || !(1..=1_000).contains(&options.safe_frame_ratio_milli)
        || !(1..=1_000).contains(&options.maximum_close_ratio_milli)
    {
        return Err(CoreError::InvalidArgument(
            "cell creation frame ratio is out of range",
        ));
    }
    if !matches!(
        options.pixel_format,
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(CoreError::InvalidArgument(
            "cell creation color format is unsupported",
        ));
    }

    let (width, height, hundred, margins) = match options.sizing {
        CellSizing::ImagePixels { width, height } => {
            validate_dimension(width)?;
            validate_dimension(height)?;
            let frame_width = rounded(width, 1_000, 1_000 + options.margin_milli * 2)?;
            let frame_height = rounded(height, 1_000, 1_000 + options.margin_milli * 2)?;
            validate_dimension(frame_width)?;
            validate_dimension(frame_height)?;
            let left = (width - frame_width) / 2;
            let top = (height - frame_height) / 2;
            let margins = Margins {
                left,
                top,
                right: width - frame_width - left,
                bottom: height - frame_height - top,
            };
            (
                width,
                height,
                rect(left, top, frame_width, frame_height)?,
                margins,
            )
        }
        CellSizing::FrameMicrometres { width, height } => {
            if width == 0 || height == 0 {
                return Err(CoreError::InvalidArgument(
                    "physical frame dimension must be nonzero",
                ));
            }
            let frame_width = rounded_physical(width, options.dpi_x_milli)?;
            let frame_height = rounded_physical(height, options.dpi_y_milli)?;
            validate_dimension(frame_width)?;
            validate_dimension(frame_height)?;
            let left = rounded(frame_width, options.margin_milli, 1_000)?;
            let top = rounded(frame_height, options.margin_milli, 1_000)?;
            let final_width = frame_width
                .checked_add(
                    left.checked_mul(2)
                        .ok_or(CoreError::InvalidArgument("cell width overflows"))?,
                )
                .ok_or(CoreError::InvalidArgument("cell width overflows"))?;
            let final_height = frame_height
                .checked_add(
                    top.checked_mul(2)
                        .ok_or(CoreError::InvalidArgument("cell height overflows"))?,
                )
                .ok_or(CoreError::InvalidArgument("cell height overflows"))?;
            validate_dimension(final_width)?;
            validate_dimension(final_height)?;
            (
                final_width,
                final_height,
                rect(left, top, frame_width, frame_height)?,
                Margins {
                    left,
                    top,
                    right: left,
                    bottom: top,
                },
            )
        }
    };
    let safe_frame = scaled_rect(hundred, options.safe_frame_ratio_milli, FrameAnchor::Center)?;
    let maximum_close_frame =
        scaled_rect(hundred, options.maximum_close_ratio_milli, options.anchor)?;
    let reference_frame = RectI32 {
        x: anchor_coordinate(hundred.x, hundred.width, options.anchor, true),
        y: anchor_coordinate(hundred.y, hundred.height, options.anchor, false),
        width: hundred.width,
        height: hundred.height,
    };
    let item = CellCreationPlanItem {
        width,
        height,
        dpi_x_milli: options.dpi_x_milli,
        dpi_y_milli: options.dpi_y_milli,
        frames: FrameMetadata {
            hundred_frame: hundred,
            reference_frame,
            drawing_frame: hundred,
            safe_frame,
            shooting_frame: hundred,
            maximum_close_frame,
            margins,
        },
        initial_layer_kind: options.initial_layer_kind,
        pixel_format: options.pixel_format,
    };
    Ok(CellCreationPlan {
        items: vec![item; options.count as usize],
    })
}

fn validate_dimension(value: u32) -> Result<(), CoreError> {
    if value == 0 || value > MAX_RASTER_DIMENSION || value > i32::MAX as u32 {
        Err(CoreError::InvalidArgument("cell dimension is out of range"))
    } else {
        Ok(())
    }
}

fn rounded(value: u32, numerator: u32, denominator: u32) -> Result<u32, CoreError> {
    let result = div_round_ties_even_i128(
        i128::from(value) * i128::from(numerator),
        i128::from(denominator),
    )
    .ok_or(CoreError::InvalidArgument(
        "cell creation arithmetic overflows",
    ))?;
    u32::try_from(result)
        .map_err(|_| CoreError::InvalidArgument("cell creation arithmetic overflows"))
}

fn rounded_physical(micrometres: u32, dpi_milli: u32) -> Result<u32, CoreError> {
    let result = div_round_ties_even_i128(
        i128::from(micrometres) * i128::from(dpi_milli),
        MICROMETRES_PER_INCH_MILLI,
    )
    .ok_or(CoreError::InvalidArgument("physical conversion overflows"))?;
    u32::try_from(result).map_err(|_| CoreError::InvalidArgument("physical conversion overflows"))
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Result<RectI32, CoreError> {
    Ok(RectI32 {
        x: i32::try_from(x)
            .map_err(|_| CoreError::InvalidArgument("frame coordinate is out of range"))?,
        y: i32::try_from(y)
            .map_err(|_| CoreError::InvalidArgument("frame coordinate is out of range"))?,
        width: i32::try_from(width)
            .map_err(|_| CoreError::InvalidArgument("frame dimension is out of range"))?,
        height: i32::try_from(height)
            .map_err(|_| CoreError::InvalidArgument("frame dimension is out of range"))?,
    })
}

fn scaled_rect(base: RectI32, ratio: u32, anchor: FrameAnchor) -> Result<RectI32, CoreError> {
    let width = i32::try_from(rounded(base.width as u32, ratio, RATIO_SCALE as u32)?)
        .map_err(|_| CoreError::InvalidArgument("scaled frame width is out of range"))?;
    let height = i32::try_from(rounded(base.height as u32, ratio, RATIO_SCALE as u32)?)
        .map_err(|_| CoreError::InvalidArgument("scaled frame height is out of range"))?;
    let x = match anchor {
        FrameAnchor::TopLeft | FrameAnchor::BottomLeft => base.x,
        FrameAnchor::TopRight | FrameAnchor::BottomRight => base.x + base.width - width,
        FrameAnchor::Center => base.x + (base.width - width) / 2,
    };
    let y = match anchor {
        FrameAnchor::TopLeft | FrameAnchor::TopRight => base.y,
        FrameAnchor::BottomLeft | FrameAnchor::BottomRight => base.y + base.height - height,
        FrameAnchor::Center => base.y + (base.height - height) / 2,
    };
    Ok(RectI32 {
        x,
        y,
        width,
        height,
    })
}

fn anchor_coordinate(origin: i32, extent: i32, anchor: FrameAnchor, horizontal: bool) -> i32 {
    let trailing = if horizontal {
        matches!(anchor, FrameAnchor::TopRight | FrameAnchor::BottomRight)
    } else {
        matches!(anchor, FrameAnchor::BottomLeft | FrameAnchor::BottomRight)
    };
    if trailing {
        origin + extent
    } else if anchor == FrameAnchor::Center {
        origin + extent / 2
    } else {
        origin
    }
}
