#include <windows.h>

#include <array>
#include <chrono>
#include <cstddef>
#include <cstdio>
#include <cstdint>
#include <future>
#include <utility>

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
        if (inkpod_core_new_cell(core_, &options, &info) != INKPOD_STATUS_OK) {
            return false;
        }
        document_ = info;
        return true;
    }

    bool ConfigureRasterContent() noexcept {
        const InkpodStrokeSample sample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(document_.width) * 0.5F,
            static_cast<float>(document_.height) * 0.5F,
            1.0F,
            0U};
        const InkpodStrokeInput stroke{
            sizeof(InkpodStrokeInput),
            INKPOD_TOOL_PENCIL,
            INKPOD_PLANE_COLOR,
            INKPOD_COORDINATE_SPACE_DOCUMENT,
            0U,
            UINT32_C(0x0000ffff),
            static_cast<float>(document_.width + document_.height) * 4.0F,
            &sample,
            1U,
            sizeof(InkpodStrokeSample),
            INKPOD_BRUSH_ROUND,
            0U,
            0U,
            INKPOD_START_COLOR_ANY,
            0U};
        InkpodDispatchResult dispatch{};
        dispatch.struct_size = sizeof(dispatch);
        if (inkpod_core_apply_stroke(core_, &stroke, &dispatch) != INKPOD_STATUS_OK) {
            return false;
        }
        return true;
    }

    bool CreateAdjustmentTop() noexcept {
        InkpodFilterInput filter{};
        filter.struct_size = sizeof(filter);
        filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
        filter.parameter_0 = 100;
        filter.parameter_1 = 0;
        constexpr std::array<std::uint8_t, 18U> name{
            'O', 'r', 'd', 'e', 'r', 'e', 'd', ' ', 'B', 'r', 'i', 'g', 'h', 't', 'n', 'e', 's', 's'};
        InkpodDispatchResult dispatch{};
        dispatch.struct_size = sizeof(dispatch);
        std::uint64_t adjustment_layer{};
        return inkpod_core_adjustment_create(
                   core_,
                   &filter,
                   name.data(),
                   name.size(),
                   &dispatch,
                   &adjustment_layer)
                == INKPOD_STATUS_OK
            && adjustment_layer != 0U;
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
    InkpodDocumentInfo document_{};
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

void PumpWindowMessages(bool dispatch_paint = true) {
    MSG message{};
    // Keep posted/input and internal window events moving while isolating the
    // measured render queue from additional WM_PAINT-generated render work.
    constexpr UINT non_paint_categories =
        static_cast<UINT>(QS_ALLINPUT) & ~static_cast<UINT>(QS_PAINT);
    const UINT retrieve_flags = PM_REMOVE
        | (dispatch_paint ? 0U : non_paint_categories << 16U);
    while (PeekMessageW(&message, nullptr, 0U, 0U, retrieve_flags)) {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

template <typename Operation>
auto WithWindowMessages(Operation operation, bool dispatch_paint = true) {
    // DXGI can synchronously contact the HWND owner while the renderer works.
    // Keep this fixture's owner responsive just as the product UI thread is.
    PumpWindowMessages(dispatch_paint);
    auto pending = std::async(std::launch::async, std::move(operation));
    while (pending.wait_for(std::chrono::milliseconds(0)) != std::future_status::ready) {
        PumpWindowMessages(dispatch_paint);
        Sleep(1U);
    }
    return pending.get();
}

HRESULT ReadPresentedPixel(
    inkpod::renderer::RendererHost& host,
    inkpod::app::CanvasId canvas,
    inkpod::app::Generation generation,
    UINT x,
    UINT y,
    inkpod::renderer::CanvasPixelRgba8& pixel) {
    // A frame-latency timeout does not produce a pixel. Wait for readiness,
    // without retrying render errors or a successfully read incorrect pixel.
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
    HRESULT result{};
    do {
        result = WithWindowMessages([&] {
            return host.ReadPixelForSmokeTest(canvas, generation, x, y, pixel);
        });
    } while (result == S_FALSE && std::chrono::steady_clock::now() < deadline);
    return result;
}

void PrintSurfaceState(
    const char* phase,
    inkpod::renderer::RendererHost& host,
    inkpod::app::CanvasId canvas,
    inkpod::app::Generation generation,
    HWND window) noexcept {
    inkpod::renderer::RendererSurfaceResourceUsage surface{};
    const bool have_surface = host.GetSurfaceResourceUsage(canvas, generation, surface);
    RECT bounds{};
    const bool have_bounds = GetWindowRect(window, &bounds) != FALSE;
    const POINT center{
        bounds.left + (bounds.right - bounds.left) / 2,
        bounds.top + (bounds.bottom - bounds.top) / 2};
    const bool exposed = have_bounds
        && GetAncestor(WindowFromPoint(center), GA_ROOT) == GetAncestor(window, GA_ROOT);
    const auto usage = host.ResourceUsage();
    std::fprintf(stderr,
        "%s: frames=%llu queued=%llu stale=%llu rejected=%llu surface=%u visible=%u occluded=%u hwnd_visible=%u exposed=%u bounds=%ld,%ld,%ld,%ld\n",
        phase,
        static_cast<unsigned long long>(host.PresentedFrameCount(canvas, generation)),
        static_cast<unsigned long long>(usage.queued_work_count),
        static_cast<unsigned long long>(usage.stale_snapshot_count),
        static_cast<unsigned long long>(usage.queue_rejection_count),
        static_cast<unsigned>(have_surface),
        static_cast<unsigned>(surface.visible),
        static_cast<unsigned>(surface.occluded),
        static_cast<unsigned>(IsWindowVisible(window)),
        static_cast<unsigned>(exposed),
        bounds.left, bounds.top, bounds.right, bounds.bottom);
}

bool HasBounds(
    inkpod::renderer::RendererHost& host,
    inkpod::app::CanvasId canvas,
    inkpod::app::Generation generation,
    double width,
    double height) noexcept {
    for (std::uint32_t attempt = 0U; attempt < 100U; ++attempt) {
        inkpod::renderer::CanvasDocumentBounds bounds{};
        if (SUCCEEDED(WithWindowMessages([&] {
                return host.GetDocumentBounds(canvas, generation, bounds);
            }))
            && bounds.left == 0.0 && bounds.top == 0.0
            && bounds.right == width && bounds.bottom == height) {
            return true;
        }
        Sleep(1U);
    }
    return false;
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
        336, 0, 320, 240, nullptr, nullptr, instance, nullptr);
    if (first_parent.window_ == nullptr || second_parent.window_ == nullptr) {
        return 3;
    }

    // Canvas creation queues its first visibility/resize renders. Establish
    // visible, nonoverlapping parents before those renders can consume the
    // swap chains' frame-latency signals, without changing keyboard focus.
    ShowWindow(first_parent.window_, SW_SHOWNOACTIVATE);
    ShowWindow(second_parent.window_, SW_SHOWNOACTIVATE);
    if (!SetWindowPos(first_parent.window_, HWND_TOPMOST, 0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
        || !SetWindowPos(second_parent.window_, HWND_TOPMOST, 0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)) {
        return 38;
    }
    UpdateWindow(first_parent.window_);
    UpdateWindow(second_parent.window_);
    PumpWindowMessages();

    first_canvas_window.window_ = inkpod::renderer::CreateCanvasWindow(
        instance,
        first_parent.window_,
        host,
        first_canvas,
        first_surface_generation);
    if (first_canvas_window.window_ == nullptr) {
        return 4;
    }
    PumpWindowMessages();
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)) {
        return 24;
    }
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

    PumpWindowMessages();
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)) {
        return 24;
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
    if (!first_core.Create(1U, 32U, 24U)
        || !first_core.ConfigureRasterContent()
        || !second_core.Create(2U, 48U, 16U)) {
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
    host.Resize(first_canvas, first_surface_generation, 32U, 24U);
    inkpod::renderer::CanvasPixelRgba8 ordered_pixel{};
    const HRESULT initial_pixel_result = ReadPresentedPixel(
        host, first_canvas, first_surface_generation, 16U, 12U, ordered_pixel);
    if (initial_pixel_result != S_OK
        || ordered_pixel.red != 0U || ordered_pixel.green != 0U
        || ordered_pixel.blue != 255U) {
        std::fprintf(stderr, "initial snapshot: result=%08lx rgb=%u,%u,%u\n",
            static_cast<unsigned long>(initial_pixel_result),
            static_cast<unsigned>(ordered_pixel.red),
            static_cast<unsigned>(ordered_pixel.green),
            static_cast<unsigned>(ordered_pixel.blue));
        PrintSurfaceState("initial readback", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 35;
    }
    inkpod::renderer::SnapshotEnvelope adjusted_envelope{};
    const bool adjustment_created = first_core.CreateAdjustmentTop();
    const bool adjustment_built = adjustment_created
        && first_core.Build(first_sink->Route(), adjusted_envelope);
    const bool adjustment_submitted = adjustment_built
        && first_sink->Submit(adjusted_envelope);
    const HRESULT adjustment_pixel_result = adjustment_submitted
        ? ReadPresentedPixel(
            host, first_canvas, first_surface_generation, 16U, 12U, ordered_pixel)
        : E_UNEXPECTED;
    if (!adjustment_submitted || adjustment_pixel_result != S_OK
        || ordered_pixel.red < 24U || ordered_pixel.red > 27U
        || ordered_pixel.green < 24U || ordered_pixel.green > 27U
        || ordered_pixel.blue != 255U) {
        std::fprintf(stderr,
            "adjustment snapshot: created=%u built=%u submitted=%u result=%08lx rgb=%u,%u,%u\n",
            static_cast<unsigned>(adjustment_created),
            static_cast<unsigned>(adjustment_built),
            static_cast<unsigned>(adjustment_submitted),
            static_cast<unsigned long>(adjustment_pixel_result),
            static_cast<unsigned>(ordered_pixel.red),
            static_cast<unsigned>(ordered_pixel.green),
            static_cast<unsigned>(ordered_pixel.blue));
        PrintSurfaceState("adjustment readback", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 37;
    }
    const inkpod::renderer::RendererResourceUsage initial_usage = host.ResourceUsage();
    inkpod::renderer::RendererSurfaceResourceUsage first_surface_usage{};
    if (initial_usage.gpu_tile_budget_bytes == 0U
        || initial_usage.surface_count != 2U
        || initial_usage.visible_surface_count != 2U
        || initial_usage.retained_snapshot_bytes == 0U
        || initial_usage.gpu_tile_bytes > initial_usage.gpu_tile_budget_bytes
        || initial_usage.active_tile_count > initial_usage.cached_tile_count
        || !host.GetSurfaceResourceUsage(
            first_canvas, first_surface_generation, first_surface_usage)
        || first_surface_usage.route != first_sink->Route()
        || first_surface_usage.retained_snapshot_bytes == 0U
        || !first_surface_usage.visible || first_surface_usage.occluded
        || host.GetSurfaceResourceUsage(
            first_canvas, Generation{999U}, first_surface_usage)) {
        return 30;
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
    const inkpod::renderer::RendererResourceUsage paused_usage = host.ResourceUsage();
    if (paused_usage.pending_snapshot_bytes == 0U
        || paused_usage.queue_replacement_count == 0U) {
        host.SetQueuePausedForSmokeTest(false);
        return 31;
    }
    host.SetQueuePausedForSmokeTest(false);
    if (!HasBounds(host, first_canvas, first_surface_generation, 40.0, 20.0)) {
        return 22;
    }

    PumpWindowMessages();
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)) {
        return 24;
    }
    host.SetQueuePausedForSmokeTest(true);
    const std::uint64_t frames_before_queue_drain = host.PresentedFrameCount(
        first_canvas, first_surface_generation);
    for (std::size_t index = 0U; index < 256U; ++index) {
        host.RequestRender(first_canvas, first_surface_generation);
    }
    const std::uint64_t queued_render_work =
        host.ResourceUsage().queued_work_count;
    inkpod::renderer::SnapshotEnvelope queue_failure{};
    if (!replacement_core.Build(first_sink->Route(), queue_failure)
        || first_sink->Submit(queue_failure)) {
        host.SetQueuePausedForSmokeTest(false);
        return 23;
    }
    host.SetQueuePausedForSmokeTest(false);
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)) {
        return 24;
    }
    if (host.ResourceUsage().queue_rejection_count == 0U
        || queued_render_work == 0U
        || host.PresentedFrameCount(first_canvas, first_surface_generation)
            != frames_before_queue_drain + queued_render_work) {
        std::fprintf(stderr,
            "queue drain: before=%llu after=%llu queued=%llu\n",
            static_cast<unsigned long long>(frames_before_queue_drain),
            static_cast<unsigned long long>(host.PresentedFrameCount(
                first_canvas, first_surface_generation)),
            static_cast<unsigned long long>(queued_render_work));
        PrintSurfaceState("drained surface", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 32;
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
    if (host.ResourceUsage().device_reset_count == 0U) {
        return 33;
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
    if (host.ResourceUsage().stale_snapshot_count == 0U) {
        return 34;
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

    const inkpod::renderer::SnapshotRoute before_unbind = first_sink->Route();
    inkpod::renderer::SnapshotEnvelope unbound_stale{};
    inkpod::renderer::CanvasDocumentBounds unbound_bounds{};
    inkpod::renderer::RendererSurfaceResourceUsage unbound_usage{};
    inkpod::renderer::CanvasPixelRgba8 empty_pixel{};
    if (!rebound_core.Build(before_unbind, unbound_stale)
        || !inkpod::renderer::UnbindCanvasSnapshotSink(
            first_canvas_window.window_)
        || first_sink->AcceptsSnapshots() || first_sink->Route()
        || host.Submit(unbound_stale)
        || FAILED(host.GetDocumentBounds(
            first_canvas, first_surface_generation, unbound_bounds))
        || unbound_bounds.right != 0.0 || unbound_bounds.bottom != 0.0
        || !host.GetSurfaceResourceUsage(first_canvas, first_surface_generation, unbound_usage)
        || unbound_usage.route || unbound_usage.retained_snapshot_bytes != 0U
        || unbound_usage.gpu_tile_bytes != 0U || unbound_usage.active_tile_count != 0U
        || ReadPresentedPixel(host, first_canvas, first_surface_generation, 8U, 8U, empty_pixel) != S_OK
        || empty_pixel.red < 30U || empty_pixel.red > 31U
        || empty_pixel.green < 33U || empty_pixel.green > 34U
        || empty_pixel.blue < 38U || empty_pixel.blue > 39U
        || !HasBounds(host, second_canvas, second_surface_generation, 48.0, 16.0)
        || !inkpod::renderer::BindCanvasSnapshotSink(
            first_canvas_window.window_,
            DocumentSessionId(203U),
            DocumentViewId(303U),
            Generation(22U))) {
        return 29;
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
        || host.SurfaceCount() != 0U
        || host.ResourceUsage().surface_count != 0U
        || host.GetSurfaceResourceUsage(
            second_canvas, second_surface_generation, first_surface_usage)) {
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
