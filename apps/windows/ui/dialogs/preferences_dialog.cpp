#include "preferences_dialog.h"

#include <windows.h>
#include <windowsx.h>
#ifdef SetWindowPos
#undef SetWindowPos
#endif
#include <commctrl.h>
#include <commdlg.h>

#include <algorithm>
#include <array>
#include <cstdint>
#include <cwchar>
#include <cwctype>
#include <new>
#include <optional>
#include <span>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "app/resource.h"
#include "ui/dialogs/modal_dialog_position.h"
#include "ui/command_catalog.h"
#include "ui/shortcut_preset.h"
#include "ui/ui_resources.h"

namespace inkpod::windows::ui {
namespace {

constexpr int kGeneralPage = 0;
constexpr int kSavePage = 1;
constexpr int kWorkspacePage = 2;
constexpr int kAnimationPage = 3;
constexpr int kColorPage = 4;
constexpr int kShortcutPage = 5;
constexpr int kAnonymousControlFirst = 30'000;

struct PageControl final {
    HWND window{};
    int page{};
};

struct KeyboardKey final {
    HWND window{};
    std::uint32_t logical{};
    std::uint32_t physical{};
    int row{};
    int column{};
    int width_units{1};
};

struct DialogModel final {
    PreferencesDialogState* state{};
    PreferencesValues initial;
    PreferencesValues working;
    HWND dialog{};
    HFONT font{};
    std::vector<PageControl> page_controls;
    std::vector<KeyboardKey> keyboard;
    std::vector<UINT> visible_commands;
    std::vector<ShortcutConflict> conflicts;
    std::vector<std::wstring> command_names;
    std::optional<ShortcutInputStroke> key_search;
    std::optional<ShortcutProfileBinding> previous_assignment;
    UINT selected_command{};
    ShortcutSlot selected_slot{ShortcutSlot::Primary};
    int selected_page{kShortcutPage};
    int category_filter{-1};
    std::uint32_t modifier_filter{};
    int recording_control{};
    bool recording_started{};
    bool dragging{};
    UINT dragging_command{};
    int next_anonymous{kAnonymousControlFirst};
};

DialogModel* Model(HWND dialog) noexcept {
    return reinterpret_cast<DialogModel*>(GetWindowLongPtrW(dialog, GWLP_USERDATA));
}

int Scale(HWND window, int value) noexcept {
    return MulDiv(value, static_cast<int>(GetDpiForWindow(window)), 96);
}

HWND AddControl(
    DialogModel& model,
    int page,
    DWORD ex_style,
    const wchar_t* class_name,
    const wchar_t* text,
    DWORD style,
    int id) {
    if (id == 0) {
        id = model.next_anonymous++;
    }
    HWND control = CreateWindowExW(
        ex_style,
        class_name,
        text,
        WS_CHILD | style,
        0,
        0,
        10,
        10,
        model.dialog,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)),
        reinterpret_cast<HINSTANCE>(GetWindowLongPtrW(model.dialog, GWLP_HINSTANCE)),
        nullptr);
    if (control == nullptr) {
        throw std::bad_alloc();
    }
    SendMessageW(control, WM_SETFONT, reinterpret_cast<WPARAM>(model.font), TRUE);
    model.page_controls.push_back({control, page});
    return control;
}

HWND AddLabel(DialogModel& model, int page, UiStringId text) {
    return AddControl(
        model,
        page,
        0U,
        WC_STATICW,
        UiText(text),
        SS_LEFT | SS_NOPREFIX,
        0);
}

HWND AddButton(
    DialogModel& model,
    int page,
    UiStringId text,
    int id,
    DWORD extra = BS_PUSHBUTTON | WS_TABSTOP) {
    return AddControl(
        model, page, 0U, WC_BUTTONW, UiText(text), extra, id);
}

HWND AddCombo(DialogModel& model, int page, int id) {
    return AddControl(
        model,
        page,
        0U,
        WC_COMBOBOXW,
        L"",
        CBS_DROPDOWNLIST | WS_VSCROLL | WS_TABSTOP,
        id);
}

void AddComboText(HWND combo, UiStringId text) noexcept {
    SendMessageW(combo, CB_ADDSTRING, 0U, reinterpret_cast<LPARAM>(UiText(text)));
}

ShortcutProfile* ActiveProfile(DialogModel& model) noexcept {
    return model.working.shortcuts.active_profile
            < model.working.shortcuts.profiles.size()
        ? &model.working.shortcuts.profiles[model.working.shortcuts.active_profile]
        : nullptr;
}

const ShortcutProfile* ActiveProfile(const DialogModel& model) noexcept {
    return model.working.shortcuts.active_profile
            < model.working.shortcuts.profiles.size()
        ? &model.working.shortcuts.profiles[model.working.shortcuts.active_profile]
        : nullptr;
}

ShortcutProfile& EnsureEditableProfile(DialogModel& model) {
    ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr) {
        throw std::bad_alloc();
    }
    if (!profile->built_in) {
        return *profile;
    }
    if (model.working.shortcuts.profiles.size() >= kMaximumShortcutProfiles) {
        throw std::bad_alloc();
    }
    ShortcutProfile copy = *profile;
    copy.built_in = false;
    copy.name = UiText(UiStringId::ShortcutPresetLabel);
    copy.name += L' ';
    copy.name += std::to_wstring(model.working.shortcuts.profiles.size());
    model.working.shortcuts.profiles.push_back(std::move(copy));
    model.working.shortcuts.active_profile =
        model.working.shortcuts.profiles.size() - 1U;
    return model.working.shortcuts.profiles.back();
}

std::wstring BindingText(const ShortcutProfileBinding* binding) {
    if (binding == nullptr) {
        return UiText(UiStringId::ShortcutUnassigned);
    }
    InkpodShortcutSequence sequence{};
    sequence.struct_size = sizeof(sequence);
    sequence.command_id = binding->command_id;
    sequence.stroke_count = binding->stroke_count;
    for (std::uint32_t index = 0U; index < binding->stroke_count; ++index) {
        sequence.strokes[index].virtual_key = binding->strokes[index].logical_key;
        sequence.strokes[index].modifiers = binding->strokes[index].modifiers;
    }
    return FormatShortcutSequence(sequence);
}

const wchar_t* ContextText(ShortcutContext context) noexcept {
    switch (context) {
        case ShortcutContext::Canvas: return UiText(UiStringId::ShortcutCanvas);
        case ShortcutContext::Timeline: return UiText(UiStringId::ShortcutTimeline);
        case ShortcutContext::Pane: return UiText(UiStringId::ShortcutPane);
        default: return UiText(UiStringId::ShortcutGlobal);
    }
}

const wchar_t* ActionText(ShortcutAction action) noexcept {
    switch (action) {
        case ShortcutAction::Hold: return UiText(UiStringId::ShortcutHold);
        case ShortcutAction::Toggle: return UiText(UiStringId::ShortcutToggle);
        default: return UiText(UiStringId::ShortcutExecute);
    }
}

std::wstring CommandCategory(HWND menu_owner, UINT command) {
    std::wstring display = MenuCommandDisplayName(GetMenu(menu_owner), command);
    const std::size_t open = display.rfind(L'[');
    const std::size_t close = display.rfind(L']');
    if (open != std::wstring::npos && close == display.size() - 1U && open < close) {
        return display.substr(open + 1U, close - open - 1U);
    }
    return UiText(UiStringId::ShortcutAllCommands);
}

std::wstring CommandName(HWND menu_owner, UINT command) {
    std::wstring display = MenuCommandDisplayName(GetMenu(menu_owner), command);
    const std::size_t open = display.rfind(L" [");
    if (open != std::wstring::npos) {
        display.resize(open);
    }
    return display;
}

std::wstring Lower(std::wstring text) {
    std::transform(text.begin(), text.end(), text.begin(), [](wchar_t value) {
        return static_cast<wchar_t>(std::towlower(value));
    });
    return text;
}

bool CommandHasConflict(const DialogModel& model, UINT command) noexcept {
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr) {
        return false;
    }
    return std::any_of(model.conflicts.begin(), model.conflicts.end(), [&](const auto& item) {
        return (item.first_index < profile->bindings.size()
                   && profile->bindings[item.first_index].command_id == command)
            || (item.second_index < profile->bindings.size()
                && profile->bindings[item.second_index].command_id == command);
    });
}

bool CommandChanged(const DialogModel& model, UINT command) noexcept {
    const ShortcutProfile* current = ActiveProfile(model);
    if (current == nullptr || model.working.shortcuts.profiles.empty()) {
        return false;
    }
    const ShortcutProfile& defaults = model.working.shortcuts.profiles.front();
    for (const ShortcutSlot slot : {ShortcutSlot::Primary, ShortcutSlot::Secondary}) {
        const auto* left = FindShortcutBinding(
            std::span<const ShortcutProfileBinding>(current->bindings), command, slot);
        const auto* right = FindShortcutBinding(
            std::span<const ShortcutProfileBinding>(defaults.bindings), command, slot);
        if ((left == nullptr) != (right == nullptr)
            || (left != nullptr && *left != *right)) {
            return true;
        }
    }
    return false;
}

bool CommandUnassigned(const DialogModel& model, UINT command) noexcept {
    const ShortcutProfile* profile = ActiveProfile(model);
    return profile == nullptr
        || (FindShortcutBinding(
                std::span<const ShortcutProfileBinding>(profile->bindings),
                command,
                ShortcutSlot::Primary)
                == nullptr
            && FindShortcutBinding(
                   std::span<const ShortcutProfileBinding>(profile->bindings),
                   command,
                   ShortcutSlot::Secondary)
                == nullptr);
}

bool StrokeMatchesSearch(
    const ShortcutProfileBinding& binding,
    const ShortcutInputStroke& search) noexcept {
    for (std::uint32_t index = 0U; index < binding.stroke_count; ++index) {
        const auto& stroke = binding.strokes[index];
        if (stroke.modifiers == search.modifiers
            && (stroke.logical_key == search.logical_key
                || stroke.physical_key == search.physical_key)) {
            return true;
        }
    }
    return false;
}

