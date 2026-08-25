#pragma once

#include <windows.h>

#include <commctrl.h>

#include <algorithm>
#include <span>
#include <string>
#include <string_view>

namespace inkpod::windows::ui::panes {

inline int ScalePaneDip(HWND dialog, int value) noexcept {
    const UINT dpi = dialog == nullptr ? 96U : GetDpiForWindow(dialog);
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

inline bool PaneWindowHasBounds(
    HWND child, int x, int y, int width, int height) noexcept {
    RECT bounds{};
    if (child == nullptr || GetWindowRect(child, &bounds) == FALSE) {
        return false;
    }
    const HWND parent = GetParent(child);
    POINT top_left{bounds.left, bounds.top};
    POINT bottom_right{bounds.right, bounds.bottom};
    if (parent != nullptr
        && (ScreenToClient(parent, &top_left) == FALSE
            || ScreenToClient(parent, &bottom_right) == FALSE)) {
        return false;
    }
    return top_left.x == x && top_left.y == y
        && bottom_right.x - top_left.x == std::max(0, width)
        && bottom_right.y - top_left.y == std::max(0, height);
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
    if (PaneWindowHasBounds(child, x, y, width, height)) {
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

inline int MeasurePaneButtonTextWidth(
    HWND button, std::wstring_view text, UINT dpi) noexcept {
    if (button == nullptr) {
        return 0;
    }
    HDC device = GetDC(button);
    if (device == nullptr) {
        return 0;
    }
    const HFONT font = reinterpret_cast<HFONT>(
        SendMessageW(button, WM_GETFONT, 0, 0));
    const HGDIOBJ previous = font == nullptr ? nullptr : SelectObject(device, font);
    SIZE extent{};
    const bool measured = text.size() <= static_cast<std::size_t>(INT_MAX)
        && GetTextExtentPoint32W(
               device,
               text.data(),
               static_cast<int>(text.size()),
               &extent) != FALSE;
    if (previous != nullptr) {
        SelectObject(device, previous);
    }
    ReleaseDC(button, device);
    const int padding = MulDiv(20, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
    return measured ? extent.cx + padding : 0;
}

inline int PaneButtonIdealWidthAtDpi(
    HWND dialog, int control, UINT requested_dpi) noexcept {
    const HWND button = GetDlgItem(dialog, control);
    if (button == nullptr) {
        return 0;
    }
    const UINT window_dpi = GetDpiForWindow(button);
    const UINT dpi = requested_dpi != 0U
        ? requested_dpi
        : (window_dpi == 0U ? 96U : window_dpi);
    const DWORD style = static_cast<DWORD>(
        GetWindowLongPtrW(button, GWL_STYLE));
    if ((style & BS_ICON) != 0U) {
        return MulDiv(32, static_cast<int>(dpi), 96);
    }
    int text_length = GetWindowTextLengthW(button);
    if (text_length < 0) {
        text_length = 0;
    }
    std::wstring text;
    try {
        text.resize(static_cast<std::size_t>(text_length) + 1U, L'\0');
    } catch (const std::bad_alloc&) {
        return ScalePaneDip(dialog, 32);
    }
    const int copied = GetWindowTextW(button, text.data(), text_length + 1);
    text.resize(static_cast<std::size_t>(std::max(0, copied)));
    SIZE ideal{};
    const int common_controls_width = SendMessageW(
        button, BCM_GETIDEALSIZE, 0, reinterpret_cast<LPARAM>(&ideal)) != FALSE
        ? ideal.cx
        : 0;
    return std::max({
        ScalePaneDip(dialog, 32),
        common_controls_width,
        MeasurePaneButtonTextWidth(button, text, dpi)});
}

inline int PaneButtonIdealWidth(HWND dialog, int control) noexcept {
    return PaneButtonIdealWidthAtDpi(dialog, control, 0U);
}

inline bool PaneButtonTextFits(HWND dialog, int control) noexcept {
    const HWND button = GetDlgItem(dialog, control);
    RECT bounds{};
    return button != nullptr && GetClientRect(button, &bounds) != FALSE
        && bounds.right - bounds.left >= PaneButtonIdealWidth(dialog, control);
}

inline std::size_t PaneButtonRowCount(
    HWND dialog,
    std::span<const int> controls,
    int available_width,
    int gap,
    UINT dpi = 0U) noexcept {
    if (controls.empty() || available_width <= 0) {
        return controls.empty() ? 0U : controls.size();
    }
    std::size_t rows = 1U;
    int used{};
    for (const int control : controls) {
        const int ideal = std::min(
            available_width, PaneButtonIdealWidthAtDpi(dialog, control, dpi));
        if (used != 0 && used + gap + ideal > available_width) {
            ++rows;
            used = 0;
        }
        used += (used == 0 ? 0 : gap) + ideal;
    }
    return rows;
}

inline std::size_t PlacePaneButtonRows(
    HWND dialog,
    std::span<const int> controls,
    int x,
    int y,
    int available_width,
    int row_height,
    int gap,
    UINT dpi = 0U) noexcept {
    std::size_t first{};
    std::size_t row{};
    while (first < controls.size()) {
        std::size_t last = first;
        int ideal_total{};
        while (last < controls.size()) {
            const int ideal = std::min(
                available_width,
                PaneButtonIdealWidthAtDpi(dialog, controls[last], dpi));
            const int candidate = ideal_total
                + (last == first ? 0 : gap) + ideal;
            if (last != first && candidate > available_width) {
                break;
            }
            ideal_total = candidate;
            ++last;
        }
        const int count = static_cast<int>(last - first);
        const int extra = std::max(0, available_width - ideal_total);
        int cursor = x;
        int distributed{};
        for (std::size_t index = first; index < last; ++index) {
            const int share = count == 0 ? 0 : extra / count
                + (static_cast<int>(index - first) < extra % count ? 1 : 0);
            distributed += share;
            int width = std::min(
                available_width,
                PaneButtonIdealWidthAtDpi(dialog, controls[index], dpi)) + share;
            if (index + 1U == last) {
                width += extra - distributed;
            }
            PlacePaneDialogControl(
                dialog,
                controls[index],
                cursor,
                y + static_cast<int>(row) * (row_height + gap),
                width,
                row_height);
            cursor += width + gap;
        }
        first = last;
        ++row;
    }
    return row;
}

inline void PlacePaneTargetRow(
    HWND dialog,
    int target_control,
    int button_control,
    int margin,
    int y,
    int available_width,
    int target_y_offset,
    int target_height,
    int button_height,
    int gap) noexcept {
    const int button_width = std::min(
        std::max(0, available_width),
        PaneButtonIdealWidth(dialog, button_control));
    PlacePaneDialogControl(
        dialog,
        button_control,
        margin + std::max(0, available_width - button_width),
        y,
        button_width,
        button_height);
    PlacePaneDialogControl(
        dialog,
        target_control,
        margin,
        y + target_y_offset,
        std::max(0, available_width - button_width - gap),
        target_height);
}

}  // namespace inkpod::windows::ui::panes
