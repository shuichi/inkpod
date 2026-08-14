if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(CATALOG
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/localization_catalog.json")
set(GENERATED_IDS
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/localization_catalog_ids.generated.inc")
set(GENERATED_TABLE
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/localization_catalog.generated.inc")
set(RESOURCE_TEMPLATE
    "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui.template.rc")
set(RESOURCE_JA
    "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui_ja.generated.rc")
set(RESOURCE_EN
    "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui_en.generated.rc")

foreach(REQUIRED_FILE IN ITEMS
        "${CATALOG}" "${GENERATED_IDS}" "${GENERATED_TABLE}"
        "${RESOURCE_TEMPLATE}" "${RESOURCE_JA}" "${RESOURCE_EN}")
    if(NOT EXISTS "${REQUIRED_FILE}")
        message(FATAL_ERROR "localization artifact is missing: ${REQUIRED_FILE}")
    endif()
endforeach()

# Generated artifacts are checked in, but the JSON catalog is their only
# editable source. A stale artifact must fail even when it still compiles.
file(SHA256 "${CATALOG}" CATALOG_SHA256)
foreach(GENERATED_FILE IN ITEMS
        "${GENERATED_IDS}" "${GENERATED_TABLE}" "${RESOURCE_JA}" "${RESOURCE_EN}")
    file(READ "${GENERATED_FILE}" GENERATED_CONTENT)
    string(FIND
        "${GENERATED_CONTENT}"
        "Catalog SHA-256: ${CATALOG_SHA256}"
        HASH_POSITION)
    if(HASH_POSITION LESS 0)
        message(FATAL_ERROR
            "generated localization artifact is stale: ${GENERATED_FILE}")
    endif()
endforeach()

file(READ "${RESOURCE_JA}" RESOURCE_JA_CONTENT)
file(READ "${RESOURCE_EN}" RESOURCE_EN_CONTENT)
foreach(GENERATED_RESOURCE_CONTENT IN ITEMS
        "${RESOURCE_JA_CONTENT}" "${RESOURCE_EN_CONTENT}")
    string(FIND "${GENERATED_RESOURCE_CONTENT}" "@INKPOD_UI_TEXT_" MARKER_POSITION)
    if(NOT MARKER_POSITION LESS 0)
        message(FATAL_ERROR "generated resource contains an unresolved catalog marker")
    endif()
endforeach()
if(NOT RESOURCE_JA_CONTENT MATCHES
        "LANGUAGE[ \t]+LANG_JAPANESE,[ \t]+SUBLANG_JAPANESE_JAPAN")
    message(FATAL_ERROR "Japanese resources do not declare ja-JP explicitly")
endif()
if(NOT RESOURCE_EN_CONTENT MATCHES
        "LANGUAGE[ \t]+LANG_ENGLISH,[ \t]+SUBLANG_ENGLISH_US")
    message(FATAL_ERROR "English resources do not declare en-US explicitly")
endif()

file(READ "${INKPOD_SOURCE_DIR}/apps/windows/app/app.rc" AGGREGATE_RESOURCE)
foreach(REQUIRED_INCLUDE IN ITEMS
        "app_common.rc" "app_ui_ja.generated.rc" "app_ui_en.generated.rc")
    string(FIND "${AGGREGATE_RESOURCE}" "${REQUIRED_INCLUDE}" INCLUDE_POSITION)
    if(INCLUDE_POSITION LESS 0)
        message(FATAL_ERROR "app.rc does not include ${REQUIRED_INCLUDE}")
    endif()
endforeach()

# Japanese presentation text is permitted only in the canonical JSON catalog
# and its generated Japanese table/resource. Product C++ and resource templates
# must reference UiStringId or an @INKPOD_UI_TEXT_* marker instead.
file(GLOB_RECURSE PRODUCT_SOURCE_FILES
    RELATIVE "${INKPOD_SOURCE_DIR}"
    "${INKPOD_SOURCE_DIR}/apps/windows/*.cpp"
    "${INKPOD_SOURCE_DIR}/apps/windows/*.h"
    "${INKPOD_SOURCE_DIR}/apps/windows/*.inc"
    "${INKPOD_SOURCE_DIR}/apps/windows/*.rc")
list(SORT PRODUCT_SOURCE_FILES)
foreach(RELATIVE_PATH IN LISTS PRODUCT_SOURCE_FILES)
    if(RELATIVE_PATH STREQUAL
            "apps/windows/ui/localization_catalog.generated.inc"
        OR RELATIVE_PATH STREQUAL
            "apps/windows/app/app_ui_ja.generated.rc")
        continue()
    endif()
    file(READ "${INKPOD_SOURCE_DIR}/${RELATIVE_PATH}" CONTENT)
    file(READ "${INKPOD_SOURCE_DIR}/${RELATIVE_PATH}" CONTENT_HEX HEX)
    string(REGEX REPLACE
        "([0-9a-f][0-9a-f])" "\\1;" CONTENT_BYTES "${CONTENT_HEX}")
    string(REGEX MATCH
        "(^|;)(e[3-9];[89ab][0-9a-f];[89ab][0-9a-f];|ef;[89ab][0-9a-f];[89ab][0-9a-f];|f0;a[0-9a-f];[89ab][0-9a-f];[89ab][0-9a-f];)"
        RAW_JAPANESE
        "${CONTENT_BYTES}")
    string(REGEX MATCH
        "\\\\(u|x)(3[0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f]|[4-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f][0-9A-Fa-f])"
        ESCAPED_NON_ASCII
        "${CONTENT}")
    string(REGEX MATCH
        "0[xX](e[3-9]|E[3-9]|ef|eF|Ef|EF|f0|F0)"
        ENCODED_UTF8_LEAD_BYTE
        "${CONTENT}")
    if(RAW_JAPANESE OR ESCAPED_NON_ASCII OR ENCODED_UTF8_LEAD_BYTE)
        message(FATAL_ERROR
            "Japanese literal bypasses UiStringId: ${RELATIVE_PATH}")
    endif()
endforeach()

# Product call sites must use the exact-language loaders. ui_resources.cpp is
# the sole adapter allowed to call the underlying Win32 resource APIs.
foreach(RELATIVE_PATH IN LISTS PRODUCT_SOURCE_FILES)
    if(NOT RELATIVE_PATH MATCHES "\\.(cpp|h)$"
        OR RELATIVE_PATH STREQUAL "apps/windows/ui/ui_resources.cpp")
        continue()
    endif()
    file(READ "${INKPOD_SOURCE_DIR}/${RELATIVE_PATH}" CONTENT)
    string(REGEX MATCH
        "(^|[^A-Za-z])(LoadStringW|LoadMenuW|CreateDialogParamW|DialogBoxParamW)[ \t\r\n]*\\("
        FALLBACK_RESOURCE_CALL
        "${CONTENT}")
    if(FALLBACK_RESOURCE_CALL)
        message(FATAL_ERROR
            "fallback resource loading bypasses selected LANGID: ${RELATIVE_PATH}")
    endif()
endforeach()

file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/localization.cpp"
    LOCALIZATION_IMPLEMENTATION)
foreach(FORBIDDEN_LEGACY_TOKEN IN ITEMS
        "LocalizeText" "LocalizeMenuTree" "LocalizeWindowTree"
        "SetWindowsHookEx" "CallNextHookEx")
    string(FIND
        "${LOCALIZATION_IMPLEMENTATION}"
        "${FORBIDDEN_LEGACY_TOKEN}"
        LEGACY_POSITION)
    if(NOT LEGACY_POSITION LESS 0)
        message(FATAL_ERROR
            "legacy partial localization remains: ${FORBIDDEN_LEGACY_TOKEN}")
    endif()
endforeach()

# Product presentation must not branch on the active language or compare
# language-specific history labels. Selection is exclusively UiStringId-based.
foreach(RELATIVE_PATH IN LISTS PRODUCT_SOURCE_FILES)
    if(NOT RELATIVE_PATH MATCHES "\\.(cpp|h)$"
        OR RELATIVE_PATH STREQUAL "apps/windows/ui/localization.cpp"
        OR RELATIVE_PATH STREQUAL "apps/windows/ui/localization.h"
        OR RELATIVE_PATH STREQUAL "apps/windows/app/app_smoke.cpp")
        continue()
    endif()
    file(READ "${INKPOD_SOURCE_DIR}/${RELATIVE_PATH}" CONTENT)
    foreach(FORBIDDEN_LANGUAGE_BRANCH IN ITEMS
            "CurrentUiLanguage()" "LocalizedHistoryLabel"
            "L\"Untitled Cell" "L\"Recovered Cell" "L\" [View ")
        string(FIND
            "${CONTENT}"
            "${FORBIDDEN_LANGUAGE_BRANCH}"
            LANGUAGE_BRANCH_POSITION)
        if(NOT LANGUAGE_BRANCH_POSITION LESS 0)
            message(FATAL_ERROR
                "language-specific product presentation remains in ${RELATIVE_PATH}: ${FORBIDDEN_LANGUAGE_BRANCH}")
        endif()
    endforeach()
endforeach()
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window.cpp"
    MAIN_WINDOW_SOURCE)
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp"
    MAIN_WINDOW_RUNTIME_SOURCE)
