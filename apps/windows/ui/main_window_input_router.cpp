#include "main_window_runtime.h"

#include <array>
#include <cwchar>

#include "app/application_host.h"
#include "main_window_runtime_internal.h"

namespace inkpod::windows::ui::runtime {

bool PreTranslateKeyboardMessage(
    app::ApplicationHost& state,
    const MSG& message) noexcept {
    if (message.message != WM_KEYDOWN && message.message != WM_SYSKEYDOWN) {
        return false;
    }
    const bool control = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
    const bool workspace_navigation = message.wParam == VK_F6
        || (control && (message.wParam == VK_TAB || message.wParam == VK_F4));
    const HWND focus = GetFocus();
    if (focus != nullptr) {
        wchar_t class_name[64]{};
        if (GetClassNameW(
                focus, class_name, static_cast<int>(std::size(class_name))) > 0
            && (_wcsicmp(class_name, L"Edit") == 0
                || _wcsnicmp(class_name, L"RichEdit", 8) == 0)) {
            if (!workspace_navigation) {
                return false;
            }
        }
    }
    app::WorkspaceWindow* owner = state.WorkspaceForWindow(message.hwnd);
    if (owner == nullptr
        || !state.ActivateWorkspaceWindow(owner->id, true)) {
        return false;
    }
    const HWND workspace = owner->windows.window;
    return RouteKeyboardMessage(
               &state,
               workspace,
               message.message,
               message.wParam,
               message.lParam)
        .has_value();
}

}  // namespace inkpod::windows::ui::runtime
