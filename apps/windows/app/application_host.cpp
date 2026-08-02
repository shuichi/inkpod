#include "application_host.h"

#include <array>
#include <cassert>
#include <utility>

#include "application_owner_graph.h"
#include "renderer/canvas.h"

namespace inkpod::app {

bool ApplicationHost::InitializeOwners() noexcept {
    routing.targets.Initialize();
    const Generation generation = routing.targets.CurrentGeneration();
    if (!InitializeOwnerGraph(
        workspaces_,
        documents_,
        this,
        routing.targets.Workspace(),
        routing.targets.EditorGroup(),
        routing.targets.Canvas(),
        generation)) {
        return false;
    }
    if (!RegisterWorkspacePanes(Workspace())) {
        ClearOwnerGraph(documents_, workspaces_);
        return false;
    }
    return true;
}

void ApplicationHost::ClearOwners() noexcept {
    ClearOwnerGraph(documents_, workspaces_);
}

WorkspaceWindow& ApplicationHost::Workspace() noexcept {
    assert(workspaces_.Current() != nullptr);
    return *workspaces_.Current();
}

const WorkspaceWindow& ApplicationHost::Workspace() const noexcept {
    assert(workspaces_.Current() != nullptr);
    return *workspaces_.Current();
}

WorkspaceWindow* ApplicationHost::FindWorkspace(
    WorkspaceWindowId id) noexcept {
    return workspaces_.Find(id);
}

const WorkspaceWindow* ApplicationHost::FindWorkspace(
    WorkspaceWindowId id) const noexcept {
    return workspaces_.Find(id);
}

WorkspaceWindow* ApplicationHost::WorkspaceForView(
    DocumentViewId view) noexcept {
    const WorkspaceWindowId workspace = routing.targets.WorkspaceForView(view);
    return workspaces_.Find(workspace);
}

const WorkspaceWindow* ApplicationHost::WorkspaceForView(
    DocumentViewId view) const noexcept {
    const WorkspaceWindowId workspace = routing.targets.WorkspaceForView(view);
    return workspaces_.Find(workspace);
}

WorkspaceWindow* ApplicationHost::WorkspaceForWindow(HWND window) noexcept {
    return const_cast<WorkspaceWindow*>(
        static_cast<const ApplicationHost&>(*this).WorkspaceForWindow(window));
}

const WorkspaceWindow* ApplicationHost::WorkspaceForWindow(
    HWND window) const noexcept {
    if (window == nullptr) {
        return nullptr;
    }
    const HWND root_owner = GetAncestor(window, GA_ROOTOWNER);
    for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
        const WorkspaceWindow* workspace = workspaces_.At(index);
        if (workspace == nullptr || workspace->windows.window == nullptr) {
            continue;
        }
        const std::array owned{
            workspace->windows.window,
            workspace->tools.palette,
            workspace->panes.layer_palette,
            workspace->batch_palette,
            workspace->locator_palette,
            workspace->sequence_palette,
            workspace->light_table_palette,
            workspace->subpalette_palette};
        for (const HWND candidate : owned) {
            if (candidate != nullptr
                && (window == candidate || root_owner == candidate
                    || IsChild(candidate, window) != FALSE)) {
                return workspace;
            }
        }
    }
    return nullptr;
}

WorkspaceWindowRegistry& ApplicationHost::Workspaces() noexcept {
    return workspaces_;
}

const WorkspaceWindowRegistry& ApplicationHost::Workspaces() const noexcept {
    return workspaces_;
}

bool ApplicationHost::RegisterWorkspacePanes(
    WorkspaceWindow& workspace) noexcept {
    using Policy = PaneTargetPolicy;
    constexpr std::array policies{
        Policy::Application,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView,
        Policy::FollowActiveView};
    std::array<PaneInstanceId, policies.size()> panes{};
    std::size_t registered{};
    for (; registered < panes.size(); ++registered) {
        const auto pane = routing.targets.RegisterPane();
        if (!pane.has_value()
            || routing.pane_targets.Register(pane.value(), policies[registered])
                != PaneTargetStatus::Ok) {
            if (pane.has_value()) {
                (void)routing.targets.UnregisterPane(pane.value());
            }
            for (std::size_t index = 0U; index < registered; ++index) {
                (void)routing.pane_targets.Unregister(panes[index]);
                (void)routing.targets.UnregisterPane(panes[index]);
            }
            return false;
        }
        panes[registered] = pane.value();
    }
    workspace.pane_ids = WorkspacePaneIds{
        panes[0], panes[1], panes[2], panes[3], panes[4],
        panes[5], panes[6], panes[7], panes[8], panes[9]};
    BindWorkspacePaneAliases(workspace);
    return true;
}

