use super::model::{BatchSource, BatchSourceContent};
use super::validation::{empty_pixel, ensure_pixel_matches_format, validate_operation};
use super::*;
use crate::primitive::{CanonicalInvocation, InvocationResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OperationResult {
    Applied,
    Skipped,
}

pub(super) fn apply_operation(
    core: &mut Core,
    operation: &BatchOperation,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<OperationResult, CoreError> {
    validate_operation(operation)?;
    let target = match operation.target.as_ref() {
        Some(selector) => match resolve_target(core, selector)? {
            Some(target) => Some(target),
            None => return Ok(OperationResult::Skipped),
        },
        None => None,
    };
    let target_plane = || {
        target
            .and_then(|(_, plane)| plane)
            .ok_or(CoreError::InvalidArgument(
                "batch operation requires a target plane",
            ))
    };
    match &operation.kind {
        BatchOperationKind::ColorReplace(pairs) => {
            apply_color_replacement(core, target_plane()?, pairs, &mut progress)?;
        }
        BatchOperationKind::ContinuousFill(seeds) => {
            let (layer_id, plane_id) = target.ok_or(CoreError::InvalidArgument(
                "continuous fill requires a stable target",
            ))?;
            let plane_id = plane_id.ok_or(CoreError::InvalidArgument(
                "continuous fill requires a target plane",
            ))?;
            core.set_active_node(layer_id, plane_id)?;
            for (index, seed) in seeds.iter().enumerate() {
                if !progress(index as u64, seeds.len() as u64) {
                    return Err(CoreError::Cancelled);
                }
                core.apply_fill_with_cancel(
                    &FillRequest {
                        operation: FillOperation::Seed,
                        seed_x: seed.x,
                        seed_y: seed.y,
                        color: seed.color,
                        selection: None,
                        use_document_selection: false,
                        tolerance: seed.tolerance,
                        detached_regions: false,
                        overflow_abort: true,
                        gap_close: seed.gap_close,
                        transparent_only: false,
                        inclusion_mode: InclusionMode::None,
                        inclusion_colors: Vec::new(),
                        extension_distance: 0,
                    },
                    || !progress(index as u64, seeds.len() as u64),
                )?;
            }
        }
        BatchOperationKind::Separation(options) => {
            apply_separation(core, target_plane()?, options, &mut progress)?;
        }
        BatchOperationKind::Visibility { visible } => {
            let (layer_id, plane_id) = target.ok_or(CoreError::InvalidArgument(
                "visibility requires a stable target",
            ))?;
            let layers = core.layers()?;
            let layer = layers
                .iter()
                .find(|layer| layer.id == layer_id)
                .ok_or(CoreError::InvalidState("batch layer target disappeared"))?;
            if let Some(plane_id) = plane_id {
                let plane = layer
                    .planes
                    .iter()
                    .find(|plane| plane.id == plane_id)
                    .ok_or(CoreError::InvalidState("batch plane target disappeared"))?;
                core.set_plane_properties(
                    plane.id,
                    *visible,
                    plane.editable,
                    plane.opacity_milli,
                    &plane.name,
                )?;
            } else {
                core.set_layer_properties(
                    layer.id,
                    *visible,
                    layer.editable,
                    layer.opacity_milli,
                    &layer.name,
                )?;
            }
        }
        BatchOperationKind::LineWidth(mode) => {
            let plane_id = target_plane()?;
            let ids: Vec<_> = core
                .vector_paths()?
                .into_iter()
                .filter(|path| path.plane_id == plane_id)
                .map(|path| path.id)
                .collect();
            if ids.is_empty() {
                return Err(CoreError::InvalidArgument(
                    "line-width target has no vector paths",
                ));
            }
            core.vector_correct_width(&ids, *mode)?;
        }
        BatchOperationKind::Filter(filter) => {
            let plane_id = target_plane()?;
            core.begin_filter_preview_with_progress(plane_id, filter.clone(), &mut progress)?;
            core.apply_filter_preview()?;
        }
        BatchOperationKind::BoundaryAirbrush(effect) => {
            core.apply_boundary_airbrush_to_plane(target_plane()?, effect)?;
        }
        BatchOperationKind::DustRemoval(options) => {
            core.apply_dust_removal_to_plane(target_plane()?, None, *options, &mut progress)?;
        }
        BatchOperationKind::Mirror(axis) => {
            core.mirror_document(*axis)?;
        }
        BatchOperationKind::Rotate90(direction) => {
            core.rotate_document(*direction)?;
        }
        BatchOperationKind::Resize(resize) => {
            core.resize_document(*resize)?;
        }
        BatchOperationKind::ConvertPlane {
            destination_kind,
            destination_format,
        } => {
            core.convert_plane(target_plane()?, *destination_kind, *destination_format)?;
        }
    }
    Ok(OperationResult::Applied)
}

pub(crate) fn apply_color_replacement(
    core: &mut Core,
    plane_id: u64,
    pairs: &[BatchColorPair],
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<DispatchOutcome, CoreError> {
    if !core.canonical_invocation_is_active() {
        let pairs = pairs.to_vec();
        let staged_pairs = pairs.clone();
        return core
            .execute_canonical_invocation_with(
                CanonicalInvocation::ReplaceRasterColors { plane_id, pairs },
                move |staged| {
                    apply_color_replacement(staged, plane_id, &staged_pairs, progress)
                        .map(InvocationResult::dispatch)
                },
            )
            .map(|result| result.dispatch);
    }
    let plane_id = PlaneId::from_raw(plane_id);
    let base_revision = core.document_revision;
    let before = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
    let source = before
        .plane_by_id(plane_id)
        .ok_or(CoreError::InvalidArgument(
            "batch plane target does not exist",
        ))?;
    let work = u64::from(source.raster.width())
        .checked_mul(u64::from(source.raster.height()))
        .ok_or(CoreError::InvalidArgument("batch raster work overflows"))?;
    if work > MAX_IMAGE_EDIT_PIXELS {
        return Err(CoreError::InvalidArgument(
            "batch raster exceeds the bounded work limit",
        ));
    }
    let revision = core.next_document_revision()?;
    let mut after = before.clone();
    let raster = &mut after
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("batch plane target disappeared"))?
        .raster;
    let mut touched = BTreeSet::new();
    for y in 0..raster.height() {
        if !progress(u64::from(y), u64::from(raster.height()).max(1)) {
            return Err(CoreError::Cancelled);
        }
        for x in 0..raster.width() {
            let value = raster.pixel(x, y)?;
            if let Some(replacement) = pairs
                .iter()
                .find(|pair| pair.enabled && pair.old == value)
                .map(|pair| pair.new)
            {
                ensure_pixel_matches_format(replacement, raster.format())?;
                raster.set_pixel(x, y, replacement, revision.get())?;
                touched.insert(TileCoord {
                    x: x / TILE_SIZE,
                    y: y / TILE_SIZE,
                });
            }
        }
    }
    for coord in touched {
        raster.remove_tile_if_empty(coord);
    }
    core.commit_deferred_document_edit(before, after, base_revision, revision)
}

pub(crate) fn apply_separation(
    core: &mut Core,
    plane_id: u64,
    options: &BatchSeparation,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<DispatchOutcome, CoreError> {
    if !core.canonical_invocation_is_active() {
        let options = options.clone();
        let staged_options = options.clone();
        return core
            .execute_canonical_invocation_with(
                CanonicalInvocation::SeparateRasterColors { plane_id, options },
                move |staged| {
                    apply_separation(staged, plane_id, &staged_options, progress)
                        .map(InvocationResult::dispatch)
                },
            )
            .map(|result| result.dispatch);
    }
    let plane_id = PlaneId::from_raw(plane_id);
    let base_revision = core.document_revision;
    let before = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
    let source = before
        .plane_by_id(plane_id)
        .ok_or(CoreError::InvalidArgument(
            "batch plane target does not exist",
        ))?;
    ensure_pixel_matches_format(options.replacement, source.raster.format())?;
    let empty = empty_pixel(source.raster.format());
    let revision = core.next_document_revision()?;
    let mut after = before.clone();
    let raster = &mut after
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("batch plane target disappeared"))?
        .raster;
    for y in 0..raster.height() {
        if !progress(u64::from(y), u64::from(raster.height()).max(1)) {
            return Err(CoreError::Cancelled);
        }
        for x in 0..raster.width() {
            let value = raster.pixel(x, y)?;
            let selected = options.colors.contains(&value) ^ options.invert;
            raster.set_pixel(
                x,
                y,
                if selected { options.replacement } else { empty },
                revision.get(),
            )?;
        }
    }
    core.commit_deferred_document_edit(before, after, base_revision, revision)
}

pub(super) fn resolve_target(
    core: &Core,
    selector: &BatchTargetSelector,
) -> Result<Option<(u64, Option<u64>)>, CoreError> {
    let layers = core.layers()?;
    let layer = layers.iter().find(|layer| {
        selector.layer_id.is_none_or(|id| layer.id == id)
            && selector.layer_kind.is_none_or(|kind| layer.kind == kind)
    });
    let Some(layer) = layer else {
        return missing_target(selector.missing_policy);
    };
    let plane = if selector.plane_id.is_none() && selector.plane_kind.is_none() {
        None
    } else {
        layer.planes.iter().find(|plane| {
            selector.plane_id.is_none_or(|id| plane.id == id)
                && selector.plane_kind.is_none_or(|kind| plane.kind == kind)
        })
    };
    if (selector.plane_id.is_some() || selector.plane_kind.is_some()) && plane.is_none() {
        return missing_target(selector.missing_policy);
    }
    Ok(Some((layer.id, plane.map(|plane| plane.id))))
}

pub(super) fn missing_target(
    policy: BatchMissingTargetPolicy,
) -> Result<Option<(u64, Option<u64>)>, CoreError> {
    match policy {
        BatchMissingTargetPolicy::Skip => Ok(None),
        BatchMissingTargetPolicy::Error => Err(CoreError::InvalidArgument(
            "batch stable target does not exist in this cell",
        )),
    }
}

pub(super) fn working_core(source: &BatchSource) -> Result<Core, CoreError> {
    match &source.content {
        BatchSourceContent::Path(path) => {
            let mut core = Core::new();
            core.open(path)?;
            Ok(core)
        }
        BatchSourceContent::Document { document, assets } => {
            core_from_document(document.as_ref().clone(), assets.clone())
        }
        BatchSourceContent::Sequence(cell) => {
            let mut core = Core::new();
            core.new_cell_with_uuid(
                cell.raster.width(),
                cell.raster.height(),
                cell.dpi_x_milli,
                cell.dpi_y_milli,
                cell.document_uuid,
            )?;
            let revision = core.next_document_revision()?;
            let document = core.document.as_mut().ok_or(CoreError::NoDocument)?;
            document.frames = cell.frames;
            document
                .raster_mut(ActivePlane::Color)
                .clone_from(&cell.raster);
            core.document_revision = revision;
            core.reset_history(true);
            Ok(core)
        }
    }
}

pub(super) fn core_from_document(
    document: CellDocument,
    assets: asset::AssetStore,
) -> Result<Core, CoreError> {
    let mut core = Core::new();
    core.next_id = StableIdCursor::from_next_raw(document.max_stable_id().saturating_add(1));
    core.document_revision = DocumentRevision::from_raw(1);
    core.assets = assets;
    core.document = Some(document);
    core.reset_history(true);
    core.reset_editor_state(true);
    core.collect_unreferenced_assets()?;
    Ok(core)
}

pub(super) fn output_path_for(
    graph: &BatchGraph,
    source: &BatchSource,
    index: usize,
) -> Result<PathBuf, CoreError> {
    if graph.output.policy == BatchOutputPolicy::ExplicitOverwrite {
        return source.input_path.clone().ok_or(CoreError::InvalidArgument(
            "explicit overwrite requires a file-backed input",
        ));
    }
    let source_stem = Path::new(&source.label)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("cell");
    let number = if graph.output.descending {
        graph.output.start_number.saturating_sub(index as u32)
    } else {
        graph.output.start_number.saturating_add(index as u32)
    };
    let base_folder = if graph.output.folder.is_empty() {
        source
            .input_path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    } else {
        PathBuf::from(&graph.output.folder)
    };
    if base_folder.as_os_str().is_empty() {
        return Err(CoreError::InvalidArgument(
            "batch output folder is required for an in-memory input",
        ));
    }
    let folder = if graph.output.cell_folder {
        base_folder.join(source_stem)
    } else {
        base_folder
    };
    let file_name = match graph.output.policy {
        BatchOutputPolicy::Duplicate if graph.output.basename.is_empty() => {
            format!("{source_stem}_batch.inkpod")
        }
        BatchOutputPolicy::Duplicate | BatchOutputPolicy::NewSave => {
            let basename = if graph.output.basename.is_empty() {
                "cell"
            } else {
                &graph.output.basename
            };
            format!("{basename}_{number:04}.inkpod")
        }
        BatchOutputPolicy::ExplicitOverwrite => unreachable!(),
    };
    Ok(folder.join(file_name))
}

pub(super) fn save_batch_output(
    working: &Core,
    graph: &BatchGraph,
    source: &BatchSource,
    path: &Path,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(), CoreError> {
    working.document.as_ref().ok_or(CoreError::NoDocument)?;
    if graph.output.policy != BatchOutputPolicy::ExplicitOverwrite {
        if source.input_path.as_deref() == Some(path) {
            return Err(CoreError::InvalidState(
                "non-overwrite batch policy resolved to the input path",
            ));
        }
        if path.exists() {
            return Err(CoreError::InvalidState(
                "non-overwrite batch output already exists",
            ));
        }
    }
    if is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|error| CoreError::Format(error.to_string()))?;
    }
    let editor_savepoint = working
        .editor_session
        .as_ref()
        .ok_or(CoreError::NoDocument)?
        .digest;
    let file = working.build_procedure_file(Some(working.current_state), Some(editor_savepoint))?;
    inkpod_format::save_procedure_file_atomic_with_cancel(path, &file, &mut is_cancelled)?;
    Ok(())
}

pub(super) fn cancelled_item(
    source: &BatchSource,
    output_path: Option<PathBuf>,
) -> BatchItemResult {
    BatchItemResult {
        input_name: source.label.clone(),
        output_path,
        outcome: BatchItemOutcome::Cancelled,
        message: "cancelled before atomic commit".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_mismatched_target_selector_is_skipped() {
        let mut core = Core::new();
        core.new_cell(2, 2, 96_000, 96_000).unwrap();
        let layers = core.layers().unwrap();
        let coloring = layers
            .iter()
            .find(|layer| layer.kind == LayerKind::BinaryColoring)
            .unwrap();
        let color_plane = coloring
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Color)
            .unwrap();
        let selector = BatchTargetSelector {
            layer_id: Some(coloring.id),
            plane_id: Some(color_plane.id),
            layer_kind: Some(LayerKind::VectorColoring),
            plane_kind: Some(PlaneType::Color),
            missing_policy: BatchMissingTargetPolicy::Skip,
        };
        assert_eq!(resolve_target(&core, &selector).unwrap(), None);
    }
}
