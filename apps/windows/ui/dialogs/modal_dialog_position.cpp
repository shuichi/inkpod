#include "modal_dialog_position.h"

#include <algorithm>

namespace inkpod::windows::ui {

bool CenterModalDialogOnOwner(HWND dialog) noexcept {
    if (dialog == nullptr) {
        return false;
    }
    HWND owner = GetWindow(dialog, GW_OWNER);
    if (owner != nullptr) {
        const HWND root_owner = GetAncestor(owner, GA_ROOTOWNER);
        if (root_owner != nullptr) {
            owner = root_owner;
        }
    }
    const HWND monitor_source = owner == nullptr ? dialog : owner;
    const HMONITOR monitor = MonitorFromWindow(monitor_source, MONITOR_DEFAULTTONEAREST);
    MONITORINFO monitor_info{sizeof(monitor_info)};
    RECT dialog_bounds{};
    if (monitor == nullptr || GetMonitorInfoW(monitor, &monitor_info) == FALSE
        || GetWindowRect(dialog, &dialog_bounds) == FALSE) {
        return false;
    }
    RECT anchor = monitor_info.rcWork;
    if (owner != nullptr && IsIconic(owner) == FALSE) {
        RECT owner_bounds{};
        if (GetWindowRect(owner, &owner_bounds) != FALSE) {
            anchor = owner_bounds;
        }
    }
    const LONG width = dialog_bounds.right - dialog_bounds.left;
    const LONG height = dialog_bounds.bottom - dialog_bounds.top;
    const LONG work_width = monitor_info.rcWork.right - monitor_info.rcWork.left;
    const LONG work_height = monitor_info.rcWork.bottom - monitor_info.rcWork.top;
    const LONG maximum_x =
        std::max(monitor_info.rcWork.left, monitor_info.rcWork.right - width);
    const LONG maximum_y =
        std::max(monitor_info.rcWork.top, monitor_info.rcWork.bottom - height);
    const LONG centered_x = anchor.left + ((anchor.right - anchor.left) - width) / 2;
    const LONG centered_y = anchor.top + ((anchor.bottom - anchor.top) - height) / 2;
    const LONG x = width >= work_width
        ? monitor_info.rcWork.left
        : std::clamp(centered_x, monitor_info.rcWork.left, maximum_x);
    const LONG y = height >= work_height
        ? monitor_info.rcWork.top
        : std::clamp(centered_y, monitor_info.rcWork.top, maximum_y);
    if (SetWindowPos(
            dialog,
            nullptr,
            static_cast<int>(x),
            static_cast<int>(y),
            0,
            0,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE)
        == FALSE) {
        return false;
    }
    RECT centered_bounds{};
    return GetWindowRect(dialog, &centered_bounds) != FALSE && centered_bounds.left == x
        && centered_bounds.top == y;
}

}  // namespace inkpod::windows::ui
