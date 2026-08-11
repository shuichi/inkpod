//! Output-color guard scan and selection transaction.

use super::*;
use crate::animation::visit_visible_document_composite_rgba16;
use crate::primitive::{CanonicalInvocation, InvocationResult};
use crate::selection::{combine_selection_masks, selection_masks_have_same_coverage};

impl Core {
    /// Selects visible committed composite pixels outside the requested guard profile.
    ///
    /// The source pixels are never changed. A changed selection is one canonical
    /// transaction and Undo unit. A semantic no-op, stale base revision, scan
    /// failure, or cancellation leaves revision, history, dirty state, and IDs
    /// unchanged.
    pub fn select_output_color_guard(
        &mut self,
        profile: OutputColorGuardProfile,
        operation: SelectionOperation,
        base_revision: u64,
    ) -> Result<OutputColorGuardResult, CoreError> {
        self.select_output_color_guard_with_cancel(
            profile,
            operation,
            base_revision,
            |_completed, _total| true,
        )
    }

    /// Selects output-color guard failures with cooperative row progress and cancellation.
    ///
    /// The callback receives completed and total document rows. Returning `false`
    /// cancels before the transaction commit point and returns [`CoreError::Cancelled`].
    pub fn select_output_color_guard_with_cancel(
        &mut self,
        profile: OutputColorGuardProfile,
        operation: SelectionOperation,
        base_revision: u64,
        mut continue_progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<OutputColorGuardResult, CoreError> {
        if !self.canonical_invocation_is_active() {
            let mut summary = None;
            let result = self.execute_canonical_invocation_with(
                CanonicalInvocation::SelectOutputColorGuard {
                    profile,
                    operation,
                    base_revision,
                },
                |staged| {
                    let outcome = staged.select_output_color_guard_internal(
                        profile,
                        operation,
                        base_revision,
                        &mut continue_progress,
                    )?;
                    summary = Some(outcome.summary);
                    Ok(InvocationResult::dispatch(outcome.dispatch))
                },
            )?;
            return Ok(OutputColorGuardResult {
                dispatch: result.dispatch,
                summary: summary.ok_or(CoreError::InvalidState(
                    "output-color guard scan did not return its summary",
                ))?,
            });
        }
        self.select_output_color_guard_internal(
            profile,
            operation,
            base_revision,
            &mut continue_progress,
        )
    }

    fn select_output_color_guard_internal(
        &mut self,
        profile: OutputColorGuardProfile,
        operation: SelectionOperation,
        base_revision: u64,
        continue_progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<OutputColorGuardResult, CoreError> {
        self.ensure_no_active_stroke()?;
        if self.document_revision.get() != base_revision {
            return Err(CoreError::InvalidState(
                "output-color guard base revision is stale",
            ));
        }
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let revision = self.next_document_revision()?;
        let total_rows = u64::from(document.height);
        if !continue_progress(0, total_rows) {
            return Err(CoreError::Cancelled);
        }
        let mut candidate =
            TileRaster::new(document.width, document.height, PixelFormat::BinaryMask8)?;
        let mut summary = OutputColorGuardSummary::default();
        visit_visible_document_composite_rgba16(document, &self.assets, |x, y, pixel| {
            match profile {
                OutputColorGuardProfile::Bt709ConservativeYCbCr => {
                    match inkpod_image::bt709_conservative_guard_category(PixelValue::Rgba16(
                        pixel,
                    ))? {
                        inkpod_image::OutputColorGuardCategory::Transparent => {
                            summary.transparent_pixel_count =
                                summary.transparent_pixel_count.checked_add(1).ok_or(
                                    CoreError::InvalidState("output-color guard counter overflow"),
                                )?;
                        }
                        inkpod_image::OutputColorGuardCategory::Safe => {
                            summary.scanned_pixel_count =
                                summary.scanned_pixel_count.checked_add(1).ok_or(
                                    CoreError::InvalidState("output-color guard counter overflow"),
                                )?;
                        }
                        inkpod_image::OutputColorGuardCategory::Outside => {
                            summary.scanned_pixel_count =
                                summary.scanned_pixel_count.checked_add(1).ok_or(
                                    CoreError::InvalidState("output-color guard counter overflow"),
                                )?;
                            summary.selected_pixel_count =
                                summary.selected_pixel_count.checked_add(1).ok_or(
                                    CoreError::InvalidState("output-color guard counter overflow"),
                                )?;
                            candidate.set_pixel(
                                x,
                                y,
                                PixelValue::Binary(u8::MAX),
                                revision.get(),
                            )?;
                        }
                    }
                }
            }
            if x + 1 == document.width && !continue_progress(u64::from(y) + 1, total_rows) {
                return Err(CoreError::Cancelled);
            }
            Ok(())
        })?;

        if self.document_revision.get() != base_revision {
            return Err(CoreError::InvalidState(
                "output-color guard base revision became stale",
            ));
        }
        let mut edit = self.begin_document_edit()?;
        let (before, after) = edit.documents();
        let combined =
            combine_selection_masks(&before.selection, &candidate, operation, revision.get())?;
        if selection_masks_have_same_coverage(&before.selection, &combined)? {
            drop(edit);
            return Ok(OutputColorGuardResult {
                dispatch: self.noop_outcome(),
                summary,
            });
        }
        after.selection = combined;
        let dispatch = edit.commit(self)?;
        Ok(OutputColorGuardResult { dispatch, summary })
    }
}
