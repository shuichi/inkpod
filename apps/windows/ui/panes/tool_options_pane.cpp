#include "tool_options_pane.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <climits>
#include <cmath>
#include <cwchar>
#include <new>
#include <string>

#include "app/resource.h"
#include "ui/localization.h"
#include "ui/tools/tool_state.h"

namespace inkpod::windows::ui::panes {
namespace {

constexpr UINT_PTR kPaneSubclass = 1U;
constexpr wchar_t kFlyoutClassName[] = L"InkpodToolOptionsFlyout";

constexpr std::array<int, 4U> kViewLabelIds{
    IDC_VIEW_VALUE_LABEL,
    IDC_VIEW_VALUE2_LABEL,
    IDC_VIEW_VALUE3_LABEL,
    IDC_VIEW_VALUE4_LABEL};
constexpr std::array<int, 4U> kViewEditIds{
    IDC_VIEW_VALUE,
    IDC_VIEW_VALUE2,
    IDC_VIEW_VALUE3,
    IDC_VIEW_VALUE4};
constexpr std::array<int, 4U> kViewChoiceIds{
    IDC_VIEW_VALUE_CHOICE,
    IDC_VIEW_VALUE2_CHOICE,
    IDC_VIEW_VALUE3_CHOICE,
    IDC_VIEW_VALUE4_CHOICE};
constexpr std::array<int, 5U> kEffectLabelIds{
    IDC_EFFECT_PARAMETER0_LABEL,
    IDC_EFFECT_PARAMETER1_LABEL,
    IDC_EFFECT_PARAMETER2_LABEL,
    IDC_EFFECT_PARAMETER3_LABEL,
    IDC_EFFECT_PARAMETER4_LABEL};
constexpr std::array<int, 5U> kEffectEditIds{
    IDC_EFFECT_PARAMETER0,
    IDC_EFFECT_PARAMETER1,
    IDC_EFFECT_PARAMETER2,
    IDC_EFFECT_PARAMETER3,
    IDC_EFFECT_PARAMETER4};
constexpr std::array<int, 6U> kFillCheckIds{
    IDC_FILL_OVERFLOW,
    IDC_FILL_DETACHED,
    IDC_FILL_TRANSPARENT,
    IDC_FILL_SELECTION,
    IDC_FILL_LIGHT_BOUNDARY,
    IDC_FILL_LIGHT_COLOR};
constexpr std::array<int, 6U> kFillLabelIds{
    IDC_TOOL_OPTIONS_FILL_OPERATION_LABEL,
    IDC_TOOL_OPTIONS_FILL_TOLERANCE_LABEL,
    IDC_TOOL_OPTIONS_FILL_GAP_LABEL,
    IDC_TOOL_OPTIONS_FILL_EXTENSION_LABEL,
    IDC_TOOL_OPTIONS_FILL_INCLUSION_LABEL,
    IDC_TOOL_OPTIONS_FILL_COLORS_LABEL};

int ScaleForDpi(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

HFONT CreatePaneFont(HWND pane, int point_size) noexcept {
    const UINT dpi = GetDpiForWindow(pane);
    return CreateFontW(
        -MulDiv(point_size, static_cast<int>(dpi == 0U ? 96U : dpi), 72),
        0,
        0,
        0,
        FW_NORMAL,
        FALSE,
        FALSE,
        FALSE,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH | FF_DONTCARE,
        L"Segoe UI");
}

const wchar_t* ToolLabel(std::uint32_t tool) noexcept {
    if (tool == INKPOD_TOOL_PENCIL) return UiText(UiStringId::ToolPencil);
    if (tool == INKPOD_TOOL_BRUSH) return UiText(UiStringId::ToolBrush);
    if (tool == INKPOD_TOOL_ERASER) return UiText(UiStringId::ToolEraser);
    if (tool == tools::kInteractionFill) return UiText(UiStringId::ToolFill);
    if (tool == tools::kInteractionEyedropper) return UiText(UiStringId::ToolEyedropper);
    if (tool == tools::kInteractionSelection) return UiText(UiStringId::Text0976);
    if (tool == tools::kInteractionVectorLine) return UiText(UiStringId::ToolVectorLine);
    if (tool == tools::kInteractionVectorCurve) return UiText(UiStringId::ToolVectorCurve);
    if (tool == tools::kInteractionVectorRectangle) return UiText(UiStringId::ToolVectorRectangle);
    if (tool == tools::kInteractionVectorEllipse) return UiText(UiStringId::ToolVectorEllipse);
    if (tool == tools::kInteractionVectorPolyline) return UiText(UiStringId::ToolVectorPolyline);
    if (tool == tools::kInteractionVectorPolygon) return UiText(UiStringId::ToolVectorPolygon);
    if (tool == tools::kInteractionVectorEraser) return UiText(UiStringId::ToolVectorEraser);
    if (tool == tools::kInteractionEffectGradient) return UiText(UiStringId::ToolGradient);
    if (tool == tools::kInteractionEffectAirbrush) return UiText(UiStringId::ToolAirbrush);
    if (tool == tools::kInteractionEffectBlur) return UiText(UiStringId::ToolBlur);
    if (tool == tools::kInteractionEffectStamp) return UiText(UiStringId::ToolStamp);
    if (tool == tools::kInteractionEffectDust) return UiText(UiStringId::ToolDustRemoval);
    if (tool == tools::kInteractionEffectAlphaGradient) return UiText(UiStringId::ToolAlphaGradient);
    return UiText(UiStringId::ToolGeneric);
}

bool HasDiameter(std::uint32_t tool) noexcept {
    return tool == INKPOD_TOOL_PENCIL || tool == INKPOD_TOOL_BRUSH
        || tool == INKPOD_TOOL_ERASER || tools::IsVectorCanvasTool(tool);
}

bool CanEditDiameter(std::uint32_t tool) noexcept {
    return tool == INKPOD_TOOL_BRUSH || tool == INKPOD_TOOL_ERASER
        || tools::IsVectorCanvasTool(tool);
}

HWND CreateControl(
    HINSTANCE instance,
    HWND parent,
    const wchar_t* class_name,
    const wchar_t* text,
    DWORD style,
    int id) noexcept {
    return CreateWindowExW(
        0,
        class_name,
        text == nullptr ? L"" : text,
        WS_CHILD | style,
        0,
        0,
        0,
        0,
        parent,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)),
        instance,
        nullptr);
}

void SetVisible(HWND pane, int id, bool visible) noexcept {
    const HWND control = GetDlgItem(pane, id);
    if (control != nullptr) ShowWindow(control, visible ? SW_SHOW : SW_HIDE);
}

void SetControlBounds(
    HWND pane, int id, int x, int y, int width, int height) noexcept {
    const HWND control = GetDlgItem(pane, id);
    if (control != nullptr) {
        SetWindowPos(
            control,
            nullptr,
            x,
            y,
            std::max(0, width),
            std::max(0, height),
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
    }
}

void HideDetailControls(HWND pane) noexcept {
    for (const int id : kViewLabelIds) SetVisible(pane, id, false);
    for (const int id : kViewEditIds) SetVisible(pane, id, false);
    for (const int id : kViewChoiceIds) SetVisible(pane, id, false);
    for (const int id : kEffectLabelIds) SetVisible(pane, id, false);
    for (const int id : kEffectEditIds) SetVisible(pane, id, false);
    for (const int id : kFillLabelIds) SetVisible(pane, id, false);
    for (const int id : kFillCheckIds) SetVisible(pane, id, false);
    for (const int id : {
             IDC_TOOL_OPTIONS_PAGE_TITLE,
             IDC_TOOL_OPTIONS_APPLY,
             IDC_FILL_OPERATION,
             IDC_FILL_TOLERANCE,
             IDC_FILL_GAP,
             IDC_FILL_EXTENSION,
             IDC_FILL_INCLUSION_MODE,
             IDC_FILL_COLORS,
             IDC_TOOL_OPTIONS_EFFECT_CHANNEL_LABEL,
             IDC_TOOL_OPTIONS_EFFECT_MODE_LABEL,
             IDC_TOOL_OPTIONS_EFFECT_POINTS_LABEL,
             IDC_EFFECT_CHANNEL,
             IDC_EFFECT_MODE,
             IDC_EFFECT_POINTS,
             IDC_EFFECT_OPTION1,
             IDC_EFFECT_OPTION2}) {
        SetVisible(pane, id, false);
    }
}

void SelectComboValue(HWND combo, std::int64_t value) noexcept {
    if (combo == nullptr) return;
    const LRESULT count = SendMessageW(combo, CB_GETCOUNT, 0, 0);
    for (LRESULT index = 0; index < count; ++index) {
        if (SendMessageW(combo, CB_GETITEMDATA, index, 0) == value) {
            SendMessageW(combo, CB_SETCURSEL, index, 0);
            return;
        }
    }
    SendMessageW(combo, CB_SETCURSEL, 0, 0);
}

bool AddComboItem(HWND combo, const wchar_t* label, std::int64_t value) noexcept {
    if (combo == nullptr || label == nullptr) return false;
    const LRESULT index = SendMessageW(
        combo, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(label));
    return index != CB_ERR && index != CB_ERRSPACE
        && SendMessageW(combo, CB_SETITEMDATA, index, value) != CB_ERR;
}

void PopulateFillControls(HWND pane, const FillToolOptions& options) noexcept {
    const HWND operation = GetDlgItem(pane, IDC_FILL_OPERATION);
    SendMessageW(operation, CB_RESETCONTENT, 0, 0);
    AddComboItem(operation, UiText(UiStringId::Text0957), INKPOD_FILL_SEED);
    AddComboItem(operation, UiText(UiStringId::Text1014), INKPOD_FILL_CLOSED_REGION);
    AddComboItem(operation, UiText(UiStringId::Text0598), INKPOD_FILL_EXTENSION);
    SelectComboValue(operation, options.operation);

    const HWND inclusion = GetDlgItem(pane, IDC_FILL_INCLUSION_MODE);
    SendMessageW(inclusion, CB_RESETCONTENT, 0, 0);
    AddComboItem(inclusion, UiText(UiStringId::Text0113), INKPOD_INCLUSION_NONE);
    AddComboItem(inclusion, UiText(UiStringId::Text0669), INKPOD_INCLUSION_SPECIFIED);
    AddComboItem(
        inclusion,
        UiText(UiStringId::Text0670),
        INKPOD_INCLUSION_EXCEPT_SPECIFIED);
    SelectComboValue(inclusion, options.inclusion_mode);

    SetDlgItemInt(pane, IDC_FILL_TOLERANCE, options.tolerance, FALSE);
    SetDlgItemInt(pane, IDC_FILL_GAP, options.gap_close, FALSE);
    SetDlgItemInt(pane, IDC_FILL_EXTENSION, options.extension_distance, FALSE);
    std::wstring colors;
    if (FormatFillOptionColors(options.inclusion_colors, colors)) {
        SetDlgItemTextW(pane, IDC_FILL_COLORS, colors.c_str());
    }
    const std::array<bool, 6U> checks{
        options.overflow_abort,
        options.detached_regions,
        options.transparent_only,
        options.use_document_selection,
        options.light_table_boundary,
        options.light_table_color};
    for (std::size_t index = 0U; index < checks.size(); ++index) {
        CheckDlgButton(
            pane,
            kFillCheckIds[index],
            checks[index] ? BST_CHECKED : BST_UNCHECKED);
    }
}

void PopulateViewControls(
    HWND pane, const ViewOptionsDialogState& view) noexcept {
    for (std::size_t index = 0U; index < kViewLabelIds.size(); ++index) {
        const bool visible = index < view.value_count;
        const bool choice = visible && view.choice_counts[index] != 0U;
        SetVisible(pane, kViewLabelIds[index], visible);
        SetVisible(pane, kViewEditIds[index], visible && !choice);
        SetVisible(pane, kViewChoiceIds[index], choice);
        if (!visible) continue;
        SetDlgItemTextW(
            pane,
            kViewLabelIds[index],
            view.labels[index] == nullptr
                ? UiText(UiStringId::Text0480)
                : view.labels[index]);
        if (!choice) {
            std::array<wchar_t, 32U> text{};
            _snwprintf_s(
                text.data(), text.size(), _TRUNCATE, L"%d", view.values[index]);
            SetDlgItemTextW(pane, kViewEditIds[index], text.data());
            continue;
        }
        const HWND combo = GetDlgItem(pane, kViewChoiceIds[index]);
        SendMessageW(combo, CB_RESETCONTENT, 0, 0);
        for (std::uint32_t choice_index = 0U;
             choice_index < view.choice_counts[index];
             ++choice_index) {
            const auto& item = view.choices[index][choice_index];
            AddComboItem(combo, item.label, item.value);
        }
        SelectComboValue(combo, view.values[index]);
    }
}

void PopulateEffectControls(
    HWND pane, const EffectEditorState& effect, bool boundary) noexcept {
    for (std::size_t index = 0U; index < kEffectLabelIds.size(); ++index) {
        SetDlgItemTextW(pane, kEffectLabelIds[index], effect.parameter_labels[index]);
        std::array<wchar_t, 32U> value{};
        _snwprintf_s(
            value.data(), value.size(), _TRUNCATE, L"%d", effect.parameters[index]);
        SetDlgItemTextW(pane, kEffectEditIds[index], value.data());
    }
    const HWND channel = GetDlgItem(pane, IDC_EFFECT_CHANNEL);
    SendMessageW(channel, CB_RESETCONTENT, 0, 0);
    for (std::size_t index = 0U; index < effect.channel_count; ++index) {
        AddComboItem(channel, effect.channel_labels[index], effect.channel_values[index]);
    }
    SelectComboValue(channel, effect.channel);
    const HWND mode = GetDlgItem(pane, IDC_EFFECT_MODE);
    SendMessageW(mode, CB_RESETCONTENT, 0, 0);
    for (std::size_t index = 0U; index < effect.mode_count; ++index) {
        AddComboItem(mode, effect.mode_labels[index], effect.mode_values[index]);
    }
    SelectComboValue(mode, effect.mode);
    SetDlgItemTextW(pane, IDC_EFFECT_POINTS, effect.points.c_str());
    SetDlgItemTextW(pane, IDC_EFFECT_OPTION1, effect.option1_label);
    SetDlgItemTextW(pane, IDC_EFFECT_OPTION2, effect.option2_label);
    CheckDlgButton(
        pane, IDC_EFFECT_OPTION1, effect.option1 ? BST_CHECKED : BST_UNCHECKED);
    CheckDlgButton(
        pane, IDC_EFFECT_OPTION2, effect.option2 ? BST_CHECKED : BST_UNCHECKED);
    EnableWindow(
        GetDlgItem(pane, IDC_EFFECT_OPTION1),
        effect.option1_enabled ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(pane, IDC_EFFECT_OPTION2),
        effect.option2_enabled ? TRUE : FALSE);
    SetVisible(pane, IDC_TOOL_OPTIONS_APPLY, boundary);
}

void PopulateDetailControls(HWND pane, ToolOptionsPaneState& state) noexcept {
    state.updating_detail = true;
    HideDetailControls(pane);
    const ToolOptionsDetailModel& detail = state.detail;
    if (detail.kind == ToolOptionsDetailKind::None) {
        state.updating_detail = false;
        return;
    }
    SetVisible(pane, IDC_TOOL_OPTIONS_PAGE_TITLE, true);
    const wchar_t* title = detail.kind == ToolOptionsDetailKind::Fill
        ? UiText(UiStringId::Text0301)
        : (detail.kind == ToolOptionsDetailKind::View
                  ? detail.view.title
                  : detail.effect.title);
    SetDlgItemTextW(
        pane,
        IDC_TOOL_OPTIONS_PAGE_TITLE,
        title == nullptr ? ToolLabel(state.active_tool) : title);
    if (detail.kind == ToolOptionsDetailKind::Fill) {
        for (const int id : kFillLabelIds) SetVisible(pane, id, true);
        for (const int id : kFillCheckIds) SetVisible(pane, id, true);
        for (const int id : {
                 IDC_FILL_OPERATION,
                 IDC_FILL_TOLERANCE,
                 IDC_FILL_GAP,
                 IDC_FILL_EXTENSION,
                 IDC_FILL_INCLUSION_MODE,
                 IDC_FILL_COLORS}) {
            SetVisible(pane, id, true);
        }
        PopulateFillControls(pane, detail.fill);
    } else if (detail.kind == ToolOptionsDetailKind::View) {
        PopulateViewControls(pane, detail.view);
    } else {
        for (const int id : kEffectLabelIds) SetVisible(pane, id, true);
        for (const int id : kEffectEditIds) SetVisible(pane, id, true);
        SetVisible(
            pane,
            IDC_TOOL_OPTIONS_EFFECT_CHANNEL_LABEL,
            detail.effect.channel_count != 0U);
        SetVisible(pane, IDC_EFFECT_CHANNEL, detail.effect.channel_count != 0U);
        SetVisible(
            pane,
            IDC_TOOL_OPTIONS_EFFECT_MODE_LABEL,
            detail.effect.mode_count != 0U);
        SetVisible(pane, IDC_EFFECT_MODE, detail.effect.mode_count != 0U);
        SetVisible(
            pane,
            IDC_TOOL_OPTIONS_EFFECT_POINTS_LABEL,
            !detail.effect.points.empty());
        SetVisible(pane, IDC_EFFECT_POINTS, !detail.effect.points.empty());
        SetVisible(pane, IDC_EFFECT_OPTION1, detail.effect.option1_enabled);
        SetVisible(pane, IDC_EFFECT_OPTION2, detail.effect.option2_enabled);
        PopulateEffectControls(
            pane,
            detail.effect,
            detail.kind == ToolOptionsDetailKind::BoundaryEffect);
    }
    state.updating_detail = false;
}

void LayoutLabeledRow(
    HWND pane,
    int label,
    int control,
    int margin,
    int y,
    int width,
    int row,
    int label_width) noexcept {
    SetControlBounds(pane, label, margin, y, label_width, row);
    SetControlBounds(
        pane,
        control,
        margin + label_width,
        y,
        std::max(0, width - margin * 2 - label_width),
        row);
}

void LayoutPane(HWND pane) noexcept {
    RECT client{};
    if (GetClientRect(pane, &client) == FALSE) return;
    auto* state = reinterpret_cast<ToolOptionsPaneState*>(
        GetWindowLongPtrW(pane, GWLP_USERDATA));
    if (state == nullptr) return;
    const UINT dpi = GetDpiForWindow(pane);
    const int margin = ScaleForDpi(12, dpi);
    const int gap = ScaleForDpi(6, dpi);
    const int row = ScaleForDpi(26, dpi);
    const int width = static_cast<int>(client.right);
    const int label_width = std::min(
        ScaleForDpi(126, dpi), std::max(0, width / 2));
    int y = margin;
    SetControlBounds(
        pane,
        IDC_TOOL_OPTIONS_LABEL,
        margin,
        y,
        std::max(0, width - margin * 2),
        ScaleForDpi(30, dpi));
    y += ScaleForDpi(34, dpi);

    if (state->active_tool == INKPOD_TOOL_ERASER) {
        SetControlBounds(
            pane, IDC_TOOL_OPTIONS_TARGET_LABEL, margin, y, label_width, row);
        const int target_x = margin + label_width;
        const int target_width = std::max(0, width - margin - target_x);
        SetControlBounds(
            pane,
            IDC_TOOL_OPTIONS_TARGET_MAIN_LINE,
            target_x,
            y,
            target_width / 2,
            row);
        SetControlBounds(
            pane,
            IDC_TOOL_OPTIONS_TARGET_COLOR,
            target_x + target_width / 2,
            y,
            target_width - target_width / 2,
            row);
        y += row + gap;
    }

    if (HasDiameter(state->active_tool)) {
        LayoutLabeledRow(
            pane,
            IDC_TOOL_OPTIONS_DIAMETER_LABEL,
            IDC_TOOL_OPTIONS_DIAMETER,
            margin,
            y,
            width,
            row,
            label_width);
        y += row + gap;
    }
    if (state->active_tool == INKPOD_TOOL_BRUSH) {
        SetControlBounds(
            pane,
            IDC_TOOL_OPTIONS_BRUSH_SHAPE,
            margin,
            y,
            std::max(0, width - margin * 2),
            ScaleForDpi(120, dpi));
        y += row + gap;
        SetControlBounds(
            pane,
            IDC_TOOL_OPTIONS_BRUSH_SMOOTHING,
            margin,
            y,
            std::max(0, width - margin * 2),
            row);
        y += row + gap;
        SetControlBounds(
            pane,
            IDC_TOOL_OPTIONS_BRUSH_START_COLOR,
            margin,
            y,
            std::max(0, width - margin * 2),
            row);
        y += row + gap;
    }

    if (state->detail.kind == ToolOptionsDetailKind::None) return;
    SetControlBounds(
        pane,
        IDC_TOOL_OPTIONS_PAGE_TITLE,
        margin,
        y,
        std::max(0, width - margin * 2),
        ScaleForDpi(28, dpi));
    y += ScaleForDpi(32, dpi);

    if (state->detail.kind == ToolOptionsDetailKind::View) {
        for (std::size_t index = 0U;
             index < state->detail.view.value_count && index < kViewLabelIds.size();
             ++index) {
            const int control = state->detail.view.choice_counts[index] == 0U
                ? kViewEditIds[index]
                : kViewChoiceIds[index];
            LayoutLabeledRow(
                pane,
                kViewLabelIds[index],
                control,
                margin,
                y,
                width,
                row,
                label_width);
            if (control == kViewChoiceIds[index]) {
                SetControlBounds(
                    pane,
                    control,
                    margin + label_width,
                    y,
                    std::max(0, width - margin * 2 - label_width),
                    ScaleForDpi(160, dpi));
            }
            y += row + gap;
        }
        return;
    }
    if (state->detail.kind == ToolOptionsDetailKind::Fill) {
        const std::array<int, 6U> controls{
            IDC_FILL_OPERATION,
            IDC_FILL_TOLERANCE,
            IDC_FILL_GAP,
            IDC_FILL_EXTENSION,
            IDC_FILL_INCLUSION_MODE,
            IDC_FILL_COLORS};
        for (std::size_t index = 0U; index < controls.size(); ++index) {
            const int height = controls[index] == IDC_FILL_COLORS
                ? ScaleForDpi(58, dpi)
                : row;
            LayoutLabeledRow(
                pane,
                kFillLabelIds[index],
                controls[index],
                margin,
                y,
                width,
                height,
                label_width);
            if (controls[index] == IDC_FILL_OPERATION
                || controls[index] == IDC_FILL_INCLUSION_MODE) {
                SetControlBounds(
                    pane,
                    controls[index],
                    margin + label_width,
                    y,
                    std::max(0, width - margin * 2 - label_width),
                    ScaleForDpi(160, dpi));
            }
            y += height + gap;
        }
        for (const int id : kFillCheckIds) {
            SetControlBounds(
                pane,
                id,
                margin,
                y,
                std::max(0, width - margin * 2),
                row);
            y += row;
        }
        return;
    }

    for (std::size_t index = 0U; index < kEffectEditIds.size(); ++index) {
        LayoutLabeledRow(
            pane,
            kEffectLabelIds[index],
            kEffectEditIds[index],
            margin,
            y,
            width,
            row,
            label_width);
        y += row + gap;
    }
    if (state->detail.effect.channel_count != 0U) {
        LayoutLabeledRow(
            pane,
            IDC_TOOL_OPTIONS_EFFECT_CHANNEL_LABEL,
            IDC_EFFECT_CHANNEL,
            margin,
            y,
            width,
            row,
            label_width);
        SetControlBounds(
            pane,
            IDC_EFFECT_CHANNEL,
            margin + label_width,
            y,
            std::max(0, width - margin * 2 - label_width),
            ScaleForDpi(160, dpi));
        y += row + gap;
    }
    if (state->detail.effect.mode_count != 0U) {
        LayoutLabeledRow(
            pane,
            IDC_TOOL_OPTIONS_EFFECT_MODE_LABEL,
            IDC_EFFECT_MODE,
            margin,
            y,
            width,
            row,
            label_width);
        SetControlBounds(
            pane,
            IDC_EFFECT_MODE,
            margin + label_width,
            y,
            std::max(0, width - margin * 2 - label_width),
            ScaleForDpi(160, dpi));
        y += row + gap;
    }
    if (!state->detail.effect.points.empty()) {
        const int points_height = ScaleForDpi(72, dpi);
        LayoutLabeledRow(
            pane,
            IDC_TOOL_OPTIONS_EFFECT_POINTS_LABEL,
            IDC_EFFECT_POINTS,
            margin,
            y,
            width,
            points_height,
            label_width);
        y += points_height + gap;
    }
    for (const int id : {IDC_EFFECT_OPTION1, IDC_EFFECT_OPTION2}) {
        if (IsWindowVisible(GetDlgItem(pane, id)) != FALSE) {
            SetControlBounds(
                pane,
                id,
                margin,
                y,
                std::max(0, width - margin * 2),
                row);
            y += row;
        }
    }
    if (state->detail.kind == ToolOptionsDetailKind::BoundaryEffect) {
        SetControlBounds(
            pane,
            IDC_TOOL_OPTIONS_APPLY,
            std::max(margin, width - margin - ScaleForDpi(96, dpi)),
            y + gap,
            ScaleForDpi(96, dpi),
            row);
    }
}

void UpdateFont(HWND pane, ToolOptionsPaneState& state) noexcept {
    const HFONT replacement = CreatePaneFont(pane, 9);
    const HFONT edit_replacement = CreatePaneFont(pane, 9);
    if (replacement == nullptr || edit_replacement == nullptr) {
        if (replacement != nullptr) DeleteObject(replacement);
        if (edit_replacement != nullptr) DeleteObject(edit_replacement);
        return;
    }
    for (HWND child = GetWindow(pane, GW_CHILD);
         child != nullptr;
         child = GetWindow(child, GW_HWNDNEXT)) {
        std::array<wchar_t, 32U> name{};
        const bool named = GetClassNameW(
            child, name.data(), static_cast<int>(name.size())) > 0;
        const bool edit = named
            && (_wcsicmp(name.data(), L"Edit") == 0
                || _wcsicmp(name.data(), WC_COMBOBOXW) == 0);
        SendMessageW(
            child,
            WM_SETFONT,
            reinterpret_cast<WPARAM>(edit ? edit_replacement : replacement),
            TRUE);
    }
    if (state.font != nullptr) DeleteObject(state.font);
    if (state.edit_font != nullptr) DeleteObject(state.edit_font);
    state.font = replacement;
    state.edit_font = edit_replacement;
}

bool ReadSignedValue(HWND pane, int id, std::int32_t& output) noexcept {
    std::array<wchar_t, 64U> text{};
    if (GetDlgItemTextW(pane, id, text.data(), static_cast<int>(text.size())) <= 0) {
        return false;
    }
    wchar_t* end{};
    errno = 0;
    const long value = std::wcstol(text.data(), &end, 10);
    if (errno == ERANGE || end == text.data() || *end != L'\0'
        || value < INT_MIN || value > INT_MAX) {
        return false;
    }
    output = static_cast<std::int32_t>(value);
    return true;
}

bool ReadComboValue(HWND pane, int id, std::int32_t& output) noexcept {
    const LRESULT selected = SendDlgItemMessageW(pane, id, CB_GETCURSEL, 0, 0);
    if (selected == CB_ERR) return false;
    const LRESULT value = SendDlgItemMessageW(
        pane, id, CB_GETITEMDATA, static_cast<WPARAM>(selected), 0);
    if (value == CB_ERR || value < INT_MIN || value > INT_MAX) return false;
    output = static_cast<std::int32_t>(value);
    return true;
}

bool ReadFillControls(HWND pane, FillToolOptions& options) noexcept {
    std::int32_t operation{};
    std::int32_t inclusion{};
    BOOL tolerance_ok{};
    BOOL gap_ok{};
    BOOL extension_ok{};
    const UINT tolerance = GetDlgItemInt(
        pane, IDC_FILL_TOLERANCE, &tolerance_ok, FALSE);
    const UINT gap = GetDlgItemInt(pane, IDC_FILL_GAP, &gap_ok, FALSE);
    const UINT extension = GetDlgItemInt(
        pane, IDC_FILL_EXTENSION, &extension_ok, FALSE);
    std::array<wchar_t, 256U> colors{};
    GetDlgItemTextW(
        pane, IDC_FILL_COLORS, colors.data(), static_cast<int>(colors.size()));
    std::vector<InkpodColorValue> parsed;
    if (!ReadComboValue(pane, IDC_FILL_OPERATION, operation)
        || !ReadComboValue(pane, IDC_FILL_INCLUSION_MODE, inclusion)
        || tolerance_ok == FALSE || gap_ok == FALSE || extension_ok == FALSE
        || tolerance > UINT16_MAX || gap > UINT16_MAX || extension == 0U
        || !ParseFillOptionColors(colors.data(), parsed)
        || (inclusion != INKPOD_INCLUSION_NONE && parsed.empty())) {
        return false;
    }
    options.operation = static_cast<InkpodFillOperation>(operation);
    options.inclusion_mode = static_cast<InkpodInclusionMode>(inclusion);
    options.tolerance = static_cast<std::uint16_t>(tolerance);
    options.gap_close = static_cast<std::uint16_t>(gap);
    options.extension_distance = extension;
    options.inclusion_colors = std::move(parsed);
    options.overflow_abort = IsDlgButtonChecked(pane, IDC_FILL_OVERFLOW) == BST_CHECKED;
    options.detached_regions = IsDlgButtonChecked(pane, IDC_FILL_DETACHED) == BST_CHECKED;
    options.transparent_only = IsDlgButtonChecked(pane, IDC_FILL_TRANSPARENT) == BST_CHECKED;
    options.use_document_selection = IsDlgButtonChecked(pane, IDC_FILL_SELECTION) == BST_CHECKED;
    options.light_table_boundary = IsDlgButtonChecked(pane, IDC_FILL_LIGHT_BOUNDARY) == BST_CHECKED;
    options.light_table_color = IsDlgButtonChecked(pane, IDC_FILL_LIGHT_COLOR) == BST_CHECKED;
    return true;
}

bool ReadViewControls(HWND pane, ViewOptionsDialogState& view) noexcept {
    auto values = view.values;
    for (std::size_t index = 0U; index < view.value_count; ++index) {
        if (view.choice_counts[index] == 0U) {
            if (!ReadSignedValue(pane, kViewEditIds[index], values[index])) return false;
        } else if (!ReadComboValue(
                       pane, kViewChoiceIds[index], values[index])) {
            return false;
        }
    }
    if (view.validate != nullptr
        && view.validate(view.validation_context, values, view.value_count)
            != nullptr) {
        return false;
    }
    view.values = values;
    return true;
}

bool ReadEffectControls(HWND pane, EffectEditorState& effect) noexcept {
    auto parameters = effect.parameters;
    for (std::size_t index = 0U; index < parameters.size(); ++index) {
        if (!ReadSignedValue(pane, kEffectEditIds[index], parameters[index])) {
            return false;
        }
    }
    std::int32_t channel = static_cast<std::int32_t>(effect.channel);
    std::int32_t mode = static_cast<std::int32_t>(effect.mode);
    if ((effect.channel_count != 0U
            && !ReadComboValue(pane, IDC_EFFECT_CHANNEL, channel))
        || (effect.mode_count != 0U
            && !ReadComboValue(pane, IDC_EFFECT_MODE, mode))) {
        return false;
    }
    std::array<wchar_t, 1024U> points{};
    GetDlgItemTextW(
        pane, IDC_EFFECT_POINTS, points.data(), static_cast<int>(points.size()));
    try {
        effect.points.assign(points.data());
    } catch (const std::bad_alloc&) {
        return false;
    }
    effect.parameters = parameters;
    effect.channel = static_cast<std::uint32_t>(channel);
    effect.mode = static_cast<std::uint32_t>(mode);
    effect.option1 = IsDlgButtonChecked(pane, IDC_EFFECT_OPTION1) == BST_CHECKED;
    effect.option2 = IsDlgButtonChecked(pane, IDC_EFFECT_OPTION2) == BST_CHECKED;
    return true;
}

bool CommitDetail(HWND pane, ToolOptionsPaneState& state, bool execute) noexcept {
    if (state.updating_detail || state.change_detail == nullptr
        || state.detail.kind == ToolOptionsDetailKind::None) {
        return false;
    }
    try {
        ToolOptionsDetailModel candidate = state.detail;
        const bool read = candidate.kind == ToolOptionsDetailKind::Fill
            ? ReadFillControls(pane, candidate.fill)
            : (candidate.kind == ToolOptionsDetailKind::View
                      ? ReadViewControls(pane, candidate.view)
                      : ReadEffectControls(pane, candidate.effect));
        if (!read
            || !state.change_detail(
                state.context, state.detail_command, candidate, execute)) {
            MessageBeep(MB_ICONWARNING);
            return false;
        }
        state.detail = std::move(candidate);
        RefreshToolOptionsDetail(pane, state.detail_command);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

void CommitDiameter(HWND pane, ToolOptionsPaneState& state) noexcept {
    if (state.updating) return;
    if (!CanEditDiameter(state.active_tool) || state.change_diameter == nullptr) {
        UpdateToolOptionsPane(
            pane, state.active_tool, state.active_plane, state.diameter, state.brush);
        return;
    }
    std::array<wchar_t, 64U> text{};
    GetDlgItemTextW(
        pane, IDC_TOOL_OPTIONS_DIAMETER, text.data(), static_cast<int>(text.size()));
    wchar_t* end{};
    const double value = std::wcstod(text.data(), &end);
    if (end != text.data() && *end == L'\0' && std::isfinite(value)
        && value >= static_cast<double>(kMinimumToolDiameter)
        && value <= static_cast<double>(kMaximumToolDiameter)) {
        state.change_diameter(state.context, static_cast<float>(value));
    }
    UpdateToolOptionsPane(
        pane, state.active_tool, state.active_plane, state.diameter, state.brush);
}

void CommitBrushSmoothing(HWND pane, ToolOptionsPaneState& state) noexcept {
    if (state.updating || state.active_tool != INKPOD_TOOL_BRUSH
        || state.change_brush == nullptr) {
        return;
    }
    std::array<wchar_t, 64U> text{};
    GetDlgItemTextW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_SMOOTHING,
        text.data(),
        static_cast<int>(text.size()));
    wchar_t* end{};
    const unsigned long value = std::wcstoul(text.data(), &end, 10);
    if (end != text.data() && *end == L'\0' && value <= 1000UL) {
        InkpodEditorBrushOptions options = state.brush;
        options.struct_size = sizeof(options);
        options.smoothing = static_cast<std::uint16_t>(value);
        state.change_brush(state.context, options);
    }
}

bool IsDetailEditControl(UINT control) noexcept {
    return std::find(kViewEditIds.begin(), kViewEditIds.end(), control)
            != kViewEditIds.end()
        || std::find(kEffectEditIds.begin(), kEffectEditIds.end(), control)
            != kEffectEditIds.end()
        || control == IDC_FILL_TOLERANCE || control == IDC_FILL_GAP
        || control == IDC_FILL_EXTENSION || control == IDC_FILL_COLORS
        || control == IDC_EFFECT_POINTS;
}

bool IsDetailComboControl(UINT control) noexcept {
    return std::find(kViewChoiceIds.begin(), kViewChoiceIds.end(), control)
            != kViewChoiceIds.end()
        || control == IDC_FILL_OPERATION || control == IDC_FILL_INCLUSION_MODE
        || control == IDC_EFFECT_CHANNEL || control == IDC_EFFECT_MODE;
}

bool IsDetailCheckControl(UINT control) noexcept {
    return std::find(kFillCheckIds.begin(), kFillCheckIds.end(), control)
            != kFillCheckIds.end()
        || control == IDC_EFFECT_OPTION1 || control == IDC_EFFECT_OPTION2;
}

LRESULT CALLBACK PaneSubclassProcedure(
    HWND pane,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<ToolOptionsPaneState*>(reference);
    switch (message) {
        case WM_SIZE:
            LayoutPane(pane);
            return 0;
        case WM_KEYDOWN:
            if (wparam == VK_ESCAPE) {
                HWND parent = GetParent(pane);
                if (parent != nullptr) ShowWindow(parent, SW_HIDE);
                return 0;
            }
            break;
        case WM_COMMAND: {
            if (state == nullptr) break;
            const UINT control = LOWORD(wparam);
            const UINT notification = HIWORD(wparam);
            if (control == IDC_TOOL_OPTIONS_DIAMETER
                && notification == EN_SETFOCUS) {
                state->editing = true;
                return 0;
            }
            if (control == IDC_TOOL_OPTIONS_DIAMETER
                && notification == EN_KILLFOCUS) {
                state->editing = false;
                CommitDiameter(pane, *state);
                return 0;
            }
            if (control == IDC_TOOL_OPTIONS_BRUSH_SMOOTHING
                && notification == EN_SETFOCUS) {
                state->editing_smoothing = true;
                return 0;
            }
            if (control == IDC_TOOL_OPTIONS_BRUSH_SMOOTHING
                && notification == EN_KILLFOCUS) {
                state->editing_smoothing = false;
                CommitBrushSmoothing(pane, *state);
                return 0;
            }
            if (control == IDC_TOOL_OPTIONS_BRUSH_SHAPE
                && notification == CBN_SELCHANGE
                && state->active_tool == INKPOD_TOOL_BRUSH
                && state->change_brush != nullptr) {
                const LRESULT selected = SendDlgItemMessageW(
                    pane, IDC_TOOL_OPTIONS_BRUSH_SHAPE, CB_GETCURSEL, 0, 0);
                if (selected == 0 || selected == 1) {
                    InkpodEditorBrushOptions options = state->brush;
                    options.struct_size = sizeof(options);
                    options.shape = selected == 0
                        ? INKPOD_BRUSH_ROUND
                        : INKPOD_BRUSH_SQUARE;
                    state->change_brush(state->context, options);
                }
                return 0;
            }
            if (control == IDC_TOOL_OPTIONS_BRUSH_START_COLOR
                && notification == BN_CLICKED
                && state->active_tool == INKPOD_TOOL_BRUSH
                && state->change_brush != nullptr) {
                InkpodEditorBrushOptions options = state->brush;
                options.struct_size = sizeof(options);
                options.start_color = IsDlgButtonChecked(
                    pane, IDC_TOOL_OPTIONS_BRUSH_START_COLOR) == BST_CHECKED
                    ? INKPOD_START_COLOR_EXACT_NATIVE
                    : INKPOD_START_COLOR_ANY;
                state->change_brush(state->context, options);
                return 0;
            }
            if ((control == IDC_TOOL_OPTIONS_TARGET_MAIN_LINE
                    || control == IDC_TOOL_OPTIONS_TARGET_COLOR)
                && notification == BN_CLICKED
                && state->dispatch_command != nullptr) {
                state->dispatch_command(
                    state->context,
                    control == IDC_TOOL_OPTIONS_TARGET_MAIN_LINE
                        ? IDM_PLANE_MAIN_LINE
                        : IDM_PLANE_COLOR);
                return 0;
            }
            if (control == IDC_TOOL_OPTIONS_APPLY
                && notification == BN_CLICKED) {
                CommitDetail(pane, *state, true);
                return 0;
            }
            if ((IsDetailComboControl(control) && notification == CBN_SELCHANGE)
                || (IsDetailCheckControl(control) && notification == BN_CLICKED)
                || (IsDetailEditControl(control) && notification == EN_KILLFOCUS)) {
                CommitDetail(pane, *state, false);
                return 0;
            }
            break;
        }
        case WM_DPICHANGED_AFTERPARENT:
            if (state != nullptr) {
                UpdateFont(pane, *state);
                LayoutPane(pane);
            }
            return 0;
        case WM_NCDESTROY:
            if (state != nullptr) {
                if (state->font != nullptr) DeleteObject(state->font);
                if (state->edit_font != nullptr) DeleteObject(state->edit_font);
                state->font = nullptr;
                state->edit_font = nullptr;
            }
            RemoveWindowSubclass(pane, PaneSubclassProcedure, kPaneSubclass);
            SetWindowLongPtrW(pane, GWLP_USERDATA, 0);
            break;
        default:
            break;
    }
    return DefSubclassProc(pane, message, wparam, lparam);
}

bool CreateDetailControls(HINSTANCE instance, HWND pane) noexcept {
    for (std::size_t index = 0U; index < kViewLabelIds.size(); ++index) {
        if (CreateControl(
                instance, pane, L"STATIC", L"", SS_LEFT | SS_CENTERIMAGE,
                kViewLabelIds[index]) == nullptr
            || CreateControl(
                   instance, pane, L"EDIT", L"",
                   WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL,
                   kViewEditIds[index]) == nullptr
            || CreateControl(
                   instance, pane, WC_COMBOBOXW, L"",
                   WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
                   kViewChoiceIds[index]) == nullptr) {
            return false;
        }
    }
    for (std::size_t index = 0U; index < kEffectLabelIds.size(); ++index) {
        if (CreateControl(
                instance, pane, L"STATIC", L"", SS_LEFT | SS_CENTERIMAGE,
                kEffectLabelIds[index]) == nullptr
            || CreateControl(
                   instance, pane, L"EDIT", L"",
                   WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL,
                   kEffectEditIds[index]) == nullptr) {
            return false;
        }
    }
    const std::array<const wchar_t*, 6U> fill_labels{
        UiText(UiStringId::Text0692),
        UiText(UiStringId::Text0919),
        UiText(UiStringId::Text1030),
        UiText(UiStringId::Text0114),
        UiText(UiStringId::Text0573),
        UiText(UiStringId::Text0628)};
    for (std::size_t index = 0U; index < kFillLabelIds.size(); ++index) {
        if (CreateControl(
                instance,
                pane,
                L"STATIC",
                fill_labels[index],
                SS_LEFT | SS_CENTERIMAGE,
                kFillLabelIds[index]) == nullptr) {
            return false;
        }
    }
    return CreateControl(
               instance, pane, L"STATIC", L"", SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_PAGE_TITLE) != nullptr
        && CreateControl(
               instance, pane, WC_COMBOBOXW, L"",
               WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
               IDC_FILL_OPERATION) != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"",
               WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL | ES_NUMBER,
               IDC_FILL_TOLERANCE) != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"",
               WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL | ES_NUMBER,
               IDC_FILL_GAP) != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"",
               WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL | ES_NUMBER,
               IDC_FILL_EXTENSION) != nullptr
        && CreateControl(
               instance, pane, WC_COMBOBOXW, L"",
               WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
               IDC_FILL_INCLUSION_MODE) != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"",
               WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL | ES_MULTILINE
                   | WS_VSCROLL,
               IDC_FILL_COLORS) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text0596),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_FILL_OVERFLOW) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text1033),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_FILL_DETACHED) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text0953),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_FILL_TRANSPARENT) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text0697),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_FILL_SELECTION) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text0065),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_FILL_LIGHT_BOUNDARY) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text0064),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_FILL_LIGHT_COLOR) != nullptr
        && CreateControl(
               instance, pane, L"STATIC", UiText(UiStringId::Text0507),
               SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_EFFECT_CHANNEL_LABEL) != nullptr
        && CreateControl(
               instance, pane, L"STATIC", UiText(UiStringId::Text0692),
               SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_EFFECT_MODE_LABEL) != nullptr
        && CreateControl(
               instance, pane, L"STATIC", UiText(UiStringId::Text0775),
               SS_LEFT,
               IDC_TOOL_OPTIONS_EFFECT_POINTS_LABEL) != nullptr
        && CreateControl(
               instance, pane, WC_COMBOBOXW, L"",
               WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
               IDC_EFFECT_CHANNEL) != nullptr
        && CreateControl(
               instance, pane, WC_COMBOBOXW, L"",
               WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
               IDC_EFFECT_MODE) != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"",
               WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL | ES_MULTILINE
                   | WS_VSCROLL,
               IDC_EFFECT_POINTS) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text0314),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_EFFECT_OPTION1) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Text0034),
               WS_TABSTOP | BS_AUTOCHECKBOX, IDC_EFFECT_OPTION2) != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Apply),
               WS_TABSTOP | BS_PUSHBUTTON, IDC_TOOL_OPTIONS_APPLY) != nullptr;
}

