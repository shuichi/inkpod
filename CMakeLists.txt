cmake_minimum_required(VERSION 3.25)

cmake_policy(SET CMP0091 NEW)
project(inkpod VERSION 0.1.0 LANGUAGES C CXX)

# MSVC 19.51 can emit UTF-8 /showIncludes text while CMake 4.4 decodes it
# through the active Japanese code page. Repair that detected prefix so Ninja
# records header dependencies instead of silently retaining stale objects.
if(MSVC AND CMAKE_GENERATOR MATCHES "^Ninja"
        AND CMAKE_CL_SHOWINCLUDES_PREFIX MATCHES "^繝")
    set(CMAKE_CL_SHOWINCLUDES_PREFIX "メモ: インクルード ファイル:  ")
endif()

set(INKPOD_MSIX_PROCESSOR_ARCHITECTURE x64)
if(WIN32)
    set(INKPOD_WINDOWS_ARCHITECTURE "x64" CACHE STRING
        "Windows target architecture (x64 or arm64)")
    set_property(CACHE INKPOD_WINDOWS_ARCHITECTURE PROPERTY STRINGS x64 arm64)
    string(TOLOWER
        "${INKPOD_WINDOWS_ARCHITECTURE}"
        INKPOD_WINDOWS_ARCHITECTURE_NORMALIZED)

    if(INKPOD_WINDOWS_ARCHITECTURE_NORMALIZED STREQUAL "x64")
        set(INKPOD_RUST_TARGET_TRIPLE x86_64-pc-windows-msvc)
        set(INKPOD_MSVC_REDIST_ARCHITECTURE x64)
        set(INKPOD_MSIX_PROCESSOR_ARCHITECTURE x64)
        set(INKPOD_WINDOWS_ARCHITECTURE_MACRO _M_X64)
        set(INKPOD_WINDOWS_PRESET_FAMILY windows-x64)
    elseif(INKPOD_WINDOWS_ARCHITECTURE_NORMALIZED STREQUAL "arm64")
        set(INKPOD_RUST_TARGET_TRIPLE aarch64-pc-windows-msvc)
        set(INKPOD_MSVC_REDIST_ARCHITECTURE arm64)
        set(INKPOD_MSIX_PROCESSOR_ARCHITECTURE arm64)
        set(INKPOD_WINDOWS_ARCHITECTURE_MACRO _M_ARM64)
        set(INKPOD_WINDOWS_PRESET_FAMILY windows-arm)
    else()
        message(FATAL_ERROR
            "INKPOD_WINDOWS_ARCHITECTURE must be 'x64' or 'arm64', not "
            "'${INKPOD_WINDOWS_ARCHITECTURE}'. Use a checked-in Windows preset.")
    endif()

    if(NOT CMAKE_SIZEOF_VOID_P EQUAL 8)
        message(FATAL_ERROR
            "Inkpod supports only 64-bit Windows targets, but CMake selected a "
            "32-bit compiler: ${CMAKE_CXX_COMPILER}. Initialize the matching MSVC "
            "developer environment, then rerun a ${INKPOD_WINDOWS_PRESET_FAMILY} "
            "preset with '--fresh'.")
    endif()

    include(CheckCXXSourceCompiles)
    set(INKPOD_ARCHITECTURE_PROBE_SOURCE
        "#if !defined(${INKPOD_WINDOWS_ARCHITECTURE_MACRO})\n#error Wrong MSVC target architecture\n#endif\nint main() { return 0; }")
    set(INKPOD_ARCHITECTURE_PROBE_RESULT
        "INKPOD_COMPILER_TARGET_IS_${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}")
    check_cxx_source_compiles(
        "${INKPOD_ARCHITECTURE_PROBE_SOURCE}"
        ${INKPOD_ARCHITECTURE_PROBE_RESULT})
    if(NOT ${INKPOD_ARCHITECTURE_PROBE_RESULT})
        message(FATAL_ERROR
            "The ${INKPOD_WINDOWS_PRESET_FAMILY} preset requires an "
            "${INKPOD_MSIX_PROCESSOR_ARCHITECTURE} MSVC target environment, but "
            "${CMAKE_CXX_COMPILER} targets another architecture. Initialize "
            "VsDevCmd with '-arch=${INKPOD_MSVC_REDIST_ARCHITECTURE}', then rerun "
            "with 'cmake --fresh --preset ${INKPOD_WINDOWS_PRESET_FAMILY}-debug' "
            "or '${INKPOD_WINDOWS_PRESET_FAMILY}-release'.")
    endif()
endif()

file(MAKE_DIRECTORY "${CMAKE_BINARY_DIR}/package")
set(INKPOD_PACKAGE_MANIFEST "${CMAKE_BINARY_DIR}/package/AppxManifest.xml")
configure_file(
    "${CMAKE_SOURCE_DIR}/apps/windows/package/Package.appxmanifest"
    "${INKPOD_PACKAGE_MANIFEST}"
    COPYONLY)