void ApplicationHost::BindWorkspacePaneAliases(
    const WorkspaceWindow& workspace) noexcept {
    routing.tool_pane = workspace.pane_ids.tool;
    routing.tool_options_pane = workspace.pane_ids.tool_options;
    routing.color_pane = workspace.pane_ids.color;
    routing.layer_pane = workspace.pane_ids.layer;
    routing.batch_pane = workspace.pane_ids.batch;
    routing.locator_pane = workspace.pane_ids.locator;
    routing.sequence_pane = workspace.pane_ids.sequence;
    routing.light_table_pane = workspace.pane_ids.light_table;
    routing.reference_pane = workspace.pane_ids.reference;
    routing.subpalette_pane = workspace.pane_ids.subpalette;
}

void ApplicationHost::UnregisterWorkspacePanes(
    WorkspaceWindow& workspace) noexcept {
    const std::array panes{
        workspace.pane_ids.tool,
        workspace.pane_ids.tool_options,
        workspace.pane_ids.color,
        workspace.pane_ids.layer,
        workspace.pane_ids.batch,
        workspace.pane_ids.locator,
        workspace.pane_ids.sequence,
        workspace.pane_ids.light_table,
        workspace.pane_ids.reference,
        workspace.pane_ids.subpalette};
    for (const PaneInstanceId pane : panes) {
        if (pane) {
            (void)routing.pane_targets.Unregister(pane);
            (void)routing.targets.UnregisterPane(pane);
        }
    }
    workspace.pane_ids = {};
}

WorkspaceWindow* ApplicationHost::AddWorkspaceWindow() noexcept {
    const WorkspaceWindowId previous = routing.targets.Workspace();
    const auto binding = routing.targets.AddWorkspace();
    if (!binding.has_value()) {
        return nullptr;
    }
    std::uint32_t slot{};
    for (; slot < WorkspaceWindowRegistry::kMaximumWindows; ++slot) {
        bool used{};
        for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
            const WorkspaceWindow* workspace = workspaces_.At(index);
            used = used || (workspace != nullptr
                && workspace->persistence_slot == slot);
        }
        if (!used) {
            break;
        }
    }
    if (slot >= WorkspaceWindowRegistry::kMaximumWindows) {
        (void)routing.targets.RemoveWorkspace(binding->workspace);
        (void)routing.targets.ActivateWorkspace(previous);
        return nullptr;
    }
    if (!workspaces_.Add(
            this,
            binding->workspace,
            binding->editor_group,
            binding->canvas,
            routing.targets.CurrentGeneration(),
            slot)) {
        (void)routing.targets.RemoveWorkspace(binding->workspace);
        (void)routing.targets.ActivateWorkspace(previous);
        return nullptr;
    }
    WorkspaceWindow* workspace = workspaces_.Find(binding->workspace);
    if (workspace == nullptr || !RegisterWorkspacePanes(*workspace)) {
        (void)workspaces_.Remove(binding->workspace);
        (void)routing.targets.RemoveWorkspace(binding->workspace);
        (void)routing.targets.ActivateWorkspace(previous);
        (void)workspaces_.Activate(previous, false);
        return nullptr;
    }
    return workspace;
}

bool ApplicationHost::ActivateWorkspaceWindow(
    WorkspaceWindowId id, bool record_focus) noexcept {
    WorkspaceWindow* workspace = workspaces_.Find(id);
    if (workspace == nullptr
        || !routing.targets.ActivateWorkspace(id)
        || !workspaces_.Activate(id, record_focus)) {
        return false;
    }
    BindWorkspacePaneAliases(*workspace);
    EditorGroup* group = workspace->editors.Active();
    const DocumentViewId view = group == nullptr
        ? DocumentViewId{}
        : group->ActiveView();
    DocumentSession* document = documents_.FindByView(view);
    DocumentView* document_view = document == nullptr
        ? nullptr
        : document->FindView(view);
    if (document == nullptr || document_view == nullptr) {
        return document == nullptr && document_view == nullptr;
    }
    if (!documents_.Activate(document->id) || !document->ActivateView(view)) {
        return false;
    }
    return engine == nullptr
        || (engine->SetActiveSession(document->id, document->generation)
            && engine->SetActiveView(document_view->core_view_id)
                == INKPOD_STATUS_OK);
}

