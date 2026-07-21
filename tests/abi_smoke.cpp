#include "inkpod/core_ffi.h"

#include <array>
#include <cstddef>
#include <cstdint>
#include <thread>
#include <type_traits>
#include <vector>

static_assert(std::is_standard_layout_v<InkpodCoreConfig>);
static_assert(std::is_standard_layout_v<InkpodSnapshotView>);
static_assert(sizeof(InkpodCoreConfig) == 16U);
static_assert(sizeof(InkpodCommand) == 16U);
static_assert(sizeof(InkpodCommandBatch) == 40U);
static_assert(sizeof(InkpodDispatchResult) == 24U);
static_assert(sizeof(InkpodSnapshotOptions) == 16U);
static_assert(sizeof(InkpodSnapshotTile) == 64U);
static_assert(sizeof(InkpodSnapshotView) == 48U);
static_assert(sizeof(InkpodCellCreateOptions) == 48U);
static_assert(sizeof(InkpodDocumentInfo) == 192U);
static_assert(sizeof(InkpodStrokeSample) == 24U);
static_assert(sizeof(InkpodStrokeInput) == 56U);
static_assert(sizeof(InkpodViewInput) == 48U);
static_assert(sizeof(InkpodSnapshotTransform) == 48U);
static_assert(sizeof(InkpodSnapshotGuide) == 24U);
static_assert(sizeof(InkpodSnapshotOverlay) == 56U);
static_assert(sizeof(InkpodColorValue) == 16U);
static_assert(sizeof(InkpodColorArray) == 40U);
static_assert(sizeof(InkpodColorBuffer) == 48U);
static_assert(sizeof(InkpodFillInput) == 96U);
static_assert(sizeof(InkpodFillResult) == 32U);
static_assert(sizeof(InkpodTreeEdit) == 64U);
static_assert(sizeof(InkpodNodeInfo) == 72U);
static_assert(sizeof(InkpodSelectionPoint) == 16U);
static_assert(sizeof(InkpodSelectionInput) == 72U);
static_assert(sizeof(InkpodFloatingTransform) == 48U);
static_assert(sizeof(InkpodGridInput) == 32U);
static_assert(sizeof(InkpodLocatorOutput) == 48U);
static_assert(sizeof(InkpodM4RasterInput) == 96U);
static_assert(sizeof(InkpodLightTableItemInput) == 168U);
static_assert(sizeof(InkpodSequenceCellInput) == 120U);
static_assert(sizeof(InkpodSequenceInput) == 40U);
static_assert(sizeof(InkpodMotionCheckInput) == 16U);
static_assert(sizeof(InkpodMotionFrame) == 40U);
static_assert(sizeof(InkpodVectorPoint) == 8U);
static_assert(sizeof(InkpodVectorCubicSegment) == 48U);
static_assert(sizeof(InkpodVectorPathInput) == 64U);
static_assert(sizeof(InkpodVectorFillInput) == 56U);
static_assert(sizeof(InkpodVectorEraseInput) == 32U);
static_assert(sizeof(InkpodVectorWidthInput) == 40U);
static_assert(sizeof(InkpodVectorSelectionInput) == 32U);
static_assert(sizeof(InkpodVectorSelectionRange) == 24U);
static_assert(sizeof(InkpodVectorSelectionBuffer) == 56U);
static_assert(sizeof(InkpodVectorRasterizeInput) == 32U);
static_assert(sizeof(InkpodVectorRasterBuffer) == 48U);
static_assert(sizeof(InkpodRasterVectorizeInput) == 32U);
static_assert(sizeof(InkpodSnapshotVectorSegment) == 80U);
static_assert(sizeof(InkpodSnapshotVectorFill) == 48U);
static_assert(sizeof(InkpodSnapshotVectorView) == 80U);

extern "C" int inkpod_header_c11_smoke(void);

