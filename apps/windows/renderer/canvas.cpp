#include "canvas.h"

#include <d2d1_1.h>
#include <d3d11.h>
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
constexpr std::uint64_t kMaximumVectorSegments = 262144U;
constexpr std::uint64_t kMaximumVectorFills = 65536U;
constexpr std::uint64_t kMaximumVectorBoundaries = 262144U;
constexpr std::uint64_t kMaximumOverlayLines = 8192U;
constexpr std::size_t kMaximumPointerHistory = 256U;
constexpr std::size_t kMaximumPendingCanvasInput = 64U;
constexpr std::uint64_t kMaximumStrokeSamples = UINT64_C(1048576);
constexpr std::uint32_t kVectorSamplesPerSegment = 24U;
constexpr float kVectorMiterLimit = 4.0F;

struct CachedTile {
    std::uint64_t revision{};
    int origin_x{};
    int origin_y{};
    UINT width{};
    UINT height{};
    ComPtr<ID2D1Bitmap1> bitmap;
};

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
        result = d2d_factory_->CreateDevice(dxgi_device_.Get(), &d2d_device_);
        if (SUCCEEDED(result)) {
            ++generation_;
        }
        return result;
    }

    void Discard() noexcept {
        d2d_device_.Reset();
        d2d_factory_.Reset();
        dxgi_factory_.Reset();
        dxgi_device_.Reset();
        d3d_context_.Reset();
        d3d_device_.Reset();
    }

    [[nodiscard]] ID3D11Device* D3dDevice() const noexcept {
        return d3d_device_.Get();
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
    std::uint64_t generation_{};
};

