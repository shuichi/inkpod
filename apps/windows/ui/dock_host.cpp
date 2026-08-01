#include "dock_host.h"

#include <commctrl.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstdint>

#include "app/resource.h"

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kFloatingPaneClass[] = L"InkpodFloatingDockPaneV1";
constexpr UINT_PTR kPaneSubclass = 1U;
constexpr UINT_PTR kSplitterSubclass = 1U;
constexpr UINT_PTR kTabSubclass = 1U;
constexpr int kTabHeightDip = 28;

constexpr UINT kContextDockTop = 1U;
constexpr UINT kContextDockLeft = 2U;
constexpr UINT kContextDockRight = 3U;
constexpr UINT kContextDockBottom = 4U;
constexpr UINT kContextFloat = 5U;
constexpr UINT kContextHide = 6U;
constexpr UINT kContextReset = 7U;
constexpr UINT kContextSplit = 8U;
constexpr UINT kContextTabs = 9U;

int ScaleDip(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

int PixelsToDip(int value, UINT dpi) noexcept {
    return MulDiv(value, 96, static_cast<int>(dpi == 0U ? 96U : dpi));
}

RECT ToRect(const DockRect& value) noexcept {
    return RECT{
        value.x,
        value.y,
        value.x + std::max(0, value.width),
        value.y + std::max(0, value.height)};
}

bool HasArea(const DockRect& value) noexcept {
    return value.width > 0 && value.height > 0;
}

void PlaceWindow(HWND window, const DockRect& bounds, bool visible) noexcept {
    if (window == nullptr) {
        return;
    }
    const bool show = visible && HasArea(bounds);
    SetWindowPos(
        window,
        nullptr,
        bounds.x,
        bounds.y,
        std::max(0, bounds.width),
        std::max(0, bounds.height),
        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER
            | (show ? SWP_SHOWWINDOW : SWP_HIDEWINDOW));
}

const wchar_t* ZoneLabel(DockZone zone) noexcept {
    switch (zone) {
        case DockZone::TopContext: return L"上へドック";
        case DockZone::Left: return L"左へドック";
        case DockZone::Right: return L"右へドック";
        case DockZone::Bottom: return L"下へドック";
        default: return L"ドック";
    }
}

const wchar_t* LoadPaneTitle(
    HINSTANCE instance,
    const PaneDescriptor& descriptor,
    wchar_t (&buffer)[128]) noexcept {
    if (instance != nullptr
        && LoadStringW(
               instance,
               static_cast<UINT>(descriptor.title_resource_id),
               buffer,
               static_cast<int>(std::size(buffer)))
            > 0) {
        return buffer;
    }
    return descriptor.fallback_title;
}

UINT ZoneCommand(DockZone zone) noexcept {
    switch (zone) {
        case DockZone::TopContext: return kContextDockTop;
        case DockZone::Left: return kContextDockLeft;
        case DockZone::Right: return kContextDockRight;
        case DockZone::Bottom: return kContextDockBottom;
        default: return 0U;
    }
}

DockZone CommandZone(UINT command) noexcept {
    switch (command) {
        case kContextDockTop: return DockZone::TopContext;
        case kContextDockLeft: return DockZone::Left;
        case kContextDockRight: return DockZone::Right;
        case kContextDockBottom: return DockZone::Bottom;
        default: return DockZone::Count;
    }
}

const wchar_t* SplitterName(const DockSplitterGeometry& splitter) noexcept {
    if (splitter.kind == DockSplitterKind::StackBoundary) {
        switch (splitter.zone) {
            case DockZone::TopContext: return L"上ドック内の分割位置";
            case DockZone::Left: return L"左ドック内の分割位置";
            case DockZone::Right: return L"右ドック内の分割位置";
            case DockZone::Bottom: return L"下ドック内の分割位置";
            default: return L"パネルの分割位置";
        }
    }
    switch (splitter.zone) {
        case DockZone::TopContext: return L"上ドックの高さ";
        case DockZone::Left: return L"左ドックの幅";
        case DockZone::Right: return L"右ドックの幅";
        case DockZone::Bottom: return L"下ドックの高さ";
        default: return L"ドックの寸法";
    }
}

}  // namespace