LRESULT CALLBACK FlyoutWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<ToolOptionsFlyoutState*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));
    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        state = create == nullptr
            ? nullptr
            : static_cast<ToolOptionsFlyoutState*>(create->lpCreateParams);
        if (state == nullptr) return FALSE;
        SetWindowLongPtrW(
            window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
        state->window = window;
    }
    switch (message) {
        case WM_SIZE:
            if (state != nullptr && state->pane != nullptr) {
                RECT client{};
                if (GetClientRect(window, &client) != FALSE) {
                    SetWindowPos(
                        state->pane,
                        nullptr,
                        0,
                        0,
                        client.right,
                        client.bottom,
                        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER
                            | SWP_SHOWWINDOW);
                }
            }
            return 0;
        case WM_CLOSE:
            ShowWindow(window, SW_HIDE);
            return 0;
        case WM_COMMAND:
            if (LOWORD(wparam) == IDCANCEL) {
                ShowWindow(window, SW_HIDE);
                return 0;
            }
            break;
        case WM_GETMINMAXINFO: {
            auto* limits = reinterpret_cast<MINMAXINFO*>(lparam);
            if (limits != nullptr) {
                const UINT dpi = GetDpiForWindow(window);
                limits->ptMinTrackSize.x = ScaleForDpi(320, dpi);
                limits->ptMinTrackSize.y = ScaleForDpi(320, dpi);
            }
            return 0;
        }
        case WM_DPICHANGED:
            if (const auto* suggested = reinterpret_cast<const RECT*>(lparam);
                suggested != nullptr) {
                SetWindowPos(
                    window,
                    nullptr,
                    suggested->left,
                    suggested->top,
                    suggested->right - suggested->left,
                    suggested->bottom - suggested->top,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
            }
            return 0;
        case WM_NCDESTROY:
            if (state != nullptr) {
                state->window = nullptr;
                state->pane = nullptr;
                state->anchor = nullptr;
                state->command = 0U;
            }
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            break;
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

bool RegisterFlyoutClass(HINSTANCE instance) noexcept {
    WNDCLASSEXW existing{};
    existing.cbSize = sizeof(existing);
    if (GetClassInfoExW(instance, kFlyoutClassName, &existing) != FALSE) return true;
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.style = CS_HREDRAW | CS_VREDRAW;
    window_class.lpfnWndProc = FlyoutWindowProcedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.hbrBackground = GetSysColorBrush(COLOR_BTNFACE);
    window_class.lpszClassName = kFlyoutClassName;
    return RegisterClassExW(&window_class) != 0U;
}

ToolOptionsFlyoutState* FlyoutState(HWND flyout) noexcept {
    return flyout == nullptr
        ? nullptr
        : reinterpret_cast<ToolOptionsFlyoutState*>(
              GetWindowLongPtrW(flyout, GWLP_USERDATA));
}

void PositionFlyout(HWND flyout, HWND anchor) noexcept {
    if (flyout == nullptr) return;
    const UINT dpi = anchor == nullptr ? GetDpiForWindow(flyout) : GetDpiForWindow(anchor);
    RECT anchor_bounds{};
    if (anchor == nullptr || GetWindowRect(anchor, &anchor_bounds) == FALSE) {
        HWND owner = GetWindow(flyout, GW_OWNER);
        GetWindowRect(owner, &anchor_bounds);
    }
    const int width = ScaleForDpi(380, dpi);
    const int height = ScaleForDpi(620, dpi);
    RECT desired{
        anchor_bounds.right + ScaleForDpi(8, dpi),
        anchor_bounds.top,
        anchor_bounds.right + ScaleForDpi(8, dpi) + width,
        anchor_bounds.top + height};
    const HMONITOR monitor = MonitorFromRect(&anchor_bounds, MONITOR_DEFAULTTONEAREST);
    MONITORINFO info{};
    info.cbSize = sizeof(info);
    if (GetMonitorInfoW(monitor, &info) != FALSE) {
        if (desired.right > info.rcWork.right) {
            desired.left = anchor_bounds.left - ScaleForDpi(8, dpi) - width;
            desired.right = desired.left + width;
        }
        desired.left = std::clamp(
            static_cast<int>(desired.left),
            static_cast<int>(info.rcWork.left),
            std::max(static_cast<int>(info.rcWork.left),
                     static_cast<int>(info.rcWork.right) - width));
        desired.top = std::clamp(
            static_cast<int>(desired.top),
            static_cast<int>(info.rcWork.top),
            std::max(static_cast<int>(info.rcWork.top),
                     static_cast<int>(info.rcWork.bottom) - height));
    }
    SetWindowPos(
        flyout,
        HWND_TOP,
        desired.left,
        desired.top,
        width,
        height,
        SWP_NOOWNERZORDER);
}

}  // namespace

HWND CreateToolOptionsPane(
    HINSTANCE instance,
    HWND parent,
    ToolOptionsPaneState& state) noexcept {
    const HWND pane = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        L"STATIC",
        nullptr,
        WS_CHILD | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        parent,
        nullptr,
        instance,
        nullptr);
    if (pane == nullptr
        || CreateControl(
               instance, pane, L"STATIC", UiText(UiStringId::ToolGeneric),
               WS_VISIBLE | SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_LABEL) == nullptr
        || CreateControl(
               instance, pane, L"STATIC", UiText(UiStringId::ToolEraseTarget),
               WS_VISIBLE | SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_TARGET_LABEL) == nullptr
        || CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::MainLine),
               WS_VISIBLE | WS_TABSTOP | WS_GROUP | BS_AUTORADIOBUTTON
                   | BS_PUSHLIKE,
               IDC_TOOL_OPTIONS_TARGET_MAIN_LINE) == nullptr
        || CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Coloring),
               WS_VISIBLE | WS_TABSTOP | BS_AUTORADIOBUTTON | BS_PUSHLIKE,
               IDC_TOOL_OPTIONS_TARGET_COLOR) == nullptr
        || CreateControl(
               instance, pane, L"STATIC", UiText(UiStringId::ToolDiameter),
               WS_VISIBLE | SS_LEFT | SS_CENTERIMAGE,
               IDC_TOOL_OPTIONS_DIAMETER_LABEL) == nullptr
        || CreateControl(
               instance, pane, L"EDIT", L"8.0",
               WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
               IDC_TOOL_OPTIONS_DIAMETER) == nullptr
        || CreateControl(
               instance, pane, WC_COMBOBOXW, L"",
               WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST | WS_VSCROLL,
               IDC_TOOL_OPTIONS_BRUSH_SHAPE) == nullptr
        || CreateControl(
               instance, pane, L"EDIT", L"0",
               WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL
                   | ES_NUMBER,
               IDC_TOOL_OPTIONS_BRUSH_SMOOTHING) == nullptr
        || CreateControl(
               instance,
               pane,
               L"BUTTON",
               UiText(UiStringId::ToolFillMatchingStartColor),
               WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
               IDC_TOOL_OPTIONS_BRUSH_START_COLOR) == nullptr
        || !CreateDetailControls(instance, pane)) {
        if (pane != nullptr) DestroyWindow(pane);
        return nullptr;
    }
    SetWindowSubclass(
        pane,
        PaneSubclassProcedure,
        kPaneSubclass,
        reinterpret_cast<DWORD_PTR>(&state));
    SetWindowLongPtrW(pane, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(&state));
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_SHAPE,
        CB_ADDSTRING,
        0,
        reinterpret_cast<LPARAM>(UiText(UiStringId::Text0508)));
    SendDlgItemMessageW(
        pane,
        IDC_TOOL_OPTIONS_BRUSH_SHAPE,
        CB_ADDSTRING,
        0,
        reinterpret_cast<LPARAM>(UiText(UiStringId::Text0821)));
    UpdateFont(pane, state);
    UpdateToolOptionsPane(
        pane, state.active_tool, state.active_plane, state.diameter, state.brush);
    HideDetailControls(pane);
    return pane;
}

