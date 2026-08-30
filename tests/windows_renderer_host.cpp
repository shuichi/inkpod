#include <windows.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <climits>
#include <cstddef>
#include <cstdio>
#include <cstdint>
#include <future>
#include <string>
#include <utility>
#include <vector>

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

    bool ConfigureRasterContent(bool preview = false) noexcept {
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
            std::min(256.0F, static_cast<float>(document_.width + document_.height) * 4.0F),
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
        if (preview) {
            return inkpod_core_stroke_begin(core_, &stroke) == INKPOD_STATUS_OK;
        }
        if (inkpod_core_apply_stroke(core_, &stroke, &dispatch) != INKPOD_STATUS_OK) {
            return false;
        }
        return true;
    }

    bool CancelPreview() noexcept {
        return inkpod_core_stroke_cancel(core_) == INKPOD_STATUS_OK;
    }

    bool ConfigureSequence(std::uint8_t first_red = 32U, std::uint32_t width = 64U) {
        constexpr std::size_t count = 10U;
        constexpr std::uint32_t height = 32U;
        std::array<std::vector<std::uint8_t>, count> pixels;
        std::array<std::string, count> names;
        std::array<InkpodSequenceCellInput, count> cells{};
        for (std::size_t index = 0U; index < count; ++index) {
            pixels[index].resize(width * height * 4U);
            const auto red = static_cast<std::uint8_t>(first_red + index * 16U);
            for (std::size_t offset = 0U; offset < pixels[index].size(); offset += 4U) {
                pixels[index][offset] = red;
                pixels[index][offset + 1U] = 64U;
                pixels[index][offset + 2U] = 96U;
                pixels[index][offset + 3U] = 255U;
            }
            names[index] = "source" + std::to_string(index + 1U) + ".tga";
            InkpodRasterSourceInput source{};
            source.struct_size = sizeof(source);
            source.pixel_format = INKPOD_STORAGE_RGBA8;
            source.document_uuid_high = UINT64_C(0x736f757263656361);
            source.document_uuid_low = index + 1U;
            source.source_revision = 1U;
            source.width = width;
            source.height = height;
            source.dpi_x_milli = 96000U;
            source.dpi_y_milli = 96000U;
            source.reference_frame = InkpodFrameRect{0, 0, static_cast<std::int32_t>(width), height};
            source.pixels = pixels[index].data();
            source.pixel_bytes = pixels[index].size();
            source.row_stride_bytes = width * 4U;
            cells[index] = InkpodSequenceCellInput{
                sizeof(InkpodSequenceCellInput), 0U,
                reinterpret_cast<const std::uint8_t*>(names[index].data()),
                names[index].size(), source};
        }
        const InkpodSequenceInput input{
            sizeof(InkpodSequenceInput), 0U, INKPOD_FEATURE_NONE,
            cells.data(), cells.size(), sizeof(InkpodSequenceCellInput)};
        return inkpod_core_sequence_set(core_, &input) == INKPOD_STATUS_OK;
    }

    bool SelectSequence(std::uint32_t index) noexcept {
        document_.struct_size = sizeof(document_);
        return inkpod_core_sequence_activate(core_, index, &document_) == INKPOD_STATUS_OK;
    }

    bool Undo() noexcept {
        InkpodDispatchResult result{};
        result.struct_size = sizeof(result);
        return inkpod_core_undo(core_, &result) == INKPOD_STATUS_OK;
    }

    void SetPresentationEpoch(std::uint64_t epoch) noexcept {
        presentation_epoch_ = epoch;
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
        InkpodDocumentInfo committed = document_;
        committed.struct_size = sizeof(committed);
        if (inkpod_snapshot_get_view(snapshot, &view) != INKPOD_STATUS_OK
            || inkpod_snapshot_get_transform(snapshot, &transform) != INKPOD_STATUS_OK
            || inkpod_core_get_document_info(core_, &committed) != INKPOD_STATUS_OK) {
            inkpod_snapshot_release(&snapshot);
            return false;
        }
        envelope = inkpod::renderer::SnapshotEnvelope{
            route, view.revision, transform.view_revision, snapshot, 0U,
            committed.document_revision, 0U, presentation_epoch_};
        return true;
    }

private:
    InkpodCore* core_{};
    InkpodDocumentInfo document_{};
    std::uint64_t presentation_epoch_{};
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

class StrokeObserver final {
public:
    explicit StrokeObserver(HWND canvas) noexcept
        : canvas_(canvas), parent_(GetParent(canvas)),
          previous_data_(GetWindowLongPtrW(parent_, GWLP_USERDATA)),
          previous_procedure_(reinterpret_cast<WNDPROC>(
              GetWindowLongPtrW(parent_, GWLP_WNDPROC))) {
        SetWindowLongPtrW(parent_, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(this));
        installed_ = SetWindowLongPtrW(parent_, GWLP_WNDPROC,
            reinterpret_cast<LONG_PTR>(&Procedure)) != 0;
    }

    ~StrokeObserver() {
        inkpod::renderer::CancelCanvasStroke(canvas_);
        (void)inkpod::renderer::SetCanvasSequenceFence(canvas_, false, 0U, 0U);
        if (GetCapture() == canvas_) {
            ReleaseCapture();
        }
        if (installed_) {
            SetWindowLongPtrW(parent_, GWLP_WNDPROC,
                reinterpret_cast<LONG_PTR>(previous_procedure_));
        }
        SetWindowLongPtrW(parent_, GWLP_USERDATA, previous_data_);
    }

    bool installed_{};
    bool pending_gate_rejected_{true};
    std::array<std::uint32_t, 4U> received_{};

private:
    static LRESULT CALLBACK Procedure(
        HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
        auto* observer = reinterpret_cast<StrokeObserver*>(
            GetWindowLongPtrW(window, GWLP_USERDATA));
        if (message == inkpod::renderer::kCanvasStrokeReady) {
            observer->pending_gate_rejected_ = observer->pending_gate_rejected_
                && !inkpod::renderer::SetCanvasSequenceFence(observer->canvas_, true, 0U, 0U);
            inkpod::renderer::OwnedCanvasStrokeEvent event{};
            if (!inkpod::renderer::TakeCanvasStrokeEvent(observer->canvas_,
                    static_cast<std::uint64_t>(wparam),
                    inkpod::app::Generation(static_cast<std::uint64_t>(lparam)), event)) {
                return 0;
            }
            ++observer->received_[static_cast<std::size_t>(event.kind)];
            return 1;
        }
        return CallWindowProcW(observer->previous_procedure_, window, message, wparam, lparam);
    }

    HWND canvas_{};
    HWND parent_{};
    LONG_PTR previous_data_{};
    WNDPROC previous_procedure_{};
};

