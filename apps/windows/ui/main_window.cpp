#include "main_window.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>

#include "app/resource.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT_PTR kSplitterSubclass = 1U;

bool RectHasArea(const RECT& bounds) noexcept {
    return bounds.right > bounds.left && bounds.bottom > bounds.top;
}

void PlaceChild(HWND child, const RECT& bounds, bool requested_visible) noexcept {
    if (child == nullptr) {
        return;
    }
    const bool visible = requested_visible && RectHasArea(bounds);
    SetWindowPos(
        child,
        nullptr,
        bounds.left,
        bounds.top,
        std::max(0L, bounds.right - bounds.left),
        std::max(0L, bounds.bottom - bounds.top),
        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER
            | (visible ? SWP_SHOWWINDOW : SWP_HIDEWINDOW));
}

int PixelsToDip(int value, UINT dpi) noexcept {
    return MulDiv(value, 96, static_cast<int>(dpi == 0U ? 96U : dpi));
}

void RelayoutFromSplitter(app::MainWindowHandles& windows) noexcept {
    RECT client{};
    if (windows.window != nullptr
        && GetClientRect(windows.window, &client) != FALSE) {
        LayoutMainChrome(
            windows,
            false,
            client.right - client.left,
            client.bottom - client.top);
    }
}

int EditorExtent(const app::MainWindowHandles& windows) noexcept {
    if (windows.editors == nullptr) {
        return 0;
    }
    const UINT dpi = windows.window == nullptr ? 96U : GetDpiForWindow(windows.window);
    const WorkspaceLayoutRects layout = ComputeWorkspaceLayout(
        windows.workspace.last_client_width,
        windows.workspace.last_client_height,
        0,
        dpi,
        windows.workspace);
    return windows.editors->Orientation() == app::EditorSplitOrientation::Horizontal
        ? layout.canvas.bottom - layout.document_tabs.top
        : layout.document_tabs.right - layout.document_tabs.left;
}

