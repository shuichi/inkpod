//! Immutable document Genesis and typed base-surface metadata.

use crate::{
    AssetId, CellDocument, CoreError, PixelFormat, PlaneId, PlaneType, StateId, TILE_SIZE,
    TileRaster, asset,
};

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
    /// A transparent underlay for imports containing any non-opaque alpha.
    Transparent,
}

pub(crate) fn imported_main_line_base_surface(
    raster: &TileRaster,
) -> Result<BaseSurface, CoreError> {
    if !matches!(
        raster.format(),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(CoreError::InvalidArgument(
            "editable raster import requires straight RGBA",
        ));
    }

    let expected_tiles = u64::from(raster.width().div_ceil(TILE_SIZE))
        .checked_mul(u64::from(raster.height().div_ceil(TILE_SIZE)))
        .ok_or(CoreError::InvalidState(
            "imported raster tile count overflows",
        ))?;
    let allocated_tiles = u64::try_from(raster.allocated_tile_count())
        .map_err(|_| CoreError::InvalidState("imported raster tile count is not representable"))?;
    if allocated_tiles != expected_tiles {
        return Ok(BaseSurface::Transparent);
    }

    for coord in raster.allocated_coords() {
        let view = raster.tile_view(coord).ok_or(CoreError::InvalidState(
            "imported raster allocated tile is missing",
        ))?;
        let bytes_per_pixel = raster.format().bytes_per_pixel();
        let row_bytes = view.width() as usize * bytes_per_pixel;
        let row_stride = view.row_stride_bytes() as usize;
        for row in 0..view.height() as usize {
            let start = row * row_stride;
            let bytes = &view.bytes()[start..start + row_bytes];
            let has_nonopaque_alpha = match raster.format() {
                PixelFormat::StraightRgba8 => {
                    bytes.chunks_exact(4).any(|pixel| pixel[3] != u8::MAX)
                }
                PixelFormat::StraightRgba16 => bytes
                    .chunks_exact(8)
                    .any(|pixel| pixel[6] != u8::MAX || pixel[7] != u8::MAX),
                _ => unreachable!("format was validated before scanning alpha"),
            };
            if has_nonopaque_alpha {
                return Ok(BaseSurface::Transparent);
            }
        }
    }
    Ok(BaseSurface::SolidWhite)
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
            let source_plane = document
                .plane_by_id(source.plane_id)
                .ok_or(CoreError::InvalidState("Genesis source plane is missing"))?;
            if source_plane.kind != PlaneType::MainLine
                || !matches!(
                    source_plane.raster.format(),
                    PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
                )
            {
                return Err(CoreError::InvalidState(
                    "Genesis source plane type or format differs",
                ));
            }
            if document.base_surface != imported_main_line_base_surface(&source_plane.raster)? {
                return Err(CoreError::InvalidState(
                    "Genesis source underlay does not match exact raster alpha",
                ));
            }
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
            .plane_by_id(source.plane_id)
            .ok_or(CoreError::InvalidState("Genesis source plane is missing"))?;
        if plane.kind != PlaneType::MainLine || plane.raster.format() != raster.format() {
            return Err(CoreError::InvalidState(
                "Genesis source plane type or format differs",
            ));
        }
        if self.document.base_surface != imported_main_line_base_surface(raster)? {
            return Err(CoreError::InvalidState(
                "Genesis source underlay does not match exact raster alpha",
            ));
        }
        let plane = self
            .document
            .plane_by_id_mut(source.plane_id)
            .ok_or(CoreError::InvalidState("Genesis source plane is missing"))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommonRaster, CommonRasterFormat, Core};

    #[test]
    fn raster_source_rejects_underlay_that_disagrees_with_exact_alpha() {
        for (alpha, wrong_surface) in [
            (u8::MAX, BaseSurface::Transparent),
            (u8::MAX - 1, BaseSurface::SolidWhite),
        ] {
            let source = CommonRaster::new(
                1,
                1,
                PixelFormat::StraightRgba8,
                None,
                None,
                vec![1, 2, 3, alpha],
            )
            .unwrap();
            let mut core = Core::new();
            core.import_decoded_common_raster(CommonRasterFormat::Tga, &source, 0x4745_4e53)
                .unwrap();
            let mut genesis = core.genesis.clone().expect("imported Genesis");
            genesis.document.base_surface = wrong_surface;

            assert_eq!(
                genesis.archive_document().unwrap_err(),
                CoreError::InvalidState(
                    "Genesis source underlay does not match exact raster alpha"
                )
            );
            assert_eq!(
                genesis.materialize_raster_source(&core.assets).unwrap_err(),
                CoreError::InvalidState(
                    "Genesis source underlay does not match exact raster alpha"
                )
            );
        }
    }
}