DockHost::DockHost() noexcept {
    for (std::size_t index = 0U; index < panes_.size(); ++index) {
        panes_[index].host = this;
        panes_[index].type = static_cast<DockPaneType>(index);
    }
    for (SplitterHostState& splitter : splitter_states_) {
        splitter.host = this;
    }
    for (std::size_t index = 0U; index < tab_states_.size(); ++index) {
        tab_states_[index].host = this;
        tab_states_[index].zone = static_cast<DockZone>(index);
    }
}

bool DockHost::Initialize(
    HWND owner, HINSTANCE instance, DockLayoutModel& model) noexcept {
    if (initialized_ || owner == nullptr || instance == nullptr) {
        return false;
    }
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.lpfnWndProc = FloatingWindowProcedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.hbrBackground = GetSysColorBrush(COLOR_BTNFACE);
    window_class.lpszClassName = kFloatingPaneClass;
    if (RegisterClassExW(&window_class) == 0
        && GetLastError() != ERROR_CLASS_ALREADY_EXISTS) {
        return false;
    }
    owner_ = owner;
    instance_ = instance;
    model_ = &model;
    preview_ = CreateWindowExW(
        WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
        L"STATIC",
        nullptr,
        WS_CHILD | SS_ETCHEDFRAME,
        0,
        0,
        0,
        0,
        owner_,
        nullptr,
        instance_,
        nullptr);
    if (preview_ == nullptr) {
        return false;
    }
    SetWindowTextW(preview_, L"ドック先のプレビュー");
    for (std::size_t index = 0U; index < splitters_.size(); ++index) {
        splitters_[index] = CreateWindowExW(
            0,
            L"STATIC",
            nullptr,
            WS_CHILD | WS_TABSTOP | SS_NOTIFY,
            0,
            0,
            0,
            0,
            owner_,
            nullptr,
            instance_,
            nullptr);
        if (splitters_[index] == nullptr
            || SetWindowSubclass(
                   splitters_[index],
                   SplitterSubclassProcedure,
                   kSplitterSubclass,
                   reinterpret_cast<DWORD_PTR>(&splitter_states_[index]))
                == FALSE) {
            return false;
        }
    }
    for (TabHostState& tabs : tab_states_) {
        tabs.control = CreateWindowExW(
            0,
            WC_TABCONTROLW,
            nullptr,
            WS_CHILD | WS_CLIPSIBLINGS | WS_TABSTOP,
            0,
            0,
            0,
            0,
            owner_,
            nullptr,
            instance_,
            nullptr);
        if (tabs.control == nullptr
            || SetWindowSubclass(
                   tabs.control,
                   TabSubclassProcedure,
                   kTabSubclass,
                   reinterpret_cast<DWORD_PTR>(&tabs))
                == FALSE) {
            return false;
        }
    }
    initialized_ = true;
    return true;
}

void DockHost::SetChangedCallback(
    DockHostChangedCallback callback, void* context) noexcept {
    changed_ = callback;
    changed_context_ = context;
}

bool DockHost::AttachPane(DockPaneType type, HWND content) noexcept {
    PaneHostState* pane = PaneState(type);
    if (!initialized_ || pane == nullptr || content == nullptr
        || pane->content != nullptr || GetParent(content) != owner_) {
        return false;
    }
    pane->content = content;
    if (SetWindowSubclass(
            content,
            PaneSubclassProcedure,
            kPaneSubclass,
            reinterpret_cast<DWORD_PTR>(pane))
        == FALSE) {
        pane->content = nullptr;
        return false;
    }
    return true;
}

