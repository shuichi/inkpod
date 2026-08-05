#include "launch_options.h"

#include <windows.h>
#include <shellapi.h>

#include <cwchar>
#include <new>
#include <utility>

namespace inkpod::app {
namespace {

bool IsOption(const wchar_t* argument, const wchar_t* expected) noexcept {
    return argument != nullptr && wcscmp(argument, expected) == 0;
}

constexpr std::size_t kMaximumLaunchPaths = 64U;
constexpr std::size_t kMaximumLaunchCharacters = 1024U * 1024U;

}  // namespace

LaunchParseStatus ParseLaunchArguments(
    int argument_count,
    const wchar_t* const* arguments,
    LaunchOptions& output) noexcept {
    if (argument_count < 1 || arguments == nullptr || arguments[0] == nullptr) {
        return LaunchParseStatus::InvalidArguments;
    }

    LaunchOptions parsed{};
    bool options_ended{};
    bool has_mode{};
    bool has_new_workspace{};
    std::size_t total_path_characters{};
    try {
        for (int index = 1; index < argument_count; ++index) {
            const wchar_t* argument = arguments[index];
            if (argument == nullptr || argument[0] == L'\0') {
                return LaunchParseStatus::InvalidArguments;
            }
            if (!options_ended && IsOption(argument, L"--")) {
                options_ended = true;
                continue;
            }
            if (!options_ended && IsOption(argument, L"--abi-smoke-test")) {
                if (has_mode) {
                    return LaunchParseStatus::InvalidArguments;
                }
                parsed.mode = LaunchMode::AbiSmoke;
                has_mode = true;
                continue;
            }
            if (!options_ended && IsOption(argument, L"--smoke-test")) {
                if (has_mode) {
                    return LaunchParseStatus::InvalidArguments;
                }
                parsed.mode = LaunchMode::ApplicationSmoke;
                has_mode = true;
                continue;
            }
            if (!options_ended
                && IsOption(argument, L"--performance-smoke-test")) {
                if (has_mode) {
                    return LaunchParseStatus::InvalidArguments;
                }
                parsed.mode = LaunchMode::PerformanceSmoke;
                has_mode = true;
                continue;
            }
            if (!options_ended && IsOption(argument, L"--new-window")) {
                if (has_new_workspace) {
                    return LaunchParseStatus::InvalidArguments;
                }
                parsed.open_in_new_workspace = true;
                has_new_workspace = true;
                continue;
            }
            if (!options_ended && argument[0] == L'-') {
                return LaunchParseStatus::InvalidArguments;
            }
            const std::size_t path_characters = wcslen(argument);
            if (path_characters > 32767U
                || parsed.document_paths.size() >= kMaximumLaunchPaths
                || total_path_characters
                    > kMaximumLaunchCharacters - path_characters) {
                return LaunchParseStatus::InvalidArguments;
            }
            total_path_characters += path_characters;
            parsed.document_paths.emplace_back(argument);
        }
    } catch (const std::bad_alloc&) {
        return LaunchParseStatus::OutOfMemory;
    }

    if (has_mode
        && (!parsed.document_paths.empty() || parsed.open_in_new_workspace)) {
        return LaunchParseStatus::InvalidArguments;
    }
    output = std::move(parsed);
    return LaunchParseStatus::Ok;
}

LaunchParseStatus ParseProcessLaunchOptions(LaunchOptions& output) noexcept {
    int argument_count{};
    wchar_t** arguments = CommandLineToArgvW(GetCommandLineW(), &argument_count);
    if (arguments == nullptr) {
        return LaunchParseStatus::OutOfMemory;
    }
    const LaunchParseStatus status =
        ParseLaunchArguments(argument_count, arguments, output);
    LocalFree(arguments);
    return status;
}

}  // namespace inkpod::app
