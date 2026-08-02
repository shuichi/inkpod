#include "application.h"

#include <commctrl.h>

#include <array>
#include <memory>
#include <new>
#include <string>
#include <string_view>
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
#include "ui/workspace_layout.h"

namespace inkpod::app {
namespace {

std::array<wchar_t, 96U> WorkspaceRegistryValueName(
    std::wstring_view base, std::uint32_t slot) noexcept {
    std::array<wchar_t, 96U> result{};
    _snwprintf_s(
        result.data(), result.size(), _TRUNCATE, L"%.*ls.%u",
        static_cast<int>(base.size()), base.data(), slot);
    return result;
}

bool InitializeFrontendRouting(ApplicationHost& state) noexcept {
    return state.InitializeOwners();
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
    if (!state.lifetime.smoke_test) {
        (void)windows::ui::SaveWorkspaceWindowCount(
            static_cast<std::uint32_t>(state.Workspaces().Count()));
    }
    for (std::size_t index = 0U; index < state.Workspaces().Count(); ++index) {
        WorkspaceWindow* workspace = state.Workspaces().At(index);
        if (workspace == nullptr) {
            continue;
        }
        (void)state.ActivateWorkspaceWindow(workspace->id, false);
        for (std::size_t group_index = 0U;
             group_index < workspace->editors.GroupCount(); ++group_index) {
            const EditorGroup* group = workspace->editors.GroupAt(group_index);
            if (group != nullptr && group->canvas != nullptr) {
                (void)renderer::UnbindCanvasSnapshotSink(group->canvas);
            }
        }
        if (workspace->subpalette_dialog.canvas != nullptr) {
            (void)renderer::UnbindCanvasSnapshotSink(
                workspace->subpalette_dialog.canvas);
        }
        if (state.engine != nullptr
            && workspace->subpalette_core_view_id != 0U
            && workspace->subpalette_session
            && workspace->subpalette_document_generation) {
            const std::uint64_t view_id = workspace->subpalette_core_view_id;
            (void)state.engine->Invoke(
                workspace->subpalette_session,
                workspace->subpalette_document_generation,
                [view_id](InkpodCore* core) {
                    return inkpod_core_view_close(core, view_id);
                },
                false,
                false);
            workspace->subpalette_core_view_id = 0U;
        }
        if (workspace->subpalette_canvas_id) {
            (void)state.routing.targets.UnregisterAuxiliaryCanvas(
                workspace->subpalette_canvas_id);
            workspace->subpalette_canvas_id = {};
        }
        if (workspace->subpalette_palette != nullptr) {
            DestroyWindow(workspace->subpalette_palette);
            workspace->subpalette_palette = nullptr;
        }
    }

    if (state.engine != nullptr) {
        state.DetachCoreSessions();
        state.engine->Stop();
        state.engine.reset();
    }
    if (state.renderer != nullptr) {
        state.renderer->Stop();
    }
    for (std::size_t index = 0U; index < state.Workspaces().Count(); ++index) {
        WorkspaceWindow* workspace = state.Workspaces().At(index);
        if (workspace == nullptr) {
            continue;
        }
        (void)state.ActivateWorkspaceWindow(workspace->id, false);
        if (!state.lifetime.smoke_test) {
            windows::ui::runtime::CaptureWorkspacePresentation(state);
            const auto session_name =
                WorkspaceRegistryValueName(
                    L"WorkspaceSessionV4", workspace->persistence_slot);
            windows::ui::SaveWorkspaceLayout(
                workspace->windows.workspace, session_name.data());
            const auto locator_name =
                WorkspaceRegistryValueName(
                    L"LocatorPaletteV1", workspace->persistence_slot);
            const auto sequence_name =
                WorkspaceRegistryValueName(
                    L"SequencePaletteV1", workspace->persistence_slot);
            const auto light_table_name =
                WorkspaceRegistryValueName(
                    L"LightTablePaletteV1", workspace->persistence_slot);
            const auto subpalette_name =
                WorkspaceRegistryValueName(
                    L"SubpalettePaletteV1", workspace->persistence_slot);
            (void)windows::ui::SavePaletteWindowPlacement(
                workspace->locator_palette, locator_name.data());
            (void)windows::ui::SavePaletteWindowPlacement(
                workspace->sequence_palette, sequence_name.data());
            (void)windows::ui::SavePaletteWindowPlacement(
                workspace->light_table_palette, light_table_name.data());
            (void)windows::ui::SavePaletteWindowPlacement(
                workspace->subpalette_palette, subpalette_name.data());
        }
        const std::array<HWND*, 9U> owned{
            &workspace->effects_progress,
            &workspace->batch_progress,
            &workspace->tools.palette,
            &workspace->windows.tool_options,
            &workspace->windows.color_pane,
            &workspace->panes.layer_palette,
            &workspace->batch_palette,
            &workspace->locator_palette,
            &workspace->sequence_palette};
        for (HWND* handle : owned) {
            if (*handle != nullptr) {
                DestroyWindow(*handle);
                *handle = nullptr;
            }
        }
        if (workspace->light_table_palette != nullptr) {
            DestroyWindow(workspace->light_table_palette);
            workspace->light_table_palette = nullptr;
        }
        workspace->windows.tool_palette = nullptr;
        workspace->windows.layer_palette = nullptr;
    }

    const InkpodStatus task_status = inkpod_task_release(&state.effects.task);
    const InkpodStatus batch_task_status =
        inkpod_batch_task_release(&state.batch.task);
    const InkpodStatus preview_status =
        inkpod_batch_preview_release(&state.batch.preview);
    const InkpodStatus report_status =
        inkpod_batch_report_release(&state.batch.report);
    const InkpodStatus graph_status =
        inkpod_batch_graph_release(&state.batch.graph);
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
        for (std::size_t workspace_index = 0U;
             workspace_index < state.Workspaces().Count() && !dialog_message;
             ++workspace_index) {
            const WorkspaceWindow* workspace =
                state.Workspaces().At(workspace_index);
            if (workspace == nullptr) {
                continue;
            }
            const std::array<HWND, 7U> palettes{
                workspace->tools.palette,
                workspace->panes.layer_palette,
                workspace->batch_palette,
                workspace->locator_palette,
                workspace->sequence_palette,
                workspace->light_table_palette,
                workspace->subpalette_palette};
            for (const HWND palette : palettes) {
                if (palette != nullptr && IsWindowVisible(palette) != FALSE
                    && IsDialogMessageW(palette, &message) != FALSE) {
                    dialog_message = true;
                    break;
                }
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
    state.lifetime.window_class_name = class_name.data();
    state.lifetime.window_title = title.data();
    state.lifetime.show_command = launch_.show_command;
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
    const WorkspaceWindowId initial_workspace = state.Workspace().id;
    if (!launch_.smoke_test) {
        std::uint32_t window_count{1U};
        (void)windows::ui::LoadWorkspaceWindowCount(window_count);
        for (std::uint32_t index = 1U; index < window_count; ++index) {
            if (windows::ui::runtime::CreateWorkspaceWindow(state, false)
                == nullptr) {
                break;
            }
        }
        (void)state.ActivateWorkspaceWindow(initial_workspace, false);
    }
    windows::ui::runtime::UpdateMenuState(state);

    int exit_code{};
    if (launch_.smoke_test) {
        exit_code = windows::ui::RunApplicationSmoke(state);
    } else {
        for (std::size_t index = 0U; index < state.Workspaces().Count(); ++index) {
            WorkspaceWindow* workspace = state.Workspaces().At(index);
            if (workspace == nullptr || workspace->windows.window == nullptr) {
                continue;
            }
            (void)state.ActivateWorkspaceWindow(workspace->id, false);
            ShowWindow(workspace->windows.window, launch_.show_command);
            windows::ui::runtime::ShowInitialPalettes(state);
            UpdateWindow(workspace->windows.window);
        }
        (void)state.ActivateWorkspaceWindow(initial_workspace, true);
        exit_code = RunMessageLoop(state);
    }

    core_status = StopCore(state);
    for (std::size_t index = state.Workspaces().Count(); index > 0U; --index) {
        WorkspaceWindow* workspace = state.Workspaces().At(index - 1U);
        if (workspace != nullptr && workspace->windows.window != nullptr) {
            DestroyWindow(workspace->windows.window);
            workspace->windows.window = nullptr;
        }
    }
    state.ClearOwners();
    host_.reset();
    if (core_status != INKPOD_STATUS_OK && exit_code == 0) {
        return 18;
    }
    return exit_code;
}

}  // namespace inkpod::app
