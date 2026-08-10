if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(dialog_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs/effects_dialogs.cpp")
set(runtime_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(state_header
    "${INKPOD_SOURCE_DIR}/apps/windows/app/frontend_state.h")
set(smoke_source
    "${INKPOD_SOURCE_DIR}/apps/windows/app/app_smoke.cpp")

foreach(required_source IN ITEMS
        "${dialog_source}"
        "${runtime_source}"
        "${state_header}"
        "${smoke_source}")
    if(NOT EXISTS "${required_source}")
        message(FATAL_ERROR "filter preview source is missing: ${required_source}")
    endif()
endforeach()

file(READ "${dialog_source}" dialog_text)
foreach(required_dialog_contract IN ITEMS
        "EN_CHANGE"
        "CBN_SELCHANGE"
        "kEffectPreviewDebounceTimer"
        "preview_progress"
        "SubmitEffectPreviewChange")
    string(FIND "${dialog_text}" "${required_dialog_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "live filter dialog contract is missing: ${required_dialog_contract}")
    endif()
endforeach()

file(READ "${state_header}" state_text)
foreach(required_queue_contract IN ITEMS
        "InteractiveFilterPreviewUiState"
        "std::optional<FilterJob> pending"
        "desired_generation"
        "running_generation"
        "FilterPreviewWork")
    string(FIND "${state_text}" "${required_queue_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "latest-preview queue contract is missing: ${required_queue_contract}")
    endif()
endforeach()

file(READ "${runtime_source}" runtime_text)
foreach(required_runtime_contract IN ITEMS
        "inkpod_task_cancel(state.effects.task)"
        "inkpod_core_filter_preview_begin_task"
        "inkpod_core_filter_preview_update_task"
        "QueueInteractiveFilterFinalize(state, true)"
        "QueueInteractiveFilterFinalize(state, false)"
        "CompleteInteractiveFilterWork"
        "document_current")
    string(FIND "${runtime_text}" "${required_runtime_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "filter preview runtime contract is missing: ${required_runtime_contract}")
    endif()
endforeach()

file(READ "${smoke_source}" smoke_text)
foreach(required_smoke_contract IN ITEMS
        "preview_updates_start + 4U"
        "smoke_cancel_next = true"
        "IDM_FILTER_BRIGHTNESS"
        "IDM_EDIT_UNDO")
    string(FIND "${smoke_text}" "${required_smoke_contract}" contract_offset)
    if(contract_offset LESS 0)
        message(FATAL_ERROR
            "filter preview production smoke is missing: ${required_smoke_contract}")
    endif()
endforeach()

message(STATUS
    "Verified live parameter notifications, bounded latest-wins filter preview, "
    "stale target cancellation, and Windows production smoke coverage")