LRESULT CALLBACK SplitterSubclassProcedure(
    HWND splitter,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* windows = reinterpret_cast<app::MainWindowHandles*>(reference);
    if (windows == nullptr) {
        return DefSubclassProc(splitter, message, wparam, lparam);
    }
    const int control = GetDlgCtrlID(splitter);
    WorkspaceLayoutState& layout = windows->workspace;
    switch (message) {
        case WM_LBUTTONDOWN:
            layout.drag_control = control;
            GetCursorPos(&layout.drag_start);
            layout.drag_tool_width_dip = layout.tool_width_dip;
            layout.drag_inspector_width_dip = layout.inspector_width_dip;
            layout.drag_color_split_milli = layout.color_split_milli;
            if (control == IDC_EDITOR_GROUP_SPLITTER && windows->editors != nullptr) {
                windows->editors->drag_start = layout.drag_start;
                windows->editors->drag_ratio_milli =
                    windows->editors->SplitRatioMilli();
                windows->editors->last_drag_layout_tick = 0U;
            }
            SetCapture(splitter);
            SetFocus(splitter);
            return 0;
        case WM_MOUSEMOVE:
            if (GetCapture() == splitter && layout.drag_control == control) {
                POINT current{};
                GetCursorPos(&current);
                const UINT dpi = windows->window == nullptr
                    ? 96U
                    : GetDpiForWindow(windows->window);
                const int delta_x_dip = PixelsToDip(current.x - layout.drag_start.x, dpi);
                if (control == IDC_EDITOR_GROUP_SPLITTER
                    && windows->editors != nullptr) {
                    const int extent = EditorExtent(*windows);
                    const int delta = windows->editors->Orientation()
                            == app::EditorSplitOrientation::Horizontal
                        ? current.y - windows->editors->drag_start.y
                        : current.x - windows->editors->drag_start.x;
                    if (extent > 0) {
                        windows->editors->SetSplitRatioMilli(
                            static_cast<std::uint32_t>(std::clamp(
                                static_cast<int>(windows->editors->drag_ratio_milli)
                                    + delta * 1000 / extent,
                                200,
                                800)));
                    }
                } else if (control == IDC_WORKSPACE_TOOL_SPLITTER) {
                    layout.tool_width_dip = std::clamp(
                        layout.drag_tool_width_dip
                            + (layout.mirrored ? -delta_x_dip : delta_x_dip),
                80,
                        160);
                } else if (control == IDC_WORKSPACE_INSPECTOR_SPLITTER) {
                    layout.inspector_width_dip = std::clamp(
                        layout.drag_inspector_width_dip
                            + (layout.mirrored ? delta_x_dip : -delta_x_dip),
                        240,
                        640);
                } else if (control == IDC_WORKSPACE_COLOR_SPLITTER
                           && layout.last_body_height > 0) {
                    const int delta_y = current.y - layout.drag_start.y;
                    const int delta_milli = static_cast<int>(
                        static_cast<std::int64_t>(delta_y) * 1000
                        / layout.last_body_height);
                    layout.color_split_milli = static_cast<std::uint32_t>(
                        std::clamp(
                            static_cast<int>(layout.drag_color_split_milli)
                                + delta_milli,
                            150,
                            700));
                }
                const std::uint64_t now = GetTickCount64();
                if (control != IDC_EDITOR_GROUP_SPLITTER
                    || windows->editors == nullptr
                    || now - windows->editors->last_drag_layout_tick >= 16U) {
                    if (windows->editors != nullptr) {
                        windows->editors->last_drag_layout_tick = now;
                    }
                    RelayoutFromSplitter(*windows);
                }
                return 0;
            }
            break;
        case WM_LBUTTONUP:
            if (GetCapture() == splitter) {
                ReleaseCapture();
            }
            layout.drag_control = 0;
            RelayoutFromSplitter(*windows);
            if (windows->editors != nullptr) {
                for (std::size_t index = 0U;
                     index < windows->editors->GroupCount();
                     ++index) {
                    const auto* group = windows->editors->GroupAt(index);
                    if (group != nullptr && group->canvas != nullptr) {
                        InvalidateRect(group->canvas, nullptr, FALSE);
                    }
                }
            }
            return 0;
        case WM_CAPTURECHANGED:
            layout.drag_control = 0;
            return 0;
        case WM_KEYDOWN: {
            const int direction = wparam == VK_LEFT || wparam == VK_UP
                ? -1
                : (wparam == VK_RIGHT || wparam == VK_DOWN ? 1 : 0);
            if (direction == 0) {
                break;
            }
            if (control == IDC_EDITOR_GROUP_SPLITTER
                && windows->editors != nullptr) {
                windows->editors->SetSplitRatioMilli(
                    static_cast<std::uint32_t>(std::clamp(
                        static_cast<int>(windows->editors->SplitRatioMilli())
                            + direction * 20,
                        200,
                        800)));
            } else if (control == IDC_WORKSPACE_TOOL_SPLITTER) {
                layout.tool_width_dip = std::clamp(
                    layout.tool_width_dip
                        + direction * (layout.mirrored ? -4 : 4),
                80,
                    160);
            } else if (control == IDC_WORKSPACE_INSPECTOR_SPLITTER) {
                layout.inspector_width_dip = std::clamp(
                    layout.inspector_width_dip
                        + direction * (layout.mirrored ? 8 : -8),
                    240,
                    640);
            } else {
                layout.color_split_milli = static_cast<std::uint32_t>(
                    std::clamp(
                        static_cast<int>(layout.color_split_milli) + direction * 20,
                        150,
                        700));
            }
            RelayoutFromSplitter(*windows);
            return 0;
        }
        case WM_SETCURSOR:
            SetCursor(LoadCursorW(
                nullptr,
                control == IDC_WORKSPACE_COLOR_SPLITTER
                        || (control == IDC_EDITOR_GROUP_SPLITTER
                            && windows->editors != nullptr
                            && windows->editors->Orientation()
                                == app::EditorSplitOrientation::Horizontal)
                    ? IDC_SIZENS
                    : IDC_SIZEWE));
            return TRUE;
        case WM_NCDESTROY:
            RemoveWindowSubclass(splitter, SplitterSubclassProcedure, kSplitterSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(splitter, message, wparam, lparam);
}

HWND CreateSplitter(
    app::MainWindowHandles& windows,
    HINSTANCE instance,
    int control) noexcept {
    const HWND splitter = CreateWindowExW(
        0,
        L"STATIC",
        nullptr,
        WS_CHILD | WS_TABSTOP | SS_NOTIFY,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(control)),
        instance,
        nullptr);
    if (splitter != nullptr) {
        SetWindowSubclass(
            splitter,
            SplitterSubclassProcedure,
            kSplitterSubclass,
            reinterpret_cast<DWORD_PTR>(&windows));
    }
    return splitter;
}

}  // namespace

