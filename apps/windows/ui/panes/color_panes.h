#pragma once

#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreEngine;
struct PaneUiState;
}

namespace inkpod::windows::ui::panes {

// Owns palette/chart Core adaptation. The Win32 list presentation remains in
// MainWindow until R4, while color values stay Core-owned and copied in batches.
class ColorPanesController final {
public:
    explicit ColorPanesController(app::CoreEngine& engine) noexcept;

    InkpodStatus RefreshModel(app::PaneUiState& panes) noexcept;
    InkpodStatus ReplacePalette(
        const std::vector<InkpodColorValue>& colors) noexcept;

private:
    InkpodStatus LoadPalette(std::vector<InkpodColorValue>& colors) noexcept;

    app::CoreEngine& engine_;
};

} // namespace inkpod::windows::ui::panes