bool VerifyCanvasSequenceFence(
    inkpod::renderer::RendererHost& host,
    HWND canvas,
    const inkpod::renderer::SnapshotRoute& route) {
    using inkpod::renderer::CanvasStrokeEventKind;
    inkpod::renderer::RendererSurfaceResourceUsage usage{};
    if (!host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, usage)
        || usage.last_presented_document_revision == 0U) {
        return false;
    }
    const InkpodStrokeSample sample{
        sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U};
    const auto submit = [&](CanvasStrokeEventKind kind) {
        return inkpod::renderer::SubmitCanvasStrokeEvent(canvas, kind, &sample, 1U);
    };
    StrokeObserver observer(canvas);
    if (!observer.installed_
        || !inkpod::renderer::SetCanvasSequenceFence(canvas, false,
            usage.last_presented_document_revision, usage.last_presented_presentation_epoch)
        || SendMessageW(canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(10, 10)) != 1
        || inkpod::renderer::SetCanvasSequenceFence(canvas, true, 0U, 0U)
        || !inkpod::renderer::SetCanvasSequenceFence(canvas, false,
            usage.last_presented_document_revision + 1U, usage.last_presented_presentation_epoch)
        || SendMessageW(canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(11, 10)) != 1
        || SendMessageW(canvas, WM_LBUTTONUP, 0U, MAKELPARAM(12, 10)) != 1
        || observer.received_[0] != 1U || observer.received_[1] != 1U
        || observer.received_[2] != 1U || submit(CanvasStrokeEventKind::Begin)
        || !inkpod::renderer::SetCanvasSequenceFence(canvas, true, 0U, 0U)
        || submit(CanvasStrokeEventKind::Begin)
        || !inkpod::renderer::SetCanvasSequenceFence(canvas, false,
            usage.last_presented_document_revision, usage.last_presented_presentation_epoch)
        || SendMessageW(canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(10, 10)) != 1
        || !inkpod::renderer::SetCanvasSequenceFence(canvas, false,
            usage.last_presented_document_revision + 1U, usage.last_presented_presentation_epoch)) {
        return false;
    }
    inkpod::renderer::CancelCanvasStroke(canvas);
    return observer.pending_gate_rejected_ && observer.received_[0] == 2U
        && observer.received_[3] == 1U;
}

bool VerifyPresentationEpochFence(
    inkpod::renderer::RendererHost& host,
    HWND canvas,
    inkpod::renderer::CanvasSnapshotSink& sink,
    CoreOwner& core,
    std::uint64_t epoch,
    bool same_revision,
    std::uint8_t red) {
    const auto route = sink.Route();
    inkpod::renderer::RendererSurfaceResourceUsage before{};
    if (!host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, before)) {
        return false;
    }
    core.SetPresentationEpoch(epoch);
    inkpod::renderer::SnapshotEnvelope replacement{};
    if (!core.Build(route, replacement)) {
        return false;
    }
    const auto revision = replacement.committed_document_revision;
    StrokeObserver observer(canvas);
    const InkpodStrokeSample sample{
        sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U};
    const auto begin = [&] {
        return inkpod::renderer::SubmitCanvasStrokeEvent(canvas,
            inkpod::renderer::CanvasStrokeEventKind::Begin, &sample, 1U);
    };
    // Recovery/Core replacement can keep or lower the revision. Neither the
    // old revision nor an accepted-but-unpresented queue item releases input.
    if (!observer.installed_
        || (same_revision ? revision != before.last_presented_document_revision
                          : revision >= before.last_presented_document_revision)
        || !inkpod::renderer::SetCanvasSequenceFence(canvas, true, revision, epoch)
        || begin()
        || !inkpod::renderer::SetCanvasSequenceFence(canvas, false, revision, epoch)
        || begin()) {
        inkpod_snapshot_release(&replacement.snapshot);
        return false;
    }
    host.SetQueuePausedForSmokeTest(true);
    const bool accepted = sink.Submit(replacement);
    inkpod::renderer::RendererSurfaceResourceUsage queued{};
    const bool have_queued = host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, queued);
    const bool blocked_while_queued = !begin();
    host.SetQueuePausedForSmokeTest(false);
    inkpod::renderer::CanvasPixelRgba8 pixel{};
    inkpod::renderer::RendererSurfaceResourceUsage presented{};
    if (!accepted || !have_queued || !blocked_while_queued
        || queued.last_presented_presentation_epoch != before.last_presented_presentation_epoch
        || ReadPresentedPixel(host, route.canvas, route.surface_generation, 16U, 12U, pixel) != S_OK
        || pixel.red != red || pixel.green != 64U || pixel.blue != 96U
        || !host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, presented)
        || presented.last_presented_document_revision != revision
        || presented.last_presented_presentation_epoch != epoch
        || presented.first_presented_revision_qpc <= before.first_presented_revision_qpc
        || presented.first_frame_ready_qpc < presented.last_snapshot_submission_qpc
        || presented.first_present_begin_qpc < presented.first_frame_ready_qpc
        || presented.first_presented_revision_qpc < presented.first_present_begin_qpc
        || (same_revision && presented.uploaded_tile_count != before.uploaded_tile_count)
        || !begin()
        || !inkpod::renderer::SubmitCanvasStrokeEvent(canvas,
            inkpod::renderer::CanvasStrokeEventKind::Cancel, nullptr, 0U)) {
        return false;
    }
    const auto first_present = presented.first_presented_revision_qpc;
    const auto first_ready = presented.first_frame_ready_qpc;
    const auto first_present_begin = presented.first_present_begin_qpc;
    return ReadPresentedPixel(host, route.canvas, route.surface_generation, 16U, 12U, pixel) == S_OK
        && host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, presented)
        && presented.first_presented_revision_qpc == first_present
        && presented.first_frame_ready_qpc == first_ready
        && presented.first_present_begin_qpc == first_present_begin
        && presented.last_presented_presentation_epoch == epoch
        && observer.received_[0] == 1U && observer.pending_gate_rejected_;
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
        "%s: frames=%llu queued=%llu stale=%llu rejected=%llu surface=%u visible=%u occluded=%u hwnd_visible=%u exposed=%u bounds=%ld,%ld,%ld,%ld latency_timeouts=%llu last_render=%08lx\n",
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
        bounds.left, bounds.top, bounds.right, bounds.bottom,
        static_cast<unsigned long long>(surface.frame_latency_timeout_count),
        static_cast<unsigned long>(surface.last_render_result));
}

class RendererPhaseDiagnostics final {
public:
    RendererPhaseDiagnostics(
        inkpod::renderer::RendererHost& host,
        inkpod::app::CanvasId canvas,
        inkpod::app::Generation generation,
        const HWND& window) noexcept
        : host_(host), canvas_(canvas), generation_(generation), window_(window),
          before_(Capture()) {}

