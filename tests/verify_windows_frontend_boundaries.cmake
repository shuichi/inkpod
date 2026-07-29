if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(main_source "${INKPOD_SOURCE_DIR}/apps/windows/app/main.cpp")
set(application_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/application.cpp")
set(launch_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/launch_options.cpp")
set(smoke_source "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp")
set(runtime_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(chrome_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window.cpp")
set(chrome_header
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window.h")
set(cmake_source "${INKPOD_SOURCE_DIR}/CMakeLists.txt")

foreach(required_source IN ITEMS
        "${main_source}"
        "${application_source}"
        "${launch_source}"
        "${smoke_source}"
        "${runtime_source}"
        "${chrome_source}"
        "${chrome_header}")
    if(NOT EXISTS "${required_source}")
        message(FATAL_ERROR "frontend source is missing: ${required_source}")
    endif()
endforeach()

file(READ "${chrome_source}" chrome_text)
file(READ "${chrome_header}" chrome_header_text)
string(APPEND chrome_text "${chrome_header_text}")
foreach(forbidden_chrome_token IN ITEMS
        "TOOLBARCLASSNAME"
        "TRACKBAR_CLASSW"
        "L\"LISTBOX\""
        "right_pane_width"
        "zoom_slider"
        "locator_label"
        "layer_list"
        "plane_list"
        "light_table_set_list"
        "light_table_item_list"
        "sequence_list"
        "motion_label"
        "color_palette_list"
        "color_chart_list")
    string(FIND "${chrome_text}" "${forbidden_chrome_token}" token_offset)
    if(NOT token_offset LESS 0)
        message(FATAL_ERROR
            "MainWindow chrome contains retired persistent UI: "
            "${forbidden_chrome_token}")
    endif()
endforeach()

file(READ "${main_source}" main_text)
file(STRINGS "${main_source}" main_lines)
list(LENGTH main_lines main_line_count)
if(main_line_count GREATER 200)
    message(FATAL_ERROR
        "main.cpp is no longer startup-only: ${main_line_count} lines")
endif()

foreach(required_main_token IN ITEMS
        "application.h"
        "launch_options.h"
        "ParseProcessLaunchOptions"
        "ApplicationLaunch")
    string(FIND "${main_text}" "${required_main_token}" token_offset)
    if(token_offset LESS 0)
        message(FATAL_ERROR
            "main.cpp is missing ${required_main_token}")
    endif()
endforeach()

file(READ "${launch_source}" launch_text)
foreach(required_launch_token IN ITEMS
        "CommandLineToArgvW"
        "ParseLaunchArguments"
        "--smoke-test"
        "--abi-smoke-test")
    string(FIND "${launch_text}" "${required_launch_token}" token_offset)
    if(token_offset LESS 0)
        message(FATAL_ERROR
            "launch_options.cpp is missing ${required_launch_token}")
    endif()
endforeach()

foreach(forbidden_main_token IN ITEMS
        "AppContext"
        "CoreEngine"
        "CreateWindowExW"
        "GetMessageW"
        "IDM_"
        "RunDrawingPersistenceSmoke"
        "DialogBoxParamW"
        "CreateDialogParamW")
    string(FIND "${main_text}" "${forbidden_main_token}" token_offset)
    if(NOT token_offset LESS 0)
        message(FATAL_ERROR
            "main.cpp contains non-launch responsibility: "
            "${forbidden_main_token}")
    endif()
endforeach()

file(READ "${application_source}" application_text)
foreach(required_application_token IN ITEMS
        "InitCommonControlsEx"
        "ComApartment"
        "NewestPrivateRecovery"
        "launch_.document_path"
        "OpenDocumentFromPath"
        "CreateDefaultCell"
        "RunMessageLoop"
        "RunApplicationSmoke"
        "StopCore")
    string(FIND
        "${application_text}" "${required_application_token}" token_offset)
    if(token_offset LESS 0)
        message(FATAL_ERROR
            "Application is missing lifetime responsibility: "
            "${required_application_token}")
    endif()
endforeach()
string(FIND "${application_text}" "IDM_" application_command_offset)
if(NOT application_command_offset LESS 0)
    message(FATAL_ERROR "Application contains feature command handling")
endif()

file(READ "${smoke_source}" smoke_text)
foreach(required_smoke IN ITEMS
        "RunDrawingPersistenceSmoke"
        "RunPaintingRecoverySmoke"
        "RunDocumentEditingSmoke"
        "RunProductionWorkflowSmoke"
        "RunVectorWorkflowSmoke"
        "RunImageEffectsSmoke"
        "RunBatchWorkflowSmoke")
    string(FIND "${smoke_text}" "${required_smoke}" smoke_offset)
    if(smoke_offset LESS 0)
        message(FATAL_ERROR
            "app_smoke.cpp is missing ${required_smoke}")
    endif()
endforeach()

file(READ "${runtime_source}" runtime_text)
foreach(forbidden_runtime_token IN ITEMS
        "wWinMain"
        "RunDrawingPersistenceSmoke"
        "RunBatchWorkflowSmoke"
        "--smoke-test"
        "--abi-smoke-test")
    string(FIND "${runtime_text}" "${forbidden_runtime_token}" token_offset)
    if(NOT token_offset LESS 0)
        message(FATAL_ERROR
            "MainWindow runtime contains bootstrap/smoke responsibility: "
            "${forbidden_runtime_token}")
    endif()
endforeach()

file(READ "${cmake_source}" cmake_text)
foreach(required_cmake_source IN ITEMS
        "apps/windows/app/app_smoke.cpp"
        "apps/windows/app/application.cpp"
        "apps/windows/app/launch_options.cpp"
        "apps/windows/app/main.cpp"
        "apps/windows/ui/command_catalog.cpp"
        "apps/windows/ui/shortcut_controller.cpp"
        "apps/windows/ui/main_window_runtime.cpp")
    string(FIND "${cmake_text}" "${required_cmake_source}" source_offset)
    if(source_offset LESS 0)
        message(FATAL_ERROR
            "CMake source list is missing ${required_cmake_source}")
    endif()
endforeach()

message(STATUS
    "Verified startup/application/smoke/MainWindow boundaries; "
    "main.cpp has ${main_line_count} lines")
