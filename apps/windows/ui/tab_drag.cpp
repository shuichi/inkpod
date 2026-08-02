#include "tab_drag.h"

#include <commctrl.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

#include "app/application_host.h"
#include "app/resource.h"
#include "app/workspace_window.h"
#include "main_window_runtime.h"
#include "main_window_runtime_internal.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT_PTR kTabDragSubclass = 2U;
constexpr wchar_t kDragImageProperty[] = L"Inkpod.TabDragImage";

bool ContainsScreenPoint(HWND window, POINT point) noexcept {
    RECT bounds{};
    return window != nullptr && GetWindowRect(window, &bounds) != FALSE
        && PtInRect(&bounds, point) != FALSE;
}

app::WorkspaceWindow* WorkspaceFromTabs(HWND tabs) noexcept {
    const HWND root = GetAncestor(tabs, GA_ROOT);
    return root == nullptr
        ? nullptr
        : reinterpret_cast<app::WorkspaceWindow*>(
            GetWindowLongPtrW(root, GWLP_USERDATA));
}

app::DocumentViewId TabViewAt(HWND tabs, int index) noexcept {
    TCITEMW item{};
    item.mask = TCIF_PARAM;
    return index >= 0 && TabCtrl_GetItem(tabs, index, &item) != FALSE
        ? app::DocumentViewId{static_cast<std::uint64_t>(item.lParam)}
        : app::DocumentViewId{};
}

int HitTab(HWND tabs, POINT client) noexcept {
    TCHITTESTINFO hit{};
    hit.pt = client;
    const int index = TabCtrl_HitTest(tabs, &hit);
    return index >= 0 && (hit.flags & TCHT_NOWHERE) == 0U ? index : -1;
}

std::size_t InsertionIndex(HWND tabs, POINT screen) noexcept {
    POINT client = screen;
    ScreenToClient(tabs, &client);
    const int hit = HitTab(tabs, client);
    if (hit < 0) {
        return static_cast<std::size_t>(std::max(0, TabCtrl_GetItemCount(tabs)));
    }
    RECT item{};
    if (TabCtrl_GetItemRect(tabs, hit, &item) == FALSE) {
        return static_cast<std::size_t>(hit);
    }
    return static_cast<std::size_t>(
        client.x >= item.left + (item.right - item.left) / 2 ? hit + 1 : hit);
}

std::optional<app::TabDropTarget> FindDropTarget(
    app::ApplicationHost& state,
    const app::DragToken& token,
    POINT screen) noexcept {
    const HWND hit = WindowFromPoint(screen);
    const HWND hit_root = hit == nullptr ? nullptr : GetAncestor(hit, GA_ROOT);
    bool inside_workspace{};
    for (std::size_t workspace_index = 0U;
         workspace_index < state.Workspaces().Count(); ++workspace_index) {
        app::WorkspaceWindow* workspace = state.Workspaces().At(workspace_index);
        if (workspace == nullptr || workspace->windows.window == nullptr
            || (IsWindowVisible(workspace->windows.window) == FALSE
                && !state.lifetime.smoke_test)
            || (hit_root != workspace->windows.window
                && !(state.lifetime.smoke_test
                    && ContainsScreenPoint(workspace->windows.window, screen)))
            || !ContainsScreenPoint(workspace->windows.window, screen)) {
            continue;
        }
        inside_workspace = true;
        for (std::size_t group_index = 0U;
             group_index < workspace->editors.GroupCount(); ++group_index) {
            const app::EditorGroup* group = workspace->editors.GroupAt(group_index);
            if (group == nullptr
                || (!ContainsScreenPoint(group->document_tabs, screen)
                    && !ContainsScreenPoint(group->canvas, screen))) {
                continue;
            }
            const std::size_t insertion = ContainsScreenPoint(
                    group->document_tabs, screen)
                ? InsertionIndex(group->document_tabs, screen)
                : group->ViewCount();
            const bool same_group = token.context.workspace == workspace->id
                && token.context.editor_group == group->id;
            return app::TabDropTarget{
                same_group
                    ? app::TabDropKind::Reorder
                    : app::TabDropKind::EditorGroup,
                workspace->id,
                group->id,
                insertion};
        }
        break;
    }
    if (!inside_workspace) {
        return app::TabDropTarget{app::TabDropKind::TearOut, {}, {}, 0U};
    }
    return std::nullopt;
}

