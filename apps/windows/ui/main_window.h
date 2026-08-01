#pragma once

#include <windows.h>

#include "app/editor_area.h"
#include "ui/dock_host.h"
#include "ui/workspace_layout.h"

namespace inkpod::app {

struct MainWindowHandles {
    HWND window{};
    // Non-owning aliases to the active EditorGroup. Group HWND ownership and
    // view placement live in EditorArea; aliases are refreshed transactionally.
    HWND canvas{};
    HWND status_bar{};
    HWND document_tabs{};
    HWND tool_options{};
    HWND tool_palette{};
    HWND color_pane{};
    HWND layer_palette{};
    EditorArea* editors{};
    windows::ui::WorkspaceLayoutState workspace{};
    windows::ui::DockHost dock_host{};
};

}

namespace inkpod::windows::ui {

// Owns creation and geometry of the main frame's standard child controls.
// Feature palettes and their callbacks remain with their feature owners.
bool CreateMainChrome(
    app::MainWindowHandles& windows,
    app::EditorArea& editors,
    HINSTANCE instance,
    bool smoke_test) noexcept;

bool CreateEditorGroupTabs(
    app::MainWindowHandles& windows,
    app::EditorGroup& group,
    HINSTANCE instance,
    bool smoke_test) noexcept;

void SyncActiveEditorHandles(app::MainWindowHandles& windows) noexcept;

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
