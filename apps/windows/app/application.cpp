#include "ui/ui_resources.h"

#include "ui/localization.h"

#include "application.h"

#include <commctrl.h>

#include <array>
#include <algorithm>
#include <memory>
#include <new>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "activation.h"
#include "application_host.h"
#include "app_smoke.h"
#include "com_runtime.h"
#include "core_host.h"
#include "document_shell.h"
#include "inkpod/core_ffi.h"
#include "renderer/canvas.h"
#include "resource.h"
#include "session_recovery.h"
#include "ui/main_window.h"
#include "ui/main_window_runtime.h"
#include "ui/dialogs/history_visualization_dialog.h"
#include "ui/shortcut_controller.h"
#include "ui/workspace_layout.h"

namespace inkpod::app {
using windows::ui::LoadLocalizedStringW;
using windows::ui::UiStringId;
using windows::ui::UiText;

namespace {

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
    const InkpodStatus io_status = state.file_io.Initialize(
        state.settings.Values().validated_sidecar_cache_mib);
    if (io_status != INKPOD_STATUS_OK) {
        return io_status;
    }
    try {
        state.engine = std::make_unique<CoreHost>();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Start(
        renderer::GetCanvasSnapshotSink(state.Workspace().windows.canvas),
        state.Workspace().windows.window,
        state.file_io.Manager());
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (SetTimer(state.Workspace().windows.window, kFileIoPollTimer,
            kFileIoPollMilliseconds, nullptr) == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const DocumentSessionId session = state.routing.targets.ReplaceDocument();
    if (!state.ReplaceDocumentSession(
            session,
            state.routing.targets.CurrentGeneration(),
            state.routing.targets.ActiveDocumentView())) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return windows::ui::InitializeShortcuts(
        *state.engine, state.shortcuts, state.settings.Values().shortcuts);
}

InkpodStatus StopCore(ApplicationHost& state) noexcept {
    state.file_io.CancelAll();
    const InkpodStatus recovery_cleanup_status =
        windows::ui::runtime::DrainRecoveryCleanupsForShutdown(state)
        ? INKPOD_STATUS_OK : INKPOD_STATUS_INVALID_STATE;
    windows::ui::CloseAllHistoryVisualizationDialogs(state);
    const InkpodStatus clipboard_status = inkpod_clipboard_release(&state.clipboard);
    if (state.effects.task != nullptr) {
        inkpod_task_cancel(state.effects.task);
    }
    if (state.batch.task != nullptr) {
        inkpod_batch_task_cancel(state.batch.task);
    }
    (void)windows::ui::runtime::PersistApplicationSettings(state);
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
        ++workspace->subpalette_load_generation;
        state.file_io.Cancel(workspace->subpalette_io_request);
        workspace->subpalette_io_request = 0U;
        if (state.engine != nullptr && workspace->subpalette_candidate != nullptr) {
            (void)state.engine->ReleaseSubpalette(
                &workspace->subpalette_candidate);
        }
        workspace->subpalette_loading = false;
        if (state.engine != nullptr && workspace->subpalette != nullptr) {
            (void)state.engine->ReleaseSubpalette(&workspace->subpalette);
        }
        if (workspace->subpalette_canvas_id) {
            (void)state.routing.targets.UnregisterAuxiliaryCanvas(
                workspace->subpalette_canvas_id);
            workspace->subpalette_canvas_id = {};
            workspace->subpalette_source_id = {};
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
        state.file_io.ClearCompleted();
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
        const std::array<HWND*, 7U> owned{
            &workspace->tools.palette,
            &workspace->windows.tool_options_flyout,
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
        workspace->windows.tool_options = nullptr;
        workspace->tools.options_flyout = {};
        if (workspace->light_table_palette != nullptr) {
            DestroyWindow(workspace->light_table_palette);
            workspace->light_table_palette = nullptr;
        }
        workspace->windows.tool_palette = nullptr;
        workspace->windows.layer_palette = nullptr;
        workspace->windows.sequence_palette = nullptr;
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
    const InkpodStatus run_graph_status =
        inkpod_batch_graph_release(&state.batch.run_graph);
    for (const InkpodStatus status : {
             clipboard_status,
             task_status,
             batch_task_status,
             preview_status,
             report_status,
             graph_status,
             run_graph_status,
             recovery_cleanup_status}) {
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
    }
    state.routing.timers.Clear();
    state.routing.targets.InvalidateAll();
    return INKPOD_STATUS_OK;
}

std::wstring RecoveryDisplayPath(const RecoveryCandidate& candidate) {
    if (!candidate.has_metadata) {
        return UiText(UiStringId::Text0018);
    }
    if (!candidate.metadata.original_path.empty()) {
        return candidate.metadata.original_path;
    }
    if (!candidate.metadata.source_path.empty()) {
        return candidate.metadata.source_path;
    }
    if (!candidate.metadata.original_identity.normalized_path.empty()) {
        return candidate.metadata.original_identity.normalized_path;
    }
    return UiText(UiStringId::Text0020);
}

std::wstring RecoveryPromptText(
    const RecoveryCandidate& candidate,
    std::size_t index,
    std::size_t count,
    bool newer) {
    FILETIME local_time{};
    SYSTEMTIME system_time{};
    (void)FileTimeToLocalFileTime(&candidate.modified, &local_time);
    (void)FileTimeToSystemTime(&local_time, &system_time);
    const std::wstring original = RecoveryDisplayPath(candidate);
    std::array<wchar_t, 1024U> text{};
    _snwprintf_s(
        text.data(),
        text.size(),
        _TRUNCATE,
        UiText(UiStringId::RecoveryCandidatePromptFormat),
        index + 1U,
        count,
        original.c_str(),
        system_time.wYear,
        system_time.wMonth,
        system_time.wDay,
        system_time.wHour,
        system_time.wMinute,
        system_time.wSecond,
        static_cast<unsigned long long>(
            candidate.has_metadata ? candidate.metadata.session.Value() : 0U),
        static_cast<unsigned long long>(
            candidate.has_metadata ? candidate.metadata.generation.Value() : 0U),
        candidate.has_metadata
            ? (newer ? UiText(UiStringId::Text0490) : UiText(UiStringId::Text0082))
            : UiText(UiStringId::Text0098));
    return text.data();
}

void EnsureStartupDocument(ApplicationHost& state, CommandContext initial) noexcept {
    try {
        (void)state.file_io.WhenIdle({}, [&state, initial] {
            if (state.engine == nullptr || !initial.document_session.has_value()
                || !initial.generation.has_value() || !initial.document_view.has_value()
                || !initial.workspace.has_value()) {
                return;
            }
            const auto* workspace = state.FindWorkspace(initial.workspace.value());
            const auto* document = state.Documents().Find(initial.document_session.value());
            if (workspace == nullptr || !IsWindowVisible(workspace->windows.window)
                || document == nullptr || document->generation != initial.generation.value()) {
                return;
            }
            InkpodDocumentInfo info{};
            info.struct_size = sizeof(info);
            if (state.engine->GetDocumentInfo(document->id, document->generation, info)) {
                return;
            }
            const auto previous_view = state.routing.targets.ActiveDocumentView();
            if (state.ActivateDocumentView(initial.document_view.value())) {
                const InkpodStatus status = windows::ui::runtime::CreateDefaultCell(state);
                if (status != INKPOD_STATUS_OK) {
                    windows::ui::runtime::ShowCoreError(
                        state, workspace->windows.window, UiText(UiStringId::Text0225));
                }
            }
            if (previous_view && previous_view != initial.document_view.value()) {
                (void)state.ActivateDocumentView(previous_view);
            }
            windows::ui::runtime::UpdateMenuState(state);
        });
    } catch (const std::bad_alloc&) {
        state.engine->SetLocalFailure(UiText(UiStringId::Text0225));
    }
}

struct RecoveryReview final : std::enable_shared_from_this<RecoveryReview> {
    ApplicationHost* state{};
    CommandContext initial;
    Generation workspace_generation{};
    HWND owner{};
    std::vector<RecoveryCandidate> candidates;
    std::size_t index{};

    bool Valid() const noexcept {
        const auto* workspace = initial.workspace.has_value()
            ? state->FindWorkspace(initial.workspace.value()) : nullptr;
        return state->engine != nullptr && workspace != nullptr
            && workspace->generation == workspace_generation
            && workspace->windows.window == owner && IsWindow(owner);
    }

    void Finish(bool cancelled = false) {
        if (!cancelled && Valid()) {
            EnsureStartupDocument(*state, initial);
        }
    }

    void Continue() {
        const auto self = shared_from_this();
        if (!state->file_io.WhenIdle({}, [self] { self->Next(); })) {
            Finish();
        }
    }

    void Prompt(bool newer) {
        if (!Valid() || index >= candidates.size()) {
            Finish();
            return;
        }
        const auto& candidate = candidates[index];
        const std::wstring prompt = RecoveryPromptText(candidate, index, candidates.size(), newer);
        const int choice = MessageBoxW(owner, prompt.c_str(), L"inkpod Recovery",
            MB_YESNOCANCEL | MB_ICONQUESTION);
        if (!Valid()) {
            return;
        }
        if (choice == IDYES) {
            const auto previous_view = state->routing.targets.ActiveDocumentView();
            (void)state->ActivateWorkspaceWindow(initial.workspace.value(), false);
            const InkpodStatus status = windows::ui::runtime::OpenRecoveryCandidate(*state, candidate);
            if (previous_view) {
                (void)state->ActivateDocumentView(previous_view);
            }
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_PENDING) {
                windows::ui::runtime::ShowCoreError(*state, owner, UiText(UiStringId::Text0080));
            }
        } else if (choice == IDNO) {
            FileIoRequest request{};
            request.context = initial;
            request.kind = INKPOD_IO_RECOVERY_DISCARD;
            request.paths = {candidate.recovery_path};
            const auto self = shared_from_this();
            if (!state->file_io.Queue(*state->engine, std::move(request),
                    [self](FileIoResult&& result) {
                        if (result.status != INKPOD_STATUS_OK && self->Valid()) {
                            MessageBoxW(self->owner, UiText(UiStringId::Text0079),
                                L"inkpod Recovery", MB_OK | MB_ICONWARNING);
                        }
                    })) {
                MessageBoxW(owner, UiText(UiStringId::Text0079),
                    L"inkpod Recovery", MB_OK | MB_ICONWARNING);
            }
        }
        ++index;
        Continue();
    }

    void Next() {
        if (!Valid() || index >= candidates.size()) {
            Finish();
            return;
        }
        const auto& candidate = candidates[index];
        if (!candidate.has_metadata || candidate.metadata.original_path.empty()) {
            Prompt(false);
            return;
        }
        FileIoRequest request{};
        request.context = initial;
        request.kind = INKPOD_IO_RECOVERY_PROBE;
        request.paths = {candidate.metadata.original_path, candidate.recovery_path};
        const auto self = shared_from_this();
        if (!state->file_io.Queue(*state->engine, std::move(request),
                [self](FileIoResult&& result) {
                    if (result.status == INKPOD_STATUS_CANCELLED) {
                        self->Finish(true);
                        return;
                    }
                    self->Prompt(result.status == INKPOD_STATUS_OK && !result.items.empty());
                })) {
            Finish();
        }
    }
};

bool ReviewRecoveryCandidates(
    ApplicationHost& state,
    HWND owner,
    bool& document_initialized) noexcept {
    try {
        std::wstring directory;
        if (!RecoveryRootDirectory(directory)) {
            return false;
        }
        auto review = std::make_shared<RecoveryReview>();
        review->state = &state;
        review->initial = state.routing.targets.Capture();
        review->workspace_generation = state.Workspace().generation;
        review->owner = owner;
        FileIoRequest request{};
        request.context = review->initial;
        request.kind = INKPOD_IO_RECOVERY_LIST;
        request.paths = {std::move(directory)};
        if (!state.file_io.Queue(*state.engine, std::move(request),
                [review](FileIoResult&& result) {
                    if (!review->Valid()) {
                        return;
                    }
                    if (result.status != INKPOD_STATUS_OK) {
                        if (result.status != INKPOD_STATUS_CANCELLED) {
                            MessageBoxW(review->owner, UiText(UiStringId::RecoveryEnumerationFailure),
                                L"inkpod Recovery", MB_OK | MB_ICONWARNING);
                        }
                        review->Finish(result.status == INKPOD_STATUS_CANCELLED);
                        return;
                    }
                    review->candidates = std::move(result.recovery_candidates);
                    review->Continue();
                })) {
            return false;
        }
        document_initialized = true;
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}
bool OpenDocumentPaths(
    ApplicationHost& state,
    HWND owner,
    const std::vector<std::wstring>& paths,
    bool& document_initialized) noexcept {
    bool all_opened = true;
    for (const auto& path : paths) {
        const InkpodStatus status = windows::ui::runtime::OpenDocumentFromPath(
            state, path);
        if (status == INKPOD_STATUS_OK || status == INKPOD_STATUS_PENDING) {
            document_initialized = true;
        } else {
            windows::ui::runtime::ShowCoreError(
                state, owner, UiText(UiStringId::Text0940));
            all_opened = false;
        }
    }
    return all_opened;
}

void RestorePreviousDocuments(
    ApplicationHost& state,
    HWND owner,
    bool& document_initialized) noexcept {
    if (!state.lifetime.restore_previous_documents) {
        return;
    }
    std::vector<std::wstring> paths;
    if (!LoadPreviousDocumentPaths(paths)) {
        MessageBoxW(
            owner,
            UiText(UiStringId::Text0536),
            L"inkpod",
            MB_OK | MB_ICONWARNING);
        return;
    }
    (void)OpenDocumentPaths(state, owner, paths, document_initialized);
}

void SavePreviousDocuments(ApplicationHost& state) noexcept {
    if (!state.lifetime.restore_previous_documents) {
        (void)ClearPreviousDocumentPaths();
        return;
    }
    std::vector<std::wstring> paths;
    try {
        paths.reserve(state.Documents().Count());
        for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
            const DocumentSession* document = state.Documents().SessionAt(index);
            if (document == nullptr || document->shell.current_path.empty()
                || std::find(
                    paths.begin(), paths.end(), document->shell.current_path)
                    != paths.end()) {
                continue;
            }
            paths.push_back(document->shell.current_path);
        }
    } catch (const std::bad_alloc&) {
        return;
    }
    (void)SavePreviousDocumentPaths(paths);
}

int RunMessageLoop(ApplicationHost& state) noexcept {
    MSG message{};
    BOOL result{};
    while ((result = GetMessageW(&message, nullptr, 0, 0)) > 0) {
        DispatchApplicationMessage(state, message);
    }
    return result == -1 ? 17 : static_cast<int>(message.wParam);
}

}  // namespace

void DispatchApplicationMessage(ApplicationHost& state, MSG& message) noexcept {
    if (message.hwnd == nullptr
        && message.message == kApplicationActivationMessage) {
        const std::uint64_t token =
            static_cast<std::uint64_t>(message.wParam & UINT32_MAX)
            | (static_cast<std::uint64_t>(
                   static_cast<std::uint32_t>(message.lParam))
               << 32U);
        ActivationRequest request{};
        if (state.activation != nullptr && state.activation->Take(token, request)) {
            (void)windows::ui::runtime::HandleApplicationActivation(state, request);
        }
        return;
    }
    for (std::size_t workspace_index = 0U;
         workspace_index < state.Workspaces().Count(); ++workspace_index) {
        const WorkspaceWindow* workspace = state.Workspaces().At(workspace_index);
        if (workspace == nullptr) {
            continue;
        }
        const std::array<HWND, 8U> palettes{
            workspace->tools.palette,
            workspace->windows.tool_options_flyout,
            workspace->panes.layer_palette,
            workspace->batch_palette,
            workspace->locator_palette,
            workspace->sequence_palette,
            workspace->light_table_palette,
            workspace->subpalette_palette};
        for (const HWND palette : palettes) {
            if (palette != nullptr && IsWindowVisible(palette) != FALSE
                && IsDialogMessageW(palette, &message) != FALSE) {
                return;
            }
        }
    }
    if (windows::ui::TranslateHistoryVisualizationDialogMessage(state, message)
        || windows::ui::runtime::PreTranslateKeyboardMessage(state, message)) {
        return;
    }
    TranslateMessage(&message);
    DispatchMessageW(&message);
}

Application::Application(ApplicationLaunch launch) noexcept
    : launch_(std::move(launch)) {}

Application::~Application() = default;

int Application::Run() {
    try {
        host_ = std::make_unique<ApplicationHost>();
        if (!launch_.smoke_test) {
            host_->activation = std::make_unique<ActivationService>();
        }
    } catch (const std::bad_alloc&) {
        return 14;
    }
    if (!launch_.smoke_test) {
        MSG queue_probe{};
        (void)PeekMessageW(&queue_probe, nullptr, WM_USER, WM_USER, PM_NOREMOVE);
        const ActivationRole role = host_->activation->Start(GetCurrentThreadId());
        if (role == ActivationRole::Failed) {
            MessageBoxW(
                nullptr,
                UiText(UiStringId::Text0563),
                L"inkpod",
                MB_OK | MB_ICONERROR);
            host_.reset();
            return 19;
        }
        if (role == ActivationRole::Secondary) {
            ActivationRequest request{};
            request.request_id = NewActivationRequestId();
            request.target = launch_.open_in_new_workspace
                ? ActivationTargetPreference::NewWorkspace
                : ActivationTargetPreference::LastFocusedWorkspace;
            try {
                request.paths = launch_.document_paths;
            } catch (const std::bad_alloc&) {
                host_.reset();
                return 14;
            }
            const ActivationSendStatus sent = host_->activation->Send(
                request, 5000U);
            if (sent != ActivationSendStatus::Accepted
                && sent != ActivationSendStatus::Duplicate) {
                MessageBoxW(
                    nullptr,
                    sent == ActivationSendStatus::Timeout
                        ? UiText(UiStringId::Text0941)
                        : UiText(UiStringId::Text0942),
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
                host_.reset();
                return 20;
            }
            host_.reset();
            return 0;
        }
    }

    windows::ui::ShortcutProfileSet default_shortcuts{};
    try {
        default_shortcuts = windows::ui::BuildDefaultShortcutProfileSet();
    } catch (const std::bad_alloc&) {
        host_.reset();
        return 14;
    }
    const ApplicationSettingsLoadResult settings_result = launch_.smoke_test
        ? (host_->settings.UseDefaults(default_shortcuts)
               ? ApplicationSettingsLoadResult::Missing
               : ApplicationSettingsLoadResult::IoError)
        : host_->settings.Load(default_shortcuts);
    if (settings_result == ApplicationSettingsLoadResult::IoError) {
        host_.reset();
        return 14;
    }
    if (settings_result == ApplicationSettingsLoadResult::Invalid) {
        OutputDebugStringW(
            L"inkpod: inkpod-settings.json is invalid; defaults are active.\n");
    }

    INITCOMMONCONTROLSEX controls{};
    controls.dwSize = sizeof(controls);
    controls.dwICC = ICC_STANDARD_CLASSES | ICC_BAR_CLASSES | ICC_TAB_CLASSES
        | ICC_LISTVIEW_CLASSES | ICC_UPDOWN_CLASS | ICC_PROGRESS_CLASS
        | ICC_HOTKEY_CLASS;
    if (!InitCommonControlsEx(&controls)) {
        MessageBoxW(
            nullptr,
            UiText(UiStringId::Text0056),
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 10;
    }

    ComApartment com;
    if (FAILED(com.Initialize())) {
        MessageBoxW(
            nullptr,
            UiText(UiStringId::Text0042),
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 11;
    }

    std::array<wchar_t, 128> title{};
    std::array<wchar_t, 128> class_name{};
    if (LoadLocalizedStringW(
            launch_.instance,
            IDS_APP_TITLE,
            title.data(),
            static_cast<int>(title.size())) == 0
        || LoadLocalizedStringW(
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

    ApplicationHost& state = *host_;
    state.lifetime.instance = launch_.instance;
    state.lifetime.window_class_name = class_name.data();
    state.lifetime.window_title = title.data();
    state.lifetime.show_command = launch_.show_command;
    state.lifetime.smoke_test = launch_.smoke_test;
    const ApplicationSettings& settings = state.settings.Values();
    state.lifetime.restore_previous_documents =
        settings.restore_previous_documents;
    state.lifetime.sequence_switch_policy = settings.sequence_switch_policy;
    state.lifetime.sequence_endpoint_policy = settings.sequence_endpoint_policy;
    state.effects.output_color_guard_profile =
        static_cast<InkpodOutputColorGuardProfile>(
            settings.output_color_guard_profile);
    if (!InitializeFrontendRouting(state)) {
        host_.reset();
        return 14;
    }
    if (FAILED(StartRenderer(state))) {
        state.ClearOwners();
        host_.reset();
        return 15;
    }
    HMENU menu = windows::ui::LoadLocalizedMenuW(
        launch_.instance, MAKEINTRESOURCEW(IDR_MAIN_MENU));
    if (menu == nullptr) {
        state.renderer->Stop();
        state.ClearOwners();
        host_.reset();
        return 14;
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
        menu,
        launch_.instance,
        &state.Workspace());
    windows::ui::runtime::ApplySystemDarkTitleBar(window);
    if (window == nullptr) {
        DestroyMenu(menu);
        state.renderer->Stop();
        state.ClearOwners();
        host_.reset();
        return 14;
    }

    InkpodStatus core_status = StartCore(state);
    if (core_status != INKPOD_STATUS_OK) {
        if (!launch_.smoke_test) {
            windows::ui::runtime::ShowCoreError(
                state, window, UiText(UiStringId::Text0083));
        }
        StopCore(state);
        DestroyWindow(window);
        state.ClearOwners();
        host_.reset();
        return 15;
    }

    const CommandContext initial_document = state.routing.targets.Capture();
    bool document_initialized{};
    if (!launch_.document_paths.empty()) {
        (void)OpenDocumentPaths(
            state, window, launch_.document_paths, document_initialized);
    }
    if (!launch_.smoke_test) {
        const bool reviewing_recovery =
            ReviewRecoveryCandidates(state, window, document_initialized);
        RestorePreviousDocuments(state, window, document_initialized);
        if (!reviewing_recovery && document_initialized) {
            EnsureStartupDocument(state, initial_document);
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
                UiText(UiStringId::Text0225));
        }
        StopCore(state);
        DestroyWindow(window);
        state.ClearOwners();
        host_.reset();
        return 16;
    }
    const WorkspaceWindowId initial_workspace = state.Workspace().id;
    if (!launch_.smoke_test) {
        const std::uint32_t window_count = static_cast<std::uint32_t>(
            std::max<std::size_t>(1U, state.settings.Values().workspaces.size()));
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
        exit_code = launch_.sequence_performance_smoke_test
            ? windows::ui::RunSequencePerformanceSmoke(state)
            : launch_.performance_smoke_test
            ? windows::ui::RunPerformanceSmoke(state)
            : windows::ui::RunApplicationSmoke(state);
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

    if (!launch_.smoke_test) {
        if (state.activation != nullptr) {
            state.activation->Stop();
        }
        SavePreviousDocuments(state);
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
