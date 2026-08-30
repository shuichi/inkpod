#include "batch_parameter_editor.h"

#include <commctrl.h>
#include <commdlg.h>

#include <algorithm>
#include <array>
#include <cwchar>
#include <new>
#include <string>

#include "app/frontend_state.h"
#include "app/resource.h"
#include "ui/batch_color_editor_model.h"
#include "ui/batch_input_picker.h"
#include "ui/localization.h"

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kEditorClassName[] = L"InkpodBatchParameterEditor";
constexpr int kTitle = 1;
constexpr int kPrimary = 2;
constexpr int kSecondary = 3;
constexpr int kPathLabel = 4;
constexpr int kPath = 5;
constexpr int kTemplateLabel = 6;
constexpr int kTemplate = 7;
constexpr int kBrowse = 8;
constexpr int kFirstLabel = 9;
constexpr int kFirst = 10;
constexpr int kLastLabel = 11;
constexpr int kLast = 12;
constexpr int kRows = 13;
constexpr int kAdd = 14;
constexpr int kRemove = 15;
constexpr int kSwap = 16;
constexpr int kCurrent = 17;
constexpr int kOldSwatch = 18;
constexpr int kNewSwatch = 19;
constexpr int kPrimaryAlphaLabel = 20;
constexpr int kPrimaryAlpha = 21;
constexpr int kSecondaryAlphaLabel = 22;
constexpr int kSecondaryAlpha = 23;

struct ColorReplaceTargetOption {
    InkpodTypedPlaneKind plane_kind;
    UiStringId label;
};

constexpr std::array<ColorReplaceTargetOption, 2U> kColorReplaceTargets{{
    {INKPOD_TYPED_PLANE_RASTER, UiStringId::PlaneRaster},
    {INKPOD_TYPED_PLANE_COLOR, UiStringId::Coloring},
}};

struct EditorState {
    BatchParameterEditorBinding* binding{};
    std::uint32_t stage{};
    bool updating{};
    bool enabled{true};
    std::size_t selected_input{};
    std::size_t selected_color_row{};
    int scroll_y{};
    HWND title{};
    HWND primary{};
    HWND secondary{};
    HWND path_label{};
    HWND path{};
    HWND template_label{};
    HWND naming_template{};
    HWND browse{};
    HWND first_label{};
    HWND first{};
    HWND last_label{};
    HWND last{};
    HWND rows{};
    HWND add{};
    HWND remove{};
    HWND swap{};
    HWND current{};
    HWND old_swatch{};
    HWND new_swatch{};
    HWND primary_alpha_label{};
    HWND primary_alpha{};
    HWND secondary_alpha_label{};
    HWND secondary_alpha{};
    HWND target_label{};
    HWND target{};
    HWND target_color{};
    HWND target_fixed{};
};

int Scale(HWND window, int dip) noexcept {
    return MulDiv(dip, static_cast<int>(GetDpiForWindow(window)), 96);
}

HWND Child(
    HWND parent,
    DWORD ex_style,
    const wchar_t* class_name,
    const wchar_t* text,
    DWORD style,
    int id) noexcept {
    return CreateWindowExW(
        ex_style,
        class_name,
        text,
        WS_CHILD | style,
        0,
        0,
        0,
        0,
        parent,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)),
        reinterpret_cast<HINSTANCE>(GetWindowLongPtrW(parent, GWLP_HINSTANCE)),
        nullptr);
}

void Show(HWND control, bool visible) noexcept {
    ShowWindow(control, visible ? SW_SHOWNA : SW_HIDE);
}

void SetText(HWND control, const std::wstring& value) noexcept {
    SetWindowTextW(control, value.c_str());
}

std::wstring GetText(HWND control) {
    const int length = GetWindowTextLengthW(control);
    std::wstring value(
        static_cast<std::size_t>(std::max(0, length)) + 1U, L'\0');
    if (length > 0) {
        const int copied = GetWindowTextW(control, value.data(), length + 1);
        value.resize(static_cast<std::size_t>(std::max(0, copied)));
    } else {
        value.clear();
    }
    return value;
}

std::wstring Number(std::uint64_t value) {
    return std::to_wstring(value);
}

bool ReadNumber(HWND control, std::uint64_t& value) noexcept {
    std::array<wchar_t, 32U> text{};
    if (GetWindowTextW(control, text.data(), static_cast<int>(text.size())) <= 0) {
        return false;
    }
    wchar_t* end{};
    const unsigned long long parsed = wcstoull(text.data(), &end, 10);
    if (end == text.data() || *end != L'\0') {
        return false;
    }
    value = static_cast<std::uint64_t>(parsed);
    return true;
}

std::wstring ColorText(const InkpodColorValue& color) {
    return std::to_wstring(color.depth) + L" / "
        + std::to_wstring(color.red) + L", "
        + std::to_wstring(color.green) + L", "
        + std::to_wstring(color.blue) + L", "
        + std::to_wstring(color.alpha);
}

std::size_t SelectedRow(const EditorState& state) noexcept {
    const int index = ListView_GetNextItem(state.rows, -1, LVNI_SELECTED);
    return index < 0 ? state.selected_color_row
                     : static_cast<std::size_t>(index);
}

void Changed(EditorState& state) noexcept {
    if (state.binding != nullptr && state.binding->changed != nullptr) {
        state.binding->changed(state.binding->context);
    }
}

void ResetColumns(HWND list, std::initializer_list<const wchar_t*> labels) noexcept {
    while (Header_GetItemCount(ListView_GetHeader(list)) > 0) {
        ListView_DeleteColumn(list, 0);
    }
    int index = 0;
    for (const wchar_t* label : labels) {
        LVCOLUMNW column{};
        column.mask = LVCF_TEXT | LVCF_WIDTH | LVCF_SUBITEM;
        column.pszText = const_cast<wchar_t*>(label);
        column.cx = 100;
        column.iSubItem = index;
        ListView_InsertColumn(list, index, &column);
        ++index;
    }
}

void AddCell(HWND list, int row, int column, const std::wstring& value) noexcept {
    if (column == 0) {
        LVITEMW item{};
        item.mask = LVIF_TEXT;
        item.iItem = row;
        item.pszText = const_cast<wchar_t*>(value.c_str());
        ListView_InsertItem(list, &item);
    } else {
        ListView_SetItemText(
            list, row, column, const_cast<wchar_t*>(value.c_str()));
    }
}

const wchar_t* InputKindLabel(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_BATCH_INPUT_FILE:
            return UiText(UiStringId::BatchMultipleFiles);
        case INKPOD_BATCH_INPUT_FOLDER:
            return UiText(UiStringId::BatchFolder);
        default:
            return UiText(UiStringId::BatchActiveDocument);
    }
}

void PopulateInputRows(EditorState& state, app::BatchUiState& draft) noexcept {
    ResetColumns(
        state.rows,
        {UiText(UiStringId::BatchInput), UiText(UiStringId::BatchFolder),
         UiText(UiStringId::Text0234), UiText(UiStringId::BatchValidation)});
    ListView_DeleteAllItems(state.rows);
    for (std::size_t index = 0U; index < draft.inputs.size(); ++index) {
        const auto& input = draft.inputs[index];
        AddCell(state.rows, static_cast<int>(index), 0, InputKindLabel(input.kind));
        AddCell(state.rows, static_cast<int>(index), 1, input.path);
        const std::wstring range = std::to_wstring(input.first_cell) + L"–"
            + std::to_wstring(input.last_cell);
        AddCell(state.rows, static_cast<int>(index), 2, range);
        AddCell(
            state.rows,
            static_cast<int>(index),
            3,
            input.validation_text.empty()
                ? UiText(UiStringId::BatchNoValidationIssues)
                : input.validation_text);
    }
    if (!draft.inputs.empty()) {
        state.selected_input = std::min(
            state.selected_input, draft.inputs.size() - 1U);
        ListView_SetItemState(
            state.rows,
            static_cast<int>(state.selected_input),
            LVIS_SELECTED | LVIS_FOCUSED,
            LVIS_SELECTED | LVIS_FOCUSED);
    }
}

