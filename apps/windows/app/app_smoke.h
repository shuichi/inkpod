#pragma once

namespace inkpod::app {
struct AppContext;
}

namespace inkpod::windows::ui {

// Runs the M1-M7 regression scenarios through the packaged executable's real
// window, Core-engine, snapshot-queue, and renderer paths.
int RunApplicationSmoke(app::AppContext& state) noexcept;

}  // namespace inkpod::windows::ui
