#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <oleacc.h>
#include <shlobj.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <chrono>
#include <climits>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <cwctype>
#include <functional>
#include <memory>
#include <optional>
#include <span>
#include <string>
#include <utility>
#include <vector>

#include "app/application_host.h"
#include "app/activation.h"
#include "canvas.h"
#include "app/clipboard_adapter.h"
#include "app/core_host.h"
#include "app/document_shell.h"
#include "inkpod/core_ffi.h"
#include "app/resource.h"
#include "app/session_recovery.h"
#include "ui/dialogs/about_dialog.h"
#include "ui/dialogs/basic_dialogs.h"
#include "ui/dialogs/batch_dialog.h"
#include "ui/dialogs/effects_dialogs.h"
#include "ui/dialogs/layer_palette.h"
#include "ui/command_catalog.h"
#include "ui/command_state.h"
#include "ui/shortcut_controller.h"
#include "ui/panes/document_panes.h"
#include "ui/panes/color_panes.h"
#include "ui/tools/fill_controller.h"
#include "ui/tools/floating_paste_controller.h"
#include "ui/tools/selection_controller.h"
#include "ui/tools/tool_state.h"
#include "ui/tools/view_controller.h"
#include "ui/tools/vector_controller.h"
#include "ui/effects_controller.h"
#include "ui/batch_controller.h"
#include "ui/main_window.h"
#include "ui/main_window_runtime.h"

#include "app/app_smoke.h"

namespace inkpod::windows::ui::runtime {

using inkpod::app::ApplicationHost;
using inkpod::app::ActivationRequest;
using inkpod::app::ActivationTargetPreference;
using inkpod::app::DiscardRecoveryArtifact;
using inkpod::app::EnumerateRecoveryCandidates;
using inkpod::app::InkpodClipboardFormat;
using inkpod::app::PublishStandardClipboard;
using inkpod::app::ReadBoundedFile;
using inkpod::app::ReadRecoveryMetadata;
using inkpod::app::RecoveryCandidate;
using inkpod::app::RecoveryMetadata;
using inkpod::app::RecoveryMetadataPath;
using inkpod::app::SequenceCellSwitchPolicy;
using inkpod::app::WriteFileAtomically;
using inkpod::app::CommandTimerKind;
using inkpod::app::DocumentSessionId;
using inkpod::app::DocumentViewId;
using inkpod::windows::ui::tools::kInteractionEffectAirbrush;

bool CommandSurfacesMatchComputedState(const ApplicationHost& state) noexcept;

template <typename Mutator>
bool UpdateEditorFillOptionsForSmoke(
    ApplicationHost& state, Mutator&& mutate) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    InkpodEditorStateInfo editor{};
    editor.struct_size = sizeof(editor);
    if (!state.engine->GetEditorState(
            state.Document().id, state.Document().generation, editor)) {
        return false;
    }
    InkpodEditorStateUpdate update{};
    update.struct_size = sizeof(update);
    update.kind = INKPOD_EDITOR_UPDATE_FILL_OPTIONS;
    update.expected_editor_revision = editor.editor_revision;
    update.fill = editor.fill;
    update.fill.struct_size = sizeof(InkpodEditorFillOptions);
    std::forward<Mutator>(mutate)(update.fill);
    return state.UpdateEditorState(update) == INKPOD_STATUS_OK;
}

template <typename Mutator>
bool UpdateEditorSelectionOptionsForSmoke(
    ApplicationHost& state, Mutator&& mutate) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    InkpodEditorStateInfo editor{};
    editor.struct_size = sizeof(editor);
    if (!state.engine->GetEditorState(
            state.Document().id, state.Document().generation, editor)) {
        return false;
    }
    InkpodEditorStateUpdate update{};
    update.struct_size = sizeof(update);
    update.kind = INKPOD_EDITOR_UPDATE_SELECTION_OPTIONS;
    update.expected_editor_revision = editor.editor_revision;
    update.selection = editor.selection;
    update.selection.struct_size = sizeof(InkpodEditorSelectionOptions);
    std::forward<Mutator>(mutate)(update.selection);
    return state.UpdateEditorState(update) == INKPOD_STATUS_OK;
}

bool WindowHasAccessibleName(HWND window) noexcept {
    IAccessible* accessible = nullptr;
    const HRESULT object_result = AccessibleObjectFromWindow(
        window,
        static_cast<DWORD>(OBJID_CLIENT),
        IID_IAccessible,
        reinterpret_cast<void**>(&accessible));
    if (FAILED(object_result) || accessible == nullptr) {
        return false;
    }
    VARIANT self{};
    self.vt = VT_I4;
    self.lVal = CHILDID_SELF;
    BSTR name = nullptr;
    const HRESULT name_result = accessible->get_accName(self, &name);
    const bool has_name =
        SUCCEEDED(name_result) && name != nullptr && SysStringLen(name) > 0U;
    SysFreeString(name);
    accessible->Release();
    return has_name;
}

bool AccessibleWindowNameContains(
    HWND window, std::wstring_view expected) noexcept {
    if (window == nullptr || expected.empty()) {
        return false;
    }
    IAccessible* accessible = nullptr;
    const HRESULT object_result = AccessibleObjectFromWindow(
        window,
        static_cast<DWORD>(OBJID_CLIENT),
        IID_IAccessible,
        reinterpret_cast<void**>(&accessible));
    if (FAILED(object_result) || accessible == nullptr) {
        return false;
    }
    VARIANT self{};
    self.vt = VT_I4;
    self.lVal = CHILDID_SELF;
    BSTR name = nullptr;
    const HRESULT name_result = accessible->get_accName(self, &name);
    const bool contains = SUCCEEDED(name_result) && name != nullptr
        && std::wstring_view(name, SysStringLen(name)).find(expected)
            != std::wstring_view::npos;
    SysFreeString(name);
    accessible->Release();
    return contains;
}

bool WindowHasVisibleStyle(HWND window) noexcept {
    return window != nullptr
        && (static_cast<DWORD>(GetWindowLongPtrW(window, GWL_STYLE))
            & WS_VISIBLE)
            != 0U;
}

bool AccessibleChildNameContains(
    HWND window, std::wstring_view expected) noexcept {
    if (window == nullptr || expected.empty()) {
        return false;
    }
    IAccessible* accessible = nullptr;
    const HRESULT object_result = AccessibleObjectFromWindow(
        window,
        static_cast<DWORD>(OBJID_CLIENT),
        IID_IAccessible,
        reinterpret_cast<void**>(&accessible));
    if (FAILED(object_result) || accessible == nullptr) {
        return false;
    }
    LONG child_count = 0;
    bool found = false;
    if (SUCCEEDED(accessible->get_accChildCount(&child_count))) {
        for (LONG child = 1; child <= child_count && !found; ++child) {
            VARIANT child_id{};
            child_id.vt = VT_I4;
            child_id.lVal = child;
            BSTR name = nullptr;
            if (SUCCEEDED(accessible->get_accName(child_id, &name))
                && name != nullptr) {
                const std::wstring_view value(name, SysStringLen(name));
                found = value.find(expected) != std::wstring_view::npos;
            }
            SysFreeString(name);
        }
    }
    accessible->Release();
    return found;
}

bool RouteKeyboardKey(
    ApplicationHost& state,
    UINT virtual_key,
    bool control,
    bool shift) noexcept {
    std::array<BYTE, 256U> keyboard{};
    GetKeyboardState(keyboard.data());
    const BYTE previous_control = keyboard[VK_CONTROL];
    const BYTE previous_shift = keyboard[VK_SHIFT];
    keyboard[VK_CONTROL] = control ? static_cast<BYTE>(0x80U) : 0U;
    keyboard[VK_SHIFT] = shift ? static_cast<BYTE>(0x80U) : 0U;
    SetKeyboardState(keyboard.data());
    MSG key{};
    key.hwnd = GetFocus() != nullptr
        ? GetFocus()
        : state.Workspace().windows.window;
    key.message = WM_KEYDOWN;
    key.wParam = virtual_key;
    const bool handled = PreTranslateKeyboardMessage(state, key);
    keyboard[VK_CONTROL] = previous_control;
    keyboard[VK_SHIFT] = previous_shift;
    SetKeyboardState(keyboard.data());
    return handled;
}

bool IsCaptionlessAccessibleSplitter(HWND window) noexcept {
    return window != nullptr
        && (GetWindowLongPtrW(window, GWL_STYLE) & WS_TABSTOP) != 0
        && GetWindowTextLengthW(window) == 0
        && WindowHasAccessibleName(window);
}

InkpodStatus CreateCell(ApplicationHost& state, std::uint32_t width, std::uint32_t height, std::uint32_t dpi_milli) noexcept;
InkpodStatus CreateCellsFromOptions(
    ApplicationHost& state,
    const InkpodCellCreationOptions& options,
    std::optional<std::uint32_t> smoke_failure_index,
    std::vector<ApplicationHost::DocumentBinding>* created = nullptr) noexcept;
bool DispatchEnabledCommand(
    ApplicationHost& state,
    HWND window,
    UINT command,
    std::optional<inkpod::app::PaneInstanceId> pane = std::nullopt) noexcept;
std::optional<LRESULT> IssueCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM lparam,
    std::optional<inkpod::app::PaneInstanceId> pane) noexcept;
InkpodDocumentInfo EmptyDocumentInfo() noexcept;
InkpodStatus ApplyView(
    ApplicationHost& state,
    InkpodViewCommandKind kind,
    double value1,
    double value2,
    double value3 = 0.0,
    double value4 = 0.0) noexcept;
bool ActivateDocumentTab(
    ApplicationHost& state,
    inkpod::app::DocumentViewId view) noexcept;
bool ConfirmAllDocuments(ApplicationHost& state) noexcept;
InkpodStatus FinishVectorCanvasGesture(ApplicationHost& state) noexcept;
InkpodStatus FitCanvas(ApplicationHost& state, InkpodViewCommandKind kind) noexcept;
InkpodStatus ImportCommonRasterFromPath(
    ApplicationHost& state, const std::wstring& path) noexcept;
InkpodStatus OpenFromPath(ApplicationHost& state, const std::wstring& path) noexcept;
void PumpPendingWindowMessages() noexcept;
bool QueryDocument(ApplicationHost& state, InkpodDocumentInfo& info) noexcept;
bool QuerySnapshotTransform(
    ApplicationHost& state, InkpodSnapshotTransform& transform) noexcept;
bool QueryLightTableItem(ApplicationHost& state, std::uint32_t index, InkpodLightTableItemInfo& output) noexcept;
bool QueueAutosave(
    ApplicationHost& state,
    const inkpod::app::CommandContext& context,
    const std::wstring& path) noexcept;
bool RefreshColorPanes(ApplicationHost& state) noexcept;
bool RefreshLightTablePane(ApplicationHost& state) noexcept;
bool RefreshSequencePane(ApplicationHost& state) noexcept;
bool RefreshTreePane(ApplicationHost& state) noexcept;
void ResetUiForNewActiveDocument(ApplicationHost& state) noexcept;
bool ResolveConfiguredShortcut(ApplicationHost& state, std::uint32_t virtual_key, std::uint32_t modifiers, UINT& menu_command) noexcept;
bool SamePersistentMetadata(const InkpodDocumentInfo& left, const InkpodDocumentInfo& right) noexcept;
InkpodStatus SaveToPath(ApplicationHost& state, const std::wstring& path) noexcept;
InkpodStatus SetEditorActiveTool(
    ApplicationHost& state, std::uint32_t tool) noexcept;
InkpodStatus SetEditorActiveTarget(
    ApplicationHost& state,
    std::uint64_t layer_id,
    std::uint64_t plane_id) noexcept;

bool QueryCoreResourceUsage(
    ApplicationHost& state,
    inkpod::app::DocumentSessionId session,
    inkpod::app::Generation generation,
    InkpodResourceUsage& usage) noexcept {
    usage = {};
    usage.struct_size = sizeof(usage);
    return state.engine != nullptr
        && state.engine->Invoke(
               session,
               generation,
               [&usage](InkpodCore* core) {
                   return inkpod_core_get_resource_usage(core, &usage);
               },
               false,
               false) == INKPOD_STATUS_OK;
}

bool ValidateFixedResourceScenario(
    ApplicationHost& state,
    const char* scenario,
    std::uint64_t expected_workspaces,
    std::uint64_t expected_documents,
    std::uint64_t expected_views,
    std::uint64_t expected_groups) noexcept {
    const inkpod::app::ApplicationResourceUsage usage = state.ResourceUsage();
    if (scenario == nullptr
        || usage.workspace_window_count != expected_workspaces
        || usage.document_session_count != expected_documents
        || usage.document_view_count != expected_views
        || usage.editor_group_count != expected_groups
        || usage.editor_canvas_count != expected_groups
        || usage.registered_snapshot_sink_count != expected_groups
        || usage.auxiliary_canvas_count != expected_workspaces
        || usage.pane_instance_count != expected_workspaces * 10U
        || usage.thumbnails.budget_bytes == 0U
        || usage.thumbnails.resident_bytes > usage.thumbnails.budget_bytes
        || usage.renderer.gpu_tile_budget_bytes == 0U
        || usage.renderer.gpu_tile_bytes > usage.renderer.gpu_tile_budget_bytes
         || usage.renderer.surface_count
             != usage.editor_canvas_count + usage.auxiliary_canvas_count) {
        std::fprintf(
            stderr,
            "G13 resource mismatch scenario=%s workspaces=%llu/%llu "
            "documents=%llu/%llu views=%llu/%llu groups=%llu/%llu "
            "editor_canvases=%llu snapshot_sinks=%llu auxiliary_canvases=%llu panes=%llu "
            "thumbnail=%llu/%llu renderer_surfaces=%llu renderer_budget=%llu/%llu\n",
            scenario == nullptr ? "(null)" : scenario,
            static_cast<unsigned long long>(usage.workspace_window_count),
            static_cast<unsigned long long>(expected_workspaces),
            static_cast<unsigned long long>(usage.document_session_count),
            static_cast<unsigned long long>(expected_documents),
            static_cast<unsigned long long>(usage.document_view_count),
            static_cast<unsigned long long>(expected_views),
            static_cast<unsigned long long>(usage.editor_group_count),
            static_cast<unsigned long long>(expected_groups),
            static_cast<unsigned long long>(usage.editor_canvas_count),
            static_cast<unsigned long long>(
                usage.registered_snapshot_sink_count),
            static_cast<unsigned long long>(usage.auxiliary_canvas_count),
            static_cast<unsigned long long>(usage.pane_instance_count),
            static_cast<unsigned long long>(usage.thumbnails.resident_bytes),
            static_cast<unsigned long long>(usage.thumbnails.budget_bytes),
            static_cast<unsigned long long>(usage.renderer.surface_count),
            static_cast<unsigned long long>(usage.renderer.gpu_tile_bytes),
            static_cast<unsigned long long>(usage.renderer.gpu_tile_budget_bytes));
        return false;
    }

    std::uint64_t document_tile_bytes{};
    std::uint64_t history_bytes{};
    std::uint64_t reference_bytes{};
    for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
        const auto* document = state.Documents().SessionAt(index);
        InkpodResourceUsage core{};
        if (document == nullptr
            || !QueryCoreResourceUsage(
                state, document->id, document->generation, core)) {
            return false;
        }
        document_tile_bytes += core.document_tile_bytes;
        history_bytes += core.history_bytes;
        reference_bytes += core.reference_light_table_bytes;
    }

    for (std::size_t workspace_index = 0U;
         workspace_index < state.Workspaces().Count();
         ++workspace_index) {
        const auto* workspace = state.Workspaces().At(workspace_index);
        if (workspace == nullptr) {
            return false;
        }
        inkpod::app::PaneResourceUsage layer_usage{};
        inkpod::app::PaneResourceUsage sequence_usage{};
        if (!state.GetPaneResourceUsage(workspace->pane_ids.layer, layer_usage)
            || !state.GetPaneResourceUsage(
                workspace->pane_ids.sequence, sequence_usage)
            || layer_usage.workspace != workspace->id
            || sequence_usage.workspace != workspace->id) {
            return false;
        }
        for (std::size_t group_index = 0U;
             group_index < workspace->editors.GroupCount();
             ++group_index) {
            const auto* group = workspace->editors.GroupAt(group_index);
            inkpod::renderer::RendererSurfaceResourceUsage surface{};
            const auto* document = group == nullptr
                ? nullptr
                : state.Documents().FindByView(group->ActiveView());
            if (group == nullptr || document == nullptr
                || !state.renderer->GetSurfaceResourceUsage(
                    group->canvas_id, group->generation, surface)
                || surface.route.canvas != group->canvas_id
                || surface.route.document_session != document->id
                || surface.route.document_view != group->ActiveView()
                || surface.route.document_generation != document->generation
                || surface.route.surface_generation != group->generation) {
                return false;
            }
        }
    }

    std::fprintf(
        stdout,
        "inkpod-g13-resource scenario=%s workspaces=%llu documents=%llu "
        "views=%llu editor_canvases=%llu snapshot_sinks=%llu "
        "auxiliary_canvases=%llu panes=%llu "
        "core_tile_bytes=%llu history_bytes=%llu reference_bytes=%llu "
        "thumbnail_bytes=%llu renderer_bytes=%llu\n",
        scenario,
        static_cast<unsigned long long>(usage.workspace_window_count),
        static_cast<unsigned long long>(usage.document_session_count),
        static_cast<unsigned long long>(usage.document_view_count),
        static_cast<unsigned long long>(usage.editor_canvas_count),
        static_cast<unsigned long long>(usage.registered_snapshot_sink_count),
        static_cast<unsigned long long>(usage.auxiliary_canvas_count),
        static_cast<unsigned long long>(usage.pane_instance_count),
        static_cast<unsigned long long>(document_tile_bytes),
        static_cast<unsigned long long>(history_bytes),
        static_cast<unsigned long long>(reference_bytes),
        static_cast<unsigned long long>(usage.thumbnails.resident_bytes),
        static_cast<unsigned long long>(
            usage.renderer.retained_snapshot_bytes
            + usage.renderer.pending_snapshot_bytes
            + usage.renderer.gpu_tile_bytes
            + usage.renderer.swap_chain_bytes));
    return true;
}

int RunLocatorPaneSmoke(ApplicationHost& state) noexcept {
    const HWND pane = state.Workspace().locator_palette;
    const HWND workspace_window = state.Workspace().windows.window;
    const HMENU menu = GetMenu(workspace_window);
    if (pane == nullptr || workspace_window == nullptr || menu == nullptr
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Locator)
        || GetParent(pane) != workspace_window
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_STYLE)) & WS_CHILD) == 0U
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_EXSTYLE))
            & (WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)) != 0U
        || !WindowHasAccessibleName(pane)) {
        return 850;
    }
    const UINT dpi = GetDpiForWindow(workspace_window);
    const HMONITOR monitor = MonitorFromWindow(
        workspace_window, MONITOR_DEFAULTTONEAREST);
    MONITORINFO monitor_info{sizeof(monitor_info)};
    if (monitor == nullptr || GetMonitorInfoW(monitor, &monitor_info) == FALSE) {
        return 1001;
    }
    const int work_width = std::max(
        1,
        static_cast<int>(
            monitor_info.rcWork.right - monitor_info.rcWork.left));
    const int work_height = std::max(
        1,
        static_cast<int>(
            monitor_info.rcWork.bottom - monitor_info.rcWork.top));
    if (SetWindowPos(
            workspace_window,
            nullptr,
            monitor_info.rcWork.left,
            monitor_info.rcWork.top,
            std::min(
                work_width, MulDiv(1'400, static_cast<int>(dpi), 96)),
            std::min(
                work_height, MulDiv(900, static_cast<int>(dpi), 96)),
            SWP_NOACTIVATE | SWP_NOZORDER) == FALSE) {
        return 1001;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_LOCATOR,
            0) != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Locator)
        || (GetMenuState(menu, IDM_WINDOW_LOCATOR, MF_BYCOMMAND) & MF_CHECKED) == 0U
        || GetDlgItem(pane, IDC_LOCATOR_TARGET) == nullptr
        || GetDlgItem(pane, IDC_LOCATOR_PIN) == nullptr
        || GetDlgItem(pane, IDC_LOCATOR_NEIGHBORHOOD) == nullptr
        || GetDlgItem(pane, IDC_LOCATOR_FIXED) == nullptr
        || GetDlgItem(pane, IDC_LOCATOR_AUTOSCROLL) == nullptr) {
        return 851;
    }
    const HWND locator_header =
        state.Workspace().windows.dock_host.HeaderWindow(DockPaneType::Locator);
    std::array<wchar_t, 64U> locator_title{};
    TCITEMW locator_tab{};
    locator_tab.mask = TCIF_TEXT;
    locator_tab.pszText = locator_title.data();
    locator_tab.cchTextMax = static_cast<int>(locator_title.size());
    RECT locator_header_bounds{};
    RECT locator_content_bounds{};
    if (!WindowHasVisibleStyle(pane)) {
        return 1002;
    }
    if (locator_header == nullptr) {
        return 987;
    }
    if (!WindowHasVisibleStyle(locator_header)) {
        return 988;
    }
    if (TabCtrl_GetItemCount(locator_header) != 1) {
        return 989;
    }
    if (TabCtrl_GetItem(locator_header, 0, &locator_tab) == FALSE) {
        return 990;
    }
    if (std::wcscmp(locator_title.data(), L"ロケーター") != 0) {
        return 991;
    }
    if (!AccessibleChildNameContains(locator_header, L"ロケーター")) {
        return 992;
    }
    if (GetWindowRect(locator_header, &locator_header_bounds) == FALSE
        || GetWindowRect(pane, &locator_content_bounds) == FALSE) {
        return 993;
    }
    if (locator_header_bounds.bottom > locator_content_bounds.top
        || locator_header_bounds.bottom <= locator_header_bounds.top) {
        return 994;
    }
    std::array<wchar_t, 256U> target_text{};
    if (GetDlgItemTextW(
            pane,
            IDC_LOCATOR_TARGET,
            target_text.data(),
            static_cast<int>(target_text.size())) <= 0
        || std::wcsstr(target_text.data(), L"追従") == nullptr) {
        return 852;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_LOCATOR_PIN,
            0) != 1) {
        return 853;
    }
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.locator_pane);
    target_text.fill(L'\0');
    if (binding == nullptr
        || binding->policy != inkpod::app::PaneTargetPolicy::PinnedDocument
        || (GetMenuState(menu, IDM_LOCATOR_PIN, MF_BYCOMMAND) & MF_CHECKED) == 0U
        || GetDlgItemTextW(
               pane,
               IDC_LOCATOR_TARGET,
               target_text.data(),
               static_cast<int>(target_text.size())) <= 0
        || std::wcsstr(target_text.data(), L"固定") == nullptr) {
        return 854;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_LOCATOR_FIXED,
            0) != 1
        || !state.Workspace().locator_fixed_mode
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LOCATOR_AUTOSCROLL,
               0) != 1
        || state.Workspace().locator_auto_scroll) {
        return 855;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_LOCATOR_FIXED,
            0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LOCATOR_AUTOSCROLL,
               0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LOCATOR_PIN,
               0) != 1
        || state.Workspace().locator_fixed_mode
        || !state.Workspace().locator_auto_scroll
        || state.routing.pane_targets.Find(state.routing.locator_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView) {
        return 856;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_LOCATOR,
            0) != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Locator)
        || (GetMenuState(menu, IDM_WINDOW_LOCATOR, MF_BYCOMMAND) & MF_CHECKED) != 0U) {
        return 857;
    }
    return 0;
}

int RunSequencePaneSmoke(ApplicationHost& state) noexcept {
    const HWND pane = state.Workspace().sequence_palette;
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (pane == nullptr || menu == nullptr
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Sequence)
        || GetParent(pane) != state.Workspace().windows.window
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_STYLE)) & WS_CHILD) == 0U
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_EXSTYLE))
            & (WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)) != 0U
        || !WindowHasAccessibleName(pane)) {
        return 867;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_SEQUENCE,
            0) != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Sequence)
        || (GetMenuState(menu, IDM_WINDOW_SEQUENCE, MF_BYCOMMAND) & MF_CHECKED) == 0U
        || GetDlgItem(pane, IDC_SEQUENCE_TARGET) == nullptr
        || GetDlgItem(pane, IDC_SEQUENCE_PIN) == nullptr
        || GetDlgItem(pane, IDC_SEQUENCE_CELLS) == nullptr
        || GetDlgItem(pane, IDC_SEQUENCE_PREVIOUS) == nullptr
        || GetDlgItem(pane, IDC_SEQUENCE_NEXT) == nullptr
        || GetDlgItem(pane, IDC_SEQUENCE_IMPORT) == nullptr) {
        return 868;
    }
    std::array<wchar_t, 256U> target_text{};
    const HWND cells = GetDlgItem(pane, IDC_SEQUENCE_CELLS);
    if (GetDlgItemTextW(
            pane,
            IDC_SEQUENCE_TARGET,
            target_text.data(),
            static_cast<int>(target_text.size())) <= 0
        || std::wcsstr(target_text.data(), L"追従") == nullptr
        || IsWindowEnabled(cells) != FALSE
        || SendMessageW(cells, LB_GETCOUNT, 0, 0) != 0) {
        return 869;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_SEQUENCE_PIN,
            0) != 1) {
        return 870;
    }
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.sequence_pane);
    target_text.fill(L'\0');
    if (binding == nullptr
        || binding->policy != inkpod::app::PaneTargetPolicy::PinnedDocument
        || (GetMenuState(menu, IDM_SEQUENCE_PIN, MF_BYCOMMAND) & MF_CHECKED) == 0U
        || GetDlgItemTextW(
               pane,
               IDC_SEQUENCE_TARGET,
               target_text.data(),
               static_cast<int>(target_text.size())) <= 0
        || std::wcsstr(target_text.data(), L"固定") == nullptr) {
        return 871;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_SEQUENCE_PIN,
            0) != 1
        || state.routing.pane_targets.Find(state.routing.sequence_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView) {
        return 872;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_SEQUENCE,
            0) != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Sequence)
        || (GetMenuState(menu, IDM_WINDOW_SEQUENCE, MF_BYCOMMAND) & MF_CHECKED) != 0U) {
        return 873;
    }
    return 0;
}

int RunLightTablePaneSmoke(ApplicationHost& state) noexcept {
    const HWND pane = state.Workspace().light_table_palette;
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (pane == nullptr || menu == nullptr
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::LightTable)
        || GetParent(pane) != state.Workspace().windows.window
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_STYLE)) & WS_CHILD) == 0U
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_EXSTYLE))
            & (WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)) != 0U
        || !WindowHasAccessibleName(pane)) {
        return 874;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_LIGHT_TABLE,
            0) != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::LightTable)
        || (GetMenuState(menu, IDM_WINDOW_LIGHT_TABLE, MF_BYCOMMAND) & MF_CHECKED) == 0U
        || GetDlgItem(pane, IDC_LIGHT_TABLE_TARGET) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_PIN) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_SETS) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_ITEMS) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_ITEM_ADD) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_ITEM_PROPERTIES) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_ITEM_MOVE) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_ITEM_SWAP) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_PREVIOUS) == nullptr
        || GetDlgItem(pane, IDC_LIGHT_TABLE_NEXT) == nullptr
        || GetDlgItem(pane, IDCANCEL) != nullptr) {
        return 875;
    }
    std::array<wchar_t, 256U> target_text{};
    const HWND sets = GetDlgItem(pane, IDC_LIGHT_TABLE_SETS);
    const HWND items = GetDlgItem(pane, IDC_LIGHT_TABLE_ITEMS);
    if (GetDlgItemTextW(
            pane,
            IDC_LIGHT_TABLE_TARGET,
            target_text.data(),
            static_cast<int>(target_text.size())) <= 0
        || std::wcsstr(target_text.data(), L"追従") == nullptr
        || SendMessageW(sets, CB_GETCOUNT, 0, 0) < 1
        || SendMessageW(items, LB_GETCOUNT, 0, 0) != 0
        || IsWindowEnabled(items) != FALSE) {
        return 876;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_LIGHT_TABLE_PIN,
            0) != 1) {
        return 877;
    }
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.light_table_pane);
    target_text.fill(L'\0');
    if (binding == nullptr
        || binding->policy != inkpod::app::PaneTargetPolicy::PinnedDocument
        || (GetMenuState(menu, IDM_LIGHT_TABLE_PIN, MF_BYCOMMAND) & MF_CHECKED) == 0U
        || GetDlgItemTextW(
               pane,
               IDC_LIGHT_TABLE_TARGET,
               target_text.data(),
               static_cast<int>(target_text.size())) <= 0
        || std::wcsstr(target_text.data(), L"固定") == nullptr) {
        return 878;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_LIGHT_TABLE_PIN,
            0) != 1
        || state.routing.pane_targets.Find(state.routing.light_table_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_WINDOW_LIGHT_TABLE,
               0) != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::LightTable)
        || (GetMenuState(menu, IDM_WINDOW_LIGHT_TABLE, MF_BYCOMMAND) & MF_CHECKED) != 0U) {
        return 879;
    }
    return 0;
}

int RunSubpalettePaneSmoke(ApplicationHost& state) noexcept {
    const HWND pane = state.Workspace().subpalette_palette;
    const HWND canvas = state.Workspace().subpalette_dialog.canvas;
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (pane == nullptr || canvas == nullptr || menu == nullptr
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Reference)
        || GetParent(pane) != state.Workspace().windows.window
        || GetParent(canvas) != pane
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_STYLE)) & WS_CHILD) == 0U
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_EXSTYLE))
            & (WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)) != 0U
        || !WindowHasAccessibleName(pane)) {
        return 920;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_SUBPALETTE,
            0) != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Reference)
        || GetDlgItem(pane, IDC_SUBPALETTE_TARGET) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_PIN) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_PREVIOUS) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_NEXT) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_CURRENT) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_FIT) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_ONE_TO_ONE) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_REGISTER) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_AUTO_PREVIOUS) == nullptr
        || GetDlgItem(pane, IDC_SUBPALETTE_SCROLL_SYNC) == nullptr
        || (GetMenuState(menu, IDM_WINDOW_SUBPALETTE, MF_BYCOMMAND)
            & MF_CHECKED) == 0U) {
        return 921;
    }
    std::array<wchar_t, 256U> target_text{};
    if (GetDlgItemTextW(
            pane,
            IDC_SUBPALETTE_TARGET,
            target_text.data(),
            static_cast<int>(target_text.size())) <= 0
        || std::wcsstr(target_text.data(), L"追従") == nullptr
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_SUBPALETTE_PIN,
               0) != 1) {
        return 922;
    }
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.subpalette_pane);
    if (binding == nullptr
        || binding->policy != inkpod::app::PaneTargetPolicy::PinnedDocument
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_SUBPALETTE_PIN,
               0) != 1
        || state.routing.pane_targets.Find(state.routing.subpalette_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_WINDOW_SUBPALETTE,
               0) != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Reference)) {
        return 923;
    }
    return 0;
}

int RunJobProgressPaneSmoke(ApplicationHost& state) noexcept {
    const HWND pane = state.Workspace().job_progress;
    const HWND window = state.Workspace().windows.window;
    const HMENU menu = GetMenu(window);
    if (pane == nullptr || window == nullptr || menu == nullptr
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::JobProgress)
        || GetParent(pane) != window
        || (static_cast<DWORD>(GetWindowLongPtrW(pane, GWL_STYLE)) & WS_CHILD)
            == 0U
        || GetDlgItem(pane, IDC_EFFECT_PROGRESS_TEXT) == nullptr
        || GetDlgItem(pane, IDC_EFFECT_PROGRESS_BAR) == nullptr
        || GetDlgItem(pane, IDC_EFFECT_PROGRESS_CANCEL) == nullptr
        || GetDlgItem(pane, IDC_BATCH_PROGRESS_TEXT) == nullptr
        || GetDlgItem(pane, IDC_BATCH_PROGRESS_BAR) == nullptr
        || GetDlgItem(pane, IDC_BATCH_PROGRESS_CANCEL) == nullptr
        || GetDlgItem(pane, IDC_JOB_PROGRESS_EMPTY) == nullptr) {
        return 926;
    }
    if (SendMessageW(window, WM_COMMAND, IDM_WINDOW_JOB_PROGRESS, 0) != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::JobProgress)
        || (GetMenuState(menu, IDM_WINDOW_JOB_PROGRESS, MF_BYCOMMAND)
            & MF_CHECKED) == 0U
        || !WindowHasVisibleStyle(GetDlgItem(pane, IDC_JOB_PROGRESS_EMPTY))
        || WindowHasVisibleStyle(GetDlgItem(pane, IDC_EFFECT_PROGRESS_CANCEL))
        || WindowHasVisibleStyle(GetDlgItem(pane, IDC_BATCH_PROGRESS_CANCEL))) {
        return 927;
    }
    if (SendMessageW(window, WM_COMMAND, IDM_WINDOW_JOB_PROGRESS, 0) != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::JobProgress)
        || (GetMenuState(menu, IDM_WINDOW_JOB_PROGRESS, MF_BYCOMMAND)
            & MF_CHECKED) != 0U) {
        return 928;
    }
    return 0;
}

bool MenuLeavesHaveAssignedShortcuts(
    HMENU menu,
    std::span<const InkpodShortcutSequence> bindings,
    std::size_t& leaf_count) noexcept {
    const int count = GetMenuItemCount(menu);
    for (int position = 0; position < count; ++position) {
        MENUITEMINFOW item{};
        item.cbSize = sizeof(item);
        item.fMask = MIIM_ID | MIIM_FTYPE | MIIM_SUBMENU;
        if (GetMenuItemInfoW(menu, static_cast<UINT>(position), TRUE, &item) == FALSE) {
            return false;
        }
        if (item.hSubMenu != nullptr) {
            if (!MenuLeavesHaveAssignedShortcuts(item.hSubMenu, bindings, leaf_count)) {
                return false;
            }
            continue;
        }
        if ((item.fType & MFT_SEPARATOR) != 0U) {
            continue;
        }
        ++leaf_count;
        if (windows::ui::FindShortcutSequence(bindings, item.wID) == nullptr) {
            return false;
        }
        const int length = GetMenuStringW(
            menu, static_cast<UINT>(position), nullptr, 0, MF_BYPOSITION);
        try {
            std::wstring label(static_cast<std::size_t>(length) + 1U, L'\0');
            GetMenuStringW(
                menu,
                static_cast<UINT>(position),
                label.data(),
                static_cast<int>(label.size()),
                MF_BYPOSITION);
            if (label.find(L'\t') == std::wstring::npos) {
                return false;
            }
        } catch (const std::bad_alloc&) {
            return false;
        }
    }
    return true;
}

bool ReadDocumentTabLabel(HWND tabs, int index, std::wstring& output) noexcept {
    if (tabs == nullptr || index < 0) {
        return false;
    }
    std::array<wchar_t, 1024U> buffer{};
    TCITEMW item{};
    item.mask = TCIF_TEXT;
    item.pszText = buffer.data();
    item.cchTextMax = static_cast<int>(buffer.size());
    if (TabCtrl_GetItem(tabs, index, &item) == FALSE) {
        return false;
    }
    try {
        output.assign(buffer.data());
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

bool DocumentTabsMatchRegistry(const ApplicationHost& state) noexcept {
    const HWND tabs = state.Workspace().windows.document_tabs;
    if (tabs == nullptr) {
        return false;
    }
    std::size_t expected_count{};
    for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
        const auto* document = state.Documents().SessionAt(index);
        if (document != nullptr) {
            expected_count += document->ViewCount();
        }
    }
    const int tab_count = TabCtrl_GetItemCount(tabs);
    if (tab_count < 0 || static_cast<std::size_t>(tab_count) != expected_count) {
        return false;
    }
    std::array<inkpod::app::DocumentViewId, 4096U> seen{};
    if (expected_count > seen.size()) {
        return false;
    }
    inkpod::app::DocumentViewId selected{};
    const int selected_index = TabCtrl_GetCurSel(tabs);
    for (int index = 0; index < tab_count; ++index) {
        TCITEMW item{};
        item.mask = TCIF_PARAM;
        if (TabCtrl_GetItem(tabs, index, &item) == FALSE) {
            return false;
        }
        const inkpod::app::DocumentViewId view{
            static_cast<std::uint64_t>(item.lParam)};
        if (!view || state.Documents().FindByView(view) == nullptr) {
            return false;
        }
        for (int prior = 0; prior < index; ++prior) {
            if (seen[static_cast<std::size_t>(prior)] == view) {
                return false;
            }
        }
        seen[static_cast<std::size_t>(index)] = view;
        if (index == selected_index) {
            selected = view;
        }
    }
    return selected == state.routing.targets.ActiveDocumentView();
}

bool SameCommandStates(
    const CommandStateSet& left,
    const CommandStateSet& right) noexcept {
    for (std::size_t index = 0U; index < left.size(); ++index) {
        if (left[index].command != right[index].command
            || left[index].owner != right[index].owner
            || left[index].enabled != right[index].enabled
            || left[index].checked != right[index].checked) {
            return false;
        }
    }
    return true;
}

struct ViewOptionsValidationProbe {
    std::uint32_t calls{};
    bool saw_expected_values{};
};

const wchar_t* RejectViewOptionsForSmoke(
    void* context,
    const std::array<std::int32_t, 4U>& values,
    std::uint32_t value_count) noexcept {
    auto* probe = static_cast<ViewOptionsValidationProbe*>(context);
    if (probe != nullptr) {
        ++probe->calls;
        probe->saw_expected_values = value_count == 2U
            && values[0] == INKPOD_TYPED_PLANE_RASTER
            && values[1] == INKPOD_STORAGE_RGBA8;
    }
    return L"smoke validation rejection";
}

int RunDrawingPersistenceSmoke(ApplicationHost& state) noexcept {
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_HELP_MANUAL, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_HELP_FILE_FORMAT, MF_BYCOMMAND)
            == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_HELP_WEB_PAGE, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_HELP_ABOUT, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_HELP_MANUAL, 0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_HELP_FILE_FORMAT,
               0)
            != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_HELP_WEB_PAGE,
               0)
            != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_HELP_ABOUT, 0) != 1) {
        return 29;
    }
    constexpr std::array<ViewOptionsDialogState::Choice, 2U> plane_kind_choices{{
        {L"主線", INKPOD_TYPED_PLANE_MAIN_LINE},
        {L"ラスター", INKPOD_TYPED_PLANE_RASTER},
    }};
    constexpr std::array<ViewOptionsDialogState::Choice, 2U> plane_format_choices{{
        {L"2値", INKPOD_STORAGE_BINARY8},
        {L"RGBA", INKPOD_STORAGE_RGBA8},
    }};
    ViewOptionsDialogState dropdown_dialog{};
    dropdown_dialog.title = L"共通ダイアログ smoke";
    dropdown_dialog.labels = {L"種類", L"形式", L"不透明度", nullptr};
    dropdown_dialog.values = {
        INKPOD_TYPED_PLANE_RASTER, INKPOD_STORAGE_RGBA8, 75, 0};
    dropdown_dialog.choices[0] = plane_kind_choices.data();
    dropdown_dialog.choice_counts[0] =
        static_cast<std::uint32_t>(plane_kind_choices.size());
    dropdown_dialog.choices[1] = plane_format_choices.data();
    dropdown_dialog.choice_counts[1] =
        static_cast<std::uint32_t>(plane_format_choices.size());
    dropdown_dialog.value_count = 3U;
    dropdown_dialog.close_immediately = true;
    if (ShowViewOptions(
            state.lifetime.instance,
            state.Workspace().windows.window,
            true,
            dropdown_dialog)
            != IDOK
        || dropdown_dialog.values[0] != INKPOD_TYPED_PLANE_RASTER
        || dropdown_dialog.values[1] != INKPOD_STORAGE_RGBA8
        || dropdown_dialog.values[2] != 75
        || !dropdown_dialog.centered_on_owner) {
        return 830;
    }
    ViewOptionsValidationProbe validation_probe{};
    ViewOptionsDialogState rejected_dialog = dropdown_dialog;
    rejected_dialog.value_count = 2U;
    rejected_dialog.validation_context = &validation_probe;
    rejected_dialog.validate = RejectViewOptionsForSmoke;
    if (ShowViewOptions(
            state.lifetime.instance,
            state.Workspace().windows.window,
            true,
            rejected_dialog)
            != IDCANCEL
        || validation_probe.calls != 1U
        || !validation_probe.saw_expected_values) {
        return 831;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WORKSPACE_RESET, 0)
            != 1
        || state.Workspace().tools.palette == nullptr
        || state.Workspace().windows.tool_options == nullptr
        || state.Workspace().windows.color_pane == nullptr
        || state.Workspace().panes.layer_palette == nullptr) {
        return 727;
    }
    const std::array<HWND, 4U> workspace_panes{
        state.Workspace().tools.palette,
        state.Workspace().windows.tool_options,
        state.Workspace().windows.color_pane,
        state.Workspace().panes.layer_palette};
    for (const HWND pane : workspace_panes) {
        const auto style = static_cast<DWORD>(
            GetWindowLongPtrW(pane, GWL_STYLE));
        if ((style & WS_CHILD) == 0U
            || GetParent(pane) != state.Workspace().windows.window) {
            return 728;
        }
    }
    if (ToolPaletteEntries().size() != kToolPaletteEntryCount
        || kToolPaletteEntryCount != 20U) {
        return 729;
    }
    if (!std::all_of(
            ToolPaletteEntries().begin(),
            ToolPaletteEntries().end(),
            [&](const ToolPaletteEntry& entry) {
                const HWND button = GetDlgItem(state.Workspace().tools.palette, entry.command);
                const auto glyph_length =
                    entry.glyph == nullptr ? 0U : std::wcslen(entry.glyph);
                return glyph_length >= 2U && glyph_length <= 8U
                    && std::wcschr(entry.glyph, L' ') == nullptr
                    && std::wcschr(entry.glyph, L'　') == nullptr
                    && button != nullptr
                    && (static_cast<DWORD>(
                            GetWindowLongPtrW(button, GWL_STYLE))
                            & BS_TYPEMASK)
                        == BS_OWNERDRAW;
            })) {
        return 746;
    }
    const HWND diameter_edit =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_DIAMETER);
    const HWND diameter_label =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_DIAMETER_LABEL);
    const HWND erase_target_label =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_TARGET_LABEL);
    const HWND erase_main_line =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_TARGET_MAIN_LINE);
    const HWND erase_color =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_TARGET_COLOR);
    const HWND brush_shape = GetDlgItem(
        state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_BRUSH_SHAPE);
    const HWND brush_smoothing = GetDlgItem(
        state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_BRUSH_SMOOTHING);
    const HWND brush_start_color = GetDlgItem(
        state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_BRUSH_START_COLOR);
    if (diameter_edit == nullptr || diameter_label == nullptr
        || erase_target_label == nullptr
        || erase_main_line == nullptr || erase_color == nullptr
        || brush_shape == nullptr || brush_smoothing == nullptr
        || brush_start_color == nullptr) {
        return 747;
    }
    std::array<wchar_t, 32U> brush_start_color_text{};
    if (GetWindowTextW(
            brush_start_color,
            brush_start_color_text.data(),
            static_cast<int>(brush_start_color_text.size()))
            <= 0
        || std::wcscmp(
               brush_start_color_text.data(),
               L"開始色の部分だけ塗る")
            != 0) {
        return 747;
    }
    const auto diameter_text_is = [&](const wchar_t* expected) noexcept {
        std::array<wchar_t, 32U> value{};
        return GetWindowTextW(
                   diameter_edit,
                   value.data(),
                   static_cast<int>(value.size()))
                > 0
            && std::wcscmp(value.data(), expected) == 0;
    };
    RECT diameter_label_bounds{};
    RECT diameter_edit_bounds{};
    const HFONT diameter_label_font = reinterpret_cast<HFONT>(
        SendMessageW(diameter_label, WM_GETFONT, 0, 0));
    const HFONT diameter_edit_font = reinterpret_cast<HFONT>(
        SendMessageW(diameter_edit, WM_GETFONT, 0, 0));
    LOGFONTW diameter_label_font_info{};
    LOGFONTW diameter_edit_font_info{};
    if (GetWindowRect(diameter_label, &diameter_label_bounds) == FALSE
        || GetWindowRect(diameter_edit, &diameter_edit_bounds) == FALSE
        || diameter_label_font == nullptr || diameter_edit_font == nullptr
        || GetObjectW(
               diameter_label_font,
               static_cast<int>(sizeof(diameter_label_font_info)),
               &diameter_label_font_info)
            != static_cast<int>(sizeof(diameter_label_font_info))
        || GetObjectW(
               diameter_edit_font,
               static_cast<int>(sizeof(diameter_edit_font_info)),
               &diameter_edit_font_info)
            != static_cast<int>(sizeof(diameter_edit_font_info))
        || diameter_edit_bounds.bottom - diameter_edit_bounds.top
            >= diameter_label_bounds.bottom - diameter_label_bounds.top
        || (diameter_edit_bounds.top + diameter_edit_bounds.bottom)
                - (diameter_label_bounds.top + diameter_label_bounds.bottom)
            < -1
        || (diameter_edit_bounds.top + diameter_edit_bounds.bottom)
                - (diameter_label_bounds.top + diameter_label_bounds.bottom)
            > 1
        || diameter_label_font_info.lfHeight >= 0
        || diameter_edit_font_info.lfHeight >= 0
        || diameter_edit_font_info.lfHeight
            <= diameter_label_font_info.lfHeight) {
        return 765;
    }
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_PENCIL
        || IsWindowEnabled(diameter_edit) != FALSE
        || !diameter_text_is(L"1.0")
        || IsWindowVisible(erase_target_label) != FALSE
        || IsWindowVisible(erase_main_line) != FALSE
        || IsWindowVisible(erase_color) != FALSE
        || IsWindowVisible(brush_shape) != FALSE
        || IsWindowVisible(brush_smoothing) != FALSE
        || IsWindowVisible(brush_start_color) != FALSE) {
        return 750;
    }
    const HWND main_line_label =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_MAIN_LINE_LABEL);
    const HWND main_line_swatch =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_MAIN_LINE_SWATCH);
    const HWND drawing_swatch =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_SWATCH);
    const HWND drawing_label =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_DRAWING_LABEL);
    const HWND color_picker =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_PICKER);
    const HWND color_eyedropper =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_EYEDROPPER);
    if (GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_TABS) == nullptr
        || GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_TARGET) == nullptr
        || GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_PIN) == nullptr
        || GetDlgItem(state.Workspace().windows.color_pane, IDC_PALETTE_LIST) == nullptr
        || GetDlgItem(
               state.Workspace().windows.color_pane,
               IDC_PALETTE_REGISTER_BUTTON) == nullptr
        || GetDlgItem(
               state.Workspace().windows.color_pane,
               IDC_PALETTE_DELETE_BUTTON) == nullptr
        || GetDlgItem(
               state.Workspace().windows.color_pane,
               IDC_PALETTE_CLEAR_BUTTON) == nullptr
        || GetDlgItem(
               state.Workspace().windows.color_pane,
               IDC_PALETTE_LOAD_BUTTON) == nullptr
        || GetDlgItem(
               state.Workspace().windows.color_pane,
               IDC_PALETTE_SAVE_BUTTON) == nullptr
        || main_line_label == nullptr || main_line_swatch == nullptr
        || drawing_swatch == nullptr
        || drawing_label == nullptr || color_picker == nullptr
        || color_eyedropper == nullptr
        || (GetWindowLongPtrW(color_picker, GWL_STYLE) & WS_VISIBLE) == 0
        || (GetWindowLongPtrW(color_eyedropper, GWL_STYLE) & WS_VISIBLE) == 0) {
        return 748;
    }
    RECT combined_swatch_bounds{};
    std::array<wchar_t, 32U> eyedropper_text{};
    GetWindowTextW(
        color_eyedropper,
        eyedropper_text.data(),
        static_cast<int>(eyedropper_text.size()));
    if (GetWindowRect(main_line_swatch, &combined_swatch_bounds) == FALSE
        || combined_swatch_bounds.right - combined_swatch_bounds.left
            <= combined_swatch_bounds.bottom - combined_swatch_bounds.top
        || (GetWindowLongPtrW(drawing_swatch, GWL_STYLE) & WS_VISIBLE) != 0
        || (GetWindowLongPtrW(main_line_label, GWL_STYLE) & SS_TYPEMASK)
            != SS_OWNERDRAW
        || (GetWindowLongPtrW(drawing_label, GWL_STYLE) & SS_TYPEMASK)
            != SS_OWNERDRAW
        || state.Workspace().panes.color_pane.change_main_line_color == nullptr
        || (GetWindowLongPtrW(color_eyedropper, GWL_STYLE) & BS_TYPEMASK)
            == BS_OWNERDRAW
        || std::wcscmp(eyedropper_text.data(), L"スポイト") != 0) {
        return 783;
    }
    const auto is_opaque_black = [](const InkpodColorValue& color) noexcept {
        return color.depth == INKPOD_COLOR_DEPTH_8 && color.red == 0U
            && color.green == 0U && color.blue == 0U && color.alpha == 255U;
    };
    const auto same_color = [](
                                const InkpodColorValue& left,
                                const InkpodColorValue& right) noexcept {
        return left.depth == right.depth && left.red == right.red
            && left.green == right.green && left.blue == right.blue
            && left.alpha == right.alpha;
    };
    std::array<wchar_t, 64U> main_line_text{};
    std::array<wchar_t, 64U> drawing_text{};
    std::array<wchar_t, 64U> picker_text{};
    GetWindowTextW(
        main_line_label,
        main_line_text.data(),
        static_cast<int>(main_line_text.size()));
    GetWindowTextW(
        drawing_label,
        drawing_text.data(),
        static_cast<int>(drawing_text.size()));
    GetWindowTextW(
        color_picker,
        picker_text.data(),
        static_cast<int>(picker_text.size()));
    if (!is_opaque_black(state.Workspace().tools.drawing_color)
        || state.Workspace().tools.color_rgba != UINT32_C(0x000000ff)
        || !is_opaque_black(state.Workspace().panes.main_line_color)
        || !is_opaque_black(state.Workspace().panes.color_pane.main_line_color)
        || std::wcsstr(main_line_text.data(), L"主線色") == nullptr
        || std::wcsstr(main_line_text.data(), L"#000000FF") == nullptr
        || std::wcsstr(drawing_text.data(), L"彩色用描画色") == nullptr
        || std::wcsstr(drawing_text.data(), L"#000000FF") == nullptr
        || std::wcsstr(picker_text.data(), L"色相") == nullptr
        || std::wcsstr(picker_text.data(), L"不透明度") == nullptr) {
        return 762;
    }
    RECT swatch_client{};
    if (GetClientRect(main_line_swatch, &swatch_client) == FALSE) {
        return 784;
    }
    SendMessageW(
        main_line_swatch,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(
            (swatch_client.right - swatch_client.left) * 29 / 100,
            (swatch_client.bottom - swatch_client.top) * 34 / 100));
    if (!state.Workspace().panes.color_pane.picker_targets_main_line) {
        return 785;
    }
    SendMessageW(
        main_line_swatch,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(
            (swatch_client.right - swatch_client.left) * 64 / 100,
            (swatch_client.bottom - swatch_client.top) * 62 / 100));
    if (state.Workspace().panes.color_pane.picker_targets_main_line) {
        return 787;
    }
    const InkpodColorValue picker_original_color = state.Workspace().tools.drawing_color;
    SendMessageW(color_picker, WM_KEYDOWN, VK_UP, 0);
    if (state.Workspace().tools.drawing_color.depth != INKPOD_COLOR_DEPTH_8
        || state.Workspace().tools.drawing_color.red == 0U
        || state.Workspace().tools.drawing_color.red != state.Workspace().tools.drawing_color.green
        || state.Workspace().tools.drawing_color.green != state.Workspace().tools.drawing_color.blue
        || state.Workspace().tools.drawing_color.alpha != UINT8_MAX) {
        return 766;
    }
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, picker_original_color);
    if (!is_opaque_black(state.Workspace().tools.drawing_color)) {
        return 767;
    }
    const InkpodColorValue picker_rgba16{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_16,
        UINT16_MAX,
        32768U,
        0U,
        32768U};
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, picker_rgba16);
    SendMessageW(color_picker, WM_KEYDOWN, VK_RIGHT, 0);
    if (state.Workspace().tools.drawing_color.depth != INKPOD_COLOR_DEPTH_16
        || state.Workspace().tools.drawing_color.alpha != picker_rgba16.alpha) {
        return 769;
    }
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, picker_original_color);
    SendMessageW(color_eyedropper, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool
        != inkpod::windows::ui::tools::kInteractionEyedropper) {
        return 768;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_PENCIL, 0);
    const InkpodColorValue alternate_main_line{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 17U, 34U, 51U, 255U};
    state.Workspace().panes.main_line_color = alternate_main_line;
    UpdateMenuState(state);
    GetWindowTextW(
        main_line_label,
        main_line_text.data(),
        static_cast<int>(main_line_text.size()));
    if (!same_color(state.Workspace().panes.main_line_color, alternate_main_line)
        || !same_color(state.Workspace().panes.color_pane.main_line_color, alternate_main_line)
        || std::wcsstr(main_line_text.data(), L"#112233FF") == nullptr) {
        return 763;
    }
    state.Workspace().panes.main_line_color = InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    UpdateMenuState(state);
    GetWindowTextW(
        main_line_label,
        main_line_text.data(),
        static_cast<int>(main_line_text.size()));
    if (!is_opaque_black(state.Workspace().panes.main_line_color)
        || !is_opaque_black(state.Workspace().panes.color_pane.main_line_color)
        || std::wcsstr(main_line_text.data(), L"#000000FF") == nullptr) {
        return 764;
    }
    const HWND layer_list =
        GetDlgItem(state.Workspace().panes.layer_palette, IDC_LAYER_LIST);
    const HWND plane_list =
        GetDlgItem(state.Workspace().panes.layer_palette, IDC_PLANE_LIST);
    const HWND layer_splitter = GetDlgItem(
        state.Workspace().panes.layer_palette, IDC_LAYER_PLANE_SPLITTER);
    const HWND action_target = GetDlgItem(
        state.Workspace().panes.layer_palette, IDC_LAYER_ACTION_TARGET);
    const HWND new_action =
        GetDlgItem(state.Workspace().panes.layer_palette, IDM_LAYER_NEW);
    std::array<wchar_t, 64U> action_target_text{};
    if (layer_list == nullptr || plane_list == nullptr
        || !IsCaptionlessAccessibleSplitter(layer_splitter)
        || action_target == nullptr || new_action == nullptr
        || GetWindowTextW(
               action_target,
               action_target_text.data(),
               static_cast<int>(action_target_text.size())) <= 0
        || std::wcsstr(action_target_text.data(), L"レイヤー") == nullptr
        || !AccessibleWindowNameContains(new_action, L"レイヤー")) {
        return 749;
    }
    SetFocus(plane_list);
    action_target_text.fill(L'\0');
    if (GetWindowTextW(
            action_target,
            action_target_text.data(),
            static_cast<int>(action_target_text.size())) <= 0
        || std::wcsstr(action_target_text.data(), L"プレーン") == nullptr
        || !AccessibleWindowNameContains(new_action, L"プレーン")) {
        return 985;
    }
    SetFocus(layer_list);
    const std::uint32_t layer_split_before =
        state.Workspace().panes.layer_palette_dialog.split_milli;
    SetFocus(layer_splitter);
    SendMessageW(layer_splitter, WM_KEYDOWN, VK_DOWN, 0);
    if (state.Workspace().panes.layer_palette_dialog.split_milli
        <= layer_split_before) {
        return 986;
    }
    SendMessageW(layer_splitter, WM_KEYDOWN, VK_UP, 0);
    SetFocus(layer_list);
    RECT layer_palette_client{};
    if (GetClientRect(
            state.Workspace().panes.layer_palette,
            &layer_palette_client) == FALSE) {
        return 1003;
    }
    const int split_drag_pixels = std::max(
        1,
        static_cast<int>(
            layer_palette_client.bottom - layer_palette_client.top) / 20);
    const LPARAM split_drag_start = MAKELPARAM(1, 1);
    const LPARAM split_drag_end = MAKELPARAM(1, 1 + split_drag_pixels);
    const std::uint32_t split_before_cancel =
        state.Workspace().panes.layer_palette_dialog.split_milli;
    const std::uint32_t persisted_before_cancel =
        state.Workspace().windows.workspace.layer_split_milli;
    SendMessageW(
        layer_splitter, WM_LBUTTONDOWN, MK_LBUTTON, split_drag_start);
    SendMessageW(layer_splitter, WM_MOUSEMOVE, MK_LBUTTON, split_drag_end);
    if (!state.Workspace().panes.layer_palette_dialog.split_dragging
        || state.Workspace().panes.layer_palette_dialog.split_milli
            == split_before_cancel
        || state.Workspace().windows.workspace.layer_split_milli
            != persisted_before_cancel) {
        return 1004;
    }
    SendMessageW(layer_splitter, WM_CANCELMODE, 0, 0);
    if (state.Workspace().panes.layer_palette_dialog.split_dragging
        || state.Workspace().panes.layer_palette_dialog.split_milli
            != split_before_cancel
        || state.Workspace().windows.workspace.layer_split_milli
            != persisted_before_cancel
        || GetCapture() == layer_splitter) {
        return 1005;
    }
    SendMessageW(
        layer_splitter, WM_LBUTTONDOWN, MK_LBUTTON, split_drag_start);
    SendMessageW(layer_splitter, WM_MOUSEMOVE, MK_LBUTTON, split_drag_end);
    SetCapture(state.Workspace().windows.window);
    const std::uint32_t split_after_capture_loss =
        state.Workspace().panes.layer_palette_dialog.split_milli;
    const bool capture_loss_committed =
        !state.Workspace().panes.layer_palette_dialog.split_dragging
        && split_after_capture_loss != split_before_cancel
        && state.Workspace().windows.workspace.layer_split_milli
            == split_after_capture_loss;
    ReleaseCapture();
    if (!capture_loss_committed) {
        return 1006;
    }
    SetWindowPos(
        state.Workspace().windows.window,
        nullptr,
        0,
        0,
        1'800,
        1'000,
        SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW);
    RECT client{};
    if (GetClientRect(state.Workspace().windows.window, &client) == FALSE) {
        return 730;
    }
    LayoutMainChrome(
        state.Workspace().windows,
        false,
        client.right - client.left,
        client.bottom - client.top);
    RECT color_pane_bounds{};
    if (GetWindowRect(state.Workspace().windows.color_pane, &color_pane_bounds) == FALSE) {
        return 840;
    }
    const int color_pane_width = color_pane_bounds.right - color_pane_bounds.left;
    const int color_pane_height = color_pane_bounds.bottom - color_pane_bounds.top;
    SetWindowPos(
        state.Workspace().windows.color_pane,
        nullptr,
        0,
        0,
        std::max(color_pane_width, 420),
        std::max(color_pane_height, 360),
        SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER);
    RECT color_pane_client{};
    ValidateRect(color_picker, nullptr);
    if (GetClientRect(state.Workspace().windows.color_pane, &color_pane_client) == FALSE) {
        return 840;
    }
    SendMessageW(
        state.Workspace().windows.color_pane,
        WM_SIZE,
        SIZE_RESTORED,
        MAKELPARAM(
            color_pane_client.right - color_pane_client.left,
            color_pane_client.bottom - color_pane_client.top));
    RECT color_picker_client{};
    RECT color_picker_update{};
    if (GetClientRect(color_picker, &color_picker_client) == FALSE
        || IsRectEmpty(&color_picker_client) != FALSE) {
        return 841;
    }
    if (GetUpdateRect(color_picker, &color_picker_update, FALSE) == FALSE) {
        return 842;
    }
    // A child window's update region can be clipped or coalesced by Windows
    // according to its parent and siblings, so its bounding rectangle is not
    // required to match the full client rectangle. Any pending picker paint
    // redraws the complete current-size surface; verify that path directly.
    if (UpdateWindow(color_picker) == FALSE
        || GetUpdateRect(color_picker, &color_picker_update, FALSE) != FALSE) {
        return 843;
    }
    HDC picker_dc = GetDC(color_picker);
    if (picker_dc == nullptr) {
        return 844;
    }
    const bool suppresses_background_erase = SendMessageW(
        color_picker,
        WM_ERASEBKGND,
        reinterpret_cast<WPARAM>(picker_dc),
        0) != FALSE;
    const bool prepared = state.Workspace()
        .panes.color_pane.picker_paint_buffer.Prepare(picker_dc, 16, 16);
    ReleaseDC(color_picker, picker_dc);
    if (!suppresses_background_erase) {
        return 845;
    }
    if (!prepared) {
        return 846;
    }
    if (!state.Workspace().panes.color_pane.picker_paint_buffer.ReadyFor(16, 16)) {
        return 847;
    }
    SetWindowPos(
        state.Workspace().windows.color_pane,
        nullptr,
        0,
        0,
        color_pane_width,
        color_pane_height,
        SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER);
    UpdateMenuState(state);
    const auto checked = [&](UINT command) {
        return (GetMenuState(menu, command, MF_BYCOMMAND) & MF_CHECKED) != 0U;
    };
    if (!state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool)
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::ToolOptions)
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Color)
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Layer)
        || state.Workspace().windows.workspace.dock.Mirrored()
        || !checked(IDM_WINDOW_TOOL_PALETTE)
        || !checked(IDM_WINDOW_TOOL_OPTIONS)
        || !checked(IDM_WINDOW_COLOR_PANE)
        || !checked(IDM_WINDOW_LAYER_PALETTE)) {
        return 731;
    }
    RECT tool_bounds{};
    RECT workspace_canvas_bounds{};
    RECT color_bounds{};
    RECT layer_bounds{};
    if (GetWindowRect(state.Workspace().tools.palette, &tool_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.color_pane, &color_bounds) == FALSE
        || GetWindowRect(state.Workspace().panes.layer_palette, &layer_bounds) == FALSE) {
        return 732;
    }
    if (tool_bounds.right > workspace_canvas_bounds.left
        || workspace_canvas_bounds.right > color_bounds.left
        || color_bounds.left != layer_bounds.left
        || color_bounds.right != layer_bounds.right) {
        return 732;
    }
    for (const DockZone zone : {DockZone::Left, DockZone::Right}) {
        if (!IsCaptionlessAccessibleSplitter(
                state.Workspace().windows.dock_host.SplitterWindow(
                    zone, DockSplitterKind::ZoneExtent))) {
            return 838;
        }
    }
    const HWND brush_button = GetDlgItem(state.Workspace().tools.palette, IDM_TOOL_BRUSH);
    const HWND pencil_button = GetDlgItem(state.Workspace().tools.palette, IDM_TOOL_PENCIL);
    const HWND eraser_button = GetDlgItem(state.Workspace().tools.palette, IDM_TOOL_ERASER);
    SendMessageW(brush_button, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_BRUSH) {
        return 733;
    }
    if (IsWindowEnabled(diameter_edit) == FALSE
        || !diameter_text_is(L"8.0")
        || IsWindowVisible(brush_shape) == FALSE
        || IsWindowVisible(brush_smoothing) == FALSE
        || IsWindowVisible(brush_start_color) == FALSE
        || SendMessageW(brush_shape, CB_GETCURSEL, 0, 0) != 0
        || SendMessageW(brush_start_color, BM_GETCHECK, 0, 0)
            != BST_UNCHECKED) {
        return 751;
    }
    SendMessageW(brush_shape, CB_SETCURSEL, 1, 0);
    SendMessageW(
        state.Workspace().windows.tool_options,
        WM_COMMAND,
        MAKEWPARAM(IDC_TOOL_OPTIONS_BRUSH_SHAPE, CBN_SELCHANGE),
        reinterpret_cast<LPARAM>(brush_shape));
    SetFocus(brush_smoothing);
    SetWindowTextW(brush_smoothing, L"700");
    SetFocus(state.Workspace().windows.canvas);
    SendMessageW(brush_start_color, BM_CLICK, 0, 0);
    InkpodEditorStateInfo brush_editor{};
    brush_editor.struct_size = sizeof(brush_editor);
    if (!state.engine->GetEditorState(
            state.Document().id, state.Document().generation, brush_editor)
        || brush_editor.brush.shape != INKPOD_BRUSH_SQUARE
        || brush_editor.brush.smoothing != 700U
        || brush_editor.brush.start_color != INKPOD_START_COLOR_EXACT_NATIVE
        || state.Workspace().tools.brush.shape != INKPOD_BRUSH_SQUARE
        || state.Workspace().tools.brush.smoothing != 700U
        || state.Workspace().tools.brush.start_color
            != INKPOD_START_COLOR_EXACT_NATIVE) {
        return 11001;
    }
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"20.0");
    UpdateMenuState(state);
    if (GetFocus() != diameter_edit || state.Workspace().tools.diameter != 8.0F
        || !diameter_text_is(L"20.0")) {
        return 758;
    }
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(0, 0));
    if (GetFocus() != state.Workspace().windows.canvas || state.Workspace().tools.diameter != 20.0F
        || !diameter_text_is(L"20.0")) {
        return 759;
    }
    ReleaseCapture();
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"256.0");
    SetFocus(state.Workspace().windows.canvas);
    if (state.Workspace().tools.diameter != panes::kMaximumToolDiameter
        || !diameter_text_is(L"256.0")) {
        return 752;
    }
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"256.1");
    SetFocus(state.Workspace().windows.canvas);
    if (state.Workspace().tools.diameter != panes::kMaximumToolDiameter
        || !diameter_text_is(L"256.0")) {
        return 753;
    }
    SendMessageW(eraser_button, BM_CLICK, 0, 0);
    InkpodEditorStateInfo eraser_editor{};
    eraser_editor.struct_size = sizeof(eraser_editor);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_ERASER
        || !state.engine->GetEditorState(
            state.Document().id, state.Document().generation, eraser_editor)
        || eraser_editor.active_tool != INKPOD_TOOL_ERASER
        || state.Workspace().tools.diameter
            != static_cast<float>(
                static_cast<double>(eraser_editor.current_diameter_q16)
                / 65536.0)
        || IsWindowEnabled(diameter_edit) == FALSE
        || IsWindowVisible(erase_target_label) == FALSE
        || IsWindowVisible(erase_main_line) == FALSE
        || IsWindowVisible(erase_color) == FALSE
        || SendMessageW(erase_main_line, BM_GETCHECK, 0, 0) != BST_CHECKED
        || SendMessageW(erase_color, BM_GETCHECK, 0, 0) != BST_UNCHECKED) {
        return 754;
    }
    SendMessageW(erase_color, BM_CLICK, 0, 0);
    InkpodDocumentInfo erase_target_info = EmptyDocumentInfo();
    InkpodEditorStateInfo erase_target_editor{};
    erase_target_editor.struct_size = sizeof(erase_target_editor);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_COLOR
        || SendMessageW(erase_main_line, BM_GETCHECK, 0, 0) != BST_UNCHECKED
        || SendMessageW(erase_color, BM_GETCHECK, 0, 0) != BST_CHECKED
        || !QueryDocument(state, erase_target_info)
        || !state.engine->GetEditorState(
            state.Document().id,
            state.Document().generation,
            erase_target_editor)
        || erase_target_editor.active_layer_id != erase_target_info.layer_id
        || erase_target_editor.active_plane_id != erase_target_info.color_plane_id
        || state.Workspace().panes.active_tree_layer_id != erase_target_info.layer_id
        || state.Workspace().panes.active_tree_plane_id != erase_target_info.color_plane_id) {
        return 760;
    }
    SendMessageW(erase_main_line, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE
        || SendMessageW(erase_main_line, BM_GETCHECK, 0, 0) != BST_CHECKED
        || SendMessageW(erase_color, BM_GETCHECK, 0, 0) != BST_UNCHECKED
        || !QueryDocument(state, erase_target_info)
        || !state.engine->GetEditorState(
            state.Document().id,
            state.Document().generation,
            erase_target_editor)
        || erase_target_editor.active_layer_id != erase_target_info.layer_id
        || erase_target_editor.active_plane_id != erase_target_info.main_plane_id
        || state.Workspace().panes.active_tree_layer_id != erase_target_info.layer_id
        || state.Workspace().panes.active_tree_plane_id != erase_target_info.main_plane_id) {
        return 761;
    }
    if (!ToolPaletteMatchesCommandState(
            state.Workspace().tools.palette, state.Workspace().command_states)) {
        return 896;
    }
    const auto* pair_extract_state = windows::ui::FindCommandState(
        state.Workspace().command_states, IDM_BATCH_EXTRACT_PAIRS);
    const UINT pair_extract_menu_state = GetMenuState(
        GetMenu(state.Workspace().windows.window),
        IDM_BATCH_EXTRACT_PAIRS,
        MF_BYCOMMAND);
    if (pair_extract_state == nullptr) {
        return 937;
    }
    if (pair_extract_menu_state == static_cast<UINT>(-1)) {
        return 938;
    }
    if (((pair_extract_menu_state & (MF_DISABLED | MF_GRAYED)) == 0U)
        != pair_extract_state->enabled) {
        return 939;
    }
    if (((pair_extract_menu_state & MF_CHECKED) != 0U)
        != pair_extract_state->checked) {
        return 940;
    }
    if (!CommandSurfacesMatchComputedState(state)) {
        return 929;
    }
    SendMessageW(pencil_button, BM_CLICK, 0, 0);
    InkpodEditorStateInfo pencil_editor{};
    pencil_editor.struct_size = sizeof(pencil_editor);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_PENCIL
        || !state.engine->GetEditorState(
            state.Document().id, state.Document().generation, pencil_editor)
        || pencil_editor.active_tool != INKPOD_TOOL_PENCIL
        || state.Workspace().tools.diameter
            != static_cast<float>(
                static_cast<double>(pencil_editor.current_diameter_q16)
                / 65536.0)
        || IsWindowEnabled(diameter_edit) != FALSE
        || !diameter_text_is(L"1.0")) {
        return 755;
    }
    SendMessageW(brush_button, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_BRUSH
        || IsWindowEnabled(diameter_edit) == FALSE
        || !diameter_text_is(L"256.0")) {
        return 756;
    }
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"8.0");
    SetFocus(state.Workspace().windows.canvas);
    if (state.Workspace().tools.diameter != 8.0F || !diameter_text_is(L"8.0")) {
        return 757;
    }
    SendMessageW(brush_shape, CB_SETCURSEL, 0, 0);
    SendMessageW(
        state.Workspace().windows.tool_options,
        WM_COMMAND,
        MAKEWPARAM(IDC_TOOL_OPTIONS_BRUSH_SHAPE, CBN_SELCHANGE),
        reinterpret_cast<LPARAM>(brush_shape));
    SetFocus(brush_smoothing);
    SetWindowTextW(brush_smoothing, L"0");
    SetFocus(state.Workspace().windows.canvas);
    if (SendMessageW(brush_start_color, BM_GETCHECK, 0, 0) == BST_CHECKED) {
        SendMessageW(brush_start_color, BM_CLICK, 0, 0);
    }
    InkpodEditorStateInfo restored_brush{};
    restored_brush.struct_size = sizeof(restored_brush);
    if (!state.engine->GetEditorState(
            state.Document().id, state.Document().generation, restored_brush)
        || restored_brush.brush.shape != INKPOD_BRUSH_ROUND
        || restored_brush.brush.smoothing != 0U
        || restored_brush.brush.start_color != INKPOD_START_COLOR_ANY) {
        return 11002;
    }
    SendMessageW(pencil_button, BM_CLICK, 0, 0);
    const LONG initial_canvas_width =
        workspace_canvas_bounds.right - workspace_canvas_bounds.left;
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_PALETTE, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool)
        || checked(IDM_WINDOW_TOOL_PALETTE)
        || IsWindowVisible(state.Workspace().tools.palette) != FALSE
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || workspace_canvas_bounds.right - workspace_canvas_bounds.left
            <= initial_canvas_width) {
        return 734;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_PALETTE, 0)
            != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool)
        || !checked(IDM_WINDOW_TOOL_PALETTE)) {
        return 735;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_OPTIONS, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::ToolOptions)
        || checked(IDM_WINDOW_TOOL_OPTIONS)
        || SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_OPTIONS, 0)
            != 1) {
        return 736;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_COLOR_PANE, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Color)
        || SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_LAYER_PALETTE, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Layer)
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || workspace_canvas_bounds.right - workspace_canvas_bounds.left
            <= initial_canvas_width) {
        return 737;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_COLOR_PANE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_LAYER_PALETTE, 0);
    const std::array<DockPaneType, 4U> dock_pane_types{
        DockPaneType::Tool,
        DockPaneType::ToolOptions,
        DockPaneType::Color,
        DockPaneType::Layer};
    for (const DockPaneType type : dock_pane_types) {
        HWND content = state.Workspace().windows.dock_host.ContentWindow(type);
        if (content == nullptr
            || state.Workspace().windows.dock_host.FloatPane(type)
                != DockResult::Ok) {
            return 832;
        }
        const HWND floating =
            state.Workspace().windows.dock_host.FloatingWindow(type);
        wchar_t floating_title[128]{};
        const auto extended_style = floating == nullptr
            ? DWORD_PTR{}
            : static_cast<DWORD_PTR>(GetWindowLongPtrW(floating, GWL_EXSTYLE));
        if (floating == nullptr || GetParent(content) != floating
            || GetWindowTextW(
                   floating,
                   floating_title,
                   static_cast<int>(std::size(floating_title)))
                <= 0
            || GetWindow(floating, GW_OWNER) != state.Workspace().windows.window
            || IsWindowVisible(floating) == FALSE
            || GetClassLongPtrW(floating, GCLP_HBRBACKGROUND)
                != reinterpret_cast<ULONG_PTR>(GetSysColorBrush(COLOR_BTNFACE))
            || (extended_style & (WS_EX_TOPMOST | WS_EX_NOACTIVATE)) != 0U
            || (extended_style & WS_EX_PALETTEWINDOW)
                == WS_EX_PALETTEWINDOW
            || state.Workspace().windows.dock_host.HidePane(type)
                != DockResult::Ok
            || IsWindowVisible(floating) != FALSE
            || state.Workspace().windows.dock_host.RestorePane(type)
                != DockResult::Ok
            || GetParent(content) != state.Workspace().windows.window
            || !state.Workspace().windows.workspace.dock.IsPaneDocked(type)) {
            return 833;
        }
        SendMessageW(floating, WM_THEMECHANGED, 0, 0);
        SendMessageW(floating, WM_SETTINGCHANGE, 0, 0);
        SendMessageW(floating, WM_CANCELMODE, 0, 0);
        if (state.Workspace().windows.dock_host.PreviewVisible()) {
            return 835;
        }
    }
    if (state.Workspace().windows.dock_host.DockPane(
            DockPaneType::Color, DockZone::Left)
            != DockResult::Ok
        || state.Workspace().windows.workspace.dock.PaneCount(DockZone::Left)
            != 2U
        || state.Workspace().windows.dock_host.SetZoneMode(
               DockZone::Left, DockStackMode::Tabs)
            != DockResult::Ok) {
        return 834;
    }
    HWND dock_tabs =
        state.Workspace().windows.dock_host.TabWindow(DockZone::Left);
    const DockZoneState* left_zone =
        state.Workspace().windows.workspace.dock.Zone(DockZone::Left);
    if (dock_tabs == nullptr || left_zone == nullptr
        || TabCtrl_GetItemCount(dock_tabs) != 2
        || (GetWindowLongPtrW(dock_tabs, GWL_STYLE) & WS_TABSTOP) == 0
        || left_zone->active_tab != DockPaneType::Tool) {
        return 836;
    }
    SetFocus(dock_tabs);
    SendMessageW(dock_tabs, WM_KEYDOWN, VK_RIGHT, 0);
    SendMessageW(dock_tabs, WM_KEYUP, VK_RIGHT, 0);
    if (state.Workspace().windows.workspace.dock.Zone(DockZone::Left)
            ->active_tab
        != DockPaneType::Color
        || state.Workspace().windows.dock_host.SetZoneMode(
               DockZone::Left, DockStackMode::Split)
            != DockResult::Ok) {
        return 837;
    }
    HWND dock_splitter = state.Workspace().windows.dock_host.SplitterWindow(
        DockZone::Left, DockSplitterKind::StackBoundary);
    const std::uint32_t tool_weight_before =
        state.Workspace().windows.workspace.dock.Pane(DockPaneType::Tool)
            ->split_weight;
    if (!IsCaptionlessAccessibleSplitter(dock_splitter)) {
        return 838;
    }
    SetFocus(dock_splitter);
    SendMessageW(dock_splitter, WM_KEYDOWN, VK_DOWN, 0);
    if (state.Workspace().windows.workspace.dock.Pane(DockPaneType::Tool)
            ->split_weight
        <= tool_weight_before
        || state.Workspace().windows.dock_host.ResetPane(DockPaneType::Color)
            != DockResult::Ok
        || state.Workspace().windows.workspace.dock.Pane(DockPaneType::Color)
                ->zone
            != DockZone::Right) {
        return 839;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WORKSPACE_MIRROR, 0)
            != 1
        || !state.Workspace().windows.workspace.dock.Mirrored()
        || !checked(IDM_WORKSPACE_MIRROR)
        || GetWindowRect(state.Workspace().tools.palette, &tool_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.color_pane, &color_bounds) == FALSE
        || color_bounds.right > workspace_canvas_bounds.left
        || workspace_canvas_bounds.right > tool_bounds.left) {
        return 738;
    }
    if (LayerPaletteItemCount(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.tree_layer_count
        || LayerPalettePlaneCount(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.tree_plane_count
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.active_tree_layer_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.active_tree_plane_id
        || !LayerPaletteMatchesCommandState(
            state.Workspace().panes.layer_palette, state.Workspace().command_states)) {
        return 739;
    }
    InkpodDocumentInfo before_workspace_preset{};
    InkpodDocumentInfo after_workspace_preset{};
    if (!QueryDocument(state, before_workspace_preset)
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_WORKSPACE_PRESET_REFERENCE,
               0)
            != 1
        || state.Workspace().windows.workspace.selected_preset
            != inkpod::windows::ui::WorkspacePreset::ReferenceCheck
        || !checked(IDM_WORKSPACE_PRESET_REFERENCE)) {
        return 944;
    }
    const std::size_t locator_index = static_cast<std::size_t>(
        inkpod::windows::ui::WorkspaceAuxiliaryPane::Locator);
    const HWND locator_edge =
        state.Workspace().windows.auto_hide_buttons[locator_index];
    if (locator_edge == nullptr || IsWindowVisible(locator_edge) == FALSE
        || (GetWindowLongPtrW(locator_edge, GWL_STYLE) & WS_TABSTOP) == 0
        || !WindowHasAccessibleName(locator_edge)) {
        return 945;
    }
    SendMessageW(locator_edge, BM_CLICK, 0, 0);
    if (!WindowHasVisibleStyle(state.Workspace().locator_palette)) {
        return 945;
    }
    SendMessageW(
        state.Workspace().windows.window,
        WM_ACTIVATE,
        WA_ACTIVE,
        0);
    if (WindowHasVisibleStyle(state.Workspace().locator_palette)
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_WORKSPACE_AUTOHIDE_LOCATOR,
               0)
            != 1
        || IsWindowVisible(locator_edge) != FALSE
        || checked(IDM_WORKSPACE_AUTOHIDE_LOCATOR)) {
        return 946;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WORKSPACE_PRESET_FOCUS,
            0)
            != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_WORKSPACE_PRESET_COLORING,
               0)
            != 1
        || !QueryDocument(state, after_workspace_preset)
        || after_workspace_preset.document_revision
            != before_workspace_preset.document_revision
        || after_workspace_preset.flags != before_workspace_preset.flags) {
        return 947;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WORKSPACE_RESET, 0);
    ShowWindow(state.Workspace().windows.window, SW_HIDE);

    if (state.engine == nullptr
        || MoveWindow(state.Workspace().windows.canvas, 0, 0, 640, 480, FALSE) == FALSE
        || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 30;
    }
    PumpPendingWindowMessages();
    UpdateMenuState(state);
    std::wstring tab_label;
    if (!ReadDocumentTabLabel(state.Workspace().windows.document_tabs, 0, tab_label)
        || tab_label != L"無題セル 1") {
        return 716;
    }
    std::size_t shortcut_leaf_count{};
    if (!CommandSurfacesMatchComputedState(state)
        || !MenuLeavesHaveAssignedShortcuts(
            menu, state.shortcuts.bindings, shortcut_leaf_count)
        || shortcut_leaf_count < windows::ui::MenuCommandCatalog().size()
        || FindWindowExW(state.Workspace().windows.window, nullptr, TOOLBARCLASSNAMEW, nullptr) != nullptr
        || SendMessageW(state.Workspace().windows.status_bar, SB_GETPARTS, 0, 0) != 6) {
        return 706;
    }
    if (DispatchEnabledCommand(state, state.Workspace().windows.window, IDM_EDIT_UNDO)) {
        return 714;
    }
    constexpr std::array<UINT, 6U> geometry_draw_commands{
        IDM_VECTOR_LINE,
        IDM_VECTOR_CURVE,
        IDM_VECTOR_RECTANGLE,
        IDM_VECTOR_ELLIPSE,
        IDM_VECTOR_POLYLINE,
        IDM_VECTOR_POLYGON};
    for (const UINT command : geometry_draw_commands) {
        const UINT command_state = GetMenuState(menu, command, MF_BYCOMMAND);
        if (command_state == static_cast<UINT>(-1)
            || (command_state & (MF_DISABLED | MF_GRAYED)) != 0U) {
            return 701;
        }
    }
    const UINT eraser_state = GetMenuState(menu, IDM_VECTOR_ERASER, MF_BYCOMMAND);
    if (eraser_state == static_cast<UINT>(-1)
        || (eraser_state & (MF_DISABLED | MF_GRAYED)) == 0U) {
        return 701;
    }
    if (!RefreshSequencePane(state) || state.Workspace().panes.sequence_count != 0U) {
        return 702;
    }
    InkpodSequenceCellInfo missing_sequence_cell{};
    missing_sequence_cell.struct_size = sizeof(missing_sequence_cell);
    const InkpodStatus missing_sequence_status = state.engine->Invoke(
        [&missing_sequence_cell](InkpodCore* core) {
            return inkpod_core_sequence_cell_get(core, 0U, &missing_sequence_cell);
        },
        false,
        false);
    if (missing_sequence_status != INKPOD_STATUS_INVALID_STATE
        || state.engine->LastError().find(L"no sequence is configured")
            == std::wstring::npos) {
        return 703;
    }
    const std::uint32_t initial_tool = state.Workspace().tools.active_tool;
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_ERASER, 0)
            != 0
        || state.Workspace().tools.active_tool != initial_tool) {
        return 705;
    }
    const std::wstring initial_recovery_path = state.Document().shell.recovery_path;
    RecoveryMetadata initial_recovery_metadata{};
    InkpodDocumentInfo initial_recovery_info = EmptyDocumentInfo();
    std::vector<RecoveryCandidate> recovery_candidates;
    if (initial_recovery_path.empty()
        || !QueueAutosave(
            state, state.routing.targets.Capture(), initial_recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(initial_recovery_path.c_str())
            == INVALID_FILE_ATTRIBUTES
        || !ReadRecoveryMetadata(
            initial_recovery_path, initial_recovery_metadata)
        || !QueryDocument(state, initial_recovery_info)
        || initial_recovery_metadata.session != state.Document().id
        || initial_recovery_metadata.generation != state.Document().generation
        || initial_recovery_metadata.document_uuid_high
            != initial_recovery_info.document_uuid_high
        || initial_recovery_metadata.document_uuid_low
            != initial_recovery_info.document_uuid_low
        || !EnumerateRecoveryCandidates(recovery_candidates)
        || std::none_of(
            recovery_candidates.begin(),
            recovery_candidates.end(),
            [&initial_recovery_path](const RecoveryCandidate& candidate) {
                return _wcsicmp(
                    candidate.recovery_path.c_str(),
                    initial_recovery_path.c_str()) == 0;
            })) {
        return 215;
    }
    std::wstring active_stroke_recovery_path;
    try {
        active_stroke_recovery_path = initial_recovery_path + L".active-stroke-test";
    } catch (const std::bad_alloc&) {
        return 217;
    }
    if (!DiscardRecoveryArtifact(active_stroke_recovery_path)) {
        return 218;
    }
    PumpPendingWindowMessages();
    const DWORD ui_thread = GetCurrentThreadId();
    const DWORD core_thread = state.engine->ThreadId();
    const DWORD renderer_thread = static_cast<DWORD>(SendMessageW(
        state.Workspace().windows.canvas, inkpod::renderer::kCanvasGetRendererThreadId, 0, 0));
    if (core_thread == 0U || renderer_thread == 0U || core_thread == ui_thread
        || renderer_thread == ui_thread || core_thread == renderer_thread) {
        return 31;
    }
    inkpod::renderer::CanvasDocumentBounds document_bounds{};
    RECT canvas_client{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, document_bounds)
        || GetClientRect(state.Workspace().windows.canvas, &canvas_client) == FALSE
        || initial_recovery_info.width == 0U || initial_recovery_info.height == 0U) {
        return 53;
    }
    const double client_width = static_cast<double>(canvas_client.right);
    const double client_height = static_cast<double>(canvas_client.bottom);
    const double fit_scale = std::min(
        client_width / static_cast<double>(initial_recovery_info.width),
        client_height / static_cast<double>(initial_recovery_info.height))
        * 0.95;
    const double expected_width =
        static_cast<double>(initial_recovery_info.width) * fit_scale;
    const double expected_height =
        static_cast<double>(initial_recovery_info.height) * fit_scale;
    const double expected_left = (client_width - expected_width) / 2.0;
    const double expected_top = (client_height - expected_height) / 2.0;
    if (!std::isfinite(fit_scale) || fit_scale <= 0.0
        || std::abs(document_bounds.left - expected_left) > 0.01
        || std::abs(document_bounds.top - expected_top) > 0.01
        || std::abs(document_bounds.right - (expected_left + expected_width)) > 0.01
        || std::abs(document_bounds.bottom - (expected_top + expected_height)) > 0.01) {
        return 53;
    }

    InkpodDocumentInfo before_line{};
    std::array<wchar_t, 128> initial_title{};
    if (!QueryDocument(state, before_line)
        || (before_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || GetWindowTextW(
               state.Workspace().windows.window, initial_title.data(), static_cast<int>(initial_title.size())) == 0
        || std::wcscmp(initial_title.data(), L"無題セル 1 - inkpod") != 0) {
        return 32;
    }
    const auto frames_before = static_cast<std::uint64_t>(SendMessageW(
        state.Workspace().windows.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 100));
    for (int x = 90; x <= 240; x += 15) {
        SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 120));
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WORKSPACE_PRESET_FOCUS,
            0)
        != 1) {
        return 948;
    }
    if (GetCapture() != state.Workspace().windows.canvas) {
        return 949;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WORKSPACE_PRESET_COLORING,
            0)
        != 1) {
        return 950;
    }
    if (GetCapture() != state.Workspace().windows.canvas) {
        return 951;
    }
    if (state.engine->FlushPreview() != INKPOD_STATUS_OK) {
        return 33;
    }
    if (!QueueAutosave(
            state,
            state.routing.targets.Capture(),
            active_stroke_recovery_path)) {
        return 219;
    }
    PumpPendingWindowMessages();
    if (SendMessageW(state.Workspace().windows.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 34;
    }
    InkpodDocumentInfo during_line{};
    const auto frames_during = static_cast<std::uint64_t>(SendMessageW(
        state.Workspace().windows.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    if (!QueryDocument(state, during_line)) {
        return 130;
    }
    if (during_line.document_revision != before_line.document_revision) {
        return 131;
    }
    if (during_line.main_plane_checksum != before_line.main_plane_checksum) {
        return 132;
    }
    if ((during_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)
        != (before_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)) {
        return 133;
    }
    if (frames_during <= frames_before) {
        return 134;
    }
    if (SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(250, 120)) != 1) {
        return 36;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 37;
    }
    if (GetFileAttributesW(active_stroke_recovery_path.c_str())
        == INVALID_FILE_ATTRIBUTES) {
        return 220;
    }
    if (!DiscardRecoveryArtifact(active_stroke_recovery_path)) {
        return 221;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_line{};
    if (!QueryDocument(state, after_line)
        || after_line.document_revision != before_line.document_revision + 1U
        || after_line.main_plane_checksum == before_line.main_plane_checksum
        || (after_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || (after_line.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U
        || !ReadDocumentTabLabel(state.Workspace().windows.document_tabs, 0, tab_label)
        || tab_label != L"無題セル 1 *") {
        return 38;
    }
    const std::uint64_t line_checksum = after_line.main_plane_checksum;

    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 150));
    SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(180, 150));
    SendMessageW(state.Workspace().windows.canvas, WM_CAPTURECHANGED, 0, 0);
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 54;
    }
    InkpodDocumentInfo after_cancel{};
    if (!QueryDocument(state, after_cancel)
        || after_cancel.document_revision != after_line.document_revision
        || after_cancel.main_plane_checksum != after_line.main_plane_checksum) {
        return 55;
    }

    if (SetEditorActiveTarget(
            state, after_line.layer_id, after_line.color_plane_id)
        != INKPOD_STATUS_OK) {
        return 39;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(100, 180));
    for (int x = 115; x <= 260; x += 15) {
        SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 190));
    }
    if (SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(270, 190)) != 1) {
        return 40;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 41;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_color{};
    if (!QueryDocument(state, after_color)
        || after_color.main_plane_checksum != line_checksum
        || after_color.color_plane_checksum == after_line.color_plane_checksum) {
        return 42;
    }
    const inkpod::app::EngineMetrics metrics = state.engine->Metrics();
    if (metrics.completed_strokes != 2U || metrics.completed_samples <= 2U
        || metrics.preview_snapshots == 0U) {
        return 43;
    }

    if (state.engine->Invoke(
            [](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_undo(core, &result);
            },
            true,
            true)
            != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [](InkpodCore* core) {
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_redo(core, &result);
               },
               true,
               true)
            != INKPOD_STATUS_OK) {
        return 44;
    }
    InkpodDocumentInfo after_redo{};
    if (!QueryDocument(state, after_redo)
        || after_redo.color_plane_checksum != after_color.color_plane_checksum) {
        return 45;
    }

    const std::uint64_t revision_before_view = after_redo.document_revision;
    SendMessageW(state.Workspace().windows.canvas, WM_MBUTTONDOWN, MK_MBUTTON, MAKELPARAM(300, 220));
    SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_MBUTTON, MAKELPARAM(320, 230));
    SendMessageW(state.Workspace().windows.canvas, WM_MBUTTONUP, 0, MAKELPARAM(320, 230));
    RECT canvas_bounds{};
    GetWindowRect(state.Workspace().windows.canvas, &canvas_bounds);
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_MOUSEWHEEL,
        MAKEWPARAM(0, WHEEL_DELTA),
        MAKELPARAM(canvas_bounds.left + 320, canvas_bounds.top + 240));
    InkpodDocumentInfo after_view{};
    if (!QueryDocument(state, after_view)
        || after_view.document_revision != revision_before_view
        || after_view.view_revision == after_redo.view_revision) {
        return 46;
    }

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 47;
    }
    std::array<wchar_t, MAX_PATH> failing_destination{};
    _snwprintf_s(
        failing_destination.data(),
        failing_destination.size(),
        _TRUNCATE,
        L"%lsinkpod-save-failure-%lu-%llu",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path_before_failed_save =
        state.Document().shell.current_path;
    const std::size_t recent_before_failed_save =
        state.RecentDocumentCount();
    if (CreateDirectoryW(failing_destination.data(), nullptr) == FALSE) {
        return 52;
    }
    const InkpodStatus failed_save =
        SaveToPath(state, failing_destination.data());
    const BOOL removed_failure_directory =
        RemoveDirectoryW(failing_destination.data());
    InkpodDocumentInfo after_failed_save{};
    if (failed_save != INKPOD_STATUS_IO_ERROR
        || removed_failure_directory == FALSE
        || !QueryDocument(state, after_failed_save)
        || after_failed_save.document_revision != after_view.document_revision
        || (after_failed_save.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || state.Document().shell.current_path != path_before_failed_save
        || state.RecentDocumentCount() != recent_before_failed_save) {
        return 253;
    }
    std::array<wchar_t, MAX_PATH> temporary_file{};
    _snwprintf_s(
        temporary_file.data(),
        temporary_file.size(),
        _TRUNCATE,
        L"%lsinkpod-drawing-save-smoke-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(temporary_file.data());
    InkpodEditorStateInfo editor_before_save{};
    editor_before_save.struct_size = sizeof(editor_before_save);
    if (state.engine->Invoke(
            [&editor_before_save](InkpodCore* core) {
                return inkpod_core_get_editor_state(core, &editor_before_save);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 48;
    }
    if (SaveToPath(state, path) != INKPOD_STATUS_OK) {
        return 48;
    }
    const std::size_t path_separator = path.find_last_of(L"\\/");
    const std::wstring expected_saved_tab = path_separator == std::wstring::npos
        ? path
        : path.substr(path_separator + 1U);
    if (!ReadDocumentTabLabel(state.Workspace().windows.document_tabs, 0, tab_label)
        || tab_label != expected_saved_tab) {
        DeleteFileW(path.c_str());
        return 717;
    }
    if (GetFileAttributesW(initial_recovery_path.c_str()) != INVALID_FILE_ATTRIBUTES
        || GetLastError() != ERROR_FILE_NOT_FOUND) {
        DeleteFileW(path.c_str());
        return 216;
    }
    std::wstring initial_metadata_path;
    if (!RecoveryMetadataPath(initial_recovery_path, initial_metadata_path)
        || GetFileAttributesW(initial_metadata_path.c_str())
            != INVALID_FILE_ATTRIBUTES
        || GetLastError() != ERROR_FILE_NOT_FOUND) {
        DeleteFileW(path.c_str());
        return 222;
    }
    InkpodDocumentInfo saved{};
    InkpodEditorStateInfo editor_after_save{};
    editor_after_save.struct_size = sizeof(editor_after_save);
    if (!QueryDocument(state, saved)
        || (saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || state.engine->Invoke(
               [&editor_after_save](InkpodCore* core) {
                   return inkpod_core_get_editor_state(core, &editor_after_save);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (editor_after_save.flags & INKPOD_EDITOR_STATE_DIRTY) != 0U
        || editor_after_save.editor_revision != editor_before_save.editor_revision
        || std::memcmp(
               editor_after_save.editor_digest,
               editor_before_save.editor_digest,
               sizeof(editor_after_save.editor_digest)) != 0) {
        DeleteFileW(path.c_str());
        return 49;
    }
    const inkpod::app::DocumentSessionId saved_session = state.Document().id;
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || !state.CloseDocumentSession(saved_session)
        || OpenDocumentFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 50;
    }
    InkpodDocumentInfo reopened{};
    if (!QueryDocument(state, reopened)) {
        DeleteFileW(path.c_str());
        return 51;
    }
    if (!SamePersistentMetadata(saved, reopened)) {
        DeleteFileW(path.c_str());
        return 151;
    }
    if ((reopened.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        DeleteFileW(path.c_str());
        return 251;
    }
    InkpodEditorStateInfo editor_after_reopen{};
    editor_after_reopen.struct_size = sizeof(editor_after_reopen);
    if (state.engine->Invoke(
            [&editor_after_reopen](InkpodCore* core) {
                return inkpod_core_get_editor_state(core, &editor_after_reopen);
            },
            false,
            false) != INKPOD_STATUS_OK
        || (editor_after_reopen.flags & INKPOD_EDITOR_STATE_DIRTY) != 0U
        || editor_after_reopen.editor_revision != editor_before_save.editor_revision
        || std::memcmp(
               editor_after_reopen.editor_digest,
               editor_before_save.editor_digest,
               sizeof(editor_after_reopen.editor_digest)) != 0) {
        DeleteFileW(path.c_str());
        return 252;
    }
    const InkpodStrokeSample history_sample{
        sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U};
    const InkpodStrokeInput history_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x14c832ff),
        1.0F,
        &history_sample,
        1U,
        sizeof(history_sample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    InkpodSelectionInput history_selection{};
    history_selection.struct_size = sizeof(history_selection);
    history_selection.shape = INKPOD_SELECTION_RECTANGLE;
    history_selection.operation = INKPOD_SELECTION_NEW;
    history_selection.bounds = {10, 10, 1, 1};
    history_selection.interpretation = INKPOD_RANGE_NORMAL;
    history_selection.trace_shape = INKPOD_TRACE_ROUND;
    history_selection.view_zoom_q16 = INT64_C(1) << 16;
    if (state.engine->Invoke(
            [history_stroke, history_selection](InkpodCore* core) {
                InkpodStatus status = inkpod_core_set_active_plane(
                    core, INKPOD_PLANE_COLOR);
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_apply_stroke(core, &history_stroke, &result);
                }
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_apply_selection(
                        core, &history_selection, &result);
                }
                return status;
            },
            true,
            true) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 217;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILE_REVERT_PARTIAL, 0);
    InkpodDocumentInfo partially_reverted{};
    if (!QueryDocument(state, partially_reverted)
        || partially_reverted.color_plane_checksum != reopened.color_plane_checksum) {
        DeleteFileW(path.c_str());
        return 241;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_HISTORY_BACK, 0);
    InkpodHistoryInfo smoke_history{};
    smoke_history.struct_size = sizeof(smoke_history);
    if (state.engine->Invoke(
            [&smoke_history](InkpodCore* core) {
                return inkpod_core_history_info(core, &smoke_history);
            },
            false,
            false) != INKPOD_STATUS_OK
        || smoke_history.cursor != 0U || smoke_history.item_count < 3U) {
        DeleteFileW(path.c_str());
        return 219;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_HISTORY_FORWARD, 0);
    if (state.engine->Invoke(
            [&smoke_history](InkpodCore* core) {
                return inkpod_core_history_info(core, &smoke_history);
            },
            false,
            false) != INKPOD_STATUS_OK
        || smoke_history.cursor != smoke_history.item_count) {
        DeleteFileW(path.c_str());
        return 220;
    }
    if (SetEditorActiveTarget(
            state, reopened.layer_id, reopened.main_plane_id)
        != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 220;
    }
    DeleteFileW(path.c_str());

    inkpod::renderer::CanvasDocumentBounds before_dpi_bounds{};
    inkpod::renderer::CanvasDocumentBounds after_dpi_bounds{};
    const bool bounds_before_dpi = inkpod::renderer::GetCanvasDocumentBounds(
        state.Workspace().windows.canvas, before_dpi_bounds);
    const bool dpi_changed = SendMessageW(
                                 state.Workspace().windows.canvas,
                                 WM_DPICHANGED_AFTERPARENT,
                                 0,
                                 0) == 1;
    const bool bounds_after_dpi = inkpod::renderer::GetCanvasDocumentBounds(
        state.Workspace().windows.canvas, after_dpi_bounds);
    const bool dpi_transform_stable = bounds_before_dpi && bounds_after_dpi
        && std::abs(before_dpi_bounds.left - after_dpi_bounds.left) <= 0.01
        && std::abs(before_dpi_bounds.top - after_dpi_bounds.top) <= 0.01
        && std::abs(before_dpi_bounds.right - after_dpi_bounds.right) <= 0.01
        && std::abs(before_dpi_bounds.bottom - after_dpi_bounds.bottom) <= 0.01;
    const bool device_recovered = SendMessageW(
                                      state.Workspace().windows.canvas,
                                      inkpod::renderer::kCanvasSimulateDeviceLoss,
                                      0,
                                      0) == 1;
    const bool rendered = SendMessageW(
                              state.Workspace().windows.canvas,
                              inkpod::renderer::kCanvasRenderOnce,
                              0,
                              0) == 1;
    return dpi_changed && dpi_transform_stable && device_recovered && rendered ? 0 : 52;
}

int RunPaintingRecoverySmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 200;
    }
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_TOOL_FILL, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_TOOL_EYEDROPPER, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_TOOL_COLOR_REPLACE_RECTANGLE, MF_BYCOMMAND)
            == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_COLOR_CHECK_NATIVE, MF_BYCOMMAND)
            == static_cast<UINT>(-1)) {
        return 201;
    }

    const auto same_color = [](const InkpodColorValue& left, const InkpodColorValue& right) {
        return left.depth == right.depth && left.red == right.red
            && left.green == right.green && left.blue == right.blue
            && left.alpha == right.alpha;
    };
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_PENCIL, 0);
    const InkpodColorValue pencil_color = state.Workspace().tools.drawing_color;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    const InkpodColorValue default_fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 220U, 40U, 30U, 255U};
    const InkpodColorValue fill_command_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 1U, 2U, 3U, 255U};
    if (!same_color(state.Workspace().tools.drawing_color, default_fill_color)
        || state.Workspace().panes.color_pane.change_color == nullptr) {
        return 231;
    }
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, fill_command_color);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_PENCIL, 0);
    BOOL valid_red{};
    const UINT displayed_pencil_red = GetDlgItemInt(
        state.Workspace().windows.color_pane, IDC_COLOR_RED, &valid_red, FALSE);
    std::array<wchar_t, 64U> color_label{};
    GetDlgItemTextW(
        state.Workspace().windows.color_pane,
        IDC_COLOR_DRAWING_LABEL,
        color_label.data(),
        static_cast<int>(color_label.size()));
    if (!same_color(state.Workspace().tools.drawing_color, pencil_color)
        || !same_color(state.Workspace().panes.color_pane.drawing_color, pencil_color)
        || valid_red == FALSE || displayed_pencil_red != pencil_color.red
        || std::wcsstr(color_label.data(), L"#000000FF") == nullptr) {
        return 231;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    valid_red = FALSE;
    const UINT displayed_fill_red = GetDlgItemInt(
        state.Workspace().windows.color_pane, IDC_COLOR_RED, &valid_red, FALSE);
    GetDlgItemTextW(
        state.Workspace().windows.color_pane,
        IDC_COLOR_DRAWING_LABEL,
        color_label.data(),
        static_cast<int>(color_label.size()));
    if (!same_color(state.Workspace().tools.drawing_color, fill_command_color)
        || !same_color(state.Workspace().panes.color_pane.drawing_color, fill_command_color)
        || valid_red == FALSE || displayed_fill_red != fill_command_color.red
        || std::wcsstr(color_label.data(), L"#010203FF") == nullptr) {
        return 231;
    }

    // The preceding persistence smoke intentionally leaves a selection active.
    // Painting must respect it, so clear it before constructing this fill boundary.
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);

    std::array<InkpodStrokeSample, 5> boundary_samples{{
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput boundary{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        boundary_samples.data(),
        boundary_samples.size(),
        sizeof(InkpodStrokeSample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (state.engine->Invoke(
            [boundary](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &boundary, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 202;
    }
    InkpodDocumentInfo before_fill{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, before_fill)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)) {
        return 203;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(before_fill.width);
    const int fill_x = static_cast<int>(std::lround(bounds.left + 150.0 * zoom));
    const int fill_y = static_cast<int>(std::lround(bounds.top + 150.0 * zoom));
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1) {
        return 204;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));
    InkpodDocumentInfo after_fill{};
    if (!QueryDocument(state, after_fill)
        || after_fill.document_revision != before_fill.document_revision + 1U
        || after_fill.main_plane_checksum != before_fill.main_plane_checksum
        || after_fill.color_plane_checksum == before_fill.color_plane_checksum
        || after_fill.active_plane != INKPOD_PLANE_COLOR
        || state.Workspace().tools.active_plane != INKPOD_PLANE_COLOR
        || state.Workspace().panes.active_tree_layer_id != before_fill.layer_id
        || state.Workspace().panes.active_tree_plane_id != before_fill.color_plane_id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != before_fill.layer_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != before_fill.color_plane_id) {
        return 205;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_TOOL_COLOR_REPLACE_TARGET,
            0)
            != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_TOOL_COLOR_REPLACE_RECTANGLE,
               0)
            != 1) {
        return 1070;
    }
    const InkpodColorValue replacement_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 77U, 88U, 99U, 255U};
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, replacement_color);
    const auto replacement_sample = [&bounds, zoom](double x, double y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(bounds.left + x * zoom),
            static_cast<float>(bounds.top + y * zoom),
            1.0F,
            0U};
    };
    const std::array<InkpodStrokeSample, 2U> replacement_samples{
        replacement_sample(120.0, 120.0), replacement_sample(180.0, 180.0)};
    const inkpod::renderer::CanvasStrokeEvent replacement_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin,
        replacement_samples.data(),
        1U};
    const inkpod::renderer::CanvasStrokeEvent replacement_end{
        inkpod::renderer::CanvasStrokeEventKind::End,
        replacement_samples.data() + 1U,
        1U};
    const inkpod::renderer::CanvasStrokeEvent replacement_append{
        inkpod::renderer::CanvasStrokeEventKind::Append,
        replacement_samples.data() + 1U,
        1U};
    const inkpod::renderer::CanvasStrokeEvent replacement_cancel{
        inkpod::renderer::CanvasStrokeEventKind::Cancel, nullptr, 0U};
    InkpodDocumentInfo before_replacement{};
    InkpodDocumentInfo after_replacement{};
    inkpod::renderer::CanvasGeometryPreview replacement_preview{};
    replacement_preview.struct_size = sizeof(replacement_preview);
    if (!QueryDocument(state, before_replacement)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, replacement_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, replacement_end)
        || !QueryDocument(state, after_replacement)
        || after_replacement.document_revision
            != before_replacement.document_revision + 1U
        || after_replacement.main_plane_checksum
            != before_replacement.main_plane_checksum
        || after_replacement.color_plane_checksum
            == before_replacement.color_plane_checksum) {
        return 1071;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    InkpodDocumentInfo replacement_undone{};
    if (!QueryDocument(state, replacement_undone)
        || replacement_undone.color_plane_checksum
            != before_replacement.color_plane_checksum) {
        return 1072;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    InkpodDocumentInfo replacement_redone{};
    if (!QueryDocument(state, replacement_redone)
        || replacement_redone.color_plane_checksum
            != after_replacement.color_plane_checksum) {
        return 1073;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    if (!QueryDocument(state, after_fill)) {
        return 1074;
    }
    const std::array<UINT, 4U> replacement_shape_commands{
        IDM_TOOL_COLOR_REPLACE_PEN,
        IDM_TOOL_COLOR_REPLACE_RECTANGLE,
        IDM_TOOL_COLOR_REPLACE_POLYLINE,
        IDM_TOOL_COLOR_REPLACE_LASSO};
    for (const UINT command : replacement_shape_commands) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, command, 0)
                != 1
            || !inkpod::renderer::SubmitCanvasStrokeEvent(
                state.Workspace().windows.canvas, replacement_begin)
            || !inkpod::renderer::SubmitCanvasStrokeEvent(
                state.Workspace().windows.canvas, replacement_append)
            || !inkpod::renderer::GetCanvasGeometryPreview(
                state.Workspace().windows.canvas, replacement_preview)
            || replacement_preview.active != 1U
            || !inkpod::renderer::SubmitCanvasStrokeEvent(
                state.Workspace().windows.canvas, replacement_cancel)
            || !inkpod::renderer::GetCanvasGeometryPreview(
                state.Workspace().windows.canvas, replacement_preview)
            || replacement_preview.active != 0U) {
            return 1075;
        }
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    if (state.Workspace().panes.layer_palette_dialog.select_plane == nullptr) {
        return 791;
    }
    state.Workspace().panes.layer_palette_dialog.select_plane(
        state.Workspace().panes.layer_palette_dialog.context,
        before_fill.main_plane_id);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE
        || state.Workspace().panes.active_tree_plane_id != before_fill.main_plane_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != before_fill.main_plane_id) {
        return 791;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1) {
        return 792;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));
    InkpodDocumentInfo after_noop_fill{};
    if (!QueryDocument(state, after_noop_fill)
        || after_noop_fill.document_revision != after_fill.document_revision
        || after_noop_fill.main_plane_checksum != after_fill.main_plane_checksum
        || after_noop_fill.color_plane_checksum != after_fill.color_plane_checksum
        || after_noop_fill.active_plane != INKPOD_PLANE_MAIN_LINE
        || state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE
        || state.Workspace().panes.active_tree_plane_id != before_fill.main_plane_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != before_fill.main_plane_id) {
        return 792;
    }

    const std::uint32_t fill_color = state.Workspace().tools.color_rgba;
    state.Workspace().tools.color_rgba = UINT32_C(0x010203ff);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_EYEDROPPER, 0);
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1
        || state.Workspace().tools.color_rgba != fill_color) {
        return 206;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));

    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    state.Workspace().panes.layer_palette_dialog.select_plane(
        state.Workspace().panes.layer_palette_dialog.context,
        before_fill.color_plane_id);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_COLOR) {
        return 221;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    state.Workspace().tools.color_rgba = fill_color;
    InkpodEditorStateInfo fill_options_before{};
    fill_options_before.struct_size = sizeof(fill_options_before);
    if (!state.engine->GetEditorState(
            state.Document().id,
            state.Document().generation,
            fill_options_before)
        || state.Workspace().tools.editor.editor_revision
            != fill_options_before.editor_revision) {
        return 324;
    }
    InkpodEditorStateUpdate fill_options_update{};
    fill_options_update.struct_size = sizeof(fill_options_update);
    fill_options_update.kind = INKPOD_EDITOR_UPDATE_FILL_OPTIONS;
    fill_options_update.expected_editor_revision =
        fill_options_before.editor_revision;
    fill_options_update.fill = fill_options_before.fill;
    fill_options_update.fill.struct_size = sizeof(InkpodEditorFillOptions);
    fill_options_update.fill.operation = INKPOD_FILL_CLOSED_REGION;
    fill_options_update.fill.tolerance = 257U;
    fill_options_update.fill.gap_close = 1U;
    fill_options_update.fill.extension_distance = 2U;
    fill_options_update.fill.inclusion_mode = INKPOD_INCLUSION_NONE;
    fill_options_update.fill.inclusion_color_count = 0U;
    fill_options_update.fill.flags |= INKPOD_EDITOR_FILL_DETACHED_REGIONS
        | INKPOD_EDITOR_FILL_OVERFLOW_ABORT;
    if (state.UpdateEditorState(fill_options_update) != INKPOD_STATUS_OK) {
        return 324;
    }
    if (!DispatchEnabledCommand(
            state, state.Workspace().windows.window, IDM_TOOL_FILL_OPTIONS)) {
        return 321;
    }
    InkpodEditorStateInfo fill_options_editor{};
    fill_options_editor.struct_size = sizeof(fill_options_editor);
    if (!state.engine->GetEditorState(
            state.Document().id,
            state.Document().generation,
            fill_options_editor)
        || fill_options_editor.fill.operation != INKPOD_FILL_CLOSED_REGION) {
        return 322;
    }
    if (state.Workspace().tools.fill_options.operation
        != INKPOD_FILL_CLOSED_REGION) {
        return 323;
    }
    const auto device_x = [&](double document_x) {
        return static_cast<int>(std::lround(bounds.left + document_x * zoom));
    };
    const auto device_y = [&](double document_y) {
        return static_cast<int>(std::lround(bounds.top + document_y * zoom));
    };
    const auto canvas_drag = [&state](int x1, int y1, int x2, int y2) {
        if (SendMessageW(
                state.Workspace().windows.canvas,
                WM_LBUTTONDOWN,
                MK_LBUTTON,
                MAKELPARAM(x1, y1)) != 1) {
            return false;
        }
        SendMessageW(
            state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x2, y2));
        SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(x2, y2));
        return true;
    };
    InkpodDocumentInfo before_closed{};
    if (!QueryDocument(state, before_closed)) {
        return 222;
    }
    const int closed_left = device_x(90.0);
    const int closed_top = device_y(90.0);
    const int closed_right = device_x(210.0);
    const int closed_bottom = device_y(210.0);
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(closed_left, closed_top))
        != 1) {
        return 222;
    }
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_MOUSEMOVE,
        MK_LBUTTON,
        MAKELPARAM(closed_right, closed_bottom));
    inkpod::renderer::CanvasGeometryPreview closed_fill_preview{};
    closed_fill_preview.struct_size = sizeof(closed_fill_preview);
    InkpodDocumentInfo during_closed_preview{};
    const bool closed_preview_visible =
        inkpod::renderer::GetCanvasGeometryPreview(
            state.Workspace().windows.canvas, closed_fill_preview)
        && QueryDocument(state, during_closed_preview)
        && during_closed_preview.document_revision == before_closed.document_revision
        && closed_fill_preview.active == 1U
        && closed_fill_preview.closed == 1U
        && closed_fill_preview.point_count == 4U
        && closed_fill_preview.points[0].x < closed_fill_preview.points[1].x
        && closed_fill_preview.points[1].y < closed_fill_preview.points[2].y;
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_LBUTTONUP,
        0,
        MAKELPARAM(closed_right, closed_bottom));
    if (!closed_preview_visible) {
        return 1064;
    }
    InkpodDocumentInfo after_closed{};
    closed_fill_preview = {};
    closed_fill_preview.struct_size = sizeof(closed_fill_preview);
    if (!inkpod::renderer::GetCanvasGeometryPreview(
            state.Workspace().windows.canvas, closed_fill_preview)
        || closed_fill_preview.active != 0U
        || closed_fill_preview.point_count != 0U
        || !QueryDocument(state, after_closed)
        || after_closed.document_revision != before_closed.document_revision + 1U
        || after_closed.main_plane_checksum != before_closed.main_plane_checksum
        || after_closed.color_plane_checksum == before_closed.color_plane_checksum) {
        return 223;
    }

    const int extension_left = device_x(246.0);
    const int extension_top = device_y(246.0);
    const int extension_right = device_x(254.0);
    const int extension_bottom = device_y(254.0);
    const auto extension_seed_x = static_cast<float>(std::floor(
        ((static_cast<double>(extension_left) + static_cast<double>(extension_right)) / 2.0
            - bounds.left)
        / zoom));
    const auto extension_seed_y = static_cast<float>(std::floor(
        ((static_cast<double>(extension_top) + static_cast<double>(extension_bottom)) / 2.0
            - bounds.top)
        / zoom));
    const InkpodStrokeSample extension_source{
        sizeof(InkpodStrokeSample),
        0U,
        extension_seed_x,
        extension_seed_y,
        1.0F,
        0U};
    const InkpodStrokeInput extension_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x28b45aff),
        1.0F,
        &extension_source,
        1U,
        sizeof(extension_source),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (state.engine->Invoke(
            [extension_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &extension_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 224;
    }
    InkpodDocumentInfo before_extension{};
    QueryDocument(state, before_extension);
    if (!UpdateEditorFillOptionsForSmoke(
            state,
            [](InkpodEditorFillOptions& fill) {
                fill.operation = INKPOD_FILL_EXTENSION;
                fill.extension_distance = 3U;
                fill.flags &= ~INKPOD_EDITOR_FILL_DETACHED_REGIONS;
            })
        || !DispatchEnabledCommand(
            state, state.Workspace().windows.window, IDM_TOOL_FILL_OPTIONS)) {
        return 225;
    }
    if (!canvas_drag(
            extension_left,
            extension_top,
            extension_right,
            extension_bottom)) {
        return 225;
    }
    InkpodDocumentInfo after_extension{};
    if (!QueryDocument(state, after_extension)
        || after_extension.color_plane_checksum == before_extension.color_plane_checksum) {
        return 226;
    }

    InkpodSelectionInput persistent_selection{};
    persistent_selection.struct_size = sizeof(persistent_selection);
    persistent_selection.shape = INKPOD_SELECTION_ELLIPSE;
    persistent_selection.operation = INKPOD_SELECTION_NEW;
    persistent_selection.bounds = {300, 300, 8, 8};
    persistent_selection.interpretation = INKPOD_RANGE_NORMAL;
    persistent_selection.trace_shape = INKPOD_TRACE_ROUND;
    persistent_selection.view_zoom_q16 = INT64_C(1) << 16;
    if (state.engine->Invoke(
            [persistent_selection](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(
                    core, &persistent_selection, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 227;
    }
    if (!UpdateEditorFillOptionsForSmoke(
            state,
            [](InkpodEditorFillOptions& fill) {
                fill.operation = INKPOD_FILL_SEED;
                fill.flags |= INKPOD_EDITOR_FILL_DOCUMENT_SELECTION;
                fill.flags &= ~INKPOD_EDITOR_FILL_OVERFLOW_ABORT;
                fill.gap_close = 0U;
            })
        || !DispatchEnabledCommand(
            state, state.Workspace().windows.window, IDM_TOOL_FILL_OPTIONS)) {
        return 228;
    }
    const int selected_x = device_x(304.0);
    const int selected_y = device_y(304.0);
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(selected_x, selected_y)) != 1) {
        return 228;
    }
    SendMessageW(
        state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(selected_x, selected_y));
    InkpodColorValue selected_fill{};
    selected_fill.struct_size = sizeof(selected_fill);
    InkpodColorValue outside_fill{};
    outside_fill.struct_size = sizeof(outside_fill);
    InkpodStatus outside_fill_status = INKPOD_STATUS_OK;
    if (state.engine->Invoke(
            [&selected_fill, &outside_fill, &outside_fill_status](InkpodCore* core) {
                InkpodStatus status = inkpod_core_eyedropper(
                    core,
                    INKPOD_EYEDROPPER_SELECTED_PLANE,
                    304U,
                    304U,
                    &selected_fill);
                if (status == INKPOD_STATUS_OK) {
                    outside_fill_status = inkpod_core_eyedropper(
                        core,
                        INKPOD_EYEDROPPER_SELECTED_PLANE,
                        295U,
                        295U,
                        &outside_fill);
                }
                return status;
            },
            false,
            false) != INKPOD_STATUS_OK
        || selected_fill.alpha == 0U
        || outside_fill_status != INKPOD_STATUS_INVALID_STATE) {
        return 229;
    }
    state.Workspace().tools.fill_options = {};

    InkpodDocumentInfo before_check{};
    if (!QueryDocument(state, before_check)) {
        return 230;
    }
    const std::uint64_t revision_before_check = before_check.document_revision;
    const std::uint64_t view_before_check = before_check.view_revision;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_COLOR_CHECK_NATIVE, 0);
    InkpodDocumentInfo during_check{};
    std::uint64_t check_features{};
    std::uint32_t check_digest_algorithm{};
    const InkpodStatus check_snapshot_status = state.engine->Invoke(
        [&check_features, &check_digest_algorithm](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            InkpodSnapshotView view{};
            view.struct_size = sizeof(view);
            status = inkpod_snapshot_get_view(snapshot, &view);
            if (status == INKPOD_STATUS_OK) {
                check_features = view.feature_flags;
            }
            InkpodCanonicalDigest digest{};
            digest.struct_size = sizeof(digest);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_canonical_digest(snapshot, &digest);
                check_digest_algorithm = digest.algorithm;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, during_check)
        || check_snapshot_status != INKPOD_STATUS_OK
        || check_features != (INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
            | INKPOD_SNAPSHOT_FEATURE_SOLID_WHITE_BASE)
        || check_digest_algorithm != INKPOD_DIGEST_BLAKE3_256
        || during_check.document_revision != revision_before_check
        || during_check.view_revision <= view_before_check
        || SendMessageW(state.Workspace().windows.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 207;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_COLOR_CHECK_OFF, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 208;
    }
    const auto suffix = static_cast<unsigned long long>(GetTickCount64());
    std::array<wchar_t, MAX_PATH> normal_buffer{};
    std::array<wchar_t, MAX_PATH> recovery_buffer{};
    _snwprintf_s(
        normal_buffer.data(),
        normal_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-painting-normal-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    _snwprintf_s(
        recovery_buffer.data(),
        recovery_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-painting-recovery-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    const std::wstring normal_path(normal_buffer.data());
    const std::wstring recovery_path(recovery_buffer.data());
    if (SaveToPath(state, normal_path) != INKPOD_STATUS_OK) {
        return 209;
    }
    const inkpod::app::DocumentSessionId normal_session = state.Document().id;
    InkpodDocumentInfo normally_saved{};
    if (!QueryDocument(state, normally_saved)) {
        return 210;
    }
    std::array<InkpodStrokeSample, 1> edit_sample{{
        // Center of the persisted 8x8 ellipse selection; its corner is clipped.
        {sizeof(InkpodStrokeSample), 0U, 303.0F, 303.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput edit{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x040506ff),
        1.0F,
        edit_sample.data(),
        edit_sample.size(),
        sizeof(InkpodStrokeSample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (state.engine->Invoke(
            [edit](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &edit, &result);
            },
            true,
            true) != INKPOD_STATUS_OK
        || !QueueAutosave(
               state, state.routing.targets.Capture(), recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(normal_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(recovery_path.c_str()) == INVALID_FILE_ATTRIBUTES) {
        DeleteFileW(normal_path.c_str());
        (void)DiscardRecoveryArtifact(recovery_path);
        return 211;
    }
    InkpodDocumentInfo autosaved{};
    if (!QueryDocument(state, autosaved)
        || (autosaved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        DeleteFileW(normal_path.c_str());
        (void)DiscardRecoveryArtifact(recovery_path);
        return 212;
    }
    if (!state.CloseDocumentSession(normal_session)
        || CreateDefaultCell(state) != INKPOD_STATUS_OK
        || OpenRecoveryFromPath(state, recovery_path) != INKPOD_STATUS_OK) {
        DeleteFileW(normal_path.c_str());
        (void)DiscardRecoveryArtifact(recovery_path);
        return 213;
    }
    const inkpod::app::DocumentSessionId recovered_session = state.Document().id;
    InkpodDocumentInfo recovered{};
    const bool recovery_state = QueryDocument(state, recovered)
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        && recovered.color_plane_checksum == autosaved.color_plane_checksum
        && state.Document().shell.current_path.empty();
    const InkpodStatus revert_status = state.engine->Invoke(
        [](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_revert(core, &info);
        },
        false,
        false);
    const bool normal_unchanged = state.CloseDocumentSession(recovered_session)
        && OpenDocumentFromPath(state, normal_path) == INKPOD_STATUS_OK;
    InkpodDocumentInfo reopened_normal{};
    const bool normal_matches = QueryDocument(state, reopened_normal)
        && reopened_normal.color_plane_checksum == normally_saved.color_plane_checksum
        && reopened_normal.color_plane_checksum != recovered.color_plane_checksum;
    DeleteFileW(normal_path.c_str());
    (void)DiscardRecoveryArtifact(recovery_path);
    return recovery_state && revert_status == INKPOD_STATUS_INVALID_STATE && normal_unchanged
            && normal_matches
        ? 0
        : 214;
}

int RunDocumentEditingSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 300;
    }
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    for (const UINT command : {
             IDM_EDIT_COPY,
             IDM_EDIT_PASTE,
             IDM_EDIT_MIRROR_HORIZONTAL,
             IDM_LAYER_DUPLICATE,
             IDM_LAYER_DELETE,
             IDM_LAYER_MOVE_TOP,
             IDM_SELECTION_RECTANGLE,
             IDM_SELECTION_ELLIPSE,
             IDM_SELECTION_LASSO,
             IDM_SELECTION_POLYLINE,
             IDM_SELECTION_TRACE,
             IDM_SELECTION_WAND,
             IDM_SELECTION_MODE_NEW,
             IDM_SELECTION_MODE_ADD,
             IDM_SELECTION_MODE_SUBTRACT,
             IDM_SELECTION_MODE_INTERSECT,
             IDM_SELECTION_CLEAR,
             IDM_SELECTION_COLOR,
             IDM_SELECTION_COLOR_DIFFERENT,
             IDM_SELECTION_COLOR_ADD,
             IDM_SELECTION_OUTPUT_COLOR_GUARD,
             IDM_SELECTION_TO_LAYER,
             IDM_SELECTION_FROM_LAYER,
             IDM_SELECTION_LAYER_ADD,
             IDM_SELECTION_LAYER_SUBTRACT,
             IDM_SELECTION_ALL,
             IDM_SELECTION_INVERT,
             IDM_SELECTION_EXPAND,
             IDM_SELECTION_SHRINK,
             IDM_VIEW_FLIP_HORIZONTAL,
             IDM_VIEW_FLIP_VERTICAL,
             IDM_VIEW_GRID,
             IDM_VIEW_NEW,
             IDM_SHORTCUT_EDIT,
             IDM_SHORTCUT_RESET}) {
        if (menu == nullptr
            || GetMenuState(menu, command, MF_BYCOMMAND)
                == static_cast<UINT>(-1)) {
            return 301;
        }
    }

    const InkpodCellCreateOptions source_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000001),
        UINT64_C(0x4d33000000000002),
        8U,
        8U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [source_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &source_options, &info);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 302;
    }
    ResetUiForNewActiveDocument(state);
    if (!state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)
        || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        return 303;
    }

    InkpodDocumentInfo initial{};
    if (!QueryDocument(state, initial)) {
        return 304;
    }
    state.effects.last_output_color_guard_summary.clear();
    SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_SELECTION_OUTPUT_COLOR_GUARD,
        0);
    InkpodDocumentInfo after_output_guard{};
    if (!QueryDocument(state, after_output_guard)
        || after_output_guard.document_revision != initial.document_revision
        || state.effects.output_color_guard != nullptr
        || state.effects.output_color_guard_profile
            != INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR
        || state.effects.last_output_color_guard_summary.find(L"選択 0")
            == std::wstring::npos) {
        return 341;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_DUPLICATE, 0);
    const std::uint64_t duplicate_id = state.Document().shell.smoke_layer_id;
    InkpodDocumentInfo duplicated{};
    if (duplicate_id == 0U || !QueryDocument(state, duplicated)
        || duplicated.document_revision != initial.document_revision + 1U) {
        return 305;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MOVE_TOP, 0);
    InkpodNodeInfo top_layer{};
    top_layer.struct_size = sizeof(top_layer);
    const InkpodStatus top_status = state.engine->Invoke(
        [&top_layer](InkpodCore* core) {
            return inkpod_core_node_get(
                core, 0U, UINT32_MAX, &top_layer);
        },
        false,
        false);
    if (top_status != INKPOD_STATUS_OK || top_layer.id != duplicate_id) {
        return 306;
    }

    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_DELETE, 0);
    InkpodDocumentInfo after_delete{};
    if (!QueryDocument(state, after_delete)
        || (after_delete.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 307;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id != duplicate_id) {
        return 308;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id == duplicate_id) {
        return 309;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 310;
    }
    std::array<wchar_t, MAX_PATH> file_buffer{};
    _snwprintf_s(
        file_buffer.data(),
        file_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-document-editing-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(file_buffer.data());
    if (SaveToPath(state, path) != INKPOD_STATUS_OK
        || OpenFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 311;
    }
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    const bool reopened_tree = state.engine->Invoke(
                                   [&top_layer](InkpodCore* core) {
                                       return inkpod_core_node_get(
                                           core,
                                           0U,
                                           UINT32_MAX,
                                           &top_layer);
                                   },
                                   false,
                                   false) == INKPOD_STATUS_OK
        && top_layer.id == duplicate_id;
    DeleteFileW(path.c_str());
    if (!reopened_tree) {
        return 312;
    }

    InkpodDocumentInfo before_invalid{};
    if (!QueryDocument(state, before_invalid)) {
        return 313;
    }
    static constexpr std::array<std::uint8_t, 7> invalid_name{
        'I', 'n', 'v', 'a', 'l', 'i', 'd'};
    InkpodTreeEdit invalid_plane{};
    invalid_plane.struct_size = sizeof(invalid_plane);
    invalid_plane.operation = INKPOD_TREE_CREATE_PLANE;
    invalid_plane.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
    invalid_plane.parent_id = duplicate_id;
    invalid_plane.kind = INKPOD_TYPED_PLANE_SELECTION;
    invalid_plane.pixel_format = INKPOD_STORAGE_BINARY8;
    invalid_plane.opacity_milli = 1000U;
    invalid_plane.name_utf8 = invalid_name.data();
    invalid_plane.name_bytes = invalid_name.size();
    InkpodStatus invalid_status = INKPOD_STATUS_OK;
    state.engine->Invoke(
        [&invalid_plane, &invalid_status](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t object_id{};
            invalid_status = inkpod_core_tree_edit(
                core, &invalid_plane, &result, &object_id);
            return INKPOD_STATUS_OK;
        },
        false,
        false);
    InkpodDocumentInfo after_invalid{};
    if (invalid_status != INKPOD_STATUS_INVALID_ARGUMENT
        || !QueryDocument(state, after_invalid)
        || after_invalid.document_revision != before_invalid.document_revision) {
        return 314;
    }

    inkpod::renderer::CanvasDocumentBounds selection_canvas_bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, selection_canvas_bounds)) {
        return 315;
    }
    const auto selection_sample = [&selection_canvas_bounds](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(selection_canvas_bounds.left
                + (selection_canvas_bounds.right - selection_canvas_bounds.left)
                    * static_cast<double>(x) / 8.0),
            static_cast<float>(selection_canvas_bounds.top
                + (selection_canvas_bounds.bottom - selection_canvas_bounds.top)
                    * static_cast<double>(y) / 8.0),
            1.0F,
            0U};
    };
    const auto query_selection_preview = [&state](
                                             inkpod::renderer::CanvasGeometryPreview& preview) {
        preview = {};
        preview.struct_size = sizeof(preview);
        return inkpod::renderer::GetCanvasGeometryPreview(
            state.Workspace().windows.canvas, preview);
    };
    const auto send_selection_gesture = [&state](const auto& samples) noexcept {
        if (samples.empty()) {
            return false;
        }
        const inkpod::renderer::CanvasStrokeEvent begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
        if (!inkpod::renderer::SubmitCanvasStrokeEvent(
                state.Workspace().windows.canvas, begin)) {
            return false;
        }
        if (samples.size() > 2U) {
            const inkpod::renderer::CanvasStrokeEvent append{
                inkpod::renderer::CanvasStrokeEventKind::Append,
                samples.data() + 1U,
                samples.size() - 2U};
            if (!inkpod::renderer::SubmitCanvasStrokeEvent(
                    state.Workspace().windows.canvas, append)) {
                return false;
            }
        }
        const inkpod::renderer::CanvasStrokeEvent end{
            inkpod::renderer::CanvasStrokeEventKind::End,
            samples.data() + samples.size() - 1U,
            1U};
        return inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, end);
    };
    const auto query_selection = [&state](InkpodLocatorOutput& output) noexcept {
        output = {};
        output.struct_size = sizeof(output);
        return state.engine->Invoke(
                   [&output](InkpodCore* core) {
                       return inkpod_core_locator_sample(core, 0U, 0.0, 0.0, &output);
                   },
                   false,
                   false) == INKPOD_STATUS_OK;
    };
    if (!UpdateEditorSelectionOptionsForSmoke(
            state,
            [](InkpodEditorSelectionOptions& selection) {
                selection.interpretation = INKPOD_RANGE_BOUNDARY;
                selection.aspect_ratio_q16 = 1U << 16U;
                selection.construction_flags = INKPOD_SELECTION_FROM_CENTER
                    | INKPOD_SELECTION_CONSTRAIN_ROTATION_45
                    | INKPOD_SELECTION_TRACE_PRESSURE_SIZE
                    | INKPOD_SELECTION_TRACE_SCREEN_SIZE;
                selection.rotation_turns = UINT32_C(0x20000000);
                selection.trace_shape = INKPOD_TRACE_SQUARE;
            })) {
        return 1061;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_RECTANGLE, 0);
    const std::array<InkpodStrokeSample, 2U> option_preview_samples{
        selection_sample(2.0F, 2.0F), selection_sample(5.0F, 3.0F)};
    const inkpod::renderer::CanvasStrokeEvent option_preview_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin,
        option_preview_samples.data(),
        1U};
    const inkpod::renderer::CanvasStrokeEvent option_preview_append{
        inkpod::renderer::CanvasStrokeEventKind::Append,
        option_preview_samples.data() + 1U,
        1U};
    InkpodDocumentInfo before_option_preview{};
    InkpodDocumentInfo during_option_preview{};
    inkpod::renderer::CanvasGeometryPreview option_preview{};
    InkpodEditorStateInfo option_editor{};
    option_editor.struct_size = sizeof(option_editor);
    if (!QueryDocument(state, before_option_preview)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, option_preview_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, option_preview_append)
        || !query_selection_preview(option_preview)
        || !QueryDocument(state, during_option_preview)
        || during_option_preview.document_revision != before_option_preview.document_revision
        || option_preview.active != 1U || option_preview.closed != 1U
        || option_preview.point_count != 4U
        || option_preview.points[0].x == option_preview.points[1].x
        || option_preview.points[0].y == option_preview.points[1].y
        || !state.engine->GetEditorState(
            state.Document().id, state.Document().generation, option_editor)
        || option_editor.selection.interpretation != INKPOD_RANGE_BOUNDARY
        || option_editor.selection.trace_shape != INKPOD_TRACE_SQUARE
        || option_editor.selection.construction_flags
            != (INKPOD_SELECTION_FROM_CENTER
                | INKPOD_SELECTION_CONSTRAIN_ROTATION_45
                | INKPOD_SELECTION_TRACE_PRESSURE_SIZE
                | INKPOD_SELECTION_TRACE_SCREEN_SIZE)) {
        return 1062;
    }
    const inkpod::renderer::CanvasStrokeEvent option_preview_cancel{
        inkpod::renderer::CanvasStrokeEventKind::Cancel, nullptr, 0U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, option_preview_cancel)
        || !UpdateEditorSelectionOptionsForSmoke(
            state,
            [](InkpodEditorSelectionOptions& selection) {
                selection.interpretation = INKPOD_RANGE_NORMAL;
                selection.aspect_ratio_q16 = 0U;
                selection.construction_flags = 0U;
                selection.rotation_turns = 0U;
                selection.trace_shape = INKPOD_TRACE_ROUND;
            })) {
        return 1063;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_RECTANGLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
    const std::array<InkpodStrokeSample, 2U> preview_samples{
        selection_sample(1.0F, 1.0F), selection_sample(5.0F, 6.0F)};
    const inkpod::renderer::CanvasStrokeEvent preview_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin,
        preview_samples.data(),
        1U};
    const inkpod::renderer::CanvasStrokeEvent preview_append{
        inkpod::renderer::CanvasStrokeEventKind::Append,
        preview_samples.data() + 1U,
        1U};
    InkpodDocumentInfo before_selection_preview{};
    if (!QueryDocument(state, before_selection_preview)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, preview_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, preview_append)) {
        return 610;
    }
    inkpod::renderer::CanvasGeometryPreview selection_preview{};
    InkpodDocumentInfo during_selection_preview{};
    if (!query_selection_preview(selection_preview)
        || !QueryDocument(state, during_selection_preview)
        || during_selection_preview.document_revision
            != before_selection_preview.document_revision
        || selection_preview.active != 1U || selection_preview.closed != 1U
        || selection_preview.point_count != 4U
        || selection_preview.stroke_width != 0.0F) {
        return 611;
    }
    const inkpod::renderer::CanvasStrokeEvent preview_cancel{
        inkpod::renderer::CanvasStrokeEventKind::Cancel, nullptr, 0U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, preview_cancel)
        || !query_selection_preview(selection_preview)
        || selection_preview.active != 0U || selection_preview.point_count != 0U) {
        return 612;
    }
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, preview_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, preview_append)
        || !query_selection_preview(selection_preview)
        || selection_preview.active != 1U) {
        return 613;
    }
    const inkpod::renderer::CanvasStrokeEvent preview_end{
        inkpod::renderer::CanvasStrokeEventKind::End,
        preview_samples.data() + 1U,
        1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, preview_end)
        || !query_selection_preview(selection_preview)
        || selection_preview.active != 0U || selection_preview.point_count != 0U) {
        return 614;
    }
    const auto verify_selection_preview = [&](
                                              UINT command,
                                              std::uint32_t expected_points,
                                              bool expected_closed,
                                              bool expected_region_width) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0);
        if (!inkpod::renderer::SubmitCanvasStrokeEvent(
                state.Workspace().windows.canvas, preview_begin)
            || !inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, preview_append)
            || !query_selection_preview(selection_preview)
            || selection_preview.active != 1U
            || selection_preview.point_count != expected_points
            || selection_preview.closed != (expected_closed ? 1U : 0U)
            || (selection_preview.stroke_width > 0.0F) != expected_region_width) {
            return false;
        }
        return inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, preview_cancel)
            && query_selection_preview(selection_preview)
            && selection_preview.active == 0U
            && selection_preview.point_count == 0U;
    };
    if (!verify_selection_preview(IDM_SELECTION_ELLIPSE, 48U, true, false)
        || !verify_selection_preview(IDM_SELECTION_LASSO, 2U, true, false)
        || !verify_selection_preview(IDM_SELECTION_POLYLINE, 2U, true, false)
        || !verify_selection_preview(IDM_SELECTION_TRACE, 2U, false, true)) {
        return 615;
    }
    const auto select_rectangle = [&](UINT mode, float x1, float y1, float x2, float y2) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_RECTANGLE, 0);
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, mode, 0);
        const std::array<InkpodStrokeSample, 2U> samples{
            selection_sample(x1, y1), selection_sample(x2, y2)};
        return send_selection_gesture(samples);
    };
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 0.0F, 0.0F, 4.0F, 4.0F)
        || !select_rectangle(IDM_SELECTION_MODE_ADD, 4.0F, 0.0F, 6.0F, 2.0F)
        || !select_rectangle(IDM_SELECTION_MODE_SUBTRACT, 0.0F, 0.0F, 1.0F, 4.0F)
        || !select_rectangle(IDM_SELECTION_MODE_INTERSECT, 2.0F, 0.0F, 4.0F, 4.0F)) {
        return 315;
    }
    InkpodLocatorOutput locator{};
    if (!query_selection(locator)
        || (locator.flags & 1U) == 0U || locator.selection.x != 2
        || locator.selection.y != 0 || locator.selection.width != 2
        || locator.selection.height != 4) {
        return 316;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LASSO, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
    const std::array<InkpodStrokeSample, 2U> short_lasso_samples{
        selection_sample(2.0F, 2.0F), selection_sample(3.0F, 3.0F)};
    if (!send_selection_gesture(short_lasso_samples)) {
        return 616;
    }
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 617;
    }
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 1.0F, 1.0F, 5.0F, 5.0F)) {
        return 618;
    }
    InkpodDocumentInfo before_empty_selection_operation{};
    if (!QueryDocument(state, before_empty_selection_operation)) {
        return 619;
    }
    for (const UINT mode : {
             IDM_SELECTION_MODE_ADD, IDM_SELECTION_MODE_SUBTRACT}) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LASSO, 0);
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, mode, 0);
        if (!send_selection_gesture(short_lasso_samples)) {
            return 620;
        }
        InkpodDocumentInfo after_empty_selection_operation{};
        if (!QueryDocument(state, after_empty_selection_operation)
            || after_empty_selection_operation.document_revision
                != before_empty_selection_operation.document_revision
            || !query_selection(locator) || (locator.flags & 1U) == 0U
            || locator.selection.x != 1 || locator.selection.y != 1
            || locator.selection.width != 4 || locator.selection.height != 4) {
            return 621;
        }
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_ELLIPSE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
    const std::array<InkpodStrokeSample, 2U> ellipse_samples{
        selection_sample(1.0F, 1.0F), selection_sample(7.0F, 7.0F)};
    if (!send_selection_gesture(ellipse_samples)) {
        return 357;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LASSO, 0);
    const std::array<InkpodStrokeSample, 3U> lasso_samples{
        selection_sample(0.0F, 0.0F),
        selection_sample(7.0F, 0.0F),
        selection_sample(0.0F, 7.0F)};
    if (!send_selection_gesture(lasso_samples)) {
        return 345;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_POLYLINE, 0);
    const std::array<InkpodStrokeSample, 3U> polyline_samples{
        selection_sample(1.0F, 1.0F),
        selection_sample(7.0F, 1.0F),
        selection_sample(7.0F, 7.0F)};
    if (!send_selection_gesture(polyline_samples)) {
        return 346;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_TRACE, 0);
    const std::array<InkpodStrokeSample, 2U> trace_samples{
        selection_sample(0.5F, 7.5F), selection_sample(7.5F, 0.5F)};
    if (!send_selection_gesture(trace_samples)) {
        return 347;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_WAND, 0);
    InkpodEditorStateInfo wand_editor{};
    wand_editor.struct_size = sizeof(wand_editor);
    if (!state.engine->GetEditorState(
            state.Document().id,
            state.Document().generation,
            wand_editor)
        || wand_editor.selection.shape != INKPOD_SELECTION_WAND) {
        return 448;
    }
    const std::array<InkpodStrokeSample, 1U> wand_samples{
        selection_sample(4.0F, 4.0F)};
    if (!send_selection_gesture(wand_samples)) {
        return 348;
    }
    if (!query_selection(locator)) {
        return 449;
    }
    if ((locator.flags & 1U) == 0U) {
        return 450;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_INVERT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_EXPAND, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_SHRINK, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 349;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_ALL, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.width != 8 || locator.selection.height != 8) {
        return 350;
    }

    InkpodDocumentInfo source_info{};
    if (!QueryDocument(state, source_info)
        || state.Workspace().panes.layer_palette_dialog.select_layer == nullptr
        || state.Workspace().panes.layer_palette_dialog.select_plane == nullptr) {
        return 317;
    }
    state.Workspace().panes.layer_palette_dialog.select_layer(
        state.Workspace().panes.layer_palette_dialog.context,
        source_info.layer_id);
    state.Workspace().panes.layer_palette_dialog.select_plane(
        state.Workspace().panes.layer_palette_dialog.context,
        source_info.main_plane_id);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE) {
        return 317;
    }

    const InkpodStrokeSample source_sample{
        sizeof(InkpodStrokeSample), 0U, 6.0F, 6.0F, 1.0F, 0U};
    const InkpodStrokeInput source_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &source_sample,
        1U,
        sizeof(source_sample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (state.engine->Invoke(
            [source_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &source_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 317;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_COLOR, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.x != 6 || locator.selection.y != 6
        || locator.selection.width != 1 || locator.selection.height != 1) {
        return 351;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_COLOR_DIFFERENT, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.width != 8 || locator.selection.height != 8) {
        return 352;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_COLOR_ADD, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_TO_LAYER, 0);
    if (state.Document().shell.selection_layer_id == 0U) {
        return 353;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_FROM_LAYER, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U) {
        return 354;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LAYER_ADD, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LAYER_SUBTRACT, 0);
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 355;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MAIN_LINE, 0);
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 6.0F, 6.0F, 7.0F, 7.0F)) {
        return 356;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_COPY, 0);
    if (state.clipboard == nullptr
        || IsClipboardFormatAvailable(CF_DIBV5) == FALSE
        || IsClipboardFormatAvailable(InkpodClipboardFormat()) == FALSE
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_CUT, 0) != 1) {
        return 318;
    }
    std::vector<std::uint8_t> external_dib;
    if (OpenClipboard(state.Workspace().windows.window) == FALSE) {
        return 367;
    }
    HANDLE dib_handle = GetClipboardData(CF_DIBV5);
    const SIZE_T dib_size = dib_handle == nullptr ? 0U : GlobalSize(dib_handle);
    const void* dib_source = dib_handle == nullptr ? nullptr : GlobalLock(dib_handle);
    try {
        if (dib_source != nullptr && dib_size != 0U) {
            external_dib.assign(
                static_cast<const std::uint8_t*>(dib_source),
                static_cast<const std::uint8_t*>(dib_source) + dib_size);
        }
    } catch (const std::bad_alloc&) {
        external_dib.clear();
    }
    if (dib_source != nullptr) {
        GlobalUnlock(dib_handle);
    }
    CloseClipboard();
    HGLOBAL external_handle = external_dib.empty()
        ? nullptr
        : GlobalAlloc(GMEM_MOVEABLE, external_dib.size());
    void* external_destination = external_handle == nullptr
        ? nullptr
        : GlobalLock(external_handle);
    if (external_destination == nullptr) {
        if (external_handle != nullptr) {
            GlobalFree(external_handle);
        }
        return 369;
    }
    std::memcpy(external_destination, external_dib.data(), external_dib.size());
    GlobalUnlock(external_handle);
    if (OpenClipboard(state.Workspace().windows.window) == FALSE) {
        GlobalFree(external_handle);
        return 370;
    }
    EmptyClipboard();
    const bool external_published = SetClipboardData(CF_DIBV5, external_handle) != nullptr;
    if (!external_published) {
        GlobalFree(external_handle);
    }
    CloseClipboard();
    InkpodClipboard* private_clipboard = state.clipboard;
    state.clipboard = nullptr;
    int external_failure{};
    if (!external_published) {
        external_failure = 371;
    } else if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_PASTE, 0) != 1) {
        external_failure = 372;
    } else if (!state.Workspace().tools.floating_active) {
        external_failure = 373;
    } else if (OpenClipboard(state.Workspace().windows.window) == FALSE) {
        external_failure = 374;
    } else {
        const bool emptied = EmptyClipboard() != FALSE;
        CloseClipboard();
        if (!emptied) {
            external_failure = 375;
        } else {
            inkpod_clipboard_release(&state.clipboard);
            if (SendMessageW(
                    state.Workspace().windows.window,
                    WM_COMMAND,
                    IDM_EDIT_FLOATING_COMMIT,
                    0) != 1) {
                external_failure = 376;
            } else if (state.Workspace().tools.floating_active) {
                external_failure = 377;
            }
        }
    }
    if (state.Workspace().tools.floating_active) {
        SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_FLOATING_CANCEL,
            0);
    }
    inkpod_clipboard_release(&state.clipboard);
    state.clipboard = private_clipboard;
    PublishStandardClipboard(state.Workspace().windows.window, state.clipboard);
    if (external_failure != 0) {
        return external_failure;
    }

    const InkpodCellCreateOptions destination_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000003),
        UINT64_C(0x4d33000000000004),
        4U,
        4U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [destination_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(
                    core, &destination_options, &info);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 319;
    }
    ResetUiForNewActiveDocument(state);
    if (!state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)) {
        return 451;
    }
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 452;
    }
    if (!RefreshTreePane(state)) {
        return 453;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_PASTE_SELECTED,
            0) != 1
        || !state.Workspace().tools.floating_active) {
        return 454;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_FLOATING_CANCEL,
            0) != 1) {
        return 455;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_PASTE_CONVERTED,
            0) != 1
        || !state.Workspace().tools.floating_active) {
        return 456;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_FLOATING_CANCEL,
            0) != 1) {
        return 457;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_PASTE,
            0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_FLOATING_TRANSFORM,
               0) != 1
        || state.Workspace().tools.floating_transform.anchor
            != INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT
        || state.Workspace().tools.floating_transform.target_x != 2.0
        || state.Workspace().tools.floating_transform.target_y != 1.0
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_FLOATING_CANCEL,
               0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_PASTE,
               0) != 1) {
        return 458;
    }
    inkpod::renderer::CanvasDocumentBounds floating_canvas{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, floating_canvas)) {
        return 359;
    }
    const double floating_zoom = (floating_canvas.right - floating_canvas.left) / 4.0;
    const InkpodStrokeSample floating_start{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(floating_canvas.left + 2.0 * floating_zoom),
        static_cast<float>(floating_canvas.top + 2.0 * floating_zoom), 1.0F, 0U};
    const InkpodStrokeSample floating_end{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(floating_canvas.left + 1.0 * floating_zoom),
        static_cast<float>(floating_canvas.top + 1.0 * floating_zoom), 1.0F, 0U};
    const inkpod::renderer::CanvasStrokeEvent floating_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &floating_start, 1U};
    const inkpod::renderer::CanvasStrokeEvent floating_finish{
        inkpod::renderer::CanvasStrokeEventKind::End, &floating_end, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, floating_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, floating_finish)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1) {
        return 359;
    }
    const InkpodFloatingTransform floating{
        sizeof(InkpodFloatingTransform),
        INKPOD_TRANSFORM_ANCHOR_CENTER,
        2.5,
        2.5,
        1.0,
        1.0,
        0.0};
    InkpodDocumentInfo paste_target_info{};
    if (!QueryDocument(state, paste_target_info)
        || state.Workspace().panes.layer_palette_dialog.select_layer == nullptr
        || state.Workspace().panes.layer_palette_dialog.select_plane == nullptr) {
        return 320;
    }
    state.Workspace().panes.layer_palette_dialog.select_layer(
        state.Workspace().panes.layer_palette_dialog.context,
        paste_target_info.layer_id);
    state.Workspace().panes.layer_palette_dialog.select_plane(
        state.Workspace().panes.layer_palette_dialog.context,
        paste_target_info.main_plane_id);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE) {
        return 320;
    }
    const InkpodStatus paste_status = state.engine->Invoke(
        [&state, floating](InkpodCore* core) {
            InkpodStatus status = inkpod_core_paste_begin(
                core, state.clipboard);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_floating_transform(core, &floating);
            }
            if (status == INKPOD_STATUS_OK) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_floating_commit(core, &result);
            }
            if (status != INKPOD_STATUS_OK) {
                inkpod_core_floating_cancel(core);
            }
            return status;
        },
        true,
        true);
    InkpodColorValue pasted_color{};
    pasted_color.struct_size = sizeof(pasted_color);
    if (paste_status != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [&pasted_color](InkpodCore* core) {
                   return inkpod_core_eyedropper(
                       core,
                       INKPOD_EYEDROPPER_SELECTED_PLANE,
                       2U,
                       2U,
                       &pasted_color);
               },
               false,
               false) != INKPOD_STATUS_OK
        || pasted_color.red != 0U || pasted_color.green != 0U
        || pasted_color.blue != 0U || pasted_color.alpha == 0U) {
        return 320;
    }

    InkpodDocumentInfo before_flip{};
    if (!QueryDocument(state, before_flip)) {
        return 321;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_FLIP_HORIZONTAL, 0);
    InkpodDocumentInfo after_flip{};
    InkpodSnapshotTransform transform{};
    transform.struct_size = sizeof(transform);
    const InkpodStatus transform_status = state.engine->Invoke(
        [&transform](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &snapshot);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(snapshot, &transform);
            }
            const InkpodStatus release_status =
                inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, after_flip)
        || after_flip.document_revision != before_flip.document_revision
        || after_flip.view_revision <= before_flip.view_revision
        || transform_status != INKPOD_STATUS_OK
        || (transform.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            == 0U) {
        return 322;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_MIRROR_HORIZONTAL, 0);
    InkpodDocumentInfo after_mirror{};
    if (!QueryDocument(state, after_mirror)
        || after_mirror.document_revision
            != after_flip.document_revision + 1U
        || after_mirror.view_revision != after_flip.view_revision
        || (after_mirror.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 323;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    const std::array<UINT, 6U> document_transform_commands{
        IDM_CELL_MIRROR_VERTICAL,
        IDM_CELL_ROTATE_LEFT,
        IDM_CELL_ROTATE_RIGHT,
        IDM_CELL_IMAGE_SIZE,
        IDM_CELL_RESOLUTION,
        IDM_CELL_PAPER_SETTINGS};
    for (std::size_t index = 0U; index < document_transform_commands.size(); ++index) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, document_transform_commands[index], 0) != 1) {
            return 360 + static_cast<int>(index);
        }
    }
    InkpodDocumentInfo transformed_document{};
    if (!QueryDocument(state, transformed_document)
        || transformed_document.width <= 4U || transformed_document.height <= 4U
        || transformed_document.dpi_x_milli != 120000U
        || transformed_document.dpi_y_milli != 120000U) {
        return 366;
    }

    const int tab_count_before_view = TabCtrl_GetItemCount(
        state.Workspace().windows.document_tabs);
    const std::size_t view_count_before = state.Document().ViewCount();
    std::wstring secondary_tab_label;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_VIEW_NEW,
            0) != 1
        || state.ActiveView().presentation.secondary_view_id == 0U || state.ActiveView().presentation.active_view_id != state.ActiveView().presentation.secondary_view_id
        || state.Workspace().windows.document_tabs == nullptr
        || state.Document().ViewCount() != view_count_before + 1U
        || TabCtrl_GetItemCount(state.Workspace().windows.document_tabs)
            != tab_count_before_view + 1
        || !ReadDocumentTabLabel(
            state.Workspace().windows.document_tabs,
            TabCtrl_GetCurSel(state.Workspace().windows.document_tabs),
            secondary_tab_label)
        || secondary_tab_label.find(L"[ビュー 2]") == std::wstring::npos) {
        return 324;
    }
    const InkpodViewInput secondary_pan{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_PAN_BY,
        0U,
        5.0,
        0.0,
        0.0,
        0.0};
    const std::uint64_t secondary_view_id = state.ActiveView().presentation.secondary_view_id;
    if (state.engine->Invoke(
            [secondary_view_id, secondary_pan](InkpodCore* core) {
                return inkpod_core_view_apply(
                    core, secondary_view_id, &secondary_pan);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 324;
    }
    const InkpodStrokeSample multi_view_sample{
        sizeof(InkpodStrokeSample), 0U, 0.0F, 0.0F, 1.0F, 0U};
    const InkpodStrokeInput multi_view_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &multi_view_sample,
        1U,
        sizeof(multi_view_sample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (state.engine->Invoke(
            [multi_view_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &multi_view_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 325;
    }
    std::uint64_t primary_revision{};
    std::uint64_t secondary_revision{};
    double primary_pan_x{};
    double secondary_pan_x{};
    const InkpodStatus multi_view_status = state.engine->Invoke(
        [secondary_view_id,
         &primary_revision,
         &secondary_revision,
         &primary_pan_x,
         &secondary_pan_x](
            InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* primary{};
            InkpodSnapshot* secondary{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &primary);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot_for_view(
                    core, secondary_view_id, &options, &secondary);
            }
            InkpodSnapshotView primary_view{};
            primary_view.struct_size = sizeof(primary_view);
            InkpodSnapshotView secondary_view{};
            secondary_view.struct_size = sizeof(secondary_view);
            InkpodSnapshotTransform primary_transform{};
            primary_transform.struct_size = sizeof(primary_transform);
            InkpodSnapshotTransform secondary_transform{};
            secondary_transform.struct_size = sizeof(secondary_transform);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(primary, &primary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(
                    secondary, &secondary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    primary, &primary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    secondary, &secondary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                primary_revision = primary_view.revision;
                secondary_revision = secondary_view.revision;
                primary_pan_x = primary_transform.pan_x;
                secondary_pan_x = secondary_transform.pan_x;
            }
            const InkpodStatus primary_release =
                inkpod_snapshot_release(&primary);
            const InkpodStatus secondary_release =
                inkpod_snapshot_release(&secondary);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            return primary_release == INKPOD_STATUS_OK
                    && secondary_release == INKPOD_STATUS_OK
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        false,
        false);
    if (multi_view_status != INKPOD_STATUS_OK || primary_revision == 0U
        || primary_revision != secondary_revision
        || primary_pan_x == secondary_pan_x) {
        return 326;
    }
    UpdateMenuState(state);
    std::array<wchar_t, 96U> zoom_status{};
    SendMessageW(
        state.Workspace().windows.status_bar,
        SB_GETTEXTW,
        1,
        reinterpret_cast<LPARAM>(zoom_status.data()));
    if (wcsstr(zoom_status.data(), L"ズーム:") == nullptr) {
        return 758;
    }
    const InkpodShortcutSequence* multi_stroke =
        windows::ui::FindShortcutSequence(state.shortcuts.bindings, IDM_FILE_REVERT);
    if (multi_stroke == nullptr || multi_stroke->stroke_count <= 1U) {
        return 359;
    }
    for (std::uint32_t index = 0; index < multi_stroke->stroke_count; ++index) {
        UINT resolved{};
        const InkpodShortcutMatch match = windows::ui::ResolveShortcutStroke(
            state.shortcuts, multi_stroke->strokes[index], resolved);
        const bool last = index + 1U == multi_stroke->stroke_count;
        if ((!last && match != INKPOD_SHORTCUT_MATCH_PREFIX)
            || (last
                && (match != INKPOD_SHORTCUT_MATCH_EXACT || resolved != IDM_FILE_REVERT))) {
            return 360;
        }
    }

    InkpodShortcutSequence undo_shortcut{};
    undo_shortcut.struct_size = sizeof(undo_shortcut);
    undo_shortcut.command_id = IDM_EDIT_UNDO;
    undo_shortcut.stroke_count = 1U;
    undo_shortcut.strokes[0] = {
        static_cast<std::uint32_t>('U'), INKPOD_SHORTCUT_MODIFIER_CONTROL};
    const InkpodStatus navigation_status = windows::ui::RebindShortcut(
        *state.engine, state.shortcuts, undo_shortcut, false);
    if (navigation_status != INKPOD_STATUS_OK) {
        return 327;
    }
    UINT shortcut_menu_command{};
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 328;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GRID_SETTINGS, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GUIDE_VERTICAL, 0);
    const auto query_guide_count = [&state]() noexcept {
        std::uint64_t count = UINT64_MAX;
        const std::uint64_t view_id = state.ActiveView().presentation.active_view_id;
        const InkpodStatus status = state.engine->Invoke(
            [view_id, &count](InkpodCore* core) {
                const InkpodSnapshotOptions options{
                    sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
                InkpodSnapshot* snapshot{};
                InkpodStatus inner = view_id == 0U
                    ? inkpod_core_build_snapshot(core, &options, &snapshot)
                    : inkpod_core_build_snapshot_for_view(
                          core, view_id, &options, &snapshot);
                InkpodSnapshotOverlay overlay{};
                overlay.struct_size = sizeof(overlay);
                if (inner == INKPOD_STATUS_OK) {
                    inner = inkpod_snapshot_get_overlay(snapshot, &overlay);
                }
                if (inner == INKPOD_STATUS_OK) {
                    count = overlay.guide_count;
                }
                const InkpodStatus released = inkpod_snapshot_release(&snapshot);
                return inner == INKPOD_STATUS_OK ? released : inner;
            },
            false,
            false);
        return status == INKPOD_STATUS_OK ? count : UINT64_MAX;
    };
    if (query_guide_count() != 1U) {
        return 341;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_RULER, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GRID, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_SNAP_GUIDES, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_SNAP_GRID, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_TRANSPARENT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_ZOOM_PERCENT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_BOX_ZOOM, 0);
    inkpod::renderer::CanvasDocumentBounds box_bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, box_bounds)) {
        return 329;
    }
    const std::array<InkpodStrokeSample, 2U> box_samples{
        InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(box_bounds.left
                + (box_bounds.right - box_bounds.left) * 0.25),
            static_cast<float>(box_bounds.top
                + (box_bounds.bottom - box_bounds.top) * 0.25),
            1.0F,
            0U},
        InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(box_bounds.left
                + (box_bounds.right - box_bounds.left) * 0.75),
            static_cast<float>(box_bounds.top
                + (box_bounds.bottom - box_bounds.top) * 0.75),
            1.0F,
            0U}};
    const inkpod::renderer::CanvasStrokeEvent box_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin,
        box_samples.data(),
        1U};
    const inkpod::renderer::CanvasStrokeEvent box_end{
        inkpod::renderer::CanvasStrokeEventKind::End,
        box_samples.data() + 1U,
        1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, box_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, box_end)) {
        return 329;
    }
    inkpod::renderer::CanvasDocumentBounds guide_bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, guide_bounds)) {
        return 329;
    }
    const double guide_zoom = (guide_bounds.right - guide_bounds.left) / 4.0;
    const auto send_guide_drag = [&state](
                                     InkpodStrokeSample begin_sample,
                                     InkpodStrokeSample end_sample) noexcept {
        const inkpod::renderer::CanvasStrokeEvent begin_event{
            inkpod::renderer::CanvasStrokeEventKind::Begin, &begin_sample, 1U};
        const inkpod::renderer::CanvasStrokeEvent end_event{
            inkpod::renderer::CanvasStrokeEventKind::End, &end_sample, 1U};
        return inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, begin_event)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, end_event);
    };
    const auto guide_sample = [](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample), 0U, x, y, 1.0F, 0U};
    };
    const float guide_x1 = static_cast<float>(guide_bounds.left + guide_zoom);
    const float guide_x3 = static_cast<float>(guide_bounds.left + 3.0 * guide_zoom);
    const float guide_y = static_cast<float>((guide_bounds.top + guide_bounds.bottom) / 2.0);
    if (!send_guide_drag(
            guide_sample(30.0F, 10.0F),
            guide_sample(guide_x1, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 2U) {
        return 342;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GUIDE_MOVE, 0);
    if (!send_guide_drag(
            guide_sample(guide_x1, guide_y),
            guide_sample(guide_x3, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 2U) {
        return 343;
    }
    if (!send_guide_drag(
            guide_sample(guide_x3, guide_y),
            guide_sample(-10.0F, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 1U) {
        return 344;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_SHORTCUT_EDIT, 0) != 1) {
        return 329;
    }
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 330;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SHORTCUT_RESET, 0);
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO) {
        return 361;
    }
    locator = {};
    locator.struct_size = sizeof(locator);
    const std::uint64_t active_view_id = state.ActiveView().presentation.active_view_id;
    if (!state.ActiveView().presentation.grid_visible || !state.ActiveView().presentation.ruler_visible || !state.ActiveView().presentation.snap_guides
        || !state.ActiveView().presentation.snap_grid || state.ActiveView().presentation.transparent_visible
        || state.engine->Invoke(
               [active_view_id, &locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, active_view_id, 2.0, 2.0, &locator);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        return 331;
    }
    bool overlay_connected{};
    std::uint32_t smoke_overlay_flags{};
    std::uint32_t smoke_grid_spacing{};
    std::uint32_t smoke_grid_subdivisions{};
    std::uint64_t smoke_guide_count{};
    const InkpodStatus overlay_status = state.engine->Invoke(
        [active_view_id,
         &overlay_connected,
         &smoke_overlay_flags,
         &smoke_grid_spacing,
         &smoke_grid_subdivisions,
         &smoke_guide_count](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = active_view_id == 0U
                ? inkpod_core_build_snapshot(core, &options, &snapshot)
                : inkpod_core_build_snapshot_for_view(
                      core, active_view_id, &options, &snapshot);
            InkpodSnapshotOverlay overlay{};
            overlay.struct_size = sizeof(overlay);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_overlay(snapshot, &overlay);
            }
            if (status == INKPOD_STATUS_OK) {
                smoke_overlay_flags = overlay.flags;
                smoke_grid_spacing = overlay.grid_spacing_x;
                smoke_grid_subdivisions = overlay.grid_subdivisions;
                smoke_guide_count = overlay.guide_count;
                overlay_connected =
                    (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) == 0U
                    && overlay.grid_spacing_x == 8U
                    && overlay.grid_subdivisions == 2U
                    && overlay.guide_count == 1U
                    && overlay.guides != nullptr;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (overlay_status != INKPOD_STATUS_OK || !overlay_connected) {
        if (overlay_status != INKPOD_STATUS_OK) {
            return 332;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) == 0U) {
            return 334;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE) == 0U) {
            return 335;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED) == 0U) {
            return 336;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) != 0U) {
            return 337;
        }
        if (smoke_grid_spacing != 8U || smoke_grid_subdivisions != 2U) {
            return 338;
        }
        return smoke_guide_count == 1U
            ? 339
            : 340 + static_cast<int>(std::min<std::uint64_t>(smoke_guide_count, 10U));
    }
    return SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 333;
}

int RunAnnotationWorkflowSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr || state.Workspace().windows.canvas == nullptr) {
        return 1250;
    }
    if (CreateCell(state, 128U, 96U, 96000U) != INKPOD_STATUS_OK) {
        return 1251;
    }
    if (!state.RefreshEditorPresentation(state.Document().id, state.Document().generation)) {
        return 1252;
    }
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 1264;
    }
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    for (const UINT command : {
             IDM_ANNOTATION_ADD_TEXT, IDM_ANNOTATION_EDIT_TEXT,
             IDM_ANNOTATION_DRAW_INSTRUCTION, IDM_ANNOTATION_SELECT_PREVIOUS,
             IDM_ANNOTATION_SELECT_NEXT, IDM_ANNOTATION_MOVE_LEFT,
             IDM_ANNOTATION_MOVE_RIGHT, IDM_ANNOTATION_DELETE}) {
        wchar_t accessible_name[96]{};
        if (menu == nullptr || GetMenuState(menu, command, MF_BYCOMMAND) == static_cast<UINT>(-1)
            || GetMenuStringW(
                   menu, command, accessible_name,
                   static_cast<int>(std::size(accessible_name)), MF_BYCOMMAND) == 0) {
            return 1253;
        }
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_ANNOTATION_ADD_TEXT, 0);
    InkpodDocumentInfo after_text = EmptyDocumentInfo();
    if (!QueryDocument(state, after_text)
        || state.Document().shell.annotation_layer_id == 0U
        || state.Document().shell.active_annotation_id == 0U) {
        return 1254;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_ANNOTATION_EDIT_TEXT, 0);
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_ANNOTATION_MOVE_RIGHT, 0);
    InkpodDocumentInfo after_move = EmptyDocumentInfo();
    if (!QueryDocument(state, after_move)
        || after_move.document_revision != after_text.document_revision + 1U) {
        return 1255;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_ANNOTATION_DRAW_INSTRUCTION, 0);
    if (!state.Document().shell.annotation_draw_active) {
        return 1256;
    }
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, bounds)) {
        return 1257;
    }
    const std::array<InkpodStrokeSample, 3U> samples{
        InkpodStrokeSample{sizeof(InkpodStrokeSample), 0U,
            static_cast<float>(bounds.left + 20.0),
            static_cast<float>(bounds.top + 20.0), 1.0F, 0U},
        InkpodStrokeSample{sizeof(InkpodStrokeSample), 0U,
            static_cast<float>(bounds.left + 45.0),
            static_cast<float>(bounds.top + 32.0), 0.8F, 0U},
        InkpodStrokeSample{sizeof(InkpodStrokeSample), 0U,
            static_cast<float>(bounds.left + 70.0),
            static_cast<float>(bounds.top + 25.0), 1.0F, 0U}};
    const inkpod::renderer::CanvasStrokeEvent begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
    const inkpod::renderer::CanvasStrokeEvent append{
        inkpod::renderer::CanvasStrokeEventKind::Append, samples.data() + 1U, 2U};
    const inkpod::renderer::CanvasStrokeEvent end{
        inkpod::renderer::CanvasStrokeEventKind::End, nullptr, 0U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, append)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, end)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 1258;
    }
    bool annotation_snapshot_ok{};
    const InkpodStatus snapshot_status = state.engine->Invoke(
        [&annotation_snapshot_ok](InkpodCore* core) {
            InkpodSnapshotOptions snapshot_options{};
            snapshot_options.struct_size = sizeof(snapshot_options);
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &snapshot_options, &snapshot);
            InkpodSnapshotAnnotationView view{};
            view.struct_size = sizeof(view);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_annotations(snapshot, &view);
            }
            bool has_text{};
            bool has_instruction_stroke{};
            if (status == INKPOD_STATUS_OK) {
                const auto* bytes = reinterpret_cast<const std::byte*>(view.objects);
                for (std::uint64_t index = 0U; index < view.object_count; ++index) {
                    const auto* object = reinterpret_cast<const InkpodSnapshotAnnotation*>(
                        bytes + static_cast<std::size_t>(index * view.object_stride_bytes));
                    has_text = has_text || object->kind == INKPOD_ANNOTATION_TEXT;
                    has_instruction_stroke = has_instruction_stroke
                        || (object->kind == INKPOD_ANNOTATION_STROKE
                            && object->output == INKPOD_ANNOTATION_OUTPUT_INSTRUCTION);
                }
            }
            annotation_snapshot_ok = status == INKPOD_STATUS_OK
                && has_text && has_instruction_stroke;
            const InkpodStatus release = snapshot == nullptr
                ? INKPOD_STATUS_OK : inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release : status;
        },
        false,
        false);
    if (snapshot_status != INKPOD_STATUS_OK || !annotation_snapshot_ok) {
        return 1259;
    }
    const std::uint64_t selected_annotation_id =
        state.Document().shell.active_annotation_id;
    const InkpodStatus fallback_status = state.engine->Invoke(
        [selected_annotation_id](InkpodCore* core) {
            InkpodSnapshotOptions snapshot_options{};
            snapshot_options.struct_size = sizeof(snapshot_options);
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &snapshot_options, &snapshot);
            InkpodSnapshotAnnotationView view{};
            view.struct_size = sizeof(view);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_annotations(snapshot, &view);
            }
            const InkpodSnapshotAnnotation* found{};
            if (status == INKPOD_STATUS_OK) {
                const auto* bytes = reinterpret_cast<const std::byte*>(view.objects);
                for (std::uint64_t index = 0U; index < view.object_count; ++index) {
                    const auto* object = reinterpret_cast<const InkpodSnapshotAnnotation*>(
                        bytes + static_cast<std::size_t>(index * view.object_stride_bytes));
                    if (object->object_id == selected_annotation_id) {
                        found = object;
                        break;
                    }
                }
                if (found == nullptr) {
                    status = INKPOD_STATUS_INVALID_STATE;
                }
            }
            InkpodDocumentInfo info = EmptyDocumentInfo();
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_get_document_info(core, &info);
            }
            constexpr char missing_font[] = "__inkpod_missing_font__";
            InkpodAnnotationObjectInput input{};
            InkpodAnnotationEdit edit{};
            InkpodAnnotationEditResult result{};
            if (status == INKPOD_STATUS_OK) {
                input.struct_size = sizeof(input);
                input.kind = found->kind;
                input.layer_id = found->layer_id;
                input.output = found->output;
                input.style_flags = found->style_flags;
                input.bounds = found->bounds;
                input.font_family_utf8 = reinterpret_cast<const std::uint8_t*>(missing_font);
                input.font_family_bytes = std::size(missing_font) - 1U;
                input.font_size_milli = found->font_size_milli;
                input.stroke_width_milli = found->stroke_width_milli;
                input.color = found->color;
                input.text_utf8 = view.utf8_bytes
                    + static_cast<std::size_t>(found->text_utf8_offset);
                input.text_bytes = found->text_utf8_bytes;
                input.points = nullptr;
                input.point_count = 0U;
                input.point_stride_bytes = 0U;
                edit.struct_size = sizeof(edit);
                edit.kind = INKPOD_ANNOTATION_EDIT_UPDATE;
                edit.object_id = found->object_id;
                edit.input = &input;
                result.struct_size = sizeof(result);
                status = inkpod_core_annotation_edit(
                    core, info.document_revision, &edit, 1U, sizeof(edit), &result);
            }
            const InkpodStatus release = snapshot == nullptr
                ? INKPOD_STATUS_OK : inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release : status;
        },
        true,
        true);
    if (fallback_status != INKPOD_STATUS_OK
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        return 1263;
    }
    InkpodDocumentInfo before_cancel = EmptyDocumentInfo();
    InkpodDocumentInfo after_cancel = EmptyDocumentInfo();
    const inkpod::renderer::CanvasStrokeEvent cancel{
        inkpod::renderer::CanvasStrokeEventKind::Cancel, nullptr, 0U};
    if (!QueryDocument(state, before_cancel)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, cancel)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || !QueryDocument(state, after_cancel)
        || after_cancel.document_revision != before_cancel.document_revision) {
        return 1260;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            inkpod::renderer::kCanvasSimulateDeviceLoss,
            0,
            0) != 1
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        return 1261;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_ANNOTATION_DRAW_INSTRUCTION, 0);
    return state.Document().shell.annotation_draw_active ? 1262 : 0;
}

int RunShootingFrameWorkflowSmoke(ApplicationHost& state) noexcept {
    InkpodDocumentInfo before{};
    if (!QueryDocument(state, before)) {
        return 1260;
    }
    const auto query_frame = [&state](
                                 bool& present,
                                 InkpodShootingFrameInfo& frame) noexcept {
        frame = {};
        frame.struct_size = sizeof(frame);
        std::uint32_t raw_present{};
        if (state.engine == nullptr) {
            return false;
        }
        const InkpodStatus status = state.engine->Invoke(
            [&raw_present, &frame](InkpodCore* core) {
                return inkpod_core_shooting_frame_get(
                    core, &raw_present, &frame);
            },
            false,
            false);
        present = status == INKPOD_STATUS_OK && raw_present != 0U;
        return status == INKPOD_STATUS_OK;
    };
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CELL_SHOOTING_FRAME_PROPERTIES,
            0) != 1) {
        return 1261;
    }
    InkpodDocumentInfo created_info{};
    InkpodShootingFrameInfo created{};
    bool present{};
    if (!QueryDocument(state, created_info)
        || !query_frame(present, created)
        || !present || created.frame_id == 0U
        || created.rotation_turns == 0U
        || created_info.document_revision != before.document_revision + 1U) {
        return 1262;
    }
    constexpr wchar_t kInstructionExportPath[] =
        L"inkpod-shooting-frame-instruction-smoke.png";
    std::wstring previous_smoke_raster_path;
    try {
        previous_smoke_raster_path = state.lifetime.smoke_raster_path;
        state.lifetime.smoke_raster_path = kInstructionExportPath;
    } catch (const std::bad_alloc&) {
        return 1263;
    }
    DeleteFileW(kInstructionExportPath);
    const LRESULT instruction_export = SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_FILE_EXPORT_INSTRUCTION_RASTER,
        0);
    const bool instruction_export_exists =
        GetFileAttributesW(kInstructionExportPath) != INVALID_FILE_ATTRIBUTES;
    DeleteFileW(kInstructionExportPath);
    state.lifetime.smoke_raster_path.swap(previous_smoke_raster_path);
    if (instruction_export != 1 || !instruction_export_exists) {
        return 1263;
    }
    inkpod::renderer::CanvasDocumentBounds bounds{};
    HWND canvas = state.Workspace().windows.canvas;
    if (canvas == nullptr
        || !inkpod::renderer::GetCanvasDocumentBounds(canvas, bounds)
        || before.width == 0U) {
        return 1264;
    }
    const double zoom = (bounds.right - bounds.left)
        / static_cast<double>(before.width);
    const auto to_device = [&](double x, double y) noexcept {
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(before.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(before.height) - y;
        }
        return POINT{
            static_cast<LONG>(std::llround(bounds.left + x * zoom)),
            static_cast<LONG>(std::llround(bounds.top + y * zoom))};
    };
    const POINT start = to_device(
        static_cast<double>(created.center_x_milli) / 1000.0,
        static_cast<double>(created.center_y_milli) / 1000.0);
    const POINT moved{start.x + 18, start.y + 12};
    if (SendMessageW(canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(start.x, start.y)) != 1
        || SendMessageW(canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(moved.x, moved.y)) != 1
        || SendMessageW(canvas, WM_LBUTTONUP, 0, MAKELPARAM(moved.x, moved.y)) != 1) {
        return 1265;
    }
    InkpodDocumentInfo moved_info{};
    InkpodShootingFrameInfo moved_frame{};
    if (!QueryDocument(state, moved_info)
        || !query_frame(present, moved_frame)
        || !present
        || moved_info.document_revision != created_info.document_revision + 1U
        || (moved_frame.center_x_milli == created.center_x_milli
            && moved_frame.center_y_milli == created.center_y_milli)) {
        return 1266;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    InkpodShootingFrameInfo undone{};
    if (!query_frame(present, undone) || !present
        || undone.center_x_milli != created.center_x_milli
        || undone.center_y_milli != created.center_y_milli) {
        return 1267;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    InkpodShootingFrameInfo redone{};
    InkpodDocumentInfo before_cancel{};
    if (!QueryDocument(state, before_cancel)
        || !query_frame(present, redone) || !present
        || redone.center_x_milli != moved_frame.center_x_milli
        || redone.center_y_milli != moved_frame.center_y_milli) {
        return 1268;
    }
    const POINT cancel_start = to_device(
        static_cast<double>(redone.center_x_milli) / 1000.0,
        static_cast<double>(redone.center_y_milli) / 1000.0);
    if (SendMessageW(
            canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(cancel_start.x, cancel_start.y)) != 1) {
        return 1269;
    }
    SendMessageW(canvas, WM_CAPTURECHANGED, 0, 0);
    InkpodDocumentInfo after_cancel{};
    InkpodShootingFrameInfo cancelled{};
    if (!QueryDocument(state, after_cancel)
        || !query_frame(present, cancelled) || !present
        || after_cancel.document_revision != before_cancel.document_revision
        || cancelled.center_x_milli != redone.center_x_milli
        || cancelled.center_y_milli != redone.center_y_milli
        || SendMessageW(canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 1270;
    }
    return 0;
}

int RunProductionWorkflowSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr
        || CreateCell(state, 32U, 24U, 120000U) != INKPOD_STATUS_OK) {
        return 400;
    }
    RefreshTreePane(state);
    RefreshLightTablePane(state);
    for (const UINT command : {
             IDM_CELL_FRAME_REFERENCE,
             IDM_CELL_FRAME_DRAWING,
             IDM_CELL_FRAME_SAFE,
             IDM_CELL_MARGINS}) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0);
    }
    InkpodDocumentInfo paper{};
    if (!QueryDocument(state, paper) || paper.width != 32U || paper.height != 24U
        || paper.dpi_x_milli != 120000U || paper.margin_left != 1U
        || paper.margin_top != 2U || paper.margin_right != 3U
        || paper.margin_bottom != 4U || paper.reference_frame.x != 1
        || paper.drawing_frame.x != 2 || paper.safe_frame.x != 3) {
        return 401;
    }

    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_NEW, 0) != 0
        || state.Workspace().panes.tree_layer_count < 2U
        || LayerPaletteItemCount(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.tree_layer_count) {
        return 402;
    }
    const std::uint64_t raster_layer_id = state.Workspace().panes.active_tree_layer_id;
    const std::uint64_t raster_plane_id = state.Workspace().panes.active_tree_plane_id;
    if (raster_layer_id == 0U || raster_plane_id == 0U
        || state.Workspace().panes.tree_plane_count != 1U
        || state.Workspace().panes.active_tree_plane_index != 0U) {
        return 770;
    }
    InkpodNodeInfo first_layer{};
    first_layer.struct_size = sizeof(first_layer);
    const InkpodStatus first_layer_status = state.engine->Invoke(
        [&first_layer](InkpodCore* core) {
            return inkpod_core_node_get(core, 0U, UINT32_MAX, &first_layer);
        },
        false,
        false);
    const HWND layer_list = GetDlgItem(state.Workspace().panes.layer_palette, IDC_LAYER_LIST);
    SendMessageW(layer_list, LB_SETCURSEL, 0, 0);
    SendMessageW(
        state.Workspace().panes.layer_palette,
        WM_COMMAND,
        MAKEWPARAM(IDC_LAYER_LIST, LBN_SELCHANGE),
        reinterpret_cast<LPARAM>(layer_list));
    if (first_layer_status != INKPOD_STATUS_OK
        || state.Workspace().panes.active_tree_layer_id != first_layer.id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != first_layer.id) {
        return 470;
    }
    if (state.Workspace().panes.layer_palette_dialog.select_layer == nullptr) {
        return 771;
    }
    state.Workspace().panes.layer_palette_dialog.select_layer(
        state.Workspace().panes.layer_palette_dialog.context,
        raster_layer_id);
    if (state.Workspace().panes.active_tree_layer_id != raster_layer_id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != raster_layer_id
        || state.Workspace().panes.tree_plane_count != 1U
        || state.Workspace().panes.active_tree_plane_id != raster_plane_id
        || state.Workspace().panes.active_tree_plane_index != 0U) {
        return 771;
    }
    if (!UpdateEditorFillOptionsForSmoke(
            state,
            [](InkpodEditorFillOptions& fill) {
                fill = {};
                fill.struct_size = sizeof(InkpodEditorFillOptions);
                fill.operation = INKPOD_FILL_SEED;
                fill.extension_distance = 1U;
                fill.inclusion_mode = INKPOD_INCLUSION_NONE;
            })) {
        return 793;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    const InkpodColorValue generic_fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 90U, 80U, 70U, 255U};
    inkpod::windows::ui::tools::SetActiveCommandColor(
        state.Workspace().tools, generic_fill_color);
    inkpod::renderer::CanvasDocumentBounds generic_bounds{};
    InkpodDocumentInfo before_generic_fill{};
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
        || !QueryDocument(state, before_generic_fill)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, generic_bounds)) {
        return 793;
    }
    const double generic_zoom =
        (generic_bounds.right - generic_bounds.left)
        / static_cast<double>(before_generic_fill.width);
    const int generic_fill_x = static_cast<int>(
        std::lround(generic_bounds.left + generic_zoom));
    const int generic_fill_y = static_cast<int>(
        std::lround(generic_bounds.top + generic_zoom));
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(generic_fill_x, generic_fill_y)) != 1) {
        return 794;
    }
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_LBUTTONUP,
        0,
        MAKELPARAM(generic_fill_x, generic_fill_y));
    InkpodDocumentInfo after_generic_fill{};
    if (!QueryDocument(state, after_generic_fill)) {
        return 795;
    }
    if (after_generic_fill.document_revision
        != before_generic_fill.document_revision + 1U) {
        return 796;
    }
    if (after_generic_fill.color_plane_checksum
        != before_generic_fill.color_plane_checksum) {
        return 797;
    }
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_COLOR) {
        return 798;
    }
    if (state.Workspace().panes.active_tree_layer_id != raster_layer_id
        || state.Workspace().panes.active_tree_plane_id != raster_plane_id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != raster_layer_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette) != raster_plane_id) {
        return 799;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    bool selected_raster_changed{};
    bool coloring_plane_unchanged{};
    const InkpodStatus selected_raster_stroke_status = state.engine->Invoke(
        [raster_layer_id, &selected_raster_changed, &coloring_plane_unchanged](
            InkpodCore* core) {
            InkpodDocumentInfo before = EmptyDocumentInfo();
            InkpodStatus current = inkpod_core_get_document_info(core, &before);
            const std::array<InkpodStrokeSample, 1U> samples{
                InkpodStrokeSample{
                    sizeof(InkpodStrokeSample), 0U, 3.0F, 4.0F, 1.0F, 0U}};
            const InkpodStrokeInput stroke{
                sizeof(InkpodStrokeInput),
                INKPOD_TOOL_BRUSH,
                INKPOD_PLANE_COLOR,
                INKPOD_COORDINATE_SPACE_DOCUMENT,
                0U,
                UINT32_C(0x3ac45eff),
                3.0F,
                samples.data(),
                samples.size(),
                sizeof(InkpodStrokeSample),
                INKPOD_BRUSH_ROUND,
                0U,
                0U,
                INKPOD_START_COLOR_ANY,
                0U};
            InkpodDispatchResult dispatch{};
            dispatch.struct_size = sizeof(dispatch);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_apply_stroke(core, &stroke, &dispatch);
            }
            InkpodDocumentInfo after = EmptyDocumentInfo();
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &after);
            }
            coloring_plane_unchanged = current == INKPOD_STATUS_OK
                && after.color_plane_checksum == before.color_plane_checksum;

            std::array<std::uint8_t, 32U * 24U * 4U> pixels{};
            InkpodLayerThumbnailBuffer thumbnail{};
            thumbnail.struct_size = sizeof(thumbnail);
            thumbnail.layer_id = raster_layer_id;
            thumbnail.maximum_width = 32U;
            thumbnail.maximum_height = 24U;
            thumbnail.pixels_rgba8 = pixels.data();
            thumbnail.pixel_capacity = pixels.size();
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_layer_thumbnail(core, &thumbnail);
            }
            if (current == INKPOD_STATUS_OK && thumbnail.width == 32U
                && thumbnail.height == 24U && thumbnail.stride_bytes >= 32U * 4U) {
                const std::size_t alpha = static_cast<std::size_t>(4U)
                        * thumbnail.stride_bytes
                    + static_cast<std::size_t>(3U) * 4U + 3U;
                selected_raster_changed = alpha < pixels.size() && pixels[alpha] != 0U;
            }
            if (current == INKPOD_STATUS_OK) {
                InkpodDispatchResult undo{};
                undo.struct_size = sizeof(undo);
                current = inkpod_core_undo(core, &undo);
            }
            return current;
        },
        true,
        true);
    if (selected_raster_stroke_status != INKPOD_STATUS_OK || !selected_raster_changed
        || !coloring_plane_unchanged) {
        return 790;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_TOGGLE_VISIBLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_TOGGLE_EDITABLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_OPACITY, 0);
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_PROPERTIES, 0) != 1) {
        return 402;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_CONVERT, 0);
    const std::uint32_t plane_count_before_create = state.Workspace().panes.tree_plane_count;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_NEW, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id == 0U
        || state.Workspace().panes.active_tree_plane_id == raster_plane_id
        || state.Workspace().panes.active_tree_plane_index != plane_count_before_create) {
        return 772;
    }
    const std::uint64_t created_plane_id = state.Workspace().panes.active_tree_plane_id;
    InkpodNodeInfo created_plane{};
    created_plane.struct_size = sizeof(created_plane);
    const std::uint32_t created_layer_index = state.Workspace().panes.active_tree_layer_index;
    const std::uint32_t created_plane_index = state.Workspace().panes.active_tree_plane_index;
    const InkpodStatus created_plane_status = state.engine->Invoke(
        [created_layer_index, created_plane_index, &created_plane](InkpodCore* core) {
            return inkpod_core_node_get(
                core, created_layer_index, created_plane_index, &created_plane);
        },
        false,
        false);
    if (created_plane_status != INKPOD_STATUS_OK
        || created_plane.id != created_plane_id
        || created_plane.kind != INKPOD_TYPED_PLANE_RASTER
        || created_plane.pixel_format != INKPOD_STORAGE_RGBA8) {
        return 832;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_TOGGLE_VISIBLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_TOGGLE_EDITABLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_OPACITY, 0);
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_PROPERTIES, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_CONVERT, 0) != 1
        || state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id != created_plane_id) {
        return 773;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_DUPLICATE, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 2U
        || state.Workspace().panes.active_tree_plane_id == 0U
        || state.Workspace().panes.active_tree_plane_id == created_plane_id
        || state.Workspace().panes.active_tree_plane_index != plane_count_before_create + 1U) {
        return 774;
    }
    const std::uint64_t duplicated_plane_id = state.Workspace().panes.active_tree_plane_id;
    const std::uint32_t plane_move_start = state.Workspace().panes.active_tree_plane_index;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    if (plane_move_start == 0U
        || state.Workspace().panes.active_tree_plane_index != plane_move_start - 1U
        || state.Workspace().panes.active_tree_plane_id != duplicated_plane_id) {
        return 775;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    if (state.Workspace().panes.active_tree_plane_index != 0U
        || state.Workspace().panes.active_tree_plane_id != duplicated_plane_id) {
        return 776;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_DOWN, 0);
    if (state.Workspace().panes.active_tree_plane_index != 1U
        || state.Workspace().panes.active_tree_plane_id != duplicated_plane_id) {
        return 777;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_DELETE, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id != created_plane_id
        || state.Workspace().panes.active_tree_plane_index != plane_count_before_create) {
        return 778;
    }
    const std::uint64_t merge_destination_plane_id =
        state.Workspace().panes.active_tree_plane_id;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_DUPLICATE, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 2U) {
        return 7791;
    }
    if (state.Workspace().panes.active_tree_plane_id == 0U) {
        return 7792;
    }
    if (state.Workspace().panes.active_tree_plane_id == merge_destination_plane_id) {
        return 7793;
    }
    if (state.Workspace().panes.active_tree_plane_index
        != plane_count_before_create + 1U) {
        return 7794;
    }
    const std::uint64_t merge_source_plane_id = state.Workspace().panes.active_tree_plane_id;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    if (state.Workspace().panes.active_tree_plane_index != plane_count_before_create
        || state.Workspace().panes.active_tree_plane_id != merge_source_plane_id) {
        return 780;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MERGE, 0) != 1) {
        return 781;
    }
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id != merge_destination_plane_id
        || state.Workspace().panes.active_tree_plane_index != plane_count_before_create) {
        return 782;
    }
    SendMessageW(
        GetDlgItem(state.Workspace().panes.layer_palette, IDM_LAYER_DUPLICATE),
        BM_CLICK,
        0,
        0);
    const std::uint32_t layer_move_start = state.Workspace().panes.active_tree_layer_index;
    if (layer_move_start != 0U) {
        if (state.Workspace().panes.layer_palette_dialog.reorder_layer == nullptr) {
            return 469;
        }
        state.Workspace().panes.layer_palette_dialog.reorder_layer(
            state.Workspace().panes.layer_palette_dialog.context,
            state.Workspace().panes.active_tree_layer_id,
            layer_move_start - 1U);
        if (state.Workspace().panes.active_tree_layer_index != layer_move_start - 1U) {
            return 469;
        }
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MOVE_UP, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MOVE_DOWN, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MERGE, 0);

    auto query_grouped_active_nodes = [&state](
                                          InkpodNodeInfo& layer,
                                          InkpodNodeInfo& plane) {
        layer = {};
        layer.struct_size = sizeof(layer);
        plane = {};
        plane.struct_size = sizeof(plane);
        return state.engine->Invoke(
            [&state, &layer, &plane](InkpodCore* core) {
                InkpodStatus status = inkpod_core_node_get(
                    core,
                    state.Workspace().panes.active_tree_layer_index,
                    UINT32_MAX,
                    &layer);
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_node_get(
                        core,
                        state.Workspace().panes.active_tree_layer_index,
                        state.Workspace().panes.active_tree_plane_index,
                        &plane);
                }
                return status;
            },
            false,
            false);
    };
    InkpodNodeInfo grouped_layer_node{};
    InkpodNodeInfo grouped_plane_node{};
    if (query_grouped_active_nodes(grouped_layer_node, grouped_plane_node)
        != INKPOD_STATUS_OK) {
        return 7831;
    }
    if ((grouped_layer_node.flags & INKPOD_NODE_EDITABLE) == 0U) {
        SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_LAYER_TOGGLE_EDITABLE,
            0);
    }
    if ((grouped_plane_node.flags & INKPOD_NODE_EDITABLE) == 0U) {
        SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_PLANE_TOGGLE_EDITABLE,
            0);
    }
    if (query_grouped_active_nodes(grouped_layer_node, grouped_plane_node)
            != INKPOD_STATUS_OK
        || (grouped_layer_node.flags & INKPOD_NODE_EDITABLE) == 0U
        || (grouped_plane_node.flags & INKPOD_NODE_EDITABLE) == 0U) {
        return 7832;
    }
    std::vector<InkpodEditTarget> grouped_plane_targets;
    try {
        grouped_plane_targets.reserve(state.Workspace().panes.tree_plane_count);
    } catch (const std::bad_alloc&) {
        return 7833;
    }
    for (std::uint32_t index = 0U;
         index < state.Workspace().panes.tree_plane_count;
         ++index) {
        InkpodNodeInfo node{};
        node.struct_size = sizeof(node);
        const InkpodStatus status = state.engine->Invoke(
            [&state, index, &node](InkpodCore* core) {
                return inkpod_core_node_get(
                    core,
                    state.Workspace().panes.active_tree_layer_index,
                    index,
                    &node);
            },
            false,
            false);
        if (status != INKPOD_STATUS_OK || node.id == 0U) {
            return 7833;
        }
        grouped_plane_targets.push_back(InkpodEditTarget{
            sizeof(InkpodEditTarget),
            INKPOD_EDIT_TARGET_PLANE,
            grouped_layer_node.id,
            node.id,
            0U});
    }
    InkpodEditorStateInfo grouped_editor{};
    grouped_editor.struct_size = sizeof(grouped_editor);
    const InkpodEditTargetCommand make_editable{
        sizeof(InkpodEditTargetCommand),
        INKPOD_EDIT_TARGET_SET_EDITABILITY,
        1U,
        0U,
        0U,
        0U};
    InkpodDispatchResult grouped_dispatch{};
    std::vector<InkpodEditTarget> grouped_outputs;
    if (!state.engine->GetEditorState(
            state.Document().id,
            state.Document().generation,
            grouped_editor)
        || state.engine->SetEditTargets(
               state.Document().id,
               state.Document().generation,
               grouped_editor.editor_revision,
               grouped_plane_targets) != INKPOD_STATUS_OK
        || state.engine->ApplyEditTargetCommand(
               state.Document().id,
               state.Document().generation,
               make_editable,
               grouped_dispatch,
               grouped_outputs) != INKPOD_STATUS_OK) {
        return 7833;
    }

    const std::uint32_t grouped_layer_count = state.Workspace().panes.tree_layer_count;
    if (state.Workspace().panes.layer_palette_dialog.toggle_target == nullptr
        || state.Workspace().panes.active_tree_layer_id == 0U) {
        return 783;
    }
    SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_SELECTION_ALL,
        0);
    const std::uint64_t grouped_active_layer =
        state.Workspace().panes.active_tree_layer_id;
    const std::uint64_t grouped_active_plane =
        state.Workspace().panes.active_tree_plane_id;
    state.Workspace().panes.layer_palette_dialog.toggle_target(
        state.Workspace().panes.layer_palette_dialog.context,
        state.Workspace().panes.active_tree_layer_id,
        false,
        false);
    std::vector<InkpodEditTarget> grouped_targets;
    if (state.engine->GetEditTargets(
            state.Document().id,
            state.Document().generation,
            grouped_targets) != INKPOD_STATUS_OK
        || grouped_targets.size() != 1U
        || grouped_targets[0].kind != INKPOD_EDIT_TARGET_LAYER
        || state.Workspace().panes.active_tree_layer_id != grouped_active_layer
        || state.Workspace().panes.active_tree_plane_id != grouped_active_plane
        || !inkpod::windows::ui::IsCommandEnabled(
            state.Workspace().command_states, IDM_EDIT_COPY)) {
        return 784;
    }
    grouped_dispatch = {};
    grouped_outputs.clear();
    if (state.engine->ApplyEditTargetCommand(
            state.Document().id,
            state.Document().generation,
            make_editable,
            grouped_dispatch,
            grouped_outputs) != INKPOD_STATUS_OK) {
        return 7834;
    }
    InkpodDocumentInfo grouped_before_clipboard = EmptyDocumentInfo();
    InkpodDocumentInfo grouped_after_clipboard = EmptyDocumentInfo();
    if (!QueryDocument(state, grouped_before_clipboard)) {
        return 7871;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_COPY,
            0) != 1) {
        return 7872;
    }
    if (!inkpod::windows::ui::IsCommandEnabled(
            state.Workspace().command_states, IDM_EDIT_PASTE)) {
        return 7873;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_PASTE,
            0) != 1) {
        return 7874;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDIT_FLOATING_CANCEL,
            0) != 1) {
        return 7875;
    }
    if (!QueryDocument(state, grouped_after_clipboard)
        || grouped_after_clipboard.document_revision
            != grouped_before_clipboard.document_revision
        || grouped_after_clipboard.main_plane_checksum
            != grouped_before_clipboard.main_plane_checksum
        || grouped_after_clipboard.color_plane_checksum
            != grouped_before_clipboard.color_plane_checksum) {
        return 7876;
    }
    InkpodEditTargetCapabilities grouped_capabilities{};
    grouped_capabilities.struct_size = sizeof(grouped_capabilities);
    if (state.engine->GetEditTargetCapabilities(
            state.Document().id,
            state.Document().generation,
            grouped_capabilities) != INKPOD_STATUS_OK
        || grouped_capabilities.can_duplicate == 0U
        || !inkpod::windows::ui::IsCommandEnabled(
            state.Workspace().command_states, IDM_LAYER_DUPLICATE)) {
        return 788;
    }
    SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_LAYER_DUPLICATE,
        0);
    if (state.Workspace().panes.tree_layer_count != grouped_layer_count + 1U
        || state.engine->GetEditTargets(
               state.Document().id,
               state.Document().generation,
               grouped_targets) != INKPOD_STATUS_OK
        || grouped_targets.size() != 1U
        || grouped_targets[0].layer_id
            != state.Workspace().panes.active_tree_layer_id) {
        return 785;
    }

    if (CreateCell(state, 12U, 10U, 96000U) != INKPOD_STATUS_OK) {
        return 404;
    }
    try {
        state.lifetime.smoke_raster_path = L"inkpod-io2-smoke.png";
    } catch (const std::bad_alloc&) {
        return 405;
    }
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    std::vector<std::uint8_t> smoke_raster_source;
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILE_EXPORT_RASTER, 0) != 1
        || GetFileAttributesW(state.lifetime.smoke_raster_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || !ReadBoundedFile(state.lifetime.smoke_raster_path, smoke_raster_source)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILE_IMPORT_RASTER, 0) != 1) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 406;
    }
    InkpodDocumentInfo imported{};
    const bool import_ok = QueryDocument(state, imported) && imported.width == 12U
        && imported.height == 10U && imported.dpi_x_milli >= 95000U
        && imported.dpi_x_milli <= 97000U;
    RefreshLightTablePane(state);
    if (!import_ok) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 407;
    }
    InkpodDocumentInfo imported_without_source = EmptyDocumentInfo();
    if (DeleteFileW(state.lifetime.smoke_raster_path.c_str()) == FALSE
        || GetFileAttributesW(state.lifetime.smoke_raster_path.c_str()) != INVALID_FILE_ATTRIBUTES
        || !QueryDocument(state, imported_without_source)
        || imported_without_source.document_revision != imported.document_revision
        || imported_without_source.main_plane_checksum != imported.main_plane_checksum
        || imported_without_source.color_plane_checksum != imported.color_plane_checksum
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        state.lifetime.smoke_raster_path.clear();
        return 484;
    }
    std::vector<std::uint8_t> imported_roundtrip;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_FILE_EXPORT_RASTER,
            0) != 1
        || !ReadBoundedFile(state.lifetime.smoke_raster_path, imported_roundtrip)
        || imported_roundtrip != smoke_raster_source
        || DeleteFileW(state.lifetime.smoke_raster_path.c_str()) == FALSE
        || GetFileAttributesW(state.lifetime.smoke_raster_path.c_str())
            != INVALID_FILE_ATTRIBUTES) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 489;
    }
    const std::wstring asset_base_save = L"inkpod-asset-base-smoke.inkpod";
    const std::wstring asset_base_recovery = asset_base_save + L".recovery.inkpod";
    DeleteFileW(asset_base_save.c_str());
    DeleteFileW(asset_base_recovery.c_str());
    const std::size_t recent_before_asset_base_save =
        state.RecentDocumentCount();
    InkpodDocumentInfo after_asset_base_save = EmptyDocumentInfo();
    const InkpodStatus asset_base_save_status =
        SaveToPath(state, asset_base_save);
    if (asset_base_save_status != INKPOD_STATUS_OK) {
        DeleteFileW(asset_base_save.c_str());
        DeleteFileW(asset_base_recovery.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 1487;
    }
    if (GetFileAttributesW(asset_base_save.c_str()) == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(asset_base_recovery.c_str()) != INVALID_FILE_ATTRIBUTES) {
        DeleteFileW(asset_base_save.c_str());
        DeleteFileW(asset_base_recovery.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 1488;
    }
    if (!QueryDocument(state, after_asset_base_save)) {
        state.lifetime.smoke_raster_path.clear();
        return 1489;
    }
    if ((after_asset_base_save.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || after_asset_base_save.document_revision
            != imported_without_source.document_revision
        || after_asset_base_save.view_revision != imported_without_source.view_revision
        || after_asset_base_save.active_plane != imported_without_source.active_plane
        || !SamePersistentMetadata(imported_without_source, after_asset_base_save)) {
        state.lifetime.smoke_raster_path.clear();
        return 1490;
    }
    if (state.Document().shell.current_path != asset_base_save
        || !state.Document().shell.source_path.empty()
        || state.Document().shell.recovery_path.empty()
        || !state.Document().shell.recovery_original_path.empty()
        || state.RecentDocumentCount() != recent_before_asset_base_save + 1U) {
        state.lifetime.smoke_raster_path.clear();
        return 1491;
    }
    DeleteFileW(asset_base_save.c_str());
    if (CreateCell(state, 12U, 10U, 96000U) != INKPOD_STATUS_OK
        || !RefreshLightTablePane(state)
        || !WriteFileAtomically(state.lifetime.smoke_raster_path, smoke_raster_source)) {
        DeleteFileW(asset_base_save.c_str());
        DeleteFileW(asset_base_recovery.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 488;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_COLOR_EDITOR, 0) != 1
        || state.Workspace().tools.drawing_color.depth != INKPOD_COLOR_DEPTH_16) {
        return 430;
    }
    const std::array<UINT, 9U> color_commands{
        IDM_COLOR_SOURCE_TOPMOST,
        IDM_COLOR_SOURCE_SELECTED,
        IDM_COLOR_SOURCE_COMPOSITE,
        IDM_COLOR_SOURCE_LIGHT_TABLE,
        IDM_PALETTE_REGISTER,
        IDM_PALETTE_SAVE,
        IDM_PALETTE_CLEAR,
        IDM_PALETTE_LOAD,
        IDM_PALETTE_NEXT_GROUP};
    for (std::size_t index = 0U; index < color_commands.size(); ++index) {
        if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, color_commands[index], 0) != 1) {
            return 434 + static_cast<int>(index);
        }
    }
    const auto palette_before_generation =
        state.Workspace().panes.palette_colors;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CHART_GENERATE,
            0) != 1
        || state.Workspace().panes.color_chart_generation == nullptr) {
        return 443;
    }
    const auto superseded_generation =
        state.Workspace().panes.color_chart_generation;
    const std::uint64_t superseded_token = superseded_generation->token;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CHART_GENERATE,
            0) != 1
        || state.Workspace().panes.color_chart_generation == nullptr
        || state.Workspace().panes.color_chart_generation == superseded_generation
        || state.Workspace().panes.color_chart_generation->token <= superseded_token
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 443;
    }
    PumpPendingWindowMessages();
    const auto same_color = [](
                                const InkpodColorValue& left,
                                const InkpodColorValue& right) noexcept {
        return left.depth == right.depth && left.red == right.red
            && left.green == right.green && left.blue == right.blue
            && left.alpha == right.alpha;
    };
    if (state.Workspace().panes.color_chart_generation != nullptr
        || palette_before_generation.size()
            != state.Workspace().panes.palette_colors.size()
        || !std::equal(
            palette_before_generation.begin(),
            palette_before_generation.end(),
            state.Workspace().panes.palette_colors.begin(),
            same_color)) {
        return 444;
    }
    const DocumentViewId close_race_return_view = state.ActiveView().id;
    const std::size_t close_race_document_count = state.Documents().Count();
    InkpodDocumentInfo close_race_before{};
    if (!QueryDocument(state, close_race_before)
        || CreateDefaultCell(state) != INKPOD_STATUS_OK) {
        return 446;
    }
    const DocumentSessionId closing_generation_session = state.Document().id;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CHART_GENERATE,
            0) != 1
        || state.Workspace().panes.color_chart_generation == nullptr
        || !state.CloseDocumentSession(closing_generation_session)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 446;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo close_race_after{};
    if (state.Workspace().panes.color_chart_generation != nullptr
        || state.Documents().Count() != close_race_document_count
        || !ActivateDocumentTab(state, close_race_return_view)
        || !QueryDocument(state, close_race_after)
        || close_race_after.document_revision
            != close_race_before.document_revision
        || close_race_after.main_plane_checksum
            != close_race_before.main_plane_checksum
        || close_race_after.color_plane_checksum
            != close_race_before.color_plane_checksum
        || close_race_after.flags != close_race_before.flags) {
        return 447;
    }
    panes::ColorPanesController color_controller(*state.engine);
    if (color_controller.ReplaceColorChart(
            state.Document().id,
            state.Document().generation,
            {state.Workspace().tools.drawing_color},
            {L"Smoke"},
            false) != INKPOD_STATUS_OK) {
        return 445;
    }
    RefreshColorPanes(state);
    state.Workspace().panes.selected_color_chart_index = 0U;
    const std::array<UINT, 4U> chart_navigation_commands{
        IDM_CHART_RENAME, IDM_CHART_SEARCH, IDM_CHART_NEXT, IDM_CHART_NEXT_PAGE};
    for (std::size_t index = 0U; index < chart_navigation_commands.size(); ++index) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, chart_navigation_commands[index], 0) != 1) {
            return 450 + static_cast<int>(index);
        }
    }
    state.Workspace().panes.color_chart_page = 0U;
    state.Workspace().panes.selected_color_chart_index = 0U;
    RefreshColorPanes(state);
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_SAVE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_COPY, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_PASTE, 0) != 1) {
        return 432;
    }
    state.Workspace().panes.selected_color_chart_index = 0U;
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_CUT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_LOAD, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_LOCK, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_LOCK, 0) != 1) {
        return 433;
    }
    DeleteFileW(L"inkpod-palette-smoke.inkpalette");
    DeleteFileW(L"inkpod-chart-smoke.inkchart");
    const std::array<UINT, 14U> light_table_commands{
        IDM_LT_SET_NEW,
        IDM_LT_SET_RENAME,
        IDM_LT_SET_DUPLICATE,
        IDM_LT_SET_UP,
        IDM_LT_SET_DOWN,
        IDM_LT_SET_DELETE,
        IDM_LT_ITEM_ADD,
        IDM_LT_ITEM_ADD,
        IDM_LT_ITEM_DOWN,
        IDM_LT_ITEM_UP,
        IDM_LT_GLOBAL_OPACITY,
        IDM_LT_ITEM_SAMPLE,
        IDM_LT_ITEM_PROPERTIES,
        IDM_LT_ITEM_RELOAD};
    for (std::size_t index = 0U; index < light_table_commands.size(); ++index) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, light_table_commands[index], 0) != 1) {
            DeleteFileW(state.lifetime.smoke_raster_path.c_str());
            state.lifetime.smoke_raster_path.clear();
            return 414 + static_cast<int>(index);
        }
    }
    if (DeleteFileW(state.lifetime.smoke_raster_path.c_str()) == FALSE
        || GetFileAttributesW(state.lifetime.smoke_raster_path.c_str()) != INVALID_FILE_ATTRIBUTES
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LT_ITEM_SAMPLE,
               0) != 1
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1
        || !WriteFileAtomically(state.lifetime.smoke_raster_path, smoke_raster_source)) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 486;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_LIGHT_TABLE,
            0) != 1
        || !RefreshLightTablePane(state)
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::LightTable)
        || SendMessageW(
               GetDlgItem(
                   state.Workspace().light_table_palette,
                   IDC_LIGHT_TABLE_ITEMS),
               LB_GETCOUNT,
               0,
               0) != static_cast<LRESULT>(
                   state.Workspace().panes.light_table_item_count)) {
        return 882;
    }
    InkpodLightTableItemInfo light_item{};
    if (!QueryLightTableItem(state, state.Workspace().panes.active_light_table_item_index, light_item)
        || light_item.opacity_milli != 500U
        || light_item.effective_opacity_milli != 250U
        || light_item.translate_x_milli != 1000
        || light_item.translate_y_milli != -1000) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 408;
    }
    inkpod::renderer::CanvasDocumentBounds light_canvas{};
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LT_ITEM_MOVE, 0) != 1
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, light_canvas)) {
        return 470;
    }
    const InkpodStrokeSample light_move_start{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(light_canvas.left + 2.0),
        static_cast<float>(light_canvas.top + 2.0), 1.0F, 0U};
    const InkpodStrokeSample light_move_end{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(light_canvas.left + 12.0),
        static_cast<float>(light_canvas.top + 12.0), 1.0F, 0U};
    const inkpod::renderer::CanvasStrokeEvent light_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &light_move_start, 1U};
    const inkpod::renderer::CanvasStrokeEvent light_end{
        inkpod::renderer::CanvasStrokeEventKind::End, &light_move_end, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, light_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, light_end)
        || !QueryLightTableItem(state, state.Workspace().panes.active_light_table_item_index, light_item)
        || (light_item.translate_x_milli == 1000
            && light_item.translate_y_milli == -1000)) {
        return 471;
    }
    const std::wstring swap_save = L"inkpod-lt-smoke.inkpod";
    DeleteFileW(swap_save.c_str());
    InkpodDocumentInfo before_cancelled_swap = EmptyDocumentInfo();
    InkpodDocumentInfo after_cancelled_swap = EmptyDocumentInfo();
    if (!QueryDocument(state, before_cancelled_swap)
        || (before_cancelled_swap.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LT_ITEM_SWAP,
               0) != 0
        || !QueryDocument(state, after_cancelled_swap)
        || after_cancelled_swap.document_revision
            != before_cancelled_swap.document_revision) {
        DeleteFileW(swap_save.c_str());
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 885;
    }
    if (SaveToPath(state, swap_save) != INKPOD_STATUS_OK) {
        DeleteFileW(swap_save.c_str());
        DeleteFileW((swap_save + L".recovery.inkpod").c_str());
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 4091;
    }
    InkpodDocumentInfo after_swap_save = EmptyDocumentInfo();
    if (!QueryDocument(state, after_swap_save)
        || (after_swap_save.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        DeleteFileW(swap_save.c_str());
        DeleteFileW((swap_save + L".recovery.inkpod").c_str());
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 4092;
    }
    const std::uint32_t swap_prompt_count =
        state.lifetime.smoke_dirty_prompt_count;
    state.lifetime.smoke_dirty_prompt_choice = IDOK;
    const LRESULT confirmed_swap = SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_LT_ITEM_SWAP,
        0);
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    if (confirmed_swap != 1) {
        DeleteFileW(swap_save.c_str());
        DeleteFileW((swap_save + L".recovery.inkpod").c_str());
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 4093;
    }
    if (state.lifetime.smoke_dirty_prompt_count != swap_prompt_count) {
        DeleteFileW(swap_save.c_str());
        DeleteFileW((swap_save + L".recovery.inkpod").c_str());
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 4094;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_LIGHT_TABLE,
            0) != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::LightTable)) {
        return 883;
    }
    DeleteFileW(swap_save.c_str());
    DeleteFileW((swap_save + L".recovery.inkpod").c_str());
    std::vector<std::uint8_t> sequence_source;
    if (!ReadBoundedFile(state.lifetime.smoke_raster_path, sequence_source)) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 410;
    }
    try {
        state.lifetime.smoke_sequence_paths = {L"cell10.png", L"cell1.png", L"cell3.png"};
    } catch (const std::bad_alloc&) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 410;
    }
    for (const auto& path : state.lifetime.smoke_sequence_paths) {
        DeleteFileW(path.c_str());
        if (!WriteFileAtomically(path, sequence_source)) {
            return 410;
        }
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_IMPORT, 0) != 1) {
        return 411;
    }
    RefreshSequencePane(state);
    const HWND sequence_cells = GetDlgItem(
        state.Workspace().sequence_palette, IDC_SEQUENCE_CELLS);
    const auto& sequence_view = state.Workspace().sequence_dialog.view;
    inkpod::app::PaneResourceUsage sequence_resources{};
    std::vector<inkpod::app::RecentDocumentEntry> recent_before_sequence_navigation;
    try {
        recent_before_sequence_navigation.reserve(state.RecentDocumentCount());
        for (std::size_t index = 0U; index < state.RecentDocumentCount(); ++index) {
            recent_before_sequence_navigation.push_back(
                *state.RecentDocumentAt(index));
        }
    } catch (const std::bad_alloc&) {
        return 874;
    }
    if (state.Workspace().panes.sequence_count != 3U
        || sequence_cells == nullptr
        || SendMessageW(sequence_cells, LB_GETCOUNT, 0, 0) != 3
        || sequence_view.cells.size() != 3U
        || sequence_view.cells[0].name != L"cell1.png"
        || sequence_view.cells[1].name != L"cell3.png"
        || sequence_view.cells[2].name != L"cell10.png") {
        return 1504;
    }
    if (!inkpod::windows::ui::panes::SequencePaneItemHasThumbnail(
            state.Workspace().sequence_palette, 0U)
        || sequence_view.cells[0].thumbnail_width == 0U
        || sequence_view.cells[0].thumbnail_height == 0U
        || sequence_view.cells[0].thumbnail_stride_bytes
            != sequence_view.cells[0].thumbnail_width * 4U) {
        return 1505;
    }
    if (!state.GetPaneResourceUsage(
            state.Workspace().pane_ids.sequence, sequence_resources)) {
        return 1506;
    }
    if (sequence_resources.workspace != state.Workspace().id) {
        return 1508;
    }
    if (sequence_resources.thumbnail_bytes == 0U) {
        return 1509;
    }
    if (sequence_resources.cached_item_count != 3U) {
        return 1510;
    }
    const InkpodStatus sequence_save_status = SaveToPath(state, swap_save);
    if (sequence_save_status != INKPOD_STATUS_OK) {
        return 1511;
    }
    InkpodDocumentInfo sequence_saved = EmptyDocumentInfo();
    if (!QueryDocument(state, sequence_saved)
        || (sequence_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        return 1507;
    }
    const std::uint32_t sequence_prompt_count =
        state.lifetime.smoke_dirty_prompt_count;
    state.lifetime.smoke_dirty_prompt_choice = IDOK;
    SendMessageW(sequence_cells, LB_SETCURSEL, 2U, 0);
    SendMessageW(
        state.Workspace().sequence_palette,
        WM_COMMAND,
        MAKEWPARAM(IDC_SEQUENCE_CELLS, LBN_SELCHANGE),
        reinterpret_cast<LPARAM>(sequence_cells));
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    InkpodDocumentInfo selected_ten = EmptyDocumentInfo();
    if (!QueryDocument(state, selected_ten)
        || state.Workspace().sequence_dialog.view.active_index != 2U
        || state.lifetime.smoke_dirty_prompt_count
            != sequence_prompt_count) {
        return 875;
    }
    SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_SELECTION_ALL,
        0);
    InkpodDocumentInfo dirty_ten = EmptyDocumentInfo();
    if (!QueryDocument(state, dirty_ten)
        || (dirty_ten.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        return 877;
    }
    SendMessageW(sequence_cells, LB_SETCURSEL, 0U, 0);
    SendMessageW(
        state.Workspace().sequence_palette,
        WM_COMMAND,
        MAKEWPARAM(IDC_SEQUENCE_CELLS, LBN_SELCHANGE),
        reinterpret_cast<LPARAM>(sequence_cells));
    InkpodDocumentInfo after_cancelled_switch = EmptyDocumentInfo();
    if (!QueryDocument(state, after_cancelled_switch)
        || after_cancelled_switch.document_uuid_high
            != dirty_ten.document_uuid_high
        || after_cancelled_switch.document_uuid_low != dirty_ten.document_uuid_low
        || state.Workspace().sequence_dialog.view.active_index != 2U) {
        return 878;
    }
    if (state.engine->Invoke(
            [](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_undo(core, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 879;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDOK;
    SendMessageW(sequence_cells, LB_SETCURSEL, 0U, 0);
    SendMessageW(
        state.Workspace().sequence_palette,
        WM_COMMAND,
        MAKEWPARAM(IDC_SEQUENCE_CELLS, LBN_SELCHANGE),
        reinterpret_cast<LPARAM>(sequence_cells));
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    InkpodDocumentInfo selected_one = EmptyDocumentInfo();
    if (!QueryDocument(state, selected_one)
        || selected_one.document_uuid_high == selected_ten.document_uuid_high
            && selected_one.document_uuid_low == selected_ten.document_uuid_low
        || state.Workspace().sequence_dialog.view.active_index != 0U) {
        return 880;
    }
    if (!RefreshLightTablePane(state)) {
        return 4101;
    }
    const std::uint64_t bulk_cancel_revision = selected_one.document_revision;
    state.lifetime.smoke_dirty_prompt_choice = IDCANCEL;
    const LRESULT cancelled_bulk = SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_LT_BULK_BOTH,
        0);
    state.lifetime.smoke_dirty_prompt_choice = IDOK;
    InkpodDocumentInfo after_cancelled_bulk = EmptyDocumentInfo();
    if (cancelled_bulk != 0
        || !QueryDocument(state, after_cancelled_bulk)
        || after_cancelled_bulk.document_revision != bulk_cancel_revision
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LT_BULK_BOTH,
               0) != 1) {
        state.lifetime.smoke_dirty_prompt_choice = IDNO;
        return 4102;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    InkpodDocumentInfo after_bulk = EmptyDocumentInfo();
    InkpodLightTableItemInfo bulk_item{};
    if (!QueryDocument(state, after_bulk)
        || after_bulk.document_revision != bulk_cancel_revision + 1U
        || !QueryLightTableItem(state, 0U, bulk_item)
        || bulk_item.opacity_milli != 800U
        || state.Workspace().panes.light_table_item_count != 1U) {
        return 4103;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDOK;
    const LRESULT duplicate_bulk = SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_LT_BULK_BOTH,
        0);
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    InkpodDocumentInfo after_duplicate_bulk = EmptyDocumentInfo();
    if (duplicate_bulk != 1 || !QueryDocument(state, after_duplicate_bulk)) {
        return 4104;
    }
    if (after_duplicate_bulk.document_revision != after_bulk.document_revision) {
        return 4105;
    }
    SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_EDIT_UNDO,
        0);
    if (QueryLightTableItem(state, 0U, bulk_item)) {
        return 4107;
    }
    SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_EDIT_REDO,
        0);
    if (!QueryLightTableItem(state, 0U, bulk_item)
        || bulk_item.opacity_milli != 800U) {
        return 4109;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_SUBPALETTE_SET,
            0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_SUBPALETTE_SAMPLE,
               0) != 1) {
        return 412;
    }
    const std::uint32_t navigation_prompt_count =
        state.lifetime.smoke_dirty_prompt_count;
    state.lifetime.smoke_dirty_prompt_choice = IDOK;
    const bool navigation_ok =
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_GOTO, 0) == 1
        && SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_SEQ_PREVIOUS,
               0) == 1
        && SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_SEQ_NEXT,
               0) == 1;
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    if (!navigation_ok
        || state.lifetime.smoke_dirty_prompt_count
            != navigation_prompt_count + 3U) {
        return 412;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_START, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_NEXT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_PAUSE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_PAUSE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_PREVIOUS, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FIRST, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_LAST, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_30, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_25, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_24, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_12, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_10, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_8, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_STOP, 0) != 1) {
        return 412;
    }
    std::wstring active_cell_tab;
    if (!ReadDocumentTabLabel(
            state.Workspace().windows.document_tabs,
            TabCtrl_GetCurSel(state.Workspace().windows.document_tabs),
            active_cell_tab)
        || active_cell_tab != L"cell3.png *") {
        return 718;
    }
    InkpodDocumentInfo autosave_source = EmptyDocumentInfo();
    InkpodEditorStateInfo autosave_source_editor{};
    autosave_source_editor.struct_size = sizeof(autosave_source_editor);
    InkpodSequenceSwitchRequest autosave_request{};
    autosave_request.struct_size = sizeof(autosave_request);
    if (!QueryDocument(state, autosave_source)
        || (autosave_source.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || state.engine->GetEditorState(
               state.Document().id,
               state.Document().generation,
               autosave_source_editor)
            == false
        || state.engine->Invoke(
               [&autosave_request](InkpodCore* core) {
                   return inkpod_core_sequence_switch_request(
                       core,
                       2U,
                       INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                       &autosave_request);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (autosave_request.flags & INKPOD_SEQUENCE_SWITCH_REQUIRED) == 0U) {
        return 4095;
    }
    const SequenceCellSwitchPolicy previous_sequence_policy =
        state.lifetime.sequence_switch_policy;
    const auto restore_sequence_policy = [&state, previous_sequence_policy]() {
        state.lifetime.sequence_switch_policy = previous_sequence_policy;
        UpdateMenuState(state);
    };
    state.lifetime.sequence_switch_policy =
        SequenceCellSwitchPolicy::AutosaveBeforeSwitch;
    UpdateMenuState(state);
    const std::uint32_t autosave_prompt_count =
        state.lifetime.smoke_dirty_prompt_count;
    const std::uint32_t autosave_completion_count =
        state.Workspace().animation.smoke_sequence_switch_completed;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_SEQ_NEXT,
            0) != 1
        || !state.Workspace().animation.sequence_switch_pending
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_SEQ_NEXT,
               0) != 0
        || state.engine->Invoke(
               [](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false)
            != INKPOD_STATUS_OK) {
        restore_sequence_policy();
        return 4096;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo autosave_target = EmptyDocumentInfo();
    const auto* source_binding = state.Document().FindSequenceAutosave(
        autosave_request.source_document_uuid_high,
        autosave_request.source_document_uuid_low,
        autosave_request.source_generation);
    std::wstring source_metadata_path;
    if (state.Workspace().animation.sequence_switch_pending
        || state.Workspace().animation.smoke_sequence_switch_completed
            != autosave_completion_count + 1U
        || state.Workspace().animation.smoke_sequence_switch_status
            != INKPOD_STATUS_OK
        || state.lifetime.smoke_dirty_prompt_count != autosave_prompt_count
        || !QueryDocument(state, autosave_target)
        || autosave_target.document_uuid_high
            != autosave_request.target_document_uuid_high
        || autosave_target.document_uuid_low
            != autosave_request.target_document_uuid_low
        || state.Workspace().sequence_dialog.view.active_index != 2U
        || source_binding == nullptr
        || source_binding->artifact_generation != 1U
        || GetFileAttributesW(source_binding->recovery_path.c_str())
            == INVALID_FILE_ATTRIBUTES
        || !RecoveryMetadataPath(
            source_binding->recovery_path, source_metadata_path)
        || GetFileAttributesW(source_metadata_path.c_str())
            == INVALID_FILE_ATTRIBUTES
        || !state.Document().shell.current_path.empty()) {
        restore_sequence_policy();
        return 4097;
    }
    const std::wstring source_recovery_path = source_binding->recovery_path;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_SEQ_PREVIOUS,
            0) != 1
        || !state.Workspace().animation.sequence_switch_pending
        || state.engine->Invoke(
               [](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false)
            != INKPOD_STATUS_OK) {
        restore_sequence_policy();
        return 4098;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo autosave_restored = EmptyDocumentInfo();
    InkpodEditorStateInfo autosave_restored_editor{};
    autosave_restored_editor.struct_size = sizeof(autosave_restored_editor);
    if (state.Workspace().animation.sequence_switch_pending
        || state.Workspace().animation.smoke_sequence_switch_completed
            != autosave_completion_count + 2U
        || state.Workspace().animation.smoke_sequence_switch_status
            != INKPOD_STATUS_OK
        || state.lifetime.smoke_dirty_prompt_count != autosave_prompt_count
        || !QueryDocument(state, autosave_restored)
        || state.engine->GetEditorState(
               state.Document().id,
               state.Document().generation,
               autosave_restored_editor)
            == false
        || autosave_restored.document_uuid_high
            != autosave_source.document_uuid_high
        || autosave_restored.document_uuid_low
            != autosave_source.document_uuid_low
        || autosave_restored.main_plane_checksum
            != autosave_source.main_plane_checksum
        || autosave_restored.color_plane_checksum
            != autosave_source.color_plane_checksum
        || (autosave_restored.flags
            & (INKPOD_DOCUMENT_FLAG_DIRTY | INKPOD_DOCUMENT_FLAG_RECOVERED))
            != (INKPOD_DOCUMENT_FLAG_DIRTY | INKPOD_DOCUMENT_FLAG_RECOVERED)
        || autosave_restored_editor.editor_revision
            != autosave_source_editor.editor_revision
        || std::memcmp(
               autosave_restored_editor.editor_digest,
               autosave_source_editor.editor_digest,
               sizeof(autosave_restored_editor.editor_digest)) != 0
        || (autosave_restored_editor.flags & INKPOD_EDITOR_STATE_DIRTY) == 0U
        || state.Workspace().sequence_dialog.view.active_index != 1U
        || state.Document().shell.current_path.empty() == false
        || state.Document().shell.recovery_path != source_recovery_path) {
        restore_sequence_policy();
        return 4099;
    }
    restore_sequence_policy();
    const inkpod::app::DocumentSessionId sequence_session = state.Document().id;
    const inkpod::app::DocumentViewId sequence_view_id =
        state.Document().ActiveView()->id;
    if (ImportCommonRasterFromPath(state, L"cell3.png") != INKPOD_STATUS_OK) {
        return 881;
    }
    if (state.Document().id == sequence_session) {
        return 883;
    }
    if (state.Workspace().sequence_dialog.view.cells.size() != 3U) {
        return 884;
    }
    if (state.Workspace().sequence_dialog.view.active_index != 1U) {
        return 885;
    }
    if (state.Workspace().sequence_dialog.view.cells[0].name != L"cell1.png"
        || state.Workspace().sequence_dialog.view.cells[1].name != L"cell3.png"
        || state.Workspace().sequence_dialog.view.cells[2].name != L"cell10.png") {
        return 886;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_DOCUMENT_CLOSE,
            0) != 1) {
        return 882;
    }
    if (state.Document().id != sequence_session
        && !ActivateDocumentTab(state, sequence_view_id)) {
        return 887;
    }
    if (state.Document().id != sequence_session) {
        return 887;
    }
    while (state.RecentDocumentCount() != 0U) {
        if (!state.RemoveRecentDocument(0U)) {
            return 899;
        }
    }
    for (auto entry = recent_before_sequence_navigation.rbegin();
         entry != recent_before_sequence_navigation.rend();
         ++entry) {
        if (!state.RecordRecentDocument(entry->path, entry->identity)) {
            return 899;
        }
    }
    if (!RefreshSequencePane(state)) {
        return 888;
    }
    if (state.Workspace().sequence_dialog.view.cells.size() != 3U) {
        return 889;
    }
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    state.lifetime.smoke_raster_path = L"inkpod-sequence-export.png";
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_EXPORT, 0) != 1
        || GetFileAttributesW(L"inkpod-sequence-export-cell1.png")
            == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(L"inkpod-sequence-export-cell3.png")
            == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(L"inkpod-sequence-export-cell10.png")
            == INVALID_FILE_ATTRIBUTES) {
        return 413;
    }
    for (const auto& path : state.lifetime.smoke_sequence_paths) {
        DeleteFileW(path.c_str());
    }
    DeleteFileW(swap_save.c_str());
    DeleteFileW((swap_save + L".recovery.inkpod").c_str());
    state.lifetime.smoke_sequence_paths.clear();
    DeleteFileW(L"inkpod-sequence-export-cell1.png");
    DeleteFileW(L"inkpod-sequence-export-cell3.png");
    DeleteFileW(L"inkpod-sequence-export-cell10.png");
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    state.lifetime.smoke_raster_path.clear();
    return 0;
}

int RunVectorWorkflowSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 500;
    }
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d35000000000001),
        UINT64_C(0x4d35000000000002),
        64U,
        64U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &options, &info);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 501;
    }
    ResetUiForNewActiveDocument(state);
    if (!state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)
        || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 501;
    }

    InkpodSnapshotVectorSegment geometry_before{};
    std::uint64_t vector_layer_id{};
    std::uint64_t vector_trace_plane_id{};
    std::uint64_t vector_path_id{};
    std::uint64_t vector_fill_id{};
    const InkpodStatus vector_status = state.engine->Invoke(
        [&geometry_before,
         &vector_layer_id,
         &vector_trace_plane_id,
         &vector_path_id,
         &vector_fill_id](InkpodCore* core) {
            static constexpr std::array<std::uint8_t, 6U> name{
                'V', 'e', 'c', 't', 'o', 'r'};
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CREATE_LAYER;
            edit.kind = INKPOD_LAYER_VECTOR_COLORING;
            edit.name_utf8 = name.data();
            edit.name_bytes = name.size();
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            InkpodStatus status = inkpod_core_tree_edit(
                core, &edit, &result, &vector_layer_id);
            InkpodNodeInfo trace{};
            trace.struct_size = sizeof(trace);
            InkpodNodeInfo fill{};
            fill.struct_size = sizeof(fill);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 1U, &trace);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 2U, &fill);
            }
            if (status != INKPOD_STATUS_OK || vector_layer_id == 0U
                || trace.kind != INKPOD_TYPED_PLANE_COLOR_TRACE
                || fill.kind != INKPOD_TYPED_PLANE_VECTOR_FILL) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE
                    : status;
            }
            vector_trace_plane_id = trace.id;
            constexpr auto point = [](float x, float y) noexcept {
                return InkpodVectorPoint{x, y};
            };
            constexpr auto line = [](InkpodVectorPoint start, InkpodVectorPoint end) noexcept {
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
                    5.0F};
            };
            constexpr std::array<InkpodVectorPoint, 5U> corners{
                point(8.0F, 8.0F),
                point(56.0F, 8.0F),
                point(56.0F, 56.0F),
                point(8.0F, 56.0F),
                point(8.0F, 8.0F)};
            const std::array<InkpodVectorCubicSegment, 4U> segments{
                line(corners[0], corners[1]),
                line(corners[1], corners[2]),
                line(corners[2], corners[3]),
                line(corners[3], corners[4])};
            const InkpodVectorPathInput path{
                sizeof(InkpodVectorPathInput),
                0U,
                INKPOD_VECTOR_PATH_CLOSED,
                trace.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    20U,
                    40U,
                    220U,
                    255U},
                segments.data(),
                segments.size(),
                sizeof(InkpodVectorCubicSegment)};
            status = inkpod_core_vector_add_path(
                core, &path, &result, &vector_path_id);
            const InkpodVectorFillInput topology{
                sizeof(InkpodVectorFillInput),
                0U,
                0U,
                fill.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    240U,
                    120U,
                    20U,
                    180U},
                &vector_path_id,
                1U};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_vector_add_fill(
                    core, &topology, &result, &vector_fill_id);
            }
            const InkpodSnapshotOptions snapshot_options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot(
                    core, &snapshot_options, &snapshot);
            }
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK
                && (vectors.segment_count != 4U || vectors.fill_count != 1U
                    || vectors.boundary_path_count != 1U || vectors.segments == nullptr
                    || vectors.fills == nullptr || vectors.boundary_path_ids == nullptr
                    || vectors.segments->path_id != vector_path_id
                    || vectors.fills->fill_id != vector_fill_id
                    || *vectors.boundary_path_ids != vector_path_id)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status == INKPOD_STATUS_OK) {
                geometry_before = *vectors.segments;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        true,
        true);
    if (vector_status != INKPOD_STATUS_OK || vector_layer_id == 0U
        || vector_trace_plane_id == 0U || vector_path_id == 0U || vector_fill_id == 0U) {
        return 502;
    }
    if (ApplyView(state, INKPOD_VIEW_ZOOM_AT, 2.0, 32.0, 32.0)
        != INKPOD_STATUS_OK) {
        return 503;
    }
    bool geometry_unchanged{};
    const InkpodStatus zoom_status = state.engine->Invoke(
        [&geometry_before, &geometry_unchanged](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK && vectors.segment_count != 0U
                && vectors.segments != nullptr) {
                const InkpodSnapshotVectorSegment& after = *vectors.segments;
                geometry_unchanged = after.path_id == geometry_before.path_id
                    && after.p0.x == geometry_before.p0.x
                    && after.p0.y == geometry_before.p0.y
                    && after.p1.x == geometry_before.p1.x
                    && after.p1.y == geometry_before.p1.y
                    && after.p2.x == geometry_before.p2.x
                    && after.p2.y == geometry_before.p2.y
                    && after.p3.x == geometry_before.p3.x
                    && after.p3.y == geometry_before.p3.y
                    && after.width_start == geometry_before.width_start
                    && after.width_end == geometry_before.width_end;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (zoom_status != INKPOD_STATUS_OK || !geometry_unchanged) {
        return 504;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            inkpod::renderer::kCanvasRenderOnce,
            0,
            0) != 1) {
        return 505;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            inkpod::renderer::kCanvasValidateClosedVectorStroke,
            0,
            0) != 1) {
        return 507;
    }

    std::uint64_t diagnostic_path_id{};
    const InkpodStatus diagnostic_path_status = state.engine->Invoke(
        [vector_trace_plane_id, &diagnostic_path_id](InkpodCore* core) {
            const InkpodVectorCubicSegment segment{
                sizeof(InkpodVectorCubicSegment),
                0U,
                InkpodVectorPoint{12.0F, 24.0F},
                InkpodVectorPoint{20.0F, 26.6667F},
                InkpodVectorPoint{28.0F, 29.3333F},
                InkpodVectorPoint{36.0F, 32.0F},
                3.0F,
                3.0F};
            const InkpodVectorPathInput path{
                sizeof(InkpodVectorPathInput),
                0U,
                0U,
                vector_trace_plane_id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    20U,
                    40U,
                    220U,
                    255U},
                &segment,
                1U,
                sizeof(InkpodVectorCubicSegment)};
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_vector_add_path(
                core, &path, &result, &diagnostic_path_id);
        },
        true,
        true);
    InkpodDocumentInfo diagnostics_before = EmptyDocumentInfo();
    InkpodHistoryInfo diagnostics_history_before{};
    diagnostics_history_before.struct_size = sizeof(diagnostics_history_before);
    if (diagnostic_path_status != INKPOD_STATUS_OK || diagnostic_path_id == 0U
        || !QueryDocument(state, diagnostics_before)
        || state.engine->Invoke(
               [&diagnostics_history_before](InkpodCore* core) {
                   return inkpod_core_history_info(core, &diagnostics_history_before);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        return 530;
    }
    const inkpod::app::EditorGroup* diagnostic_group =
        state.Workspace().editors.Active();
    inkpod::renderer::CanvasDocumentBounds diagnostic_bounds{};
    if (diagnostic_group == nullptr || state.renderer == nullptr
        || !state.renderer->WaitQueueIdleForSmokeTest()
        || SendMessageW(
               diagnostic_group->canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1
        || !inkpod::renderer::GetCanvasDocumentBounds(
            diagnostic_group->canvas, diagnostic_bounds)) {
        return 535;
    }
    const double diagnostic_zoom = (diagnostic_bounds.right - diagnostic_bounds.left)
        / static_cast<double>(diagnostics_before.width);
    const auto device_coordinate = [diagnostic_zoom](double origin, double document) {
        return static_cast<UINT>(std::max(
            0L, std::lround(origin + document * diagnostic_zoom)));
    };
    const UINT antialias_x = device_coordinate(diagnostic_bounds.left, 23.526F);
    const UINT antialias_y = device_coordinate(diagnostic_bounds.top, 29.423F);
    const auto read_antialias_samples = [diagnostic_group, &state, antialias_x, antialias_y](
                                           std::array<inkpod::renderer::CanvasPixelRgba8, 3U>&
                                               samples) {
        for (std::size_t index = 0U; index < samples.size(); ++index) {
            const int offset = static_cast<int>(index) - 1;
            if (FAILED(state.renderer->ReadPixelForSmokeTest(
                    diagnostic_group->canvas_id,
                    diagnostic_group->generation,
                    antialias_x,
                    static_cast<UINT>(static_cast<int>(antialias_y) + offset),
                    samples[index]))) {
                return false;
            }
        }
        return true;
    };
    std::array<inkpod::renderer::CanvasPixelRgba8, 3U> antialias_on{};
    if (!read_antialias_samples(antialias_on)) {
        return 536;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_ANTIALIAS, 0);
    std::array<inkpod::renderer::CanvasPixelRgba8, 3U> antialias_off{};
    if (!state.renderer->WaitQueueIdleForSmokeTest()
        || SendMessageW(
               diagnostic_group->canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1
        || !read_antialias_samples(antialias_off)) {
        return 536;
    }
    const bool antialias_pixels_identical = std::equal(
        antialias_on.begin(),
        antialias_on.end(),
        antialias_off.begin(),
        [](const auto& left, const auto& right) {
            return left.red == right.red && left.green == right.green
                && left.blue == right.blue && left.alpha == right.alpha;
        });
    if (antialias_pixels_identical) {
        return 536;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_CENTERLINE, 0);
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_CENTERLINE_ONLY, 0);
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_ENDPOINTS, 0);
    UpdateMenuState(state);
    const auto checked = [&state](UINT command) {
        const UINT menu_state = GetMenuState(
            GetMenu(state.Workspace().windows.window), command, MF_BYCOMMAND);
        return menu_state != static_cast<UINT>(-1)
            && (menu_state & MF_CHECKED) != 0U;
    };
    if (state.ActiveView().presentation.vector_antialias
        || state.ActiveView().presentation.vector_centerline_mode
            != INKPOD_VECTOR_CENTERLINE_ONLY
        || !state.ActiveView().presentation.vector_endpoints_visible
        || checked(IDM_VIEW_VECTOR_ANTIALIAS)
        || !checked(IDM_VIEW_VECTOR_CENTERLINE)
        || !checked(IDM_VIEW_VECTOR_CENTERLINE_ONLY)
        || !checked(IDM_VIEW_VECTOR_ENDPOINTS)) {
        return 531;
    }

    InkpodDocumentInfo diagnostics_after = EmptyDocumentInfo();
    InkpodHistoryInfo diagnostics_history_after{};
    diagnostics_history_after.struct_size = sizeof(diagnostics_history_after);
    bool diagnostics_snapshot_valid{};
    const std::uint64_t diagnostic_view_id =
        state.ActiveView().presentation.active_view_id;
    const InkpodStatus diagnostics_status = state.engine->Invoke(
        [diagnostic_view_id,
         diagnostic_path_id,
         &diagnostics_history_after,
         &diagnostics_snapshot_valid](InkpodCore* core) {
            InkpodStatus status = inkpod_core_history_info(
                core, &diagnostics_history_after);
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            if (status == INKPOD_STATUS_OK) {
                status = diagnostic_view_id == 0U
                    ? inkpod_core_build_snapshot(core, &options, &snapshot)
                    : inkpod_core_build_snapshot_for_view(
                          core, diagnostic_view_id, &options, &snapshot);
            }
            InkpodSnapshotVectorDiagnostics diagnostics{};
            diagnostics.struct_size = sizeof(diagnostics);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vector_diagnostics(
                    snapshot, &diagnostics);
            }
            if (status == INKPOD_STATUS_OK) {
                diagnostics_snapshot_valid = diagnostics.flags
                        == (INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_VISIBLE
                            | INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_ONLY
                            | INKPOD_VECTOR_DIAGNOSTIC_ENDPOINTS_VISIBLE)
                    && diagnostics.endpoint_count == 2U
                    && diagnostics.endpoints != nullptr
                    && diagnostics.endpoints[0].path_id == diagnostic_path_id
                    && diagnostics.endpoints[0].endpoint
                        == INKPOD_VECTOR_ENDPOINT_START
                    && diagnostics.endpoints[1].path_id == diagnostic_path_id
                    && diagnostics.endpoints[1].endpoint
                        == INKPOD_VECTOR_ENDPOINT_END;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (diagnostics_status != INKPOD_STATUS_OK || !diagnostics_snapshot_valid
        || !QueryDocument(state, diagnostics_after)
        || diagnostics_after.document_revision != diagnostics_before.document_revision
        || diagnostics_after.flags != diagnostics_before.flags
        || diagnostics_after.main_plane_checksum != diagnostics_before.main_plane_checksum
        || diagnostics_after.color_plane_checksum != diagnostics_before.color_plane_checksum
        || diagnostics_after.view_revision <= diagnostics_before.view_revision
        || diagnostics_history_after.cursor != diagnostics_history_before.cursor
        || diagnostics_history_after.item_count != diagnostics_history_before.item_count) {
        return 532;
    }

    if (!state.renderer->WaitQueueIdleForSmokeTest()
        || SendMessageW(
               diagnostic_group->canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1
        || !inkpod::renderer::GetCanvasDocumentBounds(
            diagnostic_group->canvas, diagnostic_bounds)) {
        return 535;
    }
    const auto diagnostic_color_present = [diagnostic_group, &state](
                                              UINT center_x,
                                              UINT center_y,
                                              bool endpoint) {
        constexpr std::array<int, 5U> offsets{0, -1, 1, -2, 2};
        for (const int y : offsets) {
            for (const int x : offsets) {
                if ((x < 0 && center_x < static_cast<UINT>(-x))
                    || (y < 0 && center_y < static_cast<UINT>(-y))) {
                    continue;
                }
                inkpod::renderer::CanvasPixelRgba8 pixel{};
                if (FAILED(state.renderer->ReadPixelForSmokeTest(
                        diagnostic_group->canvas_id,
                        diagnostic_group->generation,
                        static_cast<UINT>(static_cast<int>(center_x) + x),
                        static_cast<UINT>(static_cast<int>(center_y) + y),
                        pixel))) {
                    return false;
                }
                if (pixel.red >= 180U && pixel.green <= 120U
                    && (endpoint ? pixel.blue <= 100U : pixel.blue >= 80U)) {
                    return true;
                }
            }
        }
        return false;
    };
    if (!diagnostic_color_present(
            device_coordinate(diagnostic_bounds.left, 32.0),
            device_coordinate(diagnostic_bounds.top, 8.0),
            false)) {
        return 537;
    }
    if (!diagnostic_color_present(
            device_coordinate(diagnostic_bounds.left, 12.0),
            device_coordinate(diagnostic_bounds.top, 24.0),
            true)) {
        return 538;
    }
    if (SendMessageW(
               diagnostic_group->canvas,
               inkpod::renderer::kCanvasSimulateDeviceLoss,
               0,
               0) != 1
        || SendMessageW(
               diagnostic_group->canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        return 539;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_CENTERLINE_ONLY, 0);
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_CENTERLINE, 0);
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_ENDPOINTS, 0);
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_VECTOR_ANTIALIAS, 0);

    state.Workspace().panes.active_tree_layer_id = vector_layer_id;
    state.Workspace().panes.active_tree_plane_id = vector_trace_plane_id;
    if (!RefreshTreePane(state)) {
        return 506;
    }
    UpdateMenuState(state);
    for (const UINT command : {
             IDM_VECTOR_LINE, IDM_VECTOR_CURVE, IDM_VECTOR_RECTANGLE,
             IDM_VECTOR_ELLIPSE, IDM_VECTOR_POLYGON, IDM_VECTOR_POLYLINE,
             IDM_VECTOR_ERASER}) {
        const UINT command_state = GetMenuState(GetMenu(state.Workspace().windows.window), command, MF_BYCOMMAND);
        if (command_state == static_cast<UINT>(-1)
            || (command_state & (MF_DISABLED | MF_GRAYED)) != 0U) {
            return 508;
        }
    }
    inkpod::renderer::CanvasDocumentBounds canvas_bounds{};
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, canvas_bounds)) {
        return 506;
    }
    const double canvas_zoom = (canvas_bounds.right - canvas_bounds.left)
        / static_cast<double>(document.width);
    const auto sample = [&](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(canvas_bounds.left + static_cast<double>(x) * canvas_zoom),
            static_cast<float>(canvas_bounds.top + static_cast<double>(y) * canvas_zoom),
            1.0F,
            0U};
    };
    const auto gesture = [&](UINT command, const std::array<InkpodStrokeSample, 4U>& samples) {
        if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0) != 1) {
            return false;
        }
        const inkpod::renderer::CanvasStrokeEvent begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
        const inkpod::renderer::CanvasStrokeEvent append{
            inkpod::renderer::CanvasStrokeEventKind::Append, samples.data() + 1U, 2U};
        const inkpod::renderer::CanvasStrokeEvent end{
            inkpod::renderer::CanvasStrokeEventKind::End, samples.data() + 3U, 1U};
        return inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, begin)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, append)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, end);
    };
    const auto curve_gesture = [&](const std::array<InkpodStrokeSample, 4U>& samples) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_CURVE, 0) != 1) {
            return false;
        }
        const inkpod::renderer::CanvasStrokeEvent begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
        const inkpod::renderer::CanvasStrokeEvent end{
            inkpod::renderer::CanvasStrokeEventKind::End, samples.data() + 3U, 1U};
        const inkpod::renderer::CanvasStrokeEvent control_begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data() + 1U, 1U};
        const inkpod::renderer::CanvasStrokeEvent control_end{
            inkpod::renderer::CanvasStrokeEventKind::End, samples.data() + 1U, 1U};
        return inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, begin)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, end)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, control_begin)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, control_end);
    };
    const auto polyline_gesture = [&](const std::array<InkpodStrokeSample, 4U>& samples) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_POLYLINE, 0) != 1) {
            return false;
        }
        const auto click = [&](const InkpodStrokeSample& sample) {
            const inkpod::renderer::CanvasStrokeEvent begin{
                inkpod::renderer::CanvasStrokeEventKind::Begin, &sample, 1U};
            const inkpod::renderer::CanvasStrokeEvent end{
                inkpod::renderer::CanvasStrokeEventKind::End, &sample, 1U};
            return inkpod::renderer::SubmitCanvasStrokeEvent(
                       state.Workspace().windows.canvas, begin)
                && inkpod::renderer::SubmitCanvasStrokeEvent(
                       state.Workspace().windows.canvas, end);
        };
        for (const auto& sample : samples) {
            if (!click(sample)) {
                return false;
            }
        }
        return click(samples.back());
    };
    const InkpodStatus snap_setup = state.engine->Invoke(
        [](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            InkpodStatus status = inkpod_core_guide_delete_all(core, &result);
            const InkpodGridInput grid{
                sizeof(InkpodGridInput), 0U, 0, 0, 8U, 8U, 2U, 0U};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_grid_set(core, &grid, &result);
            }
            std::uint64_t guide_id{};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_guide_add(
                    core, INKPOD_GUIDE_VERTICAL, 5, &result, &guide_id);
            }
            return status;
        },
        true,
        true);
    const auto set_snap_enabled = [&state](UINT command, bool& current, bool enabled) {
        if (current != enabled) {
            SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0);
        }
        UpdateMenuState(state);
        const UINT menu_state = GetMenuState(
            GetMenu(state.Workspace().windows.window), command, MF_BYCOMMAND);
        return current == enabled && menu_state != static_cast<UINT>(-1)
            && ((menu_state & MF_CHECKED) != 0U) == enabled;
    };
    if (snap_setup != INKPOD_STATUS_OK
        || !set_snap_enabled(
            IDM_VIEW_SNAP_GUIDES,
            state.ActiveView().presentation.snap_guides,
            true)
        || !set_snap_enabled(
            IDM_VIEW_SNAP_GRID,
            state.ActiveView().presentation.snap_grid,
            true)) {
        return 540;
    }
    struct GeometryProbe final {
        InkpodSnapshotVectorSegment last{};
        InkpodCanonicalDigest digest{sizeof(InkpodCanonicalDigest)};
        std::uint64_t segment_count{};
        bool valid{};
    };
    const auto probe_geometry = [&state](GeometryProbe& probe) {
        return state.engine->Invoke(
            [&probe](InkpodCore* core) {
                const InkpodSnapshotOptions options{
                    sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
                InkpodSnapshot* snapshot{};
                InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
                InkpodSnapshotVectorView vectors{};
                vectors.struct_size = sizeof(vectors);
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_snapshot_get_vectors(snapshot, &vectors);
                }
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_snapshot_get_canonical_digest(snapshot, &probe.digest);
                }
                if (status == INKPOD_STATUS_OK) {
                    probe.segment_count = vectors.segment_count;
                    probe.valid = vectors.segment_count != 0U && vectors.segments != nullptr;
                    if (probe.valid) {
                        probe.last = vectors.segments[vectors.segment_count - 1U];
                    }
                }
                const InkpodStatus released = inkpod_snapshot_release(&snapshot);
                return status == INKPOD_STATUS_OK ? released : status;
            },
            false,
            false);
    };
    const auto same_digest = [](const GeometryProbe& left, const GeometryProbe& right) {
        return left.digest.algorithm == right.digest.algorithm
            && std::memcmp(
                left.digest.bytes, right.digest.bytes, sizeof(left.digest.bytes)) == 0;
    };
    const std::array<InkpodStrokeSample, 4U> snap_line_samples{
        sample(5.2F, 7.8F), sample(8.0F, 10.0F),
        sample(14.0F, 14.0F), sample(18.1F, 17.9F)};
    GeometryProbe snap_before{};
    GeometryProbe snap_after{};
    if (probe_geometry(snap_before) != INKPOD_STATUS_OK
        || !gesture(IDM_VECTOR_LINE, snap_line_samples)
        || probe_geometry(snap_after) != INKPOD_STATUS_OK
        || !snap_after.valid
        || snap_after.segment_count != snap_before.segment_count + 1U
        || snap_after.last.p0.x != 5.0F || snap_after.last.p0.y != 8.0F
        || snap_after.last.p3.x != 20.0F || snap_after.last.p3.y != 16.0F
        || same_digest(snap_before, snap_after)) {
        return 541;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    GeometryProbe snap_undone{};
    if (probe_geometry(snap_undone) != INKPOD_STATUS_OK
        || snap_undone.segment_count != snap_before.segment_count
        || !same_digest(snap_undone, snap_before)) {
        return 542;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    GeometryProbe snap_redone{};
    if (probe_geometry(snap_redone) != INKPOD_STATUS_OK
        || snap_redone.segment_count != snap_after.segment_count
        || !same_digest(snap_redone, snap_after)
        || snap_redone.last.p0.x != 5.0F || snap_redone.last.p0.y != 8.0F
        || snap_redone.last.p3.x != 20.0F || snap_redone.last.p3.y != 16.0F) {
        return 543;
    }
    if (!set_snap_enabled(
            IDM_VIEW_SNAP_GUIDES,
            state.ActiveView().presentation.snap_guides,
            false)
        || !set_snap_enabled(
            IDM_VIEW_SNAP_GRID,
            state.ActiveView().presentation.snap_grid,
            false)
        || !gesture(IDM_VECTOR_LINE, snap_line_samples)) {
        return 544;
    }
    GeometryProbe raw_probe{};
    if (probe_geometry(raw_probe) != INKPOD_STATUS_OK || !raw_probe.valid
        || std::abs(raw_probe.last.p0.x - 5.2F) > 0.001F
        || std::abs(raw_probe.last.p0.y - 7.8F) > 0.001F
        || std::abs(raw_probe.last.p3.x - 18.1F) > 0.001F
        || std::abs(raw_probe.last.p3.y - 17.9F) > 0.001F) {
        return 545;
    }
    if (!set_snap_enabled(
            IDM_VIEW_SNAP_GUIDES,
            state.ActiveView().presentation.snap_guides,
            true)
        || !set_snap_enabled(
            IDM_VIEW_SNAP_GRID,
            state.ActiveView().presentation.snap_grid,
            true)) {
        return 546;
    }
    std::array<BYTE, 256U> snap_keyboard{};
    GetKeyboardState(snap_keyboard.data());
    const BYTE previous_control = snap_keyboard[VK_CONTROL];
    snap_keyboard[VK_CONTROL] = static_cast<BYTE>(0x80U);
    SetKeyboardState(snap_keyboard.data());
    const bool bypass_gesture = gesture(IDM_VECTOR_LINE, snap_line_samples);
    snap_keyboard[VK_CONTROL] = previous_control;
    SetKeyboardState(snap_keyboard.data());
    GeometryProbe bypass_probe{};
    if (!bypass_gesture || probe_geometry(bypass_probe) != INKPOD_STATUS_OK
        || !bypass_probe.valid
        || std::abs(bypass_probe.last.p0.x - 5.2F) > 0.001F
        || std::abs(bypass_probe.last.p0.y - 7.8F) > 0.001F
        || std::abs(bypass_probe.last.p3.x - 18.1F) > 0.001F
        || std::abs(bypass_probe.last.p3.y - 17.9F) > 0.001F) {
        return 547;
    }
    const std::array<InkpodStrokeSample, 4U> line_samples{
        sample(4.0F, 8.0F), sample(5.0F, 8.0F),
        sample(6.0F, 8.0F), sample(7.0F, 8.0F)};
    const std::array<InkpodStrokeSample, 4U> curve_samples{
        sample(10.0F, 12.0F), sample(16.0F, 4.0F),
        sample(24.0F, 20.0F), sample(30.0F, 12.0F)};
    const std::array<InkpodStrokeSample, 4U> shape_samples{
        sample(34.0F, 10.0F), sample(38.0F, 16.0F),
        sample(44.0F, 24.0F), sample(50.0F, 30.0F)};
    const std::array<InkpodStrokeSample, 4U> polyline_samples{
        sample(10.0F, 40.0F), sample(20.0F, 45.0F),
        sample(28.0F, 38.0F), sample(36.0F, 48.0F)};
    if (!gesture(IDM_VECTOR_LINE, line_samples)
        || !curve_gesture(curve_samples)
        || !gesture(IDM_VECTOR_RECTANGLE, shape_samples)
        || !gesture(IDM_VECTOR_ELLIPSE, shape_samples)
        || !gesture(IDM_VECTOR_POLYGON, shape_samples)
        || !polyline_gesture(polyline_samples)) {
        return 507;
    }
    const std::array<UINT, 8U> selection_commands{
        IDM_VECTOR_SELECT_CUT,
        IDM_VECTOR_SELECT_TOUCH,
        IDM_VECTOR_SELECT_CONTAINED,
        IDM_VECTOR_SELECT_LINE,
        IDM_VECTOR_SELECT_WHOLE_LINE,
        IDM_VECTOR_SELECT_INTERSECTION,
        IDM_VECTOR_SELECT_FILL_BOUNDARY,
        IDM_VECTOR_SELECT_FILL};
    for (std::size_t index = 0U; index < selection_commands.size(); ++index) {
        const UINT command = selection_commands[index];
        if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0) != 1) {
            return 520 + static_cast<int>(index);
        }
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_SELECT_TOUCH, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_WIDTH, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_CONNECT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_ERASE_WHOLE, 0) != 1) {
        return 509;
    }
    const inkpod::renderer::CanvasStrokeEvent erase_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, line_samples.data() + 2U, 1U};
    const inkpod::renderer::CanvasStrokeEvent erase_end{
        inkpod::renderer::CanvasStrokeEventKind::End, line_samples.data() + 2U, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, erase_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, erase_end)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_RASTERIZE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_VECTORIZE, 0) != 1) {
        return 510;
    }
    PumpPendingWindowMessages();
    return 0;
}

int RunImageEffectsSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 600;
    }
    bool preview_cancelled{};
    bool undo_restored{};
    bool adjustment_preserved_source{};
    bool effect_connected{};
    const InkpodStatus status = state.engine->Invoke(
        [&preview_cancelled, &undo_restored, &adjustment_preserved_source, &effect_connected](
            InkpodCore* core) {
            InkpodDocumentInfo document = EmptyDocumentInfo();
            InkpodStatus current = inkpod_core_get_document_info(core, &document);
            const std::array<InkpodStrokeSample, 1U> samples{
                InkpodStrokeSample{
                    sizeof(InkpodStrokeSample), 0U, 32.0F, 32.0F, 1.0F, 0U}};
            InkpodStrokeInput stroke{
                sizeof(InkpodStrokeInput),
                INKPOD_TOOL_PENCIL,
                INKPOD_PLANE_COLOR,
                INKPOD_COORDINATE_SPACE_DOCUMENT,
                0U,
                UINT32_C(0x204080ff),
                3.0F,
                samples.data(),
                samples.size(),
                sizeof(InkpodStrokeSample),
                INKPOD_BRUSH_ROUND,
                0U,
                0U,
                INKPOD_START_COLOR_ANY,
                0U};
            InkpodDispatchResult dispatch{};
            dispatch.struct_size = sizeof(dispatch);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_apply_stroke(core, &stroke, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
            }
            const std::uint64_t original = document.color_plane_checksum;
            InkpodFilterInput filter{};
            filter.struct_size = sizeof(filter);
            filter.kind = INKPOD_FILTER_INVERT;
            filter.plane_id = document.color_plane_id;
            filter.channel = INKPOD_FILTER_CHANNEL_RGB;
            InkpodFilterPreviewInfo preview{};
            preview.struct_size = sizeof(preview);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK
                && (preview.base_checksum != original
                    || preview.preview_checksum == original)) {
                current = INKPOD_STATUS_INVALID_STATE;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_cancel(core, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                preview_cancelled = document.color_plane_checksum == original
                    && preview.preview_checksum == original;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_apply(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_undo(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                undo_restored = document.color_plane_checksum == original;
            }
            filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
            filter.parameter_0 = 100;
            filter.parameter_1 = 200;
            static constexpr std::array<std::uint8_t, 13U> name{
                'M', '6', ' ', 'A', 'd', 'j', 'u', 's', 't', 'm', 'e', 'n', 't'};
            std::uint64_t adjustment_id{};
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_adjustment_create(
                    core,
                    &filter,
                    name.data(),
                    name.size(),
                    &dispatch,
                    &adjustment_id);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                adjustment_preserved_source = adjustment_id != 0U
                    && document.color_plane_checksum == original;
            }
            const auto color16 = [](std::uint16_t red,
                                    std::uint16_t green,
                                    std::uint16_t blue,
                                    std::uint16_t alpha) noexcept {
                InkpodColorValue color{};
                color.struct_size = sizeof(color);
                color.depth = INKPOD_COLOR_DEPTH_16;
                color.red = red;
                color.green = green;
                color.blue = blue;
                color.alpha = alpha;
                return color;
            };
            std::array<InkpodGradientStop, 3U> stops{};
            for (auto& stop : stops) {
                stop.struct_size = sizeof(stop);
            }
            stops[0].position_milli = 0U;
            stops[0].color = color16(65535U, 0U, 0U, 65535U);
            stops[1].position_milli = 500U;
            stops[1].color = color16(0U, 65535U, 0U, 32768U);
            stops[2].position_milli = 1000U;
            stops[2].color = color16(0U, 0U, 65535U, 65535U);
            InkpodGradientInput gradient{};
            gradient.struct_size = sizeof(gradient);
            gradient.kind = INKPOD_GRADIENT_LINEAR;
            gradient.plane_id = document.color_plane_id;
            gradient.mode = INKPOD_GRADIENT_OVERWRITE;
            gradient.start_x_milli = 500;
            gradient.start_y_milli = 500;
            gradient.end_x_milli = 63500;
            gradient.end_y_milli = 500;
            gradient.stops = stops.data();
            gradient.stop_count = stops.size();
            gradient.stop_stride_bytes = sizeof(InkpodGradientStop);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_effect_gradient(core, &gradient, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                effect_connected = document.color_plane_checksum != original;
            }
            return current;
        },
        true,
        true);
    if (status != INKPOD_STATUS_OK || !preview_cancelled || !undo_restored
        || !adjustment_preserved_source || !effect_connected) {
        return 601;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_COLOR, 0);
    InkpodDocumentInfo before_menu = EmptyDocumentInfo();
    InkpodDocumentInfo after_menu = EmptyDocumentInfo();
    HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr || !QueryDocument(state, before_menu)
        || GetMenuState(menu, IDM_FILTER_LAST, MF_BYCOMMAND) == static_cast<UINT>(-1)) {
        return 602;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILTER_LAST, 0);
    if (!QueryDocument(state, after_menu)
        || after_menu.color_plane_checksum == before_menu.color_plane_checksum) {
        return 603;
    }
    InkpodDocumentInfo before_live_preview = EmptyDocumentInfo();
    InkpodDocumentInfo after_live_preview = EmptyDocumentInfo();
    const std::size_t preview_checksum_start =
        state.effects.filter_preview.smoke_checksum_count;
    const std::uint64_t preview_updates_start =
        state.effects.filter_preview.completed_updates;
    if (!QueryDocument(state, before_live_preview)) {
        return 604;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_FILTER_BRIGHTNESS, 0);
    if (!QueryDocument(state, after_live_preview)
        || after_live_preview.color_plane_checksum
            == before_live_preview.color_plane_checksum
        || state.effects.filter_preview.completed_updates
            < preview_updates_start + 4U
        || state.effects.filter_preview.smoke_checksum_count
            < preview_checksum_start + 4U) {
        return 605;
    }
    const auto& preview_checksums = state.effects.filter_preview.smoke_checksums;
    if (preview_checksums[preview_checksum_start]
            == preview_checksums[preview_checksum_start + 1U]
        || preview_checksums[preview_checksum_start + 1U]
            == preview_checksums[preview_checksum_start + 2U]
        || preview_checksums[preview_checksum_start + 2U]
            == preview_checksums[preview_checksum_start + 3U]) {
        return 606;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    InkpodDocumentInfo after_live_undo = EmptyDocumentInfo();
    if (!QueryDocument(state, after_live_undo)
        || after_live_undo.color_plane_checksum
            != before_live_preview.color_plane_checksum) {
        return 607;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    InkpodDocumentInfo before_live_cancel = EmptyDocumentInfo();
    InkpodDocumentInfo after_live_cancel = EmptyDocumentInfo();
    if (!QueryDocument(state, before_live_cancel)) {
        return 608;
    }
    state.effects.filter_preview.smoke_cancel_next = true;
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_FILTER_BRIGHTNESS, 0);
    if (!QueryDocument(state, after_live_cancel)
        || after_live_cancel.color_plane_checksum
            != before_live_cancel.color_plane_checksum
        || state.effects.filter_preview.session_active
        || state.effects.task != nullptr) {
        return 609;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    if (state.effects.adjustments.size() != 2U
        || state.effects.adjustments[0].id == state.effects.adjustments[1].id) {
        return 610;
    }
    const std::uint64_t newest_adjustment = state.effects.adjustment_id;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_PREVIOUS, 0);
    if (state.effects.adjustment_id == newest_adjustment) {
        return 611;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_EDIT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_TOGGLE, 0);
    if (state.effects.adjustment_visible) {
        return 612;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_MOVE_TOP, 0);
    InkpodDocumentInfo before_spray = EmptyDocumentInfo();
    InkpodDocumentInfo after_spray = EmptyDocumentInfo();
    inkpod::renderer::CanvasDocumentBounds spray_bounds{};
    if (!QueryDocument(state, before_spray)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, spray_bounds)) {
        return 613;
    }
    if (SetEditorActiveTool(state, kInteractionEffectAirbrush)
        != INKPOD_STATUS_OK) {
        return 614;
    }
    state.effects.options.parameters = {4000, 1000, 1000, 750, 0};
    state.effects.options.option = true;
    state.effects.options.option2 = true;
    const InkpodStrokeSample spray_sample{
        sizeof(InkpodStrokeSample),
        0U,
        static_cast<float>((spray_bounds.left + spray_bounds.right) / 2.0),
        static_cast<float>((spray_bounds.top + spray_bounds.bottom) / 2.0),
        1.0F,
        0U};
    const inkpod::renderer::CanvasStrokeEvent spray_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &spray_sample, 1U};
    const inkpod::renderer::CanvasStrokeEvent spray_end{
        inkpod::renderer::CanvasStrokeEventKind::End, &spray_sample, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, spray_begin)) {
        return 609;
    }
    const auto spray_timer = state.routing.timers.Find(
        CommandTimerKind::ContinuousSpray);
    if (!spray_timer.has_value()
        || SendMessageW(
               state.Workspace().windows.window,
               WM_TIMER,
               static_cast<WPARAM>(spray_timer->value),
               0) != 0
        || state.effects.samples.size() < 2U
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, spray_end)
        || state.engine->Invoke(
               [](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, after_spray)
        || after_spray.color_plane_checksum == before_spray.color_plane_checksum) {
        return 609;
    }
    return SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 604;
}

int RunBatchWorkflowSmoke(ApplicationHost& state) noexcept {
    constexpr wchar_t settings_path[] = L"inkpod-batch-ui-smoke.inkbatch";
    constexpr wchar_t output_path[] = L"inkpod-batch-windows-smoke_0001.inkpod";
    const auto cleanup = [&]() noexcept {
        DeleteFileW(settings_path);
        DeleteFileW(output_path);
    };
    cleanup();
    if (state.engine == nullptr || state.Workspace().batch_palette == nullptr
        || GetParent(state.Workspace().batch_palette)
            != state.Workspace().windows.window
        || (static_cast<DWORD>(GetWindowLongPtrW(
                state.Workspace().batch_palette,
                GWL_STYLE))
            & WS_CHILD)
            == 0U) {
        return 700;
    }
    InkpodDocumentInfo pair_document = EmptyDocumentInfo();
    if (!QueryDocument(state, pair_document)) {
        return 926;
    }
    constexpr std::array<std::uint8_t, 4U> pair_source_old{1U, 2U, 3U, 255U};
    constexpr std::array<std::uint8_t, 4U> pair_source_new{4U, 5U, 6U, 255U};
    constexpr std::array<std::uint8_t, 4U> pair_name_old{'o', 'l', 'd', '1'};
    constexpr std::array<std::uint8_t, 4U> pair_name_new{'n', 'e', 'w', '2'};
    const std::array<InkpodSequenceCellInput, 2U> pair_cells{
        InkpodSequenceCellInput{
            sizeof(InkpodSequenceCellInput),
            0U,
            pair_name_old.data(),
            pair_name_old.size(),
            InkpodRasterSourceInput{
                sizeof(InkpodRasterSourceInput), INKPOD_STORAGE_RGBA8, 0U,
                pair_document.document_uuid_high,
                pair_document.document_uuid_low,
                1U, 1U, 1U, 96000U, 96000U,
                InkpodFrameRect{0, 0, 1, 1}, pair_source_old.data(),
                pair_source_old.size(), 4U}},
        InkpodSequenceCellInput{
            sizeof(InkpodSequenceCellInput),
            0U,
            pair_name_new.data(),
            pair_name_new.size(),
            InkpodRasterSourceInput{
                sizeof(InkpodRasterSourceInput), INKPOD_STORAGE_RGBA8, 0U,
                pair_document.document_uuid_high ^ UINT64_C(1),
                pair_document.document_uuid_low,
                2U, 1U, 1U, 96000U, 96000U,
                InkpodFrameRect{0, 0, 1, 1}, pair_source_new.data(),
                pair_source_new.size(), 4U}}};
    const InkpodSequenceInput pair_sequence{
        sizeof(InkpodSequenceInput), 0U, 0U, pair_cells.data(), pair_cells.size(),
        sizeof(InkpodSequenceCellInput)};
    if (state.engine->Invoke(
            [&pair_sequence](InkpodCore* core) {
                return inkpod_core_sequence_set(core, &pair_sequence);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 926;
    }
    HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_WINDOW_BATCH, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_BATCH, 0) != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Batch)) {
        cleanup();
        return 701;
    }
    if (GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_TARGET) == nullptr
        || GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_PIN) == nullptr
        || GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_JOB) == nullptr
        || !WindowHasAccessibleName(
            GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_TARGET))
        || !WindowHasAccessibleName(
            GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_JOB))
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_BATCH_PIN,
               0) != 1
        || state.routing.pane_targets.Find(state.routing.batch_pane)->policy
            != inkpod::app::PaneTargetPolicy::PinnedDocument
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_BATCH_PIN,
               0) != 1
        || state.routing.pane_targets.Find(state.routing.batch_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView) {
        cleanup();
        return 925;
    }
    state.batch.output_folder = L".";
    state.batch.basename = L"inkpod-batch-windows-smoke";
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_INPUT_CURRENT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_INPUT_RANGE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OUTPUT_SETTINGS, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_ADD_COLOR_REPLACE, 0) != 1
        || state.batch.operations.size() != 1U
        || state.batch.operations[0].color_pairs.size() != 1U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_BATCH_EXTRACT_PAIRS,
               0) != 1
        || state.batch.operations[0].color_pairs.size() != 1U
        || state.batch.last_result.find(L"候補 1") == std::wstring::npos) {
        cleanup();
        return 702;
    }
    try {
        state.batch.operations[0].color_pairs.push_back(
            state.batch.operations[0].color_pairs.front());
    } catch (const std::bad_alloc&) {
        cleanup();
        return 702;
    }
    state.batch.operations[0].color_pairs[1].enabled = 0U;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_BATCH_OPERATION_EDIT,
            0) != 1
        || state.batch.operations[0].color_pairs.size() != 2U
        || state.batch.operations[0].color_pairs[1].enabled != 0U) {
        cleanup();
        return 702;
    }
    const InkpodColorValue old_before = state.batch.operations[0].color_pairs[0].old_color;
    const InkpodColorValue new_before = state.batch.operations[0].color_pairs[0].new_color;
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_REPLACE_SWAP, 0) != 1) {
        cleanup();
        return 703;
    }
    const auto& swapped = state.batch.operations[0].color_pairs[0];
    if (std::memcmp(&swapped.old_color, &new_before, sizeof(new_before)) != 0
        || std::memcmp(&swapped.new_color, &old_before, sizeof(old_before)) != 0
        || state.batch.operations[0].color_pairs[1].enabled != 0U) {
        cleanup();
        return 704;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_ADD_BOUNDARY_AIRBRUSH, 0) != 1
        || state.batch.operations.size() != 2U
        || state.batch.operations.back().colors.size() < 2U
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_DRY_RUN, 0) != 1
        || state.batch.report == nullptr
        || state.batch.job_id.has_value()
        || state.routing.pane_targets.Find(state.routing.batch_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView
        || state.batch.job_text.find(L"完了") == std::wstring::npos
        || !WindowHasAccessibleName(
            GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_TARGET))
        || !WindowHasAccessibleName(
            GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_JOB))
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_REMOVE, 0) != 1
        || state.batch.operations.size() != 1U) {
        cleanup();
        return 705;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_ADD_MIRROR, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_UP, 0) != 1
        || state.batch.operations.front().kind != INKPOD_BATCH_OPERATION_MIRROR
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_DOWN, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_EDIT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_REMOVE, 0) != 1
        || state.batch.operations.size() != 1U
        || state.batch.operations[0].kind != INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        cleanup();
        return 706;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_BATCH_ADD_SEPARATION,
            0) != 1
        || state.batch.operations.back().kind != INKPOD_BATCH_OPERATION_SEPARATION) {
        cleanup();
        return 927;
    }
    state.batch.operations.back().parameters[1] =
        INKPOD_BATCH_SEPARATION_SELECTION_MASK;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_BATCH_OPERATION_EDIT,
            0) != 1
        || state.batch.operations.back().parameters[1]
            != INKPOD_BATCH_SEPARATION_SELECTION_MASK
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_BATCH_OPERATION_REMOVE,
               0) != 1) {
        cleanup();
        return 927;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_BATCH_ADD_CONTINUOUS_FILL,
            0) != 1
        || state.batch.operations.back().seeds.size() != 1U) {
        cleanup();
        return 928;
    }
    try {
        state.batch.operations.back().seeds.push_back(
            state.batch.operations.back().seeds.front());
    } catch (const std::bad_alloc&) {
        cleanup();
        return 928;
    }
    state.batch.operations.back().seeds[1].flags &= ~INKPOD_BATCH_SEED_ENABLED;
    state.batch.operations.back().flags |=
        INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_BATCH_OPERATION_EDIT,
            0) != 1
        || state.batch.operations.back().seeds.size() != 2U
        || (state.batch.operations.back().seeds[1].flags
                & INKPOD_BATCH_SEED_ENABLED)
            != 0U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_BATCH_OPERATION_REMOVE,
               0) != 1
        || state.batch.operations.size() != 1U) {
        cleanup();
        return 928;
    }
    const LRESULT input_count = SendDlgItemMessageW(
        state.Workspace().batch_palette, IDC_BATCH_INPUTS, LB_GETCOUNT, 0, 0);
    const LRESULT operation_count = SendDlgItemMessageW(
        state.Workspace().batch_palette, IDC_BATCH_OPERATIONS, LB_GETCOUNT, 0, 0);
    if (input_count != 1 || operation_count != 1
        || GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_OUTPUT) == nullptr) {
        cleanup();
        return 707;
    }
    state.batch.operations[0].flags |=
        INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN;
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_SAVE_SET, 0) != 1
        || GetFileAttributesW(settings_path) == INVALID_FILE_ATTRIBUTES
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_LOAD_SET, 0) != 1
        || !state.batch.loaded_graph || state.batch.graph == nullptr) {
        cleanup();
        return 708;
    }
    InkpodBatchGraphInfo graph_info{};
    graph_info.struct_size = sizeof(graph_info);
    if (inkpod_batch_graph_get_info(state.batch.graph, &graph_info) != INKPOD_STATUS_OK
        || graph_info.version != INKPOD_BATCH_GRAPH_VERSION
        || graph_info.operation_count != 1U
        || graph_info.output_policy != INKPOD_BATCH_OUTPUT_DUPLICATE
        || state.batch.operations.size() != 1U
        || (state.batch.operations[0].flags
                & INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN)
            == 0U) {
        cleanup();
        return 709;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_PREVIEW, 0) != 1
        || state.batch.preview == nullptr
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_DRY_RUN, 0) != 1
        || state.batch.report == nullptr
        || GetFileAttributesW(output_path) != INVALID_FILE_ATTRIBUTES) {
        cleanup();
        return 710;
    }
    InkpodBatchReportInfo report_info{};
    report_info.struct_size = sizeof(report_info);
    InkpodBatchReportItem report_item{};
    report_item.struct_size = sizeof(report_item);
    if (inkpod_batch_report_get_info(state.batch.report, &report_info) != INKPOD_STATUS_OK
        || report_info.failure_count != 0U || report_info.item_count == 0U
        || inkpod_batch_report_get(state.batch.report, 0U, &report_item)
            != INKPOD_STATUS_OK
        || report_item.outcome != INKPOD_BATCH_ITEM_DRY_RUN) {
        cleanup();
        return 711;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_BATCH_RUN_CURRENT,
            0) != 1) {
        std::fwprintf(
            stderr,
            L"batch production run failed: %ls\n",
            state.engine->LastError().c_str());
        cleanup();
        return 941;
    }
    if (GetFileAttributesW(output_path) == INVALID_FILE_ATTRIBUTES) {
        cleanup();
        return 942;
    }
    if (inkpod_batch_report_get_info(state.batch.report, &report_info)
        != INKPOD_STATUS_OK) {
        cleanup();
        return 943;
    }
    if (report_info.failure_count != 0U) {
        cleanup();
        return 944;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_BATCH, 0) != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Batch)) {
        cleanup();
        return 713;
    }
    cleanup();
    return 0;
}

bool WaitForLocatorPresentation(
    ApplicationHost& state,
    std::uint64_t minimum_generation,
    std::chrono::milliseconds timeout) noexcept {
    const auto deadline = std::chrono::steady_clock::now() + timeout;
    do {
        PumpPendingWindowMessages();
        if (state.ActiveView().presentation.locator_presented_generation
            >= minimum_generation) {
            return true;
        }
        (void)MsgWaitForMultipleObjectsEx(
            0U,
            nullptr,
            10U,
            QS_ALLINPUT,
            MWMO_INPUTAVAILABLE);
    } while (std::chrono::steady_clock::now() < deadline);
    PumpPendingWindowMessages();
    return state.ActiveView().presentation.locator_presented_generation
        >= minimum_generation;
}

int RunMagnifiedRasterHitSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr
        || CreateCell(state, 8U, 8U, 96'000U) != INKPOD_STATUS_OK) {
        return 720;
    }
    if (SetEditorActiveTool(state, INKPOD_TOOL_PENCIL)
        != INKPOD_STATUS_OK) {
        return 720;
    }

    InkpodDocumentInfo blank = EmptyDocumentInfo();
    InkpodDocumentInfo seeded = EmptyDocumentInfo();
    const std::array<InkpodStrokeSample, 2U> source{{
        {sizeof(InkpodStrokeSample), 0U, 3.0F, 3.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 4.0F, 3.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        INKPOD_FEATURE_NONE,
        UINT32_C(0x000000ff),
        1.0F,
        source.data(),
        source.size(),
        sizeof(InkpodStrokeSample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (!QueryDocument(state, blank)
        || state.engine->Invoke(
               [stroke](InkpodCore* core) {
                   InkpodStatus status = inkpod_core_set_active_plane(
                       core, INKPOD_PLANE_MAIN_LINE);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_core_apply_stroke(core, &stroke, &result);
                   }
                   return status;
               },
               true,
               true) != INKPOD_STATUS_OK
        || !QueryDocument(state, seeded)
        || seeded.main_plane_checksum == blank.main_plane_checksum) {
        return 721;
    }

    inkpod::renderer::CanvasDocumentBounds bounds{};
    RECT client{};
    if (GetClientRect(state.Workspace().windows.canvas, &client) == FALSE
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)) {
        return 722;
    }
    const double initial_zoom = (bounds.right - bounds.left) / 8.0;
    if (!std::isfinite(initial_zoom) || initial_zoom <= 0.0
        || ApplyView(
               state,
               INKPOD_VIEW_ZOOM_AT,
               64.0 / initial_zoom,
               static_cast<double>(client.right - client.left) / 2.0,
               static_cast<double>(client.bottom - client.top) / 2.0)
            != INKPOD_STATUS_OK
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        return 722;
    }

    bounds = {};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, bounds)) {
        return 723;
    }
    const double zoom = (bounds.right - bounds.left) / 8.0;
    if (std::abs(zoom - 64.0) > 0.001) {
        return 724;
    }
    const int device_x = static_cast<int>(std::lround(bounds.left + 3.75 * zoom));
    const int drag_device_x =
        static_cast<int>(std::lround(bounds.left + 4.75 * zoom));
    const int device_y = static_cast<int>(std::lround(bounds.top + 3.75 * zoom));
    if (SendMessageW(
            state.Workspace().windows.window,
            inkpod::renderer::kCanvasPointerMoved,
            static_cast<WPARAM>(state.routing.targets.Canvas().Value()),
            MAKELPARAM(device_x, device_y)) != 1
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 858;
    }
    PumpPendingWindowMessages();
    constexpr std::size_t kLocatorCenterAlpha = ((4U * 9U + 4U) * 4U) + 3U;
    if (!state.ActiveView().presentation.locator_valid
        || state.ActiveView().presentation.locator_neighborhood_width != 9U
        || state.ActiveView().presentation.locator_neighborhood_height != 9U
        || state.ActiveView().presentation
                .locator_neighborhood[kLocatorCenterAlpha]
            != UINT8_MAX) {
        return 859;
    }
    const std::uint64_t locator_generation_before_stroke =
        state.ActiveView().presentation.locator_generation;
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(device_x, device_y)) != 1) {
        return 725;
    }
    for (std::uint32_t index = 0U; index < 32U; ++index) {
        const int move_x = (index % 2U) == 0U ? drag_device_x : device_x;
        if (SendMessageW(
                state.Workspace().windows.canvas,
                WM_MOUSEMOVE,
                MK_LBUTTON,
                MAKELPARAM(move_x, device_y)) != 1) {
            return 725;
        }
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_MOUSEMOVE,
            MK_LBUTTON,
            MAKELPARAM(drag_device_x, device_y)) != 1) {
        return 725;
    }
    const std::uint64_t latest_locator_generation =
        state.ActiveView().presentation.locator_generation;
    if (!WaitForLocatorPresentation(
            state, latest_locator_generation, std::chrono::seconds(5))) {
        return 725;
    }

    InkpodDocumentInfo during_preview = EmptyDocumentInfo();
    if (!QueryDocument(state, during_preview)
        || during_preview.document_revision != seeded.document_revision
        || during_preview.main_plane_checksum != seeded.main_plane_checksum
        || state.ActiveView().presentation.locator_generation
            <= locator_generation_before_stroke
        || state.ActiveView().presentation.locator_presented_generation
            != latest_locator_generation
        || !state.ActiveView().presentation.locator_valid
        || state.ActiveView().presentation.locator.document_x != 4
        || state.ActiveView().presentation.locator.document_y != 3
        || state.ActiveView().presentation
                .locator_neighborhood[kLocatorCenterAlpha]
            != 0U) {
        return 867;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONUP,
            0,
            MAKELPARAM(drag_device_x, device_y)) != 1) {
        return 725;
    }
    PumpPendingWindowMessages();
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 725;
    }
    PumpPendingWindowMessages();
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 725;
    }
    PumpPendingWindowMessages();

    InkpodDocumentInfo erased = EmptyDocumentInfo();
    if (!QueryDocument(state, erased)
        || erased.document_revision != seeded.document_revision + 1U
        || erased.main_plane_checksum != blank.main_plane_checksum
        || !state.ActiveView().presentation.locator_valid
        || state.ActiveView().presentation
                .locator_neighborhood[kLocatorCenterAlpha]
            != 0U) {
        return 726;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(device_x, device_y)) != 1
        || SendMessageW(
               state.Workspace().windows.canvas,
               WM_MOUSEMOVE,
               MK_LBUTTON,
               MAKELPARAM(drag_device_x, device_y)) != 1) {
        return 868;
    }
    PumpPendingWindowMessages();
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 868;
    }
    PumpPendingWindowMessages();
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 868;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo cancel_preview = EmptyDocumentInfo();
    if (!QueryDocument(state, cancel_preview)
        || cancel_preview.document_revision != erased.document_revision
        || cancel_preview.main_plane_checksum != erased.main_plane_checksum
        || !state.ActiveView().presentation.locator_valid
        || state.ActiveView().presentation
                .locator_neighborhood[kLocatorCenterAlpha]
            != UINT8_MAX) {
        return 869;
    }
    const std::uint64_t locator_generation_before_cancel =
        state.ActiveView().presentation.locator_generation;
    if (GetCapture() != state.Workspace().windows.canvas
        || ReleaseCapture() == FALSE) {
        return 870;
    }
    PumpPendingWindowMessages();
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 870;
    }
    PumpPendingWindowMessages();
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 870;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo cancelled = EmptyDocumentInfo();
    if (!QueryDocument(state, cancelled)
        || cancelled.document_revision != erased.document_revision
        || cancelled.main_plane_checksum != erased.main_plane_checksum
        || state.ActiveView().presentation.locator_generation
            <= locator_generation_before_cancel
        || !state.ActiveView().presentation.locator_valid
        || state.ActiveView().presentation
                .locator_neighborhood[kLocatorCenterAlpha]
            != 0U) {
        return 871;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_WINDOW_LOCATOR,
            0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LOCATOR_FIXED,
               0) != 1
        || state.Workspace().locator_dialog.select_pixel == nullptr) {
        return 860;
    }
    state.Workspace().locator_dialog.select_pixel(
        state.Workspace().locator_dialog.context,
        4,
        4);
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 861;
    }
    InkpodDocumentInfo locator_drawn = EmptyDocumentInfo();
    if (!QueryDocument(state, locator_drawn)
        || locator_drawn.document_revision != erased.document_revision + 1U
        || locator_drawn.main_plane_checksum == erased.main_plane_checksum) {
        return 862;
    }
    state.Workspace().locator_dialog.select_pixel(
        state.Workspace().locator_dialog.context,
        -1,
        -1);
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 863;
    }
    InkpodDocumentInfo invalid_unchanged = EmptyDocumentInfo();
    if (!QueryDocument(state, invalid_unchanged)
        || invalid_unchanged.document_revision != locator_drawn.document_revision
        || invalid_unchanged.main_plane_checksum
            != locator_drawn.main_plane_checksum
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_UNDO,
               0) != 0) {
        return 864;
    }
    InkpodDocumentInfo undone = EmptyDocumentInfo();
    if (!QueryDocument(state, undone)
        || undone.main_plane_checksum != erased.main_plane_checksum
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_REDO,
               0) != 0) {
        return 865;
    }
    InkpodDocumentInfo redone = EmptyDocumentInfo();
    if (!QueryDocument(state, redone)
        || redone.main_plane_checksum != locator_drawn.main_plane_checksum
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_LOCATOR_FIXED,
               0) != 1
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_WINDOW_LOCATOR,
               0) != 1) {
        return 866;
    }
    return 0;
}

int RunMultiDocumentTabSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::DocumentSessionId;
    using inkpod::app::DocumentViewId;
    using inkpod::app::Generation;

    InkpodDocumentInfo initial_probe = EmptyDocumentInfo();
    if (state.engine == nullptr || !QueryDocument(state, initial_probe)) {
        return 732;
    }
    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 733;
    }
    const auto suffix = static_cast<unsigned long long>(GetTickCount64());
    std::array<wchar_t, MAX_PATH> first_buffer{};
    std::array<wchar_t, MAX_PATH> second_buffer{};
    _snwprintf_s(
        first_buffer.data(),
        first_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-g5-first-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    _snwprintf_s(
        second_buffer.data(),
        second_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-g5-second-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    const std::wstring first_path(first_buffer.data());
    const std::wstring second_path(second_buffer.data());
    DeleteFileW(first_path.c_str());
    DeleteFileW(second_path.c_str());
    const auto cleanup = [&first_path, &second_path]() noexcept {
        DeleteFileW(first_path.c_str());
        DeleteFileW(second_path.c_str());
    };

    const std::size_t baseline_count = state.Documents().Count();
    const DocumentSessionId first_session = state.Document().id;
    const Generation first_generation = state.Document().generation;
    const DocumentViewId first_view = state.ActiveView().id;
    InkpodDocumentInfo first_saved = EmptyDocumentInfo();
    InkpodHistoryInfo first_history{};
    first_history.struct_size = sizeof(first_history);
    if (baseline_count == 0U
        || SaveToPath(state, first_path) != INKPOD_STATUS_OK
        || !QueryDocument(state, first_saved)
        || (first_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&first_history](InkpodCore* core) {
                   return inkpod_core_history_info(core, &first_history);
               },
               false,
               false) != INKPOD_STATUS_OK
        || state.RecentDocumentAt(0U) == nullptr
        || state.RecentDocumentAt(0U)->path != first_path) {
        cleanup();
        return 734;
    }

    if (CreateCell(state, 96U, 64U, 96000U) != INKPOD_STATUS_OK
        || state.Documents().Count() != baseline_count + 1U
        || state.engine->SessionCount() != state.Documents().Count()
        || state.Document().id == first_session
        || !DocumentTabsMatchRegistry(state)) {
        cleanup();
        return 735;
    }
    const DocumentSessionId second_session = state.Document().id;
    const Generation second_generation = state.Document().generation;
    const DocumentViewId second_view = state.ActiveView().id;
    InkpodDocumentInfo second_initial = EmptyDocumentInfo();
    if (!QueryDocument(state, second_initial)
        || state.CloseDocumentView(DocumentViewId{UINT64_MAX})) {
        cleanup();
        return 736;
    }

    if (!ActivateDocumentTab(state, first_view)
        || !RefreshLightTablePane(state)
        || IssueCommand(
               &state,
               state.Workspace().windows.window,
               IDM_LIGHT_TABLE_PIN,
               0,
               state.routing.light_table_pane).value_or(0) != 1
        || !ActivateDocumentTab(state, second_view)
        || IssueCommand(
               &state,
               state.Workspace().windows.window,
               IDM_LT_GLOBAL_OPACITY,
               0,
               state.routing.light_table_pane).value_or(0) != 1) {
        cleanup();
        return 880;
    }
    std::uint32_t first_light_opacity{};
    std::uint32_t second_light_opacity{};
    const auto read_light_opacity = [](
        InkpodCore* core, std::uint32_t& opacity) noexcept {
        InkpodLightTableSetInfo info{};
        info.struct_size = sizeof(info);
        const InkpodStatus status = inkpod_core_light_table_set_get(core, 0U, &info);
        if (status == INKPOD_STATUS_OK) {
            opacity = info.opacity_milli;
        }
        return status;
    };
    if (state.engine->Invoke(
            first_session,
            first_generation,
            [&read_light_opacity, &first_light_opacity](InkpodCore* core) {
                return read_light_opacity(core, first_light_opacity);
            },
            false,
            false) != INKPOD_STATUS_OK
        || state.engine->Invoke(
               second_session,
               second_generation,
               [&read_light_opacity, &second_light_opacity](InkpodCore* core) {
                   return read_light_opacity(core, second_light_opacity);
               },
               false,
               false) != INKPOD_STATUS_OK
        || first_light_opacity != 500U
        || second_light_opacity != 1000U
        || state.engine->Invoke(
               first_session,
               first_generation,
               [](InkpodCore* core) {
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_undo(core, &result);
               },
               true,
               true) != INKPOD_STATUS_OK
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&read_light_opacity, &first_light_opacity](InkpodCore* core) {
                   return read_light_opacity(core, first_light_opacity);
               },
               false,
               false) != INKPOD_STATUS_OK
        || first_light_opacity != 1000U
        || state.engine->Invoke(
               first_session,
               first_generation,
               [](InkpodCore* core) {
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_redo(core, &result);
               },
               true,
               true) != INKPOD_STATUS_OK
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&read_light_opacity, &first_light_opacity](InkpodCore* core) {
                   return read_light_opacity(core, first_light_opacity);
               },
               false,
               false) != INKPOD_STATUS_OK
        || first_light_opacity != 500U
        || state.engine->Invoke(
               first_session,
               first_generation,
               [](InkpodCore* core) {
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_undo(core, &result);
               },
               true,
               true) != INKPOD_STATUS_OK
        || IssueCommand(
               &state,
               state.Workspace().windows.window,
               IDM_LIGHT_TABLE_PIN,
               0,
               state.routing.light_table_pane).value_or(0) != 1
        || state.routing.pane_targets.Find(state.routing.light_table_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView) {
        cleanup();
        return 881;
    }
    if (!ActivateDocumentTab(state, first_view)
        || !QueryDocument(state, first_saved)
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&first_history](InkpodCore* core) {
                   return inkpod_core_history_info(core, &first_history);
               },
               false,
               false) != INKPOD_STATUS_OK
        || !ActivateDocumentTab(state, second_view)) {
        cleanup();
        return 884;
    }

    if (SetEditorActiveTool(state, INKPOD_TOOL_PENCIL)
            != INKPOD_STATUS_OK
        || SendMessageW(
               state.Workspace().windows.canvas,
               WM_LBUTTONDOWN,
               MK_LBUTTON,
               MAKELPARAM(120, 120)) != 1
        || !ActivateDocumentTab(state, first_view)
        || state.engine->WaitIdle(second_session, second_generation)
            != INKPOD_STATUS_OK) {
        cleanup();
        return 737;
    }
    InkpodDocumentInfo second_cancelled = EmptyDocumentInfo();
    if (!state.engine->GetDocumentInfo(
            second_session, second_generation, second_cancelled)
        || second_cancelled.document_revision
            != second_initial.document_revision
        || second_cancelled.main_plane_checksum
            != second_initial.main_plane_checksum
        || !ActivateDocumentTab(state, second_view)) {
        cleanup();
        return 738;
    }

    const InkpodStrokeSample sample{
        sizeof(InkpodStrokeSample), 0U, 12.0F, 12.0F, 1.0F, 0U};
    const InkpodStrokeInput stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        INKPOD_FEATURE_NONE,
        UINT32_C(0x000000ff),
        1.0F,
        &sample,
        1U,
        sizeof(sample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (state.engine->Invoke(
            [stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        cleanup();
        return 739;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo second_edited = EmptyDocumentInfo();
    std::wstring dirty_label;
    const int dirty_index = TabCtrl_GetCurSel(
        state.Workspace().windows.document_tabs);
    if (!QueryDocument(state, second_edited)
        || second_edited.main_plane_checksum
            == second_initial.main_plane_checksum
        || (second_edited.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || !ReadDocumentTabLabel(
            state.Workspace().windows.document_tabs, dirty_index, dirty_label)
        || !dirty_label.ends_with(L" *")
        || !AccessibleChildNameContains(
            state.Workspace().windows.document_tabs, dirty_label)) {
        cleanup();
        return 740;
    }

    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    InkpodDocumentInfo second_undone = EmptyDocumentInfo();
    if (!QueryDocument(state, second_undone)
        || second_undone.main_plane_checksum
            != second_initial.main_plane_checksum) {
        cleanup();
        return 742;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    InkpodDocumentInfo second_redone = EmptyDocumentInfo();
    if (!QueryDocument(state, second_redone)
        || second_redone.main_plane_checksum
            != second_edited.main_plane_checksum
        || SaveToPath(state, second_path) != INKPOD_STATUS_OK
        || state.RecentDocumentAt(0U) == nullptr
        || state.RecentDocumentAt(0U)->path != second_path) {
        cleanup();
        return 743;
    }
    InkpodDocumentInfo second_saved = EmptyDocumentInfo();
    const inkpod::app::CommandContext second_async_context =
        state.routing.targets.Capture();
    if (!QueryDocument(state, second_saved)) {
        cleanup();
        return 890;
    }
    if ((second_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        cleanup();
        return 891;
    }
    ActivationRequest activation{};
    activation.request_id = 1U;
    activation.target = ActivationTargetPreference::LastFocusedWorkspace;
    try {
        activation.paths.push_back(first_path);
    } catch (const std::bad_alloc&) {
        cleanup();
        return 892;
    }
    if (!HandleApplicationActivation(state, activation)) {
        cleanup();
        return 892;
    }
    if (state.Document().id != first_session) {
        cleanup();
        return 893;
    }
    if (state.Documents().Count() != baseline_count + 1U) {
        cleanup();
        return 894;
    }
    if (state.RecentDocumentAt(0U) == nullptr
        || state.RecentDocumentAt(0U)->path != first_path) {
        cleanup();
        return 895;
    }
    const std::size_t recent_count_before_missing =
        state.RecentDocumentCount();
    const std::wstring missing_recent_path = first_path + L".missing";
    inkpod::app::DocumentIdentity missing_recent_identity{};
    if (!inkpod::app::ResolveDocumentFileIdentity(
            missing_recent_path, missing_recent_identity)
        || !state.RecordRecentDocument(
            missing_recent_path, std::move(missing_recent_identity))) {
        cleanup();
        return 897;
    }
    UpdateMenuState(state);
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_FILE_RECENT_1,
            0) != 0) {
        cleanup();
        return 898;
    }
    const std::size_t expected_recent_count =
        recent_count_before_missing == inkpod::app::RecentDocumentList::kCapacity
        ? recent_count_before_missing - 1U
        : recent_count_before_missing;
    if (state.RecentDocumentCount() != expected_recent_count) {
        cleanup();
        return 899;
    }
    if (state.RecentDocumentAt(0U) == nullptr) {
        cleanup();
        return 900;
    }
    if (state.RecentDocumentAt(0U)->path != first_path) {
        cleanup();
        return 901;
    }

    const auto first_command_states = state.Workspace().command_states;
    InkpodSelectionInput second_selection{};
    second_selection.struct_size = sizeof(second_selection);
    second_selection.shape = INKPOD_SELECTION_RECTANGLE;
    second_selection.operation = INKPOD_SELECTION_NEW;
    second_selection.bounds = {2, 3, 7, 5};
    second_selection.interpretation = INKPOD_RANGE_NORMAL;
    second_selection.trace_shape = INKPOD_TRACE_ROUND;
    second_selection.view_zoom_q16 = INT64_C(1) << 16;
    if (!state.engine->Enqueue(
            second_async_context,
            [second_selection](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(
                    core, &second_selection, &result);
            },
            true,
            true,
            false)
        || state.engine->WaitIdle(second_session, second_generation)
            != INKPOD_STATUS_OK) {
        cleanup();
        return 930;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo second_after_async = EmptyDocumentInfo();
    InkpodLocatorOutput second_locator{};
    second_locator.struct_size = sizeof(second_locator);
    InkpodLocatorOutput first_locator{};
    first_locator.struct_size = sizeof(first_locator);
    if (state.Document().id != first_session
        || !SameCommandStates(
            first_command_states, state.Workspace().command_states)
        || !state.engine->GetDocumentInfo(
            second_session, second_generation, second_after_async)
        || (second_after_async.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || state.engine->Invoke(
               second_session,
               second_generation,
               [&second_locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, 0U, 0.0, 0.0, &second_locator);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (second_locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U
        || second_locator.selection.x != second_selection.bounds.x
        || second_locator.selection.y != second_selection.bounds.y
        || second_locator.selection.width != second_selection.bounds.width
        || second_locator.selection.height != second_selection.bounds.height
        || state.engine->Invoke(
               [&first_locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, 0U, 0.0, 0.0, &first_locator);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (first_locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U) {
        cleanup();
        return 931;
    }

    InkpodDocumentInfo first_after = EmptyDocumentInfo();
    InkpodHistoryInfo first_history_after{};
    first_history_after.struct_size = sizeof(first_history_after);
    if (!QueryDocument(state, first_after)
        || first_after.document_revision != first_saved.document_revision
        || first_after.main_plane_checksum != first_saved.main_plane_checksum
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&first_history_after](InkpodCore* core) {
                   return inkpod_core_history_info(core, &first_history_after);
               },
               false,
               false) != INKPOD_STATUS_OK
        || first_history_after.cursor != first_history.cursor
        || first_history_after.item_count != first_history.item_count
        || SaveToPath(state, second_path) != INKPOD_STATUS_INVALID_STATE
        || state.Document().shell.current_path != first_path) {
        cleanup();
        return 932;
    }

    const InkpodStrokeSample first_prompt_sample{
        sizeof(InkpodStrokeSample), 0U, 6.0F, 6.0F, 1.0F, 0U};
    const InkpodStrokeInput first_prompt_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        INKPOD_FEATURE_NONE,
        UINT32_C(0x000000ff),
        1.0F,
        &first_prompt_sample,
        1U,
        sizeof(first_prompt_sample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    if (state.engine->Invoke(
            [first_prompt_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &first_prompt_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        cleanup();
        return 933;
    }
    std::uint32_t expected_dirty_prompts{};
    for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
        const auto* document = state.Documents().SessionAt(index);
        InkpodDocumentInfo document_info = EmptyDocumentInfo();
        if (document != nullptr && state.engine->GetDocumentInfo(
                document->id, document->generation, document_info)
            && (document_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
            ++expected_dirty_prompts;
        }
    }
    if (expected_dirty_prompts < 2U) {
        cleanup();
        return 934;
    }
    const auto* first_document = state.Documents().Find(first_session);
    const auto* second_document = state.Documents().Find(second_session);
    if (first_document == nullptr || second_document == nullptr
        || first_document->shell.recovery_path.empty()
        || second_document->shell.recovery_path.empty()
        || !QueueAutosave(
            state,
            state.routing.targets.Capture(),
            first_document->shell.recovery_path)
        || !ActivateDocumentTab(state, second_view)
        || !QueueAutosave(
            state,
            state.routing.targets.Capture(),
            second_document->shell.recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        cleanup();
        return 952;
    }
    std::vector<RecoveryCandidate> multi_recovery_candidates;
    if (!EnumerateRecoveryCandidates(multi_recovery_candidates)) {
        cleanup();
        return 953;
    }
    const auto has_session = [&multi_recovery_candidates](
                                 inkpod::app::DocumentSessionId session) {
        return std::any_of(
            multi_recovery_candidates.begin(),
            multi_recovery_candidates.end(),
            [session](const RecoveryCandidate& candidate) {
                return candidate.has_metadata
                    && candidate.metadata.session == session;
            });
    };
    InkpodDocumentInfo first_after_autosave = EmptyDocumentInfo();
    InkpodDocumentInfo second_after_autosave = EmptyDocumentInfo();
    if (!has_session(first_session) || !has_session(second_session)
        || !state.engine->GetDocumentInfo(
            first_session, first_generation, first_after_autosave)
        || !state.engine->GetDocumentInfo(
            second_session, second_generation, second_after_autosave)
        || (first_after_autosave.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || (second_after_autosave.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || !DiscardRecoveryArtifact(first_document->shell.recovery_path)
        || !DiscardRecoveryArtifact(second_document->shell.recovery_path)
        || !ActivateDocumentTab(state, first_view)) {
        cleanup();
        return 954;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDCANCEL;
    state.lifetime.smoke_dirty_prompt_count = 0U;
    if (ConfirmAllDocuments(state)
        || state.lifetime.smoke_dirty_prompt_count != 1U
        || state.Documents().Count() != baseline_count + 1U) {
        cleanup();
        return 935;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    state.lifetime.smoke_dirty_prompt_count = 0U;
    if (!ConfirmAllDocuments(state)
        || state.lifetime.smoke_dirty_prompt_count != expected_dirty_prompts
        || state.Documents().Count() != baseline_count + 1U
        || !ActivateDocumentTab(state, first_view)) {
        cleanup();
        return 936;
    }

    const std::size_t first_view_count = state.Document().ViewCount();
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_VIEW_NEW,
            0) != 1
        || state.Document().id != first_session
        || state.Document().ViewCount() != first_view_count + 1U
        || !DocumentTabsMatchRegistry(state)
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_VIEW_CLOSE,
               0) != 1
        || state.Document().ViewCount() != first_view_count
        || !DocumentTabsMatchRegistry(state)) {
        cleanup();
        return 746;
    }

    const DocumentViewId before_next =
        state.routing.targets.ActiveDocumentView();
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_TAB_NEXT, 0);
    const DocumentViewId after_next =
        state.routing.targets.ActiveDocumentView();
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_TAB_PREVIOUS, 0);
    if (after_next == before_next
        || state.routing.targets.ActiveDocumentView() != before_next
        || !DocumentTabsMatchRegistry(state)
        || !ActivateDocumentTab(state, second_view)
        || SaveToPath(state, second_path) != INKPOD_STATUS_OK
        || !QueryDocument(state, second_saved)
        || (second_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        cleanup();
        return 747;
    }

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_DOCUMENT_CLOSE,
            0) != 1
        || state.Documents().Count() != baseline_count
        || state.engine->HasSession(second_session, second_generation)
        || state.Documents().Find(second_session) != nullptr
        || OpenDocumentFromPath(state, second_path) != INKPOD_STATUS_OK
        || state.Documents().Count() != baseline_count + 1U
        || state.Document().id == second_session) {
        cleanup();
        return 748;
    }
    InkpodDocumentInfo second_reopened = EmptyDocumentInfo();
    InkpodLocatorOutput reopened_locator{};
    reopened_locator.struct_size = sizeof(reopened_locator);
    if (!QueryDocument(state, second_reopened)
        || !SamePersistentMetadata(second_saved, second_reopened)
        || (second_reopened.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || state.engine->Invoke(
               [&reopened_locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, 0U, 0.0, 0.0, &reopened_locator);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (reopened_locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U
        || reopened_locator.selection.x != second_selection.bounds.x
        || reopened_locator.selection.y != second_selection.bounds.y
        || reopened_locator.selection.width != second_selection.bounds.width
        || reopened_locator.selection.height != second_selection.bounds.height
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_VIEW_CLOSE,
               0) != 1
        || state.Documents().Count() != baseline_count
        || !ActivateDocumentTab(state, first_view)
        || !DocumentTabsMatchRegistry(state)) {
        cleanup();
        return 749;
    }
    cleanup();
    return 0;
}

int RunSplitEditorGroupSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::DocumentSessionId;
    using inkpod::app::DocumentViewId;
    using inkpod::app::EditorSplitOrientation;
    using inkpod::app::Generation;

    auto& editors = state.Workspace().editors;
    auto* first_group = editors.Active();
    if (state.engine == nullptr || state.renderer == nullptr
        || first_group == nullptr || editors.GroupCount() != 1U
        || state.routing.targets.EditorGroupCount() != 1U
        || state.renderer->SurfaceCount() != 2U) {
        return 750;
    }
    const auto first_group_id = first_group->id;
    const auto first_canvas_id = first_group->canvas_id;
    const HWND first_canvas = first_group->canvas;
    const DocumentSessionId shared_session = state.Document().id;
    const Generation shared_generation = state.Document().generation;
    const DocumentViewId first_view = state.ActiveView().id;
    const std::size_t original_view_count = state.Document().ViewCount();

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_SPLIT_RIGHT,
            0) != 1) {
        return 751;
    }
    PumpPendingWindowMessages();
    auto* second_group = editors.Active();
    if (second_group == nullptr || second_group->id == first_group_id
        || editors.GroupCount() != 2U
        || editors.Orientation() != EditorSplitOrientation::Vertical
        || state.routing.targets.EditorGroupCount() != 2U
        || state.renderer->SurfaceCount() != 3U
        || state.Document().id != shared_session
        || state.Document().ViewCount() != original_view_count + 1U
        || second_group->canvas == nullptr
        || second_group->canvas == first_canvas
        || state.Workspace().windows.canvas != second_group->canvas
        || state.Workspace().windows.document_tabs
            != second_group->document_tabs) {
        return 752;
    }
    const auto second_group_id = second_group->id;
    const HWND second_canvas = second_group->canvas;
    const DocumentViewId second_view = second_group->ActiveView();
    const auto* first_document_view = state.Document().FindView(first_view);
    const auto* second_document_view = state.Document().FindView(second_view);
    if (!second_view || first_document_view == nullptr
        || second_document_view == nullptr
        || first_document_view->core_view_id == second_document_view->core_view_id
        || state.routing.targets.GroupForView(first_view) != first_group_id
        || state.routing.targets.GroupForView(second_view) != second_group_id) {
        return 753;
    }

    const bool first_flip_presentation =
        first_document_view->presentation.flip_horizontal;
    const auto query_transform = [&state, shared_session, shared_generation](
                                     std::uint64_t core_view,
                                     InkpodSnapshotTransform& transform) noexcept {
        transform = {};
        transform.struct_size = sizeof(transform);
        const InkpodStatus status = state.engine->Invoke(
                   shared_session,
                   shared_generation,
                   [core_view, &transform](InkpodCore* core) {
                       const InkpodSnapshotOptions options{
                           sizeof(InkpodSnapshotOptions),
                           0U,
                           INKPOD_FEATURE_NONE};
                       InkpodSnapshot* snapshot{};
                       const InkpodStatus built = core_view == 0U
                           ? inkpod_core_build_snapshot(
                                 core, &options, &snapshot)
                           : inkpod_core_build_snapshot_for_view(
                                 core, core_view, &options, &snapshot);
                       if (built != INKPOD_STATUS_OK) {
                           return built;
                       }
                       const InkpodStatus read =
                           inkpod_snapshot_get_transform(snapshot, &transform);
                       const InkpodStatus released =
                           inkpod_snapshot_release(&snapshot);
                       return read == INKPOD_STATUS_OK ? released : read;
                   },
                   false,
                   false);
        if (status != INKPOD_STATUS_OK) {
            std::fprintf(
                stderr,
                "G6 transform query failed: core_view=%llu status=%u\n",
                static_cast<unsigned long long>(core_view),
                static_cast<unsigned int>(status));
        }
        return status == INKPOD_STATUS_OK;
    };
    InkpodSnapshotTransform first_before_flip{};
    InkpodSnapshotTransform second_before_flip{};
    InkpodSnapshotTransform first_after_flip{};
    InkpodSnapshotTransform second_after_flip{};
    if (!query_transform(first_document_view->core_view_id, first_before_flip)) {
        return 774;
    }
    if (!query_transform(second_document_view->core_view_id, second_before_flip)) {
        return 775;
    }
    if (SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_VIEW_FLIP_HORIZONTAL,
               0) != 0
        || !state.ActiveView().presentation.flip_horizontal) {
        return 754;
    }
    if (!query_transform(first_document_view->core_view_id, first_after_flip)
        || !query_transform(second_document_view->core_view_id, second_after_flip)
        || (first_before_flip.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            != (first_after_flip.flags
                & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
        || (second_before_flip.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            == (second_after_flip.flags
                & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)) {
        return 755;
    }

    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    if (editors.Active() == nullptr || editors.Active()->id != first_group_id
        || state.ActiveView().id != first_view
        || state.ActiveView().presentation.flip_horizontal
            != first_flip_presentation) {
        return 756;
    }
    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    if (editors.Active() == nullptr || editors.Active()->id != second_group_id
        || state.ActiveView().id != second_view
        || !state.ActiveView().presentation.flip_horizontal) {
        return 757;
    }
    SetFocus(state.Workspace().windows.tool_palette);
    if (editors.Active() == nullptr || editors.Active()->id != second_group_id
        || state.ActiveView().id != second_view) {
        return 782;
    }
    const HWND first_tabs = editors.Find(first_group_id)->document_tabs;
    SetFocus(first_tabs);
    NMHDR first_tab_focus{};
    first_tab_focus.hwndFrom = first_tabs;
    first_tab_focus.idFrom = IDC_MAIN_DOCUMENT_TABS;
    first_tab_focus.code = NM_SETFOCUS;
    SendMessageW(
        state.Workspace().windows.window,
        WM_NOTIFY,
        first_tab_focus.idFrom,
        reinterpret_cast<LPARAM>(&first_tab_focus));
    if (editors.Active() == nullptr || editors.Active()->id != first_group_id
        || state.ActiveView().id != first_view
        || editors.Active()->focus_history != first_tabs) {
        return 783;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_COLOR_PIN,
            0) != 1
        || state.routing.pane_targets.Find(state.routing.color_pane)->policy
            != inkpod::app::PaneTargetPolicy::PinnedDocument
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_COLOR_PIN,
               0) != 1
        || state.routing.pane_targets.Find(state.routing.color_pane)->policy
            != inkpod::app::PaneTargetPolicy::FollowActiveView) {
        return 924;
    }
    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);

    const std::int32_t split_document_x = static_cast<std::int32_t>(
        second_after_flip.document_width / 3U);
    const std::int32_t split_document_y = static_cast<std::int32_t>(
        second_after_flip.document_height / 3U);
    const double split_document_center_x =
        static_cast<double>(split_document_x) + 0.5;
    const double split_document_center_y =
        static_cast<double>(split_document_y) + 0.5;
    const double split_device_x =
        ((second_after_flip.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) != 0U
             ? static_cast<double>(second_after_flip.document_width)
                 - split_document_center_x
             : split_document_center_x)
            * second_after_flip.zoom
        + second_after_flip.pan_x;
    const double split_device_y =
        ((second_after_flip.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL) != 0U
             ? static_cast<double>(second_after_flip.document_height)
                 - split_document_center_y
             : split_document_center_y)
            * second_after_flip.zoom
        + second_after_flip.pan_y;
    InkpodLocatorOutput split_locator{};
    split_locator.struct_size = sizeof(split_locator);
    if (second_after_flip.document_width == 0U
        || second_after_flip.document_height == 0U
        || !std::isfinite(split_device_x) || !std::isfinite(split_device_y)
        || state.engine->Invoke(
               shared_session,
               shared_generation,
               [core_view = second_document_view->core_view_id,
                split_device_x,
                split_device_y,
                &split_locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core,
                       core_view,
                       split_device_x,
                       split_device_y,
                       &split_locator);
               },
               false,
               false) != INKPOD_STATUS_OK
        || split_locator.document_x != split_document_x
        || split_locator.document_y != split_document_y) {
        return 1050;
    }

    const InkpodStrokeSample split_document_sample{
        sizeof(InkpodStrokeSample),
        0U,
        static_cast<float>(split_document_center_x),
        static_cast<float>(split_document_center_y),
        1.0F,
        0U};
    const auto apply_split_document_sample =
        [&state,
         shared_session,
         shared_generation,
         split_document_sample](InkpodPaintTool tool) noexcept {
            return state.engine->Invoke(
                shared_session,
                shared_generation,
                [split_document_sample, tool](InkpodCore* core) {
                    const InkpodStrokeInput input{
                        sizeof(InkpodStrokeInput),
                        tool,
                        INKPOD_PLANE_MAIN_LINE,
                        INKPOD_COORDINATE_SPACE_DOCUMENT,
                        INKPOD_FEATURE_NONE,
                        UINT32_C(0x000000ff),
                        1.0F,
                        &split_document_sample,
                        1U,
                        sizeof(split_document_sample),
                        INKPOD_BRUSH_ROUND,
                        0U,
                        0U,
                        INKPOD_START_COLOR_ANY,
                        0U};
                    InkpodDispatchResult result{};
                    result.struct_size = sizeof(result);
                    InkpodStatus status = inkpod_core_set_active_plane(
                        core, INKPOD_PLANE_MAIN_LINE);
                    if (status == INKPOD_STATUS_OK) {
                        status = inkpod_core_apply_stroke(core, &input, &result);
                    }
                    return status;
                },
                true,
                true);
        };
    InkpodDocumentInfo split_cleared = EmptyDocumentInfo();
    InkpodDocumentInfo split_seeded = EmptyDocumentInfo();
    InkpodDocumentInfo split_erased = EmptyDocumentInfo();
    if (apply_split_document_sample(INKPOD_TOOL_ERASER) != INKPOD_STATUS_OK
        || !QueryDocument(state, split_cleared)
        || apply_split_document_sample(INKPOD_TOOL_PENCIL) != INKPOD_STATUS_OK
        || !QueryDocument(state, split_seeded)
        || split_seeded.main_plane_checksum == split_cleared.main_plane_checksum
        || SetEditorActiveTarget(
               state, split_seeded.layer_id, split_seeded.main_plane_id)
            != INKPOD_STATUS_OK
        || SetEditorActiveTool(state, INKPOD_TOOL_PENCIL) != INKPOD_STATUS_OK) {
        return 1051;
    }
    const InkpodStrokeSample split_device_sample{
        sizeof(InkpodStrokeSample),
        0U,
        static_cast<float>(split_device_x),
        static_cast<float>(split_device_y),
        1.0F,
        0U};
    if (!renderer::SubmitCanvasStrokeEvent(
            second_canvas,
            renderer::CanvasStrokeEventKind::Begin,
            &split_device_sample,
            1U)
        || !renderer::SubmitCanvasStrokeEvent(
            second_canvas,
            renderer::CanvasStrokeEventKind::End,
            &split_device_sample,
            1U)
        || state.engine->WaitIdle(shared_session, shared_generation)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, split_erased)
        || split_erased.document_revision != split_seeded.document_revision + 1U
        || split_erased.main_plane_checksum != split_cleared.main_plane_checksum) {
        return 1052;
    }

    InkpodDocumentInfo before_edit = EmptyDocumentInfo();
    InkpodDocumentInfo after_edit = EmptyDocumentInfo();
    const InkpodStrokeSample shared_sample{
        sizeof(InkpodStrokeSample), 0U, 9.0F, 9.0F, 1.0F, 0U};
    InkpodSelectionInput shared_selection{};
    shared_selection.struct_size = sizeof(shared_selection);
    shared_selection.shape = INKPOD_SELECTION_RECTANGLE;
    shared_selection.operation = INKPOD_SELECTION_NEW;
    shared_selection.bounds = {3, 4, 7, 5};
    shared_selection.interpretation = INKPOD_RANGE_NORMAL;
    shared_selection.trace_shape = INKPOD_TRACE_ROUND;
    shared_selection.view_zoom_q16 = INT64_C(1) << 16;
    const auto query_selection = [&state, shared_session, shared_generation](
                                     InkpodLocatorOutput& output) noexcept {
        output = {};
        output.struct_size = sizeof(output);
        return state.engine->Invoke(
                   shared_session,
                   shared_generation,
                   [&output](InkpodCore* core) {
                       return inkpod_core_locator_sample(
                           core, 0U, 0.0, 0.0, &output);
                   },
                   false,
                   false)
            == INKPOD_STATUS_OK;
    };
    InkpodLocatorOutput before_selection{};
    if (!QueryDocument(state, before_edit)) {
        return 776;
    }
    if (!query_selection(before_selection)
        || (before_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U) {
        return 979;
    }
    const InkpodStatus shared_edit_status = state.engine->Invoke(
        shared_session,
        shared_generation,
        [shared_selection](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_apply_selection(
                core, &shared_selection, &result);
        },
        true,
        true);
    if (shared_edit_status != INKPOD_STATUS_OK) {
        std::fprintf(
            stderr,
            "G6 shared edit failed: status=%u\n",
            static_cast<unsigned int>(shared_edit_status));
        return 777;
    }
    if (!QueryDocument(state, after_edit)) {
        return 778;
    }
    InkpodLocatorOutput after_selection{};
    if (after_edit.document_revision != before_edit.document_revision + 1U
        || !query_selection(after_selection)
        || (after_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U) {
        std::fprintf(
            stderr,
            "G6 shared edit mismatch: revisions=%llu/%llu selection_flags=%u\n",
            static_cast<unsigned long long>(before_edit.document_revision),
            static_cast<unsigned long long>(after_edit.document_revision),
            static_cast<unsigned int>(after_selection.flags));
        return 758;
    }
    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    InkpodDocumentInfo shared_from_first = EmptyDocumentInfo();
    InkpodLocatorOutput selection_from_first{};
    if (!QueryDocument(state, shared_from_first)
        || shared_from_first.document_revision != after_edit.document_revision
        || !query_selection(selection_from_first)
        || (selection_from_first.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_UNDO,
               0) != 0) {
        return 759;
    }
    InkpodDocumentInfo undone = EmptyDocumentInfo();
    InkpodLocatorOutput undone_selection{};
    if (!QueryDocument(state, undone)
        || !query_selection(undone_selection)
        || (undone_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_REDO,
               0) != 0) {
        return 760;
    }
    InkpodDocumentInfo redone = EmptyDocumentInfo();
    InkpodLocatorOutput redone_selection{};
    if (!QueryDocument(state, redone)
        || !query_selection(redone_selection)
        || (redone_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U) {
        return 761;
    }

    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    InkpodDocumentInfo before_cancel = EmptyDocumentInfo();
    if (!QueryDocument(state, before_cancel)
        || !renderer::SubmitCanvasStrokeEvent(
            second_canvas,
            renderer::CanvasStrokeEventKind::Begin,
            &shared_sample,
            1U)) {
        return 762;
    }
    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    InkpodDocumentInfo after_cancel = EmptyDocumentInfo();
    if (state.engine->WaitIdle(shared_session, shared_generation)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, after_cancel)
        || after_cancel.document_revision != before_cancel.document_revision) {
        return 763;
    }
    if (!renderer::SubmitCanvasStrokeEvent(
            second_canvas,
            renderer::CanvasStrokeEventKind::Cancel,
            nullptr,
            0U)
        || state.engine->WaitIdle(shared_session, shared_generation)
            != INKPOD_STATUS_OK) {
        return 785;
    }

    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    const InkpodDocumentInfo shared_before_other_document = after_cancel;
    if (CreateCell(state, 40U, 30U, 96000U) != INKPOD_STATUS_OK
        || state.Document().id == shared_session
        || editors.Active() == nullptr
        || editors.Active()->id != second_group_id) {
        return 764;
    }
    const DocumentSessionId isolated_session = state.Document().id;
    const Generation isolated_generation = state.Document().generation;
    if (state.engine->Invoke(
            isolated_session,
            isolated_generation,
            [shared_selection](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(
                    core, &shared_selection, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 765;
    }
    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    InkpodDocumentInfo shared_after_other_document = EmptyDocumentInfo();
    if (!QueryDocument(state, shared_after_other_document)
        || shared_after_other_document.document_revision
            != shared_before_other_document.document_revision) {
        return 766;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    if (state.Document().id != isolated_session) {
        return 780;
    }
    const LRESULT isolated_close = SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_DOCUMENT_CLOSE,
        0);
    if (isolated_close != 1) {
        std::fprintf(
            stderr,
            "G6 isolated document close failed: result=%lld active=%llu expected=%llu\n",
            static_cast<long long>(isolated_close),
            static_cast<unsigned long long>(state.Document().id.Value()),
            static_cast<unsigned long long>(isolated_session.Value()));
        return 781;
    }
    if (state.engine->HasSession(isolated_session, isolated_generation)
        || state.Documents().Find(isolated_session) != nullptr
        || state.Document().id != shared_session) {
        std::fprintf(
            stderr,
            "G6 isolated close residue: engine=%d registry=%d active=%llu shared=%llu\n",
            state.engine->HasSession(isolated_session, isolated_generation) ? 1 : 0,
            state.Documents().Find(isolated_session) != nullptr ? 1 : 0,
            static_cast<unsigned long long>(state.Document().id.Value()),
            static_cast<unsigned long long>(shared_session.Value()));
        return 767;
    }

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_SPLIT_DOWN,
            0) != 1
        || editors.Orientation() != EditorSplitOrientation::Horizontal) {
        return 768;
    }
    editors.SetSplitRatioMilli(1U);
    RECT client{};
    if (editors.SplitRatioMilli() != 200U
        || GetClientRect(state.Workspace().windows.window, &client) == FALSE) {
        return 769;
    }
    LayoutMainChrome(
        state.Workspace().windows,
        state.lifetime.smoke_test,
        client.right - client.left,
        client.bottom - client.top);
    editors.SetSplitRatioMilli(999U);
    if (editors.SplitRatioMilli() != 800U) {
        return 770;
    }
    LayoutMainChrome(
        state.Workspace().windows,
        state.lifetime.smoke_test,
        client.right - client.left,
        client.bottom - client.top);
    editors.SetSplitRatioMilli(500U);
    SendMessageW(
        state.Workspace().windows.window,
        WM_SIZE,
        SIZE_RESTORED,
        MAKELPARAM(client.right - client.left, client.bottom - client.top));
    SendMessageW(second_canvas, WM_DPICHANGED_AFTERPARENT, 0, 0);
    if (editors.GroupCount() != 2U || state.renderer->SurfaceCount() != 3U
        || IsWindow(first_canvas) == FALSE || IsWindow(second_canvas) == FALSE) {
        return 784;
    }

    const std::size_t before_move_views = state.Document().ViewCount();
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_MOVE_OTHER_GROUP,
            0) != 1
        || state.Document().ViewCount() != before_move_views
        || editors.Active() == nullptr
        || editors.Active()->id != first_group_id
        || editors.Find(second_group_id) == nullptr
        || editors.Find(second_group_id)->ViewCount() != 0U
        || renderer::GetCanvasSnapshotSink(second_canvas)->AcceptsSnapshots()) {
        return 771;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_NEW_VIEW_OTHER_GROUP,
            0) != 1
        || state.Document().ViewCount() != before_move_views + 1U
        || editors.Active() == nullptr
        || editors.Active()->id != second_group_id
        || editors.Active()->ViewCount() != 1U
        || !renderer::GetCanvasSnapshotSink(second_canvas)->AcceptsSnapshots()) {
        return 772;
    }
    if (!HandleWorkspaceNavigation(
            state,
            state.Workspace().windows.window,
            VK_F6,
            INKPOD_SHORTCUT_MODIFIER_CONTROL | INKPOD_SHORTCUT_MODIFIER_SHIFT)) {
        return 1001;
    }
    if (editors.Active() == nullptr || editors.Active()->id != first_group_id) {
        return 1006;
    }
    if (!HandleWorkspaceNavigation(
            state,
            state.Workspace().windows.window,
            VK_F6,
            INKPOD_SHORTCUT_MODIFIER_CONTROL)) {
        return 1007;
    }
    if (editors.Active() == nullptr || editors.Active()->id != second_group_id) {
        return 1008;
    }
    ShowWindow(state.Workspace().windows.status_bar, SW_SHOWNA);
    SetFocus(second_canvas);
    if (!RouteKeyboardKey(state, VK_F6, false, false)
        || GetFocus() != state.Workspace().windows.status_bar) {
        return 10021;
    }
    if (!HandleWorkspaceNavigation(
            state,
            state.Workspace().windows.window,
            VK_F6,
            INKPOD_SHORTCUT_MODIFIER_SHIFT)
        || (GetFocus() != second_canvas
            && GetFocus() != editors.Find(second_group_id)->document_tabs)) {
        return 1003;
    }
    if (!HandleWorkspaceNavigation(
            state,
            state.Workspace().windows.window,
            VK_F6,
            INKPOD_SHORTCUT_MODIFIER_SHIFT)
        || (GetFocus() != state.Workspace().windows.tool_palette
            && IsChild(state.Workspace().windows.tool_palette, GetFocus()) == FALSE)) {
        return 1004;
    }
    if (!HandleWorkspaceNavigation(
            state,
            state.Workspace().windows.window,
            VK_F6,
            0U)
        || (GetFocus() != second_canvas
            && GetFocus() != editors.Find(second_group_id)->document_tabs)) {
        return 1005;
    }
    ShowWindow(state.Workspace().windows.status_bar, SW_HIDE);
    const std::size_t before_close_views = state.Document().ViewCount();
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_GROUP_NEXT,
            0) != 1
        || editors.Active() == nullptr
        || editors.Active()->id != first_group_id
        || editors.Active()->focus_history != first_tabs
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDITOR_GROUP_CLOSE,
               0) != 1
        || editors.GroupCount() != 1U
        || editors.Orientation() != EditorSplitOrientation::None
        || state.routing.targets.EditorGroupCount() != 1U
        || state.renderer->SurfaceCount() != 2U
        || state.Document().ViewCount() != before_close_views
        || editors.Active() == nullptr
        || editors.Active()->id != second_group_id
        || state.Workspace().windows.canvas != editors.Active()->canvas
        || state.routing.targets.Canvas() != editors.Active()->canvas_id
        || editors.FindByCanvas(first_canvas_id) != nullptr) {
        return 773;
    }
    return 0;
}

int RunCommandContextSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::CommandContext;
    using inkpod::app::CommandResolveStatus;
    using inkpod::app::Generation;

    const CommandContext context = state.routing.targets.Capture();
    if (state.routing.targets.Resolve(
            context, inkpod::app::kDocumentViewCommandScope)
        != CommandResolveStatus::Ok) {
        return 727;
    }
    CommandContext missing = context;
    missing.document_view.reset();
    if (state.routing.targets.Resolve(
            missing, inkpod::app::kDocumentViewCommandScope)
        != CommandResolveStatus::MissingScope) {
        return 728;
    }
    CommandContext stale = context;
    stale.generation = Generation(
        state.routing.targets.CurrentGeneration().Value() + 1U);
    if (state.routing.targets.Resolve(
            stale, inkpod::app::kDocumentViewCommandScope)
        != CommandResolveStatus::StaleGeneration) {
        return 729;
    }
    InkpodDocumentInfo before = EmptyDocumentInfo();
    InkpodDocumentInfo after = EmptyDocumentInfo();
    if (!QueryDocument(state, before)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, UINT16_MAX, 0) != 0
        || !QueryDocument(state, after)
        || before.document_revision != after.document_revision
        || before.main_plane_checksum != after.main_plane_checksum
        || IssueCommand(
               nullptr,
               state.Workspace().windows.window,
               IDM_EDIT_UNDO,
               0,
               std::nullopt)
                .value_or(1)
            != 0) {
        return 730;
    }
    return 0;
}

int RunTabDragSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::DocumentViewId;
    using inkpod::app::DragOperation;
    using inkpod::app::JobSessionId;
    using inkpod::app::WorkspaceWindow;
    using inkpod::app::WorkspaceWindowId;

    auto* group = state.Workspace().editors.Active();
    if (group == nullptr || group->document_tabs == nullptr
        || state.Workspaces().Count() != 1U) {
        return 950;
    }
    while (group->ViewCount() < 2U) {
        if (SendMessageW(
                state.Workspace().windows.window,
                WM_COMMAND,
                IDM_VIEW_NEW,
                0) != 1) {
            return 951;
        }
        group = state.Workspace().editors.Active();
    }
    SetWindowPos(
        group->document_tabs,
        nullptr,
        96,
        64,
        640,
        32,
        SWP_NOACTIVATE | SWP_NOZORDER);
    const auto begin_drag = [](HWND tabs, int index, POINT target, bool copy) noexcept {
        RECT item{};
        if (TabCtrl_GetItemRect(tabs, index, &item) == FALSE) {
            return false;
        }
        std::array<BYTE, 256U> keyboard{};
        GetKeyboardState(keyboard.data());
        const BYTE previous_control = keyboard[VK_CONTROL];
        keyboard[VK_CONTROL] = copy ? static_cast<BYTE>(0x80U) : 0U;
        SetKeyboardState(keyboard.data());
        const LPARAM start = MAKELPARAM(
            (item.left + item.right) / 2,
            (item.top + item.bottom) / 2);
        SendMessageW(
            tabs,
            WM_LBUTTONDOWN,
            MK_LBUTTON | (copy ? MK_CONTROL : 0U),
            start);
        POINT client = target;
        ScreenToClient(tabs, &client);
        SendMessageW(
            tabs,
            WM_MOUSEMOVE,
            MK_LBUTTON | (copy ? MK_CONTROL : 0U),
            MAKELPARAM(client.x, client.y));
        keyboard[VK_CONTROL] = previous_control;
        SetKeyboardState(keyboard.data());
        return true;
    };
    const auto finish_drag = [](HWND tabs, POINT target) noexcept {
        POINT client = target;
        ScreenToClient(tabs, &client);
        SendMessageW(
            tabs, WM_LBUTTONUP, 0, MAKELPARAM(client.x, client.y));
        PumpPendingWindowMessages();
    };

    InkpodDocumentInfo before{};
    InkpodDocumentInfo after{};
    const DocumentViewId first = group->ViewAt(0U);
    const DocumentViewId second = group->ViewAt(1U);
    auto* reordered_document = state.Documents().FindByView(first);
    InkpodHistoryInfo history_before{};
    InkpodHistoryInfo history_after{};
    history_before.struct_size = sizeof(history_before);
    history_after.struct_size = sizeof(history_after);
    if (reordered_document == nullptr || !state.ActivateDocumentView(first)
        || state.engine->Invoke(
               reordered_document->id,
               reordered_document->generation,
               [&history_before](InkpodCore* core) {
                   return inkpod_core_history_info(core, &history_before);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        return 952;
    }
    TabCtrl_SetCurSel(group->document_tabs, 0);
    TabCtrl_SetCurFocus(group->document_tabs, 0);
    RECT second_tab_bounds{};
    if (TabCtrl_GetItemRect(
            group->document_tabs, 1, &second_tab_bounds) == FALSE) {
        return 952;
    }
    POINT after_second{
        second_tab_bounds.left
            + (second_tab_bounds.right - second_tab_bounds.left) * 3 / 4,
        (second_tab_bounds.top + second_tab_bounds.bottom) / 2};
    ClientToScreen(group->document_tabs, &after_second);
    if (!QueryDocument(state, before)
        || !begin_drag(group->document_tabs, 0, after_second, false)
        || !state.TabDrag().IsDragging()) {
        return 952;
    }
    if (state.TabDrag().Target().kind
            != inkpod::app::TabDropKind::Reorder) {
        RECT window_bounds{};
        RECT tab_bounds{};
        GetWindowRect(state.Workspace().windows.window, &window_bounds);
        GetWindowRect(group->document_tabs, &tab_bounds);
        std::fprintf(
            stderr,
            "G11 target mismatch: kind=%u point=%ld,%ld window=%ld,%ld,%ld,%ld tabs=%ld,%ld,%ld,%ld visible=%d\n",
            static_cast<unsigned int>(state.TabDrag().Target().kind),
            after_second.x,
            after_second.y,
            window_bounds.left,
            window_bounds.top,
            window_bounds.right,
            window_bounds.bottom,
            tab_bounds.left,
            tab_bounds.top,
            tab_bounds.right,
            tab_bounds.bottom,
            IsWindowVisible(state.Workspace().windows.window) != FALSE ? 1 : 0);
    }
    finish_drag(group->document_tabs, after_second);
    group = state.Workspace().editors.Active();
    if (group == nullptr || group->ViewAt(0U) != second
        || group->ViewAt(1U) != first
        || !QueryDocument(state, after)
        || after.document_revision != before.document_revision
        || after.main_plane_checksum != before.main_plane_checksum
        || (after.flags & INKPOD_DOCUMENT_FLAG_DIRTY)
            != (before.flags & INKPOD_DOCUMENT_FLAG_DIRTY)
        || state.engine->Invoke(
               reordered_document->id,
               reordered_document->generation,
               [&history_after](InkpodCore* core) {
                   return inkpod_core_history_info(core, &history_after);
               },
               false,
               false) != INKPOD_STATUS_OK
        || history_after.cursor != history_before.cursor
        || history_after.item_count != history_before.item_count) {
        std::fprintf(
            stderr,
            "G11 reorder mismatch: order=%llu,%llu expected=%llu,%llu rev=%llu/%llu checksum=%llu/%llu flags=%u/%u armed=%d\n",
            static_cast<unsigned long long>(
                group == nullptr ? 0U : group->ViewAt(0U).Value()),
            static_cast<unsigned long long>(
                group == nullptr ? 0U : group->ViewAt(1U).Value()),
            static_cast<unsigned long long>(second.Value()),
            static_cast<unsigned long long>(first.Value()),
            static_cast<unsigned long long>(before.document_revision),
            static_cast<unsigned long long>(after.document_revision),
            static_cast<unsigned long long>(before.main_plane_checksum),
            static_cast<unsigned long long>(after.main_plane_checksum),
            before.flags,
            after.flags,
            state.TabDrag().IsArmed() ? 1 : 0);
        return 953;
    }

    const DocumentViewId cancel_first = group->ViewAt(0U);
    const DocumentViewId cancel_second = group->ViewAt(1U);
    if (!begin_drag(group->document_tabs, 0, after_second, false)
        || !state.TabDrag().IsDragging()) {
        return 954;
    }
    SendMessageW(group->document_tabs, WM_KEYDOWN, VK_ESCAPE, 0);
    PumpPendingWindowMessages();
    if (state.TabDrag().IsArmed() || group->ViewAt(0U) != cancel_first
        || group->ViewAt(1U) != cancel_second) {
        return 955;
    }
    if (!begin_drag(group->document_tabs, 0, after_second, false)) {
        return 956;
    }
    SendMessageW(group->document_tabs, WM_CAPTURECHANGED, 0, 0);
    if (state.TabDrag().IsArmed() || group->ViewAt(0U) != cancel_first) {
        return 957;
    }
    if (!begin_drag(group->document_tabs, 0, after_second, false)) {
        return 958;
    }
    RECT dpi_bounds{};
    GetWindowRect(state.Workspace().windows.window, &dpi_bounds);
    SendMessageW(
        state.Workspace().windows.window,
        WM_DPICHANGED,
        MAKELONG(192, 192),
        reinterpret_cast<LPARAM>(&dpi_bounds));
    if (state.TabDrag().IsArmed() || group->ViewAt(0U) != cancel_first) {
        return 959;
    }
    state.Workspace().tools.floating_active = true;
    if (!begin_drag(group->document_tabs, 0, after_second, false)
        || state.TabDrag().IsArmed()) {
        state.Workspace().tools.floating_active = false;
        return 960;
    }
    state.Workspace().tools.floating_active = false;
    auto* drag_document = state.Documents().FindByView(group->ViewAt(0U));
    auto* drag_view = drag_document == nullptr
        ? nullptr
        : drag_document->FindView(group->ViewAt(0U));
    if (drag_view == nullptr) {
        return 961;
    }
    drag_view->presentation.active_drag = state.routing.tokens.IssueDrag(
        state.routing.targets.Capture(), DragOperation::CanvasStroke);
    if (!begin_drag(group->document_tabs, 0, after_second, false)
        || state.TabDrag().IsArmed()) {
        drag_view->presentation.active_drag.reset();
        return 961;
    }
    drag_view->presentation.active_drag.reset();

    state.effects.task = reinterpret_cast<InkpodTask*>(group);
    if (!begin_drag(group->document_tabs, 0, after_second, false)
        || state.TabDrag().IsArmed()) {
        state.effects.task = nullptr;
        return 983;
    }
    state.effects.task = nullptr;

    state.batch.task = reinterpret_cast<InkpodBatchTask*>(group);
    state.batch.job_id = JobSessionId{0xB11U};
    if (!begin_drag(group->document_tabs, 0, after_second, false)
        || !state.TabDrag().IsDragging()) {
        state.batch.task = nullptr;
        state.batch.job_id.reset();
        return 984;
    }
    state.batch.task = nullptr;
    state.batch.job_id.reset();
    SendMessageW(group->document_tabs, WM_KEYDOWN, VK_ESCAPE, 0);
    if (state.TabDrag().IsArmed()) {
        return 984;
    }

    const DocumentViewId command_view = group->ViewAt(1U);
    if (!state.ActivateDocumentView(command_view)
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_TAB_MOVE_LEFT,
               0) != 1
        || group->ViewAt(0U) != command_view
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_TAB_MOVE_RIGHT,
               0) != 1
        || group->ViewAt(1U) != command_view) {
        return 962;
    }

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_SPLIT_RIGHT,
            0) != 1) {
        return 963;
    }
    auto* source_group = state.Workspace().editors.GroupAt(0U);
    auto* target_group = state.Workspace().editors.GroupAt(1U);
    if (source_group == nullptr || target_group == nullptr
        || source_group->ViewCount() == 0U) {
        return 964;
    }
    SetWindowPos(
        source_group->document_tabs,
        nullptr,
        64,
        64,
        360,
        32,
        SWP_NOACTIVATE | SWP_NOZORDER);
    SetWindowPos(
        target_group->document_tabs,
        nullptr,
        520,
        64,
        360,
        32,
        SWP_NOACTIVATE | SWP_NOZORDER);
    const DocumentViewId group_move_view = source_group->ViewAt(0U);
    RECT target_canvas{};
    GetWindowRect(target_group->canvas, &target_canvas);
    const POINT target_canvas_point{
        (target_canvas.left + target_canvas.right) / 2,
        (target_canvas.top + target_canvas.bottom) / 2};
    if (!begin_drag(source_group->document_tabs, 0, target_canvas_point, false)) {
        return 965;
    }
    finish_drag(source_group->document_tabs, target_canvas_point);
    if (source_group->Contains(group_move_view)
        || !target_group->Contains(group_move_view)
        || state.routing.targets.GroupForView(group_move_view) != target_group->id) {
        return 966;
    }
    SendMessageW(target_group->canvas, WM_SETFOCUS, 0, 0);
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_MOVE_OTHER_GROUP,
            0) != 1) {
        return 967;
    }
    SendMessageW(target_group->canvas, WM_SETFOCUS, 0, 0);
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_GROUP_CLOSE,
            0) != 1
        || state.Workspace().editors.GroupCount() != 1U) {
        return 968;
    }

    WorkspaceWindow* source_workspace = &state.Workspace();
    const WorkspaceWindowId source_workspace_id = source_workspace->id;
    group = source_workspace->editors.Active();
    SetWindowPos(
        group->document_tabs,
        nullptr,
        96,
        64,
        640,
        32,
        SWP_NOACTIVATE | SWP_NOZORDER);
    const DocumentViewId window_move_view = group->ViewAt(0U);
    if (!state.ActivateDocumentView(window_move_view)
        || SendMessageW(
               source_workspace->windows.window,
               WM_COMMAND,
               IDM_WORKSPACE_NEW_WINDOW,
               0) != 1
        || state.Workspaces().Count() != 2U) {
        return 969;
    }
    WorkspaceWindow* destination = state.Workspaces().At(1U);
    if (destination == nullptr || destination->id == source_workspace_id) {
        return 970;
    }
    RECT source_bounds{};
    GetWindowRect(source_workspace->windows.window, &source_bounds);
    SetWindowPos(
        destination->windows.window,
        nullptr,
        source_bounds.right + 64,
        source_bounds.top,
        source_bounds.right - source_bounds.left,
        source_bounds.bottom - source_bounds.top,
        SWP_NOACTIVATE | SWP_NOZORDER);
    RECT destination_canvas{};
    GetWindowRect(destination->windows.canvas, &destination_canvas);
    const POINT destination_point{
        (destination_canvas.left + destination_canvas.right) / 2,
        (destination_canvas.top + destination_canvas.bottom) / 2};
    if (!begin_drag(group->document_tabs, 0, destination_point, false)) {
        return 971;
    }
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    if (state.Workspaces().Count() != 1U || state.TabDrag().IsArmed()
        || !group->Contains(window_move_view)) {
        return 972;
    }

    ActivationRequest new_workspace_activation{};
    new_workspace_activation.request_id = 2U;
    new_workspace_activation.target = ActivationTargetPreference::NewWorkspace;
    if (!HandleApplicationActivation(state, new_workspace_activation)) {
        return 973;
    }
    destination = state.Workspaces().At(1U);
    if (destination == nullptr) {
        return 974;
    }
    SetWindowPos(
        destination->windows.window,
        nullptr,
        source_bounds.right + 64,
        source_bounds.top,
        source_bounds.right - source_bounds.left,
        source_bounds.bottom - source_bounds.top,
        SWP_NOACTIVATE | SWP_NOZORDER);
    GetWindowRect(destination->windows.canvas, &destination_canvas);
    const POINT move_point{
        (destination_canvas.left + destination_canvas.right) / 2,
        (destination_canvas.top + destination_canvas.bottom) / 2};
    if (!begin_drag(group->document_tabs, 0, move_point, false)) {
        return 975;
    }
    finish_drag(group->document_tabs, move_point);
    if (group->Contains(window_move_view)
        || destination->editors.Active() == nullptr
        || !destination->editors.Active()->Contains(window_move_view)) {
        return 976;
    }
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    if (state.Workspaces().Count() != 1U) {
        return 977;
    }

    source_workspace = state.Workspaces().At(0U);
    group = source_workspace == nullptr ? nullptr : source_workspace->editors.Active();
    if (group == nullptr || group->ViewCount() == 0U) {
        return 978;
    }
    const DocumentViewId copy_source = group->ViewAt(0U);
    auto* copy_document = state.Documents().FindByView(copy_source);
    if (copy_document == nullptr) {
        return 979;
    }
    const std::size_t before_copy_views = copy_document->ViewCount();
    RECT workspace_bounds{};
    GetWindowRect(source_workspace->windows.window, &workspace_bounds);
    const POINT outside{
        workspace_bounds.right + 320,
        workspace_bounds.bottom + 160};
    if (!begin_drag(group->document_tabs, 0, outside, true)) {
        return 979;
    }
    finish_drag(group->document_tabs, outside);
    if (state.Workspaces().Count() != 2U
        || !group->Contains(copy_source)
        || copy_document->ViewCount() != before_copy_views + 1U) {
        std::fprintf(
            stderr,
            "G11 copy tear-out mismatch: workspaces=%zu source=%d views=%zu/%zu current=%llu source_view=%llu\n",
            state.Workspaces().Count(),
            group->Contains(copy_source) ? 1 : 0,
            copy_document->ViewCount(),
            before_copy_views,
            static_cast<unsigned long long>(state.ActiveView().id.Value()),
            static_cast<unsigned long long>(copy_source.Value()));
        return 980;
    }
    destination = state.Workspaces().At(1U);
    if (destination == nullptr || destination->editors.Active() == nullptr
        || destination->editors.Active()->ViewCount() != 1U) {
        return 981;
    }
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    return state.Workspaces().Count() == 1U ? 0 : 982;
}

int RunG13ResourceScenarioSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::DocumentSessionId;
    using inkpod::app::DocumentViewId;
    using inkpod::app::Generation;
    using inkpod::app::WorkspaceWindow;
    using inkpod::app::WorkspaceWindowId;

    struct FixedDocument final {
        DocumentSessionId session{};
        Generation generation{};
        DocumentViewId view{};
    };

    if (state.engine == nullptr || state.renderer == nullptr
        || state.Workspaces().Count() != 1U
        || state.Workspace().editors.GroupCount() != 1U
        || state.Documents().Count() == 0U) {
        std::fprintf(
            stderr,
            "G13 baseline mismatch engine=%d renderer=%d workspaces=%zu "
            "documents=%zu groups=%zu views=%zu\n",
            state.engine != nullptr ? 1 : 0,
            state.renderer != nullptr ? 1 : 0,
            state.Workspaces().Count(),
            state.Documents().Count(),
            state.Workspace().editors.GroupCount(),
            state.Documents().Count() == 0U ? 0U : state.Document().ViewCount());
        return 1030;
    }
    const DocumentSessionId baseline_session = state.Document().id;
    while (state.Documents().Count() > 1U) {
        DocumentSessionId extra{};
        for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
            const auto* document = state.Documents().SessionAt(index);
            if (document != nullptr && document->id != baseline_session) {
                extra = document->id;
                break;
            }
        }
        if (!extra || !state.CloseDocumentSession(extra)) {
            std::fprintf(
                stderr,
                "G13 baseline document cleanup failed documents=%zu extra=%llu\n",
                state.Documents().Count(),
                static_cast<unsigned long long>(extra.Value()));
            return 1031;
        }
    }
    while (state.Document().ViewCount() > 1U) {
        DocumentViewId extra{};
        for (std::size_t index = 0U; index < state.Document().ViewCount(); ++index) {
            const auto* view = state.Document().ViewAt(index);
            if (view != nullptr && view->id != state.ActiveView().id) {
                extra = view->id;
                break;
            }
        }
        if (!extra || !state.CloseDocumentView(extra)) {
            std::fprintf(
                stderr,
                "G13 baseline view cleanup failed views=%zu active=%llu extra=%llu\n",
                state.Document().ViewCount(),
                static_cast<unsigned long long>(state.ActiveView().id.Value()),
                static_cast<unsigned long long>(extra.Value()));
            return 1032;
        }
    }
    if (state.Document().ViewCount() != 1U || !RefreshTreePane(state)) {
        return 1033;
    }
    if (!ValidateFixedResourceScenario(
            state, "one-window-one-document-one-view", 1U, 1U, 1U, 1U)) {
        return 1034;
    }

    WorkspaceWindow* source = state.Workspaces().At(0U);
    if (source == nullptr || source->editors.Active() == nullptr) {
        return 10022;
    }
    const WorkspaceWindowId source_workspace = source->id;
    const auto source_first_group = source->editors.Active()->id;
    const FixedDocument first{
        state.Document().id,
        state.Document().generation,
        state.ActiveView().id};

    if (CreateCell(state, 4096U, 4096U, 96000U) != INKPOD_STATUS_OK) {
        return 1003;
    }
    const FixedDocument large{
        state.Document().id,
        state.Document().generation,
        state.ActiveView().id};
    if (CreateCell(state, 256U, 192U, 96000U) != INKPOD_STATUS_OK) {
        return 1004;
    }
    const FixedDocument third{
        state.Document().id,
        state.Document().generation,
        state.ActiveView().id};
    if (CreateCell(state, 320U, 180U, 96000U) != INKPOD_STATUS_OK) {
        return 1005;
    }
    const FixedDocument fourth{
        state.Document().id,
        state.Document().generation,
        state.ActiveView().id};
    if (state.Documents().Count() != 4U
        || !DispatchEnabledCommand(
            state, source->windows.window, IDM_EDITOR_SPLIT_RIGHT)
        || source->editors.GroupCount() != 2U) {
        return 1006;
    }
    const auto* source_second = source->editors.Other(source_first_group);
    auto* fourth_document = state.Documents().Find(fourth.session);
    if (source_second == nullptr || fourth_document == nullptr
        || fourth_document->ViewCount() != 2U) {
        return 1007;
    }
    DocumentViewId duplicate_fourth{};
    for (std::size_t index = 0U; index < fourth_document->ViewCount(); ++index) {
        const auto* view = fourth_document->ViewAt(index);
        if (view != nullptr && view->id != fourth.view) {
            duplicate_fourth = view->id;
            break;
        }
    }
    if (!duplicate_fourth
        || !state.MoveDocumentView(
            third.view,
            source_workspace,
            source_second->id,
            0U)
        || !state.CloseDocumentView(duplicate_fourth)
        || !RefreshTreePane(state)
        || !ValidateFixedResourceScenario(
            state, "one-window-four-documents-two-groups", 1U, 4U, 4U, 2U)) {
        return 1008;
    }

    if (SendMessageW(
            source->windows.window,
            WM_COMMAND,
            IDM_WORKSPACE_NEW_WINDOW,
            0) != 1
        || state.Workspaces().Count() != 2U) {
        return 1009;
    }
    WorkspaceWindow* destination = state.Workspaces().At(1U);
    if (destination == nullptr || destination->editors.Active() == nullptr) {
        return 1010;
    }
    const WorkspaceWindowId destination_workspace = destination->id;
    const auto destination_first_group = destination->editors.Active()->id;
    if (!state.MoveDocumentView(
            fourth.view,
            destination_workspace,
            destination_first_group,
            0U)
        || !state.ActivateDocumentView(fourth.view)
        || SendMessageW(
               destination->windows.window,
               WM_COMMAND,
               IDM_EDITOR_SPLIT_RIGHT,
               0) != 1
        || destination->editors.GroupCount() != 2U) {
        std::fprintf(
            stderr,
            "G13 destination split failed workspace=%llu active_workspace=%llu "
            "groups=%zu fourth_workspace=%llu\n",
            static_cast<unsigned long long>(destination_workspace.Value()),
            static_cast<unsigned long long>(state.Workspace().id.Value()),
            destination->editors.GroupCount(),
            static_cast<unsigned long long>(
                state.routing.targets.WorkspaceForView(fourth.view).Value()));
        return 1011;
    }
    const auto* destination_second = destination->editors.Other(
        destination_first_group);
    fourth_document = state.Documents().Find(fourth.session);
    if (destination_second == nullptr || fourth_document == nullptr
        || fourth_document->ViewCount() != 2U) {
        return 1012;
    }
    duplicate_fourth = {};
    for (std::size_t index = 0U; index < fourth_document->ViewCount(); ++index) {
        const auto* view = fourth_document->ViewAt(index);
        if (view != nullptr && view->id != fourth.view) {
            duplicate_fourth = view->id;
            break;
        }
    }
    if (!duplicate_fourth
        || !state.MoveDocumentView(
            large.view,
            destination_workspace,
            destination_second->id,
            0U)
        || !state.CloseDocumentView(duplicate_fourth)) {
        return 1013;
    }

    const auto cache_before = state.Thumbnails().Usage();
    if (!state.Thumbnails().SetBudgetBytes(20000U)
        || !state.ActivateDocumentView(first.view) || !RefreshTreePane(state)
        || !state.ActivateDocumentView(large.view) || !RefreshTreePane(state)) {
        return 1014;
    }
    const auto constrained_cache = state.Thumbnails().Usage();
    inkpod::app::PaneResourceUsage destination_layer_cache{};
    if (constrained_cache.resident_bytes > constrained_cache.budget_bytes
        || constrained_cache.eviction_count <= cache_before.eviction_count
        || !state.GetPaneResourceUsage(
            destination->pane_ids.layer, destination_layer_cache)
        || destination_layer_cache.thumbnail_bytes == 0U
        || !state.Thumbnails().SetBudgetBytes(
            inkpod::windows::ui::ThumbnailCache::kDefaultBudgetBytes)
        || !state.ActivateDocumentView(first.view) || !RefreshTreePane(state)
        || !state.ActivateDocumentView(large.view) || !RefreshTreePane(state)
        || !ValidateFixedResourceScenario(
            state, "two-windows-four-documents-four-views", 2U, 4U, 4U, 4U)) {
        return 1015;
    }

    InkpodDocumentInfo large_before = EmptyDocumentInfo();
    if (!QueryDocument(state, large_before)) {
        return 1016;
    }
    const InkpodStatus large_edit_status = state.engine->Invoke(
        large.session,
        large.generation,
        [](InkpodCore* core) {
            const InkpodStrokeSample sample{
                sizeof(InkpodStrokeSample), 0U, 16.0F, 16.0F, 1.0F, 0U};
            const InkpodStrokeInput stroke{
                sizeof(InkpodStrokeInput),
                INKPOD_TOOL_PENCIL,
                INKPOD_PLANE_COLOR,
                INKPOD_COORDINATE_SPACE_DOCUMENT,
                0U,
                UINT32_C(0x2a6ec8ff),
                1.0F,
                &sample,
                1U,
                sizeof(sample),
                INKPOD_BRUSH_ROUND,
                0U,
                0U,
                INKPOD_START_COLOR_ANY,
                0U};
            InkpodDispatchResult dispatch{};
            dispatch.struct_size = sizeof(dispatch);
            InkpodStatus status = inkpod_core_set_active_plane(
                core, INKPOD_PLANE_COLOR);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_apply_stroke(core, &stroke, &dispatch);
            }

            std::array<std::uint8_t, 4U * 4U * 4U> pixels{};
            for (std::size_t offset = 0U; offset < pixels.size(); offset += 4U) {
                pixels[offset] = 48U;
                pixels[offset + 1U] = 96U;
                pixels[offset + 2U] = 192U;
                pixels[offset + 3U] = 255U;
            }
            constexpr std::array<std::uint8_t, 13U> name{
                'g', '1', '3', '-', 'r', 'e', 'f', 'e', 'r', 'e', 'n', 'c', 'e'};
            const InkpodRasterSourceInput source_input{
                sizeof(InkpodRasterSourceInput),
                INKPOD_STORAGE_RGBA8,
                0U,
                0x1300130013001300ULL,
                0x3100310031003100ULL,
                1U,
                4U,
                4U,
                96000U,
                96000U,
                InkpodFrameRect{0, 0, 4, 4},
                pixels.data(),
                pixels.size(),
                16U};
            const InkpodLightTableItemInput item{
                sizeof(InkpodLightTableItemInput),
                INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
                500U,
                INKPOD_LIGHT_TABLE_COLOR,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    48U,
                    96U,
                    192U,
                    255U},
                0,
                0,
                1000U,
                1000U,
                0,
                0U,
                name.data(),
                name.size(),
                source_input};
            std::uint64_t item_id{};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_light_table_add_item(
                    core, &item, &dispatch, &item_id);
            }
            return status == INKPOD_STATUS_OK && item_id != 0U
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        true,
        true);
    InkpodDocumentInfo large_after = EmptyDocumentInfo();
    InkpodResourceUsage large_resources{};
    if (large_edit_status != INKPOD_STATUS_OK
        || state.engine->WaitIdle(large.session, large.generation)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, large_after)
        || !QueryCoreResourceUsage(
            state, large.session, large.generation, large_resources)
        || large_after.document_revision <= large_before.document_revision
        || large_resources.document_tile_bytes == 0U
        || large_resources.history_bytes == 0U
        || large_resources.history_entry_count < 2U
        || large_resources.reference_light_table_bytes == 0U
        || large_resources.reference_light_table_tile_count == 0U) {
        return 1017;
    }

    InkpodDispatchResult history_result{};
    history_result.struct_size = sizeof(history_result);
    if (state.engine->Invoke(
            large.session,
            large.generation,
            [&history_result](InkpodCore* core) {
                return inkpod_core_undo(core, &history_result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 1018;
    }
    InkpodResourceUsage after_undo{};
    if (!QueryCoreResourceUsage(
            state, large.session, large.generation, after_undo)
        || after_undo.reference_light_table_bytes != 0U
        || state.engine->Invoke(
               large.session,
               large.generation,
               [&history_result](InkpodCore* core) {
                   return inkpod_core_redo(core, &history_result);
               },
               true,
               true) != INKPOD_STATUS_OK) {
        return 1019;
    }
    InkpodResourceUsage after_redo{};
    InkpodDocumentInfo before_device_loss = EmptyDocumentInfo();
    if (!QueryCoreResourceUsage(
            state, large.session, large.generation, after_redo)
        || after_redo.reference_light_table_bytes
            != large_resources.reference_light_table_bytes
        || !QueryDocument(state, before_device_loss)) {
        return 1020;
    }

    auto* large_group = destination->editors.FindByView(large.view);
    const auto renderer_before = state.renderer->ResourceUsage();
    if (large_group == nullptr || large_group->canvas == nullptr
        || SendMessageW(
            large_group->canvas,
            inkpod::renderer::kCanvasSimulateDeviceLoss,
            0,
            0) != 1
        || SendMessageW(
            large_group->canvas,
            inkpod::renderer::kCanvasRenderOnce,
            0,
            0) != 1
        || !state.renderer->WaitQueueIdleForSmokeTest()) {
        return 1021;
    }
    InkpodDocumentInfo after_device_loss = EmptyDocumentInfo();
    const auto renderer_after = state.renderer->ResourceUsage();
    if (!QueryDocument(state, after_device_loss)
        || after_device_loss.document_revision
            != before_device_loss.document_revision
        || after_device_loss.main_plane_checksum
            != before_device_loss.main_plane_checksum
        || after_device_loss.color_plane_checksum
            != before_device_loss.color_plane_checksum
        || renderer_after.device_reset_count
            <= renderer_before.device_reset_count
        || !ValidateFixedResourceScenario(
            state,
            "large-light-table-reference-history-device-lost",
            2U,
            4U,
            4U,
            4U)) {
        return 1022;
    }

    // Restore the one-window/one-group baseline for the following G10 smoke.
    auto* source_first = source->editors.Find(source_first_group);
    if (source_first == nullptr
        || !state.MoveDocumentView(
            large.view,
            source_workspace,
            source_first_group,
            source_first->ViewCount())
        || !state.MoveDocumentView(
            fourth.view,
            source_workspace,
            source_first_group,
            source_first->ViewCount())) {
        return 1023;
    }
    // Closing the split workspace must synchronously detach both Canvas sinks;
    // the following activation publishes a snapshot immediately.
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    if (state.Workspaces().Count() != 1U
        || !ValidateFixedResourceScenario(
            state, "workspace-close", 1U, 4U, 4U, 2U)
        || !state.ActivateDocumentView(third.view)
        || !DispatchEnabledCommand(
            state, source->windows.window, IDM_EDITOR_GROUP_CLOSE)
        || source->editors.GroupCount() != 1U
        || !ValidateFixedResourceScenario(
            state, "editor-group-close", 1U, 4U, 4U, 1U)
        || !state.ActivateDocumentView(first.view)
        || !state.CloseDocumentSession(large.session)
        || !state.CloseDocumentSession(third.session)
        || !state.CloseDocumentSession(fourth.session)
        || state.Documents().Count() != 1U
        || state.Document().id != first.session
        || state.Document().ViewCount() != 1U
        || state.Workspace().editors.GroupCount() != 1U) {
        return 1024;
    }
    return 0;
}

int RunMultiWorkspaceWindowSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::CommandResolveStatus;
    using inkpod::app::DocumentSessionId;
    using inkpod::app::DocumentViewId;
    using inkpod::app::WorkspaceWindow;
    using inkpod::app::WorkspaceWindowId;

    if (state.Workspaces().Count() != 1U || state.engine == nullptr
        || state.renderer == nullptr || state.Document().ActiveView() == nullptr) {
        return 790;
    }
    WorkspaceWindow* source = state.Workspaces().At(0U);
    if (source == nullptr || source->windows.window == nullptr
        || source->windows.canvas == nullptr) {
        return 791;
    }
    const WorkspaceWindowId source_workspace = source->id;
    const DocumentSessionId shared_session = state.Document().id;
    const DocumentViewId source_view = state.Document().ActiveView()->id;
    const std::size_t original_view_count = state.Document().ViewCount();
    InkpodDocumentInfo before_edit{};
    InkpodSnapshotTransform source_transform_before{};
    if (!QueryDocument(state, before_edit)
        || !QuerySnapshotTransform(state, source_transform_before)
        || !DispatchEnabledCommand(
               state,
               source->windows.window,
               IDM_VIEW_DUPLICATE_NEW_WINDOW)
        || state.Workspaces().Count() != 2U) {
        return 792;
    }

    WorkspaceWindow* destination = state.Workspaces().At(1U);
    if (destination == nullptr || destination->id == source_workspace
        || destination->windows.window == nullptr
        || destination->windows.window == source->windows.window
        || destination->windows.canvas == nullptr
        || state.Workspace().id != destination->id
        || state.Document().id != shared_session
        || state.Document().ViewCount() != original_view_count + 1U
        || state.Document().ActiveView() == nullptr
        || state.Document().ActiveView()->id == source_view
        || state.WorkspaceForView(source_view) != source
        || state.WorkspaceForView(state.Document().ActiveView()->id)
            != destination
        || GetMenu(source->windows.window) == nullptr
        || GetMenu(destination->windows.window) == nullptr) {
        return 793;
    }
    const WorkspaceWindowId destination_workspace = destination->id;
    const DocumentViewId destination_view = state.Document().ActiveView()->id;
    const auto destination_context = state.routing.targets.Capture();
    SendMessageW(source->windows.window, WM_ACTIVATE, WA_ACTIVE, 0);
    if (state.Workspace().id != source_workspace
        || state.Document().id != shared_session
        || state.Document().ActiveView() == nullptr
        || state.Document().ActiveView()->id != source_view) {
        return 794;
    }
    SendMessageW(destination->windows.window, WM_ACTIVATE, WA_ACTIVE, 0);
    if (state.Workspace().id != destination_workspace
        || state.Document().ActiveView() == nullptr
        || state.Document().ActiveView()->id != destination_view) {
        return 795;
    }

    RECT dpi_rect{};
    GetWindowRect(destination->windows.window, &dpi_rect);
    if (SendMessageW(
            destination->windows.window,
            WM_DPICHANGED,
            MAKELONG(144, 144),
            reinterpret_cast<LPARAM>(&dpi_rect)) != 0
        || IsWindow(destination->windows.window) == FALSE) {
        return 796;
    }

    InkpodSnapshotTransform destination_transform{};
    InkpodSnapshotTransform destination_transform_before{};
    InkpodSnapshotTransform source_transform{};
    constexpr std::uint32_t horizontal_flip =
        INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL;
    if (!QuerySnapshotTransform(state, destination_transform_before)
        || !DispatchEnabledCommand(
            state,
            destination->windows.window,
            IDM_VIEW_FLIP_HORIZONTAL)
        || !QuerySnapshotTransform(state, destination_transform)
        || ((destination_transform.flags ^ destination_transform_before.flags)
            & horizontal_flip) == 0U
        || !state.ActivateDocumentView(source_view)
        || !QuerySnapshotTransform(state, source_transform)
        || (source_transform.flags & horizontal_flip)
            != (source_transform_before.flags & horizontal_flip)) {
        return 797;
    }

    InkpodSnapshotTransform edit_transform{};
    if (!state.ActivateDocumentView(destination_view)
        || !DispatchEnabledCommand(
            state, destination->windows.window, IDM_VIEW_FIT)
        || !QuerySnapshotTransform(state, edit_transform)
        || !std::isfinite(edit_transform.zoom)
        || edit_transform.zoom <= 0.0
        || !DispatchEnabledCommand(
               state, destination->windows.window, IDM_PLANE_MAIN_LINE)
        || !DispatchEnabledCommand(
               state, destination->windows.window, IDM_TOOL_PENCIL)
        || SendMessageW(
               destination->windows.canvas,
               WM_LBUTTONDOWN,
               MK_LBUTTON,
               MAKELPARAM(
                   static_cast<int>(std::lround(
                       edit_transform.pan_x
                       + static_cast<double>(edit_transform.document_width)
                           * edit_transform.zoom * 0.4)),
                   static_cast<int>(std::lround(
                       edit_transform.pan_y
                       + static_cast<double>(edit_transform.document_height)
                           * edit_transform.zoom * 0.5)))) != 1) {
        return 798;
    }
    SendMessageW(
        destination->windows.canvas,
        WM_MOUSEMOVE,
        MK_LBUTTON,
        MAKELPARAM(
            static_cast<int>(std::lround(
                edit_transform.pan_x
                + static_cast<double>(edit_transform.document_width)
                    * edit_transform.zoom * 0.5)),
            static_cast<int>(std::lround(
                edit_transform.pan_y
                + static_cast<double>(edit_transform.document_height)
                    * edit_transform.zoom * 0.5))));
    if (SendMessageW(
            destination->windows.canvas,
            WM_LBUTTONUP,
            0,
            MAKELPARAM(
                static_cast<int>(std::lround(
                    edit_transform.pan_x
                    + static_cast<double>(edit_transform.document_width)
                        * edit_transform.zoom * 0.6)),
                static_cast<int>(std::lround(
                    edit_transform.pan_y
                    + static_cast<double>(edit_transform.document_height)
                        * edit_transform.zoom * 0.5)))) != 1
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 799;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo destination_edit{};
    InkpodDocumentInfo source_observed{};
    const bool queried_destination = QueryDocument(state, destination_edit);
    const bool activated_source = state.ActivateDocumentView(source_view);
    if (activated_source) {
        SendMessageW(source->windows.window, WM_ACTIVATE, WA_ACTIVE, 0);
    }
    const bool queried_source = activated_source
        && QueryDocument(state, source_observed);
    const bool dispatched_undo = queried_source
        && DispatchEnabledCommand(state, source->windows.window, IDM_EDIT_UNDO);
    const InkpodStatus undo_idle = state.engine->WaitIdle();
    if (!queried_destination
        || destination_edit.document_revision <= before_edit.document_revision
        || destination_edit.main_plane_checksum == before_edit.main_plane_checksum
        || (destination_edit.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || !activated_source || !queried_source
        || source_observed.document_revision != destination_edit.document_revision
        || source_observed.main_plane_checksum
            != destination_edit.main_plane_checksum
        || !dispatched_undo || undo_idle != INKPOD_STATUS_OK) {
        return 800;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo source_undo{};
    InkpodDocumentInfo destination_observed_undo{};
    if (!QueryDocument(state, source_undo)
        || source_undo.main_plane_checksum != before_edit.main_plane_checksum
        || !state.ActivateDocumentView(destination_view)
        || !QueryDocument(state, destination_observed_undo)
        || destination_observed_undo.main_plane_checksum
            != source_undo.main_plane_checksum) {
        return 801;
    }
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    if (state.Workspaces().Count() != 1U
        || state.FindWorkspace(destination_workspace) != nullptr
        || state.Document().ViewCount() != original_view_count
        || state.routing.targets.Resolve(
               destination_context,
               inkpod::app::kDocumentViewCommandScope)
            != CommandResolveStatus::StaleTarget
        || !state.ActivateDocumentView(source_view)) {
        return 802;
    }
    if (SendMessageW(
            source->windows.window,
            WM_COMMAND,
            IDM_WORKSPACE_NEW_WINDOW,
            0) != 1
        || state.Workspaces().Count() != 2U) {
        return 803;
    }
    destination = state.Workspaces().At(1U);
    if (destination == nullptr || destination->windows.window == nullptr
        || state.Workspace().id != destination->id
        || state.routing.targets.DocumentSession()) {
        return 804;
    }
    const std::size_t document_count = state.Documents().Count();
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || state.Documents().Count() != document_count + 1U
        || state.Document().id == shared_session
        || state.Document().ActiveView() == nullptr
        || state.WorkspaceForView(state.Document().ActiveView()->id)
            != destination) {
        return 805;
    }
    const DocumentSessionId isolated_session = state.Document().id;
    const DocumentViewId isolated_view = state.Document().ActiveView()->id;
    InkpodDocumentInfo isolated_before{};
    if (!QueryDocument(state, isolated_before)
        || !DispatchEnabledCommand(
               state, destination->windows.window, IDM_PLANE_MAIN_LINE)
        || !DispatchEnabledCommand(
               state, destination->windows.window, IDM_TOOL_PENCIL)
        || SendMessageW(
               destination->windows.canvas,
               WM_LBUTTONDOWN,
               MK_LBUTTON,
               MAKELPARAM(120, 120)) != 1) {
        return 806;
    }
    SendMessageW(
        destination->windows.canvas,
        WM_MOUSEMOVE,
        MK_LBUTTON,
        MAKELPARAM(180, 136));
    SendMessageW(
        destination->windows.canvas,
        WM_LBUTTONUP,
        0,
        MAKELPARAM(220, 144));
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 807;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo isolated_after{};
    InkpodDocumentInfo source_after_isolated_edit{};
    if (!state.ActivateDocumentView(isolated_view)
        || !QueryDocument(state, isolated_after)
        || isolated_after.main_plane_checksum
            == isolated_before.main_plane_checksum
        || !state.ActivateDocumentView(source_view)
        || !QueryDocument(state, source_after_isolated_edit)
        || source_after_isolated_edit.main_plane_checksum
            != source_undo.main_plane_checksum) {
        return 808;
    }

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    std::array<wchar_t, MAX_PATH> temporary_file{};
    const DWORD temporary_length = GetTempPathW(
        static_cast<DWORD>(temporary_directory.size()),
        temporary_directory.data());
    if (temporary_length == 0U
        || temporary_length >= temporary_directory.size()) {
        return 812;
    }
    _snwprintf_s(
        temporary_file.data(),
        temporary_file.size(),
        _TRUNCATE,
        L"%lsinkpod-multi-window-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring isolated_path(temporary_file.data());
    const auto cleanup_isolated_file = [&isolated_path]() noexcept {
        DeleteFileW(isolated_path.c_str());
        DeleteFileW((isolated_path + L".recovery.inkpod").c_str());
    };
    InkpodDocumentInfo isolated_saved{};
    if (!state.ActivateDocumentView(isolated_view)
        || SaveToPath(state, isolated_path) != INKPOD_STATUS_OK
        || !QueryDocument(state, isolated_saved)
        || (isolated_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || isolated_saved.main_plane_checksum
            != isolated_after.main_plane_checksum) {
        cleanup_isolated_file();
        return 813;
    }
    const bool source_locator_visible =
        source->windows.workspace.dock.IsPaneVisible(DockPaneType::Locator);
    const bool destination_locator_visible =
        destination->windows.workspace.dock.IsPaneVisible(DockPaneType::Locator);
    if (!DispatchEnabledCommand(
            state, destination->windows.window, IDM_WINDOW_LOCATOR)
        || source->windows.workspace.dock.IsPaneVisible(DockPaneType::Locator)
            != source_locator_visible
        || destination->windows.workspace.dock.IsPaneVisible(
               DockPaneType::Locator)
            == destination_locator_visible
        || !DispatchEnabledCommand(
            state, destination->windows.window, IDM_WINDOW_LOCATOR)) {
        cleanup_isolated_file();
        return 814;
    }
    const std::uint32_t clean_prompt_count =
        state.lifetime.smoke_dirty_prompt_count;
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    if (state.Workspaces().Count() != 1U
        || state.Documents().Find(isolated_session) != nullptr
        || state.lifetime.smoke_dirty_prompt_count != clean_prompt_count
        || !state.ActivateDocumentView(source_view)
        || SendMessageW(
               source->windows.window,
               WM_COMMAND,
               IDM_WORKSPACE_NEW_WINDOW,
               0) != 1
        || state.Workspaces().Count() != 2U) {
        cleanup_isolated_file();
        return 815;
    }
    destination = state.Workspaces().At(1U);
    if (destination == nullptr || destination->windows.window == nullptr
        || OpenDocumentFromPath(state, isolated_path) != INKPOD_STATUS_OK
        || state.Document().id == isolated_session
        || state.Document().id == shared_session
        || state.Document().ActiveView() == nullptr) {
        cleanup_isolated_file();
        return 816;
    }
    const WorkspaceWindowId isolated_workspace = destination->id;
    const DocumentSessionId reopened_session = state.Document().id;
    const DocumentViewId reopened_view = state.Document().ActiveView()->id;
    InkpodDocumentInfo reopened{};
    InkpodDocumentInfo source_after_reopen{};
    if (!QueryDocument(state, reopened)
        || reopened.main_plane_checksum != isolated_saved.main_plane_checksum
        || (reopened.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || !state.ActivateDocumentView(source_view)
        || !QueryDocument(state, source_after_reopen)
        || source_after_reopen.main_plane_checksum
            != source_undo.main_plane_checksum
        || !state.ActivateDocumentView(reopened_view)
        || !DispatchEnabledCommand(
            state, destination->windows.window, IDM_SELECTION_ALL)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        cleanup_isolated_file();
        return 817;
    }
    cleanup_isolated_file();
    PumpPendingWindowMessages();
    InkpodDocumentInfo reopened_dirty{};
    if (!state.ActivateDocumentView(reopened_view)
        || !QueryDocument(state, reopened_dirty)
        || (reopened_dirty.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        return 818;
    }

    const std::uint32_t prompt_count = state.lifetime.smoke_dirty_prompt_count;
    state.lifetime.smoke_dirty_prompt_choice = IDCANCEL;
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    if (state.Workspaces().Count() != 2U
        || state.Documents().Find(reopened_session) == nullptr
        || state.lifetime.smoke_dirty_prompt_count != prompt_count + 1U) {
        return 809;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    SendMessageW(destination->windows.window, WM_CLOSE, 0, 0);
    if (state.Workspaces().Count() != 1U
        || state.Documents().Find(reopened_session) != nullptr
        || state.FindWorkspace(isolated_workspace) != nullptr
        || !state.ActivateDocumentView(source_view)) {
        return 810;
    }
    const LRESULT final_close = SendMessageW(
        source->windows.window, WM_CLOSE, 0, 0);
    MSG quit{};
    BOOL found_quit{};
    while (PeekMessageW(&quit, nullptr, 0, 0, PM_REMOVE) != FALSE) {
        if (quit.message == WM_QUIT) {
            found_quit = TRUE;
            break;
        }
        TranslateMessage(&quit);
        DispatchMessageW(&quit);
    }
    if (found_quit == FALSE
        || quit.message != WM_QUIT || state.Workspaces().Count() != 1U) {
        std::fprintf(
            stderr,
            "g10 close result=%lld quit=%d message=%u workspaces=%zu window=%d\n",
            static_cast<long long>(final_close),
            found_quit,
            quit.message,
            state.Workspaces().Count(),
            IsWindow(source->windows.window) != FALSE ? 1 : 0);
        return 811;
    }
    return 0;
}

int RunEditorStateOwnershipSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr
        || !state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)) {
        return 1001;
    }
    const auto first_session = state.Document().id;
    const auto first_generation = state.Document().generation;
    const auto first_view = state.ActiveView().id;
    InkpodDocumentInfo document_before = EmptyDocumentInfo();
    InkpodHistoryInfo history_before{};
    history_before.struct_size = sizeof(history_before);
    InkpodEditorDefaults defaults{};
    InkpodEditorStateInfo editor_before{};
    editor_before.struct_size = sizeof(editor_before);
    if (!QueryDocument(state, document_before)
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&history_before](InkpodCore* core) {
                   return inkpod_core_history_info(core, &history_before);
               },
               false,
               false) != INKPOD_STATUS_OK
        || state.engine->GetEditorDefaults(
               first_session, first_generation, defaults) != INKPOD_STATUS_OK
        || !state.engine->GetEditorState(
            first_session, first_generation, editor_before)) {
        return 10023;
    }

    InkpodEditorStateUpdate tool_update{};
    tool_update.struct_size = sizeof(tool_update);
    tool_update.kind = INKPOD_EDITOR_UPDATE_ACTIVE_TOOL;
    tool_update.expected_editor_revision = editor_before.editor_revision;
    tool_update.tool = editor_before.active_tool == INKPOD_TOOL_BRUSH
        ? INKPOD_TOOL_PENCIL
        : INKPOD_TOOL_BRUSH;
    if (state.UpdateEditorState(tool_update) != INKPOD_STATUS_OK) {
        return 1003;
    }
    InkpodEditorStateInfo editor_after_tool{};
    editor_after_tool.struct_size = sizeof(editor_after_tool);
    if (!state.engine->GetEditorState(
            first_session, first_generation, editor_after_tool)
        || editor_after_tool.active_tool != tool_update.tool
        || editor_after_tool.editor_revision
            != editor_before.editor_revision + 1U) {
        return 1004;
    }

    InkpodEditorStateUpdate color_update{};
    color_update.struct_size = sizeof(color_update);
    color_update.kind = INKPOD_EDITOR_UPDATE_TOOL_COLOR;
    color_update.expected_editor_revision = editor_after_tool.editor_revision;
    color_update.tool = editor_after_tool.active_tool;
    color_update.color = InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_16,
        UINT16_C(0x1234),
        UINT16_C(0x5678),
        UINT16_C(0x9abc),
        UINT16_C(0xdef0)};
    if (state.UpdateEditorState(color_update) != INKPOD_STATUS_OK) {
        return 1005;
    }
    InkpodEditorStateInfo first_editor{};
    first_editor.struct_size = sizeof(first_editor);
    InkpodDocumentInfo document_after = EmptyDocumentInfo();
    InkpodHistoryInfo history_after{};
    history_after.struct_size = sizeof(history_after);
    if (!state.engine->GetEditorState(
            first_session, first_generation, first_editor)
        || first_editor.current_color.depth != INKPOD_COLOR_DEPTH_16
        || first_editor.current_color.red != color_update.color.red
        || first_editor.current_color.green != color_update.color.green
        || first_editor.current_color.blue != color_update.color.blue
        || first_editor.current_color.alpha != color_update.color.alpha
        || !QueryDocument(state, document_after)
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&history_after](InkpodCore* core) {
                   return inkpod_core_history_info(core, &history_after);
               },
               false,
               false) != INKPOD_STATUS_OK
        || document_after.document_revision != document_before.document_revision
        || document_after.main_plane_checksum != document_before.main_plane_checksum
        || document_after.color_plane_checksum != document_before.color_plane_checksum
        || history_after.cursor != history_before.cursor
        || history_after.item_count != history_before.item_count) {
        return 1006;
    }

    if (CreateCell(state, 16U, 12U, 96000U) != INKPOD_STATUS_OK) {
        return 1007;
    }
    const auto second_session = state.Document().id;
    const auto second_generation = state.Document().generation;
    const auto second_view = state.ActiveView().id;
    InkpodEditorStateInfo second_editor{};
    second_editor.struct_size = sizeof(second_editor);
    if (second_session == first_session
        || !state.engine->GetEditorState(
            second_session, second_generation, second_editor)
        || second_editor.active_tool != defaults.state.active_tool
        || second_editor.current_color.depth != defaults.state.current_color.depth
        || second_editor.current_color.red != defaults.state.current_color.red
        || second_editor.current_color.green != defaults.state.current_color.green
        || second_editor.current_color.blue != defaults.state.current_color.blue
        || second_editor.current_color.alpha != defaults.state.current_color.alpha) {
        return 1008;
    }

    state.Workspace().tools.active_tool = INKPOD_TOOL_ERASER;
    state.Workspace().tools.drawing_color = InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 255U, 0U, 0U, 255U};
    if (!ActivateDocumentTab(state, first_view)
        || state.Workspace().tools.editor.session != first_session
        || state.Workspace().tools.editor.generation != first_generation
        || state.Workspace().tools.active_tool != first_editor.active_tool
        || state.Workspace().tools.drawing_color.depth != INKPOD_COLOR_DEPTH_16
        || state.Workspace().tools.drawing_color.red != first_editor.current_color.red
        || !ActivateDocumentTab(state, second_view)
        || state.Workspace().tools.editor.session != second_session
        || state.Workspace().tools.editor.generation != second_generation
        || state.Workspace().tools.active_tool != second_editor.active_tool) {
        return 1009;
    }

    InkpodEditorStateUpdate external_vector_update{};
    external_vector_update.struct_size = sizeof(external_vector_update);
    external_vector_update.kind = INKPOD_EDITOR_UPDATE_VECTOR_OPTIONS;
    external_vector_update.expected_editor_revision =
        second_editor.editor_revision;
    external_vector_update.vector = second_editor.vector;
    external_vector_update.vector.erase_mode =
        second_editor.vector.erase_mode == INKPOD_VECTOR_ERASE_PARTIAL
        ? INKPOD_VECTOR_ERASE_WHOLE_PATH
        : INKPOD_VECTOR_ERASE_PARTIAL;
    InkpodEditorStateInfo externally_updated_editor{};
    externally_updated_editor.struct_size = sizeof(externally_updated_editor);
    if (state.engine->Invoke(
            second_session,
            second_generation,
            [&external_vector_update, &externally_updated_editor](
                InkpodCore* core) {
                return inkpod_core_update_editor_state(
                    core,
                    &external_vector_update,
                    &externally_updated_editor);
            },
            false,
            false) != INKPOD_STATUS_OK
        || externally_updated_editor.editor_revision
            != second_editor.editor_revision + 1U) {
        return 1010;
    }
    state.Workspace().tools.vector_selection_mode = INKPOD_VECTOR_SELECT_FILL;
    const std::optional<LRESULT> failed_vector_command = IssueCommand(
        &state,
        state.Workspace().windows.window,
        IDM_VECTOR_SELECT_FILL,
        0,
        std::nullopt);
    if (!failed_vector_command.has_value()
        || failed_vector_command.value() != 0) {
        return 1011;
    }
    InkpodEditorStateInfo editor_after_failed_vector_update{};
    editor_after_failed_vector_update.struct_size =
        sizeof(editor_after_failed_vector_update);
    if (!state.engine->GetEditorState(
            second_session,
            second_generation,
            editor_after_failed_vector_update)
        || editor_after_failed_vector_update.editor_revision
            != externally_updated_editor.editor_revision
        || std::memcmp(
               editor_after_failed_vector_update.editor_digest,
               externally_updated_editor.editor_digest,
               sizeof(externally_updated_editor.editor_digest))
            != 0
        || state.Workspace().tools.editor.session != second_session
        || state.Workspace().tools.editor.generation != second_generation
        || state.Workspace().tools.editor.editor_revision
            != externally_updated_editor.editor_revision
        || state.Workspace().tools.vector_erase_mode
            != externally_updated_editor.vector.erase_mode
        || state.Workspace().tools.vector_selection_mode
            != externally_updated_editor.vector.selection_mode) {
        return 1012;
    }
    return 0;
}

int RunCellCreationSmoke(ApplicationHost& state) noexcept {
    const std::size_t baseline_count = state.Documents().Count();
    const std::size_t engine_baseline = state.engine == nullptr
        ? 0U
        : state.engine->SessionCount();
    const std::size_t recent_baseline = state.RecentDocumentCount();
    const DocumentViewId previous_view = state.routing.targets.ActiveDocumentView();
    std::array<DocumentSessionId, inkpod::app::DocumentRegistry::kMaximumSessions> baseline{};
    for (std::size_t index = 0U; index < baseline_count; ++index) {
        const auto* document = state.Documents().SessionAt(index);
        if (document == nullptr) {
            return 920;
        }
        baseline[index] = document->id;
    }

    (void)SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_FILE_NEW,
        0);
    if (state.Documents().Count() != baseline_count + 3U
        || state.engine == nullptr
        || state.engine->SessionCount() != engine_baseline + 3U) {
        return 921;
    }
    std::array<DocumentSessionId, 3U> created{};
    std::size_t created_count{};
    for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
        const auto* document = state.Documents().SessionAt(index);
        if (document == nullptr) {
            return 924;
        }
        const bool existed = std::find(
            baseline.cbegin(), baseline.cbegin() + baseline_count, document->id)
            != baseline.cbegin() + baseline_count;
        if (!existed && created_count < created.size()) {
            created[created_count++] = document->id;
        }
    }
    if (created_count != created.size()) {
        return 925;
    }
    InkpodDocumentInfo expected{};
    for (std::size_t index = 0U; index < created_count; ++index) {
        auto* document = state.Documents().Find(created[index]);
        const auto* view = document == nullptr ? nullptr : document->ViewAt(0U);
        InkpodDocumentInfo info{};
        if (view == nullptr || !state.ActivateDocumentView(view->id)
            || !QueryDocument(state, info)
            || info.width == 0U || info.height == 0U
            || std::memcmp(
                   &info.shooting_frame,
                   &info.hundred_frame,
                   sizeof(info.hundred_frame)) != 0
            || info.maximum_close_frame.width >= info.hundred_frame.width
            || info.maximum_close_frame.height >= info.hundred_frame.height
            || info.margin_left == 0U || info.margin_top == 0U) {
            return 922;
        }
        if (index == 0U) {
            expected = info;
        } else if (info.width != expected.width
            || info.height != expected.height
            || info.dpi_x_milli != expected.dpi_x_milli
            || info.dpi_y_milli != expected.dpi_y_milli
            || std::memcmp(
                   &info.hundred_frame,
                   &expected.hundred_frame,
                   sizeof(info.hundred_frame) * 6U) != 0
            || info.margin_left != expected.margin_left
            || info.margin_top != expected.margin_top
            || info.margin_right != expected.margin_right
            || info.margin_bottom != expected.margin_bottom) {
            return 923;
        }
        std::array<std::uint8_t, 32U> name{};
        InkpodNodeInfo color{};
        color.struct_size = sizeof(color);
        color.name_utf8 = name.data();
        color.name_capacity = name.size();
        if (state.engine->Invoke(
                [&color](InkpodCore* core) {
                    return inkpod_core_node_get(core, 0U, 1U, &color);
                },
                false,
                true) != INKPOD_STATUS_OK
            || color.pixel_format != INKPOD_STORAGE_RGBA8) {
            return 924;
        }
    }

    const std::size_t before_failure_documents = state.Documents().Count();
    const std::size_t before_failure_engines = state.engine->SessionCount();
    const std::size_t before_failure_recent = state.RecentDocumentCount();
    const DocumentViewId before_failure_view =
        state.routing.targets.ActiveDocumentView();
    const InkpodCellCreationOptions failure_options{
        sizeof(InkpodCellCreationOptions),
        INKPOD_CELL_SIZING_IMAGE_PIXELS,
        INKPOD_FEATURE_NONE,
        64U,
        48U,
        96'000U,
        96'000U,
        50U,
        900U,
        500U,
        INKPOD_FRAME_ANCHOR_CENTER,
        INKPOD_LAYER_GRAYSCALE_COLORING,
        INKPOD_STORAGE_RGBA8,
        3U,
        0U};
    if (CreateCellsFromOptions(state, failure_options, 1U)
            != INKPOD_STATUS_INVALID_STATE
        || state.Documents().Count() != before_failure_documents
        || state.engine->SessionCount() != before_failure_engines
        || state.RecentDocumentCount() != before_failure_recent
        || state.routing.targets.ActiveDocumentView() != before_failure_view) {
        return 929;
    }
    for (std::size_t index = created_count; index != 0U; --index) {
        if (!state.CloseDocumentSession(created[index - 1U])) {
            return 926;
        }
    }
    if (previous_view && !state.ActivateDocumentView(previous_view)) {
        return 927;
    }
    return state.Documents().Count() == baseline_count
            && state.engine->SessionCount() == engine_baseline
            && state.RecentDocumentCount() == recent_baseline
        ? 0
        : 928;
}

int RunCutWorkflowSmoke(ApplicationHost& state) noexcept {
    constexpr std::array<const wchar_t*, 6U> kFiles{
        L"inkpod-cut-smoke.inkpod",
        L"inkpod-cut-smoke-0001.inkpod",
        L"inkpod-cut-smoke-0002.inkpod",
        L"inkpod-cut-smoke-0003.inkpod",
        L"inkpod-cut-smoke-0004.inkpod",
        L"inkpod-cut-smoke-0005.inkpod"};
    for (const wchar_t* path : kFiles) {
        DeleteFileW(path);
    }

    const std::size_t baseline_count = state.Documents().Count();
    const std::size_t engine_baseline = state.engine == nullptr
        ? 0U
        : state.engine->SessionCount();
    const DocumentViewId previous_view = state.routing.targets.ActiveDocumentView();
    std::array<DocumentSessionId, inkpod::app::DocumentRegistry::kMaximumSessions>
        baseline{};
    for (std::size_t index = 0U; index < baseline_count; ++index) {
        const auto* document = state.Documents().SessionAt(index);
        if (document == nullptr) {
            return 1030;
        }
        baseline[index] = document->id;
    }
    std::vector<DocumentSessionId> created;
    const auto cleanup = [&]() noexcept {
        bool clean = state.DestroyCutSession(state.Workspace());
        for (std::size_t index = created.size(); index != 0U; --index) {
            clean = state.CloseDocumentSession(created[index - 1U]) && clean;
        }
        if (previous_view) {
            clean = state.ActivateDocumentView(previous_view) && clean;
        }
        for (const wchar_t* path : kFiles) {
            DeleteFileW(path);
        }
        return clean;
    };
    const auto finish = [&](int code) noexcept {
        return cleanup() ? code : 1049;
    };
    const auto query_info = [&](InkpodCutInfo& info) noexcept {
        info = {};
        info.struct_size = sizeof(info);
        InkpodCut* cut = state.Workspace().cut.handle;
        return cut != nullptr && state.engine != nullptr
            && state.engine->Invoke(
                   [cut, &info](InkpodCore*) {
                       return inkpod_cut_info(cut, &info);
                   },
                   false,
                   false) == INKPOD_STATUS_OK;
    };

    const bool had_cut = state.Workspace().cut.handle != nullptr;
    const LRESULT create_result = state.engine == nullptr || had_cut
        ? 0
        : SendMessageW(
              state.Workspace().windows.window,
              WM_COMMAND,
              IDM_FILE_NEW_CUT,
              0);
    if (state.engine == nullptr || had_cut || create_result != 1
        || state.Workspace().cut.handle == nullptr) {
        const std::wstring detail = state.engine == nullptr
            ? std::wstring{}
            : state.engine->LastError();
        std::fwprintf(
            stderr,
            L"Cut create mismatch: result=%lld handle=%p detail=%ls\n",
            static_cast<long long>(create_result),
            static_cast<void*>(state.Workspace().cut.handle),
            detail.c_str());
        return finish(1031);
    }
    for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
        const auto* document = state.Documents().SessionAt(index);
        if (document == nullptr) {
            return finish(1032);
        }
        const bool existed = std::find(
            baseline.cbegin(), baseline.cbegin() + baseline_count, document->id)
            != baseline.cbegin() + baseline_count;
        if (!existed) {
            created.push_back(document->id);
        }
    }
    InkpodCutInfo created_cut{};
    if (created.size() != 5U || !query_info(created_cut)
        || created_cut.cut_id == 0U || created_cut.member_count != 5U
        || created_cut.revision != 0U
        || (created_cut.flags
            & (INKPOD_CUT_FLAG_DIRTY | INKPOD_CUT_FLAG_CAN_UNDO
               | INKPOD_CUT_FLAG_CAN_REDO)) != 0U
        || state.Workspace().cut.current_path != kFiles[0]
        || state.Workspace().cut.cut_name != L"SmokeCut"
        || state.Workspace().cut.members.size() != 5U) {
        return finish(1033);
    }

    std::array<InkpodDocumentInfo, 5U> cell_infos{};
    std::array<DocumentSessionId, 5U> cell_sessions{};
    for (std::size_t index = 0U; index < cell_infos.size(); ++index) {
        std::array<std::uint8_t, 64U> path{};
        InkpodCutMemberInfo member{};
        member.struct_size = sizeof(member);
        member.relative_path = InkpodUtf8Buffer{path.data(), path.size(), 0U};
        InkpodCut* cut = state.Workspace().cut.handle;
        const InkpodStatus member_status = state.engine->Invoke(
            [cut, index, &member](InkpodCore*) {
                return inkpod_cut_member_get(
                    cut, static_cast<std::uint32_t>(index), &member);
            },
            false,
            false);
        std::array<char, 64U> expected{};
        const int expected_count = std::snprintf(
            expected.data(), expected.size(),
            "inkpod-cut-smoke-%04zu.inkpod", index + 1U);
        const std::size_t expected_bytes = expected_count <= 0
            ? 0U
            : static_cast<std::size_t>(expected_count);
        inkpod::app::DocumentSession* document{};
        for (const DocumentSessionId id : created) {
            auto* candidate = state.Documents().Find(id);
            if (candidate != nullptr
                && candidate->shell.current_path == kFiles[index + 1U]) {
                document = candidate;
                break;
            }
        }
        if (member_status != INKPOD_STATUS_OK
            || member.display_number != index + 1U || member.cell_id == 0U
            || member.relative_path.byte_count != expected_bytes
            || expected_bytes >= expected.size()
            || std::memcmp(path.data(), expected.data(), expected_bytes) != 0
            || document == nullptr
            || !state.engine->GetDocumentInfo(
                document->id, document->generation, cell_infos[index])
            || cell_infos[index].cell_id != member.cell_id
            || cell_infos[index].document_uuid_high != member.document_uuid_high
            || cell_infos[index].document_uuid_low != member.document_uuid_low
            || cell_infos[index].width != created_cut.width
            || cell_infos[index].height != created_cut.height
            || cell_infos[index].dpi_x_milli != created_cut.dpi_x_milli
            || cell_infos[index].dpi_y_milli != created_cut.dpi_y_milli) {
            return finish(1034);
        }
        cell_sessions[index] = document->id;
    }
    if (!RefreshSequencePane(state)
        || SendMessageW(
               GetDlgItem(
                   state.Workspace().sequence_palette, IDC_SEQUENCE_CELLS),
               LB_GETCOUNT,
               0,
               0) != 5
        || state.Workspace().sequence_dialog.view.cells.size() != 5U
        || std::any_of(
            state.Workspace().sequence_dialog.view.cells.cbegin(),
            state.Workspace().sequence_dialog.view.cells.cend(),
            [](const auto& cell) {
                return cell.thumbnail_width == 0U
                    || cell.thumbnail_height == 0U
                    || cell.thumbnail_stride_bytes != cell.thumbnail_width * 4U
                    || cell.thumbnail_checksum == 0U || !cell.thumbnail_key;
            })) {
        return finish(1035);
    }

    const HWND sequence_list = GetDlgItem(
        state.Workspace().sequence_palette, IDC_SEQUENCE_CELLS);
    const auto* drag_document = state.Documents().Find(cell_sessions[1]);
    const auto* drag_view = drag_document == nullptr
        ? nullptr
        : drag_document->ViewAt(0U);
    if (drag_view == nullptr || !state.ActivateDocumentView(drag_view->id)
        || !RefreshSequencePane(state)) {
        return finish(1046);
    }
    const std::uint64_t active_thumbnail_checksum =
        state.Workspace().sequence_dialog.view.cells[1].thumbnail_checksum;
    RECT destination_item{};
    RECT source_item{};
    if (sequence_list == nullptr
        || SendMessageW(
               sequence_list,
               LB_GETITEMRECT,
               0,
               reinterpret_cast<LPARAM>(&destination_item))
            == LB_ERR
        || SendMessageW(
               sequence_list,
               LB_GETITEMRECT,
               1,
               reinterpret_cast<LPARAM>(&source_item))
            == LB_ERR) {
        return finish(1046);
    }
    SendMessageW(
        sequence_list,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(
            (source_item.left + source_item.right) / 2,
            (source_item.top + source_item.bottom) / 2));
    SendMessageW(
        sequence_list,
        WM_LBUTTONUP,
        0,
        MAKELPARAM(
            (destination_item.left + destination_item.right) / 2,
            (destination_item.top + destination_item.bottom) / 2));
    InkpodCutInfo reordered{};
    if (!query_info(reordered) || reordered.revision != created_cut.revision + 1U
        || state.Workspace().cut.members.size() != 5U
        || state.Workspace().cut.members[0].document_uuid_high
            != cell_infos[1].document_uuid_high
        || state.Workspace().cut.members[0].document_uuid_low
            != cell_infos[1].document_uuid_low
        || state.Workspace().cut.members[1].document_uuid_high
            != cell_infos[0].document_uuid_high
        || state.Workspace().cut.members[1].document_uuid_low
            != cell_infos[0].document_uuid_low
        || state.Workspace().cut.members[2].document_uuid_low
            != cell_infos[2].document_uuid_low
        || state.Workspace().cut.members[3].document_uuid_low
            != cell_infos[3].document_uuid_low
        || state.Workspace().cut.members[4].document_uuid_low
            != cell_infos[4].document_uuid_low
        || state.Workspace().sequence_dialog.view.active_index != 0U
        || state.Workspace().sequence_dialog.view.cells[0].thumbnail_checksum
            != active_thumbnail_checksum) {
        std::fprintf(
            stderr,
            "Cut reorder mismatch: revision=%llu expected=%llu members=%zu "
            "active=%u first=%llu expected_first=%llu thumbnail=%llu/%llu\n",
            static_cast<unsigned long long>(reordered.revision),
            static_cast<unsigned long long>(created_cut.revision + 1U),
            state.Workspace().cut.members.size(),
            state.Workspace().sequence_dialog.view.active_index,
            state.Workspace().cut.members.empty()
                ? 0ULL
                : static_cast<unsigned long long>(
                    state.Workspace().cut.members[0].cell_id),
            static_cast<unsigned long long>(cell_infos[1].cell_id),
            state.Workspace().sequence_dialog.view.cells.empty()
                ? 0ULL
                : static_cast<unsigned long long>(
                    state.Workspace().sequence_dialog.view.cells[0]
                        .thumbnail_checksum),
            static_cast<unsigned long long>(active_thumbnail_checksum));
        return finish(1047);
    }
    constexpr std::array<std::uint32_t, 5U> kRenumbered{1U, 2U, 3U, 4U, 5U};
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CUT_SEQUENCE_RENUMBER,
            0) != 1
        || !std::equal(
            state.Workspace().cut.members.cbegin(),
            state.Workspace().cut.members.cend(),
            kRenumbered.cbegin(),
            [](const auto& member, std::uint32_t number) {
                return member.display_number == number;
            })) {
        return finish(1048);
    }
    SendMessageW(sequence_list, LB_SETCURSEL, 0, 0);
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CUT_SEQUENCE_REMOVE,
            0) != 1
        || state.Workspace().cut.members.size() != 4U
        || GetFileAttributesW(kFiles[2]) == INVALID_FILE_ATTRIBUTES
        || state.Workspace().sequence_dialog.view.active_index != UINT32_MAX
        || state.Workspace().sequence_dialog.view.target_text.find(
               L"現在のセルはメンバー外") == std::wstring::npos) {
        return finish(1049);
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CUT_UNDO,
            0) != 1
        || state.Workspace().cut.members.size() != 5U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_CUT_REDO,
               0) != 1
        || state.Workspace().cut.members.size() != 4U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_CUT_SEQUENCE_ADD,
               0) != 1
        || state.Workspace().cut.members.size() != 5U
        || state.Workspace().cut.members[4].document_uuid_high
            != cell_infos[1].document_uuid_high
        || state.Workspace().cut.members[4].document_uuid_low
            != cell_infos[1].document_uuid_low) {
        return finish(1050);
    }
    SendMessageW(sequence_list, LB_SETCURSEL, 4, 0);
    for (std::uint32_t move = 0U; move < 4U; ++move) {
        if (SendMessageW(
                state.Workspace().windows.window,
                WM_COMMAND,
                IDM_CUT_SEQUENCE_MOVE_UP,
                0) != 1) {
            return finish(1052);
        }
    }
    if (state.Workspace().cut.members.size() != 5U
        || state.Workspace().cut.members[0].document_uuid_high
            != cell_infos[1].document_uuid_high
        || state.Workspace().cut.members[0].document_uuid_low
            != cell_infos[1].document_uuid_low
        || state.Workspace().cut.members[1].document_uuid_high
            != cell_infos[0].document_uuid_high
        || state.Workspace().cut.members[1].document_uuid_low
            != cell_infos[0].document_uuid_low
        || state.Workspace().cut.members[2].document_uuid_low
            != cell_infos[2].document_uuid_low
        || state.Workspace().cut.members[3].document_uuid_low
            != cell_infos[3].document_uuid_low
        || state.Workspace().cut.members[4].document_uuid_low
            != cell_infos[4].document_uuid_low
        || state.Workspace().sequence_dialog.view.active_index != 0U
        || state.Workspace().sequence_dialog.view.cells[0].thumbnail_checksum
            != active_thumbnail_checksum) {
        return finish(1052);
    }
    InkpodCutInfo sequence_edited{};
    if (!query_info(sequence_edited)
        || sequence_edited.member_count != 5U
        || (sequence_edited.flags & INKPOD_CUT_FLAG_CAN_UNDO) == 0U) {
        return finish(1051);
    }

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CUT_PROPERTIES,
            0) != 1) {
        return finish(1036);
    }
    InkpodCutInfo updated{};
    if (!query_info(updated)
        || updated.revision != sequence_edited.revision + 1U
        || updated.duration_frames != created_cut.duration_frames + 1U
        || (updated.flags
            & (INKPOD_CUT_FLAG_DIRTY | INKPOD_CUT_FLAG_CAN_UNDO))
            != (INKPOD_CUT_FLAG_DIRTY | INKPOD_CUT_FLAG_CAN_UNDO)
        || state.Workspace().cut.cut_name != L"SmokeCut-updated") {
        return finish(1037);
    }
    for (std::size_t index = 0U; index < cell_infos.size(); ++index) {
        auto* document = state.Documents().Find(cell_sessions[index]);
        InkpodDocumentInfo unchanged{};
        if (document == nullptr
            || !state.engine->GetDocumentInfo(
                document->id, document->generation, unchanged)
            || unchanged.cell_id != cell_infos[index].cell_id
            || unchanged.document_revision != cell_infos[index].document_revision
            || unchanged.width != cell_infos[index].width
            || unchanged.height != cell_infos[index].height
            || unchanged.dpi_x_milli != cell_infos[index].dpi_x_milli
            || unchanged.dpi_y_milli != cell_infos[index].dpi_y_milli) {
            return finish(1038);
        }
    }

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CUT_UNDO,
            0) != 1) {
        return finish(1039);
    }
    InkpodCutInfo undone{};
    if (!query_info(undone) || undone.duration_frames != created_cut.duration_frames
        || (undone.flags & INKPOD_CUT_FLAG_CAN_REDO) == 0U
        || state.Workspace().cut.cut_name != L"SmokeCut") {
        return finish(1040);
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_CUT_REDO,
            0) != 1) {
        return finish(1041);
    }
    InkpodCutInfo redone{};
    if (!query_info(redone) || redone.duration_frames != updated.duration_frames
        || state.Workspace().cut.cut_name != L"SmokeCut-updated"
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_CUT_SAVE,
               0) != 1) {
        return finish(1042);
    }
    InkpodCutInfo saved{};
    if (!query_info(saved) || (saved.flags & INKPOD_CUT_FLAG_DIRTY) != 0U
        || !state.DestroyCutSession(state.Workspace())
        || OpenDocumentFromPath(state, kFiles[0]) != INKPOD_STATUS_OK) {
        return finish(1043);
    }
    InkpodCutInfo reopened{};
    if (!query_info(reopened) || reopened.member_count != 5U
        || reopened.duration_frames != updated.duration_frames
        || reopened.cut_id != created_cut.cut_id
        || state.Workspace().cut.cut_name != L"SmokeCut-updated"
        || state.Workspace().cut.members.size() != 5U
        || state.Workspace().cut.members[0].document_uuid_high
            != cell_infos[1].document_uuid_high
        || state.Workspace().cut.members[0].document_uuid_low
            != cell_infos[1].document_uuid_low
        || state.Workspace().cut.members[1].document_uuid_high
            != cell_infos[0].document_uuid_high
        || state.Workspace().cut.members[1].document_uuid_low
            != cell_infos[0].document_uuid_low
        || state.Workspace().cut.members[2].document_uuid_low
            != cell_infos[2].document_uuid_low
        || state.Workspace().cut.members[3].document_uuid_low
            != cell_infos[3].document_uuid_low
        || state.Workspace().cut.members[4].document_uuid_low
            != cell_infos[4].document_uuid_low
        || state.Workspace().sequence_dialog.view.active_index != 0U
        || state.Workspace().sequence_dialog.view.cells.size() != 5U
        || std::any_of(
            state.Workspace().sequence_dialog.view.cells.cbegin(),
            state.Workspace().sequence_dialog.view.cells.cend(),
            [](const auto& cell) {
                return cell.thumbnail_width == 0U
                    || cell.thumbnail_height == 0U
                    || cell.thumbnail_checksum == 0U || !cell.thumbnail_key;
            })) {
        return finish(1044);
    }
    const bool counts_ok = state.Documents().Count() == baseline_count + 5U
        && state.engine->SessionCount() == engine_baseline + 5U;
    return finish(counts_ok ? 0 : 1045);
}

int RunRevisionMaxPerformanceSmoke(ApplicationHost& state) noexcept {
    constexpr std::uint32_t kDocumentExtent = 1024U;
    constexpr int kTileRows = 16;
    constexpr int kWarmWheelPairs = 32;
    constexpr int kMeasuredWheelPairs = 256;
    constexpr int kMeasuredStrokes = 16;
    constexpr int kStrokeSegments = 32;

    auto* group = state.Workspace().editors.Active();
    HWND canvas = state.Workspace().windows.canvas;
    if (group == nullptr || canvas == nullptr || group->canvas != canvas
        || state.engine == nullptr || state.renderer == nullptr
        || CreateCell(state, kDocumentExtent, kDocumentExtent, 96'000U)
            != INKPOD_STATUS_OK
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || !state.renderer->WaitQueueIdleForSmokeTest()) {
        return 901;
    }
    PumpPendingWindowMessages();

    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(canvas, bounds)) {
        return 902;
    }
    const double zoom = (bounds.right - bounds.left)
        / static_cast<double>(kDocumentExtent);
    if (!std::isfinite(zoom) || zoom <= 0.0
        || std::abs(
               (bounds.bottom - bounds.top)
                   / static_cast<double>(kDocumentExtent)
               - zoom)
            > 0.001) {
        return 903;
    }

    const auto device_x = [bounds, zoom](double document_x) noexcept {
        return static_cast<int>(std::lround(bounds.left + document_x * zoom));
    };
    const auto device_y = [bounds, zoom](double document_y) noexcept {
        return static_cast<int>(std::lround(bounds.top + document_y * zoom));
    };
    const auto send_stroke = [&](bool vertical, double fixed_document) noexcept {
        const int start_x = vertical ? device_x(fixed_document) : device_x(1.0);
        const int start_y = vertical ? device_y(1.0) : device_y(fixed_document);
        state.renderer->SetQueuePausedForSmokeTest(true);
        bool sent = SendMessageW(
                        canvas,
                        WM_LBUTTONDOWN,
                        MK_LBUTTON,
                        MAKELPARAM(start_x, start_y))
            == 1;
        for (int segment = 1; sent && segment <= kStrokeSegments; ++segment) {
            const double moving_document = 1.0
                + (static_cast<double>(kDocumentExtent) - 2.0)
                    * static_cast<double>(segment)
                    / static_cast<double>(kStrokeSegments);
            const int x = vertical ? start_x : device_x(moving_document);
            const int y = vertical ? device_y(moving_document) : start_y;
            sent = SendMessageW(
                       canvas,
                       WM_MOUSEMOVE,
                       MK_LBUTTON,
                       MAKELPARAM(x, y))
                == 1;
        }
        const int end_x = vertical
            ? start_x
            : device_x(static_cast<double>(kDocumentExtent) - 1.0);
        const int end_y = vertical
            ? device_y(static_cast<double>(kDocumentExtent) - 1.0)
            : start_y;
        if (sent) {
            sent = SendMessageW(
                       canvas, WM_LBUTTONUP, 0, MAKELPARAM(end_x, end_y))
                    == 1
                && state.engine->WaitIdle() == INKPOD_STATUS_OK;
        }
        state.renderer->SetQueuePausedForSmokeTest(false);
        return sent && state.renderer->WaitQueueIdleForSmokeTest();
    };

    // Materialize every 64x64 tile before timing. This makes a payload scan in
    // a view-only snapshot scale with the complete allocated tile grid.
    for (int row = 0; row < kTileRows; ++row) {
        if (!send_stroke(
                false,
                static_cast<double>(row * 64) + 16.0)) {
            return 904;
        }
    }
    InkpodResourceUsage core_usage{};
    if (!state.renderer->WaitQueueIdleForSmokeTest()
        || !QueryCoreResourceUsage(
            state,
            state.Document().id,
            state.Document().generation,
            core_usage)
        || core_usage.document_tile_count
            != static_cast<std::uint64_t>(kTileRows * kTileRows)
        || core_usage.document_tile_bytes != 1'048'576U) {
        return 905;
    }

    RECT canvas_rect{};
    if (GetWindowRect(canvas, &canvas_rect) == FALSE) {
        return 906;
    }
    const int wheel_x = canvas_rect.left
        + (canvas_rect.right - canvas_rect.left) / 2;
    const int wheel_y = canvas_rect.top
        + (canvas_rect.bottom - canvas_rect.top) / 2;
    const auto send_wheel = [&](int delta) noexcept {
        state.renderer->SetQueuePausedForSmokeTest(true);
        const bool sent = SendMessageW(
                              canvas,
                              WM_MOUSEWHEEL,
                              MAKEWPARAM(0, delta),
                              MAKELPARAM(wheel_x, wheel_y))
            == 1;
        state.renderer->SetQueuePausedForSmokeTest(false);
        return sent && state.renderer->WaitQueueIdleForSmokeTest();
    };
    const auto send_wheel_pair = [&]() noexcept {
        return send_wheel(WHEEL_DELTA) && send_wheel(-WHEEL_DELTA);
    };
    for (int pair = 0; pair < kWarmWheelPairs; ++pair) {
        if (!send_wheel_pair()) {
            return 907;
        }
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK
        || !state.renderer->WaitQueueIdleForSmokeTest()) {
        return 908;
    }

    InkpodDocumentInfo before_wheel{};
    if (!QueryDocument(state, before_wheel)) {
        return 909;
    }
    const auto wheel_resources_before = state.renderer->ResourceUsage();
    const std::uint64_t wheel_frames_before =
        state.renderer->PresentedFrameCount(
            group->canvas_id,
            group->generation);
    const auto wheel_started = std::chrono::steady_clock::now();
    for (int pair = 0; pair < kMeasuredWheelPairs; ++pair) {
        if (!send_wheel_pair()) {
            return 910;
        }
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK
        || !state.renderer->WaitQueueIdleForSmokeTest()) {
        return 911;
    }
    const auto wheel_elapsed = std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::steady_clock::now() - wheel_started)
                                   .count();
    InkpodDocumentInfo after_wheel{};
    const auto wheel_resources_after = state.renderer->ResourceUsage();
    const std::uint64_t wheel_frames_after =
        state.renderer->PresentedFrameCount(
            group->canvas_id,
            group->generation);
    const bool queried_after_wheel = QueryDocument(state, after_wheel);
    if (!queried_after_wheel
        || after_wheel.document_revision != before_wheel.document_revision
        || after_wheel.main_plane_checksum != before_wheel.main_plane_checksum
        || after_wheel.view_revision == before_wheel.view_revision
        || wheel_frames_after - wheel_frames_before
            != static_cast<std::uint64_t>(kMeasuredWheelPairs * 2)
        || wheel_resources_after.queue_rejection_count
            != wheel_resources_before.queue_rejection_count
        || wheel_resources_after.resource_limit_count
            != wheel_resources_before.resource_limit_count) {
        std::fprintf(
            stderr,
            "wheel validation query=%d document=%llu/%llu view=%llu/%llu "
            "checksum=%llu/%llu frames=%llu replacements=%llu/%llu "
            "rejections=%llu/%llu limits=%llu/%llu\n",
            queried_after_wheel ? 1 : 0,
            static_cast<unsigned long long>(before_wheel.document_revision),
            static_cast<unsigned long long>(after_wheel.document_revision),
            static_cast<unsigned long long>(before_wheel.view_revision),
            static_cast<unsigned long long>(after_wheel.view_revision),
            static_cast<unsigned long long>(before_wheel.main_plane_checksum),
            static_cast<unsigned long long>(after_wheel.main_plane_checksum),
            static_cast<unsigned long long>(
                wheel_frames_after - wheel_frames_before),
            static_cast<unsigned long long>(
                wheel_resources_before.queue_replacement_count),
            static_cast<unsigned long long>(
                wheel_resources_after.queue_replacement_count),
            static_cast<unsigned long long>(
                wheel_resources_before.queue_rejection_count),
            static_cast<unsigned long long>(
                wheel_resources_after.queue_rejection_count),
            static_cast<unsigned long long>(
                wheel_resources_before.resource_limit_count),
            static_cast<unsigned long long>(
                wheel_resources_after.resource_limit_count));
        return 912;
    }
    std::fprintf(
        stderr,
        "inkpod-native-performance scenario=wheel_zoom pairs=%d "
        "events=%d tiles=%llu tile_bytes=%llu presented_frames=%llu "
        "queue_replacements=%llu "
        "elapsed_ns=%lld\n",
        kMeasuredWheelPairs,
        kMeasuredWheelPairs * 2,
        static_cast<unsigned long long>(core_usage.document_tile_count),
        static_cast<unsigned long long>(core_usage.document_tile_bytes),
        static_cast<unsigned long long>(
            wheel_frames_after - wheel_frames_before),
        static_cast<unsigned long long>(
            wheel_resources_after.queue_replacement_count
            - wheel_resources_before.queue_replacement_count),
        static_cast<long long>(wheel_elapsed));

    // One untimed stroke warms the complete input/Core/renderer route without
    // overlapping the measured tile-centre strokes.
    if (!send_stroke(true, 8.0)
        || !state.renderer->WaitQueueIdleForSmokeTest()) {
        return 913;
    }
    InkpodDocumentInfo before_drawing{};
    if (!QueryDocument(state, before_drawing)) {
        return 914;
    }
    const inkpod::app::EngineMetrics drawing_metrics_before =
        state.engine->Metrics();
    const auto drawing_resources_before = state.renderer->ResourceUsage();
    const std::uint64_t drawing_frames_before =
        state.renderer->PresentedFrameCount(
            group->canvas_id,
            group->generation);
    const auto drawing_started = std::chrono::steady_clock::now();
    for (int stroke = 0; stroke < kMeasuredStrokes; ++stroke) {
        if (!send_stroke(
                true,
                static_cast<double>(stroke * 64) + 32.0)) {
            return 915;
        }
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK
        || !state.renderer->WaitQueueIdleForSmokeTest()) {
        return 916;
    }
    const auto drawing_elapsed =
        std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now() - drawing_started)
            .count();
    InkpodDocumentInfo after_drawing{};
    const inkpod::app::EngineMetrics drawing_metrics_after =
        state.engine->Metrics();
    const auto drawing_resources_after = state.renderer->ResourceUsage();
    const std::uint64_t drawing_frames_after =
        state.renderer->PresentedFrameCount(
            group->canvas_id,
            group->generation);
    if (!QueryDocument(state, after_drawing)
        || after_drawing.document_revision
            != before_drawing.document_revision + kMeasuredStrokes
        || after_drawing.main_plane_checksum
            == before_drawing.main_plane_checksum
        || drawing_metrics_after.completed_strokes
            != drawing_metrics_before.completed_strokes + kMeasuredStrokes
        || drawing_metrics_after.completed_samples
            != drawing_metrics_before.completed_samples
                + static_cast<std::uint64_t>(
                    kMeasuredStrokes * (kStrokeSegments + 2))
        || drawing_frames_after - drawing_frames_before
            != static_cast<std::uint64_t>(kMeasuredStrokes)
        || drawing_resources_after.queue_rejection_count
            != drawing_resources_before.queue_rejection_count
        || drawing_resources_after.resource_limit_count
            != drawing_resources_before.resource_limit_count) {
        return 917;
    }
    std::fprintf(
        stderr,
        "inkpod-native-performance scenario=drawing strokes=%d "
        "samples=%llu presented_frames=%llu queue_replacements=%llu "
        "elapsed_ns=%lld\n",
        kMeasuredStrokes,
        static_cast<unsigned long long>(
            drawing_metrics_after.completed_samples
            - drawing_metrics_before.completed_samples),
        static_cast<unsigned long long>(
            drawing_frames_after - drawing_frames_before),
        static_cast<unsigned long long>(
            drawing_resources_after.queue_replacement_count
            - drawing_resources_before.queue_replacement_count),
        static_cast<long long>(drawing_elapsed));
    return 0;
}


}  // namespace inkpod::windows::ui::runtime

namespace inkpod::windows::ui {

int RunApplicationSmoke(app::ApplicationHost& state) noexcept {
    const auto context = state.routing.targets.Capture();
    int exit_code = state.Workspace().application != &state
            || state.Workspace().id != state.routing.targets.Workspace()
            || state.Workspace().windows.window == nullptr
            || state.Document().id != state.routing.targets.DocumentSession()
            || state.Document().Core() != state.engine.get()
            || state.Document().ActiveView() == nullptr
            || !context.document_view.has_value()
            || state.Document().ActiveView()->id != context.document_view.value()
        ? 731
        : 0;
    if (exit_code == 0) {
        const InkpodShortcutSequence* original = FindShortcutSequence(
            state.shortcuts.bindings, IDM_VIEW_VECTOR_ENDPOINTS);
        if (original == nullptr) {
            exit_code = 732;
        } else {
            const InkpodShortcutSequence saved = *original;
            InkpodShortcutSequence replacement{};
            replacement.struct_size = sizeof(replacement);
            replacement.command_id = IDM_VIEW_VECTOR_ENDPOINTS;
            replacement.stroke_count = 1U;
            replacement.strokes[0] = {
                static_cast<std::uint32_t>('9'),
                INKPOD_SHORTCUT_MODIFIER_CONTROL | INKPOD_SHORTCUT_MODIFIER_ALT};
            UINT resolved{};
            const InkpodStatus rebind = RebindShortcut(
                *state.engine, state.shortcuts, replacement, false);
            const bool shortcut_resolved = rebind == INKPOD_STATUS_OK
                && runtime::ResolveConfiguredShortcut(
                    state,
                    static_cast<std::uint32_t>('9'),
                    INKPOD_SHORTCUT_MODIFIER_CONTROL | INKPOD_SHORTCUT_MODIFIER_ALT,
                    resolved)
                && resolved == IDM_VIEW_VECTOR_ENDPOINTS;
            const InkpodStatus restore = rebind == INKPOD_STATUS_OK
                ? RebindShortcut(*state.engine, state.shortcuts, saved, false)
                : rebind;
            if (!shortcut_resolved || restore != INKPOD_STATUS_OK) {
                exit_code = 733;
            }
        }
    }
    if (exit_code == 0) {
        exit_code = runtime::RunCommandContextSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunLocatorPaneSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunSequencePaneSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunLightTablePaneSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunSubpalettePaneSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunJobProgressPaneSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunDrawingPersistenceSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunPaintingRecoverySmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunDocumentEditingSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunAnnotationWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunShootingFrameWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunProductionWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunVectorWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunImageEffectsSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunBatchWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunMagnifiedRasterHitSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunMultiDocumentTabSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunSplitEditorGroupSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunTabDragSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunG13ResourceScenarioSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunMultiWorkspaceWindowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunEditorStateOwnershipSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunCellCreationSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunCutWorkflowSmoke(state);
    }
    if (exit_code != 0) {
        std::fprintf(stderr, "inkpod application smoke failed: %d\n", exit_code);
    }
    return exit_code;
}

int RunPerformanceSmoke(app::ApplicationHost& state) noexcept {
    const int exit_code = runtime::RunRevisionMaxPerformanceSmoke(state);
    if (exit_code != 0) {
        std::fprintf(
            stderr,
            "inkpod native performance smoke failed: %d\n",
            exit_code);
    }
    return exit_code;
}

}  // namespace inkpod::windows::ui
