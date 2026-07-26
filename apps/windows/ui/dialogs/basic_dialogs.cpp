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

namespace inkpod::windows::ui {
namespace {

struct ShortcutEntry {
    std::uint32_t command_id;
    UINT menu_command;
    const wchar_t* label;
};

constexpr std::array<ShortcutEntry, 24U> kShortcutEntries{{
    {1U, IDM_EDIT_UNDO, L"[メニュー] 元に戻す"},
    {2U, IDM_EDIT_REDO, L"[メニュー] やり直し"},
    {3U, IDM_EDIT_COPY, L"[メニュー] コピー"},
    {4U, IDM_EDIT_PASTE, L"[メニュー] 貼り付け"},
    {IDM_FILE_NEW, IDM_FILE_NEW, L"[メニュー] 新規セル"},
    {IDM_FILE_OPEN, IDM_FILE_OPEN, L"[メニュー] 開く"},
    {IDM_FILE_SAVE, IDM_FILE_SAVE, L"[メニュー] 保存"},
    {IDM_VIEW_FIT, IDM_VIEW_FIT, L"[メニュー] 全体表示"},
    {IDM_VIEW_ONE_TO_ONE, IDM_VIEW_ONE_TO_ONE, L"[メニュー] ピクセル等倍"},
    {IDM_VIEW_FLIP_HORIZONTAL, IDM_VIEW_FLIP_HORIZONTAL, L"[メニュー] 表示を左右反転"},
    {IDM_VIEW_FLIP_VERTICAL, IDM_VIEW_FLIP_VERTICAL, L"[メニュー] 表示を上下反転"},
    {IDM_SELECTION_ALL, IDM_SELECTION_ALL, L"[メニュー] すべて選択"},
    {IDM_SELECTION_CLEAR, IDM_SELECTION_CLEAR, L"[メニュー] 選択解除"},
    {IDM_TOOL_PENCIL, IDM_TOOL_PENCIL, L"[ツール] 鉛筆"},
    {IDM_TOOL_BRUSH, IDM_TOOL_BRUSH, L"[ツール] ブラシ"},
    {IDM_TOOL_ERASER, IDM_TOOL_ERASER, L"[ツール] 消しゴム"},
    {IDM_TOOL_FILL, IDM_TOOL_FILL, L"[ツール] フィル"},
    {IDM_TOOL_EYEDROPPER, IDM_TOOL_EYEDROPPER, L"[ツール] スポイト"},
    {IDM_SELECTION_RECTANGLE, IDM_SELECTION_RECTANGLE, L"[ツール] 長方形選択"},
    {IDM_SELECTION_WAND, IDM_SELECTION_WAND, L"[ツール] 色の杖"},
    {IDM_VIEW_GRID, IDM_VIEW_GRID, L"[その他] グリッド表示"},
    {IDM_VIEW_GUIDES, IDM_VIEW_GUIDES, L"[その他] ガイド表示"},
    {IDM_VIEW_NEW, IDM_VIEW_NEW, L"[その他] 新規セルビュー"},
    {IDM_COLOR_CHOOSE, IDM_COLOR_CHOOSE, L"[その他] 描画色"},
}};

constexpr WORD DefaultShortcutHotkey(std::uint32_t command_id) noexcept {
    const BYTE key = command_id == 2U
        ? static_cast<BYTE>('Y')
        : (command_id == 3U
                  ? static_cast<BYTE>('C')
                  : (command_id == 4U ? static_cast<BYTE>('V')
                                      : (command_id == 1U ? static_cast<BYTE>('Z') : 0U)));
    return key == 0U ? 0U : MAKEWORD(key, HOTKEYF_CONTROL);
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
            for (const auto& entry : kShortcutEntries) {
                const LRESULT item = SendMessageW(
                    commands,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(entry.label));
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
            SendDlgItemMessageW(
                dialog,
                IDC_SHORTCUT_HOTKEY,
                HKM_SETHOTKEY,
                DefaultShortcutHotkey(1U),
                0);
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
                    SendDlgItemMessageW(
                        dialog,
                        IDC_SHORTCUT_HOTKEY,
                        HKM_SETHOTKEY,
                        DefaultShortcutHotkey(command_id),
                        0);
                }
                return TRUE;
            }
            if (LOWORD(wparam) == IDOK) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_COMMAND, CB_GETCURSEL, 0, 0);
                const LRESULT command_id = selected == CB_ERR
                    ? CB_ERR
                    : SendDlgItemMessageW(
                          dialog,
                          IDC_SHORTCUT_COMMAND,
                          CB_GETITEMDATA,
                          static_cast<WPARAM>(selected),
                          0);
                const WORD hotkey = static_cast<WORD>(SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_HOTKEY, HKM_GETHOTKEY, 0, 0));
                if (command_id == CB_ERR || LOBYTE(hotkey) == 0U) {
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
                state->virtual_key = LOBYTE(hotkey);
                state->modifiers = ShortcutModifiers(HIBYTE(hotkey));
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
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<ViewOptionsDialogState*>(lparam);
            if (state == nullptr || state->value_count == 0U
                || state->value_count > label_ids.size()) {
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
                ShowWindow(GetDlgItem(dialog, label_ids[index]), visible ? SW_SHOW : SW_HIDE);
                ShowWindow(GetDlgItem(dialog, edit_ids[index]), visible ? SW_SHOW : SW_HIDE);
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
            }
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
                for (std::size_t index = 0; index < state->value_count; ++index) {
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
                    state->values[index] = static_cast<std::int32_t>(value);
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

bool ParseFillColors(const wchar_t* text, std::vector<std::uint32_t>& colors) noexcept {
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
                if (last - first != 8U || colors.size() >= 6U) {
                    return false;
                }
                const std::wstring token = input.substr(first, last - first);
                wchar_t* token_end{};
                errno = 0;
                const unsigned long value = std::wcstoul(
                    token.c_str(), &token_end, 16);
                if (errno == ERANGE || token_end == token.c_str()
                    || *token_end != L'\0') {
                    return false;
                }
                colors.push_back(static_cast<std::uint32_t>(value));
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
                std::array<wchar_t, 16U> token{};
                for (std::size_t index = 0; index < state->options.inclusion_rgba.size(); ++index) {
                    if (index != 0U) {
                        color_text += L';';
                    }
                    _snwprintf_s(
                        token.data(),
                        token.size(),
                        _TRUNCATE,
                        L"%08X",
                        state->options.inclusion_rgba[index]);
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
                std::vector<std::uint32_t> colors;
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
                state->options.inclusion_rgba = std::move(colors);
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
    for (const auto& entry : kShortcutEntries) {
        if (entry.command_id == command_id) {
            return entry.menu_command;
        }
    }
    return 0U;
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
