#include "color_dock_pane.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cwchar>
#include <new>
#include <utility>
#include <vector>

#include <windowsx.h>

#include "app/resource.h"
#include "pane_dialog_layout.h"
#include "ui/icons/fluent_icons.h"
#include "ui/localization.h"

namespace inkpod::windows::ui::panes {

bool GdiPaintBuffer::Prepare(
    HDC reference,
    int width,
    int height) noexcept {
    if (ReadyFor(width, height)) {
        return true;
    }
    Reset();
    if (reference == nullptr || width <= 0 || height <= 0) {
        return false;
    }

    BITMAPINFO bitmap_info{};
    bitmap_info.bmiHeader.biSize = sizeof(bitmap_info.bmiHeader);
    bitmap_info.bmiHeader.biWidth = width;
    bitmap_info.bmiHeader.biHeight = -height;
    bitmap_info.bmiHeader.biPlanes = 1U;
    bitmap_info.bmiHeader.biBitCount = 32U;
    bitmap_info.bmiHeader.biCompression = BI_RGB;

    HDC dc = CreateCompatibleDC(reference);
    void* bits{};
    HBITMAP bitmap = dc == nullptr
        ? nullptr
        : CreateDIBSection(
              reference,
              &bitmap_info,
              DIB_RGB_COLORS,
              &bits,
              nullptr,
              0U);
    HGDIOBJ previous = bitmap == nullptr ? nullptr : SelectObject(dc, bitmap);
    if (previous == nullptr || previous == HGDI_ERROR || bits == nullptr) {
        if (dc != nullptr && previous != nullptr && previous != HGDI_ERROR) {
            SelectObject(dc, previous);
        }
        if (bitmap != nullptr) {
            DeleteObject(bitmap);
        }
        if (dc != nullptr) {
            DeleteDC(dc);
        }
        return false;
    }

    dc_ = dc;
    bitmap_ = bitmap;
    previous_bitmap_ = previous;
    bits_ = bits;
    width_ = width;
    height_ = height;
    return true;
}

bool GdiPaintBuffer::ReadyFor(int width, int height) const noexcept {
    return dc_ != nullptr && bitmap_ != nullptr && bits_ != nullptr
        && width_ == width && height_ == height;
}

HDC GdiPaintBuffer::Dc() const noexcept {
    return dc_;
}

void* GdiPaintBuffer::Bits() const noexcept {
    return bits_;
}

bool GdiPaintBuffer::Present(HDC destination) const noexcept {
    return destination != nullptr && dc_ != nullptr && width_ > 0 && height_ > 0
        && BitBlt(
               destination,
               0,
               0,
               width_,
               height_,
               dc_,
               0,
               0,
               SRCCOPY) != FALSE;
}

namespace {

constexpr UINT_PTR kPaneSubclass = 1U;
constexpr UINT_PTR kPickerSubclass = 2U;
constexpr UINT_PTR kSwatchSubclass = 3U;
constexpr UINT_PTR kColorLabelSubclass = 4U;
constexpr double kPi = 3.14159265358979323846;

enum PickerDragTarget : int {
    kPickerDragNone = 0,
    kPickerDragHue = 1,
    kPickerDragSaturationValue = 2,
    kPickerDragAlpha = 3,
};

enum SwatchTarget : int {
    kSwatchNone = 0,
    kSwatchMainLine = 1,
    kSwatchDrawing = 2,
};

struct HsvColor {
    double hue_degrees{};
    double saturation{};
    double value{};
};

struct DoublePoint {
    double x{};
    double y{};
};

struct PickerGeometry {
    RECT client{};
    double center_x{};
    double center_y{};
    double outer_radius{};
    double inner_radius{};
    DoublePoint hue_vertex{};
    DoublePoint black_vertex{};
    DoublePoint white_vertex{};
    RECT alpha_track{};
    bool valid{};
};

struct SwatchGeometry {
    double main_x{};
    double main_y{};
    double main_outer{};
    double main_inner{};
    double drawing_x{};
    double drawing_y{};
    double drawing_outer{};
};

int ScaleForDpi(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? 96U : dpi), 96);
}

ColorDockPaneState* PaneState(HWND pane) noexcept {
    return reinterpret_cast<ColorDockPaneState*>(
        GetWindowLongPtrW(pane, GWLP_USERDATA));
}

std::uint8_t Channel8(const InkpodColorValue& color, std::uint16_t value) noexcept {
    return static_cast<std::uint8_t>(
        color.depth == INKPOD_COLOR_DEPTH_16
            ? (static_cast<std::uint32_t>(value) + 128U) / 257U
            : value & 0xffU);
}

COLORREF ColorRef(const InkpodColorValue& color) noexcept {
    return RGB(
        Channel8(color, color.red),
        Channel8(color, color.green),
        Channel8(color, color.blue));
}

InkpodColorValue& ActivePickerColor(ColorDockPaneState& state) noexcept {
    return state.picker_targets_main_line
        ? state.main_line_color
        : state.drawing_color;
}

const InkpodColorValue& ActivePickerColor(
    const ColorDockPaneState& state) noexcept {
    return state.picker_targets_main_line
        ? state.main_line_color
        : state.drawing_color;
}

double& ActivePickerHue(ColorDockPaneState& state) noexcept {
    return state.picker_targets_main_line
        ? state.main_line_hue_degrees
        : state.drawing_hue_degrees;
}

double ActivePickerHue(const ColorDockPaneState& state) noexcept {
    return state.picker_targets_main_line
        ? state.main_line_hue_degrees
        : state.drawing_hue_degrees;
}

double ClampUnit(double value) noexcept {
    return std::clamp(value, 0.0, 1.0);
}

double ChannelUnit(const InkpodColorValue& color, std::uint16_t value) noexcept {
    const double maximum = color.depth == INKPOD_COLOR_DEPTH_16 ? 65535.0 : 255.0;
    return std::clamp(static_cast<double>(value) / maximum, 0.0, 1.0);
}

HsvColor ToHsv(const InkpodColorValue& color, double fallback_hue) noexcept {
    const double red = ChannelUnit(color, color.red);
    const double green = ChannelUnit(color, color.green);
    const double blue = ChannelUnit(color, color.blue);
    const double maximum = std::max({red, green, blue});
    const double minimum = std::min({red, green, blue});
    const double chroma = maximum - minimum;
    HsvColor result{fallback_hue, maximum == 0.0 ? 0.0 : chroma / maximum, maximum};
    if (chroma <= 0.0) {
        return result;
    }
    if (maximum == red) {
        result.hue_degrees = 60.0 * std::fmod((green - blue) / chroma, 6.0);
    } else if (maximum == green) {
        result.hue_degrees = 60.0 * (((blue - red) / chroma) + 2.0);
    } else {
        result.hue_degrees = 60.0 * (((red - green) / chroma) + 4.0);
    }
    if (result.hue_degrees < 0.0) {
        result.hue_degrees += 360.0;
    }
    return result;
}

std::array<double, 3U> HsvRgb(double hue_degrees, double saturation, double value) noexcept {
    double hue = std::fmod(hue_degrees, 360.0);
    if (hue < 0.0) {
        hue += 360.0;
    }
    const double chroma = ClampUnit(value) * ClampUnit(saturation);
    const double sector = hue / 60.0;
    const double secondary = chroma * (1.0 - std::abs(std::fmod(sector, 2.0) - 1.0));
    double red{};
    double green{};
    double blue{};
    if (sector < 1.0) {
        red = chroma;
        green = secondary;
    } else if (sector < 2.0) {
        red = secondary;
        green = chroma;
    } else if (sector < 3.0) {
        green = chroma;
        blue = secondary;
    } else if (sector < 4.0) {
        green = secondary;
        blue = chroma;
    } else if (sector < 5.0) {
        red = secondary;
        blue = chroma;
    } else {
        red = chroma;
        blue = secondary;
    }
    const double match = ClampUnit(value) - chroma;
    return {red + match, green + match, blue + match};
}

InkpodColorValue FromHsv(
    const InkpodColorValue& source,
    double hue_degrees,
    double saturation,
    double value) noexcept {
    InkpodColorValue result = source;
    const auto rgb = HsvRgb(hue_degrees, saturation, value);
    const double maximum = source.depth == INKPOD_COLOR_DEPTH_16 ? 65535.0 : 255.0;
    result.red = static_cast<std::uint16_t>(
        std::lround(ClampUnit(rgb[0]) * maximum));
    result.green = static_cast<std::uint16_t>(
        std::lround(ClampUnit(rgb[1]) * maximum));
    result.blue = static_cast<std::uint16_t>(
        std::lround(ClampUnit(rgb[2]) * maximum));
    return result;
}

std::array<double, 3U> Barycentric(
    double x,
    double y,
    const PickerGeometry& geometry) noexcept {
    const DoublePoint& a = geometry.hue_vertex;
    const DoublePoint& b = geometry.black_vertex;
    const DoublePoint& c = geometry.white_vertex;
    const double denominator = (b.y - c.y) * (a.x - c.x)
        + (c.x - b.x) * (a.y - c.y);
    if (std::abs(denominator) < 0.0001) {
        return {-1.0, -1.0, -1.0};
    }
    const double hue_weight = ((b.y - c.y) * (x - c.x)
        + (c.x - b.x) * (y - c.y)) / denominator;
    const double black_weight = ((c.y - a.y) * (x - c.x)
        + (a.x - c.x) * (y - c.y)) / denominator;
    return {hue_weight, black_weight, 1.0 - hue_weight - black_weight};
}

PickerGeometry MakePickerGeometry(HWND picker, double hue_degrees) noexcept {
    PickerGeometry geometry{};
    if (GetClientRect(picker, &geometry.client) == FALSE) {
        return geometry;
    }
    const UINT dpi = GetDpiForWindow(picker);
    const int opacity_height = ScaleForDpi(46, dpi);
    const int color_height = std::max(
        0, static_cast<int>(geometry.client.bottom) - opacity_height);
    const int horizontal_allowance = ScaleForDpi(96, dpi);
    const int maximum_diameter = ScaleForDpi(222, dpi);
    const int diameter = std::min(
        maximum_diameter,
        std::min(color_height - ScaleForDpi(4, dpi),
                 static_cast<int>(geometry.client.right) - horizontal_allowance));
    if (diameter < ScaleForDpi(68, dpi)) {
        return geometry;
    }
    geometry.center_x = static_cast<double>(geometry.client.right) * 0.5;
    geometry.center_y = static_cast<double>(color_height) * 0.5;
    geometry.outer_radius = static_cast<double>(diameter) * 0.5;
    geometry.inner_radius = geometry.outer_radius
        - static_cast<double>(std::max(ScaleForDpi(12, dpi), diameter / 10));
    const double triangle_radius = geometry.inner_radius
        - static_cast<double>(ScaleForDpi(4, dpi));
    const auto vertex_at = [&](double angle_degrees) noexcept {
        const double radians = angle_degrees * kPi / 180.0;
        return DoublePoint{
            geometry.center_x + std::cos(radians) * triangle_radius,
            geometry.center_y + std::sin(radians) * triangle_radius};
    };
    geometry.hue_vertex = vertex_at(hue_degrees);
    geometry.black_vertex = vertex_at(hue_degrees - 120.0);
    geometry.white_vertex = vertex_at(hue_degrees + 120.0);
    geometry.alpha_track = {
        ScaleForDpi(48, dpi),
        geometry.client.bottom - ScaleForDpi(19, dpi),
        geometry.client.right - ScaleForDpi(45, dpi),
        geometry.client.bottom - ScaleForDpi(7, dpi)};
    geometry.valid = geometry.alpha_track.right > geometry.alpha_track.left;
    return geometry;
}

