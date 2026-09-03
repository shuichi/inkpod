#include "line_correction_options.h"

#include "app/resource.h"
#include "inkpod/core_ffi.h"
#include "tool_state.h"

namespace inkpod::windows::ui::tools {

bool IsGlobalLineCorrectionCommand(UINT command) noexcept {
    return command == IDM_LINE_DUST_APPLY || command == IDM_LINE_CONNECT_APPLY
        || command == IDM_LINE_WIDTH_APPLY;
}

bool IsLineCorrectionCommand(UINT command) noexcept {
    return IsGlobalLineCorrectionCommand(command) || command == IDM_EFFECT_DUST
        || command == IDM_EFFECT_LINE_CONNECT || command == IDM_EFFECT_LINE_WIDTH;
}

bool PrepareLineCorrectionEditor(
    UINT command, EffectEditorState& editor, std::uint32_t& interaction) noexcept {
    if (!IsLineCorrectionCommand(command)) return false;
    try {
        editor = {};
        editor.line_options = true;
        editor.parameter_count = 2U;
        editor.parameters = {3, 24, 1, 0, 0};
        editor.parameter_labels[0] = UiText(UiStringId::LineMaximumPixels);
        editor.parameter_labels[1] = UiText(UiStringId::LineBandDiameter);
        editor.channel_labels = {UiText(UiStringId::LineSelectionOrImage),
            UiText(UiStringId::Text0345), UiText(UiStringId::Text0821),
            UiText(UiStringId::ToolPolyline), UiText(UiStringId::Text0664)};
        editor.channel_values = {0U, INKPOD_SELECTION_TRACE, INKPOD_SELECTION_RECTANGLE,
            INKPOD_SELECTION_POLYLINE, INKPOD_SELECTION_LASSO};
        editor.channel_count = IsGlobalLineCorrectionCommand(command) ? 1U : 5U;
        editor.channel = IsGlobalLineCorrectionCommand(command) ? 0U : INKPOD_SELECTION_TRACE;
        editor.points = L"#ffffffff";
        editor.points_label = UiText(UiStringId::LineBackgroundColor);
        editor.option1 = true;
        editor.option2_enabled = false;
        if (command == IDM_EFFECT_DUST || command == IDM_LINE_DUST_APPLY) {
            interaction = kInteractionEffectDust;
            editor.title = UiText(UiStringId::ToolDustRemoval);
            editor.mode_labels = {UiText(UiStringId::Text0542), UiText(UiStringId::Text0954),
                UiText(UiStringId::Text0871), nullptr};
            editor.mode_values = {INKPOD_LINE_REMOVE_DUST, INKPOD_LINE_FILL_HOLES,
                INKPOD_LINE_REPLACE_OUTLIERS, 0U};
            editor.mode_count = 3U;
            editor.mode = INKPOD_LINE_REMOVE_DUST;
        } else if (command == IDM_EFFECT_LINE_CONNECT || command == IDM_LINE_CONNECT_APPLY) {
            interaction = kInteractionEffectLineConnect;
            editor.title = UiText(UiStringId::LineConnect);
            editor.parameter_count = 3U;
            editor.parameter_labels[0] = UiText(UiStringId::LineGap);
            editor.parameter_labels[2] = UiText(UiStringId::LineConnectionWidth);
            editor.mode = INKPOD_LINE_CONNECT;
        } else {
            interaction = kInteractionEffectLineWidth;
            editor.title = UiText(UiStringId::LineWidth);
            editor.parameters[0] = 1;
            editor.parameter_labels[0] = UiText(UiStringId::LineWidthAmount);
            editor.mode_labels = {UiText(UiStringId::LineThicken), UiText(UiStringId::LineThin),
                UiText(UiStringId::LineUniform), nullptr};
            editor.mode_values = {INKPOD_LINE_THICKEN, INKPOD_LINE_THIN, INKPOD_LINE_UNIFORM, 0U};
            editor.mode_count = 3U;
            editor.mode = INKPOD_LINE_THICKEN;
        }
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ReadLineBackground(
    const EffectEditorState& editor, std::array<std::uint16_t, 4U>& color) noexcept {
    color = {};
    if (editor.line_values[0] < 1U || editor.line_values[0] > 2U
        || editor.line_values[1] > 3U || editor.line_values[2] > 2U) return false;
    if (editor.line_values[2] != 2U) return true;
    const auto& value = editor.points;
    if ((value.size() != 9U && value.size() != 17U) || value.front() != L'#') return false;
    const std::size_t digits = (value.size() - 1U) / 4U;
    for (std::size_t channel = 0U; channel < color.size(); ++channel) {
        std::uint32_t component{};
        for (std::size_t digit = 0U; digit < digits; ++digit) {
            const wchar_t ch = value[1U + channel * digits + digit];
            const int hex = ch >= L'0' && ch <= L'9' ? ch - L'0'
                : (ch >= L'a' && ch <= L'f' ? ch - L'a' + 10
                    : (ch >= L'A' && ch <= L'F' ? ch - L'A' + 10 : -1));
            if (hex < 0) return false;
            component = component * 16U + static_cast<std::uint32_t>(hex);
        }
        color[channel] = static_cast<std::uint16_t>(digits == 2U ? component * 257U : component);
    }
    return true;
}

}  // namespace inkpod::windows::ui::tools
