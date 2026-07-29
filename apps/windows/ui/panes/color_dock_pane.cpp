#include "color_dock_pane.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cwchar>
#include <new>

#include "app/resource.h"

namespace inkpod::windows::ui::panes {
namespace {

constexpr UINT_PTR kPaneSubclass = 1U;

int ScaleForDpi(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

ColorDockPaneState* PaneState(HWND pane) noexcept {
    return reinterpret_cast<ColorDockPaneState*>(
        GetWindowLongPtrW(pane, GWLP_USERDATA));
}

std::uint8_t Channel8(const InkpodColorValue& color, std::uint16_t value) noexcept {
    return static_cast<std::uint8_t>(
        color.depth == INKPOD_COLOR_DEPTH_16
            ? (static_cast<std::uint32_t>(value) + 128U) / 257U
            : value & 0xffU);
}

COLORREF ColorRef(const InkpodColorValue& color) noexcept {
    return RGB(
        Channel8(color, color.red),
        Channel8(color, color.green),
        Channel8(color, color.blue));
}

void ShowTabControls(HWND pane, int tab) noexcept {
    for (const int control : {
             IDC_COLOR_SWATCH,
             IDC_COLOR_RED,
             IDC_COLOR_GREEN,
             IDC_COLOR_BLUE,
             IDC_COLOR_ALPHA,
             IDC_COLOR_APPLY}) {
        ShowWindow(GetDlgItem(pane, control), tab == 0 ? SW_SHOW : SW_HIDE);
    }
    for (const int control : {
             IDC_PALETTE_LIST,
             IDC_PALETTE_PREVIOUS,
             IDC_PALETTE_NEXT}) {
        ShowWindow(GetDlgItem(pane, control), tab == 1 ? SW_SHOW : SW_HIDE);
    }
    ShowWindow(
        GetDlgItem(pane, IDC_COLOR_CHART_LIST), tab == 2 ? SW_SHOW : SW_HIDE);
}

void LayoutPane(HWND pane) noexcept {
    RECT client{};
    if (GetClientRect(pane, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(pane);
    const int margin = ScaleForDpi(6, dpi);
    const int tabs_height = ScaleForDpi(28, dpi);
    const int row = ScaleForDpi(24, dpi);
    const int gap = ScaleForDpi(5, dpi);
    SetWindowPos(
        GetDlgItem(pane, IDC_COLOR_TABS),
        nullptr,
        margin,
        margin,
        std::max(0, static_cast<int>(client.right) - margin * 2),
        std::max(0, static_cast<int>(client.bottom) - margin * 2),
        SWP_NOACTIVATE | SWP_NOZORDER);
    RECT content{margin * 2, margin + tabs_height, client.right - margin * 2,
                 client.bottom - margin * 2};
    const int swatch = ScaleForDpi(48, dpi);
    SetWindowPos(
        GetDlgItem(pane, IDC_COLOR_SWATCH),
        nullptr,
        content.left,
        content.top,
        swatch,
        swatch,
        SWP_NOACTIVATE | SWP_NOZORDER);
    int x = content.left + swatch + gap;
    const int field_width = std::max(
        ScaleForDpi(38, dpi),
        (static_cast<int>(content.right) - x - gap * 3) / 4);
    for (const int control : {
             IDC_COLOR_RED, IDC_COLOR_GREEN, IDC_COLOR_BLUE, IDC_COLOR_ALPHA}) {
        SetWindowPos(
            GetDlgItem(pane, control),
            nullptr,
            x,
            content.top,
            field_width,
            row,
            SWP_NOACTIVATE | SWP_NOZORDER);
        x += field_width + gap;
    }
    SetWindowPos(
        GetDlgItem(pane, IDC_COLOR_APPLY),
        nullptr,
        content.left + swatch + gap,
        content.top + row + gap,
        ScaleForDpi(78, dpi),
        row,
        SWP_NOACTIVATE | SWP_NOZORDER);
    const int button_width = ScaleForDpi(32, dpi);
    SetWindowPos(
        GetDlgItem(pane, IDC_PALETTE_PREVIOUS),
        nullptr,
        content.left,
        content.top,
        button_width,
        row,
        SWP_NOACTIVATE | SWP_NOZORDER);
    SetWindowPos(
        GetDlgItem(pane, IDC_PALETTE_NEXT),
        nullptr,
        content.right - button_width,
        content.top,
        button_width,
        row,
        SWP_NOACTIVATE | SWP_NOZORDER);
    SetWindowPos(
        GetDlgItem(pane, IDC_PALETTE_LIST),
        nullptr,
        content.left,
        content.top + row + gap,
        std::max(0, static_cast<int>(content.right - content.left)),
        std::max(
            0, static_cast<int>(content.bottom - content.top) - row - gap),
        SWP_NOACTIVATE | SWP_NOZORDER);
    SetWindowPos(
        GetDlgItem(pane, IDC_COLOR_CHART_LIST),
        nullptr,
        content.left,
        content.top,
        std::max(0, static_cast<int>(content.right - content.left)),
        std::max(0, static_cast<int>(content.bottom - content.top)),
        SWP_NOACTIVATE | SWP_NOZORDER);
}

void UpdateFont(HWND pane, ColorDockPaneState& state) noexcept {
    const HFONT replacement = CreateFontW(
        -MulDiv(9, static_cast<int>(GetDpiForWindow(pane)), 72),
        0,
        0,
        0,
        FW_NORMAL,
        FALSE,
        FALSE,
        FALSE,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH | FF_DONTCARE,
        L"Segoe UI");
    if (replacement == nullptr) {
        return;
    }
    for (const int control : {
             IDC_COLOR_TABS,
             IDC_COLOR_RED,
             IDC_COLOR_GREEN,
             IDC_COLOR_BLUE,
             IDC_COLOR_ALPHA,
             IDC_COLOR_APPLY,
             IDC_PALETTE_LIST,
             IDC_PALETTE_PREVIOUS,
             IDC_PALETTE_NEXT,
             IDC_COLOR_CHART_LIST}) {
        SendDlgItemMessageW(
            pane, control, WM_SETFONT, reinterpret_cast<WPARAM>(replacement), TRUE);
    }
    if (state.font != nullptr) {
        DeleteObject(state.font);
    }
    state.font = replacement;
}

void SetColorFields(HWND pane, const InkpodColorValue& color) noexcept {
    const std::array<std::pair<int, std::uint16_t>, 4U> fields{{
        {IDC_COLOR_RED, color.red},
        {IDC_COLOR_GREEN, color.green},
        {IDC_COLOR_BLUE, color.blue},
        {IDC_COLOR_ALPHA, color.alpha},
    }};
    for (const auto& [control, value] : fields) {
        std::array<wchar_t, 16U> text{};
        swprintf_s(text.data(), text.size(), L"%u", static_cast<unsigned>(value));
        SetDlgItemTextW(pane, control, text.data());
    }
}

bool ReadChannel(HWND pane, int control, std::uint32_t maximum, std::uint16_t& output) noexcept {
    std::array<wchar_t, 32U> text{};
    GetDlgItemTextW(pane, control, text.data(), static_cast<int>(text.size()));
    wchar_t* end{};
    const unsigned long value = std::wcstoul(text.data(), &end, 10);
    if (end == text.data() || *end != L'\0' || value > maximum) {
        return false;
    }
    output = static_cast<std::uint16_t>(value);
    return true;
}

void ApplyFields(HWND pane, ColorDockPaneState& state) noexcept {
    InkpodColorValue color = state.drawing_color;
    const std::uint32_t maximum = color.depth == INKPOD_COLOR_DEPTH_16
        ? UINT16_MAX
        : UINT8_MAX;
    if (ReadChannel(pane, IDC_COLOR_RED, maximum, color.red)
        && ReadChannel(pane, IDC_COLOR_GREEN, maximum, color.green)
        && ReadChannel(pane, IDC_COLOR_BLUE, maximum, color.blue)
        && ReadChannel(pane, IDC_COLOR_ALPHA, maximum, color.alpha)
        && state.change_color != nullptr) {
        state.change_color(state.context, color);
    } else {
        SetColorFields(pane, state.drawing_color);
    }
}

void DrawSwatch(const DRAWITEMSTRUCT& draw, const InkpodColorValue& color) noexcept {
    const HBRUSH brush = CreateSolidBrush(ColorRef(color));
    if (brush != nullptr) {
        FillRect(draw.hDC, &draw.rcItem, brush);
        DeleteObject(brush);
    }
    FrameRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(COLOR_WINDOWTEXT));
}

void DrawColorListItem(
    const DRAWITEMSTRUCT& draw,
    const ColorDockPaneState& state,
    bool chart) noexcept {
    if (draw.itemID == static_cast<UINT>(-1)) {
        return;
    }
    const std::size_t index = static_cast<std::size_t>(draw.itemData);
    if (index >= state.colors.size()) {
        return;
    }
    const bool selected = (draw.itemState & ODS_SELECTED) != 0U;
    FillRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(selected ? COLOR_HIGHLIGHT : COLOR_WINDOW));
    RECT chip = draw.rcItem;
    chip.left += 4;
    chip.top += 3;
    chip.right = chip.left + std::max(12L, draw.rcItem.bottom - draw.rcItem.top - 6L);
    chip.bottom -= 3;
    const HBRUSH color_brush = CreateSolidBrush(ColorRef(state.colors[index]));
    if (color_brush != nullptr) {
        FillRect(draw.hDC, &chip, color_brush);
        DeleteObject(color_brush);
    }
    FrameRect(
        draw.hDC, &chip, GetSysColorBrush(COLOR_WINDOWTEXT));
    RECT label = draw.rcItem;
    label.left = chip.right + 8;
    SetBkMode(draw.hDC, TRANSPARENT);
    SetTextColor(
        draw.hDC,
        GetSysColor(selected ? COLOR_HIGHLIGHTTEXT : COLOR_WINDOWTEXT));
    std::array<wchar_t, 128U> text{};
    if (chart && index < state.names.size()) {
        wcsncpy_s(text.data(), text.size(), state.names[index].c_str(), _TRUNCATE);
    } else {
        swprintf_s(
            text.data(),
            text.size(),
            L"%u  #%02X%02X%02X",
            static_cast<unsigned>(index + 1U),
            Channel8(state.colors[index], state.colors[index].red),
            Channel8(state.colors[index], state.colors[index].green),
            Channel8(state.colors[index], state.colors[index].blue));
    }
    DrawTextW(
        draw.hDC,
        text.data(),
        -1,
        &label,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    if ((draw.itemState & ODS_FOCUS) != 0U) {
        DrawFocusRect(draw.hDC, &draw.rcItem);
    }
}

void SelectListColor(
    HWND pane,
    ColorDockPaneState& state,
    int control,
    bool chart) noexcept {
    const HWND list = GetDlgItem(pane, control);
    const LRESULT selection = SendMessageW(list, LB_GETCURSEL, 0, 0);
    if (selection == LB_ERR || state.select_color == nullptr) {
        return;
    }
    const LRESULT data = SendMessageW(list, LB_GETITEMDATA, selection, 0);
    if (data != LB_ERR && data >= 0) {
        state.select_color(
            state.context, static_cast<std::uint32_t>(data), chart);
    }
}

LRESULT CALLBACK PaneSubclassProcedure(
    HWND pane,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<ColorDockPaneState*>(reference);
    switch (message) {
        case WM_SIZE:
            LayoutPane(pane);
            return 0;
        case WM_NOTIFY:
            if (state != nullptr) {
                const auto* notification = reinterpret_cast<const NMHDR*>(lparam);
                if (notification != nullptr && notification->idFrom == IDC_COLOR_TABS
                    && notification->code == TCN_SELCHANGE) {
                    state->active_tab = std::max(
                        0, TabCtrl_GetCurSel(notification->hwndFrom));
                    ShowTabControls(pane, state->active_tab);
                    return 0;
                }
            }
            break;
        case WM_COMMAND:
            if (state == nullptr || state->updating) {
                break;
            }
            if (LOWORD(wparam) == IDC_COLOR_APPLY && HIWORD(wparam) == BN_CLICKED) {
                ApplyFields(pane, *state);
                return 0;
            }
            if (LOWORD(wparam) == IDC_PALETTE_PREVIOUS
                && HIWORD(wparam) == BN_CLICKED && state->change_group != nullptr) {
                state->change_group(state->context, -1);
                return 0;
            }
            if (LOWORD(wparam) == IDC_PALETTE_NEXT
                && HIWORD(wparam) == BN_CLICKED && state->change_group != nullptr) {
                state->change_group(state->context, 1);
                return 0;
            }
            if (LOWORD(wparam) == IDC_PALETTE_LIST
                && (HIWORD(wparam) == LBN_SELCHANGE
                    || HIWORD(wparam) == LBN_DBLCLK)) {
                SelectListColor(pane, *state, IDC_PALETTE_LIST, false);
                if (HIWORD(wparam) == LBN_DBLCLK
                    && state->dispatch_command != nullptr) {
                    state->dispatch_command(state->context, IDM_PALETTE_REGISTER);
                }
                return 0;
            }
            if (LOWORD(wparam) == IDC_COLOR_CHART_LIST
                && HIWORD(wparam) == LBN_SELCHANGE) {
                SelectListColor(pane, *state, IDC_COLOR_CHART_LIST, true);
                return 0;
            }
            break;
        case WM_DRAWITEM:
            if (state == nullptr) {
                break;
            }
            if (wparam == IDC_COLOR_SWATCH) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawSwatch(*draw, state->drawing_color);
                }
                return TRUE;
            }
            if (wparam == IDC_PALETTE_LIST || wparam == IDC_COLOR_CHART_LIST) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawColorListItem(
                        *draw, *state, wparam == IDC_COLOR_CHART_LIST);
                }
                return TRUE;
            }
            break;
        case WM_DPICHANGED_AFTERPARENT:
            if (state != nullptr) {
                UpdateFont(pane, *state);
                LayoutPane(pane);
            }
            return 0;
        case WM_NCDESTROY:
            if (state != nullptr && state->font != nullptr) {
                DeleteObject(state->font);
                state->font = nullptr;
            }
            SetWindowLongPtrW(pane, GWLP_USERDATA, 0);
            RemoveWindowSubclass(pane, PaneSubclassProcedure, kPaneSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(pane, message, wparam, lparam);
}

HWND CreateControl(
    HINSTANCE instance,
    HWND parent,
    const wchar_t* class_name,
    const wchar_t* text,
    DWORD style,
    int id) noexcept {
    return CreateWindowExW(
        0,
        class_name,
        text,
        WS_CHILD | WS_VISIBLE | style,
        0,
        0,
        0,
        0,
        parent,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)),
        instance,
        nullptr);
}

