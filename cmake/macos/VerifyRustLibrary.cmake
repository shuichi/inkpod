if(NOT DEFINED INKPOD_RUST_STATICLIB OR INKPOD_RUST_STATICLIB STREQUAL "")
    message(FATAL_ERROR "INKPOD_RUST_STATICLIB is required")
endif()

if(NOT EXISTS "${INKPOD_RUST_STATICLIB}")
    message(FATAL_ERROR
        "Inkpod Rust static library is missing: ${INKPOD_RUST_STATICLIB}. "
        "Build it through the owning CMake target before invoking Xcode.")
endif()

file(SIZE "${INKPOD_RUST_STATICLIB}" INKPOD_RUST_STATICLIB_SIZE)
if(INKPOD_RUST_STATICLIB_SIZE EQUAL 0)
    message(FATAL_ERROR
        "Inkpod Rust static library is empty: ${INKPOD_RUST_STATICLIB}")
endif()
