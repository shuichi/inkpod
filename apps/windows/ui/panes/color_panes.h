#pragma once

#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreEngine;
struct PaneUiState;
}

namespace inkpod::windows::ui::panes {

// Owns palette/chart Core adaptation. A future modeless floating palette may
// present this state, while color values stay Core-owned and copied in batches.
class ColorPanesController final {
public:
    explicit ColorPanesController(app::CoreEngine& engine) noexcept;

    InkpodStatus RefreshModel(app::PaneUiState& panes) noexcept;
    InkpodStatus ReplacePalette(
        const std::vector<InkpodColorValue>& colors) noexcept;
    InkpodStatus SetMainLineColor(const InkpodColorValue& color) noexcept;

private:
    InkpodStatus LoadPalette(std::vector<InkpodColorValue>& colors) noexcept;
    InkpodStatus LoadMainLineColor(InkpodColorValue& color) noexcept;

    app::CoreEngine& engine_;
};

} // namespace inkpod::windows::ui::panes
