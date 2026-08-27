//! Current-only procedure-authoritative `.inkpod` v29 container codec.
//!
//! This module owns byte layout, directory validation, digest verification,
//! resource limits, and atomic file replacement. Section payload meaning stays
//! in the Core mapping layer so format DTOs never depend on runtime types.

use crate::FormatError;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const MAGIC: [u8; 8] = *b"INKPOD\0\0";
const META: [u8; 4] = *b"META";
const GENS: [u8; 4] = *b"GENS";
const ASST: [u8; 4] = *b"ASST";
const PROC: [u8; 4] = *b"PROC";
const EDIT: [u8; 4] = *b"EDIT";
const CKPT: [u8; 4] = *b"CKPT";
const EXTM: [u8; 4] = *b"EXTM";
/// Exact current native file version. Earlier and later versions are rejected.
pub const FORMAT_VERSION: u32 = 29;
const REPLAY_EPOCH: u32 = 9;
const HEADER_BYTES: usize = 128;
const DIRECTORY_ENTRY_BYTES: usize = 128;
const RECORD_HEADER_BYTES: usize = 16;
const REQUIRED_ALIGNMENT: u32 = 8;
const MAX_FILE_BYTES: u64 = 1_073_741_824;
const MAX_SECTION_COUNT: usize = 64;
const MAX_SECTION_BYTES: u64 = 768 * 1_024 * 1_024;
const MAX_TOTAL_LOGICAL_BYTES: u64 = 1_073_741_824;
const MAX_OPAQUE_BYTES: u64 = 16 * 1_024 * 1_024;
const MAX_CHECKPOINT_BYTES: u64 = 512 * 1_024 * 1_024;
const MAX_RECORD_COUNT: u64 = 2_097_152;
const ATOMIC_WRITE_CHUNK_BYTES: usize = 1_024 * 1_024;
const SECTION_STORED_CONTEXT: &str = "org.inkpod.digest.section-stored.v1";
const SECTION_LOGICAL_CONTEXT: &str = "org.inkpod.digest.section-logical.v1";
const FILE_ROOT_CONTEXT: &str = "org.inkpod.digest.file-root.v1";

/// Directory flag marking a section as required for document meaning.
pub const SECTION_CRITICAL: u16 = 1;
/// Directory flag requiring an unknown optional section to round-trip opaquely.
pub const OPAQUE_PRESERVE: u16 = 2;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// One packed record inside a native section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRecord {
    pub kind: u16,
    pub schema_version: u16,
    pub flags: u32,
    pub payload: Vec<u8>,
}

/// One logical section. Records are represented independently of runtime types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSection {
    pub fourcc: [u8; 4],
    pub schema_version: u16,
    pub flags: u16,
    pub records: Vec<NativeRecord>,
}

/// Procedure-authoritative container DTO.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFile {
    pub primitive_catalog_digest: [u8; 32],
    pub sections: Vec<NativeSection>,
}

/// Validates a current native DTO's descriptors and bounded encoded layout.
///
/// This checks section/record schemas and resource limits without copying or
/// hashing payloads. It is the boundary for trusted stream decoders handing a
/// DTO to Core; section payload meaning remains Core's responsibility.
pub fn validate_procedure_file(file: &NativeFile) -> Result<(), FormatError> {
    validate_section_set(&file.sections)?;
    let mut total_logical = 0_u64;
    let mut next_offset = HEADER_BYTES as u64;
    let mut ordered = file.sections.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|section| (section.fourcc, section.schema_version));
    for section in ordered {
        let length = encoded_records_length(&section.records)?;
        if length > MAX_SECTION_BYTES {
            return Err(FormatError::Invalid("native section exceeds byte limit"));
        }
        total_logical = total_logical
            .checked_add(length)
            .ok_or(FormatError::Invalid("native logical byte total overflows"))?;
        next_offset = align_eight(next_offset)?
            .checked_add(length)
            .ok_or(FormatError::Invalid("native file length overflows"))?;
    }
    let directory_length = (file.sections.len() as u64) * DIRECTORY_ENTRY_BYTES as u64;
    let total_length = align_eight(next_offset)?
        .checked_add(directory_length)
        .ok_or(FormatError::Invalid("native file length overflows"))?;
    if total_logical > MAX_TOTAL_LOGICAL_BYTES || total_length > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("native file exceeds byte limit"));
    }
    Ok(())
}

#[derive(Clone)]
struct EncodedSection {
    section: NativeSection,
    logical: Vec<u8>,
    offset: u64,
    stored_digest: [u8; 32],
    logical_digest: [u8; 32],
}

#[derive(Clone, Copy)]
struct PreparedSection<'a> {
    section: &'a NativeSection,
    logical_length: u64,
    offset: u64,
    stored_digest: [u8; 32],
    logical_digest: [u8; 32],
}

#[derive(Clone, Copy)]
struct DirectoryEntry {
    fourcc: [u8; 4],
    schema_version: u16,
    flags: u16,
    compression: u32,
    alignment: u32,
    offset: u64,
    stored_length: u64,
    logical_length: u64,
    record_count: u64,
    stored_digest: [u8; 32],
    logical_digest: [u8; 32],
}

fn prepare_sections<'a>(
    file: &'a NativeFile,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<PreparedSection<'a>>, FormatError> {
    let mut sections = file.sections.iter().collect::<Vec<_>>();
    sections.sort_by_key(|section| (section.fourcc, section.schema_version));
    let mut next_offset = HEADER_BYTES as u64;
    let mut prepared = Vec::new();
    prepared
        .try_reserve_exact(sections.len())
        .map_err(|_| FormatError::Invalid("native section allocation failed"))?;
    for section in sections {
        if cancelled() {
            return Err(FormatError::Cancelled);
        }
        let logical_length = encoded_records_length(&section.records)?;
        next_offset = align_eight(next_offset)?;
        let offset = next_offset;
        next_offset = next_offset
            .checked_add(logical_length)
            .ok_or(FormatError::Invalid("native file length overflows"))?;
        prepared.push(PreparedSection {
            section,
            logical_length,
            offset,
            stored_digest: section_records_digest(
                SECTION_STORED_CONTEXT,
                section,
                Some(0),
                logical_length,
                cancelled,
            )?,
            logical_digest: section_records_digest(
                SECTION_LOGICAL_CONTEXT,
                section,
                None,
                logical_length,
                cancelled,
            )?,
        });
    }
    Ok(prepared)
}

