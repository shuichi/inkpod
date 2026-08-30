#include "ui/ui_resources.h"

#include "light_table_pane.h"

#include <algorithm>
#include <array>
#include <cwchar>
#include <utility>

#include "app/resource.h"
#include "inkpod/core_ffi.h"
#include "pane_dialog_layout.h"
#include "ui/icons/fluent_icons.h"
#include "ui/localization.h"

namespace inkpod::windows::ui::panes {
namespace {

void Dispatch(LightTablePaneDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

void LayoutLightTablePane(HWND dialog) noexcept {
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

    const int pin_width = std::min(
        content_width, PaneButtonIdealWidth(dialog, IDC_LIGHT_TABLE_PIN));
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_PIN,
        margin + std::max(0, content_width - pin_width),
        margin,
        pin_width,
        row_height));
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_TARGET,
        margin,
        margin + ScalePaneDip(dialog, 4),
        std::max(0, content_width - pin_width - gap),
        line_height));

    const int set_top = margin + row_height + gap;
    const int set_label_width = ScalePaneDip(dialog, 48);
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_SET_LABEL,
        margin,
        set_top + ScalePaneDip(dialog, 4),
        set_label_width,
        line_height));
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_SETS,
        margin + set_label_width + gap,
        set_top,
        std::max(0, content_width - set_label_width - gap),
        row_height));

    const int set_actions_top = set_top + row_height + gap;
    const std::array<int, 4U> set_action_controls{
        IDC_LIGHT_TABLE_SET_NEW,
        IDC_LIGHT_TABLE_SET_DUPLICATE,
        IDC_LIGHT_TABLE_SET_DELETE,
        IDC_LIGHT_TABLE_GLOBAL_OPACITY};
    const std::size_t set_action_rows = PaneButtonRowCount(
        dialog, set_action_controls, content_width, gap);
    const int set_actions_height = static_cast<int>(set_action_rows) * row_height
        + std::max(0, static_cast<int>(set_action_rows) - 1) * gap;
    PlacePaneButtonRows(
        plan,
        set_action_controls,
        margin,
        set_actions_top,
        content_width,
        row_height,
        gap);

    const int items_label_top = set_actions_top + set_actions_height + gap;
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_ITEMS_LABEL,
        margin,
        items_label_top,
        content_width,
        line_height));
    const int list_top = items_label_top + line_height;

    const int hint_top = std::max(list_top, height - margin - line_height);
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_HINT,
        margin,
        hint_top,
        content_width,
        line_height));
    const int cell_label_width = ScalePaneDip(dialog, 58);
    const int labelled_button_width = std::max(
        0, content_width - cell_label_width - gap);
    const std::array<int, 2U> cell_controls{
        IDC_LIGHT_TABLE_PREVIOUS,
        IDC_LIGHT_TABLE_NEXT};
    const std::size_t cell_rows = PaneButtonRowCount(
        dialog, cell_controls, labelled_button_width, gap);
    const int cell_height = static_cast<int>(cell_rows) * row_height
        + std::max(0, static_cast<int>(cell_rows) - 1) * gap;
    const int cell_top = std::max(list_top, hint_top - gap - cell_height);
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_CELL_LABEL,
        margin,
        cell_top + ScalePaneDip(dialog, 4),
        cell_label_width,
        line_height));
    PlacePaneButtonRows(
        plan,
        cell_controls,
        margin + cell_label_width + gap,
        cell_top,
        labelled_button_width,
        row_height,
        gap);

    const std::array<int, 3U> bulk_controls{
        IDC_LIGHT_TABLE_BULK_PREVIOUS,
        IDC_LIGHT_TABLE_BULK_NEXT,
        IDC_LIGHT_TABLE_BULK_BOTH};
    const std::size_t bulk_rows = PaneButtonRowCount(
        dialog, bulk_controls, labelled_button_width, gap);
    const int bulk_height = static_cast<int>(bulk_rows) * row_height
        + std::max(0, static_cast<int>(bulk_rows) - 1) * gap;
    const int bulk_top = std::max(list_top, cell_top - gap - bulk_height);
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_BULK_LABEL,
        margin,
        bulk_top + ScalePaneDip(dialog, 4),
        cell_label_width,
        line_height));
    PlacePaneButtonRows(
        plan,
        bulk_controls,
        margin + cell_label_width + gap,
        bulk_top,
        labelled_button_width,
        row_height,
        gap);

    const std::array<int, 3U> property_controls{
        IDC_LIGHT_TABLE_ITEM_PROPERTIES,
        IDC_LIGHT_TABLE_ITEM_MOVE,
        IDC_LIGHT_TABLE_ITEM_SWAP};
    const std::size_t property_rows = PaneButtonRowCount(
        dialog, property_controls, content_width, gap);
    const int property_height = static_cast<int>(property_rows) * row_height
        + std::max(0, static_cast<int>(property_rows) - 1) * gap;
    const int property_top = std::max(list_top, bulk_top - gap - property_height);
    PlacePaneButtonRows(
        plan,
        property_controls,
        margin,
        property_top,
        content_width,
        row_height,
        gap);

    const std::array<int, 5U> item_controls{
        IDC_LIGHT_TABLE_ITEM_ADD,
        IDC_LIGHT_TABLE_ITEM_RELOAD,
        IDC_LIGHT_TABLE_ITEM_DELETE,
        IDC_LIGHT_TABLE_ITEM_UP,
        IDC_LIGHT_TABLE_ITEM_DOWN};
    const std::size_t item_rows = PaneButtonRowCount(
        dialog, item_controls, content_width, gap);
    const int item_height = static_cast<int>(item_rows) * row_height
        + std::max(0, static_cast<int>(item_rows) - 1) * gap;
    const int item_actions_top = std::max(
        list_top, property_top - gap - item_height);
    PlacePaneButtonRows(
        plan,
        item_controls,
        margin,
        item_actions_top,
        content_width,
        row_height,
        gap);
    const int list_height = std::max(0, item_actions_top - gap - list_top);
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_ITEMS,
        margin,
        list_top,
        content_width,
        list_height));
    static_cast<void>(plan.PlaceControl(
        IDC_LIGHT_TABLE_EMPTY,
        margin + gap,
        list_top + std::max(0, (list_height - line_height) / 2),
        std::max(0, content_width - gap * 2),
        line_height));
    static_cast<void>(plan.Commit(PaneDialogRepaint::Complete));
}

