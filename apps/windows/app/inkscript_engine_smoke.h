#pragma once

namespace inkpod::app {

// Exercises the private production CoreHost route through the exact-current
// parser, catalog, planner, executor, fragment exporter, and native writer.
// No product command, file filter, or pane is registered by this smoke hook.
int RunPrivateInkScriptEngineSmoke() noexcept;

}  // namespace inkpod::app