bool CommandMatchesFilters(
    const DialogModel& model,
    UINT command,
    std::wstring_view search) {
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr) {
        return false;
    }
    if (model.category_filter == -2 && !CommandHasConflict(model, command)) {
        return false;
    }
    if (model.category_filter == -3 && !CommandChanged(model, command)) {
        return false;
    }
    if (model.category_filter == -4 && !CommandUnassigned(model, command)) {
        return false;
    }
    if (model.category_filter >= 0
        && static_cast<int>(command / 100U) != model.category_filter) {
        return false;
    }
    if (Button_GetCheck(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICTS_ONLY))
            == BST_CHECKED
        && !CommandHasConflict(model, command)) {
        return false;
    }
    const int context_selection = static_cast<int>(SendDlgItemMessageW(
        model.dialog, IDC_SHORTCUT_CONTEXT_FILTER, CB_GETCURSEL, 0U, 0U));
    const ShortcutContext context = context_selection <= 0
        ? ShortcutContext::Global
        : static_cast<ShortcutContext>(context_selection);
    bool has_context = context_selection <= 0;
    bool key_match = !model.key_search.has_value();
    std::wstring searchable = Lower(CommandName(GetParent(model.dialog), command));
    for (const auto& binding : profile->bindings) {
        if (binding.command_id != command) {
            continue;
        }
        has_context = has_context || binding.context == context;
        key_match = key_match
            || (model.key_search.has_value()
                && StrokeMatchesSearch(binding, *model.key_search));
        searchable += L' ';
        searchable += Lower(BindingText(&binding));
    }
    return has_context && key_match
        && (search.empty() || searchable.find(search) != std::wstring::npos);
}

void RefreshConflictState(DialogModel& model) {
    model.conflicts.clear();
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile != nullptr) {
        model.conflicts = AnalyzeShortcutConflicts(profile->bindings);
    }
    const std::size_t exact = static_cast<std::size_t>(std::count_if(
        model.conflicts.begin(), model.conflicts.end(), [](const auto& conflict) {
            return conflict.kind == ShortcutConflictKind::Exact;
        }));
    const std::size_t prefix = model.conflicts.size() - exact;
    std::array<wchar_t, 160U> text{};
    _snwprintf_s(
        text.data(),
        text.size(),
        _TRUNCATE,
        UiText(UiStringId::ShortcutConflictSummaryFormat),
        exact,
        prefix);
    SetDlgItemTextW(model.dialog, IDC_SHORTCUT_CONFLICT_BANNER, text.data());
    const BOOL can_apply = model.conflicts.empty() ? TRUE : FALSE;
    EnableWindow(GetDlgItem(model.dialog, IDOK), can_apply);
    EnableWindow(GetDlgItem(model.dialog, IDC_PREFERENCES_APPLY), can_apply);
}

void RefreshPresetCombo(DialogModel& model) noexcept {
    HWND combo = GetDlgItem(model.dialog, IDC_SHORTCUT_PRESET);
    SendMessageW(combo, CB_RESETCONTENT, 0U, 0U);
    for (const auto& profile : model.working.shortcuts.profiles) {
        SendMessageW(
            combo, CB_ADDSTRING, 0U, reinterpret_cast<LPARAM>(profile.name.c_str()));
    }
    SendMessageW(
        combo,
        CB_SETCURSEL,
        static_cast<WPARAM>(model.working.shortcuts.active_profile),
        0U);
}

void RefreshCategories(DialogModel& model) {
    HWND list = GetDlgItem(model.dialog, IDC_SHORTCUT_CATEGORIES);
    SendMessageW(list, LB_RESETCONTENT, 0U, 0U);
    const auto add = [list](const std::wstring& text, int data) {
        const LRESULT index = SendMessageW(
            list, LB_ADDSTRING, 0U, reinterpret_cast<LPARAM>(text.c_str()));
        if (index >= 0) {
            SendMessageW(list, LB_SETITEMDATA, static_cast<WPARAM>(index), data);
        }
    };
    add(UiText(UiStringId::ShortcutAllCommands), -1);
    add(UiText(UiStringId::ShortcutConflicts), -2);
    add(UiText(UiStringId::ShortcutChanged), -3);
    add(UiText(UiStringId::ShortcutUnassigned), -4);
    int previous_group{-1};
    for (const UINT command : ShortcutCommandCatalog()) {
        const int group = static_cast<int>(command / 100U);
        if (group == previous_group) {
            continue;
        }
        previous_group = group;
        add(CommandCategory(GetParent(model.dialog), command), group);
    }
    int select{};
    for (int index = 0; index < ListBox_GetCount(list); ++index) {
        if (static_cast<int>(SendMessageW(list, LB_GETITEMDATA, index, 0U))
            == model.category_filter) {
            select = index;
            break;
        }
    }
    ListBox_SetCurSel(list, select);
}

void RefreshCommandList(DialogModel& model) {
    RefreshConflictState(model);
    HWND list = GetDlgItem(model.dialog, IDC_SHORTCUT_COMMANDS);
    ListView_DeleteAllItems(list);
    SendMessageW(list, LVM_REMOVEALLGROUPS, 0U, 0U);
    model.visible_commands.clear();
    std::array<wchar_t, 256U> search_buffer{};
    GetDlgItemTextW(
        model.dialog,
        IDC_SHORTCUT_SEARCH,
        search_buffer.data(),
        static_cast<int>(search_buffer.size()));
    const std::wstring search = Lower(search_buffer.data());
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr) {
        return;
    }
    int previous_group{-1};
    for (const UINT command : ShortcutCommandCatalog()) {
        if (!CommandMatchesFilters(model, command, search)) {
            continue;
        }
        const int group = static_cast<int>(command / 100U);
        if (group != previous_group) {
            previous_group = group;
            std::wstring header = CommandCategory(GetParent(model.dialog), command);
            LVGROUP group_info{};
            group_info.cbSize = sizeof(group_info);
            group_info.mask = LVGF_GROUPID | LVGF_HEADER;
            group_info.iGroupId = group;
            group_info.pszHeader = header.data();
            SendMessageW(
                list, LVM_INSERTGROUP, static_cast<WPARAM>(-1),
                reinterpret_cast<LPARAM>(&group_info));
        }
        model.visible_commands.push_back(command);
        std::wstring name = CommandName(GetParent(model.dialog), command);
        const auto* primary = FindShortcutBinding(
            std::span<const ShortcutProfileBinding>(profile->bindings),
            command,
            ShortcutSlot::Primary);
        const auto* secondary = FindShortcutBinding(
            std::span<const ShortcutProfileBinding>(profile->bindings),
            command,
            ShortcutSlot::Secondary);
        LVITEMW item{};
        item.mask = LVIF_TEXT | LVIF_PARAM | LVIF_GROUPID;
        item.iItem = ListView_GetItemCount(list);
        item.iGroupId = group;
        item.lParam = command;
        item.pszText = name.data();
        const int row = ListView_InsertItem(list, &item);
        const std::wstring primary_text = BindingText(primary);
        const std::wstring secondary_text = BindingText(secondary);
        ListView_SetItemText(list, row, 1, const_cast<wchar_t*>(primary_text.c_str()));
        ListView_SetItemText(list, row, 2, const_cast<wchar_t*>(secondary_text.c_str()));
        const ShortcutProfileBinding* metadata = primary != nullptr ? primary : secondary;
        const wchar_t* action = metadata == nullptr
            ? L"—"
            : ActionText(metadata->action);
        const wchar_t* context = metadata == nullptr
            ? L"—"
            : ContextText(metadata->context);
        ListView_SetItemText(list, row, 3, const_cast<wchar_t*>(action));
        ListView_SetItemText(list, row, 4, const_cast<wchar_t*>(context));
        if (command == model.selected_command) {
            ListView_SetItemState(
                list, row, LVIS_SELECTED | LVIS_FOCUSED, LVIS_SELECTED | LVIS_FOCUSED);
        }
    }
    InvalidateRect(model.dialog, nullptr, FALSE);
}

void SelectComboEnum(HWND combo, std::uint32_t value, std::uint32_t first) noexcept {
    SendMessageW(combo, CB_SETCURSEL, value - first, 0U);
}

void RefreshDetail(DialogModel& model) noexcept {
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr || model.selected_command == 0U) {
        return;
    }
    SetDlgItemTextW(
        model.dialog,
        IDC_SHORTCUT_DETAIL_TITLE,
        CommandName(GetParent(model.dialog), model.selected_command).c_str());
    const std::string stable = CommandStableKey(model.selected_command);
    const std::wstring stable_wide(stable.begin(), stable.end());
    SetDlgItemTextW(model.dialog, IDC_SHORTCUT_DETAIL_KEY, stable_wide.c_str());
    const auto* primary = FindShortcutBinding(
        std::span<const ShortcutProfileBinding>(profile->bindings),
        model.selected_command,
        ShortcutSlot::Primary);
    const auto* secondary = FindShortcutBinding(
        std::span<const ShortcutProfileBinding>(profile->bindings),
        model.selected_command,
        ShortcutSlot::Secondary);
    SetDlgItemTextW(
        model.dialog, IDC_SHORTCUT_PRIMARY_VALUE, BindingText(primary).c_str());
    SetDlgItemTextW(
        model.dialog, IDC_SHORTCUT_SECONDARY_VALUE, BindingText(secondary).c_str());
    const ShortcutProfileBinding* metadata = primary != nullptr ? primary : secondary;
    if (metadata == nullptr) {
        SelectComboEnum(
            GetDlgItem(model.dialog, IDC_SHORTCUT_DETAIL_CONTEXT),
            static_cast<std::uint32_t>(DefaultShortcutContext(model.selected_command)),
            1U);
        SelectComboEnum(
            GetDlgItem(model.dialog, IDC_SHORTCUT_KEY_MATCH),
            static_cast<std::uint32_t>(ShortcutKeyMatch::Logical),
            1U);
    } else {
        SelectComboEnum(
            GetDlgItem(model.dialog, IDC_SHORTCUT_DETAIL_CONTEXT),
            static_cast<std::uint32_t>(metadata->context),
            1U);
        SelectComboEnum(
            GetDlgItem(model.dialog, IDC_SHORTCUT_KEY_MATCH),
            static_cast<std::uint32_t>(metadata->key_match),
            1U);
    }
    const ShortcutAction action = metadata == nullptr
        ? DefaultShortcutAction(model.selected_command)
        : metadata->action;
    Button_SetCheck(
        GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_EXECUTE),
        action == ShortcutAction::Execute ? BST_CHECKED : BST_UNCHECKED);
    Button_SetCheck(
        GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_HOLD),
        action == ShortcutAction::Hold ? BST_CHECKED : BST_UNCHECKED);
    Button_SetCheck(
        GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_TOGGLE),
        action == ShortcutAction::Toggle ? BST_CHECKED : BST_UNCHECKED);
    const std::uint32_t mask = SupportedShortcutActionMask(model.selected_command);
    EnableWindow(
        GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_HOLD),
        (mask & (1U << (static_cast<std::uint32_t>(ShortcutAction::Hold) - 1U))) != 0U);
    EnableWindow(
        GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_TOGGLE),
        (mask & (1U << (static_cast<std::uint32_t>(ShortcutAction::Toggle) - 1U))) != 0U);
    const bool conflict = CommandHasConflict(model, model.selected_command);
    ShowWindow(
        GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_CARD), conflict ? SW_SHOW : SW_HIDE);
    for (const int id : {
             IDC_SHORTCUT_CONFLICT_GOTO,
             IDC_SHORTCUT_CONFLICT_CLEAR,
             IDC_SHORTCUT_CONFLICT_SWAP}) {
        ShowWindow(GetDlgItem(model.dialog, id), conflict ? SW_SHOW : SW_HIDE);
    }
}

