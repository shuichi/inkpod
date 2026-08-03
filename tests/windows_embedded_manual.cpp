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
    const std::wstring& path) noexcept {
    const HRSRC resource = FindResourceW(
        instance,
        MAKEINTRESOURCEW(IDR_MANUAL_HTML),
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
    const std::wstring& manual_path) noexcept {
    DeleteFileW(manual_path.c_str());
    const std::wstring version_directory =
        root + L"\\inkpod\\Help\\" INKPOD_FILE_VERSION_STRING_WIDE;
    RemoveDirectoryW(version_directory.c_str());
    RemoveDirectoryW((root + L"\\inkpod\\Help").c_str());
    RemoveDirectoryW((root + L"\\inkpod").c_str());
    RemoveDirectoryW(root.c_str());
}

}  // namespace

int main() {
    using inkpod::app::EmbeddedManualStatus;
    using inkpod::app::ExtractEmbeddedManual;

    const HINSTANCE instance = GetModuleHandleW(nullptr);
    std::wstring output;
    if (ExtractEmbeddedManual(nullptr, L"C:\\invalid", output)
            != EmbeddedManualStatus::InvalidArgument
        || ExtractEmbeddedManual(instance, L"", output)
            != EmbeddedManualStatus::InvalidArgument) {
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

    const EmbeddedManualStatus first = ExtractEmbeddedManual(instance, root, output);
    const std::wstring expected =
        root + L"\\inkpod\\Help\\" INKPOD_FILE_VERSION_STRING_WIDE L"\\manual.html";
    if (first != EmbeddedManualStatus::Ok || output != expected
        || !FileMatchesResource(instance, output)) {
        CleanupTestDirectory(root, output);
        return 4;
    }

    std::wstring reused_path;
    if (ExtractEmbeddedManual(instance, root, reused_path) != EmbeddedManualStatus::Ok
        || reused_path != expected || !FileMatchesResource(instance, reused_path)) {
        CleanupTestDirectory(root, output);
        return 5;
    }

    if (!OverwriteWithStaleContent(output)) {
        CleanupTestDirectory(root, output);
        return 6;
    }
    std::wstring replaced_path;
    if (ExtractEmbeddedManual(instance, root, replaced_path)
            != EmbeddedManualStatus::Ok
        || replaced_path != expected
        || !FileMatchesResource(instance, replaced_path)) {
        CleanupTestDirectory(root, output);
        return 7;
    }

    CleanupTestDirectory(root, output);
    return 0;
}
