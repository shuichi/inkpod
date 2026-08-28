#include "job_progress.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cwchar>
#include <limits>
#include <string_view>

#include "ui/panes/pane_dialog_layout.h"

namespace inkpod::windows::ui {
namespace {
constexpr UINT_PTR kProgressSubclass = 0x4a505247U;
constexpr UINT_PTR kCancelSubclass = 0x4a50434eU;
constexpr UINT_PTR kProgressTimer = 0x4a50U;
constexpr int kSelector = 1;
constexpr int kBar = 2;
constexpr int kCancel = 3;

LRESULT CALLBACK StatusProgressProcedure(HWND, UINT, WPARAM, LPARAM, UINT_PTR, DWORD_PTR) noexcept;

LRESULT CALLBACK CancelButtonProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam,
    UINT_PTR subclass, DWORD_PTR data) noexcept;

JobProgressState* AttachedState(HWND status) noexcept {
    DWORD_PTR data{};
    return status != nullptr
            && GetWindowSubclass(status, StatusProgressProcedure, kProgressSubclass, &data)
        ? reinterpret_cast<JobProgressState*>(data)
        : nullptr;
}

const JobProgressItem* FindItem(
    const JobProgressState& state, JobProgressIdentity identity) noexcept {
    for (std::size_t index = 0U; index < state.item_count; ++index) {
        if (state.items[index].identity == identity) {
            return &state.items[index];
        }
    }
    return nullptr;
}

const wchar_t* TaskName(JobProgressSlot slot) noexcept {
    switch (slot) {
        case JobProgressSlot::Effect: return UiText(UiStringId::Text0806);
        case JobProgressSlot::Batch: return UiText(UiStringId::Text0255);
        case JobProgressSlot::ColorChart: return UiText(UiStringId::Text0053);
        default: return UiText(UiStringId::Text0511);
    }
}

void FormatItem(const JobProgressItem& item, std::span<wchar_t> text) noexcept {
    const wchar_t* phase{};
    switch (item.phase) {
        case JobProgressPhase::Queued: phase = UiText(UiStringId::JobStatusQueued); break;
        case JobProgressPhase::Applying: phase = UiText(UiStringId::JobStatusApplying); break;
        case JobProgressPhase::Cancelling: phase = UiText(UiStringId::Cancelling); break;
        default: break;
    }
    if (phase != nullptr) {
        _snwprintf_s(text.data(), text.size(), _TRUNCATE, L"%ls — %ls", item.name, phase);
    } else if (item.total_work == 0U) {
        _snwprintf_s(text.data(), text.size(), _TRUNCATE, L"%ls", item.name);
    } else {
        _snwprintf_s(text.data(), text.size(), _TRUNCATE,
            L"%ls %u%%", item.name, JobProgressPosition(item) / 10U);
    }
}

void SetTextIfChanged(HWND window, const wchar_t* text) noexcept {
    std::array<wchar_t, 384U> previous{};
    GetWindowTextW(window, previous.data(), static_cast<int>(previous.size()));
    if (std::wstring_view{previous.data()} != text) {
        SetWindowTextW(window, text);
    }
}

void Present(HWND status, JobProgressState& state) noexcept {
    const JobProgressItem* item = FindItem(state, state.selected);
    const bool was_visible = state.visible;
    state.visible = item != nullptr;
    if (item == nullptr) {
        KillTimer(status, kProgressTimer);
        SendMessageW(state.bar, PBM_SETMARQUEE, FALSE, 0);
        SetWindowLongPtrW(state.bar, GWL_STYLE,
            GetWindowLongPtrW(state.bar, GWL_STYLE) & ~static_cast<LONG_PTR>(PBS_MARQUEE));
        state.marquee = false;
        SendMessageW(status, SB_SETTEXTW, 5U,
            reinterpret_cast<LPARAM>(state.idle_text.data()));
        LayoutJobProgress(status);
        return;
    }
    std::array<wchar_t, 256U> description{};
    FormatItem(*item, description);
    std::array<wchar_t, 384U> label{};
    if (state.item_count > 1U) {
        _snwprintf_s(label.data(), label.size(), _TRUNCATE,
            UiText(UiStringId::JobStatusMoreFormat), description.data(), state.item_count - 1U);
    } else {
        wcscpy_s(label.data(), label.size(), description.data());
    }
    SetTextIfChanged(state.selector, label.data());
    SetTextIfChanged(state.bar, description.data());
    SetTextIfChanged(state.cancel, UiText(UiStringId::JobStatusCancel));
    EnableWindow(state.cancel,
        item->cancellable && item->phase != JobProgressPhase::Cancelling);
    const bool marquee = item->total_work == 0U || item->phase != JobProgressPhase::Running;
    if (state.marquee != marquee) {
        const LONG_PTR style = GetWindowLongPtrW(state.bar, GWL_STYLE);
        SetWindowLongPtrW(state.bar, GWL_STYLE,
            marquee ? (style | PBS_MARQUEE) : (style & ~static_cast<LONG_PTR>(PBS_MARQUEE)));
        SendMessageW(state.bar, PBM_SETMARQUEE, marquee, 50);
        state.marquee = marquee;
    }
    if (!marquee) {
        const unsigned position = JobProgressPosition(*item);
        if (SendMessageW(state.bar, PBM_GETPOS, 0, 0) != static_cast<LRESULT>(position)) {
            SendMessageW(state.bar, PBM_SETPOS, position, 0);
        }
    }
    SendMessageW(status, SB_SETTEXTW, 5U, reinterpret_cast<LPARAM>(L""));
    if (!was_visible) {
        SetTimer(status, kProgressTimer, 100U, nullptr);
    }
    LayoutJobProgress(status);
}

void ChooseJob(HWND status, JobProgressState& state) noexcept {
    const HMENU menu = CreatePopupMenu();
    if (menu == nullptr) {
        return;
    }
    // TrackPopupMenu runs a nested message loop. Preserve identities, not row
    // indices, and revalidate the subclass and selected identity afterwards.
    std::array<JobProgressIdentity, kMaximumJobProgress> identities{};
    const std::size_t count = state.item_count;
    for (std::size_t index = 0U; index < count; ++index) {
        identities[index] = state.items[index].identity;
        std::array<wchar_t, 256U> label{};
        FormatItem(state.items[index], label);
        AppendMenuW(menu, MF_STRING
                | (identities[index] == state.selected ? MF_CHECKED : MF_UNCHECKED),
            index + 1U, label.data());
    }
    RECT bounds{};
    GetWindowRect(state.selector, &bounds);
    JobProgressState* const original = &state;
    const UINT selected = TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
        bounds.left, bounds.top, status, nullptr);
    DestroyMenu(menu);
    if (selected != 0U && selected <= count && AttachedState(status) == original) {
        (void)SelectJobProgress(status, *original, identities[selected - 1U]);
    }
}