void SetColorLabel(
    HWND pane,
    int control,
    const wchar_t* name,
    const InkpodColorValue& color) noexcept {
    std::array<wchar_t, 64U> text{};
    if (color.depth == INKPOD_COLOR_DEPTH_16) {
        swprintf_s(
            text.data(),
            text.size(),
            L"%ls  #%04X%04X%04X%04X",
            name,
            static_cast<unsigned>(color.red),
            static_cast<unsigned>(color.green),
            static_cast<unsigned>(color.blue),
            static_cast<unsigned>(color.alpha));
    } else {
        swprintf_s(
            text.data(),
            text.size(),
            L"%ls  #%02X%02X%02X%02X",
            name,
            static_cast<unsigned>(color.red),
            static_cast<unsigned>(color.green),
            static_cast<unsigned>(color.blue),
            static_cast<unsigned>(color.alpha));
    }
    SetDlgItemTextW(pane, control, text.data());
}

void ShowTabControls(HWND pane, int tab) noexcept {
    for (const int control : {
             IDC_COLOR_MAIN_LINE_LABEL,
             IDC_COLOR_MAIN_LINE_SWATCH,
             IDC_COLOR_DRAWING_LABEL,
             IDC_COLOR_PICKER,
             IDC_COLOR_EYEDROPPER,
             IDC_COLOR_RED,
             IDC_COLOR_GREEN,
             IDC_COLOR_BLUE,
             IDC_COLOR_ALPHA,
             IDC_COLOR_APPLY}) {
        ShowWindow(GetDlgItem(pane, control), tab == 0 ? SW_SHOW : SW_HIDE);
    }
    ShowWindow(GetDlgItem(pane, IDC_COLOR_SWATCH), SW_HIDE);
    for (const int control : {
             IDC_PALETTE_LIST,
             IDC_PALETTE_PREVIOUS,
             IDC_PALETTE_NEXT,
             IDC_PALETTE_REGISTER_BUTTON,
             IDC_PALETTE_DELETE_BUTTON,
             IDC_PALETTE_CLEAR_BUTTON,
             IDC_PALETTE_LOAD_BUTTON,
             IDC_PALETTE_SAVE_BUTTON}) {
        ShowWindow(GetDlgItem(pane, control), tab == 1 ? SW_SHOW : SW_HIDE);
    }
    ShowWindow(
        GetDlgItem(pane, IDC_COLOR_CHART_LIST), tab == 2 ? SW_SHOW : SW_HIDE);
}

void LayoutPane(HWND pane) noexcept {
    RECT client{};
    if (GetClientRect(pane, &client) == FALSE) {
        return;
    }
    const UINT dpi = GetDpiForWindow(pane);
    const int margin = ScaleForDpi(6, dpi);
    const int tabs_height = ScaleForDpi(28, dpi);
    const int target_height = ScaleForDpi(22, dpi);
    const int row = ScaleForDpi(24, dpi);
    const int gap = ScaleForDpi(5, dpi);
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_TABS,
        margin,
        margin + target_height,
        std::max(0, static_cast<int>(client.right) - margin * 2),
        std::max(
            0,
            static_cast<int>(client.bottom) - margin * 2 - target_height));
    PlacePaneTargetRow(
        pane,
        IDC_COLOR_TARGET,
        IDC_COLOR_PIN,
        margin,
        margin,
        std::max(0, static_cast<int>(client.right) - margin * 2),
        0,
        target_height,
        target_height,
        gap);
    RECT content{margin * 2, margin + target_height + tabs_height,
                 client.right - margin * 2,
                 client.bottom - margin * 2};
    const int top_row = ScaleForDpi(44, dpi);
    const int swatch_width = ScaleForDpi(60, dpi);
    const int eyedropper_width = std::min(
        std::max(0, static_cast<int>(content.right - content.left)),
        PaneButtonIdealWidth(pane, IDC_COLOR_EYEDROPPER));
    const int eyedropper_height = ScaleForDpi(24, dpi);
    const int swatch_left = content.left + ScaleForDpi(3, dpi);
    const int label_left = content.left + ScaleForDpi(67, dpi);
    const int label_width = std::max(
        0,
        static_cast<int>(content.right) - eyedropper_width - gap - label_left);
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_MAIN_LINE_LABEL,
        label_left,
        content.top,
        label_width,
        top_row / 2);
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_MAIN_LINE_SWATCH,
        swatch_left,
        content.top,
        swatch_width,
        top_row);
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_DRAWING_LABEL,
        label_left,
        content.top + top_row / 2,
        label_width,
        top_row / 2);
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_SWATCH,
        0,
        0,
        0,
        0);
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_EYEDROPPER,
        content.right - eyedropper_width,
        content.top + (top_row - eyedropper_height) / 2,
        eyedropper_width,
        eyedropper_height);
    const int fields_top = std::max(
        static_cast<int>(content.top) + top_row + gap,
        static_cast<int>(content.bottom) - row);
    const int picker_top = content.top + top_row + gap;
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_PICKER,
        content.left,
        picker_top,
        std::max(0, static_cast<int>(content.right - content.left)),
        std::max(0, fields_top - gap - picker_top));
    const int apply_width = std::min(
        std::max(0, static_cast<int>(content.right - content.left)),
        PaneButtonIdealWidth(pane, IDC_COLOR_APPLY));
    const int field_area_width = std::max(
        0,
        static_cast<int>(content.right - content.left) - apply_width - gap);
    const int field_width = std::max(
        ScaleForDpi(30, dpi), (field_area_width - gap * 3) / 4);
    int x = content.left;
    for (const int control : {
             IDC_COLOR_RED, IDC_COLOR_GREEN, IDC_COLOR_BLUE, IDC_COLOR_ALPHA}) {
        PlacePaneDialogControl(
            pane,
            control,
            x,
            fields_top,
            field_width,
            row);
        x += field_width + gap;
    }
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_APPLY,
        content.right - apply_width,
        fields_top,
        apply_width,
        row);
    const int button_width = ScaleForDpi(32, dpi);
    PlacePaneDialogControl(
        pane,
        IDC_PALETTE_PREVIOUS,
        content.left,
        content.top,
        button_width,
        row);
    PlacePaneDialogControl(
        pane,
        IDC_PALETTE_NEXT,
        content.right - button_width,
        content.top,
        button_width,
        row);
    const std::array<int, 5U> palette_action_controls{
        IDC_PALETTE_REGISTER_BUTTON,
        IDC_PALETTE_DELETE_BUTTON,
        IDC_PALETTE_CLEAR_BUTTON,
        IDC_PALETTE_LOAD_BUTTON,
        IDC_PALETTE_SAVE_BUTTON};
    const int palette_content_width = std::max(
        0, static_cast<int>(content.right - content.left));
    const std::size_t palette_action_rows = PaneButtonRowCount(
        pane, palette_action_controls, palette_content_width, gap);
    const int palette_action_height = static_cast<int>(palette_action_rows) * row
        + std::max(0, static_cast<int>(palette_action_rows) - 1) * gap;
    const int palette_action_top = content.top + row + gap;
    PlacePaneButtonRows(
        pane,
        palette_action_controls,
        content.left,
        palette_action_top,
        palette_content_width,
        row,
        gap);
    PlacePaneDialogControl(
        pane,
        IDC_PALETTE_LIST,
        content.left,
        palette_action_top + palette_action_height + gap,
        std::max(0, static_cast<int>(content.right - content.left)),
        std::max(
            0,
            static_cast<int>(content.bottom)
                - palette_action_top - palette_action_height - gap));
    PlacePaneDialogControl(
        pane,
        IDC_COLOR_CHART_LIST,
        content.left,
        content.top,
        std::max(0, static_cast<int>(content.right - content.left)),
        std::max(0, static_cast<int>(content.bottom - content.top)));
}

void UpdateFont(HWND pane, ColorDockPaneState& state) noexcept {
    const HFONT replacement = CreateFontW(
        -MulDiv(9, static_cast<int>(GetDpiForWindow(pane)), 72),
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
        return;
    }
    for (const int control : {
             IDC_COLOR_TABS,
             IDC_COLOR_TARGET,
             IDC_COLOR_PIN,
             IDC_COLOR_MAIN_LINE_LABEL,
             IDC_COLOR_DRAWING_LABEL,
             IDC_COLOR_PICKER,
             IDC_COLOR_EYEDROPPER,
             IDC_COLOR_RED,
             IDC_COLOR_GREEN,
             IDC_COLOR_BLUE,
             IDC_COLOR_ALPHA,
             IDC_COLOR_APPLY,
             IDC_PALETTE_LIST,
             IDC_PALETTE_PREVIOUS,
             IDC_PALETTE_NEXT,
             IDC_PALETTE_REGISTER_BUTTON,
             IDC_PALETTE_DELETE_BUTTON,
             IDC_PALETTE_CLEAR_BUTTON,
             IDC_PALETTE_LOAD_BUTTON,
             IDC_PALETTE_SAVE_BUTTON,
             IDC_COLOR_CHART_LIST}) {
        SendDlgItemMessageW(
            pane, control, WM_SETFONT, reinterpret_cast<WPARAM>(replacement), TRUE);
    }
    if (state.font != nullptr) {
        DeleteObject(state.font);
    }
    state.font = replacement;
}

void SetColorFields(HWND pane, const InkpodColorValue& color) noexcept {
    const std::array<std::pair<int, std::uint16_t>, 4U> fields{{
        {IDC_COLOR_RED, color.red},
        {IDC_COLOR_GREEN, color.green},
        {IDC_COLOR_BLUE, color.blue},
        {IDC_COLOR_ALPHA, color.alpha},
    }};
    for (const auto& [control, value] : fields) {
        std::array<wchar_t, 16U> text{};
        swprintf_s(text.data(), text.size(), L"%u", static_cast<unsigned>(value));
        SetDlgItemTextW(pane, control, text.data());
    }
}

bool ReadChannel(HWND pane, int control, std::uint32_t maximum, std::uint16_t& output) noexcept {
    std::array<wchar_t, 32U> text{};
    GetDlgItemTextW(pane, control, text.data(), static_cast<int>(text.size()));
    wchar_t* end{};
    const unsigned long value = std::wcstoul(text.data(), &end, 10);
    if (end == text.data() || *end != L'\0' || value > maximum) {
        return false;
    }
    output = static_cast<std::uint16_t>(value);
    return true;
}

