#include "inkpod/core_ffi.h"
#include "app/inkscript_engine_smoke.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <type_traits>
#include <vector>

static_assert(std::is_standard_layout_v<InkpodCoreConfig>);
static_assert(std::is_standard_layout_v<InkpodSnapshotView>);
static_assert(sizeof(InkpodCoreConfig) == 16U);
static_assert(std::is_standard_layout_v<InkpodReplayContract>);
static_assert(std::is_standard_layout_v<InkpodCanonicalDigest>);
static_assert(sizeof(InkpodReplayContract) == 64U);
static_assert(sizeof(InkpodCanonicalDigest) == 40U);
static_assert(sizeof(InkpodPersistenceInfo) == 72U);
static_assert(sizeof(InkpodCompactionPlan) == 128U);
static_assert(sizeof(InkpodDispatchResult) == 24U);
static_assert(sizeof(InkpodSnapshotOptions) == 16U);
static_assert(sizeof(InkpodSnapshotTile) == 64U);
static_assert(sizeof(InkpodSnapshotView) == 48U);
static_assert(std::is_standard_layout_v<InkpodSnapshotRenderPass>);
static_assert(std::is_standard_layout_v<InkpodSnapshotRenderPlan>);
static_assert(sizeof(InkpodSnapshotRenderPass) == 48U);
static_assert(sizeof(InkpodSnapshotRenderPlan) == 40U);
static_assert(sizeof(InkpodCellCreateOptions) == 48U);
static_assert(sizeof(InkpodDocumentInfo) == 232U);
static_assert(sizeof(InkpodResourceUsage) == 136U);
static_assert(std::is_standard_layout_v<InkpodSnapshotSourceIdentity>);
static_assert(sizeof(InkpodSnapshotSourceIdentity) == 40U);
static_assert(std::is_standard_layout_v<InkpodSequenceCatalogInfo>);
static_assert(sizeof(InkpodSequenceCatalogInfo) == 32U);
static_assert(sizeof(InkpodStrokeSample) == 24U);
static_assert(sizeof(InkpodStrokeInput) == 72U);
static_assert(sizeof(InkpodViewInput) == 48U);
static_assert(sizeof(InkpodSnapshotTransform) == 48U);
static_assert(sizeof(InkpodSnapshotGuide) == 24U);
static_assert(sizeof(InkpodSnapshotOverlay) == 56U);
static_assert(std::is_standard_layout_v<InkpodShootingFrameInput>);
static_assert(std::is_standard_layout_v<InkpodShootingFrameInfo>);
static_assert(std::is_standard_layout_v<InkpodSnapshotShootingFrameView>);
static_assert(sizeof(InkpodShootingFrameInput) == 64U);
static_assert(sizeof(InkpodShootingFramePoint) == 16U);
static_assert(sizeof(InkpodShootingFrameInfo) == 136U);
static_assert(sizeof(InkpodSnapshotShootingFrameView) == 40U);
static_assert(std::is_standard_layout_v<InkpodSavedSelectionInfo>);
static_assert(sizeof(InkpodSavedSelectionInfo) == 40U);
static_assert(sizeof(InkpodColorValue) == 16U);
static_assert(std::is_standard_layout_v<InkpodObjectId>);
static_assert(std::is_standard_layout_v<InkpodPrimitiveRequestV3>);
static_assert(std::is_standard_layout_v<InkpodPrimitiveResultV3>);
static_assert(std::is_standard_layout_v<InkpodRasterAssetInputV3>);
static_assert(std::is_standard_layout_v<InkpodObjectInfoV3>);
static_assert(std::is_standard_layout_v<InkpodSnapshotInfoV3>);
static_assert(std::is_standard_layout_v<InkpodSnapshotTileInfoV3>);
static_assert(std::is_standard_layout_v<InkpodBufferCopyV3>);
static_assert(sizeof(InkpodObjectId) == 32U);
static_assert(sizeof(InkpodPrimitiveRequestV3) == 120U);
static_assert(sizeof(InkpodPrimitiveResultV3) == 48U);
static_assert(sizeof(InkpodRasterAssetInputV3) == 56U);
static_assert(sizeof(InkpodObjectInfoV3) == 72U);
static_assert(sizeof(InkpodSnapshotInfoV3) == 80U);
static_assert(sizeof(InkpodSnapshotTileInfoV3) == 56U);
static_assert(sizeof(InkpodBufferCopyV3) == 56U);
static_assert(sizeof(InkpodColorArray) == 40U);
static_assert(sizeof(InkpodColorBuffer) == 48U);
static_assert(std::is_standard_layout_v<InkpodEditorStateInfo>);
static_assert(sizeof(InkpodEditorFillOptions) == 136U);
static_assert(sizeof(InkpodEditorSelectionOptions) == 56U);
static_assert(sizeof(InkpodEditorBrushOptions) == 20U);
static_assert(sizeof(InkpodEditorStateInfo) == 336U);
static_assert(sizeof(InkpodEditorDefaults) == 368U);
static_assert(sizeof(InkpodEditorStateUpdate) == 296U);
static_assert(sizeof(InkpodEditorStrokeInput) == 48U);
static_assert(sizeof(InkpodFillInput) == 96U);
static_assert(sizeof(InkpodFillResult) == 32U);
static_assert(sizeof(InkpodTreeEdit) == 64U);
static_assert(sizeof(InkpodNodeInfo) == 72U);
static_assert(sizeof(InkpodSelectionPoint) == 24U);
static_assert(sizeof(InkpodSelectionInput) == 104U);
static_assert(sizeof(InkpodOutputColorGuardRequest) == 32U);
static_assert(sizeof(InkpodOutputColorGuardResult) == 56U);
static_assert(sizeof(InkpodScopedColorReplaceInput) == 120U);
static_assert(sizeof(InkpodScopedColorReplacePreview) == 40U);
static_assert(sizeof(InkpodFloatingTransform) == 48U);
static_assert(sizeof(InkpodGridInput) == 32U);
static_assert(sizeof(InkpodLocatorOutput) == 48U);
static_assert(sizeof(InkpodLocatorNeighborhoodBuffer) == 56U);
static_assert(sizeof(InkpodRasterSourceInput) == 96U);
static_assert(sizeof(InkpodLightTableItemInput) == 168U);
static_assert(sizeof(InkpodSequenceCellInput) == 120U);
static_assert(sizeof(InkpodSequenceInput) == 40U);
static_assert(sizeof(InkpodNamedRasterInput) == 48U);
static_assert(sizeof(InkpodSequenceThumbnailBuffer) == 56U);
static_assert(sizeof(InkpodSequenceSwitchRequest) == 88U);
static_assert(sizeof(InkpodSequenceStepPlan) == 96U);
static_assert(sizeof(InkpodMotionCheckInput) == 16U);
static_assert(sizeof(InkpodMotionFrame) == 40U);
static_assert(sizeof(InkpodGeometryPoint) == 16U);
static_assert(std::is_standard_layout_v<InkpodGeometryPointResolveInput>);
static_assert(std::is_standard_layout_v<InkpodGeometryPointResolveResult>);
static_assert(sizeof(InkpodGeometryPointResolveInput) == 56U);
static_assert(sizeof(InkpodGeometryPointResolveResult) == 24U);
static_assert(sizeof(InkpodGeometryInput) == 104U);
static_assert(sizeof(InkpodGeometryPreviewInfo) == 32U);
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
static_assert(sizeof(InkpodBatchTargetInput) == 48U);
static_assert(sizeof(InkpodBatchOperationInput) == 160U);
static_assert(sizeof(InkpodBatchGraphInput) == 144U);
static_assert(sizeof(InkpodBatchGraphInfo) == 104U);
static_assert(sizeof(InkpodBatchPreviewItem) == 56U);
static_assert(sizeof(InkpodBatchReportInfo) == 32U);
static_assert(sizeof(InkpodBatchReportItem) == 56U);
static_assert(sizeof(InkpodSequenceSourceIdentity) == 32U);
static_assert(sizeof(InkpodBatchPairPreviewInfo) == 40U);
static_assert(sizeof(InkpodBatchPairCandidate) == 64U);
static_assert(sizeof(InkpodLayerThumbnailBuffer) == 80U);
static_assert(std::is_standard_layout_v<InkpodInkScriptSourceInput>);
static_assert(std::is_standard_layout_v<InkpodInkScriptDiagnostic>);
static_assert(std::is_standard_layout_v<InkpodInkScriptCompileRequest>);
static_assert(std::is_standard_layout_v<InkpodInkScriptExportRequest>);
static_assert(sizeof(InkpodInkScriptSourceInput) == 56U);
static_assert(sizeof(InkpodInkScriptSourceSummary) == 72U);
static_assert(sizeof(InkpodInkScriptUtf8Buffer) == 48U);
static_assert(sizeof(InkpodInkScriptDiagnostic) == 128U);
static_assert(sizeof(InkpodInkScriptDiagnosticBuffer) == 96U);
static_assert(sizeof(InkpodInkScriptParameterChoice) == 56U);
static_assert(sizeof(InkpodInkScriptCompileRequest) == 80U);
static_assert(sizeof(InkpodInkScriptProgramSummary) == 152U);
static_assert(sizeof(InkpodInkScriptJournalEvent) == 24U);
static_assert(sizeof(InkpodInkScriptExportRequest) == 96U);
static_assert(sizeof(InkpodInkScriptFragmentSummary) == 88U);
static_assert(sizeof(InkpodInkScriptUtf8Span) == 16U);
static_assert(sizeof(InkpodInkScriptPathIdentity) == 200U);
static_assert(sizeof(InkpodInkScriptNativeFingerprint) == 136U);
static_assert(sizeof(InkpodInkScriptSessionInput) == 80U);
static_assert(sizeof(InkpodInkScriptSequenceMember) == 48U);
static_assert(sizeof(InkpodInkScriptOpenSession) == 56U);
static_assert(sizeof(InkpodInkScriptAuthorityGrant) == 80U);
static_assert(sizeof(InkpodInkScriptTemporaryIdentity) == 96U);
static_assert(sizeof(InkpodInkScriptHostRequest) == 280U);
static_assert(sizeof(InkpodInkScriptHostResponse) == 280U);
static_assert(sizeof(InkpodInkScriptHostAdapter) == 32U);
static_assert(sizeof(InkpodInkScriptPlanTaskRequest) == 120U);
static_assert(sizeof(InkpodInkScriptPathIntent) == 72U);
static_assert(sizeof(InkpodInkScriptPathIntentBuffer) == 88U);
static_assert(sizeof(InkpodInkScriptPlanSummary) == 80U);
static_assert(sizeof(InkpodInkScriptPreviewItem) == 72U);
static_assert(sizeof(InkpodInkScriptPreviewBuffer) == 96U);
static_assert(sizeof(InkpodInkScriptConfirmationRequest) == 72U);
static_assert(sizeof(InkpodInkScriptRunRequest) == 80U);
static_assert(sizeof(InkpodInkScriptTaskEvent) == 64U);
static_assert(sizeof(InkpodInkScriptReportSummary) == 40U);
static_assert(sizeof(InkpodInkScriptReportItem) == 120U);
static_assert(sizeof(InkpodInkScriptReportBuffer) == 96U);
static_assert(sizeof(InkpodSubpaletteSourceInput) == 32U);
static_assert(sizeof(InkpodSubpaletteRasterInput) == 32U);
static_assert(sizeof(InkpodSubpaletteInfo) == 32U);
static_assert(sizeof(InkpodSubpaletteItemInfo) == 40U);
static_assert(std::is_standard_layout_v<InkpodIoRequest>);
static_assert(std::is_standard_layout_v<InkpodIoJobInfo>);
static_assert(std::is_standard_layout_v<InkpodIoRecoveryMetadata>);
static_assert(sizeof(InkpodIoConfig) == 48U);
static_assert(sizeof(InkpodIoPath) == 24U);
static_assert(sizeof(InkpodIoRequest) == 72U);
static_assert(sizeof(InkpodIoJobInfo) == 104U);
static_assert(sizeof(InkpodIoFileIdentity) == 32U);
static_assert(sizeof(InkpodIoItemInfo) == 80U);
static_assert(sizeof(InkpodIoCacheInfo) == 72U);
static_assert(sizeof(InkpodIoRecoveryMetadata) == 160U);

