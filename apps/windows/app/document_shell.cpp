#include "document_shell.h"

#include <commdlg.h>
#include <shlobj.h>

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

#include "frontend_state.h"
#include "core_host.h"

namespace inkpod::app {
namespace {

InkpodDocumentInfo EmptyDocumentInfo() noexcept {
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return info;
}

bool EnsureDirectory(const std::wstring& path) noexcept {
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

bool RecoveryDirectory(std::wstring& output) noexcept {
    PWSTR local_app_data{};
    if (FAILED(SHGetKnownFolderPath(
            FOLDERID_LocalAppData, KF_FLAG_CREATE, nullptr, &local_app_data))) {
        return false;
    }
    try {
        std::wstring root(local_app_data);
        CoTaskMemFree(local_app_data);
        local_app_data = nullptr;
        root += L"\\inkpod";
        if (!EnsureDirectory(root)) {
            return false;
        }
        root += L"\\Recovery";
        if (!EnsureDirectory(root)) {
            return false;
        }
        output = std::move(root);
        return true;
    } catch (const std::bad_alloc&) {
        if (local_app_data != nullptr) {
            CoTaskMemFree(local_app_data);
        }
        return false;
    }
}

} // namespace

DocumentShellController::DocumentShellController(
    DocumentShellState& state, CoreHost& engine) noexcept
    : state_(state), engine_(engine) {}

InkpodStatus DocumentShellController::Save(const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::wstring old_recovery_path;
    std::wstring next_current_path;
    std::wstring next_recovery_path;
    try {
        old_recovery_path = state_.recovery_path;
        next_current_path = path;
        next_recovery_path = path + L".recovery.inkpod";
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = engine_.Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_save(core, utf8.data(), utf8.size(), &info);
        },
        false,
        true);
    if (status == INKPOD_STATUS_OK) {
        state_.current_path = std::move(next_current_path);
        state_.recovery_path = std::move(next_recovery_path);
        if (!old_recovery_path.empty() && old_recovery_path != path) {
            DeleteFileW(old_recovery_path.c_str());
        }
    }
    return status;
}

InkpodStatus DocumentShellController::Open(const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::wstring next_current_path;
    std::wstring next_recovery_path;
    try {
        next_current_path = path;
        next_recovery_path = path + L".recovery.inkpod";
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = engine_.Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_open(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        state_.current_path = std::move(next_current_path);
        state_.recovery_path = std::move(next_recovery_path);
    }
    return status;
}

InkpodStatus DocumentShellController::OpenRecovery(const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::wstring next_recovery_path;
    try {
        next_recovery_path = path;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = engine_.Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_open_recovery(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        state_.current_path.clear();
        state_.recovery_path = std::move(next_recovery_path);
    }
    return status;
}

InkpodStatus DocumentShellController::ImportCommonRaster(
    const std::wstring& path) noexcept {
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(path);
    std::vector<std::uint8_t> bytes;
    if (format == 0U || !ReadBoundedFile(path, bytes)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    GUID uuid{};
    if (FAILED(CoCreateGuid(&uuid))) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t high{};
    std::uint64_t low{};
    std::memcpy(&high, &uuid, sizeof(high));
    std::memcpy(
        &low, reinterpret_cast<const std::uint8_t*>(&uuid) + sizeof(high),
        sizeof(low));
    const InkpodStatus status = engine_.Invoke(
        [format, bytes = std::move(bytes), high, low](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_import_common_raster(
                core, format, bytes.data(), bytes.size(), high, low, &info);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        state_.current_path.clear();
        state_.recovery_path.clear();
    }
    return status;
}

InkpodStatus DocumentShellController::ExportCommonRaster(
    const std::wstring& path, bool composite_white) noexcept {
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(path);
    if (format == 0U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<std::uint8_t> bytes;
    const InkpodStatus status = engine_.Invoke(
        [format, composite_white, &bytes](InkpodCore* core) {
            InkpodByteBuffer* buffer{};
            InkpodStatus current = inkpod_core_export_common_raster(
                core, format, composite_white ? 1U : 0U, &buffer);
            const std::uint8_t* data{};
            std::uint64_t length{};
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_byte_buffer_view(buffer, &data, &length);
            }
            if (current == INKPOD_STATUS_OK) {
                try {
                    bytes.assign(data, data + static_cast<std::size_t>(length));
                } catch (const std::bad_alloc&) {
                    current = INKPOD_STATUS_INVALID_STATE;
                }
            }
            const InkpodStatus release_status = inkpod_byte_buffer_release(&buffer);
            return current == INKPOD_STATUS_OK ? release_status : current;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    return WriteFileAtomically(path, bytes)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_IO_ERROR;
}

bool DocumentShellController::QueueAutosave(
    const CommandContext& context,
    const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return false;
    }
    return engine_.Enqueue(
        context,
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_autosave(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false,
        true);
}

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
    constexpr wchar_t filter[] =
        L"inkpod セル (*.inkpod)\0*.inkpod\0すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
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
    constexpr wchar_t filter[] =
        L"対応画像 (*.png;*.tif;*.tiff;*.tga;*.bmp)\0*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"PNG (*.png)\0*.png\0TIFF (*.tif;*.tiff)\0*.tif;*.tiff\0"
        L"TGA (*.tga)\0*.tga\0BMP (*.bmp)\0*.bmp\0すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
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
    constexpr wchar_t filter[] =
        L"対応画像 (*.png;*.tif;*.tiff;*.tga;*.bmp)\0*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
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
    constexpr wchar_t filter[] =
        L"inkpod/対応画像 (*.inkpod;*.png;*.tif;*.tiff;*.tga;*.bmp)\0"
        L"*.inkpod;*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"inkpod セル (*.inkpod)\0*.inkpod\0対応画像\0*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
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
    std::wstring directory;
    if (!RecoveryDirectory(directory)) {
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

bool NewestPrivateRecovery(std::wstring& output) noexcept {
    std::wstring directory;
    if (!RecoveryDirectory(directory)) {
        return false;
    }
    std::wstring pattern;
    try {
        pattern = directory + L"\\*.inkpod";
    } catch (const std::bad_alloc&) {
        return false;
    }
    WIN32_FIND_DATAW entry{};
    HANDLE search = FindFirstFileW(pattern.c_str(), &entry);
    if (search == INVALID_HANDLE_VALUE) {
        return false;
    }
    FILETIME newest{};
    bool found{};
    do {
        if ((entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) == 0U
            && (!found || CompareFileTime(&entry.ftLastWriteTime, &newest) > 0)) {
            try {
                output = directory + L"\\" + entry.cFileName;
            } catch (const std::bad_alloc&) {
                FindClose(search);
                return false;
            }
            newest = entry.ftLastWriteTime;
            found = true;
        }
    } while (FindNextFileW(search, &entry) != FALSE);
    FindClose(search);
    return found;
}

bool RecoveryIsNewer(
    const std::wstring& normal_path,
    const std::wstring& recovery_path) noexcept {
    WIN32_FILE_ATTRIBUTE_DATA recovery{};
    if (GetFileAttributesExW(
            recovery_path.c_str(), GetFileExInfoStandard, &recovery) == FALSE) {
        return false;
    }
    WIN32_FILE_ATTRIBUTE_DATA normal{};
    if (GetFileAttributesExW(
            normal_path.c_str(), GetFileExInfoStandard, &normal) == FALSE) {
        return true;
    }
    return CompareFileTime(&recovery.ftLastWriteTime, &normal.ftLastWriteTime) > 0;
}

} // namespace inkpod::app
