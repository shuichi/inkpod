#pragma once

#include <windows.h>

#include "app/session_recovery.h"
#include "ui/localization.h"
#include "ui/shortcut_profile.h"
#include "ui/workspace_layout.h"

namespace inkpod::windows::ui {

struct PreferencesValues final {
    UiLanguagePreference language{UiLanguagePreference::System};
    bool restore_previous_documents{};
    app::SequenceCellSwitchPolicy sequence_switch_policy{
        app::SequenceCellSwitchPolicy::Prompt};
    app::SequenceEndpointPolicy sequence_endpoint_policy{
        app::SequenceEndpointPolicy::Stop};
    app::OutputColorGuardProfileSetting color_profile{
        app::OutputColorGuardProfileSetting::Bt709ConservativeYcbcr};
    WorkspacePreset workspace_preset{WorkspacePreset::Coloring};
    WorkspaceDensity workspace_density{WorkspaceDensity::Standard};
    bool workspace_mirrored{};
    ShortcutProfileSet shortcuts;

    friend bool operator==(const PreferencesValues&, const PreferencesValues&) = default;
};

struct PreferencesDialogState final {
    using ApplyCallback = bool (*)(
        void* context, const PreferencesValues& values, HWND owner) noexcept;

    PreferencesValues values;
    void* apply_context{};
    ApplyCallback apply{};
    bool close_immediately{};
};

INT_PTR ShowPreferencesDialog(
    HINSTANCE instance,
    HWND owner,
    PreferencesDialogState& state) noexcept;

}  // namespace inkpod::windows::ui
