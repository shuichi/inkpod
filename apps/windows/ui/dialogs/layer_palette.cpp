#include "ui/ui_resources.h"

#include "layer_palette.h"
#include "layer_palette_badge_layout.h"
#include "layer_palette_status_layout.h"

#include <commctrl.h>
#include <oleacc.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cwchar>
#include <new>
#include <utility>

#include "app/resource.h"
#include "ui/localization.h"
#include "ui/panes/pane_dialog_layout.h"
#include "ui/icons/fluent_icons.h"

namespace inkpod::windows::ui {
namespace {

constexpr int kReferenceDpi = 96;
constexpr int kMargin = 6;
constexpr int kLayerTileHeight = 84;
constexpr int kPlaneTileHeight = 62;
constexpr int kThumbnailWidth = 72;
constexpr int kThumbnailHeight = 54;
constexpr int kButtonHeight = 24;
constexpr int kButtonGap = 4;
constexpr UINT_PTR kListSubclass = 1U;
constexpr UINT_PTR kSplitSubclass = 2U;
constexpr std::array<UINT, 6U> kLayerActionCommands{
    IDM_LAYER_NEW,
    IDM_LAYER_DUPLICATE,
    IDM_LAYER_DELETE,
    IDM_LAYER_MOVE_UP,
    IDM_LAYER_MOVE_DOWN,
    IDM_LAYER_PROPERTIES};
constexpr std::array<UINT, 6U> kPlaneActionCommands{
    IDM_PLANE_NEW,
    IDM_PLANE_DUPLICATE,
    IDM_PLANE_DELETE,
    IDM_PLANE_MOVE_UP,
    IDM_PLANE_MOVE_DOWN,
    IDM_PLANE_PROPERTIES};

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

bool IsPlaneList(HWND list) noexcept {
    return GetDlgCtrlID(list) == IDC_PLANE_LIST;
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

template <std::size_t Size>
const wchar_t* LoadUiString(
    HWND dialog,
    UINT resource,
    const wchar_t* fallback,
    wchar_t (&buffer)[Size]) noexcept {
    const HINSTANCE instance = dialog == nullptr
        ? nullptr
        : reinterpret_cast<HINSTANCE>(
              GetWindowLongPtrW(dialog, GWLP_HINSTANCE));
    if (instance != nullptr
        && LoadLocalizedStringW(instance, resource, buffer, static_cast<int>(Size)) > 0) {
        return buffer;
    }
    return fallback;
}

void ApplyActionCommandState(
    HWND dialog,
    const LayerPaletteDialogState& state,
    const CommandStateSet& command_states) noexcept {
    for (std::size_t index = 0U; index < kLayerActionCommands.size(); ++index) {
        const UINT command = state.plane_active
            ? kPlaneActionCommands[index]
            : kLayerActionCommands[index];
        const CommandState* command_state = FindCommandState(command_states, command);
        const HWND button = GetDlgItem(dialog, kLayerActionCommands[index]);
        if (command_state != nullptr && button != nullptr) {
            EnableWindow(button, command_state->enabled ? TRUE : FALSE);
        }
    }
}

void UpdateActionTargetPresentation(
    HWND dialog, LayerPaletteDialogState& state) noexcept {
    const wchar_t* target = state.plane_active
        ? UiText(UiStringId::OperationTargetPlane)
        : UiText(UiStringId::OperationTargetLayer);
    SetDlgItemTextW(dialog, IDC_LAYER_ACTION_TARGET, target);
    for (const UINT control : kLayerActionCommands) {
        const HWND button = GetDlgItem(dialog, static_cast<int>(control));
        if (button == nullptr) {
            continue;
        }
        wchar_t caption[48]{};
        GetWindowTextW(button, caption, static_cast<int>(std::size(caption)));
        wchar_t accessible[128]{};
        _snwprintf_s(
            accessible,
            std::size(accessible),
            _TRUNCATE,
            L"%ls: %ls",
            target,
            caption);
        static_cast<void>(SetAccessibleName(button, accessible));
    }
    if (state.has_command_states) {
        ApplyActionCommandState(dialog, state, state.command_states);
    }
}

void SetActionTarget(
    HWND dialog, LayerPaletteDialogState& state, bool plane) noexcept {
    state.plane_active = plane;
    UpdateActionTargetPresentation(dialog, state);
}

void PaintLayerPlaneSplitter(HWND splitter, bool highlighted) noexcept {
    PAINTSTRUCT paint{};
    HDC context = BeginPaint(splitter, &paint);
    if (context == nullptr) {
        return;
    }
    RECT client{};
    if (GetClientRect(splitter, &client) != FALSE) {
        FillRect(context, &client, GetSysColorBrush(COLOR_BTNFACE));
        RECT rule = client;
        const LONG center = client.top + (client.bottom - client.top) / 2;
        rule.top = center;
        rule.bottom = std::min(client.bottom, center + 1);
        FillRect(
            context,
            &rule,
            GetSysColorBrush(highlighted ? COLOR_HIGHLIGHT : COLOR_3DSHADOW));
        if (GetFocus() == splitter) {
            DrawFocusRect(context, &client);
        }
    }
    EndPaint(splitter, &paint);
}

UiStringId LayerKindLabelId(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_LAYER_BINARY_COLORING: return UiStringId::LayerBinaryColoring;
        case INKPOD_LAYER_GRAYSCALE_COLORING: return UiStringId::LayerGrayscaleColoring;
        case INKPOD_LAYER_RASTER: return UiStringId::LayerRasterGeneral;
        case INKPOD_LAYER_SELECTION: return UiStringId::LayerSelection;
        case INKPOD_LAYER_FRAME: return UiStringId::LayerFrame;
        case INKPOD_LAYER_VANISHING_POINT: return UiStringId::LayerVanishingPoint;
        case INKPOD_LAYER_ADJUSTMENT: return UiStringId::LayerAdjustment;
        default: return UiStringId::LayerUnknown;
    }
}

UiStringId PlaneKindLabelId(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_TYPED_PLANE_MAIN_LINE: return UiStringId::MainLine;
        case INKPOD_TYPED_PLANE_COLOR: return UiStringId::Coloring;
        case INKPOD_TYPED_PLANE_RASTER: return UiStringId::PlaneRaster;
        case INKPOD_TYPED_PLANE_SELECTION: return UiStringId::PlaneSelection;
        default: return UiStringId::LayerUnknown;
    }
}

UiStringId PlaneKindBadgeLabelId(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_TYPED_PLANE_MAIN_LINE: return UiStringId::PlaneBadgeMainLine;
        case INKPOD_TYPED_PLANE_COLOR: return UiStringId::PlaneBadgeColoring;
        case INKPOD_TYPED_PLANE_RASTER: return UiStringId::PlaneBadgeRaster;
        case INKPOD_TYPED_PLANE_SELECTION: return UiStringId::PlaneBadgeSelection;
        default: return UiStringId::PlaneBadgeUnknown;
    }
}

