#include "shortcut_controller.h"

#include <windows.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>
#include <new>
#include <span>
#include <string>
#include <vector>

#include "command_catalog.h"
#include "shortcut_preset.h"
#include "ui/localization.h"

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kSettingsKey[] = L"Software\\Inkpod";
constexpr wchar_t kSettingsValue[] = L"ShortcutSequences";
constexpr std::uint32_t kSettingsMagic = UINT32_C(0x534b5049);
constexpr std::uint32_t kLegacySettingsVersion = 1U;
constexpr std::uint32_t kSettingsVersion = 2U;
constexpr std::size_t kMaximumSettingsBytes = 8U * 1024U * 1024U;
constexpr ULONGLONG kSequenceTimeoutMilliseconds = 1'500U;

struct LegacySettingsHeader final {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t count;
    std::uint32_t reserved;
};

void PushU32(std::vector<std::uint8_t>& bytes, std::uint32_t value) {
    bytes.push_back(static_cast<std::uint8_t>(value));
    bytes.push_back(static_cast<std::uint8_t>(value >> 8U));
    bytes.push_back(static_cast<std::uint8_t>(value >> 16U));
    bytes.push_back(static_cast<std::uint8_t>(value >> 24U));
}

bool ReadU32(
    std::span<const std::uint8_t> bytes,
    std::size_t& cursor,
    std::uint32_t& value) noexcept {
    if (cursor > bytes.size() || bytes.size() - cursor < 4U) {
        return false;
    }
    value = static_cast<std::uint32_t>(bytes[cursor])
        | (static_cast<std::uint32_t>(bytes[cursor + 1U]) << 8U)
        | (static_cast<std::uint32_t>(bytes[cursor + 2U]) << 16U)
        | (static_cast<std::uint32_t>(bytes[cursor + 3U]) << 24U);
    cursor += 4U;
    return true;
}

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

bool SameSequence(
    const InkpodShortcutSequence& left,
    const InkpodShortcutSequence& right) noexcept {
    if (left.command_id != right.command_id || left.stroke_count != right.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0U; index < left.stroke_count; ++index) {
        if (left.strokes[index].virtual_key != right.strokes[index].virtual_key
            || left.strokes[index].modifiers != right.strokes[index].modifiers) {
            return false;
        }
    }
    return true;
}

bool HasCompleteCatalog(std::span<const InkpodShortcutSequence> bindings) noexcept {
    const auto commands = ShortcutCommandCatalog();
    if (bindings.size() != commands.size()) {
        return false;
    }
    return std::all_of(commands.begin(), commands.end(), [bindings](UINT command) {
        return FindShortcutSequence(bindings, command) != nullptr;
    });
}

bool BuiltInIsComplete(const ShortcutProfile& profile) noexcept {
    const auto commands = ShortcutCommandCatalog();
    return profile.built_in && std::all_of(
        commands.begin(), commands.end(), [&profile](UINT command) {
            return FindShortcutBinding(
                       std::span<const ShortcutProfileBinding>(profile.bindings),
                       command,
                       ShortcutSlot::Primary)
                != nullptr;
        });
}

bool ValidProfileSet(const ShortcutProfileSet& set) noexcept {
    if (set.profiles.empty() || set.profiles.size() > kMaximumShortcutProfiles
        || set.active_profile >= set.profiles.size()
        || !BuiltInIsComplete(set.profiles.front())
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

bool ReadRegistryBytes(std::vector<std::uint8_t>& bytes) noexcept {
    HKEY key{};
    if (RegOpenKeyExW(HKEY_CURRENT_USER, kSettingsKey, 0U, KEY_QUERY_VALUE, &key)
        != ERROR_SUCCESS) {
        return false;
    }
    DWORD type{};
    DWORD byte_count{};
    LONG result = RegQueryValueExW(
        key, kSettingsValue, nullptr, &type, nullptr, &byte_count);
    if (result != ERROR_SUCCESS || type != REG_BINARY || byte_count == 0U
        || byte_count > kMaximumSettingsBytes) {
        RegCloseKey(key);
        return false;
    }
    try {
        bytes.resize(byte_count);
    } catch (const std::bad_alloc&) {
        RegCloseKey(key);
        return false;
    }
    result = RegQueryValueExW(
        key, kSettingsValue, nullptr, &type, bytes.data(), &byte_count);
    RegCloseKey(key);
    return result == ERROR_SUCCESS
        && static_cast<std::size_t>(byte_count) == bytes.size();
}

bool WriteRegistryBytes(std::span<const std::uint8_t> bytes) noexcept {
    if (bytes.empty() || bytes.size() > kMaximumSettingsBytes
        || bytes.size() > std::numeric_limits<DWORD>::max()) {
        return false;
    }
    HKEY key{};
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            0U,
            nullptr,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            nullptr,
            &key,
            nullptr)
        != ERROR_SUCCESS) {
        return false;
    }
    const LONG result = RegSetValueExW(
        key,
        kSettingsValue,
        0U,
        REG_BINARY,
        bytes.data(),
        static_cast<DWORD>(bytes.size()));
    RegCloseKey(key);
    return result == ERROR_SUCCESS;
}