fn directory_entries(sections: &[PreparedSection<'_>]) -> Vec<DirectoryEntry> {
    sections
        .iter()
        .map(|section| DirectoryEntry {
            fourcc: section.section.fourcc,
            schema_version: section.section.schema_version,
            flags: section.section.flags,
            compression: 0,
            alignment: REQUIRED_ALIGNMENT,
            offset: section.offset,
            stored_length: section.logical_length,
            logical_length: section.logical_length,
            record_count: section.section.records.len() as u64,
            stored_digest: section.stored_digest,
            logical_digest: section.logical_digest,
        })
        .collect()
}

/// Encodes a fully validated current native container.
pub fn encode_procedure_file(file: &NativeFile) -> Result<Vec<u8>, FormatError> {
    validate_section_set(&file.sections)?;
    let mut sections = file
        .sections
        .iter()
        .cloned()
        .map(|section| {
            let logical = encode_records(&section.records)?;
            let logical_length = logical.len() as u64;
            if logical_length > MAX_SECTION_BYTES {
                return Err(FormatError::Invalid("native section exceeds byte limit"));
            }
            let stored_digest = section_digest(
                SECTION_STORED_CONTEXT,
                section.fourcc,
                section.schema_version,
                Some(0),
                &logical,
            );
            let logical_digest = section_digest(
                SECTION_LOGICAL_CONTEXT,
                section.fourcc,
                section.schema_version,
                None,
                &logical,
            );
            Ok(EncodedSection {
                section,
                logical,
                offset: 0,
                stored_digest,
                logical_digest,
            })
        })
        .collect::<Result<Vec<_>, FormatError>>()?;
    sections.sort_by_key(|section| (section.section.fourcc, section.section.schema_version));

    let mut next_offset = HEADER_BYTES as u64;
    for section in &mut sections {
        next_offset = align_eight(next_offset)?;
        section.offset = next_offset;
        next_offset = next_offset
            .checked_add(section.logical.len() as u64)
            .ok_or(FormatError::Invalid("native file length overflows"))?;
    }
    let directory_offset = align_eight(next_offset)?;
    let directory_length = sections
        .len()
        .checked_mul(DIRECTORY_ENTRY_BYTES)
        .ok_or(FormatError::Invalid("native directory length overflows"))?
        as u64;
    let total_length = directory_offset
        .checked_add(directory_length)
        .ok_or(FormatError::Invalid("native file length overflows"))?;
    if total_length > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("native file exceeds byte limit"));
    }

    let entries = sections
        .iter()
        .map(|section| DirectoryEntry {
            fourcc: section.section.fourcc,
            schema_version: section.section.schema_version,
            flags: section.section.flags,
            compression: 0,
            alignment: REQUIRED_ALIGNMENT,
            offset: section.offset,
            stored_length: section.logical.len() as u64,
            logical_length: section.logical.len() as u64,
            record_count: section.section.records.len() as u64,
            stored_digest: section.stored_digest,
            logical_digest: section.logical_digest,
        })
        .collect::<Vec<_>>();
    let directory = encode_directory(&entries);
    let mut header = encode_header(
        total_length,
        directory_offset,
        entries.len() as u32,
        file.primitive_catalog_digest,
        [0; 32],
    );
    let root = file_root_digest(&header, &directory);
    header[80..112].copy_from_slice(&root);

    let capacity = usize::try_from(total_length)
        .map_err(|_| FormatError::Invalid("native file is not addressable"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| FormatError::Invalid("native file allocation failed"))?;
    bytes.extend_from_slice(&header);
    for section in &sections {
        push_zero_padding(&mut bytes, section.offset)?;
        bytes.extend_from_slice(&section.logical);
    }
    push_zero_padding(&mut bytes, directory_offset)?;
    bytes.extend_from_slice(&directory);
    if bytes.len() as u64 != total_length {
        return Err(FormatError::Invalid(
            "native encoded length is inconsistent",
        ));
    }
    Ok(bytes)
}

