#include "basic_dialogs.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <climits>
#include <cwchar>
#include <cwctype>
#include <new>
#include <string>
#include <utility>
#include <vector>

#include "app/resource.h"
#include "modal_dialog_position.h"

namespace inkpod::windows::ui {
namespace {

constexpr std::array<int, 4U> kShortcutHotkeyControls{
    IDC_SHORTCUT_HOTKEY,
    IDC_SHORTCUT_HOTKEY2,
    IDC_SHORTCUT_HOTKEY3,
    IDC_SHORTCUT_HOTKEY4};

WORD ShortcutHotkey(const InkpodShortcutStroke& stroke) noexcept {
    BYTE flags{};
    if ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_CONTROL) != 0U) {
        flags |= HOTKEYF_CONTROL;
    }
    if ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_SHIFT) != 0U) {
        flags |= HOTKEYF_SHIFT;
    }
    if ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_ALT) != 0U) {
        flags |= HOTKEYF_ALT;
    }
    if ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_EXTENDED) != 0U) {
        flags |= HOTKEYF_EXT;
    }
    return MAKEWORD(static_cast<BYTE>(stroke.virtual_key), flags);
}

void ShowShortcutSequence(HWND dialog, const InkpodShortcutSequence& sequence) noexcept {
    for (std::size_t index = 0; index < kShortcutHotkeyControls.size(); ++index) {
        const WORD hotkey = index < sequence.stroke_count
            ? ShortcutHotkey(sequence.strokes[index])
            : 0U;
        SendDlgItemMessageW(
            dialog, kShortcutHotkeyControls[index], HKM_SETHOTKEY, hotkey, 0);
    }
}

const ShortcutDialogEntry* FindShortcutEntry(
    const ShortcutDialogState& state, std::uint32_t command_id) noexcept {
    const auto found = std::find_if(
        state.entries.begin(), state.entries.end(), [command_id](const auto& entry) {
            return entry.command_id == command_id;
        });
    return found == state.entries.end() ? nullptr : &*found;
}

void SelectShortcutFromTypedPrefix(HWND dialog, const ShortcutDialogState& state) noexcept {
    const HWND commands = GetDlgItem(dialog, IDC_SHORTCUT_COMMAND);
    const int length = commands == nullptr ? 0 : GetWindowTextLengthW(commands);
    if (length <= 0) {
        return;
    }
    std::vector<wchar_t> text;
    try {
        text.resize(static_cast<std::size_t>(length) + 1U);
    } catch (const std::bad_alloc&) {
        return;
    }
    GetWindowTextW(commands, text.data(), static_cast<int>(text.size()));
    const LRESULT found = SendMessageW(
        commands, CB_FINDSTRING, static_cast<WPARAM>(-1), reinterpret_cast<LPARAM>(text.data()));
    if (found == CB_ERR) {
        return;
    }
    SendMessageW(commands, CB_SETCURSEL, static_cast<WPARAM>(found), 0);
    SendMessageW(commands, CB_SETEDITSEL, 0, MAKELPARAM(length, -1));
    const auto command_id = static_cast<std::uint32_t>(
        SendMessageW(commands, CB_GETITEMDATA, static_cast<WPARAM>(found), 0));
    if (const auto* entry = FindShortcutEntry(state, command_id)) {
        ShowShortcutSequence(dialog, entry->sequence);
    }
}

