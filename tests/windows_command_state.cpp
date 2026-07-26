#include <array>
#include <cstdint>

#include "app/app_context.h"
#include "app/resource.h"
#include "ui/command_state.h"
#include "ui/tools/tool_state.h"

namespace {

using inkpod::app::ToolUiState;
using inkpod::windows::ui::CommandStateInputs;
using inkpod::windows::ui::CommandStateOwner;
using inkpod::windows::ui::CommandStateSet;
using inkpod::windows::ui::ComputeCommandStates;
using inkpod::windows::ui::FindCommandState;
using inkpod::windows::ui::IsCommandChecked;
using inkpod::windows::ui::IsCommandEnabled;
using inkpod::windows::ui::kProductionCommandStateCount;
using inkpod::windows::ui::tools::HandleActivePlaneTransition;
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
    std::array<std::size_t, 10U> owner_counts{};
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

} // namespace

int main() {
    CommandStateInputs inputs{};
    CommandStateSet states = ComputeCommandStates(inputs);
    if (!CatalogHasExactlyOneOwner(states)
        || FindCommandState(states, IDM_HELP_ABOUT) == nullptr
        || IsCommandEnabled(states, IDM_FILE_SAVE)
        || IsCommandEnabled(states, IDM_VIEW_FIT)
        || IsCommandEnabled(states, IDM_VIEW_ONE_TO_ONE)
        || IsCommandEnabled(states, IDM_SELECTION_ALL)
        || IsCommandEnabled(states, IDM_FILTER_INVERT)
        || IsCommandEnabled(states, IDM_BATCH_ADD_COLOR_REPLACE)
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
    inputs.selection_view.active_tool = kInteractionVectorLine;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_VECTOR_LINE)
        || !IsCommandChecked(states, IDM_VECTOR_LINE)) {
        return 4;
    }
    inputs.tool.vector_stroke_plane = false;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_VECTOR_LINE)
        || inputs.tool.active_tool != kInteractionVectorLine) {
        return 5;
    }

    ToolUiState tools{};
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