void PopulateOperationRows(EditorState& state, app::BatchOperationUi& operation) noexcept {
    ListView_DeleteAllItems(state.rows);
    if (operation.kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        ResetColumns(
            state.rows,
            {UiText(UiStringId::BatchEnabled), UiText(UiStringId::Text0723),
             UiText(UiStringId::Text0705)});
        for (std::size_t index = 0U; index < operation.color_pairs.size(); ++index) {
            const auto& pair = operation.color_pairs[index];
            AddCell(
                state.rows,
                static_cast<int>(index),
                0,
                pair.enabled != 0U ? L"✓" : L"–");
            AddCell(
                state.rows,
                static_cast<int>(index),
                1,
                ColorText(pair.old_color));
            AddCell(
                state.rows,
                static_cast<int>(index),
                2,
                ColorText(pair.new_color));
        }
    } else {
        ResetColumns(state.rows, {UiText(UiStringId::BatchColors)});
        for (std::size_t index = 0U; index < operation.colors.size(); ++index) {
            AddCell(
                state.rows,
                static_cast<int>(index),
                0,
                ColorText(operation.colors[index]));
        }
    }
    const int row_count = ListView_GetItemCount(state.rows);
    if (row_count > 0) {
        state.selected_color_row = std::min(
            state.selected_color_row,
            static_cast<std::size_t>(row_count - 1));
        ListView_SetItemState(
            state.rows,
            static_cast<int>(state.selected_color_row),
            LVIS_SELECTED | LVIS_FOCUSED,
            LVIS_SELECTED | LVIS_FOCUSED);
    } else {
        state.selected_color_row = 0U;
    }
}

void ResizeRowColumns(EditorState& state) noexcept {
    RECT client{};
    if (GetClientRect(state.rows, &client) == FALSE) {
        return;
    }
    const int width = std::max(0, static_cast<int>(client.right - client.left));
    const std::size_t operation_count = state.binding == nullptr
            || state.binding->draft == nullptr
        ? 0U
        : state.binding->draft->operations.size();
    if (state.stage == 0U) {
        const int kind = width * 20 / 100;
        const int path = width * 42 / 100;
        const int range = width * 16 / 100;
        ListView_SetColumnWidth(state.rows, 0, kind);
        ListView_SetColumnWidth(state.rows, 1, path);
        ListView_SetColumnWidth(state.rows, 2, range);
        ListView_SetColumnWidth(state.rows, 3, std::max(0, width - kind - path - range));
        return;
    }
    if (state.stage == 0U || state.stage > operation_count) {
        return;
    }
    const auto& operation = state.binding->draft->operations[
        static_cast<std::size_t>(state.stage - 1U)];
    if (operation.kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        const int enabled = std::min(Scale(state.rows, 58), width / 4);
        const int colors = std::max(0, width - enabled);
        const int old_color = colors / 2;
        ListView_SetColumnWidth(state.rows, 0, enabled);
        ListView_SetColumnWidth(state.rows, 1, old_color);
        ListView_SetColumnWidth(
            state.rows, 2, std::max(0, colors - old_color));
    } else {
        ListView_SetColumnWidth(state.rows, 0, width);
    }
}

void Layout(EditorState& state, HWND window) noexcept {
    RECT client{};
    if (GetClientRect(window, &client) == FALSE) {
        return;
    }
    const int margin = Scale(window, 8);
    const int gap = Scale(window, 6);
    const int row = Scale(window, 25);
    const int width = std::max(
        0, static_cast<int>(client.right - client.left) - margin * 2);
    int y = margin - state.scroll_y;
    const auto place = [&](HWND control, int x, int top, int cx, int cy) {
        MoveWindow(control, x, top, std::max(0, cx), std::max(0, cy), TRUE);
    };
    place(state.title, margin, y, width, row);
    y += row + gap;
    const int half = std::max(0, (width - gap) / 2);
    const std::size_t operation_count = state.binding == nullptr
            || state.binding->draft == nullptr
        ? 0U
        : state.binding->draft->operations.size();
    const bool input = state.stage == 0U;
    const bool output = state.stage == operation_count + 1U;
    if (input) {
        place(state.primary, margin, y, width, row);
        y += row + gap;
        place(state.path_label, margin, y, width, row);
        y += row;
        const int browse_width = std::min(Scale(window, 92), width / 3);
        place(state.path, margin, y, std::max(0, width - browse_width - gap), row);
        place(
            state.browse,
            margin + std::max(0, width - browse_width),
            y,
            browse_width,
            row);
        y += row + gap;
        place(state.first_label, margin, y, half, row);
        place(state.last_label, margin + half + gap, y, half, row);
        y += row;
        place(state.first, margin, y, half, row);
        place(state.last, margin + half + gap, y, half, row);
        y += row + gap;
        const int list_height = Scale(window, 118);
        place(state.rows, margin, y, width, list_height);
        y += list_height + gap;
        const int button_width = std::max(0, (width - gap) / 2);
        place(state.add, margin, y, button_width, row);
        place(state.remove, margin + button_width + gap, y, button_width, row);
        y += row;
    } else if (output) {
        place(state.primary, margin, y, half, row);
        place(state.secondary, margin + half + gap, y, half, row);
        y += row + gap;
        if (IsWindowVisible(state.path_label) != FALSE) {
            place(state.path_label, margin, y, width, row);
            y += row;
            const int browse_width = std::min(Scale(window, 92), width / 3);
            place(state.path, margin, y, std::max(0, width - browse_width - gap), row);
            place(
                state.browse,
                margin + std::max(0, width - browse_width),
                y,
                browse_width,
                row);
            y += row + gap;
        }
        place(state.template_label, margin, y, width, row);
        y += row;
        place(state.naming_template, margin, y, width, row);
        y += row;
    } else {
        if (IsWindowVisible(state.target) != FALSE) {
            place(state.target_label, margin, y, width, row);
            y += row;
            place(state.target, margin, y, width, row);
            y += row + gap;
            place(state.target_color, margin, y, width, row);
            y += row + gap;
            if (IsWindowVisible(state.target_fixed) != FALSE) {
                place(state.target_fixed, margin, y, width, row);
                y += row + gap;
            }
        }
        if (IsWindowVisible(state.first_label) != FALSE) {
            place(state.first_label, margin, y, half, row);
            place(state.last_label, margin + half + gap, y, half, row);
            y += row;
            place(state.first, margin, y, half, row);
            place(state.last, margin + half + gap, y, half, row);
            y += row + gap;
        }
        const int list_height = Scale(window, 150);
        place(state.rows, margin, y, width, list_height);
        y += list_height + gap;
        const int button_width = std::max(0, (width - gap * 2) / 3);
        place(state.add, margin, y, button_width, row);
        place(state.remove, margin + button_width + gap, y, button_width, row);
        place(state.swap, margin + (button_width + gap) * 2, y, button_width, row);
        y += row + gap;
        place(state.current, margin, y, button_width, row);
        place(state.old_swatch, margin + button_width + gap, y, button_width, row);
        place(state.new_swatch, margin + (button_width + gap) * 2, y, button_width, row);
        y += row + gap;
        const bool two_alpha_fields =
            (GetWindowLongPtrW(state.secondary_alpha_label, GWL_STYLE)
             & WS_VISIBLE)
            != 0;
        const int alpha_width = two_alpha_fields ? half : width;
        place(state.primary_alpha_label, margin, y, alpha_width, row);
        if (two_alpha_fields) {
            place(
                state.secondary_alpha_label,
                margin + alpha_width + gap,
                y,
                alpha_width,
                row);
        }
        y += row;
        place(state.primary_alpha, margin, y, alpha_width, row);
        if (two_alpha_fields) {
            place(
                state.secondary_alpha,
                margin + alpha_width + gap,
                y,
                alpha_width,
                row);
        }
        y += row;
    }
    ResizeRowColumns(state);

    const int viewport = client.bottom - client.top;
    const int content = std::max(0, y + state.scroll_y + margin);
    SCROLLINFO scroll{};
    scroll.cbSize = sizeof(scroll);
    scroll.fMask = SIF_PAGE | SIF_RANGE | SIF_POS;
    scroll.nMin = 0;
    scroll.nMax = std::max(0, content - 1);
    scroll.nPage = static_cast<UINT>(std::max(0, viewport));
    scroll.nPos = state.scroll_y;
    SetScrollInfo(window, SB_VERT, &scroll, TRUE);
}

