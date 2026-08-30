#include "ui/ui_resources.h"

#include "dock_host.h"

#include <commctrl.h>
#include <initguid.h>
#include <oleacc.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstdlib>
#include <cstdint>
#include <cwchar>
#include <span>

#include "app/resource.h"
#include "ui/localization.h"
#include "ui/panes/pane_dialog_layout.h"
#include "ui/tab_close_button.h"

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kFloatingPaneClass[] = L"InkpodFloatingDockPaneV1";
constexpr UINT_PTR kPaneSubclass = 1U;
constexpr UINT_PTR kSplitterSubclass = 1U;
constexpr UINT_PTR kTabSubclass = 1U;
constexpr UINT_PTR kToolTabSubclass = 1U;
constexpr UINT_PTR kToolTabCloseButtonSubclass = 2U;
constexpr UINT_PTR kPaneTabCloseButtonSubclass = 3U;
constexpr int kTabHeightDip = 28;

constexpr UINT kContextFloat = 1U;
constexpr UINT kContextClose = 2U;
constexpr UINT kContextMoveToNewTab = 3U;
constexpr UINT kContextMoveFirst = 100U;

int ScaleDip(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

int PixelsToDip(int value, UINT dpi) noexcept {
    return MulDiv(value, 96, static_cast<int>(dpi == 0U ? 96U : dpi));
}

RECT ClampToVisibleWorkArea(RECT candidate) noexcept {
    const HMONITOR monitor = MonitorFromRect(&candidate, MONITOR_DEFAULTTONEAREST);
    MONITORINFO info{sizeof(info)};
    if (monitor == nullptr || GetMonitorInfoW(monitor, &info) == FALSE) {
        return candidate;
    }
    const int available_width = std::max(1L, info.rcWork.right - info.rcWork.left);
    const int available_height = std::max(1L, info.rcWork.bottom - info.rcWork.top);
    const int width = std::clamp(
        static_cast<int>(candidate.right - candidate.left), 1, available_width);
    const int height = std::clamp(
        static_cast<int>(candidate.bottom - candidate.top), 1, available_height);
    candidate.left = std::clamp(
        candidate.left, info.rcWork.left, info.rcWork.right - width);
    candidate.top = std::clamp(
        candidate.top, info.rcWork.top, info.rcWork.bottom - height);
    candidate.right = candidate.left + width;
    candidate.bottom = candidate.top + height;
    return candidate;
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

bool WindowMatchesPlacement(
    HWND window, const DockRect& bounds, bool visible) noexcept {
    const bool show = visible && HasArea(bounds);
    const bool has_visible_style =
        (GetWindowLongPtrW(window, GWL_STYLE) & WS_VISIBLE) != 0;
    if (has_visible_style != show) {
        return false;
    }
    if (!show) {
        return true;
    }
    RECT current{};
    if (GetWindowRect(window, &current) == FALSE) {
        return false;
    }
    const HWND parent = GetParent(window);
    POINT top_left{current.left, current.top};
    POINT bottom_right{current.right, current.bottom};
    if (parent != nullptr
        && (ScreenToClient(parent, &top_left) == FALSE
            || ScreenToClient(parent, &bottom_right) == FALSE)) {
        return false;
    }
    return top_left.x == bounds.x && top_left.y == bounds.y
        && bottom_right.x - top_left.x == std::max(0, bounds.width)
        && bottom_right.y - top_left.y == std::max(0, bounds.height);
}

void PlaceWindow(HWND window, const DockRect& bounds, bool visible) noexcept {
    if (window == nullptr) {
        return;
    }
    const bool show = visible && HasArea(bounds);
    if (WindowMatchesPlacement(window, bounds, visible)) {
        return;
    }
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

const wchar_t* LoadPaneTitle(
    HINSTANCE instance,
    const PaneDescriptor& descriptor,
    wchar_t (&buffer)[128]) noexcept {
    if (instance != nullptr
        && LoadLocalizedStringW(
               instance,
               static_cast<UINT>(descriptor.title_resource_id),
               buffer,
               static_cast<int>(std::size(buffer)))
            > 0) {
        return buffer;
    }
    return descriptor.fallback_title;
}

const wchar_t* LoadToolTabTitle(
    HINSTANCE instance,
    const ToolTab& tab,
    wchar_t (&buffer)[128]) noexcept {
    if (tab.pane_count == 0U) {
        buffer[0] = L'\0';
        return buffer;
    }
    const PaneDescriptor* descriptor = FindPaneDescriptor(tab.panes[0]);
    if (descriptor == nullptr) {
        buffer[0] = L'\0';
        return buffer;
    }
    return LoadPaneTitle(instance, *descriptor, buffer);
}

bool LoadToolTabDescription(
    HINSTANCE instance,
    const ToolTab& tab,
    std::span<wchar_t> output) noexcept {
    if (tab.pane_count == 0U || output.empty()) {
        return false;
    }
    std::size_t written{};
    for (std::size_t index = 0U; index < tab.pane_count; ++index) {
        const PaneDescriptor* descriptor = FindPaneDescriptor(tab.panes[index]);
        if (descriptor == nullptr) {
            output[0] = L'\0';
            return false;
        }
        wchar_t title_buffer[128]{};
        const wchar_t* title = LoadPaneTitle(instance, *descriptor, title_buffer);
        const std::size_t length = std::wcslen(title);
        const std::size_t separator = index == 0U ? 0U : 2U;
        if (length + separator >= output.size() - written) {
            output[0] = L'\0';
            return false;
        }
        if (separator != 0U) {
            output[written++] = L',';
            output[written++] = L' ';
        }
        std::copy_n(title, length, output.begin() + written);
        written += length;
    }
    output[written] = L'\0';
    return true;
}

ToolTabId ToolTabAt(HWND tabs, int index) noexcept {
    if (tabs == nullptr || index < 0) {
        return {};
    }
    TCITEMW item{};
    item.mask = TCIF_PARAM;
    return TabCtrl_GetItem(tabs, index, &item) == FALSE
        ? ToolTabId{}
        : ToolTabId{static_cast<std::uint32_t>(item.lParam)};
}

int HitToolTab(HWND tabs, POINT client) noexcept {
    TCHITTESTINFO hit{};
    hit.pt = client;
    const int index = TabCtrl_HitTest(tabs, &hit);
    return index >= 0 && (hit.flags & TCHT_NOWHERE) == 0U ? index : -1;
}

bool ExceedsDragThreshold(POINT origin, POINT current) noexcept {
    return std::abs(current.x - origin.x) >= GetSystemMetrics(SM_CXDRAG)
        || std::abs(current.y - origin.y) >= GetSystemMetrics(SM_CYDRAG);
}

void RedrawSplitterNow(HWND window) noexcept {
    RedrawWindow(
        window,
        nullptr,
        nullptr,
        RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW);
}

const wchar_t* SplitterName(const DockSplitterGeometry& splitter) noexcept {
    if (splitter.kind == DockSplitterKind::StackBoundary) {
        switch (splitter.zone) {
            case DockZone::TopContext: return UiText(UiStringId::DockTopSplitPosition);
            case DockZone::Left: return UiText(UiStringId::DockLeftSplitPosition);
            case DockZone::Right: return UiText(UiStringId::DockRightSplitPosition);
            case DockZone::Bottom: return UiText(UiStringId::DockBottomSplitPosition);
            default: return UiText(UiStringId::DockPanelSplitPosition);
        }
    }
    switch (splitter.zone) {
        case DockZone::TopContext: return UiText(UiStringId::DockTopSize);
        case DockZone::Left: return UiText(UiStringId::DockLeftSize);
        case DockZone::Right: return UiText(UiStringId::DockRightSize);
        case DockZone::Bottom: return UiText(UiStringId::DockBottomSize);
        default: return UiText(UiStringId::DockGenericSize);
    }
}

bool SetAccessibleName(HWND window, const wchar_t* name) noexcept {
    IAccPropServices* properties = nullptr;
    const HRESULT create_result = CoCreateInstance(
        CLSID_AccPropServices,
        nullptr,
        CLSCTX_INPROC_SERVER,
        IID_PPV_ARGS(&properties));
    if (FAILED(create_result) || properties == nullptr) {
        return false;
    }
    const HRESULT set_result = properties->SetHwndPropStr(
        window,
        static_cast<DWORD>(OBJID_CLIENT),
        static_cast<DWORD>(CHILDID_SELF),
        PROPID_ACC_NAME,
        name);
    properties->Release();
    return SUCCEEDED(set_result);
}

bool SplitterHasHorizontalLine(
    const DockSplitterGeometry& geometry) noexcept {
    return geometry.kind == DockSplitterKind::ZoneExtent
        ? geometry.zone == DockZone::TopContext
            || geometry.zone == DockZone::Bottom
        : geometry.zone == DockZone::Left
            || geometry.zone == DockZone::Right;
}

void PaintSplitter(
    HWND window,
    const DockSplitterGeometry& geometry,
    bool highlighted,
    bool focused) noexcept {
    PAINTSTRUCT paint{};
    HDC context = BeginPaint(window, &paint);
    if (context == nullptr) {
        return;
    }
    RECT client{};
    if (GetClientRect(window, &client) != FALSE) {
        FillRect(context, &client, GetSysColorBrush(COLOR_BTNFACE));
        RECT rule = client;
        if (SplitterHasHorizontalLine(geometry)) {
            const LONG center = client.top + (client.bottom - client.top) / 2;
            rule.top = center;
            rule.bottom = std::min(client.bottom, center + 1);
        } else {
            const LONG center = client.left + (client.right - client.left) / 2;
            rule.left = center;
            rule.right = std::min(client.right, center + 1);
        }
        FillRect(
            context,
            &rule,
            GetSysColorBrush(highlighted ? COLOR_HIGHLIGHT : COLOR_3DSHADOW));
        if (focused) {
            DrawFocusRect(context, &client);
        }
    }
    EndPaint(window, &paint);
}

template <std::size_t Capacity>
class ScopedWindowRedrawSuspension final {
public:
    ScopedWindowRedrawSuspension() noexcept = default;
    ~ScopedWindowRedrawSuspension() noexcept { Restore(); }
    ScopedWindowRedrawSuspension(const ScopedWindowRedrawSuspension&) = delete;
    ScopedWindowRedrawSuspension& operator=(
        const ScopedWindowRedrawSuspension&) = delete;

    [[nodiscard]] bool Suspend(HWND window) noexcept {
        if (window == nullptr || count_ >= windows_.size()
            || (GetWindowLongPtrW(window, GWL_STYLE) & WS_VISIBLE) == 0) {
            return false;
        }
        SendMessageW(window, WM_SETREDRAW, FALSE, 0);
        windows_[count_++] = window;
        return true;
    }

    void Restore() noexcept {
        while (count_ > 0U) {
            const HWND window = windows_[--count_];
            if (IsWindow(window) != FALSE) {
                SendMessageW(window, WM_SETREDRAW, TRUE, 0);
            }
        }
    }

private:
    std::array<HWND, Capacity> windows_{};
    std::size_t count_{};
};

template <std::size_t Capacity>
class ScopedPaneDialogResizeDeferral final {
public:
    ScopedPaneDialogResizeDeferral() noexcept = default;
    ~ScopedPaneDialogResizeDeferral() noexcept { Restore(); }
    ScopedPaneDialogResizeDeferral(const ScopedPaneDialogResizeDeferral&) = delete;
    ScopedPaneDialogResizeDeferral& operator=(
        const ScopedPaneDialogResizeDeferral&) = delete;

    [[nodiscard]] bool Defer(HWND pane_root) noexcept {
        if (pane_root == nullptr
            || GetPropW(
                   pane_root, panes::kPaneDialogResizeDeferredProperty)
                != nullptr) {
            return true;
        }
        if (count_ >= pane_roots_.size()) {
            return false;
        }
        if (!panes::BeginPaneDialogLayoutTransaction(pane_root)) {
            return false;
        }
        if (panes::SetPaneDialogResizeDeferred(pane_root, true)) {
            pane_roots_[count_++] = pane_root;
            return true;
        }
        panes::EndPaneDialogLayoutTransaction(pane_root);
        return false;
    }

    [[nodiscard]] bool LayoutsSucceeded() const noexcept {
        for (std::size_t index = 0U; index < count_; ++index) {
            if (panes::PaneDialogLayoutFailed(pane_roots_[index])) {
                return false;
            }
        }
        return true;
    }

    void Restore() noexcept {
        while (count_ > 0U) {
            const HWND pane_root = pane_roots_[--count_];
            if (IsWindow(pane_root) != FALSE) {
                static_cast<void>(
                    panes::SetPaneDialogResizeDeferred(pane_root, false));
                panes::EndPaneDialogLayoutTransaction(pane_root);
            }
        }
    }

private:
    std::array<HWND, Capacity> pane_roots_{};
    std::size_t count_{};
};

}  // namespace

class DockHost::PlacementBatch final {
public:
    explicit PlacementBatch(HWND owner) noexcept : owner_(owner) {}

    [[nodiscard]] bool Place(
        HWND window, const DockRect& bounds, bool visible) noexcept {
        if (window == nullptr) {
            return true;
        }
        if (owner_ == nullptr || overflowed_) {
            return false;
        }
        const bool show = visible && HasArea(bounds);
        const bool redraw_suspended = WasRedrawSuspended(window);
        if (!redraw_suspended
            && WindowMatchesPlacement(window, bounds, visible)) {
            return true;
        }
        const bool was_visible =
            (GetWindowLongPtrW(window, GWL_STYLE) & WS_VISIBLE) != 0;
        RECT previous{};
        if (was_visible && WindowBoundsInOwner(window, previous)) {
            IncludeDirty(previous);
        }
        if (show) {
            IncludeDirty(ToRect(bounds));
        }
        for (std::size_t index = 0U; index < count_; ++index) {
            if (placements_[index].window == window) {
                placements_[index].bounds = bounds;
                placements_[index].show = show;
                return true;
            }
        }
        if (count_ >= placements_.size()) {
            overflowed_ = true;
            return false;
        }
        placements_[count_++] = PendingPlacement{window, bounds, show};
        return true;
    }

    [[nodiscard]] bool PrepareRedrawSuspension(HWND window) noexcept {
        if (window == nullptr
            || redraw_suspended_count_ >= redraw_suspended_tabs_.size()
            || (GetWindowLongPtrW(window, GWL_STYLE) & WS_VISIBLE) == 0) {
            return false;
        }
        IncludeWindow(window);
        redraw_suspended_tabs_[redraw_suspended_count_++] = window;
        return true;
    }

    void IncludeWindow(HWND window) noexcept {
        RECT bounds{};
        if (window != nullptr
            && (GetWindowLongPtrW(window, GWL_STYLE) & WS_VISIBLE) != 0
            && WindowBoundsInOwner(window, bounds)) {
            IncludeDirty(bounds);
        }
    }

    void IncludeDirty(const DockRect& bounds) noexcept {
        IncludeDirty(ToRect(bounds));
    }

    [[nodiscard]] bool Commit() noexcept {
        if (overflowed_) {
            return false;
        }
        if (count_ == 0U) {
            return true;
        }
        for (std::size_t index = 0U; index < count_; ++index) {
            if (!CapturePreviousState(placements_[index])) {
                return false;
            }
        }
        HDWP deferred = BeginDeferWindowPos(static_cast<int>(count_));
        bool complete = deferred != nullptr;
        if (complete) {
            for (std::size_t index = 0U; index < count_; ++index) {
                const PendingPlacement& placement = placements_[index];
                deferred = DeferWindowPos(
                    deferred,
                    placement.window,
                    nullptr,
                    placement.bounds.x,
                    placement.bounds.y,
                    std::max(0, placement.bounds.width),
                    std::max(0, placement.bounds.height),
                    PlacementFlags(placement.show));
                if (deferred == nullptr) {
                    complete = false;
                    break;
                }
            }
        }
        if (complete && EndDeferWindowPos(deferred) != FALSE
            && HasFinalState()) {
            return true;
        }
        complete = true;
        for (std::size_t index = 0U; index < count_; ++index) {
            const PendingPlacement& placement = placements_[index];
            const bool positioned = SetWindowPos(
                                        placement.window,
                                        nullptr,
                                        placement.bounds.x,
                                        placement.bounds.y,
                                        std::max(0, placement.bounds.width),
                                        std::max(0, placement.bounds.height),
                                        PlacementFlags(placement.show))
                != FALSE;
            complete = positioned && complete;
        }
        complete = HasFinalState() && complete;
        if (!complete) {
            // Direct placement can fail after publishing an earlier prefix.
            // Restore every registered window to the owner-relative bounds and
            // local visibility captured before either placement path ran.
            static_cast<void>(RestorePreviousState());
        }
        return complete;
    }

    [[nodiscard]] bool Rollback() const noexcept {
        return RestorePreviousState();
    }

    void Redraw() const noexcept {
        RECT client{};
        RECT clipped{};
        if (!has_dirty_ || GetClientRect(owner_, &client) == FALSE
            || IntersectRect(&clipped, &dirty_, &client) == FALSE) {
            return;
        }
        RedrawWindow(
            owner_,
            &clipped,
            nullptr,
            RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_UPDATENOW
                | RDW_ALLCHILDREN);
    }

private:
    struct PendingPlacement {
        HWND window{};
        DockRect bounds{};
        bool show{};
        DockRect previous_bounds{};
        bool previous_show{};
    };

    static constexpr std::size_t kMaximumPlacementCount =
        1U + kMaximumDockTabStacks + kDockPaneCount + kMaximumDockSplitters;

    static UINT PlacementFlags(bool show) noexcept {
        return SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER
            | SWP_NOREDRAW | SWP_NOCOPYBITS
            | (show ? SWP_SHOWWINDOW : SWP_HIDEWINDOW);
    }

    [[nodiscard]] bool CapturePreviousState(
        PendingPlacement& placement) const noexcept {
        RECT bounds{};
        if (placement.window == nullptr || GetParent(placement.window) != owner_
            || !WindowBoundsInOwner(placement.window, bounds)) {
            return false;
        }
        placement.previous_bounds = DockRect{
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top};
        placement.previous_show =
            (GetWindowLongPtrW(placement.window, GWL_STYLE) & WS_VISIBLE) != 0;
        return true;
    }

    [[nodiscard]] bool WindowMatchesState(
        const PendingPlacement& placement,
        const DockRect& bounds,
        bool show) const noexcept {
        if (placement.window == nullptr || GetParent(placement.window) != owner_
            || ((GetWindowLongPtrW(placement.window, GWL_STYLE) & WS_VISIBLE) != 0)
                != show) {
            return false;
        }
        RECT current{};
        return WindowBoundsInOwner(placement.window, current)
            && current.left == bounds.x && current.top == bounds.y
            && current.right - current.left == std::max(0, bounds.width)
            && current.bottom - current.top == std::max(0, bounds.height);
    }

    [[nodiscard]] bool HasFinalState() const noexcept {
        for (std::size_t index = 0U; index < count_; ++index) {
            const PendingPlacement& placement = placements_[index];
            if (!WindowMatchesState(
                    placement, placement.bounds, placement.show)) {
                return false;
            }
        }
        return true;
    }

    [[nodiscard]] bool RestorePreviousState() const noexcept {
        bool restored = true;
        for (std::size_t index = 0U; index < count_; ++index) {
            const PendingPlacement& placement = placements_[index];
            const bool positioned = SetWindowPos(
                                        placement.window,
                                        nullptr,
                                        placement.previous_bounds.x,
                                        placement.previous_bounds.y,
                                        std::max(0, placement.previous_bounds.width),
                                        std::max(0, placement.previous_bounds.height),
                                        PlacementFlags(placement.previous_show))
                != FALSE;
            restored = positioned && restored;
        }
        for (std::size_t index = 0U; index < count_; ++index) {
            const PendingPlacement& placement = placements_[index];
            restored = WindowMatchesState(
                           placement,
                           placement.previous_bounds,
                           placement.previous_show)
                && restored;
        }
        return restored;
    }

    [[nodiscard]] bool WasRedrawSuspended(HWND window) const noexcept {
        return std::find(
                   redraw_suspended_tabs_.begin(),
                   redraw_suspended_tabs_.begin()
                       + static_cast<std::ptrdiff_t>(redraw_suspended_count_),
                   window)
            != redraw_suspended_tabs_.begin()
                + static_cast<std::ptrdiff_t>(redraw_suspended_count_);
    }

    bool WindowBoundsInOwner(HWND window, RECT& bounds) const noexcept {
        if (GetWindowRect(window, &bounds) == FALSE) {
            return false;
        }
        POINT top_left{bounds.left, bounds.top};
        POINT bottom_right{bounds.right, bounds.bottom};
        if (ScreenToClient(owner_, &top_left) == FALSE
            || ScreenToClient(owner_, &bottom_right) == FALSE) {
            return false;
        }
        bounds = RECT{top_left.x, top_left.y, bottom_right.x, bottom_right.y};
        return true;
    }

    void IncludeDirty(const RECT& bounds) noexcept {
        if (bounds.right <= bounds.left || bounds.bottom <= bounds.top) {
            return;
        }
        if (!has_dirty_) {
            dirty_ = bounds;
            has_dirty_ = true;
            return;
        }
        RECT combined{};
        if (UnionRect(&combined, &dirty_, &bounds) != FALSE) {
            dirty_ = combined;
        }
    }

    HWND owner_{};
    std::array<PendingPlacement, kMaximumPlacementCount> placements_{};
    std::array<HWND, kMaximumDockTabStacks + 1U> redraw_suspended_tabs_{};
    std::size_t count_{};
    std::size_t redraw_suspended_count_{};
    RECT dirty_{};
    bool has_dirty_{};
    bool overflowed_{};
};

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
        tab_states_[index].zone = static_cast<DockZone>(index / kDockPaneCount);
        tab_states_[index].stack = static_cast<std::uint8_t>(
            index % kDockPaneCount);
    }
    for (ToolTabCloseButtonSlot& slot : tool_tab_close_buttons_) {
        slot.host = this;
    }
}