    void Report(const char* phase) noexcept {
        const Sample after = Capture();
        const double elapsed_ms = std::chrono::duration<double, std::milli>(
            after.time - before_.time).count();
        const bool continuous = before_.available && after.available
            && after.frames >= before_.frames && after.timeouts >= before_.timeouts;
        // Observe existing phase boundaries only: no extra render, message
        // pump, readiness wait, or elapsed-time pass/fail condition is added.
        // State pairs are before/after samples, not continuous monitoring.
        std::fprintf(stderr,
            "renderer_phase phase=%s elapsed_ms=%.3f frames_before=%llu frames_after=%llu presented_delta=%llu timeouts_before=%llu timeouts_after=%llu timeout_delta=%llu counters_continuous=%u surface=%u/%u foreground=%u/%u visible=%u/%u occluded=%u/%u hwnd_visible=%u/%u exposed=%u/%u\n",
            phase, elapsed_ms,
            static_cast<unsigned long long>(before_.frames),
            static_cast<unsigned long long>(after.frames),
            static_cast<unsigned long long>(continuous ? after.frames - before_.frames : 0U),
            static_cast<unsigned long long>(before_.timeouts),
            static_cast<unsigned long long>(after.timeouts),
            static_cast<unsigned long long>(continuous ? after.timeouts - before_.timeouts : 0U),
            static_cast<unsigned>(continuous),
            static_cast<unsigned>(before_.available), static_cast<unsigned>(after.available),
            static_cast<unsigned>(before_.foreground), static_cast<unsigned>(after.foreground),
            static_cast<unsigned>(before_.visible), static_cast<unsigned>(after.visible),
            static_cast<unsigned>(before_.occluded), static_cast<unsigned>(after.occluded),
            static_cast<unsigned>(before_.hwnd_visible), static_cast<unsigned>(after.hwnd_visible),
            static_cast<unsigned>(before_.exposed), static_cast<unsigned>(after.exposed));
        before_ = after;
        before_.time = std::chrono::steady_clock::now();
    }

private:
    struct Sample {
        std::chrono::steady_clock::time_point time;
        std::uint64_t frames{};
        std::uint64_t timeouts{};
        bool available{};
        bool foreground{};
        bool visible{};
        bool occluded{};
        bool hwnd_visible{};
        bool exposed{};
    };

    Sample Capture() const noexcept {
        Sample sample{};
        sample.time = std::chrono::steady_clock::now();
        sample.frames = host_.PresentedFrameCount(canvas_, generation_);
        inkpod::renderer::RendererSurfaceResourceUsage surface{};
        sample.available = host_.GetSurfaceResourceUsage(canvas_, generation_, surface);
        sample.timeouts = surface.frame_latency_timeout_count;
        sample.visible = surface.visible;
        sample.occluded = surface.occluded;
        sample.hwnd_visible = IsWindowVisible(window_) != FALSE;
        const HWND root = GetAncestor(window_, GA_ROOT);
        sample.foreground = root != nullptr && GetForegroundWindow() == root;
        RECT bounds{};
        if (root != nullptr && GetWindowRect(window_, &bounds) != FALSE) {
            const POINT center{
                bounds.left + (bounds.right - bounds.left) / 2,
                bounds.top + (bounds.bottom - bounds.top) / 2};
            sample.exposed = GetAncestor(WindowFromPoint(center), GA_ROOT) == root;
        }
        return sample;
    }

    inkpod::renderer::RendererHost& host_;
    inkpod::app::CanvasId canvas_;
    inkpod::app::Generation generation_;
    const HWND& window_;
    Sample before_;
};

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

bool VerifyPreviewPublicationAndFramePermit(
    inkpod::renderer::RendererHost& host,
    const inkpod::renderer::SnapshotRoute& route) {
    using inkpod::renderer::CanvasGeometryPreview;
    using inkpod::renderer::CanvasPixelRgba8;
    using inkpod::renderer::RendererSurfaceResourceUsage;
    const auto frames = [&] {
        return host.PresentedFrameCount(route.canvas, route.surface_generation);
    };
    const auto usage = [&] {
        RendererSurfaceResourceUsage value{};
        (void)host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, value);
        return value;
    };
    const auto set_geometry = [&](const CanvasGeometryPreview& preview) {
        const HRESULT result = WithWindowMessages([&] {
            return host.SetGeometryPreview(route.canvas, route.surface_generation, preview);
        }, false);
        if (result == S_OK
            && !WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)) {
            return E_FAIL;
        }
        return result;
    };
    CanvasGeometryPreview empty{};
    empty.struct_size = sizeof(empty);
    CanvasPixelRgba8 background{};
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)
        || ReadPresentedPixel(host, route.canvas, route.surface_generation,
            16U, 8U, background) != S_OK) {
        return false;
    }
    const auto empty_frames = frames();
    const auto empty_usage = usage();
    if (set_geometry(empty) != S_FALSE || set_geometry(empty) != S_FALSE
        || set_geometry(empty) != S_FALSE) {
        return false;
    }
    host.Resize(route.canvas, route.surface_generation, 32U, 24U);
    inkpod::renderer::CanvasFloatingPreview floating{};
    floating.struct_size = sizeof(floating);
    floating.transform = InkpodFloatingTransform{
        sizeof(InkpodFloatingTransform), INKPOD_TRANSFORM_ANCHOR_TOP_LEFT,
        0.0, 0.0, 1.0, 1.0, 0.0};
    if (WithWindowMessages([&] {
            return host.SetFloatingPreview(route.canvas, route.surface_generation, floating);
        }, false) != S_FALSE
        || frames() != empty_frames
        || usage().frame_latency_timeout_count != empty_usage.frame_latency_timeout_count) {
        return false;
    }

    CanvasGeometryPreview line = empty;
    line.active = 1U;
    line.point_count = 2U;
    line.stroke_width = 3.0F;
    line.points[0] = {8.0F, 8.0F};
    line.points[1] = {24.0F, 8.0F};
    if (set_geometry(line) != S_OK || frames() != empty_frames + 1U
        || set_geometry(line) != S_FALSE || frames() != empty_frames + 1U) {
        return false;
    }
    CanvasGeometryPreview invalid = line;
    invalid.active = 2U;
    CanvasPixelRgba8 drawn{};
    if (set_geometry(invalid) != E_INVALIDARG || frames() != empty_frames + 1U
        || ReadPresentedPixel(host, route.canvas, route.surface_generation,
               16U, 8U, drawn) != S_OK
        || (drawn.red == background.red && drawn.green == background.green
            && drawn.blue == background.blue)) {
        return false;
    }
    const auto clear_frames = frames();
    if (set_geometry(empty) != S_OK || frames() != clear_frames + 1U
        || set_geometry(empty) != S_FALSE || frames() != clear_frames + 1U
        || ReadPresentedPixel(host, route.canvas, route.surface_generation,
               16U, 8U, drawn) != S_OK
        || drawn.red != background.red || drawn.green != background.green
        || drawn.blue != background.blue || drawn.alpha != background.alpha) {
        return false;
    }

    // Readback validates coordinates after EndDraw. Its error is a real
    // post-wait/pre-Present failure and must not consume another frame permit.
    const auto failure_frames = frames();
    if (ReadPresentedPixel(host, route.canvas, route.surface_generation,
            UINT_MAX, UINT_MAX, drawn) != E_INVALIDARG
        || frames() != failure_frames) {
        return false;
    }
    const auto failed = usage();
    if (failed.last_render_result != E_INVALIDARG
        || WithWindowMessages([&] {
               return host.ReadPixelForSmokeTest(route.canvas, route.surface_generation,
                   16U, 8U, drawn);
           }, false) != S_OK
        || frames() != failure_frames + 1U
        || usage().frame_latency_timeout_count != failed.frame_latency_timeout_count) {
        return false;
    }
    if (ReadPresentedPixel(host, route.canvas, route.surface_generation,
            UINT_MAX, UINT_MAX, drawn) != E_INVALIDARG) {
        return false;
    }
    const auto resize_frames = frames();
    const auto before_resize = usage();
    host.Resize(route.canvas, route.surface_generation, 64U, 32U);
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)
        || frames() != resize_frames + 1U || usage().last_render_result != S_OK
        || usage().frame_latency_timeout_count != before_resize.frame_latency_timeout_count) {
        return false;
    }
    host.Resize(route.canvas, route.surface_generation, 32U, 24U);
    return WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false);
}

