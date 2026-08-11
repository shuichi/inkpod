use super::coordinates::*;
use crate::selection::mask_bounds;
use crate::*;

impl Core {
    /// Creates a secondary view initialized from the primary view.
    ///
    /// The returned stable view ID remains valid until [`Core::close_view`] or a
    /// document replacement. Document revision, history, and dirty state are unchanged.
    pub fn create_view(&mut self) -> Result<u64, CoreError> {
        if self.document.is_none() {
            return Err(CoreError::NoDocument);
        }
        let id = self.next_view_id;
        self.next_view_id = self
            .next_view_id
            .checked_next()
            .ok_or(CoreError::InvalidState("view ID overflow"))?;
        self.secondary_views.insert(id, self.view);
        Ok(id.get())
    }

    /// Closes a secondary view identified by a Core-local stable ID.
    pub fn close_view(&mut self, view_id: u64) -> Result<(), CoreError> {
        let view_id = ViewId::from_raw(view_id);
        self.secondary_views
            .remove(&view_id)
            .map(|_| ())
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))
    }

    /// Applies a view-only command to one secondary view.
    ///
    /// Invalid input leaves both primary and secondary views unchanged. Success
    /// never changes document revision, history, or dirty state.
    pub fn apply_view_for(
        &mut self,
        view_id: u64,
        command: ViewCommand,
    ) -> Result<ViewState, CoreError> {
        let view_id = ViewId::from_raw(view_id);
        let original = self.view;
        self.view = *self
            .secondary_views
            .get(&view_id)
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let result = self.apply_view(command);
        let updated = self.view;
        self.view = original;
        if result.is_ok() {
            self.secondary_views.insert(view_id, updated);
        }
        result.map(|_| updated)
    }

    /// Builds an immutable snapshot using one secondary view transform.
    ///
    /// The primary view is restored before return and document state is not changed.
    pub fn build_snapshot_for(&mut self, view_id: u64) -> Result<RenderSnapshot, CoreError> {
        let view_id = ViewId::from_raw(view_id);
        let selected = *self
            .secondary_views
            .get(&view_id)
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let original = self.view;
        self.view = selected;
        let snapshot = self.build_snapshot();
        self.view = original;
        Ok(snapshot)
    }

    /// Applies a view-only command using the registered subpalette cell bounds.
    ///
    /// The selected secondary view is independent from document views. The
    /// operation never changes the active document, history, dirty state, or
    /// subpalette source, and is safe while an edit stroke is active because it
    /// cannot affect the editable Canvas transform.
    pub fn apply_subpalette_view_for(
        &mut self,
        view_id: u64,
        command: ViewCommand,
    ) -> Result<ViewState, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = self
            .subpalette_index
            .ok_or(CoreError::InvalidState("subpalette has no registered cell"))?;
        let cell = sequence
            .cells
            .get(index)
            .ok_or(CoreError::InvalidState("subpalette source disappeared"))?;
        let document_size = DocumentSizeU32::new(cell.raster.width(), cell.raster.height());
        let view_id = ViewId::from_raw(view_id);
        let original = self.view;
        self.view = *self
            .secondary_views
            .get(&view_id)
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let result = self.apply_view_for_document_size(command, document_size);
        let updated = self.view;
        self.view = original;
        if result.is_ok() {
            self.secondary_views.insert(view_id, updated);
        }
        result.map(|_| updated)
    }

    /// Samples the registered subpalette source through an independent view.
    ///
    /// Device coordinates use the same half-open pixel-cell and flip rules as
    /// editable Canvas locator sampling. The source and all document state remain
    /// unchanged.
    pub fn subpalette_view_sample(
        &self,
        view_id: u64,
        device_x: f64,
        device_y: f64,
    ) -> Result<PixelValue, CoreError> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(CoreError::InvalidState("no sequence is configured"))?;
        let index = self
            .subpalette_index
            .ok_or(CoreError::InvalidState("subpalette has no registered cell"))?;
        let cell = sequence
            .cells
            .get(index)
            .ok_or(CoreError::InvalidState("subpalette source disappeared"))?;
        let view = *self
            .secondary_views
            .get(&ViewId::from_raw(view_id))
            .ok_or(CoreError::InvalidArgument("view ID does not exist"))?;
        let point = device_to_document(
            view,
            DocumentSizeU32::new(cell.raster.width(), cell.raster.height()),
            DevicePointF64::new(device_x, device_y)
                .map_err(|_| CoreError::InvalidArgument("sample coordinate is invalid"))?,
        );
        let x = point.x.floor();
        let y = point.y.floor();
        if x < 0.0
            || y < 0.0
            || x >= f64::from(cell.raster.width())
            || y >= f64::from(cell.raster.height())
        {
            return Err(CoreError::InvalidArgument(
                "subpalette sample is outside source bounds",
            ));
        }
        Ok(cell.raster.pixel(x as u32, y as u32)?)
    }

    /// Resolves a device-pixel point through the primary or selected secondary view.
    ///
    /// The returned document cell uses floor semantics and half-open document bounds.
    /// Active stroke and filter previews are sampled in the same priority as rendering.
    /// Sampling is read-only and does not affect revisions, history, or dirty state.
    pub fn locator_sample(
        &self,
        view_id: Option<u64>,
        device_x: f64,
        device_y: f64,
    ) -> Result<LocatorSample, CoreError> {
        let document = self.locator_document()?;
        let view = match view_id.map(ViewId::from_raw) {
            Some(id) => *self
                .secondary_views
                .get(&id)
                .ok_or(CoreError::InvalidArgument("view ID does not exist"))?,
            None => self.view,
        };
        let device_point = DevicePointF64::new(device_x, device_y)?;
        let point = device_to_document(
            view,
            DocumentSizeU32::new(document.width, document.height),
            device_point,
        );
        let document_x = point.x.floor() as i32;
        let document_y = point.y.floor() as i32;
        let color = if document_x >= 0
            && document_y >= 0
            && document_x < document.width as i32
            && document_y < document.height as i32
        {
            self.eyedropper_in_document(
                document,
                EyedropperSource::Composite,
                document_x as u32,
                document_y as u32,
            )
            .ok()
        } else {
            None
        };
        Ok(LocatorSample {
            document_x,
            document_y,
            selection_bounds: mask_bounds(&document.selection)?,
            color,
        })
    }

    /// Samples a bounded composite-color neighborhood around one device point.
    ///
    /// The output always has `(radius * 2 + 1)` pixels on each side. Pixels
    /// outside the half-open document bounds are transparent. Active stroke and
    /// filter previews are sampled in the same priority as rendering. Sampling
    /// is read-only and does not allocate more than a 33 by 33 RGBA8 buffer.
    pub fn locator_neighborhood(
        &self,
        view_id: Option<u64>,
        device_x: f64,
        device_y: f64,
        radius: u32,
    ) -> Result<LocatorNeighborhood, CoreError> {
        const MAX_RADIUS: u32 = 16;
        if radius > MAX_RADIUS {
            return Err(CoreError::InvalidArgument("locator radius exceeds maximum"));
        }
        let document = self.locator_document()?;
        let center = self.locator_sample(view_id, device_x, device_y)?;
        let radius_i32 = i32::try_from(radius)
            .map_err(|_| CoreError::InvalidArgument("locator radius is invalid"))?;
        let origin_x =
            center
                .document_x
                .checked_sub(radius_i32)
                .ok_or(CoreError::InvalidArgument(
                    "locator x-coordinate is out of range",
                ))?;
        let origin_y =
            center
                .document_y
                .checked_sub(radius_i32)
                .ok_or(CoreError::InvalidArgument(
                    "locator y-coordinate is out of range",
                ))?;
        let side = radius
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(CoreError::InvalidArgument("locator dimensions overflow"))?;
        let byte_count = usize::try_from(side)
            .ok()
            .and_then(|value| value.checked_mul(value))
            .and_then(|value| value.checked_mul(4))
            .ok_or(CoreError::InvalidArgument("locator byte count overflow"))?;
        let mut pixels_rgba8 = vec![0_u8; byte_count];
        for row in 0..side {
            for column in 0..side {
                let x = origin_x + column as i32;
                let y = origin_y + row as i32;
                if x < 0 || y < 0 || x >= document.width as i32 || y >= document.height as i32 {
                    continue;
                }
                let color = self
                    .eyedropper_in_document(
                        document,
                        EyedropperSource::Composite,
                        x as u32,
                        y as u32,
                    )
                    .unwrap_or(PixelValue::Rgba([0; 4]));
                let rgba = match color {
                    PixelValue::Binary(value) | PixelValue::Grayscale8(value) => {
                        [value, value, value, u8::MAX]
                    }
                    PixelValue::Grayscale16(value) => {
                        let value = ((u32::from(value) + 128) / 257) as u8;
                        [value, value, value, u8::MAX]
                    }
                    PixelValue::Rgba(value) => value,
                    PixelValue::Rgba16(value) => {
                        value.map(|channel| ((u32::from(channel) + 128) / 257) as u8)
                    }
                };
                let offset = ((row as usize * side as usize) + column as usize) * 4;
                pixels_rgba8[offset..offset + 4].copy_from_slice(&rgba);
            }
        }
        Ok(LocatorNeighborhood {
            origin_x,
            origin_y,
            width: side,
            height: side,
            pixels_rgba8,
        })
    }

    fn locator_document(&self) -> Result<&CellDocument, CoreError> {
        self.active_stroke
            .as_ref()
            .map(|session| &session.preview_document)
            .or_else(|| {
                self.filter_preview
                    .as_ref()
                    .map(|session| &session.preview_document)
            })
            .or(self.document.as_ref())
            .ok_or(CoreError::NoDocument)
    }
}
