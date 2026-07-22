use super::{
    Adjustment, AirbrushStroke, BoundaryAirbrush, CellDocument, Core, CoreError, DispatchOutcome,
    Filter, Gradient, LayerKind, LayerNode, PixelFormat, PlaneType, Stamp,
};
use inkpod_image::{
    TileRaster, apply_airbrush, apply_boundary_airbrush, apply_filter, apply_gradient, apply_stamp,
    edit_alpha,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilterPreviewInfo {
    pub plane_id: u64,
    pub base_checksum: u64,
    pub preview_checksum: u64,
    pub preview_revision: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct FilterPreview {
    pub(crate) plane_id: u64,
    pub(crate) base_document: CellDocument,
    pub(crate) preview_document: CellDocument,
    pub(crate) filter: Filter,
    pub(crate) preview_revision: u64,
}

impl Core {
    pub fn begin_filter_preview(
        &mut self,
        plane_id: u64,
        filter: Filter,
    ) -> Result<FilterPreviewInfo, CoreError> {
        self.ensure_no_active_stroke()?;
        let base_document = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let preview_revision = self.allocate_preview_revision()?;
        let preview_document =
            filter_document(&base_document, plane_id, &filter, preview_revision)?;
        let info = preview_info(
            plane_id,
            &base_document,
            &preview_document,
            preview_revision,
        )?;
        self.filter_preview = Some(FilterPreview {
            plane_id,
            base_document,
            preview_document,
            filter,
            preview_revision,
        });
        self.render_cache.clear();
        Ok(info)
    }

    pub fn update_filter_preview(
        &mut self,
        plane_id: u64,
        filter: Filter,
    ) -> Result<FilterPreviewInfo, CoreError> {
        let (active_plane_id, base_document) = self
            .filter_preview
            .as_ref()
            .map(|preview| (preview.plane_id, preview.base_document.clone()))
            .ok_or(CoreError::InvalidState("there is no active filter preview"))?;
        if plane_id != active_plane_id {
            return Err(CoreError::InvalidArgument(
                "filter update plane does not match the active preview",
            ));
        }
        let preview_revision = self.allocate_preview_revision()?;
        let preview_document =
            filter_document(&base_document, plane_id, &filter, preview_revision)?;
        let info = preview_info(
            plane_id,
            &base_document,
            &preview_document,
            preview_revision,
        )?;
        self.filter_preview = Some(FilterPreview {
            plane_id,
            base_document,
            preview_document,
            filter,
            preview_revision,
        });
        self.render_cache.clear();
        Ok(info)
    }

    pub fn cancel_filter_preview(&mut self) -> Result<FilterPreviewInfo, CoreError> {
        let preview = self
            .filter_preview
            .take()
            .ok_or(CoreError::InvalidState("there is no active filter preview"))?;
        self.render_cache.clear();
        let checksum = preview
            .base_document
            .plane_by_id(preview.plane_id)
            .ok_or(CoreError::InvalidState("preview plane no longer exists"))?
            .raster
            .checksum();
        Ok(FilterPreviewInfo {
            plane_id: preview.plane_id,
            base_checksum: checksum,
            preview_checksum: checksum,
            preview_revision: self.document_revision,
        })
    }

    pub fn apply_filter_preview(&mut self) -> Result<DispatchOutcome, CoreError> {
        let preview = self
            .filter_preview
            .as_ref()
            .cloned()
            .ok_or(CoreError::InvalidState("there is no active filter preview"))?;
        let result = self.commit_document_edit(preview.base_document, preview.preview_document);
        if result.is_ok() {
            self.filter_preview = None;
            self.last_filter = Some(preview.filter);
        }
        result
    }

    pub fn apply_last_filter(&mut self, plane_id: u64) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        let filter = self
            .last_filter
            .clone()
            .ok_or(CoreError::InvalidState("there is no last filter"))?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let after = filter_document(&before, plane_id, &filter, revision)?;
        self.commit_document_edit_with_revision(before, after, revision)
    }

    pub fn create_adjustment_layer(
        &mut self,
        name: &str,
        adjustment: Adjustment,
    ) -> Result<(DispatchOutcome, u64), CoreError> {
        self.ensure_no_active_stroke()?;
        super::validate_node_name(name)?;
        inkpod_image::apply_adjustment(super::PixelValue::Rgba([0; 4]), &adjustment)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        if before.layers.len() >= super::MAX_LAYERS {
            return Err(CoreError::InvalidState("layer limit reached"));
        }
        let layer_id = self.next_id;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(CoreError::InvalidState("stable ID overflow"))?;
        let mut after = before.clone();
        after.layers.insert(
            0,
            LayerNode {
                id: layer_id,
                kind: LayerKind::Adjustment,
                name: super::unique_layer_name(&after.layers, name),
                visible: true,
                editable: true,
                opacity_milli: 1_000,
                planes: Vec::new(),
            },
        );
        after.adjustments.insert(layer_id, adjustment);
        after.active_layer_id = layer_id;
        let outcome = self.commit_document_edit(before, after)?;
        self.next_id = next_id;
        Ok((outcome, layer_id))
    }

    pub fn update_adjustment_layer(
        &mut self,
        layer_id: u64,
        adjustment: Adjustment,
    ) -> Result<DispatchOutcome, CoreError> {
        self.ensure_no_active_stroke()?;
        inkpod_image::apply_adjustment(super::PixelValue::Rgba([0; 4]), &adjustment)?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let mut after = before.clone();
        let layer = after
            .layers
            .iter()
            .find(|layer| layer.id == layer_id)
            .ok_or(CoreError::InvalidArgument("layer ID does not exist"))?;
        if layer.kind != LayerKind::Adjustment {
            return Err(CoreError::InvalidArgument(
                "layer is not an adjustment layer",
            ));
        }
        after.adjustments.insert(layer_id, adjustment);
        self.commit_document_edit(before, after)
    }

    pub fn adjustment(&self, layer_id: u64) -> Result<&Adjustment, CoreError> {
        self.document
            .as_ref()
            .ok_or(CoreError::NoDocument)?
            .adjustments
            .get(&layer_id)
            .ok_or(CoreError::InvalidArgument(
                "adjustment layer ID does not exist",
            ))
    }

    pub fn apply_gradient_to_plane(
        &mut self,
        plane_id: u64,
        gradient: &Gradient,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_gradient(raster, selection, gradient, revision)
        })
    }

    pub fn apply_boundary_airbrush_to_plane(
        &mut self,
        plane_id: u64,
        effect: &BoundaryAirbrush,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_boundary_airbrush(raster, selection, effect, revision)
        })
    }

    pub fn apply_airbrush_to_plane(
        &mut self,
        plane_id: u64,
        stroke: AirbrushStroke,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_airbrush(raster, selection, stroke, revision)
        })
    }

    pub fn apply_stamp_to_plane(
        &mut self,
        plane_id: u64,
        stamp: Stamp,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            apply_stamp(raster, selection, stamp, revision)
        })
    }

    pub fn edit_plane_alpha(
        &mut self,
        plane_id: u64,
        alpha: &TileRaster,
    ) -> Result<DispatchOutcome, CoreError> {
        self.apply_raster_operation(plane_id, |raster, selection, revision| {
            edit_alpha(raster, selection, alpha, revision)
        })
    }

    fn apply_raster_operation<F>(
        &mut self,
        plane_id: u64,
        operation: F,
    ) -> Result<DispatchOutcome, CoreError>
    where
        F: FnOnce(
            &TileRaster,
            Option<&TileRaster>,
            u64,
        ) -> Result<TileRaster, inkpod_image::RasterError>,
    {
        self.ensure_no_active_stroke()?;
        let before = self.document.as_ref().ok_or(CoreError::NoDocument)?.clone();
        let revision = self.next_document_revision()?;
        let plane = editable_color_plane(&before, plane_id)?;
        let selection = (before.selection.allocated_tile_count() != 0).then_some(&before.selection);
        let raster = operation(&plane.raster, selection, revision)?;
        let mut after = before.clone();
        after
            .plane_by_id_mut(plane_id)
            .ok_or(CoreError::InvalidState("operation plane disappeared"))?
            .raster = raster;
        self.commit_document_edit_with_revision(before, after, revision)
    }
}

