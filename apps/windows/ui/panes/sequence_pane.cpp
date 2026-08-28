#include "ui/ui_resources.h"

#include "sequence_pane.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cwchar>
#include <new>
#include <utility>

#include <commctrl.h>
#include <windowsx.h>

#include "app/resource.h"
#include "pane_dialog_layout.h"
#include "ui/icons/fluent_icons.h"
#include "ui/localization.h"

namespace inkpod::windows::ui::panes {
namespace {

void Dispatch(SequencePaneDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

constexpr int kMaximumSequenceRowPixels = 255;
constexpr int kSequenceColumnDip = 112;

void LayoutSequencePane(HWND dialog, bool redraw = true) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const int margin = ScalePaneDip(dialog, 6);
    const int gap = ScalePaneDip(dialog, 4);
    const int header_height = ScalePaneDip(dialog, 24);
    const int line_height = std::max(ScalePaneDip(dialog, 18),
        PaneControlTextHeight(GetDlgItem(dialog, IDC_SEQUENCE_TARGET)));
    const int button_height = PaneReadableControlHeight(
        dialog, IDC_SEQUENCE_IMPORT, 24, 4);
    const int width = static_cast<int>(client.right - client.left);
    const int height = static_cast<int>(client.bottom - client.top);
    const int content_width = std::max(0, width - margin * 2);
    const int import_width = std::min(content_width,
        PaneButtonIdealWidth(dialog, IDC_SEQUENCE_IMPORT));

    PlacePaneTargetRow(
        dialog,
        IDC_SEQUENCE_TARGET,
        IDC_SEQUENCE_PIN,
        margin,
        margin,
        std::max(0, content_width - import_width - gap),
        ScalePaneDip(dialog, 4),
        line_height,
        header_height,
        gap,
        false);
    PlacePaneDialogControl(
        dialog, IDC_SEQUENCE_IMPORT,
        std::max(margin, width - margin - import_width), margin,
        import_width, button_height, false);
    const std::array<int, 4U> edit_controls{
        IDC_SEQUENCE_REMOVE,
        IDC_SEQUENCE_MOVE_UP,
        IDC_SEQUENCE_MOVE_DOWN,
        IDC_SEQUENCE_RENUMBER};
    const auto* state = reinterpret_cast<const SequencePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    const bool cut_editable = state != nullptr && state->view.cut_editable;
    const std::size_t edit_rows = cut_editable
        ? PaneButtonRowCount(dialog, edit_controls, content_width, gap)
        : 0U;
    const int edit_buttons_height = static_cast<int>(edit_rows) * button_height
        + std::max(0, static_cast<int>(edit_rows) - 1) * gap;
    const int edit_buttons_top = std::max(
        margin + header_height + gap,
        height - margin - edit_buttons_height);
    const int list_top = margin + std::max(header_height, button_height) + gap;
    const int list_bottom = cut_editable ? edit_buttons_top - gap : height - margin;
    const UINT dpi = GetDpiForWindow(dialog);
    const int list_frame = GetSystemMetricsForDpi(SM_CYHSCROLL, dpi)
        + 2 * GetSystemMetricsForDpi(SM_CYBORDER, dpi);
    // A Win32 ListBox item is at most 255 device pixels high. Bound the actual
    // ListBox as well as the row so a tall/high-DPI pane never wraps into rows.
    const int list_height = std::clamp(
        list_bottom - list_top, 0, kMaximumSequenceRowPixels + list_frame);
    const HWND list = GetDlgItem(dialog, IDC_SEQUENCE_CELLS);
    const LRESULT first_visible = SendMessageW(list, LB_GETTOPINDEX, 0, 0);
    const bool geometry_changed = !PaneWindowHasBounds(
        list, margin, list_top, content_width, list_height);
    if (geometry_changed) {
        SendMessageW(list, WM_SETREDRAW, FALSE, 0);
    }
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_CELLS,
        margin,
        list_top,
        content_width,
        list_height,
        false);
    RECT list_client{};
    if (GetClientRect(list, &list_client) != FALSE) {
        const int row_height = std::clamp(
            static_cast<int>(list_client.bottom - list_client.top),
            1, kMaximumSequenceRowPixels);
        if (SendMessageW(list, LB_GETITEMHEIGHT, 0, 0) != row_height) {
            SendMessageW(list, LB_SETITEMHEIGHT, 0, row_height);
        }
    }
    SendMessageW(list, LB_SETCOLUMNWIDTH,
        static_cast<WPARAM>(ScalePaneDip(dialog, kSequenceColumnDip)), 0);
    if (first_visible >= 0
        && first_visible < SendMessageW(list, LB_GETCOUNT, 0, 0)) {
        SendMessageW(list, LB_SETTOPINDEX, first_visible, 0);
    }
    if (geometry_changed) {
        SendMessageW(list, WM_SETREDRAW, TRUE, 0);
    }
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_EMPTY,
        margin + gap,
        list_top + std::max(0, (list_height - line_height) / 2),
        std::max(0, width - margin * 2 - gap * 2),
        line_height,
        false);
    if (cut_editable) {
        int action_width = gap * (static_cast<int>(edit_controls.size()) - 1);
        for (const int control : edit_controls) {
            action_width += PaneButtonIdealWidth(dialog, control);
        }
        PlacePaneButtonRows(
            dialog, edit_controls, margin, edit_buttons_top,
            std::min(content_width, action_width), button_height, gap, 0U, false);
    }
    if (redraw) {
        CompletePaneDialogResize(dialog);
    }
}

