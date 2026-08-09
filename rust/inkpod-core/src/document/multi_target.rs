//! Atomic grouped layer/plane commands driven by the persisted editor target set.

use super::*;
use crate::primitive::CanonicalInvocation;

impl Core {
    /// Returns capabilities for the effective grouped target set without mutation.
    pub fn edit_target_capabilities(&self) -> Result<EditTargetCapabilities, CoreError> {
        let targets = self.effective_edit_targets()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let plane_only = targets
            .iter()
            .all(|target| matches!(target, EditTarget::Plane(_)));
        let layer_only = targets
            .iter()
            .all(|target| matches!(target, EditTarget::Layer(_)));
        let duplicate = targets.iter().all(|target| match target {
            EditTarget::Layer(_) => document.layers.len() < MAX_LAYERS,
            EditTarget::Plane(target) => document
                .layers
                .iter()
                .find(|layer| layer.id.get() == target.layer_id)
                .and_then(|layer| {
                    layer
                        .planes
                        .iter()
                        .find(|plane| plane.id.get() == target.plane_id)
                        .map(|plane| {
                            layer.planes.len() < MAX_PLANES_PER_LAYER
                                && !is_required_singleton_plane(plane.kind)
                        })
                })
                .unwrap_or(false),
        });
        let delete = grouped_delete_is_valid(document, &targets);
        Ok(EditTargetCapabilities {
            duplicate,
            delete,
            visibility: !targets.is_empty(),
            editability: !targets.is_empty(),
            merge: grouped_merge_pair(document, &targets).is_some(),
            convert_planes: plane_only,
            convert_layers: layer_only
                && targets.iter().all(|target| match target {
                    EditTarget::Layer(id) => document.layers.iter().any(|layer| {
                        layer.id.get() == *id
                            && matches!(
                                layer.kind,
                                LayerKind::BinaryColoring | LayerKind::GrayscaleColoring
                            )
                    }),
                    EditTarget::Plane(_) => false,
                }),
        })
    }

