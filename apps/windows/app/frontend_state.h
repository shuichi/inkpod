#pragma once

#include "ui/localization.h"

#include <windows.h>

#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <functional>
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

class FileIoController;

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

struct RecoveryCleanupEntry final {
    std::wstring path;
    InkpodIoRecoveryArtifactProof proof{};
};

struct SequenceSwitchAsyncResult {
    PostedNotificationToken token;
    CommandContext context;
    InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
    InkpodSequenceSwitchRequest request{sizeof(InkpodSequenceSwitchRequest)};
    std::wstring source_recovery_path;
    RecoveryMetadata source_metadata{};
    InkpodIoRecoveryArtifactProof source_artifact_proof{};
    InkpodCommonRasterFormat source_raster_format_hint{};
    std::wstring source_previous_recovery_path;
    InkpodIoRecoveryArtifactProof source_previous_artifact_proof{};
    std::uint64_t source_previous_artifact_generation{};
    bool source_previous_artifact_published{};
    std::wstring source_shell_recovery_path;
    InkpodIoRecoveryArtifactProof source_shell_artifact_proof{};
    std::uint64_t source_shell_artifact_generation{};
    bool source_shell_artifact_published{};
    std::uint64_t source_persistence_epoch{};
    // DocumentRegistry capability for the exact target authority reserved by
    // this replacement. Stored as the fixed token value here to avoid making
    // frontend_state.h depend on document_session.h (which owns this record).
    std::uint64_t identity_reservation_token{};
    std::vector<RecoveryCleanupEntry> recovery_cleanup;
    std::wstring target_recovery_path;
    InkpodIoRecoveryArtifactProof target_artifact_proof{};
    std::uint64_t target_artifact_generation{};
    bool target_artifact_published{};
    RecoveryMetadata target_metadata{};
    InkpodCommonRasterFormat target_raster_format_hint{};
    std::wstring target_source_path;
    std::wstring target_pair_raster_path;
    std::wstring target_pair_native_path;
    // Independently allocated before Core apply. The shell and sequence slot
    // each own their raster path, so completion can publish both by move
    // without allocating after the document replacement has committed.
    std::wstring target_binding_raster_path;
    DocumentIdentity target_pair_raster_identity{};
    DocumentIdentity target_pair_native_identity{};
    // Recovery replacement must resolve to the exact pair authority captured
    // with the target binding, not merely the same Genesis/UUID.
    DocumentIdentity target_expected_raster_identity{};
    bool target_pair_native_exists{};
    bool target_pair_resolved{};
    bool source_autosaved{};
    bool interactive_activation{};
    bool replace_document{};
    InkpodStatus mutation_status{INKPOD_STATUS_INVALID_STATE};
    InkpodDocumentInfo activated_document{sizeof(InkpodDocumentInfo)};
};

struct SequenceNavigationIntent {
    CommandContext context;
    std::optional<std::uint32_t> index;
    InkpodSequenceDirection direction{};
    InkpodSequenceEndpointPolicy endpoint_policy{};
    std::uint64_t catalog_revision{};
    std::uint64_t catalog_owner_generation{};
};

struct BatchOperationUi {
    std::uint32_t kind{INKPOD_BATCH_OPERATION_COLOR_REPLACE};
    std::uint64_t flags{INKPOD_BATCH_OPERATION_ENABLED};
    std::uint64_t layer_id{};
    std::uint64_t plane_id{};
    InkpodTypedPlaneKind plane_kind{INKPOD_TYPED_PLANE_COLOR};
    InkpodBatchMissingPolicy missing_policy{INKPOD_BATCH_MISSING_ERROR};
    std::vector<InkpodBatchTargetInput> additional_targets;
    std::vector<InkpodColorValue> colors;
    std::vector<InkpodBatchColorPairInput> color_pairs;
    std::wstring label;
};

struct BatchInputUi {
    InkpodBatchInputKind kind{INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT};
    std::wstring path;
    std::uint32_t first_cell{};
    std::uint32_t last_cell{};
    std::uint64_t resolved_count{};
    std::wstring validation_text;
};

struct AppLifetimeState {
    HINSTANCE instance{};
    std::wstring window_class_name;
    std::wstring window_title;
    int show_command{SW_SHOWNORMAL};
    bool smoke_test{};
    int smoke_dirty_prompt_choice{IDNO};
    std::uint32_t smoke_dirty_prompt_count{};
    std::uint32_t smoke_pair_overwrite_prompt_count{};
    bool smoke_lock_pair_raster_during_install{};
    bool smoke_fail_repaired_item_refresh{};
    bool smoke_hold_sequence_auto_publication{};
    bool smoke_async_sequence_import{};
    std::function<InkpodStatus()> smoke_deferred_sequence_auto_publication;
    std::function<void()> smoke_after_native_save_path_selected;
    std::function<void()> smoke_after_native_save_committed;
    std::function<void(
        const std::wstring&,
        const std::wstring&,
        const DocumentIdentity&,
        const DocumentIdentity&)> smoke_after_sequence_pair_reserved;
    InkpodStatus smoke_last_sequence_switch_status{INKPOD_STATUS_INVALID_STATE};
    // Type-erased owners for proof-bound exact cleanup chains. The UI timer
    // keeps them alive across bounded FileIo saturation and shutdown drains.
    std::vector<std::shared_ptr<void>> recovery_cleanup_owners;
    std::wstring smoke_raster_path;
    std::wstring smoke_native_save_path;
    std::vector<std::wstring> smoke_sequence_paths;
    bool restore_previous_documents{};
    SequenceCellSwitchPolicy sequence_switch_policy{
        SequenceCellSwitchPolicy::Prompt};
    SequenceEndpointPolicy sequence_endpoint_policy{
        SequenceEndpointPolicy::Stop};
};

