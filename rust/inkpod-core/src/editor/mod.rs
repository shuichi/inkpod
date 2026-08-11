//! Core-owned EditorDefaults, per-document-session EditorState, and canonical EDIT DTOs.

pub(crate) mod codec;
mod model;
mod operations;

pub use model::{
    ColorChartCursor, EditTarget, EditorBrushOptions, EditorDefaults, EditorFillOptions,
    EditorFrameDisposition, EditorRevision, EditorSavepointToken, EditorSelectionOptions,
    EditorSelectionShape, EditorState, EditorStateDigest, EditorStateInfo, EditorStateUpdate,
    EditorStrokeInput, EditorTarget, EditorTool, EditorToolStyle, EditorVectorOptions,
    InitialDocumentSpec, MAX_EDIT_TARGETS, PaletteCursor,
};

pub(crate) use model::EditorSessionState;
