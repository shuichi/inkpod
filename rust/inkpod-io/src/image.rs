use crate::cache::{BudgetKind, Reservation, SequenceRenderReservation};
use crate::manager::ManagerInner;
use crate::{FileIdentity, FileStamp, IoResult};
use inkpod_format::{CommonRaster, CommonRasterFormat};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Charged<T> {
    value: T,
    _bytes: Reservation,
    slot: Arc<Reservation>,
}

#[derive(Clone)]
pub(crate) struct ByteLease(Arc<Charged<Vec<u8>>>);

impl ByteLease {
    pub(crate) fn new(value: Vec<u8>, bytes: Reservation, slot: Reservation) -> Self {
        Self(Arc::new(Charged {
            value,
            _bytes: bytes,
            slot: Arc::new(slot),
        }))
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0.value
    }

    pub(crate) fn unpinned(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

#[derive(Clone)]
pub(crate) struct ImageLease(Arc<Charged<CommonRaster>>);

impl ImageLease {
    pub(crate) fn new(value: CommonRaster, bytes: Reservation, source: &ByteLease) -> Self {
        Self(Arc::new(Charged {
            value,
            _bytes: bytes,
            slot: Arc::clone(&source.0.slot),
        }))
    }

    pub(crate) fn raster(&self) -> &CommonRaster {
        &self.0.value
    }

    pub(crate) fn reserved_bytes(&self) -> u64 {
        self.0._bytes.amount()
    }

    pub(crate) fn unpinned(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

/// Reservation retained by a consumer-owned tiled/derived image. Cloning shares
/// one charge. A consumer must keep this lease alive for the copied allocation's
/// entire lifetime; dropping the linear source does not release this charge.
#[derive(Clone)]
pub struct DecodedLease(Arc<DerivedCharge>);

struct DerivedCharge {
    bytes: u64,
    // Field order releases the subset before the enclosing decoded charge.
    _sequence_render: Option<SequenceRenderReservation>,
    _reservation: Reservation,
    _slot: Arc<Reservation>,
    // Cache entries never own derived leases, so retaining this owner is acyclic.
    cache_owner: Arc<ManagerInner>,
}

impl DecodedLease {
    pub(crate) fn new(source: &LoadedImage, bytes: u64, reservation: Reservation) -> Self {
        Self(Arc::new(DerivedCharge {
            bytes,
            _sequence_render: None,
            _reservation: reservation,
            _slot: Arc::clone(&source.source.lease.0.slot),
            cache_owner: Arc::clone(&source.cache_owner),
        }))
    }

    /// Reserves one additional immutable sequence display payload from the
    /// original source's manager. `bytes` is the total pixel allocation capacity
    /// in bytes and must be nonzero. Call this before allocating the payload and
    /// retain the returned lease with every owner, including snapshots.
    ///
    /// Each successful call consumes one of
    /// [`crate::MAX_SEQUENCE_RENDER_ALLOCATIONS`] allocations and contributes to
    /// both [`crate::MAX_SEQUENCE_RENDER_BYTES`] and the manager's decoded budget.
    /// Clones share one charge; the last clone releases both reservations. The
    /// new lease shares the source's image slot but does not retain this lease's
    /// pixel charge. All manager clones and their consumers share the limits.
    ///
    /// Zero returns [`crate::IoError::InvalidInput`]; an allocation exceeding
    /// either byte limit returns [`crate::IoError::LimitExceeded`]. Insufficient
    /// remaining capacity returns [`crate::IoError::ResourceBusy`] without
    /// changing existing reservations or cache entries. Successful admission may
    /// evict unpinned image-cache entries. This does no I/O or pixel allocation
    /// and, like [`LoadedImage::reserve_derived`], remains available after worker
    /// shutdown or release of the public manager handle.
    pub fn reserve_sequence_render(&self, bytes: u64) -> IoResult<Self> {
        let (reservation, sequence_render) =
            self.0.cache_owner.cache.reserve_sequence_render(bytes)?;
        Ok(Self(Arc::new(DerivedCharge {
            bytes,
            _sequence_render: Some(sequence_render),
            _reservation: reservation,
            _slot: Arc::clone(&self.0._slot),
            cache_owner: Arc::clone(&self.0.cache_owner),
        })))
    }

    /// Pixel bytes reserved for this allocation; clones do not add another charge.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.0.bytes
    }
}

impl fmt::Debug for DecodedLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecodedLease")
            .field("bytes", &self.0.bytes)
            .finish()
    }
}

/// Immutable encoded bytes. Clones share the byte allocation and keep it charged
/// to the manager's encoded budget even after invalidation or cache eviction.
#[derive(Clone)]
pub struct LoadedBytes {
    pub(crate) path: PathBuf,
    pub(crate) stamp: FileStamp,
    pub(crate) generation: u64,
    pub(crate) lease: ByteLease,
}

impl LoadedBytes {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    }

    #[must_use]
    pub const fn identity(&self) -> FileIdentity {
        self.stamp.identity
    }

    #[must_use]
    pub const fn stamp(&self) -> FileStamp {
        self.stamp
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.lease.bytes()
    }
}

/// Immutable decoded source, without document, history, editor state, or savepoint.
/// Clones share both allocations; no unaccounted mutable/Arc payload escapes.
#[derive(Clone)]
pub struct LoadedImage {
    pub(crate) source: LoadedBytes,
    pub(crate) format: CommonRasterFormat,
    pub(crate) raster: ImageLease,
    // Only public source owners retain the cache. Cache entries hold ImageLease,
    // never LoadedImage, so this does not create a cache ownership cycle.
    pub(crate) cache_owner: Arc<ManagerInner>,
}

impl LoadedImage {
    #[must_use]
    pub fn source(&self) -> &LoadedBytes {
        &self.source
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.source.path()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.source.name()
    }

    #[must_use]
    pub const fn format(&self) -> CommonRasterFormat {
        self.format
    }

    #[must_use]
    pub const fn identity(&self) -> FileIdentity {
        self.source.identity()
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.source.generation()
    }

    #[must_use]
    pub fn raster(&self) -> &CommonRaster {
        self.raster.raster()
    }

    /// Reserves a derived pixel allocation from this image's application budget.
    /// Keep the lease for the allocation's entire lifetime, including snapshots.
    /// Cloning a lease shares its charge. This reserves before allocation and can
    /// fail when pinned images leave insufficient capacity; it never waits for I/O.
    pub fn reserve_derived(&self, bytes: u64) -> IoResult<DecodedLease> {
        let reservation = self.cache_owner.cache.reserve(BudgetKind::Decoded, bytes)?;
        Ok(DecodedLease::new(self, bytes, reservation))
    }
}

impl fmt::Debug for LoadedImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedImage")
            .field("identity", &self.identity())
            .field("generation", &self.generation())
            .field("format", &self.format)
            .field("info", &self.raster().info)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for LoadedBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedBytes")
            .field("identity", &self.identity())
            .field("generation", &self.generation())
            .field("length", &self.stamp.length)
            .finish_non_exhaustive()
    }
}