void RefreshStatus(DialogModel& model) noexcept {
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr) {
        return;
    }
    std::size_t assigned{};
    std::size_t changed{};
    for (const UINT command : ShortcutCommandCatalog()) {
        if (!CommandUnassigned(model, command)) {
            ++assigned;
        }
        if (CommandChanged(model, command)) {
            ++changed;
        }
    }
    std::array<wchar_t, 192U> text{};
    _snwprintf_s(
        text.data(),
        text.size(),
        _TRUNCATE,
        UiText(UiStringId::ShortcutStatusFormat),
        ShortcutCommandCatalog().size(),
        assigned,
        model.conflicts.size(),
        changed);
    SetDlgItemTextW(model.dialog, IDC_SHORTCUT_STATUS, text.data());
}

void RefreshShortcutUi(DialogModel& model) {
    RefreshPresetCombo(model);
    RefreshCategories(model);
    RefreshCommandList(model);
    RefreshDetail(model);
    RefreshStatus(model);
}

RECT PageRect(HWND dialog) noexcept {
    RECT bounds{};
    GetClientRect(GetDlgItem(dialog, IDC_PREFERENCES_TABS), &bounds);
    TabCtrl_AdjustRect(GetDlgItem(dialog, IDC_PREFERENCES_TABS), FALSE, &bounds);
    POINT origin{bounds.left, bounds.top};
    ClientToScreen(GetDlgItem(dialog, IDC_PREFERENCES_TABS), &origin);
    ScreenToClient(dialog, &origin);
    bounds.right += origin.x - bounds.left;
    bounds.bottom += origin.y - bounds.top;
    bounds.left = origin.x;
    bounds.top = origin.y;
    return bounds;
}

void Place(HWND window, int x, int y, int width, int height) noexcept {
    MoveWindow(window, x, y, std::max(width, 1), std::max(height, 1), TRUE);
}

void LayoutSimplePage(DialogModel& model, const RECT& page) noexcept {
    const int x = page.left + Scale(model.dialog, 28);
    const int y = page.top + Scale(model.dialog, 28);
    const int label_width = Scale(model.dialog, 210);
    const int control_x = x + label_width;
    const int control_width = Scale(model.dialog, 300);
    if (model.selected_page == kGeneralPage) {
        Place(model.page_controls[0].window, x, y + 4, label_width, 24);
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_LANGUAGE), control_x, y, control_width, 180);
        Place(model.page_controls[2].window, x, y + 48, page.right - x - 30, 38);
    } else if (model.selected_page == kSavePage) {
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_RESTORE_DOCUMENTS), x, y, 520, 28);
    } else if (model.selected_page == kWorkspacePage) {
        Place(model.page_controls[4].window, x, y + 4, label_width, 24);
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_WORKSPACE_PRESET), control_x, y, control_width, 200);
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_WORKSPACE_MIRROR), x, y + 50, 520, 26);
        Place(model.page_controls[7].window, x, y + 100, label_width, 24);
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_WORKSPACE_DENSITY), control_x, y + 96, control_width, 100);
    } else if (model.selected_page == kAnimationPage) {
        Place(model.page_controls[9].window, x, y + 4, label_width, 24);
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_SEQUENCE_SWITCH), control_x, y, control_width, 120);
        Place(model.page_controls[11].window, x, y + 54, label_width, 24);
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_SEQUENCE_ENDPOINT), control_x, y + 50, control_width, 120);
    } else if (model.selected_page == kColorPage) {
        Place(model.page_controls[13].window, x, y + 4, label_width, 24);
        Place(GetDlgItem(model.dialog, IDC_PREFERENCES_COLOR_PROFILE), control_x, y, control_width, 100);
    }
}

void LayoutKeyboard(DialogModel& model, int left, int top, int width, int height) noexcept {
    const int unit = std::max(Scale(model.dialog, 34), width / 25);
    const int key_height = std::max(Scale(model.dialog, 25), height / 6);
    for (const auto& key : model.keyboard) {
        const int indent = key.row == 1 ? unit / 2 : (key.row == 2 ? unit : 0);
        Place(
            key.window,
            left + indent + key.column * (unit + 3),
            top + key.row * (key_height + 3),
            key.width_units * unit + (key.width_units - 1) * 3,
            key_height);
    }
}

void LayoutShortcutPage(DialogModel& model, const RECT& page) noexcept {
    const int gap = Scale(model.dialog, 8);
    const int row = Scale(model.dialog, 30);
    const int x = page.left + gap;
    const int width = page.right - page.left - 2 * gap;
    int y = page.top + gap;
    const int label_width = Scale(model.dialog, 72);
    Place(model.page_controls[15].window, x, y + 5, label_width, 22);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_PRESET), x + label_width, y, Scale(model.dialog, 260), 200);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_DUPLICATE), x + label_width + Scale(model.dialog, 268), y, Scale(model.dialog, 78), row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_IMPORT), page.right - Scale(model.dialog, 292), y, Scale(model.dialog, 86), row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_EXPORT), page.right - Scale(model.dialog, 198), y, Scale(model.dialog, 86), row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_RESET_PROFILE), page.right - Scale(model.dialog, 104), y, Scale(model.dialog, 96), row);
    y += row + gap;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_SEARCH), x, y, width / 2, row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_KEY_SEARCH), x + width / 2 + gap, y, Scale(model.dialog, 155), row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONTEXT_FILTER), x + width / 2 + Scale(model.dialog, 171), y, width / 2 - Scale(model.dialog, 171), 160);
    y += row + gap;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_BANNER), x, y, width - Scale(model.dialog, 300), row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_PREVIOUS), page.right - Scale(model.dialog, 292), y, Scale(model.dialog, 82), row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_NEXT), page.right - Scale(model.dialog, 202), y, Scale(model.dialog, 82), row);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICTS_ONLY), page.right - Scale(model.dialog, 112), y, Scale(model.dialog, 104), row);
    y += row + gap;

    const int keyboard_height = Scale(model.dialog, 170);
    const int status_height = Scale(model.dialog, 24);
    const int main_bottom = page.bottom - keyboard_height - status_height - 2 * gap;
    const int left_width = Scale(model.dialog, 175);
    const int right_width = Scale(model.dialog, 300);
    const int center_x = x + left_width + gap;
    const int right_x = page.right - right_width - gap;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CATEGORIES), x, y, left_width, main_bottom - y);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_COMMANDS), center_x, y, right_x - center_x - gap, main_bottom - y);

    int detail_y = y;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_DETAIL_TITLE), right_x, detail_y, right_width, 25);
    detail_y += 25;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_DETAIL_KEY), right_x, detail_y, right_width, 20);
    detail_y += 28;
    Place(model.page_controls[32].window, right_x, detail_y + 4, 70, 20);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_PRIMARY_VALUE), right_x + 72, detail_y, right_width - 72, 26);
    detail_y += 30;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_PRIMARY_RECORD), right_x, detail_y, 122, 27);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_PRIMARY_CLEAR), right_x + 130, detail_y, 78, 27);
    detail_y += 34;
    Place(model.page_controls[36].window, right_x, detail_y + 4, 70, 20);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_SECONDARY_VALUE), right_x + 72, detail_y, right_width - 72, 26);
    detail_y += 30;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_SECONDARY_RECORD), right_x, detail_y, 122, 27);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_SECONDARY_CLEAR), right_x + 130, detail_y, 78, 27);
    detail_y += 36;
    Place(model.page_controls[40].window, right_x, detail_y + 3, 58, 20);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_EXECUTE), right_x + 60, detail_y, 72, 24);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_HOLD), right_x + 134, detail_y, 86, 24);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_TOGGLE), right_x + 222, detail_y, 76, 24);
    detail_y += 30;
    Place(model.page_controls[44].window, right_x, detail_y + 3, 68, 20);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_DETAIL_CONTEXT), right_x + 70, detail_y, 104, 120);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_KEY_MATCH), right_x + 180, detail_y, 118, 100);
    detail_y += 34;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_CARD), right_x, detail_y, right_width, 40);
    detail_y += 44;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_GOTO), right_x, detail_y, 88, 26);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_CLEAR), right_x + 94, detail_y, 98, 26);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_CONFLICT_SWAP), right_x + 198, detail_y, 88, 26);
    detail_y += 34;
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_RESET_COMMAND), right_x, detail_y, right_width, 27);

    const int keyboard_top = main_bottom + gap;
    Place(model.page_controls[52].window, x, keyboard_top + 4, 48, 22);
    int mod_x = x + 50;
    for (const int id : {
             IDC_SHORTCUT_MOD_NONE,
             IDC_SHORTCUT_MOD_CTRL,
             IDC_SHORTCUT_MOD_SHIFT,
             IDC_SHORTCUT_MOD_ALT,
             IDC_SHORTCUT_MOD_WIN}) {
        Place(GetDlgItem(model.dialog, id), mod_x, keyboard_top, 62, 27);
        mod_x += 66;
    }
    Place(model.page_controls[58].window, page.right - 276, keyboard_top + 4, 112, 22);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_KEYBOARD_LAYOUT), page.right - 158, keyboard_top, 150, 120);
    LayoutKeyboard(model, x + 8, keyboard_top + 34, width / 2, keyboard_height - 40);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_KEYBOARD_INSTRUCTION), x + width / 2 + 20, keyboard_top + 42, width / 2 - 28, 54);
    Place(GetDlgItem(model.dialog, IDC_SHORTCUT_STATUS), x, page.bottom - status_height, width, status_height);
}

