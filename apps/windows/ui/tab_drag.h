#pragma once

#include <windows.h>

#include "app/identity.h"

namespace inkpod::app {
class ApplicationHost;
}

namespace inkpod::windows::ui {

[[nodiscard]] bool AttachDocumentTabDrag(
    HWND tabs, app::EditorGroupId group) noexcept;
void CancelDocumentTabDrag(
    app::ApplicationHost& state, bool restore_active_view = true) noexcept;

}  // namespace inkpod::windows::ui
