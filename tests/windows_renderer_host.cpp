#include <windows.h>

#include <array>
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
        if (inkpod_core_new_cell(core_, &options, &info) != INKPOD_STATUS_OK) {
            return false;
        }
        document_ = info;
        return true;
    }

    bool ConfigureMixedOrder() noexcept {
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
            sizeof(InkpodStrokeSample)};
        InkpodDispatchResult dispatch{};
        dispatch.struct_size = sizeof(dispatch);
        if (inkpod_core_apply_stroke(core_, &stroke, &dispatch) != INKPOD_STATUS_OK) {
            return false;
        }
        constexpr std::array<std::uint8_t, 12U> name{
            'O', 'r', 'd', 'e', 'r', 'e', 'd', ' ', 'V', 'e', 'c', 't'};
        InkpodTreeEdit edit{};
        edit.struct_size = sizeof(edit);
        edit.operation = INKPOD_TREE_CREATE_LAYER;
        edit.kind = INKPOD_LAYER_VECTOR_COLORING;
        edit.name_utf8 = name.data();
        edit.name_bytes = name.size();
        if (inkpod_core_tree_edit(core_, &edit, &dispatch, &vector_layer_id_)
                != INKPOD_STATUS_OK
            || vector_layer_id_ == 0U) {
            return false;
        }
        std::uint32_t vector_index = UINT32_MAX;
        InkpodNodeInfo layer{};
        layer.struct_size = sizeof(layer);
        for (std::uint32_t index = 0U; index < 32U; ++index) {
            if (inkpod_core_node_get(core_, index, UINT32_MAX, &layer) != INKPOD_STATUS_OK) {
                break;
            }
            if (layer.id == vector_layer_id_) {
                vector_index = index;
                break;
            }
        }
        InkpodNodeInfo plane{};
        plane.struct_size = sizeof(plane);
        if (vector_index == UINT32_MAX
            || inkpod_core_node_get(core_, vector_index, 1U, &plane) != INKPOD_STATUS_OK
            || plane.kind != INKPOD_TYPED_PLANE_COLOR_TRACE) {
            return false;
        }
        const std::uint64_t trace_plane_id = plane.id;
        if (inkpod_core_node_get(core_, vector_index, 2U, &plane) != INKPOD_STATUS_OK
            || plane.kind != INKPOD_TYPED_PLANE_VECTOR_FILL) {
            return false;
        }
        const std::uint64_t fill_plane_id = plane.id;
        const auto line = [](InkpodVectorPoint start, InkpodVectorPoint end) noexcept {
            return InkpodVectorCubicSegment{
                sizeof(InkpodVectorCubicSegment),
                0U,
                start,
                InkpodVectorPoint{
                    (start.x * 2.0F + end.x) / 3.0F,
                    (start.y * 2.0F + end.y) / 3.0F},
                InkpodVectorPoint{
                    (start.x + end.x * 2.0F) / 3.0F,
                    (start.y + end.y * 2.0F) / 3.0F},
                end,
                1.0F,
                1.0F};
        };
        const std::array<InkpodVectorPoint, 5U> points{
            InkpodVectorPoint{-4.0F, -4.0F},
            InkpodVectorPoint{static_cast<float>(document_.width) + 4.0F, -4.0F},
            InkpodVectorPoint{
                static_cast<float>(document_.width) + 4.0F,
                static_cast<float>(document_.height) + 4.0F},
            InkpodVectorPoint{-4.0F, static_cast<float>(document_.height) + 4.0F},
            InkpodVectorPoint{-4.0F, -4.0F}};
        const std::array<InkpodVectorCubicSegment, 4U> segments{
            line(points[0], points[1]),
            line(points[1], points[2]),
            line(points[2], points[3]),
            line(points[3], points[4])};
        const InkpodVectorPathInput path{
            sizeof(InkpodVectorPathInput),
            0U,
            INKPOD_VECTOR_PATH_CLOSED,
            trace_plane_id,
            InkpodColorValue{
                sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 0U},
            segments.data(),
            segments.size(),
            sizeof(InkpodVectorCubicSegment)};
        std::uint64_t path_id{};
        if (inkpod_core_vector_add_path(core_, &path, &dispatch, &path_id)
                != INKPOD_STATUS_OK
            || path_id == 0U) {
            return false;
        }
        const InkpodVectorFillInput fill{
            sizeof(InkpodVectorFillInput),
            0U,
            0U,
            fill_plane_id,
            InkpodColorValue{
                sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 255U, 0U, 0U, 255U},
            &path_id,
            1U};
        std::uint64_t fill_id{};
        return inkpod_core_vector_add_fill(core_, &fill, &dispatch, &fill_id)
                == INKPOD_STATUS_OK
            && fill_id != 0U;
    }

    bool ReorderVectorTop() noexcept {
        InkpodTreeEdit edit{};
        edit.struct_size = sizeof(edit);
        edit.operation = INKPOD_TREE_REORDER_LAYER;
        edit.object_id = vector_layer_id_;
        edit.destination_index = 0U;
        InkpodDispatchResult dispatch{};
        dispatch.struct_size = sizeof(dispatch);
        std::uint64_t ignored{};
        return vector_layer_id_ != 0U
            && inkpod_core_tree_edit(core_, &edit, &dispatch, &ignored) == INKPOD_STATUS_OK;
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
    std::uint64_t vector_layer_id_{};
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
    if (!first_core.Create(1U, 32U, 24U)
        || !first_core.ConfigureMixedOrder()
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
    if (FAILED(host.ReadPixelForSmokeTest(
            first_canvas, first_surface_generation, 16U, 12U, ordered_pixel))
        || ordered_pixel.red != 0U || ordered_pixel.green != 0U
        || ordered_pixel.blue != 255U) {
        return 35;
    }
    inkpod::renderer::SnapshotEnvelope reordered_envelope{};
    if (!first_core.ReorderVectorTop()
        || !first_core.Build(first_sink->Route(), reordered_envelope)
        || !first_sink->Submit(reordered_envelope)
        || FAILED(host.ReadPixelForSmokeTest(
            first_canvas, first_surface_generation, 16U, 12U, ordered_pixel))
        || ordered_pixel.red != 255U || ordered_pixel.green != 0U
        || ordered_pixel.blue != 0U) {
        return 36;
    }
    inkpod::renderer::SnapshotEnvelope adjusted_envelope{};
    if (!first_core.CreateAdjustmentTop()
        || !first_core.Build(first_sink->Route(), adjusted_envelope)
        || !first_sink->Submit(adjusted_envelope)
        || FAILED(host.ReadPixelForSmokeTest(
            first_canvas, first_surface_generation, 16U, 12U, ordered_pixel))
        || ordered_pixel.red != 255U
        || ordered_pixel.green < 24U || ordered_pixel.green > 27U
        || ordered_pixel.blue < 24U || ordered_pixel.blue > 27U) {
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
    if (!host.WaitQueueIdleForSmokeTest()) {
        return 24;
    }
    if (host.ResourceUsage().queue_rejection_count == 0U
        || queued_render_work == 0U
        || host.PresentedFrameCount(first_canvas, first_surface_generation)
            != frames_before_queue_drain + queued_render_work) {
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
    if (!rebound_core.Build(before_unbind, unbound_stale)
        || !inkpod::renderer::UnbindCanvasSnapshotSink(
            first_canvas_window.window_)
        || first_sink->AcceptsSnapshots() || first_sink->Route()
        || host.Submit(unbound_stale)
        || FAILED(host.GetDocumentBounds(
            first_canvas, first_surface_generation, unbound_bounds))
        || unbound_bounds.right != 0.0 || unbound_bounds.bottom != 0.0
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
