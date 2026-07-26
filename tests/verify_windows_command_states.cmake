if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(catalog_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/command_state_catalog.inc")
set(resource_source "${INKPOD_SOURCE_DIR}/apps/windows/app/app.rc")
file(READ "${catalog_source}" catalog_text)
file(READ "${resource_source}" resource_text)

string(REGEX MATCHALL
    "INKPOD_COMMAND_STATE\\([A-Za-z]+, (IDM_[A-Z0-9_]+)\\)"
    catalog_entries "${catalog_text}")
set(catalog_ids)
foreach(entry IN LISTS catalog_entries)
    string(REGEX REPLACE
        ".* (IDM_[A-Z0-9_]+)\\)" "\\1" command_id "${entry}")
    list(APPEND catalog_ids "${command_id}")
endforeach()
list(LENGTH catalog_ids catalog_count)
set(unique_catalog_ids ${catalog_ids})
list(REMOVE_DUPLICATES unique_catalog_ids)
list(LENGTH unique_catalog_ids unique_catalog_count)
if(NOT catalog_count EQUAL unique_catalog_count)
    message(FATAL_ERROR
        "R5 command-state catalog has duplicate owners: ${catalog_count} "
        "entries, ${unique_catalog_count} unique IDs")
endif()

string(REGEX MATCHALL "IDM_[A-Z0-9_]+" resource_ids "${resource_text}")
list(REMOVE_DUPLICATES resource_ids)
list(SORT resource_ids)
list(SORT unique_catalog_ids)
if(NOT resource_ids STREQUAL unique_catalog_ids)
    message(FATAL_ERROR
        "R5 command-state catalog differs from the production app.rc command set")
endif()

list(LENGTH resource_ids production_count)
message(STATUS
    "Verified ${production_count} production command IDs with one R5 state owner each")
