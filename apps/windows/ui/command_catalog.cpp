#include "ui/localization.h"

#include "command_catalog.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cwchar>
#include <initializer_list>
#include <limits>
#include <new>
#include <string>
#include <string_view>
#include <utility>

#include "app/resource.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT kCommandIds[] = {
#define INKPOD_COMMAND_STATE(owner, command) command,
#include "command_state_catalog.inc"
#undef INKPOD_COMMAND_STATE
};

constexpr const char* kCommandNames[] = {
#define INKPOD_COMMAND_STATE(owner, command) #command,
#include "command_state_catalog.inc"
#undef INKPOD_COMMAND_STATE
};

static_assert(std::size(kCommandIds) == std::size(kCommandNames));

constexpr bool IsPaneLocalCommand(UINT command) noexcept {
    switch (command) {
        case IDM_LOCATOR_PIN:
        case IDM_LOCATOR_FIXED:
        case IDM_LOCATOR_AUTOSCROLL:
        case IDM_SEQUENCE_PIN:
        case IDM_LIGHT_TABLE_PIN:
        case IDM_SUBPALETTE_PIN:
        case IDM_COLOR_PIN:
        case IDM_BATCH_PIN:
            return true;
        default:
            return false;
    }
}

consteval auto BuildMenuCommandIds() {
    std::array<UINT, std::size(kCommandIds) - 8U> result{};
    std::size_t index{};
    for (const UINT command : kCommandIds) {
        if (!IsPaneLocalCommand(command)) {
            result[index++] = command;
        }
    }
    return result;
}

constexpr auto kMenuCommandIds = BuildMenuCommandIds();
static_assert(kMenuCommandIds.size() == 316U);

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

const wchar_t* GroupName(UINT command) noexcept {
    switch (command / 100U) {
        case 400: return UiText(UiStringId::Text0279);
        case 401: return UiText(UiStringId::Editable);
        case 402: return UiText(UiStringId::Visible);
        case 403: return UiText(UiStringId::ToolGeneric);
        case 404: return UiText(UiStringId::Plane);
        case 405: return UiText(UiStringId::Text0865);
        case 406: return UiText(UiStringId::Text0332);
        case 407: return UiText(UiStringId::Layer);
        case 408: return UiText(UiStringId::ToolSelection);
        case 409: return UiText(UiStringId::Text0916);
        case 410: return UiText(UiStringId::Text0285);
        case 411: return UiText(UiStringId::Text0781);
        case 413: return UiText(UiStringId::Text0219);
        case 414: return UiText(UiStringId::Layer);
        case 415: return UiText(UiStringId::Plane);
        case 416: return UiText(UiStringId::LightTable);
        case 417: return UiText(UiStringId::Text0967);
        case 418: return UiText(UiStringId::ToolGeometry);
        case 419:
            return command == IDM_WINDOW_TOOL_PALETTE
                    || command == IDM_WINDOW_LAYER_PALETTE
                    || command == IDM_WINDOW_TOOL_OPTIONS
                    || command == IDM_WINDOW_COLOR_PANE
                    || command == IDM_COLOR_PIN
                    || command == IDM_WORKSPACE_RESET
                    || command == IDM_WORKSPACE_SAVE
                    || command == IDM_WORKSPACE_SAVE_AS
                    || command == IDM_WORKSPACE_RESTORE
                    || command == IDM_WORKSPACE_MIRROR
                    || (command >= IDM_WORKSPACE_PRESET_COLORING
                        && command <= IDM_VIEW_DUPLICATE_NEW_WINDOW)
                    || command == IDM_WINDOW_LOCATOR
                    || command == IDM_LOCATOR_PIN
                    || command == IDM_LOCATOR_FIXED
                    || command == IDM_LOCATOR_AUTOSCROLL
                    || command == IDM_WINDOW_SEQUENCE
                    || command == IDM_SEQUENCE_PIN
                    || command == IDM_WINDOW_LIGHT_TABLE
                    || command == IDM_LIGHT_TABLE_PIN
                    || command == IDM_WINDOW_SUBPALETTE
                    || command == IDM_SUBPALETTE_PIN
                    || command == IDM_BATCH_PIN
                    || command == IDM_WINDOW_BATCH
                ? UiText(UiStringId::Text0133)
                : UiText(UiStringId::Text0255);
        case 420: return UiText(UiStringId::Text0269);
        default: return UiText(UiStringId::Text0111);
    }
}

