use super::catalog::CatalogAssetMetadata;
use crate::asset::{AssetRecord, AssetStore, RasterAssetInput};
use crate::{
    AssetAlphaSemantics, AssetColorSpace, AssetDescriptor, AssetId, AssetKind, CoreError,
    MAX_RASTER_DIMENSION, PixelFormat, TileRaster,
};
use inkpod_format::{
    InkScriptTypedAsset, InkScriptTypedValue, InkScriptTypedValueKind,
    MAX_INKSCRIPT_ASSET_TOTAL_BYTES, MAX_INKSCRIPT_CONTAINER_ELEMENTS,
    MAX_INKSCRIPT_EXTERNAL_ASSET_BYTES, MAX_INKSCRIPT_INLINE_ASSET_BYTES,
    MAX_INKSCRIPT_INLINE_ASSET_TOTAL_BYTES,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const ASSET_READ_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct AuthorizedAssetIdentity {
    object: [u8; 32],
    generation: u64,
    logical_length: u64,
}

impl AuthorizedAssetIdentity {
    pub const fn new(object: [u8; 32], generation: u64, logical_length: u64) -> Self {
        Self {
            object,
            generation,
            logical_length,
        }
    }

    pub const fn object(self) -> [u8; 32] {
        self.object
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }

    pub const fn logical_length(self) -> u64 {
        self.logical_length
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum AuthorizedAssetReadError {
    IdentityUnavailable,
    ReadFailed,
}

/// A caller-owned reader used only on the planning owner thread.
///
/// Identity observations must describe the same already-authorized object before and after the
/// bounded read. Core never interprets an OS handle, authority token, or filesystem path.
#[doc(hidden)]
pub trait AuthorizedAssetReader: Send {
    fn observe_identity(&mut self) -> Result<AuthorizedAssetIdentity, AuthorizedAssetReadError>;
    fn read_chunk(&mut self, target: &mut [u8]) -> Result<usize, AuthorizedAssetReadError>;
}

#[doc(hidden)]
pub struct AuthorizedAssetStream<'reader> {
    asset_symbol: &'reader str,
    authorized_identity: AuthorizedAssetIdentity,
    reader: &'reader mut dyn AuthorizedAssetReader,
}

impl<'reader> AuthorizedAssetStream<'reader> {
    pub fn new(
        asset_symbol: &'reader str,
        authorized_identity: AuthorizedAssetIdentity,
        reader: &'reader mut dyn AuthorizedAssetReader,
    ) -> Self {
        Self {
            asset_symbol,
            authorized_identity,
            reader,
        }
    }

    pub(super) const fn asset_symbol(&self) -> &str {
        self.asset_symbol
    }

    pub(super) const fn authorized_identity(&self) -> AuthorizedAssetIdentity {
        self.authorized_identity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScriptAssetLimits {
    asset_count: u64,
    inline_asset_bytes: u64,
    inline_asset_total_bytes: u64,
    external_asset_bytes: u64,
    asset_total_bytes: u64,
}

impl ScriptAssetLimits {
    pub(crate) const fn exact_current() -> Self {
        Self {
            asset_count: MAX_INKSCRIPT_CONTAINER_ELEMENTS as u64,
            inline_asset_bytes: MAX_INKSCRIPT_INLINE_ASSET_BYTES as u64,
            inline_asset_total_bytes: MAX_INKSCRIPT_INLINE_ASSET_TOTAL_BYTES,
            external_asset_bytes: MAX_INKSCRIPT_EXTERNAL_ASSET_BYTES,
            asset_total_bytes: MAX_INKSCRIPT_ASSET_TOTAL_BYTES,
        }
    }

    pub(crate) const fn with_external_asset_bytes(mut self, maximum: u64) -> Self {
        self.external_asset_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_EXTERNAL_ASSET_BYTES);
        self
    }

    pub(crate) const fn with_inline_asset_bytes(mut self, maximum: u64) -> Self {
        self.inline_asset_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_INLINE_ASSET_BYTES as u64);
        self
    }

    pub(crate) const fn with_asset_total_bytes(mut self, maximum: u64) -> Self {
        self.asset_total_bytes = minimum_nonzero(maximum, MAX_INKSCRIPT_ASSET_TOTAL_BYTES);
        self
    }
}

const fn minimum_nonzero(requested: u64, exact: u64) -> u64 {
    if requested == 0 {
        1
    } else if requested < exact {
        requested
    } else {
        exact
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable failure categories produced while validating or freezing InkScript assets.
pub enum ScriptAssetError {
    /// The typed source model does not match the exact-current asset schema.
    InvalidTypedModel,
    /// An asset descriptor is invalid.
    InvalidDescriptor,
    /// An inline or authorized payload is invalid.
    InvalidPayload,
    /// The payload digest does not match its descriptor.
    DigestMismatch,
    /// The payload length does not match its descriptor.
    LengthMismatch,
    /// Duplicate declarations disagree about one content-addressed asset.
    DuplicateDescriptorMismatch,
    /// More than one authorized stream was supplied for one asset.
    DuplicateAuthorizedStream,
    /// A required authorized stream was not supplied.
    MissingAuthorizedStream,
    /// A stream was supplied for an inline-only or undeclared asset.
    UnexpectedAuthorizedStream,
    /// The authorized stream identity changed after authority was granted.
    StaleAuthorizedStream,
    /// The authorized stream identity could not be validated.
    StreamIdentityFailed,
    /// Reading an authorized asset failed.
    StreamReadFailed,
    /// The authorized stream ended before its declared length.
    Truncated,
    /// The caller cancelled before the immutable asset set was published.
    Cancelled,
    /// A declared or aggregate asset bound was exceeded.
    ResourceLimit,
    /// A step referenced an undeclared asset.
    UnknownAsset,
    /// An asset does not satisfy the command's catalog role.
    RoleMismatch,
    /// The command catalog does not permit the supplied source form.
    SourceNotPermitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScriptAssetSource {
    Inline,
    Authorized,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScriptAssetUsage {
    pub(crate) declaration_count: u64,
    pub(crate) unique_asset_count: u64,
    pub(crate) logical_payload_bytes: u64,
    pub(crate) unique_logical_payload_bytes: u64,
    pub(crate) inline_decoded_bytes: u64,
    pub(crate) authorized_read_bytes: u64,
    pub(crate) payload_copy_bytes: u64,
}

#[derive(Clone, Debug)]
struct FrozenScriptAsset {
    record: Arc<AssetRecord>,
    source: ScriptAssetSource,
    authorized_identity: Option<AuthorizedAssetIdentity>,
}

#[derive(Clone, Debug)]
pub(crate) struct FrozenScriptAssets {
    store: AssetStore,
    by_symbol: BTreeMap<String, FrozenScriptAsset>,
    usage: ScriptAssetUsage,
}

impl FrozenScriptAssets {
    pub(crate) fn asset_id(&self, symbol: &str) -> Option<AssetId> {
        self.by_symbol.get(symbol).map(|asset| asset.record.id())
    }

    pub(crate) fn logical_bytes(&self, symbol: &str) -> Option<&[u8]> {
        self.by_symbol
            .get(symbol)
            .map(|asset| asset.record.payload())
    }

    pub(crate) fn raster_input(&self, symbol: &str) -> Result<RasterAssetInput, ScriptAssetError> {
        let asset = self
            .by_symbol
            .get(symbol)
            .ok_or(ScriptAssetError::UnknownAsset)?;
        let descriptor = asset.record.descriptor();
        if descriptor.kind != AssetKind::CanonicalRaster {
            return Err(ScriptAssetError::RoleMismatch);
        }
        let width = descriptor
            .width
            .ok_or(ScriptAssetError::InvalidDescriptor)?;
        let height = descriptor
            .height
            .ok_or(ScriptAssetError::InvalidDescriptor)?;
        let pixel_format = descriptor
            .pixel_format
            .ok_or(ScriptAssetError::InvalidDescriptor)?;
        let canonical_stride = descriptor
            .canonical_stride
            .ok_or(ScriptAssetError::InvalidDescriptor)?;
        let alpha_semantics = descriptor
            .alpha_semantics
            .ok_or(ScriptAssetError::InvalidDescriptor)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(asset.record.payload().len())
            .map_err(|_| ScriptAssetError::ResourceLimit)?;
        pixels.extend_from_slice(asset.record.payload());
        Ok(RasterAssetInput {
            width,
            height,
            pixel_format,
            color_space: descriptor.color_space,
            alpha_semantics,
            canonical_stride,
            pixels,
            expected_id: Some(asset.record.id()),
        })
    }

    pub(crate) fn raster(&self, symbol: &str) -> Result<&TileRaster, ScriptAssetError> {
        self.by_symbol
            .get(symbol)
            .ok_or(ScriptAssetError::UnknownAsset)?
            .record
            .raster()
            .map(AsRef::as_ref)
            .ok_or(ScriptAssetError::RoleMismatch)
    }

    pub(crate) fn raster_record(&self, symbol: &str) -> Result<Arc<AssetRecord>, ScriptAssetError> {
        let asset = self
            .by_symbol
            .get(symbol)
            .ok_or(ScriptAssetError::UnknownAsset)?;
        if asset.record.raster().is_none() {
            return Err(ScriptAssetError::RoleMismatch);
        }
        Ok(Arc::clone(&asset.record))
    }

    pub(crate) const fn usage(&self) -> ScriptAssetUsage {
        self.usage
    }

    pub(super) fn plan_records(&self) -> Vec<FrozenAssetPlanRecord> {
        self.by_symbol
            .iter()
            .map(|(symbol, asset)| FrozenAssetPlanRecord {
                symbol: symbol.clone(),
                asset_id: asset.record.id(),
                descriptor: asset.record.descriptor(),
                source: asset.source,
                authorized_identity: asset.authorized_identity,
            })
            .collect()
    }

    pub(crate) fn bind_role(
        &self,
        role: &CatalogAssetMetadata,
        symbol: &str,
    ) -> Result<ScriptAssetRolePlan, ScriptAssetError> {
        let asset = self
            .by_symbol
            .get(symbol)
            .ok_or(ScriptAssetError::UnknownAsset)?;
        let descriptor = asset.record.descriptor();
        if role.kind != asset_kind_name(descriptor.kind) {
            return Err(ScriptAssetError::RoleMismatch);
        }
        let permitted = match asset.source {
            ScriptAssetSource::Inline => role.inline,
            ScriptAssetSource::Authorized => role.external,
        };
        if !permitted {
            return Err(ScriptAssetError::SourceNotPermitted);
        }
        Ok(ScriptAssetRolePlan {
            role_name: role.name,
            asset_id: asset.record.id(),
            descriptor,
            authorized_identity: asset.authorized_identity,
            summary: ScriptAssetSummary {
                kind: role.kind,
                source: asset.source,
                logical_payload_bytes: descriptor.logical_payload_length,
            },
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FrozenAssetPlanRecord {
    pub(super) symbol: String,
    pub(super) asset_id: AssetId,
    pub(super) descriptor: AssetDescriptor,
    pub(super) source: ScriptAssetSource,
    pub(super) authorized_identity: Option<AuthorizedAssetIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScriptAssetSummary {
    pub(crate) kind: &'static str,
    pub(crate) source: ScriptAssetSource,
    pub(crate) logical_payload_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScriptAssetRolePlan {
    pub(crate) role_name: &'static str,
    pub(crate) asset_id: AssetId,
    pub(crate) descriptor: AssetDescriptor,
    pub(crate) authorized_identity: Option<AuthorizedAssetIdentity>,
    pub(crate) summary: ScriptAssetSummary,
}

#[derive(Clone, Copy)]
enum PreparedPayload<'asset> {
    Inline(&'asset [u8]),
    Authorized,
}

#[derive(Clone, Copy)]
struct PreparedAsset<'asset> {
    symbol: &'asset str,
    expected_id: AssetId,
    descriptor: AssetDescriptor,
    payload: PreparedPayload<'asset>,
}

pub(crate) fn freeze_inkscript_assets(
    declarations: &[InkScriptTypedAsset],
    streams: &mut [AuthorizedAssetStream<'_>],
    limits: ScriptAssetLimits,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<FrozenScriptAssets, ScriptAssetError> {
    if declarations.is_empty() {
        if streams.is_empty() {
            return Ok(FrozenScriptAssets {
                store: AssetStore::default(),
                by_symbol: BTreeMap::new(),
                usage: ScriptAssetUsage::default(),
            });
        }
        return Err(ScriptAssetError::UnexpectedAuthorizedStream);
    }
    poll_cancel(is_cancelled)?;
    let stream_indices = stream_indices(streams)?;
    let prepared = prepare_assets(declarations, streams, &stream_indices, limits, is_cancelled)?;

    let mut store = AssetStore::default();
    let mut by_symbol = BTreeMap::new();
    let mut usage = ScriptAssetUsage {
        declaration_count: prepared.len() as u64,
        ..ScriptAssetUsage::default()
    };
    for asset in prepared {
        poll_cancel(is_cancelled)?;
        let (payload, source, authorized_identity, read_bytes, copy_bytes) = match asset.payload {
            PreparedPayload::Inline(bytes) => (
                copy_inline_payload(bytes, is_cancelled)?,
                ScriptAssetSource::Inline,
                None,
                0,
                bytes.len() as u64,
            ),
            PreparedPayload::Authorized => {
                let index = *stream_indices
                    .get(asset.symbol)
                    .ok_or(ScriptAssetError::MissingAuthorizedStream)?;
                let stream = &mut streams[index];
                let bytes = read_authorized_payload(
                    stream,
                    asset.descriptor.logical_payload_length,
                    is_cancelled,
                )?;
                let count = bytes.len() as u64;
                (
                    bytes,
                    ScriptAssetSource::Authorized,
                    Some(stream.authorized_identity),
                    count,
                    count,
                )
            }
        };
        usage.authorized_read_bytes = checked_add(usage.authorized_read_bytes, read_bytes)?;
        usage.payload_copy_bytes = checked_add(usage.payload_copy_bytes, copy_bytes)?;
        let record = ingest_prepared(&mut store, asset, payload)?;
        if by_symbol
            .insert(
                asset.symbol.to_owned(),
                FrozenScriptAsset {
                    record,
                    source,
                    authorized_identity,
                },
            )
            .is_some()
        {
            return Err(ScriptAssetError::InvalidTypedModel);
        }
        poll_cancel(is_cancelled)?;
    }
    let store_usage = store.usage();
    usage.unique_asset_count = store_usage.asset_count;
    usage.unique_logical_payload_bytes = store_usage.logical_payload_bytes;
    usage.logical_payload_bytes = by_symbol.values().try_fold(0_u64, |total, asset| {
        checked_add(total, asset.record.descriptor().logical_payload_length)
    })?;
    usage.inline_decoded_bytes = by_symbol.values().try_fold(0_u64, |total, asset| {
        if asset.source == ScriptAssetSource::Inline {
            checked_add(total, asset.record.descriptor().logical_payload_length)
        } else {
            Ok(total)
        }
    })?;
    Ok(FrozenScriptAssets {
        store,
        by_symbol,
        usage,
    })
}

fn stream_indices(
    streams: &[AuthorizedAssetStream<'_>],
) -> Result<BTreeMap<String, usize>, ScriptAssetError> {
    let mut indices = BTreeMap::new();
    for (index, stream) in streams.iter().enumerate() {
        if indices
            .insert(stream.asset_symbol.to_owned(), index)
            .is_some()
        {
            return Err(ScriptAssetError::DuplicateAuthorizedStream);
        }
    }
    Ok(indices)
}

fn prepare_assets<'asset>(
    declarations: &'asset [InkScriptTypedAsset],
    streams: &[AuthorizedAssetStream<'_>],
    stream_indices: &BTreeMap<String, usize>,
    limits: ScriptAssetLimits,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<Vec<PreparedAsset<'asset>>, ScriptAssetError> {
    if declarations.len() as u64 > limits.asset_count {
        return Err(ScriptAssetError::ResourceLimit);
    }
    let mut prepared = Vec::new();
    prepared
        .try_reserve_exact(declarations.len())
        .map_err(|_| ScriptAssetError::ResourceLimit)?;
    let mut inline_total = 0_u64;
    let mut asset_total = 0_u64;
    let mut descriptors = BTreeMap::<AssetId, AssetDescriptor>::new();
    let mut expected_streams = BTreeMap::<&str, ()>::new();
    for declaration in declarations {
        poll_cancel(is_cancelled)?;
        let asset = prepare_asset(declaration)?;
        if let Some(existing) = descriptors.insert(asset.expected_id, asset.descriptor)
            && existing != asset.descriptor
        {
            return Err(ScriptAssetError::DuplicateDescriptorMismatch);
        }
        let length = asset.descriptor.logical_payload_length;
        match asset.payload {
            PreparedPayload::Inline(bytes) => {
                if bytes.len() as u64 != length || length > limits.inline_asset_bytes {
                    return Err(if bytes.len() as u64 != length {
                        ScriptAssetError::LengthMismatch
                    } else {
                        ScriptAssetError::ResourceLimit
                    });
                }
                inline_total = checked_add(inline_total, length)?;
                if inline_total > limits.inline_asset_total_bytes {
                    return Err(ScriptAssetError::ResourceLimit);
                }
            }
            PreparedPayload::Authorized => {
                let stream = stream_indices
                    .get(asset.symbol)
                    .and_then(|index| streams.get(*index))
                    .ok_or(ScriptAssetError::MissingAuthorizedStream)?;
                expected_streams.insert(asset.symbol, ());
                if stream.authorized_identity.logical_length != length {
                    return Err(ScriptAssetError::LengthMismatch);
                }
                if length > limits.external_asset_bytes {
                    return Err(ScriptAssetError::ResourceLimit);
                }
            }
        }
        asset_total = checked_add(asset_total, length)?;
        if asset_total > limits.asset_total_bytes {
            return Err(ScriptAssetError::ResourceLimit);
        }
        prepared.push(asset);
    }
    if streams
        .iter()
        .any(|stream| !expected_streams.contains_key(stream.asset_symbol))
    {
        return Err(ScriptAssetError::UnexpectedAuthorizedStream);
    }
    Ok(prepared)
}

fn prepare_asset(asset: &InkScriptTypedAsset) -> Result<PreparedAsset<'_>, ScriptAssetError> {
    let body = typed_record(asset.body(), "canonical_raster_asset")?;
    if typed_string(required_field(body, "kind")?)? != "canonical_raster" {
        return Err(ScriptAssetError::InvalidTypedModel);
    }
    let expected_id = AssetId::from_bytes(decode_digest(typed_digest(required_field(
        body, "asset_id",
    )?)?)?);
    let descriptor = canonical_raster_descriptor(required_field(body, "descriptor")?)?;
    let data = optional_base64(body.get("data"))?;
    let data_file = optional_string(body.get("data_file"))?;
    let payload = match (data, data_file) {
        (Some(bytes), None) => PreparedPayload::Inline(bytes),
        (None, Some(_)) => PreparedPayload::Authorized,
        _ => return Err(ScriptAssetError::InvalidTypedModel),
    };
    Ok(PreparedAsset {
        symbol: asset.name(),
        expected_id,
        descriptor,
        payload,
    })
}

pub(super) fn external_asset_path(
    asset: &InkScriptTypedAsset,
) -> Result<Option<&str>, ScriptAssetError> {
    let body = typed_record(asset.body(), "canonical_raster_asset")?;
    optional_string(body.get("data_file"))
}

fn canonical_raster_descriptor(
    value: &InkScriptTypedValue,
) -> Result<AssetDescriptor, ScriptAssetError> {
    let values = typed_record(value, "canonical_raster_descriptor")?;
    let pixel_format = match typed_enum(required_field(values, "pixel_format")?)? {
        "mask8" => PixelFormat::BinaryMask8,
        "gray8" => PixelFormat::Grayscale8,
        "gray16" => PixelFormat::Grayscale16,
        "rgba8" => PixelFormat::StraightRgba8,
        "rgba16" => PixelFormat::StraightRgba16,
        _ => return Err(ScriptAssetError::InvalidDescriptor),
    };
    if typed_enum(required_field(values, "color_space")?)? != "srgb"
        || typed_enum(required_field(values, "alpha")?)? != "straight"
    {
        return Err(ScriptAssetError::InvalidDescriptor);
    }
    let width = typed_u32(required_field(values, "width")?)?;
    let height = typed_u32(required_field(values, "height")?)?;
    if width == 0 || height == 0 || width > MAX_RASTER_DIMENSION || height > MAX_RASTER_DIMENSION {
        return Err(ScriptAssetError::InvalidDescriptor);
    }
    let stride = u64::from(typed_u32(required_field(values, "stride")?)?);
    let expected_stride = u64::from(width)
        .checked_mul(pixel_format.bytes_per_pixel() as u64)
        .ok_or(ScriptAssetError::InvalidDescriptor)?;
    if stride != expected_stride {
        return Err(ScriptAssetError::InvalidDescriptor);
    }
    let element_count = typed_u64(required_field(values, "element_count")?)?;
    let expected_elements = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(ScriptAssetError::InvalidDescriptor)?;
    if element_count != expected_elements {
        return Err(ScriptAssetError::InvalidDescriptor);
    }
    let logical_payload_length = stride
        .checked_mul(u64::from(height))
        .ok_or(ScriptAssetError::InvalidDescriptor)?;
    let (color_space, alpha_semantics) = match pixel_format {
        PixelFormat::BinaryMask8 => (None, AssetAlphaSemantics::CoverageMask),
        PixelFormat::Grayscale8 | PixelFormat::Grayscale16 => (None, AssetAlphaSemantics::Opaque),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16 => {
            (Some(AssetColorSpace::Srgb), AssetAlphaSemantics::Straight)
        }
        PixelFormat::PremultipliedBgra8 => return Err(ScriptAssetError::InvalidDescriptor),
    };
    Ok(AssetDescriptor {
        kind: AssetKind::CanonicalRaster,
        pixel_format: Some(pixel_format),
        color_space,
        alpha_semantics: Some(alpha_semantics),
        width: Some(width),
        height: Some(height),
        canonical_stride: Some(stride),
        logical_element_count: element_count,
        logical_payload_length,
    })
}

fn copy_inline_payload(
    source: &[u8],
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<Vec<u8>, ScriptAssetError> {
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(source.len())
        .map_err(|_| ScriptAssetError::ResourceLimit)?;
    for chunk in source.chunks(ASSET_READ_CHUNK_BYTES) {
        poll_cancel(is_cancelled)?;
        payload.extend_from_slice(chunk);
    }
    Ok(payload)
}

fn read_authorized_payload(
    stream: &mut AuthorizedAssetStream<'_>,
    length: u64,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<Vec<u8>, ScriptAssetError> {
    poll_cancel(is_cancelled)?;
    let before = stream
        .reader
        .observe_identity()
        .map_err(|_| ScriptAssetError::StreamIdentityFailed)?;
    if before != stream.authorized_identity {
        return Err(ScriptAssetError::StaleAuthorizedStream);
    }
    let length = usize::try_from(length).map_err(|_| ScriptAssetError::ResourceLimit)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(length)
        .map_err(|_| ScriptAssetError::ResourceLimit)?;
    let mut chunk = [0_u8; ASSET_READ_CHUNK_BYTES];
    while payload.len() < length {
        poll_cancel(is_cancelled)?;
        let remaining = length - payload.len();
        let requested = remaining.min(chunk.len());
        let count =
            stream
                .reader
                .read_chunk(&mut chunk[..requested])
                .map_err(|error| match error {
                    AuthorizedAssetReadError::IdentityUnavailable => {
                        ScriptAssetError::StreamIdentityFailed
                    }
                    AuthorizedAssetReadError::ReadFailed => ScriptAssetError::StreamReadFailed,
                })?;
        if count == 0 {
            return Err(ScriptAssetError::Truncated);
        }
        if count > requested {
            return Err(ScriptAssetError::StreamReadFailed);
        }
        payload.extend_from_slice(&chunk[..count]);
    }
    poll_cancel(is_cancelled)?;
    let mut extra = [0_u8; 1];
    if stream
        .reader
        .read_chunk(&mut extra)
        .map_err(|_| ScriptAssetError::StreamReadFailed)?
        != 0
    {
        return Err(ScriptAssetError::LengthMismatch);
    }
    let after = stream
        .reader
        .observe_identity()
        .map_err(|_| ScriptAssetError::StreamIdentityFailed)?;
    if after != stream.authorized_identity {
        return Err(ScriptAssetError::StaleAuthorizedStream);
    }
    Ok(payload)
}

fn ingest_prepared(
    store: &mut AssetStore,
    asset: PreparedAsset<'_>,
    payload: Vec<u8>,
) -> Result<Arc<AssetRecord>, ScriptAssetError> {
    let descriptor = asset.descriptor;
    let record = store
        .ingest_raster(RasterAssetInput {
            width: descriptor
                .width
                .ok_or(ScriptAssetError::InvalidDescriptor)?,
            height: descriptor
                .height
                .ok_or(ScriptAssetError::InvalidDescriptor)?,
            pixel_format: descriptor
                .pixel_format
                .ok_or(ScriptAssetError::InvalidDescriptor)?,
            color_space: descriptor.color_space,
            alpha_semantics: descriptor
                .alpha_semantics
                .ok_or(ScriptAssetError::InvalidDescriptor)?,
            canonical_stride: descriptor
                .canonical_stride
                .ok_or(ScriptAssetError::InvalidDescriptor)?,
            pixels: payload,
            expected_id: None,
        })
        .map_err(map_store_error)?;
    if record.descriptor() != descriptor {
        return Err(ScriptAssetError::InvalidDescriptor);
    }
    if record.id() != asset.expected_id {
        return Err(ScriptAssetError::DigestMismatch);
    }
    Ok(record)
}

fn map_store_error(error: CoreError) -> ScriptAssetError {
    match error {
        CoreError::InvalidArgument(_) => ScriptAssetError::InvalidPayload,
        CoreError::InvalidState(_) | CoreError::Raster(_) => ScriptAssetError::ResourceLimit,
        _ => ScriptAssetError::InvalidPayload,
    }
}

fn typed_record<'value>(
    value: &'value InkScriptTypedValue,
    expected_type: &str,
) -> Result<&'value BTreeMap<String, InkScriptTypedValue>, ScriptAssetError> {
    if value.type_name() != expected_type {
        return Err(ScriptAssetError::InvalidTypedModel);
    }
    match value.kind() {
        InkScriptTypedValueKind::Record(values) => Ok(values),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn required_field<'value>(
    values: &'value BTreeMap<String, InkScriptTypedValue>,
    name: &str,
) -> Result<&'value InkScriptTypedValue, ScriptAssetError> {
    values.get(name).ok_or(ScriptAssetError::InvalidTypedModel)
}

fn typed_string(value: &InkScriptTypedValue) -> Result<&str, ScriptAssetError> {
    match value.kind() {
        InkScriptTypedValueKind::String(value) => Ok(value),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn typed_enum(value: &InkScriptTypedValue) -> Result<&str, ScriptAssetError> {
    match value.kind() {
        InkScriptTypedValueKind::Enum(value) => Ok(value),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn typed_digest(value: &InkScriptTypedValue) -> Result<&str, ScriptAssetError> {
    match value.kind() {
        InkScriptTypedValueKind::Digest(value) => Ok(value),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn typed_u32(value: &InkScriptTypedValue) -> Result<u32, ScriptAssetError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(*value),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn typed_u64(value: &InkScriptTypedValue) -> Result<u64, ScriptAssetError> {
    match value.kind() {
        InkScriptTypedValueKind::U64(value) => Ok(*value),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn optional_base64(value: Option<&InkScriptTypedValue>) -> Result<Option<&[u8]>, ScriptAssetError> {
    match value.map(InkScriptTypedValue::kind) {
        None | Some(InkScriptTypedValueKind::None) => Ok(None),
        Some(InkScriptTypedValueKind::Base64(value)) => Ok(Some(value)),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn optional_string(value: Option<&InkScriptTypedValue>) -> Result<Option<&str>, ScriptAssetError> {
    match value.map(InkScriptTypedValue::kind) {
        None | Some(InkScriptTypedValueKind::None) => Ok(None),
        Some(InkScriptTypedValueKind::String(value)) => Ok(Some(value)),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn decode_digest(value: &str) -> Result<[u8; 32], ScriptAssetError> {
    if value.len() != 64 {
        return Err(ScriptAssetError::InvalidTypedModel);
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = decode_hex(pair[0])?
            .checked_mul(16)
            .and_then(|high| high.checked_add(decode_hex(pair[1]).ok()?))
            .ok_or(ScriptAssetError::InvalidTypedModel)?;
    }
    Ok(bytes)
}

fn decode_hex(value: u8) -> Result<u8, ScriptAssetError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(ScriptAssetError::InvalidTypedModel),
    }
}

fn checked_add(left: u64, right: u64) -> Result<u64, ScriptAssetError> {
    left.checked_add(right)
        .ok_or(ScriptAssetError::ResourceLimit)
}

fn poll_cancel(is_cancelled: &mut dyn FnMut() -> bool) -> Result<(), ScriptAssetError> {
    if is_cancelled() {
        Err(ScriptAssetError::Cancelled)
    } else {
        Ok(())
    }
}

const fn asset_kind_name(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::CanonicalRaster => "canonical_raster",
        AssetKind::CanonicalSampleStream => "canonical_sample_stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetStore, RasterAssetInput};
    use crate::{AssetAlphaSemantics, AssetColorSpace, PixelFormat};
    use inkpod_format::{
        InkScriptDeclarationModel, InkScriptSchemaView, InkScriptSource, InkScriptSourceId,
        InkScriptTypeDiagnosticCode, build_inkscript_declaration_model, parse_inkscript,
    };

    fn digest_text(id: crate::AssetId) -> String {
        let mut text = String::with_capacity(64);
        for byte in id.as_bytes() {
            use std::fmt::Write as _;
            write!(text, "{byte:02x}").unwrap();
        }
        text
    }

    fn direct_rgba_asset(payload: Vec<u8>, width: u32, height: u32) -> crate::AssetId {
        let mut store = AssetStore::default();
        store
            .ingest_raster(RasterAssetInput {
                width,
                height,
                pixel_format: PixelFormat::StraightRgba8,
                color_space: Some(AssetColorSpace::Srgb),
                alpha_semantics: AssetAlphaSemantics::Straight,
                canonical_stride: u64::from(width) * 4,
                pixels: payload,
                expected_id: None,
            })
            .unwrap()
            .id()
    }

    fn complete_model(
        assets: &str,
    ) -> Result<InkScriptDeclarationModel, InkScriptTypeDiagnosticCode> {
        let text = format!(
            r#"inkscript 2;
requires {{ procedure_catalog = 4; replay_epoch = 25; }}
inputs {{ current_document; }}
program {{}}
output {{ policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "asset"; start_number = 1; direction = ascending; }}
execution {{ failure = stop; wait_ms = 0; preview_before_save = false; }}
assets {{ {assets} }}
"#
        );
        let source = InkScriptSource::new(InkScriptSourceId::new(110), text.as_bytes()).unwrap();
        let parsed = parse_inkscript(&source);
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
        let schema = InkScriptSchemaView::exact_current(&[], &[]).unwrap();
        build_inkscript_declaration_model(&parsed, &schema).map_err(|error| error.code())
    }

    fn inline_asset(name: &str, id: crate::AssetId, width: u32, height: u32, data: &str) -> String {
        format!(
            r#"asset {name} {{
                asset_id = blake3"{}";
                kind = "canonical_raster";
                descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = {width}; height = {height}; stride = {}; element_count = {}; }};
                data = base64"""{data}""";
            }};"#,
            digest_text(id),
            width * 4,
            u64::from(width) * u64::from(height),
        )
    }

    fn external_asset(name: &str, id: crate::AssetId, width: u32, height: u32) -> String {
        format!(
            r#"asset {name} {{
                asset_id = blake3"{}";
                kind = "canonical_raster";
                descriptor = {{ pixel_format = rgba8; color_space = srgb; alpha = straight; width = {width}; height = {height}; stride = {}; element_count = {}; }};
                data_file = "caller-resolves-this";
            }};"#,
            digest_text(id),
            width * 4,
            u64::from(width) * u64::from(height),
        )
    }

    #[derive(Debug)]
    struct MemoryReader {
        bytes: Vec<u8>,
        cursor: usize,
        before: AuthorizedAssetIdentity,
        after: AuthorizedAssetIdentity,
        identity_checks: u32,
        fail_at: Option<usize>,
        invalid_count: bool,
    }

    impl MemoryReader {
        fn stable(bytes: Vec<u8>, identity: AuthorizedAssetIdentity) -> Self {
            Self {
                bytes,
                cursor: 0,
                before: identity,
                after: identity,
                identity_checks: 0,
                fail_at: None,
                invalid_count: false,
            }
        }
    }

    impl AuthorizedAssetReader for MemoryReader {
        fn observe_identity(
            &mut self,
        ) -> Result<AuthorizedAssetIdentity, AuthorizedAssetReadError> {
            self.identity_checks += 1;
            Ok(if self.identity_checks == 1 {
                self.before
            } else {
                self.after
            })
        }

        fn read_chunk(&mut self, target: &mut [u8]) -> Result<usize, AuthorizedAssetReadError> {
            if self.fail_at.is_some_and(|offset| self.cursor >= offset) {
                return Err(AuthorizedAssetReadError::ReadFailed);
            }
            if self.invalid_count {
                return Ok(target.len().saturating_add(1));
            }
            let count = target
                .len()
                .min(self.bytes.len().saturating_sub(self.cursor));
            target[..count].copy_from_slice(&self.bytes[self.cursor..self.cursor + count]);
            self.cursor += count;
            Ok(count)
        }
    }

    fn identity(tag: u8, length: u64) -> AuthorizedAssetIdentity {
        AuthorizedAssetIdentity::new([tag; 32], 7, length)
    }

    #[test]
    fn inline_and_authorized_stream_freeze_to_the_same_owned_asset_and_role_plan() {
        let payload = vec![1, 2, 3, 4];
        let id = direct_rgba_asset(payload.clone(), 1, 1);
        let inline_model = complete_model(&inline_asset("paint", id, 1, 1, "AQIDBA==")).unwrap();
        let mut never_cancel = || false;
        let inline = freeze_inkscript_assets(
            inline_model.assets(),
            &mut [],
            ScriptAssetLimits::exact_current(),
            &mut never_cancel,
        )
        .unwrap();

        let external_model = complete_model(&external_asset("paint", id, 1, 1)).unwrap();
        let authorized_identity = identity(9, 4);
        let mut reader = MemoryReader::stable(payload.clone(), authorized_identity);
        let external = {
            let mut streams = [AuthorizedAssetStream::new(
                "paint",
                authorized_identity,
                &mut reader,
            )];
            freeze_inkscript_assets(
                external_model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            )
            .unwrap()
        };

        assert_eq!(inline.asset_id("paint"), Some(id));
        assert_eq!(external.asset_id("paint"), Some(id));
        assert_eq!(inline.logical_bytes("paint"), Some(payload.as_slice()));
        assert_eq!(external.logical_bytes("paint"), Some(payload.as_slice()));
        assert_eq!(inline.usage().declaration_count, 1);
        assert_eq!(inline.usage().unique_asset_count, 1);
        assert_eq!(inline.usage().inline_decoded_bytes, 4);
        assert_eq!(inline.usage().authorized_read_bytes, 0);
        assert_eq!(inline.usage().payload_copy_bytes, 4);
        assert_eq!(external.usage().authorized_read_bytes, 4);
        assert_eq!(external.usage().payload_copy_bytes, 4);
        assert_eq!(reader.identity_checks, 2);

        let role = CatalogAssetMetadata {
            name: "paint_source",
            kind: "canonical_raster",
            inline: true,
            external: true,
        };
        let inline_plan = inline.bind_role(&role, "paint").unwrap();
        let external_plan = external.bind_role(&role, "paint").unwrap();
        assert_eq!(inline_plan.role_name, "paint_source");
        assert_eq!(inline_plan.asset_id, id);
        assert_eq!(inline_plan.summary.source, ScriptAssetSource::Inline);
        assert_eq!(external_plan.summary.source, ScriptAssetSource::Authorized);
        assert_eq!(external_plan.summary.logical_payload_bytes, 4);

        let external_only = CatalogAssetMetadata {
            name: "external_only",
            kind: "canonical_raster",
            inline: false,
            external: true,
        };
        assert_eq!(
            inline.bind_role(&external_only, "paint"),
            Err(ScriptAssetError::SourceNotPermitted)
        );

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FrozenScriptAssets>();
    }

    #[test]
    fn empty_asset_freeze_is_a_noop_and_stream_set_must_match_exactly() {
        let model = complete_model("").unwrap();
        let mut cancelled = || true;
        let frozen = freeze_inkscript_assets(
            model.assets(),
            &mut [],
            ScriptAssetLimits::exact_current(),
            &mut cancelled,
        )
        .unwrap();
        assert_eq!(frozen.usage(), ScriptAssetUsage::default());

        let unexpected_identity = identity(1, 0);
        let mut reader = MemoryReader::stable(Vec::new(), unexpected_identity);
        let mut streams = [AuthorizedAssetStream::new(
            "unexpected",
            unexpected_identity,
            &mut reader,
        )];
        let mut never_cancel = || false;
        assert!(matches!(
            freeze_inkscript_assets(
                model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptAssetError::UnexpectedAuthorizedStream)
        ));
    }

    #[test]
    fn descriptor_digest_and_duplicate_mismatch_fail_without_a_frozen_store() {
        let payload = vec![1, 2, 3, 4];
        let id = direct_rgba_asset(payload, 1, 1);
        let forged = crate::AssetId::from_bytes([0x55; 32]);
        let digest_model =
            complete_model(&inline_asset("paint", forged, 1, 1, "AQIDBA==")).unwrap();
        let mut never_cancel = || false;
        assert!(matches!(
            freeze_inkscript_assets(
                digest_model.assets(),
                &mut [],
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptAssetError::DigestMismatch)
        ));

        let invalid_descriptor =
            inline_asset("paint", id, 1, 1, "AQIDBA==").replace("stride = 4", "stride = 8");
        let invalid_model = complete_model(&invalid_descriptor).unwrap();
        assert!(matches!(
            freeze_inkscript_assets(
                invalid_model.assets(),
                &mut [],
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptAssetError::InvalidDescriptor)
        ));

        let duplicate = format!(
            "{} {}",
            inline_asset("first", id, 1, 1, "AQIDBA=="),
            inline_asset("second", id, 2, 1, "AQIDBAUGBwg=")
        );
        let duplicate_model = complete_model(&duplicate).unwrap();
        assert!(matches!(
            freeze_inkscript_assets(
                duplicate_model.assets(),
                &mut [],
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            ),
            Err(ScriptAssetError::DuplicateDescriptorMismatch)
        ));

        let inline_limited = complete_model(&inline_asset("paint", id, 1, 1, "AQIDBA==")).unwrap();
        assert!(matches!(
            freeze_inkscript_assets(
                inline_limited.assets(),
                &mut [],
                ScriptAssetLimits::exact_current().with_inline_asset_bytes(3),
                &mut never_cancel,
            ),
            Err(ScriptAssetError::ResourceLimit)
        ));

        let second_payload = vec![5, 6, 7, 8];
        let second_id = direct_rgba_asset(second_payload, 1, 1);
        let total_limited = complete_model(&format!(
            "{} {}",
            inline_asset("first", id, 1, 1, "AQIDBA=="),
            inline_asset("second", second_id, 1, 1, "BQYHCA==")
        ))
        .unwrap();
        assert!(matches!(
            freeze_inkscript_assets(
                total_limited.assets(),
                &mut [],
                ScriptAssetLimits::exact_current().with_asset_total_bytes(7),
                &mut never_cancel,
            ),
            Err(ScriptAssetError::ResourceLimit)
        ));
    }

    #[test]
    fn authorized_stream_truncation_stale_cancel_failure_and_limits_are_atomic() {
        let payload = vec![7; 8];
        let id = direct_rgba_asset(payload.clone(), 2, 1);
        let model = complete_model(&external_asset("paint", id, 2, 1)).unwrap();
        let authorized_identity = identity(3, 8);
        let mut never_cancel = || false;

        let mut short = MemoryReader::stable(vec![7; 4], authorized_identity);
        let short_result = {
            let mut streams = [AuthorizedAssetStream::new(
                "paint",
                authorized_identity,
                &mut short,
            )];
            freeze_inkscript_assets(
                model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            )
        };
        assert!(matches!(short_result, Err(ScriptAssetError::Truncated)));

        let mut stale = MemoryReader::stable(payload.clone(), authorized_identity);
        stale.after = AuthorizedAssetIdentity::new([3; 32], 8, 8);
        let stale_result = {
            let mut streams = [AuthorizedAssetStream::new(
                "paint",
                authorized_identity,
                &mut stale,
            )];
            freeze_inkscript_assets(
                model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            )
        };
        assert!(matches!(
            stale_result,
            Err(ScriptAssetError::StaleAuthorizedStream)
        ));

        let mut failed = MemoryReader::stable(payload.clone(), authorized_identity);
        failed.fail_at = Some(0);
        let failed_result = {
            let mut streams = [AuthorizedAssetStream::new(
                "paint",
                authorized_identity,
                &mut failed,
            )];
            freeze_inkscript_assets(
                model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            )
        };
        assert!(matches!(
            failed_result,
            Err(ScriptAssetError::StreamReadFailed)
        ));

        let mut invalid_count = MemoryReader::stable(payload.clone(), authorized_identity);
        invalid_count.invalid_count = true;
        let invalid_count_result = {
            let mut streams = [AuthorizedAssetStream::new(
                "paint",
                authorized_identity,
                &mut invalid_count,
            )];
            freeze_inkscript_assets(
                model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current(),
                &mut never_cancel,
            )
        };
        assert!(matches!(
            invalid_count_result,
            Err(ScriptAssetError::StreamReadFailed)
        ));

        let mut oversized = MemoryReader::stable(payload.clone(), authorized_identity);
        let oversized_result = {
            let mut streams = [AuthorizedAssetStream::new(
                "paint",
                authorized_identity,
                &mut oversized,
            )];
            freeze_inkscript_assets(
                model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current().with_external_asset_bytes(4),
                &mut never_cancel,
            )
        };
        assert!(matches!(
            oversized_result,
            Err(ScriptAssetError::ResourceLimit)
        ));
        assert_eq!(
            oversized.cursor, 0,
            "oversize must fail before payload read"
        );

        let large_payload = vec![11; 128 * 1024];
        let large_id = direct_rgba_asset(large_payload.clone(), 32 * 1024, 1);
        let large_model = complete_model(&external_asset("paint", large_id, 32 * 1024, 1)).unwrap();
        let large_identity = identity(4, large_payload.len() as u64);
        let mut large_reader = MemoryReader::stable(large_payload, large_identity);
        let mut polls = 0_u32;
        let mut cancel_mid_read = || {
            polls += 1;
            polls == 6
        };
        let cancelled = {
            let mut streams = [AuthorizedAssetStream::new(
                "paint",
                large_identity,
                &mut large_reader,
            )];
            freeze_inkscript_assets(
                large_model.assets(),
                &mut streams,
                ScriptAssetLimits::exact_current(),
                &mut cancel_mid_read,
            )
        };
        assert!(matches!(cancelled, Err(ScriptAssetError::Cancelled)));
        assert!(large_reader.cursor > 0);
        assert!(large_reader.cursor < 128 * 1024);
    }
}
