#include "pane_target.h"

#include <algorithm>
#include <utility>

namespace inkpod::app {
namespace {

PaneTargetStatus ResolveStatus(CommandResolveStatus status) noexcept {
    switch (status) {
        case CommandResolveStatus::Ok:
            return PaneTargetStatus::Ok;
        case CommandResolveStatus::MissingScope:
            return PaneTargetStatus::MissingTarget;
        case CommandResolveStatus::UnknownCommand:
        case CommandResolveStatus::StaleGeneration:
        case CommandResolveStatus::StaleTarget:
            return PaneTargetStatus::StaleTarget;
    }
    return PaneTargetStatus::StaleTarget;
}

CommandContext ApplicationContext(
    PaneInstanceId pane,
    const CommandContext& active) noexcept {
    CommandContext result = active;
    result.pane = pane;
    result.job.reset();
    return result;
}

CommandContext FollowContext(
    PaneInstanceId pane,
    const CommandContext& active) noexcept {
    CommandContext result = active;
    result.pane = pane;
    result.job.reset();
    return result;
}

}  // namespace

PaneTargetStatus PaneTargetRegistry::Register(
    PaneInstanceId pane,
    PaneTargetPolicy policy) noexcept {
    if (!pane || policy == PaneTargetPolicy::PinnedDocument
        || policy == PaneTargetPolicy::Job) {
        return PaneTargetStatus::InvalidPolicy;
    }
    if (FindMutable(pane) != nullptr) {
        return PaneTargetStatus::NoOp;
    }
    if (count_ >= bindings_.size()) {
        return PaneTargetStatus::MissingTarget;
    }
    bindings_[count_++] = PaneTargetBinding{pane, policy};
    return PaneTargetStatus::Ok;
}

PaneTargetStatus PaneTargetRegistry::Unregister(PaneInstanceId pane) noexcept {
    for (std::size_t index = 0U; index < count_; ++index) {
        if (bindings_[index].pane != pane) {
            continue;
        }
        for (std::size_t tail = index + 1U; tail < count_; ++tail) {
            bindings_[tail - 1U] = bindings_[tail];
        }
        bindings_[--count_] = {};
        return PaneTargetStatus::Ok;
    }
    return PaneTargetStatus::UnknownPane;
}

PaneTargetStatus PaneTargetRegistry::FollowActive(PaneInstanceId pane) noexcept {
    PaneTargetBinding* binding = FindMutable(pane);
    if (binding == nullptr) {
        return PaneTargetStatus::UnknownPane;
    }
    if (binding->policy == PaneTargetPolicy::FollowActiveView
        && !binding->fixed_context.document_session.has_value()
        && !binding->fixed_context.job.has_value()) {
        return PaneTargetStatus::NoOp;
    }
    binding->policy = PaneTargetPolicy::FollowActiveView;
    binding->fixed_context = {};
    return PaneTargetStatus::Ok;
}

PaneTargetStatus PaneTargetRegistry::PinDocument(
    PaneInstanceId pane,
    const CommandContext& target,
    const CommandTargetRegistry& targets) noexcept {
    PaneTargetBinding* binding = FindMutable(pane);
    if (binding == nullptr) {
        return PaneTargetStatus::UnknownPane;
    }
    const CommandResolveStatus resolved = targets.Resolve(
        target, kDocumentViewCommandScope);
    if (resolved != CommandResolveStatus::Ok) {
        return ResolveStatus(resolved);
    }
    binding->policy = PaneTargetPolicy::PinnedDocument;
    binding->fixed_context = FollowContext(pane, target);
    return PaneTargetStatus::Ok;
}

PaneTargetStatus PaneTargetRegistry::BindJob(
    PaneInstanceId pane,
    const CommandContext& target,
    const CommandTargetRegistry& targets) noexcept {
    PaneTargetBinding* binding = FindMutable(pane);
    if (binding == nullptr) {
        return PaneTargetStatus::UnknownPane;
    }
    const CommandTargetScope required = CommandTargetScope::Pane
        | CommandTargetScope::Job | CommandTargetScope::DocumentSession;
    const CommandResolveStatus resolved = targets.Resolve(target, required);
    if (resolved != CommandResolveStatus::Ok || target.pane != pane) {
        return resolved == CommandResolveStatus::Ok
            ? PaneTargetStatus::StaleTarget
            : ResolveStatus(resolved);
    }
    binding->policy = PaneTargetPolicy::Job;
    binding->fixed_context = target;
    return PaneTargetStatus::Ok;
}

PaneActionTarget PaneTargetRegistry::CaptureAction(
    PaneInstanceId pane,
    const CommandContext& active,
    const CommandTargetRegistry& targets) const noexcept {
    const PaneTargetBinding* binding = Find(pane);
    if (binding == nullptr) {
        return {};
    }
    PaneActionTarget result{};
    result.policy = binding->policy;
    CommandTargetScope required{};
    switch (binding->policy) {
        case PaneTargetPolicy::Application:
            result.context = ApplicationContext(pane, active);
            required = CommandTargetScope::Workspace | CommandTargetScope::Pane;
            break;
        case PaneTargetPolicy::FollowActiveView:
            result.context = FollowContext(pane, active);
            required = kDocumentViewCommandScope | CommandTargetScope::Pane;
            break;
        case PaneTargetPolicy::PinnedDocument:
            result.context = binding->fixed_context;
            required = kDocumentViewCommandScope | CommandTargetScope::Pane;
            break;
        case PaneTargetPolicy::Job:
            result.context = binding->fixed_context;
            required = CommandTargetScope::Pane | CommandTargetScope::Job
                | CommandTargetScope::DocumentSession;
            break;
    }
    result.status = ResolveStatus(targets.Resolve(result.context, required));
    return result;
}

void PaneTargetRegistry::DocumentClosed(DocumentSessionId document) noexcept {
    for (std::size_t index = 0U; index < count_; ++index) {
        PaneTargetBinding& binding = bindings_[index];
        if (binding.policy != PaneTargetPolicy::PinnedDocument
            || binding.fixed_context.document_session != document) {
            continue;
        }
        binding.policy = PaneTargetPolicy::FollowActiveView;
        binding.fixed_context = {};
        SetNotice(binding, PaneTargetNotice::PinnedDocumentClosed);
    }
}

void PaneTargetRegistry::JobClosed(JobSessionId job) noexcept {
    for (std::size_t index = 0U; index < count_; ++index) {
        PaneTargetBinding& binding = bindings_[index];
        if (binding.policy != PaneTargetPolicy::Job
            || binding.fixed_context.job != job) {
            continue;
        }
        binding.fixed_context = {};
        SetNotice(binding, PaneTargetNotice::JobClosed);
    }
}

PaneTargetNotice PaneTargetRegistry::ConsumeNotice(
    PaneInstanceId pane,
    std::uint64_t& sequence) noexcept {
    PaneTargetBinding* binding = FindMutable(pane);
    if (binding == nullptr || binding->pending_notice == PaneTargetNotice::None) {
        return PaneTargetNotice::None;
    }
    sequence = binding->notice_sequence;
    const PaneTargetNotice notice = binding->pending_notice;
    binding->pending_notice = PaneTargetNotice::None;
    return notice;
}

const PaneTargetBinding* PaneTargetRegistry::Find(PaneInstanceId pane) const noexcept {
    const auto end = bindings_.begin() + static_cast<std::ptrdiff_t>(count_);
    const auto found = std::find_if(
        bindings_.begin(), end, [pane](const PaneTargetBinding& binding) {
            return binding.pane == pane;
        });
    return found == end ? nullptr : &*found;
}

PaneTargetBinding* PaneTargetRegistry::FindMutable(PaneInstanceId pane) noexcept {
    return const_cast<PaneTargetBinding*>(
        std::as_const(*this).Find(pane));
}

void PaneTargetRegistry::SetNotice(
    PaneTargetBinding& binding,
    PaneTargetNotice notice) noexcept {
    binding.pending_notice = notice;
    ++binding.notice_sequence;
    if (binding.notice_sequence == 0U) {
        binding.notice_sequence = 1U;
    }
}

}  // namespace inkpod::app
