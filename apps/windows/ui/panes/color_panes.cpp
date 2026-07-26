#include "color_panes.h"

#include <algorithm>
#include <cstddef>
#include <new>

#include "app/app_context.h"
#include "app/core_engine.h"

namespace inkpod::windows::ui::panes {

ColorPanesController::ColorPanesController(app::CoreEngine& engine) noexcept
    : engine_(engine) {}

InkpodStatus ColorPanesController::RefreshModel(
    app::PaneUiState& panes) noexcept {
    std::vector<InkpodColorValue> colors;
    const InkpodStatus status = LoadPalette(colors);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    try {
        panes.palette_colors = colors;
        const std::size_t previous_names = panes.color_chart_names.size();
        panes.color_chart_names.resize(colors.size());
        for (std::size_t index = previous_names; index < colors.size(); ++index) {
            panes.color_chart_names[index] =
                L"Color " + std::to_wstring(index + 1U);
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint32_t palette_group_count = std::max<std::uint32_t>(
        1U, static_cast<std::uint32_t>((colors.size() + 9U) / 10U));
    panes.palette_group %= palette_group_count;
    constexpr std::size_t chart_page_size = 20U;
    const std::uint32_t chart_page_count = std::max<std::uint32_t>(
        1U,
        static_cast<std::uint32_t>(
            (colors.size() + chart_page_size - 1U) / chart_page_size));
    panes.color_chart_page %= chart_page_count;
    return INKPOD_STATUS_OK;
}

InkpodStatus ColorPanesController::LoadPalette(
    std::vector<InkpodColorValue>& colors) noexcept {
    return engine_.Invoke(
        [&colors](InkpodCore* core) {
            InkpodColorBuffer query{};
            query.struct_size = sizeof(query);
            InkpodStatus status = inkpod_core_palette_get(core, &query);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            try {
                colors.resize(static_cast<std::size_t>(query.color_count));
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            for (auto& color : colors) {
                color.struct_size = sizeof(color);
            }
            if (colors.empty()) {
                return INKPOD_STATUS_OK;
            }
            InkpodColorBuffer output{};
            output.struct_size = sizeof(output);
            output.colors = colors.data();
            output.color_capacity = colors.size();
            output.color_stride_bytes = sizeof(InkpodColorValue);
            return inkpod_core_palette_get(core, &output);
        },
        false,
        false);
}

InkpodStatus ColorPanesController::ReplacePalette(
    const std::vector<InkpodColorValue>& colors) noexcept {
    return engine_.Invoke(
        [&colors](InkpodCore* core) {
            InkpodColorArray input{};
            input.struct_size = sizeof(input);
            input.colors = colors.empty() ? nullptr : colors.data();
            input.color_count = colors.size();
            input.color_stride_bytes = colors.empty()
                ? 0U
                : sizeof(InkpodColorValue);
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_palette_set(core, &input, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::panes
