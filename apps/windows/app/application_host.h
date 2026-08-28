#pragma once

#include <memory>
#include <optional>
#include <cstdint>

#include "core_host.h"
#include "activation.h"
#include "application_settings.h"
#include "document_session.h"
#include "file_io_controller.h"
#include "recent_documents.h"
#include "renderer/renderer_host.h"
#include "tab_drag.h"
#include "ui/thumbnail_cache.h"
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

// UI-thread value snapshot for the frontend owner graph. Renderer telemetry is
// already a pointer-free copy, and thumbnail usage comes from the one
// application-wide cache. Core logical categories remain queried per session
// on the CoreHost owner thread.
struct ApplicationResourceUsage final {
    std::uint64_t workspace_window_count{};
    std::uint64_t document_session_count{};
    std::uint64_t document_view_count{};
    std::uint64_t editor_group_count{};
    std::uint64_t editor_canvas_count{};
    std::uint64_t registered_snapshot_sink_count{};
    std::uint64_t auxiliary_canvas_count{};
    std::uint64_t pane_instance_count{};
    windows::ui::ThumbnailCacheUsage thumbnails{};
    renderer::RendererResourceUsage renderer{};
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
    [[nodiscard]] std::optional<DocumentBinding>
    PrepareDocumentSession() noexcept;
    [[nodiscard]] std::optional<DocumentBinding> PrepareBatchResultSession(
        InkpodBatchReport* report,
        std::uint64_t result_index) noexcept;
    [[nodiscard]] bool PublishPreparedDocumentSession(
        const DocumentBinding& binding,
        EditorGroupId destination_group) noexcept;
    [[nodiscard]] bool DiscardPreparedDocumentSession(
        const DocumentBinding& binding) noexcept;
    [[nodiscard]] bool ActivateDocumentView(DocumentViewId view) noexcept;
    [[nodiscard]] bool ActivateEmptyEditorGroup(EditorGroupId group) noexcept;
    [[nodiscard]] bool RefreshEditorPresentation(
        DocumentSessionId session,
        Generation generation,
        bool refresh_core = true) noexcept;
    // UI-thread observation before a Canvas route is rebound or destroyed.
    // Copies published telemetry only; never waits for Core or Present.
    void ObserveCanvasSequencePresentation(HWND canvas) noexcept;
    // Side-effect-free UI query. A captured session must have presented its
    // current navigation epoch once, or this exact view must be presenting it.
    // Canvas stroke Begin separately requires the current bound route's frame.
    [[nodiscard]] bool SequenceEditReady(const CommandContext& context) const noexcept;
    [[nodiscard]] InkpodStatus UpdateEditorState(
        const InkpodEditorStateUpdate& update) noexcept;
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
    [[nodiscard]] ApplicationResourceUsage ResourceUsage() const noexcept;
    [[nodiscard]] windows::ui::ThumbnailCache& Thumbnails() noexcept;
    [[nodiscard]] const windows::ui::ThumbnailCache& Thumbnails() const noexcept;
    [[nodiscard]] bool ReplaceDocumentSession(
        DocumentSessionId id,
        Generation generation,
        DocumentViewId initial_view) noexcept;
    void DetachCoreSessions() noexcept;
    [[nodiscard]] bool DestroyCutSession(WorkspaceWindow& workspace) noexcept;
    [[nodiscard]] bool DestroyAllCutSessions() noexcept;

    AppLifetimeState lifetime{};
    ApplicationSettingsStore settings{};
    EffectsUiState effects{};
    BatchUiState batch{};
    windows::ui::ShortcutUiState shortcuts{};
    FrontendRoutingState routing{};
    InkpodClipboard* clipboard{};
    FileIoController file_io;
    std::unique_ptr<CoreHost> engine;
    std::unique_ptr<renderer::RendererHost> renderer;
    std::unique_ptr<ActivationService> activation;

private:
    [[nodiscard]] bool BindDocumentCanvas(
        HWND canvas, const DocumentSession& document, DocumentViewId view) noexcept;
    [[nodiscard]] bool UnbindDocumentCanvas(HWND canvas) noexcept;
    [[nodiscard]] bool RegisterWorkspacePanes(
        WorkspaceWindow& workspace) noexcept;
    void UnregisterWorkspacePanes(WorkspaceWindow& workspace) noexcept;
    void BindWorkspacePaneAliases(
        const WorkspaceWindow& workspace) noexcept;

    WorkspaceWindowRegistry workspaces_;
    DocumentRegistry documents_;
    RecentDocumentList recent_documents_;
    TabDragCoordinator tab_drag_;
    windows::ui::ThumbnailCache thumbnails_;
    std::uint32_t next_untitled_number_{1U};
};

}  // namespace inkpod::app
