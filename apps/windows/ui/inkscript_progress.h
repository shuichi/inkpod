#pragma once

#include <utility>

#include "app/core_host.h"
#include "job_progress.h"

namespace inkpod::windows::ui {

// UI-thread presentation of a private engine job. It owns no source, Core or
// task handle. Cached immutable notifications feed the common status controls.
class InkScriptProgressPresentation final {
public:
    InkScriptProgressPresentation(app::CoreHost& host, HWND status, JobProgressState& state) noexcept
        : host_(host), status_(status), state_(state) {}
    ~InkScriptProgressPresentation() {
        if (job_ != 0U) {
            (void)host_.CancelInkScript(job_, context_);
        }
        Clear();
    }
    InkScriptProgressPresentation(const InkScriptProgressPresentation&) = delete;
    InkScriptProgressPresentation& operator=(const InkScriptProgressPresentation&) = delete;

    [[nodiscard]] bool Submit(app::InkScriptEngineRequest request) noexcept {
        if (job_ != 0U) {
            return false;
        }
        job_ = request.job_id;
        context_ = request.context;
        progress_ = {};
        ProgressDialogState binding{this, &Query, &Cancel, L"InkScript"};
        if (!BindJobProgress(status_, state_, JobProgressSlot::InkScript, binding)
            || !host_.EnqueueInkScript(std::move(request))) {
            Clear();
            return false;
        }
        return true;
    }

    [[nodiscard]] bool Observe(const app::CoreNotification& notification) noexcept {
        if (job_ == 0U || notification.kind != app::CoreNotificationKind::InkScript
            || notification.context != context_ || notification.inkscript.job_id != job_) {
            return false;
        }
        const auto& result = notification.inkscript;
        if (result.kind == app::InkScriptEngineNotificationKind::Completed) {
            Clear();
        } else {
            progress_.completed_work = result.completed_items;
            progress_.total_work = result.phase == app::InkScriptEnginePhase::Running
                ? result.total_items : 0U;
            RefreshJobProgress(status_, state_);
        }
        return true;
    }

    void Clear() noexcept {
        ClearJobProgressIfContext(status_, state_, JobProgressSlot::InkScript, this);
        job_ = 0U;
        context_ = {};
        progress_ = {};
    }

private:
    static bool Query(void* value, ProgressDialogInfo& output) noexcept {
        const auto& self = *static_cast<InkScriptProgressPresentation*>(value);
        InkpodTaskInfo task{};
        if (self.host_.QueryInkScriptProgress(self.job_, self.context_, task)) {
            output = {task.completed_work, task.total_work};
        } else {
            output = self.progress_;
        }
        return self.job_ != 0U;
    }
    static void Cancel(void* value) noexcept {
        auto& self = *static_cast<InkScriptProgressPresentation*>(value);
        (void)self.host_.CancelInkScript(self.job_, self.context_);
    }
    app::CoreHost& host_;
    HWND status_{};
    JobProgressState& state_;
    std::uint64_t job_{};
    app::CommandContext context_;
    ProgressDialogInfo progress_;
};

}  // namespace inkpod::windows::ui
