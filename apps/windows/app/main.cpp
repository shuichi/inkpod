#include <windows.h>

#include <utility>

#include "application.h"
#include "launch_options.h"

int InkpodRunAbiSmoke();

int APIENTRY wWinMain(
    HINSTANCE instance,
    HINSTANCE,
    wchar_t*,
    int show_command) {
    inkpod::app::LaunchOptions options{};
    const inkpod::app::LaunchParseStatus parse_status =
        inkpod::app::ParseProcessLaunchOptions(options);
    if (parse_status != inkpod::app::LaunchParseStatus::Ok) {
        MessageBoxW(
            nullptr,
            parse_status == inkpod::app::LaunchParseStatus::OutOfMemory
                ? L"起動引数を処理するメモリが不足しています。"
                : L"起動引数が正しくありません。開くファイルは64件以内で指定してください。",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 9;
    }
    if (options.mode == inkpod::app::LaunchMode::AbiSmoke) {
        return InkpodRunAbiSmoke();
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
