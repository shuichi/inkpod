//! Read-only decoded images and their independent view/snapshot state.

use crate::*;
use inkpod_format::CommonRaster;

pub(super) enum ReferencePixels {
    Memory(CommonRaster),
    Managed(inkpod_io::LoadedImage),
}

pub(super) struct ReferenceImage {
    pixels: ReferencePixels,
    revision: RenderRevision,
}

impl ReferenceImage {
    pub(super) fn memory(raster: CommonRaster, revision: u64) -> Result<Self, CoreError> {
        raster.validate()?;
        if revision == 0 {
            return Err(CoreError::InvalidArgument("reference revision is zero"));
        }
        Ok(Self {
            pixels: ReferencePixels::Memory(raster),
            revision: RenderRevision::from_raw(revision),
        })
    }

    pub(super) fn managed(image: inkpod_io::LoadedImage, revision: u64) -> Result<Self, CoreError> {
        image.raster().validate()?;
        if revision == 0 {
            return Err(CoreError::InvalidArgument("reference revision is zero"));
        }
        Ok(Self {
            pixels: ReferencePixels::Managed(image),
            revision: RenderRevision::from_raw(revision),
        })
    }

    fn raster(&self) -> &CommonRaster {
        match &self.pixels {
            ReferencePixels::Memory(raster) => raster,
            ReferencePixels::Managed(image) => image.raster(),
        }
    }

    fn reserve_pixels(&self, bytes: u64) -> Result<Option<inkpod_io::DecodedLease>, CoreError> {
        match &self.pixels {
            ReferencePixels::Memory(_) => Ok(None),
            ReferencePixels::Managed(image) => {
                image.reserve_derived(bytes).map(Some).map_err(Into::into)
            }
        }
    }

    pub(super) fn size(&self) -> DocumentSizeU32 {
        let raster = self.raster();
        DocumentSizeU32::new(raster.info.width, raster.info.height)
    }

