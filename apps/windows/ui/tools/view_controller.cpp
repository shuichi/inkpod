#include "view_controller.h"

#include <cstddef>
#include <cstdint>
#include <new>
#include <vector>

#include "app/core_host.h"

namespace inkpod::windows::ui::tools {

ViewController::ViewController(app::CoreHost& engine) noexcept : engine_(engine) {}

InkpodStatus ViewController::Apply(
    std::uint64_t view_id, const InkpodViewInput& input) noexcept {
    const bool reset_scroll_range = input.kind == INKPOD_VIEW_FIT
        || input.kind == INKPOD_VIEW_ONE_TO_ONE;
    return engine_.Invoke(
        [view_id, input](InkpodCore* core) {
            InkpodDocumentInfo info{};
            info.struct_size = sizeof(info);
            InkpodStatus status = view_id == 0U
                ? inkpod_core_apply_view(core, &input, &info)
                : inkpod_core_view_apply(core, view_id, &input);
            if (status == INKPOD_STATUS_OK && view_id != 0U) {
                status = inkpod_core_get_document_info(core, &info);
            }
            return status;
        },
        true,
        true,
        reset_scroll_range
            ? app::ScrollRangeResetRequest{
                  app::ScrollRangeResetScope::TargetView, view_id}
            : app::ScrollRangeResetRequest{});
}

InkpodStatus ViewController::AddGuide(
    std::uint32_t axis,
    std::int32_t position_milli,
    std::uint64_t& guide_id) noexcept {
    return engine_.Invoke(
        [axis, position_milli, &guide_id](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_guide_add(
                core, axis, position_milli, &result, &guide_id);
        },
        true,
        true);
}

InkpodStatus ViewController::MoveGuide(
    std::uint64_t guide_id, std::int32_t position_milli) noexcept {
    return engine_.Invoke(
        [guide_id, position_milli](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_guide_move(
                core, guide_id, position_milli, &result);
        },
        true,
        true);
}

InkpodStatus ViewController::DeleteGuide(std::uint64_t guide_id) noexcept {
    return engine_.Invoke(
        [guide_id](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_guide_delete(core, guide_id, &result);
        },
        true,
        true);
}

InkpodStatus ViewController::DeleteAllGuides() noexcept {
    return engine_.Invoke(
        [](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_guide_delete_all(core, &result);
        },
        true,
        true);
}

InkpodStatus ViewController::SetGrid(const InkpodGridInput& input) noexcept {
    return engine_.Invoke(
        [input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_grid_set(core, &input, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::tools
