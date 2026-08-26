#include "application_data_paths.h"

#include <windows.h>
#include <knownfolders.h>
#include <shlobj.h>

#include <new>
#include <string_view>

namespace inkpod::app {
namespace {

constexpr wchar_t kApplicationDirectoryName[] = L"inkpod";
constexpr wchar_t kSettingsDirectoryName[] = L"Settings";
constexpr wchar_t kSessionDirectoryName[] = L"Session";
constexpr wchar_t kRecoveryDirectoryName[] = L"Recovery";
constexpr wchar_t kBatchSetsDirectoryName[] = L"batch-sets";
constexpr wchar_t kCacheDirectoryName[] = L"Cache";
constexpr wchar_t kLogsDirectoryName[] = L"Logs";
constexpr wchar_t kSettingsFileName[] = L"inkpod-settings.json";
constexpr wchar_t kSessionFileName[] = L"inkpod-session.bin";

std::wstring_view DirectoryName(ApplicationDataDirectory directory) noexcept {
    switch (directory) {
    case ApplicationDataDirectory::Root:
        return {};
    case ApplicationDataDirectory::Settings:
        return kSettingsDirectoryName;
    case ApplicationDataDirectory::Session:
        return kSessionDirectoryName;
    case ApplicationDataDirectory::Recovery:
        return kRecoveryDirectoryName;
    case ApplicationDataDirectory::BatchSets:
        return kBatchSetsDirectoryName;
    case ApplicationDataDirectory::Cache:
        return kCacheDirectoryName;
    case ApplicationDataDirectory::Logs:
        return kLogsDirectoryName;
    }
    return {};
}

bool AppendPathComponent(
    std::wstring& path, std::wstring_view component) noexcept {
    try {
        if (!path.empty() && path.back() != L'\\') {
            path.push_back(L'\\');
        }
        path.append(component);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool EnsureDirectory(const std::wstring& path) noexcept {
    if (CreateDirectoryW(path.c_str(), nullptr) != FALSE) {
        return true;
    }
    return GetLastError() == ERROR_ALREADY_EXISTS
        && (GetFileAttributesW(path.c_str()) & FILE_ATTRIBUTE_DIRECTORY) != 0U;
}

}  // namespace

bool ResolveApplicationDataDirectory(
    ApplicationDataDirectory directory,
    std::wstring& output) noexcept {
    PWSTR local_app_data{};
    if (FAILED(SHGetKnownFolderPath(
            FOLDERID_LocalAppData, KF_FLAG_DEFAULT, nullptr, &local_app_data))
        || local_app_data == nullptr) {
        return false;
    }
    try {
        output.assign(local_app_data);
    } catch (const std::bad_alloc&) {
        CoTaskMemFree(local_app_data);
        return false;
    }
    CoTaskMemFree(local_app_data);
    if (!AppendPathComponent(output, kApplicationDirectoryName)) {
        return false;
    }
    const std::wstring_view child = DirectoryName(directory);
    return child.empty() || AppendPathComponent(output, child);
}

bool EnsureApplicationDataDirectory(
    ApplicationDataDirectory directory,
    std::wstring& output) noexcept {
    std::wstring root;
    if (!ResolveApplicationDataDirectory(ApplicationDataDirectory::Root, root)
        || !EnsureDirectory(root)
        || !ResolveApplicationDataDirectory(directory, output)) {
        return false;
    }
    return directory == ApplicationDataDirectory::Root
        || EnsureDirectory(output);
}

bool ResolveApplicationSettingsPath(std::wstring& output) noexcept {
    return ResolveApplicationDataDirectory(
               ApplicationDataDirectory::Settings, output)
        && AppendPathComponent(output, kSettingsFileName);
}

bool ResolveApplicationSessionPath(std::wstring& output) noexcept {
    return ResolveApplicationDataDirectory(
               ApplicationDataDirectory::Session, output)
        && AppendPathComponent(output, kSessionFileName);
}

}  // namespace inkpod::app
