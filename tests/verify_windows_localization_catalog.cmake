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
# Git checkout settings may materialize text files with LF or CRLF. Hash the
# catalog's canonical LF form so generated artifacts are portable between
# developer worktrees and CI runners.
file(READ "${CATALOG}" CATALOG_CONTENT)
string(REPLACE "\r\n" "\n" CATALOG_CONTENT "${CATALOG_CONTENT}")
string(REPLACE "\r" "\n" CATALOG_CONTENT "${CATALOG_CONTENT}")
string(SHA256 CATALOG_SHA256 "${CATALOG_CONTENT}")
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
    foreach(DUPLICATED_CONTROL_ESCAPE IN ITEMS
            [=[\t\u0009]=]
            [=[\n\u000a]=]
            [=[\r\u000d]=]
            [=[\0\u0000]=])
        string(FIND
            "${GENERATED_CONTENT}"
            "${DUPLICATED_CONTROL_ESCAPE}"
            DUPLICATED_CONTROL_POSITION)
        if(NOT DUPLICATED_CONTROL_POSITION LESS 0)
            message(FATAL_ERROR
                "generated localization artifact duplicates a control escape: ${GENERATED_FILE}")
        endif()
    endforeach()
endforeach()

file(READ "${RESOURCE_JA}" RESOURCE_JA_CONTENT)
file(READ "${RESOURCE_EN}" RESOURCE_EN_CONTENT)
foreach(GENERATED_RESOURCE_CONTENT IN ITEMS
        "${RESOURCE_JA_CONTENT}" "${RESOURCE_EN_CONTENT}")
    string(FIND "${GENERATED_RESOURCE_CONTENT}" "@INKPOD_UI_TEXT_" MARKER_POSITION)
    if(NOT MARKER_POSITION LESS 0)
        message(FATAL_ERROR "generated resource contains an unresolved catalog marker")
    endif()
    string(FIND "${GENERATED_RESOURCE_CONTENT}" [=[\u]=] JSON_UNICODE_ESCAPE_POSITION)
    if(NOT JSON_UNICODE_ESCAPE_POSITION LESS 0)
        message(FATAL_ERROR
            "generated resource contains a JSON Unicode escape that rc.exe displays literally")
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

# Both languages are generated from one resource topology and must expose the
# exact same command/control/resource identifiers.  Comparing every symbolic
# ID prevents one language from silently losing or renaming a menu, dialog,
# control, string, or other product resource while still compiling.
string(REGEX MATCHALL
    "(IDM|IDC|IDD|IDR|IDS)_[A-Z0-9_]+"
    RESOURCE_JA_IDS
    "${RESOURCE_JA_CONTENT}")
string(REGEX MATCHALL
    "(IDM|IDC|IDD|IDR|IDS)_[A-Z0-9_]+"
    RESOURCE_EN_IDS
    "${RESOURCE_EN_CONTENT}")