fn editable_color_plane(
    document: &CellDocument,
    plane_id: u64,
) -> Result<&super::PlaneNode, CoreError> {
    let layer = document
        .layers
        .iter()
        .find(|layer| layer.planes.iter().any(|plane| plane.id == plane_id))
        .ok_or(CoreError::InvalidArgument("plane ID does not exist"))?;
    let plane = layer
        .planes
        .iter()
        .find(|plane| plane.id == plane_id)
        .expect("located containing layer");
    if !layer.editable || !plane.editable {
        return Err(CoreError::InvalidState("target plane is locked"));
    }
    if !matches!(plane.kind, PlaneType::Color | PlaneType::Raster)
        || !matches!(
            plane.raster.format(),
            PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
        )
    {
        return Err(CoreError::InvalidArgument(
            "target is not an editable RGBA raster plane",
        ));
    }
    Ok(plane)
}

fn filter_document(
    base: &CellDocument,
    plane_id: u64,
    filter: &Filter,
    revision: u64,
) -> Result<CellDocument, CoreError> {
    let plane = editable_color_plane(base, plane_id)?;
    let selection = (base.selection.allocated_tile_count() != 0).then_some(&base.selection);
    let raster = apply_filter(&plane.raster, selection, filter, revision)?;
    let mut preview = base.clone();
    preview
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("preview plane disappeared"))?
        .raster = raster;
    Ok(preview)
}

