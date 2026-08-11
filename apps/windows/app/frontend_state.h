#pragma once

#include <windows.h>

#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#include "command_context.h"
#include "pane_target.h"
#include "inkpod/core_ffi.h"
#include "session_recovery.h"
#include "ui/command_state.h"
#include "ui/dialogs/basic_dialogs.h"
#include "ui/dialogs/batch_dialog.h"
#include "ui/dialogs/effects_dialogs.h"
#include "ui/dialogs/layer_palette.h"
#include "ui/dialogs/tool_palette.h"
#include "ui/panes/color_dock_pane.h"
#include "ui/panes/tool_options_pane.h"
#include "ui/shortcut_controller.h"
#include "ui/tools/tool_state.h"

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

enum class FilterPreviewWork : std::uint8_t {
    None,
    Begin,
    Update,
    Apply,
    Cancel,
};

struct InteractiveFilterPreviewUiState {
    CommandContext context;
    std::uint32_t kind{};
    std::uint64_t plane_id{};
    std::optional<FilterJob> pending;
    std::uint64_t desired_generation{};
    std::uint64_t pending_generation{};
    std::uint64_t running_generation{};
    FilterPreviewWork work{FilterPreviewWork::None};
    bool session_active{};
    bool dialog_active{};
    bool accept_requested{};
    bool cancel_requested{};
    HWND dialog{};

    // Fixed-size smoke observations avoid adding an unbounded diagnostic queue.
    std::uint64_t completed_updates{};
    std::array<std::uint64_t, 8U> smoke_checksums{};
    std::size_t smoke_checksum_count{};
    bool smoke_cancel_next{};
};

struct LocatorAsyncResult {
    PostedNotificationToken token;
    CommandContext context;
    std::uint64_t sample_generation{};
    InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
    InkpodLocatorOutput output{};
    InkpodLocatorNeighborhoodBuffer neighborhood_output{};
    std::array<std::uint8_t, 9U * 9U * 4U> neighborhood{};
};

struct SequenceSwitchAsyncResult {
    PostedNotificationToken token;
    CommandContext context;
    InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
    InkpodSequenceSwitchRequest request{sizeof(InkpodSequenceSwitchRequest)};
    std::wstring source_recovery_path;
    RecoveryMetadata source_metadata{};
    std::wstring target_recovery_path;
    RecoveryMetadata target_metadata{};
    bool source_autosaved{};
    bool target_restored{};
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
    std::wstring window_class_name;
    std::wstring window_title;
    int show_command{SW_SHOWNORMAL};
    bool smoke_test{};
    int smoke_dirty_prompt_choice{IDNO};
    std::uint32_t smoke_dirty_prompt_count{};
    std::wstring smoke_raster_path;
    std::vector<std::wstring> smoke_sequence_paths;
    bool restore_previous_documents{};
    SequenceCellSwitchPolicy sequence_switch_policy{
        SequenceCellSwitchPolicy::Prompt};
};

struct DocumentShellState {
    std::wstring current_path;
    std::wstring source_path;
    std::wstring recovery_path;
    std::wstring recovery_original_path;
    std::uint64_t smoke_layer_id{};
    std::uint64_t selection_layer_id{};
};

// A workspace only retains a copied presentation of the Core-owned editor
// state.  The binding prevents a value left visible by another document or an
// older session generation from becoming an update base.
struct EditorPresentationBinding final {
    DocumentSessionId session{};
    Generation generation{};
    std::uint64_t editor_revision{};
    std::array<std::uint8_t, 32U> editor_digest{};
    bool valid{};
};

// Immutable values copied at an interaction boundary.  Finish paths consume
// this record instead of re-reading a mutable workspace presentation.
struct EditorProcedureCapture final {
    DocumentSessionId session{};
    Generation generation{};
    std::uint64_t core_view_id{};
    InkpodEditorStateInfo state{sizeof(InkpodEditorStateInfo)};
    bool valid{};
};

