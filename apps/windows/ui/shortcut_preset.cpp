#include "shortcut_preset.h"

#include <algorithm>
#include <atomic>
#include <limits>
#include <new>
#include <string>

#include "app/application_settings.h"

namespace inkpod::windows::ui {
namespace {

constexpr std::size_t kMaximumPresetBytes = 2U * 1024U * 1024U;
std::atomic<std::uint32_t> g_temporary_sequence{1U};

ShortcutPresetStatus ToPresetStatus(
    app::ShortcutPresetJsonResult status) noexcept {
    switch (status) {
    case app::ShortcutPresetJsonResult::Ok:
        return ShortcutPresetStatus::Ok;
    case app::ShortcutPresetJsonResult::Invalid:
        return ShortcutPresetStatus::Invalid;
    case app::ShortcutPresetJsonResult::UnsupportedVersion:
        return ShortcutPresetStatus::UnsupportedVersion;
    case app::ShortcutPresetJsonResult::CapacityExceeded:
        return ShortcutPresetStatus::CapacityExceeded;
    }
    return ShortcutPresetStatus::Invalid;
}

bool WriteAll(HANDLE file, std::span<const std::uint8_t> bytes) noexcept {
    std::size_t cursor{};
    while (cursor < bytes.size()) {
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(
            bytes.size() - cursor, std::numeric_limits<DWORD>::max()));
        DWORD written{};
        if (WriteFile(file, bytes.data() + cursor, chunk, &written, nullptr)
                == FALSE
            || written != chunk) {
            return false;
        }
        cursor += written;
    }
    return true;
}

std::wstring TemporaryPath(const wchar_t* path, std::uint32_t sequence) {
    std::wstring result(path);
    result += L".tmp.";
    result += std::to_wstring(GetCurrentProcessId());
    result += L'.';
    result += std::to_wstring(sequence);
    return result;
}

}  // namespace

ShortcutPresetStatus EncodeShortcutPreset(
    const ShortcutProfile& profile,
    std::vector<std::uint8_t>& output) noexcept {
    std::string json;
    const app::ShortcutPresetJsonResult encoded =
        app::EncodeShortcutPresetJson(profile, json);
    if (encoded != app::ShortcutPresetJsonResult::Ok) {
        return ToPresetStatus(encoded);
    }
    try {
        output.assign(json.begin(), json.end());
        return ShortcutPresetStatus::Ok;
    } catch (const std::bad_alloc&) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
}

ShortcutPresetStatus DecodeShortcutPreset(
    std::span<const std::uint8_t> bytes,
    ShortcutProfile& output) noexcept {
    if (bytes.size() > kMaximumPresetBytes) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
    const std::string_view json(
        reinterpret_cast<const char*>(bytes.data()), bytes.size());
    return ToPresetStatus(app::DecodeShortcutPresetJson(json, output));
}

ShortcutPresetStatus ReadShortcutPreset(
    const wchar_t* path,
    ShortcutProfile& output) noexcept {
    if (path == nullptr || path[0] == L'\0') {
        return ShortcutPresetStatus::Invalid;
    }
    const HANDLE file = CreateFileW(
        path,
        GENERIC_READ,
        FILE_SHARE_READ,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_SEQUENTIAL_SCAN,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return ShortcutPresetStatus::IoError;
    }
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) == FALSE || size.QuadPart <= 0
        || size.QuadPart > static_cast<LONGLONG>(kMaximumPresetBytes)) {
        CloseHandle(file);
        return size.QuadPart > static_cast<LONGLONG>(kMaximumPresetBytes)
            ? ShortcutPresetStatus::CapacityExceeded
            : ShortcutPresetStatus::Invalid;
    }
    std::vector<std::uint8_t> bytes;
    try {
        bytes.resize(static_cast<std::size_t>(size.QuadPart));
    } catch (const std::bad_alloc&) {
        CloseHandle(file);
        return ShortcutPresetStatus::CapacityExceeded;
    }
    DWORD read{};
    const bool read_all = bytes.size() <= std::numeric_limits<DWORD>::max()
        && ReadFile(
               file,
               bytes.data(),
               static_cast<DWORD>(bytes.size()),
               &read,
               nullptr)
            != FALSE
        && static_cast<std::size_t>(read) == bytes.size();
    const bool closed = CloseHandle(file) != FALSE;
    return read_all && closed
        ? DecodeShortcutPreset(bytes, output)
        : ShortcutPresetStatus::IoError;
}

ShortcutPresetStatus SaveShortcutPresetAtomic(
    const wchar_t* path,
    const ShortcutProfile& profile) noexcept {
    if (path == nullptr || path[0] == L'\0') {
        return ShortcutPresetStatus::Invalid;
    }
    std::vector<std::uint8_t> bytes;
    const ShortcutPresetStatus encode = EncodeShortcutPreset(profile, bytes);
    if (encode != ShortcutPresetStatus::Ok) {
        return encode;
    }
    std::wstring temporary;
    HANDLE file = INVALID_HANDLE_VALUE;
    try {
        for (std::uint32_t attempt = 0U; attempt < 32U; ++attempt) {
            temporary = TemporaryPath(path, g_temporary_sequence.fetch_add(1U));
            file = CreateFileW(
                temporary.c_str(),
                GENERIC_WRITE,
                0U,
                nullptr,
                CREATE_NEW,
                FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_WRITE_THROUGH,
                nullptr);
            if (file != INVALID_HANDLE_VALUE
                || GetLastError() != ERROR_FILE_EXISTS) {
                break;
            }
        }
    } catch (const std::bad_alloc&) {
        return ShortcutPresetStatus::CapacityExceeded;
    }
    if (file == INVALID_HANDLE_VALUE) {
        return ShortcutPresetStatus::IoError;
    }
    const bool written = WriteAll(file, bytes) && FlushFileBuffers(file) != FALSE;
    const bool closed = CloseHandle(file) != FALSE;
    if (!written || !closed
        || MoveFileExW(
               temporary.c_str(),
               path,
               MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            == FALSE) {
        DeleteFileW(temporary.c_str());
        return ShortcutPresetStatus::IoError;
    }
    return ShortcutPresetStatus::Ok;
}

}  // namespace inkpod::windows::ui