void StepSequenceCell(SequencePaneDialogState& state, bool next) noexcept {
    if (!state.view.target_available || state.view.cells.empty()) {
        return;
    }
    if (!state.view.cut_editable) {
        Dispatch(state, next ? IDM_SEQ_NEXT : IDM_SEQ_PREVIOUS);
        return;
    }
    // Cut membership is an explicitly ordered list of independent Cell files,
    // not the active Cell Core's naturally ordered raster sequence.
    const auto count = static_cast<std::uint32_t>(state.view.cells.size());
    std::uint32_t target = state.view.active_index;
    if (target >= count) {
        target = next ? 0U : count - 1U;
    } else if (next && target + 1U < count) {
        ++target;
    } else if (!next && target > 0U) {
        --target;
    } else if (state.view.wrap_navigation && count > 1U) {
        target = next ? 0U : count - 1U;
    } else {
        return;
    }
    state.activate_cell(state.context, state.view.cells[target].sequence_index);
}

bool SameSequenceCell(
    const SequencePaneCellView& left, const SequencePaneCellView& right) noexcept {
    return left.document_uuid_high == right.document_uuid_high
        && left.document_uuid_low == right.document_uuid_low;
}

bool ReplaceSequenceItems(
    HWND list, const std::vector<std::wstring>& labels) noexcept {
    SendMessageW(list, LB_RESETCONTENT, 0, 0);
    for (const auto& label : labels) {
        const LRESULT item = SendMessageW(
            list, LB_ADDSTRING, 0, reinterpret_cast<LPARAM>(label.c_str()));
        if (item == LB_ERR || item == LB_ERRSPACE) {
            return false;
        }
    }
    return true;
}

void SelectCommittedCell(
    HWND list, std::uint32_t selected, LRESULT first_visible,
    bool ensure_visible) noexcept {
    const LRESULT desired = selected == UINT32_MAX
        ? LB_ERR : static_cast<LRESULT>(selected);
    if (SendMessageW(list, LB_GETCURSEL, 0, 0) != desired) {
        SendMessageW(list, LB_SETCURSEL, static_cast<WPARAM>(desired), 0);
    }
    const LRESULT count = SendMessageW(list, LB_GETCOUNT, 0, 0);
    if (count > 0) {
        SendMessageW(list, LB_SETTOPINDEX,
            static_cast<WPARAM>(std::clamp(first_visible, LRESULT{0}, count - 1)), 0);
    }
    if (!ensure_visible || desired < 0 || desired >= count) {
        return;
    }
    RECT item{};
    RECT client{};
    if (SendMessageW(list, LB_GETITEMRECT, selected,
            reinterpret_cast<LPARAM>(&item)) == LB_ERR
        || GetClientRect(list, &client) == FALSE) {
        return;
    }
    if (item.left < client.left) {
        SendMessageW(list, LB_SETTOPINDEX, selected, 0);
    } else if (item.right > client.right) {
        const int columns = std::max(1,
            static_cast<int>(client.right - client.left)
                / std::max(1, static_cast<int>(item.right - item.left)));
        const LRESULT first = std::max(LRESULT{0}, desired - columns + 1);
        SendMessageW(list, LB_SETTOPINDEX, static_cast<WPARAM>(first), 0);
    }
}

