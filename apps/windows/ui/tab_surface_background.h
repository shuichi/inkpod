#pragma once

#include <windows.h>

namespace inkpod::windows::ui {

inline void PaintTabSurfaceBackground(
    HWND tabs,
    HWND overlay,
    HDC target,
    const RECT& client) noexcept {
    if (target == nullptr) {
        return;
    }
    FillRect(target, &client, GetSysColorBrush(COLOR_BTNFACE));
    if (tabs == nullptr || overlay == nullptr) {
        return;
    }
    POINT overlay_origin{};
    MapWindowPoints(overlay, tabs, &overlay_origin, 1U);
    const int saved = SaveDC(target);
    if (saved == 0) {
        return;
    }
    POINT viewport_origin{};
    GetViewportOrgEx(target, &viewport_origin);
    SetViewportOrgEx(
        target,
        viewport_origin.x - overlay_origin.x,
        viewport_origin.y - overlay_origin.y,
        nullptr);
    IntersectClipRect(
        target,
        overlay_origin.x + client.left,
        overlay_origin.y + client.top,
        overlay_origin.x + client.right,
        overlay_origin.y + client.bottom);
    SendMessageW(
        tabs,
        WM_PRINTCLIENT,
        reinterpret_cast<WPARAM>(target),
        PRF_CLIENT | PRF_ERASEBKGND);
    RestoreDC(target, saved);
}

}  // namespace inkpod::windows::ui
