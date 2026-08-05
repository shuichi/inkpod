if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(main_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(resource_source "${INKPOD_SOURCE_DIR}/apps/windows/app/app.rc")
file(READ "${main_source}" main_text)
file(READ "${resource_source}" resource_text)

set(route_begin "std::optional<LRESULT> RouteBatchCommand")
set(route_end "std::optional<LRESULT> RouteWindowLifecycleMessage")
string(FIND "${main_text}" "${route_begin}" route_begin_offset)
string(FIND "${main_text}" "${route_end}" route_end_offset)
if(route_begin_offset LESS 0 OR route_end_offset LESS_EQUAL route_begin_offset)
    message(FATAL_ERROR "command route boundaries were not found")
endif()
math(EXPR route_length "${route_end_offset} - ${route_begin_offset}")
string(SUBSTRING "${main_text}" ${route_begin_offset} ${route_length} route_text)

# Direct owner cases use eight spaces. Nested mapping switches are deliberately
# excluded so each production command is counted at its single routing owner.
string(REGEX MATCHALL "\n        case IDM_[A-Z0-9_]+:" route_matches "${route_text}")
set(route_ids)
foreach(route_match IN LISTS route_matches)
    string(REGEX REPLACE ".*case (IDM_[A-Z0-9_]+):" "\\1" route_id "${route_match}")
    list(APPEND route_ids "${route_id}")
endforeach()
list(LENGTH route_ids route_count)
set(unique_route_ids ${route_ids})
list(REMOVE_DUPLICATES unique_route_ids)
list(LENGTH unique_route_ids unique_route_count)
if(NOT route_count EQUAL unique_route_count)
    message(FATAL_ERROR
        "command routes contain duplicate owners: ${route_count} cases, "
        "${unique_route_count} unique IDs")
endif()

string(REGEX MATCHALL "IDM_[A-Z0-9_]+" resource_ids "${resource_text}")
list(REMOVE_DUPLICATES resource_ids)
list(SORT resource_ids)
list(SORT unique_route_ids)
if(NOT resource_ids STREQUAL unique_route_ids)
    message(FATAL_ERROR
        "command routes differ from the production app.rc command set")
endif()

set(vector_select_begin "        case IDM_VECTOR_SELECT_CUT:")
set(vector_select_end "        case IDM_VECTOR_RASTERIZE:")
string(FIND "${route_text}" "${vector_select_begin}" vector_select_begin_offset)
string(FIND "${route_text}" "${vector_select_end}" vector_select_end_offset)
if(vector_select_begin_offset LESS 0
        OR vector_select_end_offset LESS_EQUAL vector_select_begin_offset)
    message(FATAL_ERROR "vector selection route boundaries were not found")
endif()
math(EXPR vector_select_length
    "${vector_select_end_offset} - ${vector_select_begin_offset}")
string(SUBSTRING
    "${route_text}"
    ${vector_select_begin_offset}
    ${vector_select_length}
    vector_select_text)
foreach(required_vector_select_token IN ITEMS
        "const CommandContext captured_context ="
        "state->routing.targets.Capture()"
        "option_status != INKPOD_STATUS_OK"
        "captured_context.document_session.has_value()"
        "captured_context.generation.has_value()"
        "state->RefreshEditorPresentation("
        "captured_context.document_session.value()"
        "captured_context.generation.value()")
    string(FIND
        "${vector_select_text}"
        "${required_vector_select_token}"
        vector_select_token_offset)
    if(vector_select_token_offset LESS 0)
        message(FATAL_ERROR
            "vector selection failure does not restore the captured Core "
            "presentation: ${required_vector_select_token}")
    endif()
endforeach()

list(LENGTH resource_ids production_count)
message(STATUS
    "Verified ${production_count} production command IDs with one route owner each")
