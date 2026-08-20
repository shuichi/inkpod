//! Transactional, region-scoped raster color replacement.

use super::*;
use crate::document::bounded_document_pixels;
use crate::primitive::CanonicalInvocation;
use crate::selection::{combine_selection_masks, mask_bounds, selection_mask_for_shape};

mod model;

pub use model::{ScopedColorReplaceMode, ScopedColorReplacePreview, ScopedColorReplaceRequest};

#[derive(Clone)]
struct EffectiveRegion {
    mask: Option<TileRaster>,
    bounds: Option<RectI32>,
}

impl EffectiveRegion {
    fn contains(&self, x: u32, y: u32) -> Result<bool, CoreError> {
        let Some(bounds) = self.bounds else {
            return Ok(false);
        };
        if x < bounds.x as u32
            || y < bounds.y as u32
            || x >= (bounds.x + bounds.width) as u32
            || y >= (bounds.y + bounds.height) as u32
        {
            return Ok(false);
        }
        match &self.mask {
            Some(mask) => Ok(mask.pixel(x, y)? == PixelValue::Binary(255)),
            None => Ok(true),
        }
    }
}

impl Core {
    /// Evaluates one scoped color replacement without changing document state.
    ///
    /// The request revision must equal the current document revision. The result
    /// counts exact native-depth raster pixels and
    /// can be discarded to cancel the interaction. No history, dirty state,
    /// savepoint, revision, or persistent ID changes are made.
    pub fn preview_scoped_color_replace(
        &self,
        request: &ScopedColorReplaceRequest,
    ) -> Result<ScopedColorReplacePreview, CoreError> {
        self.ensure_no_active_stroke()?;
        if request.base_document_revision != self.document_revision.get() {
            return Err(CoreError::InvalidState(
                "scoped color replacement base revision is stale",
            ));
        }
        self.preview_scoped_color_replace_arguments(
            request.plane_id,
            request.mode,
            request.target,
            request.replacement,
            request.region.as_ref(),
        )
    }

    /// Commits one exact scoped color replacement as a canonical undo unit.
    ///
    /// `base_document_revision` must still be current. Raster replacement compares
    /// native-depth values including alpha.
    /// A semantic no-op preserves revision, history, dirty state, savepoint, and IDs.
    pub fn apply_scoped_color_replace(
        &mut self,
        request: ScopedColorReplaceRequest,
    ) -> Result<DispatchOutcome, CoreError> {
        if request.base_document_revision != self.document_revision.get() {
            return Err(CoreError::InvalidState(
                "scoped color replacement base revision is stale",
            ));
        }
        if !self.canonical_invocation_is_active() {
            return self
                .execute_canonical_invocation(CanonicalInvocation::ScopedColorReplace {
                    plane_id: request.plane_id,
                    mode: request.mode,
                    target: request.target,
                    replacement: request.replacement,
                    region: request.region,
                })
                .map(|result| result.dispatch);
        }
        self.apply_scoped_color_replace_arguments(
            request.plane_id,
            request.mode,
            request.target,
            request.replacement,
            request.region.as_ref(),
        )
    }

    pub(crate) fn apply_scoped_color_replace_arguments(
        &mut self,
        plane_id: u64,
        mode: ScopedColorReplaceMode,
        target: PixelValue,
        replacement: PixelValue,
        region: Option<&SelectionShape>,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let mut edit = self.begin_document_edit()?;
        let revision = edit.revision().get();
        let (before, after) = edit.documents();
        validate_target(
            before,
            PlaneId::from_raw(plane_id),
            mode,
            target,
            replacement,
        )?;
        let effective = effective_region(before, PlaneId::from_raw(plane_id), region, revision)?;
        if target == replacement || effective.bounds.is_none() {
            return Ok(self.noop_outcome());
        }
        match mode {
            ScopedColorReplaceMode::RasterColor | ScopedColorReplaceMode::RasterMainLine => {
                let raster = &mut after
                    .plane_by_id_mut(PlaneId::from_raw(plane_id))
                    .ok_or(CoreError::InvalidState("scoped raster target disappeared"))?
                    .raster;
                let bounds = effective.bounds.expect("non-empty region was checked");
                let mut touched = BTreeSet::new();
                for y in bounds.y as u32..(bounds.y + bounds.height) as u32 {
                    for x in bounds.x as u32..(bounds.x + bounds.width) as u32 {
                        if effective.contains(x, y)? && raster.pixel(x, y)? == target {
                            raster.set_pixel(x, y, replacement, revision)?;
                            touched.insert(TileCoord {
                                x: x / TILE_SIZE,
                                y: y / TILE_SIZE,
                            });
                        }
                    }
                }
                for coord in touched {
                    raster.remove_tile_if_empty(coord);
                }
            }
        }
        edit.preserve_render_cache_by_raster_revision();
        edit.commit(self)
    }

