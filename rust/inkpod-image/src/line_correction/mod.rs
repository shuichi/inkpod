mod background;
mod connect;
pub(crate) mod grid;
mod model;
mod width;

pub use background::LineBackground;
pub use connect::{apply_line_connection, virtual_gap_barrier};
pub(crate) use grid::{bounded_vec, neighbors};
pub use model::{LineCorrection, LineWidthMode};
pub use width::apply_line_width;
