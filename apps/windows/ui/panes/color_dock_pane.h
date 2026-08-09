#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui::panes {

using ColorPaneCommandCallback = void (*)(void* context, UINT command) noexcept;
using ColorPaneValueCallback = void (*)(
    void* context, const InkpodColorValue& color) noexcept;
using ColorPaneSelectionCallback = void (*)(
    void* context, std::uint32_t index, bool chart) noexcept;
using ColorPaneGroupCallback = void (*)(void* context, int delta) noexcept;

class GdiPaintBuffer final {
public:
    GdiPaintBuffer() noexcept = default;
    ~GdiPaintBuffer() noexcept {
        Reset();
    }

    GdiPaintBuffer(const GdiPaintBuffer&) = delete;
    GdiPaintBuffer& operator=(const GdiPaintBuffer&) = delete;
    GdiPaintBuffer(GdiPaintBuffer&&) = delete;
    GdiPaintBuffer& operator=(GdiPaintBuffer&&) = delete;

    bool Prepare(HDC reference, int width, int height) noexcept;
    void Reset() noexcept {
        if (dc_ != nullptr && previous_bitmap_ != nullptr
            && previous_bitmap_ != HGDI_ERROR) {
            SelectObject(dc_, previous_bitmap_);
        }
        if (bitmap_ != nullptr) {
            DeleteObject(bitmap_);
        }
        if (dc_ != nullptr) {
            DeleteDC(dc_);
        }
        dc_ = nullptr;
        bitmap_ = nullptr;
        previous_bitmap_ = nullptr;
        bits_ = nullptr;
        width_ = 0;
        height_ = 0;
    }
    [[nodiscard]] bool ReadyFor(int width, int height) const noexcept;
    [[nodiscard]] HDC Dc() const noexcept;
    [[nodiscard]] void* Bits() const noexcept;
    [[nodiscard]] bool Present(HDC destination) const noexcept;

private:
    HDC dc_{};
    HBITMAP bitmap_{};
    HGDIOBJ previous_bitmap_{};
    void* bits_{};
    int width_{};
    int height_{};
};

struct ColorDockPaneState {
    void* context{};
    ColorPaneCommandCallback dispatch_command{};
    ColorPaneValueCallback change_color{};
    ColorPaneValueCallback change_main_line_color{};
    ColorPaneSelectionCallback select_color{};
    ColorPaneGroupCallback change_group{};
    InkpodColorValue main_line_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    InkpodColorValue drawing_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    std::vector<InkpodColorValue> colors;
    std::vector<std::wstring> names;
    std::uint32_t palette_group{};
    std::uint32_t chart_page{};
    bool chart_locked{};
    std::wstring target_text;
    bool target_available{};
    bool pinned{};
    int active_tab{};
    double main_line_hue_degrees{};
    double drawing_hue_degrees{};
    int picker_drag_target{};
    bool picker_targets_main_line{};
    InkpodColorValue main_line_drag_origin{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    double main_line_drag_origin_hue{};
    bool main_line_preview_active{};
    std::vector<std::uint32_t> picker_ring_pixels;
    std::vector<std::uint32_t> picker_triangle_pixels;
    std::vector<std::uint32_t> picker_frame_pixels;
    std::vector<std::uint32_t> picker_present_pixels;
    int picker_cache_width{};
    int picker_cache_height{};
    UINT picker_cache_dpi{};
    COLORREF picker_cache_face{};
    COLORREF picker_cache_window{};
    COLORREF picker_cache_light{};
    double picker_cache_hue_degrees{};
    std::uint32_t picker_cache_rgb{};
    bool picker_ring_cache_valid{};
    bool picker_triangle_cache_valid{};
    bool picker_frame_cache_valid{};
    GdiPaintBuffer picker_paint_buffer;
    GdiPaintBuffer color_label_paint_buffer;
    bool updating{};
    HFONT font{};
};

HWND CreateColorDockPane(
    HINSTANCE instance,
    HWND parent,
    ColorDockPaneState& state) noexcept;

void UpdateColorDockPane(
    HWND pane,
    const InkpodColorValue& main_line_color,
    const InkpodColorValue& drawing_color,
    const std::vector<InkpodColorValue>& colors,
    const std::vector<std::wstring>& names,
    std::uint32_t palette_group,
    std::uint32_t chart_page,
    bool chart_locked) noexcept;

void UpdateColorDockPaneDrawingColor(
    HWND pane,
    const InkpodColorValue& drawing_color) noexcept;

void UpdateColorDockPaneMainLineColor(
    HWND pane,
    const InkpodColorValue& main_line_color) noexcept;

void UpdateColorDockPaneTarget(
    HWND pane,
    std::wstring target_text,
    bool target_available,
    bool pinned) noexcept;

}  // namespace inkpod::windows::ui::panes