UiStringId PixelFormatLabelId(std::uint32_t format) noexcept {
    switch (format) {
        case INKPOD_STORAGE_BINARY8: return UiStringId::Text0030;
        case INKPOD_STORAGE_GRAYSCALE8: return UiStringId::Text0182;
        case INKPOD_STORAGE_GRAYSCALE16: return UiStringId::Text0181;
        case INKPOD_STORAGE_RGBA8: return UiStringId::Text0069;
        case INKPOD_STORAGE_RGBA16: return UiStringId::Text0068;
        default: return UiStringId::NameUnavailable;
    }
}

std::wstring Utf8ToWide(const std::string& text) {
    if (text.empty()) {
        return UiText(UiStringId::NameMissing);
    }
    const int required = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        text.data(),
        static_cast<int>(text.size()),
        nullptr,
        0);
    if (required <= 0) {
        return UiText(UiStringId::NameUnavailable);
    }
    std::wstring output(static_cast<std::size_t>(required), L'\0');
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               text.data(),
               static_cast<int>(text.size()),
               output.data(),
               required)
            == required
        ? output
        : UiText(UiStringId::NameUnavailable);
}

const std::vector<LayerPaletteItem>& ItemsFor(
    const LayerPaletteDialogState& state,
    bool plane) noexcept {
    return plane ? state.plane_items : state.items;
}

std::vector<LayerPaletteItem>& ItemsFor(
    LayerPaletteDialogState& state,
    bool plane) noexcept {
    return plane ? state.plane_items : state.items;
}

