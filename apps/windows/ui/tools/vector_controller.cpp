#include "vector_controller.h"

#include <algorithm>
#include <cstddef>
#include <new>

#include "app/core_host.h"
#include "renderer/renderer_host.h"

namespace inkpod::windows::ui::tools {

VectorController::VectorController(app::CoreHost& engine) noexcept
    : engine_(engine) {}

InkpodStatus VectorController::ResolveGeometryPoints(
    std::uint64_t view_id,
    std::uint64_t expected_view_revision,
    bool bypass_snap,
    const std::vector<InkpodStrokeSample>& samples,
    std::vector<InkpodGeometryPoint>& points,
    std::uint64_t& resolved_view_revision) noexcept {
    if (samples.empty()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    try {
        const std::size_t limit = renderer::kCanvasGeometryPreviewPoints;
        const std::size_t stride = std::max<std::size_t>(
            1U, (samples.size() + limit - 1U) / limit);
        std::vector<InkpodStrokeSample> bounded;
        bounded.reserve(std::min(samples.size() + 1U, limit));
        for (std::size_t index = 0U; index < samples.size(); index += stride) {
            bounded.push_back(samples[index]);
        }
        if (bounded.back().x != samples.back().x || bounded.back().y != samples.back().y) {
            if (bounded.size() == limit) {
                bounded.back() = samples.back();
            } else {
                bounded.push_back(samples.back());
            }
        }
        std::vector<InkpodGeometryPoint> resolved(
            bounded.size(), InkpodGeometryPoint{sizeof(InkpodGeometryPoint), 0U, 0.0F, 0.0F});
        InkpodGeometryPointResolveInput input{
            sizeof(InkpodGeometryPointResolveInput),
            INKPOD_COORDINATE_SPACE_DEVICE,
            bypass_snap ? INKPOD_GEOMETRY_RESOLVE_BYPASS_SNAP
                        : INKPOD_GEOMETRY_RESOLVE_USE_VIEW_SNAP,
            view_id,
            expected_view_revision,
            bounded.data(),
            bounded.size(),
            sizeof(InkpodStrokeSample)};
        InkpodGeometryPointResolveResult result{};
        result.struct_size = sizeof(result);
        const InkpodStatus status = engine_.Invoke(
            [&input, &result, &resolved](InkpodCore* core) {
                return inkpod_core_geometry_points_resolve(
                    core, &input, &result, resolved.data(), resolved.size());
            },
            true,
            false);
        if (status == INKPOD_STATUS_OK) {
            points.swap(resolved);
            resolved_view_revision = result.view_revision;
        }
        return status;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodStatus VectorController::BeginGeometry(
    const InkpodGeometryInput& input,
    InkpodGeometryPreviewInfo& info) noexcept {
    return engine_.Invoke(
        [&input, &info](InkpodCore* core) {
            return inkpod_core_geometry_preview_begin(core, &input, &info);
        },
        true,
        false);
}

InkpodStatus VectorController::UpdateGeometry(
    const InkpodGeometryInput& input,
    InkpodGeometryPreviewInfo& info) noexcept {
    return engine_.Invoke(
        [&input, &info](InkpodCore* core) {
            return inkpod_core_geometry_preview_update(core, &input, &info);
        },
        true,
        false);
}

InkpodStatus VectorController::CommitGeometry() noexcept {
    return engine_.Invoke(
        [](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t path_id{};
            std::uint64_t fill_id{};
            return inkpod_core_geometry_preview_commit(
                core, &result, &path_id, &fill_id);
        },
        true,
        true);
}

InkpodStatus VectorController::CancelGeometry() noexcept {
    return engine_.Invoke(
        [](InkpodCore* core) {
            return inkpod_core_geometry_preview_cancel(core);
        },
        true,
        false);
}

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