bool CreateMainChrome(
    app::MainWindowHandles& windows,
    app::EditorArea& editors,
    HINSTANCE instance,
    bool smoke_test) noexcept {
    const DWORD visible = smoke_test ? 0U : WS_VISIBLE;
    windows.editors = &editors;
    windows.status_bar = CreateWindowExW(
        0,
        STATUSCLASSNAMEW,
        nullptr,
        WS_CHILD | visible | SBARS_SIZEGRIP,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_STATUS)),
        instance,
        nullptr);
    app::EditorGroup* primary_group = editors.GroupAt(0U);
    if (primary_group == nullptr
        || !CreateEditorGroupTabs(windows, *primary_group, instance, smoke_test)) {
        return false;
    }
    windows.document_tabs = primary_group->document_tabs;
    windows.canvas = primary_group->canvas;
    if (windows.status_bar == nullptr || windows.document_tabs == nullptr) {
        return false;
    }
    windows.tool_splitter = CreateSplitter(
        windows, instance, IDC_WORKSPACE_TOOL_SPLITTER);
    windows.inspector_splitter = CreateSplitter(
        windows, instance, IDC_WORKSPACE_INSPECTOR_SPLITTER);
    windows.color_splitter = CreateSplitter(
        windows, instance, IDC_WORKSPACE_COLOR_SPLITTER);
    editors.splitter = CreateSplitter(
        windows, instance, IDC_EDITOR_GROUP_SPLITTER);
    if (windows.tool_splitter == nullptr || windows.inspector_splitter == nullptr
        || windows.color_splitter == nullptr || editors.splitter == nullptr) {
        return false;
    }

    TCITEMW primary{};
    primary.mask = TCIF_TEXT | TCIF_PARAM;
    primary.pszText = const_cast<wchar_t*>(L"無題セル 1");
    primary.lParam = 0;
    if (TabCtrl_InsertItem(windows.document_tabs, 0, &primary) < 0) {
        return false;
    }
    return true;
}

bool CreateEditorGroupTabs(
    app::MainWindowHandles& windows,
    app::EditorGroup& group,
    HINSTANCE instance,
    bool smoke_test) noexcept {
    if (group.document_tabs != nullptr || windows.window == nullptr) {
        return false;
    }
    const DWORD visible = smoke_test ? 0U : WS_VISIBLE;
    const int control = &group == windows.editors->GroupAt(0U)
        ? IDC_MAIN_DOCUMENT_TABS
        : IDC_MAIN_DOCUMENT_TABS_SECONDARY;
    group.document_tabs = CreateWindowExW(
        0,
        WC_TABCONTROLW,
        nullptr,
        WS_CHILD | visible | WS_CLIPSIBLINGS | WS_TABSTOP,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(control)),
        instance,
        nullptr);
    return group.document_tabs != nullptr;
}

void SyncActiveEditorHandles(app::MainWindowHandles& windows) noexcept {
    const app::EditorGroup* active = windows.editors == nullptr
        ? nullptr
        : windows.editors->Active();
    windows.document_tabs = active == nullptr ? nullptr : active->document_tabs;
    windows.canvas = active == nullptr ? nullptr : active->canvas;
}

namespace {

void PlaceEditorGroup(
    app::EditorGroup& group,
    const RECT& bounds,
    int tab_height,
    bool show_tabs) noexcept {
    RECT tabs = bounds;
    tabs.bottom = std::min(bounds.bottom, bounds.top + tab_height);
    RECT canvas = bounds;
    canvas.top = tabs.bottom;
    PlaceChild(group.document_tabs, tabs, show_tabs);
    PlaceChild(group.canvas, canvas, true);
}

void LayoutEditorArea(
    app::MainWindowHandles& windows,
    const WorkspaceLayoutRects& layout,
    bool smoke_test,
    UINT dpi) noexcept {
    app::EditorArea* editors = windows.editors;
    if (editors == nullptr || editors->GroupCount() == 0U) {
        return;
    }
    app::EditorGroup* first = editors->GroupAt(0U);
    if (first == nullptr) {
        return;
    }
    const int tab_height = std::max(
        0L, layout.canvas.top - layout.document_tabs.top);
    const RECT editor_bounds{
        layout.document_tabs.left,
        layout.document_tabs.top,
        layout.document_tabs.right,
        layout.canvas.bottom};
    if (editors->GroupCount() == 1U) {
        PlaceEditorGroup(*first, editor_bounds, tab_height, !smoke_test);
        PlaceChild(editors->splitter, {}, false);
        return;
    }
    app::EditorGroup* second = editors->GroupAt(1U);
    if (second == nullptr) {
        return;
    }
    const int splitter_size = std::max(1, MulDiv(4, static_cast<int>(dpi), 96));
    const int minimum_canvas = std::max(1, MulDiv(160, static_cast<int>(dpi), 96));
    RECT first_bounds = editor_bounds;
    RECT second_bounds = editor_bounds;
    RECT splitter_bounds{};
    const bool horizontal = editors->Orientation()
        == app::EditorSplitOrientation::Horizontal;
    const int extent = horizontal
        ? editor_bounds.bottom - editor_bounds.top
        : editor_bounds.right - editor_bounds.left;
    const int usable = std::max(0, extent - splitter_size);
    const int minimum = std::min(minimum_canvas, usable / 2);
    const int first_extent = std::clamp(
        static_cast<int>(
            static_cast<std::int64_t>(usable) * editors->SplitRatioMilli() / 1000),
        minimum,
        std::max(minimum, usable - minimum));
    if (horizontal) {
        first_bounds.bottom = editor_bounds.top + first_extent;
        splitter_bounds = RECT{
            editor_bounds.left,
            first_bounds.bottom,
            editor_bounds.right,
            first_bounds.bottom + splitter_size};
        second_bounds.top = splitter_bounds.bottom;
    } else {
        first_bounds.right = editor_bounds.left + first_extent;
        splitter_bounds = RECT{
            first_bounds.right,
            editor_bounds.top,
            first_bounds.right + splitter_size,
            editor_bounds.bottom};
        second_bounds.left = splitter_bounds.right;
    }
    PlaceEditorGroup(*first, first_bounds, tab_height, !smoke_test);
    PlaceChild(editors->splitter, splitter_bounds, true);
    PlaceEditorGroup(*second, second_bounds, tab_height, !smoke_test);
}

}  // namespace

