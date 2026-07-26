#include "clipboard_adapter.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <new>
#include <vector>

namespace inkpod::app {
namespace {

struct ClipboardPrivateHeader {
    std::uint32_t magic{UINT32_C(0x504b4e49)};
    std::uint32_t version{1U};
    std::int32_t origin_x{};
    std::int32_t origin_y{};
    std::uint32_t width{};
    std::uint32_t height{};
    std::uint64_t stride{};
    std::uint64_t bytes{};
};

} // namespace

UINT InkpodClipboardFormat() noexcept {
    static const UINT format = RegisterClipboardFormatW(L"inkpod/typed-rgba-v1");
    return format;
}

bool PublishStandardClipboard(HWND owner, const InkpodClipboard* clipboard) noexcept {
    if (clipboard == nullptr) {
        return false;
    }
    InkpodClipboardRasterBuffer view{};
    view.struct_size = sizeof(view);
    if (inkpod_clipboard_render_rgba8(clipboard, &view) != INKPOD_STATUS_OK
        || view.required_bytes == 0U
        || view.required_bytes > static_cast<std::uint64_t>(SIZE_MAX)) {
        return false;
    }
    std::vector<std::uint8_t> rgba;
    try {
        rgba.resize(static_cast<std::size_t>(view.required_bytes));
    } catch (const std::bad_alloc&) {
        return false;
    }
    view.pixels_rgba8 = rgba.data();
    view.pixel_capacity = rgba.size();
    if (inkpod_clipboard_render_rgba8(clipboard, &view) != INKPOD_STATUS_OK) {
        return false;
    }
    const std::uint64_t dib_bytes_u64 = sizeof(BITMAPV5HEADER) + view.required_bytes;
    const std::uint64_t private_bytes_u64 = sizeof(ClipboardPrivateHeader) + view.required_bytes;
    if (dib_bytes_u64 > static_cast<std::uint64_t>(SIZE_MAX)
        || private_bytes_u64 > static_cast<std::uint64_t>(SIZE_MAX)) {
        return false;
    }
    HGLOBAL dib = GlobalAlloc(GMEM_MOVEABLE, static_cast<SIZE_T>(dib_bytes_u64));
    HGLOBAL typed = GlobalAlloc(GMEM_MOVEABLE, static_cast<SIZE_T>(private_bytes_u64));
    if (dib == nullptr || typed == nullptr) {
        if (dib != nullptr) {
            GlobalFree(dib);
        }
        if (typed != nullptr) {
            GlobalFree(typed);
        }
        return false;
    }
    auto* dib_memory = static_cast<std::uint8_t*>(GlobalLock(dib));
    auto* typed_memory = static_cast<std::uint8_t*>(GlobalLock(typed));
    if (dib_memory == nullptr || typed_memory == nullptr) {
        if (dib_memory != nullptr) {
            GlobalUnlock(dib);
        }
        if (typed_memory != nullptr) {
            GlobalUnlock(typed);
        }
        GlobalFree(dib);
        GlobalFree(typed);
        return false;
    }
    BITMAPV5HEADER header{};
    header.bV5Size = sizeof(header);
    header.bV5Width = static_cast<LONG>(view.width);
    header.bV5Height = -static_cast<LONG>(view.height);
    header.bV5Planes = 1U;
    header.bV5BitCount = 32U;
    header.bV5Compression = BI_BITFIELDS;
    header.bV5SizeImage = static_cast<DWORD>(std::min<std::uint64_t>(
        view.required_bytes, static_cast<std::uint64_t>(UINT32_MAX)));
    header.bV5RedMask = UINT32_C(0x00ff0000);
    header.bV5GreenMask = UINT32_C(0x0000ff00);
    header.bV5BlueMask = UINT32_C(0x000000ff);
    header.bV5AlphaMask = UINT32_C(0xff000000);
    header.bV5CSType = LCS_sRGB;
    std::memcpy(dib_memory, &header, sizeof(header));
    auto* bgra = dib_memory + sizeof(header);
    for (std::uint32_t y = 0U; y < view.height; ++y) {
        for (std::uint32_t x = 0U; x < view.width; ++x) {
            const std::size_t offset = static_cast<std::size_t>(
                static_cast<std::uint64_t>(y) * view.row_stride_bytes
                + static_cast<std::uint64_t>(x) * 4U);
            bgra[offset] = rgba[offset + 2U];
            bgra[offset + 1U] = rgba[offset + 1U];
            bgra[offset + 2U] = rgba[offset];
            bgra[offset + 3U] = rgba[offset + 3U];
        }
    }
    const ClipboardPrivateHeader private_header{
        UINT32_C(0x504b4e49),
        1U,
        view.origin_x,
        view.origin_y,
        view.width,
        view.height,
        view.row_stride_bytes,
        view.required_bytes};
    std::memcpy(typed_memory, &private_header, sizeof(private_header));
    std::memcpy(typed_memory + sizeof(private_header), rgba.data(), rgba.size());
    GlobalUnlock(dib);
    GlobalUnlock(typed);
    if (OpenClipboard(owner) == FALSE) {
        GlobalFree(dib);
        GlobalFree(typed);
        return false;
    }
    EmptyClipboard();
    const bool dib_ok = SetClipboardData(CF_DIBV5, dib) != nullptr;
    if (!dib_ok) {
        GlobalFree(dib);
    }
    const UINT typed_format = InkpodClipboardFormat();
    const bool typed_ok = typed_format != 0U
        && SetClipboardData(typed_format, typed) != nullptr;
    if (!typed_ok) {
        GlobalFree(typed);
    }
    CloseClipboard();
    return dib_ok && typed_ok;
}

bool ImportStandardClipboard(HWND owner, InkpodClipboard*& output) noexcept {
    const UINT typed_format = InkpodClipboardFormat();
    if (OpenClipboard(owner) == FALSE) {
        return false;
    }
    InkpodClipboard* replacement{};
    InkpodStatus status = INKPOD_STATUS_INVALID_ARGUMENT;
    if (typed_format != 0U) {
        HANDLE handle = GetClipboardData(typed_format);
        if (handle != nullptr) {
            const SIZE_T bytes = GlobalSize(handle);
            const auto* memory = static_cast<const std::uint8_t*>(GlobalLock(handle));
            if (memory != nullptr && bytes >= sizeof(ClipboardPrivateHeader)) {
                ClipboardPrivateHeader header{};
                std::memcpy(&header, memory, sizeof(header));
                const bool valid = header.magic == UINT32_C(0x504b4e49)
                    && header.version == 1U && header.width != 0U && header.height != 0U
                    && header.stride >= static_cast<std::uint64_t>(header.width) * 4U
                    && header.bytes <= bytes - sizeof(ClipboardPrivateHeader)
                    && header.stride <= header.bytes
                    && header.height <= header.bytes / header.stride;
                if (valid) {
                    const InkpodClipboardRgbaInput input{
                        sizeof(InkpodClipboardRgbaInput),
                        0U,
                        header.origin_x,
                        header.origin_y,
                        header.width,
                        header.height,
                        memory + sizeof(ClipboardPrivateHeader),
                        header.bytes,
                        header.stride};
                    status = inkpod_clipboard_create_rgba8(&input, &replacement);
                }
            }
            if (memory != nullptr) {
                GlobalUnlock(handle);
            }
        }
    }

    // Private data preserves the Inkpod document origin. For images copied by
    // another application, import a conventional DIB at document origin (0, 0).
    for (const UINT format : {CF_DIBV5, CF_DIB}) {
        if (status == INKPOD_STATUS_OK) {
            break;
        }
        HANDLE handle = GetClipboardData(format);
        if (handle == nullptr) {
            continue;
        }
        const SIZE_T bytes = GlobalSize(handle);
        const auto* memory = static_cast<const std::uint8_t*>(GlobalLock(handle));
        if (memory == nullptr || bytes < sizeof(BITMAPINFOHEADER)) {
            if (memory != nullptr) {
                GlobalUnlock(handle);
            }
            continue;
        }
        BITMAPINFOHEADER header{};
        std::memcpy(&header, memory, sizeof(header));
        const std::int64_t height64 = header.biHeight < 0
            ? -static_cast<std::int64_t>(header.biHeight)
            : static_cast<std::int64_t>(header.biHeight);
        const bool bitfields = header.biCompression == BI_BITFIELDS;
        const bool format_valid = header.biSize >= sizeof(BITMAPINFOHEADER)
            && header.biSize <= bytes && header.biWidth > 0 && height64 > 0
            && height64 <= static_cast<std::int64_t>(UINT32_MAX)
            && header.biPlanes == 1U && (header.biBitCount == 24U || header.biBitCount == 32U)
            && (header.biCompression == BI_RGB || (bitfields && header.biBitCount == 32U));
        std::uint64_t pixel_offset = header.biSize;
        std::array<std::uint32_t, 4U> masks{
            UINT32_C(0x00ff0000), UINT32_C(0x0000ff00),
            UINT32_C(0x000000ff), UINT32_C(0xff000000)};
        if (format_valid && bitfields) {
            if (header.biSize >= sizeof(BITMAPV4HEADER)) {
                std::memcpy(masks.data(), memory + sizeof(BITMAPINFOHEADER), sizeof(masks));
            } else {
                if (bytes - pixel_offset < 3U * sizeof(std::uint32_t)) {
                    GlobalUnlock(handle);
                    continue;
                }
                std::memcpy(masks.data(), memory + pixel_offset, 3U * sizeof(std::uint32_t));
                masks[3] = 0U;
                pixel_offset += 3U * sizeof(std::uint32_t);
            }
        }
        const std::uint64_t width = format_valid
            ? static_cast<std::uint64_t>(header.biWidth)
            : 0U;
        const std::uint64_t bits_per_row = width * header.biBitCount;
        const std::uint64_t row_stride = ((bits_per_row + 31U) / 32U) * 4U;
        const std::uint64_t required = row_stride * static_cast<std::uint64_t>(height64);
        const std::uint64_t rgba_bytes = width * static_cast<std::uint64_t>(height64) * 4U;
        if (!format_valid || pixel_offset > bytes || required > bytes - pixel_offset
            || rgba_bytes > UINT64_C(536870912) || rgba_bytes > SIZE_MAX) {
            GlobalUnlock(handle);
            continue;
        }
        std::vector<std::uint8_t> rgba;
        try {
            rgba.resize(static_cast<std::size_t>(rgba_bytes));
        } catch (const std::bad_alloc&) {
            GlobalUnlock(handle);
            continue;
        }
        const auto channel = [](std::uint32_t value, std::uint32_t mask, std::uint8_t fallback) {
            if (mask == 0U) {
                return fallback;
            }
            std::uint32_t shift{};
            while (((mask >> shift) & 1U) == 0U && shift < 31U) {
                ++shift;
            }
            const std::uint32_t maximum = mask >> shift;
            return static_cast<std::uint8_t>(
                ((value & mask) >> shift) * UINT32_C(255) / maximum);
        };
        const std::uint32_t height = static_cast<std::uint32_t>(height64);
        for (std::uint32_t y = 0U; y < height; ++y) {
            const std::uint32_t source_y = header.biHeight < 0
                ? y
                : height - 1U - y;
            const auto* source = memory + pixel_offset
                + static_cast<std::uint64_t>(source_y) * row_stride;
            auto* destination = rgba.data()
                + static_cast<std::uint64_t>(y) * width * 4U;
            for (std::uint32_t x = 0U; x < width; ++x) {
                if (header.biBitCount == 24U) {
                    destination[x * 4U] = source[x * 3U + 2U];
                    destination[x * 4U + 1U] = source[x * 3U + 1U];
                    destination[x * 4U + 2U] = source[x * 3U];
                    destination[x * 4U + 3U] = 255U;
                } else {
                    std::uint32_t value{};
                    std::memcpy(&value, source + x * 4U, sizeof(value));
                    destination[x * 4U] = channel(value, masks[0], 0U);
                    destination[x * 4U + 1U] = channel(value, masks[1], 0U);
                    destination[x * 4U + 2U] = channel(value, masks[2], 0U);
                    destination[x * 4U + 3U] = bitfields
                        ? channel(value, masks[3], 255U)
                        : 255U;
                }
            }
        }
        const InkpodClipboardRgbaInput input{
            sizeof(InkpodClipboardRgbaInput),
            0U,
            0,
            0,
            static_cast<std::uint32_t>(width),
            height,
            rgba.data(),
            rgba.size(),
            width * 4U};
        status = inkpod_clipboard_create_rgba8(&input, &replacement);
        GlobalUnlock(handle);
    }
    CloseClipboard();
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    inkpod_clipboard_release(&output);
    output = replacement;
    return true;
}

} // namespace inkpod::app
