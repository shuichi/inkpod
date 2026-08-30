use super::model::{BatchSource, BatchSourceContent};
use super::operations::{save_batch_output_with_format, working_core};
use super::*;
use crate::Thumbnail;
use inkpod_io::{IoManager, JobContext};

const PREVIEW_TEMPORARY_BYTE_LIMIT: u64 = 4 * 1_024 * 1_024 * 1_024;
const PREVIEW_THUMBNAIL_MAXIMUM_DIMENSION: u32 = 160;
const PREVIEW_CELL_PADDING: u32 = 8;
const PREVIEW_DIRECTORY_NAME: &str = "inkpod-batch-preview";

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

impl Core {
    /// Runs every expanded Batch v5 input in isolated temporary storage and returns one
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
        progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        self.batch_contact_sheet_preview_with_context(graph, &JobContext::new(), progress)
    }

    pub(crate) fn batch_contact_sheet_preview_with_context(
        &self,
        graph: &BatchGraph,
        context: &JobContext,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<BatchRunReport, CoreError> {
        graph.validate()?;
        let mut progress = |completed, total| !context.is_cancelled() && progress(completed, total);
        let manager = self.file_io_manager()?;
        let processing_context = context.child();
        let sources = self.resolve_batch_sources(graph, BatchRunScope::All, &manager, context)?;
        let total = (sources.len() as u64)
            .checked_mul(3)
            .and_then(|value| value.checked_add(1))
            .ok_or(CoreError::InvalidState(
                "batch preview progress range overflows",
            ))?;
        if !progress(0, total) {
            return Err(CoreError::Cancelled);
        }

        let temporary = manager.create_temporary_directory(PREVIEW_DIRECTORY_NAME, context)?;
        let result = (|| {
            let input_directory = temporary.path().join("inputs");
            let output_directory = temporary.path().join("outputs");
            manager.create_dir(&input_directory, context)?;
            manager.create_dir(&output_directory, context)?;

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
                        copy_file_bounded(
                            &manager,
                            context,
                            path,
                            &destination,
                            &mut temporary_bytes,
                            || !progress(completed, total),
                        )?;
                        destination
                    }
                    BatchSourceContent::Document { .. } => {
                        let destination = input_directory.join(format!("{index:05}.inkpod"));
                        let working = working_core(source, &manager, &processing_context)?;
                        let length = save_batch_output_with_format(
                            &working,
                            BatchOutputFormat::Inkpod,
                            source,
                            &destination,
                            PREVIEW_TEMPORARY_BYTE_LIMIT - temporary_bytes,
                            &processing_context,
                            || !progress(completed, total),
                        )?;
                        temporary_bytes += length;
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
                    let mut working = working_core(source, &manager, &processing_context)?;
                    // Count each successfully decoded input once. Temporary
                    // output readback and the copy's extra decode are internal
                    // work, not additional source images in the public total.
                    context.record_loaded();
                    working.apply_batch_operations(&graph.operations, || {
                        !progress(completed, total)
                    })?;
                    let output_path = output_directory.join(format!(
                        "{index:05}.{}",
                        batch_output_extension(output_format)
                    ));
                    let length = save_batch_output_with_format(
                        &working,
                        output_format,
                        source,
                        &output_path,
                        PREVIEW_TEMPORARY_BYTE_LIMIT - temporary_bytes,
                        &processing_context,
                        || !progress(completed, total),
                    )?;
                    temporary_bytes += length;
                    completed += 1;
                    if !progress(completed, total) {
                        return Err(CoreError::Cancelled);
                    }

                    let output_source = BatchSource {
                        label: source.label.clone(),
                        input_path: Some(output_path.clone()),
                        content: BatchSourceContent::Path(output_path),
                    };
                    let reopened = working_core(&output_source, &manager, &processing_context)?;
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
            staged.bind_file_io(manager.clone())?;
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

        let cleanup = temporary.cleanup().map_err(CoreError::from);
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
    manager: &IoManager,
    context: &JobContext,
    source: &Path,
    destination: &Path,
    temporary_bytes: &mut u64,
    is_cancelled: impl FnMut() -> bool,
) -> Result<(), CoreError> {
    let per_file_limit = if source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("inkpod"))
    {
        1024 * 1024 * 1024
    } else {
        512 * 1024 * 1024
    };
    let remaining = PREVIEW_TEMPORARY_BYTE_LIMIT
        .checked_sub(*temporary_bytes)
        .ok_or(CoreError::InvalidState(
            "batch preview temporary storage exceeds the 4 GiB limit",
        ))?;
    let length = manager.copy_file_with_cancel(
        source,
        destination,
        remaining.min(per_file_limit),
        context,
        is_cancelled,
    )?;
    *temporary_bytes += length;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn dedicated_preview_directory_is_removed_by_explicit_and_drop_cleanup() {
        let manager = IoManager::new(inkpod_io::IoConfig::default()).unwrap();
        let context = JobContext::new();
        let temporary = manager
            .create_temporary_directory(PREVIEW_DIRECTORY_NAME, &context)
            .unwrap();
        let explicit_root = temporary.path().to_path_buf();
        fs::write(explicit_root.join("owned.tmp"), b"preview").unwrap();
        temporary.cleanup().unwrap();
        assert!(!explicit_root.exists());

        let dropped_root = {
            let temporary = manager
                .create_temporary_directory(PREVIEW_DIRECTORY_NAME, &context)
                .unwrap();
            let root = temporary.path().to_path_buf();
            fs::write(root.join("owned.tmp"), b"preview").unwrap();
            root
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while dropped_root.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!dropped_root.exists());
    }
}
