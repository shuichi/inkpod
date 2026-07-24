#pragma once

#include <windows.h>

#include <cstdint>
#include <functional>
#include <memory>
#include <string>
#include <string_view>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::renderer {
class CanvasSnapshotSink;
}

namespace inkpod::app {

inline constexpr UINT kCoreStateChanged = WM_APP + 0x160U;
inline constexpr UINT kCoreAsyncFailed = WM_APP + 0x161U;

enum class StrokeEventKind : std::uint32_t {
    Begin,
    Append,
    End,
    Cancel,
};

struct StrokeStyle {
    InkpodPaintTool tool{INKPOD_TOOL_PENCIL};
    InkpodPlaneKind plane{INKPOD_PLANE_MAIN_LINE};
    InkpodCoordinateSpace coordinate_space{INKPOD_COORDINATE_SPACE_DEVICE};
    std::uint64_t flags{};
    std::uint32_t color_rgba{};
    float diameter{1.0F};
};

struct StrokeEvent {
    StrokeEventKind kind{StrokeEventKind::Cancel};
    StrokeStyle style{};
    std::vector<InkpodStrokeSample> samples;
};

struct EngineMetrics {
    std::uint64_t completed_strokes{};
    std::uint64_t completed_samples{};
    std::uint64_t preview_snapshots{};
};

class CoreEngine final {
public:
    CoreEngine();
    ~CoreEngine();

    CoreEngine(const CoreEngine&) = delete;
    CoreEngine& operator=(const CoreEngine&) = delete;

    InkpodStatus Start(renderer::CanvasSnapshotSink* canvas, HWND owner) noexcept;
    void Stop() noexcept;

    InkpodStatus Invoke(
        std::function<InkpodStatus(InkpodCore*)> operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept;
    bool Enqueue(
        std::function<InkpodStatus(InkpodCore*)> operation,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke,
        std::function<void(InkpodStatus)> completion = {}) noexcept;
    bool EnqueueStroke(StrokeEvent event) noexcept;
    InkpodStatus WaitIdle() noexcept;
    InkpodStatus FlushPreview() noexcept;
    InkpodStatus SetActiveView(std::uint64_t view_id) noexcept;

    bool GetDocumentInfo(InkpodDocumentInfo& info) const noexcept;
    std::wstring LastError() const;
    void SetLocalFailure(std::wstring_view message) noexcept;
    EngineMetrics Metrics() const noexcept;
    DWORD ThreadId() const noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::app