app::BatchOperationUi* SelectedOperation(EditorState& state) noexcept {
    if (state.binding == nullptr || state.binding->draft == nullptr
        || state.stage == 0U) {
        return nullptr;
    }
    auto& operations = state.binding->draft->operations;
    const std::size_t index = static_cast<std::size_t>(state.stage - 1U);
    return index < operations.size() ? &operations[index] : nullptr;
}

bool MatchesTarget(
    std::uint64_t layer_id,
    std::uint64_t plane_id,
    InkpodTypedPlaneKind plane_kind,
    const ColorReplaceTargetOption& option) noexcept {
    return layer_id == 0U && plane_id == 0U
        && plane_kind == option.plane_kind;
}

bool HasColorReplaceTarget(
    const app::BatchOperationUi& operation,
    const ColorReplaceTargetOption& option) noexcept {
    if (MatchesTarget(
            operation.layer_id,
            operation.plane_id,
            operation.plane_kind,
            option)) {
        return true;
    }
    return std::any_of(
        operation.additional_targets.begin(),
        operation.additional_targets.end(),
        [&](const InkpodBatchTargetInput& target) {
            return MatchesTarget(
                target.layer_id,
                target.plane_id,
                target.plane_kind,
                option);
        });
}

bool HasFixedColorReplaceTarget(const app::BatchOperationUi& operation) noexcept {
    const auto known = [](
                           std::uint64_t layer_id,
                           std::uint64_t plane_id,
                           InkpodTypedPlaneKind plane_kind) {
        return std::any_of(
            kColorReplaceTargets.begin(),
            kColorReplaceTargets.end(),
            [&](const ColorReplaceTargetOption& option) {
                return MatchesTarget(
                    layer_id, plane_id, plane_kind, option);
            });
    };
    if (!known(
            operation.layer_id,
            operation.plane_id,
            operation.plane_kind)) {
        return true;
    }
    return std::any_of(
        operation.additional_targets.begin(),
        operation.additional_targets.end(),
        [&](const InkpodBatchTargetInput& target) {
            return !known(
                target.layer_id,
                target.plane_id,
                target.plane_kind);
        });
}

InkpodBatchTargetInput TargetRecord(
    const ColorReplaceTargetOption& option,
    InkpodBatchMissingPolicy missing_policy) noexcept {
    InkpodBatchTargetInput target{};
    target.struct_size = sizeof(target);
    target.plane_kind = option.plane_kind;
    target.missing_policy = missing_policy;
    return target;
}