void ApplyFields(HWND pane, ColorDockPaneState& state) noexcept {
    InkpodColorValue color = ActivePickerColor(state);
    const std::uint32_t maximum = color.depth == INKPOD_COLOR_DEPTH_16
        ? UINT16_MAX
        : UINT8_MAX;
    if (ReadChannel(pane, IDC_COLOR_RED, maximum, color.red)
        && ReadChannel(pane, IDC_COLOR_GREEN, maximum, color.green)
        && ReadChannel(pane, IDC_COLOR_BLUE, maximum, color.blue)
        && ReadChannel(pane, IDC_COLOR_ALPHA, maximum, color.alpha)) {
        if (state.picker_targets_main_line) {
            state.main_line_color = color;
            if (state.change_main_line_color != nullptr) {
                state.change_main_line_color(state.context, color);
            }
        } else if (state.change_color != nullptr) {
            state.change_color(state.context, color);
        }
    } else {
        SetColorFields(pane, ActivePickerColor(state));
    }
}

std::uint32_t DibPixel(double red, double green, double blue) noexcept {
    const auto channel = [](double value) noexcept {
        return static_cast<std::uint32_t>(
            std::lround(ClampUnit(value) * 255.0));
    };
    return (channel(red) << 16U) | (channel(green) << 8U) | channel(blue);
}

std::array<double, 3U> PixelRgb(std::uint32_t pixel) noexcept {
    return {
        static_cast<double>((pixel >> 16U) & 0xffU) / 255.0,
        static_cast<double>((pixel >> 8U) & 0xffU) / 255.0,
        static_cast<double>(pixel & 0xffU) / 255.0};
}

void BlendDibPixel(
    std::uint32_t& destination,
    const std::array<double, 3U>& source,
    double alpha) noexcept {
    const double amount = ClampUnit(alpha);
    if (amount <= 0.0) {
        return;
    }
    const auto background = PixelRgb(destination);
    destination = DibPixel(
        source[0] * amount + background[0] * (1.0 - amount),
        source[1] * amount + background[1] * (1.0 - amount),
        source[2] * amount + background[2] * (1.0 - amount));
}

void BlitDibPixels(
    HDC dc,
    int width,
    int height,
    const std::vector<std::uint32_t>& pixels) noexcept {
    BITMAPINFO bitmap{};
    bitmap.bmiHeader.biSize = sizeof(bitmap.bmiHeader);
    bitmap.bmiHeader.biWidth = width;
    bitmap.bmiHeader.biHeight = -height;
    bitmap.bmiHeader.biPlanes = 1U;
    bitmap.bmiHeader.biBitCount = 32U;
    bitmap.bmiHeader.biCompression = BI_RGB;
    SetDIBitsToDevice(
        dc,
        0,
        0,
        static_cast<DWORD>(width),
        static_cast<DWORD>(height),
        0,
        0,
        0,
        static_cast<UINT>(height),
        pixels.data(),
        &bitmap,
        DIB_RGB_COLORS);
}

std::array<double, 3U> SystemRgb(int index) noexcept {
    const COLORREF color = GetSysColor(index);
    return {
        static_cast<double>(GetRValue(color)) / 255.0,
        static_cast<double>(GetGValue(color)) / 255.0,
        static_cast<double>(GetBValue(color)) / 255.0};
}

double SampledCircleCoverage(
    int x,
    int y,
    double center_x,
    double center_y,
    double outer_radius,
    double inner_radius = 0.0) noexcept {
    constexpr std::array<double, 2U> offsets{0.25, 0.75};
    unsigned covered{};
    for (const double offset_y : offsets) {
        for (const double offset_x : offsets) {
            const double delta_x = static_cast<double>(x) + offset_x - center_x;
            const double delta_y = static_cast<double>(y) + offset_y - center_y;
            const double distance_squared = delta_x * delta_x + delta_y * delta_y;
            if (distance_squared <= outer_radius * outer_radius
                && distance_squared >= inner_radius * inner_radius) {
                ++covered;
            }
        }
    }
    return static_cast<double>(covered) / 4.0;
}

SwatchGeometry MakeSwatchGeometry(int width, int height) noexcept {
    const double short_side = static_cast<double>(std::min(width, height));
    return SwatchGeometry{
        static_cast<double>(width) * 0.29,
        static_cast<double>(height) * 0.34,
        short_side * 0.31,
        short_side * 0.16,
        static_cast<double>(width) * 0.64,
        static_cast<double>(height) * 0.62,
        short_side * 0.38};
}

bool PointInCircle(
    double x,
    double y,
    double center_x,
    double center_y,
    double radius) noexcept {
    const double delta_x = x - center_x;
    const double delta_y = y - center_y;
    return delta_x * delta_x + delta_y * delta_y <= radius * radius;
}

SwatchTarget HitSwatchTarget(
    const SwatchGeometry& geometry,
    bool main_line_in_front,
    int x,
    int y) noexcept {
    const bool hits_main = PointInCircle(
        static_cast<double>(x),
        static_cast<double>(y),
        geometry.main_x,
        geometry.main_y,
        geometry.main_outer);
    const bool hits_drawing = PointInCircle(
        static_cast<double>(x),
        static_cast<double>(y),
        geometry.drawing_x,
        geometry.drawing_y,
        geometry.drawing_outer);
    if (hits_main && hits_drawing) {
        return main_line_in_front ? kSwatchMainLine : kSwatchDrawing;
    }
    if (hits_main) {
        return kSwatchMainLine;
    }
    return hits_drawing ? kSwatchDrawing : kSwatchNone;
}

void DrawCombinedSwatches(
    const DRAWITEMSTRUCT& draw,
    const ColorDockPaneState& state) noexcept {
    const int width = static_cast<int>(draw.rcItem.right - draw.rcItem.left);
    const int height = static_cast<int>(draw.rcItem.bottom - draw.rcItem.top);
    if (width <= 0 || height <= 0) {
        return;
    }
    const auto background = SystemRgb(COLOR_3DFACE);
    std::vector<std::uint32_t> pixels;
    try {
        pixels.assign(
            static_cast<std::size_t>(width) * static_cast<std::size_t>(height),
            DibPixel(background[0], background[1], background[2]));
    } catch (const std::bad_alloc&) {
        FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_3DFACE));
        return;
    }
    const double short_side = static_cast<double>(std::min(width, height));
    const SwatchGeometry geometry = MakeSwatchGeometry(width, height);
    const double outline = std::max(1.0, short_side * 0.035);
    const auto shadow = SystemRgb(COLOR_3DSHADOW);
    const auto selected_border = SystemRgb(COLOR_WINDOW);
    const auto unselected_border = SystemRgb(COLOR_3DLIGHT);
    const auto main_color = std::array<double, 3U>{
        ChannelUnit(state.main_line_color, state.main_line_color.red),
        ChannelUnit(state.main_line_color, state.main_line_color.green),
        ChannelUnit(state.main_line_color, state.main_line_color.blue)};
    const auto drawing_color = std::array<double, 3U>{
        ChannelUnit(state.drawing_color, state.drawing_color.red),
        ChannelUnit(state.drawing_color, state.drawing_color.green),
        ChannelUnit(state.drawing_color, state.drawing_color.blue)};
    const double main_alpha = ChannelUnit(
        state.main_line_color, state.main_line_color.alpha);
    const double drawing_alpha = ChannelUnit(
        state.drawing_color, state.drawing_color.alpha);
    const int checker_size = std::max(2, height / 7);
    const auto checker_light = SystemRgb(COLOR_WINDOW);
    const auto checker_dark = SystemRgb(COLOR_3DLIGHT);
    const auto draw_main_line = [&](bool selected) noexcept {
        const auto& border = selected ? selected_border : unselected_border;
        for (int y = 0; y < height; ++y) {
            for (int x = 0; x < width; ++x) {
                auto& pixel = pixels[static_cast<std::size_t>(y)
                    * static_cast<std::size_t>(width) + static_cast<std::size_t>(x)];
                BlendDibPixel(
                    pixel,
                    shadow,
                    SampledCircleCoverage(
                        x,
                        y,
                        geometry.main_x,
                        geometry.main_y,
                        geometry.main_outer,
                        geometry.main_inner));
                BlendDibPixel(
                    pixel,
                    border,
                    SampledCircleCoverage(
                        x,
                        y,
                        geometry.main_x,
                        geometry.main_y,
                        geometry.main_outer - outline * 0.35,
                        geometry.main_inner + outline * 0.35));
                BlendDibPixel(
                    pixel,
                    main_color,
                    SampledCircleCoverage(
                        x,
                        y,
                        geometry.main_x,
                        geometry.main_y,
                        geometry.main_outer - outline * 1.25,
                        geometry.main_inner + outline * 1.25)
                        * main_alpha);
            }
        }
    };
    const auto draw_drawing = [&](bool selected) noexcept {
        const auto& border = selected ? selected_border : unselected_border;
        for (int y = 0; y < height; ++y) {
            for (int x = 0; x < width; ++x) {
                auto& pixel = pixels[static_cast<std::size_t>(y)
                    * static_cast<std::size_t>(width) + static_cast<std::size_t>(x)];
                BlendDibPixel(
                    pixel,
                    shadow,
                    SampledCircleCoverage(
                        x,
                        y,
                        geometry.drawing_x,
                        geometry.drawing_y,
                        geometry.drawing_outer));
                BlendDibPixel(
                    pixel,
                    border,
                    SampledCircleCoverage(
                        x,
                        y,
                        geometry.drawing_x,
                        geometry.drawing_y,
                        geometry.drawing_outer - outline * 0.4));
                const double fill_coverage = SampledCircleCoverage(
                    x,
                    y,
                    geometry.drawing_x,
                    geometry.drawing_y,
                    geometry.drawing_outer - outline * 1.35);
                const auto& checker = ((x / checker_size) + (y / checker_size)) % 2 == 0
                    ? checker_light
                    : checker_dark;
                const std::array<double, 3U> displayed{
                    drawing_color[0] * drawing_alpha
                        + checker[0] * (1.0 - drawing_alpha),
                    drawing_color[1] * drawing_alpha
                        + checker[1] * (1.0 - drawing_alpha),
                    drawing_color[2] * drawing_alpha
                        + checker[2] * (1.0 - drawing_alpha)};
                BlendDibPixel(pixel, displayed, fill_coverage);
            }
        }
    };
    if (state.picker_targets_main_line) {
        draw_drawing(false);
        draw_main_line(true);
    } else {
        draw_main_line(false);
        draw_drawing(true);
    }
    BlitDibPixels(draw.hDC, width, height, pixels);
    if ((draw.itemState & ODS_FOCUS) != 0U || GetFocus() == draw.hwndItem) {
        RECT focus = draw.rcItem;
        InflateRect(&focus, -1, -1);
        DrawFocusRect(draw.hDC, &focus);
    }
}

