#include "sequence_pane.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cwchar>
#include <utility>

#include "app/resource.h"

namespace inkpod::windows::ui::panes {
namespace {

void Dispatch(SequencePaneDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

void DrawThumbnail(
    HDC dc, const RECT& destination, const SequencePaneCellView& cell) noexcept {
    if (cell.thumbnail_width == 0U || cell.thumbnail_height == 0U
        || cell.thumbnail_width > 64U || cell.thumbnail_height > 64U
        || cell.thumbnail_stride_bytes != cell.thumbnail_width * 4U
        || cell.thumbnail_rgba.size()
            != static_cast<std::size_t>(cell.thumbnail_stride_bytes)
                * cell.thumbnail_height) {
        FillRect(dc, &destination, GetSysColorBrush(COLOR_3DFACE));
        return;
    }
    std::array<std::uint8_t, 64U * 64U * 4U> bgra{};
    for (std::uint32_t y = 0U; y < cell.thumbnail_height; ++y) {
        for (std::uint32_t x = 0U; x < cell.thumbnail_width; ++x) {
            const std::size_t offset = static_cast<std::size_t>(y)
                    * cell.thumbnail_stride_bytes
                + static_cast<std::size_t>(x) * 4U;
            const std::uint32_t alpha = cell.thumbnail_rgba[offset + 3U];
            const std::uint32_t checker = ((x / 8U) + (y / 8U)) % 2U == 0U
                ? 248U
                : 216U;
            const auto composite = [alpha, checker](std::uint8_t channel) {
                return static_cast<std::uint8_t>(
                    (std::uint32_t{channel} * alpha
                         + checker * (255U - alpha) + 127U)
                    / 255U);
            };
            bgra[offset] = composite(cell.thumbnail_rgba[offset + 2U]);
            bgra[offset + 1U] = composite(cell.thumbnail_rgba[offset + 1U]);
            bgra[offset + 2U] = composite(cell.thumbnail_rgba[offset]);
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
    DrawThumbnail(item.hDC, thumbnail, cell);

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
                    Dispatch(*state, IDM_SEQ_IMPORT);
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
                    ShowWindow(dialog, SW_HIDE);
                    return TRUE;
                default:
                    break;
            }
            break;
        case WM_CLOSE:
            ShowWindow(dialog, SW_HIDE);
            return TRUE;
        case WM_NCDESTROY:
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
        IDC_SEQUENCE_EMPTY,
        has_sequence ? L"" : state->view.empty_text.c_str());
}

}  // namespace inkpod::windows::ui::panes
