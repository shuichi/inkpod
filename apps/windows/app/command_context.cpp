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

template <typename Id>
bool MatchesRequired(
    const std::optional<Id>& supplied,
    Id current,
    bool required) noexcept {
    if (required && !supplied.has_value()) {
        return false;
    }
    return !supplied.has_value() || supplied.value() == current;
}

}  // namespace

void CommandTargetRegistry::Initialize() noexcept {
    if (workspace_) {
        return;
    }
    generation_ = Generation(1U);
    workspace_ = Issue<WorkspaceWindowId>();
    editor_group_ = Issue<EditorGroupId>();
    canvas_ = Issue<CanvasId>();
}

void CommandTargetRegistry::InvalidateAll() noexcept {
    AdvanceGeneration();
    document_session_ = {};
    active_document_view_ = {};
    views_.fill({});
    view_count_ = 0U;
    jobs_.fill({});
    job_count_ = 0U;
    panes_.fill({});
    pane_count_ = 0U;
    workspace_ = {};
    editor_group_ = {};
    canvas_ = {};
}

DocumentSessionId CommandTargetRegistry::ReplaceDocument() noexcept {
    Initialize();
    AdvanceGeneration();
    document_session_ = Issue<DocumentSessionId>();
    views_.fill({});
    view_count_ = 1U;
    views_[0] = Issue<DocumentViewId>();
    active_document_view_ = views_[0];
    jobs_.fill({});
    job_count_ = 0U;
    return document_session_;
}

std::optional<DocumentViewId> CommandTargetRegistry::AddDocumentView() noexcept {
    if (!document_session_ || view_count_ >= views_.size()) {
        return std::nullopt;
    }
    const DocumentViewId view = Issue<DocumentViewId>();
    views_[view_count_++] = view;
    active_document_view_ = view;
    return view;
}

bool CommandTargetRegistry::ActivateDocumentView(DocumentViewId view) noexcept {
    if (!ContainsView(view)) {
        return false;
    }
    active_document_view_ = view;
    return true;
}

bool CommandTargetRegistry::RemoveDocumentView(DocumentViewId view) noexcept {
    if (!Remove(views_, view_count_, view)) {
        return false;
    }
    if (active_document_view_ == view) {
        active_document_view_ = view_count_ == 0U ? DocumentViewId{} : views_[0];
    }
    return true;
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

std::optional<JobSessionId> CommandTargetRegistry::BeginJob() noexcept {
    if (!document_session_ || job_count_ >= jobs_.size()) {
        return std::nullopt;
    }
    const JobSessionId job = Issue<JobSessionId>();
    jobs_[job_count_++] = job;
    return job;
}

bool CommandTargetRegistry::EndJob(JobSessionId job) noexcept {
    return Remove(jobs_, job_count_, job);
}

CommandContext CommandTargetRegistry::Capture(
    std::optional<PaneInstanceId> pane,
    std::optional<JobSessionId> job) const noexcept {
    CommandContext context{};
    if (workspace_) {
        context.workspace = workspace_;
    }
    if (editor_group_) {
        context.editor_group = editor_group_;
    }
    if (document_session_) {
        context.document_session = document_session_;
    }
    if (active_document_view_) {
        context.document_view = active_document_view_;
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
    if (!MatchesRequired(
            context.workspace,
            workspace_,
            HasScope(required, CommandTargetScope::Workspace))
        || !MatchesRequired(
            context.editor_group,
            editor_group_,
            HasScope(required, CommandTargetScope::EditorGroup))) {
        return context.workspace.has_value() && context.editor_group.has_value()
            ? CommandResolveStatus::StaleTarget
            : CommandResolveStatus::MissingScope;
    }
    if (HasScope(required, CommandTargetScope::DocumentSession)
        && !context.document_session.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    if (context.document_session.has_value()
        && context.document_session.value() != document_session_) {
        return CommandResolveStatus::StaleTarget;
    }
    if (HasScope(required, CommandTargetScope::DocumentView)
        && !context.document_view.has_value()) {
        return CommandResolveStatus::MissingScope;
    }
    if (context.document_view.has_value()
        && !ContainsView(context.document_view.value())) {
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
    if (context.job.has_value() && !ContainsJob(context.job.value())) {
        return CommandResolveStatus::StaleTarget;
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
    return workspace_;
}

EditorGroupId CommandTargetRegistry::EditorGroup() const noexcept {
    return editor_group_;
}

CanvasId CommandTargetRegistry::Canvas() const noexcept {
    return canvas_;
}

DocumentSessionId CommandTargetRegistry::DocumentSession() const noexcept {
    return document_session_;
}

DocumentViewId CommandTargetRegistry::ActiveDocumentView() const noexcept {
    return active_document_view_;
}

void CommandTargetRegistry::AdvanceGeneration() noexcept {
    std::uint64_t next = generation_.Value() + 1U;
    if (next == 0U) {
        next = 1U;
    }
    generation_ = Generation(next);
}

bool CommandTargetRegistry::ContainsView(DocumentViewId view) const noexcept {
    return Contains(views_, view_count_, view);
}

bool CommandTargetRegistry::ContainsPane(PaneInstanceId pane) const noexcept {
    return Contains(panes_, pane_count_, pane);
}

bool CommandTargetRegistry::ContainsJob(JobSessionId job) const noexcept {
    return Contains(jobs_, job_count_, job);
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

DragToken FrontendTokenSource::IssueDrag(const CommandContext& context) noexcept {
    return DragToken{IssueValue(), context};
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
