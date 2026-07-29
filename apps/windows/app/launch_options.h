#pragma once

#include <string>

namespace inkpod::app {

enum class LaunchMode {
    Application,
    ApplicationSmoke,
    AbiSmoke,
};

enum class LaunchParseStatus {
    Ok,
    InvalidArguments,
    OutOfMemory,
};

struct LaunchOptions {
    LaunchMode mode{LaunchMode::Application};
    std::wstring document_path;
};

LaunchParseStatus ParseLaunchArguments(
    int argument_count,
    const wchar_t* const* arguments,
    LaunchOptions& output) noexcept;

LaunchParseStatus ParseProcessLaunchOptions(LaunchOptions& output) noexcept;

}  // namespace inkpod::app
