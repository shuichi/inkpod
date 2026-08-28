#pragma once

#include <string>
#include <vector>

#include "app/identity.h"
#include "inkpod/core_ffi.h"

namespace inkpod::app {
class CoreHost;
struct PaneUiState;
}

namespace inkpod::windows::ui::panes {

// Owns palette/chart Core adaptation. A future modeless floating palette may
// present this state, while color values stay Core-owned and copied in batches.
class ColorPanesController final {
public:
    explicit ColorPanesController(app::CoreHost& engine) noexcept;

    InkpodStatus RefreshModel(app::PaneUiState& panes) noexcept;
    InkpodStatus RefreshModel(
        app::DocumentSessionId session,
        app::Generation generation,
        app::PaneUiState& panes) noexcept;
    InkpodStatus ReplacePalette(
        const std::vector<InkpodColorValue>& colors) noexcept;
    InkpodStatus ReplaceColorChart(
        app::DocumentSessionId session,
        app::Generation generation,
        const std::vector<InkpodColorValue>& colors,
        const std::vector<std::wstring>& names,
        bool locked) noexcept;
    InkpodStatus ReplacePalette(
        app::DocumentSessionId session,
        app::Generation generation,
        const std::vector<InkpodColorValue>& colors) noexcept;
    // Reuses the first exact-depth RGBA match across all groups without editing
    // the document. A new color uses the existing replacement primitive;
    // selected_index is published only after success.
    InkpodStatus RegisterPaletteColor(
        app::DocumentSessionId session,
        app::Generation generation,
        const InkpodColorValue& color,
        std::uint32_t& selected_index) noexcept;
    InkpodStatus SetMainLineColor(const InkpodColorValue& color) noexcept;

private:
    InkpodStatus LoadPalette(std::vector<InkpodColorValue>& colors) noexcept;
    InkpodStatus LoadPalette(
        app::DocumentSessionId session,
        app::Generation generation,
        std::vector<InkpodColorValue>& colors) noexcept;
    InkpodStatus LoadMainLineColor(InkpodColorValue& color) noexcept;
    InkpodStatus LoadMainLineColor(
        app::DocumentSessionId session,
        app::Generation generation,
        InkpodColorValue& color) noexcept;
    InkpodStatus LoadColorChart(
        std::vector<InkpodColorValue>& colors,
        std::vector<std::wstring>& names,
        InkpodColorChartInfo& info) noexcept;
    InkpodStatus LoadColorChart(
        app::DocumentSessionId session,
        app::Generation generation,
        std::vector<InkpodColorValue>& colors,
        std::vector<std::wstring>& names,
        InkpodColorChartInfo& info) noexcept;
    static InkpodStatus ApplyLoadedModel(
        app::PaneUiState& panes,
        const InkpodColorValue& main_line_color,
        const std::vector<InkpodColorValue>& colors,
        const std::vector<InkpodColorValue>& chart_colors,
        const std::vector<std::wstring>& chart_names,
        const InkpodColorChartInfo& chart_info) noexcept;

    app::CoreHost& engine_;
};

} // namespace inkpod::windows::ui::panes
