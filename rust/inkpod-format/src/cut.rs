//! Current-only Cut descriptor codec for individually referenced Cell documents.

use crate::{FORMAT_VERSION, FormatError};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: [u8; 8] = *b"INKCUT\0\0";
const HEADER_BYTES: usize = 64;
const SCHEMA_VERSION: u32 = 2;
const MAX_FILE_BYTES: usize = 16 * 1_024 * 1_024;
const MAX_MEMBERS: usize = 64;
const MAX_HISTORY: usize = 4096;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_PATH_BYTES: usize = 255;
const DIGEST_CONTEXT: &str = "org.inkpod.cut-descriptor.v2";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Runtime replay epoch for the current Cut canonical procedure semantics.
pub const CUT_DESCRIPTOR_REPLAY_EPOCH: u32 = 23;

/// Persisted Cut metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileCutMetadata {
    pub work_title: String,
    pub episode: String,
    pub scene: String,
    pub cut_name: String,
    pub instruction: String,
    pub duration_frames: u32,
}

/// Persisted default Cell creation values copied explicitly at Cell creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileCutDefaults {
    pub sizing_mode: u32,
    pub size_a: u32,
    pub size_b: u32,
    pub dpi_x_milli: u32,
    pub dpi_y_milli: u32,
    pub margin_milli: u32,
    pub safe_frame_ratio_milli: u32,
    pub maximum_close_ratio_milli: u32,
    pub anchor: u32,
    pub initial_layer_kind: u32,
    pub pixel_format: u32,
}

/// Immutable identity-to-path asset for one independently saved Cell `.inkpod`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileCutMemberAsset {
    pub cell_id: u64,
    pub document_uuid: [u8; 16],
    pub relative_path: String,
}

/// One ordered membership reference. Paths never occur in canonical history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileCutMembership {
    pub cell_id: u64,
    pub document_uuid: [u8; 16],
    pub display_number: u32,
}

/// One canonical Cut edit. Member paths never occur in procedures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileCutHistoryEntry {
    pub procedure_id: u64,
    pub base_state_id: u64,
    pub committed_state_id: u64,
    pub before_metadata: FileCutMetadata,
    pub before_defaults: FileCutDefaults,
    pub before_members: Vec<FileCutMembership>,
    pub after_metadata: FileCutMetadata,
    pub after_defaults: FileCutDefaults,
    pub after_members: Vec<FileCutMembership>,
}

/// Complete current Cut descriptor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileCutDescriptor {
    pub cut_id: u64,
    pub cut_uuid: [u8; 16],
    pub current_state_id: u64,
    pub savepoint_state_id: u64,
    pub next_state_id: u64,
    pub next_procedure_id: u64,
    pub history_cursor: u32,
    pub genesis_metadata: FileCutMetadata,
    pub genesis_defaults: FileCutDefaults,
    pub genesis_members: Vec<FileCutMembership>,
    pub metadata: FileCutMetadata,
    pub defaults: FileCutDefaults,
    pub member_assets: Vec<FileCutMemberAsset>,
    pub members: Vec<FileCutMembership>,
    pub active_history: Vec<FileCutHistoryEntry>,
    pub inactive_history: Vec<FileCutHistoryEntry>,
}

