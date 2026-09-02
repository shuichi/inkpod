#include "app/application_data_paths.h"
#include "app/application_settings.h"

#include <windows.h>

#include <algorithm>
#include <array>
#include <cstdint>
#include <filesystem>
#include <limits>
#include <span>
#include <string>
#include <string_view>

#include "app/resource.h"
#include "ui/command_catalog.h"
#include "ui/shortcut_profile.h"

namespace {

using inkpod::app::ApplicationSettings;
using inkpod::app::DecodeApplicationSettingsJson;
using inkpod::app::EncodeApplicationSettingsJson;
using inkpod::app::PersistedWorkspace;
using inkpod::windows::ui::BuildDefaultShortcutProfile;
using inkpod::windows::ui::FindShortcutBinding;
using inkpod::windows::ui::ShortcutAction;
using inkpod::windows::ui::ShortcutContext;
using inkpod::windows::ui::ShortcutKeyMatch;
using inkpod::windows::ui::ShortcutProfile;
using inkpod::windows::ui::ShortcutProfileBinding;
using inkpod::windows::ui::ShortcutProfileSet;
using inkpod::windows::ui::ShortcutPhysicalKeyFromVirtualKey;
using inkpod::windows::ui::ShortcutSlot;

ShortcutProfileSet Defaults() {
    ShortcutProfileSet result{};
    result.profiles.push_back(BuildDefaultShortcutProfile(L"Built-in"));
    return result;
}

ApplicationSettings Sample() {
    ApplicationSettings result{};
    result.ui_language = inkpod::windows::ui::UiLanguagePreference::Japanese;
    result.restore_previous_documents = true;
    result.default_raster_format = inkpod::app::RasterFileFormatSetting::Tiff;
    result.sequence_switch_policy =
        inkpod::app::SequenceCellSwitchPolicy::AutosaveBeforeSwitch;
    result.sequence_endpoint_policy = inkpod::app::SequenceEndpointPolicy::Wrap;
    result.sequence_thumbnail_width_dip = 88U;
    result.validated_sidecar_cache_mib = 768U;
    result.shortcuts = Defaults();

    ShortcutProfile custom{};
    custom.name = L"My shortcuts";
    ShortcutProfileBinding binding{};
    binding.command_id = IDM_FILE_SAVE;
    binding.slot = ShortcutSlot::Primary;
    binding.context = ShortcutContext::Global;
    binding.action = ShortcutAction::Execute;
    binding.key_match = ShortcutKeyMatch::Logical;
    binding.stroke_count = 1U;
    binding.strokes[0].logical_key = 'S';
    binding.strokes[0].physical_key = 0x1fU;
    binding.strokes[0].modifiers = INKPOD_SHORTCUT_MODIFIER_CONTROL;
    custom.bindings.push_back(binding);
    ShortcutProfileBinding legacy_custom{};
    legacy_custom.command_id = IDM_TOOL_PENCIL;
    legacy_custom.slot = ShortcutSlot::Primary;
    legacy_custom.context = ShortcutContext::Canvas;
    legacy_custom.action = ShortcutAction::Execute;
    legacy_custom.key_match = ShortcutKeyMatch::Logical;
    constexpr std::array<std::uint32_t, 3U> keys{'Q', 'K', 'A'};
    legacy_custom.stroke_count = static_cast<std::uint32_t>(keys.size());
    for (std::size_t index = 0U; index < keys.size(); ++index) {
        legacy_custom.strokes[index].logical_key = keys[index];
        legacy_custom.strokes[index].physical_key =
            ShortcutPhysicalKeyFromVirtualKey(keys[index], 0U);
    }
    custom.bindings.push_back(legacy_custom);
    result.shortcuts.profiles.push_back(std::move(custom));
    result.shortcuts.active_profile = 1U;

    PersistedWorkspace workspace{};
    workspace.slot = 0U;
    workspace.layout.selected_preset =
        inkpod::windows::ui::WorkspacePreset::Custom;
    workspace.layout.density = inkpod::windows::ui::WorkspaceDensity::Compact;
    workspace.layout.layer_split_milli = 625U;
    workspace.layout.window = {10, 20, 1200, 800, 144U, true, SW_SHOWMAXIMIZED};
    result.workspaces.push_back(std::move(workspace));
    return result;
}

bool Contains(const std::string& text, const char* needle) {
    return text.find(needle) != std::string::npos;
}

bool WriteTextFile(
    const std::filesystem::path& path, std::string_view text) noexcept {
    const HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        CREATE_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    std::size_t cursor{};
    bool success = true;
    while (cursor < text.size()) {
        const DWORD requested = static_cast<DWORD>(std::min<std::size_t>(
            text.size() - cursor, std::numeric_limits<DWORD>::max()));
        DWORD written{};
        if (WriteFile(
                file, text.data() + cursor, requested, &written, nullptr) == FALSE
            || written == 0U) {
            success = false;
            break;
        }
        cursor += written;
    }
    return CloseHandle(file) != FALSE && success;
}

}  // namespace

