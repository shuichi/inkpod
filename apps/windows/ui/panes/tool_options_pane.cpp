#include "tool_options_pane.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cwchar>

#include "app/resource.h"
#include "inkpod/core_ffi.h"
#include "ui/tools/tool_state.h"

namespace inkpod::windows::ui::panes {
namespace {

constexpr UINT_PTR kPaneSubclass = 1U;

int ScaleForDpi(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

HFONT CreatePaneFont(HWND pane, int point_size) noexcept {
    const UINT dpi = GetDpiForWindow(pane);
    return CreateFontW(
        -MulDiv(point_size, static_cast<int>(dpi == 0U ? 96U : dpi), 72),
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
}

int EditControlHeight(HWND pane, HFONT font, UINT dpi) noexcept {
    const int fallback = ScaleForDpi(20, dpi);
    if (font == nullptr) {
        return fallback;
    }
    const HDC dc = GetDC(pane);
    if (dc == nullptr) {
        return fallback;
    }
    const HGDIOBJ previous = SelectObject(dc, font);
    TEXTMETRICW metrics{};
    const bool measured = previous != nullptr
        && GetTextMetricsW(dc, &metrics) != FALSE;
    if (previous != nullptr) {
        SelectObject(dc, previous);
    }
    ReleaseDC(pane, dc);
    if (!measured) {
        return fallback;
    }
    const UINT effective_dpi = dpi == 0U ? 96U : dpi;
    const int border = std::max(
        1, GetSystemMetricsForDpi(SM_CYBORDER, effective_dpi));
    const int padding = ScaleForDpi(2, dpi);
    return metrics.tmHeight + 2 * (border + padding);
}

const wchar_t* ToolLabel(std::uint32_t tool) noexcept {
    if (tool == INKPOD_TOOL_PENCIL) return L"鉛筆";
    if (tool == INKPOD_TOOL_BRUSH) return L"ブラシ";
    if (tool == INKPOD_TOOL_ERASER) return L"消しゴム";
    if (tool == tools::kInteractionFill) return L"フィル";
    if (tool == tools::kInteractionEyedropper) return L"スポイト";
    if (tool == tools::kInteractionVectorLine) return L"直線";
    if (tool == tools::kInteractionVectorCurve) return L"曲線";
    if (tool == tools::kInteractionVectorRectangle) return L"長方形";
    if (tool == tools::kInteractionVectorEllipse) return L"楕円";
    if (tool == tools::kInteractionVectorPolyline) return L"折れ線";
    if (tool == tools::kInteractionVectorEraser) return L"ベクター消しゴム";
    if (tool == tools::kInteractionEffectGradient) return L"グラデーション";
    if (tool == tools::kInteractionEffectAirbrush) return L"エアブラシ";
    if (tool == tools::kInteractionEffectBlur) return L"ぼかし";
    if (tool == tools::kInteractionEffectStamp) return L"スタンプ";
    if (tool == tools::kInteractionEffectDust) return L"ゴミ取り";
    if (tool == tools::kInteractionEffectAlphaGradient) return L"アルファグラデーション";
    return L"ツール";
}

UINT DetailsCommand(std::uint32_t tool) noexcept {
    if (tool == tools::kInteractionFill) return IDM_TOOL_FILL_OPTIONS;
    if (tool == tools::kInteractionEffectGradient) return IDM_EFFECT_GRADIENT;
    if (tool == tools::kInteractionEffectAirbrush) return IDM_EFFECT_AIRBRUSH;
    if (tool == tools::kInteractionEffectBlur) return IDM_EFFECT_BLUR;
    if (tool == tools::kInteractionEffectStamp) return IDM_EFFECT_STAMP;
    if (tool == tools::kInteractionEffectDust) return IDM_EFFECT_DUST;
    if (tool == tools::kInteractionEffectAlphaGradient) {
        return IDM_EFFECT_ALPHA_GRADIENT;
    }
    return 0U;
}

bool HasDiameter(std::uint32_t tool) noexcept {
    return tool == INKPOD_TOOL_PENCIL || tool == INKPOD_TOOL_BRUSH
        || tool == INKPOD_TOOL_ERASER;
}

bool CanEditDiameter(std::uint32_t tool) noexcept {
    return tool == INKPOD_TOOL_BRUSH || tool == INKPOD_TOOL_ERASER;
}

void LayoutPane(HWND pane) noexcept {
    RECT client{};
    if (GetClientRect(pane, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(pane);
    const int margin = ScaleForDpi(8, dpi);
    const int row = ScaleForDpi(24, dpi);
    auto* state = reinterpret_cast<ToolOptionsPaneState*>(
        GetWindowLongPtrW(pane, GWLP_USERDATA));
    const int edit_height = std::min(
        row,
        EditControlHeight(
            pane, state == nullptr ? nullptr : state->edit_font, dpi));
    const int tool_width = ScaleForDpi(150, dpi);
    const int target_label_width = ScaleForDpi(70, dpi);
    const int target_button_width = ScaleForDpi(58, dpi);
    const int diameter_label_width = ScaleForDpi(54, dpi);
    const int edit_width = ScaleForDpi(72, dpi);
    const int details_width = ScaleForDpi(78, dpi);
    int x = margin;
    const int y = std::max(
        0, (static_cast<int>(client.bottom) - row) / 2);
    const int edit_y = y + (row - edit_height) / 2;
    SetWindowPos(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_LABEL),
        nullptr,
        x,
        y,
        tool_width,
        row,
        SWP_NOACTIVATE | SWP_NOZORDER);
    x += tool_width + margin;
    if (state != nullptr && state->active_tool == INKPOD_TOOL_ERASER) {
        SetWindowPos(
            GetDlgItem(pane, IDC_TOOL_OPTIONS_TARGET_LABEL),
            nullptr,
            x,
            y,
            target_label_width,
            row,
            SWP_NOACTIVATE | SWP_NOZORDER);
        x += target_label_width;
        SetWindowPos(
            GetDlgItem(pane, IDC_TOOL_OPTIONS_TARGET_MAIN_LINE),
            nullptr,
            x,
            y,
            target_button_width,
            row,
            SWP_NOACTIVATE | SWP_NOZORDER);
        x += target_button_width;
        SetWindowPos(
            GetDlgItem(pane, IDC_TOOL_OPTIONS_TARGET_COLOR),
            nullptr,
            x,
            y,
            target_button_width,
            row,
            SWP_NOACTIVATE | SWP_NOZORDER);
        x += target_button_width + margin;
    }
    SetWindowPos(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DIAMETER_LABEL),
        nullptr,
        x,
        y,
        diameter_label_width,
        row,
        SWP_NOACTIVATE | SWP_NOZORDER);
    x += diameter_label_width;
    SetWindowPos(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DIAMETER),
        nullptr,
        x,
        edit_y,
        edit_width,
        edit_height,
        SWP_NOACTIVATE | SWP_NOZORDER);
    x += edit_width + margin;
    SetWindowPos(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DETAILS),
        nullptr,
        x,
        y,
        details_width,
        row,
        SWP_NOACTIVATE | SWP_NOZORDER);
}

void UpdateFont(HWND pane, ToolOptionsPaneState& state) noexcept {
    const HFONT replacement = CreatePaneFont(pane, 9);
    const HFONT edit_replacement = CreatePaneFont(pane, 8);
    if (replacement == nullptr || edit_replacement == nullptr) {
        if (replacement != nullptr) {
            DeleteObject(replacement);
        }
        if (edit_replacement != nullptr) {
            DeleteObject(edit_replacement);
        }
        return;
    }
    for (const int control : {
             IDC_TOOL_OPTIONS_LABEL,
             IDC_TOOL_OPTIONS_DIAMETER_LABEL,
             IDC_TOOL_OPTIONS_DETAILS,
             IDC_TOOL_OPTIONS_TARGET_LABEL,
             IDC_TOOL_OPTIONS_TARGET_MAIN_LINE,
             IDC_TOOL_OPTIONS_TARGET_COLOR}) {
        SendDlgItemMessageW(
            pane, control, WM_SETFONT, reinterpret_cast<WPARAM>(replacement), TRUE);
    }
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_DIAMETER,
        WM_SETFONT,
        reinterpret_cast<WPARAM>(edit_replacement),
        TRUE);
    if (state.font != nullptr) {
        DeleteObject(state.font);
    }
    if (state.edit_font != nullptr) {
        DeleteObject(state.edit_font);
    }
    state.font = replacement;
    state.edit_font = edit_replacement;
}

void CommitDiameter(HWND pane, ToolOptionsPaneState& state) noexcept {
    if (state.updating) {
        return;
    }
    if (!CanEditDiameter(state.active_tool) || state.change_diameter == nullptr) {
        UpdateToolOptionsPane(
            pane, state.active_tool, state.active_plane, state.diameter);
        return;
    }
    std::array<wchar_t, 64U> text{};
    GetDlgItemTextW(
        pane,
        IDC_TOOL_OPTIONS_DIAMETER,
        text.data(),
        static_cast<int>(text.size()));
    wchar_t* end{};
    const double value = std::wcstod(text.data(), &end);
    if (end != text.data() && *end == L'\0' && std::isfinite(value)
        && value >= static_cast<double>(kMinimumToolDiameter)
        && value <= static_cast<double>(kMaximumToolDiameter)) {
        state.change_diameter(state.context, static_cast<float>(value));
    }
    UpdateToolOptionsPane(
        pane, state.active_tool, state.active_plane, state.diameter);
}

LRESULT CALLBACK PaneSubclassProcedure(
    HWND pane,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<ToolOptionsPaneState*>(reference);
    switch (message) {
        case WM_SIZE:
            LayoutPane(pane);
            return 0;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDC_TOOL_OPTIONS_DIAMETER
                && HIWORD(wparam) == EN_SETFOCUS) {
                state->editing = true;
                return 0;
            }
            if (LOWORD(wparam) == IDC_TOOL_OPTIONS_DIAMETER
                && HIWORD(wparam) == EN_KILLFOCUS) {
                state->editing = false;
                CommitDiameter(pane, *state);
                return 0;
            }
            if (LOWORD(wparam) == IDC_TOOL_OPTIONS_DETAILS
                && HIWORD(wparam) == BN_CLICKED) {
                const UINT command = DetailsCommand(state->active_tool);
                if (command != 0U && state->dispatch_command != nullptr) {
                    state->dispatch_command(state->context, command);
                }
                return 0;
            }
            if ((LOWORD(wparam) == IDC_TOOL_OPTIONS_TARGET_MAIN_LINE
                    || LOWORD(wparam) == IDC_TOOL_OPTIONS_TARGET_COLOR)
                && HIWORD(wparam) == BN_CLICKED) {
                const UINT command = LOWORD(wparam) == IDC_TOOL_OPTIONS_TARGET_MAIN_LINE
                    ? IDM_PLANE_MAIN_LINE
                    : IDM_PLANE_COLOR;
                if (state->dispatch_command != nullptr) {
                    state->dispatch_command(state->context, command);
                }
                return 0;
            }
            break;
        case WM_DPICHANGED_AFTERPARENT:
            if (state != nullptr) {
                UpdateFont(pane, *state);
                LayoutPane(pane);
            }
            return 0;
        case WM_NCDESTROY:
            if (state != nullptr) {
                if (state->font != nullptr) {
                    DeleteObject(state->font);
                    state->font = nullptr;
                }
                if (state->edit_font != nullptr) {
                    DeleteObject(state->edit_font);
                    state->edit_font = nullptr;
                }
            }
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

}  // namespace

HWND CreateToolOptionsPane(
    HINSTANCE instance,
    HWND parent,
    ToolOptionsPaneState& state) noexcept {
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
    if (pane == nullptr
        || CreateControl(
               instance,
               pane,
               L"STATIC",
               L"ツール",
               SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_LABEL)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"STATIC",
               L"直径",
               SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_DIAMETER_LABEL)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"EDIT",
               L"8.0",
               WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
               IDC_TOOL_OPTIONS_DIAMETER)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"BUTTON",
               L"詳細...",
               WS_TABSTOP | BS_PUSHBUTTON,
               IDC_TOOL_OPTIONS_DETAILS)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"STATIC",
               L"消去対象",
               SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_TARGET_LABEL)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"BUTTON",
               L"主線",
               WS_TABSTOP | WS_GROUP | BS_AUTORADIOBUTTON | BS_PUSHLIKE,
               IDC_TOOL_OPTIONS_TARGET_MAIN_LINE)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"BUTTON",
               L"彩色",
               WS_TABSTOP | BS_AUTORADIOBUTTON | BS_PUSHLIKE,
               IDC_TOOL_OPTIONS_TARGET_COLOR)
            == nullptr) {
        if (pane != nullptr) {
            DestroyWindow(pane);
        }
        return nullptr;
    }
    SetWindowSubclass(
        pane,
        PaneSubclassProcedure,
        kPaneSubclass,
        reinterpret_cast<DWORD_PTR>(&state));
    SetWindowLongPtrW(
        pane, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(&state));
    UpdateFont(pane, state);
    UpdateToolOptionsPane(
        pane, state.active_tool, state.active_plane, state.diameter);
    return pane;
}

