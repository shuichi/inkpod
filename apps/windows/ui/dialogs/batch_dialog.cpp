#include "ui/ui_resources.h"

#include "ui/localization.h"

#include "batch_dialog.h"

#include <algorithm>
#include <array>
#include <cstddef>

#include "app/resource.h"
#include "ui/panes/pane_dialog_layout.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT_PTR kBatchRefreshTimer = 1U;

const std::array<BatchPaletteEntry, 24U> kBatchPaletteEntries{{
    {IDM_BATCH_ADD_COLOR_REPLACE, UiText(UiStringId::ToolColorReplacement)},
    {IDM_BATCH_ADD_CONTINUOUS_FILL, UiText(UiStringId::Text0970)},
    {IDM_BATCH_ADD_SEPARATION, UiText(UiStringId::Text0870)},
    {IDM_BATCH_ADD_VISIBILITY, UiText(UiStringId::Text0406)},
    {IDM_BATCH_ADD_LINE_WIDTH, UiText(UiStringId::Text0848)},
    {IDM_BATCH_ADD_BOUNDARY_AIRBRUSH, UiText(UiStringId::Text0603)},
    {IDM_BATCH_ADD_DUST, UiText(UiStringId::ToolDustRemoval)},
    {IDM_BATCH_ADD_MIRROR, UiText(UiStringId::Text1009)},
    {IDM_BATCH_ADD_ROTATE, UiText(UiStringId::Text0039)},
    {IDM_BATCH_ADD_RESIZE, UiText(UiStringId::Text0804)},
    {IDM_BATCH_ADD_CONVERT, UiText(UiStringId::Text0385)},
    {IDM_BATCH_ADD_FILTER_SHARPEN_WEAK, UiText(UiStringId::Text0293)},
    {IDM_BATCH_ADD_FILTER_SHARPEN_STRONG, UiText(UiStringId::Text0294)},
    {IDM_BATCH_ADD_FILTER_BLUR_WEAK, UiText(UiStringId::Text0288)},
    {IDM_BATCH_ADD_FILTER_BLUR_STRONG, UiText(UiStringId::Text0289)},
    {IDM_BATCH_ADD_FILTER_GAUSSIAN, UiText(UiStringId::Text0292)},
    {IDM_BATCH_ADD_FILTER_INVERT, UiText(UiStringId::Text0299)},
    {IDM_BATCH_ADD_FILTER_AUTO_CONTRAST, UiText(UiStringId::Text0298)},
    {IDM_BATCH_ADD_FILTER_BRIGHTNESS, UiText(UiStringId::Text0297)},
    {IDM_BATCH_ADD_FILTER_TONE_CURVE, UiText(UiStringId::Text0295)},
    {IDM_BATCH_ADD_FILTER_LEVELS, UiText(UiStringId::Text0296)},
    {IDM_BATCH_ADD_FILTER_HSV, UiText(UiStringId::Text0287)},
    {IDM_BATCH_ADD_FILTER_COLOR_BALANCE, UiText(UiStringId::Text0291)},
    {IDM_BATCH_ADD_FILTER_UNSHARP, UiText(UiStringId::Text0290)},
}};

