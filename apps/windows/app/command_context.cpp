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
    active_document_ = {};
    documents_.fill({});
    document_count_ = 0U;
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
    documents_.fill({});
    document_count_ = 1U;
    DocumentTarget& target = documents_[0];
    target.document = Issue<DocumentSessionId>();
    target.view_count = 1U;
    target.views[0] = Issue<DocumentViewId>();
    target.active_view = target.views[0];
    active_document_ = target.document;
    jobs_.fill({});
    job_count_ = 0U;
    return active_document_;
}

std::optional<DocumentSessionId> CommandTargetRegistry::AddDocument() noexcept {
    Initialize();
    if (document_count_ >= documents_.size()) {
        return std::nullopt;
    }
    DocumentTarget& target = documents_[document_count_++];
    target = {};
    target.document = Issue<DocumentSessionId>();
    target.view_count = 1U;
    target.views[0] = Issue<DocumentViewId>();
    target.active_view = target.views[0];
    active_document_ = target.document;
    return target.document;
}

bool CommandTargetRegistry::ActivateDocument(
    DocumentSessionId document,
    DocumentViewId view) noexcept {
    DocumentTarget* target = FindDocument(document);
    if (target == nullptr || !Contains(target->views, target->view_count, view)) {
        return false;
    }
    target->active_view = view;
    active_document_ = document;
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
        active_document_ = document_count_ == 0U
            ? DocumentSessionId{}
            : documents_[0].document;
    }
    return true;
}

std::optional<DocumentViewId> CommandTargetRegistry::AddDocumentView() noexcept {
    DocumentTarget* target = FindDocument(active_document_);
    if (target == nullptr || target->view_count >= target->views.size()) {
        return std::nullopt;
    }
    const DocumentViewId view = Issue<DocumentViewId>();
    target->views[target->view_count++] = view;
    target->active_view = view;
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
    if (target == nullptr || !Remove(target->views, target->view_count, view)) {
        return false;
    }
    if (target->active_view == view) {
        target->active_view = target->view_count == 0U
            ? DocumentViewId{}
            : target->views[0];
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
    if (!active_document_ || job_count_ >= jobs_.size()) {
        return std::nullopt;
    }
    const JobSessionId job = Issue<JobSessionId>();
    jobs_[job_count_++] = JobTarget{job, active_document_};
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
    if (workspace_) {
        context.workspace = workspace_;
    }
    if (editor_group_) {
        context.editor_group = editor_group_;
    }
    const DocumentTarget* document = FindDocument(active_document_);
    if (document != nullptr) {
        context.document_session = document->document;
        if (document->active_view) {
            context.document_view = document->active_view;
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
                context.document_view.value()))) {
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
    return workspace_;
}

EditorGroupId CommandTargetRegistry::EditorGroup() const noexcept {
    return editor_group_;
}

CanvasId CommandTargetRegistry::Canvas() const noexcept {
    return canvas_;
}

DocumentSessionId CommandTargetRegistry::DocumentSession() const noexcept {
    return active_document_;
}

DocumentViewId CommandTargetRegistry::ActiveDocumentView() const noexcept {
    const DocumentTarget* document = FindDocument(active_document_);
    return document == nullptr ? DocumentViewId{} : document->active_view;
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
