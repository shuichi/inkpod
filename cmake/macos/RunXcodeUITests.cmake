foreach(INKPOD_REQUIRED_VARIABLE
        INKPOD_XCODEBUILD
        INKPOD_XCODE_PROJECT
        INKPOD_XCODE_SCHEME
        INKPOD_ONLY_TESTING
        INKPOD_XCCONFIG
        INKPOD_DERIVED_DATA
        INKPOD_RUST_STATICLIB)
    if(NOT DEFINED ${INKPOD_REQUIRED_VARIABLE}
            OR "${${INKPOD_REQUIRED_VARIABLE}}" STREQUAL "")
        message(FATAL_ERROR "${INKPOD_REQUIRED_VARIABLE} is required")
    endif()
endforeach()

include("${CMAKE_CURRENT_LIST_DIR}/VerifyRustLibrary.cmake")

set(INKPOD_METAL_VALIDATION 0)
if(DEFINED INKPOD_ENABLE_METAL_VALIDATION
        AND INKPOD_ENABLE_METAL_VALIDATION)
    set(INKPOD_METAL_VALIDATION 1)
endif()

set(INKPOD_ONLY_TESTING_ARGUMENTS "-only-testing:${INKPOD_ONLY_TESTING}")
if(DEFINED INKPOD_ADDITIONAL_ONLY_TESTING
        AND NOT "${INKPOD_ADDITIONAL_ONLY_TESTING}" STREQUAL "")
    list(APPEND INKPOD_ONLY_TESTING_ARGUMENTS
        "-only-testing:${INKPOD_ADDITIONAL_ONLY_TESTING}")
endif()

execute_process(
    COMMAND "${CMAKE_COMMAND}" -E env
            "INKPOD_ENABLE_METAL_VALIDATION=${INKPOD_METAL_VALIDATION}"
            "MTL_DEBUG_LAYER=${INKPOD_METAL_VALIDATION}"
            "MTL_DEBUG_LAYER_ERROR_MODE=assert"
            "${INKPOD_XCODEBUILD}"
            test
            -project "${INKPOD_XCODE_PROJECT}"
            -scheme "${INKPOD_XCODE_SCHEME}"
            -configuration Debug
            -destination "platform=macOS,arch=arm64"
            -derivedDataPath "${INKPOD_DERIVED_DATA}"
            -xcconfig "${INKPOD_XCCONFIG}"
            -disableAutomaticPackageResolution
            -parallel-testing-enabled NO
            ${INKPOD_ONLY_TESTING_ARGUMENTS}
            CODE_SIGNING_ALLOWED=YES
            CODE_SIGNING_REQUIRED=YES
            CODE_SIGN_IDENTITY=-
    RESULT_VARIABLE INKPOD_XCODE_RESULT
    OUTPUT_VARIABLE INKPOD_XCODE_OUTPUT
    ERROR_VARIABLE INKPOD_XCODE_ERROR)

if(NOT INKPOD_XCODE_RESULT EQUAL 0)
    message(FATAL_ERROR
        "macOS UI tests failed (${INKPOD_XCODE_RESULT})\n"
        "${INKPOD_XCODE_OUTPUT}\n${INKPOD_XCODE_ERROR}")
endif()

set(INKPOD_APP
    "${INKPOD_DERIVED_DATA}/Build/Products/Debug/Inkpod.app")
set(INKPOD_APP_EXECUTABLE "${INKPOD_APP}/Contents/MacOS/Inkpod")
set(INKPOD_SHADER "${INKPOD_APP}/Contents/Resources/CanvasShaders.metal")
foreach(INKPOD_PRODUCT "${INKPOD_APP_EXECUTABLE}" "${INKPOD_SHADER}")
    if(NOT EXISTS "${INKPOD_PRODUCT}")
        message(FATAL_ERROR "macOS product artifact is missing: ${INKPOD_PRODUCT}")
    endif()
endforeach()

execute_process(
    COMMAND /usr/bin/codesign -d --entitlements :- "${INKPOD_APP}"
    RESULT_VARIABLE INKPOD_CODESIGN_RESULT
    OUTPUT_VARIABLE INKPOD_ENTITLEMENTS_OUTPUT
    ERROR_VARIABLE INKPOD_ENTITLEMENTS_ERROR)
if(NOT INKPOD_CODESIGN_RESULT EQUAL 0)
    message(FATAL_ERROR
        "sandbox entitlement inspection failed (${INKPOD_CODESIGN_RESULT})\n"
        "${INKPOD_ENTITLEMENTS_OUTPUT}\n${INKPOD_ENTITLEMENTS_ERROR}")
endif()
set(INKPOD_ENTITLEMENTS
    "${INKPOD_ENTITLEMENTS_OUTPUT}${INKPOD_ENTITLEMENTS_ERROR}")
string(FIND "${INKPOD_ENTITLEMENTS}"
    "<key>com.apple.security.app-sandbox</key>" INKPOD_SANDBOX_KEY)
if(INKPOD_SANDBOX_KEY EQUAL -1)
    message(FATAL_ERROR "macOS UI product is not App Sandbox enabled")
endif()

message(STATUS
    "macOS product UI/Core/Metal test passed; Metal validation=${INKPOD_METAL_VALIDATION}")
