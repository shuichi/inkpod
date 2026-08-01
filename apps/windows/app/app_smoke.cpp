#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <shlobj.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <climits>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <cwctype>
#include <functional>
#include <memory>
#include <optional>
#include <span>
#include <string>
#include <utility>
#include <vector>

#include "app/application_host.h"
#include "canvas.h"
#include "app/clipboard_adapter.h"
#include "app/core_host.h"
#include "app/document_shell.h"
#include "inkpod/core_ffi.h"
#include "app/resource.h"
#include "ui/dialogs/about_dialog.h"
#include "ui/dialogs/basic_dialogs.h"
#include "ui/dialogs/batch_dialog.h"
#include "ui/dialogs/effects_dialogs.h"
#include "ui/dialogs/layer_palette.h"
#include "ui/command_catalog.h"
#include "ui/command_state.h"
#include "ui/shortcut_controller.h"
#include "ui/panes/document_panes.h"
#include "ui/panes/color_panes.h"
#include "ui/tools/fill_controller.h"
#include "ui/tools/floating_paste_controller.h"
#include "ui/tools/selection_controller.h"
#include "ui/tools/tool_state.h"
#include "ui/tools/view_controller.h"
#include "ui/tools/vector_controller.h"
#include "ui/effects_controller.h"
#include "ui/batch_controller.h"
#include "ui/main_window.h"
#include "ui/main_window_runtime.h"

#include "app/app_smoke.h"

namespace inkpod::windows::ui::runtime {

using inkpod::app::ApplicationHost;
using inkpod::app::InkpodClipboardFormat;
using inkpod::app::NewestPrivateRecovery;
using inkpod::app::PublishStandardClipboard;
using inkpod::app::ReadBoundedFile;
using inkpod::app::WriteFileAtomically;
using inkpod::app::CommandTimerKind;
using inkpod::windows::ui::tools::TransitionActiveTool;
using inkpod::windows::ui::tools::kInteractionEffectAirbrush;

constexpr wchar_t kVectorStrokePlaneRequired[] =
    L"ベクター描画には、ベクター主線または色トレース線プレーンの選択が必要です。";

bool CommandSurfacesMatchComputedState(const ApplicationHost& state) noexcept;
InkpodStatus CreateCell(ApplicationHost& state, std::uint32_t width, std::uint32_t height, std::uint32_t dpi_milli) noexcept;
bool DispatchEnabledCommand(
    ApplicationHost& state,
    HWND window,
    UINT command,
    std::optional<inkpod::app::PaneInstanceId> pane = std::nullopt) noexcept;
std::optional<LRESULT> IssueCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM lparam,
    std::optional<inkpod::app::PaneInstanceId> pane) noexcept;
InkpodDocumentInfo EmptyDocumentInfo() noexcept;
InkpodStatus ApplyView(
    ApplicationHost& state,
    InkpodViewCommandKind kind,
    double value1,
    double value2,
    double value3 = 0.0,
    double value4 = 0.0) noexcept;
bool ActivateDocumentTab(
    ApplicationHost& state,
    inkpod::app::DocumentViewId view) noexcept;
bool ConfirmAllDocuments(ApplicationHost& state) noexcept;
InkpodStatus FinishVectorCanvasGesture(ApplicationHost& state) noexcept;
InkpodStatus FitCanvas(ApplicationHost& state, InkpodViewCommandKind kind) noexcept;
InkpodStatus OpenFromPath(ApplicationHost& state, const std::wstring& path) noexcept;
void PumpPendingWindowMessages() noexcept;
bool QueryDocument(ApplicationHost& state, InkpodDocumentInfo& info) noexcept;
bool QueryLightTableItem(ApplicationHost& state, std::uint32_t index, InkpodLightTableItemInfo& output) noexcept;
bool QueueAutosave(
    ApplicationHost& state,
    const inkpod::app::CommandContext& context,
    const std::wstring& path) noexcept;
bool RefreshColorPanes(ApplicationHost& state) noexcept;
bool RefreshLightTablePane(ApplicationHost& state) noexcept;
bool RefreshSequencePane(ApplicationHost& state) noexcept;
bool RefreshTreePane(ApplicationHost& state) noexcept;
void ResetUiForNewActiveDocument(ApplicationHost& state) noexcept;
bool ResolveConfiguredShortcut(ApplicationHost& state, std::uint32_t virtual_key, std::uint32_t modifiers, UINT& menu_command) noexcept;
bool SamePersistentMetadata(const InkpodDocumentInfo& left, const InkpodDocumentInfo& right) noexcept;
InkpodStatus SaveToPath(ApplicationHost& state, const std::wstring& path) noexcept;

bool MenuLeavesHaveAssignedShortcuts(
    HMENU menu,
    std::span<const InkpodShortcutSequence> bindings,
    std::size_t& leaf_count) noexcept {
    const int count = GetMenuItemCount(menu);
    for (int position = 0; position < count; ++position) {
        MENUITEMINFOW item{};
        item.cbSize = sizeof(item);
        item.fMask = MIIM_ID | MIIM_FTYPE | MIIM_SUBMENU;
        if (GetMenuItemInfoW(menu, static_cast<UINT>(position), TRUE, &item) == FALSE) {
            return false;
        }
        if (item.hSubMenu != nullptr) {
            if (!MenuLeavesHaveAssignedShortcuts(item.hSubMenu, bindings, leaf_count)) {
                return false;
            }
            continue;
        }
        if ((item.fType & MFT_SEPARATOR) != 0U) {
            continue;
        }
        ++leaf_count;
        if (windows::ui::FindShortcutSequence(bindings, item.wID) == nullptr) {
            return false;
        }
        const int length = GetMenuStringW(
            menu, static_cast<UINT>(position), nullptr, 0, MF_BYPOSITION);
        try {
            std::wstring label(static_cast<std::size_t>(length) + 1U, L'\0');
            GetMenuStringW(
                menu,
                static_cast<UINT>(position),
                label.data(),
                static_cast<int>(label.size()),
                MF_BYPOSITION);
            if (label.find(L'\t') == std::wstring::npos) {
                return false;
            }
        } catch (const std::bad_alloc&) {
            return false;
        }
    }
    return true;
}