bool VerifySequenceSourceCache(
    inkpod::renderer::RendererHost& host,
    HWND canvas_window,
    inkpod::renderer::CanvasSnapshotSink& sink) {
    const auto route = sink.Route();
    CoreOwner core;
    const auto fail = [](const char* phase) {
        std::fprintf(stderr, "sequence cache phase: %s\n", phase);
        return false;
    };
    if (!core.Create(50U, 64U, 32U) || !core.ConfigureSequence()) {
        return fail("create sequence");
    }
    host.Resize(route.canvas, route.surface_generation, 64U, 32U);
    const auto select_and_present = [&](CoreOwner& source, std::uint32_t index,
                                        std::uint8_t red) {
        inkpod::renderer::SnapshotEnvelope envelope{};
        inkpod::renderer::CanvasPixelRgba8 pixel{};
        InkpodSnapshotSourceIdentity identity{};
        identity.struct_size = sizeof(identity);
        if (!source.SelectSequence(index) || !source.Build(sink.Route(), envelope)) {
            std::fprintf(stderr, "sequence select/build index=%u failed\n", index);
            return false;
        }
        if (inkpod_snapshot_get_source_identity(envelope.snapshot, &identity) != INKPOD_STATUS_OK
            || identity.flags != INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE
            || identity.owner_generation == 0U) {
            std::fprintf(stderr, "sequence identity index=%u flags=%u owner=%llu\n",
                index, identity.flags, static_cast<unsigned long long>(identity.owner_generation));
            inkpod_snapshot_release(&envelope.snapshot);
            return false;
        }
        const auto document_revision = envelope.committed_document_revision;
        const auto view_revision = envelope.view_revision;
        const auto presentation_epoch = envelope.presentation_epoch;
        inkpod::renderer::RendererSurfaceResourceUsage presented{};
        const bool submitted = sink.Submit(envelope);
        const HRESULT read = submitted
            ? ReadPresentedPixel(host, route.canvas, route.surface_generation, 16U, 12U, pixel)
            : E_UNEXPECTED;
        const bool valid = read == S_OK
            && pixel.red == red && pixel.green == 64U && pixel.blue == 96U
            && host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, presented)
            && presented.last_presented_document_revision == document_revision
            && presented.last_presented_view_revision == view_revision
            && presented.last_presented_presentation_epoch == presentation_epoch
            && presented.last_presented_source.flags == identity.flags
            && presented.last_presented_source.document_uuid_high == identity.document_uuid_high
            && presented.last_presented_source.document_uuid_low == identity.document_uuid_low
            && presented.last_presented_source.source_generation == identity.source_generation
            && presented.last_presented_source.owner_generation == identity.owner_generation;
        if (!valid) {
            std::fprintf(stderr,
                "sequence presentation index=%u read=%08lx rgb=%u,%u,%u expected_red=%u revision=%llu/%llu\n",
                index, static_cast<unsigned long>(read), static_cast<unsigned>(pixel.red),
                static_cast<unsigned>(pixel.green), static_cast<unsigned>(pixel.blue),
                static_cast<unsigned>(red),
                static_cast<unsigned long long>(presented.last_presented_document_revision),
                static_cast<unsigned long long>(document_revision));
        }
        return valid;
    };
    const auto usage = [&] {
        inkpod::renderer::RendererSurfaceResourceUsage value{};
        (void)host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, value);
        return value;
    };
    if (!select_and_present(core, 0U, 32U) || !select_and_present(core, 1U, 48U)) {
        return fail("first source presentations");
    }
    const auto warm = usage();
    if (!select_and_present(core, 0U, 32U) || !select_and_present(core, 1U, 48U)
        || usage().uploaded_tile_count != warm.uploaded_tile_count
        || usage().uploaded_tile_bytes != warm.uploaded_tile_bytes
        || usage().sequence_cache_source_count != 2U) {
        return fail("warm source reuse");
    }
    if (!VerifyCanvasSequenceFence(host, canvas_window, route)) {
        return fail("canvas sequence input fence");
    }
    if (!VerifyPresentationEpochFence(host, canvas_window, sink, core, 101U, true, 48U)) {
        return fail("same-revision presentation epoch");
    }
    const auto before_preview = usage();
    inkpod::renderer::SnapshotEnvelope preview{};
    inkpod::renderer::CanvasPixelRgba8 preview_pixel{};
    if (!core.ConfigureRasterContent(true) || !core.Build(sink.Route(), preview)) {
        return fail("build real stroke preview");
    }
    const auto committed_before_preview = preview.committed_document_revision;
    if (preview.document_revision <= committed_before_preview) {
        inkpod_snapshot_release(&preview.snapshot);
        return fail("real preview revision separation");
    }
    if (!sink.Submit(preview)
        || ReadPresentedPixel(host, route.canvas, route.surface_generation,
               16U, 12U, preview_pixel) != S_OK
        || usage().last_presented_document_revision != committed_before_preview
        || usage().first_presented_revision_qpc != before_preview.first_presented_revision_qpc
        || usage().first_presented_revision_qpc == 0U
        || usage().last_snapshot_submission_qpc == 0U
        || !core.CancelPreview() || !core.SelectSequence(0U)) {
        return fail("preview presentation or source activation");
    }
    inkpod::renderer::SnapshotEnvelope target{};
    if (!core.Build(sink.Route(), target)) {
        return fail("next source build");
    }
    {
        StrokeObserver observer(canvas_window);
        const InkpodStrokeSample sample{
            sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U};
        const auto begin = [&] {
            return inkpod::renderer::SubmitCanvasStrokeEvent(canvas_window,
                inkpod::renderer::CanvasStrokeEventKind::Begin, &sample, 1U);
        };
        // The old rendered preview has a high preview revision. It must not
        // unlock editing of the newly committed source before that source is shown.
        if (!observer.installed_
            || target.committed_document_revision <= committed_before_preview
            || !inkpod::renderer::SetCanvasSequenceFence(canvas_window, false,
                target.committed_document_revision, 0U)
            || begin()) {
            inkpod_snapshot_release(&target.snapshot);
            return fail("preview cannot unlock next source");
        }
        if (!sink.Submit(target)
            || ReadPresentedPixel(host, route.canvas, route.surface_generation,
                   16U, 12U, preview_pixel) != S_OK
            || preview_pixel.red != 32U || preview_pixel.green != 64U
            || preview_pixel.blue != 96U || !begin()
            || !inkpod::renderer::SubmitCanvasStrokeEvent(canvas_window,
                inkpod::renderer::CanvasStrokeEventKind::Cancel, nullptr, 0U)
            || observer.received_[0] != 1U || !observer.pending_gate_rejected_) {
            return fail("presented source unlocks input");
        }
    }
    if (!select_and_present(core, 1U, 48U)) {
        return fail("return from preview");
    }
    // Enqueueing another source must not acknowledge it as displayed. The UI
    // uses this boundary to keep editing disabled until the target is presented.
    inkpod::renderer::SnapshotEnvelope pending{};
    if (!core.SelectSequence(0U) || !core.Build(sink.Route(), pending)) {
        return fail("queued source build");
    }
    const auto before_queued = usage();
    host.SetQueuePausedForSmokeTest(true);
    const bool accepted = sink.Submit(pending);
    const auto queued = usage();
    host.SetQueuePausedForSmokeTest(false);
    if (!accepted
        || queued.last_presented_source.document_uuid_low
            != before_queued.last_presented_source.document_uuid_low
        || queued.last_presented_document_revision
            != before_queued.last_presented_document_revision
        || !select_and_present(core, 0U, 32U)) {
        return fail("queue does not acknowledge presentation");
    }
    // Use the product Core/FFI/renderer path to fill the bounded source LRU.
    for (std::uint32_t index = 2U; index < 10U; ++index) {
        if (!select_and_present(core, index, static_cast<std::uint8_t>(32U + index * 16U))) {
            return fail("fill LRU presentations");
        }
        const auto resources = host.ResourceUsage();
        if (resources.sequence_cache_source_count > 8U
            || resources.sequence_cache_bytes > UINT64_C(128) * 1024U * 1024U
            || resources.gpu_tile_bytes > resources.gpu_tile_budget_bytes) {
            return fail("LRU resource caps");
        }
    }
    const auto before_evicted = usage();
    if (!select_and_present(core, 0U, 32U)
        || usage().uploaded_tile_bytes <= before_evicted.uploaded_tile_bytes
        || usage().sequence_cache_eviction_count == 0U) {
        return fail("evicted source uploads again");
    }
    if (!select_and_present(core, 1U, 48U)) {
        return fail("before device reset");
    }
    const auto before_reset = usage();
    if (FAILED(WithWindowMessages([&] {
            return host.SimulateDeviceLoss(route.canvas, route.surface_generation);
        }))
        || usage().sequence_cache_source_count > 1U
        || !select_and_present(core, 0U, 32U)
        || usage().uploaded_tile_bytes <= before_reset.uploaded_tile_bytes) {
        return fail("after device reset");
    }
    // A real edit must not overwrite a pristine source's cached bitmap.
    inkpod::renderer::SnapshotEnvelope edited{};
    inkpod::renderer::CanvasPixelRgba8 pixel{};
    const bool edit_created = core.ConfigureRasterContent();
    const bool edit_built = edit_created && core.Build(sink.Route(), edited);
    InkpodSnapshotView edit_view{};
    edit_view.struct_size = sizeof(edit_view);
    InkpodSnapshotSourceIdentity edit_source{};
    edit_source.struct_size = sizeof(edit_source);
    std::array<std::uint8_t, 4U> cpu_pixel{};
    if (edit_built
        && inkpod_snapshot_get_view(edited.snapshot, &edit_view) == INKPOD_STATUS_OK
        && inkpod_snapshot_get_source_identity(edited.snapshot, &edit_source) == INKPOD_STATUS_OK
        && edit_view.tile_count != 0U && edit_view.tiles[0].origin_x == 0
        && edit_view.tiles[0].origin_y == 0 && edit_view.tiles[0].width > 32U
        && edit_view.tiles[0].height > 16U) {
        const auto* at = edit_view.tiles[0].pixels + 16U * edit_view.tiles[0].stride_bytes + 32U * 4U;
        std::copy_n(at, cpu_pixel.size(), cpu_pixel.begin());
    }
    const bool edit_submitted = edit_built && sink.Submit(edited);
    const HRESULT edit_read = edit_submitted
        ? ReadPresentedPixel(host, route.canvas, route.surface_generation, 32U, 16U, pixel)
        : E_UNEXPECTED;
    const bool edit_matches = edit_read == S_OK
        && pixel.red == 0U && pixel.green == 0U && pixel.blue == 255U;
    const bool undone = edit_matches && core.Undo();
    if (!undone
        || !select_and_present(core, 1U, 48U)
        || !select_and_present(core, 0U, 32U)
        || ReadPresentedPixel(host, route.canvas, route.surface_generation, 32U, 16U, pixel) != S_OK
        || pixel.red != 32U || pixel.green != 64U || pixel.blue != 96U) {
        std::fprintf(stderr,
            "sequence edit: created=%u built=%u submitted=%u read=%08lx rgb=%u,%u,%u undo=%u\n",
            static_cast<unsigned>(edit_created), static_cast<unsigned>(edit_built),
            static_cast<unsigned>(edit_submitted), static_cast<unsigned long>(edit_read),
            static_cast<unsigned>(pixel.red), static_cast<unsigned>(pixel.green),
            static_cast<unsigned>(pixel.blue), static_cast<unsigned>(undone));
        std::fprintf(stderr, "sequence edit snapshot: actual=%llu render=%llu source=%u cpu=%u,%u,%u,%u\n",
            static_cast<unsigned long long>(edited.committed_document_revision),
            static_cast<unsigned long long>(edited.document_revision), edit_source.flags,
            static_cast<unsigned>(cpu_pixel[0]), static_cast<unsigned>(cpu_pixel[1]),
            static_cast<unsigned>(cpu_pixel[2]), static_cast<unsigned>(cpu_pixel[3]));
        return fail("edit/undo preserve source pixels");
    }
    // Reusing UUID/source-generation values in another Core must not alias.
    CoreOwner other;
    if (!other.Create(51U, 64U, 32U) || !other.ConfigureSequence(48U)
        || !other.SelectSequence(0U)
        || !VerifyPresentationEpochFence(host, canvas_window, sink, other, 202U, false, 48U)
        || !select_and_present(other, 0U, 48U)) {
        return fail("independent Core source namespace");
    }
    if (!inkpod::renderer::BindCanvasSnapshotSink(canvas_window,
            route.document_session, route.document_view,
            inkpod::app::Generation(route.document_generation.Value() + 1U))) {
        return fail("generation rebind");
    }
    const auto rebound = usage();
    const bool clean_rebind = rebound.sequence_cache_source_count == 0U
        && rebound.sequence_cache_bytes == 0U && rebound.gpu_tile_bytes == 0U
        && rebound.last_presented_document_revision == 0U
        && rebound.last_presented_view_revision == 0U
        && rebound.last_presented_source.flags == 0U
        && rebound.last_presented_source.owner_generation == 0U
        && rebound.last_snapshot_submission_qpc == 0U
        && rebound.first_presented_revision_qpc == 0U
        && rebound.first_frame_ready_qpc == 0U
        && rebound.first_present_begin_qpc == 0U
        && rebound.last_presented_presentation_epoch == 0U;
    return clean_rebind || fail("rebind releases source resources");
}

