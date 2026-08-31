#include "main_window_runtime.h"

#include <algorithm>
#include <array>
#include <cwchar>

#include "app/application_host.h"
#include "main_window_runtime_internal.h"

namespace inkpod::windows::ui::runtime {

namespace {

bool NativeMenuOwnsKey(const MSG& message) noexcept {
    if (message.message != WM_KEYDOWN && message.message != WM_SYSKEYDOWN) {
        return false;
    }
    const bool control = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
    const bool shift = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
    const bool alt = (GetKeyState(VK_MENU) & 0x8000) != 0;
    const bool windows = (GetKeyState(VK_LWIN) & 0x8000) != 0
        || (GetKeyState(VK_RWIN) & 0x8000) != 0;
    if (message.wParam == VK_MENU) {
        return true;
    }
    if (message.wParam == VK_F10 && !control && !shift && !alt && !windows) {
        return true;
    }
    if (!alt || control || windows) {
        return false;
    }
    if (message.wParam == VK_F4 && !shift) {
        return true;
    }
    if (message.wParam == VK_SPACE) {
        return true;
    }
    constexpr std::array<WPARAM, 11U> kTopLevelMnemonics{
        L'F', L'E', L'V', L'L', L'S', L'I', L'T', L'C', L'P', L'W', L'H'};
    return std::find(
               kTopLevelMnemonics.begin(),
               kTopLevelMnemonics.end(),
               message.wParam)
        != kTopLevelMnemonics.end();
}

}  // namespace

bool PreTranslateKeyboardMessage(
    app::ApplicationHost& state,
    const MSG& message) noexcept {
    const bool key_down = message.message == WM_KEYDOWN
        || message.message == WM_SYSKEYDOWN;
    const bool key_up = message.message == WM_KEYUP
        || message.message == WM_SYSKEYUP;
    if (!key_down && !key_up) {
        return false;
    }
    if (key_up && !state.shortcuts.hold_active) {
        return false;
    }
    if (NativeMenuOwnsKey(message)) {
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
            if (!workspace_navigation && !state.shortcuts.hold_active) {
                return false;
            }
        }
    }
    app::WorkspaceWindow* owner = state.WorkspaceForWindow(message.hwnd);
    const HWND status = owner == nullptr ? nullptr : owner->windows.status_bar;
    if (key_down && !control && (GetKeyState(VK_MENU) & 0x8000) == 0
        && status != nullptr && focus != nullptr
        && (focus == status || IsChild(status, focus) != FALSE)) {
        // Status controls use native tab order and button activation without
        // IsDialogMessage: DM_SETDEFID overlaps the status bar's SB_SETTEXTA.
        // Keep these UI-only keys ahead of workspace activation/Core queries.
        if (message.wParam == VK_TAB) {
            if (focus != status) {
                const HWND next = GetNextDlgTabItem(
                    status, focus, (GetKeyState(VK_SHIFT) & 0x8000) != 0);
                if (next != nullptr) {
                    // Update this subtree directly. WM_CHANGEUISTATE climbs
                    // to the main frame and would synchronize its Core target.
                    SendMessageW(status, WM_UPDATEUISTATE,
                        MAKEWPARAM(UIS_CLEAR, UISF_HIDEFOCUS), 0);
                    SetFocus(next);
                }
            }
            return true;
        }
        if (message.wParam == VK_RETURN) {
            if (focus != status && IsWindowEnabled(focus) != FALSE
                && (GetWindowLongPtrW(focus, GWL_STYLE) & WS_VISIBLE) != 0
                && (SendMessageW(focus, WM_GETDLGCODE, 0, 0) & DLGC_BUTTON) != 0) {
                SendMessageW(focus, BM_CLICK, 0, 0);
            }
            return true;
        }
        if (message.wParam == VK_SPACE) {
            // The ordinary TranslateMessage/DispatchMessage path delivers
            // Space down/up to BUTTON; do not resolve Canvas hold shortcuts.
            return false;
        }
    }
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