/// Decodes and validates a current native container without interpreting Core payloads.
pub fn decode_procedure_file(bytes: &[u8]) -> Result<NativeFile, FormatError> {
    if bytes.len() < HEADER_BYTES {
        return Err(FormatError::Invalid("native header is truncated"));
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("native file exceeds byte limit"));
    }
    if bytes[0..8] != MAGIC {
        return Err(FormatError::Invalid("native magic does not match"));
    }
    let version = read_u32(bytes, 8)?;
    if version != FORMAT_VERSION {
        return Err(FormatError::Unsupported("format version is not supported"));
    }
    if read_u32(bytes, 12)? != REPLAY_EPOCH {
        return Err(FormatError::Unsupported("replay epoch is not supported"));
    }
    if read_u32(bytes, 16)? != HEADER_BYTES as u32
        || read_u32(bytes, 20)? != 0
        || read_u32(bytes, 44)? != DIRECTORY_ENTRY_BYTES as u32
        || bytes[112..128].iter().any(|byte| *byte != 0)
    {
        return Err(FormatError::Invalid("native header fields are invalid"));
    }
    let total_length = read_u64(bytes, 24)?;
    if total_length != bytes.len() as u64 {
        return Err(FormatError::Invalid(
            "native total length does not match input",
        ));
    }
    let directory_offset = read_u64(bytes, 32)?;
    let section_count = read_u32(bytes, 40)? as usize;
    if section_count > MAX_SECTION_COUNT || directory_offset % 8 != 0 {
        return Err(FormatError::Invalid("native directory bounds are invalid"));
    }
    let directory_length = section_count
        .checked_mul(DIRECTORY_ENTRY_BYTES)
        .ok_or(FormatError::Invalid("native directory length overflows"))?;
    let directory_start = usize::try_from(directory_offset)
        .map_err(|_| FormatError::Invalid("native directory offset is not addressable"))?;
    let directory_end = directory_start
        .checked_add(directory_length)
        .ok_or(FormatError::Invalid("native directory end overflows"))?;
    if directory_start < HEADER_BYTES || directory_end != bytes.len() {
        return Err(FormatError::Invalid("native directory range is invalid"));
    }
    let directory = &bytes[directory_start..directory_end];
    let mut zeroed_header = bytes[..HEADER_BYTES].to_vec();
    let stored_root: [u8; 32] = zeroed_header[80..112].try_into().unwrap();
    zeroed_header[80..112].fill(0);
    if file_root_digest(&zeroed_header, directory) != stored_root {
        return Err(FormatError::ChecksumMismatch);
    }
    let catalog_digest: [u8; 32] = bytes[48..80].try_into().unwrap();
    let entries = decode_directory(directory)?;
    validate_directory_ranges(bytes, directory_start, &entries)?;

    let mut sections = Vec::new();
    let mut total_logical = 0_u64;
    let mut opaque_bytes = 0_u64;
    for entry in entries {
        if entry.compression != 0 || entry.stored_length != entry.logical_length {
            return Err(FormatError::Unsupported(
                "native compression is not supported",
            ));
        }
        if entry.logical_length > MAX_SECTION_BYTES {
            return Err(FormatError::Invalid("native section exceeds byte limit"));
        }
        total_logical = total_logical
            .checked_add(entry.logical_length)
            .ok_or(FormatError::Invalid("native logical byte total overflows"))?;
        if total_logical > MAX_TOTAL_LOGICAL_BYTES {
            return Err(FormatError::Invalid(
                "native logical byte total exceeds limit",
            ));
        }
        if entry.flags & OPAQUE_PRESERVE != 0 {
            opaque_bytes = opaque_bytes
                .checked_add(entry.logical_length)
                .ok_or(FormatError::Invalid("native opaque byte total overflows"))?;
            if opaque_bytes > MAX_OPAQUE_BYTES {
                return Err(FormatError::Invalid(
                    "native opaque byte total exceeds limit",
                ));
            }
        }
        let start = usize::try_from(entry.offset)
            .map_err(|_| FormatError::Invalid("native section offset is not addressable"))?;
        let end = start
            .checked_add(entry.stored_length as usize)
            .ok_or(FormatError::Invalid("native section end overflows"))?;
        let logical = &bytes[start..end];
        if section_digest(
            SECTION_STORED_CONTEXT,
            entry.fourcc,
            entry.schema_version,
            Some(entry.compression),
            logical,
        ) != entry.stored_digest
            || section_digest(
                SECTION_LOGICAL_CONTEXT,
                entry.fourcc,
                entry.schema_version,
                None,
                logical,
            ) != entry.logical_digest
        {
            return Err(FormatError::ChecksumMismatch);
        }
        let records = decode_records(logical, entry.record_count)?;
        sections.push(NativeSection {
            fourcc: entry.fourcc,
            schema_version: entry.schema_version,
            flags: entry.flags,
            records,
        });
    }
    validate_section_set(&sections)?;
    Ok(NativeFile {
        primitive_catalog_digest: catalog_digest,
        sections,
    })
}

fn decode_header(
    header: &[u8; HEADER_BYTES],
    actual_length: u64,
) -> Result<(u64, usize, [u8; 32], [u8; 32]), FormatError> {
    if header[0..8] != MAGIC {
        return Err(FormatError::Invalid("native magic does not match"));
    }
    if read_u32(header, 8)? != FORMAT_VERSION {
        return Err(FormatError::Unsupported("format version is not supported"));
    }
    if read_u32(header, 12)? != REPLAY_EPOCH {
        return Err(FormatError::Unsupported("replay epoch is not supported"));
    }
    if read_u32(header, 16)? != HEADER_BYTES as u32
        || read_u32(header, 20)? != 0
        || read_u32(header, 44)? != DIRECTORY_ENTRY_BYTES as u32
        || header[112..128].iter().any(|byte| *byte != 0)
    {
        return Err(FormatError::Invalid("native header fields are invalid"));
    }
    if read_u64(header, 24)? != actual_length {
        return Err(FormatError::Invalid(
            "native total length does not match input",
        ));
    }
    let directory_offset = read_u64(header, 32)?;
    let section_count = read_u32(header, 40)? as usize;
    if section_count > MAX_SECTION_COUNT || directory_offset % 8 != 0 {
        return Err(FormatError::Invalid("native directory bounds are invalid"));
    }
    Ok((
        directory_offset,
        section_count,
        header[48..80].try_into().unwrap(),
        header[80..112].try_into().unwrap(),
    ))
}

fn validate_entry_resource_totals(
    entry: &DirectoryEntry,
    total_logical: &mut u64,
    opaque_bytes: &mut u64,
) -> Result<(), FormatError> {
    if entry.compression != 0 || entry.stored_length != entry.logical_length {
        return Err(FormatError::Unsupported(
            "native compression is not supported",
        ));
    }
    if entry.logical_length > MAX_SECTION_BYTES {
        return Err(FormatError::Invalid("native section exceeds byte limit"));
    }
    *total_logical = total_logical
        .checked_add(entry.logical_length)
        .ok_or(FormatError::Invalid("native logical byte total overflows"))?;
    if *total_logical > MAX_TOTAL_LOGICAL_BYTES {
        return Err(FormatError::Invalid(
            "native logical byte total exceeds limit",
        ));
    }
    if entry.flags & OPAQUE_PRESERVE != 0 {
        *opaque_bytes = opaque_bytes
            .checked_add(entry.logical_length)
            .ok_or(FormatError::Invalid("native opaque byte total overflows"))?;
        if *opaque_bytes > MAX_OPAQUE_BYTES {
            return Err(FormatError::Invalid(
                "native opaque byte total exceeds limit",
            ));
        }
    }
    if entry.fourcc == CKPT && entry.logical_length > MAX_CHECKPOINT_BYTES {
        return Err(FormatError::Invalid(
            "checkpoint section exceeds byte limit",
        ));
    }
    Ok(())
}

