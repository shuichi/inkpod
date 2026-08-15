#include "InkpodCoreC.h"

#include <cstddef>
#include <type_traits>

static_assert(INKPOD_ABI_VERSION == 15U);
static_assert(std::is_standard_layout_v<InkpodCoreConfig>);
static_assert(std::is_standard_layout_v<InkpodSnapshotOptions>);
static_assert(std::is_standard_layout_v<InkpodSnapshotView>);
static_assert(sizeof(InkpodCoreConfig) == 16U);
static_assert(sizeof(InkpodSnapshotOptions) == 16U);
static_assert(sizeof(InkpodSnapshotView) == 48U);
static_assert(std::is_standard_layout_v<InkpodShortcutStrokeV2>);
static_assert(std::is_standard_layout_v<InkpodShortcutSequenceV2>);
static_assert(sizeof(InkpodShortcutStrokeV2) == 16U);
static_assert(sizeof(InkpodShortcutSequenceV2) == 80U);

int inkpod_macos_header_cxx20() {
    return static_cast<int>(offsetof(InkpodCoreConfig, feature_flags));
}