void EndDragImage(app::ApplicationHost& state) noexcept {
    const app::DragToken* token = state.TabDrag().Token();
    const app::WorkspaceWindow* workspace = token == nullptr
        || !token->context.workspace.has_value()
        ? nullptr
        : state.FindWorkspace(token->context.workspace.value());
    const app::EditorGroup* group = workspace == nullptr
        || !token->context.editor_group.has_value()
        ? nullptr
        : workspace->editors.Find(token->context.editor_group.value());
    const HWND tabs = group == nullptr ? nullptr : group->document_tabs;
    auto image = tabs == nullptr
        ? nullptr
        : reinterpret_cast<HIMAGELIST>(RemovePropW(tabs, kDragImageProperty));
    if (image != nullptr) {
        ImageList_DragLeave(nullptr);
        ImageList_EndDrag();
        ImageList_Destroy(image);
    }
}

void RestoreCapturedContext(
    app::ApplicationHost& state,
    const app::CommandContext& context) noexcept {
    if (state.routing.targets.Resolve(context, app::kDocumentViewCommandScope)
            != app::CommandResolveStatus::Ok
        || !context.workspace.has_value()
        || !context.document_view.has_value()) {
        return;
    }
    (void)state.ActivateWorkspaceWindow(context.workspace.value(), false);
    (void)state.ActivateDocumentView(context.document_view.value());
    runtime::UpdateMenuState(state);
}

bool DragIsBlocked(
    app::ApplicationHost& state,
    const app::WorkspaceWindow& source,
    app::DocumentViewId view,
    HWND tabs) noexcept {
    const app::DocumentSession* document = state.Documents().FindByView(view);
    const app::DocumentView* document_view = document == nullptr
        ? nullptr
        : document->FindView(view);
    const HWND capture = GetCapture();
    return document_view == nullptr
        || IsWindowEnabled(source.windows.window) == FALSE
        || source.tools.floating_active
        || state.effects.task != nullptr
        || document_view->presentation.active_drag.has_value()
        || (capture != nullptr && capture != tabs);
}

bool BeginDragImage(HWND tabs, int source_index, POINT screen) noexcept {
    RECT item{};
    POINT client = screen;
    ScreenToClient(tabs, &client);
    if (TabCtrl_GetItemRect(tabs, source_index, &item) == FALSE) {
        return false;
    }
    const int width = std::max(1L, item.right - item.left);
    const int height = std::max(1L, item.bottom - item.top);
    HDC source = GetDC(tabs);
    HDC memory = source == nullptr ? nullptr : CreateCompatibleDC(source);
    HBITMAP bitmap = memory == nullptr
        ? nullptr
        : CreateCompatibleBitmap(source, width, height);
    auto image = ImageList_Create(width, height, ILC_COLOR32, 1, 1);
    bool captured = source != nullptr && memory != nullptr && bitmap != nullptr
        && image != nullptr;
    HGDIOBJ previous{};
    if (captured) {
        previous = SelectObject(memory, bitmap);
        captured = previous != nullptr
            && BitBlt(
                memory,
                0,
                0,
                width,
                height,
                source,
                item.left,
                item.top,
                SRCCOPY) != FALSE
            && ImageList_Add(image, bitmap, nullptr) >= 0;
    }
    if (previous != nullptr) {
        SelectObject(memory, previous);
    }
    if (bitmap != nullptr) {
        DeleteObject(bitmap);
    }
    if (memory != nullptr) {
        DeleteDC(memory);
    }
    if (source != nullptr) {
        ReleaseDC(tabs, source);
    }
    if (!captured
        || ImageList_BeginDrag(
               image,
               0,
               std::clamp(client.x - item.left, 0L, item.right - item.left),
               std::clamp(client.y - item.top, 0L, item.bottom - item.top)) == FALSE) {
        ImageList_Destroy(image);
        return false;
    }
    SetPropW(tabs, kDragImageProperty, reinterpret_cast<HANDLE>(image));
    if (GetPropW(tabs, kDragImageProperty) == nullptr) {
        ImageList_EndDrag();
        ImageList_Destroy(image);
        return false;
    }
    ImageList_DragEnter(nullptr, screen.x, screen.y);
    return true;
}

