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

struct ToolOptionsPaneState {
    void* context{};
    ToolOptionsCommandCallback dispatch_command{};
    ToolOptionsDiameterCallback change_diameter{};
    std::uint32_t active_tool{};
    InkpodPlaneKind active_plane{INKPOD_PLANE_MAIN_LINE};
    float diameter{8.0F};
    HFONT font{};
    bool updating{};
    bool editing{};
};

HWND CreateToolOptionsPane(
    HINSTANCE instance,
    HWND parent,
    ToolOptionsPaneState& state) noexcept;

void UpdateToolOptionsPane(
    HWND pane,
    std::uint32_t active_tool,
    InkpodPlaneKind active_plane,
    float diameter) noexcept;

}  // namespace inkpod::windows::ui::panes
