#pragma once

#include <windows.h>

#include <algorithm>

namespace inkpod::windows::ui {

constexpr int kLayerPaletteStatusButtonSizeDip = 32;
constexpr int kLayerPaletteStatusGapDip = 4;

inline int ScaleLayerPaletteStatusDip(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

struct LayerPaletteStatusCellLayout {
    RECT visibility{};
    RECT editability{};
    int text_right{};
};

inline LayerPaletteStatusCellLayout LayoutLayerPaletteStatusCells(
    RECT content_bounds, UINT dpi) noexcept {
    const int available_width = std::max(
        0, static_cast<int>(content_bounds.right - content_bounds.left));
    const int available_height = std::max(
        0, static_cast<int>(content_bounds.bottom - content_bounds.top));
    const int gap = std::min(
        available_width,
        ScaleLayerPaletteStatusDip(kLayerPaletteStatusGapDip, dpi));
    const int button_size = std::min({
        ScaleLayerPaletteStatusDip(kLayerPaletteStatusButtonSizeDip, dpi),
        available_height,
        std::max(0, (available_width - gap) / 2)});
    const int button_top = content_bounds.top
        + (available_height - button_size) / 2;
    LayerPaletteStatusCellLayout layout{};
    layout.editability = RECT{
        content_bounds.right - button_size,
        button_top,
        content_bounds.right,
        button_top + button_size};
    layout.visibility = RECT{
        layout.editability.left - gap - button_size,
        button_top,
        layout.editability.left - gap,
        button_top + button_size};
    layout.text_right = layout.visibility.left;
    return layout;
}

}  // namespace inkpod::windows::ui
