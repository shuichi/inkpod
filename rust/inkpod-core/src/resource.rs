use super::*;

fn document_raster_bytes(document: &CellDocument) -> u64 {
    document.logical_raster_usage().1
}

fn history_document_raster_bytes(document: &CellDocument) -> u64 {
    document_raster_bytes(document).saturating_add(document.light_table.logical_raster_usage().1)
}

fn history_change_bytes(change: &HistoryChange) -> u64 {
    match change {
        HistoryChange::Pixels { changes, .. } => {
            (changes.len() as u64).saturating_mul(std::mem::size_of::<PixelChange>() as u64)
        }
        HistoryChange::Palette { before, after } => ((before.colors().len() + after.colors().len())
            as u64)
            .saturating_mul(std::mem::size_of::<PixelValue>() as u64),
        HistoryChange::ColorChart { before, after } => before
            .entries()
            .iter()
            .chain(after.entries())
            .fold(0_u64, |bytes, entry| {
                bytes
                    .saturating_add(std::mem::size_of::<ColorChartEntry>() as u64)
                    .saturating_add(entry.name.len() as u64)
            }),
        HistoryChange::MainLineColor { .. } => (2 * std::mem::size_of::<PixelValue>()) as u64,
        HistoryChange::Document { before, after } => history_document_raster_bytes(before)
            .saturating_add(history_document_raster_bytes(after)),
    }
}

impl Core {
    /// Returns deterministic category-by-category logical resource usage.
    ///
    /// The query is read-only and does not build a snapshot, advance revisions,
    /// or change history/savepoint state.
    #[must_use]
    pub fn resource_usage(&self) -> ResourceUsage {
        let (document_tile_count, document_tile_bytes) = self
            .document
            .as_ref()
            .map_or((0_u64, 0_u64), CellDocument::logical_raster_usage);
        let (reference_light_table_tile_count, reference_light_table_bytes) =
            self.document.as_ref().map_or((0_u64, 0_u64), |document| {
                document.light_table.logical_raster_usage()
            });
        let (sequence_source_tile_count, sequence_source_bytes) = self.sequence.as_ref().map_or(
            (0_u64, 0_u64),
            animation::SequenceState::logical_raster_usage,
        );
        let history_bytes = self.history.iter().fold(0_u64, |bytes, entry| {
            bytes.saturating_add(entry.change.as_ref().map_or(0, history_change_bytes))
        });
        let render_cache_bytes = self.render_cache.values().fold(0_u64, |bytes, tile| {
            bytes.saturating_add(tile.pixels().len() as u64)
        });

        let mut cpu_staging_bytes = 0_u64;
        if let Some(stroke) = &self.active_stroke {
            cpu_staging_bytes = cpu_staging_bytes
                .saturating_add(document_raster_bytes(&stroke.preview_document))
                .saturating_add(stroke.canonical_payload_bytes());
        }
        if let Some(preview) = &self.filter_preview {
            cpu_staging_bytes = cpu_staging_bytes
                .saturating_add(document_raster_bytes(&preview.base_document))
                .saturating_add(document_raster_bytes(&preview.preview_document));
        }
        if let Some(floating) = &self.floating {
            for plane in &floating.payload.planes {
                cpu_staging_bytes = cpu_staging_bytes.saturating_add(
                    (plane.pixels.len() as u64)
                        .saturating_mul(std::mem::size_of::<ClipboardPixel>() as u64),
                );
            }
        }

        ResourceUsage {
            document_tile_bytes,
            document_tile_count,
            history_bytes,
            history_entry_count: self.history.len() as u64,
            render_cache_bytes,
            render_cache_tile_count: self.render_cache.len() as u64,
            cpu_staging_bytes,
            reference_light_table_bytes,
            reference_light_table_tile_count,
            sequence_source_bytes,
            sequence_source_tile_count,
            thumbnail_cache_bytes: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_usage_is_read_only_and_tracks_document_history_and_cache() {
        let mut core = Core::new();
        core.new_cell(128, 64, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let before = core.document_info().unwrap();
        let blank = core.resource_usage();
        assert_eq!(blank.document_tile_bytes, 0);
        assert_eq!(blank.history_entry_count, 0);

        core.apply_stroke(&Stroke {
            tool: PaintTool::Pencil,
            plane: ActivePlane::MainLine,
            color: [0, 0, 0, 255],
            diameter: 1.0,
            shape: BrushShape::Round,
            smoothing: 0,
            start_color: StartColorPredicate::Any,
            auto_erase: false,
            pressure_size: false,
            coordinate_space: CoordinateSpace::Document,
            samples: vec![StrokeSample {
                x: 2.0,
                y: 3.0,
                pressure: 1.0,
            }],
        })
        .unwrap();
        let _snapshot = core.build_snapshot();
        let usage = core.resource_usage();
        let after = core.document_info().unwrap();
        assert!(usage.document_tile_bytes > 0);
        assert_eq!(usage.document_tile_count, 1);
        assert_eq!(usage.history_entry_count, 1);
        assert!(usage.history_bytes > 0);
        assert!(usage.render_cache_bytes > 0);
        assert_eq!(usage.thumbnail_cache_bytes, 0);
        assert_eq!(after.document_revision, before.document_revision + 1);
        assert_eq!(core.document_info().unwrap(), after);
    }
}
