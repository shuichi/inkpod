#pragma once

#include <windows.h>

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <span>
#include <string>
#include <string_view>

namespace inkpod::windows::ui::panes {

inline int ScalePaneDip(HWND dialog, int value) noexcept {
    const UINT dpi = dialog == nullptr ? 96U : GetDpiForWindow(dialog);
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

inline void EnablePaneDialogResizePainting(HWND dialog) noexcept {
    if (dialog == nullptr) {
        return;
    }
    const LONG_PTR style = GetWindowLongPtrW(dialog, GWL_STYLE);
    if ((style & WS_CLIPCHILDREN) == 0) {
        SetWindowLongPtrW(dialog, GWL_STYLE, style | WS_CLIPCHILDREN);
    }
}

inline constexpr wchar_t kPaneDialogResizeDeferredProperty[] =
    L"Inkpod.PaneDialogResizeDeferred";
inline constexpr wchar_t kPaneDialogLayoutFailedProperty[] =
    L"Inkpod.PaneDialogLayoutFailed";
inline constexpr wchar_t kPaneDialogLayoutTransactionProperty[] =
    L"Inkpod.PaneDialogLayoutTransaction";

// An outer DockHost placement begins one transaction on each pane root. Any
// nested dialog plan walks to that root and publishes a sticky failure there;
// a later successful sibling plan must not erase an earlier failure from the
// same transaction.
inline bool BeginPaneDialogLayoutTransaction(HWND pane_root) noexcept {
    if (pane_root == nullptr) {
        return false;
    }
    if (GetPropW(pane_root, kPaneDialogLayoutTransactionProperty) != nullptr) {
        return true;
    }
    if (GetPropW(pane_root, kPaneDialogLayoutFailedProperty) != nullptr
        && RemovePropW(pane_root, kPaneDialogLayoutFailedProperty) == nullptr) {
        return false;
    }
    return SetPropW(
               pane_root,
               kPaneDialogLayoutTransactionProperty,
               reinterpret_cast<HANDLE>(static_cast<ULONG_PTR>(1U)))
        != FALSE;
}

inline bool PaneDialogLayoutFailed(HWND pane_root) noexcept {
    return pane_root != nullptr
        && GetPropW(pane_root, kPaneDialogLayoutFailedProperty) != nullptr;
}

inline void EndPaneDialogLayoutTransaction(HWND pane_root) noexcept {
    if (pane_root != nullptr
        && GetPropW(pane_root, kPaneDialogLayoutTransactionProperty) != nullptr) {
        RemovePropW(pane_root, kPaneDialogLayoutTransactionProperty);
    }
}

inline HWND PaneDialogLayoutTransactionRoot(HWND dialog) noexcept {
    for (HWND current = dialog; current != nullptr; current = GetParent(current)) {
        if (GetPropW(current, kPaneDialogLayoutTransactionProperty) != nullptr) {
            return current;
        }
    }
    return nullptr;
}

inline void MarkPaneDialogLayoutFailed(HWND dialog) noexcept {
    const HWND transaction_root = PaneDialogLayoutTransactionRoot(dialog);
    const HWND target = transaction_root == nullptr ? dialog : transaction_root;
    if (target != nullptr) {
        SetPropW(
            target,
            kPaneDialogLayoutFailedProperty,
            reinterpret_cast<HANDLE>(static_cast<ULONG_PTR>(1U)));
    }
}

inline void ClearStandalonePaneDialogLayoutFailure(HWND dialog) noexcept {
    if (dialog != nullptr
        && PaneDialogLayoutTransactionRoot(dialog) == nullptr
        && GetPropW(dialog, kPaneDialogLayoutFailedProperty) != nullptr) {
        RemovePropW(dialog, kPaneDialogLayoutFailedProperty);
    }
}

// DockHost sets this property while it is positioning every pane root. Child
// dialog procedures can still compute and commit their final geometry, but
// their synchronous completion repaint is deferred until the host has placed
// all sibling panes.
inline bool SetPaneDialogResizeDeferred(
    HWND pane_root, bool deferred) noexcept {
    if (pane_root == nullptr) {
        return false;
    }
    if (deferred) {
        if (GetPropW(pane_root, kPaneDialogResizeDeferredProperty) != nullptr) {
            return true;
        }
        return SetPropW(
                   pane_root,
                   kPaneDialogResizeDeferredProperty,
                   reinterpret_cast<HANDLE>(static_cast<ULONG_PTR>(1U)))
            != FALSE;
    }
    if (GetPropW(pane_root, kPaneDialogResizeDeferredProperty) == nullptr) {
        return true;
    }
    return RemovePropW(pane_root, kPaneDialogResizeDeferredProperty) != nullptr;
}

inline bool IsPaneDialogResizeDeferred(HWND window) noexcept {
    for (HWND current = window; current != nullptr; current = GetParent(current)) {
        if (GetPropW(current, kPaneDialogResizeDeferredProperty) != nullptr) {
            return true;
        }
    }
    return false;
}

inline void CompletePaneDialogResize(HWND dialog) noexcept {
    if (dialog == nullptr || IsPaneDialogResizeDeferred(dialog)) {
        return;
    }
    if (IsWindowVisible(dialog) == FALSE) {
        // Showing the pane will invalidate it. Do not leave deferred child
        // update regions on a hidden tab merely because its geometry changed.
        return;
    }
    // The pane owns the vacated pixels from every child moved during WM_SIZE.
    // Erase them once, then synchronously paint the final child geometry so a
    // rapid splitter drag cannot present intermediate frames or leave trails.
    // Explicit child invalidation is required before the one synchronous
    // subtree update: RDW_ALLCHILDREN alone may skip a page control overlapped
    // by a Common Controls tab sibling even when its final z-order is correct.
    // RDW_FRAME publishes a pane-owned non-client scrollbar in this same final
    // frame instead of requiring a control-specific immediate redraw.
    EnumChildWindows(
        dialog,
        [](HWND child, LPARAM) noexcept -> BOOL {
            if (IsWindowVisible(child) != FALSE) {
                RedrawWindow(
                    child,
                    nullptr,
                    nullptr,
                    RDW_INVALIDATE | RDW_ERASE | RDW_NOCHILDREN);
            }
            return TRUE;
        },
        0);
    RedrawWindow(
        dialog,
        nullptr,
        nullptr,
        RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_UPDATENOW
            | RDW_ALLCHILDREN);
}

// Common Controls may draw synchronously while processing metric messages
// (for example column width or owner-draw item height changes). Keep those
// mutations inside the same pane resize transaction, then let the shared final
// repaint expose them with the committed geometry.
class ScopedPaneControlRedrawSuspension final {
public:
    explicit ScopedPaneControlRedrawSuspension(HWND control) noexcept
        : control_(control) {
        if (control_ != nullptr
            && (GetWindowLongPtrW(control_, GWL_STYLE) & WS_VISIBLE) != 0) {
            SendMessageW(control_, WM_SETREDRAW, FALSE, 0);
            suspended_ = true;
        }
    }

    ~ScopedPaneControlRedrawSuspension() noexcept { Restore(); }
    ScopedPaneControlRedrawSuspension(
        const ScopedPaneControlRedrawSuspension&) = delete;
    ScopedPaneControlRedrawSuspension& operator=(
        const ScopedPaneControlRedrawSuspension&) = delete;

    void Restore() noexcept {
        if (suspended_ && IsWindow(control_) != FALSE) {
            SendMessageW(control_, WM_SETREDRAW, TRUE, 0);
        }
        suspended_ = false;
    }

private:
    HWND control_{};
    bool suspended_{};
};

inline int PaneControlTextHeight(HWND control) noexcept {
    if (control == nullptr) {
        return 0;
    }
    HDC device = GetDC(control);
    if (device == nullptr) {
        return 0;
    }
    const HFONT font = reinterpret_cast<HFONT>(
        SendMessageW(control, WM_GETFONT, 0, 0));
    const HGDIOBJ previous = font == nullptr ? nullptr : SelectObject(device, font);
    TEXTMETRICW metrics{};
    const bool measured = GetTextMetricsW(device, &metrics) != FALSE;
    if (previous != nullptr && previous != HGDI_ERROR) {
        SelectObject(device, previous);
    }
    ReleaseDC(control, device);
    return measured
        ? static_cast<int>(metrics.tmHeight + metrics.tmExternalLeading)
        : 0;
}

inline int PaneReadableControlHeight(
    HWND dialog,
    int control,
    int minimum_height_dip,
    int vertical_padding_dip) noexcept {
    return std::max(
        ScalePaneDip(dialog, minimum_height_dip),
        PaneControlTextHeight(GetDlgItem(dialog, control))
            + ScalePaneDip(dialog, vertical_padding_dip));
}

inline bool PaneWindowUsesSystemNormalizedHeight(HWND child) noexcept {
    if (child == nullptr) {
        return false;
    }
    std::array<wchar_t, 32U> class_name{};
    const bool combo_box = GetClassNameW(
            child, class_name.data(), static_cast<int>(class_name.size())) > 0
        && CompareStringOrdinal(
               class_name.data(), -1, WC_COMBOBOXW, -1, TRUE) == CSTR_EQUAL;
    const LONG_PTR combo_box_type = GetWindowLongPtrW(child, GWL_STYLE) & 0x0003L;
    return combo_box
        && (combo_box_type == CBS_DROPDOWN
            || combo_box_type == CBS_DROPDOWNLIST);
}

inline bool PaneWindowHasBounds(
    HWND child, int x, int y, int width, int height) noexcept {
    RECT bounds{};
    if (child == nullptr || GetWindowRect(child, &bounds) == FALSE) {
        return false;
    }
    const HWND parent = GetParent(child);
    POINT top_left{bounds.left, bounds.top};
    POINT bottom_right{bounds.right, bounds.bottom};
    if (parent != nullptr
        && (ScreenToClient(parent, &top_left) == FALSE
            || ScreenToClient(parent, &bottom_right) == FALSE)) {
        return false;
    }
    const bool system_normalized_height =
        PaneWindowUsesSystemNormalizedHeight(child);
    const int actual_height = bottom_right.y - top_left.y;
    const int requested_height = std::max(0, height);
    return top_left.x == x && top_left.y == y
        && bottom_right.x - top_left.x == std::max(0, width)
        && (actual_height == requested_height
            || (system_normalized_height
                && requested_height > 0
                && actual_height > 0));
}

inline constexpr std::size_t kPaneDialogLayoutCapacity = 64U;

// Common Controls tab pages in a modeless pane are sibling HWNDs rather than
// child windows of the tab control. Finish their z-order before the one final
// repaint; putting only the tab at HWND_BOTTOM is not sufficient after every
// dialog-manager resize, because a still-higher tab clips otherwise-valid page
// controls through WS_CLIPSIBLINGS.
inline bool FinalizePaneTabPageZOrder(
    HWND dialog,
    int tab_control,
    std::span<const int> page_controls) noexcept {
    const HWND tabs = GetDlgItem(dialog, tab_control);
    if (tabs == nullptr) {
        MarkPaneDialogLayoutFailed(dialog);
        return false;
    }
    constexpr UINT flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
        | SWP_NOOWNERZORDER | SWP_NOREDRAW | SWP_NOCOPYBITS;
    std::array<HWND, kPaneDialogLayoutCapacity> visible_pages{};
    std::size_t visible_page_count{};
    std::array<HWND, kPaneDialogLayoutCapacity> original_order{};
    std::size_t original_count{};
    for (HWND sibling = GetWindow(tabs, GW_HWNDFIRST);
         sibling != nullptr;
         sibling = GetWindow(sibling, GW_HWNDNEXT)) {
        if (original_count == original_order.size()) {
            MarkPaneDialogLayoutFailed(dialog);
            return false;
        }
        original_order[original_count++] = sibling;
        const int control = GetDlgCtrlID(sibling);
        if (sibling != tabs
            && (GetWindowLongPtrW(sibling, GWL_STYLE) & WS_VISIBLE) != 0
            && std::find(page_controls.begin(), page_controls.end(), control)
                != page_controls.end()
            && visible_page_count < visible_pages.size()) {
            visible_pages[visible_page_count++] = sibling;
        }
    }
    bool positioned = SetWindowPos(tabs, HWND_BOTTOM, 0, 0, 0, 0, flags)
        != FALSE;
    // Raise bottom-to-top so the visible page controls keep their existing
    // sibling order (which is also the dialog's keyboard traversal order).
    for (std::size_t index = visible_page_count; index > 0U; --index) {
        positioned = SetWindowPos(
                         visible_pages[index - 1U],
                         HWND_TOP,
                         0,
                         0,
                         0,
                         0,
                         flags)
                != FALSE
            && positioned;
    }
    const auto is_below = [](HWND lower, HWND upper) noexcept {
        for (HWND current = GetWindow(upper, GW_HWNDNEXT);
             current != nullptr;
             current = GetWindow(current, GW_HWNDNEXT)) {
            if (current == lower) {
                return true;
            }
        }
        return false;
    };
    for (std::size_t index = 0U; index < visible_page_count; ++index) {
        positioned = is_below(tabs, visible_pages[index]) && positioned;
        if (index > 0U) {
            positioned = is_below(
                             visible_pages[index], visible_pages[index - 1U])
                && positioned;
        }
    }
    if (positioned) {
        return true;
    }
    bool restored = true;
    for (std::size_t index = original_count; index > 0U; --index) {
        restored = SetWindowPos(
                       original_order[index - 1U],
                       HWND_TOP,
                       0,
                       0,
                       0,
                       0,
                       flags)
                != FALSE
            && restored;
    }
    std::size_t verified{};
    for (HWND sibling = GetWindow(tabs, GW_HWNDFIRST);
         sibling != nullptr && verified < original_count;
         sibling = GetWindow(sibling, GW_HWNDNEXT), ++verified) {
        restored = sibling == original_order[verified] && restored;
    }
    restored = verified == original_count && restored;
    static_cast<void>(restored);
    MarkPaneDialogLayoutFailed(dialog);
    return false;
}

enum class PaneDialogRepaint : std::uint8_t {
    None,
    Complete,
};

// A fixed-capacity, explicit-commit placement plan for modeless pane dialogs.
// Registration only records final geometry. Commit first removes unchanged
// windows, then applies the complete changed set without intermediate painting.
class PaneDialogLayoutPlan final {
public:
    explicit PaneDialogLayoutPlan(HWND dialog) noexcept
        : dialog_(dialog), valid_(dialog != nullptr) {
        EnablePaneDialogResizePainting(dialog_);
    }

    PaneDialogLayoutPlan(const PaneDialogLayoutPlan&) = delete;
    PaneDialogLayoutPlan& operator=(const PaneDialogLayoutPlan&) = delete;

    [[nodiscard]] bool PlaceControl(
        int control, int x, int y, int width, int height) noexcept {
        if (committed_ || !valid_) {
            return false;
        }
        const HWND window = GetDlgItem(dialog_, control);
        if (window == nullptr) {
            valid_ = false;
            return false;
        }
        return PlaceWindow(window, x, y, width, height);
    }

    [[nodiscard]] bool PlaceWindow(
        HWND window, int x, int y, int width, int height) noexcept {
        if (committed_ || !valid_) {
            return false;
        }
        if (window == nullptr) {
            valid_ = false;
            return false;
        }
        // CBS_DROPDOWN and CBS_DROPDOWNLIST publish a system-selected closed
        // height even when SetWindowPos receives a different positive height.
        // A non-positive request has no valid normalized counterpart, so reject
        // it before any part of the placement plan can be published.
        if (height <= 0 && PaneWindowUsesSystemNormalizedHeight(window)) {
            valid_ = false;
            return false;
        }
        for (std::size_t index = 0U; index < placement_count_; ++index) {
            if (placements_[index].window == window) {
                placements_[index] = Placement{
                    window, x, y, std::max(0, width), std::max(0, height)};
                return true;
            }
        }
        if (placement_count_ == placements_.size()) {
            overflowed_ = true;
            valid_ = false;
            return false;
        }
        placements_[placement_count_++] = Placement{
            window, x, y, std::max(0, width), std::max(0, height)};
        return true;
    }

    [[nodiscard]] bool Commit(PaneDialogRepaint repaint) noexcept {
        if (committed_) {
            return false;
        }
        committed_ = true;
        if (!valid_) {
            MarkPaneDialogLayoutFailed(dialog_);
            return false;
        }

        std::size_t changed_count{};
        for (std::size_t index = 0U; index < placement_count_; ++index) {
            Placement& placement = placements_[index];
            if (!CaptureOriginalBounds(placement)) {
                MarkPaneDialogLayoutFailed(dialog_);
                return false;
            }
            // Use the same comparison as final verification. In particular,
            // this prevents a normalized ComboBox height from making every
            // otherwise-identical layout look changed forever.
            placement.changed = !PaneWindowHasBounds(
                placement.window,
                placement.x,
                placement.y,
                placement.width,
                placement.height);
            changed_count += placement.changed ? 1U : 0U;
        }

        bool placed = true;
        if (changed_count != 0U) {
            HDWP deferred = BeginDeferWindowPos(static_cast<int>(changed_count));
            placed = deferred != nullptr;
            for (std::size_t index = 0U;
                 placed && index < placement_count_;
                 ++index) {
                const Placement& placement = placements_[index];
                if (!placement.changed) {
                    continue;
                }
                deferred = DeferWindowPos(
                    deferred,
                    placement.window,
                    nullptr,
                    placement.x,
                    placement.y,
                    placement.width,
                    placement.height,
                    kPlacementFlags);
                placed = deferred != nullptr;
            }
            if (placed) {
                placed = EndDeferWindowPos(deferred) != FALSE;
            }
            if (placed) {
                placed = HasFinalGeometry();
            }
            if (!placed) {
                // A failed deferred sequence publishes no reliable subset.
                // Reapply the complete registered final geometry so callers do
                // not need to reason about partial Begin/Defer/End outcomes.
                placed = true;
                for (std::size_t index = 0U; index < placement_count_; ++index) {
                    const Placement& placement = placements_[index];
                    if (!placement.changed) {
                        continue;
                    }
                    const bool positioned = SetWindowPos(
                        placement.window,
                        nullptr,
                        placement.x,
                        placement.y,
                        placement.width,
                        placement.height,
                        kPlacementFlags)
                        != FALSE;
                    placed = positioned && placed;
                }
                placed = HasFinalGeometry() && placed;
                if (!placed) {
                    // SetWindowPos can fail after earlier windows have already
                    // moved. Restore every changed window, including those not
                    // yet attempted, so a false result never intentionally
                    // leaves a successfully positioned prefix published.
                    rollback_succeeded_ = RestoreOriginalGeometry();
                }
            }
        }

        if (placed && repaint == PaneDialogRepaint::Complete) {
            CompletePaneDialogResize(dialog_);
        }
        if (placed) {
            ClearStandalonePaneDialogLayoutFailure(dialog_);
        } else {
            MarkPaneDialogLayoutFailed(dialog_);
        }
        return placed;
    }

    [[nodiscard]] HWND Dialog() const noexcept { return dialog_; }
    [[nodiscard]] bool IsValid() const noexcept { return valid_; }
    [[nodiscard]] bool Overflowed() const noexcept { return overflowed_; }
    [[nodiscard]] bool RollbackSucceeded() const noexcept {
        return rollback_succeeded_;
    }

private:
    struct Placement final {
        HWND window{};
        int x{};
        int y{};
        int width{};
        int height{};
        int original_x{};
        int original_y{};
        int original_width{};
        int original_height{};
        bool changed{};
    };

    [[nodiscard]] static bool CaptureOriginalBounds(
        Placement& placement) noexcept {
        RECT bounds{};
        if (placement.window == nullptr
            || GetWindowRect(placement.window, &bounds) == FALSE) {
            return false;
        }
        const HWND parent = GetParent(placement.window);
        POINT top_left{bounds.left, bounds.top};
        POINT bottom_right{bounds.right, bounds.bottom};
        if (parent != nullptr
            && (ScreenToClient(parent, &top_left) == FALSE
                || ScreenToClient(parent, &bottom_right) == FALSE)) {
            return false;
        }
        placement.original_x = top_left.x;
        placement.original_y = top_left.y;
        placement.original_width = bottom_right.x - top_left.x;
        placement.original_height = bottom_right.y - top_left.y;
        return true;
    }

    [[nodiscard]] bool HasFinalGeometry() const noexcept {
        for (std::size_t index = 0U; index < placement_count_; ++index) {
            const Placement& placement = placements_[index];
            if (!PaneWindowHasBounds(
                    placement.window,
                    placement.x,
                    placement.y,
                    placement.width,
                    placement.height)) {
                return false;
            }
        }
        return true;
    }

    [[nodiscard]] bool RestoreOriginalGeometry() const noexcept {
        bool restored = true;
        for (std::size_t index = 0U; index < placement_count_; ++index) {
            const Placement& placement = placements_[index];
            if (!placement.changed) {
                continue;
            }
            const bool positioned = SetWindowPos(
                placement.window,
                nullptr,
                placement.original_x,
                placement.original_y,
                placement.original_width,
                placement.original_height,
                kPlacementFlags)
                != FALSE;
            restored = positioned && restored;
        }
        // Do not equate successful SetWindowPos calls with a successful
        // rollback. Common Controls can normalize dimensions and a target can
        // disappear during failure recovery, so verify the exact geometry that
        // was captured before the transaction.
        for (std::size_t index = 0U; index < placement_count_; ++index) {
            const Placement& placement = placements_[index];
            if (!placement.changed) {
                continue;
            }
            Placement current{};
            current.window = placement.window;
            const bool captured = CaptureOriginalBounds(current);
            const bool exact = captured
                && current.original_x == placement.original_x
                && current.original_y == placement.original_y
                && current.original_width == placement.original_width
                && current.original_height == placement.original_height;
            restored = exact && restored;
        }
        return restored;
    }

    static constexpr UINT kPlacementFlags = SWP_NOACTIVATE
        | SWP_NOOWNERZORDER | SWP_NOZORDER | SWP_NOREDRAW | SWP_NOCOPYBITS;

    HWND dialog_{};
    std::array<Placement, kPaneDialogLayoutCapacity> placements_{};
    std::size_t placement_count_{};
    bool valid_{};
    bool overflowed_{};
    bool committed_{};
    bool rollback_succeeded_{true};
};

inline bool PlacePaneDialogControl(
    PaneDialogLayoutPlan& plan,
    int control,
    int x,
    int y,
    int width,
    int height) noexcept {
    return plan.PlaceControl(control, x, y, width, height);
}

inline void PlacePaneDialogControl(
    HWND dialog,
    int control,
    int x,
    int y,
    int width,
    int height,
    bool redraw = true) noexcept {
    const HWND child = GetDlgItem(dialog, control);
    if (child == nullptr) {
        return;
    }
    if (PaneWindowHasBounds(child, x, y, width, height)) {
        return;
    }
    UINT flags = SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER;
    if (!redraw) {
        flags |= SWP_NOREDRAW;
    }
    SetWindowPos(
        child,
        nullptr,
        x,
        y,
        std::max(0, width),
        std::max(0, height),
        flags);
}

inline int MeasurePaneButtonTextWidth(
    HWND button, std::wstring_view text, UINT dpi) noexcept {
    if (button == nullptr) {
        return 0;
    }
    HDC device = GetDC(button);
    if (device == nullptr) {
        return 0;
    }
    const HFONT font = reinterpret_cast<HFONT>(
        SendMessageW(button, WM_GETFONT, 0, 0));
    const HGDIOBJ previous = font == nullptr ? nullptr : SelectObject(device, font);
    SIZE extent{};
    const bool measured = text.size() <= static_cast<std::size_t>(INT_MAX)
        && GetTextExtentPoint32W(
               device,
               text.data(),
               static_cast<int>(text.size()),
               &extent) != FALSE;
    if (previous != nullptr) {
        SelectObject(device, previous);
    }
    ReleaseDC(button, device);
    const int padding = MulDiv(20, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
    return measured ? extent.cx + padding : 0;
}

inline int PaneButtonIdealWidthAtDpi(
    HWND dialog, int control, UINT requested_dpi) noexcept {
    const HWND button = GetDlgItem(dialog, control);
    if (button == nullptr) {
        return 0;
    }
    const UINT window_dpi = GetDpiForWindow(button);
    const UINT dpi = requested_dpi != 0U
        ? requested_dpi
        : (window_dpi == 0U ? 96U : window_dpi);
    const DWORD style = static_cast<DWORD>(
        GetWindowLongPtrW(button, GWL_STYLE));
    if ((style & BS_ICON) != 0U) {
        return MulDiv(32, static_cast<int>(dpi), 96);
    }
    int text_length = GetWindowTextLengthW(button);
    if (text_length < 0) {
        text_length = 0;
    }
    std::wstring text;
    try {
        text.resize(static_cast<std::size_t>(text_length) + 1U, L'\0');
    } catch (const std::bad_alloc&) {
        return ScalePaneDip(dialog, 32);
    }
    const int copied = GetWindowTextW(button, text.data(), text_length + 1);
    text.resize(static_cast<std::size_t>(std::max(0, copied)));
    SIZE ideal{};
    const int common_controls_width = SendMessageW(
        button, BCM_GETIDEALSIZE, 0, reinterpret_cast<LPARAM>(&ideal)) != FALSE
        ? ideal.cx
        : 0;
    return std::max({
        ScalePaneDip(dialog, 32),
        common_controls_width,
        MeasurePaneButtonTextWidth(button, text, dpi)});
}

inline int PaneButtonIdealWidth(HWND dialog, int control) noexcept {
    return PaneButtonIdealWidthAtDpi(dialog, control, 0U);
}

inline bool PaneButtonTextFits(HWND dialog, int control) noexcept {
    const HWND button = GetDlgItem(dialog, control);
    RECT bounds{};
    return button != nullptr && GetClientRect(button, &bounds) != FALSE
        && bounds.right - bounds.left >= PaneButtonIdealWidth(dialog, control);
}

inline std::size_t PaneButtonRowCount(
    HWND dialog,
    std::span<const int> controls,
    int available_width,
    int gap,
    UINT dpi = 0U) noexcept {
    if (controls.empty() || available_width <= 0) {
        return controls.empty() ? 0U : controls.size();
    }
    std::size_t rows = 1U;
    int used{};
    for (const int control : controls) {
        const int ideal = std::min(
            available_width, PaneButtonIdealWidthAtDpi(dialog, control, dpi));
        if (used != 0 && used + gap + ideal > available_width) {
            ++rows;
            used = 0;
        }
        used += (used == 0 ? 0 : gap) + ideal;
    }
    return rows;
}

inline std::size_t PlacePaneButtonRows(
    PaneDialogLayoutPlan& plan,
    std::span<const int> controls,
    int x,
    int y,
    int available_width,
    int row_height,
    int gap,
    UINT dpi = 0U) noexcept {
    const HWND dialog = plan.Dialog();
    std::size_t first{};
    std::size_t row{};
    while (first < controls.size()) {
        std::size_t last = first;
        int ideal_total{};
        while (last < controls.size()) {
            const int ideal = std::min(
                available_width,
                PaneButtonIdealWidthAtDpi(dialog, controls[last], dpi));
            const int candidate = ideal_total
                + (last == first ? 0 : gap) + ideal;
            if (last != first && candidate > available_width) {
                break;
            }
            ideal_total = candidate;
            ++last;
        }
        const int count = static_cast<int>(last - first);
        const int extra = std::max(0, available_width - ideal_total);
        int cursor = x;
        int distributed{};
        for (std::size_t index = first; index < last; ++index) {
            const int share = count == 0 ? 0 : extra / count
                + (static_cast<int>(index - first) < extra % count ? 1 : 0);
            distributed += share;
            int width = std::min(
                available_width,
                PaneButtonIdealWidthAtDpi(dialog, controls[index], dpi)) + share;
            if (index + 1U == last) {
                width += extra - distributed;
            }
            PlacePaneDialogControl(
                plan,
                controls[index],
                cursor,
                y + static_cast<int>(row) * (row_height + gap),
                width,
                row_height);
            cursor += width + gap;
        }
        first = last;
        ++row;
    }
    return row;
}

inline std::size_t PlacePaneButtonRows(
    HWND dialog,
    std::span<const int> controls,
    int x,
    int y,
    int available_width,
    int row_height,
    int gap,
    UINT dpi = 0U,
    bool redraw = true) noexcept {
    std::size_t first{};
    std::size_t row{};
    while (first < controls.size()) {
        std::size_t last = first;
        int ideal_total{};
        while (last < controls.size()) {
            const int ideal = std::min(
                available_width,
                PaneButtonIdealWidthAtDpi(dialog, controls[last], dpi));
            const int candidate = ideal_total
                + (last == first ? 0 : gap) + ideal;
            if (last != first && candidate > available_width) {
                break;
            }
            ideal_total = candidate;
            ++last;
        }
        const int count = static_cast<int>(last - first);
        const int extra = std::max(0, available_width - ideal_total);
        int cursor = x;
        int distributed{};
        for (std::size_t index = first; index < last; ++index) {
            const int share = count == 0 ? 0 : extra / count
                + (static_cast<int>(index - first) < extra % count ? 1 : 0);
            distributed += share;
            int width = std::min(
                available_width,
                PaneButtonIdealWidthAtDpi(dialog, controls[index], dpi)) + share;
            if (index + 1U == last) {
                width += extra - distributed;
            }
            PlacePaneDialogControl(
                dialog,
                controls[index],
                cursor,
                y + static_cast<int>(row) * (row_height + gap),
                width,
                row_height,
                redraw);
            cursor += width + gap;
        }
        first = last;
        ++row;
    }
    return row;
}

inline void PlacePaneTargetRow(
    PaneDialogLayoutPlan& plan,
    int target_control,
    int button_control,
    int margin,
    int y,
    int available_width,
    int target_y_offset,
    int target_height,
    int button_height,
    int gap) noexcept {
    const int button_width = std::min(
        std::max(0, available_width),
        PaneButtonIdealWidth(plan.Dialog(), button_control));
    PlacePaneDialogControl(
        plan,
        button_control,
        margin + std::max(0, available_width - button_width),
        y,
        button_width,
        button_height);
    PlacePaneDialogControl(
        plan,
        target_control,
        margin,
        y + target_y_offset,
        std::max(0, available_width - button_width - gap),
        target_height);
}

inline void PlacePaneTargetRow(
    HWND dialog,
    int target_control,
    int button_control,
    int margin,
    int y,
    int available_width,
    int target_y_offset,
    int target_height,
    int button_height,
    int gap,
    bool redraw = true) noexcept {
    const int button_width = std::min(
        std::max(0, available_width),
        PaneButtonIdealWidth(dialog, button_control));
    PlacePaneDialogControl(
        dialog,
        button_control,
        margin + std::max(0, available_width - button_width),
        y,
        button_width,
        button_height,
        redraw);
    PlacePaneDialogControl(
        dialog,
        target_control,
        margin,
        y + target_y_offset,
        std::max(0, available_width - button_width - gap),
        target_height,
        redraw);
}

}  // namespace inkpod::windows::ui::panes