struct ToolUiState {
    EditorPresentationBinding editor{};
    EditorProcedureCapture procedure{};
    std::uint32_t active_tool{};
    std::uint32_t last_color_consuming_tool{};
    InkpodPlaneKind active_plane{};
    std::uint32_t color_rgba{};
    InkpodColorValue drawing_color{sizeof(InkpodColorValue)};
    InkpodEyedropperSource eyedropper_source{INKPOD_EYEDROPPER_COMPOSITE};
    float diameter{8.0F};
    InkpodEditorBrushOptions brush{
        sizeof(InkpodEditorBrushOptions),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};

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
    InkpodRangeInterpretation selection_interpretation{INKPOD_RANGE_NORMAL};
    std::uint32_t selection_aspect_ratio_q16{};
    std::uint64_t selection_construction_flags{};
    std::uint32_t selection_rotation_turns{};
    InkpodTraceBrushShape selection_trace_shape{INKPOD_TRACE_ROUND};
    std::vector<InkpodStrokeSample> selection_gesture_samples;
    InkpodColorValue color_replace_target{sizeof(InkpodColorValue)};
    InkpodSelectionShape color_replace_shape{INKPOD_SELECTION_TRACE};
    InkpodScopedColorReplaceMode color_replace_mode{INKPOD_COLOR_REPLACE_RASTER_COLOR};
    float color_replace_diameter{8.0F};
    std::uint64_t color_replace_base_revision{};
    std::vector<InkpodStrokeSample> color_replace_gesture_samples;
    std::vector<InkpodStrokeSample> vector_gesture_samples;
    std::vector<InkpodGeometryPoint> vector_geometry_points;
    std::uint64_t vector_geometry_base_revision{};
    std::uint64_t vector_geometry_view_revision{};
    std::uint64_t vector_geometry_flags{INKPOD_GEOMETRY_OUTLINE};
    std::uint32_t vector_geometry_polygon_sides{5U};
    std::uint32_t vector_geometry_aspect_ratio_q16{};
    std::uint32_t vector_geometry_rotation_turns{};
    std::uint32_t vector_geometry_phase{};
    DWORD vector_last_click_time{};
    POINT vector_last_click_device{};
    bool vector_geometry_preview_active{};
    bool vector_geometry_snap_bypass{};
    InkpodVectorEraseMode vector_erase_mode{INKPOD_VECTOR_ERASE_PARTIAL};
    InkpodVectorSelectionMode vector_selection_mode{INKPOD_VECTOR_SELECT_TOUCHING};
    std::vector<std::uint64_t> vector_selected_path_ids;

    HWND palette{};
    windows::ui::ToolPaletteDialogState palette_dialog{};
    windows::ui::panes::ToolOptionsPaneState options_pane{};
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
    bool vector_antialias{true};
    InkpodVectorCenterlineMode vector_centerline_mode{INKPOD_VECTOR_CENTERLINE_HIDDEN};
    bool vector_endpoints_visible{};
    std::int32_t pointer_device_x{};
    std::int32_t pointer_device_y{};
    std::uint64_t locator_generation{};
    std::uint64_t locator_presented_generation{};
    bool locator_valid{};
    InkpodLocatorOutput locator{};
    std::uint32_t locator_neighborhood_width{};
    std::uint32_t locator_neighborhood_height{};
    std::int32_t locator_neighborhood_origin_x{};
    std::int32_t locator_neighborhood_origin_y{};
    std::array<std::uint8_t, 9U * 9U * 4U> locator_neighborhood{};
    std::vector<InkpodStrokeSample> gesture_samples;
    bool guide_drag_active{};
    std::uint32_t guide_drag_axis{};
    std::uint64_t guide_drag_id{};
    std::optional<DragToken> active_drag;
};

struct ColorChartGenerationJob {
    ColorChartGenerationJob() noexcept {
        summary.struct_size = sizeof(summary);
    }

    ~ColorChartGenerationJob() {
        (void)inkpod_color_chart_preview_release(&preview);
        (void)inkpod_task_release(&task);
    }

    ColorChartGenerationJob(const ColorChartGenerationJob&) = delete;
    ColorChartGenerationJob& operator=(const ColorChartGenerationJob&) = delete;

    InkpodTask* task{};
    InkpodColorChartPreview* preview{};
    InkpodColorChartPreviewSummary summary{};
    CommandContext context;
    std::atomic_uint32_t status{INKPOD_STATUS_INVALID_STATE};
    std::uint64_t token{};
    std::uint32_t maximum_colors{};
    std::uint32_t quantization_bits{};
};

struct OutputColorGuardJob {
    OutputColorGuardJob() noexcept {
        request.struct_size = sizeof(request);
        result.struct_size = sizeof(result);
    }

    InkpodOutputColorGuardRequest request{};
    InkpodOutputColorGuardResult result{};
};

