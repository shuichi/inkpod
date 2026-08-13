#pragma once

#include <windows.h>

#include <cstdint>
#include <string>

namespace inkpod::app {

enum class EmbeddedHelpDocument : std::uint8_t {
    Manual,
    FileFormat,
    Acknowledgements,
};

enum class EmbeddedHelpStatus : std::uint8_t {
    Ok,
    InvalidArgument,
    ResourceUnavailable,
    CacheDirectoryUnavailable,
    WriteFailed,
    LaunchFailed,
};

// Extracts an embedded, self-contained UTF-8 help document below the supplied
// LocalAppData root.
// Existing byte-identical content is reused; stale content is replaced atomically.
[[nodiscard]] EmbeddedHelpStatus ExtractEmbeddedHelpDocument(
    HINSTANCE instance,
    const std::wstring& local_app_data_root,
    EmbeddedHelpDocument document,
    std::wstring& output_path) noexcept;

// Resolves the current user's LocalAppData directory and extracts the document.
[[nodiscard]] EmbeddedHelpStatus PrepareEmbeddedHelpDocument(
    HINSTANCE instance,
    EmbeddedHelpDocument document,
    std::wstring& output_path) noexcept;

// Prepares the document and opens it with the user's default HTML handler.
[[nodiscard]] EmbeddedHelpStatus OpenEmbeddedHelpDocument(
    HINSTANCE instance,
    HWND owner,
    EmbeddedHelpDocument document) noexcept;

}  // namespace inkpod::app
