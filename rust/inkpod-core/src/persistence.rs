//! Save, open, recovery, and revert operations.

use super::*;

impl Core {
    pub fn save(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        inkpod_format::save_atomic(path, &document.to_file())?;
        self.savepoint = Some(self.current_state);
        self.current_path = Some(path.to_path_buf());
        self.recovered = false;
        self.document_info()
    }

    pub fn autosave(&self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        inkpod_format::save_recovery_atomic(path, &document.to_file())?;
        self.document_info()
    }

    pub fn open(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let file = inkpod_format::read(path)?;
        let revision = self.next_document_revision()?;
        let document = CellDocument::from_file(file, revision)?;
        let max_id = document.max_stable_id();
        self.next_id = self.next_id.max(max_id.saturating_add(1));
        self.document = Some(document);
        self.filter_preview = None;
        self.last_filter = None;
        self.render_cache.clear();
        self.document_revision = revision;
        self.reset_history(true);
        self.reset_view();
        self.current_path = Some(path.to_path_buf());
        self.recovered = false;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.sequence = None;
        self.motion_check = None;
        self.subpalette_index = None;
        self.document_info()
    }

    pub fn open_recovery(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let file = inkpod_format::read(path)?;
        let revision = self.next_document_revision()?;
        let document = CellDocument::from_file(file, revision)?;
        let max_id = document.max_stable_id();
        self.next_id = self.next_id.max(max_id.saturating_add(1));
        self.document = Some(document);
        self.filter_preview = None;
        self.last_filter = None;
        self.render_cache.clear();
        self.document_revision = revision;
        // Recovery content is deliberately an unsaved document. A subsequent
        // explicit save needs a caller-selected destination.
        self.reset_history(false);
        self.reset_view();
        self.current_path = None;
        self.recovered = true;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.sequence = None;
        self.motion_check = None;
        self.subpalette_index = None;
        self.document_info()
    }

    pub fn recovery_is_newer(
        &self,
        normal_path: &Path,
        recovery_path: &Path,
    ) -> Result<bool, CoreError> {
        Ok(inkpod_format::recovery_is_newer(
            normal_path,
            recovery_path,
        )?)
    }

    pub fn discard_recovery(&self, path: &Path) -> Result<(), CoreError> {
        inkpod_format::discard_recovery(path)?;
        Ok(())
    }

    pub fn revert(&mut self) -> Result<DocumentInfo, CoreError> {
        let path = self
            .current_path
            .clone()
            .ok_or(CoreError::InvalidState("document has no normal-save path"))?;
        self.open(&path)
    }
}

// Shared implementation helpers for this responsibility.

pub(super) fn raster_to_file_plane(id: u64, kind: FilePlaneKind, raster: &TileRaster) -> FilePlane {
    let tiles = raster
        .allocated_coords()
        .filter_map(|coord| raster.tile_data(coord))
        .map(|tile| FileTile {
            coord: tile.coord,
            width: tile.width,
            height: tile.height,
            bytes: tile.bytes,
        })
        .collect();
    FilePlane {
        id,
        kind,
        pixel_format: raster.format(),
        width: raster.width(),
        height: raster.height(),
        tiles,
    }
}

pub(super) fn file_plane_to_raster(
    plane: &FilePlane,
    revision: u64,
) -> Result<TileRaster, CoreError> {
    let mut raster = TileRaster::new(plane.width, plane.height, plane.pixel_format)?;
    for tile in &plane.tiles {
        raster.insert_tile(TileData {
            coord: tile.coord,
            width: tile.width,
            height: tile.height,
            bytes: tile.bytes.clone(),
            revision,
        })?;
    }
    Ok(raster)
}