LRESULT CALLBACK CancelButtonProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam,
    UINT_PTR subclass, DWORD_PTR data) noexcept {
    auto* state = reinterpret_cast<JobProgressState*>(data);
    const HWND status = GetParent(window);
    if (state == nullptr || AttachedState(status) != state) {
        return DefSubclassProc(window, message, wparam, lparam);
    }
    const bool space_down = message == WM_KEYDOWN && wparam == VK_SPACE;
    const bool begin = message == WM_LBUTTONDOWN || message == WM_LBUTTONDBLCLK
        || message == BM_CLICK || (space_down && (lparam & (1LL << 30)) == 0);
    if (begin && IsWindowEnabled(window)) {
        // A held mouse/Space press belongs to the job visible at press time.
        // Completion may select another job before the native BN_CLICKED.
        state->cancel_target = state->selected;
        state->cancel_armed = true;
    }
    const bool end = message == WM_LBUTTONUP || message == BM_CLICK
        || (message == WM_KEYUP && wparam == VK_SPACE) || message == WM_CANCELMODE;
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(window, CancelButtonProcedure, subclass);
    }
    const LRESULT result = DefSubclassProc(window, message, wparam, lparam);
    if (end && AttachedState(status) == state) {
        state->cancel_armed = false;
    }
    return result;
}

