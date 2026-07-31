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
    InkpodColorValue main_line_color{};
    main_line_color.struct_size = sizeof(main_line_color);
    InkpodStatus status = LoadMainLineColor(main_line_color);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    std::vector<InkpodColorValue> colors;
    status = LoadPalette(colors);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    try {
        panes.main_line_color = main_line_color;
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
    if (colors.empty()) {
        panes.selected_palette_index = 0U;
        panes.selected_color_chart_index = 0U;
    } else {
        const auto last = static_cast<std::uint32_t>(colors.size() - 1U);
        panes.selected_palette_index = std::min(panes.selected_palette_index, last);
        panes.selected_color_chart_index = std::min(
            panes.selected_color_chart_index, last);
    }
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

InkpodStatus ColorPanesController::LoadMainLineColor(
    InkpodColorValue& color) noexcept {
    return engine_.Invoke(
        [&color](InkpodCore* core) {
            return inkpod_core_get_main_line_color(core, &color);
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

InkpodStatus ColorPanesController::SetMainLineColor(
    const InkpodColorValue& color) noexcept {
    return engine_.Invoke(
        [&color](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_set_main_line_color(core, &color, &result);
        },
        true,
        true);
}

} // namespace inkpod::windows::ui::panes