void UpdateToolOptionsPane(
    HWND pane,
    std::uint32_t active_tool,
    InkpodPlaneKind active_plane,
    float diameter,
    const InkpodEditorBrushOptions& brush) noexcept {
    auto* state = pane == nullptr
        ? nullptr
        : reinterpret_cast<ToolOptionsPaneState*>(
              GetWindowLongPtrW(pane, GWLP_USERDATA));
    if (state == nullptr) return;
    const bool preserve_diameter = state->editing
        && state->active_tool == active_tool && CanEditDiameter(active_tool);
    const bool preserve_smoothing = state->editing_smoothing
        && state->active_tool == active_tool && active_tool == INKPOD_TOOL_BRUSH;
    state->updating = true;
    state->active_tool = active_tool;
    state->active_plane = active_plane;
    state->diameter = diameter;
    state->brush = brush;
    SetDlgItemTextW(pane, IDC_TOOL_OPTIONS_LABEL, ToolLabel(active_tool));
    const bool show_erase_target = active_tool == INKPOD_TOOL_ERASER;
    SetVisible(pane, IDC_TOOL_OPTIONS_TARGET_LABEL, show_erase_target);
    SetVisible(pane, IDC_TOOL_OPTIONS_TARGET_MAIN_LINE, show_erase_target);
    SetVisible(pane, IDC_TOOL_OPTIONS_TARGET_COLOR, show_erase_target);
    CheckDlgButton(
        pane,
        IDC_TOOL_OPTIONS_TARGET_MAIN_LINE,
        active_plane == INKPOD_PLANE_MAIN_LINE ? BST_CHECKED : BST_UNCHECKED);
    CheckDlgButton(
        pane,
        IDC_TOOL_OPTIONS_TARGET_COLOR,
        active_plane == INKPOD_PLANE_COLOR ? BST_CHECKED : BST_UNCHECKED);

    const bool has_diameter = HasDiameter(active_tool);
    SetVisible(pane, IDC_TOOL_OPTIONS_DIAMETER_LABEL, has_diameter);
    SetVisible(pane, IDC_TOOL_OPTIONS_DIAMETER, has_diameter);
    EnableWindow(
        GetDlgItem(pane, IDC_TOOL_OPTIONS_DIAMETER),
        CanEditDiameter(active_tool) ? TRUE : FALSE);
    if (!preserve_diameter) {
        std::array<wchar_t, 32U> value{};
        const float shown = active_tool == INKPOD_TOOL_PENCIL
            ? kPencilToolDiameter
            : diameter;
        _snwprintf_s(
            value.data(), value.size(), _TRUNCATE, L"%.1f", shown);
        SetDlgItemTextW(pane, IDC_TOOL_OPTIONS_DIAMETER, value.data());
    }
    const bool show_brush = active_tool == INKPOD_TOOL_BRUSH;
    SetVisible(pane, IDC_TOOL_OPTIONS_BRUSH_SHAPE, show_brush);
    SetVisible(pane, IDC_TOOL_OPTIONS_BRUSH_SMOOTHING, show_brush);
    SetVisible(pane, IDC_TOOL_OPTIONS_BRUSH_START_COLOR, show_brush);
    if (show_brush) {
        SendDlgItemMessageW(
            pane,
            IDC_TOOL_OPTIONS_BRUSH_SHAPE,
            CB_SETCURSEL,
            brush.shape == INKPOD_BRUSH_SQUARE ? 1 : 0,
            0);
        if (!preserve_smoothing) {
            std::array<wchar_t, 32U> value{};
            _snwprintf_s(
                value.data(), value.size(), _TRUNCATE, L"%u",
                static_cast<unsigned>(brush.smoothing));
            SetDlgItemTextW(
                pane, IDC_TOOL_OPTIONS_BRUSH_SMOOTHING, value.data());
        }
        CheckDlgButton(
            pane,
            IDC_TOOL_OPTIONS_BRUSH_START_COLOR,
            brush.start_color == INKPOD_START_COLOR_EXACT_NATIVE
                ? BST_CHECKED
                : BST_UNCHECKED);
    }
    state->updating = false;
    LayoutPane(pane);
}

