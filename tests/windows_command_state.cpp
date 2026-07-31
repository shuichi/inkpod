#include <array>
#include <cstdint>

#include "app/frontend_state.h"
#include "app/resource.h"
#include "ui/command_catalog.h"
#include "ui/command_state.h"
#include "ui/tools/tool_state.h"

namespace {

using inkpod::app::ToolUiState;
using inkpod::windows::ui::CommandStateInputs;
using inkpod::windows::ui::CommandStateOwner;
using inkpod::windows::ui::CommandStateSet;
using inkpod::windows::ui::ComputeCommandStates;
using inkpod::windows::ui::BuildDefaultShortcutSequences;
using inkpod::windows::ui::FindShortcutSequence;
using inkpod::windows::ui::FindCommandState;
using inkpod::windows::ui::IsCommandChecked;
using inkpod::windows::ui::IsCommandEnabled;
using inkpod::windows::ui::MenuCommandCatalog;
using inkpod::windows::ui::kProductionCommandStateCount;
using inkpod::windows::ui::tools::HandleActivePlaneTransition;
using inkpod::windows::ui::tools::SetActiveCommandColor;
using inkpod::windows::ui::tools::TransitionActiveTool;
using inkpod::windows::ui::tools::kInteractionEyedropper;
using inkpod::windows::ui::tools::kInteractionEffectGradient;
using inkpod::windows::ui::tools::kInteractionFill;
using inkpod::windows::ui::tools::kInteractionSelection;
using inkpod::windows::ui::tools::kInteractionVectorLine;

bool SameStates(const CommandStateSet& left, const CommandStateSet& right) noexcept {
    for (std::size_t index = 0; index < left.size(); ++index) {
        if (left[index].command != right[index].command
            || left[index].owner != right[index].owner
            || left[index].enabled != right[index].enabled
            || left[index].checked != right[index].checked) {
            return false;
        }
    }
    return true;
}

bool CatalogHasExactlyOneOwner(const CommandStateSet& states) noexcept {
    std::array<std::size_t, 11U> owner_counts{};
    for (std::size_t left = 0; left < states.size(); ++left) {
        if (states[left].command == 0U) {
            return false;
        }
        const auto owner = static_cast<std::size_t>(states[left].owner);
        if (owner >= owner_counts.size()) {
            return false;
        }
        ++owner_counts[owner];
        for (std::size_t right = left + 1U; right < states.size(); ++right) {
            if (states[left].command == states[right].command) {
                return false;
            }
        }
    }
    for (const std::size_t count : owner_counts) {
        if (count == 0U) {
            return false;
        }
    }
    return states.size() == kProductionCommandStateCount;
}

bool SameColor(
    const InkpodColorValue& left, const InkpodColorValue& right) noexcept {
    return left.depth == right.depth && left.red == right.red
        && left.green == right.green && left.blue == right.blue
        && left.alpha == right.alpha;
}

bool StartsWith(
    const InkpodShortcutSequence& sequence,
    const InkpodShortcutSequence& prefix) noexcept {
    if (prefix.stroke_count > sequence.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0; index < prefix.stroke_count; ++index) {
        if (sequence.strokes[index].virtual_key != prefix.strokes[index].virtual_key
            || sequence.strokes[index].modifiers != prefix.strokes[index].modifiers) {
            return false;
        }
    }
    return true;
}

bool ShortcutCatalogIsCompleteAndPrefixFree() {
    const auto commands = MenuCommandCatalog();
    const auto shortcuts = BuildDefaultShortcutSequences();
    if (shortcuts.size() != commands.size() || commands.size() != kProductionCommandStateCount) {
        return false;
    }
    for (const UINT command : commands) {
        const auto* sequence = FindShortcutSequence(shortcuts, command);
        if (sequence == nullptr || sequence->command_id != command
            || sequence->struct_size != sizeof(InkpodShortcutSequence)
            || sequence->stroke_count == 0U
            || sequence->stroke_count > INKPOD_SHORTCUT_MAX_STROKES) {
            return false;
        }
        for (std::uint32_t index = 0; index < sequence->stroke_count; ++index) {
            if (sequence->strokes[index].virtual_key == 0U) {
                return false;
            }
        }
    }
    for (std::size_t left = 0; left < shortcuts.size(); ++left) {
        for (std::size_t right = left + 1U; right < shortcuts.size(); ++right) {
            if (shortcuts[left].command_id == shortcuts[right].command_id
                || StartsWith(shortcuts[left], shortcuts[right])
                || StartsWith(shortcuts[right], shortcuts[left])) {
                return false;
            }
        }
    }
    const auto* save = FindShortcutSequence(shortcuts, IDM_FILE_SAVE);
    const auto* pencil = FindShortcutSequence(shortcuts, IDM_TOOL_PENCIL);
    const auto* batch = FindShortcutSequence(shortcuts, IDM_WINDOW_BATCH);
    const auto* tool_palette =
        FindShortcutSequence(shortcuts, IDM_WINDOW_TOOL_PALETTE);
    const auto* layer_palette =
        FindShortcutSequence(shortcuts, IDM_WINDOW_LAYER_PALETTE);
    return save != nullptr && save->stroke_count == 1U
        && save->strokes[0].virtual_key == static_cast<std::uint32_t>('S')
        && save->strokes[0].modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL
        && pencil != nullptr && pencil->stroke_count == 1U
        && pencil->strokes[0].virtual_key == static_cast<std::uint32_t>('P')
        && pencil->strokes[0].modifiers == 0U
        && batch != nullptr && batch->stroke_count > 1U
        && tool_palette != nullptr && tool_palette->stroke_count == 3U
        && layer_palette != nullptr && layer_palette->stroke_count == 3U;
}

} // namespace

