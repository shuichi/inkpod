use super::raster::{common_to_tile_raster, thumbnail_allocation_bytes, thumbnail_for_raster};
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Small owned straight-alpha RGBA8 preview of a sequence cell.
pub struct Thumbnail {
    /// Preview width in pixels.
    pub width: u32,
    /// Preview height in pixels.
    pub height: u32,
    /// Tightly packed top-to-bottom straight-alpha RGBA8 bytes.
    pub rgba8: Vec<u8>,
    /// Deterministic checksum of `rgba8` and geometry.
    pub checksum: u64,
}

#[derive(Clone, Debug)]
/// Validated immutable flattened source for one sequence cell.
pub struct SequenceCellSource {
    /// User-visible name containing a parseable trailing cell number.
    pub name: String,
    /// Parsed cell number used for natural ordering and navigation.
    pub cell_number: u32,
    /// Persistent nonzero document UUID.
    pub document_uuid: u128,
    /// Nonzero generation of the immutable raster payload owned by Core.
    pub source_generation: u64,
    /// Horizontal resolution in thousandths of a DPI.
    pub dpi_x_milli: u32,
    /// Vertical resolution in thousandths of a DPI.
    pub dpi_y_milli: u32,
    /// Paper/frame alignment metadata in source document pixels.
    pub frames: FrameMetadata,
    /// Raster companion format retained when this source becomes editable.
    pub raster_file_format: CommonRasterFormat,
    // Runtime-only charge for the tiled image and thumbnail; not replay identity.
    decoded_lease: Option<inkpod_io::DecodedLease>,
    pub(super) thumbnail: Arc<Thumbnail>,
    pub(crate) raster: TileRaster,
}

impl PartialEq for SequenceCellSource {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.cell_number == other.cell_number
            && self.document_uuid == other.document_uuid
            && self.source_generation == other.source_generation
            && self.dpi_x_milli == other.dpi_x_milli
            && self.dpi_y_milli == other.dpi_y_milli
            && self.frames == other.frames
            && self.raster_file_format == other.raster_file_format
            && self.raster == other.raster
    }
}

impl Eq for SequenceCellSource {}

impl SequenceCellSource {
    pub(crate) fn encode_raster(
        &self,
        format: CommonRasterFormat,
        composite_white: bool,
    ) -> Result<Vec<u8>, CoreError> {
        let raster = super::raster::tile_to_common(
            &self.raster,
            Some(self.dpi_x_milli),
            Some(self.dpi_y_milli),
        )?;
        Ok(encode_common_raster(format, &raster, composite_white)?)
    }
    /// Copies a managed source into the sequence's immutable tiled representation.
    /// Reserves its full tile and thumbnail payloads before allocation and retains that charge
    /// across clones, replacement, and cache invalidation.
    pub fn from_loaded_image(
        manager: &inkpod_io::IoManager,
        image: &inkpod_io::LoadedImage,
        document_uuid: u128,
    ) -> Result<Self, CoreError> {
        let info = image.raster().info;
        let tile = u64::from(inkpod_image::TILE_SIZE);
        let bytes_per_pixel = match info.pixel_format {
            PixelFormat::StraightRgba8 => 4_u64,
            PixelFormat::StraightRgba16 => 8_u64,
            _ => return Err(CoreError::InvalidArgument("sequence requires RGBA pixels")),
        };
        let bytes = u64::from(info.width)
            .div_ceil(tile)
            .checked_mul(u64::from(info.height).div_ceil(tile))
            .and_then(|count| count.checked_mul(tile * tile * bytes_per_pixel))
            .and_then(|bytes| {
                bytes.checked_add(thumbnail_allocation_bytes(info.width, info.height))
            })
            .ok_or(CoreError::InvalidArgument(
                "sequence tile allocation overflows",
            ))?;
        let lease = manager.reserve_derived_image(image, bytes)?;
        let mut source = Self::from_common_raster_with_generation(
            image.name(),
            document_uuid,
            image.generation(),
            image.raster(),
        )?;
        source.raster_file_format = image.format();
        source.decoded_lease = Some(lease);
        Ok(source)
    }
    /// Validates and converts owned tightly packed raster bytes into a sequence cell.
    pub fn from_rgba_bytes(
        name: impl Into<String>,
        document_uuid: u128,
        raster: RgbaRasterBytes,
    ) -> Result<Self, CoreError> {
        let raster = CommonRaster::new(
            raster.width,
            raster.height,
            raster.pixel_format,
            raster.dpi_x_milli,
            raster.dpi_y_milli,
            raster.pixels,
        )?;
        Self::from_common_raster(name, document_uuid, &raster)
    }

