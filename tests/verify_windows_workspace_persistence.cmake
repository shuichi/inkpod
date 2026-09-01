if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(LAYOUT_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/ui/workspace_layout.h")
set(LAYOUT_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/ui/workspace_layout.cpp")
set(SETTINGS_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/app/application_settings.h")
set(SETTINGS_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/application_settings.cpp")
set(WORKSPACE_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/app/workspace_window.h")
set(DATA_PATH_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/application_data_paths.cpp")
set(MAIN_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window.cpp")
set(RUNTIME_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(CANVAS_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/renderer/canvas.h")
set(CANVAS_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/renderer/canvas.cpp")
set(TEST_SOURCE "${INKPOD_SOURCE_DIR}/tests/windows_workspace_layout.cpp")
set(SETTINGS_TEST "${INKPOD_SOURCE_DIR}/tests/windows_application_settings.cpp")
set(RESOURCE_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui_ja.generated.rc")

foreach(FILE IN ITEMS
        "${LAYOUT_HEADER}"
        "${LAYOUT_SOURCE}"
        "${SETTINGS_HEADER}"
        "${SETTINGS_SOURCE}"
        "${WORKSPACE_HEADER}"
        "${DATA_PATH_SOURCE}"
        "${MAIN_SOURCE}"
        "${RUNTIME_SOURCE}"
        "${CANVAS_HEADER}"
        "${CANVAS_SOURCE}"
        "${TEST_SOURCE}"
        "${SETTINGS_TEST}"
        "${RESOURCE_SOURCE}")
    if(NOT EXISTS "${FILE}")
        message(FATAL_ERROR "Missing G9 source: ${FILE}")
    endif()
endforeach()

file(READ "${LAYOUT_HEADER}" LAYOUT_HEADER_TEXT)
foreach(REQUIRED IN ITEMS
        "WorkspacePreset"
        "WorkspaceWindowPlacement"
        "WorkspaceAuxiliaryPaneState"
        "WorkspaceAutoHideEdge"
        "WorkspaceSplitOrientation"
        "RightToolTabsModel right_tool_tabs"
        "kMaximumPersistedWorkspaceWindows"
        "ClampWorkspacePlacement")
    string(FIND "${LAYOUT_HEADER_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 bounded layout contract is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${LAYOUT_SOURCE}" LAYOUT_SOURCE_TEXT)
foreach(REQUIRED IN ITEMS
        "ValidateCurrentTabs"
        "kMaximumWorkspaceLayoutRecordBytes"
        "WorkspacePreset::ReferenceCheck"
        "WorkspacePreset::Focus"
        "MONITORINFO"
        "GetDpiForMonitor"
        "g_monitor_collection")
    string(FIND "${LAYOUT_SOURCE_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 persistence implementation is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${MAIN_SOURCE}" MAIN_TEXT)
foreach(REQUIRED IN ITEMS
        "auto_hide_buttons"
        "WS_TABSTOP | BS_PUSHBUTTON"
        "layout.auto_hide_buttons")
    string(FIND "${MAIN_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 accessible auto-hide strip is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${RUNTIME_SOURCE}" RUNTIME_TEXT)
foreach(REQUIRED IN ITEMS
        "CaptureWorkspacePresentation"
        "ApplyWorkspacePresentation"
        "CollapseAutoHiddenPanes"
        "PersistApplicationSettings"
        "SaveApplicationSettingsUpdate"
        "SaveWorkspaceSnapshot"
        "state.settings.Workspace"
        "IDM_WORKSPACE_SAVE_AS"
        "IDM_WORKSPACE_PRESET_COLORING"
        "IDM_WORKSPACE_AUTOHIDE_LOCATOR"
        "ApplyOrDeferWorkspacePresentation"
        "workspace_presentation_pending"
        "kCanvasInteractionEnded"
        "WM_DISPLAYCHANGE")
    string(FIND "${RUNTIME_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 UI integration is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${SETTINGS_HEADER}" SETTINGS_HEADER_TEXT)
file(READ "${SETTINGS_SOURCE}" SETTINGS_SOURCE_TEXT)
file(READ "${WORKSPACE_HEADER}" WORKSPACE_HEADER_TEXT)
file(READ "${DATA_PATH_SOURCE}" DATA_PATH_SOURCE_TEXT)
foreach(REQUIRED IN ITEMS
        "kApplicationSettingsFormatVersion = 4U"
        "ApplicationSettingsStore"
        "PersistedWorkspace"
        "inkpod-settings.json"
        "inkpod-settings"
        "formatVersion"
        "savedLayouts"
        "right-tab-"
        "layer-plane"
        "SaveApplicationSettingsFile"
        "MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH")
    string(FIND "${SETTINGS_HEADER_TEXT}${SETTINGS_SOURCE_TEXT}${DATA_PATH_SOURCE_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Readable application-settings contract is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(FORBIDDEN IN ITEMS
        "DockPaneType::JobProgress"
        "JobProgressState"
        "job_progress_state"
        "\"job-progress\"")
    string(FIND "${LAYOUT_HEADER_TEXT}${LAYOUT_SOURCE_TEXT}${SETTINGS_SOURCE_TEXT}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Transient statusbar job progress must not be persisted as workspace layout: ${FORBIDDEN}")
    endif()
endforeach()
string(REGEX REPLACE "[ \t\r\n]+" " " RUNTIME_COMPACT "${RUNTIME_TEXT}")
if(NOT WORKSPACE_HEADER_TEXT MATCHES "JobProgressState job_progress_state")
    message(FATAL_ERROR "Transient job progress must be owned by each WorkspaceWindow")
endif()
foreach(REQUIRED IN ITEMS
        "state.Workspace().job_progress_state = {}"
        "InitializeJobProgress( state.Workspace().windows.status_bar, state.Workspace().job_progress_state,"
        "owner->windows.status_bar, owner->job_progress_state")
    string(FIND "${RUNTIME_COMPACT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Transient job progress must bind to its workspace statusbar: ${REQUIRED}")
    endif()
endforeach()
foreach(FORBIDDEN IN ITEMS
        "HKEY_CURRENT_USER"
        "RegGetValueW"
        "RegSetValueExW"
        "WorkspaceSessionV"
        "WorkspaceSavedV")
    string(FIND "${SETTINGS_SOURCE_TEXT}${RUNTIME_TEXT}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Application settings still use retired registry persistence: ${FORBIDDEN}")
    endif()
endforeach()

file(READ "${CANVAS_HEADER}" CANVAS_HEADER_TEXT)
file(READ "${CANVAS_SOURCE}" CANVAS_SOURCE_TEXT)
foreach(REQUIRED IN ITEMS
        "kCanvasInteractionEnded"
        "host->Canvas().Value()"
        "host->SurfaceGeneration().Value()")
    string(FIND "${CANVAS_HEADER_TEXT}${CANVAS_SOURCE_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 input-safe layout routing is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${RESOURCE_SOURCE}" RESOURCE_TEXT)
foreach(REQUIRED IN ITEMS
        "ワークスペースを保存"
        "名前を付けて保存"
        "参照・チェック"
        "補助ペインを自動格納")
    string(FIND "${RESOURCE_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 workspace resource is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${TEST_SOURCE}" TEST_TEXT)
foreach(REQUIRED IN ITEMS
        "replacement_tabs"
        "capacity_model"
        "mixed_serialized"
        "RetiredJobProgressPaneIsAbsent"
        "retired_job_progress"
        "unknown_pane"
        "missing_monitor"
        "added_monitor"
        "WorkspaceDensity::Compact")
    string(FIND "${TEST_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 persistence test evidence is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${SETTINGS_TEST}" SETTINGS_TEST_TEXT)
foreach(REQUIRED IN ITEMS
        "inkpod-settings"
        "formatVersion"
        "file.save"
        "logicalKey"
        "physicalKey"
        "base64"
        "wrong_version"
        "duplicate"
        "unknown")
    string(FIND "${SETTINGS_TEST_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Readable settings test evidence is missing: ${REQUIRED}")
    endif()
endforeach()

message(STATUS
    "Verified bounded human-readable settings JSON, dynamic-tab and saved-layout persistence, "
    "transient statusbar progress isolation, monitor recovery, accessible auxiliary-pane auto-hide "
    "integration, and bounded workspace count")
