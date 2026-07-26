#pragma once

#include <windows.h>

namespace inkpod::app {

struct MainWindowHandles {
    HWND window{};
    HWND canvas{};
    HWND toolbar{};
    HWND zoom_slider{};
    HWND status_bar{};
    HWND document_tabs{};
    HWND locator_label{};
    HWND layer_list{};
    HWND plane_list{};
    HWND light_table_set_list{};
    HWND light_table_item_list{};
    HWND sequence_list{};
    HWND motion_label{};
    HWND color_palette_list{};
    HWND color_chart_list{};
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
    const app::MainWindowHandles& windows,
    bool smoke_test,
    int width,
    int height) noexcept;

bool RegisterMainWindowClass(
    HINSTANCE instance,
    const wchar_t* class_name,
    WNDPROC procedure) noexcept;

} // namespace inkpod::windows::ui
