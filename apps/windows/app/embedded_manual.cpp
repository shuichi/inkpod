#include "embedded_manual.h"

#include <shellapi.h>
#include <shlobj.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <new>
#include <string>
#include <utility>

#include "app_version.h"
#include "resource.h"

namespace inkpod::app {
namespace {

constexpr DWORD kMaximumManualBytes = 8U * 1024U * 1024U;
constexpr std::size_t kMaximumPathCharacters = 32700U;
constexpr DWORD kComparisonBufferBytes = 64U * 1024U;

bool EnsureDirectory(const std::wstring& path) noexcept {
    if (path.empty()) {
        return false;
    }
    if (CreateDirectoryW(path.c_str(), nullptr) != FALSE) {
        return true;
    }
    if (GetLastError() != ERROR_ALREADY_EXISTS) {
        return false;
    }
    const DWORD attributes = GetFileAttributesW(path.c_str());
    return attributes != INVALID_FILE_ATTRIBUTES
        && (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0U;
}

bool AppendPathComponent(
    std::wstring& path,
    const wchar_t* component) {
    if (path.empty() || component == nullptr || component[0] == L'\0') {
        return false;
    }
    if (path.back() != L'\\' && path.back() != L'/') {
        path.push_back(L'\\');
    }
    path.append(component);
    return path.size() <= kMaximumPathCharacters;
}

bool BuildManualPath(
    const std::wstring& local_app_data_root,
    std::wstring& output_path) {
    if (local_app_data_root.empty()
        || local_app_data_root.size() > kMaximumPathCharacters
        || local_app_data_root.find(L'\0') != std::wstring::npos) {
        return false;
    }
    std::wstring path = local_app_data_root;
    if (!EnsureDirectory(path)
        || !AppendPathComponent(path, L"inkpod")
        || !EnsureDirectory(path)
        || !AppendPathComponent(path, L"Help")
        || !EnsureDirectory(path)
        || !AppendPathComponent(path, INKPOD_FILE_VERSION_STRING_WIDE)
        || !EnsureDirectory(path)
        || !AppendPathComponent(path, L"manual.html")) {
        return false;
    }
    output_path = std::move(path);
    return true;
}

bool FileMatchesBytes(
    const std::wstring& path,
    const std::byte* expected,
    DWORD expected_size) noexcept {
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
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) == FALSE
        || size.QuadPart != static_cast<LONGLONG>(expected_size)) {
        CloseHandle(file);
        return false;
    }

    std::array<std::byte, kComparisonBufferBytes> buffer{};
    DWORD offset = 0U;
    bool matches = true;
    while (offset < expected_size) {
        const DWORD remaining = expected_size - offset;
        const DWORD requested = (std::min)(remaining, kComparisonBufferBytes);
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

bool WriteFileAtomic(
    const std::wstring& path,
    const std::byte* bytes,
    DWORD byte_count) noexcept {
    static std::atomic<std::uint32_t> sequence{1U};
    std::wstring temporary;
    HANDLE file = INVALID_HANDLE_VALUE;
    try {
        for (std::uint32_t attempt = 0U; attempt < 16U; ++attempt) {
            temporary = path + L".tmp." + std::to_wstring(GetCurrentProcessId())
                + L"." + std::to_wstring(sequence.fetch_add(1U));
            if (temporary.size() > kMaximumPathCharacters) {
                return false;
            }
            file = CreateFileW(
                temporary.c_str(),
                GENERIC_WRITE,
                0U,
                nullptr,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                nullptr);
            if (file != INVALID_HANDLE_VALUE) {
                break;
            }
            if (GetLastError() != ERROR_FILE_EXISTS
                && GetLastError() != ERROR_ALREADY_EXISTS) {
                return false;
            }
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }

    DWORD written{};
    const bool wrote = WriteFile(file, bytes, byte_count, &written, nullptr) != FALSE
        && written == byte_count;
    const bool flushed = wrote && FlushFileBuffers(file) != FALSE;
    CloseHandle(file);

    const bool replaced = flushed
        && MoveFileExW(
               temporary.c_str(),
               path.c_str(),
               MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            != FALSE;
    if (!replaced) {
        DeleteFileW(temporary.c_str());
    }
    return replaced;
}

}  // namespace

EmbeddedManualStatus ExtractEmbeddedManual(
    HINSTANCE instance,
    const std::wstring& local_app_data_root,
    std::wstring& output_path) noexcept {
    output_path.clear();
    if (instance == nullptr || local_app_data_root.empty()) {
        return EmbeddedManualStatus::InvalidArgument;
    }

    const HRSRC resource = FindResourceW(
        instance,
        MAKEINTRESOURCEW(IDR_MANUAL_HTML),
        RT_RCDATA);
    if (resource == nullptr) {
        return EmbeddedManualStatus::ResourceUnavailable;
    }
    const HGLOBAL loaded = LoadResource(instance, resource);
    const DWORD byte_count = SizeofResource(instance, resource);
    const void* locked = loaded == nullptr ? nullptr : LockResource(loaded);
    if (loaded == nullptr || locked == nullptr || byte_count == 0U
        || byte_count > kMaximumManualBytes) {
        return EmbeddedManualStatus::ResourceUnavailable;
    }
    const auto* bytes = static_cast<const std::byte*>(locked);

    try {
        std::wstring manual_path;
        if (!BuildManualPath(local_app_data_root, manual_path)) {
            return EmbeddedManualStatus::CacheDirectoryUnavailable;
        }
        if (!FileMatchesBytes(manual_path, bytes, byte_count)
            && !WriteFileAtomic(manual_path, bytes, byte_count)) {
            return EmbeddedManualStatus::WriteFailed;
        }
        output_path = std::move(manual_path);
        return EmbeddedManualStatus::Ok;
    } catch (const std::bad_alloc&) {
        return EmbeddedManualStatus::CacheDirectoryUnavailable;
    }
}

EmbeddedManualStatus PrepareEmbeddedManual(
    HINSTANCE instance,
    std::wstring& output_path) noexcept {
    output_path.clear();
    if (instance == nullptr) {
        return EmbeddedManualStatus::InvalidArgument;
    }

    PWSTR local_app_data{};
    if (FAILED(SHGetKnownFolderPath(
            FOLDERID_LocalAppData,
            KF_FLAG_CREATE,
            nullptr,
            &local_app_data))
        || local_app_data == nullptr) {
        return EmbeddedManualStatus::CacheDirectoryUnavailable;
    }
    try {
        const std::wstring root(local_app_data);
        CoTaskMemFree(local_app_data);
        return ExtractEmbeddedManual(instance, root, output_path);
    } catch (const std::bad_alloc&) {
        CoTaskMemFree(local_app_data);
        return EmbeddedManualStatus::CacheDirectoryUnavailable;
    }
}

EmbeddedManualStatus OpenEmbeddedManual(
    HINSTANCE instance,
    HWND owner) noexcept {
    std::wstring path;
    const EmbeddedManualStatus prepared = PrepareEmbeddedManual(instance, path);
    if (prepared != EmbeddedManualStatus::Ok) {
        return prepared;
    }
    const HINSTANCE launched = ShellExecuteW(
        owner,
        L"open",
        path.c_str(),
        nullptr,
        nullptr,
        SW_SHOWNORMAL);
    return reinterpret_cast<INT_PTR>(launched) > 32
        ? EmbeddedManualStatus::Ok
        : EmbeddedManualStatus::LaunchFailed;
}

}  // namespace inkpod::app