void LayoutDialog(DialogModel& model) noexcept {
    RECT client{};
    GetClientRect(model.dialog, &client);
    const int margin = Scale(model.dialog, 8);
    const int button_width = Scale(model.dialog, 92);
    const int button_height = Scale(model.dialog, 30);
    const int button_y = client.bottom - margin - button_height;
    Place(GetDlgItem(model.dialog, IDOK), client.right - margin - 3 * button_width - 2 * margin, button_y, button_width, button_height);
    Place(GetDlgItem(model.dialog, IDCANCEL), client.right - margin - 2 * button_width - margin, button_y, button_width, button_height);
    Place(GetDlgItem(model.dialog, IDC_PREFERENCES_APPLY), client.right - margin - button_width, button_y, button_width, button_height);
    Place(GetDlgItem(model.dialog, IDC_PREFERENCES_TABS), margin, margin, client.right - 2 * margin, button_y - 2 * margin);
    const RECT page = PageRect(model.dialog);
    if (model.selected_page == kShortcutPage) {
        LayoutShortcutPage(model, page);
    } else {
        LayoutSimplePage(model, page);
    }
}

void ShowSelectedPage(DialogModel& model) noexcept {
    for (const auto& control : model.page_controls) {
        ShowWindow(control.window, control.page == model.selected_page ? SW_SHOW : SW_HIDE);
    }
    LayoutDialog(model);
}

void CreateSimplePages(DialogModel& model) {
    AddLabel(model, kGeneralPage, UiStringId::PreferencesLanguage);
    HWND language = AddCombo(model, kGeneralPage, IDC_PREFERENCES_LANGUAGE);
    AddComboText(language, UiStringId::PreferencesLanguageSystem);
    AddComboText(language, UiStringId::PreferencesLanguageJapanese);
    AddComboText(language, UiStringId::PreferencesLanguageEnglish);
    AddLabel(model, kGeneralPage, UiStringId::PreferencesLanguageRestart);

    AddButton(
        model,
        kSavePage,
        UiStringId::PreferencesRestoreDocuments,
        IDC_PREFERENCES_RESTORE_DOCUMENTS,
        BS_AUTOCHECKBOX | WS_TABSTOP);

    AddLabel(model, kWorkspacePage, UiStringId::PreferencesWorkspacePreset);
    HWND preset = AddCombo(model, kWorkspacePage, IDC_PREFERENCES_WORKSPACE_PRESET);
    for (std::uint32_t index = 0U;
         index < static_cast<std::uint32_t>(WorkspacePreset::Count);
         ++index) {
        SendMessageW(
            preset,
            CB_ADDSTRING,
            0U,
            reinterpret_cast<LPARAM>(WorkspacePresetDisplayName(
                static_cast<WorkspacePreset>(index))));
    }
    AddButton(
        model,
        kWorkspacePage,
        UiStringId::PreferencesWorkspaceMirror,
        IDC_PREFERENCES_WORKSPACE_MIRROR,
        BS_AUTOCHECKBOX | WS_TABSTOP);
    AddLabel(model, kWorkspacePage, UiStringId::PreferencesWorkspaceDensity);
    HWND density = AddCombo(model, kWorkspacePage, IDC_PREFERENCES_WORKSPACE_DENSITY);
    AddComboText(density, UiStringId::PreferencesWorkspaceStandard);
    AddComboText(density, UiStringId::PreferencesWorkspaceCompact);

    AddLabel(model, kAnimationPage, UiStringId::PreferencesSequenceSwitch);
    HWND switch_policy = AddCombo(
        model, kAnimationPage, IDC_PREFERENCES_SEQUENCE_SWITCH);
    AddComboText(switch_policy, UiStringId::PreferencesSequencePrompt);
    AddComboText(switch_policy, UiStringId::PreferencesSequenceAutosave);
    AddLabel(model, kAnimationPage, UiStringId::PreferencesEndpoint);
    HWND endpoint = AddCombo(
        model, kAnimationPage, IDC_PREFERENCES_SEQUENCE_ENDPOINT);
    AddComboText(endpoint, UiStringId::PreferencesEndpointStop);
    AddComboText(endpoint, UiStringId::PreferencesEndpointWrap);

    AddLabel(model, kColorPage, UiStringId::PreferencesOutputGuard);
    HWND color = AddCombo(model, kColorPage, IDC_PREFERENCES_COLOR_PROFILE);
    AddComboText(color, UiStringId::PreferencesOutputGuardBt709);
}

void AddListColumn(HWND list, int index, UiStringId text, int width) noexcept {
    LVCOLUMNW column{};
    column.mask = LVCF_TEXT | LVCF_WIDTH | LVCF_SUBITEM;
    column.iSubItem = index;
    column.cx = width;
    column.pszText = const_cast<wchar_t*>(UiText(text));
    ListView_InsertColumn(list, index, &column);
}

void BuildKeyboard(DialogModel& model) {
    for (const auto& key : model.keyboard) {
        DestroyWindow(key.window);
        const auto found = std::find_if(
            model.page_controls.begin(), model.page_controls.end(), [&](const auto& item) {
                return item.window == key.window;
            });
        if (found != model.page_controls.end()) {
            model.page_controls.erase(found);
        }
    }
    model.keyboard.clear();
    struct Definition final {
        std::uint32_t key;
        const wchar_t* label;
        int row;
        int column;
        int width;
    };
    static constexpr Definition keys[] = {
        {'1', L"1", 0, 0, 1}, {'2', L"2", 0, 1, 1}, {'3', L"3", 0, 2, 1},
        {'4', L"4", 0, 3, 1}, {'5', L"5", 0, 4, 1}, {'6', L"6", 0, 5, 1},
        {'7', L"7", 0, 6, 1}, {'8', L"8", 0, 7, 1}, {'9', L"9", 0, 8, 1},
        {'0', L"0", 0, 9, 1}, {'Q', L"Q", 1, 0, 1}, {'W', L"W", 1, 1, 1},
        {'E', L"E", 1, 2, 1}, {'R', L"R", 1, 3, 1}, {'T', L"T", 1, 4, 1},
        {'Y', L"Y", 1, 5, 1}, {'U', L"U", 1, 6, 1}, {'I', L"I", 1, 7, 1},
        {'O', L"O", 1, 8, 1}, {'P', L"P", 1, 9, 1}, {'A', L"A", 2, 0, 1},
        {'S', L"S", 2, 1, 1}, {'D', L"D", 2, 2, 1}, {'F', L"F", 2, 3, 1},
        {'G', L"G", 2, 4, 1}, {'H', L"H", 2, 5, 1}, {'J', L"J", 2, 6, 1},
        {'K', L"K", 2, 7, 1}, {'L', L"L", 2, 8, 1}, {'Z', L"Z", 3, 0, 1},
        {'X', L"X", 3, 1, 1}, {'C', L"C", 3, 2, 1}, {'V', L"V", 3, 3, 1},
        {'B', L"B", 3, 4, 1}, {'N', L"N", 3, 5, 1}, {'M', L"M", 3, 6, 1},
        {VK_SPACE, L"Space", 3, 8, 3},
    };
    int id = IDC_SHORTCUT_KEY_FIRST;
    for (const auto& definition : keys) {
        const UINT scan = MapVirtualKeyW(definition.key, MAPVK_VK_TO_VSC_EX);
        HWND window = AddControl(
            model,
            kShortcutPage,
            0U,
            WC_BUTTONW,
            definition.label,
            BS_OWNERDRAW | WS_TABSTOP,
            id++);
        model.keyboard.push_back({
            window,
            definition.key,
            scan == 0U ? definition.key : scan,
            definition.row,
            definition.column,
            definition.width});
    }
    if (model.working.shortcuts.keyboard_layout == ShortcutKeyboardLayout::Jis109
        && id <= IDC_SHORTCUT_KEY_LAST) {
        const UINT scan = MapVirtualKeyW(VK_OEM_102, MAPVK_VK_TO_VSC_EX);
        HWND window = AddControl(
            model,
            kShortcutPage,
            0U,
            WC_BUTTONW,
            L"\\_",
            BS_OWNERDRAW | WS_TABSTOP,
            id);
        model.keyboard.push_back({window, VK_OEM_102, scan, 3, 7, 1});
    }
}

