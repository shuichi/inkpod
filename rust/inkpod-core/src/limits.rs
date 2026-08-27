/// Feature bits supported by this version of the Rust Core API.
pub const CORE_FEATURES: u64 = 1;
/// Snapshot feature bit indicating legacy-white color-check rendering.
pub const SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE: u64 = 1 << 0;
/// Snapshot feature bit indicating native-alpha color-check rendering.
pub const SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA: u64 = 1 << 1;
/// Snapshot feature bit identifying an allocation-free SolidWhite Genesis base.
pub const SNAPSHOT_FEATURE_SOLID_WHITE_BASE: u64 = 1 << 2;
/// Default horizontal and vertical resolution in thousandths of a DPI.
pub const DEFAULT_DPI_MILLI: u32 = 96_000;
/// Maximum number of layers accepted in one document.
pub const MAX_LAYERS: usize = 4_096;
/// Maximum number of planes accepted in one layer.
pub const MAX_PLANES_PER_LAYER: usize = 4_096;
/// Maximum number of document guides.
pub const MAX_GUIDES: usize = 4_096;
/// Maximum number of configured shortcut commands.
pub const MAX_SHORTCUTS: usize = 1_024;
/// Maximum number of key strokes in one shortcut sequence.
pub const MAX_SHORTCUT_STROKES: usize = 4;
/// Shortcut modifier bit for the Control key.
pub const SHORTCUT_MODIFIER_CONTROL: u32 = 1 << 0;
/// Shortcut modifier bit for the Shift key.
pub const SHORTCUT_MODIFIER_SHIFT: u32 = 1 << 1;
/// Shortcut modifier bit for the Alt key.
pub const SHORTCUT_MODIFIER_ALT: u32 = 1 << 2;
/// Shortcut modifier bit distinguishing extended virtual keys.
pub const SHORTCUT_MODIFIER_EXTENDED: u32 = 1 << 3;
/// Mask containing every supported shortcut modifier bit.
pub const SHORTCUT_MODIFIER_MASK: u32 = SHORTCUT_MODIFIER_CONTROL
    | SHORTCUT_MODIFIER_SHIFT
    | SHORTCUT_MODIFIER_ALT
    | SHORTCUT_MODIFIER_EXTENDED;