void PopulateLists(HWND pane, ColorDockPaneState& state) noexcept {
    const HWND palette = GetDlgItem(pane, IDC_PALETTE_LIST);
    const HWND chart = GetDlgItem(pane, IDC_COLOR_CHART_LIST);
    SendMessageW(palette, WM_SETREDRAW, FALSE, 0);
    SendMessageW(chart, WM_SETREDRAW, FALSE, 0);
    SendMessageW(palette, LB_RESETCONTENT, 0, 0);
    SendMessageW(chart, LB_RESETCONTENT, 0, 0);
    const std::size_t palette_begin = static_cast<std::size_t>(state.palette_group) * 10U;
    const std::size_t palette_end = std::min(state.colors.size(), palette_begin + 10U);
    for (std::size_t index = palette_begin; index < palette_end; ++index) {
        const LRESULT item = SendMessageW(
            palette, LB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L""));
        if (item != LB_ERR && item != LB_ERRSPACE) {
            SendMessageW(palette, LB_SETITEMDATA, item, static_cast<LPARAM>(index));
        }
    }
    const std::size_t chart_begin = static_cast<std::size_t>(state.chart_page) * 20U;
    const std::size_t chart_end = std::min(state.colors.size(), chart_begin + 20U);
    for (std::size_t index = chart_begin; index < chart_end; ++index) {
        const LRESULT item = SendMessageW(
            chart, LB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L""));
        if (item != LB_ERR && item != LB_ERRSPACE) {
            SendMessageW(chart, LB_SETITEMDATA, item, static_cast<LPARAM>(index));
        }
    }
    EnableWindow(chart, state.chart_locked ? FALSE : TRUE);
    SendMessageW(palette, WM_SETREDRAW, TRUE, 0);
    SendMessageW(chart, WM_SETREDRAW, TRUE, 0);
    InvalidateRect(palette, nullptr, TRUE);
    InvalidateRect(chart, nullptr, TRUE);
}

}  // namespace

