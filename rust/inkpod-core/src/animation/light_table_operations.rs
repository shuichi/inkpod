use super::light_table::*;
use super::raster::*;
use super::*;

impl Core {
    pub fn light_table_set_global_opacity(
        &mut self,
        opacity_milli: u32,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        if opacity_milli > 1_000 {
            return Err(CoreError::InvalidArgument(
                "light-table opacity exceeds one thousand",
            ));
        }
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        after
            .light_table
            .active_mut()
            .ok_or(CoreError::InvalidState("active light-table set is missing"))?
            .global_opacity_milli = opacity_milli;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_create_set(
        &mut self,
        name: impl Into<String>,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let name = name.into();
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
        let id = self.allocate_id();
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let name = unique_light_table_set_name(&after.light_table.sets, &name);
        after.light_table.sets.push(LightTableSet {
            id,
            name,
            global_opacity_milli: 1_000,
            items: Vec::new(),
        });
        after.light_table.active_set_id = id;
        Ok((self.commit_document_edit(before, after)?, id))
    }

    pub fn light_table_duplicate_set(
        &mut self,
        set_id: u64,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let source = before
            .light_table
            .sets
            .iter()
            .find(|set| set.id == set_id)
            .cloned()
            .ok_or(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ))?;
        if before.light_table.sets.len() >= MAX_LIGHT_TABLE_SETS
            || before
                .light_table
                .item_count()
                .checked_add(source.items.len())
                .is_none_or(|count| count > MAX_LIGHT_TABLE_ITEMS)
        {
            return Err(CoreError::InvalidState(
                "duplicated light-table content exceeds its bound",
            ));
        }
        let new_set_id = self.allocate_id();
        let mut items = Vec::with_capacity(source.items.len());
        for mut item in source.items {
            item.id = self.allocate_id();
            item.source_plane_id = self.allocate_id();
            items.push(item);
        }
        let mut after = before.clone();
        let name = unique_light_table_set_name(&after.light_table.sets, &source.name);
        after.light_table.sets.push(LightTableSet {
            id: new_set_id,
            name,
            global_opacity_milli: source.global_opacity_milli,
            items,
        });
        after.light_table.active_set_id = new_set_id;
        Ok((self.commit_document_edit(before, after)?, new_set_id))
    }

    pub fn light_table_delete_set(&mut self, set_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.light_table.sets.len() == 1 {
            return Err(CoreError::InvalidState(
                "the final light-table set cannot be deleted",
            ));
        }
        let mut after = before.clone();
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
        self.commit_document_edit(before, after)
    }

    pub fn light_table_rename_set(
        &mut self,
        set_id: u64,
        name: impl Into<String>,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let name = name.into();
        validate_node_name(&name)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
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
        self.commit_document_edit(before, after)
    }

    pub fn light_table_reorder_set(
        &mut self,
        set_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if destination_index >= before.light_table.sets.len() {
            return Err(CoreError::InvalidArgument(
                "light-table set destination is outside bounds",
            ));
        }
        let mut after = before.clone();
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
        self.commit_document_edit(before, after)
    }

    pub fn light_table_set_active(&mut self, set_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if !before.light_table.sets.iter().any(|set| set.id == set_id) {
            return Err(CoreError::InvalidArgument(
                "light-table set ID does not exist",
            ));
        }
        let mut after = before.clone();
        after.light_table.active_set_id = set_id;
        self.commit_document_edit(before, after)
    }

    pub fn light_table_add_item(
        &mut self,
        input: LightTableItemInput,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
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
        let item_id = self.allocate_id();
        let source_plane_id = self.allocate_id();
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        after
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
        let outcome = self.commit_document_edit(before, after)?;
        Ok((outcome, item_id))
    }

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

    pub fn light_table_update_item_properties(
        &mut self,
        item_id: u64,
        properties: LightTableItemProperties,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let item = after
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
        self.commit_document_edit(before, after)
    }

    pub fn light_table_reload_common_raster(
        &mut self,
        item_id: u64,
        format: CommonRasterFormat,
        bytes: &[u8],
        document_uuid: u128,
        source_revision: u64,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
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
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        after
            .light_table
            .active_mut()
            .and_then(|set| set.items.iter_mut().find(|item| item.id == item_id))
            .ok_or(CoreError::InvalidArgument(
                "light-table item ID does not exist",
            ))?
            .source = replacement;
        self.commit_document_edit(before, after)
    }

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
                id: item.id,
                source_plane_id: item.source_plane_id,
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
                id: set.id,
                name: set.name.clone(),
                active: set.id == state.active_set_id,
                global_opacity_milli: set.global_opacity_milli,
                item_count: set.items.len(),
            })
            .collect())
    }

    pub fn light_table_update_item(
        &mut self,
        item_id: u64,
        input: LightTableItemInput,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_item_input(&input)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let item = after
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
        self.commit_document_edit(before, after)
    }

    pub fn light_table_remove_item(&mut self, item_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let items = &mut after
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
        self.commit_document_edit(before, after)
    }

    pub fn light_table_reorder_item(
        &mut self,
        item_id: u64,
        destination_index: usize,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let items = &mut after
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
        self.commit_document_edit(before, after)
    }

    pub fn light_table_sample(&self, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        document
            .light_table
            .sample(document.frames.reference_frame, x, y)?
            .ok_or(CoreError::InvalidState(
                "light-table sample is transparent or unavailable",
            ))
    }

    pub fn light_table_swap_with_active(
        &mut self,
        item_id: u64,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_info()?.dirty {
            return Err(CoreError::UnsavedChanges);
        }
        let current = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
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
        let selected_source = current.light_table.sets[set_index].items[item_index]
            .source
            .clone();
        let outgoing = LightTableSource {
            document_uuid: current.uuid,
            source_revision: self.document_revision.max(1),
            reference_frame: current.frames.reference_frame,
            dpi_x_milli: current.dpi_x_milli,
            dpi_y_milli: current.dpi_y_milli,
            raster: flatten_document(&current, self.document_revision.max(1))?,
        };
        let ids = DocumentIds {
            document: self.allocate_id(),
            layer: self.allocate_id(),
            main_plane: self.allocate_id(),
            color_plane: self.allocate_id(),
            selection_plane: self.allocate_id(),
            light_table_set: self.allocate_id(),
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
        next.plane_for_role_mut(ActivePlane::Color)?.raster = selected_source.raster;
        next.light_table = current.light_table;
        next.light_table.sets[set_index].items[item_index].source = outgoing;

        let revision = self.next_document_revision()?;
        self.document = Some(next);
        self.document_revision = revision;
        self.render_cache.clear();
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.floating = None;
        self.motion_check = None;
        self.document_info()
    }
}
