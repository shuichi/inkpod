//! Internal document/view/device coordinate and dimension types.

use crate::{CoreError, MAX_STROKE_COORDINATE, MAX_ZOOM, MIN_ZOOM, RectI32};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DocumentPointF64 {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl DocumentPointF64 {
    pub(crate) fn new(x: f64, y: f64) -> Result<Self, CoreError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(CoreError::InvalidArgument(
                "document coordinate is not finite",
            ));
        }
        Ok(Self { x, y })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DocumentPointF32 {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl DocumentPointF32 {
    pub(crate) fn new(x: f32, y: f32) -> Result<Self, CoreError> {
        if !x.is_finite()
            || !y.is_finite()
            || x.abs() > MAX_STROKE_COORDINATE
            || y.abs() > MAX_STROKE_COORDINATE
        {
            return Err(CoreError::InvalidArgument(
                "document coordinate is outside bounds",
            ));
        }
        Ok(Self { x, y })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentPointI32 {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DevicePointF64 {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl DevicePointF64 {
    pub(crate) fn new(x: f64, y: f64) -> Result<Self, CoreError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(CoreError::InvalidArgument(
                "device coordinate is not finite",
            ));
        }
        Ok(Self { x, y })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentSizeU32 {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl DocumentSizeU32 {
    pub(crate) const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceSizeF64 {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl DeviceSizeF64 {
    pub(crate) const ONE: Self = Self {
        width: 1.0,
        height: 1.0,
    };

    pub(crate) fn new(width: f64, height: f64) -> Result<Self, CoreError> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(CoreError::InvalidArgument(
                "viewport dimensions are invalid",
            ));
        }
        Ok(Self { width, height })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceOffsetF64 {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl DeviceOffsetF64 {
    pub(crate) const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub(crate) fn new(x: f64, y: f64) -> Result<Self, CoreError> {
        if !supported_view_translation(x) || !supported_view_translation(y) {
            return Err(CoreError::InvalidArgument(
                "view translation is outside the finite supported range",
            ));
        }
        Ok(Self { x, y })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentOffsetI32 {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DocumentScaleF64 {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl DocumentScaleF64 {
    pub(crate) fn between(source: DocumentSizeU32, destination: DocumentSizeU32) -> Self {
        Self {
            x: f64::from(destination.width) / f64::from(source.width),
            y: f64::from(destination.height) / f64::from(source.height),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentRectI32 {
    pub(crate) origin: DocumentPointI32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl DocumentRectI32 {
    pub(crate) const fn from_public(rect: RectI32) -> Self {
        Self {
            origin: DocumentPointI32 {
                x: rect.x,
                y: rect.y,
            },
            width: rect.width,
            height: rect.height,
        }
    }

    pub(crate) const fn into_public(self) -> RectI32 {
        RectI32 {
            x: self.origin.x,
            y: self.origin.y,
            width: self.width,
            height: self.height,
        }
    }

    pub(crate) const fn has_positive_size(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZoomFactor(f64);

impl ZoomFactor {
    pub(crate) const ONE: Self = Self(1.0);

    pub(crate) fn clamped(value: f64) -> Result<Self, CoreError> {
        if value.is_nan() {
            return Err(CoreError::InvalidArgument("zoom is not a number"));
        }
        Ok(Self(value.clamp(MIN_ZOOM, MAX_ZOOM)))
    }

    pub(crate) const fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewTransform {
    document_size: DocumentSizeU32,
    zoom: ZoomFactor,
    pan: DeviceOffsetF64,
    flip_horizontal: bool,
    flip_vertical: bool,
}

impl ViewTransform {
    pub(crate) const fn new(
        document_size: DocumentSizeU32,
        zoom: ZoomFactor,
        pan: DeviceOffsetF64,
        flip_horizontal: bool,
        flip_vertical: bool,
    ) -> Self {
        Self {
            document_size,
            zoom,
            pan,
            flip_horizontal,
            flip_vertical,
        }
    }

    pub(crate) fn document_to_device(self, point: DocumentPointF64) -> DevicePointF64 {
        let document_x = if self.flip_horizontal {
            f64::from(self.document_size.width) - point.x
        } else {
            point.x
        };
        let document_y = if self.flip_vertical {
            f64::from(self.document_size.height) - point.y
        } else {
            point.y
        };
        DevicePointF64 {
            x: document_x.mul_add(self.zoom.get(), self.pan.x),
            y: document_y.mul_add(self.zoom.get(), self.pan.y),
        }
    }

    pub(crate) fn device_to_document(self, point: DevicePointF64) -> DocumentPointF64 {
        let mut document_x = (point.x - self.pan.x) / self.zoom.get();
        let mut document_y = (point.y - self.pan.y) / self.zoom.get();
        if self.flip_horizontal {
            document_x = f64::from(self.document_size.width) - document_x;
        }
        if self.flip_vertical {
            document_y = f64::from(self.document_size.height) - document_y;
        }
        DocumentPointF64 {
            x: document_x,
            y: document_y,
        }
    }

    pub(crate) fn pan_for_anchor(
        document_size: DocumentSizeU32,
        zoom: ZoomFactor,
        flip_horizontal: bool,
        flip_vertical: bool,
        document_point: DocumentPointF64,
        device_point: DevicePointF64,
    ) -> Result<DeviceOffsetF64, CoreError> {
        let without_pan = Self::new(
            document_size,
            zoom,
            DeviceOffsetF64::ZERO,
            flip_horizontal,
            flip_vertical,
        )
        .document_to_device(document_point);
        DeviceOffsetF64::new(
            device_point.x - without_pan.x,
            device_point.y - without_pan.y,
        )
    }
}

pub(crate) fn supported_view_translation(value: f64) -> bool {
    value.is_finite() && value.abs() <= f64::from(MAX_STROKE_COORDINATE)
}