HWND CreateColorDockPane(
    HINSTANCE instance,
    HWND parent,
    ColorDockPaneState& state) noexcept {
    const HWND pane = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        L"STATIC",
        nullptr,
        WS_CHILD | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        parent,
        nullptr,
        instance,
        nullptr);
    if (pane == nullptr) {
        return nullptr;
    }
    const HWND tabs = CreateControl(
        instance,
        pane,
        WC_TABCONTROLW,
        nullptr,
        WS_TABSTOP | WS_CLIPSIBLINGS,
        IDC_COLOR_TABS);
    const bool controls_created = tabs != nullptr
        && CreateControl(
               instance, pane, L"STATIC", nullptr, SS_OWNERDRAW, IDC_COLOR_SWATCH)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"0", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_RED)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"0", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_GREEN)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"0", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_BLUE)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"255", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_ALPHA)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", L"適用", WS_TABSTOP | BS_PUSHBUTTON,
               IDC_COLOR_APPLY)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"LISTBOX",
               nullptr,
               WS_BORDER | WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY
                   | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT,
               IDC_PALETTE_LIST)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", L"<", WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_PREVIOUS)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", L">", WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_NEXT)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"LISTBOX",
               nullptr,
               WS_BORDER | WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY
                   | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT,
               IDC_COLOR_CHART_LIST)
            != nullptr;
    if (!controls_created) {
        DestroyWindow(pane);
        return nullptr;
    }
    for (const wchar_t* label : {L"カラー", L"パレット", L"チャート"}) {
        TCITEMW item{};
        item.mask = TCIF_TEXT;
        item.pszText = const_cast<wchar_t*>(label);
        TabCtrl_InsertItem(tabs, TabCtrl_GetItemCount(tabs), &item);
    }
    SetWindowLongPtrW(
        pane, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(&state));
    SetWindowSubclass(
        pane,
        PaneSubclassProcedure,
        kPaneSubclass,
        reinterpret_cast<DWORD_PTR>(&state));
    UpdateFont(pane, state);
    ShowTabControls(pane, 0);
    LayoutPane(pane);
    return pane;
}

void UpdateColorDockPane(
    HWND pane,
    const InkpodColorValue& drawing_color,
    const std::vector<InkpodColorValue>& colors,
    const std::vector<std::wstring>& names,
    std::uint32_t palette_group,
    std::uint32_t chart_page,
    bool chart_locked) noexcept {
    ColorDockPaneState* state = pane == nullptr ? nullptr : PaneState(pane);
    if (state == nullptr) {
        return;
    }
    try {
        state->colors = colors;
        state->names = names;
    } catch (const std::bad_alloc&) {
        return;
    }
    state->updating = true;
    state->drawing_color = drawing_color;
    state->palette_group = palette_group;
    state->chart_page = chart_page;
    state->chart_locked = chart_locked;
    SetColorFields(pane, drawing_color);
    PopulateLists(pane, *state);
    InvalidateRect(GetDlgItem(pane, IDC_COLOR_SWATCH), nullptr, TRUE);
    state->updating = false;
}

}  // namespace inkpod::windows::ui::panes
