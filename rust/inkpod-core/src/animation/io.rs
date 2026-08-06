use super::raster::*;
use super::*;
use crate::primitive::CanonicalInvocation;

impl Core {
    /// Decodes a common raster into an immutable Genesis asset and opens a clean cell.
    ///
    /// UUID must be nonzero. Success resets history, savepoint, view, sequence, and
    /// transient sessions; decode/validation failure leaves the current document intact.
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
        self.new_cell_from_raster_asset(input, dpi_x_milli, dpi_y_milli, document_uuid)
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
        document.base_surface = BaseSurface::Asset(record.id());
        assets = self.prepare_asset_store_for_session_reset(assets, &document)?;
        let revision = self.next_document_revision()?;
        self.cancel_stroke();
        self.filter_preview = None;
        self.last_filter = None;
        self.next_id = next_id;
        self.assets = assets;
        self.document = Some(document);
        self.document_revision = revision;
        self.render_cache.clear();
        self.reset_history(true);
        self.reset_view();
        self.current_path = None;
        self.recovered = false;
        self.color_check = None;
        self.secondary_views.clear();
        self.floating = None;
        self.motion_check = None;
        self.sequence = None;
        self.subpalette_index = None;
        self.reset_editor_state(true);
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
