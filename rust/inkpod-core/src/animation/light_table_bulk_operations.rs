use super::light_table::*;
use super::light_table_bulk::*;
use super::*;
use crate::primitive::CanonicalInvocation;

impl Core {
    /// Captures a side-effect-free, stale-detecting bulk-registration request.
    ///
    /// `neighbor_count == 0` is valid and previews/commits as a no-op. Opacity
    /// values must be in `0..=1000`. The current sequence cell is never a
    /// candidate. This query does not allocate persistent IDs or change document,
    /// sequence, history, revisions, dirty state, or savepoints.
    pub fn light_table_bulk_registration_request(
        &self,
        target_set_id: u64,
        direction: LightTableBulkDirection,
        neighbor_count: u32,
        base_opacity_milli: u32,
        distance_step_milli: u32,
    ) -> Result<LightTableBulkRegistrationRequest, CoreError> {
        self.ensure_no_active_stroke()?;
        validate_bulk_options(neighbor_count, base_opacity_milli, distance_step_milli)?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if !document
            .light_table
            .sets
            .iter()
            .any(|set| set.id.get() == target_set_id)
        {
            return Err(CoreError::InvalidArgument(
                "light-table bulk target set ID does not exist",
            ));
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let active_index = sequence.active_index.ok_or(CoreError::InvalidState(
            "the active document is not in the configured sequence",
        ))?;
        let active = sequence
            .cells
            .get(active_index)
            .ok_or(CoreError::InvalidState(
                "the active sequence index is invalid",
            ))?;
        if active.document_uuid != document.uuid {
            return Err(CoreError::InvalidState(
                "the active sequence source does not match the document",
            ));
        }
        Ok(LightTableBulkRegistrationRequest {
            target_set_id,
            direction,
            neighbor_count,
            base_opacity_milli,
            distance_step_milli,
            base_document_revision: self.document_revision.get(),
            sequence_revision: sequence.revision,
            active_document_uuid: active.document_uuid,
            active_source_generation: active.source_generation,
        })
    }

    /// Builds the exact duplicate and z-order preview for a captured request.
    ///
    /// Duplicate matching is by persistent source-document UUID only. Existing
    /// item properties and source revision are reported and left untouched. A
    /// stale or invalid token returns an error without mutation.
    pub fn preview_light_table_bulk_registration(
        &self,
        request: &LightTableBulkRegistrationRequest,
    ) -> Result<LightTableBulkRegistrationPreview, CoreError> {
        self.ensure_no_active_stroke()?;
        let (document, sequence, active_index, target_set) =
            self.validate_light_table_bulk_request(request)?;
        let _ = document;

        let count = usize::try_from(request.neighbor_count).map_err(|_| {
            CoreError::InvalidArgument("light-table bulk neighbor count is not representable")
        })?;
        let mut indices = Vec::new();
        if matches!(
            request.direction,
            LightTableBulkDirection::Previous | LightTableBulkDirection::Both
        ) {
            let start = active_index.saturating_sub(count);
            indices.extend(start..active_index);
        }
        if matches!(
            request.direction,
            LightTableBulkDirection::Next | LightTableBulkDirection::Both
        ) {
            let end = active_index
                .checked_add(count)
                .and_then(|value| value.checked_add(1))
                .unwrap_or(usize::MAX)
                .min(sequence.cells.len());
            indices.extend(active_index.saturating_add(1)..end);
        }
        indices.sort_unstable_by(|left, right| right.cmp(left));

        let mut seen = BTreeMap::new();
        for item in &target_set.items {
            seen.entry(item.source.document_uuid)
                .or_insert(item.source.source_revision);
        }
        let mut entries = Vec::with_capacity(indices.len());
        let mut add_count = 0_u32;
        let mut skip_count = 0_u32;
        for index in indices {
            let cell = sequence.cells.get(index).ok_or(CoreError::InvalidState(
                "light-table bulk sequence candidate is missing",
            ))?;
            let distance = u32::try_from(active_index.abs_diff(index)).map_err(|_| {
                CoreError::InvalidState("light-table bulk distance is not representable")
            })?;
            let decrement = request
                .distance_step_milli
                .checked_mul(distance.saturating_sub(1))
                .ok_or(CoreError::InvalidArgument(
                    "light-table bulk opacity calculation overflows",
                ))?;
            let opacity_milli = request.base_opacity_milli.saturating_sub(decrement);
            let existing_source_revision = seen.get(&cell.document_uuid).copied();
            let action = if existing_source_revision.is_some() {
                skip_count = skip_count.checked_add(1).ok_or(CoreError::InvalidState(
                    "light-table bulk skip count overflows",
                ))?;
                LightTableBulkRegistrationAction::SkipExisting
            } else {
                add_count = add_count.checked_add(1).ok_or(CoreError::InvalidState(
                    "light-table bulk add count overflows",
                ))?;
                seen.insert(cell.document_uuid, cell.source_generation);
                LightTableBulkRegistrationAction::Add
            };
            entries.push(LightTableBulkRegistrationEntry {
                sequence_index: u32::try_from(index).map_err(|_| {
                    CoreError::InvalidState("light-table bulk sequence index is not representable")
                })?,
                cell_number: cell.cell_number,
                name: cell.name.clone(),
                document_uuid: cell.document_uuid,
                source_generation: cell.source_generation,
                distance,
                opacity_milli,
                action,
                existing_source_revision,
            });
        }
        Ok(LightTableBulkRegistrationPreview {
            target_set_id: request.target_set_id,
            entries,
            add_count,
            skip_count,
        })
    }

    /// Commits all nonduplicate preview entries as one canonical undo unit.
    ///
    /// Newly added items form one block above the existing target-set items in
    /// preview order. An empty/all-duplicate request is a no-op. Invalid, stale,
    /// overflow, allocation, and asset failures publish no partial state and
    /// consume no persistent IDs.
    pub fn light_table_bulk_register(
        &mut self,
        request: LightTableBulkRegistrationRequest,
    ) -> Result<(DispatchOutcome, LightTableBulkRegistrationSummary), CoreError> {
        let preview = self.preview_light_table_bulk_registration(&request)?;
        if preview.add_count == 0 {
            return Ok((
                self.noop_outcome(),
                LightTableBulkRegistrationSummary {
                    add_count: 0,
                    skip_count: preview.skip_count,
                    added_item_ids: Vec::new(),
                },
            ));
        }

        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let mut inputs = Vec::with_capacity(preview.add_count as usize);
        for entry in &preview.entries {
            if entry.action != LightTableBulkRegistrationAction::Add {
                continue;
            }
            let cell = sequence.cells.get(entry.sequence_index as usize).ok_or(
                CoreError::InvalidState("light-table bulk sequence candidate is missing"),
            )?;
            let source = LightTableSource::from_tile_raster(
                cell.document_uuid,
                cell.source_generation,
                cell.frames.reference_frame,
                cell.dpi_x_milli,
                cell.dpi_y_milli,
                cell.raster.clone(),
            )?;
            let mut input = LightTableItemInput::new(cell.name.clone(), source);
            input.opacity_milli = entry.opacity_milli;
            inputs.push(input);
        }
        let result =
            self.execute_canonical_invocation(CanonicalInvocation::LightTableBulkRegister {
                target_set_id: request.target_set_id,
                inputs,
            })?;
        if result.output_ids.len() != preview.add_count as usize {
            return Err(CoreError::InvalidState(
                "light-table bulk primitive returned an invalid output count",
            ));
        }
        Ok((
            result.dispatch,
            LightTableBulkRegistrationSummary {
                add_count: preview.add_count,
                skip_count: preview.skip_count,
                added_item_ids: result.output_ids,
            },
        ))
    }

    pub(crate) fn light_table_bulk_register_resolved(
        &mut self,
        target_set_id: u64,
        mut inputs: Vec<LightTableItemInput>,
    ) -> Result<(DispatchOutcome, Vec<u64>), CoreError> {
        if !self.canonical_invocation_is_active() {
            let result =
                self.execute_canonical_invocation(CanonicalInvocation::LightTableBulkRegister {
                    target_set_id,
                    inputs,
                })?;
            return Ok((result.dispatch, result.output_ids));
        }
        self.ensure_no_active_stroke()?;
        if inputs.is_empty() || inputs.len() > MAX_LIGHT_TABLE_ITEMS {
            return Err(CoreError::InvalidArgument(
                "light-table bulk resolved item count is outside bounds",
            ));
        }
        for input in &inputs {
            validate_item_input(input)?;
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let set = document
            .light_table
            .sets
            .iter()
            .find(|set| set.id.get() == target_set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table bulk target set ID does not exist",
            ))?;
        if document
            .light_table
            .item_count()
            .checked_add(inputs.len())
            .is_none_or(|count| count > MAX_LIGHT_TABLE_ITEMS)
        {
            return Err(CoreError::InvalidState(
                "light-table item count exceeds its bound",
            ));
        }
        let mut source_uuids = BTreeSet::new();
        for input in &inputs {
            if !source_uuids.insert(input.source.document_uuid)
                || set
                    .items
                    .iter()
                    .any(|item| item.source.document_uuid == input.source.document_uuid)
            {
                return Err(CoreError::InvalidArgument(
                    "light-table bulk resolved sources contain a duplicate UUID",
                ));
            }
        }
        let id_slots = u64::try_from(inputs.len())
            .ok()
            .and_then(|count| count.checked_mul(2))
            .ok_or(CoreError::InvalidState(
                "light-table bulk stable-ID count overflows",
            ))?;
        let final_cursor =
            self.next_id
                .next_raw()
                .checked_add(id_slots)
                .ok_or(CoreError::InvalidState(
                    "light-table bulk stable-ID namespace overflows",
                ))?;
        if final_cursor.saturating_sub(1) > MAX_PERSISTENT_NUMERIC_ID {
            return Err(CoreError::InvalidState(
                "light-table bulk stable-ID namespace overflows",
            ));
        }

        let mut staged_assets = self.assets.clone();
        for input in &mut inputs {
            input.source.intern_into(&mut staged_assets)?;
        }
        let base_revision = self.document_revision;
        let mut next_id = self.next_id;
        let mut ids = Vec::with_capacity(inputs.len());
        let mut items = Vec::with_capacity(inputs.len());
        for input in inputs {
            let item_id = next_id.take_light_table_item();
            let source_plane_id = next_id.take_plane();
            ids.push(item_id.get());
            items.push(LightTableItem {
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
            });
        }
        let mut edit = self.begin_document_edit()?;
        let target = edit
            .working_mut()
            .light_table
            .sets
            .iter_mut()
            .find(|set| set.id.get() == target_set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table bulk target set ID does not exist",
            ))?;
        target.items.splice(0..0, items);
        staged_assets =
            self.prepare_asset_store_for_document_edit(staged_assets, edit.working_mut())?;
        let outcome = edit.commit(self)?;
        if outcome.revision() != base_revision.get() {
            self.next_id = next_id;
            self.assets = staged_assets;
        }
        Ok((outcome, ids))
    }

