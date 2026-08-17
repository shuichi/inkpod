#include "ui/ui_resources.h"

#include "tool_palette.h"

#include <commctrl.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstddef>

#include "app/resource.h"

namespace inkpod::windows::ui {
namespace {

constexpr std::array<ToolPaletteEntry, kToolPaletteEntryCount>
    kToolPaletteEntries{{
        {IDM_TOOL_PENCIL, UiStringId::ToolPencil, UiStringId::ToolPencil,
         ToolIconId::Pencil},
        {IDM_TOOL_BRUSH, UiStringId::ToolBrush, UiStringId::ToolBrush,
         ToolIconId::Brush},
        {IDM_TOOL_ERASER, UiStringId::ToolEraser, UiStringId::ToolEraser,
         ToolIconId::Eraser},
        {IDM_TOOL_FILL, UiStringId::ToolFill, UiStringId::ToolFill,
         ToolIconId::Fill},
        {IDM_TOOL_CLOSED_FILL, UiStringId::ToolClosedRegionFill,
         UiStringId::ToolClosedRegionFillCompact, ToolIconId::ClosedRegionFill},
        {IDM_TOOL_FILL_EXTENSION, UiStringId::ToolFillExtension,
         UiStringId::ToolFillExtensionCompact, ToolIconId::FillExtension},
        {IDM_TOOL_EYEDROPPER, UiStringId::ToolEyedropper,
         UiStringId::ToolEyedropper, ToolIconId::Eyedropper},
        {IDM_VECTOR_LINE, UiStringId::ToolVectorLine, UiStringId::ToolVectorLine,
         ToolIconId::VectorLine},
        {IDM_VECTOR_CURVE, UiStringId::ToolVectorCurve,
         UiStringId::ToolVectorCurve, ToolIconId::VectorCurve},
        {IDM_VECTOR_RECTANGLE, UiStringId::ToolVectorRectangle,
         UiStringId::ToolVectorRectangle, ToolIconId::VectorRectangle},
        {IDM_VECTOR_ELLIPSE, UiStringId::ToolVectorEllipse,
         UiStringId::ToolVectorEllipse, ToolIconId::VectorEllipse},
        {IDM_VECTOR_POLYLINE, UiStringId::ToolVectorPolyline,
         UiStringId::ToolVectorPolyline, ToolIconId::VectorPolyline},
        {IDM_VECTOR_ERASER, UiStringId::ToolVectorEraser,
         UiStringId::ToolVectorEraserCompact, ToolIconId::VectorEraser},
        {IDM_EFFECT_GRADIENT, UiStringId::ToolGradient, UiStringId::ToolGradient,
         ToolIconId::Gradient},
        {IDM_EFFECT_AIRBRUSH, UiStringId::ToolAirbrush, UiStringId::ToolAirbrush,
         ToolIconId::Airbrush},
        {IDM_EFFECT_BOUNDARY_AIRBRUSH, UiStringId::ToolBoundaryAirbrush,
         UiStringId::ToolBoundaryAirbrushCompact, ToolIconId::BoundaryAirbrush},
        {IDM_EFFECT_BLUR, UiStringId::ToolBlur, UiStringId::ToolBlur,
         ToolIconId::Blur},
        {IDM_EFFECT_STAMP, UiStringId::ToolStamp, UiStringId::ToolStamp,
         ToolIconId::Stamp},
        {IDM_EFFECT_DUST, UiStringId::ToolDustRemoval,
         UiStringId::ToolDustRemovalCompact, ToolIconId::DustRemoval},
        {IDM_EFFECT_ALPHA_GRADIENT, UiStringId::ToolAlphaGradient,
         UiStringId::ToolAlphaGradientCompact, ToolIconId::AlphaGradient},
    }};

constexpr int kReferenceDpi = 96;
constexpr int kMarginDip = 4;
constexpr int kButtonWidthDip = 72;
constexpr int kButtonHeightDip = 34;
constexpr int kGapDip = 3;

int ScaleForDpi(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? kReferenceDpi : dpi), kReferenceDpi);
}

ToolPaletteDialogState* DialogState(HWND dialog) noexcept {
    return reinterpret_cast<ToolPaletteDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
}

const ToolPaletteEntry* EntryForCommand(UINT command) noexcept {
    const auto found = std::find_if(
        kToolPaletteEntries.begin(),
        kToolPaletteEntries.end(),
        [command](const ToolPaletteEntry& entry) {
            return entry.command == command;
        });
    return found == kToolPaletteEntries.end() ? nullptr : &*found;
}

void SetScrollPosition(
    HWND dialog,
    ToolPaletteDialogState& state,
    int requested) noexcept {
    SCROLLINFO info{};
    info.cbSize = sizeof(info);
    info.fMask = SIF_RANGE | SIF_PAGE;
    GetScrollInfo(dialog, SB_VERT, &info);
    const int maximum = std::max(
        0, info.nMax - static_cast<int>(info.nPage) + 1);
    state.scroll_position = std::clamp(requested, 0, maximum);
    SetScrollPos(dialog, SB_VERT, state.scroll_position, TRUE);
}

