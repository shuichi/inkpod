#pragma once

namespace inkpod::app {
class ApplicationHost;
}

namespace inkpod::windows::ui {

// Runs the end-to-end regression scenarios through the packaged executable's real
// window, Core-engine, snapshot-queue, and renderer paths.
int RunApplicationSmoke(app::ApplicationHost& state) noexcept;

// Runs the reproducible native wheel/drawing performance scenarios used for
// same-host revision-max comparisons. Timings are emitted to standard error.
int RunPerformanceSmoke(app::ApplicationHost& state) noexcept;

// Exercises loaded TGA sequence navigation through the real keyboard, Core,
// and renderer paths. Existing revision-max workloads remain independent.
int RunSequencePerformanceSmoke(app::ApplicationHost& state) noexcept;

}  // namespace inkpod::windows::ui