void CreateShortcutPage(DialogModel& model) {
    AddLabel(model, kShortcutPage, UiStringId::ShortcutPresetLabel);
    AddCombo(model, kShortcutPage, IDC_SHORTCUT_PRESET);
    AddButton(model, kShortcutPage, UiStringId::ShortcutDuplicate, IDC_SHORTCUT_DUPLICATE);
    AddButton(model, kShortcutPage, UiStringId::ShortcutImport, IDC_SHORTCUT_IMPORT);
    AddButton(model, kShortcutPage, UiStringId::ShortcutExport, IDC_SHORTCUT_EXPORT);
    AddButton(model, kShortcutPage, UiStringId::ShortcutReset, IDC_SHORTCUT_RESET_PROFILE);
    HWND search = AddControl(
        model,
        kShortcutPage,
        WS_EX_CLIENTEDGE,
        WC_EDITW,
        L"",
        ES_AUTOHSCROLL | WS_TABSTOP,
        IDC_SHORTCUT_SEARCH);
    SendMessageW(
        search,
        EM_SETCUEBANNER,
        TRUE,
        reinterpret_cast<LPARAM>(UiText(UiStringId::ShortcutSearchPlaceholder)));
    AddButton(model, kShortcutPage, UiStringId::ShortcutSearchByKey, IDC_SHORTCUT_KEY_SEARCH);
    HWND filter = AddCombo(model, kShortcutPage, IDC_SHORTCUT_CONTEXT_FILTER);
    AddComboText(filter, UiStringId::ShortcutContextAll);
    AddComboText(filter, UiStringId::ShortcutGlobal);
    AddComboText(filter, UiStringId::ShortcutCanvas);
    AddComboText(filter, UiStringId::ShortcutTimeline);
    AddComboText(filter, UiStringId::ShortcutPane);
    SendMessageW(filter, CB_SETCURSEL, 0U, 0U);
    AddControl(
        model,
        kShortcutPage,
        0U,
        WC_STATICW,
        L"",
        SS_LEFT | SS_CENTERIMAGE | SS_NOPREFIX,
        IDC_SHORTCUT_CONFLICT_BANNER);
    AddButton(model, kShortcutPage, UiStringId::ShortcutPrevious, IDC_SHORTCUT_CONFLICT_PREVIOUS);
    AddButton(model, kShortcutPage, UiStringId::ShortcutNext, IDC_SHORTCUT_CONFLICT_NEXT);
    AddButton(
        model,
        kShortcutPage,
        UiStringId::ShortcutConflictsOnly,
        IDC_SHORTCUT_CONFLICTS_ONLY,
        BS_AUTOCHECKBOX | WS_TABSTOP);
    AddControl(
        model,
        kShortcutPage,
        WS_EX_CLIENTEDGE,
        WC_LISTBOXW,
        L"",
        LBS_NOTIFY | WS_VSCROLL | WS_TABSTOP,
        IDC_SHORTCUT_CATEGORIES);
    HWND commands = AddControl(
        model,
        kShortcutPage,
        WS_EX_CLIENTEDGE,
        WC_LISTVIEWW,
        L"",
        LVS_REPORT | LVS_SHOWSELALWAYS | WS_TABSTOP,
        IDC_SHORTCUT_COMMANDS);
    ListView_SetExtendedListViewStyle(
        commands, LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP);
    SendMessageW(commands, LVM_ENABLEGROUPVIEW, TRUE, 0U);
    AddListColumn(commands, 0, UiStringId::ShortcutCommandColumn, 220);
    AddListColumn(commands, 1, UiStringId::ShortcutPrimary, 112);
    AddListColumn(commands, 2, UiStringId::ShortcutSecondary, 112);
    AddListColumn(commands, 3, UiStringId::ShortcutAction, 90);
    AddListColumn(commands, 4, UiStringId::ShortcutContext, 96);

    AddControl(model, kShortcutPage, 0U, WC_STATICW, L"", SS_LEFT | SS_NOPREFIX, IDC_SHORTCUT_DETAIL_TITLE);
    AddControl(model, kShortcutPage, 0U, WC_STATICW, L"", SS_LEFT | SS_NOPREFIX, IDC_SHORTCUT_DETAIL_KEY);
    AddLabel(model, kShortcutPage, UiStringId::ShortcutPrimary);
    AddControl(model, kShortcutPage, WS_EX_CLIENTEDGE, WC_STATICW, L"", SS_LEFT | SS_CENTERIMAGE | SS_NOPREFIX, IDC_SHORTCUT_PRIMARY_VALUE);
    AddButton(model, kShortcutPage, UiStringId::ShortcutRecordKey, IDC_SHORTCUT_PRIMARY_RECORD);
    AddButton(model, kShortcutPage, UiStringId::Clear, IDC_SHORTCUT_PRIMARY_CLEAR);
    AddLabel(model, kShortcutPage, UiStringId::ShortcutSecondary);
    AddControl(model, kShortcutPage, WS_EX_CLIENTEDGE, WC_STATICW, L"", SS_LEFT | SS_CENTERIMAGE | SS_NOPREFIX, IDC_SHORTCUT_SECONDARY_VALUE);
    AddButton(model, kShortcutPage, UiStringId::ShortcutAdd, IDC_SHORTCUT_SECONDARY_RECORD);
    AddButton(model, kShortcutPage, UiStringId::Clear, IDC_SHORTCUT_SECONDARY_CLEAR);
    AddLabel(model, kShortcutPage, UiStringId::ShortcutAction);
    AddButton(model, kShortcutPage, UiStringId::ShortcutExecute, IDC_SHORTCUT_ACTION_EXECUTE, BS_AUTORADIOBUTTON | WS_GROUP | WS_TABSTOP);
    AddButton(model, kShortcutPage, UiStringId::ShortcutHold, IDC_SHORTCUT_ACTION_HOLD, BS_AUTORADIOBUTTON | WS_TABSTOP);
    AddButton(model, kShortcutPage, UiStringId::ShortcutToggle, IDC_SHORTCUT_ACTION_TOGGLE, BS_AUTORADIOBUTTON | WS_TABSTOP);
    AddLabel(model, kShortcutPage, UiStringId::ShortcutContext);
    HWND context = AddCombo(model, kShortcutPage, IDC_SHORTCUT_DETAIL_CONTEXT);
    AddComboText(context, UiStringId::ShortcutGlobal);
    AddComboText(context, UiStringId::ShortcutCanvas);
    AddComboText(context, UiStringId::ShortcutTimeline);
    AddComboText(context, UiStringId::ShortcutPane);
    HWND matching = AddCombo(model, kShortcutPage, IDC_SHORTCUT_KEY_MATCH);
    AddComboText(matching, UiStringId::ShortcutLogical);
    AddComboText(matching, UiStringId::ShortcutPhysical);
    AddControl(model, kShortcutPage, WS_EX_CLIENTEDGE, WC_STATICW, UiText(UiStringId::ShortcutExactConflict), SS_LEFT | SS_CENTERIMAGE | SS_NOPREFIX, IDC_SHORTCUT_CONFLICT_CARD);
    AddButton(model, kShortcutPage, UiStringId::ShortcutGoToOther, IDC_SHORTCUT_CONFLICT_GOTO);
    AddButton(model, kShortcutPage, UiStringId::ShortcutClearOther, IDC_SHORTCUT_CONFLICT_CLEAR);
    AddButton(model, kShortcutPage, UiStringId::ShortcutSwap, IDC_SHORTCUT_CONFLICT_SWAP);
    AddButton(model, kShortcutPage, UiStringId::ShortcutResetCommand, IDC_SHORTCUT_RESET_COMMAND);
    AddLabel(model, kShortcutPage, UiStringId::ShortcutModifiers);
    AddButton(model, kShortcutPage, UiStringId::ShortcutNone, IDC_SHORTCUT_MOD_NONE, BS_AUTORADIOBUTTON | WS_GROUP | WS_TABSTOP);
    AddButton(model, kShortcutPage, UiStringId::Text0653, IDC_SHORTCUT_MOD_CTRL, BS_AUTORADIOBUTTON | WS_TABSTOP);
    AddButton(model, kShortcutPage, UiStringId::Text0589, IDC_SHORTCUT_MOD_SHIFT, BS_AUTORADIOBUTTON | WS_TABSTOP);
    AddButton(model, kShortcutPage, UiStringId::Text0006, IDC_SHORTCUT_MOD_ALT, BS_AUTORADIOBUTTON | WS_TABSTOP);
    AddControl(model, kShortcutPage, 0U, WC_BUTTONW, L"Win", BS_AUTORADIOBUTTON | WS_TABSTOP, IDC_SHORTCUT_MOD_WIN);
    AddLabel(model, kShortcutPage, UiStringId::ShortcutKeyboardLayout);
    HWND layout = AddCombo(model, kShortcutPage, IDC_SHORTCUT_KEYBOARD_LAYOUT);
    AddComboText(layout, UiStringId::ShortcutKeyboardAuto);
    AddComboText(layout, UiStringId::ShortcutKeyboardJis);
    AddComboText(layout, UiStringId::ShortcutKeyboardUs);
    AddControl(model, kShortcutPage, 0U, WC_STATICW, UiText(UiStringId::ShortcutKeyboardInstruction), SS_LEFT | SS_NOPREFIX, IDC_SHORTCUT_KEYBOARD_INSTRUCTION);
    AddControl(model, kShortcutPage, 0U, WC_STATICW, L"", SS_LEFT | SS_NOPREFIX, IDC_SHORTCUT_STATUS);
    Button_SetCheck(GetDlgItem(model.dialog, IDC_SHORTCUT_MOD_NONE), BST_CHECKED);
    BuildKeyboard(model);
}

void LoadControls(DialogModel& model) noexcept {
    SendDlgItemMessageW(
        model.dialog,
        IDC_PREFERENCES_LANGUAGE,
        CB_SETCURSEL,
        static_cast<WPARAM>(model.working.language) - 1U,
        0U);
    Button_SetCheck(
        GetDlgItem(model.dialog, IDC_PREFERENCES_RESTORE_DOCUMENTS),
        model.working.restore_previous_documents ? BST_CHECKED : BST_UNCHECKED);
    SendDlgItemMessageW(
        model.dialog,
        IDC_PREFERENCES_WORKSPACE_PRESET,
        CB_SETCURSEL,
        static_cast<WPARAM>(model.working.workspace_preset),
        0U);
    Button_SetCheck(
        GetDlgItem(model.dialog, IDC_PREFERENCES_WORKSPACE_MIRROR),
        model.working.workspace_mirrored ? BST_CHECKED : BST_UNCHECKED);
    SendDlgItemMessageW(
        model.dialog,
        IDC_PREFERENCES_WORKSPACE_DENSITY,
        CB_SETCURSEL,
        static_cast<WPARAM>(model.working.workspace_density),
        0U);
    SendDlgItemMessageW(
        model.dialog,
        IDC_PREFERENCES_SEQUENCE_SWITCH,
        CB_SETCURSEL,
        static_cast<WPARAM>(model.working.sequence_switch_policy) - 1U,
        0U);
    SendDlgItemMessageW(
        model.dialog,
        IDC_PREFERENCES_SEQUENCE_ENDPOINT,
        CB_SETCURSEL,
        static_cast<WPARAM>(model.working.sequence_endpoint_policy) - 1U,
        0U);
    SendDlgItemMessageW(model.dialog, IDC_PREFERENCES_COLOR_PROFILE, CB_SETCURSEL, 0U, 0U);
    SendDlgItemMessageW(
        model.dialog,
        IDC_SHORTCUT_KEYBOARD_LAYOUT,
        CB_SETCURSEL,
        static_cast<WPARAM>(model.working.shortcuts.keyboard_layout) - 1U,
        0U);
}

