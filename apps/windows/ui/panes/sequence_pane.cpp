#include "sequence_pane.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cwchar>
#include <utility>

#include <commctrl.h>

#include "app/resource.h"
#include "pane_dialog_layout.h"

namespace inkpod::windows::ui::panes {
namespace {

void Dispatch(SequencePaneDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

void LayoutSequencePane(HWND dialog) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const int margin = ScalePaneDip(dialog, 8);
    const int gap = ScalePaneDip(dialog, 6);
    const int header_height = ScalePaneDip(dialog, 24);
    const int line_height = ScalePaneDip(dialog, 18);
    const int button_height = ScalePaneDip(dialog, 26);
    const int pin_width = ScalePaneDip(dialog, 88);
    const int nav_width = ScalePaneDip(dialog, 58);
    const int import_width = ScalePaneDip(dialog, 112);
    const int width = static_cast<int>(client.right - client.left);
    const int height = static_cast<int>(client.bottom - client.top);

    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_PIN,
        std::max(margin, width - margin - pin_width),
        margin,
        pin_width,
        header_height);
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_TARGET,
        margin,
        margin + ScalePaneDip(dialog, 4),
        std::max(0, width - margin * 3 - pin_width),
        line_height);
    const int edit_buttons_top = std::max(
        margin + header_height + gap,
        height - margin - button_height);
    const int buttons_top = std::max(
        margin + header_height + gap,
        edit_buttons_top - gap - button_height);
    const int list_top = margin + header_height + gap;
    const int list_height = std::max(0, buttons_top - gap - list_top);
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_CELLS,
        margin,
        list_top,
        std::max(0, width - margin * 2),
        list_height);
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_EMPTY,
        margin + gap,
        list_top + std::max(0, (list_height - line_height) / 2),
        std::max(0, width - margin * 2 - gap * 2),
        line_height);
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_PREVIOUS,
        margin,
        buttons_top,
        nav_width,
        button_height);
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_NEXT,
        margin + nav_width + gap,
        buttons_top,
        nav_width,
        button_height);
    PlacePaneDialogControl(
        dialog,
        IDC_SEQUENCE_IMPORT,
        std::max(margin, width - margin - import_width),
        buttons_top,
        import_width,
        button_height);
    const int edit_width = std::max(1, (width - margin * 2 - gap * 3) / 4);
    PlacePaneDialogControl(
        dialog, IDC_SEQUENCE_REMOVE, margin, edit_buttons_top,
        edit_width, button_height);
    PlacePaneDialogControl(
        dialog, IDC_SEQUENCE_MOVE_UP,
        margin + edit_width + gap, edit_buttons_top,
        edit_width, button_height);
    PlacePaneDialogControl(
        dialog, IDC_SEQUENCE_MOVE_DOWN,
        margin + (edit_width + gap) * 2, edit_buttons_top,
        edit_width, button_height);
    PlacePaneDialogControl(
        dialog, IDC_SEQUENCE_RENUMBER,
        margin + (edit_width + gap) * 3, edit_buttons_top,
        std::max(1, width - margin * 2 - (edit_width + gap) * 3),
        button_height);
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
    if (message == WM_KEYDOWN && state->view.cut_editable) {
        const bool control = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
        const bool alt = (GetKeyState(VK_MENU) & 0x8000) != 0;
        if (wparam == VK_INSERT) {
            Dispatch(*state, IDM_CUT_SEQUENCE_ADD);
            return 0;
        }
        if (wparam == VK_DELETE) {
            Dispatch(*state, IDM_CUT_SEQUENCE_REMOVE);
            return 0;
        }
        if (alt && wparam == VK_UP) {
            Dispatch(*state, IDM_CUT_SEQUENCE_MOVE_UP);
            return 0;
        }
        if (alt && wparam == VK_DOWN) {
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
        if (HIWORD(item) == 0) {
            const std::uint32_t destination = LOWORD(item);
            if (source != destination && state->reorder_cell != nullptr) {
                state->reorder_cell(
                    state->context, source, destination);
                return 0;
            }
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

    const int padding = 6;
    const int available_height = item.rcItem.bottom - item.rcItem.top - padding * 2;
    const int thumbnail_width = std::max(
        1,
        static_cast<int>(cell.thumbnail_width) * available_height
            / std::max(1, static_cast<int>(cell.thumbnail_height)));
    RECT thumbnail{
        item.rcItem.left + padding,
        item.rcItem.top + padding,
        item.rcItem.left + padding + std::min(available_height, thumbnail_width),
        item.rcItem.bottom - padding};
    DrawThumbnail(item.hDC, thumbnail, cell, state.thumbnail_cache);

    RECT text{
        thumbnail.right + 8,
        item.rcItem.top + padding,
        item.rcItem.right - padding,
        item.rcItem.bottom - padding};
    std::array<wchar_t, 384U> label{};
    (void)swprintf_s(
        label.data(),
        label.size(),
        L"%u  %ls\n%u × %u",
        cell.cell_number,
        cell.name.c_str(),
        cell.width,
        cell.height);
    DrawTextW(
        item.hDC,
        label.data(),
        -1,
        &text,
        DT_LEFT | DT_VCENTER | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX);
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
            LayoutSequencePane(dialog);
            return TRUE;
        case WM_MEASUREITEM:
            if (wparam == static_cast<WPARAM>(IDC_SEQUENCE_CELLS)) {
                auto* measure = reinterpret_cast<MEASUREITEMSTRUCT*>(lparam);
                measure->itemHeight = static_cast<UINT>(
                    MulDiv(76, static_cast<int>(GetDpiForWindow(dialog)), 96));
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
                case IDC_SEQUENCE_PREVIOUS:
                    Dispatch(*state, IDM_SEQ_PREVIOUS);
                    return TRUE;
                case IDC_SEQUENCE_NEXT:
                    Dispatch(*state, IDM_SEQ_NEXT);
                    return TRUE;
                case IDC_SEQUENCE_CELLS:
                    if (HIWORD(wparam) == LBN_SELCHANGE) {
                        const LRESULT selected = SendDlgItemMessageW(
                            dialog, IDC_SEQUENCE_CELLS, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR
                            && static_cast<std::size_t>(selected)
                                < state->view.cells.size()) {
                            state->activate_cell(
                                state->context,
                                state->view.cells[static_cast<std::size_t>(selected)]
                                    .sequence_index);
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
    const HWND dialog = CreateDialogParamW(
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
    LayoutSequencePane(dialog);
    SetWindowTextW(
        GetDlgItem(dialog, IDC_SEQUENCE_CELLS),
        L"シーケンスのサムネイル一覧");
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
    state->view = std::move(view);
    SetDlgItemTextW(dialog, IDC_SEQUENCE_TARGET, state->view.target_text.c_str());
    SetDlgItemTextW(
        dialog,
        IDC_SEQUENCE_PIN,
        state->view.pinned ? L"追従へ戻す" : L"文書に固定");
    const HWND list = GetDlgItem(dialog, IDC_SEQUENCE_CELLS);
    SendMessageW(list, WM_SETREDRAW, FALSE, 0);
    SendMessageW(list, LB_RESETCONTENT, 0, 0);
    for (std::size_t index = 0U; index < state->view.cells.size(); ++index) {
        const LRESULT item = SendMessageW(list, LB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L""));
        if (item != LB_ERR && item != LB_ERRSPACE) {
            SendMessageW(list, LB_SETITEMDATA, static_cast<WPARAM>(item), index);
        }
    }
    if (state->view.active_index != UINT32_MAX
        && state->view.active_index < state->view.cells.size()) {
        SendMessageW(list, LB_SETCURSEL, state->view.active_index, 0);
    }
    SendMessageW(list, WM_SETREDRAW, TRUE, 0);
    InvalidateRect(list, nullptr, TRUE);
    const bool has_sequence = state->view.target_available
        && !state->view.cells.empty();
    EnableWindow(list, has_sequence ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SEQUENCE_PIN),
        state->view.target_available ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SEQUENCE_PREVIOUS),
        has_sequence ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SEQUENCE_NEXT),
        has_sequence ? TRUE : FALSE);
    EnableWindow(
        GetDlgItem(dialog, IDC_SEQUENCE_IMPORT),
        state->view.target_available ? TRUE : FALSE);
    SetDlgItemTextW(
        dialog,
        IDC_SEQUENCE_IMPORT,
        state->view.cut_editable ? L"既存セルを追加" : L"ファイルを追加");
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
