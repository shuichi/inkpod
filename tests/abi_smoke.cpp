#include "inkpod/core_ffi.h"

#include <array>
#include <cstddef>
#include <cstdint>
#include <thread>
#include <type_traits>

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
    if (inkpod_core_undo(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_redo(core, &dispatch) != INKPOD_STATUS_OK) {
        return 27;
    }
    snapshot = nullptr;
    if (inkpod_core_build_snapshot(core, &options, &snapshot) != INKPOD_STATUS_OK
        || snapshot == nullptr) {
        return 28;
    }
    view = InkpodSnapshotView{};
    view.struct_size = sizeof(view);
    InkpodSnapshotTransform transform{};
    transform.struct_size = sizeof(transform);
    if (inkpod_snapshot_get_view(snapshot, &view) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_transform(snapshot, &transform) != INKPOD_STATUS_OK
        || view.tiles == nullptr || view.tile_count == 0U
        || transform.document_width != 1920U || transform.document_height != 1080U) {
        return 29;
    }
    if (inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK
        || inkpod_core_destroy(&core) != INKPOD_STATUS_OK) {
        return 30;
    }
    return 0;
}