bool ApplicationHost::RemoveWorkspaceWindow(WorkspaceWindowId id) noexcept {
    WorkspaceWindow* workspace = workspaces_.Find(id);
    if (workspace == nullptr || workspaces_.Count() <= 1U) {
        return false;
    }
    for (std::size_t index = 0U; index < workspace->editors.GroupCount(); ++index) {
        const EditorGroup* group = workspace->editors.GroupAt(index);
        if (group == nullptr || group->ViewCount() != 0U) {
            return false;
        }
    }
    UnregisterWorkspacePanes(*workspace);
    if (!routing.targets.RemoveWorkspace(id)
        || !workspaces_.Remove(id)) {
        return false;
    }
    WorkspaceWindow* current = workspaces_.Current();
    return current != nullptr
        && ActivateWorkspaceWindow(current->id, false);
}

bool ApplicationHost::MoveDocumentViewToWorkspace(
    DocumentViewId view, WorkspaceWindowId destination) noexcept {
    WorkspaceWindow* target = workspaces_.Find(destination);
    EditorGroup* target_group = target == nullptr
        ? nullptr
        : target->editors.Active();
    return target_group != nullptr
        && MoveDocumentView(
            view,
            destination,
            target_group->id,
            target_group->ViewCount());
}

bool ApplicationHost::MoveDocumentView(
    DocumentViewId view,
    WorkspaceWindowId destination_workspace,
    EditorGroupId destination_group,
    std::size_t insertion_index) noexcept {
    WorkspaceWindow* source = WorkspaceForView(view);
    WorkspaceWindow* target = workspaces_.Find(destination_workspace);
    EditorGroup* source_group = source == nullptr
        ? nullptr
        : source->editors.FindByView(view);
    EditorGroup* target_group = target == nullptr
        ? nullptr
        : target->editors.Find(destination_group);
    DocumentSession* document = documents_.FindByView(view);
    if (source == nullptr || target == nullptr || source_group == nullptr
        || target_group == nullptr || document == nullptr
        || (target_group->ViewCount() >= EditorGroup::kMaximumViews
            && source_group != target_group)
        || insertion_index > target_group->ViewCount()) {
        return false;
    }
    const WorkspaceWindowId source_workspace = source->id;
    const EditorGroupId source_group_id = source_group->id;
    const EditorArea source_before = source->editors;
    const EditorArea target_before = target->editors;
    const CommandContext previous = routing.targets.Capture();
    const bool placed = source == target
        ? source->editors.MoveView(view, destination_group, insertion_index)
        : source->editors.RemoveView(view)
            && target_group->InsertView(view, insertion_index);
    if (!placed) {
        source->editors = source_before;
        if (source != target) {
            target->editors = target_before;
        }
        return false;
    }
    if (source_group == target_group) {
        if (ActivateDocumentView(view)) {
            return true;
        }
        source->editors = source_before;
        return false;
    }
    if (!routing.targets.MoveDocumentView(view, destination_group)) {
        source->editors = source_before;
        if (source != target) {
            target->editors = target_before;
        }
        return false;
    }
    const auto bind_group = [this](WorkspaceWindow& workspace, EditorGroupId id) {
        EditorGroup* group = workspace.editors.Find(id);
        if (group == nullptr || group->canvas == nullptr) {
            return group != nullptr;
        }
        const DocumentSession* active = documents_.FindByView(group->ActiveView());
        return active == nullptr
            ? renderer::UnbindCanvasSnapshotSink(group->canvas)
            : renderer::BindCanvasSnapshotSink(
                group->canvas,
                active->id,
                group->ActiveView(),
                active->generation);
    };
    if (bind_group(*source, source_group_id)
        && bind_group(*target, destination_group)
        && ActivateDocumentView(view)) {
        return true;
    }

    (void)routing.targets.MoveDocumentView(view, source_group_id);
    source->editors = source_before;
    if (source != target) {
        target->editors = target_before;
    }
    (void)bind_group(*source, source_group_id);
    if (source != target || source_group_id != destination_group) {
        (void)bind_group(*target, destination_group);
    }
    if (previous.workspace.has_value()) {
        (void)ActivateWorkspaceWindow(previous.workspace.value(), false);
    }
    if (previous.document_view.has_value()) {
        (void)ActivateDocumentView(previous.document_view.value());
    } else {
        (void)ActivateWorkspaceWindow(source_workspace, false);
    }
    return false;
}

