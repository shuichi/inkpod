#include "canvas.h"

#include <d2d1_1.h>
#include <d3d11.h>
#include <dxgi1_2.h>
#include <wrl/client.h>

#include <new>

namespace inkpod::renderer {
namespace {

using Microsoft::WRL::ComPtr;

constexpr wchar_t kCanvasClassName[] = L"InkpodCanvasWindow";

class CanvasRenderer final {
public:
    explicit CanvasRenderer(HWND window) noexcept : window_(window) {}

    HRESULT Initialize() noexcept {
        return CreateDeviceResources();
    }

    HRESULT Resize(UINT width, UINT height) noexcept {
        if (width == 0U || height == 0U) {
            return S_OK;
        }
        if (!swap_chain_) {
            return CreateDeviceResources();
        }

        d2d_context_->SetTarget(nullptr);
        target_bitmap_.Reset();
        HRESULT result = swap_chain_->ResizeBuffers(
            0U, width, height, DXGI_FORMAT_UNKNOWN, 0U);
        if (IsDeviceLost(result)) {
            return RecreateAfterDeviceLoss();
        }
        if (FAILED(result)) {
            return result;
        }
        return CreateTargetBitmap();
    }

    HRESULT Render() noexcept {
        if (!d2d_context_ || !target_bitmap_) {
            const HRESULT create_result = CreateDeviceResources();
            if (FAILED(create_result)) {
                return create_result;
            }
        }

        d2d_context_->BeginDraw();
        d2d_context_->Clear(D2D1::ColorF(0.12F, 0.13F, 0.15F, 1.0F));
        HRESULT result = d2d_context_->EndDraw();
        if (result == D2DERR_RECREATE_TARGET) {
            target_bitmap_.Reset();
            result = CreateTargetBitmap();
        }
        if (FAILED(result)) {
            return result;
        }

        result = swap_chain_->Present(1U, 0U);
        if (result == DXGI_STATUS_OCCLUDED) {
            return S_OK;
        }
        if (IsDeviceLost(result)) {
            return RecreateAfterDeviceLoss();
        }
        return result;
    }

    void SetSnapshotRevision(std::uint64_t revision) noexcept {
        if (snapshot_revision_ != revision) {
            snapshot_revision_ = revision;
            InvalidateRect(window_, nullptr, FALSE);
        }
    }

private:
    static bool IsDeviceLost(HRESULT result) noexcept {
        return result == DXGI_ERROR_DEVICE_REMOVED || result == DXGI_ERROR_DEVICE_RESET;
    }

    HRESULT CreateDeviceResources() noexcept {
        DiscardDeviceResources();

        constexpr D3D_FEATURE_LEVEL feature_levels[] = {
            D3D_FEATURE_LEVEL_11_1,
            D3D_FEATURE_LEVEL_11_0,
            D3D_FEATURE_LEVEL_10_1,
            D3D_FEATURE_LEVEL_10_0,
        };
        UINT creation_flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
#if defined(_DEBUG)
        creation_flags |= D3D11_CREATE_DEVICE_DEBUG;
#endif
        D3D_FEATURE_LEVEL selected_level{};
        HRESULT result = D3D11CreateDevice(
            nullptr,
            D3D_DRIVER_TYPE_HARDWARE,
            nullptr,
            creation_flags,
            feature_levels,
            ARRAYSIZE(feature_levels),
            D3D11_SDK_VERSION,
            &d3d_device_,
            &selected_level,
            &d3d_context_);
        if (FAILED(result) && (creation_flags & D3D11_CREATE_DEVICE_DEBUG) != 0U) {
            creation_flags &= ~D3D11_CREATE_DEVICE_DEBUG;
            result = D3D11CreateDevice(
                nullptr,
                D3D_DRIVER_TYPE_HARDWARE,
                nullptr,
                creation_flags,
                feature_levels,
                ARRAYSIZE(feature_levels),
                D3D11_SDK_VERSION,
                &d3d_device_,
                &selected_level,
                &d3d_context_);
        }
        if (FAILED(result)) {
            result = D3D11CreateDevice(
                nullptr,
                D3D_DRIVER_TYPE_WARP,
                nullptr,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                feature_levels,
                ARRAYSIZE(feature_levels),
                D3D11_SDK_VERSION,
                &d3d_device_,
                &selected_level,
                &d3d_context_);
        }
        if (FAILED(result)) {
            return result;
        }

        ComPtr<IDXGIDevice> dxgi_device;
        result = d3d_device_.As(&dxgi_device);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<IDXGIAdapter> adapter;
        result = dxgi_device->GetAdapter(&adapter);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<IDXGIFactory2> dxgi_factory;
        result = adapter->GetParent(IID_PPV_ARGS(&dxgi_factory));
        if (FAILED(result)) {
            return result;
        }

        DXGI_SWAP_CHAIN_DESC1 swap_chain_description{};
        swap_chain_description.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        swap_chain_description.SampleDesc.Count = 1U;
        swap_chain_description.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        swap_chain_description.BufferCount = 2U;
        swap_chain_description.Scaling = DXGI_SCALING_STRETCH;
        swap_chain_description.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
        swap_chain_description.AlphaMode = DXGI_ALPHA_MODE_IGNORE;
        result = dxgi_factory->CreateSwapChainForHwnd(
            d3d_device_.Get(),
            window_,
            &swap_chain_description,
            nullptr,
            nullptr,
            &swap_chain_);
        if (FAILED(result)) {
            return result;
        }
        result = dxgi_factory->MakeWindowAssociation(
            GetAncestor(window_, GA_ROOT), DXGI_MWA_NO_ALT_ENTER);
        if (FAILED(result)) {
            return result;
        }

        D2D1_FACTORY_OPTIONS factory_options{};
        result = D2D1CreateFactory(
            D2D1_FACTORY_TYPE_SINGLE_THREADED,
            __uuidof(ID2D1Factory1),
            &factory_options,
            reinterpret_cast<void**>(d2d_factory_.GetAddressOf()));
        if (FAILED(result)) {
            return result;
        }
        result = d2d_factory_->CreateDevice(dxgi_device.Get(), &d2d_device_);
        if (FAILED(result)) {
            return result;
        }
        result = d2d_device_->CreateDeviceContext(
            D2D1_DEVICE_CONTEXT_OPTIONS_NONE, &d2d_context_);
        if (FAILED(result)) {
            return result;
        }
        return CreateTargetBitmap();
    }

