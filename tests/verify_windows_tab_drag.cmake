if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(APP_DIR "${INKPOD_SOURCE_DIR}/apps/windows/app")
set(UI_DIR "${INKPOD_SOURCE_DIR}/apps/windows/ui")

foreach(path IN ITEMS
        "${APP_DIR}/tab_drag.h"
        "${APP_DIR}/tab_drag.cpp"
        "${UI_DIR}/tab_drag.h"
        "${UI_DIR}/tab_drag.cpp")
    if(NOT EXISTS "${path}")
        message(FATAL_ERROR "G11 tab drag source is missing: ${path}")
    endif()
endforeach()

file(READ "${APP_DIR}/application_host.h" HOST_HEADER)
file(READ "${APP_DIR}/application_host.cpp" HOST_SOURCE)
file(READ "${APP_DIR}/tab_drag.h" MODEL_HEADER)
file(READ "${UI_DIR}/tab_drag.cpp" UI_SOURCE)
file(READ "${UI_DIR}/tab_drag.h" UI_HEADER)
file(READ "${UI_DIR}/main_window.cpp" MAIN_SOURCE)
file(READ "${UI_DIR}/main_window_runtime.cpp" RUNTIME_SOURCE)
file(READ "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp" SMOKE_SOURCE)
file(READ "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui_ja.generated.rc" RESOURCE_SOURCE)

foreach(required IN ITEMS
        "TabDragCoordinator tab_drag_"
        "MoveDocumentView\\("
        "TabDrag\\(\\)")
    if(NOT HOST_HEADER MATCHES "${required}")
        message(FATAL_ERROR "ApplicationHost is missing G11 ownership/API: ${required}")
    endif()
endforeach()

foreach(required IN ITEMS
        "TabDropKind::Reorder"
        "TabDropKind::EditorGroup"
        "TabDropKind::TearOut"
        "restore_context"
        "DragOperation::TabCopy")
    if(NOT MODEL_HEADER MATCHES "${required}" AND NOT UI_SOURCE MATCHES "${required}")
        message(FATAL_ERROR "G11 value-token/rollback contract is missing: ${required}")
    endif()
endforeach()

foreach(required IN ITEMS
        "SetWindowSubclass"
        "WM_CAPTURECHANGED"
        "WM_CONTEXTMENU"
        "WindowFromPoint"
        "ImageList_BeginDrag"
        "GetSystemMetrics\\(SM_CXDRAG\\)"
        "CreateDocumentViewInGroup"
        "MoveOrDuplicateViewToNewWorkspace"
        "state.MoveDocumentView")
    if(NOT UI_SOURCE MATCHES "${required}")
        message(FATAL_ERROR "G11 Common Controls drag route is missing: ${required}")
    endif()
endforeach()

if(UI_SOURCE MATCHES "PostMessageW?\\([^\n]*(DragToken|TabDropRequest)"
        OR UI_SOURCE MATCHES "reinterpret_cast<LPARAM>\\([^\n]*(DragToken|TabDropRequest)")
    message(FATAL_ERROR "G11 must not post raw drag object pointers")
endif()

if(NOT MAIN_SOURCE MATCHES "AttachDocumentTabDrag"
        OR NOT RUNTIME_SOURCE MATCHES "CancelDocumentTabDrag"
        OR NOT RUNTIME_SOURCE MATCHES "WM_DPICHANGED")
    message(FATAL_ERROR "tab HWND attach or DPI/window cancellation is missing")
endif()

foreach(required IN ITEMS
        "SyncDocumentTabCloseButtons"
        "IDC_DOCUMENT_TAB_CLOSE"
        "BS_OWNERDRAW"
        "WM_DRAWITEM"
        "BN_CLICKED"
        "IDM_VIEW_CLOSE"
        "WM_DPICHANGED_AFTERPARENT"
        "WM_THEMECHANGED"
        "WM_SYSCOLORCHANGE")
    if(NOT UI_HEADER MATCHES "${required}"
            AND NOT UI_SOURCE MATCHES "${required}"
            AND NOT RUNTIME_SOURCE MATCHES "${required}")
        message(FATAL_ERROR
            "document-tab close-button contract is missing: ${required}")
    endif()
endforeach()

if(NOT RUNTIME_SOURCE MATCHES "SyncDocumentTabCloseButtons"
        OR NOT UI_SOURCE MATCHES "DocumentViewId"
        OR UI_SOURCE MATCHES "PostMessageW?\\([^\n]*DocumentViewId")
    message(FATAL_ERROR
        "document-tab close buttons must synchronize and route by value identity")
endif()

foreach(command IN ITEMS
        IDM_TAB_MOVE_LEFT
        IDM_TAB_MOVE_RIGHT
        IDM_VIEW_MOVE_NEXT_WINDOW
        IDM_VIEW_DUPLICATE_NEXT_WINDOW)
    if(NOT RESOURCE_SOURCE MATCHES "${command}")
        message(FATAL_ERROR "G11 keyboard/menu fallback is missing: ${command}")
    endif()
endforeach()

foreach(required IN ITEMS
        "RunTabDragSmoke"
        "WM_CAPTURECHANGED"
        "WM_DPICHANGED"
        "effects.task"
        "batch.job_id"
        "IDM_TAB_MOVE_LEFT"
        "IDM_EDITOR_MOVE_OTHER_GROUP"
        "IDM_WORKSPACE_NEW_WINDOW"
        "before_copy_views"
        "IDC_DOCUMENT_TAB_CLOSE"
        "BM_CLICK"
        "smoke_dirty_prompt_choice")
    if(NOT SMOKE_SOURCE MATCHES "${required}")
        message(FATAL_ERROR "G11 native smoke is missing: ${required}")
    endif()
endforeach()

if(NOT HOST_SOURCE MATCHES "source_before"
        OR NOT HOST_SOURCE MATCHES "target_before"
        OR NOT HOST_SOURCE MATCHES "routing.targets.MoveDocumentView")
    message(FATAL_ERROR "G11 transfer rollback is not structurally enforced")
endif()

message(STATUS "Verified G11 value-only tab drag, transactional transfer, and native routes")
