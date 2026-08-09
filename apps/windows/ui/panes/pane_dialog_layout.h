#pragma once

#include <windows.h>

#include <algorithm>

namespace inkpod::windows::ui::panes {

inline int ScalePaneDip(HWND dialog, int value) noexcept {
    const UINT dpi = dialog == nullptr ? 96U : GetDpiForWindow(dialog);
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

inline void PlacePaneDialogControl(
    HWND dialog,
    int control,
    int x,
    int y,
    int width,
    int height) noexcept {
    const HWND child = GetDlgItem(dialog, control);
    if (child == nullptr) {
        return;
    }
    SetWindowPos(
        child,
        nullptr,
        x,
        y,
        std::max(0, width),
        std::max(0, height),
        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
}

}  // namespace inkpod::windows::ui::panes