bool DirectSequence(UINT command, InkpodShortcutSequence& sequence) noexcept {
    constexpr auto control = INKPOD_SHORTCUT_MODIFIER_CONTROL;
    constexpr auto shift = INKPOD_SHORTCUT_MODIFIER_SHIFT;
    constexpr auto alt = INKPOD_SHORTCUT_MODIFIER_ALT;
    constexpr auto extended = INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    switch (command) {
        case IDM_FILE_NEW: sequence = Sequence(command, {Stroke(L'N', control)}); return true;
        case IDM_FILE_OPEN: sequence = Sequence(command, {Stroke(L'O', control)}); return true;
        case IDM_FILE_SAVE: sequence = Sequence(command, {Stroke(L'S', control)}); return true;
        case IDM_FILE_SAVE_AS:
            sequence = Sequence(command, {Stroke(L'S', control | shift)});
            return true;
        case IDM_APP_EXIT: sequence = Sequence(command, {Stroke(VK_F4, alt)}); return true;
        case IDM_HELP_MANUAL: sequence = Sequence(command, {Stroke(VK_F1)}); return true;
        case IDM_VIEW_CLOSE:
            sequence = Sequence(command, {Stroke(L'W', control)});
            return true;
        case IDM_TAB_NEXT:
            sequence = Sequence(command, {Stroke(VK_NEXT, control | extended)});
            return true;
        case IDM_TAB_PREVIOUS:
            sequence = Sequence(command, {Stroke(VK_PRIOR, control | extended)});
            return true;
        case IDM_TAB_MOVE_LEFT:
            sequence = Sequence(
                command, {Stroke(VK_PRIOR, control | shift | extended)});
            return true;
        case IDM_TAB_MOVE_RIGHT:
            sequence = Sequence(
                command, {Stroke(VK_NEXT, control | shift | extended)});
            return true;
        case IDM_EDITOR_SPLIT_RIGHT:
            sequence = Sequence(command, {Stroke(VK_OEM_5, control)});
            return true;
        case IDM_EDITOR_MOVE_OTHER_GROUP:
            sequence = Sequence(
                command, {Stroke(VK_RIGHT, control | alt | extended)});
            return true;
        case IDM_EDITOR_GROUP_CLOSE:
            sequence = Sequence(command, {Stroke(L'K', control), Stroke(L'W')});
            return true;
        case IDM_EDITOR_GROUP_NEXT:
            sequence = Sequence(
                command,
                {Stroke(L'K', control), Stroke(VK_RIGHT, control | extended)});
            return true;
        case IDM_EDITOR_GROUP_FIRST:
            sequence = Sequence(command, {Stroke(L'1', control)});
            return true;
        case IDM_EDITOR_GROUP_SECOND:
            sequence = Sequence(command, {Stroke(L'2', control)});
            return true;
        case IDM_WORKSPACE_NEW_WINDOW:
            sequence = Sequence(command, {Stroke(L'N', control | shift)});
            return true;
        case IDM_VIEW_DUPLICATE_NEW_WINDOW:
            sequence = Sequence(command, {Stroke(L'K', control), Stroke(L'O')});
            return true;
        case IDM_EDIT_UNDO: sequence = Sequence(command, {Stroke(L'Z', control)}); return true;
        case IDM_EDIT_REDO: sequence = Sequence(command, {Stroke(L'Y', control)}); return true;
        case IDM_EDIT_CUT: sequence = Sequence(command, {Stroke(L'X', control)}); return true;
        case IDM_EDIT_COPY: sequence = Sequence(command, {Stroke(L'C', control)}); return true;
        case IDM_EDIT_PASTE: sequence = Sequence(command, {Stroke(L'V', control)}); return true;
        case IDM_SELECTION_ALL: sequence = Sequence(command, {Stroke(L'A', control)}); return true;
        case IDM_SHORTCUT_EDIT:
            sequence = Sequence(command, {Stroke(VK_OEM_COMMA, control)});
            return true;
        case IDM_SHORTCUT_KEYBOARD:
            sequence = Sequence(command, {Stroke(L'K', control), Stroke(L'S', control)});
            return true;
        case IDM_VIEW_ZOOM_IN:
            sequence = Sequence(command, {Stroke(VK_OEM_PLUS, control)});
            return true;
        case IDM_VIEW_ZOOM_OUT:
            sequence = Sequence(command, {Stroke(VK_OEM_MINUS, control)});
            return true;
        default: return false;
    }
}

