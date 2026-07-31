#pragma once

#include <functional>

#include "app/command_context.h"
#include "dialogs/effects_dialogs.h"
#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
struct AppLifetimeState;
struct EffectsUiState;
struct MainWindowHandles;
}

namespace inkpod::windows::ui {

class EffectsController final {
public:
    using Operation = std::function<InkpodStatus(InkpodCore*, InkpodTask*)>;

    EffectsController(
        app::AppLifetimeState& lifetime,
        app::MainWindowHandles& windows,
        HWND& progress,
        app::EffectsUiState& effects,
        app::CoreHost& engine) noexcept;

    InkpodStatus StartTask(
        const app::CommandContext& context,
        bool preview_prompt,
        Operation operation,
        UINT completion_message) noexcept;

    static bool QueryProgress(
        void* context, ProgressDialogInfo& output) noexcept;
    static void CancelProgress(void* context) noexcept;

private:
    app::AppLifetimeState& lifetime_;
    app::MainWindowHandles& windows_;
    HWND& progress_;
    app::EffectsUiState& effects_;
    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui
