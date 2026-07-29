#include "layer_palette.h"

#include <commctrl.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cwchar>
#include <new>
#include <utility>

#include "app/resource.h"
#include "ui/palette_window.h"

namespace inkpod::windows::ui {
namespace {

constexpr int kReferenceDpi = 96;
constexpr int kMargin = 8;
constexpr int kTileHeight = 92;
constexpr int kThumbnailWidth = 80;
constexpr int kThumbnailHeight = 60;
constexpr int kActionWidth = 44;
constexpr int kButtonHeight = 24;
constexpr int kButtonGap = 4;
constexpr int kMinimumWidth = 300;
constexpr int kMinimumHeight = 260;
constexpr UINT_PTR kLayerListSubclass = 1U;
constexpr std::array<UINT, 6U> kActionCommands{
    IDM_LAYER_NEW,
    IDM_LAYER_DUPLICATE,
    IDM_LAYER_DELETE,
    IDM_LAYER_MOVE_UP,
    IDM_LAYER_MOVE_DOWN,
    IDM_LAYER_PROPERTIES};

int ScaleForDpi(int value, UINT dpi) noexcept {
    return MulDiv(
        value,
        static_cast<int>(dpi == 0U ? kReferenceDpi : dpi),
        kReferenceDpi);
}

LayerPaletteDialogState* DialogState(HWND dialog) noexcept {
    return reinterpret_cast<LayerPaletteDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
}

LayerPaletteDialogState* ListState(HWND list) noexcept {
    const HWND dialog = GetParent(list);
    return dialog == nullptr ? nullptr : DialogState(dialog);
}

const wchar_t* LayerKindLabel(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_LAYER_BINARY_COLORING: return L"2値彩色";
        case INKPOD_LAYER_GRAYSCALE_COLORING: return L"階調彩色";
        case INKPOD_LAYER_RASTER: return L"ラスター汎用";
        case INKPOD_LAYER_SELECTION: return L"選択範囲";
        case INKPOD_LAYER_FRAME: return L"フレーム";
        case INKPOD_LAYER_VANISHING_POINT: return L"消失点";
        case INKPOD_LAYER_ADJUSTMENT: return L"調整";
        case INKPOD_LAYER_TEXT: return L"テキスト";
        case INKPOD_LAYER_ANNOTATION: return L"指示";
        case INKPOD_LAYER_VECTOR_COLORING: return L"ベクター彩色";
        default: return L"不明";
    }
}

std::wstring Utf8ToWide(const std::string& text) {
    if (text.empty()) {
        return L"(名称なし)";
    }
    const int required = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        text.data(),
        static_cast<int>(text.size()),
        nullptr,
        0);
    if (required <= 0) {
        return L"(名称を表示できません)";
    }
    std::wstring output(static_cast<std::size_t>(required), L'\0');
    if (MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            text.data(),
            static_cast<int>(text.size()),
            output.data(),
            required)
        != required) {
        return L"(名称を表示できません)";
    }
    return output;
}

void NotifyVisibilityChanged(LayerPaletteDialogState& state) noexcept {
    if (state.visibility_changed != nullptr) {
        state.visibility_changed(state.context);
    }
}

void HidePalette(HWND dialog, LayerPaletteDialogState& state) noexcept {
    SetPaletteWindowShown(dialog, false);
    NotifyVisibilityChanged(state);
}

