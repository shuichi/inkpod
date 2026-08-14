#pragma once

#include <windows.h>

namespace inkpod::windows::ui {

// Product-owned resources are always selected by the language resolved during
// startup.  These functions do not use the thread-language fallback chain.
[[nodiscard]] int LoadLocalizedStringW(
    HINSTANCE instance,
    UINT identifier,
    wchar_t* buffer,
    int buffer_count) noexcept;

[[nodiscard]] HMENU LoadLocalizedMenuW(
    HINSTANCE instance, LPCWSTR resource_name) noexcept;

[[nodiscard]] HWND CreateLocalizedDialogParamW(
    HINSTANCE instance,
    LPCWSTR template_name,
    HWND owner,
    DLGPROC procedure,
    LPARAM parameter) noexcept;

[[nodiscard]] INT_PTR DialogBoxLocalizedParamW(
    HINSTANCE instance,
    LPCWSTR template_name,
    HWND owner,
    DLGPROC procedure,
    LPARAM parameter) noexcept;

}  // namespace inkpod::windows::ui
