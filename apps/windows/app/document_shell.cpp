#include "ui/localization.h"

#include "document_shell.h"

#include <commdlg.h>
#include <objbase.h>
#include <algorithm>
#include <array>
#include <climits>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cwchar>
#include <cwctype>
#include <new>
#include <utility>

#include "session_recovery.h"

namespace inkpod::app {
using windows::ui::UiStringId;
using windows::ui::UiText;

bool WidePathToUtf8(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept {
    if (path.empty() || path.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int required = WideCharToMultiByte(
        CP_UTF8, WC_ERR_INVALID_CHARS, path.data(),
        static_cast<int>(path.size()), nullptr, 0, nullptr, nullptr);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return WideCharToMultiByte(
               CP_UTF8, WC_ERR_INVALID_CHARS, path.data(),
               static_cast<int>(path.size()),
               reinterpret_cast<char*>(output.data()), required, nullptr, nullptr)
        == required;
}

bool ChooseInkpodPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    if (!selected_path.empty()) {
        wcsncpy_s(path.data(), path.size(), selected_path.c_str(), _TRUNCATE);
    }
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = UiText(UiStringId::Text0091);
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.lpstrDefExt = L"inkpod";
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    const BOOL accepted = save
        ? GetSaveFileNameW(&dialog)
        : GetOpenFileNameW(&dialog);
    if (accepted == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ChooseCommonRasterPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    if (!selected_path.empty()) {
        wcsncpy_s(path.data(), path.size(), selected_path.c_str(), _TRUNCATE);
    }
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = UiText(UiStringId::RasterImageFileFilter);
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.lpstrDefExt = L"png";
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    const BOOL accepted = save
        ? GetSaveFileNameW(&dialog)
        : GetOpenFileNameW(&dialog);
    if (accepted == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ChooseCommonRasterPaths(
    HWND owner, std::vector<std::wstring>& selected_paths) noexcept {
    std::vector<wchar_t> buffer;
    try {
        buffer.resize(65536U);
    } catch (const std::bad_alloc&) {
        return false;
    }
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = UiText(UiStringId::CommonRasterFileFilter);
    dialog.lpstrFile = buffer.data();
    dialog.nMaxFile = static_cast<DWORD>(buffer.size());
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | OFN_FILEMUSTEXIST | OFN_ALLOWMULTISELECT;
    if (GetOpenFileNameW(&dialog) == FALSE) {
        return false;
    }
    selected_paths.clear();
    try {
        const std::wstring first(buffer.data());
        const wchar_t* cursor = buffer.data() + first.size() + 1U;
        if (*cursor == L'\0') {
            selected_paths.push_back(first);
            return true;
        }
        while (*cursor != L'\0') {
            const std::wstring name(cursor);
            selected_paths.push_back(first + L"\\" + name);
            cursor += name.size() + 1U;
        }
        return !selected_paths.empty();
    } catch (const std::bad_alloc&) {
        selected_paths.clear();
        return false;
    }
}

bool ChooseOpenDocumentPath(HWND owner, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = UiText(UiStringId::OpenDocumentFileFilter);
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | OFN_FILEMUSTEXIST;
    if (GetOpenFileNameW(&dialog) == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

InkpodCommonRasterFormat CommonRasterFormatFromPath(
    const std::wstring& path) noexcept {
    const std::size_t dot = path.find_last_of(L'.');
    if (dot == std::wstring::npos) {
        return 0U;
    }
    std::wstring extension;
    try {
        extension = path.substr(dot + 1U);
    } catch (const std::bad_alloc&) {
        return 0U;
    }
    std::transform(extension.begin(), extension.end(), extension.begin(), towlower);
    if (extension == L"png") {
        return INKPOD_COMMON_RASTER_PNG;
    }
    if (extension == L"tif" || extension == L"tiff") {
        return INKPOD_COMMON_RASTER_TIFF;
    }
    if (extension == L"tga") {
        return INKPOD_COMMON_RASTER_TGA;
    }
    if (extension == L"bmp") {
        return INKPOD_COMMON_RASTER_BMP;
    }
    return 0U;
}

bool ReadBoundedFile(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept {
    constexpr std::uint64_t maximum_bytes = UINT64_C(512) * 1024U * 1024U;
    HANDLE file = CreateFileW(
        path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_SEQUENTIAL_SCAN, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) == FALSE || size.QuadPart <= 0
        || static_cast<std::uint64_t>(size.QuadPart) > maximum_bytes) {
        CloseHandle(file);
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(size.QuadPart));
    } catch (const std::bad_alloc&) {
        CloseHandle(file);
        return false;
    }
    std::size_t offset{};
    while (offset < output.size()) {
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(
            output.size() - offset, 1024U * 1024U));
        DWORD read{};
        if (ReadFile(file, output.data() + offset, chunk, &read, nullptr) == FALSE
            || read != chunk) {
            CloseHandle(file);
            output.clear();
            return false;
        }
        offset += read;
    }
    const bool closed = CloseHandle(file) != FALSE;
    return closed;
}

bool WriteFileAtomically(
    const std::wstring& path, const std::vector<std::uint8_t>& bytes) noexcept {
    if (path.empty() || bytes.empty()) {
        return false;
    }
    GUID guid{};
    std::array<wchar_t, 40> guid_text{};
    if (FAILED(CoCreateGuid(&guid))
        || StringFromGUID2(
               guid, guid_text.data(), static_cast<int>(guid_text.size())) <= 0) {
        return false;
    }
    std::wstring temporary;
    try {
        temporary = path + L"." + guid_text.data() + L".tmp";
    } catch (const std::bad_alloc&) {
        return false;
    }
    HANDLE file = CreateFileW(
        temporary.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW,
        FILE_ATTRIBUTE_TEMPORARY, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    bool success = true;
    std::size_t offset{};
    while (offset < bytes.size()) {
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(
            bytes.size() - offset, 1024U * 1024U));
        DWORD written{};
        if (WriteFile(file, bytes.data() + offset, chunk, &written, nullptr) == FALSE
            || written != chunk) {
            success = false;
            break;
        }
        offset += written;
    }
    success = success && FlushFileBuffers(file) != FALSE;
    success = CloseHandle(file) != FALSE && success;
    if (success) {
        success = MoveFileExW(
                      temporary.c_str(), path.c_str(),
                      MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            != FALSE;
    }
    if (!success) {
        DeleteFileW(temporary.c_str());
    }
    return success;
}

bool PrivateRecoveryPath(
    std::uint64_t uuid_high,
    std::uint64_t uuid_low,
    std::wstring& output) noexcept {
    if (uuid_high == 0U && uuid_low == 0U) {
        return false;
    }
    std::wstring directory;
    if (!RecoveryRootDirectory(directory)) {
        return false;
    }
    std::array<wchar_t, 96> name{};
    _snwprintf_s(
        name.data(), name.size(), _TRUNCATE, L"\\%016llx%016llx.inkpod",
        static_cast<unsigned long long>(uuid_high),
        static_cast<unsigned long long>(uuid_low));
    try {
        output = directory + name.data();
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

} // namespace inkpod::app
