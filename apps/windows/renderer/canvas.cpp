#include "canvas.h"

#include <d2d1_1.h>
#include <d3d11.h>
#include <dxgi1_3.h>
#include <windowsx.h>
#include <wrl/client.h>

#include <algorithm>
#include <array>
#include <condition_variable>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <future>
#include <limits>
#include <memory>
#include <mutex>
#include <new>
#include <thread>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

namespace inkpod::renderer {
namespace {

using Microsoft::WRL::ComPtr;

constexpr wchar_t kCanvasClassName[] = L"InkpodCanvasWindow";
constexpr std::uint64_t kMaximumSnapshotTiles = 262144U;
constexpr std::size_t kMaximumPointerHistory = 256U;

struct CachedTile {
    std::uint64_t revision{};
    int origin_x{};
    int origin_y{};
    UINT width{};
    UINT height{};
    ComPtr<ID2D1Bitmap1> bitmap;
};

class CanvasRenderer final {
public:
    explicit CanvasRenderer(HWND window) noexcept : window_(window) {}

    ~CanvasRenderer() {
        if (snapshot_ != nullptr) {
            inkpod_snapshot_release(&snapshot_);
        }
        DiscardDeviceResources();
    }

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
            0U,
            width,
            height,
            DXGI_FORMAT_UNKNOWN,
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT);
        if (IsDeviceLost(result)) {
            return RecreateAfterDeviceLoss();
        }
        if (FAILED(result)) {
            return result;
        }
        return CreateTargetBitmap();
    }

