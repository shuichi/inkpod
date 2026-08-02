#include "application.h"

#include <commctrl.h>

#include <array>
#include <memory>
#include <new>
#include <string>
#include <utility>

#include "application_host.h"
#include "app_smoke.h"
#include "com_runtime.h"
#include "core_host.h"
#include "document_shell.h"
#include "inkpod/core_ffi.h"
#include "renderer/canvas.h"
#include "resource.h"
#include "ui/main_window.h"
#include "ui/main_window_runtime.h"
#include "ui/palette_window.h"
#include "ui/shortcut_controller.h"

namespace inkpod::app {
namespace {

bool InitializeFrontendRouting(ApplicationHost& state) noexcept {
    if (!state.InitializeOwners()) {
        return false;
    }
    const auto tool = state.routing.targets.RegisterPane();
    const auto tool_options = state.routing.targets.RegisterPane();
    const auto color = state.routing.targets.RegisterPane();
    const auto layer = state.routing.targets.RegisterPane();
    const auto batch = state.routing.targets.RegisterPane();
    const auto locator = state.routing.targets.RegisterPane();
    const auto sequence = state.routing.targets.RegisterPane();
    const auto light_table = state.routing.targets.RegisterPane();
    const auto reference = state.routing.targets.RegisterPane();
    const auto subpalette = state.routing.targets.RegisterPane();
    if (!tool.has_value() || !tool_options.has_value() || !color.has_value()
        || !layer.has_value() || !batch.has_value() || !locator.has_value()
        || !sequence.has_value() || !light_table.has_value()
        || !reference.has_value() || !subpalette.has_value()) {
        state.ClearOwners();
        return false;
    }
    state.routing.tool_pane = tool.value();
    state.routing.tool_options_pane = tool_options.value();
    state.routing.color_pane = color.value();
    state.routing.layer_pane = layer.value();
    state.routing.batch_pane = batch.value();
    state.routing.locator_pane = locator.value();
    state.routing.sequence_pane = sequence.value();
    state.routing.light_table_pane = light_table.value();
    state.routing.reference_pane = reference.value();
    state.routing.subpalette_pane = subpalette.value();

    using PanePolicy = PaneTargetPolicy;
    const std::array registrations{
        std::pair{tool.value(), PanePolicy::Application},
        std::pair{tool_options.value(), PanePolicy::FollowActiveView},
        std::pair{color.value(), PanePolicy::FollowActiveView},
        std::pair{layer.value(), PanePolicy::FollowActiveView},
        std::pair{batch.value(), PanePolicy::FollowActiveView},
        std::pair{locator.value(), PanePolicy::FollowActiveView},
        std::pair{sequence.value(), PanePolicy::FollowActiveView},
        std::pair{light_table.value(), PanePolicy::FollowActiveView},
        std::pair{reference.value(), PanePolicy::FollowActiveView},
        std::pair{subpalette.value(), PanePolicy::FollowActiveView}};
    for (const auto& [pane, policy] : registrations) {
        if (state.routing.pane_targets.Register(pane, policy)
            != PaneTargetStatus::Ok) {
            state.ClearOwners();
            return false;
        }
    }
    return true;
}

HRESULT StartRenderer(ApplicationHost& state) noexcept {
    try {
        state.renderer = std::make_unique<renderer::RendererHost>();
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
    const HRESULT result = state.renderer->Start();
    if (FAILED(result)) {
        state.renderer.reset();
    }
    return result;
}

InkpodStatus StartCore(ApplicationHost& state) noexcept {
    try {
        state.engine = std::make_unique<CoreHost>();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Start(
        renderer::GetCanvasSnapshotSink(state.Workspace().windows.canvas),
        state.Workspace().windows.window);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    const DocumentSessionId session = state.routing.targets.ReplaceDocument();
    if (!state.ReplaceDocumentSession(
            session,
            state.routing.targets.CurrentGeneration(),
            state.routing.targets.ActiveDocumentView())) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return windows::ui::InitializeShortcuts(
        *state.engine, state.shortcuts, !state.lifetime.smoke_test);
}

InkpodStatus StopCore(ApplicationHost& state) noexcept {
    const InkpodStatus clipboard_status = inkpod_clipboard_release(&state.clipboard);
    if (state.effects.task != nullptr) {
        inkpod_task_cancel(state.effects.task);
    }
    if (state.batch.task != nullptr) {
        inkpod_batch_task_cancel(state.batch.task);
    }
    if (state.Workspace().subpalette_dialog.canvas != nullptr) {
        (void)renderer::UnbindCanvasSnapshotSink(
            state.Workspace().subpalette_dialog.canvas);
    }
    if (state.engine != nullptr
        && state.Workspace().subpalette_core_view_id != 0U
        && state.Workspace().subpalette_session
        && state.Workspace().subpalette_document_generation) {
        const std::uint64_t view_id =
            state.Workspace().subpalette_core_view_id;
        (void)state.engine->Invoke(
            state.Workspace().subpalette_session,
            state.Workspace().subpalette_document_generation,
            [view_id](InkpodCore* core) {
                return inkpod_core_view_close(core, view_id);
            },
            false,
            false);
        state.Workspace().subpalette_core_view_id = 0U;
    }
    if (state.Workspace().subpalette_palette != nullptr) {
        if (!state.lifetime.smoke_test) {
            (void)windows::ui::SavePaletteWindowPlacement(
                state.Workspace().subpalette_palette,
                L"SubpalettePaletteV1");
        }
        DestroyWindow(state.Workspace().subpalette_palette);
        state.Workspace().subpalette_palette = nullptr;
    }
    if (state.Workspace().subpalette_canvas_id) {
        (void)state.routing.targets.UnregisterAuxiliaryCanvas(
            state.Workspace().subpalette_canvas_id);
        state.Workspace().subpalette_canvas_id = {};
    }
    if (state.engine != nullptr) {
        state.DetachCoreSessions();
        state.engine->Stop();
        state.engine.reset();
    }
    if (state.renderer != nullptr) {
        state.renderer->Stop();
    }
    if (state.Workspace().effects_progress != nullptr) {
        DestroyWindow(state.Workspace().effects_progress);
        state.Workspace().effects_progress = nullptr;
    }
    if (state.Workspace().batch_progress != nullptr) {
        DestroyWindow(state.Workspace().batch_progress);
        state.Workspace().batch_progress = nullptr;
    }
    if (!state.lifetime.smoke_test) {
        windows::ui::runtime::CaptureWorkspacePresentation(state);
        windows::ui::SaveWorkspaceLayout(
            state.Workspace().windows.workspace, L"WorkspaceSessionV4");
        (void)windows::ui::SavePaletteWindowPlacement(
            state.Workspace().locator_palette, L"LocatorPaletteV1");
        (void)windows::ui::SavePaletteWindowPlacement(
            state.Workspace().sequence_palette, L"SequencePaletteV1");
        (void)windows::ui::SavePaletteWindowPlacement(
            state.Workspace().light_table_palette, L"LightTablePaletteV1");
    }
    if (state.Workspace().tools.palette != nullptr) {
        DestroyWindow(state.Workspace().tools.palette);
        state.Workspace().tools.palette = nullptr;
        state.Workspace().windows.tool_palette = nullptr;
    }
    if (state.Workspace().windows.tool_options != nullptr) {
        DestroyWindow(state.Workspace().windows.tool_options);
        state.Workspace().windows.tool_options = nullptr;
    }
    if (state.Workspace().windows.color_pane != nullptr) {
        DestroyWindow(state.Workspace().windows.color_pane);
        state.Workspace().windows.color_pane = nullptr;
    }
    if (state.Workspace().panes.layer_palette != nullptr) {
        DestroyWindow(state.Workspace().panes.layer_palette);
        state.Workspace().panes.layer_palette = nullptr;
        state.Workspace().windows.layer_palette = nullptr;
    }
    if (state.Workspace().batch_palette != nullptr) {
        DestroyWindow(state.Workspace().batch_palette);
        state.Workspace().batch_palette = nullptr;
    }
    if (state.Workspace().locator_palette != nullptr) {
        DestroyWindow(state.Workspace().locator_palette);
        state.Workspace().locator_palette = nullptr;
    }
    if (state.Workspace().sequence_palette != nullptr) {
        DestroyWindow(state.Workspace().sequence_palette);
        state.Workspace().sequence_palette = nullptr;
    }
    if (state.Workspace().light_table_palette != nullptr) {
        DestroyWindow(state.Workspace().light_table_palette);
        state.Workspace().light_table_palette = nullptr;
    }
    const InkpodStatus task_status = inkpod_task_release(&state.effects.task);
    const InkpodStatus batch_task_status = inkpod_batch_task_release(&state.batch.task);
    const InkpodStatus preview_status = inkpod_batch_preview_release(&state.batch.preview);
    const InkpodStatus report_status = inkpod_batch_report_release(&state.batch.report);
    const InkpodStatus graph_status = inkpod_batch_graph_release(&state.batch.graph);
    for (const InkpodStatus status : {
             clipboard_status,
             task_status,
             batch_task_status,
             preview_status,
             report_status,
             graph_status}) {
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
    }
    state.routing.timers.Clear();
    state.routing.targets.InvalidateAll();
    return INKPOD_STATUS_OK;
}

int RunMessageLoop(ApplicationHost& state) noexcept {
    MSG message{};
    BOOL result{};
    while ((result = GetMessageW(&message, nullptr, 0, 0)) > 0) {
        bool dialog_message{};
        const std::array<HWND, 7U> palettes{
            state.Workspace().tools.palette,
            state.Workspace().panes.layer_palette,
            state.Workspace().batch_palette,
            state.Workspace().locator_palette,
            state.Workspace().sequence_palette,
            state.Workspace().light_table_palette,
            state.Workspace().subpalette_palette};
        for (const HWND palette : palettes) {
            if (palette != nullptr && IsWindowVisible(palette) != FALSE
                && IsDialogMessageW(palette, &message) != FALSE) {
                dialog_message = true;
                break;
            }
        }
        if (dialog_message) {
            continue;
        }
        if (windows::ui::runtime::PreTranslateKeyboardMessage(state, message)) {
            continue;
        }
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    return result == -1 ? 17 : static_cast<int>(message.wParam);
}

}  // namespace

Application::Application(ApplicationLaunch launch) noexcept
    : launch_(std::move(launch)) {}

Application::~Application() = default;

int Application::Run() {
    INITCOMMONCONTROLSEX controls{};
    controls.dwSize = sizeof(controls);
    controls.dwICC = ICC_STANDARD_CLASSES | ICC_BAR_CLASSES | ICC_TAB_CLASSES;
    if (!InitCommonControlsEx(&controls)) {
        MessageBoxW(
            nullptr,
            L"Common Controls の初期化に失敗しました。",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 10;
    }

    ComApartment com;
    if (FAILED(com.Initialize())) {
        MessageBoxW(
            nullptr,
            L"COM の初期化に失敗しました。",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 11;
    }

    std::array<wchar_t, 128> title{};
    std::array<wchar_t, 128> class_name{};
    if (LoadStringW(
            launch_.instance,
            IDS_APP_TITLE,
            title.data(),
            static_cast<int>(title.size())) == 0
        || LoadStringW(
               launch_.instance,
               IDS_MAIN_WINDOW_CLASS,
               class_name.data(),
               static_cast<int>(class_name.size())) == 0) {
        return 12;
    }
    if (!renderer::RegisterCanvasClass(launch_.instance)
        || !windows::ui::RegisterMainWindowClass(
            launch_.instance,
            class_name.data(),
            windows::ui::runtime::MainWindowProcedure)) {
        return 13;
    }

    try {
        host_ = std::make_unique<ApplicationHost>();
    } catch (const std::bad_alloc&) {
        return 14;
    }
    ApplicationHost& state = *host_;
    state.lifetime.instance = launch_.instance;
    state.lifetime.smoke_test = launch_.smoke_test;
    if (!InitializeFrontendRouting(state)) {
        host_.reset();
        return 14;
    }
    if (FAILED(StartRenderer(state))) {
        state.ClearOwners();
        host_.reset();
        return 15;
    }
    HWND window = CreateWindowExW(
        0,
        class_name.data(),
        title.data(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1024,
        720,
        nullptr,
        nullptr,
        launch_.instance,
        &state.Workspace());
    if (window == nullptr) {
        state.renderer->Stop();
        state.ClearOwners();
        host_.reset();
        return 14;
    }

    InkpodStatus core_status = StartCore(state);
    if (core_status != INKPOD_STATUS_OK) {
        if (!launch_.smoke_test) {
            windows::ui::runtime::ShowCoreError(
                state, window, L"Rust Core の初期化");
        }
        StopCore(state);
        DestroyWindow(window);
        state.ClearOwners();
        host_.reset();
        return 15;
    }

    bool document_initialized{};
    if (!launch_.document_path.empty()) {
        core_status = windows::ui::runtime::OpenDocumentFromPath(
            state, launch_.document_path);
        document_initialized = core_status == INKPOD_STATUS_OK;
    }
    if (core_status == INKPOD_STATUS_OK && !document_initialized
        && !launch_.smoke_test) {
        std::wstring recovery;
        if (NewestPrivateRecovery(recovery)) {
            const int choice = MessageBoxW(
                window,
                L"未処理のRecoveryがあります。\n\n"
                L"はい: Recoveryを開く\nいいえ: Recoveryを破棄\n"
                L"キャンセル: 後で判断して新規セルを開く",
                L"inkpod Recovery",
                MB_YESNOCANCEL | MB_ICONQUESTION);
            if (choice == IDYES) {
                core_status = windows::ui::runtime::OpenRecoveryFromPath(
                    state, recovery);
                document_initialized = core_status == INKPOD_STATUS_OK;
                if (!document_initialized) {
                    windows::ui::runtime::ShowCoreError(
                        state, window, L"起動時Recoveryを開く");
                    core_status = INKPOD_STATUS_OK;
                }
            } else if (choice == IDNO
                && DeleteFileW(recovery.c_str()) == FALSE
                && GetLastError() != ERROR_FILE_NOT_FOUND) {
                MessageBoxW(
                    window,
                    L"Recoveryを削除できませんでした。ファイルを残して新規セルを開きます。",
                    L"inkpod Recovery",
                    MB_OK | MB_ICONWARNING);
            }
        }
    }
    if (core_status == INKPOD_STATUS_OK && !document_initialized) {
        core_status = windows::ui::runtime::CreateDefaultCell(state);
        document_initialized = core_status == INKPOD_STATUS_OK;
    }
    if (core_status != INKPOD_STATUS_OK || !document_initialized) {
        if (!launch_.smoke_test) {
            windows::ui::runtime::ShowCoreError(
                state,
                window,
                launch_.document_path.empty()
                    ? L"セルまたはRecoveryの初期化"
                    : L"起動ファイルを開く");
        }
        StopCore(state);
        DestroyWindow(window);
        state.ClearOwners();
        host_.reset();
        return 16;
    }
    windows::ui::runtime::UpdateMenuState(state);

    int exit_code{};
    if (launch_.smoke_test) {
        exit_code = windows::ui::RunApplicationSmoke(state);
    } else {
        ShowWindow(window, launch_.show_command);
        windows::ui::runtime::ShowInitialPalettes(state);
        UpdateWindow(window);
        exit_code = RunMessageLoop(state);
    }

    core_status = StopCore(state);
    DestroyWindow(window);
    state.ClearOwners();
    host_.reset();
    if (core_status != INKPOD_STATUS_OK && exit_code == 0) {
        return 18;
    }
    return exit_code;
}

}  // namespace inkpod::app
