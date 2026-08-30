//! Immutable document Genesis and typed base-surface metadata.

use crate::{AssetId, CellDocument, CoreError, PlaneId, PlaneType, StateId, TileRaster, asset};

/// The immutable surface below every editable layer in a document.
///
/// A solid-white base contributes only to a flattened composite. An asset base
/// names canonical immutable pixels owned by the document's Core asset store.
/// None of these variants is an editable layer, plane, or selection mask.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BaseSurface {
    /// An allocation-free opaque sRGB white underlay.
    SolidWhite,
    /// A canonical raster asset whose dimensions equal the document paper.
    Asset(AssetId),
    /// An allocation-free transparent underlay for imported editable images.
    Transparent,
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
    pub(crate) raster_source: Option<GenesisRasterSource>,
}

/// Immutable source of the initial editable main-line plane, not a display layer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GenesisRasterSource {
    pub(crate) plane_id: PlaneId,
    pub(crate) asset_id: AssetId,
}

impl Genesis {
    pub(crate) const fn new(document: CellDocument) -> Self {
        Self {
            document,
            raster_source: None,
        }
    }

    pub(crate) fn archive_document(&self) -> Result<CellDocument, CoreError> {
        let mut document = self.document.clone();
        if let Some(source) = self.raster_source {
            let plane = document
                .plane_by_id_mut(source.plane_id)
                .ok_or(CoreError::InvalidState("Genesis source plane is missing"))?;
            // The immutable asset is the serialized authority. Never duplicate
            // its payload in GENS or serialize the current edited plane here.
            plane.raster = TileRaster::new(
                plane.raster.width(),
                plane.raster.height(),
                plane.raster.format(),
            )?;
        }
        Ok(document)
    }

    pub(crate) fn materialize_raster_source(
        &mut self,
        assets: &asset::AssetStore,
    ) -> Result<(), CoreError> {
        let Some(source) = self.raster_source else {
            return Ok(());
        };
        let record = assets
            .get(source.asset_id)
            .ok_or(CoreError::InvalidState("Genesis source asset is missing"))?;
        let raster = record.raster().ok_or(CoreError::InvalidState(
            "Genesis source asset is not a raster",
        ))?;
        if raster.width() != self.document.width || raster.height() != self.document.height {
            return Err(CoreError::InvalidState(
                "Genesis source dimensions differ from the document",
            ));
        }
        let plane = self
            .document
            .plane_by_id_mut(source.plane_id)
            .ok_or(CoreError::InvalidState("Genesis source plane is missing"))?;
        if plane.kind != PlaneType::MainLine || plane.raster.format() != raster.format() {
            return Err(CoreError::InvalidState(
                "Genesis source plane type or format differs",
            ));
        }
        plane.raster = raster.as_ref().clone();
        Ok(())
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