foreach(FORBIDDEN_PRESENTATION_TOKEN IN ITEMS
        "CurrentUiLanguage()" "LocalizedHistoryLabel"
        "L\"Untitled Cell" "L\"Recovered Cell" "L\" [View ")
    string(FIND
        "${MAIN_WINDOW_SOURCE}${MAIN_WINDOW_RUNTIME_SOURCE}"
        "${FORBIDDEN_PRESENTATION_TOKEN}"
        PRESENTATION_POSITION)
    if(NOT PRESENTATION_POSITION LESS 0)
        message(FATAL_ERROR
            "language-specific product presentation remains: ${FORBIDDEN_PRESENTATION_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_PRESENTATION_TOKEN IN ITEMS
        "UiStringId::Text0777" "UiStringId::Text0778"
        "UiStringId::Text0012" "UiStringId::RecoveredCell"
        "HistoryUiStringId" "Utf8UserText(node.name)")
    string(FIND
        "${MAIN_WINDOW_SOURCE}${MAIN_WINDOW_RUNTIME_SOURCE}"
        "${REQUIRED_PRESENTATION_TOKEN}"
        PRESENTATION_POSITION)
    if(PRESENTATION_POSITION LESS 0)
        message(FATAL_ERROR
            "typed presentation route is incomplete: ${REQUIRED_PRESENTATION_TOKEN}")
    endif()
