cmake_minimum_required(VERSION 3.25)

cmake_policy(SET CMP0091 NEW)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded")
project(inkpod VERSION 0.2.4 LANGUAGES C CXX)

set(INKPOD_BUILD_NUMBER "0" CACHE STRING
    "Numeric application build number (0-65535)")
if(NOT INKPOD_BUILD_NUMBER MATCHES "^[0-9]+$")
    message(FATAL_ERROR
        "INKPOD_BUILD_NUMBER must contain only decimal digits, not "
        "'${INKPOD_BUILD_NUMBER}'")
endif()
string(LENGTH "${INKPOD_BUILD_NUMBER}" INKPOD_BUILD_NUMBER_LENGTH)
if(INKPOD_BUILD_NUMBER_LENGTH GREATER 5 OR INKPOD_BUILD_NUMBER GREATER 65535)
    message(FATAL_ERROR
        "INKPOD_BUILD_NUMBER must be between 0 and 65535, not "
        "'${INKPOD_BUILD_NUMBER}'")
endif()
math(EXPR INKPOD_BUILD_NUMBER "${INKPOD_BUILD_NUMBER}")
set(INKPOD_FOUR_PART_VERSION "${PROJECT_VERSION}.${INKPOD_BUILD_NUMBER}")

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
        set(INKPOD_MSVC_TARGET_ARCHITECTURE x64)
        set(INKPOD_MSIX_PROCESSOR_ARCHITECTURE x64)
        set(INKPOD_PORTABLE_ARCHITECTURE x64)
        set(INKPOD_WINDOWS_ARCHITECTURE_MACRO _M_X64)
        set(INKPOD_WINDOWS_PRESET_FAMILY windows-x64)
    elseif(INKPOD_WINDOWS_ARCHITECTURE_NORMALIZED STREQUAL "arm64")
        set(INKPOD_RUST_TARGET_TRIPLE aarch64-pc-windows-msvc)
        set(INKPOD_MSVC_TARGET_ARCHITECTURE arm64)
        set(INKPOD_MSIX_PROCESSOR_ARCHITECTURE arm64)
        set(INKPOD_PORTABLE_ARCHITECTURE arm)
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
            "VsDevCmd with '-arch=${INKPOD_MSVC_TARGET_ARCHITECTURE}', then rerun "
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
    "ProcessorArchitecture=\"${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}\"\\1Version=\"${INKPOD_FOUR_PART_VERSION}\""
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
set(INKPOD_CARGO_ENVIRONMENT)
if(WIN32)
    set(INKPOD_RUST_OUTPUT_DIR
        "${INKPOD_CARGO_TARGET_DIR}/${INKPOD_RUST_TARGET_TRIPLE}")
    set(INKPOD_CARGO_TARGET_ARGUMENT --target "${INKPOD_RUST_TARGET_TRIPLE}")
    set(INKPOD_CARGO_ENVIRONMENT
        "CARGO_ENCODED_RUSTFLAGS=-Ctarget-feature=+crt-static")
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
    "${CMAKE_SOURCE_DIR}/.cargo/config.toml"
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
            ${INKPOD_CARGO_ENVIRONMENT}
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
            "-DINKPOD_BUILD_NUMBER=${INKPOD_BUILD_NUMBER}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_assets.cmake")

