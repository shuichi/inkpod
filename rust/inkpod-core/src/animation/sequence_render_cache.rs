//! Bounded ownership of immutable, normally displayed sequence compositions.
//!
//! This catalog cache selects already composed source tiles. The existing
//! per-tile revision-max validation remains the only composition cache check.

use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

mod prefetch;

use prefetch::PendingSequenceRender;

const MAX_RETAINED_SOURCES: u64 = inkpod_io::MAX_SEQUENCE_RENDER_ALLOCATIONS;
const MAX_RETAINED_BYTES: u64 = inkpod_io::MAX_SEQUENCE_RENDER_BYTES;

/// Runtime identity of a pristine immutable sequence source composition.
///
/// All fields are nonzero. The UUID and source generation identify the imported
/// pixels; the owner generation distinguishes independent Core owners and
/// catalog replacements, including replacement with the same source identifiers.
/// This identity is process-local derived state, never a persistent ID, render
/// revision, semantic digest input, or proof that an edited document is pristine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequenceRenderSourceIdentity {
    /// Persistent UUID of the immutable source document.
    pub document_uuid: u128,
    /// Nonzero generation of the immutable source pixels.
    pub source_generation: u64,
    /// Nonzero process-local cache-owner generation; never reused or wrapped.
    pub owner_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SequenceRenderOwnerGeneration(u64);

impl SequenceRenderOwnerGeneration {
    fn allocate() -> Option<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |value| {
            value.checked_add(1)
        })
        .ok()
        .map(Self)
    }
}

#[derive(Clone, Copy, Debug)]
struct PristineSequenceSource {
    identity: SequenceRenderSourceIdentity,
    document_id: DocumentId,
    cell_id: CellId,
    document_revision: DocumentRevision,
    state: StateId,
}

#[derive(Clone, Copy, Debug, Default)]
struct SequenceRenderUsage {
    bytes: u64,
    sources: u64,
    tiles: u64,
}

/// A charge shared by every tile clone, including snapshots outliving Core.
/// The reservation covers the clipped output-tile upper bound, so transparent
/// tiles cannot cause an unreserved allocation during composition.
#[derive(Debug)]
pub(crate) struct SequenceRenderReservation {
    ledger: Arc<Mutex<SequenceRenderUsage>>,
    bytes: u64,
    tiles: u64,
    _decoded_lease: Option<inkpod_io::DecodedLease>,
}

impl Drop for SequenceRenderReservation {
    fn drop(&mut self) {
        let mut usage = self
            .ledger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        usage.bytes -= self.bytes;
        usage.sources -= 1;
        usage.tiles -= self.tiles;
    }
}

#[derive(Clone, Debug)]
struct CachedSequenceRender {
    identity: SequenceRenderSourceIdentity,
    tiles: BTreeMap<(u64, TileCoord), RenderTile>,
}

#[derive(Clone, Debug)]
pub(crate) struct SequenceRenderCache {
    owner: Option<SequenceRenderOwnerGeneration>,
    pristine: Option<PristineSequenceSource>,
    active_source: Option<SequenceRenderSourceIdentity>,
    admission_attempted: bool,
    entries: VecDeque<CachedSequenceRender>,
    pending: Vec<Arc<PendingSequenceRender>>,
    prefetch_anchor: Option<(u64, usize)>,
    ledger: Arc<Mutex<SequenceRenderUsage>>,
}

impl SequenceRenderCache {
    pub(crate) fn new() -> Self {
        Self {
            owner: SequenceRenderOwnerGeneration::allocate(),
            pristine: None,
            active_source: None,
            admission_attempted: false,
            entries: VecDeque::new(),
            pending: Vec::new(),
            prefetch_anchor: None,
            ledger: Arc::new(Mutex::new(SequenceRenderUsage::default())),
        }
    }

    pub(crate) fn owner_generation(&self) -> u64 {
        self.owner.map_or(0, |owner| owner.0)
    }

