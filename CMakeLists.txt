cmake_minimum_required(VERSION 3.25)

cmake_policy(SET CMP0091 NEW)
project(inkpod VERSION 0.1.0 LANGUAGES C CXX)

configure_file(
    "${CMAKE_SOURCE_DIR}/apps/windows/app/app_version.h.in"
    "${CMAKE_BINARY_DIR}/generated/app_version.h"
    @ONLY)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")

if(CMAKE_CONFIGURATION_TYPES)
    message(FATAL_ERROR "inkpod uses a single-config generator so Cargo and CMake share one profile")
endif()

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Debug CACHE STRING "Build type" FORCE)
endif()

find_program(
    CARGO_EXECUTABLE
    NAMES cargo
    HINTS "$ENV{USERPROFILE}/.cargo/bin" "$ENV{HOME}/.cargo/bin"
    DOC "Path to the Rust Cargo executable"
    REQUIRED)

set(INKPOD_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo-target")
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(INKPOD_RUST_PROFILE release)
    set(INKPOD_CARGO_PROFILE_ARGUMENT --release)
else()
    set(INKPOD_RUST_PROFILE debug)
    set(INKPOD_CARGO_PROFILE_ARGUMENT)
endif()

if(WIN32)
    set(INKPOD_RUST_STATICLIB "${INKPOD_CARGO_TARGET_DIR}/${INKPOD_RUST_PROFILE}/inkpod_ffi.lib")
else()
    set(INKPOD_RUST_STATICLIB "${INKPOD_CARGO_TARGET_DIR}/${INKPOD_RUST_PROFILE}/libinkpod_ffi.a")
endif()
set(INKPOD_RUST_RLIB "${INKPOD_CARGO_TARGET_DIR}/${INKPOD_RUST_PROFILE}/libinkpod_ffi.rlib")
set(INKPOD_RUST_STAMP "${INKPOD_CARGO_TARGET_DIR}/${INKPOD_RUST_PROFILE}/inkpod_ffi.cmake.stamp")

file(GLOB_RECURSE INKPOD_RUST_SOURCE_INPUTS CONFIGURE_DEPENDS
    "${CMAKE_SOURCE_DIR}/rust/inkpod-core/src/*.rs"
    "${CMAKE_SOURCE_DIR}/rust/inkpod-ffi/src/*.rs"
    "${CMAKE_SOURCE_DIR}/rust/inkpod-format/src/*.rs"
    "${CMAKE_SOURCE_DIR}/rust/inkpod-image/src/*.rs")

set(INKPOD_RUST_INPUTS
    "${CMAKE_SOURCE_DIR}/Cargo.toml"
    "${CMAKE_SOURCE_DIR}/Cargo.lock"
    "${CMAKE_SOURCE_DIR}/rust/inkpod-core/Cargo.toml"
    "${CMAKE_SOURCE_DIR}/rust/inkpod-ffi/Cargo.toml"
    "${CMAKE_SOURCE_DIR}/rust/inkpod-format/Cargo.toml"
    "${CMAKE_SOURCE_DIR}/rust/inkpod-image/Cargo.toml"
    ${INKPOD_RUST_SOURCE_INPUTS})

add_custom_command(
    OUTPUT "${INKPOD_RUST_STAMP}"
    BYPRODUCTS "${INKPOD_RUST_STATICLIB}" "${INKPOD_RUST_RLIB}"
    COMMAND "${CMAKE_COMMAND}" -E env
            "CARGO_TARGET_DIR=${INKPOD_CARGO_TARGET_DIR}"
            "${CARGO_EXECUTABLE}" build --locked --package inkpod-ffi ${INKPOD_CARGO_PROFILE_ARGUMENT}
    COMMAND "${CMAKE_COMMAND}" -E touch_nocreate
            "${INKPOD_RUST_STATICLIB}" "${INKPOD_RUST_RLIB}"
    COMMAND "${CMAKE_COMMAND}" -E touch "${INKPOD_RUST_STAMP}"
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
    DEPENDS ${INKPOD_RUST_INPUTS}
    COMMENT "Building inkpod-ffi Rust static library (${INKPOD_RUST_PROFILE})"
    VERBATIM)

add_custom_target(inkpod_rust ALL DEPENDS "${INKPOD_RUST_STAMP}")
add_library(inkpod_ffi_static STATIC IMPORTED GLOBAL)
set_target_properties(inkpod_ffi_static PROPERTIES
    IMPORTED_LOCATION "${INKPOD_RUST_STATICLIB}"
    INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_SOURCE_DIR}/include")
add_dependencies(inkpod_ffi_static inkpod_rust)
if(WIN32)
    set_property(TARGET inkpod_ffi_static PROPERTY INTERFACE_LINK_LIBRARIES
        "ntdll;userenv;ws2_32;dbghelp")
endif()

include(CTest)

add_test(
    NAME inkpod_windows_assets
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_PACKAGE_DIR=${CMAKE_SOURCE_DIR}/apps/windows/package"
            "-DINKPOD_PROJECT_VERSION=${PROJECT_VERSION}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_assets.cmake")

add_test(
    NAME inkpod_windows_command_routes
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_command_routes.cmake")

add_library(inkpod_header_c11 OBJECT tests/header_c11.c)
target_include_directories(inkpod_header_c11 PRIVATE "${CMAKE_SOURCE_DIR}/include")

if(MSVC)
    function(inkpod_enable_strict_warnings target)
        target_compile_options(${target} PRIVATE /W4 /WX /permissive- /utf-8)
    endfunction()
    inkpod_enable_strict_warnings(inkpod_header_c11)
endif()

if(WIN32)
    add_executable(inkpod WIN32
        apps/windows/app/app_context.h
        apps/windows/app/clipboard_adapter.cpp
        apps/windows/app/com_runtime.cpp
        apps/windows/app/main.cpp
        apps/windows/app/core_engine.cpp
        apps/windows/app/document_shell.cpp
        apps/windows/app/app.rc
        apps/windows/renderer/canvas.cpp
        apps/windows/ui/dialogs/about_dialog.cpp
        apps/windows/ui/dialogs/basic_dialogs.cpp
        apps/windows/ui/dialogs/batch_dialog.cpp
        apps/windows/ui/dialogs/effects_dialogs.cpp
        apps/windows/ui/panes/document_panes.cpp
        apps/windows/ui/panes/color_panes.cpp
        apps/windows/ui/tools/view_controller.cpp
        apps/windows/ui/tools/fill_controller.cpp
        apps/windows/ui/tools/selection_controller.cpp
        apps/windows/ui/tools/floating_paste_controller.cpp
        apps/windows/ui/tools/vector_controller.cpp
        apps/windows/ui/effects_controller.cpp
        apps/windows/ui/batch_controller.cpp
        apps/windows/ui/main_window.cpp
        tests/abi_smoke.cpp
        $<TARGET_OBJECTS:inkpod_header_c11>)
    target_include_directories(inkpod PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_SOURCE_DIR}/apps/windows/renderer"
        "${CMAKE_BINARY_DIR}/generated"
        "${CMAKE_SOURCE_DIR}/include")
    target_compile_definitions(inkpod PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod PRIVATE
        inkpod_ffi_static
        comdlg32
        comctl32
        d2d1
        d3d11
        dxgi
        ole32
        shell32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod)
        target_compile_options(inkpod PRIVATE /EHsc)
        target_link_options(inkpod PRIVATE /DEBUG:FULL)
        if(CMAKE_BUILD_TYPE STREQUAL "Release")
            # Retain optimized code while emitting PDB metadata. Besides useful
            # crash diagnostics, this permits execution under local enterprise
            # application-control policies that reject fresh opaque binaries.
            target_compile_options(inkpod PRIVATE /Zi)
        endif()
        # The Common Controls v6 / PerMonitorV2 manifest is compiled by app.rc.
        target_link_options(inkpod PRIVATE /MANIFEST:NO)
    endif()

    add_test(NAME inkpod_abi_smoke COMMAND inkpod --abi-smoke-test)
    add_test(NAME inkpod_windows_smoke COMMAND inkpod --smoke-test)

    file(GLOB INKPOD_MSIX_ASSETS CONFIGURE_DEPENDS
        "${CMAKE_SOURCE_DIR}/apps/windows/package/Assets/*.png")
    if(NOT MSVC)
        message(FATAL_ERROR "inkpod MSIX packaging requires the supported MSVC toolchain")
    endif()
    get_filename_component(INKPOD_MSVC_COMPILER_DIR "${CMAKE_CXX_COMPILER}" DIRECTORY)
    get_filename_component(INKPOD_MSVC_HOST_DIR "${INKPOD_MSVC_COMPILER_DIR}" DIRECTORY)
    get_filename_component(INKPOD_MSVC_BIN_DIR "${INKPOD_MSVC_HOST_DIR}" DIRECTORY)
    get_filename_component(INKPOD_MSVC_VERSION_DIR "${INKPOD_MSVC_BIN_DIR}" DIRECTORY)
    get_filename_component(INKPOD_MSVC_VERSION "${INKPOD_MSVC_VERSION_DIR}" NAME)
    get_filename_component(INKPOD_MSVC_FAMILY_DIR "${INKPOD_MSVC_VERSION_DIR}" DIRECTORY)
    get_filename_component(INKPOD_MSVC_TOOLS_DIR "${INKPOD_MSVC_FAMILY_DIR}" DIRECTORY)
    get_filename_component(INKPOD_VC_DIR "${INKPOD_MSVC_TOOLS_DIR}" DIRECTORY)
    file(GLOB INKPOD_MSVC_CRT_DIRECTORIES
        "${INKPOD_VC_DIR}/Redist/MSVC/${INKPOD_MSVC_VERSION}/x64/Microsoft.VC*.CRT")
    list(LENGTH INKPOD_MSVC_CRT_DIRECTORIES INKPOD_MSVC_CRT_DIRECTORY_COUNT)
    if(NOT INKPOD_MSVC_CRT_DIRECTORY_COUNT EQUAL 1)
        message(FATAL_ERROR
            "Expected one x64 MSVC CRT redist directory for ${INKPOD_MSVC_VERSION}; "
            "found ${INKPOD_MSVC_CRT_DIRECTORY_COUNT}")
    endif()
    list(GET INKPOD_MSVC_CRT_DIRECTORIES 0 INKPOD_MSVC_CRT_DIRECTORY)
    file(GLOB INKPOD_MSVC_RUNTIME_DLLS "${INKPOD_MSVC_CRT_DIRECTORY}/*.dll")
    foreach(INKPOD_REQUIRED_RUNTIME IN ITEMS
            msvcp140.dll
            vcruntime140.dll
            vcruntime140_1.dll)
        if(NOT EXISTS "${INKPOD_MSVC_CRT_DIRECTORY}/${INKPOD_REQUIRED_RUNTIME}")
            message(FATAL_ERROR
                "MSVC CRT redist is missing ${INKPOD_REQUIRED_RUNTIME}: "
                "${INKPOD_MSVC_CRT_DIRECTORY}")
        endif()
    endforeach()
    set(INKPOD_MSIX_PATH
        "${CMAKE_BINARY_DIR}/package/inkpod-${PROJECT_VERSION}-x64.msix")
    add_custom_command(
        OUTPUT "${INKPOD_MSIX_PATH}"
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/build_msix.ps1"
                -Executable "$<TARGET_FILE:inkpod>"
                -Manifest "${CMAKE_SOURCE_DIR}/apps/windows/package/Package.appxmanifest"
                -AssetsDirectory "${CMAKE_SOURCE_DIR}/apps/windows/package/Assets"
                -License "${CMAKE_SOURCE_DIR}/LICENSE"
                -ThirdPartyNotices "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
                -RuntimeDirectory "${INKPOD_MSVC_CRT_DIRECTORY}"
                -OutputPath "${INKPOD_MSIX_PATH}"
        DEPENDS
            inkpod
            "${CMAKE_SOURCE_DIR}/tests/build_msix.ps1"
            "${CMAKE_SOURCE_DIR}/apps/windows/package/Package.appxmanifest"
            "${CMAKE_SOURCE_DIR}/LICENSE"
            "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
            ${INKPOD_MSIX_ASSETS}
            ${INKPOD_MSVC_RUNTIME_DLLS}
        COMMENT "Assembling unsigned inkpod MSIX"
        VERBATIM)
    add_custom_target(inkpod_msix ALL DEPENDS "${INKPOD_MSIX_PATH}")

    add_test(
        NAME m8_windows_msix_payload_smoke
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/verify_msix.ps1"
                -PackagePath "${INKPOD_MSIX_PATH}"
                -ExpectedVersion "${PROJECT_VERSION}.0")
    set_tests_properties(m8_windows_msix_payload_smoke PROPERTIES
        DEPENDS inkpod_windows_assets
        TIMEOUT 60)

    add_test(
        NAME m8_windows_msix_install_uninstall_smoke
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/msix_install_smoke.ps1"
                -PackagePath "${INKPOD_MSIX_PATH}"
                -ExpectedVersion "${PROJECT_VERSION}.0")
    set_tests_properties(m8_windows_msix_install_uninstall_smoke PROPERTIES
        DEPENDS m8_windows_msix_payload_smoke
        RUN_SERIAL TRUE
        TIMEOUT 180)
endif()