endforeach()

set(HISTORY_KIND_SOURCES)
foreach(HISTORY_KIND_PATH IN ITEMS
        "rust/inkpod-core/src/history.rs"
        "rust/inkpod-core/src/journal.rs"
        "rust/inkpod-ffi/src/paint_history/history.rs"
        "include/inkpod/core_ffi.h")
    file(READ
        "${INKPOD_SOURCE_DIR}/${HISTORY_KIND_PATH}"
        HISTORY_KIND_CONTENT)
    string(APPEND HISTORY_KIND_SOURCES "${HISTORY_KIND_CONTENT}")
endforeach()
foreach(FORBIDDEN_HISTORY_TOKEN IN ITEMS
        "Raster edit" "Palette edit" "Color chart edit"
        "Main-line color" "Document edit")
    string(FIND
        "${HISTORY_KIND_SOURCES}"
        "${FORBIDDEN_HISTORY_TOKEN}"
        HISTORY_TOKEN_POSITION)
    if(NOT HISTORY_TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "history string-key route remains: ${FORBIDDEN_HISTORY_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_HISTORY_TOKEN IN ITEMS
        "HistoryEntryKind" "InkpodHistoryEntryKind" "entry_kind"
        "HistoryEntryKind::ColorChart" "INKPOD_HISTORY_ENTRY_COLOR_CHART")
    string(FIND
        "${HISTORY_KIND_SOURCES}"
        "${REQUIRED_HISTORY_TOKEN}"
        HISTORY_TOKEN_POSITION)
    if(HISTORY_TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "typed history route is incomplete: ${REQUIRED_HISTORY_TOKEN}")
    endif()
endforeach()

