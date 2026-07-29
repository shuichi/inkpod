#include "command_catalog.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cwchar>
#include <initializer_list>
#include <limits>
#include <map>
#include <new>
#include <string>
#include <utility>

#include "app/resource.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT kCommandIds[] = {
#define INKPOD_COMMAND_STATE(owner, command) command,
#include "command_state_catalog.inc"
#undef INKPOD_COMMAND_STATE
};

constexpr InkpodShortcutStroke Stroke(UINT key, std::uint32_t modifiers = 0U) noexcept {
    return {key, modifiers};
}

InkpodShortcutSequence Sequence(
    UINT command,
    std::initializer_list<InkpodShortcutStroke> strokes) noexcept {
    InkpodShortcutSequence sequence{};
    sequence.struct_size = sizeof(sequence);
    sequence.command_id = command;
    sequence.stroke_count = static_cast<std::uint32_t>(strokes.size());
    std::copy(strokes.begin(), strokes.end(), sequence.strokes);
    return sequence;
}

wchar_t GroupKey(UINT group) noexcept {
    switch (group) {
        case 400: return L'F';
        case 401: return L'E';
        case 402: return L'V';
        case 403: return L'T';
        case 404: return L'U';
        case 405: return L'C';
        case 406: return L'H';
        case 407: return L'J';
        case 408: return L'S';
        case 409: return L'K';
        case 410: return L'I';
        case 411: return L'G';
        case 412: return L'A';
        case 413: return L'D';
        case 414: return L'L';
        case 415: return L'P';
        case 416: return L'R';
        case 417: return L'M';
        case 418: return L'X';
        default: return 0;
    }
}

const wchar_t* GroupName(UINT command) noexcept {
    switch (command / 100U) {
        case 400: return L"ファイル";
        case 401: return L"編集";
        case 402: return L"表示";
        case 403: return L"ツール";
        case 404: return L"プレーン";
        case 405: return L"色";
        case 406: return L"ヘルプ";
        case 407: return L"レイヤー";
        case 408: return L"選択範囲";
        case 409: return L"設定";
        case 410: return L"フィルタ";
        case 411: return L"特効";
        case 412: return L"調整レイヤー";
        case 413: return L"セル";
        case 414: return L"レイヤー";
        case 415: return L"プレーン";
        case 416: return L"ライトテーブル";
        case 417: return L"連番・モーション";
        case 418: return L"ベクター";
        case 419:
            return command == IDM_WINDOW_TOOL_PALETTE
                    || command == IDM_WINDOW_LAYER_PALETTE
                    || command == IDM_WINDOW_TOOL_OPTIONS
                    || command == IDM_WINDOW_COLOR_PANE
                    || command == IDM_WORKSPACE_RESET
                    || command == IDM_WORKSPACE_SAVE
                    || command == IDM_WORKSPACE_RESTORE
                    || command == IDM_WORKSPACE_MIRROR
                    || command == IDM_WINDOW_BATCH
                ? L"ウィンドウ"
                : L"バッチ";
        case 420: return L"バッチ項目";
        default: return L"その他";
    }
}

