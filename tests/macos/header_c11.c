#include "InkpodCoreC.h"

#include <stdint.h>

_Static_assert(INKPOD_ABI_VERSION == 15U, "unexpected ABI version");
_Static_assert(sizeof(InkpodCoreConfig) == 16U, "core config layout drift");
_Static_assert(sizeof(InkpodSnapshotOptions) == 16U, "snapshot options layout drift");
_Static_assert(sizeof(InkpodSnapshotView) == 48U, "snapshot view layout drift");
_Static_assert(sizeof(InkpodShortcutStrokeV2) == 16U, "shortcut stroke layout drift");
_Static_assert(sizeof(InkpodShortcutSequenceV2) == 80U, "shortcut sequence layout drift");

int inkpod_macos_header_c11(void) {
    InkpodCoreConfig config = {0};
    config.struct_size = (uint32_t)sizeof(config);
    return config.abi_version == INKPOD_ABI_VERSION;
}
