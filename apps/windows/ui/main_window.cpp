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
                if (control == IDC_WORKSPACE_TOOL_SPLITTER) {
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
                RelayoutFromSplitter(*windows);
                return 0;
            }
            break;
        case WM_LBUTTONUP:
            if (GetCapture() == splitter) {
                ReleaseCapture();
            }
            layout.drag_control = 0;
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
            if (control == IDC_WORKSPACE_TOOL_SPLITTER) {
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
                control == IDC_WORKSPACE_COLOR_SPLITTER ? IDC_SIZENS : IDC_SIZEWE));
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
    HINSTANCE instance,
    bool smoke_test) noexcept {
    const DWORD visible = smoke_test ? 0U : WS_VISIBLE;
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
    windows.document_tabs = CreateWindowExW(
        0,
        WC_TABCONTROLW,
        nullptr,
        WS_CHILD | visible | WS_CLIPSIBLINGS | WS_TABSTOP,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_DOCUMENT_TABS)),
        instance,
        nullptr);
    if (windows.status_bar == nullptr || windows.document_tabs == nullptr) {
        return false;
    }
    windows.tool_splitter = CreateSplitter(
        windows, instance, IDC_WORKSPACE_TOOL_SPLITTER);
    windows.inspector_splitter = CreateSplitter(
        windows, instance, IDC_WORKSPACE_INSPECTOR_SPLITTER);
    windows.color_splitter = CreateSplitter(
        windows, instance, IDC_WORKSPACE_COLOR_SPLITTER);
    if (windows.tool_splitter == nullptr || windows.inspector_splitter == nullptr
        || windows.color_splitter == nullptr) {
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
    PlaceChild(
        windows.document_tabs,
        layout.document_tabs,
        !smoke_test);
    PlaceChild(windows.canvas, layout.canvas, true);
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
