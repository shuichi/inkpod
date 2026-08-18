#pragma once

#include <windows.h>

#include <cstdint>

#include "inkpod/core_ffi.h"
#include "ui/dialogs/basic_dialogs.h"
#include "ui/dialogs/effects_dialogs.h"

namespace inkpod::windows::ui::panes {

inline constexpr float kMinimumToolDiameter = 0.1F;
inline constexpr float kMaximumToolDiameter = 256.0F;
inline constexpr float kPencilToolDiameter = 1.0F;

using ToolOptionsCommandCallback = void (*)(void* context, UINT command) noexcept;
using ToolOptionsDiameterCallback = void (*)(void* context, float diameter) noexcept;
using ToolOptionsBrushCallback = void (*)(
    void* context, const InkpodEditorBrushOptions& options) noexcept;

enum class ToolOptionsDetailKind : std::uint8_t {
    None,
    Fill,
    View,
    Effect,
    BoundaryEffect,
};

struct ToolOptionsDetailModel {
    ToolOptionsDetailKind kind{ToolOptionsDetailKind::None};
    FillToolOptions fill{};
    ViewOptionsDialogState view{};
    EffectEditorState effect{};
};

using ToolOptionsDetailQueryCallback = bool (*)(
    void* context, UINT command, ToolOptionsDetailModel& output) noexcept;
using ToolOptionsDetailChangeCallback = bool (*)(
    void* context,
    UINT command,
    const ToolOptionsDetailModel& value,
    bool execute) noexcept;

struct ToolOptionsPaneState {
    void* context{};
    ToolOptionsCommandCallback dispatch_command{};
    ToolOptionsDiameterCallback change_diameter{};
    ToolOptionsBrushCallback change_brush{};
    ToolOptionsDetailQueryCallback query_detail{};
    ToolOptionsDetailChangeCallback change_detail{};
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
    UINT detail_command{};
    ToolOptionsDetailModel detail{};
    HFONT font{};
    HFONT edit_font{};
    int scroll_position{};
    int content_height{};
    bool updating{};
    bool editing{};
    bool editing_smoothing{};
    bool updating_detail{};
};

struct ToolOptionsFlyoutState {
    ToolOptionsPaneState* pane_state{};
    HWND window{};
    HWND pane{};
    HWND pin_button{};
    HWND close_button{};
    HWND tooltip{};
    HWND anchor{};
    UINT command{};
    bool pinned{};
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

HWND CreateToolOptionsFlyout(
    HINSTANCE instance,
    HWND owner,
    ToolOptionsFlyoutState& flyout,
    ToolOptionsPaneState& pane_state) noexcept;

bool ToggleToolOptionsFlyout(
    HWND flyout,
    HWND anchor,
    UINT command) noexcept;

bool ShowToolOptionsFlyout(
    HWND flyout,
    HWND anchor,
    UINT command) noexcept;

void HideToolOptionsFlyout(HWND flyout) noexcept;

bool IsToolOptionsFlyoutVisible(HWND flyout) noexcept;

void RefreshToolOptionsDetail(HWND pane, UINT command) noexcept;

}  // namespace inkpod::windows::ui::panes
