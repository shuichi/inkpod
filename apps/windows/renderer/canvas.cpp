#include "canvas.h"
#include "canvas_scroll_model.h"

#include <d2d1_1.h>
#include <d3d11.h>
#include <dwrite.h>
#include <dxgi1_3.h>
#include <windowsx.h>
#include <wrl/client.h>

#include <algorithm>
#include <atomic>
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
#include <optional>
#include <string>
#include <thread>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <variant>
#include <vector>

namespace inkpod::renderer {
namespace {

using Microsoft::WRL::ComPtr;

constexpr wchar_t kCanvasClassName[] = L"InkpodCanvasWindow";
constexpr std::uint64_t kMaximumSnapshotTiles = 262144U;
constexpr std::uint64_t kMaximumSnapshotGuides = 4096U;
constexpr std::uint64_t kMaximumRenderPasses = 1048576U;
constexpr std::uint64_t kMaximumOverlayLines = 8192U;
constexpr std::size_t kMaximumPointerHistory = 256U;
constexpr std::size_t kMaximumPendingCanvasInput = 64U;
constexpr std::uint64_t kMaximumStrokeSamples = UINT64_C(1048576);
constexpr UINT kCanvasScrollProjectionChanged = WM_APP + 0x12CU;
constexpr int kCanvasScrollLineReferencePixels = 32;
constexpr UINT_PTR kCanvasBindingPresentRetryTimer = 0x4A11U;
constexpr UINT_PTR kCanvasScrollProjectionRetryTimer = 0x4A12U;
constexpr std::uint8_t kMaximumScrollProjectionApplyAttempts = 3U;
constexpr std::uint8_t kMaximumScrollRefreshDeliveryAttempts = 1U;
constexpr DWORD kFrameLatencyRetryMilliseconds = 4U;
constexpr ULONGLONG kTransientOcclusionRetryMilliseconds = 250U;
constexpr std::uint64_t kApplicationGpuTileBudgetBytes = UINT64_C(1024) * 1024U * 1024U;
constexpr std::size_t kMaximumSequenceCacheSources = 64U;
constexpr std::uint64_t kSequenceGpuCacheBudgetBytes = UINT64_C(1024) * 1024U * 1024U;
std::atomic<std::uint64_t> gApplicationTileUseSequence{};

enum class CanvasScrollAxis : std::uint8_t {
    Horizontal,
    Vertical,
};

struct CachedTile {
    std::uint64_t revision{};
    std::uint64_t last_used{};
    std::uint64_t byte_count{};
    int origin_x{};
    int origin_y{};
    UINT width{};
    UINT height{};
    bool active{};
    ComPtr<ID2D1Bitmap1> bitmap;
};

using TileCache = std::unordered_map<std::uint64_t, CachedTile>;

// A source key comes from the immutable Rust snapshot, never the UI selection.
// The owner generation separates catalog/Core replacement from warm navigation.
struct SequenceSourceKey {
    std::uint64_t document_uuid_high{};
    std::uint64_t document_uuid_low{};
    std::uint64_t source_generation{};
    std::uint64_t owner_generation{};

    [[nodiscard]] explicit operator bool() const noexcept {
        return owner_generation != 0U;
    }

    bool operator==(const SequenceSourceKey&) const noexcept = default;
};

struct CachedSequenceSource {
    SequenceSourceKey key;
    TileCache tiles;
    std::uint64_t bytes{};
    std::uint64_t last_used{};
};

std::uint64_t SaturatingAdd(std::uint64_t left, std::uint64_t right) noexcept {
    return left > UINT64_MAX - right ? UINT64_MAX : left + right;
}

std::uint64_t SaturatingProduct(std::uint64_t left, std::uint64_t right) noexcept {
    return left != 0U && right > UINT64_MAX / left ? UINT64_MAX : left * right;
}

std::uint64_t PerformanceCounterTicks() noexcept {
    LARGE_INTEGER counter{};
    return QueryPerformanceCounter(&counter) != FALSE && counter.QuadPart > 0
        ? static_cast<std::uint64_t>(counter.QuadPart) : 0U;
}

bool CanvasAncestorsVisible(HWND window) noexcept {
    const HWND parent = GetParent(window);
    const HWND root = GetAncestor(window, GA_ROOT);
    return (parent == nullptr || IsWindowVisible(parent) != FALSE)
        && (root == nullptr || IsIconic(root) == FALSE);
}

std::uint64_t EstimateSnapshotPayloadBytes(InkpodSnapshot* snapshot) noexcept {
    if (snapshot == nullptr) {
        return 0U;
    }
    InkpodSnapshotView view{};
    view.struct_size = sizeof(view);
    InkpodSnapshotOverlay overlay{};
    overlay.struct_size = sizeof(overlay);
    InkpodSnapshotShootingFrameView shooting_frames{};
    shooting_frames.struct_size = sizeof(shooting_frames);
    InkpodSnapshotRenderPlan plan{};
    plan.struct_size = sizeof(plan);
    if (inkpod_snapshot_get_view(snapshot, &view) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_overlay(snapshot, &overlay) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_shooting_frames(snapshot, &shooting_frames) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_render_plan(snapshot, &plan) != INKPOD_STATUS_OK) {
        return 0U;
    }
    std::uint64_t bytes = sizeof(InkpodSnapshotView) + sizeof(InkpodSnapshotTransform)
        + sizeof(InkpodSnapshotOverlay)
        + sizeof(InkpodSnapshotShootingFrameView)
        + sizeof(InkpodSnapshotRenderPlan);
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        view.tile_count, view.tile_stride_bytes));
    if (view.tiles != nullptr && view.tile_stride_bytes >= sizeof(InkpodSnapshotTile)
        && view.tile_stride_bytes <= static_cast<std::uint64_t>(SIZE_MAX)) {
        const auto* base = reinterpret_cast<const std::uint8_t*>(view.tiles);
        const std::size_t stride = static_cast<std::size_t>(view.tile_stride_bytes);
        for (std::uint64_t index = 0U; index < view.tile_count; ++index) {
            if (index > static_cast<std::uint64_t>(SIZE_MAX / stride)) {
                return UINT64_MAX;
            }
            const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                base + static_cast<std::size_t>(index) * stride);
            bytes = SaturatingAdd(bytes, tile->pixel_bytes);
        }
    }
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        overlay.guide_count, overlay.guide_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        shooting_frames.frame_count, shooting_frames.frame_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        plan.pass_count, plan.pass_stride_bytes));
    std::uint64_t prepared_source_count{};
    if (inkpod_snapshot_sequence_prepared_source_count(
            snapshot, &prepared_source_count) != INKPOD_STATUS_OK) {
        return 0U;
    }
    for (std::uint64_t source_index = 0U;
         source_index < prepared_source_count; ++source_index) {
        InkpodSnapshotSequenceSourceView source{};
        source.struct_size = sizeof(source);
        if (inkpod_snapshot_sequence_prepared_source_get(
                snapshot, source_index, &source) != INKPOD_STATUS_OK) {
            return 0U;
        }
        bytes = SaturatingAdd(bytes, sizeof(source));
        bytes = SaturatingAdd(bytes, SaturatingProduct(
            source.tile_count, source.tile_stride_bytes));
        if (source.tiles == nullptr
            || source.tile_stride_bytes < sizeof(InkpodSnapshotTile)
            || source.tile_stride_bytes > static_cast<std::uint64_t>(SIZE_MAX)) {
            return UINT64_MAX;
        }
        const auto* source_base = reinterpret_cast<const std::uint8_t*>(source.tiles);
        const std::size_t source_stride = static_cast<std::size_t>(
            source.tile_stride_bytes);
        for (std::uint64_t tile_index = 0U;
             tile_index < source.tile_count; ++tile_index) {
            if (tile_index > static_cast<std::uint64_t>(SIZE_MAX / source_stride)) {
                return UINT64_MAX;
            }
            const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                source_base + static_cast<std::size_t>(tile_index) * source_stride);
            bytes = SaturatingAdd(bytes, tile->pixel_bytes);
        }
    }
    return bytes;
}

class SharedRendererDevice final {
public:
    HRESULT Initialize() noexcept {
        Discard();
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
        result = d3d_device_.As(&dxgi_device_);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<IDXGIAdapter> adapter;
        result = dxgi_device_->GetAdapter(&adapter);
        if (FAILED(result)) {
            return result;
        }
        result = adapter->GetParent(IID_PPV_ARGS(&dxgi_factory_));
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
        result = DWriteCreateFactory(
            DWRITE_FACTORY_TYPE_SHARED,
            __uuidof(IDWriteFactory),
            reinterpret_cast<IUnknown**>(dwrite_factory_.GetAddressOf()));
        if (FAILED(result)) {
            return result;
        }
        result = d2d_factory_->CreateDevice(dxgi_device_.Get(), &d2d_device_);
        if (SUCCEEDED(result)) {
            ++generation_;
        }
        return result;
    }

    void Discard() noexcept {
        d2d_device_.Reset();
        dwrite_factory_.Reset();
        d2d_factory_.Reset();
        dxgi_factory_.Reset();
        dxgi_device_.Reset();
        d3d_context_.Reset();
        d3d_device_.Reset();
    }

    [[nodiscard]] ID3D11Device* D3dDevice() const noexcept {
        return d3d_device_.Get();
    }

    [[nodiscard]] ID3D11DeviceContext* D3dContext() const noexcept {
        return d3d_context_.Get();
    }

    [[nodiscard]] IDXGIFactory2* DxgiFactory() const noexcept {
        return dxgi_factory_.Get();
    }

    [[nodiscard]] ID2D1Device* D2dDevice() const noexcept {
        return d2d_device_.Get();
    }

    [[nodiscard]] ID2D1Factory1* D2dFactory() const noexcept {
        return d2d_factory_.Get();
    }

    [[nodiscard]] IDWriteFactory* DwriteFactory() const noexcept {
        return dwrite_factory_.Get();
    }

    [[nodiscard]] std::uint64_t Generation() const noexcept {
        return generation_;
    }

private:
    ComPtr<ID3D11Device> d3d_device_;
    ComPtr<ID3D11DeviceContext> d3d_context_;
    ComPtr<IDXGIDevice> dxgi_device_;
    ComPtr<IDXGIFactory2> dxgi_factory_;
    ComPtr<ID2D1Factory1> d2d_factory_;
    ComPtr<ID2D1Device> d2d_device_;
    ComPtr<IDWriteFactory> dwrite_factory_;
    std::uint64_t generation_{};
};

class CanvasSurface final {
public:
    CanvasSurface(
        HWND window,
        HWND owner_window,
        SharedRendererDevice& shared)
        : window_(window), owner_window_(owner_window), shared_(shared) {}

    ~CanvasSurface() {
        if (snapshot_ != nullptr) {
            inkpod_snapshot_release(&snapshot_);
        }
        DiscardSurfaceResources();
    }

    HRESULT Initialize() noexcept {
        return CreateSurfaceResources();
    }

    HRESULT Resize(UINT width, UINT height) noexcept {
        if (width == 0U || height == 0U) {
            return S_FALSE;
        }
        if (!swap_chain_) {
            return CreateSurfaceResources();
        }
        if (target_bitmap_ && width == surface_width_ && height == surface_height_) {
            return S_FALSE;
        }

        d2d_context_->SetTarget(nullptr);
        target_bitmap_.Reset();
        HRESULT result = swap_chain_->ResizeBuffers(
            0U,
            width,
            height,
            DXGI_FORMAT_UNKNOWN,
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT);
        if (FAILED(result)) {
            return result;
        }
        result = CreateTargetBitmap();
        if (SUCCEEDED(result)) {
            surface_width_ = width;
            surface_height_ = height;
        }
        return result;
    }

    HRESULT Render(
        CanvasPixelRgba8* sampled_pixel = nullptr,
        UINT sample_x = 0U,
        UINT sample_y = 0U,
        DWORD wait_milliseconds = 100U,
        HANDLE interrupt_event = nullptr) noexcept {
        if (!d2d_context_ || !target_bitmap_) {
            const HRESULT create_result = CreateSurfaceResources();
            if (FAILED(create_result)) {
                return create_result;
            }
            const HRESULT cache_result = RebuildTileCache();
            if (FAILED(cache_result)) {
                return cache_result;
            }
        }
        if (!snapshot_ready_) {
            // A failed upload may leave reusable bitmaps, but must never
            // acknowledge a partially prepared target as successfully shown.
            return E_UNEXPECTED;
        }

        // The waitable object grants capacity for a frame, not for a draw
        // attempt. A failed draw/readback must retain that permit
        // until Present succeeds; waiting twice can consume the next signal
        // without ever having submitted the preceding frame.
        const HRESULT readiness = AcquireFrameLatencyPermit(wait_milliseconds, interrupt_event);
        if (readiness != S_OK) {
            return readiness;
        }

        const bool record_first_present = snapshot_ != nullptr
            && (!has_presented_snapshot_
                || last_presented_document_revision_ != snapshot_document_revision_
                || last_presented_presentation_epoch_ != snapshot_presentation_epoch_);
        const std::uint64_t frame_ready_qpc = record_first_present
            ? PerformanceCounterTicks() : 0U;

        d2d_context_->BeginDraw();
        d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
        d2d_context_->Clear(D2D1::ColorF(0.12F, 0.13F, 0.15F, 1.0F));

        if (transform_.document_width != 0U && transform_.document_height != 0U) {
            d2d_context_->SetTransform(DocumentTransform());

            ComPtr<ID2D1SolidColorBrush> paper_brush;
            const bool legacy_check = (snapshot_view_.feature_flags
                & INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE) != 0U;
            const bool native_check = (snapshot_view_.feature_flags
                & INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA) != 0U;
            const bool solid_white_base = (snapshot_view_.feature_flags
                & INKPOD_SNAPSHOT_FEATURE_SOLID_WHITE_BASE) != 0U;
            const bool transparent_view = (overlay_.flags
                & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) != 0U;
            const D2D1_COLOR_F paper_color = solid_white_base
                ? D2D1::ColorF(D2D1::ColorF::White)
                : (legacy_check
                ? D2D1::ColorF(D2D1::ColorF::Black)
                : (native_check ? D2D1::ColorF(D2D1::ColorF::Magenta)
                                : (transparent_view
                                          ? D2D1::ColorF(0.78F, 0.78F, 0.78F, 1.0F)
                                          : D2D1::ColorF(D2D1::ColorF::White))));
            HRESULT result = d2d_context_->CreateSolidColorBrush(
                paper_color, &paper_brush);
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
            constexpr UINT checker_size = 16U;
            const std::uint64_t checker_columns =
                (transform_.document_width + checker_size - 1U) / checker_size;
            const std::uint64_t checker_rows =
                (transform_.document_height + checker_size - 1U) / checker_size;
            if (transparent_view && !solid_white_base && checker_columns != 0U
                && checker_rows <= UINT64_C(16384) / checker_columns) {
                paper_brush->SetColor(D2D1::ColorF(0.9F, 0.9F, 0.9F, 1.0F));
                for (std::uint64_t y = 0; y < transform_.document_height; y += checker_size) {
                    for (std::uint64_t x = 0; x < transform_.document_width; x += checker_size) {
                        if (((x / checker_size) + (y / checker_size)) % 2U == 0U) {
                            continue;
                        }
                        d2d_context_->FillRectangle(
                            D2D1::RectF(
                                static_cast<float>(x),
                                static_cast<float>(y),
                                static_cast<float>(std::min<std::uint64_t>(
                                    transform_.document_width, x + checker_size)),
                                static_cast<float>(std::min<std::uint64_t>(
                                    transform_.document_height, y + checker_size))),
                            paper_brush.Get());
                    }
                }
            }
            result = DrawOrderedContent();
            if (FAILED(result)) {
                d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
                d2d_context_->EndDraw();
                return result;
            }
            result = DrawOverlays();
            if (FAILED(result)) {
                d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
                d2d_context_->EndDraw();
                return result;
            }
            result = DrawShootingFrame();
            if (FAILED(result)) {
                d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
                d2d_context_->EndDraw();
                return result;
            }
            result = DrawFloatingPreview();
            if (FAILED(result)) {
                d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
                d2d_context_->EndDraw();
                return result;
            }
            result = DrawGeometryPreview();
            if (FAILED(result)) {
                d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
                d2d_context_->EndDraw();
                return result;
            }
            d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
            result = DrawRulers();
            if (FAILED(result)) {
                d2d_context_->EndDraw();
                return result;
            }
        }

        HRESULT result = d2d_context_->EndDraw();
        if (FAILED(result)) {
            return result;
        }

        if (sampled_pixel != nullptr) {
            result = CopyBackBufferPixel(sample_x, sample_y, *sampled_pixel);
            if (FAILED(result)) {
                return result;
            }
        }

        const std::uint64_t present_begin_qpc = record_first_present
            ? PerformanceCounterTicks() : 0U;
        result = swap_chain_->Present(1U, 0U);
        const std::uint64_t present_end_qpc = record_first_present
            ? PerformanceCounterTicks() : 0U;
        if (result == S_OK) {
            frame_latency_permit_ = false;
            if (record_first_present) {
                first_frame_ready_qpc_ = frame_ready_qpc;
                first_present_begin_qpc_ = present_begin_qpc;
                first_presented_revision_qpc_ = present_end_qpc;
            }
            has_presented_snapshot_ = snapshot_ != nullptr;
            last_presented_document_revision_ = snapshot_document_revision_;
            last_presented_presentation_epoch_ = snapshot_presentation_epoch_;
            last_presented_view_revision_ = transform_.view_revision;
            last_presented_source_ = InkpodSnapshotSourceIdentity{
                sizeof(InkpodSnapshotSourceIdentity),
                active_source_ ? INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE : 0U,
                active_source_.document_uuid_high,
                active_source_.document_uuid_low,
                active_source_.source_generation,
                active_source_.owner_generation};
        }
        return result;
    }

    // Consumes the Rust snapshot handle even when validation or upload fails.
    HRESULT SetSnapshot(
        InkpodSnapshot* snapshot,
        std::uint64_t committed_document_revision,
        std::uint64_t submission_qpc,
        std::uint64_t presentation_epoch) noexcept {
        if (snapshot == nullptr) {
            return E_INVALIDARG;
        }
        InkpodSnapshotView view{};
        view.struct_size = sizeof(view);
        InkpodSnapshotTransform transform{};
        transform.struct_size = sizeof(transform);
        InkpodSnapshotOverlay overlay{};
        overlay.struct_size = sizeof(overlay);
        InkpodSnapshotShootingFrameView shooting_frames{};
        shooting_frames.struct_size = sizeof(shooting_frames);
        InkpodSnapshotRenderPlan render_plan{};
        render_plan.struct_size = sizeof(render_plan);
        InkpodSnapshotSourceIdentity source_identity{};
        source_identity.struct_size = sizeof(source_identity);
        const InkpodStatus view_status = inkpod_snapshot_get_view(snapshot, &view);
        const InkpodStatus transform_status = view_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_transform(snapshot, &transform)
            : view_status;
        const InkpodStatus overlay_status = transform_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_overlay(snapshot, &overlay)
            : transform_status;
        const InkpodStatus shooting_frame_status = overlay_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_shooting_frames(snapshot, &shooting_frames)
            : overlay_status;
        const InkpodStatus render_plan_status = shooting_frame_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_render_plan(snapshot, &render_plan)
            : shooting_frame_status;
        const InkpodStatus source_identity_status = render_plan_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_source_identity(snapshot, &source_identity)
            : render_plan_status;
        if (view_status != INKPOD_STATUS_OK || transform_status != INKPOD_STATUS_OK
            || overlay_status != INKPOD_STATUS_OK
            || shooting_frame_status != INKPOD_STATUS_OK
            || render_plan_status != INKPOD_STATUS_OK
            || source_identity_status != INKPOD_STATUS_OK
            || !ValidateSourceIdentity(source_identity)
            || !ValidateOverlay(overlay)
            || !ValidateShootingFrames(shooting_frames)
            || !ValidateRenderPlan(render_plan, view)) {
            inkpod_snapshot_release(&snapshot);
            return E_INVALIDARG;
        }
        const SequenceSourceKey next_source{
            source_identity.document_uuid_high, source_identity.document_uuid_low,
            source_identity.source_generation, source_identity.owner_generation};
        const bool same_presentation = snapshot_presentation_epoch_ == presentation_epoch;
        if (!active_source_ && !next_source && !same_presentation) {
            // A document replacement ends any ordinary-cache continuation of
            // a previously pristine source, including untouched tile IDs that
            // an unrelated Core may reuse. Normal same-document edits keep it.
            tile_cache_.clear();
            gpu_tile_bytes_ = 0U;
        }
        SelectSourceCache(next_source, same_presentation && presentation_epoch != 0U);
        if (snapshot_ != nullptr) {
            inkpod_snapshot_release(&snapshot_);
        }
        snapshot_ = snapshot;
        snapshot_document_revision_ = committed_document_revision;
        snapshot_presentation_epoch_ = presentation_epoch;
        last_snapshot_submission_qpc_ = submission_qpc;
        snapshot_ready_ = false;
        snapshot_view_ = view;
        transform_ = transform;
        overlay_ = overlay;
        shooting_frames_ = shooting_frames;
        render_plan_ = render_plan;
        retained_snapshot_bytes_ = EstimateSnapshotPayloadBytes(snapshot);
        return PrepareTileCache();
    }

    // RendererHost reserves application-wide capacity after metadata preparation
    // and before any CreateBitmap call. Matched current tiles stay protected.
    HRESULT UploadPreparedSnapshot() noexcept {
        return UploadPreparedTiles();
    }

