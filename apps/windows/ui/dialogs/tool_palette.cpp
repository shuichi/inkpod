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
        {IDM_TOOL_PENCIL, L"鉛筆"},
        {IDM_TOOL_BRUSH, L"ブラシ"},
        {IDM_TOOL_ERASER, L"消しゴム"},
        {IDM_TOOL_FILL, L"フィル"},
        {IDM_TOOL_CLOSED_FILL, L"閉領域フィル"},
        {IDM_TOOL_FILL_EXTENSION, L"塗りのばし"},
        {IDM_TOOL_FILL_OPTIONS, L"フィル設定..."},
        {IDM_TOOL_EYEDROPPER, L"スポイト"},
        {IDM_VECTOR_LINE, L"ベクター描画: 直線"},
        {IDM_VECTOR_CURVE, L"ベクター描画: 曲線"},
        {IDM_VECTOR_RECTANGLE, L"ベクター描画: 長方形"},
        {IDM_VECTOR_ELLIPSE, L"ベクター描画: 楕円"},
        {IDM_VECTOR_POLYLINE, L"ベクター描画: 折れ線"},
        {IDM_VECTOR_ERASER, L"ベクター描画: 消しゴム"},
        {IDM_VECTOR_ERASE_PARTIAL, L"ベクター消去: 触れた部分"},
        {IDM_VECTOR_ERASE_INTERSECTION, L"ベクター消去: 交点まで"},
        {IDM_VECTOR_ERASE_WHOLE, L"ベクター消去: 線全体"},
        {IDM_VECTOR_CONNECT, L"ベクター: 線つなぎ..."},
        {IDM_VECTOR_WIDTH, L"ベクター: 線幅修正..."},
        {IDM_VECTOR_SELECT_CUT, L"ベクター選択: 選択範囲で切断"},
        {IDM_VECTOR_SELECT_TOUCH, L"ベクター選択: 一部でも触れる線"},
        {IDM_VECTOR_SELECT_CONTAINED, L"ベクター選択: 完全に含まれる線"},
        {IDM_VECTOR_SELECT_LINE, L"ベクター選択: 線を選択"},
        {IDM_VECTOR_SELECT_WHOLE_LINE, L"ベクター選択: 線全体"},
        {IDM_VECTOR_SELECT_INTERSECTION, L"ベクター選択: 交点まで"},
        {IDM_VECTOR_SELECT_FILL_BOUNDARY, L"ベクター選択: 塗りを囲む線"},
        {IDM_VECTOR_SELECT_FILL, L"ベクター選択: 塗り"},
        {IDM_VECTOR_RASTERIZE, L"ベクターをラスタライズ..."},
        {IDM_VECTOR_VECTORIZE, L"ラスターをベクター化..."},
        {IDM_EFFECT_GRADIENT, L"グラデーション..."},
        {IDM_EFFECT_AIRBRUSH, L"エアブラシ..."},
        {IDM_EFFECT_BOUNDARY_AIRBRUSH, L"境界色エアブラシ..."},
        {IDM_EFFECT_BLUR, L"ぼかしツール..."},
        {IDM_EFFECT_STAMP, L"スタンプ..."},
        {IDM_EFFECT_DUST, L"ゴミ取り..."},
        {IDM_EFFECT_ALPHA_GRADIENT, L"アルファグラデーション..."},
        {IDM_EFFECT_ALPHA_VIEW, L"アルファをグレースケール表示"},
    }};

constexpr int kReferenceDpi = 96;
constexpr int kMargin = 8;
constexpr int kButtonHeight = 26;
constexpr int kButtonGap = 4;
constexpr int kMinimumWidth = 220;
constexpr int kMinimumHeight = 180;

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

int ContentHeight(UINT dpi) noexcept {
    const int margin = ScaleForDpi(kMargin, dpi);
    const int height = ScaleForDpi(kButtonHeight, dpi);
    const int gap = ScaleForDpi(kButtonGap, dpi);
    return margin * 2
        + static_cast<int>(kToolPaletteEntries.size()) * height
        + static_cast<int>(kToolPaletteEntries.size() - 1U) * gap;
}

void LayoutButtons(HWND dialog, ToolPaletteDialogState& state) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(dialog);
    const int margin = ScaleForDpi(kMargin, dpi);
    const int button_height = ScaleForDpi(kButtonHeight, dpi);
    const int gap = ScaleForDpi(kButtonGap, dpi);
    const int page = std::max(
        0,
        static_cast<int>(client.bottom - client.top));
    const int maximum = std::max(0, ContentHeight(dpi) - page);
    state.scroll_position = std::clamp(state.scroll_position, 0, maximum);

    SCROLLINFO scroll{};
    scroll.cbSize = sizeof(scroll);
    scroll.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
    scroll.nMin = 0;
    scroll.nMax = std::max(0, ContentHeight(dpi) - 1);
    scroll.nPage = static_cast<UINT>(page);
    scroll.nPos = state.scroll_position;
    SetScrollInfo(dialog, SB_VERT, &scroll, TRUE);

    const int width = std::max(
        0,
        static_cast<int>(client.right - client.left) - margin * 2);
    for (std::size_t index = 0; index < kToolPaletteEntries.size(); ++index) {
        const HWND button = GetDlgItem(dialog, kToolPaletteEntries[index].command);
        if (button == nullptr) {
            continue;
        }
        const int y = margin
            + static_cast<int>(index) * (button_height + gap)
            - state.scroll_position;
        SetWindowPos(
            button,
            nullptr,
            margin,
            y,
            width,
            button_height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
    }
}

void SetScrollPosition(
    HWND dialog,
    ToolPaletteDialogState& state,
    int position) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const int maximum = std::max(
        0,
        ContentHeight(GetDpiForWindow(dialog))
            - static_cast<int>(client.bottom - client.top));
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
    RECT client{};
    RECT bounds{};
    if (button == nullptr || GetClientRect(dialog, &client) == FALSE
        || GetWindowRect(button, &bounds) == FALSE) {
        return;
    }
    MapWindowPoints(nullptr, dialog, reinterpret_cast<POINT*>(&bounds), 2);
    if (bounds.top < client.top) {
        SetScrollPosition(dialog, state, state.scroll_position + bounds.top - client.top);
    } else if (bounds.bottom > client.bottom) {
        SetScrollPosition(
            dialog,
            state,
            state.scroll_position + bounds.bottom - client.bottom);
    }
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
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_CHECKBOX | BS_PUSHLIKE
                | BS_LEFT | BS_VCENTER | BS_NOTIFY,
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
    const int height = -MulDiv(9, static_cast<int>(GetDpiForWindow(dialog)), 72);
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
            if (!CreateButtons(dialog) || !UpdatePaletteFont(dialog, *state)) {
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
