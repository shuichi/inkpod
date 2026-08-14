#include "ui/ui_resources.h"

#include "locator_pane.h"

#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstddef>

#include "app/resource.h"
#include "pane_dialog_layout.h"
#include "ui/localization.h"

namespace inkpod::windows::ui::panes {
namespace {

void Dispatch(LocatorPaneDialogState& state, UINT command) noexcept {
    if (state.dispatch_command != nullptr) {
        state.dispatch_command(state.context, command);
    }
}

void LayoutLocatorPane(HWND dialog) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const int margin = ScalePaneDip(dialog, 8);
    const int gap = ScalePaneDip(dialog, 6);
    const int header_height = ScalePaneDip(dialog, 24);
    const int pin_width = ScalePaneDip(dialog, 88);
    const int line_height = ScalePaneDip(dialog, 18);
    const int option_height = ScalePaneDip(dialog, 20);
    const int width = std::max(
        0, static_cast<int>(client.right - client.left));
    const int height = std::max(
        0, static_cast<int>(client.bottom - client.top));

    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_PIN,
        std::max(margin, width - margin - pin_width),
        margin,
        pin_width,
        header_height);
    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_TARGET,
        margin,
        margin + ScalePaneDip(dialog, 4),
        std::max(0, width - margin * 3 - pin_width),
        line_height);

    const int options_top = std::max(
        margin + header_height + gap,
        height - margin - option_height);
    const int color_top = std::max(
        margin + header_height + gap,
        options_top - gap - line_height);
    const int selection_top = std::max(
        margin + header_height + gap,
        color_top - line_height);
    const int coordinate_top = std::max(
        margin + header_height + gap,
        selection_top - line_height);
    const int neighborhood_top = margin + header_height + gap;
    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_NEIGHBORHOOD,
        margin,
        neighborhood_top,
        std::max(0, width - margin * 2),
        std::max(0, coordinate_top - gap - neighborhood_top));
    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_COORDINATE,
        margin,
        coordinate_top,
        std::max(0, width - margin * 2),
        line_height);
    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_SELECTION,
        margin,
        selection_top,
        std::max(0, width - margin * 2),
        line_height);
    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_COLOR,
        margin,
        color_top,
        std::max(0, width - margin * 2),
        line_height);
    const int option_width = std::max(0, (width - margin * 2 - gap) / 2);
    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_FIXED,
        margin,
        options_top,
        option_width,
        option_height);
    PlacePaneDialogControl(
        dialog,
        IDC_LOCATOR_AUTOSCROLL,
        margin + option_width + gap,
        options_top,
        std::max(0, width - margin * 2 - gap - option_width),
        option_height);
}

void DrawNeighborhood(
    const DRAWITEMSTRUCT& item,
    const LocatorPaneDialogState& state) noexcept {
    HDC dc = item.hDC;
    RECT bounds = item.rcItem;
    FillRect(dc, &bounds, GetSysColorBrush(COLOR_WINDOW));
    if (state.neighborhood_width == 0U || state.neighborhood_height == 0U
        || state.neighborhood_width > 9U || state.neighborhood_height > 9U) {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, GetSysColor(COLOR_GRAYTEXT));
        DrawTextW(
            dc,
            UiText(UiStringId::LocatorMovePointer),
            -1,
            &bounds,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        return;
    }
    const int width = bounds.right - bounds.left;
    const int height = bounds.bottom - bounds.top;
    const int columns = static_cast<int>(state.neighborhood_width);
    const int rows = static_cast<int>(state.neighborhood_height);
    const int cell = std::max(1, std::min(width / columns, height / rows));
    const int grid_width = cell * columns;
    const int grid_height = cell * rows;
    const int origin_x = bounds.left + (width - grid_width) / 2;
    const int origin_y = bounds.top + (height - grid_height) / 2;
    const HBRUSH brush = static_cast<HBRUSH>(GetStockObject(DC_BRUSH));
    const HPEN pen = static_cast<HPEN>(GetStockObject(DC_PEN));
    const HGDIOBJ old_brush = SelectObject(dc, brush);
    const HGDIOBJ old_pen = SelectObject(dc, pen);
    for (int row = 0; row < rows; ++row) {
        for (int column = 0; column < columns; ++column) {
            const std::size_t offset =
                (static_cast<std::size_t>(row) * state.neighborhood_width
                    + static_cast<std::size_t>(column))
                * 4U;
            const std::uint32_t alpha = state.neighborhood[offset + 3U];
            const std::uint32_t checker = ((row + column) & 1) == 0 ? 240U : 208U;
            const auto composite = [alpha, checker](std::uint8_t channel) {
                return static_cast<std::uint8_t>(
                    (std::uint32_t{channel} * alpha + checker * (255U - alpha) + 127U)
                    / 255U);
            };
            SetDCBrushColor(
                dc,
                RGB(
                    composite(state.neighborhood[offset]),
                    composite(state.neighborhood[offset + 1U]),
                    composite(state.neighborhood[offset + 2U])));
            SetDCPenColor(dc, GetSysColor(COLOR_3DSHADOW));
            Rectangle(
                dc,
                origin_x + column * cell,
                origin_y + row * cell,
                origin_x + (column + 1) * cell + 1,
                origin_y + (row + 1) * cell + 1);
        }
    }
    SetDCPenColor(dc, GetSysColor(COLOR_HIGHLIGHT));
    const int center_x = columns / 2;
    const int center_y = rows / 2;
    Rectangle(
        dc,
        origin_x + center_x * cell,
        origin_y + center_y * cell,
        origin_x + (center_x + 1) * cell + 1,
        origin_y + (center_y + 1) * cell + 1);
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
}