    pub(crate) fn fork_owner(&mut self) {
        self.pending.clear();
        self.prefetch_anchor = None;
        self.owner = SequenceRenderOwnerGeneration::allocate();
        let Some(owner) = self.owner else {
            self.clear_retained();
            return;
        };
        if let Some(pristine) = &mut self.pristine {
            pristine.identity.owner_generation = owner.0;
        }
        if let Some(identity) = &mut self.active_source {
            identity.owner_generation = owner.0;
        }
        for entry in &mut self.entries {
            entry.identity.owner_generation = owner.0;
        }
        // COW clones share the same physical payload and its reservation. A
        // distinct identity must not duplicate the charge or detach its lifetime.
    }

    pub(crate) fn invalidate_document(&mut self) {
        self.pristine = None;
        self.active_source = None;
        self.admission_attempted = false;
        self.prefetch_anchor = None;
    }

    pub(crate) fn clear_retained(&mut self) {
        self.invalidate_document();
        self.entries.clear();
        self.pending.clear();
    }

    fn catalog_changed(&mut self) {
        for pending in &self.pending {
            pending.cancel();
        }
        self.clear_retained();
        self.owner = SequenceRenderOwnerGeneration::allocate();
        // Keep the ledger: old snapshots still own charges until their last
        // RenderTile is released, even after the entire catalog is replaced.
    }

    pub(crate) fn resource_usage(&self) -> (u64, u64, u64) {
        let usage = self
            .ledger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (usage.bytes, usage.sources, usage.tiles)
    }

    fn evict_unreferenced(&mut self) -> bool {
        let Some(index) = self.entries.iter().rposition(|entry| {
            entry
                .tiles
                .values()
                .all(RenderTile::sequence_payload_is_exclusive)
        }) else {
            return false;
        };
        self.entries.remove(index);
        true
    }

    fn reserve(
        &mut self,
        source: &SequenceCellSource,
        allow_eviction: bool,
    ) -> Option<SequenceRenderReservation> {
        let bytes = source
            .raster
            .allocated_coords()
            .try_fold(0_u64, |bytes, coord| {
                let x = coord.x.checked_mul(TILE_SIZE)?;
                let y = coord.y.checked_mul(TILE_SIZE)?;
                let width = source.raster.width().checked_sub(x)?.min(TILE_SIZE);
                let height = source.raster.height().checked_sub(y)?.min(TILE_SIZE);
                bytes.checked_add(u64::from(width) * u64::from(height) * 4)
            })?;
        if bytes == 0 || bytes > MAX_RETAINED_BYTES {
            return None;
        }
        loop {
            let mut usage = self
                .ledger
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if usage.sources < MAX_RETAINED_SOURCES && usage.bytes <= MAX_RETAINED_BYTES - bytes {
                usage.bytes += bytes;
                usage.sources += 1;
                break;
            }
            drop(usage);
            if !allow_eviction || !self.evict_unreferenced() {
                return None;
            }
        }
        let mut reservation = SequenceRenderReservation {
            ledger: self.ledger.clone(),
            bytes,
            tiles: 0,
            _decoded_lease: None,
        };
        // The application-owned I/O lease enforces the same 8-source/128-MiB
        // bound across every managed Core and the existing decoded-byte budget.
        loop {
            match source.reserve_render_payload(bytes) {
                Ok(lease) => {
                    reservation._decoded_lease = lease;
                    break;
                }
                Err(CoreError::InvalidState(_)) if allow_eviction && self.evict_unreferenced() => {}
                Err(_) => return None,
            }
        }
        Some(reservation)
    }

