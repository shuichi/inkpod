//! Immutable document Genesis and typed base-surface metadata.

use crate::{AssetId, CellDocument, StateId};

/// The immutable surface below every editable layer in a document.
///
/// A solid-white base contributes only to a flattened composite. An asset base
/// names canonical immutable pixels owned by the document's Core asset store.
/// Neither variant is an editable layer, plane, or selection mask.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaseSurface {
    /// An allocation-free opaque sRGB white underlay.
    SolidWhite,
    /// A canonical raster asset whose dimensions equal the document paper.
    Asset(AssetId),
}

/// Read-only identity and base-surface metadata for the active Genesis state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenesisInfo {
    /// Persistent Genesis state identifier, always [`StateId::GENESIS`].
    pub state_id: StateId,
    /// Stable Document ID in the active document namespace.
    pub document_id: u64,
    /// Distinct stable Cell ID in the same namespace.
    pub cell_id: u64,
    /// Immutable base surface captured by Genesis.
    pub base_surface: BaseSurface,
}

#[derive(Clone, Debug)]
pub(crate) struct Genesis {
    pub(crate) document: CellDocument,
}

impl Genesis {
    pub(crate) const fn new(document: CellDocument) -> Self {
        Self { document }
    }

    pub(crate) const fn info(&self) -> GenesisInfo {
        GenesisInfo {
            state_id: StateId::GENESIS,
            document_id: self.document.id.get(),
            cell_id: self.document.cell_id.get(),
            base_surface: self.document.base_surface,
        }
    }
}
