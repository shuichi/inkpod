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
constexpr int kButtonWidthDip = 64;
constexpr int kButtonHeightDip = 34;
constexpr int kGapDip = 3;
constexpr int kExpandWidthDip = 20;
constexpr UINT_PTR kToolButtonSubclass = 1U;
constexpr LONG_PTR kButtonChecked = 1;
constexpr LONG_PTR kButtonHovered = 2;

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

std::size_t EntryIndexForCommand(UINT command) noexcept {
    const auto found = std::find_if(
        kToolPaletteEntries.begin(),
        kToolPaletteEntries.end(),
        [command](const ToolPaletteEntry& entry) {
            return entry.command == command;
        });
    return found == kToolPaletteEntries.end()
        ? kToolPaletteEntries.size()
        : static_cast<std::size_t>(found - kToolPaletteEntries.begin());
}

UINT ExpandControlId(std::size_t index) noexcept {
    return IDC_TOOL_OPTIONS_EXPAND_FIRST + static_cast<UINT>(index);
}

const ToolPaletteEntry* EntryForExpandControl(UINT control) noexcept {
    if (control < IDC_TOOL_OPTIONS_EXPAND_FIRST
        || control > IDC_TOOL_OPTIONS_EXPAND_LAST) {
        return nullptr;
    }
    const std::size_t index = static_cast<std::size_t>(
        control - IDC_TOOL_OPTIONS_EXPAND_FIRST);
    return index < kToolPaletteEntries.size()
        ? &kToolPaletteEntries[index]
        : nullptr;
}

