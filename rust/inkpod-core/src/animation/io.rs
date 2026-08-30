use super::raster::*;
use super::*;
use crate::primitive::CanonicalInvocation;

#[derive(Clone, Copy)]
enum InitialRasterPlacement {
    Base,
    MainLinePlane,
}

impl Core {
    /// Decodes a common raster into the initial editable main-line plane of a clean cell.
    ///
    /// UUID must be nonzero. Success resets history, savepoint, view, sequence, and
    /// transient sessions; decode/validation failure leaves the current document intact.
    /// Exact RGBA8/16 pixels and alpha are preserved without automatic line separation.
    /// The main-line plane is selected, the underlay is transparent, and the immutable
    /// source asset belongs to Genesis rather than participating in composition.
    pub fn import_common_raster(
        &mut self,
        format: CommonRasterFormat,
        bytes: &[u8],
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if document_uuid == 0 {
            return Err(CoreError::InvalidArgument(
                "common-raster document UUID must be nonzero",
            ));
        }
        let raster = decode_common_raster(format, bytes)?;
        self.import_owned_common_raster(format, raster, document_uuid)
    }

    /// Opens an already decoded raster as an editable native document.
    ///
    /// The decoded cache entry remains caller-owned. Core validates and owns
    /// immutable canonical pixels before publication, records only the closed
    /// raster format as native file metadata, and never retains the external
    /// source path. Failure leaves the live document and its format unchanged.
    pub fn import_decoded_common_raster(
        &mut self,
        format: CommonRasterFormat,
        raster: &inkpod_format::CommonRaster,
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        raster.validate()?;
        if document_uuid == 0 {
            return Err(CoreError::InvalidArgument(
                "common-raster document UUID must be nonzero",
            ));
        }
        self.import_owned_common_raster(format, raster.clone(), document_uuid)
    }