void SelectSet(HWND dialog, LightTablePaneDialogState& state) noexcept {
    const LRESULT selected = SendDlgItemMessageW(
        dialog, IDC_LIGHT_TABLE_SETS, CB_GETCURSEL, 0, 0);
    if (selected == CB_ERR
        || static_cast<std::size_t>(selected) >= state.view.sets.size()
        || state.select_entry == nullptr) {
        return;
    }
    const auto index = static_cast<std::uint32_t>(selected);
    state.select_entry(
        state.context, true, index, state.view.sets[index].id);
}

void SelectItem(HWND dialog, LightTablePaneDialogState& state) noexcept {
    const LRESULT selected = SendDlgItemMessageW(
        dialog, IDC_LIGHT_TABLE_ITEMS, LB_GETCURSEL, 0, 0);
    if (selected == LB_ERR
        || static_cast<std::size_t>(selected) >= state.view.items.size()
        || state.select_entry == nullptr) {
        return;
    }
    const auto index = static_cast<std::uint32_t>(selected);
    state.select_entry(
        state.context, false, index, state.view.items[index].id);
}

INT_PTR CALLBACK LightTablePaneProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM) noexcept {
    auto* state = reinterpret_cast<LightTablePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            return TRUE;
        case WM_SIZE:
            LayoutLightTablePane(dialog);
            return TRUE;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDC_LIGHT_TABLE_PIN:
                    Dispatch(*state, IDM_LIGHT_TABLE_PIN);
                    return TRUE;
                case IDC_LIGHT_TABLE_SETS:
                    if (HIWORD(wparam) == CBN_SELCHANGE) {
                        SelectSet(dialog, *state);
                    }
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEMS:
                    if (HIWORD(wparam) == LBN_SELCHANGE) {
                        SelectItem(dialog, *state);
                    } else if (HIWORD(wparam) == LBN_DBLCLK) {
                        SelectItem(dialog, *state);
                        Dispatch(*state, IDM_LT_ITEM_SWAP);
                    }
                    return TRUE;
                case IDC_LIGHT_TABLE_SET_NEW:
                    Dispatch(*state, IDM_LT_SET_NEW);
                    return TRUE;
                case IDC_LIGHT_TABLE_SET_DUPLICATE:
                    Dispatch(*state, IDM_LT_SET_DUPLICATE);
                    return TRUE;
                case IDC_LIGHT_TABLE_SET_DELETE:
                    Dispatch(*state, IDM_LT_SET_DELETE);
                    return TRUE;
                case IDC_LIGHT_TABLE_GLOBAL_OPACITY:
                    Dispatch(*state, IDM_LT_GLOBAL_OPACITY);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_ADD:
                    Dispatch(*state, IDM_LT_ITEM_ADD);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_RELOAD:
                    Dispatch(*state, IDM_LT_ITEM_RELOAD);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_DELETE:
                    Dispatch(*state, IDM_LT_ITEM_DELETE);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_UP:
                    Dispatch(*state, IDM_LT_ITEM_UP);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_DOWN:
                    Dispatch(*state, IDM_LT_ITEM_DOWN);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_PROPERTIES:
                    Dispatch(*state, IDM_LT_ITEM_PROPERTIES);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_MOVE:
                    Dispatch(*state, IDM_LT_ITEM_MOVE);
                    return TRUE;
                case IDC_LIGHT_TABLE_ITEM_SWAP:
                    Dispatch(*state, IDM_LT_ITEM_SWAP);
                    return TRUE;
                case IDC_LIGHT_TABLE_BULK_PREVIOUS:
                    Dispatch(*state, IDM_LT_BULK_PREVIOUS);
                    return TRUE;
                case IDC_LIGHT_TABLE_BULK_NEXT:
                    Dispatch(*state, IDM_LT_BULK_NEXT);
                    return TRUE;
                case IDC_LIGHT_TABLE_BULK_BOTH:
                    Dispatch(*state, IDM_LT_BULK_BOTH);
                    return TRUE;
                case IDC_LIGHT_TABLE_PREVIOUS:
                    Dispatch(*state, IDM_SEQ_PREVIOUS);
                    return TRUE;
                case IDC_LIGHT_TABLE_NEXT:
                    Dispatch(*state, IDM_SEQ_NEXT);
                    return TRUE;
                case IDCANCEL:
                    Dispatch(*state, IDM_WINDOW_LIGHT_TABLE);
                    return TRUE;
                default:
                    break;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr) {
                Dispatch(*state, IDM_WINDOW_LIGHT_TABLE);
            }
            return TRUE;
        case WM_NCDESTROY:
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

