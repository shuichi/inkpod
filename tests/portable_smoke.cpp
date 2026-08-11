#include "inkpod/core_ffi.h"

int InkpodRunPortableSmoke() {
    if (inkpod_abi_version() != INKPOD_ABI_VERSION) {
        return 1;
    }

    const InkpodCoreConfig config{
        sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
    InkpodCore* core = nullptr;
    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK
        || core == nullptr) {
        return 2;
    }
    if (inkpod_core_destroy(&core) != INKPOD_STATUS_OK || core != nullptr) {
        return 3;
    }
    return 0;
}
