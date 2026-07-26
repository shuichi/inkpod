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

namespace inkpod::windows::ui {
namespace {

constexpr UINT_PTR kProgressTimer = 1U;

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
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                constexpr std::array<int, 5U> edits{
                    IDC_EFFECT_PARAMETER0,
                    IDC_EFFECT_PARAMETER1,
                    IDC_EFFECT_PARAMETER2,
                    IDC_EFFECT_PARAMETER3,
                    IDC_EFFECT_PARAMETER4};
                for (std::size_t index = 0; index < edits.size(); ++index) {
                    if (!ReadSignedDialogValue(
                            dialog, edits[index], state->parameters[index])) {
                        if (!state->close_immediately) {
                            MessageBoxW(
                                dialog,
                                L"数値パラメーターを10進整数で入力してください。",
                                L"inkpod",
                                MB_OK | MB_ICONWARNING);
                        }
                        return TRUE;
                    }
                }
                state->channel = SelectedEffectCombo(
                    dialog, IDC_EFFECT_CHANNEL, state->channel);
                state->mode = SelectedEffectCombo(
                    dialog, IDC_EFFECT_MODE, state->mode);
                std::array<wchar_t, 1024U> points{};
                GetDlgItemTextW(
                    dialog,
                    IDC_EFFECT_POINTS,
                    points.data(),
                    static_cast<int>(points.size()));
                try {
                    state->points.assign(points.data());
                } catch (const std::bad_alloc&) {
                    return TRUE;
                }
                state->option1 =
                    IsDlgButtonChecked(dialog, IDC_EFFECT_OPTION1) == BST_CHECKED;
                state->option2 =
                    IsDlgButtonChecked(dialog, IDC_EFFECT_OPTION2) == BST_CHECKED;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR CALLBACK ProgressDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<ProgressDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<ProgressDialogState*>(lparam);
            if (state == nullptr || state->query == nullptr
                || state->cancel == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            if (state->title != nullptr) {
                SetWindowTextW(dialog, state->title);
            }
            SendDlgItemMessageW(
                dialog, IDC_EFFECT_PROGRESS_BAR, PBM_SETRANGE32, 0, 1000);
            SetTimer(dialog, kProgressTimer, 100U, nullptr);
            return TRUE;
        case WM_TIMER:
            if (state != nullptr && wparam == kProgressTimer) {
                ProgressDialogInfo info{};
                if (state->query(state->context, info)) {
                    const std::uint64_t value = info.total_work == 0U
                        ? 0U
                        : std::min<std::uint64_t>(
                              1000U,
                              info.completed_work * 1000U / info.total_work);
                    SendDlgItemMessageW(
                        dialog, IDC_EFFECT_PROGRESS_BAR, PBM_SETPOS, value, 0);
                    std::array<wchar_t, 128U> text{};
                    _snwprintf_s(
                        text.data(),
                        text.size(),
                        _TRUNCATE,
                        L"%ls %llu / %llu",
                        state->progress_prefix,
                        static_cast<unsigned long long>(info.completed_work),
                        static_cast<unsigned long long>(info.total_work));
                    SetDlgItemTextW(dialog, IDC_EFFECT_PROGRESS_TEXT, text.data());
                }
                return TRUE;
            }
            break;
        case WM_COMMAND:
            if (LOWORD(wparam) == IDCANCEL && state != nullptr) {
                state->cancel(state->context);
                EnableWindow(GetDlgItem(dialog, IDCANCEL), FALSE);
                SetDlgItemTextW(
                    dialog, IDC_EFFECT_PROGRESS_TEXT, state->cancelling_text);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr) {
                state->cancel(state->context);
            }
            return TRUE;
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
        const INT_PTR result = DialogBoxParamW(
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

HWND CreateProgressDialog(
    HINSTANCE instance, HWND owner, ProgressDialogState& state) noexcept {
    return CreateDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_EFFECT_PROGRESS),
        owner,
        ProgressDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

}  // namespace inkpod::windows::ui
