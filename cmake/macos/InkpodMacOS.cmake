find_package(Python3 3.9 REQUIRED COMPONENTS Interpreter)
find_program(INKPOD_XCODEBUILD_EXECUTABLE NAMES xcodebuild REQUIRED)
find_program(INKPOD_XCRUN_EXECUTABLE NAMES xcrun REQUIRED)
find_program(INKPOD_FILE_EXECUTABLE NAMES file REQUIRED)

set(INKPOD_MACOS_PROJECT
    "${CMAKE_SOURCE_DIR}/apps/macos/Inkpod.xcodeproj")
set(INKPOD_MACOS_SCHEME InkpodCoreBridge)
set(INKPOD_MACOS_DERIVED_DATA
    "${CMAKE_BINARY_DIR}/xcode-derived")
set(INKPOD_MACOS_GENERATED_XCCONFIG
    "${CMAKE_BINARY_DIR}/generated/InkpodGenerated.xcconfig")
set(INKPOD_MACOS_PARITY_MANIFEST
    "${CMAKE_SOURCE_DIR}/tests/macos/macos-command-parity.json")
set(INKPOD_MACOS_PARITY_VERIFIER
    "${CMAKE_SOURCE_DIR}/tests/macos/verify_command_parity.py")
set(INKPOD_MACOS_CORE_HOST_VERIFIER
    "${CMAKE_SOURCE_DIR}/tests/macos/verify_core_host_contract.py")
set(INKPOD_MACOS_LOCALIZATION_VERIFIER
    "${CMAKE_SOURCE_DIR}/tests/macos/verify_localization_catalog.py")

set(INKPOD_XCODE_RUST_STATICLIB "${INKPOD_RUST_STATICLIB}")
configure_file(
    "${CMAKE_SOURCE_DIR}/cmake/macos/InkpodGenerated.xcconfig.in"
    "${INKPOD_MACOS_GENERATED_XCCONFIG}"
    @ONLY)

add_library(inkpod_macos_header_c11 OBJECT
    "${CMAKE_SOURCE_DIR}/tests/macos/header_c11.c")
target_include_directories(inkpod_macos_header_c11 PRIVATE
    "${CMAKE_SOURCE_DIR}/apps/macos/CoreBridge/C/include"
    "${CMAKE_SOURCE_DIR}/include")
target_compile_options(inkpod_macos_header_c11 PRIVATE
    -Wall -Wextra -Werror -pedantic)

add_library(inkpod_macos_header_cxx20 OBJECT
    "${CMAKE_SOURCE_DIR}/tests/macos/header_cxx20.cpp")
target_include_directories(inkpod_macos_header_cxx20 PRIVATE
    "${CMAKE_SOURCE_DIR}/apps/macos/CoreBridge/C/include"
    "${CMAKE_SOURCE_DIR}/include")
target_compile_options(inkpod_macos_header_cxx20 PRIVATE
    -Wall -Wextra -Werror -pedantic)

add_custom_target(inkpod_macos_check
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_XCODEBUILD=${INKPOD_XCODEBUILD_EXECUTABLE}"
            "-DINKPOD_XCODE_PROJECT=${INKPOD_MACOS_PROJECT}"
            "-DINKPOD_XCODE_SCHEME=${INKPOD_MACOS_SCHEME}"
            "-DINKPOD_XCCONFIG=${INKPOD_MACOS_GENERATED_XCCONFIG}"
            "-DINKPOD_DERIVED_DATA=${INKPOD_MACOS_DERIVED_DATA}"
            "-DINKPOD_RUST_STATICLIB=${INKPOD_RUST_STATICLIB}"
            -P "${CMAKE_SOURCE_DIR}/cmake/macos/RunXcodeTests.cmake"
    COMMAND "${Python3_EXECUTABLE}"
            "${INKPOD_MACOS_PARITY_VERIFIER}"
            --repository "${CMAKE_SOURCE_DIR}"
            --manifest "${INKPOD_MACOS_PARITY_MANIFEST}"
    COMMAND "${Python3_EXECUTABLE}"
            "${INKPOD_MACOS_CORE_HOST_VERIFIER}"
            --repository "${CMAKE_SOURCE_DIR}"
    COMMAND "${Python3_EXECUTABLE}"
            "${INKPOD_MACOS_LOCALIZATION_VERIFIER}"
            --repository "${CMAKE_SOURCE_DIR}"
    DEPENDS
        inkpod_rust
        inkpod_macos_header_c11
        inkpod_macos_header_cxx20
        "${INKPOD_MACOS_PARITY_MANIFEST}"
        "${INKPOD_MACOS_PARITY_VERIFIER}"
        "${INKPOD_MACOS_CORE_HOST_VERIFIER}"
        "${INKPOD_MACOS_LOCALIZATION_VERIFIER}"
    COMMENT "Running macOS ABI/import and 384-command parity checks"
    VERBATIM)