    HRESULT UploadPreparedSequenceSources() noexcept {
        if (snapshot_ == nullptr || !d2d_context_) {
            return snapshot_ == nullptr ? S_OK : E_UNEXPECTED;
        }
        std::uint64_t source_count{};
        if (inkpod_snapshot_sequence_prepared_source_count(
                snapshot_, &source_count) != INKPOD_STATUS_OK
            || source_count > kMaximumSequenceCacheSources) {
            return E_INVALIDARG;
        }
        try {
            for (std::uint64_t source_index = 0U;
                 source_index < source_count; ++source_index) {
                InkpodSnapshotSequenceSourceView source{};
                source.struct_size = sizeof(source);
                if (inkpod_snapshot_sequence_prepared_source_get(
                        snapshot_, source_index, &source) != INKPOD_STATUS_OK) {
                    return E_INVALIDARG;
                }
                const InkpodSnapshotSourceIdentity source_identity{
                    sizeof(InkpodSnapshotSourceIdentity),
                    source.flags,
                    source.document_uuid_high,
                    source.document_uuid_low,
                    source.source_generation,
                    source.owner_generation};
                if (!ValidateSourceIdentity(source_identity)
                    || source.flags != INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE
                    || source.tile_count == 0U
                    || source.tile_count > kMaximumSnapshotTiles
                    || source.tiles == nullptr
                    || source.tile_stride_bytes < sizeof(InkpodSnapshotTile)
                    || source.tile_stride_bytes
                        > static_cast<std::uint64_t>(SIZE_MAX)) {
                    return E_INVALIDARG;
                }
                const SequenceSourceKey key{
                    source.document_uuid_high,
                    source.document_uuid_low,
                    source.source_generation,
                    source.owner_generation};
                if (key == active_source_) {
                    continue;
                }
                for (auto& retained : retained_sources_) {
                    if (retained.key
                        && retained.key.owner_generation != key.owner_generation) {
                        retained.tiles.clear();
                        retained.key = {};
                        retained.bytes = 0U;
                        retained.last_used = 0U;
                    }
                }
                const auto existing_source = std::find_if(
                    retained_sources_.begin(), retained_sources_.end(),
                    [key](const CachedSequenceSource& retained) {
                        return retained.key == key;
                    });
                if (existing_source != retained_sources_.end()) {
                    existing_source->last_used =
                        gApplicationTileUseSequence.fetch_add(
                            1U, std::memory_order_relaxed) + 1U;
                    continue;
                }

                const auto* base = reinterpret_cast<const std::uint8_t*>(source.tiles);
                const std::size_t stride = static_cast<std::size_t>(
                    source.tile_stride_bytes);
                std::uint64_t source_bytes{};
                for (std::uint64_t tile_index = 0U;
                     tile_index < source.tile_count; ++tile_index) {
                    if (tile_index > static_cast<std::uint64_t>(SIZE_MAX / stride)) {
                        return E_INVALIDARG;
                    }
                    const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                        base + static_cast<std::size_t>(tile_index) * stride);
                    if (tile->struct_size < sizeof(InkpodSnapshotTile)
                        || tile->pixel_format
                            != INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8
                        || tile->reserved != 0U || tile->width == 0U
                        || tile->height == 0U || tile->pixels == nullptr
                        || tile->stride_bytes < tile->width * 4U
                        || tile->pixel_bytes
                            < static_cast<std::uint64_t>(tile->stride_bytes)
                                * tile->height
                        || tile->pixel_bytes > kSequenceGpuCacheBudgetBytes
                        || source_bytes
                            > kSequenceGpuCacheBudgetBytes - tile->pixel_bytes) {
                        return E_INVALIDARG;
                    }
                    source_bytes += tile->pixel_bytes;
                }
                if (source_bytes > tile_budget_bytes_) {
                    continue;
                }
                while (GpuTileBytes() > tile_budget_bytes_ - source_bytes) {
                    if (!EvictOldestInactive()) {
                        break;
                    }
                }
                if (GpuTileBytes() > tile_budget_bytes_ - source_bytes) {
                    continue;
                }

                TileCache prepared;
                prepared.reserve(static_cast<std::size_t>(source.tile_count));
                std::uint64_t prepared_bytes{};
                for (std::uint64_t tile_index = 0U;
                     tile_index < source.tile_count; ++tile_index) {
                    const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                        base + static_cast<std::size_t>(tile_index) * stride);
                    if (prepared.find(tile->tile_id) != prepared.end()) {
                        return E_INVALIDARG;
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
                    cache_entry.last_used =
                        gApplicationTileUseSequence.fetch_add(
                            1U, std::memory_order_relaxed) + 1U;
                    cache_entry.byte_count = tile->pixel_bytes;
                    cache_entry.origin_x = tile->origin_x;
                    cache_entry.origin_y = tile->origin_y;
                    cache_entry.width = tile->width;
                    cache_entry.height = tile->height;
                    cache_entry.active = false;
                    cache_entry.bitmap = std::move(bitmap);
                    prepared.emplace(tile->tile_id, std::move(cache_entry));
                    prepared_bytes += tile->pixel_bytes;
                    uploaded_tile_count_ = SaturatingAdd(uploaded_tile_count_, 1U);
                    uploaded_tile_bytes_ = SaturatingAdd(
                        uploaded_tile_bytes_, tile->pixel_bytes);
                }
                auto slot = std::find_if(
                    retained_sources_.begin(), retained_sources_.end(),
                    [](const CachedSequenceSource& retained) {
                        return !retained.key;
                    });
                if (slot == retained_sources_.end()) {
                    (void)EvictOldestRetainedSource();
                    slot = std::find_if(
                        retained_sources_.begin(), retained_sources_.end(),
                        [](const CachedSequenceSource& retained) {
                            return !retained.key;
                        });
                }
                if (slot == retained_sources_.end()) {
                    continue;
                }
                slot->key = key;
                slot->tiles = std::move(prepared);
                slot->bytes = prepared_bytes;
                slot->last_used =
                    gApplicationTileUseSequence.fetch_add(
                        1U, std::memory_order_relaxed) + 1U;
            }
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    HRESULT DpiChanged() noexcept {
        if (!swap_chain_ || !d2d_context_) {
            return CreateSurfaceResources();
        }
        d2d_context_->SetTarget(nullptr);
        target_bitmap_.Reset();
        return CreateTargetBitmap();
    }

    HRESULT SetFloatingPreview(const CanvasFloatingPreview& preview) noexcept {
        if (preview.struct_size < sizeof(CanvasFloatingPreview)
            || preview.transform.struct_size < sizeof(InkpodFloatingTransform)
            || preview.bounds.width < 0 || preview.bounds.height < 0
            || preview.transform.anchor < INKPOD_TRANSFORM_ANCHOR_TOP_LEFT
            || preview.transform.anchor > INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT
            || !std::isfinite(preview.transform.target_x)
            || !std::isfinite(preview.transform.target_y)
            || !std::isfinite(preview.transform.scale_x)
            || !std::isfinite(preview.transform.scale_y)
            || preview.transform.scale_x <= 0.0 || preview.transform.scale_y <= 0.0
            || !std::isfinite(preview.transform.rotation_degrees)) {
            return E_INVALIDARG;
        }
        const bool unchanged = preview.active == 0U && floating_preview_.active == 0U;
        floating_preview_ = preview;
        return unchanged ? S_FALSE : S_OK;
    }

    HRESULT SetGeometryPreview(const CanvasGeometryPreview& preview) noexcept {
        if (preview.struct_size < sizeof(CanvasGeometryPreview)
            || preview.active > 1U || preview.closed > 1U
            || preview.point_count > kCanvasGeometryPreviewPoints
            || !std::isfinite(preview.stroke_width) || preview.stroke_width < 0.0F
            || preview.stroke_width > 4096.0F || preview.reserved != 0U
            || preview.brush_shape > INKPOD_TRACE_SQUARE) {
            return E_INVALIDARG;
        }
        for (std::uint32_t index = 0U; index < preview.point_count; ++index) {
            if (!std::isfinite(preview.points[index].x)
                || !std::isfinite(preview.points[index].y)
                || !std::isfinite(preview.point_diameters[index])
                || preview.point_diameters[index] < 0.0F
                || preview.point_diameters[index] > 4096.0F) {
                return E_INVALIDARG;
            }
        }
        // Repeated cancellation is common when switching cells. It must not
        // consume a frame-latency signal or present the previous cell again.
        const bool unchanged = (preview.active == 0U && geometry_preview_.active == 0U)
            || (preview.active == geometry_preview_.active
                && preview.point_count == geometry_preview_.point_count
                && preview.closed == geometry_preview_.closed
                && preview.stroke_width == geometry_preview_.stroke_width
                && preview.brush_shape == geometry_preview_.brush_shape
                && std::equal(preview.point_diameters, preview.point_diameters + preview.point_count,
                    geometry_preview_.point_diameters)
                && std::equal(preview.points, preview.points + preview.point_count,
                    geometry_preview_.points,
                    [](const CanvasGeometryPoint& first, const CanvasGeometryPoint& second) {
                        return first.x == second.x && first.y == second.y;
                    }));
        geometry_preview_ = preview;
        return unchanged ? S_FALSE : S_OK;
    }

    HRESULT GetGeometryPreviewForSmokeTest(
        CanvasGeometryPreview& preview) const noexcept {
        preview = geometry_preview_;
        return S_OK;
    }

    HRESULT SimulateDeviceLossForSmokeTest() noexcept {
        DiscardSurfaceResources();
        return DXGI_ERROR_DEVICE_RESET;
    }

    HRESULT RecreateAfterSharedDeviceReset() noexcept {
        const HRESULT result = CreateSurfaceResources();
        return SUCCEEDED(result) ? RebuildTileCache() : result;
    }

    void DiscardForDeviceReset() noexcept {
        DiscardSurfaceResources();
    }

    void ClearSnapshot() noexcept {
        if (snapshot_ != nullptr) {
            inkpod_snapshot_release(&snapshot_);
        }
        snapshot_view_ = {};
        snapshot_document_revision_ = 0U;
        snapshot_presentation_epoch_ = 0U;
        snapshot_ready_ = true;
        transform_ = {};
        overlay_ = {};
        shooting_frames_ = {};
        floating_preview_ = {};
        geometry_preview_ = {};
        render_plan_ = {};
        tile_cache_.clear();
        ClearRetainedSources();
        active_source_ = {};
        source_retention_enabled_ = false;
        pending_upload_bytes_ = 0U;
        candidate_active_bytes_ = 0U;
        last_presented_document_revision_ = 0U;
        last_presented_presentation_epoch_ = 0U;
        last_presented_view_revision_ = 0U;
        last_presented_source_ = {};
        retained_snapshot_bytes_ = 0U;
        last_snapshot_submission_qpc_ = 0U;
        first_presented_revision_qpc_ = 0U;
        first_frame_ready_qpc_ = 0U;
        first_present_begin_qpc_ = 0U;
        has_presented_snapshot_ = false;
        gpu_tile_bytes_ = 0U;
    }

    void SetTileBudgetBytes(std::uint64_t tile_budget_bytes) noexcept {
        tile_budget_bytes_ = std::max<std::uint64_t>(1U, tile_budget_bytes);
        TrimTileCache();
    }

    [[nodiscard]] std::uint64_t RetainedSnapshotBytes() const noexcept {
        return retained_snapshot_bytes_;
    }

    [[nodiscard]] std::uint64_t GpuTileBytes() const noexcept {
        std::uint64_t bytes = gpu_tile_bytes_;
        for (const auto& source : retained_sources_) {
            bytes = SaturatingAdd(bytes, source.bytes);
        }
        return bytes;
    }

    [[nodiscard]] std::uint64_t PendingUploadBytes() const noexcept {
        return pending_upload_bytes_;
    }

    [[nodiscard]] std::uint64_t SequenceCacheSourceCount() const noexcept {
        std::uint64_t count = source_retention_enabled_ && candidate_active_bytes_ != 0U ? 1U : 0U;
        for (const auto& source : retained_sources_) {
            count += source.key ? 1U : 0U;
        }
        return count;
    }

    [[nodiscard]] std::uint64_t SequenceCacheBytes() const noexcept {
        std::uint64_t bytes = source_retention_enabled_
            ? SaturatingAdd(gpu_tile_bytes_, pending_upload_bytes_) : 0U;
        for (const auto& source : retained_sources_) {
            bytes = SaturatingAdd(bytes, source.bytes);
        }
        return bytes;
    }

    [[nodiscard]] std::uint64_t SequenceCacheEvictionCount() const noexcept {
        return sequence_cache_eviction_count_;
    }

    [[nodiscard]] std::uint64_t UploadedTileCount() const noexcept {
        return uploaded_tile_count_;
    }

    [[nodiscard]] std::uint64_t UploadedTileBytes() const noexcept {
        return uploaded_tile_bytes_;
    }

    [[nodiscard]] std::uint64_t LastPresentedDocumentRevision() const noexcept {
        return last_presented_document_revision_;
    }

    [[nodiscard]] std::uint64_t LastPresentedPresentationEpoch() const noexcept {
        return last_presented_presentation_epoch_;
    }

    [[nodiscard]] std::uint64_t LastPresentedViewRevision() const noexcept {
        return last_presented_view_revision_;
    }

    [[nodiscard]] InkpodSnapshotSourceIdentity LastPresentedSource() const noexcept {
        return last_presented_source_;
    }

    [[nodiscard]] std::uint64_t LastSnapshotSubmissionQpc() const noexcept {
        return last_snapshot_submission_qpc_;
    }

    [[nodiscard]] std::uint64_t FirstPresentedRevisionQpc() const noexcept {
        return first_presented_revision_qpc_;
    }

    [[nodiscard]] std::uint64_t FirstFrameReadyQpc() const noexcept {
        return first_frame_ready_qpc_;
    }

    [[nodiscard]] std::uint64_t FirstPresentBeginQpc() const noexcept {
        return first_present_begin_qpc_;
    }

    [[nodiscard]] std::uint64_t FrameLatencyTimeoutCount() const noexcept {
        return frame_latency_timeout_count_;
    }

    [[nodiscard]] HANDLE FrameLatencyHandle() const noexcept {
        return frame_latency_waitable_;
    }

    HRESULT AcquireFrameLatencyPermit(DWORD milliseconds, HANDLE interrupt_event = nullptr) noexcept {
        if (frame_latency_permit_ || frame_latency_waitable_ == nullptr) {
            return S_OK;
        }
        const HANDLE events[]{interrupt_event, frame_latency_waitable_};
        const DWORD wait = interrupt_event == nullptr
            ? WaitForSingleObjectEx(frame_latency_waitable_, milliseconds, FALSE)
            : WaitForMultipleObjectsEx(2U, events, FALSE, milliseconds, FALSE);
        if (wait == WAIT_TIMEOUT) {
            if (milliseconds != 0U) {
                RecordFrameLatencyTimeout();
            }
            return S_FALSE;
        }
        if (wait == WAIT_FAILED) {
            return HRESULT_FROM_WIN32(GetLastError());
        }
        if (interrupt_event != nullptr && wait == WAIT_OBJECT_0) {
            return S_FALSE;
        }
        frame_latency_permit_ = true;
        return S_OK;
    }

    void AcceptFrameLatencySignal() noexcept {
        // RendererHost has just consumed this exact handle in its multi-surface
        // wait. No second wait may consume the same frame's readiness.
        frame_latency_permit_ = true;
    }

    void RecordFrameLatencyTimeout() noexcept {
        frame_latency_timeout_count_ = SaturatingAdd(frame_latency_timeout_count_, 1U);
    }

    [[nodiscard]] std::optional<std::uint64_t> OldestRetainedSourceUse() const noexcept {
        std::optional<std::uint64_t> oldest;
        for (const auto& source : retained_sources_) {
            if (source.key && (!oldest.has_value() || source.last_used < oldest.value())) {
                oldest = source.last_used;
            }
        }
        return oldest;
    }

    [[nodiscard]] std::optional<std::uint64_t> ActiveCachedSourceUse() const noexcept {
        return source_retention_enabled_ && candidate_active_bytes_ != 0U
            ? std::optional(active_source_last_used_) : std::nullopt;
    }

    void DisableSourceRetention() noexcept {
        // Keep every active bitmap. Only its eligibility for reuse after leaving
        // this source is removed when active sources exceed the smaller cache cap.
        source_retention_enabled_ = false;
    }

    bool EvictOldestRetainedSource() noexcept {
        auto victim = retained_sources_.end();
        for (auto iterator = retained_sources_.begin(); iterator != retained_sources_.end(); ++iterator) {
            if (iterator->key
                && (victim == retained_sources_.end() || iterator->last_used < victim->last_used)) {
                victim = iterator;
            }
        }
        if (victim == retained_sources_.end()) {
            return false;
        }
        victim->tiles.clear();
        victim->key = {};
        victim->bytes = 0U;
        victim->last_used = 0U;
        sequence_cache_eviction_count_ = SaturatingAdd(sequence_cache_eviction_count_, 1U);
        return true;
    }

    [[nodiscard]] std::uint64_t SwapChainBytes() const noexcept {
        return static_cast<std::uint64_t>(surface_width_) * surface_height_ * 4U * 2U;
    }

    [[nodiscard]] std::uint64_t CachedTileCount() const noexcept {
        std::uint64_t count = static_cast<std::uint64_t>(tile_cache_.size());
        for (const auto& source : retained_sources_) {
            count = SaturatingAdd(count, static_cast<std::uint64_t>(source.tiles.size()));
        }
        return count;
    }

    [[nodiscard]] std::uint64_t ActiveTileCount() const noexcept {
        return static_cast<std::uint64_t>(std::count_if(
            tile_cache_.cbegin(), tile_cache_.cend(), [](const auto& entry) {
                return entry.second.active;
            }));
    }

    [[nodiscard]] std::uint64_t ActiveTileBytes() const noexcept {
        std::uint64_t bytes{};
        for (const auto& entry : tile_cache_) {
            if (entry.second.active) {
                bytes = SaturatingAdd(bytes, entry.second.byte_count);
            }
        }
        // A failed preparation/upload still reserves its complete candidate
        // for recovery. Do not allocate another view into that reservation.
        return std::max(bytes, candidate_active_bytes_);
    }

    [[nodiscard]] std::optional<std::uint64_t> OldestInactiveUse() const noexcept {
        std::optional<std::uint64_t> oldest = OldestRetainedSourceUse();
        for (const auto& entry : tile_cache_) {
            if (!entry.second.active
                && (!oldest.has_value() || entry.second.last_used < oldest.value())) {
                oldest = entry.second.last_used;
            }
        }
        return oldest;
    }

    bool EvictOldestInactive() noexcept {
        auto victim = tile_cache_.end();
        for (auto iterator = tile_cache_.begin(); iterator != tile_cache_.end(); ++iterator) {
            if (iterator->second.active) {
                continue;
            }
            if (victim == tile_cache_.end()
                || iterator->second.last_used < victim->second.last_used) {
                victim = iterator;
            }
        }
        const auto source_use = OldestRetainedSourceUse();
        if (source_use.has_value()
            && (victim == tile_cache_.end() || source_use.value() <= victim->second.last_used)) {
            return EvictOldestRetainedSource();
        }
        if (victim == tile_cache_.end()) {
            return false;
        }
        gpu_tile_bytes_ -= victim->second.byte_count;
        tile_cache_.erase(victim);
        return true;
    }


    HRESULT RenderAndReadPixelForSmokeTest(
        UINT x,
        UINT y,
        CanvasPixelRgba8& pixel,
        HANDLE interrupt_event) noexcept {
        return Render(&pixel, x, y, 100U, interrupt_event);
    }

    HRESULT CopyBackBufferPixel(
        UINT x,
        UINT y,
        CanvasPixelRgba8& pixel) noexcept {
        if (!swap_chain_ || x >= surface_width_ || y >= surface_height_) {
            return E_INVALIDARG;
        }
        ComPtr<ID3D11Texture2D> source;
        HRESULT result = swap_chain_->GetBuffer(0U, IID_PPV_ARGS(&source));
        if (FAILED(result)) {
            return result;
        }
        D3D11_TEXTURE2D_DESC description{};
        source->GetDesc(&description);
        description.Usage = D3D11_USAGE_STAGING;
        description.BindFlags = 0U;
        description.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        description.MiscFlags = 0U;
        ComPtr<ID3D11Texture2D> staging;
        result = shared_.D3dDevice()->CreateTexture2D(&description, nullptr, &staging);
        if (FAILED(result)) {
            return result;
        }
        shared_.D3dContext()->CopyResource(staging.Get(), source.Get());
        D3D11_MAPPED_SUBRESOURCE mapped{};
        result = shared_.D3dContext()->Map(
            staging.Get(), 0U, D3D11_MAP_READ, 0U, &mapped);
        if (FAILED(result)) {
            return result;
        }
        const auto* bgra = static_cast<const std::uint8_t*>(mapped.pData)
            + static_cast<std::size_t>(y) * mapped.RowPitch
            + static_cast<std::size_t>(x) * 4U;
        pixel = CanvasPixelRgba8{bgra[2], bgra[1], bgra[0], bgra[3]};
        shared_.D3dContext()->Unmap(staging.Get(), 0U);
        return S_OK;
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
    static bool ValidateSourceIdentity(const InkpodSnapshotSourceIdentity& source) noexcept {
        const bool has_uuid = source.document_uuid_high != 0U || source.document_uuid_low != 0U;
        if (source.flags == 0U) {
            return !has_uuid && source.source_generation == 0U && source.owner_generation == 0U;
        }
        return source.flags == INKPOD_SNAPSHOT_SOURCE_SEQUENCE_PRISTINE
            && has_uuid && source.source_generation != 0U && source.owner_generation != 0U;
    }

    void ClearRetainedSources() noexcept {
        for (auto& source : retained_sources_) {
            source.tiles.clear();
            source.key = {};
            source.bytes = 0U;
            source.last_used = 0U;
        }
    }

    void SelectSourceCache(SequenceSourceKey next, bool continue_active_document) noexcept {
        const std::uint64_t next_use =
            gApplicationTileUseSequence.fetch_add(1U, std::memory_order_relaxed) + 1U;
        if (active_source_ == next) {
            active_source_last_used_ = next_use;
            return;
        }
        if (active_source_ && !next && continue_active_document) {
            // The first edit/preview retires this pristine bank but keeps its
            // current bitmap map as the ordinary cache. Existing tile revision
            // checks then upload only changed tiles, without duplicating GPU
            // ownership or modifying any retained pristine source bank.
            active_source_ = {};
            active_source_last_used_ = next_use;
            source_retention_enabled_ = false;
            return;
        }
        if (next) {
            // Replacement catalogs cannot keep old identities alive in this
            // Canvas, including after an intervening non-pristine edit.
            for (auto& source : retained_sources_) {
                if (source.key && source.key.owner_generation != next.owner_generation) {
                    source.tiles.clear();
                    source.key = {};
                    source.bytes = 0U;
                    source.last_used = 0U;
                }
            }
            if (active_source_ && active_source_.owner_generation != next.owner_generation) {
                source_retention_enabled_ = false;
            }
        }
        const bool retain_previous = source_retention_enabled_ && active_source_
            && gpu_tile_bytes_ != 0U;
        const auto target = std::find_if(retained_sources_.begin(), retained_sources_.end(),
            [next](const CachedSequenceSource& source) { return next && source.key == next; });
        if (target != retained_sources_.end()) {
            tile_cache_.swap(target->tiles);
            std::swap(gpu_tile_bytes_, target->bytes);
            if (retain_previous) {
                target->key = active_source_;
                target->last_used = active_source_last_used_;
            } else {
                target->tiles.clear();
                target->key = {};
                target->bytes = 0U;
                target->last_used = 0U;
            }
        } else if (retain_previous) {
            auto slot = std::find_if(retained_sources_.begin(), retained_sources_.end(),
                [](const CachedSequenceSource& source) { return !source.key; });
            if (slot == retained_sources_.end()) {
                (void)EvictOldestRetainedSource();
                slot = std::find_if(retained_sources_.begin(), retained_sources_.end(),
                    [](const CachedSequenceSource& source) { return !source.key; });
            }
            if (slot != retained_sources_.end()) {
                slot->tiles.swap(tile_cache_);
                slot->key = active_source_;
                slot->bytes = std::exchange(gpu_tile_bytes_, 0U);
                slot->last_used = active_source_last_used_;
            }
        } else {
            tile_cache_.clear();
            gpu_tile_bytes_ = 0U;
        }
        active_source_ = next;
        active_source_last_used_ = next_use;
        source_retention_enabled_ = false;
    }

    D2D1_MATRIX_3X2_F DocumentTransform() const noexcept {
        const float scale = static_cast<float>(transform_.zoom);
        const bool flip_horizontal = (transform_.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) != 0U;
        const bool flip_vertical = (transform_.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL) != 0U;
        return D2D1::Matrix3x2F(
            flip_horizontal ? -scale : scale,
            0.0F,
            0.0F,
            flip_vertical ? -scale : scale,
            static_cast<float>(transform_.pan_x)
                + (flip_horizontal
                        ? static_cast<float>(transform_.document_width) * scale
                        : 0.0F),
            static_cast<float>(transform_.pan_y)
                + (flip_vertical
                        ? static_cast<float>(transform_.document_height) * scale
                        : 0.0F));
    }

    static bool ValidateOverlay(const InkpodSnapshotOverlay& overlay) noexcept {
        constexpr std::uint32_t known_flags =
            INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE
            | INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE
            | INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE
            | INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED
            | INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW;
        if ((overlay.flags & ~known_flags) != 0U || overlay.reserved != 0U
            || overlay.grid_spacing_x == 0U || overlay.grid_spacing_y == 0U
            || overlay.grid_subdivisions == 0U || overlay.grid_subdivisions > 1024U
            || overlay.guide_count > kMaximumSnapshotGuides
            || overlay.guide_stride_bytes < sizeof(InkpodSnapshotGuide)
            || overlay.guide_stride_bytes % alignof(InkpodSnapshotGuide) != 0U
            || (overlay.guide_count != 0U && overlay.guides == nullptr)
            || (overlay.guide_count != 0U
                && reinterpret_cast<std::uintptr_t>(overlay.guides)
                    % alignof(InkpodSnapshotGuide) != 0U)
            || overlay.guide_stride_bytes
                > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
            || (overlay.guide_count > 1U
                && overlay.guide_stride_bytes
                    > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
                        / (overlay.guide_count - 1U))) {
            return false;
        }
        const auto* bytes = reinterpret_cast<const std::byte*>(overlay.guides);
        for (std::uint64_t index = 0; index < overlay.guide_count; ++index) {
            const auto* guide = reinterpret_cast<const InkpodSnapshotGuide*>(
                bytes + static_cast<std::size_t>(index * overlay.guide_stride_bytes));
            if (guide->struct_size < sizeof(InkpodSnapshotGuide)
                || guide->struct_size > overlay.guide_stride_bytes
                || guide->reserved != 0U
                || (guide->axis != INKPOD_GUIDE_HORIZONTAL
                    && guide->axis != INKPOD_GUIDE_VERTICAL)) {
                return false;
            }
        }
        return true;
    }


    static bool ValidateShootingFrames(
        const InkpodSnapshotShootingFrameView& view) noexcept {
        if (view.abi_version != INKPOD_ABI_VERSION || view.feature_flags != 0U
            || view.frame_count > 1U
            || view.frame_stride_bytes < sizeof(InkpodShootingFrameInfo)
            || view.frame_stride_bytes % alignof(InkpodShootingFrameInfo) != 0U
            || (view.frame_count != 0U && view.frames == nullptr)) {
            return false;
        }
        if (view.frame_count == 0U) {
            return true;
        }
        const auto& frame = *view.frames;
        return frame.struct_size >= sizeof(InkpodShootingFrameInfo)
            && frame.struct_size <= view.frame_stride_bytes
            && frame.feature_flags == 0U && frame.frame_id <= INT64_MAX
            && frame.width_milli != 0U && frame.height_milli != 0U
            && frame.visible <= 1U
            && frame.reserved == 0U
            && frame.anchor >= INKPOD_SHOOTING_FRAME_ANCHOR_TOP_LEFT
            && frame.anchor <= INKPOD_SHOOTING_FRAME_ANCHOR_BOTTOM_RIGHT;
    }

    static bool ValidateRenderPlan(
        const InkpodSnapshotRenderPlan& plan,
        const InkpodSnapshotView& view) noexcept {
        if (plan.abi_version != INKPOD_ABI_VERSION || plan.feature_flags != 0U
            || plan.pass_count > kMaximumRenderPasses
            || plan.pass_stride_bytes < sizeof(InkpodSnapshotRenderPass)
            || plan.pass_stride_bytes % alignof(InkpodSnapshotRenderPass) != 0U
            || (plan.pass_count != 0U && plan.passes == nullptr)
            || (plan.passes != nullptr
                && reinterpret_cast<std::uintptr_t>(plan.passes)
                    % alignof(InkpodSnapshotRenderPass) != 0U)
            || plan.pass_stride_bytes
                > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
            || (plan.pass_count > 1U
                && plan.pass_stride_bytes
                    > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
                        / (plan.pass_count - 1U))) {
            return false;
        }
        const auto range_is_valid = [](std::uint64_t first, std::uint64_t count,
                                        std::uint64_t total) noexcept {
            return first <= total && count <= total - first;
        };
        const auto* pass_bytes = reinterpret_cast<const std::byte*>(plan.passes);
        std::uint64_t active_layer{};
        for (std::uint64_t index = 0; index < plan.pass_count; ++index) {
            const auto* pass = reinterpret_cast<const InkpodSnapshotRenderPass*>(
                pass_bytes + static_cast<std::size_t>(index * plan.pass_stride_bytes));
            if (pass->struct_size < sizeof(InkpodSnapshotRenderPass)
                || pass->struct_size > plan.pass_stride_bytes || pass->reserved != 0U
                || pass->opacity_milli > 1000U) {
                return false;
            }
            switch (pass->kind) {
                case INKPOD_RENDER_PASS_LAYER_BEGIN:
                    if (active_layer != 0U || pass->layer_id == 0U || pass->plane_id != 0U
                        || pass->first_item != 0U || pass->item_count != 0U) {
                        return false;
                    }
                    active_layer = pass->layer_id;
                    break;
                case INKPOD_RENDER_PASS_LAYER_END:
                    if (active_layer == 0U || pass->layer_id != active_layer
                        || pass->plane_id != 0U || pass->opacity_milli != 1000U
                        || pass->first_item != 0U || pass->item_count != 0U) {
                        return false;
                    }
                    active_layer = 0U;
                    break;
                case INKPOD_RENDER_PASS_RASTER_TILES:
                    if (pass->item_count == 0U || pass->opacity_milli != 1000U
                        || !range_is_valid(
                            pass->first_item, pass->item_count, view.tile_count)
                        || (active_layer != 0U && pass->layer_id != active_layer)
                        || (active_layer == 0U && pass->layer_id != 0U)) {
                        return false;
                    }
                    break;
                default:
                    return false;
            }
        }
        return active_layer == 0U;
    }



    static D2D1_COLOR_F CanvasColor(const InkpodColorValue& value) noexcept {
        const float maximum = value.depth == INKPOD_COLOR_DEPTH_16 ? 65535.0F : 255.0F;
        return D2D1::ColorF(
            static_cast<float>(value.red) / maximum,
            static_cast<float>(value.green) / maximum,
            static_cast<float>(value.blue) / maximum,
            static_cast<float>(value.alpha) / maximum);
    }


    HRESULT DrawShootingFrame() noexcept {
        if (shooting_frames_.frame_count == 0U || shooting_frames_.frames == nullptr) {
            return S_OK;
        }
        const InkpodShootingFrameInfo& frame = *shooting_frames_.frames;
        if (frame.visible == 0U) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> shadow;
        ComPtr<ID2D1SolidColorBrush> foreground;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.05F, 0.05F, 0.05F, 0.75F), &shadow);
        if (SUCCEEDED(result)) {
            result = d2d_context_->CreateSolidColorBrush(
                D2D1::ColorF(1.0F, 64.0F / 255.0F, 64.0F / 255.0F, 1.0F), &foreground);
        }
        if (FAILED(result)) {
            return result;
        }
        std::array<D2D1_POINT_2F, 4U> points{};
        for (std::size_t index = 0U; index < points.size(); ++index) {
            points[index] = D2D1::Point2F(
                static_cast<float>(frame.corners[index].x_milli) / 1000.0F,
                static_cast<float>(frame.corners[index].y_milli) / 1000.0F);
        }
        const float width = static_cast<float>(std::max(1.0, 1.5 / transform_.zoom));
        const float shadow_width = width * 2.5F;
        for (std::size_t index = 0U; index < points.size(); ++index) {
            const D2D1_POINT_2F next = points[(index + 1U) % points.size()];
            d2d_context_->DrawLine(points[index], next, shadow.Get(), shadow_width);
            d2d_context_->DrawLine(points[index], next, foreground.Get(), width);
        }
        const float radius = static_cast<float>(std::max(2.0, 3.0 / transform_.zoom));
        for (const D2D1_POINT_2F point : points) {
            d2d_context_->FillEllipse(D2D1::Ellipse(point, radius, radius), shadow.Get());
            d2d_context_->DrawEllipse(D2D1::Ellipse(point, radius, radius), foreground.Get(), width);
        }
        const D2D1_POINT_2F center = D2D1::Point2F(
            static_cast<float>(frame.center_x_milli) / 1000.0F,
            static_cast<float>(frame.center_y_milli) / 1000.0F);
        d2d_context_->DrawEllipse(D2D1::Ellipse(center, radius, radius), foreground.Get(), width);
        const float edge_x = points[1].x - points[0].x;
        const float edge_y = points[1].y - points[0].y;
        const float edge_length = std::hypot(edge_x, edge_y);
        if (edge_length > 0.0F) {
            const D2D1_POINT_2F edge_center = D2D1::Point2F(
                (points[0].x + points[1].x) * 0.5F,
                (points[0].y + points[1].y) * 0.5F);
            const float handle_distance = static_cast<float>(24.0 / transform_.zoom);
            const D2D1_POINT_2F rotation_handle = D2D1::Point2F(
                edge_center.x + edge_y / edge_length * handle_distance,
                edge_center.y - edge_x / edge_length * handle_distance);
            d2d_context_->DrawLine(
                edge_center, rotation_handle, foreground.Get(), width);
            d2d_context_->FillEllipse(
                D2D1::Ellipse(rotation_handle, radius, radius), shadow.Get());
            d2d_context_->DrawEllipse(
                D2D1::Ellipse(rotation_handle, radius, radius), foreground.Get(), width);
        }
        return S_OK;
    }