    /// Applies one grouped command through one canonical procedure and transaction.
    ///
    /// The target set is captured at issue time. Success publishes at most one
    /// document revision/history/journal entry; no-op and every error preserve all
    /// document state and stable-ID allocation.
    pub fn apply_edit_target_command(
        &mut self,
        command: EditTargetCommand,
    ) -> Result<EditTargetCommandResult, CoreError> {
        if !self.canonical_invocation_is_active() {
            let targets = self.effective_edit_targets()?;
            let result = self.execute_canonical_invocation(CanonicalInvocation::EditTargets {
                targets: targets.clone(),
                command,
            })?;
            let output_targets = match command {
                EditTargetCommand::Duplicate | EditTargetCommand::Merge => targets
                    .iter()
                    .zip(result.output_ids.iter())
                    .map(|(target, id)| match target {
                        EditTarget::Layer(_) => EditTarget::Layer(*id),
                        EditTarget::Plane(source) => EditTarget::Plane(EditorTarget {
                            layer_id: source.layer_id,
                            plane_id: *id,
                        }),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            return Ok(EditTargetCommandResult {
                dispatch: result.dispatch,
                output_targets,
            });
        }
        self.apply_edit_target_command_to(self.effective_edit_targets()?, command)
    }

    pub(crate) fn apply_edit_target_command_to(
        &mut self,
        targets: Vec<EditTarget>,
        command: EditTargetCommand,
    ) -> Result<EditTargetCommandResult, CoreError> {
        self.ensure_no_active_stroke()?;
        if targets.is_empty() {
            return Err(CoreError::InvalidState("there are no edit targets"));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let normalized = Self::normalize_edit_targets_in(document, &targets, true)?;
        if normalized != targets {
            return Err(CoreError::InvalidArgument(
                "edit targets are not in canonical document-tree order",
            ));
        }
        match command {
            EditTargetCommand::Duplicate => self.duplicate_edit_targets(targets),
            EditTargetCommand::Delete => self.delete_edit_targets(targets),
            EditTargetCommand::SetVisibility(visible) => {
                self.set_edit_target_flag(targets, visible, true)
            }
            EditTargetCommand::SetEditability(editable) => {
                self.set_edit_target_flag(targets, editable, false)
            }
            EditTargetCommand::ConvertPlanes { kind, format } => {
                self.convert_edit_target_planes(targets, kind, format)
            }
            EditTargetCommand::ConvertLayers { kind } => {
                self.convert_edit_target_layers(targets, kind)
            }
            EditTargetCommand::Merge => self.merge_edit_targets(targets),
        }
    }

    fn duplicate_edit_targets(
        &mut self,
        targets: Vec<EditTarget>,
    ) -> Result<EditTargetCommandResult, CoreError> {
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        let selected = targets.iter().copied().collect::<BTreeSet<_>>();
        let layer_count = targets
            .iter()
            .filter(|target| matches!(target, EditTarget::Layer(_)))
            .count();
        if before.layers.len().saturating_add(layer_count) > MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        for layer in &before.layers {
            let additions = targets
                .iter()
                .filter(|target| {
                    matches!(target, EditTarget::Plane(target) if target.layer_id == layer.id.get())
                })
                .count();
            if layer.planes.len().saturating_add(additions) > MAX_PLANES_PER_LAYER {
                return Err(CoreError::InvalidState("plane limit reached"));
            }
        }

        let mut next_id = self.next_id;
        let mut plane_map = BTreeMap::new();
        let mut output_targets = Vec::with_capacity(targets.len());
        let mut next_layers = Vec::with_capacity(before.layers.len() + layer_count);
        for source_layer in &before.layers {
            next_layers.push(source_layer.clone());
            if selected.contains(&EditTarget::Layer(source_layer.id.get())) {
                let mut duplicate = source_layer.clone();
                let source_layer_id = duplicate.id;
                duplicate.id = next_id.take_layer();
                duplicate.name =
                    unique_layer_name(&next_layers, &format!("{} Copy", duplicate.name));
                for plane in &mut duplicate.planes {
                    let source = plane.id;
                    plane.id = next_id.take_plane();
                    plane_map.insert(source, plane.id);
                    plane.name = format!("{} Copy", plane.name);
                }
                if let Some(adjustment) = before.adjustments.get(&source_layer_id).cloned() {
                    after.adjustments.insert(duplicate.id, adjustment);
                }
                output_targets.push(EditTarget::Layer(duplicate.id.get()));
                next_layers.push(duplicate);
                continue;
            }
            let destination = next_layers
                .last_mut()
                .ok_or(CoreError::InvalidState("duplicate layer staging failed"))?;
            let mut next_planes = Vec::with_capacity(destination.planes.len());
            for source_plane in &source_layer.planes {
                next_planes.push(source_plane.clone());
                let source_target = EditTarget::Plane(EditorTarget {
                    layer_id: source_layer.id.get(),
                    plane_id: source_plane.id.get(),
                });
                if selected.contains(&source_target) {
                    if is_required_singleton_plane(source_plane.kind) {
                        return Err(CoreError::InvalidState(
                            "required singleton planes cannot be duplicated individually",
                        ));
                    }
                    let mut duplicate = source_plane.clone();
                    duplicate.id = next_id.take_plane();
                    duplicate.name =
                        unique_plane_name(&next_planes, &format!("{} Copy", duplicate.name));
                    plane_map.insert(source_plane.id, duplicate.id);
                    output_targets.push(EditTarget::Plane(EditorTarget {
                        layer_id: source_layer.id.get(),
                        plane_id: duplicate.id.get(),
                    }));
                    next_planes.push(duplicate);
                }
            }
            destination.planes = next_planes;
        }
        after.layers = next_layers;
        after.vector.duplicate_planes(&plane_map, &mut next_id);
        after.vector.ensure_limits()?;
        let preferred_active = preferred_active_target(after, &output_targets);
        edit.prefer_edit_targets(output_targets.clone());
        if let Some(target) = preferred_active {
            edit.prefer_editor_target(target);
        }
        let dispatch = edit.commit(self)?;
        self.next_id = next_id;
        Ok(EditTargetCommandResult {
            dispatch,
            output_targets,
        })
    }

    fn delete_edit_targets(
        &mut self,
        targets: Vec<EditTarget>,
    ) -> Result<EditTargetCommandResult, CoreError> {
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        if !grouped_delete_is_valid(before, &targets) {
            return Err(CoreError::InvalidState(
                "grouped deletion would invalidate document topology",
            ));
        }
        let selected = targets.iter().copied().collect::<BTreeSet<_>>();
        for target in &targets {
            match target {
                EditTarget::Layer(layer_id) => {
                    let id = LayerId::from_raw(*layer_id);
                    after.vector.remove_layer(before, id);
                    after.adjustments.remove(&id);
                }
                EditTarget::Plane(target) => after
                    .vector
                    .remove_plane(PlaneId::from_raw(target.plane_id)),
            }
        }
        after
            .layers
            .retain(|layer| !selected.contains(&EditTarget::Layer(layer.id.get())));
        for layer in &mut after.layers {
            layer.planes.retain(|plane| {
                !selected.contains(&EditTarget::Plane(EditorTarget {
                    layer_id: layer.id.get(),
                    plane_id: plane.id.get(),
                }))
            });
            validate_layer_kind(layer.kind, &layer.planes)?;
        }
        edit.prefer_edit_targets(Vec::new());
        let dispatch = edit.commit(self)?;
        Ok(EditTargetCommandResult {
            dispatch,
            output_targets: Vec::new(),
        })
    }

    fn set_edit_target_flag(
        &mut self,
        targets: Vec<EditTarget>,
        value: bool,
        visibility: bool,
    ) -> Result<EditTargetCommandResult, CoreError> {
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        for target in &targets {
            match target {
                EditTarget::Layer(layer_id) => {
                    let layer = after
                        .layers
                        .iter_mut()
                        .find(|layer| layer.id.get() == *layer_id)
                        .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
                    if visibility {
                        layer.visible = value;
                    } else {
                        layer.editable = value;
                    }
                }
                EditTarget::Plane(target) => {
                    let plane = after
                        .plane_by_id_mut(PlaneId::from_raw(target.plane_id))
                        .ok_or(CoreError::InvalidArgument("plane ID does not exist"))?;
                    if visibility {
                        plane.visible = value;
                    } else {
                        plane.editable = value;
                    }
                }
            }
        }
        let dispatch = edit.commit(self)?;
        Ok(EditTargetCommandResult {
            dispatch,
            output_targets: Vec::new(),
        })
    }

    fn convert_edit_target_planes(
        &mut self,
        targets: Vec<EditTarget>,
        kind: PlaneType,
        format: PixelFormat,
    ) -> Result<EditTargetCommandResult, CoreError> {
        validate_plane_format(kind, format)?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision().get();
        let (before, after) = edit.documents();
        for target in &targets {
            let EditTarget::Plane(target) = target else {
                return Err(CoreError::InvalidArgument(
                    "plane conversion requires only plane targets",
                ));
            };
            let source = before
                .plane_by_id(PlaneId::from_raw(target.plane_id))
                .ok_or(CoreError::InvalidArgument("plane ID does not exist"))?;
            if is_vector_plane(source.kind) || is_vector_plane(kind) {
                return Err(CoreError::InvalidArgument(
                    "vector plane conversion requires explicit rasterize/vectorize",
                ));
            }
            let converted = convert_plane_raster(&source.raster, format, revision)?;
            let destination = after
                .plane_by_id_mut(source.id)
                .ok_or(CoreError::InvalidState("conversion destination is missing"))?;
            destination.kind = kind;
            destination.raster = converted;
        }
        for layer in &after.layers {
            validate_layer_kind(layer.kind, &layer.planes)?;
        }
        let dispatch = edit.commit(self)?;
        Ok(EditTargetCommandResult {
            dispatch,
            output_targets: Vec::new(),
        })
    }

    fn convert_edit_target_layers(
        &mut self,
        targets: Vec<EditTarget>,
        kind: LayerKind,
    ) -> Result<EditTargetCommandResult, CoreError> {
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision().get();
        let after = edit.working_mut();
        for target in &targets {
            let EditTarget::Layer(layer_id) = target else {
                return Err(CoreError::InvalidArgument(
                    "layer conversion requires only layer targets",
                ));
            };
            let layer = after
                .layers
                .iter_mut()
                .find(|layer| layer.id.get() == *layer_id)
                .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
            if layer.kind == kind {
                continue;
            }
            if !matches!(
                (layer.kind, kind),
                (LayerKind::BinaryColoring, LayerKind::GrayscaleColoring)
                    | (LayerKind::GrayscaleColoring, LayerKind::BinaryColoring)
            ) {
                return Err(CoreError::InvalidArgument(
                    "requested layer conversion would lose unsupported semantics",
                ));
            }
            let main = layer
                .planes
                .iter_mut()
                .find(|plane| plane.kind == PlaneType::MainLine)
                .ok_or(CoreError::InvalidState("coloring layer has no main plane"))?;
            main.raster = convert_main_line_raster(
                &main.raster,
                kind == LayerKind::GrayscaleColoring,
                revision,
            )?;
            layer.kind = kind;
            validate_layer_kind(layer.kind, &layer.planes)?;
        }
        let dispatch = edit.commit(self)?;
        Ok(EditTargetCommandResult {
            dispatch,
            output_targets: Vec::new(),
        })
    }

    fn merge_edit_targets(
        &mut self,
        targets: Vec<EditTarget>,
    ) -> Result<EditTargetCommandResult, CoreError> {
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision().get();
        let (before, after) = edit.documents();
        let pair = grouped_merge_pair(before, &targets).ok_or(CoreError::InvalidArgument(
            "merge requires one adjacent compatible upper/lower target pair",
        ))?;
        let output = match pair {
            MergePair::Layers { upper, lower } => {
                if before.layers[upper].kind == LayerKind::Adjustment
                    || before.layers[upper].kind != before.layers[lower].kind
                    || before.layers[upper].planes.len() != before.layers[lower].planes.len()
                    || before.layers[upper]
                        .planes
                        .iter()
                        .zip(&before.layers[lower].planes)
                        .any(|(left, right)| {
                            left.kind != right.kind || left.raster.format() != right.raster.format()
                        })
                {
                    return Err(CoreError::InvalidArgument(
                        "selected layers do not have compatible topology",
                    ));
                }
                let source_planes = after.layers[upper].planes.clone();
                let lower_id = after.layers[lower].id;
                for (destination, source) in
                    after.layers[lower].planes.iter_mut().zip(&source_planes)
                {
                    merge_raster(&mut destination.raster, &source.raster, revision)?;
                    after.vector.reassign_plane(source.id, destination.id);
                }
                after.layers.remove(upper);
                EditTarget::Layer(lower_id.get())
            }
            MergePair::Planes {
                layer,
                upper,
                lower,
            } => {
                let source = after.layers[layer].planes[upper].clone();
                let destination_id = after.layers[layer].planes[lower].id;
                merge_raster(
                    &mut after.layers[layer].planes[lower].raster,
                    &source.raster,
                    revision,
                )?;
                after.vector.reassign_plane(source.id, destination_id);
                after.layers[layer].planes.remove(upper);
                validate_layer_kind(after.layers[layer].kind, &after.layers[layer].planes)?;
                EditTarget::Plane(EditorTarget {
                    layer_id: after.layers[layer].id.get(),
                    plane_id: destination_id.get(),
                })
            }
        };
        let outputs = vec![output];
        let preferred_active = preferred_active_target(after, &outputs);
        edit.prefer_edit_targets(outputs.clone());
        if let Some(target) = preferred_active {
            edit.prefer_editor_target(target);
        }
        let dispatch = edit.commit(self)?;
        Ok(EditTargetCommandResult {
            dispatch,
            output_targets: outputs,
        })
    }
}

fn preferred_active_target(
    document: &CellDocument,
    outputs: &[EditTarget],
) -> Option<EditorTarget> {
    outputs.first().and_then(|target| match target {
        EditTarget::Layer(layer_id) => document
            .layers
            .iter()
            .find(|layer| layer.id.get() == *layer_id)
            .and_then(|layer| layer.planes.first())
            .map(|plane| EditorTarget {
                layer_id: *layer_id,
                plane_id: plane.id.get(),
            }),
        EditTarget::Plane(target) => Some(*target),
    })
}

fn grouped_delete_is_valid(document: &CellDocument, targets: &[EditTarget]) -> bool {
    let selected = targets.iter().copied().collect::<BTreeSet<_>>();
    let remaining_coloring = document
        .layers
        .iter()
        .filter(|layer| {
            is_coloring_layer(layer.kind) && !selected.contains(&EditTarget::Layer(layer.id.get()))
        })
        .count();
    if remaining_coloring == 0 {
        return false;
    }
    document.layers.iter().all(|layer| {
        if selected.contains(&EditTarget::Layer(layer.id.get())) {
            return true;
        }
        let planes = layer
            .planes
            .iter()
            .filter(|plane| {
                !selected.contains(&EditTarget::Plane(EditorTarget {
                    layer_id: layer.id.get(),
                    plane_id: plane.id.get(),
                }))
            })
            .cloned()
            .collect::<Vec<_>>();
        validate_layer_kind(layer.kind, &planes).is_ok()
    })
}

#[derive(Clone, Copy)]
enum MergePair {
    Layers {
        upper: usize,
        lower: usize,
    },
    Planes {
        layer: usize,
        upper: usize,
        lower: usize,
    },
}

fn grouped_merge_pair(document: &CellDocument, targets: &[EditTarget]) -> Option<MergePair> {
    if targets.len() != 2 {
        return None;
    }
    match (targets[0], targets[1]) {
        (EditTarget::Layer(upper_id), EditTarget::Layer(lower_id)) => {
            let upper = document
                .layers
                .iter()
                .position(|layer| layer.id.get() == upper_id)?;
            let lower = document
                .layers
                .iter()
                .position(|layer| layer.id.get() == lower_id)?;
            (lower == upper + 1).then_some(MergePair::Layers { upper, lower })
        }
        (EditTarget::Plane(upper_target), EditTarget::Plane(lower_target))
            if upper_target.layer_id == lower_target.layer_id =>
        {
            let layer = document
                .layers
                .iter()
                .position(|layer| layer.id.get() == upper_target.layer_id)?;
            let upper = document.layers[layer]
                .planes
                .iter()
                .position(|plane| plane.id.get() == upper_target.plane_id)?;
            let lower = document.layers[layer]
                .planes
                .iter()
                .position(|plane| plane.id.get() == lower_target.plane_id)?;
            let source = &document.layers[layer].planes[upper];
            let destination = &document.layers[layer].planes[lower];
            (lower == upper + 1
                && source.kind == destination.kind
                && source.raster.format() == destination.raster.format()
                && !(document.layers[layer].kind == LayerKind::VectorColoring
                    && matches!(
                        source.kind,
                        PlaneType::VectorMainLine | PlaneType::VectorFill
                    )))
            .then_some(MergePair::Planes {
                layer,
                upper,
                lower,
            })
        }
        _ => None,
    }
}

const fn is_required_singleton_plane(kind: PlaneType) -> bool {
    matches!(
        kind,
        PlaneType::MainLine | PlaneType::Color | PlaneType::VectorMainLine | PlaneType::VectorFill
    )
}

const fn is_vector_plane(kind: PlaneType) -> bool {
    matches!(
        kind,
        PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill
    )
}