class CanvasSurface final {
public:
    CanvasSurface(
        HWND window,
        HWND owner_window,
        SharedRendererDevice& shared) noexcept
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
            return S_OK;
        }
        if (!swap_chain_) {
            return CreateSurfaceResources();
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
            const HRESULT create_result = CreateSurfaceResources();
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
            const bool legacy_check = (snapshot_view_.feature_flags
                & INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE) != 0U;
            const bool native_check = (snapshot_view_.feature_flags
                & INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA) != 0U;
            const bool transparent_view = (overlay_.flags
                & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) != 0U;
            const D2D1_COLOR_F paper_color = legacy_check
                ? D2D1::ColorF(D2D1::ColorF::Black)
                : (native_check ? D2D1::ColorF(D2D1::ColorF::Magenta)
                                : (transparent_view
                                          ? D2D1::ColorF(0.78F, 0.78F, 0.78F, 1.0F)
                                          : D2D1::ColorF(D2D1::ColorF::White)));
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
            if (transparent_view && checker_columns != 0U
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
            result = DrawVectors();
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

        result = swap_chain_->Present(1U, 0U);
        if (result == DXGI_STATUS_OCCLUDED) {
            return result;
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
        InkpodSnapshotOverlay overlay{};
        overlay.struct_size = sizeof(overlay);
        InkpodSnapshotVectorView vectors{};
        vectors.struct_size = sizeof(vectors);
        const InkpodStatus view_status = inkpod_snapshot_get_view(snapshot, &view);
        const InkpodStatus transform_status = view_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_transform(snapshot, &transform)
            : view_status;
        const InkpodStatus overlay_status = transform_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_overlay(snapshot, &overlay)
            : transform_status;
        const InkpodStatus vector_status = overlay_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_vectors(snapshot, &vectors)
            : overlay_status;
        if (view_status != INKPOD_STATUS_OK || transform_status != INKPOD_STATUS_OK
            || overlay_status != INKPOD_STATUS_OK || vector_status != INKPOD_STATUS_OK
            || !ValidateOverlay(overlay) || !ValidateVectors(vectors)) {
            inkpod_snapshot_release(&snapshot);
            return E_INVALIDARG;
        }
        if (snapshot_ != nullptr) {
            inkpod_snapshot_release(&snapshot_);
        }
        snapshot_ = snapshot;
        snapshot_view_ = view;
        transform_ = transform;
        overlay_ = overlay;
        vectors_ = vectors;
        return RebuildTileCache();
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
            || !std::isfinite(preview.transform.translate_x)
            || !std::isfinite(preview.transform.translate_y)
            || !std::isfinite(preview.transform.scale_x)
            || !std::isfinite(preview.transform.scale_y)
            || !std::isfinite(preview.transform.rotation_degrees)) {
            return E_INVALIDARG;
        }
        floating_preview_ = preview;
        return S_OK;
    }

    HRESULT SetGeometryPreview(const CanvasGeometryPreview& preview) noexcept {
        if (preview.struct_size < sizeof(CanvasGeometryPreview)
            || preview.active > 1U || preview.closed > 1U
            || preview.point_count > kCanvasGeometryPreviewPoints
            || !std::isfinite(preview.stroke_width) || preview.stroke_width < 0.0F
            || preview.stroke_width > 4096.0F || preview.reserved != 0U) {
            return E_INVALIDARG;
        }
        for (std::uint32_t index = 0U; index < preview.point_count; ++index) {
            if (!std::isfinite(preview.points[index].x)
                || !std::isfinite(preview.points[index].y)) {
                return E_INVALIDARG;
            }
        }
        geometry_preview_ = preview;
        return S_OK;
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
        transform_ = {};
        overlay_ = {};
        vectors_ = {};
        tile_cache_.clear();
    }

    void SetTileBudget(std::size_t tile_budget) noexcept {
        tile_budget_ = std::max<std::size_t>(1U, tile_budget);
    }

    HRESULT ValidateClosedVectorStrokeForSmokeTest() noexcept {
        const auto* segment_bytes = reinterpret_cast<const std::byte*>(vectors_.segments);
        for (std::uint64_t index = 0U; index < vectors_.segment_count;) {
            const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                segment_bytes + static_cast<std::size_t>(index * vectors_.segment_stride_bytes));
            const VectorPathSpan path{
                segment->path_id,
                segment->z_order,
                segment->flags,
                segment,
                segment->segment_count};
            if ((path.flags & INKPOD_SNAPSHOT_VECTOR_CLOSED) != 0U) {
                ComPtr<ID2D1PathGeometry> geometry;
                const HRESULT create_result = CreateStrokeGeometry(path, geometry);
                if (FAILED(create_result)) {
                    return create_result;
                }
                const auto* last = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                    reinterpret_cast<const std::byte*>(path.first)
                    + static_cast<std::size_t>(
                        (path.count - 1U) * vectors_.segment_stride_bytes));
                constexpr float seam_probe_amount =
                    (static_cast<float>(kVectorSamplesPerSegment) - 0.5F)
                    / static_cast<float>(kVectorSamplesPerSegment);
                BOOL contains{};
                const HRESULT contains_result = geometry->FillContainsPoint(
                    CubicPoint(*last, seam_probe_amount),
                    nullptr,
                    0.01F,
                    &contains);
                if (FAILED(contains_result)) {
                    return contains_result;
                }
                if (contains == FALSE || path.count < 2U) {
                    return E_FAIL;
                }
                const auto* next = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                    reinterpret_cast<const std::byte*>(path.first)
                    + static_cast<std::size_t>(vectors_.segment_stride_bytes));
                float incoming_x{};
                float incoming_y{};
                float outgoing_x{};
                float outgoing_y{};
                if (!UnitDirection(
                        D2D1::Point2F(path.first->p2.x, path.first->p2.y),
                        D2D1::Point2F(path.first->p3.x, path.first->p3.y),
                        incoming_x,
                        incoming_y)
                    || !UnitDirection(
                        D2D1::Point2F(next->p0.x, next->p0.y),
                        D2D1::Point2F(next->p1.x, next->p1.y),
                        outgoing_x,
                        outgoing_y)) {
                    return E_FAIL;
                }
                float miter_x = -incoming_y - outgoing_y;
                float miter_y = incoming_x + outgoing_x;
                const float miter_length = std::hypot(miter_x, miter_y);
                if (miter_length <= 0.000001F) {
                    return E_FAIL;
                }
                miter_x /= miter_length;
                miter_y /= miter_length;
                const float denominator =
                    miter_x * -outgoing_y + miter_y * outgoing_x;
                const float half_width = path.first->width_end * 0.5F;
                if (denominator <= 0.000001F || half_width <= 0.0F) {
                    return E_FAIL;
                }
                const float corrected_miter = half_width / denominator;
                if (corrected_miter <= half_width
                    || corrected_miter > half_width * kVectorMiterLimit) {
                    return E_FAIL;
                }
                const float corner_probe_distance =
                    (half_width + corrected_miter) * 0.5F;
                const D2D1_POINT_2F corner_probe = D2D1::Point2F(
                    path.first->p3.x + miter_x * corner_probe_distance,
                    path.first->p3.y + miter_y * corner_probe_distance);
                contains = FALSE;
                const HRESULT corner_result = geometry->FillContainsPoint(
                    corner_probe, nullptr, 0.01F, &contains);
                if (FAILED(corner_result)) {
                    return corner_result;
                }
                return contains != FALSE ? S_OK : E_FAIL;
            }
            index += path.count;
        }
        return E_INVALIDARG;
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

    static bool ValidateVectors(const InkpodSnapshotVectorView& vectors) noexcept {
        if (vectors.abi_version != INKPOD_ABI_VERSION || vectors.feature_flags != 0U
            || vectors.segment_count > kMaximumVectorSegments
            || vectors.fill_count > kMaximumVectorFills
            || vectors.boundary_path_count > kMaximumVectorBoundaries
            || vectors.segment_stride_bytes < sizeof(InkpodSnapshotVectorSegment)
            || vectors.segment_stride_bytes % alignof(InkpodSnapshotVectorSegment) != 0U
            || vectors.fill_stride_bytes < sizeof(InkpodSnapshotVectorFill)
            || vectors.fill_stride_bytes % alignof(InkpodSnapshotVectorFill) != 0U
            || (vectors.segment_count != 0U && vectors.segments == nullptr)
            || (vectors.fill_count != 0U && vectors.fills == nullptr)
            || (vectors.boundary_path_count != 0U && vectors.boundary_path_ids == nullptr)
            || (vectors.segments != nullptr
                && reinterpret_cast<std::uintptr_t>(vectors.segments)
                    % alignof(InkpodSnapshotVectorSegment) != 0U)
            || (vectors.fills != nullptr
                && reinterpret_cast<std::uintptr_t>(vectors.fills)
                    % alignof(InkpodSnapshotVectorFill) != 0U)
            || (vectors.boundary_path_ids != nullptr
                && reinterpret_cast<std::uintptr_t>(vectors.boundary_path_ids)
                    % alignof(std::uint64_t) != 0U)
            || vectors.segment_stride_bytes
                > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
            || vectors.fill_stride_bytes
                > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
            || (vectors.segment_count > 1U
                && vectors.segment_stride_bytes
                    > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
                        / (vectors.segment_count - 1U))
            || (vectors.fill_count > 1U
                && vectors.fill_stride_bytes
                    > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
                        / (vectors.fill_count - 1U))) {
            return false;
        }
        const auto* segment_bytes = reinterpret_cast<const std::byte*>(vectors.segments);
        std::uint64_t active_path_id = 0U;
        std::uint32_t active_count = 0U;
        std::uint32_t next_index = 0U;
        std::uint32_t active_flags = 0U;
        for (std::uint64_t index = 0; index < vectors.segment_count; ++index) {
            const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                segment_bytes + static_cast<std::size_t>(index * vectors.segment_stride_bytes));
            constexpr std::uint32_t known_flags = INKPOD_SNAPSHOT_VECTOR_CLOSED
                | INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE;
            const auto finite_point = [](const InkpodVectorPoint& point) noexcept {
                return std::isfinite(point.x) && std::isfinite(point.y)
                    && std::abs(point.x) <= 2000000.0F && std::abs(point.y) <= 2000000.0F;
            };
            if (segment->struct_size < sizeof(InkpodSnapshotVectorSegment)
                || segment->struct_size > vectors.segment_stride_bytes
                || (segment->flags & ~known_flags) != 0U || segment->path_id == 0U
                || segment->plane_id == 0U || segment->z_order > 4096U
                || segment->segment_count == 0U
                || segment->segment_index >= segment->segment_count
                || !finite_point(segment->p0) || !finite_point(segment->p1)
                || !finite_point(segment->p2) || !finite_point(segment->p3)
                || !std::isfinite(segment->width_start)
                || !std::isfinite(segment->width_end) || segment->width_start <= 0.0F
                || segment->width_end <= 0.0F || segment->width_start > 4096.0F
                || segment->width_end > 4096.0F) {
                return false;
            }
            if (segment->segment_index == 0U) {
                if (next_index != active_count) {
                    return false;
                }
                active_path_id = segment->path_id;
                active_count = segment->segment_count;
                next_index = 0U;
                active_flags = segment->flags;
            }
            if (segment->path_id != active_path_id || segment->segment_count != active_count
                || segment->segment_index != next_index || segment->flags != active_flags) {
                return false;
            }
            ++next_index;
        }
        if (vectors.segment_count != 0U && next_index != active_count) {
            return false;
        }
        const auto* fill_bytes = reinterpret_cast<const std::byte*>(vectors.fills);
        for (std::uint64_t index = 0; index < vectors.fill_count; ++index) {
            const auto* fill = reinterpret_cast<const InkpodSnapshotVectorFill*>(
                fill_bytes + static_cast<std::size_t>(index * vectors.fill_stride_bytes));
            if (fill->struct_size < sizeof(InkpodSnapshotVectorFill)
                || fill->struct_size > vectors.fill_stride_bytes || fill->reserved != 0U
                || fill->fill_id == 0U || fill->plane_id == 0U || fill->z_order > 4096U
                || fill->boundary_path_count == 0U
                || fill->first_boundary_path > vectors.boundary_path_count
                || fill->boundary_path_count
                    > vectors.boundary_path_count - fill->first_boundary_path) {
                return false;
            }
            for (std::uint64_t boundary = 0; boundary < fill->boundary_path_count; ++boundary) {
                if (vectors.boundary_path_ids[fill->first_boundary_path + boundary] == 0U) {
                    return false;
                }
            }
        }
        return true;
    }

    struct VectorPathSpan {
        std::uint64_t id{};
        std::uint32_t z_order{};
        std::uint32_t flags{};
        const InkpodSnapshotVectorSegment* first{};
        std::uint32_t count{};
    };

    static D2D1_COLOR_F VectorColor(std::uint32_t rgba) noexcept {
        return D2D1::ColorF(
            static_cast<float>((rgba >> 24U) & 0xffU) / 255.0F,
            static_cast<float>((rgba >> 16U) & 0xffU) / 255.0F,
            static_cast<float>((rgba >> 8U) & 0xffU) / 255.0F,
            static_cast<float>(rgba & 0xffU) / 255.0F);
    }

    static D2D1_POINT_2F CubicPoint(
        const InkpodSnapshotVectorSegment& segment,
        float amount) noexcept {
        const float inverse = 1.0F - amount;
        const std::array<float, 4> weights{
            inverse * inverse * inverse,
            3.0F * inverse * inverse * amount,
            3.0F * inverse * amount * amount,
            amount * amount * amount};
        return D2D1::Point2F(
            weights[0] * segment.p0.x + weights[1] * segment.p1.x
                + weights[2] * segment.p2.x + weights[3] * segment.p3.x,
            weights[0] * segment.p0.y + weights[1] * segment.p1.y
                + weights[2] * segment.p2.y + weights[3] * segment.p3.y);
    }

    static float CubicWidth(
        const InkpodSnapshotVectorSegment& segment,
        float amount) noexcept {
        return segment.width_start + (segment.width_end - segment.width_start) * amount;
    }

    static bool UnitDirection(
        D2D1_POINT_2F start,
        D2D1_POINT_2F end,
        float& x,
        float& y) noexcept {
        x = end.x - start.x;
        y = end.y - start.y;
        const float length = std::hypot(x, y);
        if (length <= 0.000001F) {
            return false;
        }
        x /= length;
        y /= length;
        return true;
    }

    HRESULT DrawFillGeometry(
        const InkpodSnapshotVectorFill& fill,
        const std::unordered_map<std::uint64_t, VectorPathSpan>& paths,
        ID2D1SolidColorBrush* brush) noexcept {
        ComPtr<ID2D1PathGeometry> geometry;
        HRESULT result = shared_.D2dFactory()->CreatePathGeometry(&geometry);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<ID2D1GeometrySink> sink;
        result = geometry->Open(&sink);
        if (FAILED(result)) {
            return result;
        }
        sink->SetFillMode(D2D1_FILL_MODE_ALTERNATE);
        for (std::uint64_t index = 0; index < fill.boundary_path_count; ++index) {
            const std::uint64_t path_id =
                vectors_.boundary_path_ids[fill.first_boundary_path + index];
            const auto iterator = paths.find(path_id);
            if (iterator == paths.end()
                || (iterator->second.flags & INKPOD_SNAPSHOT_VECTOR_CLOSED) == 0U) {
                sink->Close();
                return E_INVALIDARG;
            }
            const VectorPathSpan& path = iterator->second;
            sink->BeginFigure(
                D2D1::Point2F(path.first->p0.x, path.first->p0.y),
                D2D1_FIGURE_BEGIN_FILLED);
            for (std::uint32_t segment_index = 0; segment_index < path.count;
                 ++segment_index) {
                const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                    reinterpret_cast<const std::byte*>(path.first)
                    + static_cast<std::size_t>(segment_index * vectors_.segment_stride_bytes));
                sink->AddBezier(D2D1::BezierSegment(
                    D2D1::Point2F(segment->p1.x, segment->p1.y),
                    D2D1::Point2F(segment->p2.x, segment->p2.y),
                    D2D1::Point2F(segment->p3.x, segment->p3.y)));
            }
            sink->EndFigure(D2D1_FIGURE_END_CLOSED);
        }
        result = sink->Close();
        if (FAILED(result)) {
            return result;
        }
        brush->SetColor(VectorColor(fill.color_rgba));
        d2d_context_->FillGeometry(geometry.Get(), brush);
        return S_OK;
    }

    HRESULT CreateStrokeGeometry(
        const VectorPathSpan& path,
        ComPtr<ID2D1PathGeometry>& geometry) noexcept {
        try {
            std::vector<D2D1_POINT_2F> centers;
            std::vector<float> widths;
            centers.reserve(
                static_cast<std::size_t>(path.count) * kVectorSamplesPerSegment + 1U);
            widths.reserve(centers.capacity());
            for (std::uint32_t segment_index = 0; segment_index < path.count;
                 ++segment_index) {
                const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                    reinterpret_cast<const std::byte*>(path.first)
                    + static_cast<std::size_t>(segment_index * vectors_.segment_stride_bytes));
                for (std::uint32_t sample = 0U; sample <= kVectorSamplesPerSegment;
                     ++sample) {
                    if (segment_index != 0U && sample == 0U) {
                        continue;
                    }
                    const float amount = static_cast<float>(sample)
                        / static_cast<float>(kVectorSamplesPerSegment);
                    centers.push_back(CubicPoint(*segment, amount));
                    widths.push_back(CubicWidth(*segment, amount));
                }
            }
            const bool closed = (path.flags & INKPOD_SNAPSHOT_VECTOR_CLOSED) != 0U;
            if (closed && centers.size() > 1U
                && std::abs(centers.front().x - centers.back().x) < 0.0001F
                && std::abs(centers.front().y - centers.back().y) < 0.0001F) {
                centers.pop_back();
                widths.pop_back();
            }
            if (centers.size() < 2U) {
                return E_INVALIDARG;
            }
            std::vector<D2D1_POINT_2F> left;
            std::vector<D2D1_POINT_2F> right;
            left.reserve(centers.size() * 2U);
            right.reserve(centers.size() * 2U);
            for (std::size_t index = 0; index < centers.size(); ++index) {
                const std::size_t previous = index == 0U
                    ? (closed ? centers.size() - 1U : 0U)
                    : index - 1U;
                const std::size_t next = index + 1U == centers.size()
                    ? (closed ? 0U : centers.size() - 1U)
                    : index + 1U;
                float incoming_x{};
                float incoming_y{};
                float outgoing_x{};
                float outgoing_y{};
                const bool has_incoming = (closed || index != 0U)
                    && UnitDirection(
                        centers[previous], centers[index], incoming_x, incoming_y);
                const bool has_outgoing = (closed || index + 1U != centers.size())
                    && UnitDirection(
                        centers[index], centers[next], outgoing_x, outgoing_y);
                if (!has_incoming && !has_outgoing) {
                    incoming_x = 1.0F;
                    incoming_y = 0.0F;
                    outgoing_x = incoming_x;
                    outgoing_y = incoming_y;
                } else if (!has_incoming) {
                    incoming_x = outgoing_x;
                    incoming_y = outgoing_y;
                } else if (!has_outgoing) {
                    outgoing_x = incoming_x;
                    outgoing_y = incoming_y;
                }
                const float half_width = widths[index] * 0.5F;
                const float incoming_normal_x = -incoming_y;
                const float incoming_normal_y = incoming_x;
                const float outgoing_normal_x = -outgoing_y;
                const float outgoing_normal_y = outgoing_x;
                float miter_x = incoming_normal_x + outgoing_normal_x;
                float miter_y = incoming_normal_y + outgoing_normal_y;
                const float miter_vector_length = std::hypot(miter_x, miter_y);
                if (miter_vector_length > 0.000001F) {
                    miter_x /= miter_vector_length;
                    miter_y /= miter_vector_length;
                    const float denominator =
                        miter_x * outgoing_normal_x + miter_y * outgoing_normal_y;
                    if (denominator > 0.000001F) {
                        const float miter_length = half_width / denominator;
                        if (miter_length <= half_width * kVectorMiterLimit) {
                            left.push_back(D2D1::Point2F(
                                centers[index].x + miter_x * miter_length,
                                centers[index].y + miter_y * miter_length));
                            right.push_back(D2D1::Point2F(
                                centers[index].x - miter_x * miter_length,
                                centers[index].y - miter_y * miter_length));
                            continue;
                        }
                    }
                }
                left.push_back(D2D1::Point2F(
                    centers[index].x + incoming_normal_x * half_width,
                    centers[index].y + incoming_normal_y * half_width));
                right.push_back(D2D1::Point2F(
                    centers[index].x - incoming_normal_x * half_width,
                    centers[index].y - incoming_normal_y * half_width));
                if (incoming_normal_x != outgoing_normal_x
                    || incoming_normal_y != outgoing_normal_y) {
                    left.push_back(D2D1::Point2F(
                        centers[index].x + outgoing_normal_x * half_width,
                        centers[index].y + outgoing_normal_y * half_width));
                    right.push_back(D2D1::Point2F(
                        centers[index].x - outgoing_normal_x * half_width,
                        centers[index].y - outgoing_normal_y * half_width));
                }
            }
            geometry.Reset();
            HRESULT result = shared_.D2dFactory()->CreatePathGeometry(&geometry);
            if (FAILED(result)) {
                return result;
            }
            ComPtr<ID2D1GeometrySink> sink;
            result = geometry->Open(&sink);
            if (FAILED(result)) {
                return result;
            }
            sink->SetFillMode(D2D1_FILL_MODE_ALTERNATE);
            if (closed) {
                sink->BeginFigure(left.front(), D2D1_FIGURE_BEGIN_FILLED);
                sink->AddLines(
                    left.data() + 1U, static_cast<UINT32>(left.size() - 1U));
                sink->EndFigure(D2D1_FIGURE_END_CLOSED);
                sink->BeginFigure(right.front(), D2D1_FIGURE_BEGIN_FILLED);
                sink->AddLines(
                    right.data() + 1U, static_cast<UINT32>(right.size() - 1U));
                sink->EndFigure(D2D1_FIGURE_END_CLOSED);
            } else {
                sink->BeginFigure(left.front(), D2D1_FIGURE_BEGIN_FILLED);
                sink->AddLines(
                    left.data() + 1U, static_cast<UINT32>(left.size() - 1U));
                for (auto iterator = right.rbegin(); iterator != right.rend(); ++iterator) {
                    sink->AddLine(*iterator);
                }
                sink->EndFigure(D2D1_FIGURE_END_CLOSED);
            }
            result = sink->Close();
            if (FAILED(result)) {
                return result;
            }
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    HRESULT DrawStrokeGeometry(
        const VectorPathSpan& path,
        ID2D1SolidColorBrush* brush) noexcept {
        ComPtr<ID2D1PathGeometry> geometry;
        const HRESULT result = CreateStrokeGeometry(path, geometry);
        if (FAILED(result)) {
            return result;
        }
        brush->SetColor(VectorColor(path.first->color_rgba));
        d2d_context_->FillGeometry(geometry.Get(), brush);
        return S_OK;
    }

    HRESULT DrawVectors() noexcept {
        if (vectors_.segment_count == 0U && vectors_.fill_count == 0U) {
            return S_OK;
        }
        try {
            std::vector<VectorPathSpan> ordered_paths;
            std::unordered_map<std::uint64_t, VectorPathSpan> paths;
            ordered_paths.reserve(static_cast<std::size_t>(vectors_.segment_count));
            const auto* segment_bytes = reinterpret_cast<const std::byte*>(vectors_.segments);
            for (std::uint64_t index = 0; index < vectors_.segment_count;) {
                const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                    segment_bytes + static_cast<std::size_t>(index * vectors_.segment_stride_bytes));
                VectorPathSpan span{
                    segment->path_id,
                    segment->z_order,
                    segment->flags,
                    segment,
                    segment->segment_count};
                ordered_paths.push_back(span);
                paths.emplace(span.id, span);
                index += span.count;
            }
            ComPtr<ID2D1SolidColorBrush> brush;
            HRESULT result = d2d_context_->CreateSolidColorBrush(
                D2D1::ColorF(D2D1::ColorF::Black), &brush);
            if (FAILED(result)) {
                return result;
            }
            std::uint32_t maximum_z = 0U;
            for (const auto& path : ordered_paths) {
                maximum_z = std::max(maximum_z, path.z_order);
            }
            const auto* fill_bytes = reinterpret_cast<const std::byte*>(vectors_.fills);
            for (std::uint64_t index = 0; index < vectors_.fill_count; ++index) {
                const auto* fill = reinterpret_cast<const InkpodSnapshotVectorFill*>(
                    fill_bytes + static_cast<std::size_t>(index * vectors_.fill_stride_bytes));
                maximum_z = std::max(maximum_z, fill->z_order);
            }
            for (std::uint32_t z_order = 0U; z_order <= maximum_z; ++z_order) {
                for (std::uint64_t index = 0; index < vectors_.fill_count; ++index) {
                    const auto* fill = reinterpret_cast<const InkpodSnapshotVectorFill*>(
                        fill_bytes + static_cast<std::size_t>(index * vectors_.fill_stride_bytes));
                    if (fill->z_order == z_order && (fill->color_rgba & 0xffU) != 0U) {
                        result = DrawFillGeometry(*fill, paths, brush.Get());
                        if (FAILED(result)) {
                            return result;
                        }
                    }
                }
                for (const auto& path : ordered_paths) {
                    if (path.z_order == z_order
                        && (path.flags & INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE) != 0U
                        && (path.first->color_rgba & 0xffU) != 0U) {
                        result = DrawStrokeGeometry(path, brush.Get());
                        if (FAILED(result)) {
                            return result;
                        }
                    }
                }
            }
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
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
        const double center_x = static_cast<double>(bounds.x)
            + static_cast<double>(bounds.width - 1) / 2.0;
        const double center_y = static_cast<double>(bounds.y)
            + static_cast<double>(bounds.height - 1) / 2.0;
        const double radians = transform.rotation_degrees * 3.14159265358979323846 / 180.0;
        const double sine = std::sin(radians);
        const double cosine = std::cos(radians);
        const auto point = [&](double x, double y) {
            const double local_x = (x - center_x) * transform.scale_x;
            const double local_y = (y - center_y) * transform.scale_y;
            return D2D1::Point2F(
                static_cast<float>(center_x + local_x * cosine - local_y * sine
                    + transform.translate_x),
                static_cast<float>(center_y + local_x * sine + local_y * cosine
                    + transform.translate_y));
        };
        const double left = static_cast<double>(bounds.x);
        const double top = static_cast<double>(bounds.y);
        const double right = static_cast<double>(bounds.x + bounds.width - 1);
        const double bottom = static_cast<double>(bounds.y + bounds.height - 1);
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

    HRESULT RebuildTileCache() noexcept {
        if (!d2d_context_) {
            return E_UNEXPECTED;
        }
        if (snapshot_ == nullptr) {
            tile_cache_.clear();
            return S_OK;
        }
        if (snapshot_view_.tile_count > kMaximumSnapshotTiles
            || snapshot_view_.tile_count > tile_budget_
            || (snapshot_view_.feature_flags
                    & ~(INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE
                        | INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA))
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
        }
        return result;
    }

    void DiscardSurfaceResources() noexcept {
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
        swap_chain_.Reset();
    }

    HWND window_{};
    HWND owner_window_{};
    SharedRendererDevice& shared_;
    InkpodSnapshot* snapshot_{};
    InkpodSnapshotView snapshot_view_{};
    InkpodSnapshotTransform transform_{};
    InkpodSnapshotOverlay overlay_{};
    InkpodSnapshotVectorView vectors_{};
    CanvasFloatingPreview floating_preview_{};
    CanvasGeometryPreview geometry_preview_{};
    std::unordered_map<std::uint64_t, CachedTile> tile_cache_;
    ComPtr<IDXGISwapChain1> swap_chain_;
    ComPtr<ID2D1DeviceContext> d2d_context_;
    ComPtr<ID2D1Bitmap1> target_bitmap_;
    HANDLE frame_latency_waitable_{};
    std::size_t tile_budget_{kMaximumSnapshotTiles};
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
    ValidateClosedVectorStroke,
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
};

struct PublishedSurface {
    app::CanvasId canvas{};
    app::Generation generation{};
    SnapshotRoute route{};
    bool visible{};
    bool occluded{};
    std::uint64_t presented_frames{};
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
                stopping_ = false;
                running_ = true;
            }
            worker_ = std::thread([this, ready] { Run(ready); });
            const HRESULT result = future.get();
            if (FAILED(result) && worker_.joinable()) {
                worker_.join();
            }
            return result;
        } catch (const std::system_error&) {
            return E_FAIL;
        } catch (const std::future_error&) {
            return E_FAIL;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    void Stop() noexcept {
        {
            std::lock_guard lock(mutex_);
            if (!worker_.joinable()) {
                running_ = false;
                stopping_ = true;
                return;
            }
            stopping_ = true;
        }
        wake_.notify_one();
        worker_.join();
        std::lock_guard lock(mutex_);
        running_ = false;
        published_.clear();
    }

    HRESULT Invoke(HostControl control) noexcept {
        SnapshotEnvelope discarded{};
        try {
            auto completion = std::make_shared<std::promise<HRESULT>>();
            auto future = completion->get_future();
            control.completion = completion;
            {
                std::lock_guard lock(mutex_);
                if (!running_ || stopping_ || work_.size() >= kMaximumHostWork) {
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
        try {
            {
                std::lock_guard lock(mutex_);
                if (control.kind == HostControlKind::Visibility && !control.visible) {
                    const auto published = FindPublishedLocked(
                        control.canvas, control.surface_generation);
                    if (published != published_.end()) {
                        published->visible = false;
                    }
                }
                if (!running_ || stopping_
                    || work_.size() >= kMaximumNoncriticalHostWork) {
                    return;
                }
                work_.emplace_back(std::move(control));
            }
            wake_.notify_one();
        } catch (const std::bad_alloc&) {
        }
    }

    bool Submit(SnapshotEnvelope envelope) noexcept {
        if (envelope.snapshot == nullptr || !envelope.route) {
            ReleaseEnvelope(envelope);
            return false;
        }
        SnapshotEnvelope replaced{};
        bool accepted{};
        try {
            {
                std::lock_guard lock(mutex_);
                const auto status = FindPublishedLocked(envelope.route.canvas,
                    envelope.route.surface_generation);
                if (!running_ || stopping_ || status == published_.end()
                    || status->route != envelope.route || !status->visible
                    || status->occluded) {
                    accepted = false;
                } else {
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
                        accepted = true;
                    } else if (work_.size() < kMaximumNoncriticalHostWork) {
                        work_.emplace_back(envelope);
                        envelope.snapshot = nullptr;
                        accepted = true;
                    }
                }
            }
        } catch (const std::bad_alloc&) {
            accepted = false;
        }
        ReleaseEnvelope(replaced);
        if (!accepted) {
            ReleaseEnvelope(envelope);
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

    void SetQueuePausedForSmokeTest(bool paused) noexcept {
        {
            std::lock_guard lock(mutex_);
            queue_paused_for_smoke_test_ = paused;
        }
        wake_.notify_one();
    }

    bool WaitQueueIdleForSmokeTest() noexcept {
        std::unique_lock lock(mutex_);
        if (!running_ || stopping_ || queue_paused_for_smoke_test_) {
            return false;
        }
        queue_idle_.wait(lock, [this] {
            return stopping_ || work_.empty();
        });
        return !stopping_;
    }

private:
    static constexpr std::size_t kMaximumHostWork = 256U;
    static constexpr std::size_t kReservedHostControlWork = 8U;
    static constexpr std::size_t kMaximumNoncriticalHostWork =
        kMaximumHostWork - kReservedHostControlWork;

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
            surface.visible,
            surface.occluded,
            surface.presented_frames};
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

    void UpdateTileBudgets() noexcept {
        const std::size_t per_surface = std::max<std::size_t>(
            1U, static_cast<std::size_t>(kMaximumSnapshotTiles) / std::max<std::size_t>(1U, surfaces_.size()));
        for (auto& surface : surfaces_) {
            surface.surface->SetTileBudget(per_surface);
        }
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
            PublishSurface(surface);
        }
        return S_OK;
    }

    HRESULT NormalizeResult(SurfaceRecord& surface, HRESULT result) noexcept {
        if (IsDeviceLoss(result)) {
            result = RecoverDevice();
        }
        if (result == DXGI_STATUS_OCCLUDED) {
            surface.occluded = true;
            PublishSurface(surface);
            return S_OK;
        }
        if (result == S_OK && surface.occluded) {
            surface.occluded = false;
            PublishSurface(surface);
        }
        return result;
    }

    HRESULT RenderAndCount(SurfaceRecord& surface) noexcept {
        HRESULT result = surface.surface->Render();
        const bool presented = result == S_OK;
        result = NormalizeResult(surface, result);
        if (SUCCEEDED(result) && presented) {
            ++surface.presented_frames;
            PublishSurface(surface);
        }
        return result;
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
            return E_INVALIDARG;
        }
        HRESULT result = found->surface->SetSnapshot(envelope.snapshot);
        envelope.snapshot = nullptr;
        result = NormalizeResult(*found, result);
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
                PublishSurface(surfaces_.back());
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
            RemovePublished(found->canvas, found->generation);
            surfaces_.erase(found);
            UpdateTileBudgets();
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
                surface.surface->ClearSnapshot();
                surface.route = control.route;
                surface.occluded = false;
                PublishSurface(surface);
                break;
            case HostControlKind::Unbind:
                surface.surface->ClearSnapshot();
                surface.route = {};
                surface.occluded = false;
                PublishSurface(surface);
                break;
            case HostControlKind::Resize:
                result = surface.surface->Resize(control.width, control.height);
                render = surface.visible;
                break;
            case HostControlKind::Visibility:
                surface.visible = control.visible;
                if (surface.visible) {
                    surface.occluded = false;
                    render = true;
                }
                PublishSurface(surface);
                break;
            case HostControlKind::Render:
                render = surface.visible;
                break;
            case HostControlKind::DpiChanged:
                result = surface.surface->DpiChanged();
                render = surface.visible;
                break;
            case HostControlKind::SimulateDeviceLoss:
                result = surface.surface->SimulateDeviceLossForSmokeTest();
                render = surface.visible;
                break;
            case HostControlKind::ValidateClosedVectorStroke:
                result = surface.surface->ValidateClosedVectorStrokeForSmokeTest();
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
                render = surface.visible;
                break;
            case HostControlKind::SetGeometryPreview:
                result = surface.surface->SetGeometryPreview(control.geometry_preview);
                render = surface.visible;
                break;
            case HostControlKind::Register:
            case HostControlKind::Unregister:
                break;
        }
        result = NormalizeResult(surface, result);
        if (SUCCEEDED(result) && render) {
            result = RenderAndCount(surface);
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
            {
                std::unique_lock lock(mutex_);
                wake_.wait(lock, [this] {
                    return stopping_
                        || (!queue_paused_for_smoke_test_ && !work_.empty());
                });
                if (stopping_) {
                    std::deque<HostWork> abandoned;
                    abandoned.swap(work_);
                    lock.unlock();
                    AbortWork(abandoned);
                    break;
                }
                item = std::move(work_.front());
                work_.pop_front();
                if (work_.empty()) {
                    queue_idle_.notify_all();
                }
            }
            HRESULT result{};
            SurfaceRecord* failure_surface{};
            if (auto* envelope = std::get_if<SnapshotEnvelope>(&item)) {
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
    bool stopping_{true};
    bool running_{};
    bool queue_paused_for_smoke_test_{};
    std::deque<HostWork> work_;
    std::vector<PublishedSurface> published_;
    std::vector<SurfaceRecord> surfaces_;
    SharedRendererDevice shared_;
    std::atomic<DWORD> thread_id_{};
    std::atomic<std::uint64_t> device_generation_{};
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
        return renderer_.RegisterSurface(
            canvas_, surface_generation_, window_, owner_window_);
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
        std::lock_guard lock(route_mutex_);
        route_ = route;
        return true;
    }

    bool Unbind() noexcept {
        if (FAILED(renderer_.UnbindSurface(canvas_, surface_generation_))) {
            return false;
        }
        std::lock_guard lock(route_mutex_);
        route_ = {};
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
        return renderer_.Submit(envelope);
    }

    bool SendStroke(
        CanvasStrokeEventKind kind,
        const InkpodStrokeSample* samples,
        std::uint64_t sample_count) noexcept {
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
    struct PendingStroke {
        std::uint64_t token{};
        OwnedCanvasStrokeEvent event;
    };

    struct PendingGesture {
        std::uint64_t token{};
        CanvasViewGesture gesture{};
    };

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
    std::mutex input_mutex_;
    std::deque<PendingStroke> pending_strokes_;
    std::deque<PendingGesture> pending_gestures_;
    std::uint64_t next_input_token_{};
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
                host->Renderer().SetVisible(
                    host->Canvas(), host->SurfaceGeneration(), wparam != SIZE_MINIMIZED);
                if (wparam != SIZE_MINIMIZED) {
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
        case WM_SHOWWINDOW:
            if (host != nullptr) {
                host->Renderer().SetVisible(
                    host->Canvas(), host->SurfaceGeneration(), wparam != FALSE);
            }
            break;
        case WM_PAINT: {
            PAINTSTRUCT paint{};
            BeginPaint(window, &paint);
            EndPaint(window, &paint);
            if (host != nullptr) {
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
                SendMessageW(
                    GetParent(window),
                    kCanvasInteractionEnded,
                    static_cast<WPARAM>(host->Canvas().Value()),
                    static_cast<LPARAM>(host->SurfaceGeneration().Value()));
            }
            return 0;
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
        case kCanvasValidateClosedVectorStroke:
            return host != nullptr
                    && SUCCEEDED(host->Renderer().ValidateClosedVectorStroke(
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

HRESULT RendererHost::ValidateClosedVectorStroke(
    app::CanvasId canvas,
    app::Generation surface_generation) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::ValidateClosedVectorStroke;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
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
        const_cast<CanvasCreateParameters*>(&parameters));
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
