#include "fluent_icons.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <bit>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <new>
#include <span>

#include "app/resource.h"

namespace inkpod::windows::ui {
namespace {

constexpr std::array<std::byte, 8U> kAtlasMagic{
    std::byte{'I'}, std::byte{'N'}, std::byte{'K'}, std::byte{'P'},
    std::byte{'O'}, std::byte{'D'}, std::byte{'I'}, std::byte{'A'}};
constexpr std::uint16_t kAtlasVersion = 1U;
constexpr std::uint16_t kAtlasWidth = 48U;
constexpr std::uint16_t kAtlasHeight = 48U;
constexpr std::size_t kAtlasHeaderBytes = 24U;
constexpr std::size_t kAtlasIconCount = kToolIconCount + kPaneIconCount;
constexpr UINT_PTR kPaneButtonSubclass = 0x49434F4EU;

struct AtlasView {
    const std::uint8_t* masks{};
    std::size_t size{};
};

struct PaneButtonIconState {
    PaneIconId icon{PaneIconId::PinDocument};
    HICON image{};
    UINT dpi{};
    COLORREF foreground{};
    bool enabled{};
};

std::uint16_t ReadU16(const std::byte* value) noexcept {
    return static_cast<std::uint16_t>(std::to_integer<std::uint8_t>(value[0]))
        | static_cast<std::uint16_t>(
            static_cast<std::uint16_t>(
                std::to_integer<std::uint8_t>(value[1]))
            << 8U);
}

std::uint32_t ReadU32(const std::byte* value) noexcept {
    return static_cast<std::uint32_t>(
               std::to_integer<std::uint8_t>(value[0]))
        | (static_cast<std::uint32_t>(
               std::to_integer<std::uint8_t>(value[1]))
           << 8U)
        | (static_cast<std::uint32_t>(
               std::to_integer<std::uint8_t>(value[2]))
           << 16U)
        | (static_cast<std::uint32_t>(
               std::to_integer<std::uint8_t>(value[3]))
           << 24U);
}

std::uint32_t Fnv1a(std::span<const std::uint8_t> bytes) noexcept {
    std::uint32_t hash = UINT32_C(2166136261);
    for (const std::uint8_t byte : bytes) {
        hash ^= byte;
        hash *= UINT32_C(16777619);
    }
    return hash;
}

AtlasView LoadAtlasResource(HINSTANCE instance) noexcept {
    if (instance == nullptr) {
        return {};
    }
    const HRSRC resource = FindResourceW(
        instance,
        MAKEINTRESOURCEW(IDR_FLUENT_ICON_MASK_ATLAS),
        RT_RCDATA);
    if (resource == nullptr) {
        return {};
    }
    const DWORD resource_size = SizeofResource(instance, resource);
    const HGLOBAL loaded = LoadResource(instance, resource);
    const auto* bytes = static_cast<const std::byte*>(LockResource(loaded));
    if (bytes == nullptr || resource_size < kAtlasHeaderBytes
        || !std::equal(kAtlasMagic.begin(), kAtlasMagic.end(), bytes)
        || ReadU16(bytes + 8U) != kAtlasVersion
        || ReadU16(bytes + 10U) != kAtlasWidth
        || ReadU16(bytes + 12U) != kAtlasHeight
        || ReadU16(bytes + 14U) != kAtlasIconCount) {
        return {};
    }
    const std::uint32_t payload_size = ReadU32(bytes + 16U);
    const std::uint32_t expected_hash = ReadU32(bytes + 20U);
    const std::size_t required_size = kAtlasHeaderBytes
        + static_cast<std::size_t>(payload_size);
    const std::size_t expected_payload = kAtlasIconCount
        * static_cast<std::size_t>(kAtlasWidth)
        * static_cast<std::size_t>(kAtlasHeight);
    if (required_size != static_cast<std::size_t>(resource_size)
        || payload_size != expected_payload) {
        return {};
    }
    const auto* masks = reinterpret_cast<const std::uint8_t*>(
        bytes + kAtlasHeaderBytes);
    if (Fnv1a(std::span<const std::uint8_t>(masks, expected_payload))
        != expected_hash) {
        return {};
    }
    return {masks, expected_payload};
}

AtlasView LoadAtlas(HINSTANCE requested_instance) noexcept {
    static const HINSTANCE application_instance = GetModuleHandleW(nullptr);
    static const AtlasView application_atlas =
        LoadAtlasResource(application_instance);
    if (requested_instance == nullptr
        || requested_instance == application_instance) {
        return application_atlas;
    }
    return LoadAtlasResource(requested_instance);
}

std::size_t AtlasIndex(ToolIconId icon) noexcept {
    return static_cast<std::size_t>(icon);
}

std::size_t AtlasIndex(PaneIconId icon) noexcept {
    return kToolIconCount + static_cast<std::size_t>(icon);
}

const std::uint8_t* IconMask(const AtlasView& atlas, std::size_t index) noexcept {
    constexpr std::size_t kMaskBytes =
        static_cast<std::size_t>(kAtlasWidth)
        * static_cast<std::size_t>(kAtlasHeight);
    if (atlas.masks == nullptr || index >= kAtlasIconCount
        || (index + 1U) * kMaskBytes > atlas.size) {
        return nullptr;
    }
    return atlas.masks + index * kMaskBytes;
}

std::uint32_t PremultipliedPixel(COLORREF color, std::uint8_t alpha) noexcept {
    const auto premultiply = [alpha](std::uint8_t component) noexcept {
        return static_cast<std::uint32_t>(
            (static_cast<std::uint32_t>(component) * alpha + 127U) / 255U);
    };
    const std::uint32_t red = premultiply(GetRValue(color));
    const std::uint32_t green = premultiply(GetGValue(color));
    const std::uint32_t blue = premultiply(GetBValue(color));
    return blue | (green << 8U) | (red << 16U)
        | (static_cast<std::uint32_t>(alpha) << 24U);
}

bool DrawAtlasIcon(
    HINSTANCE instance,
    HDC destination,
    RECT bounds,
    std::size_t index,
    COLORREF foreground) noexcept {
    const AtlasView atlas = LoadAtlas(instance);
    const std::uint8_t* mask = IconMask(atlas, index);
    const int available_width = static_cast<int>(bounds.right - bounds.left);
    const int available_height = static_cast<int>(bounds.bottom - bounds.top);
    const int target_size = std::min(available_width, available_height);
    if (destination == nullptr || mask == nullptr || target_size <= 0) {
        return false;
    }
    constexpr std::size_t kMaskPixels =
        static_cast<std::size_t>(kAtlasWidth)
        * static_cast<std::size_t>(kAtlasHeight);
    std::array<std::uint32_t, kMaskPixels> pixels{};
    for (std::size_t pixel = 0; pixel < pixels.size(); ++pixel) {
        pixels[pixel] = PremultipliedPixel(foreground, mask[pixel]);
    }
    BITMAPINFO bitmap{};
    bitmap.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bitmap.bmiHeader.biWidth = kAtlasWidth;
    bitmap.bmiHeader.biHeight = -static_cast<LONG>(kAtlasHeight);
    bitmap.bmiHeader.biPlanes = 1;
    bitmap.bmiHeader.biBitCount = 32;
    bitmap.bmiHeader.biCompression = BI_RGB;
    const int x = bounds.left + (available_width - target_size) / 2;
    const int y = bounds.top + (available_height - target_size) / 2;
    HDC source = CreateCompatibleDC(destination);
    void* dib_pixels{};
    const HBITMAP dib = CreateDIBSection(
        destination, &bitmap, DIB_RGB_COLORS, &dib_pixels, nullptr, 0U);
    if (source == nullptr || dib == nullptr || dib_pixels == nullptr) {
        if (dib != nullptr) DeleteObject(dib);
        if (source != nullptr) DeleteDC(source);
        return false;
    }
    std::memcpy(dib_pixels, pixels.data(), pixels.size() * sizeof(pixels[0]));
    const HGDIOBJ previous = SelectObject(source, dib);
    BLENDFUNCTION blend{};
    blend.BlendOp = AC_SRC_OVER;
    blend.SourceConstantAlpha = 255U;
    blend.AlphaFormat = AC_SRC_ALPHA;
    const bool drawn = AlphaBlend(
        destination,
        x,
        y,
        target_size,
        target_size,
        source,
        0,
        0,
        kAtlasWidth,
        kAtlasHeight,
        blend) != FALSE;
    if (previous != nullptr && previous != HGDI_ERROR) {
        SelectObject(source, previous);
    }
    DeleteObject(dib);
    DeleteDC(source);
    return drawn;
}

HICON CreateAtlasIcon(
    HINSTANCE instance,
    PaneIconId icon,
    COLORREF foreground,
    int requested_size) noexcept {
    const AtlasView atlas = LoadAtlas(instance);
    const std::uint8_t* mask = IconMask(atlas, AtlasIndex(icon));
    if (mask == nullptr) {
        return nullptr;
    }
    constexpr int kSourceSize = static_cast<int>(kAtlasWidth);
    BITMAPINFO bitmap{};
    bitmap.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bitmap.bmiHeader.biWidth = kSourceSize;
    bitmap.bmiHeader.biHeight = -kSourceSize;
    bitmap.bmiHeader.biPlanes = 1;
    bitmap.bmiHeader.biBitCount = 32;
    bitmap.bmiHeader.biCompression = BI_RGB;
    void* dib_pixels{};
    const HBITMAP color = CreateDIBSection(
        nullptr, &bitmap, DIB_RGB_COLORS, &dib_pixels, nullptr, 0U);
    constexpr std::size_t kMaskStride =
        ((static_cast<std::size_t>(kSourceSize) + 15U) / 16U) * 2U;
    std::array<std::uint8_t,
        kMaskStride * static_cast<std::size_t>(kSourceSize)> mask_bits{};
    const HBITMAP monochrome = CreateBitmap(
        kSourceSize, kSourceSize, 1U, 1U, mask_bits.data());
    if (color == nullptr || monochrome == nullptr || dib_pixels == nullptr) {
        if (color != nullptr) DeleteObject(color);
        if (monochrome != nullptr) DeleteObject(monochrome);
        return nullptr;
    }
    auto* pixels = static_cast<std::uint32_t*>(dib_pixels);
    constexpr std::size_t kMaskPixels =
        static_cast<std::size_t>(kAtlasWidth)
        * static_cast<std::size_t>(kAtlasHeight);
    for (std::size_t pixel = 0; pixel < kMaskPixels; ++pixel) {
        pixels[pixel] = PremultipliedPixel(foreground, mask[pixel]);
    }
    ICONINFO info{};
    info.fIcon = TRUE;
    info.hbmColor = color;
    info.hbmMask = monochrome;
    const HICON source = CreateIconIndirect(&info);
    DeleteObject(color);
    DeleteObject(monochrome);
    if (source == nullptr) {
        return nullptr;
    }
    const int size = std::clamp(requested_size, 12, 64);
    const HICON scaled = static_cast<HICON>(
        CopyImage(source, IMAGE_ICON, size, size, 0U));
    DestroyIcon(source);
    return scaled;
}

HCURSOR CreateAtlasCursor(
    HINSTANCE instance,
    ToolIconId icon,
    COLORREF foreground,
    int requested_size) noexcept {
    const AtlasView atlas = LoadAtlas(instance);
    const std::uint8_t* mask = IconMask(atlas, AtlasIndex(icon));
    if (mask == nullptr) {
        return nullptr;
    }
    constexpr int kSourceSize = static_cast<int>(kAtlasWidth);
    BITMAPINFO bitmap{};
    bitmap.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bitmap.bmiHeader.biWidth = kSourceSize;
    bitmap.bmiHeader.biHeight = -kSourceSize;
    bitmap.bmiHeader.biPlanes = 1;
    bitmap.bmiHeader.biBitCount = 32;
    bitmap.bmiHeader.biCompression = BI_RGB;
    void* dib_pixels{};
    const HBITMAP color = CreateDIBSection(
        nullptr, &bitmap, DIB_RGB_COLORS, &dib_pixels, nullptr, 0U);
    constexpr std::size_t kMaskStride =
        ((static_cast<std::size_t>(kSourceSize) + 15U) / 16U) * 2U;
    std::array<std::uint8_t,
        kMaskStride * static_cast<std::size_t>(kSourceSize)> mask_bits{};
    const HBITMAP monochrome = CreateBitmap(
        kSourceSize, kSourceSize, 1U, 1U, mask_bits.data());
    if (color == nullptr || monochrome == nullptr || dib_pixels == nullptr) {
        if (color != nullptr) DeleteObject(color);
        if (monochrome != nullptr) DeleteObject(monochrome);
        return nullptr;
    }
    auto* pixels = static_cast<std::uint32_t*>(dib_pixels);
    constexpr std::size_t kMaskPixels =
        static_cast<std::size_t>(kAtlasWidth)
        * static_cast<std::size_t>(kAtlasHeight);
    for (std::size_t pixel = 0; pixel < kMaskPixels; ++pixel) {
        pixels[pixel] = PremultipliedPixel(foreground, mask[pixel]);
    }
    ICONINFO info{};
    info.fIcon = FALSE;
    info.xHotspot = 5U;
    info.yHotspot = 42U;
    info.hbmColor = color;
    info.hbmMask = monochrome;
    const HCURSOR source = static_cast<HCURSOR>(CreateIconIndirect(&info));
    DeleteObject(color);
    DeleteObject(monochrome);
    if (source == nullptr) {
        return nullptr;
    }
    const int size = std::clamp(requested_size, 16, 64);
    const HCURSOR scaled = static_cast<HCURSOR>(
        CopyImage(source, IMAGE_CURSOR, size, size, 0U));
    DestroyCursor(source);
    return scaled;
}

void ShowTextFallback(HWND button, PaneButtonIconState& state) noexcept {
    const HICON previous = reinterpret_cast<HICON>(
        SendMessageW(button, BM_SETIMAGE, IMAGE_ICON, 0));
    if (previous != nullptr && previous != state.image) {
        DestroyIcon(previous);
    }
    if (state.image != nullptr) {
        DestroyIcon(state.image);
        state.image = nullptr;
    }
    const LONG_PTR style = GetWindowLongPtrW(button, GWL_STYLE);
    SetWindowLongPtrW(button, GWL_STYLE, style & ~static_cast<LONG_PTR>(BS_ICON));
    InvalidateRect(button, nullptr, TRUE);
}

bool UpdatePaneButtonImage(HWND button, PaneButtonIconState& state) noexcept {
    const UINT window_dpi = GetDpiForWindow(button);
    const UINT dpi = window_dpi == 0U ? 96U : window_dpi;
    const bool enabled = IsWindowEnabled(button) != FALSE;
    const COLORREF foreground = GetSysColor(enabled ? COLOR_BTNTEXT : COLOR_GRAYTEXT);
    if (state.image != nullptr && state.dpi == dpi
        && state.foreground == foreground && state.enabled == enabled) {
        return true;
    }
    const HINSTANCE instance = reinterpret_cast<HINSTANCE>(
        GetWindowLongPtrW(button, GWLP_HINSTANCE));
    const int requested_size = MulDiv(16, static_cast<int>(dpi), 96);
    const HICON image = CreateAtlasIcon(
        instance, state.icon, foreground, requested_size);
    if (image == nullptr) {
        ShowTextFallback(button, state);
        return false;
    }
    const LONG_PTR style = GetWindowLongPtrW(button, GWL_STYLE);
    SetWindowLongPtrW(button, GWL_STYLE, style | static_cast<LONG_PTR>(BS_ICON));
    const HICON previous = reinterpret_cast<HICON>(SendMessageW(
        button, BM_SETIMAGE, IMAGE_ICON, reinterpret_cast<LPARAM>(image)));
    if (previous != nullptr && previous != state.image) {
        DestroyIcon(previous);
    }
    if (state.image != nullptr) {
        DestroyIcon(state.image);
    }
    state.image = image;
    state.dpi = dpi;
    state.foreground = foreground;
    state.enabled = enabled;
    InvalidateRect(button, nullptr, TRUE);
    return true;
}

LRESULT CALLBACK PaneIconButtonSubclassProcedure(
    HWND button,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR,
    DWORD_PTR reference) noexcept {
    auto* state = reinterpret_cast<PaneButtonIconState*>(reference);
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(
            button, PaneIconButtonSubclassProcedure, kPaneButtonSubclass);
        if (state != nullptr) {
            if (state->image != nullptr) {
                DestroyIcon(state->image);
            }
            delete state;
        }
        return DefSubclassProc(button, message, wparam, lparam);
    }
    const LRESULT result = DefSubclassProc(button, message, wparam, lparam);
    if (state != nullptr
        && (message == WM_ENABLE || message == WM_THEMECHANGED
            || message == WM_SYSCOLORCHANGE
            || message == WM_DPICHANGED_AFTERPARENT)) {
        state->dpi = 0U;
        static_cast<void>(UpdatePaneButtonImage(button, *state));
    }
    return result;
}

}  // namespace

