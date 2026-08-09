if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(UI_DIR "${INKPOD_SOURCE_DIR}/apps/windows/ui")
set(MODEL_HEADER "${UI_DIR}/dock_layout.h")
set(MODEL_SOURCE "${UI_DIR}/dock_layout.cpp")
set(HOST_HEADER "${UI_DIR}/dock_host.h")
set(HOST_SOURCE "${UI_DIR}/dock_host.cpp")
set(MAIN_HEADER "${UI_DIR}/main_window.h")
set(MAIN_SOURCE "${UI_DIR}/main_window.cpp")
set(RUNTIME_SOURCE "${UI_DIR}/main_window_runtime.cpp")
set(TEST_SOURCE "${INKPOD_SOURCE_DIR}/tests/windows_workspace_layout.cpp")
set(RESOURCE_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/app/resource.h")
set(RESOURCE_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/app.rc")
set(LOCATOR_SOURCE "${UI_DIR}/panes/locator_pane.cpp")
set(SEQUENCE_SOURCE "${UI_DIR}/panes/sequence_pane.cpp")
set(LIGHT_TABLE_SOURCE "${UI_DIR}/panes/light_table_pane.cpp")
set(REFERENCE_SOURCE "${UI_DIR}/panes/subpalette_pane.cpp")
set(BATCH_SOURCE "${UI_DIR}/dialogs/batch_dialog.cpp")
set(PROGRESS_SOURCE "${UI_DIR}/dialogs/effects_dialogs.cpp")
set(PROGRESS_HEADER "${UI_DIR}/dialogs/effects_dialogs.h")

foreach(FILE IN ITEMS
        "${MODEL_HEADER}"
        "${MODEL_SOURCE}"
        "${HOST_HEADER}"
        "${HOST_SOURCE}"
        "${MAIN_HEADER}"
        "${MAIN_SOURCE}"
        "${RUNTIME_SOURCE}"
        "${TEST_SOURCE}"
        "${RESOURCE_HEADER}"
        "${RESOURCE_SOURCE}"
        "${LOCATOR_SOURCE}"
        "${SEQUENCE_SOURCE}"
        "${LIGHT_TABLE_SOURCE}"
        "${REFERENCE_SOURCE}"
        "${BATCH_SOURCE}"
        "${PROGRESS_SOURCE}"
        "${PROGRESS_HEADER}")
    if(NOT EXISTS "${FILE}")
        message(FATAL_ERROR "Missing G7 source: ${FILE}")
    endif()
endforeach()

set(PANE_IMPLEMENTATION "")
foreach(FILE IN ITEMS
        "${LOCATOR_SOURCE}"
        "${SEQUENCE_SOURCE}"
        "${LIGHT_TABLE_SOURCE}"
        "${REFERENCE_SOURCE}"
        "${BATCH_SOURCE}"
        "${PROGRESS_SOURCE}"
        "${PROGRESS_HEADER}")
    file(READ "${FILE}" PANE_SOURCE_TEXT)
    string(APPEND PANE_IMPLEMENTATION "${PANE_SOURCE_TEXT}")
endforeach()
foreach(REQUIRED IN ITEMS
        "LayoutLocatorPane"
        "LayoutSequencePane"
        "LayoutLightTablePane"
        "LayoutSubpalettePaneDialog"
        "LayoutBatchPane"
        "CreateJobProgressPane"
        "enum class JobProgressSlot"
        "JobProgressSlot::Count")
    string(FIND "${PANE_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Docked modeless pane implementation is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${MODEL_HEADER}" MODEL)
foreach(REQUIRED IN ITEMS
        "class DockLayoutModel final"
        "struct PaneDescriptor"
        "title_resource_id"
        "TopContext,"
        "Floating,"
        "Hidden,"
        "DockResult AddPane"
        "DockResult RemovePane"
        "DockResult MovePane"
        "DockResult TabPane"
        "DockResult FloatPane"
        "DockResult HidePane"
        "DockResult SetPaneAutoHide"
        "DockResult ResetPane"
        "ComputeDockLayout(")
    string(FIND "${MODEL}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "DockLayoutModel contract is missing: ${REQUIRED}")
    endif()
endforeach()
if(MODEL MATCHES "#include <windows.h>" OR MODEL MATCHES "HWND")
    message(FATAL_ERROR "Pure DockLayoutModel contains Win32 ownership")
endif()

file(READ "${RESOURCE_HEADER}" RESOURCE_IDS)
file(READ "${RESOURCE_SOURCE}" RESOURCE_TEXT)
foreach(REQUIRED IN ITEMS
        "IDS_DOCK_PANE_TOOL"
        "IDS_DOCK_PANE_TOOL_OPTIONS"
        "IDS_DOCK_PANE_COLOR"
        "IDS_DOCK_PANE_LAYER")
    string(FIND "${RESOURCE_IDS}" "${REQUIRED}" HEADER_OFFSET)
    string(FIND "${RESOURCE_TEXT}" "${REQUIRED}" SOURCE_OFFSET)
    if(HEADER_OFFSET LESS 0 OR SOURCE_OFFSET LESS 0)
        message(FATAL_ERROR "Dock pane title resource is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${HOST_SOURCE}" HOST)
foreach(REQUIRED IN ITEMS
        "WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME"
        "SetParent(pane.content, pane.floating_window)"
        "SetParent(pane.content, owner_)"
        "WM_DPICHANGED"
        "WM_CONTEXTMENU"
        "WM_CANCELMODE"
        "WM_GETMINMAXINFO"
        "WM_THEMECHANGED"
        "WM_SETTINGCHANGE"
        "LoadPaneTitle"
        "ShowDockPreview"
        "PreviewZoneAt"
        "DockSplitterKind::StackBoundary"
        "DockStackMode::Tabs")
    string(FIND "${HOST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "DockHost implementation is missing: ${REQUIRED}")
    endif()
endforeach()
if(HOST MATCHES "WS_EX_TOPMOST" OR HOST MATCHES "WS_EX_PALETTEWINDOW"
        OR HOST MATCHES "WS_EX_NOACTIVATE,[ \t\r\n]*kFloatingPaneClass")
    message(FATAL_ERROR "Floating primary pane uses a forbidden top-level style")
endif()

file(READ "${MAIN_HEADER}" MAIN_HEADER_TEXT)
if(NOT MAIN_HEADER_TEXT MATCHES "DockHost dock_host")
    message(FATAL_ERROR "WorkspaceWindow chrome does not own DockHost")
endif()
file(READ "${MAIN_SOURCE}" MAIN)
if(MAIN MATCHES "IDC_WORKSPACE_TOOL_SPLITTER"
        OR MAIN MATCHES "IDC_WORKSPACE_INSPECTOR_SPLITTER"
        OR MAIN MATCHES "IDC_WORKSPACE_COLOR_SPLITTER")
    message(FATAL_ERROR "Retired fixed-layout splitter geometry remains")
endif()

file(READ "${RUNTIME_SOURCE}" RUNTIME)
foreach(REQUIRED IN ITEMS
        "DockPaneType::Tool, state.Workspace().windows.tool_palette"
        "DockPaneType::ToolOptions, state.Workspace().windows.tool_options"
        "DockPaneType::Color, state.Workspace().windows.color_pane"
        "DockPaneType::Layer, state.Workspace().windows.layer_palette"
        "DockPaneType::Locator, state.Workspace().locator_palette"
        "DockPaneType::Sequence, state.Workspace().sequence_palette"
        "DockPaneType::LightTable, state.Workspace().light_table_palette"
        "DockPaneType::Reference, state.Workspace().subpalette_palette"
        "DockPaneType::Batch, state.Workspace().batch_palette"
        "DockPaneType::JobProgress, state.Workspace().job_progress"
        "NotifyDockHostChanged")
    string(FIND "${RUNTIME}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Primary pane DockHost integration is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${TEST_SOURCE}" TEST)
foreach(REQUIRED IN ITEMS
        "model.AddPane"
        "model.RemovePane"
        "model.MovePane"
        "model.TabPane"
        "model.FloatPane"
        "model.HidePane"
        "SetPaneAutoHide"
        "DockPaneType::JobProgress"
        "model.RestorePane"
        "model.ResetPane"
        "temporarily_auto_hidden"
        "high_dpi"
        "dpi_120"
        "dpi_192")
    string(FIND "${TEST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G7 pure-model test evidence is missing: ${REQUIRED}")
    endif()
endforeach()

message(STATUS
    "Verified pure bounded DockLayoutModel, WorkspaceWindow-owned DockHost, "
    "primary/auxiliary/job-pane docking integration, auto-hide, and removal "
    "of fixed geometry")