void RefreshTransferWindows(
    app::ApplicationHost& state,
    app::WorkspaceWindowId source,
    app::WorkspaceWindowId destination) noexcept {
    if (state.FindWorkspace(source) != nullptr) {
        (void)state.ActivateWorkspaceWindow(source, false);
        runtime::UpdateMenuState(state);
    }
    if (state.FindWorkspace(destination) != nullptr) {
        (void)state.ActivateWorkspaceWindow(destination, true);
        runtime::UpdateMenuState(state);
    }
}

bool CommitDrop(
    app::ApplicationHost& state,
    const app::TabDropRequest& request,
    POINT screen) noexcept {
    const app::CommandContext& context = request.token.context;
    if (state.routing.targets.Resolve(context, app::kDocumentViewCommandScope)
            != app::CommandResolveStatus::Ok
        || !context.workspace.has_value()
        || !context.editor_group.has_value()
        || !context.document_view.has_value()) {
        RestoreCapturedContext(state, request.restore_context);
        return false;
    }
    app::WorkspaceWindow* source = state.FindWorkspace(context.workspace.value());
    app::EditorGroup* source_group = source == nullptr
        ? nullptr
        : source->editors.Find(context.editor_group.value());
    const app::DocumentViewId view = context.document_view.value();
    if (source == nullptr || source_group == nullptr
        || source_group->ViewIndex(view) != request.source_index
        || DragIsBlocked(state, *source, view, source_group->document_tabs)) {
        RestoreCapturedContext(state, request.restore_context);
        return false;
    }
    const bool duplicate = request.token.operation == app::DragOperation::TabCopy;
    if (request.target.kind == app::TabDropKind::TearOut) {
        const bool result = runtime::MoveOrDuplicateViewToNewWorkspace(
            state, context, duplicate, screen);
        if (!result) {
            RestoreCapturedContext(state, request.restore_context);
        }
        return result;
    }
    app::WorkspaceWindow* target = state.FindWorkspace(request.target.workspace);
    app::EditorGroup* target_group = target == nullptr
        ? nullptr
        : target->editors.Find(request.target.group);
    if (target == nullptr || target_group == nullptr
        || request.target.insertion_index > target_group->ViewCount()) {
        RestoreCapturedContext(state, request.restore_context);
        return false;
    }
    bool committed{};
    if (duplicate) {
        committed = state.ActivateDocumentView(view)
            && runtime::CreateDocumentViewInGroup(
                state,
                target_group->id,
                target->windows.window,
                request.target.insertion_index);
    } else {
        committed = state.MoveDocumentView(
            view,
            target->id,
            target_group->id,
            request.target.insertion_index);
    }
    if (!committed) {
        RestoreCapturedContext(state, request.restore_context);
        return false;
    }
    RefreshTransferWindows(state, source->id, target->id);
    return true;
}

void AppendMenuCommand(HMENU popup, HMENU main, UINT command, bool enabled) noexcept {
    std::array<wchar_t, 128U> label{};
    if (GetMenuStringW(
            main,
            command,
            label.data(),
            static_cast<int>(label.size()),
            MF_BYCOMMAND) == 0) {
        return;
    }
    AppendMenuW(
        popup,
        MF_STRING | (enabled ? MF_ENABLED : MF_GRAYED),
        command,
        label.data());
}

