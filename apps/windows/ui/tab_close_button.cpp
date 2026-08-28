#include "tab_close_button.h"

#include <algorithm>

namespace inkpod::windows::ui {

void PaintTabCloseButton(
    const DRAWITEMSTRUCT& draw, bool active, bool hovered) noexcept {
    const UINT window_dpi = GetDpiForWindow(draw.hwndItem);
    const UINT dpi = window_dpi == 0U ? 96U : window_dpi;
    const auto scale = [dpi](int value) noexcept {
        return MulDiv(value, static_cast<int>(dpi), 96);
    };
    const bool disabled = (draw.itemState & ODS_DISABLED) != 0U;
    const bool pressed = (draw.itemState & ODS_SELECTED) != 0U;
    const int background = pressed
        ? COLOR_3DSHADOW
        : (hovered ? COLOR_3DLIGHT : (active ? COLOR_WINDOW : COLOR_BTNFACE));
    const int foreground = disabled ? COLOR_GRAYTEXT : COLOR_BTNTEXT;
    FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(background));

    const int inset = std::max(4, scale(6));
    const int offset = pressed ? std::max(1, scale(1)) : 0;
    const HPEN pen = CreatePen(
        PS_SOLID, std::max(1, scale(1)), GetSysColor(foreground));
    if (pen != nullptr) {
        const HGDIOBJ previous = SelectObject(draw.hDC, pen);
        MoveToEx(
            draw.hDC,
            draw.rcItem.left + inset + offset,
            draw.rcItem.top + inset + offset,
            nullptr);
        LineTo(
            draw.hDC,
            draw.rcItem.right - inset + offset,
            draw.rcItem.bottom - inset + offset);
        MoveToEx(
            draw.hDC,
            draw.rcItem.right - inset + offset,
            draw.rcItem.top + inset + offset,
            nullptr);
        LineTo(
            draw.hDC,
            draw.rcItem.left + inset + offset,
            draw.rcItem.bottom - inset + offset);
        if (previous != nullptr) {
            SelectObject(draw.hDC, previous);
        }
        DeleteObject(pen);
    }
    if ((draw.itemState & ODS_FOCUS) != 0U) {
        RECT focus = draw.rcItem;
        InflateRect(&focus, -2, -2);
        DrawFocusRect(draw.hDC, &focus);
    }
}

}  // namespace inkpod::windows::ui