std::wstring ItemLabel(const LightTablePaneItemView& item) {
    const wchar_t* mode = item.display_mode == INKPOD_LIGHT_TABLE_MONOTONE
        ? UiText(UiStringId::Monochrome)
        : (item.display_mode == INKPOD_LIGHT_TABLE_HALFTONE
               ? UiText(UiStringId::Halftone)
               : UiText(UiStringId::Color));
    std::array<wchar_t, 512U> label{};
    (void)_snwprintf_s(
        label.data(),
        label.size(),
        _TRUNCATE,
        L"%ls  [%ls / %u%%]  X %.1f  Y %.1f%ls",
        item.name.c_str(),
        mode,
        item.opacity_milli / 10U,
        static_cast<double>(item.translate_x_milli) / 1000.0,
        static_cast<double>(item.translate_y_milli) / 1000.0,
        (item.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE) != 0U
            ? L""
             : UiText(UiStringId::HiddenSuffix));
    return label.data();
}

}  // namespace

HWND CreateLightTablePaneDialog(
    HINSTANCE instance, HWND owner, LightTablePaneDialogState& state) noexcept {
    if (state.dispatch_command == nullptr || state.select_entry == nullptr) {
        return nullptr;
    }
    const HWND dialog = CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_LIGHT_TABLE_PALETTE),
        owner,
        LightTablePaneProcedure,
        0);
    if (dialog == nullptr) {
        return nullptr;
    }
    SetWindowLongPtrW(
        dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(&state));
    EnablePaneDialogResizePainting(dialog);
    LayoutLightTablePane(dialog);
    SetWindowTextW(
        GetDlgItem(dialog, IDC_LIGHT_TABLE_SETS),
        UiText(UiStringId::LightTableSetsAccessibleName));
    SetWindowTextW(
        GetDlgItem(dialog, IDC_LIGHT_TABLE_ITEMS),
        UiText(UiStringId::LightTableItemsAccessibleName));
    return dialog;
}