fn read_records_streaming(
    file: &mut impl Read,
    entry: &DirectoryEntry,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<NativeRecord>, FormatError> {
    if entry.record_count > MAX_RECORD_COUNT
        || entry
            .record_count
            .checked_mul(RECORD_HEADER_BYTES as u64)
            .is_none_or(|minimum| minimum > entry.logical_length)
    {
        return Err(FormatError::Invalid("native record count exceeds bounds"));
    }
    let count = usize::try_from(entry.record_count)
        .map_err(|_| FormatError::Invalid("native record count is not addressable"))?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(count)
        .map_err(|_| FormatError::Invalid("native record allocation failed"))?;
    let mut stored_hasher = section_payload_hasher(
        SECTION_STORED_CONTEXT,
        entry.fourcc,
        entry.schema_version,
        Some(entry.compression),
        entry.stored_length,
    );
    let mut logical_hasher = section_payload_hasher(
        SECTION_LOGICAL_CONTEXT,
        entry.fourcc,
        entry.schema_version,
        None,
        entry.logical_length,
    );
    let mut consumed = 0_u64;
    for _ in 0..count {
        if cancelled() {
            return Err(FormatError::Cancelled);
        }
        let mut header = [0_u8; RECORD_HEADER_BYTES];
        file.read_exact(&mut header)?;
        consumed = consumed
            .checked_add(RECORD_HEADER_BYTES as u64)
            .ok_or(FormatError::Invalid("native record bytes overflow"))?;
        let payload_length = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let end = consumed
            .checked_add(payload_length)
            .ok_or(FormatError::Invalid("native record payload end overflows"))?;
        if end > entry.logical_length {
            return Err(FormatError::Invalid("native record payload is truncated"));
        }
        let payload_length = usize::try_from(payload_length)
            .map_err(|_| FormatError::Invalid("native record payload is not addressable"))?;
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(payload_length)
            .map_err(|_| FormatError::Invalid("native record allocation failed"))?;
        payload.resize(payload_length, 0);
        stored_hasher.update(&header);
        logical_hasher.update(&header);
        for chunk in payload.chunks_mut(ATOMIC_WRITE_CHUNK_BYTES) {
            if cancelled() {
                return Err(FormatError::Cancelled);
            }
            file.read_exact(chunk)?;
            stored_hasher.update(chunk);
            logical_hasher.update(chunk);
        }
        records.push(NativeRecord {
            kind: u16::from_le_bytes(header[0..2].try_into().unwrap()),
            schema_version: u16::from_le_bytes(header[2..4].try_into().unwrap()),
            flags: u32::from_le_bytes(header[4..8].try_into().unwrap()),
            payload,
        });
        consumed = end;
    }
    if consumed != entry.logical_length {
        return Err(FormatError::Invalid("native section has trailing bytes"));
    }
    if *stored_hasher.finalize().as_bytes() != entry.stored_digest
        || *logical_hasher.finalize().as_bytes() != entry.logical_digest
    {
        return Err(FormatError::ChecksumMismatch);
    }
    Ok(records)
}

/// Reads one current native container from disk.
pub fn read_procedure_file(path: &Path) -> Result<NativeFile, FormatError> {
    let mut file = fs::File::open(path)?;
    read_procedure_from_reader(&mut file, || false)
}

/// Reads one complete current container from an already opened seekable stream.
///
/// The stream is read from byte zero regardless of its initial position. Length,
/// directory, records, checksums, and the same 1 GiB native limit as disk reads
/// are validated before a DTO is returned. Cancellation is checked between
/// sections, records, and at most 1 MiB payload chunks. This codec owns no path,
/// file lock, or stream lifetime; the caller keeps the source stable throughout.
pub fn read_procedure_from_reader(
    file: &mut (impl Read + Seek),
    mut cancelled: impl FnMut() -> bool,
) -> Result<NativeFile, FormatError> {
    if cancelled() {
        return Err(FormatError::Cancelled);
    }
    let length = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    if length > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("native file exceeds byte limit"));
    }
    if length < HEADER_BYTES as u64 {
        return Err(FormatError::Invalid("native header is truncated"));
    }
    let mut header = [0_u8; HEADER_BYTES];
    file.read_exact(&mut header)?;
    let (directory_offset, section_count, catalog_digest, stored_root) =
        decode_header(&header, length)?;
    let directory_length = section_count
        .checked_mul(DIRECTORY_ENTRY_BYTES)
        .ok_or(FormatError::Invalid("native directory length overflows"))?;
    let directory_end = directory_offset
        .checked_add(directory_length as u64)
        .ok_or(FormatError::Invalid("native directory end overflows"))?;
    if directory_offset < HEADER_BYTES as u64 || directory_end != length {
        return Err(FormatError::Invalid("native directory range is invalid"));
    }
    file.seek(SeekFrom::Start(directory_offset))?;
    let mut directory = vec![0_u8; directory_length];
    file.read_exact(&mut directory)?;
    let mut zeroed_header = header;
    zeroed_header[80..112].fill(0);
    if file_root_digest(&zeroed_header, &directory) != stored_root {
        return Err(FormatError::ChecksumMismatch);
    }
    let entries = decode_directory(&directory)?;
    validate_file_directory_ranges(file, directory_offset, &entries, &mut cancelled)?;

    let mut sections = Vec::new();
    sections
        .try_reserve_exact(entries.len())
        .map_err(|_| FormatError::Invalid("native section allocation failed"))?;
    let mut total_logical = 0_u64;
    let mut opaque_bytes = 0_u64;
    for entry in entries {
        if cancelled() {
            return Err(FormatError::Cancelled);
        }
        validate_entry_resource_totals(&entry, &mut total_logical, &mut opaque_bytes)?;
        file.seek(SeekFrom::Start(entry.offset))?;
        let records = read_records_streaming(file, &entry, &mut cancelled)?;
        sections.push(NativeSection {
            fourcc: entry.fourcc,
            schema_version: entry.schema_version,
            flags: entry.flags,
            records,
        });
    }
    validate_section_set(&sections)?;
    if cancelled() {
        return Err(FormatError::Cancelled);
    }
    Ok(NativeFile {
        primitive_catalog_digest: catalog_digest,
        sections,
    })
}