add_custom_target(inkpod_macos_tsan
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_XCODEBUILD=${INKPOD_XCODEBUILD_EXECUTABLE}"
            "-DINKPOD_XCODE_PROJECT=${INKPOD_MACOS_PROJECT}"
            "-DINKPOD_XCODE_SCHEME=${INKPOD_MACOS_SCHEME}"
            "-DINKPOD_XCCONFIG=${INKPOD_MACOS_GENERATED_XCCONFIG}"
            "-DINKPOD_DERIVED_DATA=${CMAKE_BINARY_DIR}/xcode-tsan-derived"
            "-DINKPOD_RUST_STATICLIB=${INKPOD_RUST_STATICLIB}"
            -DINKPOD_ENABLE_THREAD_SANITIZER=YES
            -P "${CMAKE_SOURCE_DIR}/cmake/macos/RunXcodeTests.cmake"
    DEPENDS
        inkpod_rust
        inkpod_macos_header_c11
        inkpod_macos_header_cxx20
    COMMENT "Running extended macOS CoreHost tests with Thread Sanitizer"
    VERBATIM)

add_custom_target(inkpod_macos_ui_test
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_XCODEBUILD=${INKPOD_XCODEBUILD_EXECUTABLE}"
            "-DINKPOD_XCODE_PROJECT=${INKPOD_MACOS_PROJECT}"
            "-DINKPOD_XCODE_SCHEME=${INKPOD_MACOS_SCHEME}"
            "-DINKPOD_ONLY_TESTING=InkpodCoreBridgeTests/ProductCanvasLifecycleTests"
            "-DINKPOD_ADDITIONAL_ONLY_TESTING=InkpodUITests/InkpodUITests"
            "-DINKPOD_XCCONFIG=${INKPOD_MACOS_GENERATED_XCCONFIG}"
            "-DINKPOD_DERIVED_DATA=${CMAKE_BINARY_DIR}/xcode-ui-derived"
            "-DINKPOD_RUST_STATICLIB=${INKPOD_RUST_STATICLIB}"
            -P "${CMAKE_SOURCE_DIR}/cmake/macos/RunXcodeUITests.cmake"
    DEPENDS inkpod_rust
    COMMENT "Running macOS product UI/Core/Metal vertical test"
    VERBATIM)

add_custom_target(inkpod_macos_metal_check
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_XCODEBUILD=${INKPOD_XCODEBUILD_EXECUTABLE}"
            "-DINKPOD_XCODE_PROJECT=${INKPOD_MACOS_PROJECT}"
            "-DINKPOD_XCODE_SCHEME=${INKPOD_MACOS_SCHEME}"
            "-DINKPOD_ONLY_TESTING=InkpodCoreBridgeTests/ProductCanvasLifecycleTests"
            "-DINKPOD_XCCONFIG=${INKPOD_MACOS_GENERATED_XCCONFIG}"
            "-DINKPOD_DERIVED_DATA=${CMAKE_BINARY_DIR}/xcode-metal-derived"
            "-DINKPOD_RUST_STATICLIB=${INKPOD_RUST_STATICLIB}"
            -DINKPOD_ENABLE_METAL_VALIDATION=YES
            -P "${CMAKE_SOURCE_DIR}/cmake/macos/RunXcodeUITests.cmake"
    DEPENDS inkpod_rust
    COMMENT "Running macOS Metal API validation profile"
    VERBATIM)

add_test(
    NAME inkpod_macos_command_parity
    COMMAND "${Python3_EXECUTABLE}"
            "${INKPOD_MACOS_PARITY_VERIFIER}"
            --repository "${CMAKE_SOURCE_DIR}"
            --manifest "${INKPOD_MACOS_PARITY_MANIFEST}")

add_test(
    NAME inkpod_macos_core_host_contract
    COMMAND "${Python3_EXECUTABLE}"
            "${INKPOD_MACOS_CORE_HOST_VERIFIER}"
            --repository "${CMAKE_SOURCE_DIR}")

add_test(
    NAME inkpod_macos_localization_catalog
    COMMAND "${Python3_EXECUTABLE}"
            "${INKPOD_MACOS_LOCALIZATION_VERIFIER}"
            --repository "${CMAKE_SOURCE_DIR}")

add_test(
    NAME inkpod_macos_xcode_abi_smoke
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_XCODEBUILD=${INKPOD_XCODEBUILD_EXECUTABLE}"
            "-DINKPOD_XCODE_PROJECT=${INKPOD_MACOS_PROJECT}"
            "-DINKPOD_XCODE_SCHEME=${INKPOD_MACOS_SCHEME}"
            "-DINKPOD_XCCONFIG=${INKPOD_MACOS_GENERATED_XCCONFIG}"
            "-DINKPOD_DERIVED_DATA=${INKPOD_MACOS_DERIVED_DATA}"
            "-DINKPOD_RUST_STATICLIB=${INKPOD_RUST_STATICLIB}"
            -P "${CMAKE_SOURCE_DIR}/cmake/macos/RunXcodeTests.cmake")