    fn prepare(
        &mut self,
        identity: Option<SequenceRenderSourceIdentity>,
        source: Option<&SequenceCellSource>,
        display_mode_override: bool,
        cache: &mut BTreeMap<(u64, TileCoord), RenderTile>,
    ) -> Option<SequenceRenderReservation> {
        let changed = identity != self.active_source;
        if changed {
            // An edit or stroke preview only revokes pristine-bank admission.
            // Keep current tiles for canonical revision-max validation and let
            // the existing metadata/preview commit paths invalidate as needed.
            // Display modes are not part of revision-max; they still need the
            // barrier, including alpha snapshots from another registered view.
            if identity.is_some() || display_mode_override {
                cache.clear();
            }
            self.active_source = identity;
            self.admission_attempted = false;
        }
        let identity = identity?;
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.identity == identity)
        {
            let entry = self.entries.remove(index)?;
            if changed || cache.keys().next().is_none_or(|(band, _)| *band != 0) {
                *cache = entry.tiles.clone();
            }
            self.entries.push_front(entry);
            self.admission_attempted = true;
            return None;
        }
        if self.admission_attempted {
            return None;
        }
        self.admission_attempted = true;
        // A failed admission stays an ordinary cache for this activation. Never
        // attach a new charge to payloads already exported in earlier snapshots.
        cache.clear();
        self.reserve(source?, true)
    }

    pub(crate) fn finish(
        &mut self,
        identity: Option<SequenceRenderSourceIdentity>,
        reservation: Option<SequenceRenderReservation>,
        cache: &mut BTreeMap<(u64, TileCoord), RenderTile>,
    ) {
        let (Some(identity), Some(mut reservation)) = (identity, reservation) else {
            return;
        };
        if cache.is_empty() {
            return;
        }
        reservation.tiles = cache.len() as u64;
        {
            let mut usage = reservation
                .ledger
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            usage.tiles += reservation.tiles;
        }
        let reservation = Arc::new(reservation);
        for tile in cache.values_mut() {
            tile.retain_sequence_reservation(reservation.clone());
        }
        self.entries.push_front(CachedSequenceRender {
            identity,
            tiles: cache.clone(),
        });
    }
}

impl Core {
    pub(crate) fn sequence_render_catalog_changed(&mut self) {
        self.sequence_render_cache.catalog_changed();
        self.render_cache.retain(|(band, _), _| *band == 0);
    }

    pub(crate) fn register_pristine_sequence_source(&mut self, source: &SequenceCellSource) {
        let (Some(owner), Some(document)) =
            (self.sequence_render_cache.owner, self.document.as_ref())
        else {
            return;
        };
        self.sequence_render_cache.pristine = Some(PristineSequenceSource {
            identity: SequenceRenderSourceIdentity {
                document_uuid: source.document_uuid,
                source_generation: source.source_generation,
                owner_generation: owner.0,
            },
            document_id: document.id,
            cell_id: document.cell_id,
            document_revision: self.document_revision,
            state: self.current_state,
        });
    }

    fn pristine_sequence_render_source(&self) -> Option<SequenceRenderSourceIdentity> {
        let pristine = self.sequence_render_cache.pristine?;
        let document = self.document.as_ref()?;
        let sequence = self.sequence.as_ref()?;
        let source = sequence.cells.get(sequence.active_index?)?;
        (document.uuid == pristine.identity.document_uuid
            && document.id == pristine.document_id
            && document.cell_id == pristine.cell_id
            && self.document_revision == pristine.document_revision
            && self.current_state == pristine.state
            && source.document_uuid == pristine.identity.document_uuid
            && source.source_generation == pristine.identity.source_generation
            && self.active_stroke.is_none()
            && self.filter_preview.is_none()
            && self.shooting_frame_preview.is_none()
            && self.vanishing_point_preview.is_none()
            && self.floating.is_none()
            && self.color_check.is_none()
            && !self.view.alpha_view)
            .then_some(pristine.identity)
    }

    pub(crate) fn prepare_sequence_render_snapshot(
        &mut self,
    ) -> (
        Option<SequenceRenderSourceIdentity>,
        Option<SequenceRenderReservation>,
    ) {
        self.poll_sequence_render_preparations();
        let identity = self.pristine_sequence_render_source();
        let source = self.sequence.as_ref().and_then(|sequence| {
            sequence
                .active_index
                .and_then(|index| sequence.cells.get(index))
        });
        let reservation = self.sequence_render_cache.prepare(
            identity,
            source,
            self.color_check.is_some() || self.view.alpha_view,
            &mut self.render_cache,
        );
        (identity, reservation)
    }
}
