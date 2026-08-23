#include "ui/ui_resources.h"

#include "ui/localization.h"

#include "batch_dialog.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <new>

#include "app/frontend_state.h"
#include "app/resource.h"
#include "ui/panes/pane_dialog_layout.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT_PTR kBatchRefreshTimer = 1U;
constexpr UINT kBatchStageStateChanged = WM_APP + 0x41U;

constexpr std::array<BatchPaletteEntry, 4U> kBatchPaletteEntries{{
    {IDM_BATCH_ADD_COLOR_REPLACE, UiStringId::ToolColorReplacement},
    {IDM_BATCH_ADD_MOVE_TO_COLOR_PLANE, UiStringId::BatchMoveToColorPlane},
    {IDM_BATCH_ADD_MASKING, UiStringId::BatchMasking},
    {IDM_BATCH_ADD_ERASE, UiStringId::BatchErase},
}};

void DispatchCommand(BatchPaletteDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

void LayoutBatchPane(HWND dialog, BatchPaletteDialogState* state) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    using panes::PlacePaneDialogControl;
    const auto scale = [dialog](int value) {
        return panes::ScalePaneDip(dialog, value);
    };
    const int margin = scale(8);
    const int gap = scale(6);
    const int line = scale(18);
    const int row = scale(26);
    const int width = static_cast<int>(client.right - client.left);
    const int height = static_cast<int>(client.bottom - client.top);
    const int content = std::max(0, width - margin * 2);

    int top = margin;
    PlacePaneDialogControl(dialog, IDC_BATCH_JOB, margin, top, content, line);
    top += line + gap;
    PlacePaneDialogControl(
        dialog, IDC_BATCH_INPUT_LABEL, margin, top, content, line);
    top += line;
    PlacePaneDialogControl(dialog, IDC_BATCH_INPUTS, margin, top, content, row);
    top += row + gap;
    PlacePaneDialogControl(
        dialog, IDC_BATCH_OPERATIONS_LABEL, margin, top, content, line);
    top += line;
    const int stage_height = std::max(scale(92), height / 5);
    PlacePaneDialogControl(
        dialog, IDC_BATCH_OPERATIONS, margin, top, content, stage_height);
    const HWND stages = GetDlgItem(dialog, IDC_BATCH_OPERATIONS);
    RECT stage_client{};
    if (stages != nullptr && GetClientRect(stages, &stage_client) != FALSE) {
        ListView_SetColumnWidth(stages, 0, stage_client.right - stage_client.left);
    }
    top += stage_height + gap;

    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_ADD,
        margin,
        top,
        content,
        row);
    top += row + gap;
    const std::array<int, 4U> edit_controls{
        IDC_BATCH_EDIT, IDC_BATCH_REMOVE, IDC_BATCH_UP, IDC_BATCH_DOWN};
    panes::PlacePaneButtonRows(
        dialog, edit_controls, margin, top, content, row, gap);
    const std::size_t edit_rows = panes::PaneButtonRowCount(
        dialog, edit_controls, content, gap);
    top += static_cast<int>(edit_rows) * row
        + static_cast<int>(edit_rows) * gap;

    const std::array<int, 3U> bottom_controls{
        IDC_BATCH_SAVE_SET, IDC_BATCH_LOAD_SET, IDCANCEL};
    const std::size_t bottom_rows = panes::PaneButtonRowCount(
        dialog, bottom_controls, content, gap);
    const int bottom_height = static_cast<int>(bottom_rows) * row
        + std::max(0, static_cast<int>(bottom_rows) - 1) * gap;
    const int bottom_top = std::max(top, height - margin - bottom_height);
    panes::PlacePaneButtonRows(
        dialog, bottom_controls, margin, bottom_top, content, row, gap);

    const std::array<int, 5U> run_controls{
        IDC_BATCH_PREVIEW,
        IDC_BATCH_DRY_RUN,
        IDC_BATCH_RUN_CURRENT,
        IDC_BATCH_RUN_ALL,
        IDC_BATCH_CANCEL};
    const std::size_t run_rows = panes::PaneButtonRowCount(
        dialog, run_controls, content, gap);
    const int run_height = static_cast<int>(run_rows) * row
        + std::max(0, static_cast<int>(run_rows) - 1) * gap;
    const int run_top = std::max(top, bottom_top - gap - run_height);
    panes::PlacePaneButtonRows(
        dialog, run_controls, margin, run_top, content, row, gap);

    const int validation_height = scale(40);
    const int validation_top = std::max(top, run_top - gap - validation_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OUTPUT,
        margin,
        validation_top,
        content,
        validation_height);
    const int validation_label_top = std::max(top, validation_top - line);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OUTPUT_LABEL,
        margin,
        validation_label_top,
        content,
        line);
    if (state != nullptr && state->parameter_host != nullptr) {
        MoveWindow(
            state->parameter_host,
            margin,
            top,
            content,
            std::max(0, validation_label_top - gap - top),
            TRUE);
    }
}

