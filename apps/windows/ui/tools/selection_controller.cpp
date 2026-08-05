#include "selection_controller.h"

#include "app/core_host.h"

namespace inkpod::windows::ui::tools {

SelectionController::SelectionController(app::CoreHost& engine) noexcept
    : engine_(engine) {}

InkpodStatus SelectionController::Apply(
    std::uint64_t layer_id,
    std::uint64_t plane_id,
    InkpodSelectionInput input,
    const std::vector<InkpodSelectionPoint>& points) noexcept {
    return engine_.Invoke(
        [layer_id, plane_id, input, points](InkpodCore* core) mutable {
            if (!points.empty()) {
                input.points = points.data();
            }
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_apply_selection_for_editor_target(
                core, layer_id, plane_id, &input, &result);
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
    std::uint64_t layer_id,
    std::uint64_t plane_id,
    const InkpodColorValue& color,
    bool different,
    InkpodSelectionOperation operation) noexcept {
    return engine_.Invoke(
        [layer_id, plane_id, color, different, operation](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_select_color_for_editor_target(
                core,
                layer_id,
                plane_id,
                &color,
                0U,
                different ? 1U : 0U,
                operation,
                &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::tools
