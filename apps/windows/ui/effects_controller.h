#pragma once

#include <functional>

#include "dialogs/effects_dialogs.h"
#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreEngine;
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
        app::EffectsUiState& effects,
        app::CoreEngine& engine) noexcept;

    InkpodStatus StartTask(
        bool preview_prompt,
        Operation operation,
        UINT completion_message) noexcept;

    static bool QueryProgress(
        void* context, ProgressDialogInfo& output) noexcept;
    static void CancelProgress(void* context) noexcept;

private:
    app::AppLifetimeState& lifetime_;
    app::MainWindowHandles& windows_;
    app::EffectsUiState& effects_;
    app::CoreEngine& engine_;
};

} // namespace inkpod::windows::ui
