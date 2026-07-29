use super::*;

impl Core {
    pub fn set_active_plane(&mut self, plane: ActivePlane) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let kind = match plane {
            ActivePlane::MainLine => PlaneType::MainLine,
            ActivePlane::Color => PlaneType::Color,
        };
        let (layer_id, plane_id) = document
            .layers
            .iter()
            .find_map(|layer| {
                layer
                    .planes
                    .iter()
                    .find(|candidate| candidate.kind == kind)
                    .map(|candidate| (layer.id, candidate.id))
            })
            .ok_or(CoreError::InvalidState(
                "requested plane role is unavailable",
            ))?;
        document.active_layer_id = layer_id;
        document.active_plane_id = plane_id;
        Ok(())
    }

    pub fn layers(&self) -> Result<Vec<LayerInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .layers
            .iter()
            .map(LayerNode::info)
            .collect())
    }

    pub fn set_active_node(&mut self, layer_id: u64, plane_id: u64) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let layer = document
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if !layer.planes.iter().any(|plane| plane.id == plane_id) {
            return Err(CoreError::InvalidArgument(
                "plane ID does not belong to the requested layer",
            ));
        }
        document.active_layer_id = layer_id;
        document.active_plane_id = plane_id;
        Ok(())
    }

    pub fn create_layer(
        &mut self,
        kind: LayerKind,
        name: &str,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let layer_id = self.allocate_id();
        let mut planes = Vec::new();
        let (width, height) = (before.width, before.height);
        match kind {
            LayerKind::BinaryColoring | LayerKind::GrayscaleColoring => {
                let main_id = self.allocate_id();
                let color_id = self.allocate_id();
                planes.push(PlaneNode {
                    id: main_id,
                    kind: PlaneType::MainLine,
                    name: "Main Line".to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                    raster: TileRaster::new(
                        width,
                        height,
                        if kind == LayerKind::BinaryColoring {
                            PixelFormat::BinaryMask8
                        } else {
                            PixelFormat::Grayscale8
                        },
                    )?,
                });
                planes.push(PlaneNode {
                    id: color_id,
                    kind: PlaneType::Color,
                    name: "Color".to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                    raster: TileRaster::new(width, height, PixelFormat::StraightRgba8)?,
                });
            }
            LayerKind::Raster => planes.push(PlaneNode {
                id: self.allocate_id(),
                kind: PlaneType::Raster,
                name: "Raster".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: TileRaster::new(width, height, PixelFormat::StraightRgba8)?,
            }),
            LayerKind::Selection => planes.push(PlaneNode {
                id: self.allocate_id(),
                kind: PlaneType::Selection,
                name: "Selection".to_owned(),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                raster: TileRaster::new(width, height, PixelFormat::BinaryMask8)?,
            }),
            LayerKind::VectorColoring => {
                for (kind, name) in [
                    (PlaneType::VectorMainLine, "Vector Main Line"),
                    (PlaneType::ColorTrace, "Color Trace"),
                    (PlaneType::VectorFill, "Vector Fill"),
                ] {
                    planes.push(PlaneNode {
                        id: self.allocate_id(),
                        kind,
                        name: name.to_owned(),
                        visible: true,
                        editable: true,
                        opacity_milli: 1_000,
                        raster: TileRaster::new(width, height, PixelFormat::StraightRgba8)?,
                    });
                }
            }
            LayerKind::Frame
            | LayerKind::VanishingPoint
            | LayerKind::Adjustment
            | LayerKind::Text
            | LayerKind::Annotation => {}
        }
        validate_layer_kind(kind, &planes)?;
        let mut after = before.clone();
        after.layers.push(LayerNode {
            id: layer_id,
            kind,
            name: unique_layer_name(&after.layers, name),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            planes,
        });
        if kind == LayerKind::Adjustment {
            after.adjustments.insert(
                layer_id,
                Adjustment::BrightnessContrast {
                    brightness_milli: 0,
                    contrast_milli: 0,
                },
            );
        }
        after.active_layer_id = layer_id;
        if let Some(plane) = after.layers.last().and_then(|layer| layer.planes.first()) {
            after.active_plane_id = plane.id;
        }
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, layer_id))
    }

    pub fn duplicate_layer(&mut self, layer_id: u64) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        let mut next_id = self.next_id;
        let allocate_id = |next_id: &mut u64| {
            let id = *next_id;
            *next_id = next_id.saturating_add(1).max(1);
            id
        };
        let mut duplicate = before.layers[index].clone();
        duplicate.id = allocate_id(&mut next_id);
        duplicate.name = unique_layer_name(&before.layers, &format!("{} Copy", duplicate.name));
        let mut plane_map = BTreeMap::new();
        for plane in &mut duplicate.planes {
            let source_id = plane.id;
            plane.id = allocate_id(&mut next_id);
            plane_map.insert(source_id, plane.id);
            plane.name = format!("{} Copy", plane.name);
        }
        let duplicate_id = duplicate.id;
        let active_plane_id = duplicate.planes.first().map(|plane| plane.id);
        let mut after = before.clone();
        after.vector.duplicate_planes(&plane_map, &mut next_id);
        if let Some(adjustment) = before.adjustments.get(&layer_id).cloned() {
            after.adjustments.insert(duplicate_id, adjustment);
        }
        after.vector.ensure_limits()?;
        after.layers.insert(index + 1, duplicate);
        after.active_layer_id = duplicate_id;
        if let Some(id) = active_plane_id {
            after.active_plane_id = id;
        }
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, duplicate_id))
    }

    pub fn delete_layer(&mut self, layer_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if is_coloring_layer(before.layers[index].kind)
            && before
                .layers
                .iter()
                .filter(|layer| is_coloring_layer(layer.kind))
                .count()
                == 1
        {
            return Err(CoreError::InvalidState(
                "the final coloring layer cannot be deleted",
            ));
        }
        let mut after = before.clone();
        after.vector.remove_layer(&before, layer_id);
        after.adjustments.remove(&layer_id);
        after.layers.remove(index);
        if after.active_layer_id == layer_id {
            let replacement = after
                .layers
                .get(index.min(after.layers.len().saturating_sub(1)))
                .ok_or(CoreError::InvalidState("document must retain a layer"))?;
            after.active_layer_id = replacement.id;
            after.active_plane_id = replacement
                .planes
                .first()
                .map_or(after.primary_ids().1, |plane| plane.id);
        }
        if after.plane_by_id(after.active_plane_id).is_none() {
            after.active_plane_id = after
                .layers
                .iter()
                .find(|layer| layer.id == after.active_layer_id)
                .and_then(|layer| layer.planes.first())
                .map_or(after.primary_ids().1, |plane| plane.id);
        }
        self.commit_document_edit(before, after)
    }

    pub fn reorder_layer(
        &mut self,
        layer_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let source = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if destination_index >= before.layers.len() {
            return Err(CoreError::InvalidArgument(
                "layer destination index is outside the tree",
            ));
        }
        if source == destination_index {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        let layer = after.layers.remove(source);
        after.layers.insert(destination_index, layer);
        self.commit_document_edit(before, after)
    }

    pub fn set_layer_properties(
        &mut self,
        layer_id: u64,
        visible: bool,
        editable: bool,
        opacity_milli: u32,
        name: &str,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument("opacity exceeds 1000"));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let layer = after
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        layer.visible = visible;
        layer.editable = editable;
        layer.opacity_milli = opacity_milli;
        layer.name = name.to_owned();
        if after.layers == before.layers {
            return Ok(self.noop_outcome());
        }
        self.commit_document_edit(before, after)
    }

    pub fn create_plane(
        &mut self,
        layer_id: u64,
        kind: PlaneType,
        format: PixelFormat,
        name: &str,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        validate_plane_format(kind, format)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let layer_index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if before.layers[layer_index].planes.len() >= MAX_PLANES_PER_LAYER {
            return Err(CoreError::InvalidState("plane limit reached"));
        }
        let plane_id = self.allocate_id();
        let mut after = before.clone();
        after.layers[layer_index].planes.push(PlaneNode {
            id: plane_id,
            kind,
            name: name.to_owned(),
            visible: true,
            editable: true,
            opacity_milli: 1_000,
            raster: TileRaster::new(after.width, after.height, format)?,
        });
        validate_layer_kind(
            after.layers[layer_index].kind,
            &after.layers[layer_index].planes,
        )?;
        after.active_layer_id = layer_id;
        after.active_plane_id = plane_id;
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, plane_id))
    }

    pub fn duplicate_plane(&mut self, plane_id: u64) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, plane_index) = find_plane_indices(&before, plane_id)?;
        if before.layers[layer_index].planes.len() >= MAX_PLANES_PER_LAYER {
            return Err(CoreError::InvalidState("plane limit reached"));
        }
        if matches!(
            before.layers[layer_index].planes[plane_index].kind,
            PlaneType::MainLine
                | PlaneType::Color
                | PlaneType::VectorMainLine
                | PlaneType::VectorFill
        ) {
            return Err(CoreError::InvalidState(
                "required singleton planes cannot be duplicated",
            ));
        }
        let mut duplicate = before.layers[layer_index].planes[plane_index].clone();
        let source_plane_id = duplicate.id;
        let mut next_id = self.next_id;
        let duplicate_id = next_id;
        next_id = next_id.saturating_add(1).max(1);
        duplicate.id = duplicate_id;
        duplicate.name = unique_plane_name(
            &before.layers[layer_index].planes,
            &format!("{} Copy", duplicate.name),
        );
        let mut after = before.clone();
        let mut plane_map = BTreeMap::new();
        plane_map.insert(source_plane_id, duplicate_id);
        after.vector.duplicate_planes(&plane_map, &mut next_id);
        after.vector.ensure_limits()?;
        after.layers[layer_index]
            .planes
            .insert(plane_index + 1, duplicate);
        after.active_layer_id = after.layers[layer_index].id;
        after.active_plane_id = duplicate_id;
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, duplicate_id))
    }

    pub fn delete_plane(&mut self, plane_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, plane_index) = find_plane_indices(&before, plane_id)?;
        let mut after = before.clone();
        after.vector.remove_plane(plane_id);
        after.layers[layer_index].planes.remove(plane_index);
        validate_layer_kind(
            after.layers[layer_index].kind,
            &after.layers[layer_index].planes,
        )?;
        if after.active_plane_id == plane_id {
            after.active_plane_id = after.layers[layer_index]
                .planes
                .first()
                .map_or(after.primary_ids().1, |plane| plane.id);
        }
        self.commit_document_edit(before, after)
    }

    pub fn reorder_plane(
        &mut self,
        plane_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, source) = find_plane_indices(&before, plane_id)?;
        if destination_index >= before.layers[layer_index].planes.len() {
            return Err(CoreError::InvalidArgument(
                "plane destination index is outside its layer",
            ));
        }
        if source == destination_index {
            return Ok(self.noop_outcome());
        }
        let mut after = before.clone();
        let plane = after.layers[layer_index].planes.remove(source);
        after.layers[layer_index]
            .planes
            .insert(destination_index, plane);
        self.commit_document_edit(before, after)
    }

    pub fn set_plane_properties(
        &mut self,
        plane_id: u64,
        visible: bool,
        editable: bool,
        opacity_milli: u32,
        name: &str,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument("opacity exceeds 1000"));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let plane = after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidArgument("plane ID does not exist"))?;
        plane.visible = visible;
        plane.editable = editable;
        plane.opacity_milli = opacity_milli;
        plane.name = name.to_owned();
        if after.layers == before.layers {
            return Ok(self.noop_outcome());
        }
        self.commit_document_edit(before, after)
    }

    pub fn convert_plane(
        &mut self,
        plane_id: u64,
        destination_kind: PlaneType,
        destination_format: PixelFormat,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_plane_format(destination_kind, destination_format)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, plane_index) = find_plane_indices(&before, plane_id)?;
        let source = &before.layers[layer_index].planes[plane_index];
        if source.kind == destination_kind && source.raster.format() == destination_format {
            return Ok(self.noop_outcome());
        }
        if matches!(
            source.kind,
            PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill
        ) || matches!(
            destination_kind,
            PlaneType::VectorMainLine | PlaneType::ColorTrace | PlaneType::VectorFill
        ) {
            return Err(CoreError::InvalidArgument(
                "vector plane conversion requires explicit rasterize/vectorize",
            ));
        }
        let revision = self.next_document_revision()?;
        let converted = convert_plane_raster(&source.raster, destination_format, revision)?;
        let mut after = before.clone();
        let plane = &mut after.layers[layer_index].planes[plane_index];
        plane.kind = destination_kind;
        plane.raster = converted;
        validate_layer_kind(
            after.layers[layer_index].kind,
            &after.layers[layer_index].planes,
        )?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn merge_plane_into_below(&mut self, plane_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let (layer_index, upper) = find_plane_indices(&before, plane_id)?;
        if upper + 1 >= before.layers[layer_index].planes.len() {
            return Err(CoreError::InvalidArgument("plane has no lower sibling"));
        }
        let lower = upper + 1;
        let source = &before.layers[layer_index].planes[upper];
        let destination = &before.layers[layer_index].planes[lower];
        if source.kind != destination.kind || source.raster.format() != destination.raster.format()
        {
            return Err(CoreError::InvalidArgument(
                "only planes with compatible type and pixel format can merge",
            ));
        }
        if before.layers[layer_index].kind == LayerKind::VectorColoring
            && matches!(
                source.kind,
                PlaneType::VectorMainLine | PlaneType::VectorFill
            )
        {
            return Err(CoreError::InvalidArgument(
                "required singleton vector planes cannot be merged",
            ));
        }
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        let source = after.layers[layer_index].planes[upper].clone();
        let destination_id = after.layers[layer_index].planes[lower].id;
        merge_raster(
            &mut after.layers[layer_index].planes[lower].raster,
            &source.raster,
            revision,
        )?;
        after.vector.reassign_plane(source.id, destination_id);
        after.layers[layer_index].planes.remove(upper);
        after.active_layer_id = after.layers[layer_index].id;
        after.active_plane_id = destination_id;
        validate_layer_kind(
            after.layers[layer_index].kind,
            &after.layers[layer_index].planes,
        )?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn convert_layer(
        &mut self,
        layer_id: u64,
        destination: LayerKind,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let index = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        let source = before.layers[index].kind;
        if source == destination {
            return Ok(self.noop_outcome());
        }
        if !matches!(
            (source, destination),
            (LayerKind::BinaryColoring, LayerKind::GrayscaleColoring)
                | (LayerKind::GrayscaleColoring, LayerKind::BinaryColoring)
        ) {
            return Err(CoreError::InvalidArgument(
                "requested layer conversion would lose unsupported semantics",
            ));
        }
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        let main = after.layers[index]
            .planes
            .iter_mut()
            .find(|plane| plane.kind == PlaneType::MainLine)
            .ok_or(CoreError::InvalidState("coloring layer has no main plane"))?;
        main.raster = convert_main_line_raster(
            &main.raster,
            destination == LayerKind::GrayscaleColoring,
            revision,
        )?;
        after.layers[index].kind = destination;
        validate_layer_kind(destination, &after.layers[index].planes)?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn merge_layer_into_below(&mut self, layer_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let upper = before
            .layers
            .iter()
            .position(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if upper + 1 >= before.layers.len() {
            return Err(CoreError::InvalidArgument("layer has no lower sibling"));
        }
        let lower = upper + 1;
        if before.layers[upper].kind == LayerKind::Adjustment {
            return Err(CoreError::InvalidArgument(
                "adjustment layers cannot merge without an explicit parameter composition",
            ));
        }
        if before.layers[upper].kind != before.layers[lower].kind
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
                "only layers with compatible type and plane topology can merge",
            ));
        }
        let revision = self.next_document_revision()?;
        let mut after = before.clone();
        let source_planes = after.layers[upper].planes.clone();
        let lower_id = after.layers[lower].id;
        let lower_plane_id = after.layers[lower]
            .planes
            .first()
            .map_or(after.primary_ids().1, |plane| plane.id);
        let mut plane_reassignments = Vec::new();
        for (destination, source) in after.layers[lower].planes.iter_mut().zip(&source_planes) {
            merge_raster(&mut destination.raster, &source.raster, revision)?;
            plane_reassignments.push((source.id, destination.id));
        }
        for (source_id, destination_id) in plane_reassignments {
            after.vector.reassign_plane(source_id, destination_id);
        }
        after.layers.remove(upper);
        after.active_layer_id = lower_id;
        after.active_plane_id = lower_plane_id;
        self.commit_document_edit_with_revision(before, after, revision)
    }
}

// Shared implementation helpers for this responsibility.

pub(crate) fn validate_node_name(name: &str) -> Result<(), CoreError> {
    if name.is_empty() || name.len() > 1_024 || name.chars().any(char::is_control) {
        Err(CoreError::InvalidArgument("node name is invalid"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_plane_id_remains_valid_after_deleting_a_duplicate_layer() {
        let mut core = Core::new();
        let created = core.new_cell(1, 1, 96_000, 96_000).unwrap();
        let (_, duplicate) = core.duplicate_layer(created.layer_id).unwrap();
        core.create_layer(LayerKind::Frame, "Frame").unwrap();
        core.delete_layer(duplicate).unwrap();

        let document = core.document.as_ref().unwrap();
        assert!(document.plane_by_id(document.active_plane_id).is_some());
    }
}
