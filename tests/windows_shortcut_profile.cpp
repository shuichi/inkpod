#include "ui/shortcut_profile.h"
#include "ui/shortcut_preset.h"
#include "ui/command_catalog.h"
#include "app/resource.h"

#include <algorithm>
#include <array>
#include <cstdio>
#include <span>
#include <string>
#include <string_view>
#include <vector>

namespace {

using inkpod::windows::ui::AnalyzeShortcutConflicts;
using inkpod::windows::ui::ApplyShortcutLabelsToMenu;
using inkpod::windows::ui::BuildDefaultShortcutProfile;
using inkpod::windows::ui::FindShortcutBinding;
using inkpod::windows::ui::ResolveShortcutProfile;
using inkpod::windows::ui::ShortcutAction;
using inkpod::windows::ui::ShortcutConflictKind;
using inkpod::windows::ui::ShortcutContext;
using inkpod::windows::ui::ShortcutInputStroke;
using inkpod::windows::ui::ShortcutKeyMatch;
using inkpod::windows::ui::ShortcutProfile;
using inkpod::windows::ui::ShortcutProfileBinding;
using inkpod::windows::ui::ShortcutProfileValidation;
using inkpod::windows::ui::ShortcutPhysicalKeyFromMessage;
using inkpod::windows::ui::ShortcutPhysicalKeyFromVirtualKey;
using inkpod::windows::ui::ShortcutPresetStatus;
using inkpod::windows::ui::ShortcutSlot;
using inkpod::windows::ui::ShortcutStrokeReservedForNativeMenu;
using inkpod::windows::ui::ValidateShortcutProfile;

ShortcutProfileBinding Binding(
    std::uint32_t command,
    ShortcutSlot slot,
    ShortcutContext context,
    std::initializer_list<std::uint32_t> logical_keys,
    ShortcutKeyMatch match = ShortcutKeyMatch::Logical) {
    ShortcutProfileBinding binding{};
    binding.command_id = command;
    binding.slot = slot;
    binding.context = context;
    binding.key_match = match;
    for (const std::uint32_t key : logical_keys) {
        binding.strokes[binding.stroke_count++] = ShortcutInputStroke{
            key, ShortcutPhysicalKeyFromVirtualKey(key, 0U), 0U};
    }
    return binding;
}

bool BindingMatches(
    const ShortcutProfile& profile,
    std::uint32_t command,
    ShortcutSlot slot,
    std::initializer_list<InkpodShortcutStroke> expected) {
    const ShortcutProfileBinding* binding = FindShortcutBinding(
        std::span<const ShortcutProfileBinding>(profile.bindings), command, slot);
    if (binding == nullptr
        || binding->stroke_count != static_cast<std::uint32_t>(expected.size())
        || binding->key_match != ShortcutKeyMatch::Logical) {
        return false;
    }
    std::size_t index{};
    for (const InkpodShortcutStroke& stroke : expected) {
        if (binding->strokes[index].logical_key != stroke.virtual_key
            || binding->strokes[index].modifiers != stroke.modifiers) {
            return false;
        }
        ++index;
    }
    return true;
}

bool SparseBuiltInProfileIsAuthoritative(const ShortcutProfile& profile) {
    const auto commands = inkpod::windows::ui::ShortcutCommandCatalog();
    if (!profile.built_in || commands.size() != 312U
        || profile.bindings.size() != 33U
        || ValidateShortcutProfile(profile, false)
            != ShortcutProfileValidation::Ok) {
        return false;
    }
    for (const ShortcutProfileBinding& binding : profile.bindings) {
        if (std::find(commands.begin(), commands.end(), binding.command_id)
                == commands.end()) {
            return false;
        }
        for (std::uint32_t index = 0U; index < binding.stroke_count; ++index) {
            if (binding.strokes[index].logical_key
                == static_cast<std::uint32_t>('Q')) {
                return false;
            }
        }
    }
    const ShortcutProfileBinding* next = FindShortcutBinding(
        std::span<const ShortcutProfileBinding>(profile.bindings),
        IDM_TAB_NEXT,
        ShortcutSlot::Primary);
    constexpr auto control = INKPOD_SHORTCUT_MODIFIER_CONTROL;
    constexpr auto shift = INKPOD_SHORTCUT_MODIFIER_SHIFT;
    constexpr auto extended = INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    const std::uint32_t page_down_physical = ShortcutPhysicalKeyFromVirtualKey(
        VK_NEXT, control | extended);
    const UINT page_down_scan = MapVirtualKeyW(VK_NEXT, MAPVK_VK_TO_VSC_EX);
    const LPARAM page_down_key_data = static_cast<LPARAM>(
        ((page_down_scan & 0xffU) << 16U) | (1U << 24U));
    return next != nullptr && next->stroke_count == 1U
        && next->strokes[0].physical_key == page_down_physical
        && (page_down_physical & UINT32_C(0x100)) != 0U
        && page_down_physical
            == ShortcutPhysicalKeyFromMessage(VK_NEXT, page_down_key_data)
        && BindingMatches(
               profile,
               IDM_EDIT_REDO,
               ShortcutSlot::Primary,
               {{'Y', control}})
        && BindingMatches(
            profile,
            IDM_EDIT_REDO,
            ShortcutSlot::Secondary,
            {{'Z', control | shift}})
        && BindingMatches(
            profile,
            IDM_TAB_NEXT,
            ShortcutSlot::Primary,
            {{VK_NEXT, control | extended}})
        && BindingMatches(
            profile,
            IDM_TAB_NEXT,
            ShortcutSlot::Secondary,
            {{VK_TAB, control}})
        && BindingMatches(
            profile,
            IDM_TAB_PREVIOUS,
            ShortcutSlot::Primary,
            {{VK_PRIOR, control | extended}})
        && BindingMatches(
            profile,
            IDM_TAB_PREVIOUS,
            ShortcutSlot::Secondary,
            {{VK_TAB, control | shift}})
        && BindingMatches(
            profile,
            IDM_VIEW_CLOSE,
            ShortcutSlot::Primary,
            {{'W', control}})
        && BindingMatches(
            profile,
            IDM_VIEW_CLOSE,
            ShortcutSlot::Secondary,
            {{VK_F4, control}})
        && FindShortcutBinding(
               std::span<const ShortcutProfileBinding>(profile.bindings),
               IDM_TOOL_PENCIL,
               ShortcutSlot::Primary)
            == nullptr
        && FindShortcutBinding(
               std::span<const ShortcutProfileBinding>(profile.bindings),
               IDM_WINDOW_BATCH,
               ShortcutSlot::Primary)
            == nullptr
        && FindShortcutBinding(
               std::span<const ShortcutProfileBinding>(profile.bindings),
               IDM_PALETTE_NEXT_GROUP,
               ShortcutSlot::Primary)
            == nullptr;
}

bool UnassignedMenuLabelDropsStaleSuffix() {
    HMENU menu = CreateMenu();
    if (menu == nullptr
        || !AppendMenuW(menu, MF_STRING, IDM_FILE_SAVE, L"&Save\tOld shortcut")) {
        if (menu != nullptr) {
            DestroyMenu(menu);
        }
        return false;
    }
    ApplyShortcutLabelsToMenu(
        menu, std::span<const InkpodShortcutSequence>{});
    std::array<wchar_t, 128U> text{};
    const int length = GetMenuStringW(
        menu,
        IDM_FILE_SAVE,
        text.data(),
        static_cast<int>(text.size()),
        MF_BYCOMMAND);
    const bool result = length == 5 && std::wstring_view(text.data()) == L"&Save";
    DestroyMenu(menu);
    return result;
}

bool NativeMenuReservationsMatchRuntimePolicy() {
    const auto stroke = [](std::uint32_t key, std::uint32_t modifiers) {
        return ShortcutInputStroke{
            key,
            ShortcutPhysicalKeyFromVirtualKey(key, modifiers),
            modifiers};
    };
    constexpr auto control = INKPOD_SHORTCUT_MODIFIER_CONTROL;
    constexpr auto shift = INKPOD_SHORTCUT_MODIFIER_SHIFT;
    constexpr auto alt = INKPOD_SHORTCUT_MODIFIER_ALT;
    if (!ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_MENU, 0U))
        || !ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_LMENU, alt))
        || !ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_RMENU, alt))
        || !ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_F10, 0U))
        || ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_F10, shift))
        || !ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_SPACE, alt))
        || !ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_SPACE, alt | shift))
        || ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_SPACE, alt | control))
        || !ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_F4, alt))
        || ShortcutStrokeReservedForNativeMenu(
            IDM_APP_EXIT, stroke(VK_F4, alt))
        || ShortcutStrokeReservedForNativeMenu(
            IDM_FILE_SAVE, stroke(VK_F4, alt | shift))) {
        return false;
    }
    constexpr std::wstring_view top_level_mnemonics = L"FEVLSITCPWH";
    for (const wchar_t key : top_level_mnemonics) {
        if (!ShortcutStrokeReservedForNativeMenu(
                IDM_FILE_SAVE, stroke(key, alt))
            || !ShortcutStrokeReservedForNativeMenu(
                IDM_FILE_SAVE, stroke(key, alt | shift))
            || ShortcutStrokeReservedForNativeMenu(
                IDM_FILE_SAVE, stroke(key, alt | control))
            || ShortcutStrokeReservedForNativeMenu(
                IDM_FILE_SAVE, stroke(key, 0U))) {
            return false;
        }
    }
    return true;
}

}  // namespace

