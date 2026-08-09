#include "batch_dialog.h"

#include <algorithm>
#include <array>
#include <cstddef>

#include "app/resource.h"
#include "ui/panes/pane_dialog_layout.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT_PTR kBatchRefreshTimer = 1U;

constexpr std::array<BatchPaletteEntry, 24U> kBatchPaletteEntries{{
    {IDM_BATCH_ADD_COLOR_REPLACE, L"色置換"},
    {IDM_BATCH_ADD_CONTINUOUS_FILL, L"連続フィル"},
    {IDM_BATCH_ADD_SEPARATION, L"色分解"},
    {IDM_BATCH_ADD_VISIBILITY, L"レイヤー表示"},
    {IDM_BATCH_ADD_LINE_WIDTH, L"線幅"},
    {IDM_BATCH_ADD_BOUNDARY_AIRBRUSH, L"境界色エアブラシ"},
    {IDM_BATCH_ADD_DUST, L"ゴミ取り"},
    {IDM_BATCH_ADD_MIRROR, L"鏡像"},
    {IDM_BATCH_ADD_ROTATE, L"90度回転"},
    {IDM_BATCH_ADD_RESIZE, L"画像サイズ・解像度"},
    {IDM_BATCH_ADD_CONVERT, L"ラスター変換"},
    {IDM_BATCH_ADD_FILTER_SHARPEN_WEAK, L"フィルタ: シャープ（弱）"},
    {IDM_BATCH_ADD_FILTER_SHARPEN_STRONG, L"フィルタ: シャープ（強）"},
    {IDM_BATCH_ADD_FILTER_BLUR_WEAK, L"フィルタ: ぼかし（弱）"},
    {IDM_BATCH_ADD_FILTER_BLUR_STRONG, L"フィルタ: ぼかし（強）"},
    {IDM_BATCH_ADD_FILTER_GAUSSIAN, L"フィルタ: ガウスぼかし"},
    {IDM_BATCH_ADD_FILTER_INVERT, L"フィルタ: 階調反転"},
    {IDM_BATCH_ADD_FILTER_AUTO_CONTRAST, L"フィルタ: 自動コントラスト"},
    {IDM_BATCH_ADD_FILTER_BRIGHTNESS, L"フィルタ: 明るさ・コントラスト"},
    {IDM_BATCH_ADD_FILTER_TONE_CURVE, L"フィルタ: トーンカーブ"},
    {IDM_BATCH_ADD_FILTER_LEVELS, L"フィルタ: レベル補正"},
    {IDM_BATCH_ADD_FILTER_HSV, L"フィルタ: HSV"},
    {IDM_BATCH_ADD_FILTER_COLOR_BALANCE, L"フィルタ: カラーバランス"},
    {IDM_BATCH_ADD_FILTER_UNSHARP, L"フィルタ: アンシャープ"},
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
    const int pin_width = scale(90);
    const int width = static_cast<int>(client.right - client.left);
    const int height = static_cast<int>(client.bottom - client.top);
    const int content_width = std::max(0, width - margin * 2);

    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_PIN,
        std::max(margin, width - margin - pin_width),
        margin,
        pin_width,
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_TARGET,
        margin,
        margin + scale(4),
        std::max(0, width - margin * 3 - pin_width),
        line_height);
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

    const int bottom_top = std::max(operations_top, height - margin - row_height);
    const int save_width = scale(82);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_SAVE_SET,
        margin,
        bottom_top,
        save_width,
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_LOAD_SET,
        margin + save_width + gap,
        bottom_top,
        save_width,
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDCANCEL,
        std::max(margin, width - margin - save_width),
        bottom_top,
        save_width,
        row_height);

    const int run_top = std::max(operations_top, bottom_top - gap - row_height);
    const std::array<int, 5U> run_controls{
        IDC_BATCH_PREVIEW,
        IDC_BATCH_DRY_RUN,
        IDC_BATCH_RUN_CURRENT,
        IDC_BATCH_RUN_ALL,
        IDC_BATCH_CANCEL};
    const int run_width = std::max(
        0, (content_width - gap * 4) / static_cast<int>(run_controls.size()));
    int cursor = margin;
    for (const int control : run_controls) {
        PlacePaneDialogControl(
            dialog, control, cursor, run_top, run_width, row_height);
        cursor += run_width + gap;
    }

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

    const int reorder_top = std::max(
        operations_top, output_label_top - gap - row_height);
    const int small_button = scale(56);
    cursor = margin;
    for (const int control : {
             IDC_BATCH_UP, IDC_BATCH_DOWN, IDC_BATCH_EDIT}) {
        PlacePaneDialogControl(
            dialog, control, cursor, reorder_top, small_button, row_height);
        cursor += small_button + gap;
    }
    const int add_top = std::max(
        operations_top, reorder_top - gap - row_height);
    const int action_width = scale(56);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_OPERATION_KIND,
        margin,
        add_top,
        std::max(0, content_width - action_width * 2 - gap * 2),
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_ADD,
        std::max(margin, width - margin - action_width * 2 - gap),
        add_top,
        action_width,
        row_height);
    PlacePaneDialogControl(
        dialog,
        IDC_BATCH_REMOVE,
        std::max(margin, width - margin - action_width),
        add_top,
        action_width,
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
    return CreateDialogParamW(
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
        view.pinned ? L"追従へ戻す" : L"文書に固定");
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
