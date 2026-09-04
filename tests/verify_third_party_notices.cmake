if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()

set(notices_path "${INKPOD_SOURCE_DIR}/docs/third-party-notices.txt")
if(NOT EXISTS "${notices_path}")
    message(FATAL_ERROR "Third-party notices source is missing: ${notices_path}")
endif()

file(READ "${notices_path}" notices_text)
file(READ "${notices_path}" notices_hex HEX)
file(READ "${INKPOD_SOURCE_DIR}/Cargo.lock" cargo_lock_text)

string(REGEX MATCHALL "[0-9a-f][0-9a-f]" notice_bytes "${notices_hex}")
foreach(notice_byte IN LISTS notice_bytes)
    if(notice_byte MATCHES "^[89a-f]")
        message(FATAL_ERROR
            "Third-party notices must contain English ASCII text only")
    endif()
endforeach()

foreach(dependency IN ITEMS
        png
        fdeflate
        flate2
        crc32fast
        miniz_oxide
        adler2
        simd-adler32
        cfg-if
        bitflags
        blake3
        arrayref
        arrayvec
        constant_time_eq
        cpufeatures
        cc
        find-msvc-tools
        shlex)
    string(REGEX MATCH
        "name = \"${dependency}\"[\r\n]+version = \"([^\"]+)\""
        dependency_match
        "${cargo_lock_text}")
    if(NOT dependency_match)
        message(FATAL_ERROR "Cargo.lock has no package named ${dependency}")
    endif()
    set(version "${CMAKE_MATCH_1}")
    set(marker "${dependency} ${version}")
    string(FIND "${notices_text}" "${marker}" marker_offset)
    if(marker_offset LESS 0)
        message(FATAL_ERROR
            "Third-party notices do not identify ${dependency} ${version}")
    endif()
endforeach()

foreach(forbidden_text IN ITEMS
        "```"
        "| crate |"
        "|---"
        "# Third"
        "# サード")
    string(FIND "${notices_text}" "${forbidden_text}" forbidden_offset)
    if(NOT forbidden_offset LESS 0)
        message(FATAL_ERROR
            "Third-party notices contain Markdown or non-English source text: ${forbidden_text}")
    endif()
endforeach()

foreach(required_text IN ITEMS
        "THIRD-PARTY NOTICES"
        "Inkpod includes the following third-party software and assets."
        "Fluent UI System Icons 1.1.337"
        "blake3 1.8.5"
        "png 0.17.16"
        "Microsoft Visual C/C++ Runtime"
        "Apache License"
        "MIT License"
        "BSD 2-Clause License"
        "MIT No Attribution License")
    string(FIND "${notices_text}" "${required_text}" required_offset)
    if(required_offset LESS 0)
        message(FATAL_ERROR
            "Third-party notices are missing required text: ${required_text}")
    endif()
endforeach()

message(STATUS "Verified plain-text, end-user third-party notices")