bool FluentIconResourceAvailable(HINSTANCE instance) noexcept {
    return LoadAtlas(instance).masks != nullptr;
}

bool DrawToolIcon(
    HINSTANCE instance,
    HDC destination,
    RECT bounds,
    ToolIconId icon,
    COLORREF foreground) noexcept {
    return DrawAtlasIcon(
        instance, destination, bounds, AtlasIndex(icon), foreground);
}

bool DrawPaneIcon(
    HINSTANCE instance,
    HDC destination,
    RECT bounds,
    PaneIconId icon,
    COLORREF foreground) noexcept {
    return DrawAtlasIcon(
        instance, destination, bounds, AtlasIndex(icon), foreground);
}

bool SetPaneIconButton(HWND button, PaneIconId icon) noexcept {
    if (button == nullptr) {
        return false;
    }
    DWORD_PTR reference{};
    PaneButtonIconState* state{};
    if (GetWindowSubclass(
            button,
            PaneIconButtonSubclassProcedure,
            kPaneButtonSubclass,
            &reference) != FALSE) {
        state = reinterpret_cast<PaneButtonIconState*>(reference);
    } else {
        state = new (std::nothrow) PaneButtonIconState{};
        if (state == nullptr
            || SetWindowSubclass(
                   button,
                   PaneIconButtonSubclassProcedure,
                   kPaneButtonSubclass,
                   reinterpret_cast<DWORD_PTR>(state)) == FALSE) {
            delete state;
            return false;
        }
    }
    if (state == nullptr) {
        return false;
    }
    if (state->icon != icon) {
        state->icon = icon;
        state->dpi = 0U;
    }
    return UpdatePaneButtonImage(button, *state);
}

HCURSOR CreateToolCursor(
    HINSTANCE instance, ToolIconId icon, UINT dpi) noexcept {
    const UINT effective_dpi = dpi == 0U ? 96U : dpi;
    return CreateAtlasCursor(
        instance,
        icon,
        GetSysColor(COLOR_WINDOWTEXT),
        MulDiv(24, static_cast<int>(effective_dpi), 96));
}

}  // namespace inkpod::windows::ui
