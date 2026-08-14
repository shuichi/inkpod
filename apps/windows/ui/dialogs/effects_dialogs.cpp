#include "ui/ui_resources.h"

#include "ui/localization.h"

#include "effects_dialogs.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <climits>
#include <cwchar>
#include <new>
#include <utility>

#include "app/resource.h"
#include "modal_dialog_position.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT_PTR kProgressTimer = 1U;
constexpr UINT_PTR kEffectPreviewDebounceTimer = 2U;
constexpr UINT_PTR kEffectPreviewProgressTimer = 3U;
constexpr UINT kEffectEditorSmokeChange = WM_APP + 91U;

bool ReadSignedDialogValue(
    HWND dialog, int control, std::int32_t& output) noexcept {
    std::array<wchar_t, 64U> text{};
    if (GetDlgItemTextW(
            dialog, control, text.data(), static_cast<int>(text.size())) <= 0) {
        return false;
    }
    wchar_t* end{};
    errno = 0;
    const long value = std::wcstol(text.data(), &end, 10);
    if (errno == ERANGE || end == text.data() || *end != L'\0'
        || value < INT32_MIN || value > INT32_MAX) {
        return false;
    }
    output = static_cast<std::int32_t>(value);
    return true;
}

template <std::size_t Count>
void FillEffectCombo(
    HWND combo,
    const std::array<const wchar_t*, Count>& labels,
    const std::array<std::uint32_t, Count>& values,
    std::size_t count,
    std::uint32_t selected_value) noexcept {
    int selected = 0;
    for (std::size_t index = 0; index < count; ++index) {
        const LRESULT item = SendMessageW(
            combo, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(labels[index]));
        if (item == CB_ERR || item == CB_ERRSPACE) {
            continue;
        }
        SendMessageW(combo, CB_SETITEMDATA, item, static_cast<LPARAM>(values[index]));
        if (values[index] == selected_value) {
            selected = static_cast<int>(item);
        }
    }
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
    EnableWindow(combo, count != 0U ? TRUE : FALSE);
}

std::uint32_t SelectedEffectCombo(
    HWND dialog, int control, std::uint32_t fallback) noexcept {
    const LRESULT selected = SendDlgItemMessageW(
        dialog, control, CB_GETCURSEL, 0, 0);
    if (selected == CB_ERR) {
        return fallback;
    }
    const LRESULT value = SendDlgItemMessageW(
        dialog, control, CB_GETITEMDATA, static_cast<WPARAM>(selected), 0);
    return value == CB_ERR ? fallback : static_cast<std::uint32_t>(value);
}

bool ReadEffectEditorState(
    HWND dialog, EffectEditorState& state, bool show_error) noexcept {
    constexpr std::array<int, 5U> edits{
        IDC_EFFECT_PARAMETER0,
        IDC_EFFECT_PARAMETER1,
        IDC_EFFECT_PARAMETER2,
        IDC_EFFECT_PARAMETER3,
        IDC_EFFECT_PARAMETER4};
    auto parameters = state.parameters;
    for (std::size_t index = 0; index < edits.size(); ++index) {
        if (!ReadSignedDialogValue(dialog, edits[index], parameters[index])) {
            if (show_error && !state.close_immediately) {
                MessageBoxW(
                    dialog,
                    UiText(UiStringId::Text0694),
                    L"inkpod",
                    MB_OK | MB_ICONWARNING);
            }
            return false;
        }
    }
    std::array<wchar_t, 1024U> points{};
    GetDlgItemTextW(
        dialog,
        IDC_EFFECT_POINTS,
        points.data(),
        static_cast<int>(points.size()));
    try {
        state.points.assign(points.data());
    } catch (const std::bad_alloc&) {
        return false;
    }
    state.parameters = parameters;
    state.channel = SelectedEffectCombo(
        dialog, IDC_EFFECT_CHANNEL, state.channel);
    state.mode = SelectedEffectCombo(dialog, IDC_EFFECT_MODE, state.mode);
    state.option1 =
        IsDlgButtonChecked(dialog, IDC_EFFECT_OPTION1) == BST_CHECKED;
    state.option2 =
        IsDlgButtonChecked(dialog, IDC_EFFECT_OPTION2) == BST_CHECKED;
    return true;
}

