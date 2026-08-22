#pragma once

#include <windows.h>

#include <array>
#include <cstdint>
#include <string>
#include <vector>

#include "ui/batch_parameter_editor.h"

namespace inkpod::windows::ui {

struct BatchPaletteEntry {
    UINT command;
    const wchar_t* label;
};

const std::array<BatchPaletteEntry, 4U>& BatchPaletteEntries() noexcept;

using BatchPaletteCommandCallback = void (*)(
    void* context, UINT command) noexcept;
using BatchPaletteSelectionCallback = void (*)(
    void* context, std::uint32_t selected_index) noexcept;
using BatchPaletteRefreshCallback = void (*)(void* context) noexcept;

struct BatchPaletteDialogState {
    void* context{};
    BatchPaletteCommandCallback dispatch_command{};
    BatchPaletteSelectionCallback select_operation{};
    BatchPaletteRefreshCallback refresh{};
    BatchParameterEditorBinding parameter_editor{};
    HWND parameter_host{};
};

struct BatchPaletteView {
    std::wstring target_text;
    std::wstring job_text;
    std::wstring set_name;
    std::vector<std::wstring> stage_labels;
    std::uint32_t selected_stage{};
    std::wstring validation_text;
    bool idle{true};
    bool runnable{};
    bool target_available{};
    bool pinned{};
};

HWND CreateBatchPaletteDialog(
    HINSTANCE instance, HWND owner, BatchPaletteDialogState& state) noexcept;

void UpdateBatchPaletteDialog(
    HWND dialog, const BatchPaletteView& view) noexcept;

}  // namespace inkpod::windows::ui
