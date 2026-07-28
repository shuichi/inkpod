#pragma once

#include <windows.h>

namespace inkpod::windows::ui {

bool RestorePaletteWindowPlacement(
    HWND palette,
    HWND owner,
    const wchar_t* value_name,
    bool load_persisted) noexcept;

bool SavePaletteWindowPlacement(
    HWND palette,
    const wchar_t* value_name) noexcept;

bool PaletteRectIntersectsCurrentMonitor(const RECT& bounds) noexcept;

bool PaletteWindowIsShown(HWND palette) noexcept;

bool SetPaletteWindowShown(HWND palette, bool shown) noexcept;

}  // namespace inkpod::windows::ui