fn preview_info(
    plane_id: u64,
    base: &CellDocument,
    preview: &CellDocument,
    preview_revision: u64,
) -> Result<FilterPreviewInfo, CoreError> {
    Ok(FilterPreviewInfo {
        plane_id,
        base_checksum: base
            .plane_by_id(plane_id)
            .ok_or(CoreError::InvalidState("preview plane disappeared"))?
            .raster
            .checksum(),
        preview_checksum: preview
            .plane_by_id(plane_id)
            .ok_or(CoreError::InvalidState("preview plane disappeared"))?
            .raster
            .checksum(),
        preview_revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, CurveInterpolation, CurvePoint, PixelValue};

    fn seeded_core() -> (Core, u64) {
        let mut core = Core::new();
        core.new_cell(4, 1, 96_000, 96_000).unwrap();
        let plane_id = core.document.as_ref().unwrap().primary_ids().2;
        let plane = core
            .document
            .as_mut()
            .unwrap()
            .plane_by_id_mut(plane_id)
            .unwrap();
        for (x, color) in [
            [20, 40, 60, 255],
            [80, 100, 120, 128],
            [160, 180, 200, 255],
            [220, 230, 240, 255],
        ]
        .into_iter()
        .enumerate()
        {
            plane
                .raster
                .set_pixel(x as u32, 0, PixelValue::Rgba(color), 1)
                .unwrap();
        }
        (core, plane_id)
    }

    #[test]
    fn m6_acceptance_cancel_restores_the_original_tile_checksum() {
        let (mut core, plane_id) = seeded_core();
        let original = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        let preview = core
            .begin_filter_preview(
                plane_id,
                Filter::Invert {
                    channel: Channel::Rgb,
                },
            )
            .unwrap();
        assert_eq!(preview.base_checksum, original);
        assert_ne!(preview.preview_checksum, original);
        assert_ne!(
            core.build_snapshot().revision(),
            core.document_info().unwrap().document_revision
        );
        let cancelled = core.cancel_filter_preview().unwrap();
        assert_eq!(cancelled.preview_checksum, original);
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );
    }

    #[test]
    fn m6_acceptance_apply_is_exactly_one_undo_unit_and_last_filter_reuses_it() {
        let (mut core, plane_id) = seeded_core();
        let original = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        core.begin_filter_preview(
            plane_id,
            Filter::BrightnessContrast {
                brightness_milli: 100,
                contrast_milli: 200,
            },
        )
        .unwrap();
        core.apply_filter_preview().unwrap();
        assert_eq!(core.history.len(), 1);
        let filtered = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        assert_ne!(filtered, original);
        core.undo().unwrap();
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );
        core.redo().unwrap();
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            filtered
        );
        core.apply_last_filter(plane_id).unwrap();
        assert_eq!(core.history.len(), 2);
    }

    #[test]
    fn m6_acceptance_adjustment_order_changes_composite_without_changing_source_plane() {
        let (mut core, plane_id) = seeded_core();
        let original = core
            .document
            .as_ref()
            .unwrap()
            .plane_by_id(plane_id)
            .unwrap()
            .raster
            .checksum();
        let (_, brightness) = core
            .create_adjustment_layer(
                "Brightness",
                Adjustment::BrightnessContrast {
                    brightness_milli: 200,
                    contrast_milli: 0,
                },
            )
            .unwrap();
        let (_, curve) = core
            .create_adjustment_layer(
                "Curve",
                Adjustment::ToneCurve {
                    channel: Channel::Rgb,
                    interpolation: CurveInterpolation::Bezier,
                    points: vec![
                        CurvePoint {
                            input: 0,
                            output: 0,
                        },
                        CurvePoint {
                            input: 32_768,
                            output: 8_000,
                        },
                        CurvePoint {
                            input: 65_535,
                            output: 65_535,
                        },
                    ],
                },
            )
            .unwrap();
        let first = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
        core.reorder_layer(brightness, 0).unwrap();
        let second = core.build_snapshot().tiles()[0].pixels()[..4].to_vec();
        assert_ne!(first, second);
        assert_eq!(
            core.document
                .as_ref()
                .unwrap()
                .plane_by_id(plane_id)
                .unwrap()
                .raster
                .checksum(),
            original
        );
        assert!(core.adjustment(curve).is_ok());

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inkpod-m6-adjustment-{}-{nonce}.inkpod",
            std::process::id()
        ));
        core.save(&path).unwrap();
        let mut reopened = Core::new();
        reopened.open(&path).unwrap();
        assert_eq!(
            reopened.adjustment(curve).unwrap(),
            core.adjustment(curve).unwrap()
        );
        assert_eq!(reopened.build_snapshot().tiles()[0].pixels()[..4], second);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn m6_acceptance_boundary_airbrush_preserves_uniform_regions() {
        let mut source = TileRaster::new(7, 1, PixelFormat::StraightRgba8).unwrap();
        for x in 0..7 {
            source
                .set_pixel(
                    x,
                    0,
                    PixelValue::Rgba(if x < 3 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    }),
                    1,
                )
                .unwrap();
        }
        let output = apply_boundary_airbrush(
            &source,
            None,
            &BoundaryAirbrush {
                colors: vec![[65_535, 0, 0, 65_535], [0, 0, 65_535, 65_535]],
                width: 1,
                strength_milli: 1_000,
            },
            2,
        )
        .unwrap();
        assert_eq!(source.pixel(0, 0).unwrap(), output.pixel(0, 0).unwrap());
        assert_eq!(source.pixel(6, 0).unwrap(), output.pixel(6, 0).unwrap());
        assert_ne!(source.pixel(2, 0).unwrap(), output.pixel(2, 0).unwrap());
    }

    #[test]
    fn generic_adjustment_tree_edits_remain_saveable_and_reject_ambiguous_merge() {
        let (mut core, _) = seeded_core();
        let (_, first) = core
            .create_layer(LayerKind::Adjustment, "Generic Adjustment")
            .unwrap();
        let (_, second) = core.duplicate_layer(first).unwrap();
        assert!(core.adjustment(first).is_ok());
        assert!(core.adjustment(second).is_ok());
        assert!(inkpod_format::encode(&core.document.as_ref().unwrap().to_file()).is_ok());
        assert!(matches!(
            core.merge_layer_into_below(second),
            Err(CoreError::InvalidArgument(_))
        ));
    }
}
