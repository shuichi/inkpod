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
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/layer_palette.cpp"
    LAYER_PALETTE_SOURCE)
foreach(REQUIRED_TOKEN IN ITEMS
        "kind_label_id" "format_label_id" "visibility_label_id"
        "editability_label_id" "accessible_text" "item.kind_text")
    string(FIND "${LAYER_PALETTE_SOURCE}" "${REQUIRED_TOKEN}" TOKEN_POSITION)
    if(TOKEN_POSITION LESS 0)
        message(FATAL_ERROR "layer/plane presentation is not pre-resolved: ${REQUIRED_TOKEN}")
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