void LayoutButtons(HWND dialog, ToolPaletteDialogState& state) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(dialog);
    const int margin = ScaleForDpi(kMarginDip, dpi);
    const int button_width = ScaleForDpi(kButtonWidthDip, dpi);
    const int button_height = ScaleForDpi(kButtonHeightDip, dpi);
    const int gap = ScaleForDpi(kGapDip, dpi);
    const int viewport = std::max(
        0, static_cast<int>(client.bottom) - margin * 2);
    const int content = static_cast<int>(kToolPaletteEntries.size())
        * (button_height + gap) - gap;
    SCROLLINFO scroll{};
    scroll.cbSize = sizeof(scroll);
    scroll.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
    scroll.nMin = 0;
    scroll.nMax = std::max(0, content - 1);
    scroll.nPage = static_cast<UINT>(viewport);
    scroll.nPos = state.scroll_position;
    SetScrollInfo(dialog, SB_VERT, &scroll, TRUE);
    SetScrollPosition(dialog, state, state.scroll_position);

    GetClientRect(dialog, &client);
    const int available_width = std::max(
        0, static_cast<int>(client.right) - margin * 2);
    const int actual_width = std::min(button_width, available_width);
    const int x = margin + std::max(0, (available_width - actual_width) / 2);
    int y = margin - state.scroll_position;
    for (const auto& entry : kToolPaletteEntries) {
        const HWND control = GetDlgItem(dialog, static_cast<int>(entry.command));
        if (control != nullptr) {
            SetWindowPos(
                control,
                nullptr,
                x,
                y,
                actual_width,
                button_height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
        }
        y += button_height + gap;
    }
}

