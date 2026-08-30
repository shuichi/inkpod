#include "ui/ui_resources.h"

#include "subpalette_pane.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <utility>

#include "app/resource.h"
#include "pane_dialog_layout.h"
#include "renderer/renderer_host.h"
#include "ui/icons/fluent_icons.h"
#include "ui/localization.h"

namespace inkpod::windows::ui::panes {
namespace {

constexpr UINT_PTR kSubpaletteCanvasSubclass = 1U;
constexpr UINT_PTR kSubpaletteKeySubclass = 2U;

bool AddSubpaletteTooltip(
    HWND tooltip, HWND dialog, int control, UiStringId text) noexcept {
    const HWND button = GetDlgItem(dialog, control);
    if (tooltip == nullptr || button == nullptr) {
        return false;
    }
    TOOLINFOW tool{};
    tool.cbSize = sizeof(tool);
    tool.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    tool.hwnd = dialog;
    tool.uId = reinterpret_cast<UINT_PTR>(button);
    tool.lpszText = const_cast<wchar_t*>(UiText(text));
    return SendMessageW(
               tooltip,
               TTM_ADDTOOLW,
               0,
               reinterpret_cast<LPARAM>(&tool))
        != FALSE;
}

std::size_t PlaceCompactSubpaletteButtonRows(
    PaneDialogLayoutPlan& plan,
    HWND dialog,
    std::span<const int> controls,
    int x,
    int y,
    int available_width,
    int row_height,
    int gap) noexcept {
    if (controls.empty()) {
        return 0U;
    }
    std::size_t row{};
    int used{};
    for (const int control : controls) {
        const int ideal_width = control == IDC_SUBPALETTE_REGISTER
            ? ScalePaneDip(dialog, 32)
            : PaneButtonIdealWidth(dialog, control);
        const int control_width =
            std::min(std::max(0, available_width), ideal_width);
        if (used != 0 && used + gap + control_width > available_width) {
            ++row;
            used = 0;
        }
        const int offset = used == 0 ? 0 : gap;
        static_cast<void>(plan.PlaceControl(
            control,
            x + used + offset,
            y + static_cast<int>(row) * (row_height + gap),
            control_width,
            row_height));
        used += offset + control_width;
    }
    return row + 1U;
}

void Dispatch(SubpalettePaneDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

void Perform(SubpalettePaneDialogState& state, SubpalettePaneAction action) noexcept {
    if (state.perform_action != nullptr) {
        state.perform_action(state.context, action);
    }
}

bool PerformNavigationKey(
    SubpalettePaneDialogState& state, WPARAM virtual_key) noexcept {
    if (virtual_key == VK_LEFT || virtual_key == VK_UP
        || virtual_key == VK_PRIOR) {
        Perform(state, SubpalettePaneAction::Previous);
        return true;
    }
    if (virtual_key == VK_RIGHT || virtual_key == VK_DOWN
        || virtual_key == VK_NEXT) {
        Perform(state, SubpalettePaneAction::Next);
        return true;
    }
    return false;
}

LRESULT NavigationDialogCode(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    const LRESULT base = DefSubclassProc(window, message, wparam, lparam);
    if (wparam == VK_LEFT || wparam == VK_RIGHT
        || wparam == VK_UP || wparam == VK_DOWN) {
        return base | DLGC_WANTARROWS;
    }
    if (wparam == VK_PRIOR || wparam == VK_NEXT) {
        return base | DLGC_WANTMESSAGE;
    }
    return base;
}

LRESULT CALLBACK SubpaletteKeySubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<SubpalettePaneDialogState*>(reference);
    switch (message) {
        case WM_GETDLGCODE:
            return NavigationDialogCode(window, message, wparam, lparam);
        case WM_KEYDOWN:
            if (state != nullptr && PerformNavigationKey(*state, wparam)) {
                return 0;
            }
            break;
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                window,
                SubpaletteKeySubclassProcedure,
                kSubpaletteKeySubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

std::uint8_t SampleChannel8(
    const InkpodColorValue& color, std::uint16_t channel) noexcept {
    return static_cast<std::uint8_t>(
        color.depth == INKPOD_COLOR_DEPTH_16
            ? (static_cast<std::uint32_t>(channel) + 128U) / 257U
            : channel & 0xffU);
}

COLORREF CompositeSampleColor(
    const InkpodColorValue& color, COLORREF background) noexcept {
    const std::uint32_t alpha = SampleChannel8(color, color.alpha);
    const auto blend = [alpha](std::uint8_t source, std::uint8_t destination) noexcept {
        return static_cast<std::uint8_t>(
            (static_cast<std::uint32_t>(source) * alpha
             + static_cast<std::uint32_t>(destination) * (255U - alpha)
             + 127U)
            / 255U);
    };
    return RGB(
        blend(SampleChannel8(color, color.red), GetRValue(background)),
        blend(SampleChannel8(color, color.green), GetGValue(background)),
        blend(SampleChannel8(color, color.blue), GetBValue(background)));
}

void DrawSampleRegisterButton(
    const DRAWITEMSTRUCT& draw, const SubpalettePaneView& view) noexcept {
    RECT bounds = draw.rcItem;
    UINT frame_state = DFCS_BUTTONPUSH;
    if ((draw.itemState & ODS_SELECTED) != 0U) {
        frame_state |= DFCS_PUSHED;
    }
    if ((draw.itemState & ODS_DISABLED) != 0U) {
        frame_state |= DFCS_INACTIVE;
    }
    DrawFrameControl(draw.hDC, &bounds, DFC_BUTTON, frame_state);
    InflateRect(&bounds, -ScalePaneDip(draw.hwndItem, 4), -ScalePaneDip(draw.hwndItem, 4));
    if (view.sample_available) {
        const COLORREF light = GetSysColor(COLOR_WINDOW);
        const COLORREF dark = GetSysColor(COLOR_3DLIGHT);
        const COLORREF colors[]{
            CompositeSampleColor(view.sample_color, light),
            CompositeSampleColor(view.sample_color, dark)};
        HBRUSH brushes[]{CreateSolidBrush(colors[0]), CreateSolidBrush(colors[1])};
        if (brushes[0] != nullptr && brushes[1] != nullptr) {
            const int checker = std::max(2, ScalePaneDip(draw.hwndItem, 4));
            for (int y = bounds.top; y < bounds.bottom; y += checker) {
                for (int x = bounds.left; x < bounds.right; x += checker) {
                    const RECT tile{
                        x,
                        y,
                        std::min<LONG>(
                            bounds.right, static_cast<LONG>(x + checker)),
                        std::min<LONG>(
                            bounds.bottom, static_cast<LONG>(y + checker))};
                    FillRect(
                        draw.hDC,
                        &tile,
                        brushes[((x - bounds.left) / checker
                                 + (y - bounds.top) / checker)
                                & 1]);
                }
            }
        }
        if (brushes[0] != nullptr) {
            DeleteObject(brushes[0]);
        }
        if (brushes[1] != nullptr) {
            DeleteObject(brushes[1]);
        }
    }
    FrameRect(draw.hDC, &bounds, GetSysColorBrush(COLOR_WINDOWTEXT));
    if ((draw.itemState & ODS_FOCUS) != 0U) {
        RECT focus = draw.rcItem;
        InflateRect(&focus, -ScalePaneDip(draw.hwndItem, 2), -ScalePaneDip(draw.hwndItem, 2));
        DrawFocusRect(draw.hDC, &focus);
    }
}

LRESULT CALLBACK SubpaletteCanvasSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<SubpalettePaneDialogState*>(reference);
    if (state == nullptr) {
        return DefSubclassProc(window, message, wparam, lparam);
    }
    switch (message) {
        case WM_GETDLGCODE:
            return NavigationDialogCode(window, message, wparam, lparam);
        case WM_SETCURSOR:
            if (LOWORD(lparam) == HTCLIENT && state->eyedropper_cursor != nullptr) {
                SetCursor(state->eyedropper_cursor);
                return TRUE;
            }
            break;
        case WM_KEYDOWN:
            if (PerformNavigationKey(*state, wparam)) {
                return 0;
            }
            break;
        case WM_DPICHANGED_AFTERPARENT: {
            const UINT dpi = GetDpiForWindow(window);
            const HCURSOR cursor = CreateToolCursor(
                reinterpret_cast<HINSTANCE>(
                    GetWindowLongPtrW(window, GWLP_HINSTANCE)),
                ToolIconId::Eyedropper,
                dpi == 0U ? 96U : dpi);
            if (cursor != nullptr) {
                if (state->eyedropper_cursor != nullptr) {
                    DestroyCursor(state->eyedropper_cursor);
                }
                state->eyedropper_cursor = cursor;
            }
            break;
        }
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                window,
                SubpaletteCanvasSubclassProcedure,
                kSubpaletteCanvasSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

INT_PTR CALLBACK SubpalettePaneProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<SubpalettePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<SubpalettePaneDialogState*>(lparam);
            if (state == nullptr || state->dispatch_command == nullptr
                || state->perform_action == nullptr || state->sample == nullptr
                || state->apply_view == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            return TRUE;
        case WM_SIZE:
            LayoutSubpalettePaneDialog(dialog);
            return TRUE;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDC_SUBPALETTE_TARGET:
                    Perform(*state, SubpalettePaneAction::OpenFiles);
                    return TRUE;
                case IDC_SUBPALETTE_PIN:
                    Perform(*state, SubpalettePaneAction::OpenFolder);
                    return TRUE;
                case IDC_SUBPALETTE_PREVIOUS:
                    Perform(*state, SubpalettePaneAction::Previous);
                    return TRUE;
                case IDC_SUBPALETTE_NEXT:
                    Perform(*state, SubpalettePaneAction::Next);
                    return TRUE;
                case IDC_SUBPALETTE_FIT:
                    Perform(*state, SubpalettePaneAction::Fit);
                    return TRUE;
                case IDC_SUBPALETTE_ONE_TO_ONE:
                    Perform(*state, SubpalettePaneAction::OneToOne);
                    return TRUE;
                case IDC_SUBPALETTE_REGISTER:
                    Perform(*state, SubpalettePaneAction::RegisterSample);
                    return TRUE;
                case IDCANCEL:
                    Dispatch(*state, IDM_WINDOW_SUBPALETTE);
                    return TRUE;
                default:
                    break;
            }
            break;
        case WM_DRAWITEM:
            if (state != nullptr
                && static_cast<int>(wparam) == IDC_SUBPALETTE_REGISTER) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawSampleRegisterButton(*draw, state->view);
                }
                return TRUE;
            }
            break;
        case renderer::kCanvasStrokeReady: {
            bool handled{};
            if (state != nullptr && state->canvas != nullptr) {
                renderer::OwnedCanvasStrokeEvent event{};
                if (renderer::TakeCanvasStrokeEvent(
                        state->canvas,
                        static_cast<std::uint64_t>(wparam),
                        app::Generation{static_cast<std::uint64_t>(lparam)},
                        event)) {
                    handled = true;
                    if (event.kind != renderer::CanvasStrokeEventKind::Cancel
                        && !event.samples.empty()) {
                        const auto& sample = event.samples.back();
                        state->sample(state->context, sample.x, sample.y);
                    }
                }
            }
            SetWindowLongPtrW(dialog, DWLP_MSGRESULT, handled ? 1 : 0);
            return TRUE;
        }
        case renderer::kCanvasViewGesture: {
            bool handled{};
            if (state != nullptr && state->canvas != nullptr) {
                renderer::CanvasViewGesture gesture{};
                if (renderer::TakeCanvasViewGesture(
                        state->canvas,
                        static_cast<std::uint64_t>(wparam),
                        app::Generation{static_cast<std::uint64_t>(lparam)},
                        gesture)) {
                    state->apply_view(state->context, gesture);
                    handled = true;
                }
            }
            SetWindowLongPtrW(dialog, DWLP_MSGRESULT, handled ? 1 : 0);
            return TRUE;
        }
        case renderer::kCanvasViewportChanged:
            if (state != nullptr) {
                const renderer::CanvasViewGesture gesture{
                    INKPOD_VIEW_VIEWPORT_RESIZED,
                    static_cast<double>(LOWORD(lparam)),
                    static_cast<double>(HIWORD(lparam)),
                    0.0};
                state->apply_view(state->context, gesture);
            }
            return TRUE;
        case renderer::kCanvasActivated:
            return TRUE;
        case WM_CLOSE:
            if (state != nullptr) {
                Dispatch(*state, IDM_WINDOW_SUBPALETTE);
            }
            return TRUE;
        case WM_NCDESTROY:
            if (state != nullptr) {
                state->canvas = nullptr;
                state->tooltip = nullptr;
                if (state->eyedropper_cursor != nullptr) {
                    DestroyCursor(state->eyedropper_cursor);
                    state->eyedropper_cursor = nullptr;
                }
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

HWND CreateSubpalettePaneDialog(
    HINSTANCE instance,
    HWND owner,
    renderer::RendererHost& renderer_host,
    app::CanvasId canvas_id,
    app::Generation surface_generation,
    SubpalettePaneDialogState& state) noexcept {
    state.surface_generation = surface_generation;
    const HWND dialog = CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_SUBPALETTE_PALETTE),
        owner,
        SubpalettePaneProcedure,
        reinterpret_cast<LPARAM>(&state));
    if (dialog == nullptr) {
        return nullptr;
    }
    EnablePaneDialogResizePainting(dialog);
    const HWND placeholder = GetDlgItem(dialog, IDC_SUBPALETTE_CANVAS);
    RECT bounds{};
    if (placeholder == nullptr || GetWindowRect(placeholder, &bounds) == FALSE) {
        DestroyWindow(dialog);
        return nullptr;
    }
    MapWindowPoints(nullptr, dialog, reinterpret_cast<POINT*>(&bounds), 2);
    state.canvas = renderer::CreateCanvasWindow(
        instance, dialog, renderer_host, canvas_id, surface_generation);
    if (state.canvas == nullptr) {
        DestroyWindow(dialog);
        return nullptr;
    }
    const UINT window_dpi = GetDpiForWindow(state.canvas);
    state.eyedropper_cursor = CreateToolCursor(
        instance,
        ToolIconId::Eyedropper,
        window_dpi == 0U ? 96U : window_dpi);
    if (SetWindowSubclass(
            state.canvas,
            SubpaletteCanvasSubclassProcedure,
            kSubpaletteCanvasSubclass,
            reinterpret_cast<DWORD_PTR>(&state)) == FALSE) {
        DestroyWindow(dialog);
        return nullptr;
    }
    SetWindowPos(
        state.canvas,
        placeholder,
        bounds.left,
        bounds.top,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        SWP_NOACTIVATE | SWP_SHOWWINDOW);
    ShowWindow(placeholder, SW_HIDE);
    SetWindowTextW(
        GetDlgItem(dialog, IDC_SUBPALETTE_SAMPLE_SWATCH),
        UiText(UiStringId::DrawingColor));
    const HWND register_button = GetDlgItem(dialog, IDC_SUBPALETTE_REGISTER);
    if (register_button == nullptr) {
        DestroyWindow(dialog);
        return nullptr;
    }
    const LONG_PTR register_style =
        GetWindowLongPtrW(register_button, GWL_STYLE);
    SetWindowLongPtrW(
        register_button,
        GWL_STYLE,
        (register_style & ~static_cast<LONG_PTR>(BS_TYPEMASK))
            | BS_OWNERDRAW | BS_ICON);
    ShowWindow(GetDlgItem(dialog, IDC_SUBPALETTE_SAMPLE_SWATCH), SW_HIDE);
    for (const auto [control, icon] : std::array{
             std::pair{IDC_SUBPALETTE_TARGET, PaneIconId::OpenFiles},
             std::pair{IDC_SUBPALETTE_PIN, PaneIconId::OpenFolder},
             std::pair{IDC_SUBPALETTE_PREVIOUS, PaneIconId::Previous},
             std::pair{IDC_SUBPALETTE_NEXT, PaneIconId::Next},
             std::pair{IDC_SUBPALETTE_FIT, PaneIconId::Fit},
             std::pair{IDC_SUBPALETTE_ONE_TO_ONE, PaneIconId::OneToOne}}) {
        (void)SetPaneIconButton(GetDlgItem(dialog, control), icon);
    }
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
    const std::array tooltips{
        std::pair{IDC_SUBPALETTE_TARGET, UiStringId::SubpaletteOpenFiles},
        std::pair{IDC_SUBPALETTE_PIN, UiStringId::SubpaletteOpenFolder},
        std::pair{IDC_SUBPALETTE_PREVIOUS, UiStringId::Text0533},
        std::pair{IDC_SUBPALETTE_NEXT, UiStringId::Text0762},
        std::pair{IDC_SUBPALETTE_FIT, UiStringId::Text0501},
        std::pair{IDC_SUBPALETTE_ONE_TO_ONE, UiStringId::Text0838},
        std::pair{
            IDC_SUBPALETTE_REGISTER,
            UiStringId::SubpaletteRegisterSample}};
    if (state.tooltip == nullptr
        || !std::all_of(
            tooltips.cbegin(),
            tooltips.cend(),
            [tooltip = state.tooltip, dialog](const auto& entry) {
                return AddSubpaletteTooltip(
                    tooltip, dialog, entry.first, entry.second);
            })) {
        DestroyWindow(dialog);
        return nullptr;
    }
    for (const int control : std::array{
             IDC_SUBPALETTE_TARGET,
             IDC_SUBPALETTE_PIN,
             IDC_SUBPALETTE_PREVIOUS,
             IDC_SUBPALETTE_NEXT,
             IDC_SUBPALETTE_FIT,
             IDC_SUBPALETTE_ONE_TO_ONE,
             IDC_SUBPALETTE_REGISTER}) {
        const HWND child = GetDlgItem(dialog, control);
        if (child == nullptr
            || SetWindowSubclass(
                   child,
                   SubpaletteKeySubclassProcedure,
                   kSubpaletteKeySubclass,
                   reinterpret_cast<DWORD_PTR>(&state)) == FALSE) {
            DestroyWindow(dialog);
            return nullptr;
        }
    }
    LayoutSubpalettePaneDialog(dialog);
    return dialog;
}

void LayoutSubpalettePaneDialog(HWND dialog) noexcept {
    if (dialog == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<SubpalettePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    if (state == nullptr || state->canvas == nullptr) {
        return;
    }
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const int margin = ScalePaneDip(dialog, 8);
    const int gap = ScalePaneDip(dialog, 6);
    const int line_height = ScalePaneDip(dialog, 18);
    const int row_height = ScalePaneDip(dialog, 26);
    const int width = static_cast<int>(client.right - client.left);
    const int height = static_cast<int>(client.bottom - client.top);
    const int content_width = std::max(0, width - margin * 2);
    PaneDialogLayoutPlan plan(dialog);

    const std::array<int, 7U> toolbar_actions{
        IDC_SUBPALETTE_TARGET,
        IDC_SUBPALETTE_PIN,
        IDC_SUBPALETTE_PREVIOUS,
        IDC_SUBPALETTE_NEXT,
        IDC_SUBPALETTE_FIT,
        IDC_SUBPALETTE_ONE_TO_ONE,
        IDC_SUBPALETTE_REGISTER};
    const std::size_t toolbar_rows = PlaceCompactSubpaletteButtonRows(
        plan,
        dialog,
        toolbar_actions,
        margin,
        margin,
        content_width,
        row_height,
        gap);
    const int source_top = margin
        + static_cast<int>(toolbar_rows) * row_height
        + std::max(0, static_cast<int>(toolbar_rows) - 1) * gap + gap;
    static_cast<void>(plan.PlaceControl(
        IDC_SUBPALETTE_SOURCE,
        margin,
        source_top,
        content_width,
        line_height));

    const int hint_top = std::max(
        source_top + line_height + gap,
        height - margin - line_height);
    static_cast<void>(plan.PlaceControl(
        IDC_SUBPALETTE_HINT,
        margin,
        hint_top,
        content_width,
        line_height));
    const int canvas_top = source_top + line_height + gap;
    const int canvas_height = std::max(0, hint_top - gap - canvas_top);
    static_cast<void>(plan.PlaceWindow(
        state->canvas,
        margin,
        canvas_top,
        std::max(1, content_width),
        std::max(1, canvas_height)));
    static_cast<void>(plan.PlaceControl(
        IDC_SUBPALETTE_EMPTY,
        margin + gap,
        canvas_top + std::max(0, (canvas_height - line_height) / 2),
        std::max(0, content_width - gap * 2),
        line_height));
    static_cast<void>(plan.Commit(PaneDialogRepaint::Complete));
}

void UpdateSubpalettePaneDialog(HWND dialog, SubpalettePaneView view) noexcept {
    if (dialog == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<SubpalettePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    if (state == nullptr) {
        return;
    }
    state->view = std::move(view);
    SetDlgItemTextW(dialog, IDC_SUBPALETTE_SOURCE, state->view.source_text.c_str());
    SetDlgItemTextW(dialog, IDC_SUBPALETTE_EMPTY, state->view.empty_text.c_str());
    const bool source = state->view.source_available;
    EnableWindow(
        GetDlgItem(dialog, IDC_SUBPALETTE_TARGET),
        state->view.loading ? FALSE : TRUE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SUBPALETTE_PIN),
        state->view.loading ? FALSE : TRUE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SUBPALETTE_PREVIOUS),
        state->view.can_previous && !state->view.loading ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SUBPALETTE_NEXT),
        state->view.can_next && !state->view.loading ? TRUE : FALSE);
    EnableWindow(GetDlgItem(dialog, IDC_SUBPALETTE_FIT), source ? TRUE : FALSE);
    EnableWindow(GetDlgItem(dialog, IDC_SUBPALETTE_ONE_TO_ONE), source ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SUBPALETTE_REGISTER),
        state->view.sample_available ? TRUE : FALSE);
    InvalidateRect(
        GetDlgItem(dialog, IDC_SUBPALETTE_REGISTER), nullptr, TRUE);
    if (state->canvas != nullptr) {
        EnableWindow(state->canvas, source ? TRUE : FALSE);
        ShowWindow(state->canvas, source ? SW_SHOW : SW_HIDE);
    }
    ShowWindow(
        GetDlgItem(dialog, IDC_SUBPALETTE_EMPTY),
        source ? SW_HIDE : SW_SHOW);
    LayoutSubpalettePaneDialog(dialog);
}

}  // namespace inkpod::windows::ui::panes
