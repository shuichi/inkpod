#pragma once

#include <memory>

#include "core_host.h"
#include "document_session.h"
#include "renderer/renderer_host.h"
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
    void DetachCoreSessions() noexcept;

    AppLifetimeState lifetime{};
    EffectsUiState effects{};
    BatchUiState batch{};
    windows::ui::ShortcutUiState shortcuts{};
    FrontendRoutingState routing{};
    InkpodClipboard* clipboard{};
    std::unique_ptr<CoreHost> engine;
    std::unique_ptr<renderer::RendererHost> renderer;

private:
    WorkspaceWindowRegistry workspaces_;
    DocumentRegistry documents_;
};

}  // namespace inkpod::app