/// Encodes one fully validated current Cut descriptor.
pub fn encode_cut_descriptor(descriptor: &FileCutDescriptor) -> Result<Vec<u8>, FormatError> {
    validate_descriptor(descriptor)?;
    let mut payload = Vec::new();
    push_u32(&mut payload, SCHEMA_VERSION);
    push_u32(&mut payload, CUT_DESCRIPTOR_REPLAY_EPOCH);
    push_u64(&mut payload, descriptor.cut_id);
    payload.extend_from_slice(&descriptor.cut_uuid);
    push_u64(&mut payload, descriptor.current_state_id);
    push_u64(&mut payload, descriptor.savepoint_state_id);
    push_u64(&mut payload, descriptor.next_state_id);
    push_u64(&mut payload, descriptor.next_procedure_id);
    push_u32(&mut payload, descriptor.history_cursor);
    push_u32(&mut payload, descriptor.member_assets.len() as u32);
    push_u32(&mut payload, descriptor.genesis_members.len() as u32);
    push_u32(&mut payload, descriptor.members.len() as u32);
    push_u32(&mut payload, descriptor.active_history.len() as u32);
    push_u32(&mut payload, descriptor.inactive_history.len() as u32);
    encode_metadata(&mut payload, &descriptor.genesis_metadata)?;
    encode_defaults(&mut payload, descriptor.genesis_defaults);
    encode_metadata(&mut payload, &descriptor.metadata)?;
    encode_defaults(&mut payload, descriptor.defaults);
    for asset in &descriptor.member_assets {
        encode_member_asset(&mut payload, asset)?;
    }
    for member in &descriptor.genesis_members {
        encode_membership(&mut payload, *member);
    }
    for member in &descriptor.members {
        encode_membership(&mut payload, *member);
    }
    for entry in descriptor
        .active_history
        .iter()
        .chain(descriptor.inactive_history.iter())
    {
        encode_history(&mut payload, entry)?;
    }
    let total = HEADER_BYTES
        .checked_add(payload.len())
        .ok_or(FormatError::Invalid("Cut descriptor length overflows"))?;
    if total > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("Cut descriptor exceeds byte limit"));
    }
    let digest = blake3::derive_key(DIGEST_CONTEXT, &payload);
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(&MAGIC);
    push_u32(&mut output, FORMAT_VERSION);
    push_u32(&mut output, CUT_DESCRIPTOR_REPLAY_EPOCH);
    push_u64(&mut output, total as u64);
    push_u64(&mut output, payload.len() as u64);
    output.extend_from_slice(&digest);
    output.extend_from_slice(&payload);
    Ok(output)
}

