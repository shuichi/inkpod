#include "selection_controller.h"

#include "app/core_engine.h"

namespace inkpod::windows::ui::tools {

SelectionController::SelectionController(app::CoreEngine& engine) noexcept
    : engine_(engine) {}

InkpodStatus SelectionController::Apply(
    InkpodSelectionInput input,
    const std::vector<InkpodSelectionPoint>& points) noexcept {
    return engine_.Invoke(
        [input, points](InkpodCore* core) mutable {
            if (!points.empty()) {
                input.points = points.data();
            }
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_apply_selection(core, &input, &result);
        },
        true,
        true);
}

InkpodStatus SelectionController::ApplyEmpty(
    InkpodSelectionOperation operation) noexcept {
    if (operation == INKPOD_SELECTION_ADD
        || operation == INKPOD_SELECTION_SUBTRACT) {
        return INKPOD_STATUS_OK;
    }
    if (operation != INKPOD_SELECTION_NEW
        && operation != INKPOD_SELECTION_INTERSECT) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    return engine_.Invoke(
        [](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_selection_clear(core, &result);
        },
        true,
        true);
}

InkpodStatus SelectionController::SelectColor(
    const InkpodColorValue& color,
    bool different,
    InkpodSelectionOperation operation) noexcept {
    return engine_.Invoke(
        [color, different, operation](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_select_color(
                core, &color, 0U, different ? 1U : 0U, operation, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::tools
