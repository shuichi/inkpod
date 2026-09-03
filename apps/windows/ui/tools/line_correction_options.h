#pragma once

#include "ui/dialogs/effects_dialogs.h"

namespace inkpod::windows::ui::tools {

bool IsLineCorrectionCommand(UINT command) noexcept;
bool IsGlobalLineCorrectionCommand(UINT command) noexcept;
bool PrepareLineCorrectionEditor(
    UINT command, EffectEditorState& editor, std::uint32_t& interaction) noexcept;
bool ReadLineBackground(
    const EffectEditorState& editor, std::array<std::uint16_t, 4U>& color) noexcept;

}  // namespace inkpod::windows::ui::tools