LRESULT CALLBACK SequenceListSubclass(
    HWND list,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<SequencePaneDialogState*>(reference);
    if (state == nullptr) {
        return DefSubclassProc(list, message, wparam, lparam);
    }
    if (message == WM_GETDLGCODE) {
        return DefSubclassProc(list, message, wparam, lparam) | DLGC_WANTARROWS;
    }
    if (message == WM_MOUSEHWHEEL || message == WM_MOUSEWHEEL) {
        const int delta = GET_WHEEL_DELTA_WPARAM(wparam);
        state->wheel_remainder += message == WM_MOUSEHWHEEL ? delta : -delta;
        while (state->wheel_remainder >= WHEEL_DELTA) {
            SendMessageW(list, WM_HSCROLL, SB_LINERIGHT, 0);
            state->wheel_remainder -= WHEEL_DELTA;
        }
        while (state->wheel_remainder <= -WHEEL_DELTA) {
            SendMessageW(list, WM_HSCROLL, SB_LINELEFT, 0);
            state->wheel_remainder += WHEEL_DELTA;
        }
        return 0;
    }
    if (message == WM_KEYDOWN) {
        const bool control = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
        const bool alt = (GetKeyState(VK_MENU) & 0x8000) != 0;
        const bool shift = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
        if (!control && !alt && !shift
            && (wparam == VK_LEFT || wparam == VK_RIGHT)) {
            const bool retain_focus = GetFocus() == list;
            StepSequenceCell(*state, wparam == VK_RIGHT);
            if (retain_focus && IsWindow(list) != FALSE && IsWindowVisible(list) != FALSE) {
                SetFocus(list);
            }
            return 0;
        }
        if (!state->view.cut_editable) {
            return DefSubclassProc(list, message, wparam, lparam);
        }
        if (wparam == VK_INSERT) {
            Dispatch(*state, IDM_CUT_SEQUENCE_ADD);
            return 0;
        }
        if (wparam == VK_DELETE) {
            Dispatch(*state, IDM_CUT_SEQUENCE_REMOVE);
            return 0;
        }
        if (alt && (wparam == VK_LEFT || wparam == VK_UP)) {
            Dispatch(*state, IDM_CUT_SEQUENCE_MOVE_UP);
            return 0;
        }
        if (alt && (wparam == VK_RIGHT || wparam == VK_DOWN)) {
            Dispatch(*state, IDM_CUT_SEQUENCE_MOVE_DOWN);
            return 0;
        }
        if (control && wparam == 'R') {
            Dispatch(*state, IDM_CUT_SEQUENCE_RENUMBER);
            return 0;
        }
        if (control && wparam == 'Z') {
            Dispatch(*state, IDM_CUT_UNDO);
            return 0;
        }
        if (control && wparam == 'Y') {
            Dispatch(*state, IDM_CUT_REDO);
            return 0;
        }
    }
    if (message == WM_CANCELMODE || message == WM_CAPTURECHANGED) {
        state->drag_index = UINT32_MAX;
    }
    if (state->view.cut_editable && message == WM_LBUTTONDOWN) {
        const DWORD item = static_cast<DWORD>(SendMessageW(
            list, LB_ITEMFROMPOINT, 0, lparam));
        state->drag_index = HIWORD(item) == 0
            ? static_cast<std::uint32_t>(LOWORD(item))
            : UINT32_MAX;
    } else if (state->view.cut_editable && message == WM_MOUSEMOVE
               && state->drag_index != UINT32_MAX
               && (wparam & MK_LBUTTON) != 0U) {
        const DWORD item = static_cast<DWORD>(SendMessageW(
            list, LB_ITEMFROMPOINT, 0, lparam));
        if (HIWORD(item) == 0) {
            SendMessageW(list, LB_SETCURSEL, LOWORD(item), 0);
        }
    } else if (message == WM_LBUTTONUP
               && state->drag_index != UINT32_MAX) {
        const std::uint32_t source = state->drag_index;
        state->drag_index = UINT32_MAX;
        const DWORD item = static_cast<DWORD>(SendMessageW(
            list, LB_ITEMFROMPOINT, 0, lparam));
        // LOWORD is the nearest item even when the captured pointer is outside.
        const std::uint32_t destination = LOWORD(item);
        if (destination < state->view.cells.size() && source != destination
            && state->reorder_cell != nullptr) {
            state->reorder_cell(
                state->context, source, destination);
            return 0;
        }
    }
    return DefSubclassProc(list, message, wparam, lparam);
}

