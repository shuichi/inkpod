#pragma once

#include <windows.h>

#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace inkpod::windows::ui {

struct BatchPaletteEntry {
    UINT command;
    const wchar_t* label;
};

const std::array<BatchPaletteEntry, 24U>& BatchPaletteEntries() noexcept;

using BatchPaletteCommandCallback = void (*)(
    void* context, UINT command) noexcept;
using BatchPaletteSelectionCallback = void (*)(
    void* context, std::uint32_t selected_index) noexcept;

struct BatchPaletteDialogState {
    void* context{};
    BatchPaletteCommandCallback dispatch_command{};
    BatchPaletteSelectionCallback select_operation{};
    bool loaded_graph{};
};

struct BatchPaletteView {
    std::wstring input_label;
    std::vector<std::wstring> operation_labels;
    std::uint32_t selected_operation{};
    std::wstring output_text;
    bool loaded_graph{};
    bool idle{true};
    bool runnable{};
};

HWND CreateBatchPaletteDialog(
    HINSTANCE instance, HWND owner, BatchPaletteDialogState& state) noexcept;

void UpdateBatchPaletteDialog(
    HWND dialog, const BatchPaletteView& view) noexcept;

}  // namespace inkpod::windows::ui
