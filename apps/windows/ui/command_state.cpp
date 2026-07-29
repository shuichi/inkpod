#include "command_state.h"

#include <algorithm>
#include <initializer_list>
#include <iterator>

#include "app/resource.h"
#include "tools/tool_state.h"

namespace inkpod::windows::ui {
namespace {

constexpr CommandState kCommandCatalog[] = {
#define INKPOD_COMMAND_STATE(owner, command) \
    CommandState{command, CommandStateOwner::owner, true, false},
#include "command_state_catalog.inc"
#undef INKPOD_COMMAND_STATE
};

static_assert(std::size(kCommandCatalog) == kProductionCommandStateCount);

consteval bool CommandCatalogIsUnique() {
    for (std::size_t left = 0; left < std::size(kCommandCatalog); ++left) {
        for (std::size_t right = left + 1U; right < std::size(kCommandCatalog); ++right) {
            if (kCommandCatalog[left].command == kCommandCatalog[right].command) {
                return false;
            }
        }
    }
    return true;
}

static_assert(CommandCatalogIsUnique());

CommandState* FindMutable(CommandStateSet& states, UINT command) noexcept {
    const auto found = std::find_if(
        states.begin(), states.end(), [command](const CommandState& state) {
            return state.command == command;
        });
    return found == states.end() ? nullptr : &*found;
}

void SetEnabled(CommandStateSet& states, UINT command, bool enabled) noexcept {
    if (CommandState* state = FindMutable(states, command); state != nullptr) {
        state->enabled = enabled;
    }
}

void SetChecked(CommandStateSet& states, UINT command, bool checked) noexcept {
    if (CommandState* state = FindMutable(states, command); state != nullptr) {
        state->checked = checked;
    }
}

void SetEnabled(
    CommandStateSet& states,
    std::initializer_list<UINT> commands,
    bool enabled) noexcept {
    for (const UINT command : commands) {
        SetEnabled(states, command, enabled);
    }
}

void SetUnchecked(
    CommandStateSet& states, std::initializer_list<UINT> commands) noexcept {
    for (const UINT command : commands) {
        SetChecked(states, command, false);
    }
}

void ProvideDocumentCommandStates(
    const DocumentCommandStateInput& input, CommandStateSet& states) noexcept {
    SetEnabled(
        states,
        {IDM_FILE_SAVE, IDM_FILE_SAVE_AS, IDM_FILE_AUTOSAVE_NOW},
        input.has_document);
    SetEnabled(
        states,
        {IDM_FILE_REVERT, IDM_FILE_REVERT_PARTIAL},
        input.has_document && input.has_saved_path);
}

void ProvideEditCommandStates(
    const CommandStateInputs& input, CommandStateSet& states) noexcept {
    SetEnabled(states, IDM_EDIT_UNDO, input.document.has_document && input.edit.can_undo);
    SetEnabled(states, IDM_EDIT_REDO, input.document.has_document && input.edit.can_redo);
    SetEnabled(
        states,
        IDM_EDIT_HISTORY_BACK,
        input.document.has_document && input.edit.can_history_back);
    SetEnabled(
        states,
        IDM_EDIT_HISTORY_FORWARD,
        input.document.has_document && input.edit.can_history_forward);
    SetEnabled(
        states,
        {IDM_EDIT_COPY, IDM_EDIT_MIRROR_HORIZONTAL},
        input.document.has_document);
    SetEnabled(
        states,
        {IDM_EDIT_PASTE, IDM_EDIT_PASTE_SELECTED, IDM_EDIT_PASTE_CONVERTED},
        input.document.has_document && input.edit.clipboard_available
            && !input.edit.floating_active);
    SetEnabled(
        states,
        {IDM_EDIT_FLOATING_TRANSFORM, IDM_EDIT_FLOATING_COMMIT,
         IDM_EDIT_FLOATING_CANCEL},
        input.edit.floating_active);
}

void ProvideEffectsCommandStates(
    const CommandStateInputs& input, CommandStateSet& states) noexcept {
    const bool raster_effect_available =
        input.document.has_document && input.effects.color_plane_active;
    SetEnabled(
        states,
        {IDM_FILTER_LAST,
         IDM_FILTER_INVERT,
         IDM_FILTER_BLUR_WEAK,
         IDM_FILTER_SHARPEN_WEAK,
         IDM_FILTER_SHARPEN_STRONG,
         IDM_FILTER_BLUR_STRONG,
         IDM_FILTER_GAUSSIAN,
         IDM_FILTER_AUTO_CONTRAST,
         IDM_FILTER_BRIGHTNESS,
         IDM_FILTER_TONE_CURVE,
         IDM_FILTER_LEVELS,
         IDM_FILTER_HSV,
         IDM_FILTER_COLOR_BALANCE,
         IDM_FILTER_UNSHARP},
        raster_effect_available);
    SetEnabled(states, IDM_ADJUSTMENT_CREATE, raster_effect_available);
    SetEnabled(
        states,
        {IDM_ADJUSTMENT_EDIT, IDM_ADJUSTMENT_TOGGLE, IDM_ADJUSTMENT_MOVE_TOP},
        input.document.has_document && input.effects.adjustment_available);
    SetEnabled(
        states,
        {IDM_ADJUSTMENT_PREVIOUS, IDM_ADJUSTMENT_NEXT},
        input.document.has_document && input.effects.multiple_adjustments);
    SetChecked(states, IDM_ADJUSTMENT_TOGGLE, input.effects.adjustment_visible);
}

void ProvideDocumentPaneCommandStates(
    const CommandStateInputs& input, CommandStateSet& states) noexcept {
    SetChecked(
        states,
        IDM_WINDOW_LAYER_PALETTE,
        input.document_pane.layer_palette_visible);
    SetEnabled(
        states,
        {IDM_LAYER_DUPLICATE, IDM_LAYER_MOVE_TOP},
        input.document.has_document);
    SetEnabled(
        states,
        IDM_LAYER_DELETE,
        input.document.has_document && input.document_pane.removable_layer_available);
}

void ProvideAnimationCommandStates(
    const AnimationCommandStateInput& input, CommandStateSet& states) noexcept {
    SetUnchecked(
        states,
        {IDM_MOTION_FPS_30,
         IDM_MOTION_FPS_25,
         IDM_MOTION_FPS_24,
         IDM_MOTION_FPS_12,
         IDM_MOTION_FPS_10,
         IDM_MOTION_FPS_8});
    const UINT command = input.motion_fps == 30U
        ? IDM_MOTION_FPS_30
        : (input.motion_fps == 25U
                  ? IDM_MOTION_FPS_25
                  : (input.motion_fps == 24U
                            ? IDM_MOTION_FPS_24
                            : (input.motion_fps == 12U
                                      ? IDM_MOTION_FPS_12
                                      : (input.motion_fps == 10U ? IDM_MOTION_FPS_10
                                                                 : IDM_MOTION_FPS_8))));
    SetChecked(states, command, true);
}

void ProvideSelectionViewCommandStates(
    const CommandStateInputs& input, CommandStateSet& states) noexcept {
    SetEnabled(
        states,
        {IDM_SELECTION_ALL,
         IDM_SELECTION_CLEAR,
         IDM_SELECTION_RECTANGLE,
         IDM_SELECTION_ELLIPSE,
         IDM_SELECTION_LASSO,
         IDM_SELECTION_POLYLINE,
         IDM_SELECTION_TRACE,
         IDM_SELECTION_WAND,
         IDM_SELECTION_MODE_NEW,
         IDM_SELECTION_MODE_ADD,
         IDM_SELECTION_MODE_SUBTRACT,
         IDM_SELECTION_MODE_INTERSECT,
         IDM_SELECTION_COLOR,
         IDM_SELECTION_COLOR_DIFFERENT,
         IDM_SELECTION_COLOR_ADD,
         IDM_SELECTION_TO_LAYER,
         IDM_SELECTION_INVERT,
         IDM_SELECTION_EXPAND,
         IDM_SELECTION_SHRINK,
         IDM_VIEW_FIT,
         IDM_VIEW_ONE_TO_ONE,
         IDM_VIEW_ZOOM_PERCENT,
         IDM_VIEW_BOX_ZOOM,
         IDM_VIEW_FLIP_HORIZONTAL,
         IDM_VIEW_FLIP_VERTICAL,
         IDM_VIEW_RULER,
         IDM_VIEW_GUIDES,
         IDM_VIEW_GRID,
         IDM_VIEW_SNAP_GUIDES,
         IDM_VIEW_SNAP_GRID,
         IDM_VIEW_TRANSPARENT,
         IDM_VIEW_GUIDE_VERTICAL,
         IDM_VIEW_GUIDE_HORIZONTAL,
         IDM_VIEW_GUIDE_MOVE,
         IDM_VIEW_GUIDE_DELETE_ALL,
         IDM_VIEW_GRID_SETTINGS,
         IDM_VIEW_NEW},
        input.document.has_document);
    SetEnabled(
        states,
        {IDM_SELECTION_FROM_LAYER, IDM_SELECTION_LAYER_ADD,
         IDM_SELECTION_LAYER_SUBTRACT},
        input.document.has_document && input.selection_view.selection_layer_available);

    SetChecked(states, IDM_VIEW_FLIP_HORIZONTAL, input.selection_view.flip_horizontal);
    SetChecked(states, IDM_VIEW_FLIP_VERTICAL, input.selection_view.flip_vertical);
    SetChecked(states, IDM_VIEW_GRID, input.selection_view.grid_visible);
    SetChecked(states, IDM_VIEW_RULER, input.selection_view.ruler_visible);
    SetChecked(states, IDM_VIEW_GUIDES, input.selection_view.guides_visible);
    SetChecked(states, IDM_VIEW_SNAP_GUIDES, input.selection_view.snap_guides);
    SetChecked(states, IDM_VIEW_SNAP_GRID, input.selection_view.snap_grid);
    SetChecked(states, IDM_VIEW_TRANSPARENT, input.selection_view.transparent_visible);
    SetChecked(
        states,
        IDM_VIEW_BOX_ZOOM,
        input.selection_view.active_tool == tools::kInteractionBoxZoom);

    SetUnchecked(
        states,
        {IDM_SELECTION_RECTANGLE,
         IDM_SELECTION_ELLIPSE,
         IDM_SELECTION_LASSO,
         IDM_SELECTION_POLYLINE,
         IDM_SELECTION_TRACE,
         IDM_SELECTION_WAND,
         IDM_SELECTION_MODE_NEW,
         IDM_SELECTION_MODE_ADD,
         IDM_SELECTION_MODE_SUBTRACT,
         IDM_SELECTION_MODE_INTERSECT});
    const UINT shape_command = input.selection_view.selection_shape == INKPOD_SELECTION_ELLIPSE
        ? IDM_SELECTION_ELLIPSE
        : (input.selection_view.selection_shape == INKPOD_SELECTION_LASSO
                  ? IDM_SELECTION_LASSO
                  : (input.selection_view.selection_shape == INKPOD_SELECTION_POLYLINE
                            ? IDM_SELECTION_POLYLINE
                            : (input.selection_view.selection_shape == INKPOD_SELECTION_TRACE
                                      ? IDM_SELECTION_TRACE
                                      : (input.selection_view.selection_shape
                                                    == INKPOD_SELECTION_WAND
                                                ? IDM_SELECTION_WAND
                                                : IDM_SELECTION_RECTANGLE))));
    const UINT operation_command = input.selection_view.selection_operation
            == INKPOD_SELECTION_ADD
        ? IDM_SELECTION_MODE_ADD
        : (input.selection_view.selection_operation == INKPOD_SELECTION_SUBTRACT
                  ? IDM_SELECTION_MODE_SUBTRACT
                  : (input.selection_view.selection_operation == INKPOD_SELECTION_INTERSECT
                            ? IDM_SELECTION_MODE_INTERSECT
                            : IDM_SELECTION_MODE_NEW));
    SetChecked(states, shape_command, true);
    SetChecked(states, operation_command, true);
}

void ProvideToolCommandStates(
    const CommandStateInputs& input, CommandStateSet& states) noexcept {
    SetChecked(states, IDM_WINDOW_TOOL_PALETTE, input.tool.palette_visible);
    SetUnchecked(
        states,
        {IDM_TOOL_PENCIL,
         IDM_TOOL_BRUSH,
         IDM_TOOL_ERASER,
         IDM_TOOL_FILL,
         IDM_TOOL_CLOSED_FILL,
         IDM_TOOL_FILL_EXTENSION,
         IDM_TOOL_EYEDROPPER,
         IDM_PLANE_MAIN_LINE,
         IDM_PLANE_COLOR});
    const UINT tool_command = input.tool.active_tool == INKPOD_TOOL_PENCIL
        ? IDM_TOOL_PENCIL
        : (input.tool.active_tool == INKPOD_TOOL_BRUSH
                  ? IDM_TOOL_BRUSH
                  : (input.tool.active_tool == INKPOD_TOOL_ERASER
                            ? IDM_TOOL_ERASER
                            : (input.tool.active_tool == tools::kInteractionFill
                                      ? (input.tool.fill_operation == INKPOD_FILL_CLOSED_REGION
                                                ? IDM_TOOL_CLOSED_FILL
                                                : (input.tool.fill_operation
                                                            == INKPOD_FILL_EXTENSION
                                                      ? IDM_TOOL_FILL_EXTENSION
                                                      : IDM_TOOL_FILL))
                                      : IDM_TOOL_EYEDROPPER)));
    const bool ordinary_tool = input.tool.active_tool != tools::kInteractionBoxZoom
        && input.tool.active_tool != tools::kInteractionGuideMove
        && input.tool.active_tool != tools::kInteractionSelection
        && input.tool.active_tool != tools::kInteractionFloatingTransform
        && input.tool.active_tool != tools::kInteractionLightTableMove
        && !tools::IsVectorCanvasTool(input.tool.active_tool)
        && !(input.tool.active_tool >= tools::kInteractionEffectGradient
            && input.tool.active_tool <= tools::kInteractionEffectAlphaGradient);
    if (ordinary_tool) {
        SetChecked(states, tool_command, true);
    }

    SetEnabled(
        states,
        {IDM_VECTOR_LINE,
         IDM_VECTOR_CURVE,
         IDM_VECTOR_RECTANGLE,
         IDM_VECTOR_ELLIPSE,
         IDM_VECTOR_POLYLINE,
         IDM_VECTOR_ERASER},
        input.tool.vector_stroke_plane);
    SetUnchecked(
        states,
        {IDM_VECTOR_LINE,
         IDM_VECTOR_CURVE,
         IDM_VECTOR_RECTANGLE,
         IDM_VECTOR_ELLIPSE,
         IDM_VECTOR_POLYLINE,
         IDM_VECTOR_ERASER});
    const UINT vector_command = input.tool.active_tool == tools::kInteractionVectorLine
        ? IDM_VECTOR_LINE
        : (input.tool.active_tool == tools::kInteractionVectorCurve
                  ? IDM_VECTOR_CURVE
                  : (input.tool.active_tool == tools::kInteractionVectorRectangle
                            ? IDM_VECTOR_RECTANGLE
                            : (input.tool.active_tool == tools::kInteractionVectorEllipse
                                      ? IDM_VECTOR_ELLIPSE
                                      : (input.tool.active_tool
                                                    == tools::kInteractionVectorPolyline
                                                ? IDM_VECTOR_POLYLINE
                                                : IDM_VECTOR_ERASER))));
    if (tools::IsVectorCanvasTool(input.tool.active_tool)) {
        SetChecked(states, vector_command, true);
    }

    SetUnchecked(
        states,
        {IDM_VECTOR_ERASE_PARTIAL, IDM_VECTOR_ERASE_INTERSECTION,
         IDM_VECTOR_ERASE_WHOLE});
    SetChecked(
        states,
        input.tool.vector_erase_mode == INKPOD_VECTOR_ERASE_TO_INTERSECTION
            ? IDM_VECTOR_ERASE_INTERSECTION
            : (input.tool.vector_erase_mode == INKPOD_VECTOR_ERASE_WHOLE_PATH
                      ? IDM_VECTOR_ERASE_WHOLE
                      : IDM_VECTOR_ERASE_PARTIAL),
        true);
    SetUnchecked(
        states,
        {IDM_VECTOR_SELECT_CUT,
         IDM_VECTOR_SELECT_TOUCH,
         IDM_VECTOR_SELECT_CONTAINED,
         IDM_VECTOR_SELECT_LINE,
         IDM_VECTOR_SELECT_WHOLE_LINE,
         IDM_VECTOR_SELECT_INTERSECTION,
         IDM_VECTOR_SELECT_FILL_BOUNDARY,
         IDM_VECTOR_SELECT_FILL});
    const UINT vector_selection_command =
        input.tool.vector_selection_mode == INKPOD_VECTOR_SELECT_CUT_BY_SELECTION
        ? IDM_VECTOR_SELECT_CUT
        : (input.tool.vector_selection_mode == INKPOD_VECTOR_SELECT_FULLY_CONTAINED
                  ? IDM_VECTOR_SELECT_CONTAINED
                  : (input.tool.vector_selection_mode == INKPOD_VECTOR_SELECT_LINE
                            ? IDM_VECTOR_SELECT_LINE
                            : (input.tool.vector_selection_mode
                                          == INKPOD_VECTOR_SELECT_WHOLE_LINE
                                      ? IDM_VECTOR_SELECT_WHOLE_LINE
                                      : (input.tool.vector_selection_mode
                                                    == INKPOD_VECTOR_SELECT_TO_INTERSECTION
                                                ? IDM_VECTOR_SELECT_INTERSECTION
                                                : (input.tool.vector_selection_mode
                                                              == INKPOD_VECTOR_SELECT_FILL_BOUNDARY
                                                          ? IDM_VECTOR_SELECT_FILL_BOUNDARY
                                                          : (input.tool.vector_selection_mode
                                                                        == INKPOD_VECTOR_SELECT_FILL
                                                                    ? IDM_VECTOR_SELECT_FILL
                                                                    : IDM_VECTOR_SELECT_TOUCH))))));
    SetChecked(states, vector_selection_command, true);
    SetChecked(
        states,
        input.tool.active_plane == INKPOD_PLANE_MAIN_LINE ? IDM_PLANE_MAIN_LINE
                                                          : IDM_PLANE_COLOR,
        true);

    const bool effect_available =
        input.document.has_document && input.effects.color_plane_active;
    SetEnabled(
        states,
        {IDM_EFFECT_GRADIENT,
         IDM_EFFECT_AIRBRUSH,
         IDM_EFFECT_BOUNDARY_AIRBRUSH,
         IDM_EFFECT_BLUR,
         IDM_EFFECT_STAMP,
         IDM_EFFECT_DUST,
         IDM_EFFECT_ALPHA_GRADIENT,
         IDM_EFFECT_ALPHA_VIEW},
        effect_available);
    SetUnchecked(
        states,
        {IDM_EFFECT_GRADIENT,
         IDM_EFFECT_AIRBRUSH,
         IDM_EFFECT_BOUNDARY_AIRBRUSH,
         IDM_EFFECT_BLUR,
         IDM_EFFECT_STAMP,
         IDM_EFFECT_DUST,
         IDM_EFFECT_ALPHA_GRADIENT});
    const UINT effect_command = input.tool.active_tool == tools::kInteractionEffectGradient
        ? IDM_EFFECT_GRADIENT
        : (input.tool.active_tool == tools::kInteractionEffectAirbrush
                  ? IDM_EFFECT_AIRBRUSH
                  : (input.tool.active_tool == tools::kInteractionEffectBlur
                            ? IDM_EFFECT_BLUR
                            : (input.tool.active_tool == tools::kInteractionEffectStamp
                                      ? IDM_EFFECT_STAMP
                                      : (input.tool.active_tool == tools::kInteractionEffectDust
                                                ? IDM_EFFECT_DUST
                                                : (input.tool.active_tool
                                                              == tools::kInteractionEffectAlphaGradient
                                                          ? IDM_EFFECT_ALPHA_GRADIENT
                                                          : 0U)))));
    if (effect_command != 0U) {
        SetChecked(states, effect_command, true);
    }
    SetChecked(states, IDM_EFFECT_ALPHA_VIEW, input.effects.alpha_view);
}

void ProvideColorCommandStates(
    const ColorCommandStateInput& input, CommandStateSet& states) noexcept {
    SetUnchecked(
        states,
        {IDM_COLOR_CHECK_OFF, IDM_COLOR_CHECK_LEGACY, IDM_COLOR_CHECK_NATIVE});
    const UINT color_check = input.color_check_mode == INKPOD_COLOR_CHECK_LEGACY_WHITE
        ? IDM_COLOR_CHECK_LEGACY
        : (input.color_check_mode == INKPOD_COLOR_CHECK_NATIVE_ALPHA
                  ? IDM_COLOR_CHECK_NATIVE
                  : IDM_COLOR_CHECK_OFF);
    SetChecked(states, color_check, true);

    SetUnchecked(
        states,
        {IDM_COLOR_SOURCE_TOPMOST,
         IDM_COLOR_SOURCE_SELECTED,
         IDM_COLOR_SOURCE_COMPOSITE,
         IDM_COLOR_SOURCE_LIGHT_TABLE});
    const UINT source = input.eyedropper_source == INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT
        ? IDM_COLOR_SOURCE_TOPMOST
        : (input.eyedropper_source == INKPOD_EYEDROPPER_SELECTED_PLANE
                  ? IDM_COLOR_SOURCE_SELECTED
                  : (input.eyedropper_source == INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST
                            ? IDM_COLOR_SOURCE_LIGHT_TABLE
                            : IDM_COLOR_SOURCE_COMPOSITE));
    SetChecked(states, source, true);
    SetChecked(states, IDM_CHART_LOCK, input.chart_locked);
}

void ProvideBatchCommandStates(
    const CommandStateInputs& input, CommandStateSet& states) noexcept {
    SetChecked(states, IDM_WINDOW_BATCH, input.batch.palette_visible);
    SetEnabled(
        states,
        {IDM_BATCH_INPUT_FILE,
         IDM_BATCH_INPUT_FOLDER,
         IDM_BATCH_INPUT_CURRENT,
         IDM_BATCH_INPUT_RANGE,
         IDM_BATCH_OUTPUT_DUPLICATE,
         IDM_BATCH_OUTPUT_NEW,
         IDM_BATCH_OUTPUT_OVERWRITE,
         IDM_BATCH_OUTPUT_SETTINGS,
         IDM_BATCH_FAILURE_CONTINUE,
         IDM_BATCH_FAILURE_STOP},
        input.batch.idle && !input.batch.loaded_graph);
    SetEnabled(states, IDM_BATCH_LOAD_SET, input.batch.idle);
    SetEnabled(
        states,
        {IDM_BATCH_ADD_COLOR_REPLACE,
         IDM_BATCH_ADD_CONTINUOUS_FILL,
         IDM_BATCH_ADD_SEPARATION,
         IDM_BATCH_ADD_VISIBILITY,
         IDM_BATCH_ADD_LINE_WIDTH,
         IDM_BATCH_ADD_BOUNDARY_AIRBRUSH,
         IDM_BATCH_ADD_DUST,
         IDM_BATCH_ADD_MIRROR,
         IDM_BATCH_ADD_ROTATE,
         IDM_BATCH_ADD_RESIZE,
         IDM_BATCH_ADD_CONVERT,
         IDM_BATCH_ADD_FILTER_SHARPEN_WEAK,
         IDM_BATCH_ADD_FILTER_SHARPEN_STRONG,
         IDM_BATCH_ADD_FILTER_BLUR_WEAK,
         IDM_BATCH_ADD_FILTER_BLUR_STRONG,
         IDM_BATCH_ADD_FILTER_GAUSSIAN,
         IDM_BATCH_ADD_FILTER_INVERT,
         IDM_BATCH_ADD_FILTER_AUTO_CONTRAST,
         IDM_BATCH_ADD_FILTER_BRIGHTNESS,
         IDM_BATCH_ADD_FILTER_TONE_CURVE,
         IDM_BATCH_ADD_FILTER_LEVELS,
         IDM_BATCH_ADD_FILTER_HSV,
         IDM_BATCH_ADD_FILTER_COLOR_BALANCE,
         IDM_BATCH_ADD_FILTER_UNSHARP},
        input.batch.idle && input.document.has_document);
    SetEnabled(
        states,
        {IDM_BATCH_OPERATION_EDIT,
         IDM_BATCH_OPERATION_REMOVE,
         IDM_BATCH_OPERATION_UP,
         IDM_BATCH_OPERATION_DOWN,
         IDM_BATCH_REPLACE_SWAP},
        input.batch.editable_item);
    SetEnabled(
        states,
        {IDM_BATCH_PREVIEW,
         IDM_BATCH_DRY_RUN,
         IDM_BATCH_RUN_CURRENT,
         IDM_BATCH_RUN_ALL,
         IDM_BATCH_SAVE_SET},
        input.batch.idle && input.batch.has_operations && input.document.has_document);
    SetEnabled(states, IDM_BATCH_CANCEL, !input.batch.idle);

    SetUnchecked(
        states,
        {IDM_BATCH_OUTPUT_DUPLICATE,
         IDM_BATCH_OUTPUT_NEW,
         IDM_BATCH_OUTPUT_OVERWRITE,
         IDM_BATCH_FAILURE_CONTINUE,
         IDM_BATCH_FAILURE_STOP});
    SetChecked(
        states,
        input.batch.output_policy == INKPOD_BATCH_OUTPUT_NEW_SAVE
            ? IDM_BATCH_OUTPUT_NEW
            : (input.batch.output_policy == INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE
                      ? IDM_BATCH_OUTPUT_OVERWRITE
                      : IDM_BATCH_OUTPUT_DUPLICATE),
        true);
    SetChecked(
        states,
        input.batch.failure_policy == INKPOD_BATCH_FAILURE_STOP
            ? IDM_BATCH_FAILURE_STOP
            : IDM_BATCH_FAILURE_CONTINUE,
        true);
}

void ProvideWorkspaceCommandStates(
    const WorkspaceCommandStateInput& input,
    CommandStateSet& states) noexcept {
    SetChecked(states, IDM_WINDOW_TOOL_PALETTE, input.tool_visible);
    SetChecked(states, IDM_WINDOW_TOOL_OPTIONS, input.tool_options_visible);
    SetChecked(states, IDM_WINDOW_COLOR_PANE, input.color_visible);
    SetChecked(states, IDM_WINDOW_LAYER_PALETTE, input.layer_visible);
    SetChecked(states, IDM_WORKSPACE_MIRROR, input.mirrored);
}

} // namespace

CommandStateSet ComputeCommandStates(const CommandStateInputs& inputs) noexcept {
    CommandStateSet states{};
    std::copy(std::begin(kCommandCatalog), std::end(kCommandCatalog), states.begin());
    ProvideBatchCommandStates(inputs, states);
    ProvideDocumentCommandStates(inputs.document, states);
    ProvideEditCommandStates(inputs, states);
    ProvideEffectsCommandStates(inputs, states);
    ProvideDocumentPaneCommandStates(inputs, states);
    ProvideAnimationCommandStates(inputs.animation, states);
    ProvideSelectionViewCommandStates(inputs, states);
    ProvideToolCommandStates(inputs, states);
    ProvideColorCommandStates(inputs.color, states);
    ProvideWorkspaceCommandStates(inputs.workspace, states);
    return states;
}

const CommandState* FindCommandState(
    const CommandStateSet& states, UINT command) noexcept {
    const auto found = std::find_if(
        states.begin(), states.end(), [command](const CommandState& state) {
            return state.command == command;
        });
    return found == states.end() ? nullptr : &*found;
}

bool IsCommandEnabled(const CommandStateSet& states, UINT command) noexcept {
    const CommandState* state = FindCommandState(states, command);
    return state != nullptr && state->enabled;
}

bool IsCommandChecked(const CommandStateSet& states, UINT command) noexcept {
    const CommandState* state = FindCommandState(states, command);
    return state != nullptr && state->checked;
}

void ApplyCommandStates(const CommandStateSet& states, HMENU menu) noexcept {
    for (const CommandState& state : states) {
        if (menu != nullptr) {
            EnableMenuItem(
                menu,
                state.command,
                MF_BYCOMMAND | (state.enabled ? MF_ENABLED : MF_GRAYED));
            CheckMenuItem(
                menu,
                state.command,
                MF_BYCOMMAND | (state.checked ? MF_CHECKED : MF_UNCHECKED));
        }
    }
}

} // namespace inkpod::windows::ui