void LayoutControls(HWND dialog) noexcept {
    LayerPaletteDialogState* state = DialogState(dialog);
    RECT client{};
    if (state == nullptr || GetClientRect(dialog, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(dialog);
    const int margin = ScaleForDpi(kMargin, dpi);
    const int gap = ScaleForDpi(kButtonGap, dpi);
    const int label_height = std::max(
        panes::PaneReadableControlHeight(
            dialog, IDC_LAYER_SECTION, 18, 4),
        panes::PaneReadableControlHeight(
            dialog, IDC_PLANE_SECTION, 18, 4));
    const int split_height = ScaleForDpi(4, dpi);
    const int action_target_height = panes::PaneReadableControlHeight(
        dialog, IDC_LAYER_ACTION_TARGET, 18, 4);
    const int button_height = panes::PaneReadableControlHeight(
        dialog, IDM_LAYER_NEW, kButtonHeight, 8);
    const int width = std::max(
        0, static_cast<int>(client.right) - margin * 2);
    std::array<int, kLayerActionCommands.size()> action_controls{};
    for (std::size_t index = 0; index < kLayerActionCommands.size(); ++index) {
        action_controls[index] = static_cast<int>(kLayerActionCommands[index]);
    }
    const std::size_t action_rows = panes::PaneButtonRowCount(
        dialog, action_controls, width, gap);
    const int action_buttons_height = static_cast<int>(action_rows) * button_height
        + std::max(0, static_cast<int>(action_rows) - 1) * gap;
    const int button_y = std::max(
        margin,
        static_cast<int>(client.bottom) - margin - action_buttons_height);
    const int action_target_y = std::max(
        margin,
        button_y - gap - action_target_height);
    const int list_bottom = std::max(margin, action_target_y - gap);
    const int available_lists = std::max(
        0, list_bottom - margin - label_height * 2 - split_height);
    int layer_height = static_cast<int>(
        static_cast<std::int64_t>(available_lists)
        * std::clamp<std::uint32_t>(state->split_milli, 200U, 800U) / 1000);
    if (available_lists >= ScaleForDpi(160, dpi)) {
        layer_height = std::clamp(
            layer_height,
            ScaleForDpi(80, dpi),
            available_lists - ScaleForDpi(80, dpi));
    }
    int y = margin;
    panes::PlacePaneDialogControl(
        dialog,
        IDC_LAYER_SECTION,
        margin,
        y,
        width,
        label_height);
    y += label_height;
    const HWND layer_list = GetDlgItem(dialog, IDC_LAYER_LIST);
    const bool layer_list_geometry_changed = !panes::PaneWindowHasBounds(
        layer_list, margin, y, width, layer_height);
    panes::PlacePaneDialogControl(
        dialog,
        IDC_LAYER_LIST,
        margin,
        y,
        width,
        layer_height);
    y += layer_height;
    panes::PlacePaneDialogControl(
        dialog,
        IDC_LAYER_PLANE_SPLITTER,
        margin,
        y,
        width,
        split_height);
    y += split_height;
    panes::PlacePaneDialogControl(
        dialog,
        IDC_PLANE_SECTION,
        margin,
        y,
        width,
        label_height);
    y += label_height;
    const int plane_height = std::max(
        0, list_bottom - y);
    const HWND plane_list = GetDlgItem(dialog, IDC_PLANE_LIST);
    const bool plane_list_geometry_changed = !panes::PaneWindowHasBounds(
        plane_list, margin, y, width, plane_height);
    panes::PlacePaneDialogControl(
        dialog,
        IDC_PLANE_LIST,
        margin,
        y,
        width,
        plane_height);
    SendDlgItemMessageW(
        dialog,
        IDC_LAYER_LIST,
        LB_SETITEMHEIGHT,
        0,
        ScaleForDpi(kLayerTileHeight, dpi));
    SendDlgItemMessageW(
        dialog,
        IDC_PLANE_LIST,
        LB_SETITEMHEIGHT,
        0,
        ScaleForDpi(kPlaneTileHeight, dpi));
    for (const auto [list, geometry_changed] : {
             std::pair{layer_list, layer_list_geometry_changed},
             std::pair{plane_list, plane_list_geometry_changed}}) {
        if (geometry_changed && list != nullptr
            && IsWindowVisible(list) != FALSE) {
            // Owner-draw status cells depend on the full row width. Paint the
            // new layout before the next splitter sample so their old right-
            // aligned positions cannot remain visible during a live resize.
            RedrawWindow(
                list,
                nullptr,
                nullptr,
                RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
        }
    }

    panes::PlacePaneDialogControl(
        dialog,
        IDC_LAYER_ACTION_TARGET,
        margin,
        action_target_y,
        width,
        action_target_height);

    panes::PlacePaneButtonRows(
        dialog,
        action_controls,
        margin,
        button_y,
        width,
        button_height,
        gap);
}

bool UpdatePaletteFont(HWND dialog, LayerPaletteDialogState& state) noexcept {
    const UINT dpi = GetDpiForWindow(dialog);
    const HFONT replacement = CreateFontW(
        -MulDiv(9, static_cast<int>(dpi), 72),
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
    for (const UINT control : kLayerActionCommands) {
        SendDlgItemMessageW(
            dialog, control, WM_SETFONT, reinterpret_cast<WPARAM>(replacement), FALSE);
    }
    for (const int control : {
             IDC_LAYER_SECTION,
             IDC_PLANE_SECTION,
             IDC_LAYER_ACTION_TARGET,
             IDC_LAYER_LIST,
             IDC_PLANE_LIST}) {
        SendDlgItemMessageW(
            dialog, control, WM_SETFONT, reinterpret_cast<WPARAM>(replacement), FALSE);
    }
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
    const bool plane = IsPlaneList(list);
    const auto& items = ItemsFor(state, plane);
    if (index < 0 || static_cast<std::size_t>(index) >= items.size()) {
        return;
    }
    SetActionTarget(GetParent(list), state, plane);
    SendMessageW(list, LB_SETCURSEL, static_cast<WPARAM>(index), 0);
    const std::uint64_t id = items[static_cast<std::size_t>(index)].id;
    std::uint64_t& selected = plane
        ? state.selected_plane_id
        : state.selected_layer_id;
    if (id == selected) {
        return;
    }
    selected = id;
    if (state.updating) {
        return;
    }
    LayerPaletteSelectionCallback callback = plane
        ? state.select_plane
        : state.select_layer;
    if (callback != nullptr) {
        callback(state.context, id);
    }
}

int ItemFromPoint(HWND list, POINT point) noexcept {
    const LRESULT result = SendMessageW(
        list, LB_ITEMFROMPOINT, 0, MAKELPARAM(point.x, point.y));
    return HIWORD(result) == 0 ? static_cast<int>(LOWORD(result)) : -1;
}

void DrawThumbnail(
    HDC dc,
    const RECT& bounds,
    const LayerPaletteItem& item,
    ThumbnailCache* cache,
    HFONT font,
    UINT dpi,
    bool plane) noexcept {
    const int requested_width = plane
        ? ScaleLayerPaletteBadgeDip(kLayerPalettePlaneBadgeWidthDip, dpi)
        : ScaleForDpi(kThumbnailWidth, dpi);
    const int requested_height = plane
        ? ScaleLayerPaletteBadgeDip(kLayerPalettePlaneBadgeHeightDip, dpi)
        : ScaleForDpi(kThumbnailHeight, dpi);
    RECT frame{
        bounds.left,
        bounds.top + std::max(
            0,
            (static_cast<int>(bounds.bottom - bounds.top) - requested_height) / 2),
        bounds.left + requested_width,
        0};
    frame.bottom = frame.top + requested_height;
    FillRect(dc, &frame, GetSysColorBrush(COLOR_WINDOW));
    FrameRect(dc, &frame, GetSysColorBrush(COLOR_3DSHADOW));
    if (plane) {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, GetSysColor(COLOR_WINDOWTEXT));
        RECT text_bounds = LayoutLayerPalettePlaneBadgeText(
            dc, font, dpi, item.badge_text, frame);
        const HGDIOBJ previous = font == nullptr ? nullptr : SelectObject(dc, font);
        DrawTextW(
            dc,
            item.badge_text.c_str(),
            static_cast<int>(item.badge_text.size()),
            &text_bounds,
            kLayerPalettePlaneBadgeTextFlags);
        if (previous != nullptr) {
            SelectObject(dc, previous);
        }
        return;
    }
    ThumbnailImageView image{};
    if (cache == nullptr || !cache->Get(item.thumbnail_key, image)
        || image.layout != ThumbnailPixelLayout::Bgra8
        || image.width != item.thumbnail_width
        || image.height != item.thumbnail_height
        || image.stride_bytes != item.thumbnail_stride_bytes) {
        return;
    }
    const int available_width = std::max(1, requested_width - 2);
    const int available_height = std::max(1, requested_height - 2);
    const double scale = std::min(
        static_cast<double>(available_width) / item.thumbnail_width,
        static_cast<double>(available_height) / item.thumbnail_height);
    const int draw_width = std::max(
        1, static_cast<int>(item.thumbnail_width * scale + 0.5));
    const int draw_height = std::max(
        1, static_cast<int>(item.thumbnail_height * scale + 0.5));
    BITMAPINFO bitmap{};
    bitmap.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bitmap.bmiHeader.biWidth = static_cast<LONG>(item.thumbnail_width);
    bitmap.bmiHeader.biHeight = -static_cast<LONG>(item.thumbnail_height);
    bitmap.bmiHeader.biPlanes = 1;
    bitmap.bmiHeader.biBitCount = 32;
    bitmap.bmiHeader.biCompression = BI_RGB;
    SetStretchBltMode(dc, HALFTONE);
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
        image.pixels.data(),
        &bitmap,
        DIB_RGB_COLORS,
        SRCCOPY);
}

void DrawStatusButton(
    HDC dc,
    RECT bounds,
    const wchar_t* label,
    bool active,
    HINSTANCE instance,
    PaneIconId icon,
    UINT dpi) noexcept {
    DrawFrameControl(
        dc,
        &bounds,
        DFC_BUTTON,
        static_cast<UINT>(DFCS_BUTTONPUSH)
            | (active ? static_cast<UINT>(DFCS_PUSHED) : 0U));
    const int icon_size = ScaleForDpi(16, dpi);
    RECT icon_bounds = bounds;
    icon_bounds.left += std::max(
        0, (static_cast<int>(bounds.right - bounds.left) - icon_size) / 2);
    icon_bounds.top += std::max(
        0, (static_cast<int>(bounds.bottom - bounds.top) - icon_size) / 2);
    icon_bounds.right = std::min(bounds.right, icon_bounds.left + icon_size);
    icon_bounds.bottom = std::min(bounds.bottom, icon_bounds.top + icon_size);
    if (!DrawPaneIcon(
            instance,
            dc,
            icon_bounds,
            icon,
            GetSysColor(COLOR_BTNTEXT))) {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, GetSysColor(COLOR_BTNTEXT));
        DrawTextW(
            dc,
            label,
            -1,
            &bounds,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    }
}

void DrawItem(
    const DRAWITEMSTRUCT& draw,
    const LayerPaletteDialogState& state,
    bool plane) noexcept {
    const auto& items = ItemsFor(state, plane);
    if (draw.itemID == static_cast<UINT>(-1)
        || static_cast<std::size_t>(draw.itemID) >= items.size()) {
        return;
    }
    const LayerPaletteItem& item = items[draw.itemID];
    const bool selected = (draw.itemState & ODS_SELECTED) != 0U;
    FillRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(selected ? COLOR_HIGHLIGHT : COLOR_WINDOW));
    if (item.edit_target) {
        RECT marker = draw.rcItem;
        marker.right = marker.left + std::max(3, ScaleForDpi(4, GetDpiForWindow(draw.hwndItem)));
        FillRect(draw.hDC, &marker, GetSysColorBrush(COLOR_HOTLIGHT));
    }
    RECT inner = draw.rcItem;
    const UINT dpi = GetDpiForWindow(draw.hwndItem);
    const int margin = ScaleForDpi(kMargin, dpi);
    InflateRect(&inner, -margin, -ScaleForDpi(4, dpi));
    DrawThumbnail(
        draw.hDC,
        inner,
        item,
        state.thumbnail_cache,
        state.font,
        dpi,
        plane);

    const LayerPaletteStatusCellLayout status_layout =
        LayoutLayerPaletteStatusCells(inner, dpi);
    const int thumbnail_width = plane
        ? ScaleLayerPaletteBadgeDip(kLayerPalettePlaneBadgeWidthDip, dpi)
        : ScaleForDpi(kThumbnailWidth, dpi);
    RECT text_bounds{
        inner.left + thumbnail_width + margin,
        inner.top,
        status_layout.text_right - margin,
        inner.bottom};
    SetBkMode(draw.hDC, TRANSPARENT);
    SetTextColor(
        draw.hDC,
        GetSysColor(selected ? COLOR_HIGHLIGHTTEXT : COLOR_WINDOWTEXT));
    const HGDIOBJ previous = state.font == nullptr
        ? nullptr
        : SelectObject(draw.hDC, state.font);
    RECT line = text_bounds;
    line.bottom = line.top + ScaleForDpi(22, dpi);
    DrawTextW(
        draw.hDC,
        item.name.c_str(),
        -1,
        &line,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    line.top += ScaleForDpi(24, dpi);
    line.bottom = line.top + ScaleForDpi(20, dpi);
    DrawTextW(
        draw.hDC,
        item.detail_text.c_str(),
        -1,
        &line,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    DrawStatusButton(
        draw.hDC,
        status_layout.visibility,
        item.visibility_text.c_str(),
        (item.flags & INKPOD_NODE_VISIBLE) != 0U,
        reinterpret_cast<HINSTANCE>(
            GetWindowLongPtrW(draw.hwndItem, GWLP_HINSTANCE)),
        (item.flags & INKPOD_NODE_VISIBLE) != 0U
            ? PaneIconId::Visible
            : PaneIconId::Hidden,
        dpi);
    DrawStatusButton(
        draw.hDC,
        status_layout.editability,
        item.editability_text.c_str(),
        (item.flags & INKPOD_NODE_EDITABLE) != 0U,
        reinterpret_cast<HINSTANCE>(
            GetWindowLongPtrW(draw.hwndItem, GWLP_HINSTANCE)),
        (item.flags & INKPOD_NODE_EDITABLE) != 0U
            ? PaneIconId::Editable
            : PaneIconId::Protected,
        dpi);
    if (previous != nullptr) {
        SelectObject(draw.hDC, previous);
    }
    FrameRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(COLOR_3DSHADOW));
    if (state.drop_index == static_cast<int>(draw.itemID)
        && state.drag_list_id == GetDlgCtrlID(draw.hwndItem)) {
        RECT marker = draw.rcItem;
        marker.bottom = marker.top + std::max(2, ScaleForDpi(2, dpi));
        FillRect(draw.hDC, &marker, GetSysColorBrush(COLOR_HIGHLIGHT));
    }
    if ((draw.itemState & ODS_FOCUS) != 0U) {
        DrawFocusRect(draw.hDC, &draw.rcItem);
    }
}

LRESULT CALLBACK ListSubclassProcedure(
    HWND list,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR) noexcept {
    LayerPaletteDialogState* state = ListState(list);
    const bool plane = IsPlaneList(list);
    switch (message) {
        case WM_LBUTTONDOWN: {
            if (state == nullptr) break;
            const POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
            const int index = ItemFromPoint(list, point);
            if (index < 0) break;
            const bool control = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
            const bool shift = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
            if ((control || shift) && state->toggle_target != nullptr) {
                const auto& items = ItemsFor(*state, plane);
                if (static_cast<std::size_t>(index) < items.size()) {
                    state->toggle_target(
                        state->context,
                        items[static_cast<std::size_t>(index)].id,
                        plane,
                        shift);
                }
                return 0;
            }
            SelectItem(list, *state, index);
            RECT item_bounds{};
            if (SendMessageW(
                    list,
                    LB_GETITEMRECT,
                    static_cast<WPARAM>(index),
                    reinterpret_cast<LPARAM>(&item_bounds)) == LB_ERR) {
                break;
            }
            const UINT dpi = GetDpiForWindow(list);
            const int margin = ScaleForDpi(kMargin, dpi);
            InflateRect(&item_bounds, -margin, -ScaleForDpi(4, dpi));
            const LayerPaletteStatusCellLayout status_layout =
                LayoutLayerPaletteStatusCells(item_bounds, dpi);
            if (PtInRect(&status_layout.editability, point) != FALSE) {
                state->dispatch_command(
                    state->context,
                    plane ? IDM_PLANE_TOGGLE_EDITABLE
                          : IDM_LAYER_TOGGLE_EDITABLE);
                return 0;
            }
            if (PtInRect(&status_layout.visibility, point) != FALSE) {
                state->dispatch_command(
                    state->context,
                    plane ? IDM_PLANE_TOGGLE_VISIBLE
                          : IDM_LAYER_TOGGLE_VISIBLE);
                return 0;
            }
            state->drag_source = index;
            state->drop_index = index;
            state->drag_list_id = GetDlgCtrlID(list);
            SetCapture(list);
            InvalidateRect(list, nullptr, FALSE);
            break;
        }
        case WM_SETFOCUS:
            if (state != nullptr) {
                SetActionTarget(GetParent(list), *state, plane);
            }
            break;
        case WM_KEYDOWN:
            if (state != nullptr && wparam == VK_SPACE
                && state->toggle_target != nullptr) {
                const int index = static_cast<int>(
                    SendMessageW(list, LB_GETCURSEL, 0, 0));
                const auto& items = ItemsFor(*state, plane);
                if (index >= 0 && static_cast<std::size_t>(index) < items.size()) {
                    state->toggle_target(
                        state->context,
                        items[static_cast<std::size_t>(index)].id,
                        plane,
                        (GetKeyState(VK_SHIFT) & 0x8000) != 0);
                    return 0;
                }
            }
            break;
        case WM_MOUSEMOVE:
            if (state != nullptr && GetCapture() == list
                && state->drag_source >= 0) {
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
            if (state != nullptr && GetCapture() == list
                && state->drag_source >= 0) {
                const int source = state->drag_source;
                const int destination = state->drop_index;
                state->drag_source = -1;
                state->drop_index = -1;
                state->drag_list_id = 0;
                ReleaseCapture();
                InvalidateRect(list, nullptr, FALSE);
                const auto& items = ItemsFor(*state, plane);
                LayerPaletteReorderCallback callback = plane
                    ? state->reorder_plane
                    : state->reorder_layer;
                if (destination >= 0 && source != destination
                    && static_cast<std::size_t>(source) < items.size()
                    && callback != nullptr) {
                    callback(
                        state->context,
                        items[static_cast<std::size_t>(source)].id,
                        static_cast<std::uint32_t>(destination));
                }
                return 0;
            }
            break;
        case WM_CAPTURECHANGED:
            if (state != nullptr) {
                state->drag_source = -1;
                state->drop_index = -1;
                state->drag_list_id = 0;
                InvalidateRect(list, nullptr, FALSE);
            }
            break;
        case WM_NCDESTROY:
            RemoveWindowSubclass(list, ListSubclassProcedure, kListSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(list, message, wparam, lparam);
}

LRESULT CALLBACK SplitSubclassProcedure(
    HWND splitter,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR) noexcept {
    LayerPaletteDialogState* state = ListState(splitter);
    const HWND dialog = GetParent(splitter);
    switch (message) {
        case WM_LBUTTONDOWN:
            if (state != nullptr) {
                state->split_drag_start = POINT{
                    GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                if (ClientToScreen(splitter, &state->split_drag_start) == FALSE) {
                    GetCursorPos(&state->split_drag_start);
                }
                state->split_drag_initial = state->split_milli;
                SetCapture(splitter);
                state->split_dragging = GetCapture() == splitter;
                SetFocus(splitter);
                InvalidateRect(splitter, nullptr, FALSE);
            }
            return 0;
        case WM_MOUSEMOVE:
            if (state != nullptr && !state->split_hovered) {
                TRACKMOUSEEVENT tracking{};
                tracking.cbSize = sizeof(tracking);
                tracking.dwFlags = TME_LEAVE;
                tracking.hwndTrack = splitter;
                state->split_hovered = TrackMouseEvent(&tracking) != FALSE;
                InvalidateRect(splitter, nullptr, FALSE);
            }
            if (state != nullptr && GetCapture() == splitter && dialog != nullptr) {
                POINT current{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                RECT client{};
                if (ClientToScreen(splitter, &current) == FALSE) {
                    GetCursorPos(&current);
                }
                GetClientRect(dialog, &client);
                const int height = std::max(
                    1, static_cast<int>(client.bottom - client.top));
                const int delta = static_cast<int>(
                    static_cast<std::int64_t>(current.y - state->split_drag_start.y)
                    * 1000 / height);
                state->split_milli = static_cast<std::uint32_t>(std::clamp(
                    static_cast<int>(state->split_drag_initial) + delta,
                    200,
                    800));
                LayoutControls(dialog);
                return 0;
            }
            break;
        case WM_MOUSELEAVE:
            if (state != nullptr) {
                state->split_hovered = false;
                InvalidateRect(splitter, nullptr, FALSE);
            }
            return 0;
        case WM_LBUTTONUP:
            if (state != nullptr && state->split_dragging) {
                if (GetCapture() == splitter) {
                    ReleaseCapture();
                }
                // ReleaseCapture normally commits through WM_CAPTURECHANGED.
                // Keep this fallback for a capture implementation that does not
                // synchronously deliver that notification.
                if (state->split_dragging) {
                    state->split_dragging = false;
                    if (state->change_split != nullptr) {
                        state->change_split(state->context, state->split_milli);
                    }
                }
            }
            InvalidateRect(splitter, nullptr, FALSE);
            return 0;
        case WM_CAPTURECHANGED:
            if (state != nullptr && state->split_dragging) {
                state->split_dragging = false;
                if (state->change_split != nullptr) {
                    state->change_split(state->context, state->split_milli);
                }
            }
            InvalidateRect(splitter, nullptr, FALSE);
            break;
        case WM_SETFOCUS:
        case WM_KILLFOCUS:
        case WM_THEMECHANGED:
        case WM_SYSCOLORCHANGE:
        case WM_SETTINGCHANGE:
            InvalidateRect(splitter, nullptr, FALSE);
            break;
        case WM_CANCELMODE:
            if (state != nullptr && state->split_dragging) {
                state->split_dragging = false;
                if (state->split_milli != state->split_drag_initial) {
                    state->split_milli = state->split_drag_initial;
                    if (dialog != nullptr) {
                        LayoutControls(dialog);
                    }
                }
            }
            if (GetCapture() == splitter) {
                ReleaseCapture();
            }
            InvalidateRect(splitter, nullptr, FALSE);
            return 0;
        case WM_ERASEBKGND:
            return 1;
        case WM_PAINT:
            PaintLayerPlaneSplitter(
                splitter,
                state != nullptr
                    && (state->split_hovered || GetCapture() == splitter
                        || GetFocus() == splitter));
            return 0;
        case WM_GETDLGCODE:
            return DefSubclassProc(splitter, message, wparam, lparam)
                | DLGC_WANTARROWS;
        case WM_KEYDOWN:
            if (state != nullptr && dialog != nullptr
                && (wparam == VK_UP || wparam == VK_DOWN)) {
                const int direction = wparam == VK_UP ? -1 : 1;
                const auto adjusted = static_cast<std::uint32_t>(std::clamp(
                    static_cast<int>(state->split_milli) + direction * 20,
                    200,
                    800));
                if (adjusted != state->split_milli) {
                    state->split_milli = adjusted;
                    LayoutControls(dialog);
                    if (state->change_split != nullptr) {
                        state->change_split(state->context, state->split_milli);
                    }
                }
                return 0;
            }
            break;
        case WM_SETCURSOR:
            SetCursor(LoadCursorW(nullptr, IDC_SIZENS));
            return TRUE;
        case WM_NCDESTROY:
            RemoveWindowSubclass(splitter, SplitSubclassProcedure, kSplitSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(splitter, message, wparam, lparam);
}

UINT ActiveActionCommand(
    const LayerPaletteDialogState& state,
    UINT button_command) noexcept {
    const auto found = std::find(
        kLayerActionCommands.begin(), kLayerActionCommands.end(), button_command);
    if (found == kLayerActionCommands.end()) {
        return 0U;
    }
    const std::size_t index = static_cast<std::size_t>(
        std::distance(kLayerActionCommands.begin(), found));
    return state.plane_active ? kPlaneActionCommands[index] : button_command;
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
                || state->select_layer == nullptr || state->select_plane == nullptr
                || state->reorder_layer == nullptr || state->reorder_plane == nullptr
                || state->change_split == nullptr || state->toggle_target == nullptr) {
                return FALSE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HINSTANCE instance = reinterpret_cast<HINSTANCE>(
                GetWindowLongPtrW(dialog, GWLP_HINSTANCE));
            const HWND splitter = CreateWindowExW(
                0,
                L"STATIC",
                nullptr,
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | SS_NOTIFY,
                0,
                0,
                0,
                0,
                dialog,
                reinterpret_cast<HMENU>(
                    static_cast<INT_PTR>(IDC_LAYER_PLANE_SPLITTER)),
                instance,
                nullptr);
            const HWND layer_list = GetDlgItem(dialog, IDC_LAYER_LIST);
            const HWND plane_list = GetDlgItem(dialog, IDC_PLANE_LIST);
            if (splitter == nullptr || layer_list == nullptr || plane_list == nullptr
                || SetWindowSubclass(
                       layer_list, ListSubclassProcedure, kListSubclass, 0)
                    == FALSE
                || SetWindowSubclass(
                       plane_list, ListSubclassProcedure, kListSubclass, 0)
                    == FALSE
                || SetWindowSubclass(
                       splitter, SplitSubclassProcedure, kSplitSubclass, 0)
                    == FALSE
                || !UpdatePaletteFont(dialog, *state)) {
                return FALSE;
            }
            static_cast<void>(SetAccessibleName(
                layer_list, UiText(UiStringId::Layer)));
            static_cast<void>(SetAccessibleName(
                plane_list, UiText(UiStringId::Plane)));
            wchar_t splitter_name[96]{};
            static_cast<void>(SetAccessibleName(
                splitter,
                LoadUiString(
                    dialog,
                    IDS_LAYER_PLANE_SPLITTER,
                    UiText(UiStringId::LayerPlaneSplitterAccessibleName),
                    splitter_name)));
            UpdateActionTargetPresentation(dialog, *state);
            LayoutControls(dialog);
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) break;
            if (LOWORD(wparam) == IDC_LAYER_LIST
                || LOWORD(wparam) == IDC_PLANE_LIST) {
                const HWND list = GetDlgItem(dialog, LOWORD(wparam));
                if (HIWORD(wparam) == LBN_SELCHANGE) {
                    SelectItem(
                        list,
                        *state,
                        static_cast<int>(SendMessageW(list, LB_GETCURSEL, 0, 0)));
                    return TRUE;
                }
                if (HIWORD(wparam) == LBN_DBLCLK) {
                    state->dispatch_command(
                        state->context,
                        LOWORD(wparam) == IDC_PLANE_LIST
                            ? IDM_PLANE_PROPERTIES
                            : IDM_LAYER_PROPERTIES);
                    return TRUE;
                }
            }
            if (HIWORD(wparam) == BN_CLICKED) {
                const UINT command = ActiveActionCommand(*state, LOWORD(wparam));
                if (command != 0U) {
                    state->dispatch_command(state->context, command);
                    return TRUE;
                }
            }
            break;
        case WM_DRAWITEM:
            if (state != nullptr
                && (wparam == IDC_LAYER_LIST || wparam == IDC_PLANE_LIST)) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawItem(*draw, *state, wparam == IDC_PLANE_LIST);
                }
                return TRUE;
            }
            break;
        case WM_SIZE:
            LayoutControls(dialog);
            return TRUE;
        case WM_DPICHANGED_AFTERPARENT:
            if (state != nullptr) {
                UpdatePaletteFont(dialog, *state);
                LayoutControls(dialog);
                UpdateActionTargetPresentation(dialog, *state);
                InvalidateRect(GetDlgItem(dialog, IDC_LAYER_LIST), nullptr, TRUE);
                InvalidateRect(GetDlgItem(dialog, IDC_PLANE_LIST), nullptr, TRUE);
                InvalidateRect(
                    GetDlgItem(dialog, IDC_LAYER_PLANE_SPLITTER), nullptr, FALSE);
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

std::vector<LayerPaletteItem> MakeItems(
    const std::vector<panes::TreePaneNode>& nodes,
    const std::vector<InkpodEditTarget>& targets,
    bool plane,
    std::uint64_t layer_id) {
    std::vector<LayerPaletteItem> items;
    items.reserve(nodes.size());
    for (const auto& node : nodes) {
        const bool target = std::any_of(
            targets.begin(),
            targets.end(),
            [&](const InkpodEditTarget& candidate) {
                return plane
                    ? (candidate.kind == INKPOD_EDIT_TARGET_LAYER
                           && candidate.layer_id == layer_id)
                        || (candidate.kind == INKPOD_EDIT_TARGET_PLANE
                            && candidate.layer_id == layer_id
                            && candidate.plane_id == node.id)
                    : candidate.kind == INKPOD_EDIT_TARGET_LAYER
                        && candidate.layer_id == node.id;
            });
        LayerPaletteItem item{};
        item.id = node.id;
        item.index = node.index;
        item.kind = node.kind;
        item.pixel_format = node.pixel_format;
        item.opacity_milli = node.opacity_milli;
        item.plane_count = node.child_count;
        item.flags = node.flags;
        item.edit_target = target;
        item.kind_label_id = plane
            ? PlaneKindLabelId(node.kind)
            : LayerKindLabelId(node.kind);
        item.badge_label_id = plane
            ? PlaneKindBadgeLabelId(node.kind)
            : UiStringId::PlaneBadgeUnknown;
        item.format_label_id = PixelFormatLabelId(node.pixel_format);
        item.visibility_label_id = (node.flags & INKPOD_NODE_VISIBLE) != 0U
            ? UiStringId::Visible
            : UiStringId::Hidden;
        item.editability_label_id = (node.flags & INKPOD_NODE_EDITABLE) != 0U
            ? UiStringId::Editable
            : UiStringId::Protected;
        item.name = Utf8ToWide(node.name);
        item.kind_text = UiText(item.kind_label_id);
        item.badge_text = UiText(item.badge_label_id);
        item.format_text = UiText(item.format_label_id);
        item.visibility_text = UiText(item.visibility_label_id);
        item.editability_text = UiText(item.editability_label_id);
        std::array<wchar_t, 160U> detail{};
        if (plane) {
            _snwprintf_s(
                detail.data(),
                detail.size(),
                _TRUNCATE,
                L"%ls  |  %ls: %ls  |  %u.%u%%",
                item.kind_text.c_str(),
                UiText(UiStringId::FormatLabel),
                item.format_text.c_str(),
                item.opacity_milli / 10U,
                item.opacity_milli % 10U);
        } else {
            _snwprintf_s(
                detail.data(),
                detail.size(),
                _TRUNCATE,
                L"%ls  |  %u%ls  |  %u.%u%%",
                item.kind_text.c_str(),
                item.plane_count,
                UiText(UiStringId::PlaneCountSuffix),
                item.opacity_milli / 10U,
                item.opacity_milli % 10U);
        }
        item.detail_text = detail.data();
        item.accessible_text.reserve(
            item.name.size() + item.detail_text.size()
            + item.visibility_text.size() + item.editability_text.size() + 16U);
        if (item.edit_target) {
            item.accessible_text.append(UiText(UiStringId::EditTargetPrefix));
        }
        item.accessible_text.append(item.name);
        item.accessible_text.append(L" | ");
        item.accessible_text.append(item.detail_text);
        item.accessible_text.append(L" | ");
        item.accessible_text.append(item.visibility_text);
        item.accessible_text.append(L" | ");
        item.accessible_text.append(item.editability_text);
        item.thumbnail_width = node.thumbnail_width;
        item.thumbnail_height = node.thumbnail_height;
        item.thumbnail_stride_bytes = node.thumbnail_stride_bytes;
        item.thumbnail_key = node.thumbnail_key;
        items.push_back(std::move(item));
    }
    return items;
}

void PopulateList(
    HWND list,
    const std::vector<LayerPaletteItem>& items,
    std::uint64_t selected_id) noexcept {
    SendMessageW(list, WM_SETREDRAW, FALSE, 0);
    SendMessageW(list, LB_RESETCONTENT, 0, 0);
    int selected_index = -1;
    for (std::size_t index = 0; index < items.size(); ++index) {
        const LRESULT added = SendMessageW(
            list,
            LB_ADDSTRING,
            0,
            reinterpret_cast<LPARAM>(items[index].accessible_text.c_str()));
        if (added == LB_ERR || added == LB_ERRSPACE) {
            SendMessageW(list, LB_RESETCONTENT, 0, 0);
            break;
        }
        if (items[index].id == selected_id) {
            selected_index = static_cast<int>(index);
        }
    }
    if (selected_index >= 0) {
        SendMessageW(list, LB_SETCURSEL, static_cast<WPARAM>(selected_index), 0);
    }
    SendMessageW(list, WM_SETREDRAW, TRUE, 0);
    InvalidateRect(list, nullptr, TRUE);
}

}  // namespace

HWND CreateLayerPaletteDialog(
    HINSTANCE instance,
    HWND owner,
    LayerPaletteDialogState& state) noexcept {
    const HWND dialog = CreateLocalizedDialogParamW(
        instance,
        MAKEINTRESOURCEW(IDD_LAYER_PALETTE),
        owner,
        LayerPaletteDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
    if (dialog != nullptr) {
        const LONG_PTR style = GetWindowLongPtrW(dialog, GWL_STYLE);
        SetWindowLongPtrW(dialog, GWL_STYLE, style | WS_CLIPCHILDREN);
    }
    return dialog;
}

void UpdateLayerPaletteDialog(
    HWND dialog,
    const std::vector<panes::TreePaneNode>& layers,
    const std::vector<panes::TreePaneNode>& planes,
    const std::vector<InkpodEditTarget>& edit_targets,
    std::uint64_t selected_layer_id,
    std::uint64_t selected_plane_id,
    std::uint32_t split_milli) noexcept {
    LayerPaletteDialogState* state = DialogState(dialog);
    if (state == nullptr) {
        return;
    }
    try {
        state->updating = true;
        state->items = MakeItems(layers, edit_targets, false, 0U);
        state->plane_items = MakeItems(
            planes, edit_targets, true, selected_layer_id);
        state->selected_layer_id = selected_layer_id;
        state->selected_plane_id = selected_plane_id;
        state->split_milli = std::clamp<std::uint32_t>(split_milli, 200U, 800U);
        PopulateList(
            GetDlgItem(dialog, IDC_LAYER_LIST), state->items, selected_layer_id);
        PopulateList(
            GetDlgItem(dialog, IDC_PLANE_LIST), state->plane_items, selected_plane_id);
        state->updating = false;
        LayoutControls(dialog);
    } catch (const std::bad_alloc&) {
        state->updating = false;
    }
}

void UpdateLayerPaletteCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    LayerPaletteDialogState* palette = DialogState(dialog);
    if (palette == nullptr) {
        return;
    }
    palette->command_states = states;
    palette->has_command_states = true;
    ApplyActionCommandState(dialog, *palette, states);
}

bool LayerPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept {
    LayerPaletteDialogState* palette = DialogState(dialog);
    if (palette == nullptr) {
        return false;
    }
    for (std::size_t index = 0; index < kLayerActionCommands.size(); ++index) {
        const UINT command = palette->plane_active
            ? kPlaneActionCommands[index]
            : kLayerActionCommands[index];
        const CommandState* state = FindCommandState(states, command);
        const HWND button = GetDlgItem(dialog, kLayerActionCommands[index]);
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

std::size_t LayerPalettePlaneCount(HWND dialog) noexcept {
    const LayerPaletteDialogState* state = DialogState(dialog);
    return state == nullptr ? 0U : state->plane_items.size();
}

std::uint64_t LayerPaletteSelectedPlane(HWND dialog) noexcept {
    const LayerPaletteDialogState* state = DialogState(dialog);
    return state == nullptr ? 0U : state->selected_plane_id;
}

bool LayerPaletteItemHasThumbnail(HWND dialog, std::size_t index) noexcept {
    const LayerPaletteDialogState* state = DialogState(dialog);
    ThumbnailImageView image{};
    return state != nullptr && state->thumbnail_cache != nullptr
        && index < state->items.size()
        && state->thumbnail_cache->Peek(
            state->items[index].thumbnail_key, image)
        && image.layout == ThumbnailPixelLayout::Bgra8;
}

}  // namespace inkpod::windows::ui