void ShowTabContextMenu(
    app::ApplicationHost& state,
    app::WorkspaceWindow& workspace,
    HWND tabs,
    POINT screen) noexcept {
    POINT client = screen;
    ScreenToClient(tabs, &client);
    int index = HitTab(tabs, client);
    if (index < 0 && screen.x == -1 && screen.y == -1) {
        index = TabCtrl_GetCurSel(tabs);
        RECT selected{};
        if (index >= 0 && TabCtrl_GetItemRect(tabs, index, &selected) != FALSE) {
            client = POINT{selected.left, selected.bottom};
            screen = client;
            ClientToScreen(tabs, &screen);
        }
    }
    const app::DocumentViewId view = TabViewAt(tabs, index);
    if (!view || !state.ActivateWorkspaceWindow(workspace.id, true)
        || !state.ActivateDocumentView(view)) {
        return;
    }
    const HMENU main = GetMenu(workspace.windows.window);
    const HMENU popup = CreatePopupMenu();
    if (main == nullptr || popup == nullptr) {
        if (popup != nullptr) {
            DestroyMenu(popup);
        }
        return;
    }
    AppendMenuCommand(popup, main, IDM_TAB_MOVE_LEFT, index > 0);
    AppendMenuCommand(
        popup,
        main,
        IDM_TAB_MOVE_RIGHT,
        index >= 0 && index + 1 < TabCtrl_GetItemCount(tabs));
    AppendMenuW(popup, MF_SEPARATOR, 0, nullptr);
    AppendMenuCommand(
        popup,
        main,
        IDM_EDITOR_MOVE_OTHER_GROUP,
        workspace.editors.GroupCount() == 2U);
    AppendMenuCommand(
        popup,
        main,
        IDM_EDITOR_NEW_VIEW_OTHER_GROUP,
        workspace.editors.GroupCount() == 2U);
    AppendMenuCommand(
        popup,
        main,
        IDM_VIEW_MOVE_NEXT_WINDOW,
        state.Workspaces().Count() > 1U);
    AppendMenuCommand(
        popup,
        main,
        IDM_VIEW_DUPLICATE_NEXT_WINDOW,
        state.Workspaces().Count() > 1U);
    AppendMenuCommand(popup, main, IDM_VIEW_MOVE_NEW_WINDOW, true);
    AppendMenuCommand(popup, main, IDM_VIEW_DUPLICATE_NEW_WINDOW, true);
    const UINT command = TrackPopupMenuEx(
        popup,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN,
        screen.x,
        screen.y,
        workspace.windows.window,
        nullptr);
    DestroyMenu(popup);
    if (command != 0U) {
        SendMessageW(workspace.windows.window, WM_COMMAND, command, 0);
    }
}