void DrawThumbnail(
    HDC dc,
    const RECT& destination,
    const SequencePaneCellView& cell,
    ThumbnailCache* cache) noexcept {
    ThumbnailImageView image{};
    if (cell.thumbnail_width == 0U || cell.thumbnail_height == 0U
        || cell.thumbnail_width > 64U || cell.thumbnail_height > 64U
        || cell.thumbnail_stride_bytes != cell.thumbnail_width * 4U
        || cache == nullptr || !cache->Get(cell.thumbnail_key, image)
        || image.layout != ThumbnailPixelLayout::Rgba8
        || image.width != cell.thumbnail_width
        || image.height != cell.thumbnail_height
        || image.stride_bytes != cell.thumbnail_stride_bytes) {
        FillRect(dc, &destination, GetSysColorBrush(COLOR_3DFACE));
        return;
    }
    std::array<std::uint8_t, 64U * 64U * 4U> bgra{};
    for (std::uint32_t y = 0U; y < cell.thumbnail_height; ++y) {
        for (std::uint32_t x = 0U; x < cell.thumbnail_width; ++x) {
            const std::size_t offset = static_cast<std::size_t>(y)
                    * cell.thumbnail_stride_bytes
                + static_cast<std::size_t>(x) * 4U;
            const std::uint32_t alpha = image.pixels[offset + 3U];
            const std::uint32_t checker = ((x / 8U) + (y / 8U)) % 2U == 0U
                ? 248U
                : 216U;
            const auto composite = [alpha, checker](std::uint8_t channel) {
                return static_cast<std::uint8_t>(
                    (std::uint32_t{channel} * alpha
                         + checker * (255U - alpha) + 127U)
                    / 255U);
            };
            bgra[offset] = composite(image.pixels[offset + 2U]);
            bgra[offset + 1U] = composite(image.pixels[offset + 1U]);
            bgra[offset + 2U] = composite(image.pixels[offset]);
            bgra[offset + 3U] = 255U;
        }
    }
    BITMAPINFO bitmap{};
    bitmap.bmiHeader.biSize = sizeof(bitmap.bmiHeader);
    bitmap.bmiHeader.biWidth = static_cast<LONG>(cell.thumbnail_width);
    bitmap.bmiHeader.biHeight = -static_cast<LONG>(cell.thumbnail_height);
    bitmap.bmiHeader.biPlanes = 1U;
    bitmap.bmiHeader.biBitCount = 32U;
    bitmap.bmiHeader.biCompression = BI_RGB;
    StretchDIBits(
        dc,
        destination.left,
        destination.top,
        destination.right - destination.left,
        destination.bottom - destination.top,
        0,
        0,
        static_cast<int>(cell.thumbnail_width),
        static_cast<int>(cell.thumbnail_height),
        bgra.data(),
        &bitmap,
        DIB_RGB_COLORS,
        SRCCOPY);
    FrameRect(dc, &destination, GetSysColorBrush(COLOR_3DSHADOW));
}

