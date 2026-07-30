use crate::*;

pub(crate) fn device_to_document(
    view: ViewState,
    document_size: DocumentSizeU32,
    device_point: DevicePointF64,
) -> DocumentPointF64 {
    view_transform(view, document_size).device_to_document(device_point)
}

fn view_transform(view: ViewState, document_size: DocumentSizeU32) -> ViewTransform {
    ViewTransform::new(
        document_size,
        view.zoom,
        view.pan,
        view.flip_horizontal,
        view.flip_vertical,
    )
}

pub(super) fn centered_pan(
    document_size: DocumentSizeU32,
    viewport: DeviceSizeF64,
    zoom: ZoomFactor,
) -> Result<DeviceOffsetF64, CoreError> {
    DeviceOffsetF64::new(
        (viewport.width - f64::from(document_size.width) * zoom.get()) / 2.0,
        (viewport.height - f64::from(document_size.height) * zoom.get()) / 2.0,
    )
}

pub(super) fn box_zoom_pan(
    document_rect: DocumentRectI32,
    viewport: DeviceSizeF64,
    zoom: ZoomFactor,
) -> Result<DeviceOffsetF64, CoreError> {
    DeviceOffsetF64::new(
        (viewport.width - f64::from(document_rect.width) * zoom.get()) / 2.0
            - f64::from(document_rect.origin.x) * zoom.get(),
        (viewport.height - f64::from(document_rect.height) * zoom.get()) / 2.0
            - f64::from(document_rect.origin.y) * zoom.get(),
    )
}

pub(crate) fn stroke_coordinate_is_supported(value: f64) -> bool {
    value.is_finite() && value.abs() <= f64::from(MAX_STROKE_COORDINATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_document_round_trip_covers_flips_edges_and_extreme_zoom() {
        let document_size = DocumentSizeU32::new(37, 19);
        for (flip_horizontal, flip_vertical) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            for zoom in [MIN_ZOOM, 1.0, MAX_ZOOM] {
                let view = ViewState {
                    zoom: ZoomFactor::clamped(zoom).unwrap(),
                    pan: DeviceOffsetF64::new(-17.25, 31.5).unwrap(),
                    flip_horizontal,
                    flip_vertical,
                    ..ViewState::default()
                };
                let transform = view_transform(view, document_size);
                for (x, y) in [
                    (0.0, 0.0),
                    (36.999_999, 18.999_999),
                    (37.0, 19.0),
                    (38.0, 20.0),
                    (12.25, 7.75),
                ] {
                    let document_point = DocumentPointF64::new(x, y).unwrap();
                    let device_point = transform.document_to_device(document_point);
                    let actual_document = device_to_document(view, document_size, device_point);
                    assert!((actual_document.x - x).abs() <= 1.0e-8);
                    assert!((actual_document.y - y).abs() <= 1.0e-8);

                    let actual_device = transform.document_to_device(actual_document);
                    assert!((actual_device.x - device_point.x).abs() <= 1.0e-8);
                    assert!((actual_device.y - device_point.y).abs() <= 1.0e-8);
                }
            }
        }
    }
}
