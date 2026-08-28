#include "command_context.h"

#include <algorithm>

namespace inkpod::app {
namespace {

template <typename Id, std::size_t Size>
bool Contains(
    const std::array<Id, Size>& values,
    std::size_t count,
    Id requested) noexcept {
    return std::find(values.cbegin(), values.cbegin() + count, requested)
        != values.cbegin() + count;
}

template <typename Id, std::size_t Size>
bool Remove(
    std::array<Id, Size>& values,
    std::size_t& count,
    Id requested) noexcept {
    const auto end = values.begin() + count;
    const auto found = std::find(values.begin(), end, requested);
    if (found == end) {
        return false;
    }
    std::move(found + 1, end, found);
    --count;
    values[count] = Id{};
    return true;
}

}  // namespace

void CommandTargetRegistry::Initialize() noexcept {
    if (active_workspace_) {
        return;
    }
    generation_ = Generation(1U);
    WorkspaceTarget& workspace = workspaces_[0];
    workspace.workspace = Issue<WorkspaceWindowId>();
    editor_groups_[0].group = Issue<EditorGroupId>();
    editor_groups_[0].canvas = Issue<CanvasId>();
    editor_groups_[0].workspace = workspace.workspace;
    workspace.groups[0] = editor_groups_[0].group;
    workspace.group_count = 1U;
    workspace.active_group = editor_groups_[0].group;
    workspace_count_ = 1U;
    editor_group_count_ = 1U;
    active_workspace_ = workspace.workspace;
    active_editor_group_ = editor_groups_[0].group;
}

void CommandTargetRegistry::InvalidateAll() noexcept {
    AdvanceGeneration();
    active_document_ = {};
    documents_.fill({});
    document_count_ = 0U;
    jobs_.fill({});
    job_count_ = 0U;
    panes_.fill({});
    pane_count_ = 0U;
    auxiliary_canvases_.fill({});
    auxiliary_canvas_count_ = 0U;
    workspaces_.fill({});
    workspace_count_ = 0U;
    active_workspace_ = {};
    editor_groups_.fill({});
    editor_group_count_ = 0U;
    active_editor_group_ = {};
}

std::optional<WorkspaceBinding> CommandTargetRegistry::AddWorkspace() noexcept {
    Initialize();
    if (workspace_count_ >= workspaces_.size()
        || editor_group_count_ >= editor_groups_.size()) {
        return std::nullopt;
    }
    WorkspaceTarget& workspace = workspaces_[workspace_count_++];
    workspace = {};
    workspace.workspace = Issue<WorkspaceWindowId>();
    EditorGroupTarget& group = editor_groups_[editor_group_count_++];
    group = {};
    group.workspace = workspace.workspace;
    group.group = Issue<EditorGroupId>();
    group.canvas = Issue<CanvasId>();
    workspace.groups[0] = group.group;
    workspace.group_count = 1U;
    workspace.active_group = group.group;
    active_workspace_ = workspace.workspace;
    active_editor_group_ = group.group;
    active_document_ = {};
    return WorkspaceBinding{workspace.workspace, group.group, group.canvas};
}

bool CommandTargetRegistry::ActivateWorkspace(
    WorkspaceWindowId workspace_id) noexcept {
    WorkspaceTarget* workspace = FindWorkspace(workspace_id);
    if (workspace == nullptr) {
        return false;
    }
    active_workspace_ = workspace_id;
    active_editor_group_ = workspace->active_group;
    const EditorGroupTarget* group = FindEditorGroup(active_editor_group_);
    const DocumentTarget* document = group != nullptr && group->active_view
        ? FindDocumentForView(group->active_view)
        : nullptr;
    active_document_ = document == nullptr
        ? DocumentSessionId{}
        : document->document;
    return true;
}

bool CommandTargetRegistry::RemoveWorkspace(
    WorkspaceWindowId workspace_id) noexcept {
    WorkspaceTarget* workspace = FindWorkspace(workspace_id);
    if (workspace == nullptr) {
        return false;
    }
    for (std::size_t index = 0U; index < workspace->group_count; ++index) {
        const EditorGroupTarget* group = FindEditorGroup(workspace->groups[index]);
        if (group == nullptr || group->view_count != 0U) {
            return false;
        }
    }
    for (std::size_t index = workspace->group_count; index > 0U; --index) {
        const EditorGroupId group_id = workspace->groups[index - 1U];
        const auto end = editor_groups_.begin()
            + static_cast<std::ptrdiff_t>(editor_group_count_);
        const auto found = std::find_if(
            editor_groups_.begin(), end,
            [group_id](const EditorGroupTarget& candidate) {
                return candidate.group == group_id;
            });
        if (found != end) {
            std::move(found + 1, end, found);
            --editor_group_count_;
            editor_groups_[editor_group_count_] = {};
        }
    }
    const auto workspace_end = workspaces_.begin()
        + static_cast<std::ptrdiff_t>(workspace_count_);
    const auto found = std::find_if(
        workspaces_.begin(), workspace_end,
        [workspace_id](const WorkspaceTarget& candidate) {
            return candidate.workspace == workspace_id;
        });
    std::move(found + 1, workspace_end, found);
    --workspace_count_;
    workspaces_[workspace_count_] = {};
    if (active_workspace_ == workspace_id) {
        active_workspace_ = workspace_count_ == 0U
            ? WorkspaceWindowId{}
            : workspaces_[0].workspace;
        return !active_workspace_ || ActivateWorkspace(active_workspace_);
    }
    return true;
}

DocumentSessionId CommandTargetRegistry::ReplaceDocument() noexcept {
    Initialize();
    AdvanceGeneration();
    documents_.fill({});
    document_count_ = 1U;
    DocumentTarget& target = documents_[0];
    target.document = Issue<DocumentSessionId>();
    target.view_count = 1U;
    target.views[0] = Issue<DocumentViewId>();
    target.active_view = target.views[0];
    active_document_ = target.document;
    for (std::size_t index = 0U; index < editor_group_count_; ++index) {
        editor_groups_[index].views.fill({});
        editor_groups_[index].view_count = 0U;
        editor_groups_[index].active_view = {};
    }
    EditorGroupTarget* group = FindEditorGroup(active_editor_group_);
    if (group != nullptr) {
        group->views[group->view_count++] = target.active_view;
        group->active_view = target.active_view;
    }
    jobs_.fill({});
    job_count_ = 0U;
    return active_document_;
}

std::optional<DocumentSessionId> CommandTargetRegistry::AddDocument() noexcept {
    Initialize();
    if (document_count_ >= documents_.size()) {
        return std::nullopt;
    }
    const DocumentSessionId previous_document = active_document_;
    DocumentTarget& target = documents_[document_count_++];
    target = {};
    target.document = Issue<DocumentSessionId>();
    target.view_count = 1U;
    target.views[0] = Issue<DocumentViewId>();
    target.active_view = target.views[0];
    active_document_ = target.document;
    EditorGroupTarget* group = FindEditorGroup(active_editor_group_);
    if (group == nullptr || group->view_count >= group->views.size()) {
        --document_count_;
        documents_[document_count_] = {};
        active_document_ = previous_document;
        return std::nullopt;
    }
    group->views[group->view_count++] = target.active_view;
    group->active_view = target.active_view;
    return target.document;
}

bool CommandTargetRegistry::ActivateDocument(
    DocumentSessionId document,
    DocumentViewId view) noexcept {
    DocumentTarget* target = FindDocument(document);
    if (target == nullptr || !Contains(target->views, target->view_count, view)) {
        return false;
    }
    EditorGroupTarget* group = FindEditorGroupForView(view);
    if (group == nullptr) {
        return false;
    }
    target->active_view = view;
    active_document_ = document;
    group->active_view = view;
    active_editor_group_ = group->group;
    active_workspace_ = group->workspace;
    WorkspaceTarget* workspace = FindWorkspace(group->workspace);
    if (workspace != nullptr) {
        workspace->active_group = group->group;
    }
    return true;
}

bool CommandTargetRegistry::RemoveDocument(
    DocumentSessionId document) noexcept {
    const auto end = documents_.begin() + document_count_;
    const auto found = std::find_if(
        documents_.begin(), end, [document](const DocumentTarget& target) {
            return target.document == document;
        });
    if (found == end) {
        return false;
    }
    for (std::size_t index = 0U; index < found->view_count; ++index) {
        EditorGroupTarget* group = FindEditorGroupForView(found->views[index]);
        if (group != nullptr) {
            (void)Remove(group->views, group->view_count, found->views[index]);
            if (group->active_view == found->views[index]) {
                group->active_view = group->view_count == 0U
                    ? DocumentViewId{}
                    : group->views[0];
            }
        }
    }
    std::move(found + 1, end, found);
    --document_count_;
    documents_[document_count_] = {};
    for (std::size_t index = job_count_; index > 0U; --index) {
        if (jobs_[index - 1U].document == document) {
            std::move(
                jobs_.begin() + static_cast<std::ptrdiff_t>(index),
                jobs_.begin() + static_cast<std::ptrdiff_t>(job_count_),
                jobs_.begin() + static_cast<std::ptrdiff_t>(index - 1U));
            --job_count_;
            jobs_[job_count_] = {};
        }
    }
    if (active_document_ == document) {
        const EditorGroupTarget* active_group =
            FindEditorGroup(active_editor_group_);
        const DocumentTarget* replacement = active_group == nullptr
            ? nullptr
            : FindDocumentForView(active_group->active_view);
        active_document_ = replacement == nullptr
            ? DocumentSessionId{}
            : replacement->document;
    }
    return true;
}

std::optional<DocumentViewId> CommandTargetRegistry::AddDocumentView() noexcept {
    return AddDocumentViewTo(active_editor_group_);
}

std::optional<DocumentViewId> CommandTargetRegistry::AddDocumentViewTo(
    EditorGroupId group_id) noexcept {
    DocumentTarget* target = FindDocument(active_document_);
    EditorGroupTarget* group = FindEditorGroup(group_id);
    if (target == nullptr || group == nullptr
        || target->view_count >= target->views.size()
        || group->view_count >= group->views.size()) {
        return std::nullopt;
    }
    const DocumentViewId view = Issue<DocumentViewId>();
    target->views[target->view_count++] = view;
    target->active_view = view;
    group->views[group->view_count++] = view;
    group->active_view = view;
    active_editor_group_ = group_id;
    active_workspace_ = group->workspace;
    WorkspaceTarget* workspace = FindWorkspace(group->workspace);
    if (workspace != nullptr) {
        workspace->active_group = group_id;
    }
    return view;
}

bool CommandTargetRegistry::ActivateDocumentView(DocumentViewId view) noexcept {
    const DocumentTarget* target = FindDocumentForView(view);
    if (target == nullptr) {
        return false;
    }
    return ActivateDocument(target->document, view);
}

bool CommandTargetRegistry::RemoveDocumentView(DocumentViewId view) noexcept {
    const DocumentTarget* owner = FindDocumentForView(view);
    if (owner == nullptr) {
        return false;
    }
    DocumentTarget* target = FindDocument(owner->document);
    EditorGroupTarget* group = FindEditorGroupForView(view);
    if (target == nullptr || group == nullptr
        || !Remove(target->views, target->view_count, view)
        || !Remove(group->views, group->view_count, view)) {
        return false;
    }
    if (target->active_view == view) {
        target->active_view = target->view_count == 0U
            ? DocumentViewId{}
            : target->views[0];
    }
    if (group->active_view == view) {
        group->active_view = group->view_count == 0U
            ? DocumentViewId{}
            : group->views[0];
    }
    return true;
}

std::optional<EditorGroupBinding> CommandTargetRegistry::AddEditorGroup() noexcept {
    Initialize();
    WorkspaceTarget* workspace = FindWorkspace(active_workspace_);
    if (workspace == nullptr || workspace->group_count >= workspace->groups.size()
        || editor_group_count_ >= editor_groups_.size()) {
        return std::nullopt;
    }
    EditorGroupTarget& group = editor_groups_[editor_group_count_++];
    group = {};
    group.workspace = active_workspace_;
    group.group = Issue<EditorGroupId>();
    group.canvas = Issue<CanvasId>();
    workspace->groups[workspace->group_count++] = group.group;
    return EditorGroupBinding{group.group, group.canvas};
}

bool CommandTargetRegistry::ActivateEditorGroup(EditorGroupId group_id) noexcept {
    EditorGroupTarget* group = FindEditorGroup(group_id);
    if (group == nullptr) {
        return false;
    }
    const DocumentTarget* document = group->active_view
        ? FindDocumentForView(group->active_view)
        : nullptr;
    if (group->active_view && document == nullptr) {
        return false;
    }
    active_workspace_ = group->workspace;
    active_editor_group_ = group_id;
    WorkspaceTarget* workspace = FindWorkspace(group->workspace);
    if (workspace != nullptr) {
        workspace->active_group = group_id;
    }
    if (group->active_view) {
        active_document_ = document->document;
        DocumentTarget* mutable_document = FindDocument(active_document_);
        if (mutable_document != nullptr) {
            mutable_document->active_view = group->active_view;
        }
    } else {
        active_document_ = {};
    }
    return true;
}

void CommandTargetRegistry::ClearActiveDocument() noexcept {
    active_document_ = {};
    if (auto* group = FindEditorGroup(active_editor_group_); group != nullptr) {
        group->active_view = {};
    }
}

bool CommandTargetRegistry::MoveDocumentView(
    DocumentViewId view, EditorGroupId destination) noexcept {
    EditorGroupTarget* source = FindEditorGroupForView(view);
    EditorGroupTarget* target = FindEditorGroup(destination);
    if (source == nullptr || target == nullptr || source == target
        || target->view_count >= target->views.size()) {
        return false;
    }
    if (!Remove(source->views, source->view_count, view)) {
        return false;
    }
    if (source->active_view == view) {
        source->active_view = source->view_count == 0U
            ? DocumentViewId{}
            : source->views[0];
    }
    target->views[target->view_count++] = view;
    target->active_view = view;
    active_workspace_ = target->workspace;
    active_editor_group_ = destination;
    WorkspaceTarget* workspace = FindWorkspace(target->workspace);
    if (workspace != nullptr) {
        workspace->active_group = destination;
    }
    const DocumentTarget* document = FindDocumentForView(view);
    if (document != nullptr) {
        active_document_ = document->document;
    }
    return true;
}

bool CommandTargetRegistry::RemoveEditorGroup(EditorGroupId group_id) noexcept {
    WorkspaceTarget* workspace = FindWorkspace(active_workspace_);
    if (workspace == nullptr || workspace->group_count != 2U
        || !Contains(workspace->groups, workspace->group_count, group_id)) {
        return false;
    }
    EditorGroupTarget* source = FindEditorGroup(group_id);
    const EditorGroupId target_id = workspace->groups[0] == group_id
        ? workspace->groups[1]
        : workspace->groups[0];
    EditorGroupTarget* target = FindEditorGroup(target_id);
    if (source == nullptr || target == nullptr
        || source->view_count + target->view_count > target->views.size()) {
        return false;
    }
    for (std::size_t index = 0U; index < source->view_count; ++index) {
        target->views[target->view_count++] = source->views[index];
        target->active_view = source->views[index];
    }
    const auto end = editor_groups_.begin()
        + static_cast<std::ptrdiff_t>(editor_group_count_);
    const auto found = std::find_if(
        editor_groups_.begin(), end,
        [group_id](const EditorGroupTarget& candidate) {
            return candidate.group == group_id;
        });
    std::move(found + 1, end, found);
    --editor_group_count_;
    editor_groups_[editor_group_count_] = {};
    workspace->groups = {target_id, EditorGroupId{}};
    workspace->group_count = 1U;
    workspace->active_group = target_id;
    active_editor_group_ = target_id;
    return ActivateEditorGroup(active_editor_group_);
}

std::optional<PaneInstanceId> CommandTargetRegistry::RegisterPane() noexcept {
    Initialize();
    if (pane_count_ >= panes_.size()) {
        return std::nullopt;
    }
    const PaneInstanceId pane = Issue<PaneInstanceId>();
    panes_[pane_count_++] = pane;
    return pane;
}

bool CommandTargetRegistry::UnregisterPane(PaneInstanceId pane) noexcept {
    return Remove(panes_, pane_count_, pane);
}

std::optional<CanvasId> CommandTargetRegistry::RegisterAuxiliaryCanvas() noexcept {
    Initialize();
    if (auxiliary_canvas_count_ >= auxiliary_canvases_.size()) {
        return std::nullopt;
    }
    const CanvasId canvas = Issue<CanvasId>();
    auxiliary_canvases_[auxiliary_canvas_count_++] = canvas;
    return canvas;
}

bool CommandTargetRegistry::UnregisterAuxiliaryCanvas(CanvasId canvas) noexcept {
    return Remove(auxiliary_canvases_, auxiliary_canvas_count_, canvas);
}

std::optional<JobSessionId> CommandTargetRegistry::BeginJob() noexcept {
    return BeginJob(Capture());
}

std::optional<JobSessionId> CommandTargetRegistry::BeginJob(
    const CommandContext& target) noexcept {
    if (job_count_ >= jobs_.size()
        || Resolve(target, kDocumentSessionCommandScope)
            != CommandResolveStatus::Ok) {
        return std::nullopt;
    }
    const JobSessionId job = Issue<JobSessionId>();
    jobs_[job_count_++] = JobTarget{job, target.document_session.value()};
    return job;
}

bool CommandTargetRegistry::EndJob(JobSessionId job) noexcept {
    const auto end = jobs_.begin() + job_count_;
    const auto found = std::find_if(
        jobs_.begin(), end, [job](const JobTarget& target) {
            return target.job == job;
        });
    if (found == end) {
        return false;
    }
    std::move(found + 1, end, found);
    --job_count_;
    jobs_[job_count_] = {};
    return true;
}

CommandContext CommandTargetRegistry::Capture(
    std::optional<PaneInstanceId> pane,
    std::optional<JobSessionId> job) const noexcept {
    CommandContext context{};
    if (active_workspace_) {
        context.workspace = active_workspace_;
    }
    const EditorGroupTarget* group = FindEditorGroup(active_editor_group_);
    if (group != nullptr) {
        context.editor_group = group->group;
    }
    const DocumentTarget* document = group != nullptr && group->active_view
        ? FindDocumentForView(group->active_view)
        : FindDocument(active_document_);
    if (document != nullptr) {
        context.document_session = document->document;
        const DocumentViewId view = group != nullptr
            ? group->active_view
            : document->active_view;
        if (view) {
            context.document_view = view;
        }
    }
    if (pane.has_value()) {
        context.pane = pane;
    }
    if (job.has_value()) {
        context.job = job;
    }
    if (generation_) {
        context.generation = generation_;
    }
    return context;
}

CommandResolveStatus CommandTargetRegistry::Resolve(
    const CommandContext& context,
    CommandTargetScope required) const noexcept {
    if (!context.generation.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    if (context.generation.value() != generation_) {
        return CommandResolveStatus::StaleGeneration;
    }
    if (HasScope(required, CommandTargetScope::Workspace)
        && !context.workspace.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    const WorkspaceTarget* workspace = context.workspace.has_value()
        ? FindWorkspace(context.workspace.value())
        : nullptr;
    if (context.workspace.has_value() && workspace == nullptr) {
        return CommandResolveStatus::StaleTarget;
    }
    const EditorGroupTarget* group = context.editor_group.has_value()
        ? FindEditorGroup(context.editor_group.value())
        : nullptr;
    if ((HasScope(required, CommandTargetScope::EditorGroup)
         && !context.editor_group.has_value())
        || (context.editor_group.has_value()
            && (group == nullptr
                || (workspace != nullptr
                    && group->workspace != workspace->workspace)))) {
        return context.workspace.has_value() && context.editor_group.has_value()
            ? CommandResolveStatus::StaleTarget
            : CommandResolveStatus::MissingScope;
    }
    if (HasScope(required, CommandTargetScope::DocumentSession)
        && !context.document_session.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    const DocumentTarget* document = context.document_session.has_value()
        ? FindDocument(context.document_session.value())
        : nullptr;
    if (context.document_session.has_value() && document == nullptr) {
        return CommandResolveStatus::StaleTarget;
    }
    if (HasScope(required, CommandTargetScope::DocumentView)
        && !context.document_view.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    if (context.document_view.has_value()
        && (document == nullptr
            || !Contains(
                document->views,
                document->view_count,
                context.document_view.value())
            || (group != nullptr
                && !Contains(
                    group->views,
                    group->view_count,
                    context.document_view.value())))) {
        return CommandResolveStatus::StaleTarget;
    }
    if (HasScope(required, CommandTargetScope::Pane) && !context.pane.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    if (context.pane.has_value() && !ContainsPane(context.pane.value())) {
        return CommandResolveStatus::StaleTarget;
    }
    if (HasScope(required, CommandTargetScope::Job) && !context.job.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    if (context.job.has_value()) {
        const auto found = std::find_if(
            jobs_.cbegin(), jobs_.cbegin() + job_count_,
            [&context](const JobTarget& target) {
                return target.job == context.job.value();
            });
        if (found == jobs_.cbegin() + job_count_
            || (context.document_session.has_value()
                && found->document != context.document_session.value())) {
            return CommandResolveStatus::StaleTarget;
        }
    }
    return CommandResolveStatus::Ok;
}

CommandResolveStatus CommandTargetRegistry::Resolve(
    const CommandRequest& request,
    CommandTargetScope required) const noexcept {
    return request.command == 0U
        ? CommandResolveStatus::UnknownCommand
        : Resolve(request.context, required);
}

Generation CommandTargetRegistry::CurrentGeneration() const noexcept {
    return generation_;
}

WorkspaceWindowId CommandTargetRegistry::Workspace() const noexcept {
    return active_workspace_;
}

WorkspaceWindowId CommandTargetRegistry::WorkspaceForGroup(
    EditorGroupId group) const noexcept {
    const EditorGroupTarget* target = FindEditorGroup(group);
    return target == nullptr ? WorkspaceWindowId{} : target->workspace;
}

WorkspaceWindowId CommandTargetRegistry::WorkspaceForView(
    DocumentViewId view) const noexcept {
    const EditorGroupTarget* target = FindEditorGroupForView(view);
    return target == nullptr ? WorkspaceWindowId{} : target->workspace;
}

std::size_t CommandTargetRegistry::WorkspaceCount() const noexcept {
    return workspace_count_;
}

EditorGroupId CommandTargetRegistry::EditorGroup() const noexcept {
    return active_editor_group_;
}

CanvasId CommandTargetRegistry::Canvas() const noexcept {
    return CanvasForGroup(active_editor_group_);
}

CanvasId CommandTargetRegistry::CanvasForGroup(EditorGroupId group) const noexcept {
    const EditorGroupTarget* target = FindEditorGroup(group);
    return target == nullptr ? CanvasId{} : target->canvas;
}

EditorGroupId CommandTargetRegistry::GroupForView(DocumentViewId view) const noexcept {
    const EditorGroupTarget* target = FindEditorGroupForView(view);
    return target == nullptr ? EditorGroupId{} : target->group;
}

std::size_t CommandTargetRegistry::EditorGroupCount() const noexcept {
    const WorkspaceTarget* workspace = FindWorkspace(active_workspace_);
    return workspace == nullptr ? 0U : workspace->group_count;
}

DocumentSessionId CommandTargetRegistry::DocumentSession() const noexcept {
    return active_document_;
}

DocumentViewId CommandTargetRegistry::ActiveDocumentView() const noexcept {
    const EditorGroupTarget* group = FindEditorGroup(active_editor_group_);
    return group == nullptr ? DocumentViewId{} : group->active_view;
}

void CommandTargetRegistry::AdvanceGeneration() noexcept {
    std::uint64_t next = generation_.Value() + 1U;
    if (next == 0U) {
        next = 1U;
    }
    generation_ = Generation(next);
}

bool CommandTargetRegistry::ContainsView(DocumentViewId view) const noexcept {
    return FindDocumentForView(view) != nullptr;
}

bool CommandTargetRegistry::ContainsPane(PaneInstanceId pane) const noexcept {
    return Contains(panes_, pane_count_, pane);
}

bool CommandTargetRegistry::ContainsJob(JobSessionId job) const noexcept {
    return std::find_if(
               jobs_.cbegin(),
               jobs_.cbegin() + job_count_,
               [job](const JobTarget& target) { return target.job == job; })
        != jobs_.cbegin() + job_count_;
}

CommandTargetRegistry::WorkspaceTarget* CommandTargetRegistry::FindWorkspace(
    WorkspaceWindowId workspace) noexcept {
    return const_cast<WorkspaceTarget*>(
        static_cast<const CommandTargetRegistry&>(*this).FindWorkspace(workspace));
}

const CommandTargetRegistry::WorkspaceTarget*
CommandTargetRegistry::FindWorkspace(WorkspaceWindowId workspace) const noexcept {
    const auto found = std::find_if(
        workspaces_.cbegin(),
        workspaces_.cbegin() + static_cast<std::ptrdiff_t>(workspace_count_),
        [workspace](const WorkspaceTarget& target) {
            return target.workspace == workspace;
        });
    return found == workspaces_.cbegin()
            + static_cast<std::ptrdiff_t>(workspace_count_)
        ? nullptr
        : &*found;
}

CommandTargetRegistry::DocumentTarget* CommandTargetRegistry::FindDocument(
    DocumentSessionId document) noexcept {
    return const_cast<DocumentTarget*>(
        static_cast<const CommandTargetRegistry&>(*this).FindDocument(document));
}

const CommandTargetRegistry::DocumentTarget* CommandTargetRegistry::FindDocument(
    DocumentSessionId document) const noexcept {
    const auto found = std::find_if(
        documents_.cbegin(),
        documents_.cbegin() + document_count_,
        [document](const DocumentTarget& target) {
            return target.document == document;
        });
    return found == documents_.cbegin() + document_count_ ? nullptr : &*found;
}

const CommandTargetRegistry::DocumentTarget*
CommandTargetRegistry::FindDocumentForView(DocumentViewId view) const noexcept {
    const auto found = std::find_if(
        documents_.cbegin(),
        documents_.cbegin() + document_count_,
        [view](const DocumentTarget& target) {
            return Contains(target.views, target.view_count, view);
        });
    return found == documents_.cbegin() + document_count_ ? nullptr : &*found;
}

CommandTargetRegistry::EditorGroupTarget*
CommandTargetRegistry::FindEditorGroup(EditorGroupId group) noexcept {
    return const_cast<EditorGroupTarget*>(
        static_cast<const CommandTargetRegistry&>(*this).FindEditorGroup(group));
}

const CommandTargetRegistry::EditorGroupTarget*
CommandTargetRegistry::FindEditorGroup(EditorGroupId group) const noexcept {
    const auto found = std::find_if(
        editor_groups_.cbegin(),
        editor_groups_.cbegin() + editor_group_count_,
        [group](const EditorGroupTarget& target) {
            return target.group == group;
        });
    return found == editor_groups_.cbegin() + editor_group_count_
        ? nullptr
        : &*found;
}

CommandTargetRegistry::EditorGroupTarget*
CommandTargetRegistry::FindEditorGroupForView(DocumentViewId view) noexcept {
    return const_cast<EditorGroupTarget*>(
        static_cast<const CommandTargetRegistry&>(*this)
            .FindEditorGroupForView(view));
}

const CommandTargetRegistry::EditorGroupTarget*
CommandTargetRegistry::FindEditorGroupForView(DocumentViewId view) const noexcept {
    const auto found = std::find_if(
        editor_groups_.cbegin(),
        editor_groups_.cbegin() + editor_group_count_,
        [view](const EditorGroupTarget& target) {
            return Contains(target.views, target.view_count, view);
        });
    return found == editor_groups_.cbegin() + editor_group_count_
        ? nullptr
        : &*found;
}

CommandTimerToken CommandTimerRegistry::Arm(
    CommandTimerKind kind,
    const CommandContext& context) noexcept {
    std::uint64_t value = next_token_++;
    if (next_token_ == 0U) {
        next_token_ = 1U;
    }
    if (value == 0U) {
        value = next_token_++;
    }
    CommandTimerToken token{value, kind, context};
    timers_[static_cast<std::size_t>(kind)] = token;
    return token;
}

bool CommandTimerRegistry::Disarm(CommandTimerKind kind) noexcept {
    auto& timer = timers_[static_cast<std::size_t>(kind)];
    const bool armed = timer.has_value();
    timer.reset();
    return armed;
}

std::optional<CommandTimerToken> CommandTimerRegistry::Find(
    CommandTimerKind kind) const noexcept {
    return timers_[static_cast<std::size_t>(kind)];
}

std::optional<CommandTimerToken> CommandTimerRegistry::Resolve(
    std::uint64_t value) const noexcept {
    const auto found = std::find_if(
        timers_.cbegin(),
        timers_.cend(),
        [value](const std::optional<CommandTimerToken>& timer) {
            return timer.has_value() && timer->value == value;
        });
    return found == timers_.cend() ? std::nullopt : *found;
}

void CommandTimerRegistry::Clear() noexcept {
    timers_.fill(std::nullopt);
}

DragToken FrontendTokenSource::IssueDrag(
    const CommandContext& context,
    DragOperation operation) noexcept {
    return DragToken{IssueValue(), context, operation};
}

PostedNotificationToken FrontendTokenSource::IssueNotification(
    Generation generation) noexcept {
    return PostedNotificationToken{IssueValue(), generation};
}

std::uint64_t FrontendTokenSource::IssueValue() noexcept {
    std::uint64_t value = next_value_++;
    if (next_value_ == 0U) {
        next_value_ = 1U;
    }
    if (value == 0U) {
        value = next_value_++;
    }
    return value;
}

}  // namespace inkpod::app