    fn import_owned_common_raster(
        &mut self,
        format: CommonRasterFormat,
        raster: inkpod_format::CommonRaster,
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        let dpi_x_milli = raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI);
        let dpi_y_milli = raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI);
        let input = RasterAssetInput {
            width: raster.info.width,
            height: raster.info.height,
            pixel_format: raster.info.pixel_format,
            color_space: Some(AssetColorSpace::Srgb),
            alpha_semantics: AssetAlphaSemantics::Straight,
            canonical_stride: u64::from(raster.info.width)
                .checked_mul(raster.info.pixel_format.bytes_per_pixel() as u64)
                .ok_or(CoreError::InvalidArgument(
                    "common-raster asset stride overflows",
                ))?,
            pixels: raster.pixels,
            expected_id: None,
        };
        let info = self.new_cell_from_raster_asset_with_placement(
            input,
            dpi_x_milli,
            dpi_y_milli,
            document_uuid,
            InitialRasterPlacement::MainLinePlane,
        )?;
        self.raster_file_format = format;
        Ok(info)
    }

    /// Opens canonical raster bytes as the immutable Genesis base of a clean cell.
    ///
    /// The asset's exact pixel format, color/alpha semantics, dimensions, stride,
    /// payload length, and optional expected identity are validated before any live
    /// state changes. DPI is document metadata and deliberately does not contribute
    /// to [`AssetId`]. Success owns an immutable copy and retains no caller buffer,
    /// encoded-image bytes, codec, path, or provenance. History is reset to Genesis.
    pub fn new_cell_from_raster_asset(
        &mut self,
        raster: RasterAssetInput,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        document_uuid: u128,
    ) -> Result<DocumentInfo, CoreError> {
        self.new_cell_from_raster_asset_with_placement(
            raster,
            dpi_x_milli,
            dpi_y_milli,
            document_uuid,
            InitialRasterPlacement::Base,
        )
    }

    fn new_cell_from_raster_asset_with_placement(
        &mut self,
        raster: RasterAssetInput,
        dpi_x_milli: u32,
        dpi_y_milli: u32,
        document_uuid: u128,
        placement: InitialRasterPlacement,
    ) -> Result<DocumentInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        if document_uuid == 0 {
            return Err(CoreError::InvalidArgument(
                "asset document UUID must be nonzero",
            ));
        }
        let width = raster.width;
        let height = raster.height;
        let mut assets = asset::AssetStore::default();
        let record = assets.ingest_raster(raster)?;
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
        let mut document = CellDocument::new(
            ids,
            document_uuid,
            PaperSpec {
                width,
                height,
                dpi_x_milli,
                dpi_y_milli,
            },
        )?;
        let (raster_source, active_plane) = match placement {
            InitialRasterPlacement::Base => {
                document.base_surface = BaseSurface::Asset(record.id());
                assets = self.prepare_asset_store_for_session_reset(assets, &document)?;
                (None, ids.main_plane)
            }
            InitialRasterPlacement::MainLinePlane => {
                let source = record
                    .raster()
                    .ok_or(CoreError::InvalidState("import source is not a raster"))?;
                initialize_imported_main_line(&mut document, source.as_ref().clone())?;
                assets.garbage_collect([record.id()])?;
                (
                    Some(genesis::GenesisRasterSource {
                        plane_id: ids.main_plane,
                        asset_id: record.id(),
                    }),
                    ids.main_plane,
                )
            }
        };
        let genesis = genesis::Genesis {
            document: document.clone(),
            raster_source,
        };
        let editor = self.initial_editor_session(
            Some(EditorTarget {
                layer_id: ids.layer.get(),
                plane_id: active_plane.get(),
            }),
            true,
        );
        let revision = self.next_document_revision()?;
        let persistence_state = self.persistence_state.next()?;
        self.cancel_stroke();
        self.shooting_frame_preview = None;
        self.filter_preview = None;
        self.last_filter = None;
        self.next_id = next_id;
        self.assets = assets;
        self.document = Some(document);
        self.document_revision = revision;
        self.render_cache.clear();
        self.reset_history(true);
        self.genesis = Some(genesis);
        self.reset_view();
        self.current_path = None;
        self.raster_file_format = self.new_cell_raster_format;
        self.io_pair_authority = None;
        self.persistence_state = persistence_state;
        self.recovered = false;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.motion_check = None;
        self.sequence = None;
        self.sequence_render_catalog_changed();
        self.subpalette_index = None;
        self.publish_editor_session(Some(editor));
        self.document_info()
    }

    /// Flattens the active document and encodes it into a common raster format.
    ///
    /// The query does not advance revisions, history, dirty state, or savepoint.
    pub fn export_common_raster(
        &self,
        format: CommonRasterFormat,
        composite_white: bool,
    ) -> Result<Vec<u8>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let flattened =
            flatten_document(document, &self.assets, self.document_revision.get().max(1))?;
        let raster = tile_to_common(
            &flattened,
            Some(document.dpi_x_milli),
            Some(document.dpi_y_milli),
        )?;
        Ok(encode_common_raster(format, &raster, composite_white)?)
    }

    /// Normal-save companion encoding retains native component depth. Legacy
    /// explicit display exports keep their existing RGBA8 contract separately.
    pub(crate) fn export_native_save_raster(
        &self,
        format: CommonRasterFormat,
    ) -> Result<Vec<u8>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let requires_16_bit = |format| {
            matches!(
                format,
                PixelFormat::Grayscale16 | PixelFormat::StraightRgba16
            )
        };
        let base_is_16_bit = match document.base_surface {
            BaseSurface::SolidWhite | BaseSurface::Transparent => false,
            BaseSurface::Asset(id) => requires_16_bit(
                self.assets
                    .get(id)
                    .ok_or(CoreError::InvalidState("Genesis base asset is missing"))?
                    .raster()
                    .ok_or(CoreError::InvalidState(
                        "Genesis base asset is not a raster",
                    ))?
                    .format(),
            ),
        };
        let visible_is_16_bit = document.layers.iter().any(|layer| {
            layer.visible
                && layer
                    .planes
                    .iter()
                    .any(|plane| plane.visible && requires_16_bit(plane.raster.format()))
        });
        if !base_is_16_bit
            && !visible_is_16_bit
            && !matches!(document.main_line_color, PixelValue::Rgba16(_))
        {
            return self.export_common_raster(format, false);
        }
        if matches!(format, CommonRasterFormat::Tga | CommonRasterFormat::Bmp) {
            return Err(CoreError::InvalidArgument(
                "normal-save raster format cannot retain 16-bit components",
            ));
        }
        bounded_document_pixels(document.width, document.height)?;
        let byte_count = u64::from(document.width)
            .checked_mul(u64::from(document.height))
            .and_then(|count| count.checked_mul(8))
            .and_then(|count| usize::try_from(count).ok())
            .ok_or(CoreError::InvalidArgument(
                "normal-save raster size overflows",
            ))?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(byte_count)
            .map_err(|_| CoreError::InvalidState("normal-save raster allocation failed"))?;
        visit_native_save_composite_rgba16(document, &self.assets, |_, _, rgba| {
            for channel in rgba {
                pixels.extend_from_slice(&channel.to_le_bytes());
            }
            Ok(())
        })?;
        let raster = inkpod_format::CommonRaster::new(
            document.width,
            document.height,
            PixelFormat::StraightRgba16,
            Some(document.dpi_x_milli),
            Some(document.dpi_y_milli),
            pixels,
        )?;
        Ok(encode_common_raster(format, &raster, false)?)
    }

    /// Flattens the document with instruction overlays and the optional
    /// angled shooting-frame overlay, then encodes a common raster.
    ///
    /// This explicit query does not change ordinary export authority, paper-fit
    /// bounds, thumbnails, revisions, history, dirty state, or savepoints.
    pub fn export_instruction_common_raster(
        &self,
        format: CommonRasterFormat,
        composite_white: bool,
    ) -> Result<Vec<u8>, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let flattened = flatten_document_with_instructions(
            document,
            &self.assets,
            self.document_revision.get().max(1),
        )?;
        let raster = tile_to_common(
            &flattened,
            Some(document.dpi_x_milli),
            Some(document.dpi_y_milli),
        )?;
        Ok(encode_common_raster(format, &raster, composite_white)?)
    }

    /// Builds a bounded aspect-preserving thumbnail of the visible document.
    ///
    /// The returned straight-alpha RGBA8 pixels are owned by the caller. This is
    /// a query and does not change revisions, history, dirty state, or savepoint.
    pub fn document_thumbnail(&self) -> Result<Thumbnail, CoreError> {
        self.document_thumbnail_with_max(THUMBNAIL_MAX_DIMENSION)
    }

    pub(crate) fn document_thumbnail_with_max(
        &self,
        maximum_dimension: u32,
    ) -> Result<Thumbnail, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let flattened =
            flatten_document(document, &self.assets, self.document_revision.get().max(1))?;
        thumbnail_for_raster_with_max(&flattened, maximum_dimension)
    }

    /// Quantizes visible composite colors into a replacement document palette.
    ///
    /// Success follows [`Core::replace_palette`] history/no-op semantics. Exceeding
    /// configured bounds fails before palette commit.
    pub fn generate_palette_from_document(
        &mut self,
        maximum_colors: usize,
        quantization_bits: u8,
    ) -> Result<DispatchOutcome, CoreError> {
        if maximum_colors == 0 || maximum_colors > inkpod_image::MAX_PALETTE_COLORS {
            return Err(CoreError::InvalidArgument(
                "generated palette color limit is invalid",
            ));
        }
        if quantization_bits > 7 {
            return Err(CoreError::InvalidArgument(
                "palette quantization must retain at least one bit",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let flattened =
            flatten_document(document, &self.assets, self.document_revision.get().max(1))?;
        let mask = u8::MAX << quantization_bits;
        let mut unique = BTreeSet::new();
        for y in 0..flattened.height() {
            for x in 0..flattened.width() {
                let PixelValue::Rgba(mut rgba) = flattened.pixel(x, y)? else {
                    return Err(CoreError::InvalidState(
                        "flattened palette source is not RGBA8",
                    ));
                };
                if rgba[3] == 0 {
                    continue;
                }
                rgba[0] &= mask;
                rgba[1] &= mask;
                rgba[2] &= mask;
                rgba[3] &= mask;
                unique.insert(rgba);
                if unique.len() > maximum_colors {
                    return Err(CoreError::InvalidState(
                        "generated palette exceeds the configured maximum; increase quantization",
                    ));
                }
            }
        }
        let colors = unique.into_iter().map(PixelValue::Rgba).collect::<Vec<_>>();
        self.replace_palette(&colors)
    }

    /// Replaces paper/frame metadata as one undoable document edit.
    ///
    /// All frames and margins are validated against document dimensions first.
    pub fn update_paper_frames(
        &mut self,
        frames: FrameMetadata,
    ) -> Result<DispatchOutcome, CoreError> {
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::UpdatePaperFrames { frames })
                .map(|result| result.dispatch);
        }
        self.ensure_no_active_stroke()?;
        validate_frames(self.document.as_ref().ok_or(CoreError::NoDocument)?, frames)?;
        let mut edit = self.begin_document_edit()?;
        edit.working_mut().frames = frames;
        edit.commit(self)
    }
}
