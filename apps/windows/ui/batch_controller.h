#pragma once

#include <string>

#include "app/command_context.h"
#include "dialogs/effects_dialogs.h"
#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreEngine;
struct AppLifetimeState;
struct BatchUiState;
struct MainWindowHandles;
}

namespace inkpod::windows::ui {

class BatchController final {
public:
    BatchController(
        app::AppLifetimeState& lifetime,
        app::MainWindowHandles& windows,
        HWND& progress,
        HWND& palette,
        app::BatchUiState& batch,
        app::CoreEngine& engine) noexcept;

    InkpodStatus Preview(InkpodBatchRunScope scope) noexcept;
    InkpodStatus Start(
        const app::CommandContext& context,
        InkpodBatchRunScope scope,
        bool dry_run,
        UINT completion_message) noexcept;
    InkpodStatus SaveGraph(
        const std::uint8_t* path_utf8, std::size_t path_bytes) noexcept;
    InkpodStatus LoadGraph(
        const std::uint8_t* path_utf8, std::size_t path_bytes) noexcept;

    static bool QueryProgress(
        void* context, ProgressDialogInfo& output) noexcept;
    static void CancelProgress(void* context) noexcept;
    static void RefreshPalette(app::BatchUiState& batch, HWND palette) noexcept;
    static void ResetDerivedState(app::BatchUiState& batch) noexcept;
    static std::wstring ReportSummary(const InkpodBatchReport* report);
    static bool ChooseFolder(HWND owner, std::wstring& selected_path) noexcept;

private:
    InkpodStatus BuildGraph() noexcept;

    app::AppLifetimeState& lifetime_;
    app::MainWindowHandles& windows_;
    HWND& progress_;
    HWND& palette_;
    app::BatchUiState& batch_;
    app::CoreEngine& engine_;
};

} // namespace inkpod::windows::ui