bool CopyPixelBuffer(
    std::vector<std::uint32_t>& destination,
    const std::vector<std::uint32_t>& source) noexcept {
    try {
        destination = source;
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

void InvalidatePickerCaches(ColorDockPaneState& state) noexcept {
    state.picker_ring_cache_valid = false;
    state.picker_triangle_cache_valid = false;
    state.picker_frame_cache_valid = false;
}

bool EnsureHueRingCache(
    ColorDockPaneState& state,
    const PickerGeometry& geometry,
    UINT dpi) noexcept {
    const int width = static_cast<int>(geometry.client.right);
    const int height = static_cast<int>(geometry.client.bottom);
    const COLORREF face = GetSysColor(COLOR_3DFACE);
    const COLORREF window = GetSysColor(COLOR_WINDOW);
    const COLORREF light = GetSysColor(COLOR_3DLIGHT);
    if (state.picker_ring_cache_valid && state.picker_cache_width == width
        && state.picker_cache_height == height && state.picker_cache_dpi == dpi
        && state.picker_cache_face == face && state.picker_cache_window == window
        && state.picker_cache_light == light) {
        return true;
    }
    const auto background = SystemRgb(COLOR_3DFACE);
    try {
        state.picker_ring_pixels.assign(
            static_cast<std::size_t>(width) * static_cast<std::size_t>(height),
            DibPixel(background[0], background[1], background[2]));
    } catch (const std::bad_alloc&) {
        InvalidatePickerCaches(state);
        return false;
    }
    constexpr std::array<double, 2U> offsets{0.25, 0.75};
    const int left = std::max(
        0, static_cast<int>(std::floor(geometry.center_x - geometry.outer_radius - 1.0)));
    const int right = std::min(
        width, static_cast<int>(std::ceil(geometry.center_x + geometry.outer_radius + 1.0)));
    const int top = std::max(
        0, static_cast<int>(std::floor(geometry.center_y - geometry.outer_radius - 1.0)));
    const int bottom = std::min(
        height, static_cast<int>(std::ceil(geometry.center_y + geometry.outer_radius + 1.0)));
    for (int y = top; y < bottom; ++y) {
        for (int x = left; x < right; ++x) {
            std::array<double, 3U> accumulated{
                background[0] * 4.0,
                background[1] * 4.0,
                background[2] * 4.0};
            for (const double offset_y : offsets) {
                for (const double offset_x : offsets) {
                    const double delta_x = static_cast<double>(x) + offset_x
                        - geometry.center_x;
                    const double delta_y = static_cast<double>(y) + offset_y
                        - geometry.center_y;
                    const double distance_squared = delta_x * delta_x + delta_y * delta_y;
                    if (distance_squared < geometry.inner_radius * geometry.inner_radius
                        || distance_squared > geometry.outer_radius * geometry.outer_radius) {
                        continue;
                    }
                    double hue = std::atan2(delta_y, delta_x) * 180.0 / kPi;
                    if (hue < 0.0) {
                        hue += 360.0;
                    }
                    const auto ring_color = HsvRgb(hue, 1.0, 1.0);
                    for (std::size_t channel = 0; channel < accumulated.size(); ++channel) {
                        accumulated[channel] += ring_color[channel] - background[channel];
                    }
                }
            }
            state.picker_ring_pixels[static_cast<std::size_t>(y)
                * static_cast<std::size_t>(width) + static_cast<std::size_t>(x)] =
                DibPixel(
                    accumulated[0] * 0.25,
                    accumulated[1] * 0.25,
                    accumulated[2] * 0.25);
        }
    }
    state.picker_cache_width = width;
    state.picker_cache_height = height;
    state.picker_cache_dpi = dpi;
    state.picker_cache_face = face;
    state.picker_cache_window = window;
    state.picker_cache_light = light;
    state.picker_ring_cache_valid = true;
    state.picker_triangle_cache_valid = false;
    state.picker_frame_cache_valid = false;
    return true;
}

bool EnsureTriangleCache(
    ColorDockPaneState& state,
    const PickerGeometry& geometry,
    UINT dpi) noexcept {
    if (!EnsureHueRingCache(state, geometry, dpi)) {
        return false;
    }
    const double hue_degrees = ActivePickerHue(state);
    if (state.picker_triangle_cache_valid
        && std::abs(state.picker_cache_hue_degrees - hue_degrees)
            < 0.0001) {
        return true;
    }
    if (!CopyPixelBuffer(state.picker_triangle_pixels, state.picker_ring_pixels)) {
        state.picker_triangle_cache_valid = false;
        return false;
    }
    constexpr std::array<double, 2U> offsets{0.25, 0.75};
    const auto pure_hue = HsvRgb(hue_degrees, 1.0, 1.0);
    const int width = state.picker_cache_width;
    const int height = state.picker_cache_height;
    const double minimum_x = std::min({
        geometry.hue_vertex.x,
        geometry.black_vertex.x,
        geometry.white_vertex.x});
    const double maximum_x = std::max({
        geometry.hue_vertex.x,
        geometry.black_vertex.x,
        geometry.white_vertex.x});
    const double minimum_y = std::min({
        geometry.hue_vertex.y,
        geometry.black_vertex.y,
        geometry.white_vertex.y});
    const double maximum_y = std::max({
        geometry.hue_vertex.y,
        geometry.black_vertex.y,
        geometry.white_vertex.y});
    const int left = std::max(0, static_cast<int>(std::floor(minimum_x - 1.0)));
    const int right = std::min(width, static_cast<int>(std::ceil(maximum_x + 1.0)));
    const int top = std::max(0, static_cast<int>(std::floor(minimum_y - 1.0)));
    const int bottom = std::min(height, static_cast<int>(std::ceil(maximum_y + 1.0)));
    for (int y = top; y < bottom; ++y) {
        for (int x = left; x < right; ++x) {
            const std::size_t index = static_cast<std::size_t>(y)
                * static_cast<std::size_t>(width) + static_cast<std::size_t>(x);
            const auto background = PixelRgb(state.picker_ring_pixels[index]);
            std::array<double, 3U> accumulated{
                background[0] * 4.0,
                background[1] * 4.0,
                background[2] * 4.0};
            for (const double offset_y : offsets) {
                for (const double offset_x : offsets) {
                    const auto weights = Barycentric(
                        static_cast<double>(x) + offset_x,
                        static_cast<double>(y) + offset_y,
                        geometry);
                    if (weights[0] < 0.0 || weights[1] < 0.0 || weights[2] < 0.0) {
                        continue;
                    }
                    for (std::size_t channel = 0; channel < accumulated.size(); ++channel) {
                        const double triangle = pure_hue[channel] * weights[0] + weights[2];
                        accumulated[channel] += triangle - background[channel];
                    }
                }
            }
            state.picker_triangle_pixels[index] = DibPixel(
                accumulated[0] * 0.25,
                accumulated[1] * 0.25,
                accumulated[2] * 0.25);
        }
    }
    state.picker_cache_hue_degrees = hue_degrees;
    state.picker_triangle_cache_valid = true;
    state.picker_frame_cache_valid = false;
    return true;
}

bool EnsurePickerFrame(
    ColorDockPaneState& state,
    const PickerGeometry& geometry,
    UINT dpi) noexcept {
    if (!EnsureTriangleCache(state, geometry, dpi)) {
        return false;
    }
    const InkpodColorValue& selected_color = ActivePickerColor(state);
    const std::uint32_t rgb_key =
        (static_cast<std::uint32_t>(Channel8(selected_color, selected_color.red))
             << 16U)
        | (static_cast<std::uint32_t>(
               Channel8(selected_color, selected_color.green))
           << 8U)
        | static_cast<std::uint32_t>(
            Channel8(selected_color, selected_color.blue));
    if (state.picker_frame_cache_valid && state.picker_cache_rgb == rgb_key) {
        return true;
    }
    if (!CopyPixelBuffer(state.picker_frame_pixels, state.picker_triangle_pixels)) {
        state.picker_frame_cache_valid = false;
        return false;
    }
    const auto selected_rgb = std::array<double, 3U>{
        ChannelUnit(selected_color, selected_color.red),
        ChannelUnit(selected_color, selected_color.green),
        ChannelUnit(selected_color, selected_color.blue)};
    const auto checker_light = SystemRgb(COLOR_WINDOW);
    const auto checker_dark = SystemRgb(COLOR_3DLIGHT);
    const auto border = SystemRgb(COLOR_WINDOWTEXT);
    const int checker_size = std::max(2, ScaleForDpi(4, dpi));
    const int alpha_width = std::max(
        1,
        static_cast<int>(geometry.alpha_track.right - geometry.alpha_track.left) - 3);
    for (int y = geometry.alpha_track.top; y < geometry.alpha_track.bottom; ++y) {
        for (int x = geometry.alpha_track.left; x < geometry.alpha_track.right; ++x) {
            std::array<double, 3U> pixel = border;
            if (x > geometry.alpha_track.left && x < geometry.alpha_track.right - 1
                && y > geometry.alpha_track.top && y < geometry.alpha_track.bottom - 1) {
                const auto& checker = (((x - geometry.alpha_track.left) / checker_size)
                    + ((y - geometry.alpha_track.top) / checker_size)) % 2 == 0
                    ? checker_light
                    : checker_dark;
                const double alpha = ClampUnit(
                    static_cast<double>(x - geometry.alpha_track.left - 1)
                    / static_cast<double>(alpha_width));
                for (std::size_t channel = 0; channel < pixel.size(); ++channel) {
                    pixel[channel] = selected_rgb[channel] * alpha
                        + checker[channel] * (1.0 - alpha);
                }
            }
            state.picker_frame_pixels[static_cast<std::size_t>(y)
                * static_cast<std::size_t>(state.picker_cache_width)
                + static_cast<std::size_t>(x)] = DibPixel(pixel[0], pixel[1], pixel[2]);
        }
    }
    state.picker_cache_rgb = rgb_key;
    state.picker_frame_cache_valid = true;
    return true;
}

void DrawAntialiasedMarker(
    std::vector<std::uint32_t>& pixels,
    int width,
    int height,
    double center_x,
    double center_y,
    double radius) noexcept {
    const int left = std::max(0, static_cast<int>(std::floor(center_x - radius - 1.0)));
    const int right = std::min(width, static_cast<int>(std::ceil(center_x + radius + 1.0)));
    const int top = std::max(0, static_cast<int>(std::floor(center_y - radius - 1.0)));
    const int bottom = std::min(height, static_cast<int>(std::ceil(center_y + radius + 1.0)));
    const auto outline = std::array<double, 3U>{0.10, 0.10, 0.10};
    const auto fill = std::array<double, 3U>{0.98, 0.98, 0.98};
    const double inner_radius = std::max(0.0, radius - 2.0);
    for (int y = top; y < bottom; ++y) {
        for (int x = left; x < right; ++x) {
            const double delta_x = static_cast<double>(x) + 0.5 - center_x;
            const double delta_y = static_cast<double>(y) + 0.5 - center_y;
            const double distance = std::sqrt(delta_x * delta_x + delta_y * delta_y);
            auto& pixel = pixels[static_cast<std::size_t>(y)
                * static_cast<std::size_t>(width) + static_cast<std::size_t>(x)];
            BlendDibPixel(pixel, outline, ClampUnit(radius + 0.5 - distance));
            BlendDibPixel(pixel, fill, ClampUnit(inner_radius + 0.5 - distance));
        }
    }
}

void DrawPicker(
    const DRAWITEMSTRUCT& draw,
    ColorDockPaneState& state) noexcept {
    const double hue_degrees = ActivePickerHue(state);
    const PickerGeometry geometry = MakePickerGeometry(draw.hwndItem, hue_degrees);
    const int width = static_cast<int>(geometry.client.right);
    const int height = static_cast<int>(geometry.client.bottom);
    if (!geometry.valid || width <= 0 || height <= 0) {
        FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_3DFACE));
        return;
    }
    const UINT dpi = GetDpiForWindow(draw.hwndItem);
    if (!EnsurePickerFrame(state, geometry, dpi)
        || !CopyPixelBuffer(state.picker_present_pixels, state.picker_frame_pixels)) {
        FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(COLOR_3DFACE));
        return;
    }
    const InkpodColorValue& selected_color = ActivePickerColor(state);
    const HsvColor hsv = ToHsv(selected_color, hue_degrees);
    const double hue_radians = hue_degrees * kPi / 180.0;
    const double ring_radius = (geometry.inner_radius + geometry.outer_radius) * 0.5;
    const double marker_radius = static_cast<double>(ScaleForDpi(5, dpi));
    DrawAntialiasedMarker(
        state.picker_present_pixels,
        width,
        height,
        geometry.center_x + std::cos(hue_radians) * ring_radius,
        geometry.center_y + std::sin(hue_radians) * ring_radius,
        marker_radius);
    const double hue_weight = hsv.saturation * hsv.value;
    const double white_weight = (1.0 - hsv.saturation) * hsv.value;
    const double black_weight = 1.0 - hsv.value;
    DrawAntialiasedMarker(
        state.picker_present_pixels,
        width,
        height,
        geometry.hue_vertex.x * hue_weight
            + geometry.white_vertex.x * white_weight
            + geometry.black_vertex.x * black_weight,
        geometry.hue_vertex.y * hue_weight
            + geometry.white_vertex.y * white_weight
            + geometry.black_vertex.y * black_weight,
        marker_radius);
    const double alpha_x = static_cast<double>(geometry.alpha_track.left)
        + ChannelUnit(selected_color, selected_color.alpha)
            * static_cast<double>(
                geometry.alpha_track.right - geometry.alpha_track.left - 1);
    DrawAntialiasedMarker(
        state.picker_present_pixels,
        width,
        height,
        alpha_x,
        static_cast<double>(geometry.alpha_track.top + geometry.alpha_track.bottom) * 0.5,
        marker_radius);
    const std::size_t pixel_count = static_cast<std::size_t>(width)
        * static_cast<std::size_t>(height);
    const bool buffered = state.picker_present_pixels.size() == pixel_count
        && state.picker_paint_buffer.Prepare(draw.hDC, width, height);
    HDC paint_dc = draw.hDC;
    if (buffered) {
        std::memcpy(
            state.picker_paint_buffer.Bits(),
            state.picker_present_pixels.data(),
            pixel_count * sizeof(std::uint32_t));
        paint_dc = state.picker_paint_buffer.Dc();
    } else {
        BlitDibPixels(draw.hDC, width, height, state.picker_present_pixels);
    }

    const auto draw_overlay = [&](HDC target) noexcept {
        const HGDIOBJ old_font = state.font == nullptr
            ? nullptr
            : SelectObject(target, state.font);
        SetBkMode(target, TRANSPARENT);
        SetTextColor(target, GetSysColor(COLOR_WINDOWTEXT));
        TEXTMETRICW metrics{};
        const int line_height = GetTextMetricsW(target, &metrics) != FALSE
            ? static_cast<int>(metrics.tmHeight + metrics.tmExternalLeading)
            : ScaleForDpi(17, dpi);
        const int margin = ScaleForDpi(4, dpi);
        const int color_bottom = static_cast<int>(geometry.client.bottom)
            - ScaleForDpi(46, dpi);
        const int details_top = std::max(
            margin, color_bottom - line_height * 3 - margin);
        const int left_text_right = std::max(
            margin + ScaleForDpi(42, dpi),
            static_cast<int>(std::floor(
                geometry.center_x - geometry.outer_radius)) - margin);
        std::array<wchar_t, 64U> label{};
        const auto draw_left_row = [&](const wchar_t* text, int row) noexcept {
            RECT bounds{
                margin,
                details_top + row * line_height,
                left_text_right,
                details_top + (row + 1) * line_height};
            DrawTextW(
                target,
                text,
                -1,
                &bounds,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        };
        swprintf_s(label.data(), label.size(), L"H: %u", static_cast<unsigned>(
            std::lround(hue_degrees)) % 360U);
        draw_left_row(label.data(), 0);
        swprintf_s(label.data(), label.size(), L"S: %u", static_cast<unsigned>(
            std::lround(hsv.saturation * 100.0)));
        draw_left_row(label.data(), 1);
        swprintf_s(label.data(), label.size(), L"V: %u", static_cast<unsigned>(
            std::lround(hsv.value * 100.0)));
        draw_left_row(label.data(), 2);

        swprintf_s(
            label.data(),
            label.size(),
            L"#%02X%02X%02X",
            Channel8(selected_color, selected_color.red),
            Channel8(selected_color, selected_color.green),
            Channel8(selected_color, selected_color.blue));
        SIZE hex_extent{};
        GetTextExtentPoint32W(
            target,
            label.data(),
            static_cast<int>(wcslen(label.data())),
            &hex_extent);
        const int hex_left = std::max(
            margin, width - margin - static_cast<int>(hex_extent.cx));
        RECT hex_rect{
            hex_left,
            details_top + line_height * 2,
            width - margin,
            details_top + line_height * 3};
        DrawTextW(target, label.data(), -1, &hex_rect,
                  DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

        RECT opacity_label{
            margin,
            color_bottom + ScaleForDpi(1, dpi),
            width - margin,
            geometry.alpha_track.top};
        DrawTextW(target, UiText(UiStringId::Opacity), -1, &opacity_label,
                  DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        const unsigned opacity_percent = static_cast<unsigned>(std::lround(
            ChannelUnit(selected_color, selected_color.alpha) * 100.0));
        swprintf_s(label.data(), label.size(), L"%u%%", opacity_percent);
        DrawTextW(target, label.data(), -1, &opacity_label,
                  DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        if (GetFocus() == draw.hwndItem) {
            RECT focus = geometry.client;
            InflateRect(&focus, -1, -1);
            DrawFocusRect(target, &focus);
        }
        if (old_font != nullptr && old_font != HGDI_ERROR) {
            SelectObject(target, old_font);
        }
    };
    draw_overlay(paint_dc);
    if (buffered && !state.picker_paint_buffer.Present(draw.hDC)) {
        BlitDibPixels(draw.hDC, width, height, state.picker_present_pixels);
        draw_overlay(draw.hDC);
    }
}

void DrawColorLabel(
    const DRAWITEMSTRUCT& draw,
    ColorDockPaneState& state) noexcept {
    RECT client{};
    if (GetClientRect(draw.hwndItem, &client) == FALSE) {
        return;
    }
    const int width = static_cast<int>(client.right - client.left);
    const int height = static_cast<int>(client.bottom - client.top);
    if (width <= 0 || height <= 0) {
        return;
    }
    std::array<wchar_t, 64U> text{};
    GetWindowTextW(
        draw.hwndItem,
        text.data(),
        static_cast<int>(text.size()));
    const auto paint = [&](HDC target) noexcept {
        FillRect(target, &client, GetSysColorBrush(COLOR_3DFACE));
        const HGDIOBJ old_font = state.font == nullptr
            ? nullptr
            : SelectObject(target, state.font);
        SetBkMode(target, TRANSPARENT);
        SetTextColor(target, GetSysColor(COLOR_WINDOWTEXT));
        DrawTextW(
            target,
            text.data(),
            -1,
            &client,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX
                | DT_END_ELLIPSIS);
        if (old_font != nullptr && old_font != HGDI_ERROR) {
            SelectObject(target, old_font);
        }
    };
    if (state.color_label_paint_buffer.Prepare(draw.hDC, width, height)) {
        paint(state.color_label_paint_buffer.Dc());
        if (!state.color_label_paint_buffer.Present(draw.hDC)) {
            paint(draw.hDC);
        }
    } else {
        paint(draw.hDC);
    }
}

void DrawColorListItem(
    const DRAWITEMSTRUCT& draw,
    const ColorDockPaneState& state,
    bool chart) noexcept {
    if (draw.itemID == static_cast<UINT>(-1)) {
        return;
    }
    const std::size_t index = static_cast<std::size_t>(draw.itemData);
    const auto& colors = chart ? state.chart_colors : state.palette_colors;
    if (index >= colors.size()) {
        return;
    }
    const bool selected = (draw.itemState & ODS_SELECTED) != 0U;
    FillRect(
        draw.hDC,
        &draw.rcItem,
        GetSysColorBrush(selected ? COLOR_HIGHLIGHT : COLOR_WINDOW));
    RECT chip = draw.rcItem;
    chip.left += 4;
    chip.top += 3;
    chip.right = chip.left + std::max(12L, draw.rcItem.bottom - draw.rcItem.top - 6L);
    chip.bottom -= 3;
    const HBRUSH color_brush = CreateSolidBrush(ColorRef(colors[index]));
    if (color_brush != nullptr) {
        FillRect(draw.hDC, &chip, color_brush);
        DeleteObject(color_brush);
    }
    FrameRect(
        draw.hDC, &chip, GetSysColorBrush(COLOR_WINDOWTEXT));
    RECT label = draw.rcItem;
    label.left = chip.right + 8;
    SetBkMode(draw.hDC, TRANSPARENT);
    SetTextColor(
        draw.hDC,
        GetSysColor(selected ? COLOR_HIGHLIGHTTEXT : COLOR_WINDOWTEXT));
    std::array<wchar_t, 128U> text{};
    if (chart && index < state.names.size()) {
        wcsncpy_s(text.data(), text.size(), state.names[index].c_str(), _TRUNCATE);
    } else {
        swprintf_s(
            text.data(),
            text.size(),
            L"%u  #%02X%02X%02X",
            static_cast<unsigned>(index + 1U),
            Channel8(colors[index], colors[index].red),
            Channel8(colors[index], colors[index].green),
            Channel8(colors[index], colors[index].blue));
    }
    DrawTextW(
        draw.hDC,
        text.data(),
        -1,
        &label,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    if ((draw.itemState & ODS_FOCUS) != 0U) {
        DrawFocusRect(draw.hDC, &draw.rcItem);
    }
}

void SelectListColor(
    HWND pane,
    ColorDockPaneState& state,
    int control,
    bool chart) noexcept {
    const HWND list = GetDlgItem(pane, control);
    const LRESULT selection = SendMessageW(list, LB_GETCURSEL, 0, 0);
    if (selection == LB_ERR || state.select_color == nullptr) {
        return;
    }
    const LRESULT data = SendMessageW(list, LB_GETITEMDATA, selection, 0);
    if (data != LB_ERR && data >= 0) {
        state.select_color(
            state.context, static_cast<std::uint32_t>(data), chart);
    }
}

int HitPickerTarget(const PickerGeometry& geometry, int x, int y) noexcept {
    if (!geometry.valid) {
        return kPickerDragNone;
    }
    RECT alpha_hit = geometry.alpha_track;
    InflateRect(&alpha_hit, 6, 8);
    const POINT point{x, y};
    if (PtInRect(&alpha_hit, point) != FALSE) {
        return kPickerDragAlpha;
    }
    const double delta_x = static_cast<double>(x) - geometry.center_x;
    const double delta_y = static_cast<double>(y) - geometry.center_y;
    const double distance = std::sqrt(delta_x * delta_x + delta_y * delta_y);
    if (distance >= geometry.inner_radius - 3.0
        && distance <= geometry.outer_radius + 3.0) {
        return kPickerDragHue;
    }
    const auto weights = Barycentric(
        static_cast<double>(x), static_cast<double>(y), geometry);
    if (weights[0] >= 0.0 && weights[1] >= 0.0 && weights[2] >= 0.0) {
        return kPickerDragSaturationValue;
    }
    return kPickerDragNone;
}

void PublishPickerColor(ColorDockPaneState& state) noexcept {
    const InkpodColorValue color = ActivePickerColor(state);
    if (state.picker_targets_main_line) {
        if (state.change_main_line_color != nullptr) {
            state.change_main_line_color(state.context, color);
        }
    } else if (state.change_color != nullptr) {
        state.change_color(state.context, color);
    }
}

void PresentPickerColor(
    HWND picker,
    ColorDockPaneState& state,
    const InkpodColorValue& color,
    bool publish) noexcept {
    const HWND pane = GetParent(picker);
    if (state.picker_targets_main_line) {
        state.main_line_color = color;
        SetColorLabel(pane, IDC_COLOR_MAIN_LINE_LABEL, UiText(UiStringId::MainLineColor), color);
    } else {
        state.drawing_color = color;
        SetColorLabel(pane, IDC_COLOR_DRAWING_LABEL, UiText(UiStringId::DrawingColor), color);
    }
    SetColorFields(pane, color);
    InvalidateRect(GetDlgItem(pane, IDC_COLOR_MAIN_LINE_SWATCH), nullptr, FALSE);
    InvalidateRect(picker, nullptr, FALSE);
    // WM_PAINT is lower priority than the continuous mouse-input stream. Paint
    // the local preview before any external state publication so the ring,
    // triangle, and markers remain attached to the pointer while dragging.
    UpdateWindow(picker);
    if (!publish) {
        return;
    }
    PublishPickerColor(state);
}

void UpdatePickerFromPoint(
    HWND picker,
    ColorDockPaneState& state,
    int x,
    int y,
    bool commit) noexcept {
    double& hue_degrees = ActivePickerHue(state);
    const PickerGeometry geometry = MakePickerGeometry(picker, hue_degrees);
    if (!geometry.valid) {
        return;
    }
    InkpodColorValue color = ActivePickerColor(state);
    HsvColor hsv = ToHsv(color, hue_degrees);
    switch (state.picker_drag_target) {
        case kPickerDragHue: {
            double hue = std::atan2(
                static_cast<double>(y) - geometry.center_y,
                static_cast<double>(x) - geometry.center_x) * 180.0 / kPi;
            if (hue < 0.0) {
                hue += 360.0;
            }
            hue_degrees = hue;
            color = FromHsv(color, hue, hsv.saturation, hsv.value);
            break;
        }
        case kPickerDragSaturationValue: {
            auto weights = Barycentric(
                static_cast<double>(x), static_cast<double>(y), geometry);
            for (double& weight : weights) {
                weight = std::max(0.0, weight);
            }
            const double total = weights[0] + weights[1] + weights[2];
            if (total <= 0.0) {
                return;
            }
            for (double& weight : weights) {
                weight /= total;
            }
            hsv.value = ClampUnit(weights[0] + weights[2]);
            hsv.saturation = hsv.value <= 0.0
                ? 0.0
                : ClampUnit(weights[0] / hsv.value);
            color = FromHsv(
                color,
                hue_degrees,
                hsv.saturation,
                hsv.value);
            break;
        }
        case kPickerDragAlpha: {
            const int width = std::max(
                1,
                static_cast<int>(
                    geometry.alpha_track.right - geometry.alpha_track.left));
            const double alpha = ClampUnit(
                static_cast<double>(x - geometry.alpha_track.left)
                / static_cast<double>(width));
            const double maximum = color.depth == INKPOD_COLOR_DEPTH_16
                ? 65535.0
                : 255.0;
            color.alpha = static_cast<std::uint16_t>(
                std::lround(alpha * maximum));
            break;
        }
        default:
            return;
    }
    PresentPickerColor(picker, state, color, commit);
}

void UpdatePickerFromKeyboard(
    HWND picker,
    ColorDockPaneState& state,
    WPARAM key) noexcept {
    double& hue_degrees = ActivePickerHue(state);
    InkpodColorValue color = ActivePickerColor(state);
    HsvColor hsv = ToHsv(color, hue_degrees);
    const double step = (GetKeyState(VK_CONTROL) & 0x8000) != 0 ? 10.0 : 1.0;
    bool handled = true;
    switch (key) {
        case VK_LEFT:
            hue_degrees = std::fmod(
                hue_degrees + 360.0 - step, 360.0);
            color = FromHsv(
                color,
                hue_degrees,
                hsv.saturation,
                hsv.value);
            break;
        case VK_RIGHT:
            hue_degrees = std::fmod(
                hue_degrees + step, 360.0);
            color = FromHsv(
                color,
                hue_degrees,
                hsv.saturation,
                hsv.value);
            break;
        case VK_UP:
        case VK_DOWN: {
            const double direction = key == VK_UP ? 1.0 : -1.0;
            if ((GetKeyState(VK_SHIFT) & 0x8000) != 0) {
                hsv.saturation = ClampUnit(hsv.saturation + direction * step / 100.0);
            } else {
                hsv.value = ClampUnit(hsv.value + direction * step / 100.0);
            }
            color = FromHsv(
                color,
                hue_degrees,
                hsv.saturation,
                hsv.value);
            break;
        }
        case VK_PRIOR:
        case VK_NEXT: {
            const double maximum = color.depth == INKPOD_COLOR_DEPTH_16
                ? 65535.0
                : 255.0;
            const double direction = key == VK_PRIOR ? 1.0 : -1.0;
            const double alpha = ClampUnit(
                static_cast<double>(color.alpha) / maximum + direction * 0.05);
            color.alpha = static_cast<std::uint16_t>(
                std::lround(alpha * maximum));
            break;
        }
        default:
            handled = false;
            break;
    }
    if (handled) {
        PresentPickerColor(picker, state, color, true);
    }
}

void SelectSwatchTarget(
    HWND swatch,
    ColorDockPaneState& state,
    SwatchTarget target) noexcept {
    if (target == kSwatchNone) {
        return;
    }
    state.picker_targets_main_line = target == kSwatchMainLine;
    double& hue_degrees = ActivePickerHue(state);
    const InkpodColorValue& color = ActivePickerColor(state);
    const HsvColor hsv = ToHsv(color, hue_degrees);
    if (hsv.saturation > 0.0001 && hsv.value > 0.0001) {
        hue_degrees = hsv.hue_degrees;
    }
    const HWND pane = GetParent(swatch);
    SetColorFields(pane, color);
    SetWindowTextW(
        swatch,
        state.picker_targets_main_line
            ? UiText(UiStringId::SelectedMainLineColor)
            : UiText(UiStringId::SelectedDrawingColor));
    InvalidateRect(swatch, nullptr, FALSE);
    InvalidateRect(GetDlgItem(pane, IDC_COLOR_PICKER), nullptr, FALSE);
}

LRESULT CALLBACK ColorLabelSubclassProcedure(
    HWND label,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR) noexcept {
    switch (message) {
        case WM_ERASEBKGND:
            return TRUE;
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                label, ColorLabelSubclassProcedure, kColorLabelSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(label, message, wparam, lparam);
}

LRESULT CALLBACK SwatchSubclassProcedure(
    HWND swatch,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<ColorDockPaneState*>(reference);
    switch (message) {
        case WM_GETDLGCODE:
            return DefSubclassProc(swatch, message, wparam, lparam)
                | DLGC_WANTCHARS;
        case WM_LBUTTONDOWN:
            if (state != nullptr) {
                RECT client{};
                if (GetClientRect(swatch, &client) != FALSE) {
                    SetFocus(swatch);
                    const SwatchGeometry geometry = MakeSwatchGeometry(
                        client.right - client.left,
                        client.bottom - client.top);
                    SelectSwatchTarget(
                        swatch,
                        *state,
                        HitSwatchTarget(
                            geometry,
                            state->picker_targets_main_line,
                            GET_X_LPARAM(lparam),
                            GET_Y_LPARAM(lparam)));
                    return 0;
                }
            }
            break;
        case WM_KEYDOWN:
            if (state != nullptr && (wparam == VK_SPACE || wparam == VK_RETURN)) {
                SelectSwatchTarget(
                    swatch,
                    *state,
                    state->picker_targets_main_line
                        ? kSwatchDrawing
                        : kSwatchMainLine);
                return 0;
            }
            break;
        case WM_SETFOCUS:
        case WM_KILLFOCUS:
            InvalidateRect(swatch, nullptr, FALSE);
            break;
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                swatch, SwatchSubclassProcedure, kSwatchSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(swatch, message, wparam, lparam);
}

LRESULT CALLBACK PickerSubclassProcedure(
    HWND picker,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<ColorDockPaneState*>(reference);
    switch (message) {
        case WM_SIZE:
            // Picker geometry and cached gradients depend on the full client
            // extent, so repaint this owner-drawn child once after its own
            // size changes instead of erasing the complete pane subtree.
            InvalidateRect(picker, nullptr, FALSE);
            break;
        case WM_ERASEBKGND:
            // DrawPicker covers the complete client area and presents the
            // finished color surface and text in one blit.
            return TRUE;
        case WM_GETDLGCODE:
            return DefSubclassProc(picker, message, wparam, lparam) | DLGC_WANTARROWS;
        case WM_LBUTTONDOWN:
            if (state != nullptr) {
                SetFocus(picker);
                const PickerGeometry geometry = MakePickerGeometry(
                    picker, ActivePickerHue(*state));
                state->picker_drag_target = HitPickerTarget(
                    geometry, GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam));
                if (state->picker_drag_target != kPickerDragNone) {
                    state->picker_drag_origin = ActivePickerColor(*state);
                    state->picker_drag_origin_hue = ActivePickerHue(*state);
                    state->picker_preview_active = true;
                    SetCapture(picker);
                    UpdatePickerFromPoint(
                        picker,
                        *state,
                        GET_X_LPARAM(lparam),
                        GET_Y_LPARAM(lparam),
                        false);
                    return 0;
                }
            }
            break;
        case WM_MOUSEMOVE:
            if (state != nullptr && GetCapture() == picker
                && state->picker_drag_target != kPickerDragNone) {
                UpdatePickerFromPoint(
                    picker,
                    *state,
                    GET_X_LPARAM(lparam),
                    GET_Y_LPARAM(lparam),
                    false);
                return 0;
            }
            break;
        case WM_LBUTTONUP:
            if (state != nullptr && GetCapture() == picker) {
                UpdatePickerFromPoint(
                    picker,
                    *state,
                        GET_X_LPARAM(lparam),
                        GET_Y_LPARAM(lparam),
                        false);
                state->picker_drag_target = kPickerDragNone;
                state->picker_preview_active = false;
                ReleaseCapture();
                PublishPickerColor(*state);
                return 0;
            }
            break;
        case WM_CAPTURECHANGED:
        case WM_CANCELMODE:
            if (state != nullptr) {
                state->picker_drag_target = kPickerDragNone;
                if (state->picker_preview_active) {
                    ActivePickerColor(*state) = state->picker_drag_origin;
                    ActivePickerHue(*state) = state->picker_drag_origin_hue;
                    state->picker_preview_active = false;
                    const HWND pane = GetParent(picker);
                    SetColorLabel(
                        pane,
                        state->picker_targets_main_line
                            ? IDC_COLOR_MAIN_LINE_LABEL
                            : IDC_COLOR_DRAWING_LABEL,
                        state->picker_targets_main_line
                            ? UiText(UiStringId::MainLineColor)
                            : UiText(UiStringId::DrawingColor),
                        ActivePickerColor(*state));
                    SetColorFields(pane, ActivePickerColor(*state));
                    InvalidateRect(
                        GetDlgItem(pane, IDC_COLOR_MAIN_LINE_SWATCH),
                        nullptr,
                        FALSE);
                    InvalidateRect(picker, nullptr, FALSE);
                    UpdateWindow(picker);
                }
            }
            break;
        case WM_KEYDOWN:
            if (state != nullptr) {
                UpdatePickerFromKeyboard(picker, *state, wparam);
                if (wparam == VK_LEFT || wparam == VK_RIGHT || wparam == VK_UP
                    || wparam == VK_DOWN || wparam == VK_PRIOR
                    || wparam == VK_NEXT) {
                    return 0;
                }
            }
            break;
        case WM_SETFOCUS:
        case WM_KILLFOCUS:
            InvalidateRect(picker, nullptr, FALSE);
            break;
        case WM_NCDESTROY:
            RemoveWindowSubclass(
                picker, PickerSubclassProcedure, kPickerSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(picker, message, wparam, lparam);
}

LRESULT CALLBACK PaneSubclassProcedure(
    HWND pane,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<ColorDockPaneState*>(reference);
    switch (message) {
        case WM_SIZE:
            LayoutPane(pane);
            if (const HWND picker = GetDlgItem(pane, IDC_COLOR_PICKER);
                picker != nullptr) {
                InvalidateRect(picker, nullptr, FALSE);
            }
            return 0;
        case WM_NOTIFY:
            if (state != nullptr) {
                const auto* notification = reinterpret_cast<const NMHDR*>(lparam);
                if (notification != nullptr && notification->idFrom == IDC_COLOR_TABS
                    && notification->code == TCN_SELCHANGE) {
                    state->active_tab = std::max(
                        0, TabCtrl_GetCurSel(notification->hwndFrom));
                    ShowTabControls(pane, state->active_tab);
                    return 0;
                }
            }
            break;
        case WM_COMMAND: {
            if (state == nullptr || state->updating) {
                break;
            }
            if (LOWORD(wparam) == IDC_COLOR_APPLY && HIWORD(wparam) == BN_CLICKED) {
                ApplyFields(pane, *state);
                return 0;
            }
            if (LOWORD(wparam) == IDC_COLOR_EYEDROPPER
                && HIWORD(wparam) == BN_CLICKED
                && state->dispatch_command != nullptr) {
                state->dispatch_command(state->context, IDM_TOOL_EYEDROPPER);
                return 0;
            }
            if (LOWORD(wparam) == IDC_COLOR_PIN
                && HIWORD(wparam) == BN_CLICKED
                && state->dispatch_command != nullptr) {
                state->dispatch_command(state->context, IDM_COLOR_PIN);
                return 0;
            }
            UINT palette_command{};
            switch (LOWORD(wparam)) {
                case IDC_PALETTE_REGISTER_BUTTON:
                    palette_command = IDM_PALETTE_REGISTER;
                    break;
                case IDC_PALETTE_DELETE_BUTTON:
                    palette_command = IDM_PALETTE_DELETE;
                    break;
                case IDC_PALETTE_CLEAR_BUTTON:
                    palette_command = IDM_PALETTE_CLEAR;
                    break;
                case IDC_PALETTE_LOAD_BUTTON:
                    palette_command = IDM_PALETTE_LOAD;
                    break;
                case IDC_PALETTE_SAVE_BUTTON:
                    palette_command = IDM_PALETTE_SAVE;
                    break;
                default:
                    break;
            }
            if (palette_command != 0U && HIWORD(wparam) == BN_CLICKED
                && state->dispatch_command != nullptr) {
                state->dispatch_command(state->context, palette_command);
                return 0;
            }
            if (LOWORD(wparam) == IDC_PALETTE_PREVIOUS
                && HIWORD(wparam) == BN_CLICKED && state->change_group != nullptr) {
                state->change_group(state->context, -1);
                return 0;
            }
            if (LOWORD(wparam) == IDC_PALETTE_NEXT
                && HIWORD(wparam) == BN_CLICKED && state->change_group != nullptr) {
                state->change_group(state->context, 1);
                return 0;
            }
            if (LOWORD(wparam) == IDC_PALETTE_LIST
                && (HIWORD(wparam) == LBN_SELCHANGE
                    || HIWORD(wparam) == LBN_DBLCLK)) {
                SelectListColor(pane, *state, IDC_PALETTE_LIST, false);
                if (HIWORD(wparam) == LBN_DBLCLK
                    && state->dispatch_command != nullptr) {
                    state->dispatch_command(state->context, IDM_PALETTE_REGISTER);
                }
                return 0;
            }
            if (LOWORD(wparam) == IDC_COLOR_CHART_LIST
                && HIWORD(wparam) == LBN_SELCHANGE) {
                SelectListColor(pane, *state, IDC_COLOR_CHART_LIST, true);
                return 0;
            }
            break;
        }
        case WM_DRAWITEM:
            if (state == nullptr) {
                break;
            }
            if (wparam == IDC_COLOR_MAIN_LINE_LABEL
                || wparam == IDC_COLOR_DRAWING_LABEL) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawColorLabel(*draw, *state);
                }
                return TRUE;
            }
            if (wparam == IDC_COLOR_MAIN_LINE_SWATCH) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawCombinedSwatches(*draw, *state);
                }
                return TRUE;
            }
            if (wparam == IDC_COLOR_SWATCH) {
                return TRUE;
            }
            if (wparam == IDC_COLOR_PICKER) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawPicker(*draw, *state);
                }
                return TRUE;
            }
            if (wparam == IDC_PALETTE_LIST || wparam == IDC_COLOR_CHART_LIST) {
                const auto* draw = reinterpret_cast<const DRAWITEMSTRUCT*>(lparam);
                if (draw != nullptr) {
                    DrawColorListItem(
                        *draw, *state, wparam == IDC_COLOR_CHART_LIST);
                }
                return TRUE;
            }
            break;
        case WM_DPICHANGED_AFTERPARENT:
            if (state != nullptr) {
                UpdateFont(pane, *state);
                LayoutPane(pane);
                InvalidateRect(GetDlgItem(pane, IDC_COLOR_PICKER), nullptr, TRUE);
            }
            return 0;
        case WM_NCDESTROY:
            if (state != nullptr && state->font != nullptr) {
                DeleteObject(state->font);
                state->font = nullptr;
            }
            if (state != nullptr) {
                state->picker_paint_buffer.Reset();
                state->color_label_paint_buffer.Reset();
            }
            SetWindowLongPtrW(pane, GWLP_USERDATA, 0);
            RemoveWindowSubclass(pane, PaneSubclassProcedure, kPaneSubclass);
            break;
        default:
            break;
    }
    return DefSubclassProc(pane, message, wparam, lparam);
}

HWND CreateControl(
    HINSTANCE instance,
    HWND parent,
    const wchar_t* class_name,
    const wchar_t* text,
    DWORD style,
    int id) noexcept {
    return CreateWindowExW(
        0,
        class_name,
        text == nullptr ? L"" : text,
        WS_CHILD | WS_VISIBLE | style,
        0,
        0,
        0,
        0,
        parent,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)),
        instance,
        nullptr);
}