bool VerifyFirstSequenceEditTileReuse(
    inkpod::renderer::RendererHost& host,
    inkpod::renderer::CanvasSnapshotSink& sink) {
    const auto route = sink.Route();
    const auto usage = [&] {
        inkpod::renderer::RendererSurfaceResourceUsage value{};
        (void)host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, value);
        return value;
    };
    const auto read = [&](UINT x, std::uint8_t red, std::uint8_t green, std::uint8_t blue) {
        inkpod::renderer::CanvasPixelRgba8 pixel{};
        return ReadPresentedPixel(host, route.canvas, route.surface_generation, x, 16U, pixel) == S_OK
            && pixel.red == red && pixel.green == green && pixel.blue == blue;
    };
    const auto publish = [&](CoreOwner& core) {
        inkpod::renderer::SnapshotEnvelope envelope{};
        return core.Build(sink.Route(), envelope) && sink.Submit(envelope);
    };
    host.Resize(route.canvas, route.surface_generation, 128U, 32U);
    for (const bool preview : {true, false}) {
        CoreOwner core;
        std::uint64_t epoch = preview ? 401U : 501U;
        core.SetPresentationEpoch(epoch);
        if (!core.Create(epoch, 128U, 32U) || !core.ConfigureSequence(32U, 128U)
            || !core.SelectSequence(0U) || !publish(core) || !read(64U, 32U, 64U, 96U)) {
            return false;
        }
        const auto before = usage();
        inkpod::renderer::SnapshotEnvelope edited{};
        InkpodSnapshotView edited_view{};
        edited_view.struct_size = sizeof(edited_view);
        InkpodSnapshotSourceIdentity edited_source{};
        edited_source.struct_size = sizeof(edited_source);
        if (before.active_tile_count != 2U || before.sequence_cache_source_count != 1U
            || !core.ConfigureRasterContent(preview) || !core.Build(sink.Route(), edited)) {
            return false;
        }
        if (inkpod_snapshot_get_view(edited.snapshot, &edited_view) != INKPOD_STATUS_OK
            || inkpod_snapshot_get_source_identity(edited.snapshot, &edited_source) != INKPOD_STATUS_OK
            || edited_view.tile_count != 2U || edited_source.flags != 0U) {
            inkpod_snapshot_release(&edited.snapshot);
            return false;
        }
        const auto untouched = edited_view.tiles[0];
        if (!sink.Submit(edited) || !read(64U, 0U, 0U, 255U) || !read(16U, 32U, 64U, 96U)) {
            return false;
        }
        const auto after = usage();
        if (after.uploaded_tile_count != before.uploaded_tile_count + 1U
            || after.uploaded_tile_bytes != before.uploaded_tile_bytes + UINT64_C(64) * 32U * 4U
            || after.gpu_tile_bytes != before.gpu_tile_bytes
            || after.sequence_cache_source_count != 0U || after.sequence_cache_bytes != 0U) {
            std::fprintf(stderr, "first sequence %s: uploads=%llu/%llu bytes=%llu/%llu sources=%llu\n",
                preview ? "preview" : "edit",
                static_cast<unsigned long long>(before.uploaded_tile_count),
                static_cast<unsigned long long>(after.uploaded_tile_count),
                static_cast<unsigned long long>(before.gpu_tile_bytes),
                static_cast<unsigned long long>(after.gpu_tile_bytes),
                static_cast<unsigned long long>(after.sequence_cache_source_count));
            return false;
        }
        if (!preview) {
            // Replacement clears the Windows continuity epoch. This unrelated
            // Core deliberately reuses the untouched tile ID/revision while
            // carrying different pixels, so ordinary-cache handoff cannot leak.
            CoreOwner replacement;
            inkpod::renderer::SnapshotEnvelope prime{};
            inkpod::renderer::SnapshotEnvelope other{};
            if (!replacement.Create(601U, 128U, 32U)
                || !replacement.ConfigureSequence(96U, 128U) || !replacement.SelectSequence(0U)
                || !replacement.Build(sink.Route(), prime)) {
                return false;
            }
            inkpod_snapshot_release(&prime.snapshot);
            if (!replacement.ConfigureRasterContent() || !replacement.Build(sink.Route(), other)) {
                return false;
            }
            InkpodSnapshotView other_view{};
            other_view.struct_size = sizeof(other_view);
            if (inkpod_snapshot_get_view(other.snapshot, &other_view) != INKPOD_STATUS_OK
                || other_view.tile_count != 2U || other.presentation_epoch != 0U
                || other_view.tiles[0].tile_id != untouched.tile_id
                || other_view.tiles[0].tile_revision != untouched.tile_revision) {
                inkpod_snapshot_release(&other.snapshot);
                return false;
            }
            if (!sink.Submit(other) || !read(16U, 96U, 64U, 96U) || !read(64U, 0U, 0U, 255U)
                || usage().uploaded_tile_count != after.uploaded_tile_count + 2U) {
                return false;
            }
        }
        if (!(preview ? core.CancelPreview() : core.Undo())) {
            return false;
        }
        core.SetPresentationEpoch(++epoch);
        if (!core.SelectSequence(1U) || !publish(core) || !read(64U, 48U, 64U, 96U)) {
            return false;
        }
        core.SetPresentationEpoch(++epoch);
        if (!core.SelectSequence(0U) || !publish(core) || !read(64U, 32U, 64U, 96U)
            || !read(16U, 32U, 64U, 96U)) {
            return false;
        }
    }
    return SUCCEEDED(host.BindSurface(route));
}

