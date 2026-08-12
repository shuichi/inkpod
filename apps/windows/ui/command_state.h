#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

enum class CommandStateOwner : std::uint8_t {
    Batch,
    Document,
    Edit,
    Effects,
    DocumentPane,
    Animation,
    SelectionView,
    Tool,
    Color,
    Workspace,
    Application,
};

struct CommandState {
    UINT command{};
    CommandStateOwner owner{};
    bool enabled{true};
    bool checked{};
};

inline constexpr std::size_t kProductionCommandStateCount = 364U;
using CommandStateSet = std::array<CommandState, kProductionCommandStateCount>;

struct DocumentCommandStateInput {
    bool has_document{};
    bool has_saved_path{};
    bool dirty{};
    std::size_t recent_document_count{};
};

struct EditCommandStateInput {
    bool can_undo{};
    bool can_redo{};
    bool can_history_back{};
    bool can_history_forward{};
    bool clipboard_available{};
    bool floating_active{};
};

struct EffectsCommandStateInput {
    bool color_plane_active{};
    bool adjustment_available{};
    bool multiple_adjustments{};
    bool adjustment_visible{true};
    bool alpha_view{};
};

struct DocumentPaneCommandStateInput {
    bool removable_layer_available{};
    bool layer_palette_visible{};
};

struct AnimationCommandStateInput {
    std::uint32_t motion_fps{24U};
    bool sequence_switch_pending{};
};

struct SelectionViewCommandStateInput {
    std::uint32_t active_tool{};
    InkpodSelectionShape selection_shape{INKPOD_SELECTION_RECTANGLE};
    InkpodSelectionOperation selection_operation{INKPOD_SELECTION_NEW};
    bool flip_horizontal{};
    bool flip_vertical{};
    bool ruler_visible{};
    bool guides_visible{true};
    bool grid_visible{};
    bool snap_guides{};
    bool snap_grid{};
    bool transparent_visible{true};
    bool vector_antialias{true};
    InkpodVectorCenterlineMode vector_centerline_mode{INKPOD_VECTOR_CENTERLINE_HIDDEN};
    bool vector_endpoints_visible{};
    bool selection_layer_available{};
    std::size_t document_count{};
    std::size_t view_count{};
    std::size_t editor_group_count{1U};
    std::size_t active_group_view_count{};
    std::size_t active_tab_index{};
    std::size_t workspace_count{1U};
};

struct ToolCommandStateInput {
    std::uint32_t active_tool{};
    InkpodPlaneKind active_plane{INKPOD_PLANE_MAIN_LINE};
    InkpodFillOperation fill_operation{INKPOD_FILL_SEED};
    InkpodSelectionShape color_replace_shape{INKPOD_SELECTION_TRACE};
    InkpodVectorEraseMode vector_erase_mode{INKPOD_VECTOR_ERASE_PARTIAL};
    InkpodVectorSelectionMode vector_selection_mode{INKPOD_VECTOR_SELECT_TOUCHING};
    bool vector_stroke_plane{};
    bool geometry_drawable_plane{};
    bool palette_visible{};
};

struct ColorCommandStateInput {
    InkpodEyedropperSource eyedropper_source{INKPOD_EYEDROPPER_COMPOSITE};
    InkpodColorCheckMode color_check_mode{INKPOD_COLOR_CHECK_OFF};
    bool chart_locked{};
};

struct BatchCommandStateInput {
    bool idle{true};
    bool has_operations{};
    bool loaded_graph{};
    bool editable_item{};
    bool palette_visible{};
    InkpodBatchOutputPolicy output_policy{INKPOD_BATCH_OUTPUT_DUPLICATE};
    InkpodBatchFailurePolicy failure_policy{INKPOD_BATCH_FAILURE_CONTINUE};
};

struct WorkspaceCommandStateInput {
    bool tool_visible{true};
    bool tool_options_visible{true};
    bool color_visible{true};
    bool color_target_available{};
    bool color_pinned{};
    bool layer_visible{true};
    bool locator_visible{};
    bool locator_target_available{};
    bool locator_pinned{};
    bool locator_fixed{};
    bool locator_auto_scroll{true};
    bool sequence_visible{};
    bool sequence_target_available{};
    bool sequence_pinned{};
    bool light_table_visible{};
    bool light_table_target_available{};
    bool light_table_pinned{};
    bool subpalette_visible{};
    bool subpalette_target_available{};
    bool subpalette_pinned{};
    bool batch_target_available{};
    bool batch_pinned{};
    bool job_progress_visible{};
    bool mirrored{};
    std::uint32_t selected_workspace_preset{};
    bool locator_auto_hidden{};
    bool sequence_auto_hidden{};
    bool light_table_auto_hidden{};
    bool reference_auto_hidden{};
    bool batch_auto_hidden{};
};

struct ApplicationCommandStateInput {
    bool restore_previous_documents{};
    bool sequence_autosave_before_switch{};
};

struct CommandStateInputs {
    DocumentCommandStateInput document;
    EditCommandStateInput edit;
    EffectsCommandStateInput effects;
    DocumentPaneCommandStateInput document_pane;
    AnimationCommandStateInput animation;
    SelectionViewCommandStateInput selection_view;
    ToolCommandStateInput tool;
    ColorCommandStateInput color;
    BatchCommandStateInput batch;
    WorkspaceCommandStateInput workspace;
    ApplicationCommandStateInput application;
};

CommandStateSet ComputeCommandStates(const CommandStateInputs& inputs) noexcept;

const CommandState* FindCommandState(
    const CommandStateSet& states, UINT command) noexcept;

bool IsCommandEnabled(const CommandStateSet& states, UINT command) noexcept;
bool IsCommandChecked(const CommandStateSet& states, UINT command) noexcept;

void ApplyCommandStates(const CommandStateSet& states, HMENU menu) noexcept;

} // namespace inkpod::windows::ui