void DispatchCommand(BatchPaletteDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

void LayoutBatchPane(HWND dialog) noexcept {
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
    const int line_height = scale(18);
    const int row_height = scale(26);
    const int width = static_cast<int>(client.right - client.left);
    const int height = static_cast<int>(client.bottom - client.top);
    const int content_width = std::max(0, width - margin * 2);

    panes::PlacePaneTargetRow(
        dialog,
        IDC_BATCH_TARGET,
        IDC_BATCH_PIN,
        margin,
        margin,
        content_width,
        scale(4),
        line_height,
        row_height,
        gap);
    const int job_top = margin + row_height + gap;
    PlacePaneDialogControl(
        dialog, IDC_BATCH_JOB, margin, job_top, content_width, line_height);
    const int input_label_top = job_top + line_height + gap;
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_INPUT_LABEL,
        margin,
        input_label_top,
        content_width,
        line_height);
    const int inputs_top = input_label_top + line_height;
    const int inputs_height = scale(48);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_INPUTS,
        margin,
        inputs_top,
        content_width,
        inputs_height);
    const int operations_label_top = inputs_top + inputs_height + gap;
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OPERATIONS_LABEL,
        margin,
        operations_label_top,
        content_width,
        line_height);
    const int operations_top = operations_label_top + line_height;

    const std::array<int, 3U> bottom_controls{
        IDC_BATCH_SAVE_SET,
        IDC_BATCH_LOAD_SET,
        IDCANCEL};
    const std::size_t bottom_rows = panes::PaneButtonRowCount(
        dialog, bottom_controls, content_width, gap);
    const int bottom_height = static_cast<int>(bottom_rows) * row_height
        + std::max(0, static_cast<int>(bottom_rows) - 1) * gap;
    const int bottom_top = std::max(
        operations_top, height - margin - bottom_height);
    panes::PlacePaneButtonRows(
        dialog,
        bottom_controls,
        margin,
        bottom_top,
        content_width,
        row_height,
        gap);

    const std::array<int, 5U> run_controls{
        IDC_BATCH_PREVIEW,
        IDC_BATCH_DRY_RUN,
        IDC_BATCH_RUN_CURRENT,
        IDC_BATCH_RUN_ALL,
        IDC_BATCH_CANCEL};
    const std::size_t run_rows = panes::PaneButtonRowCount(
        dialog, run_controls, content_width, gap);
    const int run_height = static_cast<int>(run_rows) * row_height
        + std::max(0, static_cast<int>(run_rows) - 1) * gap;
    const int run_top = std::max(
        operations_top, bottom_top - gap - run_height);
    panes::PlacePaneButtonRows(
        dialog,
        run_controls,
        margin,
        run_top,
        content_width,
        row_height,
        gap);

    const int output_height = scale(34);
    const int output_top = std::max(operations_top, run_top - gap - output_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OUTPUT,
        margin,
        output_top,
        content_width,
        output_height);
    const int output_label_top = std::max(
        operations_top, output_top - line_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OUTPUT_LABEL,
        margin,
        output_label_top,
        content_width,
        line_height);

    const std::array<int, 3U> reorder_controls{
        IDC_BATCH_UP,
        IDC_BATCH_DOWN,
        IDC_BATCH_EDIT};
    const std::size_t reorder_rows = panes::PaneButtonRowCount(
        dialog, reorder_controls, content_width, gap);
    const int reorder_height = static_cast<int>(reorder_rows) * row_height
        + std::max(0, static_cast<int>(reorder_rows) - 1) * gap;
    const int reorder_top = std::max(
        operations_top, output_label_top - gap - reorder_height);
    panes::PlacePaneButtonRows(
        dialog,
        reorder_controls,
        margin,
        reorder_top,
        content_width,
        row_height,
        gap);
    const int add_top = std::max(
        operations_top, reorder_top - gap - row_height);
    const int add_width = panes::PaneButtonIdealWidth(dialog, IDC_BATCH_ADD);
    const int remove_width = panes::PaneButtonIdealWidth(dialog, IDC_BATCH_REMOVE);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OPERATION_KIND,
        margin,
        add_top,
        std::max(0, content_width - add_width - remove_width - gap * 2),
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_ADD,
        std::max(margin, width - margin - add_width - remove_width - gap),
        add_top,
        add_width,
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_REMOVE,
        std::max(margin, width - margin - remove_width),
        add_top,
        remove_width,
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OPERATIONS,
        margin,
        operations_top,
        content_width,
        std::max(0, add_top - gap - operations_top));
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
                || state->refresh == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND combo = GetDlgItem(dialog, IDC_BATCH_OPERATION_KIND);
            for (const auto& entry : kBatchPaletteEntries) {
                SendMessageW(
                    combo,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(entry.label));
            }
            SendMessageW(combo, CB_SETCURSEL, 0, 0);
            LayoutBatchPane(dialog);
            return TRUE;
        }
        case WM_SIZE:
            LayoutBatchPane(dialog);
            return TRUE;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDC_BATCH_PIN:
                    DispatchCommand(*state, IDM_BATCH_PIN);
                    return TRUE;
                case IDC_BATCH_ADD: {
                    const LRESULT index = SendDlgItemMessageW(
                        dialog, IDC_BATCH_OPERATION_KIND, CB_GETCURSEL, 0, 0);
                    if (index >= 0
                        && static_cast<std::size_t>(index)
                            < kBatchPaletteEntries.size()) {
                        DispatchCommand(
                            *state,
                            kBatchPaletteEntries[static_cast<std::size_t>(index)]
                                .command);
                    }
                    return TRUE;
                }
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
                    DispatchCommand(*state, IDM_BATCH_OPERATION_EDIT);
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
                case IDC_BATCH_OPERATIONS:
                    if (HIWORD(wparam) == LBN_SELCHANGE && !state->loaded_graph) {
                        const LRESULT index = SendDlgItemMessageW(
                            dialog, IDC_BATCH_OPERATIONS, LB_GETCURSEL, 0, 0);
                        if (index >= 0) {
                            state->select_operation(
                                state->context,
                                static_cast<std::uint32_t>(index));
                        }
                    }
                    return TRUE;
                case IDCANCEL:
                    DispatchCommand(*state, IDM_WINDOW_BATCH);
                    return TRUE;
                default:
                    break;
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
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