bool ReadDocumentTabLabel(HWND tabs, int index, std::wstring& output) noexcept {
    if (tabs == nullptr || index < 0) {
        return false;
    }
    std::array<wchar_t, 1024U> buffer{};
    TCITEMW item{};
    item.mask = TCIF_TEXT;
    item.pszText = buffer.data();
    item.cchTextMax = static_cast<int>(buffer.size());
    if (TabCtrl_GetItem(tabs, index, &item) == FALSE) {
        return false;
    }
    try {
        output.assign(buffer.data());
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

bool DocumentTabsMatchRegistry(const ApplicationHost& state) noexcept {
    const HWND tabs = state.Workspace().windows.document_tabs;
    if (tabs == nullptr) {
        return false;
    }
    std::size_t expected_count{};
    for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
        const auto* document = state.Documents().SessionAt(index);
        if (document != nullptr) {
            expected_count += document->ViewCount();
        }
    }
    const int tab_count = TabCtrl_GetItemCount(tabs);
    if (tab_count < 0 || static_cast<std::size_t>(tab_count) != expected_count) {
        return false;
    }
    std::array<inkpod::app::DocumentViewId, 4096U> seen{};
    if (expected_count > seen.size()) {
        return false;
    }
    inkpod::app::DocumentViewId selected{};
    const int selected_index = TabCtrl_GetCurSel(tabs);
    for (int index = 0; index < tab_count; ++index) {
        TCITEMW item{};
        item.mask = TCIF_PARAM;
        if (TabCtrl_GetItem(tabs, index, &item) == FALSE) {
            return false;
        }
        const inkpod::app::DocumentViewId view{
            static_cast<std::uint64_t>(item.lParam)};
        if (!view || state.Documents().FindByView(view) == nullptr) {
            return false;
        }
        for (int prior = 0; prior < index; ++prior) {
            if (seen[static_cast<std::size_t>(prior)] == view) {
                return false;
            }
        }
        seen[static_cast<std::size_t>(index)] = view;
        if (index == selected_index) {
            selected = view;
        }
    }
    return selected == state.routing.targets.ActiveDocumentView();
}

bool SameCommandStates(
    const CommandStateSet& left,
    const CommandStateSet& right) noexcept {
    for (std::size_t index = 0U; index < left.size(); ++index) {
        if (left[index].command != right[index].command
            || left[index].owner != right[index].owner
            || left[index].enabled != right[index].enabled
            || left[index].checked != right[index].checked) {
            return false;
        }
    }
    return true;
}

struct ViewOptionsValidationProbe {
    std::uint32_t calls{};
    bool saw_expected_values{};
};

const wchar_t* RejectViewOptionsForSmoke(
    void* context,
    const std::array<std::int32_t, 4U>& values,
    std::uint32_t value_count) noexcept {
    auto* probe = static_cast<ViewOptionsValidationProbe*>(context);
    if (probe != nullptr) {
        ++probe->calls;
        probe->saw_expected_values = value_count == 2U
            && values[0] == INKPOD_TYPED_PLANE_RASTER
            && values[1] == INKPOD_STORAGE_RGBA8;
    }
    return L"smoke validation rejection";
}

int RunDrawingPersistenceSmoke(ApplicationHost& state) noexcept {
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_HELP_ABOUT, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_HELP_ABOUT, 0) != 1) {
        return 29;
    }
    constexpr std::array<ViewOptionsDialogState::Choice, 2U> plane_kind_choices{{
        {L"主線", INKPOD_TYPED_PLANE_MAIN_LINE},
        {L"ラスター", INKPOD_TYPED_PLANE_RASTER},
    }};
    constexpr std::array<ViewOptionsDialogState::Choice, 2U> plane_format_choices{{
        {L"2値", INKPOD_STORAGE_BINARY8},
        {L"RGBA", INKPOD_STORAGE_RGBA8},
    }};
    ViewOptionsDialogState dropdown_dialog{};
    dropdown_dialog.title = L"共通ダイアログ smoke";
    dropdown_dialog.labels = {L"種類", L"形式", L"不透明度", nullptr};
    dropdown_dialog.values = {
        INKPOD_TYPED_PLANE_RASTER, INKPOD_STORAGE_RGBA8, 75, 0};
    dropdown_dialog.choices[0] = plane_kind_choices.data();
    dropdown_dialog.choice_counts[0] =
        static_cast<std::uint32_t>(plane_kind_choices.size());
    dropdown_dialog.choices[1] = plane_format_choices.data();
    dropdown_dialog.choice_counts[1] =
        static_cast<std::uint32_t>(plane_format_choices.size());
    dropdown_dialog.value_count = 3U;
    dropdown_dialog.close_immediately = true;
    if (ShowViewOptions(
            state.lifetime.instance,
            state.Workspace().windows.window,
            true,
            dropdown_dialog)
            != IDOK
        || dropdown_dialog.values[0] != INKPOD_TYPED_PLANE_RASTER
        || dropdown_dialog.values[1] != INKPOD_STORAGE_RGBA8
        || dropdown_dialog.values[2] != 75
        || !dropdown_dialog.centered_on_owner) {
        return 830;
    }
    ViewOptionsValidationProbe validation_probe{};
    ViewOptionsDialogState rejected_dialog = dropdown_dialog;
    rejected_dialog.value_count = 2U;
    rejected_dialog.validation_context = &validation_probe;
    rejected_dialog.validate = RejectViewOptionsForSmoke;
    if (ShowViewOptions(
            state.lifetime.instance,
            state.Workspace().windows.window,
            true,
            rejected_dialog)
            != IDCANCEL
        || validation_probe.calls != 1U
        || !validation_probe.saw_expected_values) {
        return 831;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WORKSPACE_RESET, 0)
            != 1
        || state.Workspace().tools.palette == nullptr
        || state.Workspace().windows.tool_options == nullptr
        || state.Workspace().windows.color_pane == nullptr
        || state.Workspace().panes.layer_palette == nullptr) {
        return 727;
    }
    const std::array<HWND, 4U> workspace_panes{
        state.Workspace().tools.palette,
        state.Workspace().windows.tool_options,
        state.Workspace().windows.color_pane,
        state.Workspace().panes.layer_palette};
    for (const HWND pane : workspace_panes) {
        const auto style = static_cast<DWORD>(
            GetWindowLongPtrW(pane, GWL_STYLE));
        if ((style & WS_CHILD) == 0U
            || GetParent(pane) != state.Workspace().windows.window) {
            return 728;
        }
    }
    if (ToolPaletteEntries().size() != kToolPaletteEntryCount
        || kToolPaletteEntryCount != 20U) {
        return 729;
    }
    if (!std::all_of(
            ToolPaletteEntries().begin(),
            ToolPaletteEntries().end(),
            [&](const ToolPaletteEntry& entry) {
                const HWND button = GetDlgItem(state.Workspace().tools.palette, entry.command);
                const auto glyph_length =
                    entry.glyph == nullptr ? 0U : std::wcslen(entry.glyph);
                return glyph_length >= 2U && glyph_length <= 8U
                    && std::wcschr(entry.glyph, L' ') == nullptr
                    && std::wcschr(entry.glyph, L'　') == nullptr
                    && button != nullptr
                    && (static_cast<DWORD>(
                            GetWindowLongPtrW(button, GWL_STYLE))
                            & BS_TYPEMASK)
                        == BS_OWNERDRAW;
            })) {
        return 746;
    }
    const HWND diameter_edit =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_DIAMETER);
    const HWND diameter_label =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_DIAMETER_LABEL);
    const HWND erase_target_label =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_TARGET_LABEL);
    const HWND erase_main_line =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_TARGET_MAIN_LINE);
    const HWND erase_color =
        GetDlgItem(state.Workspace().windows.tool_options, IDC_TOOL_OPTIONS_TARGET_COLOR);
    if (diameter_edit == nullptr || diameter_label == nullptr
        || erase_target_label == nullptr
        || erase_main_line == nullptr || erase_color == nullptr) {
        return 747;
    }
    const auto diameter_text_is = [&](const wchar_t* expected) noexcept {
        std::array<wchar_t, 32U> value{};
        return GetWindowTextW(
                   diameter_edit,
                   value.data(),
                   static_cast<int>(value.size()))
                > 0
            && std::wcscmp(value.data(), expected) == 0;
    };
    RECT diameter_label_bounds{};
    RECT diameter_edit_bounds{};
    const HFONT diameter_label_font = reinterpret_cast<HFONT>(
        SendMessageW(diameter_label, WM_GETFONT, 0, 0));
    const HFONT diameter_edit_font = reinterpret_cast<HFONT>(
        SendMessageW(diameter_edit, WM_GETFONT, 0, 0));
    LOGFONTW diameter_label_font_info{};
    LOGFONTW diameter_edit_font_info{};
    if (GetWindowRect(diameter_label, &diameter_label_bounds) == FALSE
        || GetWindowRect(diameter_edit, &diameter_edit_bounds) == FALSE
        || diameter_label_font == nullptr || diameter_edit_font == nullptr
        || GetObjectW(
               diameter_label_font,
               static_cast<int>(sizeof(diameter_label_font_info)),
               &diameter_label_font_info)
            != static_cast<int>(sizeof(diameter_label_font_info))
        || GetObjectW(
               diameter_edit_font,
               static_cast<int>(sizeof(diameter_edit_font_info)),
               &diameter_edit_font_info)
            != static_cast<int>(sizeof(diameter_edit_font_info))
        || diameter_edit_bounds.bottom - diameter_edit_bounds.top
            >= diameter_label_bounds.bottom - diameter_label_bounds.top
        || (diameter_edit_bounds.top + diameter_edit_bounds.bottom)
                - (diameter_label_bounds.top + diameter_label_bounds.bottom)
            < -1
        || (diameter_edit_bounds.top + diameter_edit_bounds.bottom)
                - (diameter_label_bounds.top + diameter_label_bounds.bottom)
            > 1
        || diameter_label_font_info.lfHeight >= 0
        || diameter_edit_font_info.lfHeight >= 0
        || diameter_edit_font_info.lfHeight
            <= diameter_label_font_info.lfHeight) {
        return 765;
    }
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_PENCIL
        || IsWindowEnabled(diameter_edit) != FALSE
        || !diameter_text_is(L"1.0")
        || IsWindowVisible(erase_target_label) != FALSE
        || IsWindowVisible(erase_main_line) != FALSE
        || IsWindowVisible(erase_color) != FALSE) {
        return 750;
    }
    const HWND main_line_label =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_MAIN_LINE_LABEL);
    const HWND main_line_swatch =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_MAIN_LINE_SWATCH);
    const HWND drawing_swatch =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_SWATCH);
    const HWND drawing_label =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_DRAWING_LABEL);
    const HWND color_picker =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_PICKER);
    const HWND color_eyedropper =
        GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_EYEDROPPER);
    if (GetDlgItem(state.Workspace().windows.color_pane, IDC_COLOR_TABS) == nullptr
        || GetDlgItem(state.Workspace().windows.color_pane, IDC_PALETTE_LIST) == nullptr
        || main_line_label == nullptr || main_line_swatch == nullptr
        || drawing_swatch == nullptr
        || drawing_label == nullptr || color_picker == nullptr
        || color_eyedropper == nullptr
        || (GetWindowLongPtrW(color_picker, GWL_STYLE) & WS_VISIBLE) == 0
        || (GetWindowLongPtrW(color_eyedropper, GWL_STYLE) & WS_VISIBLE) == 0) {
        return 748;
    }
    RECT combined_swatch_bounds{};
    std::array<wchar_t, 32U> eyedropper_text{};
    GetWindowTextW(
        color_eyedropper,
        eyedropper_text.data(),
        static_cast<int>(eyedropper_text.size()));
    if (GetWindowRect(main_line_swatch, &combined_swatch_bounds) == FALSE
        || combined_swatch_bounds.right - combined_swatch_bounds.left
            <= combined_swatch_bounds.bottom - combined_swatch_bounds.top
        || (GetWindowLongPtrW(drawing_swatch, GWL_STYLE) & WS_VISIBLE) != 0
        || state.Workspace().panes.color_pane.change_main_line_color == nullptr
        || (GetWindowLongPtrW(color_eyedropper, GWL_STYLE) & BS_TYPEMASK)
            == BS_OWNERDRAW
        || std::wcscmp(eyedropper_text.data(), L"スポイト") != 0) {
        return 783;
    }
    const auto is_opaque_black = [](const InkpodColorValue& color) noexcept {
        return color.depth == INKPOD_COLOR_DEPTH_8 && color.red == 0U
            && color.green == 0U && color.blue == 0U && color.alpha == 255U;
    };
    const auto same_color = [](
                                const InkpodColorValue& left,
                                const InkpodColorValue& right) noexcept {
        return left.depth == right.depth && left.red == right.red
            && left.green == right.green && left.blue == right.blue
            && left.alpha == right.alpha;
    };
    std::array<wchar_t, 64U> main_line_text{};
    std::array<wchar_t, 64U> drawing_text{};
    std::array<wchar_t, 64U> picker_text{};
    GetWindowTextW(
        main_line_label,
        main_line_text.data(),
        static_cast<int>(main_line_text.size()));
    GetWindowTextW(
        drawing_label,
        drawing_text.data(),
        static_cast<int>(drawing_text.size()));
    GetWindowTextW(
        color_picker,
        picker_text.data(),
        static_cast<int>(picker_text.size()));
    if (!is_opaque_black(state.Workspace().tools.drawing_color)
        || state.Workspace().tools.color_rgba != UINT32_C(0x000000ff)
        || !is_opaque_black(state.Workspace().panes.main_line_color)
        || !is_opaque_black(state.Workspace().panes.color_pane.main_line_color)
        || std::wcsstr(main_line_text.data(), L"主線色") == nullptr
        || std::wcsstr(main_line_text.data(), L"#000000FF") == nullptr
        || std::wcsstr(drawing_text.data(), L"彩色用描画色") == nullptr
        || std::wcsstr(drawing_text.data(), L"#000000FF") == nullptr
        || std::wcsstr(picker_text.data(), L"色相") == nullptr
        || std::wcsstr(picker_text.data(), L"不透明度") == nullptr) {
        return 762;
    }
    RECT swatch_client{};
    if (GetClientRect(main_line_swatch, &swatch_client) == FALSE) {
        return 784;
    }
    SendMessageW(
        main_line_swatch,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(
            (swatch_client.right - swatch_client.left) * 29 / 100,
            (swatch_client.bottom - swatch_client.top) * 34 / 100));
    if (!state.Workspace().panes.color_pane.picker_targets_main_line) {
        return 785;
    }
    SendMessageW(
        main_line_swatch,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(
            (swatch_client.right - swatch_client.left) * 64 / 100,
            (swatch_client.bottom - swatch_client.top) * 62 / 100));
    if (state.Workspace().panes.color_pane.picker_targets_main_line) {
        return 787;
    }
    const InkpodColorValue picker_original_color = state.Workspace().tools.drawing_color;
    SendMessageW(color_picker, WM_KEYDOWN, VK_UP, 0);
    if (state.Workspace().tools.drawing_color.depth != INKPOD_COLOR_DEPTH_8
        || state.Workspace().tools.drawing_color.red == 0U
        || state.Workspace().tools.drawing_color.red != state.Workspace().tools.drawing_color.green
        || state.Workspace().tools.drawing_color.green != state.Workspace().tools.drawing_color.blue
        || state.Workspace().tools.drawing_color.alpha != UINT8_MAX) {
        return 766;
    }
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, picker_original_color);
    if (!is_opaque_black(state.Workspace().tools.drawing_color)) {
        return 767;
    }
    const InkpodColorValue picker_rgba16{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_16,
        UINT16_MAX,
        32768U,
        0U,
        32768U};
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, picker_rgba16);
    SendMessageW(color_picker, WM_KEYDOWN, VK_RIGHT, 0);
    if (state.Workspace().tools.drawing_color.depth != INKPOD_COLOR_DEPTH_16
        || state.Workspace().tools.drawing_color.alpha != picker_rgba16.alpha) {
        return 769;
    }
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, picker_original_color);
    SendMessageW(color_eyedropper, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool
        != inkpod::windows::ui::tools::kInteractionEyedropper) {
        return 768;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_PENCIL, 0);
    const InkpodColorValue alternate_main_line{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 17U, 34U, 51U, 255U};
    state.Workspace().panes.main_line_color = alternate_main_line;
    UpdateMenuState(state);
    GetWindowTextW(
        main_line_label,
        main_line_text.data(),
        static_cast<int>(main_line_text.size()));
    if (!same_color(state.Workspace().panes.main_line_color, alternate_main_line)
        || !same_color(state.Workspace().panes.color_pane.main_line_color, alternate_main_line)
        || std::wcsstr(main_line_text.data(), L"#112233FF") == nullptr) {
        return 763;
    }
    state.Workspace().panes.main_line_color = InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    UpdateMenuState(state);
    GetWindowTextW(
        main_line_label,
        main_line_text.data(),
        static_cast<int>(main_line_text.size()));
    if (!is_opaque_black(state.Workspace().panes.main_line_color)
        || !is_opaque_black(state.Workspace().panes.color_pane.main_line_color)
        || std::wcsstr(main_line_text.data(), L"#000000FF") == nullptr) {
        return 764;
    }
    if (GetDlgItem(state.Workspace().panes.layer_palette, IDC_LAYER_LIST) == nullptr
        || GetDlgItem(state.Workspace().panes.layer_palette, IDC_PLANE_LIST) == nullptr
        || GetDlgItem(state.Workspace().panes.layer_palette, IDC_LAYER_PLANE_SPLITTER)
            == nullptr) {
        return 749;
    }
    SetWindowPos(
        state.Workspace().windows.window,
        nullptr,
        0,
        0,
        1'800,
        1'000,
        SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW);
    RECT client{};
    if (GetClientRect(state.Workspace().windows.window, &client) == FALSE) {
        return 730;
    }
    LayoutMainChrome(
        state.Workspace().windows,
        false,
        client.right - client.left,
        client.bottom - client.top);
    UpdateMenuState(state);
    const auto checked = [&](UINT command) {
        return (GetMenuState(menu, command, MF_BYCOMMAND) & MF_CHECKED) != 0U;
    };
    if (!state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool)
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::ToolOptions)
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Color)
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Layer)
        || state.Workspace().windows.workspace.dock.Mirrored()
        || !checked(IDM_WINDOW_TOOL_PALETTE)
        || !checked(IDM_WINDOW_TOOL_OPTIONS)
        || !checked(IDM_WINDOW_COLOR_PANE)
        || !checked(IDM_WINDOW_LAYER_PALETTE)) {
        return 731;
    }
    RECT tool_bounds{};
    RECT workspace_canvas_bounds{};
    RECT color_bounds{};
    RECT layer_bounds{};
    if (GetWindowRect(state.Workspace().tools.palette, &tool_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.color_pane, &color_bounds) == FALSE
        || GetWindowRect(state.Workspace().panes.layer_palette, &layer_bounds) == FALSE) {
        return 732;
    }
    if (tool_bounds.right > workspace_canvas_bounds.left
        || workspace_canvas_bounds.right > color_bounds.left
        || color_bounds.left != layer_bounds.left
        || color_bounds.right != layer_bounds.right) {
        return 732;
    }
    const HWND brush_button = GetDlgItem(state.Workspace().tools.palette, IDM_TOOL_BRUSH);
    const HWND pencil_button = GetDlgItem(state.Workspace().tools.palette, IDM_TOOL_PENCIL);
    const HWND eraser_button = GetDlgItem(state.Workspace().tools.palette, IDM_TOOL_ERASER);
    SendMessageW(brush_button, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_BRUSH) {
        return 733;
    }
    if (IsWindowEnabled(diameter_edit) == FALSE
        || !diameter_text_is(L"8.0")) {
        return 751;
    }
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"20.0");
    UpdateMenuState(state);
    if (GetFocus() != diameter_edit || state.Workspace().tools.diameter != 8.0F
        || !diameter_text_is(L"20.0")) {
        return 758;
    }
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_LBUTTONDOWN,
        MK_LBUTTON,
        MAKELPARAM(0, 0));
    if (GetFocus() != state.Workspace().windows.canvas || state.Workspace().tools.diameter != 20.0F
        || !diameter_text_is(L"20.0")) {
        return 759;
    }
    ReleaseCapture();
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"256.0");
    SetFocus(state.Workspace().windows.canvas);
    if (state.Workspace().tools.diameter != panes::kMaximumToolDiameter
        || !diameter_text_is(L"256.0")) {
        return 752;
    }
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"256.1");
    SetFocus(state.Workspace().windows.canvas);
    if (state.Workspace().tools.diameter != panes::kMaximumToolDiameter
        || !diameter_text_is(L"256.0")) {
        return 753;
    }
    SendMessageW(eraser_button, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_ERASER
        || IsWindowEnabled(diameter_edit) == FALSE
        || !diameter_text_is(L"256.0")
        || IsWindowVisible(erase_target_label) == FALSE
        || IsWindowVisible(erase_main_line) == FALSE
        || IsWindowVisible(erase_color) == FALSE
        || SendMessageW(erase_main_line, BM_GETCHECK, 0, 0) != BST_CHECKED
        || SendMessageW(erase_color, BM_GETCHECK, 0, 0) != BST_UNCHECKED) {
        return 754;
    }
    SendMessageW(erase_color, BM_CLICK, 0, 0);
    InkpodDocumentInfo erase_target_info = EmptyDocumentInfo();
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_COLOR
        || SendMessageW(erase_main_line, BM_GETCHECK, 0, 0) != BST_UNCHECKED
        || SendMessageW(erase_color, BM_GETCHECK, 0, 0) != BST_CHECKED
        || !QueryDocument(state, erase_target_info)
        || erase_target_info.active_plane != INKPOD_PLANE_COLOR
        || state.Workspace().panes.active_tree_layer_id != erase_target_info.layer_id
        || state.Workspace().panes.active_tree_plane_id != erase_target_info.color_plane_id) {
        return 760;
    }
    SendMessageW(erase_main_line, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE
        || SendMessageW(erase_main_line, BM_GETCHECK, 0, 0) != BST_CHECKED
        || SendMessageW(erase_color, BM_GETCHECK, 0, 0) != BST_UNCHECKED
        || !QueryDocument(state, erase_target_info)
        || erase_target_info.active_plane != INKPOD_PLANE_MAIN_LINE
        || state.Workspace().panes.active_tree_layer_id != erase_target_info.layer_id
        || state.Workspace().panes.active_tree_plane_id != erase_target_info.main_plane_id) {
        return 761;
    }
    if (!ToolPaletteMatchesCommandState(
            state.Workspace().tools.palette, state.Workspace().command_states)) {
        return 744;
    }
    if (!CommandSurfacesMatchComputedState(state)) {
        return 745;
    }
    SendMessageW(pencil_button, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_PENCIL
        || state.Workspace().tools.diameter != panes::kMaximumToolDiameter
        || IsWindowEnabled(diameter_edit) != FALSE
        || !diameter_text_is(L"1.0")) {
        return 755;
    }
    SendMessageW(brush_button, BM_CLICK, 0, 0);
    if (state.Workspace().tools.active_tool != INKPOD_TOOL_BRUSH
        || IsWindowEnabled(diameter_edit) == FALSE
        || !diameter_text_is(L"256.0")) {
        return 756;
    }
    SetFocus(diameter_edit);
    SetWindowTextW(diameter_edit, L"8.0");
    SetFocus(state.Workspace().windows.canvas);
    if (state.Workspace().tools.diameter != 8.0F || !diameter_text_is(L"8.0")) {
        return 757;
    }
    SendMessageW(pencil_button, BM_CLICK, 0, 0);
    const LONG initial_canvas_width =
        workspace_canvas_bounds.right - workspace_canvas_bounds.left;
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_PALETTE, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool)
        || checked(IDM_WINDOW_TOOL_PALETTE)
        || IsWindowVisible(state.Workspace().tools.palette) != FALSE
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || workspace_canvas_bounds.right - workspace_canvas_bounds.left
            <= initial_canvas_width) {
        return 734;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_PALETTE, 0)
            != 1
        || !state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool)
        || !checked(IDM_WINDOW_TOOL_PALETTE)) {
        return 735;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_OPTIONS, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::ToolOptions)
        || checked(IDM_WINDOW_TOOL_OPTIONS)
        || SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_TOOL_OPTIONS, 0)
            != 1) {
        return 736;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_COLOR_PANE, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Color)
        || SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_LAYER_PALETTE, 0)
            != 1
        || state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Layer)
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || workspace_canvas_bounds.right - workspace_canvas_bounds.left
            <= initial_canvas_width) {
        return 737;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_COLOR_PANE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_LAYER_PALETTE, 0);
    const std::array<DockPaneType, 4U> dock_pane_types{
        DockPaneType::Tool,
        DockPaneType::ToolOptions,
        DockPaneType::Color,
        DockPaneType::Layer};
    for (const DockPaneType type : dock_pane_types) {
        HWND content = state.Workspace().windows.dock_host.ContentWindow(type);
        if (content == nullptr
            || state.Workspace().windows.dock_host.FloatPane(type)
                != DockResult::Ok) {
            return 832;
        }
        const HWND floating =
            state.Workspace().windows.dock_host.FloatingWindow(type);
        wchar_t floating_title[128]{};
        const auto extended_style = floating == nullptr
            ? DWORD_PTR{}
            : static_cast<DWORD_PTR>(GetWindowLongPtrW(floating, GWL_EXSTYLE));
        if (floating == nullptr || GetParent(content) != floating
            || GetWindowTextW(
                   floating,
                   floating_title,
                   static_cast<int>(std::size(floating_title)))
                <= 0
            || GetWindow(floating, GW_OWNER) != state.Workspace().windows.window
            || IsWindowVisible(floating) == FALSE
            || GetClassLongPtrW(floating, GCLP_HBRBACKGROUND)
                != reinterpret_cast<ULONG_PTR>(GetSysColorBrush(COLOR_BTNFACE))
            || (extended_style & (WS_EX_TOPMOST | WS_EX_NOACTIVATE)) != 0U
            || (extended_style & WS_EX_PALETTEWINDOW)
                == WS_EX_PALETTEWINDOW
            || state.Workspace().windows.dock_host.HidePane(type)
                != DockResult::Ok
            || IsWindowVisible(floating) != FALSE
            || state.Workspace().windows.dock_host.RestorePane(type)
                != DockResult::Ok
            || GetParent(content) != state.Workspace().windows.window
            || !state.Workspace().windows.workspace.dock.IsPaneDocked(type)) {
            return 833;
        }
        SendMessageW(floating, WM_THEMECHANGED, 0, 0);
        SendMessageW(floating, WM_SETTINGCHANGE, 0, 0);
        SendMessageW(floating, WM_CANCELMODE, 0, 0);
        if (state.Workspace().windows.dock_host.PreviewVisible()) {
            return 835;
        }
    }
    if (state.Workspace().windows.dock_host.DockPane(
            DockPaneType::Color, DockZone::Left)
            != DockResult::Ok
        || state.Workspace().windows.workspace.dock.PaneCount(DockZone::Left)
            != 2U
        || state.Workspace().windows.dock_host.SetZoneMode(
               DockZone::Left, DockStackMode::Tabs)
            != DockResult::Ok) {
        return 834;
    }
    HWND dock_tabs =
        state.Workspace().windows.dock_host.TabWindow(DockZone::Left);
    const DockZoneState* left_zone =
        state.Workspace().windows.workspace.dock.Zone(DockZone::Left);
    if (dock_tabs == nullptr || left_zone == nullptr
        || TabCtrl_GetItemCount(dock_tabs) != 2
        || (GetWindowLongPtrW(dock_tabs, GWL_STYLE) & WS_TABSTOP) == 0
        || left_zone->active_tab != DockPaneType::Tool) {
        return 836;
    }
    SetFocus(dock_tabs);
    SendMessageW(dock_tabs, WM_KEYDOWN, VK_RIGHT, 0);
    SendMessageW(dock_tabs, WM_KEYUP, VK_RIGHT, 0);
    if (state.Workspace().windows.workspace.dock.Zone(DockZone::Left)
            ->active_tab
        != DockPaneType::Color
        || state.Workspace().windows.dock_host.SetZoneMode(
               DockZone::Left, DockStackMode::Split)
            != DockResult::Ok) {
        return 837;
    }
    HWND dock_splitter = state.Workspace().windows.dock_host.SplitterWindow(
        DockZone::Left, DockSplitterKind::StackBoundary);
    wchar_t splitter_name[64]{};
    const std::uint32_t tool_weight_before =
        state.Workspace().windows.workspace.dock.Pane(DockPaneType::Tool)
            ->split_weight;
    if (dock_splitter == nullptr
        || (GetWindowLongPtrW(dock_splitter, GWL_STYLE) & WS_TABSTOP) == 0
        || GetWindowTextW(
               dock_splitter,
               splitter_name,
               static_cast<int>(std::size(splitter_name)))
            <= 0) {
        return 838;
    }
    SetFocus(dock_splitter);
    SendMessageW(dock_splitter, WM_KEYDOWN, VK_DOWN, 0);
    if (state.Workspace().windows.workspace.dock.Pane(DockPaneType::Tool)
            ->split_weight
        <= tool_weight_before
        || state.Workspace().windows.dock_host.ResetPane(DockPaneType::Color)
            != DockResult::Ok
        || state.Workspace().windows.workspace.dock.Pane(DockPaneType::Color)
                ->zone
            != DockZone::Right) {
        return 839;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_WORKSPACE_MIRROR, 0)
            != 1
        || !state.Workspace().windows.workspace.dock.Mirrored()
        || !checked(IDM_WORKSPACE_MIRROR)
        || GetWindowRect(state.Workspace().tools.palette, &tool_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.canvas, &workspace_canvas_bounds) == FALSE
        || GetWindowRect(state.Workspace().windows.color_pane, &color_bounds) == FALSE
        || color_bounds.right > workspace_canvas_bounds.left
        || workspace_canvas_bounds.right > tool_bounds.left) {
        return 738;
    }
    if (LayerPaletteItemCount(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.tree_layer_count
        || LayerPalettePlaneCount(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.tree_plane_count
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.active_tree_layer_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.active_tree_plane_id
        || !LayerPaletteMatchesCommandState(
            state.Workspace().panes.layer_palette, state.Workspace().command_states)) {
        return 739;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WORKSPACE_RESET, 0);
    ShowWindow(state.Workspace().windows.window, SW_HIDE);

    if (state.engine == nullptr
        || MoveWindow(state.Workspace().windows.canvas, 0, 0, 640, 480, FALSE) == FALSE
        || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 30;
    }
    PumpPendingWindowMessages();
    UpdateMenuState(state);
    std::wstring tab_label;
    if (!ReadDocumentTabLabel(state.Workspace().windows.document_tabs, 0, tab_label)
        || tab_label != L"無題セル 1") {
        return 716;
    }
    std::size_t shortcut_leaf_count{};
    if (!CommandSurfacesMatchComputedState(state)
        || !MenuLeavesHaveAssignedShortcuts(
            menu, state.shortcuts.bindings, shortcut_leaf_count)
        || shortcut_leaf_count < windows::ui::MenuCommandCatalog().size()
        || FindWindowExW(state.Workspace().windows.window, nullptr, TOOLBARCLASSNAMEW, nullptr) != nullptr
        || SendMessageW(state.Workspace().windows.status_bar, SB_GETPARTS, 0, 0) != 6) {
        return 706;
    }
    if (DispatchEnabledCommand(state, state.Workspace().windows.window, IDM_EDIT_UNDO)) {
        return 714;
    }
    constexpr std::array<UINT, 6U> vector_draw_commands{
        IDM_VECTOR_LINE,
        IDM_VECTOR_CURVE,
        IDM_VECTOR_RECTANGLE,
        IDM_VECTOR_ELLIPSE,
        IDM_VECTOR_POLYLINE,
        IDM_VECTOR_ERASER};
    for (const UINT command : vector_draw_commands) {
        const UINT command_state = GetMenuState(menu, command, MF_BYCOMMAND);
        if (command_state == static_cast<UINT>(-1)
            || (command_state & (MF_DISABLED | MF_GRAYED)) == 0U) {
            return 701;
        }
    }
    if (!RefreshSequencePane(state) || state.Workspace().panes.sequence_count != 0U) {
        return 702;
    }
    InkpodSequenceCellInfo missing_sequence_cell{};
    missing_sequence_cell.struct_size = sizeof(missing_sequence_cell);
    const InkpodStatus missing_sequence_status = state.engine->Invoke(
        [&missing_sequence_cell](InkpodCore* core) {
            return inkpod_core_sequence_cell_get(core, 0U, &missing_sequence_cell);
        },
        false,
        false);
    if (missing_sequence_status != INKPOD_STATUS_INVALID_STATE
        || state.engine->LastError().find(L"no sequence is configured")
            == std::wstring::npos) {
        return 703;
    }
    if (FinishVectorCanvasGesture(state) != INKPOD_STATUS_INVALID_STATE
        || state.engine->LastError() != kVectorStrokePlaneRequired) {
        return 704;
    }
    const std::uint32_t initial_tool = state.Workspace().tools.active_tool;
    for (const UINT command : vector_draw_commands) {
        if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0) != 0
            || state.Workspace().tools.active_tool != initial_tool) {
            return 705;
        }
    }
    const std::wstring initial_recovery_path = state.Document().shell.recovery_path;
    std::wstring discovered_recovery;
    if (initial_recovery_path.empty()
        || !QueueAutosave(
               state, state.routing.targets.Capture(), initial_recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(initial_recovery_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || !NewestPrivateRecovery(discovered_recovery)
        || _wcsicmp(discovered_recovery.c_str(), initial_recovery_path.c_str()) != 0) {
        return 215;
    }
    std::wstring active_stroke_recovery_path;
    try {
        active_stroke_recovery_path = initial_recovery_path + L".active-stroke-test";
    } catch (const std::bad_alloc&) {
        return 217;
    }
    if (DeleteFileW(active_stroke_recovery_path.c_str()) == FALSE
        && GetLastError() != ERROR_FILE_NOT_FOUND) {
        return 218;
    }
    PumpPendingWindowMessages();
    const DWORD ui_thread = GetCurrentThreadId();
    const DWORD core_thread = state.engine->ThreadId();
    const DWORD renderer_thread = static_cast<DWORD>(SendMessageW(
        state.Workspace().windows.canvas, inkpod::renderer::kCanvasGetRendererThreadId, 0, 0));
    if (core_thread == 0U || renderer_thread == 0U || core_thread == ui_thread
        || renderer_thread == ui_thread || core_thread == renderer_thread) {
        return 31;
    }
    inkpod::renderer::CanvasDocumentBounds document_bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, document_bounds)
        || std::abs(document_bounds.left - 16.0) > 0.01
        || std::abs(document_bounds.top - 69.0) > 0.01
        || std::abs(document_bounds.right - 624.0) > 0.01
        || std::abs(document_bounds.bottom - 411.0) > 0.01) {
        return 53;
    }

    InkpodDocumentInfo before_line{};
    std::array<wchar_t, 128> initial_title{};
    if (!QueryDocument(state, before_line)
        || (before_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || GetWindowTextW(
               state.Workspace().windows.window, initial_title.data(), static_cast<int>(initial_title.size())) == 0
        || std::wcscmp(initial_title.data(), L"無題セル 1 - inkpod") != 0) {
        return 32;
    }
    const auto frames_before = static_cast<std::uint64_t>(SendMessageW(
        state.Workspace().windows.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 100));
    for (int x = 90; x <= 240; x += 15) {
        SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 120));
    }
    if (state.engine->FlushPreview() != INKPOD_STATUS_OK) {
        return 33;
    }
    if (!QueueAutosave(
            state,
            state.routing.targets.Capture(),
            active_stroke_recovery_path)) {
        return 219;
    }
    PumpPendingWindowMessages();
    if (SendMessageW(state.Workspace().windows.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 34;
    }
    InkpodDocumentInfo during_line{};
    const auto frames_during = static_cast<std::uint64_t>(SendMessageW(
        state.Workspace().windows.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    if (!QueryDocument(state, during_line)) {
        return 130;
    }
    if (during_line.document_revision != before_line.document_revision) {
        return 131;
    }
    if (during_line.main_plane_checksum != before_line.main_plane_checksum) {
        return 132;
    }
    if ((during_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)
        != (before_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)) {
        return 133;
    }
    if (frames_during <= frames_before) {
        return 134;
    }
    if (SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(250, 120)) != 1) {
        return 36;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 37;
    }
    if (GetFileAttributesW(active_stroke_recovery_path.c_str())
        == INVALID_FILE_ATTRIBUTES) {
        return 220;
    }
    DeleteFileW(active_stroke_recovery_path.c_str());
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_line{};
    if (!QueryDocument(state, after_line)
        || after_line.document_revision != before_line.document_revision + 1U
        || after_line.main_plane_checksum == before_line.main_plane_checksum
        || (after_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || (after_line.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U
        || !ReadDocumentTabLabel(state.Workspace().windows.document_tabs, 0, tab_label)
        || tab_label != L"無題セル 1 *") {
        return 38;
    }
    const std::uint64_t line_checksum = after_line.main_plane_checksum;

    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 150));
    SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(180, 150));
    SendMessageW(state.Workspace().windows.canvas, WM_CAPTURECHANGED, 0, 0);
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 54;
    }
    InkpodDocumentInfo after_cancel{};
    if (!QueryDocument(state, after_cancel)
        || after_cancel.document_revision != after_line.document_revision
        || after_cancel.main_plane_checksum != after_line.main_plane_checksum) {
        return 55;
    }

    state.Workspace().tools.active_plane = INKPOD_PLANE_COLOR;
    if (state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR);
            },
            false,
            true)
        != INKPOD_STATUS_OK) {
        return 39;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(100, 180));
    for (int x = 115; x <= 260; x += 15) {
        SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 190));
    }
    if (SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(270, 190)) != 1) {
        return 40;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 41;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_color{};
    if (!QueryDocument(state, after_color)
        || after_color.main_plane_checksum != line_checksum
        || after_color.color_plane_checksum == after_line.color_plane_checksum) {
        return 42;
    }
    const inkpod::app::EngineMetrics metrics = state.engine->Metrics();
    if (metrics.completed_strokes != 2U || metrics.completed_samples <= 2U
        || metrics.preview_snapshots == 0U) {
        return 43;
    }

    if (state.engine->Invoke(
            [](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_undo(core, &result);
            },
            true,
            true)
            != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [](InkpodCore* core) {
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_redo(core, &result);
               },
               true,
               true)
            != INKPOD_STATUS_OK) {
        return 44;
    }
    InkpodDocumentInfo after_redo{};
    if (!QueryDocument(state, after_redo)
        || after_redo.color_plane_checksum != after_color.color_plane_checksum) {
        return 45;
    }

    const std::uint64_t revision_before_view = after_redo.document_revision;
    SendMessageW(state.Workspace().windows.canvas, WM_MBUTTONDOWN, MK_MBUTTON, MAKELPARAM(300, 220));
    SendMessageW(state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_MBUTTON, MAKELPARAM(320, 230));
    SendMessageW(state.Workspace().windows.canvas, WM_MBUTTONUP, 0, MAKELPARAM(320, 230));
    RECT canvas_bounds{};
    GetWindowRect(state.Workspace().windows.canvas, &canvas_bounds);
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_MOUSEWHEEL,
        MAKEWPARAM(0, WHEEL_DELTA),
        MAKELPARAM(canvas_bounds.left + 320, canvas_bounds.top + 240));
    InkpodDocumentInfo after_view{};
    if (!QueryDocument(state, after_view)
        || after_view.document_revision != revision_before_view
        || after_view.view_revision == after_redo.view_revision) {
        return 46;
    }

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 47;
    }
    std::array<wchar_t, MAX_PATH> temporary_file{};
    _snwprintf_s(
        temporary_file.data(),
        temporary_file.size(),
        _TRUNCATE,
        L"%lsinkpod-drawing-save-smoke-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(temporary_file.data());
    if (SaveToPath(state, path) != INKPOD_STATUS_OK) {
        return 48;
    }
    const std::size_t path_separator = path.find_last_of(L"\\/");
    const std::wstring expected_saved_tab = path_separator == std::wstring::npos
        ? path
        : path.substr(path_separator + 1U);
    if (!ReadDocumentTabLabel(state.Workspace().windows.document_tabs, 0, tab_label)
        || tab_label != expected_saved_tab) {
        DeleteFileW(path.c_str());
        return 717;
    }
    if (GetFileAttributesW(initial_recovery_path.c_str()) != INVALID_FILE_ATTRIBUTES
        || GetLastError() != ERROR_FILE_NOT_FOUND) {
        DeleteFileW(path.c_str());
        return 216;
    }
    InkpodDocumentInfo saved{};
    if (!QueryDocument(state, saved)
        || (saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        DeleteFileW(path.c_str());
        return 49;
    }
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || OpenDocumentFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 50;
    }
    InkpodDocumentInfo reopened{};
    const bool round_trip = QueryDocument(state, reopened)
        && SamePersistentMetadata(saved, reopened)
        && (reopened.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U;
    if (!round_trip) {
        DeleteFileW(path.c_str());
        return 51;
    }
    const InkpodStrokeSample history_sample{
        sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U};
    const InkpodStrokeInput history_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x14c832ff),
        1.0F,
        &history_sample,
        1U,
        sizeof(history_sample)};
    InkpodSelectionInput history_selection{};
    history_selection.struct_size = sizeof(history_selection);
    history_selection.shape = INKPOD_SELECTION_RECTANGLE;
    history_selection.operation = INKPOD_SELECTION_NEW;
    history_selection.bounds = {10, 10, 1, 1};
    if (state.engine->Invoke(
            [history_stroke, history_selection](InkpodCore* core) {
                InkpodStatus status = inkpod_core_set_active_plane(
                    core, INKPOD_PLANE_COLOR);
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_apply_stroke(core, &history_stroke, &result);
                }
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_apply_selection(
                        core, &history_selection, &result);
                }
                return status;
            },
            true,
            true) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 217;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILE_REVERT_PARTIAL, 0);
    InkpodDocumentInfo partially_reverted{};
    if (!QueryDocument(state, partially_reverted)
        || partially_reverted.color_plane_checksum != reopened.color_plane_checksum) {
        DeleteFileW(path.c_str());
        return 241;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_HISTORY_BACK, 0);
    InkpodHistoryInfo smoke_history{};
    smoke_history.struct_size = sizeof(smoke_history);
    if (state.engine->Invoke(
            [&smoke_history](InkpodCore* core) {
                return inkpod_core_history_info(core, &smoke_history);
            },
            false,
            false) != INKPOD_STATUS_OK
        || smoke_history.cursor != 0U || smoke_history.item_count < 3U) {
        DeleteFileW(path.c_str());
        return 219;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_HISTORY_FORWARD, 0);
    if (state.engine->Invoke(
            [&smoke_history](InkpodCore* core) {
                return inkpod_core_history_info(core, &smoke_history);
            },
            false,
            false) != INKPOD_STATUS_OK
        || smoke_history.cursor != smoke_history.item_count
        || state.engine->Invoke(
               [](InkpodCore* core) {
                   return inkpod_core_set_active_plane(
                       core, INKPOD_PLANE_MAIN_LINE);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 220;
    }
    state.Workspace().tools.active_plane = INKPOD_PLANE_MAIN_LINE;
    DeleteFileW(path.c_str());

    inkpod::renderer::CanvasDocumentBounds before_dpi_bounds{};
    inkpod::renderer::CanvasDocumentBounds after_dpi_bounds{};
    const bool bounds_before_dpi = inkpod::renderer::GetCanvasDocumentBounds(
        state.Workspace().windows.canvas, before_dpi_bounds);
    const bool dpi_changed = SendMessageW(
                                 state.Workspace().windows.canvas,
                                 WM_DPICHANGED_AFTERPARENT,
                                 0,
                                 0) == 1;
    const bool bounds_after_dpi = inkpod::renderer::GetCanvasDocumentBounds(
        state.Workspace().windows.canvas, after_dpi_bounds);
    const bool dpi_transform_stable = bounds_before_dpi && bounds_after_dpi
        && std::abs(before_dpi_bounds.left - after_dpi_bounds.left) <= 0.01
        && std::abs(before_dpi_bounds.top - after_dpi_bounds.top) <= 0.01
        && std::abs(before_dpi_bounds.right - after_dpi_bounds.right) <= 0.01
        && std::abs(before_dpi_bounds.bottom - after_dpi_bounds.bottom) <= 0.01;
    const bool device_recovered = SendMessageW(
                                      state.Workspace().windows.canvas,
                                      inkpod::renderer::kCanvasSimulateDeviceLoss,
                                      0,
                                      0) == 1;
    const bool rendered = SendMessageW(
                              state.Workspace().windows.canvas,
                              inkpod::renderer::kCanvasRenderOnce,
                              0,
                              0) == 1;
    return dpi_changed && dpi_transform_stable && device_recovered && rendered ? 0 : 52;
}

int RunPaintingRecoverySmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 200;
    }
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_TOOL_FILL, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_TOOL_EYEDROPPER, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_COLOR_CHECK_NATIVE, MF_BYCOMMAND)
            == static_cast<UINT>(-1)) {
        return 201;
    }

    const auto same_color = [](const InkpodColorValue& left, const InkpodColorValue& right) {
        return left.depth == right.depth && left.red == right.red
            && left.green == right.green && left.blue == right.blue
            && left.alpha == right.alpha;
    };
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_PENCIL, 0);
    const InkpodColorValue pencil_color = state.Workspace().tools.drawing_color;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    const InkpodColorValue default_fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 220U, 40U, 30U, 255U};
    const InkpodColorValue fill_command_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 1U, 2U, 3U, 255U};
    if (!same_color(state.Workspace().tools.drawing_color, default_fill_color)
        || state.Workspace().panes.color_pane.change_color == nullptr) {
        return 231;
    }
    state.Workspace().panes.color_pane.change_color(
        state.Workspace().panes.color_pane.context, fill_command_color);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_PENCIL, 0);
    BOOL valid_red{};
    const UINT displayed_pencil_red = GetDlgItemInt(
        state.Workspace().windows.color_pane, IDC_COLOR_RED, &valid_red, FALSE);
    std::array<wchar_t, 64U> color_label{};
    GetDlgItemTextW(
        state.Workspace().windows.color_pane,
        IDC_COLOR_DRAWING_LABEL,
        color_label.data(),
        static_cast<int>(color_label.size()));
    if (!same_color(state.Workspace().tools.drawing_color, pencil_color)
        || !same_color(state.Workspace().panes.color_pane.drawing_color, pencil_color)
        || valid_red == FALSE || displayed_pencil_red != pencil_color.red
        || std::wcsstr(color_label.data(), L"#000000FF") == nullptr) {
        return 231;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    valid_red = FALSE;
    const UINT displayed_fill_red = GetDlgItemInt(
        state.Workspace().windows.color_pane, IDC_COLOR_RED, &valid_red, FALSE);
    GetDlgItemTextW(
        state.Workspace().windows.color_pane,
        IDC_COLOR_DRAWING_LABEL,
        color_label.data(),
        static_cast<int>(color_label.size()));
    if (!same_color(state.Workspace().tools.drawing_color, fill_command_color)
        || !same_color(state.Workspace().panes.color_pane.drawing_color, fill_command_color)
        || valid_red == FALSE || displayed_fill_red != fill_command_color.red
        || std::wcsstr(color_label.data(), L"#010203FF") == nullptr) {
        return 231;
    }

    std::array<InkpodStrokeSample, 5> boundary_samples{{
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput boundary{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        boundary_samples.data(),
        boundary_samples.size(),
        sizeof(InkpodStrokeSample)};
    if (state.engine->Invoke(
            [boundary](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &boundary, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 202;
    }
    InkpodDocumentInfo before_fill{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, before_fill)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)) {
        return 203;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(before_fill.width);
    const int fill_x = static_cast<int>(std::lround(bounds.left + 150.0 * zoom));
    const int fill_y = static_cast<int>(std::lround(bounds.top + 150.0 * zoom));
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1) {
        return 204;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));
    InkpodDocumentInfo after_fill{};
    if (!QueryDocument(state, after_fill)
        || after_fill.document_revision != before_fill.document_revision + 1U
        || after_fill.main_plane_checksum != before_fill.main_plane_checksum
        || after_fill.color_plane_checksum == before_fill.color_plane_checksum
        || after_fill.active_plane != INKPOD_PLANE_COLOR
        || state.Workspace().tools.active_plane != INKPOD_PLANE_COLOR
        || state.Workspace().panes.active_tree_layer_id != before_fill.layer_id
        || state.Workspace().panes.active_tree_plane_id != before_fill.color_plane_id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != before_fill.layer_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != before_fill.color_plane_id) {
        return 205;
    }
    if (state.Workspace().panes.layer_palette_dialog.select_plane == nullptr) {
        return 791;
    }
    state.Workspace().panes.layer_palette_dialog.select_plane(
        state.Workspace().panes.layer_palette_dialog.context,
        before_fill.main_plane_id);
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE
        || state.Workspace().panes.active_tree_plane_id != before_fill.main_plane_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != before_fill.main_plane_id) {
        return 791;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1) {
        return 792;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));
    InkpodDocumentInfo after_noop_fill{};
    if (!QueryDocument(state, after_noop_fill)
        || after_noop_fill.document_revision != after_fill.document_revision
        || after_noop_fill.main_plane_checksum != after_fill.main_plane_checksum
        || after_noop_fill.color_plane_checksum != after_fill.color_plane_checksum
        || after_noop_fill.active_plane != INKPOD_PLANE_MAIN_LINE
        || state.Workspace().tools.active_plane != INKPOD_PLANE_MAIN_LINE
        || state.Workspace().panes.active_tree_plane_id != before_fill.main_plane_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette)
            != before_fill.main_plane_id) {
        return 792;
    }

    const std::uint32_t fill_color = state.Workspace().tools.color_rgba;
    state.Workspace().tools.color_rgba = UINT32_C(0x010203ff);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_EYEDROPPER, 0);
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1
        || state.Workspace().tools.color_rgba != fill_color) {
        return 206;
    }
    SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));

    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    state.Workspace().tools.color_rgba = fill_color;
    state.Workspace().tools.fill_options.operation = INKPOD_FILL_CLOSED_REGION;
    state.Workspace().tools.fill_options.tolerance = 257U;
    state.Workspace().tools.fill_options.gap_close = 1U;
    state.Workspace().tools.fill_options.extension_distance = 2U;
    state.Workspace().tools.fill_options.detached_regions = true;
    state.Workspace().tools.fill_options.overflow_abort = true;
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL_OPTIONS, 0) != 0
        || state.Workspace().tools.fill_options.operation != INKPOD_FILL_CLOSED_REGION) {
        return 221;
    }
    const auto device_x = [&](double document_x) {
        return static_cast<int>(std::lround(bounds.left + document_x * zoom));
    };
    const auto device_y = [&](double document_y) {
        return static_cast<int>(std::lround(bounds.top + document_y * zoom));
    };
    const auto canvas_drag = [&state](int x1, int y1, int x2, int y2) {
        if (SendMessageW(
                state.Workspace().windows.canvas,
                WM_LBUTTONDOWN,
                MK_LBUTTON,
                MAKELPARAM(x1, y1)) != 1) {
            return false;
        }
        SendMessageW(
            state.Workspace().windows.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x2, y2));
        SendMessageW(state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(x2, y2));
        return true;
    };
    InkpodDocumentInfo before_closed{};
    if (!QueryDocument(state, before_closed)
        || !canvas_drag(
            device_x(90.0),
            device_y(90.0),
            device_x(210.0),
            device_y(210.0))) {
        return 222;
    }
    InkpodDocumentInfo after_closed{};
    if (!QueryDocument(state, after_closed)
        || after_closed.document_revision != before_closed.document_revision + 1U
        || after_closed.main_plane_checksum != before_closed.main_plane_checksum
        || after_closed.color_plane_checksum == before_closed.color_plane_checksum) {
        return 223;
    }

    const int extension_left = device_x(246.0);
    const int extension_top = device_y(246.0);
    const int extension_right = device_x(254.0);
    const int extension_bottom = device_y(254.0);
    const auto extension_seed_x = static_cast<float>(std::floor(
        ((static_cast<double>(extension_left) + static_cast<double>(extension_right)) / 2.0
            - bounds.left)
        / zoom));
    const auto extension_seed_y = static_cast<float>(std::floor(
        ((static_cast<double>(extension_top) + static_cast<double>(extension_bottom)) / 2.0
            - bounds.top)
        / zoom));
    const InkpodStrokeSample extension_source{
        sizeof(InkpodStrokeSample),
        0U,
        extension_seed_x,
        extension_seed_y,
        1.0F,
        0U};
    const InkpodStrokeInput extension_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x28b45aff),
        1.0F,
        &extension_source,
        1U,
        sizeof(extension_source)};
    if (state.engine->Invoke(
            [extension_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &extension_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 224;
    }
    InkpodDocumentInfo before_extension{};
    QueryDocument(state, before_extension);
    state.Workspace().tools.fill_options.operation = INKPOD_FILL_EXTENSION;
    state.Workspace().tools.fill_options.extension_distance = 3U;
    state.Workspace().tools.fill_options.detached_regions = false;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL_OPTIONS, 0);
    if (!canvas_drag(
            extension_left,
            extension_top,
            extension_right,
            extension_bottom)) {
        return 225;
    }
    InkpodDocumentInfo after_extension{};
    if (!QueryDocument(state, after_extension)
        || after_extension.color_plane_checksum == before_extension.color_plane_checksum) {
        return 226;
    }

    InkpodSelectionInput persistent_selection{};
    persistent_selection.struct_size = sizeof(persistent_selection);
    persistent_selection.shape = INKPOD_SELECTION_ELLIPSE;
    persistent_selection.operation = INKPOD_SELECTION_NEW;
    persistent_selection.bounds = {300, 300, 8, 8};
    if (state.engine->Invoke(
            [persistent_selection](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(
                    core, &persistent_selection, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 227;
    }
    state.Workspace().tools.fill_options.operation = INKPOD_FILL_SEED;
    state.Workspace().tools.fill_options.use_document_selection = true;
    state.Workspace().tools.fill_options.overflow_abort = false;
    state.Workspace().tools.fill_options.gap_close = 0U;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL_OPTIONS, 0);
    const int selected_x = device_x(304.0);
    const int selected_y = device_y(304.0);
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(selected_x, selected_y)) != 1) {
        return 228;
    }
    SendMessageW(
        state.Workspace().windows.canvas, WM_LBUTTONUP, 0, MAKELPARAM(selected_x, selected_y));
    InkpodColorValue selected_fill{};
    selected_fill.struct_size = sizeof(selected_fill);
    InkpodColorValue outside_fill{};
    outside_fill.struct_size = sizeof(outside_fill);
    InkpodStatus outside_fill_status = INKPOD_STATUS_OK;
    if (state.engine->Invoke(
            [&selected_fill, &outside_fill, &outside_fill_status](InkpodCore* core) {
                InkpodStatus status = inkpod_core_eyedropper(
                    core,
                    INKPOD_EYEDROPPER_SELECTED_PLANE,
                    304U,
                    304U,
                    &selected_fill);
                if (status == INKPOD_STATUS_OK) {
                    outside_fill_status = inkpod_core_eyedropper(
                        core,
                        INKPOD_EYEDROPPER_SELECTED_PLANE,
                        295U,
                        295U,
                        &outside_fill);
                }
                return status;
            },
            false,
            false) != INKPOD_STATUS_OK
        || selected_fill.alpha == 0U
        || outside_fill_status != INKPOD_STATUS_INVALID_STATE) {
        return 229;
    }
    state.Workspace().tools.fill_options = {};

    InkpodDocumentInfo before_check{};
    if (!QueryDocument(state, before_check)) {
        return 230;
    }
    const std::uint64_t revision_before_check = before_check.document_revision;
    const std::uint64_t view_before_check = before_check.view_revision;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_COLOR_CHECK_NATIVE, 0);
    InkpodDocumentInfo during_check{};
    std::uint64_t check_features{};
    const InkpodStatus check_snapshot_status = state.engine->Invoke(
        [&check_features](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            InkpodSnapshotView view{};
            view.struct_size = sizeof(view);
            status = inkpod_snapshot_get_view(snapshot, &view);
            if (status == INKPOD_STATUS_OK) {
                check_features = view.feature_flags;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, during_check)
        || check_snapshot_status != INKPOD_STATUS_OK
        || check_features != INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
        || during_check.document_revision != revision_before_check
        || during_check.view_revision <= view_before_check
        || SendMessageW(state.Workspace().windows.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 207;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_COLOR_CHECK_OFF, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 208;
    }
    const auto suffix = static_cast<unsigned long long>(GetTickCount64());
    std::array<wchar_t, MAX_PATH> normal_buffer{};
    std::array<wchar_t, MAX_PATH> recovery_buffer{};
    _snwprintf_s(
        normal_buffer.data(),
        normal_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-painting-normal-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    _snwprintf_s(
        recovery_buffer.data(),
        recovery_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-painting-recovery-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    const std::wstring normal_path(normal_buffer.data());
    const std::wstring recovery_path(recovery_buffer.data());
    if (SaveToPath(state, normal_path) != INKPOD_STATUS_OK) {
        return 209;
    }
    const inkpod::app::DocumentSessionId normal_session = state.Document().id;
    InkpodDocumentInfo normally_saved{};
    if (!QueryDocument(state, normally_saved)) {
        return 210;
    }
    std::array<InkpodStrokeSample, 1> edit_sample{{
        {sizeof(InkpodStrokeSample), 0U, 300.0F, 300.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput edit{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x010203ff),
        1.0F,
        edit_sample.data(),
        edit_sample.size(),
        sizeof(InkpodStrokeSample)};
    if (state.engine->Invoke(
            [edit](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &edit, &result);
            },
            true,
            true) != INKPOD_STATUS_OK
        || !QueueAutosave(
               state, state.routing.targets.Capture(), recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(normal_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(recovery_path.c_str()) == INVALID_FILE_ATTRIBUTES) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 211;
    }
    InkpodDocumentInfo autosaved{};
    if (!QueryDocument(state, autosaved)
        || (autosaved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 212;
    }
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || OpenRecoveryFromPath(state, recovery_path) != INKPOD_STATUS_OK) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 213;
    }
    InkpodDocumentInfo recovered{};
    const bool recovery_state = QueryDocument(state, recovered)
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        && recovered.color_plane_checksum == autosaved.color_plane_checksum
        && state.Document().shell.current_path.empty();
    const InkpodStatus revert_status = state.engine->Invoke(
        [](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_revert(core, &info);
        },
        false,
        false);
    const bool normal_unchanged = state.CloseDocumentSession(normal_session)
        && OpenDocumentFromPath(state, normal_path) == INKPOD_STATUS_OK;
    InkpodDocumentInfo reopened_normal{};
    const bool normal_matches = QueryDocument(state, reopened_normal)
        && reopened_normal.color_plane_checksum == normally_saved.color_plane_checksum
        && reopened_normal.color_plane_checksum != recovered.color_plane_checksum;
    DeleteFileW(normal_path.c_str());
    DeleteFileW(recovery_path.c_str());
    return recovery_state && revert_status == INKPOD_STATUS_INVALID_STATE && normal_unchanged
            && normal_matches
        ? 0
        : 214;
}

int RunDocumentEditingSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 300;
    }
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    for (const UINT command : {
             IDM_EDIT_COPY,
             IDM_EDIT_PASTE,
             IDM_EDIT_MIRROR_HORIZONTAL,
             IDM_LAYER_DUPLICATE,
             IDM_LAYER_DELETE,
             IDM_LAYER_MOVE_TOP,
             IDM_SELECTION_RECTANGLE,
             IDM_SELECTION_ELLIPSE,
             IDM_SELECTION_LASSO,
             IDM_SELECTION_POLYLINE,
             IDM_SELECTION_TRACE,
             IDM_SELECTION_WAND,
             IDM_SELECTION_MODE_NEW,
             IDM_SELECTION_MODE_ADD,
             IDM_SELECTION_MODE_SUBTRACT,
             IDM_SELECTION_MODE_INTERSECT,
             IDM_SELECTION_CLEAR,
             IDM_SELECTION_COLOR,
             IDM_SELECTION_COLOR_DIFFERENT,
             IDM_SELECTION_COLOR_ADD,
             IDM_SELECTION_TO_LAYER,
             IDM_SELECTION_FROM_LAYER,
             IDM_SELECTION_LAYER_ADD,
             IDM_SELECTION_LAYER_SUBTRACT,
             IDM_SELECTION_ALL,
             IDM_SELECTION_INVERT,
             IDM_SELECTION_EXPAND,
             IDM_SELECTION_SHRINK,
             IDM_VIEW_FLIP_HORIZONTAL,
             IDM_VIEW_FLIP_VERTICAL,
             IDM_VIEW_GRID,
             IDM_VIEW_NEW,
             IDM_SHORTCUT_EDIT,
             IDM_SHORTCUT_RESET}) {
        if (menu == nullptr
            || GetMenuState(menu, command, MF_BYCOMMAND)
                == static_cast<UINT>(-1)) {
            return 301;
        }
    }

    const InkpodCellCreateOptions source_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000001),
        UINT64_C(0x4d33000000000002),
        8U,
        8U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [source_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &source_options, &info);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 302;
    }
    ResetUiForNewActiveDocument(state);
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        return 303;
    }

    InkpodDocumentInfo initial{};
    if (!QueryDocument(state, initial)) {
        return 304;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_DUPLICATE, 0);
    const std::uint64_t duplicate_id = state.Document().shell.smoke_layer_id;
    InkpodDocumentInfo duplicated{};
    if (duplicate_id == 0U || !QueryDocument(state, duplicated)
        || duplicated.document_revision != initial.document_revision + 1U) {
        return 305;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MOVE_TOP, 0);
    InkpodNodeInfo top_layer{};
    top_layer.struct_size = sizeof(top_layer);
    const InkpodStatus top_status = state.engine->Invoke(
        [&top_layer](InkpodCore* core) {
            return inkpod_core_node_get(
                core, 0U, UINT32_MAX, &top_layer);
        },
        false,
        false);
    if (top_status != INKPOD_STATUS_OK || top_layer.id != duplicate_id) {
        return 306;
    }

    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_DELETE, 0);
    InkpodDocumentInfo after_delete{};
    if (!QueryDocument(state, after_delete)
        || (after_delete.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 307;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id != duplicate_id) {
        return 308;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id == duplicate_id) {
        return 309;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 310;
    }
    std::array<wchar_t, MAX_PATH> file_buffer{};
    _snwprintf_s(
        file_buffer.data(),
        file_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-document-editing-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(file_buffer.data());
    if (SaveToPath(state, path) != INKPOD_STATUS_OK
        || OpenFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 311;
    }
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    const bool reopened_tree = state.engine->Invoke(
                                   [&top_layer](InkpodCore* core) {
                                       return inkpod_core_node_get(
                                           core,
                                           0U,
                                           UINT32_MAX,
                                           &top_layer);
                                   },
                                   false,
                                   false) == INKPOD_STATUS_OK
        && top_layer.id == duplicate_id;
    DeleteFileW(path.c_str());
    if (!reopened_tree) {
        return 312;
    }

    InkpodDocumentInfo before_invalid{};
    if (!QueryDocument(state, before_invalid)) {
        return 313;
    }
    static constexpr std::array<std::uint8_t, 7> invalid_name{
        'I', 'n', 'v', 'a', 'l', 'i', 'd'};
    InkpodTreeEdit invalid_plane{};
    invalid_plane.struct_size = sizeof(invalid_plane);
    invalid_plane.operation = INKPOD_TREE_CREATE_PLANE;
    invalid_plane.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
    invalid_plane.parent_id = duplicate_id;
    invalid_plane.kind = INKPOD_TYPED_PLANE_SELECTION;
    invalid_plane.pixel_format = INKPOD_STORAGE_BINARY8;
    invalid_plane.opacity_milli = 1000U;
    invalid_plane.name_utf8 = invalid_name.data();
    invalid_plane.name_bytes = invalid_name.size();
    InkpodStatus invalid_status = INKPOD_STATUS_OK;
    state.engine->Invoke(
        [&invalid_plane, &invalid_status](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t object_id{};
            invalid_status = inkpod_core_tree_edit(
                core, &invalid_plane, &result, &object_id);
            return INKPOD_STATUS_OK;
        },
        false,
        false);
    InkpodDocumentInfo after_invalid{};
    if (invalid_status != INKPOD_STATUS_INVALID_ARGUMENT
        || !QueryDocument(state, after_invalid)
        || after_invalid.document_revision != before_invalid.document_revision) {
        return 314;
    }

    inkpod::renderer::CanvasDocumentBounds selection_canvas_bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, selection_canvas_bounds)) {
        return 315;
    }
    const auto selection_sample = [&selection_canvas_bounds](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(selection_canvas_bounds.left
                + (selection_canvas_bounds.right - selection_canvas_bounds.left)
                    * static_cast<double>(x) / 8.0),
            static_cast<float>(selection_canvas_bounds.top
                + (selection_canvas_bounds.bottom - selection_canvas_bounds.top)
                    * static_cast<double>(y) / 8.0),
            1.0F,
            0U};
    };
    const auto query_selection_preview = [&state](
                                             inkpod::renderer::CanvasGeometryPreview& preview) {
        preview = {};
        preview.struct_size = sizeof(preview);
        return inkpod::renderer::GetCanvasGeometryPreview(
            state.Workspace().windows.canvas, preview);
    };
    const auto send_selection_gesture = [&state](const auto& samples) noexcept {
        if (samples.empty()) {
            return false;
        }
        const inkpod::renderer::CanvasStrokeEvent begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
        if (!inkpod::renderer::SubmitCanvasStrokeEvent(
                state.Workspace().windows.canvas, begin)) {
            return false;
        }
        if (samples.size() > 2U) {
            const inkpod::renderer::CanvasStrokeEvent append{
                inkpod::renderer::CanvasStrokeEventKind::Append,
                samples.data() + 1U,
                samples.size() - 2U};
            if (!inkpod::renderer::SubmitCanvasStrokeEvent(
                    state.Workspace().windows.canvas, append)) {
                return false;
            }
        }
        const inkpod::renderer::CanvasStrokeEvent end{
            inkpod::renderer::CanvasStrokeEventKind::End,
            samples.data() + samples.size() - 1U,
            1U};
        return inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, end);
    };
    const auto query_selection = [&state](InkpodLocatorOutput& output) noexcept {
        output = {};
        output.struct_size = sizeof(output);
        return state.engine->Invoke(
                   [&output](InkpodCore* core) {
                       return inkpod_core_locator_sample(core, 0U, 0.0, 0.0, &output);
                   },
                   false,
                   false) == INKPOD_STATUS_OK;
    };
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_RECTANGLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
    const std::array<InkpodStrokeSample, 2U> preview_samples{
        selection_sample(1.0F, 1.0F), selection_sample(5.0F, 6.0F)};
    const inkpod::renderer::CanvasStrokeEvent preview_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin,
        preview_samples.data(),
        1U};
    const inkpod::renderer::CanvasStrokeEvent preview_append{
        inkpod::renderer::CanvasStrokeEventKind::Append,
        preview_samples.data() + 1U,
        1U};
    InkpodDocumentInfo before_selection_preview{};
    if (!QueryDocument(state, before_selection_preview)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, preview_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, preview_append)) {
        return 610;
    }
    inkpod::renderer::CanvasGeometryPreview selection_preview{};
    InkpodDocumentInfo during_selection_preview{};
    if (!query_selection_preview(selection_preview)
        || !QueryDocument(state, during_selection_preview)
        || during_selection_preview.document_revision
            != before_selection_preview.document_revision
        || selection_preview.active != 1U || selection_preview.closed != 1U
        || selection_preview.point_count != 4U
        || selection_preview.stroke_width != 0.0F) {
        return 611;
    }
    const inkpod::renderer::CanvasStrokeEvent preview_cancel{
        inkpod::renderer::CanvasStrokeEventKind::Cancel, nullptr, 0U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, preview_cancel)
        || !query_selection_preview(selection_preview)
        || selection_preview.active != 0U || selection_preview.point_count != 0U) {
        return 612;
    }
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, preview_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, preview_append)
        || !query_selection_preview(selection_preview)
        || selection_preview.active != 1U) {
        return 613;
    }
    const inkpod::renderer::CanvasStrokeEvent preview_end{
        inkpod::renderer::CanvasStrokeEventKind::End,
        preview_samples.data() + 1U,
        1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, preview_end)
        || !query_selection_preview(selection_preview)
        || selection_preview.active != 0U || selection_preview.point_count != 0U) {
        return 614;
    }
    const auto verify_selection_preview = [&](
                                              UINT command,
                                              std::uint32_t expected_points,
                                              bool expected_closed,
                                              bool expected_region_width) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0);
        if (!inkpod::renderer::SubmitCanvasStrokeEvent(
                state.Workspace().windows.canvas, preview_begin)
            || !inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, preview_append)
            || !query_selection_preview(selection_preview)
            || selection_preview.active != 1U
            || selection_preview.point_count != expected_points
            || selection_preview.closed != (expected_closed ? 1U : 0U)
            || (selection_preview.stroke_width > 0.0F) != expected_region_width) {
            return false;
        }
        return inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, preview_cancel)
            && query_selection_preview(selection_preview)
            && selection_preview.active == 0U
            && selection_preview.point_count == 0U;
    };
    if (!verify_selection_preview(IDM_SELECTION_ELLIPSE, 48U, true, false)
        || !verify_selection_preview(IDM_SELECTION_LASSO, 2U, true, false)
        || !verify_selection_preview(IDM_SELECTION_POLYLINE, 2U, true, false)
        || !verify_selection_preview(IDM_SELECTION_TRACE, 2U, false, true)) {
        return 615;
    }
    const auto select_rectangle = [&](UINT mode, float x1, float y1, float x2, float y2) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_RECTANGLE, 0);
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, mode, 0);
        const std::array<InkpodStrokeSample, 2U> samples{
            selection_sample(x1, y1), selection_sample(x2, y2)};
        return send_selection_gesture(samples);
    };
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 0.0F, 0.0F, 4.0F, 4.0F)
        || !select_rectangle(IDM_SELECTION_MODE_ADD, 4.0F, 0.0F, 6.0F, 2.0F)
        || !select_rectangle(IDM_SELECTION_MODE_SUBTRACT, 0.0F, 0.0F, 1.0F, 4.0F)
        || !select_rectangle(IDM_SELECTION_MODE_INTERSECT, 2.0F, 0.0F, 4.0F, 4.0F)) {
        return 315;
    }
    InkpodLocatorOutput locator{};
    if (!query_selection(locator)
        || (locator.flags & 1U) == 0U || locator.selection.x != 2
        || locator.selection.y != 0 || locator.selection.width != 2
        || locator.selection.height != 4) {
        return 316;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LASSO, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
    const std::array<InkpodStrokeSample, 2U> short_lasso_samples{
        selection_sample(2.0F, 2.0F), selection_sample(3.0F, 3.0F)};
    if (!send_selection_gesture(short_lasso_samples)) {
        return 616;
    }
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 617;
    }
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 1.0F, 1.0F, 5.0F, 5.0F)) {
        return 618;
    }
    InkpodDocumentInfo before_empty_selection_operation{};
    if (!QueryDocument(state, before_empty_selection_operation)) {
        return 619;
    }
    for (const UINT mode : {
             IDM_SELECTION_MODE_ADD, IDM_SELECTION_MODE_SUBTRACT}) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LASSO, 0);
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, mode, 0);
        if (!send_selection_gesture(short_lasso_samples)) {
            return 620;
        }
        InkpodDocumentInfo after_empty_selection_operation{};
        if (!QueryDocument(state, after_empty_selection_operation)
            || after_empty_selection_operation.document_revision
                != before_empty_selection_operation.document_revision
            || !query_selection(locator) || (locator.flags & 1U) == 0U
            || locator.selection.x != 1 || locator.selection.y != 1
            || locator.selection.width != 4 || locator.selection.height != 4) {
            return 621;
        }
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_ELLIPSE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
    const std::array<InkpodStrokeSample, 2U> ellipse_samples{
        selection_sample(1.0F, 1.0F), selection_sample(7.0F, 7.0F)};
    if (!send_selection_gesture(ellipse_samples)) {
        return 357;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LASSO, 0);
    const std::array<InkpodStrokeSample, 3U> lasso_samples{
        selection_sample(0.0F, 0.0F),
        selection_sample(7.0F, 0.0F),
        selection_sample(0.0F, 7.0F)};
    if (!send_selection_gesture(lasso_samples)) {
        return 345;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_POLYLINE, 0);
    const std::array<InkpodStrokeSample, 3U> polyline_samples{
        selection_sample(1.0F, 1.0F),
        selection_sample(7.0F, 1.0F),
        selection_sample(7.0F, 7.0F)};
    if (!send_selection_gesture(polyline_samples)) {
        return 346;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_TRACE, 0);
    const std::array<InkpodStrokeSample, 2U> trace_samples{
        selection_sample(0.5F, 7.5F), selection_sample(7.5F, 0.5F)};
    if (!send_selection_gesture(trace_samples)) {
        return 347;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_WAND, 0);
    const std::array<InkpodStrokeSample, 1U> wand_samples{
        selection_sample(4.0F, 4.0F)};
    if (!send_selection_gesture(wand_samples) || !query_selection(locator)
        || (locator.flags & 1U) == 0U) {
        return 348;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_INVERT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_EXPAND, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_SHRINK, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 349;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_ALL, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.width != 8 || locator.selection.height != 8) {
        return 350;
    }

    const InkpodStrokeSample source_sample{
        sizeof(InkpodStrokeSample), 0U, 6.0F, 6.0F, 1.0F, 0U};
    const InkpodStrokeInput source_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &source_sample,
        1U,
        sizeof(source_sample)};
    if (state.engine->Invoke(
            [source_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &source_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 317;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_COLOR, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.x != 6 || locator.selection.y != 6
        || locator.selection.width != 1 || locator.selection.height != 1) {
        return 351;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_COLOR_DIFFERENT, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.width != 8 || locator.selection.height != 8) {
        return 352;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_COLOR_ADD, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_TO_LAYER, 0);
    if (state.Document().shell.selection_layer_id == 0U) {
        return 353;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_FROM_LAYER, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U) {
        return 354;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LAYER_ADD, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SELECTION_LAYER_SUBTRACT, 0);
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 355;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MAIN_LINE, 0);
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 6.0F, 6.0F, 7.0F, 7.0F)) {
        return 356;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_COPY, 0);
    if (state.clipboard == nullptr
        || IsClipboardFormatAvailable(CF_DIBV5) == FALSE
        || IsClipboardFormatAvailable(InkpodClipboardFormat()) == FALSE
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_CUT, 0) != 1) {
        return 318;
    }
    std::vector<std::uint8_t> external_dib;
    if (OpenClipboard(state.Workspace().windows.window) == FALSE) {
        return 367;
    }
    HANDLE dib_handle = GetClipboardData(CF_DIBV5);
    const SIZE_T dib_size = dib_handle == nullptr ? 0U : GlobalSize(dib_handle);
    const void* dib_source = dib_handle == nullptr ? nullptr : GlobalLock(dib_handle);
    try {
        if (dib_source != nullptr && dib_size != 0U) {
            external_dib.assign(
                static_cast<const std::uint8_t*>(dib_source),
                static_cast<const std::uint8_t*>(dib_source) + dib_size);
        }
    } catch (const std::bad_alloc&) {
        external_dib.clear();
    }
    if (dib_source != nullptr) {
        GlobalUnlock(dib_handle);
    }
    CloseClipboard();
    HGLOBAL external_handle = external_dib.empty()
        ? nullptr
        : GlobalAlloc(GMEM_MOVEABLE, external_dib.size());
    void* external_destination = external_handle == nullptr
        ? nullptr
        : GlobalLock(external_handle);
    if (external_destination == nullptr) {
        if (external_handle != nullptr) {
            GlobalFree(external_handle);
        }
        return 369;
    }
    std::memcpy(external_destination, external_dib.data(), external_dib.size());
    GlobalUnlock(external_handle);
    if (OpenClipboard(state.Workspace().windows.window) == FALSE) {
        GlobalFree(external_handle);
        return 370;
    }
    EmptyClipboard();
    const bool external_published = SetClipboardData(CF_DIBV5, external_handle) != nullptr;
    if (!external_published) {
        GlobalFree(external_handle);
    }
    CloseClipboard();
    InkpodClipboard* private_clipboard = state.clipboard;
    state.clipboard = nullptr;
    int external_failure{};
    if (!external_published) {
        external_failure = 371;
    } else if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_PASTE, 0) != 1) {
        external_failure = 372;
    } else if (!state.Workspace().tools.floating_active) {
        external_failure = 373;
    } else if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1) {
        external_failure = 374;
    }
    inkpod_clipboard_release(&state.clipboard);
    state.clipboard = private_clipboard;
    PublishStandardClipboard(state.Workspace().windows.window, state.clipboard);
    if (external_failure != 0) {
        return external_failure;
    }

    const InkpodCellCreateOptions destination_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000003),
        UINT64_C(0x4d33000000000004),
        4U,
        4U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [destination_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(
                    core, &destination_options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 319;
    }
    ResetUiForNewActiveDocument(state);
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
        || !RefreshTreePane(state)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_PASTE_SELECTED, 0) != 1
        || !state.Workspace().tools.floating_active
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_PASTE_CONVERTED, 0) != 1
        || !state.Workspace().tools.floating_active
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_PASTE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_FLOATING_TRANSFORM, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_PASTE, 0) != 1) {
        return 358;
    }
    inkpod::renderer::CanvasDocumentBounds floating_canvas{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, floating_canvas)) {
        return 359;
    }
    const double floating_zoom = (floating_canvas.right - floating_canvas.left) / 4.0;
    const InkpodStrokeSample floating_start{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(floating_canvas.left + 2.0 * floating_zoom),
        static_cast<float>(floating_canvas.top + 2.0 * floating_zoom), 1.0F, 0U};
    const InkpodStrokeSample floating_end{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(floating_canvas.left + 1.0 * floating_zoom),
        static_cast<float>(floating_canvas.top + 1.0 * floating_zoom), 1.0F, 0U};
    const inkpod::renderer::CanvasStrokeEvent floating_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &floating_start, 1U};
    const inkpod::renderer::CanvasStrokeEvent floating_finish{
        inkpod::renderer::CanvasStrokeEventKind::End, &floating_end, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, floating_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, floating_finish)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1) {
        return 359;
    }
    const InkpodFloatingTransform floating{
        sizeof(InkpodFloatingTransform),
        0U,
        -4.0,
        -4.0,
        1.0,
        1.0,
        0.0};
    const InkpodStatus paste_status = state.engine->Invoke(
        [&state, floating](InkpodCore* core) {
            InkpodStatus status = inkpod_core_paste_begin(
                core, state.clipboard);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_floating_transform(core, &floating);
            }
            if (status == INKPOD_STATUS_OK) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_floating_commit(core, &result);
            }
            if (status != INKPOD_STATUS_OK) {
                inkpod_core_floating_cancel(core);
            }
            return status;
        },
        true,
        true);
    InkpodColorValue pasted_color{};
    pasted_color.struct_size = sizeof(pasted_color);
    if (paste_status != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [&pasted_color](InkpodCore* core) {
                   return inkpod_core_eyedropper(
                       core,
                       INKPOD_EYEDROPPER_SELECTED_PLANE,
                       2U,
                       2U,
                       &pasted_color);
               },
               false,
               false) != INKPOD_STATUS_OK
        || pasted_color.red != 0U || pasted_color.green != 0U
        || pasted_color.blue != 0U || pasted_color.alpha == 0U) {
        return 320;
    }

    InkpodDocumentInfo before_flip{};
    if (!QueryDocument(state, before_flip)) {
        return 321;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_FLIP_HORIZONTAL, 0);
    InkpodDocumentInfo after_flip{};
    InkpodSnapshotTransform transform{};
    transform.struct_size = sizeof(transform);
    const InkpodStatus transform_status = state.engine->Invoke(
        [&transform](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &snapshot);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(snapshot, &transform);
            }
            const InkpodStatus release_status =
                inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, after_flip)
        || after_flip.document_revision != before_flip.document_revision
        || after_flip.view_revision <= before_flip.view_revision
        || transform_status != INKPOD_STATUS_OK
        || (transform.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            == 0U) {
        return 322;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_MIRROR_HORIZONTAL, 0);
    InkpodDocumentInfo after_mirror{};
    if (!QueryDocument(state, after_mirror)
        || after_mirror.document_revision
            != after_flip.document_revision + 1U
        || after_mirror.view_revision != after_flip.view_revision
        || (after_mirror.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 323;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    const std::array<UINT, 6U> document_transform_commands{
        IDM_CELL_MIRROR_VERTICAL,
        IDM_CELL_ROTATE_LEFT,
        IDM_CELL_ROTATE_RIGHT,
        IDM_CELL_IMAGE_SIZE,
        IDM_CELL_RESOLUTION,
        IDM_CELL_PAPER_SETTINGS};
    for (std::size_t index = 0U; index < document_transform_commands.size(); ++index) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, document_transform_commands[index], 0) != 1) {
            return 360 + static_cast<int>(index);
        }
    }
    InkpodDocumentInfo transformed_document{};
    if (!QueryDocument(state, transformed_document)
        || transformed_document.width <= 4U || transformed_document.height <= 4U
        || transformed_document.dpi_x_milli != 120000U
        || transformed_document.dpi_y_milli != 120000U) {
        return 366;
    }

    const int tab_count_before_view = TabCtrl_GetItemCount(
        state.Workspace().windows.document_tabs);
    const std::size_t view_count_before = state.Document().ViewCount();
    std::wstring secondary_tab_label;
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_VIEW_NEW,
            0) != 1
        || state.ActiveView().presentation.secondary_view_id == 0U || state.ActiveView().presentation.active_view_id != state.ActiveView().presentation.secondary_view_id
        || state.Workspace().windows.document_tabs == nullptr
        || state.Document().ViewCount() != view_count_before + 1U
        || TabCtrl_GetItemCount(state.Workspace().windows.document_tabs)
            != tab_count_before_view + 1
        || !ReadDocumentTabLabel(
            state.Workspace().windows.document_tabs,
            TabCtrl_GetCurSel(state.Workspace().windows.document_tabs),
            secondary_tab_label)
        || secondary_tab_label.find(L"[ビュー 2]") == std::wstring::npos) {
        return 324;
    }
    const InkpodViewInput secondary_pan{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_PAN_BY,
        0U,
        5.0,
        0.0,
        0.0,
        0.0};
    const std::uint64_t secondary_view_id = state.ActiveView().presentation.secondary_view_id;
    if (state.engine->Invoke(
            [secondary_view_id, secondary_pan](InkpodCore* core) {
                return inkpod_core_view_apply(
                    core, secondary_view_id, &secondary_pan);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 324;
    }
    const InkpodStrokeSample multi_view_sample{
        sizeof(InkpodStrokeSample), 0U, 0.0F, 0.0F, 1.0F, 0U};
    const InkpodStrokeInput multi_view_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &multi_view_sample,
        1U,
        sizeof(multi_view_sample)};
    if (state.engine->Invoke(
            [multi_view_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &multi_view_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 325;
    }
    std::uint64_t primary_revision{};
    std::uint64_t secondary_revision{};
    double primary_pan_x{};
    double secondary_pan_x{};
    const InkpodStatus multi_view_status = state.engine->Invoke(
        [secondary_view_id,
         &primary_revision,
         &secondary_revision,
         &primary_pan_x,
         &secondary_pan_x](
            InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* primary{};
            InkpodSnapshot* secondary{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &primary);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot_for_view(
                    core, secondary_view_id, &options, &secondary);
            }
            InkpodSnapshotView primary_view{};
            primary_view.struct_size = sizeof(primary_view);
            InkpodSnapshotView secondary_view{};
            secondary_view.struct_size = sizeof(secondary_view);
            InkpodSnapshotTransform primary_transform{};
            primary_transform.struct_size = sizeof(primary_transform);
            InkpodSnapshotTransform secondary_transform{};
            secondary_transform.struct_size = sizeof(secondary_transform);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(primary, &primary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(
                    secondary, &secondary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    primary, &primary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    secondary, &secondary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                primary_revision = primary_view.revision;
                secondary_revision = secondary_view.revision;
                primary_pan_x = primary_transform.pan_x;
                secondary_pan_x = secondary_transform.pan_x;
            }
            const InkpodStatus primary_release =
                inkpod_snapshot_release(&primary);
            const InkpodStatus secondary_release =
                inkpod_snapshot_release(&secondary);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            return primary_release == INKPOD_STATUS_OK
                    && secondary_release == INKPOD_STATUS_OK
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        false,
        false);
    if (multi_view_status != INKPOD_STATUS_OK || primary_revision == 0U
        || primary_revision != secondary_revision
        || primary_pan_x == secondary_pan_x) {
        return 326;
    }
    UpdateMenuState(state);
    std::array<wchar_t, 96U> zoom_status{};
    SendMessageW(
        state.Workspace().windows.status_bar,
        SB_GETTEXTW,
        1,
        reinterpret_cast<LPARAM>(zoom_status.data()));
    if (wcsstr(zoom_status.data(), L"ズーム:") == nullptr) {
        return 358;
    }
    const InkpodShortcutSequence* multi_stroke =
        windows::ui::FindShortcutSequence(state.shortcuts.bindings, IDM_FILE_REVERT);
    if (multi_stroke == nullptr || multi_stroke->stroke_count <= 1U) {
        return 359;
    }
    for (std::uint32_t index = 0; index < multi_stroke->stroke_count; ++index) {
        UINT resolved{};
        const InkpodShortcutMatch match = windows::ui::ResolveShortcutStroke(
            state.shortcuts, multi_stroke->strokes[index], resolved);
        const bool last = index + 1U == multi_stroke->stroke_count;
        if ((!last && match != INKPOD_SHORTCUT_MATCH_PREFIX)
            || (last
                && (match != INKPOD_SHORTCUT_MATCH_EXACT || resolved != IDM_FILE_REVERT))) {
            return 360;
        }
    }

    InkpodShortcutSequence undo_shortcut{};
    undo_shortcut.struct_size = sizeof(undo_shortcut);
    undo_shortcut.command_id = IDM_EDIT_UNDO;
    undo_shortcut.stroke_count = 1U;
    undo_shortcut.strokes[0] = {
        static_cast<std::uint32_t>('U'), INKPOD_SHORTCUT_MODIFIER_CONTROL};
    const InkpodStatus navigation_status = windows::ui::RebindShortcut(
        *state.engine, state.shortcuts, undo_shortcut, false);
    if (navigation_status != INKPOD_STATUS_OK) {
        return 327;
    }
    UINT shortcut_menu_command{};
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 328;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GRID_SETTINGS, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GUIDE_VERTICAL, 0);
    const auto query_guide_count = [&state]() noexcept {
        std::uint64_t count = UINT64_MAX;
        const std::uint64_t view_id = state.ActiveView().presentation.active_view_id;
        const InkpodStatus status = state.engine->Invoke(
            [view_id, &count](InkpodCore* core) {
                const InkpodSnapshotOptions options{
                    sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
                InkpodSnapshot* snapshot{};
                InkpodStatus inner = view_id == 0U
                    ? inkpod_core_build_snapshot(core, &options, &snapshot)
                    : inkpod_core_build_snapshot_for_view(
                          core, view_id, &options, &snapshot);
                InkpodSnapshotOverlay overlay{};
                overlay.struct_size = sizeof(overlay);
                if (inner == INKPOD_STATUS_OK) {
                    inner = inkpod_snapshot_get_overlay(snapshot, &overlay);
                }
                if (inner == INKPOD_STATUS_OK) {
                    count = overlay.guide_count;
                }
                const InkpodStatus released = inkpod_snapshot_release(&snapshot);
                return inner == INKPOD_STATUS_OK ? released : inner;
            },
            false,
            false);
        return status == INKPOD_STATUS_OK ? count : UINT64_MAX;
    };
    if (query_guide_count() != 1U) {
        return 341;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_RULER, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GRID, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_SNAP_GUIDES, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_SNAP_GRID, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_TRANSPARENT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_ZOOM_PERCENT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_BOX_ZOOM, 0);
    inkpod::renderer::CanvasDocumentBounds box_bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, box_bounds)) {
        return 329;
    }
    const std::array<InkpodStrokeSample, 2U> box_samples{
        InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(box_bounds.left
                + (box_bounds.right - box_bounds.left) * 0.25),
            static_cast<float>(box_bounds.top
                + (box_bounds.bottom - box_bounds.top) * 0.25),
            1.0F,
            0U},
        InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(box_bounds.left
                + (box_bounds.right - box_bounds.left) * 0.75),
            static_cast<float>(box_bounds.top
                + (box_bounds.bottom - box_bounds.top) * 0.75),
            1.0F,
            0U}};
    const inkpod::renderer::CanvasStrokeEvent box_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin,
        box_samples.data(),
        1U};
    const inkpod::renderer::CanvasStrokeEvent box_end{
        inkpod::renderer::CanvasStrokeEventKind::End,
        box_samples.data() + 1U,
        1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, box_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, box_end)) {
        return 329;
    }
    inkpod::renderer::CanvasDocumentBounds guide_bounds{};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, guide_bounds)) {
        return 329;
    }
    const double guide_zoom = (guide_bounds.right - guide_bounds.left) / 4.0;
    const auto send_guide_drag = [&state](
                                     InkpodStrokeSample begin_sample,
                                     InkpodStrokeSample end_sample) noexcept {
        const inkpod::renderer::CanvasStrokeEvent begin_event{
            inkpod::renderer::CanvasStrokeEventKind::Begin, &begin_sample, 1U};
        const inkpod::renderer::CanvasStrokeEvent end_event{
            inkpod::renderer::CanvasStrokeEventKind::End, &end_sample, 1U};
        return inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, begin_event)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, end_event);
    };
    const auto guide_sample = [](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample), 0U, x, y, 1.0F, 0U};
    };
    const float guide_x1 = static_cast<float>(guide_bounds.left + guide_zoom);
    const float guide_x3 = static_cast<float>(guide_bounds.left + 3.0 * guide_zoom);
    const float guide_y = static_cast<float>((guide_bounds.top + guide_bounds.bottom) / 2.0);
    if (!send_guide_drag(
            guide_sample(30.0F, 10.0F),
            guide_sample(guide_x1, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 2U) {
        return 342;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VIEW_GUIDE_MOVE, 0);
    if (!send_guide_drag(
            guide_sample(guide_x1, guide_y),
            guide_sample(guide_x3, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 2U) {
        return 343;
    }
    if (!send_guide_drag(
            guide_sample(guide_x3, guide_y),
            guide_sample(-10.0F, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 1U) {
        return 344;
    }
    if (SendMessageW(
            state.Workspace().windows.window, WM_COMMAND, IDM_SHORTCUT_EDIT, 0) != 1) {
        return 329;
    }
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 330;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SHORTCUT_RESET, 0);
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO) {
        return 361;
    }
    locator = {};
    locator.struct_size = sizeof(locator);
    const std::uint64_t active_view_id = state.ActiveView().presentation.active_view_id;
    if (!state.ActiveView().presentation.grid_visible || !state.ActiveView().presentation.ruler_visible || !state.ActiveView().presentation.snap_guides
        || !state.ActiveView().presentation.snap_grid || state.ActiveView().presentation.transparent_visible
        || state.engine->Invoke(
               [active_view_id, &locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, active_view_id, 2.0, 2.0, &locator);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        return 331;
    }
    bool overlay_connected{};
    std::uint32_t smoke_overlay_flags{};
    std::uint32_t smoke_grid_spacing{};
    std::uint32_t smoke_grid_subdivisions{};
    std::uint64_t smoke_guide_count{};
    const InkpodStatus overlay_status = state.engine->Invoke(
        [active_view_id,
         &overlay_connected,
         &smoke_overlay_flags,
         &smoke_grid_spacing,
         &smoke_grid_subdivisions,
         &smoke_guide_count](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = active_view_id == 0U
                ? inkpod_core_build_snapshot(core, &options, &snapshot)
                : inkpod_core_build_snapshot_for_view(
                      core, active_view_id, &options, &snapshot);
            InkpodSnapshotOverlay overlay{};
            overlay.struct_size = sizeof(overlay);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_overlay(snapshot, &overlay);
            }
            if (status == INKPOD_STATUS_OK) {
                smoke_overlay_flags = overlay.flags;
                smoke_grid_spacing = overlay.grid_spacing_x;
                smoke_grid_subdivisions = overlay.grid_subdivisions;
                smoke_guide_count = overlay.guide_count;
                overlay_connected =
                    (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) == 0U
                    && overlay.grid_spacing_x == 8U
                    && overlay.grid_subdivisions == 2U
                    && overlay.guide_count == 1U
                    && overlay.guides != nullptr;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (overlay_status != INKPOD_STATUS_OK || !overlay_connected) {
        if (overlay_status != INKPOD_STATUS_OK) {
            return 332;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) == 0U) {
            return 334;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE) == 0U) {
            return 335;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED) == 0U) {
            return 336;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) != 0U) {
            return 337;
        }
        if (smoke_grid_spacing != 8U || smoke_grid_subdivisions != 2U) {
            return 338;
        }
        return smoke_guide_count == 1U
            ? 339
            : 340 + static_cast<int>(std::min<std::uint64_t>(smoke_guide_count, 10U));
    }
    return SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 333;
}

int RunProductionWorkflowSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr
        || CreateCell(state, 32U, 24U, 120000U) != INKPOD_STATUS_OK) {
        return 400;
    }
    RefreshTreePane(state);
    RefreshLightTablePane(state);
    for (const UINT command : {
             IDM_CELL_FRAME_REFERENCE,
             IDM_CELL_FRAME_DRAWING,
             IDM_CELL_FRAME_SAFE,
             IDM_CELL_MARGINS}) {
        SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0);
    }
    InkpodDocumentInfo paper{};
    if (!QueryDocument(state, paper) || paper.width != 32U || paper.height != 24U
        || paper.dpi_x_milli != 120000U || paper.margin_left != 1U
        || paper.margin_top != 2U || paper.margin_right != 3U
        || paper.margin_bottom != 4U || paper.reference_frame.x != 1
        || paper.drawing_frame.x != 2 || paper.safe_frame.x != 3) {
        return 401;
    }

    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_NEW, 0) != 0
        || state.Workspace().panes.tree_layer_count < 2U
        || LayerPaletteItemCount(state.Workspace().panes.layer_palette)
            != state.Workspace().panes.tree_layer_count) {
        return 402;
    }
    const std::uint64_t raster_layer_id = state.Workspace().panes.active_tree_layer_id;
    const std::uint64_t raster_plane_id = state.Workspace().panes.active_tree_plane_id;
    if (raster_layer_id == 0U || raster_plane_id == 0U
        || state.Workspace().panes.tree_plane_count != 1U
        || state.Workspace().panes.active_tree_plane_index != 0U) {
        return 770;
    }
    InkpodNodeInfo first_layer{};
    first_layer.struct_size = sizeof(first_layer);
    const InkpodStatus first_layer_status = state.engine->Invoke(
        [&first_layer](InkpodCore* core) {
            return inkpod_core_node_get(core, 0U, UINT32_MAX, &first_layer);
        },
        false,
        false);
    const HWND layer_list = GetDlgItem(state.Workspace().panes.layer_palette, IDC_LAYER_LIST);
    SendMessageW(layer_list, LB_SETCURSEL, 0, 0);
    SendMessageW(
        state.Workspace().panes.layer_palette,
        WM_COMMAND,
        MAKEWPARAM(IDC_LAYER_LIST, LBN_SELCHANGE),
        reinterpret_cast<LPARAM>(layer_list));
    if (first_layer_status != INKPOD_STATUS_OK
        || state.Workspace().panes.active_tree_layer_id != first_layer.id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != first_layer.id) {
        return 470;
    }
    if (state.Workspace().panes.layer_palette_dialog.select_layer == nullptr) {
        return 771;
    }
    state.Workspace().panes.layer_palette_dialog.select_layer(
        state.Workspace().panes.layer_palette_dialog.context,
        raster_layer_id);
    if (state.Workspace().panes.active_tree_layer_id != raster_layer_id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != raster_layer_id
        || state.Workspace().panes.tree_plane_count != 1U
        || state.Workspace().panes.active_tree_plane_id != raster_plane_id
        || state.Workspace().panes.active_tree_plane_index != 0U) {
        return 771;
    }
    state.Workspace().tools.fill_options = FillToolOptions{};
    state.Workspace().tools.fill_options.overflow_abort = false;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    const InkpodColorValue generic_fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 90U, 80U, 70U, 255U};
    inkpod::windows::ui::tools::SetActiveCommandColor(
        state.Workspace().tools, generic_fill_color);
    inkpod::renderer::CanvasDocumentBounds generic_bounds{};
    InkpodDocumentInfo before_generic_fill{};
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
        || !QueryDocument(state, before_generic_fill)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, generic_bounds)) {
        return 793;
    }
    const double generic_zoom =
        (generic_bounds.right - generic_bounds.left)
        / static_cast<double>(before_generic_fill.width);
    const int generic_fill_x = static_cast<int>(
        std::lround(generic_bounds.left + generic_zoom));
    const int generic_fill_y = static_cast<int>(
        std::lround(generic_bounds.top + generic_zoom));
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(generic_fill_x, generic_fill_y)) != 1) {
        return 794;
    }
    SendMessageW(
        state.Workspace().windows.canvas,
        WM_LBUTTONUP,
        0,
        MAKELPARAM(generic_fill_x, generic_fill_y));
    InkpodDocumentInfo after_generic_fill{};
    if (!QueryDocument(state, after_generic_fill)) {
        return 795;
    }
    if (after_generic_fill.document_revision
        != before_generic_fill.document_revision + 1U) {
        return 796;
    }
    if (after_generic_fill.color_plane_checksum
        != before_generic_fill.color_plane_checksum) {
        return 797;
    }
    if (state.Workspace().tools.active_plane != INKPOD_PLANE_COLOR) {
        return 798;
    }
    if (state.Workspace().panes.active_tree_layer_id != raster_layer_id
        || state.Workspace().panes.active_tree_plane_id != raster_plane_id
        || LayerPaletteSelectedLayer(state.Workspace().panes.layer_palette) != raster_layer_id
        || LayerPaletteSelectedPlane(state.Workspace().panes.layer_palette) != raster_plane_id) {
        return 799;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    bool selected_raster_changed{};
    bool coloring_plane_unchanged{};
    const InkpodStatus selected_raster_stroke_status = state.engine->Invoke(
        [raster_layer_id, &selected_raster_changed, &coloring_plane_unchanged](
            InkpodCore* core) {
            InkpodDocumentInfo before = EmptyDocumentInfo();
            InkpodStatus current = inkpod_core_get_document_info(core, &before);
            const std::array<InkpodStrokeSample, 1U> samples{
                InkpodStrokeSample{
                    sizeof(InkpodStrokeSample), 0U, 3.0F, 4.0F, 1.0F, 0U}};
            const InkpodStrokeInput stroke{
                sizeof(InkpodStrokeInput),
                INKPOD_TOOL_BRUSH,
                INKPOD_PLANE_COLOR,
                INKPOD_COORDINATE_SPACE_DOCUMENT,
                0U,
                UINT32_C(0x3ac45eff),
                3.0F,
                samples.data(),
                samples.size(),
                sizeof(InkpodStrokeSample)};
            InkpodDispatchResult dispatch{};
            dispatch.struct_size = sizeof(dispatch);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_apply_stroke(core, &stroke, &dispatch);
            }
            InkpodDocumentInfo after = EmptyDocumentInfo();
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &after);
            }
            coloring_plane_unchanged = current == INKPOD_STATUS_OK
                && after.color_plane_checksum == before.color_plane_checksum;

            std::array<std::uint8_t, 32U * 24U * 4U> pixels{};
            InkpodLayerThumbnailBuffer thumbnail{};
            thumbnail.struct_size = sizeof(thumbnail);
            thumbnail.layer_id = raster_layer_id;
            thumbnail.maximum_width = 32U;
            thumbnail.maximum_height = 24U;
            thumbnail.pixels_rgba8 = pixels.data();
            thumbnail.pixel_capacity = pixels.size();
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_layer_thumbnail(core, &thumbnail);
            }
            if (current == INKPOD_STATUS_OK && thumbnail.width == 32U
                && thumbnail.height == 24U && thumbnail.stride_bytes >= 32U * 4U) {
                const std::size_t alpha = static_cast<std::size_t>(4U)
                        * thumbnail.stride_bytes
                    + static_cast<std::size_t>(3U) * 4U + 3U;
                selected_raster_changed = alpha < pixels.size() && pixels[alpha] != 0U;
            }
            if (current == INKPOD_STATUS_OK) {
                InkpodDispatchResult undo{};
                undo.struct_size = sizeof(undo);
                current = inkpod_core_undo(core, &undo);
            }
            return current;
        },
        true,
        true);
    if (selected_raster_stroke_status != INKPOD_STATUS_OK || !selected_raster_changed
        || !coloring_plane_unchanged) {
        return 790;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_TOGGLE_VISIBLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_TOGGLE_EDITABLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_OPACITY, 0);
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_PROPERTIES, 0) != 1) {
        return 402;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_CONVERT, 0);
    const std::uint32_t plane_count_before_create = state.Workspace().panes.tree_plane_count;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_NEW, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id == 0U
        || state.Workspace().panes.active_tree_plane_id == raster_plane_id
        || state.Workspace().panes.active_tree_plane_index != plane_count_before_create) {
        return 772;
    }
    const std::uint64_t created_plane_id = state.Workspace().panes.active_tree_plane_id;
    InkpodNodeInfo created_plane{};
    created_plane.struct_size = sizeof(created_plane);
    const std::uint32_t created_layer_index = state.Workspace().panes.active_tree_layer_index;
    const std::uint32_t created_plane_index = state.Workspace().panes.active_tree_plane_index;
    const InkpodStatus created_plane_status = state.engine->Invoke(
        [created_layer_index, created_plane_index, &created_plane](InkpodCore* core) {
            return inkpod_core_node_get(
                core, created_layer_index, created_plane_index, &created_plane);
        },
        false,
        false);
    if (created_plane_status != INKPOD_STATUS_OK
        || created_plane.id != created_plane_id
        || created_plane.kind != INKPOD_TYPED_PLANE_RASTER
        || created_plane.pixel_format != INKPOD_STORAGE_RGBA8) {
        return 832;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_TOGGLE_VISIBLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_TOGGLE_EDITABLE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_OPACITY, 0);
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_PROPERTIES, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_CONVERT, 0) != 1
        || state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id != created_plane_id) {
        return 773;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_DUPLICATE, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 2U
        || state.Workspace().panes.active_tree_plane_id == 0U
        || state.Workspace().panes.active_tree_plane_id == created_plane_id
        || state.Workspace().panes.active_tree_plane_index != plane_count_before_create + 1U) {
        return 774;
    }
    const std::uint64_t duplicated_plane_id = state.Workspace().panes.active_tree_plane_id;
    const std::uint32_t plane_move_start = state.Workspace().panes.active_tree_plane_index;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    if (plane_move_start == 0U
        || state.Workspace().panes.active_tree_plane_index != plane_move_start - 1U
        || state.Workspace().panes.active_tree_plane_id != duplicated_plane_id) {
        return 775;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    if (state.Workspace().panes.active_tree_plane_index != 0U
        || state.Workspace().panes.active_tree_plane_id != duplicated_plane_id) {
        return 776;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_DOWN, 0);
    if (state.Workspace().panes.active_tree_plane_index != 1U
        || state.Workspace().panes.active_tree_plane_id != duplicated_plane_id) {
        return 777;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_DELETE, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id != raster_plane_id
        || state.Workspace().panes.active_tree_plane_index != 0U) {
        return 778;
    }
    const std::uint64_t merge_destination_plane_id =
        state.Workspace().panes.active_tree_plane_id;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_DUPLICATE, 0);
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 2U
        || state.Workspace().panes.active_tree_plane_id == 0U
        || state.Workspace().panes.active_tree_plane_id == merge_destination_plane_id
        || state.Workspace().panes.active_tree_plane_index != 1U) {
        return 779;
    }
    const std::uint64_t merge_source_plane_id = state.Workspace().panes.active_tree_plane_id;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    if (state.Workspace().panes.active_tree_plane_index != 0U
        || state.Workspace().panes.active_tree_plane_id != merge_source_plane_id) {
        return 780;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_MERGE, 0) != 1) {
        return 781;
    }
    if (state.Workspace().panes.tree_plane_count != plane_count_before_create + 1U
        || state.Workspace().panes.active_tree_plane_id != merge_destination_plane_id
        || state.Workspace().panes.active_tree_plane_index != 0U) {
        return 782;
    }
    SendMessageW(
        GetDlgItem(state.Workspace().panes.layer_palette, IDM_LAYER_DUPLICATE),
        BM_CLICK,
        0,
        0);
    const std::uint32_t layer_move_start = state.Workspace().panes.active_tree_layer_index;
    if (layer_move_start != 0U) {
        if (state.Workspace().panes.layer_palette_dialog.reorder_layer == nullptr) {
            return 469;
        }
        state.Workspace().panes.layer_palette_dialog.reorder_layer(
            state.Workspace().panes.layer_palette_dialog.context,
            state.Workspace().panes.active_tree_layer_id,
            layer_move_start - 1U);
        if (state.Workspace().panes.active_tree_layer_index != layer_move_start - 1U) {
            return 469;
        }
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MOVE_UP, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MOVE_DOWN, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LAYER_MERGE, 0);

    if (CreateCell(state, 12U, 10U, 96000U) != INKPOD_STATUS_OK) {
        return 404;
    }
    try {
        state.lifetime.smoke_raster_path = L"inkpod-io2-smoke.png";
    } catch (const std::bad_alloc&) {
        return 405;
    }
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILE_EXPORT_RASTER, 0) != 1
        || GetFileAttributesW(state.lifetime.smoke_raster_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILE_IMPORT_RASTER, 0) != 1) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 406;
    }
    InkpodDocumentInfo imported{};
    const bool import_ok = QueryDocument(state, imported) && imported.width == 12U
        && imported.height == 10U && imported.dpi_x_milli >= 95000U
        && imported.dpi_x_milli <= 97000U;
    RefreshLightTablePane(state);
    if (!import_ok) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 407;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_COLOR_EDITOR, 0) != 1
        || state.Workspace().tools.drawing_color.depth != INKPOD_COLOR_DEPTH_16) {
        return 430;
    }
    const std::array<UINT, 10U> color_commands{
        IDM_COLOR_SOURCE_TOPMOST,
        IDM_COLOR_SOURCE_SELECTED,
        IDM_COLOR_SOURCE_COMPOSITE,
        IDM_COLOR_SOURCE_LIGHT_TABLE,
        IDM_PALETTE_REGISTER,
        IDM_PALETTE_SAVE,
        IDM_PALETTE_CLEAR,
        IDM_PALETTE_LOAD,
        IDM_PALETTE_NEXT_GROUP,
        IDM_CHART_GENERATE};
    for (std::size_t index = 0U; index < color_commands.size(); ++index) {
        if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, color_commands[index], 0) != 1) {
            return 434 + static_cast<int>(index);
        }
    }
    RefreshColorPanes(state);
    state.Workspace().panes.selected_color_chart_index = 0U;
    const std::array<UINT, 4U> chart_navigation_commands{
        IDM_CHART_RENAME, IDM_CHART_SEARCH, IDM_CHART_NEXT, IDM_CHART_NEXT_PAGE};
    for (std::size_t index = 0U; index < chart_navigation_commands.size(); ++index) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, chart_navigation_commands[index], 0) != 1) {
            return 450 + static_cast<int>(index);
        }
    }
    state.Workspace().panes.color_chart_page = 0U;
    state.Workspace().panes.selected_color_chart_index = 0U;
    RefreshColorPanes(state);
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_SAVE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_COPY, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_PASTE, 0) != 1) {
        return 432;
    }
    state.Workspace().panes.selected_color_chart_index = 0U;
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_CUT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_LOAD, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_LOCK, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_CHART_LOCK, 0) != 1) {
        return 433;
    }
    DeleteFileW(L"inkpod-palette-smoke.inkpalette");
    DeleteFileW(L"inkpod-chart-smoke.inkchart");
    const std::array<UINT, 14U> light_table_commands{
        IDM_LT_SET_NEW,
        IDM_LT_SET_RENAME,
        IDM_LT_SET_DUPLICATE,
        IDM_LT_SET_UP,
        IDM_LT_SET_DOWN,
        IDM_LT_SET_DELETE,
        IDM_LT_ITEM_ADD,
        IDM_LT_ITEM_ADD,
        IDM_LT_ITEM_DOWN,
        IDM_LT_ITEM_UP,
        IDM_LT_GLOBAL_OPACITY,
        IDM_LT_ITEM_SAMPLE,
        IDM_LT_ITEM_PROPERTIES,
        IDM_LT_ITEM_RELOAD};
    for (std::size_t index = 0U; index < light_table_commands.size(); ++index) {
        if (SendMessageW(
                state.Workspace().windows.window, WM_COMMAND, light_table_commands[index], 0) != 1) {
            DeleteFileW(state.lifetime.smoke_raster_path.c_str());
            state.lifetime.smoke_raster_path.clear();
            return 414 + static_cast<int>(index);
        }
    }
    InkpodLightTableItemInfo light_item{};
    if (!QueryLightTableItem(state, state.Workspace().panes.active_light_table_item_index, light_item)
        || light_item.opacity_milli != 500U
        || light_item.effective_opacity_milli != 250U
        || light_item.translate_x_milli != 1000
        || light_item.translate_y_milli != -1000) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 408;
    }
    inkpod::renderer::CanvasDocumentBounds light_canvas{};
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LT_ITEM_MOVE, 0) != 1
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, light_canvas)) {
        return 470;
    }
    const InkpodStrokeSample light_move_start{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(light_canvas.left + 2.0),
        static_cast<float>(light_canvas.top + 2.0), 1.0F, 0U};
    const InkpodStrokeSample light_move_end{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(light_canvas.left + 12.0),
        static_cast<float>(light_canvas.top + 12.0), 1.0F, 0U};
    const inkpod::renderer::CanvasStrokeEvent light_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &light_move_start, 1U};
    const inkpod::renderer::CanvasStrokeEvent light_end{
        inkpod::renderer::CanvasStrokeEventKind::End, &light_move_end, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, light_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, light_end)
        || !QueryLightTableItem(state, state.Workspace().panes.active_light_table_item_index, light_item)
        || (light_item.translate_x_milli == 1000
            && light_item.translate_y_milli == -1000)) {
        return 471;
    }
    const std::wstring swap_save = L"inkpod-lt-smoke.inkpod";
    DeleteFileW(swap_save.c_str());
    if (SaveToPath(state, swap_save) != INKPOD_STATUS_OK
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_LT_ITEM_SWAP, 0) != 1) {
        DeleteFileW(swap_save.c_str());
        DeleteFileW((swap_save + L".recovery.inkpod").c_str());
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 409;
    }
    DeleteFileW(swap_save.c_str());
    DeleteFileW((swap_save + L".recovery.inkpod").c_str());
    std::vector<std::uint8_t> sequence_source;
    if (!ReadBoundedFile(state.lifetime.smoke_raster_path, sequence_source)) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 410;
    }
    try {
        state.lifetime.smoke_sequence_paths = {L"cell10.png", L"cell1.png", L"cell3.png"};
    } catch (const std::bad_alloc&) {
        DeleteFileW(state.lifetime.smoke_raster_path.c_str());
        state.lifetime.smoke_raster_path.clear();
        return 410;
    }
    for (const auto& path : state.lifetime.smoke_sequence_paths) {
        DeleteFileW(path.c_str());
        if (!WriteFileAtomically(path, sequence_source)) {
            return 410;
        }
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_IMPORT, 0) != 1) {
        return 411;
    }
    RefreshSequencePane(state);
    if (state.Workspace().panes.sequence_count != 3U
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SUBPALETTE_SET, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SUBPALETTE_SAMPLE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_GOTO, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_PREVIOUS, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_NEXT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_START, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_NEXT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_PAUSE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_PAUSE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_PREVIOUS, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FIRST, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_LAST, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_30, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_25, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_24, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_12, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_10, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_FPS_8, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_MOTION_STOP, 0) != 1) {
        return 412;
    }
    std::wstring active_cell_tab;
    if (!ReadDocumentTabLabel(
            state.Workspace().windows.document_tabs,
            TabCtrl_GetCurSel(state.Workspace().windows.document_tabs),
            active_cell_tab)
        || active_cell_tab != L"cell3.png") {
        return 718;
    }
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    state.lifetime.smoke_raster_path = L"inkpod-sequence-export.png";
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_SEQ_EXPORT, 0) != 1
        || GetFileAttributesW(L"inkpod-sequence-export-cell1.png")
            == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(L"inkpod-sequence-export-cell3.png")
            == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(L"inkpod-sequence-export-cell10.png")
            == INVALID_FILE_ATTRIBUTES) {
        return 413;
    }
    for (const auto& path : state.lifetime.smoke_sequence_paths) {
        DeleteFileW(path.c_str());
    }
    state.lifetime.smoke_sequence_paths.clear();
    DeleteFileW(L"inkpod-sequence-export-cell1.png");
    DeleteFileW(L"inkpod-sequence-export-cell3.png");
    DeleteFileW(L"inkpod-sequence-export-cell10.png");
    DeleteFileW(state.lifetime.smoke_raster_path.c_str());
    state.lifetime.smoke_raster_path.clear();
    return 0;
}

int RunVectorWorkflowSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 500;
    }
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d35000000000001),
        UINT64_C(0x4d35000000000002),
        64U,
        64U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 501;
    }
    ResetUiForNewActiveDocument(state);
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 501;
    }

    InkpodSnapshotVectorSegment geometry_before{};
    std::uint64_t vector_layer_id{};
    std::uint64_t vector_trace_plane_id{};
    std::uint64_t vector_path_id{};
    std::uint64_t vector_fill_id{};
    const InkpodStatus vector_status = state.engine->Invoke(
        [&geometry_before,
         &vector_layer_id,
         &vector_trace_plane_id,
         &vector_path_id,
         &vector_fill_id](InkpodCore* core) {
            static constexpr std::array<std::uint8_t, 6U> name{
                'V', 'e', 'c', 't', 'o', 'r'};
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CREATE_LAYER;
            edit.kind = INKPOD_LAYER_VECTOR_COLORING;
            edit.name_utf8 = name.data();
            edit.name_bytes = name.size();
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            InkpodStatus status = inkpod_core_tree_edit(
                core, &edit, &result, &vector_layer_id);
            InkpodNodeInfo trace{};
            trace.struct_size = sizeof(trace);
            InkpodNodeInfo fill{};
            fill.struct_size = sizeof(fill);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 1U, &trace);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 2U, &fill);
            }
            if (status != INKPOD_STATUS_OK || vector_layer_id == 0U
                || trace.kind != INKPOD_TYPED_PLANE_COLOR_TRACE
                || fill.kind != INKPOD_TYPED_PLANE_VECTOR_FILL) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE
                    : status;
            }
            vector_trace_plane_id = trace.id;
            constexpr auto point = [](float x, float y) noexcept {
                return InkpodVectorPoint{x, y};
            };
            constexpr auto line = [](InkpodVectorPoint start, InkpodVectorPoint end) noexcept {
                return InkpodVectorCubicSegment{
                    sizeof(InkpodVectorCubicSegment),
                    0U,
                    start,
                    InkpodVectorPoint{
                        (start.x * 2.0F + end.x) / 3.0F,
                        (start.y * 2.0F + end.y) / 3.0F},
                    InkpodVectorPoint{
                        (start.x + end.x * 2.0F) / 3.0F,
                        (start.y + end.y * 2.0F) / 3.0F},
                    end,
                    1.0F,
                    5.0F};
            };
            constexpr std::array<InkpodVectorPoint, 5U> corners{
                point(8.0F, 8.0F),
                point(56.0F, 8.0F),
                point(56.0F, 56.0F),
                point(8.0F, 56.0F),
                point(8.0F, 8.0F)};
            const std::array<InkpodVectorCubicSegment, 4U> segments{
                line(corners[0], corners[1]),
                line(corners[1], corners[2]),
                line(corners[2], corners[3]),
                line(corners[3], corners[4])};
            const InkpodVectorPathInput path{
                sizeof(InkpodVectorPathInput),
                0U,
                INKPOD_VECTOR_PATH_CLOSED,
                trace.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    20U,
                    40U,
                    220U,
                    255U},
                segments.data(),
                segments.size(),
                sizeof(InkpodVectorCubicSegment)};
            status = inkpod_core_vector_add_path(
                core, &path, &result, &vector_path_id);
            const InkpodVectorFillInput topology{
                sizeof(InkpodVectorFillInput),
                0U,
                0U,
                fill.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    240U,
                    120U,
                    20U,
                    180U},
                &vector_path_id,
                1U};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_vector_add_fill(
                    core, &topology, &result, &vector_fill_id);
            }
            const InkpodSnapshotOptions snapshot_options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot(
                    core, &snapshot_options, &snapshot);
            }
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK
                && (vectors.segment_count != 4U || vectors.fill_count != 1U
                    || vectors.boundary_path_count != 1U || vectors.segments == nullptr
                    || vectors.fills == nullptr || vectors.boundary_path_ids == nullptr
                    || vectors.segments->path_id != vector_path_id
                    || vectors.fills->fill_id != vector_fill_id
                    || *vectors.boundary_path_ids != vector_path_id)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status == INKPOD_STATUS_OK) {
                geometry_before = *vectors.segments;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        true,
        true);
    if (vector_status != INKPOD_STATUS_OK || vector_layer_id == 0U
        || vector_trace_plane_id == 0U || vector_path_id == 0U || vector_fill_id == 0U) {
        return 502;
    }
    if (ApplyView(state, INKPOD_VIEW_ZOOM_AT, 2.0, 32.0, 32.0)
        != INKPOD_STATUS_OK) {
        return 503;
    }
    bool geometry_unchanged{};
    const InkpodStatus zoom_status = state.engine->Invoke(
        [&geometry_before, &geometry_unchanged](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK && vectors.segment_count != 0U
                && vectors.segments != nullptr) {
                const InkpodSnapshotVectorSegment& after = *vectors.segments;
                geometry_unchanged = after.path_id == geometry_before.path_id
                    && after.p0.x == geometry_before.p0.x
                    && after.p0.y == geometry_before.p0.y
                    && after.p1.x == geometry_before.p1.x
                    && after.p1.y == geometry_before.p1.y
                    && after.p2.x == geometry_before.p2.x
                    && after.p2.y == geometry_before.p2.y
                    && after.p3.x == geometry_before.p3.x
                    && after.p3.y == geometry_before.p3.y
                    && after.width_start == geometry_before.width_start
                    && after.width_end == geometry_before.width_end;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (zoom_status != INKPOD_STATUS_OK || !geometry_unchanged) {
        return 504;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            inkpod::renderer::kCanvasRenderOnce,
            0,
            0) != 1) {
        return 505;
    }
    if (SendMessageW(
            state.Workspace().windows.canvas,
            inkpod::renderer::kCanvasValidateClosedVectorStroke,
            0,
            0) != 1) {
        return 507;
    }

    state.Workspace().panes.active_tree_layer_id = vector_layer_id;
    state.Workspace().panes.active_tree_plane_id = vector_trace_plane_id;
    if (!RefreshTreePane(state)) {
        return 506;
    }
    UpdateMenuState(state);
    for (const UINT command : {
             IDM_VECTOR_LINE, IDM_VECTOR_CURVE, IDM_VECTOR_RECTANGLE,
             IDM_VECTOR_ELLIPSE, IDM_VECTOR_POLYLINE, IDM_VECTOR_ERASER}) {
        const UINT command_state = GetMenuState(GetMenu(state.Workspace().windows.window), command, MF_BYCOMMAND);
        if (command_state == static_cast<UINT>(-1)
            || (command_state & (MF_DISABLED | MF_GRAYED)) != 0U) {
            return 508;
        }
    }
    inkpod::renderer::CanvasDocumentBounds canvas_bounds{};
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, canvas_bounds)) {
        return 506;
    }
    const double canvas_zoom = (canvas_bounds.right - canvas_bounds.left)
        / static_cast<double>(document.width);
    const auto sample = [&](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(canvas_bounds.left + static_cast<double>(x) * canvas_zoom),
            static_cast<float>(canvas_bounds.top + static_cast<double>(y) * canvas_zoom),
            1.0F,
            0U};
    };
    const auto gesture = [&](UINT command, const std::array<InkpodStrokeSample, 4U>& samples) {
        if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0) != 1) {
            return false;
        }
        const inkpod::renderer::CanvasStrokeEvent begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
        const inkpod::renderer::CanvasStrokeEvent append{
            inkpod::renderer::CanvasStrokeEventKind::Append, samples.data() + 1U, 2U};
        const inkpod::renderer::CanvasStrokeEvent end{
            inkpod::renderer::CanvasStrokeEventKind::End, samples.data() + 3U, 1U};
        return inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, begin)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, append)
            && inkpod::renderer::SubmitCanvasStrokeEvent(
                   state.Workspace().windows.canvas, end);
    };
    const std::array<InkpodStrokeSample, 4U> line_samples{
        sample(4.0F, 8.0F), sample(5.0F, 8.0F),
        sample(6.0F, 8.0F), sample(7.0F, 8.0F)};
    const std::array<InkpodStrokeSample, 4U> curve_samples{
        sample(10.0F, 12.0F), sample(16.0F, 4.0F),
        sample(24.0F, 20.0F), sample(30.0F, 12.0F)};
    const std::array<InkpodStrokeSample, 4U> shape_samples{
        sample(34.0F, 10.0F), sample(38.0F, 16.0F),
        sample(44.0F, 24.0F), sample(50.0F, 30.0F)};
    const std::array<InkpodStrokeSample, 4U> polyline_samples{
        sample(10.0F, 40.0F), sample(20.0F, 45.0F),
        sample(28.0F, 38.0F), sample(36.0F, 48.0F)};
    if (!gesture(IDM_VECTOR_LINE, line_samples)
        || !gesture(IDM_VECTOR_CURVE, curve_samples)
        || !gesture(IDM_VECTOR_RECTANGLE, shape_samples)
        || !gesture(IDM_VECTOR_ELLIPSE, shape_samples)
        || !gesture(IDM_VECTOR_POLYLINE, polyline_samples)) {
        return 507;
    }
    const std::array<UINT, 8U> selection_commands{
        IDM_VECTOR_SELECT_CUT,
        IDM_VECTOR_SELECT_TOUCH,
        IDM_VECTOR_SELECT_CONTAINED,
        IDM_VECTOR_SELECT_LINE,
        IDM_VECTOR_SELECT_WHOLE_LINE,
        IDM_VECTOR_SELECT_INTERSECTION,
        IDM_VECTOR_SELECT_FILL_BOUNDARY,
        IDM_VECTOR_SELECT_FILL};
    for (std::size_t index = 0U; index < selection_commands.size(); ++index) {
        const UINT command = selection_commands[index];
        if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, command, 0) != 1) {
            return 520 + static_cast<int>(index);
        }
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_SELECT_TOUCH, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_WIDTH, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_CONNECT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_ERASE_WHOLE, 0) != 1) {
        return 509;
    }
    const inkpod::renderer::CanvasStrokeEvent erase_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, line_samples.data() + 2U, 1U};
    const inkpod::renderer::CanvasStrokeEvent erase_end{
        inkpod::renderer::CanvasStrokeEventKind::End, line_samples.data() + 2U, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, erase_begin)
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, erase_end)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_RASTERIZE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_VECTOR_VECTORIZE, 0) != 1) {
        return 510;
    }
    return 0;
}

int RunImageEffectsSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return 600;
    }
    bool preview_cancelled{};
    bool undo_restored{};
    bool adjustment_preserved_source{};
    bool effect_connected{};
    const InkpodStatus status = state.engine->Invoke(
        [&preview_cancelled, &undo_restored, &adjustment_preserved_source, &effect_connected](
            InkpodCore* core) {
            InkpodDocumentInfo document = EmptyDocumentInfo();
            InkpodStatus current = inkpod_core_get_document_info(core, &document);
            const std::array<InkpodStrokeSample, 1U> samples{
                InkpodStrokeSample{
                    sizeof(InkpodStrokeSample), 0U, 32.0F, 32.0F, 1.0F, 0U}};
            InkpodStrokeInput stroke{
                sizeof(InkpodStrokeInput),
                INKPOD_TOOL_PENCIL,
                INKPOD_PLANE_COLOR,
                INKPOD_COORDINATE_SPACE_DOCUMENT,
                0U,
                UINT32_C(0x204080ff),
                3.0F,
                samples.data(),
                samples.size(),
                sizeof(InkpodStrokeSample)};
            InkpodDispatchResult dispatch{};
            dispatch.struct_size = sizeof(dispatch);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_apply_stroke(core, &stroke, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
            }
            const std::uint64_t original = document.color_plane_checksum;
            InkpodFilterInput filter{};
            filter.struct_size = sizeof(filter);
            filter.kind = INKPOD_FILTER_INVERT;
            filter.plane_id = document.color_plane_id;
            filter.channel = INKPOD_FILTER_CHANNEL_RGB;
            InkpodFilterPreviewInfo preview{};
            preview.struct_size = sizeof(preview);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK
                && (preview.base_checksum != original
                    || preview.preview_checksum == original)) {
                current = INKPOD_STATUS_INVALID_STATE;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_cancel(core, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                preview_cancelled = document.color_plane_checksum == original
                    && preview.preview_checksum == original;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_apply(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_undo(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                undo_restored = document.color_plane_checksum == original;
            }
            filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
            filter.parameter_0 = 100;
            filter.parameter_1 = 200;
            static constexpr std::array<std::uint8_t, 13U> name{
                'M', '6', ' ', 'A', 'd', 'j', 'u', 's', 't', 'm', 'e', 'n', 't'};
            std::uint64_t adjustment_id{};
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_adjustment_create(
                    core,
                    &filter,
                    name.data(),
                    name.size(),
                    &dispatch,
                    &adjustment_id);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                adjustment_preserved_source = adjustment_id != 0U
                    && document.color_plane_checksum == original;
            }
            const auto color16 = [](std::uint16_t red,
                                    std::uint16_t green,
                                    std::uint16_t blue,
                                    std::uint16_t alpha) noexcept {
                InkpodColorValue color{};
                color.struct_size = sizeof(color);
                color.depth = INKPOD_COLOR_DEPTH_16;
                color.red = red;
                color.green = green;
                color.blue = blue;
                color.alpha = alpha;
                return color;
            };
            std::array<InkpodGradientStop, 3U> stops{};
            for (auto& stop : stops) {
                stop.struct_size = sizeof(stop);
            }
            stops[0].position_milli = 0U;
            stops[0].color = color16(65535U, 0U, 0U, 65535U);
            stops[1].position_milli = 500U;
            stops[1].color = color16(0U, 65535U, 0U, 32768U);
            stops[2].position_milli = 1000U;
            stops[2].color = color16(0U, 0U, 65535U, 65535U);
            InkpodGradientInput gradient{};
            gradient.struct_size = sizeof(gradient);
            gradient.kind = INKPOD_GRADIENT_LINEAR;
            gradient.plane_id = document.color_plane_id;
            gradient.mode = INKPOD_GRADIENT_OVERWRITE;
            gradient.start_x_milli = 500;
            gradient.start_y_milli = 500;
            gradient.end_x_milli = 63500;
            gradient.end_y_milli = 500;
            gradient.stops = stops.data();
            gradient.stop_count = stops.size();
            gradient.stop_stride_bytes = sizeof(InkpodGradientStop);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_effect_gradient(core, &gradient, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                effect_connected = document.color_plane_checksum != original;
            }
            return current;
        },
        true,
        true);
    if (status != INKPOD_STATUS_OK || !preview_cancelled || !undo_restored
        || !adjustment_preserved_source || !effect_connected) {
        return 601;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_PLANE_COLOR, 0);
    InkpodDocumentInfo before_menu = EmptyDocumentInfo();
    InkpodDocumentInfo after_menu = EmptyDocumentInfo();
    HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr || !QueryDocument(state, before_menu)
        || GetMenuState(menu, IDM_FILTER_LAST, MF_BYCOMMAND) == static_cast<UINT>(-1)) {
        return 602;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_FILTER_LAST, 0);
    if (!QueryDocument(state, after_menu)
        || after_menu.color_plane_checksum == before_menu.color_plane_checksum) {
        return 603;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    if (state.effects.adjustments.size() != 2U
        || state.effects.adjustments[0].id == state.effects.adjustments[1].id) {
        return 605;
    }
    const std::uint64_t newest_adjustment = state.effects.adjustment_id;
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_PREVIOUS, 0);
    if (state.effects.adjustment_id == newest_adjustment) {
        return 606;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_EDIT, 0);
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_TOGGLE, 0);
    if (state.effects.adjustment_visible) {
        return 607;
    }
    SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_ADJUSTMENT_MOVE_TOP, 0);
    InkpodDocumentInfo before_spray = EmptyDocumentInfo();
    InkpodDocumentInfo after_spray = EmptyDocumentInfo();
    inkpod::renderer::CanvasDocumentBounds spray_bounds{};
    if (!QueryDocument(state, before_spray)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, spray_bounds)) {
        return 608;
    }
    TransitionActiveTool(
        state.Workspace().tools, state.Workspace().windows.canvas, kInteractionEffectAirbrush);
    state.effects.options.parameters = {4000, 1000, 1000, 750, 0};
    state.effects.options.option = true;
    state.effects.options.option2 = true;
    const InkpodStrokeSample spray_sample{
        sizeof(InkpodStrokeSample),
        0U,
        static_cast<float>((spray_bounds.left + spray_bounds.right) / 2.0),
        static_cast<float>((spray_bounds.top + spray_bounds.bottom) / 2.0),
        1.0F,
        0U};
    const inkpod::renderer::CanvasStrokeEvent spray_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &spray_sample, 1U};
    const inkpod::renderer::CanvasStrokeEvent spray_end{
        inkpod::renderer::CanvasStrokeEventKind::End, &spray_sample, 1U};
    if (!inkpod::renderer::SubmitCanvasStrokeEvent(
            state.Workspace().windows.canvas, spray_begin)) {
        return 609;
    }
    const auto spray_timer = state.routing.timers.Find(
        CommandTimerKind::ContinuousSpray);
    if (!spray_timer.has_value()
        || SendMessageW(
               state.Workspace().windows.window,
               WM_TIMER,
               static_cast<WPARAM>(spray_timer->value),
               0) != 0
        || state.effects.samples.size() < 2U
        || !inkpod::renderer::SubmitCanvasStrokeEvent(
               state.Workspace().windows.canvas, spray_end)
        || state.engine->Invoke(
               [](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, after_spray)
        || after_spray.color_plane_checksum == before_spray.color_plane_checksum) {
        return 609;
    }
    return SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 604;
}

