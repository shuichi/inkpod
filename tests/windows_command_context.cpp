#include "app/command_context.h"

#include <cstdlib>
#include <functional>
#include <type_traits>
#include <unordered_map>

namespace {

using inkpod::app::CommandContext;
using inkpod::app::CommandRequest;
using inkpod::app::CommandResolveStatus;
using inkpod::app::CommandTargetRegistry;
using inkpod::app::CommandTimerKind;
using inkpod::app::CommandTimerRegistry;
using inkpod::app::DocumentSessionId;
using inkpod::app::DocumentViewId;
using inkpod::app::Generation;
using inkpod::app::JobSessionId;
using inkpod::app::PaneInstanceId;
using inkpod::app::WorkspaceWindowId;
using inkpod::app::FrontendTokenSource;
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

}  // namespace

int main() {
    return StrongIdsHashAndCompare()
            && CapturedViewSurvivesFocusChangeButNotClose()
            && ReplacementRejectsQueuedContext()
            && InvalidRequestsAreRejected()
            && PaneAndJobTargetsDoNotFallback()
            && GenerationTaggedTokensNeverRetarget()
        ? EXIT_SUCCESS
        : EXIT_FAILURE;
}