void SelectNeighborhoodPixel(
    HWND dialog, LocatorPaneDialogState& state) noexcept {
    if (!state.fixed_mode || state.select_pixel == nullptr
        || state.neighborhood_width == 0U || state.neighborhood_height == 0U) {
        return;
    }
    const HWND surface = GetDlgItem(dialog, IDC_LOCATOR_NEIGHBORHOOD);
    RECT bounds{};
    POINT point{
        GET_X_LPARAM(GetMessagePos()),
        GET_Y_LPARAM(GetMessagePos())};
    if (surface == nullptr || GetClientRect(surface, &bounds) == FALSE
        || ScreenToClient(surface, &point) == FALSE) {
        return;
    }
    const int width = bounds.right - bounds.left;
    const int height = bounds.bottom - bounds.top;
    const int columns = static_cast<int>(state.neighborhood_width);
    const int rows = static_cast<int>(state.neighborhood_height);
    const int cell = std::max(1, std::min(width / columns, height / rows));
    const int grid_width = cell * columns;
    const int grid_height = cell * rows;
    const int origin_x = (width - grid_width) / 2;
    const int origin_y = (height - grid_height) / 2;
    const int column = (point.x - origin_x) / cell;
    const int row = (point.y - origin_y) / cell;
    if (point.x < origin_x || point.y < origin_y || column < 0 || row < 0
        || column >= columns || row >= rows) {
        return;
    }
    state.select_pixel(
        state.context,
        state.neighborhood_origin_x + column,
        state.neighborhood_origin_y + row);
}

INT_PTR CALLBACK LocatorPaneProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<LocatorPaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            return TRUE;
        case WM_SIZE:
            LayoutLocatorPane(dialog);
            return TRUE;
        case WM_DRAWITEM:
            if (state != nullptr
                && wparam == static_cast<WPARAM>(IDC_LOCATOR_NEIGHBORHOOD)) {
                DrawNeighborhood(
                    *reinterpret_cast<const DRAWITEMSTRUCT*>(lparam), *state);
                return TRUE;
            }
            break;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDC_LOCATOR_PIN:
                    Dispatch(*state, IDM_LOCATOR_PIN);
                    return TRUE;
                case IDC_LOCATOR_FIXED:
                    Dispatch(*state, IDM_LOCATOR_FIXED);
                    return TRUE;
                case IDC_LOCATOR_AUTOSCROLL:
                    Dispatch(*state, IDM_LOCATOR_AUTOSCROLL);
                    return TRUE;
                case IDC_LOCATOR_NEIGHBORHOOD:
                    if (HIWORD(wparam) == STN_CLICKED) {
                        SelectNeighborhoodPixel(dialog, *state);
                    }
                    return TRUE;
                case IDCANCEL:
                    Dispatch(*state, IDM_WINDOW_LOCATOR);
                    return TRUE;
                default:
                    break;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr) {
                Dispatch(*state, IDM_WINDOW_LOCATOR);
            }
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

HWND CreateLocatorPaneDialog(
    HINSTANCE instance, HWND owner, LocatorPaneDialogState& state) noexcept {
    if (state.dispatch_command == nullptr || state.select_pixel == nullptr) {
        return nullptr;
    }
    const HWND dialog = CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_LOCATOR_PALETTE),
        owner,
        LocatorPaneProcedure,
        0);
    if (dialog == nullptr) {
        return nullptr;
    }
    SetWindowLongPtrW(
        dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(&state));
    LayoutLocatorPane(dialog);
    SetWindowTextW(
        GetDlgItem(dialog, IDC_LOCATOR_NEIGHBORHOOD),
        UiText(UiStringId::LocatorAccessibleName));
    return dialog;
}

void UpdateLocatorPaneDialog(
    HWND dialog, const LocatorPaneView& view) noexcept {
    if (dialog == nullptr) {
        return;
    }
    auto* state = reinterpret_cast<LocatorPaneDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    if (state != nullptr) {
        state->neighborhood_width = view.valid ? view.neighborhood_width : 0U;
        state->neighborhood_height = view.valid ? view.neighborhood_height : 0U;
        state->neighborhood_origin_x = view.neighborhood_origin_x;
        state->neighborhood_origin_y = view.neighborhood_origin_y;
        state->neighborhood = view.neighborhood;
        state->fixed_mode = view.fixed_mode;
    }
    SetDlgItemTextW(dialog, IDC_LOCATOR_TARGET, view.target_text.c_str());
    SetDlgItemTextW(dialog, IDC_LOCATOR_COORDINATE, view.coordinate_text.c_str());
    SetDlgItemTextW(dialog, IDC_LOCATOR_SELECTION, view.selection_text.c_str());
    SetDlgItemTextW(dialog, IDC_LOCATOR_COLOR, view.color_text.c_str());
    SetDlgItemTextW(
        dialog,
        IDC_LOCATOR_PIN,
        view.pinned ? UiText(UiStringId::ReturnToFollowing)
                    : UiText(UiStringId::PinDocument));
    CheckDlgButton(dialog, IDC_LOCATOR_FIXED, view.fixed_mode ? BST_CHECKED : BST_UNCHECKED);
    CheckDlgButton(
        dialog,
        IDC_LOCATOR_AUTOSCROLL,
        view.auto_scroll ? BST_CHECKED : BST_UNCHECKED);
    InvalidateRect(GetDlgItem(dialog, IDC_LOCATOR_NEIGHBORHOOD), nullptr, FALSE);
}

}  // namespace inkpod::windows::ui::panes