LRESULT CALLBACK TabSubclassProcedure(
    HWND tabs,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    app::WorkspaceWindow* workspace = WorkspaceFromTabs(tabs);
    app::ApplicationHost* state = workspace == nullptr
        ? nullptr
        : workspace->application;
    const app::EditorGroupId group_id{static_cast<std::uint64_t>(reference)};
    app::EditorGroup* group = workspace == nullptr
        ? nullptr
        : workspace->editors.Find(group_id);
    switch (message) {
        case WM_LBUTTONDOWN: {
            POINT client{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
            const int index = HitTab(tabs, client);
            const app::DocumentViewId view = TabViewAt(tabs, index);
            const app::CommandContext restore = state == nullptr
                ? app::CommandContext{}
                : state->routing.targets.Capture();
            const LRESULT result = DefSubclassProc(tabs, message, wparam, lparam);
            if (state == nullptr || workspace == nullptr || group == nullptr
                || !view || DragIsBlocked(*state, *workspace, view, tabs)
                || !state->ActivateWorkspaceWindow(workspace->id, true)
                || !state->ActivateDocumentView(view)) {
                return result;
            }
            const app::CommandContext context = state->routing.targets.Capture();
            if (context.document_view != view || context.editor_group != group_id) {
                return result;
            }
            POINT screen = client;
            ClientToScreen(tabs, &screen);
            const app::DragOperation operation = (GetKeyState(VK_CONTROL) < 0)
                ? app::DragOperation::TabCopy
                : app::DragOperation::TabMove;
            const auto token = state->routing.tokens.IssueDrag(context, operation);
            (void)state->TabDrag().Arm(
                token,
                restore,
                static_cast<std::size_t>(index),
                screen.x,
                screen.y);
            return result;
        }
        case WM_MOUSEMOVE:
            if (state != nullptr && state->TabDrag().IsArmed()) {
                if ((wparam & MK_LBUTTON) == 0U) {
                    CancelDocumentTabDrag(*state);
                    break;
                }
                POINT screen{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                ClientToScreen(tabs, &screen);
                const bool was_dragging = state->TabDrag().IsDragging();
                if (!state->TabDrag().TryBegin(
                        screen.x,
                        screen.y,
                        GetSystemMetrics(SM_CXDRAG),
                        GetSystemMetrics(SM_CYDRAG))) {
                    break;
                }
                if (!was_dragging) {
                    SetCapture(tabs);
                    const app::DragToken* token = state->TabDrag().Token();
                    const int source_index = token == nullptr
                            || !token->context.document_view.has_value()
                        ? -1
                        : static_cast<int>(group->ViewIndex(
                            token->context.document_view.value()).value_or(0U));
                    (void)BeginDragImage(tabs, source_index, screen);
                }
                (void)state->TabDrag().SetOperation(
                    GetKeyState(VK_CONTROL) < 0
                        ? app::DragOperation::TabCopy
                        : app::DragOperation::TabMove);
                const app::DragToken* token = state->TabDrag().Token();
                const auto target = token == nullptr
                    ? std::nullopt
                    : FindDropTarget(*state, *token, screen);
                (void)state->TabDrag().UpdateTarget(
                    target.value_or(app::TabDropTarget{}));
                ImageList_DragMove(screen.x, screen.y);
                SetCursor(LoadCursorW(
                    nullptr,
                    !target.has_value()
                        ? IDC_NO
                        : state->TabDrag().Token()->operation
                                == app::DragOperation::TabCopy
                            ? IDC_CROSS
                            : IDC_SIZEALL));
                return 0;
            }
            break;
        case WM_LBUTTONUP:
            if (state != nullptr && state->TabDrag().IsArmed()) {
                if (!state->TabDrag().IsDragging()) {
                    (void)state->TabDrag().Cancel();
                    return DefSubclassProc(tabs, message, wparam, lparam);
                }
                POINT screen{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                ClientToScreen(tabs, &screen);
                const app::DragToken* token = state->TabDrag().Token();
                const auto target = token == nullptr
                    ? std::nullopt
                    : FindDropTarget(*state, *token, screen);
                (void)state->TabDrag().UpdateTarget(
                    target.value_or(app::TabDropTarget{}));
                EndDragImage(*state);
                const auto request = state->TabDrag().TakeDrop();
                if (GetCapture() == tabs) {
                    ReleaseCapture();
                }
                if (request.has_value()) {
                    (void)CommitDrop(*state, request.value(), screen);
                }
                return 0;
            }
            break;
        case WM_KEYDOWN:
            if (state != nullptr && wparam == VK_ESCAPE
                && state->TabDrag().IsDragging()) {
                CancelDocumentTabDrag(*state);
                return 0;
            }
            break;
        case WM_CONTEXTMENU:
            if (state != nullptr && workspace != nullptr && group != nullptr) {
                ShowTabContextMenu(
                    *state,
                    *workspace,
                    tabs,
                    POINT{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
                return 0;
            }
            break;
        case WM_CANCELMODE:
        case WM_CAPTURECHANGED:
            if (state != nullptr && state->TabDrag().ReferencesGroup(group_id)) {
                CancelDocumentTabDrag(*state);
            }
            break;
        case WM_NCDESTROY:
            if (state != nullptr && state->TabDrag().ReferencesGroup(group_id)) {
                CancelDocumentTabDrag(*state, false);
            }
            if (auto image = reinterpret_cast<HIMAGELIST>(
                    RemovePropW(tabs, kDragImageProperty));
                image != nullptr) {
                ImageList_EndDrag();
                ImageList_Destroy(image);
            }
            RemoveWindowSubclass(
                tabs, TabSubclassProcedure, kTabDragSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(tabs, message, wparam, lparam);
}

}  // namespace

bool AttachDocumentTabDrag(HWND tabs, app::EditorGroupId group) noexcept {
    return tabs != nullptr && group
        && SetWindowSubclass(
            tabs,
            TabSubclassProcedure,
            kTabDragSubclass,
            static_cast<DWORD_PTR>(group.Value())) != FALSE;
}

void CancelDocumentTabDrag(
    app::ApplicationHost& state, bool restore_active_view) noexcept {
    const app::DragToken* token = state.TabDrag().Token();
    const app::WorkspaceWindow* source = token == nullptr
            || !token->context.workspace.has_value()
        ? nullptr
        : state.FindWorkspace(token->context.workspace.value());
    const app::EditorGroup* source_group = source == nullptr
            || !token->context.editor_group.has_value()
        ? nullptr
        : source->editors.Find(token->context.editor_group.value());
    const bool owns_capture = source_group != nullptr
        && GetCapture() == source_group->document_tabs;
    const bool dragging = state.TabDrag().IsDragging();
    const app::CommandContext restore = state.TabDrag().RestoreContext() == nullptr
        ? app::CommandContext{}
        : *state.TabDrag().RestoreContext();
    EndDragImage(state);
    (void)state.TabDrag().Cancel();
    if (owns_capture) {
        ReleaseCapture();
    }
    if (dragging && restore_active_view) {
        RestoreCapturedContext(state, restore);
    }
}

}  // namespace inkpod::windows::ui
