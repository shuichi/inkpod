if(NOT DEFINED INKPOD_PACKAGE_DIR)
    message(FATAL_ERROR "INKPOD_PACKAGE_DIR is required")
endif()
if(NOT DEFINED INKPOD_PACKAGE_MANIFEST)
    message(FATAL_ERROR "INKPOD_PACKAGE_MANIFEST is required")
endif()
if(NOT DEFINED INKPOD_PACKAGE_ARCHITECTURE)
    message(FATAL_ERROR "INKPOD_PACKAGE_ARCHITECTURE is required")
endif()
if(NOT DEFINED INKPOD_PROJECT_VERSION)
    message(FATAL_ERROR "INKPOD_PROJECT_VERSION is required")
endif()

file(READ "${INKPOD_PACKAGE_MANIFEST}" package_manifest)
string(
    FIND
    "${package_manifest}"
    "Version=\"${INKPOD_PROJECT_VERSION}.0\""
    manifest_version_offset)
if(manifest_version_offset EQUAL -1)
    message(FATAL_ERROR
        "Package.appxmanifest version does not match CMake project version "
        "${INKPOD_PROJECT_VERSION}")
endif()
foreach(required_manifest_text IN ITEMS
        "Executable=\"inkpod.exe\""
        "ProcessorArchitecture=\"${INKPOD_PACKAGE_ARCHITECTURE}\""
        "MinVersion=\"10.0.18362.0\""
        "Publisher=\"CN=inkpod\""
        "Category=\"windows.fileTypeAssociation\""
        "<uap:FileTypeAssociation Name=\"inkpod\">"
        "<uap:FileType>.inkpod</uap:FileType>")
    string(FIND "${package_manifest}" "${required_manifest_text}" manifest_text_offset)
    if(manifest_text_offset EQUAL -1)
        message(FATAL_ERROR
            "Package.appxmanifest is missing ${required_manifest_text}")
    endif()
endforeach()

set(assets_dir "${INKPOD_PACKAGE_DIR}/Assets")
set(expected_assets app.ico)

foreach(base IN ITEMS StoreLogo MedTile AppList WideTile)
    list(APPEND expected_assets
        "${base}.png"
        "${base}.scale-125.png"
        "${base}.scale-150.png"
        "${base}.scale-200.png"
        "${base}.scale-400.png")
endforeach()

foreach(size IN ITEMS 16 20 24 30 32 36 40 48 60 64 72 80 96 256)
    list(APPEND expected_assets
        "AppList.targetsize-${size}.png"
        "AppList.targetsize-${size}_altform-unplated.png")
endforeach()

list(LENGTH expected_assets expected_count)
if(NOT expected_count EQUAL 49)
    message(FATAL_ERROR "Asset test itself has an unexpected count: ${expected_count}")
endif()

foreach(asset IN LISTS expected_assets)
    set(path "${assets_dir}/${asset}")
    if(NOT EXISTS "${path}")
        message(FATAL_ERROR "Missing Windows package asset: ${asset}")
    endif()
    file(SIZE "${path}" asset_size)
    if(asset_size EQUAL 0)
        message(FATAL_ERROR "Empty Windows package asset: ${asset}")
    endif()
endforeach()

function(require_png_dimensions name expected_dimensions)
    file(READ "${assets_dir}/${name}" header LIMIT 24 HEX)
    string(TOLOWER "${header}" header)
    string(SUBSTRING "${header}" 0 16 signature)
    string(SUBSTRING "${header}" 32 16 dimensions)
    if(NOT signature STREQUAL "89504e470d0a1a0a")
        message(FATAL_ERROR "${name} is not a PNG file")
    endif()
    if(NOT dimensions STREQUAL "${expected_dimensions}")
        message(FATAL_ERROR "${name} has unexpected dimensions: ${dimensions}")
    endif()
endfunction()

require_png_dimensions("StoreLogo.png" "0000003200000032")
require_png_dimensions("MedTile.png" "0000009600000096")
require_png_dimensions("AppList.png" "0000002c0000002c")
require_png_dimensions("AppList.scale-200.png" "0000005800000058")
require_png_dimensions("AppList.targetsize-256.png" "0000010000000100")
require_png_dimensions("WideTile.png" "0000013600000096")

file(READ "${assets_dir}/app.ico" ico_header LIMIT 86 HEX)
string(TOLOWER "${ico_header}" ico_header)
string(SUBSTRING "${ico_header}" 0 12 ico_directory)
if(NOT ico_directory STREQUAL "000001000500")
    message(FATAL_ERROR "app.ico is not a five-image Windows icon: ${ico_directory}")
endif()

set(expected_icon_dimensions "1010;1818;2020;3030;0000")
set(icon_index 0)
foreach(expected IN LISTS expected_icon_dimensions)
    math(EXPR entry_offset "12 + ${icon_index} * 32")
    string(SUBSTRING "${ico_header}" ${entry_offset} 4 actual)
    if(NOT actual STREQUAL "${expected}")
        message(FATAL_ERROR
            "app.ico entry ${icon_index} has unexpected dimensions: ${actual}")
    endif()
    math(EXPR icon_index "${icon_index} + 1")
endforeach()