/// Atomically saves one current native container.
pub fn save_procedure_file_atomic(path: &Path, file: &NativeFile) -> Result<(), FormatError> {
    save_procedure_file_atomic_with_cancel(path, file, || false)
}

/// Atomically saves one current native container with cancellation before replace.
pub fn save_procedure_file_atomic_with_cancel(
    path: &Path,
    file: &NativeFile,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), FormatError> {
    atomic_replace_streaming(path, file, &mut cancelled)
}

/// Writes a current container to a caller-owned stream without allocating an
/// encoded copy of the entire file.
///
/// The caller must supply an empty stream positioned at byte zero. The return
/// value is the complete encoded length in bytes. On cancellation or an I/O
/// error the stream may contain a partial file; the caller must discard it.
/// The caller alone owns flush, durability, closing, and destination replacement.
pub fn write_procedure_to_writer(
    output: &mut impl Write,
    file: &NativeFile,
    mut cancelled: impl FnMut() -> bool,
) -> Result<u64, FormatError> {
    if cancelled() {
        return Err(FormatError::Cancelled);
    }
    validate_section_set(&file.sections)?;
    let prepared = prepare_sections(file, &mut cancelled)?;
    let entries = directory_entries(&prepared);
    let directory = encode_directory(&entries);
    let total_length = entries.last().map_or(HEADER_BYTES as u64, |entry| {
        entry.offset.saturating_add(entry.stored_length)
    });
    let directory_offset = align_eight(total_length)?;
    let total_length = directory_offset
        .checked_add(directory.len() as u64)
        .ok_or(FormatError::Invalid("native file length overflows"))?;
    if total_length > MAX_FILE_BYTES {
        return Err(FormatError::Invalid("native file exceeds byte limit"));
    }
    let mut header = encode_header(
        total_length,
        directory_offset,
        entries.len() as u32,
        file.primitive_catalog_digest,
        [0; 32],
    );
    let root = file_root_digest(&header, &directory);
    header[80..112].copy_from_slice(&root);
    if cancelled() {
        return Err(FormatError::Cancelled);
    }
    output.write_all(&header)?;
    let mut position = header.len() as u64;
    for section in &prepared {
        write_zero_padding_streaming(output, &mut position, section.offset, &mut cancelled)?;
        for record in &section.section.records {
            if cancelled() {
                return Err(FormatError::Cancelled);
            }
            let header = record_header(record);
            output.write_all(&header)?;
            position += header.len() as u64;
            for chunk in record.payload.chunks(ATOMIC_WRITE_CHUNK_BYTES) {
                if cancelled() {
                    return Err(FormatError::Cancelled);
                }
                output.write_all(chunk)?;
                position += chunk.len() as u64;
            }
        }
    }
    write_zero_padding_streaming(output, &mut position, directory_offset, &mut cancelled)?;
    if cancelled() {
        return Err(FormatError::Cancelled);
    }
    output.write_all(&directory)?;
    Ok(total_length)
}

/// Recovery save uses the identical durable same-directory replacement protocol.
pub fn save_recovery_procedure_file_atomic(
    path: &Path,
    file: &NativeFile,
) -> Result<(), FormatError> {
    save_procedure_file_atomic(path, file)
}

fn validate_section_set(sections: &[NativeSection]) -> Result<(), FormatError> {
    if sections.len() > MAX_SECTION_COUNT {
        return Err(FormatError::Invalid("native section count exceeds limit"));
    }
    let mut identities = BTreeSet::new();
    let mut required = BTreeSet::new();
    let mut opaque_total = 0_u64;
    for section in sections {
        if !identities.insert((section.fourcc, section.schema_version)) {
            return Err(FormatError::Invalid("native section is duplicated"));
        }
        if section.flags & !(SECTION_CRITICAL | OPAQUE_PRESERVE) != 0 {
            return Err(FormatError::Invalid("native section flags are invalid"));
        }
        match section.fourcc {
            META | GENS | ASST | PROC | EDIT => {
                let expected_schema = if section.fourcc == META { 2 } else { 1 };
                if section.schema_version != expected_schema || section.flags != SECTION_CRITICAL {
                    return Err(FormatError::Invalid(
                        "required native section descriptor is invalid",
                    ));
                }
                required.insert(section.fourcc);
                validate_known_records(section)?;
            }
            CKPT => {
                if section.schema_version != 1 || section.flags != 0 || section.records.len() != 1 {
                    return Err(FormatError::Invalid(
                        "checkpoint section descriptor is invalid",
                    ));
                }
                validate_known_records(section)?;
                let checkpoint_bytes = encoded_records_length(&section.records)?;
                if checkpoint_bytes > MAX_CHECKPOINT_BYTES {
                    return Err(FormatError::Invalid(
                        "checkpoint section exceeds byte limit",
                    ));
                }
            }
            EXTM => {
                if section.schema_version != 1 || section.flags != OPAQUE_PRESERVE {
                    return Err(FormatError::Invalid(
                        "opaque metadata section descriptor is invalid",
                    ));
                }
            }
            _ => {
                if section.flags != OPAQUE_PRESERVE {
                    return Err(if section.flags & SECTION_CRITICAL != 0 {
                        FormatError::Unsupported("unknown critical native section")
                    } else {
                        FormatError::Invalid("unknown optional section is not opaque-preserve")
                    });
                }
            }
        }
        if section.flags & OPAQUE_PRESERVE != 0 {
            opaque_total = opaque_total
                .checked_add(encoded_records_length(&section.records)?)
                .ok_or(FormatError::Invalid("native opaque byte total overflows"))?;
        }
    }
    for fourcc in [META, GENS, ASST, PROC, EDIT] {
        if !required.contains(&fourcc) {
            return Err(FormatError::Invalid("required native section is missing"));
        }
    }
    if opaque_total > MAX_OPAQUE_BYTES {
        return Err(FormatError::Invalid(
            "native opaque byte total exceeds limit",
        ));
    }
    Ok(())
}