# Lock in the three owner-draw migrations that otherwise regress silently.
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/tool_palette.cpp"
    TOOL_PALETTE_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "ToolPaletteEntry" "UiText(entry.label)" "TTM_ADDTOOLW"
        "UiText(entry->glyph)")
    string(FIND "${TOOL_PALETTE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "tool palette typed localization is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()

file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/panes/pane_dialog_layout.h"
    PANE_LAYOUT_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "BCM_GETIDEALSIZE" "MeasurePaneButtonTextWidth"
        "PaneButtonIdealWidthAtDpi" "PaneButtonTextFits"
        "PaneButtonRowCount" "PlacePaneButtonRows" "PlacePaneTargetRow")
    string(FIND "${PANE_LAYOUT_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "localized button-fit gate is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
file(READ
    "${INKPOD_SOURCE_DIR}/tests/windows_localization.cpp"
    LOCALIZATION_TEST_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "LocalizedButtonLayoutContract" "96U, 120U, 144U, 192U"
        "PaneButtonIdealWidthAtDpi" "IntersectRect"
        "UiStringId::PinDocument" "UiStringId::ReturnToFollowing"
        "OpaqueUserTextContract(UiLanguagePreference::Japanese)")
    string(FIND "${LOCALIZATION_TEST_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "multi-DPI button-fit test is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp"
    APP_SMOKE_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "PaneButtonsFit" "button fit failure" "PaneButtonIdealWidth")
    string(FIND "${APP_SMOKE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "product button-fit smoke gate is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()

file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/layer_palette.cpp"
    LAYER_PALETTE_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "kind_label_id" "format_label_id" "visibility_label_id"
        "editability_label_id" "badge_label_id" "accessible_text"
        "item.kind_text" "item.badge_text"
        "state.status_cell_width"
        "MeasureLayerPaletteStatusCellWidth"
        "LayoutLayerPaletteStatusCells"
        "PtInRect(&status_layout.editability"
        "PtInRect(&status_layout.visibility")
    string(FIND "${LAYER_PALETTE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "layer/plane presentation is not pre-resolved: ${REQUIRED_TOKEN}")
    endif()
endforeach()
string(FIND "${LAYER_PALETTE_SOURCE}" "kActionWidth" FIXED_WIDTH_POSITION)
if(NOT FIXED_WIDTH_POSITION LESS 0)
    message(FATAL_ERROR
        "layer/plane owner-draw status cells reverted to a fixed action width")
endif()
foreach(REQUIRED_TOKEN IN ITEMS
        "PlaneKindBadgeLabelId"
        "UiStringId::PlaneBadgeMainLine"
        "UiStringId::PlaneBadgeColoring"
        "UiStringId::PlaneBadgeColorTrace"
        "UiStringId::PlaneBadgeRaster"
        "UiStringId::PlaneBadgeSelection"
        "UiStringId::PlaneBadgeVectorMainLine"
        "UiStringId::PlaneBadgeVectorFill"
        "UiStringId::PlaneBadgeUnknown"
        "kLayerPalettePlaneBadgeTextFlags")
    string(FIND "${LAYER_PALETTE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact badge route is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
string(FIND
    "${LAYER_PALETTE_SOURCE}"
    "DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS"
    LEGACY_BADGE_POSITION)
if(NOT LEGACY_BADGE_POSITION LESS 0)
    message(FATAL_ERROR
        "layer/plane badge reverted to single-line ellipsis")
endif()
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/layer_palette_badge_layout.h"
    LAYER_PALETTE_BADGE_LAYOUT_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "kLayerPalettePlaneBadgeWidthDip"
        "kLayerPalettePlaneBadgeHeightDip"
        "kLayerPalettePlaneBadgeTextFlags"
        "MeasureLayerPalettePlaneBadgeText"
        "LayerPalettePlaneBadgeTextFits"
        "LayoutLayerPalettePlaneBadgeText")
    string(FIND
        "${LAYER_PALETTE_BADGE_LAYOUT_SOURCE}"
        "${REQUIRED_TOKEN}"
        TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact badge layout gate is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/layer_palette_status_layout.h"
    LAYER_PALETTE_STATUS_LAYOUT_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "kLayerPaletteStatusMinimumWidthDip"
        "kLayerPaletteStatusHorizontalPaddingDip"
        "MeasureLayerPaletteStatusCellWidth"
        "LayerPaletteStatusCellLayout"
        "LayoutLayerPaletteStatusCells")
    string(FIND
        "${LAYER_PALETTE_STATUS_LAYOUT_SOURCE}"
        "${REQUIRED_TOKEN}"
        TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane owner-draw width gate is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_TOKEN IN ITEMS
        "LayerPaletteOwnerDrawCellWidthContract"
        "UiStringId::Visible" "UiStringId::Hidden"
        "UiStringId::Editable" "UiStringId::Protected")
    string(FIND "${LOCALIZATION_TEST_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane owner-draw width test is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_TOKEN IN ITEMS
        "LayerPalettePlaneBadgeLayoutContract"
        "LayerPalettePlaneBadgeTextFits"
        "UiStringId::PlaneBadgeMainLine"
        "UiStringId::PlaneBadgeColoring"
        "UiStringId::PlaneBadgeColorTrace"
        "UiStringId::PlaneBadgeVectorMainLine")
    string(FIND "${LOCALIZATION_TEST_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact badge test is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()

file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/panes/color_dock_pane.cpp"
    COLOR_PANE_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "UiStringId::Color" "UiStringId::Palette" "UiStringId::Chart")
    string(FIND "${COLOR_PANE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "Color tab bypasses UiStringId: ${REQUIRED_TOKEN}")
    endif()
endforeach()
