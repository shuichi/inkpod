use super::compile::StaticScriptProgram;
use super::execute::{ScriptRunError, run_inkscript_on_staged_core};
use super::output::{encode_output, materialize, raster_format};
use super::plan::{
    PlannedInputSource, ScriptCommandContext, ScriptConfirmationToken, ScriptExecutionPlan,
};
use super::run::{
    ScriptItemFailure, ScriptItemOutcome, ScriptRunAdapter, ScriptRunAdapterError,
    ScriptRunItemReport, ScriptRunReport, validate_runtime_state, validate_source,
};
use crate::batch::{ContactSheetSlot, compose_contact_sheet, contact_sheet_layout_with_limit};
use crate::{
    AssetAlphaSemantics, AssetColorSpace, Core, CoreError, DEFAULT_DPI_MILLI, PixelFormat,
    RasterAssetInput,
};
use inkpod_format::{
    InkScriptExecutionFailure, InkScriptInputProfile, InkScriptOutputFormat, decode_procedure_file,
    encode_procedure_file,
};
use inkpod_io::{IoManager, JobContext};
use std::io::Write;

/// Caller-lowerable bounds for isolated image preview storage and RGBA8 composition.
#[derive(Clone, Copy, Debug)]
pub struct ScriptImagePreviewLimits {
    temporary_bytes: u64,
    pixels: u64,
}
impl ScriptImagePreviewLimits {
    /// Uses the product limits: 4 GiB temporary files and 16,777,216 contact-sheet pixels.
    pub const fn exact_current() -> Self {
        Self {
            temporary_bytes: 4 * 1024 * 1024 * 1024,
            pixels: 16_777_216,
        }
    }
    /// Lowers the aggregate encoded input/output budget; cannot raise the product bound.
    pub const fn with_temporary_bytes(mut self, maximum: u64) -> Self {
        self.temporary_bytes = if maximum < self.temporary_bytes {
            maximum
        } else {
            self.temporary_bytes
        };
        self
    }
    /// Lowers the contact-sheet pixel count bound.
    pub const fn with_pixels(mut self, maximum: u64) -> Self {
        self.pixels = if maximum < self.pixels {
            maximum
        } else {
            self.pixels
        };
        self
    }
}

/// Preview failure. No display result is published for cancellation, stale input or cleanup failure.
#[derive(Debug)]
pub enum ScriptImagePreviewError {
    /// Source, plan or one-shot confirmation do not match an all-input preview.
    Plan,
    /// Cooperative cancellation was observed before publication.
    Cancelled,
    /// Authority or original input context changed.
    Stale(ScriptItemFailure),
    /// Input fingerprint/read failed; retains the adapter cause without asserting source change.
    InputRead(ScriptRunAdapterError),
    /// A caller-lowered or product resource bound was exceeded.
    ResourceLimit,
    /// Copy/materialization/cleanup failed without publishing a preview.
    Io(CoreError),
}
impl From<ScriptRunAdapterError> for ScriptImagePreviewError {
    fn from(error: ScriptRunAdapterError) -> Self {
        match error {
            ScriptRunAdapterError::Cancelled => Self::Cancelled,
            error => Self::InputRead(error),
        }
    }
}
impl From<CoreError> for ScriptImagePreviewError {
    fn from(value: CoreError) -> Self {
        if matches!(value, CoreError::Cancelled) {
            Self::Cancelled
        } else {
            Self::Io(value)
        }
    }
}
impl From<inkpod_io::IoError> for ScriptImagePreviewError {
    fn from(value: inkpod_io::IoError) -> Self {
        match value {
            inkpod_io::IoError::Cancelled => Self::Cancelled,
            inkpod_io::IoError::LimitExceeded(_) => Self::ResourceLimit,
            error => Self::Io(error.into()),
        }
    }
}

/// A clean, pathless display-only contact sheet and the original issue-time context.
/// The filesystem temporary has already been removed when this value is returned.
#[derive(Debug)]
pub struct ScriptImagePreviewResult {
    core: Core,
    report: ScriptRunReport,
    origin: ScriptCommandContext,
}
impl ScriptImagePreviewResult {
    /// Borrows the display-only document; this is not an active-output candidate.
    pub const fn core(&self) -> &Core {
        &self.core
    }
    /// Returns per-input outcomes, retaining failures and Stop-policy unprocessed slots.
    pub const fn report(&self) -> &ScriptRunReport {
        &self.report
    }
    /// Returns the original command target for the frontend's preview-tab context.
    pub const fn origin(&self) -> &ScriptCommandContext {
        &self.origin
    }
    /// Transfers the display document and its original target together exactly once.
    pub fn into_display(self) -> (Core, ScriptCommandContext) {
        (self.core, self.origin)
    }
}

