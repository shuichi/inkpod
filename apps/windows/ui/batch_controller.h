#pragma once

#include <string>

#include "app/command_context.h"
#include "dialogs/effects_dialogs.h"
#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
class FileIoController;
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
        HWND progress,
        JobProgressPaneState& progress_state,
        HWND& palette,
        app::BatchUiState& batch,
        app::CoreHost& engine,
        app::FileIoController& file_io) noexcept;

    InkpodStatus StartContactSheetPreview(
        const app::CommandContext& context,
        UINT completion_message) noexcept;
    InkpodStatus Start(
        const app::CommandContext& context,
        InkpodBatchRunScope scope,
        bool dry_run,
        UINT completion_message) noexcept;
    InkpodStatus SaveGraph(
        const std::uint8_t* path_utf8, std::size_t path_bytes) noexcept;
    InkpodStatus LoadGraph(
        const std::uint8_t* path_utf8, std::size_t path_bytes) noexcept;
    InkpodStatus SaveStoredGraph() noexcept;
    InkpodStatus LoadStoredGraph() noexcept;

    static bool QueryProgress(
        void* context, ProgressDialogInfo& output) noexcept;
    static void CancelProgress(void* context) noexcept;
    static void RefreshPalette(app::BatchUiState& batch, HWND palette) noexcept;
    static bool RefreshSetCatalog(app::BatchUiState& batch) noexcept;
    static void ResetDerivedState(app::BatchUiState& batch) noexcept;
    static std::wstring ReportSummary(const InkpodBatchReport* report);
    static bool ChooseFolder(HWND owner, std::wstring& selected_path) noexcept;

private:
    InkpodStatus BuildGraph() noexcept;
    InkpodStatus StartIo(
        const app::CommandContext& context,
        InkpodBatchRunScope scope,
        bool dry_run,
        bool contact_sheet,
        UINT completion_message) noexcept;

    app::AppLifetimeState& lifetime_;
    app::MainWindowHandles& windows_;
    HWND progress_{};
    JobProgressPaneState& progress_state_;
    HWND& palette_;
    app::BatchUiState& batch_;
    app::CoreHost& engine_;
    app::FileIoController& file_io_;
};

} // namespace inkpod::windows::ui