    /// Validates and copies a common raster into a sequence cell.
    ///
    /// The name must contain a cell number and UUID must be nonzero.
    pub fn from_common_raster(
        name: impl Into<String>,
        document_uuid: u128,
        raster: &CommonRaster,
    ) -> Result<Self, CoreError> {
        Self::from_common_raster_with_generation(name, document_uuid, 1, raster)
    }

    /// Validates and copies a common raster with an explicit immutable source generation.
    ///
    /// The name must contain a cell number, and both UUID and generation must be nonzero.
    pub fn from_common_raster_with_generation(
        name: impl Into<String>,
        document_uuid: u128,
        source_generation: u64,
        raster: &CommonRaster,
    ) -> Result<Self, CoreError> {
        let name = name.into();
        validate_node_name(&name)?;
        let cell_number = parse_cell_number(&name).ok_or(CoreError::InvalidArgument(
            "sequence name has no cell number",
        ))?;
        if document_uuid == 0 || source_generation == 0 {
            return Err(CoreError::InvalidArgument(
                "sequence document UUID and source generation must be nonzero",
            ));
        }
        let width = raster.info.width;
        let height = raster.info.height;
        let dpi_x_milli = raster.info.dpi_x_milli.unwrap_or(DEFAULT_DPI_MILLI);
        let dpi_y_milli = raster.info.dpi_y_milli.unwrap_or(DEFAULT_DPI_MILLI);
        let reference_frame = RectI32 {
            x: (width / 2) as i32,
            y: (height / 2) as i32,
            width: width as i32,
            height: height as i32,
        };
        let full = RectI32 {
            x: 0,
            y: 0,
            width: width as i32,
            height: height as i32,
        };
        let raster = common_to_tile_raster(raster, 1)?;
        let thumbnail = Arc::new(thumbnail_for_raster(&raster)?);
        Ok(Self {
            name,
            cell_number,
            document_uuid,
            source_generation,
            raster_file_format: CommonRasterFormat::Png,
            decoded_lease: None,
            thumbnail,
            dpi_x_milli,
            dpi_y_milli,
            frames: FrameMetadata {
                hundred_frame: full,
                reference_frame,
                drawing_frame: full,
                safe_frame: full,
                shooting_frame: full,
                maximum_close_frame: full,
                margins: Margins::default(),
            },
            raster,
        })
    }

    /// Copies the bounded aspect-preserving thumbnail prepared with this source.
    /// Cloned sources share the cached pixels; querying never resamples the raster.
    pub fn thumbnail(&self) -> Result<Thumbnail, CoreError> {
        Ok(self.thumbnail.as_ref().clone())
    }

