#pragma once

#include <windows.h>

#include <string>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
struct ActivationRequest;
class ApplicationHost;
struct RecoveryCandidate;
struct WorkspaceWindow;
}

namespace inkpod::windows::ui::runtime {

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept;
void ApplySystemDarkTitleBar(HWND window) noexcept;

// Application bootstrap uses these existing UI-coordinated document paths so
// startup follows the same reset, Fit, and command-state behavior as commands.
InkpodStatus CreateDefaultCell(app::ApplicationHost& state) noexcept;
InkpodStatus OpenDocumentFromPath(
    app::ApplicationHost& state, const std::wstring& path) noexcept;
InkpodStatus OpenRecoveryFromPath(
    app::ApplicationHost& state, const std::wstring& path) noexcept;
InkpodStatus OpenRecoveryCandidate(
    app::ApplicationHost& state,
    const app::RecoveryCandidate& candidate) noexcept;
bool HandleApplicationActivation(
    app::ApplicationHost& state,
    const app::ActivationRequest& request) noexcept;
void UpdateMenuState(app::ApplicationHost& state) noexcept;
void ShowInitialPalettes(app::ApplicationHost& state) noexcept;
void CaptureWorkspacePresentation(app::ApplicationHost& state) noexcept;
void ShowCoreError(
    const app::ApplicationHost& state,
    HWND owner,
    const wchar_t* operation) noexcept;
bool PreTranslateKeyboardMessage(app::ApplicationHost& state, const MSG& message) noexcept;
bool HandleWorkspaceNavigation(
    app::ApplicationHost& state,
    HWND window,
    std::uint32_t virtual_key,
    std::uint32_t modifiers) noexcept;
app::WorkspaceWindow* CreateWorkspaceWindow(
    app::ApplicationHost& state, bool show) noexcept;

}  // namespace inkpod::windows::ui::runtime
