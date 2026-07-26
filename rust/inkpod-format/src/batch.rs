use super::{FormatError, checksum};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const BATCH_GRAPH_VERSION: u32 = 1;
const MAGIC: [u8; 8] = *b"INKBATCH";
const MAX_BATCH_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_BATCH_INPUTS: usize = 16_384;
const MAX_BATCH_OPERATIONS: usize = 1_024;
const MAX_BATCH_STRING_BYTES: usize = 32_768;
const MAX_OPERATION_PAYLOAD_BYTES: usize = 1_048_576;
const ATOMIC_WRITE_CHUNK_BYTES: usize = 1_048_576;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBatchInput {
    pub kind: u32,
    pub path: String,
    pub first_cell: u32,
    pub last_cell: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileBatchTarget {
    pub layer_id: u64,
    pub plane_id: u64,
    pub layer_kind: u32,
    pub plane_kind: u32,
    pub missing_policy: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBatchOperation {
    pub version: u32,
    pub kind: u32,
    pub flags: u64,
    pub target: FileBatchTarget,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBatchOutput {
    pub policy: u32,
    pub folder: String,
    pub cell_folder: bool,
    pub format: u32,
    pub basename: String,
    pub start_number: u32,
    pub descending: bool,
    pub failure_policy: u32,
    pub wait_milliseconds: u32,
    pub preview_before_save: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBatchGraph {
    pub version: u32,
    pub name: String,
    pub inputs: Vec<FileBatchInput>,
    pub operations: Vec<FileBatchOperation>,
    pub output: FileBatchOutput,
}

pub fn encode_batch_graph(graph: &FileBatchGraph) -> Result<Vec<u8>, FormatError> {
    validate_graph(graph)?;
    let mut body = Vec::new();
    push_string(&mut body, &graph.name)?;
    push_u32(&mut body, graph.inputs.len() as u32);
    for input in &graph.inputs {
        push_u32(&mut body, input.kind);
        push_u32(&mut body, input.first_cell);
        push_u32(&mut body, input.last_cell);
        push_string(&mut body, &input.path)?;
    }
    push_u32(&mut body, graph.operations.len() as u32);
    for operation in &graph.operations {
        push_u32(&mut body, operation.version);
        push_u32(&mut body, operation.kind);
        push_u64(&mut body, operation.flags);
        push_u64(&mut body, operation.target.layer_id);
        push_u64(&mut body, operation.target.plane_id);
        push_u32(&mut body, operation.target.layer_kind);
        push_u32(&mut body, operation.target.plane_kind);
        push_u32(&mut body, operation.target.missing_policy);
        push_bytes(&mut body, &operation.payload)?;
    }
    push_u32(&mut body, graph.output.policy);
    push_u32(&mut body, graph.output.format);
    push_u32(&mut body, graph.output.start_number);
    push_u32(&mut body, graph.output.failure_policy);
    push_u32(&mut body, graph.output.wait_milliseconds);
    let mut output_flags = 0_u32;
    if graph.output.cell_folder {
        output_flags |= 1;
    }
    if graph.output.descending {
        output_flags |= 1 << 1;
    }
    if graph.output.preview_before_save {
        output_flags |= 1 << 2;
    }
    push_u32(&mut body, output_flags);
    push_string(&mut body, &graph.output.folder)?;
    push_string(&mut body, &graph.output.basename)?;

    let body_len = u64::try_from(body.len())
        .map_err(|_| FormatError::Invalid("batch graph body length is not representable"))?;
    let total = body_len
        .checked_add(28)
        .ok_or(FormatError::Invalid("batch graph size overflows"))?;
    if total > MAX_BATCH_FILE_BYTES {
        return Err(FormatError::Invalid("batch graph exceeds the bounded size"));
    }
    let mut output = Vec::with_capacity(total as usize);
    output.extend_from_slice(&MAGIC);
    push_u32(&mut output, graph.version);
    push_u64(&mut output, body_len);
    push_u64(&mut output, checksum(&body));
    output.extend_from_slice(&body);
    Ok(output)
}

pub fn decode_batch_graph(bytes: &[u8]) -> Result<FileBatchGraph, FormatError> {
    if bytes.len() as u64 > MAX_BATCH_FILE_BYTES {
        return Err(FormatError::Invalid("batch graph exceeds the bounded size"));
    }
    let mut reader = Reader::new(bytes);
    if reader.take(8)? != MAGIC {
        return Err(FormatError::Invalid("batch graph magic is invalid"));
    }
    let version = reader.u32()?;
    if version != BATCH_GRAPH_VERSION {
        return Err(FormatError::Invalid("batch graph version is unsupported"));
    }
    let body_len = reader.u64()?;
    let expected_checksum = reader.u64()?;
    let body_len = usize::try_from(body_len)
        .map_err(|_| FormatError::Invalid("batch graph body length is not representable"))?;
    let body = reader.take(body_len)?;
    if !reader.is_empty() {
        return Err(FormatError::Invalid("batch graph has trailing bytes"));
    }
    if checksum(body) != expected_checksum {
        return Err(FormatError::Invalid("batch graph checksum does not match"));
    }
    let mut body = Reader::new(body);
    let name = body.string()?;
    let input_count = bounded_count(body.u32()?, MAX_BATCH_INPUTS, "batch input count")?;
    let mut inputs = Vec::with_capacity(input_count);
    for _ in 0..input_count {
        inputs.push(FileBatchInput {
            kind: body.u32()?,
            first_cell: body.u32()?,
            last_cell: body.u32()?,
            path: body.string()?,
        });
    }
    let operation_count =
        bounded_count(body.u32()?, MAX_BATCH_OPERATIONS, "batch operation count")?;
    let mut operations = Vec::with_capacity(operation_count);
    for _ in 0..operation_count {
        operations.push(FileBatchOperation {
            version: body.u32()?,
            kind: body.u32()?,
            flags: body.u64()?,
            target: FileBatchTarget {
                layer_id: body.u64()?,
                plane_id: body.u64()?,
                layer_kind: body.u32()?,
                plane_kind: body.u32()?,
                missing_policy: body.u32()?,
            },
            payload: body.bytes()?,
        });
    }
    let policy = body.u32()?;
    let format = body.u32()?;
    let start_number = body.u32()?;
    let failure_policy = body.u32()?;
    let wait_milliseconds = body.u32()?;
    let output_flags = body.u32()?;
    if output_flags & !0x7 != 0 {
        return Err(FormatError::Invalid("batch output flags are invalid"));
    }
    let folder = body.string()?;
    let basename = body.string()?;
    if !body.is_empty() {
        return Err(FormatError::Invalid("batch graph body has trailing bytes"));
    }
    let graph = FileBatchGraph {
        version,
        name,
        inputs,
        operations,
        output: FileBatchOutput {
            policy,
            folder,
            cell_folder: output_flags & 1 != 0,
            format,
            basename,
            start_number,
            descending: output_flags & (1 << 1) != 0,
            failure_policy,
            wait_milliseconds,
            preview_before_save: output_flags & (1 << 2) != 0,
        },
    };
    validate_graph(&graph)?;
    Ok(graph)
}

pub fn read_batch_graph(path: &Path) -> Result<FileBatchGraph, FormatError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_BATCH_FILE_BYTES {
        return Err(FormatError::Invalid("batch graph exceeds the bounded size"));
    }
    let mut input = OpenOptions::new().read(true).open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut input)
        .take(MAX_BATCH_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    decode_batch_graph(&bytes)
}

pub fn save_batch_graph_atomic(path: &Path, graph: &FileBatchGraph) -> Result<(), FormatError> {
    save_batch_graph_atomic_with_cancel(path, graph, || false)
}

pub fn save_batch_graph_atomic_with_cancel(
    path: &Path,
    graph: &FileBatchGraph,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(), FormatError> {
    if is_cancelled() {
        return Err(FormatError::Cancelled);
    }
    let bytes = encode_batch_graph(graph)?;
    if is_cancelled() {
        return Err(FormatError::Cancelled);
    }
    let (temporary_path, mut temporary) = create_temporary(path)?;
    let result = (|| {
        for chunk in bytes.chunks(ATOMIC_WRITE_CHUNK_BYTES) {
            if is_cancelled() {
                return Err(FormatError::Cancelled);
            }
            temporary.write_all(chunk)?;
        }
        temporary.flush()?;
        temporary.sync_all()?;
        drop(temporary);
        if is_cancelled() {
            return Err(FormatError::Cancelled);
        }
        fs::rename(&temporary_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn validate_graph(graph: &FileBatchGraph) -> Result<(), FormatError> {
    if graph.version != BATCH_GRAPH_VERSION {
        return Err(FormatError::Invalid("batch graph version is unsupported"));
    }
    validate_string(&graph.name, "batch graph name")?;
    if graph.inputs.is_empty() || graph.inputs.len() > MAX_BATCH_INPUTS {
        return Err(FormatError::Invalid("batch input count is outside bounds"));
    }
    if graph.operations.is_empty() || graph.operations.len() > MAX_BATCH_OPERATIONS {
        return Err(FormatError::Invalid(
            "batch operation count is outside bounds",
        ));
    }
    for input in &graph.inputs {
        validate_string(&input.path, "batch input path")?;
        if input.first_cell != 0 && input.last_cell != 0 && input.first_cell > input.last_cell {
            return Err(FormatError::Invalid("batch input range is reversed"));
        }
    }
    for operation in &graph.operations {
        if operation.version == 0 {
            return Err(FormatError::Invalid("batch operation version is zero"));
        }
        if operation.payload.len() > MAX_OPERATION_PAYLOAD_BYTES {
            return Err(FormatError::Invalid(
                "batch operation payload exceeds the bounded size",
            ));
        }
    }
    validate_string(&graph.output.folder, "batch output folder")?;
    validate_string(&graph.output.basename, "batch output basename")?;
    if graph.output.wait_milliseconds > 3_600_000 {
        return Err(FormatError::Invalid("batch wait duration exceeds one hour"));
    }
    Ok(())
}

fn validate_string(value: &str, field: &'static str) -> Result<(), FormatError> {
    if value.len() > MAX_BATCH_STRING_BYTES || value.as_bytes().contains(&0) {
        return Err(FormatError::Invalid(field));
    }
    Ok(())
}

fn create_temporary(path: &Path) -> Result<(PathBuf, std::fs::File), FormatError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    path.file_name()
        .ok_or(FormatError::Invalid("batch destination has no file name"))?;
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_path =
            parent.join(format!(".inkbatch.tmp.{}.{}", std::process::id(), sequence));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a batch temporary file",
    )
    .into())
}

fn bounded_count(value: u32, maximum: usize, field: &'static str) -> Result<usize, FormatError> {
    let value = value as usize;
    if value > maximum {
        return Err(FormatError::Invalid(field));
    }
    Ok(value)
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<(), FormatError> {
    let length = u32::try_from(value.len())
        .map_err(|_| FormatError::Invalid("batch byte span length is not representable"))?;
    push_u32(output, length);
    output.extend_from_slice(value);
    Ok(())
}

fn push_string(output: &mut Vec<u8>, value: &str) -> Result<(), FormatError> {
    push_bytes(output, value.as_bytes())
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(FormatError::Invalid("batch graph offset overflows"))?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or(FormatError::Invalid("batch graph is truncated"))?;
        self.cursor = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, FormatError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| FormatError::Invalid("batch u32 is truncated"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, FormatError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| FormatError::Invalid("batch u64 is truncated"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, FormatError> {
        let length = bounded_count(
            self.u32()?,
            MAX_OPERATION_PAYLOAD_BYTES,
            "batch payload length",
        )?;
        Ok(self.take(length)?.to_vec())
    }

    fn string(&mut self) -> Result<String, FormatError> {
        let length = bounded_count(self.u32()?, MAX_BATCH_STRING_BYTES, "batch string length")?;
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| FormatError::Invalid("batch string is not UTF-8"))
    }

    fn is_empty(&self) -> bool {
        self.cursor == self.bytes.len()
    }
}

#[cfg(test)]
#[path = "../tests/unit/batch.rs"]
mod tests;
