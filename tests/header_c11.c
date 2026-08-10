#include "inkpod/core_ffi.h"

#include <stdint.h>

_Static_assert(INKPOD_ABI_VERSION == 9U, "unexpected ABI version");
_Static_assert(sizeof(InkpodCoreConfig) == 16U, "core config layout drift");
_Static_assert(sizeof(InkpodSnapshotOptions) == 16U, "snapshot options layout drift");
_Static_assert(sizeof(InkpodPersistenceInfo) == 72U, "persistence info layout drift");
_Static_assert(sizeof(InkpodCompactionPlan) == 128U, "compaction plan layout drift");
_Static_assert(sizeof(InkpodSnapshotView) == 48U, "snapshot view layout drift");
_Static_assert(sizeof(InkpodSnapshotRenderPass) == 48U, "render pass layout drift");
_Static_assert(sizeof(InkpodSnapshotRenderPlan) == 64U, "render plan layout drift");
_Static_assert(sizeof(InkpodCellCreateOptions) == 48U, "cell options layout drift");
_Static_assert(sizeof(InkpodCellCreationOptions) == 64U, "cell creation options layout drift");
_Static_assert(sizeof(InkpodCellCreationPlanItem) == 144U, "cell creation plan item layout drift");
_Static_assert(sizeof(InkpodDocumentInfo) == 224U, "document info layout drift");
_Static_assert(sizeof(InkpodResourceUsage) == 112U, "resource usage layout drift");
_Static_assert(sizeof(InkpodStrokeSample) == 24U, "stroke sample layout drift");
_Static_assert(sizeof(InkpodStrokeInput) == 72U, "stroke input layout drift");
_Static_assert(sizeof(InkpodEditorBrushOptions) == 20U, "editor brush layout drift");
_Static_assert(sizeof(InkpodStrokeSampleSpan) == 40U, "stroke span layout drift");
_Static_assert(sizeof(InkpodViewInput) == 48U, "view input layout drift");
_Static_assert(sizeof(InkpodSnapshotTransform) == 48U, "snapshot transform layout drift");
_Static_assert(sizeof(InkpodSnapshotGuide) == 24U, "snapshot guide layout drift");
_Static_assert(sizeof(InkpodSnapshotOverlay) == 56U, "snapshot overlay layout drift");
_Static_assert(sizeof(InkpodSnapshotVectorEndpoint) == 32U, "vector endpoint layout drift");
_Static_assert(sizeof(InkpodSnapshotVectorDiagnostics) == 40U, "vector diagnostics layout drift");
_Static_assert(sizeof(InkpodObjectId) == 32U, "object id layout drift");
_Static_assert(sizeof(InkpodPrimitiveRequestV3) == 120U, "primitive request layout drift");
_Static_assert(sizeof(InkpodPrimitiveResultV3) == 48U, "primitive result layout drift");
_Static_assert(sizeof(InkpodRasterAssetInputV3) == 56U, "raster asset layout drift");
_Static_assert(sizeof(InkpodObjectInfoV3) == 72U, "object info layout drift");
_Static_assert(sizeof(InkpodSnapshotInfoV3) == 104U, "snapshot info layout drift");
_Static_assert(sizeof(InkpodSnapshotTileInfoV3) == 56U, "snapshot tile info layout drift");
_Static_assert(sizeof(InkpodBufferCopyV3) == 56U, "buffer copy layout drift");
_Static_assert(sizeof(InkpodRasterSourceInput) == 96U, "raster source layout drift");
_Static_assert(sizeof(InkpodLightTableItemInput) == 168U, "light-table input layout drift");
_Static_assert(sizeof(InkpodSequenceCellInput) == 120U, "sequence cell layout drift");
_Static_assert(sizeof(InkpodSequenceInput) == 40U, "sequence input layout drift");
_Static_assert(sizeof(InkpodSequenceSwitchRequest) == 88U, "sequence switch request layout drift");
_Static_assert(sizeof(InkpodMotionCheckInput) == 16U, "motion input layout drift");
_Static_assert(sizeof(InkpodMotionFrame) == 40U, "motion frame layout drift");
_Static_assert(sizeof(InkpodLayerThumbnailBuffer) == 80U, "layer thumbnail layout drift");
_Static_assert(sizeof(InkpodScopedColorReplaceInput) == 120U, "scoped replace input layout drift");
_Static_assert(sizeof(InkpodScopedColorReplacePreview) == 48U, "scoped replace preview layout drift");
_Static_assert(sizeof(InkpodSequenceSourceIdentity) == 32U, "sequence identity layout drift");
_Static_assert(sizeof(InkpodBatchPairPreviewInfo) == 40U, "pair preview info layout drift");
_Static_assert(sizeof(InkpodBatchPairCandidate) == 64U, "pair candidate layout drift");

int inkpod_header_c11_smoke(void) {
    InkpodSnapshotView view = {0};
    view.struct_size = (uint32_t)sizeof(view);
    return view.struct_size == 0U;
}
