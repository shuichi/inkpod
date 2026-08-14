#include <windows.h>

#include <utility>

#include "application.h"
#include "launch_options.h"
#include "ui/localization.h"

int InkpodRunAbiSmoke();
int InkpodRunPortableSmoke();

int APIENTRY wWinMain(
    HINSTANCE instance,
    HINSTANCE,
    wchar_t*,
    int show_command) {
    using inkpod::windows::ui::UiLanguagePreference;
    using inkpod::windows::ui::UiStringId;
    using inkpod::windows::ui::UiText;
    UiLanguagePreference language_preference = UiLanguagePreference::System;
    (void)inkpod::windows::ui::LoadUiLanguagePreference(language_preference);
    if (!inkpod::windows::ui::InitializeUiLocalization(
            instance, language_preference)) {
        MessageBoxW(
            nullptr,
            L"UI language resources could not be initialized.",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        inkpod::windows::ui::ShutdownUiLocalization();
        return 8;
    }
    struct LocalizationLifetime final {
        ~LocalizationLifetime() {
            inkpod::windows::ui::ShutdownUiLocalization();
        }
    };
    [[maybe_unused]] LocalizationLifetime localization_lifetime;
    inkpod::app::LaunchOptions options{};
    const inkpod::app::LaunchParseStatus parse_status =
        inkpod::app::ParseProcessLaunchOptions(options);
    if (parse_status != inkpod::app::LaunchParseStatus::Ok) {
        MessageBoxW(
            nullptr,
            parse_status == inkpod::app::LaunchParseStatus::OutOfMemory
                ? UiText(UiStringId::Text0944)
                : UiText(UiStringId::Text0943),
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 9;
    }
    if (options.smoke_ui_language != inkpod::app::SmokeUiLanguage::System) {
        inkpod::windows::ui::ShutdownUiLocalization();
        const UiLanguagePreference override_preference =
            options.smoke_ui_language == inkpod::app::SmokeUiLanguage::Japanese
            ? UiLanguagePreference::Japanese
            : UiLanguagePreference::English;
        if (!inkpod::windows::ui::InitializeUiLocalization(
                instance, override_preference)) {
            return 8;
        }
    }
    if (options.mode == inkpod::app::LaunchMode::AbiSmoke) {
        return InkpodRunAbiSmoke();
    }
    if (options.mode == inkpod::app::LaunchMode::PortableSmoke) {
        return InkpodRunPortableSmoke();
    }
    const bool performance_smoke =
        options.mode == inkpod::app::LaunchMode::PerformanceSmoke;
    inkpod::app::ApplicationLaunch launch{
        instance,
        show_command,
        options.mode == inkpod::app::LaunchMode::ApplicationSmoke
            || performance_smoke,
        performance_smoke,
        options.open_in_new_workspace,
        std::move(options.document_paths)};
    return inkpod::app::Application(std::move(launch)).Run();
}
