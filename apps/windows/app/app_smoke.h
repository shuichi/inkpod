#pragma once

namespace inkpod::app {
struct AppContext;
}

namespace inkpod::windows::ui {

// Runs the end-to-end regression scenarios through the packaged executable's real
// window, Core-engine, snapshot-queue, and renderer paths.
int RunApplicationSmoke(app::AppContext& state) noexcept;

}  // namespace inkpod::windows::ui
