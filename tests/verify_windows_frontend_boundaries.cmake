if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(main_source "${INKPOD_SOURCE_DIR}/apps/windows/app/main.cpp")
set(application_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/application.cpp")
set(smoke_source "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp")
set(runtime_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(cmake_source "${INKPOD_SOURCE_DIR}/CMakeLists.txt")

foreach(required_source IN ITEMS
        "${main_source}"
        "${application_source}"
        "${smoke_source}"
        "${runtime_source}")
    if(NOT EXISTS "${required_source}")
        message(FATAL_ERROR "R6 source is missing: ${required_source}")
    endif()
endforeach()

file(READ "${main_source}" main_text)
file(STRINGS "${main_source}" main_lines)
list(LENGTH main_lines main_line_count)
if(main_line_count GREATER 200)
    message(FATAL_ERROR
        "R6 main.cpp is no longer startup-only: ${main_line_count} lines")
endif()

foreach(required_main_token IN ITEMS
        "application.h"
        "ParseLaunchMode"
        "--smoke-test"
        "--abi-smoke-test"
        "Application({")
    string(FIND "${main_text}" "${required_main_token}" token_offset)
    if(token_offset LESS 0)
        message(FATAL_ERROR
            "R6 main.cpp is missing ${required_main_token}")
    endif()
endforeach()

foreach(forbidden_main_token IN ITEMS
        "AppContext"
        "CoreEngine"
        "CreateWindowExW"
        "GetMessageW"
        "IDM_"
        "RunM1Smoke"
        "DialogBoxParamW"
        "CreateDialogParamW")
    string(FIND "${main_text}" "${forbidden_main_token}" token_offset)
    if(NOT token_offset LESS 0)
        message(FATAL_ERROR
            "R6 main.cpp contains non-launch responsibility: "
            "${forbidden_main_token}")
    endif()
endforeach()

file(READ "${application_source}" application_text)
foreach(required_application_token IN ITEMS
        "InitCommonControlsEx"
        "ComApartment"
        "NewestPrivateRecovery"
        "CreateDefaultCell"
        "RunMessageLoop"
        "RunApplicationSmoke"
        "StopCore")
    string(FIND
        "${application_text}" "${required_application_token}" token_offset)
    if(token_offset LESS 0)
        message(FATAL_ERROR
            "R6 Application is missing lifetime responsibility: "
            "${required_application_token}")
    endif()
endforeach()
string(FIND "${application_text}" "IDM_" application_command_offset)
if(NOT application_command_offset LESS 0)
    message(FATAL_ERROR "R6 Application contains feature command handling")
endif()

file(READ "${smoke_source}" smoke_text)
foreach(smoke_stage RANGE 1 7)
    string(FIND "${smoke_text}" "RunM${smoke_stage}Smoke" stage_offset)
    if(stage_offset LESS 0)
        message(FATAL_ERROR
            "R6 app_smoke.cpp is missing M${smoke_stage} smoke")
    endif()
endforeach()

file(READ "${runtime_source}" runtime_text)
foreach(forbidden_runtime_token IN ITEMS
        "wWinMain"
        "RunM1Smoke"
        "RunM7Smoke"
        "--smoke-test"
        "--abi-smoke-test")
    string(FIND "${runtime_text}" "${forbidden_runtime_token}" token_offset)
    if(NOT token_offset LESS 0)
        message(FATAL_ERROR
            "R6 MainWindow runtime contains bootstrap/smoke responsibility: "
            "${forbidden_runtime_token}")
    endif()
endforeach()

file(READ "${cmake_source}" cmake_text)
foreach(required_cmake_source IN ITEMS
        "apps/windows/app/app_smoke.cpp"
        "apps/windows/app/application.cpp"
        "apps/windows/app/main.cpp"
        "apps/windows/ui/main_window_runtime.cpp")
    string(FIND "${cmake_text}" "${required_cmake_source}" source_offset)
    if(source_offset LESS 0)
        message(FATAL_ERROR
            "R6 CMake source list is missing ${required_cmake_source}")
    endif()
endforeach()

message(STATUS
    "Verified R6 startup/application/smoke/MainWindow boundaries; "
    "main.cpp has ${main_line_count} lines")