struct DocumentShellState {
    std::wstring current_path;
    std::wstring source_path;
    // A raster opened for editing owns a deterministic same-stem native
    // destination before the first successful normal save.  This is a plan,
    // not normal path authority: Revert must continue to require current_path.
    std::wstring planned_native_path;
    // The raster member of the editable pair.  It remains associated after a
    // successful save so either member can identify the same document session.
    std::wstring pair_raster_path;
    // Non-authoritative format memory retained after a terminal pair-authority
    // revoke so the required next Save As can derive its raster companion.
    InkpodCommonRasterFormat pair_raster_format_hint{};
    std::wstring display_name;
    // A Batch preview is a display-only document. Batch commands issued while
    // it is active keep targeting this exact issue-time source view.
    std::optional<CommandContext> batch_preview_source_context;
    std::wstring recovery_path;
    // A recovery path is authoritative only when the same successful durable
    // publication installed this exact proof and monotonically increasing
    // runtime generation.  A planned or failed attempt is never stored here.
    InkpodIoRecoveryArtifactProof recovery_artifact_proof{};
    std::uint64_t recovery_artifact_generation{};
    bool recovery_artifact_published{};
    std::wstring recovery_original_path;
    std::uint64_t smoke_layer_id{};
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
        sizeof(InkpodFloatingTransform), INKPOD_TRANSFORM_ANCHOR_CENTER, 0.0, 0.0, 1.0, 1.0, 0.0};
    InkpodFrameRect floating_bounds{};
    std::vector<InkpodStrokeSample> floating_gesture_samples;
    std::uint32_t floating_drag_mode{};
    InkpodFloatingTransform floating_drag_start{
        sizeof(InkpodFloatingTransform), INKPOD_TRANSFORM_ANCHOR_CENTER, 0.0, 0.0, 1.0, 1.0, 0.0};

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
    std::vector<InkpodStrokeSample> geometry_gesture_samples;
    std::uint64_t geometry_base_revision{};
    std::uint64_t geometry_view_revision{};
    bool geometry_preview_active{};
    bool geometry_snap_bypass{};
    // Keep a failed shared Canvas overlay clear retryable after gesture reset.
    bool geometry_preview_clear_pending{};
    InkpodColorValue color_replace_target{sizeof(InkpodColorValue)};
    InkpodSelectionShape color_replace_shape{INKPOD_SELECTION_TRACE};
    InkpodScopedColorReplaceMode color_replace_mode{INKPOD_COLOR_REPLACE_RASTER_COLOR};
    float color_replace_diameter{8.0F};
    std::uint64_t color_replace_base_revision{};
    std::vector<InkpodStrokeSample> color_replace_gesture_samples;
    InkpodShootingFrameInput shooting_frame_drag_value{
        sizeof(InkpodShootingFrameInput)};
    std::vector<InkpodStrokeSample> shooting_frame_gesture_samples;
    std::uint32_t shooting_frame_drag_handle{};
    bool shooting_frame_preview_active{};
    HWND palette{};
    windows::ui::ToolPaletteDialogState palette_dialog{};
    windows::ui::panes::ToolOptionsPaneState options_pane{};
    windows::ui::panes::ToolOptionsFlyoutState options_flyout{};
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
    bool saved_selection_available{};
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
    InkpodTask* task{};
    windows::ui::ProgressDialogState progress_dialog{};
    bool preview_prompt{};
    bool alpha_view{};
    bool stamp_source_valid{};
    InkpodStrokeSample stamp_source{};
    UINT options_command{};
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

enum class BatchJobKind : std::uint8_t {
    None,
    Execute,
    ContactSheetPreview,
};

struct BatchUiState {
    std::wstring target_text{
        windows::ui::UiText(windows::ui::UiStringId::FollowingNoTarget)};
    std::wstring job_text{
        windows::ui::UiText(windows::ui::UiStringId::Text0655)};
    bool target_available{};
    bool target_pinned{};
    bool return_to_pinned{};
    CommandContext return_context;
    std::wstring set_name{L"Windows Batch Set"};
    std::vector<std::wstring> available_set_names;
    std::vector<BatchInputUi> inputs{BatchInputUi{}};
    std::vector<BatchOperationUi> operations;
    std::uint32_t selected_stage{};
    std::uint32_t selected_operation{};
    InkpodBatchOutputDestination output_destination{
        INKPOD_BATCH_OUTPUT_NEW_TABS};
    InkpodBatchOutputFormat output_format{INKPOD_BATCH_FORMAT_INKPOD};
    InkpodBatchFailurePolicy failure_policy{INKPOD_BATCH_FAILURE_CONTINUE};
    std::wstring output_folder{L"."};
    std::wstring naming_template{L"{stem}_batch"};
    std::uint32_t wait_milliseconds{};
    bool preview_before_save{};
    std::wstring validation_text;
    std::wstring last_result;

    windows::ui::BatchPaletteDialogState palette_dialog{};
    InkpodBatchGraph* graph{};
    InkpodBatchGraph* run_graph{};
    InkpodBatchPreview* preview{};
    InkpodBatchReport* report{};
    InkpodBatchTask* task{};
    FileIoController* io_owner{};
    std::uint64_t io_request{};
    InkpodStatus io_completion_status{INKPOD_STATUS_OK};
    std::optional<JobSessionId> job_id;
    BatchJobKind job_kind{BatchJobKind::None};
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
    // UI-owned, bounded input intentions. The worker never reads this queue.
    std::deque<SequenceNavigationIntent> sequence_navigation_queue;
    bool sequence_navigation_pending{};
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
