//! Canonical immutable assets and the session-local content-addressed store.
//!
//! Asset identities cover the exact canonical descriptor and logical payload.
//! Encoded source bytes, paths, provenance, allocation layout, and tile revisions
//! never contribute to an [`AssetId`].

use crate::{CoreError, MAX_RASTER_DIMENSION, PixelFormat, PixelValue, TileRaster};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const ASSET_DIGEST_CONTEXT: &str = "org.inkpod.digest.asset.v1";
const ASSET_SCHEMA_VERSION: u32 = 1;
const ASSET_DIGEST_FIELD_COUNT: u32 = 11;
const MAX_ASSET_COUNT: usize = 65_536;
const MAX_ONE_ASSET_BYTES: u64 = 512 * 1_024 * 1_024;
const MAX_TOTAL_ASSET_BYTES: u64 = 768 * 1_024 * 1_024;
const MAX_STREAM_ELEMENTS: u64 = 1_048_576;
const MATERIALIZED_ASSET_REVISION: u64 = 1;

/// A full BLAKE3-256 identity for one canonical immutable asset.
///
/// The identity belongs to the asset's canonical descriptor and logical payload,
/// not to a store slot, path, codec, or caller-owned buffer. It remains valid
/// across Core sessions that use the same asset digest schema.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct AssetId([u8; 32]);

impl AssetId {
    /// Constructs an identity from its complete fixed-width digest bytes.
    ///
    /// This does not establish that the bytes identify a registered asset. APIs
    /// accepting an expected ID recompute and verify it before publication.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrows the complete digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the complete digest bytes by value.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Semantic kind of an immutable canonical asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetKind {
    /// A tightly packed, top-to-bottom canonical raster.
    CanonicalRaster,
    /// A primitive-schema-defined ordered vector record stream.
    CanonicalVectorStream,
    /// A primitive-schema-defined ordered input-sample stream.
    CanonicalSampleStream,
}

impl AssetKind {
    const fn code(self) -> u32 {
        match self {
            Self::CanonicalRaster => 1,
            Self::CanonicalVectorStream => 2,
            Self::CanonicalSampleStream => 3,
        }
    }
}

/// Canonical color space carried by a color raster asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetColorSpace {
    /// Standard RGB with the canonical sRGB transfer function.
    Srgb,
}

impl AssetColorSpace {
    const fn code(self) -> u32 {
        match self {
            Self::Srgb => 1,
        }
    }
}

/// Canonical interpretation of an asset raster's alpha-like component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetAlphaSemantics {
    /// Every sample is opaque.
    Opaque,
    /// Color channels use straight, unassociated alpha.
    Straight,
    /// A binary raster represents coverage rather than an opaque color channel.
    CoverageMask,
}

impl AssetAlphaSemantics {
    const fn code(self) -> u32 {
        match self {
            Self::Opaque => 1,
            Self::Straight => 2,
            Self::CoverageMask => 3,
        }
    }
}

/// Canonical metadata included in an immutable asset's content identity.
///
/// Raster assets have every optional raster field present. Vector and sample
/// streams have all raster-specific fields absent and define their element
/// records in the primitive schema that references them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetDescriptor {
    /// Semantic asset kind.
    pub kind: AssetKind,
    /// Canonical raster format, or `None` for a stream asset.
    pub pixel_format: Option<PixelFormat>,
    /// Canonical color space, present only for straight RGBA rasters.
    pub color_space: Option<AssetColorSpace>,
    /// Alpha or coverage interpretation, present only for raster assets.
    pub alpha_semantics: Option<AssetAlphaSemantics>,
    /// Raster width in pixels, or `None` for a stream asset.
    pub width: Option<u32>,
    /// Raster height in pixels, or `None` for a stream asset.
    pub height: Option<u32>,
    /// Exact logical row stride, or `None` for a stream asset.
    pub canonical_stride: Option<u64>,
    /// Raster pixel count or primitive-schema-defined stream element count.
    pub logical_element_count: u64,
    /// Exact number of bytes in the canonical logical payload.
    pub logical_payload_length: u64,
}

/// Owned input for one canonical raster asset ingestion.
///
/// `canonical_stride`, color space, and alpha semantics are supplied explicitly
/// so malformed or forged metadata can be rejected rather than silently fixed.
/// On success the store owns immutable copies; no reference to this value is
/// retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterAssetInput {
    /// Raster width in pixels.
    pub width: u32,
    /// Raster height in pixels.
    pub height: u32,
    /// Exact canonical pixel representation.
    pub pixel_format: PixelFormat,
    /// Color space declared by the source descriptor.
    pub color_space: Option<AssetColorSpace>,
    /// Alpha or coverage interpretation declared by the source descriptor.
    pub alpha_semantics: AssetAlphaSemantics,
    /// Exact byte distance between adjacent logical rows.
    pub canonical_stride: u64,
    /// Tightly packed top-to-bottom logical pixels with no row padding.
    pub pixels: Vec<u8>,
    /// Optional identity supplied by a decoder or persisted descriptor.
    pub expected_id: Option<AssetId>,
}

impl RasterAssetInput {
    /// Copies an existing tiled raster into the canonical dense logical layout.
    ///
    /// Missing sparse tiles are emitted as exact all-zero pixels. Display-only
    /// premultiplied BGRA and payloads over the per-asset bound are rejected.
    pub fn from_tile_raster(
        raster: &TileRaster,
        expected_id: Option<AssetId>,
    ) -> Result<Self, CoreError> {
        let (color_space, alpha_semantics) = canonical_raster_semantics(raster.format())?;
        let bytes_per_pixel = u64::try_from(raster.format().bytes_per_pixel())
            .map_err(|_| CoreError::InvalidState("asset pixel size is not representable"))?;
        let canonical_stride = u64::from(raster.width())
            .checked_mul(bytes_per_pixel)
            .ok_or(CoreError::InvalidArgument("asset stride overflows"))?;
        let payload_length = canonical_stride
            .checked_mul(u64::from(raster.height()))
            .ok_or(CoreError::InvalidArgument("asset payload length overflows"))?;
        validate_payload_bound(payload_length)?;
        let capacity = usize::try_from(payload_length)
            .map_err(|_| CoreError::InvalidArgument("asset payload length is not addressable"))?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::InvalidState("asset payload allocation failed"))?;
        for y in 0..raster.height() {
            for x in 0..raster.width() {
                append_pixel_bytes(&mut pixels, raster.pixel(x, y)?);
            }
        }
        Ok(Self {
            width: raster.width(),
            height: raster.height(),
            pixel_format: raster.format(),
            color_space,
            alpha_semantics,
            canonical_stride,
            pixels,
            expected_id,
        })
    }
}

