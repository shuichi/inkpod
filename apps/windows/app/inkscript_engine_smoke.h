#pragma once

#include "ui/job_progress.h"

namespace inkpod::app {

class ApplicationHost;

// Exercises the private production CoreHost route through the exact-current
// parser, catalog, planner, executor, fragment exporter, and native writer.
// No product command, file filter, or pane is registered by this smoke hook.
int RunPrivateInkScriptEngineSmoke(
    HWND status_bar, windows::ui::JobProgressState& progress_state) noexcept;
int RunPrivateInkScriptEngineSmoke() noexcept;
int RunPrivateInkScriptPublicationSmoke(ApplicationHost& application) noexcept;

}  // namespace inkpod::app
