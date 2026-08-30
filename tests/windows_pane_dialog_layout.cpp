#include <windows.h>

#include <array>
#include <cstdio>

#include "ui/panes/pane_dialog_layout.h"

namespace {

using inkpod::windows::ui::panes::CompletePaneDialogResize;
using inkpod::windows::ui::panes::BeginPaneDialogLayoutTransaction;
using inkpod::windows::ui::panes::EndPaneDialogLayoutTransaction;
using inkpod::windows::ui::panes::FinalizePaneTabPageZOrder;
using inkpod::windows::ui::panes::IsPaneDialogResizeDeferred;
using inkpod::windows::ui::panes::kPaneDialogLayoutCapacity;
using inkpod::windows::ui::panes::PaneDialogLayoutPlan;
using inkpod::windows::ui::panes::PaneDialogLayoutFailed;
using inkpod::windows::ui::panes::PaneDialogRepaint;
using inkpod::windows::ui::panes::PaneButtonIdealWidth;
using inkpod::windows::ui::panes::PaneWindowHasBounds;
using inkpod::windows::ui::panes::PlacePaneButtonRows;
using inkpod::windows::ui::panes::PlacePaneTargetRow;
using inkpod::windows::ui::panes::SetPaneDialogResizeDeferred;
using inkpod::windows::ui::panes::ScopedPaneControlRedrawSuspension;

constexpr wchar_t kTestWindowClass[] = L"InkpodPaneDialogLayoutTestWindowV1";
constexpr int kFirstControl = 101;
constexpr int kSecondControl = 102;
constexpr int kUnchangedControl = 103;

struct WindowProbe final {
    std::uint32_t paints{};
    std::uint32_t erases{};
    std::uint32_t positions{};
    HWND destroy_on_position{};

    void Reset() noexcept {
        paints = 0U;
        erases = 0U;
        positions = 0U;
    }
};

struct ComboPositionProbe final {
    std::uint32_t changed{};
    WNDPROC original_procedure{};
};

constexpr wchar_t kComboPositionProbeProperty[] =
    L"Inkpod.PaneDialogLayout.ComboPositionProbe";

LRESULT CALLBACK ComboPositionSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    auto* probe = reinterpret_cast<ComboPositionProbe*>(
        GetPropW(window, kComboPositionProbeProperty));
    if (message == WM_WINDOWPOSCHANGED && probe != nullptr) {
        ++probe->changed;
    }
    const WNDPROC original = probe == nullptr
        ? DefWindowProcW
        : probe->original_procedure;
    if (message == WM_NCDESTROY) {
        SetWindowLongPtrW(
            window, GWLP_WNDPROC, reinterpret_cast<LONG_PTR>(original));
        RemovePropW(window, kComboPositionProbeProperty);
    }
    return CallWindowProcW(original, window, message, wparam, lparam);
}

bool InstallComboPositionProbe(
    HWND window, ComboPositionProbe& probe) noexcept {
    if (window == nullptr) {
        return false;
    }
    probe.original_procedure = reinterpret_cast<WNDPROC>(
        GetWindowLongPtrW(window, GWLP_WNDPROC));
    if (probe.original_procedure == nullptr
        || SetPropW(window, kComboPositionProbeProperty, &probe) == FALSE) {
        return false;
    }
    SetLastError(ERROR_SUCCESS);
    const LONG_PTR previous = SetWindowLongPtrW(
        window,
        GWLP_WNDPROC,
        reinterpret_cast<LONG_PTR>(ComboPositionSubclassProcedure));
    if (previous == 0 && GetLastError() != ERROR_SUCCESS) {
        RemovePropW(window, kComboPositionProbeProperty);
        return false;
    }
    probe.original_procedure = reinterpret_cast<WNDPROC>(previous);
    return probe.original_procedure != nullptr;
}