LRESULT CALLBACK StatusProgressProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam,
    UINT_PTR subclass, DWORD_PTR data) noexcept {
    auto* state = reinterpret_cast<JobProgressState*>(data);
    if (state == nullptr) {
        return DefSubclassProc(window, message, wparam, lparam);
    }
    switch (message) {
        case WM_TIMER:
            if (wparam == kProgressTimer) {
                RefreshJobProgress(window, *state);
                return 0;
            }
            break;
        case WM_COMMAND:
            if (HIWORD(wparam) == BN_CLICKED) {
                if (reinterpret_cast<HWND>(lparam) == state->selector) {
                    ChooseJob(window, *state);
                    return 0;
                }
                if (reinterpret_cast<HWND>(lparam) == state->cancel) {
                    const auto target = state->cancel_armed ? state->cancel_target : state->selected;
                    state->cancel_armed = false;
                    (void)CancelJobProgress(window, *state, target);
                    return 0;
                }
            }
            break;
        case WM_SETFOCUS:
            if (state->visible) {
                SetFocus(state->selector);
            }
            break;
        case WM_SIZE:
        case WM_DPICHANGED_AFTERPARENT: {
            const LRESULT result = DefSubclassProc(window, message, wparam, lparam);
            LayoutJobProgress(window);
            return result;
        }
        case WM_NCDESTROY:
            KillTimer(window, kProgressTimer);
            state->selector = state->bar = state->cancel = nullptr;
            RemoveWindowSubclass(window, StatusProgressProcedure, subclass);
            break;
        default: break;
    }
    return DefSubclassProc(window, message, wparam, lparam);
}
}  // namespace

unsigned JobProgressPosition(const JobProgressItem& item) noexcept {
    if (item.total_work == 0U || item.phase != JobProgressPhase::Running) {
        return 0U;
    }
    const long double ratio = static_cast<long double>(
        std::min(item.completed_work, item.total_work)) / static_cast<long double>(item.total_work);
    return std::min(999U, static_cast<unsigned>(ratio * 1000.0L));
}

bool InitializeJobProgress(
    HWND status, JobProgressState& state,
    FileJobCancelCallback cancel_file, void* file_context) noexcept {
    if (status == nullptr || AttachedState(status) != nullptr) {
        return false;
    }
    const HINSTANCE instance = reinterpret_cast<HINSTANCE>(GetWindowLongPtrW(status, GWLP_HINSTANCE));
    const auto create = [status, instance](const wchar_t* type, int id, DWORD style) {
        return CreateWindowExW(0, type, nullptr, WS_CHILD | style,
            0, 0, 0, 0, status, reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)), instance, nullptr);
    };
    state.selector = create(L"BUTTON", kSelector, BS_PUSHBUTTON | WS_TABSTOP);
    state.bar = create(PROGRESS_CLASSW, kBar, PBS_SMOOTH);
    state.cancel = create(L"BUTTON", kCancel, BS_PUSHBUTTON | WS_TABSTOP);
    if (state.selector == nullptr || state.bar == nullptr || state.cancel == nullptr
        || !SetWindowSubclass(status, StatusProgressProcedure, kProgressSubclass,
            reinterpret_cast<DWORD_PTR>(&state))
        || !SetWindowSubclass(state.cancel, CancelButtonProcedure, kCancelSubclass,
            reinterpret_cast<DWORD_PTR>(&state))) {
        RemoveWindowSubclass(status, StatusProgressProcedure, kProgressSubclass);
        for (HWND child : {state.selector, state.bar, state.cancel}) {
            if (child != nullptr) {
                DestroyWindow(child);
            }
        }
        state.selector = state.bar = state.cancel = nullptr;
        return false;
    }
    state.cancel_file = cancel_file;
    state.file_context = file_context;
    panes::EnablePaneDialogResizePainting(status);
    SetWindowLongPtrW(status, GWL_EXSTYLE, GetWindowLongPtrW(status, GWL_EXSTYLE) | WS_EX_CONTROLPARENT);
    const WPARAM font = SendMessageW(status, WM_GETFONT, 0, 0);
    for (HWND child : {state.selector, state.bar, state.cancel}) {
        SendMessageW(child, WM_SETFONT, font, FALSE);
    }
    SendMessageW(state.bar, PBM_SETRANGE32, 0, 1000);
    LayoutJobProgress(status);
    return true;
}

bool BindJobProgress(HWND status, JobProgressState& state,
    JobProgressSlot slot, const ProgressDialogState& progress) noexcept {
    if (AttachedState(status) != &state || slot >= JobProgressSlot::Count
        || progress.query == nullptr || progress.cancel == nullptr
        || state.next_generation == std::numeric_limits<std::uint64_t>::max()) {
        return false;
    }
    JobProgressEntry* vacant{};
    for (auto& entry : state.entries) {
        if (entry.active && entry.slot == slot && entry.progress.context == progress.context) {
            return false;
        }
        if (!entry.active && vacant == nullptr) {
            vacant = &entry;
        }
    }
    if (vacant == nullptr) {
        return false;
    }
    *vacant = {progress, slot, state.next_generation++, true, false};
    RefreshJobProgress(status, state);
    return true;
}

