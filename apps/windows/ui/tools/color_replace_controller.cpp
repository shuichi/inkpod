#include "color_replace_controller.h"

#include "app/core_host.h"

namespace inkpod::windows::ui::tools {

ColorReplaceController::ColorReplaceController(app::CoreHost& engine) noexcept
    : engine_(engine) {}

InkpodStatus ColorReplaceController::Apply(
    InkpodScopedColorReplaceInput input,
    const std::vector<InkpodSelectionPoint>& points,
    InkpodDispatchResult& result) noexcept {
    return engine_.Invoke(
        [input, points, &result](InkpodCore* core) mutable {
            if (!points.empty()) {
                input.points = points.data();
            }
            return inkpod_core_apply_scoped_color_replace(core, &input, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::tools