add_test(
    NAME inkpod_windows_fluent_icons
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_fluent_icons.cmake")

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
    NAME inkpod_acknowledgements_dependency_versions
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_acknowledgements.cmake")

add_test(
    NAME inkpod_windows_frontend_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_frontend_boundaries.cmake")

add_test(
    NAME inkpod_windows_modal_dialog_positioning
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_modal_dialog_positioning.cmake")

add_test(
    NAME inkpod_windows_activation_recovery_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_activation_recovery.cmake")

add_test(
    NAME inkpod_windows_command_context_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_command_context.cmake")

add_test(
    NAME inkpod_windows_filter_preview_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_filter_preview.cmake")

add_test(
    NAME inkpod_windows_tab_drag_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_tab_drag.cmake")

add_test(
    NAME inkpod_windows_application_host_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_application_host.cmake")

add_test(
    NAME inkpod_windows_core_host_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_core_host.cmake")

add_test(
    NAME inkpod_windows_renderer_host_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_renderer_host.cmake")

add_test(
    NAME inkpod_windows_dock_host_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_dock_host.cmake")

add_test(
    NAME inkpod_windows_workspace_persistence_boundaries
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}"
            -P "${CMAKE_SOURCE_DIR}/tests/verify_windows_workspace_persistence.cmake")

add_library(inkpod_header_c11 OBJECT tests/header_c11.c)
target_include_directories(inkpod_header_c11 PRIVATE "${CMAKE_SOURCE_DIR}/include")

if(MSVC)
    function(inkpod_enable_strict_warnings target)
        target_compile_options(${target} PRIVATE /W4 /WX /permissive- /utf-8)
    endfunction()
    inkpod_enable_strict_warnings(inkpod_header_c11)
endif()

if(WIN32)
    set_property(SOURCE apps/windows/app/app.rc APPEND PROPERTY
        OBJECT_DEPENDS "${CMAKE_SOURCE_DIR}/html/manual.html")
    set_property(SOURCE apps/windows/app/app.rc APPEND PROPERTY
        OBJECT_DEPENDS "${CMAKE_SOURCE_DIR}/html/file_format.html")
    set_property(SOURCE apps/windows/app/app.rc APPEND PROPERTY
        OBJECT_DEPENDS "${CMAKE_SOURCE_DIR}/html/acknowledgements.html")
    set_property(SOURCE apps/windows/app/app.rc APPEND PROPERTY OBJECT_DEPENDS
        "${CMAKE_SOURCE_DIR}/apps/windows/app/app_common.rc")
    set_property(SOURCE apps/windows/app/app.rc APPEND PROPERTY OBJECT_DEPENDS
        "${CMAKE_SOURCE_DIR}/apps/windows/ui/icons/fluent/fluent_icon_masks.bin")
    set_property(SOURCE apps/windows/app/app.rc APPEND PROPERTY OBJECT_DEPENDS
        "${CMAKE_SOURCE_DIR}/apps/windows/app/app_ui_ja.generated.rc")
    set_property(SOURCE apps/windows/app/app.rc APPEND PROPERTY OBJECT_DEPENDS
        "${CMAKE_SOURCE_DIR}/apps/windows/app/app_ui_en.generated.rc")
    set_property(SOURCE tests/windows_embedded_manual.rc APPEND PROPERTY
        OBJECT_DEPENDS "${CMAKE_SOURCE_DIR}/html/manual.html")
    set_property(SOURCE tests/windows_embedded_manual.rc APPEND PROPERTY
        OBJECT_DEPENDS "${CMAKE_SOURCE_DIR}/html/file_format.html")
    set_property(SOURCE tests/windows_embedded_manual.rc APPEND PROPERTY
        OBJECT_DEPENDS "${CMAKE_SOURCE_DIR}/html/acknowledgements.html")

    add_executable(inkpod_windows_activation_tests
        apps/windows/app/activation.cpp
        apps/windows/app/activation.h
        tests/windows_activation.cpp)
    target_include_directories(inkpod_windows_activation_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows")
    target_compile_definitions(inkpod_windows_activation_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_activation_tests PRIVATE
        advapi32
        user32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_activation_tests)
        target_compile_options(inkpod_windows_activation_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_activation
        COMMAND inkpod_windows_activation_tests)

    add_executable(inkpod_windows_session_recovery_tests
        apps/windows/app/document_identity.h
        apps/windows/app/identity.h
        apps/windows/app/session_recovery.cpp
        apps/windows/app/session_recovery.h
        tests/windows_session_recovery.cpp)
    target_include_directories(inkpod_windows_session_recovery_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows")
    target_compile_definitions(inkpod_windows_session_recovery_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_session_recovery_tests PRIVATE
        advapi32
        ole32
        shell32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_session_recovery_tests)
        target_compile_options(inkpod_windows_session_recovery_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_session_recovery
        COMMAND inkpod_windows_session_recovery_tests)

    add_executable(inkpod_windows_embedded_manual_tests
        apps/windows/app/embedded_manual.cpp
        apps/windows/app/embedded_manual.h
        tests/windows_embedded_manual.cpp
        tests/windows_embedded_manual.rc)
    target_include_directories(inkpod_windows_embedded_manual_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_BINARY_DIR}/generated")
    target_compile_definitions(inkpod_windows_embedded_manual_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_embedded_manual_tests PRIVATE
        ole32
        shell32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_embedded_manual_tests)
        target_compile_options(inkpod_windows_embedded_manual_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_embedded_manual
        COMMAND inkpod_windows_embedded_manual_tests)

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
        apps/windows/ui/localization.cpp
        apps/windows/ui/localization.h
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
    target_link_libraries(inkpod_windows_command_state_tests PRIVATE
        advapi32
        comctl32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_command_state_tests)
        target_compile_options(inkpod_windows_command_state_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_command_state
        COMMAND inkpod_windows_command_state_tests)

    add_executable(inkpod_windows_localization_tests
        apps/windows/ui/localization.cpp
        apps/windows/ui/localization.h
        apps/windows/ui/history_presentation.h
        apps/windows/ui/dialogs/layer_palette_badge_layout.h
        apps/windows/ui/dialogs/layer_palette_status_layout.h
        apps/windows/ui/ui_resources.cpp
        apps/windows/ui/ui_resources.h
        tests/windows_localization.cpp
        apps/windows/app/app.rc)
    target_include_directories(inkpod_windows_localization_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_SOURCE_DIR}/include"
        "${CMAKE_BINARY_DIR}/generated")
    target_compile_definitions(inkpod_windows_localization_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_localization_tests PRIVATE
        advapi32
        comctl32
        user32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_localization_tests)
        target_compile_options(inkpod_windows_localization_tests PRIVATE /EHsc)
        target_link_options(inkpod_windows_localization_tests PRIVATE /MANIFEST:NO)
    endif()
    add_test(
        NAME inkpod_windows_localization
        COMMAND inkpod_windows_localization_tests)
    add_test(
        NAME inkpod_windows_localization_catalog
        COMMAND ${CMAKE_COMMAND}
            -DINKPOD_SOURCE_DIR=${CMAKE_SOURCE_DIR}
            -P ${CMAKE_SOURCE_DIR}/tests/verify_windows_localization_catalog.cmake)

    add_executable(inkpod_windows_command_context_tests
        apps/windows/app/command_context.cpp
        apps/windows/app/command_context.h
        apps/windows/app/identity.h
        apps/windows/app/tab_drag.cpp
        apps/windows/app/tab_drag.h
        apps/windows/app/pane_target.cpp
        apps/windows/app/pane_target.h
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
        apps/windows/ui/localization.cpp
        apps/windows/ui/localization.h
        apps/windows/ui/dock_layout.cpp
        apps/windows/ui/dock_layout.h
        apps/windows/ui/right_tool_tabs.cpp
        apps/windows/ui/right_tool_tabs.h
        apps/windows/ui/workspace_layout.cpp
        apps/windows/ui/workspace_layout.h
        tests/windows_workspace_layout.cpp)
    target_include_directories(inkpod_windows_workspace_layout_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows")
    target_compile_definitions(inkpod_windows_workspace_layout_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_workspace_layout_tests PRIVATE
        advapi32
        shcore)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_workspace_layout_tests)
        target_compile_options(inkpod_windows_workspace_layout_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_workspace_layout
        COMMAND inkpod_windows_workspace_layout_tests)

    add_executable(inkpod_windows_modal_dialog_position_tests
        apps/windows/ui/dialogs/modal_dialog_position.cpp
        apps/windows/ui/dialogs/modal_dialog_position.h
        tests/windows_modal_dialog_position.cpp)
    target_include_directories(inkpod_windows_modal_dialog_position_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows")
    target_compile_definitions(inkpod_windows_modal_dialog_position_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_modal_dialog_position_tests PRIVATE user32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_modal_dialog_position_tests)
        target_compile_options(inkpod_windows_modal_dialog_position_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_modal_dialog_position
        COMMAND inkpod_windows_modal_dialog_position_tests)

    add_executable(inkpod_windows_owner_model_tests
        apps/windows/app/application_owner_graph.h
        apps/windows/app/editor_area.cpp
        apps/windows/app/editor_area.h
        apps/windows/app/document_identity.cpp
        apps/windows/app/document_identity.h
        apps/windows/app/document_session.cpp
        apps/windows/app/document_session.h
        apps/windows/app/recent_documents.cpp
        apps/windows/app/recent_documents.h
        apps/windows/app/frontend_state.h
        apps/windows/app/workspace_window.cpp
        apps/windows/app/workspace_window.h
        apps/windows/ui/dock_layout.cpp
        apps/windows/ui/dock_layout.h
        apps/windows/ui/dock_host.cpp
        apps/windows/ui/dock_host.h
        apps/windows/ui/right_tool_tabs.cpp
        apps/windows/ui/right_tool_tabs.h
        apps/windows/ui/localization.cpp
        apps/windows/ui/localization.h
        apps/windows/ui/history_presentation.h
        apps/windows/ui/ui_resources.cpp
        apps/windows/ui/ui_resources.h
        apps/windows/ui/thumbnail_cache.cpp
        apps/windows/ui/thumbnail_cache.h
        tests/windows_owner_model.cpp)
    target_include_directories(inkpod_windows_owner_model_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_SOURCE_DIR}/include")
    target_compile_definitions(inkpod_windows_owner_model_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_owner_model_tests PRIVATE
        comctl32
        advapi32
        ole32
        oleacc)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_owner_model_tests)
        target_compile_options(inkpod_windows_owner_model_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_owner_model
        COMMAND inkpod_windows_owner_model_tests)

    add_executable(inkpod_windows_core_host_tests
        apps/windows/app/core_host.cpp
        apps/windows/app/core_host.h
        apps/windows/app/command_context.h
        apps/windows/app/identity.h
        tests/windows_core_host.cpp)
    target_include_directories(inkpod_windows_core_host_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_SOURCE_DIR}/apps/windows/renderer"
        "${CMAKE_SOURCE_DIR}/include")
    target_compile_definitions(inkpod_windows_core_host_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_core_host_tests PRIVATE
        inkpod_ffi_static
        user32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_core_host_tests)
        target_compile_options(inkpod_windows_core_host_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_core_host
        COMMAND inkpod_windows_core_host_tests)

    add_executable(inkpod_windows_renderer_host_tests
        apps/windows/app/identity.h
        apps/windows/renderer/canvas.cpp
        apps/windows/renderer/canvas.h
        apps/windows/renderer/renderer_host.h
        tests/windows_renderer_host.cpp)
    target_include_directories(inkpod_windows_renderer_host_tests PRIVATE
        "${CMAKE_SOURCE_DIR}/apps/windows"
        "${CMAKE_SOURCE_DIR}/apps/windows/renderer"
        "${CMAKE_SOURCE_DIR}/include")
    target_compile_definitions(inkpod_windows_renderer_host_tests PRIVATE
        UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
    target_link_libraries(inkpod_windows_renderer_host_tests PRIVATE
        inkpod_ffi_static
        d2d1
        d3d11
        dwrite
        dxguid
        dxgi
        user32)
    if(MSVC)
        inkpod_enable_strict_warnings(inkpod_windows_renderer_host_tests)
        target_compile_options(inkpod_windows_renderer_host_tests PRIVATE /EHsc)
    endif()
    add_test(
        NAME inkpod_windows_renderer_host
        COMMAND inkpod_windows_renderer_host_tests)

    add_executable(inkpod WIN32
        apps/windows/app/activation.cpp
        apps/windows/app/activation.h
        apps/windows/app/application_host.cpp
        apps/windows/app/application_host.h
        apps/windows/app/application_owner_graph.h
        apps/windows/app/editor_area.cpp
        apps/windows/app/editor_area.h
        apps/windows/app/command_context.cpp
        apps/windows/app/command_context.h
        apps/windows/app/document_identity.cpp
        apps/windows/app/document_identity.h
        apps/windows/app/document_session.cpp
        apps/windows/app/document_session.h
        apps/windows/app/embedded_manual.cpp
        apps/windows/app/embedded_manual.h
        apps/windows/app/recent_documents.cpp
        apps/windows/app/recent_documents.h
        apps/windows/app/session_recovery.cpp
        apps/windows/app/session_recovery.h
        apps/windows/app/frontend_state.h
        apps/windows/app/identity.h
        apps/windows/app/pane_target.cpp
        apps/windows/app/pane_target.h
        apps/windows/app/tab_drag.cpp
        apps/windows/app/tab_drag.h
        apps/windows/app/workspace_window.cpp
        apps/windows/app/workspace_window.h
        apps/windows/app/app_smoke.cpp
        apps/windows/app/app_smoke.h
        apps/windows/app/application.cpp
        apps/windows/app/application.h
        apps/windows/app/clipboard_adapter.cpp
        apps/windows/app/com_runtime.cpp
        apps/windows/app/main.cpp
        apps/windows/app/core_host.cpp
        apps/windows/app/document_shell.cpp
        apps/windows/app/launch_options.cpp
        apps/windows/app/launch_options.h
        apps/windows/app/app.rc
        apps/windows/renderer/canvas.cpp
        apps/windows/renderer/renderer_host.h
        apps/windows/ui/dialogs/about_dialog.cpp
        apps/windows/ui/dialogs/basic_dialogs.cpp
        apps/windows/ui/dialogs/batch_dialog.cpp
        apps/windows/ui/dialogs/effects_dialogs.cpp
        apps/windows/ui/dialogs/history_visualization_dialog.cpp
        apps/windows/ui/dialogs/history_visualization_dialog.h
        apps/windows/ui/dialogs/layer_palette.cpp
        apps/windows/ui/dialogs/layer_palette.h
        apps/windows/ui/dialogs/layer_palette_badge_layout.h
        apps/windows/ui/dialogs/layer_palette_status_layout.h
        apps/windows/ui/dialogs/modal_dialog_position.cpp
        apps/windows/ui/dialogs/modal_dialog_position.h
        apps/windows/ui/dialogs/tool_palette.cpp
        apps/windows/ui/dialogs/tool_palette.h
        apps/windows/ui/icons/fluent_icons.cpp
        apps/windows/ui/icons/fluent_icons.h
        apps/windows/ui/panes/document_panes.cpp
        apps/windows/ui/panes/light_table_pane.cpp
        apps/windows/ui/panes/light_table_pane.h
        apps/windows/ui/panes/locator_pane.cpp
        apps/windows/ui/panes/locator_pane.h
        apps/windows/ui/panes/pane_dialog_layout.h
        apps/windows/ui/panes/sequence_pane.cpp
        apps/windows/ui/panes/sequence_pane.h
        apps/windows/ui/panes/subpalette_pane.cpp
        apps/windows/ui/panes/subpalette_pane.h
        apps/windows/ui/panes/color_panes.cpp
        apps/windows/ui/panes/color_dock_pane.cpp
        apps/windows/ui/panes/color_dock_pane.h
        apps/windows/ui/panes/tool_options_pane.cpp
        apps/windows/ui/panes/tool_options_pane.h
        apps/windows/ui/tools/view_controller.cpp
        apps/windows/ui/tools/fill_controller.cpp
        apps/windows/ui/tools/color_replace_controller.cpp
        apps/windows/ui/tools/color_replace_controller.h
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
        apps/windows/ui/localization.cpp
        apps/windows/ui/localization.h
        apps/windows/ui/ui_resources.cpp
        apps/windows/ui/ui_resources.h
        apps/windows/ui/effects_controller.cpp
        apps/windows/ui/batch_controller.cpp
        apps/windows/ui/dock_layout.cpp
        apps/windows/ui/dock_layout.h
        apps/windows/ui/dock_host.cpp
        apps/windows/ui/dock_host.h
        apps/windows/ui/right_tool_tabs.cpp
        apps/windows/ui/right_tool_tabs.h
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
        apps/windows/ui/thumbnail_cache.cpp
        apps/windows/ui/thumbnail_cache.h
        apps/windows/ui/tab_drag.cpp
        apps/windows/ui/tab_drag.h
        apps/windows/ui/workspace_layout.cpp
        apps/windows/ui/workspace_layout.h
        tests/abi_smoke.cpp
        tests/portable_smoke.cpp
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
        dwrite
        dxguid
        dxgi
        ole32
        oleacc
        oleaut32
        msimg32
        uiautomationcore
        advapi32
        shcore
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
    add_test(
        NAME inkpod_windows_smoke
        COMMAND inkpod --smoke-test --smoke-test-language=en-US)
    add_test(
        NAME inkpod_windows_smoke_japanese
        COMMAND inkpod --smoke-test --smoke-test-language=ja-JP)
    add_test(
        NAME inkpod_windows_release_automation_dry_run
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/verify_windows_release_automation.ps1"
                -RepositoryRoot "${CMAKE_SOURCE_DIR}")
    set_tests_properties(
        inkpod_windows_release_automation_dry_run
        PROPERTIES TIMEOUT 30)
    if(INKPOD_WINDOWS_ARCHITECTURE STREQUAL "x64")
        add_test(
            NAME inkpod_windows_local_build_number_dry_run
            COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                    -File "${CMAKE_SOURCE_DIR}/scripts/build-windows-x64.ps1"
                    -DryRun)
        set_tests_properties(inkpod_windows_local_build_number_dry_run PROPERTIES
            TIMEOUT 30)
    elseif(INKPOD_WINDOWS_ARCHITECTURE STREQUAL "arm64")
        add_test(
            NAME inkpod_windows_local_build_number_dry_run
            COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                    -File "${CMAKE_SOURCE_DIR}/scripts/build-windows-arm64.ps1"
                    -DryRun)
        set_tests_properties(inkpod_windows_local_build_number_dry_run PROPERTIES
            TIMEOUT 30)
    endif()

    file(GLOB INKPOD_MSIX_ASSETS CONFIGURE_DEPENDS
        "${CMAKE_SOURCE_DIR}/apps/windows/package/Assets/*.png")
    if(NOT MSVC)
        message(FATAL_ERROR "inkpod Windows packaging requires the supported MSVC toolchain")
    endif()
    find_program(
        INKPOD_DUMPBIN_EXECUTABLE
        NAMES dumpbin.exe
        DOC "Path to the MSVC COFF/PE dependency inspector"
        REQUIRED)

    set(INKPOD_STATIC_CRT_STAMP
        "${CMAKE_BINARY_DIR}/package/inkpod-static-crt.cmake.stamp")
    add_custom_command(
        OUTPUT "${INKPOD_STATIC_CRT_STAMP}"
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/verify_static_msvc_runtime.ps1"
                -Executable "$<TARGET_FILE:inkpod>"
                -Dumpbin "${INKPOD_DUMPBIN_EXECUTABLE}"
        COMMAND "${CMAKE_COMMAND}" -E touch "${INKPOD_STATIC_CRT_STAMP}"
        DEPENDS
            inkpod
            "${CMAKE_SOURCE_DIR}/tests/verify_static_msvc_runtime.ps1"
        COMMENT "Verifying static MSVC CRT linkage"
        VERBATIM)
    add_custom_target(inkpod_static_crt ALL DEPENDS "${INKPOD_STATIC_CRT_STAMP}")

    add_test(
        NAME inkpod_windows_static_crt
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/verify_static_msvc_runtime.ps1"
                -Executable "$<TARGET_FILE:inkpod>"
                -Dumpbin "${INKPOD_DUMPBIN_EXECUTABLE}")
    set_tests_properties(inkpod_windows_static_crt PROPERTIES TIMEOUT 30)

    set(INKPOD_PORTABLE_README
        "${CMAKE_SOURCE_DIR}/apps/windows/package/README.portable.txt")
    set(INKPOD_PORTABLE_ZIP_PATH
        "${CMAKE_BINARY_DIR}/package/Inkpod-${PROJECT_VERSION}-windows-${INKPOD_PORTABLE_ARCHITECTURE}.zip")
    add_custom_command(
        OUTPUT "${INKPOD_PORTABLE_ZIP_PATH}"
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/scripts/package-windows-portable.ps1"
                -Executable "$<TARGET_FILE:inkpod>"
                -Readme "${INKPOD_PORTABLE_README}"
                -License "${CMAKE_SOURCE_DIR}/LICENSE"
                -ThirdPartyNotices "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
                -Version "${PROJECT_VERSION}"
                -Architecture "${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}"
                -OutputPath "${INKPOD_PORTABLE_ZIP_PATH}"
        DEPENDS
            "${INKPOD_STATIC_CRT_STAMP}"
            "${CMAKE_SOURCE_DIR}/scripts/package-windows-portable.ps1"
            "${INKPOD_PORTABLE_README}"
            "${CMAKE_SOURCE_DIR}/LICENSE"
            "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
        COMMENT "Assembling inkpod portable ZIP"
        VERBATIM)
    add_custom_target(inkpod_portable_zip ALL DEPENDS "${INKPOD_PORTABLE_ZIP_PATH}")

    set(INKPOD_MSIX_PATH
        "${CMAKE_BINARY_DIR}/package/inkpod-${INKPOD_FOUR_PART_VERSION}-${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}.msix")
    add_custom_command(
        OUTPUT "${INKPOD_MSIX_PATH}"
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/build_msix.ps1"
                -Executable "$<TARGET_FILE:inkpod>"
                -Manifest "${INKPOD_PACKAGE_MANIFEST}"
                -AssetsDirectory "${CMAKE_SOURCE_DIR}/apps/windows/package/Assets"
                -License "${CMAKE_SOURCE_DIR}/LICENSE"
                -ThirdPartyNotices "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
                -OutputPath "${INKPOD_MSIX_PATH}"
        DEPENDS
            "${INKPOD_STATIC_CRT_STAMP}"
            "${CMAKE_SOURCE_DIR}/tests/build_msix.ps1"
            "${INKPOD_PACKAGE_MANIFEST}"
            "${CMAKE_SOURCE_DIR}/LICENSE"
            "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
            ${INKPOD_MSIX_ASSETS}
        COMMENT "Assembling unsigned inkpod MSIX"
        VERBATIM)
    add_custom_target(inkpod_msix ALL DEPENDS "${INKPOD_MSIX_PATH}")

    add_test(
        NAME inkpod_windows_msix_payload_smoke
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/verify_msix.ps1"
                -PackagePath "${INKPOD_MSIX_PATH}"
                -ExpectedVersion "${INKPOD_FOUR_PART_VERSION}"
                -ExpectedArchitecture "${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}")
    set_tests_properties(inkpod_windows_msix_payload_smoke PROPERTIES
        DEPENDS "inkpod_windows_assets;inkpod_windows_static_crt"
        TIMEOUT 60)

    set(INKPOD_PORTABLE_TEST_ARGUMENTS)
    if(INKPOD_WINDOWS_ARCHITECTURE STREQUAL "x64")
        set(INKPOD_PORTABLE_TEST_ARGUMENTS -RunPortableSmoke)
    endif()
    add_test(
        NAME inkpod_windows_portable_zip_payload_smoke
        COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_SOURCE_DIR}/tests/verify_portable_zip.ps1"
                -PackagePath "${INKPOD_PORTABLE_ZIP_PATH}"
                -Executable "$<TARGET_FILE:inkpod>"
                -Readme "${INKPOD_PORTABLE_README}"
                -License "${CMAKE_SOURCE_DIR}/LICENSE"
                -ThirdPartyNotices "${CMAKE_SOURCE_DIR}/docs/third-party-notices.md"
                -ExpectedVersion "${PROJECT_VERSION}"
                -ExpectedArchitecture "${INKPOD_MSIX_PROCESSOR_ARCHITECTURE}"
                ${INKPOD_PORTABLE_TEST_ARGUMENTS})
    set_tests_properties(inkpod_windows_portable_zip_payload_smoke PROPERTIES
        DEPENDS inkpod_windows_static_crt
        TIMEOUT 60)

    if(INKPOD_ENABLE_ELEVATED_MSIX_TESTS)
        add_test(
            NAME inkpod_windows_msix_install_uninstall_smoke
            COMMAND powershell.exe -NoProfile -ExecutionPolicy Bypass
                    -File "${CMAKE_SOURCE_DIR}/tests/msix_install_smoke.ps1"
                    -PackagePath "${INKPOD_MSIX_PATH}"
                    -ExpectedVersion "${INKPOD_FOUR_PART_VERSION}")
        set_tests_properties(inkpod_windows_msix_install_uninstall_smoke PROPERTIES
            DEPENDS inkpod_windows_msix_payload_smoke
            RUN_SERIAL TRUE
            TIMEOUT 180)
    endif()
endif()
