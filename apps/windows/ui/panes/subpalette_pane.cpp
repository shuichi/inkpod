#include "subpalette_pane.h"

#include <algorithm>
#include <utility>

#include "app/resource.h"
#include "renderer/renderer_host.h"

namespace inkpod::windows::ui::panes {
namespace {

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
                case IDC_SUBPALETTE_PIN:
                    Dispatch(*state, IDM_SUBPALETTE_PIN);
                    return TRUE;
                case IDC_SUBPALETTE_PREVIOUS:
                    Perform(*state, SubpalettePaneAction::Previous);
                    return TRUE;
                case IDC_SUBPALETTE_NEXT:
                    Perform(*state, SubpalettePaneAction::Next);
                    return TRUE;
                case IDC_SUBPALETTE_CURRENT:
                    Perform(*state, SubpalettePaneAction::Current);
                    return TRUE;
                case IDC_SUBPALETTE_FIT:
                    Perform(*state, SubpalettePaneAction::Fit);
                    return TRUE;
                case IDC_SUBPALETTE_ONE_TO_ONE:
                    Perform(*state, SubpalettePaneAction::OneToOne);
                    return TRUE;
                case IDC_SUBPALETTE_AUTO_PREVIOUS:
                    Perform(*state, SubpalettePaneAction::ToggleAutoPrevious);
                    return TRUE;
                case IDC_SUBPALETTE_SCROLL_SYNC:
                    Perform(*state, SubpalettePaneAction::ToggleScrollSync);
                    return TRUE;
                case IDC_SUBPALETTE_REGISTER:
                    Dispatch(*state, IDM_PALETTE_REGISTER);
                    return TRUE;
                case IDCANCEL:
                    ShowWindow(dialog, SW_HIDE);
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
                renderer::CanvasViewGesture gesture{
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
            ShowWindow(dialog, SW_HIDE);
            return TRUE;
        case WM_NCDESTROY:
            if (state != nullptr) {
                state->canvas = nullptr;
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
    const HWND dialog = CreateDialogParamW(
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
    SetWindowPos(
        state.canvas,
        placeholder,
        bounds.left,
        bounds.top,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        SWP_NOACTIVATE | SWP_SHOWWINDOW);
    ShowWindow(placeholder, SW_HIDE);
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
    const int dpi = static_cast<int>(GetDpiForWindow(dialog));
    const int margin = MulDiv(8, dpi, 96);
    const int top = MulDiv(49, dpi, 96);
    const int bottom = MulDiv(85, dpi, 96);
    SetWindowPos(
        state->canvas,
        nullptr,
        margin,
        top,
        std::max(1, static_cast<int>(client.right) - margin * 2),
        std::max(1, static_cast<int>(client.bottom) - top - bottom),
        SWP_NOACTIVATE | SWP_NOZORDER);
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
    SetDlgItemTextW(dialog, IDC_SUBPALETTE_TARGET, state->view.target_text.c_str());
    SetDlgItemTextW(dialog, IDC_SUBPALETTE_SOURCE, state->view.source_text.c_str());
    SetDlgItemTextW(dialog, IDC_SUBPALETTE_EMPTY, state->view.empty_text.c_str());
    SetDlgItemTextW(
        dialog,
        IDC_SUBPALETTE_PIN,
        state->view.pinned ? L"追従へ戻す" : L"文書に固定");
    CheckDlgButton(
        dialog,
        IDC_SUBPALETTE_AUTO_PREVIOUS,
        state->view.auto_previous ? BST_CHECKED : BST_UNCHECKED);
    CheckDlgButton(
        dialog,
        IDC_SUBPALETTE_SCROLL_SYNC,
        state->view.scroll_sync ? BST_CHECKED : BST_UNCHECKED);
    const bool target = state->view.target_available;
    const bool source = target && state->view.source_available;
    EnableWindow(GetDlgItem(dialog, IDC_SUBPALETTE_PIN), target ? TRUE : FALSE);
    for (const int control : {
             IDC_SUBPALETTE_PREVIOUS,
             IDC_SUBPALETTE_NEXT,
             IDC_SUBPALETTE_CURRENT,
             IDC_SUBPALETTE_FIT,
             IDC_SUBPALETTE_ONE_TO_ONE,
             IDC_SUBPALETTE_AUTO_PREVIOUS,
             IDC_SUBPALETTE_SCROLL_SYNC,
             IDC_SUBPALETTE_REGISTER}) {
        EnableWindow(GetDlgItem(dialog, control), source ? TRUE : FALSE);
    }
    if (state->canvas != nullptr) {
        EnableWindow(state->canvas, source ? TRUE : FALSE);
        ShowWindow(state->canvas, source ? SW_SHOW : SW_HIDE);
    }
    ShowWindow(
        GetDlgItem(dialog, IDC_SUBPALETTE_EMPTY),
        source ? SW_HIDE : SW_SHOW);
}

}  // namespace inkpod::windows::ui::panes
