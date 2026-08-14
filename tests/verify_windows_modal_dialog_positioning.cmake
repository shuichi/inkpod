if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(dialog_directory "${INKPOD_SOURCE_DIR}/apps/windows/ui/dialogs")
set(position_source "${dialog_directory}/modal_dialog_position.cpp")
set(position_header "${dialog_directory}/modal_dialog_position.h")
foreach(required_source IN ITEMS "${position_source}" "${position_header}")
    if(NOT EXISTS "${required_source}")
        message(FATAL_ERROR "modal dialog positioning source is missing: ${required_source}")
    endif()
endforeach()

file(READ "${position_source}" position_text)
foreach(required_position_token IN ITEMS
        "GetAncestor(owner, GA_ROOTOWNER)"
        "MonitorFromWindow"
        "MONITOR_DEFAULTTONEAREST"
        "GetMonitorInfoW"
        "GetWindowRect(owner"
        "SetWindowPos"
        "SWP_NOSIZE")
    string(FIND "${position_text}" "${required_position_token}" token_offset)
    if(token_offset LESS 0)
        message(FATAL_ERROR
            "modal dialog positioning is missing ${required_position_token}")
    endif()
endforeach()

file(GLOB_RECURSE windows_sources "${INKPOD_SOURCE_DIR}/apps/windows/*.cpp")
set(modal_dialog_count 0)
foreach(windows_source IN LISTS windows_sources)
    if(windows_source MATCHES "/ui_resources\\.cpp$")
        continue()
    endif()
    file(READ "${windows_source}" dialog_text)
    string(REGEX MATCHALL
        "DialogBoxLocalizedParamW[ \t\r\n]*\\(" dialog_calls "${dialog_text}")
    list(LENGTH dialog_calls dialog_call_count)
    if(dialog_call_count EQUAL 0)
        continue()
    endif()
    string(REGEX MATCHALL
        "CenterModalDialogOnOwner[ \t\r\n]*\\([ \t\r\n]*dialog[ \t\r\n]*\\)"
        center_calls "${dialog_text}")
    list(LENGTH center_calls center_call_count)
    if(NOT center_call_count EQUAL dialog_call_count)
        message(FATAL_ERROR
            "${windows_source} has ${dialog_call_count} modal dialog(s) but "
            "${center_call_count} owner-centering call(s)")
    endif()
    math(EXPR modal_dialog_count "${modal_dialog_count} + ${dialog_call_count}")
endforeach()

if(modal_dialog_count EQUAL 0)
    message(FATAL_ERROR "no resource-backed modal dialogs were found")
endif()