/// Owned canonical bytes for a vector or input-sample stream asset.
///
/// The referencing primitive schema defines and validates individual element
/// records before ingestion. The store enforces the common count, byte, and
/// identity bounds and retains no caller-owned memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalStreamInput {
    /// Either [`AssetKind::CanonicalVectorStream`] or
    /// [`AssetKind::CanonicalSampleStream`].
    pub kind: AssetKind,
    /// Number of canonical records in `payload`.
    pub element_count: u64,
    /// Exact canonical record bytes in semantic order.
    pub payload: Vec<u8>,
    /// Optional identity supplied by a decoder or persisted descriptor.
    pub expected_id: Option<AssetId>,
}

/// Read-only metadata for one retained store entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetInfo {
    /// Content-derived immutable identity.
    pub id: AssetId,
    /// Canonical descriptor covered by `id`.
    pub descriptor: AssetDescriptor,
    /// Number of semantic roots supplied by the most recent GC root scan.
    pub reference_count: u64,
}

/// Deterministic logical resource usage for an immutable asset store.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AssetStoreUsage {
    /// Number of retained unique content identities.
    pub asset_count: u64,
    /// Sum of canonical logical payload lengths after deduplication.
    pub logical_payload_bytes: u64,
    /// Number of retained assets with at least one semantic root.
    pub referenced_asset_count: u64,
    /// Sum of semantic reference counts from the most recent GC root scan.
    pub total_reference_count: u64,
}

#[derive(Debug)]
pub(crate) struct AssetRecord {
    id: AssetId,
    descriptor: AssetDescriptor,
    payload: Arc<[u8]>,
    raster: Option<Arc<TileRaster>>,
}

impl AssetRecord {
    pub(crate) const fn id(&self) -> AssetId {
        self.id
    }

    pub(crate) const fn descriptor(&self) -> AssetDescriptor {
        self.descriptor
    }

    pub(crate) fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub(crate) fn raster(&self) -> Option<&Arc<TileRaster>> {
        self.raster.as_ref()
    }