void UpdateLightTablePaneDialog(HWND dialog, LightTablePaneView view) noexcept {
    if (dialog == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<LightTablePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    if (state == nullptr) {
        return;
    }
    state->view = std::move(view);
    SetDlgItemTextW(dialog, IDC_LIGHT_TABLE_TARGET, state->view.target_text.c_str());
    SetDlgItemTextW(
        dialog,
        IDC_LIGHT_TABLE_PIN,
        state->view.pinned ? UiText(UiStringId::ReturnToFollowing)
                           : UiText(UiStringId::PinDocument));
    static_cast<void>(SetPaneIconButton(
        GetDlgItem(dialog, IDC_LIGHT_TABLE_PIN),
        state->view.pinned ? PaneIconId::ReturnToFollowing
                           : PaneIconId::PinDocument));

    const HWND sets = GetDlgItem(dialog, IDC_LIGHT_TABLE_SETS);
    SendMessageW(sets, WM_SETREDRAW, FALSE, 0);
    SendMessageW(sets, CB_RESETCONTENT, 0, 0);
    for (const auto& set : state->view.sets) {
        std::array<wchar_t, 384U> label{};
        (void)_snwprintf_s(
            label.data(),
            label.size(),
            _TRUNCATE,
             L"%ls  (%u %ls / %u%%)",
             set.name.c_str(),
             set.item_count,
             UiText(UiStringId::ItemsLabel),
             set.opacity_milli / 10U);
        (void)SendMessageW(
            sets, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(label.data()));
    }
    if (state->view.selected_set_index != UINT32_MAX
        && state->view.selected_set_index < state->view.sets.size()) {
        SendMessageW(sets, CB_SETCURSEL, state->view.selected_set_index, 0);
    }
    SendMessageW(sets, WM_SETREDRAW, TRUE, 0);
    InvalidateRect(sets, nullptr, TRUE);

    const HWND items = GetDlgItem(dialog, IDC_LIGHT_TABLE_ITEMS);
    SendMessageW(items, WM_SETREDRAW, FALSE, 0);
    SendMessageW(items, LB_RESETCONTENT, 0, 0);
    for (const auto& item : state->view.items) {
        const std::wstring label = ItemLabel(item);
        (void)SendMessageW(
            items, LB_ADDSTRING, 0, reinterpret_cast<LPARAM>(label.c_str()));
    }
    if (state->view.selected_item_index != UINT32_MAX
        && state->view.selected_item_index < state->view.items.size()) {
        SendMessageW(items, LB_SETCURSEL, state->view.selected_item_index, 0);
    }
    SendMessageW(items, WM_SETREDRAW, TRUE, 0);
    InvalidateRect(items, nullptr, TRUE);

    const bool target = state->view.target_available;
    const bool has_set = target && !state->view.sets.empty();
    const bool has_item = target && !state->view.items.empty();
    EnableWindow(GetDlgItem(dialog, IDC_LIGHT_TABLE_PIN), target ? TRUE : FALSE);
    EnableWindow(sets, has_set ? TRUE : FALSE);
    EnableWindow(items, has_item ? TRUE : FALSE);
    for (const int control : {
             IDC_LIGHT_TABLE_SET_NEW,
             IDC_LIGHT_TABLE_PREVIOUS,
             IDC_LIGHT_TABLE_NEXT}) {
        EnableWindow(GetDlgItem(dialog, control), target ? TRUE : FALSE);
    }
    for (const int control : {
             IDC_LIGHT_TABLE_SET_DUPLICATE,
             IDC_LIGHT_TABLE_SET_DELETE,
             IDC_LIGHT_TABLE_GLOBAL_OPACITY,
             IDC_LIGHT_TABLE_ITEM_ADD}) {
        EnableWindow(GetDlgItem(dialog, control), has_set ? TRUE : FALSE);
    }
    for (const int control : {
             IDC_LIGHT_TABLE_ITEM_RELOAD,
             IDC_LIGHT_TABLE_ITEM_DELETE,
             IDC_LIGHT_TABLE_ITEM_UP,
             IDC_LIGHT_TABLE_ITEM_DOWN,
             IDC_LIGHT_TABLE_ITEM_PROPERTIES,
             IDC_LIGHT_TABLE_ITEM_MOVE,
             IDC_LIGHT_TABLE_ITEM_SWAP}) {
        EnableWindow(GetDlgItem(dialog, control), has_item ? TRUE : FALSE);
    }
    SetDlgItemTextW(
        dialog,
        IDC_LIGHT_TABLE_EMPTY,
        has_item ? L"" : state->view.empty_text.c_str());
    LayoutLightTablePane(dialog);
}

}  // namespace inkpod::windows::ui::panes
