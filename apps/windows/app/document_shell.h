#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"
#include "session_recovery.h"

namespace inkpod::app {

bool WidePathToUtf8(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept;
bool ChooseInkpodPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept;
bool ChooseCommonRasterPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept;
bool ChooseCommonRasterPaths(
    HWND owner, std::vector<std::wstring>& selected_paths) noexcept;
bool ChooseOpenDocumentPath(HWND owner, std::wstring& selected_path) noexcept;
InkpodCommonRasterFormat CommonRasterFormatFromPath(
    const std::wstring& path) noexcept;

bool ReadBoundedFile(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept;
bool WriteFileAtomically(
    const std::wstring& path, const std::vector<std::uint8_t>& bytes) noexcept;

bool PrivateRecoveryPath(
    std::uint64_t uuid_high,
    std::uint64_t uuid_low,
    std::wstring& output) noexcept;
bool PrivateRecoveryAttemptPath(
    std::uint64_t uuid_high,
    std::uint64_t uuid_low,
    std::wstring& output) noexcept;

} // namespace inkpod::app