    HRESULT DrawRenderPass(
        const InkpodSnapshotRenderPass& pass,
        bool& layer_active) noexcept {
        switch (pass.kind) {
            case INKPOD_RENDER_PASS_LAYER_BEGIN: {
                if (layer_active) {
                    return E_INVALIDARG;
                }
                D2D1_LAYER_PARAMETERS1 parameters{};
                parameters.contentBounds = D2D1::InfiniteRect();
                parameters.maskAntialiasMode = D2D1_ANTIALIAS_MODE_PER_PRIMITIVE;
                parameters.maskTransform = D2D1::Matrix3x2F::Identity();
                parameters.opacity = static_cast<float>(pass.opacity_milli) / 1000.0F;
                parameters.layerOptions = D2D1_LAYER_OPTIONS1_NONE;
                d2d_context_->PushLayer(parameters, nullptr);
                layer_active = true;
                return S_OK;
            }
            case INKPOD_RENDER_PASS_LAYER_END:
                if (!layer_active) {
                    return E_INVALIDARG;
                }
                d2d_context_->PopLayer();
                layer_active = false;
                return S_OK;
            case INKPOD_RENDER_PASS_RASTER_TILES: {
                const auto* tile_bytes = reinterpret_cast<const std::byte*>(snapshot_view_.tiles);
                for (std::uint64_t offset = 0; offset < pass.item_count; ++offset) {
                    const std::uint64_t index = pass.first_item + offset;
                    const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                        tile_bytes
                        + static_cast<std::size_t>(index * snapshot_view_.tile_stride_bytes));
                    const auto cached = tile_cache_.find(tile->tile_id);
                    if (cached == tile_cache_.end() || !cached->second.active) {
                        return E_INVALIDARG;
                    }
                    const CachedTile& value = cached->second;
                    d2d_context_->DrawBitmap(
                        value.bitmap.Get(),
                        D2D1::RectF(
                            static_cast<float>(value.origin_x),
                            static_cast<float>(value.origin_y),
                            static_cast<float>(value.origin_x) + static_cast<float>(value.width),
                            static_cast<float>(value.origin_y) + static_cast<float>(value.height)),
                        1.0F,
                        D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR);
                }
                return S_OK;
            }
            default:
                return E_INVALIDARG;
        }
    }

    HRESULT DrawOrderedContent() noexcept {
        HRESULT result = S_OK;
        bool layer_active{};
        const auto* pass_bytes = reinterpret_cast<const std::byte*>(render_plan_.passes);
        for (std::uint64_t index = 0; index < render_plan_.pass_count; ++index) {
            const auto* pass = reinterpret_cast<const InkpodSnapshotRenderPass*>(
                pass_bytes + static_cast<std::size_t>(index * render_plan_.pass_stride_bytes));
            result = DrawRenderPass(*pass, layer_active);
            if (FAILED(result)) {
                if (layer_active) {
                    d2d_context_->PopLayer();
                }
                return result;
            }
        }
        return layer_active ? E_INVALIDARG : S_OK;
    }

    HRESULT DrawOverlays() noexcept {
        ComPtr<ID2D1SolidColorBrush> brush;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.15F, 0.55F, 0.95F, 0.45F), &brush);
        if (FAILED(result)) {
            return result;
        }
        const float stroke_width = static_cast<float>(1.0 / transform_.zoom);
        if ((overlay_.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) != 0U) {
            const double step_x = static_cast<double>(overlay_.grid_spacing_x)
                / static_cast<double>(overlay_.grid_subdivisions);
            const double step_y = static_cast<double>(overlay_.grid_spacing_y)
                / static_cast<double>(overlay_.grid_subdivisions);
            const auto x_count = static_cast<std::uint64_t>(
                std::ceil(static_cast<double>(transform_.document_width) / step_x) + 2.0);
            const auto y_count = static_cast<std::uint64_t>(
                std::ceil(static_cast<double>(transform_.document_height) / step_y) + 2.0);
            if (step_x * transform_.zoom >= 4.0 && x_count <= kMaximumOverlayLines) {
                const double first = static_cast<double>(overlay_.grid_origin_x)
                    + std::ceil(-static_cast<double>(overlay_.grid_origin_x) / step_x) * step_x;
                for (double x = first; x <= transform_.document_width; x += step_x) {
                    d2d_context_->DrawLine(
                        D2D1::Point2F(static_cast<float>(x), 0.0F),
                        D2D1::Point2F(
                            static_cast<float>(x),
                            static_cast<float>(transform_.document_height)),
                        brush.Get(),
                        stroke_width);
                }
            }
            if (step_y * transform_.zoom >= 4.0 && y_count <= kMaximumOverlayLines) {
                const double first = static_cast<double>(overlay_.grid_origin_y)
                    + std::ceil(-static_cast<double>(overlay_.grid_origin_y) / step_y) * step_y;
                for (double y = first; y <= transform_.document_height; y += step_y) {
                    d2d_context_->DrawLine(
                        D2D1::Point2F(0.0F, static_cast<float>(y)),
                        D2D1::Point2F(
                            static_cast<float>(transform_.document_width),
                            static_cast<float>(y)),
                        brush.Get(),
                        stroke_width);
                }
            }
        }
        if ((overlay_.flags & INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE) != 0U) {
            const auto* bytes = reinterpret_cast<const std::byte*>(overlay_.guides);
            brush->SetColor(D2D1::ColorF(0.1F, 0.8F, 0.95F, 0.9F));
            for (std::uint64_t index = 0; index < overlay_.guide_count; ++index) {
                const auto* guide = reinterpret_cast<const InkpodSnapshotGuide*>(
                    bytes + static_cast<std::size_t>(index * overlay_.guide_stride_bytes));
                if (guide->axis == INKPOD_GUIDE_VERTICAL) {
                    d2d_context_->DrawLine(
                        D2D1::Point2F(static_cast<float>(guide->position), 0.0F),
                        D2D1::Point2F(
                            static_cast<float>(guide->position),
                            static_cast<float>(transform_.document_height)),
                        brush.Get(),
                        stroke_width);
                } else {
                    d2d_context_->DrawLine(
                        D2D1::Point2F(0.0F, static_cast<float>(guide->position)),
                        D2D1::Point2F(
                            static_cast<float>(transform_.document_width),
                            static_cast<float>(guide->position)),
                        brush.Get(),
                        stroke_width);
                }
            }
        }
        return S_OK;
    }

    HRESULT DrawFloatingPreview() noexcept {
        if (floating_preview_.active == 0U || floating_preview_.bounds.width <= 0
            || floating_preview_.bounds.height <= 0) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> brush;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.92F, 0.2F, 0.78F, 1.0F), &brush);
        if (FAILED(result)) {
            return result;
        }
        const auto& bounds = floating_preview_.bounds;
        const auto& transform = floating_preview_.transform;
        const double left = static_cast<double>(bounds.x);
        const double top = static_cast<double>(bounds.y);
        const double right = left + static_cast<double>(bounds.width);
        const double bottom = top + static_cast<double>(bounds.height);
        const double anchor_x = transform.anchor == INKPOD_TRANSFORM_ANCHOR_TOP_RIGHT
                || transform.anchor == INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT
            ? right
            : transform.anchor == INKPOD_TRANSFORM_ANCHOR_CENTER ? (left + right) / 2.0 : left;
        const double anchor_y = transform.anchor == INKPOD_TRANSFORM_ANCHOR_BOTTOM_LEFT
                || transform.anchor == INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT
            ? bottom
            : transform.anchor == INKPOD_TRANSFORM_ANCHOR_CENTER ? (top + bottom) / 2.0 : top;
        const double radians = transform.rotation_degrees * 3.14159265358979323846 / 180.0;
        const double sine = std::sin(radians);
        const double cosine = std::cos(radians);
        const auto point = [&](double x, double y) {
            const double local_x = (x - anchor_x) * transform.scale_x;
            const double local_y = (y - anchor_y) * transform.scale_y;
            return D2D1::Point2F(
                static_cast<float>(transform.target_x + local_x * cosine - local_y * sine),
                static_cast<float>(transform.target_y + local_x * sine + local_y * cosine));
        };
        const std::array<D2D1_POINT_2F, 4U> corners{
            point(left, top), point(right, top), point(right, bottom), point(left, bottom)};
        const float stroke_width = static_cast<float>(std::max(1.0, 1.5 / transform_.zoom));
        const float handle_radius = static_cast<float>(std::max(2.0, 4.0 / transform_.zoom));
        for (std::size_t index = 0U; index < corners.size(); ++index) {
            d2d_context_->DrawLine(
                corners[index], corners[(index + 1U) % corners.size()], brush.Get(), stroke_width);
            d2d_context_->FillEllipse(
                D2D1::Ellipse(corners[index], handle_radius, handle_radius), brush.Get());
        }
        return S_OK;
    }

    HRESULT DrawGeometryPreview() noexcept {
        if (geometry_preview_.active == 0U || geometry_preview_.point_count == 0U) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> shadow;
        ComPtr<ID2D1SolidColorBrush> foreground;
        const bool region_preview = geometry_preview_.stroke_width > 0.0F;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(
                0.05F, 0.05F, 0.05F, region_preview ? 0.55F : 0.9F),
            &shadow);
        if (SUCCEEDED(result)) {
            result = d2d_context_->CreateSolidColorBrush(
                D2D1::ColorF(
                    0.1F, 0.85F, 1.0F, region_preview ? 0.45F : 1.0F),
                &foreground);
        }
        if (FAILED(result)) {
            return result;
        }
        if (geometry_preview_.brush_shape != 0U) {
            // Interpolate bounded display samples only. The Core owns the exact pixel mask.
            for (std::uint32_t index = 0U; index < geometry_preview_.point_count; ++index) {
                const auto& a = geometry_preview_.points[index == 0U ? 0U : index - 1U];
                const auto& b = geometry_preview_.points[index];
                const float da = geometry_preview_.point_diameters[index == 0U ? 0U : index - 1U];
                const float db = geometry_preview_.point_diameters[index];
                const double distance = std::hypot(static_cast<double>(b.x) - a.x, static_cast<double>(b.y) - a.y);
                const auto steps = static_cast<std::uint32_t>(std::clamp(std::ceil(distance * 2.0), 1.0, 256.0));
                for (std::uint32_t step = 0U; step <= steps; ++step) {
                    const float t = static_cast<float>(step) / static_cast<float>(steps);
                    const auto center = D2D1::Point2F(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    const float radius = (da + (db - da) * t) / 2.0F;
                    if (radius <= 0.0F) continue;
                    if (geometry_preview_.brush_shape == INKPOD_TRACE_SQUARE) {
                        d2d_context_->FillRectangle(D2D1::RectF(center.x - radius, center.y - radius,
                            center.x + radius, center.y + radius), foreground.Get());
                    } else {
                        d2d_context_->FillEllipse(D2D1::Ellipse(center, radius, radius), foreground.Get());
                    }
                }
            }
            return S_OK;
        }
        const float device_width =
            static_cast<float>(std::max(1.0, 1.5 / transform_.zoom));
        const float width = region_preview
            ? std::max(device_width, geometry_preview_.stroke_width)
            : device_width;
        const float shadow_width = region_preview
            ? width + device_width * 2.0F
            : width * 2.5F;
        if (geometry_preview_.point_count == 1U) {
            if (!region_preview) {
                return S_OK;
            }
            const auto center = D2D1::Point2F(
                geometry_preview_.points[0].x, geometry_preview_.points[0].y);
            d2d_context_->FillEllipse(
                D2D1::Ellipse(
                    center, shadow_width / 2.0F, shadow_width / 2.0F),
                shadow.Get());
            d2d_context_->FillEllipse(
                D2D1::Ellipse(center, width / 2.0F, width / 2.0F),
                foreground.Get());
            return S_OK;
        }
        const auto draw = [&](std::uint32_t first, std::uint32_t second) {
            const auto start = D2D1::Point2F(
                geometry_preview_.points[first].x, geometry_preview_.points[first].y);
            const auto end = D2D1::Point2F(
                geometry_preview_.points[second].x, geometry_preview_.points[second].y);
            d2d_context_->DrawLine(start, end, shadow.Get(), shadow_width);
            d2d_context_->DrawLine(start, end, foreground.Get(), width);
        };
        for (std::uint32_t index = 1U; index < geometry_preview_.point_count; ++index) {
            draw(index - 1U, index);
        }
        if (geometry_preview_.closed != 0U && geometry_preview_.point_count > 2U) {
            draw(geometry_preview_.point_count - 1U, 0U);
        }
        return S_OK;
    }

    HRESULT DrawRulers() noexcept {
        if ((overlay_.flags & INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE) == 0U
            || transform_.zoom <= 0.0) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> background;
        ComPtr<ID2D1SolidColorBrush> ticks;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.16F, 0.17F, 0.19F, 0.96F), &background);
        if (FAILED(result)) {
            return result;
        }
        result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.82F, 0.84F, 0.88F, 1.0F), &ticks);
        if (FAILED(result)) {
            return result;
        }
        constexpr float extent = 22.0F;
        const D2D1_SIZE_F size = d2d_context_->GetSize();
        d2d_context_->FillRectangle(
            D2D1::RectF(0.0F, 0.0F, size.width, extent), background.Get());
        d2d_context_->FillRectangle(
            D2D1::RectF(0.0F, 0.0F, extent, size.height), background.Get());
        d2d_context_->DrawLine(
            D2D1::Point2F(0.0F, extent),
            D2D1::Point2F(size.width, extent),
            ticks.Get());
        d2d_context_->DrawLine(
            D2D1::Point2F(extent, 0.0F),
            D2D1::Point2F(extent, size.height),
            ticks.Get());

        double step = 1.0;
        while (step * transform_.zoom < 8.0) {
            step *= 10.0;
        }
        const bool flip_x = (transform_.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) != 0U;
        const bool flip_y = (transform_.flags
            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL) != 0U;
        const auto device_x = [&](double document_x) {
            const double source = flip_x
                ? static_cast<double>(transform_.document_width) - document_x
                : document_x;
            return static_cast<float>(transform_.pan_x + source * transform_.zoom);
        };
        const auto device_y = [&](double document_y) {
            const double source = flip_y
                ? static_cast<double>(transform_.document_height) - document_y
                : document_y;
            return static_cast<float>(transform_.pan_y + source * transform_.zoom);
        };
        const auto x_count = static_cast<std::uint64_t>(
            std::ceil(static_cast<double>(transform_.document_width) / step) + 1.0);
        const auto y_count = static_cast<std::uint64_t>(
            std::ceil(static_cast<double>(transform_.document_height) / step) + 1.0);
        if (x_count <= kMaximumOverlayLines) {
            for (std::uint64_t index = 0U; index < x_count; ++index) {
                const float x = device_x(static_cast<double>(index) * step);
                if (x >= extent && x <= size.width) {
                    const float length = index % 5U == 0U ? 10.0F : 5.0F;
                    d2d_context_->DrawLine(
                        D2D1::Point2F(x, extent - length),
                        D2D1::Point2F(x, extent),
                        ticks.Get());
                }
            }
        }
        if (y_count <= kMaximumOverlayLines) {
            for (std::uint64_t index = 0U; index < y_count; ++index) {
                const float y = device_y(static_cast<double>(index) * step);
                if (y >= extent && y <= size.height) {
                    const float length = index % 5U == 0U ? 10.0F : 5.0F;
                    d2d_context_->DrawLine(
                        D2D1::Point2F(extent - length, y),
                        D2D1::Point2F(extent, y),
                        ticks.Get());
                }
            }
        }
        return S_OK;
    }

    void TrimTileCache() noexcept {
        while (GpuTileBytes() > tile_budget_bytes_) {
            if (!EvictOldestInactive()) {
                break;
            }
        }
    }

    HRESULT PrepareTileCache() noexcept {
        pending_upload_bytes_ = 0U;
        candidate_active_bytes_ = 0U;
        source_retention_enabled_ = false;
        if (!d2d_context_) {
            return E_UNEXPECTED;
        }
        if (snapshot_ == nullptr) {
            tile_cache_.clear();
            ClearRetainedSources();
            gpu_tile_bytes_ = 0U;
            return S_OK;
        }
        if (snapshot_view_.tile_count > kMaximumSnapshotTiles
            || (snapshot_view_.feature_flags
                    & ~(INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE
                        | INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
                        | INKPOD_SNAPSHOT_FEATURE_SOLID_WHITE_BASE))
                != 0U
            || (snapshot_view_.feature_flags
                    & INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE)
                    != 0U
                && (snapshot_view_.feature_flags
                        & INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA)
                    != 0U
            || snapshot_view_.tile_stride_bytes < sizeof(InkpodSnapshotTile)
            || (snapshot_view_.tile_count != 0U && snapshot_view_.tiles == nullptr)
            || snapshot_view_.tile_stride_bytes
                > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())) {
            return E_INVALIDARG;
        }
        try {
            const auto* base = reinterpret_cast<const std::uint8_t*>(snapshot_view_.tiles);
            const std::size_t stride = static_cast<std::size_t>(
                snapshot_view_.tile_stride_bytes);
            std::uint64_t active_bytes{};
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
                if (tile->pixel_bytes > tile_budget_bytes_
                    || active_bytes > tile_budget_bytes_ - tile->pixel_bytes) {
                    return E_OUTOFMEMORY;
                }
                active_bytes += tile->pixel_bytes;
            }
            candidate_active_bytes_ = active_bytes;
            source_retention_enabled_ = active_source_ && active_bytes != 0U
                && active_bytes <= kSequenceGpuCacheBudgetBytes;
            for (auto& entry : tile_cache_) {
                entry.second.active = false;
            }
            for (std::uint64_t index = 0; index < snapshot_view_.tile_count; ++index) {
                const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                    base + static_cast<std::size_t>(index) * stride);
                auto existing = tile_cache_.find(tile->tile_id);
                if (existing != tile_cache_.end()
                    && existing->second.revision == tile->tile_revision
                    && existing->second.width == tile->width
                    && existing->second.height == tile->height) {
                    existing->second.origin_x = tile->origin_x;
                    existing->second.origin_y = tile->origin_y;
                    existing->second.active = true;
                    existing->second.last_used =
                        gApplicationTileUseSequence.fetch_add(1U, std::memory_order_relaxed) + 1U;
                } else {
                    pending_upload_bytes_ = SaturatingAdd(
                        pending_upload_bytes_, tile->pixel_bytes);
                }
            }
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    HRESULT UploadPreparedTiles() noexcept {
        if (!d2d_context_) {
            return E_UNEXPECTED;
        }
        if (pending_upload_bytes_ > tile_budget_bytes_) {
            return E_OUTOFMEMORY;
        }
        // Normal snapshot submission reserves capacity across all surfaces
        // before reaching here. Recovery also applies the local hard bound.
        while (GpuTileBytes() > tile_budget_bytes_ - pending_upload_bytes_) {
            if (!EvictOldestInactive()) {
                return E_OUTOFMEMORY;
            }
        }
        try {
            const auto* base = reinterpret_cast<const std::uint8_t*>(snapshot_view_.tiles);
            const std::size_t stride = static_cast<std::size_t>(
                snapshot_view_.tile_stride_bytes);
            for (std::uint64_t index = 0; index < snapshot_view_.tile_count; ++index) {
                const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                    base + static_cast<std::size_t>(index) * stride);
                auto existing = tile_cache_.find(tile->tile_id);
                if (existing != tile_cache_.end() && existing->second.active) {
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
                uploaded_tile_count_ = SaturatingAdd(uploaded_tile_count_, 1U);
                uploaded_tile_bytes_ = SaturatingAdd(uploaded_tile_bytes_, tile->pixel_bytes);
                CachedTile cache_entry{};
                cache_entry.revision = tile->tile_revision;
                cache_entry.last_used =
                    gApplicationTileUseSequence.fetch_add(1U, std::memory_order_relaxed) + 1U;
                cache_entry.byte_count = tile->pixel_bytes;
                cache_entry.origin_x = tile->origin_x;
                cache_entry.origin_y = tile->origin_y;
                cache_entry.width = tile->width;
                cache_entry.height = tile->height;
                cache_entry.active = true;
                cache_entry.bitmap = std::move(bitmap);
                if (existing != tile_cache_.end()) {
                    gpu_tile_bytes_ -= existing->second.byte_count;
                }
                tile_cache_[tile->tile_id] = std::move(cache_entry);
                gpu_tile_bytes_ += tile->pixel_bytes;
                pending_upload_bytes_ -= tile->pixel_bytes;
            }
            TrimTileCache();
            snapshot_ready_ = true;
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    HRESULT RebuildTileCache() noexcept {
        const HRESULT prepared = PrepareTileCache();
        if (FAILED(prepared)) {
            return prepared;
        }
        const HRESULT active = UploadPreparedTiles();
        return SUCCEEDED(active) ? UploadPreparedSequenceSources() : active;
    }

    HRESULT CreateSurfaceResources() noexcept {
        DiscardSurfaceResources();
        if (shared_.D3dDevice() == nullptr || shared_.DxgiFactory() == nullptr
            || shared_.D2dDevice() == nullptr) {
            return E_UNEXPECTED;
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
        HRESULT result = shared_.DxgiFactory()->CreateSwapChainForHwnd(
            shared_.D3dDevice(),
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
        result = shared_.DxgiFactory()->MakeWindowAssociation(
            owner_window_, DXGI_MWA_NO_ALT_ENTER);
        if (FAILED(result)) {
            return result;
        }
        result = shared_.D2dDevice()->CreateDeviceContext(
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
            const D2D1_SIZE_U size = target_bitmap_->GetPixelSize();
            surface_width_ = size.width;
            surface_height_ = size.height;
        }
        return result;
    }

    void DiscardSurfaceResources() noexcept {
        frame_latency_permit_ = false;
        if (frame_latency_waitable_ != nullptr) {
            CloseHandle(frame_latency_waitable_);
            frame_latency_waitable_ = nullptr;
        }
        tile_cache_.clear();
        ClearRetainedSources();
        snapshot_ready_ = snapshot_ == nullptr;
        source_retention_enabled_ = false;
        pending_upload_bytes_ = 0U;
        candidate_active_bytes_ = 0U;
        last_presented_document_revision_ = 0U;
        last_presented_presentation_epoch_ = 0U;
        last_presented_view_revision_ = 0U;
        last_presented_source_ = {};
        gpu_tile_bytes_ = 0U;
        last_snapshot_submission_qpc_ = 0U;
        first_presented_revision_qpc_ = 0U;
        first_frame_ready_qpc_ = 0U;
        first_present_begin_qpc_ = 0U;
        has_presented_snapshot_ = false;
        if (d2d_context_) {
            d2d_context_->SetTarget(nullptr);
        }
        target_bitmap_.Reset();
        d2d_context_.Reset();
        swap_chain_.Reset();
        surface_width_ = 0U;
        surface_height_ = 0U;
    }

    HWND window_{};
    HWND owner_window_{};
    SharedRendererDevice& shared_;
    InkpodSnapshot* snapshot_{};
    std::uint64_t snapshot_document_revision_{};
    std::uint64_t snapshot_presentation_epoch_{};
    bool snapshot_ready_{true};
    InkpodSnapshotView snapshot_view_{};
    InkpodSnapshotTransform transform_{};
    InkpodSnapshotOverlay overlay_{};
    InkpodSnapshotShootingFrameView shooting_frames_{};
    InkpodSnapshotRenderPlan render_plan_{};
    CanvasFloatingPreview floating_preview_{};
    CanvasGeometryPreview geometry_preview_{};
    TileCache tile_cache_;
    std::array<CachedSequenceSource, kMaximumSequenceCacheSources> retained_sources_;
    SequenceSourceKey active_source_;
    bool source_retention_enabled_{};
    std::uint64_t active_source_last_used_{};
    std::uint64_t candidate_active_bytes_{};
    std::uint64_t pending_upload_bytes_{};
    std::uint64_t sequence_cache_eviction_count_{};
    std::uint64_t uploaded_tile_count_{};
    std::uint64_t uploaded_tile_bytes_{};
    std::uint64_t last_presented_document_revision_{};
    std::uint64_t last_presented_presentation_epoch_{};
    std::uint64_t last_presented_view_revision_{};
    InkpodSnapshotSourceIdentity last_presented_source_{};
    std::uint64_t last_snapshot_submission_qpc_{};
    std::uint64_t first_presented_revision_qpc_{};
    std::uint64_t first_frame_ready_qpc_{};
    std::uint64_t first_present_begin_qpc_{};
    std::uint64_t frame_latency_timeout_count_{};
    bool has_presented_snapshot_{};
    ComPtr<IDXGISwapChain1> swap_chain_;
    ComPtr<ID2D1DeviceContext> d2d_context_;
    ComPtr<ID2D1Bitmap1> target_bitmap_;
    HANDLE frame_latency_waitable_{};
    // Bound to the swap chain, so ResizeBuffers (same waitable object) keeps
    // an acquired permit. Recreating or discarding the chain invalidates it.
    bool frame_latency_permit_{};
    std::uint64_t tile_budget_bytes_{kApplicationGpuTileBudgetBytes};
    std::uint64_t retained_snapshot_bytes_{};
    std::uint64_t gpu_tile_bytes_{};
    UINT surface_width_{};
    UINT surface_height_{};
};

enum class HostControlKind {
    Register,
    Unregister,
    Bind,
    Unbind,
    Resize,
    Visibility,
    Render,
    DpiChanged,
    SimulateDeviceLoss,
    ReadPixel,
    GetDocumentBounds,
    GetGeometryPreview,
    SetFloatingPreview,
    SetGeometryPreview,
};

struct HostControl {
    HostControlKind kind{};
    app::CanvasId canvas{};
    app::Generation surface_generation{};
    SnapshotRoute route{};
    HWND window{};
    HWND owner_window{};
    UINT width{};
    UINT height{};
    bool visible{};
    CanvasDocumentBounds* out_bounds{};
    CanvasPixelRgba8* out_pixel{};
    UINT pixel_x{};
    UINT pixel_y{};
    CanvasGeometryPreview* out_geometry_preview{};
    CanvasFloatingPreview floating_preview{};
    CanvasGeometryPreview geometry_preview{};
    std::shared_ptr<std::promise<HRESULT>> completion;
};

using HostWork = std::variant<HostControl, SnapshotEnvelope>;

struct SurfaceRecord {
    app::CanvasId canvas{};
    app::Generation generation{};
    HWND window{};
    HWND owner_window{};
    SnapshotRoute route{};
    bool visible{true};
    bool occluded{};
    std::uint64_t presented_frames{};
    std::unique_ptr<CanvasSurface> surface;
    HRESULT last_render_result{S_OK};
    std::size_t pending_render_requests{};
    bool presentation_pending{};
    ULONGLONG occlusion_retry_deadline_ms{};
    ULONGLONG next_occlusion_retry_ms{};
};

struct PublishedSurface {
    app::CanvasId canvas{};
    app::Generation generation{};
    SnapshotRoute route{};
    bool visible{};
    bool occluded{};
    std::uint64_t presented_frames{};
    std::uint64_t retained_snapshot_bytes{};
    std::uint64_t gpu_tile_bytes{};
    std::uint64_t swap_chain_bytes{};
    std::uint64_t cached_tile_count{};
    std::uint64_t active_tile_count{};
    std::uint64_t sequence_cache_source_count{};
    std::uint64_t sequence_cache_bytes{};
    std::uint64_t sequence_cache_eviction_count{};
    std::uint64_t uploaded_tile_count{};
    std::uint64_t uploaded_tile_bytes{};
    std::uint64_t last_presented_document_revision{};
    std::uint64_t last_presented_view_revision{};
    InkpodSnapshotSourceIdentity last_presented_source{};
    std::uint64_t last_snapshot_submission_qpc{};
    std::uint64_t first_presented_revision_qpc{};
    std::uint64_t last_presented_presentation_epoch{};
    std::uint64_t frame_latency_timeout_count{};
    HRESULT last_render_result{S_OK};
    bool visibility_pending{};
    std::uint64_t first_frame_ready_qpc{};
    std::uint64_t first_present_begin_qpc{};
};

class RendererHostState final {
public:
    ~RendererHostState() {
        Stop();
    }

    HRESULT Start() noexcept {
        try {
            auto ready = std::make_shared<std::promise<HRESULT>>();
            auto future = ready->get_future();
            {
                std::lock_guard lock(mutex_);
                if (worker_.joinable()) {
                    return E_UNEXPECTED;
                }
                work_event_ = CreateEventW(nullptr, TRUE, FALSE, nullptr);
                if (work_event_ == nullptr) {
                    return HRESULT_FROM_WIN32(GetLastError());
                }
                stopping_ = false;
                running_ = true;
                in_flight_work_ = 0U;
                pending_presentation_work_ = 0U;
            }
            worker_ = std::thread([this, ready] { Run(ready); });
            const HRESULT result = future.get();
            if (FAILED(result) && worker_.joinable()) {
                worker_.join();
                std::lock_guard lock(mutex_);
                CloseHandle(std::exchange(work_event_, nullptr));
            }
            return result;
        } catch (const std::system_error&) {
            Stop();
            return E_FAIL;
        } catch (const std::future_error&) {
            Stop();
            return E_FAIL;
        } catch (const std::bad_alloc&) {
            Stop();
            return E_OUTOFMEMORY;
        }
    }

    void Stop() noexcept {
        {
            std::lock_guard lock(mutex_);
            if (!worker_.joinable()) {
                running_ = false;
                stopping_ = true;
                if (work_event_ != nullptr) {
                    CloseHandle(std::exchange(work_event_, nullptr));
                }
                return;
            }
            stopping_ = true;
            SignalWorkLocked();
        }
        wake_.notify_one();
        queue_idle_.notify_all();
        worker_.join();
        std::lock_guard lock(mutex_);
        running_ = false;
        published_.clear();
        pending_presentation_work_ = 0U;
        if (work_event_ != nullptr) {
            CloseHandle(std::exchange(work_event_, nullptr));
        }
    }

    HRESULT Invoke(HostControl control) noexcept {
        SnapshotEnvelope discarded{};
        try {
            auto completion = std::make_shared<std::promise<HRESULT>>();
            auto future = completion->get_future();
            control.completion = completion;
            {
                std::lock_guard lock(mutex_);
                if (!running_ || stopping_ || OutstandingWorkLocked() >= kMaximumHostWork) {
                    return E_UNEXPECTED;
                }
                const bool supersedes_surface =
                    control.kind == HostControlKind::Bind
                    || control.kind == HostControlKind::Unbind
                    || control.kind == HostControlKind::Unregister;
                if (supersedes_surface) {
                    const auto pending = std::find_if(
                        work_.begin(), work_.end(), [&control](HostWork& item) {
                            const auto* snapshot = std::get_if<SnapshotEnvelope>(&item);
                            return snapshot != nullptr
                                && snapshot->route.canvas == control.canvas
                                && snapshot->route.surface_generation
                                    == control.surface_generation;
                        });
                    if (pending != work_.end()) {
                        discarded = std::exchange(
                            std::get<SnapshotEnvelope>(*pending), {});
                        work_.erase(pending);
                    }
                    work_.emplace_front(std::move(control));
                } else {
                    work_.emplace_back(std::move(control));
                }
                SignalWorkLocked();
            }
            ReleaseEnvelope(discarded);
            wake_.notify_one();
            return future.get();
        } catch (const std::future_error&) {
            ReleaseEnvelope(discarded);
            return E_FAIL;
        } catch (const std::bad_alloc&) {
            ReleaseEnvelope(discarded);
            return E_OUTOFMEMORY;
        }
    }

    void Post(HostControl control) noexcept {
        if (control.kind == HostControlKind::Visibility) {
            {
                std::lock_guard lock(mutex_);
                const auto published = FindPublishedLocked(control.canvas, control.surface_generation);
                if (!running_ || stopping_ || published == published_.end()) {
                    return;
                }
                if (published->visible == control.visible
                    && (!control.visible || !published->occluded)) {
                    return;
                }
                // This bounded per-surface mailbox cannot be displaced by a
                // saturated render queue. The latest requested visibility wins.
                if (control.visible) {
                    published->visible = true;
                } else {
                    published->visible = false;
                }
                published->visibility_pending = true;
                SignalWorkLocked();
            }
            wake_.notify_one();
            return;
        }
        try {
            {
                std::lock_guard lock(mutex_);
                if (!running_ || stopping_
                    || OutstandingWorkLocked() >= kMaximumNoncriticalHostWork) {
                    return;
                }
                work_.emplace_back(std::move(control));
                SignalWorkLocked();
            }
            wake_.notify_one();
        } catch (const std::bad_alloc&) {
        }
    }

    bool Submit(SnapshotEnvelope envelope) noexcept {
        if (envelope.snapshot == nullptr || !envelope.route) {
            ReleaseEnvelope(envelope);
            queue_rejection_count_.fetch_add(1U, std::memory_order_relaxed);
            return false;
        }
        if (envelope.estimated_payload_bytes == 0U) {
            envelope.estimated_payload_bytes = EstimateSnapshotPayloadBytes(
                envelope.snapshot);
        }
        SnapshotEnvelope replaced{};
        bool accepted{};
        bool stale_rejected{};
        try {
            {
                std::lock_guard lock(mutex_);
                const auto status = FindPublishedLocked(envelope.route.canvas,
                    envelope.route.surface_generation);
                stale_rejected = running_ && !stopping_
                    && (status == published_.end() || status->route != envelope.route);
                if (!running_ || stopping_ || status == published_.end()
                    || status->route != envelope.route || !status->visible
                    || status->occluded) {
                    accepted = false;
                } else {
                    envelope.submission_qpc = PerformanceCounterTicks();
                    const auto pending = std::find_if(
                        work_.begin(), work_.end(), [&envelope](HostWork& item) {
                            const auto* snapshot = std::get_if<SnapshotEnvelope>(&item);
                            return snapshot != nullptr
                                && snapshot->route.canvas == envelope.route.canvas
                                && snapshot->route.surface_generation
                                    == envelope.route.surface_generation;
                        });
                    if (pending != work_.end()) {
                        replaced = std::exchange(
                            std::get<SnapshotEnvelope>(*pending), envelope);
                        queue_replacement_count_.fetch_add(
                            1U, std::memory_order_relaxed);
                        accepted = true;
                    } else if (OutstandingWorkLocked() < kMaximumNoncriticalHostWork) {
                        work_.emplace_back(envelope);
                        envelope.snapshot = nullptr;
                        accepted = true;
                    }
                }
                if (accepted) {
                    SignalWorkLocked();
                }
            }
        } catch (const std::bad_alloc&) {
            accepted = false;
        }
        ReleaseEnvelope(replaced);
        if (!accepted) {
            ReleaseEnvelope(envelope);
            queue_rejection_count_.fetch_add(1U, std::memory_order_relaxed);
            if (stale_rejected) {
                stale_snapshot_count_.fetch_add(1U, std::memory_order_relaxed);
            }
            return false;
        }
        wake_.notify_one();
        return true;
    }

    bool SurfaceAcceptsSnapshots(const SnapshotRoute& route) const noexcept {
        if (!route) {
            return false;
        }
        std::lock_guard lock(mutex_);
        const auto found = FindPublishedLocked(route.canvas, route.surface_generation);
        return running_ && !stopping_ && found != published_.cend()
            && found->route == route && found->visible && !found->occluded;
    }

    DWORD ThreadId() const noexcept {
        return thread_id_.load(std::memory_order_acquire);
    }

    std::uint64_t PresentedFrameCount(
        app::CanvasId canvas, app::Generation generation) const noexcept {
        std::lock_guard lock(mutex_);
        const auto found = FindPublishedLocked(canvas, generation);
        return found == published_.cend() ? 0U : found->presented_frames;
    }

    std::size_t SurfaceCount() const noexcept {
        std::lock_guard lock(mutex_);
        return published_.size();
    }

    std::uint64_t DeviceGeneration() const noexcept {
        return device_generation_.load(std::memory_order_acquire);
    }

    RendererResourceUsage ResourceUsage() const noexcept {
        RendererResourceUsage usage{};
        usage.gpu_tile_budget_bytes = kApplicationGpuTileBudgetBytes;
        std::lock_guard lock(mutex_);
        usage.surface_count = static_cast<std::uint64_t>(published_.size());
        usage.queued_work_count = static_cast<std::uint64_t>(OutstandingWorkLocked());
        for (const PublishedSurface& surface : published_) {
            usage.retained_snapshot_bytes = SaturatingAdd(
                usage.retained_snapshot_bytes, surface.retained_snapshot_bytes);
            usage.gpu_tile_bytes = SaturatingAdd(
                usage.gpu_tile_bytes, surface.gpu_tile_bytes);
            usage.swap_chain_bytes = SaturatingAdd(
                usage.swap_chain_bytes, surface.swap_chain_bytes);
            usage.cached_tile_count = SaturatingAdd(
                usage.cached_tile_count, surface.cached_tile_count);
            usage.active_tile_count = SaturatingAdd(
                usage.active_tile_count, surface.active_tile_count);
            usage.sequence_cache_source_count = SaturatingAdd(
                usage.sequence_cache_source_count, surface.sequence_cache_source_count);
            usage.sequence_cache_bytes = SaturatingAdd(
                usage.sequence_cache_bytes, surface.sequence_cache_bytes);
            usage.sequence_cache_eviction_count = SaturatingAdd(
                usage.sequence_cache_eviction_count, surface.sequence_cache_eviction_count);
            usage.uploaded_tile_count = SaturatingAdd(
                usage.uploaded_tile_count, surface.uploaded_tile_count);
            usage.uploaded_tile_bytes = SaturatingAdd(
                usage.uploaded_tile_bytes, surface.uploaded_tile_bytes);
            usage.visible_surface_count += surface.visible && !surface.occluded ? 1U : 0U;
        }
        for (const HostWork& item : work_) {
            const auto* envelope = std::get_if<SnapshotEnvelope>(&item);
            if (envelope != nullptr) {
                usage.pending_snapshot_bytes = SaturatingAdd(
                    usage.pending_snapshot_bytes, envelope->estimated_payload_bytes);
            }
        }
        usage.queue_rejection_count = queue_rejection_count_.load(
            std::memory_order_relaxed);
        usage.queue_replacement_count = queue_replacement_count_.load(
            std::memory_order_relaxed);
        usage.stale_snapshot_count = stale_snapshot_count_.load(
            std::memory_order_relaxed);
        usage.resource_limit_count = resource_limit_count_.load(
            std::memory_order_relaxed);
        usage.device_reset_count = device_reset_count_.load(
            std::memory_order_relaxed);
        return usage;
    }

    bool GetSurfaceResourceUsage(
        app::CanvasId canvas,
        app::Generation surface_generation,
        RendererSurfaceResourceUsage& usage) const noexcept {
        std::lock_guard lock(mutex_);
        const auto found = FindPublishedLocked(canvas, surface_generation);
        if (found == published_.end()) {
            return false;
        }
        usage = RendererSurfaceResourceUsage{
            found->route,
            found->retained_snapshot_bytes,
            found->gpu_tile_bytes,
            found->swap_chain_bytes,
            found->cached_tile_count,
            found->active_tile_count,
            found->visible,
            found->occluded,
            found->sequence_cache_source_count,
            found->sequence_cache_bytes,
            found->sequence_cache_eviction_count,
            found->uploaded_tile_count,
            found->uploaded_tile_bytes,
            found->last_presented_document_revision,
            found->last_presented_view_revision,
            found->last_presented_source,
            found->last_snapshot_submission_qpc,
            found->first_presented_revision_qpc,
            found->last_presented_presentation_epoch,
            found->frame_latency_timeout_count,
            found->last_render_result,
            found->first_frame_ready_qpc,
            found->first_present_begin_qpc};
        return true;
    }

    void SetQueuePausedForSmokeTest(bool paused) noexcept {
        {
            std::lock_guard lock(mutex_);
            queue_paused_for_smoke_test_ = paused;
            SignalWorkLocked();
        }
        wake_.notify_one();
    }

    bool WaitQueueIdleForSmokeTest() noexcept {
        std::unique_lock lock(mutex_);
        if (!running_ || stopping_ || queue_paused_for_smoke_test_) {
            return false;
        }
        queue_idle_.wait(lock, [this] {
            return stopping_ || (work_.empty() && in_flight_work_ == 0U
                && pending_presentation_work_ == 0U && !HasPendingVisibilityLocked());
        });
        return !stopping_;
    }

private:
    static constexpr std::size_t kMaximumHostWork = 256U;
    static constexpr std::size_t kReservedHostControlWork = 8U;
    static constexpr std::size_t kMaximumNoncriticalHostWork =
        kMaximumHostWork - kReservedHostControlWork;

    [[nodiscard]] std::size_t OutstandingWorkLocked() const noexcept {
        return work_.size() + pending_presentation_work_ + (in_flight_queue_reservation_ ? 1U : 0U);
    }

    void SignalWorkLocked() const noexcept {
        if (work_event_ != nullptr) {
            SetEvent(work_event_);
        }
    }

    void SetPresentationWork(
        SurfaceRecord& surface, std::size_t requests, bool pending) noexcept {
        std::lock_guard lock(mutex_);
        SetPresentationWorkLocked(surface, requests, pending);
    }

    void SetPresentationWorkLocked(
        SurfaceRecord& surface, std::size_t requests, bool pending) noexcept {
        const auto previous = std::max<std::size_t>(
            surface.pending_render_requests, surface.presentation_pending ? 1U : 0U);
        const auto next = std::max<std::size_t>(requests, pending ? 1U : 0U);
        if (next > previous && in_flight_queue_reservation_) {
            // Move the accepted item's slot to its deferred presentation. A
            // producer cannot claim the slot between queue pop and this move.
            in_flight_queue_reservation_ = false;
        }
        pending_presentation_work_ -= previous;
        surface.pending_render_requests = requests;
        surface.presentation_pending = pending;
        pending_presentation_work_ += next;
    }

    bool HasPendingVisibilityLocked() const noexcept {
        return std::any_of(published_.begin(), published_.end(),
            [](const PublishedSurface& surface) { return surface.visibility_pending; });
    }

    void ApplyPendingVisibilityLocked() noexcept {
        for (auto& surface : surfaces_) {
            const auto state = FindPublishedLocked(surface.canvas, surface.generation);
            if (state == published_.end() || !state->visibility_pending) {
                continue;
            }
            surface.visible = state->visible;
            if (!surface.visible) {
                ResetOcclusionRetry(surface);
                SetPresentationWorkLocked(surface, 0U, false);
            } else {
                surface.occluded = false;
                ResetOcclusionRetry(surface);
                state->occluded = false;
                if (!surface.presentation_pending && surface.pending_render_requests == 0U
                    && OutstandingWorkLocked() >= kMaximumHostWork) {
                    // Visibility is already correct. Keep the show mailbox
                    // until a presentation slot becomes available.
                    continue;
                }
                SetPresentationWorkLocked(surface, surface.pending_render_requests, true);
            }
            state->visibility_pending = false;
        }
    }

    static void ReleaseEnvelope(SnapshotEnvelope& envelope) noexcept {
        if (envelope.snapshot != nullptr) {
            inkpod_snapshot_release(&envelope.snapshot);
        }
    }

    static bool IsDeviceLoss(HRESULT result) noexcept {
        return result == D2DERR_RECREATE_TARGET
            || result == DXGI_ERROR_DEVICE_HUNG
            || result == DXGI_ERROR_DEVICE_REMOVED
            || result == DXGI_ERROR_DEVICE_RESET
            || result == DXGI_ERROR_DRIVER_INTERNAL_ERROR;
    }

    auto FindPublishedLocked(app::CanvasId canvas, app::Generation generation) noexcept {
        return std::find_if(published_.begin(), published_.end(),
            [canvas, generation](const PublishedSurface& surface) {
                return surface.canvas == canvas && surface.generation == generation;
            });
    }

    auto FindPublishedLocked(
        app::CanvasId canvas, app::Generation generation) const noexcept {
        return std::find_if(published_.cbegin(), published_.cend(),
            [canvas, generation](const PublishedSurface& surface) {
                return surface.canvas == canvas && surface.generation == generation;
            });
    }

    auto FindSurface(app::CanvasId canvas, app::Generation generation) noexcept {
        return std::find_if(surfaces_.begin(), surfaces_.end(),
            [canvas, generation](const SurfaceRecord& surface) {
                return surface.canvas == canvas && surface.generation == generation;
            });
    }

    void PublishSurface(const SurfaceRecord& surface) noexcept {
        std::lock_guard lock(mutex_);
        const auto found = FindPublishedLocked(surface.canvas, surface.generation);
        const PublishedSurface value{
            surface.canvas,
            surface.generation,
            surface.route,
            found != published_.end() && found->visibility_pending ? found->visible : surface.visible,
            surface.occluded,
            surface.presented_frames,
            surface.surface->RetainedSnapshotBytes(),
            surface.surface->GpuTileBytes(),
            surface.surface->SwapChainBytes(),
            surface.surface->CachedTileCount(),
            surface.surface->ActiveTileCount(),
            surface.surface->SequenceCacheSourceCount(),
            surface.surface->SequenceCacheBytes(),
            surface.surface->SequenceCacheEvictionCount(),
            surface.surface->UploadedTileCount(),
            surface.surface->UploadedTileBytes(),
            surface.surface->LastPresentedDocumentRevision(),
            surface.surface->LastPresentedViewRevision(),
            surface.surface->LastPresentedSource(),
            surface.surface->LastSnapshotSubmissionQpc(),
            surface.surface->FirstPresentedRevisionQpc(),
            surface.surface->LastPresentedPresentationEpoch(),
            surface.surface->FrameLatencyTimeoutCount(),
            surface.last_render_result,
            found != published_.end() && found->visibility_pending,
            surface.surface->FirstFrameReadyQpc(),
            surface.surface->FirstPresentBeginQpc()};
        if (found == published_.end()) {
            try {
                published_.push_back(value);
            } catch (const std::bad_alloc&) {
            }
        } else {
            *found = value;
        }
    }

    void RemovePublished(app::CanvasId canvas, app::Generation generation) noexcept {
        std::lock_guard lock(mutex_);
        const auto found = FindPublishedLocked(canvas, generation);
        if (found != published_.end()) {
            published_.erase(found);
        }
    }

    void UpdateSequenceCacheBudgets() noexcept {
        for (;;) {
            std::uint64_t source_count{};
            std::uint64_t source_bytes{};
            for (const auto& surface : surfaces_) {
                source_count = SaturatingAdd(
                    source_count, surface.surface->SequenceCacheSourceCount());
                source_bytes = SaturatingAdd(
                    source_bytes, surface.surface->SequenceCacheBytes());
            }
            if (source_count <= kMaximumSequenceCacheSources
                && source_bytes <= kSequenceGpuCacheBudgetBytes) {
                return;
            }
            auto victim = surfaces_.end();
            std::uint64_t oldest = UINT64_MAX;
            for (auto iterator = surfaces_.begin(); iterator != surfaces_.end(); ++iterator) {
                const auto candidate = iterator->surface->OldestRetainedSourceUse();
                if (candidate.has_value() && candidate.value() < oldest) {
                    oldest = candidate.value();
                    victim = iterator;
                }
            }
            if (victim != surfaces_.end()) {
                (void)victim->surface->EvictOldestRetainedSource();
                continue;
            }
            // All remaining candidates are displayed by active views. Leave
            // their pixels resident under the normal GPU cap, but stop retaining
            // the least recently used source when its view next switches away.
            for (auto iterator = surfaces_.begin(); iterator != surfaces_.end(); ++iterator) {
                const auto candidate = iterator->surface->ActiveCachedSourceUse();
                if (candidate.has_value() && candidate.value() < oldest) {
                    oldest = candidate.value();
                    victim = iterator;
                }
            }
            if (victim == surfaces_.end()) {
                return;
            }
            victim->surface->DisableSourceRetention();
        }
    }

    bool ReserveGpuTileCapacity() noexcept {
        for (;;) {
            std::uint64_t total_bytes{};
            for (const auto& surface : surfaces_) {
                total_bytes = SaturatingAdd(
                    total_bytes, surface.surface->GpuTileBytes());
                total_bytes = SaturatingAdd(
                    total_bytes, surface.surface->PendingUploadBytes());
            }
            if (total_bytes <= kApplicationGpuTileBudgetBytes) {
                return true;
            }
            auto victim = surfaces_.end();
            std::uint64_t oldest = UINT64_MAX;
            for (auto iterator = surfaces_.begin(); iterator != surfaces_.end(); ++iterator) {
                const auto candidate = iterator->surface->OldestInactiveUse();
                if (candidate.has_value() && candidate.value() < oldest) {
                    oldest = candidate.value();
                    victim = iterator;
                }
            }
            if (victim == surfaces_.end() || !victim->surface->EvictOldestInactive()) {
                return false;
            }
        }
    }

    void UpdateTileBudgets() noexcept {
        for (auto& surface : surfaces_) {
            surface.surface->SetTileBudgetBytes(kApplicationGpuTileBudgetBytes);
        }
        UpdateSequenceCacheBudgets();
        (void)ReserveGpuTileCapacity();
    }

    HRESULT RecoverDevice() noexcept {
        for (auto& surface : surfaces_) {
            surface.surface->DiscardForDeviceReset();
        }
        const HRESULT initialize = shared_.Initialize();
        if (FAILED(initialize)) {
            return initialize;
        }
        device_generation_.store(shared_.Generation(), std::memory_order_release);
        for (auto& surface : surfaces_) {
            const HRESULT result = surface.surface->RecreateAfterSharedDeviceReset();
            if (FAILED(result)) {
                return result;
            }
            surface.occluded = false;
            ResetOcclusionRetry(surface);
        }
        UpdateTileBudgets();
        for (const auto& surface : surfaces_) {
            PublishSurface(surface);
        }
        device_reset_count_.fetch_add(1U, std::memory_order_relaxed);
        return S_OK;
    }

    static void ResetOcclusionRetry(SurfaceRecord& surface) noexcept {
        surface.occlusion_retry_deadline_ms = 0U;
        surface.next_occlusion_retry_ms = 0U;
    }

    HRESULT NormalizeResult(SurfaceRecord& surface, HRESULT result) noexcept {
        if (IsDeviceLoss(result)) {
            result = RecoverDevice();
        }
        if (result == DXGI_STATUS_OCCLUDED) {
            const ULONGLONG now = GetTickCount64();
            if (surface.visible && (surface.occlusion_retry_deadline_ms == 0U
                    || now < surface.occlusion_retry_deadline_ms)) {
                if (surface.occlusion_retry_deadline_ms == 0U) {
                    surface.occlusion_retry_deadline_ms =
                        now + kTransientOcclusionRetryMilliseconds;
                }
                surface.next_occlusion_retry_ms = now + kFrameLatencyRetryMilliseconds;
                // A newly shown swap chain can report OCCLUDED while DWM is
                // still publishing its final window state. Preserve the
                // accepted frame for a short, paced retry window instead of
                // permanently discarding it after the first Present.
                return S_FALSE;
            }
            ResetOcclusionRetry(surface);
            surface.occluded = true;
            PublishSurface(surface);
            return S_OK;
        }
        if (result == S_OK) {
            ResetOcclusionRetry(surface);
            if (surface.occluded) {
                surface.occluded = false;
                PublishSurface(surface);
            }
        } else if (FAILED(result)) {
            ResetOcclusionRetry(surface);
        }
        return result;
    }

    HRESULT RenderAndCount(SurfaceRecord& surface, DWORD wait_milliseconds = 0U) noexcept {
        HRESULT result = surface.surface->Render(
            nullptr, 0U, 0U, wait_milliseconds,
            wait_milliseconds == 0U ? nullptr : work_event_);
        surface.last_render_result = result;
        const bool presented = result == S_OK;
        const bool device_lost = IsDeviceLoss(result);
        result = NormalizeResult(surface, result);
        if (SUCCEEDED(result) && presented) {
            ++surface.presented_frames;
            SetPresentationWork(surface,
                surface.pending_render_requests == 0U ? 0U : surface.pending_render_requests - 1U,
                false);
        } else if (result == S_FALSE || (device_lost && SUCCEEDED(result))) {
            SetPresentationWork(surface, surface.pending_render_requests, true);
        } else {
            // Hidden/occluded surfaces and terminal failures do not keep the
            // owner thread spinning. A later show/recovery requests a frame.
            SetPresentationWork(surface, 0U, false);
        }
        PublishSurface(surface);
        return result;
    }

    SurfaceRecord* WaitForReadySurface() noexcept {
        std::array<HANDLE, MAXIMUM_WAIT_OBJECTS> handles{};
        std::array<std::size_t, MAXIMUM_WAIT_OBJECTS> indices{};
        handles[0] = work_event_;
        DWORD count = 1U;
        bool overflow{};
        DWORD delayed_occlusion_retry = INFINITE;
        if (surfaces_.empty() || WaitForSingleObject(work_event_, 0U) == WAIT_OBJECT_0) {
            return nullptr;
        }
        const auto start = next_retry_surface_ % surfaces_.size();
        for (std::size_t offset = 0U; offset < surfaces_.size(); ++offset) {
            const auto index = (start + offset) % surfaces_.size();
            auto& surface = surfaces_[index];
            if (!surface.visible || surface.occluded) {
                if (surface.presentation_pending || surface.pending_render_requests != 0U) {
                    SetPresentationWork(surface, 0U, false);
                    PublishSurface(surface);
                }
                continue;
            }
            if (!surface.presentation_pending && surface.pending_render_requests == 0U) {
                continue;
            }
            const ULONGLONG now = GetTickCount64();
            if (surface.next_occlusion_retry_ms > now) {
                const ULONGLONG remaining = surface.next_occlusion_retry_ms - now;
                delayed_occlusion_retry = std::min<DWORD>(delayed_occlusion_retry,
                    static_cast<DWORD>(std::min<ULONGLONG>(remaining, MAXDWORD)));
                continue;
            }
            const HRESULT ready = surface.surface->AcquireFrameLatencyPermit(0U);
            if (ready == S_OK) {
                next_retry_surface_ = index + 1U;
                return WaitForSingleObject(work_event_, 0U) == WAIT_OBJECT_0 ? nullptr : &surface;
            }
            if (FAILED(ready)) {
                surface.last_render_result = ready;
                SetPresentationWork(surface, 0U, false);
                PublishSurface(surface);
                ReportFailure(surface, ready);
                return nullptr;
            }
            if (count < MAXIMUM_WAIT_OBJECTS) {
                handles[count] = surface.surface->FrameLatencyHandle();
                indices[count] = index;
                ++count;
            } else {
                overflow = true;
            }
        }
        if (count == 1U) {
            if (delayed_occlusion_retry != INFINITE) {
                (void)WaitForSingleObject(work_event_, delayed_occlusion_retry);
            }
            return nullptr;
        }
        // All normal visible surfaces fit one OS wait. A frame-latency handle
        // is advisory scheduling capacity and can transiently remain unsignaled
        // across compositor transitions. Retry at a short interruptible cadence
        // so one missed notification cannot add a 100ms navigation tail. Larger
        // registries still rotate 1ms batches after probing every handle above.
        const DWORD normal_retry = overflow ? 1U : kFrameLatencyRetryMilliseconds;
        const DWORD milliseconds = delayed_occlusion_retry == INFINITE
            ? normal_retry : std::min(normal_retry, delayed_occlusion_retry);
        const DWORD wait = WaitForMultipleObjectsEx(count, handles.data(), FALSE, milliseconds, FALSE);
        if (wait >= WAIT_OBJECT_0 + 1U && wait < WAIT_OBJECT_0 + count) {
            const auto index = indices[wait - WAIT_OBJECT_0];
            auto& surface = surfaces_[index];
            surface.surface->AcceptFrameLatencySignal();
            next_retry_surface_ = index + 1U;
            return &surface;
        }
        if (wait == WAIT_TIMEOUT) {
            next_retry_surface_ = indices[count - 1U] + 1U;
            // A short retry expiry is normal pacing, not a 100ms frame-latency
            // failure. Explicit bounded Render calls still record their actual
            // timeout through AcquireFrameLatencyPermit.
        } else if (wait == WAIT_FAILED) {
            const HRESULT failure = HRESULT_FROM_WIN32(GetLastError());
            for (DWORD index = 1U; index < count; ++index) {
                auto& surface = surfaces_[indices[index]];
                surface.last_render_result = failure;
                SetPresentationWork(surface, 0U, false);
                PublishSurface(surface);
                ReportFailure(surface, failure);
            }
        }
        return nullptr;
    }

    void ReportFailure(const SurfaceRecord& surface, HRESULT result) const noexcept {
        PostMessageW(
            surface.owner_window,
            kCanvasRenderFailed,
            static_cast<WPARAM>(result),
            static_cast<LPARAM>(surface.generation.Value()));
    }

    HRESULT ProcessSnapshot(SnapshotEnvelope& envelope) noexcept {
        const auto found = FindSurface(
            envelope.route.canvas, envelope.route.surface_generation);
        if (found == surfaces_.end() || found->route != envelope.route
            || !found->visible || found->occluded) {
            ReleaseEnvelope(envelope);
            stale_snapshot_count_.fetch_add(1U, std::memory_order_relaxed);
            return S_FALSE;
        }
        InkpodSnapshotView view{};
        view.struct_size = sizeof(view);
        InkpodSnapshotTransform transform{};
        transform.struct_size = sizeof(transform);
        if (inkpod_snapshot_get_view(envelope.snapshot, &view) != INKPOD_STATUS_OK
            || inkpod_snapshot_get_transform(envelope.snapshot, &transform)
                != INKPOD_STATUS_OK
            || view.revision != envelope.document_revision
            || transform.view_revision != envelope.view_revision) {
            ReleaseEnvelope(envelope);
            stale_snapshot_count_.fetch_add(1U, std::memory_order_relaxed);
            return E_INVALIDARG;
        }
        std::uint64_t other_active_bytes{};
        for (const auto& surface : surfaces_) {
            if (&surface != &*found) {
                other_active_bytes = SaturatingAdd(
                    other_active_bytes, surface.surface->ActiveTileBytes());
            }
        }
        std::uint64_t candidate_tile_bytes{};
        if (view.tiles != nullptr && view.tile_stride_bytes >= sizeof(InkpodSnapshotTile)
            && view.tile_stride_bytes <= static_cast<std::uint64_t>(SIZE_MAX)) {
            const auto* base = reinterpret_cast<const std::uint8_t*>(view.tiles);
            const std::size_t stride = static_cast<std::size_t>(view.tile_stride_bytes);
            for (std::uint64_t index = 0U; index < view.tile_count; ++index) {
                if (index > static_cast<std::uint64_t>(SIZE_MAX / stride)) {
                    candidate_tile_bytes = UINT64_MAX;
                    break;
                }
                const auto* tile = reinterpret_cast<const InkpodSnapshotTile*>(
                    base + static_cast<std::size_t>(index) * stride);
                candidate_tile_bytes = SaturatingAdd(
                    candidate_tile_bytes, tile->pixel_bytes);
            }
        }
        if (other_active_bytes >= kApplicationGpuTileBudgetBytes
            || candidate_tile_bytes
                > kApplicationGpuTileBudgetBytes - other_active_bytes) {
            ReleaseEnvelope(envelope);
            resource_limit_count_.fetch_add(1U, std::memory_order_relaxed);
            return E_OUTOFMEMORY;
        }
        found->surface->SetTileBudgetBytes(
            kApplicationGpuTileBudgetBytes - other_active_bytes);
        HRESULT result = found->surface->SetSnapshot(
            envelope.snapshot, envelope.committed_document_revision, envelope.submission_qpc,
            envelope.presentation_epoch);
        envelope.snapshot = nullptr;
        if (SUCCEEDED(result)) {
            UpdateSequenceCacheBudgets();
            result = ReserveGpuTileCapacity()
                ? found->surface->UploadPreparedSnapshot() : E_OUTOFMEMORY;
            if (result == E_OUTOFMEMORY) {
                resource_limit_count_.fetch_add(1U, std::memory_order_relaxed);
            }
        }
        if (SUCCEEDED(result)) {
            std::uint64_t other_gpu_bytes{};
            for (const auto& surface : surfaces_) {
                if (&surface != &*found) {
                    other_gpu_bytes = SaturatingAdd(
                        other_gpu_bytes, surface.surface->GpuTileBytes());
                }
            }
            if (other_gpu_bytes < kApplicationGpuTileBudgetBytes) {
                found->surface->SetTileBudgetBytes(
                    kApplicationGpuTileBudgetBytes - other_gpu_bytes);
                result = found->surface->UploadPreparedSequenceSources();
            }
            UpdateSequenceCacheBudgets();
            if (!ReserveGpuTileCapacity()) {
                result = E_OUTOFMEMORY;
            }
            if (result == E_OUTOFMEMORY) {
                resource_limit_count_.fetch_add(1U, std::memory_order_relaxed);
            }
        }
        result = NormalizeResult(*found, result);
        UpdateTileBudgets();
        for (const auto& surface : surfaces_) {
            PublishSurface(surface);
        }
        if (SUCCEEDED(result) && found->visible) {
            result = RenderAndCount(*found);
        }
        return result;
    }

    HRESULT ProcessControl(HostControl& control) noexcept {
        if (control.kind == HostControlKind::Register) {
            if (!control.canvas || !control.surface_generation
                || control.window == nullptr || control.owner_window == nullptr
                || FindSurface(control.canvas, control.surface_generation) != surfaces_.end()) {
                return E_INVALIDARG;
            }
            try {
                SurfaceRecord surface{};
                surface.canvas = control.canvas;
                surface.generation = control.surface_generation;
                surface.window = control.window;
                surface.owner_window = control.owner_window;
                surface.surface = std::make_unique<CanvasSurface>(
                    control.window, control.owner_window, shared_);
                const HRESULT initialize = surface.surface->Initialize();
                if (FAILED(initialize)) {
                    return initialize;
                }
                surfaces_.push_back(std::move(surface));
                UpdateTileBudgets();
                for (const auto& current : surfaces_) {
                    PublishSurface(current);
                }
                return S_OK;
            } catch (const std::bad_alloc&) {
                return E_OUTOFMEMORY;
            }
        }
        const auto found = FindSurface(control.canvas, control.surface_generation);
        if (found == surfaces_.end()) {
            return E_INVALIDARG;
        }
        if (control.kind == HostControlKind::Unregister) {
            SetPresentationWork(*found, 0U, false);
            RemovePublished(found->canvas, found->generation);
            surfaces_.erase(found);
            UpdateTileBudgets();
            for (const auto& current : surfaces_) {
                PublishSurface(current);
            }
            return S_OK;
        }
        SurfaceRecord& surface = *found;
        HRESULT result = S_OK;
        bool render{};
        switch (control.kind) {
            case HostControlKind::Bind:
                if (!control.route || control.route.canvas != surface.canvas
                    || control.route.surface_generation != surface.generation) {
                    return E_INVALIDARG;
                }
                SetPresentationWork(surface, 0U, false);
                surface.surface->ClearSnapshot();
                surface.route = control.route;
                surface.occluded = false;
                ResetOcclusionRetry(surface);
                PublishSurface(surface);
                break;
            case HostControlKind::Unbind:
                SetPresentationWork(surface, 0U, false);
                surface.surface->ClearSnapshot();
                surface.route = {};
                surface.occluded = false;
                ResetOcclusionRetry(surface);
                PublishSurface(surface);
                render = surface.visible;
                break;
            case HostControlKind::Resize:
                result = surface.surface->Resize(control.width, control.height);
                render = result == S_OK && surface.visible;
                break;
            case HostControlKind::Visibility:
                surface.visible = control.visible;
                if (surface.visible) {
                    surface.occluded = false;
                    ResetOcclusionRetry(surface);
                    render = true;
                } else {
                    ResetOcclusionRetry(surface);
                    SetPresentationWork(surface, 0U, false);
                }
                PublishSurface(surface);
                break;
            case HostControlKind::Render:
                render = surface.visible;
                if (render) {
                    SetPresentationWork(surface, surface.pending_render_requests + 1U, true);
                }
                break;
            case HostControlKind::DpiChanged:
                result = surface.surface->DpiChanged();
                render = surface.visible;
                break;
            case HostControlKind::SimulateDeviceLoss:
                result = surface.surface->SimulateDeviceLossForSmokeTest();
                render = surface.visible;
                break;
            case HostControlKind::ReadPixel:
                if (control.out_pixel == nullptr) {
                    result = E_POINTER;
                } else {
                    result = surface.surface->RenderAndReadPixelForSmokeTest(
                        control.pixel_x, control.pixel_y, *control.out_pixel, work_event_);
                    surface.last_render_result = result;
                    if (result == S_OK) {
                        ++surface.presented_frames;
                        SetPresentationWork(surface, surface.pending_render_requests, false);
                    }
                    PublishSurface(surface);
                }
                break;
            case HostControlKind::GetDocumentBounds:
                if (control.out_bounds == nullptr) {
                    result = E_POINTER;
                } else {
                    *control.out_bounds = surface.surface->DocumentBounds();
                }
                break;
            case HostControlKind::GetGeometryPreview:
                if (control.out_geometry_preview == nullptr) {
                    result = E_POINTER;
                } else {
                    result = surface.surface->GetGeometryPreviewForSmokeTest(
                        *control.out_geometry_preview);
                }
                break;
            case HostControlKind::SetFloatingPreview:
                result = surface.surface->SetFloatingPreview(control.floating_preview);
                render = result == S_OK && surface.visible;
                break;
            case HostControlKind::SetGeometryPreview:
                result = surface.surface->SetGeometryPreview(control.geometry_preview);
                render = result == S_OK && surface.visible;
                break;
            case HostControlKind::Register:
            case HostControlKind::Unregister:
                break;
        }
        result = NormalizeResult(surface, result);
        if (SUCCEEDED(result) && render) {
            result = RenderAndCount(surface,
                control.kind == HostControlKind::Render && control.completion != nullptr ? 100U : 0U);
            if (result == S_FALSE && (control.kind == HostControlKind::SetGeometryPreview
                    || control.kind == HostControlKind::SetFloatingPreview)) {
                // State is applied; the renderer owns the pending frame. UI
                // pointer delivery does not synchronously wait for Present.
                result = S_OK;
            }
        }
        return result;
    }

    void AbortWork(std::deque<HostWork>& work) noexcept {
        for (auto& item : work) {
            if (auto* envelope = std::get_if<SnapshotEnvelope>(&item)) {
                ReleaseEnvelope(*envelope);
            } else {
                auto& control = std::get<HostControl>(item);
                if (control.completion != nullptr) {
                    control.completion->set_value(E_ABORT);
                }
            }
        }
    }

    void Run(const std::shared_ptr<std::promise<HRESULT>>& ready) noexcept {
        thread_id_.store(GetCurrentThreadId(), std::memory_order_release);
        const HRESULT initialize = shared_.Initialize();
        device_generation_.store(shared_.Generation(), std::memory_order_release);
        ready->set_value(initialize);
        if (FAILED(initialize)) {
            std::lock_guard lock(mutex_);
            running_ = false;
            stopping_ = true;
            return;
        }
        for (;;) {
            HostWork item;
            bool retry_present{};
            {
                std::unique_lock lock(mutex_);
                wake_.wait(lock, [this] {
                    return stopping_
                        || (!queue_paused_for_smoke_test_
                            && (!work_.empty() || pending_presentation_work_ != 0U
                                || HasPendingVisibilityLocked()));
                });
                if (stopping_) {
                    std::deque<HostWork> abandoned;
                    abandoned.swap(work_);
                    lock.unlock();
                    AbortWork(abandoned);
                    break;
                }
                ApplyPendingVisibilityLocked();
                if (work_.empty() && pending_presentation_work_ == 0U) {
                    queue_idle_.notify_all();
                    ResetEvent(work_event_);
                    continue;
                }
                retry_present = work_.empty();
                in_flight_queue_reservation_ = !retry_present;
                if (!retry_present) {
                    item = std::move(work_.front());
                    work_.pop_front();
                }
                if (work_.empty()) {
                    ResetEvent(work_event_);
                } else {
                    SignalWorkLocked();
                }
                ++in_flight_work_;
            }
            HRESULT result{};
            SurfaceRecord* failure_surface{};
            if (retry_present) {
                failure_surface = WaitForReadySurface();
                result = failure_surface == nullptr ? S_FALSE : RenderAndCount(*failure_surface);
            } else if (auto* envelope = std::get_if<SnapshotEnvelope>(&item)) {
                const auto found = FindSurface(
                    envelope->route.canvas, envelope->route.surface_generation);
                failure_surface = found == surfaces_.end() ? nullptr : &*found;
                result = ProcessSnapshot(*envelope);
            } else {
                auto& control = std::get<HostControl>(item);
                const auto found = FindSurface(control.canvas, control.surface_generation);
                failure_surface = found == surfaces_.end() ? nullptr : &*found;
                result = ProcessControl(control);
                if (control.completion != nullptr) {
                    control.completion->set_value(result);
                }
            }
            if (FAILED(result) && result != E_INVALIDARG && failure_surface != nullptr) {
                ReportFailure(*failure_surface, result);
            }
            {
                std::lock_guard lock(mutex_);
                in_flight_queue_reservation_ = false;
                --in_flight_work_;
                if (work_.empty() && in_flight_work_ == 0U && pending_presentation_work_ == 0U
                    && !HasPendingVisibilityLocked()) {
                    queue_idle_.notify_all();
                }
            }
        }
        surfaces_.clear();
        shared_.Discard();
        device_generation_.store(0U, std::memory_order_release);
        thread_id_.store(0U, std::memory_order_release);
    }

    mutable std::mutex mutex_;
    std::condition_variable wake_;
    std::condition_variable queue_idle_;
    std::thread worker_;
    HANDLE work_event_{};
    bool stopping_{true};
    bool running_{};
    bool queue_paused_for_smoke_test_{};
    std::size_t in_flight_work_{};
    std::size_t pending_presentation_work_{};
    bool in_flight_queue_reservation_{};
    std::size_t next_retry_surface_{};
    std::deque<HostWork> work_;
    std::vector<PublishedSurface> published_;
    std::vector<SurfaceRecord> surfaces_;
    SharedRendererDevice shared_;
    std::atomic<DWORD> thread_id_{};
    std::atomic<std::uint64_t> device_generation_{};
    std::atomic<std::uint64_t> queue_rejection_count_{};
    std::atomic<std::uint64_t> queue_replacement_count_{};
    std::atomic<std::uint64_t> stale_snapshot_count_{};
    std::atomic<std::uint64_t> resource_limit_count_{};
    std::atomic<std::uint64_t> device_reset_count_{};
};

class CanvasHost final : public CanvasSnapshotSink {
public:
    CanvasHost(
        HWND window,
        HWND owner_window,
        RendererHost& renderer,
        app::CanvasId canvas,
        app::Generation surface_generation) noexcept
        : window_(window),
          owner_window_(owner_window),
          renderer_(renderer),
          canvas_(canvas),
          surface_generation_(surface_generation) {}

    HRESULT Initialize() noexcept {
        const HRESULT result = renderer_.RegisterSurface(
            canvas_, surface_generation_, window_, owner_window_);
        if (SUCCEEDED(result)) {
            ResetScrollProjection();
            SynchronizeVisibility(
                IsWindowVisible(window_) != FALSE
                    && CanvasAncestorsVisible(window_),
                false);
        }
        return result;
    }

    ~CanvasHost() override {
        renderer_.UnregisterSurface(canvas_, surface_generation_);
    }

    bool Bind(
        app::DocumentSessionId document_session,
        app::DocumentViewId document_view,
        app::Generation document_generation) noexcept {
        const SnapshotRoute route{
            document_session,
            document_view,
            canvas_,
            document_generation,
            surface_generation_};
        if (FAILED(renderer_.BindSurface(route))) {
            return false;
        }
        {
            std::lock_guard lock(route_mutex_);
            route_ = route;
            sequence_activation_pending_ = false;
            required_presented_revision_ = 0U;
            required_presentation_epoch_ = 0U;
        }
        PrepareScrollBinding(route);
        return true;
    }

    bool BindAuxiliary(
        app::AuxiliarySourceId source,
        app::Generation generation) noexcept {
        const SnapshotRoute route{
            {},
            {},
            canvas_,
            generation,
            surface_generation_,
            SnapshotOwnerKind::Auxiliary,
            source};
        if (FAILED(renderer_.BindSurface(route))) {
            return false;
        }
        {
            std::lock_guard lock(route_mutex_);
            route_ = route;
            sequence_activation_pending_ = false;
            required_presented_revision_ = 0U;
            required_presentation_epoch_ = 0U;
        }
        PrepareScrollBinding(route);
        return true;
    }

    bool Unbind() noexcept {
        if (FAILED(renderer_.UnbindSurface(canvas_, surface_generation_))) {
            return false;
        }
        {
            std::lock_guard lock(route_mutex_);
            route_ = {};
            sequence_activation_pending_ = false;
            required_presented_revision_ = 0U;
            required_presentation_epoch_ = 0U;
        }
        ResetScrollProjection();
        return true;
    }

    SnapshotRoute Route() const noexcept override {
        std::lock_guard lock(route_mutex_);
        return route_;
    }

    bool AcceptsSnapshots() const noexcept override {
        const SnapshotRoute route = Route();
        return renderer_.SurfaceAcceptsSnapshots(route);
    }

    bool Submit(SnapshotEnvelope envelope) noexcept override {
        const SnapshotRoute route = Route();
        if (!route || envelope.route != route) {
            if (envelope.snapshot != nullptr) {
                inkpod_snapshot_release(&envelope.snapshot);
            }
            return false;
        }
        const InkpodSnapshotTransform transform = envelope.transform;
        const CanvasScrollRangeHint range_hint = envelope.scroll_range_hint;
        const std::uint64_t scroll_cause_token = envelope.scroll_cause_token;
        const bool accepted = renderer_.Submit(envelope);
        if (accepted) {
            QueueScrollProjection(
                route, transform, range_hint, scroll_cause_token);
        }
        return accepted;
    }

    void QueueScrollProjection(
        const SnapshotRoute& route,
        const InkpodSnapshotTransform& transform,
        CanvasScrollRangeHint range_hint,
        std::uint64_t scroll_cause_token) noexcept {
        std::uint64_t token{};
        {
            std::lock_guard lock(scroll_mailbox_mutex_);
            ++next_scroll_projection_token_;
            if (next_scroll_projection_token_ == 0U) {
                ++next_scroll_projection_token_;
            }
            token = next_scroll_projection_token_;
            last_renderer_accepted_scroll_route_ = route;
            if (pending_scroll_projection_.has_value()
                && pending_scroll_projection_->route == route
                && pending_scroll_projection_->range_hint
                    == CanvasScrollRangeHint::ResetToBase
                && range_hint == CanvasScrollRangeHint::Preserve) {
                // A later ordinary snapshot may replace renderer work before
                // the UI consumes the reset snapshot. Keep the cause latched
                // until one accepted projection reaches the owner thread.
                range_hint = CanvasScrollRangeHint::ResetToBase;
                scroll_cause_token =
                    pending_scroll_projection_->scroll_cause_token;
            }
            pending_scroll_projection_ = PendingScrollProjection{
                token, route, transform, range_hint, scroll_cause_token, 0U, 0U};
        }
        if (PostMessageW(
                window_,
                kCanvasScrollProjectionChanged,
                static_cast<WPARAM>(token),
                static_cast<LPARAM>(surface_generation_.Value())) == FALSE) {
            // A window timer is not subject to the posted-message quota. The
            // invalidation fallback still gives the owner thread a later paint
            // opportunity if timer allocation itself fails.
            if (SetTimer(
                    window_, kCanvasScrollProjectionRetryTimer, 16U, nullptr)
                == 0U) {
                (void)RedrawWindow(
                    window_, nullptr, nullptr, RDW_INVALIDATE | RDW_NOERASE);
            }
        }
    }

    bool ApplyQueuedScrollProjection(
        std::uint64_t token,
        app::Generation surface_generation) noexcept {
        if (token == 0U || surface_generation != surface_generation_
            || scroll_projection_apply_active_) {
            return false;
        }
        std::optional<PendingScrollProjection> pending;
        {
            std::lock_guard lock(scroll_mailbox_mutex_);
            if (pending_scroll_projection_.has_value()
                && pending_scroll_projection_->token == token) {
                pending = pending_scroll_projection_;
            }
        }
        if (!pending.has_value()) {
            WakeSupersedingScrollProjection(token);
            return false;
        }
        if (pending->route != Route()) {
            {
                std::lock_guard lock(scroll_mailbox_mutex_);
                if (pending_scroll_projection_.has_value()
                    && pending_scroll_projection_->token == token) {
                    pending_scroll_projection_.reset();
                }
            }
            WakeSupersedingScrollProjection(token);
            return false;
        }
        scroll_projection_apply_active_ = true;
        const bool applied = ApplyAcceptedScrollProjection(
            pending->route, pending->transform, pending->range_hint);
        scroll_projection_apply_active_ = false;
        if (!applied) {
            bool schedule_retry{};
            bool request_refresh{};
            {
                std::lock_guard lock(scroll_mailbox_mutex_);
                if (pending_scroll_projection_.has_value()
                    && pending_scroll_projection_->token == token) {
                    if (pending_scroll_projection_->apply_attempts
                        < kMaximumScrollProjectionApplyAttempts) {
                        ++pending_scroll_projection_->apply_attempts;
                        schedule_retry = true;
                    } else if (!scroll_projection_refresh_requested_) {
                        scroll_projection_refresh_requested_ = true;
                        scroll_projection_recovery_required_ = true;
                        request_refresh = true;
                    }
                }
            }
            if (schedule_retry) {
                if (SetTimer(
                        window_, kCanvasScrollProjectionRetryTimer, 16U, nullptr) == 0U) {
                    (void)RedrawWindow(
                        window_, nullptr, nullptr, RDW_INVALIDATE | RDW_NOERASE);
                }
            } else if (request_refresh) {
                if (!DeliverScrollProjectionViewportRefresh()) {
                    scroll_projection_refresh_requested_ = false;
                    bool retry_delivery{};
                    {
                        std::lock_guard lock(scroll_mailbox_mutex_);
                        if (pending_scroll_projection_.has_value()
                            && pending_scroll_projection_->token == token
                            && pending_scroll_projection_->refresh_delivery_attempts
                                < kMaximumScrollRefreshDeliveryAttempts) {
                            ++pending_scroll_projection_->refresh_delivery_attempts;
                            retry_delivery = true;
                        }
                    }
                    if (retry_delivery
                        && SetTimer(
                               window_, kCanvasScrollProjectionRetryTimer, 16U, nullptr)
                            == 0U) {
                            (void)RedrawWindow(
                                window_, nullptr, nullptr,
                                RDW_INVALIDATE | RDW_NOERASE);
                    }
                }
            }
            WakeSupersedingScrollProjection(token);
            return false;
        }
        KillTimer(window_, kCanvasScrollProjectionRetryTimer);
        {
            std::lock_guard lock(scroll_mailbox_mutex_);
            if (pending_scroll_projection_.has_value()
                && pending_scroll_projection_->token == token) {
                pending_scroll_projection_.reset();
            }
        }
        WakeSupersedingScrollProjection(token);
        return true;
    }

    void WakeSupersedingScrollProjection(std::uint64_t completed_token) noexcept {
        bool wake_required{};
        {
            std::lock_guard lock(scroll_mailbox_mutex_);
            wake_required = pending_scroll_projection_.has_value()
                && pending_scroll_projection_->token != completed_token;
        }
        if (wake_required
            && SetTimer(
                   window_, kCanvasScrollProjectionRetryTimer, 16U, nullptr)
                == 0U) {
            // This runs after the outer projection apply has left its reentrant
            // redraw guard, so a paint fallback cannot consume the only wakeup.
            (void)RedrawWindow(
                window_, nullptr, nullptr, RDW_INVALIDATE | RDW_NOERASE);
        }
    }

    bool DeliverScrollProjectionViewportRefresh() noexcept {
        RECT client{};
        const HWND parent = GetParent(window_);
        if (parent == nullptr || GetClientRect(window_, &client) == FALSE) {
            return false;
        }
        const WPARAM canvas = static_cast<WPARAM>(canvas_.Value());
        const LPARAM viewport = MAKELPARAM(
            client.right - client.left,
            client.bottom - client.top);
        if (PostMessageW(
                parent, kCanvasViewportChanged, canvas, viewport) != FALSE) {
            return true;
        }
        if (GetWindowThreadProcessId(parent, nullptr) != GetCurrentThreadId()) {
            return false;
        }
        // The Canvas and its pane parent share the UI owner thread. A same-thread
        // send is the finite fallback when the posted-message quota is exhausted.
        (void)SendMessageW(parent, kCanvasViewportChanged, canvas, viewport);
        return true;
    }

    void DrainScrollProjectionMailbox() noexcept {
        std::uint64_t token{};
        {
            std::lock_guard lock(scroll_mailbox_mutex_);
            if (pending_scroll_projection_.has_value()) {
                token = pending_scroll_projection_->token;
            }
        }
        if (token != 0U) {
            (void)ApplyQueuedScrollProjection(token, surface_generation_);
        }
    }

    void RefreshScrollProjectionForViewport() noexcept {
        if (scroll_projection_apply_active_
            || !has_scroll_transform_ || scroll_route_ != Route()) {
            return;
        }
        (void)ReprojectScrollbars(
            CanvasScrollRangeUpdate::Preserve,
            CanvasScrollRangeUpdate::Preserve);
    }

    bool HandleScroll(CanvasScrollAxis axis, UINT request_code) noexcept {
        if (request_code == SB_ENDSCROLL) {
            EndScrollInteraction(axis);
            return true;
        }
        const SnapshotRoute route = Route();
        if (!route || scroll_route_ != route
            || scroll_projection_recovery_required_
            || scroll_command_pending_
            || (requested_scroll_reset_route_.has_value()
                && requested_scroll_reset_route_.value() == route)) {
            return false;
        }
        const CanvasScrollProjection* projection = axis == CanvasScrollAxis::Horizontal
            ? (has_horizontal_scroll_ ? &horizontal_scroll_ : nullptr)
            : (has_vertical_scroll_ ? &vertical_scroll_ : nullptr);
        if (projection == nullptr) {
            return false;
        }

        CanvasScrollTargetRequest request{};
        switch (request_code) {
            case SB_LINELEFT:
                request.kind = CanvasScrollTargetKind::LineBackward;
                break;
            case SB_LINERIGHT:
                request.kind = CanvasScrollTargetKind::LineForward;
                break;
            case SB_PAGELEFT:
                request.kind = CanvasScrollTargetKind::PageBackward;
                break;
            case SB_PAGERIGHT:
                request.kind = CanvasScrollTargetKind::PageForward;
                break;
            case SB_LEFT:
                request.kind = CanvasScrollTargetKind::Start;
                break;
            case SB_RIGHT:
                request.kind = CanvasScrollTargetKind::End;
                break;
            case SB_THUMBTRACK:
            case SB_THUMBPOSITION: {
                SCROLLINFO info{};
                info.cbSize = sizeof(info);
                info.fMask = SIF_TRACKPOS;
                const int bar = axis == CanvasScrollAxis::Horizontal
                    ? SB_HORZ : SB_VERT;
                if (GetScrollInfo(window_, bar, &info) == FALSE) {
                    return false;
                }
                request.kind = CanvasScrollTargetKind::Thumb;
                request.thumb_position = info.nTrackPos;
                if (axis == CanvasScrollAxis::Horizontal) {
                    horizontal_scroll_tracking_ = true;
                } else {
                    vertical_scroll_tracking_ = true;
                }
                break;
            }
            default:
                return false;
        }

        if (axis == CanvasScrollAxis::Horizontal) {
            horizontal_interaction_shrink_pending_ = false;
        } else {
            vertical_interaction_shrink_pending_ = false;
        }

        const UINT dpi = std::max<UINT>(96U, GetDpiForWindow(window_));
        request.line_step = static_cast<std::uint32_t>(std::max(
            1,
            MulDiv(kCanvasScrollLineReferencePixels, static_cast<int>(dpi), 96)));
        request.page_step = projection->native.page > request.line_step
            ? projection->native.page - request.line_step
            : 1U;
        const CanvasScrollTargetResult target = ResolveCanvasScrollTarget(
            *projection, request);
        if (target.status != CanvasScrollStatus::Ok) {
            return false;
        }
        if (!target.changed) {
            return true;
        }
        const CanvasViewGesture gesture{
            INKPOD_VIEW_PAN_BY,
            axis == CanvasScrollAxis::Horizontal ? target.pan_by_delta : 0.0,
            axis == CanvasScrollAxis::Vertical ? target.pan_by_delta : 0.0,
            0.0};
        // Until an accepted snapshot reprojects the bar, another relative
        // command would be based on the same old q and could double-apply if
        // Core committed but renderer submission failed.
        scroll_command_pending_ = true;
        const bool sent = SendGesture(gesture);
        if (!sent) {
            RECT client{};
            if (GetClientRect(window_, &client) != FALSE) {
                (void)PostMessageW(
                    GetParent(window_),
                    kCanvasViewportChanged,
                    static_cast<WPARAM>(canvas_.Value()),
                    MAKELPARAM(
                        client.right - client.left,
                        client.bottom - client.top));
            }
        }
        return sent;
    }

    void EndScrollInteraction(CanvasScrollAxis axis) noexcept {
        if (axis == CanvasScrollAxis::Horizontal) {
            horizontal_scroll_tracking_ = false;
        } else {
            vertical_scroll_tracking_ = false;
        }
        if (!has_scroll_transform_ || scroll_route_ != Route()) {
            return;
        }
        if (axis == CanvasScrollAxis::Horizontal) {
            horizontal_interaction_shrink_pending_ = true;
        } else {
            vertical_interaction_shrink_pending_ = true;
        }
        if (!scroll_command_pending_
            && !HasPendingScrollProjection(scroll_route_)) {
            (void)ApplyPendingInteractionShrink();
        }
    }

    void EndAllScrollInteractions() noexcept {
        horizontal_scroll_tracking_ = false;
        vertical_scroll_tracking_ = false;
        if (!has_scroll_transform_ || scroll_route_ != Route()) {
            return;
        }
        horizontal_interaction_shrink_pending_ = true;
        vertical_interaction_shrink_pending_ = true;
        if (!scroll_command_pending_
            && !HasPendingScrollProjection(scroll_route_)) {
            (void)ApplyPendingInteractionShrink();
        }
    }

    void BeginPanInteraction() noexcept {
        horizontal_interaction_shrink_pending_ = false;
        vertical_interaction_shrink_pending_ = false;
    }

    void SynchronizeVisibility(bool visible, bool notify_viewport) noexcept {
        RendererSurfaceResourceUsage usage{};
        const bool reappeared = visible && notify_viewport
            && renderer_.GetSurfaceResourceUsage(canvas_, surface_generation_, usage)
            && (!usage.visible || usage.occluded);
        renderer_.SetVisible(canvas_, surface_generation_, visible);
        if (reappeared) {
            // Showing a parent does not guarantee a WM_SHOWWINDOW for a child.
            // A hidden view may have skipped snapshot publication altogether;
            // request its current final viewport once when it becomes visible.
            RECT client{};
            if (GetClientRect(window_, &client) != FALSE) {
                PostMessageW(GetParent(window_), kCanvasViewportChanged,
                    static_cast<WPARAM>(canvas_.Value()),
                    MAKELPARAM(client.right - client.left, client.bottom - client.top));
            }
        }
    }

    bool SendStroke(
        CanvasStrokeEventKind kind,
        const InkpodStrokeSample* samples,
        std::uint64_t sample_count) noexcept {
        if (kind == CanvasStrokeEventKind::Begin) {
            if (sequence_activation_pending_) {
                return false;
            }
            if (required_presented_revision_ != 0U || required_presentation_epoch_ != 0U) {
                RendererSurfaceResourceUsage usage{};
                if (!renderer_.GetSurfaceResourceUsage(canvas_, surface_generation_, usage)
                    || usage.route != Route()
                    || usage.last_presented_document_revision < required_presented_revision_
                    || (required_presentation_epoch_ != 0U
                        && usage.last_presented_presentation_epoch != required_presentation_epoch_)) {
                    return false;
                }
                required_presented_revision_ = 0U;
                required_presentation_epoch_ = 0U;
            }
        }
        if (sample_count > kMaximumStrokeSamples
            || (sample_count != 0U && samples == nullptr)) {
            return false;
        }
        OwnedCanvasStrokeEvent event{};
        event.kind = kind;
        try {
            if (sample_count != 0U) {
                event.samples.assign(
                    samples, samples + static_cast<std::size_t>(sample_count));
            }
        } catch (const std::bad_alloc&) {
            return false;
        }
        const std::uint64_t token = QueueStroke(std::move(event));
        if (token == 0U) {
            return false;
        }
        const LRESULT result = SendMessageW(
            GetParent(window_),
            kCanvasStrokeReady,
            static_cast<WPARAM>(token),
            static_cast<LPARAM>(surface_generation_.Value()));
        DiscardStroke(token);
        return result == 1;
    }

    bool SetSequenceFence(
        bool activation_pending,
        std::uint64_t required_presented_revision,
        std::uint64_t required_presentation_epoch) noexcept {
        if (activation_pending) {
            std::lock_guard lock(input_mutex_);
            if (stroke_active_ || !pending_strokes_.empty()) {
                return false;
            }
        }
        sequence_activation_pending_ = activation_pending;
        required_presented_revision_ = required_presented_revision;
        required_presentation_epoch_ = required_presentation_epoch;
        return true;
    }

    bool SendGesture(const CanvasViewGesture& gesture) noexcept {
        const std::uint64_t token = QueueGesture(gesture);
        if (token == 0U) {
            return false;
        }
        const LRESULT result = SendMessageW(
            GetParent(window_),
            kCanvasViewGesture,
            static_cast<WPARAM>(token),
            static_cast<LPARAM>(surface_generation_.Value()));
        DiscardGesture(token);
        return result == 1;
    }

    bool TakeStroke(
        std::uint64_t token,
        app::Generation surface_generation,
        OwnedCanvasStrokeEvent& event) noexcept {
        if (token == 0U || surface_generation != surface_generation_) {
            return false;
        }
        std::lock_guard lock(input_mutex_);
        const auto found = std::find_if(
            pending_strokes_.begin(), pending_strokes_.end(),
            [token](const PendingStroke& pending) { return pending.token == token; });
        if (found == pending_strokes_.end()) {
            return false;
        }
        event = std::move(found->event);
        pending_strokes_.erase(found);
        return true;
    }

    bool TakeGesture(
        std::uint64_t token,
        app::Generation surface_generation,
        CanvasViewGesture& gesture) noexcept {
        if (token == 0U || surface_generation != surface_generation_) {
            return false;
        }
        std::lock_guard lock(input_mutex_);
        const auto found = std::find_if(
            pending_gestures_.begin(), pending_gestures_.end(),
            [token](const PendingGesture& pending) { return pending.token == token; });
        if (found == pending_gestures_.end()) {
            return false;
        }
        gesture = found->gesture;
        pending_gestures_.erase(found);
        return true;
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

    RendererHost& Renderer() noexcept {
        return renderer_;
    }

    app::CanvasId Canvas() const noexcept {
        return canvas_;
    }

    app::Generation SurfaceGeneration() const noexcept {
        return surface_generation_;
    }

    bool PointerStrokeActive() const noexcept {
        return stroke_active_ && pointer_stroke_;
    }

    POINT last_pan_point{};
    bool panning{};

private:
    struct PendingScrollProjection {
        std::uint64_t token{};
        SnapshotRoute route{};
        InkpodSnapshotTransform transform{};
        CanvasScrollRangeHint range_hint{CanvasScrollRangeHint::Preserve};
        std::uint64_t scroll_cause_token{};
        std::uint8_t apply_attempts{};
        std::uint8_t refresh_delivery_attempts{};
    };

    struct ProjectedScrollbars {
        CanvasScrollProjection horizontal{};
        CanvasScrollProjection vertical{};
    };

    struct PendingStroke {
        std::uint64_t token{};
        OwnedCanvasStrokeEvent event;
    };

    struct PendingGesture {
        std::uint64_t token{};
        CanvasViewGesture gesture{};
    };

    static SCROLLINFO NativeScrollInfo(
        const CanvasNativeScrollInfo& native) noexcept {
        SCROLLINFO info{};
        info.cbSize = sizeof(info);
        info.fMask = SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL;
        info.nMin = native.minimum;
        info.nMax = native.maximum;
        info.nPage = native.page;
        info.nPos = native.position;
        return info;
    }

    void RedrawScrollFrame() noexcept {
        if (IsWindowVisible(window_) != FALSE
            && CanvasAncestorsVisible(window_)) {
            RedrawWindow(
                window_,
                nullptr,
                nullptr,
                RDW_INVALIDATE | RDW_FRAME | RDW_UPDATENOW
                    | RDW_NOCHILDREN | RDW_NOERASE);
        }
    }

    void DisableScrollbars() noexcept {
        SCROLLINFO disabled{};
        disabled.cbSize = sizeof(disabled);
        disabled.fMask = SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL;
        disabled.nMin = 0;
        disabled.nMax = 0;
        disabled.nPage = 1U;
        disabled.nPos = 0;
        (void)SetScrollInfo(window_, SB_HORZ, &disabled, FALSE);
        (void)SetScrollInfo(window_, SB_VERT, &disabled, FALSE);
        RedrawScrollFrame();
    }

    void InvalidatePendingScrollProjection() noexcept {
        std::lock_guard lock(scroll_mailbox_mutex_);
        ++next_scroll_projection_token_;
        if (next_scroll_projection_token_ == 0U) {
            ++next_scroll_projection_token_;
        }
        pending_scroll_projection_.reset();
        horizontal_scroll_tracking_ = false;
        vertical_scroll_tracking_ = false;
    }

    void PrepareScrollBinding(const SnapshotRoute& route) noexcept {
        InvalidatePendingScrollProjection();
        scroll_command_pending_ = false;
        horizontal_interaction_shrink_pending_ = false;
        vertical_interaction_shrink_pending_ = false;
        std::optional<SnapshotRoute> last_renderer_accepted;
        {
            std::lock_guard lock(scroll_mailbox_mutex_);
            last_renderer_accepted = last_renderer_accepted_scroll_route_;
        }
        const bool rolling_back_unaccepted_binding =
            requested_scroll_reset_route_.has_value()
            && requested_scroll_reset_route_.value() != scroll_route_
            && route == scroll_route_
            && (!last_renderer_accepted.has_value()
                || last_renderer_accepted.value()
                    != requested_scroll_reset_route_.value());
        if (rolling_back_unaccepted_binding) {
            requested_scroll_reset_route_.reset();
        } else {
            requested_scroll_reset_route_ = route;
        }
    }

    void ResetScrollProjection() noexcept {
        KillTimer(window_, kCanvasScrollProjectionRetryTimer);
        InvalidatePendingScrollProjection();
        {
            std::lock_guard lock(scroll_mailbox_mutex_);
            last_renderer_accepted_scroll_route_.reset();
        }
        scroll_route_ = {};
        scroll_transform_ = {};
        has_scroll_transform_ = false;
        horizontal_scroll_ = {};
        vertical_scroll_ = {};
        has_horizontal_scroll_ = false;
        has_vertical_scroll_ = false;
        horizontal_scroll_tracking_ = false;
        vertical_scroll_tracking_ = false;
        requested_scroll_reset_route_.reset();
        scroll_command_pending_ = false;
        horizontal_interaction_shrink_pending_ = false;
        vertical_interaction_shrink_pending_ = false;
        scroll_projection_recovery_required_ = false;
        scroll_projection_refresh_requested_ = false;
        DisableScrollbars();
    }

    bool ApplyAcceptedScrollProjection(
        const SnapshotRoute& route,
        const InkpodSnapshotTransform& transform,
        CanvasScrollRangeHint range_hint) noexcept {
        if (!route || route != Route()
            || transform.struct_size < sizeof(InkpodSnapshotTransform)
            || (range_hint != CanvasScrollRangeHint::Preserve
                && range_hint != CanvasScrollRangeHint::ResetToBase)
            || !std::isfinite(transform.zoom) || transform.zoom <= 0.0
            || !std::isfinite(transform.pan_x)
            || !std::isfinite(transform.pan_y)) {
            return false;
        }
        const bool route_changed = scroll_route_ != route;
        const bool binding_reset = requested_scroll_reset_route_.has_value()
            && requested_scroll_reset_route_.value() == route;
        const CanvasScrollRangeUpdate update =
            route_changed || binding_reset
                || range_hint == CanvasScrollRangeHint::ResetToBase
            ? CanvasScrollRangeUpdate::ResetToBase
            : CanvasScrollRangeUpdate::Preserve;
        const CanvasScrollRange horizontal_range = !route_changed
                && has_horizontal_scroll_
            ? horizontal_scroll_.range
            : CanvasScrollRange{};
        const CanvasScrollRange vertical_range = !route_changed
                && has_vertical_scroll_
            ? vertical_scroll_.range
            : CanvasScrollRange{};
        const CanvasScrollRangeLock horizontal_lock = !route_changed
                && horizontal_scroll_tracking_
                && horizontal_range.initialized
                && update == CanvasScrollRangeUpdate::Preserve
            ? CanvasScrollRangeLock::Freeze
            : CanvasScrollRangeLock::Expand;
        const CanvasScrollRangeLock vertical_lock = !route_changed
                && vertical_scroll_tracking_
                && vertical_range.initialized
                && update == CanvasScrollRangeUpdate::Preserve
            ? CanvasScrollRangeLock::Freeze
            : CanvasScrollRangeLock::Expand;
        ProjectedScrollbars projected{};
        if (!BuildScrollProjections(
                transform,
                horizontal_range,
                vertical_range,
                update,
                update,
                horizontal_lock,
                vertical_lock,
                projected)) {
            return false;
        }

        const bool consume_horizontal_shrink =
            horizontal_interaction_shrink_pending_;
        const bool consume_vertical_shrink =
            vertical_interaction_shrink_pending_;
        const CanvasScrollRangeUpdate horizontal_update =
            consume_horizontal_shrink
                && projected.horizontal.coordinate_in_base_range
            ? CanvasScrollRangeUpdate::ResetToBase
            : update;
        const CanvasScrollRangeUpdate vertical_update =
            consume_vertical_shrink && projected.vertical.coordinate_in_base_range
            ? CanvasScrollRangeUpdate::ResetToBase
            : update;
        if ((horizontal_update != update || vertical_update != update)
            && !BuildScrollProjections(
                transform,
                horizontal_range,
                vertical_range,
                horizontal_update,
                vertical_update,
                horizontal_lock,
                vertical_lock,
                projected)) {
            return false;
        }

        if (!CommitScrollProjections(projected)) {
            return false;
        }
        scroll_route_ = route;
        scroll_transform_ = transform;
        has_scroll_transform_ = true;
        if (route_changed || update == CanvasScrollRangeUpdate::ResetToBase) {
            horizontal_scroll_tracking_ = false;
            vertical_scroll_tracking_ = false;
        }
        if (binding_reset) {
            requested_scroll_reset_route_.reset();
        }
        if (update == CanvasScrollRangeUpdate::ResetToBase
            || consume_horizontal_shrink) {
            horizontal_interaction_shrink_pending_ = false;
        }
        if (update == CanvasScrollRangeUpdate::ResetToBase
            || consume_vertical_shrink) {
            vertical_interaction_shrink_pending_ = false;
        }
        scroll_command_pending_ = false;
        RedrawScrollFrame();
        if (binding_reset) {
            // A surface can transiently report DXGI_STATUS_OCCLUDED when its
            // first snapshot races the final non-client frame publication.
            // Retry once, asynchronously, after the accepted binding frame has
            // had a chance to reach the window manager.
            (void)SetTimer(
                window_, kCanvasBindingPresentRetryTimer, 16U, nullptr);
        }
        return true;
    }

    bool BuildScrollProjections(
        const InkpodSnapshotTransform& transform,
        const CanvasScrollRange& horizontal_range,
        const CanvasScrollRange& vertical_range,
        CanvasScrollRangeUpdate horizontal_update,
        CanvasScrollRangeUpdate vertical_update,
        CanvasScrollRangeLock horizontal_lock,
        CanvasScrollRangeLock vertical_lock,
        ProjectedScrollbars& projected) noexcept {
        RECT client{};
        if (GetClientRect(window_, &client) == FALSE
            || client.right <= client.left || client.bottom <= client.top) {
            return false;
        }
        const auto width = static_cast<std::uint32_t>(client.right - client.left);
        const auto height = static_cast<std::uint32_t>(client.bottom - client.top);
        const CanvasScrollProjectionResult horizontal = ProjectCanvasScrollAxis(
            CanvasScrollAxisInput{
                transform.pan_x,
                transform.zoom,
                transform.document_width,
                width,
                horizontal_range,
                horizontal_update,
                horizontal_lock});
        const CanvasScrollProjectionResult vertical = ProjectCanvasScrollAxis(
            CanvasScrollAxisInput{
                transform.pan_y,
                transform.zoom,
                transform.document_height,
                height,
                vertical_range,
                vertical_update,
                vertical_lock});
        if (horizontal.status != CanvasScrollStatus::Ok
            || vertical.status != CanvasScrollStatus::Ok) {
            return false;
        }
        projected.horizontal = horizontal.projection;
        projected.vertical = vertical.projection;
        return true;
    }

    static bool ScrollInfoMatches(
        const SCROLLINFO& actual,
        const CanvasNativeScrollInfo& expected) noexcept {
        return actual.nMin == expected.minimum
            && actual.nMax == expected.maximum
            && actual.nPage == expected.page
            && actual.nPos == expected.position;
    }

    static bool ScrollInfoMatches(
        const SCROLLINFO& actual,
        const SCROLLINFO& expected) noexcept {
        return actual.nMin == expected.nMin
            && actual.nMax == expected.nMax
            && actual.nPage == expected.nPage
            && actual.nPos == expected.nPos;
    }

    bool ReadScrollInfo(int bar, SCROLLINFO& info) const noexcept {
        info = {};
        info.cbSize = sizeof(info);
        info.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
        return GetScrollInfo(window_, bar, &info) != FALSE;
    }

    bool CommitScrollProjections(
        const ProjectedScrollbars& projected) noexcept {
        const bool recovery_was_required = scroll_projection_recovery_required_;
        SCROLLINFO previous_horizontal{};
        SCROLLINFO previous_vertical{};
        if (!ReadScrollInfo(SB_HORZ, previous_horizontal)
            || !ReadScrollInfo(SB_VERT, previous_vertical)) {
            return false;
        }
        SCROLLINFO horizontal_info = NativeScrollInfo(projected.horizontal.native);
        SCROLLINFO vertical_info = NativeScrollInfo(projected.vertical.native);
        (void)SetScrollInfo(window_, SB_HORZ, &horizontal_info, FALSE);
        (void)SetScrollInfo(window_, SB_VERT, &vertical_info, FALSE);
        SCROLLINFO applied_horizontal{};
        SCROLLINFO applied_vertical{};
        const bool applied = ReadScrollInfo(SB_HORZ, applied_horizontal)
            && ReadScrollInfo(SB_VERT, applied_vertical)
            && ScrollInfoMatches(
                applied_horizontal, projected.horizontal.native)
            && ScrollInfoMatches(applied_vertical, projected.vertical.native);
        if (!applied) {
            previous_horizontal.fMask =
                SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL;
            previous_vertical.fMask =
                SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL;
            (void)SetScrollInfo(
                window_, SB_HORZ, &previous_horizontal, FALSE);
            (void)SetScrollInfo(
                window_, SB_VERT, &previous_vertical, FALSE);
            SCROLLINFO restored_horizontal{};
            SCROLLINFO restored_vertical{};
            const bool restored = ReadScrollInfo(SB_HORZ, restored_horizontal)
                && ReadScrollInfo(SB_VERT, restored_vertical)
                && ScrollInfoMatches(restored_horizontal, previous_horizontal)
                && ScrollInfoMatches(restored_vertical, previous_vertical);
            scroll_projection_recovery_required_ =
                recovery_was_required || !restored;
            if (!restored) {
                RECT client{};
                if (GetClientRect(window_, &client) != FALSE) {
                    (void)PostMessageW(
                        GetParent(window_),
                        kCanvasViewportChanged,
                        static_cast<WPARAM>(canvas_.Value()),
                        MAKELPARAM(
                            client.right - client.left,
                            client.bottom - client.top));
                }
            }
            return false;
        }
        horizontal_scroll_ = projected.horizontal;
        vertical_scroll_ = projected.vertical;
        has_horizontal_scroll_ = true;
        has_vertical_scroll_ = true;
        scroll_projection_recovery_required_ = false;
        scroll_projection_refresh_requested_ = false;
        return true;
    }

    bool ReprojectScrollbars(
        CanvasScrollRangeUpdate horizontal_update,
        CanvasScrollRangeUpdate vertical_update) noexcept {
        if (!has_scroll_transform_ || scroll_route_ != Route()) {
            return false;
        }
        const CanvasScrollRange horizontal_range = has_horizontal_scroll_
            ? horizontal_scroll_.range : CanvasScrollRange{};
        const CanvasScrollRange vertical_range = has_vertical_scroll_
            ? vertical_scroll_.range : CanvasScrollRange{};
        const CanvasScrollRangeLock horizontal_lock =
            horizontal_scroll_tracking_ && horizontal_range.initialized
                && horizontal_update == CanvasScrollRangeUpdate::Preserve
            ? CanvasScrollRangeLock::Freeze
            : CanvasScrollRangeLock::Expand;
        const CanvasScrollRangeLock vertical_lock =
            vertical_scroll_tracking_ && vertical_range.initialized
                && vertical_update == CanvasScrollRangeUpdate::Preserve
            ? CanvasScrollRangeLock::Freeze
            : CanvasScrollRangeLock::Expand;
        ProjectedScrollbars projected{};
        if (!BuildScrollProjections(
                scroll_transform_,
                horizontal_range,
                vertical_range,
                horizontal_update,
                vertical_update,
                horizontal_lock,
                vertical_lock,
                projected)) {
            if (!has_horizontal_scroll_ && !has_vertical_scroll_) {
                DisableScrollbars();
            }
            return false;
        }
        if (!CommitScrollProjections(projected)) {
            return false;
        }
        RedrawScrollFrame();
        return true;
    }

    bool HasPendingScrollProjection(const SnapshotRoute& route) noexcept {
        std::lock_guard lock(scroll_mailbox_mutex_);
        return pending_scroll_projection_.has_value()
            && pending_scroll_projection_->route == route;
    }

    bool ApplyPendingInteractionShrink() noexcept {
        if ((!horizontal_interaction_shrink_pending_
                && !vertical_interaction_shrink_pending_)
            || !has_scroll_transform_ || scroll_route_ != Route()) {
            return false;
        }
        const CanvasScrollRangeUpdate horizontal_update =
            horizontal_interaction_shrink_pending_ && has_horizontal_scroll_
                && horizontal_scroll_.coordinate_in_base_range
            ? CanvasScrollRangeUpdate::ResetToBase
            : CanvasScrollRangeUpdate::Preserve;
        const CanvasScrollRangeUpdate vertical_update =
            vertical_interaction_shrink_pending_ && has_vertical_scroll_
                && vertical_scroll_.coordinate_in_base_range
            ? CanvasScrollRangeUpdate::ResetToBase
            : CanvasScrollRangeUpdate::Preserve;
        if (!ReprojectScrollbars(horizontal_update, vertical_update)) {
            return false;
        }
        horizontal_interaction_shrink_pending_ = false;
        vertical_interaction_shrink_pending_ = false;
        return true;
    }

    std::uint64_t NextInputToken() noexcept {
        ++next_input_token_;
        if (next_input_token_ == 0U) {
            ++next_input_token_;
        }
        return next_input_token_;
    }

    std::uint64_t QueueStroke(OwnedCanvasStrokeEvent event) noexcept {
        std::lock_guard lock(input_mutex_);
        if (pending_strokes_.size() >= kMaximumPendingCanvasInput) {
            return 0U;
        }
        const std::uint64_t token = NextInputToken();
        try {
            pending_strokes_.push_back(PendingStroke{token, std::move(event)});
        } catch (const std::bad_alloc&) {
            return 0U;
        }
        return token;
    }

    std::uint64_t QueueGesture(const CanvasViewGesture& gesture) noexcept {
        std::lock_guard lock(input_mutex_);
        if (pending_gestures_.size() >= kMaximumPendingCanvasInput) {
            return 0U;
        }
        const std::uint64_t token = NextInputToken();
        try {
            pending_gestures_.push_back(PendingGesture{token, gesture});
        } catch (const std::bad_alloc&) {
            return 0U;
        }
        return token;
    }

    void DiscardStroke(std::uint64_t token) noexcept {
        std::lock_guard lock(input_mutex_);
        std::erase_if(
            pending_strokes_,
            [token](const PendingStroke& pending) { return pending.token == token; });
    }

    void DiscardGesture(std::uint64_t token) noexcept {
        std::lock_guard lock(input_mutex_);
        std::erase_if(
            pending_gestures_,
            [token](const PendingGesture& pending) { return pending.token == token; });
    }

    HWND window_{};
    HWND owner_window_{};
    RendererHost& renderer_;
    app::CanvasId canvas_{};
    app::Generation surface_generation_{};
    mutable std::mutex route_mutex_;
    SnapshotRoute route_{};
    std::mutex scroll_mailbox_mutex_;
    std::optional<PendingScrollProjection> pending_scroll_projection_;
    std::optional<SnapshotRoute> last_renderer_accepted_scroll_route_;
    std::uint64_t next_scroll_projection_token_{};
    SnapshotRoute scroll_route_{};
    InkpodSnapshotTransform scroll_transform_{};
    CanvasScrollProjection horizontal_scroll_{};
    CanvasScrollProjection vertical_scroll_{};
    bool has_scroll_transform_{};
    bool has_horizontal_scroll_{};
    bool has_vertical_scroll_{};
    bool horizontal_scroll_tracking_{};
    bool vertical_scroll_tracking_{};
    std::optional<SnapshotRoute> requested_scroll_reset_route_;
    bool scroll_command_pending_{};
    bool horizontal_interaction_shrink_pending_{};
    bool vertical_interaction_shrink_pending_{};
    bool scroll_projection_recovery_required_{};
    bool scroll_projection_refresh_requested_{};
    bool scroll_projection_apply_active_{};
    std::mutex input_mutex_;
    std::deque<PendingStroke> pending_strokes_;
    std::deque<PendingGesture> pending_gestures_;
    std::uint64_t next_input_token_{};
    bool sequence_activation_pending_{};
    std::uint64_t required_presented_revision_{};
    std::uint64_t required_presentation_epoch_{};
    bool stroke_active_{};
    bool pointer_stroke_{};
    UINT32 active_pointer_id_{};
};

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

struct CanvasCreateParameters {
    RendererHost* renderer;
    app::CanvasId canvas;
    app::Generation surface_generation;
    HWND owner_window;
};

LRESULT CALLBACK CanvasWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    switch (message) {
        case WM_CREATE: {
            const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
            const auto* parameters = create == nullptr
                ? nullptr
                : static_cast<const CanvasCreateParameters*>(create->lpCreateParams);
            if (parameters == nullptr || parameters->renderer == nullptr
                || !parameters->canvas || !parameters->surface_generation
                || parameters->owner_window == nullptr) {
                SetLastError(ERROR_INVALID_PARAMETER);
                return -1;
            }
            auto* created = new (std::nothrow) CanvasHost(
                window,
                parameters->owner_window,
                *parameters->renderer,
                parameters->canvas,
                parameters->surface_generation);
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
            if (host != nullptr) {
                host->CancelStroke();
                host->DrainScrollProjectionMailbox();
                const bool visible = wparam != SIZE_MINIMIZED
                    && IsWindowVisible(window) != FALSE
                    && CanvasAncestorsVisible(window);
                host->SynchronizeVisibility(visible, false);
                if (wparam != SIZE_MINIMIZED) {
                    host->RefreshScrollProjectionForViewport();
                    const UINT width = LOWORD(lparam);
                    const UINT height = HIWORD(lparam);
                    host->Renderer().Resize(
                        host->Canvas(), host->SurfaceGeneration(), width, height);
                    PostMessageW(
                        GetParent(window),
                        kCanvasViewportChanged,
                        static_cast<WPARAM>(host->Canvas().Value()),
                        MAKELPARAM(width, height));
                }
            }
            return 0;
        case WM_GETDLGCODE: {
            const LRESULT base = DefWindowProcW(window, message, wparam, lparam);
            if (wparam == VK_LEFT || wparam == VK_RIGHT
                || wparam == VK_UP || wparam == VK_DOWN) {
                return base | DLGC_WANTARROWS;
            }
            if (wparam == VK_PRIOR || wparam == VK_NEXT) {
                return base | DLGC_WANTMESSAGE;
            }
            return base;
        }
        case WM_SHOWWINDOW:
            if (host != nullptr) {
                host->DrainScrollProjectionMailbox();
                host->SynchronizeVisibility(
                    wparam != FALSE && CanvasAncestorsVisible(window), true);
            }
            break;
        case WM_PAINT: {
            if (host != nullptr) {
                host->DrainScrollProjectionMailbox();
            }
            PAINTSTRUCT paint{};
            BeginPaint(window, &paint);
            EndPaint(window, &paint);
            if (host != nullptr) {
                host->SynchronizeVisibility(
                    IsWindowVisible(window) != FALSE && CanvasAncestorsVisible(window), true);
                host->Renderer().RequestRender(
                    host->Canvas(), host->SurfaceGeneration());
            }
            return 0;
        }
        case WM_ERASEBKGND:
            return 1;
        case WM_SETFOCUS:
            if (host != nullptr) {
                SendMessageW(
                    GetParent(window),
                    kCanvasActivated,
                    static_cast<WPARAM>(host->Canvas().Value()),
                    static_cast<LPARAM>(host->SurfaceGeneration().Value()));
            }
            return 0;
        case WM_DPICHANGED_AFTERPARENT: {
            if (host != nullptr) {
                host->Renderer().DpiChanged(
                    host->Canvas(), host->SurfaceGeneration());
            }
            return host == nullptr ? 0 : 1;
        }
        case WM_HSCROLL:
            return host != nullptr
                    && host->HandleScroll(
                        CanvasScrollAxis::Horizontal,
                        LOWORD(wparam))
                ? 1
                : 0;
        case WM_VSCROLL:
            return host != nullptr
                    && host->HandleScroll(
                        CanvasScrollAxis::Vertical,
                        LOWORD(wparam))
                ? 1
                : 0;
        case WM_KEYDOWN:
            if (host != nullptr
                && (GetKeyState(VK_SHIFT) & 0x8000) != 0
                && (GetKeyState(VK_CONTROL) & 0x8000) == 0
                && (GetKeyState(VK_MENU) & 0x8000) == 0) {
                switch (wparam) {
                    case VK_LEFT:
                        return host->HandleScroll(
                                   CanvasScrollAxis::Horizontal,
                                   SB_LINELEFT)
                            ? 1 : 0;
                    case VK_RIGHT:
                        return host->HandleScroll(
                                   CanvasScrollAxis::Horizontal,
                                   SB_LINERIGHT)
                            ? 1 : 0;
                    case VK_UP:
                        return host->HandleScroll(
                                   CanvasScrollAxis::Vertical,
                                   SB_LINEUP)
                            ? 1 : 0;
                    case VK_DOWN:
                        return host->HandleScroll(
                                   CanvasScrollAxis::Vertical,
                                   SB_LINEDOWN)
                            ? 1 : 0;
                    case VK_PRIOR:
                        return host->HandleScroll(
                                   CanvasScrollAxis::Vertical,
                                   SB_PAGEUP)
                            ? 1 : 0;
                    case VK_NEXT:
                        return host->HandleScroll(
                                   CanvasScrollAxis::Vertical,
                                   SB_PAGEDOWN)
                            ? 1 : 0;
                    default:
                        break;
                }
            }
            break;
        case WM_TIMER:
            if (host != nullptr
                && wparam == kCanvasBindingPresentRetryTimer) {
                KillTimer(window, kCanvasBindingPresentRetryTimer);
                host->Renderer().RequestRender(
                    host->Canvas(), host->SurfaceGeneration());
                return 0;
            }
            if (host != nullptr
                && wparam == kCanvasScrollProjectionRetryTimer) {
                KillTimer(window, kCanvasScrollProjectionRetryTimer);
                host->DrainScrollProjectionMailbox();
                return 0;
            }
            break;
        case WM_LBUTTONDOWN:
            if (host != nullptr && !host->PointerStrokeActive()) {
                SetFocus(window);
                SetCapture(window);
                return host->BeginMouse(
                           static_cast<float>(GET_X_LPARAM(lparam)),
                           static_cast<float>(GET_Y_LPARAM(lparam)))
                    ? 1
                    : 0;
            }
            return 0;
        case WM_MOUSEMOVE:
            if ((wparam & (MK_LBUTTON | MK_MBUTTON | MK_RBUTTON)) == 0U) {
                PostMessageW(
                    GetParent(window),
                    kCanvasPointerMoved,
                    host == nullptr
                        ? 0
                        : static_cast<WPARAM>(host->Canvas().Value()),
                    MAKELPARAM(GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)));
            }
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
                return host->SendGesture(gesture) ? 1 : 0;
            }
            return 0;
        case WM_LBUTTONUP:
            if (host != nullptr && !host->PointerStrokeActive()) {
                const bool completed = host->EndMouse(
                    static_cast<float>(GET_X_LPARAM(lparam)),
                    static_cast<float>(GET_Y_LPARAM(lparam)));
                ReleaseCapture();
                SendMessageW(
                    GetParent(window),
                    kCanvasInteractionEnded,
                    static_cast<WPARAM>(host->Canvas().Value()),
                    static_cast<LPARAM>(host->SurfaceGeneration().Value()));
                return completed ? 1 : 0;
            }
            return 0;
        case WM_MBUTTONDOWN:
            if (host != nullptr) {
                SetFocus(window);
                host->BeginPanInteraction();
                host->panning = true;
                host->last_pan_point = POINT{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
                SetCapture(window);
                return 1;
            }
            return 0;
        case WM_MBUTTONUP:
            if (host != nullptr) {
                host->panning = false;
                host->EndAllScrollInteractions();
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
            return host == nullptr || !host->SendGesture(gesture) ? 0 : 1;
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
                SetFocus(window);
                SetCapture(window);
                return host->BeginPointer(pointer_id, samples) ? 1 : 0;
            }
            if (message == WM_POINTERUPDATE) {
                return host->AppendPointer(pointer_id, samples) ? 1 : 0;
            }
            const bool completed = host->EndPointer(pointer_id, samples);
            ReleaseCapture();
            SendMessageW(
                GetParent(window),
                kCanvasInteractionEnded,
                static_cast<WPARAM>(host->Canvas().Value()),
                static_cast<LPARAM>(host->SurfaceGeneration().Value()));
            return completed ? 1 : 0;
        }
        case WM_CAPTURECHANGED:
            if (host != nullptr) {
                host->CancelStroke();
                host->panning = false;
                host->EndAllScrollInteractions();
                SendMessageW(
                    GetParent(window),
                    kCanvasInteractionEnded,
                    static_cast<WPARAM>(host->Canvas().Value()),
                    static_cast<LPARAM>(host->SurfaceGeneration().Value()));
            }
            return 0;
        case kCanvasScrollProjectionChanged:
            return host != nullptr
                    && host->ApplyQueuedScrollProjection(
                        static_cast<std::uint64_t>(wparam),
                        app::Generation(static_cast<std::uint64_t>(lparam)))
                ? 1
                : 0;
        case kCanvasRenderOnce:
            return host != nullptr
                    && SUCCEEDED(host->Renderer().RenderOnce(
                        host->Canvas(), host->SurfaceGeneration()))
                ? 1
                : 0;
        case kCanvasSimulateDeviceLoss:
            return host != nullptr
                    && SUCCEEDED(host->Renderer().SimulateDeviceLoss(
                        host->Canvas(), host->SurfaceGeneration()))
                ? 1
                : 0;
        case kCanvasClearGeometryPreview: {
            CanvasGeometryPreview preview{};
            preview.struct_size = sizeof(preview);
            return host != nullptr
                    && SUCCEEDED(host->Renderer().SetGeometryPreview(
                        host->Canvas(), host->SurfaceGeneration(), preview))
                ? 1
                : 0;
        }
        case kCanvasGetRendererThreadId:
            return host == nullptr ? 0 : static_cast<LRESULT>(host->Renderer().ThreadId());
        case kCanvasGetPresentedFrameCount:
            return host == nullptr
                ? 0
                : static_cast<LRESULT>(host->Renderer().PresentedFrameCount(
                      host->Canvas(), host->SurfaceGeneration()));
        case WM_NCDESTROY:
            KillTimer(window, kCanvasBindingPresentRetryTimer);
            KillTimer(window, kCanvasScrollProjectionRetryTimer);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            delete host;
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

}  // namespace

struct RendererHost::Impl final {
    RendererHostState state;
};

RendererHost::RendererHost() noexcept = default;

RendererHost::~RendererHost() {
    Stop();
}

HRESULT RendererHost::Start() noexcept {
    if (impl_ != nullptr) {
        return E_UNEXPECTED;
    }
    try {
        auto candidate = std::make_unique<Impl>();
        const HRESULT result = candidate->state.Start();
        if (FAILED(result)) {
            return result;
        }
        impl_ = std::move(candidate);
        return S_OK;
    } catch (const std::bad_alloc&) {
        return E_OUTOFMEMORY;
    }
}

void RendererHost::Stop() noexcept {
    if (impl_ != nullptr) {
        impl_->state.Stop();
        impl_.reset();
    }
}

HRESULT RendererHost::RegisterSurface(
    app::CanvasId canvas,
    app::Generation surface_generation,
    HWND window,
    HWND owner_window) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::Register;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.window = window;
    control.owner_window = owner_window;
    return impl_->state.Invoke(std::move(control));
}

void RendererHost::UnregisterSurface(
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    if (impl_ == nullptr) {
        return;
    }
    HostControl control{};
    control.kind = HostControlKind::Unregister;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    (void)impl_->state.Invoke(std::move(control));
}

HRESULT RendererHost::BindSurface(const SnapshotRoute& route) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::Bind;
    control.canvas = route.canvas;
    control.surface_generation = route.surface_generation;
    control.route = route;
    return impl_->state.Invoke(std::move(control));
}

HRESULT RendererHost::UnbindSurface(
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    if (impl_ == nullptr || !canvas || !surface_generation) {
        return E_INVALIDARG;
    }
    HostControl control{};
    control.kind = HostControlKind::Unbind;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    return impl_->state.Invoke(std::move(control));
}

bool RendererHost::SurfaceAcceptsSnapshots(const SnapshotRoute& route) const noexcept {
    return impl_ != nullptr && impl_->state.SurfaceAcceptsSnapshots(route);
}

bool RendererHost::Submit(SnapshotEnvelope envelope) noexcept {
    if (impl_ != nullptr) {
        return impl_->state.Submit(envelope);
    }
    if (envelope.snapshot != nullptr) {
        inkpod_snapshot_release(&envelope.snapshot);
    }
    return false;
}

void RendererHost::Resize(
    app::CanvasId canvas,
    app::Generation surface_generation,
    UINT width,
    UINT height) noexcept {
    if (impl_ == nullptr) {
        return;
    }
    HostControl control{};
    control.kind = HostControlKind::Resize;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.width = width;
    control.height = height;
    impl_->state.Post(std::move(control));
}

void RendererHost::SetVisible(
    app::CanvasId canvas,
    app::Generation surface_generation,
    bool visible) noexcept {
    if (impl_ == nullptr) {
        return;
    }
    HostControl control{};
    control.kind = HostControlKind::Visibility;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.visible = visible;
    impl_->state.Post(std::move(control));
}

void RendererHost::RequestRender(
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    if (impl_ == nullptr) {
        return;
    }
    HostControl control{};
    control.kind = HostControlKind::Render;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    impl_->state.Post(std::move(control));
}

void RendererHost::DpiChanged(
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    if (impl_ == nullptr) {
        return;
    }
    HostControl control{};
    control.kind = HostControlKind::DpiChanged;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    impl_->state.Post(std::move(control));
}

HRESULT RendererHost::RenderOnce(
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::Render;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    return impl_->state.Invoke(std::move(control));
}

HRESULT RendererHost::SimulateDeviceLoss(
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::SimulateDeviceLoss;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    return impl_->state.Invoke(std::move(control));
}


HRESULT RendererHost::ReadPixelForSmokeTest(
    app::CanvasId canvas,
    app::Generation surface_generation,
    UINT x,
    UINT y,
    CanvasPixelRgba8& pixel) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::ReadPixel;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.out_pixel = &pixel;
    control.pixel_x = x;
    control.pixel_y = y;
    return impl_->state.Invoke(std::move(control));
}

