#include "shortcut_controller.h"

#include <windows.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>
#include <new>
#include <span>
#include <vector>

#include "command_catalog.h"

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kSettingsKey[] = L"Software\\Inkpod";
constexpr wchar_t kSettingsValue[] = L"ShortcutSequences";
constexpr std::uint32_t kSettingsMagic = UINT32_C(0x534b5049);
constexpr std::uint32_t kSettingsVersion = 1U;
constexpr ULONGLONG kSequenceTimeoutMilliseconds = 1'500U;

struct SettingsHeader {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t count;
    std::uint32_t reserved;
};

bool SameSequence(
    const InkpodShortcutSequence& left,
    const InkpodShortcutSequence& right) noexcept {
    if (left.stroke_count != right.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0; index < left.stroke_count; ++index) {
        if (left.strokes[index].virtual_key != right.strokes[index].virtual_key
            || left.strokes[index].modifiers != right.strokes[index].modifiers) {
            return false;
        }
    }
    return true;
}

bool StartsWith(
    const InkpodShortcutSequence& sequence,
    const InkpodShortcutSequence& prefix) noexcept {
    if (prefix.stroke_count > sequence.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0; index < prefix.stroke_count; ++index) {
        if (sequence.strokes[index].virtual_key != prefix.strokes[index].virtual_key
            || sequence.strokes[index].modifiers != prefix.strokes[index].modifiers) {
            return false;
        }
    }
    return true;
}

bool Conflicts(
    const InkpodShortcutSequence& left,
    const InkpodShortcutSequence& right) noexcept {
    return StartsWith(left, right) || StartsWith(right, left);
}

bool HasCompleteCatalog(std::span<const InkpodShortcutSequence> bindings) noexcept {
    const auto commands = MenuCommandCatalog();
    if (bindings.size() != commands.size()) {
        return false;
    }
    return std::all_of(commands.begin(), commands.end(), [bindings](UINT command) {
        return FindShortcutSequence(bindings, command) != nullptr;
    });
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

InkpodStatus CopyCoreBindings(
    app::CoreHost& engine,
    std::vector<InkpodShortcutSequence>& bindings) noexcept {
    std::uint64_t count{};
    InkpodStatus status = engine.Invoke(
        [&count](InkpodCore* core) {
            return inkpod_core_shortcut_sequences_copy(core, nullptr, 0, 0, &count);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK
        || count > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())) {
        return status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status;
    }
    try {
        bindings.assign(static_cast<std::size_t>(count), InkpodShortcutSequence{});
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return engine.Invoke(
        [&bindings, &count](InkpodCore* core) {
            return inkpod_core_shortcut_sequences_copy(
                core,
                bindings.data(),
                bindings.size(),
                sizeof(InkpodShortcutSequence),
                &count);
        },
        false,
        false);
}

bool LoadPersistedShortcuts(std::vector<InkpodShortcutSequence>& bindings) noexcept {
    HKEY key{};
    if (RegOpenKeyExW(HKEY_CURRENT_USER, kSettingsKey, 0, KEY_QUERY_VALUE, &key) != ERROR_SUCCESS) {
        return false;
    }
    DWORD type{};
    DWORD byte_count{};
    LONG result = RegQueryValueExW(
        key, kSettingsValue, nullptr, &type, nullptr, &byte_count);
    if (result != ERROR_SUCCESS || type != REG_BINARY || byte_count < sizeof(SettingsHeader)) {
        RegCloseKey(key);
        return false;
    }
    std::vector<std::byte> bytes;
    try {
        bytes.resize(byte_count);
    } catch (const std::bad_alloc&) {
        RegCloseKey(key);
        return false;
    }
    result = RegQueryValueExW(
        key,
        kSettingsValue,
        nullptr,
        &type,
        reinterpret_cast<BYTE*>(bytes.data()),
        &byte_count);
    RegCloseKey(key);
    if (result != ERROR_SUCCESS) {
        return false;
    }
    SettingsHeader header{};
    std::memcpy(&header, bytes.data(), sizeof(header));
    const std::size_t expected = sizeof(header)
        + static_cast<std::size_t>(header.count) * sizeof(InkpodShortcutSequence);
    if (header.magic != kSettingsMagic || header.version != kSettingsVersion
        || expected != bytes.size()) {
        return false;
    }
    try {
        bindings.resize(header.count);
    } catch (const std::bad_alloc&) {
        return false;
    }
    std::memcpy(
        bindings.data(), bytes.data() + sizeof(header), bindings.size() * sizeof(bindings[0]));
    return HasCompleteCatalog(bindings);
}

bool SavePersistedShortcuts(std::span<const InkpodShortcutSequence> bindings) noexcept {
    if (!HasCompleteCatalog(bindings)
        || bindings.size() > static_cast<std::size_t>(std::numeric_limits<std::uint32_t>::max())) {
        return false;
    }
    const SettingsHeader header{
        kSettingsMagic, kSettingsVersion, static_cast<std::uint32_t>(bindings.size()), 0U};
    std::vector<std::byte> bytes;
    try {
        bytes.resize(sizeof(header) + bindings.size_bytes());
    } catch (const std::bad_alloc&) {
        return false;
    }
    std::memcpy(bytes.data(), &header, sizeof(header));
    std::memcpy(bytes.data() + sizeof(header), bindings.data(), bindings.size_bytes());
    HKEY key{};
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            0,
            nullptr,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            nullptr,
            &key,
            nullptr) != ERROR_SUCCESS) {
        return false;
    }
    const LONG result = RegSetValueExW(
        key,
        kSettingsValue,
        0,
        REG_BINARY,
        reinterpret_cast<const BYTE*>(bytes.data()),
        static_cast<DWORD>(bytes.size()));
    RegCloseKey(key);
    return result == ERROR_SUCCESS;
}