TabDragCoordinator& ApplicationHost::TabDrag() noexcept {
    return tab_drag_;
}

const TabDragCoordinator& ApplicationHost::TabDrag() const noexcept {
    return tab_drag_;
}

DocumentSession& ApplicationHost::Document() noexcept {
    assert(documents_.Current() != nullptr);
    return *documents_.Current();
}

const DocumentSession& ApplicationHost::Document() const noexcept {
    assert(documents_.Current() != nullptr);
    return *documents_.Current();
}

DocumentView& ApplicationHost::ActiveView() noexcept {
    assert(Document().ActiveView() != nullptr);
    return *Document().ActiveView();
}

const DocumentView& ApplicationHost::ActiveView() const noexcept {
    assert(Document().ActiveView() != nullptr);
    return *Document().ActiveView();
}

bool ApplicationHost::ReplaceDocumentSession(
    DocumentSessionId id,
    Generation generation,
    DocumentViewId initial_view) noexcept {
    if (id != routing.targets.DocumentSession()
        || generation != routing.targets.CurrentGeneration()
        || initial_view != routing.targets.ActiveDocumentView()
        || engine == nullptr) {
        return false;
    }
    DocumentSession& current = Document();
    const DocumentSessionId old_id = current.id;
    const Generation old_generation = current.generation;
    const bool had_core = old_id && old_generation
        && engine->HasSession(old_id, old_generation);
    const InkpodStatus binding_status = had_core
        ? engine->RebindSession(old_id, old_generation, id, generation)
        : engine->CreateSession(id, generation);
    if (binding_status != INKPOD_STATUS_OK) {
        return false;
    }
    if (!engine->RegisterDocumentView(id, generation, initial_view, 0U)
        || !documents_.Replace(id, generation, initial_view, engine.get())
        || !Workspace().editors.ResetViews(initial_view)) {
        (void)engine->UnregisterDocumentView(id, generation, initial_view);
        if (had_core) {
            (void)engine->RebindSession(id, generation, old_id, old_generation);
        } else {
            (void)engine->CloseSession(id, generation);
        }
        return false;
    }
    if (!engine->SetActiveSession(id, generation)) {
        return false;
    }
    return Workspace().windows.canvas == nullptr
        || renderer::BindCanvasSnapshotSink(
            Workspace().windows.canvas,
            id,
            initial_view,
            generation);
}

DocumentRegistry& ApplicationHost::Documents() noexcept {
    return documents_;
}

const DocumentRegistry& ApplicationHost::Documents() const noexcept {
    return documents_;
}

std::optional<ApplicationHost::DocumentBinding>
ApplicationHost::AddDocumentSession() noexcept {
    if (engine == nullptr) {
        return std::nullopt;
    }
    const CommandContext previous = routing.targets.Capture();
    const auto issued = routing.targets.AddDocument();
    if (!issued.has_value()) {
        return std::nullopt;
    }
    const DocumentBinding binding{
        issued.value(),
        routing.targets.ActiveDocumentView(),
        routing.targets.CurrentGeneration()};
    if (engine->CreateSession(binding.session, binding.generation)
            != INKPOD_STATUS_OK
        || !engine->RegisterDocumentView(
            binding.session, binding.generation, binding.view, 0U)
        || !documents_.Add(
            binding.session,
            binding.generation,
            binding.view,
            engine.get())
        || !Workspace().editors.AddView(
            routing.targets.EditorGroup(), binding.view)) {
        (void)engine->UnregisterDocumentView(
            binding.session, binding.generation, binding.view);
        (void)documents_.Remove(binding.session);
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)routing.targets.ActivateDocument(
                previous.document_session.value(),
                previous.document_view.value());
        }
        return std::nullopt;
    }
    if (!ActivateDocumentView(binding.view)) {
        (void)Workspace().editors.RemoveView(binding.view);
        (void)documents_.Remove(binding.session);
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)ActivateDocumentView(previous.document_view.value());
        }
        return std::nullopt;
    }
    return binding;
}

