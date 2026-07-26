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

    TCITEMW primary{};
    primary.mask = TCIF_TEXT | TCIF_PARAM;
    primary.pszText = const_cast<wchar_t*>(L"セル");
    primary.lParam = 0;
    if (TabCtrl_InsertItem(windows.document_tabs, 0, &primary) < 0) {
        return false;
    }
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
        constexpr int tabs_height = 28;
        if (windows.document_tabs != nullptr) {
            MoveWindow(
                windows.document_tabs,
                content_left,
                content_top,
                content_width,
                tabs_height,
                TRUE);
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
