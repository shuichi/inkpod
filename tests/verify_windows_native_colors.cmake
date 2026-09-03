if(NOT DEFINED INKPOD_SOURCE_DIR)
    message(FATAL_ERROR "INKPOD_SOURCE_DIR is required")
endif()
file(GLOB_RECURSE UI_SOURCES
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/*.cpp"
    "${INKPOD_SOURCE_DIR}/apps/windows/ui/*.h"
    "${INKPOD_SOURCE_DIR}/apps/windows/app/*.cpp"
    "${INKPOD_SOURCE_DIR}/apps/windows/app/*.h")
foreach(SOURCE IN LISTS UI_SOURCES)
    file(READ "${SOURCE}" TEXT)
    if(TEXT MATCHES "UISettings|ColorValuesChanged|AppsUseLightTheme|UiColorMode|UiBrush\\(|UiColor\\(|SetWindowTheme\\(|SetSysColors\\(|SetPreferredAppMode|AllowDarkModeFor")
        message(FATAL_ERROR "Client UI must retain native colors without an app-theme override: ${SOURCE}")
    endif()
endforeach()
file(READ "${INKPOD_SOURCE_DIR}/apps/windows/ui/main_window_runtime.cpp" WINDOW)
if(NOT WINDOW MATCHES "DWMWA_USE_IMMERSIVE_DARK_MODE")
    message(FATAL_ERROR "The documented native dark title bar must remain available")
endif()
message(STATUS "Verified native client colors and title-bar-only dark opt-in")