int RunBatchWorkflowSmoke(ApplicationHost& state) noexcept {
    constexpr wchar_t settings_path[] = L"inkpod-batch-ui-smoke.inkbatch";
    constexpr wchar_t output_path[] = L"inkpod-batch-windows-smoke_0001.inkpod";
    const auto cleanup = [&]() noexcept {
        DeleteFileW(settings_path);
        DeleteFileW(output_path);
    };
    cleanup();
    if (state.engine == nullptr || state.Workspace().batch_palette == nullptr) {
        return 700;
    }
    HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_WINDOW_BATCH, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_BATCH, 0) != 1
        || IsWindowVisible(state.Workspace().batch_palette) == FALSE) {
        cleanup();
        return 701;
    }
    state.batch.output_folder = L".";
    state.batch.basename = L"inkpod-batch-windows-smoke";
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_INPUT_CURRENT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_INPUT_RANGE, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OUTPUT_SETTINGS, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_ADD_COLOR_REPLACE, 0) != 1
        || state.batch.operations.size() != 1U
        || state.batch.operations[0].color_pairs.size() != 1U) {
        cleanup();
        return 702;
    }
    const InkpodColorValue old_before = state.batch.operations[0].color_pairs[0].old_color;
    const InkpodColorValue new_before = state.batch.operations[0].color_pairs[0].new_color;
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_REPLACE_SWAP, 0) != 1) {
        cleanup();
        return 703;
    }
    const auto& swapped = state.batch.operations[0].color_pairs[0];
    if (std::memcmp(&swapped.old_color, &new_before, sizeof(new_before)) != 0
        || std::memcmp(&swapped.new_color, &old_before, sizeof(old_before)) != 0) {
        cleanup();
        return 704;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_ADD_BOUNDARY_AIRBRUSH, 0) != 1
        || state.batch.operations.size() != 2U
        || state.batch.operations.back().colors.size() < 2U
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_DRY_RUN, 0) != 1
        || state.batch.report == nullptr
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_REMOVE, 0) != 1
        || state.batch.operations.size() != 1U) {
        cleanup();
        return 705;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_ADD_MIRROR, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_UP, 0) != 1
        || state.batch.operations.front().kind != INKPOD_BATCH_OPERATION_MIRROR
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_DOWN, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_EDIT, 0) != 1
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_OPERATION_REMOVE, 0) != 1
        || state.batch.operations.size() != 1U
        || state.batch.operations[0].kind != INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        cleanup();
        return 706;
    }
    const LRESULT input_count = SendDlgItemMessageW(
        state.Workspace().batch_palette, IDC_BATCH_INPUTS, LB_GETCOUNT, 0, 0);
    const LRESULT operation_count = SendDlgItemMessageW(
        state.Workspace().batch_palette, IDC_BATCH_OPERATIONS, LB_GETCOUNT, 0, 0);
    if (input_count != 1 || operation_count != 1
        || GetDlgItem(state.Workspace().batch_palette, IDC_BATCH_OUTPUT) == nullptr) {
        cleanup();
        return 707;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_SAVE_SET, 0) != 1
        || GetFileAttributesW(settings_path) == INVALID_FILE_ATTRIBUTES
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_LOAD_SET, 0) != 1
        || !state.batch.loaded_graph || state.batch.graph == nullptr) {
        cleanup();
        return 708;
    }
    InkpodBatchGraphInfo graph_info{};
    graph_info.struct_size = sizeof(graph_info);
    if (inkpod_batch_graph_get_info(state.batch.graph, &graph_info) != INKPOD_STATUS_OK
        || graph_info.version != INKPOD_BATCH_GRAPH_VERSION
        || graph_info.operation_count != 1U
        || graph_info.output_policy != INKPOD_BATCH_OUTPUT_DUPLICATE) {
        cleanup();
        return 709;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_PREVIEW, 0) != 1
        || state.batch.preview == nullptr
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_DRY_RUN, 0) != 1
        || state.batch.report == nullptr
        || GetFileAttributesW(output_path) != INVALID_FILE_ATTRIBUTES) {
        cleanup();
        return 710;
    }
    InkpodBatchReportInfo report_info{};
    report_info.struct_size = sizeof(report_info);
    InkpodBatchReportItem report_item{};
    report_item.struct_size = sizeof(report_item);
    if (inkpod_batch_report_get_info(state.batch.report, &report_info) != INKPOD_STATUS_OK
        || report_info.failure_count != 0U || report_info.item_count == 0U
        || inkpod_batch_report_get(state.batch.report, 0U, &report_item)
            != INKPOD_STATUS_OK
        || report_item.outcome != INKPOD_BATCH_ITEM_DRY_RUN) {
        cleanup();
        return 711;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_BATCH_RUN_CURRENT, 0) != 1
        || GetFileAttributesW(output_path) == INVALID_FILE_ATTRIBUTES
        || inkpod_batch_report_get_info(state.batch.report, &report_info) != INKPOD_STATUS_OK
        || report_info.failure_count != 0U) {
        cleanup();
        return 712;
    }
    if (SendMessageW(state.Workspace().windows.window, WM_COMMAND, IDM_WINDOW_BATCH, 0) != 1
        || IsWindowVisible(state.Workspace().batch_palette) != FALSE) {
        cleanup();
        return 713;
    }
    cleanup();
    return 0;
}

