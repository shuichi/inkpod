#pragma once

#include <windows.h>

#include <array>
#include <cstdint>

#include "ui/command_state.h"

namespace inkpod::windows::ui {

enum class ToolPalettePage : std::uint8_t {
    Basic,
    Vector,
    Effects,
};

inline constexpr std::size_t kToolPalettePageCount = 3U;

struct ToolPaletteEntry {
    UINT command;
    const wchar_t* label;
    ToolPalettePage page;
};

inline constexpr std::size_t kToolPaletteEntryCount = 37U;

using ToolPaletteCommandCallback = void (*)(
    void* context, UINT command) noexcept;
using ToolPaletteVisibilityCallback = void (*)(void* context) noexcept;

struct ToolPaletteDialogState {
    void* context{};
    ToolPaletteCommandCallback dispatch_command{};
    ToolPaletteVisibilityCallback visibility_changed{};
    int scroll_position{};
    ToolPalettePage active_page{ToolPalettePage::Basic};
    HFONT font{};
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