    fn pixel(&self, x: u32, y: u32) -> Result<PixelValue, CoreError> {
        let raster = self.raster();
        if x >= raster.info.width || y >= raster.info.height {
            return Err(CoreError::InvalidArgument(
                "reference pixel is outside source bounds",
            ));
        }
        let channels = raster.info.pixel_format.bytes_per_pixel();
        let index = (u64::from(y) * u64::from(raster.info.width) + u64::from(x))
            .checked_mul(channels as u64)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or(CoreError::InvalidArgument(
                "reference pixel offset overflows",
            ))?;
        let end = index
            .checked_add(channels)
            .ok_or(CoreError::InvalidArgument("reference pixel end overflows"))?;
        let bytes = raster
            .pixels
            .get(index..end)
            .ok_or(CoreError::InvalidArgument(
                "reference pixel bytes are truncated",
            ))?;
        match raster.info.pixel_format {
            PixelFormat::StraightRgba8 => {
                Ok(PixelValue::Rgba([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            PixelFormat::StraightRgba16 => Ok(PixelValue::Rgba16([
                u16::from_le_bytes([bytes[0], bytes[1]]),
                u16::from_le_bytes([bytes[2], bytes[3]]),
                u16::from_le_bytes([bytes[4], bytes[5]]),
                u16::from_le_bytes([bytes[6], bytes[7]]),
            ])),
            _ => Err(CoreError::InvalidArgument(
                "reference raster format is unsupported",
            )),
        }
    }
}

pub(super) struct ReferenceView {
    state: ViewState,
    source: Option<(usize, RenderRevision)>,
    tiles: BTreeMap<TileCoord, Option<RenderTile>>,
}

impl ReferenceView {
    pub(super) fn new(viewport_width: f64, viewport_height: f64) -> Result<Self, CoreError> {
        let (state, _) = view::apply_view_state(
            ViewState::default(),
            ViewCommand::ViewportResized {
                viewport_width,
                viewport_height,
            },
            DocumentSizeU32::new(1, 1),
        )?;
        Ok(Self {
            state,
            source: None,
            tiles: BTreeMap::new(),
        })
    }

    pub(super) fn fitted(
        size: DocumentSizeU32,
        viewport_width: f64,
        viewport_height: f64,
    ) -> Result<Self, CoreError> {
        let mut view = Self::new(viewport_width, viewport_height)?;
        view.apply(
            ViewCommand::Fit {
                viewport_width,
                viewport_height,
            },
            size,
        )?;
        Ok(view)
    }

    pub(super) fn apply(
        &mut self,
        command: ViewCommand,
        size: DocumentSizeU32,
    ) -> Result<ViewState, CoreError> {
        let (state, invalidate) = view::apply_view_state(self.state, command, size)?;
        self.state = state;
        if invalidate {
            self.tiles.clear();
        }
        Ok(state)
    }

    pub(super) fn prepare_selection(
        &self,
        index: usize,
        image: &ReferenceImage,
        viewport_width: f64,
        viewport_height: f64,
    ) -> Result<Self, CoreError> {
        let mut candidate = Self {
            state: self.state,
            source: None,
            tiles: BTreeMap::new(),
        };
        candidate.apply(
            ViewCommand::Fit {
                viewport_width,
                viewport_height,
            },
            image.size(),
        )?;
        // The current view and any renderer snapshot keep their pixels until the
        // complete candidate display fits. Failure drops only candidate leases.
        candidate.snapshot(index, image)?;
        Ok(candidate)
    }

    pub(super) fn sample(
        &self,
        image: &ReferenceImage,
        device_x: f64,
        device_y: f64,
    ) -> Result<PixelValue, CoreError> {
        let point = view::device_to_document(
            self.state,
            image.size(),
            DevicePointF64::new(device_x, device_y)
                .map_err(|_| CoreError::InvalidArgument("sample coordinate is invalid"))?,
        );
        let x = point.x.floor();
        let y = point.y.floor();
        if x < 0.0
            || y < 0.0
            || x >= f64::from(image.size().width)
            || y >= f64::from(image.size().height)
        {
            return Err(CoreError::InvalidArgument(
                "subpalette sample is outside source bounds",
            ));
        }
        image.pixel(x as u32, y as u32)
    }

    pub(super) fn snapshot(
        &mut self,
        index: usize,
        image: &ReferenceImage,
    ) -> Result<RenderSnapshot, CoreError> {
        let source = (index, image.revision);
        if self.source != Some(source) {
            self.source = Some(source);
            self.tiles.clear();
        }
        let size = image.size();
        let first = view::device_to_document(self.state, size, DevicePointF64::new(0.0, 0.0)?);
        let last = view::device_to_document(
            self.state,
            size,
            DevicePointF64::new(self.state.viewport.width, self.state.viewport.height)?,
        );
        let left = first
            .x
            .min(last.x)
            .floor()
            .clamp(0.0, f64::from(size.width)) as u32;
        let top = first
            .y
            .min(last.y)
            .floor()
            .clamp(0.0, f64::from(size.height)) as u32;
        let right = first.x.max(last.x).ceil().clamp(0.0, f64::from(size.width)) as u32;
        let bottom = first
            .y
            .max(last.y)
            .ceil()
            .clamp(0.0, f64::from(size.height)) as u32;
        let mut visible = Vec::new();
        if left < right && top < bottom {
            for y in top / TILE_SIZE..=(bottom - 1) / TILE_SIZE {
                for x in left / TILE_SIZE..=(right - 1) / TILE_SIZE {
                    visible.push(TileCoord { x, y });
                }
            }
        }
        // Release no-longer-visible cache owners before reserving new pixels.
        // Snapshots and individually cloned tiles retain their own shared charge.
        self.tiles.retain(|coord, _| {
            left < right
                && top < bottom
                && (left / TILE_SIZE..=(right - 1) / TILE_SIZE).contains(&coord.x)
                && (top / TILE_SIZE..=(bottom - 1) / TILE_SIZE).contains(&coord.y)
        });
        let mut tiles = Vec::new();
        for coord in visible {
            if let std::collections::btree_map::Entry::Vacant(entry) = self.tiles.entry(coord) {
                entry.insert(reference_tile(index, image, coord)?);
            }
            if let Some(Some(tile)) = self.tiles.get(&coord) {
                tiles.push(tile.clone());
            }
        }
        Ok(RenderSnapshot::reference(
            self.state,
            size,
            image.revision,
            tiles,
        ))
    }
}

fn reference_tile(
    index: usize,
    image: &ReferenceImage,
    coord: TileCoord,
) -> Result<Option<RenderTile>, CoreError> {
    let origin_x = coord
        .x
        .checked_mul(TILE_SIZE)
        .ok_or(CoreError::InvalidArgument("reference tile x overflows"))?;
    let origin_y = coord
        .y
        .checked_mul(TILE_SIZE)
        .ok_or(CoreError::InvalidArgument("reference tile y overflows"))?;
    let width = TILE_SIZE.min(image.size().width - origin_x);
    let height = TILE_SIZE.min(image.size().height - origin_y);
    let capacity = (u64::from(width) * u64::from(height) * 4) as usize;
    let lease = image.reserve_pixels(capacity as u64)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(capacity)
        .map_err(|_| CoreError::InvalidState("reference tile allocation failed"))?;
    if pixels.capacity() > capacity {
        return Err(CoreError::InvalidState(
            "reference tile allocation exceeded its reservation",
        ));
    }
    let mut visible = false;
    for y in 0..height {
        for x in 0..width {
            let rgba = snapshot::rgba8_for_display(image.pixel(origin_x + x, origin_y + y)?)
                .ok_or(CoreError::InvalidArgument(
                    "reference pixel format is unsupported",
                ))?;
            let alpha = u32::from(rgba[3]);
            visible |= alpha != 0;
            let premultiply = |channel: u8| ((u32::from(channel) * alpha + 127) / 255) as u8;
            pixels.extend_from_slice(&[
                premultiply(rgba[2]),
                premultiply(rgba[1]),
                premultiply(rgba[0]),
                rgba[3],
            ]);
        }
    }
    if !visible {
        return Ok(None);
    }
    let index = u16::try_from(index)
        .map_err(|_| CoreError::InvalidArgument("reference image index overflows"))?;
    let x = u16::try_from(coord.x)
        .map_err(|_| CoreError::InvalidArgument("reference tile x overflows"))?;
    let y = u16::try_from(coord.y)
        .map_err(|_| CoreError::InvalidArgument("reference tile y overflows"))?;
    let tile_id = (1_u64 << 62) | (u64::from(index) << 32) | (u64::from(y) << 16) | u64::from(x);
    // Arc<[u8]>::from(Vec<u8>) temporarily retains both pixel allocations.
    // Reserve that second allocation before conversion, then release only the
    // transient charge. The tile owns the remaining charge, including clones.
    let transient = image.reserve_pixels(capacity as u64)?;
    let tile = RenderTile::reference(
        tile_id,
        image.revision,
        DocumentPointI32 {
            x: origin_x as i32,
            y: origin_y as i32,
        },
        DocumentSizeU32::new(width, height),
        pixels,
        lease,
    )?;
    drop(transient);
    Ok(Some(tile))
}