bool SavePersistedProfileSet(const ShortcutProfileSet& set) noexcept {
    if (!ValidProfileSet(set)
        || set.profiles.size() - 1U > std::numeric_limits<std::uint32_t>::max()
        || set.active_profile > std::numeric_limits<std::uint32_t>::max()) {
        return false;
    }
    try {
        std::vector<std::uint8_t> bytes;
        bytes.reserve(1'024U);
        PushU32(bytes, kSettingsMagic);
        PushU32(bytes, kSettingsVersion);
        PushU32(bytes, static_cast<std::uint32_t>(set.profiles.size() - 1U));
        PushU32(bytes, static_cast<std::uint32_t>(set.active_profile));
        PushU32(bytes, static_cast<std::uint32_t>(set.keyboard_layout));
        for (std::size_t index = 1U; index < set.profiles.size(); ++index) {
            std::vector<std::uint8_t> encoded;
            if (EncodeShortcutPreset(set.profiles[index], encoded)
                != ShortcutPresetStatus::Ok
                || encoded.size() > std::numeric_limits<std::uint32_t>::max()) {
                return false;
            }
            PushU32(bytes, static_cast<std::uint32_t>(encoded.size()));
            bytes.insert(bytes.end(), encoded.begin(), encoded.end());
            if (bytes.size() > kMaximumSettingsBytes) {
                return false;
            }
        }
        return WriteRegistryBytes(bytes);
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool LoadVersionTwo(
    std::span<const std::uint8_t> bytes,
    ShortcutProfileSet& set) noexcept {
    std::size_t cursor{};
    std::uint32_t magic{};
    std::uint32_t version{};
    std::uint32_t custom_count{};
    std::uint32_t active{};
    std::uint32_t layout{};
    if (!ReadU32(bytes, cursor, magic) || !ReadU32(bytes, cursor, version)
        || !ReadU32(bytes, cursor, custom_count) || !ReadU32(bytes, cursor, active)
        || !ReadU32(bytes, cursor, layout) || magic != kSettingsMagic
        || version != kSettingsVersion
        || custom_count >= kMaximumShortcutProfiles) {
        return false;
    }
    set.keyboard_layout = static_cast<ShortcutKeyboardLayout>(layout);
    try {
        for (std::uint32_t index = 0U; index < custom_count; ++index) {
            std::uint32_t encoded_size{};
            if (!ReadU32(bytes, cursor, encoded_size) || encoded_size == 0U
                || cursor > bytes.size() || encoded_size > bytes.size() - cursor) {
                return false;
            }
            ShortcutProfile profile{};
            if (DecodeShortcutPreset(bytes.subspan(cursor, encoded_size), profile)
                != ShortcutPresetStatus::Ok) {
                return false;
            }
            set.profiles.push_back(std::move(profile));
            cursor += encoded_size;
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (cursor != bytes.size()) {
        return false;
    }
    set.active_profile = active < set.profiles.size() ? active : 0U;
    return ValidProfileSet(set);
}

bool LoadLegacyVersionOne(
    std::span<const std::uint8_t> bytes,
    std::span<const InkpodShortcutSequence> defaults,
    ShortcutProfileSet& set) noexcept {
    if (bytes.size() < sizeof(LegacySettingsHeader)) {
        return false;
    }
    LegacySettingsHeader header{};
    std::memcpy(&header, bytes.data(), sizeof(header));
    if (header.magic != kSettingsMagic || header.version != kLegacySettingsVersion
        || header.reserved != 0U
        || header.count > std::numeric_limits<std::size_t>::max()
            / sizeof(InkpodShortcutSequence)) {
        return false;
    }
    const std::size_t expected = sizeof(header)
        + static_cast<std::size_t>(header.count) * sizeof(InkpodShortcutSequence);
    if (expected != bytes.size()) {
        return false;
    }
    std::vector<InkpodShortcutSequence> migrated;
    try {
        migrated.resize(header.count);
    } catch (const std::bad_alloc&) {
        return false;
    }
    std::memcpy(
        migrated.data(),
        bytes.data() + sizeof(header),
        migrated.size() * sizeof(migrated.front()));
    if (!HasCompleteCatalog(migrated)) {
        return false;
    }
    const bool same_as_default = migrated.size() == defaults.size()
        && std::equal(migrated.begin(), migrated.end(), defaults.begin(), SameSequence);
    if (same_as_default) {
        set.active_profile = 0U;
        return true;
    }
    try {
        set.profiles.push_back(BuildShortcutProfileFromLegacy(
            PresetName(1U), false, migrated));
    } catch (const std::bad_alloc&) {
        return false;
    }
    set.active_profile = 1U;
    return ValidProfileSet(set);
}

bool LoadPersistedProfileSet(
    std::span<const InkpodShortcutSequence> defaults,
    ShortcutProfileSet& set) noexcept {
    std::vector<std::uint8_t> bytes;
    if (!ReadRegistryBytes(bytes) || bytes.size() < 8U) {
        return false;
    }
    std::size_t cursor{};
    std::uint32_t magic{};
    std::uint32_t version{};
    if (!ReadU32(bytes, cursor, magic) || !ReadU32(bytes, cursor, version)
        || magic != kSettingsMagic) {
        return false;
    }
    if (version == kSettingsVersion) {
        return LoadVersionTwo(bytes, set);
    }
    if (version == kLegacySettingsVersion) {
        return LoadLegacyVersionOne(bytes, defaults, set);
    }
    return false;
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
    bool load_persisted) noexcept {
    try {
        const std::vector<InkpodShortcutSequence> defaults =
            BuildDefaultShortcutSequences();
        ShortcutProfileSet set{};
        set.profiles.push_back(BuildShortcutProfileFromLegacy(
            PresetName(0U), true, defaults));
        if (load_persisted) {
            ShortcutProfileSet loaded = set;
            if (LoadPersistedProfileSet(defaults, loaded)) {
                set = std::move(loaded);
            }
        }
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

InkpodStatus ApplyShortcutProfileSet(
    app::CoreHost& engine,
    ShortcutUiState& state,
    const ShortcutProfileSet& replacement,
    bool persist) noexcept {
    if (!ValidProfileSet(replacement)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    try {
        ShortcutProfileSet candidate = replacement;
        std::vector<InkpodShortcutSequence> defaults = BuildDefaultShortcutSequences();
        std::vector<InkpodShortcutSequence> mirror = BuildCoreCompatibilityMirror();
        std::vector<InkpodShortcutSequence> menu = BuildMenuBindings(
            candidate.profiles[candidate.active_profile]);
        const ShortcutProfileSet previous = state.profile_set;
        if (persist && !SavePersistedProfileSet(candidate)) {
            return INKPOD_STATUS_IO_ERROR;
        }
        InkpodStatus status = SetCoreBindings(engine, mirror, false);
        if (status == INKPOD_STATUS_OK) {
            status = UpdateSessionInitializer(engine, defaults, mirror);
        }
        if (status != INKPOD_STATUS_OK) {
            if (persist) {
                (void)SavePersistedProfileSet(previous);
            }
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
    ShortcutUiState& state,
    bool persist) noexcept {
    try {
        ShortcutProfileSet replacement = state.profile_set;
        replacement.active_profile = 0U;
        return ApplyShortcutProfileSet(engine, state, replacement, persist);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodStatus RebindShortcut(
    app::CoreHost& engine,
    ShortcutUiState& state,
    const InkpodShortcutSequence& replacement,
    bool persist) noexcept {
    if (replacement.command_id == 0U || replacement.stroke_count == 0U
        || replacement.stroke_count > INKPOD_SHORTCUT_MAX_STROKES) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
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
        if (target == nullptr) {
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
            const UINT scan = MapVirtualKeyW(source.virtual_key, MAPVK_VK_TO_VSC_EX);
            target->strokes[index] = {
                source.virtual_key,
                scan == 0U ? source.virtual_key : scan,
                source.modifiers};
        }

        std::vector<ShortcutConflict> conflicts =
            AnalyzeShortcutConflicts(profile->bindings);
        for (const ShortcutConflict& conflict : conflicts) {
            if (conflict.kind == ShortcutConflictKind::Prefix) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            const std::size_t other_index = conflict.first_index ==
                    static_cast<std::size_t>(target - profile->bindings.data())
                ? conflict.second_index
                : conflict.first_index;
            if (other_index >= profile->bindings.size()) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            ShortcutProfileBinding& other = profile->bindings[other_index];
            if (other.slot != ShortcutSlot::Primary) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            const std::uint32_t other_command = other.command_id;
            other = prior;
            other.command_id = other_command;
        }
        return ApplyShortcutProfileSet(engine, state, candidate, persist);
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
    const UINT scan = MapVirtualKeyW(stroke.virtual_key, MAPVK_VK_TO_VSC_EX);
    const ShortcutInputStroke input{
        stroke.virtual_key,
        scan == 0U ? stroke.virtual_key : scan,
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