const std::array<BatchPaletteEntry, 24U>& BatchPaletteEntries() noexcept {
    return kBatchPaletteEntries;
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
    const HWND inputs = GetDlgItem(dialog, IDC_BATCH_INPUTS);
    const HWND operations = GetDlgItem(dialog, IDC_BATCH_OPERATIONS);
    if (inputs == nullptr || operations == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<BatchPaletteDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    if (state != nullptr) {
        state->loaded_graph = view.loaded_graph;
    }

    SetDlgItemTextW(dialog, IDC_BATCH_TARGET, view.target_text.c_str());
    SetDlgItemTextW(dialog, IDC_BATCH_JOB, view.job_text.c_str());
    SetDlgItemTextW(
        dialog,
        IDC_BATCH_PIN,
        view.pinned ? UiText(UiStringId::ReturnToFollowing) : UiText(UiStringId::PinDocument));
    EnableWindow(
        GetDlgItem(dialog, IDC_BATCH_PIN),
        view.target_available && view.idle ? TRUE : FALSE);
    if (view.idle) {
        KillTimer(dialog, kBatchRefreshTimer);
    } else {
        SetTimer(dialog, kBatchRefreshTimer, 250U, nullptr);
    }

    SendMessageW(inputs, LB_RESETCONTENT, 0, 0);
    SendMessageW(
        inputs,
        LB_ADDSTRING,
        0,
        reinterpret_cast<LPARAM>(view.input_label.c_str()));

    SendMessageW(operations, LB_RESETCONTENT, 0, 0);
    for (const auto& label : view.operation_labels) {
        SendMessageW(
            operations,
            LB_ADDSTRING,
            0,
            reinterpret_cast<LPARAM>(label.c_str()));
    }
    if (!view.operation_labels.empty() && !view.loaded_graph) {
        SendMessageW(
            operations,
            LB_SETCURSEL,
            view.selected_operation,
            0);
    }

    SetDlgItemTextW(dialog, IDC_BATCH_OUTPUT, view.output_text.c_str());
    const bool editable = view.idle && !view.loaded_graph;
    for (const int control : {
             IDC_BATCH_OPERATION_KIND,
             IDC_BATCH_ADD,
             IDC_BATCH_REMOVE,
             IDC_BATCH_UP,
             IDC_BATCH_DOWN,
             IDC_BATCH_EDIT}) {
        EnableWindow(GetDlgItem(dialog, control), editable ? TRUE : FALSE);
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
}

}  // namespace inkpod::windows::ui