/// Decodes and validates one exact-current Cut descriptor.
pub fn decode_cut_descriptor(bytes: &[u8]) -> Result<FileCutDescriptor, FormatError> {
    if bytes.len() < HEADER_BYTES || bytes.len() > MAX_FILE_BYTES {
        return Err(FormatError::Invalid(
            "Cut descriptor length is outside bounds",
        ));
    }
    if bytes[..8] != MAGIC {
        return Err(FormatError::Invalid("Cut descriptor magic does not match"));
    }
    if read_u32(bytes, 8)? != FORMAT_VERSION || read_u32(bytes, 12)? != CUT_DESCRIPTOR_REPLAY_EPOCH
    {
        return Err(FormatError::Unsupported(
            "Cut descriptor format or replay version is not current",
        ));
    }
    let total = usize::try_from(read_u64(bytes, 16)?)
        .map_err(|_| FormatError::Invalid("Cut descriptor length is not addressable"))?;
    let payload_length = usize::try_from(read_u64(bytes, 24)?)
        .map_err(|_| FormatError::Invalid("Cut payload length is not addressable"))?;
    if total != bytes.len() || payload_length != bytes.len().saturating_sub(HEADER_BYTES) {
        return Err(FormatError::Invalid(
            "Cut descriptor lengths are inconsistent",
        ));
    }
    let payload = &bytes[HEADER_BYTES..];
    if bytes[32..64] != blake3::derive_key(DIGEST_CONTEXT, payload) {
        return Err(FormatError::Invalid(
            "Cut descriptor checksum does not match",
        ));
    }
    let mut reader = Reader::new(payload);
    if reader.u32()? != SCHEMA_VERSION || reader.u32()? != CUT_DESCRIPTOR_REPLAY_EPOCH {
        return Err(FormatError::Unsupported(
            "Cut payload schema is not current",
        ));
    }
    let cut_id = reader.u64()?;
    let cut_uuid = reader.array_16()?;
    let current_state_id = reader.u64()?;
    let savepoint_state_id = reader.u64()?;
    let next_state_id = reader.u64()?;
    let next_procedure_id = reader.u64()?;
    let history_cursor = reader.u32()?;
    let member_asset_count = reader.count(MAX_MEMBERS, "Cut member asset count")?;
    let genesis_member_count = reader.count(MAX_MEMBERS, "Cut Genesis member count")?;
    let member_count = reader.count(MAX_MEMBERS, "Cut member count")?;
    let active_count = reader.count(MAX_HISTORY, "Cut active history count")?;
    let inactive_count = reader.count(MAX_HISTORY, "Cut inactive history count")?;
    if active_count.saturating_add(inactive_count) > MAX_HISTORY {
        return Err(FormatError::Invalid("Cut history count exceeds limit"));
    }
    let genesis_metadata = decode_metadata(&mut reader)?;
    let genesis_defaults = decode_defaults(&mut reader)?;
    let metadata = decode_metadata(&mut reader)?;
    let defaults = decode_defaults(&mut reader)?;
    let mut member_assets = Vec::new();
    member_assets
        .try_reserve_exact(member_asset_count)
        .map_err(|_| FormatError::Invalid("Cut member asset allocation failed"))?;
    for _ in 0..member_asset_count {
        member_assets.push(decode_member_asset(&mut reader)?);
    }
    let mut genesis_members = Vec::new();
    genesis_members
        .try_reserve_exact(genesis_member_count)
        .map_err(|_| FormatError::Invalid("Cut Genesis membership allocation failed"))?;
    for _ in 0..genesis_member_count {
        genesis_members.push(decode_membership(&mut reader)?);
    }
    let mut members = Vec::new();
    members
        .try_reserve_exact(member_count)
        .map_err(|_| FormatError::Invalid("Cut member allocation failed"))?;
    for _ in 0..member_count {
        members.push(decode_membership(&mut reader)?);
    }
    let mut active_history = Vec::new();
    active_history
        .try_reserve_exact(active_count)
        .map_err(|_| FormatError::Invalid("Cut history allocation failed"))?;
    for _ in 0..active_count {
        active_history.push(decode_history(&mut reader)?);
    }
    let mut inactive_history = Vec::new();
    inactive_history
        .try_reserve_exact(inactive_count)
        .map_err(|_| FormatError::Invalid("Cut history allocation failed"))?;
    for _ in 0..inactive_count {
        inactive_history.push(decode_history(&mut reader)?);
    }
    if !reader.is_empty() {
        return Err(FormatError::Invalid("Cut payload has trailing bytes"));
    }
    let descriptor = FileCutDescriptor {
        cut_id,
        cut_uuid,
        current_state_id,
        savepoint_state_id,
        next_state_id,
        next_procedure_id,
        history_cursor,
        genesis_metadata,
        genesis_defaults,
        genesis_members,
        metadata,
        defaults,
        member_assets,
        members,
        active_history,
        inactive_history,
    };
    validate_descriptor(&descriptor)?;
    Ok(descriptor)
}

