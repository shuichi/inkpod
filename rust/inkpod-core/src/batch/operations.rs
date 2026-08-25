use super::model::{BatchSource, BatchSourceContent};
use super::validation::{
    empty_pixel, ensure_pixel_matches_format, validate_component, validate_naming_template,
    validate_operation,
};
use super::*;
use crate::primitive::{CanonicalInvocation, InvocationResult};

impl Core {
    /// Applies an ordered Batch v4 operation list as one canonical procedure and Undo unit.
    ///
    /// Disabled operations are ignored. Invalid targets, cancellation, stale revision,
    /// overflow, and allocation failure publish no partial document state.
    pub fn apply_batch_operations(
        &mut self,
        operations: &[BatchOperation],
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<DispatchOutcome, CoreError> {
        let enabled = operations
            .iter()
            .filter(|operation| operation.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if enabled.is_empty() {
            return Err(CoreError::InvalidArgument(
                "batch operation list has no enabled operation",
            ));
        }
        for operation in &enabled {
            validate_operation(operation)?;
        }
        if !self.canonical_invocation_is_active() {
            let canonical = lower_batch_operations(
                self.document.as_ref().ok_or(CoreError::NoDocument)?,
                &enabled,
            )?;
            if canonical.is_empty() {
                return Ok(self.noop_outcome());
            }
            let staged = canonical.clone();
            return self
                .execute_canonical_invocation_with(
                    CanonicalInvocation::ApplyBatchOperations {
                        operations: canonical,
                    },
                    move |core| {
                        apply_batch_operations_canonical(core, &staged, &mut is_cancelled)
                            .map(InvocationResult::dispatch)
                    },
                )
                .map(|result| result.dispatch);
        }
        apply_batch_operations_canonical(self, &enabled, &mut is_cancelled)
    }
}

pub(crate) fn apply_batch_operations_canonical(
    core: &mut Core,
    operations: &[BatchOperation],
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<DispatchOutcome, CoreError> {
    if !core.canonical_invocation_is_active() {
        return Err(CoreError::InvalidState(
            "batch operations require a canonical primitive",
        ));
    }
    let base_revision = core.document_revision;
    let before = core.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
    let revision = core.next_document_revision()?;
    let mut after = before.clone();
    let mut total_work = 0_u64;

    for operation in operations {
        validate_operation(operation)?;
        if !operation.enabled {
            continue;
        }
        if is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let Some((_, plane_id)) = resolve_target_in_document(&after, &operation.target)? else {
            continue;
        };
        let plane_id = PlaneId::from_raw(plane_id);
        let source = after
            .plane_by_id(plane_id)
            .ok_or(CoreError::InvalidState("batch target plane disappeared"))?;
        let work = u64::from(source.raster.width())
            .checked_mul(u64::from(source.raster.height()))
            .ok_or(CoreError::InvalidArgument("batch raster work overflows"))?;
        total_work = total_work
            .checked_add(work)
            .ok_or(CoreError::InvalidArgument("batch operation work overflows"))?;
        if total_work > MAX_IMAGE_EDIT_PIXELS {
            return Err(CoreError::InvalidArgument(
                "batch operation list exceeds the bounded work limit",
            ));
        }

        match &operation.kind {
            BatchOperationKind::ColorReplace(pairs) => apply_color_replace_to_document(
                &mut after,
                plane_id,
                pairs,
                revision,
                is_cancelled,
            )?,
            BatchOperationKind::MoveToColorPlane(colors) => {
                move_colors_to_color_plane(&mut after, plane_id, colors, revision, is_cancelled)?
            }
            BatchOperationKind::Masking(colors) => {
                replace_fill_protection_mask(&mut after, plane_id, colors, revision, is_cancelled)?
            }
            BatchOperationKind::Erase(colors) => {
                erase_colors_from_document(&mut after, plane_id, colors, revision, is_cancelled)?
            }
        }
    }

    core.commit_deferred_document_edit(before, after, base_revision, revision)
}

fn lower_batch_operations(
    document: &CellDocument,
    operations: &[BatchOperation],
) -> Result<Vec<BatchOperation>, CoreError> {
    let mut lowered = Vec::new();
    for operation in operations {
        let all_matches = matches!(operation.kind, BatchOperationKind::ColorReplace(_));
        let mut resolved = Vec::new();
        for selector in
            std::iter::once(&operation.target).chain(operation.additional_targets.iter())
        {
            for target in resolve_targets_in_document(document, selector, all_matches)? {
                if !resolved.iter().any(|existing: &BatchTargetSelector| {
                    existing.layer_id == target.layer_id && existing.plane_id == target.plane_id
                }) {
                    resolved.push(target);
                }
            }
        }
        for target in resolved {
            lowered.push(BatchOperation {
                version: operation.version,
                enabled: operation.enabled,
                target,
                additional_targets: Vec::new(),
                kind: operation.kind.clone(),
            });
        }
    }
    Ok(lowered)
}

fn resolve_targets_in_document(
    document: &CellDocument,
    selector: &BatchTargetSelector,
    all_matches: bool,
) -> Result<Vec<BatchTargetSelector>, CoreError> {
    let mut matches = Vec::new();
    for layer in document.layers.iter().filter(|layer| {
        selector.layer_id.is_none_or(|id| layer.id.get() == id)
            && selector.layer_kind.is_none_or(|kind| layer.kind == kind)
    }) {
        for plane in layer.planes.iter().filter(|plane| {
            selector.plane_id.is_none_or(|id| plane.id.get() == id)
                && selector.plane_kind.is_none_or(|kind| plane.kind == kind)
        }) {
            matches.push(BatchTargetSelector {
                layer_id: Some(layer.id.get()),
                plane_id: Some(plane.id.get()),
                layer_kind: Some(layer.kind),
                plane_kind: Some(plane.kind),
                missing_policy: BatchMissingTargetPolicy::Error,
            });
            if !all_matches {
                return Ok(matches);
            }
        }
    }
    if matches.is_empty() && selector.missing_policy == BatchMissingTargetPolicy::Error {
        return Err(CoreError::InvalidArgument(
            "batch stable target does not exist in this cell",
        ));
    }
    Ok(matches)
}

fn resolve_target_in_document(
    document: &CellDocument,
    selector: &BatchTargetSelector,
) -> Result<Option<(LayerId, u64)>, CoreError> {
    let layer = document.layers.iter().find(|layer| {
        selector.layer_id.is_none_or(|id| layer.id.get() == id)
            && selector.layer_kind.is_none_or(|kind| layer.kind == kind)
    });
    let Some(layer) = layer else {
        return match selector.missing_policy {
            BatchMissingTargetPolicy::Skip => Ok(None),
            BatchMissingTargetPolicy::Error => Err(CoreError::InvalidArgument(
                "batch stable target does not exist in this cell",
            )),
        };
    };
    let plane = layer.planes.iter().find(|plane| {
        selector.plane_id.is_none_or(|id| plane.id.get() == id)
            && selector.plane_kind.is_none_or(|kind| plane.kind == kind)
    });
    let Some(plane) = plane else {
        return match selector.missing_policy {
            BatchMissingTargetPolicy::Skip => Ok(None),
            BatchMissingTargetPolicy::Error => Err(CoreError::InvalidArgument(
                "batch stable target does not exist in this cell",
            )),
        };
    };
    Ok(Some((layer.id, plane.id.get())))
}

fn validate_editable_source(document: &CellDocument, plane_id: PlaneId) -> Result<(), CoreError> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.planes.iter().any(|plane| plane.id == plane_id))
        .ok_or(CoreError::InvalidArgument(
            "batch plane target does not exist",
        ))?;
    let plane = layer
        .planes
        .iter()
        .find(|plane| plane.id == plane_id)
        .ok_or(CoreError::InvalidState("batch target plane disappeared"))?;
    if !layer.visible || !layer.editable || !plane.visible || !plane.editable {
        return Err(CoreError::InvalidArgument(
            "batch target is hidden or non-editable",
        ));
    }
    Ok(())
}

fn apply_color_replace_to_document(
    document: &mut CellDocument,
    plane_id: PlaneId,
    pairs: &[BatchColorPair],
    revision: DocumentRevision,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), CoreError> {
    validate_editable_source(document, plane_id)?;
    let raster = &mut document
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("batch target plane disappeared"))?
        .raster;
    for pair in pairs.iter().filter(|pair| pair.enabled) {
        ensure_pixel_matches_format(pair.old, raster.format())?;
        ensure_pixel_matches_format(pair.new, raster.format())?;
    }
    let mut touched = BTreeSet::new();
    for y in 0..raster.height() {
        if is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        for x in 0..raster.width() {
            let value = raster.pixel(x, y)?;
            if let Some(replacement) = pairs
                .iter()
                .find(|pair| pair.enabled && pair.old == value)
                .map(|pair| pair.new)
                && replacement != value
            {
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
    Ok(())
}

fn move_colors_to_color_plane(
    document: &mut CellDocument,
    source_id: PlaneId,
    colors: &[PixelValue],
    revision: DocumentRevision,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), CoreError> {
    validate_editable_source(document, source_id)?;
    let layer_index = document
        .layers
        .iter()
        .position(|layer| layer.planes.iter().any(|plane| plane.id == source_id))
        .ok_or(CoreError::InvalidArgument(
            "batch move source plane does not exist",
        ))?;
    let source_index = document.layers[layer_index]
        .planes
        .iter()
        .position(|plane| plane.id == source_id)
        .ok_or(CoreError::InvalidState("batch move source disappeared"))?;
    if document.layers[layer_index].planes[source_index].kind == PlaneType::MainLine {
        return Err(CoreError::InvalidArgument(
            "batch move cannot modify the protected main-line plane",
        ));
    }
    let destination_index = document.layers[layer_index]
        .planes
        .iter()
        .position(|plane| plane.kind == PlaneType::Color)
        .ok_or(CoreError::InvalidArgument(
            "batch move color-plane destination is missing",
        ))?;
    if source_index == destination_index {
        return Err(CoreError::InvalidArgument(
            "batch move source and destination must be different planes",
        ));
    }
    let source = &document.layers[layer_index].planes[source_index];
    let destination = &document.layers[layer_index].planes[destination_index];
    if !destination.visible || !destination.editable {
        return Err(CoreError::InvalidArgument(
            "batch move destination is hidden or non-editable",
        ));
    }
    if source.raster.format() != destination.raster.format()
        || source.raster.width() != destination.raster.width()
        || source.raster.height() != destination.raster.height()
    {
        return Err(CoreError::InvalidArgument(
            "batch move source and destination raster contracts do not match",
        ));
    }
    for color in colors {
        ensure_pixel_matches_format(*color, source.raster.format())?;
    }
    let source_raster = source.raster.clone();
    let mut matches = Vec::new();
    for y in 0..source_raster.height() {
        if is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        for x in 0..source_raster.width() {
            let value = source_raster.pixel(x, y)?;
            if colors.contains(&value) {
                matches
                    .try_reserve(1)
                    .map_err(|_| CoreError::InvalidState("batch move allocation failed"))?;
                matches.push((x, y, value));
            }
        }
    }
    let empty = empty_pixel(source_raster.format());
    let layer = &mut document.layers[layer_index];
    let (source, destination) = if source_index < destination_index {
        let (left, right) = layer.planes.split_at_mut(destination_index);
        (&mut left[source_index], &mut right[0])
    } else {
        let (left, right) = layer.planes.split_at_mut(source_index);
        (&mut right[0], &mut left[destination_index])
    };
    let mut source_tiles = BTreeSet::new();
    let mut destination_tiles = BTreeSet::new();
    for (x, y, value) in matches {
        destination.raster.set_pixel(x, y, value, revision.get())?;
        source.raster.set_pixel(x, y, empty, revision.get())?;
        let coord = TileCoord {
            x: x / TILE_SIZE,
            y: y / TILE_SIZE,
        };
        source_tiles.insert(coord);
        destination_tiles.insert(coord);
    }
    for coord in source_tiles {
        source.raster.remove_tile_if_empty(coord);
    }
    for coord in destination_tiles {
        destination.raster.remove_tile_if_empty(coord);
    }
    Ok(())
}

fn replace_fill_protection_mask(
    document: &mut CellDocument,
    source_id: PlaneId,
    colors: &[PixelValue],
    revision: DocumentRevision,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), CoreError> {
    validate_editable_source(document, source_id)?;
    let source = document
        .plane_by_id(source_id)
        .ok_or(CoreError::InvalidState("batch mask source disappeared"))?;
    for color in colors {
        ensure_pixel_matches_format(*color, source.raster.format())?;
    }
    let source_raster = source.raster.clone();
    let mut mask = inkpod_image::TileRaster::new(
        source_raster.width(),
        source_raster.height(),
        PixelFormat::BinaryMask8,
    )?;
    for y in 0..source_raster.height() {
        if is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        for x in 0..source_raster.width() {
            if colors.contains(&source_raster.pixel(x, y)?) {
                mask.set_pixel(x, y, PixelValue::Binary(u8::MAX), revision.get())?;
            }
        }
    }
    document.fill_protection = mask;
    Ok(())
}

fn erase_colors_from_document(
    document: &mut CellDocument,
    plane_id: PlaneId,
    colors: &[PixelValue],
    revision: DocumentRevision,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), CoreError> {
    validate_editable_source(document, plane_id)?;
    let raster = &mut document
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("batch erase target disappeared"))?
        .raster;
    for color in colors {
        ensure_pixel_matches_format(*color, raster.format())?;
    }
    let empty = empty_pixel(raster.format());
    let mut touched = BTreeSet::new();
    for y in 0..raster.height() {
        if is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        for x in 0..raster.width() {
            if colors.contains(&raster.pixel(x, y)?) {
                raster.set_pixel(x, y, empty, revision.get())?;
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
    Ok(())
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
    let source_layer = before
        .layers
        .iter()
        .find(|layer| layer.planes.iter().any(|plane| plane.id == plane_id))
        .ok_or(CoreError::InvalidArgument(
            "batch plane target does not exist",
        ))?;
    let source = source_layer
        .planes
        .iter()
        .find(|plane| plane.id == plane_id)
        .ok_or(CoreError::InvalidState(
            "batch separation source disappeared",
        ))?;
    if !source_layer.visible || !source_layer.editable || !source.visible || !source.editable {
        return Err(CoreError::InvalidArgument(
            "batch separation source is hidden or non-editable",
        ));
    }
    for color in &options.colors {
        ensure_pixel_matches_format(*color, source.raster.format())?;
    }
    let destination_plane_id = match options.destination {
        BatchSeparationDestination::ReplaceSource | BatchSeparationDestination::NativeFile => {
            Some(plane_id)
        }
        BatchSeparationDestination::SelectionMask => None,
        BatchSeparationDestination::MainLinePlane => Some(
            source_layer
                .planes
                .iter()
                .find(|plane| plane.kind == PlaneType::MainLine)
                .ok_or(CoreError::InvalidArgument(
                    "batch separation main-line destination is missing",
                ))?
                .id,
        ),
        BatchSeparationDestination::ColorPlane => Some(
            source_layer
                .planes
                .iter()
                .find(|plane| plane.kind == PlaneType::Color)
                .ok_or(CoreError::InvalidArgument(
                    "batch separation color destination is missing",
                ))?
                .id,
        ),
    };
    let destination_format = if let Some(destination_plane_id) = destination_plane_id {
        let destination = source_layer
            .planes
            .iter()
            .find(|plane| plane.id == destination_plane_id)
            .ok_or(CoreError::InvalidState(
                "batch separation destination disappeared",
            ))?;
        if !destination.visible || !destination.editable {
            return Err(CoreError::InvalidArgument(
                "batch separation destination is hidden or non-editable",
            ));
        }
        if destination.raster.width() != source.raster.width()
            || destination.raster.height() != source.raster.height()
        {
            return Err(CoreError::InvalidArgument(
                "batch separation destination dimensions do not match",
            ));
        }
        ensure_pixel_matches_format(options.replacement, destination.raster.format())?;
        destination.raster.format()
    } else {
        PixelFormat::BinaryMask8
    };
    let source_raster = source.raster.clone();
    let empty = empty_pixel(destination_format);
    let revision = core.next_document_revision()?;
    let mut after = before.clone();
    let raster = if let Some(destination_plane_id) = destination_plane_id {
        &mut after
            .plane_by_id_mut(destination_plane_id)
            .ok_or(CoreError::InvalidState(
                "batch separation destination disappeared",
            ))?
            .raster
    } else {
        &mut after.selection
    };
    for y in 0..source_raster.height() {
        if !progress(u64::from(y), u64::from(source_raster.height()).max(1)) {
            return Err(CoreError::Cancelled);
        }
        for x in 0..source_raster.width() {
            let value = source_raster.pixel(x, y)?;
            let selected = options.colors.contains(&value) ^ options.invert;
            let selected_value = if destination_plane_id.is_none() {
                PixelValue::Binary(u8::MAX)
            } else {
                options.replacement
            };
            raster.set_pixel(
                x,
                y,
                if selected { selected_value } else { empty },
                revision.get(),
            )?;
        }
    }
    let allocated: Vec<_> = raster.allocated_coords().collect();
    for coord in allocated {
        raster.remove_tile_if_empty(coord);
    }
    core.commit_deferred_document_edit(before, after, base_revision, revision)
}

pub(super) fn working_core(source: &BatchSource) -> Result<Core, CoreError> {
    match &source.content {
        BatchSourceContent::Path(path) => {
            let mut core = Core::new();
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("inkpod"))
            {
                core.open(path)?;
            } else {
                let extension = path.extension().and_then(|value| value.to_str()).ok_or(
                    CoreError::InvalidArgument("batch input extension is unsupported"),
                )?;
                let format = CommonRasterFormat::from_extension(extension).ok_or(
                    CoreError::InvalidArgument("batch input extension is unsupported"),
                )?;
                let bytes = fs::read(path).map_err(|error| CoreError::Format(error.to_string()))?;
                let digest = blake3::hash(&bytes);
                let mut uuid_bytes = [0_u8; 16];
                uuid_bytes.copy_from_slice(&digest.as_bytes()[..16]);
                let mut uuid = u128::from_le_bytes(uuid_bytes);
                if uuid == 0 {
                    uuid = 1;
                }
                core.import_common_raster(format, &bytes, uuid)?;
            }
            Ok(core)
        }
        BatchSourceContent::Document { document, assets } => {
            core_from_document(document.as_ref().clone(), assets.clone())
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
    if graph.output.destination != BatchOutputDestination::Folder {
        return Err(CoreError::InvalidArgument(
            "non-folder batch output has no file path",
        ));
    }
    let source_stem = Path::new(&source.label)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("cell");
    let base_folder = PathBuf::from(&graph.output.folder);
    if base_folder.as_os_str().is_empty() {
        return Err(CoreError::InvalidArgument(
            "batch output folder is required for an in-memory input",
        ));
    }
    let basename = render_naming_template(&graph.output.naming_template, source_stem, index)?;
    let extension = match graph.output.format {
        BatchOutputFormat::Inkpod => "inkpod",
        BatchOutputFormat::Png => "png",
        BatchOutputFormat::Tiff => "tiff",
        BatchOutputFormat::Tga => "tga",
        BatchOutputFormat::Bmp => "bmp",
    };
    Ok(base_folder.join(format!("{basename}.{extension}")))
}

fn render_naming_template(template: &str, stem: &str, index: usize) -> Result<String, CoreError> {
    validate_naming_template(template)?;
    let mut rendered = String::new();
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        rendered.push_str(&remaining[..open]);
        remaining = &remaining[open..];
        if let Some(rest) = remaining.strip_prefix("{stem}") {
            rendered.push_str(stem);
            remaining = rest;
            continue;
        }
        let Some(rest) = remaining.strip_prefix("{index:") else {
            return Err(CoreError::InvalidArgument(
                "batch naming template contains an unknown token",
            ));
        };
        let Some(close) = rest.find('}') else {
            return Err(CoreError::InvalidArgument(
                "batch naming template token is unterminated",
            ));
        };
        let width = rest[..close]
            .parse::<usize>()
            .map_err(|_| CoreError::InvalidArgument("batch index width is invalid"))?;
        if !(1..=12).contains(&width) {
            return Err(CoreError::InvalidArgument(
                "batch index width is outside bounds",
            ));
        }
        rendered.push_str(&format!("{index:0width$}", index = index + 1));
        remaining = &rest[close + 1..];
    }
    if remaining.contains('}') {
        return Err(CoreError::InvalidArgument(
            "batch naming template contains an unmatched brace",
        ));
    }
    rendered.push_str(remaining);
    validate_component(&rendered, true)?;
    Ok(rendered)
}

pub(super) fn save_batch_output(
    working: &Core,
    graph: &BatchGraph,
    source: &BatchSource,
    path: &Path,
    is_cancelled: impl FnMut() -> bool,
) -> Result<(), CoreError> {
    save_batch_output_with_format(working, graph.output.format, source, path, is_cancelled)
}

pub(super) fn save_batch_output_with_format(
    working: &Core,
    format: BatchOutputFormat,
    source: &BatchSource,
    path: &Path,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(), CoreError> {
    working.document.as_ref().ok_or(CoreError::NoDocument)?;
    if source.input_path.as_deref() == Some(path) {
        return Err(CoreError::InvalidState(
            "batch output resolves to the input path",
        ));
    }
    if path.exists() {
        return Err(CoreError::InvalidState("batch output already exists"));
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
    match format {
        BatchOutputFormat::Inkpod => {
            let editor_savepoint = working
                .editor_session
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .digest;
            let file = working
                .build_procedure_file(Some(working.current_state), Some(editor_savepoint))?;
            inkpod_format::save_procedure_file_atomic_with_cancel(path, &file, &mut is_cancelled)?;
        }
        format => {
            let common = match format {
                BatchOutputFormat::Png => CommonRasterFormat::Png,
                BatchOutputFormat::Tiff => CommonRasterFormat::Tiff,
                BatchOutputFormat::Tga => CommonRasterFormat::Tga,
                BatchOutputFormat::Bmp => CommonRasterFormat::Bmp,
                BatchOutputFormat::Inkpod => unreachable!(),
            };
            let bytes = working.export_common_raster(common, false)?;
            inkpod_format::save_common_raster_bytes_atomic_with_cancel(
                path,
                &bytes,
                &mut is_cancelled,
            )?;
        }
    }
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
