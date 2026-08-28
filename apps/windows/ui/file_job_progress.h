#pragma once

#include <windows.h>

#include "app/identity.h"

namespace inkpod::app {
class FileIoController;
}

namespace inkpod::windows::ui {

struct JobProgressState;

// UI-thread presentation of cached file jobs for their issuing workspace.
// Never polls Rust, performs file I/O, or follows the active document.
void RefreshFileJobProgress(
    app::FileIoController& controller,
    app::WorkspaceWindowId workspace,
    HWND status_bar,
    JobProgressState& state) noexcept;

}  // namespace inkpod::windows::ui
