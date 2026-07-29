#pragma once

#include <windows.h>

#include <array>

#include "ui/command_state.h"

namespace inkpod::windows::ui {

struct ToolPaletteEntry {
    UINT command;
    const wchar_t* label;
    const wchar_t* glyph;
};

inline constexpr std::size_t kToolPaletteEntryCount = 20U;

using ToolPaletteCommandCallback = void (*)(
    void* context, UINT command) noexcept;
using ToolPaletteVisibilityCallback = void (*)(void* context) noexcept;

struct ToolPaletteDialogState {
    void* context{};
    ToolPaletteCommandCallback dispatch_command{};
    ToolPaletteVisibilityCallback visibility_changed{};
    int scroll_position{};
    HFONT font{};
    HWND tooltip{};
};

const std::array<ToolPaletteEntry, kToolPaletteEntryCount>&
ToolPaletteEntries() noexcept;

HWND CreateToolPaletteDialog(
    HINSTANCE instance,
    HWND owner,
    ToolPaletteDialogState& state) noexcept;

void UpdateToolPaletteDialog(
    HWND dialog,
    const CommandStateSet& states) noexcept;

bool ToolPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept;

}  // namespace inkpod::windows::ui
