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
    const auto* locator =
        FindShortcutSequence(shortcuts, IDM_WINDOW_LOCATOR);
    const auto* sequence =
        FindShortcutSequence(shortcuts, IDM_WINDOW_SEQUENCE);
    const auto* light_table =
        FindShortcutSequence(shortcuts, IDM_WINDOW_LIGHT_TABLE);
    const auto* close_view = FindShortcutSequence(shortcuts, IDM_VIEW_CLOSE);
    const auto* next_tab = FindShortcutSequence(shortcuts, IDM_TAB_NEXT);
    const auto* previous_tab =
        FindShortcutSequence(shortcuts, IDM_TAB_PREVIOUS);
    const auto* manual = FindShortcutSequence(shortcuts, IDM_HELP_MANUAL);
    return save != nullptr && save->stroke_count == 1U
        && save->strokes[0].virtual_key == static_cast<std::uint32_t>('S')
        && save->strokes[0].modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL
        && pencil != nullptr && pencil->stroke_count == 1U
        && pencil->strokes[0].virtual_key == static_cast<std::uint32_t>('P')
        && pencil->strokes[0].modifiers == 0U
        && batch != nullptr && batch->stroke_count > 1U
        && tool_palette != nullptr && tool_palette->stroke_count == 3U
        && layer_palette != nullptr && layer_palette->stroke_count == 3U
        && locator != nullptr && locator->stroke_count == 3U
        && sequence != nullptr && sequence->stroke_count == 3U
        && sequence->strokes[2].virtual_key == static_cast<std::uint32_t>('F')
        && light_table != nullptr && light_table->stroke_count == 3U
        && light_table->strokes[2].virtual_key == static_cast<std::uint32_t>('H')
        && close_view != nullptr && close_view->stroke_count == 1U
        && close_view->strokes[0].virtual_key == VK_F4
        && close_view->strokes[0].modifiers
            == INKPOD_SHORTCUT_MODIFIER_CONTROL
        && next_tab != nullptr && next_tab->stroke_count == 1U
        && next_tab->strokes[0].virtual_key == VK_TAB
        && next_tab->strokes[0].modifiers
            == INKPOD_SHORTCUT_MODIFIER_CONTROL
        && previous_tab != nullptr && previous_tab->stroke_count == 1U
        && previous_tab->strokes[0].virtual_key == VK_TAB
        && previous_tab->strokes[0].modifiers
            == (INKPOD_SHORTCUT_MODIFIER_CONTROL
                | INKPOD_SHORTCUT_MODIFIER_SHIFT)
        && manual != nullptr && manual->stroke_count == 1U
        && manual->strokes[0].virtual_key == VK_F1
        && manual->strokes[0].modifiers == 0U;
}

} // namespace