bool DirectSequence(UINT command, InkpodShortcutSequence& sequence) noexcept {
    constexpr auto control = INKPOD_SHORTCUT_MODIFIER_CONTROL;
    constexpr auto shift = INKPOD_SHORTCUT_MODIFIER_SHIFT;
    constexpr auto alt = INKPOD_SHORTCUT_MODIFIER_ALT;
    switch (command) {
        case IDM_FILE_NEW: sequence = Sequence(command, {Stroke(L'N', control)}); return true;
        case IDM_FILE_OPEN: sequence = Sequence(command, {Stroke(L'O', control)}); return true;
        case IDM_FILE_SAVE: sequence = Sequence(command, {Stroke(L'S', control)}); return true;
        case IDM_FILE_SAVE_AS:
            sequence = Sequence(command, {Stroke(L'S', control | shift)});
            return true;
        case IDM_APP_EXIT: sequence = Sequence(command, {Stroke(VK_F4, alt)}); return true;
        case IDM_EDIT_UNDO: sequence = Sequence(command, {Stroke(L'Z', control)}); return true;
        case IDM_EDIT_REDO: sequence = Sequence(command, {Stroke(L'Y', control)}); return true;
        case IDM_EDIT_CUT: sequence = Sequence(command, {Stroke(L'X', control)}); return true;
        case IDM_EDIT_COPY: sequence = Sequence(command, {Stroke(L'C', control)}); return true;
        case IDM_EDIT_PASTE: sequence = Sequence(command, {Stroke(L'V', control)}); return true;
        case IDM_SELECTION_ALL: sequence = Sequence(command, {Stroke(L'A', control)}); return true;
        case IDM_TOOL_PENCIL: sequence = Sequence(command, {Stroke(L'P')}); return true;
        case IDM_TOOL_BRUSH: sequence = Sequence(command, {Stroke(L'B')}); return true;
        case IDM_TOOL_ERASER: sequence = Sequence(command, {Stroke(L'E')}); return true;
        case IDM_TOOL_FILL: sequence = Sequence(command, {Stroke(L'F')}); return true;
        case IDM_TOOL_CLOSED_FILL:
            sequence = Sequence(command, {Stroke(L'F', shift)});
            return true;
        case IDM_TOOL_FILL_EXTENSION: sequence = Sequence(command, {Stroke(L'X')}); return true;
        case IDM_TOOL_EYEDROPPER: sequence = Sequence(command, {Stroke(L'I')}); return true;
        case IDM_SELECTION_RECTANGLE: sequence = Sequence(command, {Stroke(L'R')}); return true;
        case IDM_SELECTION_ELLIPSE: sequence = Sequence(command, {Stroke(L'O')}); return true;
        case IDM_SELECTION_LASSO: sequence = Sequence(command, {Stroke(L'L')}); return true;
        case IDM_SELECTION_WAND: sequence = Sequence(command, {Stroke(L'W')}); return true;
        case IDM_EFFECT_GRADIENT: sequence = Sequence(command, {Stroke(L'G')}); return true;
        case IDM_EFFECT_AIRBRUSH: sequence = Sequence(command, {Stroke(L'A')}); return true;
        case IDM_PALETTE_NEXT_GROUP: sequence = Sequence(command, {Stroke(VK_TAB)}); return true;
        case IDM_MOTION_FPS_30:
            sequence = Sequence(command, {Stroke(L'3', control | alt)});
            return true;
        case IDM_MOTION_FPS_25:
            sequence = Sequence(command, {Stroke(L'2', control | alt)});
            return true;
        case IDM_MOTION_FPS_24:
            sequence = Sequence(command, {Stroke(L'4', control | alt)});
            return true;
        case IDM_MOTION_FPS_12:
            sequence = Sequence(command, {Stroke(L'1', control | alt)});
            return true;
        case IDM_MOTION_FPS_10:
            sequence = Sequence(command, {Stroke(L'0', control | alt)});
            return true;
        case IDM_MOTION_FPS_8:
            sequence = Sequence(command, {Stroke(L'8', control | alt)});
            return true;
        case IDM_WINDOW_BATCH:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'B')});
            return true;
        case IDM_WINDOW_TOOL_PALETTE:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'T')});
            return true;
        case IDM_WINDOW_LAYER_PALETTE:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'L')});
            return true;
        case IDM_WINDOW_TOOL_OPTIONS:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'O')});
            return true;
        case IDM_WINDOW_COLOR_PANE:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'C')});
            return true;
        case IDM_WORKSPACE_RESET:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'R')});
            return true;
        case IDM_WORKSPACE_SAVE:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'S')});
            return true;
        case IDM_WORKSPACE_RESTORE:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'U')});
            return true;
        case IDM_WORKSPACE_MIRROR:
            sequence = Sequence(command, {Stroke(L'Q'), Stroke(L'N'), Stroke(L'M')});
            return true;
        default: return false;
    }
}

