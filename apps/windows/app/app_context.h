#pragma once

#include <windows.h>

#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "core_engine.h"
#include "inkpod/core_ffi.h"
#include "ui/command_state.h"
#include "ui/dialogs/basic_dialogs.h"
#include "ui/dialogs/batch_dialog.h"
#include "ui/dialogs/effects_dialogs.h"
#include "ui/main_window.h"

namespace inkpod::app {

struct GradientStopValue {
    std::uint32_t position_milli{};
    std::uint32_t rgba{};
};

struct CanvasEffectOptions {
    std::array<std::int32_t, 5U> parameters{};
    std::uint32_t shape{INKPOD_SELECTION_TRACE};
    std::uint32_t mode{};
    bool option{};
    bool option2{};
    std::vector<GradientStopValue> stops;
};

struct FilterJob {
    std::uint32_t kind{INKPOD_FILTER_INVERT};
    std::uint32_t channel{INKPOD_FILTER_CHANNEL_RGB};
    std::uint32_t interpolation{INKPOD_CURVE_BEZIER};
    std::array<std::int32_t, 5U> parameters{};
    std::vector<InkpodCurvePoint> points;
    std::uint64_t plane_id{};
    bool preview{};
};

struct AdjustmentLayerUiState {
    std::uint64_t id{};
    bool visible{true};
    FilterJob job;
    std::string name;
};

struct BatchOperationUi {
    std::uint32_t kind{INKPOD_BATCH_OPERATION_COLOR_REPLACE};
    std::uint64_t flags{INKPOD_BATCH_OPERATION_ENABLED};
    std::uint64_t layer_id{};
    std::uint64_t plane_id{};
    InkpodLayerKind layer_kind{INKPOD_LAYER_BINARY_COLORING};
    InkpodTypedPlaneKind plane_kind{INKPOD_TYPED_PLANE_COLOR};
    InkpodBatchMissingPolicy missing_policy{INKPOD_BATCH_MISSING_ERROR};
    std::array<std::int64_t, 8U> parameters{};
    InkpodColorValue color_0{};
    InkpodColorValue color_1{};
    std::vector<InkpodColorValue> colors;
    std::vector<InkpodBatchColorPairInput> color_pairs;
    std::vector<InkpodBatchSeedInput> seeds;
    FilterJob filter;
    std::wstring label;
};

struct AppLifetimeState {
    HINSTANCE instance{};
    bool smoke_test{};
    std::wstring smoke_raster_path;
    std::vector<std::wstring> smoke_sequence_paths;
};

struct DocumentShellState {
    std::wstring current_path;
    std::wstring recovery_path;
    InkpodClipboard* clipboard{};
    std::uint64_t smoke_layer_id{};
    std::uint64_t selection_layer_id{};
};

struct ToolUiState {
    std::uint32_t active_tool{INKPOD_TOOL_PENCIL};
    InkpodPlaneKind active_plane{INKPOD_PLANE_MAIN_LINE};
    std::uint32_t color_rgba{UINT32_C(0xdc281eff)};
    InkpodColorValue drawing_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 220U, 40U, 30U, 255U};
    InkpodEyedropperSource eyedropper_source{INKPOD_EYEDROPPER_COMPOSITE};
    float diameter{8.0F};

    bool floating_active{};
    InkpodFloatingTransform floating_transform{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};
    InkpodFrameRect floating_bounds{};
    std::vector<InkpodStrokeSample> floating_gesture_samples;
    std::uint32_t floating_drag_mode{};
    InkpodFloatingTransform floating_drag_start{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};

    windows::ui::FillToolOptions fill_options{};
    std::vector<InkpodStrokeSample> fill_gesture_samples;
    InkpodSelectionShape selection_shape{INKPOD_SELECTION_RECTANGLE};
    InkpodSelectionOperation selection_operation{INKPOD_SELECTION_NEW};
    std::uint16_t selection_tolerance{};
    std::uint16_t selection_gap_close{};
    float selection_diameter{8.0F};
    std::vector<InkpodStrokeSample> selection_gesture_samples;
    std::vector<InkpodStrokeSample> vector_gesture_samples;
    InkpodVectorEraseMode vector_erase_mode{INKPOD_VECTOR_ERASE_PARTIAL};
    InkpodVectorSelectionMode vector_selection_mode{INKPOD_VECTOR_SELECT_TOUCHING};
    std::vector<std::uint64_t> vector_selected_path_ids;
};

