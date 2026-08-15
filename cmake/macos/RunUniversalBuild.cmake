foreach(INKPOD_REQUIRED_VARIABLE
        INKPOD_XCODEBUILD
        INKPOD_XCRUN
        INKPOD_FILE
        INKPOD_XCODE_PROJECT
        INKPOD_XCODE_SCHEME
        INKPOD_XCCONFIG
        INKPOD_DERIVED_DATA
        INKPOD_RUST_STATICLIB)
    if(NOT DEFINED ${INKPOD_REQUIRED_VARIABLE}
            OR "${${INKPOD_REQUIRED_VARIABLE}}" STREQUAL "")
        message(FATAL_ERROR "${INKPOD_REQUIRED_VARIABLE} is required")
    endif()
endforeach()

include("${CMAKE_CURRENT_LIST_DIR}/VerifyRustLibrary.cmake")

execute_process(
    COMMAND "${INKPOD_XCODEBUILD}"
            build-for-testing
            -project "${INKPOD_XCODE_PROJECT}"
            -scheme "${INKPOD_XCODE_SCHEME}"
            -configuration Release
            -destination "generic/platform=macOS"
            -derivedDataPath "${INKPOD_DERIVED_DATA}"
            -xcconfig "${INKPOD_XCCONFIG}"
            -disableAutomaticPackageResolution
            "ARCHS=arm64 x86_64"
            ONLY_ACTIVE_ARCH=NO
            ENABLE_TESTABILITY=YES
            CODE_SIGNING_ALLOWED=NO
    RESULT_VARIABLE INKPOD_XCODE_RESULT
    OUTPUT_VARIABLE INKPOD_XCODE_OUTPUT
    ERROR_VARIABLE INKPOD_XCODE_ERROR)

if(NOT INKPOD_XCODE_RESULT EQUAL 0)
    message(FATAL_ERROR
        "Universal xcodebuild failed (${INKPOD_XCODE_RESULT})\n"
        "${INKPOD_XCODE_OUTPUT}\n${INKPOD_XCODE_ERROR}")
endif()

set(INKPOD_TEST_EXECUTABLE
    "${INKPOD_DERIVED_DATA}/Build/Products/Release/InkpodCoreBridgeTests.xctest/Contents/MacOS/InkpodCoreBridgeTests")
if(NOT EXISTS "${INKPOD_TEST_EXECUTABLE}")
    message(FATAL_ERROR
        "Universal ABI smoke executable was not produced: ${INKPOD_TEST_EXECUTABLE}")
endif()

set(INKPOD_CORE_HOST_INTEGRATION
    "${INKPOD_DERIVED_DATA}/Build/Products/Release/InkpodCoreHostIntegration")
if(NOT EXISTS "${INKPOD_CORE_HOST_INTEGRATION}")
    message(FATAL_ERROR
        "Universal CoreHost integration executable was not produced: "
        "${INKPOD_CORE_HOST_INTEGRATION}")
endif()

set(INKPOD_APP_EXECUTABLE
    "${INKPOD_DERIVED_DATA}/Build/Products/Release/Inkpod.app/Contents/MacOS/Inkpod")
if(NOT EXISTS "${INKPOD_APP_EXECUTABLE}")
    message(FATAL_ERROR
        "Universal macOS product executable was not produced: ${INKPOD_APP_EXECUTABLE}")
endif()

foreach(INKPOD_BINARY
        "${INKPOD_RUST_STATICLIB}"
        "${INKPOD_TEST_EXECUTABLE}"
        "${INKPOD_CORE_HOST_INTEGRATION}"
        "${INKPOD_APP_EXECUTABLE}")
    execute_process(
        COMMAND "${INKPOD_XCRUN}" lipo -archs "${INKPOD_BINARY}"
        RESULT_VARIABLE INKPOD_LIPO_RESULT
        OUTPUT_VARIABLE INKPOD_LIPO_ARCHS
        ERROR_VARIABLE INKPOD_LIPO_ERROR
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT INKPOD_LIPO_RESULT EQUAL 0)
        message(FATAL_ERROR
            "lipo could not inspect ${INKPOD_BINARY}: ${INKPOD_LIPO_ERROR}")
    endif()
    if(NOT INKPOD_LIPO_ARCHS MATCHES "(^| )arm64($| )"
            OR NOT INKPOD_LIPO_ARCHS MATCHES "(^| )x86_64($| )")
        message(FATAL_ERROR
            "${INKPOD_BINARY} is not Universal 2: ${INKPOD_LIPO_ARCHS}")
    endif()
    execute_process(
        COMMAND "${INKPOD_FILE}" "${INKPOD_BINARY}"
        RESULT_VARIABLE INKPOD_FILE_RESULT
        OUTPUT_VARIABLE INKPOD_FILE_OUTPUT
        ERROR_VARIABLE INKPOD_FILE_ERROR
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT INKPOD_FILE_RESULT EQUAL 0
            OR NOT INKPOD_FILE_OUTPUT MATCHES
                "Mach-O universal binary with 2 architectures")
        message(FATAL_ERROR
            "file did not recognize ${INKPOD_BINARY} as Universal 2: "
            "${INKPOD_FILE_OUTPUT}${INKPOD_FILE_ERROR}")
    endif()
    message(STATUS "Universal 2 ${INKPOD_BINARY}: ${INKPOD_LIPO_ARCHS}")
endforeach()