DockHost::~DockHost() noexcept {
    if (tab_font_ == nullptr) {
        return;
    }
    for (const TabHostState& tabs : tab_states_) {
        if (tabs.control != nullptr) {
            SendMessageW(tabs.control, WM_SETFONT, 0, FALSE);
        }
    }
    if (right_tool_tab_control_ != nullptr) {
        SendMessageW(right_tool_tab_control_, WM_SETFONT, 0, FALSE);
    }
    DeleteObject(tab_font_);
}

bool DockHost::Initialize(
    HWND owner,
    HINSTANCE instance,
    DockLayoutModel& model,
    RightToolTabsModel& right_tool_tabs) noexcept {
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
    right_tool_tabs_ = &right_tool_tabs;
    committed_model_ = model;
    committed_right_tool_tabs_ = right_tool_tabs;
    for (std::size_t index = 0U; index < panes_.size(); ++index) {
        committed_auto_hide_expanded_[index] = panes_[index].auto_hide_expanded;
        committed_auto_hide_edges_[index] = panes_[index].auto_hide_edge;
    }
    committed_layout_state_valid_ = true;
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
    SetAccessibleName(preview_, UiText(UiStringId::DockPreviewAccessibleName));
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
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_TABSTOP,
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
    right_tool_tab_control_ = CreateWindowExW(
        0,
        WC_TABCONTROLW,
        nullptr,
        WS_CHILD | WS_CLIPSIBLINGS | WS_TABSTOP | TCS_SINGLELINE | TCS_TOOLTIPS,
        0,
        0,
        0,
        0,
        owner_,
        nullptr,
        instance_,
        nullptr);
    if (right_tool_tab_control_ == nullptr
        || SetWindowSubclass(
               right_tool_tab_control_,
               ToolTabSubclassProcedure,
               kToolTabSubclass,
               reinterpret_cast<DWORD_PTR>(this))
            == FALSE) {
        return false;
    }
    SetAccessibleName(
        right_tool_tab_control_, UiText(UiStringId::RightToolTabsAccessibleName));
    if (!UpdateTabFont(GetDpiForWindow(owner_))) {
        return false;
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

bool DockHost::ApplyLayout(
    const DockLayoutGeometry& geometry,
    UINT dpi,
    DockHostChangeKind kind) noexcept {
    if (!initialized_ || model_ == nullptr) {
        return false;
    }
    const DockLayoutGeometry previous_geometry = geometry_;
    const UINT previous_dpi = dpi_;
    const auto finish_failed_layout = [this,
                                        &previous_geometry,
                                        previous_dpi]() noexcept {
        applying_ = false;
        const bool retry_committed_layout = layout_mutation_pending_
            && committed_layout_state_valid_;
        if (retry_committed_layout) {
            RestoreLayoutMutation();
        }
        if (retry_committed_layout && !rolling_back_layout_) {
            rolling_back_layout_ = true;
            static_cast<void>(ApplyLayout(
                previous_geometry,
                previous_dpi,
                DockHostChangeKind::Structure));
            rolling_back_layout_ = false;
        }
        return false;
    };
    applying_ = true;
    geometry_ = geometry;
    dpi_ = dpi == 0U ? 96U : dpi;
    const bool synchronize_items = kind == DockHostChangeKind::Structure;
    const bool synchronize_tab_metrics =
        tab_font_ == nullptr || tab_font_dpi_ != dpi_;

    PlacementBatch placements(owner_);
    if (kind == DockHostChangeKind::Structure) {
        const std::size_t right = static_cast<std::size_t>(DockZone::Right);
        placements.IncludeDirty(previous_geometry.right_tool_tabs);
        placements.IncludeDirty(previous_geometry.zones[right]);
        placements.IncludeDirty(geometry_.right_tool_tabs);
        placements.IncludeDirty(geometry_.zones[right]);
    }
    ScopedPaneDialogResizeDeferral<kDockPaneCount> pane_resize_deferral;
    bool pane_resize_deferred = true;
    for (const PaneHostState& pane : panes_) {
        const DockPanePlacement* placement = model_->Pane(pane.type);
        // Only pane roots whose final parent is this DockHost participate in
        // the owner's bounded final repaint. Floating and expanded AutoHide
        // roots must complete under their own parent instead of being deferred
        // into an unrelated owner subtree.
        if (pane.content != nullptr && GetParent(pane.content) == owner_
            && placement != nullptr && placement->zone != DockZone::Floating
            && !(placement->zone == DockZone::AutoHide
                && pane.auto_hide_expanded)) {
            pane_resize_deferred = pane_resize_deferral.Defer(pane.content)
                && pane_resize_deferred;
        }
    }
    if (!pane_resize_deferred) {
        geometry_ = previous_geometry;
        dpi_ = previous_dpi;
        pane_resize_deferral.Restore();
        // A pending DockHost mutation is reprojected from its captured model by
        // finish_failed_layout. Do not expose the rejected tab projection first.
        return finish_failed_layout();
    }
    ScopedWindowRedrawSuspension<kMaximumDockTabStacks + 1U>
        tab_redraw_suspension;
    if (synchronize_items || synchronize_tab_metrics) {
        if (placements.PrepareRedrawSuspension(right_tool_tab_control_)) {
            static_cast<void>(
                tab_redraw_suspension.Suspend(right_tool_tab_control_));
        }
        for (const TabHostState& tabs : tab_states_) {
            if (placements.PrepareRedrawSuspension(tabs.control)) {
                static_cast<void>(tab_redraw_suspension.Suspend(tabs.control));
            }
        }
    }
    static_cast<void>(UpdateTabFont(dpi_));

    ApplyToolTabLayout(synchronize_items, placements);
    for (TabHostState& tabs : tab_states_) {
        ApplyTabLayout(tabs, synchronize_items, placements);
    }
    for (PaneHostState& pane : panes_) {
        ApplyPaneLayout(pane, &placements);
    }
    for (std::size_t index = 0U; index < splitters_.size(); ++index) {
        if (index < geometry_.splitter_count) {
            static_cast<void>(placements.Place(
                splitters_[index], geometry_.splitters[index].bounds, true));
        } else {
            static_cast<void>(placements.Place(splitters_[index], {}, false));
        }
    }
    tab_redraw_suspension.Restore();
    bool geometry_committed = placements.Commit();
    if (geometry_committed && !pane_resize_deferral.LayoutsSucceeded()) {
        static_cast<void>(placements.Rollback());
        geometry_committed = false;
    }
    if (!geometry_committed) {
        geometry_ = previous_geometry;
        dpi_ = previous_dpi;
        static_cast<void>(UpdateTabFont(previous_dpi));
        pane_resize_deferral.Restore();
        if (!layout_mutation_pending_) {
            placements.Redraw();
        }
        return finish_failed_layout();
    }
    for (std::size_t index = 0U; index < splitter_states_.size(); ++index) {
        SplitterHostState& state = splitter_states_[index];
        if (index < geometry_.splitter_count) {
            const DockSplitterGeometry& next = geometry_.splitters[index];
            if (!state.accessible_name_set
                || state.geometry.zone != next.zone
                || state.geometry.kind != next.kind) {
                state.accessible_name_set =
                    SetAccessibleName(splitters_[index], SplitterName(next));
            }
            state.geometry = next;
        } else {
            state.hovered = false;
            state.focused = false;
        }
    }
    LayoutToolTabCloseButtons();
    for (TabHostState& tabs : tab_states_) {
        LayoutPaneTabCloseButtons(tabs);
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
    pane_resize_deferral.Restore();
    placements.Redraw();
    applying_ = false;
    CommitLayoutState();
    return true;
}

DockResult DockHost::TogglePane(DockPaneType type) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    return model_->IsPaneVisible(type) ? HidePane(type) : RestorePane(type);
}

DockResult DockHost::DockPane(DockPaneType type, DockZone zone) noexcept {
    if (model_ == nullptr || !BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->MovePane(type, zone);
    if (result == DockResult::Ok) {
        if (zone == DockZone::Right && right_tool_tabs_ != nullptr) {
            SelectVisibleToolTabForPane(type);
        } else {
            RemovePaneFromToolTabs(type);
        }
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
    } else {
        CancelLayoutMutation();
    }
    return result;
}

DockResult DockHost::FloatPane(DockPaneType type) noexcept {
    if (model_ == nullptr || !BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    if (pane == nullptr) {
        CancelLayoutMutation();
        return DockResult::InvalidPane;
    }
    const DockResult result = model_->FloatPane(type, pane->floating);
    if (result == DockResult::Ok) {
        RemovePaneFromToolTabs(type);
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
    } else {
        CancelLayoutMutation();
    }
    return result;
}

DockResult DockHost::HidePane(DockPaneType type) noexcept {
    if (model_ == nullptr || !BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->HidePane(type);
    if (result == DockResult::Ok) {
        RemovePaneFromToolTabs(type);
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
    } else {
        CancelLayoutMutation();
    }
    return result;
}

DockResult DockHost::SetPaneAutoHide(
    DockPaneType type, bool auto_hide) noexcept {
    if (model_ == nullptr || !BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->SetPaneAutoHide(type, auto_hide);
    if (result == DockResult::Ok) {
        const DockPanePlacement* placement = model_->Pane(type);
        if (placement != nullptr && placement->zone == DockZone::Right) {
            SelectVisibleToolTabForPane(type);
        } else {
            RemovePaneFromToolTabs(type);
        }
        if (PaneHostState* pane = PaneState(type); pane != nullptr) {
            pane->auto_hide_expanded = false;
        }
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
    } else {
        CancelLayoutMutation();
    }
    return result;
}

DockResult DockHost::RestorePane(DockPaneType type) noexcept {
    if (model_ == nullptr || !BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->RestorePane(type);
    if (result == DockResult::Ok) {
        const DockPanePlacement* pane = model_->Pane(type);
        if (pane != nullptr && pane->zone == DockZone::Right
            && right_tool_tabs_ != nullptr) {
            SelectVisibleToolTabForPane(type);
        }
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
        FocusPane(type);
    } else {
        CancelLayoutMutation();
    }
    return result;
}

DockResult DockHost::ResetPane(DockPaneType type) noexcept {
    if (model_ == nullptr || !BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->ResetPane(type);
    if (result == DockResult::Ok) {
        const DockPanePlacement* pane = model_->Pane(type);
        if (pane != nullptr && pane->zone == DockZone::Right
            && right_tool_tabs_ != nullptr) {
            SelectVisibleToolTabForPane(type);
        }
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
    } else {
        CancelLayoutMutation();
    }
    return result;
}

DockResult DockHost::SetZoneMode(
    DockZone zone, DockStackMode mode) noexcept {
    if (model_ == nullptr || !BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->SetZoneMode(zone, mode);
    if (result == DockResult::Ok) {
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
    } else {
        CancelLayoutMutation();
    }
    return result;
}

DockResult DockHost::ActivatePane(DockPaneType type) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    if (pane == nullptr || !pane->present || !IsDockedZone(pane->zone)) {
        return DockResult::InvalidState;
    }
    if (!BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockZone zone = pane->zone;
    bool tool_tab_changed{};
    if (zone == DockZone::Right && right_tool_tabs_ != nullptr) {
        const ToolTabId tab = right_tool_tabs_->TabForPane(type);
        if (tab && right_tool_tabs_->IsVisible(tab)) {
            tool_tab_changed = right_tool_tabs_->SetSelected(tab)
                == ToolTabResult::Ok;
        }
    }
    if (model_->StackPaneCount(zone, pane->stack) < 2U) {
        if (tool_tab_changed) {
            return NotifyChanged() ? DockResult::Ok : DockResult::InvalidState;
        }
        CancelLayoutMutation();
        return DockResult::NoOp;
    }
    const DockResult active_result = model_->SetActiveTab(zone, type);
    if (active_result == DockResult::Ok || tool_tab_changed) {
        if (!NotifyChanged()) {
            return DockResult::InvalidState;
        }
    } else {
        CancelLayoutMutation();
    }
    return tool_tab_changed && active_result == DockResult::NoOp
        ? DockResult::Ok
        : active_result;
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
    for (const TabHostState& tabs : tab_states_) {
        if (tabs.zone == zone && tabs.control != nullptr
            && IsWindowVisible(tabs.control) != FALSE) {
            return tabs.control;
        }
    }
    for (const TabHostState& tabs : tab_states_) {
        if (tabs.zone == zone) {
            return tabs.control;
        }
    }
    return nullptr;
}

HWND DockHost::HeaderWindow(DockPaneType type) const noexcept {
    if (model_ == nullptr) {
        return nullptr;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    if (pane == nullptr || !pane->present || !IsDockedZone(pane->zone)) {
        return nullptr;
    }
    const std::uint8_t header_stack = pane->zone == DockZone::Right
            && right_tool_tabs_ != nullptr
        ? static_cast<std::uint8_t>(PaneIndex(type))
        : pane->stack;
    for (const TabHostState& tabs : tab_states_) {
        if (tabs.zone == pane->zone && tabs.stack == header_stack) {
            return tabs.control;
        }
    }
    return nullptr;
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

bool DockHost::ShowAutoHiddenPane(
    DockPaneType type, DockZone edge) noexcept {
    PaneHostState* pane = PaneState(type);
    const DockPanePlacement* placement = model_ == nullptr
        ? nullptr
        : model_->Pane(type);
    if (pane == nullptr || placement == nullptr
        || placement->zone != DockZone::AutoHide
        || (edge != DockZone::Left && edge != DockZone::Right
            && edge != DockZone::Bottom)) {
        return false;
    }
    pane->auto_hide_edge = edge;
    pane->auto_hide_expanded = true;
    ApplyPaneLayout(*pane);
    return pane->floating_window != nullptr
        && IsWindowVisible(pane->floating_window) != FALSE;
}

bool DockHost::AutoHiddenPaneVisible(DockPaneType type) const noexcept {
    const PaneHostState* pane = PaneState(type);
    return pane != nullptr && pane->auto_hide_expanded
        && pane->floating_window != nullptr
        && IsWindowVisible(pane->floating_window) != FALSE;
}

void DockHost::HideAutoHiddenPane(DockPaneType type) noexcept {
    PaneHostState* pane = PaneState(type);
    if (pane == nullptr || !pane->auto_hide_expanded) {
        return;
    }
    pane->auto_hide_expanded = false;
    if (pane->floating_window != nullptr) {
        ShowWindow(pane->floating_window, SW_HIDE);
    }
    if (pane->content != nullptr) {
        ShowWindow(pane->content, SW_HIDE);
    }
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

void DockHost::LayoutAutoHiddenContent(PaneHostState& pane) noexcept {
    const PaneDescriptor* descriptor = FindPaneDescriptor(pane.type);
    if (pane.floating_window == nullptr || descriptor == nullptr
        || owner_ == nullptr) {
        return;
    }
    DockRect available = geometry_.editor;
    if (!HasArea(available)) {
        RECT client{};
        if (GetClientRect(owner_, &client) == FALSE) {
            return;
        }
        available = DockRect{
            0,
            0,
            static_cast<int>(client.right - client.left),
            static_cast<int>(client.bottom - client.top)};
    }
    POINT origin{available.x, available.y};
    if (ClientToScreen(owner_, &origin) == FALSE) {
        return;
    }
    const int client_width = std::max(1, available.width);
    const int client_height = std::max(1, available.height);
    const int preferred_width = std::min(
        client_width, ScaleDip(descriptor->preferred_width_dip, dpi_));
    const int preferred_height = std::min(
        client_height, ScaleDip(descriptor->preferred_height_dip, dpi_));
    RECT bounds{};
    if (pane.auto_hide_edge == DockZone::Left) {
        bounds = RECT{
            origin.x,
            origin.y,
            origin.x + preferred_width,
            origin.y + client_height};
    } else if (pane.auto_hide_edge == DockZone::Bottom) {
        bounds = RECT{
            origin.x,
            origin.y + client_height - preferred_height,
            origin.x + client_width,
            origin.y + client_height};
    } else {
        bounds = RECT{
            origin.x + client_width - preferred_width,
            origin.y,
            origin.x + client_width,
            origin.y + client_height};
    }
    bounds = ClampToVisibleWorkArea(bounds);
    SetWindowPos(
        pane.floating_window,
        nullptr,
        bounds.left,
        bounds.top,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER | SWP_SHOWWINDOW);
    LayoutFloatingContent(pane);
}

bool DockHost::ShouldShowStackHeader(
    DockZone zone, std::uint8_t stack) const noexcept {
    if (model_ == nullptr || !IsDockedZone(zone)) {
        return false;
    }
    const std::size_t count = model_->StackPaneCount(zone, stack);
    if (count > 1U) {
        return true;
    }
    if (count != 1U) {
        return false;
    }
    for (std::size_t index = 0U; index < kDockPaneCount; ++index) {
        const auto type = static_cast<DockPaneType>(index);
        const DockPanePlacement* pane = model_->Pane(type);
        if (pane == nullptr || !pane->present || pane->zone != zone
            || pane->stack != stack) {
            continue;
        }
        const PaneDescriptor* descriptor = FindPaneDescriptor(type);
        return descriptor != nullptr
            && descriptor->show_header_when_singleton;
    }
    return false;
}

bool DockHost::PaneInSelectedToolTab(DockPaneType type) const noexcept {
    if (right_tool_tabs_ == nullptr) {
        return true;
    }
    const ToolTab* selected = right_tool_tabs_->SelectedTab();
    if (selected == nullptr) {
        return false;
    }
    return std::find(
               selected->panes.begin(),
               selected->panes.begin()
                   + static_cast<std::ptrdiff_t>(selected->pane_count),
               type)
        != selected->panes.begin()
            + static_cast<std::ptrdiff_t>(selected->pane_count);
}

void DockHost::SelectVisibleToolTabForPane(DockPaneType type) noexcept {
    if (right_tool_tabs_ == nullptr) {
        return;
    }
    const DockRect right = geometry_.zones[static_cast<std::size_t>(
        DockZone::Right)];
    int available_height = right.height;
    if (available_height <= 0 && owner_ != nullptr) {
        RECT client{};
        if (GetClientRect(owner_, &client) != FALSE) {
            available_height = std::max(
                0L, client.bottom - client.top - ScaleDip(kTabHeightDip, dpi_));
        }
    }
    if (!right_tool_tabs_->TabForPane(type)) {
        DockPanePlacement* placement = model_ == nullptr
            ? nullptr
            : model_->Pane(type);
        const PaneDescriptor* descriptor = FindPaneDescriptor(type);
        if (placement != nullptr && descriptor != nullptr) {
            placement->split_weight = static_cast<std::uint32_t>(
                std::max(1, descriptor->preferred_height_dip));
        }
    }
    static_cast<void>(right_tool_tabs_->AddPaneToSelected(
        type,
        available_height,
        dpi_,
        ScaleDip(4, dpi_)));
    const ToolTabId tab = right_tool_tabs_->TabForPane(type);
    if (tab) {
        static_cast<void>(right_tool_tabs_->SetSelected(tab));
    }
}

void DockHost::RemovePaneFromToolTabs(DockPaneType type) noexcept {
    if (right_tool_tabs_ != nullptr) {
        static_cast<void>(right_tool_tabs_->RemovePane(type));
    }
}

void DockHost::FocusPane(DockPaneType type) noexcept {
    HWND content = ContentWindow(type);
    if (content == nullptr) {
        return;
    }
    HWND target = GetNextDlgTabItem(content, nullptr, FALSE);
    SetFocus(target == nullptr ? content : target);
}

bool DockHost::ShouldShowPaneHeader(DockPaneType type) const noexcept {
    if (model_ == nullptr) {
        return false;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    if (pane == nullptr || !pane->present || !IsDockedZone(pane->zone)) {
        return false;
    }
    if (pane->zone != DockZone::Right || right_tool_tabs_ == nullptr) {
        return ShouldShowStackHeader(pane->zone, pane->stack);
    }
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    return PaneInSelectedToolTab(type) && descriptor != nullptr;
}

bool DockHost::UpdateTabFont(UINT dpi) noexcept {
    const UINT normalized_dpi = dpi == 0U ? 96U : dpi;
    if (tab_font_ != nullptr && tab_font_dpi_ == normalized_dpi) {
        return true;
    }
    const HFONT replacement = CreateFontW(
        -MulDiv(9, static_cast<int>(normalized_dpi), 72),
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
    if (replacement == nullptr) {
        return false;
    }
    for (const TabHostState& tabs : tab_states_) {
        if (tabs.control != nullptr) {
            SendMessageW(
                tabs.control,
                WM_SETFONT,
                reinterpret_cast<WPARAM>(replacement),
                FALSE);
            if (tabs.zone != DockZone::Right) {
                SendMessageW(tabs.control, TCM_SETPADDING, 0,
                    MAKELPARAM(ScaleDip(24, normalized_dpi),
                        ScaleDip(3, normalized_dpi)));
            }
        }
    }
    if (right_tool_tab_control_ != nullptr) {
        SendMessageW(
            right_tool_tab_control_,
            WM_SETFONT,
            reinterpret_cast<WPARAM>(replacement),
            FALSE);
    }
    if (tab_font_ != nullptr) {
        DeleteObject(tab_font_);
    }
    tab_font_ = replacement;
    tab_font_dpi_ = normalized_dpi;
    return true;
}

void DockHost::ApplyPaneLayout(
    PaneHostState& pane, PlacementBatch* placements) noexcept {
    if (model_ == nullptr || pane.content == nullptr) {
        return;
    }
    const auto hide_content = [&pane, placements, this]() noexcept {
        if (placements != nullptr && GetParent(pane.content) == owner_) {
            static_cast<void>(placements->Place(pane.content, {}, false));
        } else {
            ShowWindow(pane.content, SW_HIDE);
        }
    };
    const DockPanePlacement* placement = model_->Pane(pane.type);
    if (placement == nullptr || !placement->present) {
        hide_content();
        return;
    }
    if (placement->zone == DockZone::Floating) {
        pane.auto_hide_expanded = false;
        if (!EnsureFloatingWindow(pane)) {
            hide_content();
            return;
        }
        if (GetParent(pane.content) != pane.floating_window) {
            if (placements != nullptr && GetParent(pane.content) == owner_) {
                placements->IncludeWindow(pane.content);
            }
            SetParent(pane.content, pane.floating_window);
        }
        if (IsWindowVisible(pane.floating_window) == FALSE) {
            const RECT restored = ClampToVisibleWorkArea(RECT{
                ScaleDip(placement->floating.x_dip, dpi_),
                ScaleDip(placement->floating.y_dip, dpi_),
                ScaleDip(
                    placement->floating.x_dip + placement->floating.width_dip,
                    dpi_),
                ScaleDip(
                    placement->floating.y_dip + placement->floating.height_dip,
                    dpi_)});
            SetWindowPos(
                pane.floating_window,
                nullptr,
                restored.left,
                restored.top,
                restored.right - restored.left,
                restored.bottom - restored.top,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
            ShowWindow(pane.floating_window, SW_SHOWNOACTIVATE);
        }
        LayoutFloatingContent(pane);
        return;
    }
    if (placement->zone == DockZone::AutoHide) {
        if (!pane.auto_hide_expanded) {
            if (pane.floating_window != nullptr) {
                ShowWindow(pane.floating_window, SW_HIDE);
            }
            hide_content();
            return;
        }
        if (!EnsureFloatingWindow(pane)) {
            hide_content();
            return;
        }
        if (GetParent(pane.content) != pane.floating_window) {
            if (placements != nullptr && GetParent(pane.content) == owner_) {
                placements->IncludeWindow(pane.content);
            }
            SetParent(pane.content, pane.floating_window);
        }
        LayoutAutoHiddenContent(pane);
        return;
    }
    pane.auto_hide_expanded = false;
    if (pane.floating_window != nullptr) {
        ShowWindow(pane.floating_window, SW_HIDE);
    }
    if (placement->zone == DockZone::Hidden) {
        hide_content();
        return;
    }
    if (GetParent(pane.content) != owner_) {
        SetParent(pane.content, owner_);
    }
    DockPaneGeometry geometry = geometry_.panes[PaneIndex(pane.type)];
    if (ShouldShowPaneHeader(pane.type) && geometry.shown) {
        const int tab_height = std::min(
            geometry.bounds.height, ScaleDip(kTabHeightDip, dpi_));
        geometry.bounds.y += tab_height;
        geometry.bounds.height -= tab_height;
    }
    if (placements != nullptr) {
        static_cast<void>(placements->Place(
            pane.content,
            geometry.bounds,
            geometry.shown && !geometry.temporarily_auto_hidden));
    } else {
        PlaceWindow(
            pane.content,
            geometry.bounds,
            geometry.shown && !geometry.temporarily_auto_hidden);
    }
}

void DockHost::ApplyTabLayout(
    TabHostState& tabs,
    bool synchronize_items,
    PlacementBatch& placements) noexcept {
    if (model_ == nullptr || !IsDockedZone(tabs.zone)) {
        return;
    }
    if (tabs.zone == DockZone::Right && right_tool_tabs_ != nullptr) {
        const auto type = static_cast<DockPaneType>(tabs.stack);
        const DockPanePlacement* placement = model_->Pane(type);
        const PaneDescriptor* descriptor = FindPaneDescriptor(type);
        const DockPaneGeometry& pane_geometry = geometry_.panes[PaneIndex(type)];
        const bool show = placement != nullptr && placement->present
            && placement->zone == DockZone::Right && pane_geometry.shown
            && !pane_geometry.temporarily_auto_hidden
            && PaneInSelectedToolTab(type) && descriptor != nullptr;
        if (!show) {
            static_cast<void>(placements.Place(tabs.control, {}, false));
            return;
        }
        if (synchronize_items) {
            TabCtrl_DeleteAllItems(tabs.control);
            wchar_t title[128]{};
            TCITEMW item{};
            item.mask = TCIF_TEXT | TCIF_PARAM;
            item.pszText = const_cast<wchar_t*>(
                LoadPaneTitle(instance_, *descriptor, title));
            item.lParam = static_cast<LPARAM>(type);
            TabCtrl_InsertItem(tabs.control, 0, &item);
            TabCtrl_SetCurSel(tabs.control, 0);
        }
        DockRect bounds = pane_geometry.bounds;
        bounds.height = std::min(bounds.height, ScaleDip(kTabHeightDip, dpi_));
        static_cast<void>(placements.Place(tabs.control, bounds, true));
        return;
    }
    const bool show = ShouldShowStackHeader(tabs.zone, tabs.stack)
        && HasArea(geometry_.zones[static_cast<std::size_t>(tabs.zone)]);
    if (!show) {
        static_cast<void>(placements.Place(tabs.control, {}, false));
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
        if (pane != nullptr && pane->present && pane->zone == tabs.zone
            && pane->stack == tabs.stack) {
            ordered[count++] = OrderedPane{pane->tab_order, type};
        }
    }
    std::sort(
        ordered.begin(),
        ordered.begin() + static_cast<std::ptrdiff_t>(count),
        [](const OrderedPane& left, const OrderedPane& right) {
            return left.order < right.order;
        });
    if (synchronize_items) {
        TabCtrl_DeleteAllItems(tabs.control);
        int selected{};
        for (std::size_t index = 0U; index < count; ++index) {
            const PaneDescriptor* descriptor = FindPaneDescriptor(
                ordered[index].type);
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
            const DockPanePlacement* pane = model_->Pane(ordered[index].type);
            if (pane != nullptr && pane->active_tab) {
                selected = static_cast<int>(index);
            }
        }
        TabCtrl_SetCurSel(tabs.control, selected);
    }
    DockRect bounds{};
    if (count > 0U) {
        bounds = geometry_.panes[PaneIndex(ordered[0].type)].bounds;
    }
    bounds.height = std::min(bounds.height, ScaleDip(kTabHeightDip, dpi_));
    static_cast<void>(placements.Place(tabs.control, bounds, true));
}

void DockHost::LayoutPaneTabCloseButtons(TabHostState& tabs) noexcept {
    if (tabs.control == nullptr || model_ == nullptr || tabs.zone == DockZone::Right) {
        return;
    }
    RECT client{};
    if (GetClientRect(tabs.control, &client) == FALSE) {
        return;
    }
    const int button_size = std::max(1, ScaleDip(20, dpi_));
    const int edge = std::max(1, ScaleDip(3, dpi_));
    bool geometry_changed{};
    for (PaneHostState& pane : panes_) {
        const DockPanePlacement* placement = model_->Pane(pane.type);
        const bool belongs = placement != nullptr && placement->present
            && placement->zone == tabs.zone && placement->stack == tabs.stack;
        if (!belongs
            && (pane.tab_close_button == nullptr
                || GetParent(pane.tab_close_button) != tabs.control)) {
            continue;
        }
        RECT item{};
        bool item_found{};
        if (belongs) {
            const int count = std::max(0, TabCtrl_GetItemCount(tabs.control));
            for (int index = 0; index < count; ++index) {
                TCITEMW candidate{};
                candidate.mask = TCIF_PARAM;
                if (TabCtrl_GetItem(tabs.control, index, &candidate) != FALSE
                    && static_cast<DockPaneType>(candidate.lParam) == pane.type) {
                    item_found = TabCtrl_GetItemRect(tabs.control, index, &item) != FALSE;
                    break;
                }
            }
        }
        const bool show = item_found
            && (GetWindowLongPtrW(tabs.control, GWL_STYLE) & WS_VISIBLE) != 0
            && item.left >= client.left && item.right <= client.right
            && item.top >= client.top && item.bottom <= client.bottom
            && item.right - item.left >= button_size + edge * 2;
        if (!show) {
            pane.close_hovered = false;
        }
        if (item_found && pane.tab_close_button == nullptr) {
            const HWND button = CreateWindowExW(
                0, WC_BUTTONW, UiText(UiStringId::DockClose),
                WS_CHILD | WS_TABSTOP | BS_OWNERDRAW | BS_FLAT,
                0, 0, 0, 0, tabs.control,
                reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_RIGHT_TOOL_TAB_CLOSE)),
                instance_, nullptr);
            if (button == nullptr) {
                continue;
            }
            if (SetWindowSubclass(button, PaneTabCloseButtonSubclassProcedure,
                    kPaneTabCloseButtonSubclass,
                    reinterpret_cast<DWORD_PTR>(&pane)) == FALSE) {
                DestroyWindow(button);
                continue;
            }
            pane.tab_close_button = button;
        }
        if (pane.tab_close_button == nullptr) {
            continue;
        }
        if (item_found && GetParent(pane.tab_close_button) != tabs.control) {
            SetParent(pane.tab_close_button, tabs.control);
        }
        const int size = std::min(button_size,
            std::max(1, static_cast<int>(item.bottom - item.top) - edge * 2));
        const DockRect bounds{item.right - edge - size,
            item.top + std::max(0, static_cast<int>(item.bottom - item.top) - size) / 2,
            size, size};
        if (!WindowMatchesPlacement(pane.tab_close_button, bounds, show)) {
            SetWindowPos(pane.tab_close_button, HWND_TOP,
                bounds.x, bounds.y, bounds.width, bounds.height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOREDRAW
                    | SWP_NOCOPYBITS
                    | (show ? SWP_SHOWWINDOW : SWP_HIDEWINDOW));
            geometry_changed = true;
        }
    }
    if (geometry_changed && !applying_) {
        RedrawWindow(tabs.control, nullptr, nullptr,
            RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW | RDW_ALLCHILDREN);
    }
}

bool DockHost::DrawPaneTabCloseButton(const DRAWITEMSTRUCT& draw) noexcept {
    const auto pane = std::find_if(panes_.begin(), panes_.end(),
        [&draw](const PaneHostState& candidate) {
            return candidate.tab_close_button == draw.hwndItem;
        });
    if (pane == panes_.end()) {
        return false;
    }
    const DockPanePlacement* placement = model_ == nullptr ? nullptr : model_->Pane(pane->type);
    const bool active = placement != nullptr && placement->active_tab;
    PaintTabCloseButton(draw, active, pane->close_hovered);
    return true;
}

void DockHost::ApplyToolTabLayout(
    bool synchronize_items, PlacementBatch& placements) noexcept {
    if (right_tool_tab_control_ == nullptr || right_tool_tabs_ == nullptr) {
        return;
    }
    const int visible_count = static_cast<int>(right_tool_tabs_->Tabs().size());
    const int horizontal_padding = std::max(1, ScaleDip(24, dpi_));
    const int vertical_padding = std::max(1, ScaleDip(3, dpi_));
    SendMessageW(
        right_tool_tab_control_,
        TCM_SETPADDING,
        0,
        MAKELPARAM(horizontal_padding, vertical_padding));
    if (synchronize_items) {
        TabCtrl_DeleteAllItems(right_tool_tab_control_);
        int selected = -1;
        int visible_index{};
        for (const ToolTab& tab : right_tool_tabs_->Tabs()) {
            wchar_t title[128]{};
            TCITEMW item{};
            item.mask = TCIF_TEXT | TCIF_PARAM;
            item.pszText = const_cast<wchar_t*>(
                LoadToolTabTitle(instance_, tab, title));
            item.lParam = static_cast<LPARAM>(tab.id.Value());
            TabCtrl_InsertItem(right_tool_tab_control_, visible_index, &item);
            if (tab.id == right_tool_tabs_->Selected()) {
                selected = visible_index;
            }
            ++visible_index;
        }
        TabCtrl_SetCurSel(right_tool_tab_control_, selected);
        if (const ToolTab* active = right_tool_tabs_->SelectedTab(); active != nullptr) {
            std::array<wchar_t, kMaximumToolTabDescriptionLength> description{};
            if (LoadToolTabDescription(instance_, *active, description)) {
                static_cast<void>(SetAccessibleName(
                    right_tool_tab_control_, description.data()));
            }
        }
        static_cast<void>(SynchronizeToolTabCloseButtons());
    }
    static_cast<void>(placements.Place(
        right_tool_tab_control_,
        geometry_.right_tool_tabs,
        visible_count > 0));
}

DockHost::ToolTabCloseButtonSlot* DockHost::FindToolTabCloseButton(
    ToolTabId tab) noexcept {
    const auto found = std::find_if(
        tool_tab_close_buttons_.begin(),
        tool_tab_close_buttons_.end(),
        [tab](const ToolTabCloseButtonSlot& slot) { return slot.tab == tab; });
    return found == tool_tab_close_buttons_.end() ? nullptr : &*found;
}

DockHost::ToolTabCloseButtonSlot* DockHost::FindToolTabCloseButton(
    HWND button) noexcept {
    const auto found = std::find_if(
        tool_tab_close_buttons_.begin(),
        tool_tab_close_buttons_.end(),
        [button](const ToolTabCloseButtonSlot& slot) {
            return slot.button == button;
        });
    return found == tool_tab_close_buttons_.end() ? nullptr : &*found;
}

HWND DockHost::CreateToolTabCloseButton(
    ToolTabCloseButtonSlot& slot) noexcept {
    const HWND button = CreateWindowExW(
        0,
        WC_BUTTONW,
        UiText(UiStringId::DockClose),
        WS_CHILD | WS_TABSTOP | BS_OWNERDRAW | BS_FLAT,
        0,
        0,
        0,
        0,
        right_tool_tab_control_,
        reinterpret_cast<HMENU>(
            static_cast<INT_PTR>(IDC_RIGHT_TOOL_TAB_CLOSE)),
        instance_,
        nullptr);
    if (button == nullptr
        || SetWindowSubclass(
               button,
               ToolTabCloseButtonSubclassProcedure,
               kToolTabCloseButtonSubclass,
               reinterpret_cast<DWORD_PTR>(&slot)) == FALSE) {
        if (button != nullptr) {
            DestroyWindow(button);
        }
        return nullptr;
    }
    return button;
}

void DockHost::DestroyToolTabCloseButton(
    ToolTabCloseButtonSlot& slot) noexcept {
    const HWND button = slot.button;
    slot.tab = {};
    slot.hovered = false;
    if (button != nullptr && IsWindow(button) != FALSE) {
        DestroyWindow(button);
    }
    slot.button = nullptr;
}

bool DockHost::SynchronizeToolTabCloseButtons() noexcept {
    std::array<bool, kMaximumToolTabs> retained{};
    bool complete = right_tool_tabs_ != nullptr
        && right_tool_tabs_->Tabs().size() <= retained.size();
    if (right_tool_tabs_ != nullptr) {
        for (const ToolTab& tab : right_tool_tabs_->Tabs()) {
            ToolTabCloseButtonSlot* slot = FindToolTabCloseButton(tab.id);
            if (slot == nullptr) {
                const auto available = std::find_if(
                    tool_tab_close_buttons_.begin(),
                    tool_tab_close_buttons_.end(),
                    [](const ToolTabCloseButtonSlot& candidate) {
                        return !candidate.tab && candidate.button == nullptr;
                    });
                if (available == tool_tab_close_buttons_.end()) {
                    complete = false;
                    continue;
                }
                slot = &*available;
                slot->tab = tab.id;
                slot->button = CreateToolTabCloseButton(*slot);
                if (slot->button == nullptr) {
                    slot->tab = {};
                    complete = false;
                    continue;
                }
            }
            retained[static_cast<std::size_t>(
                slot - tool_tab_close_buttons_.data())] = true;
        }
    }
    for (std::size_t index = 0U; index < tool_tab_close_buttons_.size(); ++index) {
        if (!retained[index] && tool_tab_close_buttons_[index].tab) {
            DestroyToolTabCloseButton(tool_tab_close_buttons_[index]);
        }
    }
    return complete;
}

void DockHost::LayoutToolTabCloseButtons() noexcept {
    if (right_tool_tab_control_ == nullptr) {
        return;
    }
    RECT client{};
    if (GetClientRect(right_tool_tab_control_, &client) == FALSE) {
        return;
    }
    const int button_size = std::max(1, ScaleDip(20, dpi_));
    const int edge = std::max(1, ScaleDip(3, dpi_));
    const int minimum_item_width = button_size + edge * 2;
    bool geometry_changed{};
    for (ToolTabCloseButtonSlot& slot : tool_tab_close_buttons_) {
        if (slot.button == nullptr || !slot.tab) {
            continue;
        }
        int tab_index = -1;
        const int count = std::max(0, TabCtrl_GetItemCount(right_tool_tab_control_));
        for (int index = 0; index < count; ++index) {
            if (ToolTabAt(right_tool_tab_control_, index) == slot.tab) {
                tab_index = index;
                break;
            }
        }
        RECT item{};
        const bool item_available = tab_index >= 0
            && TabCtrl_GetItemRect(right_tool_tab_control_, tab_index, &item)
                != FALSE;
        const bool fully_visible = item_available
            && item.left >= client.left && item.right <= client.right
            && item.top >= client.top && item.bottom <= client.bottom
            && item.right - item.left >= minimum_item_width;
        if (!fully_visible) {
            if (!WindowMatchesPlacement(slot.button, {}, false)) {
                SetWindowPos(
                    slot.button,
                    nullptr,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER
                        | SWP_NOREDRAW | SWP_NOCOPYBITS | SWP_HIDEWINDOW);
                geometry_changed = true;
            }
            continue;
        }
        const int item_height = item.bottom - item.top;
        const int size = std::min(
            button_size, std::max(1, item_height - edge * 2));
        const int x = item.right - edge - size;
        const int y = item.top + std::max(0, (item_height - size) / 2);
        const DockRect bounds{x, y, size, size};
        if (!WindowMatchesPlacement(slot.button, bounds, true)) {
            SetWindowPos(
                slot.button,
                HWND_TOP,
                x,
                y,
                size,
                size,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOREDRAW
                    | SWP_NOCOPYBITS | SWP_SHOWWINDOW);
            geometry_changed = true;
        }
    }
    if (geometry_changed && !applying_) {
        RedrawWindow(
            right_tool_tab_control_,
            nullptr,
            nullptr,
            RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW | RDW_ALLCHILDREN);
    }
}

bool DockHost::DrawToolTabCloseButton(
    const DRAWITEMSTRUCT& draw) noexcept {
    ToolTabCloseButtonSlot* slot = FindToolTabCloseButton(draw.hwndItem);
    if (slot == nullptr) {
        return false;
    }
    const int selected = TabCtrl_GetCurSel(right_tool_tab_control_);
    const bool active = selected >= 0
        && ToolTabAt(right_tool_tab_control_, selected) == slot->tab;
    PaintTabCloseButton(draw, active, slot->hovered);
    return true;
}

bool DockHost::BeginLayoutMutation() noexcept {
    if (layout_mutation_pending_ || model_ == nullptr
        || right_tool_tabs_ == nullptr) {
        return false;
    }
    CommitLayoutState();
    layout_mutation_pending_ = true;
    return true;
}

void DockHost::CancelLayoutMutation() noexcept {
    // A compound DockHost command can mutate the right-tab projection before
    // a later DockLayout primitive rejects the same command. Cancellation must
    // therefore restore the complete snapshot; merely dropping the pending bit
    // would publish the successful prefix of a failed mutation.
    RestoreLayoutMutation();
}

void DockHost::RestoreLayoutMutation() noexcept {
    if (!layout_mutation_pending_ || !committed_layout_state_valid_
        || model_ == nullptr || right_tool_tabs_ == nullptr) {
        layout_mutation_pending_ = false;
        return;
    }
    *model_ = committed_model_;
    *right_tool_tabs_ = committed_right_tool_tabs_;
    for (std::size_t index = 0U; index < panes_.size(); ++index) {
        panes_[index].auto_hide_expanded = committed_auto_hide_expanded_[index];
        panes_[index].auto_hide_edge = committed_auto_hide_edges_[index];
    }
    layout_mutation_pending_ = false;
}

void DockHost::CommitLayoutState() noexcept {
    if (model_ == nullptr || right_tool_tabs_ == nullptr) {
        layout_mutation_pending_ = false;
        return;
    }
    committed_model_ = *model_;
    committed_right_tool_tabs_ = *right_tool_tabs_;
    for (std::size_t index = 0U; index < panes_.size(); ++index) {
        committed_auto_hide_expanded_[index] = panes_[index].auto_hide_expanded;
        committed_auto_hide_edges_[index] = panes_[index].auto_hide_edge;
    }
    committed_layout_state_valid_ = true;
    layout_mutation_pending_ = false;
}

bool DockHost::NotifyChanged(DockHostChangeKind kind) noexcept {
    if (applying_) {
        return false;
    }
    if (changed_ == nullptr) {
        CommitLayoutState();
        return true;
    }
    const bool committed = changed_(changed_context_, kind);
    if (!committed && layout_mutation_pending_) {
        RestoreLayoutMutation();
    } else if (committed && layout_mutation_pending_) {
        // A non-visual test callback may accept the mutation without calling
        // ApplyLayout. Treat that synchronous acknowledgement as the commit.
        CommitLayoutState();
    }
    return committed;
}

void DockHost::ShowContextMenu(
    DockPaneType type, POINT screen) noexcept {
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    if (model_ == nullptr || descriptor == nullptr) {
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
    if (type == DockPaneType::Tool) {
        HMENU menu = CreatePopupMenu();
        if (menu == nullptr) {
            return;
        }
        AppendMenuW(menu, MF_STRING, kContextClose, UiText(UiStringId::DockClose));
        const UINT command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            screen.x,
            screen.y,
            0,
            owner_,
            nullptr);
        DestroyMenu(menu);
        if (command == kContextClose) {
            static_cast<void>(HidePane(type));
        }
        return;
    }
    HMENU menu = CreatePopupMenu();
    HMENU move_menu = CreatePopupMenu();
    if (menu == nullptr || move_menu == nullptr) {
        if (menu != nullptr) {
            DestroyMenu(menu);
        }
        if (move_menu != nullptr) {
            DestroyMenu(move_menu);
        }
        return;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    std::array<ToolTabId, kMaximumToolTabs> destinations{};
    std::size_t destination_count{};
    const ToolTabId current_tab = right_tool_tabs_ == nullptr
        ? ToolTabId{}
        : right_tool_tabs_->TabForPane(type);
    if (right_tool_tabs_ != nullptr) {
        for (const ToolTab& tab : right_tool_tabs_->Tabs()) {
            const UINT command = kContextMoveFirst
                + static_cast<UINT>(destination_count);
            const UINT flags = MF_STRING
                | (tab.id == current_tab ? MF_CHECKED | MF_GRAYED : 0U);
            wchar_t title[128]{};
            AppendMenuW(
                move_menu,
                flags,
                command,
                LoadToolTabTitle(instance_, tab, title));
            destinations[destination_count++] = tab.id;
        }
    }
    AppendMenuW(
        menu,
        MF_POPUP | (model_->IsZoneAllowed(type, DockZone::Right)
                           ? MF_ENABLED
                           : MF_GRAYED),
        reinterpret_cast<UINT_PTR>(move_menu),
        UiText(UiStringId::DockMovePaneToTab));
    AppendMenuW(
        move_menu,
        MF_STRING,
        kContextMoveToNewTab,
        UiText(UiStringId::Text0706));
    AppendMenuW(menu, MF_SEPARATOR, 0U, nullptr);
    AppendMenuW(
        menu,
        MF_STRING
            | (pane != nullptr && pane->zone == DockZone::Floating ? MF_CHECKED : 0U),
        kContextFloat,
        UiText(UiStringId::DockFloating));
    if (!descriptor->can_float) {
        EnableMenuItem(menu, kContextFloat, MF_BYCOMMAND | MF_GRAYED);
    }
    AppendMenuW(menu, MF_STRING, kContextClose, UiText(UiStringId::DockClose));
    const UINT command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        screen.x,
        screen.y,
        0,
        owner_,
        nullptr);
    DestroyMenu(menu);
    if (command >= kContextMoveFirst
        && command < kContextMoveFirst + destination_count) {
        static_cast<void>(MovePaneToToolTab(
            type, destinations[command - kContextMoveFirst]));
    } else if (command == kContextMoveToNewTab) {
        static_cast<void>(MovePaneToNewToolTab(type));
    } else if (command == kContextFloat) {
        static_cast<void>(FloatPane(type));
    } else if (command == kContextClose) {
        static_cast<void>(HidePane(type));
    }
}

ToolTabResult DockHost::MovePaneToToolTab(
    DockPaneType type, ToolTabId destination) noexcept {
    if (model_ == nullptr || right_tool_tabs_ == nullptr
        || right_tool_tabs_->Find(destination) == nullptr
        || !model_->IsZoneAllowed(type, DockZone::Right)) {
        return ToolTabResult::InvalidTab;
    }
    if (!BeginLayoutMutation()) {
        return ToolTabResult::InvalidPane;
    }
    const ToolTabId previous = right_tool_tabs_->TabForPane(type);
    const ToolTabResult move_result = right_tool_tabs_->MovePane(
        type, destination);
    if (move_result != ToolTabResult::Ok) {
        CancelLayoutMutation();
        return move_result;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    if (pane == nullptr || pane->zone != DockZone::Right) {
        const DockResult dock_result = model_->MovePane(type, DockZone::Right);
        if (dock_result != DockResult::Ok && dock_result != DockResult::NoOp) {
            if (previous) {
                static_cast<void>(right_tool_tabs_->MovePane(type, previous));
            }
            CancelLayoutMutation();
            return ToolTabResult::InvalidPane;
        }
    }
    static_cast<void>(right_tool_tabs_->SetSelected(destination));
    if (!NotifyChanged()) {
        return ToolTabResult::InvalidPane;
    }
    FocusPane(type);
    return ToolTabResult::Ok;
}

ToolTabResult DockHost::MovePaneToNewToolTab(
    DockPaneType type) noexcept {
    if (model_ == nullptr || right_tool_tabs_ == nullptr
        || !model_->IsZoneAllowed(type, DockZone::Right)) {
        return ToolTabResult::InvalidPane;
    }
    if (!BeginLayoutMutation()) {
        return ToolTabResult::InvalidPane;
    }
    RightToolTabsModel original = *right_tool_tabs_;
    const ToolTabResult result = right_tool_tabs_->MovePaneToNewTab(type);
    if (result != ToolTabResult::Ok) {
        CancelLayoutMutation();
        return result;
    }
    const DockPanePlacement* pane = model_->Pane(type);
    if (pane == nullptr || pane->zone != DockZone::Right) {
        const DockResult dock_result = model_->MovePane(type, DockZone::Right);
        if (dock_result != DockResult::Ok && dock_result != DockResult::NoOp) {
            *right_tool_tabs_ = original;
            CancelLayoutMutation();
            return ToolTabResult::InvalidPane;
        }
    }
    if (!NotifyChanged()) {
        return ToolTabResult::InvalidPane;
    }
    FocusPane(type);
    return ToolTabResult::Ok;
}

ToolTabResult DockHost::CloseToolTab(ToolTabId tab) noexcept {
    if (model_ == nullptr || right_tool_tabs_ == nullptr) {
        return ToolTabResult::InvalidTab;
    }
    DockLayoutModel dock_candidate = *model_;
    RightToolTabsModel tab_candidate = *right_tool_tabs_;
    std::array<DockPaneType, kDockPaneCount> closed_panes{};
    std::size_t closed_count{};
    const ToolTabResult close_result = tab_candidate.CloseTab(
        tab, closed_panes, closed_count);
    if (close_result != ToolTabResult::Ok) {
        return close_result;
    }
    for (std::size_t index = 0U; index < closed_count; ++index) {
        if (dock_candidate.HidePane(closed_panes[index]) != DockResult::Ok) {
            return ToolTabResult::InvalidPane;
        }
    }
    if (!BeginLayoutMutation()) {
        return ToolTabResult::InvalidPane;
    }
    *model_ = dock_candidate;
    *right_tool_tabs_ = tab_candidate;
    for (std::size_t index = 0U; index < closed_count; ++index) {
        if (PaneHostState* pane = PaneState(closed_panes[index]); pane != nullptr) {
            pane->auto_hide_expanded = false;
        }
    }
    return NotifyChanged() ? ToolTabResult::Ok : ToolTabResult::InvalidPane;
}

std::array<DockPaneType, kDockPaneCount>
DockHost::SelectedRightDockedPanes(std::size_t& count) const noexcept {
    std::array<DockPaneType, kDockPaneCount> output{};
    count = 0U;
    if (model_ == nullptr || right_tool_tabs_ == nullptr) {
        return output;
    }
    const ToolTab* selected = right_tool_tabs_->SelectedTab();
    if (selected == nullptr) {
        return output;
    }
    for (std::size_t index = 0U;
         index < selected->pane_count && count < output.size();
         ++index) {
        const DockPaneType type = selected->panes[index];
        const DockPanePlacement* pane = model_->Pane(type);
        if (pane != nullptr && pane->present && pane->zone == DockZone::Right) {
            output[count++] = type;
        }
    }
    return output;
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
        if (!BeginLayoutMutation()) {
            return;
        }
        const DockResult result = model_->MovePane(pane.type, target);
        if (result == DockResult::Ok) {
            if (target == DockZone::Right && right_tool_tabs_ != nullptr) {
                SelectVisibleToolTabForPane(pane.type);
            }
            static_cast<void>(NotifyChanged());
        } else {
            CancelLayoutMutation();
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
    if (!BeginLayoutMutation()) {
        return;
    }
    placement->floating = DockFloatingPlacement{
        PixelsToDip(bounds.left, dpi),
        PixelsToDip(bounds.top, dpi),
        PixelsToDip(bounds.right - bounds.left, dpi),
        PixelsToDip(bounds.bottom - bounds.top, dpi)};
    static_cast<void>(NotifyChanged(DockHostChangeKind::Geometry));
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
    if (!BeginLayoutMutation()) {
        return;
    }
    const DockResult result = model_->SetZoneExtentDip(
        splitter.geometry.zone, PixelsToDip(std::max(1, extent), dpi_));
    if (result == DockResult::Ok) {
        static_cast<void>(NotifyChanged(DockHostChangeKind::Geometry));
    } else {
        CancelLayoutMutation();
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
    splitter.last_screen = screen;
    DockResult result = DockResult::InvalidState;
    if (splitter.geometry.zone == DockZone::Right
        && right_tool_tabs_ != nullptr) {
        std::size_t count{};
        const auto panes = SelectedRightDockedPanes(count);
        const std::size_t boundary = splitter.geometry.boundary;
        if (boundary + 1U < count) {
            const DockPaneGeometry& first_geometry = geometry_.panes[
                PaneIndex(panes[boundary])];
            const DockPaneGeometry& second_geometry = geometry_.panes[
                PaneIndex(panes[boundary + 1U])];
            const int pair_extent = std::max(
                1,
                first_geometry.bounds.height
                    + second_geometry.bounds.height);
            int delta_milli = static_cast<int>(
                static_cast<std::int64_t>(delta) * 1000 / pair_extent);
            if (delta_milli == 0 && delta != 0) {
                delta_milli = delta < 0 ? -1 : 1;
            }
            result = AdjustRightPaneBoundary(
                panes[boundary], panes[boundary + 1U], delta_milli);
        }
    } else {
        if (!BeginLayoutMutation()) {
            return;
        }
        const DockRect zone = geometry_.zones[static_cast<std::size_t>(
            splitter.geometry.zone)];
        const int extent = std::max(1, horizontal ? zone.width : zone.height);
        const int delta_milli = delta * 1000 / extent;
        result = model_->AdjustSplitBoundary(
            splitter.geometry.zone,
            splitter.geometry.boundary,
            delta_milli);
    }
    if (result == DockResult::Ok) {
        static_cast<void>(NotifyChanged(DockHostChangeKind::StackBoundary));
    } else if (layout_mutation_pending_) {
        CancelLayoutMutation();
    }
}

DockResult DockHost::AdjustRightPaneBoundary(
    DockPaneType first,
    DockPaneType second,
    int delta_milli) noexcept {
    if (model_ == nullptr) {
        return DockResult::InvalidState;
    }
    const DockPaneGeometry& first_geometry = geometry_.panes[PaneIndex(first)];
    const DockPaneGeometry& second_geometry = geometry_.panes[PaneIndex(second)];
    const PaneDescriptor* first_descriptor = FindPaneDescriptor(first);
    const PaneDescriptor* second_descriptor = FindPaneDescriptor(second);
    if (first_descriptor == nullptr || second_descriptor == nullptr) {
        return DockResult::InvalidState;
    }
    if ((delta_milli < 0
            && first_geometry.bounds.height
                <= ScaleDip(first_descriptor->minimum_height_dip, dpi_))
        || (delta_milli > 0
            && second_geometry.bounds.height
                <= ScaleDip(second_descriptor->minimum_height_dip, dpi_))) {
        return DockResult::NoOp;
    }
    const int available_extent_pixels = first_geometry.bounds.height
        + second_geometry.bounds.height;
    if (available_extent_pixels <= 0) {
        return DockResult::InvalidState;
    }
    if (!BeginLayoutMutation()) {
        return DockResult::InvalidState;
    }
    const DockResult result = model_->AdjustPaneBoundary(
        first,
        second,
        delta_milli,
        PixelsToDip(available_extent_pixels, dpi_));
    if (result != DockResult::Ok) {
        CancelLayoutMutation();
    }
    return result;
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
    if (!BeginLayoutMutation()) {
        return;
    }
    const DockResult result = model_->SetActiveTab(
        tabs.zone, static_cast<DockPaneType>(item.lParam));
    if (result == DockResult::Ok) {
        static_cast<void>(NotifyChanged());
    } else {
        CancelLayoutMutation();
    }
}

void DockHost::ActivateSelectedToolTab() noexcept {
    if (applying_ || right_tool_tabs_ == nullptr
        || right_tool_tab_control_ == nullptr) {
        return;
    }
    const int selected = TabCtrl_GetCurSel(right_tool_tab_control_);
    if (selected < 0) {
        return;
    }
    TCITEMW item{};
    item.mask = TCIF_PARAM;
    if (TabCtrl_GetItem(right_tool_tab_control_, selected, &item) == FALSE) {
        return;
    }
    if (!BeginLayoutMutation()) {
        return;
    }
    const ToolTabResult result = right_tool_tabs_->SetSelected(
        ToolTabId{static_cast<std::uint32_t>(item.lParam)});
    if (result == ToolTabResult::Ok) {
        static_cast<void>(NotifyChanged());
    } else {
        CancelLayoutMutation();
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
        case WM_SYSCOLORCHANGE:
        case WM_SETTINGCHANGE:
            RedrawWindow(
                window,
                nullptr,
                nullptr,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
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
            InvalidateRect(window, nullptr, FALSE);
            return 0;
        case WM_MOUSEMOVE:
            if (!splitter->hovered) {
                TRACKMOUSEEVENT tracking{};
                tracking.cbSize = sizeof(tracking);
                tracking.dwFlags = TME_LEAVE;
                tracking.hwndTrack = window;
                splitter->hovered = TrackMouseEvent(&tracking) != FALSE;
                InvalidateRect(window, nullptr, FALSE);
            }
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
        case WM_MOUSELEAVE:
            splitter->hovered = false;
            InvalidateRect(window, nullptr, FALSE);
            return 0;
        case WM_LBUTTONUP:
            if (GetCapture() == window) {
                ReleaseCapture();
            }
            InvalidateRect(window, nullptr, FALSE);
            return 0;
        case WM_CAPTURECHANGED:
        case WM_THEMECHANGED:
        case WM_SYSCOLORCHANGE:
        case WM_SETTINGCHANGE:
            InvalidateRect(window, nullptr, FALSE);
            break;
        case WM_SETFOCUS:
            splitter->focused = true;
            RedrawSplitterNow(window);
            break;
        case WM_KILLFOCUS:
            splitter->focused = false;
            RedrawSplitterNow(window);
            break;
        case WM_CANCELMODE:
            if (GetCapture() == window) {
                ReleaseCapture();
            }
            InvalidateRect(window, nullptr, FALSE);
            return 0;
        case WM_ERASEBKGND:
            return 1;
        case WM_PAINT:
            PaintSplitter(
                window,
                splitter->geometry,
                splitter->hovered || GetCapture() == window
                    || splitter->focused,
                splitter->focused);
            return 0;
        case WM_GETDLGCODE:
            return DefSubclassProc(window, message, wparam, lparam)
                | DLGC_WANTARROWS;
        case WM_KEYDOWN: {
            const int direction = wparam == VK_LEFT || wparam == VK_UP
                ? -1
                : (wparam == VK_RIGHT || wparam == VK_DOWN ? 1 : 0);
            if (direction == 0 || splitter->host->model_ == nullptr) {
                break;
            }
            DockResult result = DockResult::NoOp;
            if (splitter->geometry.kind == DockSplitterKind::StackBoundary) {
                if (splitter->geometry.zone == DockZone::Right
                    && splitter->host->right_tool_tabs_ != nullptr) {
                    std::size_t count{};
                    const auto panes =
                        splitter->host->SelectedRightDockedPanes(count);
                    const std::size_t boundary = splitter->geometry.boundary;
                    if (boundary + 1U < count) {
                        result = splitter->host->AdjustRightPaneBoundary(
                            panes[boundary],
                            panes[boundary + 1U],
                            direction * 20);
                    }
                } else {
                    if (!splitter->host->BeginLayoutMutation()) {
                        return 0;
                    }
                    result = splitter->host->model_->AdjustSplitBoundary(
                        splitter->geometry.zone,
                        splitter->geometry.boundary,
                        direction * 20);
                }
            } else {
                const DockZoneState* zone = splitter->host->model_->Zone(
                    splitter->geometry.zone);
                if (zone != nullptr) {
                    if (!splitter->host->BeginLayoutMutation()) {
                        return 0;
                    }
                    result = splitter->host->model_->SetZoneExtentDip(
                        splitter->geometry.zone,
                        zone->extent_dip + direction * 4);
                }
            }
            if (result == DockResult::Ok) {
                static_cast<void>(splitter->host->NotifyChanged(
                    splitter->geometry.kind == DockSplitterKind::StackBoundary
                        ? DockHostChangeKind::StackBoundary
                        : DockHostChangeKind::Geometry));
            } else if (splitter->host->layout_mutation_pending_) {
                splitter->host->CancelLayoutMutation();
            }
            return 0;
        }
        case WM_SETCURSOR: {
            const bool horizontal_line =
                SplitterHasHorizontalLine(splitter->geometry);
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
    if (tabs != nullptr && tabs->host != nullptr) {
        DockHost& host = *tabs->host;
        if (message == WM_COMMAND && LOWORD(wparam) == IDC_RIGHT_TOOL_TAB_CLOSE
            && HIWORD(wparam) == BN_CLICKED) {
            const HWND button = reinterpret_cast<HWND>(lparam);
            for (const PaneHostState& pane : host.panes_) {
                const DockPanePlacement* placement = host.model_ == nullptr
                    ? nullptr : host.model_->Pane(pane.type);
                if (pane.tab_close_button == button && GetParent(button) == window
                    && placement != nullptr && placement->present
                    && placement->zone == tabs->zone && placement->stack == tabs->stack) {
                    const HWND focus = GetFocus();
                    const bool return_focus = focus == button || focus == pane.content
                        || (pane.content != nullptr && IsChild(pane.content, focus) != FALSE);
                    // The button owns a stable pane identity, never the selected
                    // tab index. A stale click cannot toggle a hidden pane open.
                    if (host.HidePane(pane.type) == DockResult::Ok && return_focus) {
                        SetFocus(host.owner_);
                    }
                    break;
                }
            }
            return 0;
        }
        if (message == WM_DRAWITEM) {
            const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
            if (draw != nullptr && host.DrawPaneTabCloseButton(*draw)) {
                return TRUE;
            }
        }
    }
    if (tabs != nullptr && tabs->host != nullptr
        && message == WM_CONTEXTMENU) {
        const int selected = TabCtrl_GetCurSel(window);
        TCITEMW item{};
        item.mask = TCIF_PARAM;
        if (selected >= 0 && TabCtrl_GetItem(window, selected, &item) != FALSE) {
            tabs->host->ShowContextMenu(
                static_cast<DockPaneType>(item.lParam),
                POINT{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)});
            return 0;
        }
    }
    const LRESULT result = DefSubclassProc(window, message, wparam, lparam);
    if (tabs != nullptr && tabs->host != nullptr
        && (message == WM_LBUTTONUP || message == WM_KEYUP)) {
        tabs->host->ActivateSelectedTab(*tabs);
        tabs->host->LayoutPaneTabCloseButtons(*tabs);
    }
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(window, TabSubclassProcedure, kTabSubclass);
        if (tabs != nullptr) {
            tabs->control = nullptr;
        }
    }
    return result;
}

LRESULT CALLBACK DockHost::PaneTabCloseButtonSubclassProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam,
    UINT_PTR, DWORD_PTR reference) noexcept {
    auto* pane = reinterpret_cast<PaneHostState*>(reference);
    if (pane != nullptr) {
        switch (message) {
            case WM_MOUSEMOVE:
                if (!pane->close_hovered) {
                    TRACKMOUSEEVENT tracking{sizeof(TRACKMOUSEEVENT), TME_LEAVE, window, 0U};
                    if (TrackMouseEvent(&tracking) != FALSE) {
                        pane->close_hovered = true;
                        InvalidateRect(window, nullptr, TRUE);
                    }
                }
                break;
            case WM_MOUSELEAVE:
                pane->close_hovered = false;
                InvalidateRect(window, nullptr, TRUE);
                return 0;
            case WM_THEMECHANGED:
            case WM_SYSCOLORCHANGE:
            case WM_DPICHANGED_AFTERPARENT:
                InvalidateRect(window, nullptr, TRUE);
                break;
            case WM_NCDESTROY:
                RemoveWindowSubclass(window, PaneTabCloseButtonSubclassProcedure,
                    kPaneTabCloseButtonSubclass);
                if (pane->tab_close_button == window) {
                    pane->tab_close_button = nullptr;
                    pane->close_hovered = false;
                }
                break;
            default:
                break;
        }
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

LRESULT CALLBACK DockHost::ToolTabSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* host = reinterpret_cast<DockHost*>(reference);
    if (host == nullptr) {
        return DefSubclassProc(window, message, wparam, lparam);
    }
    if (message == WM_COMMAND
        && LOWORD(wparam) == IDC_RIGHT_TOOL_TAB_CLOSE
        && HIWORD(wparam) == BN_CLICKED) {
        ToolTabCloseButtonSlot* slot = host->FindToolTabCloseButton(
            reinterpret_cast<HWND>(lparam));
        if (slot != nullptr && slot->tab) {
            static_cast<void>(host->CloseToolTab(slot->tab));
        }
        return 0;
    }
    if (message == WM_DRAWITEM) {
        const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
        if (draw != nullptr && host->DrawToolTabCloseButton(*draw)) {
            return TRUE;
        }
    }
    if (message == WM_LBUTTONDOWN) {
        const POINT client{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
        const int index = HitToolTab(window, client);
        const ToolTabId tab = ToolTabAt(window, index);
        if (tab) {
            host->dragging_tool_tab_ = tab;
            host->tool_tab_drag_origin_ = client;
            host->tool_tab_drag_active_ = false;
            SetCapture(window);
        }
    }

    const LRESULT result = DefSubclassProc(window, message, wparam, lparam);
    if (message == WM_MOUSEMOVE && GetCapture() == window
        && host->dragging_tool_tab_) {
        const POINT client{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
        if (!host->tool_tab_drag_active_
            && ExceedsDragThreshold(host->tool_tab_drag_origin_, client)) {
            host->tool_tab_drag_active_ = true;
        }
    }
    if (message == WM_LBUTTONUP) {
        ToolTabResult reorder_result = ToolTabResult::NoOp;
        if (host->tool_tab_drag_active_ && host->dragging_tool_tab_
            && host->right_tool_tabs_ != nullptr) {
            const POINT client{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
            const int index = HitToolTab(window, client);
            const ToolTabId target = ToolTabAt(window, index);
            RECT bounds{};
            if (target && target != host->dragging_tool_tab_
                && TabCtrl_GetItemRect(window, index, &bounds) != FALSE) {
                const bool after = client.x
                    >= bounds.left + (bounds.right - bounds.left) / 2;
                if (host->BeginLayoutMutation()) {
                    reorder_result = host->right_tool_tabs_->Reorder(
                        host->dragging_tool_tab_, target, after);
                }
            }
        }
        host->dragging_tool_tab_ = {};
        host->tool_tab_drag_active_ = false;
        if (GetCapture() == window) {
            ReleaseCapture();
        }
        if (reorder_result == ToolTabResult::Ok) {
            static_cast<void>(host->NotifyChanged());
        } else if (host->layout_mutation_pending_) {
            host->CancelLayoutMutation();
        }
        host->ActivateSelectedToolTab();
    } else if (message == WM_KEYUP) {
        host->ActivateSelectedToolTab();
    } else if (message == WM_KEYDOWN && wparam == VK_ESCAPE
               && host->dragging_tool_tab_) {
        host->dragging_tool_tab_ = {};
        host->tool_tab_drag_active_ = false;
        if (GetCapture() == window) {
            ReleaseCapture();
        }
        return 0;
    } else if (message == WM_CANCELMODE || message == WM_CAPTURECHANGED) {
        host->dragging_tool_tab_ = {};
        host->tool_tab_drag_active_ = false;
    } else if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(
            window, ToolTabSubclassProcedure, kToolTabSubclass);
        host->right_tool_tab_control_ = nullptr;
        host->dragging_tool_tab_ = {};
        host->tool_tab_drag_active_ = false;
    }
    return result;
}

LRESULT CALLBACK DockHost::ToolTabCloseButtonSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* slot = reinterpret_cast<ToolTabCloseButtonSlot*>(reference);
    switch (message) {
        case WM_MOUSEMOVE:
            if (slot != nullptr && !slot->hovered) {
                TRACKMOUSEEVENT tracking{};
                tracking.cbSize = sizeof(tracking);
                tracking.dwFlags = TME_LEAVE;
                tracking.hwndTrack = window;
                if (TrackMouseEvent(&tracking) != FALSE) {
                    slot->hovered = true;
                    InvalidateRect(window, nullptr, TRUE);
                }
            }
            break;
        case WM_MOUSELEAVE:
            if (slot != nullptr && slot->hovered) {
                slot->hovered = false;
                InvalidateRect(window, nullptr, TRUE);
            }
            return 0;
        case WM_THEMECHANGED:
        case WM_SYSCOLORCHANGE:
        case WM_DPICHANGED_AFTERPARENT:
            InvalidateRect(window, nullptr, TRUE);
            break;
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                window,
                ToolTabCloseButtonSubclassProcedure,
                kToolTabCloseButtonSubclass);
            if (slot != nullptr && slot->button == window) {
                slot->button = nullptr;
                slot->hovered = false;
            }
            break;
        default:
            break;
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

}  // namespace inkpod::windows::ui
