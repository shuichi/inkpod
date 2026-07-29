#pragma once

#include <windows.h>

#include <cstdint>

namespace inkpod::windows::ui {

struct WorkspaceLayoutState {
    bool tool_visible{true};
    bool tool_options_visible{true};
    bool color_visible{true};
    bool layer_visible{true};
    bool mirrored{};
    int tool_width_dip{80};
    int inspector_width_dip{320};
    int tool_options_height_dip{40};
    std::uint32_t color_split_milli{320U};
    std::uint32_t layer_split_milli{550U};

    // Transient drag/layout state. These values are never persisted.
    int drag_control{};
    POINT drag_start{};
    int drag_tool_width_dip{};
    int drag_inspector_width_dip{};
    std::uint32_t drag_color_split_milli{};
    int last_client_width{};
    int last_client_height{};
    int last_body_height{};
};

struct WorkspaceLayoutRects {
    RECT tool_options{};
    RECT tool{};
    RECT tool_splitter{};
    RECT document_tabs{};
    RECT canvas{};
    RECT inspector_splitter{};
    RECT color{};
    RECT color_splitter{};
    RECT layer{};
};

int ScaleWorkspaceDip(int value, UINT dpi) noexcept;

WorkspaceLayoutRects ComputeWorkspaceLayout(
    int client_width,
    int client_height,
    int status_height,
    UINT dpi,
    const WorkspaceLayoutState& state) noexcept;

void ResetWorkspaceLayout(WorkspaceLayoutState& state) noexcept;

bool LoadWorkspaceLayout(
    WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept;

bool SaveWorkspaceLayout(
    const WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept;

}  // namespace inkpod::windows::ui
