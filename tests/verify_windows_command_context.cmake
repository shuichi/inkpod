if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(identity_header
    "${INKPOD_SOURCE_DIR}/apps/windows/app/identity.h")
set(context_header
    "${INKPOD_SOURCE_DIR}/apps/windows/app/command_context.h")
set(context_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/command_context.cpp")
set(engine_header
    "${INKPOD_SOURCE_DIR}/apps/windows/app/core_engine.h")
set(runtime_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(command_router_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_command_router.cpp")
set(effect_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/effects_controller.cpp")
set(batch_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/batch_controller.cpp")
set(cmake_source "${INKPOD_SOURCE_DIR}/CMakeLists.txt")

foreach(required_source IN ITEMS
        "${identity_header}"
        "${context_header}"
        "${context_source}"
        "${engine_header}"
        "${runtime_source}"
        "${command_router_source}"
        "${effect_source}"
        "${batch_source}")
    if(NOT EXISTS "${required_source}")
        message(FATAL_ERROR "G1 source is missing: ${required_source}")
    endif()
endforeach()

file(READ "${identity_header}" identity_text)
foreach(required_id IN ITEMS
        "WorkspaceWindowId"
        "DocumentSessionId"
        "DocumentViewId"
        "EditorGroupId"
        "CanvasId"
        "PaneInstanceId"
        "JobSessionId"
        "Generation")
    string(FIND "${identity_text}" "${required_id}" id_offset)
    if(id_offset LESS 0)
        message(FATAL_ERROR "G1 strong ID is missing: ${required_id}")
    endif()
endforeach()
foreach(required_identity_contract IN ITEMS
        "explicit constexpr StrongFrontendId"
        "operator<=>"
        "struct hash<inkpod::app::StrongFrontendId<Tag>>")
    string(FIND
        "${identity_text}" "${required_identity_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "G1 identity contract is missing: ${required_identity_contract}")
    endif()
endforeach()

file(READ "${context_header}" context_text)
foreach(required_context_contract IN ITEMS
        "struct CommandContext"
        "std::optional<WorkspaceWindowId>"
        "std::optional<EditorGroupId>"
        "std::optional<DocumentSessionId>"
        "std::optional<DocumentViewId>"
        "std::optional<PaneInstanceId>"
        "std::optional<JobSessionId>"
        "std::optional<Generation>"
        "class CommandTargetRegistry"
        "CommandTimerToken"
        "DragToken"
        "PostedNotificationToken")
    string(FIND
        "${context_text}" "${required_context_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "G1 CommandContext contract is missing: ${required_context_contract}")
    endif()
endforeach()

file(READ "${runtime_source}" runtime_text)
file(READ "${command_router_source}" command_router_text)
string(APPEND runtime_text "\n${command_router_text}")
foreach(required_runtime_contract IN ITEMS
        "IssueCommand("
        "TargetScopeForOwner"
        "routing.targets.Resolve"
        "routing.targets.Capture"
        "completion_context"
        "ResolveCommandTimer"
        "locator_results"
        "IssueDrag")
    string(FIND
        "${runtime_text}" "${required_runtime_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "G1 runtime routing contract is missing: ${required_runtime_contract}")
    endif()
endforeach()

string(REGEX MATCHALL
    "Route[A-Za-z]+Command\\([^\\{]*const (app::)?CommandContext&"
    context_route_matches
    "${runtime_text}")
list(LENGTH context_route_matches context_route_count)
if(context_route_count LESS 11)
    message(FATAL_ERROR
        "G1 command routes do not all receive CommandContext: "
        "${context_route_count} matches")
endif()

foreach(forbidden_runtime_token IN ITEMS
        "reinterpret_cast<LPARAM>(delivery)"
        "kAutosaveTimer ="
        "kContinuousSprayTimer ="
        "kMotionPlaybackTimer ="
        "kShortcutSequenceTimer ="
        "kStatusProgressTimer ="
        "SendMessageW(window, WM_COMMAND")
    string(FIND "${runtime_text}" "${forbidden_runtime_token}" token_offset)
    if(NOT token_offset LESS 0)
        message(FATAL_ERROR
            "G1 runtime retains implicit/raw routing: ${forbidden_runtime_token}")
    endif()
endforeach()

file(READ "${engine_header}" engine_text)
foreach(required_engine_contract IN ITEMS
        "CommandContext context"
        "const CommandContext& context"
        "SetCommandGeneration")
    string(FIND
        "${engine_text}" "${required_engine_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "Core queue is missing G1 context copy: ${required_engine_contract}")
    endif()
endforeach()

foreach(notification_source IN ITEMS
        "${runtime_source}"
        "${effect_source}"
        "${batch_source}")
    file(READ "${notification_source}" notification_text)
    string(REGEX MATCH
        "PostMessageW\\([^;]*reinterpret_cast<LPARAM>"
        raw_posted_pointer
        "${notification_text}")
    if(raw_posted_pointer)
        message(FATAL_ERROR
            "posted notification contains a raw pointer: ${notification_source}")
    endif()
endforeach()

file(READ "${cmake_source}" cmake_text)
foreach(required_test IN ITEMS
        "inkpod_windows_command_context_tests"
        "tests/windows_command_context.cpp"
        "verify_windows_command_context.cmake")
    string(FIND "${cmake_text}" "${required_test}" test_offset)
    if(test_offset LESS 0)
        message(FATAL_ERROR "G1 test gate is missing: ${required_test}")
    endif()
endforeach()

string(FIND
    "${cmake_text}" "CMAKE_CL_SHOWINCLUDES_PREFIX" showincludes_offset)
if(showincludes_offset LESS 0)
    message(FATAL_ERROR
        "Windows MSVC /showIncludes dependency-prefix repair is missing")
endif()

message(STATUS
    "Verified G1 strong IDs, issue-time CommandContext, generation tokens, "
    "and raw-pointer-free posted command notifications")