/// Copies/materializes every input before executing any invocation, saves and reopens each
/// result using the selected output codec, then cleans temporary storage before publication.
/// Neither real output destinations nor live source documents are changed. The caller runs
/// this on its worker, forwards progress, and must retain `origin()` when showing the result.
#[allow(clippy::too_many_arguments)]
pub fn preview_inkscript_images(
    program: &StaticScriptProgram,
    plan: &ScriptExecutionPlan,
    confirmation: &mut ScriptConfirmationToken,
    manager: &IoManager,
    adapter: &mut dyn ScriptRunAdapter,
    limits: ScriptImagePreviewLimits,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<ScriptImagePreviewResult, ScriptImagePreviewError> {
    if !plan.matches_program(program) {
        return Err(ScriptImagePreviewError::Plan);
    }
    let confirmation = confirmation
        .consume_for_run(plan)
        .map_err(|_| ScriptImagePreviewError::Plan)?;
    if !matches!(confirmation.scope(), super::plan::ScriptRunScope::All) {
        return Err(ScriptImagePreviewError::Plan);
    }
    validate_runtime_state(plan, &confirmation, adapter).map_err(ScriptImagePreviewError::Stale)?;
    let total = (plan.input_count() as u64)
        .checked_mul(3)
        .and_then(|n| n.checked_add(1))
        .ok_or(ScriptImagePreviewError::ResourceLimit)?;
    if !progress(0, total) {
        return Err(ScriptImagePreviewError::Cancelled);
    }
    let context = JobContext::new();
    let temporary = manager.create_temporary_directory("inkpod-script-preview", &context)?;
    let result = (|| {
        let layout = contact_sheet_layout_with_limit(plan.input_count(), limits.pixels)
            .map_err(|_| ScriptImagePreviewError::ResourceLimit)?;
        let mut temporary_bytes = 0_u64;
        let mut copied = Vec::new();
        let mut completed = 0;
        for (ordinal, item) in plan.items().iter().enumerate() {
            if !progress(completed, total) {
                return Err(ScriptImagePreviewError::Cancelled);
            }
            let (bytes, extension) = match item.source() {
                PlannedInputSource::File(expected) => {
                    let observed = adapter
                        .fingerprint_native(expected)
                        .map_err(ScriptImagePreviewError::from)?;
                    if !observed.matches_exact(expected) {
                        return Err(ScriptImagePreviewError::Stale(
                            ScriptItemFailure::StaleInput,
                        ));
                    }
                    let read = adapter
                        .read_native(expected, &mut || !progress(completed, total))
                        .map_err(ScriptImagePreviewError::from)?;
                    if !read.matches(expected) {
                        return Err(ScriptImagePreviewError::Stale(
                            ScriptItemFailure::StaleInput,
                        ));
                    }
                    (
                        read.into_bytes(),
                        expected
                            .display_label()
                            .rsplit('.')
                            .next()
                            .unwrap_or("inkpod")
                            .to_owned(),
                    )
                }
                PlannedInputSource::Session(snapshot) => {
                    validate_source(item, adapter).map_err(ScriptImagePreviewError::Stale)?;
                    let core = snapshot.clone_staged_core().map_err(|_| {
                        ScriptImagePreviewError::Stale(ScriptItemFailure::StaleInput)
                    })?;
                    let core = if program.envelope.input_profile() == InkScriptInputProfile::Batch {
                        materialize(&core)?
                    } else {
                        core
                    };
                    // Snapshot materialization preserves canonical history/editor/savepoints;
                    // batch materialize intentionally reconstructs those before serialization.
                    let bytes = encode_procedure_file(
                        &core.build_procedure_file(
                            core.savepoint,
                            core.editor_session
                                .as_ref()
                                .and_then(|editor| editor.savepoint),
                        )?,
                    )
                    .map_err(|error| CoreError::Format(error.to_string()))?;
                    (bytes, "inkpod".to_owned())
                }
            };
            let path = temporary
                .path()
                .join(format!("input-{ordinal:05}.{extension}"));
            write_temporary(
                manager,
                &context,
                &path,
                &bytes,
                &mut temporary_bytes,
                limits.temporary_bytes,
                &mut || !progress(completed, total),
            )?;
            copied.push(path);
            completed += 1;
        }
        let mut slots = vec![ContactSheetSlot::Unprocessed; copied.len()];
        let mut report = ScriptRunReport {
            dry_run: false,
            cancelled: false,
            created_directories: Vec::new(),
            items: plan
                .items()
                .iter()
                .enumerate()
                .map(|(ordinal, item)| ScriptRunItemReport {
                    ordinal,
                    input_label: item.display_label().to_owned(),
                    destination_key: String::new(),
                    outcome: ScriptItemOutcome::NotStarted,
                    execution: None,
                })
                .collect(),
        };
        for (ordinal, path) in copied.iter().enumerate() {
            if !progress(completed, total) {
                return Err(ScriptImagePreviewError::Cancelled);
            }
            let result = (|| {
                let bytes = manager
                    .read_bytes(path, limits.temporary_bytes, &context)
                    .map_err(PreviewItemError::from)?;
                let mut working = decode_copy(
                    bytes.bytes(),
                    path.extension().and_then(|v| v.to_str()).unwrap_or(""),
                    plan.items()[ordinal].document_uuid(),
                )
                .map_err(|_| PreviewItemError::Failed(ScriptItemFailure::Decode))?;
                working
                    .bind_file_io(manager.clone())
                    .map_err(|_| PreviewItemError::Failed(ScriptItemFailure::Adapter))?;
                let executed = run_inkscript_on_staged_core(
                    program,
                    working,
                    Some(plan.frozen_assets()),
                    &mut || !progress(completed, total),
                )
                .map_err(|error| match error {
                    ScriptRunError::Cancelled => PreviewItemError::Cancelled,
                    ScriptRunError::ResourceLimit => PreviewItemError::ResourceLimit,
                    _ => PreviewItemError::Failed(ScriptItemFailure::Execute),
                })?;
                let format = program.envelope.output().format();
                let encoded = encode_output(&executed.staged, format)
                    .map_err(|_| PreviewItemError::Failed(ScriptItemFailure::Encode))?;
                let output_path = temporary
                    .path()
                    .join(format!("output-{ordinal:05}.{}", extension(format)));
                write_temporary(
                    manager,
                    &context,
                    &output_path,
                    &encoded,
                    &mut temporary_bytes,
                    limits.temporary_bytes,
                    &mut || !progress(completed, total),
                )
                .map_err(|error| match error {
                    ScriptImagePreviewError::Cancelled => PreviewItemError::Cancelled,
                    ScriptImagePreviewError::ResourceLimit => PreviewItemError::ResourceLimit,
                    _ => PreviewItemError::Failed(ScriptItemFailure::Save),
                })?;
                completed += 1;
                if !progress(completed, total) {
                    return Err(PreviewItemError::Cancelled);
                }
                let bytes = manager
                    .read_bytes(&output_path, limits.temporary_bytes, &context)
                    .map_err(PreviewItemError::from)?;
                let reopened = decode_copy(
                    bytes.bytes(),
                    extension(format),
                    plan.items()[ordinal].document_uuid(),
                )
                .map_err(|_| PreviewItemError::Failed(ScriptItemFailure::Decode))?;
                Ok::<_, PreviewItemError>((
                    reopened
                        .document_thumbnail_with_max(layout.thumbnail_maximum_dimension)
                        .map_err(|_| PreviewItemError::Failed(ScriptItemFailure::Encode))?,
                    executed.report,
                ))
            })();
            match result {
                Ok((thumbnail, execution)) => {
                    slots[ordinal] = ContactSheetSlot::Thumbnail(thumbnail);
                    report.items[ordinal].outcome = ScriptItemOutcome::Staged;
                    report.items[ordinal].execution = Some(execution);
                }
                Err(PreviewItemError::Cancelled) => return Err(ScriptImagePreviewError::Cancelled),
                Err(PreviewItemError::ResourceLimit) => {
                    return Err(ScriptImagePreviewError::ResourceLimit);
                }
                Err(PreviewItemError::Failed(failure)) => {
                    slots[ordinal] = ContactSheetSlot::Failed;
                    report.items[ordinal].outcome = ScriptItemOutcome::Failed(failure);
                    if program.envelope.execution().failure() == InkScriptExecutionFailure::Stop {
                        break;
                    }
                }
            }
            completed += 1;
        }
        let (pixels, uuid) = compose_contact_sheet(&layout, &slots)?;
        let mut core = Core::new();
        core.bind_file_io(manager.clone())?;
        core.new_cell_from_raster_asset(
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
            uuid,
        )?;
        if !progress(total, total) {
            return Err(ScriptImagePreviewError::Cancelled);
        }
        validate_runtime_state(plan, &confirmation, adapter)
            .map_err(ScriptImagePreviewError::Stale)?;
        for item in plan.items() {
            if matches!(item.source(), PlannedInputSource::Session(_)) {
                validate_source(item, adapter).map_err(ScriptImagePreviewError::Stale)?;
            }
        }
        Ok(ScriptImagePreviewResult {
            core,
            report,
            origin: plan.command_context().clone(),
        })
    })();
    temporary.cleanup()?;
    let result = result?;
    if !progress(total, total) {
        return Err(ScriptImagePreviewError::Cancelled);
    }
    validate_runtime_state(plan, &confirmation, adapter).map_err(ScriptImagePreviewError::Stale)?;
    for item in plan.items() {
        if matches!(item.source(), PlannedInputSource::Session(_)) {
            validate_source(item, adapter).map_err(ScriptImagePreviewError::Stale)?;
        }
    }
    Ok(result)
}

enum PreviewItemError {
    Cancelled,
    ResourceLimit,
    Failed(ScriptItemFailure),
}
impl From<inkpod_io::IoError> for PreviewItemError {
    fn from(error: inkpod_io::IoError) -> Self {
        match error {
            inkpod_io::IoError::Cancelled => Self::Cancelled,
            inkpod_io::IoError::LimitExceeded(_) => Self::ResourceLimit,
            _ => Self::Failed(ScriptItemFailure::Save),
        }
    }
}

fn decode_copy(bytes: &[u8], extension: &str, uuid: u128) -> Result<Core, CoreError> {
    if extension.eq_ignore_ascii_case("inkpod") {
        Core::from_procedure_file(
            decode_procedure_file(bytes).map_err(|error| CoreError::Format(error.to_string()))?,
        )
    } else {
        let format = inkpod_format::CommonRasterFormat::from_extension(extension)
            .ok_or(CoreError::InvalidArgument("unsupported preview codec"))?;
        let mut core = Core::new();
        core.import_common_raster(format, bytes, uuid)?;
        Ok(core)
    }
}

fn extension(format: InkScriptOutputFormat) -> &'static str {
    match raster_format(format) {
        None => "inkpod",
        Some(inkpod_format::CommonRasterFormat::Png) => "png",
        Some(inkpod_format::CommonRasterFormat::Tiff) => "tiff",
        Some(inkpod_format::CommonRasterFormat::Tga) => "tga",
        Some(inkpod_format::CommonRasterFormat::Bmp) => "bmp",
    }
}

#[allow(clippy::too_many_arguments)]
fn write_temporary(
    manager: &IoManager,
    context: &JobContext,
    path: &std::path::Path,
    bytes: &[u8],
    used: &mut u64,
    maximum: u64,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), ScriptImagePreviewError> {
    let length = bytes.len() as u64;
    let per_file = if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("inkpod"))
    {
        1024 * 1024 * 1024
    } else {
        512 * 1024 * 1024
    };
    if length > per_file || used.checked_add(length).is_none_or(|total| total > maximum) {
        return Err(ScriptImagePreviewError::ResourceLimit);
    }
    if cancelled() {
        return Err(ScriptImagePreviewError::Cancelled);
    }
    manager.write_new_atomic(path, context, |writer| {
        for chunk in bytes.chunks(64 * 1024) {
            if cancelled() {
                return Err(inkpod_io::IoError::Cancelled);
            }
            writer.write_all(chunk)?;
        }
        Ok(())
    })?;
    *used += length;
    Ok(())
}