    fn preview_scoped_color_replace_arguments(
        &self,
        plane_id: u64,
        mode: ScopedColorReplaceMode,
        target: PixelValue,
        replacement: PixelValue,
        region: Option<&SelectionShape>,
    ) -> Result<ScopedColorReplacePreview, CoreError> {
        let document = self.document.as_ref().ok_or(CoreError::NoDocument)?;
        let plane_id = PlaneId::from_raw(plane_id);
        validate_target(document, plane_id, mode, target, replacement)?;
        let effective = effective_region(document, plane_id, region, self.document_revision.get())?;
        if target == replacement || effective.bounds.is_none() {
            return Ok(ScopedColorReplacePreview {
                base_document_revision: self.document_revision.get(),
                matched_pixels: 0,
                affected_bounds: None,
            });
        }
        match mode {
            ScopedColorReplaceMode::RasterColor | ScopedColorReplaceMode::RasterMainLine => {
                let raster = &document
                    .plane_by_id(plane_id)
                    .ok_or(CoreError::InvalidState("scoped raster target disappeared"))?
                    .raster;
                let bounds = effective.bounds.expect("non-empty region was checked");
                let mut count = 0_u64;
                let mut affected = None;
                for y in bounds.y as u32..(bounds.y + bounds.height) as u32 {
                    for x in bounds.x as u32..(bounds.x + bounds.width) as u32 {
                        if effective.contains(x, y)? && raster.pixel(x, y)? == target {
                            count = count.checked_add(1).ok_or(CoreError::InvalidState(
                                "scoped color replacement match count overflow",
                            ))?;
                            affected = extend_bounds(affected, x, y)?;
                        }
                    }
                }
                Ok(ScopedColorReplacePreview {
                    base_document_revision: self.document_revision.get(),
                    matched_pixels: count,
                    affected_bounds: affected,
                })
            }
        }
    }
}

fn effective_region(
    document: &CellDocument,
    plane_id: PlaneId,
    region: Option<&SelectionShape>,
    revision: u64,
) -> Result<EffectiveRegion, CoreError> {
    bounded_document_pixels(document.width, document.height)?;
    validate_region(region)?;
    let region_mask = region
        .map(|shape| {
            selection_mask_for_shape(
                document,
                plane_id,
                shape,
                RangeInterpretation::Normal,
                SelectionConstructionOptions::default(),
                revision,
            )
        })
        .transpose()?;
    let has_selection = mask_bounds(&document.selection)?.is_some();
    let mask = match (region_mask, has_selection) {
        (Some(region_mask), true) => Some(combine_selection_masks(
            &document.selection,
            &region_mask,
            SelectionOperation::Intersect,
            revision,
        )?),
        (Some(region_mask), false) => Some(region_mask),
        (None, true) => Some(document.selection.clone()),
        (None, false) => None,
    };
    let bounds = match &mask {
        Some(mask) => mask_bounds(mask)?,
        None => Some(RectI32 {
            x: 0,
            y: 0,
            width: document.width as i32,
            height: document.height as i32,
        }),
    };
    Ok(EffectiveRegion { mask, bounds })
}

fn validate_region(region: Option<&SelectionShape>) -> Result<(), CoreError> {
    if region.is_some_and(|shape| {
        !matches!(
            shape,
            SelectionShape::Trace { .. }
                | SelectionShape::TraceBrush { .. }
                | SelectionShape::Rectangle(_)
                | SelectionShape::RectangleGesture { .. }
                | SelectionShape::Polyline(_)
                | SelectionShape::Lasso(_)
        )
    }) {
        Err(CoreError::InvalidArgument(
            "scoped color replacement region must be pen, rectangle, polyline, or lasso",
        ))
    } else {
        Ok(())
    }
}

fn validate_target(
    document: &CellDocument,
    plane_id: PlaneId,
    mode: ScopedColorReplaceMode,
    target: PixelValue,
    replacement: PixelValue,
) -> Result<(), CoreError> {
    let (layer, plane) = document
        .layers
        .iter()
        .find_map(|layer| {
            layer
                .planes
                .iter()
                .find(|plane| plane.id == plane_id)
                .map(|plane| (layer, plane))
        })
        .ok_or(CoreError::InvalidArgument(
            "scoped color replacement plane does not exist",
        ))?;
    if !layer.visible || !plane.visible {
        return Err(CoreError::InvalidState(
            "scoped color replacement target is hidden",
        ));
    }
    if !layer.editable || !plane.editable {
        return Err(CoreError::InvalidState(
            "scoped color replacement target is locked",
        ));
    }
    let mode_matches = match mode {
        ScopedColorReplaceMode::RasterColor => {
            matches!(plane.kind, PlaneType::Color | PlaneType::Raster)
        }
        ScopedColorReplaceMode::RasterMainLine => plane.kind == PlaneType::MainLine,
    };
    if !mode_matches {
        return Err(CoreError::InvalidArgument(
            "scoped color replacement mode does not match the target plane",
        ));
    }
    let color_matches = |color: PixelValue| match plane.raster.format() {
        PixelFormat::BinaryMask8 => matches!(color, PixelValue::Binary(_)),
        PixelFormat::Grayscale8 => matches!(color, PixelValue::Grayscale8(_)),
        PixelFormat::Grayscale16 => matches!(color, PixelValue::Grayscale16(_)),
        PixelFormat::StraightRgba8 => matches!(color, PixelValue::Rgba(_)),
        PixelFormat::StraightRgba16 => matches!(color, PixelValue::Rgba16(_)),
        PixelFormat::PremultipliedBgra8 => false,
    };
    if !color_matches(target) || !color_matches(replacement) {
        return Err(CoreError::InvalidArgument(
            "scoped color replacement colors do not match target depth",
        ));
    }
    Ok(())
}

fn extend_bounds(bounds: Option<RectI32>, x: u32, y: u32) -> Result<Option<RectI32>, CoreError> {
    let x = i32::try_from(x)
        .map_err(|_| CoreError::InvalidState("affected X coordinate is not representable"))?;
    let y = i32::try_from(y)
        .map_err(|_| CoreError::InvalidState("affected Y coordinate is not representable"))?;
    let Some(bounds) = bounds else {
        return Ok(Some(RectI32 {
            x,
            y,
            width: 1,
            height: 1,
        }));
    };
    let left = bounds.x.min(x);
    let top = bounds.y.min(y);
    let right = (bounds.x + bounds.width).max(x + 1);
    let bottom = (bounds.y + bounds.height).max(y + 1);
    Ok(Some(RectI32 {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }))
}
