#include <windows.h>
#include <commctrl.h>

#include <array>
#include <algorithm>
#include <cstdio>
#include <limits>
#include <string_view>

#include "ui/job_progress.h"

namespace {
using namespace inkpod::windows::ui;

struct TaskFixture {
    std::uint64_t completed{3U};
    std::uint64_t total{10U};
    unsigned cancelled{};
};

bool Query(void* context, ProgressDialogInfo& output) noexcept {
    const auto& fixture = *static_cast<TaskFixture*>(context);
    output = {fixture.completed, fixture.total};
    return true;
}

void Cancel(void* context) noexcept {
    ++static_cast<TaskFixture*>(context)->cancelled;
}

void CancelFile(void* context, std::uint64_t request) noexcept {
    *static_cast<std::uint64_t*>(context) = request;
}

const JobProgressEntry* FindTask(const JobProgressState& state, const void* context) {
    const auto found = std::find_if(state.entries.begin(), state.entries.end(),
        [context](const auto& entry) { return entry.active && entry.progress.context == context; });
    return found == state.entries.end() ? nullptr : &*found;
}

bool Verify(HWND status, JobProgressState& state) {
    std::uint64_t cancelled_file{};
    if (!InitializeJobProgress(status, state, CancelFile, &cancelled_file)) {
        return false;
    }
    SetJobProgressIdleText(status, L"Unmodified");
    if (state.visible || state.item_count != 0U) {
        return false;
    }
    TaskFixture fixture;
    ProgressDialogState task{&fixture, Query, Cancel, L"Effect"};
    if (!BindJobProgress(status, state, JobProgressSlot::Effect, task)
        || BindJobProgress(status, state, JobProgressSlot::Effect, task)
        || !state.visible || state.item_count != 1U
        || JobProgressPosition(state.items[0]) != 300U
        || SendMessageW(state.bar, PBM_GETPOS, 0, 0) != 300) {
        return false;
    }
    const JobProgressIdentity old_task = state.selected;
    const std::array<JobProgressItem, 2U> files{{
        {{JobProgressSource::FileIo, 10U, 1U}, L"Read", 1U, 4U},
        {{JobProgressSource::FileIo, 11U, 1U}, L"Save", 0U, 0U,
         JobProgressPhase::Applying},
    }};
    if (!SetFileJobProgress(status, state, files) || state.item_count != 3U
        || state.selected != old_task
        || !SelectJobProgress(status, state, files[1].identity)
        || !state.marquee
        || !CancelJobProgress(status, state, files[1].identity)
        || cancelled_file != 11U
        || CancelJobProgress(status, state, files[1].identity)) {
        return false;
    }
    if (!CancelJobProgress(status, state, old_task) || fixture.cancelled != 1U
        || CancelJobProgress(status, state, old_task)) {
        return false;
    }
    ClearJobProgressIfContext(status, state, JobProgressSlot::Effect, &fixture);
    if (!BindJobProgress(status, state, JobProgressSlot::Effect, task)
        || CancelJobProgress(status, state, old_task) || fixture.cancelled != 1U) {
        return false;
    }
    const auto* current = FindTask(state, &fixture);
    ClearJobProgressIfContext(status, state, JobProgressSlot::Effect, &cancelled_file);
    if (current == nullptr || !current->active || current->generation == old_task.generation) {
        return false;
    }
    JobProgressItem huge = files[0];
    huge.completed_work = std::numeric_limits<std::uint64_t>::max() - 1U;
    huge.total_work = std::numeric_limits<std::uint64_t>::max();
    if (JobProgressPosition(huge) > 999U) {
        return false;
    }
    ClearJobProgressIfContext(status, state, JobProgressSlot::Effect, &fixture);
    if (!SetFileJobProgress(status, state, {}) || state.visible || state.item_count != 0U
        || (GetWindowLongPtrW(state.bar, GWL_STYLE) & (WS_VISIBLE | PBS_MARQUEE)) != 0) {
        return false;
    }
    std::array<wchar_t, 256U> idle{};
    SendMessageW(status, SB_GETTEXTW, 5U, reinterpret_cast<LPARAM>(idle.data()));
    if (std::wstring_view{idle.data()} != L"Unmodified") {
        return false;
    }

    // Completing the displayed job during a held native click/Space must not
    // cancel the next job selected by the refresh.
    for (const bool keyboard : {false, true}) {
        cancelled_file = 0U;
        if (!SetFileJobProgress(status, state, files)
            || !SelectJobProgress(status, state, files[0].identity)) {
            return false;
        }
        SendMessageW(state.cancel, keyboard ? WM_KEYDOWN : WM_LBUTTONDOWN,
            keyboard ? VK_SPACE : MK_LBUTTON, keyboard ? 1 : MAKELPARAM(3, 3));
        if (!state.cancel_armed || state.cancel_target != files[0].identity
            || !SetFileJobProgress(status, state, std::span{files}.subspan(1U))) {
            return false;
        }
        SendMessageW(state.cancel, keyboard ? WM_KEYUP : WM_LBUTTONUP,
            keyboard ? VK_SPACE : 0, keyboard ? 1 : MAKELPARAM(3, 3));
        if (cancelled_file != 0U || state.cancel_armed
            || state.selected != files[1].identity) {
            return false;
        }
        SendMessageW(state.cancel, BM_CLICK, 0, 0);
        if (cancelled_file != files[1].identity.id || IsWindowEnabled(state.cancel)
            || !SetFileJobProgress(status, state, {})) {
            return false;
        }
    }

    // A determinate job after hiding a marquee must restore the native style.
    if (!BindJobProgress(status, state, JobProgressSlot::Effect, task)
        || state.marquee || (GetWindowLongPtrW(state.bar, GWL_STYLE) & PBS_MARQUEE) != 0
        || SendMessageW(state.bar, PBM_GETPOS, 0, 0) != 300) {
        return false;
    }
    fixture.completed = fixture.total;
    RefreshJobProgress(status, state);
    if (!state.marquee || state.items[0].phase != JobProgressPhase::Applying) {
        return false;
    }
    ClearJobProgressIfContext(status, state, JobProgressSlot::Effect, &fixture);

    // The same task kind may be registered by multiple document controllers.
    std::array<TaskFixture, kMaximumTaskProgress> concurrent{};
    for (auto& entry : concurrent) {
        ProgressDialogState progress{&entry, Query, Cancel, L"History"};
        if (!BindJobProgress(status, state, JobProgressSlot::HistoryVisualization, progress)) {
            return false;
        }
    }
    ProgressDialogState overflow{&fixture, Query, Cancel, L"History"};
    if (state.item_count != concurrent.size()
        || BindJobProgress(status, state, JobProgressSlot::HistoryVisualization, overflow)
        || !CancelJobProgress(status, state, state.items[1].identity)
        || concurrent[0].cancelled != 0U || concurrent[1].cancelled != 1U) {
        return false;
    }
    const auto selected_before = state.selected;
    ClearJobProgressIfContext(status, state, JobProgressSlot::HistoryVisualization, &concurrent[1]);
    if (state.item_count != concurrent.size() - 1U || state.selected != selected_before) {
        return false;
    }
    for (auto& entry : concurrent) {
        ClearJobProgressIfContext(status, state, JobProgressSlot::HistoryVisualization, &entry);
    }
    return !HasActiveJobProgress(state) && !state.visible;
}
}  // namespace

int main() {
    INITCOMMONCONTROLSEX controls{sizeof(controls), ICC_BAR_CLASSES | ICC_PROGRESS_CLASS};
    if (!InitCommonControlsEx(&controls)) {
        return 1;
    }
    const HWND owner = CreateWindowExW(0, L"STATIC", L"Job progress test",
        WS_OVERLAPPEDWINDOW, 0, 0, 900, 300, nullptr, nullptr, GetModuleHandleW(nullptr), nullptr);
    const HWND status = CreateWindowExW(0, STATUSCLASSNAMEW, nullptr,
        WS_CHILD | WS_VISIBLE | SBARS_SIZEGRIP, 0, 0, 900, 26,
        owner, nullptr, GetModuleHandleW(nullptr), nullptr);
    JobProgressState state;
    const bool passed = owner != nullptr && status != nullptr && Verify(status, state);
    if (owner != nullptr) {
        DestroyWindow(owner);
    }
    if (!passed) {
        std::fputs("status job progress contract failed\n", stderr);
        return 1;
    }
    return 0;
}
