#pragma once

#include <optional>

#include "inkpod/core_ffi.h"
#include "localization.h"

namespace inkpod::windows::ui {

// Core history categories are language-neutral. Only this Windows
// presentation boundary maps them to localized product string IDs.
[[nodiscard]] constexpr std::optional<UiStringId> HistoryUiStringId(
    InkpodHistoryEntryKind kind) noexcept {
    switch (kind) {
        case INKPOD_HISTORY_ENTRY_RASTER:
            return UiStringId::HistoryRasterEdit;
        case INKPOD_HISTORY_ENTRY_PALETTE:
            return UiStringId::HistoryPaletteEdit;
        case INKPOD_HISTORY_ENTRY_COLOR_CHART:
            return UiStringId::HistoryColorChartEdit;
        case INKPOD_HISTORY_ENTRY_MAIN_LINE_COLOR:
            return UiStringId::HistoryMainLineColorEdit;
        case INKPOD_HISTORY_ENTRY_DOCUMENT:
            return UiStringId::HistoryDocumentEdit;
        default:
            return std::nullopt;
    }
}

}  // namespace inkpod::windows::ui
