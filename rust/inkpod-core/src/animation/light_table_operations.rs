use super::light_table::*;
use super::raster::*;
use super::*;
use crate::primitive::CanonicalInvocation;

impl Core {
    /// Sets active light-table set opacity in `0..=1000` as one undoable edit.
    pub fn light_table_set_global_opacity(
        &mut self,
        opacity_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableSetGlobalOpacity {
                    opacity_milli,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument(
                "light-table opacity exceeds one thousand",
            ));
        }
        let mut edit = self.begin_document_edit()?;
        edit.working_mut()
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .global_opacity_milli = opacity_milli;
        edit.commit(self)
    }

    /// Creates and activates an empty light-table set.
    ///
    /// Success is one undoable edit and returns a new stable set ID.
    pub fn light_table_create_set(
        &mut self,
        name: impl Into<String>,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        let name = name.into();
        if !self.canonical_invocation_is_active() {
            let result = self
                .execute_canonical_invocation(CanonicalInvocation::LightTableCreateSet { name })?;
            let id = *result.output_ids.first().ok_or(CoreError::InvalidState(
                "light-table-create-set primitive did not return its output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        validate_node_name(&name)?;
        if self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table
            .sets
            .len()
            >= MAX_LIGHT_TABLE_SETS
        {
            return Err(CoreError::InvalidState(
                "light-table set count exceeds its bound",
            ));
        }
        let id = self.allocate_light_table_set_id();
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        let name = unique_light_table_set_name(&after.light_table.sets, &name);
        after.light_table.sets.push(LightTableSet {
            id,
            name,
            global_opacity_milli: 1_000,
            items: Vec::new(),
        });
        after.light_table.active_set_id = id;
        Ok((edit.commit(self)?, id.get()))
    }

    /// Duplicates a set and all items using fresh stable IDs.
    ///
    /// Success is one undoable edit; bounds or invalid ID failure is atomic.
    pub fn light_table_duplicate_set(
        &mut self,
        set_id: u64,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result =
                self.execute_canonical_invocation(CanonicalInvocation::LightTableDuplicateSet {
                    set_id,
                })?;
            let id = *result.output_ids.first().ok_or(CoreError::InvalidState(
                "light-table-duplicate-set primitive did not return its output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        let set_id = LightTableSetId::from_raw(set_id);
        let source = {
            let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
            let source = document
                .light_table
                .sets
                .iter()
                .find(|set| set.id == set_id)
                .cloned()
                .ok_or(CoreError::InvalidArgument(
                    "light-table set ID does not exist",
                ))?;
            if document.light_table.sets.len() >= MAX_LIGHT_TABLE_SETS
                || document
                    .light_table
                    .item_count()
                    .checked_add(source.items.len())
                    .is_none_or(|count| count > MAX_LIGHT_TABLE_ITEMS)
            {
                return Err(CoreError::InvalidState(
                    "duplicated light-table content exceeds its bound",
                ));
            }
            source
        };
        let new_set_id = self.allocate_light_table_set_id();
        let mut items = Vec::with_capacity(source.items.len());
        for mut item in source.items {
            item.id = self.allocate_light_table_item_id();
            item.source_plane_id = self.allocate_plane_id();
            items.push(item);
        }
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        let name = unique_light_table_set_name(&after.light_table.sets, &source.name);
        after.light_table.sets.push(LightTableSet {
            id: new_set_id,
            name,
            global_opacity_milli: source.global_opacity_milli,
            items,
        });
        after.light_table.active_set_id = new_set_id;
        Ok((edit.commit(self)?, new_set_id.get()))
    }

    /// Deletes a light-table set as one undoable edit.
    ///
    /// The final set cannot be deleted; active-set selection is repaired on success.
    pub fn light_table_delete_set(&mut self, set_id: u64) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableDeleteSet { set_id })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let set_id = LightTableSetId::from_raw(set_id);
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        if after.light_table.sets.len() == 1 {
            return Err(CoreError::InvalidState(
                "the final light-table set cannot be deleted",
            ));
        }
        let index = after
            .light_table
            .sets
            .iter()
            .position(|set| set.id == set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?;
        after.light_table.sets.remove(index);
        if after.light_table.active_set_id == set_id {
            after.light_table.active_set_id = after.light_table.sets
                [index.min(after.light_table.sets.len().saturating_sub(1))]
            .id;
        }
        edit.commit(self)
    }

    /// Renames a set with a validated unique display name as one undoable edit.
    pub fn light_table_rename_set(
        &mut self,
        set_id: u64,
        name: impl Into<String>,
    ) -> Result<DispatchOutcome, CoreError> {
        let name = name.into();
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableRenameSet {
                    set_id,
                    name,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let set_id = LightTableSetId::from_raw(set_id);
        validate_node_name(&name)?;
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        let other_sets = after
            .light_table
            .sets
            .iter()
            .filter(|set| set.id != set_id)
            .cloned()
            .collect::<Vec<_>>();
        let unique = unique_light_table_set_name(&other_sets, &name);
        after
            .light_table
            .sets
            .iter_mut()
            .find(|set| set.id == set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?
            .name = unique;
        edit.commit(self)
    }

    /// Moves a set to a zero-based stacking index as one undoable edit.
    ///
    /// Moving to the current index is a semantic no-op through the transaction layer.
    pub fn light_table_reorder_set(
        &mut self,
        set_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            let destination_index = u64::try_from(destination_index).map_err(|_| {
                CoreError::InvalidArgument("light-table set destination is not representable")
            })?;
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableReorderSet {
                    set_id,
                    destination_index,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let set_id = LightTableSetId::from_raw(set_id);
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        if destination_index >= after.light_table.sets.len() {
            return Err(CoreError::InvalidArgument(
                "light-table set destination is outside bounds",
            ));
        }
        let source_index = after
            .light_table
            .sets
            .iter()
            .position(|set| set.id == set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?;
        let set = after.light_table.sets.remove(source_index);
        after.light_table.sets.insert(destination_index, set);
        edit.commit(self)
    }

    /// Activates an existing set as one undoable document metadata edit.
    pub fn light_table_set_active(&mut self, set_id: u64) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableSetActive { set_id })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let set_id = LightTableSetId::from_raw(set_id);
        let mut edit = self.begin_document_edit()?;
        let after = edit.working_mut();
        if !after.light_table.sets.iter().any(|set| set.id == set_id) {
            return Err(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ));
        }
        after.light_table.active_set_id = set_id;
        edit.commit(self)
    }

    /// Inserts an item at the top of the active set.
    ///
    /// Success is one undoable edit and returns a new stable item ID.
    pub fn light_table_add_item(
        &mut self,
        mut input: LightTableItemInput,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result = self
                .execute_canonical_invocation(CanonicalInvocation::LightTableAddItem { input })?;
            let id = *result.output_ids.first().ok_or(CoreError::InvalidState(
                "light-table-add-item primitive did not return its output ID",
            ))?;
            return Ok((result.dispatch, id));
        }
        self.ensure_no_active_stroke()?;
        validate_item_input(&input)?;
        if self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table
            .item_count()
            >= MAX_LIGHT_TABLE_ITEMS
        {
            return Err(CoreError::InvalidState(
                "light-table item count exceeds its bound",
            ));
        }
        let mut staged_assets = self.assets.clone();
        input.source.intern_into(&mut staged_assets)?;
        let base_revision = self.document_revision;
        let mut next_id = self.next_id;
        let item_id = next_id.take_light_table_item();
        let source_plane_id = next_id.take_plane();
        let mut edit = self.begin_document_edit()?;
        edit.working_mut()
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .items
            .insert(
                0,
                LightTableItem {
                    id: item_id,
                    source_plane_id,
                    name: input.name,
                    source: input.source,
                    visible: input.visible,
                    opacity_milli: input.opacity_milli,
                    display_mode: input.display_mode,
                    display_color: input.display_color,
                    translate_x_milli: input.translate_x_milli,
                    translate_y_milli: input.translate_y_milli,
                    scale_x_milli: input.scale_x_milli,
                    scale_y_milli: input.scale_y_milli,
                    rotation_milli_degrees: input.rotation_milli_degrees,
                },
            );
        staged_assets =
            self.prepare_asset_store_for_document_edit(staged_assets, edit.working_mut())?;
        let outcome = edit.commit(self)?;
        if outcome.revision() != base_revision.get() {
            self.next_id = next_id;
            self.assets = staged_assets;
        }
        Ok((outcome, item_id.get()))
    }

    /// Decodes a common raster and inserts it as a default light-table item.
    ///
    /// Decode and source validation complete before the atomic document edit.
    pub fn light_table_add_common_raster(
        &mut self,
        format: CommonRasterFormat,
        bytes: &[u8],
        name: impl Into<String>,
        document_uuid: u128,
        source_revision: u64,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        let raster = decode_common_raster(format, bytes)?;
        let reference_frame = RectI32 {
            x: 0,
            y: 0,
            width: i32::try_from(raster.info.width)
                .map_err(|_| CoreError::InvalidArgument("reference width exceeds i32"))?,
            height: i32::try_from(raster.info.height)
                .map_err(|_| CoreError::InvalidArgument("reference height exceeds i32"))?,
        };
        let source = LightTableSource::from_common_raster(
            document_uuid,
            source_revision,
            reference_frame,
            &raster,
        )?;
        self.light_table_add_item(LightTableItemInput::new(name, source))
    }

    /// Replaces display/transform properties of an active-set item.
    ///
    /// Values validate before one undoable commit; identical properties are a no-op.
    pub fn light_table_update_item_properties(
        &mut self,
        item_id: u64,
        properties: LightTableItemProperties,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableUpdateItemProperties {
                    item_id,
                    properties,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let item_id = LightTableItemId::from_raw(item_id);
        let mut edit = self.begin_document_edit()?;
        let item = edit
            .working_mut()
            .light_table
            .active_mut()
            .and_then(|set| set.items.iter_mut().find(|item| item.id == item_id))
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?;
        let candidate = LightTableItemInput {
            name: item.name.clone(),
            source: item.source.clone(),
            visible: properties.visible,
            opacity_milli: properties.opacity_milli,
            display_mode: properties.display_mode,
            display_color: properties.display_color,
            translate_x_milli: properties.translate_x_milli,
            translate_y_milli: properties.translate_y_milli,
            scale_x_milli: properties.scale_x_milli,
            scale_y_milli: properties.scale_y_milli,
            rotation_milli_degrees: properties.rotation_milli_degrees,
        };
        validate_item_input(&candidate)?;
        item.visible = candidate.visible;
        item.opacity_milli = candidate.opacity_milli;
        item.display_mode = candidate.display_mode;
        item.display_color = candidate.display_color;
        item.translate_x_milli = candidate.translate_x_milli;
        item.translate_y_milli = candidate.translate_y_milli;
        item.scale_x_milli = candidate.scale_x_milli;
        item.scale_y_milli = candidate.scale_y_milli;
        item.rotation_milli_degrees = candidate.rotation_milli_degrees;
        edit.commit(self)
    }

    /// Reloads item source pixels and identity from encoded common-raster bytes.
    ///
    /// Decode/validation failure retains the previous valid source; success is one
    /// undoable document edit.
    pub fn light_table_reload_common_raster(
        &mut self,
        item_id: u64,
        format: CommonRasterFormat,
        bytes: &[u8],
        document_uuid: u128,
        source_revision: u64,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let item_id = LightTableItemId::from_raw(item_id);
        let raster = decode_common_raster(format, bytes)?;
        let reference_frame = RectI32 {
            x: 0,
            y: 0,
            width: i32::try_from(raster.info.width)
                .map_err(|_| CoreError::InvalidArgument("reference width exceeds i32"))?,
            height: i32::try_from(raster.info.height)
                .map_err(|_| CoreError::InvalidArgument("reference height exceeds i32"))?,
        };
        let replacement = LightTableSource::from_common_raster(
            document_uuid,
            source_revision,
            reference_frame,
            &raster,
        )?;
        let input = {
            let item = self
                .document
                .as_ref()
                .ok_or(CoreError::NoDocument)?
                .light_table
                .active()
                .and_then(|set| set.items.iter().find(|item| item.id == item_id))
                .ok_or(CoreError::InvalidArgument(
                    "light-table item ID does not exist",
                ))?;
            LightTableItemInput {
                name: item.name.clone(),
                source: replacement,
                visible: item.visible,
                opacity_milli: item.opacity_milli,
                display_mode: item.display_mode,
                display_color: item.display_color,
                translate_x_milli: item.translate_x_milli,
                translate_y_milli: item.translate_y_milli,
                scale_x_milli: item.scale_x_milli,
                scale_y_milli: item.scale_y_milli,
                rotation_milli_degrees: item.rotation_milli_degrees,
            }
        };
        self.light_table_update_item(item_id.get(), input)
    }

    /// Returns owned metadata for items in the active set, in stacking order.
    pub fn light_table_items(&self) -> Result<Vec<LightTableItemInfo>, CoreError> {
        let state = &self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table;
        let set = state
            .active()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?;
        Ok(set
            .items
            .iter()
            .map(|item| LightTableItemInfo {
                id: item.id.get(),
                source_plane_id: item.source_plane_id.get(),
                name: item.name.clone(),
                source_document_uuid: item.source.document_uuid,
                source_revision: item.source.source_revision,
                visible: item.visible,
                opacity_milli: item.opacity_milli,
                effective_opacity_milli: effective_opacity(
                    item.opacity_milli,
                    set.global_opacity_milli,
                ),
                display_mode: item.display_mode,
                display_color: item.display_color,
                translate_x_milli: item.translate_x_milli,
                translate_y_milli: item.translate_y_milli,
                scale_x_milli: item.scale_x_milli,
                scale_y_milli: item.scale_y_milli,
                rotation_milli_degrees: item.rotation_milli_degrees,
            })
            .collect())
    }

    /// Returns owned metadata for every set in deterministic order.
    pub fn light_table_sets(&self) -> Result<Vec<LightTableSetInfo>, CoreError> {
        let state = &self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table;
        Ok(state
            .sets
            .iter()
            .map(|set| LightTableSetInfo {
                id: set.id.get(),
                name: set.name.clone(),
                active: set.id == state.active_set_id,
                global_opacity_milli: set.global_opacity_milli,
                item_count: set.items.len(),
            })
            .collect())
    }

    pub(crate) fn light_table_item_owners(&self) -> Result<Vec<(u64, u64)>, CoreError> {
        let state = &self
            .document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .light_table;
        let mut result = Vec::new();
        result
            .try_reserve_exact(state.item_count())
            .map_err(|_| CoreError::InvalidState("light-table item snapshot allocation failed"))?;
        for set in &state.sets {
            for item in &set.items {
                result.push((set.id.get(), item.id.get()));
            }
        }
        Ok(result)
    }

    /// Replaces an item's name, source, and display properties while retaining IDs.
    ///
    /// Success is one undoable edit; validation failure retains the existing item.
    pub fn light_table_update_item(
        &mut self,
        item_id: u64,
        mut input: LightTableItemInput,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableUpdateItem {
                    item_id,
                    input,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let item_id = LightTableItemId::from_raw(item_id);
        validate_item_input(&input)?;
        let mut staged_assets = self.assets.clone();
        input.source.intern_into(&mut staged_assets)?;
        let base_revision = self.document_revision;
        let mut edit = self.begin_document_edit()?;
        let item = edit
            .working_mut()
            .light_table
            .active_mut()
            .and_then(|set| set.items.iter_mut().find(|item| item.id == item_id))
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?;
        *item = LightTableItem {
            id: item.id,
            source_plane_id: item.source_plane_id,
            name: input.name,
            source: input.source,
            visible: input.visible,
            opacity_milli: input.opacity_milli,
            display_mode: input.display_mode,
            display_color: input.display_color,
            translate_x_milli: input.translate_x_milli,
            translate_y_milli: input.translate_y_milli,
            scale_x_milli: input.scale_x_milli,
            scale_y_milli: input.scale_y_milli,
            rotation_milli_degrees: input.rotation_milli_degrees,
        };
        staged_assets =
            self.prepare_asset_store_for_document_edit(staged_assets, edit.working_mut())?;
        let outcome = edit.commit(self)?;
        if outcome.revision() != base_revision.get() {
            self.assets = staged_assets;
        }
        Ok(outcome)
    }

    /// Removes an item from the active set as one undoable edit.
    pub fn light_table_remove_item(&mut self, item_id: u64) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableRemoveItem { item_id })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let item_id = LightTableItemId::from_raw(item_id);
        let mut edit = self.begin_document_edit()?;
        let items = &mut edit
            .working_mut()
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .items;
        let index =
            items
                .iter()
                .position(|item| item.id == item_id)
                .ok_or(CoreError::InvalidArgument(
                    "light-table item ID does not exist",
                ))?;
        items.remove(index);
        edit.commit(self)
    }

    /// Moves an active-set item to a zero-based stacking index.
    ///
    /// Success is one undoable document edit.
    pub fn light_table_reorder_item(
        &mut self,
        item_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            let destination_index = u64::try_from(destination_index).map_err(|_| {
                CoreError::InvalidArgument("light-table item destination is not representable")
            })?;
            return self
                .execute_canonical_invocation(CanonicalInvocation::LightTableReorderItem {
                    item_id,
                    destination_index,
                })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        let item_id = LightTableItemId::from_raw(item_id);
        let mut edit = self.begin_document_edit()?;
        let items = &mut edit
            .working_mut()
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .items;
        if destination_index >= items.len() {
            return Err(CoreError::InvalidArgument(
                "light-table item destination is outside bounds",
            ));
        }
        let source_index =
            items
                .iter()
                .position(|item| item.id == item_id)
                .ok_or(CoreError::InvalidArgument(
                    "light-table item ID does not exist",
                ))?;
        let item = items.remove(source_index);
        items.insert(destination_index, item);
        edit.commit(self)
    }

    /// Samples the composed active light table at an in-bounds document pixel.
    ///
    /// Transparent/unavailable samples are errors; the query is read-only.
    pub fn light_table_sample(&self, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        document
            .light_table
            .sample(document.frames.reference_frame, x, y)?
            .ok_or(CoreError::InvalidState(
                "light-table sample is transparent or unavailable",
            ))
    }

    /// Swaps the clean active document with one light-table source.
    ///
    /// Unsaved document changes are rejected. Editor-only dirty is retained and does
    /// not block the swap because non-target EditorState survives intact. Success
    /// installs the selected source with a clean document savepoint and preserves the
    /// outgoing flattened document as that item. Deterministic active-target
    /// reconciliation may advance the editor revision and leave the combined session
    /// dirty. Validation failure is atomic.
    pub fn light_table_swap_with_active(
        &mut self,
        item_id: u64,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let item_id = LightTableItemId::from_raw(item_id);
        let current = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if self.savepoint != Some(self.current_state) {
            return Err(CoreError::UnsavedChanges);
        }
        let current = current.clone();
        let set_index = current
            .light_table
            .sets
            .iter()
            .position(|set| set.id == current.light_table.active_set_id)
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?;
        let item_index = current.light_table.sets[set_index]
            .items
            .iter()
            .position(|item| item.id == item_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?;
        let mut staged_assets = self.assets.clone();
        let mut selected_source = current.light_table.sets[set_index].items[item_index]
            .source
            .clone();
        selected_source.intern_into(&mut staged_assets)?;
        let mut outgoing = LightTableSource::from_tile_raster(
            current.uuid,
            self.document_revision.get().max(1),
            current.frames.reference_frame,
            current.dpi_x_milli,
            current.dpi_y_milli,
            flatten_document(&current, &self.assets, self.document_revision.get().max(1))?,
        )?;
        outgoing.intern_into(&mut staged_assets)?;
        let mut next_id = self.next_id;
        let ids = DocumentIds {
            document: next_id.take_document(),
            layer: next_id.take_layer(),
            main_plane: next_id.take_plane(),
            color_plane: next_id.take_plane(),
            selection_plane: next_id.take_plane(),
            fill_protection_plane: next_id.take_plane(),
            light_table_set: next_id.take_light_table_set(),
            cell: next_id.take_cell(),
        };
        let mut next = CellDocument::new(
            ids,
            selected_source.document_uuid,
            PaperSpec {
                width: selected_source.width(),
                height: selected_source.height(),
                dpi_x_milli: selected_source.dpi_x_milli,
                dpi_y_milli: selected_source.dpi_y_milli,
            },
        )?;
        next.frames.reference_frame = selected_source.reference_frame;
        next.plane_for_role_mut(ActivePlane::Color)?.raster =
            selected_source.immutable_raster().clone();
        next.light_table = current.light_table;
        next.light_table.sets[set_index].items[item_index].source = outgoing;
        staged_assets = self.prepare_asset_store_for_session_reset(staged_assets, &next)?;

        let revision = self.next_document_revision()?;
        let editor = self.stage_reconciled_editor_target(&next, None, None)?;
        self.document = Some(next);
        self.document_revision = revision;
        self.next_id = next_id;
        self.assets = staged_assets;
        self.render_cache.clear();
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.floating = None;
        self.motion_check = None;
        self.publish_editor_session(editor);
        self.document_info()
    }
}
