use super::LineBackground;

/// Changes an existing raster line, independently of brush or selection width.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineWidthMode {
    /// Circular dilation by the specified one-sided integer radius.
    Thicken,
    /// Circular erosion by the specified one-sided integer radius.
    Thin,
    /// Reconstruct the topology-preserving centerline at a specified full width.
    Uniform,
}

/// Fixed-width, deterministic line-correction parameters in document pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineCorrection {
    /// Remove bounded components using the supplied background policy.
    Dust(crate::DustRemoval),
    /// Connect mutually unambiguous facing endpoints. `gap` counts empty grid steps.
    Connect {
        gap: u32,
        width: u32,
        background: LineBackground,
    },
    /// Modify only the operation mask; neighborhood reads use the entire source.
    Width {
        mode: LineWidthMode,
        amount: u32,
        background: LineBackground,
    },
}
