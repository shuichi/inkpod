#include "tool_palette.h"

#include <commctrl.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>

#include "app/resource.h"
#include "ui/palette_window.h"

namespace inkpod::windows::ui {
namespace {

constexpr std::array<ToolPaletteEntry, kToolPaletteEntryCount>
    kToolPaletteEntries{{
        {IDM_TOOL_PENCIL, L"鉛筆", ToolPalettePage::Basic},
        {IDM_TOOL_BRUSH, L"ブラシ", ToolPalettePage::Basic},
        {IDM_TOOL_ERASER, L"消しゴム", ToolPalettePage::Basic},
        {IDM_TOOL_FILL, L"フィル", ToolPalettePage::Basic},
        {IDM_TOOL_CLOSED_FILL, L"閉領域フィル", ToolPalettePage::Basic},
        {IDM_TOOL_FILL_EXTENSION, L"塗りのばし", ToolPalettePage::Basic},
        {IDM_TOOL_FILL_OPTIONS, L"フィル設定...", ToolPalettePage::Basic},
        {IDM_TOOL_EYEDROPPER, L"スポイト", ToolPalettePage::Basic},
        {IDM_VECTOR_LINE, L"描画: 直線", ToolPalettePage::Vector},
        {IDM_VECTOR_CURVE, L"描画: 曲線", ToolPalettePage::Vector},
        {IDM_VECTOR_RECTANGLE, L"描画: 長方形", ToolPalettePage::Vector},
        {IDM_VECTOR_ELLIPSE, L"描画: 楕円", ToolPalettePage::Vector},
        {IDM_VECTOR_POLYLINE, L"描画: 折れ線", ToolPalettePage::Vector},
        {IDM_VECTOR_ERASER, L"描画: 消しゴム", ToolPalettePage::Vector},
        {IDM_VECTOR_ERASE_PARTIAL, L"消去: 触れた部分", ToolPalettePage::Vector},
        {IDM_VECTOR_ERASE_INTERSECTION, L"消去: 交点まで", ToolPalettePage::Vector},
        {IDM_VECTOR_ERASE_WHOLE, L"消去: 線全体", ToolPalettePage::Vector},
        {IDM_VECTOR_CONNECT, L"線つなぎ...", ToolPalettePage::Vector},
        {IDM_VECTOR_WIDTH, L"線幅修正...", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_CUT, L"選択: 選択範囲で切断", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_TOUCH, L"選択: 一部でも触れる線", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_CONTAINED, L"選択: 完全に含まれる線", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_LINE, L"選択: 線を選択", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_WHOLE_LINE, L"選択: 線全体", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_INTERSECTION, L"選択: 交点まで", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_FILL_BOUNDARY, L"選択: 塗りを囲む線", ToolPalettePage::Vector},
        {IDM_VECTOR_SELECT_FILL, L"選択: 塗り", ToolPalettePage::Vector},
        {IDM_VECTOR_RASTERIZE, L"ラスタライズ...", ToolPalettePage::Vector},
        {IDM_VECTOR_VECTORIZE, L"ベクター化...", ToolPalettePage::Vector},
        {IDM_EFFECT_GRADIENT, L"グラデーション...", ToolPalettePage::Effects},
        {IDM_EFFECT_AIRBRUSH, L"エアブラシ...", ToolPalettePage::Effects},
        {IDM_EFFECT_BOUNDARY_AIRBRUSH, L"境界色エアブラシ...", ToolPalettePage::Effects},
        {IDM_EFFECT_BLUR, L"ぼかしツール...", ToolPalettePage::Effects},
        {IDM_EFFECT_STAMP, L"スタンプ...", ToolPalettePage::Effects},
        {IDM_EFFECT_DUST, L"ゴミ取り...", ToolPalettePage::Effects},
        {IDM_EFFECT_ALPHA_GRADIENT, L"アルファグラデーション...", ToolPalettePage::Effects},
        {IDM_EFFECT_ALPHA_VIEW, L"アルファをグレースケール表示", ToolPalettePage::Effects},
    }};

constexpr std::array<const wchar_t*, kToolPalettePageCount>
    kToolPalettePageLabels{{L"基本", L"ベクター", L"効果"}};

constexpr int kReferenceDpi = 96;
constexpr int kMargin = 5;
constexpr int kPageInset = 3;
constexpr int kButtonHeight = 18;
constexpr int kButtonGap = 2;
constexpr int kFontSizePoints = 5;
constexpr int kMinimumWidth = 150;
constexpr int kMinimumHeight = 130;

int ScaleForDpi(int value, UINT dpi) noexcept {
    return MulDiv(
        value,
        static_cast<int>(dpi == 0U ? kReferenceDpi : dpi),
        kReferenceDpi);
}

ToolPaletteDialogState* DialogState(HWND dialog) noexcept {
    return reinterpret_cast<ToolPaletteDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
}

std::size_t PageEntryCount(ToolPalettePage page) noexcept {
    return static_cast<std::size_t>(std::count_if(
        kToolPaletteEntries.begin(),
        kToolPaletteEntries.end(),
        [page](const ToolPaletteEntry& entry) { return entry.page == page; }));
}

int ContentHeight(ToolPalettePage page, UINT dpi) noexcept {
    const std::size_t count = PageEntryCount(page);
    if (count == 0U) {
        return 0;
    }
    const int height = ScaleForDpi(kButtonHeight, dpi);
    const int gap = ScaleForDpi(kButtonGap, dpi);
    return static_cast<int>(count) * height
        + static_cast<int>(count - 1U) * gap;
}

bool PalettePageBounds(
    HWND dialog,
    UINT dpi,
    bool layout_tab,
    RECT& bounds) noexcept {
    const HWND tabs = GetDlgItem(dialog, IDC_TOOL_PALETTE_TAB);
    RECT client{};
    if (tabs == nullptr || GetClientRect(dialog, &client) == FALSE) {
        return false;
    }
    const int margin = ScaleForDpi(kMargin, dpi);
    if (layout_tab
        && MoveWindow(
               tabs,
               margin,
               margin,
               std::max(0, static_cast<int>(client.right) - margin * 2),
               std::max(0, static_cast<int>(client.bottom) - margin * 2),
               TRUE)
            == FALSE) {
        return false;
    }
    if (GetClientRect(tabs, &bounds) == FALSE) {
        return false;
    }
    TabCtrl_AdjustRect(tabs, FALSE, &bounds);
    MapWindowPoints(tabs, dialog, reinterpret_cast<POINT*>(&bounds), 2);
    const int inset = ScaleForDpi(kPageInset, dpi);
    InflateRect(&bounds, -inset, -inset);
    return bounds.right > bounds.left && bounds.bottom > bounds.top;
}

void LayoutButtons(HWND dialog, ToolPaletteDialogState& state) noexcept {
    const UINT dpi = GetDpiForWindow(dialog);
    RECT page_bounds{};
    if (!PalettePageBounds(dialog, dpi, true, page_bounds)) {
        return;
    }
    const int button_height = ScaleForDpi(kButtonHeight, dpi);
    const int gap = ScaleForDpi(kButtonGap, dpi);
    const int viewport_height = page_bounds.bottom - page_bounds.top;
    const int content_height = ContentHeight(state.active_page, dpi);
    const int maximum = std::max(0, content_height - viewport_height);
    state.scroll_position = std::clamp(state.scroll_position, 0, maximum);

    SCROLLINFO scroll{};
    scroll.cbSize = sizeof(scroll);
    scroll.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
    scroll.nMin = 0;
    scroll.nMax = std::max(0, content_height - 1);
    scroll.nPage = static_cast<UINT>(viewport_height);
    scroll.nPos = state.scroll_position;
    SetScrollInfo(dialog, SB_VERT, &scroll, TRUE);

    const int width = page_bounds.right - page_bounds.left;
    std::size_t page_index{};
    for (const auto& entry : kToolPaletteEntries) {
        const HWND button = GetDlgItem(dialog, entry.command);
        if (button == nullptr) {
            continue;
        }
        const bool visible = entry.page == state.active_page;
        const int y = page_bounds.top
            + static_cast<int>(page_index) * (button_height + gap)
            - state.scroll_position;
        SetWindowPos(
            button,
            nullptr,
            page_bounds.left,
            y,
            width,
            button_height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER
                | (visible ? SWP_SHOWWINDOW : SWP_HIDEWINDOW));
        if (visible) {
            ++page_index;
        }
    }
}

void SetScrollPosition(
    HWND dialog,
    ToolPaletteDialogState& state,
    int position) noexcept {
    const UINT dpi = GetDpiForWindow(dialog);
    RECT page_bounds{};
    if (!PalettePageBounds(dialog, dpi, false, page_bounds)) {
        return;
    }
    const int maximum = std::max(
        0,
        ContentHeight(state.active_page, dpi)
            - static_cast<int>(page_bounds.bottom - page_bounds.top));
    const int next = std::clamp(position, 0, maximum);
    if (next == state.scroll_position) {
        return;
    }
    state.scroll_position = next;
    LayoutButtons(dialog, state);
    InvalidateRect(dialog, nullptr, TRUE);
}

void EnsureButtonVisible(
    HWND dialog,
    ToolPaletteDialogState& state,
    HWND button) noexcept {
    RECT page_bounds{};
    RECT bounds{};
    if (button == nullptr
        || !PalettePageBounds(
            dialog, GetDpiForWindow(dialog), false, page_bounds)
        || GetWindowRect(button, &bounds) == FALSE) {
        return;
    }
    MapWindowPoints(nullptr, dialog, reinterpret_cast<POINT*>(&bounds), 2);
    if (bounds.top < page_bounds.top) {
        SetScrollPosition(
            dialog,
            state,
            state.scroll_position + bounds.top - page_bounds.top);
    } else if (bounds.bottom > page_bounds.bottom) {
        SetScrollPosition(
            dialog,
            state,
            state.scroll_position + bounds.bottom - page_bounds.bottom);
    }
}

bool InitializeTabs(HWND dialog, ToolPaletteDialogState& state) noexcept {
    const HWND tabs = GetDlgItem(dialog, IDC_TOOL_PALETTE_TAB);
    if (tabs == nullptr) {
        return false;
    }
    for (std::size_t index = 0; index < kToolPalettePageLabels.size(); ++index) {
        TCITEMW item{};
        item.mask = TCIF_TEXT;
        item.pszText = const_cast<wchar_t*>(kToolPalettePageLabels[index]);
        if (TabCtrl_InsertItem(tabs, static_cast<int>(index), &item) < 0) {
            return false;
        }
    }
    TabCtrl_SetCurSel(tabs, 0);
    if (TabCtrl_GetCurSel(tabs) != 0) {
        return false;
    }
    state.active_page = ToolPalettePage::Basic;
    state.scroll_position = 0;
    return true;
}

bool CreateButtons(HWND dialog) noexcept {
    const HINSTANCE instance = reinterpret_cast<HINSTANCE>(
        GetWindowLongPtrW(dialog, GWLP_HINSTANCE));
    const HFONT font = reinterpret_cast<HFONT>(
        SendMessageW(dialog, WM_GETFONT, 0, 0));
    for (const auto& entry : kToolPaletteEntries) {
        const HWND button = CreateWindowExW(
            0,
            L"BUTTON",
            entry.label,
            WS_CHILD | WS_TABSTOP | WS_CLIPSIBLINGS | BS_CHECKBOX
                | BS_PUSHLIKE | BS_LEFT | BS_VCENTER | BS_NOTIFY,
            0,
            0,
            0,
            0,
            dialog,
            reinterpret_cast<HMENU>(static_cast<INT_PTR>(entry.command)),
            instance,
            nullptr);
        if (button == nullptr) {
            return false;
        }
        SendMessageW(
            button,
            WM_SETFONT,
            reinterpret_cast<WPARAM>(font),
            FALSE);
    }
    return true;
}

bool UpdatePaletteFont(
    HWND dialog,
    ToolPaletteDialogState& state) noexcept {
    const int height = -MulDiv(
        kFontSizePoints,
        static_cast<int>(GetDpiForWindow(dialog)),
        72);
    const HFONT replacement = CreateFontW(
        height,
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
        return false;
    }
    SendMessageW(
        dialog,
        WM_SETFONT,
        reinterpret_cast<WPARAM>(replacement),
        FALSE);
    SendDlgItemMessageW(
        dialog,
        IDC_TOOL_PALETTE_TAB,
        WM_SETFONT,
        reinterpret_cast<WPARAM>(replacement),
        FALSE);
    for (const auto& entry : kToolPaletteEntries) {
        SendDlgItemMessageW(
            dialog,
            entry.command,
            WM_SETFONT,
            reinterpret_cast<WPARAM>(replacement),
            FALSE);
    }
    if (state.font != nullptr) {
        DeleteObject(state.font);
    }
    state.font = replacement;
    return true;
}

void NotifyVisibilityChanged(ToolPaletteDialogState& state) noexcept {
    if (state.visibility_changed != nullptr) {
        state.visibility_changed(state.context);
    }
}

void HidePalette(HWND dialog, ToolPaletteDialogState& state) noexcept {
    SetPaletteWindowShown(dialog, false);
    NotifyVisibilityChanged(state);
}

INT_PTR CALLBACK ToolPaletteDialogProcedure(
    HWND dialog,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    ToolPaletteDialogState* state = DialogState(dialog);
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<ToolPaletteDialogState*>(lparam);
            if (state == nullptr || state->dispatch_command == nullptr
                || state->visibility_changed == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog,
                GWLP_USERDATA,
                reinterpret_cast<LONG_PTR>(state));
            if (!InitializeTabs(dialog, *state) || !CreateButtons(dialog)
                || !UpdatePaletteFont(dialog, *state)) {
                DestroyWindow(dialog);
                return TRUE;
            }
            LayoutButtons(dialog, *state);
            return TRUE;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                HidePalette(dialog, *state);
                return TRUE;
            }
            if (HIWORD(wparam) == BN_CLICKED) {
                state->dispatch_command(state->context, LOWORD(wparam));
                return TRUE;
            }
            if (HIWORD(wparam) == BN_SETFOCUS) {
                EnsureButtonVisible(
                    dialog,
                    *state,
                    reinterpret_cast<HWND>(lparam));
                return TRUE;
            }
            break;
        case WM_NOTIFY:
            if (state != nullptr) {
                const auto* notification =
                    reinterpret_cast<const NMHDR*>(lparam);
                if (notification != nullptr
                    && notification->idFrom == IDC_TOOL_PALETTE_TAB
                    && notification->code == TCN_SELCHANGE) {
                    const int selection =
                        TabCtrl_GetCurSel(notification->hwndFrom);
                    if (selection >= 0
                        && selection < static_cast<int>(kToolPalettePageCount)) {
                        state->active_page =
                            static_cast<ToolPalettePage>(selection);
                        state->scroll_position = 0;
                        LayoutButtons(dialog, *state);
                    }
                    return TRUE;
                }
            }
            break;
        case WM_SIZE:
            if (state != nullptr) {
                LayoutButtons(dialog, *state);
            }
            return TRUE;
        case WM_VSCROLL:
            if (state != nullptr) {
                SCROLLINFO scroll{};
                scroll.cbSize = sizeof(scroll);
                scroll.fMask = SIF_ALL;
                GetScrollInfo(dialog, SB_VERT, &scroll);
                int next = state->scroll_position;
                switch (LOWORD(wparam)) {
                    case SB_LINEUP:
                        next -= ScaleForDpi(
                            kButtonHeight,
                            GetDpiForWindow(dialog));
                        break;
                    case SB_LINEDOWN:
                        next += ScaleForDpi(
                            kButtonHeight,
                            GetDpiForWindow(dialog));
                        break;
                    case SB_PAGEUP: next -= static_cast<int>(scroll.nPage); break;
                    case SB_PAGEDOWN: next += static_cast<int>(scroll.nPage); break;
                    case SB_THUMBPOSITION:
                    case SB_THUMBTRACK: next = scroll.nTrackPos; break;
                    case SB_TOP: next = scroll.nMin; break;
                    case SB_BOTTOM: next = scroll.nMax; break;
                    default: return TRUE;
                }
                SetScrollPosition(dialog, *state, next);
            }
            return TRUE;
        case WM_MOUSEWHEEL:
            if (state != nullptr) {
                const int delta = GET_WHEEL_DELTA_WPARAM(wparam);
                const int line = ScaleForDpi(
                    kButtonHeight + kButtonGap,
                    GetDpiForWindow(dialog));
                SetScrollPosition(
                    dialog,
                    *state,
                    state->scroll_position - (delta / WHEEL_DELTA) * line * 3);
            }
            return TRUE;
        case WM_DPICHANGED: {
            const auto* bounds = reinterpret_cast<const RECT*>(lparam);
            if (bounds != nullptr) {
                SetWindowPos(
                    dialog,
                    nullptr,
                    bounds->left,
                    bounds->top,
                    bounds->right - bounds->left,
                    bounds->bottom - bounds->top,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
            }
            if (state != nullptr) {
                UpdatePaletteFont(dialog, *state);
                LayoutButtons(dialog, *state);
            }
            return TRUE;
        }
        case WM_GETMINMAXINFO: {
            auto* limits = reinterpret_cast<MINMAXINFO*>(lparam);
            if (limits != nullptr) {
                const UINT dpi = GetDpiForWindow(dialog);
                limits->ptMinTrackSize.x = ScaleForDpi(kMinimumWidth, dpi);
                limits->ptMinTrackSize.y = ScaleForDpi(kMinimumHeight, dpi);
            }
            return TRUE;
        }
        case WM_CLOSE:
            if (state != nullptr) {
                HidePalette(dialog, *state);
            }
            return TRUE;
        case WM_NCDESTROY:
            if (state != nullptr && state->font != nullptr) {
                DeleteObject(state->font);
                state->font = nullptr;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

const std::array<ToolPaletteEntry, kToolPaletteEntryCount>&
ToolPaletteEntries() noexcept {
    return kToolPaletteEntries;
}

HWND CreateToolPaletteDialog(
    HINSTANCE instance,
    HWND owner,
    ToolPaletteDialogState& state) noexcept {
    return CreateDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_TOOL_PALETTE),
        owner,
        ToolPaletteDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

void UpdateToolPaletteDialog(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    if (dialog == nullptr) {
        return;
    }
    for (const auto& entry : kToolPaletteEntries) {
        const HWND button = GetDlgItem(dialog, entry.command);
        const CommandState* state = FindCommandState(states, entry.command);
        if (button == nullptr || state == nullptr) {
            continue;
        }
        EnableWindow(button, state->enabled ? TRUE : FALSE);
        SendMessageW(
            button,
            BM_SETCHECK,
            state->checked ? BST_CHECKED : BST_UNCHECKED,
            0);
    }
}

bool ToolPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    if (dialog == nullptr) {
        return false;
    }
    for (const auto& entry : kToolPaletteEntries) {
        const HWND button = GetDlgItem(dialog, entry.command);
        const CommandState* state = FindCommandState(states, entry.command);
        if (button == nullptr || state == nullptr
            || (IsWindowEnabled(button) != FALSE) != state->enabled
            || (SendMessageW(button, BM_GETCHECK, 0, 0) == BST_CHECKED)
                != state->checked) {
            return false;
        }
    }
    return true;
}

}  // namespace inkpod::windows::ui
