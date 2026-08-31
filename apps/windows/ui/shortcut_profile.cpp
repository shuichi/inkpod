#include "shortcut_profile.h"

#include <algorithm>
#include <new>
#include <tuple>

#include "app/resource.h"
#include "command_catalog.h"

namespace inkpod::windows::ui {
namespace {

bool ValidSlot(ShortcutSlot value) noexcept {
    return value == ShortcutSlot::Primary || value == ShortcutSlot::Secondary;
}

bool ValidContext(ShortcutContext value) noexcept {
    return value == ShortcutContext::Global || value == ShortcutContext::Canvas
        || value == ShortcutContext::Timeline || value == ShortcutContext::Pane;
}

bool ValidAction(ShortcutAction value) noexcept {
    return value == ShortcutAction::Execute || value == ShortcutAction::Hold
        || value == ShortcutAction::Toggle;
}

bool ValidKeyMatch(ShortcutKeyMatch value) noexcept {
    return value == ShortcutKeyMatch::Logical || value == ShortcutKeyMatch::Physical;
}

bool SameStroke(
    const ShortcutProfileBinding& binding,
    const ShortcutInputStroke& expected,
    const ShortcutInputStroke& entered) noexcept {
    const std::uint32_t expected_key = binding.key_match == ShortcutKeyMatch::Physical
        ? expected.physical_key
        : expected.logical_key;
    const std::uint32_t entered_key = binding.key_match == ShortcutKeyMatch::Physical
        ? entered.physical_key
        : entered.logical_key;
    return expected_key != 0U && expected_key == entered_key
        && expected.modifiers == entered.modifiers;
}

bool StartsWith(
    const ShortcutProfileBinding& sequence,
    const ShortcutProfileBinding& prefix) noexcept {
    if (sequence.key_match != prefix.key_match
        || prefix.stroke_count > sequence.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0U; index < prefix.stroke_count; ++index) {
        if (!SameStroke(sequence, sequence.strokes[index], prefix.strokes[index])) {
            return false;
        }
    }
    return true;
}

bool StartsWithEntered(
    const ShortcutProfileBinding& binding,
    std::span<const ShortcutInputStroke> entered) noexcept {
    if (entered.size() > binding.stroke_count) {
        return false;
    }
    for (std::size_t index = 0U; index < entered.size(); ++index) {
        if (!SameStroke(binding, binding.strokes[index], entered[index])) {
            return false;
        }
    }
    return true;
}

bool ContextMatches(ShortcutContext binding, ShortcutContext active) noexcept {
    return binding == ShortcutContext::Global || binding == active;
}

bool ValidBinding(const ShortcutProfileBinding& binding) noexcept {
    if (binding.command_id == 0U || !ValidSlot(binding.slot)
        || !ValidContext(binding.context) || !ValidAction(binding.action)
        || !ValidKeyMatch(binding.key_match) || binding.stroke_count == 0U
        || binding.stroke_count > INKPOD_SHORTCUT_MAX_STROKES) {
        return false;
    }
    const std::uint32_t action_bit =
        1U << (static_cast<std::uint32_t>(binding.action) - 1U);
    if (CommandStableKey(binding.command_id).empty()
        || (SupportedShortcutActionMask(binding.command_id) & action_bit) == 0U) {
        return false;
    }
    for (std::uint32_t index = 0U; index < binding.stroke_count; ++index) {
        const auto& stroke = binding.strokes[index];
        if (stroke.logical_key == 0U || stroke.physical_key == 0U
            || (stroke.modifiers & ~kShortcutProfileModifierMask) != 0U) {
            return false;
        }
    }
    return true;
}

}  // namespace

bool ShortcutContextsOverlap(ShortcutContext left, ShortcutContext right) noexcept {
    return left == ShortcutContext::Global || right == ShortcutContext::Global
        || left == right;
}

std::vector<ShortcutConflict> AnalyzeShortcutConflicts(
    std::span<const ShortcutProfileBinding> bindings) {
    std::vector<ShortcutConflict> result;
    for (std::size_t left = 0U; left < bindings.size(); ++left) {
        if (!ValidBinding(bindings[left])) {
            continue;
        }
        for (std::size_t right = left + 1U; right < bindings.size(); ++right) {
            if (!ValidBinding(bindings[right])
                || !ShortcutContextsOverlap(
                    bindings[left].context, bindings[right].context)
                || bindings[left].key_match != bindings[right].key_match) {
                continue;
            }
            const bool left_starts = StartsWith(bindings[left], bindings[right]);
            const bool right_starts = StartsWith(bindings[right], bindings[left]);
            if (!left_starts && !right_starts) {
                continue;
            }
            result.push_back({
                bindings[left].stroke_count == bindings[right].stroke_count
                    ? ShortcutConflictKind::Exact
                    : ShortcutConflictKind::Prefix,
                left,
                right});
        }
    }
    return result;
}

ShortcutProfileValidation ValidateShortcutProfile(
    const ShortcutProfile& profile,
    bool allow_exact_conflicts,
    std::vector<ShortcutConflict>* conflicts) noexcept {
    if (profile.name.empty() || profile.name.size() > kMaximumShortcutProfileNameLength
        || profile.bindings.size() > kMaximumShortcutProfileBindings) {
        return ShortcutProfileValidation::CapacityExceeded;
    }
    for (std::size_t index = 0U; index < profile.bindings.size(); ++index) {
        const auto& binding = profile.bindings[index];
        if (!ValidBinding(binding)) {
            return ShortcutProfileValidation::InvalidValue;
        }
        if (std::any_of(
                profile.bindings.begin(),
                profile.bindings.begin() + static_cast<std::ptrdiff_t>(index),
                [&binding](const auto& candidate) {
                    return candidate.command_id == binding.command_id
                        && candidate.slot == binding.slot;
                })) {
            return ShortcutProfileValidation::DuplicateSlot;
        }
    }
    try {
        std::vector<ShortcutConflict> found = AnalyzeShortcutConflicts(profile.bindings);
        if (conflicts != nullptr) {
            *conflicts = found;
        }
        if (std::any_of(found.begin(), found.end(), [](const auto& conflict) {
                return conflict.kind == ShortcutConflictKind::Prefix;
            })) {
            return ShortcutProfileValidation::PrefixConflict;
        }
        if (!allow_exact_conflicts && !found.empty()) {
            return ShortcutProfileValidation::ExactConflict;
        }
        return ShortcutProfileValidation::Ok;
    } catch (const std::bad_alloc&) {
        return ShortcutProfileValidation::CapacityExceeded;
    }
}

ShortcutResolution ResolveShortcutProfile(
    std::span<const ShortcutProfileBinding> bindings,
    ShortcutContext active_context,
    std::span<const ShortcutInputStroke> entered) noexcept {
    ShortcutResolution result{};
    if (!ValidContext(active_context) || entered.empty()
        || entered.size() > INKPOD_SHORTCUT_MAX_STROKES) {
        return result;
    }
    bool prefix{};
    for (const auto& binding : bindings) {
        if (!ValidBinding(binding) || !ContextMatches(binding.context, active_context)
            || !StartsWithEntered(binding, entered)) {
            continue;
        }
        if (entered.size() == binding.stroke_count) {
            if (result.match == INKPOD_SHORTCUT_MATCH_EXACT) {
                return ShortcutResolution{};
            }
            result.match = INKPOD_SHORTCUT_MATCH_EXACT;
            result.command_id = binding.command_id;
            result.slot = binding.slot;
            result.action = binding.action;
        } else {
            prefix = true;
        }
    }
    if (result.match != INKPOD_SHORTCUT_MATCH_EXACT && prefix) {
        result.match = INKPOD_SHORTCUT_MATCH_PREFIX;
    }
    return result;
}

const ShortcutProfileBinding* FindShortcutBinding(
    std::span<const ShortcutProfileBinding> bindings,
    std::uint32_t command_id,
    ShortcutSlot slot) noexcept {
    const auto found = std::find_if(bindings.begin(), bindings.end(), [=](const auto& binding) {
        return binding.command_id == command_id && binding.slot == slot;
    });
    return found == bindings.end() ? nullptr : &*found;
}

ShortcutProfileBinding* FindShortcutBinding(
    std::span<ShortcutProfileBinding> bindings,
    std::uint32_t command_id,
    ShortcutSlot slot) noexcept {
    const auto found = std::find_if(bindings.begin(), bindings.end(), [=](const auto& binding) {
        return binding.command_id == command_id && binding.slot == slot;
    });
    return found == bindings.end() ? nullptr : &*found;
}

std::vector<InkpodShortcutSequence> FlattenShortcutProfile(const ShortcutProfile& profile) {
    std::vector<InkpodShortcutSequence> result;
    result.reserve(profile.bindings.size());
    for (const ShortcutProfileBinding& binding : profile.bindings) {
        if (binding.slot != ShortcutSlot::Primary
            || binding.key_match != ShortcutKeyMatch::Logical) {
            continue;
        }
        InkpodShortcutSequence sequence{};
        sequence.struct_size = sizeof(sequence);
        sequence.command_id = binding.command_id;
        sequence.stroke_count = binding.stroke_count;
        for (std::uint32_t index = 0U; index < binding.stroke_count; ++index) {
            sequence.strokes[index].virtual_key = binding.strokes[index].logical_key;
            sequence.strokes[index].modifiers = binding.strokes[index].modifiers
                & (INKPOD_SHORTCUT_MODIFIER_CONTROL | INKPOD_SHORTCUT_MODIFIER_SHIFT
                   | INKPOD_SHORTCUT_MODIFIER_ALT | INKPOD_SHORTCUT_MODIFIER_EXTENDED);
        }
        result.push_back(sequence);
    }
    return result;
}

ShortcutProfile BuildShortcutProfileFromLegacy(
    std::wstring name,
    bool built_in,
    std::span<const InkpodShortcutSequence> sequences) {
    ShortcutProfile profile{std::move(name), built_in, {}};
    profile.bindings.reserve(sequences.size());
    for (const auto& sequence : sequences) {
        ShortcutProfileBinding binding{};
        binding.command_id = sequence.command_id;
        binding.context = DefaultShortcutContext(sequence.command_id);
        binding.action = DefaultShortcutAction(sequence.command_id);
        binding.stroke_count = std::min(
            sequence.stroke_count,
            static_cast<std::uint32_t>(INKPOD_SHORTCUT_MAX_STROKES));
        for (std::uint32_t index = 0U; index < binding.stroke_count; ++index) {
            const auto& source = sequence.strokes[index];
            binding.strokes[index] = {
                source.virtual_key,
                ShortcutPhysicalKeyFromVirtualKey(
                    source.virtual_key, source.modifiers),
                source.modifiers};
        }
        profile.bindings.push_back(binding);
    }
    return profile;
}

bool ShortcutStrokeReservedForNativeMenu(
    std::uint32_t command_id, const ShortcutInputStroke& stroke) noexcept {
    if (stroke.logical_key == VK_MENU || stroke.logical_key == VK_LMENU
        || stroke.logical_key == VK_RMENU) {
        return true;
    }
    constexpr std::uint32_t navigation_modifiers =
        INKPOD_SHORTCUT_MODIFIER_CONTROL | INKPOD_SHORTCUT_MODIFIER_SHIFT
        | INKPOD_SHORTCUT_MODIFIER_ALT | kShortcutModifierWindows;
    const std::uint32_t modifiers = stroke.modifiers & navigation_modifiers;
    if (stroke.logical_key == VK_F10 && modifiers == 0U) {
        return true;
    }
    if (stroke.logical_key == VK_F4
        && modifiers == INKPOD_SHORTCUT_MODIFIER_ALT) {
        return command_id != IDM_APP_EXIT;
    }
    if ((modifiers & INKPOD_SHORTCUT_MODIFIER_ALT) == 0U
        || (modifiers
            & (INKPOD_SHORTCUT_MODIFIER_CONTROL | kShortcutModifierWindows))
            != 0U) {
        return false;
    }
    if (stroke.logical_key == VK_SPACE) {
        return true;
    }
    constexpr std::array<std::uint32_t, 11U> top_level_mnemonics{
        'F', 'E', 'V', 'L', 'S', 'I', 'T', 'C', 'P', 'W', 'H'};
    return std::find(
               top_level_mnemonics.begin(),
               top_level_mnemonics.end(),
               stroke.logical_key)
        != top_level_mnemonics.end();
}

std::uint32_t ShortcutPhysicalKeyFromMessage(WPARAM virtual_key, LPARAM key_data) noexcept {
    const auto scan = static_cast<std::uint32_t>((key_data >> 16U) & 0xffU);
    const bool extended = (key_data & (1L << 24U)) != 0;
    if (scan != 0U) {
        return scan | (extended ? UINT32_C(0x100) : 0U);
    }
    return ShortcutPhysicalKeyFromVirtualKey(
        static_cast<std::uint32_t>(virtual_key),
        extended ? INKPOD_SHORTCUT_MODIFIER_EXTENDED : 0U);
}

}  // namespace inkpod::windows::ui
