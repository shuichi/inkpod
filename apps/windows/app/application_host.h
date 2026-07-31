#pragma once

#include <memory>

#include "core_engine.h"
#include "document_session.h"
#include "workspace_window.h"

namespace inkpod::app {

class ApplicationHost final {
public:
    [[nodiscard]] bool InitializeOwners() noexcept;
    void ClearOwners() noexcept;

    [[nodiscard]] WorkspaceWindow& Workspace() noexcept;
    [[nodiscard]] const WorkspaceWindow& Workspace() const noexcept;
    [[nodiscard]] DocumentSession& Document() noexcept;
    [[nodiscard]] const DocumentSession& Document() const noexcept;
    [[nodiscard]] DocumentView& ActiveView() noexcept;
    [[nodiscard]] const DocumentView& ActiveView() const noexcept;
    [[nodiscard]] bool ReplaceDocumentSession(
        DocumentSessionId id,
        Generation generation,
        DocumentViewId initial_view) noexcept;

    AppLifetimeState lifetime{};
    EffectsUiState effects{};
    BatchUiState batch{};
    windows::ui::ShortcutUiState shortcuts{};
    FrontendRoutingState routing{};
    InkpodClipboard* clipboard{};
    std::unique_ptr<CoreEngine> engine;

private:
    WorkspaceWindowRegistry workspaces_;
    DocumentRegistry documents_;
};

}  // namespace inkpod::app
