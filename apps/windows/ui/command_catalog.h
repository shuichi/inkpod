#pragma once

#include <windows.h>

#include <span>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

std::span<const UINT> MenuCommandCatalog() noexcept;

std::vector<InkpodShortcutSequenceV2> BuildDefaultShortcutSequences();

const InkpodShortcutSequenceV2* FindShortcutSequence(
    std::span<const InkpodShortcutSequenceV2> bindings,
    UINT command) noexcept;

std::wstring FormatShortcutSequence(const InkpodShortcutSequenceV2& sequence);

std::wstring MenuCommandDisplayName(HMENU menu, UINT command);

void ApplyShortcutLabelsToMenu(
    HMENU menu,
    std::span<const InkpodShortcutSequenceV2> bindings) noexcept;

}  // namespace inkpod::windows::ui
