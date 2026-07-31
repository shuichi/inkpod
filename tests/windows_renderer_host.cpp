#include <windows.h>

#include <cstddef>
#include <cstdio>
#include <cstdint>

#include "app/identity.h"
#include "inkpod/core_ffi.h"
#include "renderer/canvas.h"
#include "renderer/renderer_host.h"

namespace {

class CoreOwner final {
public:
    ~CoreOwner() {
        if (core_ != nullptr) {
            inkpod_core_destroy(&core_);
        }
    }

    bool Create(std::uint64_t uuid, std::uint32_t width, std::uint32_t height) noexcept {
        const InkpodCoreConfig config{
            sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
        if (inkpod_core_create(&config, &core_) != INKPOD_STATUS_OK) {
            return false;
        }
        const InkpodCellCreateOptions options{
            sizeof(InkpodCellCreateOptions),
            0U,
            INKPOD_FEATURE_NONE,
            UINT64_C(0x52454e4400000000) | uuid,
            UINT64_C(0x484f535400000000) | uuid,
            width,
            height,
            96000U,
            96000U};
        InkpodDocumentInfo info{};
        info.struct_size = sizeof(info);
        return inkpod_core_new_cell(core_, &options, &info) == INKPOD_STATUS_OK;
    }

    bool Build(
        const inkpod::renderer::SnapshotRoute& route,
        inkpod::renderer::SnapshotEnvelope& envelope) noexcept {
        const InkpodSnapshotOptions options{
            sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
        InkpodSnapshot* snapshot{};
        if (inkpod_core_build_snapshot(core_, &options, &snapshot) != INKPOD_STATUS_OK) {
            return false;
        }
        InkpodSnapshotView view{};
        view.struct_size = sizeof(view);
        InkpodSnapshotTransform transform{};
        transform.struct_size = sizeof(transform);
        if (inkpod_snapshot_get_view(snapshot, &view) != INKPOD_STATUS_OK
            || inkpod_snapshot_get_transform(snapshot, &transform) != INKPOD_STATUS_OK) {
            inkpod_snapshot_release(&snapshot);
            return false;
        }
        envelope = inkpod::renderer::SnapshotEnvelope{
            route, view.revision, transform.view_revision, snapshot};
        return true;
    }

private:
    InkpodCore* core_{};
};

class WindowOwner final {
public:
    ~WindowOwner() {
        Reset();
    }

    void Reset() noexcept {
        if (window_ != nullptr) {
            DestroyWindow(window_);
            window_ = nullptr;
        }
    }

    HWND window_{};
};

bool HasBounds(
    inkpod::renderer::RendererHost& host,
    inkpod::app::CanvasId canvas,
    inkpod::app::Generation generation,
    double width,
    double height) noexcept {
    inkpod::renderer::CanvasDocumentBounds bounds{};
    return SUCCEEDED(host.GetDocumentBounds(canvas, generation, bounds))
        && bounds.left == 0.0 && bounds.top == 0.0
        && bounds.right == width && bounds.bottom == height;
}

int Run() {
    using inkpod::app::CanvasId;
    using inkpod::app::DocumentSessionId;
    using inkpod::app::DocumentViewId;
    using inkpod::app::Generation;

    constexpr CanvasId first_canvas{101U};
    constexpr CanvasId second_canvas{102U};
    constexpr Generation first_surface_generation{11U};
    constexpr Generation second_surface_generation{12U};
    constexpr Generation document_generation{21U};

    HINSTANCE instance = GetModuleHandleW(nullptr);
    if (!inkpod::renderer::RegisterCanvasClass(instance)) {
        return 1;
    }
    inkpod::renderer::RendererHost host;
    if (FAILED(host.Start()) || host.ThreadId() == 0U
        || host.ThreadId() == GetCurrentThreadId() || host.DeviceGeneration() == 0U) {
        return 2;
    }

    WindowOwner first_parent;
    WindowOwner second_parent;
    WindowOwner first_canvas_window;
    WindowOwner second_canvas_window;
    first_parent.window_ = CreateWindowExW(
        0, L"STATIC", L"renderer-host-1", WS_OVERLAPPEDWINDOW,
        0, 0, 320, 240, nullptr, nullptr, instance, nullptr);
    second_parent.window_ = CreateWindowExW(
        0, L"STATIC", L"renderer-host-2", WS_OVERLAPPEDWINDOW,
        0, 0, 320, 240, nullptr, nullptr, instance, nullptr);
    if (first_parent.window_ == nullptr || second_parent.window_ == nullptr) {
        return 3;
    }
    first_canvas_window.window_ = inkpod::renderer::CreateCanvasWindow(
        instance,
        first_parent.window_,
        host,
        first_canvas,
        first_surface_generation);
    second_canvas_window.window_ = inkpod::renderer::CreateCanvasWindow(
        instance,
        second_parent.window_,
        host,
        second_canvas,
        second_surface_generation);
    if (first_canvas_window.window_ == nullptr || second_canvas_window.window_ == nullptr
        || host.SurfaceCount() != 2U
        || SendMessageW(first_canvas_window.window_,
               inkpod::renderer::kCanvasGetRendererThreadId, 0, 0)
            != static_cast<LRESULT>(host.ThreadId())
        || SendMessageW(second_canvas_window.window_,
               inkpod::renderer::kCanvasGetRendererThreadId, 0, 0)
            != static_cast<LRESULT>(host.ThreadId())) {
        return 4;
    }

    if (!inkpod::renderer::BindCanvasSnapshotSink(
            first_canvas_window.window_,
            DocumentSessionId(201U),
            DocumentViewId(301U),
            document_generation)
        || !inkpod::renderer::BindCanvasSnapshotSink(
            second_canvas_window.window_,
            DocumentSessionId(202U),
            DocumentViewId(302U),
            document_generation)) {
        return 5;
    }
    auto* first_sink = inkpod::renderer::GetCanvasSnapshotSink(
        first_canvas_window.window_);
    auto* second_sink = inkpod::renderer::GetCanvasSnapshotSink(
        second_canvas_window.window_);
    if (first_sink == nullptr || second_sink == nullptr
        || !first_sink->AcceptsSnapshots() || !second_sink->AcceptsSnapshots()) {
        return 6;
    }

    CoreOwner first_core;
    CoreOwner second_core;
    if (!first_core.Create(1U, 32U, 24U) || !second_core.Create(2U, 48U, 16U)) {
        return 7;
    }
    inkpod::renderer::SnapshotEnvelope first_envelope{};
    inkpod::renderer::SnapshotEnvelope second_envelope{};
    if (!first_core.Build(first_sink->Route(), first_envelope)
        || !second_core.Build(second_sink->Route(), second_envelope)
        || !first_sink->Submit(first_envelope)
        || !second_sink->Submit(second_envelope)
        || !HasBounds(host, first_canvas, first_surface_generation, 32.0, 24.0)
        || !HasBounds(host, second_canvas, second_surface_generation, 48.0, 16.0)) {
        return 8;
    }

    CoreOwner replacement_core;
    if (!replacement_core.Create(4U, 40U, 20U)) {
        return 20;
    }
    inkpod::renderer::SnapshotEnvelope replaced{};
    inkpod::renderer::SnapshotEnvelope replacement{};
    host.SetQueuePausedForSmokeTest(true);
    if (!first_core.Build(first_sink->Route(), replaced)
        || !replacement_core.Build(first_sink->Route(), replacement)
        || !first_sink->Submit(replaced)
        || !first_sink->Submit(replacement)) {
        host.SetQueuePausedForSmokeTest(false);
        return 21;
    }
    host.SetQueuePausedForSmokeTest(false);
    if (!HasBounds(host, first_canvas, first_surface_generation, 40.0, 20.0)) {
        return 22;
    }

    host.SetQueuePausedForSmokeTest(true);
    for (std::size_t index = 0U; index < 256U; ++index) {
        host.RequestRender(first_canvas, first_surface_generation);
    }
    inkpod::renderer::SnapshotEnvelope queue_failure{};
    if (!replacement_core.Build(first_sink->Route(), queue_failure)
        || first_sink->Submit(queue_failure)) {
        host.SetQueuePausedForSmokeTest(false);
        return 23;
    }
    host.SetQueuePausedForSmokeTest(false);
    if (!host.WaitQueueIdleForSmokeTest()) {
        return 24;
    }
    host.SetVisible(first_canvas, first_surface_generation, true);
    if (FAILED(host.RenderOnce(first_canvas, first_surface_generation))) {
        return 26;
    }

    const std::uint64_t device_generation_before = host.DeviceGeneration();
    const HRESULT device_loss = host.SimulateDeviceLoss(
        first_canvas, first_surface_generation);
    if (FAILED(device_loss)) {
        std::fwprintf(stderr, L"device loss recovery failed: 0x%08lx\n",
            static_cast<unsigned long>(device_loss));
        return 90;
    }
    if (host.DeviceGeneration() <= device_generation_before) {
        return 91;
    }
    if (!HasBounds(host, first_canvas, first_surface_generation, 40.0, 20.0)) {
        return 92;
    }
    if (!HasBounds(host, second_canvas, second_surface_generation, 48.0, 16.0)) {
        return 93;
    }

    inkpod::renderer::SnapshotEnvelope stale{};
    if (!first_core.Build(first_sink->Route(), stale)
        || !inkpod::renderer::BindCanvasSnapshotSink(
            first_canvas_window.window_,
            DocumentSessionId(203U),
            DocumentViewId(303U),
            Generation(22U))
        || host.Submit(stale)) {
        return 10;
    }
    inkpod::renderer::CanvasDocumentBounds cleared{};
    if (FAILED(host.GetDocumentBounds(
            first_canvas, first_surface_generation, cleared))
        || cleared.right != 0.0 || cleared.bottom != 0.0) {
        return 11;
    }

    CoreOwner rebound_core;
    if (!rebound_core.Create(3U, 64U, 12U)) {
        return 12;
    }
    inkpod::renderer::SnapshotEnvelope rebound{};
    if (!rebound_core.Build(first_sink->Route(), rebound)
        || !first_sink->Submit(rebound)
        || !HasBounds(host, first_canvas, first_surface_generation, 64.0, 12.0)) {
        return 13;
    }

    host.SetVisible(first_canvas, first_surface_generation, false);
    if (FAILED(host.RenderOnce(first_canvas, first_surface_generation))
        || first_sink->AcceptsSnapshots()) {
        return 14;
    }
    inkpod::renderer::SnapshotEnvelope hidden{};
    if (!rebound_core.Build(first_sink->Route(), hidden) || first_sink->Submit(hidden)) {
        return 15;
    }
    host.SetVisible(first_canvas, first_surface_generation, true);
    host.Resize(first_canvas, first_surface_generation, 256U, 192U);
    host.DpiChanged(first_canvas, first_surface_generation);
    if (FAILED(host.RenderOnce(first_canvas, first_surface_generation))
        || !first_sink->AcceptsSnapshots()
        || !HasBounds(host, second_canvas, second_surface_generation, 48.0, 16.0)) {
        return 16;
    }

    const inkpod::renderer::SnapshotRoute stopped_route = first_sink->Route();
    inkpod::renderer::SnapshotEnvelope close_pending{};
    if (!replacement_core.Build(first_sink->Route(), close_pending)
        || !first_sink->Submit(close_pending)) {
        return 27;
    }
    first_canvas_window.Reset();
    if (host.SurfaceCount() != 1U
        || !HasBounds(host, second_canvas, second_surface_generation, 48.0, 16.0)) {
        return 17;
    }
    inkpod::renderer::SnapshotEnvelope shutdown_pending{};
    if (!second_core.Build(second_sink->Route(), shutdown_pending)
        || !second_sink->Submit(shutdown_pending)) {
        return 28;
    }
    host.Stop();
    if (host.ThreadId() != 0U || host.DeviceGeneration() != 0U
        || host.SurfaceCount() != 0U) {
        return 19;
    }
    second_canvas_window.Reset();
    inkpod::renderer::SnapshotEnvelope stopped{};
    if (!replacement_core.Build(stopped_route, stopped) || host.Submit(stopped)) {
        return 25;
    }
    return 0;
}

}  // namespace

int wmain() {
    return Run();
}
