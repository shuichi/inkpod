use super::FormatError;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const PALETTE_FORMAT_VERSION: u32 = 1;
pub const COLOR_CHART_FORMAT_VERSION: u32 = 1;
pub const MAX_APPLICATION_COLORS: usize = 4_096;
pub const MAX_COLOR_CHART_NAME_BYTES: usize = 1_024;

const PALETTE_MAGIC: [u8; 8] = *b"INKPAL1\0";
const COLOR_CHART_MAGIC: [u8; 8] = *b"INKCHT1\0";
const COLOR_RECORD_BYTES: usize = 16;
const COLOR_RECORD_STRUCT_SIZE: u32 = COLOR_RECORD_BYTES as u32;
const COLOR_DEPTH_8: u32 = 8;
const COLOR_DEPTH_16: u32 = 16;
const MAX_APPLICATION_FILE_BYTES: u64 = 16 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationColor {
    pub depth: u32,
    pub red: u16,
    pub green: u16,
    pub blue: u16,
    pub alpha: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePalette {
    pub colors: Vec<ApplicationColor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileColorChartEntry {
    pub color: ApplicationColor,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileColorChart {
    pub entries: Vec<FileColorChartEntry>,
}

pub fn encode_palette(palette: &FilePalette) -> Result<Vec<u8>, FormatError> {
    validate_colors(&palette.colors)?;
    let capacity = 12_usize
        .checked_add(
            palette
                .colors
                .len()
                .checked_mul(COLOR_RECORD_BYTES)
                .ok_or(FormatError::Invalid("palette size overflows"))?,
        )
        .ok_or(FormatError::Invalid("palette size overflows"))?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(&PALETTE_MAGIC);
    push_u32(&mut bytes, palette.colors.len() as u32);
    for color in &palette.colors {
        push_color(&mut bytes, *color);
    }
    Ok(bytes)
}

pub fn decode_palette(bytes: &[u8]) -> Result<FilePalette, FormatError> {
    if bytes.len() as u64 > MAX_APPLICATION_FILE_BYTES {
        return Err(FormatError::Invalid("palette exceeds the bounded size"));
    }
    let mut reader = Reader::new(bytes);
    if reader.take(8)? != PALETTE_MAGIC {
        return Err(FormatError::Invalid(
            "palette version or magic is unsupported",
        ));
    }
    let count = bounded_count(reader.u32()?, "palette color count")?;
    let mut colors = Vec::with_capacity(count);
    for _ in 0..count {
        colors.push(reader.color()?);
    }
    if !reader.is_empty() {
        return Err(FormatError::Invalid("palette has trailing bytes"));
    }
    validate_colors(&colors)?;
    Ok(FilePalette { colors })
}

pub fn read_palette(path: &Path) -> Result<FilePalette, FormatError> {
    decode_palette(&read_bounded(path)?)
}

pub fn save_palette_atomic(path: &Path, palette: &FilePalette) -> Result<(), FormatError> {
    write_atomic(path, &encode_palette(palette)?, ".inkpalette.tmp")
}

pub fn encode_color_chart(chart: &FileColorChart) -> Result<Vec<u8>, FormatError> {
    if chart.entries.len() > MAX_APPLICATION_COLORS {
        return Err(FormatError::Invalid(
            "color chart entry count exceeds the limit",
        ));
    }
    let mut capacity = 12_usize;
    for entry in &chart.entries {
        validate_color(entry.color)?;
        validate_name(&entry.name)?;
        capacity = capacity
            .checked_add(COLOR_RECORD_BYTES + 4)
            .and_then(|value| value.checked_add(entry.name.len()))
            .ok_or(FormatError::Invalid("color chart size overflows"))?;
    }
    if capacity as u64 > MAX_APPLICATION_FILE_BYTES {
        return Err(FormatError::Invalid("color chart exceeds the bounded size"));
    }
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(&COLOR_CHART_MAGIC);
    push_u32(&mut bytes, chart.entries.len() as u32);
    for entry in &chart.entries {
        push_color(&mut bytes, entry.color);
        push_u32(&mut bytes, entry.name.len() as u32);
        bytes.extend_from_slice(entry.name.as_bytes());
    }
    Ok(bytes)
}

pub fn decode_color_chart(bytes: &[u8]) -> Result<FileColorChart, FormatError> {
    if bytes.len() as u64 > MAX_APPLICATION_FILE_BYTES {
        return Err(FormatError::Invalid("color chart exceeds the bounded size"));
    }
    let mut reader = Reader::new(bytes);
    if reader.take(8)? != COLOR_CHART_MAGIC {
        return Err(FormatError::Invalid(
            "color chart version or magic is unsupported",
        ));
    }
    let count = bounded_count(reader.u32()?, "color chart entry count")?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let color = reader.color()?;
        validate_color(color)?;
        let name_bytes = reader.u32()? as usize;
        if name_bytes == 0 || name_bytes > MAX_COLOR_CHART_NAME_BYTES {
            return Err(FormatError::Invalid(
                "color chart name length is outside bounds",
            ));
        }
        let name = std::str::from_utf8(reader.take(name_bytes)?)
            .map_err(|_| FormatError::Invalid("color chart name is not UTF-8"))?
            .to_owned();
        entries.push(FileColorChartEntry { color, name });
    }
    if !reader.is_empty() {
        return Err(FormatError::Invalid("color chart has trailing bytes"));
    }
    Ok(FileColorChart { entries })
}

pub fn read_color_chart(path: &Path) -> Result<FileColorChart, FormatError> {
    decode_color_chart(&read_bounded(path)?)
}

pub fn save_color_chart_atomic(path: &Path, chart: &FileColorChart) -> Result<(), FormatError> {
    write_atomic(path, &encode_color_chart(chart)?, ".inkchart.tmp")
}

fn validate_colors(colors: &[ApplicationColor]) -> Result<(), FormatError> {
    if colors.len() > MAX_APPLICATION_COLORS {
        return Err(FormatError::Invalid(
            "palette color count exceeds the limit",
        ));
    }
    for color in colors {
        validate_color(*color)?;
    }
    Ok(())
}

fn validate_color(color: ApplicationColor) -> Result<(), FormatError> {
    if color.depth != COLOR_DEPTH_8 && color.depth != COLOR_DEPTH_16 {
        return Err(FormatError::Invalid(
            "application color depth is unsupported",
        ));
    }
    if color.depth == COLOR_DEPTH_8
        && [color.red, color.green, color.blue, color.alpha]
            .into_iter()
            .any(|channel| channel > u16::from(u8::MAX))
    {
        return Err(FormatError::Invalid(
            "8-bit application color channel exceeds its depth",
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), FormatError> {
    if name.is_empty() || name.len() > MAX_COLOR_CHART_NAME_BYTES {
        Err(FormatError::Invalid(
            "color chart name length is outside bounds",
        ))
    } else {
        Ok(())
    }
}

fn push_color(output: &mut Vec<u8>, color: ApplicationColor) {
    push_u32(output, COLOR_RECORD_STRUCT_SIZE);
    push_u32(output, color.depth);
    output.extend_from_slice(&color.red.to_le_bytes());
    output.extend_from_slice(&color.green.to_le_bytes());
    output.extend_from_slice(&color.blue.to_le_bytes());
    output.extend_from_slice(&color.alpha.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn bounded_count(value: u32, field: &'static str) -> Result<usize, FormatError> {
    let value = value as usize;
    if value > MAX_APPLICATION_COLORS {
        Err(FormatError::Invalid(field))
    } else {
        Ok(value)
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, FormatError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_APPLICATION_FILE_BYTES {
        return Err(FormatError::Invalid(
            "application data file exceeds the bounded size",
        ));
    }
    let mut input = OpenOptions::new().read(true).open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut input)
        .take(MAX_APPLICATION_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_APPLICATION_FILE_BYTES {
        return Err(FormatError::Invalid(
            "application data file exceeds the bounded size",
        ));
    }
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8], suffix: &str) -> Result<(), FormatError> {
    let (temporary_path, mut temporary) = create_temporary(path, suffix)?;
    let result = (|| {
        temporary.write_all(bytes)?;
        temporary.flush()?;
        temporary.sync_all()?;
        drop(temporary);
        fs::rename(&temporary_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn create_temporary(path: &Path, suffix: &str) -> Result<(PathBuf, fs::File), FormatError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    path.file_name().ok_or(FormatError::Invalid(
        "application data destination has no file name",
    ))?;
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_path = parent.join(format!("{suffix}.{}.{}", std::process::id(), sequence));
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
        "could not reserve an application data temporary file",
    )
    .into())
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
            .ok_or(FormatError::Invalid("application data offset overflows"))?;
        let result = self
            .bytes
            .get(self.cursor..end)
            .ok_or(FormatError::Invalid("application data file is truncated"))?;
        self.cursor = end;
        Ok(result)
    }

    fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(
            |_| FormatError::Invalid("application u16 is truncated"),
        )?))
    }

    fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| FormatError::Invalid("application u32 is truncated"),
        )?))
    }

    fn color(&mut self) -> Result<ApplicationColor, FormatError> {
        if self.u32()? != COLOR_RECORD_STRUCT_SIZE {
            return Err(FormatError::Invalid(
                "application color record size is unsupported",
            ));
        }
        Ok(ApplicationColor {
            depth: self.u32()?,
            red: self.u16()?,
            green: self.u16()?,
            blue: self.u16()?,
            alpha: self.u16()?,
        })
    }

    fn is_empty(&self) -> bool {
        self.cursor == self.bytes.len()
    }
}

#[cfg(test)]
#[path = "../tests/unit/application_data.rs"]
mod tests;