void LayoutMainChrome(
    app::MainWindowHandles& windows,
    bool smoke_test,
    int width,
    int height) noexcept {
    int status_height{};
    if (!smoke_test && windows.status_bar != nullptr) {
        SendMessageW(windows.status_bar, WM_SIZE, 0, 0);
        RECT bounds{};
        if (GetWindowRect(windows.status_bar, &bounds) != FALSE) {
            status_height = bounds.bottom - bounds.top;
        }
        const std::array<int, 6U> parts{
            width * 20 / 100,
            width * 33 / 100,
            width * 47 / 100,
            width * 64 / 100,
            width * 81 / 100,
            -1};
        SendMessageW(
            windows.status_bar,
            SB_SETPARTS,
            static_cast<WPARAM>(parts.size()),
            reinterpret_cast<LPARAM>(parts.data()));
    }
    const UINT dpi = windows.window == nullptr ? 96U : GetDpiForWindow(windows.window);
    const WorkspaceLayoutRects layout = ComputeWorkspaceLayout(
        width,
        height,
        smoke_test ? 0 : status_height,
        dpi,
        windows.workspace);
    windows.workspace.last_client_width = width;
    windows.workspace.last_client_height = height;
    windows.workspace.last_body_height = std::max(
        0L, layout.tool.bottom - layout.tool.top);

    PlaceChild(
        windows.tool_options,
        layout.tool_options,
        windows.workspace.tool_options_visible);
    PlaceChild(
        windows.tool_palette,
        layout.tool,
        windows.workspace.tool_visible);
    PlaceChild(
        windows.tool_splitter,
        layout.tool_splitter,
        windows.workspace.tool_visible);
    PlaceChild(
        windows.inspector_splitter,
        layout.inspector_splitter,
        windows.workspace.color_visible || windows.workspace.layer_visible);
    PlaceChild(
        windows.color_pane,
        layout.color,
        windows.workspace.color_visible);
    PlaceChild(
        windows.color_splitter,
        layout.color_splitter,
        windows.workspace.color_visible && windows.workspace.layer_visible);
    PlaceChild(
        windows.layer_palette,
        layout.layer,
        windows.workspace.layer_visible);
    LayoutEditorArea(windows, layout, smoke_test, dpi);
}

bool RegisterMainWindowClass(
    HINSTANCE instance,
    const wchar_t* class_name,
    WNDPROC procedure) noexcept {
    const auto app_icon = LoadIconW(instance, MAKEINTRESOURCEW(IDI_APP_ICON));
    const auto small_icon = reinterpret_cast<HICON>(LoadImageW(
        instance,
        MAKEINTRESOURCEW(IDI_APP_ICON),
        IMAGE_ICON,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
        LR_DEFAULTCOLOR | LR_SHARED));
    if (app_icon == nullptr) {
        return false;
    }
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.style = CS_HREDRAW | CS_VREDRAW;
    window_class.lpfnWndProc = procedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.hIcon = app_icon;
    window_class.hbrBackground = nullptr;
    window_class.lpszMenuName = MAKEINTRESOURCEW(IDR_MAIN_MENU);
    window_class.lpszClassName = class_name;
    window_class.hIconSm = small_icon != nullptr ? small_icon : app_icon;
    return RegisterClassExW(&window_class) != 0;
}

} // namespace inkpod::windows::ui
