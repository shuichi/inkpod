use super::model::{BatchSource, BatchSourceContent};
use super::operations::{save_batch_output_with_format, working_core};
use super::*;
use crate::Thumbnail;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

const PREVIEW_TEMPORARY_BYTE_LIMIT: u64 = 4 * 1_024 * 1_024 * 1_024;
const PREVIEW_THUMBNAIL_MAXIMUM_DIMENSION: u32 = 160;
const PREVIEW_CELL_PADDING: u32 = 8;
const PREVIEW_COPY_BUFFER_BYTES: usize = 64 * 1_024;
const PREVIEW_DIRECTORY_ATTEMPTS: u64 = 1_024;
const PREVIEW_DIRECTORY_NAME: &str = "inkpod-batch-preview";

static PREVIEW_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
enum ContactSheetSlot {
    Thumbnail(Thumbnail),
    Failed,
    Unprocessed,
}

struct ContactSheetLayout {
    columns: u32,
    cell_size: u32,
    padding: u32,
    width: u32,
    height: u32,
    thumbnail_maximum_dimension: u32,
}

struct PreviewTemporaryDirectory {
    base: PathBuf,
    root: PathBuf,
    active: bool,
}

impl PreviewTemporaryDirectory {
    fn create() -> Result<Self, CoreError> {
        let base = std::env::temp_dir().join(PREVIEW_DIRECTORY_NAME);
        fs::create_dir_all(&base).map_err(io_error)?;
        for _ in 0..PREVIEW_DIRECTORY_ATTEMPTS {
            let sequence = PREVIEW_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = base.join(format!("{}-{sequence}", std::process::id()));
            match fs::create_dir(&root) {
                Ok(()) => {
                    return Ok(Self {
                        base,
                        root,
                        active: true,
                    });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        Err(CoreError::InvalidState(
            "batch preview temporary directory could not be reserved",
        ))
    }

    fn inputs(&self) -> PathBuf {
        self.root.join("inputs")
    }

    fn outputs(&self) -> PathBuf {
        self.root.join("outputs")
    }

    fn cleanup(mut self) -> Result<(), CoreError> {
        self.validate_cleanup_target()?;
        fs::remove_dir_all(&self.root).map_err(io_error)?;
        self.active = false;
        let _ = fs::remove_dir(&self.base);
        Ok(())
    }

    fn validate_cleanup_target(&self) -> Result<(), CoreError> {
        if self.root.parent() != Some(self.base.as_path())
            || self.base.file_name().and_then(|name| name.to_str()) != Some(PREVIEW_DIRECTORY_NAME)
        {
            return Err(CoreError::InvalidState(
                "batch preview cleanup target escaped its dedicated temporary root",
            ));
        }
        Ok(())
    }
}

impl Drop for PreviewTemporaryDirectory {
    fn drop(&mut self) {
        if self.active && self.validate_cleanup_target().is_ok() {
            let _ = fs::remove_dir_all(&self.root);
            let _ = fs::remove_dir(&self.base);
        }
    }
}

impl Core {
    /// Runs every expanded Batch v3 input in isolated temporary storage and returns one
    /// clean, pathless contact-sheet document as a staged result.
    ///
    /// All file and active-document inputs are fully copied or materialized before the
    /// first operation is applied. The configured real output destination is never
    /// written. Cancellation returns no staged result, and temporary storage must be
    /// removed before a successful report is published. This query does not change the
    /// receiver's document, revisions, history, dirty state, or savepoint.
    pub fn batch_contact_sheet_preview(
        &self,
        graph: &BatchGraph,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        graph.validate()?;
        let sources = self.resolve_batch_sources(graph, BatchRunScope::All)?;
        let total = (sources.len() as u64)
            .checked_mul(3)
            .and_then(|value| value.checked_add(1))
            .ok_or(CoreError::InvalidState(
                "batch preview progress range overflows",
            ))?;
        if !progress(0, total) {
            return Err(CoreError::Cancelled);
        }

        let temporary = PreviewTemporaryDirectory::create()?;
        let result = (|| {
            let input_directory = temporary.inputs();
            let output_directory = temporary.outputs();
            fs::create_dir(&input_directory).map_err(io_error)?;
            fs::create_dir(&output_directory).map_err(io_error)?;

            let mut completed = 0_u64;
            let mut temporary_bytes = 0_u64;
            let mut copied_sources = Vec::with_capacity(sources.len());
            for (index, source) in sources.iter().enumerate() {
                if !progress(completed, total) {
                    return Err(CoreError::Cancelled);
                }
                let copied_path = match &source.content {
                    BatchSourceContent::Path(path) => {
                        let extension = path.extension().and_then(|value| value.to_str()).ok_or(
                            CoreError::InvalidArgument(
                                "batch preview input extension is unavailable",
                            ),
                        )?;
                        let destination = input_directory.join(format!("{index:05}.{extension}"));
                        copy_file_bounded(path, &destination, &mut temporary_bytes, || {
                            !progress(completed, total)
                        })?;
                        destination
                    }
                    BatchSourceContent::Document { .. } => {
                        let destination = input_directory.join(format!("{index:05}.inkpod"));
                        let working = working_core(source)?;
                        save_batch_output_with_format(
                            &working,
                            BatchOutputFormat::Inkpod,
                            source,
                            &destination,
                            || !progress(completed, total),
                        )?;
                        add_file_to_budget(&destination, &mut temporary_bytes)?;
                        destination
                    }
                };
                copied_sources.push(BatchSource {
                    label: source.label.clone(),
                    input_path: Some(copied_path.clone()),
                    content: BatchSourceContent::Path(copied_path),
                });
                completed += 1;
                if !progress(completed, total) {
                    return Err(CoreError::Cancelled);
                }
            }

            let layout = contact_sheet_layout(copied_sources.len())?;
            let mut slots = vec![ContactSheetSlot::Unprocessed; copied_sources.len()];
            let mut report = BatchRunReport {
                items: Vec::with_capacity(copied_sources.len()),
                cancelled: false,
                staged_results: Vec::new(),
            };
            let output_format = if graph.output.destination == BatchOutputDestination::Folder {
                graph.output.format
            } else {
                BatchOutputFormat::Inkpod
            };
            for (index, source) in copied_sources.iter().enumerate() {
                if !progress(completed, total) {
                    return Err(CoreError::Cancelled);
                }
                let item_start_completed = completed;
                let item_result = (|| {
                    let mut working = working_core(source)?;
                    working.apply_batch_operations(&graph.operations, || {
                        !progress(completed, total)
                    })?;
                    let output_path = output_directory.join(format!(
                        "{index:05}.{}",
                        batch_output_extension(output_format)
                    ));
                    save_batch_output_with_format(
                        &working,
                        output_format,
                        source,
                        &output_path,
                        || !progress(completed, total),
                    )?;
                    add_file_to_budget(&output_path, &mut temporary_bytes)?;
                    completed += 1;
                    if !progress(completed, total) {
                        return Err(CoreError::Cancelled);
                    }

                    let output_source = BatchSource {
                        label: source.label.clone(),
                        input_path: Some(output_path.clone()),
                        content: BatchSourceContent::Path(output_path),
                    };
                    let reopened = working_core(&output_source)?;
                    let thumbnail =
                        reopened.document_thumbnail_with_max(layout.thumbnail_maximum_dimension)?;
                    Ok::<Thumbnail, CoreError>(thumbnail)
                })();

                match item_result {
                    Ok(thumbnail) => {
                        slots[index] = ContactSheetSlot::Thumbnail(thumbnail);
                        report.items.push(BatchItemResult {
                            input_name: sources[index].label.clone(),
                            output_path: None,
                            outcome: BatchItemOutcome::Succeeded,
                            message: "preview completed in isolated temporary storage".to_owned(),
                        });
                    }
                    Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                    Err(error) => {
                        slots[index] = ContactSheetSlot::Failed;
                        report.items.push(BatchItemResult {
                            input_name: sources[index].label.clone(),
                            output_path: None,
                            outcome: BatchItemOutcome::Failed,
                            message: error.to_string(),
                        });
                        if graph.output.failure_policy == BatchFailurePolicy::Stop {
                            break;
                        }
                        completed = item_start_completed + 1;
                    }
                }
                completed += 1;
                if !progress(completed, total) {
                    return Err(CoreError::Cancelled);
                }
            }

            let (pixels, document_uuid) = compose_contact_sheet(&layout, &slots)?;
            let mut staged = Core::new();
            staged.new_cell_from_raster_asset(
                RasterAssetInput {
                    width: layout.width,
                    height: layout.height,
                    pixel_format: PixelFormat::StraightRgba8,
                    color_space: Some(AssetColorSpace::Srgb),
                    alpha_semantics: AssetAlphaSemantics::Straight,
                    canonical_stride: u64::from(layout.width) * 4,
                    pixels,
                    expected_id: None,
                },
                DEFAULT_DPI_MILLI,
                DEFAULT_DPI_MILLI,
                document_uuid,
            )?;
            if !progress(total, total) {
                return Err(CoreError::Cancelled);
            }
            report.staged_results.push(BatchStagedResult {
                generation: 1,
                core: Box::new(staged),
            });
            Ok(report)
        })();

        let cleanup = temporary.cleanup();
        match (result, cleanup) {
            (Ok(report), Ok(())) => Ok(report),
            (_, Err(error)) => Err(error),
            (Err(error), Ok(())) => Err(error),
        }
    }
}

const fn batch_output_extension(format: BatchOutputFormat) -> &'static str {
    match format {
        BatchOutputFormat::Inkpod => "inkpod",
        BatchOutputFormat::Png => "png",
        BatchOutputFormat::Tiff => "tiff",
        BatchOutputFormat::Tga => "tga",
        BatchOutputFormat::Bmp => "bmp",
    }
}

fn copy_file_bounded(
    source: &Path,
    destination: &Path,
    temporary_bytes: &mut u64,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(), CoreError> {
    let mut input = File::open(source).map_err(io_error)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(io_error)?;
    let mut buffer = [0_u8; PREVIEW_COPY_BUFFER_BYTES];
    loop {
        if is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let read = input.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        let next = temporary_bytes
            .checked_add(read as u64)
            .ok_or(CoreError::InvalidState(
                "batch preview temporary byte count overflows",
            ))?;
        if next > PREVIEW_TEMPORARY_BYTE_LIMIT {
            return Err(CoreError::InvalidState(
                "batch preview temporary storage exceeds the 4 GiB limit",
            ));
        }
        output.write_all(&buffer[..read]).map_err(io_error)?;
        *temporary_bytes = next;
    }
    output.sync_all().map_err(io_error)?;
    Ok(())
}

fn add_file_to_budget(path: &Path, temporary_bytes: &mut u64) -> Result<(), CoreError> {
    let length = fs::metadata(path).map_err(io_error)?.len();
    let next = temporary_bytes
        .checked_add(length)
        .ok_or(CoreError::InvalidState(
            "batch preview temporary byte count overflows",
        ))?;
    if next > PREVIEW_TEMPORARY_BYTE_LIMIT {
        return Err(CoreError::InvalidState(
            "batch preview temporary storage exceeds the 4 GiB limit",
        ));
    }
    *temporary_bytes = next;
    Ok(())
}

fn contact_sheet_layout(item_count: usize) -> Result<ContactSheetLayout, CoreError> {
    if item_count == 0 {
        return Err(CoreError::InvalidArgument(
            "batch preview contact sheet requires an item",
        ));
    }
    let count = u32::try_from(item_count)
        .map_err(|_| CoreError::InvalidState("batch preview item count is not representable"))?;
    let mut columns = 1_u32;
    while u64::from(columns) * u64::from(columns) < u64::from(count) {
        columns = columns.checked_add(1).ok_or(CoreError::InvalidState(
            "batch preview contact-sheet column count overflows",
        ))?;
    }
    let rows = count.div_ceil(columns);
    let mut cell_size = PREVIEW_THUMBNAIL_MAXIMUM_DIMENSION + PREVIEW_CELL_PADDING * 2;
    loop {
        let width = u64::from(columns) * u64::from(cell_size);
        let height = u64::from(rows) * u64::from(cell_size);
        if width
            .checked_mul(height)
            .is_some_and(|pixels| pixels <= MAX_IMAGE_EDIT_PIXELS)
        {
            let padding = PREVIEW_CELL_PADDING.min(cell_size.saturating_sub(1) / 2);
            return Ok(ContactSheetLayout {
                columns,
                cell_size,
                padding,
                width: u32::try_from(width).map_err(|_| {
                    CoreError::InvalidState("batch preview contact-sheet width overflows")
                })?,
                height: u32::try_from(height).map_err(|_| {
                    CoreError::InvalidState("batch preview contact-sheet height overflows")
                })?,
                thumbnail_maximum_dimension: cell_size
                    .saturating_sub(padding.saturating_mul(2))
                    .max(1),
            });
        }
        cell_size = cell_size.checked_sub(1).ok_or(CoreError::InvalidState(
            "batch preview contact sheet exceeds the image limit",
        ))?;
    }
}

fn compose_contact_sheet(
    layout: &ContactSheetLayout,
    slots: &[ContactSheetSlot],
) -> Result<(Vec<u8>, u128), CoreError> {
    let pixel_count = u64::from(layout.width)
        .checked_mul(u64::from(layout.height))
        .ok_or(CoreError::InvalidState(
            "batch preview contact-sheet pixel count overflows",
        ))?;
    if pixel_count > MAX_IMAGE_EDIT_PIXELS {
        return Err(CoreError::InvalidState(
            "batch preview contact sheet exceeds the image limit",
        ));
    }
    let byte_count = pixel_count
        .checked_mul(4)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(CoreError::InvalidState(
            "batch preview contact-sheet byte count overflows",
        ))?;
    let mut pixels = vec![0_u8; byte_count];
    for y in 0..layout.height {
        for x in 0..layout.width {
            let shade = if ((x / 8) + (y / 8)) % 2 == 0 { 48 } else { 64 };
            set_opaque_pixel(&mut pixels, layout.width, x, y, [shade, shade, shade]);
        }
    }

    for (index, slot) in slots.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| CoreError::InvalidState("batch preview contact-sheet index overflows"))?;
        let cell_x = (index % layout.columns) * layout.cell_size;
        let cell_y = (index / layout.columns) * layout.cell_size;
        match slot {
            ContactSheetSlot::Thumbnail(thumbnail) => {
                let start_x = cell_x + (layout.cell_size - thumbnail.width) / 2;
                let start_y = cell_y + (layout.cell_size - thumbnail.height) / 2;
                blit_thumbnail(&mut pixels, layout.width, start_x, start_y, thumbnail)?;
            }
            ContactSheetSlot::Failed => {
                draw_placeholder(&mut pixels, layout, cell_x, cell_y, [138, 42, 48], true)
            }
            ContactSheetSlot::Unprocessed => {
                draw_placeholder(&mut pixels, layout, cell_x, cell_y, [88, 88, 92], false)
            }
        }
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"org.inkpod.batch-contact-sheet.v1");
    hasher.update(&layout.width.to_le_bytes());
    hasher.update(&layout.height.to_le_bytes());
    hasher.update(&pixels);
    let digest = hasher.finalize();
    let mut uuid_bytes = [0_u8; 16];
    uuid_bytes.copy_from_slice(&digest.as_bytes()[..16]);
    let document_uuid = u128::from_le_bytes(uuid_bytes).max(1);
    Ok((pixels, document_uuid))
}

fn blit_thumbnail(
    destination: &mut [u8],
    destination_width: u32,
    start_x: u32,
    start_y: u32,
    thumbnail: &Thumbnail,
) -> Result<(), CoreError> {
    let expected = u64::from(thumbnail.width)
        .checked_mul(u64::from(thumbnail.height))
        .and_then(|value| value.checked_mul(4))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(CoreError::InvalidState(
            "batch preview thumbnail byte count overflows",
        ))?;
    if thumbnail.rgba8.len() != expected {
        return Err(CoreError::InvalidState(
            "batch preview thumbnail payload length is invalid",
        ));
    }
    for y in 0..thumbnail.height {
        for x in 0..thumbnail.width {
            let source_index = ((y as usize * thumbnail.width as usize) + x as usize) * 4;
            let source = &thumbnail.rgba8[source_index..source_index + 4];
            let destination_index = (((start_y + y) as usize * destination_width as usize)
                + (start_x + x) as usize)
                * 4;
            let alpha = u32::from(source[3]);
            for channel in 0..3 {
                let foreground = u32::from(source[channel]);
                let background = u32::from(destination[destination_index + channel]);
                destination[destination_index + channel] =
                    ((foreground * alpha + background * (255 - alpha) + 127) / 255) as u8;
            }
            destination[destination_index + 3] = u8::MAX;
        }
    }
    Ok(())
}

fn draw_placeholder(
    pixels: &mut [u8],
    layout: &ContactSheetLayout,
    cell_x: u32,
    cell_y: u32,
    color: [u8; 3],
    crossed: bool,
) {
    let start = layout.padding;
    let end = layout.cell_size.saturating_sub(layout.padding);
    for y in start..end {
        for x in start..end {
            let mut pixel = color;
            if crossed && (x.abs_diff(y) <= 2 || x.abs_diff(layout.cell_size - 1 - y) <= 2) {
                pixel = [238, 220, 220];
            }
            set_opaque_pixel(pixels, layout.width, cell_x + x, cell_y + y, pixel);
        }
    }
}

fn set_opaque_pixel(pixels: &mut [u8], width: u32, x: u32, y: u32, rgb: [u8; 3]) {
    let index = ((y as usize * width as usize) + x as usize) * 4;
    pixels[index..index + 3].copy_from_slice(&rgb);
    pixels[index + 3] = u8::MAX;
}

fn io_error(error: std::io::Error) -> CoreError {
    CoreError::Format(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_preview_directory_is_removed_by_explicit_and_drop_cleanup() {
        let temporary = PreviewTemporaryDirectory::create().unwrap();
        let explicit_root = temporary.root.clone();
        fs::write(explicit_root.join("owned.tmp"), b"preview").unwrap();
        temporary.cleanup().unwrap();
        assert!(!explicit_root.exists());

        let dropped_root = {
            let temporary = PreviewTemporaryDirectory::create().unwrap();
            let root = temporary.root.clone();
            fs::write(root.join("owned.tmp"), b"preview").unwrap();
            root
        };
        assert!(!dropped_root.exists());
    }
}
