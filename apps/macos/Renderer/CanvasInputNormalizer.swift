import Foundation

struct CanvasTilt: Equatable, Sendable {
    let x: Float
    let y: Float
}

enum CanvasInputNormalizer {
    static func localDevicePoint(
        backingPoint: CGPoint,
        backingBounds: CGRect,
        isFlipped: Bool
    ) -> CGPoint? {
        guard backingPoint.x.isFinite,
              backingPoint.y.isFinite,
              backingBounds.origin.x.isFinite,
              backingBounds.origin.y.isFinite,
              backingBounds.width.isFinite,
              backingBounds.height.isFinite,
              backingBounds.width > 0,
              backingBounds.height > 0,
              backingBounds.contains(backingPoint)
        else {
            return nil
        }
        return CGPoint(
            x: backingPoint.x - backingBounds.minX,
            y: isFlipped
                ? backingBounds.maxY - backingPoint.y
                : backingPoint.y - backingBounds.minY
        )
    }

    static func sample(
        deviceX: Double,
        deviceY: Double,
        drawableWidth: Double,
        drawableHeight: Double,
        pressure: Float?,
        tilt: CanvasTilt?
    ) -> CorePointerSample? {
        guard deviceX.isFinite,
              deviceY.isFinite,
              drawableWidth.isFinite,
              drawableHeight.isFinite,
              drawableWidth > 0,
              drawableHeight > 0,
              deviceX >= 0,
              deviceY >= 0,
              deviceX < drawableWidth,
              deviceY < drawableHeight
        else {
            return nil
        }
        let normalizedPressure = min(max(pressure ?? 1, 0), 1)
        let normalizedTiltX = min(max(tilt?.x ?? 0, -1), 1)
        let normalizedTiltY = min(max(tilt?.y ?? 0, -1), 1)
        return CorePointerSample(
            deviceX: Float(deviceX),
            deviceY: Float(deviceY),
            pressure: normalizedPressure,
            tiltX: normalizedTiltX,
            tiltY: normalizedTiltY
        )
    }
}
