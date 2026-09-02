#include "main_window_runtime.h"

#include "app/application_host.h"
#include "app/workspace_window.h"
#include "main_window_runtime_internal.h"

namespace inkpod::windows::ui::runtime {

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    if (message == WM_KILLFOCUS || message == WM_IME_SETCONTEXT
        || message == WM_IME_NOTIFY) {
        // Native focus/IME bookkeeping is synchronous and has no document
        // command here. Let Windows handle it without waiting for Core before
        // the user can reach a background job's cancel button.
        return DefWindowProcW(window, message, wparam, lparam);
    }
    auto* workspace = reinterpret_cast<app::WorkspaceWindow*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        workspace = static_cast<app::WorkspaceWindow*>(create->lpCreateParams);
        if (workspace == nullptr || workspace->application == nullptr) {
            return FALSE;
        }
        workspace->windows.window = window;
        SetWindowLongPtrW(
            window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(workspace));
    }

    app::ApplicationHost* application =
        workspace == nullptr ? nullptr : workspace->application;
    if (message == WM_TIMER) {
        if (const auto result = RouteCachedProgressTimerMessage(
                application, window, wparam)) {
            return *result;
        }
    }
    app::WorkspaceWindowId previous_workspace{};
    bool restore_workspace{};
    if (application != nullptr && workspace != nullptr) {
        const app::WorkspaceWindow* previous = application->Workspaces().Current();
        previous_workspace = previous == nullptr
            ? app::WorkspaceWindowId{}
            : previous->id;
        const bool starts_mouse_interaction = message == WM_LBUTTONDOWN
            || message == WM_RBUTTONDOWN || message == WM_MBUTTONDOWN
            || message == WM_XBUTTONDOWN || message == WM_LBUTTONDBLCLK
            || message == WM_RBUTTONDBLCLK || message == WM_MBUTTONDBLCLK
            || message == WM_XBUTTONDBLCLK || message == WM_MOUSEWHEEL
            || message == WM_MOUSEHWHEEL;
        const bool starts_pointer_interaction = message == WM_POINTERDOWN
            || message == WM_POINTERWHEEL || message == WM_POINTERHWHEEL;
        const bool records_focus = message == WM_COMMAND
            || message == WM_SETFOCUS
            || (message == WM_ACTIVATE && LOWORD(wparam) != WA_INACTIVE)
            || (message >= WM_KEYFIRST && message <= WM_KEYLAST)
            || starts_mouse_interaction || starts_pointer_interaction;
        if (!application->ActivateWorkspaceWindow(
                workspace->id, records_focus)) {
            return DefWindowProcW(window, message, wparam, lparam);
        }
        restore_workspace = !records_focus && previous_workspace
            && previous_workspace != workspace->id;
    }
    const auto finish = [application, previous_workspace, restore_workspace](
                            LRESULT result) noexcept {
        if (restore_workspace && application != nullptr
            && application->FindWorkspace(previous_workspace) != nullptr) {
            (void)application->ActivateWorkspaceWindow(
                previous_workspace, false);
        }
        return result;
    };
    if (message == WM_COMMAND) {
        if (const auto result = IssueCommand(
                application, window, wparam, lparam)) {
            return finish(*result);
        }
    } else if (const auto result = RouteMainWindowMessage(
                   application, window, message, wparam, lparam)) {
        return finish(*result);
    }
    return finish(DefWindowProcW(window, message, wparam, lparam));
}

}  // namespace inkpod::windows::ui::runtime