void PopulateLists(HWND pane, ColorDockPaneState& state) noexcept {
    const HWND palette = GetDlgItem(pane, IDC_PALETTE_LIST);
    const HWND chart = GetDlgItem(pane, IDC_COLOR_CHART_LIST);
    SendMessageW(palette, WM_SETREDRAW, FALSE, 0);
    SendMessageW(chart, WM_SETREDRAW, FALSE, 0);
    SendMessageW(palette, LB_RESETCONTENT, 0, 0);
    SendMessageW(chart, LB_RESETCONTENT, 0, 0);
    const std::size_t palette_begin = static_cast<std::size_t>(state.palette_group) * 10U;
    const std::size_t palette_end = std::min(
        state.palette_colors.size(), palette_begin + 10U);
    for (std::size_t index = palette_begin; index < palette_end; ++index) {
        const LRESULT item = SendMessageW(
            palette, LB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L""));
        if (item != LB_ERR && item != LB_ERRSPACE) {
            SendMessageW(palette, LB_SETITEMDATA, item, static_cast<LPARAM>(index));
        }
    }
    const std::size_t chart_begin = static_cast<std::size_t>(state.chart_page) * 20U;
    const std::size_t chart_end = std::min(
        state.chart_colors.size(), chart_begin + 20U);
    for (std::size_t index = chart_begin; index < chart_end; ++index) {
        const LRESULT item = SendMessageW(
            chart, LB_ADDSTRING, 0, reinterpret_cast<LPARAM>(L""));
        if (item != LB_ERR && item != LB_ERRSPACE) {
            SendMessageW(chart, LB_SETITEMDATA, item, static_cast<LPARAM>(index));
        }
    }
    EnableWindow(chart, state.chart_locked ? FALSE : TRUE);
    SendMessageW(palette, WM_SETREDRAW, TRUE, 0);
    SendMessageW(chart, WM_SETREDRAW, TRUE, 0);
    InvalidateRect(palette, nullptr, TRUE);
    InvalidateRect(chart, nullptr, TRUE);
}

}  // namespace