std::uint32_t ShortcutModifiers(BYTE hotkey_flags) noexcept {
    std::uint32_t modifiers{};
    if ((hotkey_flags & HOTKEYF_CONTROL) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_CONTROL;
    }
    if ((hotkey_flags & HOTKEYF_SHIFT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_SHIFT;
    }
    if ((hotkey_flags & HOTKEYF_ALT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_ALT;
    }
    if ((hotkey_flags & HOTKEYF_EXT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    }
    return modifiers;
}

INT_PTR CALLBACK ShortcutDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<ShortcutDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<ShortcutDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND commands = GetDlgItem(dialog, IDC_SHORTCUT_COMMAND);
            if (commands == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            for (const auto& entry : state->entries) {
                const LRESULT item = SendMessageW(
                    commands,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(entry.label.c_str()));
                if (item == CB_ERR || item == CB_ERRSPACE) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                SendMessageW(
                    commands,
                    CB_SETITEMDATA,
                    static_cast<WPARAM>(item),
                    static_cast<LPARAM>(entry.command_id));
            }
            SendMessageW(commands, CB_SETCURSEL, 0, 0);
            if (!state->entries.empty()) {
                ShowShortcutSequence(dialog, state->entries.front().sequence);
            }
            static_cast<void>(CenterModalDialogOnOwner(dialog));
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDC_SHORTCUT_COMMAND
                && HIWORD(wparam) == CBN_SELCHANGE) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_COMMAND, CB_GETCURSEL, 0, 0);
                if (selected != CB_ERR) {
                    const auto command_id = static_cast<std::uint32_t>(
                        SendDlgItemMessageW(
                            dialog,
                            IDC_SHORTCUT_COMMAND,
                            CB_GETITEMDATA,
                            static_cast<WPARAM>(selected),
                            0));
                    if (const auto* entry = FindShortcutEntry(*state, command_id)) {
                        ShowShortcutSequence(dialog, entry->sequence);
                    }
                }
                return TRUE;
            }
            if (LOWORD(wparam) == IDC_SHORTCUT_COMMAND
                && HIWORD(wparam) == CBN_EDITUPDATE) {
                SelectShortcutFromTypedPrefix(dialog, *state);
                return TRUE;
            }
            if (LOWORD(wparam) == IDOK) {
                LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_COMMAND, CB_GETCURSEL, 0, 0);
                if (selected == CB_ERR) {
                    std::array<wchar_t, 512U> typed{};
                    GetDlgItemTextW(
                        dialog,
                        IDC_SHORTCUT_COMMAND,
                        typed.data(),
                        static_cast<int>(typed.size()));
                    selected = SendDlgItemMessageW(
                        dialog,
                        IDC_SHORTCUT_COMMAND,
                        CB_FINDSTRINGEXACT,
                        static_cast<WPARAM>(-1),
                        reinterpret_cast<LPARAM>(typed.data()));
                }
                const LRESULT command_id = selected == CB_ERR
                    ? CB_ERR
                    : SendDlgItemMessageW(
                          dialog,
                          IDC_SHORTCUT_COMMAND,
                          CB_GETITEMDATA,
                          static_cast<WPARAM>(selected),
                          0);
                InkpodShortcutSequence sequence{};
                sequence.struct_size = sizeof(sequence);
                sequence.command_id = command_id == CB_ERR
                    ? 0U
                    : static_cast<std::uint32_t>(command_id);
                for (const int control : kShortcutHotkeyControls) {
                    const WORD hotkey = static_cast<WORD>(
                        SendDlgItemMessageW(dialog, control, HKM_GETHOTKEY, 0, 0));
                    if (LOBYTE(hotkey) == 0U) {
                        break;
                    }
                    auto& stroke = sequence.strokes[sequence.stroke_count++];
                    stroke.virtual_key = LOBYTE(hotkey);
                    stroke.modifiers = ShortcutModifiers(HIBYTE(hotkey));
                }
                if (command_id == CB_ERR || sequence.stroke_count == 0U) {
                    if (!state->close_immediately) {
                        MessageBoxW(
                            dialog,
                            L"コマンドとキーを指定してください。",
                            L"inkpod",
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                state->command_id = static_cast<std::uint32_t>(command_id);
                state->virtual_key = sequence.strokes[0].virtual_key;
                state->modifiers = sequence.strokes[0].modifiers;
                state->sequence = sequence;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR CALLBACK ViewOptionsDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<ViewOptionsDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    constexpr std::array<int, 4U> label_ids{
        IDC_VIEW_VALUE_LABEL,
        IDC_VIEW_VALUE2_LABEL,
        IDC_VIEW_VALUE3_LABEL,
        IDC_VIEW_VALUE4_LABEL};
    constexpr std::array<int, 4U> edit_ids{
        IDC_VIEW_VALUE,
        IDC_VIEW_VALUE2,
        IDC_VIEW_VALUE3,
        IDC_VIEW_VALUE4};
    constexpr std::array<int, 4U> choice_ids{
        IDC_VIEW_VALUE_CHOICE,
        IDC_VIEW_VALUE2_CHOICE,
        IDC_VIEW_VALUE3_CHOICE,
        IDC_VIEW_VALUE4_CHOICE};
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<ViewOptionsDialogState*>(lparam);
            bool invalid = state == nullptr || state->value_count == 0U
                || state->value_count > label_ids.size();
            if (!invalid) {
                for (std::size_t index = 0U; index < label_ids.size(); ++index) {
                    invalid = invalid
                        || (state->choices[index] == nullptr)
                            != (state->choice_counts[index] == 0U)
                        || (index >= state->value_count && state->choice_counts[index] != 0U);
                }
            }
            if (invalid) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            if (state->title != nullptr) {
                SetWindowTextW(dialog, state->title);
            }
            for (std::size_t index = 0; index < label_ids.size(); ++index) {
                const bool visible = index < state->value_count;
                const bool value_is_choice = visible && state->choice_counts[index] != 0U;
                ShowWindow(GetDlgItem(dialog, label_ids[index]), visible ? SW_SHOW : SW_HIDE);
                ShowWindow(
                    GetDlgItem(dialog, edit_ids[index]),
                    visible && !value_is_choice ? SW_SHOW : SW_HIDE);
                ShowWindow(
                    GetDlgItem(dialog, choice_ids[index]),
                    value_is_choice ? SW_SHOW : SW_HIDE);
                if (!visible) {
                    continue;
                }
                SetDlgItemTextW(
                    dialog,
                    label_ids[index],
                    state->labels[index] == nullptr ? L"値" : state->labels[index]);
                std::array<wchar_t, 32U> value{};
                _snwprintf_s(
                    value.data(), value.size(), _TRUNCATE, L"%d", state->values[index]);
                SetDlgItemTextW(dialog, edit_ids[index], value.data());
                if (!value_is_choice) {
                    continue;
                }
                const HWND choice_control = GetDlgItem(dialog, choice_ids[index]);
                int selected = CB_ERR;
                for (std::uint32_t choice_index = 0U;
                     choice_index < state->choice_counts[index];
                     ++choice_index) {
                    const auto& choice = state->choices[index][choice_index];
                    if (choice.label == nullptr) {
                        EndDialog(dialog, IDCANCEL);
                        return TRUE;
                    }
                    const LRESULT added = SendMessageW(
                        choice_control,
                        CB_ADDSTRING,
                        0,
                        reinterpret_cast<LPARAM>(choice.label));
                    if (added == CB_ERR || added == CB_ERRSPACE) {
                        EndDialog(dialog, IDCANCEL);
                        return TRUE;
                    }
                    if (choice.value == state->values[index]) {
                        selected = static_cast<int>(added);
                    }
                }
                if (selected == CB_ERR
                    || SendMessageW(choice_control, CB_SETCURSEL, selected, 0) == CB_ERR) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
            }
            state->centered_on_owner = CenterModalDialogOnOwner(dialog);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                std::array<std::int32_t, 4U> candidate_values = state->values;
                for (std::size_t index = 0; index < state->value_count; ++index) {
                    if (state->choice_counts[index] != 0U) {
                        const LRESULT selected = SendDlgItemMessageW(
                            dialog, choice_ids[index], CB_GETCURSEL, 0, 0);
                        if (selected == CB_ERR
                            || static_cast<std::uint64_t>(selected)
                                >= state->choice_counts[index]) {
                            return TRUE;
                        }
                        candidate_values[index] =
                            state->choices[index][static_cast<std::size_t>(selected)].value;
                        continue;
                    }
                    std::array<wchar_t, 32U> text{};
                    if (GetDlgItemTextW(
                            dialog,
                            edit_ids[index],
                            text.data(),
                            static_cast<int>(text.size())) <= 0) {
                        return TRUE;
                    }
                    wchar_t* end{};
                    errno = 0;
                    const long value = std::wcstol(text.data(), &end, 10);
                    if (errno == ERANGE || end == text.data() || *end != L'\0'
                        || value < INT_MIN || value > INT_MAX) {
                        if (!state->close_immediately) {
                            MessageBoxW(
                                dialog,
                                L"すべての値を整数で指定してください。",
                                L"inkpod",
                                MB_OK | MB_ICONWARNING);
                        }
                        return TRUE;
                    }
                    candidate_values[index] = static_cast<std::int32_t>(value);
                }
                state->values = candidate_values;
                const wchar_t* validation_error = state->validate == nullptr
                    ? nullptr
                    : state->validate(
                          state->validation_context, state->values, state->value_count);
                if (validation_error != nullptr) {
                    if (state->close_immediately) {
                        EndDialog(dialog, IDCANCEL);
                    } else {
                        MessageBoxW(
                            dialog,
                            validation_error,
                            state->title == nullptr ? L"inkpod" : state->title,
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

constexpr std::array<ViewOptionsDialogState::Choice, 5U> kCellAnchorChoices{{
    {L"左上", INKPOD_FRAME_ANCHOR_TOP_LEFT},
    {L"右上", INKPOD_FRAME_ANCHOR_TOP_RIGHT},
    {L"中央", INKPOD_FRAME_ANCHOR_CENTER},
    {L"左下", INKPOD_FRAME_ANCHOR_BOTTOM_LEFT},
    {L"右下", INKPOD_FRAME_ANCHOR_BOTTOM_RIGHT},
}};

constexpr std::array<ViewOptionsDialogState::Choice, 2U> kCellDepthChoices{{
    {L"8 bit/channel", INKPOD_STORAGE_RGBA8},
    {L"16 bit/channel", INKPOD_STORAGE_RGBA16},
}};

bool PopulateCellChoice(
    HWND dialog,
    int control_id,
    const ViewOptionsDialogState::Choice* choices,
    std::uint32_t choice_count,
    std::uint32_t selected_value) noexcept {
    if (choices == nullptr || choice_count == 0U) {
        return false;
    }
    const HWND control = GetDlgItem(dialog, control_id);
    int selected = CB_ERR;
    for (std::uint32_t index = 0U; index < choice_count; ++index) {
        if (choices[index].label == nullptr || choices[index].value < 0) {
            return false;
        }
        const LRESULT added = SendMessageW(
            control, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(choices[index].label));
        if (added == CB_ERR || added == CB_ERRSPACE
            || SendMessageW(
                   control,
                   CB_SETITEMDATA,
                   static_cast<WPARAM>(added),
                   static_cast<LPARAM>(choices[index].value))
                == CB_ERR) {
            return false;
        }
        if (static_cast<std::uint32_t>(choices[index].value) == selected_value) {
            selected = static_cast<int>(added);
        }
    }
    return selected != CB_ERR
        && SendMessageW(control, CB_SETCURSEL, static_cast<WPARAM>(selected), 0) != CB_ERR;
}

bool ReadCellUnsigned(HWND dialog, int control_id, std::uint32_t& output) noexcept {
    std::array<wchar_t, 32U> text{};
    if (GetDlgItemTextW(
            dialog, control_id, text.data(), static_cast<int>(text.size())) <= 0) {
        return false;
    }
    wchar_t* end{};
    errno = 0;
    const unsigned long value = std::wcstoul(text.data(), &end, 10);
    if (errno == ERANGE || end == text.data() || *end != L'\0' || value > UINT32_MAX) {
        return false;
    }
    output = static_cast<std::uint32_t>(value);
    return true;
}

bool ReadCellChoice(HWND dialog, int control_id, std::uint32_t& output) noexcept {
    const LRESULT selected = SendDlgItemMessageW(dialog, control_id, CB_GETCURSEL, 0, 0);
    if (selected == CB_ERR) {
        return false;
    }
    const LRESULT value = SendDlgItemMessageW(
        dialog, control_id, CB_GETITEMDATA, static_cast<WPARAM>(selected), 0);
    if (value == CB_ERR || value < 0 || static_cast<std::uint64_t>(value) > UINT32_MAX) {
        return false;
    }
    output = static_cast<std::uint32_t>(value);
    return true;
}

void SetCellUnsigned(HWND dialog, int control_id, std::uint32_t value) noexcept {
    std::array<wchar_t, 32U> text{};
    _snwprintf_s(text.data(), text.size(), _TRUNCATE, L"%u", value);
    SetDlgItemTextW(dialog, control_id, text.data());
}

bool BuildCellCreationPreview(
    HWND dialog,
    const CellCreationDialogState& state,
    InkpodCellCreationOptions& options,
    InkpodCellCreationPlanItem& preview) noexcept {
    options = state.options;
    options.struct_size = sizeof(options);
    options.sizing_mode = IsDlgButtonChecked(dialog, IDC_CELL_SIZING_FRAME) == BST_CHECKED
        ? INKPOD_CELL_SIZING_FRAME_MICROMETRES
        : INKPOD_CELL_SIZING_IMAGE_PIXELS;
    std::uint32_t dpi{};
    if (!ReadCellUnsigned(dialog, IDC_CELL_WIDTH, options.width)
        || !ReadCellUnsigned(dialog, IDC_CELL_HEIGHT, options.height)
        || !ReadCellUnsigned(dialog, IDC_CELL_DPI, dpi)
        || !ReadCellUnsigned(dialog, IDC_CELL_MARGIN, options.margin_milli)
        || !ReadCellUnsigned(dialog, IDC_CELL_SAFE_RATIO, options.safe_frame_ratio_milli)
        || !ReadCellUnsigned(
            dialog,
            IDC_CELL_MAXIMUM_CLOSE_RATIO,
            options.maximum_close_ratio_milli)
        || !ReadCellChoice(dialog, IDC_CELL_ANCHOR, options.anchor)
        || !ReadCellChoice(dialog, IDC_CELL_LAYER_KIND, options.initial_layer_kind)
        || !ReadCellChoice(dialog, IDC_CELL_DEPTH, options.pixel_format)
        || !ReadCellUnsigned(dialog, IDC_CELL_COUNT, options.count)
        || dpi == 0U || dpi > 2'400U) {
        return false;
    }
    options.dpi_x_milli = dpi * 1'000U;
    options.dpi_y_milli = dpi * 1'000U;

    preview = {};
    preview.struct_size = sizeof(preview);
    return state.build_preview != nullptr
        && state.build_preview(state.preview_context, options, preview);
}

bool RefreshCellCreationPreview(HWND dialog, CellCreationDialogState& state) noexcept {
    const bool physical =
        IsDlgButtonChecked(dialog, IDC_CELL_SIZING_FRAME) == BST_CHECKED;
    SetDlgItemTextW(
        dialog, IDC_CELL_WIDTH_LABEL, physical ? L"幅 (μm)" : L"幅 (px)");
    SetDlgItemTextW(
        dialog, IDC_CELL_HEIGHT_LABEL, physical ? L"高さ (μm)" : L"高さ (px)");

    InkpodCellCreationOptions options{};
    InkpodCellCreationPlanItem preview{};
    if (!BuildCellCreationPreview(dialog, state, options, preview)) {
        SetDlgItemTextW(
            dialog,
            IDC_CELL_SUMMARY,
            L"入力条件から有効な作成計画を計算できません。値と上限を確認してください。");
        EnableWindow(GetDlgItem(dialog, IDOK), FALSE);
        return false;
    }
    std::array<wchar_t, 512U> summary{};
    _snwprintf_s(
        summary.data(),
        summary.size(),
        _TRUNCATE,
        L"画像 %u x %u px / DPI %.3f / %u Cell\r\n"
        L"100%%フレーム (%d, %d) %d x %d / 安全 %d x %d\r\n"
        L"最大寄り (%d, %d) %d x %d / 余白 L%u T%u R%u B%u",
        preview.width,
        preview.height,
        static_cast<double>(preview.dpi_x_milli) / 1000.0,
        options.count,
        preview.hundred_frame.x,
        preview.hundred_frame.y,
        preview.hundred_frame.width,
        preview.hundred_frame.height,
        preview.safe_frame.width,
        preview.safe_frame.height,
        preview.maximum_close_frame.x,
        preview.maximum_close_frame.y,
        preview.maximum_close_frame.width,
        preview.maximum_close_frame.height,
        preview.margin_left,
        preview.margin_top,
        preview.margin_right,
        preview.margin_bottom);
    SetDlgItemTextW(dialog, IDC_CELL_SUMMARY, summary.data());
    state.preview = preview;
    EnableWindow(GetDlgItem(dialog, IDOK), TRUE);
    return true;
}

INT_PTR CALLBACK CellCreationDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<CellCreationDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<CellCreationDialogState*>(lparam);
            if (state == nullptr || state->options.struct_size != sizeof(state->options)
                || state->build_preview == nullptr
                || !PopulateCellChoice(
                    dialog,
                    IDC_CELL_ANCHOR,
                    kCellAnchorChoices.data(),
                    static_cast<std::uint32_t>(kCellAnchorChoices.size()),
                    state->options.anchor)
                || !PopulateCellChoice(
                    dialog,
                    IDC_CELL_LAYER_KIND,
                    state->layer_choices,
                    state->layer_choice_count,
                    state->options.initial_layer_kind)
                || !PopulateCellChoice(
                    dialog,
                    IDC_CELL_DEPTH,
                    kCellDepthChoices.data(),
                    static_cast<std::uint32_t>(kCellDepthChoices.size()),
                    state->options.pixel_format)) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            CheckRadioButton(
                dialog,
                IDC_CELL_SIZING_IMAGE,
                IDC_CELL_SIZING_FRAME,
                state->options.sizing_mode == INKPOD_CELL_SIZING_FRAME_MICROMETRES
                    ? IDC_CELL_SIZING_FRAME
                    : IDC_CELL_SIZING_IMAGE);
            SetCellUnsigned(dialog, IDC_CELL_WIDTH, state->options.width);
            SetCellUnsigned(dialog, IDC_CELL_HEIGHT, state->options.height);
            SetCellUnsigned(dialog, IDC_CELL_DPI, state->options.dpi_x_milli / 1'000U);
            SetCellUnsigned(dialog, IDC_CELL_MARGIN, state->options.margin_milli);
            SetCellUnsigned(
                dialog, IDC_CELL_SAFE_RATIO, state->options.safe_frame_ratio_milli);
            SetCellUnsigned(
                dialog,
                IDC_CELL_MAXIMUM_CLOSE_RATIO,
                state->options.maximum_close_ratio_milli);
            SetCellUnsigned(dialog, IDC_CELL_COUNT, state->options.count);
            SendDlgItemMessageW(dialog, IDC_CELL_WIDTH_SPIN, UDM_SETRANGE32, 1, 1'048'576);
            SendDlgItemMessageW(dialog, IDC_CELL_HEIGHT_SPIN, UDM_SETRANGE32, 1, 1'048'576);
            SendDlgItemMessageW(dialog, IDC_CELL_DPI_SPIN, UDM_SETRANGE32, 1, 2'400);
            SendDlgItemMessageW(dialog, IDC_CELL_MARGIN_SPIN, UDM_SETRANGE32, 0, 1'000);
            SendDlgItemMessageW(dialog, IDC_CELL_SAFE_RATIO_SPIN, UDM_SETRANGE32, 1, 1'000);
            SendDlgItemMessageW(
                dialog, IDC_CELL_MAXIMUM_CLOSE_RATIO_SPIN, UDM_SETRANGE32, 1, 1'000);
            SendDlgItemMessageW(
                dialog,
                IDC_CELL_COUNT_SPIN,
                UDM_SETRANGE32,
                1,
                INKPOD_MAX_CELL_CREATION_COUNT);
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            state->centered_on_owner = CenterModalDialogOnOwner(dialog);
            (void)RefreshCellCreationPreview(dialog, *state);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                InkpodCellCreationOptions options{};
                InkpodCellCreationPlanItem preview{};
                if (!BuildCellCreationPreview(dialog, *state, options, preview)) {
                    (void)RefreshCellCreationPreview(dialog, *state);
                    return TRUE;
                }
                state->options = options;
                state->preview = preview;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            if (HIWORD(wparam) == EN_CHANGE || HIWORD(wparam) == CBN_SELCHANGE
                || HIWORD(wparam) == BN_CLICKED) {
                (void)RefreshCellCreationPreview(dialog, *state);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR CALLBACK TextInputDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<TextInputDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<TextInputDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            if (state->title != nullptr) {
                SetWindowTextW(dialog, state->title);
            }
            SetDlgItemTextW(
                dialog, IDC_TEXT_INPUT_LABEL,
                state->label == nullptr ? L"値" : state->label);
            SetDlgItemTextW(dialog, IDC_TEXT_INPUT_VALUE, state->value.c_str());
            static_cast<void>(CenterModalDialogOnOwner(dialog));
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                std::array<wchar_t, 1025U> text{};
                const int length = GetDlgItemTextW(
                    dialog, IDC_TEXT_INPUT_VALUE, text.data(), static_cast<int>(text.size()));
                if (length <= 0) {
                    return TRUE;
                }
                try {
                    state->value.assign(text.data(), static_cast<std::size_t>(length));
                } catch (const std::bad_alloc&) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

struct FillOptionsDialogState {
    FillToolOptions options;
    bool close_immediately{};
};

bool ParseFillColors(
    const wchar_t* text, std::vector<InkpodColorValue>& colors) noexcept {
    colors.clear();
    if (text == nullptr) {
        return false;
    }
    try {
        const std::wstring input(text);
        std::size_t start{};
        while (start < input.size()) {
            const std::size_t separator = input.find(L';', start);
            const std::size_t end = separator == std::wstring::npos
                ? input.size()
                : separator;
            std::size_t first = start;
            while (first < end && iswspace(input[first]) != 0) {
                ++first;
            }
            std::size_t last = end;
            while (last > first && iswspace(input[last - 1U]) != 0) {
                --last;
            }
            if (first != last) {
                const std::size_t digits = last - first;
                if ((digits != 8U && digits != 16U) || colors.size() >= 6U) {
                    return false;
                }
                const std::wstring token = input.substr(first, last - first);
                wchar_t* token_end{};
                errno = 0;
                const unsigned long long value = std::wcstoull(
                    token.c_str(), &token_end, 16);
                if (errno == ERANGE || token_end == token.c_str()
                    || *token_end != L'\0') {
                    return false;
                }
                const std::uint32_t shift = digits == 16U ? 16U : 8U;
                const std::uint64_t mask = digits == 16U ? UINT16_MAX : UINT8_MAX;
                colors.push_back(InkpodColorValue{
                    sizeof(InkpodColorValue),
                    digits == 16U ? INKPOD_COLOR_DEPTH_16 : INKPOD_COLOR_DEPTH_8,
                    static_cast<std::uint16_t>((value >> (shift * 3U)) & mask),
                    static_cast<std::uint16_t>((value >> (shift * 2U)) & mask),
                    static_cast<std::uint16_t>((value >> shift) & mask),
                    static_cast<std::uint16_t>(value & mask)});
            }
            if (separator == std::wstring::npos) {
                break;
            }
            start = separator + 1U;
        }
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

INT_PTR CALLBACK FillOptionsDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<FillOptionsDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<FillOptionsDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND operation = GetDlgItem(dialog, IDC_FILL_OPERATION);
            for (const wchar_t* label : {L"通常フィル", L"閉領域フィル", L"塗りのばし"}) {
                SendMessageW(
                    operation,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(label));
            }
            SendMessageW(
                operation,
                CB_SETCURSEL,
                state->options.operation == INKPOD_FILL_CLOSED_REGION
                    ? 1
                    : (state->options.operation == INKPOD_FILL_EXTENSION ? 2 : 0),
                0);
            const HWND inclusion = GetDlgItem(dialog, IDC_FILL_INCLUSION_MODE);
            for (const wchar_t* label : {L"なし", L"指定色", L"指定色以外"}) {
                SendMessageW(
                    inclusion,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(label));
            }
            SendMessageW(
                inclusion,
                CB_SETCURSEL,
                state->options.inclusion_mode == INKPOD_INCLUSION_SPECIFIED
                    ? 1
                    : (state->options.inclusion_mode == INKPOD_INCLUSION_EXCEPT_SPECIFIED
                              ? 2
                              : 0),
                0);
            SetDlgItemInt(dialog, IDC_FILL_TOLERANCE, state->options.tolerance, FALSE);
            SetDlgItemInt(dialog, IDC_FILL_GAP, state->options.gap_close, FALSE);
            SetDlgItemInt(
                dialog, IDC_FILL_EXTENSION, state->options.extension_distance, FALSE);
            std::wstring color_text;
            try {
                std::array<wchar_t, 20U> token{};
                for (std::size_t index = 0; index < state->options.inclusion_colors.size(); ++index) {
                    if (index != 0U) {
                        color_text += L';';
                    }
                    const InkpodColorValue& color =
                        state->options.inclusion_colors[index];
                    if (color.depth == INKPOD_COLOR_DEPTH_16) {
                        _snwprintf_s(
                            token.data(),
                            token.size(),
                            _TRUNCATE,
                            L"%04X%04X%04X%04X",
                            color.red,
                            color.green,
                            color.blue,
                            color.alpha);
                    } else {
                        _snwprintf_s(
                            token.data(),
                            token.size(),
                            _TRUNCATE,
                            L"%02X%02X%02X%02X",
                            color.red,
                            color.green,
                            color.blue,
                            color.alpha);
                    }
                    color_text += token.data();
                }
            } catch (const std::bad_alloc&) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetDlgItemTextW(dialog, IDC_FILL_COLORS, color_text.c_str());
            for (const auto& [control, checked] : std::array<std::pair<int, bool>, 6U>{
                     std::pair{IDC_FILL_OVERFLOW, state->options.overflow_abort},
                     std::pair{IDC_FILL_DETACHED, state->options.detached_regions},
                     std::pair{IDC_FILL_TRANSPARENT, state->options.transparent_only},
                     std::pair{IDC_FILL_SELECTION, state->options.use_document_selection},
                     std::pair{IDC_FILL_LIGHT_BOUNDARY, state->options.light_table_boundary},
                     std::pair{IDC_FILL_LIGHT_COLOR, state->options.light_table_color}}) {
                CheckDlgButton(dialog, control, checked ? BST_CHECKED : BST_UNCHECKED);
            }
            static_cast<void>(CenterModalDialogOnOwner(dialog));
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                BOOL tolerance_ok{};
                BOOL gap_ok{};
                BOOL extension_ok{};
                const UINT tolerance = GetDlgItemInt(
                    dialog, IDC_FILL_TOLERANCE, &tolerance_ok, FALSE);
                const UINT gap = GetDlgItemInt(dialog, IDC_FILL_GAP, &gap_ok, FALSE);
                const UINT extension = GetDlgItemInt(
                    dialog, IDC_FILL_EXTENSION, &extension_ok, FALSE);
                std::array<wchar_t, 256U> color_text{};
                GetDlgItemTextW(
                    dialog,
                    IDC_FILL_COLORS,
                    color_text.data(),
                    static_cast<int>(color_text.size()));
                std::vector<InkpodColorValue> colors;
                if (tolerance_ok == FALSE || gap_ok == FALSE || extension_ok == FALSE
                    || tolerance > UINT16_MAX || gap > UINT16_MAX || extension == 0U
                    || !ParseFillColors(color_text.data(), colors)) {
                    if (!state->close_immediately) {
                        MessageBoxW(
                            dialog,
                            L"許容差、隙間、距離、対象色を確認してください。",
                            L"inkpod",
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                const LRESULT operation = SendDlgItemMessageW(
                    dialog, IDC_FILL_OPERATION, CB_GETCURSEL, 0, 0);
                const LRESULT inclusion = SendDlgItemMessageW(
                    dialog, IDC_FILL_INCLUSION_MODE, CB_GETCURSEL, 0, 0);
                if (operation == CB_ERR || inclusion == CB_ERR) {
                    return TRUE;
                }
                state->options.operation = operation == 1
                    ? INKPOD_FILL_CLOSED_REGION
                    : (operation == 2 ? INKPOD_FILL_EXTENSION : INKPOD_FILL_SEED);
                state->options.inclusion_mode = inclusion == 1
                    ? INKPOD_INCLUSION_SPECIFIED
                    : (inclusion == 2 ? INKPOD_INCLUSION_EXCEPT_SPECIFIED
                                      : INKPOD_INCLUSION_NONE);
                if (state->options.inclusion_mode != INKPOD_INCLUSION_NONE
                    && colors.empty()) {
                    if (!state->close_immediately) {
                        MessageBoxW(
                            dialog,
                            L"含み塗りには対象色が必要です。",
                            L"inkpod",
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                state->options.tolerance = static_cast<std::uint16_t>(tolerance);
                state->options.gap_close = static_cast<std::uint16_t>(gap);
                state->options.extension_distance = extension;
                state->options.inclusion_colors = std::move(colors);
                state->options.overflow_abort =
                    IsDlgButtonChecked(dialog, IDC_FILL_OVERFLOW) == BST_CHECKED;
                state->options.detached_regions =
                    IsDlgButtonChecked(dialog, IDC_FILL_DETACHED) == BST_CHECKED;
                state->options.transparent_only =
                    IsDlgButtonChecked(dialog, IDC_FILL_TRANSPARENT) == BST_CHECKED;
                state->options.use_document_selection =
                    IsDlgButtonChecked(dialog, IDC_FILL_SELECTION) == BST_CHECKED;
                state->options.light_table_boundary =
                    IsDlgButtonChecked(dialog, IDC_FILL_LIGHT_BOUNDARY) == BST_CHECKED;
                state->options.light_table_color =
                    IsDlgButtonChecked(dialog, IDC_FILL_LIGHT_COLOR) == BST_CHECKED;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR CALLBACK HistoryDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<HistoryDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<HistoryDialogState*>(lparam);
            if (state == nullptr || state->labels.empty()
                || state->labels.size() != state->cursors.size()) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND list = GetDlgItem(dialog, IDC_HISTORY_LIST);
            for (std::size_t index = 0; index < state->labels.size(); ++index) {
                const LRESULT item = SendMessageW(
                    list,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(state->labels[index].c_str()));
                if (item == CB_ERR || item == CB_ERRSPACE) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                SendMessageW(
                    list,
                    CB_SETITEMDATA,
                    static_cast<WPARAM>(item),
                    static_cast<LPARAM>(index));
            }
            const auto selected = static_cast<WPARAM>(
                std::min(state->selected_index, state->labels.size() - 1U));
            SendMessageW(list, CB_SETCURSEL, selected, 0);
            static_cast<void>(CenterModalDialogOnOwner(dialog));
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_HISTORY_LIST, CB_GETCURSEL, 0, 0);
                if (selected == CB_ERR) {
                    return TRUE;
                }
                const auto index = static_cast<std::size_t>(SendDlgItemMessageW(
                    dialog,
                    IDC_HISTORY_LIST,
                    CB_GETITEMDATA,
                    static_cast<WPARAM>(selected),
                    0));
                if (index >= state->cursors.size()) {
                    return TRUE;
                }
                state->selected_cursor = state->cursors[index];
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

UINT ShortcutMenuCommand(std::uint32_t command_id) noexcept {
    switch (command_id) {
        case 1U: return IDM_EDIT_UNDO;
        case 2U: return IDM_EDIT_REDO;
        case 3U: return IDM_EDIT_COPY;
        case 4U: return IDM_EDIT_PASTE;
        default: break;
    }
    return command_id >= 40000U && command_id <= 42099U
        ? static_cast<UINT>(command_id)
        : 0U;
}

INT_PTR ShowShortcutEditor(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    ShortcutDialogState& state) noexcept {
    ShortcutDialogState candidate = state;
    candidate.close_immediately = close_immediately;
    const INT_PTR result = DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_SHORTCUT_EDITOR),
        owner,
        ShortcutDialogProcedure,
        reinterpret_cast<LPARAM>(&candidate));
    if (result == IDOK) {
        state = candidate;
    }
    return result;
}

INT_PTR ShowViewOptions(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    ViewOptionsDialogState& state) noexcept {
    ViewOptionsDialogState candidate = state;
    candidate.close_immediately = close_immediately;
    const INT_PTR result = DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_VIEW_OPTIONS),
        owner,
        ViewOptionsDialogProcedure,
        reinterpret_cast<LPARAM>(&candidate));
    if (result == IDOK) {
        state = candidate;
    }
    return result;
}

INT_PTR ShowCellCreationOptions(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    CellCreationDialogState& state) noexcept {
    CellCreationDialogState candidate = state;
    candidate.close_immediately = close_immediately;
    const INT_PTR result = DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_CELL_CREATION),
        owner,
        CellCreationDialogProcedure,
        reinterpret_cast<LPARAM>(&candidate));
    if (result == IDOK) {
        state = candidate;
    }
    return result;
}

INT_PTR ShowTextInput(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    TextInputDialogState& state) noexcept {
    try {
        TextInputDialogState candidate = state;
        candidate.close_immediately = close_immediately;
        const INT_PTR result = DialogBoxParamW(
            instance,
            MAKEINTRESOURCEW(IDD_TEXT_INPUT),
            owner,
            TextInputDialogProcedure,
            reinterpret_cast<LPARAM>(&candidate));
        if (result == IDOK) {
            state = std::move(candidate);
        }
        return result;
    } catch (const std::bad_alloc&) {
        return IDCANCEL;
    }
}

bool ShowFillOptions(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    FillToolOptions& options) noexcept {
    try {
        FillOptionsDialogState state{options, close_immediately};
        if (DialogBoxParamW(
                instance,
                MAKEINTRESOURCEW(IDD_FILL_OPTIONS),
                owner,
                FillOptionsDialogProcedure,
                reinterpret_cast<LPARAM>(&state)) != IDOK) {
            return false;
        }
        options = std::move(state.options);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

INT_PTR ShowHistoryDialog(
    HINSTANCE instance, HWND owner, HistoryDialogState& state) noexcept {
    try {
        HistoryDialogState candidate = state;
        const INT_PTR result = DialogBoxParamW(
            instance,
            MAKEINTRESOURCEW(IDD_HISTORY),
            owner,
            HistoryDialogProcedure,
            reinterpret_cast<LPARAM>(&candidate));
        if (result == IDOK) {
            state = std::move(candidate);
        }
        return result;
    } catch (const std::bad_alloc&) {
        return IDCANCEL;
    }
}

}  // namespace inkpod::windows::ui