bool VerifySaturatedVisibility(
    inkpod::renderer::RendererHost& host,
    const inkpod::renderer::SnapshotRoute& route) {
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)) {
        return false;
    }
    host.SetQueuePausedForSmokeTest(true);
    const auto before = host.PresentedFrameCount(route.canvas, route.surface_generation);
    for (std::size_t index = 0U; index < 256U; ++index) {
        host.RequestRender(route.canvas, route.surface_generation);
    }
    const auto queued = host.ResourceUsage().queued_work_count;
    host.SetVisible(route.canvas, route.surface_generation, false);
    host.SetQueuePausedForSmokeTest(false);
    inkpod::renderer::RendererSurfaceResourceUsage hidden{};
    if (queued != 248U
        || !WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)
        || !host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, hidden)
        || hidden.visible || host.ResourceUsage().queued_work_count != 0U
        || host.PresentedFrameCount(route.canvas, route.surface_generation) != before) {
        return false;
    }
    host.SetVisible(route.canvas, route.surface_generation, true);
    return WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)
        && host.SurfaceAcceptsSnapshots(route)
        && host.PresentedFrameCount(route.canvas, route.surface_generation) == before + 1U;
}

bool VerifyHiddenCanvasResize(
    inkpod::renderer::RendererHost& host,
    HWND canvas,
    inkpod::renderer::CanvasSnapshotSink& sink) {
    const auto route = sink.Route();
    const HWND parent = GetAncestor(canvas, GA_ROOT);
    RECT original{};
    if (parent == nullptr || !GetClientRect(canvas, &original)) {
        return false;
    }
    const int width = original.right - original.left;
    const int height = original.bottom - original.top;
    const auto idle = [&] {
        return WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false);
    };
    const auto resize = [&](int extra) {
        return SetWindowPos(canvas, nullptr, 0, 0, width + extra, height + extra,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW) != FALSE;
    };
    const auto hidden_resize = [&](int extra) {
        if (!idle()) {
            return false;
        }
        const auto frames = host.PresentedFrameCount(route.canvas, route.surface_generation);
        inkpod::renderer::RendererSurfaceResourceUsage usage{};
        return resize(extra) && idle()
            && host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, usage)
            && !usage.visible && !sink.AcceptsSnapshots()
            && host.PresentedFrameCount(route.canvas, route.surface_generation) == frames;
    };
    const auto restored = [&] {
        inkpod::renderer::RendererSurfaceResourceUsage usage{};
        // No child WM_SIZE accompanies a parent-only show. The first real
        // paint must synchronize effective visibility and allow snapshots.
        if (RedrawWindow(canvas, nullptr, nullptr,
                RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE) == FALSE) {
            return false;
        }
        return idle() && IsWindowVisible(canvas) != FALSE
            && IsIconic(parent) == FALSE
            && host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, usage)
            && usage.visible && sink.AcceptsSnapshots() && resize(0) && idle();
    };

    // Geometry changes send real WM_SIZE even while the child or its ancestor
    // is hidden. SIZE_RESTORED alone must never re-enable their snapshots.
    ShowWindow(canvas, SW_HIDE);
    if (!hidden_resize(1)) {
        return false;
    }
    ShowWindow(canvas, SW_SHOWNOACTIVATE);
    if (!restored()) {
        return false;
    }
    ShowWindow(parent, SW_HIDE);
    if (!hidden_resize(2)) {
        return false;
    }
    ShowWindow(parent, SW_SHOWNOACTIVATE);
    if (!restored()) {
        return false;
    }
    // IsWindowVisible still reports the WS_VISIBLE style of a minimized root;
    // the Canvas must also consult that root's iconic state.
    ShowWindow(parent, SW_SHOWMINNOACTIVE);
    if (IsIconic(parent) == FALSE || !hidden_resize(3)) {
        return false;
    }
    ShowWindow(parent, SW_SHOWNOACTIVATE);
    return restored();
}