void ClearJobProgressIfContext(HWND status, JobProgressState& state,
    JobProgressSlot slot, const void* context) noexcept {
    for (auto& entry : state.entries) {
        if (entry.active && entry.slot == slot && entry.progress.context == context) {
            entry = {};
            RefreshJobProgress(status, state);
            return;
        }
    }
}

bool HasActiveJobProgress(const JobProgressState& state) noexcept {
    return state.file_count != 0U || std::any_of(state.entries.begin(), state.entries.end(),
        [](const JobProgressEntry& entry) { return entry.active; });
}

bool SetFileJobProgress(HWND status, JobProgressState& state,
    std::span<const JobProgressItem> items) noexcept {
    if (AttachedState(status) != &state || items.size() > state.file_items.size()) {
        return false;
    }
    for (std::size_t index = 0U; index < items.size(); ++index) {
        if (items[index].identity.source != JobProgressSource::FileIo
            || items[index].identity.id == 0U || items[index].name == nullptr) {
            return false;
        }
        for (std::size_t prior = 0U; prior < index; ++prior) {
            if (items[index].identity == items[prior].identity) {
                return false;
            }
        }
    }
    // Preserve an accepted cancellation across an older cached poll result.
    std::array<JobProgressItem, kMaximumFileJobProgress> next{};
    std::copy(items.begin(), items.end(), next.begin());
    for (std::size_t index = 0U; index < items.size(); ++index) {
        const auto* previous = FindItem(state, items[index].identity);
        if (previous != nullptr && previous->phase == JobProgressPhase::Cancelling) {
            next[index].phase = JobProgressPhase::Cancelling;
        }
    }
    state.file_items = next;
    state.file_count = items.size();
    RefreshJobProgress(status, state);
    return true;
}

void RefreshJobProgress(HWND status, JobProgressState& state) noexcept {
    if (AttachedState(status) != &state || state.refreshing) {
        return;
    }
    state.refreshing = true;
    state.item_count = 0U;
    for (std::size_t index = 0U; index < state.entries.size(); ++index) {
        const auto& entry = state.entries[index];
        if (!entry.active) {
            continue;
        }
        ProgressDialogInfo info{};
        const bool queried = entry.progress.query(entry.progress.context, info);
        JobProgressItem item{
            {JobProgressSource::Task, index, entry.generation},
            entry.progress.title != nullptr ? entry.progress.title : TaskName(entry.slot),
            info.completed_work, info.total_work};
        if (entry.cancelling) {
            item.phase = JobProgressPhase::Cancelling;
        } else if (!queried || (info.total_work != 0U && info.completed_work >= info.total_work)) {
            item.phase = JobProgressPhase::Applying;
        }
        state.items[state.item_count++] = item;
    }
    for (std::size_t index = 0U; index < state.file_count; ++index) {
        state.items[state.item_count++] = state.file_items[index];
    }
    if (FindItem(state, state.selected) == nullptr) {
        state.selected = state.item_count == 0U ? JobProgressIdentity{} : state.items[0].identity;
    }
    Present(status, state);
    state.refreshing = false;
}

bool SelectJobProgress(HWND status, JobProgressState& state, JobProgressIdentity identity) noexcept {
    if (AttachedState(status) != &state || FindItem(state, identity) == nullptr) {
        return false;
    }
    state.selected = identity;
    Present(status, state);
    return true;
}

bool CancelJobProgress(HWND status, JobProgressState& state, JobProgressIdentity identity) noexcept {
    if (AttachedState(status) != &state) {
        return false;
    }
    const JobProgressItem* item = FindItem(state, identity);
    if (item == nullptr || !item->cancellable || item->phase == JobProgressPhase::Cancelling) {
        return false;
    }
    if (identity.source == JobProgressSource::Task) {
        if (identity.id >= state.entries.size()) {
            return false;
        }
        auto& entry = state.entries[static_cast<std::size_t>(identity.id)];
        if (!entry.active || entry.generation != identity.generation || entry.progress.cancel == nullptr) {
            return false;
        }
        entry.cancelling = true;
        const auto callback = entry.progress.cancel;
        void* const context = entry.progress.context;
        callback(context);
    } else if (identity.source == JobProgressSource::FileIo && state.cancel_file != nullptr) {
        for (std::size_t index = 0U; index < state.file_count; ++index) {
            if (state.file_items[index].identity == identity) {
                state.file_items[index].phase = JobProgressPhase::Cancelling;
            }
        }
        state.cancel_file(state.file_context, identity.id);
    } else {
        return false;
    }
    if (AttachedState(status) == &state) {
        RefreshJobProgress(status, state);
    }
    return true;
}

