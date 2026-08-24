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
        case WM_SETCURSOR:
            if (LOWORD(lparam) == HTCLIENT && state->eyedropper_cursor != nullptr) {
                SetCursor(state->eyedropper_cursor);
                return TRUE;
            }
            break;
        case WM_KEYDOWN:
            if (wparam == VK_LEFT || wparam == VK_UP || wparam == VK_PRIOR) {
                Perform(*state, SubpalettePaneAction::Previous);
                return 0;
            }
            if (wparam == VK_RIGHT || wparam == VK_DOWN || wparam == VK_NEXT) {
                Perform(*state, SubpalettePaneAction::Next);
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
        case renderer::kCanvasStrokeReady:
            if (state != nullptr && state->canvas != nullptr) {
                renderer::OwnedCanvasStrokeEvent event{};
                if (renderer::TakeCanvasStrokeEvent(
                        state->canvas,
                        static_cast<std::uint64_t>(wparam),
                        app::Generation{static_cast<std::uint64_t>(lparam)},
                        event)
                    && event.kind == renderer::CanvasStrokeEventKind::End
                    && !event.samples.empty()) {
                    const auto& sample = event.samples.back();
                    state->sample(state->context, sample.x, sample.y);
                }
            }
            return TRUE;
        case renderer::kCanvasViewGesture:
            if (state != nullptr && state->canvas != nullptr) {
                renderer::CanvasViewGesture gesture{};
                if (renderer::TakeCanvasViewGesture(
                        state->canvas,
                        static_cast<std::uint64_t>(wparam),
                        app::Generation{static_cast<std::uint64_t>(lparam)},
                        gesture)) {
                    state->apply_view(state->context, gesture);
                }
            }
            return TRUE;
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
    for (const auto [control, icon] : std::array{
             std::pair{IDC_SUBPALETTE_PREVIOUS, PaneIconId::Previous},
             std::pair{IDC_SUBPALETTE_NEXT, PaneIconId::Next},
             std::pair{IDC_SUBPALETTE_FIT, PaneIconId::Fit},
             std::pair{IDC_SUBPALETTE_ONE_TO_ONE, PaneIconId::OneToOne}}) {
        (void)SetPaneIconButton(GetDlgItem(dialog, control), icon);
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

    const std::array<int, 2U> open_actions{
        IDC_SUBPALETTE_TARGET,
        IDC_SUBPALETTE_PIN};
    PlacePaneButtonRows(
        dialog,
        open_actions,
        margin,
        margin,
        content_width,
        row_height,
        gap);
    const int navigation_top = margin + row_height + gap;
    const std::array<int, 5U> navigation_actions{
        IDC_SUBPALETTE_PREVIOUS,
        IDC_SUBPALETTE_NEXT,
        IDC_SUBPALETTE_FIT,
        IDC_SUBPALETTE_ONE_TO_ONE,
        IDC_SUBPALETTE_REGISTER};
    PlacePaneButtonRows(
        dialog,
        navigation_actions,
        margin,
        navigation_top,
        content_width,
        row_height,
        gap);
    const std::size_t navigation_rows = PaneButtonRowCount(
        dialog, navigation_actions, content_width, gap);
    const int source_top = navigation_top
        + static_cast<int>(navigation_rows) * row_height
        + std::max(0, static_cast<int>(navigation_rows) - 1) * gap + gap;
    PlacePaneDialogControl(
        dialog,
        IDC_SUBPALETTE_SOURCE,
        margin,
        source_top,
        content_width,
        line_height);

    const int hint_top = std::max(
        source_top + line_height + gap,
        height - margin - line_height);
    PlacePaneDialogControl(
        dialog,
        IDC_SUBPALETTE_HINT,
        margin,
        hint_top,
        content_width,
        line_height);
    const int canvas_top = source_top + line_height + gap;
    const int canvas_height = std::max(0, hint_top - gap - canvas_top);
    SetWindowPos(
        state->canvas,
        nullptr,
        margin,
        canvas_top,
        std::max(1, content_width),
        std::max(1, canvas_height),
        SWP_NOACTIVATE | SWP_NOZORDER);
    PlacePaneDialogControl(
        dialog,
        IDC_SUBPALETTE_EMPTY,
        margin + gap,
        canvas_top + std::max(0, (canvas_height - line_height) / 2),
        std::max(0, content_width - gap * 2),
        line_height);
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