bool VerifyPresentationDuringContinuousSnapshots(
    inkpod::renderer::RendererHost& host,
    inkpod::renderer::CanvasSnapshotSink& sink) {
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)) {
        return false;
    }
    std::atomic<bool> stop{};
    std::atomic<bool> producing{};
    auto producer = std::async(std::launch::async, [&] {
        CoreOwner core;
        core.SetPresentationEpoch(9090U);
        if (!core.Create(9090U, 64U, 32U) || !core.ConfigureRasterContent()) {
            return false;
        }
        producing.store(true, std::memory_order_release);
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
        while (!stop.load(std::memory_order_acquire) && std::chrono::steady_clock::now() < deadline) {
            inkpod::renderer::SnapshotEnvelope snapshot{};
            if (!core.Build(sink.Route(), snapshot)) {
                producing.store(false, std::memory_order_release);
                return false;
            }
            // Replacement/rejection may occur under pressure; accepted latest
            // frames must still be presented before the producer stops.
            (void)sink.Submit(snapshot);
        }
        producing.store(false, std::memory_order_release);
        return true;
    });
    bool presented_while_producing{};
    bool bounded = true;
    const auto route = sink.Route();
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
    while (producer.wait_for(std::chrono::milliseconds(0)) != std::future_status::ready
        && std::chrono::steady_clock::now() < deadline) {
        PumpWindowMessages(false);
        inkpod::renderer::RendererSurfaceResourceUsage usage{};
        bounded = bounded && host.ResourceUsage().queued_work_count <= 256U;
        if (host.GetSurfaceResourceUsage(route.canvas, route.surface_generation, usage)
            && usage.last_presented_presentation_epoch == 9090U) {
            presented_while_producing = producing.load(std::memory_order_acquire);
            break;
        }
        Sleep(1U);
    }
    stop.store(true, std::memory_order_release);
    return WithWindowMessages([&] { return producer.get(); }, false)
        && presented_while_producing && bounded
        && WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false);
}