int wmain() {
    const ShortcutProfileSet defaults = Defaults();
    const ShortcutProfileBinding* redo_alias = FindShortcutBinding(
        std::span<const ShortcutProfileBinding>(defaults.profiles.front().bindings),
        IDM_EDIT_REDO,
        ShortcutSlot::Secondary);
    if (defaults.profiles.front().bindings.size() != 33U
        || redo_alias == nullptr || redo_alias->stroke_count != 1U
        || redo_alias->strokes[0].logical_key != static_cast<std::uint32_t>('Z')
        || redo_alias->strokes[0].modifiers
            != (INKPOD_SHORTCUT_MODIFIER_CONTROL
                | INKPOD_SHORTCUT_MODIFIER_SHIFT)) {
        return 16;
    }
    const ApplicationSettings sample = Sample();
    std::string json;
    if (!EncodeApplicationSettingsJson(sample, json)) {
        return 1;
    }
    if (!Contains(json, "\"format\": \"inkpod-settings\"")
        || !Contains(json, "\"formatVersion\": 5")
        || !Contains(json, "\"sequenceThumbnailWidthDip\": 88")
        || !Contains(json, "\"validatedSidecarCacheMiB\": 768")
        || !Contains(json, "\"defaultRasterFormat\": \"tiff\"")
        || !Contains(json, "\"command\": \"file.save\"")
        || !Contains(json, "\"logicalKey\": \"S\"")
        || !Contains(json, "\"physicalKey\": \"KeyS\"")
        || Contains(json, "base64") || Contains(json, "IDM_FILE_SAVE")
        || json.empty() || json.back() != '\n') {
        return 2;
    }

    ApplicationSettings decoded{};
    if (!DecodeApplicationSettingsJson(json, defaults, decoded)
        || decoded.ui_language != sample.ui_language
        || decoded.restore_previous_documents
            != sample.restore_previous_documents
        || decoded.default_raster_format != sample.default_raster_format
        || decoded.sequence_switch_policy != sample.sequence_switch_policy
        || decoded.sequence_endpoint_policy != sample.sequence_endpoint_policy
        || decoded.sequence_thumbnail_width_dip
            != sample.sequence_thumbnail_width_dip
        || decoded.validated_sidecar_cache_mib
            != sample.validated_sidecar_cache_mib
        || decoded.shortcuts.profiles.size() != 2U
        || decoded.shortcuts.profiles[0] != defaults.profiles[0]
        || decoded.shortcuts.profiles[1].name != L"My shortcuts"
        || decoded.shortcuts.profiles[1].bindings.size() != 2U
        || decoded.shortcuts.profiles[1].bindings[0] !=
            sample.shortcuts.profiles[1].bindings[0]
        || decoded.shortcuts.profiles[1].bindings[1] !=
            sample.shortcuts.profiles[1].bindings[1]
        || decoded.workspaces.size() != 1U
        || decoded.workspaces[0].slot != 0U
        || decoded.workspaces[0].layout.layer_split_milli != 625U) {
        return 3;
    }

    const std::string wrong_version =
        "{\"format\":\"inkpod-settings\",\"formatVersion\":4} ";
    const std::string duplicate =
        "{\"format\":\"inkpod-settings\",\"format\":\"inkpod-settings\","
        "\"formatVersion\":5}";
    const std::string unknown =
        "{\"format\":\"inkpod-settings\",\"formatVersion\":5,"
        "\"mystery\":true}";
    if (DecodeApplicationSettingsJson(wrong_version, defaults, decoded)
        || DecodeApplicationSettingsJson(duplicate, defaults, decoded)
        || DecodeApplicationSettingsJson(unknown, defaults, decoded)) {
        return 4;
    }

    const std::string invalid_raster_format =
        "{\"format\":\"inkpod-settings\",\"formatVersion\":5,"
        "\"saveAndRecovery\":{\"restorePreviousDocuments\":false,"
        "\"defaultRasterFormat\":\"jpeg\"}}";
    if (DecodeApplicationSettingsJson(invalid_raster_format, defaults, decoded)) {
        return 13;
    }
    for (const auto format : {
             inkpod::app::RasterFileFormatSetting::Png,
             inkpod::app::RasterFileFormatSetting::Tiff,
             inkpod::app::RasterFileFormatSetting::Tga,
             inkpod::app::RasterFileFormatSetting::Bmp}) {
        ApplicationSettings candidate = sample;
        candidate.default_raster_format = format;
        std::string encoded;
        if (!EncodeApplicationSettingsJson(candidate, encoded)
            || !DecodeApplicationSettingsJson(encoded, defaults, decoded)
            || decoded.default_raster_format != format) {
            return 14;
        }
    }
    if (!DecodeApplicationSettingsJson(
            "{\"format\":\"inkpod-settings\",\"formatVersion\":5}",
            defaults,
            decoded)
        || decoded.default_raster_format != inkpod::app::RasterFileFormatSetting::Png
        || decoded.sequence_thumbnail_width_dip
            != inkpod::app::kDefaultSequenceThumbnailWidthDip
        || decoded.validated_sidecar_cache_mib
            != inkpod::app::kDefaultValidatedSidecarCacheMiB) {
        return 15;
    }

    for (const std::uint32_t width : {
             inkpod::app::kMinimumSequenceThumbnailWidthDip,
             inkpod::app::kDefaultSequenceThumbnailWidthDip,
             inkpod::app::kMaximumSequenceThumbnailWidthDip}) {
        ApplicationSettings candidate = sample;
        candidate.sequence_thumbnail_width_dip = width;
        std::string encoded;
        if (!EncodeApplicationSettingsJson(candidate, encoded)
            || !DecodeApplicationSettingsJson(encoded, defaults, decoded)
            || decoded.sequence_thumbnail_width_dip != width) {
            return 17;
        }
    }
    for (const char* width : {"31", "97"}) {
        const std::string invalid_width =
            "{\"format\":\"inkpod-settings\",\"formatVersion\":5,"
            "\"animation\":{\"sequenceCellSwitch\":\"prompt\","
            "\"sequenceEndpoint\":\"stop\","
            "\"sequenceThumbnailWidthDip\":" + std::string(width) + ","
            "\"validatedSidecarCacheMiB\":256}}";
        if (DecodeApplicationSettingsJson(invalid_width, defaults, decoded)) {
            return 19;
        }
    }

    for (const std::uint32_t maximum_mib : {0U, 256U, 1024U}) {
        ApplicationSettings candidate = sample;
        candidate.validated_sidecar_cache_mib = maximum_mib;
        std::string encoded;
        if (!EncodeApplicationSettingsJson(candidate, encoded)
            || !DecodeApplicationSettingsJson(encoded, defaults, decoded)
            || decoded.validated_sidecar_cache_mib != maximum_mib) {
            return 20;
        }
    }
    {
        ApplicationSettings candidate = sample;
        candidate.validated_sidecar_cache_mib = 1025U;
        std::string encoded;
        if (EncodeApplicationSettingsJson(candidate, encoded)) {
            return 21;
        }
    }
    for (const std::uint32_t width : {
             inkpod::app::kMinimumSequenceThumbnailWidthDip - 1U,
             inkpod::app::kMaximumSequenceThumbnailWidthDip + 1U}) {
        ApplicationSettings candidate = sample;
        candidate.sequence_thumbnail_width_dip = width;
        std::string encoded;
        if (EncodeApplicationSettingsJson(candidate, encoded)) {
            return 18;
        }
    }

    std::wstring settings_path;
    if (!inkpod::app::ResolveApplicationSettingsPath(settings_path)
        || settings_path.size() < 45U
        || settings_path.ends_with(L"\\inkpod\\Settings\\inkpod-settings.json")
            == false) {
        return 5;
    }

    wchar_t temporary_root[MAX_PATH]{};
    if (GetTempPathW(MAX_PATH, temporary_root) == 0U) {
        return 6;
    }
    const std::filesystem::path directory =
        std::filesystem::path(temporary_root)
        / (L"inkpod-settings-test-" + std::to_wstring(GetCurrentProcessId()));
    std::error_code error;
    std::filesystem::remove_all(directory, error);
    error.clear();
    if (!std::filesystem::create_directories(directory, error) || error) {
        return 7;
    }
    const std::filesystem::path file = directory / L"inkpod-settings.json";
    if (!inkpod::app::SaveApplicationSettingsFile(file.wstring(), sample)) {
        std::filesystem::remove_all(directory, error);
        return 8;
    }
    ApplicationSettings loaded{};
    std::string loaded_json;
    if (inkpod::app::LoadApplicationSettingsFile(
            file.wstring(), defaults, loaded)
            != inkpod::app::ApplicationSettingsLoadResult::Loaded
        || !EncodeApplicationSettingsJson(loaded, loaded_json)
        || loaded_json != json) {
        std::filesystem::remove_all(directory, error);
        return 9;
    }
    constexpr std::string_view invalid_json =
        "{\"format\":\"inkpod-settings\"}";
    if (!WriteTextFile(file, invalid_json)) {
        std::filesystem::remove_all(directory, error);
        return 10;
    }
    loaded = sample;
    if (inkpod::app::LoadApplicationSettingsFile(
            file.wstring(), defaults, loaded)
            != inkpod::app::ApplicationSettingsLoadResult::Invalid
        || loaded.ui_language
            != inkpod::windows::ui::UiLanguagePreference::System
        || loaded.restore_previous_documents
        || loaded.default_raster_format != inkpod::app::RasterFileFormatSetting::Png
        || loaded.sequence_switch_policy
            != inkpod::app::SequenceCellSwitchPolicy::Prompt
        || loaded.sequence_endpoint_policy
            != inkpod::app::SequenceEndpointPolicy::Stop
        || loaded.validated_sidecar_cache_mib
            != inkpod::app::kDefaultValidatedSidecarCacheMiB
        || loaded.shortcuts.profiles.size() != defaults.profiles.size()
        || !loaded.workspaces.empty() || !loaded.saved_workspaces.empty()
        || GetFileAttributesW(file.c_str()) == INVALID_FILE_ATTRIBUTES) {
        std::filesystem::remove_all(directory, error);
        return 11;
    }

    for (std::uint32_t version = 1U;
         version < inkpod::app::kApplicationSettingsFormatVersion;
         ++version) {
        const std::string outdated =
            "{\"format\":\"inkpod-settings\",\"formatVersion\":"
            + std::to_string(version) + "}";
        if (!WriteTextFile(file, outdated)) {
            std::filesystem::remove_all(directory, error);
            return 20;
        }
        loaded = sample;
        SetLastError(ERROR_SUCCESS);
        const auto result = inkpod::app::LoadApplicationSettingsFile(
            file.wstring(), defaults, loaded);
        const DWORD attributes = GetFileAttributesW(file.c_str());
        const DWORD delete_error = GetLastError();
        if (result != inkpod::app::ApplicationSettingsLoadResult::Missing
            || attributes != INVALID_FILE_ATTRIBUTES
            || (delete_error != ERROR_FILE_NOT_FOUND
                && delete_error != ERROR_PATH_NOT_FOUND)
            || loaded.ui_language
                != inkpod::windows::ui::UiLanguagePreference::System
            || loaded.sequence_thumbnail_width_dip
                != inkpod::app::kDefaultSequenceThumbnailWidthDip
            || loaded.validated_sidecar_cache_mib
                != inkpod::app::kDefaultValidatedSidecarCacheMiB
            || loaded.shortcuts.profiles.size() != defaults.profiles.size()) {
            std::filesystem::remove_all(directory, error);
            return 21;
        }
    }

    const std::array retained_noncurrent{
        std::string_view{
            "{\"format\":\"inkpod-settings\",\"formatVersion\":6}"},
        std::string_view{
            "{\"format\":\"inkpod-settings\",\"formatVersion\":0}"},
        std::string_view{
            "{\"format\":\"inkpod-settings\",\"formatVersion\":1,"
            "\"formatVersion\":1}"},
        std::string_view{
            "{\"format\":\"other-settings\",\"formatVersion\":1}"},
        std::string_view{
            "{\"format\":\"inkpod-settings\",\"formatVersion\":5,"
            "\"mystery\":true}"},
    };
    for (const std::string_view retained : retained_noncurrent) {
        if (!WriteTextFile(file, retained)) {
            std::filesystem::remove_all(directory, error);
            return 22;
        }
        loaded = sample;
        if (inkpod::app::LoadApplicationSettingsFile(
                file.wstring(), defaults, loaded)
                != inkpod::app::ApplicationSettingsLoadResult::Invalid
            || GetFileAttributesW(file.c_str()) == INVALID_FILE_ATTRIBUTES) {
            std::filesystem::remove_all(directory, error);
            return 23;
        }
    }

    constexpr std::string_view locked_outdated =
        "{\"format\":\"inkpod-settings\",\"formatVersion\":1}";
    if (!WriteTextFile(file, locked_outdated)) {
        std::filesystem::remove_all(directory, error);
        return 24;
    }
    const HANDLE blocker = CreateFileW(
        file.c_str(),
        GENERIC_READ,
        FILE_SHARE_READ,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (blocker == INVALID_HANDLE_VALUE) {
        std::filesystem::remove_all(directory, error);
        return 24;
    }
    loaded = sample;
    const auto blocked_result = inkpod::app::LoadApplicationSettingsFile(
        file.wstring(), defaults, loaded);
    const bool blocked_file_retained =
        GetFileAttributesW(file.c_str()) != INVALID_FILE_ATTRIBUTES;
    const bool blocker_closed = CloseHandle(blocker) != FALSE;
    if (blocked_result != inkpod::app::ApplicationSettingsLoadResult::IoError
        || !blocked_file_retained || !blocker_closed) {
        std::filesystem::remove_all(directory, error);
        return 25;
    }
    std::filesystem::remove_all(directory, error);
    if (error) {
        return 12;
    }
    return 0;
}