void DrawCell(
    const DRAWITEMSTRUCT& item, const SequencePaneDialogState& state) noexcept {
    if (item.itemID == UINT32_MAX
        || item.itemID >= state.view.cells.size()) {
        return;
    }
    const auto& cell = state.view.cells[item.itemID];
    const bool selected = (item.itemState & ODS_SELECTED) != 0U;
    FillRect(
        item.hDC,
        &item.rcItem,
        GetSysColorBrush(selected ? COLOR_HIGHLIGHT : COLOR_WINDOW));
    SetBkMode(item.hDC, TRANSPARENT);
    SetTextColor(
        item.hDC,
        GetSysColor(selected ? COLOR_HIGHLIGHTTEXT : COLOR_WINDOWTEXT));

    const int padding = ScalePaneDip(item.hwndItem, 4);
    const int text_height = std::max(ScalePaneDip(item.hwndItem, 16),
        PaneControlTextHeight(item.hwndItem));
    const int available_width = std::max(1,
        static_cast<int>(item.rcItem.right - item.rcItem.left) - padding * 2);
    const int available_height = std::max(1,
        static_cast<int>(item.rcItem.bottom - item.rcItem.top)
            - padding * 3 - text_height);
    const int side = std::max(1, std::min({available_width, available_height,
        ScalePaneDip(item.hwndItem, 64)}));
    const int source_width = std::max(1, static_cast<int>(cell.thumbnail_width));
    const int source_height = std::max(1, static_cast<int>(cell.thumbnail_height));
    const int thumbnail_width = std::max(1,
        source_width >= source_height ? side : side * source_width / source_height);
    const int thumbnail_height = std::max(1,
        source_height >= source_width ? side : side * source_height / source_width);
    const int thumbnail_left = item.rcItem.left
        + (item.rcItem.right - item.rcItem.left - thumbnail_width) / 2;
    RECT thumbnail{
        thumbnail_left,
        item.rcItem.top + padding,
        thumbnail_left + thumbnail_width,
        item.rcItem.top + padding + thumbnail_height};
    DrawThumbnail(item.hDC, thumbnail, cell, state.thumbnail_cache);

    RECT text{
        item.rcItem.left + padding,
        item.rcItem.bottom - padding - text_height,
        item.rcItem.right - padding,
        item.rcItem.bottom - padding};
    DrawTextW(
        item.hDC,
        item.itemID < state.item_labels.size()
            ? state.item_labels[item.itemID].c_str() : cell.name.c_str(),
        -1,
        &text,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    if ((item.itemState & ODS_FOCUS) != 0U) {
        DrawFocusRect(item.hDC, &item.rcItem);
    }
}

INT_PTR CALLBACK SequencePaneProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<SequencePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            return TRUE;
        case WM_SIZE:
        case WM_DPICHANGED_AFTERPARENT:
            LayoutSequencePane(dialog, false);
            CompletePaneDialogResize(dialog);
            return TRUE;
        case WM_MEASUREITEM:
            if (wparam == static_cast<WPARAM>(IDC_SEQUENCE_CELLS)) {
                auto* measure = reinterpret_cast<MEASUREITEMSTRUCT*>(lparam);
                measure->itemHeight = static_cast<UINT>(
                    std::min(kMaximumSequenceRowPixels, ScalePaneDip(dialog, 72)));
                measure->itemWidth = static_cast<UINT>(
                    ScalePaneDip(dialog, kSequenceColumnDip));
                return TRUE;
            }
            break;
        case WM_DRAWITEM:
            if (state != nullptr
                && wparam == static_cast<WPARAM>(IDC_SEQUENCE_CELLS)) {
                DrawCell(*reinterpret_cast<const DRAWITEMSTRUCT*>(lparam), *state);
                return TRUE;
            }
            break;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDC_SEQUENCE_PIN:
                    Dispatch(*state, IDM_SEQUENCE_PIN);
                    return TRUE;
                case IDC_SEQUENCE_IMPORT:
                    Dispatch(
                        *state,
                        state->view.cut_editable
                            ? IDM_CUT_SEQUENCE_ADD
                            : IDM_SEQ_IMPORT);
                    return TRUE;
                case IDC_SEQUENCE_REMOVE:
                    Dispatch(*state, IDM_CUT_SEQUENCE_REMOVE);
                    return TRUE;
                case IDC_SEQUENCE_MOVE_UP:
                    Dispatch(*state, IDM_CUT_SEQUENCE_MOVE_UP);
                    return TRUE;
                case IDC_SEQUENCE_MOVE_DOWN:
                    Dispatch(*state, IDM_CUT_SEQUENCE_MOVE_DOWN);
                    return TRUE;
                case IDC_SEQUENCE_RENUMBER:
                    Dispatch(*state, IDM_CUT_SEQUENCE_RENUMBER);
                    return TRUE;
                case IDC_SEQUENCE_CELLS:
                    if (HIWORD(wparam) == LBN_SELCHANGE) {
                        const LRESULT selected = SendDlgItemMessageW(
                            dialog, IDC_SEQUENCE_CELLS, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR
                            && static_cast<std::size_t>(selected)
                                < state->view.cells.size()) {
                            const std::uint32_t target = state->view.cells[
                                static_cast<std::size_t>(selected)].sequence_index;
                            const HWND list = GetDlgItem(dialog, IDC_SEQUENCE_CELLS);
                            const bool retain_focus = GetFocus() == list;
                            SelectCommittedCell(list, state->view.active_index,
                                SendMessageW(list, LB_GETTOPINDEX, 0, 0), false);
                            state->activate_cell(
                                state->context, target);
                            if (retain_focus && IsWindow(list) != FALSE
                                && IsWindowVisible(list) != FALSE) {
                                SetFocus(list);
                            }
                        }
                    }
                    return TRUE;
                case IDCANCEL:
                    Dispatch(*state, IDM_WINDOW_SEQUENCE);
                    return TRUE;
                default:
                    break;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr) {
                Dispatch(*state, IDM_WINDOW_SEQUENCE);
            }
            return TRUE;
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                GetDlgItem(dialog, IDC_SEQUENCE_CELLS),
                SequenceListSubclass,
                1U);
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

