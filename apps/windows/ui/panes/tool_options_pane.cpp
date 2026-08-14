#include "tool_options_pane.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cwchar>

#include "app/resource.h"
#include "inkpod/core_ffi.h"
#include "ui/localization.h"
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
    if (tool == INKPOD_TOOL_PENCIL) return UiText(UiStringId::ToolPencil);
    if (tool == INKPOD_TOOL_BRUSH) return UiText(UiStringId::ToolBrush);
    if (tool == INKPOD_TOOL_ERASER) return UiText(UiStringId::ToolEraser);
    if (tool == tools::kInteractionFill) return UiText(UiStringId::ToolFill);
    if (tool == tools::kInteractionEyedropper) return UiText(UiStringId::ToolEyedropper);
    if (tool == tools::kInteractionVectorLine) return UiText(UiStringId::ToolVectorLine);
    if (tool == tools::kInteractionVectorCurve) return UiText(UiStringId::ToolVectorCurve);
    if (tool == tools::kInteractionVectorRectangle) return UiText(UiStringId::ToolVectorRectangle);
    if (tool == tools::kInteractionVectorEllipse) return UiText(UiStringId::ToolVectorEllipse);
    if (tool == tools::kInteractionVectorPolyline) return UiText(UiStringId::ToolVectorPolyline);
    if (tool == tools::kInteractionVectorPolygon) return UiText(UiStringId::ToolVectorPolygon);
    if (tool == tools::kInteractionVectorEraser) return UiText(UiStringId::ToolVectorEraser);
    if (tool == tools::kInteractionEffectGradient) return UiText(UiStringId::ToolGradient);
    if (tool == tools::kInteractionEffectAirbrush) return UiText(UiStringId::ToolAirbrush);
    if (tool == tools::kInteractionEffectBlur) return UiText(UiStringId::ToolBlur);
    if (tool == tools::kInteractionEffectStamp) return UiText(UiStringId::ToolStamp);
    if (tool == tools::kInteractionEffectDust) return UiText(UiStringId::ToolDustRemoval);
    if (tool == tools::kInteractionEffectAlphaGradient) return UiText(UiStringId::ToolAlphaGradient);
    return UiText(UiStringId::ToolGeneric);
}

UINT DetailsCommand(std::uint32_t tool) noexcept {
    if (tools::IsVectorCanvasTool(tool) && tool != tools::kInteractionVectorEraser) {
        return IDM_GEOMETRY_OPTIONS;
    }
    if (tool == tools::kInteractionFill) return IDM_TOOL_FILL_OPTIONS;
    if (tool == tools::kInteractionSelection) return IDM_SELECTION_OPTIONS;
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
        || tool == INKPOD_TOOL_ERASER || tools::IsVectorCanvasTool(tool);
}

bool CanEditDiameter(std::uint32_t tool) noexcept {
    return tool == INKPOD_TOOL_BRUSH || tool == INKPOD_TOOL_ERASER
        || tools::IsVectorCanvasTool(tool);
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
    const int brush_shape_width = ScaleForDpi(78, dpi);
    const int brush_smoothing_width = ScaleForDpi(80, dpi);
    const int brush_start_width = ScaleForDpi(150, dpi);
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
    if (state != nullptr && state->active_tool == INKPOD_TOOL_BRUSH) {
        SetWindowPos(
            GetDlgItem(pane, IDC_TOOL_OPTIONS_BRUSH_SHAPE),
            nullptr,
            x,
            y,
            brush_shape_width,
            ScaleForDpi(120, dpi),
            SWP_NOACTIVATE | SWP_NOZORDER);
        x += brush_shape_width + margin;
        SetWindowPos(
            GetDlgItem(pane, IDC_TOOL_OPTIONS_BRUSH_SMOOTHING),
            nullptr,
            x,
            edit_y,
            brush_smoothing_width,
            edit_height,
            SWP_NOACTIVATE | SWP_NOZORDER);
        x += brush_smoothing_width + margin;
        SetWindowPos(
            GetDlgItem(pane, IDC_TOOL_OPTIONS_BRUSH_START_COLOR),
            nullptr,
            x,
            y,
            brush_start_width,
            row,
            SWP_NOACTIVATE | SWP_NOZORDER);
        x += brush_start_width + margin;
    }
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
             IDC_TOOL_OPTIONS_TARGET_COLOR,
             IDC_TOOL_OPTIONS_BRUSH_SHAPE,
             IDC_TOOL_OPTIONS_BRUSH_START_COLOR}) {
        SendDlgItemMessageW(
            pane, control, WM_SETFONT, reinterpret_cast<WPARAM>(replacement), TRUE);
    }
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_DIAMETER,
        WM_SETFONT,
        reinterpret_cast<WPARAM>(edit_replacement),
        TRUE);
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_SMOOTHING,
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
            pane, state.active_tool, state.active_plane, state.diameter, state.brush);
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
        pane, state.active_tool, state.active_plane, state.diameter, state.brush);
}