std::wstring StripShortcutSuffix(std::wstring text) {
    if (const std::size_t separator = text.find(L'\t'); separator != std::wstring::npos) {
        text.resize(separator);
    }
    return text;
}

std::wstring StripMnemonic(std::wstring text) {
    text.erase(std::remove(text.begin(), text.end(), L'&'), text.end());
    return text;
}

bool FindMenuText(HMENU menu, UINT command, std::wstring& output) {
    const int count = GetMenuItemCount(menu);
    for (int position = 0; position < count; ++position) {
        MENUITEMINFOW item{};
        item.cbSize = sizeof(item);
        item.fMask = MIIM_ID | MIIM_FTYPE | MIIM_SUBMENU;
        if (GetMenuItemInfoW(menu, static_cast<UINT>(position), TRUE, &item) == FALSE) {
            continue;
        }
        if (item.hSubMenu != nullptr && FindMenuText(item.hSubMenu, command, output)) {
            return true;
        }
        if ((item.fType & MFT_SEPARATOR) == 0U && item.wID == command) {
            const int length = GetMenuStringW(
                menu, static_cast<UINT>(position), nullptr, 0, MF_BYPOSITION);
            if (length < 0) {
                return false;
            }
            std::wstring text(static_cast<std::size_t>(length) + 1U, L'\0');
            GetMenuStringW(
                menu,
                static_cast<UINT>(position),
                text.data(),
                static_cast<int>(text.size()),
                MF_BYPOSITION);
            text.resize(std::wcslen(text.c_str()));
            output = StripMnemonic(StripShortcutSuffix(std::move(text)));
            return true;
        }
    }
    return false;
}

void ApplyShortcutLabels(
    HMENU menu,
    std::span<const InkpodShortcutSequence> bindings) noexcept {
    const int count = GetMenuItemCount(menu);
    for (int position = 0; position < count; ++position) {
        MENUITEMINFOW item{};
        item.cbSize = sizeof(item);
        item.fMask = MIIM_ID | MIIM_FTYPE | MIIM_SUBMENU;
        if (GetMenuItemInfoW(menu, static_cast<UINT>(position), TRUE, &item) == FALSE) {
            continue;
        }
        if (item.hSubMenu != nullptr) {
            ApplyShortcutLabels(item.hSubMenu, bindings);
        }
        if ((item.fType & MFT_SEPARATOR) != 0U || item.wID == 0U
            || item.wID == std::numeric_limits<UINT>::max()) {
            continue;
        }
        const InkpodShortcutSequence* sequence = FindShortcutSequence(bindings, item.wID);
        if (sequence == nullptr) {
            continue;
        }
        const int length = GetMenuStringW(
            menu, static_cast<UINT>(position), nullptr, 0, MF_BYPOSITION);
        if (length < 0) {
            continue;
        }
        try {
            std::wstring text(static_cast<std::size_t>(length) + 1U, L'\0');
            GetMenuStringW(
                menu,
                static_cast<UINT>(position),
                text.data(),
                static_cast<int>(text.size()),
                MF_BYPOSITION);
            text.resize(std::wcslen(text.c_str()));
            text = StripShortcutSuffix(std::move(text));
            text += L'\t';
            text += FormatShortcutSequence(*sequence);
            MENUITEMINFOW update{};
            update.cbSize = sizeof(update);
            update.fMask = MIIM_STRING;
            update.dwTypeData = text.data();
            SetMenuItemInfoW(menu, static_cast<UINT>(position), TRUE, &update);
        } catch (const std::bad_alloc&) {
            return;
        }
    }
}

