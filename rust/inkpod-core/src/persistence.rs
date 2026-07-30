//! Save, open, recovery, and revert operations.

use super::*;

impl Core {
    /// Atomically writes the active document to a normal-save path.
    ///
    /// Success records the current history state as savepoint, clears recovered
    /// status, and makes dirty false without changing document revision/history.
    /// Write failure leaves the previous file and Core savepoint/path unchanged.
    pub fn save(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        inkpod_format::save_atomic(path, &document.to_file())?;
        self.savepoint = Some(self.current_state);
        self.current_path = Some(path.to_path_buf());
        self.recovered = false;
        self.document_info()
    }

    /// Atomically writes recovery data without advancing the normal-save savepoint.
    ///
    /// Document revision, history, dirty state, current normal path, and recovered
    /// status are unchanged.
    pub fn autosave(&self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        inkpod_format::save_recovery_atomic(path, &document.to_file())?;
        self.document_info()
    }

    /// Reads and fully validates a native document before replacing Core state.
    ///
    /// Success establishes a clean savepoint, records `path`, advances revision,
    /// and resets history/view/transient state. Read or validation failure retains
    /// the previously open document and its savepoint.
    pub fn open(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let file = inkpod_format::read(path)?;
        let revision = self.next_document_revision()?;
        let document = CellDocument::from_file(file, revision)?;
        let max_id = document.max_stable_id();
        self.next_id.advance_past_raw(max_id);
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

    /// Opens validated recovery data as a dirty recovered document.
    ///
    /// No normal-save path/savepoint is adopted. Failure leaves current Core state
    /// unchanged; success resets history/view/transient state.
    pub fn open_recovery(&mut self, path: &Path) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let file = inkpod_format::read(path)?;
        let revision = self.next_document_revision()?;
        let document = CellDocument::from_file(file, revision)?;
        let max_id = document.max_stable_id();
        self.next_id.advance_past_raw(max_id);
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

    /// Compares recovery and normal-save timestamps using format-layer policy.
    ///
    /// This query does not mutate Core or either file.
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

    /// Removes a recovery artifact using the format layer's bounded path policy.
    ///
    /// This external file operation does not alter Core document/savepoint state.
    pub fn discard_recovery(&self, path: &Path) -> Result<(), CoreError> {
        inkpod_format::discard_recovery(path)?;
        Ok(())
    }

    /// Reopens the last successful normal-save path, discarding live edits.
    ///
    /// The operation uses [`Core::open`] atomic replacement semantics and is an
    /// error when no normal-save path is known.
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
