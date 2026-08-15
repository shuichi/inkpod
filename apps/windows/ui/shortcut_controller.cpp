#include "ui/localization.h"

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
constexpr std::uint32_t kSettingsVersion = 2U;
constexpr ULONGLONG kSequenceTimeoutMilliseconds = 1'500U;

struct SettingsHeader {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t count;
    std::uint32_t reserved;
};

bool SameSequence(
    const InkpodShortcutSequenceV2& left,
    const InkpodShortcutSequenceV2& right) noexcept {
    if (left.stroke_count != right.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0; index < left.stroke_count; ++index) {
        if (left.strokes[index].key_kind != right.strokes[index].key_kind
            || left.strokes[index].key_value != right.strokes[index].key_value
            || left.strokes[index].modifiers != right.strokes[index].modifiers) {
            return false;
        }
    }
    return true;
}

bool StartsWith(
    const InkpodShortcutSequenceV2& sequence,
    const InkpodShortcutSequenceV2& prefix) noexcept {
    if (prefix.stroke_count > sequence.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0; index < prefix.stroke_count; ++index) {
        if (sequence.strokes[index].key_kind != prefix.strokes[index].key_kind
            || sequence.strokes[index].key_value != prefix.strokes[index].key_value
            || sequence.strokes[index].modifiers != prefix.strokes[index].modifiers) {
            return false;
        }
    }
    return true;
}

bool Conflicts(
    const InkpodShortcutSequenceV2& left,
    const InkpodShortcutSequenceV2& right) noexcept {
    return StartsWith(left, right) || StartsWith(right, left);
}

bool HasCompleteCatalog(std::span<const InkpodShortcutSequenceV2> bindings) noexcept {
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
    std::span<const InkpodShortcutSequenceV2> bindings,
    bool defaults) noexcept {
    return engine.InvokeAll(
        [bindings, defaults](InkpodCore* core) {
            return defaults
                ? inkpod_core_shortcut_defaults_set_v2(
                      core, bindings.data(), bindings.size(), sizeof(InkpodShortcutSequenceV2))
                : inkpod_core_shortcut_sequences_set_v2(
                      core, bindings.data(), bindings.size(), sizeof(InkpodShortcutSequenceV2));
        },
        false,
        false);
}

