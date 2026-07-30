use super::raster::*;
use super::sequence::*;
use super::*;

impl Core {
    /// Installs validated sequence sources in deterministic natural-name order.
    ///
    /// This changes sequence-only state, clears motion/subpalette state, and does
    /// not change the active document, revision, history, dirty state, or savepoint.
    pub fn set_sequence(&mut self, mut cells: Vec<SequenceCellSource>) -> Result<(), CoreError> {
        if cells.is_empty() || cells.len() > MAX_SEQUENCE_CELLS {
            return Err(CoreError::InvalidArgument(
                "sequence cell count is outside bounds",
            ));
        }
        for cell in &cells {
            validate_sequence_cell(cell)?;
        }
        cells.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        if cells
            .windows(2)
            .any(|pair| pair[0].name.eq_ignore_ascii_case(&pair[1].name))
        {
            return Err(CoreError::InvalidArgument(
                "sequence contains duplicate names",
            ));
        }
        let current_uuid = self
            .document
            .as_ref()
            .map(|document| document.uuid)
            .unwrap_or(0);
        let active_index = cells
            .iter()
            .position(|cell| cell.document_uuid == current_uuid);
        self.sequence = Some(SequenceState {
            cells,
            active_index,
        });
        self.motion_check = None;
        self.subpalette_index = None;
        Ok(())
    }

    /// Decodes named common-raster files and atomically installs them as a sequence.
    ///
    /// Any decode or validation failure retains the previous sequence state.
    pub fn import_sequence(
        &mut self,
        format: CommonRasterFormat,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<(), CoreError> {
        if files.is_empty() || files.len() > MAX_SEQUENCE_CELLS {
            return Err(CoreError::InvalidArgument(
                "sequence import count is outside bounds",
            ));
        }
        let mut cells = Vec::with_capacity(files.len());
        for (index, (name, bytes)) in files.into_iter().enumerate() {
            let raster = decode_common_raster(format, &bytes)?;
            let uuid = (u128::from(0x494e_4b50_4f44_5334_u64) << 64)
                | u128::try_from(index + 1)
                    .map_err(|_| CoreError::InvalidState("sequence UUID index overflows"))?;
            cells.push(SequenceCellSource::from_common_raster(name, uuid, &raster)?);
        }
        self.set_sequence(cells)
    }

    /// Returns owned metadata and thumbnails in natural sequence order.
    pub fn sequence_cells(&self) -> Result<Vec<SequenceCellInfo>, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        sequence
            .cells
            .iter()
            .map(|cell| {
                Ok(SequenceCellInfo {
                    name: cell.name.clone(),
                    cell_number: cell.cell_number,
                    document_uuid: cell.document_uuid,
                    width: cell.raster.width(),
                    height: cell.raster.height(),
                    thumbnail: cell.thumbnail()?,
                })
            })
            .collect()
    }

    /// Activates the adjacent sequence cell, optionally wrapping at the ends.
    ///
    /// Dirty documents and active strokes are rejected. Endpoint no-op keeps the
    /// current document; a switch installs a clean document and resets history/view.
    pub fn sequence_step(
        &mut self,
        direction: SequenceDirection,
        loop_sequence: bool,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_info()?.dirty {
            return Err(CoreError::UnsavedChanges);
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let count = sequence.cells.len();
        let current = sequence.active_index.unwrap_or(match direction {
            SequenceDirection::Previous => count,
            SequenceDirection::Next => usize::MAX,
        });
        let target = match direction {
            SequenceDirection::Previous => {
                if current == 0 {
                    if loop_sequence { count - 1 } else { 0 }
                } else {
                    current.min(count) - 1
                }
            }
            SequenceDirection::Next => {
                if current == usize::MAX {
                    0
                } else if current + 1 >= count {
                    if loop_sequence { 0 } else { count - 1 }
                } else {
                    current + 1
                }
            }
        };
        self.sequence_activate(target)
    }

    /// Activates a sequence cell by zero-based natural-order index.
    ///
    /// The current index is a no-op. Dirty/invalid switches do not replace the active
    /// document; success establishes a clean savepoint with reset history and view.
    pub fn sequence_activate(&mut self, target: usize) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_info()?.dirty {
            return Err(CoreError::UnsavedChanges);
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        if target >= sequence.cells.len() {
            return Err(CoreError::InvalidArgument(
                "sequence target index is outside bounds",
            ));
        }
        if sequence.active_index == Some(target) {
            return self.document_info();
        }
        let source = sequence.cells[target].clone();
        let revision = self.next_document_revision()?;
        let document = self.document_from_sequence_source(&source, revision)?;
        self.document = Some(document);
        self.document_revision = revision;
        self.render_cache.clear();
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.floating = None;
        self.sequence
            .as_mut()
            .ok_or(CoreError::InvalidState("sequence disappeared"))?
            .active_index = Some(target);
        self.document_info()
    }

    /// Encodes every installed sequence source without mutating Core state.
    pub fn export_sequence(
        &self,
        format: CommonRasterFormat,
        composite_white: bool,
    ) -> Result<Vec<(String, Vec<u8>)>, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        sequence
            .cells
            .iter()
            .map(|cell| {
                let raster =
                    tile_to_common(&cell.raster, Some(cell.dpi_x_milli), Some(cell.dpi_y_milli))?;
                Ok((
                    cell.name.clone(),
                    encode_common_raster(format, &raster, composite_white)?,
                ))
            })
            .collect()
    }

