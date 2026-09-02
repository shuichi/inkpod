use super::{MAX_JOURNAL_BYTES, Member, PAIR_JOURNAL_VERSION, Proof, Record, validate_leaf};
use crate::{FileIdentity, FileStamp, IoError, IoResult};
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"INKPAIR\0";

pub(super) fn encode(record: &Record) -> IoResult<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&PAIR_JOURNAL_VERSION.to_le_bytes());
    bytes.push(u8::from(record.committed));
    for member in [&record.native, &record.raster] {
        for name in [&member.name, &member.stage, &member.backup] {
            validate_leaf(name)?;
            bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
        }
        for proof in [
            member.original.as_ref(),
            Some(&member.replacement),
            member.backup_proof.as_ref(),
        ] {
            bytes.push(u8::from(proof.is_some()));
            if let Some(proof) = proof {
                bytes.extend_from_slice(&proof.stamp.identity.volume.to_le_bytes());
                bytes.extend_from_slice(&proof.stamp.identity.file.to_le_bytes());
                bytes.extend_from_slice(&proof.stamp.length.to_le_bytes());
                bytes.extend_from_slice(&proof.stamp.modified.to_le_bytes());
                bytes.extend_from_slice(&proof.stamp.changed.to_le_bytes());
                bytes.push(u8::from(proof.stamp.readonly));
                bytes.extend_from_slice(&proof.digest);
            }
        }
    }
    let hash = blake3::hash(&bytes);
    bytes.extend_from_slice(hash.as_bytes());
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(IoError::LimitExceeded("paired journal exceeds its bound"));
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> IoResult<Record> {
    if bytes.len() < 44 || bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(IoError::InvalidInput("paired journal length is invalid"));
    }
    let payload = &bytes[..bytes.len() - 32];
    if blake3::hash(payload).as_bytes() != &bytes[bytes.len() - 32..] {
        return Err(IoError::InvalidInput("paired journal checksum is invalid"));
    }
    let mut reader = Reader {
        bytes: payload,
        offset: 0,
    };
    if reader.fixed::<8>()? != *MAGIC || u32::from_le_bytes(reader.fixed()?) != PAIR_JOURNAL_VERSION
    {
        return Err(IoError::InvalidInput(
            "paired journal version is unsupported",
        ));
    }
    let committed = match reader.fixed::<1>()?[0] {
        0 => false,
        1 => true,
        _ => {
            return Err(IoError::InvalidInput(
                "paired journal commit phase is invalid",
            ));
        }
    };
    let native = reader.member()?;
    let raster = reader.member()?;
    if reader.offset != payload.len() {
        return Err(IoError::InvalidInput("paired journal has trailing data"));
    }
    let mut names = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for member in [&native, &raster] {
        if member.original.is_some() != member.backup_proof.is_some() {
            return Err(IoError::InvalidInput(
                "paired journal backup state is inconsistent",
            ));
        }
        for name in [&member.name, &member.stage, &member.backup] {
            let key = crate::backend::normalized_leaf(name);
            if !names.insert(key) {
                return Err(IoError::InvalidInput(
                    "paired journal contains duplicate paths",
                ));
            }
        }
        if !member.stage.starts_with(".inkpod-pair-") || !member.backup.starts_with(".inkpod-pair-")
        {
            return Err(IoError::InvalidInput(
                "paired journal artifact is outside its private namespace",
            ));
        }
        if let (Some(original), Some(backup)) = (&member.original, &member.backup_proof) {
            if original.digest != backup.digest || original.stamp.length != backup.stamp.length {
                return Err(IoError::InvalidInput(
                    "paired journal backup digest is inconsistent",
                ));
            }
        }
        for proof in [
            member.original.as_ref(),
            Some(&member.replacement),
            member.backup_proof.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if !identities.insert(proof.stamp.identity) {
                return Err(IoError::InvalidInput(
                    "paired journal contains aliased artifacts",
                ));
            }
        }
    }
    Ok(Record {
        committed,
        native,
        raster,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> IoResult<&[u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(IoError::InvalidInput("paired journal offset overflows"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(IoError::InvalidInput("paired journal is truncated"))?;
        self.offset = end;
        Ok(value)
    }
    fn fixed<const N: usize>(&mut self) -> IoResult<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| IoError::InvalidInput("paired journal field is truncated"))
    }
    fn string(&mut self) -> IoResult<String> {
        let length = u32::from_le_bytes(self.fixed()?) as usize;
        if length > 4096 {
            return Err(IoError::LimitExceeded(
                "paired journal name exceeds its bound",
            ));
        }
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_| IoError::InvalidInput("paired journal name is not UTF-8"))?;
        validate_leaf(value)?;
        Ok(value.to_owned())
    }
    fn proof(&mut self) -> IoResult<Option<Proof>> {
        match self.fixed::<1>()?[0] {
            0 => return Ok(None),
            1 => {}
            _ => {
                return Err(IoError::InvalidInput(
                    "paired journal presence flag is invalid",
                ));
            }
        }
        let identity = FileIdentity {
            volume: u64::from_le_bytes(self.fixed()?),
            file: u128::from_le_bytes(self.fixed()?),
        };
        let length = u64::from_le_bytes(self.fixed()?);
        if length > super::MAX_NATIVE_BYTES {
            return Err(IoError::LimitExceeded(
                "paired journal file size exceeds its bound",
            ));
        }
        let modified = i128::from_le_bytes(self.fixed()?);
        let changed = i128::from_le_bytes(self.fixed()?);
        let readonly = match self.fixed::<1>()?[0] {
            0 => false,
            1 => true,
            _ => {
                return Err(IoError::InvalidInput(
                    "paired journal readonly flag is invalid",
                ));
            }
        };
        let digest = self.fixed()?;
        Ok(Some(Proof {
            stamp: FileStamp {
                identity,
                length,
                modified,
                changed,
                readonly,
            },
            digest,
        }))
    }
    fn member(&mut self) -> IoResult<Member> {
        Ok(Member {
            name: self.string()?,
            stage: self.string()?,
            backup: self.string()?,
            original: self.proof()?,
            replacement: self.proof()?.ok_or(IoError::InvalidInput(
                "paired journal replacement proof is missing",
            ))?,
            backup_proof: self.proof()?,
        })
    }
}
