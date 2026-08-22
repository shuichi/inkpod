#include "batch_parameter_editor.h"

#include <commctrl.h>
#include <commdlg.h>

#include <algorithm>
#include <array>
#include <cwchar>
#include <new>
#include <string>

#include "app/frontend_state.h"
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
constexpr int kEnabled = 8;
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
constexpr int kContentHeightDip = 370;

struct EditorState {
    BatchParameterEditorBinding* binding{};
    std::uint32_t stage{};
    bool updating{};
    bool enabled{true};
    std::size_t selected_input{};
    int scroll_y{};
    HWND title{};
    HWND primary{};
    HWND secondary{};
    HWND path_label{};
    HWND path{};
    HWND template_label{};
    HWND naming_template{};
    HWND enabled_check{};
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
    return index < 0 ? 0U : static_cast<std::size_t>(index);
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
        column.cx = index == 1 ? 180 : 92;
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
            AddCell(state.rows, static_cast<int>(index), 1, ColorText(pair.old_color));
            AddCell(state.rows, static_cast<int>(index), 2, ColorText(pair.new_color));
        }
    } else {
        ResetColumns(state.rows, {UiText(UiStringId::BatchColors)});
        for (std::size_t index = 0U; index < operation.colors.size(); ++index) {
            AddCell(state.rows, static_cast<int>(index), 0, ColorText(operation.colors[index]));
        }
    }
    if (ListView_GetItemCount(state.rows) > 0) {
        ListView_SetItemState(state.rows, 0, LVIS_SELECTED, LVIS_SELECTED);
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
    place(state.primary, margin, y, width, row);
    place(state.secondary, margin, y, width, row);
    place(state.enabled_check, margin, y, width, row);
    y += row + gap;
    place(state.path_label, margin, y, width, row);
    y += row;
    place(state.path, margin, y, width, row);
    y += row + gap;
    place(state.template_label, margin, y, width, row);
    y += row;
    place(state.naming_template, margin, y, width, row);
    y += row + gap;
    const int half = std::max(0, (width - gap) / 2);
    place(state.first_label, margin, y, half, row);
    place(state.last_label, margin + half + gap, y, half, row);
    y += row;
    place(state.first, margin, y, half, row);
    place(state.last, margin + half + gap, y, half, row);
    y += row + gap;
    place(state.rows, margin, y, width, Scale(window, 116));
    y += Scale(window, 116) + gap;
    const int button_width = std::max(Scale(window, 72), (width - gap * 2) / 3);
    place(state.add, margin, y, button_width, row);
    place(state.remove, margin + button_width + gap, y, button_width, row);
    place(state.swap, margin + (button_width + gap) * 2, y, button_width, row);
    y += row + gap;
    place(state.current, margin, y, button_width, row);
    place(state.old_swatch, margin + button_width + gap, y, button_width, row);
    place(state.new_swatch, margin + (button_width + gap) * 2, y, button_width, row);

    const int viewport = client.bottom - client.top;
    const int content = Scale(window, kContentHeightDip);
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

void Refresh(EditorState& state, HWND window) noexcept {
    if (state.binding == nullptr || state.binding->draft == nullptr) {
        return;
    }
    auto& draft = *state.binding->draft;
    state.updating = true;
    const bool input = state.stage == 0U;
    const bool output = state.stage == draft.operations.size() + 1U;
    const bool operation = !input && !output;
    app::BatchOperationUi* const selected_operation =
        operation ? SelectedOperation(state) : nullptr;
    const bool color_replace = selected_operation != nullptr
        && selected_operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE;
    Show(state.primary, input || output);
    Show(state.secondary, output);
    Show(state.path_label, input || output);
    Show(state.path, input || output);
    Show(state.template_label, output);
    Show(state.naming_template, output);
    Show(state.enabled_check, operation);
    Show(state.first_label, input || operation);
    Show(state.first, input || operation);
    Show(state.last_label, input || operation);
    Show(state.last, input || operation);
    Show(state.rows, input || operation);
    Show(state.add, input || operation);
    Show(state.remove, input || operation);
    Show(state.swap, operation);
    Show(state.current, operation);
    Show(state.old_swatch, operation);
    Show(state.new_swatch, color_replace);

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
        SendMessageW(
            state.enabled_check,
            BM_SETCHECK,
            (selected->flags & INKPOD_BATCH_OPERATION_ENABLED) != 0U
                ? BST_CHECKED
                : BST_UNCHECKED,
            0);
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
    for (HWND control : {state.primary, state.secondary, state.path,
                         state.naming_template, state.enabled_check, state.first,
                         state.last, state.rows, state.add, state.remove, state.swap,
                         state.current, state.old_swatch, state.new_swatch}) {
        EnableWindow(control, state.enabled ? TRUE : FALSE);
    }
    if (input && !draft.inputs.empty()
        && draft.inputs[state.selected_input].kind
            == INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT) {
        EnableWindow(state.path, FALSE);
    }
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
    const std::size_t row = SelectedRow(state);
    if (operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        if (row >= operation->color_pairs.size()) {
            return nullptr;
        }
        return old_color ? &operation->color_pairs[row].old_color
                         : &operation->color_pairs[row].new_color;
    }
    return row < operation->colors.size() ? &operation->colors[row] : nullptr;
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
    InkpodColorValue* color{};
    if (operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        if (row >= operation->color_pairs.size()) {
            return;
        }
        color = old_color ? &operation->color_pairs[row].old_color
                          : &operation->color_pairs[row].new_color;
    } else {
        if (row >= operation->colors.size()) {
            return;
        }
        color = &operation->colors[row];
    }
    static std::array<COLORREF, 16U> custom{};
    const unsigned divisor = color->depth == INKPOD_COLOR_DEPTH_16 ? 257U : 1U;
    CHOOSECOLORW chooser{};
    chooser.lStructSize = sizeof(chooser);
    chooser.hwndOwner = window;
    chooser.rgbResult = RGB(
        color->red / divisor, color->green / divisor, color->blue / divisor);
    chooser.lpCustColors = custom.data();
    chooser.Flags = CC_FULLOPEN | CC_RGBINIT;
    if (ChooseColorW(&chooser) == FALSE) {
        return;
    }
    color->red = static_cast<std::uint16_t>(GetRValue(chooser.rgbResult) * divisor);
    color->green = static_cast<std::uint16_t>(GetGValue(chooser.rgbResult) * divisor);
    color->blue = static_cast<std::uint16_t>(GetBValue(chooser.rgbResult) * divisor);
    Changed(state);
    Refresh(state, window);
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
            state->enabled_check = Child(
                window, 0, WC_BUTTONW, UiText(UiStringId::BatchEnabled),
                BS_AUTOCHECKBOX | WS_TABSTOP, kEnabled);
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
                case kEnabled:
                    if (auto* operation = SelectedOperation(*state); operation != nullptr) {
                        if (SendMessageW(state->enabled_check, BM_GETCHECK, 0, 0)
                            == BST_CHECKED) {
                            operation->flags |= INKPOD_BATCH_OPERATION_ENABLED;
                        } else {
                            operation->flags &= ~INKPOD_BATCH_OPERATION_ENABLED;
                        }
                        Changed(*state);
                    }
                    return 0;
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
                case kPath:
                case kTemplate:
                case kFirst:
                case kLast:
                    if (HIWORD(wparam) == EN_KILLFOCUS) {
                        CommitText(*state);
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
                    if (auto* operation = SelectedOperation(*state); operation != nullptr) {
                        const std::size_t row = SelectedRow(*state);
                        if (operation->kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE
                            && row < operation->color_pairs.size()) {
                            operation->color_pairs[row].new_color = DrawingColor(*state);
                        } else if (row < operation->colors.size()) {
                            operation->colors[row] = DrawingColor(*state);
                        }
                        Changed(*state);
                        Refresh(*state, window);
                    }
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
    state->stage = selected_stage;
    state->enabled = enabled;
    Refresh(*state, editor);
}

}  // namespace inkpod::windows::ui