    /// Registers one sequence cell as the read-only subpalette sampling source.
    pub fn set_subpalette_cell(&mut self, index: usize) -> Result<(), CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        if index >= sequence.cells.len() {
            return Err(CoreError::InvalidArgument(
                "subpalette sequence index is outside bounds",
            ));
        }
        self.subpalette_index = Some(index);
        Ok(())
    }

    /// Samples the registered subpalette cell at an in-bounds source pixel.
    pub fn subpalette_sample(&self, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = self
            .subpalette_index
            .ok_or(CoreError::InvalidState("subpalette has no registered cell"))?;
        Ok(sequence.cells[index].raster.pixel(x, y)?)
    }

    /// Starts sequence motion-check playback using a supported FPS.
    ///
    /// Playback state is transient and does not change document revisions/history.
    pub fn motion_check_start(
        &mut self,
        config: MotionCheckConfig,
    ) -> Result<MotionFrame, CoreError> {
        if !matches!(config.fps, 8 | 10 | 12 | 24 | 25 | 30) {
            return Err(CoreError::InvalidArgument(
                "motion-check FPS is unsupported",
            ));
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = sequence.active_index.unwrap_or(0);
        self.motion_check = Some(MotionCheckState {
            config,
            index,
            paused: false,
        });
        self.motion_frame()
    }

    /// Moves motion-check playback one frame in the requested direction.
    pub fn motion_check_step(
        &mut self,
        direction: SequenceDirection,
    ) -> Result<MotionFrame, CoreError> {
        let count = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .cells
            .len();
        let state = self
            .motion_check
            .as_mut()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        state.index = match direction {
            SequenceDirection::Previous => {
                if state.index == 0 {
                    if state.config.loop_playback {
                        count - 1
                    } else {
                        0
                    }
                } else {
                    state.index - 1
                }
            }
            SequenceDirection::Next => {
                if state.index + 1 >= count {
                    if state.config.loop_playback {
                        0
                    } else {
                        count - 1
                    }
                } else {
                    state.index + 1
                }
            }
        };
        self.motion_frame()
    }

    /// Toggles motion-check pause state and returns the current frame.
    pub fn motion_check_toggle_pause(&mut self) -> Result<MotionFrame, CoreError> {
        let state = self
            .motion_check
            .as_mut()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        state.paused = !state.paused;
        self.motion_frame()
    }

    /// Stops motion-check playback; calling it while stopped is a no-op.
    pub fn motion_check_stop(&mut self) {
        self.motion_check = None;
    }

    fn motion_frame(&self) -> Result<MotionFrame, CoreError> {
        let state = self
            .motion_check
            .as_ref()
            .ok_or(CoreError::InvalidState("motion check is not active"))?;
        let cell = &self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?
            .cells[state.index];
        Ok(MotionFrame {
            sequence_index: state.index,
            cell_number: cell.cell_number,
            name: cell.name.clone(),
            thumbnail: cell.thumbnail()?,
            paused: state.paused,
            fps: state.config.fps,
            include_selection: state.config.include_selection,
            include_light_table: state.config.include_light_table,
        })
    }

    fn document_from_sequence_source(
        &mut self,
        source: &SequenceCellSource,
        _revision: DocumentRevision,
    ) -> Result<CellDocument, CoreError> {
        let ids = DocumentIds {
            document: self.next_id.take_document(),
            layer: self.next_id.take_layer(),
            main_plane: self.next_id.take_plane(),
            color_plane: self.next_id.take_plane(),
            selection_plane: self.next_id.take_plane(),
            light_table_set: self.next_id.take_light_table_set(),
        };
        let mut document = CellDocument::new(
            ids,
            source.document_uuid,
            PaperSpec {
                width: source.raster.width(),
                height: source.raster.height(),
                dpi_x_milli: source.dpi_x_milli,
                dpi_y_milli: source.dpi_y_milli,
            },
        )?;
        document.frames = source.frames;
        document.plane_for_role_mut(ActivePlane::Color)?.raster = source.raster.clone();
        Ok(document)
    }
}
