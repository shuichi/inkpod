#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

#include "identity.h"

namespace inkpod::app {

struct CommandContext {
    std::optional<WorkspaceWindowId> workspace;
    std::optional<EditorGroupId> editor_group;
    std::optional<DocumentSessionId> document_session;
    std::optional<DocumentViewId> document_view;
    std::optional<PaneInstanceId> pane;
    std::optional<JobSessionId> job;
    std::optional<Generation> generation;

    constexpr auto operator<=>(const CommandContext&) const noexcept = default;
};

enum class CommandTargetScope : std::uint32_t {
    None = 0U,
    Workspace = 1U << 0U,
    EditorGroup = 1U << 1U,
    DocumentSession = 1U << 2U,
    DocumentView = 1U << 3U,
    Pane = 1U << 4U,
    Job = 1U << 5U,
};

[[nodiscard]] constexpr CommandTargetScope operator|(
    CommandTargetScope left,
    CommandTargetScope right) noexcept {
    return static_cast<CommandTargetScope>(
        static_cast<std::uint32_t>(left) | static_cast<std::uint32_t>(right));
}

[[nodiscard]] constexpr bool HasScope(
    CommandTargetScope value,
    CommandTargetScope requested) noexcept {
    return (static_cast<std::uint32_t>(value)
            & static_cast<std::uint32_t>(requested))
        != 0U;
}

inline constexpr CommandTargetScope kWorkspaceCommandScope =
    CommandTargetScope::Workspace;
inline constexpr CommandTargetScope kDocumentSessionCommandScope =
    CommandTargetScope::Workspace | CommandTargetScope::EditorGroup
    | CommandTargetScope::DocumentSession;
inline constexpr CommandTargetScope kDocumentViewCommandScope =
    kDocumentSessionCommandScope | CommandTargetScope::DocumentView;

enum class CommandResolveStatus : std::uint8_t {
    Ok,
    UnknownCommand,
    MissingScope,
    StaleGeneration,
    StaleTarget,
};

struct CommandRequest {
    std::uint32_t command{};
    CommandContext context;
};

struct EditorGroupBinding {
    EditorGroupId group{};
    CanvasId canvas{};
};

struct WorkspaceBinding {
    WorkspaceWindowId workspace{};
    EditorGroupId editor_group{};
    CanvasId canvas{};
};

// Owned and mutated only by the UI/Input thread. Captured CommandContext values
// are immutable pointer-free copies that may cross queues and outlive focus.
class CommandTargetRegistry final {
public:
    void Initialize() noexcept;
    void InvalidateAll() noexcept;

    [[nodiscard]] std::optional<WorkspaceBinding> AddWorkspace() noexcept;
    bool ActivateWorkspace(WorkspaceWindowId workspace) noexcept;
    bool RemoveWorkspace(WorkspaceWindowId workspace) noexcept;

    [[nodiscard]] DocumentSessionId ReplaceDocument() noexcept;
    [[nodiscard]] std::optional<DocumentSessionId> AddDocument() noexcept;
    bool ActivateDocument(
        DocumentSessionId document, DocumentViewId view) noexcept;
    bool RemoveDocument(DocumentSessionId document) noexcept;
    [[nodiscard]] std::optional<DocumentViewId> AddDocumentView() noexcept;
    [[nodiscard]] std::optional<DocumentViewId> AddDocumentViewTo(
        EditorGroupId group) noexcept;
    bool ActivateDocumentView(DocumentViewId view) noexcept;
    bool RemoveDocumentView(DocumentViewId view) noexcept;
    [[nodiscard]] std::optional<EditorGroupBinding> AddEditorGroup() noexcept;
    bool ActivateEditorGroup(EditorGroupId group) noexcept;
    bool MoveDocumentView(
        DocumentViewId view, EditorGroupId destination) noexcept;
    bool RemoveEditorGroup(EditorGroupId group) noexcept;