LRESULT CALLBACK ToolButtonSubclassProcedure(
    HWND button,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR) noexcept {
    LONG_PTR flags = GetWindowLongPtrW(button, GWLP_USERDATA);
    switch (message) {
        case WM_MOUSEMOVE:
            if ((flags & kButtonHovered) == 0) {
                TRACKMOUSEEVENT tracking{};
                tracking.cbSize = sizeof(tracking);
                tracking.dwFlags = TME_LEAVE;
                tracking.hwndTrack = button;
                if (TrackMouseEvent(&tracking) != FALSE) {
                    SetWindowLongPtrW(
                        button, GWLP_USERDATA, flags | kButtonHovered);
                    InvalidateRect(button, nullptr, TRUE);
                }
            }
            break;
        case WM_MOUSELEAVE:
            SetWindowLongPtrW(
                button, GWLP_USERDATA, flags & ~kButtonHovered);
            InvalidateRect(button, nullptr, TRUE);
            return 0;
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                button, ToolButtonSubclassProcedure, kToolButtonSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(button, message, wparam, lparam);
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
    const int expand_width = std::min(
        button_width, ScaleForDpi(kExpandWidthDip, dpi));
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
    for (std::size_t index = 0U; index < kToolPaletteEntries.size(); ++index) {
        const auto& entry = kToolPaletteEntries[index];
        const HWND control = GetDlgItem(dialog, static_cast<int>(entry.command));
        if (control != nullptr) {
            SetWindowPos(
                control,
                nullptr,
                x,
                y,
                std::max(0, actual_width - expand_width),
                button_height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
        }
        const HWND expand = GetDlgItem(
            dialog, static_cast<int>(ExpandControlId(index)));
        if (expand != nullptr) {
            SetWindowPos(
                expand,
                nullptr,
                x + std::max(0, actual_width - expand_width),
                y,
                std::min(expand_width, actual_width),
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
    for (std::size_t index = 0U; index < kToolPaletteEntries.size(); ++index) {
        const auto& entry = kToolPaletteEntries[index];
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
        if (SetWindowSubclass(
                button,
                ToolButtonSubclassProcedure,
                kToolButtonSubclass,
                0U) == FALSE) {
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
        const HWND expand = CreateWindowExW(
            0,
            L"BUTTON",
            UiText(UiStringId::ToolDetails),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
            0,
            0,
            0,
            0,
            dialog,
            reinterpret_cast<HMENU>(
                static_cast<INT_PTR>(ExpandControlId(index))),
            instance,
            nullptr);
        if (expand == nullptr
            || SetWindowSubclass(
                   expand,
                   ToolButtonSubclassProcedure,
                   kToolButtonSubclass,
                   0U) == FALSE) {
            return false;
        }
        TOOLINFOW expand_tool{};
        expand_tool.cbSize = sizeof(expand_tool);
        expand_tool.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
        expand_tool.hwnd = dialog;
        expand_tool.uId = reinterpret_cast<UINT_PTR>(expand);
        expand_tool.lpszText = const_cast<wchar_t*>(
            UiText(UiStringId::ToolDetails));
        SendMessageW(
            state.tooltip,
            TTM_ADDTOOLW,
            0,
            reinterpret_cast<LPARAM>(&expand_tool));
    }
    return true;
}

void DrawToolButton(const DRAWITEMSTRUCT& draw, bool expand) noexcept {
    const ToolPaletteEntry* entry = expand
        ? EntryForExpandControl(draw.CtlID)
        : EntryForCommand(draw.CtlID);
    if (entry == nullptr) {
        return;
    }
    const bool disabled = (draw.itemState & ODS_DISABLED) != 0U;
    const bool pressed = (draw.itemState & ODS_SELECTED) != 0U;
    const LONG_PTR flags = GetWindowLongPtrW(draw.hwndItem, GWLP_USERDATA);
    const bool checked = (flags & kButtonChecked) != 0;
    const bool hovered = (flags & kButtonHovered) != 0;
    const int background = checked || pressed
        ? COLOR_HIGHLIGHT
        : (hovered ? COLOR_3DLIGHT : COLOR_BTNFACE);
    const int foreground = disabled
        ? COLOR_GRAYTEXT
        : (checked || pressed
               ? COLOR_HIGHLIGHTTEXT
               : (expand && !hovered ? COLOR_GRAYTEXT : COLOR_BTNTEXT));
    FillRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(background));
    SetBkMode(draw.hDC, TRANSPARENT);
    SetTextColor(draw.hDC, GetSysColor(foreground));
    if (expand) {
        const int center_x = (draw.rcItem.left + draw.rcItem.right) / 2;
        const int center_y = (draw.rcItem.top + draw.rcItem.bottom) / 2;
        const UINT dpi = GetDpiForWindow(draw.hwndItem);
        const int horizontal_radius = std::max(2, ScaleForDpi(2, dpi));
        const int vertical_radius = std::max(2, ScaleForDpi(3, dpi));
        const HPEN pen = CreatePen(
            PS_SOLID,
            std::max(1, ScaleForDpi(1, dpi)),
            GetSysColor(foreground));
        if (pen != nullptr) {
            const HGDIOBJ previous = SelectObject(draw.hDC, pen);
            MoveToEx(
                draw.hDC,
                center_x - horizontal_radius,
                center_y - vertical_radius,
                nullptr);
            LineTo(draw.hDC, center_x + horizontal_radius, center_y);
            LineTo(
                draw.hDC,
                center_x - horizontal_radius,
                center_y + vertical_radius);
            if (previous != nullptr) {
                SelectObject(draw.hDC, previous);
            }
            DeleteObject(pen);
        }
    } else {
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
                || state->request_options == nullptr
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
            if (state != nullptr && HIWORD(wparam) == BN_CLICKED) {
                const ToolPaletteEntry* entry = EntryForExpandControl(
                    LOWORD(wparam));
                if (entry != nullptr) {
                    state->request_options(
                        state->context,
                        entry->command,
                        reinterpret_cast<HWND>(lparam));
                    return TRUE;
                }
            }
            break;
        case WM_DRAWITEM:
            if (EntryForCommand(static_cast<UINT>(wparam)) != nullptr
                || EntryForExpandControl(static_cast<UINT>(wparam)) != nullptr) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawToolButton(
                        *draw,
                        EntryForExpandControl(static_cast<UINT>(wparam))
                            != nullptr);
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
    for (std::size_t index = 0U; index < kToolPaletteEntries.size(); ++index) {
        const auto& entry = kToolPaletteEntries[index];
        const CommandState* state = FindCommandState(states, entry.command);
        const HWND control = GetDlgItem(dialog, static_cast<int>(entry.command));
        if (state != nullptr && control != nullptr) {
            EnableWindow(control, state->enabled ? TRUE : FALSE);
            const LONG_PTR main_flags = GetWindowLongPtrW(
                control, GWLP_USERDATA);
            SetWindowLongPtrW(
                control,
                GWLP_USERDATA,
                (main_flags & ~kButtonChecked)
                    | (state->checked ? kButtonChecked : 0));
            InvalidateRect(control, nullptr, TRUE);
        }
        const HWND expand = GetDlgItem(
            dialog, static_cast<int>(ExpandControlId(index)));
        if (state != nullptr && expand != nullptr) {
            EnableWindow(expand, state->enabled ? TRUE : FALSE);
            const LONG_PTR expand_flags = GetWindowLongPtrW(
                expand, GWLP_USERDATA);
            SetWindowLongPtrW(
                expand,
                GWLP_USERDATA,
                (expand_flags & ~kButtonChecked)
                    | (state->checked ? kButtonChecked : 0));
            InvalidateRect(expand, nullptr, TRUE);
        }
    }
}

bool ToolPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    if (dialog == nullptr) {
        return false;
    }
    for (std::size_t index = 0U; index < kToolPaletteEntries.size(); ++index) {
        const auto& entry = kToolPaletteEntries[index];
        const CommandState* state = FindCommandState(states, entry.command);
        const HWND control = GetDlgItem(dialog, static_cast<int>(entry.command));
        const HWND expand = GetDlgItem(
            dialog, static_cast<int>(ExpandControlId(index)));
        if (state == nullptr || control == nullptr
            || expand == nullptr
            || (IsWindowEnabled(control) != FALSE) != state->enabled
            || (IsWindowEnabled(expand) != FALSE) != state->enabled
            || ((GetWindowLongPtrW(control, GWLP_USERDATA)
                    & kButtonChecked) != 0)
                != state->checked
            || ((GetWindowLongPtrW(expand, GWLP_USERDATA)
                    & kButtonChecked) != 0)
                != state->checked) {
            return false;
        }
    }
    return true;
}

HWND ToolPaletteCheckedOptionsAnchor(HWND dialog) noexcept {
    if (dialog == nullptr) {
        return nullptr;
    }
    for (std::size_t index = 0U; index < kToolPaletteEntries.size(); ++index) {
        const HWND main = GetDlgItem(
            dialog, static_cast<int>(kToolPaletteEntries[index].command));
        if (main != nullptr
            && (GetWindowLongPtrW(main, GWLP_USERDATA) & kButtonChecked) != 0) {
            return GetDlgItem(
                dialog, static_cast<int>(ExpandControlId(index)));
        }
    }
    return nullptr;
}

bool ToolPaletteCommandHasOptions(UINT command) noexcept {
    return EntryIndexForCommand(command) < kToolPaletteEntries.size();
}

}  // namespace inkpod::windows::ui
