#pragma once

#include <windows.h>

#include <cstdint>
#include <string>

namespace inkpod::app {

enum class EmbeddedManualStatus : std::uint8_t {
    Ok,
    InvalidArgument,
    ResourceUnavailable,
    CacheDirectoryUnavailable,
    WriteFailed,
    LaunchFailed,
};

// Extracts the embedded UTF-8 manual below the supplied LocalAppData root.
// Existing byte-identical content is reused; stale content is replaced atomically.
[[nodiscard]] EmbeddedManualStatus ExtractEmbeddedManual(
    HINSTANCE instance,
    const std::wstring& local_app_data_root,
    std::wstring& output_path) noexcept;

// Resolves the current user's LocalAppData directory and extracts the manual.
[[nodiscard]] EmbeddedManualStatus PrepareEmbeddedManual(
    HINSTANCE instance,
    std::wstring& output_path) noexcept;

// Prepares the manual and opens it with the user's default HTML handler.
[[nodiscard]] EmbeddedManualStatus OpenEmbeddedManual(
    HINSTANCE instance,
    HWND owner) noexcept;

}  // namespace inkpod::app