    HRESULT CreateTargetBitmap() noexcept {
        ComPtr<IDXGISurface> surface;
        HRESULT result = swap_chain_->GetBuffer(0U, IID_PPV_ARGS(&surface));
        if (FAILED(result)) {
            return result;
        }

        const float dpi = static_cast<float>(GetDpiForWindow(window_));
        const D2D1_BITMAP_PROPERTIES1 properties = D2D1::BitmapProperties1(
            D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            D2D1::PixelFormat(
                DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_IGNORE),
            dpi,
            dpi);
        result = d2d_context_->CreateBitmapFromDxgiSurface(
            surface.Get(), &properties, &target_bitmap_);
        if (SUCCEEDED(result)) {
            d2d_context_->SetTarget(target_bitmap_.Get());
        }
        return result;
    }

    HRESULT RecreateAfterDeviceLoss() noexcept {
        const HRESULT result = CreateDeviceResources();
        if (SUCCEEDED(result)) {
            InvalidateRect(window_, nullptr, FALSE);
        }
        return result;
    }

    void DiscardDeviceResources() noexcept {
        if (d2d_context_) {
            d2d_context_->SetTarget(nullptr);
        }
        target_bitmap_.Reset();
        d2d_context_.Reset();
        d2d_device_.Reset();
        d2d_factory_.Reset();
        swap_chain_.Reset();
        d3d_context_.Reset();
        d3d_device_.Reset();
    }

    HWND window_{};
    std::uint64_t snapshot_revision_{UINT64_MAX};
    ComPtr<ID3D11Device> d3d_device_;
    ComPtr<ID3D11DeviceContext> d3d_context_;
    ComPtr<IDXGISwapChain1> swap_chain_;
    ComPtr<ID2D1Factory1> d2d_factory_;
    ComPtr<ID2D1Device> d2d_device_;
    ComPtr<ID2D1DeviceContext> d2d_context_;
    ComPtr<ID2D1Bitmap1> target_bitmap_;
};

LRESULT CALLBACK CanvasWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* renderer = reinterpret_cast<CanvasRenderer*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    switch (message) {
        case WM_CREATE: {
            auto* created = new (std::nothrow) CanvasRenderer(window);
            if (created == nullptr) {
                SetLastError(ERROR_NOT_ENOUGH_MEMORY);
                return -1;
            }
            const HRESULT result = created->Initialize();
            if (FAILED(result)) {
                delete created;
                SetLastError(static_cast<DWORD>(HRESULT_CODE(result)));
                return -1;
            }
            SetWindowLongPtrW(
                window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(created));
            return 0;
        }
        case WM_SIZE:
            if (renderer != nullptr && wparam != SIZE_MINIMIZED) {
                const HRESULT result = renderer->Resize(
                    LOWORD(lparam), HIWORD(lparam));
                if (FAILED(result)) {
                    PostMessageW(
                        GetParent(window),
                        kCanvasRenderFailed,
                        static_cast<WPARAM>(result),
                        0);
                }
            }
            return 0;
        case WM_PAINT: {
            PAINTSTRUCT paint{};
            BeginPaint(window, &paint);
            const HRESULT result = renderer == nullptr ? E_UNEXPECTED : renderer->Render();
            EndPaint(window, &paint);
            if (FAILED(result)) {
                PostMessageW(
                    GetParent(window),
                    kCanvasRenderFailed,
                    static_cast<WPARAM>(result),
                    0);
            }
            return 0;
        }
        case WM_ERASEBKGND:
            return 1;
        case kCanvasSetSnapshotRevision:
            if (renderer != nullptr) {
                renderer->SetSnapshotRevision(
                    static_cast<std::uint64_t>(wparam));
            }
            return 0;
        case kCanvasRenderOnce:
            return renderer != nullptr && SUCCEEDED(renderer->Render()) ? 1 : 0;
        case WM_NCDESTROY:
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            delete renderer;
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            return DefWindowProcW(window, message, wparam, lparam);
    }
}

}  // namespace

bool RegisterCanvasClass(HINSTANCE instance) noexcept {
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.style = CS_HREDRAW | CS_VREDRAW;
    window_class.lpfnWndProc = CanvasWindowProcedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_CROSS);
    window_class.lpszClassName = kCanvasClassName;
    return RegisterClassExW(&window_class) != 0 || GetLastError() == ERROR_CLASS_ALREADY_EXISTS;
}

HWND CreateCanvasWindow(HINSTANCE instance, HWND parent) noexcept {
    RECT client{};
    GetClientRect(parent, &client);
    return CreateWindowExW(
        0,
        kCanvasClassName,
        L"",
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
        0,
        0,
        client.right - client.left,
        client.bottom - client.top,
        parent,
        nullptr,
        instance,
        nullptr);
}

}  // namespace inkpod::renderer

