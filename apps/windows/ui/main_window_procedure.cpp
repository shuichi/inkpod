#include "main_window_runtime.h"

#include "app/application_host.h"
#include "app/workspace_window.h"
#include "main_window_runtime_internal.h"

namespace inkpod::windows::ui::runtime {

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
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
    if (message == WM_COMMAND) {
        if (const auto result = IssueCommand(
                application, window, wparam, lparam)) {
            return *result;
        }
    } else if (const auto result = RouteMainWindowMessage(
                   application, window, message, wparam, lparam)) {
        return *result;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

}  // namespace inkpod::windows::ui::runtime