void LayoutJobProgress(HWND status) noexcept {
    if (status == nullptr) {
        return;
    }
    RECT client{};
    if (!GetClientRect(status, &client)) {
        return;
    }
    JobProgressState* state = AttachedState(status);
    const bool visible = state != nullptr && state->visible;
    const int width = std::max(0L, client.right);
    const int dpi = static_cast<int>(GetDpiForWindow(status));
    const auto dip = [dpi](int value) { return MulDiv(value, dpi == 0 ? 96 : dpi, 96); };
    const int reserved = visible
        ? std::clamp(width * 45 / 100, std::min(width, dip(320)), std::min(width, dip(480))) : 0;
    const int remaining = width - reserved;
    const std::array<int, 6U> parts = visible
        ? std::array<int, 6U>{remaining * 23 / 100, remaining * 41 / 100,
            remaining * 62 / 100, remaining * 84 / 100, remaining, -1}
        : std::array<int, 6U>{width * 20 / 100, width * 33 / 100,
            width * 47 / 100, width * 64 / 100, width * 81 / 100, -1};
    std::array<int, 6U> previous{};
    bool changed = SendMessageW(status, SB_GETPARTS, previous.size(),
        reinterpret_cast<LPARAM>(previous.data())) != static_cast<LRESULT>(previous.size()) || previous != parts;
    if (changed) {
        SendMessageW(status, SB_SETPARTS, parts.size(), reinterpret_cast<LPARAM>(parts.data()));
    }
    if (state == nullptr) {
        return;
    }
    const int gap = dip(4);
    const int left = remaining + gap;
    const int usable = std::max(0, reserved - dip(20) - gap * 2);
    const int height = std::max(0L, client.bottom - dip(4));
    const int cancel_width = std::min(dip(70), usable / 4);
    const int bar_width = std::min(dip(110), usable / 3);
    const int label_width = std::max(0, usable - cancel_width - bar_width - gap * 2);
    struct Placement { HWND child; int id; int x; int y; int width; int height; };
    const std::array<Placement, 3U> placements{{
        {state->selector, kSelector, left, dip(2), label_width, height},
        {state->bar, kBar, left + label_width + gap,
            dip(2) + (height - std::min(height, dip(12))) / 2, bar_width, std::min(height, dip(12))},
        {state->cancel, kCancel, left + label_width + gap * 2 + bar_width,
            dip(2), cancel_width, height},
    }};
    for (const auto& placement : placements) {
        changed = changed || !panes::PaneWindowHasBounds(placement.child,
            placement.x, placement.y, placement.width, placement.height);
        panes::PlacePaneDialogControl(status, placement.id, placement.x, placement.y,
            placement.width, placement.height, false);
        const bool was_visible = (GetWindowLongPtrW(placement.child, GWL_STYLE) & WS_VISIBLE) != 0;
        if (was_visible != visible) {
            changed = true;
            SetWindowPos(placement.child, nullptr, 0, 0, 0, 0,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE | SWP_NOREDRAW
                    | (visible ? SWP_SHOWWINDOW : SWP_HIDEWINDOW));
        }
    }
    if (changed) {
        panes::CompletePaneDialogResize(status);
    }
}

void SetJobProgressIdleText(HWND status, const wchar_t* text) noexcept {
    if (status == nullptr || text == nullptr) {
        return;
    }
    auto* state = AttachedState(status);
    if (state != nullptr) {
        wcsncpy_s(state->idle_text.data(), state->idle_text.size(), text, _TRUNCATE);
    }
    SendMessageW(status, SB_SETTEXTW, 5U,
        reinterpret_cast<LPARAM>(state != nullptr && state->visible ? L"" : text));
}
}  // namespace inkpod::windows::ui