add_test(
    NAME inkpod_macos_missing_rust_library_rejected
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_RUST_STATICLIB=${CMAKE_BINARY_DIR}/missing/libinkpod_ffi.a"
            -P "${CMAKE_SOURCE_DIR}/cmake/macos/VerifyRustLibrary.cmake")
set_tests_properties(
    inkpod_macos_missing_rust_library_rejected
    PROPERTIES WILL_FAIL TRUE)

set(INKPOD_MACOS_ARM64_CARGO_TARGET_DIR
    "${CMAKE_BINARY_DIR}/cargo-universal-arm64")
set(INKPOD_MACOS_X86_64_CARGO_TARGET_DIR
    "${CMAKE_BINARY_DIR}/cargo-universal-x86_64")
set(INKPOD_MACOS_ARM64_RUST_STATICLIB
    "${INKPOD_MACOS_ARM64_CARGO_TARGET_DIR}/aarch64-apple-darwin/release/libinkpod_ffi.a")
set(INKPOD_MACOS_X86_64_RUST_STATICLIB
    "${INKPOD_MACOS_X86_64_CARGO_TARGET_DIR}/x86_64-apple-darwin/release/libinkpod_ffi.a")
set(INKPOD_MACOS_UNIVERSAL_RUST_STATICLIB
    "${CMAKE_BINARY_DIR}/cargo-universal/libinkpod_ffi.a")

add_custom_command(
    OUTPUT "${INKPOD_MACOS_UNIVERSAL_RUST_STATICLIB}"
    COMMAND "${CMAKE_COMMAND}" -E make_directory
            "${CMAKE_BINARY_DIR}/cargo-universal"
    COMMAND "${CMAKE_COMMAND}" -E env
            "CARGO_TARGET_DIR=${INKPOD_MACOS_ARM64_CARGO_TARGET_DIR}"
            "${CARGO_EXECUTABLE}" build --locked --package inkpod-ffi
            --target aarch64-apple-darwin --release
    COMMAND "${CMAKE_COMMAND}" -E env
            "CARGO_TARGET_DIR=${INKPOD_MACOS_X86_64_CARGO_TARGET_DIR}"
            "${CARGO_EXECUTABLE}" build --locked --package inkpod-ffi
            --target x86_64-apple-darwin --release
    COMMAND "${INKPOD_XCRUN_EXECUTABLE}" lipo -create
            "${INKPOD_MACOS_ARM64_RUST_STATICLIB}"
            "${INKPOD_MACOS_X86_64_RUST_STATICLIB}"
            -output "${INKPOD_MACOS_UNIVERSAL_RUST_STATICLIB}"
    DEPENDS ${INKPOD_RUST_INPUTS}
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
    COMMENT "Building Universal 2 inkpod-ffi static library"
    VERBATIM)

add_custom_target(inkpod_macos_rust_universal
    DEPENDS "${INKPOD_MACOS_UNIVERSAL_RUST_STATICLIB}")

set(INKPOD_MACOS_UNIVERSAL_XCCONFIG
    "${CMAKE_BINARY_DIR}/generated/InkpodUniversal.xcconfig")
set(INKPOD_XCODE_RUST_STATICLIB
    "${INKPOD_MACOS_UNIVERSAL_RUST_STATICLIB}")
configure_file(
    "${CMAKE_SOURCE_DIR}/cmake/macos/InkpodGenerated.xcconfig.in"
    "${INKPOD_MACOS_UNIVERSAL_XCCONFIG}"
    @ONLY)

add_custom_target(inkpod_macos_archive
    COMMAND "${CMAKE_COMMAND}"
            "-DINKPOD_XCODEBUILD=${INKPOD_XCODEBUILD_EXECUTABLE}"
            "-DINKPOD_XCRUN=${INKPOD_XCRUN_EXECUTABLE}"
            "-DINKPOD_FILE=${INKPOD_FILE_EXECUTABLE}"
            "-DINKPOD_XCODE_PROJECT=${INKPOD_MACOS_PROJECT}"
            "-DINKPOD_XCODE_SCHEME=${INKPOD_MACOS_SCHEME}"
            "-DINKPOD_XCCONFIG=${INKPOD_MACOS_UNIVERSAL_XCCONFIG}"
            "-DINKPOD_DERIVED_DATA=${CMAKE_BINARY_DIR}/xcode-universal-derived"
            "-DINKPOD_RUST_STATICLIB=${INKPOD_MACOS_UNIVERSAL_RUST_STATICLIB}"
            -P "${CMAKE_SOURCE_DIR}/cmake/macos/RunUniversalBuild.cmake"
    DEPENDS
        inkpod_macos_rust_universal
        inkpod_macos_header_c11
        inkpod_macos_header_cxx20
    COMMENT "Building and inspecting Universal 2 macOS ABI smoke artifacts"
    VERBATIM)
