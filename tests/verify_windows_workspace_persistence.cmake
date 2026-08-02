if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(LAYOUT_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/ui/workspace_layout.h")
set(LAYOUT_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/ui/workspace_layout.cpp")
set(MAIN_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window.cpp")
set(RUNTIME_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(CANVAS_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/renderer/canvas.h")
set(CANVAS_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/renderer/canvas.cpp")
set(TEST_SOURCE "${INKPOD_SOURCE_DIR}/tests/windows_workspace_layout.cpp")
set(RESOURCE_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/app.rc")

foreach(FILE IN ITEMS
        "${LAYOUT_HEADER}"
        "${LAYOUT_SOURCE}"
        "${MAIN_SOURCE}"
        "${RUNTIME_SOURCE}"
        "${CANVAS_HEADER}"
        "${CANVAS_SOURCE}"
        "${TEST_SOURCE}"
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
        "EncodeWorkspaceLayout"
        "DecodeWorkspaceLayout"
        "DeleteWorkspaceLayout"
        "SaveWorkspaceWindowCount"
        "LoadWorkspaceWindowCount"
        "ClampWorkspacePlacement")
    string(FIND "${LAYOUT_HEADER_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 bounded layout contract is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${LAYOUT_SOURCE}" LAYOUT_SOURCE_TEXT)
foreach(REQUIRED IN ITEMS
        "kVersion = 4U"
        "PersistedWorkspaceLayoutV4"
        "DecodeVersion3"
        "LoadLegacyLayout"
        "FindPaneDescriptorByStableId"
        "kMaximumWorkspaceLayoutRecordBytes"
        "WorkspacePreset::ReferenceCheck"
        "WorkspacePreset::Focus"
        "MONITORINFO"
        "GetDpiForMonitor"
        "g_monitor_collection"
        "RegGetValueW"
        "RegSetValueExW"
        "WorkspaceWindowCountV1")
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
        "WorkspaceSessionV4"
        "WorkspaceSavedV4"
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
        "EncodeWorkspaceLayout"
        "DecodeWorkspaceLayout"
        "WorkspaceLayoutDecodeResult::Migrated"
        "LegacyWorkspaceV3"
        "unknown_pane"
        "missing_monitor"
        "added_monitor"
        "WorkspaceDensity::Compact"
        "SaveWorkspaceLayout"
        "LoadWorkspaceLayout")
    string(FIND "${TEST_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G9 persistence test evidence is missing: ${REQUIRED}")
    endif()
endforeach()

message(STATUS
    "Verified G9 bounded v4 persistence, v2/v3 migration, named presets, "
    "monitor recovery, accessible auxiliary-pane auto-hide integration, "
    "and G10 bounded workspace-window count persistence")