struct ViewUiState {
    InkpodColorCheckMode color_check_mode{INKPOD_COLOR_CHECK_OFF};
    std::uint64_t secondary_view_id{};
    std::uint64_t active_view_id{};
    bool flip_horizontal{};
    bool flip_vertical{};
    bool ruler_visible{};
    bool guides_visible{true};
    bool grid_visible{};
    bool snap_guides{};
    bool snap_grid{};
    bool transparent_visible{true};
    std::vector<InkpodStrokeSample> gesture_samples;
    bool guide_drag_active{};
    std::uint32_t guide_drag_axis{};
    std::uint64_t guide_drag_id{};
};

struct PaneUiState {
    std::vector<InkpodColorValue> palette_colors;
    std::uint32_t palette_group{};
    std::uint32_t selected_palette_index{};
    std::vector<std::wstring> color_chart_names;
    std::uint32_t color_chart_page{};
    std::uint32_t selected_color_chart_index{};
    bool color_chart_locked{};
    std::uint64_t active_tree_layer_id{};
    std::uint64_t active_tree_plane_id{};
    std::uint32_t active_tree_layer_index{};
    std::uint32_t active_tree_plane_index{};
    std::uint32_t tree_layer_count{};
    std::uint32_t tree_plane_count{};
    std::uint64_t active_light_table_set_id{};
    std::uint64_t active_light_table_item_id{};
    std::uint32_t active_light_table_set_index{};
    std::uint32_t active_light_table_item_index{};
    std::uint32_t light_table_set_count{};
    std::uint32_t light_table_item_count{};
    std::uint32_t sequence_count{};
    std::vector<InkpodStrokeSample> light_table_move_samples;
};

struct AnimationUiState {
    std::uint32_t active_sequence_index{};
    std::uint32_t motion_fps{24U};
    std::uint64_t motion_flags{INKPOD_MOTION_FLAG_LOOP};
    bool motion_active{};
    bool motion_paused{};
};

struct EffectsUiState {
    std::uint64_t adjustment_id{};
    bool adjustment_visible{true};
    std::vector<AdjustmentLayerUiState> adjustments;
    InkpodTask* task{};
    HWND progress{};
    windows::ui::ProgressDialogState progress_dialog{};
    bool preview_prompt{};
    bool alpha_view{};
    bool stamp_source_valid{};
    InkpodStrokeSample stamp_source{};
    CanvasEffectOptions options{};
    std::vector<InkpodStrokeSample> samples;
    bool airbrush_active{};
    InkpodStrokeSample airbrush_last{};
};

struct BatchUiState {
    InkpodBatchInputKind input_kind{INKPOD_BATCH_INPUT_CURRENT_SEQUENCE};
    std::wstring input_path;
    std::uint32_t first_cell{};
    std::uint32_t last_cell{};
    std::vector<BatchOperationUi> operations;
    std::uint32_t selected_operation{};
    InkpodBatchOutputPolicy output_policy{INKPOD_BATCH_OUTPUT_DUPLICATE};
    InkpodBatchFailurePolicy failure_policy{INKPOD_BATCH_FAILURE_CONTINUE};
    std::wstring output_folder{L"."};
    std::wstring basename{L"batch"};
    std::uint32_t start_number{1U};
    std::uint32_t wait_milliseconds{};
    bool descending{};
    bool cell_folder{};
    bool preview_before_save{};
    bool loaded_graph{};
    std::wstring last_result;

    HWND palette{};
    windows::ui::BatchPaletteDialogState palette_dialog{};
    InkpodBatchGraph* graph{};
    InkpodBatchPreview* preview{};
    InkpodBatchReport* report{};
    InkpodBatchTask* task{};
    HWND progress{};
    windows::ui::ProgressDialogState progress_dialog{};
};

struct AppContext {
    AppLifetimeState lifetime;
    MainWindowHandles windows;
    DocumentShellState document;
    ToolUiState tools;
    ViewUiState view;
    PaneUiState panes;
    AnimationUiState animation;
    EffectsUiState effects;
    BatchUiState batch;
    windows::ui::CommandStateSet command_states;
    std::unique_ptr<CoreEngine> engine;
};

} // namespace inkpod::app
