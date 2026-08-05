//! Asset-retention graph and atomic store publication.

use super::*;

impl Core {
    pub(super) fn prepare_asset_store_for_session_reset(
        &self,
        mut staged: asset::AssetStore,
        document: &CellDocument,
    ) -> Result<asset::AssetStore, CoreError> {
        let mut roots = Vec::new();
        // A reset installs the same immutable state as both Genesis and the
        // current materialized document.
        append_document_asset_roots(document, &mut roots);
        append_document_asset_roots(document, &mut roots);
        staged.garbage_collect(roots)?;
        Ok(staged)
    }

    pub(super) fn prepare_asset_store_for_document_edit(
        &self,
        mut staged: asset::AssetStore,
        working: &CellDocument,
    ) -> Result<asset::AssetStore, CoreError> {
        let mut roots = self.asset_retention_roots();
        // A legacy DocumentEdit publishes `working` as both the current state
        // and the `after` history snapshot. The pre-edit current state already
        // accounts for the matching `before` snapshot.
        append_document_asset_roots(working, &mut roots);
        append_document_asset_roots(working, &mut roots);
        staged.garbage_collect(roots)?;
        Ok(staged)
    }

    pub(super) fn prepare_asset_store_for_commit(
        &self,
        staged: Option<asset::AssetStore>,
        working: &CellDocument,
        procedure: &CanonicalProcedure,
    ) -> Result<Option<asset::AssetStore>, CoreError> {
        let Some(mut staged) = staged else {
            return Ok(None);
        };
        let mut roots = self.asset_retention_roots();
        append_document_asset_roots(working, &mut roots);
        // A successful canonical commit stores the same procedure reference in
        // both the append-only journal and the visible history entry.
        roots.extend(procedure.asset_ids().iter().copied());
        roots.extend(procedure.asset_ids().iter().copied());
        staged.garbage_collect(roots)?;
        Ok(Some(staged))
    }

    /// Recomputes the complete semantic asset-retention graph and releases
    /// unreferenced immutable payloads.
    ///
    /// Roots include Genesis, the current materialized document, every retained
    /// journal branch and redo tail, runtime history snapshots for routes not yet
    /// migrated to canonical procedures, and active floating-paste assets. The
    /// scan is staged, so a missing/corrupt root leaves the live store intact.
    pub fn collect_unreferenced_assets(&mut self) -> Result<u64, CoreError> {
        let roots = self.asset_retention_roots();
        let mut staged = self.assets.clone();
        let released = staged.garbage_collect(roots)?;
        self.assets = staged;
        Ok(released)
    }

    pub(super) fn asset_retention_roots(&self) -> Vec<AssetId> {
        self.asset_retention_roots_with_floating(true)
    }

    pub(super) fn asset_store_without_floating(&self) -> Result<asset::AssetStore, CoreError> {
        let mut staged = self.assets.clone();
        staged.garbage_collect(self.asset_retention_roots_with_floating(false))?;
        Ok(staged)
    }

    fn asset_retention_roots_with_floating(&self, include_floating: bool) -> Vec<AssetId> {
        let mut roots = Vec::new();
        if let Some(genesis) = &self.genesis {
            append_document_asset_roots(&genesis.document, &mut roots);
        }
        if let Some(document) = &self.document {
            append_document_asset_roots(document, &mut roots);
        }
        for record in &self.journal {
            if let JournalEntry::Commit(commit) = record {
                roots.extend(commit.procedure().asset_ids().iter().copied());
            }
        }
        for entry in &self.history {
            if let Some(procedure) = &entry.procedure {
                roots.extend(procedure.asset_ids().iter().copied());
            }
            if let Some(HistoryChange::Document { before, after }) = &entry.change {
                append_document_asset_roots(before, &mut roots);
                append_document_asset_roots(after, &mut roots);
            }
        }
        if include_floating {
            if let Some(floating) = &self.floating {
                roots.extend(floating.asset_ids.iter().copied());
            }
        }
        roots
    }
}

fn append_document_asset_roots(document: &CellDocument, roots: &mut Vec<AssetId>) {
    if let BaseSurface::Asset(id) = document.base_surface {
        roots.push(id);
    }
    roots.extend(document.light_table.asset_ids());
}