    pub(crate) fn reserve_render_payload(
        &self,
        bytes: u64,
    ) -> Result<Option<inkpod_io::DecodedLease>, CoreError> {
        self.decoded_lease
            .as_ref()
            .map(|lease| lease.reserve_sequence_render(bytes))
            .transpose()
            .map_err(CoreError::from)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Lightweight runtime catalog state for sequence-pane invalidation.
/// Runtime owner generation is not serialized and has no replay meaning.
pub struct SequenceCatalogInfo {
    /// Sequence-only revision, or zero when no catalog is installed.
    pub revision: u64,
    /// Nonzero runtime namespace; zero disables retained frontend cache reuse.
    pub owner_generation: u64,
    /// Number of validated, naturally ordered sources, at most 10,000.
    pub cell_count: u32,
    /// Bound active source index, absent for an unbound or missing catalog.
    pub active_index: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Borrowed sequence metadata. No raster or thumbnail pixel copy is performed.
/// The name is valid while the source catalog is immutably borrowed.
pub struct SequenceCellMetadata<'a> {
    /// User-visible source name in natural sequence order.
    pub name: &'a str,
    /// Parsed cell number.
    pub cell_number: u32,
    /// Persistent nonzero source document UUID.
    pub document_uuid: u128,
    /// Immutable source payload generation.
    pub source_generation: u64,
    /// Full-resolution source width in document pixels.
    pub width: u32,
    /// Full-resolution source height in document pixels.
    pub height: u32,
    /// Cached preview width in pixels, at most 64.
    pub thumbnail_width: u32,
    /// Cached preview height in pixels, at most 64.
    pub thumbnail_height: u32,
    /// Unchanged checksum of the cached preview bytes.
    pub thumbnail_checksum: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Public sequence-cell metadata with an owned thumbnail.
pub struct SequenceCellInfo {
    /// User-visible source name.
    pub name: String,
    /// Parsed cell number.
    pub cell_number: u32,
    /// Persistent document UUID.
    pub document_uuid: u128,
    /// Generation of the immutable source raster.
    pub source_generation: u64,
    /// Source width in pixels.
    pub width: u32,
    /// Source height in pixels.
    pub height: u32,
    /// Bounded preview image.
    pub thumbnail: Thumbnail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SequenceState {
    pub(crate) cells: Vec<SequenceCellSource>,
    pub(super) active_index: Option<usize>,
    pub(crate) revision: u64,
}

impl SequenceState {
    pub(crate) fn thumbnail_cache_bytes(&self) -> u64 {
        self.cells.iter().fold(0_u64, |bytes, cell| {
            bytes.saturating_add(cell.thumbnail.rgba8.len() as u64)
        })
    }

    pub(crate) fn logical_raster_usage(&self) -> (u64, u64) {
        self.cells
            .iter()
            .fold((0_u64, 0_u64), |(tiles, bytes), cell| {
                (
                    tiles.saturating_add(cell.raster.allocated_tile_count() as u64),
                    bytes.saturating_add(cell.raster.allocated_tile_bytes()),
                )
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Relative direction used for sequence navigation.
pub enum SequenceDirection {
    /// Selects the preceding cell in natural order.
    Previous,
    /// Selects the following cell in natural order.
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
/// Application-selected behavior when normal sequence navigation reaches an endpoint.
///
/// This policy is supplied by the frontend for each request. It is not document or
/// editor state and does not affect history, dirty state, savepoints, or replay.
pub enum SequenceEndpointPolicy {
    /// Keep the active cell unchanged at the first or last existing entry.
    Stop = 1,
    /// Move from the first entry to the last, or from the last entry to the first.
    Wrap = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
/// Semantic result of resolving one previous/next sequence command.
pub enum SequenceStepResult {
    /// No sequence is configured, so there is no target.
    Empty = 1,
    /// The sequence contains only the already-active cell.
    SingleCell = 2,
    /// Stop policy retained the active endpoint cell.
    Stopped = 3,
    /// Navigation selected the adjacent existing entry in natural order.
    Advanced = 4,
    /// Wrap policy selected the opposite endpoint.
    Wrapped = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Immutable issue-time resolution of one normal sequence navigation command.
///
/// The request records sequence revision and both immutable cell identities. Commit
/// re-resolves the same direction and policy and rejects stale sequence, source, or
/// target state without changing the document. Missing cell numbers are skipped by
/// natural order and remain observable through the source/target cell numbers.
pub struct SequenceStepPlan {
    /// Requested relative direction.
    pub direction: SequenceDirection,
    /// Endpoint behavior captured when the command was issued.
    pub endpoint_policy: SequenceEndpointPolicy,
    /// Semantic resolution class.
    pub result: SequenceStepResult,
    /// Sequence-only revision, or zero when no sequence is configured.
    pub sequence_revision: u64,
    /// Zero-based natural-order source index when an active sequence exists.
    pub source_index: Option<u32>,
    /// Zero-based natural-order target index when a sequence target exists.
    pub target_index: Option<u32>,
    /// Persistent source document UUID.
    pub source_document_uuid: Option<u128>,
    /// Immutable source payload generation.
    pub source_generation: Option<u64>,
    /// Persistent target document UUID.
    pub target_document_uuid: Option<u128>,
    /// Immutable target payload generation.
    pub target_generation: Option<u64>,
    /// Parsed source cell number.
    pub source_cell_number: Option<u32>,
    /// Parsed target cell number.
    pub target_cell_number: Option<u32>,
}

impl SequenceStepPlan {
    /// Reports whether committing this plan replaces the active document.
    #[must_use]
    pub const fn requires_switch(self) -> bool {
        matches!(
            self.result,
            SequenceStepResult::Advanced | SequenceStepResult::Wrapped
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
/// User-selected dirty-cell policy captured in an immutable sequence switch request.
pub enum SequenceSwitchPolicy {
    /// The frontend must obtain confirmation and complete a normal save before switching.
    Prompt = 1,
    /// The frontend must durably write recovery data before committing the switch.
    AutosaveBeforeSwitch = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Immutable source, target, and revision token for an asynchronous sequence switch.
///
/// UUID and source-generation pairs belong to the configured sequence for the
/// lifetime of that sequence. A commit succeeds only while the active source,
/// target entry, and document revision still match this request. Creating this
/// value and rejected commits do not change document, history, dirty, savepoint,
/// path, or sequence state.
pub struct SequenceSwitchRequest {
    /// Policy selected when the command was issued.
    pub policy: SequenceSwitchPolicy,
    /// Persistent UUID of the source document being protected.
    pub source_document_uuid: u128,
    /// Immutable source-raster generation of the source sequence entry.
    pub source_generation: u64,
    /// Document revision that must still be active at commit time.
    pub source_document_revision: u64,
    /// Independent EditorState revision that must still be active at commit time.
    pub source_editor_revision: u64,
    /// Persistent UUID of the requested target document.
    pub target_document_uuid: u128,
    /// Immutable source-raster generation of the target sequence entry.
    pub target_source_generation: u64,
    /// Zero-based natural-order target index.
    pub target_index: u32,
    /// Whether the source's exact journal/editor generation must be written to
    /// recovery before this switch can discard the active in-memory session.
    pub source_recovery_required: bool,
}

impl SequenceSwitchRequest {
    /// Reports whether this request would change the active sequence entry.
    #[must_use]
    pub const fn requires_switch(self) -> bool {
        self.source_document_uuid != self.target_document_uuid
            || self.source_generation != self.target_source_generation
    }

    /// Reports the issue-time, Core-derived preservation requirement.
    #[must_use]
    pub const fn requires_source_recovery(self) -> bool {
        self.source_recovery_required
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Playback and composition settings for motion check.
pub struct MotionCheckConfig {
    /// Playback rate in frames per second.
    pub fps: u32,
    /// Whether playback wraps at sequence ends.
    pub loop_playback: bool,
    /// Whether selection visualization is included.
    pub include_selection: bool,
    /// Whether light-table compositing is included.
    pub include_light_table: bool,
}

impl Default for MotionCheckConfig {
    fn default() -> Self {
        Self {
            fps: 24,
            loop_playback: true,
            include_selection: false,
            include_light_table: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One immutable frame returned by motion-check playback.
pub struct MotionFrame {
    /// Zero-based index in natural sequence order.
    pub sequence_index: usize,
    /// Parsed source cell number.
    pub cell_number: u32,
    /// User-visible source name.
    pub name: String,
    /// Owned bounded preview for this frame.
    pub thumbnail: Thumbnail,
    /// Current paused state.
    pub paused: bool,
    /// Configured frames per second.
    pub fps: u32,
    /// Whether selection visualization is included.
    pub include_selection: bool,
    /// Whether light-table compositing is included.
    pub include_light_table: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MotionCheckState {
    pub(super) config: MotionCheckConfig,
    pub(super) index: usize,
    pub(super) paused: bool,
}
