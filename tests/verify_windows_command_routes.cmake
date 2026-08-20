if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(main_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp")
set(resource_source "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui_ja.generated.rc")
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

# These pane-owned actions are dispatched by buttons inside the dynamic pane
# tabs. They intentionally have no top-level menu resource after the direct
# pane-toggle migration, but still require one production command owner.
set(pane_only_route_ids
    IDM_BATCH_PIN
    IDM_COLOR_PIN
    IDM_LIGHT_TABLE_PIN
    IDM_LOCATOR_AUTOSCROLL
    IDM_LOCATOR_FIXED
    IDM_LOCATOR_PIN
    IDM_SEQUENCE_PIN
    IDM_SUBPALETTE_PIN)
foreach(pane_only_route_id IN LISTS pane_only_route_ids)
    if(NOT pane_only_route_id IN_LIST unique_route_ids)
        message(FATAL_ERROR
            "pane-only command has no route owner: ${pane_only_route_id}")
    endif()
endforeach()
list(REMOVE_ITEM unique_route_ids ${pane_only_route_ids})

string(REGEX MATCHALL "IDM_[A-Z0-9_]+" resource_ids "${resource_text}")
list(REMOVE_DUPLICATES resource_ids)
list(SORT resource_ids)
list(SORT unique_route_ids)
if(NOT resource_ids STREQUAL unique_route_ids)
    set(unrouted_resource_ids ${resource_ids})
    list(REMOVE_ITEM unrouted_resource_ids ${unique_route_ids})
    set(resource_less_route_ids ${unique_route_ids})
    list(REMOVE_ITEM resource_less_route_ids ${resource_ids})
    message(FATAL_ERROR
        "command routes differ from the production localized resource command set: "
        "unrouted resources=${unrouted_resource_ids}; "
        "resource-less routes=${resource_less_route_ids}")
endif()

list(LENGTH resource_ids production_count)
message(STATUS
    "Verified ${production_count} production command IDs with one route owner each")
