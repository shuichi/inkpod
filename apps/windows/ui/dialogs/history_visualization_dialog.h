#pragma once

#include <windows.h>

namespace inkpod::app {
class ApplicationHost;
class DocumentSession;
}

namespace inkpod::windows::ui {

[[nodiscard]] bool IsHistoryVisualizationCommand(UINT command) noexcept;
void UpdateHistoryVisualizationMenu(
    app::ApplicationHost& application, HMENU main_menu) noexcept;
LRESULT IssueHistoryVisualizationCommand(
    app::ApplicationHost& application, HWND owner, UINT command) noexcept;
[[nodiscard]] bool TranslateHistoryVisualizationDialogMessage(
    const app::ApplicationHost& application, MSG& message) noexcept;
void CloseHistoryVisualizationDialog(app::DocumentSession& document) noexcept;
void CloseAllHistoryVisualizationDialogs(
    app::ApplicationHost& application) noexcept;

}  // namespace inkpod::windows::ui
