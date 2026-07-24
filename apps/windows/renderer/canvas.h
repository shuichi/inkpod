#pragma once

#include <windows.h>

#include <cstddef>
#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::renderer {

inline constexpr UINT kCanvasRenderOnce = WM_APP + 0x121U;
inline constexpr UINT kCanvasRenderFailed = WM_APP + 0x122U;
inline constexpr UINT kCanvasSimulateDeviceLoss = WM_APP + 0x123U;
inline constexpr UINT kCanvasStrokeReady = WM_APP + 0x124U;
inline constexpr UINT kCanvasViewGesture = WM_APP + 0x125U;
inline constexpr UINT kCanvasViewportChanged = WM_APP + 0x126U;
inline constexpr UINT kCanvasGetRendererThreadId = WM_APP + 0x127U;
inline constexpr UINT kCanvasGetPresentedFrameCount = WM_APP + 0x128U;
inline constexpr UINT kCanvasGetDocumentBounds = WM_APP + 0x129U;
inline constexpr UINT kCanvasPointerMoved = WM_APP + 0x12AU;
inline constexpr UINT kCanvasSetFloatingPreview = WM_APP + 0x12BU;
inline constexpr UINT kCanvasSetGeometryPreview = WM_APP + 0x12CU;
inline constexpr std::size_t kCanvasGeometryPreviewPoints = 128U;

struct CanvasDocumentBounds {
    double left;
    double top;
    double right;
    double bottom;
};

struct CanvasFloatingPreview {
    std::uint32_t struct_size;
    std::uint32_t active;
    InkpodFrameRect bounds;
    InkpodFloatingTransform transform;
};

struct CanvasGeometryPreview {
    std::uint32_t struct_size;
    std::uint32_t active;
    std::uint32_t point_count;
    std::uint32_t closed;
    InkpodVectorPoint points[kCanvasGeometryPreviewPoints];
};

enum class CanvasStrokeEventKind : std::uint32_t {
    Begin,
    Append,
    End,
    Cancel,
};

struct CanvasStrokeEvent {
    CanvasStrokeEventKind kind;
    const InkpodStrokeSample* samples;
    std::uint64_t sample_count;
};

struct CanvasViewGesture {
    InkpodViewCommandKind kind;
    double value1;
    double value2;
    double value3;
};

class CanvasSnapshotSink {
public:
    /* Consumes the Rust owner on every call. The sink releases it after a
     * rejected enqueue, pending replacement, renderer replacement, or stop. */
    virtual bool Submit(InkpodSnapshot* snapshot) noexcept = 0;

protected:
    virtual ~CanvasSnapshotSink() = default;
};

bool RegisterCanvasClass(HINSTANCE instance) noexcept;
HWND CreateCanvasWindow(HINSTANCE instance, HWND parent) noexcept;
/* Call on the UI thread after Canvas creation. The sink remains valid until
 * WM_NCDESTROY; callers must stop producer threads before destroying Canvas. */
CanvasSnapshotSink* GetCanvasSnapshotSink(HWND canvas) noexcept;

}  // namespace inkpod::renderer