void InitializeStageList(HWND list) noexcept {
    const LONG_PTR style = GetWindowLongPtrW(list, GWL_STYLE);
    SetWindowLongPtrW(list, GWL_STYLE, style | LVS_NOCOLUMNHEADER);
    ListView_SetExtendedListViewStyle(
        list,
        LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP
            | LVS_EX_CHECKBOXES);
    LVCOLUMNW column{};
    column.mask = LVCF_TEXT | LVCF_WIDTH;
    column.pszText = const_cast<wchar_t*>(L"");
    column.cx = 320;
    ListView_InsertColumn(list, 0, &column);
}

LRESULT CALLBACK StageListProcedure(
    HWND list,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR subclass_id,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<BatchPaletteDialogState*>(reference);
    if (message == WM_KEYDOWN && wparam == VK_SPACE && state != nullptr
        && !state->updating) {
        const int selected = ListView_GetNextItem(list, -1, LVNI_SELECTED);
        const auto operation_count =
            state->parameter_editor.draft->operations.size();
        if (selected > 0 && selected <= static_cast<int>(operation_count)) {
            ListView_SetCheckState(
                list,
                selected,
                ListView_GetCheckState(list, selected) == FALSE);
            return 0;
        }
    }
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(list, StageListProcedure, subclass_id);
    }
    return DefSubclassProc(list, message, wparam, lparam);
}

void ShowOperationMenu(HWND dialog, BatchPaletteDialogState& state) noexcept {
    const HMENU menu = CreatePopupMenu();
    if (menu == nullptr) {
        return;
    }
    bool complete = true;
    for (const auto& entry : kBatchPaletteEntries) {
        if (AppendMenuW(
                menu,
                MF_STRING,
                entry.command,
                BatchPaletteEntryLabel(entry))
            == FALSE) {
            complete = false;
            break;
        }
    }
    RECT anchor{};
    const HWND button = GetDlgItem(dialog, IDC_BATCH_ADD);
    if (complete && button != nullptr && GetWindowRect(button, &anchor) != FALSE) {
        const UINT command = static_cast<UINT>(TrackPopupMenuEx(
            menu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            anchor.left,
            anchor.bottom,
            dialog,
            nullptr));
        if (command != 0U) {
            DispatchCommand(state, command);
        }
    }
    DestroyMenu(menu);
}

void CommitSetName(HWND dialog, BatchPaletteDialogState& state) noexcept {
    const HWND combo = GetDlgItem(dialog, IDC_BATCH_INPUTS);
    const int length = GetWindowTextLengthW(combo);
    std::wstring value;
    try {
        value.resize(
            static_cast<std::size_t>(std::max(0, length)) + 1U, L'\0');
        if (length > 0) {
            const int copied = GetWindowTextW(combo, value.data(), length + 1);
            value.resize(static_cast<std::size_t>(std::max(0, copied)));
        } else {
            value.clear();
        }
        state.parameter_editor.draft->set_name = std::move(value);
    } catch (const std::bad_alloc&) {
        return;
    }
    if (state.parameter_editor.changed != nullptr) {
        state.parameter_editor.changed(state.parameter_editor.context);
    }
}

