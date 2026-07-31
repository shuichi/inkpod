#include "vector_controller.h"

#include <algorithm>
#include <cstddef>
#include <new>

#include "app/core_host.h"

namespace inkpod::windows::ui::tools {

VectorController::VectorController(app::CoreHost& engine) noexcept
    : engine_(engine) {}

InkpodStatus VectorController::AddPath(
    const InkpodVectorPathInput& input) noexcept {
    return engine_.Invoke(
        [&input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t path_id{};
            return inkpod_core_vector_add_path(core, &input, &result, &path_id);
        },
        true,
        true);
}

InkpodStatus VectorController::Erase(
    const InkpodVectorEraseInput& input) noexcept {
    return engine_.Invoke(
        [input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_vector_erase(core, &input, &result);
        },
        true,
        true);
}

InkpodStatus VectorController::Select(
    InkpodVectorSelectionMode mode,
    std::vector<std::uint64_t>& selected_path_ids) noexcept {
    return engine_.Invoke(
        [mode, &selected_path_ids](InkpodCore* core) {
            InkpodDocumentInfo document{};
            document.struct_size = sizeof(document);
            InkpodStatus status = inkpod_core_get_document_info(core, &document);
            InkpodLocatorOutput locator{};
            locator.struct_size = sizeof(locator);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_locator_sample(core, 0U, 0.0, 0.0, &locator);
            }
            InkpodFrameRect bounds = locator.selection;
            if (bounds.width <= 0 || bounds.height <= 0) {
                bounds = InkpodFrameRect{
                    0,
                    0,
                    static_cast<std::int32_t>(document.width),
                    static_cast<std::int32_t>(document.height)};
            }
            const InkpodVectorSelectionInput input{
                sizeof(InkpodVectorSelectionInput), mode, 0U, bounds};
            InkpodVectorSelectionBuffer output{};
            output.struct_size = sizeof(output);
            status = inkpod_core_vector_select(core, &input, &output);
            if (status == INKPOD_STATUS_BUFFER_TOO_SMALL) {
                status = INKPOD_STATUS_OK;
            }
            std::vector<InkpodVectorSelectionRange> ranges;
            std::vector<std::uint64_t> fill_ids;
            try {
                ranges.resize(static_cast<std::size_t>(output.range_count));
                fill_ids.resize(static_cast<std::size_t>(output.fill_count));
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            for (auto& range : ranges) {
                range.struct_size = sizeof(range);
            }
            output.ranges = ranges.data();
            output.range_capacity = ranges.size();
            output.fill_ids = fill_ids.data();
            output.fill_capacity = fill_ids.size();
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_vector_select(core, &input, &output);
            }
            if (status == INKPOD_STATUS_OK) {
                try {
                    selected_path_ids.clear();
                    for (const auto& range : ranges) {
                        if (std::find(
                                selected_path_ids.begin(),
                                selected_path_ids.end(),
                                range.path_id)
                            == selected_path_ids.end()) {
                            selected_path_ids.push_back(range.path_id);
                        }
                    }
                } catch (const std::bad_alloc&) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
            }
            return status;
        },
        false,
        false);
}

InkpodStatus VectorController::Connect(
    std::uint64_t plane_id, float maximum_gap) noexcept {
    return engine_.Invoke(
        [plane_id, maximum_gap](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t path_id{};
            return inkpod_core_vector_connect(
                core, plane_id, maximum_gap, &result, &path_id);
        },
        true,
        true);
}

InkpodStatus VectorController::CorrectWidth(
    const InkpodVectorWidthInput& input) noexcept {
    return engine_.Invoke(
        [&input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_vector_correct_width(core, &input, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::tools