    HRESULT Render() noexcept {
        if (frame_latency_waitable_ != nullptr) {
            const DWORD wait = WaitForSingleObjectEx(frame_latency_waitable_, 100U, FALSE);
            if (wait == WAIT_TIMEOUT) {
                return S_FALSE;
            }
            if (wait == WAIT_FAILED) {
                return HRESULT_FROM_WIN32(GetLastError());
            }
        }
        if (!d2d_context_ || !target_bitmap_) {
            const HRESULT create_result = CreateDeviceResources();
            if (FAILED(create_result)) {
                return create_result;
            }
            const HRESULT cache_result = RebuildTileCache();
            if (FAILED(cache_result)) {
                return cache_result;
            }
        }

        d2d_context_->BeginDraw();
        d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
        d2d_context_->Clear(D2D1::ColorF(0.12F, 0.13F, 0.15F, 1.0F));

        if (transform_.document_width != 0U && transform_.document_height != 0U) {
            d2d_context_->SetTransform(DocumentTransform());

            ComPtr<ID2D1SolidColorBrush> paper_brush;
            HRESULT result = d2d_context_->CreateSolidColorBrush(
                D2D1::ColorF(D2D1::ColorF::White), &paper_brush);
            if (FAILED(result)) {
                d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
                d2d_context_->EndDraw();
                return result;
            }
            d2d_context_->FillRectangle(
                D2D1::RectF(
                    0.0F,
                    0.0F,
                    static_cast<float>(transform_.document_width),
                    static_cast<float>(transform_.document_height)),
                paper_brush.Get());
            for (const auto& entry : tile_cache_) {
                const CachedTile& tile = entry.second;
                const D2D1_RECT_F destination = D2D1::RectF(
                    static_cast<float>(tile.origin_x),
                    static_cast<float>(tile.origin_y),
                    static_cast<float>(tile.origin_x) + static_cast<float>(tile.width),
                    static_cast<float>(tile.origin_y) + static_cast<float>(tile.height));
                d2d_context_->DrawBitmap(
                    tile.bitmap.Get(),
                    destination,
                    1.0F,
                    D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
            }
            d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
        }

        HRESULT result = d2d_context_->EndDraw();
        if (result == D2DERR_RECREATE_TARGET) {
            return RecreateAfterDeviceLoss();
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

    // Consumes the Rust snapshot handle even when validation or upload fails.
    HRESULT SetSnapshot(InkpodSnapshot* snapshot) noexcept {
        if (snapshot == nullptr) {
            return E_INVALIDARG;
        }
        InkpodSnapshotView view{};
        view.struct_size = sizeof(view);
        InkpodSnapshotTransform transform{};
        transform.struct_size = sizeof(transform);
        const InkpodStatus view_status = inkpod_snapshot_get_view(snapshot, &view);
        const InkpodStatus transform_status = view_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_transform(snapshot, &transform)
            : view_status;
        if (view_status != INKPOD_STATUS_OK || transform_status != INKPOD_STATUS_OK) {
            inkpod_snapshot_release(&snapshot);
            return E_INVALIDARG;
        }
        if (snapshot_ != nullptr) {
            inkpod_snapshot_release(&snapshot_);
        }
        snapshot_ = snapshot;
        snapshot_view_ = view;
        transform_ = transform;
        return RebuildTileCache();
    }

    HRESULT DpiChanged() noexcept {
        if (!swap_chain_ || !d2d_context_) {
            return CreateDeviceResources();
        }
        d2d_context_->SetTarget(nullptr);
        target_bitmap_.Reset();
        return CreateTargetBitmap();
    }

    HRESULT SimulateDeviceLossForSmokeTest() noexcept {
        DiscardDeviceResources();
        return RecreateAfterDeviceLoss();
    }

    CanvasDocumentBounds DocumentBounds() const noexcept {
        const double left = transform_.pan_x;
        const double top = transform_.pan_y;
        return CanvasDocumentBounds{
            left,
            top,
            left + static_cast<double>(transform_.document_width) * transform_.zoom,
            top + static_cast<double>(transform_.document_height) * transform_.zoom};
    }

private:
    D2D1_MATRIX_3X2_F DocumentTransform() const noexcept {
        const float scale = static_cast<float>(transform_.zoom);
        return D2D1::Matrix3x2F(
            scale,
            0.0F,
            0.0F,
            scale,
            static_cast<float>(transform_.pan_x),
            static_cast<float>(transform_.pan_y));
    }

    static bool IsDeviceLost(HRESULT result) noexcept {
        return result == DXGI_ERROR_DEVICE_REMOVED || result == DXGI_ERROR_DEVICE_RESET;
    }

    HRESULT RebuildTileCache() noexcept {
        if (!d2d_context_) {
            return E_UNEXPECTED;
        }
        if (snapshot_ == nullptr) {
            tile_cache_.clear();
            return S_OK;
        }
        if (snapshot_view_.tile_count > kMaximumSnapshotTiles
            || snapshot_view_.tile_stride_bytes < sizeof(InkpodSnapshotTile)
            || (snapshot_view_.tile_count != 0U && snapshot_view_.tiles == nullptr)
            || snapshot_view_.tile_stride_bytes
                > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())) {
            return E_INVALIDARG;
        }
        try {
            std::unordered_set<std::uint64_t> present;
            present.reserve(static_cast<std::size_t>(snapshot_view_.tile_count));
            const auto* base = reinterpret_cast<const std::uint8_t*>(snapshot_view_.tiles);
            const std::size_t stride = static_cast<std::size_t>(
                snapshot_view_.tile_stride_bytes);
            for (std::uint64_t index = 0; index < snapshot_view_.tile_count; ++index) {
                if (index > static_cast<std::uint64_t>(
                                std::numeric_limits<std::size_t>::max() / stride)) {
                    return E_INVALIDARG;
                }
                const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                    base + static_cast<std::size_t>(index) * stride);
                if (tile->struct_size < sizeof(InkpodSnapshotTile)
                    || tile->pixel_format != INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8
                    || tile->reserved != 0U || tile->width == 0U || tile->height == 0U
                    || tile->pixels == nullptr
                    || tile->stride_bytes < tile->width * 4U
                    || tile->pixel_bytes
                        < static_cast<std::uint64_t>(tile->stride_bytes) * tile->height) {
                    return E_INVALIDARG;
                }
                present.insert(tile->tile_id);
                auto existing = tile_cache_.find(tile->tile_id);
                if (existing != tile_cache_.end()
                    && existing->second.revision == tile->tile_revision
                    && existing->second.width == tile->width
                    && existing->second.height == tile->height) {
                    existing->second.origin_x = tile->origin_x;
                    existing->second.origin_y = tile->origin_y;
                    continue;
                }

                const D2D1_BITMAP_PROPERTIES1 properties = D2D1::BitmapProperties1(
                    D2D1_BITMAP_OPTIONS_NONE,
                    D2D1::PixelFormat(
                        DXGI_FORMAT_B8G8R8A8_UNORM,
                        D2D1_ALPHA_MODE_PREMULTIPLIED),
                    96.0F,
                    96.0F);
                ComPtr<ID2D1Bitmap1> bitmap;
                const HRESULT create_result = d2d_context_->CreateBitmap(
                    D2D1::SizeU(tile->width, tile->height),
                    tile->pixels,
                    tile->stride_bytes,
                    properties,
                    &bitmap);
                if (FAILED(create_result)) {
                    return create_result;
                }
                CachedTile cache_entry{};
                cache_entry.revision = tile->tile_revision;
                cache_entry.origin_x = tile->origin_x;
                cache_entry.origin_y = tile->origin_y;
                cache_entry.width = tile->width;
                cache_entry.height = tile->height;
                cache_entry.bitmap = std::move(bitmap);
                tile_cache_[tile->tile_id] = std::move(cache_entry);
            }
            for (auto iterator = tile_cache_.begin(); iterator != tile_cache_.end();) {
                if (!present.contains(iterator->first)) {
                    iterator = tile_cache_.erase(iterator);
                } else {
                    ++iterator;
                }
            }
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
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
        swap_chain_description.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
        swap_chain_description.AlphaMode = DXGI_ALPHA_MODE_IGNORE;
        swap_chain_description.Flags = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;
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
        ComPtr<IDXGISwapChain2> swap_chain2;
        result = swap_chain_.As(&swap_chain2);
        if (FAILED(result)) {
            return result;
        }
        result = swap_chain2->SetMaximumFrameLatency(1U);
        if (FAILED(result)) {
            return result;
        }
        frame_latency_waitable_ = swap_chain2->GetFrameLatencyWaitableObject();
        if (frame_latency_waitable_ == nullptr) {
            return HRESULT_FROM_WIN32(GetLastError());
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
        d2d_context_->SetUnitMode(D2D1_UNIT_MODE_PIXELS);
        d2d_context_->SetDpi(96.0F, 96.0F);
        return CreateTargetBitmap();
    }

    HRESULT CreateTargetBitmap() noexcept {
        ComPtr<IDXGISurface> surface;
        HRESULT result = swap_chain_->GetBuffer(0U, IID_PPV_ARGS(&surface));
        if (FAILED(result)) {
            return result;
        }

        const D2D1_BITMAP_PROPERTIES1 properties = D2D1::BitmapProperties1(
            D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            D2D1::PixelFormat(
                DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_IGNORE),
            96.0F,
            96.0F);
        result = d2d_context_->CreateBitmapFromDxgiSurface(
            surface.Get(), &properties, &target_bitmap_);
        if (SUCCEEDED(result)) {
            d2d_context_->SetTarget(target_bitmap_.Get());
        }
        return result;
    }

    HRESULT RecreateAfterDeviceLoss() noexcept {
        HRESULT result = CreateDeviceResources();
        if (SUCCEEDED(result)) {
            result = RebuildTileCache();
        }
        if (SUCCEEDED(result)) {
            InvalidateRect(window_, nullptr, FALSE);
        }
        return result;
    }

    void DiscardDeviceResources() noexcept {
        if (frame_latency_waitable_ != nullptr) {
            CloseHandle(frame_latency_waitable_);
            frame_latency_waitable_ = nullptr;
        }
        tile_cache_.clear();
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
    InkpodSnapshot* snapshot_{};
    InkpodSnapshotView snapshot_view_{};
    InkpodSnapshotTransform transform_{};
    std::unordered_map<std::uint64_t, CachedTile> tile_cache_;
    ComPtr<ID3D11Device> d3d_device_;
    ComPtr<ID3D11DeviceContext> d3d_context_;
    ComPtr<IDXGISwapChain1> swap_chain_;
    ComPtr<ID2D1Factory1> d2d_factory_;
    ComPtr<ID2D1Device> d2d_device_;
    ComPtr<ID2D1DeviceContext> d2d_context_;
    ComPtr<ID2D1Bitmap1> target_bitmap_;
    HANDLE frame_latency_waitable_{};
};

enum class RenderControlKind {
    Render,
    DpiChanged,
    SimulateDeviceLoss,
    GetDocumentBounds,
};

struct RenderControl {
    RenderControlKind kind{};
    std::shared_ptr<std::promise<HRESULT>> completion;
    CanvasDocumentBounds* out_bounds{};
};

class RenderThread final {
public:
    explicit RenderThread(HWND window) noexcept : window_(window) {}

    ~RenderThread() {
        Stop();
    }

    RenderThread(const RenderThread&) = delete;
    RenderThread& operator=(const RenderThread&) = delete;

    HRESULT Start() noexcept {
        try {
            auto ready = std::make_shared<std::promise<HRESULT>>();
            auto future = ready->get_future();
            worker_ = std::thread([this, ready] { Run(ready); });
            return future.get();
        } catch (const std::system_error&) {
            return E_FAIL;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    void Stop() noexcept {
        {
            std::lock_guard lock(mutex_);
            stopping_ = true;
        }
        wake_.notify_one();
        if (worker_.joinable()) {
            worker_.join();
        }
        if (pending_snapshot_ != nullptr) {
            inkpod_snapshot_release(&pending_snapshot_);
        }
    }

    bool SetSnapshot(InkpodSnapshot* snapshot) noexcept {
        if (snapshot == nullptr) {
            return false;
        }
        InkpodSnapshot* replaced{};
        bool accepted{};
        {
            std::lock_guard lock(mutex_);
            if (stopping_) {
                replaced = snapshot;
            } else {
                replaced = std::exchange(pending_snapshot_, snapshot);
                accepted = true;
            }
        }
        if (replaced != nullptr) {
            inkpod_snapshot_release(&replaced);
        }
        if (!accepted) {
            return false;
        }
        wake_.notify_one();
        return true;
    }

    void Resize(UINT width, UINT height) noexcept {
        {
            std::lock_guard lock(mutex_);
            pending_width_ = width;
            pending_height_ = height;
            resize_pending_ = true;
        }
        wake_.notify_one();
    }

    void RequestRender() noexcept {
        {
            std::lock_guard lock(mutex_);
            render_pending_ = true;
        }
        wake_.notify_one();
    }

    HRESULT Invoke(
        RenderControlKind kind,
        CanvasDocumentBounds* out_bounds = nullptr) noexcept {
        try {
            auto completion = std::make_shared<std::promise<HRESULT>>();
            auto future = completion->get_future();
            {
                std::lock_guard lock(mutex_);
                if (stopping_) {
                    return E_UNEXPECTED;
                }
                controls_.push_back(RenderControl{kind, completion, out_bounds});
            }
            wake_.notify_one();
            return future.get();
        } catch (const std::future_error&) {
            return E_FAIL;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    DWORD ThreadId() const noexcept {
        return thread_id_;
    }

    std::uint64_t PresentedFrameCount() const noexcept {
        std::lock_guard lock(mutex_);
        return presented_frames_;
    }

private:
    void ReportFailure(HRESULT result) const noexcept {
        PostMessageW(
            GetParent(window_),
            kCanvasRenderFailed,
            static_cast<WPARAM>(result),
            0);
    }

    HRESULT RenderAndCount(CanvasRenderer& renderer) noexcept {
        const HRESULT result = renderer.Render();
        if (result == S_OK) {
            std::lock_guard lock(mutex_);
            ++presented_frames_;
        }
        return result;
    }

    void Run(const std::shared_ptr<std::promise<HRESULT>>& ready) noexcept {
        thread_id_ = GetCurrentThreadId();
        CanvasRenderer renderer(window_);
        const HRESULT initialize = renderer.Initialize();
        ready->set_value(initialize);
        if (FAILED(initialize)) {
            return;
        }

        for (;;) {
            InkpodSnapshot* snapshot{};
            UINT width{};
            UINT height{};
            bool resize{};
            bool render{};
            std::deque<RenderControl> controls;
            {
                std::unique_lock lock(mutex_);
                wake_.wait(lock, [this] {
                    return stopping_ || pending_snapshot_ != nullptr || resize_pending_
                        || render_pending_ || !controls_.empty();
                });
                if (stopping_) {
                    snapshot = std::exchange(pending_snapshot_, nullptr);
                    controls.swap(controls_);
                    lock.unlock();
                    if (snapshot != nullptr) {
                        inkpod_snapshot_release(&snapshot);
                    }
                    for (auto& control : controls) {
                        control.completion->set_value(E_ABORT);
                    }
                    break;
                }
                snapshot = std::exchange(pending_snapshot_, nullptr);
                resize = std::exchange(resize_pending_, false);
                width = pending_width_;
                height = pending_height_;
                render = std::exchange(render_pending_, false);
                controls.swap(controls_);
            }

            HRESULT result = S_OK;
            if (snapshot != nullptr) {
                result = renderer.SetSnapshot(snapshot);
                render = SUCCEEDED(result);
            }
            if (SUCCEEDED(result) && resize) {
                result = renderer.Resize(width, height);
                render = SUCCEEDED(result);
            }
            for (auto& control : controls) {
                HRESULT control_result = result;
                if (SUCCEEDED(control_result)) {
                    switch (control.kind) {
                        case RenderControlKind::Render:
                            control_result = RenderAndCount(renderer);
                            render = false;
                            break;
                        case RenderControlKind::DpiChanged:
                            control_result = renderer.DpiChanged();
                            render = SUCCEEDED(control_result);
                            break;
                        case RenderControlKind::SimulateDeviceLoss:
                            control_result = renderer.SimulateDeviceLossForSmokeTest();
                            render = SUCCEEDED(control_result);
                            break;
                        case RenderControlKind::GetDocumentBounds:
                            if (control.out_bounds == nullptr) {
                                control_result = E_POINTER;
                            } else {
                                *control.out_bounds = renderer.DocumentBounds();
                            }
                            break;
                    }
                }
                control.completion->set_value(control_result);
                if (FAILED(control_result)) {
                    result = control_result;
                }
            }
            if (SUCCEEDED(result) && render) {
                result = RenderAndCount(renderer);
            }
            if (FAILED(result)) {
                ReportFailure(result);
            }
        }
    }

    HWND window_{};
    mutable std::mutex mutex_;
    std::condition_variable wake_;
    std::thread worker_;
    bool stopping_{};
    InkpodSnapshot* pending_snapshot_{};
    UINT pending_width_{};
    UINT pending_height_{};
    bool resize_pending_{};
    bool render_pending_{};
    std::deque<RenderControl> controls_;
    DWORD thread_id_{};
    std::uint64_t presented_frames_{};
};

class CanvasHost final : public CanvasSnapshotSink {
public:
    explicit CanvasHost(HWND window) noexcept : window_(window), renderer_(window) {}

    HRESULT Initialize() noexcept {
        return renderer_.Start();
    }

    bool Submit(InkpodSnapshot* snapshot) noexcept override {
        return renderer_.SetSnapshot(snapshot);
    }

    bool SendStroke(
        CanvasStrokeEventKind kind,
        const InkpodStrokeSample* samples,
        std::uint64_t sample_count) noexcept {
        const CanvasStrokeEvent event{
            kind,
            sample_count == 0U ? nullptr : samples,
            sample_count};
        return SendMessageW(
                   GetParent(window_),
                   kCanvasStrokeReady,
                   0,
                   reinterpret_cast<LPARAM>(&event))
            == 1;
    }

    bool SendStroke(
        CanvasStrokeEventKind kind,
        const std::vector<InkpodStrokeSample>& samples) noexcept {
        return SendStroke(
            kind,
            samples.empty() ? nullptr : samples.data(),
            static_cast<std::uint64_t>(samples.size()));
    }

    bool SendPoint(
        CanvasStrokeEventKind kind,
        float x,
        float y,
        float pressure) noexcept {
        if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(pressure)) {
            return false;
        }
        const InkpodStrokeSample sample{
            sizeof(InkpodStrokeSample),
            0U,
            x,
            y,
            std::clamp(pressure, 0.0F, 1.0F),
            0U};
        return SendStroke(kind, &sample, 1U);
    }

    bool BeginMouse(float x, float y) noexcept {
        if (stroke_active_) {
            CancelStroke();
        }
        stroke_active_ = SendPoint(CanvasStrokeEventKind::Begin, x, y, 1.0F);
        pointer_stroke_ = false;
        return stroke_active_;
    }

    bool AppendMouse(float x, float y) noexcept {
        if (!stroke_active_ || pointer_stroke_) {
            return false;
        }
        if (SendPoint(CanvasStrokeEventKind::Append, x, y, 1.0F)) {
            return true;
        }
        CancelStroke();
        return false;
    }

    bool EndMouse(float x, float y) noexcept {
        if (!stroke_active_ || pointer_stroke_) {
            return false;
        }
        if (SendPoint(CanvasStrokeEventKind::End, x, y, 1.0F)) {
            stroke_active_ = false;
            return true;
        }
        CancelStroke();
        return false;
    }

    bool BeginPointer(UINT32 pointer_id, const std::vector<InkpodStrokeSample>& samples) noexcept {
        if (stroke_active_) {
            CancelStroke();
        }
        stroke_active_ = SendStroke(CanvasStrokeEventKind::Begin, samples);
        pointer_stroke_ = stroke_active_;
        active_pointer_id_ = stroke_active_ ? pointer_id : 0U;
        return stroke_active_;
    }

    bool AppendPointer(UINT32 pointer_id, const std::vector<InkpodStrokeSample>& samples) noexcept {
        if (!stroke_active_ || !pointer_stroke_ || active_pointer_id_ != pointer_id) {
            return false;
        }
        if (SendStroke(CanvasStrokeEventKind::Append, samples)) {
            return true;
        }
        CancelStroke();
        return false;
    }

    bool EndPointer(UINT32 pointer_id, const std::vector<InkpodStrokeSample>& samples) noexcept {
        if (!stroke_active_ || !pointer_stroke_ || active_pointer_id_ != pointer_id) {
            return false;
        }
        if (SendStroke(CanvasStrokeEventKind::End, samples)) {
            stroke_active_ = false;
            pointer_stroke_ = false;
            active_pointer_id_ = 0U;
            return true;
        }
        CancelStroke();
        return false;
    }

    void CancelStroke() noexcept {
        if (stroke_active_) {
            SendStroke(CanvasStrokeEventKind::Cancel, nullptr, 0U);
        }
        stroke_active_ = false;
        pointer_stroke_ = false;
        active_pointer_id_ = 0U;
    }

    RenderThread& Renderer() noexcept {
        return renderer_;
    }

    bool PointerStrokeActive() const noexcept {
        return stroke_active_ && pointer_stroke_;
    }

    POINT last_pan_point{};
    bool panning{};

private:
    HWND window_{};
    RenderThread renderer_;
    bool stroke_active_{};
    bool pointer_stroke_{};
    UINT32 active_pointer_id_{};
};

void ReportRenderFailure(HWND window, HRESULT result) noexcept {
    PostMessageW(
        GetParent(window),
        kCanvasRenderFailed,
        static_cast<WPARAM>(result),
        0);
}

bool PenSamples(
    HWND window,
    WPARAM wparam,
    std::vector<InkpodStrokeSample>& samples) noexcept {
    const UINT pointer_id = GET_POINTERID_WPARAM(wparam);
    POINTER_INPUT_TYPE input_type{};
    if (!GetPointerType(pointer_id, &input_type) || input_type != PT_PEN) {
        return false;
    }
    std::array<POINTER_PEN_INFO, kMaximumPointerHistory> history{};
    UINT32 count = static_cast<UINT32>(history.size());
    if (!GetPointerPenInfoHistory(pointer_id, &count, history.data()) || count == 0U) {
        count = 1U;
        if (!GetPointerPenInfo(pointer_id, history.data())) {
            return false;
        }
    }
    try {
        samples.clear();
        samples.reserve(count);
        // Win32 returns pointer history newest-first; Core requires chronological order.
        for (UINT32 index = count; index > 0U; --index) {
            const POINTER_PEN_INFO& pen = history[index - 1U];
            POINT point = pen.pointerInfo.ptPixelLocation;
            if (ScreenToClient(window, &point) == FALSE) {
                return false;
            }
            const float pressure = (pen.penMask & PEN_MASK_PRESSURE) != 0U
                    && (pen.penFlags & PEN_FLAG_ERASER) == 0U
                ? std::clamp(static_cast<float>(pen.pressure) / 1024.0F, 0.0F, 1.0F)
                : 1.0F;
            if (!samples.empty()) {
                const InkpodStrokeSample& last = samples.back();
                if (last.x == static_cast<float>(point.x)
                    && last.y == static_cast<float>(point.y) && last.pressure == pressure) {
                    continue;
                }
            }
            samples.push_back(InkpodStrokeSample{
                sizeof(InkpodStrokeSample),
                0U,
                static_cast<float>(point.x),
                static_cast<float>(point.y),
                pressure,
                0U});
        }
        return !samples.empty();
    } catch (const std::bad_alloc&) {
        samples.clear();
        return false;
    }
}

LRESULT CALLBACK CanvasWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    switch (message) {
        case WM_CREATE: {
            auto* created = new (std::nothrow) CanvasHost(window);
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
            if (host != nullptr && wparam != SIZE_MINIMIZED) {
                host->CancelStroke();
                const UINT width = LOWORD(lparam);
                const UINT height = HIWORD(lparam);
                host->Renderer().Resize(width, height);
                PostMessageW(
                    GetParent(window),
                    kCanvasViewportChanged,
                    static_cast<WPARAM>(width),
                    static_cast<LPARAM>(height));
            }
            return 0;
        case WM_PAINT: {
            PAINTSTRUCT paint{};
            BeginPaint(window, &paint);
            EndPaint(window, &paint);
            if (host != nullptr) {
                host->Renderer().RequestRender();
            }
            return 0;
        }
        case WM_ERASEBKGND:
            return 1;
        case WM_DPICHANGED_AFTERPARENT: {
            const HRESULT result = host == nullptr
                ? E_UNEXPECTED
                : host->Renderer().Invoke(RenderControlKind::DpiChanged);
            if (FAILED(result)) {
                ReportRenderFailure(window, result);
            }
            return SUCCEEDED(result) ? 1 : 0;
        }
        case WM_LBUTTONDOWN:
            if (host != nullptr && !host->PointerStrokeActive()) {
                SetCapture(window);
                return host->BeginMouse(
                           static_cast<float>(GET_X_LPARAM(lparam)),
                           static_cast<float>(GET_Y_LPARAM(lparam)))
                    ? 1
                    : 0;
            }
            return 0;
        case WM_MOUSEMOVE:
            if (host != nullptr && (wparam & MK_LBUTTON) != 0U
                && !host->PointerStrokeActive()) {
                return host->AppendMouse(
                           static_cast<float>(GET_X_LPARAM(lparam)),
                           static_cast<float>(GET_Y_LPARAM(lparam)))
                    ? 1
                    : 0;
            }
            if (host != nullptr && host->panning && (wparam & MK_MBUTTON) != 0U) {
                const POINT current{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                const CanvasViewGesture gesture{
                    INKPOD_VIEW_PAN_BY,
                    static_cast<double>(current.x - host->last_pan_point.x),
                    static_cast<double>(current.y - host->last_pan_point.y),
                    0.0};
                host->last_pan_point = current;
                return SendMessageW(
                    GetParent(window),
                    kCanvasViewGesture,
                    0,
                    reinterpret_cast<LPARAM>(&gesture));
            }
            return 0;
        case WM_LBUTTONUP:
            if (host != nullptr && !host->PointerStrokeActive()) {
                const bool completed = host->EndMouse(
                    static_cast<float>(GET_X_LPARAM(lparam)),
                    static_cast<float>(GET_Y_LPARAM(lparam)));
                ReleaseCapture();
                return completed ? 1 : 0;
            }
            return 0;
        case WM_MBUTTONDOWN:
            if (host != nullptr) {
                host->panning = true;
                host->last_pan_point = POINT{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                SetCapture(window);
                return 1;
            }
            return 0;
        case WM_MBUTTONUP:
            if (host != nullptr) {
                host->panning = false;
            }
            ReleaseCapture();
            return 1;
        case WM_MOUSEWHEEL: {
            POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
            ScreenToClient(window, &point);
            const double factor = GET_WHEEL_DELTA_WPARAM(wparam) > 0 ? 1.2 : 1.0 / 1.2;
            const CanvasViewGesture gesture{
                INKPOD_VIEW_ZOOM_AT,
                factor,
                static_cast<double>(point.x),
                static_cast<double>(point.y)};
            return SendMessageW(
                GetParent(window),
                kCanvasViewGesture,
                0,
                reinterpret_cast<LPARAM>(&gesture));
        }
        case WM_POINTERDOWN:
        case WM_POINTERUPDATE:
        case WM_POINTERUP: {
            if (host == nullptr) {
                return 0;
            }
            std::vector<InkpodStrokeSample> samples;
            if (!PenSamples(window, wparam, samples)) {
                break;
            }
            const UINT32 pointer_id = GET_POINTERID_WPARAM(wparam);
            if (message == WM_POINTERDOWN) {
                SetCapture(window);
                return host->BeginPointer(pointer_id, samples) ? 1 : 0;
            }
            if (message == WM_POINTERUPDATE) {
                return host->AppendPointer(pointer_id, samples) ? 1 : 0;
            }
            const bool completed = host->EndPointer(pointer_id, samples);
            ReleaseCapture();
            return completed ? 1 : 0;
        }
        case WM_CAPTURECHANGED:
            if (host != nullptr) {
                host->CancelStroke();
                host->panning = false;
            }
            return 0;
        case kCanvasRenderOnce:
            return host != nullptr
                    && SUCCEEDED(host->Renderer().Invoke(RenderControlKind::Render))
                ? 1
                : 0;
        case kCanvasSimulateDeviceLoss:
            return host != nullptr
                    && SUCCEEDED(
                        host->Renderer().Invoke(RenderControlKind::SimulateDeviceLoss))
                ? 1
                : 0;
        case kCanvasGetRendererThreadId:
            return host == nullptr ? 0 : static_cast<LRESULT>(host->Renderer().ThreadId());
        case kCanvasGetPresentedFrameCount:
            return host == nullptr
                ? 0
                : static_cast<LRESULT>(host->Renderer().PresentedFrameCount());
        case kCanvasGetDocumentBounds: {
            auto* bounds = reinterpret_cast<CanvasDocumentBounds*>(lparam);
            return host != nullptr && bounds != nullptr
                    && SUCCEEDED(host->Renderer().Invoke(
                        RenderControlKind::GetDocumentBounds, bounds))
                ? 1
                : 0;
        }
        case WM_NCDESTROY:
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            delete host;
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
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

CanvasSnapshotSink* GetCanvasSnapshotSink(HWND canvas) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host;
}

}  // namespace inkpod::renderer
