use super::*;
use crate::core::SelectionBoundsCache;
use crate::primitive::CanonicalInvocation;

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
        self.apply_selection_with_options_for_editor_target(
            shape,
            operation,
            RangeInterpretation::Normal,
            SelectionConstructionOptions::default(),
            target,
        )
    }

    /// Combines an option-normalized shape and raster interpretation with the selection.
    ///
    /// Preview callers and commit callers use the same construction and mask path.
    /// A changed mask is one canonical transaction and Undo unit; no-op and every
    /// validation/allocation failure leave revision, history, dirty state, and IDs unchanged.
    pub fn apply_selection_with_options(
        &mut self,
        shape: &SelectionShape,
        operation: SelectionOperation,
        interpretation: RangeInterpretation,
        options: SelectionConstructionOptions,
    ) -> Result<DispatchOutcome, CoreError> {
        let target = self.active_editor_target()?;
        self.apply_selection_with_options_for_editor_target(
            shape,
            operation,
            interpretation,
            options,
            target,
        )
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
        self.apply_selection_with_options_for_editor_target(
            shape,
            operation,
            RangeInterpretation::Normal,
            SelectionConstructionOptions::default(),
            target,
        )
    }

    /// Applies selection construction to the stable target captured at gesture begin.
    pub fn apply_selection_with_options_for_editor_target(
        &mut self,
        shape: &SelectionShape,
        operation: SelectionOperation,
        interpretation: RangeInterpretation,
        options: SelectionConstructionOptions,
        target: EditorTarget,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplySelection {
                    shape: shape.clone(),
                    operation,
                    interpretation,
                    options,
                    target,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let (_, active_plane_id) = self.editor_target_ids(target)?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let candidate = selection_mask_for_shape(
            before,
            active_plane_id,
            shape,
            interpretation,
            options,
            revision.get(),
        )?;
        let combined =
            combine_selection_masks(&before.selection, &candidate, operation, revision.get())?;
        if selection_masks_have_same_coverage(&before.selection, &combined)? {
            drop(edit);
            return Ok(self.noop_outcome());
        }
        after.selection = combined;
        edit.commit(self)
    }

    /// Inverts selection coverage across the whole document as one undoable edit.
    pub fn invert_selection(&mut self) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::InvertSelection)
                .map(|result| result.dispatch);
        }
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
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ClearSelection)
                .map(|result| result.dispatch);
        }
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
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ResizeSelection { pixels })
                .map(|result| result.dispatch);
        }
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
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::SelectColor {
                    color,
                    tolerance,
                    different,
                    operation,
                    target,
                })
                .map(|result| result.dispatch);
        }
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
        let combined =
            combine_selection_masks(&before.selection, &candidate, operation, revision.get())?;
        if selection_masks_have_same_coverage(&before.selection, &combined)? {
            drop(edit);
            return Ok(self.noop_outcome());
        }
        after.selection = combined;
        edit.commit(self)
    }

    /// Returns the smallest half-open document rectangle containing selection coverage.
    ///
    /// Returns `None` for an empty selection without mutating Core state.
    pub fn selection_bounds(&self) -> Result<Option<RectI32>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        self.selection_bounds_in_document(document)
    }

    pub(crate) fn selection_bounds_in_document(
        &self,
        document: &CellDocument,
    ) -> Result<Option<RectI32>, CoreError> {
        let key_matches = self
            .selection_bounds_cache
            .borrow()
            .as_ref()
            .is_some_and(|cache| {
                cache.document_uuid == document.uuid
                    && cache.document_id == document.id
                    && cache.selection_plane_id == document.selection_plane_id
                    && cache.document_revision == self.document_revision
            });
        if key_matches {
            return Ok(self
                .selection_bounds_cache
                .borrow()
                .as_ref()
                .and_then(|cache| cache.bounds));
        }
        let bounds = mask_bounds(&document.selection)?;
        self.selection_bounds_cache
            .replace(Some(SelectionBoundsCache {
                document_uuid: document.uuid,
                document_id: document.id,
                selection_plane_id: document.selection_plane_id,
                document_revision: self.document_revision,
                bounds,
            }));
        Ok(bounds)
    }

    /// Saves a copy of the current selection as a named document-owned mask.
    ///
    /// Success is one undoable edit and returns a stable ID. An empty current
    /// selection, invalid name, or the bounded collection limit fails atomically.
    pub fn save_selection_mask(
        &mut self,
        name: &str,
    ) -> Result<(DispatchOutcome, SavedSelectionId), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result =
                self.execute_canonical_invocation(CanonicalInvocation::SaveSelectionMask {
                    name: name.to_owned(),
                })?;
            let id = SavedSelectionId::from_raw(*result.output_ids.first().ok_or(
                CoreError::InvalidState(
                    "save-selection-mask primitive did not return its output ID",
                ),
            )?)
            .ok_or(CoreError::InvalidState(
                "save-selection-mask primitive returned an invalid output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let selection = {
            let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
            if document.saved_selection_masks.len() >= MAX_SAVED_SELECTION_MASKS {
                return Err(CoreError::InvalidState(
                    "saved-selection mask limit reached",
                ));
            }
            if mask_bounds(&document.selection)?.is_none() {
                return Err(CoreError::InvalidState("selection is empty"));
            }
            document.selection.clone()
        };
        let mut next_id = self.next_id;
        let saved_selection_id = next_id.take_saved_selection();
        let mut edit = self.begin_document_edit()?;
        {
            let after = edit.working_mut();
            after.saved_selection_masks.push(SavedSelectionMask {
                id: saved_selection_id,
                name: unique_saved_selection_name(&after.saved_selection_masks, name),
                raster: selection,
            });
        }
        let outcome = edit.commit(self)?;
        self.next_id = next_id;
        Ok((outcome, saved_selection_id))
    }

    /// Combines a saved mask with the current selection.
    ///
    /// The saved source remains unchanged. Success is one undoable edit and an
    /// unchanged result is a no-op.
    pub fn apply_saved_selection_mask(
        &mut self,
        saved_selection_id: SavedSelectionId,
        operation: SavedSelectionOperation,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ApplySavedSelectionMask {
                    saved_selection_id,
                    operation,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision();
        let (before, after) = edit.documents();
        let mask = before
            .saved_selection_masks
            .iter()
            .find(|mask| mask.id == saved_selection_id)
            .ok_or(CoreError::InvalidArgument(
                "saved-selection ID does not exist",
            ))?;
        let selection_operation = match operation {
            SavedSelectionOperation::Replace => SelectionOperation::New,
            SavedSelectionOperation::Add => SelectionOperation::Add,
            SavedSelectionOperation::Subtract => SelectionOperation::Subtract,
        };
        let combined = combine_selection_masks(
            &before.selection,
            &mask.raster,
            selection_operation,
            revision.get(),
        )?;
        if selection_masks_have_same_coverage(&before.selection, &combined)? {
            drop(edit);
            return Ok(self.noop_outcome());
        }
        after.selection = combined;
        edit.commit(self)
    }

    /// Renames a saved selection mask as one undoable edit.
    ///
    /// An identical name is a no-op. A colliding valid name receives the same
    /// deterministic numeric suffix policy used by other document nodes.
    pub fn rename_saved_selection_mask(
        &mut self,
        saved_selection_id: SavedSelectionId,
        name: &str,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::RenameSavedSelectionMask {
                    saved_selection_id,
                    name: name.to_owned(),
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        validate_node_name(name)?;
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        let index = before
            .saved_selection_masks
            .iter()
            .position(|mask| mask.id == saved_selection_id)
            .ok_or(CoreError::InvalidArgument(
                "saved-selection ID does not exist",
            ))?;
        if before.saved_selection_masks[index].name == name {
            drop(edit);
            return Ok(self.noop_outcome());
        }
        after.saved_selection_masks[index].name =
            unique_saved_selection_name(&before.saved_selection_masks, name);
        edit.commit(self)
    }

    /// Deletes a saved selection mask as one undoable edit.
    pub fn delete_saved_selection_mask(
        &mut self,
        saved_selection_id: SavedSelectionId,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::DeleteSavedSelectionMask {
                    saved_selection_id,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        let index = after
            .saved_selection_masks
            .iter()
            .position(|mask| mask.id == saved_selection_id)
            .ok_or(CoreError::InvalidArgument(
                "saved-selection ID does not exist",
            ))?;
        after.saved_selection_masks.remove(index);
        edit.commit(self)
    }

    /// Returns document-owned saved masks in persistent order.
    ///
    /// The query returns owned metadata and does not change revision, history,
    /// dirty state, or the current selection.
    pub fn saved_selection_masks(&self) -> Result<Vec<SavedSelectionInfo>, CoreError> {
        Ok(self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .saved_selection_masks
            .iter()
            .map(|mask| SavedSelectionInfo {
                id: mask.id,
                name: mask.name.clone(),
            })
            .collect())
    }

    /// Copies selected pixels from the active raster plane into an owned payload.
    ///
    /// Coordinates and half-open bounds remain in document space. The query does
    /// not change revision, history, selection, or dirty state.
    pub fn copy_selection(&self) -> Result<ClipboardPayload, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let bounds = mask_bounds(&document.selection)?
            .ok_or(CoreError::InvalidState("selection is empty"))?;
        let targets = self.effective_edit_targets()?;
        let mut selected_planes = BTreeSet::new();
        for target in targets {
            match target {
                EditTarget::Layer(layer_id) => {
                    let layer = document
                        .layers
                        .iter()
                        .find(|layer| layer.id.get() == layer_id)
                        .ok_or(CoreError::InvalidArgument(
                            "clipboard layer target is missing",
                        ))?;
                    selected_planes.extend(layer.planes.iter().map(|plane| plane.id));
                }
                EditTarget::Plane(target) => {
                    selected_planes.insert(PlaneId::from_raw(target.plane_id));
                }
            }
        }
        let mut planes = Vec::new();
        for plane in document.layers.iter().flat_map(|layer| &layer.planes) {
            if !selected_planes.contains(&plane.id) {
                continue;
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
            planes.push(ClipboardPlane {
                kind: plane.kind,
                pixel_format: plane.raster.format(),
                origin_x: bounds.x,
                origin_y: bounds.y,
                pixels,
            });
        }
        if planes.is_empty() {
            return Err(CoreError::InvalidState(
                "edit targets contain no copyable planes",
            ));
        }
        Ok(ClipboardPayload {
            source_document_uuid: document.uuid,
            bounds,
            planes,
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
        let active_layer = document.layers.iter().find(|layer| {
            layer
                .planes
                .iter()
                .any(|plane| plane.id == active_destination.id)
        });
        let mut candidates = vec![active_destination];
        if let Some(layer) = active_layer {
            candidates.extend(
                layer
                    .planes
                    .iter()
                    .filter(|plane| plane.id != active_destination.id),
            );
        }
        let mut candidate_ids = candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<BTreeSet<_>>();
        for plane in document.layers.iter().flat_map(|layer| &layer.planes) {
            if candidate_ids.insert(plane.id) {
                candidates.push(plane);
            }
        }
        let mut used = BTreeSet::new();
        let mut destinations = Vec::with_capacity(payload.planes.len());
        for source in &payload.planes {
            if source.pixels.len() as u64 > MAX_FILL_PIXELS
                || source.pixels.iter().any(|pixel| {
                    pixel.x < payload.bounds.x
                        || pixel.y < payload.bounds.y
                        || pixel.x >= payload.bounds.x + payload.bounds.width
                        || pixel.y >= payload.bounds.y + payload.bounds.height
                })
            {
                return Err(CoreError::InvalidArgument(
                    "clipboard payload exceeds bounds or work limit",
                ));
            }
            let destination = candidates
                .iter()
                .find(|destination| {
                    !used.contains(&destination.id)
                        && source.kind == destination.kind
                        && source.pixel_format == destination.raster.format()
                })
                .ok_or(CoreError::InvalidArgument(
                    "clipboard plane has no compatible typed destination",
                ))?;
            ensure_editable_plane(document, destination.id)?;
            used.insert(destination.id);
            destinations.push(destination.id);
        }
        let (staged_assets, asset_ids) = stage_clipboard_assets(self, payload)?;
        self.floating = Some(FloatingSelection {
            payload: payload.clone(),
            destination: FloatingDestination::ExistingPlanes(destinations),
            transform: floating_identity_transform(payload.bounds)?,
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
        let transform = floating_identity_transform(converted.bounds)?;
        self.floating = Some(FloatingSelection {
            payload: converted,
            destination: FloatingDestination::ExistingPlanes(vec![destination.id]),
            transform,
            asset_ids,
        });
        self.assets = staged_assets;
        Ok(())
    }

    /// Starts a converted floating paste whose destination plane is created at commit.
    ///
    /// The target layer and typed plane definition are validated now, but no stable
    /// ID, topology, revision, or history entry is published until
    /// [`Core::commit_floating`]. Cancel therefore leaves no empty plane behind.
    pub fn begin_paste_to_new_plane_converted(
        &mut self,
        payload: &ClipboardPayload,
        layer_id: u64,
        kind: PlaneType,
        format: PixelFormat,
        name: &str,
        opacity_milli: u32,
    ) -> Result<(), CoreError> {
        self.ensure_no_active_stroke()?;
        if self.floating.is_some() {
            return Err(CoreError::InvalidState("floating paste is already active"));
        }
        validate_node_name(name)?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument(
                "paste destination opacity exceeds 1000 milli",
            ));
        }
        if kind != PlaneType::Raster {
            return Err(CoreError::InvalidArgument(
                "a new floating-paste plane must be a raster plane",
            ));
        }
        self.validate_plane_creation(layer_id, format)?;
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
        converted.planes[0].kind = kind;
        converted.planes[0].pixel_format = format;
        for pixel in &mut converted.planes[0].pixels {
            pixel.value = convert_plane_pixel(pixel.value, format)?;
        }
        let (staged_assets, asset_ids) = stage_clipboard_assets(self, &converted)?;
        let transform = floating_identity_transform(converted.bounds)?;
        self.floating = Some(FloatingSelection {
            payload: converted,
            destination: FloatingDestination::NewPlane {
                layer_id: LayerId::from_raw(layer_id),
                kind,
                format,
                name: name.to_owned(),
                opacity_milli,
            },
            transform,
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
        let target = self.active_editor_target()?;
        self.execute_canonical_invocation(CanonicalInvocation::ClearSelectedContent { target })
            .map(|result| result.dispatch)
    }

    pub(crate) fn clear_selected_content_for_editor_target(
        &mut self,
        target: EditorTarget,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let (_, plane_id) = self.editor_target_ids(target)?;
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
        if !self.canonical_invocation_is_active() {
            let floating = self
                .floating
                .clone()
                .ok_or(CoreError::InvalidState("there is no floating paste"))?;
            return self
                .execute_canonical_invocation(CanonicalInvocation::CommitFloating { floating })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let floating = self
            .floating
            .clone()
            .ok_or(CoreError::InvalidState("there is no floating paste"))?;
        let retained_assets = self.asset_store_without_floating()?;
        if let FloatingDestination::NewPlane {
            layer_id,
            kind,
            format,
            ..
        } = &floating.destination
        {
            if *kind != PlaneType::Raster {
                return Err(CoreError::InvalidArgument(
                    "a new floating-paste plane must be a raster plane",
                ));
            }
            self.validate_plane_creation(layer_id.get(), *format)?;
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let source_planes = match &floating.destination {
            FloatingDestination::ExistingPlanes(plane_ids) => {
                if plane_ids.len() != floating.payload.planes.len() || plane_ids.is_empty() {
                    return Err(CoreError::InvalidArgument(
                        "floating source/destination plane counts do not match",
                    ));
                }
                let mut sources = Vec::with_capacity(plane_ids.len());
                for (plane_id, source) in plane_ids.iter().zip(&floating.payload.planes) {
                    let destination =
                        document
                            .plane_by_id(*plane_id)
                            .ok_or(CoreError::InvalidState(
                                "paste destination no longer exists",
                            ))?;
                    ensure_editable_plane(document, *plane_id)?;
                    if source.kind != destination.kind
                        || source.pixel_format != destination.raster.format()
                    {
                        return Err(CoreError::InvalidArgument(
                            "clipboard plane no longer matches its typed destination",
                        ));
                    }
                    sources.push(source);
                }
                sources
            }
            FloatingDestination::NewPlane { kind, format, .. } => vec![
                floating
                    .payload
                    .planes
                    .iter()
                    .find(|plane| plane.kind == *kind && plane.pixel_format == *format)
                    .ok_or(CoreError::InvalidArgument(
                        "compatible clipboard plane is missing",
                    ))?,
            ],
        };
        let mut staged_planes = Vec::with_capacity(source_planes.len());
        use inkpod_image::{
            CANONICAL_DOCUMENT_ONE, canonical_q16_from_f64, canonical_turns_from_degrees_f64,
            ceil_div_i128, div_round_ties_even_i128, floor_div_i128, rotate_q16,
        };
        let one = CANONICAL_DOCUMENT_ONE;
        let (source_anchor_x, source_anchor_y) =
            floating_anchor_q16(floating.payload.bounds, floating.transform.anchor)?;
        let target_x = canonical_q16_from_f64(floating.transform.target_x).ok_or(
            CoreError::InvalidArgument("floating target X is outside canonical Q16"),
        )?;
        let target_y = canonical_q16_from_f64(floating.transform.target_y).ok_or(
            CoreError::InvalidArgument("floating target Y is outside canonical Q16"),
        )?;
        let scale_x = canonical_q16_from_f64(floating.transform.scale_x)
            .filter(|value| *value > 0)
            .ok_or(CoreError::InvalidArgument(
                "floating scale is outside canonical Q16",
            ))?;
        let scale_y = canonical_q16_from_f64(floating.transform.scale_y)
            .filter(|value| *value > 0)
            .ok_or(CoreError::InvalidArgument(
                "floating scale is outside canonical Q16",
            ))?;
        let turns = canonical_turns_from_degrees_f64(floating.transform.rotation_degrees).ok_or(
            CoreError::InvalidArgument("floating angle is outside canonical turns"),
        )?;
        let transform_point = |x: i64, y: i64| -> Result<(i64, i64), CoreError> {
            let local_x = div_round_ties_even_i128(
                i128::from(x - source_anchor_x) * i128::from(scale_x),
                i128::from(one),
            )
            .and_then(|value| value.try_into().ok())
            .ok_or(CoreError::InvalidArgument("floating scale overflowed"))?;
            let local_y = div_round_ties_even_i128(
                i128::from(y - source_anchor_y) * i128::from(scale_y),
                i128::from(one),
            )
            .and_then(|value| value.try_into().ok())
            .ok_or(CoreError::InvalidArgument("floating scale overflowed"))?;
            let (rotated_x, rotated_y) = rotate_q16(local_x, local_y, turns)
                .ok_or(CoreError::InvalidArgument("floating rotation overflowed"))?;
            Ok((
                target_x
                    .checked_add(rotated_x)
                    .ok_or(CoreError::InvalidArgument("floating transform overflowed"))?,
                target_y
                    .checked_add(rotated_y)
                    .ok_or(CoreError::InvalidArgument("floating transform overflowed"))?,
            ))
        };
        let left = i64::from(floating.payload.bounds.x)
            .checked_mul(one)
            .ok_or(CoreError::InvalidArgument("floating left edge overflowed"))?;
        let top = i64::from(floating.payload.bounds.y)
            .checked_mul(one)
            .ok_or(CoreError::InvalidArgument("floating top edge overflowed"))?;
        let right = i64::from(floating.payload.bounds.x)
            .checked_add(i64::from(floating.payload.bounds.width))
            .and_then(|value| value.checked_mul(one))
            .ok_or(CoreError::InvalidArgument("floating right edge overflowed"))?;
        let bottom = i64::from(floating.payload.bounds.y)
            .checked_add(i64::from(floating.payload.bounds.height))
            .and_then(|value| value.checked_mul(one))
            .ok_or(CoreError::InvalidArgument(
                "floating bottom edge overflowed",
            ))?;
        let corners = [
            transform_point(left, top)?,
            transform_point(right, top)?,
            transform_point(left, bottom)?,
            transform_point(right, bottom)?,
        ];
        let min_x_q16 = corners.iter().map(|corner| corner.0).min().unwrap();
        let max_x_q16 = corners.iter().map(|corner| corner.0).max().unwrap();
        let min_y_q16 = corners.iter().map(|corner| corner.1).min().unwrap();
        let max_y_q16 = corners.iter().map(|corner| corner.1).max().unwrap();
        let min_x = floor_div_i128(i128::from(min_x_q16), i128::from(one))
            .unwrap()
            .max(0) as i64;
        let max_x_exclusive = ceil_div_i128(i128::from(max_x_q16), i128::from(one))
            .unwrap()
            .min(i128::from(document.width)) as i64;
        let min_y = floor_div_i128(i128::from(min_y_q16), i128::from(one))
            .unwrap()
            .max(0) as i64;
        let max_y_exclusive = ceil_div_i128(i128::from(max_y_q16), i128::from(one))
            .unwrap()
            .min(i128::from(document.height)) as i64;
        let mut contains_content = false;
        for source in source_planes {
            let mut staged = BTreeMap::new();
            if min_x < max_x_exclusive && min_y < max_y_exclusive {
                let work = u64::try_from(max_x_exclusive - min_x)
                    .ok()
                    .and_then(|width| {
                        u64::try_from(max_y_exclusive - min_y)
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
                for y in min_y..max_y_exclusive {
                    for x in min_x..max_x_exclusive {
                        let destination_x = x
                            .checked_mul(one)
                            .and_then(|value| value.checked_add(one / 2))
                            .ok_or(CoreError::InvalidArgument(
                                "floating destination X overflowed",
                            ))?;
                        let destination_y = y
                            .checked_mul(one)
                            .and_then(|value| value.checked_add(one / 2))
                            .ok_or(CoreError::InvalidArgument(
                                "floating destination Y overflowed",
                            ))?;
                        let translated_x = destination_x - target_x;
                        let translated_y = destination_y - target_y;
                        let (unrotated_x, unrotated_y) =
                            rotate_q16(translated_x, translated_y, turns.wrapping_neg()).ok_or(
                                CoreError::InvalidArgument("floating inverse rotation overflowed"),
                            )?;
                        let source_x = i128::from(source_anchor_x)
                            + div_round_ties_even_i128(
                                i128::from(unrotated_x) * i128::from(one),
                                i128::from(scale_x),
                            )
                            .ok_or(CoreError::InvalidArgument(
                                "floating inverse scale overflowed",
                            ))?;
                        let source_y = i128::from(source_anchor_y)
                            + div_round_ties_even_i128(
                                i128::from(unrotated_y) * i128::from(one),
                                i128::from(scale_y),
                            )
                            .ok_or(CoreError::InvalidArgument(
                                "floating inverse scale overflowed",
                            ))?;
                        if source_x < i128::from(left)
                            || source_x >= i128::from(right)
                            || source_y < i128::from(top)
                            || source_y >= i128::from(bottom)
                        {
                            continue;
                        }
                        let source_coord = (
                            floor_div_i128(source_x, i128::from(one))
                                .and_then(|value| i32::try_from(value).ok())
                                .ok_or(CoreError::InvalidArgument(
                                    "floating source X overflowed",
                                ))?,
                            floor_div_i128(source_y, i128::from(one))
                                .and_then(|value| i32::try_from(value).ok())
                                .ok_or(CoreError::InvalidArgument(
                                    "floating source Y overflowed",
                                ))?,
                        );
                        if let Some(value) = source_pixels.get(&source_coord) {
                            staged.insert((x as u32, y as u32), *value);
                        }
                    }
                }
            }
            contains_content |= !staged.is_empty();
            staged_planes.push(staged);
        }
        if !contains_content {
            return Err(CoreError::InvalidState(
                "floating selection contains no content inside the destination paper",
            ));
        }
        let payload_planes = floating.payload.planes;
        match floating.destination {
            FloatingDestination::ExistingPlanes(plane_ids) => {
                let mut edit = self.begin_document_edit()?;
                let revision = edit.revision().get();
                let after = edit.working_mut();
                for ((plane_id, staged), _source) in plane_ids
                    .iter()
                    .copied()
                    .zip(staged_planes)
                    .zip(&payload_planes)
                {
                    let plane = after
                        .plane_by_id_mut(plane_id)
                        .ok_or(CoreError::InvalidState(
                            "paste destination no longer exists",
                        ))?;
                    for ((x, y), source_value) in staged {
                        let before = plane.raster.pixel(x, y)?;
                        let value = paste_value(before, source_value, plane.kind)?;
                        if before != value {
                            plane.raster.set_pixel(x, y, value, revision)?;
                        }
                    }
                }
                let outcome = edit.commit(self)?;
                self.floating = None;
                self.assets = retained_assets;
                Ok(outcome)
            }
            FloatingDestination::NewPlane {
                layer_id,
                kind,
                format,
                name,
                opacity_milli,
            } => {
                let mut next_id = self.next_id;
                let plane_id = next_id.take_plane();
                let mut edit = self.begin_document_edit()?;
                let revision = edit.revision();
                let after = edit.working_mut();
                let layer = after
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == layer_id)
                    .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
                let mut raster = TileRaster::new(after.width, after.height, format)?;
                let zero = zero_pixel(format)?;
                let staged = staged_planes
                    .into_iter()
                    .next()
                    .ok_or(CoreError::InvalidState("floating plane staging is missing"))?;
                for ((x, y), source_value) in staged {
                    let value = paste_value(zero, source_value, kind)?;
                    if value != zero {
                        raster.set_pixel(x, y, value, revision.get())?;
                    }
                }
                layer.planes.push(PlaneNode {
                    id: plane_id,
                    kind,
                    name,
                    visible: true,
                    editable: true,
                    opacity_milli,
                    raster,
                });
                validate_layer(&layer.planes)?;
                edit.prefer_editor_target(EditorTarget {
                    layer_id: layer_id.get(),
                    plane_id: plane_id.get(),
                });
                let outcome = edit.commit(self)?;
                self.next_id = next_id;
                self.floating = None;
                self.assets = retained_assets;
                Ok(outcome)
            }
        }
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

#[cfg(test)]
mod saved_selection_tests {
    use super::*;

    #[test]
    fn saved_masks_are_document_owned_stable_and_undoable() {
        let mut core = Core::new();
        core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }),
            SelectionOperation::New,
        )
        .unwrap();

        let (_, id) = core.save_selection_mask("Mask").unwrap();
        assert_ne!(id.get(), 0);
        let saved = vec![SavedSelectionInfo {
            id,
            name: "Mask".to_owned(),
        }];
        assert_eq!(core.saved_selection_masks().unwrap(), saved);
        core.undo().unwrap();
        assert!(core.saved_selection_masks().unwrap().is_empty());
        core.redo().unwrap();
        assert_eq!(core.saved_selection_masks().unwrap(), saved);

        core.clear_selection().unwrap();
        assert_eq!(core.selection_bounds().unwrap(), None);
        core.apply_saved_selection_mask(id, SavedSelectionOperation::Replace)
            .unwrap();
        assert_eq!(
            core.selection_bounds().unwrap(),
            Some(RectI32 {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            })
        );

        core.rename_saved_selection_mask(id, "Renamed").unwrap();
        assert_eq!(core.saved_selection_masks().unwrap()[0].name, "Renamed");
        core.undo().unwrap();
        assert_eq!(core.saved_selection_masks().unwrap()[0].name, "Mask");
        core.redo().unwrap();
        assert_eq!(core.saved_selection_masks().unwrap()[0].name, "Renamed");
        core.delete_saved_selection_mask(id).unwrap();
        assert!(core.saved_selection_masks().unwrap().is_empty());
        core.undo().unwrap();
        assert_eq!(core.saved_selection_masks().unwrap()[0].id, id);
        core.redo().unwrap();
        assert!(core.saved_selection_masks().unwrap().is_empty());
    }

    #[test]
    fn saved_masks_and_fill_protection_follow_document_geometry() {
        let mut core = Core::new();
        core.new_cell(3, 2, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        core.apply_selection(
            &SelectionShape::Rectangle(RectI32 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            SelectionOperation::New,
        )
        .unwrap();
        let (_, id) = core.save_selection_mask("Geometry").unwrap();
        let revision = core.document_revision.get();
        core.document
            .as_mut()
            .unwrap()
            .fill_protection
            .set_pixel(0, 1, PixelValue::Binary(u8::MAX), revision)
            .unwrap();

        core.mirror_document(MirrorAxis::Horizontal).unwrap();
        core.clear_selection().unwrap();
        core.apply_saved_selection_mask(id, SavedSelectionOperation::Replace)
            .unwrap();
        assert_eq!(
            core.selection_bounds().unwrap(),
            Some(RectI32 {
                x: 2,
                y: 0,
                width: 1,
                height: 1,
            })
        );
        assert_eq!(
            core.document.as_ref().unwrap().fill_protection.pixel(2, 1),
            Ok(PixelValue::Binary(u8::MAX))
        );

        core.rotate_document(RotateDirection::Right90).unwrap();
        core.clear_selection().unwrap();
        core.apply_saved_selection_mask(id, SavedSelectionOperation::Replace)
            .unwrap();
        assert_eq!(
            core.selection_bounds().unwrap(),
            Some(RectI32 {
                x: 1,
                y: 2,
                width: 1,
                height: 1,
            })
        );
        assert_eq!(
            core.document.as_ref().unwrap().fill_protection.pixel(0, 2),
            Ok(PixelValue::Binary(u8::MAX))
        );

        core.resize_document(DocumentResize {
            width: 4,
            height: 6,
            dpi_x_milli: DEFAULT_DPI_MILLI,
            dpi_y_milli: DEFAULT_DPI_MILLI,
            resample: true,
            anchor: ResizeAnchor::TopLeft,
        })
        .unwrap();
        core.clear_selection().unwrap();
        core.apply_saved_selection_mask(id, SavedSelectionOperation::Replace)
            .unwrap();
        assert_eq!(
            core.selection_bounds().unwrap(),
            Some(RectI32 {
                x: 2,
                y: 4,
                width: 2,
                height: 2,
            })
        );
        assert_eq!(
            core.fill_protection_mask_info().unwrap().wall_pixel_count,
            4
        );
    }

    #[test]
    fn saved_mask_id_name_and_raster_round_trip_current_native_format() {
        let mut core = Core::new();
        core.new_cell(8, 8, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let bounds = RectI32 {
            x: 2,
            y: 3,
            width: 4,
            height: 2,
        };
        core.apply_selection(&SelectionShape::Rectangle(bounds), SelectionOperation::New)
            .unwrap();
        let (_, id) = core.save_selection_mask("Persistent mask").unwrap();
        let expected = core.saved_selection_masks().unwrap();
        let path = std::env::temp_dir().join(format!(
            "inkpod-saved-selection-{}.inkpod",
            std::process::id()
        ));
        core.save(&path).unwrap();

        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(reopened.saved_selection_masks().unwrap(), expected);
        reopened.clear_selection().unwrap();
        reopened
            .apply_saved_selection_mask(id, SavedSelectionOperation::Replace)
            .unwrap();
        assert_eq!(reopened.selection_bounds().unwrap(), Some(bounds));

        std::fs::remove_file(path).unwrap();
    }
}
