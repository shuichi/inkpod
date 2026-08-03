#pragma once

#include <memory>
#include <optional>
#include <cstdint>

#include "core_host.h"
#include "activation.h"
#include "document_session.h"
#include "recent_documents.h"
#include "renderer/renderer_host.h"
#include "tab_drag.h"
#include "workspace_window.h"

namespace inkpod::app {

// UI-thread value copy for memory retained by a modeless pane. Thumbnail bytes
// and other CPU cache bytes are separated so the application-wide policy can
// account for image-derived previews without exposing HWND-owned objects.
struct PaneResourceUsage final {
    WorkspaceWindowId workspace{};
    PaneInstanceId pane{};
    std::uint64_t thumbnail_bytes{};
    std::uint64_t cpu_cache_bytes{};
    std::uint64_t cached_item_count{};
};

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
    [[nodiscard]] WorkspaceWindow* FindWorkspace(
        WorkspaceWindowId id) noexcept;
    [[nodiscard]] const WorkspaceWindow* FindWorkspace(
        WorkspaceWindowId id) const noexcept;
    [[nodiscard]] WorkspaceWindow* WorkspaceForView(
        DocumentViewId view) noexcept;
    [[nodiscard]] const WorkspaceWindow* WorkspaceForView(
        DocumentViewId view) const noexcept;
    [[nodiscard]] WorkspaceWindow* WorkspaceForWindow(HWND window) noexcept;
    [[nodiscard]] const WorkspaceWindow* WorkspaceForWindow(
        HWND window) const noexcept;
    [[nodiscard]] WorkspaceWindowRegistry& Workspaces() noexcept;
    [[nodiscard]] const WorkspaceWindowRegistry& Workspaces() const noexcept;
    [[nodiscard]] WorkspaceWindow* AddWorkspaceWindow() noexcept;
    [[nodiscard]] bool ActivateWorkspaceWindow(
        WorkspaceWindowId id, bool record_focus = false) noexcept;
    [[nodiscard]] bool RemoveWorkspaceWindow(WorkspaceWindowId id) noexcept;
    [[nodiscard]] bool MoveDocumentViewToWorkspace(
        DocumentViewId view, WorkspaceWindowId destination) noexcept;
    [[nodiscard]] bool MoveDocumentView(
        DocumentViewId view,
        WorkspaceWindowId destination_workspace,
        EditorGroupId destination_group,
        std::size_t insertion_index) noexcept;
    [[nodiscard]] TabDragCoordinator& TabDrag() noexcept;
    [[nodiscard]] const TabDragCoordinator& TabDrag() const noexcept;
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
    [[nodiscard]] bool GetPaneResourceUsage(
        PaneInstanceId pane,
        PaneResourceUsage& usage) const noexcept;
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
    std::unique_ptr<ActivationService> activation;

private:
    [[nodiscard]] bool RegisterWorkspacePanes(
        WorkspaceWindow& workspace) noexcept;
    void UnregisterWorkspacePanes(WorkspaceWindow& workspace) noexcept;
    void BindWorkspacePaneAliases(
        const WorkspaceWindow& workspace) noexcept;

    WorkspaceWindowRegistry workspaces_;
    DocumentRegistry documents_;
    RecentDocumentList recent_documents_;
    TabDragCoordinator tab_drag_;
    std::uint32_t next_untitled_number_{1U};
};

}  // namespace inkpod::app
