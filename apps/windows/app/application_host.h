#pragma once

#include <memory>
#include <optional>
#include <cstdint>

#include "core_host.h"
#include "document_session.h"
#include "recent_documents.h"
#include "renderer/renderer_host.h"
#include "workspace_window.h"

namespace inkpod::app {

class ApplicationHost final {
public:
    struct DocumentBinding final {
        DocumentSessionId session{};
        DocumentViewId view{};
        Generation generation{};
    };

    [[nodiscard]] bool InitializeOwners() noexcept;
    void ClearOwners() noexcept;

    [[nodiscard]] WorkspaceWindow& Workspace() noexcept;
    [[nodiscard]] const WorkspaceWindow& Workspace() const noexcept;
    [[nodiscard]] DocumentSession& Document() noexcept;
    [[nodiscard]] const DocumentSession& Document() const noexcept;
    [[nodiscard]] DocumentView& ActiveView() noexcept;
    [[nodiscard]] const DocumentView& ActiveView() const noexcept;
    [[nodiscard]] DocumentRegistry& Documents() noexcept;
    [[nodiscard]] const DocumentRegistry& Documents() const noexcept;
    [[nodiscard]] std::optional<DocumentBinding> AddDocumentSession() noexcept;
    [[nodiscard]] bool ActivateDocumentView(DocumentViewId view) noexcept;
    [[nodiscard]] bool CloseDocumentView(DocumentViewId view) noexcept;
    [[nodiscard]] bool CloseDocumentSession(DocumentSessionId session) noexcept;
    [[nodiscard]] std::uint32_t IssueUntitledNumber() noexcept;
    [[nodiscard]] bool RecordRecentDocument(
        std::wstring path,
        DocumentIdentity identity) noexcept;
    [[nodiscard]] bool RemoveRecentDocument(std::size_t index) noexcept;
    [[nodiscard]] const RecentDocumentEntry* RecentDocumentAt(
        std::size_t index) const noexcept;
    [[nodiscard]] std::size_t RecentDocumentCount() const noexcept;
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
    RecentDocumentList recent_documents_;
    std::uint32_t next_untitled_number_{1U};
};

}  // namespace inkpod::app