void SetEffectPreviewStatus(HWND dialog, const wchar_t* text) noexcept {
    if (dialog != nullptr && IsWindow(dialog) != FALSE) {
        SetDlgItemTextW(dialog, IDC_EFFECT_PREVIEW_STATUS, text == nullptr ? L"" : text);
    }
}

bool SubmitEffectPreviewChange(
    HWND dialog, EffectEditorState& state, bool show_error) noexcept {
    if (!ReadEffectEditorState(dialog, state, show_error)) {
        SetEffectPreviewStatus(dialog, UiText(UiStringId::Text0499));
        return false;
    }
    if (state.preview_change == nullptr) {
        return true;
    }
    if (!state.preview_change(state.preview_context, state)) {
        SetEffectPreviewStatus(dialog, UiText(UiStringId::Text0315));
        return false;
    }
    SetEffectPreviewStatus(dialog, UiText(UiStringId::Text0736));
    return true;
}

bool IsEffectEditorChangeNotification(WPARAM wparam) noexcept {
    switch (LOWORD(wparam)) {
        case IDC_EFFECT_PARAMETER0:
        case IDC_EFFECT_PARAMETER1:
        case IDC_EFFECT_PARAMETER2:
        case IDC_EFFECT_PARAMETER3:
        case IDC_EFFECT_PARAMETER4:
        case IDC_EFFECT_POINTS:
            return HIWORD(wparam) == EN_CHANGE;
        case IDC_EFFECT_CHANNEL:
        case IDC_EFFECT_MODE:
            return HIWORD(wparam) == CBN_SELCHANGE;
        case IDC_EFFECT_OPTION1:
        case IDC_EFFECT_OPTION2:
            return HIWORD(wparam) == BN_CLICKED;
        default:
            return false;
    }
}

