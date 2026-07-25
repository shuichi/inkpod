cmake_minimum_required(VERSION 3.25)

project(inkpod_ffi_api_docs LANGUAGES NONE)

include(FetchContent)
find_package(Doxygen 1.17.0 REQUIRED)

get_filename_component(
    INKPOD_REPOSITORY_ROOT
    "${CMAKE_CURRENT_LIST_DIR}/../.."
    ABSOLUTE)

set(
    INKPOD_API_DOCS_OUTPUT_DIRECTORY
    "${INKPOD_REPOSITORY_ROOT}/docs/api"
    CACHE PATH
    "FFI API HTML の出力先")

# v2.4.2。tag の付け替えに影響されないよう release commit を固定する。
FetchContent_Declare(
    doxygen_awesome_css
    GIT_REPOSITORY https://github.com/jothepro/doxygen-awesome-css.git
    GIT_TAG d52eafe3e9303399fda15661f3d7bb8fe3d7eabc
    GIT_PROGRESS TRUE)
FetchContent_MakeAvailable(doxygen_awesome_css)
FetchContent_GetProperties(
    doxygen_awesome_css
    SOURCE_DIR DOXYGEN_AWESOME_CSS_DIRECTORY)

set(INKPOD_API_DOXYFILE "${CMAKE_CURRENT_BINARY_DIR}/Doxyfile")
configure_file(
    "${CMAKE_CURRENT_LIST_DIR}/Doxyfile.in"
    "${INKPOD_API_DOXYFILE}"
    @ONLY)

add_custom_target(
    inkpod_ffi_docs ALL
    COMMAND "${DOXYGEN_EXECUTABLE}" "${INKPOD_API_DOXYFILE}"
    COMMAND
        "${CMAKE_COMMAND}" -E copy_if_different
        "${DOXYGEN_AWESOME_CSS_DIRECTORY}/LICENSE"
        "${INKPOD_API_DOCS_OUTPUT_DIRECTORY}/html/doxygen-awesome-css-LICENSE.txt"
    WORKING_DIRECTORY "${INKPOD_REPOSITORY_ROOT}"
    DEPENDS
        "${INKPOD_REPOSITORY_ROOT}/include/inkpod/core_ffi.h"
        "${INKPOD_REPOSITORY_ROOT}/docs/ffi.md"
        "${CMAKE_CURRENT_LIST_DIR}/Doxyfile.in"
        "${CMAKE_CURRENT_LIST_DIR}/inkpod-doxygen.css"
    BYPRODUCTS
        "${INKPOD_API_DOCS_OUTPUT_DIRECTORY}/html/index.html"
        "${INKPOD_API_DOCS_OUTPUT_DIRECTORY}/html/doxygen-awesome-css-LICENSE.txt"
    COMMENT "Inkpod FFI API の日本語 HTML を生成"
    VERBATIM)
