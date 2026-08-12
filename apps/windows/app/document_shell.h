#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "command_context.h"
#include "inkpod/core_ffi.h"
#include "session_recovery.h"

namespace inkpod::app {

class CoreHost;
struct DocumentShellState;

class DocumentShellController final {
public:
    DocumentShellController(
        DocumentShellState& state,
        CoreHost& engine,
        DocumentSessionId session,
        Generation generation) noexcept;

    InkpodStatus Save(const std::wstring& path) noexcept;
    InkpodStatus Open(const std::wstring& path) noexcept;
    InkpodStatus OpenRecovery(const std::wstring& path) noexcept;
    InkpodStatus ImportCommonRaster(const std::wstring& path) noexcept;
    InkpodStatus ExportCommonRaster(
        const std::wstring& path, bool composite_white) noexcept;
    InkpodStatus ExportInstructionCommonRaster(
        const std::wstring& path, bool composite_white) noexcept;
    bool QueueAutosave(
        const CommandContext& context,
        const std::wstring& path,
        const RecoveryMetadata& metadata) noexcept;

private:
    DocumentShellState& state_;
    CoreHost& engine_;
    DocumentSessionId session_{};
    Generation generation_{};
};

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
bool RecoveryIsNewer(
    const std::wstring& normal_path,
    const std::wstring& recovery_path) noexcept;

} // namespace inkpod::app
