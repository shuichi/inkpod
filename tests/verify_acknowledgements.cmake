if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(core_manifest "${INKPOD_SOURCE_DIR}/rust/inkpod-core/Cargo.toml")
set(format_manifest "${INKPOD_SOURCE_DIR}/rust/inkpod-format/Cargo.toml")
set(cargo_lock "${INKPOD_SOURCE_DIR}/Cargo.lock")
set(acknowledgements "${INKPOD_SOURCE_DIR}/html/acknowledgements.html")

file(READ "${core_manifest}" core_manifest_text)
file(READ "${format_manifest}" format_manifest_text)
file(READ "${cargo_lock}" cargo_lock_text)
file(READ "${acknowledgements}" acknowledgements_text)

string(REGEX MATCH
    "blake3[ \t]*=[^\r\n]*version[ \t]*=[ \t]*\"=([0-9]+\\.[0-9]+\\.[0-9]+)\""
    blake3_match
    "${core_manifest_text}")
if(NOT blake3_match)
    message(FATAL_ERROR "inkpod-core blake3 dependency version was not found")
endif()
set(blake3_version "${CMAKE_MATCH_1}")

string(REGEX MATCH
    "png[ \t]*=[ \t]*\"([0-9]+\\.[0-9]+\\.[0-9]+)\""
    png_match
    "${format_manifest_text}")
if(NOT png_match)
    message(FATAL_ERROR "inkpod-format png dependency version was not found")
endif()
set(png_version "${CMAKE_MATCH_1}")

foreach(dependency IN ITEMS blake3 png)
    set(version "${${dependency}_version}")
    set(marker "data-crate=\"${dependency}\" data-version=\"${version}\"")
    string(FIND "${acknowledgements_text}" "${marker}" marker_offset)
    if(marker_offset LESS 0)
        message(FATAL_ERROR
            "acknowledgements.html does not identify ${dependency} ${version}")
    endif()
endforeach()

foreach(dependency IN ITEMS
        arrayref
        arrayvec
        cfg-if
        constant_time_eq
        cpufeatures
        cc
        find-msvc-tools
        shlex
        fdeflate
        flate2
        crc32fast
        miniz_oxide
        adler2
        simd-adler32
        bitflags)
    string(REGEX MATCH
        "name = \"${dependency}\"[\r\n]+version = \"([^\"]+)\""
        dependency_match
        "${cargo_lock_text}")
    if(NOT dependency_match)
        message(FATAL_ERROR "Cargo.lock has no package named ${dependency}")
    endif()
    set(version "${CMAKE_MATCH_1}")
    set(marker "data-crate=\"${dependency}\" data-version=\"${version}\"")
    string(FIND "${acknowledgements_text}" "${marker}" marker_offset)
    if(marker_offset LESS 0)
        message(FATAL_ERROR
            "acknowledgements.html does not identify ${dependency} ${version}")
    endif()
endforeach()

foreach(required_text IN ITEMS
        "ThirdPartyNotices.txt"
        "GPL-3.0-only"
        "Apache-2.0"
        "MIT OR Apache-2.0"
        "data-dependency=\"fluent-ui-system-icons\" data-version=\"1.1.337\""
        "84e8a2ae0e55b3cbe176b5cc33154fe82ef363cc")
    string(FIND "${acknowledgements_text}" "${required_text}" text_offset)
    if(text_offset LESS 0)
        message(FATAL_ERROR
            "acknowledgements.html is missing required text: ${required_text}")
    endif()
endforeach()

message(STATUS
    "Verified acknowledgements for blake3 ${blake3_version}, png ${png_version}, and Fluent UI System Icons 1.1.337")
