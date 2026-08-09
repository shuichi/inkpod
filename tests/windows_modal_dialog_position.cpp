#include <windows.h>

#include <algorithm>

#include "ui/dialogs/modal_dialog_position.h"

namespace {

LRESULT CALLBACK TestWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    return DefWindowProcW(window, message, wparam, lparam);
}

bool HasExpectedPosition(HWND dialog, HWND owner, const RECT& work_area) noexcept {
    RECT dialog_bounds{};
    RECT owner_bounds{};
    if (GetWindowRect(dialog, &dialog_bounds) == FALSE
        || GetWindowRect(owner, &owner_bounds) == FALSE) {
        return false;
    }
    const LONG width = dialog_bounds.right - dialog_bounds.left;
    const LONG height = dialog_bounds.bottom - dialog_bounds.top;
    const LONG work_width = work_area.right - work_area.left;
    const LONG work_height = work_area.bottom - work_area.top;
    const LONG maximum_x = std::max(work_area.left, work_area.right - width);
    const LONG maximum_y = std::max(work_area.top, work_area.bottom - height);
    const LONG centered_x =
        owner_bounds.left + ((owner_bounds.right - owner_bounds.left) - width) / 2;
    const LONG centered_y =
        owner_bounds.top + ((owner_bounds.bottom - owner_bounds.top) - height) / 2;
    const LONG expected_x = width >= work_width
        ? work_area.left
        : std::clamp(centered_x, work_area.left, maximum_x);
    const LONG expected_y = height >= work_height
        ? work_area.top
        : std::clamp(centered_y, work_area.top, maximum_y);
    return dialog_bounds.left == expected_x && dialog_bounds.top == expected_y;
}

}  // namespace

int wmain() {
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    constexpr wchar_t kWindowClass[] = L"InkpodModalDialogPositionTest";
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.lpfnWndProc = TestWindowProcedure;
    window_class.hInstance = instance;
    window_class.lpszClassName = kWindowClass;
    if (RegisterClassExW(&window_class) == 0) {
        return 1;
    }

    const HWND owner = CreateWindowExW(
        0,
        kWindowClass,
        L"owner",
        WS_OVERLAPPEDWINDOW,
        0,
        0,
        100,
        100,
        nullptr,
        nullptr,
        instance,
        nullptr);
    if (owner == nullptr) {
        UnregisterClassW(kWindowClass, instance);
        return 2;
    }

    MONITORINFO monitor_info{sizeof(monitor_info)};
    const HMONITOR monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
    if (monitor == nullptr || GetMonitorInfoW(monitor, &monitor_info) == FALSE) {
        DestroyWindow(owner);
        UnregisterClassW(kWindowClass, instance);
        return 3;
    }
    const LONG work_width = monitor_info.rcWork.right - monitor_info.rcWork.left;
    const LONG work_height = monitor_info.rcWork.bottom - monitor_info.rcWork.top;
    const int owner_width = static_cast<int>(std::max<LONG>(1, std::min<LONG>(800, work_width)));
    const int owner_height = static_cast<int>(std::max<LONG>(1, std::min<LONG>(600, work_height)));
    const int owner_x = monitor_info.rcWork.left + (work_width - owner_width) / 2;
    const int owner_y = monitor_info.rcWork.top + (work_height - owner_height) / 2;
    if (SetWindowPos(
            owner,
            nullptr,
            owner_x,
            owner_y,
            owner_width,
            owner_height,
            SWP_NOACTIVATE | SWP_NOZORDER)
        == FALSE) {
        DestroyWindow(owner);
        UnregisterClassW(kWindowClass, instance);
        return 4;
    }

    const HWND palette = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        kWindowClass,
        L"owned palette",
        WS_POPUP | WS_CAPTION,
        owner_x,
        owner_y,
        200,
        120,
        owner,
        nullptr,
        instance,
        nullptr);
    const HWND dialog = palette == nullptr
        ? nullptr
        : CreateWindowExW(
              WS_EX_DLGMODALFRAME,
              kWindowClass,
              L"modal dialog",
              WS_POPUP | WS_CAPTION,
              0,
              0,
              240,
              160,
              palette,
              nullptr,
              instance,
              nullptr);
    if (palette == nullptr || dialog == nullptr) {
        if (dialog != nullptr) {
            DestroyWindow(dialog);
        }
        if (palette != nullptr) {
            DestroyWindow(palette);
        }
        DestroyWindow(owner);
        UnregisterClassW(kWindowClass, instance);
        return 5;
    }

    int result{};
    if (!inkpod::windows::ui::CenterModalDialogOnOwner(dialog)
        || !HasExpectedPosition(dialog, owner, monitor_info.rcWork)) {
        result = 6;
    }
    if (result == 0) {
        const int moved_x = monitor_info.rcWork.right - owner_width / 4;
        const int moved_y = monitor_info.rcWork.bottom - owner_height / 4;
        if (SetWindowPos(
                owner,
                nullptr,
                moved_x,
                moved_y,
                owner_width,
                owner_height,
                SWP_NOACTIVATE | SWP_NOZORDER)
                == FALSE
            || !inkpod::windows::ui::CenterModalDialogOnOwner(dialog)
            || !HasExpectedPosition(dialog, owner, monitor_info.rcWork)) {
            result = 7;
        }
    }

    DestroyWindow(dialog);
    DestroyWindow(palette);
    DestroyWindow(owner);
    UnregisterClassW(kWindowClass, instance);
    return result;
}
