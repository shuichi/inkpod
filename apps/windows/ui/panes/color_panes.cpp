#include "color_panes.h"

#include <algorithm>
#include <cstddef>
#include <new>
#include <string>

#include "app/frontend_state.h"
#include "app/core_host.h"

namespace inkpod::windows::ui::panes {
namespace {

InkpodPrimitiveRequestV3 PrimitiveRequest(
    std::uint32_t opcode,
    std::uint32_t schema_version,
    std::uint64_t base_revision) noexcept {
    InkpodPrimitiveRequestV3 request{};
    request.struct_size = sizeof(request);
    request.opcode = opcode;
    request.schema_version = schema_version;
    request.base_revision = base_revision;
    request.payload_id.struct_size = sizeof(request.payload_id);
    return request;
}

bool WideFromUtf8(const std::vector<std::uint8_t>& input, std::wstring& output) {
    if (input.empty()) {
        return false;
    }
    const int count = MultiByteToWideChar(
        CP_UTF8, MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(input.data()),
        static_cast<int>(input.size()), nullptr, 0);
    if (count <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(count));
    return MultiByteToWideChar(
               CP_UTF8, MB_ERR_INVALID_CHARS,
               reinterpret_cast<const char*>(input.data()),
               static_cast<int>(input.size()), output.data(), count)
        == count;
}

bool Utf8FromWide(const std::wstring& input, std::string& output) {
    if (input.empty()) {
        return false;
    }
    const int count = WideCharToMultiByte(
        CP_UTF8, WC_ERR_INVALID_CHARS, input.data(),
        static_cast<int>(input.size()), nullptr, 0, nullptr, nullptr);
    if (count <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(count));
    return WideCharToMultiByte(
               CP_UTF8, WC_ERR_INVALID_CHARS, input.data(),
               static_cast<int>(input.size()), output.data(), count, nullptr, nullptr)
        == count;
}

InkpodStatus LoadChartFromCore(
    InkpodCore* core,
    std::vector<InkpodColorValue>& colors,
    std::vector<std::wstring>& names,
    InkpodColorChartInfo& info) {
    InkpodStatus status = inkpod_core_color_chart_info(core, &info);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    try {
        colors.resize(static_cast<std::size_t>(info.entry_count));
        names.resize(colors.size());
        for (std::size_t index = 0; index < colors.size(); ++index) {
            colors[index].struct_size = sizeof(InkpodColorValue);
            std::uint64_t name_bytes{};
            status = inkpod_core_color_chart_get(
                core, index, &colors[index], nullptr, 0U, &name_bytes);
            if (status != INKPOD_STATUS_OK || name_bytes == 0U
                || name_bytes > 1024U) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE
                    : status;
            }
            std::vector<std::uint8_t> utf8(static_cast<std::size_t>(name_bytes));
            status = inkpod_core_color_chart_get(
                core, index, &colors[index], utf8.data(), utf8.size(), &name_bytes);
            if (status != INKPOD_STATUS_OK || !WideFromUtf8(utf8, names[index])) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE
                    : status;
            }
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return INKPOD_STATUS_OK;
}

}  // namespace

ColorPanesController::ColorPanesController(app::CoreHost& engine) noexcept
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
    std::vector<InkpodColorValue> chart_colors;
    std::vector<std::wstring> chart_names;
    InkpodColorChartInfo chart_info{};
    chart_info.struct_size = sizeof(chart_info);
    status = LoadColorChart(chart_colors, chart_names, chart_info);
    return status == INKPOD_STATUS_OK
        ? ApplyLoadedModel(
              panes, main_line_color, colors, chart_colors, chart_names, chart_info)
        : status;
}

InkpodStatus ColorPanesController::RefreshModel(
    app::DocumentSessionId session,
    app::Generation generation,
    app::PaneUiState& panes) noexcept {
    InkpodColorValue main_line_color{};
    main_line_color.struct_size = sizeof(main_line_color);
    InkpodStatus status = LoadMainLineColor(
        session, generation, main_line_color);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    std::vector<InkpodColorValue> colors;
    status = LoadPalette(session, generation, colors);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    std::vector<InkpodColorValue> chart_colors;
    std::vector<std::wstring> chart_names;
    InkpodColorChartInfo chart_info{};
    chart_info.struct_size = sizeof(chart_info);
    status = LoadColorChart(
        session, generation, chart_colors, chart_names, chart_info);
    return status == INKPOD_STATUS_OK
        ? ApplyLoadedModel(
              panes, main_line_color, colors, chart_colors, chart_names, chart_info)
        : status;
}

