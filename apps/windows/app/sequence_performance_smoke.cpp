#include "app_smoke.h"

#include <windows.h>
#include <commctrl.h>
#include <dwmapi.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cstdio>
#include <new>
#include <string>
#include <vector>

#include "application.h"
#include "application_host.h"
#include "core_host.h"
#include "resource.h"
#include "renderer/canvas.h"
#include "ui/main_window_runtime.h"

namespace inkpod::windows::ui::runtime {
InkpodStatus ImportCommonRasterFromPath(
    app::ApplicationHost& state, const std::wstring& path) noexcept;
bool RefreshSequencePane(app::ApplicationHost& state) noexcept;
}

namespace inkpod::windows::ui {
namespace {

constexpr std::uint32_t kWidth = 1754U;
constexpr std::uint32_t kHeight = 1240U;
constexpr std::size_t kMeasuredSteps = 64U;
constexpr std::uint64_t kCacheBudget = 128U * 1024U * 1024U;

std::uint64_t Qpc() noexcept {
    LARGE_INTEGER value{};
    QueryPerformanceCounter(&value);
    return static_cast<std::uint64_t>(value.QuadPart);
}

void PumpMessages(app::ApplicationHost& state) noexcept {
    MSG message{};
    // A broken producer must not make the test's deadline unreachable.
    for (std::size_t count = 0U; count < 512U
         && PeekMessageW(&message, nullptr, 0U, 0U, PM_REMOVE) != FALSE; ++count) {
        if (message.message != WM_QUIT) {
            app::DispatchApplicationMessage(state, message);
        }
    }
}

struct Fixtures final {
    std::array<std::wstring, 3U> paths;
    std::array<bool, 3U> created{};
    ~Fixtures() {
        for (std::size_t index = 0U; index < paths.size(); ++index) {
            if (created[index]) {
                DeleteFileW(paths[index].c_str());
            }
        }
    }
    bool Write() {
        // Original synthetic test art, written as ordinary uncompressed TGA.
        // The normal Rust importer, not a benchmark-only decoder, opens it.
        std::vector<std::uint8_t> bytes(18U
            + static_cast<std::size_t>(kWidth) * kHeight * 3U);
        bytes[2] = 2U;
        bytes[12] = static_cast<std::uint8_t>(kWidth & 0xffU);
        bytes[13] = static_cast<std::uint8_t>(kWidth >> 8U);
        bytes[14] = static_cast<std::uint8_t>(kHeight & 0xffU);
        bytes[15] = static_cast<std::uint8_t>(kHeight >> 8U);
        bytes[16] = 24U;
        bytes[17] = 0x20U;
        for (std::size_t index = 0U; index < paths.size(); ++index) {
            paths[index] = L"inkpod-sequence-perf-"
                + std::to_wstring(GetCurrentProcessId()) + L"-"
                + std::to_wstring(index + 1U) + L".tga";
            for (std::uint32_t y = 0U; y < kHeight; ++y) {
                for (std::uint32_t x = 0U; x < kWidth; ++x) {
                    const bool ink = (x + static_cast<std::uint32_t>(index) * 11U)
                            % 97U < 2U || y % 131U < 2U;
                    const std::size_t offset = 18U
                        + (static_cast<std::size_t>(y) * kWidth + x) * 3U;
                    bytes[offset] = ink ? 83U : 255U;
                    bytes[offset + 1U] = ink ? 47U : 255U;
                    bytes[offset + 2U] = ink ? 24U : 255U;
                }
            }
            const HANDLE file = CreateFileW(paths[index].c_str(), GENERIC_WRITE,
                0U, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
            if (file == INVALID_HANDLE_VALUE) {
                return false;
            }
            created[index] = true;
            DWORD written{};
            const bool success = WriteFile(file, bytes.data(),
                static_cast<DWORD>(bytes.size()), &written, nullptr) != FALSE
                && written == bytes.size();
            const bool closed = CloseHandle(file) != FALSE;
            if (!success || !closed) {
                return false;
            }
        }
        return true;
    }
};

struct KeyboardState final {
    std::array<BYTE, 256U> original{};
    KeyboardState() noexcept {
        GetKeyboardState(original.data());
        auto clean = original;
        clean[VK_CONTROL] = 0U;
        clean[VK_SHIFT] = 0U;
        clean[VK_MENU] = 0U;
        SetKeyboardState(clean.data());
    }
    ~KeyboardState() { SetKeyboardState(original.data()); }
};

struct Sample final {
    std::uint64_t handler{};
    std::uint64_t submit{};
    std::uint64_t present{};
    std::uint64_t submitted_to_ready{};
    std::uint64_t draw{};
    std::uint64_t present_api{};
    bool foreground{};
};

struct ForegroundProbe final {
    HWND previous{GetForegroundWindow()};
    HWND window{};
    bool requested{};
    bool acquired{};
    explicit ForegroundProbe(HWND target) noexcept : window(target) {
        wchar_t setting[2]{};
        requested = GetEnvironmentVariableW(
            L"INKPOD_SEQUENCE_PERF_FOREGROUND", setting, 2U) == 1U && setting[0] == L'1';
        if (requested) {
            (void)SetForegroundWindow(window);
        }
        acquired = GetForegroundWindow() == window;
    }
    ~ForegroundProbe() {
        if (requested && acquired && previous != nullptr && previous != window
            && GetForegroundWindow() == window && IsWindow(previous)) {
            (void)SetForegroundWindow(previous);
        }
    }
};

bool QueryDocument(app::ApplicationHost& state, InkpodDocumentInfo& document) noexcept {
    return state.engine->GetDocumentInfo(
        state.Document().id, state.Document().generation, document);
}

bool WaitPresented(app::ApplicationHost& state, std::uint32_t index,
    std::uint64_t minimum_revision, renderer::RendererSurfaceResourceUsage& surface,
    bool require_pristine = true) noexcept {
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
    do {
        PumpMessages(state);
        const auto* group = state.Workspace().editors.Active();
        InkpodDocumentInfo document{};
        InkpodSequenceCatalogInfo catalog{};
        const auto& cells = state.Workspace().sequence_dialog.view.cells;
        if (group != nullptr && index < cells.size()
            && state.routing.sequence_switch_pending_token.load(std::memory_order_acquire) == 0U
            && state.routing.sequence_navigation_queue.empty()
            && QueryDocument(state, document)
            && state.engine->GetSequenceCatalog(
                state.Document().id, state.Document().generation, catalog)
            && catalog.active_index == index
            && document.document_revision >= minimum_revision
            && document.document_uuid_high == cells[index].document_uuid_high
            && document.document_uuid_low == cells[index].document_uuid_low
            && state.renderer->GetSurfaceResourceUsage(
                group->canvas_id, group->generation, surface)
            && surface.visible && !surface.occluded
            && surface.route.document_session == state.Document().id
            && surface.route.document_generation == state.Document().generation
            && surface.last_presented_presentation_epoch
                == state.Document().sequence_required_present_epoch
            && surface.last_presented_document_revision == document.document_revision
            && (!require_pristine
                || ((surface.last_presented_source.flags
                        & INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE) != 0U
                    && surface.last_presented_source.document_uuid_high
                        == document.document_uuid_high
                    && surface.last_presented_source.document_uuid_low
                        == document.document_uuid_low
                    && surface.last_presented_source.owner_generation
                        == catalog.owner_generation))) {
            return true;
        }
        MsgWaitForMultipleObjectsEx(0U, nullptr, 1U, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
    } while (std::chrono::steady_clock::now() < deadline);
    InkpodDocumentInfo document{};
    InkpodSequenceCatalogInfo catalog{};
    (void)QueryDocument(state, document);
    (void)state.engine->GetSequenceCatalog(
        state.Document().id, state.Document().generation, catalog);
    std::fprintf(stderr,
        "sequence wait failed index=%u/%u doc=%llu min=%llu present=%llu pending=%llu "
        "queue=%zu visible=%d occluded=%d pristine=%u\n",
        catalog.active_index, index, document.document_revision, minimum_revision,
        surface.last_presented_document_revision,
        state.routing.sequence_switch_pending_token.load(std::memory_order_acquire),
        state.routing.sequence_navigation_queue.size(), surface.visible, surface.occluded,
        surface.last_presented_source.flags);
    return false;
}

void Key(app::ApplicationHost& state, HWND list, bool next) noexcept {
    MSG message{};
    message.hwnd = list;
    message.message = WM_KEYDOWN;
    message.wParam = next ? VK_RIGHT : VK_LEFT;
    message.lParam = 1;
    // Exactly the production modeless-dialog, PreTranslate, native control order.
    app::DispatchApplicationMessage(state, message);
}

bool Step(app::ApplicationHost& state, HWND list, bool next,
    std::uint32_t expected_index, Sample& sample, bool warm) noexcept {
    sample.foreground = GetForegroundWindow() == state.Workspace().windows.window;
    InkpodDocumentInfo before{};
    if (!QueryDocument(state, before)) {
        return false;
    }
    const auto engine_before = state.engine->Metrics();
    const auto renderer_before = state.renderer->ResourceUsage();
    const auto* metadata = state.Workspace().sequence_dialog.view.cells.data();
    const auto* labels = state.Workspace().sequence_dialog.item_labels.data();
    const auto thumbnail_generation = state.Thumbnails().InvalidationGeneration(
        ThumbnailKind::Sequence);
    const std::uint64_t start = Qpc();
    Key(state, list, next);
    sample.handler = Qpc() - start;
    renderer::RendererSurfaceResourceUsage surface{};
    if (!WaitPresented(state, expected_index, before.document_revision + 1U, surface)) {
        return false;
    }
    if (surface.last_snapshot_submission_qpc < start
        || surface.first_frame_ready_qpc < surface.last_snapshot_submission_qpc
        || surface.first_present_begin_qpc < surface.first_frame_ready_qpc
        || surface.first_presented_revision_qpc < surface.first_present_begin_qpc) {
        std::fputs("sequence timing did not belong to the requested revision\n", stderr);
        return false;
    }
    sample.submit = surface.last_snapshot_submission_qpc - start;
    sample.present = surface.first_presented_revision_qpc - start;
    sample.submitted_to_ready = surface.first_frame_ready_qpc
        - surface.last_snapshot_submission_qpc;
    sample.draw = surface.first_present_begin_qpc - surface.first_frame_ready_qpc;
    sample.present_api = surface.first_presented_revision_qpc - surface.first_present_begin_qpc;
    InkpodDocumentInfo after{};
    const auto engine_after = state.engine->Metrics();
    const auto renderer_after = state.renderer->ResourceUsage();
    if (!QueryDocument(state, after) || (after.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || after.document_revision != before.document_revision + 1U
        || metadata != state.Workspace().sequence_dialog.view.cells.data()
        || labels != state.Workspace().sequence_dialog.item_labels.data()
        || thumbnail_generation != state.Thumbnails().InvalidationGeneration(ThumbnailKind::Sequence)
        || engine_after.submitted_snapshots - engine_before.submitted_snapshots != 1U
        || renderer_after.queue_rejection_count != renderer_before.queue_rejection_count
        || renderer_after.resource_limit_count != renderer_before.resource_limit_count
        || (warm && renderer_after.uploaded_tile_count != renderer_before.uploaded_tile_count)) {
        std::fprintf(stderr,
            "sequence semantic gate failed warm=%d snapshots=%llu uploads=%llu "
            "metadata=%d labels=%d thumbnail_generation=%llu/%llu\n", warm,
            engine_after.submitted_snapshots - engine_before.submitted_snapshots,
            renderer_after.uploaded_tile_count - renderer_before.uploaded_tile_count,
            metadata == state.Workspace().sequence_dialog.view.cells.data(),
            labels == state.Workspace().sequence_dialog.item_labels.data(),
            thumbnail_generation, state.Thumbnails().InvalidationGeneration(ThumbnailKind::Sequence));
        return false;
    }
    return true;
}

void Report(const char* scenario, const char* stage,
    std::array<std::uint64_t, kMeasuredSteps> values, double ticks_per_ms) noexcept {
    std::fprintf(stderr, "sequence_perf scenario=%s stage=%s samples_ms=[", scenario, stage);
    for (std::size_t index = 0U; index < values.size(); ++index) {
        std::fprintf(stderr, "%s%.6f", index == 0U ? "" : ",",
            static_cast<double>(values[index]) / ticks_per_ms);
    }
    std::sort(values.begin(), values.end());
    const auto quantile = [&](std::size_t percentile) noexcept {
        const auto index = (values.size() * percentile + 99U) / 100U - 1U;
        return static_cast<double>(values[index]) / ticks_per_ms;
    };
    std::fprintf(stderr, "] p50_ms=%.6f p95_ms=%.6f p99_ms=%.6f max_ms=%.6f\n",
        quantile(50U), quantile(95U), quantile(99U),
        static_cast<double>(values.back()) / ticks_per_ms);
}

int Run(app::ApplicationHost& state) {
    if (state.engine == nullptr || state.renderer == nullptr) {
        return 18001;
    }
    Fixtures fixtures;
    if (!fixtures.Write()) {
        return 18002;
    }
    const HWND window = state.Workspace().windows.window;
    // Measure an exposed Canvas without taking keyboard focus from the user.
    // Visibility alone does not prevent another application's window from
    // covering the swap chain and changing compositor scheduling.
    if (!SetWindowPos(window, HWND_TOPMOST, 0, 0, 1280, 960,
            SWP_NOMOVE | SWP_NOACTIVATE)) {
        return 18016;
    }
    ShowWindow(window, SW_SHOWNOACTIVATE);
    ForegroundProbe foreground(window);
    runtime::ShowInitialPalettes(state);
    UpdateWindow(window);
    PumpMessages(state);
    if (runtime::ImportCommonRasterFromPath(state, fixtures.paths[0]) != INKPOD_STATUS_OK) {
        return 18003;
    }
    state.lifetime.sequence_switch_policy = app::SequenceCellSwitchPolicy::Prompt;
    state.lifetime.sequence_endpoint_policy = app::SequenceEndpointPolicy::Stop;
    state.lifetime.smoke_sequence_paths.assign(fixtures.paths.begin(), fixtures.paths.end());
    if (SendMessageW(window, WM_COMMAND, IDM_SEQ_IMPORT, 0) != 1
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || !runtime::RefreshSequencePane(state)) {
        return 18004;
    }
    PumpMessages(state);
    const HWND list = GetDlgItem(state.Workspace().sequence_palette, IDC_SEQUENCE_CELLS);
    if (list == nullptr || state.Workspace().sequence_dialog.view.cells.size() != 3U) {
        return 18005;
    }
    KeyboardState keyboard;
    SetFocus(list);
    SendMessageW(list, LB_SETCURSEL, 0U, 0);
    SendMessageW(state.Workspace().sequence_palette, WM_COMMAND,
        MAKEWPARAM(IDC_SEQUENCE_CELLS, LBN_SELCHANGE), reinterpret_cast<LPARAM>(list));
    renderer::RendererSurfaceResourceUsage surface{};
    if (!WaitPresented(state, 0U, 0U, surface, false)) {
        return 18006;
    }
    LARGE_INTEGER frequency{};
    QueryPerformanceFrequency(&frequency);
    const double ticks_per_ms = static_cast<double>(frequency.QuadPart) / 1000.0;
    MONITORINFOEXW monitor{};
    monitor.cbSize = sizeof(monitor);
    DEVMODEW display{};
    display.dmSize = sizeof(display);
    const bool has_display = GetMonitorInfoW(
            MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
            static_cast<MONITORINFO*>(&monitor)) != FALSE
        && EnumDisplaySettingsW(monitor.szDevice, ENUM_CURRENT_SETTINGS, &display) != FALSE;
    std::fprintf(stderr,
        "sequence_environment window_width=1280 window_height=960 exposed=topmost "
        "dpi=%u refresh_hz=%lu qpc_frequency=%lld foreground_requested=%d foreground=%d\n",
        GetDpiForWindow(window), has_display ? display.dmDisplayFrequency : 0UL,
        frequency.QuadPart, foreground.requested, foreground.acquired);
    DWM_TIMING_INFO composition{};
    composition.cbSize = sizeof(composition);
    const HRESULT composition_status = DwmGetCompositionTimingInfo(nullptr, &composition);
    std::fprintf(stderr,
        "sequence_compositor result=0x%08lx refresh=%u/%u compose=%u/%u refresh_period_ms=%.6f\n",
        static_cast<unsigned long>(composition_status),
        composition.rateRefresh.uiNumerator, composition.rateRefresh.uiDenominator,
        composition.rateCompose.uiNumerator, composition.rateCompose.uiDenominator,
        static_cast<double>(composition.qpcRefreshPeriod) / ticks_per_ms);
    for (std::size_t index = 0U; index < 16U; ++index) {
        const std::array<std::uint32_t, 4U> targets{1U, 2U, 1U, 0U};
        Sample sample{};
        if (!Step(state, list, index % 4U < 2U, targets[index % 4U], sample, false)) {
            return 18007;
        }
        if (index < 4U) {
            std::fprintf(stderr,
                "sequence_initial step=%zu handler_ms=%.6f submit_ms=%.6f present_ms=%.6f\n",
                index, static_cast<double>(sample.handler) / ticks_per_ms,
                static_cast<double>(sample.submit) / ticks_per_ms,
                static_cast<double>(sample.present) / ticks_per_ms);
        }
    }
    InkpodIoCacheInfo io_before{sizeof(InkpodIoCacheInfo)};
    if (inkpod_io_manager_get_cache_info(state.file_io.Manager(), &io_before) != INKPOD_STATUS_OK) {
        return 18008;
    }
    const auto prompt_count = state.lifetime.smoke_dirty_prompt_count;
    const auto frame_count = state.renderer->PresentedFrameCount(
        surface.route.canvas, surface.route.surface_generation);
    for (const bool three_cells : {false, true}) {
        const auto scenario_frames = state.renderer->PresentedFrameCount(
            surface.route.canvas, surface.route.surface_generation);
        if (!state.renderer->GetSurfaceResourceUsage(
                surface.route.canvas, surface.route.surface_generation, surface)) {
            return 18014;
        }
        const auto scenario_timeouts = surface.frame_latency_timeout_count;
        std::array<std::uint64_t, kMeasuredSteps> handler{};
        std::array<std::uint64_t, kMeasuredSteps> submit{};
        std::array<std::uint64_t, kMeasuredSteps> present{};
        std::array<std::uint64_t, kMeasuredSteps> submitted_to_ready{};
        std::array<std::uint64_t, kMeasuredSteps> draw{};
        std::array<std::uint64_t, kMeasuredSteps> present_api{};
        std::size_t foreground_samples{};
        for (std::size_t index = 0U; index < kMeasuredSteps; ++index) {
            const std::array<std::uint32_t, 4U> targets{1U, 2U, 1U, 0U};
            const bool next = three_cells ? index % 4U < 2U : index % 2U == 0U;
            const auto target = three_cells ? targets[index % 4U]
                : static_cast<std::uint32_t>((index + 1U) % 2U);
            Sample sample{};
            if (!Step(state, list, next, target, sample, true)) {
                return 18009;
            }
            handler[index] = sample.handler;
            submit[index] = sample.submit;
            present[index] = sample.present;
            submitted_to_ready[index] = sample.submitted_to_ready;
            draw[index] = sample.draw;
            present_api[index] = sample.present_api;
            foreground_samples += sample.foreground ? 1U : 0U;
        }
        const char* scenario = three_cells ? "A_B_C_B_A" : "A_B_A";
        Report(scenario, "handler", handler, ticks_per_ms);
        Report(scenario, "snapshot_submission", submit, ticks_per_ms);
        Report(scenario, "first_successful_present", present, ticks_per_ms);
        Report(scenario, "submitted_to_frame_ready", submitted_to_ready, ticks_per_ms);
        Report(scenario, "draw", draw, ticks_per_ms);
        Report(scenario, "present_api", present_api, ticks_per_ms);
        if (!state.renderer->GetSurfaceResourceUsage(
                surface.route.canvas, surface.route.surface_generation, surface)) {
            return 18015;
        }
        std::fprintf(stderr,
            "sequence_presentation scenario=%s measured=%zu presented_frames=%llu "
            "frame_latency_timeouts=%llu last_render_result=0x%08lx foreground_samples=%zu\n",
            scenario, kMeasuredSteps,
            state.renderer->PresentedFrameCount(surface.route.canvas, surface.route.surface_generation)
                - scenario_frames,
            surface.frame_latency_timeout_count - scenario_timeouts,
            static_cast<unsigned long>(surface.last_render_result), foreground_samples);
    }
    const auto warm_presented_frames = state.renderer->PresentedFrameCount(
        surface.route.canvas, surface.route.surface_generation) - frame_count;
    InkpodIoCacheInfo io_after{sizeof(InkpodIoCacheInfo)};
    const auto usage = state.renderer->ResourceUsage();
    if (inkpod_io_manager_get_cache_info(state.file_io.Manager(), &io_after) != INKPOD_STATUS_OK
        || io_after.physical_reads != io_before.physical_reads
        || io_after.decodes != io_before.decodes
        || io_after.sequence_render_allocations > 8U
        || io_after.sequence_render_bytes > kCacheBudget
        || usage.sequence_cache_source_count > 8U || usage.sequence_cache_bytes > kCacheBudget
        || prompt_count != state.lifetime.smoke_dirty_prompt_count) {
        return 18010;
    }
    // Queue consecutive repeats before the first completion, then reverse.
    // Every accepted directional intent must contribute to the final position.
    state.lifetime.sequence_endpoint_policy = app::SequenceEndpointPolicy::Wrap;
    runtime::UpdateMenuState(state);
    InkpodDocumentInfo burst_before{};
    if (!QueryDocument(state, burst_before)) {
        return 18011;
    }
    for (std::size_t index = 0U; index < 50U; ++index) {
        Key(state, list, index < 24U || index >= 41U);
    }
    if (!WaitPresented(state, 1U, burst_before.document_revision + 50U, surface)) {
        return 18012;
    }
    InkpodDocumentInfo burst_after{};
    if (!QueryDocument(state, burst_after)
        || burst_after.document_revision != burst_before.document_revision + 50U) {
        return 18013;
    }
    std::fprintf(stderr,
        "sequence_semantics width=%u height=%u format=TGA sources=3 measured=%zu "
        "warm_reads=0 warm_decodes=0 warm_uploads=0 snapshots_per_step=1 "
        "cpu_sources=%llu cpu_bytes=%llu gpu_sources=%llu gpu_bytes=%llu "
        "warm_presented_frames=%llu presented_frames=%llu "
        "burst_intents=50 burst_commits=50 lost_intents=0\n",
        kWidth, kHeight, kMeasuredSteps * 2U,
        io_after.sequence_render_allocations, io_after.sequence_render_bytes,
        usage.sequence_cache_source_count, usage.sequence_cache_bytes,
        warm_presented_frames,
        state.renderer->PresentedFrameCount(surface.route.canvas, surface.route.surface_generation)
            - frame_count);
    state.lifetime.smoke_sequence_paths.clear();
    return 0;
}

}  // namespace

int RunSequencePerformanceSmoke(app::ApplicationHost& state) noexcept {
    int result{};
    try {
        result = Run(state);
    } catch (const std::bad_alloc&) {
        result = 18099;
    }
    if (result != 0) {
        std::fprintf(stderr, "inkpod sequence performance smoke failed: %d\n", result);
    }
    return result;
}

}  // namespace inkpod::windows::ui
