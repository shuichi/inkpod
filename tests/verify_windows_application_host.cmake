if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(APP_DIR "${INKPOD_SOURCE_DIR}/apps/windows/app")
set(UI_DIR "${INKPOD_SOURCE_DIR}/apps/windows/ui")

set(REQUIRED_FILES
    "${APP_DIR}/application_host.h"
    "${APP_DIR}/application_owner_graph.h"
    "${APP_DIR}/document_session.h"
    "${APP_DIR}/workspace_window.h"
    "${UI_DIR}/main_window_command_router.cpp"
    "${UI_DIR}/main_window_document_presenter.cpp"
    "${UI_DIR}/main_window_input_router.cpp"
    "${UI_DIR}/main_window_procedure.cpp"
    "${UI_DIR}/main_window_status_presenter.cpp")
foreach(FILE IN LISTS REQUIRED_FILES)
    if(NOT EXISTS "${FILE}")
        message(FATAL_ERROR "Missing G2 owner boundary: ${FILE}")
    endif()
endforeach()

file(GLOB_RECURSE FRONTEND_SOURCES
    "${APP_DIR}/*.h" "${APP_DIR}/*.cpp"
    "${UI_DIR}/*.h" "${UI_DIR}/*.cpp")
foreach(FILE IN LISTS FRONTEND_SOURCES)
    file(READ "${FILE}" CONTENT)
    if(CONTENT MATCHES "AppContext")
        message(FATAL_ERROR "Retired AppContext remains in ${FILE}")
    endif()
endforeach()

file(READ "${APP_DIR}/application_host.h" HOST)
foreach(REQUIRED IN ITEMS
        "WorkspaceWindowRegistry"
        "DocumentRegistry"
        "std::unique_ptr<CoreEngine>"
        "InkpodClipboard\\* clipboard")
    if(NOT HOST MATCHES "${REQUIRED}")
        message(FATAL_ERROR "ApplicationHost is missing ownership: ${REQUIRED}")
    endif()
endforeach()

file(READ "${APP_DIR}/application_host.cpp" HOST_SOURCE)
if(NOT HOST_SOURCE MATCHES "InitializeOwnerGraph"
    OR NOT HOST_SOURCE MATCHES "ClearOwnerGraph")
    message(FATAL_ERROR
        "ApplicationHost must use the tested construction/unwind owner graph")
endif()

file(READ "${APP_DIR}/document_session.h" DOCUMENT)
foreach(REQUIRED IN ITEMS
        "class DocumentSession"
        "struct DocumentView"
        "ViewUiState presentation"
        "CoreEngine\\* core_"
        "DocumentSessionId id"
        "Generation generation")
    if(NOT DOCUMENT MATCHES "${REQUIRED}")
        message(FATAL_ERROR "Document owner boundary is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${UI_DIR}/main_window_procedure.cpp" PROCEDURE)
if(NOT PROCEDURE MATCHES "WorkspaceWindow\\*"
    OR PROCEDURE MATCHES "reinterpret_cast<app::ApplicationHost\\*>")
    message(FATAL_ERROR
        "Window procedure must bind WorkspaceWindow and use its explicit host link")
endif()

file(READ "${UI_DIR}/main_window_runtime.cpp" RUNTIME)
foreach(RETIRED IN ITEMS
        "LRESULT CALLBACK MainWindowProcedure"
        "std::optional<LRESULT> IssueCommand"
        "bool PreTranslateKeyboardMessage")
    if(RUNTIME MATCHES "${RETIRED}")
        message(FATAL_ERROR "Runtime still owns extracted G2 entry: ${RETIRED}")
    endif()
endforeach()

message(STATUS
    "Verified ApplicationHost/WorkspaceWindow/DocumentSession/DocumentView ownership and G2 runtime boundaries")
