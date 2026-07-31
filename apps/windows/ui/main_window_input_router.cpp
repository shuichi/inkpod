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
    const HWND focus = GetFocus();
    if (focus != nullptr) {
        wchar_t class_name[64]{};
        if (GetClassNameW(
                focus, class_name, static_cast<int>(std::size(class_name))) > 0
            && (_wcsicmp(class_name, L"Edit") == 0
                || _wcsnicmp(class_name, L"RichEdit", 8) == 0)) {
            return false;
        }
    }
    const HWND workspace = state.Workspace().windows.window;
    const HWND target_root = message.hwnd == nullptr
        ? nullptr
        : GetAncestor(message.hwnd, GA_ROOTOWNER);
    if (message.hwnd != workspace
        && !IsChild(workspace, message.hwnd)
        && target_root != workspace) {
        return false;
    }
    return RouteKeyboardMessage(
               &state,
               workspace,
               message.message,
               message.wParam,
               message.lParam)
        .has_value();
}

}  // namespace inkpod::windows::ui::runtime
