#include "batch_input_picker.h"

#include <commdlg.h>
#include <shobjidl.h>

#include <algorithm>
#include <array>
#include <limits>
#include <new>

namespace inkpod::windows::ui {
namespace {

constexpr std::size_t kMaximumBatchInputFiles = 16'384U;

const wchar_t* FindTerminator(
    const wchar_t* begin, const wchar_t* end) noexcept {
    return std::find(begin, end, L'\0');
}

}  // namespace

bool ParseBatchFileSelection(
    std::span<const wchar_t> buffer,
    std::vector<std::wstring>& paths) noexcept {
    if (buffer.size() < 2U
        || buffer.size() > kMaximumBatchFileSelectionCharacters) {
        return false;
    }
    const wchar_t* const begin = buffer.data();
    const wchar_t* const end = begin + buffer.size();
    const wchar_t* const first_end = FindTerminator(begin, end);
    if (first_end == begin || first_end == end || first_end + 1 == end) {
        return false;
    }

    std::vector<std::wstring> candidate;
    try {
        if (first_end[1] == L'\0') {
            candidate.emplace_back(begin, first_end);
        } else {
            const std::wstring directory(begin, first_end);
            const wchar_t* cursor = first_end + 1;
            while (cursor < end && *cursor != L'\0') {
                const wchar_t* const name_end = FindTerminator(cursor, end);
                if (name_end == end
                    || candidate.size() >= kMaximumBatchInputFiles) {
                    return false;
                }
                std::wstring path = directory;
                if (!path.empty() && path.back() != L'\\'
                    && path.back() != L'/') {
                    path.push_back(L'\\');
                }
                path.append(cursor, name_end);
                candidate.push_back(std::move(path));
                cursor = name_end + 1;
            }
            if (cursor == end || candidate.empty()) {
                return false;
            }
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    paths.swap(candidate);
    return true;
}

bool ChooseBatchInputFiles(
    HWND owner,
    const wchar_t* filter,
    std::vector<std::wstring>& paths) noexcept {
    if (filter == nullptr) {
        return false;
    }
    std::vector<wchar_t> buffer;
    try {
        buffer.resize(kMaximumBatchFileSelectionCharacters, L'\0');
    } catch (const std::bad_alloc&) {
        return false;
    }
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
    dialog.lpstrFile = buffer.data();
    dialog.nMaxFile = static_cast<DWORD>(buffer.size());
    dialog.Flags = OFN_ALLOWMULTISELECT | OFN_EXPLORER | OFN_FILEMUSTEXIST
        | OFN_HIDEREADONLY | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST;
    if (GetOpenFileNameW(&dialog) == FALSE) {
        return false;
    }
    return ParseBatchFileSelection(buffer, paths);
}

bool ChooseBatchFolder(
    HWND owner,
    const wchar_t* title,
    std::wstring& selected_path) noexcept {
    IFileDialog* dialog{};
    if (FAILED(CoCreateInstance(
            CLSID_FileOpenDialog,
            nullptr,
            CLSCTX_INPROC_SERVER,
            IID_PPV_ARGS(&dialog)))) {
        return false;
    }
    DWORD options{};
    HRESULT status = dialog->GetOptions(&options);
    if (SUCCEEDED(status)) {
        status = dialog->SetOptions(
            options | FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR
            | FOS_PATHMUSTEXIST | FOS_PICKFOLDERS);
    }
    if (SUCCEEDED(status) && title != nullptr && title[0] != L'\0') {
        status = dialog->SetTitle(title);
    }
    if (SUCCEEDED(status)) {
        status = dialog->Show(owner);
    }
    IShellItem* item{};
    if (SUCCEEDED(status)) {
        status = dialog->GetResult(&item);
    }
    PWSTR path{};
    if (SUCCEEDED(status)) {
        status = item->GetDisplayName(SIGDN_FILESYSPATH, &path);
    }
    bool selected{};
    if (SUCCEEDED(status) && path != nullptr) {
        try {
            selected_path.assign(path);
            selected = true;
        } catch (const std::bad_alloc&) {
            selected = false;
        }
    }
    CoTaskMemFree(path);
    if (item != nullptr) {
        item->Release();
    }
    dialog->Release();
    return selected;
}

}  // namespace inkpod::windows::ui
