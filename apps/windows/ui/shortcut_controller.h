#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "app/core_engine.h"
#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

struct ShortcutUiState {
    std::vector<InkpodShortcutSequence> bindings;
    std::vector<InkpodShortcutStroke> pending_strokes;
    ULONGLONG pending_deadline{};
    std::wstring pending_text;
};

InkpodStatus InitializeShortcuts(
    app::CoreEngine& engine,
    ShortcutUiState& state,
    bool load_persisted) noexcept;

InkpodStatus ResetShortcuts(
    app::CoreEngine& engine,
    ShortcutUiState& state,
    bool persist) noexcept;

InkpodStatus RebindShortcut(
    app::CoreEngine& engine,
    ShortcutUiState& state,
    const InkpodShortcutSequence& replacement,
    bool persist) noexcept;

InkpodShortcutMatch ResolveShortcutStroke(
    ShortcutUiState& state,
    InkpodShortcutStroke stroke,
    UINT& command) noexcept;

void ClearPendingShortcut(ShortcutUiState& state) noexcept;

}  // namespace inkpod::windows::ui