void CommitBrushSmoothing(HWND pane, ToolOptionsPaneState& state) noexcept {
    if (state.updating || state.active_tool != INKPOD_TOOL_BRUSH
        || state.change_brush == nullptr) {
        UpdateToolOptionsPane(
            pane, state.active_tool, state.active_plane, state.diameter, state.brush);
        return;
    }
    std::array<wchar_t, 64U> text{};
    GetDlgItemTextW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_SMOOTHING,
        text.data(),
        static_cast<int>(text.size()));
    wchar_t* end{};
    const unsigned long value = std::wcstoul(text.data(), &end, 10);
    if (end != text.data() && *end == L'\0' && value <= 1000UL) {
        InkpodEditorBrushOptions options = state.brush;
        options.struct_size = sizeof(options);
        options.smoothing = static_cast<std::uint16_t>(value);
        state.change_brush(state.context, options);
    }
    UpdateToolOptionsPane(
        pane, state.active_tool, state.active_plane, state.diameter, state.brush);
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
            if (LOWORD(wparam) == IDC_TOOL_OPTIONS_BRUSH_SMOOTHING
                && HIWORD(wparam) == EN_SETFOCUS) {
                state->editing_smoothing = true;
                return 0;
            }
            if (LOWORD(wparam) == IDC_TOOL_OPTIONS_BRUSH_SMOOTHING
                && HIWORD(wparam) == EN_KILLFOCUS) {
                state->editing_smoothing = false;
                CommitBrushSmoothing(pane, *state);
                return 0;
            }
            if (LOWORD(wparam) == IDC_TOOL_OPTIONS_BRUSH_SHAPE
                && HIWORD(wparam) == CBN_SELCHANGE
                && state->active_tool == INKPOD_TOOL_BRUSH
                && state->change_brush != nullptr) {
                const LRESULT selected = SendDlgItemMessageW(
                    pane, IDC_TOOL_OPTIONS_BRUSH_SHAPE, CB_GETCURSEL, 0, 0);
                if (selected == 0 || selected == 1) {
                    InkpodEditorBrushOptions options = state->brush;
                    options.struct_size = sizeof(options);
                    options.shape = selected == 0
                        ? INKPOD_BRUSH_ROUND
                        : INKPOD_BRUSH_SQUARE;
                    state->change_brush(state->context, options);
                }
                return 0;
            }
            if (LOWORD(wparam) == IDC_TOOL_OPTIONS_BRUSH_START_COLOR
                && HIWORD(wparam) == BN_CLICKED
                && state->active_tool == INKPOD_TOOL_BRUSH
                && state->change_brush != nullptr) {
                InkpodEditorBrushOptions options = state->brush;
                options.struct_size = sizeof(options);
                options.start_color = SendDlgItemMessageW(
                    pane,
                    IDC_TOOL_OPTIONS_BRUSH_START_COLOR,
                    BM_GETCHECK,
                    0,
                    0) == BST_CHECKED
                    ? INKPOD_START_COLOR_EXACT_NATIVE
                    : INKPOD_START_COLOR_ANY;
                state->change_brush(state->context, options);
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
        text == nullptr ? L"" : text,
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
               UiText(UiStringId::ToolGeneric),
               SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_LABEL)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"STATIC",
               UiText(UiStringId::ToolDiameter),
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
               UiText(UiStringId::ToolDetails),
               WS_TABSTOP | BS_PUSHBUTTON,
               IDC_TOOL_OPTIONS_DETAILS)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"STATIC",
               UiText(UiStringId::ToolEraseTarget),
               SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_TARGET_LABEL)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"BUTTON",
               UiText(UiStringId::MainLine),
               WS_TABSTOP | WS_GROUP | BS_AUTORADIOBUTTON | BS_PUSHLIKE,
               IDC_TOOL_OPTIONS_TARGET_MAIN_LINE)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"BUTTON",
               UiText(UiStringId::Coloring),
               WS_TABSTOP | BS_AUTORADIOBUTTON | BS_PUSHLIKE,
               IDC_TOOL_OPTIONS_TARGET_COLOR)
            == nullptr
        || CreateControl(
               instance,
               pane,
               WC_COMBOBOXW,
               L"",
               WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
               IDC_TOOL_OPTIONS_BRUSH_SHAPE)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"EDIT",
               L"0",
               WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL | ES_NUMBER,
               IDC_TOOL_OPTIONS_BRUSH_SMOOTHING)
            == nullptr
        || CreateControl(
               instance,
               pane,
               L"BUTTON",
               UiText(UiStringId::ToolFillMatchingStartColor),
               WS_TABSTOP | BS_AUTOCHECKBOX,
               IDC_TOOL_OPTIONS_BRUSH_START_COLOR)
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
    SendDlgItemMessageW(
        pane, IDC_TOOL_OPTIONS_BRUSH_SHAPE, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L"Round"));
    SendDlgItemMessageW(
        pane, IDC_TOOL_OPTIONS_BRUSH_SHAPE, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L"Square"));
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_SMOOTHING,
        EM_SETCUEBANNER,
        TRUE,
        reinterpret_cast<LPARAM>(L"Smoothing 0-1000"));
    UpdateFont(pane, state);
    UpdateToolOptionsPane(
        pane, state.active_tool, state.active_plane, state.diameter, state.brush);
    return pane;
}