HWND CreateColorDockPane(
    HINSTANCE instance,
    HWND parent,
    ColorDockPaneState& state) noexcept {
    const HWND pane = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        L"STATIC",
        nullptr,
        WS_CHILD | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        parent,
        nullptr,
        instance,
        nullptr);
    if (pane == nullptr) {
        return nullptr;
    }
    const HWND tabs = CreateControl(
        instance,
        pane,
        WC_TABCONTROLW,
        nullptr,
        WS_TABSTOP | WS_CLIPSIBLINGS,
        IDC_COLOR_TABS);
    const bool controls_created = tabs != nullptr
        && CreateControl(
               instance,
               pane,
               L"STATIC",
               UiText(UiStringId::FollowingActive),
               SS_LEFT | SS_CENTERIMAGE | SS_ENDELLIPSIS,
               IDC_COLOR_TARGET)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"BUTTON",
               UiText(UiStringId::PinDocument),
               WS_TABSTOP | BS_PUSHBUTTON,
               IDC_COLOR_PIN)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"STATIC",
               UiText(UiStringId::MainLineColor),
               SS_OWNERDRAW,
               IDC_COLOR_MAIN_LINE_LABEL)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"STATIC",
               UiText(UiStringId::SelectedDrawingColor),
               WS_TABSTOP | SS_OWNERDRAW | SS_NOTIFY,
               IDC_COLOR_MAIN_LINE_SWATCH)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"STATIC",
               UiText(UiStringId::DrawingColor),
               SS_OWNERDRAW,
               IDC_COLOR_DRAWING_LABEL)
            != nullptr
        && CreateControl(
               instance, pane, L"STATIC", UiText(UiStringId::DrawingColor), SS_OWNERDRAW, IDC_COLOR_SWATCH)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"STATIC",
               UiText(UiStringId::ColorChannelsDescription),
               WS_TABSTOP | SS_OWNERDRAW | SS_NOTIFY,
               IDC_COLOR_PICKER)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"BUTTON",
               UiText(UiStringId::ToolEyedropper),
               WS_TABSTOP | BS_PUSHBUTTON,
               IDC_COLOR_EYEDROPPER)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"0", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_RED)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"0", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_GREEN)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"0", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_BLUE)
            != nullptr
        && CreateControl(
               instance, pane, L"EDIT", L"255", WS_BORDER | WS_TABSTOP | ES_NUMBER,
               IDC_COLOR_ALPHA)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Apply), WS_TABSTOP | BS_PUSHBUTTON,
               IDC_COLOR_APPLY)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"LISTBOX",
               nullptr,
               WS_BORDER | WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY
                   | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT,
               IDC_PALETTE_LIST)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", L"<", WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_PREVIOUS)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", L">", WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_NEXT)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Register), WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_REGISTER_BUTTON)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Delete), WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_DELETE_BUTTON)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Clear), WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_CLEAR_BUTTON)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Load), WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_LOAD_BUTTON)
            != nullptr
        && CreateControl(
               instance, pane, L"BUTTON", UiText(UiStringId::Save), WS_TABSTOP | BS_PUSHBUTTON,
               IDC_PALETTE_SAVE_BUTTON)
            != nullptr
        && CreateControl(
               instance,
               pane,
               L"LISTBOX",
               nullptr,
               WS_BORDER | WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY
                   | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT,
               IDC_COLOR_CHART_LIST)
            != nullptr;
    const HWND picker = GetDlgItem(pane, IDC_COLOR_PICKER);
    const HWND swatch = GetDlgItem(pane, IDC_COLOR_MAIN_LINE_SWATCH);
    const HWND main_line_label = GetDlgItem(pane, IDC_COLOR_MAIN_LINE_LABEL);
    const HWND drawing_label = GetDlgItem(pane, IDC_COLOR_DRAWING_LABEL);
    if (!controls_created || picker == nullptr || swatch == nullptr
        || main_line_label == nullptr || drawing_label == nullptr
        || SetWindowSubclass(
               picker,
               PickerSubclassProcedure,
               kPickerSubclass,
               reinterpret_cast<DWORD_PTR>(&state)) == FALSE
        || SetWindowSubclass(
               swatch,
               SwatchSubclassProcedure,
               kSwatchSubclass,
               reinterpret_cast<DWORD_PTR>(&state)) == FALSE) {
        DestroyWindow(pane);
        return nullptr;
    }
    if (SetWindowSubclass(
            main_line_label,
            ColorLabelSubclassProcedure,
            kColorLabelSubclass,
            0U) == FALSE
        || SetWindowSubclass(
               drawing_label,
               ColorLabelSubclassProcedure,
               kColorLabelSubclass,
               0U) == FALSE) {
        DestroyWindow(pane);
        return nullptr;
    }
    for (const auto& [control, cue] : std::array<std::pair<int, const wchar_t*>, 4U>{
             std::pair{IDC_COLOR_RED, L"R"},
             std::pair{IDC_COLOR_GREEN, L"G"},
             std::pair{IDC_COLOR_BLUE, L"B"},
             std::pair{IDC_COLOR_ALPHA, L"A"}}) {
        SendDlgItemMessageW(
            pane,
            control,
            EM_SETCUEBANNER,
            TRUE,
            reinterpret_cast<LPARAM>(cue));
    }
    for (const wchar_t* label : {
             UiText(UiStringId::Color),
             UiText(UiStringId::Palette),
             UiText(UiStringId::Chart)}) {
        TCITEMW item{};
        item.mask = TCIF_TEXT;
        item.pszText = const_cast<wchar_t*>(label);
        TabCtrl_InsertItem(tabs, TabCtrl_GetItemCount(tabs), &item);
    }
    SetWindowLongPtrW(
        pane, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(&state));
    SetWindowSubclass(
        pane,
        PaneSubclassProcedure,
        kPaneSubclass,
        reinterpret_cast<DWORD_PTR>(&state));
    UpdateFont(pane, state);
    ShowTabControls(pane, 0);
    LayoutPane(pane);
    return pane;
}