void UpdateToolOptionsPane(
    HWND pane,
    std::uint32_t active_tool,
    InkpodPlaneKind active_plane,
    float diameter) noexcept {
    auto* state = pane == nullptr
        ? nullptr
        : reinterpret_cast<ToolOptionsPaneState*>(
              GetWindowLongPtrW(pane, GWLP_USERDATA));
    if (state == nullptr) {
        return;
    }
    const bool preserve_diameter_edit = state->editing
        && state->active_tool == active_tool && CanEditDiameter(active_tool);
    state->updating = true;
    state->active_tool = active_tool;
    state->active_plane = active_plane;
    state->diameter = diameter;
    SetDlgItemTextW(pane, IDC_TOOL_OPTIONS_LABEL, ToolLabel(active_tool));
    std::array<wchar_t, 32U> value{};
    const float displayed_diameter = active_tool == INKPOD_TOOL_PENCIL
        ? kPencilToolDiameter
        : diameter;
    swprintf_s(
        value.data(),
        value.size(),
        L"%.1f",
        static_cast<double>(displayed_diameter));
    if (!preserve_diameter_edit) {
        SetDlgItemTextW(pane, IDC_TOOL_OPTIONS_DIAMETER, value.data());
    }
    const bool has_diameter = HasDiameter(active_tool);
    const bool can_edit_diameter = CanEditDiameter(active_tool);
    ShowWindow(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DIAMETER_LABEL),
        has_diameter ? SW_SHOW : SW_HIDE);
    ShowWindow(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DIAMETER),
        has_diameter ? SW_SHOW : SW_HIDE);
    EnableWindow(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DIAMETER),
        can_edit_diameter ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DETAILS),
        DetailsCommand(active_tool) != 0U ? TRUE : FALSE);
    const bool show_erase_target = active_tool == INKPOD_TOOL_ERASER;
    for (const int control : {
             IDC_TOOL_OPTIONS_TARGET_LABEL,
             IDC_TOOL_OPTIONS_TARGET_MAIN_LINE,
             IDC_TOOL_OPTIONS_TARGET_COLOR}) {
        ShowWindow(
            GetDlgItem(pane, control), show_erase_target ? SW_SHOW : SW_HIDE);
    }
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_TARGET_MAIN_LINE,
        BM_SETCHECK,
        active_plane == INKPOD_PLANE_MAIN_LINE ? BST_CHECKED : BST_UNCHECKED,
        0);
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_TARGET_COLOR,
        BM_SETCHECK,
        active_plane == INKPOD_PLANE_COLOR ? BST_CHECKED : BST_UNCHECKED,
        0);
    LayoutPane(pane);
    state->updating = false;
}

}  // namespace inkpod::windows::ui::panes
