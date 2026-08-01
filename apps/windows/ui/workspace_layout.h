#pragma once

#include <windows.h>

#include <cstdint>

#include "dock_layout.h"

namespace inkpod::windows::ui {

struct WorkspaceLayoutState {
    DockLayoutModel dock{};
    // This is the internal layer/plane split inside the Layer pane, not a dock
    // geometry value.
    std::uint32_t layer_split_milli{550U};

    // Transient measurement only. These values are never persisted.
    int last_client_width{};
    int last_client_height{};
};

struct WorkspaceLayoutRects {
    DockLayoutGeometry dock{};
    RECT editor{};
    RECT document_tabs{};
    RECT canvas{};
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