InkpodStatus ColorPanesController::ApplyLoadedModel(
    app::PaneUiState& panes,
    const InkpodColorValue& main_line_color,
    const std::vector<InkpodColorValue>& colors,
    const std::vector<InkpodColorValue>& chart_colors,
    const std::vector<std::wstring>& chart_names,
    const InkpodColorChartInfo& chart_info) noexcept {
    try {
        panes.main_line_color = main_line_color;
        panes.palette_colors = colors;
        panes.color_chart_colors = chart_colors;
        panes.color_chart_names = chart_names;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    panes.color_chart_locked =
        (chart_info.flags & INKPOD_COLOR_CHART_LOCKED) != 0U;
    const std::uint32_t palette_group_count = std::max<std::uint32_t>(
        1U, static_cast<std::uint32_t>((colors.size() + 9U) / 10U));
    panes.palette_group %= palette_group_count;
    constexpr std::size_t chart_page_size = 20U;
    const std::uint32_t chart_page_count = std::max<std::uint32_t>(
        1U,
        static_cast<std::uint32_t>(
            (chart_colors.size() + chart_page_size - 1U) / chart_page_size));
    panes.color_chart_page = chart_info.page % chart_page_count;
    if (colors.empty()) {
        panes.selected_palette_index = 0U;
    } else {
        panes.selected_palette_index = std::min(
            panes.selected_palette_index,
            static_cast<std::uint32_t>(colors.size() - 1U));
    }
    if (chart_colors.empty()) {
        panes.selected_color_chart_index = 0U;
    } else {
        panes.selected_color_chart_index =
            (chart_info.flags & INKPOD_COLOR_CHART_HAS_SELECTION) != 0U
            ? std::min(
                  static_cast<std::uint32_t>(chart_info.selected_index),
                  static_cast<std::uint32_t>(chart_colors.size() - 1U))
            : 0U;
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

InkpodStatus ColorPanesController::LoadPalette(
    app::DocumentSessionId session,
    app::Generation generation,
    std::vector<InkpodColorValue>& colors) noexcept {
    return engine_.Invoke(
        session,
        generation,
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

InkpodStatus ColorPanesController::LoadColorChart(
    std::vector<InkpodColorValue>& colors,
    std::vector<std::wstring>& names,
    InkpodColorChartInfo& info) noexcept {
    return engine_.Invoke(
        [&colors, &names, &info](InkpodCore* core) {
            return LoadChartFromCore(core, colors, names, info);
        },
        false,
        false);
}

InkpodStatus ColorPanesController::LoadColorChart(
    app::DocumentSessionId session,
    app::Generation generation,
    std::vector<InkpodColorValue>& colors,
    std::vector<std::wstring>& names,
    InkpodColorChartInfo& info) noexcept {
    return engine_.Invoke(
        session,
        generation,
        [&colors, &names, &info](InkpodCore* core) {
            return LoadChartFromCore(core, colors, names, info);
        },
        false,
        false);
}

InkpodStatus ColorPanesController::LoadMainLineColor(
    app::DocumentSessionId session,
    app::Generation generation,
    InkpodColorValue& color) noexcept {
    return engine_.Invoke(
        session,
        generation,
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

InkpodStatus ColorPanesController::ReplaceColorChart(
    app::DocumentSessionId session,
    app::Generation generation,
    const std::vector<InkpodColorValue>& colors,
    const std::vector<std::wstring>& names,
    bool locked) noexcept {
    if (colors.size() != names.size()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<std::string> utf8;
    std::vector<InkpodColorChartEntry> entries;
    try {
        utf8.resize(names.size());
        entries.resize(names.size());
        for (std::size_t index = 0; index < names.size(); ++index) {
            if (!Utf8FromWide(names[index], utf8[index])) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            entries[index].struct_size = sizeof(InkpodColorChartEntry);
            entries[index].color = colors[index];
            entries[index].name_utf8 = reinterpret_cast<const std::uint8_t*>(
                utf8[index].data());
            entries[index].name_bytes = utf8[index].size();
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return engine_.Invoke(
        session,
        generation,
        [&entries, locked](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_color_chart_set(
                core,
                entries.empty() ? nullptr : entries.data(),
                entries.size(),
                entries.empty() ? 0U : sizeof(InkpodColorChartEntry),
                locked ? 1U : 0U,
                &result);
        },
        true,
        true);
}

InkpodStatus ColorPanesController::ReplacePalette(
    app::DocumentSessionId session,
    app::Generation generation,
    const std::vector<InkpodColorValue>& colors) noexcept {
    InkpodDocumentInfo document{};
    document.struct_size = sizeof(document);
    if (!engine_.GetDocumentInfo(session, generation, document)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodColorArray input{};
    input.struct_size = sizeof(input);
    input.colors = colors.empty() ? nullptr : colors.data();
    input.color_count = colors.size();
    input.color_stride_bytes = colors.empty()
        ? 0U
        : sizeof(InkpodColorValue);
    InkpodObjectId payload{};
    payload.struct_size = sizeof(payload);
    InkpodStatus status = engine_.RegisterColorArray(
        session, generation, input, payload);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    auto request = PrimitiveRequest(
        INKPOD_PRIMITIVE_REPLACE_PALETTE, 1U, document.document_revision);
    request.payload_id = payload;
    status = engine_.InvokePrimitive(
        session, generation, request, true, true);
    const InkpodStatus release = engine_.ReleaseObject(
        session, generation, payload);
    return status == INKPOD_STATUS_OK ? release : status;
}

InkpodStatus ColorPanesController::SetMainLineColor(
    const InkpodColorValue& color) noexcept {
    InkpodDocumentInfo document{};
    document.struct_size = sizeof(document);
    if (!engine_.GetDocumentInfo(document)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto request = PrimitiveRequest(
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR,
        1U,
        document.document_revision);
    request.color = color;
    return engine_.InvokePrimitive(request, true, true);
}

} // namespace inkpod::windows::ui::panes