    [[nodiscard]] std::optional<PaneInstanceId> RegisterPane() noexcept;
    bool UnregisterPane(PaneInstanceId pane) noexcept;
    [[nodiscard]] std::optional<CanvasId> RegisterAuxiliaryCanvas() noexcept;
    bool UnregisterAuxiliaryCanvas(CanvasId canvas) noexcept;
    [[nodiscard]] std::optional<JobSessionId> BeginJob() noexcept;
    bool EndJob(JobSessionId job) noexcept;

    [[nodiscard]] CommandContext Capture(
        std::optional<PaneInstanceId> pane = std::nullopt,
        std::optional<JobSessionId> job = std::nullopt) const noexcept;
    [[nodiscard]] CommandResolveStatus Resolve(
        const CommandContext& context,
        CommandTargetScope required) const noexcept;
    [[nodiscard]] CommandResolveStatus Resolve(
        const CommandRequest& request,
        CommandTargetScope required) const noexcept;

    [[nodiscard]] Generation CurrentGeneration() const noexcept;
    [[nodiscard]] WorkspaceWindowId Workspace() const noexcept;
    [[nodiscard]] WorkspaceWindowId WorkspaceForGroup(
        EditorGroupId group) const noexcept;
    [[nodiscard]] WorkspaceWindowId WorkspaceForView(
        DocumentViewId view) const noexcept;
    [[nodiscard]] std::size_t WorkspaceCount() const noexcept;
    [[nodiscard]] EditorGroupId EditorGroup() const noexcept;
    [[nodiscard]] CanvasId Canvas() const noexcept;
    [[nodiscard]] CanvasId CanvasForGroup(EditorGroupId group) const noexcept;
    [[nodiscard]] EditorGroupId GroupForView(DocumentViewId view) const noexcept;
    [[nodiscard]] std::size_t EditorGroupCount() const noexcept;
    [[nodiscard]] DocumentSessionId DocumentSession() const noexcept;
    [[nodiscard]] DocumentViewId ActiveDocumentView() const noexcept;

private:
    static constexpr std::size_t kMaximumWorkspaces = 8U;
    static constexpr std::size_t kMaximumDocuments = 64U;
    static constexpr std::size_t kMaximumViews = 64U;
    static constexpr std::size_t kMaximumEditorGroups =
        kMaximumWorkspaces * 2U;
    static constexpr std::size_t kMaximumPanes = 128U;
    static constexpr std::size_t kMaximumAuxiliaryCanvases = 16U;
    static constexpr std::size_t kMaximumJobs = 16U;

    template <typename Id>
    [[nodiscard]] Id Issue() noexcept {
        const std::uint64_t value = next_id_++;
        if (next_id_ == 0U) {
            next_id_ = 1U;
        }
        return Id(value == 0U ? next_id_++ : value);
    }

    void AdvanceGeneration() noexcept;
    struct DocumentTarget final {
        DocumentSessionId document{};
        DocumentViewId active_view{};
        std::array<DocumentViewId, kMaximumViews> views{};
        std::size_t view_count{};
    };
    struct JobTarget final {
        JobSessionId job{};
        DocumentSessionId document{};
    };
    struct EditorGroupTarget final {
        WorkspaceWindowId workspace{};
        EditorGroupId group{};
        CanvasId canvas{};
        std::array<DocumentViewId, kMaximumViews> views{};
        std::size_t view_count{};
        DocumentViewId active_view{};
    };
    struct WorkspaceTarget final {
        WorkspaceWindowId workspace{};
        std::array<EditorGroupId, 2U> groups{};
        std::size_t group_count{};
        EditorGroupId active_group{};
    };

