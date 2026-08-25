#pragma once

#include <windows.h>

#include <cstdint>
#include <span>
#include <vector>

#include "shortcut_profile.h"

namespace inkpod::windows::ui {

enum class ShortcutPresetStatus : std::uint32_t {
    Ok = 0U,
    Invalid,
    UnsupportedVersion,
    IoError,
    CapacityExceeded,
};

[[nodiscard]] ShortcutPresetStatus EncodeShortcutPreset(
    const ShortcutProfile& profile,
    std::vector<std::uint8_t>& output) noexcept;

[[nodiscard]] ShortcutPresetStatus DecodeShortcutPreset(
    std::span<const std::uint8_t> bytes,
    ShortcutProfile& output) noexcept;

[[nodiscard]] ShortcutPresetStatus ReadShortcutPreset(
    const wchar_t* path,
    ShortcutProfile& output) noexcept;

[[nodiscard]] ShortcutPresetStatus SaveShortcutPresetAtomic(
    const wchar_t* path,
    const ShortcutProfile& profile) noexcept;

}  // namespace inkpod::windows::ui
