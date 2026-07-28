include_guard(GLOBAL)

function(_inkpod_collect_msvc_crt_directories output_variable redist_root architecture)
    file(GLOB _inkpod_msvc_crt_candidates LIST_DIRECTORIES true
        "${redist_root}/${architecture}/Microsoft.VC*.CRT")
    set(_inkpod_msvc_crt_directories)
    foreach(_inkpod_msvc_crt_candidate IN LISTS _inkpod_msvc_crt_candidates)
        if(IS_DIRECTORY "${_inkpod_msvc_crt_candidate}")
            list(APPEND _inkpod_msvc_crt_directories
                "${_inkpod_msvc_crt_candidate}")
        endif()
    endforeach()
    set(${output_variable} "${_inkpod_msvc_crt_directories}" PARENT_SCOPE)
endfunction()

function(inkpod_find_msvc_crt_directory output_variable)
    cmake_parse_arguments(INKPOD_RUNTIME ""
        "VC_DIRECTORY;TOOLSET_VERSION;ARCHITECTURE" "" ${ARGN})
    foreach(_inkpod_required_argument IN ITEMS
            VC_DIRECTORY
            TOOLSET_VERSION
            ARCHITECTURE)
        if(NOT DEFINED INKPOD_RUNTIME_${_inkpod_required_argument}
                OR INKPOD_RUNTIME_${_inkpod_required_argument} STREQUAL "")
            message(FATAL_ERROR
                "inkpod_find_msvc_crt_directory requires ${_inkpod_required_argument}")
        endif()
    endforeach()
    if(NOT INKPOD_RUNTIME_ARCHITECTURE MATCHES "^(x64|arm64)$")
        message(FATAL_ERROR
            "Unsupported MSVC CRT architecture: ${INKPOD_RUNTIME_ARCHITECTURE}")
    endif()

    set(_inkpod_msvc_crt_directories)
    if(DEFINED ENV{VCToolsRedistDir} AND NOT "$ENV{VCToolsRedistDir}" STREQUAL "")
        file(TO_CMAKE_PATH "$ENV{VCToolsRedistDir}" _inkpod_redist_root)
        cmake_path(NORMAL_PATH _inkpod_redist_root)
        _inkpod_collect_msvc_crt_directories(
            _inkpod_msvc_crt_directories
            "${_inkpod_redist_root}"
            "${INKPOD_RUNTIME_ARCHITECTURE}")
        list(LENGTH _inkpod_msvc_crt_directories _inkpod_msvc_crt_count)
        if(NOT _inkpod_msvc_crt_count EQUAL 1)
            message(FATAL_ERROR
                "VCToolsRedistDir '${_inkpod_redist_root}' must contain exactly "
                "one ${INKPOD_RUNTIME_ARCHITECTURE}/Microsoft.VC*.CRT directory; "
                "found ${_inkpod_msvc_crt_count}")
        endif()
    else()
        set(_inkpod_exact_redist_root
            "${INKPOD_RUNTIME_VC_DIRECTORY}/Redist/MSVC/${INKPOD_RUNTIME_TOOLSET_VERSION}")
        _inkpod_collect_msvc_crt_directories(
            _inkpod_msvc_crt_directories
            "${_inkpod_exact_redist_root}"
            "${INKPOD_RUNTIME_ARCHITECTURE}")
        list(LENGTH _inkpod_msvc_crt_directories _inkpod_msvc_crt_count)

        if(_inkpod_msvc_crt_count EQUAL 0)
            file(GLOB _inkpod_redist_roots LIST_DIRECTORIES true
                "${INKPOD_RUNTIME_VC_DIRECTORY}/Redist/MSVC/*")
            foreach(_inkpod_redist_root IN LISTS _inkpod_redist_roots)
                if(IS_DIRECTORY "${_inkpod_redist_root}")
                    _inkpod_collect_msvc_crt_directories(
                        _inkpod_root_crt_directories
                        "${_inkpod_redist_root}"
                        "${INKPOD_RUNTIME_ARCHITECTURE}")
                    list(APPEND _inkpod_msvc_crt_directories
                        ${_inkpod_root_crt_directories})
                endif()
            endforeach()
            list(REMOVE_DUPLICATES _inkpod_msvc_crt_directories)
            list(LENGTH _inkpod_msvc_crt_directories _inkpod_msvc_crt_count)
        endif()

        if(NOT _inkpod_msvc_crt_count EQUAL 1)
            message(FATAL_ERROR
                "Could not choose one ${INKPOD_RUNTIME_ARCHITECTURE} MSVC CRT "
                "redistributable for toolset ${INKPOD_RUNTIME_TOOLSET_VERSION}; "
                "found ${_inkpod_msvc_crt_count}. Initialize the matching MSVC "
                "developer environment so VCToolsRedistDir is set.")
        endif()
    endif()

    list(GET _inkpod_msvc_crt_directories 0 _inkpod_msvc_crt_directory)
    foreach(_inkpod_required_runtime IN ITEMS
            msvcp140.dll
            vcruntime140.dll
            vcruntime140_1.dll)
        if(NOT EXISTS
                "${_inkpod_msvc_crt_directory}/${_inkpod_required_runtime}")
            message(FATAL_ERROR
                "MSVC CRT redist is missing ${_inkpod_required_runtime}: "
                "${_inkpod_msvc_crt_directory}")
        endif()
    endforeach()

    set(${output_variable} "${_inkpod_msvc_crt_directory}" PARENT_SCOPE)
endfunction()
