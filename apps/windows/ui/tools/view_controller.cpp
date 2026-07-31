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
        true);
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
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            InkpodSnapshotOverlay overlay{};
            overlay.struct_size = sizeof(overlay);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_overlay(snapshot, &overlay);
            }
            std::vector<std::uint64_t> guide_ids;
            if (status == INKPOD_STATUS_OK) {
                try {
                    guide_ids.reserve(static_cast<std::size_t>(overlay.guide_count));
                    const auto* bytes =
                        reinterpret_cast<const std::uint8_t*>(overlay.guides);
                    for (std::uint64_t index = 0; index < overlay.guide_count; ++index) {
                        const auto* guide =
                            reinterpret_cast<const InkpodSnapshotGuide*>(
                                bytes + static_cast<std::size_t>(
                                            index * overlay.guide_stride_bytes));
                        guide_ids.push_back(guide->id);
                    }
                } catch (const std::bad_alloc&) {
                    status = INKPOD_STATUS_INVALID_STATE;
                }
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            if (release_status != INKPOD_STATUS_OK) {
                return release_status;
            }
            for (const std::uint64_t guide_id : guide_ids) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_guide_delete(core, guide_id, &result);
                if (status != INKPOD_STATUS_OK) {
                    return status;
                }
            }
            return INKPOD_STATUS_OK;
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
