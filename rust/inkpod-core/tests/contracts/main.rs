use inkpod_core::*;

mod support;
use support::*;

mod animation;
mod assets_genesis;
mod batch;
mod cell_creation;
mod determinism;
mod document_selection;
mod editor_state;
mod effects;
mod foundation;
mod history_stroke;
mod mixed_order;
mod multi_target;
#[path = "native_v9.rs"]
mod native_v11;
mod primitive_kernel;
mod procedure_journal;
mod state_machine;
mod vector;