void AppendProfileBinding(
    ShortcutProfile& profile,
    ShortcutSlot slot,
    const InkpodShortcutSequence& sequence) {
    ShortcutProfileBinding binding{};
    binding.command_id = sequence.command_id;
    binding.slot = slot;
    binding.context = DefaultShortcutContext(sequence.command_id);
    binding.action = DefaultShortcutAction(sequence.command_id);
    binding.key_match = ShortcutKeyMatch::Logical;
    binding.stroke_count = sequence.stroke_count;
    for (std::uint32_t index = 0U; index < sequence.stroke_count; ++index) {
        const auto& source = sequence.strokes[index];
        binding.strokes[index] = {
            source.virtual_key,
            ShortcutPhysicalKeyFromVirtualKey(
                source.virtual_key, source.modifiers),
            source.modifiers};
    }
    profile.bindings.push_back(binding);
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
            const InkpodShortcutSequence* sequence = FindShortcutSequence(
                bindings, item.wID);
            if (sequence != nullptr) {
                text += L'\t';
                text += FormatShortcutSequence(*sequence);
            }
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

std::string CommandStableKey(UINT command) {
    const auto found = std::find(std::begin(kCommandIds), std::end(kCommandIds), command);
    if (found == std::end(kCommandIds)) {
        return {};
    }
    const std::size_t index = static_cast<std::size_t>(found - std::begin(kCommandIds));
    std::string key(kCommandNames[index]);
    if (key.starts_with("IDM_")) {
        key.erase(0U, 4U);
    }
    std::transform(key.begin(), key.end(), key.begin(), [](char value) {
        if (value == '_') {
            return '.';
        }
        return value >= 'A' && value <= 'Z'
            ? static_cast<char>(value - 'A' + 'a')
            : value;
    });
    return key;
}

UINT CommandFromStableKey(std::string_view key) noexcept {
    for (std::size_t index = 0U; index < std::size(kCommandIds); ++index) {
        const char* symbol = kCommandNames[index];
        if (std::char_traits<char>::compare(symbol, "IDM_", 4U) != 0) {
            continue;
        }
        std::size_t key_index{};
        bool match = true;
        for (std::size_t symbol_index = 4U; symbol[symbol_index] != '\0'; ++symbol_index) {
            const char raw = symbol[symbol_index];
            const char normalized = raw == '_'
                ? '.'
                : (raw >= 'A' && raw <= 'Z' ? static_cast<char>(raw - 'A' + 'a') : raw);
            if (key_index >= key.size() || key[key_index++] != normalized) {
                match = false;
                break;
            }
        }
        if (match && key_index == key.size()) {
            return kCommandIds[index];
        }
    }
    return 0U;
}

ShortcutContext DefaultShortcutContext(UINT command) noexcept {
    const UINT group = command / 100U;
    if (group == 403U || group == 404U || group == 405U || group == 408U
        || group == 410U || group == 411U || group == 412U || group == 418U) {
        return ShortcutContext::Canvas;
    }
    if (group == 416U || group == 417U) {
        return ShortcutContext::Timeline;
    }
    if (IsPaneLocalCommand(command)) {
        return ShortcutContext::Pane;
    }
    return ShortcutContext::Global;
}

std::uint32_t SupportedShortcutActionMask(UINT command) noexcept {
    const auto action_bit = [](ShortcutAction action) constexpr {
        return 1U << (static_cast<std::uint32_t>(action) - 1U);
    };
    std::uint32_t result = action_bit(ShortcutAction::Execute);
    switch (command) {
        case IDM_TOOL_PENCIL:
        case IDM_TOOL_BRUSH:
        case IDM_TOOL_ERASER:
        case IDM_TOOL_FILL:
        case IDM_TOOL_CLOSED_FILL:
        case IDM_TOOL_FILL_EXTENSION:
        case IDM_TOOL_EYEDROPPER:
        case IDM_SELECTION_RECTANGLE:
        case IDM_SELECTION_ELLIPSE:
        case IDM_SELECTION_LASSO:
            result |= action_bit(ShortcutAction::Hold);
            break;
        default:
            break;
    }
    switch (command) {
        case IDM_VIEW_FLIP_HORIZONTAL:
        case IDM_VIEW_FLIP_VERTICAL:
        case IDM_VIEW_RULER:
        case IDM_VIEW_GUIDES:
        case IDM_VIEW_GRID:
        case IDM_VIEW_SNAP_GUIDES:
        case IDM_VIEW_SNAP_GRID:
        case IDM_VIEW_TRANSPARENT:
        case IDM_WINDOW_TOOL_PALETTE:
        case IDM_WINDOW_TOOL_OPTIONS:
        case IDM_WINDOW_COLOR_PANE:
        case IDM_WINDOW_LAYER_PALETTE:
        case IDM_WINDOW_LOCATOR:
        case IDM_WINDOW_SEQUENCE:
        case IDM_WINDOW_LIGHT_TABLE:
        case IDM_WINDOW_SUBPALETTE:
        case IDM_WINDOW_BATCH:
        case IDM_SEQ_WRAP_ENDPOINTS:
            result |= action_bit(ShortcutAction::Toggle);
            break;
        default:
            break;
    }
    return result;
}

ShortcutAction DefaultShortcutAction(UINT command) noexcept {
    constexpr std::uint32_t toggle_bit =
        1U << (static_cast<std::uint32_t>(ShortcutAction::Toggle) - 1U);
    return (SupportedShortcutActionMask(command) & toggle_bit) != 0U
        ? ShortcutAction::Toggle
        : ShortcutAction::Execute;
}

std::span<const UINT> MenuCommandCatalog() noexcept {
    return kMenuCommandIds;
}

bool IsMenuCommand(UINT command) noexcept {
    return std::find(kMenuCommandIds.begin(), kMenuCommandIds.end(), command)
        != kMenuCommandIds.end();
}

std::span<const UINT> ShortcutCommandCatalog() noexcept {
    return kCommandIds;
}

std::vector<InkpodShortcutSequence> BuildDefaultShortcutSequences() {
    std::vector<InkpodShortcutSequence> result;
    result.reserve(32U);
    for (const UINT command : kCommandIds) {
        InkpodShortcutSequence direct{};
        if (DirectSequence(command, direct)) {
            result.push_back(direct);
        }
    }
    return result;
}

ShortcutProfile BuildDefaultShortcutProfile(std::wstring name) {
    ShortcutProfile profile{std::move(name), true, {}};
    const std::vector<InkpodShortcutSequence> primary = BuildDefaultShortcutSequences();
    profile.bindings.reserve(primary.size() + 4U);
    for (const InkpodShortcutSequence& sequence : primary) {
        AppendProfileBinding(profile, ShortcutSlot::Primary, sequence);
    }

    constexpr auto control = INKPOD_SHORTCUT_MODIFIER_CONTROL;
    constexpr auto shift = INKPOD_SHORTCUT_MODIFIER_SHIFT;
    for (const InkpodShortcutSequence& sequence : {
             Sequence(IDM_EDIT_REDO, {Stroke(L'Z', control | shift)}),
             Sequence(IDM_TAB_NEXT, {Stroke(VK_TAB, control)}),
             Sequence(IDM_TAB_PREVIOUS, {Stroke(VK_TAB, control | shift)}),
             Sequence(IDM_VIEW_CLOSE, {Stroke(VK_F4, control)})}) {
        AppendProfileBinding(profile, ShortcutSlot::Secondary, sequence);
    }
    return profile;
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
        if ((stroke.modifiers & kShortcutModifierWindows) != 0U) {
            output += L"Win+";
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
