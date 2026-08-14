#pragma once

#include <windows.h>

#include <algorithm>
#include <climits>
#include <cstddef>
#include <string_view>

namespace inkpod::windows::ui {

constexpr int kLayerPalettePlaneBadgeWidthDip = 42;
constexpr int kLayerPalettePlaneBadgeHeightDip = 42;
constexpr int kLayerPalettePlaneBadgePaddingDip = 2;
constexpr UINT kLayerPalettePlaneBadgeTextFlags =
    DT_CENTER | DT_WORDBREAK | DT_NOPREFIX;

inline int ScaleLayerPaletteBadgeDip(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

inline std::size_t LayerPalettePlaneBadgeLineCount(
    std::wstring_view text) noexcept {
    return text.empty()
        ? 0U
        : 1U + static_cast<std::size_t>(
            std::count(text.begin(), text.end(), L'\n'));
}

inline SIZE MeasureLayerPalettePlaneBadgeText(
    HDC device, HFONT font, UINT dpi, std::wstring_view text) noexcept {
    SIZE measured{};
    if (device == nullptr || text.empty()
        || text.size() > static_cast<std::size_t>(INT_MAX)) {
        return measured;
    }
    const int padding = ScaleLayerPaletteBadgeDip(
        kLayerPalettePlaneBadgePaddingDip, dpi);
    RECT bounds{
        0,
        0,
        std::max(
            1,
            ScaleLayerPaletteBadgeDip(kLayerPalettePlaneBadgeWidthDip, dpi)
                - padding * 2),
        0};
    const HGDIOBJ previous = font == nullptr ? nullptr : SelectObject(device, font);
    const int height = DrawTextW(
        device,
        text.data(),
        static_cast<int>(text.size()),
        &bounds,
        kLayerPalettePlaneBadgeTextFlags | DT_CALCRECT);
    if (previous != nullptr) {
        SelectObject(device, previous);
    }
    if (height > 0) {
        measured.cx = std::max<LONG>(0L, bounds.right - bounds.left);
        measured.cy = std::max<LONG>(0L, bounds.bottom - bounds.top);
    }
    return measured;
}

inline bool LayerPalettePlaneBadgeTextFits(
    HDC device, HFONT font, UINT dpi, std::wstring_view text) noexcept {
    if (LayerPalettePlaneBadgeLineCount(text) == 0U
        || LayerPalettePlaneBadgeLineCount(text) > 2U) {
        return false;
    }
    const int padding = ScaleLayerPaletteBadgeDip(
        kLayerPalettePlaneBadgePaddingDip, dpi);
    const SIZE measured = MeasureLayerPalettePlaneBadgeText(
        device, font, dpi, text);
    return measured.cx > 0 && measured.cy > 0
        && measured.cx
            <= ScaleLayerPaletteBadgeDip(kLayerPalettePlaneBadgeWidthDip, dpi)
                - padding * 2
        && measured.cy
            <= ScaleLayerPaletteBadgeDip(kLayerPalettePlaneBadgeHeightDip, dpi)
                - padding * 2;
}

inline RECT LayoutLayerPalettePlaneBadgeText(
    HDC device,
    HFONT font,
    UINT dpi,
    std::wstring_view text,
    RECT frame) noexcept {
    const int padding = ScaleLayerPaletteBadgeDip(
        kLayerPalettePlaneBadgePaddingDip, dpi);
    InflateRect(&frame, -padding, -padding);
    const SIZE measured = MeasureLayerPalettePlaneBadgeText(
        device, font, dpi, text);
    const int available_height = std::max(
        0, static_cast<int>(frame.bottom - frame.top));
    const int text_height = std::min(
        available_height, static_cast<int>(measured.cy));
    frame.top += std::max(0, (available_height - text_height) / 2);
    frame.bottom = frame.top + text_height;
    return frame;
}

}  // namespace inkpod::windows::ui
