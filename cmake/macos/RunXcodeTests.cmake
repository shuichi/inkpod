foreach(INKPOD_REQUIRED_VARIABLE
        INKPOD_XCODEBUILD
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

set(INKPOD_XCODE_SANITIZER_ARGUMENTS)
if(DEFINED INKPOD_ENABLE_THREAD_SANITIZER
        AND INKPOD_ENABLE_THREAD_SANITIZER)
    list(APPEND INKPOD_XCODE_SANITIZER_ARGUMENTS
        -enableThreadSanitizer YES)
endif()

execute_process(
    COMMAND "${INKPOD_XCODEBUILD}"
            test
            -project "${INKPOD_XCODE_PROJECT}"
            -scheme "${INKPOD_XCODE_SCHEME}"
            -configuration Debug
            -destination "platform=macOS,arch=arm64"
            -derivedDataPath "${INKPOD_DERIVED_DATA}"
            -xcconfig "${INKPOD_XCCONFIG}"
            -disableAutomaticPackageResolution
            -parallel-testing-enabled NO
            -skip-testing:InkpodUITests
            ${INKPOD_XCODE_SANITIZER_ARGUMENTS}
            CODE_SIGNING_ALLOWED=NO
    RESULT_VARIABLE INKPOD_XCODE_RESULT
    OUTPUT_VARIABLE INKPOD_XCODE_OUTPUT
    ERROR_VARIABLE INKPOD_XCODE_ERROR)

if(NOT INKPOD_XCODE_RESULT EQUAL 0)
    message(FATAL_ERROR
        "xcodebuild test failed (${INKPOD_XCODE_RESULT})\n"
        "${INKPOD_XCODE_OUTPUT}\n${INKPOD_XCODE_ERROR}")
endif()

set(INKPOD_CORE_HOST_INTEGRATION
    "${INKPOD_DERIVED_DATA}/Build/Products/Debug/InkpodCoreHostIntegration")
if(NOT EXISTS "${INKPOD_CORE_HOST_INTEGRATION}")
    message(FATAL_ERROR
        "CoreHost integration executable was not produced: "
        "${INKPOD_CORE_HOST_INTEGRATION}")
endif()
execute_process(
    COMMAND "${INKPOD_CORE_HOST_INTEGRATION}"
    RESULT_VARIABLE INKPOD_CORE_HOST_RESULT
    OUTPUT_VARIABLE INKPOD_CORE_HOST_OUTPUT
    ERROR_VARIABLE INKPOD_CORE_HOST_ERROR)
if(NOT INKPOD_CORE_HOST_RESULT EQUAL 0)
    message(FATAL_ERROR
        "CoreHost integration executable failed (${INKPOD_CORE_HOST_RESULT})\n"
        "${INKPOD_CORE_HOST_OUTPUT}\n${INKPOD_CORE_HOST_ERROR}")
endif()

message(STATUS
    "xcodebuild tests and CoreHost integration executable passed for InkpodCoreBridge")