    fn info(&self, reference_count: u64) -> AssetInfo {
        AssetInfo {
            id: self.id,
            descriptor: self.descriptor,
            reference_count,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AssetStore {
    records: BTreeMap<AssetId, Arc<AssetRecord>>,
    reference_counts: BTreeMap<AssetId, u64>,
    logical_payload_bytes: u64,
}

impl AssetStore {
    pub(crate) fn persistent_records(&self) -> Vec<(AssetId, AssetDescriptor, &[u8])> {
        self.records
            .values()
            .map(|record| (record.id(), record.descriptor(), record.payload()))
            .collect()
    }

    pub(crate) fn ingest_persistent(
        &mut self,
        id: AssetId,
        descriptor: AssetDescriptor,
        payload: Vec<u8>,
    ) -> Result<(), CoreError> {
        let record = match descriptor.kind {
            AssetKind::CanonicalRaster => self.ingest_raster(RasterAssetInput {
                width: descriptor.width.ok_or(CoreError::Format(
                    "persistent raster asset width is missing".to_owned(),
                ))?,
                height: descriptor.height.ok_or(CoreError::Format(
                    "persistent raster asset height is missing".to_owned(),
                ))?,
                pixel_format: descriptor.pixel_format.ok_or(CoreError::Format(
                    "persistent raster asset format is missing".to_owned(),
                ))?,
                color_space: descriptor.color_space,
                alpha_semantics: descriptor.alpha_semantics.ok_or(CoreError::Format(
                    "persistent raster alpha semantics are missing".to_owned(),
                ))?,
                canonical_stride: descriptor.canonical_stride.ok_or(CoreError::Format(
                    "persistent raster stride is missing".to_owned(),
                ))?,
                pixels: payload,
                expected_id: Some(id),
            })?,
            AssetKind::CanonicalVectorStream | AssetKind::CanonicalSampleStream => self
                .ingest_stream(CanonicalStreamInput {
                    kind: descriptor.kind,
                    element_count: descriptor.logical_element_count,
                    payload,
                    expected_id: Some(id),
                })?,
        };
        if record.descriptor() != descriptor {
            return Err(CoreError::Format(
                "persistent asset descriptor is not canonical".to_owned(),
            ));
        }
        Ok(())
    }

    /// Interns a validated immutable record produced by another store.
    ///
    /// The record's canonical identity and payload were already validated at
    /// construction time, so this path deliberately does not hash or
    /// materialize the content again. An existing identity is still compared
    /// byte-for-byte to keep digest collisions fail-closed.
    pub(crate) fn intern_record(
        &mut self,
        record: Arc<AssetRecord>,
    ) -> Result<Arc<AssetRecord>, CoreError> {
        if let Some(existing) = self.records.get(&record.id) {
            return validate_deduplicated(existing, record.descriptor, record.payload());
        }
        self.ensure_new_asset_capacity(record.descriptor.logical_payload_length)?;
        self.insert_new(Arc::clone(&record))?;
        Ok(record)
    }

    pub(crate) fn ingest_raster(
        &mut self,
        input: RasterAssetInput,
    ) -> Result<Arc<AssetRecord>, CoreError> {
        let descriptor = validate_raster_input(&input)?;
        let id = canonical_asset_id(descriptor, &input.pixels)?;
        validate_expected_id(input.expected_id, id)?;
        if let Some(existing) = self.records.get(&id) {
            return validate_deduplicated(existing, descriptor, &input.pixels);
        }
        self.ensure_new_asset_capacity(descriptor.logical_payload_length)?;
        let raster = materialize_raster(descriptor, &input.pixels)?;
        let record = Arc::new(AssetRecord {
            id,
            descriptor,
            payload: Arc::from(input.pixels),
            raster: Some(Arc::new(raster)),
        });
        self.insert_new(Arc::clone(&record))?;
        Ok(record)
    }

    pub(crate) fn ingest_tile_raster(
        &mut self,
        raster: &TileRaster,
        expected_id: Option<AssetId>,
    ) -> Result<Arc<AssetRecord>, CoreError> {
        self.ingest_raster(RasterAssetInput::from_tile_raster(raster, expected_id)?)
    }

    pub(crate) fn ingest_stream(
        &mut self,
        input: CanonicalStreamInput,
    ) -> Result<Arc<AssetRecord>, CoreError> {
        let descriptor = validate_stream_input(&input)?;
        let id = canonical_asset_id(descriptor, &input.payload)?;
        validate_expected_id(input.expected_id, id)?;
        if let Some(existing) = self.records.get(&id) {
            return validate_deduplicated(existing, descriptor, &input.payload);
        }
        self.ensure_new_asset_capacity(descriptor.logical_payload_length)?;
        let record = Arc::new(AssetRecord {
            id,
            descriptor,
            payload: Arc::from(input.payload),
            raster: None,
        });
        self.insert_new(Arc::clone(&record))?;
        Ok(record)
    }

    pub(crate) fn get(&self, id: AssetId) -> Option<Arc<AssetRecord>> {
        self.records.get(&id).cloned()
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: AssetId) -> bool {
        self.records.contains_key(&id)
    }

    pub(crate) fn info(&self, id: AssetId) -> Option<AssetInfo> {
        self.records
            .get(&id)
            .map(|record| record.info(self.reference_count(id)))
    }

    pub(crate) fn infos(&self) -> Vec<AssetInfo> {
        self.records
            .values()
            .map(|record| record.info(self.reference_count(record.id)))
            .collect()
    }

    pub(crate) fn reference_count(&self, id: AssetId) -> u64 {
        self.reference_counts.get(&id).copied().unwrap_or(0)
    }

    pub(crate) fn usage(&self) -> AssetStoreUsage {
        let referenced_asset_count = self
            .records
            .keys()
            .filter(|id| self.reference_count(**id) != 0)
            .count() as u64;
        let total_reference_count = self.records.keys().fold(0_u64, |total, id| {
            total.saturating_add(self.reference_count(*id))
        });
        AssetStoreUsage {
            asset_count: self.records.len() as u64,
            logical_payload_bytes: self.logical_payload_bytes,
            referenced_asset_count,
            total_reference_count,
        }
    }

    /// Recomputes semantic reference counts and removes every unrooted asset.
    ///
    /// Duplicate IDs in `roots` represent distinct semantic owners. A missing
    /// root or reference-count overflow rejects the entire scan without changing
    /// store contents or the previously published reference counts.
    pub(crate) fn garbage_collect(
        &mut self,
        roots: impl IntoIterator<Item = AssetId>,
    ) -> Result<u64, CoreError> {
        let mut counts = BTreeMap::<AssetId, u64>::new();
        for id in roots {
            if !self.records.contains_key(&id) {
                return Err(CoreError::InvalidState(
                    "asset retention root is not registered",
                ));
            }
            let count = counts.entry(id).or_insert(0);
            *count = count
                .checked_add(1)
                .ok_or(CoreError::InvalidState("asset reference count overflows"))?;
        }

        let retained_payload_bytes = self
            .records
            .iter()
            .filter(|(id, _)| counts.contains_key(id))
            .try_fold(0_u64, |total, (_, record)| {
                total
                    .checked_add(record.descriptor.logical_payload_length)
                    .ok_or(CoreError::InvalidState("asset store usage overflows"))
            })?;
        let before = self.records.len();
        self.records.retain(|id, _| counts.contains_key(id));
        self.reference_counts = counts;
        self.logical_payload_bytes = retained_payload_bytes;
        Ok((before - self.records.len()) as u64)
    }

    /// Rebuilds every uniquely rooted asset through the canonical ingestion path.
    ///
    /// Root multiplicity is preserved for reference accounting, while records are
    /// copied in ascending content-identity order. The returned store shares no
    /// record, payload, or materialized raster allocation with this store. This is
    /// a runtime replay-validation boundary, not a native-file representation.
    pub(crate) fn detached_archive_round_trip(
        &self,
        roots: impl IntoIterator<Item = AssetId>,
    ) -> Result<Self, CoreError> {
        let roots = roots.into_iter().collect::<Vec<_>>();
        let unique_roots = roots.iter().copied().collect::<BTreeSet<_>>();
        let mut detached = Self::default();

        for id in unique_roots {
            let source = self.records.get(&id).ok_or(CoreError::InvalidState(
                "asset retention root is not registered",
            ))?;
            let descriptor = source.descriptor();
            let payload = detached_payload_copy(source.payload())?;
            let record = match descriptor.kind {
                AssetKind::CanonicalRaster => detached.ingest_raster(RasterAssetInput {
                    width: descriptor.width.ok_or(CoreError::InvalidState(
                        "detached raster asset width is missing",
                    ))?,
                    height: descriptor.height.ok_or(CoreError::InvalidState(
                        "detached raster asset height is missing",
                    ))?,
                    pixel_format: descriptor.pixel_format.ok_or(CoreError::InvalidState(
                        "detached raster asset format is missing",
                    ))?,
                    color_space: descriptor.color_space,
                    alpha_semantics: descriptor.alpha_semantics.ok_or(CoreError::InvalidState(
                        "detached raster asset alpha semantics are missing",
                    ))?,
                    canonical_stride: descriptor.canonical_stride.ok_or(
                        CoreError::InvalidState("detached raster asset stride is missing"),
                    )?,
                    pixels: payload,
                    expected_id: Some(id),
                })?,
                AssetKind::CanonicalVectorStream | AssetKind::CanonicalSampleStream => detached
                    .ingest_stream(CanonicalStreamInput {
                        kind: descriptor.kind,
                        element_count: descriptor.logical_element_count,
                        payload,
                        expected_id: Some(id),
                    })?,
            };
            if record.descriptor() != descriptor || record.payload() != source.payload() {
                return Err(CoreError::InvalidState(
                    "detached asset round-trip changed canonical content",
                ));
            }
        }

        detached.garbage_collect(roots)?;
        Ok(detached)
    }

    fn ensure_new_asset_capacity(&self, payload_length: u64) -> Result<(), CoreError> {
        if self.records.len() >= MAX_ASSET_COUNT {
            return Err(CoreError::InvalidState("asset count limit exceeded"));
        }
        if self
            .logical_payload_bytes
            .checked_add(payload_length)
            .is_none_or(|total| total > MAX_TOTAL_ASSET_BYTES)
        {
            return Err(CoreError::InvalidState(
                "total asset payload limit exceeded",
            ));
        }
        Ok(())
    }

    fn insert_new(&mut self, record: Arc<AssetRecord>) -> Result<(), CoreError> {
        let new_total = self
            .logical_payload_bytes
            .checked_add(record.descriptor.logical_payload_length)
            .ok_or(CoreError::InvalidState("asset store usage overflows"))?;
        if self.records.contains_key(&record.id) {
            return Err(CoreError::InvalidState(
                "asset store identity was inserted twice",
            ));
        }
        self.records.insert(record.id, record);
        self.logical_payload_bytes = new_total;
        Ok(())
    }
}

fn detached_payload_copy(source: &[u8]) -> Result<Vec<u8>, CoreError> {
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(source.len())
        .map_err(|_| CoreError::InvalidState("detached asset payload allocation failed"))?;
    payload.extend_from_slice(source);
    Ok(payload)
}

fn validate_deduplicated(
    existing: &Arc<AssetRecord>,
    descriptor: AssetDescriptor,
    payload: &[u8],
) -> Result<Arc<AssetRecord>, CoreError> {
    if existing.descriptor != descriptor || existing.payload.as_ref() != payload {
        return Err(CoreError::InvalidState(
            "asset digest collision has mismatched canonical content",
        ));
    }
    Ok(Arc::clone(existing))
}

fn validate_expected_id(expected: Option<AssetId>, actual: AssetId) -> Result<(), CoreError> {
    if expected.is_some_and(|expected| expected != actual) {
        Err(CoreError::InvalidArgument(
            "asset ID does not match canonical content",
        ))
    } else {
        Ok(())
    }
}

fn validate_raster_input(input: &RasterAssetInput) -> Result<AssetDescriptor, CoreError> {
    if input.width == 0
        || input.height == 0
        || input.width > MAX_RASTER_DIMENSION
        || input.height > MAX_RASTER_DIMENSION
    {
        return Err(CoreError::InvalidArgument(
            "asset raster dimensions are outside bounds",
        ));
    }
    let (color_space, alpha_semantics) = canonical_raster_semantics(input.pixel_format)?;
    if input.color_space != color_space || input.alpha_semantics != alpha_semantics {
        return Err(CoreError::InvalidArgument(
            "asset raster color or alpha semantics are not canonical",
        ));
    }
    let bytes_per_pixel = u64::try_from(input.pixel_format.bytes_per_pixel())
        .map_err(|_| CoreError::InvalidArgument("asset pixel size is not representable"))?;
    let canonical_stride = u64::from(input.width)
        .checked_mul(bytes_per_pixel)
        .ok_or(CoreError::InvalidArgument("asset stride overflows"))?;
    if input.canonical_stride != canonical_stride {
        return Err(CoreError::InvalidArgument(
            "asset raster stride is not canonical",
        ));
    }
    let logical_element_count = u64::from(input.width)
        .checked_mul(u64::from(input.height))
        .ok_or(CoreError::InvalidArgument("asset element count overflows"))?;
    let logical_payload_length = canonical_stride
        .checked_mul(u64::from(input.height))
        .ok_or(CoreError::InvalidArgument("asset payload length overflows"))?;
    validate_payload_bound(logical_payload_length)?;
    if u64::try_from(input.pixels.len()).ok() != Some(logical_payload_length) {
        return Err(CoreError::InvalidArgument(
            "asset payload length does not match its descriptor",
        ));
    }
    if input.pixel_format == PixelFormat::BinaryMask8
        && input
            .pixels
            .iter()
            .any(|value| !matches!(*value, 0 | u8::MAX))
    {
        return Err(CoreError::InvalidArgument(
            "binary asset samples must be zero or 255",
        ));
    }
    Ok(AssetDescriptor {
        kind: AssetKind::CanonicalRaster,
        pixel_format: Some(input.pixel_format),
        color_space,
        alpha_semantics: Some(alpha_semantics),
        width: Some(input.width),
        height: Some(input.height),
        canonical_stride: Some(canonical_stride),
        logical_element_count,
        logical_payload_length,
    })
}

fn validate_stream_input(input: &CanonicalStreamInput) -> Result<AssetDescriptor, CoreError> {
    if !matches!(
        input.kind,
        AssetKind::CanonicalVectorStream | AssetKind::CanonicalSampleStream
    ) {
        return Err(CoreError::InvalidArgument(
            "canonical stream asset kind is invalid",
        ));
    }
    if input.element_count > MAX_STREAM_ELEMENTS {
        return Err(CoreError::InvalidArgument(
            "canonical stream element count exceeds its bound",
        ));
    }
    let logical_payload_length = u64::try_from(input.payload.len())
        .map_err(|_| CoreError::InvalidArgument("asset payload length is not representable"))?;
    validate_payload_bound(logical_payload_length)?;
    if (input.element_count == 0) != input.payload.is_empty() {
        return Err(CoreError::InvalidArgument(
            "canonical stream count and payload emptiness disagree",
        ));
    }
    Ok(AssetDescriptor {
        kind: input.kind,
        pixel_format: None,
        color_space: None,
        alpha_semantics: None,
        width: None,
        height: None,
        canonical_stride: None,
        logical_element_count: input.element_count,
        logical_payload_length,
    })
}

fn validate_payload_bound(length: u64) -> Result<(), CoreError> {
    if length > MAX_ONE_ASSET_BYTES {
        Err(CoreError::InvalidArgument(
            "asset payload exceeds the per-asset bound",
        ))
    } else {
        Ok(())
    }
}

fn canonical_raster_semantics(
    format: PixelFormat,
) -> Result<(Option<AssetColorSpace>, AssetAlphaSemantics), CoreError> {
    match format {
        PixelFormat::BinaryMask8 => Ok((None, AssetAlphaSemantics::CoverageMask)),
        PixelFormat::Grayscale8 | PixelFormat::Grayscale16 => {
            Ok((None, AssetAlphaSemantics::Opaque))
        }
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16 => {
            Ok((Some(AssetColorSpace::Srgb), AssetAlphaSemantics::Straight))
        }
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidArgument(
            "display-only premultiplied BGRA cannot be an immutable asset",
        )),
    }
}

fn canonical_asset_id(descriptor: AssetDescriptor, payload: &[u8]) -> Result<AssetId, CoreError> {
    if u64::try_from(payload.len()).ok() != Some(descriptor.logical_payload_length) {
        return Err(CoreError::InvalidArgument(
            "asset payload length does not match its descriptor",
        ));
    }
    let mut hasher = blake3::Hasher::new_derive_key(ASSET_DIGEST_CONTEXT);
    hasher.update(&ASSET_SCHEMA_VERSION.to_le_bytes());
    hasher.update(&ASSET_DIGEST_FIELD_COUNT.to_le_bytes());
    hash_present_field(&mut hasher, 1, &ASSET_SCHEMA_VERSION.to_le_bytes())?;
    hash_present_field(&mut hasher, 2, &descriptor.kind.code().to_le_bytes())?;
    let pixel_format = descriptor.pixel_format.map(pixel_format_code).transpose()?;
    let pixel_format = pixel_format.map(u32::to_le_bytes);
    hash_optional_field(
        &mut hasher,
        3,
        pixel_format.as_ref().map(<[u8; 4]>::as_slice),
    )?;
    let color_space = descriptor.color_space.map(AssetColorSpace::code);
    let color_space = color_space.map(u32::to_le_bytes);
    hash_optional_field(
        &mut hasher,
        4,
        color_space.as_ref().map(<[u8; 4]>::as_slice),
    )?;
    let alpha_semantics = descriptor.alpha_semantics.map(AssetAlphaSemantics::code);
    let alpha_semantics = alpha_semantics.map(u32::to_le_bytes);
    hash_optional_field(
        &mut hasher,
        5,
        alpha_semantics.as_ref().map(<[u8; 4]>::as_slice),
    )?;
    let width = descriptor.width.map(u32::to_le_bytes);
    hash_optional_field(&mut hasher, 6, width.as_ref().map(<[u8; 4]>::as_slice))?;
    let height = descriptor.height.map(u32::to_le_bytes);
    hash_optional_field(&mut hasher, 7, height.as_ref().map(<[u8; 4]>::as_slice))?;
    let stride = descriptor.canonical_stride.map(u64::to_le_bytes);
    hash_optional_field(&mut hasher, 8, stride.as_ref().map(<[u8; 8]>::as_slice))?;
    hash_present_field(
        &mut hasher,
        9,
        &descriptor.logical_element_count.to_le_bytes(),
    )?;
    hash_present_field(
        &mut hasher,
        10,
        &descriptor.logical_payload_length.to_le_bytes(),
    )?;
    hash_present_field(&mut hasher, 11, payload)?;
    Ok(AssetId::from_bytes(*hasher.finalize().as_bytes()))
}

fn hash_present_field(
    hasher: &mut blake3::Hasher,
    ordinal: u32,
    bytes: &[u8],
) -> Result<(), CoreError> {
    hash_optional_field(hasher, ordinal, Some(bytes))
}

fn hash_optional_field(
    hasher: &mut blake3::Hasher,
    ordinal: u32,
    bytes: Option<&[u8]>,
) -> Result<(), CoreError> {
    let length = bytes.map_or(0, <[u8]>::len);
    let length = u64::try_from(length)
        .map_err(|_| CoreError::InvalidArgument("asset digest field length overflows"))?;
    hasher.update(&ordinal.to_le_bytes());
    hasher.update(&[u8::from(bytes.is_some()), 0, 0, 0]);
    hasher.update(&length.to_le_bytes());
    if let Some(bytes) = bytes {
        hasher.update(bytes);
    }
    Ok(())
}

fn pixel_format_code(format: PixelFormat) -> Result<u32, CoreError> {
    match format {
        PixelFormat::BinaryMask8 => Ok(1),
        PixelFormat::Grayscale8 => Ok(2),
        PixelFormat::Grayscale16 => Ok(3),
        PixelFormat::StraightRgba8 => Ok(4),
        PixelFormat::StraightRgba16 => Ok(5),
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidArgument(
            "display-only pixel format cannot enter an asset digest",
        )),
    }
}

fn materialize_raster(
    descriptor: AssetDescriptor,
    payload: &[u8],
) -> Result<TileRaster, CoreError> {
    let format = descriptor
        .pixel_format
        .ok_or(CoreError::InvalidState("raster asset format is missing"))?;
    let width = descriptor
        .width
        .ok_or(CoreError::InvalidState("raster asset width is missing"))?;
    let height = descriptor
        .height
        .ok_or(CoreError::InvalidState("raster asset height is missing"))?;
    let mut raster = TileRaster::new(width, height, format)?;
    let bytes_per_pixel = format.bytes_per_pixel();
    for (index, bytes) in payload.chunks_exact(bytes_per_pixel).enumerate() {
        let index = u64::try_from(index)
            .map_err(|_| CoreError::InvalidState("asset pixel index is not representable"))?;
        let x = u32::try_from(index % u64::from(width))
            .map_err(|_| CoreError::InvalidState("asset X coordinate overflows"))?;
        let y = u32::try_from(index / u64::from(width))
            .map_err(|_| CoreError::InvalidState("asset Y coordinate overflows"))?;
        let value = decode_pixel(format, bytes)?;
        if !value.is_zero() {
            raster.set_pixel(x, y, value, MATERIALIZED_ASSET_REVISION)?;
        }
    }
    Ok(raster)
}

fn decode_pixel(format: PixelFormat, bytes: &[u8]) -> Result<PixelValue, CoreError> {
    match format {
        PixelFormat::BinaryMask8 => bytes
            .first()
            .copied()
            .map(PixelValue::Binary)
            .ok_or(CoreError::InvalidState("binary asset pixel is truncated")),
        PixelFormat::Grayscale8 => {
            bytes
                .first()
                .copied()
                .map(PixelValue::Grayscale8)
                .ok_or(CoreError::InvalidState(
                    "grayscale asset pixel is truncated",
                ))
        }
        PixelFormat::Grayscale16 => Ok(PixelValue::Grayscale16(u16::from_le_bytes(
            bytes
                .try_into()
                .map_err(|_| CoreError::InvalidState("grayscale16 asset pixel is truncated"))?,
        ))),
        PixelFormat::StraightRgba8 => {
            Ok(PixelValue::Rgba(bytes.try_into().map_err(|_| {
                CoreError::InvalidState("RGBA8 asset pixel is truncated")
            })?))
        }
        PixelFormat::StraightRgba16 => {
            let mut channels = [0_u16; 4];
            for (channel, bytes) in channels.iter_mut().zip(bytes.chunks_exact(2)) {
                *channel = u16::from_le_bytes([bytes[0], bytes[1]]);
            }
            Ok(PixelValue::Rgba16(channels))
        }
        PixelFormat::PremultipliedBgra8 => Err(CoreError::InvalidState(
            "display-only pixel format reached asset materialization",
        )),
    }
}

fn append_pixel_bytes(bytes: &mut Vec<u8>, pixel: PixelValue) {
    match pixel {
        PixelValue::Binary(value) => bytes.push(value),
        PixelValue::Grayscale8(value) => bytes.push(value),
        PixelValue::Grayscale16(value) => bytes.extend_from_slice(&value.to_le_bytes()),
        PixelValue::Rgba(value) => bytes.extend_from_slice(&value),
        PixelValue::Rgba16(value) => {
            for channel in value {
                bytes.extend_from_slice(&channel.to_le_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster_input(
        format: PixelFormat,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> RasterAssetInput {
        let (color_space, alpha_semantics) = canonical_raster_semantics(format).unwrap();
        RasterAssetInput {
            width,
            height,
            pixel_format: format,
            color_space,
            alpha_semantics,
            canonical_stride: u64::from(width) * format.bytes_per_pixel() as u64,
            pixels,
            expected_id: None,
        }
    }

    fn independent_asset_id(descriptor: AssetDescriptor, payload: &[u8]) -> AssetId {
        fn field(bytes: &mut Vec<u8>, ordinal: u32, value: Option<&[u8]>) {
            bytes.extend_from_slice(&ordinal.to_le_bytes());
            bytes.extend_from_slice(&[u8::from(value.is_some()), 0, 0, 0]);
            bytes.extend_from_slice(&(value.map_or(0, <[u8]>::len) as u64).to_le_bytes());
            if let Some(value) = value {
                bytes.extend_from_slice(value);
            }
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&11_u32.to_le_bytes());
        field(&mut bytes, 1, Some(&1_u32.to_le_bytes()));
        field(&mut bytes, 2, Some(&descriptor.kind.code().to_le_bytes()));
        let pixel_format = descriptor
            .pixel_format
            .map(|format| pixel_format_code(format).unwrap().to_le_bytes());
        field(
            &mut bytes,
            3,
            pixel_format.as_ref().map(<[u8; 4]>::as_slice),
        );
        let color_space = descriptor
            .color_space
            .map(|value| value.code().to_le_bytes());
        field(&mut bytes, 4, color_space.as_ref().map(<[u8; 4]>::as_slice));
        let alpha = descriptor
            .alpha_semantics
            .map(|value| value.code().to_le_bytes());
        field(&mut bytes, 5, alpha.as_ref().map(<[u8; 4]>::as_slice));
        let width = descriptor.width.map(u32::to_le_bytes);
        field(&mut bytes, 6, width.as_ref().map(<[u8; 4]>::as_slice));
        let height = descriptor.height.map(u32::to_le_bytes);
        field(&mut bytes, 7, height.as_ref().map(<[u8; 4]>::as_slice));
        let stride = descriptor.canonical_stride.map(u64::to_le_bytes);
        field(&mut bytes, 8, stride.as_ref().map(<[u8; 8]>::as_slice));
        field(
            &mut bytes,
            9,
            Some(&descriptor.logical_element_count.to_le_bytes()),
        );
        field(
            &mut bytes,
            10,
            Some(&descriptor.logical_payload_length.to_le_bytes()),
        );
        field(&mut bytes, 11, Some(payload));
        let digest = blake3::Hasher::new_derive_key(ASSET_DIGEST_CONTEXT)
            .update(&bytes)
            .finalize();
        AssetId::from_bytes(*digest.as_bytes())
    }

    #[test]
    fn exact_asset_frame_and_raster_semantics_cover_every_canonical_format() {
        let cases = [
            (
                PixelFormat::BinaryMask8,
                vec![0, 255],
                None,
                AssetAlphaSemantics::CoverageMask,
                PixelValue::Binary(255),
            ),
            (
                PixelFormat::Grayscale8,
                vec![0, 127],
                None,
                AssetAlphaSemantics::Opaque,
                PixelValue::Grayscale8(127),
            ),
            (
                PixelFormat::Grayscale16,
                [0_u16, 32_768]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
                None,
                AssetAlphaSemantics::Opaque,
                PixelValue::Grayscale16(32_768),
            ),
            (
                PixelFormat::StraightRgba8,
                vec![0, 0, 0, 0, 1, 2, 3, 4],
                Some(AssetColorSpace::Srgb),
                AssetAlphaSemantics::Straight,
                PixelValue::Rgba([1, 2, 3, 4]),
            ),
            (
                PixelFormat::StraightRgba16,
                [0_u16, 0, 0, 0, 1, 2, 3, 4]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
                Some(AssetColorSpace::Srgb),
                AssetAlphaSemantics::Straight,
                PixelValue::Rgba16([1, 2, 3, 4]),
            ),
        ];

        let mut store = AssetStore::default();
        let mut ids = Vec::new();
        for (format, pixels, color_space, alpha, expected_pixel) in cases {
            let record = store
                .ingest_raster(raster_input(format, 2, 1, pixels.clone()))
                .unwrap();
            assert_eq!(record.descriptor.color_space, color_space);
            assert_eq!(record.descriptor.alpha_semantics, Some(alpha));
            assert_eq!(record.descriptor.logical_element_count, 2);
            assert_eq!(record.id, independent_asset_id(record.descriptor, &pixels));
            assert_eq!(
                record.raster().unwrap().pixel(1, 0).unwrap(),
                expected_pixel
            );
            ids.push(record.id);
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn transparent_color_bytes_are_identity_significant_and_round_trip_exactly() {
        let mut store = AssetStore::default();
        let transparent_with_color = store
            .ingest_raster(raster_input(
                PixelFormat::StraightRgba8,
                1,
                1,
                vec![90, 80, 70, 0],
            ))
            .unwrap();
        let transparent_zero = store
            .ingest_raster(raster_input(
                PixelFormat::StraightRgba8,
                1,
                1,
                vec![0, 0, 0, 0],
            ))
            .unwrap();
        assert_ne!(transparent_with_color.id(), transparent_zero.id());
        assert_eq!(transparent_with_color.payload(), [90, 80, 70, 0]);
        assert_eq!(
            transparent_with_color
                .raster()
                .unwrap()
                .pixel(0, 0)
                .unwrap(),
            PixelValue::Rgba([90, 80, 70, 0])
        );
    }

    #[test]
    fn identical_content_deduplicates_and_expected_identity_is_verified() {
        let mut store = AssetStore::default();
        let input = raster_input(PixelFormat::StraightRgba8, 1, 1, vec![10, 20, 30, 40]);
        let first = store.ingest_raster(input.clone()).unwrap();
        let mut verified = input;
        verified.expected_id = Some(first.id());
        let second = store.ingest_raster(verified).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(store.usage().asset_count, 1);
        assert_eq!(store.usage().logical_payload_bytes, 4);

        let mut forged = raster_input(PixelFormat::StraightRgba8, 1, 1, vec![10, 20, 30, 40]);
        let mut forged_id = first.id().into_bytes();
        forged_id[0] ^= 0xff;
        forged.expected_id = Some(AssetId::from_bytes(forged_id));
        assert!(matches!(
            store.ingest_raster(forged),
            Err(CoreError::InvalidArgument(
                "asset ID does not match canonical content"
            ))
        ));
        assert_eq!(store.usage().asset_count, 1);
    }

    #[test]
    fn forged_raster_metadata_and_bounds_are_rejected_atomically() {
        let mut store = AssetStore::default();
        let valid = raster_input(PixelFormat::Grayscale8, 2, 1, vec![1, 2]);
        let before = store.usage();

        let mut wrong_stride = valid.clone();
        wrong_stride.canonical_stride = 3;
        let mut wrong_alpha = valid.clone();
        wrong_alpha.alpha_semantics = AssetAlphaSemantics::Straight;
        let mut wrong_color = valid.clone();
        wrong_color.color_space = Some(AssetColorSpace::Srgb);
        let mut wrong_length = valid.clone();
        wrong_length.pixels.pop();
        let mut zero_width = valid.clone();
        zero_width.width = 0;
        let too_large = raster_input(
            PixelFormat::StraightRgba16,
            MAX_RASTER_DIMENSION,
            65,
            Vec::new(),
        );
        let invalid_binary = raster_input(PixelFormat::BinaryMask8, 1, 1, vec![1]);
        let premultiplied = RasterAssetInput {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::PremultipliedBgra8,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: 4,
            pixels: vec![0; 4],
            expected_id: None,
        };

        for malformed in [
            wrong_stride,
            wrong_alpha,
            wrong_color,
            wrong_length,
            zero_width,
            too_large,
            invalid_binary,
            premultiplied,
        ] {
            assert!(store.ingest_raster(malformed).is_err());
            assert_eq!(store.usage(), before);
        }
    }

    #[test]
    fn stream_kind_count_payload_and_total_bounds_are_enforced() {
        let mut store = AssetStore::default();
        let stream = CanonicalStreamInput {
            kind: AssetKind::CanonicalSampleStream,
            element_count: 2,
            payload: vec![1, 2, 3, 4],
            expected_id: None,
        };
        let first = store.ingest_stream(stream.clone()).unwrap();
        let second = store.ingest_stream(stream).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.raster().is_none());

        for malformed in [
            CanonicalStreamInput {
                kind: AssetKind::CanonicalRaster,
                element_count: 1,
                payload: vec![1],
                expected_id: None,
            },
            CanonicalStreamInput {
                kind: AssetKind::CanonicalVectorStream,
                element_count: MAX_STREAM_ELEMENTS + 1,
                payload: vec![1],
                expected_id: None,
            },
            CanonicalStreamInput {
                kind: AssetKind::CanonicalVectorStream,
                element_count: 0,
                payload: vec![1],
                expected_id: None,
            },
            CanonicalStreamInput {
                kind: AssetKind::CanonicalVectorStream,
                element_count: 1,
                payload: Vec::new(),
                expected_id: None,
            },
        ] {
            assert!(store.ingest_stream(malformed).is_err());
        }
        assert_eq!(store.usage().asset_count, 1);

        store.logical_payload_bytes = MAX_TOTAL_ASSET_BYTES;
        assert!(matches!(
            store.ingest_stream(CanonicalStreamInput {
                kind: AssetKind::CanonicalVectorStream,
                element_count: 1,
                payload: vec![9],
                expected_id: None,
            }),
            Err(CoreError::InvalidState(
                "total asset payload limit exceeded"
            ))
        ));
        assert_eq!(store.records.len(), 1);
    }

    #[test]
    fn root_scan_counts_duplicate_owners_and_collects_only_unrooted_assets() {
        let mut store = AssetStore::default();
        let retained = store
            .ingest_stream(CanonicalStreamInput {
                kind: AssetKind::CanonicalVectorStream,
                element_count: 1,
                payload: vec![1],
                expected_id: None,
            })
            .unwrap();
        let collected = store
            .ingest_stream(CanonicalStreamInput {
                kind: AssetKind::CanonicalVectorStream,
                element_count: 1,
                payload: vec![2],
                expected_id: None,
            })
            .unwrap();
        assert_eq!(
            store
                .garbage_collect([retained.id(), retained.id()])
                .unwrap(),
            1
        );
        assert!(store.contains(retained.id()));
        assert!(!store.contains(collected.id()));
        assert_eq!(store.reference_count(retained.id()), 2);
        assert_eq!(store.info(retained.id()).unwrap().reference_count, 2);
        assert_eq!(store.infos().len(), 1);
        assert_eq!(
            store.usage(),
            AssetStoreUsage {
                asset_count: 1,
                logical_payload_bytes: 1,
                referenced_asset_count: 1,
                total_reference_count: 2,
            }
        );

        let before = store.usage();
        assert!(
            store
                .garbage_collect([AssetId::from_bytes([0x55; 32])])
                .is_err()
        );
        assert_eq!(store.usage(), before);
    }

    #[test]
    fn detached_archive_round_trip_reingests_without_shared_allocations() {
        let mut source = AssetStore::default();
        let raster = source
            .ingest_raster(raster_input(
                PixelFormat::StraightRgba8,
                2,
                1,
                vec![1, 2, 3, 4, 5, 6, 7, 8],
            ))
            .unwrap();
        let stream = source
            .ingest_stream(CanonicalStreamInput {
                kind: AssetKind::CanonicalSampleStream,
                element_count: 2,
                payload: vec![9, 10, 11, 12],
                expected_id: None,
            })
            .unwrap();
        let unrooted = source
            .ingest_stream(CanonicalStreamInput {
                kind: AssetKind::CanonicalVectorStream,
                element_count: 1,
                payload: vec![13],
                expected_id: None,
            })
            .unwrap();

        let detached = source
            .detached_archive_round_trip([stream.id(), raster.id(), raster.id()])
            .unwrap();
        let mut expected_ids = vec![raster.id(), stream.id()];
        expected_ids.sort_unstable();
        assert_eq!(
            detached
                .infos()
                .into_iter()
                .map(|info| info.id)
                .collect::<Vec<_>>(),
            expected_ids
        );
        assert!(!detached.contains(unrooted.id()));
        assert_eq!(detached.reference_count(raster.id()), 2);
        assert_eq!(detached.reference_count(stream.id()), 1);

        for original in [raster, stream] {
            let copied = detached.get(original.id()).unwrap();
            assert_eq!(copied.id(), original.id());
            assert_eq!(copied.descriptor(), original.descriptor());
            assert_eq!(copied.payload(), original.payload());
            assert!(!Arc::ptr_eq(&copied, &original));
            assert!(!Arc::ptr_eq(&copied.payload, &original.payload));
            match (copied.raster(), original.raster()) {
                (Some(copied), Some(original)) => assert!(!Arc::ptr_eq(copied, original)),
                (None, None) => {}
                _ => panic!("detached asset changed raster materialization kind"),
            }
        }
    }

    #[test]
    fn tiled_raster_ingestion_reconstructs_dense_rows_without_padding() {
        let mut raster = TileRaster::new(65, 2, PixelFormat::Grayscale16).unwrap();
        raster
            .set_pixel(64, 1, PixelValue::Grayscale16(0x1234), 77)
            .unwrap();
        let input = RasterAssetInput::from_tile_raster(&raster, None).unwrap();
        assert_eq!(input.canonical_stride, 130);
        assert_eq!(input.pixels.len(), 260);
        assert_eq!(&input.pixels[258..], &0x1234_u16.to_le_bytes());

        let mut store = AssetStore::default();
        let record = store.ingest_tile_raster(&raster, None).unwrap();
        assert_eq!(
            record.raster().unwrap().pixel(64, 1).unwrap(),
            PixelValue::Grayscale16(0x1234)
        );
        assert_eq!(record.descriptor().canonical_stride, Some(130));
        assert_eq!(store.get(record.id()).unwrap().id(), record.id());
    }
}
