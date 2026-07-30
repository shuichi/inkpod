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

    /// Resolves a device-pixel point through the primary or selected secondary view.
    ///
    /// The returned document cell uses floor semantics and half-open document bounds.
    /// Sampling is read-only.
    pub fn locator_sample(
        &self,
        view_id: Option<u64>,
        device_x: f64,
        device_y: f64,
    ) -> Result<LocatorSample, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
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
            self.eyedropper(
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
}