int main() {
    CommandStateInputs inputs{};
    CommandStateSet states = ComputeCommandStates(inputs);
    if (!CatalogHasExactlyOneOwner(states)
        || !ShortcutCatalogIsCompleteAndPrefixFree()
        || FindCommandState(states, IDM_HELP_ABOUT) == nullptr
        || IsCommandEnabled(states, IDM_FILE_SAVE)
        || IsCommandEnabled(states, IDM_VIEW_FIT)
        || IsCommandEnabled(states, IDM_VIEW_ONE_TO_ONE)
        || IsCommandEnabled(states, IDM_SELECTION_ALL)
        || IsCommandEnabled(states, IDM_FILTER_INVERT)
        || IsCommandEnabled(states, IDM_BATCH_ADD_COLOR_REPLACE)
        || !IsCommandEnabled(states, IDM_WINDOW_TOOL_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_TOOL_PALETTE)
        || !IsCommandEnabled(states, IDM_WINDOW_LAYER_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_LAYER_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_TOOL_OPTIONS)
        || !IsCommandChecked(states, IDM_WINDOW_COLOR_PANE)
        || IsCommandChecked(states, IDM_WORKSPACE_MIRROR)
        || !IsCommandEnabled(states, IDM_FILE_NEW)) {
        return 1;
    }

    inputs.document.has_document = true;
    inputs.document.has_saved_path = true;
    inputs.document.dirty = false;
    states = ComputeCommandStates(inputs);
    CommandStateInputs dirty_inputs = inputs;
    dirty_inputs.document.dirty = true;
    const CommandStateSet dirty_states = ComputeCommandStates(dirty_inputs);
    if (!SameStates(states, dirty_states)
        || !IsCommandEnabled(states, IDM_FILE_SAVE)
        || !IsCommandEnabled(states, IDM_FILE_REVERT)) {
        return 2;
    }

    inputs.edit.can_undo = true;
    inputs.edit.can_redo = false;
    inputs.edit.can_history_back = true;
    inputs.edit.can_history_forward = false;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_EDIT_UNDO)
        || IsCommandEnabled(states, IDM_EDIT_REDO)
        || !IsCommandEnabled(states, IDM_EDIT_HISTORY_BACK)
        || IsCommandEnabled(states, IDM_EDIT_HISTORY_FORWARD)) {
        return 3;
    }

    inputs.tool.vector_stroke_plane = true;
    inputs.tool.active_tool = kInteractionVectorLine;
    inputs.tool.vector_selection_mode = INKPOD_VECTOR_SELECT_FILL_BOUNDARY;
    inputs.tool.palette_visible = true;
    inputs.document_pane.layer_palette_visible = true;
    inputs.selection_view.active_tool = kInteractionVectorLine;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_VECTOR_LINE)
        || !IsCommandChecked(states, IDM_VECTOR_LINE)
        || !IsCommandChecked(states, IDM_VECTOR_SELECT_FILL_BOUNDARY)
        || !IsCommandChecked(states, IDM_WINDOW_TOOL_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_LAYER_PALETTE)) {
        return 4;
    }
    inputs.tool.vector_stroke_plane = false;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_VECTOR_LINE)
        || inputs.tool.active_tool != kInteractionVectorLine) {
        return 5;
    }

    inputs.effects.color_plane_active = true;
    inputs.tool.active_tool = kInteractionEffectGradient;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_EFFECT_GRADIENT)
        || !IsCommandChecked(states, IDM_EFFECT_GRADIENT)) {
        return 10;
    }

    ToolUiState tools{};
    const InkpodColorValue black{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    const InkpodColorValue pencil_color = tools.drawing_color;
    if (!SameColor(pencil_color, black) || tools.color_rgba != UINT32_C(0x000000ff)) {
        return 11;
    }
    TransitionActiveTool(tools, nullptr, kInteractionFill);
    const InkpodColorValue default_coloring_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 220U, 40U, 30U, 255U};
    if (!SameColor(tools.drawing_color, default_coloring_color)) {
        return 12;
    }
    const InkpodColorValue fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 10U, 20U, 30U, 255U};
    SetActiveCommandColor(tools, fill_color);
    TransitionActiveTool(tools, nullptr, kInteractionEyedropper);
    const InkpodColorValue sampled_fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_16, 1000U, 2000U, 3000U, 65535U};
    SetActiveCommandColor(tools, sampled_fill_color);
    TransitionActiveTool(tools, nullptr, INKPOD_TOOL_PENCIL);
    if (!SameColor(tools.drawing_color, pencil_color)) {
        return 13;
    }
    TransitionActiveTool(tools, nullptr, kInteractionFill);
    if (!SameColor(tools.drawing_color, sampled_fill_color)) {
        return 14;
    }

    TransitionActiveTool(tools, nullptr, kInteractionSelection);
    tools.selection_gesture_samples.push_back(InkpodStrokeSample{});
    TransitionActiveTool(tools, nullptr, kInteractionFill);
    if (!tools.selection_gesture_samples.empty()) {
        return 13;
    }

    tools.active_tool = kInteractionVectorLine;
    tools.vector_gesture_samples.push_back(InkpodStrokeSample{});
    HandleActivePlaneTransition(tools, nullptr, false);
    if (tools.active_tool != INKPOD_TOOL_PENCIL
        || !tools.vector_gesture_samples.empty()) {
        return 6;
    }

    inputs.edit.clipboard_available = true;
    inputs.edit.floating_active = true;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_EDIT_PASTE)
        || !IsCommandEnabled(states, IDM_EDIT_FLOATING_COMMIT)
        || !IsCommandEnabled(states, IDM_EDIT_FLOATING_CANCEL)) {
        return 7;
    }

    inputs.edit.floating_active = false;
    inputs.batch.idle = true;
    inputs.batch.has_operations = true;
    inputs.batch.editable_item = true;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_BATCH_PREVIEW)
        || !IsCommandEnabled(states, IDM_BATCH_RUN_ALL)
        || IsCommandEnabled(states, IDM_BATCH_CANCEL)
        || !IsCommandEnabled(states, IDM_BATCH_OPERATION_EDIT)) {
        return 8;
    }
    inputs.batch.idle = false;
    inputs.batch.editable_item = false;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_BATCH_PREVIEW)
        || IsCommandEnabled(states, IDM_BATCH_RUN_ALL)
        || !IsCommandEnabled(states, IDM_BATCH_CANCEL)
        || IsCommandEnabled(states, IDM_BATCH_OPERATION_EDIT)) {
        return 9;
    }

    return 0;
}