bool ApplicationHost::ActivateDocumentView(DocumentViewId view) noexcept {
    DocumentSession* document = documents_.FindByView(view);
    DocumentView* target = document == nullptr ? nullptr : document->FindView(view);
    WorkspaceWindow* target_workspace = WorkspaceForView(view);
    EditorGroup* target_group = target_workspace == nullptr
        ? nullptr
        : target_workspace->editors.FindByView(view);
    if (document == nullptr || target == nullptr || target_workspace == nullptr
        || target_group == nullptr
        || engine == nullptr) {
        return false;
    }
    WorkspaceWindow* previous_workspace = workspaces_.Current();
    DocumentSession* previous_document = documents_.Current();
    DocumentView* previous_view = previous_document == nullptr
        ? nullptr
        : previous_document->ActiveView();
    EditorGroup* previous_group = previous_workspace == nullptr
        ? nullptr
        : previous_workspace->editors.Active();
    renderer::CancelCanvasStroke(
        previous_group == nullptr ? nullptr : previous_group->canvas);
    if (!engine->SetActiveSession(document->id, document->generation)
        || (target_group->canvas != nullptr
            && !renderer::BindCanvasSnapshotSink(
                target_group->canvas,
                document->id,
                target->id,
                document->generation))
        || engine->SetActiveView(target->core_view_id) != INKPOD_STATUS_OK) {
        if (previous_document != nullptr && previous_view != nullptr) {
            (void)engine->SetActiveSession(
                previous_document->id, previous_document->generation);
            if (previous_group != nullptr && previous_group->canvas != nullptr) {
                (void)renderer::BindCanvasSnapshotSink(
                    previous_group->canvas,
                    previous_document->id,
                    previous_view->id,
                    previous_document->generation);
            }
            (void)engine->SetActiveView(previous_view->core_view_id);
        }
        return false;
    }
    const bool activated = workspaces_.Activate(target_workspace->id, false)
        && target_workspace->editors.Activate(target_group->id)
        && target_group->ActivateView(view)
        && documents_.Activate(document->id)
        && document->ActivateView(view)
        && routing.targets.ActivateDocument(document->id, view);
    if (activated) {
        BindWorkspacePaneAliases(*target_workspace);
        target_workspace->windows.canvas = target_group->canvas;
        target_workspace->windows.document_tabs = target_group->document_tabs;
    }
    return activated;
}