INT_PTR CALLBACK EffectEditorDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<EffectEditorState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<EffectEditorState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            SetDlgItemTextW(dialog, IDC_EFFECT_TITLE, state->title);
            constexpr std::array<int, 5U> labels{
                IDC_EFFECT_PARAMETER0_LABEL,
                IDC_EFFECT_PARAMETER1_LABEL,
                IDC_EFFECT_PARAMETER2_LABEL,
                IDC_EFFECT_PARAMETER3_LABEL,
                IDC_EFFECT_PARAMETER4_LABEL};
            constexpr std::array<int, 5U> edits{
                IDC_EFFECT_PARAMETER0,
                IDC_EFFECT_PARAMETER1,
                IDC_EFFECT_PARAMETER2,
                IDC_EFFECT_PARAMETER3,
                IDC_EFFECT_PARAMETER4};
            for (std::size_t index = 0; index < edits.size(); ++index) {
                SetDlgItemTextW(dialog, labels[index], state->parameter_labels[index]);
                std::array<wchar_t, 32U> value{};
                _snwprintf_s(
                    value.data(), value.size(), _TRUNCATE, L"%d", state->parameters[index]);
                SetDlgItemTextW(dialog, edits[index], value.data());
            }
            FillEffectCombo(
                GetDlgItem(dialog, IDC_EFFECT_CHANNEL),
                state->channel_labels,
                state->channel_values,
                state->channel_count,
                state->channel);
            FillEffectCombo(
                GetDlgItem(dialog, IDC_EFFECT_MODE),
                state->mode_labels,
                state->mode_values,
                state->mode_count,
                state->mode);
            SetDlgItemTextW(dialog, IDC_EFFECT_POINTS, state->points.c_str());
            SetDlgItemTextW(dialog, IDC_EFFECT_OPTION1, state->option1_label);
            SetDlgItemTextW(dialog, IDC_EFFECT_OPTION2, state->option2_label);
            CheckDlgButton(
                dialog, IDC_EFFECT_OPTION1, state->option1 ? BST_CHECKED : BST_UNCHECKED);
            CheckDlgButton(
                dialog, IDC_EFFECT_OPTION2, state->option2 ? BST_CHECKED : BST_UNCHECKED);
            EnableWindow(
                GetDlgItem(dialog, IDC_EFFECT_OPTION1),
                state->option1_enabled ? TRUE : FALSE);
            EnableWindow(
                GetDlgItem(dialog, IDC_EFFECT_OPTION2),
                state->option2_enabled ? TRUE : FALSE);
            state->dialog = dialog;
            SetEffectPreviewStatus(dialog, state->preview_idle_text);
            static_cast<void>(CenterModalDialogOnOwner(dialog));
            if (state->preview_change != nullptr) {
                static_cast<void>(SubmitEffectPreviewChange(dialog, *state, false));
                SetTimer(dialog, kEffectPreviewProgressTimer, 100U, nullptr);
            }
            if (state->close_immediately && state->preview_change != nullptr) {
                PostMessageW(dialog, kEffectEditorSmokeChange, 0, 0);
            } else if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                KillTimer(dialog, kEffectPreviewDebounceTimer);
                KillTimer(dialog, kEffectPreviewProgressTimer);
                if (state->preview_change != nullptr) {
                    if (!SubmitEffectPreviewChange(dialog, *state, true)) {
                        SetTimer(dialog, kEffectPreviewProgressTimer, 100U, nullptr);
                        return TRUE;
                    }
                } else if (!ReadEffectEditorState(dialog, *state, true)) {
                    return TRUE;
                }
                state->dialog = nullptr;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                KillTimer(dialog, kEffectPreviewDebounceTimer);
                KillTimer(dialog, kEffectPreviewProgressTimer);
                state->dialog = nullptr;
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            if (state->preview_change != nullptr
                && IsEffectEditorChangeNotification(wparam)) {
                KillTimer(dialog, kEffectPreviewDebounceTimer);
                SetTimer(dialog, kEffectPreviewDebounceTimer, 120U, nullptr);
                return TRUE;
            }
            break;
        case WM_TIMER:
            if (state == nullptr) {
                break;
            }
            if (wparam == kEffectPreviewDebounceTimer) {
                KillTimer(dialog, kEffectPreviewDebounceTimer);
                static_cast<void>(SubmitEffectPreviewChange(dialog, *state, false));
                return TRUE;
            }
            if (wparam == kEffectPreviewProgressTimer
                && state->preview_progress != nullptr) {
                ProgressDialogInfo info{};
                if (state->preview_progress(state->preview_context, info)) {
                    std::array<wchar_t, 96U> status{};
                    _snwprintf_s(
                        status.data(),
                        status.size(),
                        _TRUNCATE,
                        UiText(UiStringId::Text0316),
                        static_cast<unsigned long long>(info.completed_work),
                        static_cast<unsigned long long>(info.total_work));
                    SetEffectPreviewStatus(dialog, status.data());
                }
                return TRUE;
            }
            break;
        case kEffectEditorSmokeChange:
            if (state == nullptr || state->preview_change == nullptr) {
                break;
            }
            if (state->smoke_change_step < 3U) {
                constexpr std::array<std::int32_t, 3U> values{100, 250, -150};
                const std::int32_t value = values[state->smoke_change_step++];
                std::array<wchar_t, 32U> text{};
                _snwprintf_s(
                    text.data(), text.size(), _TRUNCATE, L"%d", value);
                SetDlgItemTextW(dialog, IDC_EFFECT_PARAMETER0, text.data());
                KillTimer(dialog, kEffectPreviewDebounceTimer);
                static_cast<void>(SubmitEffectPreviewChange(dialog, *state, false));
                PostMessageW(dialog, kEffectEditorSmokeChange, 0, 0);
                return TRUE;
            }
            PostMessageW(
                dialog,
                WM_COMMAND,
                state->smoke_cancel ? IDCANCEL : IDOK,
                0);
            return TRUE;
        case WM_CLOSE:
            if (state != nullptr) {
                KillTimer(dialog, kEffectPreviewDebounceTimer);
                KillTimer(dialog, kEffectPreviewProgressTimer);
                state->dialog = nullptr;
            }
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

constexpr std::array<int, 4U> kProgressTextControls{
    IDC_EFFECT_PROGRESS_TEXT,
    IDC_BATCH_PROGRESS_TEXT,
    IDC_COLOR_CHART_PROGRESS_TEXT,
    IDC_HISTORY_PROGRESS_TEXT};
constexpr std::array<int, 4U> kProgressBarControls{
    IDC_EFFECT_PROGRESS_BAR,
    IDC_BATCH_PROGRESS_BAR,
    IDC_COLOR_CHART_PROGRESS_BAR,
    IDC_HISTORY_PROGRESS_BAR};
