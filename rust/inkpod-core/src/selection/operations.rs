use super::*;

impl Core {
    /// Combines a document-space shape with the current selection.
    ///
    /// Success is one undoable document edit; an unchanged mask is a no-op.
    /// Invalid geometry or allocation failure leaves the prior mask intact.
    pub fn apply_selection(
        &mut self,
        shape: &SelectionShape,
        operation: SelectionOperation,
    ) -> Result<DispatchOutcome, CoreError> {
        let target = self.active_editor_target()?;
        self.apply_selection_for_editor_target(shape, operation, target)
    }

    /// Combines a selection shape using the stable target captured at gesture begin.
    ///
    /// The exact layer/plane pair must still exist. Later EditorState target
    /// changes cannot redirect source-dependent selection shapes. A real mask
    /// change remains one document revision and Undo unit; no-op/failure are atomic.
    pub fn apply_selection_for_editor_target(
        &mut self,
        shape: &SelectionShape,
        operation: SelectionOperation,
        target: EditorTarget,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let (_, active_plane_id) = self.editor_target_ids(target)?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let candidate = selection_mask_for_shape(before, active_plane_id, shape, revision.get())?;
        after.selection =
            combine_selection_masks(&before.selection, &candidate, operation, revision.get())?;
        edit.commit(self)
    }

    /// Inverts selection coverage across the whole document as one undoable edit.
    pub fn invert_selection(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        after.selection = invert_selection_mask(&before.selection, revision.get())?;
        edit.commit(self)
    }

    /// Clears the current selection as one undoable edit.
    ///
    /// An already-empty selection is a no-op.
    pub fn clear_selection(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if mask_bounds(&document.selection)?.is_none() {
            return Ok(self.noop_outcome());
        }
        let empty = TileRaster::new(document.width, document.height, PixelFormat::BinaryMask8)?;
        let mut edit = self.begin_document_edit()?;
        edit.working_mut().selection = empty;
        edit.commit(self)
    }

