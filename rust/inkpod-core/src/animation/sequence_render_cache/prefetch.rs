//! Pure immutable preparation on the application's existing bounded worker pool.

use super::*;
use crate::snapshot::{compose_tile, revision_max_tile_source_revision};
use inkpod_io::{IoError, IoJob, IoResult, JobContext, JobState};

pub(super) struct PendingSequenceRender {
    identity: SequenceRenderSourceIdentity,
    source_index: usize,
    job: IoJob<PreparedSequenceRender>,
}

impl PendingSequenceRender {
    pub(super) fn cancel(&self) {
        self.job.cancel();
    }

    pub(super) const fn identity(&self) -> SequenceRenderSourceIdentity {
        self.identity
    }
}

impl std::fmt::Debug for PendingSequenceRender {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingSequenceRender")
            .field("identity", &self.identity)
            .field("state", &self.job.poll().state)
            .finish()
    }
}

struct PreparedSequenceRender {
    tiles: BTreeMap<(u64, TileCoord), RenderTile>,
    reservation: SequenceRenderReservation,
}

fn compose_source_tiles(
    source: &SequenceCellSource,
    mut check_cancelled: impl FnMut() -> IoResult<()>,
) -> IoResult<BTreeMap<(u64, TileCoord), RenderTile>> {
    // These temporary topology IDs never escape in the prepared tile payload,
    // consume the live document cursor, or become a persistent document.
    let mut ids = StableIdCursor::first();
    let document =
        Core::document_from_sequence_source(source, DocumentRevision::from_raw(1), &mut ids)
            .map_err(|_| IoError::InvalidInput("sequence render preparation failed"))?;
    let mut tiles = BTreeMap::new();
    for coord in source.raster.allocated_coords() {
        check_cancelled()?;
        if let Some(tile) = compose_tile(
            &document,
            None,
            coord,
            None,
            false,
            revision_max_tile_source_revision(&document, coord),
            RenderRevision::from_raw(0),
        ) {
            tiles.insert((0, coord), tile);
        }
    }
    check_cancelled()?;
    Ok(tiles)
}

fn prepare_source(
    source: SequenceCellSource,
    reservation: SequenceRenderReservation,
    context: JobContext,
) -> IoResult<PreparedSequenceRender> {
    let tiles = compose_source_tiles(&source, || context.check_cancelled())?;
    Ok(PreparedSequenceRender { tiles, reservation })
}

impl Core {
    pub(crate) fn prepare_sequence_render_catalog(&mut self) {
        let Some(owner) = self.sequence_render_cache.owner else {
            return;
        };
        let Some(sequence) = self.sequence.as_ref() else {
            return;
        };
        // Sequence import itself is already asynchronous. Complete the bounded
        // CPU compositions before publishing its completion, so the first user
        // navigation never cancels an unfinished prewarm job and recomposes the
        // selected cell synchronously on the Core owner thread.
        let sources = sequence
            .cells
            .iter()
            .take(MAX_RETAINED_SOURCES as usize)
            .cloned()
            .collect::<Vec<_>>();
        for source in sources {
            let identity = SequenceRenderSourceIdentity {
                document_uuid: source.document_uuid,
                source_generation: source.source_generation,
                owner_generation: owner.0,
            };
            if self
                .sequence_render_cache
                .entries
                .iter()
                .any(|entry| entry.identity == identity)
            {
                continue;
            }
            let Some(reservation) = self.sequence_render_cache.reserve(&source, false) else {
                continue;
            };
            let Ok(mut tiles) = compose_source_tiles(&source, || Ok(())) else {
                continue;
            };
            for tile in tiles.values_mut() {
                tile.assign_sequence_tile_revision(self.next_render_tile_revision);
                self.next_render_tile_revision =
                    self.next_render_tile_revision.wrapping_next_nonzero();
            }
            self.sequence_render_cache
                .finish(Some(identity), Some(reservation), &mut tiles);
        }
    }