    [[nodiscard]] WorkspaceTarget* FindWorkspace(
        WorkspaceWindowId workspace) noexcept;
    [[nodiscard]] const WorkspaceTarget* FindWorkspace(
        WorkspaceWindowId workspace) const noexcept;
    [[nodiscard]] DocumentTarget* FindDocument(
        DocumentSessionId document) noexcept;
    [[nodiscard]] const DocumentTarget* FindDocument(
        DocumentSessionId document) const noexcept;
    [[nodiscard]] const DocumentTarget* FindDocumentForView(
        DocumentViewId view) const noexcept;
    [[nodiscard]] EditorGroupTarget* FindEditorGroup(
        EditorGroupId group) noexcept;
    [[nodiscard]] const EditorGroupTarget* FindEditorGroup(
        EditorGroupId group) const noexcept;
    [[nodiscard]] EditorGroupTarget* FindEditorGroupForView(
        DocumentViewId view) noexcept;
    [[nodiscard]] const EditorGroupTarget* FindEditorGroupForView(
        DocumentViewId view) const noexcept;
    [[nodiscard]] bool ContainsView(DocumentViewId view) const noexcept;
    [[nodiscard]] bool ContainsPane(PaneInstanceId pane) const noexcept;
    [[nodiscard]] bool ContainsJob(JobSessionId job) const noexcept;

    std::uint64_t next_id_{1U};
    Generation generation_{};
    std::array<WorkspaceTarget, kMaximumWorkspaces> workspaces_{};
    std::size_t workspace_count_{};
    WorkspaceWindowId active_workspace_{};
    std::array<EditorGroupTarget, kMaximumEditorGroups> editor_groups_{};
    std::size_t editor_group_count_{};
    EditorGroupId active_editor_group_{};
    DocumentSessionId active_document_{};
    std::array<DocumentTarget, kMaximumDocuments> documents_{};
    std::size_t document_count_{};
    std::array<PaneInstanceId, kMaximumPanes> panes_{};
    std::size_t pane_count_{};
    std::array<CanvasId, kMaximumAuxiliaryCanvases> auxiliary_canvases_{};
    std::size_t auxiliary_canvas_count_{};
    std::array<JobTarget, kMaximumJobs> jobs_{};
    std::size_t job_count_{};
};

enum class CommandTimerKind : std::uint8_t {
    Autosave,
    EffectProgress,
    ContinuousSpray,
    MotionPlayback,
    ShortcutSequence,
    StatusProgress,
};

struct CommandTimerToken {
    std::uint64_t value{};
    CommandTimerKind kind{CommandTimerKind::Autosave};
    CommandContext context;

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return value != 0U;
    }
};

class CommandTimerRegistry final {
public:
    [[nodiscard]] CommandTimerToken Arm(
        CommandTimerKind kind,
        const CommandContext& context) noexcept;
    bool Disarm(CommandTimerKind kind) noexcept;
    [[nodiscard]] std::optional<CommandTimerToken> Find(
        CommandTimerKind kind) const noexcept;
    [[nodiscard]] std::optional<CommandTimerToken> Resolve(
        std::uint64_t value) const noexcept;
    void Clear() noexcept;

private:
    static constexpr std::size_t kTimerCount = 6U;
    std::uint64_t next_token_{1U};
    std::array<std::optional<CommandTimerToken>, kTimerCount> timers_{};
};

enum class DragOperation : std::uint8_t {
    CanvasStroke,
    TabMove,
    TabCopy,
};

struct DragToken {
    std::uint64_t value{};
    CommandContext context;
    DragOperation operation{DragOperation::CanvasStroke};

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return value != 0U;
    }
};

struct PostedNotificationToken {
    std::uint64_t value{};
    Generation generation{};

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return value != 0U && static_cast<bool>(generation);
    }
};

class FrontendTokenSource final {
public:
    [[nodiscard]] DragToken IssueDrag(
        const CommandContext& context,
        DragOperation operation = DragOperation::CanvasStroke) noexcept;
    [[nodiscard]] PostedNotificationToken IssueNotification(
        Generation generation) noexcept;

private:
    [[nodiscard]] std::uint64_t IssueValue() noexcept;

    std::uint64_t next_value_{1U};
};

}  // namespace inkpod::app