bool ApplicationHost::CloseDocumentView(DocumentViewId view) noexcept {
    DocumentSession* document = documents_.FindByView(view);
    if (document == nullptr || document->ViewCount() <= 1U || engine == nullptr) {
        return false;
    }
    const DocumentView* closing = document->FindView(view);
    WorkspaceWindow* source_workspace = WorkspaceForView(view);
    if (closing == nullptr || source_workspace == nullptr) {
        return false;
    }
    DocumentViewId replacement{};
    for (std::size_t index = 0U; index < document->ViewCount(); ++index) {
        const DocumentView* candidate = document->ViewAt(index);
        if (candidate != nullptr && candidate->id != view) {
            replacement = candidate->id;
            break;
        }
    }
    const InkpodStatus status = engine->Invoke(
        document->id,
        document->generation,
        [core_view_id = closing->core_view_id](InkpodCore* core) {
            return inkpod_core_view_close(core, core_view_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK
        || !engine->UnregisterDocumentView(
            document->id, document->generation, view)
        || !source_workspace->editors.RemoveView(view)
        || !document->RemoveView(view)
        || !routing.targets.RemoveDocumentView(view)) {
        return false;
    }
    EditorGroup* active_group = source_workspace->editors.Active();
    if (active_group != nullptr && active_group->ActiveView()) {
        replacement = active_group->ActiveView();
    } else if (source_workspace->editors.GroupCount() == 2U
        && active_group != nullptr) {
        const EditorGroup* other = source_workspace->editors.Other(active_group->id);
        if (other != nullptr) {
            replacement = other->ActiveView();
        }
    }
    return !replacement || ActivateDocumentView(replacement);
}

bool ApplicationHost::CloseDocumentSession(DocumentSessionId session) noexcept {
    DocumentSession* document = documents_.Find(session);
    if (document == nullptr || engine == nullptr) {
        return false;
    }
    const Generation generation = document->generation;
    for (std::size_t index = 0U; index < workspaces_.Count(); ++index) {
        WorkspaceWindow* workspace = workspaces_.At(index);
        if (workspace == nullptr) {
            continue;
        }
        for (std::size_t group_index = 0U;
             group_index < workspace->editors.GroupCount(); ++group_index) {
            EditorGroup* group = workspace->editors.GroupAt(group_index);
            if (group != nullptr) {
                for (std::size_t view_index = 0U;
                     view_index < group->ViewCount(); ++view_index) {
                    const DocumentViewId candidate = group->ViewAt(view_index);
                    if (document->FindView(candidate) != nullptr) {
                        renderer::CancelCanvasStroke(group->canvas);
                        break;
                    }
                }
            }
        }
    }
    if (engine->CloseSession(session, generation) != INKPOD_STATUS_OK) {
        return false;
    }
    routing.pane_targets.DocumentClosed(session);
    for (std::size_t view_index = document->ViewCount();
         view_index > 0U; --view_index) {
        const DocumentView* view = document->ViewAt(view_index - 1U);
        WorkspaceWindow* workspace = view == nullptr
            ? nullptr
            : WorkspaceForView(view->id);
        if (view != nullptr && workspace != nullptr) {
            (void)workspace->editors.RemoveView(view->id);
        }
    }
    if (!documents_.Remove(session)
        || !routing.targets.RemoveDocument(session)) {
        return false;
    }
    for (std::size_t workspace_index = 0U;
         workspace_index < workspaces_.Count(); ++workspace_index) {
        WorkspaceWindow* workspace = workspaces_.At(workspace_index);
        if (workspace == nullptr) {
            continue;
        }
        for (std::size_t group_index = 0U;
             group_index < workspace->editors.GroupCount(); ++group_index) {
            EditorGroup* group = workspace->editors.GroupAt(group_index);
            const DocumentSession* remaining = group == nullptr
                ? nullptr
                : documents_.FindByView(group->ActiveView());
            if (group == nullptr || group->canvas == nullptr) {
                continue;
            }
            if (remaining == nullptr) {
                (void)renderer::UnbindCanvasSnapshotSink(group->canvas);
            } else {
                (void)renderer::BindCanvasSnapshotSink(
                    group->canvas,
                    remaining->id,
                    group->ActiveView(),
                    remaining->generation);
            }
        }
    }
    WorkspaceWindow* current_workspace = workspaces_.Current();
    EditorGroup* active_group = current_workspace == nullptr
        ? nullptr
        : current_workspace->editors.Active();
    DocumentViewId replacement = active_group == nullptr
        ? DocumentViewId{}
        : active_group->ActiveView();
    if (!replacement) {
        for (std::size_t workspace_index = 0U;
             workspace_index < workspaces_.Count() && !replacement;
             ++workspace_index) {
            const WorkspaceWindow* workspace = workspaces_.At(workspace_index);
            for (std::size_t group_index = 0U;
                 workspace != nullptr
                     && group_index < workspace->editors.GroupCount();
                 ++group_index) {
                const EditorGroup* group = workspace->editors.GroupAt(group_index);
                if (group != nullptr && group->ActiveView()) {
                    replacement = group->ActiveView();
                    break;
                }
            }
        }
    }
    return !replacement || ActivateDocumentView(replacement);
}

std::uint32_t ApplicationHost::IssueUntitledNumber() noexcept {
    const std::uint32_t result = next_untitled_number_++;
    if (next_untitled_number_ == 0U) {
        next_untitled_number_ = 1U;
    }
    return result == 0U ? next_untitled_number_++ : result;
}

bool ApplicationHost::RecordRecentDocument(
    std::wstring path,
    DocumentIdentity identity) noexcept {
    return recent_documents_.Record(
        std::move(path), std::move(identity));
}

bool ApplicationHost::RemoveRecentDocument(std::size_t index) noexcept {
    return recent_documents_.Remove(index);
}

const RecentDocumentEntry* ApplicationHost::RecentDocumentAt(
    std::size_t index) const noexcept {
    return recent_documents_.At(index);
}

std::size_t ApplicationHost::RecentDocumentCount() const noexcept {
    return recent_documents_.Count();
}

void ApplicationHost::DetachCoreSessions() noexcept {
    documents_.ClearCoreBindings();
}

}  // namespace inkpod::app
