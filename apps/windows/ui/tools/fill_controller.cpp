#include "fill_controller.h"

#include "app/core_engine.h"

namespace inkpod::windows::ui::tools {

FillController::FillController(app::CoreEngine& engine) noexcept : engine_(engine) {}

InkpodStatus FillController::Apply(
    InkpodFillInput input,
    const std::vector<InkpodColorValue>& inclusion_colors,
    InkpodFillResult& result) noexcept {
    return engine_.Invoke(
        [input, inclusion_colors, &result](InkpodCore* core) mutable {
            input.inclusion_colors = inclusion_colors.empty()
                ? nullptr
                : inclusion_colors.data();
            return inkpod_core_apply_fill(core, &input, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::tools