file(READ "${INKPOD_PACKAGE_MANIFEST}" INKPOD_PACKAGE_MANIFEST_CONTENT)
string(REGEX REPLACE
    "ProcessorArchitecture=\"[^\"]+\"([ \t\r\n]+)Version=\"[0-9]+\\.[0-9]+\\.[0-9]+\\.[0-9]+\""
    "ProcessorArchitecture=\"${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}\"\\1Version=\"${PROJECT_VERSION}.0\""
    INKPOD_PACKAGE_MANIFEST_CONTENT
    "${INKPOD_PACKAGE_MANIFEST_CONTENT}")
file(WRITE
    "${INKPOD_PACKAGE_MANIFEST}"
    "${INKPOD_PACKAGE_MANIFEST_CONTENT}")

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
set(INKPOD_RUST_OUTPUT_DIR "${INKPOD_CARGO_TARGET_DIR}")
set(INKPOD_CARGO_TARGET_ARGUMENT)
if(WIN32)
    set(INKPOD_RUST_OUTPUT_DIR
        "${INKPOD_CARGO_TARGET_DIR}/${INKPOD_RUST_TARGET_TRIPLE}")
    set(INKPOD_CARGO_TARGET_ARGUMENT --target "${INKPOD_RUST_TARGET_TRIPLE}")
endif()
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(INKPOD_RUST_PROFILE release)
    set(INKPOD_CARGO_PROFILE_ARGUMENT --release)
else()
    set(INKPOD_RUST_PROFILE debug)
    set(INKPOD_CARGO_PROFILE_ARGUMENT)
endif()

if(WIN32)
    set(INKPOD_RUST_STATICLIB "${INKPOD_RUST_OUTPUT_DIR}/${INKPOD_RUST_PROFILE}/inkpod_ffi.lib")
else()
    set(INKPOD_RUST_STATICLIB "${INKPOD_RUST_OUTPUT_DIR}/${INKPOD_RUST_PROFILE}/libinkpod_ffi.a")
endif()
set(INKPOD_RUST_RLIB "${INKPOD_RUST_OUTPUT_DIR}/${INKPOD_RUST_PROFILE}/libinkpod_ffi.rlib")
set(INKPOD_RUST_STAMP "${INKPOD_RUST_OUTPUT_DIR}/${INKPOD_RUST_PROFILE}/inkpod_ffi.cmake.stamp")

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
            "${CARGO_EXECUTABLE}" build --locked --package inkpod-ffi
            ${INKPOD_CARGO_TARGET_ARGUMENT} ${INKPOD_CARGO_PROFILE_ARGUMENT}
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
option(
    INKPOD_ENABLE_ELEVATED_MSIX_TESTS
    "Register the administrator-only Windows 11 MSIX install/uninstall smoke test"
    OFF)

add_test(
    NAME inkpod_windows_assets
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_PACKAGE_DIR=${CMAKE_SOURCE_DIR}/apps/windows/package"
            "-DINKPOD_PACKAGE_MANIFEST=${INKPOD_PACKAGE_MANIFEST}"
            "-DINKPOD_PACKAGE_ARCHITECTURE=${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}"
            "-DINKPOD_PROJECT_VERSION=${PROJECT_VERSION}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_assets.cmake")

