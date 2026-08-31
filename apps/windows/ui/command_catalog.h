#pragma once

#include <windows.h>

#include <span>
#include <string_view>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"
#include "shortcut_profile.h"

namespace inkpod::windows::ui {

std::span<const UINT> MenuCommandCatalog() noexcept;

bool IsMenuCommand(UINT command) noexcept;

std::span<const UINT> ShortcutCommandCatalog() noexcept;

std::vector<InkpodShortcutSequence> BuildDefaultShortcutSequences();

ShortcutProfile BuildDefaultShortcutProfile(std::wstring name);

const InkpodShortcutSequence* FindShortcutSequence(
    std::span<const InkpodShortcutSequence> bindings,
    UINT command) noexcept;

std::wstring FormatShortcutSequence(const InkpodShortcutSequence& sequence);

std::wstring MenuCommandDisplayName(HMENU menu, UINT command);

std::string CommandStableKey(UINT command);

UINT CommandFromStableKey(std::string_view key) noexcept;

ShortcutContext DefaultShortcutContext(UINT command) noexcept;

std::uint32_t SupportedShortcutActionMask(UINT command) noexcept;

ShortcutAction DefaultShortcutAction(UINT command) noexcept;

void ApplyShortcutLabelsToMenu(
    HMENU menu,
    std::span<const InkpodShortcutSequence> bindings) noexcept;

}  // namespace inkpod::windows::ui
