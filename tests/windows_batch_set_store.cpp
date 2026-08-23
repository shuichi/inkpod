#include "ui/batch_set_store.h"

#include <windows.h>

#include <array>
#include <string>
#include <vector>

namespace {

bool CreateEmptyFile(const std::wstring& path) noexcept {
    const HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_WRITE,
        0,
        nullptr,
        CREATE_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    return CloseHandle(file) != FALSE;
}

bool Contains(
    const std::vector<std::wstring>& names, const wchar_t* expected) noexcept {
    for (const auto& name : names) {
        if (CompareStringOrdinal(name.c_str(), -1, expected, -1, TRUE)
            == CSTR_EQUAL) {
            return true;
        }
    }
    return false;
}

} // namespace

int wmain() {
    std::array<wchar_t, MAX_PATH> temporary{};
    if (GetTempPathW(static_cast<DWORD>(temporary.size()), temporary.data()) == 0U) {
        return 1;
    }
    const std::wstring directory = std::wstring(temporary.data())
        + L"inkpod-batch-set-store-" + std::to_wstring(GetCurrentProcessId())
        + L"-" + std::to_wstring(GetTickCount64());
    if (!inkpod::windows::ui::EnsureBatchSetDirectory(directory)) {
        return 2;
    }

    std::wstring alpha_path;
    std::wstring canonical;
    std::wstring beta_path;
    std::wstring ignored_path = directory + L"\\ignored.txt";
    if (!inkpod::windows::ui::ResolveBatchSetPath(
            directory, L"  Alpha.inkbatch  ", alpha_path, &canonical)
        || canonical != L"Alpha"
        || !inkpod::windows::ui::ResolveBatchSetPath(
            directory, L"beta", beta_path)
        || !CreateEmptyFile(alpha_path) || !CreateEmptyFile(beta_path)
        || !CreateEmptyFile(ignored_path)) {
        DeleteFileW(alpha_path.c_str());
        DeleteFileW(beta_path.c_str());
        DeleteFileW(ignored_path.c_str());
        RemoveDirectoryW(directory.c_str());
        return 3;
    }

    std::vector<std::wstring> names;
    const bool listed = inkpod::windows::ui::EnumerateBatchSetNames(
        directory, names);
    const bool rejected = !inkpod::windows::ui::ResolveBatchSetPath(
            directory, L"..\\escape", canonical)
        && !inkpod::windows::ui::ResolveBatchSetPath(
            directory, L"CON", canonical)
        && !inkpod::windows::ui::ResolveBatchSetPath(
            directory, L"COM1.archive", canonical)
        && !inkpod::windows::ui::ResolveBatchSetPath(
            directory, L"name.", canonical);

    DeleteFileW(alpha_path.c_str());
    DeleteFileW(beta_path.c_str());
    DeleteFileW(ignored_path.c_str());
    RemoveDirectoryW(directory.c_str());
    return listed && names.size() == 2U && Contains(names, L"Alpha")
            && Contains(names, L"beta") && rejected
        ? 0
        : 4;
}
