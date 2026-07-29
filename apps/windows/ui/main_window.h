#pragma once

#include <windows.h>

#include "ui/workspace_layout.h"

namespace inkpod::app {

struct MainWindowHandles {
    HWND window{};
    HWND canvas{};
    HWND status_bar{};
    HWND document_tabs{};
    HWND tool_options{};
    HWND tool_palette{};
    HWND color_pane{};
    HWND layer_palette{};
    HWND tool_splitter{};
    HWND inspector_splitter{};
    HWND color_splitter{};
    windows::ui::WorkspaceLayoutState workspace{};
};

}

namespace inkpod::windows::ui {

// Owns creation and geometry of the main frame's standard child controls.
// Feature palettes and their callbacks remain with their feature owners.
bool CreateMainChrome(
    app::MainWindowHandles& windows,
    HINSTANCE instance,
    bool smoke_test) noexcept;

void LayoutMainChrome(
    app::MainWindowHandles& windows,
    bool smoke_test,
    int width,
    int height) noexcept;

bool RegisterMainWindowClass(
    HINSTANCE instance,
    const wchar_t* class_name,
    WNDPROC procedure) noexcept;

} // namespace inkpod::windows::ui
