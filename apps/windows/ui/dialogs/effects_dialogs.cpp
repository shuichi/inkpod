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

}  // namespace inkpod::windows::ui