bool UpdatePaletteFont(HWND dialog, ToolPaletteDialogState& state) noexcept {
    const int height = -MulDiv(
        7, static_cast<int>(GetDpiForWindow(dialog)), 72);
    const HFONT replacement = CreateFontW(
        height,
        0,
        0,
        0,
        FW_SEMIBOLD,
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
    for (const auto& entry : kToolPaletteEntries) {
        SendDlgItemMessageW(
            dialog,
            static_cast<int>(entry.command),
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

bool CreateButtons(HWND dialog, ToolPaletteDialogState& state) noexcept {
    const HINSTANCE instance = reinterpret_cast<HINSTANCE>(
        GetWindowLongPtrW(dialog, GWLP_HINSTANCE));
    state.tooltip = CreateWindowExW(
        WS_EX_TOPMOST,
        TOOLTIPS_CLASSW,
        nullptr,
        WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        dialog,
        nullptr,
        instance,
        nullptr);
    if (state.tooltip == nullptr) {
        return false;
    }
    for (const auto& entry : kToolPaletteEntries) {
        const wchar_t* label = UiText(entry.label);
        const HWND button = CreateWindowExW(
            0,
            L"BUTTON",
            label,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
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
        TOOLINFOW tool{};
        tool.cbSize = sizeof(tool);
        tool.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
        tool.hwnd = dialog;
        tool.uId = reinterpret_cast<UINT_PTR>(button);
        tool.lpszText = const_cast<wchar_t*>(label);
        SendMessageW(
            state.tooltip,
            TTM_ADDTOOLW,
            0,
            reinterpret_cast<LPARAM>(&tool));
    }
    return true;
}

void DrawToolButton(const DRAWITEMSTRUCT& draw) noexcept {
    const ToolPaletteEntry* entry = EntryForCommand(draw.CtlID);
    if (entry == nullptr) {
        return;
    }
    const bool disabled = (draw.itemState & ODS_DISABLED) != 0U;
    const bool pressed = (draw.itemState & ODS_SELECTED) != 0U;
    const bool checked = GetWindowLongPtrW(draw.hwndItem, GWLP_USERDATA) != 0;
    const int background = checked || pressed ? COLOR_HIGHLIGHT : COLOR_BTNFACE;
    const int foreground = disabled
        ? COLOR_GRAYTEXT
        : (checked || pressed ? COLOR_HIGHLIGHTTEXT : COLOR_BTNTEXT);
    FillRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(background));
    FrameRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(checked ? COLOR_HIGHLIGHT : COLOR_3DSHADOW));
    SetBkMode(draw.hDC, TRANSPARENT);
    SetTextColor(draw.hDC, GetSysColor(foreground));
    const int icon_size = ScaleForDpi(20, GetDpiForWindow(draw.hwndItem));
    RECT icon_bounds = draw.rcItem;
    icon_bounds.left += std::max(
        0, (static_cast<int>(draw.rcItem.right - draw.rcItem.left) - icon_size) / 2);
    icon_bounds.top += std::max(
        0, (static_cast<int>(draw.rcItem.bottom - draw.rcItem.top) - icon_size) / 2);
    icon_bounds.right = std::min(draw.rcItem.right, icon_bounds.left + icon_size);
    icon_bounds.bottom = std::min(draw.rcItem.bottom, icon_bounds.top + icon_size);
    const HINSTANCE instance = reinterpret_cast<HINSTANCE>(
        GetWindowLongPtrW(draw.hwndItem, GWLP_HINSTANCE));
    if (!DrawToolIcon(
            instance,
            draw.hDC,
            icon_bounds,
            entry->icon,
            GetSysColor(foreground))) {
        const HFONT font = reinterpret_cast<HFONT>(
            SendMessageW(draw.hwndItem, WM_GETFONT, 0, 0));
        const HGDIOBJ previous =
            font == nullptr ? nullptr : SelectObject(draw.hDC, font);
        RECT text = draw.rcItem;
        DrawTextW(
            draw.hDC,
            UiText(entry->fallback_label),
            -1,
            &text,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        if (previous != nullptr) {
            SelectObject(draw.hDC, previous);
        }
    }
    if ((draw.itemState & ODS_FOCUS) != 0U) {
        RECT focus = draw.rcItem;
        InflateRect(&focus, -3, -3);
        DrawFocusRect(draw.hDC, &focus);
    }
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
                return FALSE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            if (!CreateButtons(dialog, *state)
                || !UpdatePaletteFont(dialog, *state)) {
                return FALSE;
            }
            LayoutButtons(dialog, *state);
            return TRUE;
        case WM_COMMAND:
            if (state != nullptr && HIWORD(wparam) == BN_CLICKED
                && EntryForCommand(LOWORD(wparam)) != nullptr) {
                state->dispatch_command(state->context, LOWORD(wparam));
                return TRUE;
            }
            break;
        case WM_DRAWITEM:
            if (EntryForCommand(static_cast<UINT>(wparam)) != nullptr) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawToolButton(*draw);
                }
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
                SCROLLINFO info{};
                info.cbSize = sizeof(info);
                info.fMask = SIF_ALL;
                GetScrollInfo(dialog, SB_VERT, &info);
                int requested = state->scroll_position;
                switch (LOWORD(wparam)) {
                    case SB_LINEUP: requested -= ScaleForDpi(18, GetDpiForWindow(dialog)); break;
                    case SB_LINEDOWN: requested += ScaleForDpi(18, GetDpiForWindow(dialog)); break;
                    case SB_PAGEUP: requested -= static_cast<int>(info.nPage); break;
                    case SB_PAGEDOWN: requested += static_cast<int>(info.nPage); break;
                    case SB_THUMBTRACK: requested = info.nTrackPos; break;
                    default: return TRUE;
                }
                SetScrollPosition(dialog, *state, requested);
                LayoutButtons(dialog, *state);
            }
            return TRUE;
        case WM_MOUSEWHEEL:
            if (state != nullptr) {
                const int delta = GET_WHEEL_DELTA_WPARAM(wparam);
                SetScrollPosition(
                    dialog,
                    *state,
                    state->scroll_position
                        - (delta / WHEEL_DELTA)
                            * ScaleForDpi(36, GetDpiForWindow(dialog)));
                LayoutButtons(dialog, *state);
            }
            return TRUE;
        case WM_DPICHANGED_AFTERPARENT:
            if (state != nullptr) {
                UpdatePaletteFont(dialog, *state);
                LayoutButtons(dialog, *state);
            }
            return TRUE;
        case WM_NCDESTROY:
            if (state != nullptr && state->font != nullptr) {
                DeleteObject(state->font);
                state->font = nullptr;
                state->tooltip = nullptr;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            break;
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
    return CreateLocalizedDialogParamW(
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
        const CommandState* state = FindCommandState(states, entry.command);
        const HWND control = GetDlgItem(dialog, static_cast<int>(entry.command));
        if (state != nullptr && control != nullptr) {
            EnableWindow(control, state->enabled ? TRUE : FALSE);
            SetWindowLongPtrW(
                control, GWLP_USERDATA, state->checked ? 1 : 0);
            InvalidateRect(control, nullptr, TRUE);
        }
    }
}

bool ToolPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    if (dialog == nullptr) {
        return false;
    }
    for (const auto& entry : kToolPaletteEntries) {
        const CommandState* state = FindCommandState(states, entry.command);
        const HWND control = GetDlgItem(dialog, static_cast<int>(entry.command));
        if (state == nullptr || control == nullptr
            || (IsWindowEnabled(control) != FALSE) != state->enabled
            || (GetWindowLongPtrW(control, GWLP_USERDATA) != 0)
                != state->checked) {
            return false;
        }
    }
    return true;
}

}  // namespace inkpod::windows::ui