int main() {
    CommandStateInputs inputs{};
    CommandStateSet states = ComputeCommandStates(inputs);
    if (!CatalogHasExactlyOneOwner(states)
        || !ShortcutCatalogIsCompleteAndPrefixFree()
        || FindCommandState(states, IDM_HELP_MANUAL) == nullptr
        || FindCommandState(states, IDM_HELP_FILE_FORMAT) == nullptr
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
        || !IsCommandEnabled(states, IDM_WINDOW_LOCATOR)
        || IsCommandChecked(states, IDM_WINDOW_LOCATOR)
        || IsCommandEnabled(states, IDM_LOCATOR_PIN)
        || IsCommandEnabled(states, IDM_LOCATOR_FIXED)
        || IsCommandEnabled(states, IDM_LOCATOR_AUTOSCROLL)
        || !IsCommandEnabled(states, IDM_WINDOW_SEQUENCE)
        || IsCommandChecked(states, IDM_WINDOW_SEQUENCE)
        || IsCommandEnabled(states, IDM_SEQUENCE_PIN)
        || !IsCommandEnabled(states, IDM_WINDOW_LIGHT_TABLE)
        || IsCommandChecked(states, IDM_WINDOW_LIGHT_TABLE)
        || !IsCommandEnabled(states, IDM_WINDOW_JOB_PROGRESS)
        || IsCommandChecked(states, IDM_WINDOW_JOB_PROGRESS)
        || IsCommandEnabled(states, IDM_LIGHT_TABLE_PIN)
        || !IsCommandEnabled(states, IDM_WINDOW_SUBPALETTE)
        || IsCommandChecked(states, IDM_WINDOW_SUBPALETTE)
        || IsCommandEnabled(states, IDM_SUBPALETTE_PIN)
        || IsCommandEnabled(states, IDM_COLOR_PIN)
        || IsCommandEnabled(states, IDM_BATCH_PIN)
        || IsCommandChecked(states, IDM_WORKSPACE_MIRROR)
        || IsCommandEnabled(states, IDM_DOCUMENT_CLOSE)
        || IsCommandEnabled(states, IDM_VIEW_CLOSE)
        || IsCommandEnabled(states, IDM_TAB_NEXT)
        || IsCommandEnabled(states, IDM_TAB_PREVIOUS)
        || IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || IsCommandEnabled(states, IDM_EDITOR_SPLIT_RIGHT)
        || IsCommandEnabled(states, IDM_EDITOR_SPLIT_DOWN)
        || IsCommandEnabled(states, IDM_EDITOR_MOVE_OTHER_GROUP)
        || IsCommandEnabled(states, IDM_EDITOR_NEW_VIEW_OTHER_GROUP)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_CLOSE)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_NEXT)
        || !IsCommandEnabled(states, IDM_WORKSPACE_NEW_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEW_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEW_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_FILE_RECENT_1)
        || !IsCommandEnabled(states, IDM_FILE_RESTORE_PREVIOUS)
        || IsCommandChecked(states, IDM_FILE_RESTORE_PREVIOUS)
        || !IsCommandEnabled(states, IDM_FILE_NEW)) {
        return 1;
    }

    inputs.application.restore_previous_documents = true;
    states = ComputeCommandStates(inputs);
    if (!IsCommandChecked(states, IDM_FILE_RESTORE_PREVIOUS)) {
        return 21;
    }

    inputs.document.has_document = true;
    inputs.document.has_saved_path = true;
    inputs.document.dirty = false;
    inputs.selection_view.document_count = 1U;
    inputs.selection_view.view_count = 1U;
    inputs.selection_view.active_group_view_count = 1U;
    inputs.workspace.locator_target_available = true;
    inputs.workspace.locator_visible = true;
    inputs.workspace.locator_pinned = true;
    inputs.workspace.locator_fixed = true;
    inputs.workspace.locator_auto_scroll = false;
    inputs.workspace.sequence_target_available = true;
    inputs.workspace.sequence_visible = true;
    inputs.workspace.sequence_pinned = true;
    inputs.workspace.light_table_target_available = true;
    inputs.workspace.light_table_visible = true;
    inputs.workspace.light_table_pinned = true;
    inputs.workspace.subpalette_target_available = true;
    inputs.workspace.subpalette_visible = true;
    inputs.workspace.subpalette_pinned = true;
    inputs.workspace.color_target_available = true;
    inputs.workspace.color_pinned = true;
    inputs.workspace.batch_target_available = true;
    inputs.workspace.batch_pinned = true;
    inputs.workspace.job_progress_visible = true;
    states = ComputeCommandStates(inputs);
    CommandStateInputs dirty_inputs = inputs;
    dirty_inputs.document.dirty = true;
    const CommandStateSet dirty_states = ComputeCommandStates(dirty_inputs);
    if (!SameStates(states, dirty_states)
        || !IsCommandEnabled(states, IDM_FILE_SAVE)
        || !IsCommandEnabled(states, IDM_FILE_COMPACT_COPY)
        || !IsCommandEnabled(states, IDM_FILE_REVERT)
        || !IsCommandEnabled(states, IDM_DOCUMENT_CLOSE)
        || !IsCommandEnabled(states, IDM_VIEW_CLOSE)
        || !IsCommandEnabled(states, IDM_EDITOR_SPLIT_RIGHT)
        || !IsCommandEnabled(states, IDM_EDITOR_SPLIT_DOWN)
        || !IsCommandEnabled(states, IDM_EDITOR_NEW_VIEW_OTHER_GROUP)
        || !IsCommandEnabled(states, IDM_WORKSPACE_NEW_WINDOW)
        || !IsCommandEnabled(states, IDM_VIEW_MOVE_NEW_WINDOW)
        || !IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEW_WINDOW)
        || !IsCommandEnabled(states, IDM_LOCATOR_PIN)
        || !IsCommandEnabled(states, IDM_LOCATOR_FIXED)
        || !IsCommandEnabled(states, IDM_LOCATOR_AUTOSCROLL)
        || !IsCommandChecked(states, IDM_WINDOW_LOCATOR)
        || !IsCommandChecked(states, IDM_LOCATOR_PIN)
        || !IsCommandChecked(states, IDM_LOCATOR_FIXED)
        || IsCommandChecked(states, IDM_LOCATOR_AUTOSCROLL)
        || !IsCommandChecked(states, IDM_WINDOW_SEQUENCE)
        || !IsCommandEnabled(states, IDM_SEQUENCE_PIN)
        || !IsCommandChecked(states, IDM_SEQUENCE_PIN)
        || !IsCommandChecked(states, IDM_WINDOW_LIGHT_TABLE)
        || !IsCommandChecked(states, IDM_WINDOW_JOB_PROGRESS)
        || !IsCommandEnabled(states, IDM_LIGHT_TABLE_PIN)
        || !IsCommandChecked(states, IDM_LIGHT_TABLE_PIN)
        || !IsCommandChecked(states, IDM_WINDOW_SUBPALETTE)
        || !IsCommandEnabled(states, IDM_SUBPALETTE_PIN)
        || !IsCommandChecked(states, IDM_SUBPALETTE_PIN)
        || !IsCommandEnabled(states, IDM_COLOR_PIN)
        || !IsCommandChecked(states, IDM_COLOR_PIN)
        || !IsCommandEnabled(states, IDM_BATCH_PIN)
        || !IsCommandChecked(states, IDM_BATCH_PIN)
        || IsCommandEnabled(states, IDM_EDITOR_MOVE_OTHER_GROUP)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_CLOSE)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_NEXT)
        || IsCommandEnabled(states, IDM_TAB_NEXT)
        || IsCommandEnabled(states, IDM_TAB_PREVIOUS)
        || IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)) {
        return 2;
    }

    inputs.document.recent_document_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_FILE_RECENT_1)
        || !IsCommandEnabled(states, IDM_FILE_RECENT_2)
        || IsCommandEnabled(states, IDM_FILE_RECENT_3)) {
        return 17;
    }

    inputs.selection_view.document_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_TAB_NEXT)
        || !IsCommandEnabled(states, IDM_TAB_PREVIOUS)
        || !IsCommandEnabled(states, IDM_VIEW_CLOSE)) {
        return 15;
    }
    inputs.selection_view.document_count = 1U;
    inputs.selection_view.view_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_VIEW_CLOSE)
        || !IsCommandEnabled(states, IDM_TAB_NEXT)
        || !IsCommandEnabled(states, IDM_TAB_PREVIOUS)) {
        return 16;
    }
    inputs.selection_view.view_count = 1U;

    inputs.selection_view.active_group_view_count = 3U;
    inputs.selection_view.active_tab_index = 1U;
    inputs.selection_view.workspace_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || !IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || !IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || !IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)) {
        return 19;
    }
    inputs.selection_view.active_tab_index = 0U;
    inputs.selection_view.workspace_count = 1U;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || !IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)) {
        return 20;
    }
    inputs.selection_view.active_group_view_count = 1U;
    inputs.selection_view.active_tab_index = 0U;

    inputs.selection_view.editor_group_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_EDITOR_SPLIT_RIGHT)
        || !IsCommandEnabled(states, IDM_EDITOR_SPLIT_DOWN)
        || !IsCommandEnabled(states, IDM_EDITOR_MOVE_OTHER_GROUP)
        || !IsCommandEnabled(states, IDM_EDITOR_NEW_VIEW_OTHER_GROUP)
        || !IsCommandEnabled(states, IDM_EDITOR_GROUP_CLOSE)
        || !IsCommandEnabled(states, IDM_EDITOR_GROUP_NEXT)) {
        return 18;
    }
    inputs.selection_view.editor_group_count = 1U;

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
    if (tools.editor.valid || tools.active_tool != 0U
        || tools.color_rgba != 0U) {
        return 11;
    }
    SetActiveCommandColor(tools, black);
    TransitionActiveTool(tools, nullptr, kInteractionFill);
    if (!SameColor(tools.drawing_color, black)) {
        return 12;
    }
    const InkpodColorValue sampled_fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_16, 1000U, 2000U, 3000U, 65535U};
    SetActiveCommandColor(tools, sampled_fill_color);
    TransitionActiveTool(tools, nullptr, kInteractionEyedropper);
    TransitionActiveTool(tools, nullptr, INKPOD_TOOL_PENCIL);
    if (!SameColor(tools.drawing_color, sampled_fill_color)) {
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