void DockHost::ApplyLayout(
    const DockLayoutGeometry& geometry, UINT dpi) noexcept {
    if (!initialized_ || model_ == nullptr) {
        return;
    }
    applying_ = true;
    geometry_ = geometry;
    dpi_ = dpi == 0U ? 96U : dpi;
    for (std::size_t index = 0U; index < tab_states_.size(); ++index) {
        ApplyTabLayout(static_cast<DockZone>(index));
    }
    for (PaneHostState& pane : panes_) {
        ApplyPaneLayout(pane);
    }
    for (std::size_t index = 0U; index < splitters_.size(); ++index) {
        if (index < geometry_.splitter_count) {
            splitter_states_[index].geometry = geometry_.splitters[index];
            SetWindowTextW(
                splitters_[index], SplitterName(geometry_.splitters[index]));
            PlaceWindow(splitters_[index], geometry_.splitters[index].bounds, true);
        } else {
            PlaceWindow(splitters_[index], {}, false);
        }
    }
    if (preview_ != nullptr) {
        SetWindowPos(
            preview_,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    applying_ = false;
}

DockResult DockHost::TogglePane(DockPaneType type) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->IsPaneVisible(type)
        ? model_->HidePane(type)
        : model_->RestorePane(type);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
    return result;
}

DockResult DockHost::DockPane(DockPaneType type, DockZone zone) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->MovePane(type, zone);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
    return result;
}

DockResult DockHost::FloatPane(DockPaneType type) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    if (pane == nullptr) {
        return DockResult::InvalidPane;
    }
    const DockResult result = model_->FloatPane(type, pane->floating);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
    return result;
}

DockResult DockHost::HidePane(DockPaneType type) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->HidePane(type);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
    return result;
}

DockResult DockHost::RestorePane(DockPaneType type) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->RestorePane(type);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
    return result;
}

DockResult DockHost::ResetPane(DockPaneType type) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->ResetPane(type);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
    return result;
}

DockResult DockHost::SetZoneMode(
    DockZone zone, DockStackMode mode) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->SetZoneMode(zone, mode);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
    return result;
}

HWND DockHost::FloatingWindow(DockPaneType type) const noexcept {
    const PaneHostState* pane = PaneState(type);
    return pane == nullptr ? nullptr : pane->floating_window;
}

HWND DockHost::ContentWindow(DockPaneType type) const noexcept {
    const PaneHostState* pane = PaneState(type);
    return pane == nullptr ? nullptr : pane->content;
}

HWND DockHost::TabWindow(DockZone zone) const noexcept {
    const std::size_t index = static_cast<std::size_t>(zone);
    return index < tab_states_.size() ? tab_states_[index].control : nullptr;
}

HWND DockHost::SplitterWindow(
    DockZone zone, DockSplitterKind kind) const noexcept {
    for (std::size_t index = 0U; index < geometry_.splitter_count; ++index) {
        if (splitter_states_[index].geometry.zone == zone
            && splitter_states_[index].geometry.kind == kind) {
            return splitters_[index];
        }
    }
    return nullptr;
}

bool DockHost::PreviewVisible() const noexcept {
    return preview_ != nullptr && IsWindowVisible(preview_) != FALSE;
}

std::size_t DockHost::PaneIndex(DockPaneType type) noexcept {
    return static_cast<std::size_t>(type);
}

DockHost::PaneHostState* DockHost::PaneState(DockPaneType type) noexcept {
    const std::size_t index = PaneIndex(type);
    return index < panes_.size() ? &panes_[index] : nullptr;
}

const DockHost::PaneHostState* DockHost::PaneState(
    DockPaneType type) const noexcept {
    const std::size_t index = PaneIndex(type);
    return index < panes_.size() ? &panes_[index] : nullptr;
}

bool DockHost::EnsureFloatingWindow(PaneHostState& pane) noexcept {
    if (pane.floating_window != nullptr) {
        return true;
    }
    const PaneDescriptor* descriptor = FindPaneDescriptor(pane.type);
    if (descriptor == nullptr) {
        return false;
    }
    wchar_t title[128]{};
    pane.floating_window = CreateWindowExW(
        0,
        kFloatingPaneClass,
        LoadPaneTitle(instance_, *descriptor, title),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME
            | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        owner_,
        nullptr,
        instance_,
        &pane);
    return pane.floating_window != nullptr;
}

void DockHost::LayoutFloatingContent(PaneHostState& pane) noexcept {
    if (pane.floating_window == nullptr || pane.content == nullptr) {
        return;
    }
    RECT client{};
    if (GetClientRect(pane.floating_window, &client) != FALSE) {
        SetWindowPos(
            pane.content,
            nullptr,
            0,
            0,
            std::max(0L, client.right - client.left),
            std::max(0L, client.bottom - client.top),
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER | SWP_SHOWWINDOW);
    }
}