int InkpodRunAbiSmoke() {
    if (inkpod_header_c11_smoke() != 0) {
        return 31;
    }
    if (inkpod_abi_version() != INKPOD_ABI_VERSION) {
        return 1;
    }

    InkpodCoreConfig config{
        sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
    InkpodCore* core = nullptr;
    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK
        || core == nullptr) {
        return 2;
    }

    const InkpodCommand command{
        sizeof(InkpodCommand), INKPOD_COMMAND_NO_OP, 0U};
    const InkpodCommandBatch batch{
        sizeof(InkpodCommandBatch),
        0U,
        INKPOD_FEATURE_NONE,
        &command,
        1U,
        sizeof(InkpodCommand)};
    InkpodDispatchResult dispatch{};
    dispatch.struct_size = sizeof(dispatch);
    if (inkpod_core_dispatch_batch(core, &batch, &dispatch) != INKPOD_STATUS_OK
        || dispatch.revision != 0U
        || dispatch.accepted_command_count != 1U) {
        return 3;
    }

    InkpodCommand unknown_command = command;
    unknown_command.kind = UINT32_MAX;
    InkpodCommandBatch unknown_batch = batch;
    unknown_batch.commands = &unknown_command;
    if (inkpod_core_dispatch_batch(core, &unknown_batch, &dispatch)
        != INKPOD_STATUS_UNSUPPORTED) {
        return 19;
    }

    InkpodCommandBatch invalid_stride_batch = batch;
    invalid_stride_batch.command_stride_bytes = sizeof(InkpodCommand) - 1U;
    if (inkpod_core_dispatch_batch(core, &invalid_stride_batch, &dispatch)
        != INKPOD_STATUS_INVALID_ARGUMENT) {
        return 20;
    }

    const InkpodSnapshotOptions options{
        sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
    InkpodSnapshot* snapshot = nullptr;
    InkpodSnapshotOptions short_options = options;
    short_options.struct_size = sizeof(std::uint32_t);
    if (inkpod_core_build_snapshot(core, &short_options, &snapshot)
            != INKPOD_STATUS_INCOMPATIBLE_ABI
        || snapshot != nullptr) {
        return 21;
    }
    if (inkpod_core_build_snapshot(core, &options, &snapshot) != INKPOD_STATUS_OK
        || snapshot == nullptr) {
        return 4;
    }

    InkpodSnapshotView view{};
    view.struct_size = sizeof(view);
    InkpodSnapshotView short_view{};
    short_view.struct_size = sizeof(std::uint32_t);
    if (inkpod_snapshot_get_view(snapshot, &short_view)
        != INKPOD_STATUS_INCOMPATIBLE_ABI) {
        return 22;
    }
    if (inkpod_snapshot_get_view(snapshot, &view) != INKPOD_STATUS_OK
        || view.abi_version != INKPOD_ABI_VERSION
        || view.revision != 0U
        || view.tiles != nullptr
        || view.tile_count != 0U
        || view.tile_stride_bytes != sizeof(InkpodSnapshotTile)) {
        return 5;
    }

    InkpodStatus renderer_release_status = INKPOD_STATUS_INVALID_ARGUMENT;
    std::thread renderer_thread([&snapshot, &renderer_release_status]() {
        renderer_release_status = inkpod_snapshot_release(&snapshot);
    });
    renderer_thread.join();
    if (renderer_release_status != INKPOD_STATUS_OK
        || snapshot != nullptr
        || inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK) {
        return 6;
    }

    InkpodStatus wrong_thread_status = INKPOD_STATUS_OK;
    std::thread wrong_thread([core, &wrong_thread_status]() {
        const InkpodCommand local_command{
            sizeof(InkpodCommand), INKPOD_COMMAND_NO_OP, 0U};
        const InkpodCommandBatch local_batch{
            sizeof(InkpodCommandBatch),
            0U,
            INKPOD_FEATURE_NONE,
            &local_command,
            1U,
            sizeof(InkpodCommand)};
        InkpodDispatchResult local_result{};
        local_result.struct_size = sizeof(local_result);
        wrong_thread_status = inkpod_core_dispatch_batch(
            core, &local_batch, &local_result);
    });
    wrong_thread.join();
    if (wrong_thread_status != INKPOD_STATUS_WRONG_THREAD) {
        return 7;
    }
    if (inkpod_core_destroy(&core) != INKPOD_STATUS_OK
        || core != nullptr
        || inkpod_core_destroy(&core) != INKPOD_STATUS_OK) {
        return 8;
    }

    if (inkpod_core_create(nullptr, &core) != INKPOD_STATUS_INVALID_ARGUMENT
        || core != nullptr) {
        return 9;
    }
    InkpodCoreConfig invalid = config;
    invalid.struct_size = 1U;
    if (inkpod_core_create(&invalid, &core) != INKPOD_STATUS_INCOMPATIBLE_ABI
        || core != nullptr) {
        return 10;
    }

    std::uint64_t required{};
    if (inkpod_error_message_size(&required) != INKPOD_STATUS_OK
        || required <= 1U) {
        return 11;
    }
    std::uint8_t too_small{};
    std::uint64_t written{UINT64_MAX};
    if (inkpod_error_message_copy(&too_small, 1U, &written)
            != INKPOD_STATUS_BUFFER_TOO_SMALL
        || written != 0U) {
        return 12;
    }
    std::array<std::uint8_t, 512> error_text{};
    if (required > error_text.size()
        || inkpod_error_message_copy(
               error_text.data(), error_text.size(), &written) != INKPOD_STATUS_OK
        || written == 0U
        || error_text[static_cast<std::size_t>(written)] != 0U) {
        return 13;
    }

    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK || core == nullptr) {
        return 23;
    }
    const InkpodCellCreateOptions cell_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x123456789abcdef0),
        UINT64_C(0x1032547698badcfe),
        1920U,
        1080U,
        96000U,
        96000U};
    InkpodDocumentInfo document{};
    document.struct_size = sizeof(document);
    if (inkpod_core_new_cell(core, &cell_options, &document) != INKPOD_STATUS_OK
        || document.width != 1920U || document.height != 1080U
        || (document.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        return 24;
    }
    std::array<std::uint8_t, 4U * 4U * 4U> light_pixels{};
    const std::size_t light_offset = (2U * 4U + 2U) * 4U;
    light_pixels[light_offset] = 90U;
    light_pixels[light_offset + 1U] = 80U;
    light_pixels[light_offset + 2U] = 70U;
    light_pixels[light_offset + 3U] = 255U;
    constexpr std::array<std::uint8_t, 9U> light_name{
        'r', 'e', 'f', 'e', 'r', 'e', 'n', 'c', 'e'};
    const InkpodM4RasterInput light_source{
        sizeof(InkpodM4RasterInput),
        INKPOD_STORAGE_RGBA8,
        0U,
        2U,
        2U,
        3U,
        4U,
        4U,
        96000U,
        96000U,
        InkpodFrameRect{2, 2, 4, 4},
        light_pixels.data(),
        light_pixels.size(),
        16U};
    const InkpodLightTableItemInput light_item{
        sizeof(InkpodLightTableItemInput),
        INKPOD_LIGHT_TABLE_ITEM_VISIBLE,
        500U,
        INKPOD_LIGHT_TABLE_COLOR,
        InkpodColorValue{
            sizeof(InkpodColorValue),
            INKPOD_COLOR_DEPTH_8,
            0U,
            128U,
            255U,
            255U},
        0,
        0,
        1000U,
        1000U,
        0,
        0U,
        light_name.data(),
        light_name.size(),
        light_source};
    std::uint64_t light_item_id{};
    InkpodColorValue light_sample{};
    light_sample.struct_size = sizeof(light_sample);
    if (inkpod_core_light_table_add_item(
            core, &light_item, &dispatch, &light_item_id) != INKPOD_STATUS_OK
        || light_item_id == 0U
        || inkpod_core_light_table_set_global_opacity(core, 500U, &dispatch)
            != INKPOD_STATUS_OK
        || inkpod_core_light_table_sample(core, 960U, 540U, &light_sample)
            != INKPOD_STATUS_OK
        || light_sample.red != 90U || light_sample.green != 80U
        || light_sample.blue != 70U || light_sample.alpha != 64U) {
        return 45;
    }
    InkpodFillInput light_boundary_fill{};
    light_boundary_fill.struct_size = sizeof(light_boundary_fill);
    light_boundary_fill.operation = INKPOD_FILL_SEED;
    light_boundary_fill.flags = INKPOD_FILL_FLAG_SELECTION_PRESENT
        | INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY;
    light_boundary_fill.seed_x = 956U;
    light_boundary_fill.seed_y = 536U;
    light_boundary_fill.color = InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 200U, 10U, 20U, 255U};
    light_boundary_fill.inclusion_mode = INKPOD_INCLUSION_NONE;
    light_boundary_fill.selection = InkpodFrameRect{956, 536, 8, 8};
    InkpodFillResult light_boundary_result{};
    light_boundary_result.struct_size = sizeof(light_boundary_result);
    if (inkpod_core_apply_fill(core, &light_boundary_fill, &light_boundary_result)
            != INKPOD_STATUS_OK
        || light_boundary_result.changed_pixel_count != 63U
        || inkpod_core_light_table_sample(core, 960U, 540U, &light_sample)
            != INKPOD_STATUS_OK
        || light_sample.red != 90U || light_sample.green != 80U
        || light_sample.blue != 70U || light_sample.alpha != 64U) {
        return 47;
    }
    constexpr std::array<std::uint8_t, 4U> sequence_pixel_a{1U, 2U, 3U, 255U};
    constexpr std::array<std::uint8_t, 4U> sequence_pixel_b{4U, 5U, 6U, 255U};
    constexpr std::array<std::uint8_t, 10U> sequence_name_a{
        'c', 'e', 'l', 'l', '1', '0', '.', 'p', 'n', 'g'};
    constexpr std::array<std::uint8_t, 9U> sequence_name_b{
        'c', 'e', 'l', 'l', '2', '.', 'p', 'n', 'g'};
    const std::array<InkpodSequenceCellInput, 2U> sequence_cells{
        InkpodSequenceCellInput{
            sizeof(InkpodSequenceCellInput),
            0U,
            sequence_name_a.data(),
            sequence_name_a.size(),
            InkpodM4RasterInput{
                sizeof(InkpodM4RasterInput), INKPOD_STORAGE_RGBA8, 0U, 5U, 1U, 1U,
                1U, 1U, 96000U, 96000U, InkpodFrameRect{0, 0, 1, 1},
                sequence_pixel_a.data(), sequence_pixel_a.size(), 4U}},
        InkpodSequenceCellInput{
            sizeof(InkpodSequenceCellInput),
            0U,
            sequence_name_b.data(),
            sequence_name_b.size(),
            InkpodM4RasterInput{
                sizeof(InkpodM4RasterInput), INKPOD_STORAGE_RGBA8, 0U, 5U, 2U, 1U,
                1U, 1U, 96000U, 96000U, InkpodFrameRect{0, 0, 1, 1},
                sequence_pixel_b.data(), sequence_pixel_b.size(), 4U}}};
    const InkpodSequenceInput sequence_input{
        sizeof(InkpodSequenceInput),
        0U,
        0U,
        sequence_cells.data(),
        sequence_cells.size(),
        sizeof(InkpodSequenceCellInput)};
    const InkpodMotionCheckInput motion_input{
        sizeof(InkpodMotionCheckInput), 24U, INKPOD_MOTION_FLAG_LOOP};
    InkpodMotionFrame motion_frame{};
    motion_frame.struct_size = sizeof(motion_frame);
    if (inkpod_core_sequence_set(core, &sequence_input) != INKPOD_STATUS_OK
        || inkpod_core_sequence_step(core, INKPOD_SEQUENCE_NEXT, 0U, &document)
            != INKPOD_STATUS_UNSAVED_CHANGES
        || inkpod_core_motion_check_start(core, &motion_input, &motion_frame)
            != INKPOD_STATUS_OK
        || motion_frame.cell_number != 2U || motion_frame.thumbnail_checksum == 0U
        || inkpod_core_motion_check_step(core, INKPOD_SEQUENCE_NEXT, &motion_frame)
            != INKPOD_STATUS_OK
        || motion_frame.cell_number != 10U
        || inkpod_core_motion_check_stop(core) != INKPOD_STATUS_OK) {
        return 46;
    }
    InkpodTreeEdit tree_edit{};
    tree_edit.struct_size = sizeof(tree_edit);
    tree_edit.operation = INKPOD_TREE_DUPLICATE_LAYER;
    tree_edit.object_id = document.layer_id;
    std::uint64_t tree_object_id{};
    if (inkpod_core_tree_edit(core, &tree_edit, &dispatch, &tree_object_id)
            != INKPOD_STATUS_OK
        || tree_object_id == 0U) {
        return 34;
    }
    const std::uint64_t duplicate_layer = tree_object_id;
    tree_edit.operation = INKPOD_TREE_REORDER_LAYER;
    tree_edit.object_id = duplicate_layer;
    tree_edit.destination_index = 0U;
    if (inkpod_core_tree_edit(core, &tree_edit, &dispatch, &tree_object_id)
            != INKPOD_STATUS_OK) {
        return 35;
    }
    InkpodNodeInfo node{};
    node.struct_size = sizeof(node);
    if (inkpod_core_node_get(core, 0U, UINT32_MAX, &node) != INKPOD_STATUS_OK
        || node.id != duplicate_layer || node.child_count != 2U) {
        return 36;
    }
    InkpodDocumentInfo before_invalid_tree{};
    before_invalid_tree.struct_size = sizeof(before_invalid_tree);
    InkpodDocumentInfo after_invalid_tree{};
    after_invalid_tree.struct_size = sizeof(after_invalid_tree);
    constexpr std::array<std::uint8_t, 17> invalid_plane_name{
        'I', 'n', 'v', 'a', 'l', 'i', 'd', ' ', 's', 'e', 'l', 'e', 'c', 't', 'i', 'o', 'n'};
    InkpodTreeEdit invalid_plane{};
    invalid_plane.struct_size = sizeof(invalid_plane);
    invalid_plane.operation = INKPOD_TREE_CREATE_PLANE;
    invalid_plane.parent_id = document.layer_id;
    invalid_plane.kind = INKPOD_TYPED_PLANE_SELECTION;
    invalid_plane.pixel_format = INKPOD_STORAGE_BINARY8;
    invalid_plane.name_utf8 = invalid_plane_name.data();
    invalid_plane.name_bytes = invalid_plane_name.size();
    if (inkpod_core_get_document_info(core, &before_invalid_tree) != INKPOD_STATUS_OK
        || inkpod_core_tree_edit(core, &invalid_plane, &dispatch, &tree_object_id)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_get_document_info(core, &after_invalid_tree) != INKPOD_STATUS_OK
        || after_invalid_tree.document_revision != before_invalid_tree.document_revision) {
        return 42;
    }
    tree_edit.operation = INKPOD_TREE_DELETE_LAYER;
    if (inkpod_core_tree_edit(core, &tree_edit, &dispatch, &tree_object_id)
            != INKPOD_STATUS_OK
        || inkpod_core_undo(core, &dispatch) != INKPOD_STATUS_OK) {
        return 37;
    }
    constexpr std::array<std::uint8_t, 6U> vector_layer_name{
        'V', 'e', 'c', 't', 'o', 'r'};
    tree_edit = InkpodTreeEdit{};
    tree_edit.struct_size = sizeof(tree_edit);
    tree_edit.operation = INKPOD_TREE_CREATE_LAYER;
    tree_edit.kind = INKPOD_LAYER_VECTOR_COLORING;
    tree_edit.name_utf8 = vector_layer_name.data();
    tree_edit.name_bytes = vector_layer_name.size();
    std::uint64_t vector_layer_id{};
    if (inkpod_core_tree_edit(core, &tree_edit, &dispatch, &vector_layer_id)
            != INKPOD_STATUS_OK
        || vector_layer_id == 0U) {
        return 48;
    }
    InkpodNodeInfo vector_plane{};
    vector_plane.struct_size = sizeof(vector_plane);
    if (inkpod_core_node_get(core, 2U, 1U, &vector_plane) != INKPOD_STATUS_OK
        || vector_plane.parent_id != vector_layer_id
        || vector_plane.kind != INKPOD_TYPED_PLANE_COLOR_TRACE) {
        return 49;
    }
    const std::uint64_t vector_trace_plane_id = vector_plane.id;
    if (inkpod_core_node_get(core, 2U, 2U, &vector_plane) != INKPOD_STATUS_OK
        || vector_plane.kind != INKPOD_TYPED_PLANE_VECTOR_FILL) {
        return 50;
    }
    const std::uint64_t vector_fill_plane_id = vector_plane.id;
    constexpr auto point = [](float x, float y) noexcept {
        return InkpodVectorPoint{x, y};
    };
    constexpr auto vector_line = [](InkpodVectorPoint start, InkpodVectorPoint end) noexcept {
        return InkpodVectorCubicSegment{
            sizeof(InkpodVectorCubicSegment),
            0U,
            start,
            InkpodVectorPoint{
                (start.x * 2.0F + end.x) / 3.0F,
                (start.y * 2.0F + end.y) / 3.0F},
            InkpodVectorPoint{
                (start.x + end.x * 2.0F) / 3.0F,
                (start.y + end.y * 2.0F) / 3.0F},
            end,
            1.0F,
            3.0F};
    };
    constexpr std::array<InkpodVectorPoint, 5U> vector_corners{
        point(100.0F, 100.0F),
        point(400.0F, 100.0F),
        point(400.0F, 400.0F),
        point(100.0F, 400.0F),
        point(100.0F, 100.0F)};
    const std::array<InkpodVectorCubicSegment, 4U> vector_segments{
        vector_line(vector_corners[0], vector_corners[1]),
        vector_line(vector_corners[1], vector_corners[2]),
        vector_line(vector_corners[2], vector_corners[3]),
        vector_line(vector_corners[3], vector_corners[4])};
    const InkpodVectorPathInput vector_path{
        sizeof(InkpodVectorPathInput),
        0U,
        INKPOD_VECTOR_PATH_CLOSED,
        vector_trace_plane_id,
        InkpodColorValue{
            sizeof(InkpodColorValue),
            INKPOD_COLOR_DEPTH_8,
            20U,
            40U,
            60U,
            255U},
        vector_segments.data(),
        vector_segments.size(),
        sizeof(InkpodVectorCubicSegment)};
    std::uint64_t vector_path_id{};
    if (inkpod_core_vector_add_path(core, &vector_path, &dispatch, &vector_path_id)
            != INKPOD_STATUS_OK
        || vector_path_id == 0U) {
        return 51;
    }
    InkpodVectorCubicSegment short_vector_segment = vector_segments[0];
    short_vector_segment.struct_size = sizeof(std::uint32_t);
    InkpodVectorPathInput short_vector_path = vector_path;
    short_vector_path.segments = &short_vector_segment;
    short_vector_path.segment_count = 1U;
    std::uint64_t rejected_path_id{UINT64_MAX};
    if (inkpod_core_vector_add_path(
            core, &short_vector_path, &dispatch, &rejected_path_id)
            != INKPOD_STATUS_INCOMPATIBLE_ABI
        || rejected_path_id != 0U) {
        return 52;
    }
    const InkpodVectorFillInput vector_fill{
        sizeof(InkpodVectorFillInput),
        0U,
        0U,
        vector_fill_plane_id,
        InkpodColorValue{
            sizeof(InkpodColorValue),
            INKPOD_COLOR_DEPTH_16,
            60000U,
            1000U,
            2000U,
            50000U},
        &vector_path_id,
        1U};
    std::uint64_t vector_fill_id{};
    if (inkpod_core_vector_add_fill(core, &vector_fill, &dispatch, &vector_fill_id)
            != INKPOD_STATUS_OK
        || vector_fill_id == 0U) {
        return 53;
    }
    const InkpodVectorSelectionInput vector_selection{
        sizeof(InkpodVectorSelectionInput),
        INKPOD_VECTOR_SELECT_FULLY_CONTAINED,
        0U,
        InkpodFrameRect{50, 50, 400, 400}};
    InkpodVectorSelectionBuffer vector_selection_output{};
    vector_selection_output.struct_size = sizeof(vector_selection_output);
    if (inkpod_core_vector_select(core, &vector_selection, &vector_selection_output)
            != INKPOD_STATUS_BUFFER_TOO_SMALL
        || vector_selection_output.range_count != 1U
        || vector_selection_output.fill_count != 0U) {
        return 56;
    }
    std::vector<InkpodVectorSelectionRange> selected_ranges(
        static_cast<std::size_t>(vector_selection_output.range_count));
    vector_selection_output.ranges = selected_ranges.data();
    vector_selection_output.range_capacity = selected_ranges.size();
    if (inkpod_core_vector_select(core, &vector_selection, &vector_selection_output)
            != INKPOD_STATUS_OK
        || selected_ranges[0].struct_size != sizeof(InkpodVectorSelectionRange)
        || selected_ranges[0].path_id != vector_path_id
        || selected_ranges[0].start_million != 0U
        || selected_ranges[0].end_million != 1000000U) {
        return 57;
    }
    const InkpodVectorRasterizeInput vector_rasterize{
        sizeof(InkpodVectorRasterizeInput),
        0U,
        0U,
        vector_layer_id,
        1U,
        0U};
    InkpodVectorRasterBuffer vector_raster{};
    vector_raster.struct_size = sizeof(vector_raster);
    if (inkpod_core_vector_rasterize(core, &vector_rasterize, &vector_raster)
            != INKPOD_STATUS_OK
        || vector_raster.width != document.width || vector_raster.height != document.height
        || vector_raster.stride_bytes != document.width * 4U
        || vector_raster.required_bytes
            != static_cast<std::uint64_t>(document.width) * document.height * 4U) {
        return 58;
    }
    std::vector<std::uint8_t> vector_pixels(
        static_cast<std::size_t>(vector_raster.required_bytes));
    vector_raster.pixels = vector_pixels.data();
    vector_raster.pixel_capacity = vector_pixels.size();
    if (inkpod_core_vector_rasterize(core, &vector_rasterize, &vector_raster)
            != INKPOD_STATUS_OK
        || vector_pixels[static_cast<std::size_t>(200U * vector_raster.stride_bytes + 200U * 4U + 3U)]
            == 0U) {
        return 59;
    }
    snapshot = nullptr;
    if (inkpod_core_build_snapshot(core, &options, &snapshot) != INKPOD_STATUS_OK
        || snapshot == nullptr) {
        return 54;
    }
    InkpodSnapshotVectorView vector_view{};
    vector_view.struct_size = sizeof(vector_view);
    if (inkpod_snapshot_get_vectors(snapshot, &vector_view) != INKPOD_STATUS_OK
        || vector_view.abi_version != INKPOD_ABI_VERSION
        || vector_view.segment_count != vector_segments.size()
        || vector_view.segment_stride_bytes != sizeof(InkpodSnapshotVectorSegment)
        || vector_view.fill_count != 1U
        || vector_view.fill_stride_bytes != sizeof(InkpodSnapshotVectorFill)
        || vector_view.boundary_path_count != 1U || vector_view.segments == nullptr
        || vector_view.fills == nullptr || vector_view.boundary_path_ids == nullptr
        || vector_view.segments->path_id != vector_path_id
        || vector_view.segments->width_start != 1.0F
        || vector_view.segments->width_end != 3.0F
        || vector_view.fills->fill_id != vector_fill_id
        || *vector_view.boundary_path_ids != vector_path_id
        || inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK) {
        return 55;
    }
    const InkpodRasterVectorizeInput raster_vectorize{
        sizeof(InkpodRasterVectorizeInput), 1U, 0U, document.color_plane_id, vector_layer_id};
    std::uint64_t vectorized_fill_count{};
    if (inkpod_core_raster_vectorize(
            core, &raster_vectorize, &dispatch, &vectorized_fill_count)
            != INKPOD_STATUS_OK
        || vectorized_fill_count == 0U) {
        return 60;
    }
    const std::array<InkpodColorValue, 2> palette{
        InkpodColorValue{sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 1U, 2U, 3U, 255U},
        InkpodColorValue{
            sizeof(InkpodColorValue),
            INKPOD_COLOR_DEPTH_16,
            1U,
            257U,
            32769U,
            65534U}};
    const InkpodColorArray palette_input{
        sizeof(InkpodColorArray),
        0U,
        INKPOD_FEATURE_NONE,
        palette.data(),
        palette.size(),
        sizeof(InkpodColorValue)};
    dispatch = InkpodDispatchResult{};
    dispatch.struct_size = sizeof(dispatch);
    if (inkpod_core_palette_set(core, &palette_input, &dispatch) != INKPOD_STATUS_OK) {
        return 32;
    }
    std::array<InkpodColorValue, 2> palette_copy{};
    InkpodColorBuffer palette_output{
        sizeof(InkpodColorBuffer),
        0U,
        INKPOD_FEATURE_NONE,
        palette_copy.data(),
        palette_copy.size(),
        sizeof(InkpodColorValue),
        0U};
    if (inkpod_core_palette_get(core, &palette_output) != INKPOD_STATUS_OK
        || palette_output.color_count != palette.size()
        || palette_copy[1].depth != INKPOD_COLOR_DEPTH_16
        || palette_copy[1].blue != 32769U) {
        return 33;
    }
    std::array<InkpodStrokeSample, 64> stroke_samples{};
    for (std::size_t index = 0; index < stroke_samples.size(); ++index) {
        stroke_samples[index] = InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            20.0F + static_cast<float>(index),
            30.0F,
            0.75F,
            0U};
    }
    InkpodStrokeInput stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        stroke_samples.data(),
        stroke_samples.size(),
        sizeof(InkpodStrokeSample)};
    dispatch = InkpodDispatchResult{};
    dispatch.struct_size = sizeof(dispatch);
    if (inkpod_core_apply_stroke(core, &stroke, &dispatch) != INKPOD_STATUS_OK
        || dispatch.accepted_command_count != 1U
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK) {
        return 25;
    }
    const std::uint64_t main_checksum = document.main_plane_checksum;
    stroke.plane = INKPOD_PLANE_COLOR;
    stroke.color_rgba = UINT32_C(0xdc281eff);
    if (inkpod_core_apply_stroke(core, &stroke, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.main_plane_checksum != main_checksum) {
        return 26;
    }
    const InkpodColorValue selected_color{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        UINT16_C(220),
        UINT16_C(40),
        UINT16_C(30),
        UINT16_C(255)};
    if (inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR) != INKPOD_STATUS_OK
        || inkpod_core_select_color(
               core,
               &selected_color,
               0U,
               0U,
               INKPOD_SELECTION_NEW,
               &dispatch) != INKPOD_STATUS_OK) {
        return 41;
    }
    const InkpodSelectionInput selection{
        sizeof(InkpodSelectionInput),
        INKPOD_SELECTION_RECTANGLE,
        INKPOD_SELECTION_NEW,
        0U,
        InkpodFrameRect{20, 30, 64, 1},
        nullptr,
        0U,
        0U,
        0.0F,
        0U,
        0U,
        0U,
        0U};
    InkpodClipboard* clipboard{};
    InkpodSelectionInput invalid_selection = selection;
    invalid_selection.point_stride_bytes = sizeof(InkpodSelectionPoint);
    if (inkpod_core_apply_selection(core, &invalid_selection, &dispatch)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_apply_selection(core, &selection, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_clipboard_copy(core, &clipboard) != INKPOD_STATUS_OK
        || clipboard == nullptr
        || inkpod_clipboard_release(&clipboard) != INKPOD_STATUS_OK
        || inkpod_clipboard_release(&clipboard) != INKPOD_STATUS_OK
        || clipboard != nullptr) {
        return 38;
    }
    if (inkpod_core_undo(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_redo(core, &dispatch) != INKPOD_STATUS_OK) {
        return 27;
    }
    snapshot = nullptr;
    const InkpodViewInput flip{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_FLIP_HORIZONTAL,
        0U,
        0.0,
        0.0,
        0.0,
        0.0};
    if (inkpod_core_apply_view(core, &flip, &document) != INKPOD_STATUS_OK) {
        return 39;
    }
    std::uint64_t guide_id{};
    const InkpodGridInput grid{
        sizeof(InkpodGridInput), 0U, 0, 0, 16U, 16U, 2U, 0U};
    const InkpodViewInput show_grid{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_SET_GRID_VISIBLE,
        0U,
        1.0,
        0.0,
        0.0,
        0.0};
    if (inkpod_core_guide_add(
            core, INKPOD_GUIDE_VERTICAL, 12, &dispatch, &guide_id) != INKPOD_STATUS_OK
        || inkpod_core_grid_set(core, &grid, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_apply_view(core, &show_grid, &document) != INKPOD_STATUS_OK) {
        return 43;
    }
    if (inkpod_core_build_snapshot(core, &options, &snapshot) != INKPOD_STATUS_OK
        || snapshot == nullptr) {
        return 28;
    }
    view = InkpodSnapshotView{};
    view.struct_size = sizeof(view);
    InkpodSnapshotTransform transform{};
    transform.struct_size = sizeof(transform);
    InkpodSnapshotOverlay overlay{};
    overlay.struct_size = sizeof(overlay);
    if (inkpod_snapshot_get_view(snapshot, &view) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_transform(snapshot, &transform) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_overlay(snapshot, &overlay) != INKPOD_STATUS_OK
        || view.tiles == nullptr || view.tile_count == 0U
        || transform.document_width != 1920U || transform.document_height != 1080U
        || (transform.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) == 0U
        || (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) == 0U
        || overlay.grid_spacing_x != 16U || overlay.grid_subdivisions != 2U
        || overlay.guide_count != 1U || overlay.guides == nullptr
        || overlay.guides->id != guide_id) {
        return 29;
    }
    std::uint64_t second_view{};
    InkpodSnapshot* second_snapshot{};
    InkpodSnapshotView second_snapshot_view{};
    second_snapshot_view.struct_size = sizeof(second_snapshot_view);
    InkpodSnapshotTransform second_transform{};
    second_transform.struct_size = sizeof(second_transform);
    const InkpodViewInput second_pan{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_PAN_BY,
        0U,
        5.0,
        0.0,
        0.0,
        0.0};
    if (inkpod_core_view_create(core, &second_view) != INKPOD_STATUS_OK
        || inkpod_core_view_apply(core, second_view, &second_pan) != INKPOD_STATUS_OK
        || inkpod_core_build_snapshot_for_view(
               core, second_view, &options, &second_snapshot) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_view(second_snapshot, &second_snapshot_view)
            != INKPOD_STATUS_OK
        || inkpod_snapshot_get_transform(second_snapshot, &second_transform)
            != INKPOD_STATUS_OK
        || second_snapshot_view.revision != view.revision
        || second_transform.pan_x == transform.pan_x
        || inkpod_snapshot_release(&second_snapshot) != INKPOD_STATUS_OK
        || inkpod_core_view_close(core, second_view) != INKPOD_STATUS_OK) {
        return 40;
    }
    std::uint32_t shortcut_command{};
    if (inkpod_core_shortcut_rebind(
            core,
            99U,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL) != INKPOD_STATUS_OK
        || inkpod_core_shortcut_resolve(
               core,
               static_cast<std::uint32_t>('Z'),
               INKPOD_SHORTCUT_MODIFIER_CONTROL,
               &shortcut_command) != INKPOD_STATUS_OK
        || shortcut_command != 99U || inkpod_core_shortcut_reset(core) != INKPOD_STATUS_OK
        || inkpod_core_shortcut_resolve(
               core,
               static_cast<std::uint32_t>('Z'),
               INKPOD_SHORTCUT_MODIFIER_CONTROL,
               &shortcut_command) != INKPOD_STATUS_OK
        || shortcut_command != 1U) {
        return 44;
    }
    if (inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK
        || inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK
        || inkpod_core_destroy(&core) != INKPOD_STATUS_OK) {
        return 30;
    }
    return 0;
}
