#include "shortcut_controller.h"

#include <windows.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <new>
#include <span>
#include <string>
#include <vector>

#include "command_catalog.h"
#include "ui/localization.h"

namespace inkpod::windows::ui {
namespace {

constexpr ULONGLONG kSequenceTimeoutMilliseconds = 1'500U;

std::wstring PresetName(std::size_t ordinal) {
    std::wstring result(UiText(
        ordinal == 0U
            ? UiStringId::ShortcutBuiltInPreset
            : UiStringId::ShortcutPresetLabel));
    if (ordinal != 0U) {
        result += L' ';
        result += std::to_wstring(ordinal);
    }
    return result;
}

bool ValidProfileSet(const ShortcutProfileSet& set) noexcept {
    if (set.profiles.empty() || set.profiles.size() > kMaximumShortcutProfiles
        || set.active_profile >= set.profiles.size()
        || !set.profiles.front().built_in
        || (set.keyboard_layout != ShortcutKeyboardLayout::Automatic
            && set.keyboard_layout != ShortcutKeyboardLayout::Jis109
            && set.keyboard_layout != ShortcutKeyboardLayout::UsAnsi104)) {
        return false;
    }
    for (std::size_t index = 0U; index < set.profiles.size(); ++index) {
        if ((index == 0U) != set.profiles[index].built_in
            || ValidateShortcutProfile(set.profiles[index], false)
                != ShortcutProfileValidation::Ok) {
            return false;
        }
    }
    return true;
}

InkpodStatus SetCoreBindings(
    app::CoreHost& engine,
    std::span<const InkpodShortcutSequence> bindings,
    bool defaults) noexcept {
    return engine.InvokeAll(
        [bindings, defaults](InkpodCore* core) {
            return defaults
                ? inkpod_core_shortcut_defaults_set(
                      core, bindings.data(), bindings.size(), sizeof(InkpodShortcutSequence))
                : inkpod_core_shortcut_sequences_set(
                      core, bindings.data(), bindings.size(), sizeof(InkpodShortcutSequence));
        },
        false,
        false);
}

InkpodStatus UpdateSessionInitializer(
    app::CoreHost& engine,
    std::span<const InkpodShortcutSequence> defaults,
    std::span<const InkpodShortcutSequence> bindings) noexcept {
    try {
        std::vector<InkpodShortcutSequence> default_copy(defaults.begin(), defaults.end());
        std::vector<InkpodShortcutSequence> binding_copy(bindings.begin(), bindings.end());
        engine.SetSessionInitializer(
            [defaults = std::move(default_copy), current = std::move(binding_copy)](
                InkpodCore* core) {
                InkpodStatus status = inkpod_core_shortcut_defaults_set(
                    core,
                    defaults.data(),
                    defaults.size(),
                    sizeof(InkpodShortcutSequence));
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_shortcut_sequences_set(
                        core,
                        current.data(),
                        current.size(),
                        sizeof(InkpodShortcutSequence));
                }
                return status;
            });
        return INKPOD_STATUS_OK;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

std::vector<InkpodShortcutSequence> BuildMenuBindings(const ShortcutProfile& profile) {
    std::vector<InkpodShortcutSequence> result;
    result.reserve(ShortcutCommandCatalog().size());
    for (const UINT command : ShortcutCommandCatalog()) {
        const ShortcutProfileBinding* binding = FindShortcutBinding(
            std::span<const ShortcutProfileBinding>(profile.bindings),
            command,
            ShortcutSlot::Primary);
        if (binding == nullptr) {
            binding = FindShortcutBinding(
                std::span<const ShortcutProfileBinding>(profile.bindings),
                command,
                ShortcutSlot::Secondary);
        }
        if (binding == nullptr) {
            continue;
        }
        InkpodShortcutSequence sequence{};
        sequence.struct_size = sizeof(sequence);
        sequence.command_id = command;
        sequence.stroke_count = binding->stroke_count;
        for (std::uint32_t index = 0U; index < binding->stroke_count; ++index) {
            sequence.strokes[index].virtual_key = binding->strokes[index].logical_key;
            sequence.strokes[index].modifiers = binding->strokes[index].modifiers;
        }
        result.push_back(sequence);
    }
    return result;
}

std::vector<InkpodShortcutSequence> BuildCoreCompatibilityMirror() {
    // Runtime input routing owns contexts, physical-key matching, secondary
    // bindings, hold and toggle. The current Core ABI remains a collision-free
    // logical default mirror until that OS/focus-specific model has a typed ABI.
    return BuildDefaultShortcutSequences();
}

void UpdatePendingText(ShortcutUiState& state) noexcept {
    InkpodShortcutSequence pending{};
    pending.struct_size = sizeof(pending);
    pending.stroke_count = static_cast<std::uint32_t>(state.pending_strokes.size());
    for (std::size_t index = 0U; index < state.pending_strokes.size(); ++index) {
        pending.strokes[index].virtual_key = state.pending_strokes[index].logical_key;
        pending.strokes[index].modifiers = state.pending_strokes[index].modifiers;
    }
    try {
        state.pending_text = UiText(UiStringId::Text0198)
            + FormatShortcutSequence(pending) + L", …";
    } catch (const std::bad_alloc&) {
        state.pending_text.clear();
    }
}

}  // namespace

const ShortcutProfile* ActiveShortcutProfile(const ShortcutUiState& state) noexcept {
    return state.profile_set.active_profile < state.profile_set.profiles.size()
        ? &state.profile_set.profiles[state.profile_set.active_profile]
        : nullptr;
}

ShortcutProfile* ActiveShortcutProfile(ShortcutUiState& state) noexcept {
    return state.profile_set.active_profile < state.profile_set.profiles.size()
        ? &state.profile_set.profiles[state.profile_set.active_profile]
        : nullptr;
}

InkpodStatus InitializeShortcuts(
    app::CoreHost& engine,
    ShortcutUiState& state,
    const ShortcutProfileSet& initial_profiles) noexcept {
    try {
        const std::vector<InkpodShortcutSequence> defaults =
            BuildDefaultShortcutSequences();
        if (!ValidProfileSet(initial_profiles)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        ShortcutProfileSet set = initial_profiles;
        const std::vector<InkpodShortcutSequence> mirror =
            BuildCoreCompatibilityMirror();
        InkpodStatus status = SetCoreBindings(engine, defaults, true);
        if (status == INKPOD_STATUS_OK) {
            status = SetCoreBindings(engine, mirror, false);
        }
        if (status == INKPOD_STATUS_OK) {
            status = UpdateSessionInitializer(engine, defaults, mirror);
        }
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        state.profile_set = std::move(set);
        state.bindings = BuildMenuBindings(*ActiveShortcutProfile(state));
        ClearPendingShortcut(state);
        return INKPOD_STATUS_OK;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

ShortcutProfileSet BuildDefaultShortcutProfileSet() {
    ShortcutProfileSet result{};
    result.profiles.push_back(BuildDefaultShortcutProfile(PresetName(0U)));
    return result;
}

InkpodStatus ApplyShortcutProfileSet(
    app::CoreHost& engine,
    ShortcutUiState& state,
    const ShortcutProfileSet& replacement) noexcept {
    if (!ValidProfileSet(replacement)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    try {
        ShortcutProfileSet candidate = replacement;
        std::vector<InkpodShortcutSequence> defaults = BuildDefaultShortcutSequences();
        std::vector<InkpodShortcutSequence> mirror = BuildCoreCompatibilityMirror();
        std::vector<InkpodShortcutSequence> menu = BuildMenuBindings(
            candidate.profiles[candidate.active_profile]);
        InkpodStatus status = SetCoreBindings(engine, mirror, false);
        if (status == INKPOD_STATUS_OK) {
            status = UpdateSessionInitializer(engine, defaults, mirror);
        }
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        state.profile_set = std::move(candidate);
        state.bindings = std::move(menu);
        ClearPendingShortcut(state);
        return INKPOD_STATUS_OK;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodStatus ResetShortcuts(
    app::CoreHost& engine,
    ShortcutUiState& state) noexcept {
    try {
        ShortcutProfileSet replacement = state.profile_set;
        replacement.active_profile = 0U;
        return ApplyShortcutProfileSet(engine, state, replacement);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodStatus RebindShortcut(
    app::CoreHost& engine,
    ShortcutUiState& state,
    const InkpodShortcutSequence& replacement) noexcept {
    if (replacement.command_id == 0U || replacement.stroke_count == 0U
        || replacement.stroke_count > INKPOD_SHORTCUT_MAX_STROKES) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    for (std::uint32_t index = 0U; index < replacement.stroke_count; ++index) {
        const InkpodShortcutStroke& source = replacement.strokes[index];
        const ShortcutInputStroke stroke{
            source.virtual_key,
            ShortcutPhysicalKeyFromVirtualKey(
                source.virtual_key, source.modifiers),
            source.modifiers};
        if (ShortcutStrokeReservedForNativeMenu(
                replacement.command_id, stroke)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
    }
    try {
        ShortcutProfileSet candidate = state.profile_set;
        ShortcutProfile* profile = candidate.active_profile < candidate.profiles.size()
            ? &candidate.profiles[candidate.active_profile]
            : nullptr;
        if (profile == nullptr) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        if (profile->built_in) {
            if (candidate.profiles.size() >= kMaximumShortcutProfiles) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            ShortcutProfile custom = *profile;
            custom.built_in = false;
            custom.name = PresetName(candidate.profiles.size());
            candidate.profiles.push_back(std::move(custom));
            candidate.active_profile = candidate.profiles.size() - 1U;
            profile = &candidate.profiles.back();
        }

        ShortcutProfileBinding prior{};
        ShortcutProfileBinding* target = FindShortcutBinding(
            std::span<ShortcutProfileBinding>(profile->bindings),
            replacement.command_id,
            ShortcutSlot::Primary);
        const bool target_existed = target != nullptr;
        if (!target_existed) {
            prior.command_id = replacement.command_id;
            prior.context = DefaultShortcutContext(replacement.command_id);
            prior.action = DefaultShortcutAction(replacement.command_id);
            prior.key_match = ShortcutKeyMatch::Logical;
            profile->bindings.push_back(prior);
            target = &profile->bindings.back();
        } else {
            prior = *target;
        }
        target->stroke_count = replacement.stroke_count;
        target->key_match = ShortcutKeyMatch::Logical;
        for (std::uint32_t index = 0U; index < replacement.stroke_count; ++index) {
            const auto& source = replacement.strokes[index];
            target->strokes[index] = {
                source.virtual_key,
                ShortcutPhysicalKeyFromVirtualKey(
                    source.virtual_key, source.modifiers),
                source.modifiers};
        }

        std::vector<ShortcutConflict> conflicts =
            AnalyzeShortcutConflicts(profile->bindings);
        const std::size_t target_index = static_cast<std::size_t>(
            target - profile->bindings.data());
        std::vector<std::size_t> exact_others;
        for (const ShortcutConflict& conflict : conflicts) {
            if (conflict.kind == ShortcutConflictKind::Prefix) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            const bool first_is_target = conflict.first_index == target_index;
            const bool second_is_target = conflict.second_index == target_index;
            if (first_is_target == second_is_target) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            const std::size_t other_index = first_is_target
                ? conflict.second_index
                : conflict.first_index;
            if (other_index >= profile->bindings.size()) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            if (profile->bindings[other_index].slot != ShortcutSlot::Primary) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            exact_others.push_back(other_index);
        }
        if (target_existed) {
            if (exact_others.size() > 1U) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            if (!exact_others.empty()) {
                ShortcutProfileBinding& other =
                    profile->bindings[exact_others.front()];
                ShortcutProfileBinding swapped = other;
                swapped.key_match = prior.key_match;
                swapped.stroke_count = prior.stroke_count;
                swapped.strokes = prior.strokes;
                if (std::any_of(
                        swapped.strokes.begin(),
                        swapped.strokes.begin() + swapped.stroke_count,
                        [&](const ShortcutInputStroke& stroke) {
                            return ShortcutStrokeReservedForNativeMenu(
                                swapped.command_id, stroke);
                        })) {
                    return INKPOD_STATUS_INVALID_ARGUMENT;
                }
                other = swapped;
            }
        } else {
            std::sort(exact_others.rbegin(), exact_others.rend());
            exact_others.erase(
                std::unique(exact_others.begin(), exact_others.end()),
                exact_others.end());
            for (const std::size_t other_index : exact_others) {
                profile->bindings.erase(
                    profile->bindings.begin()
                    + static_cast<std::ptrdiff_t>(other_index));
            }
        }
        return ApplyShortcutProfileSet(engine, state, candidate);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

ShortcutResolution ResolveShortcutStroke(
    ShortcutUiState& state,
    ShortcutContext context,
    ShortcutInputStroke stroke) noexcept {
    const ShortcutProfile* profile = ActiveShortcutProfile(state);
    if (profile == nullptr) {
        return {};
    }
    const ULONGLONG now = GetTickCount64();
    if ((state.pending_deadline != 0U && now > state.pending_deadline)
        || (!state.pending_strokes.empty() && state.pending_context != context)) {
        ClearPendingShortcut(state);
    }
    state.pending_context = context;
    try {
        state.pending_strokes.push_back(stroke);
    } catch (const std::bad_alloc&) {
        ClearPendingShortcut(state);
        return {};
    }
    ShortcutResolution result = ResolveShortcutProfile(
        profile->bindings, context, state.pending_strokes);
    if (result.match == INKPOD_SHORTCUT_MATCH_EXACT) {
        ClearPendingShortcut(state);
        return result;
    }
    if (result.match == INKPOD_SHORTCUT_MATCH_PREFIX) {
        state.pending_deadline = now + kSequenceTimeoutMilliseconds;
        UpdatePendingText(state);
        return result;
    }

    ClearPendingShortcut(state);
    state.pending_context = context;
    try {
        state.pending_strokes.push_back(stroke);
    } catch (const std::bad_alloc&) {
        return {};
    }
    result = ResolveShortcutProfile(profile->bindings, context, state.pending_strokes);
    if (result.match == INKPOD_SHORTCUT_MATCH_PREFIX) {
        state.pending_deadline = now + kSequenceTimeoutMilliseconds;
        UpdatePendingText(state);
    } else {
        ClearPendingShortcut(state);
    }
    return result;
}

InkpodShortcutMatch ResolveShortcutStroke(
    ShortcutUiState& state,
    InkpodShortcutStroke stroke,
    UINT& command) noexcept {
    const ShortcutInputStroke input{
        stroke.virtual_key,
        ShortcutPhysicalKeyFromVirtualKey(
            stroke.virtual_key, stroke.modifiers),
        stroke.modifiers};
    for (const ShortcutContext context : {
             ShortcutContext::Global,
             ShortcutContext::Canvas,
             ShortcutContext::Timeline,
             ShortcutContext::Pane}) {
        const ShortcutResolution result = ResolveShortcutStroke(state, context, input);
        if (result.match != INKPOD_SHORTCUT_MATCH_NONE) {
            command = result.command_id;
            return result.match;
        }
    }
    command = 0U;
    return INKPOD_SHORTCUT_MATCH_NONE;
}

void ClearPendingShortcut(ShortcutUiState& state) noexcept {
    state.pending_strokes.clear();
    state.pending_context = ShortcutContext::Global;
    state.pending_deadline = 0U;
    state.pending_text.clear();
}

}  // namespace inkpod::windows::ui
