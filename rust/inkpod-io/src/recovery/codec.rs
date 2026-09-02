use super::model::{MAX_METADATA_BYTES, MAX_PATH_UNITS, validate_string};
use super::{
    RECOVERY_METADATA_VERSION, RecoveryIdentity, RecoveryIdentityKind, RecoveryMetadata,
    RecoveryPairProof,
};
use crate::{FileIdentity, FileStamp, IoError, IoResult};

const MAGIC: &[u8; 8] = b"INKRCVR\0";
const FIXED_BYTES: usize = 286;

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
    let (pair_kind, native, raster) = match metadata.pair_proof {
        None => (0_u32, zero_stamp(), zero_stamp()),
        Some(RecoveryPairProof::Committed { native, raster }) => (1, native, raster),
        Some(RecoveryPairProof::Planned {
            native_missing,
            raster,
        }) => (
            2,
            FileStamp {
                identity: native_missing,
                length: 0,
                modified: 0,
                changed: 0,
                readonly: false,
            },
            raster,
        ),
        Some(RecoveryPairProof::RepairNeeded {
            native,
            raster_missing,
        }) => (
            3,
            native,
            FileStamp {
                identity: raster_missing,
                length: 0,
                modified: 0,
                changed: 0,
                readonly: false,
            },
        ),
    };
    bytes.extend_from_slice(&pair_kind.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    write_stamp(&mut bytes, native);
    write_stamp(&mut bytes, raster);
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

/// Decodes only metadata version 4. Invalid UTF-8, overlong strings, inconsistent
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
    let pair_kind = u32::from_le_bytes(reader.fixed()?);
    if u32::from_le_bytes(reader.fixed()?) != 0 {
        return Err(IoError::InvalidInput(
            "recovery pair proof reserved field is nonzero",
        ));
    }
    let native_pair_stamp = reader.stamp()?;
    let raster_pair_stamp = reader.stamp()?;
    let pair_proof = match pair_kind {
        0 if native_pair_stamp == zero_stamp() && raster_pair_stamp == zero_stamp() => None,
        1 => Some(RecoveryPairProof::Committed {
            native: native_pair_stamp,
            raster: raster_pair_stamp,
        }),
        2 if native_pair_stamp.length == 0
            && native_pair_stamp.modified == 0
            && native_pair_stamp.changed == 0
            && !native_pair_stamp.readonly =>
        {
            Some(RecoveryPairProof::Planned {
                native_missing: native_pair_stamp.identity,
                raster: raster_pair_stamp,
            })
        }
        3 if raster_pair_stamp.length == 0
            && raster_pair_stamp.modified == 0
            && raster_pair_stamp.changed == 0
            && !raster_pair_stamp.readonly =>
        {
            Some(RecoveryPairProof::RepairNeeded {
                native: native_pair_stamp,
                raster_missing: raster_pair_stamp.identity,
            })
        }
        _ => {
            return Err(IoError::InvalidInput(
                "recovery pair proof kind or payload is invalid",
            ));
        }
    };
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
        pair_proof,
    };
    metadata.validate()?;
    Ok(metadata)
}

fn zero_stamp() -> FileStamp {
    FileStamp {
        identity: FileIdentity { volume: 0, file: 0 },
        length: 0,
        modified: 0,
        changed: 0,
        readonly: false,
    }
}

fn write_stamp(bytes: &mut Vec<u8>, stamp: FileStamp) {
    bytes.extend_from_slice(&stamp.identity.volume.to_le_bytes());
    bytes.extend_from_slice(&stamp.identity.file.to_le_bytes());
    bytes.extend_from_slice(&stamp.length.to_le_bytes());
    bytes.extend_from_slice(&stamp.modified.to_le_bytes());
    bytes.extend_from_slice(&stamp.changed.to_le_bytes());
    bytes.push(u8::from(stamp.readonly));
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

    fn stamp(&mut self) -> IoResult<FileStamp> {
        let identity = FileIdentity {
            volume: u64::from_le_bytes(self.fixed()?),
            file: u128::from_le_bytes(self.fixed()?),
        };
        let length = u64::from_le_bytes(self.fixed()?);
        let modified = i128::from_le_bytes(self.fixed()?);
        let changed = i128::from_le_bytes(self.fixed()?);
        let readonly = match self.fixed::<1>()?[0] {
            0 => false,
            1 => true,
            _ => {
                return Err(IoError::InvalidInput(
                    "recovery pair proof readonly flag is invalid",
                ));
            }
        };
        Ok(FileStamp {
            identity,
            length,
            modified,
            changed,
            readonly,
        })
    }
}