list(REMOVE_DUPLICATES RESOURCE_JA_IDS)
list(REMOVE_DUPLICATES RESOURCE_EN_IDS)
list(SORT RESOURCE_JA_IDS)
list(SORT RESOURCE_EN_IDS)
list(LENGTH RESOURCE_JA_IDS RESOURCE_JA_ID_COUNT)
list(LENGTH RESOURCE_EN_IDS RESOURCE_EN_ID_COUNT)
if(RESOURCE_JA_ID_COUNT EQUAL 0
    OR NOT RESOURCE_JA_IDS STREQUAL RESOURCE_EN_IDS)
    message(FATAL_ERROR
        "Japanese/English resource identifier sets differ: "
        "ja=${RESOURCE_JA_ID_COUNT}, en=${RESOURCE_EN_ID_COUNT}")
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

    # Product-owned dialog/editor presentation must come from UiStringId.
    # User-owned values are stored in separate `value`/document fields and are
    # intentionally outside this gate.  Match both assignment sites and
    # in-struct default initializers, including multi-line arrays.
    string(REGEX MATCH
        "\\.(title|label|labels|parameter_labels|channel_labels|mode_labels|option1_label|option2_label|preview_idle_text)[ \t\r\n]*=[^;]*L\""
        RAW_PRESENTATION_ASSIGNMENT
        "${CONTENT}")
    string(REGEX MATCH
        "(^|[^A-Za-z0-9_])(title|label|labels|parameter_labels|channel_labels|mode_labels|option1_label|option2_label|preview_idle_text)[ \t\r\n]*\\{[^;]*L\""
        RAW_PRESENTATION_DEFAULT
        "${CONTENT}")
    if(RAW_PRESENTATION_ASSIGNMENT OR RAW_PRESENTATION_DEFAULT)
        message(FATAL_ERROR
            "product presentation literal bypasses UiStringId: ${RELATIVE_PATH}")
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
        "UiText(entry->fallback_label)" "DrawToolIcon" "ToolIconId")
    string(FIND "${TOOL_PALETTE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "tool palette typed localization is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp"
    APP_SMOKE_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "TOOLINFOW tool" "TTM_GETTEXTW" "tooltip_name"
        "state.Workspace().tools.palette_dialog.tooltip")
    string(FIND "${APP_SMOKE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "tool palette tooltip runtime contract is incomplete: ${REQUIRED_TOKEN}")
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
        "LayoutLayerPaletteStatusCells"
        "DrawPaneIcon" "PaneIconId::Visible" "PaneIconId::Protected"
        "PtInRect(&status_layout.editability"
        "PtInRect(&status_layout.visibility")
    string(FIND "${LAYER_PALETTE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "layer/plane presentation is not pre-resolved: ${REQUIRED_TOKEN}")
    endif()
endforeach()
string(FIND
    "${LAYER_PALETTE_SOURCE}"
    "MeasureLayerPaletteStatusCellWidth"
    MEASURED_STATUS_CELL_POSITION)
if(NOT MEASURED_STATUS_CELL_POSITION LESS 0)
    message(FATAL_ERROR
        "layer/plane compact status cells reverted to localized text measurement")
endif()
foreach(REQUIRED_TOKEN IN ITEMS
        "PlaneKindBadgeLabelId"
        "UiStringId::PlaneBadgeMainLine"
        "UiStringId::PlaneBadgeColoring"
        "UiStringId::PlaneBadgeRaster"
        "UiStringId::PlaneBadgeSelection"
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
        "kLayerPaletteStatusButtonSizeDip"
        "kLayerPaletteStatusGapDip"
        "LayerPaletteStatusCellLayout"
        "LayoutLayerPaletteStatusCells")
    string(FIND
        "${LAYER_PALETTE_STATUS_LAYOUT_SOURCE}"
        "${REQUIRED_TOKEN}"
        TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact status-cell gate is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
file(READ
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/layer_palette_compact_layout.h"
    LAYER_PALETTE_COMPACT_LAYOUT_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "kLayerPaletteMarginDip"
        "kLayerPaletteLayerTileHeightDip"
        "kLayerPalettePlaneTileHeightDip"
        "kLayerPaletteThumbnailWidthDip"
        "kLayerPaletteThumbnailHeightDip"
        "kLayerPaletteActionButtonSizeDip"
        "kLayerPaletteActionButtonCount")
    string(FIND
        "${LAYER_PALETTE_COMPACT_LAYOUT_SOURCE}"
        "${REQUIRED_TOKEN}"
        TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact-density gate is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_TOKEN IN ITEMS
        "LayerPaletteOwnerDrawCompactCellContract"
        "kLayerPaletteStatusButtonSizeDip"
        "UiStringId::Visible" "UiStringId::Hidden"
        "UiStringId::Editable" "UiStringId::Protected")
    string(FIND "${LOCALIZATION_TEST_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact status-cell test is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_TOKEN IN ITEMS
        "LayerPaletteCompactDensityContract"
        "LayerPaletteActionTextContract"
        "UiStringId::LayerActionNew"
        "UiStringId::PlaneActionProperties")
    string(FIND "${LOCALIZATION_TEST_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact action test is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_TOKEN IN ITEMS
        "SetPaneIconButton"
        "TOOLTIPS_CLASSW"
        "TTM_ADDTOOLW"
        "TTM_UPDATETIPTEXTW"
        "UiStringId::LayerActionNew"
        "UiStringId::PlaneActionProperties")
    string(FIND "${LAYER_PALETTE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR
            "layer/plane compact action presentation is incomplete: ${REQUIRED_TOKEN}")
    endif()
endforeach()
foreach(REQUIRED_TOKEN IN ITEMS
        "LayerPalettePlaneBadgeLayoutContract"
        "LayerPalettePlaneBadgeTextFits"
        "UiStringId::PlaneBadgeMainLine"
        "UiStringId::PlaneBadgeColoring")
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