HWND CreateSequencePaneDialog(
    HINSTANCE instance, HWND owner, SequencePaneDialogState& state) noexcept {
    if (state.dispatch_command == nullptr || state.activate_cell == nullptr) {
        return nullptr;
    }
    const HWND dialog = CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_SEQUENCE_PALETTE),
        owner,
        SequencePaneProcedure,
        0);
    if (dialog == nullptr) {
        return nullptr;
    }
    SetWindowLongPtrW(
        dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(&state));
    SetWindowSubclass(
        GetDlgItem(dialog, IDC_SEQUENCE_CELLS),
        SequenceListSubclass,
        1U,
        reinterpret_cast<DWORD_PTR>(&state));
    EnablePaneDialogResizePainting(dialog);
    static_cast<void>(SetPaneIconButton(
        GetDlgItem(dialog, IDC_SEQUENCE_REMOVE), PaneIconId::Delete));
    SetDlgItemTextW(dialog, IDC_SEQUENCE_MOVE_UP,
        UiText(UiStringId::SequenceMoveEarlier));
    SetDlgItemTextW(dialog, IDC_SEQUENCE_MOVE_DOWN,
        UiText(UiStringId::SequenceMoveLater));
    static_cast<void>(SetPaneIconButton(
        GetDlgItem(dialog, IDC_SEQUENCE_MOVE_UP), PaneIconId::Previous));
    static_cast<void>(SetPaneIconButton(
        GetDlgItem(dialog, IDC_SEQUENCE_MOVE_DOWN), PaneIconId::Next));
    LayoutSequencePane(dialog);
    SetWindowTextW(
        GetDlgItem(dialog, IDC_SEQUENCE_CELLS),
        UiText(UiStringId::SequenceAccessibleName));
    return dialog;
}

