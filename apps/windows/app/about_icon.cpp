#include "about_icon.h"

#include <wincodec.h>
#include <wrl/client.h>

#include <cstddef>
#include <cstring>
#include <memory>
#include <new>

namespace inkpod::windows {

HICON LoadPngIconResource(
    HINSTANCE instance,
    int resource_id,
    int icon_size) noexcept {
    constexpr int kMaximumAboutIconSize = 1024;
    if (instance == nullptr
        || icon_size <= 0
        || icon_size > kMaximumAboutIconSize) {
        return nullptr;
    }
    const HRSRC resource = FindResourceW(
        instance, MAKEINTRESOURCEW(resource_id), RT_RCDATA);
    if (resource == nullptr) {
        return nullptr;
    }
    const DWORD resource_size = SizeofResource(instance, resource);
    const HGLOBAL loaded_resource = LoadResource(instance, resource);
    if (resource_size == 0U || loaded_resource == nullptr) {
        return nullptr;
    }
    auto* resource_bytes = static_cast<BYTE*>(LockResource(loaded_resource));
    if (resource_bytes == nullptr) {
        return nullptr;
    }

    using Microsoft::WRL::ComPtr;
    ComPtr<IWICImagingFactory> factory;
    ComPtr<IWICStream> stream;
    ComPtr<IWICBitmapDecoder> decoder;
    ComPtr<IWICBitmapFrameDecode> frame;
    if (FAILED(CoCreateInstance(
            CLSID_WICImagingFactory,
            nullptr,
            CLSCTX_INPROC_SERVER,
            IID_PPV_ARGS(&factory)))
        || FAILED(factory->CreateStream(&stream))
        || FAILED(stream->InitializeFromMemory(resource_bytes, resource_size))
        || FAILED(factory->CreateDecoderFromStream(
            stream.Get(),
            nullptr,
            WICDecodeMetadataCacheOnLoad,
            &decoder))
        || FAILED(decoder->GetFrame(0, &frame))) {
        return nullptr;
    }

    UINT source_width = 0U;
    UINT source_height = 0U;
    if (FAILED(frame->GetSize(&source_width, &source_height))) {
        return nullptr;
    }
    ComPtr<IWICBitmapSource> bitmap_source = frame;
    ComPtr<IWICBitmapScaler> scaler;
    const UINT target_size = static_cast<UINT>(icon_size);
    if (source_width != target_size || source_height != target_size) {
        if (FAILED(factory->CreateBitmapScaler(&scaler))
            || FAILED(scaler->Initialize(
                frame.Get(),
                target_size,
                target_size,
                WICBitmapInterpolationModeFant))) {
            return nullptr;
        }
        bitmap_source = scaler;
    }

    ComPtr<IWICFormatConverter> converter;
    if (FAILED(factory->CreateFormatConverter(&converter))
        || FAILED(converter->Initialize(
            bitmap_source.Get(),
            GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            nullptr,
            0.0,
            WICBitmapPaletteTypeCustom))) {
        return nullptr;
    }

    const UINT stride = target_size * 4U;
    const UINT buffer_size = stride * target_size;
    BITMAPINFO bitmap_info{};
    bitmap_info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bitmap_info.bmiHeader.biWidth = icon_size;
    bitmap_info.bmiHeader.biHeight = -icon_size;
    bitmap_info.bmiHeader.biPlanes = 1;
    bitmap_info.bmiHeader.biBitCount = 32;
    bitmap_info.bmiHeader.biCompression = BI_RGB;
    void* color_bits = nullptr;
    const HBITMAP color_bitmap = CreateDIBSection(
        nullptr,
        &bitmap_info,
        DIB_RGB_COLORS,
        &color_bits,
        nullptr,
        0U);
    if (color_bitmap == nullptr
        || color_bits == nullptr
        || FAILED(converter->CopyPixels(
            nullptr,
            stride,
            buffer_size,
            static_cast<BYTE*>(color_bits)))) {
        if (color_bitmap != nullptr) {
            DeleteObject(color_bitmap);
        }
        return nullptr;
    }

    const std::size_t mask_stride =
        (static_cast<std::size_t>(icon_size) + 15U) / 16U * 2U;
    const std::size_t mask_size =
        mask_stride * static_cast<std::size_t>(icon_size);
    std::unique_ptr<BYTE[]> mask_bits{
        new (std::nothrow) BYTE[mask_size]};
    if (mask_bits == nullptr) {
        DeleteObject(color_bitmap);
        return nullptr;
    }
    std::memset(mask_bits.get(), 0, mask_size);
    const HBITMAP mask_bitmap = CreateBitmap(
        icon_size,
        icon_size,
        1U,
        1U,
        mask_bits.get());
    if (mask_bitmap == nullptr) {
        DeleteObject(color_bitmap);
        return nullptr;
    }

    ICONINFO icon_info{};
    icon_info.fIcon = TRUE;
    icon_info.hbmColor = color_bitmap;
    icon_info.hbmMask = mask_bitmap;
    const HICON icon = CreateIconIndirect(&icon_info);
    DeleteObject(mask_bitmap);
    DeleteObject(color_bitmap);
    return icon;
}

}  // namespace inkpod::windows