INT_PTR CALLBACK BatchPaletteDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<BatchPaletteDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<BatchPaletteDialogState*>(lparam);
            if (state == nullptr || state->dispatch_command == nullptr
                || state->select_operation == nullptr
                || state->refresh == nullptr
                || state->parameter_editor.draft == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND stages = GetDlgItem(dialog, IDC_BATCH_OPERATIONS);
            InitializeStageList(stages);
            SetWindowSubclass(
                stages,
                StageListProcedure,
                1U,
                reinterpret_cast<DWORD_PTR>(state));
            state->parameter_host = CreateBatchParameterEditor(
                reinterpret_cast<HINSTANCE>(
                    GetWindowLongPtrW(dialog, GWLP_HINSTANCE)),
                dialog,
                state->parameter_editor);
            SetWindowTextW(
                GetDlgItem(dialog, IDC_BATCH_INPUT_LABEL),
                UiText(UiStringId::BatchSetName));
            SetWindowTextW(
                GetDlgItem(dialog, IDC_BATCH_OPERATIONS_LABEL),
                UiText(UiStringId::BatchStages));
            SetWindowTextW(
                GetDlgItem(dialog, IDC_BATCH_OUTPUT_LABEL),
                UiText(UiStringId::BatchValidation));
            SetWindowTextW(
                GetDlgItem(dialog, IDC_BATCH_ADD),
                UiText(UiStringId::BatchAddOperation));
            SetWindowTextW(
                GetDlgItem(dialog, IDC_BATCH_EDIT),
                UiText(UiStringId::BatchDuplicateOperation));
            LayoutBatchPane(dialog, state);
            return TRUE;
        }
        case WM_SIZE:
            LayoutBatchPane(dialog, state);
            return TRUE;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDC_BATCH_ADD:
                    ShowOperationMenu(dialog, *state);
                    return TRUE;
                case IDC_BATCH_REMOVE:
                    DispatchCommand(*state, IDM_BATCH_OPERATION_REMOVE);
                    return TRUE;
                case IDC_BATCH_UP:
                    DispatchCommand(*state, IDM_BATCH_OPERATION_UP);
                    return TRUE;
                case IDC_BATCH_DOWN:
                    DispatchCommand(*state, IDM_BATCH_OPERATION_DOWN);
                    return TRUE;
                case IDC_BATCH_EDIT:
                    DispatchCommand(*state, IDM_BATCH_OPERATION_DUPLICATE);
                    return TRUE;
                case IDC_BATCH_PREVIEW:
                    DispatchCommand(*state, IDM_BATCH_PREVIEW);
                    return TRUE;
                case IDC_BATCH_DRY_RUN:
                    DispatchCommand(*state, IDM_BATCH_DRY_RUN);
                    return TRUE;
                case IDC_BATCH_RUN_CURRENT:
                    DispatchCommand(*state, IDM_BATCH_RUN_CURRENT);
                    return TRUE;
                case IDC_BATCH_RUN_ALL:
                    DispatchCommand(*state, IDM_BATCH_RUN_ALL);
                    return TRUE;
                case IDC_BATCH_SAVE_SET:
                    DispatchCommand(*state, IDM_BATCH_SAVE_SET);
                    return TRUE;
                case IDC_BATCH_LOAD_SET:
                    DispatchCommand(*state, IDM_BATCH_LOAD_SET);
                    return TRUE;
                case IDC_BATCH_CANCEL:
                    DispatchCommand(*state, IDM_BATCH_CANCEL);
                    return TRUE;
                case IDC_BATCH_INPUTS:
                    if (HIWORD(wparam) == CBN_SELCHANGE
                        || HIWORD(wparam) == CBN_KILLFOCUS) {
                        CommitSetName(dialog, *state);
                    }
                    return TRUE;
                case IDCANCEL:
                    DispatchCommand(*state, IDM_WINDOW_BATCH);
                    return TRUE;
                default:
                    break;
            }
            break;
        case WM_NOTIFY:
            if (state != nullptr && !state->updating
                && reinterpret_cast<NMHDR*>(lparam)->idFrom
                    == IDC_BATCH_OPERATIONS
                && reinterpret_cast<NMHDR*>(lparam)->code == LVN_ITEMCHANGED) {
                const auto* changed = reinterpret_cast<NMLISTVIEW*>(lparam);
                const auto operation_count =
                    state->parameter_editor.draft->operations.size();
                const bool operation_row = changed->iItem > 0
                    && changed->iItem <= static_cast<int>(operation_count);
                if (((changed->uOldState ^ changed->uNewState)
                     & LVIS_STATEIMAGEMASK)
                    != 0U) {
                    if (operation_row) {
                        auto& operation =
                            state->parameter_editor.draft->operations[
                                static_cast<std::size_t>(changed->iItem - 1)];
                        const bool enabled =
                            ((changed->uNewState & LVIS_STATEIMAGEMASK) >> 12U)
                            == 2U;
                        const auto previous = operation.flags;
                        if (enabled) {
                            operation.flags |= INKPOD_BATCH_OPERATION_ENABLED;
                        } else {
                            operation.flags &= ~INKPOD_BATCH_OPERATION_ENABLED;
                        }
                        if (operation.flags != previous) {
                            PostMessageW(
                                dialog, kBatchStageStateChanged, 0, 0);
                        }
                    } else if (changed->iItem >= 0) {
                        state->updating = true;
                        ListView_SetItemState(
                            GetDlgItem(dialog, IDC_BATCH_OPERATIONS),
                            changed->iItem,
                            0,
                            LVIS_STATEIMAGEMASK);
                        state->updating = false;
                    }
                }
                if ((changed->uNewState & LVIS_SELECTED) != 0U
                    && changed->iItem >= 0) {
                    state->select_operation(
                        state->context,
                        static_cast<std::uint32_t>(changed->iItem));
                }
                return TRUE;
            }
            break;
        case kBatchStageStateChanged:
            if (state != nullptr
                && state->parameter_editor.changed != nullptr) {
                state->parameter_editor.changed(
                    state->parameter_editor.context);
                return TRUE;
            }
            break;
        case WM_TIMER:
            if (state != nullptr && wparam == kBatchRefreshTimer) {
                state->refresh(state->context);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr) {
                DispatchCommand(*state, IDM_WINDOW_BATCH);
            }
            return TRUE;
        case WM_NCDESTROY:
            KillTimer(dialog, kBatchRefreshTimer);
            if (state != nullptr) {
                state->parameter_host = nullptr;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

const std::array<BatchPaletteEntry, 4U>& BatchPaletteEntries() noexcept {
    return kBatchPaletteEntries;
}

const wchar_t* BatchPaletteEntryLabel(
    const BatchPaletteEntry& entry) noexcept {
    return UiText(entry.label_id);
}

HWND CreateBatchPaletteDialog(
    HINSTANCE instance, HWND owner, BatchPaletteDialogState& state) noexcept {
    return CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_BATCH_PALETTE),
        owner,
        BatchPaletteDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

void UpdateBatchPaletteDialog(
    HWND dialog, const BatchPaletteView& view) noexcept {
    if (dialog == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<BatchPaletteDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    const HWND stages = GetDlgItem(dialog, IDC_BATCH_OPERATIONS);
    if (state == nullptr || stages == nullptr) {
        return;
    }
    SetDlgItemTextW(dialog, IDC_BATCH_JOB, view.job_text.c_str());
    const HWND set_combo = GetDlgItem(dialog, IDC_BATCH_INPUTS);
    const HWND focus = GetFocus();
    if (set_combo != nullptr
        && focus != set_combo
        && (focus == nullptr || IsChild(set_combo, focus) == FALSE)) {
        SendMessageW(set_combo, CB_RESETCONTENT, 0, 0);
        for (const auto& name : view.set_names) {
            SendMessageW(
                set_combo,
                CB_ADDSTRING,
                0,
                reinterpret_cast<LPARAM>(name.c_str()));
        }
        const LRESULT selected = SendMessageW(
            set_combo,
            CB_FINDSTRINGEXACT,
            static_cast<WPARAM>(-1),
            reinterpret_cast<LPARAM>(view.set_name.c_str()));
        if (selected == CB_ERR) {
            SetWindowTextW(set_combo, view.set_name.c_str());
        } else {
            SendMessageW(set_combo, CB_SETCURSEL, selected, 0);
        }
    }
    if (view.idle) {
        KillTimer(dialog, kBatchRefreshTimer);
    } else {
        SetTimer(dialog, kBatchRefreshTimer, 250U, nullptr);
    }

    state->updating = true;
    ListView_DeleteAllItems(stages);
    for (std::size_t index = 0U; index < view.stage_labels.size(); ++index) {
        LVITEMW item{};
        item.mask = LVIF_TEXT | LVIF_STATE;
        item.iItem = static_cast<int>(index);
        item.pszText = const_cast<wchar_t*>(view.stage_labels[index].c_str());
        item.stateMask = LVIS_STATEIMAGEMASK;
        if (index > 0U && index + 1U < view.stage_labels.size()) {
            const auto& operation = state->parameter_editor.draft->operations[
                index - 1U];
            item.state = INDEXTOSTATEIMAGEMASK(
                (operation.flags & INKPOD_BATCH_OPERATION_ENABLED) != 0U
                    ? 2U
                    : 1U);
        }
        const int inserted = ListView_InsertItem(stages, &item);
        if (inserted >= 0) {
            ListView_SetItemState(
                stages, inserted, item.state, LVIS_STATEIMAGEMASK);
        }
    }
    if (view.selected_stage < view.stage_labels.size()) {
        ListView_SetItemState(
            stages,
            static_cast<int>(view.selected_stage),
            LVIS_SELECTED | LVIS_FOCUSED,
            LVIS_SELECTED | LVIS_FOCUSED);
    }
    state->updating = false;
    SetDlgItemTextW(
        dialog,
        IDC_BATCH_OUTPUT,
        view.validation_text.empty()
            ? UiText(UiStringId::BatchNoValidationIssues)
            : view.validation_text.c_str());

    const bool operation_selected = view.selected_stage > 0U
        && view.selected_stage + 1U < view.stage_labels.size();
    const bool editable = view.idle;
    EnableWindow(GetDlgItem(dialog, IDC_BATCH_INPUTS), editable ? TRUE : FALSE);
    EnableWindow(GetDlgItem(dialog, IDC_BATCH_ADD), editable ? TRUE : FALSE);
    for (const int control : {
             IDC_BATCH_REMOVE, IDC_BATCH_UP, IDC_BATCH_DOWN, IDC_BATCH_EDIT}) {
        EnableWindow(
            GetDlgItem(dialog, control),
            editable && operation_selected ? TRUE : FALSE);
    }
    for (const int control : {
             IDC_BATCH_PREVIEW,
             IDC_BATCH_DRY_RUN,
             IDC_BATCH_RUN_CURRENT,
             IDC_BATCH_RUN_ALL,
             IDC_BATCH_SAVE_SET,
             IDC_BATCH_LOAD_SET}) {
        EnableWindow(
            GetDlgItem(dialog, control),
            (control == IDC_BATCH_LOAD_SET ? view.idle : view.runnable)
                ? TRUE
                : FALSE);
    }
    EnableWindow(
        GetDlgItem(dialog, IDC_BATCH_CANCEL), view.idle ? FALSE : TRUE);
    UpdateBatchParameterEditor(
        state->parameter_host, view.selected_stage, editable);
    LayoutBatchPane(dialog, state);
}

}  // namespace inkpod::windows::ui