constexpr std::array<int, 4U> kProgressCancelControls{
    IDC_EFFECT_PROGRESS_CANCEL,
    IDC_BATCH_PROGRESS_CANCEL,
    IDC_COLOR_CHART_PROGRESS_CANCEL,
    IDC_HISTORY_PROGRESS_CANCEL};
static_assert(
    kProgressTextControls.size()
    == static_cast<std::size_t>(JobProgressSlot::Count));
static_assert(kProgressBarControls.size() == kProgressTextControls.size());
static_assert(kProgressCancelControls.size() == kProgressTextControls.size());

void LayoutJobProgressPane(
    HWND dialog, const JobProgressPaneState& state) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(dialog);
    const int margin = MulDiv(8, static_cast<int>(dpi), 96);
    const int row_height = MulDiv(40, static_cast<int>(dpi), 96);
    const int text_height = MulDiv(16, static_cast<int>(dpi), 96);
    const int bar_height = MulDiv(10, static_cast<int>(dpi), 96);
    const int button_width = MulDiv(82, static_cast<int>(dpi), 96);
    const int button_height = MulDiv(24, static_cast<int>(dpi), 96);
    const int width = std::max(
        0, static_cast<int>(client.right - client.left));
    std::size_t visible_row{};
    for (std::size_t index = 0U; index < kProgressTextControls.size(); ++index) {
        if (!state.entries[index].active) {
            continue;
        }
        const int top = margin + static_cast<int>(visible_row++) * row_height;
        SetWindowPos(
            GetDlgItem(dialog, kProgressTextControls[index]),
            nullptr,
            margin,
            top,
            std::max(0, width - margin * 3 - button_width),
            text_height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
        SetWindowPos(
            GetDlgItem(dialog, kProgressCancelControls[index]),
            nullptr,
            std::max(margin, width - margin - button_width),
            top - MulDiv(3, static_cast<int>(dpi), 96),
            button_width,
            button_height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
        SetWindowPos(
            GetDlgItem(dialog, kProgressBarControls[index]),
            nullptr,
            margin,
            top + text_height,
            std::max(0, width - margin * 2),
            bar_height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
    }
    SetWindowPos(
        GetDlgItem(dialog, IDC_JOB_PROGRESS_EMPTY),
        nullptr,
        margin,
        std::max(
            margin,
            (static_cast<int>(client.bottom) - text_height) / 2),
        std::max(0, width - margin * 2),
        text_height,
        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
}

void RefreshJobProgressPane(
    HWND dialog, JobProgressPaneState& state) noexcept {
    bool any_active{};
    for (std::size_t index = 0U; index < state.entries.size(); ++index) {
        JobProgressEntry& entry = state.entries[index];
        any_active = any_active || entry.active;
        for (const int control : {
                 kProgressTextControls[index],
                 kProgressBarControls[index],
                 kProgressCancelControls[index]}) {
            ShowWindow(GetDlgItem(dialog, control), entry.active ? SW_SHOW : SW_HIDE);
        }
        if (!entry.active) {
            continue;
        }
        EnableWindow(
            GetDlgItem(dialog, kProgressCancelControls[index]),
            entry.cancelling ? FALSE : TRUE);
        if (entry.cancelling) {
            SetDlgItemTextW(
                dialog,
                kProgressTextControls[index],
                entry.progress.cancelling_text);
            continue;
        }
        ProgressDialogInfo info{};
        if (entry.progress.query == nullptr
            || !entry.progress.query(entry.progress.context, info)) {
            continue;
        }
        const std::uint64_t value = info.total_work == 0U
            ? 0U
            : std::min<std::uint64_t>(
                  1000U, info.completed_work * 1000U / info.total_work);
        SendDlgItemMessageW(
            dialog, kProgressBarControls[index], PBM_SETPOS, value, 0);
        std::array<wchar_t, 128U> text{};
        _snwprintf_s(
            text.data(),
            text.size(),
            _TRUNCATE,
            L"%ls %llu / %llu",
            entry.progress.progress_prefix,
            static_cast<unsigned long long>(info.completed_work),
            static_cast<unsigned long long>(info.total_work));
        SetDlgItemTextW(dialog, kProgressTextControls[index], text.data());
    }
    ShowWindow(
        GetDlgItem(dialog, IDC_JOB_PROGRESS_EMPTY),
        any_active ? SW_HIDE : SW_SHOW);
}

INT_PTR CALLBACK JobProgressPaneProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<JobProgressPaneState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<JobProgressPaneState*>(lparam);
            if (state == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            for (const int control : kProgressBarControls) {
                SendDlgItemMessageW(dialog, control, PBM_SETRANGE32, 0, 1000);
            }
            SetTimer(dialog, kProgressTimer, 100U, nullptr);
            LayoutJobProgressPane(dialog, *state);
            RefreshJobProgressPane(dialog, *state);
            return TRUE;
        case WM_SIZE:
            if (state != nullptr) {
                LayoutJobProgressPane(dialog, *state);
            }
            return TRUE;
        case WM_TIMER:
            if (state != nullptr && wparam == kProgressTimer) {
                RefreshJobProgressPane(dialog, *state);
                return TRUE;
            }
            break;
        case WM_COMMAND:
            if (state != nullptr) {
                const auto found = std::find(
                    kProgressCancelControls.begin(),
                    kProgressCancelControls.end(),
                    static_cast<int>(LOWORD(wparam)));
                if (found != kProgressCancelControls.end()) {
                    const std::size_t index = static_cast<std::size_t>(
                        found - kProgressCancelControls.begin());
                    JobProgressEntry& entry = state->entries[index];
                    if (entry.active && !entry.cancelling
                        && entry.progress.cancel != nullptr) {
                        entry.progress.cancel(entry.progress.context);
                        entry.cancelling = true;
                        RefreshJobProgressPane(dialog, *state);
                    }
                    return TRUE;
                }
            }
            break;
        case WM_NCDESTROY:
            KillTimer(dialog, kProgressTimer);
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

INT_PTR ShowEffectEditor(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    EffectEditorState& state) noexcept {
    try {
        EffectEditorState candidate = state;
        candidate.close_immediately = close_immediately;
        const INT_PTR result = DialogBoxLocalizedParamW(
            instance,
            MAKEINTRESOURCEW(IDD_EFFECT_EDITOR),
            owner,
            EffectEditorDialogProcedure,
            reinterpret_cast<LPARAM>(&candidate));
        if (result == IDOK) {
            state = std::move(candidate);
        }
        return result;
    } catch (const std::bad_alloc&) {
        return IDCANCEL;
    }
}

void SetEffectEditorPreviewStatus(HWND dialog, const wchar_t* text) noexcept {
    SetEffectPreviewStatus(dialog, text);
}

HWND CreateJobProgressPane(
    HINSTANCE instance, HWND parent, JobProgressPaneState& state) noexcept {
    return CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_EFFECT_PROGRESS),
        parent,
        JobProgressPaneProcedure,
        reinterpret_cast<LPARAM>(&state));
}

bool BindJobProgress(
    HWND pane,
    JobProgressPaneState& state,
    JobProgressSlot slot,
    const ProgressDialogState& progress) noexcept {
    const std::size_t index = static_cast<std::size_t>(slot);
    if (pane == nullptr || index >= state.entries.size()
        || progress.query == nullptr || progress.cancel == nullptr
        || state.entries[index].active) {
        return false;
    }
    state.entries[index] = JobProgressEntry{progress, true, false};
    LayoutJobProgressPane(pane, state);
    RefreshJobProgressPane(pane, state);
    return true;
}

void ClearJobProgress(
    HWND pane, JobProgressPaneState& state, JobProgressSlot slot) noexcept {
    const std::size_t index = static_cast<std::size_t>(slot);
    if (index >= state.entries.size()) {
        return;
    }
    state.entries[index] = {};
    if (pane != nullptr) {
        LayoutJobProgressPane(pane, state);
        RefreshJobProgressPane(pane, state);
    }
}

void ClearJobProgressIfContext(
    HWND pane,
    JobProgressPaneState& state,
    JobProgressSlot slot,
    const void* context) noexcept {
    const std::size_t index = static_cast<std::size_t>(slot);
    if (index >= state.entries.size()
        || state.entries[index].progress.context != context) {
        return;
    }
    ClearJobProgress(pane, state, slot);
}

bool HasActiveJobProgress(const JobProgressPaneState& state) noexcept {
    return std::any_of(
        state.entries.begin(), state.entries.end(), [](const JobProgressEntry& entry) {
            return entry.active;
        });
}

}  // namespace inkpod::windows::ui