void ReadControls(DialogModel& model) noexcept {
    const LRESULT language = SendDlgItemMessageW(
        model.dialog, IDC_PREFERENCES_LANGUAGE, CB_GETCURSEL, 0U, 0U);
    if (language >= 0) {
        model.working.language = static_cast<UiLanguagePreference>(language + 1);
    }
    model.working.restore_previous_documents = Button_GetCheck(
        GetDlgItem(model.dialog, IDC_PREFERENCES_RESTORE_DOCUMENTS)) == BST_CHECKED;
    const LRESULT preset = SendDlgItemMessageW(
        model.dialog, IDC_PREFERENCES_WORKSPACE_PRESET, CB_GETCURSEL, 0U, 0U);
    if (preset >= 0) {
        model.working.workspace_preset = static_cast<WorkspacePreset>(preset);
    }
    model.working.workspace_mirrored = Button_GetCheck(
        GetDlgItem(model.dialog, IDC_PREFERENCES_WORKSPACE_MIRROR)) == BST_CHECKED;
    const LRESULT density = SendDlgItemMessageW(
        model.dialog, IDC_PREFERENCES_WORKSPACE_DENSITY, CB_GETCURSEL, 0U, 0U);
    if (density >= 0) {
        model.working.workspace_density = static_cast<WorkspaceDensity>(density);
    }
    const LRESULT switch_policy = SendDlgItemMessageW(
        model.dialog, IDC_PREFERENCES_SEQUENCE_SWITCH, CB_GETCURSEL, 0U, 0U);
    if (switch_policy >= 0) {
        model.working.sequence_switch_policy =
            static_cast<app::SequenceCellSwitchPolicy>(switch_policy + 1);
    }
    const LRESULT endpoint = SendDlgItemMessageW(
        model.dialog, IDC_PREFERENCES_SEQUENCE_ENDPOINT, CB_GETCURSEL, 0U, 0U);
    if (endpoint >= 0) {
        model.working.sequence_endpoint_policy =
            static_cast<app::SequenceEndpointPolicy>(endpoint + 1);
    }
}

bool ApplyValues(DialogModel& model, bool closing) noexcept {
    ReadControls(model);
    if (!model.conflicts.empty()) {
        return false;
    }
    if (model.working == model.initial) {
        if (closing) {
            model.state->values = model.working;
        }
        return true;
    }
    if (model.state->apply != nullptr
        && !model.state->apply(model.state->apply_context, model.working, model.dialog)) {
        if (!model.state->close_immediately) {
            MessageBoxW(
                model.dialog,
                UiText(UiStringId::PreferencesApplyFailed),
                UiText(UiStringId::PreferencesTitle),
                MB_OK | MB_ICONERROR);
        }
        return false;
    }
    model.initial = model.working;
    model.state->values = model.working;
    return true;
}

ShortcutProfileBinding* EditableBinding(
    DialogModel& model,
    ShortcutSlot slot,
    bool create) {
    ShortcutProfile& profile = EnsureEditableProfile(model);
    auto* binding = FindShortcutBinding(
        std::span<ShortcutProfileBinding>(profile.bindings),
        model.selected_command,
        slot);
    if (binding == nullptr && create) {
        ShortcutProfileBinding value{};
        value.command_id = model.selected_command;
        value.slot = slot;
        value.context = DefaultShortcutContext(model.selected_command);
        value.action = DefaultShortcutAction(model.selected_command);
        profile.bindings.push_back(value);
        binding = &profile.bindings.back();
    }
    return binding;
}

void RemoveBinding(DialogModel& model, ShortcutSlot slot, UINT command) {
    ShortcutProfile& profile = EnsureEditableProfile(model);
    std::erase_if(profile.bindings, [=](const auto& binding) {
        return binding.command_id == command && binding.slot == slot;
    });
}

bool AssignStroke(
    DialogModel& model,
    ShortcutSlot slot,
    ShortcutInputStroke stroke,
    bool append) {
    ShortcutProfile& profile = EnsureEditableProfile(model);
    ShortcutProfileBinding* binding = EditableBinding(model, slot, true);
    if (binding == nullptr) {
        return false;
    }
    const ShortcutProfileBinding before = *binding;
    if (!append || binding->stroke_count >= INKPOD_SHORTCUT_MAX_STROKES) {
        binding->stroke_count = 0U;
    }
    binding->strokes[binding->stroke_count++] = stroke;
    const LRESULT context = SendDlgItemMessageW(
        model.dialog, IDC_SHORTCUT_DETAIL_CONTEXT, CB_GETCURSEL, 0U, 0U);
    const LRESULT match = SendDlgItemMessageW(
        model.dialog, IDC_SHORTCUT_KEY_MATCH, CB_GETCURSEL, 0U, 0U);
    binding->context = context >= 0
        ? static_cast<ShortcutContext>(context + 1)
        : DefaultShortcutContext(model.selected_command);
    binding->key_match = match == 1
        ? ShortcutKeyMatch::Physical
        : ShortcutKeyMatch::Logical;
    const auto conflicts = AnalyzeShortcutConflicts(profile.bindings);
    if (std::any_of(conflicts.begin(), conflicts.end(), [](const auto& item) {
            return item.kind == ShortcutConflictKind::Prefix;
        })) {
        *binding = before;
        MessageBoxW(
            model.dialog,
            UiText(UiStringId::ShortcutPrefixConflict),
            UiText(UiStringId::PreferencesTitle),
            MB_OK | MB_ICONWARNING);
        return false;
    }
    model.previous_assignment = before.stroke_count == 0U
        ? std::optional<ShortcutProfileBinding>{}
        : std::optional<ShortcutProfileBinding>{before};
    return true;
}

std::uint32_t CurrentModifiers() noexcept {
    std::uint32_t modifiers{};
    if ((GetKeyState(VK_CONTROL) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_CONTROL;
    }
    if ((GetKeyState(VK_SHIFT) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_SHIFT;
    }
    if ((GetKeyState(VK_MENU) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_ALT;
    }
    if ((GetKeyState(VK_LWIN) & 0x8000) != 0
        || (GetKeyState(VK_RWIN) & 0x8000) != 0) {
        modifiers |= kShortcutModifierWindows;
    }
    return modifiers;
}

bool IsModifierKey(WPARAM key) noexcept {
    return key == VK_CONTROL || key == VK_LCONTROL || key == VK_RCONTROL
        || key == VK_SHIFT || key == VK_LSHIFT || key == VK_RSHIFT
        || key == VK_MENU || key == VK_LMENU || key == VK_RMENU
        || key == VK_LWIN || key == VK_RWIN;
}

void HandleRecordedKey(DialogModel& model, WPARAM key, LPARAM data) {
    if (IsModifierKey(key)) {
        return;
    }
    const ShortcutInputStroke stroke{
        static_cast<std::uint32_t>(key),
        ShortcutPhysicalKeyFromMessage(key, data),
        CurrentModifiers()};
    if (model.recording_control == IDC_SHORTCUT_KEY_SEARCH) {
        model.key_search = stroke;
        model.recording_control = 0;
    } else {
        const ShortcutSlot slot = model.recording_control == IDC_SHORTCUT_SECONDARY_RECORD
            ? ShortcutSlot::Secondary
            : ShortcutSlot::Primary;
        if (AssignStroke(model, slot, stroke, model.recording_started)) {
            model.recording_started = true;
            const auto* binding = FindShortcutBinding(
                std::span<const ShortcutProfileBinding>(
                    EnsureEditableProfile(model).bindings),
                model.selected_command,
                slot);
            if (binding != nullptr
                && binding->stroke_count >= INKPOD_SHORTCUT_MAX_STROKES) {
                model.recording_control = 0;
                model.recording_started = false;
            }
        }
    }
    RefreshShortcutUi(model);
}

void SelectConflict(DialogModel& model, int direction) noexcept {
    if (model.conflicts.empty()) {
        return;
    }
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr) {
        return;
    }
    std::size_t current{};
    for (std::size_t index = 0U; index < model.conflicts.size(); ++index) {
        const auto& item = model.conflicts[index];
        if ((item.first_index < profile->bindings.size()
                && profile->bindings[item.first_index].command_id == model.selected_command)
            || (item.second_index < profile->bindings.size()
                && profile->bindings[item.second_index].command_id == model.selected_command)) {
            current = index;
            break;
        }
    }
    current = direction < 0
        ? (current + model.conflicts.size() - 1U) % model.conflicts.size()
        : (current + 1U) % model.conflicts.size();
    const auto& target = model.conflicts[current];
    if (target.first_index < profile->bindings.size()) {
        model.selected_command = profile->bindings[target.first_index].command_id;
        RefreshCommandList(model);
        RefreshDetail(model);
    }
}

std::optional<std::size_t> OtherConflictIndex(DialogModel& model) noexcept {
    ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr) {
        return std::nullopt;
    }
    for (const auto& item : model.conflicts) {
        if (item.first_index >= profile->bindings.size()
            || item.second_index >= profile->bindings.size()) {
            continue;
        }
        if (profile->bindings[item.first_index].command_id == model.selected_command) {
            return item.second_index;
        }
        if (profile->bindings[item.second_index].command_id == model.selected_command) {
            return item.first_index;
        }
    }
    return std::nullopt;
}

void ImportPreset(DialogModel& model) {
    std::array<wchar_t, MAX_PATH> path{};
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = model.dialog;
    dialog.lpstrFilter = UiText(UiStringId::ShortcutPresetFileFilter);
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    dialog.lpstrDefExt = L"inkshortcuts";
    if (GetOpenFileNameW(&dialog) == FALSE) {
        return;
    }
    ShortcutProfile profile{};
    if (ReadShortcutPreset(path.data(), profile) != ShortcutPresetStatus::Ok
        || model.working.shortcuts.profiles.size() >= kMaximumShortcutProfiles) {
        MessageBoxW(
            model.dialog,
            UiText(UiStringId::ShortcutImportFailed),
            UiText(UiStringId::PreferencesTitle),
            MB_OK | MB_ICONERROR);
        return;
    }
    model.working.shortcuts.profiles.push_back(std::move(profile));
    model.working.shortcuts.active_profile = model.working.shortcuts.profiles.size() - 1U;
    RefreshShortcutUi(model);
}

void ExportPreset(DialogModel& model) noexcept {
    const ShortcutProfile* profile = ActiveProfile(model);
    if (profile == nullptr || !model.conflicts.empty()) {
        return;
    }
    std::array<wchar_t, MAX_PATH> path{};
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = model.dialog;
    dialog.lpstrFilter = UiText(UiStringId::ShortcutPresetFileFilter);
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    dialog.lpstrDefExt = L"inkshortcuts";
    if (GetSaveFileNameW(&dialog) == FALSE) {
        return;
    }
    if (SaveShortcutPresetAtomic(path.data(), *profile) != ShortcutPresetStatus::Ok) {
        MessageBoxW(
            model.dialog,
            UiText(UiStringId::ShortcutExportFailed),
            UiText(UiStringId::PreferencesTitle),
            MB_OK | MB_ICONERROR);
    }
}

void UpdateBindingMetadata(DialogModel& model) {
    ShortcutProfile& profile = EnsureEditableProfile(model);
    ShortcutAction action = ShortcutAction::Execute;
    if (Button_GetCheck(GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_HOLD))
        == BST_CHECKED) {
        action = ShortcutAction::Hold;
    } else if (Button_GetCheck(GetDlgItem(model.dialog, IDC_SHORTCUT_ACTION_TOGGLE))
               == BST_CHECKED) {
        action = ShortcutAction::Toggle;
    }
    const auto context = static_cast<ShortcutContext>(
        SendDlgItemMessageW(
            model.dialog, IDC_SHORTCUT_DETAIL_CONTEXT, CB_GETCURSEL, 0U, 0U)
        + 1U);
    const ShortcutKeyMatch key_match = SendDlgItemMessageW(
        model.dialog, IDC_SHORTCUT_KEY_MATCH, CB_GETCURSEL, 0U, 0U) == 1
        ? ShortcutKeyMatch::Physical
        : ShortcutKeyMatch::Logical;
    for (auto& binding : profile.bindings) {
        if (binding.command_id == model.selected_command) {
            binding.action = action;
            binding.context = context;
            binding.key_match = key_match;
        }
    }
    RefreshShortcutUi(model);
}