    fn validate_light_table_bulk_request<'a>(
        &'a self,
        request: &LightTableBulkRegistrationRequest,
    ) -> Result<
        (
            &'a CellDocument,
            &'a SequenceState,
            usize,
            &'a LightTableSet,
        ),
        CoreError,
    > {
        validate_bulk_options(
            request.neighbor_count,
            request.base_opacity_milli,
            request.distance_step_milli,
        )?;
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        if self.document_revision.get() != request.base_document_revision {
            return Err(CoreError::InvalidState(
                "light-table bulk request document revision is stale",
            ));
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        if sequence.revision != request.sequence_revision {
            return Err(CoreError::InvalidState(
                "light-table bulk request sequence revision is stale",
            ));
        }
        let active_index = sequence.active_index.ok_or(CoreError::InvalidState(
            "the active document is not in the configured sequence",
        ))?;
        let active = sequence
            .cells
            .get(active_index)
            .ok_or(CoreError::InvalidState(
                "the active sequence index is invalid",
            ))?;
        if document.uuid != request.active_document_uuid
            || active.document_uuid != request.active_document_uuid
            || active.source_generation != request.active_source_generation
        {
            return Err(CoreError::InvalidState(
                "light-table bulk request active source is stale",
            ));
        }
        let target_set = document
            .light_table
            .sets
            .iter()
            .find(|set| set.id.get() == request.target_set_id)
            .ok_or(CoreError::InvalidArgument(
                "light-table bulk target set ID does not exist",
            ))?;
        Ok((document, sequence, active_index, target_set))
    }
}