HRESULT RendererHost::GetDocumentBounds(
    app::CanvasId canvas,
    app::Generation surface_generation,
    CanvasDocumentBounds& bounds) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::GetDocumentBounds;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.out_bounds = &bounds;
    return impl_->state.Invoke(std::move(control));
}

HRESULT RendererHost::GetGeometryPreview(
    app::CanvasId canvas,
    app::Generation surface_generation,
    CanvasGeometryPreview& preview) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::GetGeometryPreview;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.out_geometry_preview = &preview;
    return impl_->state.Invoke(std::move(control));
}

HRESULT RendererHost::SetFloatingPreview(
    app::CanvasId canvas,
    app::Generation surface_generation,
    const CanvasFloatingPreview& preview) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::SetFloatingPreview;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.floating_preview = preview;
    return impl_->state.Invoke(std::move(control));
}

HRESULT RendererHost::SetGeometryPreview(
    app::CanvasId canvas,
    app::Generation surface_generation,
    const CanvasGeometryPreview& preview) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::SetGeometryPreview;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.geometry_preview = preview;
    return impl_->state.Invoke(std::move(control));
}


DWORD RendererHost::ThreadId() const noexcept {
    return impl_ == nullptr ? 0U : impl_->state.ThreadId();
}

std::uint64_t RendererHost::PresentedFrameCount(
    app::CanvasId canvas,
    app::Generation surface_generation) const noexcept {
    return impl_ == nullptr
        ? 0U
        : impl_->state.PresentedFrameCount(canvas, surface_generation);
}