    pub(crate) fn poll_sequence_render_preparations(&mut self) {
        let pending = std::mem::take(&mut self.sequence_render_cache.pending);
        for candidate in pending {
            let current_index = self
                .sequence
                .as_ref()
                .and_then(|sequence| sequence.active_index);
            let accepted_index = self.sequence.as_ref().and_then(|sequence| {
                (self.sequence_render_cache.owner_generation()
                    == candidate.identity.owner_generation)
                    .then(|| {
                        sequence
                            .cells
                            .get(candidate.source_index)
                            .filter(|source| {
                                source.document_uuid == candidate.identity.document_uuid
                                    && source.source_generation
                                        == candidate.identity.source_generation
                            })
                            .map(|_| candidate.source_index)
                    })
                    .flatten()
            });
            if accepted_index.is_none() {
                candidate.job.cancel();
                continue;
            }
            // The worker publishes its result before the terminal state. Read
            // that state first: completion between these two reads is either
            // taken now or retained as pending for the next nonblocking poll.
            let state = candidate.job.poll().state;
            match candidate.job.try_take() {
                Some(Ok(mut prepared)) => {
                    if self
                        .sequence_render_cache
                        .entries
                        .iter()
                        .any(|entry| entry.identity == candidate.identity)
                    {
                        continue;
                    }
                    for tile in prepared.tiles.values_mut() {
                        tile.assign_sequence_tile_revision(self.next_render_tile_revision);
                        self.next_render_tile_revision =
                            self.next_render_tile_revision.wrapping_next_nonzero();
                    }
                    self.sequence_render_cache.finish(
                        Some(candidate.identity),
                        Some(prepared.reservation),
                        &mut prepared.tiles,
                    );
                }
                Some(Err(_)) => {}
                None => {
                    if accepted_index == current_index {
                        // Foreground navigation never waits for a worker. Cancel
                        // an unfinished target and use ordinary composition now.
                        candidate.job.cancel();
                    } else if matches!(state, JobState::Queued | JobState::Running) {
                        self.sequence_render_cache.pending.push(candidate);
                    }
                }
            }
        }
    }

    pub(crate) fn schedule_sequence_render_neighbors(&mut self) {
        let Some(manager) = self.io_manager.as_ref() else {
            return;
        };
        let Some(owner) = self.sequence_render_cache.owner else {
            return;
        };
        let Some(sequence) = self.sequence.as_ref() else {
            return;
        };
        let Some(active) = sequence.active_index else {
            return;
        };
        if self.view.alpha_view
            || self.color_check.is_some()
            || self.active_stroke.is_some()
            || self.filter_preview.is_some()
            || self.shooting_frame_preview.is_some()
            || self.floating.is_some()
        {
            return;
        }
        if self.sequence_render_cache.prefetch_anchor == Some((owner.0, active)) {
            return;
        }
        // At most one full-catalog speculative pass per activation. Near cells
        // are submitted first, but every source that fits the 64-source/1-GiB
        // budget is prepared. A fully transparent result is still not
        // negative-cached; ordinary redraws do not continuously resubmit it.
        self.sequence_render_cache.prefetch_anchor = Some((owner.0, active));
        let mut indices = (0..sequence.cells.len())
            .filter(|index| *index != active)
            .collect::<Vec<_>>();
        indices.sort_unstable_by_key(|index| index.abs_diff(active));
        for index in indices {
            if self.sequence_render_cache.pending.len()
                >= MAX_RETAINED_SOURCES.saturating_sub(1) as usize
            {
                break;
            }
            let Some(source) = sequence.cells.get(index) else {
                continue;
            };
            let identity = SequenceRenderSourceIdentity {
                document_uuid: source.document_uuid,
                source_generation: source.source_generation,
                owner_generation: owner.0,
            };
            if self
                .sequence_render_cache
                .entries
                .iter()
                .any(|entry| entry.identity == identity)
                || self
                    .sequence_render_cache
                    .pending
                    .iter()
                    .any(|job| job.identity == identity)
            {
                continue;
            }
            // Speculative work only uses spare budget; it must not evict a
            // recently viewed source merely to prepare an adjacent one.
            let Some(reservation) = self.sequence_render_cache.reserve(source, false) else {
                continue;
            };
            let source = source.clone();
            if let Ok(job) =
                manager.submit(move |context| prepare_source(source, reservation, context))
            {
                self.sequence_render_cache
                    .pending
                    .push(Arc::new(PendingSequenceRender {
                        identity,
                        source_index: index,
                        job,
                    }));
            }
        }
    }
}
