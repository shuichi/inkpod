use super::*;

pub(super) fn pressure_trace_contains(
    x: f64,
    y: f64,
    samples: &[DocumentStrokeSample],
    diameter: f64,
) -> bool {
    if samples.len() == 1 {
        let radius = diameter * f64::from(samples[0].pressure.clamp(0.0, 1.0)) / 2.0;
        let dx = x - f64::from(samples[0].point.x);
        let dy = y - f64::from(samples[0].point.y);
        return dx.mul_add(dx, dy * dy) <= radius * radius;
    }
    samples.windows(2).any(|segment| {
        let start_x = f64::from(segment[0].point.x);
        let start_y = f64::from(segment[0].point.y);
        let dx = f64::from(segment[1].point.x) - start_x;
        let dy = f64::from(segment[1].point.y) - start_y;
        let length_squared = dx.mul_add(dx, dy * dy);
        let t = if length_squared <= f64::EPSILON {
            0.0
        } else {
            ((x - start_x).mul_add(dx, (y - start_y) * dy) / length_squared).clamp(0.0, 1.0)
        };
        let center_x = dx.mul_add(t, start_x);
        let center_y = dy.mul_add(t, start_y);
        let start_pressure = f64::from(segment[0].pressure.clamp(0.0, 1.0));
        let end_pressure = f64::from(segment[1].pressure.clamp(0.0, 1.0));
        let pressure = (end_pressure - start_pressure).mul_add(t, start_pressure);
        let radius = diameter * pressure / 2.0;
        let distance_x = x - center_x;
        let distance_y = y - center_y;
        distance_x.mul_add(distance_x, distance_y * distance_y) <= radius * radius
    })
}

pub(super) fn effect_samples(
    samples: Vec<DocumentStrokeSample>,
) -> Result<Vec<EffectSample>, CoreError> {
    samples
        .into_iter()
        .map(|sample| {
            let x = inkpod_image::canonical_scaled_i64_from_f32(sample.point.x, 1_000, 1).ok_or(
                CoreError::InvalidArgument(
                    "effect sample coordinate is outside fixed-point bounds",
                ),
            )?;
            let y = inkpod_image::canonical_scaled_i64_from_f32(sample.point.y, 1_000, 1).ok_or(
                CoreError::InvalidArgument(
                    "effect sample coordinate is outside fixed-point bounds",
                ),
            )?;
            let pressure = inkpod_image::canonical_scaled_i64_from_f32(
                sample.pressure.clamp(0.0, 1.0),
                1_000,
                1,
            )
            .ok_or(CoreError::InvalidArgument("effect pressure is non-finite"))?;
            Ok(EffectSample {
                x_milli: x,
                y_milli: y,
                pressure_milli: pressure.clamp(0, 1_000) as u32,
            })
        })
        .collect()
}

pub(super) fn editable_rgba_plane(
    document: &CellDocument,
    plane_id: PlaneId,
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
    if !matches!(
        plane.kind,
        PlaneType::MainLine | PlaneType::Color | PlaneType::Raster
    ) || !matches!(
        plane.raster.format(),
        PixelFormat::StraightRgba8 | PixelFormat::StraightRgba16
    ) {
        return Err(CoreError::InvalidArgument(
            "target is not an editable RGBA raster plane",
        ));
    }
    Ok(plane)
}

pub(super) fn filter_document_with_progress(
    base: &CellDocument,
    plane_id: PlaneId,
    filter: &Filter,
    revision: RenderRevision,
    progress: &mut (impl FnMut(u64, u64) -> bool + ?Sized),
) -> Result<CellDocument, CoreError> {
    let plane = editable_rgba_plane(base, plane_id)?;
    let selection = (base.selection.allocated_tile_count() != 0).then_some(&base.selection);
    let raster =
        apply_filter_with_progress(&plane.raster, selection, filter, revision.get(), progress)?;
    let mut preview = base.clone();
    preview
        .plane_by_id_mut(plane_id)
        .ok_or(CoreError::InvalidState("preview plane disappeared"))?
        .raster = raster;
    Ok(preview)
}

pub(super) fn preview_info(
    plane_id: PlaneId,
    base: &CellDocument,
    preview: &CellDocument,
    preview_revision: PreviewRevision,
) -> Result<FilterPreviewInfo, CoreError> {
    Ok(FilterPreviewInfo {
        plane_id: plane_id.get(),
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
        preview_revision: preview_revision.get(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentPointF32;

    #[test]
    fn blur_pen_pressure_varies_the_screen_fixed_region() {
        let samples = [
            DocumentStrokeSample {
                point: DocumentPointF32 { x: 1.0, y: 1.0 },
                pressure: 0.25,
            },
            DocumentStrokeSample {
                point: DocumentPointF32 { x: 5.0, y: 1.0 },
                pressure: 1.0,
            },
        ];
        assert!(!pressure_trace_contains(1.0, 2.0, &samples, 4.0));
        assert!(pressure_trace_contains(5.0, 2.0, &samples, 4.0));
        assert!(pressure_trace_contains(3.0, 1.0, &samples, 4.0));
    }
}
