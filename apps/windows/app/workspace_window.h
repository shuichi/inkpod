#pragma once

#include <memory>

#include "frontend_state.h"
#include "ui/main_window.h"

namespace inkpod::app {

class ApplicationHost;

struct WorkspaceWindow final {
    ApplicationHost* application{};
    WorkspaceWindowId id{};
    Generation generation{};
    MainWindowHandles windows{};
    ToolUiState tools{};
    PaneUiState panes{};
    AnimationUiState animation{};
    windows::ui::CommandStateSet command_states{};
    HWND effects_progress{};
    HWND batch_progress{};
    HWND batch_palette{};
};

class WorkspaceWindowRegistry final {
public:
    [[nodiscard]] bool Initialize(
        ApplicationHost* application,
        WorkspaceWindowId id,
        Generation generation) noexcept;
    void Clear() noexcept;
    [[nodiscard]] WorkspaceWindow* Current() noexcept;
    [[nodiscard]] const WorkspaceWindow* Current() const noexcept;

private:
    std::unique_ptr<WorkspaceWindow> current_;
};

}  // namespace inkpod::app