bool VerifySharedSequenceCacheLimit(
    inkpod::renderer::RendererHost& host,
    inkpod::renderer::CanvasSnapshotSink& first,
    inkpod::renderer::CanvasSnapshotSink& second) {
    CoreOwner first_core;
    CoreOwner second_core;
    if (!first_core.Create(60U, 64U, 32U) || !first_core.ConfigureSequence()
        || !second_core.Create(61U, 64U, 32U) || !second_core.ConfigureSequence(64U)) {
        return false;
    }
    const auto present = [&](CoreOwner& core, inkpod::renderer::CanvasSnapshotSink& sink,
                              std::uint32_t index, std::uint8_t red) {
        inkpod::renderer::SnapshotEnvelope envelope{};
        inkpod::renderer::CanvasPixelRgba8 pixel{};
        const auto route = sink.Route();
        if (!core.SelectSequence(index) || !core.Build(route, envelope)
            || !sink.Submit(envelope)
            || ReadPresentedPixel(host, route.canvas, route.surface_generation,
                   16U, 12U, pixel) != S_OK
            || pixel.red != red || pixel.green != 64U || pixel.blue != 96U) {
            return false;
        }
        const auto usage = host.ResourceUsage();
        return usage.sequence_cache_source_count <= 8U
            && usage.sequence_cache_bytes <= UINT64_C(128) * 1024U * 1024U
            && usage.gpu_tile_bytes <= usage.gpu_tile_budget_bytes;
    };
    for (std::uint32_t index = 0U; index < 8U; ++index) {
        if (!present(first_core, first, index, static_cast<std::uint8_t>(32U + index * 16U))) {
            return false;
        }
    }
    const auto first_route = first.Route();
    inkpod::renderer::RendererSurfaceResourceUsage before{};
    if (!host.GetSurfaceResourceUsage(
            first_route.canvas, first_route.surface_generation, before)) {
        return false;
    }
    for (std::uint32_t index = 0U; index < 4U; ++index) {
        if (!present(second_core, second, index, static_cast<std::uint8_t>(64U + index * 16U))) {
            return false;
        }
    }
    inkpod::renderer::RendererSurfaceResourceUsage after{};
    inkpod::renderer::CanvasPixelRgba8 pixel{};
    const auto second_route = second.Route();
    return host.ResourceUsage().sequence_cache_source_count == 8U
        && host.GetSurfaceResourceUsage(
            first_route.canvas, first_route.surface_generation, after)
        && after.sequence_cache_source_count < before.sequence_cache_source_count
        && after.active_tile_count == before.active_tile_count
        && after.uploaded_tile_bytes == before.uploaded_tile_bytes
        && ReadPresentedPixel(host, first_route.canvas, first_route.surface_generation,
               16U, 12U, pixel) == S_OK
        && pixel.red == 144U && pixel.green == 64U && pixel.blue == 96U
        && SUCCEEDED(host.BindSurface(first_route))
        && SUCCEEDED(host.BindSurface(second_route));
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
    RendererPhaseDiagnostics phase(
        host, first_canvas, first_surface_generation, first_canvas_window.window_);
    first_parent.window_ = CreateWindowExW(
        0, L"STATIC", L"renderer-host-1", WS_OVERLAPPEDWINDOW,
        0, 0, 320, 240, nullptr, nullptr, instance, nullptr);
    second_parent.window_ = CreateWindowExW(
        0, L"STATIC", L"renderer-host-2", WS_OVERLAPPEDWINDOW,
        336, 0, 320, 240, nullptr, nullptr, instance, nullptr);
    if (first_parent.window_ == nullptr || second_parent.window_ == nullptr) {
        return 3;
    }

    // Cover both ordinary visible-parent creation and startup layout beneath
    // a hidden parent, without changing keyboard focus.
    ShowWindow(second_parent.window_, SW_SHOWNOACTIVATE);
    if (!SetWindowPos(first_parent.window_, HWND_TOPMOST, 0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
        || !SetWindowPos(second_parent.window_, HWND_TOPMOST, 0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)) {
        return 38;
    }
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
    inkpod::renderer::RendererSurfaceResourceUsage initially_hidden{};
    if (!SetWindowPos(first_canvas_window.window_, nullptr, 0, 0, 300, 200,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW)
        || !WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)
        || !host.GetSurfaceResourceUsage(first_canvas, first_surface_generation, initially_hidden)
        || initially_hidden.visible) {
        return 47;
    }
    // Showing only the ancestor need not send WM_SHOWWINDOW to its child.
    // Do not synthesize a resize to repair this production startup case.
    ShowWindow(first_parent.window_, SW_SHOWNOACTIVATE);
    RedrawWindow(first_canvas_window.window_, nullptr, nullptr,
        RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
    PumpWindowMessages();
    if (!WithWindowMessages([&] { return host.WaitQueueIdleForSmokeTest(); }, false)
        || !host.GetSurfaceResourceUsage(first_canvas, first_surface_generation, initially_hidden)
        || !initially_hidden.visible) {
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
    phase.Report("startup");

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
    phase.Report("initial_and_adjusted_readback");

    if (!VerifyPreviewPublicationAndFramePermit(host, first_sink->Route())) {
        PrintSurfaceState("preview publication/frame permit", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 41;
    }
    phase.Report("preview_and_frame_permit");
    if (!VerifySequenceSourceCache(host, first_canvas_window.window_, *first_sink)) {
        PrintSurfaceState("sequence source cache", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 39;
    }
    phase.Report("sequence_source_cache");
    if (!VerifyFirstSequenceEditTileReuse(host, *first_sink)) {
        PrintSurfaceState("first sequence edit tile reuse", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 42;
    }
    phase.Report("first_sequence_edit");
    if (!VerifyPresentationDuringContinuousSnapshots(host, *first_sink)) {
        PrintSurfaceState("continuous snapshot presentation", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 45;
    }
    phase.Report("continuous_snapshots");
    inkpod::renderer::SnapshotEnvelope restored_second{};
    if (!VerifySharedSequenceCacheLimit(host, *first_sink, *second_sink)
        || !second_core.Build(second_sink->Route(), restored_second)
        || !second_sink->Submit(restored_second)
        || !HasBounds(host, second_canvas, second_surface_generation, 48.0, 16.0)) {
        PrintSurfaceState("application sequence cache limit", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 40;
    }
    phase.Report("shared_sequence_cache_limit");

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
    phase.Report("snapshot_replacement");
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
    phase.Report("explicit_render_drain_248");
    if (!VerifySaturatedVisibility(host, first_sink->Route())) {
        PrintSurfaceState("saturated hide/show", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 43;
    }
    host.SetVisible(first_canvas, first_surface_generation, true);
    if (FAILED(host.RenderOnce(first_canvas, first_surface_generation))) {
        return 26;
    }
    phase.Report("saturated_visibility");

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
    phase.Report("device_loss");

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
    phase.Report("rebind");

    if (!VerifyHiddenCanvasResize(host, first_canvas_window.window_, *first_sink)) {
        PrintSurfaceState("hidden Canvas resize", host, first_canvas,
            first_surface_generation, first_canvas_window.window_);
        return 46;
    }
    phase.Report("hidden_resize");
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
    phase.Report("hidden_dpi_unbind");
    RendererPhaseDiagnostics shutdown_phase(
        host, second_canvas, second_surface_generation, second_canvas_window.window_);

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
    shutdown_phase.Report("first_canvas_close");
    inkpod::renderer::SnapshotEnvelope shutdown_pending{};
    if (!second_core.Build(second_sink->Route(), shutdown_pending)
        || !second_sink->Submit(shutdown_pending)) {
        return 28;
    }
    // Accepted presentation credits are still pending when shutdown starts.
    // Stop must wake an idle-barrier waiter as well as the renderer owner.
    for (std::size_t index = 0U; index < 64U; ++index) {
        host.RequestRender(second_canvas, second_surface_generation);
    }
    auto idle_at_shutdown = std::async(std::launch::async,
        [&] { return host.WaitQueueIdleForSmokeTest(); });
    const bool was_waiting = idle_at_shutdown.wait_for(std::chrono::milliseconds(10))
        == std::future_status::timeout;
    WithWindowMessages([&] { host.Stop(); return true; }, false);
    if (!was_waiting || idle_at_shutdown.wait_for(std::chrono::seconds(2)) != std::future_status::ready
        || idle_at_shutdown.get()) {
        return 44;
    }
    if (host.ThreadId() != 0U || host.DeviceGeneration() != 0U
        || host.SurfaceCount() != 0U
        || host.ResourceUsage().surface_count != 0U
        || host.GetSurfaceResourceUsage(
            second_canvas, second_surface_generation, first_surface_usage)) {
        return 19;
    }
    shutdown_phase.Report("shutdown");
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