void DrawKeyboardKey(DialogModel& model, const DRAWITEMSTRUCT& draw) noexcept {
    const auto found = std::find_if(
        model.keyboard.begin(), model.keyboard.end(), [&](const auto& key) {
            return key.window == draw.hwndItem;
        });
    if (found == model.keyboard.end()) {
        return;
    }
    COLORREF background = GetSysColor(COLOR_WINDOW);
    const ShortcutProfile* profile = ActiveProfile(model);
    bool assigned{};
    bool conflict{};
    if (profile != nullptr) {
        for (std::size_t index = 0U; index < profile->bindings.size(); ++index) {
            const auto& binding = profile->bindings[index];
            if (binding.stroke_count == 0U) {
                continue;
            }
            const auto& stroke = binding.strokes[0];
            const std::uint32_t key = binding.key_match == ShortcutKeyMatch::Physical
                ? stroke.physical_key
                : stroke.logical_key;
            const std::uint32_t shown = binding.key_match == ShortcutKeyMatch::Physical
                ? found->physical
                : found->logical;
            if (key == shown && stroke.modifiers == model.modifier_filter) {
                assigned = true;
                conflict = std::any_of(
                    model.conflicts.begin(), model.conflicts.end(), [index](const auto& item) {
                        return item.first_index == index || item.second_index == index;
                    });
            }
        }
    }
    if (conflict) {
        background = RGB(255, 218, 218);
    } else if (assigned) {
        background = RGB(220, 239, 255);
    }
    HBRUSH brush = CreateSolidBrush(background);
    FillRect(draw.hDC, &draw.rcItem, brush);
    DeleteObject(brush);
    FrameRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_3DSHADOW));
    std::array<wchar_t, 32U> text{};
    GetWindowTextW(draw.hwndItem, text.data(), static_cast<int>(text.size()));
    SetBkMode(draw.hDC, TRANSPARENT);
    DrawTextW(
        draw.hDC,
        text.data(),
        -1,
        const_cast<RECT*>(&draw.rcItem),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
}

void HandleKeyboardButton(DialogModel& model, HWND button) {
    const auto found = std::find_if(
        model.keyboard.begin(), model.keyboard.end(), [&](const auto& key) {
            return key.window == button;
        });
    if (found == model.keyboard.end()) {
        return;
    }
    const ShortcutInputStroke search{
        found->logical, found->physical, model.modifier_filter};
    if (model.dragging && model.dragging_command != 0U) {
        model.selected_command = model.dragging_command;
        (void)AssignStroke(model, ShortcutSlot::Primary, search, false);
        model.dragging = false;
        ReleaseCapture();
    } else {
        model.key_search = search;
    }
    RefreshShortcutUi(model);
}

void HandleCommand(DialogModel& model, int id, int notification, HWND source) {
    if (id >= IDC_SHORTCUT_KEY_FIRST && id <= IDC_SHORTCUT_KEY_LAST
        && notification == BN_CLICKED) {
        HandleKeyboardButton(model, source);
        return;
    }
    switch (id) {
        case IDC_SHORTCUT_PRESET:
            if (notification == CBN_SELCHANGE) {
                const LRESULT selected = SendMessageW(source, CB_GETCURSEL, 0U, 0U);
                if (selected >= 0
                    && static_cast<std::size_t>(selected)
                        < model.working.shortcuts.profiles.size()) {
                    model.working.shortcuts.active_profile =
                        static_cast<std::size_t>(selected);
                    RefreshShortcutUi(model);
                }
            }
            break;
        case IDC_SHORTCUT_DUPLICATE: {
            const ShortcutProfile* active = ActiveProfile(model);
            if (active == nullptr
                || model.working.shortcuts.profiles.size() >= kMaximumShortcutProfiles) {
                break;
            }
            ShortcutProfile copy = *active;
            copy.built_in = false;
            copy.name = UiText(UiStringId::ShortcutPresetLabel);
            copy.name += L' ';
            copy.name += std::to_wstring(model.working.shortcuts.profiles.size());
            model.working.shortcuts.profiles.push_back(std::move(copy));
            model.working.shortcuts.active_profile = model.working.shortcuts.profiles.size() - 1U;
            RefreshShortcutUi(model);
            break;
        }
        case IDC_SHORTCUT_IMPORT:
            ImportPreset(model);
            break;
        case IDC_SHORTCUT_EXPORT:
            ExportPreset(model);
            break;
        case IDC_SHORTCUT_RESET_PROFILE:
            model.working.shortcuts.active_profile = 0U;
            RefreshShortcutUi(model);
            break;
        case IDC_SHORTCUT_SEARCH:
            if (notification == EN_CHANGE) {
                model.key_search.reset();
                RefreshCommandList(model);
                RefreshStatus(model);
            }
            break;
        case IDC_SHORTCUT_KEY_SEARCH:
            model.recording_control = IDC_SHORTCUT_KEY_SEARCH;
            model.recording_started = false;
            SetFocus(model.dialog);
            break;
        case IDC_SHORTCUT_CONTEXT_FILTER:
            if (notification == CBN_SELCHANGE) {
                RefreshCommandList(model);
            }
            break;
        case IDC_SHORTCUT_CONFLICTS_ONLY:
            RefreshCommandList(model);
            break;
        case IDC_SHORTCUT_CATEGORIES:
            if (notification == LBN_SELCHANGE) {
                const int selected = ListBox_GetCurSel(source);
                if (selected >= 0) {
                    model.category_filter = static_cast<int>(SendMessageW(
                        source, LB_GETITEMDATA, selected, 0U));
                    RefreshCommandList(model);
                }
            }
            break;
        case IDC_SHORTCUT_CONFLICT_PREVIOUS:
            SelectConflict(model, -1);
            break;
        case IDC_SHORTCUT_CONFLICT_NEXT:
            SelectConflict(model, 1);
            break;
        case IDC_SHORTCUT_PRIMARY_RECORD:
        case IDC_SHORTCUT_SECONDARY_RECORD:
            model.recording_control = id;
            model.recording_started = false;
            model.selected_slot = id == IDC_SHORTCUT_SECONDARY_RECORD
                ? ShortcutSlot::Secondary
                : ShortcutSlot::Primary;
            SetFocus(model.dialog);
            break;
        case IDC_SHORTCUT_PRIMARY_CLEAR:
        case IDC_SHORTCUT_SECONDARY_CLEAR:
            RemoveBinding(
                model,
                id == IDC_SHORTCUT_SECONDARY_CLEAR
                    ? ShortcutSlot::Secondary
                    : ShortcutSlot::Primary,
                model.selected_command);
            RefreshShortcutUi(model);
            break;
        case IDC_SHORTCUT_ACTION_EXECUTE:
        case IDC_SHORTCUT_ACTION_HOLD:
        case IDC_SHORTCUT_ACTION_TOGGLE:
            if (notification == BN_CLICKED) {
                UpdateBindingMetadata(model);
            }
            break;
        case IDC_SHORTCUT_DETAIL_CONTEXT:
        case IDC_SHORTCUT_KEY_MATCH:
            if (notification == CBN_SELCHANGE) {
                UpdateBindingMetadata(model);
            }
            break;
        case IDC_SHORTCUT_CONFLICT_GOTO: {
            const auto other = OtherConflictIndex(model);
            ShortcutProfile* profile = ActiveProfile(model);
            if (other.has_value() && profile != nullptr
                && *other < profile->bindings.size()) {
                model.selected_command = profile->bindings[*other].command_id;
                RefreshShortcutUi(model);
            }
            break;
        }
        case IDC_SHORTCUT_CONFLICT_CLEAR: {
            const auto other = OtherConflictIndex(model);
            ShortcutProfile* profile = ActiveProfile(model);
            if (other.has_value() && profile != nullptr
                && *other < profile->bindings.size()) {
                const auto binding = profile->bindings[*other];
                RemoveBinding(model, binding.slot, binding.command_id);
                RefreshShortcutUi(model);
            }
            break;
        }
        case IDC_SHORTCUT_CONFLICT_SWAP: {
            const auto other = OtherConflictIndex(model);
            ShortcutProfile* profile = ActiveProfile(model);
            if (other.has_value() && profile != nullptr
                && *other < profile->bindings.size()
                && model.previous_assignment.has_value()) {
                ShortcutProfileBinding& target = profile->bindings[*other];
                const UINT command = target.command_id;
                const ShortcutSlot slot = target.slot;
                target = *model.previous_assignment;
                target.command_id = command;
                target.slot = slot;
                model.previous_assignment.reset();
                RefreshShortcutUi(model);
            }
            break;
        }
        case IDC_SHORTCUT_RESET_COMMAND: {
            if (model.working.shortcuts.profiles.empty()) {
                break;
            }
            ShortcutProfile& profile = EnsureEditableProfile(model);
            std::erase_if(profile.bindings, [&](const auto& binding) {
                return binding.command_id == model.selected_command;
            });
            const auto& defaults = model.working.shortcuts.profiles.front();
            for (const auto& binding : defaults.bindings) {
                if (binding.command_id == model.selected_command) {
                    profile.bindings.push_back(binding);
                }
            }
            RefreshShortcutUi(model);
            break;
        }
        case IDC_SHORTCUT_MOD_NONE:
        case IDC_SHORTCUT_MOD_CTRL:
        case IDC_SHORTCUT_MOD_SHIFT:
        case IDC_SHORTCUT_MOD_ALT:
        case IDC_SHORTCUT_MOD_WIN:
            model.modifier_filter = id == IDC_SHORTCUT_MOD_CTRL
                ? INKPOD_SHORTCUT_MODIFIER_CONTROL
                : (id == IDC_SHORTCUT_MOD_SHIFT
                       ? INKPOD_SHORTCUT_MODIFIER_SHIFT
                       : (id == IDC_SHORTCUT_MOD_ALT
                              ? INKPOD_SHORTCUT_MODIFIER_ALT
                              : (id == IDC_SHORTCUT_MOD_WIN
                                     ? kShortcutModifierWindows
                                     : 0U)));
            InvalidateRect(model.dialog, nullptr, FALSE);
            break;
        case IDC_SHORTCUT_KEYBOARD_LAYOUT:
            if (notification == CBN_SELCHANGE) {
                const LRESULT selection = SendMessageW(source, CB_GETCURSEL, 0U, 0U);
                if (selection >= 0) {
                    model.working.shortcuts.keyboard_layout =
                        static_cast<ShortcutKeyboardLayout>(selection + 1);
                    BuildKeyboard(model);
                    ShowSelectedPage(model);
                }
            }
            break;
        default:
            break;
    }
}

