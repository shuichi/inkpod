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
        {IDM_FILE_SAVE,
         IDM_FILE_SAVE_AS,
         IDM_FILE_COMPACT_COPY,
         IDM_FILE_AUTOSAVE_NOW,
         IDM_FILE_EXPORT_RASTER,
         IDM_FILE_EXPORT_INSTRUCTION_RASTER,
         IDM_CELL_SHOOTING_FRAME_PROPERTIES,
         IDM_CELL_VANISHING_POINT_PROPERTIES},
        input.has_document);
    SetEnabled(
        states,
        {IDM_CELL_SHOOTING_FRAME_EDIT_HANDLES,
         IDM_CELL_SHOOTING_FRAME_DELETE},
        input.has_document && input.shooting_frame_present);
    SetChecked(
        states,
        IDM_CELL_SHOOTING_FRAME_EDIT_HANDLES,
        input.has_document && input.shooting_frame_present
            && input.shooting_frame_handle_edit);
    SetEnabled(
        states,
        {IDM_CELL_VANISHING_POINT_EDIT_HANDLES,
         IDM_CELL_VANISHING_POINT_DELETE_ALL},
        input.has_document && input.vanishing_point_present);
    SetChecked(
        states,
        IDM_CELL_VANISHING_POINT_EDIT_HANDLES,
        input.has_document && input.vanishing_point_present
            && input.vanishing_point_handle_edit);
    SetEnabled(
        states,
        {IDM_FILE_REVERT, IDM_FILE_REVERT_PARTIAL},
        input.has_document && input.has_saved_path);
    constexpr std::array<UINT, 8U> recent_commands{
        IDM_FILE_RECENT_1,
        IDM_FILE_RECENT_2,
        IDM_FILE_RECENT_3,
        IDM_FILE_RECENT_4,
        IDM_FILE_RECENT_5,
        IDM_FILE_RECENT_6,
        IDM_FILE_RECENT_7,
        IDM_FILE_RECENT_8};
    for (std::size_t index = 0U; index < recent_commands.size(); ++index) {
        SetEnabled(
            states,
            recent_commands[index],
            index < input.recent_document_count);
    }
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
    SetEnabled(
        states,
        {IDM_SEQ_PREVIOUS, IDM_SEQ_NEXT, IDM_SEQ_GOTO},
        !input.sequence_switch_pending);
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
         IDM_SELECTION_OPTIONS,
         IDM_SELECTION_MODE_NEW,
         IDM_SELECTION_MODE_ADD,
         IDM_SELECTION_MODE_SUBTRACT,
         IDM_SELECTION_MODE_INTERSECT,
         IDM_SELECTION_COLOR,
         IDM_SELECTION_COLOR_DIFFERENT,
         IDM_SELECTION_COLOR_ADD,
         IDM_SELECTION_OUTPUT_COLOR_GUARD,
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
          IDM_VIEW_NEW,
          IDM_VIEW_MOVE_NEW_WINDOW,
          IDM_VIEW_DUPLICATE_NEW_WINDOW,
          IDM_EDITOR_SPLIT_RIGHT,
          IDM_EDITOR_SPLIT_DOWN,
          IDM_EDITOR_NEW_VIEW_OTHER_GROUP,
          IDM_DOCUMENT_CLOSE},
        input.document.has_document);
    SetEnabled(
        states,
        {IDM_VIEW_CLOSE},
        input.document.has_document);
    SetEnabled(
        states,
        {IDM_TAB_NEXT, IDM_TAB_PREVIOUS},
        input.document.has_document
            && (input.selection_view.document_count > 1U
                || input.selection_view.view_count > 1U));
    SetEnabled(
        states,
        {IDM_TAB_MOVE_LEFT},
        input.document.has_document
            && input.selection_view.active_group_view_count > 1U
            && input.selection_view.active_tab_index > 0U);
    SetEnabled(
        states,
        {IDM_TAB_MOVE_RIGHT},
        input.document.has_document
            && input.selection_view.active_group_view_count > 1U
            && input.selection_view.active_tab_index + 1U
                < input.selection_view.active_group_view_count);
    SetEnabled(
        states,
        {IDM_VIEW_MOVE_NEXT_WINDOW, IDM_VIEW_DUPLICATE_NEXT_WINDOW},
        input.document.has_document
            && input.selection_view.workspace_count > 1U);
    SetEnabled(
        states,
        {IDM_EDITOR_MOVE_OTHER_GROUP,
         IDM_EDITOR_GROUP_CLOSE,
         IDM_EDITOR_GROUP_NEXT},
        input.document.has_document
            && input.selection_view.editor_group_count == 2U);
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
    SetEnabled(
        states,
        {IDM_TOOL_COLOR_REPLACE_TARGET,
         IDM_TOOL_COLOR_REPLACE_PEN,
         IDM_TOOL_COLOR_REPLACE_RECTANGLE,
         IDM_TOOL_COLOR_REPLACE_POLYLINE,
         IDM_TOOL_COLOR_REPLACE_LASSO,
         IDM_TOOL_COLOR_REPLACE_ALL},
        input.document.has_document);
    SetUnchecked(
        states,
        {IDM_TOOL_COLOR_REPLACE_PEN,
         IDM_TOOL_COLOR_REPLACE_RECTANGLE,
         IDM_TOOL_COLOR_REPLACE_POLYLINE,
         IDM_TOOL_COLOR_REPLACE_LASSO});
    if (input.tool.active_tool == tools::kInteractionColorReplace) {
        SetChecked(
            states,
            input.tool.color_replace_shape == INKPOD_SELECTION_RECTANGLE
                ? IDM_TOOL_COLOR_REPLACE_RECTANGLE
                : (input.tool.color_replace_shape == INKPOD_SELECTION_POLYLINE
                          ? IDM_TOOL_COLOR_REPLACE_POLYLINE
                          : (input.tool.color_replace_shape == INKPOD_SELECTION_LASSO
                                    ? IDM_TOOL_COLOR_REPLACE_LASSO
                                    : IDM_TOOL_COLOR_REPLACE_PEN)),
            true);
    }
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
        && input.tool.active_tool != tools::kInteractionColorReplace
        && input.tool.active_tool != tools::kInteractionFloatingTransform
        && input.tool.active_tool != tools::kInteractionLightTableMove
        && input.tool.active_tool != tools::kInteractionShootingFrame
        && !tools::IsGeometryCanvasTool(input.tool.active_tool)
        && !(input.tool.active_tool >= tools::kInteractionEffectGradient
            && input.tool.active_tool <= tools::kInteractionEffectAlphaGradient);
    if (ordinary_tool) {
        SetChecked(states, tool_command, true);
    }

    SetEnabled(
        states,
        {IDM_GEOMETRY_LINE,
         IDM_GEOMETRY_CURVE,
         IDM_GEOMETRY_RECTANGLE,
         IDM_GEOMETRY_ELLIPSE,
         IDM_GEOMETRY_POLYGON,
         IDM_GEOMETRY_POLYLINE},
        input.tool.geometry_drawable_plane);
    SetUnchecked(
        states,
        {IDM_GEOMETRY_LINE,
         IDM_GEOMETRY_CURVE,
         IDM_GEOMETRY_RECTANGLE,
         IDM_GEOMETRY_ELLIPSE,
         IDM_GEOMETRY_POLYGON,
         IDM_GEOMETRY_POLYLINE});
    const UINT geometry_command =
        input.tool.active_tool == tools::kInteractionGeometryLine
        ? IDM_GEOMETRY_LINE
        : (input.tool.active_tool == tools::kInteractionGeometryCurve
              ? IDM_GEOMETRY_CURVE
              : (input.tool.active_tool == tools::kInteractionGeometryRectangle
                    ? IDM_GEOMETRY_RECTANGLE
                    : (input.tool.active_tool == tools::kInteractionGeometryEllipse
                          ? IDM_GEOMETRY_ELLIPSE
                          : (input.tool.active_tool == tools::kInteractionGeometryPolygon
                                ? IDM_GEOMETRY_POLYGON
                                : IDM_GEOMETRY_POLYLINE))));
    if (tools::IsGeometryCanvasTool(input.tool.active_tool)) {
        SetChecked(states, geometry_command, true);
    }

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
         IDM_BATCH_OUTPUT_FOLDER,
         IDM_BATCH_OUTPUT_ACTIVE_DOCUMENT,
         IDM_BATCH_OUTPUT_NEW_TABS,
         IDM_BATCH_OUTPUT_SETTINGS,
         IDM_BATCH_FAILURE_CONTINUE,
         IDM_BATCH_FAILURE_STOP},
        input.batch.idle);
    SetEnabled(states, IDM_BATCH_LOAD_SET, input.batch.idle);
    SetEnabled(
        states,
        {IDM_BATCH_ADD_COLOR_REPLACE,
         IDM_BATCH_ADD_MOVE_TO_COLOR_PLANE,
         IDM_BATCH_ADD_MASKING,
         IDM_BATCH_ADD_ERASE},
        input.batch.idle && input.document.has_document);
    SetEnabled(
        states,
        {IDM_BATCH_OPERATION_DUPLICATE,
         IDM_BATCH_OPERATION_REMOVE,
         IDM_BATCH_OPERATION_UP,
         IDM_BATCH_OPERATION_DOWN,
         IDM_BATCH_REPLACE_SWAP,
         IDM_BATCH_EXTRACT_PAIRS},
        input.batch.editable_item);
    SetEnabled(
        states,
        {IDM_BATCH_PREVIEW,
         IDM_BATCH_RUN_ALL,
         IDM_BATCH_SAVE_SET},
        input.batch.idle && input.batch.has_operations && input.document.has_document);
    SetEnabled(states, IDM_BATCH_CANCEL, !input.batch.idle);

    SetUnchecked(
        states,
        {IDM_BATCH_OUTPUT_FOLDER,
         IDM_BATCH_OUTPUT_ACTIVE_DOCUMENT,
         IDM_BATCH_OUTPUT_NEW_TABS,
         IDM_BATCH_FAILURE_CONTINUE,
         IDM_BATCH_FAILURE_STOP});
    SetChecked(
        states,
        input.batch.output_destination == INKPOD_BATCH_OUTPUT_ACTIVE_DOCUMENT
            ? IDM_BATCH_OUTPUT_ACTIVE_DOCUMENT
            : (input.batch.output_destination == INKPOD_BATCH_OUTPUT_NEW_TABS
                      ? IDM_BATCH_OUTPUT_NEW_TABS
                      : IDM_BATCH_OUTPUT_FOLDER),
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
    SetEnabled(states, IDM_WORKSPACE_NEW_WINDOW, true);
    SetChecked(states, IDM_WINDOW_TOOL_PALETTE, input.tool_visible);
    SetChecked(states, IDM_WINDOW_TOOL_OPTIONS, input.tool_options_visible);
    SetChecked(states, IDM_WINDOW_COLOR_PANE, input.color_visible);
    SetChecked(states, IDM_COLOR_PIN, input.color_pinned);
    SetEnabled(states, IDM_COLOR_PIN, input.color_target_available);
    SetChecked(states, IDM_WINDOW_LAYER_PALETTE, input.layer_visible);
    SetChecked(states, IDM_WINDOW_LOCATOR, input.locator_visible);
    SetChecked(states, IDM_LOCATOR_PIN, input.locator_pinned);
    SetChecked(states, IDM_LOCATOR_FIXED, input.locator_fixed);
    SetChecked(states, IDM_LOCATOR_AUTOSCROLL, input.locator_auto_scroll);
    SetEnabled(
        states,
        {IDM_LOCATOR_PIN, IDM_LOCATOR_FIXED, IDM_LOCATOR_AUTOSCROLL},
        input.locator_target_available);
    SetChecked(states, IDM_WINDOW_SEQUENCE, input.sequence_visible);
    SetChecked(states, IDM_SEQUENCE_PIN, input.sequence_pinned);
    SetEnabled(states, IDM_SEQUENCE_PIN, input.sequence_target_available);
    SetChecked(states, IDM_WINDOW_LIGHT_TABLE, input.light_table_visible);
    SetChecked(states, IDM_LIGHT_TABLE_PIN, input.light_table_pinned);
    SetEnabled(states, IDM_LIGHT_TABLE_PIN, input.light_table_target_available);
    SetChecked(states, IDM_WINDOW_SUBPALETTE, input.subpalette_visible);
    SetChecked(states, IDM_SUBPALETTE_PIN, input.subpalette_pinned);
    SetEnabled(states, IDM_SUBPALETTE_PIN, input.subpalette_target_available);
    SetChecked(states, IDM_BATCH_PIN, input.batch_pinned);
    SetEnabled(states, IDM_BATCH_PIN, input.batch_target_available);
    SetChecked(
        states, IDM_WINDOW_JOB_PROGRESS, input.job_progress_visible);
    SetChecked(states, IDM_WORKSPACE_MIRROR, input.mirrored);
    SetChecked(
        states, IDM_WORKSPACE_PRESET_COLORING,
        input.selected_workspace_preset == 0U);
    SetChecked(
        states, IDM_WORKSPACE_PRESET_LINE_CLEANUP,
        input.selected_workspace_preset == 1U);
    SetChecked(
        states, IDM_WORKSPACE_PRESET_REFERENCE,
        input.selected_workspace_preset == 2U);
    SetChecked(
        states, IDM_WORKSPACE_PRESET_BATCH,
        input.selected_workspace_preset == 3U);
    SetChecked(
        states, IDM_WORKSPACE_PRESET_FOCUS,
        input.selected_workspace_preset == 4U);
    SetChecked(
        states, IDM_WORKSPACE_AUTOHIDE_LOCATOR,
        input.locator_auto_hidden);
    SetChecked(
        states, IDM_WORKSPACE_AUTOHIDE_SEQUENCE,
        input.sequence_auto_hidden);
    SetChecked(
        states, IDM_WORKSPACE_AUTOHIDE_LIGHT_TABLE,
        input.light_table_auto_hidden);
    SetChecked(
        states, IDM_WORKSPACE_AUTOHIDE_REFERENCE,
        input.reference_auto_hidden);
    SetChecked(
        states, IDM_WORKSPACE_AUTOHIDE_BATCH,
        input.batch_auto_hidden);
}

void ProvideApplicationCommandStates(
    const ApplicationCommandStateInput& input,
    CommandStateSet& states) noexcept {
    SetChecked(
        states,
        IDM_FILE_RESTORE_PREVIOUS,
        input.restore_previous_documents);
    SetChecked(
        states,
        IDM_FILE_SEQUENCE_AUTOSAVE,
        input.sequence_autosave_before_switch);
    SetChecked(
        states,
        IDM_SEQ_WRAP_ENDPOINTS,
        input.sequence_wrap_endpoints);
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
    ProvideApplicationCommandStates(inputs.application, states);
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