void UpdateToolOptionsPane(
    HWND pane,
    std::uint32_t active_tool,
    InkpodPlaneKind active_plane,
    float diameter,
    const InkpodEditorBrushOptions& brush) noexcept {
    auto* state = pane == nullptr
        ? nullptr
        : reinterpret_cast<ToolOptionsPaneState*>(
              GetWindowLongPtrW(pane, GWLP_USERDATA));
    if (state == nullptr) {
        return;
    }
    const bool preserve_diameter_edit = state->editing
        && state->active_tool == active_tool && CanEditDiameter(active_tool);
    const bool preserve_smoothing_edit = state->editing_smoothing
        && state->active_tool == active_tool && active_tool == INKPOD_TOOL_BRUSH;
    state->updating = true;
    state->active_tool = active_tool;
    state->active_plane = active_plane;
    state->diameter = diameter;
    state->brush = brush;
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
    const bool show_brush = active_tool == INKPOD_TOOL_BRUSH;
    for (const int control : {
             IDC_TOOL_OPTIONS_BRUSH_SHAPE,
             IDC_TOOL_OPTIONS_BRUSH_SMOOTHING,
             IDC_TOOL_OPTIONS_BRUSH_START_COLOR}) {
        ShowWindow(GetDlgItem(pane, control), show_brush ? SW_SHOW : SW_HIDE);
    }
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_SHAPE,
        CB_SETCURSEL,
        brush.shape == INKPOD_BRUSH_SQUARE ? 1 : 0,
        0);
    if (!preserve_smoothing_edit) {
        swprintf_s(value.data(), value.size(), L"%u", static_cast<unsigned>(brush.smoothing));
        SetDlgItemTextW(pane, IDC_TOOL_OPTIONS_BRUSH_SMOOTHING, value.data());
    }
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_START_COLOR,
        BM_SETCHECK,
        brush.start_color == INKPOD_START_COLOR_EXACT_NATIVE ? BST_CHECKED : BST_UNCHECKED,
        0);
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
