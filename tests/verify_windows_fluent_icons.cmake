if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(ICON_ROOT "${INKPOD_SOURCE_DIR}/apps/windows/ui/icons/fluent")
set(SVG_ROOT "${ICON_ROOT}/svg")
set(MANIFEST "${ICON_ROOT}/selected-icons.tsv")
set(ATLAS "${ICON_ROOT}/fluent_icon_masks.bin")
set(LICENSE_FILE "${ICON_ROOT}/LICENSE.txt")
set(APP_RESOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/app_common.rc")
set(RESOURCE_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/app/resource.h")
set(ICON_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/ui/icons/fluent_icons.h")
set(ICON_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/ui/icons/fluent_icons.cpp")
set(TOOL_PALETTE "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/tool_palette.cpp")
set(LAYER_PALETTE "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/layer_palette.cpp")
set(PROVENANCE "${ICON_ROOT}/README.md")

set(EXPECTED_SEMANTIC_KEYS
    "Tool.Pencil" "Tool.Brush" "Tool.Eraser" "Tool.Fill"
    "Tool.ClosedRegionFill" "Tool.FillExtension" "Tool.Eyedropper"
    "Tool.Gradient" "Tool.Airbrush" "Tool.BoundaryAirbrush" "Tool.Blur"
    "Tool.Stamp" "Tool.DustRemoval" "Tool.AlphaGradient"
    "Pane.Visible" "Pane.Hidden" "Pane.Editable" "Pane.Protected"
    "Pane.PinDocument" "Pane.ReturnToFollowing" "Pane.Previous" "Pane.Next"
    "Pane.Fit" "Pane.OneToOne" "Pane.OpenFiles" "Pane.OpenFolder"
    "Pane.Add" "Pane.Copy" "Pane.Delete" "Pane.MoveUp" "Pane.MoveDown"
    "Pane.Properties")

foreach(REQUIRED_FILE IN ITEMS
        "${MANIFEST}" "${ATLAS}" "${LICENSE_FILE}" "${APP_RESOURCE}"
        "${RESOURCE_HEADER}" "${ICON_HEADER}" "${ICON_SOURCE}"
        "${TOOL_PALETTE}" "${LAYER_PALETTE}"
        "${PROVENANCE}")
    if(NOT EXISTS "${REQUIRED_FILE}")
        message(FATAL_ERROR "Windows Fluent icon artifact is missing: ${REQUIRED_FILE}")
    endif()
endforeach()

file(STRINGS "${MANIFEST}" MANIFEST_ROWS)
set(EXPECTED_INDEX 0)
set(SEMANTIC_KEYS)
set(SOURCE_FILES)
foreach(ROW IN LISTS MANIFEST_ROWS)
    if(ROW MATCHES "^[ \t]*#" OR ROW STREQUAL "")
        continue()
    endif()
    string(REPLACE "|" ";" FIELDS "${ROW}")
    list(LENGTH FIELDS FIELD_COUNT)
    if(NOT FIELD_COUNT EQUAL 5)
        message(FATAL_ERROR "Malformed Fluent icon manifest row: ${ROW}")
    endif()
    list(GET FIELDS 0 INDEX)
    list(GET FIELDS 1 SEMANTIC_KEY)
    list(GET FIELDS 3 SOURCE_FILE)
    list(GET FIELDS 4 EXPECTED_SHA256)
    if(NOT INDEX EQUAL EXPECTED_INDEX)
        message(FATAL_ERROR
            "Fluent icon atlas index is not contiguous: expected ${EXPECTED_INDEX}, got ${INDEX}")
    endif()
    if(SEMANTIC_KEY IN_LIST SEMANTIC_KEYS)
        message(FATAL_ERROR "Duplicate Fluent semantic key: ${SEMANTIC_KEY}")
    endif()
    if(SOURCE_FILE IN_LIST SOURCE_FILES)
        message(FATAL_ERROR "Duplicate selected Fluent source: ${SOURCE_FILE}")
    endif()
    set(SOURCE_PATH "${SVG_ROOT}/${SOURCE_FILE}")
    if(NOT EXISTS "${SOURCE_PATH}")
        message(FATAL_ERROR "Selected Fluent SVG is missing: ${SOURCE_FILE}")
    endif()
    file(SHA256 "${SOURCE_PATH}" ACTUAL_SHA256)
    if(NOT ACTUAL_SHA256 STREQUAL EXPECTED_SHA256)
        message(FATAL_ERROR
            "Selected Fluent SVG hash mismatch for ${SOURCE_FILE}: ${ACTUAL_SHA256}")
    endif()
    file(READ "${SOURCE_PATH}" SVG_TEXT)
    if(NOT SVG_TEXT MATCHES "<svg[^>]*viewBox=" OR NOT SVG_TEXT MATCHES "<path[^>]*d=")
        message(FATAL_ERROR "Selected Fluent asset is not a supported SVG path: ${SOURCE_FILE}")
    endif()
    string(TOLOWER "${SVG_TEXT}" SVG_TEXT_LOWER)
    if(SVG_TEXT_LOWER MATCHES "sf[ -]?symbols?")
        message(FATAL_ERROR "SF Symbols content is forbidden in Windows assets: ${SOURCE_FILE}")
    endif()
    list(APPEND SEMANTIC_KEYS "${SEMANTIC_KEY}")
    list(APPEND SOURCE_FILES "${SOURCE_FILE}")
    math(EXPR EXPECTED_INDEX "${EXPECTED_INDEX} + 1")
endforeach()
if(NOT EXPECTED_INDEX EQUAL 32)
    message(FATAL_ERROR "Expected 32 selected Fluent icons, found ${EXPECTED_INDEX}")
endif()
if(NOT SEMANTIC_KEYS STREQUAL EXPECTED_SEMANTIC_KEYS)
    message(FATAL_ERROR
        "Fluent semantic key order drifted: ${SEMANTIC_KEYS}")
endif()

file(GLOB CHECKED_IN_SVGS RELATIVE "${SVG_ROOT}" "${SVG_ROOT}/*.svg")
list(LENGTH CHECKED_IN_SVGS CHECKED_IN_SVG_COUNT)
if(NOT CHECKED_IN_SVG_COUNT EQUAL EXPECTED_INDEX)
    message(FATAL_ERROR
        "Selected Fluent SVG directory has untracked or missing files: ${CHECKED_IN_SVG_COUNT}")
endif()

file(SIZE "${ATLAS}" ATLAS_SIZE)
file(SHA256 "${ATLAS}" ATLAS_SHA256)
if(NOT ATLAS_SIZE EQUAL 73752)
    message(FATAL_ERROR "Fluent mask atlas has unexpected size: ${ATLAS_SIZE}")
endif()
if(NOT ATLAS_SHA256 STREQUAL
        "baaee85cfe770a2bfb9fcdc8c48fa18e1d911c6cea890639751aca94e21e1ed7")
    message(FATAL_ERROR "Fluent mask atlas hash mismatch: ${ATLAS_SHA256}")
endif()
file(READ "${ATLAS}" ATLAS_HEADER LIMIT 20 HEX)
string(TOLOWER "${ATLAS_HEADER}" ATLAS_HEADER)
if(NOT ATLAS_HEADER STREQUAL
        "494e4b504f444941010030003000200000200100")
    message(FATAL_ERROR "Fluent mask atlas header is invalid: ${ATLAS_HEADER}")
endif()

file(SHA256 "${LICENSE_FILE}" LICENSE_SHA256)
if(NOT LICENSE_SHA256 STREQUAL
        "69bc45dc42b9acb96a69823adbc6ae538374e3c0bde169b855b32c48eaaef52f")
    message(FATAL_ERROR "Fluent UI System Icons MIT license hash mismatch")
endif()
file(READ "${LICENSE_FILE}" LICENSE_TEXT)
foreach(LICENSE_MARKER IN ITEMS
        "Copyright (c) 2020 Microsoft Corporation"
        "Permission is hereby granted, free of charge"
        "THE SOFTWARE IS PROVIDED \"AS IS\"")
    string(FIND "${LICENSE_TEXT}" "${LICENSE_MARKER}" MARKER_OFFSET)
    if(MARKER_OFFSET LESS 0)
        message(FATAL_ERROR "Fluent MIT license text is incomplete: ${LICENSE_MARKER}")
    endif()
endforeach()

file(READ "${APP_RESOURCE}" APP_RESOURCE_TEXT)
if(NOT APP_RESOURCE_TEXT MATCHES
        "IDR_FLUENT_ICON_MASK_ATLAS[ \t]+RCDATA[ \t]+\"\.\./ui/icons/fluent/fluent_icon_masks\.bin\"")
    message(FATAL_ERROR "The Fluent mask atlas is not embedded by app_common.rc")
endif()
file(READ "${RESOURCE_HEADER}" RESOURCE_HEADER_TEXT)
if(NOT RESOURCE_HEADER_TEXT MATCHES
        "#[ \t]*define[ \t]+IDR_FLUENT_ICON_MASK_ATLAS[ \t]+305")
    message(FATAL_ERROR "The Fluent atlas has no fixed Win32 resource ID")
endif()
file(READ "${ICON_HEADER}" ICON_HEADER_TEXT)
if(NOT ICON_HEADER_TEXT MATCHES "enum class ToolIconId"
    OR NOT ICON_HEADER_TEXT MATCHES "enum class PaneIconId")
    message(FATAL_ERROR "Windows icons are not resolved through typed semantic IDs")
endif()
file(READ "${ICON_SOURCE}" ICON_SOURCE_TEXT)
foreach(ICON_RUNTIME_MARKER IN ITEMS
        "GetDpiForWindow" "WM_DPICHANGED_AFTERPARENT" "WM_THEMECHANGED"
        "WM_SYSCOLORCHANGE" "COLOR_BTNTEXT" "COLOR_GRAYTEXT"
        "ShowTextFallback" "BS_ICON" "static const AtlasView application_atlas")
    string(FIND "${ICON_SOURCE_TEXT}" "${ICON_RUNTIME_MARKER}"
        ICON_RUNTIME_MARKER_OFFSET)
    if(ICON_RUNTIME_MARKER_OFFSET LESS 0)
        message(FATAL_ERROR
            "Fluent icon DPI/theme/fallback contract is incomplete: ${ICON_RUNTIME_MARKER}")
    endif()
endforeach()
file(READ "${TOOL_PALETTE}" TOOL_PALETTE_TEXT)
set(EXPECTED_TOOL_IDS
    Pencil Brush Eraser Fill ClosedRegionFill FillExtension Eyedropper
    Gradient Airbrush BoundaryAirbrush Blur Stamp DustRemoval AlphaGradient)
foreach(TOOL_ID IN LISTS EXPECTED_TOOL_IDS)
    string(REGEX MATCHALL "ToolIconId::${TOOL_ID}([^A-Za-z0-9_]|$)" TOOL_ID_MATCHES
        "${TOOL_PALETTE_TEXT}")
    list(LENGTH TOOL_ID_MATCHES TOOL_ID_MATCH_COUNT)
    if(NOT TOOL_ID_MATCH_COUNT EQUAL 1)
        message(FATAL_ERROR
            "Tool palette must map ToolIconId::${TOOL_ID} exactly once")
    endif()
endforeach()
foreach(TOOL_DRAW_MARKER IN ITEMS
        "ODS_DISABLED" "ODS_SELECTED" "ODS_FOCUS" "COLOR_HIGHLIGHT"
        "DrawToolIcon" "fallback_label")
    string(FIND "${TOOL_PALETTE_TEXT}" "${TOOL_DRAW_MARKER}"
        TOOL_DRAW_MARKER_OFFSET)
    if(TOOL_DRAW_MARKER_OFFSET LESS 0)
        message(FATAL_ERROR
            "Tool icon owner-draw state/fallback contract is incomplete: ${TOOL_DRAW_MARKER}")
    endif()
endforeach()

file(READ "${LAYER_PALETTE}" LAYER_PALETTE_TEXT)
foreach(LAYER_ICON_MARKER IN ITEMS
        "PaneIconId::Visible" "PaneIconId::Hidden" "PaneIconId::Editable"
        "PaneIconId::Protected" "PaneIconId::Add" "PaneIconId::Copy"
        "PaneIconId::Delete" "PaneIconId::MoveUp" "PaneIconId::MoveDown"
        "PaneIconId::Properties" "DrawPaneIcon" "SetPaneIconButton")
    string(FIND "${LAYER_PALETTE_TEXT}" "${LAYER_ICON_MARKER}"
        LAYER_ICON_MARKER_OFFSET)
    if(LAYER_ICON_MARKER_OFFSET LESS 0)
        message(FATAL_ERROR
            "Layer/Plane semantic state icon is missing: ${LAYER_ICON_MARKER}")
    endif()
endforeach()

foreach(PIN_SOURCE IN ITEMS
        "apps/windows/ui/panes/color_dock_pane.cpp"
        "apps/windows/ui/panes/locator_pane.cpp"
        "apps/windows/ui/panes/sequence_pane.cpp"
        "apps/windows/ui/panes/light_table_pane.cpp")
    file(READ "${INKPOD_SOURCE_DIR}/${PIN_SOURCE}" PIN_SOURCE_TEXT)
    if(NOT PIN_SOURCE_TEXT MATCHES "SetPaneIconButton"
        OR NOT PIN_SOURCE_TEXT MATCHES "PaneIconId::PinDocument"
        OR NOT PIN_SOURCE_TEXT MATCHES "PaneIconId::ReturnToFollowing")
        message(FATAL_ERROR
            "Pane pin/follow semantic icons are incomplete: ${PIN_SOURCE}")
    endif()
endforeach()

file(READ "${INKPOD_SOURCE_DIR}/apps/windows/ui/panes/subpalette_pane.cpp"
    SUBPALETTE_SOURCE_TEXT)
foreach(SUBPALETTE_ICON_MARKER IN ITEMS
        "SetPaneIconButton" "PaneIconId::Previous" "PaneIconId::Next"
        "PaneIconId::Fit" "PaneIconId::OneToOne" "PaneIconId::OpenFiles"
        "PaneIconId::OpenFolder" "CreateToolCursor" "ToolIconId::Eyedropper")
    string(FIND "${SUBPALETTE_SOURCE_TEXT}" "${SUBPALETTE_ICON_MARKER}"
        SUBPALETTE_ICON_MARKER_OFFSET)
    if(SUBPALETTE_ICON_MARKER_OFFSET LESS 0)
        message(FATAL_ERROR
            "Subpalette navigation/cursor icon contract is incomplete: ${SUBPALETTE_ICON_MARKER}")
    endif()
endforeach()
foreach(SUBPALETTE_INTERACTION_MARKER IN ITEMS
        "DLGC_WANTARROWS" "IDC_SUBPALETTE_SAMPLE_SWATCH"
        "PlaceCompactSubpaletteButtonRows" "TOOLTIPS_CLASSW"
        "TTM_ADDTOOLW" "UiStringId::SubpaletteOpenFiles"
        "UiStringId::SubpaletteOpenFolder")
    string(FIND "${SUBPALETTE_SOURCE_TEXT}" "${SUBPALETTE_INTERACTION_MARKER}"
        SUBPALETTE_INTERACTION_OFFSET)
    if(SUBPALETTE_INTERACTION_OFFSET LESS 0)
        message(FATAL_ERROR
            "Subpalette interaction contract is incomplete: ${SUBPALETTE_INTERACTION_MARKER}")
    endif()
endforeach()
file(READ "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp"
    MAIN_WINDOW_RUNTIME_TEXT)
foreach(SUBPALETTE_RUNTIME_MARKER IN ITEMS
        "subpalette_presentation_epoch" "SelectColorDockPaneDrawingColor"
        "INKPOD_IO_REFERENCE_FILES" "INKPOD_IO_REFERENCE_FOLDER"
        "QueueFileIoWork"
        "inkpod_subpalette_select_cached_raster")
    string(FIND "${MAIN_WINDOW_RUNTIME_TEXT}" "${SUBPALETTE_RUNTIME_MARKER}"
        SUBPALETTE_RUNTIME_OFFSET)
    if(SUBPALETTE_RUNTIME_OFFSET LESS 0)
        message(FATAL_ERROR
            "Subpalette runtime interaction is incomplete: ${SUBPALETTE_RUNTIME_MARKER}")
    endif()
endforeach()
foreach(SUBPALETTE_LEGACY_LOAD_MARKER IN ITEMS
        "QueueSubpaletteLoad(" "inkpod_subpalette_load_common_raster("
        "ReadSubpaletteFileCallback" "EnumerateSubpaletteFolder")
    string(FIND "${MAIN_WINDOW_RUNTIME_TEXT}" "${SUBPALETTE_LEGACY_LOAD_MARKER}"
        SUBPALETTE_LEGACY_LOAD_OFFSET)
    if(SUBPALETTE_LEGACY_LOAD_OFFSET GREATER_EQUAL 0)
        message(FATAL_ERROR
            "Subpalette navigation restored per-image file/decode loading: ${SUBPALETTE_LEGACY_LOAD_MARKER}")
    endif()
endforeach()

file(READ "${INKPOD_SOURCE_DIR}/CMakeLists.txt" CMAKE_TEXT)
if(CMAKE_TEXT MATCHES "generate-windows-fluent-icons")
    message(FATAL_ERROR "Normal CMake build must not run the Fluent icon generator")
endif()
foreach(CMAKE_MARKER IN ITEMS
        "fluent_icon_masks.bin" "fluent_icons.cpp" "msimg32")
    string(FIND "${CMAKE_TEXT}" "${CMAKE_MARKER}" CMAKE_MARKER_OFFSET)
    if(CMAKE_MARKER_OFFSET LESS 0)
        message(FATAL_ERROR
            "Windows Fluent icon build integration is missing: ${CMAKE_MARKER}")
    endif()
endforeach()

file(READ "${PROVENANCE}" PROVENANCE_TEXT)
foreach(PROVENANCE_MARKER IN ITEMS
        "microsoft/fluentui-system-icons" "1.1.337"
        "84e8a2ae0e55b3cbe176b5cc33154fe82ef363cc"
        "selected-icons.tsv" "fluent_icon_masks.bin")
    string(FIND "${PROVENANCE_TEXT}" "${PROVENANCE_MARKER}"
        PROVENANCE_MARKER_OFFSET)
    if(PROVENANCE_MARKER_OFFSET LESS 0)
        message(FATAL_ERROR
            "Fluent icon provenance is incomplete: ${PROVENANCE_MARKER}")
    endif()
endforeach()

file(GLOB_RECURSE WINDOWS_SOURCE_FILES
    "${INKPOD_SOURCE_DIR}/apps/windows/*.c"
    "${INKPOD_SOURCE_DIR}/apps/windows/*.cpp"
    "${INKPOD_SOURCE_DIR}/apps/windows/*.h"
    "${INKPOD_SOURCE_DIR}/apps/windows/*.rc")
foreach(WINDOWS_SOURCE_FILE IN LISTS WINDOWS_SOURCE_FILES)
    file(READ "${WINDOWS_SOURCE_FILE}" WINDOWS_SOURCE_TEXT)
    string(TOLOWER "${WINDOWS_SOURCE_TEXT}" WINDOWS_SOURCE_TEXT_LOWER)
    if(WINDOWS_SOURCE_TEXT_LOWER MATCHES "sf[ _-]?symbols?")
        message(FATAL_ERROR
            "SF Symbols content is forbidden in Windows source: ${WINDOWS_SOURCE_FILE}")
    endif()
endforeach()

file(GLOB_RECURSE CORE_BOUNDARY_FILES
    "${INKPOD_SOURCE_DIR}/rust/*.rs"
    "${INKPOD_SOURCE_DIR}/include/*.h")
foreach(CORE_BOUNDARY_FILE IN LISTS CORE_BOUNDARY_FILES)
    file(READ "${CORE_BOUNDARY_FILE}" CORE_BOUNDARY_TEXT)
    string(TOLOWER "${CORE_BOUNDARY_TEXT}" CORE_BOUNDARY_TEXT)
    if(CORE_BOUNDARY_TEXT MATCHES "fluent[ _-]?(ui|icon)|sf[ _-]?symbols?")
        message(FATAL_ERROR
            "Platform icon name crossed the Rust/C ABI boundary: ${CORE_BOUNDARY_FILE}")
    endif()
endforeach()

file(READ "${INKPOD_SOURCE_DIR}/docs/third-party-notices.md" THIRD_PARTY_TEXT)
foreach(NOTICE_MARKER IN ITEMS
        "Fluent UI System Icons"
        "84e8a2ae0e55b3cbe176b5cc33154fe82ef363cc"
        "Copyright (c) 2020 Microsoft Corporation")
    string(FIND "${THIRD_PARTY_TEXT}" "${NOTICE_MARKER}" NOTICE_OFFSET)
    if(NOTICE_OFFSET LESS 0)
        message(FATAL_ERROR "Third-party notice is missing: ${NOTICE_MARKER}")
    endif()
endforeach()

message(STATUS
    "Verified 32 Fluent semantic icons, fixed SVG/atlas hashes, resource embedding, and MIT notice")