int main() {
    const ShortcutProfile defaults = BuildDefaultShortcutProfile(L"Built-in");
    if (!SparseBuiltInProfileIsAuthoritative(defaults)) {
        std::fprintf(stderr, "sparse built-in shortcut profile is invalid\n");
        return 12;
    }
    if (!UnassignedMenuLabelDropsStaleSuffix()) {
        std::fprintf(stderr, "unassigned menu item retained a stale shortcut suffix\n");
        return 14;
    }
    if (!NativeMenuReservationsMatchRuntimePolicy()) {
        std::fprintf(stderr, "native menu shortcut reservation mismatch\n");
        return 15;
    }

    ShortcutProfile profile{L"Custom", false, {}};
    profile.bindings = {
        Binding(IDM_TOOL_BRUSH, ShortcutSlot::Primary, ShortcutContext::Canvas, {'B'}),
        Binding(IDM_TOOL_BRUSH, ShortcutSlot::Secondary, ShortcutContext::Canvas, {'5'}),
        Binding(IDM_SEQ_NEXT, ShortcutSlot::Primary, ShortcutContext::Timeline, {'B'}),
        Binding(IDM_VIEW_GRID, ShortcutSlot::Primary, ShortcutContext::Global, {'W'}),
        Binding(IDM_WINDOW_TOOL_PALETTE, ShortcutSlot::Primary, ShortcutContext::Pane, {'Q', 'F'})};
    ShortcutProfileBinding preserved_reserved = Binding(
        IDM_HELP_WEB_PAGE,
        ShortcutSlot::Primary,
        ShortcutContext::Global,
        {'F'});
    preserved_reserved.strokes[0].modifiers = INKPOD_SHORTCUT_MODIFIER_ALT;
    profile.bindings.push_back(preserved_reserved);

    if (ValidateShortcutProfile(profile, false) != ShortcutProfileValidation::Ok
        || FindShortcutBinding(
               std::span<const ShortcutProfileBinding>(profile.bindings),
               IDM_TOOL_BRUSH,
               ShortcutSlot::Secondary)
            == nullptr) {
        return 1;
    }

    const ShortcutInputStroke canvas_b{
        'B', ShortcutPhysicalKeyFromVirtualKey('B', 0U), 0U};
    const auto canvas = ResolveShortcutProfile(
        profile.bindings, ShortcutContext::Canvas, std::span(&canvas_b, 1U));
    const auto timeline = ResolveShortcutProfile(
        profile.bindings, ShortcutContext::Timeline, std::span(&canvas_b, 1U));
    if (canvas.match != INKPOD_SHORTCUT_MATCH_EXACT
        || canvas.command_id != IDM_TOOL_BRUSH
        || timeline.match != INKPOD_SHORTCUT_MATCH_EXACT
        || timeline.command_id != IDM_SEQ_NEXT) {
        return 2;
    }

    const ShortcutInputStroke pane_q{
        'Q', ShortcutPhysicalKeyFromVirtualKey('Q', 0U), 0U};
    const auto prefix = ResolveShortcutProfile(
        profile.bindings, ShortcutContext::Pane, std::span(&pane_q, 1U));
    if (prefix.match != INKPOD_SHORTCUT_MATCH_PREFIX) {
        return 3;
    }

    profile.bindings.push_back(
        Binding(IDM_TOOL_ERASER, ShortcutSlot::Primary, ShortcutContext::Canvas, {'W'}));
    std::vector<inkpod::windows::ui::ShortcutConflict> conflicts;
    if (ValidateShortcutProfile(profile, true, &conflicts)
            != ShortcutProfileValidation::Ok
        || conflicts.size() != 1U
        || conflicts.front().kind != ShortcutConflictKind::Exact
        || ValidateShortcutProfile(profile, false)
            != ShortcutProfileValidation::ExactConflict) {
        return 4;
    }

    profile.bindings.back() =
        Binding(IDM_TOOL_ERASER, ShortcutSlot::Primary, ShortcutContext::Pane, {'Q'});
    if (ValidateShortcutProfile(profile, true)
        != ShortcutProfileValidation::PrefixConflict) {
        return 5;
    }

    profile.bindings.back() = Binding(
        IDM_TOOL_ERASER,
        ShortcutSlot::Primary,
        ShortcutContext::Canvas,
        {'X'},
        ShortcutKeyMatch::Physical);
    const ShortcutInputStroke physical_input{
        'Y', ShortcutPhysicalKeyFromVirtualKey('X', 0U), 0U};
    const auto physical = ResolveShortcutProfile(
        profile.bindings, ShortcutContext::Canvas, std::span(&physical_input, 1U));
    if (physical.match != INKPOD_SHORTCUT_MATCH_EXACT
        || physical.command_id != IDM_TOOL_ERASER) {
        return 6;
    }

    profile.bindings.front().action = ShortcutAction::Hold;
    const auto hold = ResolveShortcutProfile(
        profile.bindings, ShortcutContext::Canvas, std::span(&canvas_b, 1U));
    if (hold.match != INKPOD_SHORTCUT_MATCH_EXACT || hold.action != ShortcutAction::Hold) {
        return 7;
    }

    profile.bindings.push_back(profile.bindings.front());
    profile.bindings.back().command_id = IDM_TOOL_PENCIL;
    profile.bindings.back().slot = ShortcutSlot::Secondary;
    if (ValidateShortcutProfile(profile, false)
        != ShortcutProfileValidation::ExactConflict) {
        std::fprintf(stderr, "duplicate applied binding was not rejected\n");
        return 8;
    }
    profile.bindings.pop_back();

    std::vector<std::uint8_t> encoded;
    if (inkpod::windows::ui::EncodeShortcutPreset(profile, encoded)
        != ShortcutPresetStatus::Ok) {
        return 9;
    }
    const std::string json(encoded.begin(), encoded.end());
    if (json.find("\"format\": \"inkpod-shortcuts\"") == std::string::npos
        || json.find("\"command\": \"tool.brush\"") == std::string::npos
        || json.find("\"logicalKey\": \"B\"") == std::string::npos
        || json.find("base64") != std::string::npos) {
        return 9;
    }
    ShortcutProfile decoded{};
    if (inkpod::windows::ui::DecodeShortcutPreset(encoded, decoded)
            != ShortcutPresetStatus::Ok
        || decoded != profile) {
        return 10;
    }
    std::vector<std::uint8_t> noncurrent = encoded;
    const std::string version_token = "\"formatVersion\": 3";
    const auto version = std::search(
        noncurrent.begin(),
        noncurrent.end(),
        version_token.begin(),
        version_token.end());
    if (version == noncurrent.end()) {
        return 11;
    }
    version[version_token.size() - 1U] = static_cast<std::uint8_t>('2');
    if (inkpod::windows::ui::DecodeShortcutPreset(noncurrent, decoded)
        != ShortcutPresetStatus::UnsupportedVersion) {
        return 11;
    }
    encoded.push_back(0U);
    if (inkpod::windows::ui::DecodeShortcutPreset(encoded, decoded)
        == ShortcutPresetStatus::Ok) {
        return 13;
    }
    return 0;
}
