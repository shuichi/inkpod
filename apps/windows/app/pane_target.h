#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

#include "command_context.h"

namespace inkpod::app {

enum class PaneTargetPolicy : std::uint8_t {
    Application,
    FollowActiveView,
    PinnedDocument,
    Job,
};

enum class PaneTargetStatus : std::uint8_t {
    Ok,
    NoOp,
    UnknownPane,
    InvalidPolicy,
    MissingTarget,
    StaleTarget,
};

enum class PaneTargetNotice : std::uint8_t {
    None,
    PinnedDocumentClosed,
    JobClosed,
};

struct PaneTargetBinding final {
    PaneInstanceId pane{};
    PaneTargetPolicy policy{PaneTargetPolicy::FollowActiveView};
    CommandContext fixed_context{};
    PaneTargetNotice pending_notice{PaneTargetNotice::None};
    std::uint64_t notice_sequence{};
};

struct PaneActionTarget final {
    PaneTargetStatus status{PaneTargetStatus::UnknownPane};
    PaneTargetPolicy policy{PaneTargetPolicy::FollowActiveView};
    CommandContext context{};
};

// UI/Input-thread-owned policy registry. It stores only strong IDs and value
// contexts. Capturing an action never looks up an HWND or a C++ object and a
// stale fixed target is never replaced by whichever document is active later.
class PaneTargetRegistry final {
public:
    [[nodiscard]] PaneTargetStatus Register(
        PaneInstanceId pane,
        PaneTargetPolicy policy) noexcept;
    [[nodiscard]] PaneTargetStatus Unregister(PaneInstanceId pane) noexcept;

    [[nodiscard]] PaneTargetStatus FollowActive(PaneInstanceId pane) noexcept;
    [[nodiscard]] PaneTargetStatus PinDocument(
        PaneInstanceId pane,
        const CommandContext& target,
        const CommandTargetRegistry& targets) noexcept;
    [[nodiscard]] PaneTargetStatus BindJob(
        PaneInstanceId pane,
        const CommandContext& target,
        const CommandTargetRegistry& targets) noexcept;

    [[nodiscard]] PaneActionTarget CaptureAction(
        PaneInstanceId pane,
        const CommandContext& active,
        const CommandTargetRegistry& targets) const noexcept;

    void DocumentClosed(DocumentSessionId document) noexcept;
    void JobClosed(JobSessionId job) noexcept;
    [[nodiscard]] PaneTargetNotice ConsumeNotice(
        PaneInstanceId pane,
        std::uint64_t& sequence) noexcept;

    [[nodiscard]] const PaneTargetBinding* Find(
        PaneInstanceId pane) const noexcept;
    [[nodiscard]] std::size_t Count() const noexcept { return count_; }

private:
    static constexpr std::size_t kMaximumPaneTargets = 32U;

    [[nodiscard]] PaneTargetBinding* FindMutable(PaneInstanceId pane) noexcept;
    static void SetNotice(
        PaneTargetBinding& binding,
        PaneTargetNotice notice) noexcept;

    std::array<PaneTargetBinding, kMaximumPaneTargets> bindings_{};
    std::size_t count_{};
};

}  // namespace inkpod::app
