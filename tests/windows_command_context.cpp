#include "app/command_context.h"
#include "app/pane_target.h"
#include "app/tab_drag.h"

#include <cstdlib>
#include <functional>
#include <type_traits>
#include <unordered_map>

namespace {

using inkpod::app::CommandContext;
using inkpod::app::CanvasId;
using inkpod::app::CommandRequest;
using inkpod::app::CommandResolveStatus;
using inkpod::app::CommandTargetRegistry;
using inkpod::app::CommandTimerKind;
using inkpod::app::CommandTimerRegistry;
using inkpod::app::DocumentSessionId;
using inkpod::app::DocumentViewId;
using inkpod::app::EditorGroupId;
using inkpod::app::Generation;
using inkpod::app::JobSessionId;
using inkpod::app::PaneInstanceId;
using inkpod::app::PaneTargetNotice;
using inkpod::app::PaneTargetPolicy;
using inkpod::app::PaneTargetRegistry;
using inkpod::app::PaneTargetStatus;
using inkpod::app::WorkspaceWindowId;
using inkpod::app::FrontendTokenSource;
using inkpod::app::DragOperation;
using inkpod::app::TabDragCoordinator;
using inkpod::app::TabDropKind;
using inkpod::app::TabDropTarget;
using inkpod::app::kDocumentViewCommandScope;

static_assert(!std::is_convertible_v<WorkspaceWindowId, DocumentSessionId>);
static_assert(!std::is_convertible_v<DocumentSessionId, DocumentViewId>);
static_assert(!std::is_convertible_v<DocumentViewId, std::uint64_t>);
static_assert(!std::is_convertible_v<std::uint64_t, DocumentViewId>);
static_assert(!std::is_same_v<PaneInstanceId, JobSessionId>);
static_assert(std::is_trivially_copyable_v<Generation>);

bool StrongIdsHashAndCompare() {
    const WorkspaceWindowId first(11U);
    const WorkspaceWindowId same(11U);
    const WorkspaceWindowId other(12U);
    std::unordered_map<WorkspaceWindowId, int> values;
    values.emplace(first, 7);
    return first == same && first != other && values[same] == 7;
}

bool CapturedViewSurvivesFocusChangeButNotClose() {
    CommandTargetRegistry registry;
    registry.Initialize();
    (void)registry.ReplaceDocument();
    const DocumentViewId first = registry.ActiveDocumentView();
    const CommandContext captured = registry.Capture();
    const auto second = registry.AddDocumentView();
    if (!second.has_value() || first == second.value()
        || registry.Resolve(captured, kDocumentViewCommandScope)
               != CommandResolveStatus::Ok) {
        return false;
    }
    if (!registry.RemoveDocumentView(first)) {
        return false;
    }
    return registry.Resolve(captured, kDocumentViewCommandScope)
        == CommandResolveStatus::StaleTarget;
}

bool ReplacementRejectsQueuedContext() {
    CommandTargetRegistry registry;
    registry.Initialize();
    (void)registry.ReplaceDocument();
    const CommandContext captured = registry.Capture();
    (void)registry.ReplaceDocument();
    return registry.Resolve(captured, kDocumentViewCommandScope)
        == CommandResolveStatus::StaleGeneration;
}

bool CapturedSessionDoesNotFollowTabFocus() {
    CommandTargetRegistry registry;
    registry.Initialize();
    const DocumentSessionId first_session = registry.ReplaceDocument();
    const DocumentViewId first_view = registry.ActiveDocumentView();
    const CommandContext first = registry.Capture();
    const auto second_session = registry.AddDocument();
    if (!second_session.has_value()
        || second_session.value() == first_session) {
        return false;
    }
    const DocumentViewId second_view = registry.ActiveDocumentView();
    const CommandContext second = registry.Capture();
    if (second_view == first_view
        || registry.Resolve(first, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok
        || registry.Resolve(second, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok) {
        return false;
    }
    CommandContext crossed = second;
    crossed.document_view = first_view;
    if (registry.Resolve(crossed, kDocumentViewCommandScope)
        != CommandResolveStatus::StaleTarget) {
        return false;
    }
    if (!registry.RemoveDocument(first_session)) {
        return false;
    }
    return registry.Resolve(first, kDocumentViewCommandScope)
            == CommandResolveStatus::StaleTarget
        && registry.Resolve(second, kDocumentViewCommandScope)
            == CommandResolveStatus::Ok;
}

bool InvalidRequestsAreRejected() {
    CommandTargetRegistry registry;
    registry.Initialize();
    (void)registry.ReplaceDocument();
    CommandContext missing = registry.Capture();
    missing.document_view.reset();
    const CommandRequest unknown{0U, registry.Capture()};
    return registry.Resolve(missing, kDocumentViewCommandScope)
            == CommandResolveStatus::MissingScope
        && registry.Resolve(unknown, kDocumentViewCommandScope)
            == CommandResolveStatus::UnknownCommand;
}

bool PaneAndJobTargetsDoNotFallback() {
    CommandTargetRegistry registry;
    registry.Initialize();
    (void)registry.ReplaceDocument();
    const auto pane = registry.RegisterPane();
    const auto job = registry.BeginJob();
    if (!pane.has_value() || !job.has_value()) {
        return false;
    }
    const CommandContext context = registry.Capture(pane, job);
    if (registry.Resolve(
            context,
            inkpod::app::CommandTargetScope::Pane
                | inkpod::app::CommandTargetScope::Job)
        != CommandResolveStatus::Ok) {
        return false;
    }
    (void)registry.EndJob(job.value());
    return registry.Resolve(context, inkpod::app::CommandTargetScope::Job)
        == CommandResolveStatus::StaleTarget;
}

bool JobsCanBindAnExactInactiveDocument() {
    CommandTargetRegistry registry;
    registry.Initialize();
    (void)registry.ReplaceDocument();
    const CommandContext source = registry.Capture();
    const auto second_document = registry.AddDocument();
    if (!second_document.has_value()) {
        return false;
    }
    const CommandContext active = registry.Capture();
    const auto job = registry.BeginJob(source);
    if (!job.has_value()) {
        return false;
    }
    CommandContext source_job = source;
    source_job.job = job;
    CommandContext crossed = active;
    crossed.job = job;
    CommandContext stale = source;
    stale.generation = Generation(registry.CurrentGeneration().Value() + 1U);
    const bool valid = registry.Resolve(
                           source_job,
                           inkpod::app::kDocumentSessionCommandScope
                               | inkpod::app::CommandTargetScope::Job)
            == CommandResolveStatus::Ok
        && registry.Resolve(
               crossed,
               inkpod::app::kDocumentSessionCommandScope
                   | inkpod::app::CommandTargetScope::Job)
            == CommandResolveStatus::StaleTarget
        && !registry.BeginJob(stale).has_value();
    return registry.EndJob(job.value()) && valid;
}

bool AuxiliaryCanvasIdsHaveIndependentBoundedLifetime() {
    CommandTargetRegistry registry;
    registry.Initialize();
    (void)registry.ReplaceDocument();
    const CanvasId editor_canvas = registry.Canvas();
    const auto first = registry.RegisterAuxiliaryCanvas();
    const auto second = registry.RegisterAuxiliaryCanvas();
    if (!first.has_value() || !second.has_value()
        || first.value() == second.value() || first.value() == editor_canvas
        || second.value() == editor_canvas
        || !registry.UnregisterAuxiliaryCanvas(first.value())
        || registry.UnregisterAuxiliaryCanvas(first.value())) {
        return false;
    }
    std::array<CanvasId, 16U> canvases{};
    canvases[0] = second.value();
    for (std::size_t index = 1U; index < canvases.size(); ++index) {
        const auto canvas = registry.RegisterAuxiliaryCanvas();
        if (!canvas.has_value()) {
            return false;
        }
        canvases[index] = canvas.value();
    }
    if (registry.RegisterAuxiliaryCanvas().has_value()) {
        return false;
    }
    registry.InvalidateAll();
    return !registry.UnregisterAuxiliaryCanvas(canvases[0]);
}

bool GenerationTaggedTokensNeverRetarget() {
    CommandTargetRegistry registry;
    registry.Initialize();
    (void)registry.ReplaceDocument();
    const CommandContext captured = registry.Capture();
    CommandTimerRegistry timers;
    const auto timer = timers.Arm(CommandTimerKind::Autosave, captured);
    FrontendTokenSource tokens;
    const auto drag = tokens.IssueDrag(captured);
    const auto notification = tokens.IssueNotification(
        registry.CurrentGeneration());
    (void)registry.ReplaceDocument();
    const auto resolved_timer = timers.Resolve(timer.value);
    return resolved_timer.has_value() && drag.context == captured
        && notification.generation == captured.generation
        && registry.Resolve(
               resolved_timer->context, kDocumentViewCommandScope)
            == CommandResolveStatus::StaleGeneration;
}

bool EditorGroupsRouteCapturedViewsWithoutRetargeting() {
    CommandTargetRegistry registry;
    registry.Initialize();
    const DocumentSessionId document = registry.ReplaceDocument();
    const DocumentViewId first_view = registry.ActiveDocumentView();
    const EditorGroupId first_group = registry.EditorGroup();
    const CommandContext first = registry.Capture();
    const auto second_group = registry.AddEditorGroup();
    if (!document || !first_view || !first_group || !second_group.has_value()
        || second_group->group == first_group
        || second_group->canvas == registry.CanvasForGroup(first_group)
        || registry.EditorGroupCount() != 2U
        || registry.AddEditorGroup().has_value()) {
        return false;
    }
    const auto second_view = registry.AddDocumentViewTo(second_group->group);
    if (!second_view.has_value() || second_view.value() == first_view
        || registry.GroupForView(first_view) != first_group
        || registry.GroupForView(second_view.value()) != second_group->group) {
        return false;
    }
    const CommandContext second = registry.Capture();
    if (registry.Resolve(first, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok
        || registry.Resolve(second, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok
        || !registry.ActivateEditorGroup(first_group)
        || registry.ActiveDocumentView() != first_view
        || registry.Resolve(second, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok) {
        return false;
    }

    if (!registry.MoveDocumentView(first_view, second_group->group)
        || registry.GroupForView(first_view) != second_group->group
        || registry.Resolve(first, kDocumentViewCommandScope)
            != CommandResolveStatus::StaleTarget
        || !registry.RemoveEditorGroup(first_group)
        || registry.EditorGroupCount() != 1U
        || registry.GroupForView(first_view) != second_group->group
        || registry.GroupForView(second_view.value()) != second_group->group
        || registry.RemoveEditorGroup(second_group->group)) {
        return false;
    }
    return registry.Resolve(second, kDocumentViewCommandScope)
        == CommandResolveStatus::Ok;
}

bool WorkspacesRouteCapturedViewsWithoutFocusRetargeting() {
    CommandTargetRegistry registry;
    registry.Initialize();
    const DocumentSessionId document = registry.ReplaceDocument();
    const DocumentViewId first_view = registry.ActiveDocumentView();
    const WorkspaceWindowId first_workspace = registry.Workspace();
    const CommandContext first = registry.Capture();
    const auto second = registry.AddWorkspace();
    if (!document || !first_view || !first_workspace || !second.has_value()
        || second->workspace == first_workspace
        || registry.WorkspaceCount() != 2U
        || registry.WorkspaceForGroup(second->editor_group) != second->workspace
        || registry.Resolve(first, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok
        || registry.Capture().document_view.has_value()) {
        return false;
    }
    if (!registry.ActivateWorkspace(first_workspace)
        || registry.ActiveDocumentView() != first_view
        || registry.Resolve(first, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok
        || !registry.MoveDocumentView(first_view, second->editor_group)
        || registry.WorkspaceForView(first_view) != second->workspace
        || registry.Resolve(first, kDocumentViewCommandScope)
            != CommandResolveStatus::StaleTarget) {
        return false;
    }
    const CommandContext moved = registry.Capture();
    if (registry.Resolve(moved, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok
        || !registry.RemoveWorkspace(first_workspace)
        || registry.WorkspaceCount() != 1U
        || registry.Resolve(first, kDocumentViewCommandScope)
            != CommandResolveStatus::StaleTarget
        || registry.Resolve(moved, kDocumentViewCommandScope)
            != CommandResolveStatus::Ok
        || registry.RemoveWorkspace(second->workspace)) {
        return false;
    }
    return registry.Workspace() == second->workspace
        && registry.ActiveDocumentView() == first_view;
}

bool PanePoliciesCaptureAndRejectStaleTargets() {
    CommandTargetRegistry targets;
    targets.Initialize();
    const DocumentSessionId first_document = targets.ReplaceDocument();
    const auto follow_pane = targets.RegisterPane();
    const auto application_pane = targets.RegisterPane();
    const auto reference_pane = targets.RegisterPane();
    const auto batch_pane = targets.RegisterPane();
    if (!follow_pane.has_value() || !application_pane.has_value()
        || !reference_pane.has_value() || !batch_pane.has_value()) {
        return false;
    }

    PaneTargetRegistry panes;
    if (panes.Register(
            follow_pane.value(), PaneTargetPolicy::FollowActiveView)
            != PaneTargetStatus::Ok
        || panes.Register(
               application_pane.value(), PaneTargetPolicy::Application)
            != PaneTargetStatus::Ok
        || panes.Register(
               reference_pane.value(), PaneTargetPolicy::FollowActiveView)
            != PaneTargetStatus::Ok
        || panes.Register(
               batch_pane.value(), PaneTargetPolicy::FollowActiveView)
            != PaneTargetStatus::Ok) {
        return false;
    }
    const CommandContext first = targets.Capture();
    if (panes.PinDocument(reference_pane.value(), first, targets)
            != PaneTargetStatus::Ok) {
        return false;
    }
    const auto second_document = targets.AddDocument();
    if (!second_document.has_value()) {
        return false;
    }
    const CommandContext second = targets.Capture();
    const auto followed = panes.CaptureAction(
        follow_pane.value(), second, targets);
    const auto pinned = panes.CaptureAction(
        reference_pane.value(), second, targets);
    const auto application = panes.CaptureAction(
        application_pane.value(), second, targets);
    if (followed.status != PaneTargetStatus::Ok
        || followed.context.document_session != second_document
        || pinned.status != PaneTargetStatus::Ok
        || pinned.context.document_session != first_document
        || application.status != PaneTargetStatus::Ok
        || application.context.document_session != second_document
        || application.context.pane != application_pane) {
        return false;
    }

    const auto job = targets.BeginJob();
    if (!job.has_value()) {
        return false;
    }
    const CommandContext job_context = targets.Capture(
        batch_pane, job);
    if (panes.BindJob(batch_pane.value(), job_context, targets)
            != PaneTargetStatus::Ok
        || panes.CaptureAction(batch_pane.value(), first, targets).context
                != job_context) {
        return false;
    }
    panes.JobClosed(job.value());
    (void)targets.EndJob(job.value());
    std::uint64_t notice_sequence{};
    if (panes.FollowActive(batch_pane.value()) != PaneTargetStatus::Ok
        || panes.CaptureAction(batch_pane.value(), second, targets).status
            != PaneTargetStatus::Ok
        || panes.CaptureAction(batch_pane.value(), second, targets)
                .context.document_session != second_document
        || panes.ConsumeNotice(batch_pane.value(), notice_sequence)
            != PaneTargetNotice::JobClosed
        || notice_sequence == 0U) {
        return false;
    }

    panes.DocumentClosed(first_document);
    if (!targets.RemoveDocument(first_document)) {
        return false;
    }
    const auto after_close = panes.CaptureAction(
        reference_pane.value(), second, targets);
    return after_close.status == PaneTargetStatus::Ok
        && after_close.policy == PaneTargetPolicy::FollowActiveView
        && after_close.context.document_session == second_document
        && panes.ConsumeNotice(reference_pane.value(), notice_sequence)
            == PaneTargetNotice::PinnedDocumentClosed
        && panes.ConsumeNotice(reference_pane.value(), notice_sequence)
            == PaneTargetNotice::None;
}

bool TabDragTokensStayValueOnlyAndTransactional() {
    CommandTargetRegistry targets;
    targets.Initialize();
    (void)targets.ReplaceDocument();
    const CommandContext source = targets.Capture();
    FrontendTokenSource tokens;
    TabDragCoordinator drag;
    if (drag.Arm(
            tokens.IssueDrag(source, DragOperation::CanvasStroke),
            source,
            0U,
            100,
            100)
        || !drag.Arm(
            tokens.IssueDrag(source, DragOperation::TabMove),
            source,
            1U,
            100,
            100)
        || !drag.IsArmed()
        || drag.IsDragging()
        || drag.TryBegin(102, 102, 4, 4)
        || !drag.TryBegin(105, 100, 4, 4)
        || !drag.SetOperation(DragOperation::TabCopy)) {
        return false;
    }
    const TabDropTarget target{
        TabDropKind::EditorGroup,
        source.workspace.value(),
        source.editor_group.value(),
        2U};
    if (!drag.UpdateTarget(target)) {
        return false;
    }
    const auto request = drag.TakeDrop();
    if (!request.has_value()
        || request->token.context != source
        || request->token.operation != DragOperation::TabCopy
        || request->restore_context != source
        || request->source_index != 1U
        || request->target != target
        || drag.IsArmed()
        || drag.IsDragging()
        || drag.TakeDrop().has_value()) {
        return false;
    }
    return drag.Arm(
               tokens.IssueDrag(source, DragOperation::TabMove),
               source,
               0U,
               0,
               0)
        && drag.Cancel()
        && !drag.Cancel();
}

}  // namespace

int main() {
    return StrongIdsHashAndCompare()
            && CapturedViewSurvivesFocusChangeButNotClose()
            && ReplacementRejectsQueuedContext()
            && CapturedSessionDoesNotFollowTabFocus()
            && InvalidRequestsAreRejected()
            && PaneAndJobTargetsDoNotFallback()
            && JobsCanBindAnExactInactiveDocument()
            && AuxiliaryCanvasIdsHaveIndependentBoundedLifetime()
            && GenerationTaggedTokensNeverRetarget()
            && EditorGroupsRouteCapturedViewsWithoutRetargeting()
            && WorkspacesRouteCapturedViewsWithoutFocusRetargeting()
            && PanePoliciesCaptureAndRejectStaleTargets()
            && TabDragTokensStayValueOnlyAndTransactional()
        ? EXIT_SUCCESS
        : EXIT_FAILURE;
}
