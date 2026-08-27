use super::model::{MAX_METADATA_BYTES, MAX_PATH_UNITS, validate_string};
use super::{RECOVERY_METADATA_VERSION, RecoveryIdentity, RecoveryIdentityKind, RecoveryMetadata};
use crate::{IoError, IoResult};

const MAGIC: &[u8; 8] = b"INKRCVR\0";
const FIXED_BYTES: usize = 148;

/// Encodes a bounded, checksummed current-version metadata record without I/O.
/// A zero timestamp is accepted by the filesystem writers, not by this codec.
pub fn encode_recovery_metadata(metadata: &RecoveryMetadata) -> IoResult<Vec<u8>> {
    metadata.validate()?;
    let length = FIXED_BYTES
        .checked_add(metadata.original_path.len())
        .and_then(|value| value.checked_add(metadata.original_identity.normalized_path.len()))
        .and_then(|value| value.checked_add(metadata.source_path.len()))
        .filter(|value| *value <= MAX_METADATA_BYTES)
        .ok_or(IoError::LimitExceeded(
            "recovery metadata exceeds its byte bound",
        ))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| IoError::ResourceBusy("recovery metadata allocation failed"))?;
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&RECOVERY_METADATA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(length as u32).to_le_bytes());
    bytes.extend_from_slice(&metadata.session_id.to_le_bytes());
    bytes.extend_from_slice(&metadata.generation.to_le_bytes());
    bytes.extend_from_slice(&metadata.document_uuid.to_le_bytes());
    bytes.extend_from_slice(&metadata.written_time_100ns.to_le_bytes());
    let kind = match metadata.original_identity.kind {
        RecoveryIdentityKind::None => 0_u32,
        RecoveryIdentityKind::PhysicalFile => 1,
        RecoveryIdentityKind::NormalizedPath => 2,
        RecoveryIdentityKind::Untitled => 3,
    };
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&metadata.original_identity.volume_serial.to_le_bytes());
    bytes.extend_from_slice(&metadata.original_identity.file_id);
    bytes.extend_from_slice(&metadata.original_identity.uuid.to_le_bytes());
    for value in [
        &metadata.original_path,
        &metadata.original_identity.normalized_path,
        &metadata.source_path,
    ] {
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    let checksum = blake3::hash(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    Ok(bytes)
}

/// Decodes only metadata version 2. Invalid UTF-8, overlong strings, inconsistent
/// identity fields, nonzero reserved fields, and trailing bytes are rejected.
pub fn decode_recovery_metadata(bytes: &[u8]) -> IoResult<RecoveryMetadata> {
    if !(FIXED_BYTES..=MAX_METADATA_BYTES).contains(&bytes.len()) {
        return Err(IoError::InvalidInput("recovery metadata length is invalid"));
    }
    let payload = &bytes[..bytes.len() - 32];
    if blake3::hash(payload).as_bytes() != &bytes[bytes.len() - 32..] {
        return Err(IoError::InvalidInput(
            "recovery metadata checksum is invalid",
        ));
    }
    let mut reader = Reader {
        bytes: payload,
        offset: 0,
    };
    if reader.fixed::<8>()? != *MAGIC
        || u32::from_le_bytes(reader.fixed()?) != RECOVERY_METADATA_VERSION
        || u32::from_le_bytes(reader.fixed()?) as usize != bytes.len()
    {
        return Err(IoError::InvalidInput(
            "recovery metadata version or header is invalid",
        ));
    }
    let session_id = u64::from_le_bytes(reader.fixed()?);
    let generation = u64::from_le_bytes(reader.fixed()?);
    let document_uuid = u128::from_le_bytes(reader.fixed()?);
    let written_time_100ns = u64::from_le_bytes(reader.fixed()?);
    let kind = match u32::from_le_bytes(reader.fixed()?) {
        0 => RecoveryIdentityKind::None,
        1 => RecoveryIdentityKind::PhysicalFile,
        2 => RecoveryIdentityKind::NormalizedPath,
        3 => RecoveryIdentityKind::Untitled,
        _ => return Err(IoError::InvalidInput("recovery identity kind is unknown")),
    };
    if u32::from_le_bytes(reader.fixed()?) != 0 {
        return Err(IoError::InvalidInput(
            "recovery metadata reserved field is nonzero",
        ));
    }
    let volume_serial = u64::from_le_bytes(reader.fixed()?);
    let file_id = reader.fixed()?;
    let uuid = u128::from_le_bytes(reader.fixed()?);
    let original_path = reader.string()?;
    let normalized_path = reader.string()?;
    let source_path = reader.string()?;
    if reader.offset != payload.len() {
        return Err(IoError::InvalidInput("recovery metadata has trailing data"));
    }
    let metadata = RecoveryMetadata {
        session_id,
        generation,
        document_uuid,
        written_time_100ns,
        original_identity: RecoveryIdentity {
            kind,
            volume_serial,
            file_id,
            normalized_path,
            uuid,
        },
        original_path,
        source_path,
    };
    metadata.validate()?;
    Ok(metadata)
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl Reader<'_> {
    fn fixed<const N: usize>(&mut self) -> IoResult<[u8; N]> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(IoError::InvalidInput("recovery field length overflow"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(IoError::InvalidInput("recovery metadata is truncated"))?;
        let mut value = [0; N];
        value.copy_from_slice(slice);
        self.offset = end;
        Ok(value)
    }

    fn string(&mut self) -> IoResult<String> {
        let length = u32::from_le_bytes(self.fixed()?) as usize;
        if length > MAX_PATH_UNITS * 4 {
            return Err(IoError::LimitExceeded(
                "recovery path exceeds its byte bound",
            ));
        }
        let end = self
            .offset
            .checked_add(length)
            .ok_or(IoError::InvalidInput("recovery path length overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(IoError::InvalidInput("recovery path is truncated"))?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| IoError::InvalidInput("recovery path is not UTF-8"))?;
        validate_string(value)?;
        let mut result = String::new();
        result
            .try_reserve_exact(value.len())
            .map_err(|_| IoError::ResourceBusy("recovery path allocation failed"))?;
        result.push_str(value);
        self.offset = end;
        Ok(result)
    }
}