std::size_t RendererHost::SurfaceCount() const noexcept {
    return impl_ == nullptr ? 0U : impl_->state.SurfaceCount();
}

std::uint64_t RendererHost::DeviceGeneration() const noexcept {
    return impl_ == nullptr ? 0U : impl_->state.DeviceGeneration();
}

RendererResourceUsage RendererHost::ResourceUsage() const noexcept {
    return impl_ == nullptr ? RendererResourceUsage{} : impl_->state.ResourceUsage();
}

bool RendererHost::GetSurfaceResourceUsage(
    app::CanvasId canvas,
    app::Generation surface_generation,
    RendererSurfaceResourceUsage& usage) const noexcept {
    return impl_ != nullptr
        && impl_->state.GetSurfaceResourceUsage(
            canvas, surface_generation, usage);
}

void RendererHost::SetQueuePausedForSmokeTest(bool paused) noexcept {
    if (impl_ != nullptr) {
        impl_->state.SetQueuePausedForSmokeTest(paused);
    }
}

bool RendererHost::WaitQueueIdleForSmokeTest() noexcept {
    return impl_ != nullptr && impl_->state.WaitQueueIdleForSmokeTest();
}

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

HWND CreateCanvasWindow(
    HINSTANCE instance,
    HWND parent,
    RendererHost& renderer,
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    RECT client{};
    GetClientRect(parent, &client);
    const CanvasCreateParameters parameters{
        &renderer,
        canvas,
        surface_generation,
        GetAncestor(parent, GA_ROOT)};
    const HWND window = CreateWindowExW(
        0,
        kCanvasClassName,
        L"",
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_HSCROLL | WS_VSCROLL,
        0,
        0,
        client.right - client.left,
        client.bottom - client.top,
        parent,
        nullptr,
        instance,
        const_cast<CanvasCreateParameters*>(&parameters));
    auto* host = window == nullptr
        ? nullptr
        : reinterpret_cast<CanvasHost*>(
              GetWindowLongPtrW(window, GWLP_USERDATA));
    if (host != nullptr) {
        host->SynchronizeVisibility(
            IsWindowVisible(window) != FALSE && CanvasAncestorsVisible(window),
            false);
    }
    return window;
}

