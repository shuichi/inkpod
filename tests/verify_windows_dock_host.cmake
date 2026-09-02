if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(UI_DIR "${INKPOD_SOURCE_DIR}/apps/windows/ui")
set(MODEL_HEADER "${UI_DIR}/dock_layout.h")
set(MODEL_SOURCE "${UI_DIR}/dock_layout.cpp")
set(HOST_HEADER "${UI_DIR}/dock_host.h")
set(HOST_SOURCE "${UI_DIR}/dock_host.cpp")
set(TOOL_TABS_HEADER "${UI_DIR}/right_tool_tabs.h")
set(TOOL_TABS_SOURCE "${UI_DIR}/right_tool_tabs.cpp")
set(MAIN_HEADER "${UI_DIR}/main_window.h")
set(MAIN_SOURCE "${UI_DIR}/main_window.cpp")
set(RUNTIME_SOURCE "${UI_DIR}/main_window_runtime.cpp")
set(PROCEDURE_SOURCE "${UI_DIR}/main_window_procedure.cpp")
set(TEST_SOURCE "${INKPOD_SOURCE_DIR}/tests/windows_workspace_layout.cpp")
set(RESOURCE_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/app/resource.h")
set(RESOURCE_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui_ja.generated.rc")
set(LOCATOR_SOURCE "${UI_DIR}/panes/locator_pane.cpp")
set(SEQUENCE_SOURCE "${UI_DIR}/panes/sequence_pane.cpp")
set(LIGHT_TABLE_SOURCE "${UI_DIR}/panes/light_table_pane.cpp")
set(REFERENCE_SOURCE "${UI_DIR}/panes/subpalette_pane.cpp")
set(BATCH_SOURCE "${UI_DIR}/dialogs/batch_dialog.cpp")
set(BATCH_PARAMETER_SOURCE "${UI_DIR}/batch_parameter_editor.cpp")
set(LAYER_PALETTE_SOURCE "${UI_DIR}/dialogs/layer_palette.cpp")
set(COLOR_PANE_SOURCE "${UI_DIR}/panes/color_dock_pane.cpp")
set(PANE_DIALOG_LAYOUT_HEADER "${UI_DIR}/panes/pane_dialog_layout.h")
set(PANE_DIALOG_LAYOUT_TEST "${INKPOD_SOURCE_DIR}/tests/windows_pane_dialog_layout.cpp")
set(TAB_SURFACE_HEADER "${UI_DIR}/tab_surface_background.h")
set(PREFERENCES_SOURCE "${UI_DIR}/dialogs/preferences_dialog.cpp")
set(PROGRESS_SOURCE "${UI_DIR}/job_progress.cpp")
set(PROGRESS_HEADER "${UI_DIR}/job_progress.h")
set(FILE_PROGRESS_SOURCE "${UI_DIR}/file_job_progress.cpp")
set(FILE_IO_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/file_io_controller.cpp")
set(WORKSPACE_HEADER "${INKPOD_SOURCE_DIR}/apps/windows/app/workspace_window.h")
set(STATUS_SOURCE "${UI_DIR}/main_window_status_presenter.cpp")
set(PROGRESS_TEST "${INKPOD_SOURCE_DIR}/tests/windows_job_progress.cpp")
set(SMOKE_SOURCE "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp")

foreach(FILE IN ITEMS
        "${MODEL_HEADER}"
        "${MODEL_SOURCE}"
        "${HOST_HEADER}"
        "${HOST_SOURCE}"
        "${TOOL_TABS_HEADER}"
        "${TOOL_TABS_SOURCE}"
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
        "${BATCH_PARAMETER_SOURCE}"
        "${LAYER_PALETTE_SOURCE}"
        "${COLOR_PANE_SOURCE}"
        "${PANE_DIALOG_LAYOUT_HEADER}"
        "${PANE_DIALOG_LAYOUT_TEST}"
        "${TAB_SURFACE_HEADER}"
        "${PREFERENCES_SOURCE}"
        "${PROGRESS_SOURCE}"
        "${PROGRESS_HEADER}"
        "${FILE_PROGRESS_SOURCE}"
        "${FILE_IO_SOURCE}"
        "${WORKSPACE_HEADER}"
        "${STATUS_SOURCE}"
        "${PROGRESS_TEST}"
        "${SMOKE_SOURCE}")
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
        "${BATCH_PARAMETER_SOURCE}")
    file(READ "${FILE}" PANE_SOURCE_TEXT)
    string(APPEND PANE_IMPLEMENTATION "${PANE_SOURCE_TEXT}")
endforeach()
foreach(REQUIRED IN ITEMS
        "LayoutLocatorPane"
        "LayoutSequencePane"
        "LayoutLightTablePane"
        "LayoutSubpalettePaneDialog"
        "LayoutBatchPane")
    string(FIND "${PANE_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Docked modeless pane implementation is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${PROGRESS_HEADER}" PROGRESS_INTERFACE)
file(READ "${PROGRESS_SOURCE}" PROGRESS_IMPLEMENTATION)
file(READ "${FILE_PROGRESS_SOURCE}" FILE_PROGRESS_IMPLEMENTATION)
file(READ "${FILE_IO_SOURCE}" FILE_IO_IMPLEMENTATION)
file(READ "${WORKSPACE_HEADER}" WORKSPACE_INTERFACE)
file(READ "${STATUS_SOURCE}" STATUS_IMPLEMENTATION)
file(READ "${PROGRESS_TEST}" PROGRESS_TEST_TEXT)
foreach(REQUIRED IN ITEMS
        "enum class JobProgressSlot"
        "kMaximumFileJobProgress = 128U"
        "JobProgressIdentity selected"
        "JobProgressState"
        "PROGRESS_CLASSW"
        "PBM_SETMARQUEE"
        "PBM_SETPOS"
        "SetWindowSubclass(status, StatusProgressProcedure"
        "SelectJobProgress"
        "CancelJobProgress"
        "SetJobProgressIdleText")
    string(FIND "${PROGRESS_INTERFACE}${PROGRESS_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Bounded statusbar job progress contract is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "controller.CopyProgress(workspace, progress)"
        "JobProgressSource::FileIo, entry.request_id"
        "entry.context.generation.value_or(app::Generation{}).Value()"
        "entry.progress.completed_work"
        "entry.progress.total_work"
        "INKPOD_IO_RESULT_INSTALLING"
        "JobProgressPhase::Applying"
        "JobProgressPhase::Cancelling"
        "SetFileJobProgress(status_bar, state")
    string(FIND "${FILE_PROGRESS_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Cached file-job statusbar adapter contract is missing: ${REQUIRED}")
    endif()
endforeach()
string(REGEX REPLACE "[ \t\r\n]+" " " FILE_PROGRESS_COMPACT "${FILE_PROGRESS_IMPLEMENTATION}")
string(FIND "${FILE_PROGRESS_COMPACT}"
    "case INKPOD_IO_SEQUENCE_AUTO: case INKPOD_IO_SEQUENCE_FILES: return UiText(UiStringId::JobStatusSequence); case INKPOD_IO_SEQUENCE_SWITCH: return UiText(UiStringId::JobStatusCellLoading);"
    SEQUENCE_PROGRESS_NAMES)
if(SEQUENCE_PROGRESS_NAMES LESS 0)
    message(FATAL_ERROR
        "Automatic sequence discovery and individual cell loading must have distinct status text")
endif()
string(REPLACE "\r\n" "\n" FILE_IO_IMPLEMENTATION "${FILE_IO_IMPLEMENTATION}")
string(FIND "${FILE_IO_IMPLEMENTATION}" "std::size_t FileIoController::CopyProgress(" COPY_BEGIN)
if(COPY_BEGIN LESS 0 OR NOT FILE_IO_IMPLEMENTATION MATCHES "kMaximumJobs = 128U")
    message(FATAL_ERROR "Bounded FileIoController progress cache is missing")
endif()
string(SUBSTRING "${FILE_IO_IMPLEMENTATION}" ${COPY_BEGIN} -1 COPY_TAIL)
string(FIND "${COPY_TAIL}" "\n}\n" COPY_END)
if(COPY_END LESS 0)
    message(FATAL_ERROR "FileIoController progress cache method boundary is missing")
endif()
string(SUBSTRING "${COPY_TAIL}" 0 ${COPY_END} COPY_IMPLEMENTATION)
foreach(REQUIRED IN ITEMS
        "std::span<FileIoProgressEntry> output"
        "copied == output.size()"
        "pending.request.context.workspace != workspace"
        "value.progress = pending.progress"
        "pending.cancelled.load")
    string(FIND "${COPY_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Workspace-scoped cached progress copy is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(FORBIDDEN IN ITEMS "inkpod_io_" "Invoke(" "CreateFile" "ReadFile" "WriteFile")
    string(FIND "${COPY_IMPLEMENTATION}${PROGRESS_IMPLEMENTATION}${FILE_PROGRESS_IMPLEMENTATION}"
        "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Statusbar progress must not poll Rust or perform file I/O on UI: ${FORBIDDEN}")
    endif()
endforeach()
foreach(FORBIDDEN IN ITEMS "read_count" "loaded_count" ".Poll(")
    string(FIND "${FILE_PROGRESS_IMPLEMENTATION}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Statusbar file progress must copy work units from cache: ${FORBIDDEN}")
    endif()
endforeach()
if(NOT WORKSPACE_INTERFACE MATCHES "JobProgressState job_progress_state"
        OR NOT STATUS_IMPLEMENTATION MATCHES "SetJobProgressIdleText\\(status_bar, text\\)")
    message(FATAL_ERROR "Workspace-owned job progress / normal statusbar text connection is missing")
endif()
foreach(REQUIRED IN ITEMS
        "InitializeJobProgress"
        "BindJobProgress"
        "SetFileJobProgress"
        "PBM_GETPOS"
        "old_task"
        "state.marquee"
        "CancelJobProgress"
        "state.visible"
        "SB_GETTEXTW")
    string(FIND "${PROGRESS_TEST_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Real HWND statusbar job-progress regression evidence is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${LOCATOR_SOURCE}" LOCATOR_IMPLEMENTATION)
file(READ "${SEQUENCE_SOURCE}" SEQUENCE_IMPLEMENTATION)
foreach(REQUIRED IN ITEMS
        "kMaximumSequenceRowPixels = 255"
        "LB_SETCOLUMNWIDTH"
        "LB_SETITEMHEIGHT"
        "LB_GETTOPINDEX"
        "SameSequenceCell"
        "SelectCommittedCell"
        "item_labels != state->item_labels"
        "WM_MOUSEHWHEEL"
        "VK_LEFT"
        "VK_RIGHT"
        "PaneDialogLayoutPlan plan(dialog)"
        "PaneDialogRepaint::None"
        "CompletePaneDialogResize(dialog)")
    string(FIND "${SEQUENCE_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Horizontal sequence pane contract is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "std::wstring_view(presented) == next"
        "replacement.size() < presented.size()"
        "PaneDialogLayoutPlan plan(dialog)"
        "plan.Commit(PaneDialogRepaint::Complete)")
    string(FIND "${LOCATOR_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Locator idempotent/shorter-text repaint contract is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${MODEL_HEADER}" MODEL)
foreach(REQUIRED IN ITEMS
        "class DockLayoutModel final"
        "struct PaneDescriptor"
        "title_resource_id"
        "show_header_when_singleton"
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
string(FIND "${RESOURCE_TEXT}" "IDD_SEQUENCE_PALETTE DIALOGEX" SEQUENCE_BEGIN)
string(FIND "${RESOURCE_TEXT}" "IDD_LIGHT_TABLE_PALETTE DIALOGEX" SEQUENCE_END)
if(SEQUENCE_BEGIN LESS 0 OR SEQUENCE_END LESS_EQUAL SEQUENCE_BEGIN)
    message(FATAL_ERROR "Sequence pane resource boundaries are missing")
endif()
math(EXPR SEQUENCE_LENGTH "${SEQUENCE_END} - ${SEQUENCE_BEGIN}")
string(SUBSTRING "${RESOURCE_TEXT}" ${SEQUENCE_BEGIN} ${SEQUENCE_LENGTH} SEQUENCE_RESOURCE)
foreach(REQUIRED IN ITEMS "LBS_MULTICOLUMN" "LBS_HASSTRINGS" "LBS_DISABLENOSCROLL" "WS_HSCROLL")
    string(FIND "${SEQUENCE_RESOURCE}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Horizontal sequence list style is missing: ${REQUIRED}")
    endif()
endforeach()
if(SEQUENCE_RESOURCE MATCHES "IDC_SEQUENCE_PREVIOUS|IDC_SEQUENCE_NEXT|WS_VSCROLL")
    message(FATAL_ERROR "Sequence pane retains vertical scrolling or navigation buttons")
endif()
foreach(REQUIRED IN ITEMS
        "IDS_DOCK_PANE_TOOL"
        "IDS_DOCK_PANE_TOOL_OPTIONS"
        "IDS_DOCK_PANE_COLOR"
        "IDS_DOCK_PANE_LAYER"
        "IDS_LAYER_ACTION_TARGET_LAYER"
        "IDS_LAYER_ACTION_TARGET_PLANE"
        "IDS_LAYER_PLANE_SPLITTER")
    string(FIND "${RESOURCE_IDS}" "${REQUIRED}" HEADER_OFFSET)
    string(FIND "${RESOURCE_TEXT}" "${REQUIRED}" SOURCE_OFFSET)
    if(HEADER_OFFSET LESS 0 OR SOURCE_OFFSET LESS 0)
        message(FATAL_ERROR "Dock pane title resource is missing: ${REQUIRED}")
    endif()
endforeach()

string(FIND "${RESOURCE_TEXT}" "IDD_LIGHT_TABLE_PALETTE DIALOGEX" LIGHT_TABLE_BEGIN)
string(FIND "${RESOURCE_TEXT}" "IDD_SUBPALETTE_PALETTE DIALOGEX" LIGHT_TABLE_END)
if(LIGHT_TABLE_BEGIN LESS 0 OR LIGHT_TABLE_END LESS 0
        OR LIGHT_TABLE_END LESS_EQUAL LIGHT_TABLE_BEGIN)
    message(FATAL_ERROR "Light Table pane resource boundaries are missing")
endif()
math(EXPR LIGHT_TABLE_LENGTH "${LIGHT_TABLE_END} - ${LIGHT_TABLE_BEGIN}")
string(SUBSTRING
    "${RESOURCE_TEXT}" ${LIGHT_TABLE_BEGIN} ${LIGHT_TABLE_LENGTH} LIGHT_TABLE_RESOURCE)
if(LIGHT_TABLE_RESOURCE MATCHES "\"閉じる\"[^\r\n]*IDCANCEL")
    message(FATAL_ERROR
        "Tabbed Light Table pane retains a pane-local Close button")
endif()

file(READ "${LIGHT_TABLE_SOURCE}" LIGHT_TABLE_IMPLEMENTATION)
if(LIGHT_TABLE_IMPLEMENTATION MATCHES "const int close_width")
    message(FATAL_ERROR
        "Light Table layout still reserves space for a pane-local Close button")
endif()
foreach(REQUIRED IN ITEMS "case IDCANCEL:" "case WM_CLOSE:")
    string(FIND "${LIGHT_TABLE_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Light Table floating/keyboard close route is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${HOST_HEADER}" HOST_INTERFACE)
file(READ "${HOST_SOURCE}" HOST)
foreach(REQUIRED IN ITEMS
        "enum class DockHostChangeKind"
        "DockHostChangeKind::Geometry"
        "DockHostChangeKind::StackBoundary"
        "DockHostChangeKind::Structure")
    string(FIND "${HOST_INTERFACE}${HOST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "DockHost change classification is missing: ${REQUIRED}")
    endif()
endforeach()

foreach(REQUIRED IN ITEMS
        "PaneDialogLayoutPlan plan(dialog)"
        "plan.Commit(PaneDialogRepaint::Complete)")
    string(FIND "${LIGHT_TABLE_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Light Table atomic resize repaint contract is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME"
        "SetParent(pane.content, pane.floating_window)"
        "SetParent(pane.content, owner_)"
        "WM_DPICHANGED"
        "WM_CONTEXTMENU"
        "WM_CANCELMODE"
        "WM_GETMINMAXINFO"
        "WM_THEMECHANGED"
        "WM_SYSCOLORCHANGE"
        "WM_SETTINGCHANGE"
        "ShouldShowStackHeader"
        "HeaderWindow"
        "PaintSplitter"
        "TrackMouseEvent"
        "COLOR_3DSHADOW"
        "LoadPaneTitle"
        "ShowDockPreview"
        "PreviewZoneAt"
        "DockSplitterKind::StackBoundary"
        "class DockHost::PlacementBatch final"
        "ScopedWindowRedrawSuspension"
        "ScopedPaneDialogResizeDeferral"
        "GetParent(pane.content) == owner_"
        "placement->zone != DockZone::Floating"
        "placement->zone == DockZone::AutoHide"
        "pane.auto_hide_expanded"
        "BeginPaneDialogLayoutTransaction"
        "PaneDialogLayoutFailed"
        "LayoutsSucceeded"
        "const bool synchronize_tab_metrics"
        "synchronize_items || synchronize_tab_metrics"
        "PrepareRedrawSuspension"
        "BeginDeferWindowPos"
        "DeferWindowPos"
        "EndDeferWindowPos"
        "SWP_NOREDRAW | SWP_NOCOPYBITS"
        "CapturePreviousState"
        "HasFinalState"
        "RestorePreviousState"
        "placement.previous_show"
        "placements.IncludeDirty(previous_geometry.right_tool_tabs)"
        "pane_resize_deferral.Restore()"
        "pane_resize_deferred = pane_resize_deferral.Defer(pane.content)"
        "placements.Redraw()"
        "overflowed_ = true"
        "bool geometry_committed = placements.Commit()"
        "geometry_ = previous_geometry"
        "dpi_ = previous_dpi"
        "RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_UPDATENOW"
        "ApplyToolTabLayout"
        "ToolTabSubclassProcedure"
        "MovePaneToToolTab"
        "DockMovePaneToTab"
        "DockClose"
        "IDC_RIGHT_TOOL_TAB_CLOSE"
        "BS_OWNERDRAW"
        "WM_DRAWITEM"
        "BN_CLICKED"
        "CloseToolTab"
        "SynchronizeToolTabCloseButtons"
        "LayoutPaneTabCloseButtons"
        "PaneTabCloseButtonSubclassProcedure"
        "host.HidePane(pane.type)"
        "LoadToolTabTitle"
        "LoadToolTabDescription"
        "ExceedsDragThreshold"
        "RedrawSplitterNow"
        "splitter->focused = false"
        "UpdateTabFont(GetDpiForWindow(owner_))"
        "UpdateTabFont(dpi_)"
        "WM_SETFONT"
        "CLEARTYPE_QUALITY"
        "Segoe UI")
    string(FIND "${HOST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "DockHost implementation is missing: ${REQUIRED}")
    endif()
endforeach()
string(REGEX MATCH
    "void DockHost::CancelLayoutMutation\\(\\) noexcept \\{[^}]*RestoreLayoutMutation\\(\\);[^}]*\\}"
    CANCEL_RESTORES_MUTATION
    "${HOST}")
if(CANCEL_RESTORES_MUTATION STREQUAL "")
    message(FATAL_ERROR
        "DockHost cancellation must restore the complete model/tab snapshot")
endif()

file(READ "${TOOL_TABS_HEADER}" TOOL_TABS_MODEL)
file(READ "${TOOL_TABS_SOURCE}" TOOL_TABS_IMPLEMENTATION)
foreach(REQUIRED IN ITEMS
        "class RightToolTabsModel final"
        "struct ToolTab final"
        "ToolTabId id"
        "kMaximumToolTabs"
        "pane_count"
        "ToolTabId Selected"
        "ToolTabResult AddPaneToSelected"
        "ToolTabResult MovePane"
        "ToolTabResult MovePaneToNewTab"
        "ToolTabResult CloseTab"
        "ToolTabResult ReorderPane"
        "ToolTabResult Reorder")
    string(FIND "${TOOL_TABS_MODEL}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Right tool-tab model is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "FitsSelected"
        "CreateTab"
        "RemoveTab"
        "selected_ = tabs_[index - 1U].id"
        "RightToolTabsModel::CloseTab"
        "RightToolTabsModel candidate")
    string(FIND "${TOOL_TABS_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Right tool-tab implementation is missing: ${REQUIRED}")
    endif()
endforeach()
if(TOOL_TABS_IMPLEMENTATION MATCHES "fallback_title"
        OR TOOL_TABS_IMPLEMENTATION MATCHES "UiText")
    message(FATAL_ERROR
        "Right tool-tab model must remain independent of localized presentation")
endif()
foreach(FORBIDDEN IN ITEMS
        "kToolTabColoring"
        "kToolTabReference"
        "kToolTabWorkflow"
        "SetVisible")
    string(FIND "${TOOL_TABS_MODEL}${TOOL_TABS_IMPLEMENTATION}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Fixed right tool-tab contract remains: ${FORBIDDEN}")
    endif()
endforeach()
if(HOST MATCHES "WS_EX_TOPMOST" OR HOST MATCHES "WS_EX_PALETTEWINDOW"
        OR HOST MATCHES "WS_EX_NOACTIVATE,[ \t\r\n]*kFloatingPaneClass")
    message(FATAL_ERROR "Floating primary pane uses a forbidden top-level style")
endif()

file(READ "${LAYER_PALETTE_SOURCE}" LAYER_PALETTE)
foreach(REQUIRED IN ITEMS
        "IDC_LAYER_ACTION_TARGET"
        "UpdateActionTargetPresentation"
        "split_dragging"
        "state->split_milli = state->split_drag_initial"
        "WM_CAPTURECHANGED"
        "WM_CANCELMODE"
        "WM_PAINT"
        "WM_MOUSELEAVE"
        "WM_GETDLGCODE"
        "PaneDialogLayoutPlan plan(dialog)"
        "plan.Commit(panes::PaneDialogRepaint::None)"
        "CompletePaneDialogResize(dialog)"
        "IDS_LAYER_PLANE_SPLITTER")
    string(FIND "${LAYER_PALETTE}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Layer/Plane pane presentation is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${COLOR_PANE_SOURCE}" COLOR_PANE)
file(READ "${PANE_DIALOG_LAYOUT_HEADER}" PANE_DIALOG_LAYOUT)
file(READ "${PANE_DIALOG_LAYOUT_TEST}" PANE_DIALOG_LAYOUT_TEST_TEXT)
file(READ "${TAB_SURFACE_HEADER}" TAB_SURFACE)
foreach(REQUIRED IN ITEMS
        "EnablePaneDialogResizePainting"
        "WS_CLIPCHILDREN"
        "class PaneDialogLayoutPlan final"
        "kPaneDialogLayoutCapacity = 64U"
        "PaneWindowHasBounds"
        "FinalizePaneTabPageZOrder"
        "keyboard traversal order"
        "HWND_BOTTOM"
        "BeginDeferWindowPos"
        "DeferWindowPos"
        "EndDeferWindowPos"
        "SWP_NOREDRAW"
        "SWP_NOCOPYBITS"
        "SetPaneDialogResizeDeferred"
        "IsPaneDialogResizeDeferred"
        "overflowed_"
        "CompletePaneDialogResize"
        "EnumChildWindows"
        "RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_NOCHILDREN"
        "RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_UPDATENOW"
        "RDW_ALLCHILDREN"
        "class ScopedPaneControlRedrawSuspension final"
        "WM_SETREDRAW")
    string(FIND "${PANE_DIALOG_LAYOUT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Pane dialog resize repaint helper is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "changed placement or unchanged skip"
        "intermediate painting"
        "single final subtree repaint"
        "outer defer allowed an intermediate repaint"
        "deferred final repaint left an update region"
        "plan helper overloads"
        "tab page final z-order"
        "hidden-parent tab page final z-order"
        "fixture unexpectedly clips children before layout"
        "invalid plan published partial geometry"
        "overflow plan published partial geometry"
        "control metric redraw guard"
        "system-normalized combo height")
    string(FIND "${PANE_DIALOG_LAYOUT_TEST_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Real-HWND pane resize transaction evidence is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(FILE IN ITEMS
        "${LOCATOR_SOURCE}"
        "${SEQUENCE_SOURCE}"
        "${LIGHT_TABLE_SOURCE}"
        "${REFERENCE_SOURCE}"
        "${BATCH_SOURCE}"
        "${BATCH_PARAMETER_SOURCE}"
        "${LAYER_PALETTE_SOURCE}")
    file(READ "${FILE}" ATOMIC_PANE_IMPLEMENTATION)
    foreach(REQUIRED IN ITEMS "PaneDialogLayoutPlan")
        string(FIND "${ATOMIC_PANE_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
        if(OFFSET LESS 0)
            message(FATAL_ERROR
                "Right-pane layout is outside the shared resize transaction (${FILE}): ${REQUIRED}")
        endif()
    endforeach()
    string(FIND "${ATOMIC_PANE_IMPLEMENTATION}"
        "PaneDialogRepaint::Complete" COMPLETE_ENUM_OFFSET)
    string(FIND "${ATOMIC_PANE_IMPLEMENTATION}"
        "CompletePaneDialogResize" COMPLETE_HELPER_OFFSET)
    if(COMPLETE_ENUM_OFFSET LESS 0 AND COMPLETE_HELPER_OFFSET LESS 0)
        message(FATAL_ERROR
            "Right-pane layout has no final repaint completion (${FILE})")
    endif()
endforeach()
file(READ "${BATCH_PARAMETER_SOURCE}" BATCH_PARAMETER_IMPLEMENTATION)
foreach(REQUIRED IN ITEMS
        "GetWindowLongPtrW(control, GWL_STYLE) & WS_VISIBLE"
        "const int maximum_scroll = std::max(0, content - viewport)"
        "std::clamp(state.scroll_y, 0, maximum_scroll)"
        "SetScrollInfo(window, SB_VERT, &scroll, FALSE)"
        "const std::array<HWND, 27U> controls"
        "return -1;")
    string(FIND "${BATCH_PARAMETER_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Batch parameter resize/create contract is missing: ${REQUIRED}")
    endif()
endforeach()
file(READ "${BATCH_SOURCE}" BATCH_IMPLEMENTATION)
foreach(REQUIRED IN ITEMS
        "state->parameter_host == nullptr"
        "IsWindow(dialog) == FALSE"
        "SetWindowSubclass(")
    string(FIND "${BATCH_IMPLEMENTATION}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Batch pane creation failure contract is missing: ${REQUIRED}")
    endif()
endforeach()
file(READ "${PREFERENCES_SOURCE}" PREFERENCES)
foreach(REQUIRED IN ITEMS
        "InvalidateRect(picker, nullptr, FALSE)"
        "PaneDialogLayoutPlan plan(pane)"
        "PlacePaneTargetRow"
        "PlacePaneButtonRows"
        "plan.Commit(PaneDialogRepaint::None)"
        "FinalizePaneTabPageZOrder"
        "CompletePaneDialogResize(pane)"
        "WM_ERASEBKGND"
        "PaintTabSurfaceBackground"
        "CaptureColorTabSurfacePixels"
        "state.swatch_paint_buffer"
        "state.picker_paint_buffer"
        "ColorDockTabId"
        "palette_redraw_suspended"
        "chart_redraw_suspended"
        "TCIF_TEXT | TCIF_PARAM"
        "ColorTabSubclassProcedure"
        "ReorderColorTab"
        "GetSystemMetrics(SM_CXDRAG)"
        "GetSystemMetrics(SM_CYDRAG)")
    string(FIND "${COLOR_PANE}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Color pane bounded resize repaint is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(FORBIDDEN IN ITEMS
        "CapturePaneTargetRowBounds"
        "RepaintMovedPaneTargetRow"
        "RepaintVisibleTabControls"
        "UpdateWindow(child)")
    string(FIND "${COLOR_PANE}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR
            "Color pane retains an ad-hoc resize repaint path: ${FORBIDDEN}")
    endif()
endforeach()
string(REGEX REPLACE "[ \t\r\n]+" " " HOST_COMPACT "${HOST}")
string(FIND "${HOST_COMPACT}"
    "tab_redraw_suspension.Restore();" TAB_RESTORE_OFFSET)
string(FIND "${HOST_COMPACT}"
    "bool geometry_committed = placements.Commit();" PLACEMENT_COMMIT_OFFSET)
if(TAB_RESTORE_OFFSET LESS 0 OR PLACEMENT_COMMIT_OFFSET LESS 0
        OR NOT TAB_RESTORE_OFFSET LESS PLACEMENT_COMMIT_OFFSET)
    message(FATAL_ERROR
        "DockHost must restore WM_SETREDRAW before final show/hide placement")
endif()
foreach(REQUIRED IN ITEMS
        "PaintTabSurfaceBackground"
        "WM_PRINTCLIENT"
        "PRF_CLIENT | PRF_ERASEBKGND")
    string(FIND "${TAB_SURFACE}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Shared themed tab-surface background helper is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "PaintTabSurfaceBackground(tabs, control, context, client)"
        "GetStockObject(HOLLOW_BRUSH)"
        "WS_EX_CLIENTEDGE"
        "WM_THEMECHANGED"
        "WM_SYSCOLORCHANGE")
    string(FIND "${PREFERENCES}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Preferences tab-label themed background integration is missing: ${REQUIRED}")
    endif()
endforeach()

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
foreach(REQUIRED IN ITEMS
        "AttachDocumentTabFont"
        "DocumentTabFontSubclassProcedure"
        "WM_DPICHANGED_AFTERPARENT"
        "WM_SETFONT"
        "CLEARTYPE_QUALITY"
        "Segoe UI")
    string(FIND "${MAIN}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Document tab standard UI font integration is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${RUNTIME_SOURCE}" RUNTIME)
foreach(REQUIRED IN ITEMS
        "DockPaneType::Tool, state.Workspace().windows.tool_palette"
        "DockPaneType::Color, state.Workspace().windows.color_pane"
        "DockPaneType::Layer, state.Workspace().windows.layer_palette"
        "DockPaneType::Locator, state.Workspace().locator_palette"
        "DockPaneType::Sequence, state.Workspace().sequence_palette"
        "DockPaneType::LightTable, state.Workspace().light_table_palette"
        "DockPaneType::Reference, state.Workspace().subpalette_palette"
        "DockPaneType::Batch, state.Workspace().batch_palette"
        "WM_SYSCOLORCHANGE"
        "RDW_ALLCHILDREN"
        "if (kind == DockHostChangeKind::Structure)"
        "RelayoutWorkspace(*state, kind)"
        "NotifyDockHostChanged")
    string(FIND "${RUNTIME}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Primary pane DockHost integration is missing: ${REQUIRED}")
    endif()
endforeach()
string(FIND "${RUNTIME}" "bool NotifyDockHostChanged(" DOCK_NOTIFY_BEGIN)
string(FIND "${RUNTIME}" "bool InitializeMainChrome(" DOCK_NOTIFY_END)
if(DOCK_NOTIFY_BEGIN LESS 0 OR DOCK_NOTIFY_END LESS_EQUAL DOCK_NOTIFY_BEGIN)
    message(FATAL_ERROR "DockHost notification boundary is missing")
endif()
math(EXPR DOCK_NOTIFY_LENGTH "${DOCK_NOTIFY_END} - ${DOCK_NOTIFY_BEGIN}")
string(SUBSTRING "${RUNTIME}" ${DOCK_NOTIFY_BEGIN} ${DOCK_NOTIFY_LENGTH} DOCK_NOTIFY)
foreach(FORBIDDEN IN ITEMS
        "RefreshColorPanes"
        "RefreshDockPaneViews"
        "RefreshTreePane")
    string(FIND "${DOCK_NOTIFY}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR
            "Geometry/structure notification must not rebuild surviving pane contents: ${FORBIDDEN}")
    endif()
endforeach()

string(REGEX REPLACE "[ \t\r\n]+" " " RUNTIME_COMPACT "${RUNTIME}")
string(FIND "${RUNTIME}" "InkpodStatus QueueResolvedSequenceReplacement(" SEQUENCE_REPLACE_BEGIN)
string(FIND "${RUNTIME}" "InkpodStatus SwitchSequenceTarget(" SEQUENCE_REPLACE_END)
if(SEQUENCE_REPLACE_BEGIN LESS 0 OR SEQUENCE_REPLACE_END LESS_EQUAL SEQUENCE_REPLACE_BEGIN)
    message(FATAL_ERROR "Sequence replacement status boundary is missing")
endif()
math(EXPR SEQUENCE_REPLACE_LENGTH "${SEQUENCE_REPLACE_END} - ${SEQUENCE_REPLACE_BEGIN}")
string(SUBSTRING "${RUNTIME}" ${SEQUENCE_REPLACE_BEGIN} ${SEQUENCE_REPLACE_LENGTH}
    SEQUENCE_REPLACE_IMPLEMENTATION)
string(REGEX REPLACE "[ \t\r\n]+" " " SEQUENCE_REPLACE_COMPACT
    "${SEQUENCE_REPLACE_IMPLEMENTATION}")
string(FIND "${SEQUENCE_REPLACE_COMPACT}"
    "if (source_recovery_required) { PresentStatusBarPart( workspace->windows.status_bar, 5U, UiText(UiStringId::Text0227)); }"
    SEQUENCE_RECOVERY_STATUS)
if(SEQUENCE_RECOVERY_STATUS LESS 0)
    message(FATAL_ERROR
        "Only source recovery may present an immediate sequence-switch status")
endif()
foreach(FORBIDDEN IN ITEMS
        "UiText(UiStringId::Text0226)"
        "source_recovery_required ?")
    string(FIND "${SEQUENCE_REPLACE_COMPACT}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR
            "Ordinary cached cell switches must wait for polled job progress: ${FORBIDDEN}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "InitializeJobProgress( state.Workspace().windows.status_bar, state.Workspace().job_progress_state,"
        "static_cast<inkpod::app::FileIoController*>(context)->Cancel(request_id)"
        "timer_id == inkpod::app::kFileIoPollTimer"
        "auto* owner = state->WorkspaceForWindow(window);"
        "const WorkspaceWindowId workspace_id = owner->id;"
        "state->file_io.Poll(); DrainSequenceSwitchCompletion(*state, window); owner = state->FindWorkspace(workspace_id); if (owner != nullptr && owner->windows.window == window) { RefreshFileJobProgress(state->file_io, workspace_id, owner->windows.status_bar, owner->job_progress_state); }"
        "std::lock_guard lock(state.routing.sequence_switch_results_mutex); const auto& result = state.routing.sequence_switch_result;"
        "if (completion_token != 0U) { (void)RouteCoreNotificationMessage(&state, window, kSequenceSwitchCompleted, static_cast<WPARAM>(completion_token), static_cast<LPARAM>(completion_generation)); }"
        "token->context.workspace == workspace_id && ResolveCommandTimer(*state, window, timer_id).has_value()"
        "RefreshJobProgress(owner->windows.status_bar, owner->job_progress_state)")
    string(FIND "${RUNTIME_COMPACT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Window-scoped statusbar progress polling/cancel integration is missing: ${REQUIRED}")
    endif()
endforeach()
file(READ "${PROCEDURE_SOURCE}" PROCEDURE)
string(REGEX REPLACE "[ \t\r\n]+" " " PROCEDURE_COMPACT "${PROCEDURE}")
string(FIND "${PROCEDURE_COMPACT}"
    "if (message == WM_TIMER) { if (const auto result = RouteCachedProgressTimerMessage( application, window, wparam)) { return *result; } }"
    CACHED_TIMER_OFFSET)
string(FIND "${PROCEDURE_COMPACT}" "application->ActivateWorkspaceWindow(" ACTIVATION_OFFSET)
if(CACHED_TIMER_OFFSET LESS 0 OR ACTIVATION_OFFSET LESS 0
        OR NOT CACHED_TIMER_OFFSET LESS ACTIVATION_OFFSET)
    message(FATAL_ERROR "Cached progress timers must run before workspace/Core activation")
endif()
foreach(FORBIDDEN IN ITEMS
        "DockPaneType::JobProgress"
        "IDM_WINDOW_JOB_PROGRESS"
        "IDS_PANE_JOB_PROGRESS"
        "IDC_JOB_PROGRESS_EMPTY"
        "IDD_EFFECT_PROGRESS"
        "CreateJobProgressPane")
    string(FIND "${MODEL}${RESOURCE_IDS}${RESOURCE_TEXT}${RUNTIME}${WORKSPACE_INTERFACE}${PANE_IMPLEMENTATION}"
        "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR "Retired job progress pane must not remain in production UI: ${FORBIDDEN}")
    endif()
endforeach()

file(READ "${SMOKE_SOURCE}" SMOKE)
foreach(REQUIRED IN ITEMS
        "right_zone_splitter"
        "TCM_DELETEALLITEMS"
        "WindowMessageCounter erase_probe{WM_ERASEBKGND}"
        "VerifyColorPinResizeRepaint"
        "VerifyHorizontalSequenceLayout"
        "sequence_list_rebuilds"
        "color_list_rebuilds"
        "layer_list_rebuilds"
        "layer_list_resize_paints.count != 0U"
        "layer_list_shrink_painted"
        "layer_list_grow_painted"
        "color_owner_draw_backgrounds_match_tab"
        "color_resize_controls"
        "color_resize_paints"
        "minimum_color_content_height"
        "right_stack_splitter"
        "stack_grow_completed"
        "stack_restore_completed"
        "structure_add_ok"
        "structure_remove_ok"
        "structure_color_resets"
        "structure_layer_resets"
        "structure_color_child_paints.count != 0U"
        "structure_layer_list_paints.count != 0U"
        "resize_sentinel_cleared"
        "rejected_command == 0"
        "ChildWindowFromPointEx("
        "CWP_SKIPDISABLED | CWP_SKIPINVISIBLE | CWP_SKIPTRANSPARENT"
        "IDM_WINDOW_SUBPALETTE")
    string(FIND "${SMOKE}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Right splitter continuous-resize regression evidence is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "enum class AuxiliaryPaneVisibilityChange"
        "AuxiliaryPaneVisibilityChange::Failed"
        "change == AuxiliaryPaneVisibilityChange::Failed")
    string(FIND "${RUNTIME}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR
            "Auxiliary-pane command failure propagation is missing: ${REQUIRED}")
    endif()
endforeach()
foreach(REQUIRED IN ITEMS
        "CreateToolOptionsFlyout"
        "ToggleToolOptionsFlyout"
        "state.Workspace().windows.tool_options_flyout")
    string(FIND "${RUNTIME}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Tool options flyout integration is missing: ${REQUIRED}")
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
        "RetiredJobProgressPaneIsAbsent"
        "show_header_when_singleton"
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
    "primary/auxiliary docking integration, workspace-scoped cached statusbar progress, "
    "nested right-pane resize transactions, auto-hide, and removal of fixed geometry "
    "and the job progress pane")