void RefreshToolOptionsDetail(HWND pane, UINT command) noexcept {
    auto* state = pane == nullptr
        ? nullptr
        : reinterpret_cast<ToolOptionsPaneState*>(
              GetWindowLongPtrW(pane, GWLP_USERDATA));
    if (state == nullptr) return;
    state->detail_command = command;
    ToolOptionsDetailModel detail{};
    if (command != 0U && state->query_detail != nullptr
        && state->query_detail(state->context, command, detail)) {
        state->detail = std::move(detail);
    } else {
        state->detail = {};
    }
    PopulateDetailControls(pane, *state);
    LayoutPane(pane);
}

HWND CreateToolOptionsFlyout(
    HINSTANCE instance,
    HWND owner,
    ToolOptionsFlyoutState& flyout,
    ToolOptionsPaneState& pane_state) noexcept {
    if (!RegisterFlyoutClass(instance)) return nullptr;
    flyout.pane_state = &pane_state;
    const HWND window = CreateWindowExW(
        0,
        kFlyoutClassName,
        UiText(UiStringId::ToolGeneric),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        ScaleForDpi(380, GetDpiForWindow(owner)),
        ScaleForDpi(620, GetDpiForWindow(owner)),
        owner,
        nullptr,
        instance,
        &flyout);
    if (window == nullptr) return nullptr;
    const HWND pane = CreateToolOptionsPane(instance, window, pane_state);
    if (pane == nullptr) {
        DestroyWindow(window);
        return nullptr;
    }
    flyout.window = window;
    flyout.pane = pane;
    RECT client{};
    GetClientRect(window, &client);
    SetWindowPos(
        pane,
        nullptr,
        0,
        0,
        client.right,
        client.bottom,
        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER | SWP_SHOWWINDOW);
    ShowWindow(window, SW_HIDE);
    return window;
}

