#include "inkpod/core_ffi.h"

#include <stdint.h>

_Static_assert(INKPOD_ABI_VERSION == 1U, "unexpected ABI version");
_Static_assert(sizeof(InkpodCoreConfig) == 16U, "core config layout drift");
_Static_assert(sizeof(InkpodCommand) == 16U, "command layout drift");
_Static_assert(sizeof(InkpodSnapshotOptions) == 16U, "snapshot options layout drift");

int main(void) {
    InkpodSnapshotView view = {0};
    view.struct_size = (uint32_t)sizeof(view);
    return view.struct_size == 0U;
}

