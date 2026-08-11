#pragma once

#include <string>
#include <vector>

namespace inkpod::app {

enum class LaunchMode {
    Application,
    ApplicationSmoke,
    PerformanceSmoke,
    AbiSmoke,
    PortableSmoke,
};

enum class LaunchParseStatus {
    Ok,
    InvalidArguments,
    OutOfMemory,
};

struct LaunchOptions {
    LaunchMode mode{LaunchMode::Application};
    bool open_in_new_workspace{};
    std::vector<std::wstring> document_paths;
};

LaunchParseStatus ParseLaunchArguments(
    int argument_count,
    const wchar_t* const* arguments,
    LaunchOptions& output) noexcept;

LaunchParseStatus ParseProcessLaunchOptions(LaunchOptions& output) noexcept;

}  // namespace inkpod::app