struct PaneUiState {
    InkpodColorValue main_line_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    std::vector<InkpodColorValue> palette_colors;
    std::uint32_t palette_group{};
    std::uint32_t selected_palette_index{};
    std::vector<InkpodColorValue> color_chart_colors;
    std::vector<std::wstring> color_chart_names;
    std::uint32_t color_chart_page{};
    std::uint32_t selected_color_chart_index{};
    bool color_chart_locked{};
    std::uint64_t color_chart_generation_token{};
    std::shared_ptr<ColorChartGenerationJob> color_chart_generation;
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
    DocumentSessionId light_table_selection_session{};
    Generation light_table_selection_generation{};
    std::optional<CommandContext> light_table_move_context;
    std::uint32_t sequence_count{};
    std::vector<InkpodStrokeSample> light_table_move_samples;
    HWND layer_palette{};
    windows::ui::LayerPaletteDialogState layer_palette_dialog{};
    windows::ui::panes::ColorDockPaneState color_pane{};
};

struct AnimationUiState {
    std::uint32_t active_sequence_index{};
    std::wstring active_sequence_name;
    std::uint32_t motion_fps{24U};
    std::uint64_t motion_flags{INKPOD_MOTION_FLAG_LOOP};
    bool motion_active{};
    bool motion_paused{};
    bool sequence_switch_pending{};
    std::uint32_t smoke_sequence_switch_completed{};
    InkpodStatus smoke_sequence_switch_status{INKPOD_STATUS_OK};
};

struct EffectsUiState {
    std::uint64_t adjustment_id{};
    bool adjustment_visible{true};
    std::vector<AdjustmentLayerUiState> adjustments;
    InkpodTask* task{};
    windows::ui::ProgressDialogState progress_dialog{};
    bool preview_prompt{};
    bool alpha_view{};
    bool stamp_source_valid{};
    InkpodStrokeSample stamp_source{};
    CanvasEffectOptions options{};
    CanvasEffectOptions gesture_options{};
    bool gesture_options_valid{};
    std::vector<InkpodStrokeSample> samples;
    bool airbrush_active{};
    InkpodStrokeSample airbrush_last{};
    std::optional<CommandContext> gesture_context;
    std::optional<JobSessionId> job_id;
    CommandContext completion_context;
    InteractiveFilterPreviewUiState filter_preview;
    InkpodOutputColorGuardProfile output_color_guard_profile{
        INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR};
    std::shared_ptr<OutputColorGuardJob> output_color_guard;
    std::wstring last_output_color_guard_summary;
};

struct BatchUiState {
    std::wstring target_text{L"アクティブに追従（対象なし）"};
    std::wstring job_text{L"待機中"};
    bool target_available{};
    bool target_pinned{};
    bool return_to_pinned{};
    CommandContext return_context;
    InkpodBatchInputKind input_kind{INKPOD_BATCH_INPUT_CURRENT_SEQUENCE};
    std::wstring input_path;
    std::uint32_t first_cell{};
    std::uint32_t last_cell{};
    std::vector<BatchOperationUi> operations;
    std::vector<BatchOperationUi> run_operations;
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

    windows::ui::BatchPaletteDialogState palette_dialog{};
    InkpodBatchGraph* graph{};
    InkpodBatchGraph* run_graph{};
    InkpodBatchPreview* preview{};
    InkpodBatchReport* report{};
    InkpodBatchTask* task{};
    windows::ui::ProgressDialogState progress_dialog{};
    std::optional<JobSessionId> job_id;
    CommandContext completion_context;
};

struct FrontendRoutingState {
    CommandTargetRegistry targets;
    PaneTargetRegistry pane_targets;
    CommandTimerRegistry timers;
    FrontendTokenSource tokens;
    std::atomic_uint64_t locator_pending_token{};
    bool locator_latest_requested{};
    std::mutex locator_results_mutex;
    std::array<std::optional<LocatorAsyncResult>, 64U> locator_results{};
    std::atomic_uint64_t sequence_switch_pending_token{};
    std::mutex sequence_switch_results_mutex;
    std::shared_ptr<SequenceSwitchAsyncResult> sequence_switch_result;
    CommandContext command_state_context;
    PaneInstanceId tool_pane{};
    PaneInstanceId tool_options_pane{};
    PaneInstanceId color_pane{};
    PaneInstanceId layer_pane{};
    PaneInstanceId batch_pane{};
    PaneInstanceId locator_pane{};
    PaneInstanceId sequence_pane{};
    PaneInstanceId light_table_pane{};
    PaneInstanceId reference_pane{};
    PaneInstanceId subpalette_pane{};
};

} // namespace inkpod::app