void DockHost::ApplyPaneLayout(PaneHostState& pane) noexcept {
    if (model_ == nullptr || pane.content == nullptr) {
        return;
    }
    const DockPanePlacement* placement = model_->Pane(pane.type);
    if (placement == nullptr || !placement->present) {
        ShowWindow(pane.content, SW_HIDE);
        return;
    }
    if (placement->zone == DockZone::Floating) {
        if (!EnsureFloatingWindow(pane)) {
            ShowWindow(pane.content, SW_HIDE);
            return;
        }
        if (GetParent(pane.content) != pane.floating_window) {
            SetParent(pane.content, pane.floating_window);
        }
        if (IsWindowVisible(pane.floating_window) == FALSE) {
            SetWindowPos(
                pane.floating_window,
                nullptr,
                ScaleDip(placement->floating.x_dip, dpi_),
                ScaleDip(placement->floating.y_dip, dpi_),
                ScaleDip(placement->floating.width_dip, dpi_),
                ScaleDip(placement->floating.height_dip, dpi_),
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
            ShowWindow(pane.floating_window, SW_SHOWNOACTIVATE);
        }
        LayoutFloatingContent(pane);
        return;
    }
    if (pane.floating_window != nullptr) {
        ShowWindow(pane.floating_window, SW_HIDE);
    }
    if (placement->zone == DockZone::Hidden) {
        ShowWindow(pane.content, SW_HIDE);
        return;
    }
    if (GetParent(pane.content) != owner_) {
        SetParent(pane.content, owner_);
    }
    DockPaneGeometry geometry = geometry_.panes[PaneIndex(pane.type)];
    const DockZoneState* zone = model_->Zone(placement->zone);
    if (zone != nullptr && zone->mode == DockStackMode::Tabs
        && model_->PaneCount(placement->zone) > 1U && geometry.shown) {
        const int tab_height = std::min(
            geometry.bounds.height, ScaleDip(kTabHeightDip, dpi_));
        geometry.bounds.y += tab_height;
        geometry.bounds.height -= tab_height;
    }
    PlaceWindow(
        pane.content,
        geometry.bounds,
        geometry.shown && !geometry.temporarily_auto_hidden);
}

void DockHost::ApplyTabLayout(DockZone zone) noexcept {
    if (model_ == nullptr || !IsDockedZone(zone)) {
        return;
    }
    TabHostState& tabs = tab_states_[static_cast<std::size_t>(zone)];
    const DockZoneState* state = model_->Zone(zone);
    const bool show = state != nullptr && state->mode == DockStackMode::Tabs
        && model_->PaneCount(zone) > 1U
        && HasArea(geometry_.zones[static_cast<std::size_t>(zone)]);
    if (!show) {
        PlaceWindow(tabs.control, {}, false);
        return;
    }
    struct OrderedPane {
        std::uint8_t order{};
        DockPaneType type{};
    };
    std::array<OrderedPane, kDockPaneCount> ordered{};
    std::size_t count{};
    for (std::size_t index = 0U; index < kDockPaneCount; ++index) {
        const auto type = static_cast<DockPaneType>(index);
        const DockPanePlacement* pane = model_->Pane(type);
        if (pane != nullptr && pane->present && pane->zone == zone) {
            ordered[count++] = OrderedPane{pane->order, type};
        }
    }
    std::sort(
        ordered.begin(),
        ordered.begin() + static_cast<std::ptrdiff_t>(count),
        [](const OrderedPane& left, const OrderedPane& right) {
            return left.order < right.order;
        });
    TabCtrl_DeleteAllItems(tabs.control);
    int selected{};
    for (std::size_t index = 0U; index < count; ++index) {
        const PaneDescriptor* descriptor = FindPaneDescriptor(ordered[index].type);
        if (descriptor == nullptr) {
            continue;
        }
        wchar_t title[128]{};
        TCITEMW item{};
        item.mask = TCIF_TEXT | TCIF_PARAM;
        item.pszText = const_cast<wchar_t*>(
            LoadPaneTitle(instance_, *descriptor, title));
        item.lParam = static_cast<LPARAM>(ordered[index].type);
        TabCtrl_InsertItem(tabs.control, static_cast<int>(index), &item);
        if (state->active_tab == ordered[index].type) {
            selected = static_cast<int>(index);
        }
    }
    TabCtrl_SetCurSel(tabs.control, selected);
    DockRect bounds = geometry_.zones[static_cast<std::size_t>(zone)];
    bounds.height = std::min(bounds.height, ScaleDip(kTabHeightDip, dpi_));
    PlaceWindow(tabs.control, bounds, true);
}

void DockHost::NotifyChanged() noexcept {
    if (!applying_ && changed_ != nullptr) {
        changed_(changed_context_);
    }
}

void DockHost::ShowContextMenu(
    DockPaneType type, POINT screen) noexcept {
    if (model_ == nullptr || FindPaneDescriptor(type) == nullptr) {
        return;
    }
    if (screen.x == -1 && screen.y == -1) {
        HWND source = ContentWindow(type);
        RECT bounds{};
        if (source == nullptr || GetWindowRect(source, &bounds) == FALSE) {
            return;
        }
        screen = POINT{
            (bounds.left + bounds.right) / 2,
            (bounds.top + bounds.bottom) / 2};
    }
    HMENU menu = CreatePopupMenu();
    if (menu == nullptr) {
        return;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    for (const DockZone zone : {
             DockZone::TopContext, DockZone::Left, DockZone::Right, DockZone::Bottom}) {
        if (!model_->IsZoneAllowed(type, zone)) {
            continue;
        }
        const UINT flags = MF_STRING
            | (pane != nullptr && pane->zone == zone ? MF_CHECKED : 0U);
        AppendMenuW(menu, flags, ZoneCommand(zone), ZoneLabel(zone));
    }
    AppendMenuW(menu, MF_SEPARATOR, 0U, nullptr);
    AppendMenuW(
        menu,
        MF_STRING
            | (pane != nullptr && pane->zone == DockZone::Floating ? MF_CHECKED : 0U),
        kContextFloat,
        L"フローティング");
    if (pane != nullptr && IsDockedZone(pane->zone)
        && model_->PaneCount(pane->zone) > 1U) {
        const DockZoneState* zone = model_->Zone(pane->zone);
        AppendMenuW(menu, MF_SEPARATOR, 0U, nullptr);
        AppendMenuW(
            menu,
            MF_STRING
                | (zone != nullptr && zone->mode == DockStackMode::Split
                       ? MF_CHECKED
                       : 0U),
            kContextSplit,
            L"分割表示");
        AppendMenuW(
            menu,
            MF_STRING
                | (zone != nullptr && zone->mode == DockStackMode::Tabs
                       ? MF_CHECKED
                       : 0U),
            kContextTabs,
            L"タブ表示");
    }
    AppendMenuW(menu, MF_SEPARATOR, 0U, nullptr);
    AppendMenuW(menu, MF_STRING, kContextHide, L"非表示");
    AppendMenuW(menu, MF_STRING, kContextReset, L"このパネルを初期位置へ戻す");
    const UINT command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        screen.x,
        screen.y,
        0,
        owner_,
        nullptr);
    DestroyMenu(menu);
    const DockZone target = CommandZone(command);
    if (target != DockZone::Count) {
        static_cast<void>(DockPane(type, target));
    } else if (command == kContextFloat) {
        static_cast<void>(FloatPane(type));
    } else if (command == kContextHide) {
        static_cast<void>(HidePane(type));
    } else if (command == kContextReset) {
        static_cast<void>(ResetPane(type));
    } else if (pane != nullptr && IsDockedZone(pane->zone)
               && command == kContextSplit) {
        static_cast<void>(SetZoneMode(pane->zone, DockStackMode::Split));
    } else if (pane != nullptr && IsDockedZone(pane->zone)
               && command == kContextTabs) {
        static_cast<void>(SetZoneMode(pane->zone, DockStackMode::Tabs));
    }
}

DockZone DockHost::PreviewZoneAt(
    DockPaneType type, POINT screen) const noexcept {
    if (owner_ == nullptr || model_ == nullptr) {
        return DockZone::Count;
    }
    POINT client = screen;
    RECT bounds{};
    if (ScreenToClient(owner_, &client) == FALSE
        || GetClientRect(owner_, &bounds) == FALSE || client.x < 0 || client.y < 0
        || client.x >= bounds.right || client.y >= bounds.bottom) {
        return DockZone::Count;
    }
    const int edge_x = std::max(
        ScaleDip(48, dpi_), static_cast<int>(bounds.right / 5));
    const int edge_y = std::max(
        ScaleDip(40, dpi_), static_cast<int>(bounds.bottom / 5));
    const std::array<DockZone, 4U> candidates{
        client.y < edge_y ? DockZone::TopContext : DockZone::Count,
        client.y >= bounds.bottom - edge_y ? DockZone::Bottom : DockZone::Count,
        client.x < edge_x ? DockZone::Left : DockZone::Count,
        client.x >= bounds.right - edge_x ? DockZone::Right : DockZone::Count};
    for (const DockZone zone : candidates) {
        if (zone != DockZone::Count && model_->IsZoneAllowed(type, zone)) {
            return zone;
        }
    }
    return DockZone::Count;
}

void DockHost::ShowDockPreview(
    DockPaneType type, POINT screen) noexcept {
    const DockZone zone = PreviewZoneAt(type, screen);
    if (zone == DockZone::Count || preview_ == nullptr) {
        HideDockPreview();
        return;
    }
    RECT client{};
    if (GetClientRect(owner_, &client) == FALSE) {
        HideDockPreview();
        return;
    }
    DockRect bounds = geometry_.zones[static_cast<std::size_t>(zone)];
    if (!HasArea(bounds)) {
        const int quarter_width = std::max(1L, client.right / 4);
        const int quarter_height = std::max(1L, client.bottom / 4);
        switch (zone) {
            case DockZone::TopContext:
                bounds = DockRect{0, 0, client.right, quarter_height};
                break;
            case DockZone::Bottom:
                bounds = DockRect{
                    0, client.bottom - quarter_height, client.right, quarter_height};
                break;
            case DockZone::Left:
                bounds = DockRect{0, 0, quarter_width, client.bottom};
                break;
            case DockZone::Right:
                bounds = DockRect{
                    client.right - quarter_width, 0, quarter_width, client.bottom};
                break;
            default: break;
        }
    }
    preview_zone_ = zone;
    SetWindowPos(
        preview_,
        HWND_TOP,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW);
    InvalidateRect(preview_, nullptr, TRUE);
}

void DockHost::HideDockPreview() noexcept {
    preview_zone_ = DockZone::Count;
    if (preview_ != nullptr) {
        ShowWindow(preview_, SW_HIDE);
    }
}

void DockHost::FinishFloatingMove(PaneHostState& pane) noexcept {
    if (model_ == nullptr) {
        HideDockPreview();
        return;
    }
    const DockZone target = preview_zone_;
    HideDockPreview();
    if (target != DockZone::Count) {
        const DockResult result = model_->MovePane(pane.type, target);
        if (result == DockResult::Ok) {
            NotifyChanged();
        }
        return;
    }
    CaptureFloatingPlacement(pane);
}

void DockHost::CaptureFloatingPlacement(PaneHostState& pane) noexcept {
    if (model_ == nullptr || pane.floating_window == nullptr) {
        return;
    }
    DockPanePlacement* placement = model_->Pane(pane.type);
    RECT bounds{};
    const UINT dpi = GetDpiForWindow(pane.floating_window);
    if (placement == nullptr
        || GetWindowRect(pane.floating_window, &bounds) == FALSE) {
        return;
    }
    placement->floating = DockFloatingPlacement{
        PixelsToDip(bounds.left, dpi),
        PixelsToDip(bounds.top, dpi),
        PixelsToDip(bounds.right - bounds.left, dpi),
        PixelsToDip(bounds.bottom - bounds.top, dpi)};
    NotifyChanged();
}

void DockHost::UpdateZoneExtentFromPoint(
    const SplitterHostState& splitter, POINT screen) noexcept {
    if (model_ == nullptr || owner_ == nullptr) {
        return;
    }
    POINT client_point = screen;
    RECT client{};
    if (ScreenToClient(owner_, &client_point) == FALSE
        || GetClientRect(owner_, &client) == FALSE) {
        return;
    }
    const DockRect zone = geometry_.zones[static_cast<std::size_t>(
        splitter.geometry.zone)];
    int extent{};
    if (splitter.geometry.zone == DockZone::Left
        || splitter.geometry.zone == DockZone::Right) {
        extent = zone.x == 0 ? client_point.x : client.right - client_point.x;
    } else {
        extent = zone.y == 0 ? client_point.y : client.bottom - client_point.y;
    }
    const DockResult result = model_->SetZoneExtentDip(
        splitter.geometry.zone, PixelsToDip(std::max(1, extent), dpi_));
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
}

void DockHost::UpdateStackBoundaryFromPoint(
    SplitterHostState& splitter, POINT screen) noexcept {
    if (model_ == nullptr) {
        return;
    }
    const bool horizontal = splitter.geometry.zone == DockZone::TopContext
        || splitter.geometry.zone == DockZone::Bottom;
    const int delta = horizontal
        ? screen.x - splitter.last_screen.x
        : screen.y - splitter.last_screen.y;
    const DockRect zone = geometry_.zones[static_cast<std::size_t>(
        splitter.geometry.zone)];
    const int extent = std::max(1, horizontal ? zone.width : zone.height);
    splitter.last_screen = screen;
    const int delta_milli = delta * 1000 / extent;
    const DockResult result = model_->AdjustSplitBoundary(
        splitter.geometry.zone, splitter.geometry.boundary, delta_milli);
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
}

void DockHost::ActivateSelectedTab(TabHostState& tabs) noexcept {
    if (applying_ || model_ == nullptr || tabs.control == nullptr) {
        return;
    }
    const int selected = TabCtrl_GetCurSel(tabs.control);
    if (selected < 0) {
        return;
    }
    TCITEMW item{};
    item.mask = TCIF_PARAM;
    if (TabCtrl_GetItem(tabs.control, selected, &item) == FALSE) {
        return;
    }
    const DockResult result = model_->SetActiveTab(
        tabs.zone, static_cast<DockPaneType>(item.lParam));
    if (result == DockResult::Ok) {
        NotifyChanged();
    }
}

LRESULT CALLBACK DockHost::FloatingWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    PaneHostState* pane = reinterpret_cast<PaneHostState*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));
    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        pane = create == nullptr
            ? nullptr
            : reinterpret_cast<PaneHostState*>(create->lpCreateParams);
        if (pane == nullptr) {
            return FALSE;
        }
        SetWindowLongPtrW(window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(pane));
    }
    switch (message) {
        case WM_CLOSE:
            if (pane != nullptr && pane->host != nullptr) {
                static_cast<void>(pane->host->HidePane(pane->type));
            }
            return 0;
        case WM_SIZE:
            if (pane != nullptr && pane->host != nullptr) {
                pane->host->LayoutFloatingContent(*pane);
            }
            return 0;
        case WM_MOVING:
            if (pane != nullptr && pane->host != nullptr) {
                POINT cursor{};
                if (GetCursorPos(&cursor) != FALSE) {
                    pane->host->ShowDockPreview(pane->type, cursor);
                }
            }
            break;
        case WM_EXITSIZEMOVE:
            if (pane != nullptr && pane->host != nullptr) {
                pane->host->FinishFloatingMove(*pane);
            }
            return 0;
        case WM_DPICHANGED: {
            const auto* suggested = reinterpret_cast<const RECT*>(lparam);
            if (suggested != nullptr) {
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
        }
        case WM_GETMINMAXINFO:
            if (pane != nullptr) {
                const PaneDescriptor* descriptor = FindPaneDescriptor(pane->type);
                auto* limits = reinterpret_cast<MINMAXINFO*>(lparam);
                if (descriptor != nullptr && limits != nullptr) {
                    const UINT dpi = GetDpiForWindow(window);
                    limits->ptMinTrackSize.x = ScaleDip(
                        descriptor->minimum_width_dip, dpi);
                    limits->ptMinTrackSize.y = ScaleDip(
                        descriptor->minimum_height_dip, dpi);
                }
            }
            return 0;
        case WM_CONTEXTMENU:
            if (pane != nullptr && pane->host != nullptr) {
                pane->host->ShowContextMenu(
                    pane->type,
                    POINT{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
            }
            return 0;
        case WM_CANCELMODE:
            if (pane != nullptr && pane->host != nullptr) {
                // Preview is transient: cancelling a system move must not
                // commit a dock target or alter the saved layout model.
                pane->host->HideDockPreview();
            }
            return 0;
        case WM_THEMECHANGED:
        case WM_SETTINGCHANGE:
            InvalidateRect(window, nullptr, TRUE);
            break;
        case WM_NCDESTROY:
            if (pane != nullptr) {
                if (pane->host != nullptr) {
                    pane->host->HideDockPreview();
                }
                pane->floating_window = nullptr;
            }
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            break;
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

LRESULT CALLBACK DockHost::PaneSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* pane = reinterpret_cast<PaneHostState*>(reference);
    if (message == WM_CONTEXTMENU && pane != nullptr && pane->host != nullptr) {
        pane->host->ShowContextMenu(
            pane->type, POINT{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
        return 0;
    }
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(window, PaneSubclassProcedure, kPaneSubclass);
        if (pane != nullptr) {
            pane->content = nullptr;
        }
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

LRESULT CALLBACK DockHost::SplitterSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* splitter = reinterpret_cast<SplitterHostState*>(reference);
    if (splitter == nullptr || splitter->host == nullptr) {
        return DefSubclassProc(window, message, wparam, lparam);
    }
    switch (message) {
        case WM_LBUTTONDOWN:
            GetCursorPos(&splitter->last_screen);
            SetCapture(window);
            SetFocus(window);
            return 0;
        case WM_MOUSEMOVE:
            if (GetCapture() == window) {
                POINT current{};
                GetCursorPos(&current);
                if (splitter->geometry.kind == DockSplitterKind::ZoneExtent) {
                    splitter->host->UpdateZoneExtentFromPoint(*splitter, current);
                } else {
                    splitter->host->UpdateStackBoundaryFromPoint(*splitter, current);
                }
                return 0;
            }
            break;
        case WM_LBUTTONUP:
            if (GetCapture() == window) {
                ReleaseCapture();
            }
            return 0;
        case WM_KEYDOWN: {
            const int direction = wparam == VK_LEFT || wparam == VK_UP
                ? -1
                : (wparam == VK_RIGHT || wparam == VK_DOWN ? 1 : 0);
            if (direction == 0 || splitter->host->model_ == nullptr) {
                break;
            }
            DockResult result = DockResult::NoOp;
            if (splitter->geometry.kind == DockSplitterKind::StackBoundary) {
                result = splitter->host->model_->AdjustSplitBoundary(
                    splitter->geometry.zone,
                    splitter->geometry.boundary,
                    direction * 20);
            } else {
                const DockZoneState* zone = splitter->host->model_->Zone(
                    splitter->geometry.zone);
                if (zone != nullptr) {
                    result = splitter->host->model_->SetZoneExtentDip(
                        splitter->geometry.zone,
                        zone->extent_dip + direction * 4);
                }
            }
            if (result == DockResult::Ok) {
                splitter->host->NotifyChanged();
            }
            return 0;
        }
        case WM_SETCURSOR: {
            const bool horizontal_line =
                splitter->geometry.kind == DockSplitterKind::ZoneExtent
                ? splitter->geometry.zone == DockZone::TopContext
                    || splitter->geometry.zone == DockZone::Bottom
                : splitter->geometry.zone == DockZone::Left
                    || splitter->geometry.zone == DockZone::Right;
            SetCursor(LoadCursorW(nullptr, horizontal_line ? IDC_SIZENS : IDC_SIZEWE));
            return TRUE;
        }
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                window, SplitterSubclassProcedure, kSplitterSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

LRESULT CALLBACK DockHost::TabSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* tabs = reinterpret_cast<TabHostState*>(reference);
    const LRESULT result = DefSubclassProc(window, message, wparam, lparam);
    if (tabs != nullptr && tabs->host != nullptr
        && (message == WM_LBUTTONUP || message == WM_KEYUP)) {
        tabs->host->ActivateSelectedTab(*tabs);
    }
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(window, TabSubclassProcedure, kTabSubclass);
        if (tabs != nullptr) {
            tabs->control = nullptr;
        }
    }
    return result;
}

}  // namespace inkpod::windows::ui