fn validate_known_records(section: &NativeSection) -> Result<(), FormatError> {
    let exact_one = matches!(section.fourcc, META | GENS | EDIT | CKPT);
    if exact_one && section.records.len() != 1 {
        return Err(FormatError::Invalid(
            "native singleton section record count is invalid",
        ));
    }
    for record in &section.records {
        let expected_schema = if section.fourcc == META { 2 } else { 1 };
        if record.flags != 0 || record.schema_version != expected_schema {
            return Err(FormatError::Invalid(
                "native known record descriptor is invalid",
            ));
        }
        let valid_kind = match section.fourcc {
            META | GENS | EDIT => record.kind == 1,
            ASST => matches!(record.kind, 1 | 2),
            PROC => matches!(record.kind, 1..=3),
            CKPT => record.kind == 1,
            _ => true,
        };
        if !valid_kind {
            return Err(FormatError::Unsupported(
                "native record kind is not supported",
            ));
        }
    }
    Ok(())
}

fn encode_records(records: &[NativeRecord]) -> Result<Vec<u8>, FormatError> {
    let total = encoded_records_length(records)?;
    let total = usize::try_from(total)
        .map_err(|_| FormatError::Invalid("native section is not addressable"))?;
    if total as u64 > MAX_SECTION_BYTES {
        return Err(FormatError::Invalid("native section exceeds byte limit"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(total)
        .map_err(|_| FormatError::Invalid("native section allocation failed"))?;
    for record in records {
        bytes.extend_from_slice(&record.kind.to_le_bytes());
        bytes.extend_from_slice(&record.schema_version.to_le_bytes());
        bytes.extend_from_slice(&record.flags.to_le_bytes());
        bytes.extend_from_slice(&(record.payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&record.payload);
    }
    Ok(bytes)
}

fn encoded_records_length(records: &[NativeRecord]) -> Result<u64, FormatError> {
    if records.len() as u64 > MAX_RECORD_COUNT {
        return Err(FormatError::Invalid("native record count exceeds limit"));
    }
    let total = records.iter().try_fold(0_u64, |total, record| {
        total
            .checked_add(RECORD_HEADER_BYTES as u64)
            .and_then(|value| value.checked_add(record.payload.len() as u64))
            .ok_or(FormatError::Invalid("native record bytes overflow"))
    })?;
    if total > MAX_SECTION_BYTES {
        return Err(FormatError::Invalid("native section exceeds byte limit"));
    }
    Ok(total)
}

fn decode_records(bytes: &[u8], expected_count: u64) -> Result<Vec<NativeRecord>, FormatError> {
    if expected_count > MAX_RECORD_COUNT
        || expected_count
            .checked_mul(RECORD_HEADER_BYTES as u64)
            .is_none_or(|minimum| minimum > bytes.len() as u64)
    {
        return Err(FormatError::Invalid("native record count exceeds bounds"));
    }
    let count = usize::try_from(expected_count)
        .map_err(|_| FormatError::Invalid("native record count is not addressable"))?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(count)
        .map_err(|_| FormatError::Invalid("native record allocation failed"))?;
    let mut cursor = 0_usize;
    for _ in 0..count {
        let header_end = cursor
            .checked_add(RECORD_HEADER_BYTES)
            .ok_or(FormatError::Invalid("native record header overflows"))?;
        if header_end > bytes.len() {
            return Err(FormatError::Invalid("native record header is truncated"));
        }
        let kind = read_u16(bytes, cursor)?;
        let schema_version = read_u16(bytes, cursor + 2)?;
        let flags = read_u32(bytes, cursor + 4)?;
        let length = read_u64(bytes, cursor + 8)?;
        let payload_end =
            header_end
                .checked_add(usize::try_from(length).map_err(|_| {
                    FormatError::Invalid("native record payload is not addressable")
                })?)
                .ok_or(FormatError::Invalid("native record payload end overflows"))?;
        if payload_end > bytes.len() {
            return Err(FormatError::Invalid("native record payload is truncated"));
        }
        records.push(NativeRecord {
            kind,
            schema_version,
            flags,
            payload: bytes[header_end..payload_end].to_vec(),
        });
        cursor = payload_end;
    }
    if cursor != bytes.len() {
        return Err(FormatError::Invalid("native section has trailing bytes"));
    }
    Ok(records)
}

fn encode_header(
    total_length: u64,
    directory_offset: u64,
    section_count: u32,
    catalog_digest: [u8; 32],
    root_digest: [u8; 32],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_BYTES);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&REPLAY_EPOCH.to_le_bytes());
    bytes.extend_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&total_length.to_le_bytes());
    bytes.extend_from_slice(&directory_offset.to_le_bytes());
    bytes.extend_from_slice(&section_count.to_le_bytes());
    bytes.extend_from_slice(&(DIRECTORY_ENTRY_BYTES as u32).to_le_bytes());
    bytes.extend_from_slice(&catalog_digest);
    bytes.extend_from_slice(&root_digest);
    bytes.extend_from_slice(&[0; 16]);
    bytes
}

fn encode_directory(entries: &[DirectoryEntry]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(entries.len() * DIRECTORY_ENTRY_BYTES);
    for entry in entries {
        bytes.extend_from_slice(&entry.fourcc);
        bytes.extend_from_slice(&entry.schema_version.to_le_bytes());
        bytes.extend_from_slice(&entry.flags.to_le_bytes());
        bytes.extend_from_slice(&entry.compression.to_le_bytes());
        bytes.extend_from_slice(&entry.alignment.to_le_bytes());
        bytes.extend_from_slice(&entry.offset.to_le_bytes());
        bytes.extend_from_slice(&entry.stored_length.to_le_bytes());
        bytes.extend_from_slice(&entry.logical_length.to_le_bytes());
        bytes.extend_from_slice(&entry.record_count.to_le_bytes());
        bytes.extend_from_slice(&entry.stored_digest);
        bytes.extend_from_slice(&entry.logical_digest);
        bytes.extend_from_slice(&[0; 16]);
    }
    bytes
}

fn decode_directory(bytes: &[u8]) -> Result<Vec<DirectoryEntry>, FormatError> {
    let mut entries = Vec::new();
    let mut previous = None;
    for chunk in bytes.chunks_exact(DIRECTORY_ENTRY_BYTES) {
        if chunk[112..128].iter().any(|byte| *byte != 0) {
            return Err(FormatError::Invalid(
                "native directory reserved bytes are nonzero",
            ));
        }
        let entry = DirectoryEntry {
            fourcc: chunk[0..4].try_into().unwrap(),
            schema_version: u16::from_le_bytes(chunk[4..6].try_into().unwrap()),
            flags: u16::from_le_bytes(chunk[6..8].try_into().unwrap()),
            compression: u32::from_le_bytes(chunk[8..12].try_into().unwrap()),
            alignment: u32::from_le_bytes(chunk[12..16].try_into().unwrap()),
            offset: u64::from_le_bytes(chunk[16..24].try_into().unwrap()),
            stored_length: u64::from_le_bytes(chunk[24..32].try_into().unwrap()),
            logical_length: u64::from_le_bytes(chunk[32..40].try_into().unwrap()),
            record_count: u64::from_le_bytes(chunk[40..48].try_into().unwrap()),
            stored_digest: chunk[48..80].try_into().unwrap(),
            logical_digest: chunk[80..112].try_into().unwrap(),
        };
        if entry.alignment != REQUIRED_ALIGNMENT || entry.offset % 8 != 0 {
            return Err(FormatError::Invalid("native section alignment is invalid"));
        }
        let identity = (entry.fourcc, entry.schema_version);
        if previous.is_some_and(|value| value >= identity) {
            return Err(FormatError::Invalid(
                "native directory order or identity is invalid",
            ));
        }
        previous = Some(identity);
        entries.push(entry);
    }
    Ok(entries)
}

fn validate_directory_ranges(
    bytes: &[u8],
    directory_start: usize,
    entries: &[DirectoryEntry],
) -> Result<(), FormatError> {
    let mut ranges = Vec::with_capacity(entries.len());
    for entry in entries {
        let start = usize::try_from(entry.offset)
            .map_err(|_| FormatError::Invalid("native section offset is not addressable"))?;
        let length = usize::try_from(entry.stored_length)
            .map_err(|_| FormatError::Invalid("native section length is not addressable"))?;
        let end = start
            .checked_add(length)
            .ok_or(FormatError::Invalid("native section end overflows"))?;
        if start < HEADER_BYTES || end > directory_start || end > bytes.len() {
            return Err(FormatError::Invalid("native section range is invalid"));
        }
        ranges.push((start, end));
    }
    ranges.sort_unstable();
    let mut cursor = HEADER_BYTES;
    for (start, end) in ranges {
        if start < cursor {
            return Err(FormatError::Invalid("native section ranges overlap"));
        }
        if bytes[cursor..start].iter().any(|byte| *byte != 0) {
            return Err(FormatError::Invalid("native section padding is nonzero"));
        }
        cursor = end;
    }
    if bytes[cursor..directory_start].iter().any(|byte| *byte != 0) {
        return Err(FormatError::Invalid("native directory padding is nonzero"));
    }
    Ok(())
}

fn validate_file_directory_ranges(
    file: &mut (impl Read + Seek),
    directory_offset: u64,
    entries: &[DirectoryEntry],
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormatError> {
    let mut ranges = Vec::new();
    ranges
        .try_reserve_exact(entries.len())
        .map_err(|_| FormatError::Invalid("native range allocation failed"))?;
    for entry in entries {
        let end = entry
            .offset
            .checked_add(entry.stored_length)
            .ok_or(FormatError::Invalid("native section end overflows"))?;
        if entry.offset < HEADER_BYTES as u64 || end > directory_offset {
            return Err(FormatError::Invalid("native section range is invalid"));
        }
        ranges.push((entry.offset, end));
    }
    ranges.sort_unstable();
    let mut cursor = HEADER_BYTES as u64;
    for (start, end) in ranges {
        if start < cursor {
            return Err(FormatError::Invalid("native section ranges overlap"));
        }
        validate_zero_file_range(file, cursor, start, cancelled)?;
        cursor = end;
    }
    validate_zero_file_range(file, cursor, directory_offset, cancelled)
}

fn validate_zero_file_range(
    file: &mut (impl Read + Seek),
    start: u64,
    end: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormatError> {
    if end < start {
        return Err(FormatError::Invalid("native padding range regresses"));
    }
    file.seek(SeekFrom::Start(start))?;
    let mut remaining = end - start;
    let mut buffer = [0_u8; 4096];
    while remaining != 0 {
        if cancelled() {
            return Err(FormatError::Cancelled);
        }
        let length = usize::try_from(remaining.min(buffer.len() as u64)).unwrap();
        file.read_exact(&mut buffer[..length])?;
        if buffer[..length].iter().any(|byte| *byte != 0) {
            return Err(FormatError::Invalid("native section padding is nonzero"));
        }
        remaining -= length as u64;
    }
    Ok(())
}

fn section_digest(
    context: &str,
    fourcc: [u8; 4],
    version: u16,
    compression: Option<u32>,
    bytes: &[u8],
) -> [u8; 32] {
    let field_count: u32 = if compression.is_some() { 4 } else { 3 };
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(&1_u32.to_le_bytes());
    hasher.update(&field_count.to_le_bytes());
    hash_field(&mut hasher, 1, &fourcc);
    hash_field(&mut hasher, 2, &version.to_le_bytes());
    if let Some(value) = compression {
        hash_field(&mut hasher, 3, &value.to_le_bytes());
        hash_field(&mut hasher, 4, bytes);
    } else {
        hash_field(&mut hasher, 3, bytes);
    }
    *hasher.finalize().as_bytes()
}

fn section_records_digest(
    context: &str,
    section: &NativeSection,
    compression: Option<u32>,
    logical_length: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; 32], FormatError> {
    let mut hasher = section_payload_hasher(
        context,
        section.fourcc,
        section.schema_version,
        compression,
        logical_length,
    );
    for record in &section.records {
        if cancelled() {
            return Err(FormatError::Cancelled);
        }
        let header = record_header(record);
        hasher.update(&header);
        for chunk in record.payload.chunks(ATOMIC_WRITE_CHUNK_BYTES) {
            if cancelled() {
                return Err(FormatError::Cancelled);
            }
            hasher.update(chunk);
        }
    }
    Ok(*hasher.finalize().as_bytes())
}

fn section_payload_hasher(
    context: &str,
    fourcc: [u8; 4],
    version: u16,
    compression: Option<u32>,
    logical_length: u64,
) -> blake3::Hasher {
    let field_count: u32 = if compression.is_some() { 4 } else { 3 };
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(&1_u32.to_le_bytes());
    hasher.update(&field_count.to_le_bytes());
    hash_field(&mut hasher, 1, &fourcc);
    hash_field(&mut hasher, 2, &version.to_le_bytes());
    let payload_ordinal: u32 = if let Some(value) = compression {
        hash_field(&mut hasher, 3, &value.to_le_bytes());
        4
    } else {
        3
    };
    hasher.update(&payload_ordinal.to_le_bytes());
    hasher.update(&[1, 0, 0, 0]);
    hasher.update(&logical_length.to_le_bytes());
    hasher
}

fn record_header(record: &NativeRecord) -> [u8; RECORD_HEADER_BYTES] {
    let mut header = [0_u8; RECORD_HEADER_BYTES];
    header[0..2].copy_from_slice(&record.kind.to_le_bytes());
    header[2..4].copy_from_slice(&record.schema_version.to_le_bytes());
    header[4..8].copy_from_slice(&record.flags.to_le_bytes());
    header[8..16].copy_from_slice(&(record.payload.len() as u64).to_le_bytes());
    header
}

fn file_root_digest(header: &[u8], directory: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(FILE_ROOT_CONTEXT);
    hasher.update(&1_u32.to_le_bytes());
    hasher.update(&2_u32.to_le_bytes());
    hash_field(&mut hasher, 1, header);
    hash_field(&mut hasher, 2, directory);
    *hasher.finalize().as_bytes()
}

fn hash_field(hasher: &mut blake3::Hasher, ordinal: u32, bytes: &[u8]) {
    hasher.update(&ordinal.to_le_bytes());
    hasher.update(&[1, 0, 0, 0]);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn align_eight(value: u64) -> Result<u64, FormatError> {
    value
        .checked_add(7)
        .map(|value| value & !7)
        .ok_or(FormatError::Invalid("native alignment overflows"))
}

fn push_zero_padding(bytes: &mut Vec<u8>, target: u64) -> Result<(), FormatError> {
    let target = usize::try_from(target)
        .map_err(|_| FormatError::Invalid("native offset is not addressable"))?;
    if target < bytes.len() {
        return Err(FormatError::Invalid("native section offset regresses"));
    }
    bytes.resize(target, 0);
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, FormatError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(FormatError::Invalid("native fixed field is truncated"))?
            .try_into()
            .unwrap(),
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, FormatError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(FormatError::Invalid("native fixed field is truncated"))?
            .try_into()
            .unwrap(),
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, FormatError> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(FormatError::Invalid("native fixed field is truncated"))?
            .try_into()
            .unwrap(),
    ))
}

fn atomic_replace_streaming(
    path: &Path,
    file: &NativeFile,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormatError> {
    let parent = path.parent().ok_or(FormatError::Invalid(
        "native destination has no parent directory",
    ))?;
    let mut temporary = None;
    for _ in 0..128 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".inkpod-{}-{sequence}.tmp", std::process::id()));
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
    let (temporary_path, mut temporary_file) = temporary.ok_or(FormatError::Invalid(
        "native temporary file name space is exhausted",
    ))?;
    let result = (|| {
        write_procedure_to_writer(&mut temporary_file, file, &mut *cancelled)?;
        temporary_file.flush()?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        if cancelled() {
            return Err(FormatError::Cancelled);
        }
        replace_file(&temporary_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn write_zero_padding_streaming(
    output: &mut impl Write,
    position: &mut u64,
    target: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormatError> {
    if target < *position {
        return Err(FormatError::Invalid("native section offset regresses"));
    }
    let zeros = [0_u8; 4096];
    while *position < target {
        if cancelled() {
            return Err(FormatError::Cancelled);
        }
        let length = usize::try_from((target - *position).min(zeros.len() as u64)).unwrap();
        output.write_all(&zeros[..length])?;
        *position += length as u64;
    }
    Ok(())
}

fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}
