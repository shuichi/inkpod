#include "app/application_data_paths.h"
#include "app/application_settings.h"

#include <windows.h>

#include <filesystem>
#include <string>

#include "app/resource.h"
#include "ui/command_catalog.h"
#include "ui/shortcut_profile.h"

namespace {

using inkpod::app::ApplicationSettings;
using inkpod::app::DecodeApplicationSettingsJson;
using inkpod::app::EncodeApplicationSettingsJson;
using inkpod::app::PersistedWorkspace;
using inkpod::windows::ui::BuildShortcutProfileFromLegacy;
using inkpod::windows::ui::ShortcutAction;
using inkpod::windows::ui::ShortcutContext;
using inkpod::windows::ui::ShortcutKeyMatch;
using inkpod::windows::ui::ShortcutProfile;
using inkpod::windows::ui::ShortcutProfileBinding;
using inkpod::windows::ui::ShortcutProfileSet;
using inkpod::windows::ui::ShortcutSlot;

ShortcutProfileSet Defaults() {
    ShortcutProfileSet result{};
    result.profiles.push_back(BuildShortcutProfileFromLegacy(
        L"Built-in", true, inkpod::windows::ui::BuildDefaultShortcutSequences()));
    return result;
}

ApplicationSettings Sample() {
    ApplicationSettings result{};
    result.ui_language = inkpod::windows::ui::UiLanguagePreference::Japanese;
    result.restore_previous_documents = true;
    result.sequence_switch_policy =
        inkpod::app::SequenceCellSwitchPolicy::AutosaveBeforeSwitch;
    result.sequence_endpoint_policy = inkpod::app::SequenceEndpointPolicy::Wrap;
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

}  // namespace

int wmain() {
    const ShortcutProfileSet defaults = Defaults();
    const ApplicationSettings sample = Sample();
    std::string json;
    if (!EncodeApplicationSettingsJson(sample, json)) {
        return 1;
    }
    if (!Contains(json, "\"format\": \"inkpod-settings\"")
        || !Contains(json, "\"formatVersion\": 1")
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
        || decoded.sequence_switch_policy != sample.sequence_switch_policy
        || decoded.sequence_endpoint_policy != sample.sequence_endpoint_policy
        || decoded.shortcuts.profiles.size() != 2U
        || decoded.shortcuts.profiles[1].name != L"My shortcuts"
        || decoded.shortcuts.profiles[1].bindings.size() != 1U
        || decoded.shortcuts.profiles[1].bindings[0] !=
            sample.shortcuts.profiles[1].bindings[0]
        || decoded.workspaces.size() != 1U
        || decoded.workspaces[0].slot != 0U
        || decoded.workspaces[0].layout.layer_split_milli != 625U) {
        return 3;
    }

    const std::string wrong_version =
        "{\"format\":\"inkpod-settings\",\"formatVersion\":2} ";
    const std::string duplicate =
        "{\"format\":\"inkpod-settings\",\"format\":\"inkpod-settings\","
        "\"formatVersion\":1}";
    const std::string unknown =
        "{\"format\":\"inkpod-settings\",\"formatVersion\":1,"
        "\"mystery\":true}";
    if (DecodeApplicationSettingsJson(wrong_version, defaults, decoded)
        || DecodeApplicationSettingsJson(duplicate, defaults, decoded)
        || DecodeApplicationSettingsJson(unknown, defaults, decoded)) {
        return 4;
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
    const HANDLE invalid_file = CreateFileW(
        file.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        TRUNCATE_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    constexpr char invalid_json[] = "{\"format\":\"inkpod-settings\"}";
    DWORD written{};
    if (invalid_file == INVALID_HANDLE_VALUE) {
        std::filesystem::remove_all(directory, error);
        return 10;
    }
    const bool invalid_written = WriteFile(
        invalid_file,
        invalid_json,
        static_cast<DWORD>(sizeof(invalid_json) - 1U),
        &written,
        nullptr) != FALSE;
    const bool invalid_closed = CloseHandle(invalid_file) != FALSE;
    if (!invalid_written || written != sizeof(invalid_json) - 1U
        || !invalid_closed) {
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
        || loaded.sequence_switch_policy
            != inkpod::app::SequenceCellSwitchPolicy::Prompt
        || loaded.sequence_endpoint_policy
            != inkpod::app::SequenceEndpointPolicy::Stop
        || loaded.shortcuts.profiles.size() != defaults.profiles.size()
        || !loaded.workspaces.empty() || !loaded.saved_workspaces.empty()) {
        std::filesystem::remove_all(directory, error);
        return 11;
    }
    std::filesystem::remove_all(directory, error);
    if (error) {
        return 12;
    }
    return 0;
}