void LayoutControls(HWND dialog) noexcept {
    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(dialog);
    const int margin = ScaleForDpi(kMargin, dpi);
    const int gap = ScaleForDpi(kButtonGap, dpi);
    const int button_height = ScaleForDpi(kButtonHeight, dpi);
    const int width = std::max(
        0,
        static_cast<int>(client.right - client.left) - margin * 2);
    const int list_height = std::max(
        0,
        static_cast<int>(client.bottom - client.top) - margin * 3
            - button_height);
    const HWND list = GetDlgItem(dialog, IDC_LAYER_LIST);
    if (list != nullptr) {
        SetWindowPos(
            list,
            nullptr,
            margin,
            margin,
            width,
            list_height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
        SendMessageW(
            list,
            LB_SETITEMHEIGHT,
            0,
            static_cast<LPARAM>(ScaleForDpi(kTileHeight, dpi)));
    }
    const int button_width = std::max(
        1,
        (width - gap * (static_cast<int>(kActionCommands.size()) - 1))
            / static_cast<int>(kActionCommands.size()));
    const int y = margin * 2 + list_height;
    for (std::size_t index = 0; index < kActionCommands.size(); ++index) {
        const HWND button = GetDlgItem(dialog, kActionCommands[index]);
        if (button != nullptr) {
            SetWindowPos(
                button,
                nullptr,
                margin + static_cast<int>(index) * (button_width + gap),
                y,
                button_width,
                button_height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
        }
    }
}

bool UpdatePaletteFont(HWND dialog, LayerPaletteDialogState& state) noexcept {
    const int height = -MulDiv(9, static_cast<int>(GetDpiForWindow(dialog)), 72);
    const HFONT replacement = CreateFontW(
        height,
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
    for (const UINT control : kActionCommands) {
        SendDlgItemMessageW(
            dialog,
            control,
            WM_SETFONT,
            reinterpret_cast<WPARAM>(replacement),
            FALSE);
    }
    SendDlgItemMessageW(
        dialog,
        IDC_LAYER_LIST,
        WM_SETFONT,
        reinterpret_cast<WPARAM>(replacement),
        FALSE);
    if (state.font != nullptr) {
        DeleteObject(state.font);
    }
    state.font = replacement;
    return true;
}

void SelectItem(
    HWND list,
    LayerPaletteDialogState& state,
    int index) noexcept {
    if (index < 0 || static_cast<std::size_t>(index) >= state.items.size()) {
        return;
    }
    SendMessageW(list, LB_SETCURSEL, static_cast<WPARAM>(index), 0);
    const std::uint64_t layer_id = state.items[static_cast<std::size_t>(index)].id;
    if (layer_id == state.selected_layer_id) {
        return;
    }
    state.selected_layer_id = layer_id;
    if (!state.updating && state.select_layer != nullptr) {
        state.select_layer(state.context, layer_id);
    }
}

int ItemFromPoint(HWND list, POINT point) noexcept {
    const LRESULT result = SendMessageW(
        list,
        LB_ITEMFROMPOINT,
        0,
        MAKELPARAM(point.x, point.y));
    return HIWORD(result) == 0 ? static_cast<int>(LOWORD(result)) : -1;
}

void DrawThumbnail(
    HDC dc,
    const RECT& bounds,
    const LayerPaletteItem& item,
    UINT dpi) noexcept {
    const int requested_width = ScaleForDpi(kThumbnailWidth, dpi);
    const int requested_height = ScaleForDpi(kThumbnailHeight, dpi);
    RECT frame{
        bounds.left,
        bounds.top
            + std::max(
                0,
                (static_cast<int>(bounds.bottom - bounds.top)
                 - requested_height)
                    / 2),
        bounds.left + requested_width,
        0};
    frame.bottom = frame.top + requested_height;
    FillRect(dc, &frame, reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1));
    FrameRect(dc, &frame, reinterpret_cast<HBRUSH>(COLOR_3DSHADOW + 1));
    if (item.thumbnail_width == 0U || item.thumbnail_height == 0U
        || item.thumbnail_stride_bytes != item.thumbnail_width * 4U
        || item.thumbnail_bgra.size()
            < static_cast<std::size_t>(item.thumbnail_stride_bytes) * item.thumbnail_height) {
        return;
    }
    const int available_width = std::max(1, requested_width - 2);
    const int available_height = std::max(1, requested_height - 2);
    const double scale = std::min(
        static_cast<double>(available_width) / item.thumbnail_width,
        static_cast<double>(available_height) / item.thumbnail_height);
    const int draw_width = std::max(1, static_cast<int>(item.thumbnail_width * scale + 0.5));
    const int draw_height = std::max(1, static_cast<int>(item.thumbnail_height * scale + 0.5));
    BITMAPINFO bitmap{};
    bitmap.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bitmap.bmiHeader.biWidth = static_cast<LONG>(item.thumbnail_width);
    bitmap.bmiHeader.biHeight = -static_cast<LONG>(item.thumbnail_height);
    bitmap.bmiHeader.biPlanes = 1;
    bitmap.bmiHeader.biBitCount = 32;
    bitmap.bmiHeader.biCompression = BI_RGB;
    SetStretchBltMode(dc, HALFTONE);
    SetBrushOrgEx(dc, 0, 0, nullptr);
    StretchDIBits(
        dc,
        frame.left + (requested_width - draw_width) / 2,
        frame.top + (requested_height - draw_height) / 2,
        draw_width,
        draw_height,
        0,
        0,
        static_cast<int>(item.thumbnail_width),
        static_cast<int>(item.thumbnail_height),
        item.thumbnail_bgra.data(),
        &bitmap,
        DIB_RGB_COLORS,
        SRCCOPY);
}

void DrawStatusButton(
    HDC dc,
    RECT bounds,
    const wchar_t* active_label,
    const wchar_t* inactive_label,
    bool active) noexcept {
    DrawFrameControl(
        dc,
        &bounds,
        DFC_BUTTON,
        static_cast<UINT>(DFCS_BUTTONPUSH)
            | (active ? static_cast<UINT>(DFCS_PUSHED) : 0U));
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, GetSysColor(COLOR_BTNTEXT));
    DrawTextW(
        dc,
        active ? active_label : inactive_label,
        -1,
        &bounds,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
}

void DrawLayerItem(
    const DRAWITEMSTRUCT& draw,
    const LayerPaletteDialogState& state) noexcept {
    if (draw.itemID == static_cast<UINT>(-1)
        || static_cast<std::size_t>(draw.itemID) >= state.items.size()) {
        return;
    }
    const LayerPaletteItem& item = state.items[draw.itemID];
    const bool selected = (draw.itemState & ODS_SELECTED) != 0U;
    const COLORREF background = GetSysColor(selected ? COLOR_HIGHLIGHT : COLOR_WINDOW);
    const COLORREF foreground = GetSysColor(selected ? COLOR_HIGHLIGHTTEXT : COLOR_WINDOWTEXT);
    const HBRUSH background_brush = CreateSolidBrush(background);
    if (background_brush != nullptr) {
        FillRect(draw.hDC, &draw.rcItem, background_brush);
        DeleteObject(background_brush);
    }
    RECT inner = draw.rcItem;
    const UINT dpi = GetDpiForWindow(draw.hwndItem);
    const int margin = ScaleForDpi(kMargin, dpi);
    InflateRect(&inner, -margin, -ScaleForDpi(5, dpi));
    DrawThumbnail(draw.hDC, inner, item, dpi);

    const int action_width = ScaleForDpi(kActionWidth, dpi);
    const int thumbnail_width = ScaleForDpi(kThumbnailWidth, dpi);
    RECT text_bounds{
        inner.left + thumbnail_width + margin,
        inner.top,
        inner.right - action_width * 2 - margin,
        inner.bottom};
    SetBkMode(draw.hDC, TRANSPARENT);
    SetTextColor(draw.hDC, foreground);
    HFONT old_font = nullptr;
    if (state.font != nullptr) {
        old_font = static_cast<HFONT>(SelectObject(draw.hDC, state.font));
    }
    RECT line = text_bounds;
    line.bottom = line.top + ScaleForDpi(22, dpi);
    DrawTextW(
        draw.hDC,
        item.name.c_str(),
        -1,
        &line,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    std::array<wchar_t, 96U> detail{};
    _snwprintf_s(
        detail.data(),
        detail.size(),
        _TRUNCATE,
        L"%ls  |  %u プレーン",
        LayerKindLabel(item.kind),
        item.plane_count);
    line.top += ScaleForDpi(24, dpi);
    line.bottom = line.top + ScaleForDpi(20, dpi);
    DrawTextW(draw.hDC, detail.data(), -1, &line, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    _snwprintf_s(
        detail.data(),
        detail.size(),
        _TRUNCATE,
        L"不透明度 %u.%u%%  |  上から %u",
        item.opacity_milli / 10U,
        item.opacity_milli % 10U,
        item.index + 1U);
    line.top += ScaleForDpi(22, dpi);
    line.bottom = line.top + ScaleForDpi(20, dpi);
    DrawTextW(draw.hDC, detail.data(), -1, &line, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    if (old_font != nullptr) {
        SelectObject(draw.hDC, old_font);
    }

    RECT visible{
        inner.right - action_width * 2,
        inner.top,
        inner.right - action_width - ScaleForDpi(2, dpi),
        inner.bottom};
    RECT editable{
        inner.right - action_width,
        inner.top,
        inner.right,
        inner.bottom};
    DrawStatusButton(
        draw.hDC,
        visible,
        L"表示",
        L"非表示",
        (item.flags & INKPOD_NODE_VISIBLE) != 0U);
    DrawStatusButton(
        draw.hDC,
        editable,
        L"編集",
        L"保護",
        (item.flags & INKPOD_NODE_EDITABLE) != 0U);

    FrameRect(draw.hDC, &draw.rcItem, reinterpret_cast<HBRUSH>(COLOR_3DSHADOW + 1));
    if (state.drop_index == static_cast<int>(draw.itemID)) {
        RECT marker = draw.rcItem;
        marker.bottom = marker.top + std::max(2, ScaleForDpi(2, dpi));
        FillRect(draw.hDC, &marker, GetSysColorBrush(COLOR_HIGHLIGHT));
    }
    if ((draw.itemState & ODS_FOCUS) != 0U) {
        DrawFocusRect(draw.hDC, &draw.rcItem);
    }
}

LRESULT CALLBACK LayerListSubclassProcedure(
    HWND list,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR) noexcept {
    LayerPaletteDialogState* state = ListState(list);
    switch (message) {
        case WM_LBUTTONDOWN: {
            if (state == nullptr) {
                break;
            }
            const POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
            const int index = ItemFromPoint(list, point);
            if (index < 0) {
                break;
            }
            SelectItem(list, *state, index);
            RECT client{};
            GetClientRect(list, &client);
            const int action_width = ScaleForDpi(kActionWidth, GetDpiForWindow(list));
            if (point.x >= client.right - action_width) {
                state->dispatch_command(state->context, IDM_LAYER_TOGGLE_EDITABLE);
                return 0;
            }
            if (point.x >= client.right - action_width * 2) {
                state->dispatch_command(state->context, IDM_LAYER_TOGGLE_VISIBLE);
                return 0;
            }
            state->drag_source = index;
            state->drop_index = index;
            SetCapture(list);
            InvalidateRect(list, nullptr, FALSE);
            break;
        }
        case WM_MOUSEMOVE:
            if (state != nullptr && GetCapture() == list && state->drag_source >= 0) {
                const POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                const int index = ItemFromPoint(list, point);
                if (index >= 0 && index != state->drop_index) {
                    state->drop_index = index;
                    InvalidateRect(list, nullptr, FALSE);
                }
                return 0;
            }
            break;
        case WM_LBUTTONUP:
            if (state != nullptr && GetCapture() == list && state->drag_source >= 0) {
                const int source = state->drag_source;
                const int destination = state->drop_index;
                state->drag_source = -1;
                state->drop_index = -1;
                ReleaseCapture();
                InvalidateRect(list, nullptr, FALSE);
                if (destination >= 0 && source != destination
                    && static_cast<std::size_t>(source) < state->items.size()
                    && state->reorder_layer != nullptr) {
                    state->reorder_layer(
                        state->context,
                        state->items[static_cast<std::size_t>(source)].id,
                        static_cast<std::uint32_t>(destination));
                }
                return 0;
            }
            break;
        case WM_CAPTURECHANGED:
            if (state != nullptr) {
                state->drag_source = -1;
                state->drop_index = -1;
                InvalidateRect(list, nullptr, FALSE);
            }
            break;
        case WM_NCDESTROY:
            RemoveWindowSubclass(list, LayerListSubclassProcedure, kLayerListSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(list, message, wparam, lparam);
}

INT_PTR CALLBACK LayerPaletteDialogProcedure(
    HWND dialog,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    LayerPaletteDialogState* state = DialogState(dialog);
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<LayerPaletteDialogState*>(lparam);
            if (state == nullptr || state->dispatch_command == nullptr
                || state->select_layer == nullptr || state->reorder_layer == nullptr
                || state->visibility_changed == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND list = GetDlgItem(dialog, IDC_LAYER_LIST);
            if (list == nullptr
                || SetWindowSubclass(
                       list,
                       LayerListSubclassProcedure,
                       kLayerListSubclass,
                       0)
                    == FALSE
                || !UpdatePaletteFont(dialog, *state)) {
                DestroyWindow(dialog);
                return TRUE;
            }
            LayoutControls(dialog);
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                HidePalette(dialog, *state);
                return TRUE;
            }
            if (LOWORD(wparam) == IDC_LAYER_LIST) {
                const HWND list = GetDlgItem(dialog, IDC_LAYER_LIST);
                if (HIWORD(wparam) == LBN_SELCHANGE) {
                    SelectItem(
                        list,
                        *state,
                        static_cast<int>(SendMessageW(list, LB_GETCURSEL, 0, 0)));
                    return TRUE;
                }
                if (HIWORD(wparam) == LBN_DBLCLK) {
                    state->dispatch_command(state->context, IDM_LAYER_PROPERTIES);
                    return TRUE;
                }
            }
            if (HIWORD(wparam) == BN_CLICKED
                && std::find(
                       kActionCommands.begin(),
                       kActionCommands.end(),
                       static_cast<UINT>(LOWORD(wparam)))
                    != kActionCommands.end()) {
                state->dispatch_command(state->context, LOWORD(wparam));
                return TRUE;
            }
            break;
        case WM_DRAWITEM:
            if (state != nullptr && wparam == IDC_LAYER_LIST) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawLayerItem(*draw, *state);
                }
                return TRUE;
            }
            break;
        case WM_SIZE:
            LayoutControls(dialog);
            return TRUE;
        case WM_DPICHANGED: {
            const auto* bounds = reinterpret_cast<const RECT*>(lparam);
            if (bounds != nullptr) {
                SetWindowPos(
                    dialog,
                    nullptr,
                    bounds->left,
                    bounds->top,
                    bounds->right - bounds->left,
                    bounds->bottom - bounds->top,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
            }
            if (state != nullptr) {
                UpdatePaletteFont(dialog, *state);
                LayoutControls(dialog);
                InvalidateRect(GetDlgItem(dialog, IDC_LAYER_LIST), nullptr, TRUE);
            }
            return TRUE;
        }
        case WM_GETMINMAXINFO: {
            auto* limits = reinterpret_cast<MINMAXINFO*>(lparam);
            if (limits != nullptr) {
                const UINT dpi = GetDpiForWindow(dialog);
                limits->ptMinTrackSize.x = ScaleForDpi(kMinimumWidth, dpi);
                limits->ptMinTrackSize.y = ScaleForDpi(kMinimumHeight, dpi);
            }
            return TRUE;
        }
        case WM_CLOSE:
            if (state != nullptr) {
                HidePalette(dialog, *state);
            }
            return TRUE;
        case WM_NCDESTROY:
            if (state != nullptr && state->font != nullptr) {
                DeleteObject(state->font);
                state->font = nullptr;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

} // namespace

HWND CreateLayerPaletteDialog(
    HINSTANCE instance,
    HWND owner,
    LayerPaletteDialogState& state) noexcept {
    return CreateDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_LAYER_PALETTE),
        owner,
        LayerPaletteDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

void UpdateLayerPaletteDialog(
    HWND dialog,
    const std::vector<panes::TreePaneNode>& layers,
    std::uint64_t selected_layer_id) noexcept {
    LayerPaletteDialogState* state = DialogState(dialog);
    const HWND list = dialog == nullptr ? nullptr : GetDlgItem(dialog, IDC_LAYER_LIST);
    if (state == nullptr || list == nullptr) {
        return;
    }
    try {
        std::vector<LayerPaletteItem> items;
        items.reserve(layers.size());
        for (const auto& layer : layers) {
            items.push_back(LayerPaletteItem{
                layer.id,
                layer.index,
                layer.kind,
                layer.opacity_milli,
                layer.child_count,
                layer.flags,
                Utf8ToWide(layer.name),
                layer.thumbnail_width,
                layer.thumbnail_height,
                layer.thumbnail_stride_bytes,
                layer.thumbnail_bgra});
        }
        const bool selection_unchanged =
            state->selected_layer_id == selected_layer_id;
        state->updating = true;
        state->items = std::move(items);
        state->selected_layer_id = selected_layer_id;
        const int previous_top = static_cast<int>(
            SendMessageW(list, LB_GETTOPINDEX, 0, 0));
        SendMessageW(list, WM_SETREDRAW, FALSE, 0);
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        int selected_index = -1;
        bool add_failed{};
        for (std::size_t index = 0; index < state->items.size(); ++index) {
            const LRESULT added = SendMessageW(
                list,
                LB_ADDSTRING,
                0,
                reinterpret_cast<LPARAM>(state->items[index].name.c_str()));
            if (added == LB_ERR || added == LB_ERRSPACE) {
                add_failed = true;
                break;
            }
            if (state->items[index].id == selected_layer_id) {
                selected_index = static_cast<int>(index);
            }
        }
        if (add_failed) {
            SendMessageW(list, LB_RESETCONTENT, 0, 0);
            state->items.clear();
            state->selected_layer_id = 0U;
        } else if (selected_index >= 0) {
            SendMessageW(list, LB_SETCURSEL, static_cast<WPARAM>(selected_index), 0);
        }
        if (!state->items.empty() && selection_unchanged) {
            const int maximum_top = static_cast<int>(state->items.size() - 1U);
            SendMessageW(
                list,
                LB_SETTOPINDEX,
                static_cast<WPARAM>(std::clamp(previous_top, 0, maximum_top)),
                0);
        }
        SendMessageW(list, WM_SETREDRAW, TRUE, 0);
        InvalidateRect(list, nullptr, TRUE);
        state->updating = false;
    } catch (const std::bad_alloc&) {
        state->updating = false;
    }
}

void UpdateLayerPaletteCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    if (dialog == nullptr) {
        return;
    }
    for (const UINT command : kActionCommands) {
        const CommandState* state = FindCommandState(states, command);
        const HWND button = GetDlgItem(dialog, command);
        if (state != nullptr && button != nullptr) {
            EnableWindow(button, state->enabled ? TRUE : FALSE);
        }
    }
}

bool LayerPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    if (dialog == nullptr) {
        return false;
    }
    for (const UINT command : kActionCommands) {
        const CommandState* state = FindCommandState(states, command);
        const HWND button = GetDlgItem(dialog, command);
        if (state == nullptr || button == nullptr
            || (IsWindowEnabled(button) != FALSE) != state->enabled) {
            return false;
        }
    }
    return true;
}

std::size_t LayerPaletteItemCount(HWND dialog) noexcept {
    const LayerPaletteDialogState* state = DialogState(dialog);
    return state == nullptr ? 0U : state->items.size();
}

std::uint64_t LayerPaletteSelectedLayer(HWND dialog) noexcept {
    const LayerPaletteDialogState* state = DialogState(dialog);
    return state == nullptr ? 0U : state->selected_layer_id;
}

bool LayerPaletteItemHasThumbnail(HWND dialog, std::size_t index) noexcept {
    const LayerPaletteDialogState* state = DialogState(dialog);
    return state != nullptr && index < state->items.size()
        && state->items[index].thumbnail_width != 0U
        && state->items[index].thumbnail_height != 0U
        && !state->items[index].thumbnail_bgra.empty();
}

} // namespace inkpod::windows::ui