bool ShowToolOptionsFlyout(HWND flyout, HWND anchor, UINT command) noexcept {
    ToolOptionsFlyoutState* state = FlyoutState(flyout);
    if (state == nullptr || state->pane == nullptr || command == 0U) return false;
    state->anchor = anchor;
    state->command = command;
    RefreshToolOptionsDetail(state->pane, command);
    SetWindowTextW(
        flyout,
        state->pane_state == nullptr
            ? UiText(UiStringId::ToolGeneric)
            : ToolLabel(state->pane_state->active_tool));
    PositionFlyout(flyout, anchor);
    ShowWindow(flyout, SW_SHOWNORMAL);
    SetForegroundWindow(flyout);
    for (HWND child = GetWindow(state->pane, GW_CHILD);
         child != nullptr;
         child = GetWindow(child, GW_HWNDNEXT)) {
        const DWORD style = static_cast<DWORD>(GetWindowLongPtrW(child, GWL_STYLE));
        if (IsWindowVisible(child) != FALSE && IsWindowEnabled(child) != FALSE
            && (style & WS_TABSTOP) != 0U) {
            SetFocus(child);
            break;
        }
    }
    return true;
}

bool ToggleToolOptionsFlyout(HWND flyout, HWND anchor, UINT command) noexcept {
    ToolOptionsFlyoutState* state = FlyoutState(flyout);
    if (state == nullptr) return false;
    if (IsWindowVisible(flyout) != FALSE && state->command == command) {
        HideToolOptionsFlyout(flyout);
        return true;
    }
    return ShowToolOptionsFlyout(flyout, anchor, command);
}

void HideToolOptionsFlyout(HWND flyout) noexcept {
    if (flyout != nullptr) ShowWindow(flyout, SW_HIDE);
}

bool IsToolOptionsFlyoutVisible(HWND flyout) noexcept {
    return flyout != nullptr && IsWindowVisible(flyout) != FALSE;
}

}  // namespace inkpod::windows::ui::panes
