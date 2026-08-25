#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "app/core_host.h"
#include "inkpod/core_ffi.h"
#include "shortcut_profile.h"

namespace inkpod::windows::ui {

struct ShortcutUiState {
    std::vector<InkpodShortcutSequence> bindings;
    ShortcutProfileSet profile_set;
    std::vector<ShortcutInputStroke> pending_strokes;
    ShortcutContext pending_context{ShortcutContext::Global};
    ULONGLONG pending_deadline{};
    std::wstring pending_text;
    bool hold_active{};
    HWND hold_workspace_window{};
    std::uint32_t hold_physical_key{};
    std::uint32_t hold_restore_tool{};
    InkpodSelectionShape hold_restore_selection_shape{INKPOD_SELECTION_RECTANGLE};
    InkpodFillOperation hold_restore_fill_operation{INKPOD_FILL_SEED};
};

InkpodStatus InitializeShortcuts(
    app::CoreHost& engine,
    ShortcutUiState& state,
    bool load_persisted) noexcept;

InkpodStatus ResetShortcuts(
    app::CoreHost& engine,
    ShortcutUiState& state,
    bool persist) noexcept;

InkpodStatus RebindShortcut(
    app::CoreHost& engine,
    ShortcutUiState& state,
    const InkpodShortcutSequence& replacement,
    bool persist) noexcept;

InkpodStatus ApplyShortcutProfileSet(
    app::CoreHost& engine,
    ShortcutUiState& state,
    const ShortcutProfileSet& replacement,
    bool persist) noexcept;

[[nodiscard]] const ShortcutProfile* ActiveShortcutProfile(
    const ShortcutUiState& state) noexcept;

[[nodiscard]] ShortcutProfile* ActiveShortcutProfile(
    ShortcutUiState& state) noexcept;

ShortcutResolution ResolveShortcutStroke(
    ShortcutUiState& state,
    ShortcutContext context,
    ShortcutInputStroke stroke) noexcept;

InkpodShortcutMatch ResolveShortcutStroke(
    ShortcutUiState& state,
    InkpodShortcutStroke stroke,
    UINT& command) noexcept;

void ClearPendingShortcut(ShortcutUiState& state) noexcept;

}  // namespace inkpod::windows::ui
