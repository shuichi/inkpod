#include "batch_set_store.h"

#include <windows.h>
#include <shlobj.h>

#include <algorithm>
#include <cwctype>
#include <new>

namespace inkpod::windows::ui {
namespace {

constexpr std::size_t kMaximumSetNameUnits = 128U;

bool AppendComponent(std::wstring& path, std::wstring_view component) noexcept {
    try {
        if (!path.empty() && path.back() != L'\\' && path.back() != L'/') {
            path.push_back(L'\\');
        }
        path.append(component);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool IsDirectory(const std::wstring& path) noexcept {
    const DWORD attributes = GetFileAttributesW(path.c_str());
    return attributes != INVALID_FILE_ATTRIBUTES
        && (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0U;
}

bool EnsureDirectory(const std::wstring& path) noexcept {
    if (CreateDirectoryW(path.c_str(), nullptr) != FALSE) {
        return true;
    }
    return GetLastError() == ERROR_ALREADY_EXISTS && IsDirectory(path);
}

bool EndsWithCaseInsensitive(
    std::wstring_view value, std::wstring_view suffix) noexcept {
    if (value.size() < suffix.size()) {
        return false;
    }
    const std::wstring_view tail = value.substr(value.size() - suffix.size());
    return CompareStringOrdinal(
               tail.data(),
               static_cast<int>(tail.size()),
               suffix.data(),
               static_cast<int>(suffix.size()),
               TRUE)
        == CSTR_EQUAL;
}

bool EqualsCaseInsensitive(
    std::wstring_view left, std::wstring_view right) noexcept {
    return left.size() == right.size()
        && CompareStringOrdinal(
               left.data(),
               static_cast<int>(left.size()),
               right.data(),
               static_cast<int>(right.size()),
               TRUE)
            == CSTR_EQUAL;
}

bool IsReservedDeviceName(std::wstring_view name) noexcept {
    if (EqualsCaseInsensitive(name, L"CON")
        || EqualsCaseInsensitive(name, L"PRN")
        || EqualsCaseInsensitive(name, L"AUX")
        || EqualsCaseInsensitive(name, L"NUL")) {
        return true;
    }
    if (name.size() == 4U && (EqualsCaseInsensitive(name.substr(0U, 3U), L"COM")
                             || EqualsCaseInsensitive(name.substr(0U, 3U), L"LPT"))) {
        return name[3] >= L'1' && name[3] <= L'9';
    }
    return false;
}

bool CanonicalizeName(
    std::wstring_view requested, std::wstring& canonical) noexcept {
    while (!requested.empty() && std::iswspace(requested.front()) != 0) {
        requested.remove_prefix(1U);
    }
    while (!requested.empty() && std::iswspace(requested.back()) != 0) {
        requested.remove_suffix(1U);
    }
    if (EndsWithCaseInsensitive(requested, kBatchSetExtension)) {
        requested.remove_suffix(kBatchSetExtension.size());
    }
    const std::wstring_view device_stem = requested.substr(
        0U, requested.find(L'.'));
    if (requested.empty() || requested.size() > kMaximumSetNameUnits
        || requested == L"." || requested == L".." || requested.back() == L'.'
        || requested.back() == L' ' || IsReservedDeviceName(device_stem)) {
        return false;
    }
    for (const wchar_t character : requested) {
        if (character < 32 || character == L'<' || character == L'>'
            || character == L':' || character == L'"' || character == L'/'
            || character == L'\\' || character == L'|' || character == L'?'
            || character == L'*') {
            return false;
        }
    }
    try {
        canonical.assign(requested);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool NameLess(const std::wstring& left, const std::wstring& right) noexcept {
    return CompareStringOrdinal(
               left.c_str(),
               static_cast<int>(left.size()),
               right.c_str(),
               static_cast<int>(right.size()),
               TRUE)
        == CSTR_LESS_THAN;
}

} // namespace

bool PrepareDefaultBatchSetDirectory(std::wstring& directory) noexcept {
    PWSTR roaming_path{};
    if (SHGetKnownFolderPath(
            FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, nullptr, &roaming_path)
        != S_OK
        || roaming_path == nullptr) {
        CoTaskMemFree(roaming_path);
        return false;
    }
    std::wstring app_directory;
    try {
        app_directory.assign(roaming_path);
    } catch (const std::bad_alloc&) {
        CoTaskMemFree(roaming_path);
        return false;
    }
    CoTaskMemFree(roaming_path);
    if (!AppendComponent(app_directory, L"inkpod")
        || !EnsureDirectory(app_directory)) {
        return false;
    }
    try {
        directory = app_directory;
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (!AppendComponent(directory, L"batch-sets")) {
        return false;
    }
    return EnsureDirectory(directory);
}

bool EnsureBatchSetDirectory(const std::wstring& directory) noexcept {
    return !directory.empty() && EnsureDirectory(directory);
}

bool EnumerateBatchSetNames(
    const std::wstring& directory,
    std::vector<std::wstring>& names) noexcept {
    std::wstring pattern;
    try {
        pattern = directory;
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (!AppendComponent(pattern, L"*.inkbatch")) {
        return false;
    }
    WIN32_FIND_DATAW entry{};
    const HANDLE search = FindFirstFileW(pattern.c_str(), &entry);
    if (search == INVALID_HANDLE_VALUE) {
        if (GetLastError() == ERROR_FILE_NOT_FOUND) {
            names.clear();
            return true;
        }
        return false;
    }
    std::vector<std::wstring> found;
    bool ok = true;
    do {
        if ((entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0U) {
            continue;
        }
        const std::wstring_view filename(entry.cFileName);
        if (!EndsWithCaseInsensitive(filename, kBatchSetExtension)) {
            continue;
        }
        std::wstring canonical;
        if (!CanonicalizeName(
                filename.substr(0U, filename.size() - kBatchSetExtension.size()),
                canonical)) {
            continue;
        }
        try {
            found.push_back(std::move(canonical));
        } catch (const std::bad_alloc&) {
            ok = false;
            break;
        }
    } while (FindNextFileW(search, &entry) != FALSE);
    const DWORD final_error = GetLastError();
    FindClose(search);
    if (!ok || (final_error != ERROR_NO_MORE_FILES && final_error != ERROR_SUCCESS)) {
        return false;
    }
    std::sort(found.begin(), found.end(), NameLess);
    found.erase(
        std::unique(
            found.begin(),
            found.end(),
            [](const std::wstring& left, const std::wstring& right) noexcept {
                return EqualsCaseInsensitive(left, right);
            }),
        found.end());
    names.swap(found);
    return true;
}

bool ResolveBatchSetPath(
    const std::wstring& directory,
    std::wstring_view requested_name,
    std::wstring& path,
    std::wstring* canonical_name) noexcept {
    std::wstring canonical;
    if (directory.empty() || !CanonicalizeName(requested_name, canonical)) {
        return false;
    }
    try {
        path = directory;
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (!AppendComponent(path, canonical)) {
        return false;
    }
    try {
        path.append(kBatchSetExtension);
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (canonical_name != nullptr) {
        try {
            *canonical_name = canonical;
        } catch (const std::bad_alloc&) {
            return false;
        }
    }
    return true;
}

} // namespace inkpod::windows::ui
