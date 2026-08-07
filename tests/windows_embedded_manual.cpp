#include <windows.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>

#include "app/embedded_manual.h"
#include "app/resource.h"
#include "app_version.h"

namespace {

bool FileMatchesResource(
    HINSTANCE instance,
    const std::wstring& path,
    UINT resource_id) noexcept {
    const HRSRC resource = FindResourceW(
        instance,
        MAKEINTRESOURCEW(resource_id),
        RT_RCDATA);
    if (resource == nullptr) {
        return false;
    }
    const HGLOBAL loaded = LoadResource(instance, resource);
    const DWORD resource_size = SizeofResource(instance, resource);
    const void* resource_bytes = loaded == nullptr ? nullptr : LockResource(loaded);
    if (loaded == nullptr || resource_bytes == nullptr || resource_size == 0U) {
        return false;
    }

    HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    LARGE_INTEGER file_size{};
    if (GetFileSizeEx(file, &file_size) == FALSE
        || file_size.QuadPart != static_cast<LONGLONG>(resource_size)) {
        CloseHandle(file);
        return false;
    }

    constexpr DWORD kBufferBytes = 64U * 1024U;
    std::array<std::byte, kBufferBytes> buffer{};
    const auto* expected = static_cast<const std::byte*>(resource_bytes);
    DWORD offset = 0U;
    bool matches = true;
    while (offset < resource_size) {
        const DWORD remaining = resource_size - offset;
        const DWORD requested = (std::min)(remaining, kBufferBytes);
        DWORD read{};
        if (ReadFile(file, buffer.data(), requested, &read, nullptr) == FALSE
            || read != requested
            || std::memcmp(buffer.data(), expected + offset, requested) != 0) {
            matches = false;
            break;
        }
        offset += requested;
    }
    CloseHandle(file);
    return matches;
}

bool OverwriteWithStaleContent(const std::wstring& path) noexcept {
    HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_WRITE,
        0U,
        nullptr,
        CREATE_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    constexpr std::array<std::uint8_t, 5U> stale{{'s', 't', 'a', 'l', 'e'}};
    DWORD written{};
    const bool success = WriteFile(
                             file,
                             stale.data(),
                             static_cast<DWORD>(stale.size()),
                             &written,
                             nullptr)
            != FALSE
        && written == static_cast<DWORD>(stale.size());
    CloseHandle(file);
    return success;
}

void CleanupTestDirectory(
    const std::wstring& root,
    const std::wstring& manual_path,
    const std::wstring& file_format_path) noexcept {
    DeleteFileW(manual_path.c_str());
    DeleteFileW(file_format_path.c_str());
    const std::wstring version_directory =
        root + L"\\inkpod\\Help\\" INKPOD_FILE_VERSION_STRING_WIDE;
    RemoveDirectoryW(version_directory.c_str());
    RemoveDirectoryW((root + L"\\inkpod\\Help").c_str());
    RemoveDirectoryW((root + L"\\inkpod").c_str());
    RemoveDirectoryW(root.c_str());
}

}  // namespace

int main() {
    using inkpod::app::EmbeddedHelpDocument;
    using inkpod::app::EmbeddedHelpStatus;
    using inkpod::app::ExtractEmbeddedHelpDocument;

    const HINSTANCE instance = GetModuleHandleW(nullptr);
    std::wstring output;
    if (ExtractEmbeddedHelpDocument(
            nullptr,
            L"C:\\invalid",
            EmbeddedHelpDocument::Manual,
            output)
            != EmbeddedHelpStatus::InvalidArgument
        || ExtractEmbeddedHelpDocument(
               instance,
               L"",
               EmbeddedHelpDocument::Manual,
               output)
            != EmbeddedHelpStatus::InvalidArgument
        || ExtractEmbeddedHelpDocument(
               instance,
               L"C:\\invalid",
               static_cast<EmbeddedHelpDocument>(255U),
               output)
            != EmbeddedHelpStatus::InvalidArgument) {
        return 1;
    }

    std::array<wchar_t, 32768U> temporary_root{};
    const DWORD temporary_length = GetTempPathW(
        static_cast<DWORD>(temporary_root.size()),
        temporary_root.data());
    if (temporary_length == 0U
        || temporary_length >= static_cast<DWORD>(temporary_root.size())) {
        return 2;
    }
    const std::wstring root = std::wstring(
        temporary_root.data(),
        static_cast<std::size_t>(temporary_length))
        + L"inkpod-embedded-manual-test-"
        + std::to_wstring(GetCurrentProcessId()) + L"-"
        + std::to_wstring(GetTickCount64());
    if (CreateDirectoryW(root.c_str(), nullptr) == FALSE) {
        return 3;
    }

    struct TestDocument {
        EmbeddedHelpDocument document;
        UINT resource_id;
        const wchar_t* file_name;
    };
    constexpr std::array<TestDocument, 2U> documents{{
        {EmbeddedHelpDocument::Manual, IDR_MANUAL_HTML, L"manual.html"},
        {EmbeddedHelpDocument::FileFormat,
         IDR_FILE_FORMAT_HTML,
         L"file_format.html"},
    }};
    std::array<std::wstring, documents.size()> extracted_paths{};
    for (std::size_t index = 0U; index < documents.size(); ++index) {
        const TestDocument& test = documents[index];
        const std::wstring expected = root + L"\\inkpod\\Help\\"
            INKPOD_FILE_VERSION_STRING_WIDE L"\\" + test.file_name;
        const EmbeddedHelpStatus first = ExtractEmbeddedHelpDocument(
            instance, root, test.document, output);
        extracted_paths[index] = output;
        if (first != EmbeddedHelpStatus::Ok || output != expected
            || !FileMatchesResource(instance, output, test.resource_id)) {
            CleanupTestDirectory(root, extracted_paths[0], extracted_paths[1]);
            return 4;
        }

        std::wstring reused_path;
        if (ExtractEmbeddedHelpDocument(
                instance, root, test.document, reused_path)
                != EmbeddedHelpStatus::Ok
            || reused_path != expected
            || !FileMatchesResource(instance, reused_path, test.resource_id)) {
            CleanupTestDirectory(root, extracted_paths[0], extracted_paths[1]);
            return 5;
        }

        if (!OverwriteWithStaleContent(output)) {
            CleanupTestDirectory(root, extracted_paths[0], extracted_paths[1]);
            return 6;
        }
        std::wstring replaced_path;
        if (ExtractEmbeddedHelpDocument(
                instance, root, test.document, replaced_path)
                != EmbeddedHelpStatus::Ok
            || replaced_path != expected
            || !FileMatchesResource(instance, replaced_path, test.resource_id)) {
            CleanupTestDirectory(root, extracted_paths[0], extracted_paths[1]);
            return 7;
        }
    }

    CleanupTestDirectory(root, extracted_paths[0], extracted_paths[1]);
    return 0;
}
