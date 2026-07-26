#include "main_window.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>

#include "app/resource.h"

namespace inkpod::windows::ui {

bool CreateMainChrome(
    app::MainWindowHandles& windows,
    HINSTANCE instance,
    bool smoke_test) noexcept {
    const DWORD visible = smoke_test ? 0U : WS_VISIBLE;
    windows.toolbar = CreateWindowExW(
        0,
        TOOLBARCLASSNAMEW,
        nullptr,
        WS_CHILD | visible | TBSTYLE_FLAT | TBSTYLE_TOOLTIPS | TBSTYLE_LIST
            | CCS_NODIVIDER,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_TOOLBAR)),
        instance,
        nullptr);
    if (windows.toolbar == nullptr) {
        return false;
    }
    SendMessageW(windows.toolbar, TB_BUTTONSTRUCTSIZE, sizeof(TBBUTTON), 0);
    SendMessageW(windows.toolbar, TB_SETEXTENDEDSTYLE, 0, TBSTYLE_EX_MIXEDBUTTONS);
    constexpr wchar_t labels[] =
        L"保存\0全体\0等倍\0範囲\0左右\0上下\0ルーラー\0ガイド\0グリッド\0透明\0";
    const LRESULT first_string = SendMessageW(
        windows.toolbar,
        TB_ADDSTRINGW,
        0,
        reinterpret_cast<LPARAM>(labels));
    if (first_string < 0) {
        return false;
    }
    const std::array<UINT, 10U> commands{
        IDM_FILE_SAVE,
        IDM_VIEW_FIT,
        IDM_VIEW_ONE_TO_ONE,
        IDM_VIEW_BOX_ZOOM,
        IDM_VIEW_FLIP_HORIZONTAL,
        IDM_VIEW_FLIP_VERTICAL,
        IDM_VIEW_RULER,
        IDM_VIEW_GUIDES,
        IDM_VIEW_GRID,
        IDM_VIEW_TRANSPARENT};
    std::array<TBBUTTON, commands.size()> buttons{};
    for (std::size_t index = 0; index < buttons.size(); ++index) {
        buttons[index].iBitmap = I_IMAGENONE;
        buttons[index].idCommand = static_cast<int>(commands[index]);
        buttons[index].fsState = TBSTATE_ENABLED;
        buttons[index].fsStyle = static_cast<BYTE>(
            BTNS_BUTTON | BTNS_AUTOSIZE | BTNS_SHOWTEXT
            | (index >= 3U ? BTNS_CHECK : 0U));
        buttons[index].iString = first_string + static_cast<LRESULT>(index);
    }
    if (SendMessageW(
            windows.toolbar,
            TB_ADDBUTTONS,
            static_cast<WPARAM>(buttons.size()),
            reinterpret_cast<LPARAM>(buttons.data())) == FALSE) {
        return false;
    }

    windows.zoom_slider = CreateWindowExW(
        0,
        TRACKBAR_CLASSW,
        nullptr,
        WS_CHILD | visible | TBS_HORZ | TBS_AUTOTICKS | TBS_TOOLTIPS,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_ZOOM_SLIDER)),
        instance,
        nullptr);
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
    windows.locator_label = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"STATIC",
        L"カラーロケーター\r\nX: —  Y: —\r\nH: —  V: —  L: —\r\nRGBA: —",
        WS_CHILD | visible | SS_LEFT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LOCATOR)),
        instance,
        nullptr);
    windows.layer_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LAYER_LIST)),
        instance,
        nullptr);
    windows.plane_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_PLANE_LIST)),
        instance,
        nullptr);
    windows.light_table_set_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LT_SET_LIST)),
        instance,
        nullptr);
    windows.light_table_item_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LT_ITEM_LIST)),
        instance,
        nullptr);
    windows.sequence_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_SEQUENCE_LIST)),
        instance,
        nullptr);
    windows.motion_label = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"STATIC",
        L"モーション停止",
        WS_CHILD | visible | SS_LEFT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_MOTION_LABEL)),
        instance,
        nullptr);
    windows.color_palette_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_COLOR_PALETTE)),
        instance,
        nullptr);
    windows.color_chart_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        windows.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_COLOR_CHART)),
        instance,
        nullptr);
    if (windows.zoom_slider == nullptr || windows.status_bar == nullptr
        || windows.document_tabs == nullptr || windows.locator_label == nullptr
        || windows.layer_list == nullptr || windows.plane_list == nullptr
        || windows.light_table_set_list == nullptr
        || windows.light_table_item_list == nullptr || windows.sequence_list == nullptr
        || windows.motion_label == nullptr || windows.color_palette_list == nullptr
        || windows.color_chart_list == nullptr) {
        return false;
    }

    TCITEMW primary{};
    primary.mask = TCIF_TEXT | TCIF_PARAM;
    primary.pszText = const_cast<wchar_t*>(L"セル");
    primary.lParam = 0;
    if (TabCtrl_InsertItem(windows.document_tabs, 0, &primary) < 0) {
        return false;
    }
    SendMessageW(windows.zoom_slider, TBM_SETRANGE, TRUE, MAKELPARAM(1, 800));
    SendMessageW(windows.zoom_slider, TBM_SETTICFREQ, 100, 0);
    SendMessageW(windows.zoom_slider, TBM_SETPOS, TRUE, 100);
    return true;
}