bool SetColorReplaceTargetChecked(
    app::BatchOperationUi& operation,
    std::size_t changed_index,
    bool checked) noexcept {
    if (changed_index >= kColorReplaceTargets.size()) {
        return false;
    }
    std::array<bool, kColorReplaceTargets.size()> selected{};
    if (!HasFixedColorReplaceTarget(operation)) {
        for (std::size_t index = 0U; index < selected.size(); ++index) {
            selected[index] = HasColorReplaceTarget(
                operation, kColorReplaceTargets[index]);
        }
    }
    selected[changed_index] = checked;
    if (std::none_of(selected.begin(), selected.end(), [](bool value) { return value; })) {
        return false;
    }
    const auto primary = std::find(selected.begin(), selected.end(), true);
    const std::size_t primary_index = static_cast<std::size_t>(primary - selected.begin());
    const auto primary_target = TargetRecord(
        kColorReplaceTargets[primary_index], operation.missing_policy);
    std::vector<InkpodBatchTargetInput> additional_targets;
    try {
        additional_targets.reserve(selected.size() - primary_index - 1U);
        for (std::size_t index = primary_index + 1U; index < selected.size(); ++index) {
            if (selected[index]) {
                additional_targets.push_back(TargetRecord(
                    kColorReplaceTargets[index], operation.missing_policy));
            }
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    operation.layer_id = primary_target.layer_id;
    operation.plane_id = primary_target.plane_id;
    operation.plane_kind = primary_target.plane_kind;
    operation.additional_targets = std::move(additional_targets);
    return true;
}

std::wstring ColorAlphaLabel(
    bool named_slot,
    bool secondary,
    const InkpodColorValue& color) {
    std::wstring label;
    if (named_slot) {
        label = UiText(
            secondary ? UiStringId::Text0705 : UiStringId::Text0723);
        label.push_back(L' ');
    }
    label += UiText(UiStringId::Opacity);
    label += L" (0–";
    label += color.depth == INKPOD_COLOR_DEPTH_16 ? L"65535" : L"255";
    label.push_back(L')');
    return label;
}

void RefreshSelectedColorControls(EditorState& state) noexcept {
    auto* operation = SelectedOperation(state);
    const std::size_t row = SelectedRow(state);
    const bool color_replace = operation != nullptr
        && operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE;
    const InkpodColorValue* primary = operation == nullptr
        ? nullptr
        : BatchOperationColor(*operation, row, BatchColorSlot::Primary);
    const InkpodColorValue* secondary = !color_replace
        ? nullptr
        : BatchOperationColor(*operation, row, BatchColorSlot::Secondary);
    if (primary != nullptr) {
        SetWindowTextW(
            state.primary_alpha_label,
            ColorAlphaLabel(color_replace, false, *primary).c_str());
        SetText(state.primary_alpha, Number(primary->alpha));
    } else {
        SetWindowTextW(state.primary_alpha_label, UiText(UiStringId::Opacity));
        SetWindowTextW(state.primary_alpha, L"");
    }
    if (secondary != nullptr) {
        SetWindowTextW(
            state.secondary_alpha_label,
            ColorAlphaLabel(true, true, *secondary).c_str());
        SetText(state.secondary_alpha, Number(secondary->alpha));
    } else {
        SetWindowTextW(state.secondary_alpha_label, L"");
        SetWindowTextW(state.secondary_alpha, L"");
    }
    EnableWindow(
        state.primary_alpha,
        state.enabled && primary != nullptr ? TRUE : FALSE);
    EnableWindow(
        state.secondary_alpha,
        state.enabled && secondary != nullptr ? TRUE : FALSE);
    InvalidateRect(state.old_swatch, nullptr, TRUE);
    InvalidateRect(state.new_swatch, nullptr, TRUE);
}

void CommitColorAlpha(
    EditorState& state, bool secondary_slot) noexcept {
    auto* operation = SelectedOperation(state);
    if (operation == nullptr) {
        return;
    }
    const std::size_t row = SelectedRow(state);
    std::uint64_t alpha{};
    HWND edit = secondary_slot ? state.secondary_alpha : state.primary_alpha;
    if (ReadNumber(edit, alpha) && alpha <= UINT32_MAX
        && SetBatchOperationColorAlpha(
            *operation,
            row,
            secondary_slot ? BatchColorSlot::Secondary
                           : BatchColorSlot::Primary,
            static_cast<std::uint32_t>(alpha))) {
        state.selected_color_row = row;
        Changed(state);
    } else {
        RefreshSelectedColorControls(state);
    }
}

void Refresh(EditorState& state, HWND window) noexcept {
    if (state.binding == nullptr || state.binding->draft == nullptr) {
        return;
    }
    auto& draft = *state.binding->draft;
    state.updating = true;
    const bool input = state.stage == 0U;
    const bool output = state.stage == draft.operations.size() + 1U;
    const bool operation = !input && !output;
    const bool folder_output = output
        && draft.output_destination == INKPOD_BATCH_OUTPUT_FOLDER;
    app::BatchOperationUi* const selected_operation =
        operation ? SelectedOperation(state) : nullptr;
    const bool color_replace = selected_operation != nullptr
        && selected_operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE;
    Show(state.primary, input || output);
    Show(state.secondary, output);
    Show(state.path_label, input || folder_output);
    Show(state.path, input || folder_output);
    Show(state.browse, input || folder_output);
    Show(state.template_label, output);
    Show(state.naming_template, output);
    Show(state.first_label, input || (operation && !color_replace));
    Show(state.first, input || (operation && !color_replace));
    Show(state.last_label, input || (operation && !color_replace));
    Show(state.last, input || (operation && !color_replace));
    Show(state.rows, input || operation);
    Show(state.add, input || operation);
    Show(state.remove, input || operation);
    Show(state.swap, operation);
    Show(state.current, operation);
    Show(state.old_swatch, operation);
    Show(state.new_swatch, color_replace);
    Show(state.primary_alpha_label, operation);
    Show(state.primary_alpha, operation);
    Show(state.secondary_alpha_label, color_replace);
    Show(state.secondary_alpha, color_replace);
    Show(state.target_label, color_replace);
    Show(state.target, color_replace);
    Show(state.target_color, color_replace);
    const bool fixed_color_target = color_replace
        && HasFixedColorReplaceTarget(*selected_operation);
    Show(state.target_fixed, fixed_color_target);

    SendMessageW(state.primary, CB_RESETCONTENT, 0, 0);
    SendMessageW(state.secondary, CB_RESETCONTENT, 0, 0);
    if (input) {
        SetWindowTextW(state.title, UiText(UiStringId::BatchInput));
        for (const auto kind : {INKPOD_BATCH_INPUT_FILE, INKPOD_BATCH_INPUT_FOLDER,
                                INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT}) {
            SendMessageW(
                state.primary,
                CB_ADDSTRING,
                0,
                reinterpret_cast<LPARAM>(InputKindLabel(kind)));
        }
        state.selected_input = std::min(
            state.selected_input, draft.inputs.size() - 1U);
        const auto& selected_input = draft.inputs[state.selected_input];
        const WPARAM kind_index = selected_input.kind == INKPOD_BATCH_INPUT_FOLDER
            ? 1U
            : (selected_input.kind == INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT ? 2U
                                                                          : 0U);
        SendMessageW(state.primary, CB_SETCURSEL, kind_index, 0);
        SetWindowTextW(state.path_label, UiText(UiStringId::BatchPath));
        SetWindowTextW(state.template_label, L"");
        SetWindowTextW(state.first_label, UiText(UiStringId::Text1020));
        SetWindowTextW(state.last_label, UiText(UiStringId::Text0842));
        SetText(state.path, selected_input.path);
        SetText(state.first, Number(selected_input.first_cell));
        SetText(state.last, Number(selected_input.last_cell));
        SetWindowTextW(state.add, UiText(UiStringId::Text0951));
        PopulateInputRows(state, draft);
    } else if (output) {
        SetWindowTextW(state.title, UiText(UiStringId::BatchOutput));
        for (const wchar_t* label : {UiText(UiStringId::BatchFolder),
                                     UiText(UiStringId::BatchActiveDocument),
                                     UiText(UiStringId::BatchNewTabs)}) {
            SendMessageW(
                state.primary,
                CB_ADDSTRING,
                0,
                reinterpret_cast<LPARAM>(label));
        }
        SendMessageW(
            state.primary,
            CB_SETCURSEL,
            static_cast<WPARAM>(draft.output_destination - 1U),
            0);
        for (const wchar_t* format : {L".inkpod", L"PNG", L"TIFF", L"TGA", L"BMP"}) {
            SendMessageW(
                state.secondary,
                CB_ADDSTRING,
                0,
                reinterpret_cast<LPARAM>(format));
        }
        SendMessageW(
            state.secondary,
            CB_SETCURSEL,
            static_cast<WPARAM>(draft.output_format - 1U),
            0);
        SetWindowTextW(state.path_label, UiText(UiStringId::BatchFolder));
        SetText(state.path, draft.output_folder);
        SetWindowTextW(
            state.template_label, UiText(UiStringId::BatchNamingTemplate));
        SetText(state.naming_template, draft.naming_template);
    } else if (auto* selected = selected_operation; selected != nullptr) {
        SetText(state.title, selected->label);
        SetWindowTextW(
            state.target_label, UiText(UiStringId::OperationTargetLayer));
        if (color_replace) {
            const std::array<HWND, kColorReplaceTargets.size()> controls{
                state.target, state.target_color};
            for (std::size_t index = 0U; index < controls.size(); ++index) {
                SetWindowTextW(
                    controls[index], UiText(kColorReplaceTargets[index].label));
                SendMessageW(
                    controls[index],
                    BM_SETCHECK,
                    HasColorReplaceTarget(*selected, kColorReplaceTargets[index])
                        ? BST_CHECKED
                        : BST_UNCHECKED,
                    0);
            }
            SetWindowTextW(
                state.target_fixed, UiText(UiStringId::BatchFixedTarget));
            SendMessageW(state.target_fixed, BM_SETCHECK, BST_CHECKED, 0);
        }
        SetWindowTextW(state.first_label, UiText(UiStringId::BatchLayerId));
        SetWindowTextW(state.last_label, UiText(UiStringId::BatchPlaneId));
        SetText(state.first, Number(selected->layer_id));
        SetText(state.last, Number(selected->plane_id));
        SetWindowTextW(state.add, UiText(UiStringId::Text0951));
        SetWindowTextW(state.remove, UiText(UiStringId::Delete));
        SetWindowTextW(state.swap, UiText(UiStringId::BatchInvertAll));
        SetWindowTextW(state.current, UiText(UiStringId::BatchCurrentColor));
        SetWindowTextW(
            state.old_swatch,
            color_replace ? UiText(UiStringId::Text0723)
                          : UiText(UiStringId::BatchColors));
        SetWindowTextW(state.new_swatch, UiText(UiStringId::Text0705));
        PopulateOperationRows(state, *selected);
    }
    SetWindowTextW(state.browse, UiText(UiStringId::BatchBrowse));
    for (HWND control : {state.primary, state.secondary, state.path, state.browse,
                         state.naming_template, state.first,
                         state.last, state.rows, state.add, state.remove, state.swap,
                         state.current, state.old_swatch, state.new_swatch,
                         state.primary_alpha, state.secondary_alpha,
                         state.target, state.target_color}) {
        EnableWindow(control, state.enabled ? TRUE : FALSE);
    }
    EnableWindow(state.target_fixed, FALSE);
    if (input && !draft.inputs.empty()
        && draft.inputs[state.selected_input].kind
            == INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT) {
        EnableWindow(state.path, FALSE);
        EnableWindow(state.browse, FALSE);
    }
    RefreshSelectedColorControls(state);
    state.updating = false;
    Layout(state, window);
}

InkpodColorValue TransparentColor() noexcept {
    return InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 0U};
}

InkpodColorValue DrawingColor(const EditorState& state) noexcept {
    return state.binding != nullptr && state.binding->drawing_color != nullptr
        ? *state.binding->drawing_color
        : TransparentColor();
}

const InkpodColorValue* SelectedSwatchColor(
    EditorState& state, bool old_color) noexcept {
    auto* operation = SelectedOperation(state);
    if (operation == nullptr) {
        return nullptr;
    }
    return BatchOperationColor(
        *operation,
        SelectedRow(state),
        old_color ? BatchColorSlot::Primary : BatchColorSlot::Secondary);
}

void DrawSwatch(EditorState& state, const DRAWITEMSTRUCT& item) noexcept {
    const bool old_color = item.CtlID == kOldSwatch;
    const InkpodColorValue* color = SelectedSwatchColor(state, old_color);
    const COLORREF background = GetSysColor(
        (item.itemState & ODS_DISABLED) != 0U ? COLOR_BTNFACE : COLOR_WINDOW);
    HBRUSH background_brush = CreateSolidBrush(background);
    if (background_brush != nullptr) {
        FillRect(item.hDC, &item.rcItem, background_brush);
        DeleteObject(background_brush);
    }
    RECT sample = item.rcItem;
    sample.left += Scale(item.hwndItem, 5);
    sample.top += Scale(item.hwndItem, 5);
    sample.right = std::min(
        sample.right - Scale(item.hwndItem, 5),
        sample.left + Scale(item.hwndItem, 28));
    sample.bottom -= Scale(item.hwndItem, 5);
    const unsigned divisor = color != nullptr
            && color->depth == INKPOD_COLOR_DEPTH_16
        ? 257U
        : 1U;
    const COLORREF sample_color = color == nullptr
        ? GetSysColor(COLOR_BTNFACE)
        : RGB(
              color->red / divisor,
              color->green / divisor,
              color->blue / divisor);
    HBRUSH sample_brush = CreateSolidBrush(sample_color);
    if (sample_brush != nullptr) {
        FillRect(item.hDC, &sample, sample_brush);
        DeleteObject(sample_brush);
    }
    FrameRect(item.hDC, &sample, GetSysColorBrush(COLOR_WINDOWTEXT));
    std::array<wchar_t, 64U> label{};
    GetWindowTextW(
        item.hwndItem, label.data(), static_cast<int>(label.size()));
    RECT text = item.rcItem;
    text.left = sample.right + Scale(item.hwndItem, 6);
    SetBkMode(item.hDC, TRANSPARENT);
    SetTextColor(
        item.hDC,
        GetSysColor(
            (item.itemState & ODS_DISABLED) != 0U
                ? COLOR_GRAYTEXT
                : COLOR_BTNTEXT));
    DrawTextW(
        item.hDC,
        label.data(),
        -1,
        &text,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
    if ((item.itemState & ODS_FOCUS) != 0U) {
        RECT focus = item.rcItem;
        InflateRect(&focus, -2, -2);
        DrawFocusRect(item.hDC, &focus);
    }
}

const InkpodColorValue* ListCellColor(
    EditorState& state, std::size_t row, int column) noexcept {
    auto* operation = SelectedOperation(state);
    if (operation == nullptr) {
        return nullptr;
    }
    if (operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        return column == 1
            ? BatchOperationColor(*operation, row, BatchColorSlot::Primary)
            : (column == 2
                   ? BatchOperationColor(
                         *operation, row, BatchColorSlot::Secondary)
                   : nullptr);
    }
    return column == 0
        ? BatchOperationColor(*operation, row, BatchColorSlot::Primary)
        : nullptr;
}

COLORREF CompositeSwatchColor(
    const InkpodColorValue& color, COLORREF background) noexcept {
    const std::uint32_t maximum = color.depth == INKPOD_COLOR_DEPTH_16
        ? 65'535U
        : 255U;
    const std::uint32_t alpha = std::min<std::uint32_t>(color.alpha, maximum);
    const auto channel = [maximum, alpha](
                             std::uint32_t foreground,
                             std::uint32_t background_channel) noexcept {
        foreground = std::min(foreground, maximum);
        return static_cast<std::uint8_t>(
            (static_cast<std::uint64_t>(foreground) * alpha * 255U
             + static_cast<std::uint64_t>(background_channel)
                 * (maximum - alpha) * maximum
             + static_cast<std::uint64_t>(maximum) * maximum / 2U)
            / (static_cast<std::uint64_t>(maximum) * maximum));
    };
    return RGB(
        channel(color.red, GetRValue(background)),
        channel(color.green, GetGValue(background)),
        channel(color.blue, GetBValue(background)));
}

void DrawListColorCell(
    EditorState& state, const NMLVCUSTOMDRAW& draw) noexcept {
    const std::size_t row = static_cast<std::size_t>(draw.nmcd.dwItemSpec);
    const int column = draw.iSubItem;
    const InkpodColorValue* color = ListCellColor(state, row, column);
    if (color == nullptr) {
        return;
    }
    RECT cell{};
    if (column == 0) {
        if (ListView_GetItemRect(
                state.rows, static_cast<int>(row), &cell, LVIR_BOUNDS)
            == FALSE) {
            return;
        }
        cell.right = cell.left + ListView_GetColumnWidth(state.rows, 0);
    } else if (ListView_GetSubItemRect(
                   state.rows,
                   static_cast<int>(row),
                   column,
                   LVIR_BOUNDS,
                   &cell)
               == FALSE) {
        return;
    }
    const bool selected =
        (ListView_GetItemState(
             state.rows, static_cast<int>(row), LVIS_SELECTED)
         & LVIS_SELECTED)
        != 0U;
    const COLORREF background = GetSysColor(
        selected ? COLOR_HIGHLIGHT : COLOR_WINDOW);
    FillRect(
        draw.nmcd.hdc,
        &cell,
        GetSysColorBrush(selected ? COLOR_HIGHLIGHT : COLOR_WINDOW));

    RECT sample = cell;
    sample.left += Scale(state.rows, 5);
    sample.right = std::min(
        sample.right - Scale(state.rows, 3),
        sample.left + Scale(state.rows, 26));
    sample.top += Scale(state.rows, 3);
    sample.bottom -= Scale(state.rows, 3);
    if (sample.right <= sample.left || sample.bottom <= sample.top) {
        return;
    }

    const COLORREF checker[2]{
        background, GetSysColor(selected ? COLOR_HOTLIGHT : COLOR_BTNFACE)};
    const int square = std::max(1, Scale(state.rows, 4));
    for (int top = sample.top, y = 0; top < sample.bottom; top += square, ++y) {
        for (int left = sample.left, x = 0;
             left < sample.right;
             left += square, ++x) {
            RECT tile{
                left,
                top,
                std::min<LONG>(
                    sample.right, left + static_cast<LONG>(square)),
                std::min<LONG>(
                    sample.bottom, top + static_cast<LONG>(square))};
            const COLORREF rendered = CompositeSwatchColor(
                *color, checker[(x + y) & 1]);
            const HBRUSH brush = CreateSolidBrush(rendered);
            if (brush != nullptr) {
                FillRect(draw.nmcd.hdc, &tile, brush);
                DeleteObject(brush);
            }
        }
    }
    FrameRect(draw.nmcd.hdc, &sample, GetSysColorBrush(COLOR_WINDOWTEXT));

    RECT text = cell;
    text.left = sample.right + Scale(state.rows, 6);
    text.right -= Scale(state.rows, 4);
    const std::wstring label = ColorText(*color);
    const HFONT font = reinterpret_cast<HFONT>(
        SendMessageW(state.rows, WM_GETFONT, 0, 0));
    const HGDIOBJ previous = font == nullptr
        ? nullptr
        : SelectObject(draw.nmcd.hdc, font);
    SetBkMode(draw.nmcd.hdc, TRANSPARENT);
    SetTextColor(
        draw.nmcd.hdc,
        GetSysColor(selected ? COLOR_HIGHLIGHTTEXT : COLOR_WINDOWTEXT));
    DrawTextW(
        draw.nmcd.hdc,
        label.c_str(),
        static_cast<int>(label.size()),
        &text,
        DT_END_ELLIPSIS | DT_NOPREFIX | DT_SINGLELINE | DT_VCENTER);
    if (previous != nullptr) {
        SelectObject(draw.nmcd.hdc, previous);
    }
}

LRESULT DrawListRows(EditorState& state, const NMLVCUSTOMDRAW& draw) noexcept {
    switch (draw.nmcd.dwDrawStage) {
        case CDDS_PREPAINT:
            return CDRF_NOTIFYITEMDRAW;
        case CDDS_ITEMPREPAINT:
            return CDRF_NOTIFYSUBITEMDRAW;
        case CDDS_ITEMPREPAINT | CDDS_SUBITEM:
            if (ListCellColor(
                       state,
                       static_cast<std::size_t>(draw.nmcd.dwItemSpec),
                       draw.iSubItem)
                != nullptr) {
                DrawListColorCell(state, draw);
                return CDRF_SKIPDEFAULT;
            }
            return CDRF_DODEFAULT;
        default:
            return CDRF_DODEFAULT;
    }
}

void AddRow(EditorState& state, HWND window) noexcept {
    auto& draft = *state.binding->draft;
    try {
        if (state.stage == 0U) {
            const LRESULT selected = SendMessageW(state.primary, CB_GETCURSEL, 0, 0);
            app::BatchInputUi input{};
            input.kind = selected == 1 ? INKPOD_BATCH_INPUT_FOLDER
                : (selected == 2 ? INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT
                                 : INKPOD_BATCH_INPUT_FILE);
            input.path = GetText(state.path);
            draft.inputs.push_back(std::move(input));
            state.selected_input = draft.inputs.size() - 1U;
        } else if (auto* operation = SelectedOperation(state); operation != nullptr) {
            if (operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
                InkpodBatchColorPairInput pair{};
                pair.struct_size = sizeof(pair);
                pair.enabled = 1U;
                pair.old_color = TransparentColor();
                pair.new_color = DrawingColor(state);
                operation->color_pairs.push_back(pair);
            } else {
                operation->colors.push_back(DrawingColor(state));
            }
        }
    } catch (const std::bad_alloc&) {
        return;
    }
    Changed(state);
    Refresh(state, window);
}

void RemoveRow(EditorState& state, HWND window) noexcept {
    const std::size_t row = SelectedRow(state);
    auto& draft = *state.binding->draft;
    if (state.stage == 0U) {
        if (row < draft.inputs.size() && draft.inputs.size() > 1U) {
            draft.inputs.erase(draft.inputs.begin() + row);
            state.selected_input = std::min(
                row, draft.inputs.size() - 1U);
        } else {
            return;
        }
    } else if (auto* operation = SelectedOperation(state); operation != nullptr) {
        if (operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
            if (row >= operation->color_pairs.size()) {
                return;
            }
            operation->color_pairs.erase(operation->color_pairs.begin() + row);
        } else {
            if (row >= operation->colors.size()) {
                return;
            }
            operation->colors.erase(operation->colors.begin() + row);
        }
    }
    Changed(state);
    Refresh(state, window);
}

void CommitText(EditorState& state) noexcept {
    auto& draft = *state.binding->draft;
    if (state.stage == 0U) {
        const std::size_t row = SelectedRow(state);
        if (row < draft.inputs.size()) {
            auto& input = draft.inputs[row];
            input.path = GetText(state.path);
            std::uint64_t first{};
            std::uint64_t last{};
            if (ReadNumber(state.first, first) && first <= UINT32_MAX) {
                input.first_cell = static_cast<std::uint32_t>(first);
            }
            if (ReadNumber(state.last, last) && last <= UINT32_MAX) {
                input.last_cell = static_cast<std::uint32_t>(last);
            }
        }
    } else if (state.stage == draft.operations.size() + 1U) {
        draft.output_folder = GetText(state.path);
        draft.naming_template = GetText(state.naming_template);
    } else if (auto* operation = SelectedOperation(state); operation != nullptr) {
        std::uint64_t layer{};
        std::uint64_t plane{};
        if (ReadNumber(state.first, layer)) {
            operation->layer_id = layer;
        }
        if (ReadNumber(state.last, plane)) {
            operation->plane_id = plane;
        }
    }
    Changed(state);
}

void ChooseRowColor(EditorState& state, HWND window, bool old_color) noexcept {
    auto* operation = SelectedOperation(state);
    if (operation == nullptr) {
        return;
    }
    const std::size_t row = SelectedRow(state);
    const BatchColorSlot slot = old_color ? BatchColorSlot::Primary
                                          : BatchColorSlot::Secondary;
    const InkpodColorValue* selected = BatchOperationColor(*operation, row, slot);
    if (selected == nullptr) {
        return;
    }
    InkpodColorValue color = *selected;
    static std::array<COLORREF, 16U> custom{};
    const unsigned divisor = color.depth == INKPOD_COLOR_DEPTH_16 ? 257U : 1U;
    CHOOSECOLORW chooser{};
    chooser.lStructSize = sizeof(chooser);
    chooser.hwndOwner = window;
    chooser.rgbResult = RGB(
        color.red / divisor, color.green / divisor, color.blue / divisor);
    chooser.lpCustColors = custom.data();
    chooser.Flags = CC_FULLOPEN | CC_RGBINIT;
    if (ChooseColorW(&chooser) == FALSE) {
        return;
    }
    color.red = static_cast<std::uint16_t>(GetRValue(chooser.rgbResult) * divisor);
    color.green = static_cast<std::uint16_t>(GetGValue(chooser.rgbResult) * divisor);
    color.blue = static_cast<std::uint16_t>(GetBValue(chooser.rgbResult) * divisor);
    if (SetBatchOperationColor(*operation, row, slot, color)) {
        state.selected_color_row = row;
        Changed(state);
    }
}

void ApplyDrawingColor(EditorState& state, BatchColorSlot slot) noexcept {
    auto* operation = SelectedOperation(state);
    if (operation == nullptr) {
        return;
    }
    const std::size_t row = SelectedRow(state);
    if (SetBatchOperationColor(*operation, row, slot, DrawingColor(state))) {
        state.selected_color_row = row;
        Changed(state);
    }
}

void ShowDrawingColorMenu(EditorState& state, HWND window) noexcept {
    auto* operation = SelectedOperation(state);
    if (operation == nullptr) {
        return;
    }
    if (operation->kind != INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        ApplyDrawingColor(state, BatchColorSlot::Primary);
        return;
    }

    HMENU menu = CreatePopupMenu();
    if (menu == nullptr) {
        return;
    }
    constexpr UINT kUseForOldColor = 1U;
    constexpr UINT kUseForNewColor = 2U;
    AppendMenuW(
        menu, MF_STRING, kUseForOldColor, UiText(UiStringId::Text0723));
    AppendMenuW(
        menu, MF_STRING, kUseForNewColor, UiText(UiStringId::Text0705));
    RECT anchor{};
    GetWindowRect(state.current, &anchor);
    const UINT command = TrackPopupMenuEx(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN,
        anchor.left,
        anchor.bottom,
        window,
        nullptr);
    DestroyMenu(menu);
    if (command == kUseForOldColor) {
        ApplyDrawingColor(state, BatchColorSlot::Primary);
    } else if (command == kUseForNewColor) {
        ApplyDrawingColor(state, BatchColorSlot::Secondary);
    }
}

void BrowsePath(EditorState& state, HWND window) noexcept {
    if (state.binding == nullptr || state.binding->draft == nullptr) {
        return;
    }
    auto& draft = *state.binding->draft;
    if (state.stage == 0U) {
        if (state.selected_input >= draft.inputs.size()) {
            return;
        }
        const auto kind = draft.inputs[state.selected_input].kind;
        if (kind == INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT) {
            return;
        }
        if (kind == INKPOD_BATCH_INPUT_FOLDER) {
            std::wstring folder;
            if (!ChooseBatchFolder(
                    window, UiText(UiStringId::Text0261), folder)) {
                return;
            }
            try {
                draft.inputs[state.selected_input].path = std::move(folder);
            } catch (const std::bad_alloc&) {
                return;
            }
        } else {
            std::vector<std::wstring> paths;
            if (!ChooseBatchInputFiles(
                    window,
                    UiText(UiStringId::BatchInputFileFilter),
                    paths)
                || paths.empty()
                || draft.inputs.size() > 16'384U
                || paths.size() > 16'384U - draft.inputs.size() + 1U) {
                return;
            }
            try {
                auto candidate = draft.inputs;
                auto selected = candidate[state.selected_input];
                selected.kind = INKPOD_BATCH_INPUT_FILE;
                selected.path = paths[0];
                candidate[state.selected_input] = selected;
                for (std::size_t index = 1U; index < paths.size(); ++index) {
                    auto additional = selected;
                    additional.path = paths[index];
                    candidate.insert(
                        candidate.begin()
                            + static_cast<std::ptrdiff_t>(
                                state.selected_input + index),
                        std::move(additional));
                }
                draft.inputs.swap(candidate);
            } catch (const std::bad_alloc&) {
                return;
            }
        }
    } else if (state.stage == draft.operations.size() + 1U
               && draft.output_destination == INKPOD_BATCH_OUTPUT_FOLDER) {
        std::wstring folder;
        if (!ChooseBatchFolder(
                window, UiText(UiStringId::Text0261), folder)) {
            return;
        }
        try {
            draft.output_folder = std::move(folder);
        } catch (const std::bad_alloc&) {
            return;
        }
    } else {
        return;
    }
    Changed(state);
    Refresh(state, window);
}

void ApplyEditorFont(EditorState& state, HWND window) noexcept {
    HFONT font = reinterpret_cast<HFONT>(
        SendMessageW(GetParent(window), WM_GETFONT, 0, 0));
    if (font == nullptr) {
        font = static_cast<HFONT>(GetStockObject(DEFAULT_GUI_FONT));
    }
    SendMessageW(window, WM_SETFONT, reinterpret_cast<WPARAM>(font), FALSE);
    for (HWND control : {
             state.title,
             state.primary,
             state.secondary,
             state.path_label,
             state.path,
             state.browse,
             state.template_label,
             state.naming_template,
             state.first_label,
             state.first,
             state.last_label,
             state.last,
             state.rows,
             state.add,
             state.remove,
             state.swap,
             state.current,
             state.old_swatch,
             state.new_swatch,
             state.primary_alpha_label,
             state.primary_alpha,
             state.secondary_alpha_label,
             state.secondary_alpha,
             state.target_label,
             state.target,
             state.target_color,
             state.target_fixed}) {
        SendMessageW(control, WM_SETFONT, reinterpret_cast<WPARAM>(font), FALSE);
    }
}

LRESULT CALLBACK EditorProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<EditorState*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));
    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<CREATESTRUCTW*>(lparam);
        state = new (std::nothrow) EditorState{};
        if (state == nullptr) {
            return FALSE;
        }
        state->binding = static_cast<BatchParameterEditorBinding*>(
            create->lpCreateParams);
        SetWindowLongPtrW(window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
        return TRUE;
    }
    if (state == nullptr) {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    switch (message) {
        case WM_CREATE:
            state->title = Child(window, 0, WC_STATICW, L"", SS_LEFT, kTitle);
            state->primary = Child(
                window, 0, WC_COMBOBOXW, L"", CBS_DROPDOWNLIST | WS_TABSTOP,
                kPrimary);
            state->secondary = Child(
                window, 0, WC_COMBOBOXW, L"", CBS_DROPDOWNLIST | WS_TABSTOP,
                kSecondary);
            state->path_label = Child(window, 0, WC_STATICW, L"", SS_LEFT, kPathLabel);
            state->path = Child(
                window, WS_EX_CLIENTEDGE, WC_EDITW, L"",
                ES_AUTOHSCROLL | WS_TABSTOP, kPath);
            state->template_label = Child(
                window, 0, WC_STATICW, L"", SS_LEFT, kTemplateLabel);
            state->naming_template = Child(
                window, WS_EX_CLIENTEDGE, WC_EDITW, L"",
                ES_AUTOHSCROLL | WS_TABSTOP, kTemplate);
            state->browse = Child(
                window, 0, WC_BUTTONW, UiText(UiStringId::BatchBrowse),
                WS_TABSTOP, kBrowse);
            state->first_label = Child(window, 0, WC_STATICW, L"", SS_LEFT, kFirstLabel);
            state->first = Child(
                window, WS_EX_CLIENTEDGE, WC_EDITW, L"",
                ES_NUMBER | ES_AUTOHSCROLL | WS_TABSTOP, kFirst);
            state->last_label = Child(window, 0, WC_STATICW, L"", SS_LEFT, kLastLabel);
            state->last = Child(
                window, WS_EX_CLIENTEDGE, WC_EDITW, L"",
                ES_NUMBER | ES_AUTOHSCROLL | WS_TABSTOP, kLast);
            state->rows = Child(
                window, WS_EX_CLIENTEDGE, WC_LISTVIEWW, L"",
                LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | WS_TABSTOP,
                kRows);
            ListView_SetExtendedListViewStyle(
                state->rows,
                LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP);
            state->add = Child(window, 0, WC_BUTTONW, L"", WS_TABSTOP, kAdd);
            state->remove = Child(
                window, 0, WC_BUTTONW, UiText(UiStringId::Delete), WS_TABSTOP,
                kRemove);
            state->swap = Child(window, 0, WC_BUTTONW, L"", WS_TABSTOP, kSwap);
            state->current = Child(window, 0, WC_BUTTONW, L"", WS_TABSTOP, kCurrent);
            state->old_swatch = Child(
                window, 0, WC_BUTTONW, UiText(UiStringId::Text0723),
                BS_OWNERDRAW | WS_TABSTOP, kOldSwatch);
            state->new_swatch = Child(
                window, 0, WC_BUTTONW, UiText(UiStringId::Text0705),
                BS_OWNERDRAW | WS_TABSTOP, kNewSwatch);
            state->primary_alpha_label = Child(
                window, 0, WC_STATICW, L"", SS_LEFT, kPrimaryAlphaLabel);
            state->primary_alpha = Child(
                window, WS_EX_CLIENTEDGE, WC_EDITW, L"",
                ES_NUMBER | ES_AUTOHSCROLL | WS_TABSTOP, kPrimaryAlpha);
            state->secondary_alpha_label = Child(
                window, 0, WC_STATICW, L"", SS_LEFT, kSecondaryAlphaLabel);
            state->secondary_alpha = Child(
                window, WS_EX_CLIENTEDGE, WC_EDITW, L"",
                ES_NUMBER | ES_AUTOHSCROLL | WS_TABSTOP, kSecondaryAlpha);
            state->target_label = Child(
                window, 0, WC_STATICW, L"", SS_LEFT,
                IDC_BATCH_PARAMETER_TARGET_LABEL);
            state->target = Child(
                window, 0, WC_BUTTONW, L"",
                BS_AUTOCHECKBOX | WS_TABSTOP,
                IDC_BATCH_PARAMETER_TARGET);
            state->target_color = Child(
                window, 0, WC_BUTTONW, L"",
                BS_AUTOCHECKBOX | WS_TABSTOP,
                IDC_BATCH_PARAMETER_TARGET_COLOR);
            state->target_fixed = Child(
                window, 0, WC_BUTTONW, L"",
                BS_AUTOCHECKBOX,
                IDC_BATCH_PARAMETER_TARGET_FIXED);
            SendMessageW(state->primary_alpha, EM_SETLIMITTEXT, 5U, 0);
            SendMessageW(state->secondary_alpha, EM_SETLIMITTEXT, 5U, 0);
            ApplyEditorFont(*state, window);
            Refresh(*state, window);
            return 0;
        case WM_SIZE:
            Layout(*state, window);
            return 0;
        case WM_VSCROLL: {
            SCROLLINFO scroll{};
            scroll.cbSize = sizeof(scroll);
            scroll.fMask = SIF_ALL;
            GetScrollInfo(window, SB_VERT, &scroll);
            int next = state->scroll_y;
            switch (LOWORD(wparam)) {
                case SB_LINEUP: next -= Scale(window, 18); break;
                case SB_LINEDOWN: next += Scale(window, 18); break;
                case SB_PAGEUP: next -= static_cast<int>(scroll.nPage); break;
                case SB_PAGEDOWN: next += static_cast<int>(scroll.nPage); break;
                case SB_THUMBTRACK: next = scroll.nTrackPos; break;
                default: break;
            }
            state->scroll_y = std::clamp(
                next, scroll.nMin,
                std::max(scroll.nMin, scroll.nMax - static_cast<int>(scroll.nPage) + 1));
            Layout(*state, window);
            return 0;
        }
        case WM_COMMAND:
            if (state->updating) {
                return 0;
            }
            switch (LOWORD(wparam)) {
                case kBrowse: BrowsePath(*state, window); return 0;
                case kPrimary:
                    if (HIWORD(wparam) == CBN_SELCHANGE
                        && state->stage == 0U) {
                        auto& draft = *state->binding->draft;
                        if (state->selected_input < draft.inputs.size()) {
                            const LRESULT index = SendMessageW(
                                state->primary, CB_GETCURSEL, 0, 0);
                            auto& input = draft.inputs[state->selected_input];
                            input.kind = index == 1 ? INKPOD_BATCH_INPUT_FOLDER
                                : (index == 2
                                       ? INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT
                                       : INKPOD_BATCH_INPUT_FILE);
                            if (input.kind == INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT) {
                                input.path.clear();
                            }
                            Changed(*state);
                            Refresh(*state, window);
                        }
                    } else if (HIWORD(wparam) == CBN_SELCHANGE
                               && state->stage
                                   == state->binding->draft->operations.size()
                                       + 1U) {
                        const LRESULT index = SendMessageW(state->primary, CB_GETCURSEL, 0, 0);
                        state->binding->draft->output_destination =
                            static_cast<std::uint32_t>(index + 1);
                        Changed(*state);
                        Refresh(*state, window);
                    }
                    return 0;
                case kSecondary:
                    if (HIWORD(wparam) == CBN_SELCHANGE) {
                        const LRESULT index = SendMessageW(state->secondary, CB_GETCURSEL, 0, 0);
                        state->binding->draft->output_format =
                            static_cast<std::uint32_t>(index + 1);
                        Changed(*state);
                    }
                    return 0;
                case IDC_BATCH_PARAMETER_TARGET:
                case IDC_BATCH_PARAMETER_TARGET_COLOR:
                    if (HIWORD(wparam) == BN_CLICKED) {
                        auto* operation = SelectedOperation(*state);
                        const std::size_t selected = LOWORD(wparam)
                                == IDC_BATCH_PARAMETER_TARGET
                            ? 0U
                            : 1U;
                        const std::array<HWND, kColorReplaceTargets.size()> controls{
                            state->target,
                            state->target_color};
                        const bool checked = SendMessageW(
                                                 controls[selected],
                                                 BM_GETCHECK,
                                                 0,
                                                 0)
                            == BST_CHECKED;
                        if (operation != nullptr
                            && operation->kind
                                == INKPOD_BATCH_OPERATION_COLOR_REPLACE
                            && SetColorReplaceTargetChecked(
                                *operation, selected, checked)) {
                            Changed(*state);
                        }
                        Refresh(*state, window);
                    }
                    return 0;
                case kPath:
                case kTemplate:
                case kFirst:
                case kLast:
                    if (HIWORD(wparam) == EN_KILLFOCUS) {
                        CommitText(*state);
                    }
                    return 0;
                case kPrimaryAlpha:
                    if (HIWORD(wparam) == EN_KILLFOCUS) {
                        CommitColorAlpha(*state, false);
                    }
                    return 0;
                case kSecondaryAlpha:
                    if (HIWORD(wparam) == EN_KILLFOCUS) {
                        CommitColorAlpha(*state, true);
                    }
                    return 0;
                case kAdd: AddRow(*state, window); return 0;
                case kRemove: RemoveRow(*state, window); return 0;
                case kSwap:
                    if (auto* operation = SelectedOperation(*state);
                        operation != nullptr
                        && operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
                        for (auto& pair : operation->color_pairs) {
                            std::swap(pair.old_color, pair.new_color);
                        }
                        Changed(*state);
                        Refresh(*state, window);
                    }
                    return 0;
                case kCurrent:
                    ShowDrawingColorMenu(*state, window);
                    return 0;
                case kOldSwatch: ChooseRowColor(*state, window, true); return 0;
                case kNewSwatch: ChooseRowColor(*state, window, false); return 0;
                default: break;
            }
            break;
        case WM_DRAWITEM:
            if (LOWORD(wparam) == kOldSwatch
                || LOWORD(wparam) == kNewSwatch) {
                DrawSwatch(
                    *state, *reinterpret_cast<const DRAWITEMSTRUCT*>(lparam));
                return TRUE;
            }
            break;
        case WM_NOTIFY:
            if (reinterpret_cast<NMHDR*>(lparam)->idFrom == kRows
                && reinterpret_cast<NMHDR*>(lparam)->code == NM_CUSTOMDRAW) {
                return DrawListRows(
                    *state, *reinterpret_cast<const NMLVCUSTOMDRAW*>(lparam));
            }
            if (!state->updating
                && reinterpret_cast<NMHDR*>(lparam)->idFrom == kRows
                && reinterpret_cast<NMHDR*>(lparam)->code == LVN_ITEMCHANGED
                && state->stage == 0U) {
                const auto* changed = reinterpret_cast<NMLISTVIEW*>(lparam);
                if ((changed->uNewState & LVIS_SELECTED) != 0U
                    && changed->iItem >= 0) {
                    state->selected_input = static_cast<std::size_t>(
                        changed->iItem);
                    Refresh(*state, window);
                }
                return 0;
            }
            if (!state->updating
                && reinterpret_cast<NMHDR*>(lparam)->idFrom == kRows
                && reinterpret_cast<NMHDR*>(lparam)->code == LVN_ITEMCHANGED
                && state->stage > 0U) {
                const auto* changed = reinterpret_cast<NMLISTVIEW*>(lparam);
                if ((changed->uNewState & LVIS_SELECTED) != 0U
                    && changed->iItem >= 0) {
                    state->selected_color_row = static_cast<std::size_t>(
                        changed->iItem);
                    RefreshSelectedColorControls(*state);
                    InvalidateRect(state->rows, nullptr, FALSE);
                }
                return 0;
            }
            if (!state->updating
                && reinterpret_cast<NMHDR*>(lparam)->idFrom == kRows
                && reinterpret_cast<NMHDR*>(lparam)->code == NM_DBLCLK) {
                if (auto* operation = SelectedOperation(*state);
                    operation != nullptr
                    && operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
                    const std::size_t row = SelectedRow(*state);
                    if (row < operation->color_pairs.size()) {
                        operation->color_pairs[row].enabled =
                            operation->color_pairs[row].enabled == 0U ? 1U : 0U;
                        Changed(*state);
                        Refresh(*state, window);
                    }
                }
                return 0;
            }
            break;
        case WM_NCDESTROY:
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            delete state;
            return 0;
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

bool RegisterEditorClass(HINSTANCE instance) noexcept {
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.lpfnWndProc = EditorProcedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.hbrBackground = reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1);
    window_class.lpszClassName = kEditorClassName;
    return RegisterClassExW(&window_class) != 0U
        || GetLastError() == ERROR_CLASS_ALREADY_EXISTS;
}

}  // namespace

HWND CreateBatchParameterEditor(
    HINSTANCE instance,
    HWND parent,
    BatchParameterEditorBinding& binding) noexcept {
    if (!RegisterEditorClass(instance)) {
        return nullptr;
    }
    return CreateWindowExW(
        WS_EX_CONTROLPARENT,
        kEditorClassName,
        UiText(UiStringId::BatchParameters),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_TABSTOP,
        0,
        0,
        0,
        0,
        parent,
        nullptr,
        instance,
        &binding);
}

void UpdateBatchParameterEditor(
    HWND editor,
    std::uint32_t selected_stage,
    bool enabled) noexcept {
    if (editor == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<EditorState*>(
        GetWindowLongPtrW(editor, GWLP_USERDATA));
    if (state == nullptr) {
        return;
    }
    if (state->stage != selected_stage) {
        state->selected_color_row = 0U;
    }
    state->stage = selected_stage;
    state->enabled = enabled;
    Refresh(*state, editor);
}

}  // namespace inkpod::windows::ui