    /// Expands a selection by positive pixels or contracts it by negative pixels.
    ///
    /// The magnitude is bounded to 4096 document pixels. Success is one undoable
    /// edit; invalid input or processing failure is atomic.
    pub fn resize_selection(&mut self, pixels: i32) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if pixels == i32::MIN || pixels.unsigned_abs() > 4_096 {
            return Err(CoreError::InvalidArgument(
                "selection expansion is outside its bound",
            ));
        }
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        after.selection = morphology_selection(&before.selection, pixels, revision.get())?;
        edit.commit(self)
    }

    /// Selects pixels from the current editor target captured at command start.
    ///
    /// `tolerance` is an inclusive channel tolerance. The candidate mask is combined
    /// using `operation` and committed as one undoable edit.
    pub fn select_color(
        &mut self,
        color: PixelValue,
        tolerance: u16,
        different: bool,
        operation: SelectionOperation,
    ) -> Result<DispatchOutcome, CoreError> {
        let target = self.active_editor_target()?;
        self.select_color_for_editor_target(color, tolerance, different, operation, target)
    }

    /// Selects pixels from the stable target captured when a command began.
    ///
    /// The exact layer/plane pair must still exist in the current document
    /// namespace. Later EditorState target changes cannot redirect the color
    /// source. A real mask change remains one document revision and Undo unit;
    /// no-op and failure leave document and editor state unchanged.
    pub fn select_color_for_editor_target(
        &mut self,
        color: PixelValue,
        tolerance: u16,
        different: bool,
        operation: SelectionOperation,
        target: EditorTarget,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let (_, active_plane_id) = self.editor_target_ids(target)?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let source = before
            .plane_by_id(active_plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        let candidate =
            color_selection_mask(&source.raster, color, tolerance, different, revision.get())?;
        after.selection =
            combine_selection_masks(&before.selection, &candidate, operation, revision.get())?;
        edit.commit(self)
    }

    /// Returns the smallest half-open document rectangle containing selection coverage.
    ///
    /// Returns `None` for an empty selection without mutating Core state.
    pub fn selection_bounds(&self) -> Result<Option<RectI32>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        mask_bounds(&document.selection)
    }

    /// Copies the current selection mask into a new selection layer.
    ///
    /// Success is one undoable edit and returns the stable ID of the new layer.
    pub fn selection_to_layer(&mut self, name: &str) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let selection = {
            let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
            if document.layers.len() >= MAX_LAYERS {
                return Err(CoreError::InvalidState("layer limit reached"));
            }
            if mask_bounds(&document.selection)?.is_none() {
                return Err(CoreError::InvalidState("selection is empty"));
            }
            document.selection.clone()
        };
        let mut next_id = self.next_id;
        let layer_id = next_id.take_layer();
        let plane_id = next_id.take_plane();
        let mut edit = self.begin_document_edit()?;
        {
            let after = edit.working_mut();
            after.layers.push(LayerNode {
                id: layer_id,
                kind: LayerKind::Selection,
                name: unique_layer_name(&after.layers, name),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: vec![PlaneNode {
                    id: plane_id,
                    kind: PlaneType::Selection,
                    name: "Selection".to_owned(),
                    visible: true,
                    editable: true,
                    opacity_milli: 1_000,
                    raster: selection,
                }],
            });
        }
        edit.prefer_editor_target(EditorTarget {
            layer_id: layer_id.get(),
            plane_id: plane_id.get(),
        });
        let outcome = edit.commit(self)?;
        self.next_id = next_id;
        Ok((outcome, layer_id.get()))
    }

    /// Combines a selection layer's mask with the current selection.
    ///
    /// The source layer remains unchanged. Success is one undoable edit and an
    /// unchanged result is a no-op.
    pub fn selection_from_layer(
        &mut self,
        layer_id: u64,
        operation: SelectionLayerOperation,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let layer_id = LayerId::from_raw(layer_id);
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let layer = before
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        let mask = layer
            .planes
            .iter()
            .find(|plane| plane.kind == PlaneType::Selection)
            .ok_or(CoreError::InvalidArgument(
                "layer does not contain a selection plane",
            ))?;
        let selection_operation = match operation {
            SelectionLayerOperation::Replace => SelectionOperation::New,
            SelectionLayerOperation::Add => SelectionOperation::Add,
            SelectionLayerOperation::Subtract => SelectionOperation::Subtract,
        };
        after.selection = combine_selection_masks(
            &before.selection,
            &mask.raster,
            selection_operation,
            revision.get(),
        )?;
        edit.commit(self)
    }

    /// Copies selected pixels from the active raster plane into an owned payload.
    ///
    /// Coordinates and half-open bounds remain in document space. The query does
    /// not change revision, history, selection, or dirty state.
    pub fn copy_selection(&self) -> Result<ClipboardPayload, CoreError> {
        let (_, active_plane_id) = self.active_editor_target_ids()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let bounds = mask_bounds(&document.selection)?
            .ok_or(CoreError::InvalidState("selection is empty"))?;
        let plane = document
            .plane_by_id(active_plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        if !matches!(
            plane.kind,
            PlaneType::MainLine | PlaneType::Color | PlaneType::Raster | PlaneType::Selection
        ) {
            return Err(CoreError::InvalidState("active plane is not copyable"));
        }
        let mut pixels = Vec::new();
        for y in bounds.y..bounds.y + bounds.height {
            for x in bounds.x..bounds.x + bounds.width {
                let (Ok(x_u32), Ok(y_u32)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                if !matches!(
                    document.selection.pixel(x_u32, y_u32)?,
                    PixelValue::Binary(255)
                ) {
                    continue;
                }
                let value = plane.raster.pixel(x_u32, y_u32)?;
                if !value.is_zero() {
                    pixels.push(ClipboardPixel { x, y, value });
                }
            }
        }
        Ok(ClipboardPayload {
            source_document_uuid: document.uuid,
            bounds,
            planes: vec![ClipboardPlane {
                kind: plane.kind,
                pixel_format: plane.raster.format(),
                origin_x: bounds.x,
                origin_y: bounds.y,
                pixels,
            }],
        })
    }

    /// Starts a floating paste after validating payload plane compatibility.
    ///
    /// This stages an isolated preview and does not change document revision,
    /// history, or dirty state until [`Core::commit_floating`]. Failure is atomic.
    pub fn begin_paste(&mut self, payload: &ClipboardPayload) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        if self.floating.is_some() {
            return Err(CoreError::InvalidState("floating paste is already active"));
        }
        if payload.planes.is_empty() || payload.planes.len() > MAX_PLANES_PER_LAYER {
            return Err(CoreError::InvalidArgument(
                "clipboard plane count is invalid",
            ));
        }
        if payload.bounds.width <= 0
            || payload.bounds.height <= 0
            || payload.bounds.x.checked_add(payload.bounds.width).is_none()
            || payload
                .bounds
                .y
                .checked_add(payload.bounds.height)
                .is_none()
        {
            return Err(CoreError::InvalidArgument(
                "clipboard bounds are outside the supported range",
            ));
        }
        let (_, active_plane_id) = self.active_editor_target_ids()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let active_destination = document
            .plane_by_id(active_plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        let compatible_source = |destination: &PlaneNode| {
            payload.planes.iter().find(|plane| {
                plane.kind == destination.kind && plane.pixel_format == destination.raster.format()
            })
        };
        let active_layer = document.layers.iter().find(|layer| {
            layer
                .planes
                .iter()
                .any(|plane| plane.id == active_destination.id)
        });
        let (destination, source) =
            compatible_source(active_destination)
                .map(|source| (active_destination, source))
                .or_else(|| {
                    active_layer.and_then(|layer| {
                        layer.planes.iter().find_map(|plane| {
                            compatible_source(plane).map(|source| (plane, source))
                        })
                    })
                })
                .or_else(|| {
                    document.layers.iter().find_map(|layer| {
                        layer.planes.iter().find_map(|plane| {
                            compatible_source(plane).map(|source| (plane, source))
                        })
                    })
                })
                .ok_or(CoreError::InvalidArgument(
                    "clipboard has no compatible typed destination payload",
                ))?;
        if source.pixels.len() as u64 > MAX_FILL_PIXELS {
            return Err(CoreError::InvalidArgument(
                "clipboard payload exceeds work limit",
            ));
        }
        if source.pixels.iter().any(|pixel| {
            pixel.x < payload.bounds.x
                || pixel.y < payload.bounds.y
                || pixel.x >= payload.bounds.x + payload.bounds.width
                || pixel.y >= payload.bounds.y + payload.bounds.height
        }) {
            return Err(CoreError::InvalidArgument(
                "clipboard pixel lies outside its bounds",
            ));
        }
        let (staged_assets, asset_ids) = stage_clipboard_assets(self, payload)?;
        self.floating = Some(FloatingSelection {
            payload: payload.clone(),
            destination_plane_id: destination.id,
            transform: FloatingTransform::default(),
            asset_ids,
        });
        self.assets = staged_assets;
        Ok(())
    }

    /// Starts a floating paste converted to the active plane's pixel format.
    ///
    /// Conversion is fully staged; failure leaves the document and any prior
    /// revision/history state unchanged.
    pub fn begin_paste_to_active_converted(
        &mut self,
        payload: &ClipboardPayload,
    ) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        if self.floating.is_some() {
            return Err(CoreError::InvalidState("floating paste is already active"));
        }
        let (_, active_plane_id) = self.active_editor_target_ids()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let destination = document
            .plane_by_id(active_plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        ensure_editable_plane(document, destination.id)?;
        let source = payload
            .planes
            .first()
            .ok_or(CoreError::InvalidArgument("clipboard has no plane payload"))?;
        if source.pixels.len() as u64 > MAX_FILL_PIXELS {
            return Err(CoreError::InvalidArgument(
                "clipboard payload exceeds work limit",
            ));
        }
        let mut converted = payload.clone();
        converted.planes.truncate(1);
        converted.planes[0].kind = destination.kind;
        converted.planes[0].pixel_format = destination.raster.format();
        for pixel in &mut converted.planes[0].pixels {
            pixel.value = convert_plane_pixel(pixel.value, destination.raster.format())?;
        }
        let (staged_assets, asset_ids) = stage_clipboard_assets(self, &converted)?;
        self.floating = Some(FloatingSelection {
            payload: converted,
            destination_plane_id: destination.id,
            transform: FloatingTransform::default(),
            asset_ids,
        });
        self.assets = staged_assets;
        Ok(())
    }

    /// Clears selected pixels on the active editable plane.
    ///
    /// Empty selection or already-clear pixels are a no-op. A real change is one
    /// undoable document edit; failures do not publish partial clearing.
    pub fn clear_selected_content(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let (_, plane_id) = self.active_editor_target_ids()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        ensure_editable_plane(document, plane_id)?;
        let zero = zero_pixel(
            document
                .plane_by_id(plane_id)
                .ok_or(CoreError::InvalidState("active plane is missing"))?
                .raster
                .format(),
        )?;
        let mut coordinates = Vec::new();
        for y in 0..document.height {
            for x in 0..document.width {
                if document.selection.pixel(x, y)? != PixelValue::Binary(0) {
                    coordinates.push((x, y));
                }
            }
        }
        if coordinates.is_empty() {
            return Err(CoreError::InvalidState("selection contains no pixels"));
        }
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let plane = document
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("active plane is missing"))?;
        let mut changes = Vec::new();
        for (x, y) in coordinates {
            let before = plane.raster.pixel(x, y)?;
            if before != zero {
                plane.raster.set_pixel(x, y, zero, revision.get())?;
                changes.push(PixelChange {
                    x,
                    y,
                    before,
                    after: zero,
                });
            }
        }
        if changes.is_empty() {
            return Ok(self.noop_outcome());
        }
        self.document_revision = revision;
        self.commit_pixel_history(plane_id, changes, after_state);
        Ok(DispatchOutcome {
            revision: revision.get(),
            accepted_commands: 1,
        })
    }

    /// Replaces the transform of the active floating-paste preview.
    ///
    /// Values must be finite with nonzero bounded scales. This does not commit
    /// document, revision, history, or dirty state.
    pub fn set_floating_transform(
        &mut self,
        transform: FloatingTransform,
    ) -> Result<(), CoreError> {
        validate_floating_transform(transform)?;
        self.floating
            .as_mut()
            .ok_or(CoreError::InvalidState("there is no floating paste"))?
            .transform = transform;
        Ok(())
    }

    /// Discards the floating-paste preview and restores the unmodified base state.
    pub fn cancel_floating(&mut self) {
        let retained_assets = self.asset_store_without_floating().ok();
        self.floating = None;
        if let Some(assets) = retained_assets {
            self.assets = assets;
        }
    }

    /// Atomically applies the active floating paste as one undoable edit.
    ///
    /// Success clears the preview. Validation or raster failure keeps the live
    /// document unchanged so the caller may adjust or cancel the preview.
    pub fn commit_floating(&mut self) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let floating = self
            .floating
            .clone()
            .ok_or(CoreError::InvalidState("there is no floating paste"))?;
        let retained_assets = self.asset_store_without_floating()?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let destination =
            document
                .plane_by_id(floating.destination_plane_id)
                .ok_or(CoreError::InvalidState(
                    "paste destination no longer exists",
                ))?;
        ensure_editable_plane(document, floating.destination_plane_id)?;
        let source = floating
            .payload
            .planes
            .iter()
            .find(|plane| {
                plane.kind == destination.kind && plane.pixel_format == destination.raster.format()
            })
            .ok_or(CoreError::InvalidArgument(
                "compatible clipboard plane is missing",
            ))?;
        let mut staged = BTreeMap::new();
        let center_x = f64::from(floating.payload.bounds.x)
            + f64::from(floating.payload.bounds.width - 1) / 2.0;
        let center_y = f64::from(floating.payload.bounds.y)
            + f64::from(floating.payload.bounds.height - 1) / 2.0;
        let radians = floating.transform.rotation_degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        let transform_point = |x: f64, y: f64| {
            let local_x = (x - center_x) * floating.transform.scale_x;
            let local_y = (y - center_y) * floating.transform.scale_y;
            (
                center_x + local_x * cos - local_y * sin + floating.transform.translate_x,
                center_y + local_x * sin + local_y * cos + floating.transform.translate_y,
            )
        };
        let left = f64::from(floating.payload.bounds.x);
        let top = f64::from(floating.payload.bounds.y);
        let right = f64::from(floating.payload.bounds.x + floating.payload.bounds.width - 1);
        let bottom = f64::from(floating.payload.bounds.y + floating.payload.bounds.height - 1);
        let corners = [
            transform_point(left, top),
            transform_point(right, top),
            transform_point(left, bottom),
            transform_point(right, bottom),
        ];
        if corners
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
        {
            return Err(CoreError::InvalidArgument("floating transform overflowed"));
        }
        let min_x = corners
            .iter()
            .map(|corner| corner.0)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let max_x = corners
            .iter()
            .map(|corner| corner.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(f64::from(document.width.saturating_sub(1))) as i64;
        let min_y = corners
            .iter()
            .map(|corner| corner.1)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let max_y = corners
            .iter()
            .map(|corner| corner.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(f64::from(document.height.saturating_sub(1))) as i64;
        if min_x <= max_x && min_y <= max_y {
            let work = u64::try_from(max_x - min_x + 1)
                .ok()
                .and_then(|width| {
                    u64::try_from(max_y - min_y + 1)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .ok_or(CoreError::InvalidArgument("floating work size overflows"))?;
            if work > MAX_FILL_PIXELS {
                return Err(CoreError::InvalidArgument(
                    "floating transform exceeds the bounded work limit",
                ));
            }
            let source_pixels: BTreeMap<_, _> = source
                .pixels
                .iter()
                .map(|pixel| ((pixel.x, pixel.y), pixel.value))
                .collect();
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let translated_x = x as f64 - center_x - floating.transform.translate_x;
                    let translated_y = y as f64 - center_y - floating.transform.translate_y;
                    let source_x = center_x
                        + (translated_x * cos + translated_y * sin) / floating.transform.scale_x;
                    let source_y = center_y
                        + (-translated_x * sin + translated_y * cos) / floating.transform.scale_y;
                    let source_coord = (source_x.round() as i32, source_y.round() as i32);
                    if let Some(value) = source_pixels.get(&source_coord) {
                        staged.insert((x as u32, y as u32), *value);
                    }
                }
            }
        }
        if staged.is_empty() {
            return Err(CoreError::InvalidState(
                "floating selection contains no content inside the destination paper",
            ));
        }
        let revision = self.next_document_revision()?;
        let after_state = self.allocate_state()?;
        let document = self.document.as_mut().ok_or(CoreError::NoDocument)?;
        let plane = document
            .plane_by_id_mut(floating.destination_plane_id)
            .ok_or(CoreError::InvalidState(
                "paste destination no longer exists",
            ))?;
        let mut changes = Vec::with_capacity(staged.len());
        for ((x, y), source_value) in staged {
            let before = plane.raster.pixel(x, y)?;
            let after = paste_value(before, source_value, plane.kind)?;
            if before != after {
                plane.raster.set_pixel(x, y, after, revision.get())?;
                changes.push(PixelChange {
                    x,
                    y,
                    before,
                    after,
                });
            }
        }
        if changes.is_empty() {
            self.floating = None;
            self.assets = retained_assets;
            return Ok(self.noop_outcome());
        }
        self.document_revision = revision;
        self.commit_pixel_history(floating.destination_plane_id, changes, after_state);
        self.floating = None;
        self.assets = retained_assets;
        Ok(DispatchOutcome {
            revision: revision.get(),
            accepted_commands: 1,
        })
    }
}

// Shared implementation helpers for this responsibility.

fn stage_clipboard_assets(
    core: &Core,
    payload: &ClipboardPayload,
) -> Result<(asset::AssetStore, Vec<AssetId>), CoreError> {
    let width = u32::try_from(payload.bounds.width)
        .map_err(|_| CoreError::InvalidArgument("clipboard width is invalid"))?;
    let height = u32::try_from(payload.bounds.height)
        .map_err(|_| CoreError::InvalidArgument("clipboard height is invalid"))?;
    if width == 0 || height == 0 {
        return Err(CoreError::InvalidArgument(
            "clipboard bounds must be nonempty",
        ));
    }
    let right = payload
        .bounds
        .x
        .checked_add(payload.bounds.width)
        .ok_or(CoreError::InvalidArgument("clipboard bounds overflow"))?;
    let bottom = payload
        .bounds
        .y
        .checked_add(payload.bounds.height)
        .ok_or(CoreError::InvalidArgument("clipboard bounds overflow"))?;
    let mut staged = core.assets.clone();
    let mut asset_ids = Vec::new();
    asset_ids
        .try_reserve(payload.planes.len())
        .map_err(|_| CoreError::InvalidState("clipboard asset allocation failed"))?;
    for plane in &payload.planes {
        let mut raster = TileRaster::new(width, height, plane.pixel_format)?;
        let mut coordinates = BTreeSet::new();
        for pixel in &plane.pixels {
            if pixel.x < payload.bounds.x
                || pixel.y < payload.bounds.y
                || pixel.x >= right
                || pixel.y >= bottom
            {
                return Err(CoreError::InvalidArgument(
                    "clipboard pixel lies outside its bounds",
                ));
            }
            let local_x = u32::try_from(pixel.x - payload.bounds.x)
                .map_err(|_| CoreError::InvalidArgument("clipboard X is invalid"))?;
            let local_y = u32::try_from(pixel.y - payload.bounds.y)
                .map_err(|_| CoreError::InvalidArgument("clipboard Y is invalid"))?;
            if !coordinates.insert((local_x, local_y)) {
                return Err(CoreError::InvalidArgument(
                    "clipboard plane contains a duplicate pixel coordinate",
                ));
            }
            raster.set_pixel(local_x, local_y, pixel.value, 1)?;
        }
        let asset = staged.ingest_tile_raster(&raster, None)?;
        asset_ids.push(asset.id());
    }
    let mut roots = core.asset_retention_roots();
    roots.extend(asset_ids.iter().copied());
    staged.garbage_collect(roots)?;
    Ok((staged, asset_ids))
}
