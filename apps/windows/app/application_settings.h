#pragma once

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

#include "session_recovery.h"
#include "ui/localization.h"
#include "ui/shortcut_profile.h"
#include "ui/workspace_layout.h"

namespace inkpod::app {

inline constexpr std::uint32_t kApplicationSettingsFormatVersion = 1U;
inline constexpr std::size_t kMaximumApplicationSettingsBytes =
    16U * 1024U * 1024U;

struct PersistedWorkspace final {
    std::uint32_t slot{};
    windows::ui::WorkspaceLayoutState layout{};
};

struct ApplicationSettings final {
    windows::ui::UiLanguagePreference ui_language{
        windows::ui::UiLanguagePreference::System};
    bool restore_previous_documents{};
    SequenceCellSwitchPolicy sequence_switch_policy{
        SequenceCellSwitchPolicy::Prompt};
    SequenceEndpointPolicy sequence_endpoint_policy{
        SequenceEndpointPolicy::Stop};
    OutputColorGuardProfileSetting output_color_guard_profile{
        OutputColorGuardProfileSetting::Bt709ConservativeYcbcr};
    windows::ui::ShortcutProfileSet shortcuts;
    std::vector<PersistedWorkspace> workspaces;
    std::vector<PersistedWorkspace> saved_workspaces;
};

enum class ApplicationSettingsLoadResult : std::uint8_t {
    Loaded,
    Missing,
    Invalid,
    IoError,
};

enum class ShortcutPresetJsonResult : std::uint8_t {
    Ok,
    Invalid,
    UnsupportedVersion,
    CapacityExceeded,
};

[[nodiscard]] bool EncodeApplicationSettingsJson(
    const ApplicationSettings& settings,
    std::string& output) noexcept;

[[nodiscard]] bool DecodeApplicationSettingsJson(
    std::string_view input,
    const windows::ui::ShortcutProfileSet& defaults,
    ApplicationSettings& output) noexcept;

[[nodiscard]] ShortcutPresetJsonResult EncodeShortcutPresetJson(
    const windows::ui::ShortcutProfile& profile,
    std::string& output) noexcept;

[[nodiscard]] ShortcutPresetJsonResult DecodeShortcutPresetJson(
    std::string_view input,
    windows::ui::ShortcutProfile& output) noexcept;

[[nodiscard]] ApplicationSettingsLoadResult LoadApplicationSettingsFile(
    const std::wstring& path,
    const windows::ui::ShortcutProfileSet& defaults,
    ApplicationSettings& output) noexcept;

[[nodiscard]] bool SaveApplicationSettingsFile(
    const std::wstring& path,
    const ApplicationSettings& settings) noexcept;

[[nodiscard]] bool LoadApplicationUiLanguagePreference(
    windows::ui::UiLanguagePreference& preference) noexcept;

class ApplicationSettingsStore final {
public:
    [[nodiscard]] bool UseDefaults(
        const windows::ui::ShortcutProfileSet& defaults) noexcept;
    [[nodiscard]] bool ReplaceTransient(
        const ApplicationSettings& settings) noexcept;
    [[nodiscard]] ApplicationSettingsLoadResult Load(
        const windows::ui::ShortcutProfileSet& defaults) noexcept;
    [[nodiscard]] bool Save(const ApplicationSettings& settings) noexcept;
    [[nodiscard]] bool SaveAutomatic(
        const ApplicationSettings& settings) noexcept;

    [[nodiscard]] const ApplicationSettings& Values() const noexcept {
        return values_;
    }

    [[nodiscard]] const windows::ui::WorkspaceLayoutState* Workspace(
        std::uint32_t slot) const noexcept;

private:
    [[nodiscard]] bool SaveImpl(
        const ApplicationSettings& settings, bool replace_invalid) noexcept;

    ApplicationSettings values_{};
    bool invalid_file_loaded_{};
};

}  // namespace inkpod::app
