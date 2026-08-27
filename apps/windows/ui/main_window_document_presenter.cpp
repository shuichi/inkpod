#include "main_window_runtime.h"

#include "app/activation.h"
#include "app/application_host.h"
#include "app/session_recovery.h"
#include "main_window_runtime_internal.h"

namespace inkpod::windows::ui::runtime {

InkpodStatus CreateDefaultCell(app::ApplicationHost& state) noexcept {
    return CreateDefaultCellImpl(state);
}

InkpodStatus OpenDocumentFromPath(
    app::ApplicationHost& state,
    const std::wstring& path) noexcept {
    return OpenDocumentFromPathImpl(state, path);
}

InkpodStatus OpenRecoveryFromPath(
    app::ApplicationHost& state,
    const std::wstring& path) noexcept {
    return OpenRecoveryFromPathImpl(state, path);
}

InkpodStatus OpenRecoveryCandidate(
    app::ApplicationHost& state,
    const app::RecoveryCandidate& candidate) noexcept {
    return OpenRecoveryCandidateImpl(state, candidate);
}

bool HandleApplicationActivation(
    app::ApplicationHost& state,
    const app::ActivationRequest& request) noexcept {
    app::WorkspaceWindow* target{};
    if (request.target == app::ActivationTargetPreference::NewWorkspace) {
        target = CreateWorkspaceWindow(state, true);
    } else {
        target = state.Workspaces().LastFocused();
        if (target == nullptr) {
            target = &state.Workspace();
        }
        (void)state.ActivateWorkspaceWindow(target->id, false);
    }
    if (target == nullptr || target->windows.window == nullptr) {
        return false;
    }
    bool success = true;
    for (const auto& path : request.paths) {
        if (!state.ActivateWorkspaceWindow(target->id, false)) {
            success = false;
            break;
        }
        const InkpodStatus status = OpenDocumentFromPath(state, path);
        if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_PENDING) {
            ShowCoreError(state, target->windows.window, L"application activation");
            success = false;
        }
    }
    target = &state.Workspace();
    if (IsIconic(target->windows.window) != FALSE) {
        ShowWindow(target->windows.window, SW_RESTORE);
    } else {
        ShowWindow(target->windows.window, SW_SHOW);
    }
    (void)SetForegroundWindow(target->windows.window);
    if (target->windows.canvas != nullptr) {
        SetFocus(target->windows.canvas);
    }
    UpdateMenuState(state);
    return success;
}

}  // namespace inkpod::windows::ui::runtime