void UpdateSequencePaneDialog(HWND dialog, SequencePaneView view) noexcept {
    if (dialog == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<SequencePaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    if (state == nullptr) {
        return;
    }
    std::wstring target_text;
    std::vector<std::wstring> item_labels;
    try {
        target_text = view.auto_sequence_truncated
            ? std::wstring(UiText(UiStringId::AutoSequenceTruncated)) + L" — " + view.target_text
            : view.target_text;
        item_labels.reserve(view.cells.size());
        for (const auto& cell : view.cells) {
            item_labels.push_back(std::to_wstring(cell.cell_number) + L"  "
                + cell.name + L" — " + std::to_wstring(cell.width) + L" x "
                + std::to_wstring(cell.height));
        }
    } catch (const std::bad_alloc&) {
        return;
    }
    const HWND list = GetDlgItem(dialog, IDC_SEQUENCE_CELLS);
    const LRESULT old_first = SendMessageW(list, LB_GETTOPINDEX, 0, 0);
    LRESULT first_visible = old_first;
    if (old_first >= 0
        && static_cast<std::size_t>(old_first) < state->view.cells.size()) {
        const auto& old_cell = state->view.cells[static_cast<std::size_t>(old_first)];
        const auto first = std::find_if(view.cells.begin(), view.cells.end(),
            [&old_cell](const SequencePaneCellView& cell) {
                return SameSequenceCell(old_cell, cell);
            });
        if (first != view.cells.end()) {
            first_visible = static_cast<LRESULT>(first - view.cells.begin());
        }
    }
    const bool selection_changed = state->view.active_index != view.active_index
        || (view.active_index < view.cells.size()
            && state->view.active_index < state->view.cells.size()
            && !SameSequenceCell(view.cells[view.active_index],
                state->view.cells[state->view.active_index]));
    // Native item text is also the MSAA/UIA item name. Selection, thumbnail,
    // pin, and geometry updates do not rebuild an unchanged string list.
    const bool items_changed = item_labels != state->item_labels;
    if (items_changed) {
        SendMessageW(list, WM_SETREDRAW, FALSE, 0);
        if (!ReplaceSequenceItems(list, item_labels)) {
            static_cast<void>(ReplaceSequenceItems(list, state->item_labels));
            SelectCommittedCell(list, state->view.active_index, old_first, false);
            SendMessageW(list, WM_SETREDRAW, TRUE, 0);
            CompletePaneDialogResize(dialog);
            return;
        }
    }
    state->view = std::move(view);
    state->item_labels = std::move(item_labels);
    SetDlgItemTextW(dialog, IDC_SEQUENCE_TARGET, target_text.c_str());
    SetDlgItemTextW(
        dialog,
        IDC_SEQUENCE_PIN,
        state->view.pinned ? UiText(UiStringId::ReturnToFollowing)
                           : UiText(UiStringId::PinDocument));
    static_cast<void>(SetPaneIconButton(
        GetDlgItem(dialog, IDC_SEQUENCE_PIN),
        state->view.pinned ? PaneIconId::ReturnToFollowing
                           : PaneIconId::PinDocument));
    SelectCommittedCell(list, state->view.active_index, first_visible, selection_changed);
    if (items_changed) {
        SendMessageW(list, WM_SETREDRAW, TRUE, 0);
    }
    const bool has_sequence = state->view.target_available
        && !state->view.cells.empty();
    EnableWindow(list, has_sequence ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SEQUENCE_PIN),
        state->view.target_available ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SEQUENCE_IMPORT),
        state->view.target_available ? TRUE : FALSE);
    SetDlgItemTextW(
        dialog,
        IDC_SEQUENCE_IMPORT,
        state->view.cut_editable ? UiText(UiStringId::ExistingCellAdd)
                                 : UiText(UiStringId::FileAdd));
    const int show_edit = state->view.cut_editable ? SW_SHOW : SW_HIDE;
    for (const int control : {
             IDC_SEQUENCE_REMOVE,
             IDC_SEQUENCE_MOVE_UP,
             IDC_SEQUENCE_MOVE_DOWN,
             IDC_SEQUENCE_RENUMBER}) {
        ShowWindow(GetDlgItem(dialog, control), show_edit);
        EnableWindow(
            GetDlgItem(dialog, control),
            state->view.cut_editable && has_sequence ? TRUE : FALSE);
    }
    SetDlgItemTextW(
        dialog,
        IDC_SEQUENCE_EMPTY,
        has_sequence ? L"" : state->view.empty_text.c_str());
    ShowWindow(GetDlgItem(dialog, IDC_SEQUENCE_EMPTY), has_sequence ? SW_HIDE : SW_SHOW);
    LayoutSequencePane(dialog, false);
    // The layout keeps the preceding viewport; only a committed active-cell
    // change may subsequently scroll just enough to reveal its frame.
    SelectCommittedCell(list, state->view.active_index, first_visible, selection_changed);
    CompletePaneDialogResize(dialog);
}

bool SequencePaneItemHasThumbnail(HWND dialog, std::size_t index) noexcept {
    auto* state = reinterpret_cast<SequencePaneDialogState*>(
        dialog == nullptr ? 0 : GetWindowLongPtrW(dialog, GWLP_USERDATA));
    ThumbnailImageView image{};
    return state != nullptr && state->thumbnail_cache != nullptr
        && index < state->view.cells.size()
        && state->thumbnail_cache->Peek(
            state->view.cells[index].thumbnail_key, image)
        && image.layout == ThumbnailPixelLayout::Rgba8;
}

}  // namespace inkpod::windows::ui::panes