InkpodShortcutMatch ResolvePending(
    ShortcutUiState& state,
    UINT& command) noexcept {
    std::uint32_t match = INKPOD_SHORTCUT_MATCH_NONE;
    std::uint32_t resolved{};
    const InkpodStatus status = inkpod_shortcut_sequence_resolve(
        state.bindings.data(),
        state.bindings.size(),
        sizeof(InkpodShortcutSequence),
        state.pending_strokes.data(),
        static_cast<std::uint32_t>(state.pending_strokes.size()),
        &match,
        &resolved);
    if (status != INKPOD_STATUS_OK) {
        return INKPOD_SHORTCUT_MATCH_NONE;
    }
    command = resolved;
    return match;
}

void UpdatePendingText(ShortcutUiState& state) noexcept {
    InkpodShortcutSequence pending{};
    pending.struct_size = sizeof(pending);
    pending.stroke_count = static_cast<std::uint32_t>(state.pending_strokes.size());
    std::copy(state.pending_strokes.begin(), state.pending_strokes.end(), pending.strokes);
    try {
        state.pending_text = L"ショートカット: " + FormatShortcutSequence(pending) + L", …";
    } catch (const std::bad_alloc&) {
        state.pending_text.clear();
    }
}

}  // namespace

InkpodStatus InitializeShortcuts(
    app::CoreHost& engine,
    ShortcutUiState& state,
    bool load_persisted) noexcept {
    std::vector<InkpodShortcutSequence> defaults;
    std::vector<InkpodShortcutSequence> catalog_defaults;
    try {
        defaults = BuildDefaultShortcutSequences();
        catalog_defaults = defaults;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodStatus status = SetCoreBindings(engine, catalog_defaults, true);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (load_persisted) {
        std::vector<InkpodShortcutSequence> persisted;
        if (LoadPersistedShortcuts(persisted)) {
            const InkpodStatus persisted_status = SetCoreBindings(engine, persisted, false);
            if (persisted_status == INKPOD_STATUS_OK) {
                defaults = std::move(persisted);
            }
        }
    }
    state.bindings = std::move(defaults);
    status = UpdateSessionInitializer(
        engine, catalog_defaults, state.bindings);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    ClearPendingShortcut(state);
    return INKPOD_STATUS_OK;
}

InkpodStatus ResetShortcuts(
    app::CoreHost& engine,
    ShortcutUiState& state,
    bool persist) noexcept {
    const InkpodStatus status = engine.InvokeAll(
        [](InkpodCore* core) { return inkpod_core_shortcut_reset(core); }, false, false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    std::vector<InkpodShortcutSequence> bindings;
    const InkpodStatus copy_status = CopyCoreBindings(engine, bindings);
    if (copy_status != INKPOD_STATUS_OK) {
        return copy_status;
    }
    state.bindings = std::move(bindings);
    std::vector<InkpodShortcutSequence> defaults;
    try {
        defaults = BuildDefaultShortcutSequences();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus initializer_status = UpdateSessionInitializer(
        engine, defaults, state.bindings);
    if (initializer_status != INKPOD_STATUS_OK) {
        return initializer_status;
    }
    ClearPendingShortcut(state);
    if (persist && !SavePersistedShortcuts(state.bindings)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    return INKPOD_STATUS_OK;
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
    std::vector<InkpodShortcutSequence> candidate;
    try {
        candidate = state.bindings;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto target = std::find_if(candidate.begin(), candidate.end(), [&replacement](const auto& item) {
        return item.command_id == replacement.command_id;
    });
    if (target == candidate.end()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodShortcutSequence previous = *target;
    auto conflict = candidate.end();
    for (auto item = candidate.begin(); item != candidate.end(); ++item) {
        if (item->command_id == replacement.command_id || !Conflicts(*item, replacement)) {
            continue;
        }
        if (conflict != candidate.end() || !SameSequence(*item, replacement)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        conflict = item;
    }
    *target = replacement;
    target->struct_size = sizeof(InkpodShortcutSequence);
    if (conflict != candidate.end()) {
        const UINT conflict_command = conflict->command_id;
        *conflict = previous;
        conflict->command_id = conflict_command;
    }
    const InkpodStatus status = SetCoreBindings(engine, candidate, false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.bindings = std::move(candidate);
    std::vector<InkpodShortcutSequence> defaults;
    try {
        defaults = BuildDefaultShortcutSequences();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus initializer_status = UpdateSessionInitializer(
        engine, defaults, state.bindings);
    if (initializer_status != INKPOD_STATUS_OK) {
        return initializer_status;
    }
    ClearPendingShortcut(state);
    if (persist && !SavePersistedShortcuts(state.bindings)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    return INKPOD_STATUS_OK;
}

InkpodShortcutMatch ResolveShortcutStroke(
    ShortcutUiState& state,
    InkpodShortcutStroke stroke,
    UINT& command) noexcept {
    command = 0U;
    const ULONGLONG now = GetTickCount64();
    if (state.pending_deadline != 0U && now > state.pending_deadline) {
        ClearPendingShortcut(state);
    }
    try {
        state.pending_strokes.push_back(stroke);
    } catch (const std::bad_alloc&) {
        ClearPendingShortcut(state);
        return INKPOD_SHORTCUT_MATCH_NONE;
    }
    InkpodShortcutMatch match = ResolvePending(state, command);
    if (match == INKPOD_SHORTCUT_MATCH_EXACT) {
        ClearPendingShortcut(state);
        return match;
    }
    if (match == INKPOD_SHORTCUT_MATCH_PREFIX) {
        state.pending_deadline = now + kSequenceTimeoutMilliseconds;
        UpdatePendingText(state);
        return match;
    }

    ClearPendingShortcut(state);
    try {
        state.pending_strokes.push_back(stroke);
    } catch (const std::bad_alloc&) {
        return INKPOD_SHORTCUT_MATCH_NONE;
    }
    match = ResolvePending(state, command);
    if (match == INKPOD_SHORTCUT_MATCH_PREFIX) {
        state.pending_deadline = now + kSequenceTimeoutMilliseconds;
        UpdatePendingText(state);
    } else {
        ClearPendingShortcut(state);
    }
    return match;
}

void ClearPendingShortcut(ShortcutUiState& state) noexcept {
    state.pending_strokes.clear();
    state.pending_deadline = 0U;
    state.pending_text.clear();
}

}  // namespace inkpod::windows::ui