std::wstring KeyName(const InkpodShortcutStroke& stroke) {
    switch (stroke.virtual_key) {
        case VK_TAB: return L"Tab";
        case VK_SPACE: return L"Space";
        case VK_RETURN: return L"Enter";
        case VK_ESCAPE: return L"Esc";
        case VK_LEFT: return L"Left";
        case VK_RIGHT: return L"Right";
        case VK_HOME: return L"Home";
        case VK_END: return L"End";
        case VK_F4: return L"F4";
        default: break;
    }
    if ((stroke.virtual_key >= L'A' && stroke.virtual_key <= L'Z')
        || (stroke.virtual_key >= L'0' && stroke.virtual_key <= L'9')) {
        return std::wstring(1U, static_cast<wchar_t>(stroke.virtual_key));
    }
    const UINT scan = MapVirtualKeyW(stroke.virtual_key, MAPVK_VK_TO_VSC);
    wchar_t buffer[64]{};
    const LONG key_data = static_cast<LONG>(scan << 16)
        | ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_EXTENDED) != 0U ? (1L << 24) : 0L);
    if (GetKeyNameTextW(key_data, buffer, static_cast<int>(std::size(buffer))) > 0) {
        return buffer;
    }
    return L"?";
}

}  // namespace

std::span<const UINT> MenuCommandCatalog() noexcept {
    return kCommandIds;
}

std::vector<InkpodShortcutSequence> BuildDefaultShortcutSequences() {
    std::vector<InkpodShortcutSequence> result;
    result.reserve(std::size(kCommandIds));
    std::map<UINT, UINT> group_ordinals;
    for (const UINT command : kCommandIds) {
        InkpodShortcutSequence direct{};
        if (DirectSequence(command, direct)) {
            result.push_back(direct);
            continue;
        }
        const UINT group = command / 100U;
        const UINT ordinal = group_ordinals[group]++;
        if (group == 419U || group == 420U) {
            result.push_back(Sequence(
                command,
                {Stroke(L'Q'),
                 Stroke(L'B'),
                 Stroke(group == 419U ? L'O' : L'A'),
                 Stroke(L'A' + ordinal)}));
            continue;
        }
        const wchar_t group_key = GroupKey(group);
        result.push_back(Sequence(
            command,
            {Stroke(L'Q'), Stroke(static_cast<UINT>(group_key)), Stroke(L'A' + ordinal)}));
    }
    return result;
}

const InkpodShortcutSequence* FindShortcutSequence(
    std::span<const InkpodShortcutSequence> bindings,
    UINT command) noexcept {
    const auto found = std::find_if(bindings.begin(), bindings.end(), [command](const auto& binding) {
        return binding.command_id == command;
    });
    return found == bindings.end() ? nullptr : &*found;
}

std::wstring FormatShortcutSequence(const InkpodShortcutSequence& sequence) {
    std::wstring output;
    const std::uint32_t count = std::min(
        sequence.stroke_count,
        static_cast<std::uint32_t>(INKPOD_SHORTCUT_MAX_STROKES));
    for (std::uint32_t index = 0; index < count; ++index) {
        if (index != 0U) {
            output += L", ";
        }
        const auto& stroke = sequence.strokes[index];
        if ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_CONTROL) != 0U) {
            output += L"Ctrl+";
        }
        if ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_SHIFT) != 0U) {
            output += L"Shift+";
        }
        if ((stroke.modifiers & INKPOD_SHORTCUT_MODIFIER_ALT) != 0U) {
            output += L"Alt+";
        }
        output += KeyName(stroke);
    }
    return output;
}

std::wstring MenuCommandDisplayName(HMENU menu, UINT command) {
    std::wstring label;
    if (!FindMenuText(menu, command, label)) {
        label = L"Command " + std::to_wstring(command);
    }
    return label + L" [" + std::wstring(GroupName(command)) + L"]";
}

void ApplyShortcutLabelsToMenu(
    HMENU menu,
    std::span<const InkpodShortcutSequence> bindings) noexcept {
    if (menu != nullptr) {
        ApplyShortcutLabels(menu, bindings);
    }
}

}  // namespace inkpod::windows::ui
