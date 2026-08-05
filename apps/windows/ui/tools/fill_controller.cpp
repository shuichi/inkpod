#include "fill_controller.h"

#include "app/core_host.h"

namespace inkpod::windows::ui::tools {

FillController::FillController(app::CoreHost& engine) noexcept : engine_(engine) {}

InkpodStatus FillController::Apply(
    std::uint64_t layer_id,
    std::uint64_t plane_id,
    InkpodFillInput input,
    const std::vector<InkpodColorValue>& inclusion_colors,
    InkpodFillResult& result) noexcept {
    return engine_.Invoke(
        [layer_id, plane_id, input, inclusion_colors, &result](InkpodCore* core) mutable {
            input.inclusion_colors = inclusion_colors.empty()
                ? nullptr
                : inclusion_colors.data();
            return inkpod_core_apply_fill_for_editor_target(
                core, layer_id, plane_id, &input, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::tools