CanvasSnapshotSink* GetCanvasSnapshotSink(HWND canvas) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host;
}

bool BindCanvasSnapshotSink(
    HWND canvas,
    app::DocumentSessionId document_session,
    app::DocumentViewId document_view,
    app::Generation document_generation) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr
        && host->Bind(document_session, document_view, document_generation);
}

bool BindAuxiliaryCanvasSnapshotSink(
    HWND canvas,
    app::AuxiliarySourceId source,
    app::Generation generation) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr && host->BindAuxiliary(source, generation);
}

bool UnbindCanvasSnapshotSink(HWND canvas) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr && host->Unbind();
}

void CancelCanvasStroke(HWND canvas) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    if (host != nullptr) {
        host->CancelStroke();
    }
}

bool SetCanvasSequenceFence(
    HWND canvas,
    bool activation_pending,
    std::uint64_t required_presented_revision,
    std::uint64_t required_presentation_epoch) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    if (host == nullptr) {
        return false;
    }
    return host->SetSequenceFence(
        activation_pending, required_presented_revision, required_presentation_epoch);
}

bool TakeCanvasStrokeEvent(
    HWND canvas,
    std::uint64_t token,
    app::Generation surface_generation,
    OwnedCanvasStrokeEvent& event) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr && host->TakeStroke(token, surface_generation, event);
}

