#pragma once

#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <vector>

#include "renderer_host.h"

namespace inkpod::renderer {

inline constexpr UINT kCanvasRenderOnce = WM_APP + 0x121U;
inline constexpr UINT kCanvasRenderFailed = WM_APP + 0x122U;
inline constexpr UINT kCanvasSimulateDeviceLoss = WM_APP + 0x123U;
inline constexpr UINT kCanvasStrokeReady = WM_APP + 0x124U;
inline constexpr UINT kCanvasViewGesture = WM_APP + 0x125U;
inline constexpr UINT kCanvasViewportChanged = WM_APP + 0x126U;
inline constexpr UINT kCanvasGetRendererThreadId = WM_APP + 0x127U;
inline constexpr UINT kCanvasGetPresentedFrameCount = WM_APP + 0x128U;
inline constexpr UINT kCanvasActivated = WM_APP + 0x129U;
inline constexpr UINT kCanvasPointerMoved = WM_APP + 0x12AU;
inline constexpr UINT kCanvasInteractionEnded = WM_APP + 0x12BU;
inline constexpr UINT kCanvasValidateClosedVectorStroke = WM_APP + 0x12DU;
inline constexpr UINT kCanvasClearGeometryPreview = WM_APP + 0x12FU;
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

struct OwnedCanvasStrokeEvent {
    CanvasStrokeEventKind kind{};
    std::vector<InkpodStrokeSample> samples;
};

struct CanvasViewGesture {
    InkpodViewCommandKind kind;
    double value1;
    double value2;
    double value3;
};

class CanvasSnapshotSink {
public:
    [[nodiscard]] virtual SnapshotRoute Route() const noexcept = 0;
    [[nodiscard]] virtual bool AcceptsSnapshots() const noexcept = 0;
    /* Consumes the Rust owner on every call. */
    virtual bool Submit(SnapshotEnvelope envelope) noexcept = 0;

protected:
    virtual ~CanvasSnapshotSink() = default;
};

bool RegisterCanvasClass(HINSTANCE instance) noexcept;
HWND CreateCanvasWindow(
    HINSTANCE instance,
    HWND parent,
    RendererHost& renderer,
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept;
/* Call on the UI thread after Canvas creation. The sink remains valid until
 * WM_NCDESTROY; callers must stop producer threads before destroying Canvas. */
CanvasSnapshotSink* GetCanvasSnapshotSink(HWND canvas) noexcept;
bool BindCanvasSnapshotSink(
    HWND canvas,
    app::DocumentSessionId document_session,
    app::DocumentViewId document_view,
    app::Generation document_generation) noexcept;
bool UnbindCanvasSnapshotSink(HWND canvas) noexcept;
void CancelCanvasStroke(HWND canvas) noexcept;
/* Custom HWND notifications carry only token + generation values. The receiver
 * takes the owned payload from the Canvas that issued the notification. */
bool TakeCanvasStrokeEvent(
    HWND canvas,
    std::uint64_t token,
    app::Generation surface_generation,
    OwnedCanvasStrokeEvent& event) noexcept;
bool TakeCanvasViewGesture(
    HWND canvas,
    std::uint64_t token,
    app::Generation surface_generation,
    CanvasViewGesture& gesture) noexcept;
/* The following typed helpers are UI/Input-thread-only and do not transfer
 * ownership of CanvasHost or RendererHost. */
bool SubmitCanvasStrokeEvent(
    HWND canvas,
    CanvasStrokeEventKind kind,
    const InkpodStrokeSample* samples,
    std::uint64_t sample_count) noexcept;
bool SubmitCanvasStrokeEvent(
    HWND canvas, const CanvasStrokeEvent& event) noexcept;
bool GetCanvasDocumentBounds(
    HWND canvas, CanvasDocumentBounds& bounds) noexcept;
bool GetCanvasGeometryPreview(
    HWND canvas, CanvasGeometryPreview& preview) noexcept;
bool SetCanvasFloatingPreview(
    HWND canvas, const CanvasFloatingPreview& preview) noexcept;
bool SetCanvasGeometryPreview(
    HWND canvas, const CanvasGeometryPreview& preview) noexcept;

}  // namespace inkpod::renderer