LRESULT CALLBACK TestWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* probe = reinterpret_cast<WindowProbe*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));
    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        probe = create == nullptr
            ? nullptr
            : static_cast<WindowProbe*>(create->lpCreateParams);
        SetWindowLongPtrW(
            window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(probe));
    }
    switch (message) {
        case WM_WINDOWPOSCHANGING:
            if (probe != nullptr) {
                ++probe->positions;
                const HWND target = probe->destroy_on_position;
                probe->destroy_on_position = nullptr;
                if (target != nullptr && IsWindow(target) != FALSE) {
                    DestroyWindow(target);
                }
            }
            break;
        case WM_ERASEBKGND: {
            if (probe != nullptr) {
                ++probe->erases;
            }
            RECT client{};
            const HDC target = reinterpret_cast<HDC>(wparam);
            if (target != nullptr && GetClientRect(window, &client) != FALSE) {
                FillRect(target, &client, GetSysColorBrush(COLOR_WINDOW));
                return TRUE;
            }
            break;
        }
        case WM_PAINT: {
            PAINTSTRUCT paint{};
            const HDC target = BeginPaint(window, &paint);
            if (target != nullptr) {
                RECT client{};
                if (GetClientRect(window, &client) != FALSE) {
                    FillRect(target, &client, GetSysColorBrush(COLOR_WINDOW));
                }
                EndPaint(window, &paint);
            }
            if (probe != nullptr) {
                ++probe->paints;
            }
            return 0;
        }
        case WM_NCDESTROY: {
            const LRESULT result = DefWindowProcW(window, message, wparam, lparam);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            return result;
        }
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

class RegisteredTestWindowClass final {
public:
    explicit RegisteredTestWindowClass(HINSTANCE instance) noexcept
        : instance_(instance) {
        WNDCLASSEXW type{};
        type.cbSize = sizeof(type);
        type.lpfnWndProc = TestWindowProcedure;
        type.hInstance = instance_;
        type.hCursor = LoadCursorW(nullptr, IDC_ARROW);
        type.hbrBackground = GetSysColorBrush(COLOR_WINDOW);
        type.lpszClassName = kTestWindowClass;
        registered_ = RegisterClassExW(&type) != 0;
    }

    ~RegisteredTestWindowClass() noexcept {
        if (registered_) {
            UnregisterClassW(kTestWindowClass, instance_);
        }
    }

    RegisteredTestWindowClass(const RegisteredTestWindowClass&) = delete;
    RegisteredTestWindowClass& operator=(const RegisteredTestWindowClass&) = delete;

    [[nodiscard]] bool Registered() const noexcept { return registered_; }

private:
    HINSTANCE instance_{};
    bool registered_{};
};

class OwnedWindow final {
public:
    explicit OwnedWindow(HWND window = nullptr) noexcept : window_(window) {}

    ~OwnedWindow() noexcept {
        if (window_ != nullptr && IsWindow(window_) != FALSE) {
            DestroyWindow(window_);
        }
    }

    OwnedWindow(const OwnedWindow&) = delete;
    OwnedWindow& operator=(const OwnedWindow&) = delete;

    [[nodiscard]] HWND Get() const noexcept { return window_; }

private:
    HWND window_{};
};

HWND CreateTestWindow(
    HINSTANCE instance,
    HWND parent,
    int control,
    DWORD style,
    int x,
    int y,
    int width,
    int height,
    WindowProbe& probe) noexcept {
    return CreateWindowExW(
        parent == nullptr ? WS_EX_NOACTIVATE : 0U,
        kTestWindowClass,
        L"",
        style,
        x,
        y,
        width,
        height,
        parent,
        parent == nullptr
            ? nullptr
            : reinterpret_cast<HMENU>(static_cast<INT_PTR>(control)),
        instance,
        &probe);
}

bool HasNoPendingUpdate(HWND window) noexcept {
    RECT update{};
    return window != nullptr && GetUpdateRect(window, &update, FALSE) == FALSE;
}

bool IsBelowInSiblingZOrder(HWND lower, HWND upper) noexcept {
    if (lower == nullptr || upper == nullptr || GetParent(lower) != GetParent(upper)) {
        return false;
    }
    for (HWND current = GetWindow(upper, GW_HWNDNEXT);
         current != nullptr;
         current = GetWindow(current, GW_HWNDNEXT)) {
        if (current == lower) {
            return true;
        }
    }
    return false;
}

void ResetAndValidate(
    HWND pane,
    const std::array<HWND, 3U>& children,
    WindowProbe& pane_probe,
    std::array<WindowProbe, 3U>& child_probes) noexcept {
    ValidateRect(pane, nullptr);
    pane_probe.Reset();
    for (std::size_t index = 0U; index < children.size(); ++index) {
        ValidateRect(children[index], nullptr);
        child_probes[index].Reset();
    }
}

int Fail(int code, const char* reason) noexcept {
    std::fprintf(stderr, "pane dialog layout test failed (%d): %s\n", code, reason);
    return code;
}

}  // namespace