bool TakeCanvasViewGesture(
    HWND canvas,
    std::uint64_t token,
    app::Generation surface_generation,
    CanvasViewGesture& gesture) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr && host->TakeGesture(token, surface_generation, gesture);
}

bool SubmitCanvasStrokeEvent(
    HWND canvas,
    CanvasStrokeEventKind kind,
    const InkpodStrokeSample* samples,
    std::uint64_t sample_count) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr && host->SendStroke(kind, samples, sample_count);
}

bool SubmitCanvasStrokeEvent(
    HWND canvas, const CanvasStrokeEvent& event) noexcept {
    return SubmitCanvasStrokeEvent(
        canvas, event.kind, event.samples, event.sample_count);
}

bool GetCanvasDocumentBounds(
    HWND canvas, CanvasDocumentBounds& bounds) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr
        && SUCCEEDED(host->Renderer().GetDocumentBounds(
            host->Canvas(), host->SurfaceGeneration(), bounds));
}

bool GetCanvasGeometryPreview(
    HWND canvas, CanvasGeometryPreview& preview) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr
        && SUCCEEDED(host->Renderer().GetGeometryPreview(
            host->Canvas(), host->SurfaceGeneration(), preview));
}

bool SetCanvasFloatingPreview(
    HWND canvas, const CanvasFloatingPreview& preview) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr
        && SUCCEEDED(host->Renderer().SetFloatingPreview(
            host->Canvas(), host->SurfaceGeneration(), preview));
}

bool SetCanvasGeometryPreview(
    HWND canvas, const CanvasGeometryPreview& preview) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr
        && SUCCEEDED(host->Renderer().SetGeometryPreview(
            host->Canvas(), host->SurfaceGeneration(), preview));
}


}  // namespace inkpod::renderer