INT_PTR OnInit(HWND dialog, LPARAM parameter) {
    auto* state = reinterpret_cast<PreferencesDialogState*>(parameter);
    if (state == nullptr) {
        return FALSE;
    }
    try {
        auto* model = new DialogModel{};
        model->state = state;
        model->initial = state->values;
        model->working = state->values;
        model->dialog = dialog;
        model->font = reinterpret_cast<HFONT>(SendMessageW(dialog, WM_GETFONT, 0U, 0U));
        model->selected_command = ShortcutCommandCatalog().empty()
            ? 0U
            : ShortcutCommandCatalog().front();
        SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(model));
        SetWindowTextW(dialog, UiText(UiStringId::PreferencesTitle));
        SetDlgItemTextW(dialog, IDOK, UiText(UiStringId::Ok));
        SetDlgItemTextW(dialog, IDCANCEL, UiText(UiStringId::Text0171));
        SetDlgItemTextW(dialog, IDC_PREFERENCES_APPLY, UiText(UiStringId::Apply));
        HWND tabs = GetDlgItem(dialog, IDC_PREFERENCES_TABS);
        const std::array<UiStringId, 6U> tab_texts{
            UiStringId::PreferencesTabGeneral,
            UiStringId::PreferencesTabSaveRecovery,
            UiStringId::PreferencesTabWorkspace,
            UiStringId::PreferencesTabAnimation,
            UiStringId::PreferencesTabColor,
            UiStringId::PreferencesTabShortcuts};
        for (std::size_t index = 0U; index < tab_texts.size(); ++index) {
            TCITEMW item{};
            item.mask = TCIF_TEXT;
            item.pszText = const_cast<wchar_t*>(UiText(tab_texts[index]));
            TabCtrl_InsertItem(tabs, static_cast<int>(index), &item);
        }
        TabCtrl_SetCurSel(tabs, kShortcutPage);
        CreateSimplePages(*model);
        CreateShortcutPage(*model);
        LoadControls(*model);
        RefreshShortcutUi(*model);
        ShowSelectedPage(*model);
        RECT bounds{};
        GetWindowRect(dialog, &bounds);
        const int minimum_width = Scale(dialog, 1'180);
        const int minimum_height = Scale(dialog, 780);
        SetWindowPos(
            dialog,
            nullptr,
            bounds.left,
            bounds.top,
            std::max(static_cast<int>(bounds.right - bounds.left), minimum_width),
            std::max(static_cast<int>(bounds.bottom - bounds.top), minimum_height),
            SWP_NOZORDER | SWP_NOACTIVATE);
        static_cast<void>(CenterModalDialogOnOwner(dialog));
        if (state->close_immediately) {
            PostMessageW(dialog, WM_COMMAND, IDOK, 0U);
        }
        return TRUE;
    } catch (const std::bad_alloc&) {
        EndDialog(dialog, -1);
        return FALSE;
    }
}

INT_PTR CALLBACK PreferencesProcedure(
    HWND dialog,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    if (message == WM_INITDIALOG) {
        return OnInit(dialog, lparam);
    }
    DialogModel* model = Model(dialog);
    if (model == nullptr) {
        return FALSE;
    }
    try {
        switch (message) {
            case WM_SIZE:
                LayoutDialog(*model);
                return TRUE;
            case WM_GETMINMAXINFO: {
                auto* info = reinterpret_cast<MINMAXINFO*>(lparam);
                info->ptMinTrackSize.x = Scale(dialog, 1'000);
                info->ptMinTrackSize.y = Scale(dialog, 680);
                return TRUE;
            }
            case WM_KEYDOWN:
            case WM_SYSKEYDOWN:
                if (model->recording_control != 0) {
                    HandleRecordedKey(*model, wparam, lparam);
                    return TRUE;
                }
                break;
            case WM_DRAWITEM:
                if (wparam >= IDC_SHORTCUT_KEY_FIRST
                    && wparam <= IDC_SHORTCUT_KEY_LAST) {
                    DrawKeyboardKey(
                        *model, *reinterpret_cast<const DRAWITEMSTRUCT*>(lparam));
                    return TRUE;
                }
                break;
            case WM_NOTIFY: {
                const auto* notification = reinterpret_cast<const NMHDR*>(lparam);
                if (notification != nullptr
                    && notification->idFrom == IDC_PREFERENCES_TABS
                    && notification->code == TCN_SELCHANGE) {
                    model->selected_page = TabCtrl_GetCurSel(notification->hwndFrom);
                    ShowSelectedPage(*model);
                    return TRUE;
                }
                if (notification != nullptr
                    && notification->idFrom == IDC_SHORTCUT_COMMANDS
                    && notification->code == LVN_ITEMCHANGED) {
                    const auto* changed = reinterpret_cast<const NMLISTVIEW*>(lparam);
                    if ((changed->uNewState & LVIS_SELECTED) != 0U
                        && changed->iItem >= 0) {
                        LVITEMW item{};
                        item.mask = LVIF_PARAM;
                        item.iItem = changed->iItem;
                        if (ListView_GetItem(notification->hwndFrom, &item) != FALSE) {
                            model->selected_command = static_cast<UINT>(item.lParam);
                            RefreshDetail(*model);
                        }
                    }
                    return TRUE;
                }
                if (notification != nullptr
                    && notification->idFrom == IDC_SHORTCUT_COMMANDS
                    && notification->code == LVN_BEGINDRAG) {
                    model->dragging = true;
                    model->dragging_command = model->selected_command;
                    SetCapture(dialog);
                    return TRUE;
                }
                break;
            }
            case WM_LBUTTONUP:
                if (model->dragging) {
                    POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                    ClientToScreen(dialog, &point);
                    HWND target = WindowFromPoint(point);
                    const int id = target == nullptr ? 0 : GetDlgCtrlID(target);
                    if (id >= IDC_SHORTCUT_KEY_FIRST && id <= IDC_SHORTCUT_KEY_LAST) {
                        HandleKeyboardButton(*model, target);
                    }
                    model->dragging = false;
                    ReleaseCapture();
                    return TRUE;
                }
                break;
            case WM_COMMAND: {
                const int id = LOWORD(wparam);
                if (id == IDOK) {
                    if (ApplyValues(*model, true)) {
                        EndDialog(dialog, IDOK);
                    }
                    return TRUE;
                }
                if (id == IDCANCEL) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                if (id == IDC_PREFERENCES_APPLY) {
                    (void)ApplyValues(*model, false);
                    return TRUE;
                }
                HandleCommand(*model, id, HIWORD(wparam), reinterpret_cast<HWND>(lparam));
                return TRUE;
            }
            case WM_DESTROY:
                delete model;
                SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
                return TRUE;
            default:
                break;
        }
    } catch (const std::bad_alloc&) {
        if (!model->state->close_immediately) {
            MessageBoxW(
                dialog,
                UiText(UiStringId::PreferencesApplyFailed),
                UiText(UiStringId::PreferencesTitle),
                MB_OK | MB_ICONERROR);
        }
    }
    return FALSE;
}

}  // namespace

INT_PTR ShowPreferencesDialog(
    HINSTANCE instance,
    HWND owner,
    PreferencesDialogState& state) noexcept {
    return DialogBoxLocalizedParamW(
        instance,
        MAKEINTRESOURCEW(IDD_PREFERENCES),
        owner,
        PreferencesProcedure,
        reinterpret_cast<LPARAM>(&state));
}

}  // namespace inkpod::windows::ui
