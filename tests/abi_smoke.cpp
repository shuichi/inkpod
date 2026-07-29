#include "inkpod/core_ffi.h"

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>
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
static_assert(sizeof(InkpodRasterSourceInput) == 96U);
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
static_assert(sizeof(InkpodCurvePoint) == 16U);
static_assert(sizeof(InkpodFilterInput) == 72U);
static_assert(sizeof(InkpodFilterPreviewInfo) == 40U);
static_assert(sizeof(InkpodGradientStop) == 32U);
static_assert(sizeof(InkpodGradientInput) == 88U);
static_assert(sizeof(InkpodAirbrushInput) == 72U);
static_assert(sizeof(InkpodBoundaryAirbrushInput) == 72U);
static_assert(sizeof(InkpodBlurEffectInput) == 40U);
static_assert(sizeof(InkpodStampInput) == 56U);
static_assert(sizeof(InkpodAlphaEditInput) == 64U);
static_assert(sizeof(InkpodAirbrushGestureInput) == 96U);
static_assert(sizeof(InkpodStampGestureInput) == 104U);
static_assert(sizeof(InkpodBlurToolInput) == 72U);
static_assert(sizeof(InkpodDustInput) == 80U);
static_assert(sizeof(InkpodTaskInfo) == 32U);
static_assert(sizeof(InkpodBatchInput) == 48U);
static_assert(sizeof(InkpodBatchColorPairInput) == 48U);
static_assert(sizeof(InkpodBatchSeedInput) == 64U);
static_assert(sizeof(InkpodBatchOperationInput) == 256U);
static_assert(sizeof(InkpodBatchGraphInput) == 144U);
static_assert(sizeof(InkpodBatchGraphInfo) == 40U);
static_assert(sizeof(InkpodBatchPreviewItem) == 56U);
static_assert(sizeof(InkpodBatchReportInfo) == 32U);
static_assert(sizeof(InkpodBatchReportItem) == 56U);
static_assert(sizeof(InkpodSnapshotVectorSegment) == 80U);
static_assert(sizeof(InkpodSnapshotVectorFill) == 48U);
static_assert(sizeof(InkpodSnapshotVectorView) == 80U);
static_assert(sizeof(InkpodLayerThumbnailBuffer) == 80U);

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
        || (document.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
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
    const InkpodRasterSourceInput light_source{
        sizeof(InkpodRasterSourceInput),
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
            InkpodRasterSourceInput{
                sizeof(InkpodRasterSourceInput), INKPOD_STORAGE_RGBA8, 0U, 5U, 1U, 1U,
                1U, 1U, 96000U, 96000U, InkpodFrameRect{0, 0, 1, 1},
                sequence_pixel_a.data(), sequence_pixel_a.size(), 4U}},
        InkpodSequenceCellInput{
            sizeof(InkpodSequenceCellInput),
            0U,
            sequence_name_b.data(),
            sequence_name_b.size(),
            InkpodRasterSourceInput{
                sizeof(InkpodRasterSourceInput), INKPOD_STORAGE_RGBA8, 0U, 5U, 2U, 1U,
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
    InkpodLayerThumbnailBuffer layer_thumbnail{};
    layer_thumbnail.struct_size = sizeof(layer_thumbnail);
    layer_thumbnail.layer_id = duplicate_layer;
    layer_thumbnail.maximum_width = 80U;
    layer_thumbnail.maximum_height = 60U;
    if (inkpod_core_layer_thumbnail(core, &layer_thumbnail) != INKPOD_STATUS_OK
        || layer_thumbnail.width == 0U || layer_thumbnail.height == 0U
        || layer_thumbnail.width > 80U || layer_thumbnail.height > 60U
        || layer_thumbnail.stride_bytes != layer_thumbnail.width * 4U
        || layer_thumbnail.required_bytes
            != static_cast<std::uint64_t>(layer_thumbnail.stride_bytes)
                * layer_thumbnail.height) {
        return 89;
    }
    std::vector<std::uint8_t> layer_thumbnail_pixels(
        static_cast<std::size_t>(layer_thumbnail.required_bytes));
    layer_thumbnail.pixels_rgba8 = layer_thumbnail_pixels.data();
    layer_thumbnail.pixel_capacity = layer_thumbnail_pixels.size();
    if (inkpod_core_layer_thumbnail(core, &layer_thumbnail) != INKPOD_STATUS_OK) {
        return 90;
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
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || (document.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
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
    if (inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK) {
        return 60;
    }
    const std::uint64_t pre_filter_checksum = document.color_plane_checksum;
    InkpodFilterInput filter{};
    filter.struct_size = sizeof(filter);
    filter.kind = INKPOD_FILTER_INVERT;
    filter.plane_id = document.color_plane_id;
    filter.channel = INKPOD_FILTER_CHANNEL_RGB;
    InkpodFilterPreviewInfo preview{};
    preview.struct_size = sizeof(preview);
    if (inkpod_core_filter_preview_begin(core, &filter, &preview) != INKPOD_STATUS_OK
        || preview.plane_id != document.color_plane_id
        || preview.base_checksum != pre_filter_checksum
        || preview.preview_checksum == preview.base_checksum
        || inkpod_core_filter_preview_cancel(core, &preview) != INKPOD_STATUS_OK
        || preview.base_checksum != pre_filter_checksum
        || preview.preview_checksum != pre_filter_checksum
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.color_plane_checksum != pre_filter_checksum) {
        return 61;
    }
    if (inkpod_core_filter_preview_begin(core, &filter, &preview) != INKPOD_STATUS_OK
        || inkpod_core_filter_preview_apply(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.color_plane_checksum == pre_filter_checksum
        || inkpod_core_undo(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.color_plane_checksum != pre_filter_checksum
        || inkpod_core_redo(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK) {
        return 62;
    }
    filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
    filter.parameter_0 = 100;
    filter.parameter_1 = 200;
    constexpr std::array<std::uint8_t, 10> adjustment_name{
        'M', '6', ' ', 'A', 'd', 'j', 'u', 's', 't', '1'};
    std::uint64_t adjustment_layer_id{};
    const std::uint64_t source_checksum = document.color_plane_checksum;
    if (inkpod_core_adjustment_create(
            core,
            &filter,
            adjustment_name.data(),
            adjustment_name.size(),
            &dispatch,
            &adjustment_layer_id) != INKPOD_STATUS_OK
        || adjustment_layer_id == 0U
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.color_plane_checksum != source_checksum) {
        return 63;
    }
    filter.parameter_0 = 200;
    filter.parameter_1 = -100;
    if (inkpod_core_adjustment_update(
            core, adjustment_layer_id, &filter, &dispatch) != INKPOD_STATUS_OK) {
        return 64;
    }

    const auto color16 = [](std::uint16_t red,
                            std::uint16_t green,
                            std::uint16_t blue,
                            std::uint16_t alpha) noexcept {
        InkpodColorValue color{};
        color.struct_size = sizeof(color);
        color.depth = INKPOD_COLOR_DEPTH_16;
        color.red = red;
        color.green = green;
        color.blue = blue;
        color.alpha = alpha;
        return color;
    };
    std::array<InkpodGradientStop, 3> gradient_stops{};
    for (auto& stop : gradient_stops) {
        stop.struct_size = sizeof(stop);
    }
    gradient_stops[0].position_milli = 0U;
    gradient_stops[0].color = color16(65535U, 0U, 0U, 65535U);
    gradient_stops[1].position_milli = 500U;
    gradient_stops[1].color = color16(0U, 65535U, 0U, 32768U);
    gradient_stops[2].position_milli = 1000U;
    gradient_stops[2].color = color16(0U, 0U, 65535U, 65535U);
    InkpodGradientInput gradient{};
    gradient.struct_size = sizeof(gradient);
    gradient.kind = INKPOD_GRADIENT_LINEAR;
    gradient.plane_id = document.color_plane_id;
    gradient.mode = INKPOD_GRADIENT_OVERWRITE;
    gradient.start_x_milli = 500;
    gradient.start_y_milli = 500;
    gradient.end_x_milli = 3500;
    gradient.end_y_milli = 500;
    gradient.stops = gradient_stops.data();
    gradient.stop_count = gradient_stops.size();
    gradient.stop_stride_bytes = sizeof(InkpodGradientStop);
    if (inkpod_core_effect_gradient(core, &gradient, &dispatch) != INKPOD_STATUS_OK) {
        return 65;
    }

    InkpodAirbrushInput airbrush{};
    airbrush.struct_size = sizeof(airbrush);
    airbrush.plane_id = document.color_plane_id;
    airbrush.center_x_milli = 2000;
    airbrush.center_y_milli = 2000;
    airbrush.radius_milli = 1500U;
    airbrush.hardness_milli = 500U;
    airbrush.opacity_milli = 500U;
    airbrush.color = color16(65535U, 65535U, 65535U, 65535U);
    if (inkpod_core_effect_airbrush(core, &airbrush, &dispatch) != INKPOD_STATUS_OK) {
        return 66;
    }

    const std::array<InkpodColorValue, 2> boundary_colors{
        color16(65535U, 0U, 0U, 65535U), color16(0U, 65535U, 0U, 65535U)};
    InkpodBoundaryAirbrushInput boundary{};
    boundary.struct_size = sizeof(boundary);
    boundary.plane_id = document.color_plane_id;
    boundary.width = 1U;
    boundary.strength_milli = 1000U;
    boundary.colors.struct_size = sizeof(boundary.colors);
    boundary.colors.colors = boundary_colors.data();
    boundary.colors.color_count = boundary_colors.size();
    boundary.colors.color_stride_bytes = sizeof(InkpodColorValue);
    if (inkpod_core_effect_boundary_airbrush(core, &boundary, &dispatch)
        != INKPOD_STATUS_OK) {
        return 67;
    }

    InkpodBlurEffectInput blur{};
    blur.struct_size = sizeof(blur);
    blur.plane_id = document.color_plane_id;
    blur.radius = 1U;
    blur.strength_milli = 500U;
    if (inkpod_core_effect_blur(core, &blur, &dispatch) != INKPOD_STATUS_OK) {
        return 68;
    }

    InkpodStampInput stamp{};
    stamp.struct_size = sizeof(stamp);
    stamp.plane_id = document.color_plane_id;
    stamp.source_x = 0;
    stamp.source_y = 0;
    stamp.destination_x = 2;
    stamp.destination_y = 2;
    stamp.width = 2U;
    stamp.height = 2U;
    stamp.opacity_milli = 1000U;
    if (inkpod_core_effect_stamp(core, &stamp, &dispatch) != INKPOD_STATUS_OK) {
        return 69;
    }

    if (inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK) {
        return 70;
    }
    const std::uint64_t before_alpha_checksum = document.color_plane_checksum;
    std::vector<std::uint8_t> alpha_pixels(
        static_cast<std::size_t>(document.width) * document.height, 64U);
    InkpodAlphaEditInput alpha{};
    alpha.struct_size = sizeof(alpha);
    alpha.pixel_format = INKPOD_STORAGE_GRAYSCALE8;
    alpha.plane_id = document.color_plane_id;
    alpha.width = document.width;
    alpha.height = document.height;
    alpha.pixels = alpha_pixels.data();
    alpha.pixel_bytes = alpha_pixels.size();
    alpha.row_stride_bytes = document.width;
    if (inkpod_core_alpha_edit(core, &alpha, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.color_plane_checksum == before_alpha_checksum
        || inkpod_core_undo(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.color_plane_checksum != before_alpha_checksum) {
        return 71;
    }

    const std::array<InkpodStrokeSample, 2> gesture_samples{
        InkpodStrokeSample{sizeof(InkpodStrokeSample), 0U, 1.0F, 1.0F, 0.25F, 0U},
        InkpodStrokeSample{sizeof(InkpodStrokeSample), 0U, 4.0F, 3.0F, 1.0F, 0U}};
    InkpodAirbrushGestureInput airbrush_gesture{};
    airbrush_gesture.struct_size = sizeof(airbrush_gesture);
    airbrush_gesture.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
    airbrush_gesture.feature_flags =
        INKPOD_EFFECT_FLAG_PRESSURE_SIZE | INKPOD_EFFECT_FLAG_PRESSURE_OPACITY;
    airbrush_gesture.plane_id = document.color_plane_id;
    airbrush_gesture.radius_milli = 1000U;
    airbrush_gesture.hardness_milli = 500U;
    airbrush_gesture.spacing_milli = 500U;
    airbrush_gesture.opacity_milli = 750U;
    airbrush_gesture.continuous_dabs = 1U;
    airbrush_gesture.color = color16(65535U, 0U, 0U, 65535U);
    airbrush_gesture.samples = gesture_samples.data();
    airbrush_gesture.sample_count = gesture_samples.size();
    airbrush_gesture.sample_stride_bytes = sizeof(InkpodStrokeSample);
    if (inkpod_core_effect_airbrush_gesture(core, &airbrush_gesture, &dispatch)
        != INKPOD_STATUS_OK) {
        return 72;
    }

    InkpodStampGestureInput stamp_gesture{};
    stamp_gesture.struct_size = sizeof(stamp_gesture);
    stamp_gesture.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
    stamp_gesture.feature_flags = INKPOD_EFFECT_FLAG_PRESSURE_OPACITY;
    stamp_gesture.plane_id = document.color_plane_id;
    stamp_gesture.source = gesture_samples[0];
    stamp_gesture.radius_milli = 1000U;
    stamp_gesture.hardness_milli = 1000U;
    stamp_gesture.spacing_milli = 500U;
    stamp_gesture.opacity_milli = 750U;
    stamp_gesture.shape = INKPOD_STAMP_ROUND;
    stamp_gesture.samples = gesture_samples.data();
    stamp_gesture.sample_count = gesture_samples.size();
    stamp_gesture.sample_stride_bytes = sizeof(InkpodStrokeSample);
    if (inkpod_core_effect_stamp_gesture(core, &stamp_gesture, &dispatch)
        != INKPOD_STATUS_OK) {
        return 73;
    }

    InkpodBlurToolInput blur_tool{};
    blur_tool.struct_size = sizeof(blur_tool);
    blur_tool.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
    blur_tool.feature_flags = INKPOD_EFFECT_FLAG_PRESSURE_SIZE;
    blur_tool.plane_id = document.color_plane_id;
    blur_tool.radius = 1U;
    blur_tool.strength_milli = 500U;
    blur_tool.shape = INKPOD_SELECTION_TRACE;
    blur_tool.diameter = 1.0F;
    blur_tool.samples = gesture_samples.data();
    blur_tool.sample_count = gesture_samples.size();
    blur_tool.sample_stride_bytes = sizeof(InkpodStrokeSample);
    if (inkpod_core_effect_blur_tool(core, &blur_tool, &dispatch) != INKPOD_STATUS_OK) {
        return 74;
    }

    if (inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK) {
        return 75;
    }
    const std::uint64_t before_cancelled_filter = document.color_plane_checksum;
    filter.kind = INKPOD_FILTER_INVERT;
    filter.parameter_0 = 0;
    filter.parameter_1 = 0;
    InkpodTask* task{};
    if (inkpod_task_create(&task) != INKPOD_STATUS_OK
        || inkpod_task_cancel(task) != INKPOD_STATUS_OK
        || inkpod_core_filter_preview_begin_task(core, &filter, task, &preview)
            != INKPOD_STATUS_CANCELLED
        || inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK
        || document.color_plane_checksum != before_cancelled_filter) {
        return 76;
    }
    InkpodTaskInfo task_info{};
    task_info.struct_size = sizeof(task_info);
    if (inkpod_task_query(task, &task_info) != INKPOD_STATUS_OK
        || task_info.state != INKPOD_TASK_CANCELLED
        || inkpod_task_release(&task) != INKPOD_STATUS_OK
        || inkpod_task_release(&task) != INKPOD_STATUS_OK) {
        return 77;
    }

    InkpodTask* dust_task{};
    InkpodDustInput dust{};
    dust.struct_size = sizeof(dust);
    dust.mode = INKPOD_DUST_REMOVE_FOREGROUND;
    dust.plane_id = document.color_plane_id;
    dust.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
    dust.maximum_pixels = 1U;
    if (inkpod_task_create(&dust_task) != INKPOD_STATUS_OK
        || inkpod_core_dust_preview_begin(core, &dust, dust_task, &preview)
            != INKPOD_STATUS_OK
        || inkpod_task_query(dust_task, &task_info) != INKPOD_STATUS_OK
        || task_info.state != INKPOD_TASK_COMPLETED
        || inkpod_core_filter_preview_cancel(core, &preview) != INKPOD_STATUS_OK
        || inkpod_task_release(&dust_task) != INKPOD_STATUS_OK) {
        return 78;
    }

    gradient.feature_flags = INKPOD_GRADIENT_FLAG_CONSTRAIN_45;
    if (inkpod_core_alpha_gradient(core, &gradient, &dispatch) != INKPOD_STATUS_OK) {
        return 79;
    }
    InkpodViewInput alpha_view{};
    alpha_view.struct_size = sizeof(alpha_view);
    alpha_view.kind = INKPOD_VIEW_SET_ALPHA_VISIBLE;
    alpha_view.value1 = 1.0;
    if (inkpod_core_apply_view(core, &alpha_view, &document) != INKPOD_STATUS_OK) {
        return 80;
    }
    InkpodSnapshot* alpha_snapshot{};
    if (inkpod_core_build_snapshot(core, &options, &alpha_snapshot) != INKPOD_STATUS_OK) {
        return 81;
    }
    InkpodSnapshotOverlay alpha_overlay{};
    alpha_overlay.struct_size = sizeof(alpha_overlay);
    if (inkpod_snapshot_get_overlay(alpha_snapshot, &alpha_overlay) != INKPOD_STATUS_OK
        || (alpha_overlay.flags & INKPOD_SNAPSHOT_OVERLAY_ALPHA_VIEW) == 0U
        || inkpod_snapshot_release(&alpha_snapshot) != INKPOD_STATUS_OK) {
        return 82;
    }

    const std::string batch_name{"Batch ABI Smoke"};
    const std::string batch_output{"."};
    const std::string batch_basename{"abi-smoke"};
    InkpodBatchInput batch_input{};
    batch_input.struct_size = sizeof(batch_input);
    batch_input.kind = INKPOD_BATCH_INPUT_CURRENT_SEQUENCE;
    InkpodBatchColorPairInput batch_pair{};
    batch_pair.struct_size = sizeof(batch_pair);
    batch_pair.enabled = 1U;
    batch_pair.old_color = color16(0U, 0U, 0U, 0U);
    batch_pair.new_color = color16(65535U, 0U, 0U, 65535U);
    InkpodBatchOperationInput batch_operation{};
    batch_operation.struct_size = sizeof(batch_operation);
    batch_operation.version = INKPOD_BATCH_GRAPH_VERSION;
    batch_operation.kind = INKPOD_BATCH_OPERATION_COLOR_REPLACE;
    batch_operation.flags = INKPOD_BATCH_OPERATION_ENABLED;
    batch_operation.layer_kind = INKPOD_LAYER_BINARY_COLORING;
    batch_operation.plane_kind = INKPOD_TYPED_PLANE_COLOR;
    batch_operation.missing_policy = INKPOD_BATCH_MISSING_ERROR;
    batch_operation.color_pairs = &batch_pair;
    batch_operation.color_pair_count = 1U;
    batch_operation.color_pair_stride_bytes = sizeof(batch_pair);
    InkpodBatchGraphInput batch_graph_input{};
    batch_graph_input.struct_size = sizeof(batch_graph_input);
    batch_graph_input.version = INKPOD_BATCH_GRAPH_VERSION;
    batch_graph_input.name_utf8 = reinterpret_cast<const std::uint8_t*>(batch_name.data());
    batch_graph_input.name_bytes = batch_name.size();
    batch_graph_input.inputs = &batch_input;
    batch_graph_input.input_count = 1U;
    batch_graph_input.input_stride_bytes = sizeof(batch_input);
    batch_graph_input.operations = &batch_operation;
    batch_graph_input.operation_count = 1U;
    batch_graph_input.operation_stride_bytes = sizeof(batch_operation);
    batch_graph_input.output_policy = INKPOD_BATCH_OUTPUT_NEW_SAVE;
    batch_graph_input.failure_policy = INKPOD_BATCH_FAILURE_CONTINUE;
    batch_graph_input.output_folder_utf8 =
        reinterpret_cast<const std::uint8_t*>(batch_output.data());
    batch_graph_input.output_folder_bytes = batch_output.size();
    batch_graph_input.basename_utf8 =
        reinterpret_cast<const std::uint8_t*>(batch_basename.data());
    batch_graph_input.basename_bytes = batch_basename.size();
    batch_graph_input.start_number = 1U;
    InkpodBatchGraph* batch_graph{};
    if (inkpod_batch_graph_create(&batch_graph_input, &batch_graph) != INKPOD_STATUS_OK
        || batch_graph == nullptr) {
        return 83;
    }
    InkpodBatchGraphInfo batch_graph_info{};
    batch_graph_info.struct_size = sizeof(batch_graph_info);
    if (inkpod_batch_graph_get_info(batch_graph, &batch_graph_info) != INKPOD_STATUS_OK
        || batch_graph_info.input_count != 1U
        || batch_graph_info.operation_count != 1U
        || batch_graph_info.output_policy != INKPOD_BATCH_OUTPUT_NEW_SAVE) {
        return 84;
    }
    InkpodBatchPreview* batch_preview{};
    std::uint64_t batch_preview_count{};
    InkpodBatchPreviewItem batch_preview_item{};
    batch_preview_item.struct_size = sizeof(batch_preview_item);
    if (inkpod_core_batch_preview(
            core, batch_graph, INKPOD_BATCH_SCOPE_ALL, &batch_preview)
            != INKPOD_STATUS_OK
        || inkpod_batch_preview_count(batch_preview, &batch_preview_count)
            != INKPOD_STATUS_OK
        || batch_preview_count == 0U
        || inkpod_batch_preview_get(batch_preview, 0U, &batch_preview_item)
            != INKPOD_STATUS_OK
        || batch_preview_item.input_name_bytes == 0U
        || inkpod_batch_preview_release(&batch_preview) != INKPOD_STATUS_OK
        || inkpod_batch_preview_release(&batch_preview) != INKPOD_STATUS_OK) {
        return 85;
    }
    InkpodBatchTask* batch_task{};
    InkpodBatchReport* batch_report{};
    if (inkpod_batch_task_create(&batch_task) != INKPOD_STATUS_OK
        || inkpod_core_batch_execute(
               core,
               batch_graph,
               INKPOD_BATCH_SCOPE_ALL,
               INKPOD_BATCH_RUN_DRY | INKPOD_BATCH_RUN_PREVIEW_CONFIRMED,
               batch_task,
               &batch_report)
            != INKPOD_STATUS_OK) {
        return 86;
    }
    InkpodBatchReportInfo batch_report_info{};
    batch_report_info.struct_size = sizeof(batch_report_info);
    InkpodBatchReportItem batch_report_item{};
    batch_report_item.struct_size = sizeof(batch_report_item);
    if (inkpod_batch_report_get_info(batch_report, &batch_report_info)
            != INKPOD_STATUS_OK
        || batch_report_info.item_count == 0U
        || batch_report_info.failure_count != 0U
        || inkpod_batch_report_get(batch_report, 0U, &batch_report_item)
            != INKPOD_STATUS_OK
        || batch_report_item.outcome != INKPOD_BATCH_ITEM_DRY_RUN
        || inkpod_batch_report_release(&batch_report) != INKPOD_STATUS_OK
        || inkpod_batch_task_release(&batch_task) != INKPOD_STATUS_OK) {
        return 87;
    }
    const std::string batch_settings{"inkpod-batch-abi-smoke.inkbatch"};
    std::remove(batch_settings.c_str());
    InkpodBatchGraph* loaded_batch_graph{};
    if (inkpod_batch_graph_save(
            batch_graph,
            reinterpret_cast<const std::uint8_t*>(batch_settings.data()),
            batch_settings.size())
            != INKPOD_STATUS_OK
        || inkpod_batch_graph_load(
               reinterpret_cast<const std::uint8_t*>(batch_settings.data()),
               batch_settings.size(),
               &loaded_batch_graph)
            != INKPOD_STATUS_OK
        || loaded_batch_graph == nullptr
        || inkpod_batch_graph_release(&loaded_batch_graph) != INKPOD_STATUS_OK
        || inkpod_batch_graph_release(&batch_graph) != INKPOD_STATUS_OK
        || inkpod_batch_graph_release(&batch_graph) != INKPOD_STATUS_OK) {
        std::remove(batch_settings.c_str());
        return 88;
    }
    std::remove(batch_settings.c_str());
    if (inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK
        || inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK
        || inkpod_core_destroy(&core) != INKPOD_STATUS_OK) {
        return 30;
    }
    return 0;
}