InkpodStatus UpdateSessionInitializer(
    app::CoreHost& engine,
    std::span<const InkpodShortcutSequenceV2> defaults,
    std::span<const InkpodShortcutSequenceV2> bindings) noexcept {
    try {
        std::vector<InkpodShortcutSequenceV2> default_copy(defaults.begin(), defaults.end());
        std::vector<InkpodShortcutSequenceV2> binding_copy(bindings.begin(), bindings.end());
        engine.SetSessionInitializer(
            [defaults = std::move(default_copy), current = std::move(binding_copy)](
                InkpodCore* core) {
                InkpodStatus status = inkpod_core_shortcut_defaults_set_v2(
                    core,
                    defaults.data(),
                    defaults.size(),
                    sizeof(InkpodShortcutSequenceV2));
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_shortcut_sequences_set_v2(
                        core,
                        current.data(),
                        current.size(),
                        sizeof(InkpodShortcutSequenceV2));
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
    std::vector<InkpodShortcutSequenceV2>& bindings) noexcept {
    std::uint64_t count{};
    InkpodStatus status = engine.Invoke(
        [&count](InkpodCore* core) {
            return inkpod_core_shortcut_sequences_copy_v2(core, nullptr, 0, 0, &count);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK
        || count > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())) {
        return status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status;
    }
    try {
        bindings.assign(static_cast<std::size_t>(count), InkpodShortcutSequenceV2{});
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return engine.Invoke(
        [&bindings, &count](InkpodCore* core) {
            return inkpod_core_shortcut_sequences_copy_v2(
                core,
                bindings.data(),
                bindings.size(),
                sizeof(InkpodShortcutSequenceV2),
                &count);
        },
        false,
        false);
}

bool LoadPersistedShortcuts(std::vector<InkpodShortcutSequenceV2>& bindings) noexcept {
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
        + static_cast<std::size_t>(header.count) * sizeof(InkpodShortcutSequenceV2);
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

bool SavePersistedShortcuts(std::span<const InkpodShortcutSequenceV2> bindings) noexcept {
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
    const InkpodStatus status = inkpod_shortcut_sequence_resolve_v2(
        state.bindings.data(),
        state.bindings.size(),
        sizeof(InkpodShortcutSequenceV2),
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
    InkpodShortcutSequenceV2 pending{};
    pending.struct_size = sizeof(pending);
    pending.stroke_count = static_cast<std::uint32_t>(state.pending_strokes.size());
    std::copy(state.pending_strokes.begin(), state.pending_strokes.end(), pending.strokes);
    try {
        state.pending_text = UiText(UiStringId::Text0198) + FormatShortcutSequence(pending) + L", …";
    } catch (const std::bad_alloc&) {
        state.pending_text.clear();
    }
}

}  // namespace

InkpodStatus InitializeShortcuts(
    app::CoreHost& engine,
    ShortcutUiState& state,
    bool load_persisted) noexcept {
    std::vector<InkpodShortcutSequenceV2> defaults;
    std::vector<InkpodShortcutSequenceV2> catalog_defaults;
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
        std::vector<InkpodShortcutSequenceV2> persisted;
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
    std::vector<InkpodShortcutSequenceV2> bindings;
    const InkpodStatus copy_status = CopyCoreBindings(engine, bindings);
    if (copy_status != INKPOD_STATUS_OK) {
        return copy_status;
    }
    state.bindings = std::move(bindings);
    std::vector<InkpodShortcutSequenceV2> defaults;
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
    const InkpodShortcutSequenceV2& replacement,
    bool persist) noexcept {
    if (replacement.command_id == 0U || replacement.stroke_count == 0U
        || replacement.stroke_count > INKPOD_SHORTCUT_MAX_STROKES) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<InkpodShortcutSequenceV2> candidate;
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
    const InkpodShortcutSequenceV2 previous = *target;
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
    target->struct_size = sizeof(InkpodShortcutSequenceV2);
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
    std::vector<InkpodShortcutSequenceV2> defaults;
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
    InkpodShortcutStrokeV2 stroke,
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

bool NormalizeWindowsShortcutStroke(
    std::uint32_t virtual_key,
    std::uint32_t modifiers,
    InkpodShortcutStrokeV2& output) noexcept {
    if (virtual_key == 0U
        || (modifiers
            & ~(INKPOD_SHORTCUT_MODIFIER_PRIMARY
                | INKPOD_SHORTCUT_MODIFIER_SHIFT
                | INKPOD_SHORTCUT_MODIFIER_ALTERNATE
                | INKPOD_SHORTCUT_MODIFIER_CONTROL)) != 0U) {
        return false;
    }
    output = {};
    output.struct_size = sizeof(output);
    output.modifiers = modifiers;
    output.key_kind = INKPOD_SHORTCUT_KEY_NAMED;
    switch (virtual_key) {
        case VK_TAB: output.key_value = INKPOD_SHORTCUT_NAMED_TAB; break;
        case VK_RETURN: output.key_value = INKPOD_SHORTCUT_NAMED_RETURN; break;
        case VK_ESCAPE: output.key_value = INKPOD_SHORTCUT_NAMED_ESCAPE; break;
        case VK_SPACE: output.key_value = INKPOD_SHORTCUT_NAMED_SPACE; break;
        case VK_BACK: output.key_value = INKPOD_SHORTCUT_NAMED_BACKSPACE; break;
        case VK_DELETE: output.key_value = INKPOD_SHORTCUT_NAMED_DELETE; break;
        case VK_LEFT: output.key_value = INKPOD_SHORTCUT_NAMED_LEFT; break;
        case VK_RIGHT: output.key_value = INKPOD_SHORTCUT_NAMED_RIGHT; break;
        case VK_UP: output.key_value = INKPOD_SHORTCUT_NAMED_UP; break;
        case VK_DOWN: output.key_value = INKPOD_SHORTCUT_NAMED_DOWN; break;
        case VK_HOME: output.key_value = INKPOD_SHORTCUT_NAMED_HOME; break;
        case VK_END: output.key_value = INKPOD_SHORTCUT_NAMED_END; break;
        case VK_PRIOR: output.key_value = INKPOD_SHORTCUT_NAMED_PAGE_UP; break;
        case VK_NEXT: output.key_value = INKPOD_SHORTCUT_NAMED_PAGE_DOWN; break;
        default:
            if (virtual_key >= VK_F1 && virtual_key <= VK_F24) {
                output.key_value = INKPOD_SHORTCUT_NAMED_F1 + virtual_key - VK_F1;
                break;
            }
            output.key_kind = INKPOD_SHORTCUT_KEY_UNICODE_SCALAR;
            output.key_value = MapVirtualKeyW(virtual_key, MAPVK_VK_TO_CHAR) & 0x7fff'ffffU;
            if (output.key_value == 0U || output.key_value > 0x10ffffU
                || (output.key_value >= 0xd800U && output.key_value <= 0xdfffU)) {
                output = {};
                return false;
            }
            break;
    }
    return true;
}

void ClearPendingShortcut(ShortcutUiState& state) noexcept {
    state.pending_strokes.clear();
    state.pending_deadline = 0U;
    state.pending_text.clear();
}

}  // namespace inkpod::windows::ui