extern "C" int inkpod_header_c11_smoke(void);

int InkpodRunAbiSmoke() {
    if (inkpod_header_c11_smoke() != 0) {
        return 31;
    }
    if (inkpod_abi_version() != INKPOD_ABI_VERSION) {
        return 1;
    }
    InkpodSubpalette* subpalette{};
    constexpr std::array<std::uint8_t, 10U> subpalette_name_10{
        'c', 'e', 'l', 'l', '1', '0', '.', 'p', 'n', 'g'};
    constexpr std::array<std::uint8_t, 9U> subpalette_name_2{
        'c', 'e', 'l', 'l', '2', '.', 'p', 'n', 'g'};
    const std::array<InkpodSubpaletteSourceInput, 2U> subpalette_sources{
        InkpodSubpaletteSourceInput{
            sizeof(InkpodSubpaletteSourceInput),
            0U,
            10U,
            subpalette_name_10.data(),
            subpalette_name_10.size()},
        InkpodSubpaletteSourceInput{
            sizeof(InkpodSubpaletteSourceInput),
            0U,
            2U,
            subpalette_name_2.data(),
            subpalette_name_2.size()}};
    InkpodSubpaletteInfo subpalette_info{};
    subpalette_info.struct_size = sizeof(subpalette_info);
    InkpodSubpaletteItemInfo subpalette_item{};
    subpalette_item.struct_size = sizeof(subpalette_item);
    if (inkpod_subpalette_create(&subpalette) != INKPOD_STATUS_OK
        || subpalette == nullptr
        || inkpod_subpalette_replace_sources(
               subpalette,
               subpalette_sources.data(),
               subpalette_sources.size(),
               sizeof(InkpodSubpaletteSourceInput),
               &subpalette_info)
            != INKPOD_STATUS_OK
        || subpalette_info.item_count != 2U
        || subpalette_info.active_index != INKPOD_SUBPALETTE_INDEX_NONE
        || inkpod_subpalette_item_get(subpalette, 0U, &subpalette_item)
            != INKPOD_STATUS_OK
        || subpalette_item.source_token != 2U
        || subpalette_item.cell_number != 2U) {
        return 163;
    }
    std::uint64_t subpalette_name_bytes{};
    if (inkpod_subpalette_item_name_copy(
            subpalette, 0U, nullptr, 0U, &subpalette_name_bytes)
            != INKPOD_STATUS_BUFFER_TOO_SMALL
        || subpalette_name_bytes != subpalette_name_2.size()) {
        return 164;
    }
    InkpodStatus subpalette_wrong_thread_status = INKPOD_STATUS_OK;
    std::thread subpalette_wrong_thread([
        subpalette,
        &subpalette_wrong_thread_status]() {
        InkpodSubpaletteInfo info{};
        info.struct_size = sizeof(info);
        subpalette_wrong_thread_status =
            inkpod_subpalette_get_info(subpalette, &info);
    });
    subpalette_wrong_thread.join();
    if (subpalette_wrong_thread_status != INKPOD_STATUS_WRONG_THREAD
        || inkpod_subpalette_release(&subpalette) != INKPOD_STATUS_OK
        || subpalette != nullptr
        || inkpod_subpalette_release(&subpalette) != INKPOD_STATUS_OK) {
        return 165;
    }
    if (inkpod::app::RunPrivateInkScriptEngineSmoke() != 0) {
        return 162;
    }
    InkpodCoreConfig old_config{
        sizeof(InkpodCoreConfig), 16U, INKPOD_FEATURE_NONE};
    InkpodCore* old_core = nullptr;
    if (inkpod_core_create(&old_config, &old_core)
            != INKPOD_STATUS_INCOMPATIBLE_ABI
        || old_core != nullptr) {
        return 138;
    }

    InkpodCoreConfig config{
        sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
    InkpodCore* core = nullptr;
    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK
        || core == nullptr) {
        return 2;
    }
    constexpr char script_text[] = R"(inkscript 2;
requires { procedure_catalog = 5; replay_epoch = 27; }
inputs { current_document; }
program {
    step "Set grid" {
        enabled = true;
        invoke set_grid {
            grid = { origin_x = 1; origin_y = 2; spacing_x = 8; spacing_y = 9; subdivisions = 2; };
        };
    }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "abi"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
)";
    InkpodInkScriptSourceInput script_input{};
    script_input.struct_size = sizeof(script_input);
    script_input.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    script_input.controller_id = 501U;
    script_input.session_generation = 9U;
    script_input.source_id = 77U;
    script_input.source_utf8 = reinterpret_cast<const std::uint8_t*>(script_text);
    script_input.source_bytes = sizeof(script_text) - 1U;
    InkpodInkScriptSource* script_source = nullptr;
    if (inkpod_inkscript_source_parse(&script_input, &script_source)
            != INKPOD_STATUS_OK
        || script_source == nullptr) {
        return 139;
    }
    InkpodInkScriptSourceSummary script_source_summary{};
    script_source_summary.struct_size = sizeof(script_source_summary);
    if (inkpod_inkscript_source_summary(script_source, &script_source_summary)
            != INKPOD_STATUS_OK
        || script_source_summary.version != INKPOD_INKSCRIPT_RECORD_VERSION
        || (script_source_summary.flags & INKPOD_INKSCRIPT_SOURCE_VALID) == 0U
        || script_source_summary.diagnostic_count != 0U) {
        return 140;
    }
    InkpodInkScriptCompileRequest script_compile{};
    script_compile.struct_size = sizeof(script_compile);
    script_compile.version = INKPOD_INKSCRIPT_RECORD_VERSION;
    script_compile.controller_id = script_input.controller_id;
    script_compile.session_generation = script_input.session_generation;
    InkpodInkScriptProgram* script_program = nullptr;
    if (inkpod_core_inkscript_compile(
            core, script_source, &script_compile, &script_program)
            != INKPOD_STATUS_OK
        || script_program == nullptr) {
        return 141;
    }
    InkpodInkScriptProgramSummary script_program_summary{};
    script_program_summary.struct_size = sizeof(script_program_summary);
    if (inkpod_core_inkscript_program_summary(
            core, script_program, &script_program_summary)
            != INKPOD_STATUS_OK
        || script_program_summary.version != INKPOD_INKSCRIPT_RECORD_VERSION
        || script_program_summary.core_generation == 0U
        || script_program_summary.max_invocations != 1U) {
        return 142;
    }
    if (inkpod_inkscript_source_release(&script_source) != INKPOD_STATUS_OK
        || script_source != nullptr
        || inkpod_inkscript_source_release(&script_source) != INKPOD_STATUS_OK
        || inkpod_core_inkscript_program_release(core, &script_program)
            != INKPOD_STATUS_OK
        || script_program != nullptr
        || inkpod_core_inkscript_program_release(core, &script_program)
            != INKPOD_STATUS_OK) {
        return 143;
    }
    InkpodObjectId core_id{};
    core_id.struct_size = sizeof(core_id);
    InkpodObjectId short_core_id{};
    short_core_id.struct_size = sizeof(short_core_id) - 1U;
    if (inkpod_core_get_id_v3(core, &core_id) != INKPOD_STATUS_OK
        || core_id.object_type != INKPOD_OBJECT_CORE
        || core_id.generation == 0U || core_id.value == 0U
        || inkpod_core_get_id_v3(core, &short_core_id)
            != INKPOD_STATUS_INCOMPATIBLE_ABI) {
        return 32;
    }
    InkpodReplayContract replay_contract{};
    replay_contract.struct_size = sizeof(replay_contract);
    if (inkpod_core_get_replay_contract(core, &replay_contract) != INKPOD_STATUS_OK
        || replay_contract.replay_epoch != 27U
        || replay_contract.procedure_format_version != 31U
        || replay_contract.canonical_numeric_version != 1U
        || replay_contract.primitive_count == 0U
        || replay_contract.feature_flags != INKPOD_FEATURE_NONE) {
        return 96;
    }

    InkpodDispatchResult dispatch{};
    dispatch.struct_size = sizeof(dispatch);

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
    InkpodCanonicalDigest canonical_digest{};
    canonical_digest.struct_size = sizeof(canonical_digest);
    if (inkpod_snapshot_get_canonical_digest(snapshot, &canonical_digest)
            != INKPOD_STATUS_OK
        || canonical_digest.algorithm != INKPOD_DIGEST_BLAKE3_256
        || std::all_of(
            std::begin(canonical_digest.bytes),
            std::end(canonical_digest.bytes),
            [](std::uint8_t value) { return value == 0U; })) {
        return 97;
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
    InkpodStatus wrong_thread_resource_status = INKPOD_STATUS_OK;
    InkpodResourceUsage wrong_thread_usage{};
    wrong_thread_usage.struct_size = sizeof(wrong_thread_usage);
    wrong_thread_usage.feature_flags = UINT64_MAX;
    std::thread wrong_thread(
        [core, &wrong_thread_status, &wrong_thread_resource_status, &wrong_thread_usage]() {
        InkpodPersistenceInfo persistence{};
        persistence.struct_size = sizeof(persistence);
        wrong_thread_status = inkpod_core_get_persistence_info(core, &persistence);
        wrong_thread_resource_status = inkpod_core_get_resource_usage(
            core, &wrong_thread_usage);
    });
    wrong_thread.join();
    if (wrong_thread_status != INKPOD_STATUS_WRONG_THREAD
        || wrong_thread_resource_status != INKPOD_STATUS_WRONG_THREAD
        || wrong_thread_usage.feature_flags != UINT64_MAX) {
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
    InkpodEditorDefaults editor_defaults{};
    editor_defaults.struct_size = sizeof(editor_defaults);
    if (inkpod_core_get_editor_defaults(core, &editor_defaults) != INKPOD_STATUS_OK
        || editor_defaults.width != 1920U || editor_defaults.height != 1080U
        || editor_defaults.state.active_tool != INKPOD_EDITOR_TOOL_PENCIL
        || (editor_defaults.state.flags & INKPOD_EDITOR_STATE_HAS_TARGET) != 0U) {
        return 130;
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
    InkpodPersistenceInfo persistence{};
    persistence.struct_size = sizeof(persistence);
    InkpodCompactionPlan compaction{};
    compaction.struct_size = sizeof(compaction);
    if (inkpod_core_get_persistence_info(core, &persistence) != INKPOD_STATUS_OK
        || persistence.format_version != 31U
        || persistence.open_strategy != INKPOD_NATIVE_OPEN_NOT_OPENED
        || persistence.flags != 0U
        || persistence.feature_flags != INKPOD_FEATURE_NONE
        || persistence.journal_event_count != 0U
        || inkpod_core_compaction_plan(core, &compaction) != INKPOD_STATUS_OK
        || compaction.history_event_count != 0U
        || compaction.history_procedure_count != 0U
        || compaction.feature_flags != INKPOD_FEATURE_NONE
        || inkpod_core_write_compacted_copy(core, nullptr, 0U, &compaction)
            != INKPOD_STATUS_INVALID_ARGUMENT) {
        return 131;
    }
    InkpodCore* v3_core = nullptr;
    InkpodDocumentInfo v3_document{};
    v3_document.struct_size = sizeof(v3_document);
    if (inkpod_core_create(&config, &v3_core) != INKPOD_STATUS_OK
        || inkpod_core_new_cell(v3_core, &cell_options, &v3_document)
            != INKPOD_STATUS_OK) {
        return 33;
    }
    InkpodPrimitiveRequestV3 primitive{};
    primitive.struct_size = sizeof(primitive);
    primitive.opcode = INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR;
    primitive.schema_version = 1U;
    primitive.base_revision = v3_document.document_revision;
    primitive.payload_id.struct_size = sizeof(primitive.payload_id);
    primitive.color = InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        18U,
        52U,
        86U,
        255U};
    InkpodPrimitiveResultV3 primitive_result{};
    primitive_result.struct_size = sizeof(primitive_result);
    if (inkpod_core_primitive_execute_v3(v3_core, &primitive, &primitive_result)
            != INKPOD_STATUS_OK
        || (primitive_result.flags & INKPOD_PRIMITIVE_RESULT_COMMITTED) == 0U
        || primitive_result.revision != v3_document.document_revision + 1U) {
        return 33;
    }
    InkpodPrimitiveRequestV3 unknown_primitive = primitive;
    unknown_primitive.opcode = UINT32_MAX;
    unknown_primitive.base_revision = primitive_result.revision;
    if (inkpod_core_primitive_execute_v3(
            v3_core, &unknown_primitive, &primitive_result)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_destroy(&v3_core) != INKPOD_STATUS_OK) {
        return 34;
    }
    InkpodResourceUsage usage{};
    usage.struct_size = sizeof(usage);
    InkpodResourceUsage short_usage{};
    short_usage.struct_size = sizeof(short_usage) - 1U;
    if (inkpod_core_get_resource_usage(core, nullptr) != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_get_resource_usage(core, &short_usage)
            != INKPOD_STATUS_INCOMPATIBLE_ABI
        || inkpod_core_get_resource_usage(core, &usage) != INKPOD_STATUS_OK
        || usage.feature_flags != INKPOD_FEATURE_NONE
        || usage.document_tile_bytes != 0U || usage.document_tile_count != 0U
        || usage.history_entry_count != 0U || usage.thumbnail_cache_bytes != 0U) {
        return 97;
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
    InkpodSequenceSourceIdentity pair_old{};
    pair_old.struct_size = sizeof(pair_old);
    InkpodSequenceSourceIdentity pair_new{};
    pair_new.struct_size = sizeof(pair_new);
    InkpodBatchPairPreview* pair_preview{};
    InkpodBatchPairPreviewInfo pair_info{};
    pair_info.struct_size = sizeof(pair_info);
    InkpodBatchPairCandidate pair_candidate{};
    pair_candidate.struct_size = sizeof(pair_candidate);
    if (inkpod_core_sequence_source_identity(core, 0U, &pair_old) != INKPOD_STATUS_OK
        || inkpod_core_sequence_source_identity(core, 1U, &pair_new) != INKPOD_STATUS_OK
        || inkpod_core_batch_extract_color_pairs(
               core, &pair_old, &pair_new, &pair_preview) != INKPOD_STATUS_OK
        || pair_preview == nullptr) {
        return 155;
    }
    if (inkpod_batch_pair_preview_get_info(pair_preview, &pair_info)
            != INKPOD_STATUS_OK
        || pair_info.pixel_format != INKPOD_STORAGE_RGBA8
        || pair_info.width != 1U || pair_info.height != 1U
        || pair_info.candidate_count != 1U || pair_info.ambiguity_count != 0U
        || pair_info.unchanged_pixel_count != 0U) {
        return 156;
    }
    if (inkpod_batch_pair_preview_get_candidate(
            pair_preview, 0U, &pair_candidate) != INKPOD_STATUS_OK) {
        return 157;
    }
    if (pair_candidate.pixel_count != 1U
        || pair_candidate.bounds_x != 0 || pair_candidate.bounds_y != 0
        || pair_candidate.bounds_width != 1 || pair_candidate.bounds_height != 1
        || pair_candidate.old_color.depth != INKPOD_COLOR_DEPTH_8
        || pair_candidate.new_color.depth != INKPOD_COLOR_DEPTH_8
        || pair_candidate.old_color.red != 4U
        || pair_candidate.new_color.red != 1U) {
        std::fprintf(
            stderr,
            "pair candidate: count=%llu bounds=%d,%d,%d,%d depth=%u/%u red=%u/%u\n",
            static_cast<unsigned long long>(pair_candidate.pixel_count),
            pair_candidate.bounds_x,
            pair_candidate.bounds_y,
            pair_candidate.bounds_width,
            pair_candidate.bounds_height,
            pair_candidate.old_color.depth,
            pair_candidate.new_color.depth,
            pair_candidate.old_color.red,
            pair_candidate.new_color.red);
        return 158;
    }
    if (inkpod_batch_pair_preview_get_candidate(
               pair_preview, 1U, &pair_candidate) != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_batch_pair_preview_release(&pair_preview) != INKPOD_STATUS_OK
        || pair_preview != nullptr
        || inkpod_batch_pair_preview_release(&pair_preview) != INKPOD_STATUS_OK) {
        return 159;
    }
    std::uint64_t reference_view_id{};
    const InkpodViewInput reference_fit{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_FIT,
        0U,
        100.0,
        100.0,
        0.0,
        0.0};
    InkpodColorValue reference_color{};
    reference_color.struct_size = sizeof(reference_color);
    InkpodSnapshot* reference_snapshot{};
    const InkpodSnapshotOptions reference_options{
        sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
    if (inkpod_core_subpalette_set(core, 0U) != INKPOD_STATUS_OK
        || inkpod_core_view_create(core, &reference_view_id) != INKPOD_STATUS_OK
        || reference_view_id == 0U
        || inkpod_core_subpalette_view_apply(
               core, reference_view_id, &reference_fit) != INKPOD_STATUS_OK
        || inkpod_core_subpalette_view_sample(
               core, reference_view_id, 50.0, 50.0, &reference_color)
            != INKPOD_STATUS_OK
        || reference_color.red != 4U || reference_color.green != 5U
        || reference_color.blue != 6U || reference_color.alpha != 255U
        || inkpod_core_subpalette_build_snapshot(
               core,
               reference_view_id,
               &reference_options,
               &reference_snapshot) != INKPOD_STATUS_OK
        || reference_snapshot == nullptr
        || inkpod_snapshot_release(&reference_snapshot) != INKPOD_STATUS_OK
        || inkpod_core_view_close(core, reference_view_id) != INKPOD_STATUS_OK) {
        (void)inkpod_snapshot_release(&reference_snapshot);
        return 96;
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
    InkpodDocumentInfo after_plane_validation{};
    after_plane_validation.struct_size = sizeof(after_plane_validation);
    InkpodDocumentInfo after_invalid_tree{};
    after_invalid_tree.struct_size = sizeof(after_invalid_tree);
    constexpr std::array<std::uint8_t, 17> invalid_plane_name{
        'I', 'n', 'v', 'a', 'l', 'i', 'd', ' ', 's', 'e', 'l', 'e', 'c', 't', 'i', 'o', 'n'};
    InkpodTreeEdit invalid_plane{};
    invalid_plane.struct_size = sizeof(invalid_plane);
    invalid_plane.operation = INKPOD_TREE_CREATE_PLANE;
    invalid_plane.parent_id = document.layer_id;
    invalid_plane.pixel_format = UINT32_MAX;
    invalid_plane.name_utf8 = invalid_plane_name.data();
    invalid_plane.name_bytes = invalid_plane_name.size();
    if (inkpod_core_get_document_info(core, &before_invalid_tree) != INKPOD_STATUS_OK
        || inkpod_core_validate_plane_creation(
               nullptr,
               document.layer_id,
               INKPOD_STORAGE_RGBA8)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_validate_plane_creation(
               core,
               document.layer_id,
               INKPOD_STORAGE_RGBA8)
            != INKPOD_STATUS_OK
        || inkpod_core_validate_plane_creation(
               core,
               document.layer_id,
               UINT32_MAX)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_get_document_info(core, &after_plane_validation)
            != INKPOD_STATUS_OK
        || after_plane_validation.document_revision
            != before_invalid_tree.document_revision) {
        return 94;
    }
    if (inkpod_core_tree_edit(core, &invalid_plane, &dispatch, &tree_object_id)
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
    snapshot = nullptr;
    if (inkpod_core_build_snapshot(core, &options, &snapshot) != INKPOD_STATUS_OK
        || snapshot == nullptr) {
        return 54;
    }
    InkpodSnapshotRenderPlan short_render_plan{};
    short_render_plan.struct_size = sizeof(std::uint32_t);
    InkpodSnapshotRenderPlan render_plan{};
    render_plan.struct_size = sizeof(render_plan);
    if (inkpod_snapshot_get_render_plan(snapshot, &short_render_plan)
            != INKPOD_STATUS_INCOMPATIBLE_ABI
        || inkpod_snapshot_get_render_plan(nullptr, &render_plan)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_snapshot_get_render_plan(snapshot, &render_plan) != INKPOD_STATUS_OK
        || render_plan.abi_version != INKPOD_ABI_VERSION
        || render_plan.pass_stride_bytes != sizeof(InkpodSnapshotRenderPass)
        || render_plan.pass_count == 0U || render_plan.passes == nullptr) {
        return 107;
    }
    bool has_raster_pass{};
    for (std::uint64_t index = 0; index < render_plan.pass_count; ++index) {
        const auto& pass = render_plan.passes[index];
        if (pass.struct_size != sizeof(InkpodSnapshotRenderPass)
            || pass.opacity_milli > 1000U || pass.reserved != 0U) {
            return 108;
        }
        has_raster_pass = has_raster_pass
            || pass.kind == INKPOD_RENDER_PASS_RASTER_TILES;
    }
    if (!has_raster_pass) {
        return 109;
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
        sizeof(InkpodStrokeSample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
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
               &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_select_color_for_editor_target(
               core,
               document.layer_id,
               document.color_plane_id,
               &selected_color,
               0U,
               0U,
               INKPOD_SELECTION_NEW,
               &dispatch) != INKPOD_STATUS_OK) {
        return 41;
    }
    if (inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK) {
        return 160;
    }
    InkpodOutputColorGuardRequest output_guard{};
    output_guard.struct_size = sizeof(output_guard);
    output_guard.profile = INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR;
    output_guard.operation = INKPOD_SELECTION_NEW;
    output_guard.base_document_revision = document.document_revision;
    InkpodOutputColorGuardResult output_guard_result{};
    output_guard_result.struct_size = sizeof(output_guard_result);
    InkpodTask* output_guard_task{};
    InkpodOutputColorGuardRequest invalid_output_guard = output_guard;
    invalid_output_guard.profile = UINT32_MAX;
    if (inkpod_task_create(&output_guard_task) != INKPOD_STATUS_OK
        || inkpod_core_select_output_color_guard(
               core,
               &invalid_output_guard,
               output_guard_task,
               &output_guard_result) != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_select_output_color_guard(
               core,
               &output_guard,
               output_guard_task,
               &output_guard_result) != INKPOD_STATUS_OK
        || output_guard_result.selected_pixel_count > output_guard_result.scanned_pixel_count
        || inkpod_task_release(&output_guard_task) != INKPOD_STATUS_OK) {
        return 161;
    }
    InkpodSelectionInput selection{};
    selection.struct_size = sizeof(selection);
    selection.shape = INKPOD_SELECTION_RECTANGLE;
    selection.operation = INKPOD_SELECTION_NEW;
    selection.bounds = InkpodFrameRect{20, 30, 64, 1};
    selection.interpretation = INKPOD_RANGE_NORMAL;
    selection.trace_shape = INKPOD_TRACE_ROUND;
    selection.view_zoom_q16 = INT64_C(1) << 16;
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
    if (inkpod_core_get_document_info(core, &document) != INKPOD_STATUS_OK) {
        return 153;
    }
    InkpodScopedColorReplaceInput scoped_replace{};
    scoped_replace.struct_size = sizeof(scoped_replace);
    scoped_replace.mode = INKPOD_COLOR_REPLACE_RASTER_COLOR;
    scoped_replace.feature_flags = INKPOD_COLOR_REPLACE_HAS_REGION;
    scoped_replace.plane_id = document.color_plane_id;
    scoped_replace.base_document_revision = document.document_revision;
    scoped_replace.target_color = selected_color;
    scoped_replace.replacement_color = selected_color;
    scoped_replace.shape = INKPOD_SELECTION_RECTANGLE;
    scoped_replace.bounds = InkpodFrameRect{20, 30, 64, 1};
    InkpodScopedColorReplacePreview scoped_preview{};
    scoped_preview.struct_size = sizeof(scoped_preview);
    InkpodScopedColorReplaceInput invalid_scoped_replace = scoped_replace;
    invalid_scoped_replace.mode = UINT32_MAX;
    if (inkpod_core_preview_scoped_color_replace(
            core, &invalid_scoped_replace, &scoped_preview) != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_preview_scoped_color_replace(
               core, &scoped_replace, &scoped_preview) != INKPOD_STATUS_OK
        || inkpod_core_apply_scoped_color_replace(
               core, &scoped_replace, &dispatch) != INKPOD_STATUS_OK) {
        return 154;
    }

    const std::array<std::uint8_t, 4> expected_external_pixels{17U, 34U, 51U, 255U};
    std::array<std::uint8_t, 4> external_pixels = expected_external_pixels;
    const InkpodClipboardRgbaInput external_clipboard_input{
        sizeof(InkpodClipboardRgbaInput),
        0U,
        20,
        30,
        1U,
        1U,
        external_pixels.data(),
        external_pixels.size(),
        4U};
    InkpodClipboard* external_clipboard{};
    if (inkpod_clipboard_create_rgba8(
            &external_clipboard_input, &external_clipboard) != INKPOD_STATUS_OK
        || external_clipboard == nullptr) {
        inkpod_clipboard_release(&external_clipboard);
        return 98;
    }
    external_pixels.fill(0U);
    std::array<std::uint8_t, 4> rendered_external_pixels{};
    InkpodClipboardRasterBuffer rendered_external{
        sizeof(InkpodClipboardRasterBuffer),
        0U,
        0,
        0,
        0U,
        0U,
        rendered_external_pixels.data(),
        rendered_external_pixels.size(),
        0U,
        0U};
    if (inkpod_clipboard_render_rgba8(external_clipboard, &rendered_external)
            != INKPOD_STATUS_OK
        || rendered_external.origin_x != 20 || rendered_external.origin_y != 30
        || rendered_external.width != 1U || rendered_external.height != 1U
        || rendered_external.required_bytes != rendered_external_pixels.size()
        || rendered_external.row_stride_bytes != 4U
        || rendered_external_pixels != expected_external_pixels
        || inkpod_core_paste_begin_mode(
               core, external_clipboard, INKPOD_PASTE_ACTIVE_CONVERTED)
            != INKPOD_STATUS_OK
        || inkpod_clipboard_release(&external_clipboard) != INKPOD_STATUS_OK
        || inkpod_clipboard_release(&external_clipboard) != INKPOD_STATUS_OK
        || external_clipboard != nullptr
        || inkpod_core_floating_commit(core, &dispatch) != INKPOD_STATUS_OK) {
        inkpod_clipboard_release(&external_clipboard);
        return 99;
    }
    InkpodColorValue pasted_external_color{};
    pasted_external_color.struct_size = sizeof(pasted_external_color);
    if (inkpod_core_eyedropper(
            core,
            INKPOD_EYEDROPPER_SELECTED_PLANE,
            20U,
            30U,
            &pasted_external_color) != INKPOD_STATUS_OK
        || pasted_external_color.depth != INKPOD_COLOR_DEPTH_8
        || pasted_external_color.red != 17U || pasted_external_color.green != 34U
        || pasted_external_color.blue != 51U || pasted_external_color.alpha != 255U) {
        return 99;
    }
    if (inkpod_core_undo(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_redo(core, &dispatch) != INKPOD_STATUS_OK
        || inkpod_core_eyedropper(
               core,
               INKPOD_EYEDROPPER_SELECTED_PLANE,
               20U,
               30U,
               &pasted_external_color) != INKPOD_STATUS_OK
        || pasted_external_color.red != 17U || pasted_external_color.green != 34U
        || pasted_external_color.blue != 51U || pasted_external_color.alpha != 255U) {
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
    InkpodLocatorNeighborhoodBuffer locator_neighborhood{};
    locator_neighborhood.struct_size = sizeof(locator_neighborhood);
    locator_neighborhood.radius = 1U;
    if (inkpod_core_locator_neighborhood(
            core, 0U, 1.0, 1.0, &locator_neighborhood) != INKPOD_STATUS_OK
        || locator_neighborhood.width != 3U
        || locator_neighborhood.height != 3U
        || locator_neighborhood.required_bytes != 36U) {
        return 89;
    }
    std::array<std::uint8_t, 35U> short_neighborhood{};
    locator_neighborhood.pixels_rgba8 = short_neighborhood.data();
    locator_neighborhood.pixel_capacity = short_neighborhood.size();
    if (inkpod_core_locator_neighborhood(
            core, 0U, 1.0, 1.0, &locator_neighborhood)
            != INKPOD_STATUS_BUFFER_TOO_SMALL) {
        return 89;
    }
    std::array<std::uint8_t, 36U> neighborhood{};
    locator_neighborhood.pixels_rgba8 = neighborhood.data();
    locator_neighborhood.pixel_capacity = neighborhood.size();
    if (inkpod_core_locator_neighborhood(
            core, 0U, 1.0, 1.0, &locator_neighborhood) != INKPOD_STATUS_OK
        || locator_neighborhood.required_bytes != neighborhood.size()) {
        return 89;
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
    const std::string batch_naming_template{"{stem}_{index:4}"};
    InkpodBatchInput batch_input{};
    batch_input.struct_size = sizeof(batch_input);
    batch_input.kind = INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT;
    InkpodBatchColorPairInput batch_pair{};
    batch_pair.struct_size = sizeof(batch_pair);
    batch_pair.enabled = 1U;
    batch_pair.old_color = InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 1U, 2U, 3U, 4U};
    batch_pair.new_color = InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 255U, 0U, 0U, 255U};
    InkpodBatchOperationInput batch_operation{};
    batch_operation.struct_size = sizeof(batch_operation);
    batch_operation.version = INKPOD_BATCH_OPERATION_VERSION;
    batch_operation.kind = INKPOD_BATCH_OPERATION_COLOR_REPLACE;
    batch_operation.flags = INKPOD_BATCH_OPERATION_ENABLED;
    batch_operation.plane_kind = INKPOD_TYPED_PLANE_COLOR;
    batch_operation.missing_policy = INKPOD_BATCH_MISSING_ERROR;
    batch_operation.colors.struct_size = sizeof(batch_operation.colors);
    batch_operation.colors.color_stride_bytes = sizeof(InkpodColorValue);
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
    batch_graph_input.output_destination = INKPOD_BATCH_OUTPUT_NEW_TABS;
    batch_graph_input.failure_policy = INKPOD_BATCH_FAILURE_CONTINUE;
    batch_graph_input.naming_template_utf8 =
        reinterpret_cast<const std::uint8_t*>(batch_naming_template.data());
    batch_graph_input.naming_template_bytes = batch_naming_template.size();
    batch_graph_input.output_format = INKPOD_BATCH_FORMAT_INKPOD;
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
        || batch_graph_info.output_destination != INKPOD_BATCH_OUTPUT_NEW_TABS
        || batch_graph_info.name_bytes != batch_name.size()
        || batch_graph_info.naming_template_bytes
            != batch_naming_template.size()) {
        return 84;
    }
    InkpodBatchInput queried_batch_input{};
    queried_batch_input.struct_size = sizeof(queried_batch_input);
    if (inkpod_batch_graph_get_input(
            batch_graph, 0U, &queried_batch_input)
            != INKPOD_STATUS_OK
        || queried_batch_input.kind != INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT
        || queried_batch_input.path_bytes != 0U) {
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
    const InkpodStatus batch_report_info_status =
        inkpod_batch_report_get_info(batch_report, &batch_report_info);
    const InkpodStatus batch_report_item_status =
        inkpod_batch_report_get(batch_report, 0U, &batch_report_item);
    const InkpodStatus batch_report_release_status =
        inkpod_batch_report_release(&batch_report);
    const InkpodStatus batch_task_release_status =
        inkpod_batch_task_release(&batch_task);
    if (batch_report_info_status != INKPOD_STATUS_OK
        || batch_report_info.item_count == 0U
        || batch_report_info.failure_count != 0U
        || batch_report_item_status != INKPOD_STATUS_OK
        || batch_report_item.outcome != INKPOD_BATCH_ITEM_DRY_RUN
        || batch_report_release_status != INKPOD_STATUS_OK
        || batch_task_release_status != INKPOD_STATUS_OK) {
        std::fprintf(
            stderr,
            "batch dry run mismatch: info=%u items=%llu failures=%llu "
            "item=%u outcome=%u report_release=%u task_release=%u\n",
            batch_report_info_status,
            static_cast<unsigned long long>(batch_report_info.item_count),
            static_cast<unsigned long long>(batch_report_info.failure_count),
            batch_report_item_status,
            batch_report_item.outcome,
            batch_report_release_status,
            batch_task_release_status);
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
    InkpodEditorStateInfo editor_state{};
    editor_state.struct_size = sizeof(editor_state);
    InkpodDocumentInfo before_editor_update{};
    before_editor_update.struct_size = sizeof(before_editor_update);
    if (inkpod_core_get_editor_state(core, &editor_state) != INKPOD_STATUS_OK
        || inkpod_core_get_document_info(core, &before_editor_update) != INKPOD_STATUS_OK
        || (editor_state.flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U) {
        return 131;
    }
    InkpodEditorStateUpdate editor_update{};
    editor_update.struct_size = sizeof(editor_update);
    editor_update.kind = INKPOD_EDITOR_UPDATE_TOOL_COLOR;
    editor_update.expected_editor_revision = editor_state.editor_revision;
    editor_update.tool = INKPOD_EDITOR_TOOL_BRUSH;
    editor_update.color = InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_16, 1U, 257U, 32769U, 65534U};
    InkpodEditorStateInfo changed_editor_state{};
    changed_editor_state.struct_size = sizeof(changed_editor_state);
    if (inkpod_core_update_editor_state(core, &editor_update, &changed_editor_state)
            != INKPOD_STATUS_OK
        || changed_editor_state.editor_revision != editor_state.editor_revision + 1U) {
        return 132;
    }
    editor_update.kind = INKPOD_EDITOR_UPDATE_ACTIVE_TOOL;
    editor_update.expected_editor_revision = changed_editor_state.editor_revision;
    editor_update.tool = INKPOD_EDITOR_TOOL_BRUSH;
    if (inkpod_core_update_editor_state(core, &editor_update, &changed_editor_state)
            != INKPOD_STATUS_OK
        || changed_editor_state.current_color.depth != INKPOD_COLOR_DEPTH_16
        || changed_editor_state.current_color.red != 1U
        || changed_editor_state.current_color.green != 257U
        || changed_editor_state.current_color.blue != 32769U
        || changed_editor_state.current_color.alpha != 65534U) {
        return 133;
    }
    const std::uint64_t no_op_revision = changed_editor_state.editor_revision;
    std::array<std::uint8_t, 32U> no_op_digest{};
    std::memcpy(no_op_digest.data(), changed_editor_state.editor_digest, no_op_digest.size());
    editor_update.expected_editor_revision = no_op_revision;
    if (inkpod_core_update_editor_state(core, &editor_update, &changed_editor_state)
            != INKPOD_STATUS_OK
        || changed_editor_state.editor_revision != no_op_revision
        || std::memcmp(
               changed_editor_state.editor_digest,
               no_op_digest.data(),
               no_op_digest.size())
            != 0) {
        return 134;
    }
    const InkpodStrokeSample editor_sample{
        sizeof(InkpodStrokeSample), 0U, 2.0F, 2.0F, 1.0F, 0U};
    const InkpodEditorStrokeInput editor_stroke{
        sizeof(InkpodEditorStrokeInput),
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        0U,
        0U,
        &editor_sample,
        1U,
        sizeof(InkpodStrokeSample)};
    if (inkpod_core_editor_stroke_begin(core, &editor_stroke) != INKPOD_STATUS_OK
        || inkpod_core_stroke_cancel(core) != INKPOD_STATUS_OK) {
        return 136;
    }
    if (inkpod_core_editor_stroke_begin_for_view(core, 0U, &editor_stroke)
            != INKPOD_STATUS_OK
        || inkpod_core_stroke_cancel(core) != INKPOD_STATUS_OK) {
        return 136;
    }
    if (inkpod_core_apply_fill_for_editor_target(
            nullptr, 0U, 0U, nullptr, nullptr)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || inkpod_core_apply_selection_for_editor_target(
               nullptr, 0U, 0U, nullptr, nullptr)
            != INKPOD_STATUS_INVALID_ARGUMENT) {
        return 137;
    }
    InkpodDocumentInfo after_editor_update{};
    after_editor_update.struct_size = sizeof(after_editor_update);
    if (inkpod_core_get_document_info(core, &after_editor_update) != INKPOD_STATUS_OK
        || after_editor_update.document_revision
            != before_editor_update.document_revision) {
        return 135;
    }
    if (inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK
        || inkpod_snapshot_release(&snapshot) != INKPOD_STATUS_OK
        || inkpod_core_destroy(&core) != INKPOD_STATUS_OK) {
        return 30;
    }
    return 0;
}