fn validate_bulk_options(
    neighbor_count: u32,
    base_opacity_milli: u32,
    distance_step_milli: u32,
) -> Result<(), CoreError> {
    if usize::try_from(neighbor_count).map_or(true, |count| count > MAX_SEQUENCE_CELLS) {
        return Err(CoreError::InvalidArgument(
            "light-table bulk neighbor count exceeds its bound",
        ));
    }
    if base_opacity_milli > 1_000 || distance_step_milli > 1_000 {
        return Err(CoreError::InvalidArgument(
            "light-table bulk opacity is outside zero through one thousand",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::StableIdCursor;

    fn sequence_source(name: &str, uuid: u128, value: u8) -> SequenceCellSource {
        let raster = CommonRaster::new(
            1,
            1,
            PixelFormat::StraightRgba8,
            Some(DEFAULT_DPI_MILLI),
            Some(DEFAULT_DPI_MILLI),
            vec![value, 0, 0, 255],
        )
        .unwrap();
        SequenceCellSource::from_common_raster(name, uuid, &raster).unwrap()
    }

    #[test]
    fn stable_id_overflow_rejects_the_resolved_block_without_publication() {
        let mut core = Core::new();
        core.new_cell(1, 1, DEFAULT_DPI_MILLI, DEFAULT_DPI_MILLI)
            .unwrap();
        let current_uuid = core.document_info().unwrap().document_uuid;
        core.set_sequence(vec![
            sequence_source("cell1.png", 0x7a01, 1),
            sequence_source("cell2.png", current_uuid, 2),
        ])
        .unwrap();
        let target_set_id = core.light_table_sets().unwrap()[0].id;
        let request = core
            .light_table_bulk_registration_request(
                target_set_id,
                LightTableBulkDirection::Previous,
                1,
                1_000,
                0,
            )
            .unwrap();
        core.next_id = StableIdCursor::from_next_raw(MAX_PERSISTENT_NUMERIC_ID);
        let before = core.document_info().unwrap();
        let history_before = core.history_entries().to_vec();
        let journal_before = core.journal_entries().to_vec();
        let ids_before = core.next_id;

        assert!(matches!(
            core.light_table_bulk_register(request),
            Err(CoreError::InvalidState(
                "light-table bulk stable-ID namespace overflows"
            ))
        ));
        assert_eq!(core.document_info().unwrap(), before);
        assert!(core.light_table_items().unwrap().is_empty());
        assert_eq!(core.history_entries(), history_before);
        assert_eq!(core.journal_entries(), journal_before);
        assert_eq!(core.next_id, ids_before);
    }
}
