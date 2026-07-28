cmake_minimum_required(VERSION 3.25)

if(NOT DEFINED INKPOD_SOURCE_DIR OR INKPOD_SOURCE_DIR STREQUAL "")
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()
if(NOT DEFINED INKPOD_TEST_ROOT OR INKPOD_TEST_ROOT STREQUAL "")
    message(FATAL_ERROR "INKPOD_TEST_ROOT is required")
endif()

include("${INKPOD_SOURCE_DIR}/cmake/InkpodMsvcRuntime.cmake")

function(inkpod_create_fake_crt directory)
    file(MAKE_DIRECTORY "${directory}")
    foreach(_inkpod_runtime IN ITEMS
            msvcp140.dll
            vcruntime140.dll
            vcruntime140_1.dll)
        file(WRITE "${directory}/${_inkpod_runtime}" "fixture")
    endforeach()
endfunction()

function(inkpod_assert_same_path actual expected context)
    set(_inkpod_actual "${actual}")
    set(_inkpod_expected "${expected}")
    cmake_path(NORMAL_PATH _inkpod_actual)
    cmake_path(NORMAL_PATH _inkpod_expected)
    if(NOT _inkpod_actual STREQUAL _inkpod_expected)
        message(FATAL_ERROR
            "${context}: expected '${_inkpod_expected}', got '${_inkpod_actual}'")
    endif()
endfunction()

file(REMOVE_RECURSE "${INKPOD_TEST_ROOT}")
file(MAKE_DIRECTORY "${INKPOD_TEST_ROOT}")

set(_inkpod_hosted_vc "${INKPOD_TEST_ROOT}/hosted/VC")
set(_inkpod_hosted_crt
    "${_inkpod_hosted_vc}/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT")
inkpod_create_fake_crt("${_inkpod_hosted_crt}")
set(ENV{VCToolsRedistDir}
    "${_inkpod_hosted_vc}/Redist/MSVC/14.44.35112")
inkpod_find_msvc_crt_directory(_inkpod_selected_hosted_crt
    VC_DIRECTORY "${_inkpod_hosted_vc}"
    TOOLSET_VERSION "14.44.35207"
    ARCHITECTURE x64)
inkpod_assert_same_path(
    "${_inkpod_selected_hosted_crt}"
    "${_inkpod_hosted_crt}"
    "VCToolsRedistDir must override a different compiler toolset version")

unset(ENV{VCToolsRedistDir})
set(_inkpod_exact_vc "${INKPOD_TEST_ROOT}/exact/VC")
set(_inkpod_exact_crt
    "${_inkpod_exact_vc}/Redist/MSVC/14.51.36000/arm64/Microsoft.VC143.CRT")
inkpod_create_fake_crt("${_inkpod_exact_crt}")
inkpod_find_msvc_crt_directory(_inkpod_selected_exact_crt
    VC_DIRECTORY "${_inkpod_exact_vc}"
    TOOLSET_VERSION "14.51.36000"
    ARCHITECTURE arm64)
inkpod_assert_same_path(
    "${_inkpod_selected_exact_crt}"
    "${_inkpod_exact_crt}"
    "The matching toolset directory must remain a supported fallback")

set(_inkpod_unique_vc "${INKPOD_TEST_ROOT}/unique/VC")
set(_inkpod_unique_crt
    "${_inkpod_unique_vc}/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.CRT")
inkpod_create_fake_crt("${_inkpod_unique_crt}")
inkpod_find_msvc_crt_directory(_inkpod_selected_unique_crt
    VC_DIRECTORY "${_inkpod_unique_vc}"
    TOOLSET_VERSION "14.44.35207"
    ARCHITECTURE x64)
inkpod_assert_same_path(
    "${_inkpod_selected_unique_crt}"
    "${_inkpod_unique_crt}"
    "A unique installed redist must be used when the environment is unavailable")

file(REMOVE_RECURSE "${INKPOD_TEST_ROOT}")
