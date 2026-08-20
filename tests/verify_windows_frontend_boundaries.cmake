if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(main_source "${INKPOD_SOURCE_DIR}/apps/windows/app/main.cpp")
set(application_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/application.cpp")
set(launch_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/launch_options.cpp")
set(activation_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/activation.cpp")
set(recovery_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/session_recovery.cpp")
set(smoke_source "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp")
set(revision_max_harness_patch
    "${INKPOD_SOURCE_DIR}/tests/revision_max_native_harness_3f164db.patch")
set(runtime_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(chrome_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window.cpp")
set(chrome_header
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window.h")
set(locator_pane_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/panes/locator_pane.cpp")
set(sequence_pane_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/panes/sequence_pane.cpp")
set(light_table_pane_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/panes/light_table_pane.cpp")
set(cmake_source "${INKPOD_SOURCE_DIR}/CMakeLists.txt")

foreach(required_source IN ITEMS
        "${main_source}"
        "${application_source}"
        "${launch_source}"
        "${activation_source}"
        "${recovery_source}"
        "${smoke_source}"
        "${revision_max_harness_patch}"
        "${runtime_source}"
        "${chrome_source}"
        "${chrome_header}"
        "${locator_pane_source}"
        "${sequence_pane_source}"
        "${light_table_pane_source}")
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

set(modeless_pane_text "")
foreach(pane_source IN ITEMS
        "${locator_pane_source}"
        "${sequence_pane_source}"
        "${light_table_pane_source}")
    file(READ "${pane_source}" pane_text)
    string(APPEND modeless_pane_text "${pane_text}")
endforeach()
foreach(forbidden_pane_pointer IN ITEMS
        "reinterpret_cast<LPARAM>(&state)"
        "reinterpret_cast<WPARAM>(&state)")
    string(FIND
        "${modeless_pane_text}" "${forbidden_pane_pointer}" pointer_offset)
    if(NOT pointer_offset LESS 0)
        message(FATAL_ERROR
            "Modeless pane passes a C++ state pointer in a window message: "
            "${forbidden_pane_pointer}")
    endif()
endforeach()

file(READ "${launch_source}" launch_text)
foreach(required_launch_token IN ITEMS
        "CommandLineToArgvW"
        "ParseLaunchArguments"
        "--smoke-test"
        "--performance-smoke-test"
        "--abi-smoke-test"
        "--portable-smoke-test"
        "--new-window")
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
        "ActivationService"
        "launch_.document_paths"
        "OpenDocumentFromPath"
        "ReviewRecoveryCandidates"
        "LoadRestorePreviousDocumentsSetting"
        "CreateDefaultCell"
        "RunMessageLoop"
        "RunApplicationSmoke"
        "RunPerformanceSmoke"
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
        "RunImageEffectsSmoke"
        "RunBatchWorkflowSmoke"
        "RunLightTablePaneSmoke")
    string(FIND "${smoke_text}" "${required_smoke}" smoke_offset)
    if(smoke_offset LESS 0)
        message(FATAL_ERROR
            "app_smoke.cpp is missing ${required_smoke}")
    endif()
endforeach()

file(READ "${revision_max_harness_patch}" revision_max_harness_patch_text)
file(SHA256 "${revision_max_harness_patch}" revision_max_harness_patch_sha256)
if(NOT revision_max_harness_patch_sha256 STREQUAL
        "2b434f0ab5827fc987f0cb583ff68f65c4af6b9aaf89531fa8735bee071044a0")
    message(FATAL_ERROR
        "revision-max native harness patch changed without a baseline audit")
endif()
set(revision_max_harness_paths
    "apps/windows/app/app_smoke.cpp"
    "apps/windows/app/app_smoke.h"
    "apps/windows/app/application.cpp"
    "apps/windows/app/application.h"
    "apps/windows/app/launch_options.cpp"
    "apps/windows/app/launch_options.h"
    "apps/windows/app/main.cpp"
    "apps/windows/renderer/canvas.cpp")
string(REGEX MATCHALL
    "diff --git a/[^ \r\n]+ b/[^ \r\n]+"
    revision_max_harness_headers
    "${revision_max_harness_patch_text}")
list(LENGTH revision_max_harness_headers revision_max_harness_header_count)
if(NOT revision_max_harness_header_count EQUAL 8)
    message(FATAL_ERROR
        "revision-max native harness patch must change exactly eight files")
endif()
foreach(expected_path IN LISTS revision_max_harness_paths)
    string(FIND
        "${revision_max_harness_patch_text}"
        "diff --git a/${expected_path} b/${expected_path}"
        expected_path_offset)
    if(expected_path_offset LESS 0)
        message(FATAL_ERROR
            "revision-max native harness patch is missing ${expected_path}")
    endif()
endforeach()
string(REGEX MATCHALL
    "index [0-9a-f]+\\.\\.[0-9a-f]+ 100644"
    revision_max_harness_indexes
    "${revision_max_harness_patch_text}")
list(LENGTH revision_max_harness_indexes revision_max_harness_index_count)
if(NOT revision_max_harness_index_count EQUAL 8)
    message(FATAL_ERROR
        "revision-max native harness patch must retain eight full-index records")
endif()
foreach(index_record IN LISTS revision_max_harness_indexes)
    string(LENGTH "${index_record}" index_record_length)
    if(NOT index_record_length EQUAL 95)
        message(FATAL_ERROR
            "revision-max native harness patch contains an abbreviated object ID")
    endif()
endforeach()
string(FIND
    "${revision_max_harness_patch_text}"
    "diff --git a/apps/windows/renderer/canvas.cpp b/apps/windows/renderer/canvas.cpp"
    revision_max_canvas_offset)
string(SUBSTRING
    "${revision_max_harness_patch_text}"
    ${revision_max_canvas_offset}
    -1
    revision_max_canvas_patch)
foreach(required_canvas_instrumentation IN ITEMS
        "+                in_flight_work_ = 0U;"
        "+            return stopping_ || (work_.empty() && in_flight_work_ == 0U);"
        "+                ++in_flight_work_;"
        "+                --in_flight_work_;"
        "+    std::size_t in_flight_work_{};")
    string(FIND
        "${revision_max_canvas_patch}"
        "${required_canvas_instrumentation}"
        instrumentation_offset)
    if(instrumentation_offset LESS 0)
        message(FATAL_ERROR
            "revision-max baseline patch lost renderer idle instrumentation: "
            "${required_canvas_instrumentation}")
    endif()
endforeach()
string(REGEX MATCHALL
    "[\r\n][+-][^\r\n]*"
    revision_max_canvas_changed_lines
    "${revision_max_canvas_patch}")
foreach(forbidden_canvas_algorithm IN ITEMS
        "RenderAndCount"
        "ProcessSnapshot"
        "UpdateTileBudgets"
        "Present(")
    string(FIND
        "${revision_max_canvas_changed_lines}"
        "${forbidden_canvas_algorithm}"
        changed_algorithm_offset)
    if(NOT changed_algorithm_offset LESS 0)
        message(FATAL_ERROR
            "revision-max baseline patch changes renderer algorithm: "
            "${forbidden_canvas_algorithm}")
    endif()
endforeach()

file(READ "${runtime_source}" runtime_text)
foreach(forbidden_runtime_token IN ITEMS
        "wWinMain"
        "RunDrawingPersistenceSmoke"
        "RunBatchWorkflowSmoke"
        "--smoke-test"
        "--abi-smoke-test"
        "--portable-smoke-test")
    string(FIND "${runtime_text}" "${forbidden_runtime_token}" token_offset)
    if(NOT token_offset LESS 0)
        message(FATAL_ERROR
            "MainWindow runtime contains bootstrap/smoke responsibility: "
            "${forbidden_runtime_token}")
    endif()
endforeach()

foreach(required_raster_geometry_token IN ITEMS
        "HandleRasterGeometryCanvasEvent"
        "inkpod_core_geometry_points_resolve"
        "inkpod_core_geometry_preview_begin"
        "inkpod_core_geometry_preview_update"
        "inkpod_core_geometry_preview_commit"
        "inkpod_core_geometry_preview_cancel"
        "IDM_GEOMETRY_LINE"
        "IDM_GEOMETRY_CURVE"
        "IDM_GEOMETRY_RECTANGLE"
        "IDM_GEOMETRY_ELLIPSE"
        "IDM_GEOMETRY_POLYGON"
        "IDM_GEOMETRY_POLYLINE")
    string(FIND
        "${runtime_text}"
        "${required_raster_geometry_token}"
        raster_geometry_offset)
    if(raster_geometry_offset LESS 0)
        message(FATAL_ERROR
            "MainWindow runtime lost Raster Geometry route: "
            "${required_raster_geometry_token}")
    endif()
endforeach()

file(READ "${cmake_source}" cmake_text)
foreach(required_cmake_source IN ITEMS
        "apps/windows/app/app_smoke.cpp"
        "apps/windows/app/application.cpp"
        "apps/windows/app/activation.cpp"
        "apps/windows/app/launch_options.cpp"
        "apps/windows/app/session_recovery.cpp"
        "apps/windows/app/main.cpp"
        "apps/windows/ui/command_catalog.cpp"
        "apps/windows/ui/shortcut_controller.cpp"
        "apps/windows/ui/panes/locator_pane.cpp"
        "apps/windows/ui/panes/sequence_pane.cpp"
        "apps/windows/ui/panes/light_table_pane.cpp"
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
