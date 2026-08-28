#include "ui/shortcut_profile.h"
#include "ui/shortcut_preset.h"
#include "ui/command_catalog.h"
#include "app/resource.h"

#include <algorithm>
#include <cstdio>
#include <string>
#include <vector>

namespace {

using inkpod::windows::ui::AnalyzeShortcutConflicts;
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
using inkpod::windows::ui::ShortcutPresetStatus;
using inkpod::windows::ui::ShortcutSlot;
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
        const UINT scan = MapVirtualKeyW(key, MAPVK_VK_TO_VSC_EX);
        binding.strokes[binding.stroke_count++] = ShortcutInputStroke{
            key, scan == 0U ? key : scan, 0U};
    }
    return binding;
}

}  // namespace

int main() {
    const ShortcutProfile defaults = inkpod::windows::ui::BuildShortcutProfileFromLegacy(
        L"Built-in",
        true,
        inkpod::windows::ui::BuildDefaultShortcutSequences());
    if (defaults.bindings.size()
            != inkpod::windows::ui::ShortcutCommandCatalog().size()
        || ValidateShortcutProfile(defaults, false)
            != ShortcutProfileValidation::Ok) {
        std::fprintf(stderr, "built-in shortcut profile is incomplete or conflicting\n");
        return 12;
    }

    ShortcutProfile profile{L"Custom", false, {}};
    profile.bindings = {
        Binding(IDM_TOOL_BRUSH, ShortcutSlot::Primary, ShortcutContext::Canvas, {'B'}),
        Binding(IDM_TOOL_BRUSH, ShortcutSlot::Secondary, ShortcutContext::Canvas, {'5'}),
        Binding(IDM_SEQ_NEXT, ShortcutSlot::Primary, ShortcutContext::Timeline, {'B'}),
        Binding(IDM_VIEW_GRID, ShortcutSlot::Primary, ShortcutContext::Global, {'W'}),
        Binding(IDM_WINDOW_TOOL_PALETTE, ShortcutSlot::Primary, ShortcutContext::Pane, {'Q', 'F'})};

    if (ValidateShortcutProfile(profile, false) != ShortcutProfileValidation::Ok
        || FindShortcutBinding(
               std::span<const ShortcutProfileBinding>(profile.bindings),
               IDM_TOOL_BRUSH,
               ShortcutSlot::Secondary)
            == nullptr) {
        return 1;
    }

    const ShortcutInputStroke canvas_b{
        'B', MapVirtualKeyW('B', MAPVK_VK_TO_VSC_EX), 0U};
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
        'Q', MapVirtualKeyW('Q', MAPVK_VK_TO_VSC_EX), 0U};
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
        'Y', MapVirtualKeyW('X', MAPVK_VK_TO_VSC_EX), 0U};
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
