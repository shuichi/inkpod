if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(APP_DIR "${INKPOD_SOURCE_DIR}/apps/windows/app")
set(CORE_HEADER "${APP_DIR}/core_host.h")
set(CORE_SOURCE "${APP_DIR}/core_host.cpp")
set(DOCUMENT_HEADER "${APP_DIR}/document_session.h")
set(DOCUMENT_SOURCE "${APP_DIR}/document_session.cpp")
set(APPLICATION_SOURCE "${APP_DIR}/application.cpp")
set(HOST_SOURCE "${APP_DIR}/application_host.cpp")
set(RUNTIME_SOURCE
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(TEST_SOURCE "${INKPOD_SOURCE_DIR}/tests/windows_core_host.cpp")
set(CMAKE_SOURCE "${INKPOD_SOURCE_DIR}/CMakeLists.txt")

foreach(FILE IN ITEMS
        "${CORE_HEADER}"
        "${CORE_SOURCE}"
        "${DOCUMENT_HEADER}"
        "${DOCUMENT_SOURCE}"
        "${APPLICATION_SOURCE}"
        "${HOST_SOURCE}"
        "${RUNTIME_SOURCE}"
        "${TEST_SOURCE}")
    if(NOT EXISTS "${FILE}")
        message(FATAL_ERROR "Missing G3 source: ${FILE}")
    endif()
endforeach()
if(EXISTS "${APP_DIR}/core_engine.h" OR EXISTS "${APP_DIR}/core_engine.cpp")
    message(FATAL_ERROR "Retired single-Core CoreEngine still exists")
endif()

file(READ "${CORE_HEADER}" HEADER)
foreach(REQUIRED IN ITEMS
        "class CoreHost final"
        "DocumentSessionId session"
        "Generation generation"
        "CreateSession("
        "RebindSession("
        "CloseSession("
        "SetActiveSession("
        "RetargetNotificationOwner("
        "UnregisterSnapshotSinks("
        "SnapshotSinkCount()"
        "InvokeAll("
        "InvokePrimitive("
        "EnqueuePrimitive("
        "RegisterColorArray("
        "ReleaseObject("
        "SetSessionInitializer("
        "CoreSessionState"
        "CoreNotification"
        "TakeNotification(")
    string(FIND "${HEADER}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "CoreHost contract is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${CORE_SOURCE}" SOURCE)
foreach(REQUIRED IN ITEMS
        "struct SessionBinding"
        "using AdapterInputToken = std::uint64_t"
        "struct AdapterWork"
        "struct AdapterInput"
        "struct PrimitiveWork"
        "struct StrokeWork"
        "struct ControlWork"
        "struct CoreEntry"
        "std::vector<std::unique_ptr<CoreEntry>> entries"
        "inkpod_core_create"
        "inkpod_core_destroy"
        "FindEntry(item.binding)"
        "InkpodPrimitiveRequestV3 request"
        "ProcessPrimitive("
        "ProcessAdapter("
        "TakeAdapterInput("
        "std::vector<AdapterInput> adapter_inputs"
        "inkpod_core_primitive_execute_v3"
        "state.pending_operations"
        "ReadCoreErrorOnCurrentThread()"
        "CoreNotificationKind::StateChanged"
        "CoreNotificationKind::AsyncFailed"
        "notification_owner_mutex")
    string(FIND "${SOURCE}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "CoreHost implementation is missing: ${REQUIRED}")
    endif()
endforeach()
string(FIND "${SOURCE}" "LegacyInvokeWork" LEGACY_INVOKE)
if(NOT LEGACY_INVOKE LESS 0)
    message(FATAL_ERROR "Retired LegacyInvokeWork remains in CoreHost")
endif()
string(FIND "${SOURCE}" "struct AdapterWork" ADAPTER_START)
string(FIND "${SOURCE}" "struct AdapterInput" ADAPTER_END)
if(ADAPTER_START LESS 0 OR ADAPTER_END LESS ADAPTER_START)
    message(FATAL_ERROR "Cannot isolate the AdapterWork token record")
endif()
math(EXPR ADAPTER_LENGTH "${ADAPTER_END} - ${ADAPTER_START}")
string(SUBSTRING
    "${SOURCE}" ${ADAPTER_START} ${ADAPTER_LENGTH} ADAPTER_RECORD)
foreach(FORBIDDEN IN ITEMS
        "std::function" "std::string" "std::vector" "std::shared_ptr"
        "const char*" "const wchar_t*" "InkpodCore*" "path")
    string(FIND "${ADAPTER_RECORD}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR
            "AdapterWork contains forbidden queued state: ${FORBIDDEN}")
    endif()
endforeach()
string(FIND "${SOURCE}" "struct PrimitiveWork" PRIMITIVE_START)
string(FIND "${SOURCE}" "enum class ControlKind" PRIMITIVE_END)
if(PRIMITIVE_START LESS 0 OR PRIMITIVE_END LESS PRIMITIVE_START)
    message(FATAL_ERROR "Cannot isolate the typed PrimitiveWork record")
endif()
math(EXPR PRIMITIVE_LENGTH "${PRIMITIVE_END} - ${PRIMITIVE_START}")
string(SUBSTRING
    "${SOURCE}" ${PRIMITIVE_START} ${PRIMITIVE_LENGTH} PRIMITIVE_RECORD)
foreach(FORBIDDEN IN ITEMS "std::function" "std::string" "const char*" "const wchar_t*" "path")
    string(FIND "${PRIMITIVE_RECORD}" "${FORBIDDEN}" OFFSET)
    if(NOT OFFSET LESS 0)
        message(FATAL_ERROR
            "Typed PrimitiveWork contains forbidden queued state: ${FORBIDDEN}")
    endif()
endforeach()
string(REGEX MATCH
    "PostMessageW\\([^;]*reinterpret_cast<(W|L)PARAM>"
    RAW_POSTED_POINTER
    "${SOURCE}")
if(RAW_POSTED_POINTER)
    message(FATAL_ERROR "CoreHost posts a raw pointer through a window message")
endif()

file(READ "${DOCUMENT_HEADER}" DOCUMENT)
foreach(REQUIRED IN ITEMS
        "kMaximumSessions"
        "std::array<std::unique_ptr<DocumentSession>"
        "bool Add("
        "bool Remove("
        "bool Activate("
        "DocumentSession* Find("
        "ClearCoreBindings")
    string(FIND "${DOCUMENT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "DocumentRegistry G3 contract is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${HOST_SOURCE}" HOST)
foreach(REQUIRED IN ITEMS
        "engine->CreateSession"
        "engine->RebindSession"
        "engine->SetActiveSession")
    string(FIND "${HOST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "ApplicationHost session binding is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${RUNTIME_SOURCE}" RUNTIME)
foreach(REQUIRED IN ITEMS
        "TakeNotification("
        "RetargetCoreNotificationsBeforeWorkspaceClose("
        "UnregisterSnapshotSinks("
        "CoreNotificationKind::StateChanged"
        "CoreNotificationKind::AsyncFailed"
        "kDocumentSessionCommandScope")
    string(FIND "${RUNTIME}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "Session-keyed UI notification routing is missing: ${REQUIRED}")
    endif()
endforeach()

file(GLOB_RECURSE FRONTEND_SOURCES
    "${APP_DIR}/*.h" "${APP_DIR}/*.cpp"
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/*.h"
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/*.cpp")
foreach(FILE IN LISTS FRONTEND_SOURCES)
    file(READ "${FILE}" CONTENT)
    if(CONTENT MATCHES "CoreEngine")
        message(FATAL_ERROR "Retired CoreEngine reference remains in ${FILE}")
    endif()
endforeach()

file(READ "${TEST_SOURCE}" TEST)
foreach(REQUIRED IN ITEMS
        "host.CreateSession(first"
        "host.CreateSession(second"
        "host.RetargetNotificationOwner(owner, replacement_owner)"
        "host.UnregisterSnapshotSinks("
        "host.SnapshotSinkCount()"
        "first_info.document_id != second_info.document_id"
        "inkpod_core_undo"
        "inkpod_core_redo"
        "inkpod_core_save"
        "inkpod_core_open"
        "queued_colors[0].red = 240U"
        "PrimitiveQueueSaturationIsExactlyOnce("
        "PrimitiveShutdownCompletesExactlyOnce("
        "host.EnqueuePrimitive("
        "host.ReleaseObject(first, generation, palette_id)"
        "stale_primitive_rejected"
        "host.CloseSession"
        "host.ThreadId()")
    string(FIND "${TEST}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G3 native test evidence is missing: ${REQUIRED}")
    endif()
endforeach()

file(READ "${CMAKE_SOURCE}" CMAKE_TEXT)
foreach(REQUIRED IN ITEMS
        "inkpod_windows_core_host_tests"
        "tests/windows_core_host.cpp"
        "verify_windows_core_host.cmake")
    string(FIND "${CMAKE_TEXT}" "${REQUIRED}" OFFSET)
    if(OFFSET LESS 0)
        message(FATAL_ERROR "G3 test registration is missing: ${REQUIRED}")
    endif()
endforeach()

message(STATUS
    "Verified session-keyed CoreHost ownership, typed primitive queue, "
    "routing, notification, shutdown, and native test boundaries")
