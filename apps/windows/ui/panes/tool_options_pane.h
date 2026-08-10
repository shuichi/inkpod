#pragma once

#include <windows.h>

#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui::panes {

inline constexpr float kMinimumToolDiameter = 0.1F;
inline constexpr float kMaximumToolDiameter = 256.0F;
inline constexpr float kPencilToolDiameter = 1.0F;

using ToolOptionsCommandCallback = void (*)(void* context, UINT command) noexcept;
using ToolOptionsDiameterCallback = void (*)(void* context, float diameter) noexcept;
using ToolOptionsBrushCallback = void (*)(
    void* context, const InkpodEditorBrushOptions& options) noexcept;

struct ToolOptionsPaneState {
    void* context{};
    ToolOptionsCommandCallback dispatch_command{};
    ToolOptionsDiameterCallback change_diameter{};
    ToolOptionsBrushCallback change_brush{};
    std::uint32_t active_tool{};
    InkpodPlaneKind active_plane{INKPOD_PLANE_MAIN_LINE};
    float diameter{8.0F};
    InkpodEditorBrushOptions brush{
        sizeof(InkpodEditorBrushOptions),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    HFONT font{};
    HFONT edit_font{};
    bool updating{};
    bool editing{};
    bool editing_smoothing{};
};

HWND CreateToolOptionsPane(
    HINSTANCE instance,
    HWND parent,
    ToolOptionsPaneState& state) noexcept;

void UpdateToolOptionsPane(
    HWND pane,
    std::uint32_t active_tool,
    InkpodPlaneKind active_plane,
    float diameter,
    const InkpodEditorBrushOptions& brush) noexcept;

}  // namespace inkpod::windows::ui::panes