add_test(
    NAME inkpod_windows_command_routes
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_command_routes.cmake")

add_test(
    NAME inkpod_windows_command_state_ownership
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_command_states.cmake")

add_test(
    NAME inkpod_windows_frontend_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_frontend_boundaries.cmake")

add_test(
    NAME inkpod_windows_command_context_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_command_context.cmake")

add_test(
    NAME inkpod_windows_application_host_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_application_host.cmake")

add_test(
    NAME inkpod_msvc_runtime_discovery
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            "-DINKPOD_TEST_ROOT=${CMAKE_BINARY_DIR}/tests/msvc-runtime-discovery"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_msvc_runtime_discovery.cmake")

add_library(inkpod_header_c11 OBJECT tests/header_c11.c)
target_include_directories(inkpod_header_c11 PRIVATE "${CMAKE_SOURCE_DIR}/include")

if(MSVC)
    function(inkpod_enable_strict_warnings target)
        target_compile_options(${target} PRIVATE /W4 /WX /permissive- /utf-8)
    endfunction()
    inkpod_enable_strict_warnings(inkpod_header_c11)
endif()

if(WIN32)
    add_executable(inkpod_windows_launch_options_tests
        apps/windows/app/launch_options.cpp
        apps/windows/app/launch_options.h
        tests/windows_launch_options.cpp)
    target_include_directories(inkpod_windows_launch_options_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows")
    target_compile_definitions(inkpod_windows_launch_options_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_launch_options_tests PRIVATE shell32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_launch_options_tests)
        target_compile_options(inkpod_windows_launch_options_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_launch_options
        COMMAND inkpod_windows_launch_options_tests)

    add_executable(inkpod_windows_command_state_tests
        apps/windows/ui/command_catalog.cpp
        apps/windows/ui/command_catalog.h
        apps/windows/ui/command_state.cpp
        apps/windows/ui/command_state.h
        apps/windows/ui/command_state_catalog.inc
        apps/windows/ui/tools/tool_state.cpp
        apps/windows/ui/tools/tool_state.h
        tests/windows_command_state.cpp)
    target_include_directories(inkpod_windows_command_state_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_SOURCE_DIR}/apps/windows/renderer"
        "${CMAKE_SOURCE_DIR}/include")
    target_compile_definitions(inkpod_windows_command_state_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_command_state_tests PRIVATE comctl32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_command_state_tests)
        target_compile_options(inkpod_windows_command_state_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_command_state
        COMMAND inkpod_windows_command_state_tests)

    add_executable(inkpod_windows_command_context_tests
        apps/windows/app/command_context.cpp
        apps/windows/app/command_context.h
        apps/windows/app/identity.h
        tests/windows_command_context.cpp)
    target_include_directories(inkpod_windows_command_context_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows")
    target_compile_definitions(inkpod_windows_command_context_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_command_context_tests)
        target_compile_options(inkpod_windows_command_context_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_command_context
        COMMAND inkpod_windows_command_context_tests)

    add_executable(inkpod_windows_workspace_layout_tests
        apps/windows/ui/workspace_layout.cpp
        apps/windows/ui/workspace_layout.h
        tests/windows_workspace_layout.cpp)
    target_include_directories(inkpod_windows_workspace_layout_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows")
    target_compile_definitions(inkpod_windows_workspace_layout_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_workspace_layout_tests PRIVATE advapi32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_workspace_layout_tests)
        target_compile_options(inkpod_windows_workspace_layout_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_workspace_layout
        COMMAND inkpod_windows_workspace_layout_tests)

    add_executable(inkpod_windows_owner_model_tests
        apps/windows/app/application_owner_graph.h
        apps/windows/app/document_session.cpp
        apps/windows/app/document_session.h
        apps/windows/app/frontend_state.h
        apps/windows/app/workspace_window.cpp
        apps/windows/app/workspace_window.h
        tests/windows_owner_model.cpp)
    target_include_directories(inkpod_windows_owner_model_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_SOURCE_DIR}/include")
    target_compile_definitions(inkpod_windows_owner_model_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_owner_model_tests PRIVATE comctl32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_owner_model_tests)
        target_compile_options(inkpod_windows_owner_model_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_owner_model
        COMMAND inkpod_windows_owner_model_tests)

    add_executable(inkpod WIN32
        apps/windows/app/application_host.cpp
        apps/windows/app/application_host.h
        apps/windows/app/application_owner_graph.h
        apps/windows/app/command_context.cpp
        apps/windows/app/command_context.h
        apps/windows/app/document_session.cpp
        apps/windows/app/document_session.h
        apps/windows/app/frontend_state.h
        apps/windows/app/identity.h
        apps/windows/app/workspace_window.cpp
        apps/windows/app/workspace_window.h
        apps/windows/app/app_smoke.cpp
        apps/windows/app/app_smoke.h
        apps/windows/app/application.cpp
        apps/windows/app/application.h
        apps/windows/app/clipboard_adapter.cpp
        apps/windows/app/com_runtime.cpp
        apps/windows/app/main.cpp
        apps/windows/app/core_engine.cpp
        apps/windows/app/document_shell.cpp
        apps/windows/app/launch_options.cpp
        apps/windows/app/launch_options.h
        apps/windows/app/app.rc
        apps/windows/renderer/canvas.cpp
        apps/windows/ui/dialogs/about_dialog.cpp
        apps/windows/ui/dialogs/basic_dialogs.cpp
        apps/windows/ui/dialogs/batch_dialog.cpp
        apps/windows/ui/dialogs/effects_dialogs.cpp
        apps/windows/ui/dialogs/layer_palette.cpp
        apps/windows/ui/dialogs/layer_palette.h
        apps/windows/ui/dialogs/tool_palette.cpp
        apps/windows/ui/dialogs/tool_palette.h
        apps/windows/ui/panes/document_panes.cpp
        apps/windows/ui/panes/color_panes.cpp
        apps/windows/ui/panes/color_dock_pane.cpp
        apps/windows/ui/panes/color_dock_pane.h
        apps/windows/ui/panes/tool_options_pane.cpp
        apps/windows/ui/panes/tool_options_pane.h
        apps/windows/ui/tools/view_controller.cpp
        apps/windows/ui/tools/fill_controller.cpp
        apps/windows/ui/tools/selection_controller.cpp
        apps/windows/ui/tools/floating_paste_controller.cpp
        apps/windows/ui/tools/tool_state.cpp
        apps/windows/ui/tools/vector_controller.cpp
        apps/windows/ui/command_state.cpp
        apps/windows/ui/command_state_catalog.inc
        apps/windows/ui/command_catalog.cpp
        apps/windows/ui/command_catalog.h
        apps/windows/ui/shortcut_controller.cpp
        apps/windows/ui/shortcut_controller.h
        apps/windows/ui/effects_controller.cpp
        apps/windows/ui/batch_controller.cpp
        apps/windows/ui/palette_window.cpp
        apps/windows/ui/palette_window.h
        apps/windows/ui/main_window.cpp
        apps/windows/ui/main_window_command_router.cpp
        apps/windows/ui/main_window_document_presenter.cpp
        apps/windows/ui/main_window_input_router.cpp
        apps/windows/ui/main_window_procedure.cpp
        apps/windows/ui/main_window_runtime.cpp
        apps/windows/ui/main_window_runtime.h
        apps/windows/ui/main_window_runtime_internal.h
        apps/windows/ui/main_window_status_presenter.cpp
        apps/windows/ui/main_window_status_presenter.h
        apps/windows/ui/workspace_layout.cpp
        apps/windows/ui/workspace_layout.h
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
        advapi32
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
    include("${CMAKE_SOURCE_DIR}/cmake/InkpodMsvcRuntime.cmake")
    inkpod_find_msvc_crt_directory(INKPOD_MSVC_CRT_DIRECTORY
        VC_DIRECTORY "${INKPOD_VC_DIR}"
        TOOLSET_VERSION "${INKPOD_MSVC_VERSION}"
        ARCHITECTURE "${INKPOD_MSVC_REDIST_ARCHITECTURE}")
    message(STATUS
        "Using ${INKPOD_MSVC_REDIST_ARCHITECTURE} MSVC CRT: "
        "${INKPOD_MSVC_CRT_DIRECTORY}")
    file(GLOB INKPOD_MSVC_RUNTIME_DLLS "${INKPOD_MSVC_CRT_DIRECTORY}/*.dll")
    set(INKPOD_MSIX_PATH
        "${CMAKE_BINARY_DIR}/package/inkpod-${PROJECT_VERSION}-${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}.msix")
    add_custom_command(
        OUTPUT "${INKPOD_MSIX_PATH}"
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/build_msix.ps1"
                -Executable "$<TARGET_FILE:inkpod>"
                -Manifest "${INKPOD_PACKAGE_MANIFEST}"
                -AssetsDirectory "${CMAKE_SOURCE_DIR}/apps/windows/package/Assets"
                -License "${CMAKE_SOURCE_DIR}/LICENSE"
                -ThirdPartyNotices "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
                -RuntimeDirectory "${INKPOD_MSVC_CRT_DIRECTORY}"
                -OutputPath "${INKPOD_MSIX_PATH}"
        DEPENDS
            inkpod
            "${CMAKE_SOURCE_DIR}/tests/build_msix.ps1"
            "${INKPOD_PACKAGE_MANIFEST}"
            "${CMAKE_SOURCE_DIR}/LICENSE"
            "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
            ${INKPOD_MSIX_ASSETS}
            ${INKPOD_MSVC_RUNTIME_DLLS}
        COMMENT "Assembling unsigned inkpod MSIX"
        VERBATIM)
    add_custom_target(inkpod_msix ALL DEPENDS "${INKPOD_MSIX_PATH}")

    add_test(
        NAME inkpod_windows_msix_payload_smoke
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/verify_msix.ps1"
                -PackagePath "${INKPOD_MSIX_PATH}"
                -ExpectedVersion "${PROJECT_VERSION}.0"
                -ExpectedArchitecture "${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}")
    set_tests_properties(inkpod_windows_msix_payload_smoke PROPERTIES
        DEPENDS inkpod_windows_assets
        TIMEOUT 60)

    if(INKPOD_ENABLE_ELEVATED_MSIX_TESTS)
        add_test(
            NAME inkpod_windows_msix_install_uninstall_smoke
            COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                    -File "${CMAKE_SOURCE_DIR}/tests/msix_install_smoke.ps1"
                    -PackagePath "${INKPOD_MSIX_PATH}"
                    -ExpectedVersion "${PROJECT_VERSION}.0")
        set_tests_properties(inkpod_windows_msix_install_uninstall_smoke PROPERTIES
            DEPENDS inkpod_windows_msix_payload_smoke
            RUN_SERIAL TRUE
            TIMEOUT 180)
    endif()
endif()