int main() {
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    RegisteredTestWindowClass registration(instance);
    if (!registration.Registered()) {
        return Fail(1, "window class registration");
    }

    WindowProbe owner_probe{};
    OwnedWindow owner(CreateTestWindow(
        instance,
        nullptr,
        0,
        WS_POPUP | WS_BORDER | WS_CLIPCHILDREN,
        GetSystemMetrics(SM_XVIRTUALSCREEN) + 16,
        GetSystemMetrics(SM_YVIRTUALSCREEN) + 16,
        360,
        240,
        owner_probe));
    if (owner.Get() == nullptr) {
        return Fail(2, "owner creation");
    }

    WindowProbe pane_probe{};
    const HWND pane = CreateTestWindow(
        instance,
        owner.Get(),
        100,
        WS_CHILD | WS_VISIBLE,
        10,
        10,
        330,
        200,
        pane_probe);
    std::array<WindowProbe, 3U> child_probes{};
    const std::array<HWND, 3U> children{
        CreateTestWindow(
            instance, pane, kFirstControl, WS_CHILD | WS_VISIBLE,
            6, 6, 80, 30, child_probes[0]),
        CreateTestWindow(
            instance, pane, kSecondControl, WS_CHILD | WS_VISIBLE,
            92, 6, 80, 30, child_probes[1]),
        CreateTestWindow(
            instance, pane, kUnchangedControl, WS_CHILD | WS_VISIBLE,
            178, 6, 80, 30, child_probes[2])};
    if (pane == nullptr
        || children[0] == nullptr || children[1] == nullptr
        || children[2] == nullptr) {
        return Fail(3, "pane or child creation");
    }

    ShowWindow(owner.Get(), SW_SHOWNOACTIVATE);
    RedrawWindow(
        owner.Get(),
        nullptr,
        nullptr,
        RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW | RDW_ALLCHILDREN);
    ResetAndValidate(pane, children, pane_probe, child_probes);

    if ((GetWindowLongPtrW(pane, GWL_STYLE) & WS_CLIPCHILDREN) != 0) {
        return Fail(29, "fixture unexpectedly clips children before layout");
    }
    PaneDialogLayoutPlan batch(pane);
    if ((GetWindowLongPtrW(pane, GWL_STYLE) & WS_CLIPCHILDREN) == 0) {
        return Fail(21, "layout plan did not enable child clipping");
    }
    if (!batch.PlaceControl(kFirstControl, 10, 12, 92, 34)
        || !batch.PlaceControl(kSecondControl, 112, 12, 92, 34)
        || !batch.PlaceWindow(children[2], 178, 6, 80, 30)
        || !batch.Commit(PaneDialogRepaint::None)) {
        return Fail(4, "batch registration or commit");
    }
    if (!PaneWindowHasBounds(children[0], 10, 12, 92, 34)
        || !PaneWindowHasBounds(children[1], 112, 12, 92, 34)
        || !PaneWindowHasBounds(children[2], 178, 6, 80, 30)) {
        return Fail(5, "final batch geometry");
    }
    if (child_probes[0].positions == 0U || child_probes[1].positions == 0U
        || child_probes[2].positions != 0U) {
        return Fail(6, "changed placement or unchanged skip");
    }
    if (pane_probe.paints != 0U
        || child_probes[0].paints != 0U || child_probes[1].paints != 0U
        || child_probes[2].paints != 0U) {
        return Fail(7, "intermediate painting");
    }
    if (!HasNoPendingUpdate(pane)
        || !HasNoPendingUpdate(children[0])
        || !HasNoPendingUpdate(children[1])
        || !HasNoPendingUpdate(children[2])) {
        return Fail(8, "no-repaint batch left an update region");
    }
    if (batch.Commit(PaneDialogRepaint::None)) {
        return Fail(9, "layout plan committed twice");
    }
    if (!BeginPaneDialogLayoutTransaction(pane)
        || PaneDialogLayoutFailed(pane)) {
        return Fail(40, "outer layout transaction begin");
    }
    PaneDialogLayoutPlan nested_failure(children[0]);
    if (nested_failure.PlaceControl(9999, 0, 0, 1, 1)
        || nested_failure.Commit(PaneDialogRepaint::None)
        || !PaneDialogLayoutFailed(pane)) {
        return Fail(41, "nested plan failure did not reach pane root");
    }
    PaneDialogLayoutPlan later_success(pane);
    if (!later_success.PlaceControl(kFirstControl, 10, 12, 92, 34)
        || !later_success.Commit(PaneDialogRepaint::None)
        || !PaneDialogLayoutFailed(pane)) {
        return Fail(42, "later success erased sticky transaction failure");
    }
    EndPaneDialogLayoutTransaction(pane);
    if (!PaneDialogLayoutFailed(pane)
        || !BeginPaneDialogLayoutTransaction(pane)
        || PaneDialogLayoutFailed(pane)) {
        return Fail(43, "next transaction did not clear prior failure");
    }
    EndPaneDialogLayoutTransaction(pane);
    PaneDialogLayoutPlan standalone_failure(pane);
    if (standalone_failure.PlaceControl(9999, 0, 0, 1, 1)
        || standalone_failure.Commit(PaneDialogRepaint::None)
        || !PaneDialogLayoutFailed(pane)) {
        return Fail(47, "standalone plan failure status");
    }
    PaneDialogLayoutPlan standalone_recovery(pane);
    if (!standalone_recovery.PlaceControl(kFirstControl, 10, 12, 92, 34)
        || !standalone_recovery.Commit(PaneDialogRepaint::None)
        || PaneDialogLayoutFailed(pane)) {
        return Fail(48, "standalone plan success did not clear failure status");
    }

    ResetAndValidate(pane, children, pane_probe, child_probes);
    PaneDialogLayoutPlan unchanged(pane);
    if (!unchanged.PlaceControl(kFirstControl, 10, 12, 92, 34)
        || !unchanged.PlaceControl(kSecondControl, 112, 12, 92, 34)
        || !unchanged.PlaceControl(kUnchangedControl, 178, 6, 80, 30)
        || !unchanged.Commit(PaneDialogRepaint::None)
        || child_probes[0].positions != 0U
        || child_probes[1].positions != 0U
        || child_probes[2].positions != 0U) {
        return Fail(10, "all-unchanged commit");
    }

    ResetAndValidate(pane, children, pane_probe, child_probes);
    PaneDialogLayoutPlan complete(pane);
    if (!complete.PlaceControl(kFirstControl, 12, 58, 100, 36)
        || !complete.PlaceControl(kSecondControl, 122, 58, 100, 36)
        || !complete.PlaceWindow(children[2], 178, 6, 80, 30)
        || !complete.Commit(PaneDialogRepaint::Complete)) {
        return Fail(11, "complete commit");
    }
    if (pane_probe.paints != 1U
        || child_probes[0].paints == 0U
        || child_probes[1].paints == 0U
        || child_probes[2].paints == 0U) {
        return Fail(12, "single final subtree repaint");
    }
    if (!HasNoPendingUpdate(pane)
        || !HasNoPendingUpdate(children[0])
        || !HasNoPendingUpdate(children[1])
        || !HasNoPendingUpdate(children[2])) {
        return Fail(13, "complete repaint left an update region");
    }

    ResetAndValidate(pane, children, pane_probe, child_probes);
    if (!SetPaneDialogResizeDeferred(pane, true)
        || !SetPaneDialogResizeDeferred(pane, true)
        || !IsPaneDialogResizeDeferred(pane)
        || !IsPaneDialogResizeDeferred(children[0])) {
        return Fail(14, "defer property or ancestor lookup");
    }
    CompletePaneDialogResize(children[0]);
    if (child_probes[0].paints != 0U) {
        return Fail(15, "ancestor defer did not suppress completion");
    }

    PaneDialogLayoutPlan deferred(pane);
    if (!deferred.PlaceControl(kFirstControl, 16, 104, 104, 38)
        || !deferred.PlaceControl(kSecondControl, 130, 104, 104, 38)
        || !deferred.PlaceControl(kUnchangedControl, 244, 104, 70, 38)
        || !deferred.Commit(PaneDialogRepaint::Complete)) {
        return Fail(16, "deferred complete commit");
    }
    if (pane_probe.paints != 0U
        || child_probes[0].paints != 0U || child_probes[1].paints != 0U
        || child_probes[2].paints != 0U) {
        return Fail(17, "outer defer allowed an intermediate repaint");
    }
    if (!SetPaneDialogResizeDeferred(pane, false)
        || IsPaneDialogResizeDeferred(pane)
        || IsPaneDialogResizeDeferred(children[0])) {
        return Fail(18, "defer property removal");
    }
    CompletePaneDialogResize(pane);
    if (pane_probe.paints != 1U
        || child_probes[0].paints == 0U
        || child_probes[1].paints == 0U
        || child_probes[2].paints == 0U) {
        return Fail(19, "deferred final repaint");
    }
    if (!HasNoPendingUpdate(pane)
        || !HasNoPendingUpdate(children[0])
        || !HasNoPendingUpdate(children[1])
        || !HasNoPendingUpdate(children[2])) {
        return Fail(20, "deferred final repaint left an update region");
    }

    ResetAndValidate(pane, children, pane_probe, child_probes);
    const std::array<int, 2U> helper_controls{
        kFirstControl, kSecondControl};
    PaneDialogLayoutPlan helpers(pane);
    const std::size_t helper_rows = PlacePaneButtonRows(
        helpers,
        std::span<const int>(helper_controls),
        8,
        8,
        220,
        30,
        6);
    PlacePaneTargetRow(
        helpers,
        kFirstControl,
        kSecondControl,
        8,
        52,
        260,
        2,
        24,
        28,
        6);
    if (helper_rows != 1U || !helpers.IsValid()
        || helpers.Overflowed()
        || !helpers.Commit(PaneDialogRepaint::None)) {
        return Fail(22, "plan helper overloads");
    }
    const bool first_below_second_before =
        IsBelowInSiblingZOrder(children[0], children[1]);
    const bool second_below_first_before =
        IsBelowInSiblingZOrder(children[1], children[0]);
    FinalizePaneTabPageZOrder(
        pane,
        kUnchangedControl,
        std::span<const int>(helper_controls));
    if (!IsBelowInSiblingZOrder(children[2], children[0])
        || !IsBelowInSiblingZOrder(children[2], children[1])
        || first_below_second_before
            != IsBelowInSiblingZOrder(children[0], children[1])
        || second_below_first_before
            != IsBelowInSiblingZOrder(children[1], children[0])) {
        return Fail(27, "tab page final z-order");
    }
    SetWindowPos(
        children[2],
        HWND_TOP,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOREDRAW);
    ShowWindow(pane, SW_HIDE);
    FinalizePaneTabPageZOrder(
        pane,
        kUnchangedControl,
        std::span<const int>(helper_controls));
    const bool hidden_parent_z_order =
        IsBelowInSiblingZOrder(children[2], children[0])
        && IsBelowInSiblingZOrder(children[2], children[1]);
    ShowWindow(pane, SW_SHOWNOACTIVATE);
    if (!hidden_parent_z_order) {
        return Fail(28, "hidden-parent tab page final z-order");
    }

    const RECT before_invalid{
        8,
        54,
        8 + std::max(0, 260 - PaneButtonIdealWidth(pane, kSecondControl) - 6),
        54 + 24};
    PaneDialogLayoutPlan invalid(pane);
    if (!invalid.PlaceControl(
            kFirstControl,
            before_invalid.left + 20,
            before_invalid.top,
            before_invalid.right - before_invalid.left,
            before_invalid.bottom - before_invalid.top)
        || invalid.PlaceControl(9999, 0, 0, 1, 1)
        || invalid.IsValid()
        || invalid.Overflowed()
        || invalid.Commit(PaneDialogRepaint::None)
        || !PaneWindowHasBounds(
            children[0],
            before_invalid.left,
            before_invalid.top,
            before_invalid.right - before_invalid.left,
            before_invalid.bottom - before_invalid.top)) {
        return Fail(23, "invalid plan published partial geometry");
    }

    WindowProbe rollback_survivor_probe{};
    WindowProbe rollback_destroyed_probe{};
    const HWND rollback_survivor = CreateTestWindow(
        instance,
        pane,
        800,
        WS_CHILD,
        4,
        160,
        12,
        12,
        rollback_survivor_probe);
    const HWND rollback_destroyed = CreateTestWindow(
        instance,
        pane,
        801,
        WS_CHILD,
        20,
        160,
        12,
        12,
        rollback_destroyed_probe);
    if (rollback_survivor == nullptr || rollback_destroyed == nullptr) {
        return Fail(30, "rollback fixture creation");
    }
    rollback_survivor_probe.destroy_on_position = rollback_destroyed;
    PaneDialogLayoutPlan partial_failure(pane);
    if (!partial_failure.PlaceWindow(rollback_survivor, 4, 176, 14, 14)
        || !partial_failure.PlaceWindow(rollback_destroyed, 22, 176, 14, 14)
        || partial_failure.Commit(PaneDialogRepaint::None)
        || IsWindow(rollback_destroyed) != FALSE
        || partial_failure.RollbackSucceeded()
        || !PaneWindowHasBounds(rollback_survivor, 4, 160, 12, 12)) {
        return Fail(31, "partial SetWindowPos failure did not roll back survivor");
    }

    std::array<WindowProbe, kPaneDialogLayoutCapacity + 1U> overflow_probes{};
    std::array<HWND, kPaneDialogLayoutCapacity + 1U> overflow_windows{};
    for (std::size_t index = 0U; index < overflow_windows.size(); ++index) {
        overflow_windows[index] = CreateTestWindow(
            instance,
            pane,
            1000 + static_cast<int>(index),
            WS_CHILD,
            0,
            0,
            1,
            1,
            overflow_probes[index]);
        if (overflow_windows[index] == nullptr) {
            return Fail(24, "overflow fixture creation");
        }
    }
    PaneDialogLayoutPlan exact_capacity(pane);
    for (std::size_t index = 0U; index < kPaneDialogLayoutCapacity; ++index) {
        if (!exact_capacity.PlaceWindow(
                overflow_windows[index],
                static_cast<int>(index) + 2,
                2,
                2,
                2)) {
            return Fail(25, "capacity registration");
        }
    }
    if (!exact_capacity.IsValid()
        || exact_capacity.Overflowed()
        || !exact_capacity.Commit(PaneDialogRepaint::None)) {
        return Fail(32, "exact-capacity commit");
    }
    for (std::size_t index = 0U; index < kPaneDialogLayoutCapacity; ++index) {
        if (!PaneWindowHasBounds(
                overflow_windows[index],
                static_cast<int>(index) + 2,
                2,
                2,
                2)) {
            return Fail(33, "exact-capacity final geometry");
        }
    }

    PaneDialogLayoutPlan overflow(pane);
    for (std::size_t index = 0U; index < kPaneDialogLayoutCapacity; ++index) {
        if (!overflow.PlaceWindow(
                overflow_windows[index],
                static_cast<int>(index) + 4,
                6,
                3,
                3)) {
            return Fail(34, "overflow prefix registration");
        }
    }
    if (overflow.PlaceWindow(overflow_windows.back(), 80, 6, 3, 3)
        || overflow.IsValid()
        || !overflow.Overflowed()
        || overflow.Commit(PaneDialogRepaint::None)) {
        return Fail(26, "overflow plan published partial geometry");
    }
    for (std::size_t index = 0U; index < kPaneDialogLayoutCapacity; ++index) {
        if (!PaneWindowHasBounds(
                overflow_windows[index],
                static_cast<int>(index) + 2,
                2,
                2,
                2)) {
            return Fail(35, "overflow plan changed its registered prefix");
        }
    }

    ResetAndValidate(pane, children, pane_probe, child_probes);
    {
        ScopedPaneControlRedrawSuspension child_redraw(children[0]);
        InvalidateRect(children[0], nullptr, TRUE);
        UpdateWindow(children[0]);
        if (child_probes[0].paints != 0U) {
            return Fail(36, "control metric redraw guard allowed intermediate paint");
        }
    }
    CompletePaneDialogResize(pane);
    if ((GetWindowLongPtrW(children[0], GWL_STYLE) & WS_VISIBLE) == 0
        || child_probes[0].paints == 0U || !HasNoPendingUpdate(children[0])) {
        return Fail(37, "control metric redraw guard did not join final repaint");
    }

    ComboPositionProbe combo_probe{};
    OwnedWindow normalized_combo(CreateWindowExW(
        0,
        WC_COMBOBOXW,
        L"",
        WS_CHILD | WS_VISIBLE | CBS_DROPDOWNLIST,
        4,
        140,
        100,
        24,
        pane,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(900)),
        instance,
        nullptr));
    PaneDialogLayoutPlan combo_plan(pane);
    if (normalized_combo.Get() == nullptr
        || !InstallComboPositionProbe(normalized_combo.Get(), combo_probe)
        || !combo_plan.PlaceWindow(normalized_combo.Get(), 12, 140, 180, 60)
        || !combo_plan.Commit(PaneDialogRepaint::None)
        || !PaneWindowHasBounds(normalized_combo.Get(), 12, 140, 180, 60)
        || combo_probe.changed == 0U) {
        return Fail(38, "system-normalized combo height");
    }
    RECT combo_bounds{};
    if (GetWindowRect(normalized_combo.Get(), &combo_bounds) == FALSE
        || combo_bounds.bottom - combo_bounds.top == 60) {
        return Fail(39, "combo fixture did not normalize requested height");
    }
    combo_probe.changed = 0U;
    PaneDialogLayoutPlan unchanged_combo(pane);
    if (!unchanged_combo.PlaceWindow(
            normalized_combo.Get(), 12, 140, 180, 60)
        || !unchanged_combo.Commit(PaneDialogRepaint::None)
        || combo_probe.changed != 0U) {
        return Fail(44, "normalized combo was repositioned by unchanged layout");
    }
    if (PaneWindowHasBounds(normalized_combo.Get(), 13, 140, 180, 60)
        || PaneWindowHasBounds(normalized_combo.Get(), 12, 141, 180, 60)
        || PaneWindowHasBounds(normalized_combo.Get(), 12, 140, 181, 60)
        || PaneWindowHasBounds(normalized_combo.Get(), 12, 140, 180, 0)) {
        return Fail(45, "normalized combo accepted mismatched bounds");
    }
    PaneDialogLayoutPlan zero_combo(pane);
    if (zero_combo.PlaceWindow(normalized_combo.Get(), 12, 140, 180, 0)
        || zero_combo.IsValid()
        || zero_combo.Commit(PaneDialogRepaint::None)
        || combo_probe.changed != 0U) {
        return Fail(46, "normalized combo accepted zero target height");
    }

    return 0;
}