int RunMagnifiedRasterHitSmoke(ApplicationHost& state) noexcept {
    if (state.engine == nullptr
        || CreateCell(state, 8U, 8U, 96'000U) != INKPOD_STATUS_OK) {
        return 720;
    }
    state.Workspace().tools.active_plane = INKPOD_PLANE_MAIN_LINE;
    TransitionActiveTool(state.Workspace().tools, state.Workspace().windows.canvas, INKPOD_TOOL_PENCIL);

    InkpodDocumentInfo blank = EmptyDocumentInfo();
    InkpodDocumentInfo seeded = EmptyDocumentInfo();
    const InkpodStrokeSample source{
        sizeof(InkpodStrokeSample), 0U, 3.0F, 3.0F, 1.0F, 0U};
    const InkpodStrokeInput stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        INKPOD_FEATURE_NONE,
        UINT32_C(0x000000ff),
        1.0F,
        &source,
        1U,
        sizeof(source)};
    if (!QueryDocument(state, blank)
        || state.engine->Invoke(
               [stroke](InkpodCore* core) {
                   InkpodStatus status = inkpod_core_set_active_plane(
                       core, INKPOD_PLANE_MAIN_LINE);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_core_apply_stroke(core, &stroke, &result);
                   }
                   return status;
               },
               true,
               true) != INKPOD_STATUS_OK
        || !QueryDocument(state, seeded)
        || seeded.main_plane_checksum == blank.main_plane_checksum) {
        return 721;
    }

    inkpod::renderer::CanvasDocumentBounds bounds{};
    RECT client{};
    if (GetClientRect(state.Workspace().windows.canvas, &client) == FALSE
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)) {
        return 722;
    }
    const double initial_zoom = (bounds.right - bounds.left) / 8.0;
    if (!std::isfinite(initial_zoom) || initial_zoom <= 0.0
        || ApplyView(
               state,
               INKPOD_VIEW_ZOOM_AT,
               64.0 / initial_zoom,
               static_cast<double>(client.right - client.left) / 2.0,
               static_cast<double>(client.bottom - client.top) / 2.0)
            != INKPOD_STATUS_OK
        || SendMessageW(
               state.Workspace().windows.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) != 1) {
        return 722;
    }

    bounds = {};
    if (!inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, bounds)) {
        return 723;
    }
    const double zoom = (bounds.right - bounds.left) / 8.0;
    if (std::abs(zoom - 64.0) > 0.001) {
        return 724;
    }
    const int device_x = static_cast<int>(std::lround(bounds.left + 3.75 * zoom));
    const int device_y = static_cast<int>(std::lround(bounds.top + 3.75 * zoom));
    if (SendMessageW(
            state.Workspace().windows.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(device_x, device_y)) != 1
        || SendMessageW(
               state.Workspace().windows.canvas,
               WM_LBUTTONUP,
               0,
               MAKELPARAM(device_x, device_y)) != 1
        || state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 725;
    }

    InkpodDocumentInfo erased = EmptyDocumentInfo();
    return QueryDocument(state, erased)
            && erased.document_revision == seeded.document_revision + 1U
            && erased.main_plane_checksum == blank.main_plane_checksum
        ? 0
        : 726;
}

int RunMultiDocumentTabSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::DocumentSessionId;
    using inkpod::app::DocumentViewId;
    using inkpod::app::Generation;

    InkpodDocumentInfo initial_probe = EmptyDocumentInfo();
    if (state.engine == nullptr || !QueryDocument(state, initial_probe)) {
        return 732;
    }
    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 733;
    }
    const auto suffix = static_cast<unsigned long long>(GetTickCount64());
    std::array<wchar_t, MAX_PATH> first_buffer{};
    std::array<wchar_t, MAX_PATH> second_buffer{};
    _snwprintf_s(
        first_buffer.data(),
        first_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-g5-first-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    _snwprintf_s(
        second_buffer.data(),
        second_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-g5-second-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    const std::wstring first_path(first_buffer.data());
    const std::wstring second_path(second_buffer.data());
    DeleteFileW(first_path.c_str());
    DeleteFileW(second_path.c_str());
    const auto cleanup = [&first_path, &second_path]() noexcept {
        DeleteFileW(first_path.c_str());
        DeleteFileW(second_path.c_str());
    };

    const std::size_t baseline_count = state.Documents().Count();
    const DocumentSessionId first_session = state.Document().id;
    const Generation first_generation = state.Document().generation;
    const DocumentViewId first_view = state.ActiveView().id;
    InkpodDocumentInfo first_saved = EmptyDocumentInfo();
    InkpodHistoryInfo first_history{};
    first_history.struct_size = sizeof(first_history);
    if (baseline_count == 0U
        || SaveToPath(state, first_path) != INKPOD_STATUS_OK
        || !QueryDocument(state, first_saved)
        || (first_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&first_history](InkpodCore* core) {
                   return inkpod_core_history_info(core, &first_history);
               },
               false,
               false) != INKPOD_STATUS_OK
        || state.RecentDocumentAt(0U) == nullptr
        || state.RecentDocumentAt(0U)->path != first_path) {
        cleanup();
        return 734;
    }

    if (CreateCell(state, 96U, 64U, 96000U) != INKPOD_STATUS_OK
        || state.Documents().Count() != baseline_count + 1U
        || state.engine->SessionCount() != state.Documents().Count()
        || state.Document().id == first_session
        || !DocumentTabsMatchRegistry(state)) {
        cleanup();
        return 735;
    }
    const DocumentSessionId second_session = state.Document().id;
    const Generation second_generation = state.Document().generation;
    const DocumentViewId second_view = state.ActiveView().id;
    InkpodDocumentInfo second_initial = EmptyDocumentInfo();
    if (!QueryDocument(state, second_initial)
        || state.CloseDocumentView(DocumentViewId{UINT64_MAX})) {
        cleanup();
        return 736;
    }

    state.Workspace().tools.active_plane = INKPOD_PLANE_MAIN_LINE;
    TransitionActiveTool(
        state.Workspace().tools,
        state.Workspace().windows.canvas,
        INKPOD_TOOL_PENCIL);
    if (state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_set_active_plane(
                    core, INKPOD_PLANE_MAIN_LINE);
            },
            false,
            false) != INKPOD_STATUS_OK
        || SendMessageW(
               state.Workspace().windows.canvas,
               WM_LBUTTONDOWN,
               MK_LBUTTON,
               MAKELPARAM(120, 120)) != 1
        || !ActivateDocumentTab(state, first_view)
        || state.engine->WaitIdle(second_session, second_generation)
            != INKPOD_STATUS_OK) {
        cleanup();
        return 737;
    }
    InkpodDocumentInfo second_cancelled = EmptyDocumentInfo();
    if (!state.engine->GetDocumentInfo(
            second_session, second_generation, second_cancelled)
        || second_cancelled.document_revision
            != second_initial.document_revision
        || second_cancelled.main_plane_checksum
            != second_initial.main_plane_checksum
        || !ActivateDocumentTab(state, second_view)) {
        cleanup();
        return 738;
    }

    const InkpodStrokeSample sample{
        sizeof(InkpodStrokeSample), 0U, 12.0F, 12.0F, 1.0F, 0U};
    const InkpodStrokeInput stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        INKPOD_FEATURE_NONE,
        UINT32_C(0x000000ff),
        1.0F,
        &sample,
        1U,
        sizeof(sample)};
    if (state.engine->Invoke(
            [stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        cleanup();
        return 739;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo second_edited = EmptyDocumentInfo();
    std::wstring dirty_label;
    const int dirty_index = TabCtrl_GetCurSel(
        state.Workspace().windows.document_tabs);
    if (!QueryDocument(state, second_edited)
        || second_edited.main_plane_checksum
            == second_initial.main_plane_checksum
        || (second_edited.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || !ReadDocumentTabLabel(
            state.Workspace().windows.document_tabs, dirty_index, dirty_label)
        || !dirty_label.ends_with(L" *")) {
        cleanup();
        return 740;
    }

    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    InkpodDocumentInfo second_undone = EmptyDocumentInfo();
    if (!QueryDocument(state, second_undone)
        || second_undone.main_plane_checksum
            != second_initial.main_plane_checksum) {
        cleanup();
        return 742;
    }
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    InkpodDocumentInfo second_redone = EmptyDocumentInfo();
    if (!QueryDocument(state, second_redone)
        || second_redone.main_plane_checksum
            != second_edited.main_plane_checksum
        || SaveToPath(state, second_path) != INKPOD_STATUS_OK
        || state.RecentDocumentAt(0U) == nullptr
        || state.RecentDocumentAt(0U)->path != second_path) {
        cleanup();
        return 743;
    }
    InkpodDocumentInfo second_saved = EmptyDocumentInfo();
    const inkpod::app::CommandContext second_async_context =
        state.routing.targets.Capture();
    if (!QueryDocument(state, second_saved)
        || (second_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || OpenDocumentFromPath(state, first_path) != INKPOD_STATUS_OK
        || state.Document().id != first_session
        || state.Documents().Count() != baseline_count + 1U
        || state.RecentDocumentAt(0U) == nullptr
        || state.RecentDocumentAt(0U)->path != first_path) {
        cleanup();
        return 744;
    }
    const std::size_t recent_count_before_missing =
        state.RecentDocumentCount();
    const std::wstring missing_recent_path = first_path + L".missing";
    inkpod::app::DocumentIdentity missing_recent_identity{};
    if (!inkpod::app::ResolveDocumentFileIdentity(
            missing_recent_path, missing_recent_identity)
        || !state.RecordRecentDocument(
            missing_recent_path, std::move(missing_recent_identity))) {
        cleanup();
        return 744;
    }
    UpdateMenuState(state);
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_FILE_RECENT_1,
            0) != 0
        || state.RecentDocumentCount() != recent_count_before_missing
        || state.RecentDocumentAt(0U) == nullptr
        || state.RecentDocumentAt(0U)->path != first_path) {
        cleanup();
        return 744;
    }

    const auto first_command_states = state.Workspace().command_states;
    const InkpodSelectionInput second_selection{
        sizeof(InkpodSelectionInput),
        INKPOD_SELECTION_RECTANGLE,
        INKPOD_SELECTION_NEW,
        INKPOD_FEATURE_NONE,
        {2, 3, 7, 5},
        nullptr,
        0U,
        0U};
    if (!state.engine->Enqueue(
            second_async_context,
            [second_selection](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(
                    core, &second_selection, &result);
            },
            true,
            true,
            false)
        || state.engine->WaitIdle(second_session, second_generation)
            != INKPOD_STATUS_OK) {
        cleanup();
        return 745;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo second_after_async = EmptyDocumentInfo();
    InkpodLocatorOutput second_locator{};
    second_locator.struct_size = sizeof(second_locator);
    InkpodLocatorOutput first_locator{};
    first_locator.struct_size = sizeof(first_locator);
    if (state.Document().id != first_session
        || !SameCommandStates(
            first_command_states, state.Workspace().command_states)
        || !state.engine->GetDocumentInfo(
            second_session, second_generation, second_after_async)
        || (second_after_async.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || state.engine->Invoke(
               second_session,
               second_generation,
               [&second_locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, 0U, 0.0, 0.0, &second_locator);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (second_locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U
        || second_locator.selection.x != second_selection.bounds.x
        || second_locator.selection.y != second_selection.bounds.y
        || second_locator.selection.width != second_selection.bounds.width
        || second_locator.selection.height != second_selection.bounds.height
        || state.engine->Invoke(
               [&first_locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, 0U, 0.0, 0.0, &first_locator);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (first_locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U) {
        cleanup();
        return 745;
    }

    InkpodDocumentInfo first_after = EmptyDocumentInfo();
    InkpodHistoryInfo first_history_after{};
    first_history_after.struct_size = sizeof(first_history_after);
    if (!QueryDocument(state, first_after)
        || first_after.document_revision != first_saved.document_revision
        || first_after.main_plane_checksum != first_saved.main_plane_checksum
        || state.engine->Invoke(
               first_session,
               first_generation,
               [&first_history_after](InkpodCore* core) {
                   return inkpod_core_history_info(core, &first_history_after);
               },
               false,
               false) != INKPOD_STATUS_OK
        || first_history_after.cursor != first_history.cursor
        || first_history_after.item_count != first_history.item_count
        || SaveToPath(state, second_path) != INKPOD_STATUS_INVALID_STATE
        || state.Document().shell.current_path != first_path) {
        cleanup();
        return 745;
    }

    const InkpodStrokeSample first_prompt_sample{
        sizeof(InkpodStrokeSample), 0U, 6.0F, 6.0F, 1.0F, 0U};
    const InkpodStrokeInput first_prompt_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        INKPOD_FEATURE_NONE,
        UINT32_C(0x000000ff),
        1.0F,
        &first_prompt_sample,
        1U,
        sizeof(first_prompt_sample)};
    if (state.engine->Invoke(
            [first_prompt_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &first_prompt_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        cleanup();
        return 745;
    }
    std::uint32_t expected_dirty_prompts{};
    for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
        const auto* document = state.Documents().SessionAt(index);
        InkpodDocumentInfo document_info = EmptyDocumentInfo();
        if (document != nullptr && state.engine->GetDocumentInfo(
                document->id, document->generation, document_info)
            && (document_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
            ++expected_dirty_prompts;
        }
    }
    state.lifetime.smoke_dirty_prompt_choice = IDCANCEL;
    state.lifetime.smoke_dirty_prompt_count = 0U;
    if (expected_dirty_prompts < 2U || ConfirmAllDocuments(state)
        || state.lifetime.smoke_dirty_prompt_count != 1U
        || state.Documents().Count() != baseline_count + 1U) {
        cleanup();
        return 745;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    state.lifetime.smoke_dirty_prompt_count = 0U;
    if (!ConfirmAllDocuments(state)
        || state.lifetime.smoke_dirty_prompt_count != expected_dirty_prompts
        || state.Documents().Count() != baseline_count + 1U
        || !ActivateDocumentTab(state, first_view)) {
        cleanup();
        return 745;
    }

    const std::size_t first_view_count = state.Document().ViewCount();
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_VIEW_NEW,
            0) != 1
        || state.Document().id != first_session
        || state.Document().ViewCount() != first_view_count + 1U
        || !DocumentTabsMatchRegistry(state)
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_VIEW_CLOSE,
               0) != 1
        || state.Document().ViewCount() != first_view_count
        || !DocumentTabsMatchRegistry(state)) {
        cleanup();
        return 746;
    }

    const DocumentViewId before_next =
        state.routing.targets.ActiveDocumentView();
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_TAB_NEXT, 0);
    const DocumentViewId after_next =
        state.routing.targets.ActiveDocumentView();
    SendMessageW(
        state.Workspace().windows.window, WM_COMMAND, IDM_TAB_PREVIOUS, 0);
    if (after_next == before_next
        || state.routing.targets.ActiveDocumentView() != before_next
        || !DocumentTabsMatchRegistry(state)
        || !ActivateDocumentTab(state, second_view)
        || SaveToPath(state, second_path) != INKPOD_STATUS_OK
        || !QueryDocument(state, second_saved)
        || (second_saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        cleanup();
        return 747;
    }

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_DOCUMENT_CLOSE,
            0) != 1
        || state.Documents().Count() != baseline_count
        || state.engine->HasSession(second_session, second_generation)
        || state.Documents().Find(second_session) != nullptr
        || OpenDocumentFromPath(state, second_path) != INKPOD_STATUS_OK
        || state.Documents().Count() != baseline_count + 1U
        || state.Document().id == second_session) {
        cleanup();
        return 748;
    }
    InkpodDocumentInfo second_reopened = EmptyDocumentInfo();
    InkpodLocatorOutput reopened_locator{};
    reopened_locator.struct_size = sizeof(reopened_locator);
    if (!QueryDocument(state, second_reopened)
        || !SamePersistentMetadata(second_saved, second_reopened)
        || (second_reopened.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || state.engine->Invoke(
               [&reopened_locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, 0U, 0.0, 0.0, &reopened_locator);
               },
               false,
               false) != INKPOD_STATUS_OK
        || (reopened_locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U
        || reopened_locator.selection.x != second_selection.bounds.x
        || reopened_locator.selection.y != second_selection.bounds.y
        || reopened_locator.selection.width != second_selection.bounds.width
        || reopened_locator.selection.height != second_selection.bounds.height
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_VIEW_CLOSE,
               0) != 1
        || state.Documents().Count() != baseline_count
        || !ActivateDocumentTab(state, first_view)
        || !DocumentTabsMatchRegistry(state)) {
        cleanup();
        return 749;
    }
    cleanup();
    return 0;
}

int RunSplitEditorGroupSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::DocumentSessionId;
    using inkpod::app::DocumentViewId;
    using inkpod::app::EditorSplitOrientation;
    using inkpod::app::Generation;

    auto& editors = state.Workspace().editors;
    auto* first_group = editors.Active();
    if (state.engine == nullptr || state.renderer == nullptr
        || first_group == nullptr || editors.GroupCount() != 1U
        || state.routing.targets.EditorGroupCount() != 1U
        || state.renderer->SurfaceCount() != 1U) {
        return 750;
    }
    const auto first_group_id = first_group->id;
    const auto first_canvas_id = first_group->canvas_id;
    const HWND first_canvas = first_group->canvas;
    const DocumentSessionId shared_session = state.Document().id;
    const Generation shared_generation = state.Document().generation;
    const DocumentViewId first_view = state.ActiveView().id;
    const std::size_t original_view_count = state.Document().ViewCount();

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_SPLIT_RIGHT,
            0) != 1) {
        return 751;
    }
    PumpPendingWindowMessages();
    auto* second_group = editors.Active();
    if (second_group == nullptr || second_group->id == first_group_id
        || editors.GroupCount() != 2U
        || editors.Orientation() != EditorSplitOrientation::Vertical
        || state.routing.targets.EditorGroupCount() != 2U
        || state.renderer->SurfaceCount() != 2U
        || state.Document().id != shared_session
        || state.Document().ViewCount() != original_view_count + 1U
        || second_group->canvas == nullptr
        || second_group->canvas == first_canvas
        || state.Workspace().windows.canvas != second_group->canvas
        || state.Workspace().windows.document_tabs
            != second_group->document_tabs) {
        return 752;
    }
    const auto second_group_id = second_group->id;
    const HWND second_canvas = second_group->canvas;
    const DocumentViewId second_view = second_group->ActiveView();
    const auto* first_document_view = state.Document().FindView(first_view);
    const auto* second_document_view = state.Document().FindView(second_view);
    if (!second_view || first_document_view == nullptr
        || second_document_view == nullptr
        || first_document_view->core_view_id == second_document_view->core_view_id
        || state.routing.targets.GroupForView(first_view) != first_group_id
        || state.routing.targets.GroupForView(second_view) != second_group_id) {
        return 753;
    }

    const bool first_flip_presentation =
        first_document_view->presentation.flip_horizontal;
    const auto query_transform = [&state, shared_session, shared_generation](
                                     std::uint64_t core_view,
                                     InkpodSnapshotTransform& transform) noexcept {
        transform = {};
        transform.struct_size = sizeof(transform);
        const InkpodStatus status = state.engine->Invoke(
                   shared_session,
                   shared_generation,
                   [core_view, &transform](InkpodCore* core) {
                       const InkpodSnapshotOptions options{
                           sizeof(InkpodSnapshotOptions),
                           0U,
                           INKPOD_FEATURE_NONE};
                       InkpodSnapshot* snapshot{};
                       const InkpodStatus built = core_view == 0U
                           ? inkpod_core_build_snapshot(
                                 core, &options, &snapshot)
                           : inkpod_core_build_snapshot_for_view(
                                 core, core_view, &options, &snapshot);
                       if (built != INKPOD_STATUS_OK) {
                           return built;
                       }
                       const InkpodStatus read =
                           inkpod_snapshot_get_transform(snapshot, &transform);
                       const InkpodStatus released =
                           inkpod_snapshot_release(&snapshot);
                       return read == INKPOD_STATUS_OK ? released : read;
                   },
                   false,
                   false);
        if (status != INKPOD_STATUS_OK) {
            std::fprintf(
                stderr,
                "G6 transform query failed: core_view=%llu status=%u\n",
                static_cast<unsigned long long>(core_view),
                static_cast<unsigned int>(status));
        }
        return status == INKPOD_STATUS_OK;
    };
    InkpodSnapshotTransform first_before_flip{};
    InkpodSnapshotTransform second_before_flip{};
    InkpodSnapshotTransform first_after_flip{};
    InkpodSnapshotTransform second_after_flip{};
    if (!query_transform(first_document_view->core_view_id, first_before_flip)) {
        return 774;
    }
    if (!query_transform(second_document_view->core_view_id, second_before_flip)) {
        return 775;
    }
    if (SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_VIEW_FLIP_HORIZONTAL,
               0) != 0
        || !state.ActiveView().presentation.flip_horizontal) {
        return 754;
    }
    if (!query_transform(first_document_view->core_view_id, first_after_flip)
        || !query_transform(second_document_view->core_view_id, second_after_flip)
        || (first_before_flip.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            != (first_after_flip.flags
                & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
        || (second_before_flip.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            == (second_after_flip.flags
                & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)) {
        return 755;
    }

    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    if (editors.Active() == nullptr || editors.Active()->id != first_group_id
        || state.ActiveView().id != first_view
        || state.ActiveView().presentation.flip_horizontal
            != first_flip_presentation) {
        return 756;
    }
    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    if (editors.Active() == nullptr || editors.Active()->id != second_group_id
        || state.ActiveView().id != second_view
        || !state.ActiveView().presentation.flip_horizontal) {
        return 757;
    }
    SetFocus(state.Workspace().windows.tool_palette);
    if (editors.Active() == nullptr || editors.Active()->id != second_group_id
        || state.ActiveView().id != second_view) {
        return 782;
    }
    const HWND first_tabs = editors.Find(first_group_id)->document_tabs;
    SetFocus(first_tabs);
    NMHDR first_tab_focus{};
    first_tab_focus.hwndFrom = first_tabs;
    first_tab_focus.idFrom = IDC_MAIN_DOCUMENT_TABS;
    first_tab_focus.code = NM_SETFOCUS;
    SendMessageW(
        state.Workspace().windows.window,
        WM_NOTIFY,
        first_tab_focus.idFrom,
        reinterpret_cast<LPARAM>(&first_tab_focus));
    if (editors.Active() == nullptr || editors.Active()->id != first_group_id
        || state.ActiveView().id != first_view
        || editors.Active()->focus_history != first_tabs) {
        return 783;
    }
    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);

    InkpodDocumentInfo before_edit = EmptyDocumentInfo();
    InkpodDocumentInfo after_edit = EmptyDocumentInfo();
    const InkpodStrokeSample shared_sample{
        sizeof(InkpodStrokeSample), 0U, 9.0F, 9.0F, 1.0F, 0U};
    const InkpodSelectionInput shared_selection{
        sizeof(InkpodSelectionInput),
        INKPOD_SELECTION_RECTANGLE,
        INKPOD_SELECTION_NEW,
        INKPOD_FEATURE_NONE,
        {3, 4, 7, 5},
        nullptr,
        0U,
        0U};
    const auto query_selection = [&state, shared_session, shared_generation](
                                     InkpodLocatorOutput& output) noexcept {
        output = {};
        output.struct_size = sizeof(output);
        return state.engine->Invoke(
                   shared_session,
                   shared_generation,
                   [&output](InkpodCore* core) {
                       return inkpod_core_locator_sample(
                           core, 0U, 0.0, 0.0, &output);
                   },
                   false,
                   false)
            == INKPOD_STATUS_OK;
    };
    InkpodLocatorOutput before_selection{};
    if (!QueryDocument(state, before_edit)) {
        return 776;
    }
    if (!query_selection(before_selection)
        || (before_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U) {
        return 779;
    }
    const InkpodStatus shared_edit_status = state.engine->Invoke(
        shared_session,
        shared_generation,
        [shared_selection](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_apply_selection(
                core, &shared_selection, &result);
        },
        true,
        true);
    if (shared_edit_status != INKPOD_STATUS_OK) {
        std::fprintf(
            stderr,
            "G6 shared edit failed: status=%u\n",
            static_cast<unsigned int>(shared_edit_status));
        return 777;
    }
    if (!QueryDocument(state, after_edit)) {
        return 778;
    }
    InkpodLocatorOutput after_selection{};
    if (after_edit.document_revision != before_edit.document_revision + 1U
        || !query_selection(after_selection)
        || (after_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U) {
        std::fprintf(
            stderr,
            "G6 shared edit mismatch: revisions=%llu/%llu selection_flags=%u\n",
            static_cast<unsigned long long>(before_edit.document_revision),
            static_cast<unsigned long long>(after_edit.document_revision),
            static_cast<unsigned int>(after_selection.flags));
        return 758;
    }
    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    InkpodDocumentInfo shared_from_first = EmptyDocumentInfo();
    InkpodLocatorOutput selection_from_first{};
    if (!QueryDocument(state, shared_from_first)
        || shared_from_first.document_revision != after_edit.document_revision
        || !query_selection(selection_from_first)
        || (selection_from_first.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_UNDO,
               0) != 0) {
        return 759;
    }
    InkpodDocumentInfo undone = EmptyDocumentInfo();
    InkpodLocatorOutput undone_selection{};
    if (!QueryDocument(state, undone)
        || !query_selection(undone_selection)
        || (undone_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDIT_REDO,
               0) != 0) {
        return 760;
    }
    InkpodDocumentInfo redone = EmptyDocumentInfo();
    InkpodLocatorOutput redone_selection{};
    if (!QueryDocument(state, redone)
        || !query_selection(redone_selection)
        || (redone_selection.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U) {
        return 761;
    }

    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    InkpodDocumentInfo before_cancel = EmptyDocumentInfo();
    if (!QueryDocument(state, before_cancel)
        || !renderer::SubmitCanvasStrokeEvent(
            second_canvas,
            renderer::CanvasStrokeEventKind::Begin,
            &shared_sample,
            1U)) {
        return 762;
    }
    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    InkpodDocumentInfo after_cancel = EmptyDocumentInfo();
    if (state.engine->WaitIdle(shared_session, shared_generation)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, after_cancel)
        || after_cancel.document_revision != before_cancel.document_revision) {
        return 763;
    }

    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    const InkpodDocumentInfo shared_before_other_document = after_cancel;
    if (CreateCell(state, 40U, 30U, 96000U) != INKPOD_STATUS_OK
        || state.Document().id == shared_session
        || editors.Active() == nullptr
        || editors.Active()->id != second_group_id) {
        return 764;
    }
    const DocumentSessionId isolated_session = state.Document().id;
    const Generation isolated_generation = state.Document().generation;
    if (state.engine->Invoke(
            isolated_session,
            isolated_generation,
            [shared_selection](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(
                    core, &shared_selection, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 765;
    }
    SendMessageW(first_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(second_canvas), 0);
    InkpodDocumentInfo shared_after_other_document = EmptyDocumentInfo();
    if (!QueryDocument(state, shared_after_other_document)
        || shared_after_other_document.document_revision
            != shared_before_other_document.document_revision) {
        return 766;
    }
    state.lifetime.smoke_dirty_prompt_choice = IDNO;
    SendMessageW(second_canvas, WM_SETFOCUS, reinterpret_cast<WPARAM>(first_canvas), 0);
    if (state.Document().id != isolated_session) {
        return 780;
    }
    const LRESULT isolated_close = SendMessageW(
        state.Workspace().windows.window,
        WM_COMMAND,
        IDM_DOCUMENT_CLOSE,
        0);
    if (isolated_close != 1) {
        std::fprintf(
            stderr,
            "G6 isolated document close failed: result=%lld active=%llu expected=%llu\n",
            static_cast<long long>(isolated_close),
            static_cast<unsigned long long>(state.Document().id.Value()),
            static_cast<unsigned long long>(isolated_session.Value()));
        return 781;
    }
    if (state.engine->HasSession(isolated_session, isolated_generation)
        || state.Documents().Find(isolated_session) != nullptr
        || state.Document().id != shared_session) {
        std::fprintf(
            stderr,
            "G6 isolated close residue: engine=%d registry=%d active=%llu shared=%llu\n",
            state.engine->HasSession(isolated_session, isolated_generation) ? 1 : 0,
            state.Documents().Find(isolated_session) != nullptr ? 1 : 0,
            static_cast<unsigned long long>(state.Document().id.Value()),
            static_cast<unsigned long long>(shared_session.Value()));
        return 767;
    }

    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_SPLIT_DOWN,
            0) != 1
        || editors.Orientation() != EditorSplitOrientation::Horizontal) {
        return 768;
    }
    editors.SetSplitRatioMilli(1U);
    RECT client{};
    if (editors.SplitRatioMilli() != 200U
        || GetClientRect(state.Workspace().windows.window, &client) == FALSE) {
        return 769;
    }
    LayoutMainChrome(
        state.Workspace().windows,
        state.lifetime.smoke_test,
        client.right - client.left,
        client.bottom - client.top);
    editors.SetSplitRatioMilli(999U);
    if (editors.SplitRatioMilli() != 800U) {
        return 770;
    }
    LayoutMainChrome(
        state.Workspace().windows,
        state.lifetime.smoke_test,
        client.right - client.left,
        client.bottom - client.top);
    editors.SetSplitRatioMilli(500U);
    SendMessageW(
        state.Workspace().windows.window,
        WM_SIZE,
        SIZE_RESTORED,
        MAKELPARAM(client.right - client.left, client.bottom - client.top));
    SendMessageW(second_canvas, WM_DPICHANGED_AFTERPARENT, 0, 0);
    if (editors.GroupCount() != 2U || state.renderer->SurfaceCount() != 2U
        || IsWindow(first_canvas) == FALSE || IsWindow(second_canvas) == FALSE) {
        return 784;
    }

    const std::size_t before_move_views = state.Document().ViewCount();
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_MOVE_OTHER_GROUP,
            0) != 1
        || state.Document().ViewCount() != before_move_views
        || editors.Active() == nullptr
        || editors.Active()->id != first_group_id
        || editors.Find(second_group_id) == nullptr
        || editors.Find(second_group_id)->ViewCount() != 0U
        || renderer::GetCanvasSnapshotSink(second_canvas)->AcceptsSnapshots()) {
        return 771;
    }
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_NEW_VIEW_OTHER_GROUP,
            0) != 1
        || state.Document().ViewCount() != before_move_views + 1U
        || editors.Active() == nullptr
        || editors.Active()->id != second_group_id
        || editors.Active()->ViewCount() != 1U
        || !renderer::GetCanvasSnapshotSink(second_canvas)->AcceptsSnapshots()) {
        return 772;
    }
    const std::size_t before_close_views = state.Document().ViewCount();
    if (SendMessageW(
            state.Workspace().windows.window,
            WM_COMMAND,
            IDM_EDITOR_GROUP_NEXT,
            0) != 1
        || editors.Active() == nullptr
        || editors.Active()->id != first_group_id
        || editors.Active()->focus_history != first_tabs
        || SendMessageW(
               state.Workspace().windows.window,
               WM_COMMAND,
               IDM_EDITOR_GROUP_CLOSE,
               0) != 1
        || editors.GroupCount() != 1U
        || editors.Orientation() != EditorSplitOrientation::None
        || state.routing.targets.EditorGroupCount() != 1U
        || state.renderer->SurfaceCount() != 1U
        || state.Document().ViewCount() != before_close_views
        || editors.Active() == nullptr
        || editors.Active()->id != second_group_id
        || state.Workspace().windows.canvas != editors.Active()->canvas
        || state.routing.targets.Canvas() != editors.Active()->canvas_id
        || editors.FindByCanvas(first_canvas_id) != nullptr) {
        return 773;
    }
    return 0;
}

int RunCommandContextSmoke(ApplicationHost& state) noexcept {
    using inkpod::app::CommandContext;
    using inkpod::app::CommandResolveStatus;
    using inkpod::app::Generation;

    const CommandContext context = state.routing.targets.Capture();
    if (state.routing.targets.Resolve(
            context, inkpod::app::kDocumentViewCommandScope)
        != CommandResolveStatus::Ok) {
        return 727;
    }
    CommandContext missing = context;
    missing.document_view.reset();
    if (state.routing.targets.Resolve(
            missing, inkpod::app::kDocumentViewCommandScope)
        != CommandResolveStatus::MissingScope) {
        return 728;
    }
    CommandContext stale = context;
    stale.generation = Generation(
        state.routing.targets.CurrentGeneration().Value() + 1U);
    if (state.routing.targets.Resolve(
            stale, inkpod::app::kDocumentViewCommandScope)
        != CommandResolveStatus::StaleGeneration) {
        return 729;
    }
    InkpodDocumentInfo before = EmptyDocumentInfo();
    InkpodDocumentInfo after = EmptyDocumentInfo();
    if (!QueryDocument(state, before)
        || SendMessageW(state.Workspace().windows.window, WM_COMMAND, UINT16_MAX, 0) != 0
        || !QueryDocument(state, after)
        || before.document_revision != after.document_revision
        || before.main_plane_checksum != after.main_plane_checksum
        || IssueCommand(
               nullptr,
               state.Workspace().windows.window,
               IDM_EDIT_UNDO,
               0,
               std::nullopt)
                .value_or(1)
            != 0) {
        return 730;
    }
    return 0;
}


}  // namespace inkpod::windows::ui::runtime

namespace inkpod::windows::ui {

int RunApplicationSmoke(app::ApplicationHost& state) noexcept {
    const auto context = state.routing.targets.Capture();
    int exit_code = state.Workspace().application != &state
            || state.Workspace().id != state.routing.targets.Workspace()
            || state.Workspace().windows.window == nullptr
            || state.Document().id != state.routing.targets.DocumentSession()
            || state.Document().Core() != state.engine.get()
            || state.Document().ActiveView() == nullptr
            || !context.document_view.has_value()
            || state.Document().ActiveView()->id != context.document_view.value()
        ? 731
        : 0;
    if (exit_code == 0) {
        exit_code = runtime::RunCommandContextSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunDrawingPersistenceSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunPaintingRecoverySmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunDocumentEditingSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunProductionWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunVectorWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunImageEffectsSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunBatchWorkflowSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunMagnifiedRasterHitSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunMultiDocumentTabSmoke(state);
    }
    if (exit_code == 0) {
        exit_code = runtime::RunSplitEditorGroupSmoke(state);
    }
    if (exit_code != 0) {
        std::fprintf(stderr, "inkpod application smoke failed: %d\n", exit_code);
    }
    return exit_code;
}

}  // namespace inkpod::windows::ui
