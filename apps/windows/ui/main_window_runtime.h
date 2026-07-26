#pragma once

#include <windows.h>

#include <string>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
struct AppContext;
}

namespace inkpod::windows::ui::runtime {

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept;

// Application bootstrap uses these existing UI-coordinated document paths so
// startup follows the same reset, Fit, and command-state behavior as commands.
InkpodStatus CreateDefaultCell(app::AppContext& state) noexcept;
InkpodStatus OpenRecoveryFromPath(
    app::AppContext& state, const std::wstring& path) noexcept;
void UpdateMenuState(app::AppContext& state) noexcept;
void ShowCoreError(
    const app::AppContext& state,
    HWND owner,
    const wchar_t* operation) noexcept;
bool PreTranslateKeyboardMessage(app::AppContext& state, const MSG& message) noexcept;

}  // namespace inkpod::windows::ui::runtime