void LayoutMainChrome(
    const app::MainWindowHandles& windows,
    bool smoke_test,
    int width,
    int height) noexcept {
    int toolbar_height{};
    int status_height{};
    if (!smoke_test && windows.toolbar != nullptr) {
        SendMessageW(windows.toolbar, TB_AUTOSIZE, 0, 0);
        RECT bounds{};
        if (GetWindowRect(windows.toolbar, &bounds) != FALSE) {
            toolbar_height = bounds.bottom - bounds.top;
        }
        MoveWindow(windows.zoom_slider, std::max(0, width - 178), 2, 170, 28, TRUE);
    }
    if (!smoke_test && windows.status_bar != nullptr) {
        SendMessageW(windows.status_bar, WM_SIZE, 0, 0);
        RECT bounds{};
        if (GetWindowRect(windows.status_bar, &bounds) != FALSE) {
            status_height = bounds.bottom - bounds.top;
        }
        const std::array<int, 3U> parts{
            std::max(160, width / 4),
            std::max(320, width / 2),
            -1};
        SendMessageW(
            windows.status_bar,
            SB_SETPARTS,
            static_cast<WPARAM>(parts.size()),
            reinterpret_cast<LPARAM>(parts.data()));
    }
    int content_left{};
    int content_top = toolbar_height;
    int content_width = width;
    int content_height = std::max(0, height - toolbar_height - status_height);
    if (!smoke_test) {
        constexpr int right_pane_width = 238;
        constexpr int tabs_height = 28;
        content_width = std::max(0, width - right_pane_width);
        if (windows.document_tabs != nullptr) {
            MoveWindow(
                windows.document_tabs,
                content_left,
                content_top,
                content_width,
                tabs_height,
                TRUE);
        }
        if (windows.locator_label != nullptr) {
            MoveWindow(
                windows.locator_label,
                content_width + 6,
                content_top + 6,
                std::max(0, right_pane_width - 12),
                70,
                TRUE);
        }
        if (windows.motion_label != nullptr) {
            MoveWindow(
                windows.motion_label,
                content_width + 6,
                content_top + 78,
                std::max(0, right_pane_width - 12),
                28,
                TRUE);
        }
        const int pane_x = content_width + 6;
        const int pane_width = std::max(0, right_pane_width - 12);
        const int lists_top = content_top + 110;
        const int lists_height = std::max(0, content_height - 116);
        const int section = std::max(0, (lists_height - 24) / 7);
        const std::array<HWND, 7U> panes{
            windows.layer_list,
            windows.plane_list,
            windows.light_table_set_list,
            windows.light_table_item_list,
            windows.sequence_list,
            windows.color_palette_list,
            windows.color_chart_list};
        for (std::size_t index = 0; index < panes.size(); ++index) {
            if (panes[index] != nullptr) {
                const int pane_height = index + 1U == panes.size()
                    ? std::max(0, lists_height - (section + 4) * 6)
                    : section;
                MoveWindow(
                    panes[index],
                    pane_x,
                    lists_top + (section + 4) * static_cast<int>(index),
                    pane_width,
                    pane_height,
                    TRUE);
            }
        }
        content_top += tabs_height;
        content_height = std::max(0, content_height - tabs_height);
    }
    if (windows.canvas != nullptr) {
        MoveWindow(
            windows.canvas,
            content_left,
            content_top,
            content_width,
            content_height,
            TRUE);
    }
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
