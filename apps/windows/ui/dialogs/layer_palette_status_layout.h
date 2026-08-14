#pragma once

#include <windows.h>

#include <algorithm>
#include <climits>
#include <span>
#include <string_view>

namespace inkpod::windows::ui {

constexpr int kLayerPaletteStatusMinimumWidthDip = 42;
constexpr int kLayerPaletteStatusHorizontalPaddingDip = 16;
constexpr int kLayerPaletteStatusGapDip = 2;

inline int ScaleLayerPaletteStatusDip(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

inline int MeasureLayerPaletteStatusCellWidth(
    HDC device,
    HFONT font,
    UINT dpi,
    std::span<const std::wstring_view> labels) noexcept {
    const int minimum = ScaleLayerPaletteStatusDip(
        kLayerPaletteStatusMinimumWidthDip, dpi);
    if (device == nullptr) {
        return minimum;
    }

    const HGDIOBJ previous = font == nullptr ? nullptr : SelectObject(device, font);
    int widest{};
    for (const std::wstring_view label : labels) {
        if (label.size() > static_cast<std::size_t>(INT_MAX)) {
            continue;
        }
        SIZE extent{};
        if (GetTextExtentPoint32W(
                device,
                label.data(),
                static_cast<int>(label.size()),
                &extent) != FALSE) {
            widest = std::max(
                widest,
                static_cast<int>(std::max<LONG>(0L, extent.cx)));
        }
    }
    if (previous != nullptr) {
        SelectObject(device, previous);
    }

    const int padding = ScaleLayerPaletteStatusDip(
        kLayerPaletteStatusHorizontalPaddingDip, dpi);
    constexpr int maximum = INT_MAX / 4;
    const int measured = widest > maximum - padding
        ? maximum
        : widest + padding;
    return std::min(maximum, std::max(minimum, measured));
}

struct LayerPaletteStatusCellLayout {
    RECT visibility{};
    RECT editability{};
    int text_right{};
};

inline LayerPaletteStatusCellLayout LayoutLayerPaletteStatusCells(
    RECT content_bounds, int cell_width, UINT dpi) noexcept {
    const int width = std::max(0, cell_width);
    const int gap = ScaleLayerPaletteStatusDip(kLayerPaletteStatusGapDip, dpi);
    LayerPaletteStatusCellLayout layout{};
    layout.editability = RECT{
        content_bounds.right - width,
        content_bounds.top,
        content_bounds.right,
        content_bounds.bottom};
    layout.visibility = RECT{
        layout.editability.left - gap - width,
        content_bounds.top,
        layout.editability.left - gap,
        content_bounds.bottom};
    layout.text_right = layout.visibility.left;
    return layout;
}

}  // namespace inkpod::windows::ui