/// Reads one bounded Cut descriptor.
pub fn read_cut_descriptor(path: &Path) -> Result<FileCutDescriptor, FormatError> {
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    if length > MAX_FILE_BYTES as u64 {
        return Err(FormatError::Invalid("Cut descriptor exceeds byte limit"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length as usize)
        .map_err(|_| FormatError::Invalid("Cut descriptor allocation failed"))?;
    file.read_to_end(&mut bytes)?;
    decode_cut_descriptor(&bytes)
}

/// Durably saves one Cut descriptor through same-directory atomic replacement.
pub fn save_cut_descriptor_atomic(
    path: &Path,
    descriptor: &FileCutDescriptor,
) -> Result<(), FormatError> {
    let bytes = encode_cut_descriptor(descriptor)?;
    atomic_replace(path, &bytes)
}

/// Saves Cut recovery data without changing live savepoint ownership.
pub fn save_cut_recovery_atomic(
    path: &Path,
    descriptor: &FileCutDescriptor,
) -> Result<(), FormatError> {
    save_cut_descriptor_atomic(path, descriptor)
}

fn validate_descriptor(descriptor: &FileCutDescriptor) -> Result<(), FormatError> {
    if descriptor.cut_id == 0
        || descriptor.cut_uuid == [0; 16]
        || descriptor.current_state_id == 0
        || descriptor.next_state_id <= descriptor.current_state_id
        || descriptor.next_procedure_id == 0
        || descriptor.member_assets.len() > MAX_MEMBERS
        || descriptor.genesis_members.len() > MAX_MEMBERS
        || descriptor.members.len() > MAX_MEMBERS
        || descriptor.active_history.len() + descriptor.inactive_history.len() > MAX_HISTORY
        || descriptor.history_cursor as usize > descriptor.active_history.len()
    {
        return Err(FormatError::Invalid(
            "Cut descriptor identity or count is invalid",
        ));
    }
    validate_metadata(&descriptor.genesis_metadata)?;
    validate_metadata(&descriptor.metadata)?;
    let mut assets = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for asset in &descriptor.member_assets {
        if asset.cell_id == 0
            || asset.document_uuid == [0; 16]
            || asset.relative_path.is_empty()
            || asset.relative_path.len() > MAX_PATH_BYTES
            || !assets.insert((asset.cell_id, asset.document_uuid))
            || !paths.insert(asset.relative_path.to_lowercase())
        {
            return Err(FormatError::Invalid(
                "Cut member asset is invalid or duplicated",
            ));
        }
    }
    validate_memberships(&descriptor.genesis_members, &assets)?;
    validate_memberships(&descriptor.members, &assets)?;
    let mut procedures = std::collections::BTreeSet::new();
    for entry in descriptor
        .active_history
        .iter()
        .chain(descriptor.inactive_history.iter())
    {
        if entry.procedure_id == 0
            || entry.base_state_id == 0
            || entry.committed_state_id == 0
            || !procedures.insert(entry.procedure_id)
        {
            return Err(FormatError::Invalid(
                "Cut history identity is invalid or duplicated",
            ));
        }
        validate_metadata(&entry.before_metadata)?;
        validate_metadata(&entry.after_metadata)?;
        validate_memberships(&entry.before_members, &assets)?;
        validate_memberships(&entry.after_members, &assets)?;
    }
    Ok(())
}

fn validate_memberships(
    members: &[FileCutMembership],
    assets: &std::collections::BTreeSet<(u64, [u8; 16])>,
) -> Result<(), FormatError> {
    if members.len() > MAX_MEMBERS {
        return Err(FormatError::Invalid("Cut membership exceeds limit"));
    }
    let mut identities = std::collections::BTreeSet::new();
    let mut numbers = std::collections::BTreeSet::new();
    for member in members {
        let identity = (member.cell_id, member.document_uuid);
        if member.cell_id == 0
            || member.document_uuid == [0; 16]
            || member.display_number == 0
            || !assets.contains(&identity)
            || !identities.insert(identity)
            || !numbers.insert(member.display_number)
        {
            return Err(FormatError::Invalid(
                "Cut membership identity or display number is invalid",
            ));
        }
    }
    Ok(())
}

fn validate_metadata(metadata: &FileCutMetadata) -> Result<(), FormatError> {
    for value in [
        &metadata.work_title,
        &metadata.episode,
        &metadata.scene,
        &metadata.cut_name,
        &metadata.instruction,
    ] {
        if value.len() > MAX_TEXT_BYTES || value.as_bytes().contains(&0) {
            return Err(FormatError::Invalid("Cut metadata text is invalid"));
        }
    }
    if metadata.cut_name.is_empty() || metadata.duration_frames == 0 {
        return Err(FormatError::Invalid(
            "Cut name and duration must be nonzero",
        ));
    }
    Ok(())
}

fn encode_metadata(output: &mut Vec<u8>, value: &FileCutMetadata) -> Result<(), FormatError> {
    push_text(output, &value.work_title, MAX_TEXT_BYTES)?;
    push_text(output, &value.episode, MAX_TEXT_BYTES)?;
    push_text(output, &value.scene, MAX_TEXT_BYTES)?;
    push_text(output, &value.cut_name, MAX_TEXT_BYTES)?;
    push_text(output, &value.instruction, MAX_TEXT_BYTES)?;
    push_u32(output, value.duration_frames);
    Ok(())
}

fn decode_metadata(reader: &mut Reader<'_>) -> Result<FileCutMetadata, FormatError> {
    Ok(FileCutMetadata {
        work_title: reader.text(MAX_TEXT_BYTES)?,
        episode: reader.text(MAX_TEXT_BYTES)?,
        scene: reader.text(MAX_TEXT_BYTES)?,
        cut_name: reader.text(MAX_TEXT_BYTES)?,
        instruction: reader.text(MAX_TEXT_BYTES)?,
        duration_frames: reader.u32()?,
    })
}

fn encode_defaults(output: &mut Vec<u8>, value: FileCutDefaults) {
    for field in [
        value.sizing_mode,
        value.size_a,
        value.size_b,
        value.dpi_x_milli,
        value.dpi_y_milli,
        value.margin_milli,
        value.safe_frame_ratio_milli,
        value.maximum_close_ratio_milli,
        value.anchor,
        value.initial_layer_kind,
        value.pixel_format,
    ] {
        push_u32(output, field);
    }
}

fn decode_defaults(reader: &mut Reader<'_>) -> Result<FileCutDefaults, FormatError> {
    Ok(FileCutDefaults {
        sizing_mode: reader.u32()?,
        size_a: reader.u32()?,
        size_b: reader.u32()?,
        dpi_x_milli: reader.u32()?,
        dpi_y_milli: reader.u32()?,
        margin_milli: reader.u32()?,
        safe_frame_ratio_milli: reader.u32()?,
        maximum_close_ratio_milli: reader.u32()?,
        anchor: reader.u32()?,
        initial_layer_kind: reader.u32()?,
        pixel_format: reader.u32()?,
    })
}

fn encode_member_asset(
    output: &mut Vec<u8>,
    value: &FileCutMemberAsset,
) -> Result<(), FormatError> {
    push_u64(output, value.cell_id);
    output.extend_from_slice(&value.document_uuid);
    push_text(output, &value.relative_path, MAX_PATH_BYTES)
}

fn decode_member_asset(reader: &mut Reader<'_>) -> Result<FileCutMemberAsset, FormatError> {
    Ok(FileCutMemberAsset {
        cell_id: reader.u64()?,
        document_uuid: reader.array_16()?,
        relative_path: reader.text(MAX_PATH_BYTES)?,
    })
}

fn encode_membership(output: &mut Vec<u8>, value: FileCutMembership) {
    push_u64(output, value.cell_id);
    output.extend_from_slice(&value.document_uuid);
    push_u32(output, value.display_number);
}

fn decode_membership(reader: &mut Reader<'_>) -> Result<FileCutMembership, FormatError> {
    Ok(FileCutMembership {
        cell_id: reader.u64()?,
        document_uuid: reader.array_16()?,
        display_number: reader.u32()?,
    })
}

fn encode_history(output: &mut Vec<u8>, value: &FileCutHistoryEntry) -> Result<(), FormatError> {
    push_u64(output, value.procedure_id);
    push_u64(output, value.base_state_id);
    push_u64(output, value.committed_state_id);
    encode_metadata(output, &value.before_metadata)?;
    encode_defaults(output, value.before_defaults);
    push_u32(output, value.before_members.len() as u32);
    for member in &value.before_members {
        encode_membership(output, *member);
    }
    encode_metadata(output, &value.after_metadata)?;
    encode_defaults(output, value.after_defaults);
    push_u32(output, value.after_members.len() as u32);
    for member in &value.after_members {
        encode_membership(output, *member);
    }
    Ok(())
}

fn decode_history(reader: &mut Reader<'_>) -> Result<FileCutHistoryEntry, FormatError> {
    let procedure_id = reader.u64()?;
    let base_state_id = reader.u64()?;
    let committed_state_id = reader.u64()?;
    let before_metadata = decode_metadata(reader)?;
    let before_defaults = decode_defaults(reader)?;
    let before_count = reader.count(MAX_MEMBERS, "Cut history before-member count")?;
    let mut before_members = Vec::new();
    before_members
        .try_reserve_exact(before_count)
        .map_err(|_| FormatError::Invalid("Cut history allocation failed"))?;
    for _ in 0..before_count {
        before_members.push(decode_membership(reader)?);
    }
    let after_metadata = decode_metadata(reader)?;
    let after_defaults = decode_defaults(reader)?;
    let after_count = reader.count(MAX_MEMBERS, "Cut history after-member count")?;
    let mut after_members = Vec::new();
    after_members
        .try_reserve_exact(after_count)
        .map_err(|_| FormatError::Invalid("Cut history allocation failed"))?;
    for _ in 0..after_count {
        after_members.push(decode_membership(reader)?);
    }
    Ok(FileCutHistoryEntry {
        procedure_id,
        base_state_id,
        committed_state_id,
        before_metadata,
        before_defaults,
        before_members,
        after_metadata,
        after_defaults,
        after_members,
    })
}

fn push_text(output: &mut Vec<u8>, value: &str, maximum: usize) -> Result<(), FormatError> {
    if value.len() > maximum || value.as_bytes().contains(&0) {
        return Err(FormatError::Invalid("Cut text is outside bounds"));
    }
    push_u32(output, value.len() as u32);
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, FormatError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(FormatError::Invalid("Cut fixed field is truncated"))?
            .try_into()
            .unwrap(),
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, FormatError> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(FormatError::Invalid("Cut fixed field is truncated"))?
            .try_into()
            .unwrap(),
    ))
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(FormatError::Invalid("Cut payload offset overflows"))?;
        let result = self
            .bytes
            .get(self.cursor..end)
            .ok_or(FormatError::Invalid("Cut payload is truncated"))?;
        self.cursor = end;
        Ok(result)
    }

    fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, FormatError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn array_16(&mut self) -> Result<[u8; 16], FormatError> {
        Ok(self.take(16)?.try_into().unwrap())
    }

    fn count(&mut self, maximum: usize, name: &'static str) -> Result<usize, FormatError> {
        let value = self.u32()? as usize;
        if value > maximum {
            return Err(FormatError::Invalid(name));
        }
        Ok(value)
    }

    fn text(&mut self, maximum: usize) -> Result<String, FormatError> {
        let length = self.count(maximum, "Cut text length exceeds limit")?;
        let bytes = self.take(length)?;
        let text = std::str::from_utf8(bytes)
            .map_err(|_| FormatError::Invalid("Cut text is not UTF-8"))?;
        if bytes.contains(&0) {
            return Err(FormatError::Invalid("Cut text contains NUL"));
        }
        Ok(text.to_owned())
    }

    fn is_empty(&self) -> bool {
        self.cursor == self.bytes.len()
    }
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), FormatError> {
    let parent = path.parent().ok_or(FormatError::Invalid(
        "Cut destination has no parent directory",
    ))?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let mut temporary = None;
    for _ in 0..128 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".inkpod-cut-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let (temporary_path, mut file) = temporary.ok_or(FormatError::Invalid(
        "Cut temporary file name space is exhausted",
    ))?;
    let result = (|| {
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary_path);
    }
    result
}

#[cfg(test)]
#[path = "../tests/unit/cut.rs"]
mod tests;
