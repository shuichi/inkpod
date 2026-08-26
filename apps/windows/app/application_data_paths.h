#pragma once

#include <string>

namespace inkpod::app {

enum class ApplicationDataDirectory {
    Root,
    Settings,
    Session,
    Recovery,
    BatchSets,
    Cache,
    Logs,
};

[[nodiscard]] bool ResolveApplicationDataDirectory(
    ApplicationDataDirectory directory,
    std::wstring& output) noexcept;

[[nodiscard]] bool EnsureApplicationDataDirectory(
    ApplicationDataDirectory directory,
    std::wstring& output) noexcept;

[[nodiscard]] bool ResolveApplicationSettingsPath(
    std::wstring& output) noexcept;

[[nodiscard]] bool ResolveApplicationSessionPath(
    std::wstring& output) noexcept;

}  // namespace inkpod::app