void UpdateColorDockPane(
    HWND pane,
    const InkpodColorValue& main_line_color,
    const InkpodColorValue& drawing_color,
    const std::vector<InkpodColorValue>& palette_colors,
    const std::vector<InkpodColorValue>& chart_colors,
    const std::vector<std::wstring>& names,
    std::uint32_t palette_group,
    std::uint32_t chart_page,
    bool chart_locked) noexcept {
    ColorDockPaneState* state = pane == nullptr ? nullptr : PaneState(pane);
    if (state == nullptr) {
        return;
    }
    try {
        state->palette_colors = palette_colors;
        state->chart_colors = chart_colors;
        state->names = names;
    } catch (const std::bad_alloc&) {
        return;
    }
    state->palette_group = palette_group;
    state->chart_page = chart_page;
    state->chart_locked = chart_locked;
    UpdateColorDockPaneMainLineColor(pane, main_line_color);
    UpdateColorDockPaneDrawingColor(pane, drawing_color);
    PopulateLists(pane, *state);
    InvalidateRect(GetDlgItem(pane, IDC_COLOR_MAIN_LINE_SWATCH), nullptr, TRUE);
}

void UpdateColorDockPaneTarget(
    HWND pane,
    std::wstring target_text,
    bool target_available,
    bool pinned) noexcept {
    ColorDockPaneState* state = pane == nullptr ? nullptr : PaneState(pane);
    if (state == nullptr) {
        return;
    }
    try {
        state->target_text = std::move(target_text);
    } catch (const std::bad_alloc&) {
        return;
    }
    state->target_available = target_available;
    state->pinned = pinned;
    SetDlgItemTextW(pane, IDC_COLOR_TARGET, state->target_text.c_str());
    SetDlgItemTextW(
        pane,
        IDC_COLOR_PIN,
        pinned ? UiText(UiStringId::ReturnToFollowing) : UiText(UiStringId::PinDocument));
    EnableWindow(GetDlgItem(pane, IDC_COLOR_PIN), target_available ? TRUE : FALSE);
    static_cast<void>(SetPaneIconButton(
        GetDlgItem(pane, IDC_COLOR_PIN),
        pinned ? PaneIconId::ReturnToFollowing : PaneIconId::PinDocument));
    for (const int control : {
             IDC_PALETTE_LIST,
             IDC_PALETTE_PREVIOUS,
             IDC_PALETTE_NEXT,
             IDC_PALETTE_REGISTER_BUTTON,
             IDC_PALETTE_DELETE_BUTTON,
             IDC_PALETTE_CLEAR_BUTTON,
             IDC_PALETTE_LOAD_BUTTON,
             IDC_PALETTE_SAVE_BUTTON}) {
        EnableWindow(GetDlgItem(pane, control), target_available ? TRUE : FALSE);
    }
    LayoutPane(pane);
}

