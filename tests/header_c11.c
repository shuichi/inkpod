#include "inkpod/core_ffi.h"

#include <stdint.h>

_Static_assert(INKPOD_ABI_VERSION == 1U, "unexpected ABI version");
_Static_assert(sizeof(InkpodCoreConfig) == 16U, "core config layout drift");
_Static_assert(sizeof(InkpodCommand) == 16U, "command layout drift");
_Static_assert(sizeof(InkpodSnapshotOptions) == 16U, "snapshot options layout drift");
_Static_assert(sizeof(InkpodCommandBatch) == 40U, "command batch layout drift");
_Static_assert(sizeof(InkpodSnapshotView) == 48U, "snapshot view layout drift");
_Static_assert(sizeof(InkpodCellCreateOptions) == 48U, "cell options layout drift");
_Static_assert(sizeof(InkpodDocumentInfo) == 192U, "document info layout drift");
_Static_assert(sizeof(InkpodStrokeSample) == 24U, "stroke sample layout drift");
_Static_assert(sizeof(InkpodStrokeInput) == 56U, "stroke input layout drift");
_Static_assert(sizeof(InkpodStrokeSampleSpan) == 40U, "stroke span layout drift");
_Static_assert(sizeof(InkpodViewInput) == 48U, "view input layout drift");
_Static_assert(sizeof(InkpodSnapshotTransform) == 48U, "snapshot transform layout drift");
_Static_assert(sizeof(InkpodSnapshotGuide) == 24U, "snapshot guide layout drift");
_Static_assert(sizeof(InkpodSnapshotOverlay) == 56U, "snapshot overlay layout drift");

int inkpod_header_c11_smoke(void) {
    InkpodSnapshotView view = {0};
    view.struct_size = (uint32_t)sizeof(view);
    return view.struct_size == 0U;
}
