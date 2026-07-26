#include <windows.h>

#include <cwchar>

#include "application.h"

int InkpodRunAbiSmoke();

namespace {

enum class LaunchMode {
    Application,
    ApplicationSmoke,
    AbiSmoke,
};

LaunchMode ParseLaunchMode(const wchar_t* command_line) noexcept {
    if (command_line != nullptr
        && std::wcsstr(command_line, L"--abi-smoke-test") != nullptr) {
        return LaunchMode::AbiSmoke;
    }
    if (command_line != nullptr
        && std::wcsstr(command_line, L"--smoke-test") != nullptr) {
        return LaunchMode::ApplicationSmoke;
    }
    return LaunchMode::Application;
}

}  // namespace

int APIENTRY wWinMain(
    HINSTANCE instance,
    HINSTANCE,
    wchar_t* command_line,
    int show_command) {
    const LaunchMode launch_mode = ParseLaunchMode(command_line);
    if (launch_mode == LaunchMode::AbiSmoke) {
        return InkpodRunAbiSmoke();
    }
    return inkpod::app::Application({
        instance,
        show_command,
        launch_mode == LaunchMode::ApplicationSmoke})
        .Run();
}