void UpdateColorDockPaneDrawingColor(
    HWND pane,
    const InkpodColorValue& drawing_color) noexcept {
    ColorDockPaneState* state = pane == nullptr ? nullptr : PaneState(pane);
    if (state == nullptr) {
        return;
    }
    state->updating = true;
    state->drawing_color = drawing_color;
    const HsvColor hsv = ToHsv(drawing_color, state->drawing_hue_degrees);
    if (hsv.saturation > 0.0001 && hsv.value > 0.0001) {
        state->drawing_hue_degrees = hsv.hue_degrees;
    }
    SetColorLabel(
        pane, IDC_COLOR_DRAWING_LABEL, UiText(UiStringId::DrawingColor), drawing_color);
    if (!state->picker_targets_main_line) {
        SetColorFields(pane, drawing_color);
        InvalidateRect(GetDlgItem(pane, IDC_COLOR_PICKER), nullptr, FALSE);
    }
    InvalidateRect(GetDlgItem(pane, IDC_COLOR_MAIN_LINE_SWATCH), nullptr, FALSE);
    state->updating = false;
}

void SelectColorDockPaneDrawingColor(HWND pane) noexcept {
    ColorDockPaneState* state = pane == nullptr ? nullptr : PaneState(pane);
    const HWND swatch = pane == nullptr
        ? nullptr
        : GetDlgItem(pane, IDC_COLOR_MAIN_LINE_SWATCH);
    if (state == nullptr || swatch == nullptr) {
        return;
    }
    SelectSwatchTarget(swatch, *state, kSwatchDrawing);
}

void UpdateColorDockPaneMainLineColor(
    HWND pane,
    const InkpodColorValue& main_line_color) noexcept {
    ColorDockPaneState* state = pane == nullptr ? nullptr : PaneState(pane);
    if (state == nullptr) {
        return;
    }
    state->updating = true;
    state->main_line_color = main_line_color;
    const HsvColor hsv = ToHsv(main_line_color, state->main_line_hue_degrees);
    if (hsv.saturation > 0.0001 && hsv.value > 0.0001) {
        state->main_line_hue_degrees = hsv.hue_degrees;
    }
    SetColorLabel(
        pane, IDC_COLOR_MAIN_LINE_LABEL, UiText(UiStringId::MainLineColor), main_line_color);
    if (state->picker_targets_main_line) {
        SetColorFields(pane, main_line_color);
        InvalidateRect(GetDlgItem(pane, IDC_COLOR_PICKER), nullptr, FALSE);
    }
    InvalidateRect(GetDlgItem(pane, IDC_COLOR_MAIN_LINE_SWATCH), nullptr, FALSE);
    state->updating = false;
}

}  // namespace inkpod::windows::ui::panes
