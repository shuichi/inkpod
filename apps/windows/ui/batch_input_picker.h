#pragma once

#include <windows.h>

#include <cstddef>
#include <span>
#include <string>
#include <vector>

namespace inkpod::windows::ui {

inline constexpr std::size_t kMaximumBatchFileSelectionCharacters = 65'536U;

[[nodiscard]] bool ParseBatchFileSelection(
    std::span<const wchar_t> buffer,
    std::vector<std::wstring>& paths) noexcept;

[[nodiscard]] bool ChooseBatchInputFiles(
    HWND owner,
    const wchar_t* filter,
    std::vector<std::wstring>& paths) noexcept;

[[nodiscard]] bool ChooseBatchFolder(
    HWND owner,
    const wchar_t* title,
    std::wstring& selected_path) noexcept;

}  // namespace inkpod::windows::ui
