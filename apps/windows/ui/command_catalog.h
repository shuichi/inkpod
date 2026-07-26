#pragma once

#include <windows.h>

#include <span>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

std::span<const UINT> MenuCommandCatalog() noexcept;

std::vector<InkpodShortcutSequence> BuildDefaultShortcutSequences();

const InkpodShortcutSequence* FindShortcutSequence(
    std::span<const InkpodShortcutSequence> bindings,
    UINT command) noexcept;

std::wstring FormatShortcutSequence(const InkpodShortcutSequence& sequence);

std::wstring MenuCommandDisplayName(HMENU menu, UINT command);

void ApplyShortcutLabelsToMenu(
    HMENU menu,
    std::span<const InkpodShortcutSequence> bindings) noexcept;

}  // namespace inkpod::windows::ui
