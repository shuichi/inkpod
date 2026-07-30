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

struct ColorDockPaneState {
    void* context{};
    ColorPaneCommandCallback dispatch_command{};
    ColorPaneValueCallback change_color{};
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
    int active_tab{};
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

}  // namespace inkpod::windows::ui::panes
