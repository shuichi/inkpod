if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(catalog_source
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/command_state_catalog.inc")
set(resource_source "${INKPOD_SOURCE_DIR}/apps/windows/app/app_ui_ja.generated.rc")
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
        "command-state catalog has duplicate owners: ${catalog_count} "
        "entries, ${unique_catalog_count} unique IDs")
endif()

string(REGEX MATCHALL "IDM_[A-Z0-9_]+" resource_ids "${resource_text}")
list(REMOVE_DUPLICATES resource_ids)
list(SORT resource_ids)

# These commands moved from the menu into pane-local accessible controls. They
# still require one command-state owner, but must not remain in the localized
# menu resource.
set(pane_control_ids
    IDM_BATCH_PIN
    IDM_COLOR_PIN
    IDM_LOCATOR_PIN
    IDM_LOCATOR_FIXED
    IDM_LOCATOR_AUTOSCROLL
    IDM_SEQUENCE_PIN
    IDM_LIGHT_TABLE_PIN
    IDM_SUBPALETTE_PIN)
foreach(command_id IN LISTS pane_control_ids)
    list(FIND unique_catalog_ids "${command_id}" catalog_index)
    if(catalog_index EQUAL -1)
        message(FATAL_ERROR
            "pane-local command ${command_id} has no command-state owner")
    endif()
    list(FIND resource_ids "${command_id}" resource_index)
    if(NOT resource_index EQUAL -1)
        message(FATAL_ERROR
            "pane-local command ${command_id} remains exposed in the menu resource")
    endif()
    list(REMOVE_ITEM unique_catalog_ids "${command_id}")
endforeach()

list(SORT unique_catalog_ids)
if(NOT resource_ids STREQUAL unique_catalog_ids)
    message(FATAL_ERROR
        "command-state catalog differs from the production localized resource command set")
endif()

list(LENGTH catalog_ids production_count)
message(STATUS
    "Verified ${production_count} production command IDs with one state owner each, including pane-local controls")
