#include "canvas.h"

#include <d2d1_1.h>
#include <d2d1effects.h>
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
constexpr std::uint64_t kMaximumVectorSegments = 262144U;
constexpr std::uint64_t kMaximumVectorFills = 65536U;
constexpr std::uint64_t kMaximumVectorBoundaries = 262144U;
constexpr std::uint64_t kMaximumVectorEndpoints = 131072U;
constexpr std::uint64_t kMaximumRenderPasses = 1048576U;
constexpr std::uint64_t kMaximumAdjustmentLuts = 4096U;
constexpr std::uint64_t kMaximumAnnotations = 16384U;
constexpr std::uint64_t kMaximumAnnotationPoints = 16384U * 65536U;
constexpr std::uint64_t kMaximumAnnotationUtf8Bytes = UINT64_C(1) << 30;
constexpr std::uint64_t kMaximumOverlayLines = 8192U;
constexpr std::size_t kMaximumPointerHistory = 256U;
constexpr std::size_t kMaximumPendingCanvasInput = 64U;
constexpr std::uint64_t kMaximumStrokeSamples = UINT64_C(1048576);
constexpr std::uint32_t kVectorSamplesPerSegment = 24U;
constexpr float kVectorMiterLimit = 4.0F;
constexpr std::uint64_t kApplicationGpuTileBudgetBytes = UINT64_C(512) * 1024U * 1024U;
std::atomic<std::uint64_t> gApplicationTileUseSequence{};

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

std::uint64_t SaturatingAdd(std::uint64_t left, std::uint64_t right) noexcept {
    return left > UINT64_MAX - right ? UINT64_MAX : left + right;
}

std::uint64_t SaturatingProduct(std::uint64_t left, std::uint64_t right) noexcept {
    return left != 0U && right > UINT64_MAX / left ? UINT64_MAX : left * right;
}

std::uint64_t EstimateSnapshotPayloadBytes(InkpodSnapshot* snapshot) noexcept {
    if (snapshot == nullptr) {
        return 0U;
    }
    InkpodSnapshotView view{};
    view.struct_size = sizeof(view);
    InkpodSnapshotOverlay overlay{};
    overlay.struct_size = sizeof(overlay);
    InkpodSnapshotVectorView vectors{};
    vectors.struct_size = sizeof(vectors);
    InkpodSnapshotAnnotationView annotations{};
    annotations.struct_size = sizeof(annotations);
    InkpodSnapshotShootingFrameView shooting_frames{};
    shooting_frames.struct_size = sizeof(shooting_frames);
    InkpodSnapshotVanishingPointView vanishing_points{};
    vanishing_points.struct_size = sizeof(vanishing_points);
    InkpodSnapshotVectorDiagnostics diagnostics{};
    diagnostics.struct_size = sizeof(diagnostics);
    InkpodSnapshotRenderPlan plan{};
    plan.struct_size = sizeof(plan);
    if (inkpod_snapshot_get_view(snapshot, &view) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_overlay(snapshot, &overlay) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_vectors(snapshot, &vectors) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_annotations(snapshot, &annotations) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_shooting_frames(snapshot, &shooting_frames) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_vanishing_points(snapshot, &vanishing_points) != INKPOD_STATUS_OK
        || inkpod_snapshot_get_vector_diagnostics(snapshot, &diagnostics)
            != INKPOD_STATUS_OK
        || inkpod_snapshot_get_render_plan(snapshot, &plan) != INKPOD_STATUS_OK) {
        return 0U;
    }
    std::uint64_t bytes = sizeof(InkpodSnapshotView) + sizeof(InkpodSnapshotTransform)
        + sizeof(InkpodSnapshotOverlay) + sizeof(InkpodSnapshotVectorView)
        + sizeof(InkpodSnapshotAnnotationView)
        + sizeof(InkpodSnapshotShootingFrameView)
        + sizeof(InkpodSnapshotVanishingPointView)
        + sizeof(InkpodSnapshotVectorDiagnostics)
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
        vanishing_points.point_count, vanishing_points.point_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        vanishing_points.radial_guide_count,
        vanishing_points.radial_guide_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        vectors.segment_count, vectors.segment_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        vectors.fill_count, vectors.fill_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        vectors.boundary_path_count, sizeof(std::uint64_t)));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        annotations.object_count, annotations.object_stride_bytes));
    bytes = SaturatingAdd(bytes, annotations.utf8_byte_count);
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        annotations.point_count, annotations.point_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        diagnostics.endpoint_count, diagnostics.endpoint_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        plan.pass_count, plan.pass_stride_bytes));
    bytes = SaturatingAdd(bytes, SaturatingProduct(
        plan.adjustment_lut_count, plan.adjustment_lut_stride_bytes));
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
        UINT sample_y = 0U) noexcept {
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

        font_fallback_used_ = false;
        ComPtr<ID2D1Image> adjusted_content;
        if (HasAdjustmentPass()) {
            const HRESULT adjusted_result = BuildAdjustedContent(adjusted_content);
            if (FAILED(adjusted_result)) {
                return adjusted_result;
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
            if (adjusted_content) {
                d2d_context_->DrawImage(adjusted_content.Get());
                result = S_OK;
            } else {
                result = DrawOrderedContent();
            }
            if (FAILED(result)) {
                d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
                d2d_context_->EndDraw();
                return result;
            }
            result = DrawVectorDiagnostics();
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
            result = DrawVanishingPoints();
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
            result = DrawAnnotationSelection();
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
            result = DrawFontFallbackWarning();
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
        InkpodSnapshotAnnotationView annotations{};
        annotations.struct_size = sizeof(annotations);
        InkpodSnapshotShootingFrameView shooting_frames{};
        shooting_frames.struct_size = sizeof(shooting_frames);
        InkpodSnapshotVanishingPointView vanishing_points{};
        vanishing_points.struct_size = sizeof(vanishing_points);
        InkpodSnapshotVectorDiagnostics diagnostics{};
        diagnostics.struct_size = sizeof(diagnostics);
        InkpodSnapshotRenderPlan render_plan{};
        render_plan.struct_size = sizeof(render_plan);
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
        const InkpodStatus annotation_status = vector_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_annotations(snapshot, &annotations)
            : vector_status;
        const InkpodStatus shooting_frame_status = annotation_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_shooting_frames(snapshot, &shooting_frames)
            : annotation_status;
        const InkpodStatus vanishing_point_status = shooting_frame_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_vanishing_points(snapshot, &vanishing_points)
            : shooting_frame_status;
        const InkpodStatus diagnostics_status = vanishing_point_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_vector_diagnostics(snapshot, &diagnostics)
            : vanishing_point_status;
        const InkpodStatus render_plan_status = diagnostics_status == INKPOD_STATUS_OK
            ? inkpod_snapshot_get_render_plan(snapshot, &render_plan)
            : diagnostics_status;
        if (view_status != INKPOD_STATUS_OK || transform_status != INKPOD_STATUS_OK
            || overlay_status != INKPOD_STATUS_OK || vector_status != INKPOD_STATUS_OK
            || diagnostics_status != INKPOD_STATUS_OK || annotation_status != INKPOD_STATUS_OK
            || shooting_frame_status != INKPOD_STATUS_OK
            || vanishing_point_status != INKPOD_STATUS_OK
            || render_plan_status != INKPOD_STATUS_OK
            || !ValidateOverlay(overlay) || !ValidateVectors(vectors)
            || !ValidateAnnotations(annotations)
            || !ValidateShootingFrames(shooting_frames)
            || !ValidateVanishingPoints(vanishing_points)
            || !ValidateVectorDiagnostics(diagnostics)
            || !ValidateRenderPlan(render_plan, view, vectors, annotations)) {
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
        annotations_ = annotations;
        shooting_frames_ = shooting_frames;
        vanishing_points_ = vanishing_points;
        vector_diagnostics_ = diagnostics;
        render_plan_ = render_plan;
        retained_snapshot_bytes_ = EstimateSnapshotPayloadBytes(snapshot);
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

    HRESULT SetAnnotationSelection(std::uint64_t object_id) noexcept {
        annotation_selection_id_ = object_id;
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
        annotations_ = {};
        vector_diagnostics_ = {};
        render_plan_ = {};
        tile_cache_.clear();
        retained_snapshot_bytes_ = 0U;
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
        return gpu_tile_bytes_;
    }

    [[nodiscard]] std::uint64_t SwapChainBytes() const noexcept {
        return static_cast<std::uint64_t>(surface_width_) * surface_height_ * 4U * 2U;
    }

    [[nodiscard]] std::uint64_t CachedTileCount() const noexcept {
        return static_cast<std::uint64_t>(tile_cache_.size());
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
        return bytes;
    }

    [[nodiscard]] std::optional<std::uint64_t> OldestInactiveUse() const noexcept {
        std::optional<std::uint64_t> oldest;
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
        if (victim == tile_cache_.end()) {
            return false;
        }
        gpu_tile_bytes_ -= victim->second.byte_count;
        tile_cache_.erase(victim);
        return true;
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

    HRESULT RenderAndReadPixelForSmokeTest(
        UINT x,
        UINT y,
        CanvasPixelRgba8& pixel) noexcept {
        return Render(&pixel, x, y);
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
                | INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE
                | INKPOD_SNAPSHOT_VECTOR_SQUARE_CROSS_SECTION;
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

    static bool ValidateVectorDiagnostics(
        const InkpodSnapshotVectorDiagnostics& diagnostics) noexcept {
        constexpr std::uint32_t known_flags = INKPOD_VECTOR_DIAGNOSTIC_ANTIALIAS
            | INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_VISIBLE
            | INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_ONLY
            | INKPOD_VECTOR_DIAGNOSTIC_ENDPOINTS_VISIBLE;
        if (diagnostics.feature_flags != INKPOD_FEATURE_NONE
            || (diagnostics.flags & ~known_flags) != 0U
            || ((diagnostics.flags & INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_ONLY) != 0U
                && (diagnostics.flags & INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_VISIBLE) == 0U)
            || diagnostics.endpoint_count > kMaximumVectorEndpoints
            || diagnostics.endpoint_stride_bytes < sizeof(InkpodSnapshotVectorEndpoint)
            || diagnostics.endpoint_stride_bytes
                    % alignof(InkpodSnapshotVectorEndpoint)
                != 0U
            || (diagnostics.endpoint_count != 0U && diagnostics.endpoints == nullptr)
            || (diagnostics.endpoint_count != 0U
                && (diagnostics.flags & INKPOD_VECTOR_DIAGNOSTIC_ENDPOINTS_VISIBLE) == 0U)
            || (diagnostics.endpoints != nullptr
                && reinterpret_cast<std::uintptr_t>(diagnostics.endpoints)
                        % alignof(InkpodSnapshotVectorEndpoint)
                    != 0U)
            || diagnostics.endpoint_stride_bytes
                > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
            || (diagnostics.endpoint_count > 1U
                && diagnostics.endpoint_stride_bytes
                    > static_cast<std::uint64_t>(std::numeric_limits<std::size_t>::max())
                        / (diagnostics.endpoint_count - 1U))) {
            return false;
        }
        const auto* bytes = reinterpret_cast<const std::byte*>(diagnostics.endpoints);
        std::uint64_t previous_path_id{};
        std::uint32_t previous_endpoint{};
        for (std::uint64_t index = 0U; index < diagnostics.endpoint_count; ++index) {
            const auto* endpoint = reinterpret_cast<const InkpodSnapshotVectorEndpoint*>(
                bytes + static_cast<std::size_t>(
                    index * diagnostics.endpoint_stride_bytes));
            if (endpoint->struct_size < sizeof(InkpodSnapshotVectorEndpoint)
                || endpoint->struct_size > diagnostics.endpoint_stride_bytes
                || (endpoint->endpoint != INKPOD_VECTOR_ENDPOINT_START
                    && endpoint->endpoint != INKPOD_VECTOR_ENDPOINT_END)
                || endpoint->path_id == 0U || endpoint->plane_id == 0U
                || !std::isfinite(endpoint->point.x)
                || !std::isfinite(endpoint->point.y)
                || std::abs(endpoint->point.x) > 2000000.0F
                || std::abs(endpoint->point.y) > 2000000.0F
                || (index != 0U
                    && (endpoint->path_id < previous_path_id
                        || (endpoint->path_id == previous_path_id
                            && endpoint->endpoint <= previous_endpoint)))) {
                return false;
            }
            previous_path_id = endpoint->path_id;
            previous_endpoint = endpoint->endpoint;
        }
        return true;
    }

    static bool ValidateAnnotations(const InkpodSnapshotAnnotationView& view) noexcept {
        constexpr std::uint32_t known_styles = INKPOD_ANNOTATION_STYLE_BOLD
            | INKPOD_ANNOTATION_STYLE_ITALIC | INKPOD_ANNOTATION_STYLE_UNDERLINE;
        if (view.abi_version != INKPOD_ABI_VERSION || view.feature_flags != 0U
            || view.object_count > kMaximumAnnotations
            || view.point_count > kMaximumAnnotationPoints
            || view.utf8_byte_count > kMaximumAnnotationUtf8Bytes
            || view.object_stride_bytes < sizeof(InkpodSnapshotAnnotation)
            || view.object_stride_bytes % alignof(InkpodSnapshotAnnotation) != 0U
            || view.point_stride_bytes < sizeof(InkpodAnnotationPoint)
            || view.point_stride_bytes % alignof(InkpodAnnotationPoint) != 0U
            || (view.object_count != 0U && view.objects == nullptr)
            || (view.point_count != 0U && view.points == nullptr)
            || (view.utf8_byte_count != 0U && view.utf8_bytes == nullptr)
            || view.object_stride_bytes > static_cast<std::uint64_t>(SIZE_MAX)
            || view.point_stride_bytes > static_cast<std::uint64_t>(SIZE_MAX)
            || (view.object_count > 1U && view.object_stride_bytes
                > static_cast<std::uint64_t>(SIZE_MAX) / (view.object_count - 1U))
            || (view.point_count > 1U && view.point_stride_bytes
                > static_cast<std::uint64_t>(SIZE_MAX) / (view.point_count - 1U))) {
            return false;
        }
        const auto range_is_valid = [](std::uint64_t first, std::uint64_t count,
                                        std::uint64_t total) noexcept {
            return first <= total && count <= total - first;
        };
        const auto* object_bytes = reinterpret_cast<const std::byte*>(view.objects);
        for (std::uint64_t index = 0U; index < view.object_count; ++index) {
            const auto* object = reinterpret_cast<const InkpodSnapshotAnnotation*>(
                object_bytes + static_cast<std::size_t>(index * view.object_stride_bytes));
            if (object->struct_size < sizeof(InkpodSnapshotAnnotation)
                || object->struct_size > view.object_stride_bytes || object->feature_flags != 0U
                || object->object_id == 0U || object->layer_id == 0U
                || object->bounds.width <= 0 || object->bounds.height <= 0
                || (object->style_flags & ~known_styles) != 0U
                || (object->kind < INKPOD_ANNOTATION_TEXT
                    || object->kind > INKPOD_ANNOTATION_VALUE)
                || (object->output != INKPOD_ANNOTATION_OUTPUT_NORMAL
                    && object->output != INKPOD_ANNOTATION_OUTPUT_INSTRUCTION)
                || !range_is_valid(
                    object->font_utf8_offset, object->font_utf8_bytes, view.utf8_byte_count)
                || !range_is_valid(
                    object->text_utf8_offset, object->text_utf8_bytes, view.utf8_byte_count)
                || !range_is_valid(object->first_point, object->point_count, view.point_count)) {
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
            && frame.visible <= 1U && frame.include_in_instruction_export <= 1U
            && frame.reserved == 0U
            && frame.anchor >= INKPOD_SHOOTING_FRAME_ANCHOR_TOP_LEFT
            && frame.anchor <= INKPOD_SHOOTING_FRAME_ANCHOR_BOTTOM_RIGHT;
    }

    static bool ValidateVanishingPoints(
        const InkpodSnapshotVanishingPointView& view) noexcept {
        if (view.abi_version != INKPOD_ABI_VERSION || view.feature_flags != 0U
            || view.point_count > 64U || view.radial_guide_count > 16384U
            || view.point_stride_bytes < sizeof(InkpodVanishingPointInfo)
            || view.point_stride_bytes % alignof(InkpodVanishingPointInfo) != 0U
            || view.radial_guide_stride_bytes < sizeof(InkpodSnapshotRadialGuide)
            || view.radial_guide_stride_bytes % alignof(InkpodSnapshotRadialGuide) != 0U
            || (view.point_count != 0U && view.points == nullptr)
            || (view.radial_guide_count != 0U && view.radial_guides == nullptr)) {
            return false;
        }
        const auto* point_bytes = reinterpret_cast<const std::byte*>(view.points);
        for (std::uint64_t index = 0U; index < view.point_count; ++index) {
            const auto* point = reinterpret_cast<const InkpodVanishingPointInfo*>(
                point_bytes + static_cast<std::size_t>(index * view.point_stride_bytes));
            if (point->struct_size < sizeof(InkpodVanishingPointInfo)
                || point->struct_size > view.point_stride_bytes
                || point->feature_flags != 0U || point->layer_id == 0U
                || point->interval_milli_degrees < 1000U
                || point->interval_milli_degrees > 180000U
                || point->angle_milli_degrees >= 180000U
                || point->opacity_milli > 1000U || point->visible > 1U
                || point->reserved != 0U) {
                return false;
            }
        }
        const auto* guide_bytes = reinterpret_cast<const std::byte*>(view.radial_guides);
        for (std::uint64_t index = 0U; index < view.radial_guide_count; ++index) {
            const auto* guide = reinterpret_cast<const InkpodSnapshotRadialGuide*>(
                guide_bytes + static_cast<std::size_t>(
                    index * view.radial_guide_stride_bytes));
            if (guide->struct_size < sizeof(InkpodSnapshotRadialGuide)
                || guide->struct_size > view.radial_guide_stride_bytes
                || guide->feature_flags != 0U
                || guide->angle_milli_degrees >= 180000U
                || guide->opacity_milli > 1000U || guide->reserved != 0U) {
                return false;
            }
        }
        return true;
    }

    static bool ValidateRenderPlan(
        const InkpodSnapshotRenderPlan& plan,
        const InkpodSnapshotView& view,
        const InkpodSnapshotVectorView& vectors,
        const InkpodSnapshotAnnotationView& annotations) noexcept {
        constexpr std::uint64_t adjustment_lut_bytes = 3U * 256U;
        if (plan.abi_version != INKPOD_ABI_VERSION || plan.feature_flags != 0U
            || plan.pass_count > kMaximumRenderPasses
            || plan.adjustment_lut_count > kMaximumAdjustmentLuts
            || plan.pass_stride_bytes < sizeof(InkpodSnapshotRenderPass)
            || plan.pass_stride_bytes % alignof(InkpodSnapshotRenderPass) != 0U
            || plan.adjustment_lut_stride_bytes != adjustment_lut_bytes
            || (plan.pass_count != 0U && plan.passes == nullptr)
            || (plan.adjustment_lut_count != 0U && plan.adjustment_luts_rgb8 == nullptr)
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
                case INKPOD_RENDER_PASS_VECTOR_FILLS:
                    if (active_layer == 0U || pass->layer_id != active_layer
                        || pass->plane_id == 0U || pass->item_count == 0U
                        || pass->opacity_milli != 1000U
                        || !range_is_valid(
                            pass->first_item, pass->item_count, vectors.fill_count)) {
                        return false;
                    }
                    break;
                case INKPOD_RENDER_PASS_VECTOR_STROKES:
                    if (active_layer == 0U || pass->layer_id != active_layer
                        || pass->plane_id == 0U || pass->item_count == 0U
                        || pass->opacity_milli != 1000U
                        || !range_is_valid(
                            pass->first_item, pass->item_count, vectors.segment_count)) {
                        return false;
                    }
                    break;
                case INKPOD_RENDER_PASS_ANNOTATIONS:
                    if (active_layer == 0U || pass->layer_id != active_layer
                        || pass->plane_id != 0U || pass->item_count == 0U
                        || pass->opacity_milli != 1000U
                        || !range_is_valid(
                            pass->first_item, pass->item_count, annotations.object_count)) {
                        return false;
                    }
                    break;
                case INKPOD_RENDER_PASS_ADJUSTMENT:
                    if (active_layer != 0U || pass->layer_id == 0U || pass->plane_id != 0U
                        || pass->opacity_milli != 1000U || pass->item_count != 1U
                        || !range_is_valid(pass->first_item, 1U, plan.adjustment_lut_count)) {
                        return false;
                    }
                    break;
                default:
                    return false;
            }
        }
        return active_layer == 0U;
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
        const D2D1_ANTIALIAS_MODE previous_antialias = d2d_context_->GetAntialiasMode();
        d2d_context_->SetAntialiasMode(
            (vector_diagnostics_.flags & INKPOD_VECTOR_DIAGNOSTIC_ANTIALIAS) != 0U
                ? D2D1_ANTIALIAS_MODE_PER_PRIMITIVE
                : D2D1_ANTIALIAS_MODE_ALIASED);
        d2d_context_->FillGeometry(geometry.Get(), brush);
        d2d_context_->SetAntialiasMode(previous_antialias);
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
            const bool square_cross_section =
                (path.flags & INKPOD_SNAPSHOT_VECTOR_SQUARE_CROSS_SECTION) != 0U;
            if (!closed && square_cross_section) {
                float start_x{};
                float start_y{};
                float end_x{};
                float end_y{};
                if (UnitDirection(centers[0], centers[1], start_x, start_y)) {
                    centers[0].x -= start_x * widths[0] * 0.5F;
                    centers[0].y -= start_y * widths[0] * 0.5F;
                }
                const std::size_t last = centers.size() - 1U;
                if (UnitDirection(centers[last - 1U], centers[last], end_x, end_y)) {
                    centers[last].x += end_x * widths[last] * 0.5F;
                    centers[last].y += end_y * widths[last] * 0.5F;
                }
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
                if (!square_cross_section) {
                    const float radius = widths.back() * 0.5F;
                    sink->AddArc(D2D1::ArcSegment(
                        right.back(),
                        D2D1::SizeF(radius, radius),
                        0.0F,
                        D2D1_SWEEP_DIRECTION_CLOCKWISE,
                        D2D1_ARC_SIZE_SMALL));
                }
                for (auto iterator = right.rbegin() + (square_cross_section ? 0U : 1U);
                     iterator != right.rend(); ++iterator) {
                    sink->AddLine(*iterator);
                }
                if (!square_cross_section) {
                    const float radius = widths.front() * 0.5F;
                    sink->AddArc(D2D1::ArcSegment(
                        left.front(),
                        D2D1::SizeF(radius, radius),
                        0.0F,
                        D2D1_SWEEP_DIRECTION_CLOCKWISE,
                        D2D1_ARC_SIZE_SMALL));
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
        const D2D1_ANTIALIAS_MODE previous_antialias = d2d_context_->GetAntialiasMode();
        d2d_context_->SetAntialiasMode(
            (vector_diagnostics_.flags & INKPOD_VECTOR_DIAGNOSTIC_ANTIALIAS) != 0U
                ? D2D1_ANTIALIAS_MODE_PER_PRIMITIVE
                : D2D1_ANTIALIAS_MODE_ALIASED);
        d2d_context_->FillGeometry(geometry.Get(), brush);
        d2d_context_->SetAntialiasMode(previous_antialias);
        return S_OK;
    }

    HRESULT CreateCenterlineGeometry(
        const VectorPathSpan& path,
        ComPtr<ID2D1PathGeometry>& geometry) noexcept {
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
        sink->BeginFigure(
            D2D1::Point2F(path.first->p0.x, path.first->p0.y),
            D2D1_FIGURE_BEGIN_HOLLOW);
        for (std::uint32_t index = 0U; index < path.count; ++index) {
            const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                reinterpret_cast<const std::byte*>(path.first)
                + static_cast<std::size_t>(index * vectors_.segment_stride_bytes));
            sink->AddBezier(D2D1::BezierSegment(
                D2D1::Point2F(segment->p1.x, segment->p1.y),
                D2D1::Point2F(segment->p2.x, segment->p2.y),
                D2D1::Point2F(segment->p3.x, segment->p3.y)));
        }
        sink->EndFigure(
            (path.flags & INKPOD_SNAPSHOT_VECTOR_CLOSED) != 0U
                ? D2D1_FIGURE_END_CLOSED
                : D2D1_FIGURE_END_OPEN);
        return sink->Close();
    }

    HRESULT BuildVectorPathMap(
        std::unordered_map<std::uint64_t, VectorPathSpan>& paths) const noexcept {
        try {
            paths.clear();
            paths.reserve(static_cast<std::size_t>(vectors_.segment_count));
            const auto* segment_bytes = reinterpret_cast<const std::byte*>(vectors_.segments);
            for (std::uint64_t index = 0; index < vectors_.segment_count;) {
                const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                    segment_bytes + static_cast<std::size_t>(index * vectors_.segment_stride_bytes));
                const VectorPathSpan span{
                    segment->path_id,
                    segment->z_order,
                    segment->flags,
                    segment,
                    segment->segment_count};
                if (!paths.emplace(span.id, span).second) {
                    return E_INVALIDARG;
                }
                index += span.count;
            }
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    static HRESULT Utf8ToWide(
        const std::uint8_t* bytes,
        std::uint64_t byte_count,
        std::wstring& output) noexcept {
        output.clear();
        if (byte_count == 0U) {
            return S_OK;
        }
        if (bytes == nullptr || byte_count > static_cast<std::uint64_t>(INT_MAX)) {
            return E_INVALIDARG;
        }
        const int count = MultiByteToWideChar(
            CP_UTF8, MB_ERR_INVALID_CHARS, reinterpret_cast<const char*>(bytes),
            static_cast<int>(byte_count), nullptr, 0);
        if (count <= 0) {
            return E_INVALIDARG;
        }
        try {
            output.resize(static_cast<std::size_t>(count));
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
        if (MultiByteToWideChar(
                CP_UTF8, MB_ERR_INVALID_CHARS, reinterpret_cast<const char*>(bytes),
                static_cast<int>(byte_count), output.data(), count) != count) {
            output.clear();
            return E_INVALIDARG;
        }
        return S_OK;
    }

    static D2D1_COLOR_F AnnotationColor(const InkpodColorValue& value) noexcept {
        const float maximum = value.depth == INKPOD_COLOR_DEPTH_16 ? 65535.0F : 255.0F;
        return D2D1::ColorF(
            static_cast<float>(value.red) / maximum,
            static_cast<float>(value.green) / maximum,
            static_cast<float>(value.blue) / maximum,
            static_cast<float>(value.alpha) / maximum);
    }

    HRESULT ResolveAnnotationFont(
        const InkpodSnapshotAnnotation& object,
        std::wstring& family) noexcept {
        HRESULT result = Utf8ToWide(
            annotations_.utf8_bytes + static_cast<std::size_t>(object.font_utf8_offset),
            object.font_utf8_bytes,
            family);
        if (FAILED(result)) {
            return result;
        }
        if (family.empty()) {
            family.assign(L"Segoe UI");
            return S_OK;
        }
        ComPtr<IDWriteFontCollection> fonts;
        result = shared_.DwriteFactory()->GetSystemFontCollection(&fonts, FALSE);
        if (FAILED(result)) {
            return result;
        }
        UINT32 family_index{};
        BOOL exists{};
        result = fonts->FindFamilyName(family.c_str(), &family_index, &exists);
        if (FAILED(result)) {
            return result;
        }
        if (exists == FALSE) {
            family.assign(L"Segoe UI");
            font_fallback_used_ = true;
        }
        return S_OK;
    }

    HRESULT AnnotationTextFormat(
        const InkpodSnapshotAnnotation& object,
        IDWriteTextFormat** output) noexcept {
        if (output == nullptr || shared_.DwriteFactory() == nullptr) {
            return E_INVALIDARG;
        }
        *output = nullptr;
        std::wstring family;
        HRESULT result = ResolveAnnotationFont(object, family);
        if (FAILED(result)) {
            return result;
        }
        std::wstring key;
        try {
            key = family + L"\x1f" + std::to_wstring(object.font_size_milli) + L"\x1f"
                + std::to_wstring(object.style_flags);
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
        const auto cached = text_format_cache_.find(key);
        if (cached != text_format_cache_.end()) {
            *output = cached->second.Get();
            (*output)->AddRef();
            return S_OK;
        }
        ComPtr<IDWriteTextFormat> format;
        result = shared_.DwriteFactory()->CreateTextFormat(
            family.c_str(),
            nullptr,
            (object.style_flags & INKPOD_ANNOTATION_STYLE_BOLD) != 0U
                ? DWRITE_FONT_WEIGHT_BOLD : DWRITE_FONT_WEIGHT_NORMAL,
            (object.style_flags & INKPOD_ANNOTATION_STYLE_ITALIC) != 0U
                ? DWRITE_FONT_STYLE_ITALIC : DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            static_cast<float>(object.font_size_milli) / 1000.0F,
            L"",
            &format);
        if (FAILED(result)) {
            return result;
        }
        try {
            if (text_format_cache_.size() >= 64U) {
                text_format_cache_.clear();
            }
            const auto [entry, inserted] = text_format_cache_.emplace(key, format);
            (void)inserted;
            *output = entry->second.Get();
            (*output)->AddRef();
            return S_OK;
        } catch (const std::bad_alloc&) {
            return E_OUTOFMEMORY;
        }
    }

    HRESULT DrawAnnotationText(
        const InkpodSnapshotAnnotation& object,
        ID2D1SolidColorBrush* brush) noexcept {
        std::wstring text;
        HRESULT result = Utf8ToWide(
            annotations_.utf8_bytes + static_cast<std::size_t>(object.text_utf8_offset),
            object.text_utf8_bytes,
            text);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<IDWriteTextFormat> format;
        result = AnnotationTextFormat(object, &format);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<IDWriteTextLayout> layout;
        result = shared_.DwriteFactory()->CreateTextLayout(
            text.data(), static_cast<UINT32>(text.size()), format.Get(),
            static_cast<float>(object.bounds.width),
            static_cast<float>(object.bounds.height),
            &layout);
        if (FAILED(result)) {
            return result;
        }
        if ((object.style_flags & INKPOD_ANNOTATION_STYLE_UNDERLINE) != 0U) {
            const DWRITE_TEXT_RANGE range{0U, static_cast<UINT32>(text.size())};
            result = layout->SetUnderline(TRUE, range);
            if (FAILED(result)) {
                return result;
            }
        }
        d2d_context_->DrawTextLayout(
            D2D1::Point2F(
                static_cast<float>(object.bounds.x), static_cast<float>(object.bounds.y)),
            layout.Get(), brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        return S_OK;
    }

    HRESULT DrawAnnotationGeometry(
        const InkpodSnapshotAnnotation& object,
        ID2D1SolidColorBrush* brush) noexcept {
        if (object.point_count < 2U) {
            return E_INVALIDARG;
        }
        const auto* point_bytes = reinterpret_cast<const std::byte*>(annotations_.points);
        const float width = static_cast<float>(object.stroke_width_milli) / 1000.0F;
        const auto point_at = [&](std::uint64_t offset) noexcept {
            const auto* point = reinterpret_cast<const InkpodAnnotationPoint*>(
                point_bytes + static_cast<std::size_t>(
                    (object.first_point + offset) * annotations_.point_stride_bytes));
            return D2D1::Point2F(
                static_cast<float>(point->x_milli) / 1000.0F,
                static_cast<float>(point->y_milli) / 1000.0F);
        };
        D2D1_POINT_2F previous = point_at(0U);
        for (std::uint64_t index = 1U; index < object.point_count; ++index) {
            const D2D1_POINT_2F current = point_at(index);
            d2d_context_->DrawLine(previous, current, brush, width);
            previous = current;
        }
        return S_OK;
    }

    HRESULT DrawAnnotationPass(
        const InkpodSnapshotRenderPass& pass,
        ID2D1SolidColorBrush* brush) noexcept {
        const auto* object_bytes = reinterpret_cast<const std::byte*>(annotations_.objects);
        for (std::uint64_t offset = 0U; offset < pass.item_count; ++offset) {
            const auto* object = reinterpret_cast<const InkpodSnapshotAnnotation*>(
                object_bytes + static_cast<std::size_t>(
                    (pass.first_item + offset) * annotations_.object_stride_bytes));
            brush->SetColor(AnnotationColor(object->color));
            HRESULT result = S_OK;
            if (object->kind == INKPOD_ANNOTATION_TEXT
                || object->kind == INKPOD_ANNOTATION_VALUE) {
                result = DrawAnnotationText(*object, brush);
            }
            if (SUCCEEDED(result) && (object->kind == INKPOD_ANNOTATION_STROKE
                    || object->kind == INKPOD_ANNOTATION_LEADER
                    || object->kind == INKPOD_ANNOTATION_VALUE)) {
                result = DrawAnnotationGeometry(*object, brush);
            }
            if (FAILED(result)) {
                return result;
            }
        }
        return S_OK;
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

    HRESULT DrawVanishingPoints() noexcept {
        if (vanishing_points_.point_count == 0U
            && vanishing_points_.radial_guide_count == 0U) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> brush;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(1.0F, 1.0F, 1.0F, 1.0F), &brush);
        if (FAILED(result)) {
            return result;
        }
        const float width = static_cast<float>(std::max(1.0, 1.0 / transform_.zoom));
        const auto* guide_bytes = reinterpret_cast<const std::byte*>(
            vanishing_points_.radial_guides);
        for (std::uint64_t index = 0U;
             index < vanishing_points_.radial_guide_count; ++index) {
            const auto* guide = reinterpret_cast<const InkpodSnapshotRadialGuide*>(
                guide_bytes + static_cast<std::size_t>(
                    index * vanishing_points_.radial_guide_stride_bytes));
            D2D1_COLOR_F color = AnnotationColor(guide->color);
            color.a *= static_cast<float>(guide->opacity_milli) / 1000.0F;
            brush->SetColor(color);
            d2d_context_->DrawLine(
                D2D1::Point2F(
                    static_cast<float>(guide->start_x_milli) / 1000.0F,
                    static_cast<float>(guide->start_y_milli) / 1000.0F),
                D2D1::Point2F(
                    static_cast<float>(guide->end_x_milli) / 1000.0F,
                    static_cast<float>(guide->end_y_milli) / 1000.0F),
                brush.Get(), width);
        }
        const auto* point_bytes = reinterpret_cast<const std::byte*>(
            vanishing_points_.points);
        const float radius = static_cast<float>(std::max(3.0, 5.0 / transform_.zoom));
        for (std::uint64_t index = 0U; index < vanishing_points_.point_count; ++index) {
            const auto* point = reinterpret_cast<const InkpodVanishingPointInfo*>(
                point_bytes + static_cast<std::size_t>(
                    index * vanishing_points_.point_stride_bytes));
            D2D1_COLOR_F color = AnnotationColor(point->color);
            color.a *= static_cast<float>(point->opacity_milli) / 1000.0F;
            brush->SetColor(color);
            const D2D1_POINT_2F center = D2D1::Point2F(
                static_cast<float>(point->x_milli) / 1000.0F,
                static_cast<float>(point->y_milli) / 1000.0F);
            d2d_context_->DrawLine(
                D2D1::Point2F(center.x - radius, center.y),
                D2D1::Point2F(center.x + radius, center.y), brush.Get(), width);
            d2d_context_->DrawLine(
                D2D1::Point2F(center.x, center.y - radius),
                D2D1::Point2F(center.x, center.y + radius), brush.Get(), width);
            d2d_context_->DrawEllipse(
                D2D1::Ellipse(center, radius, radius), brush.Get(), width);
        }
        return S_OK;
    }

    HRESULT DrawAnnotationSelection() noexcept {
        if (annotation_selection_id_ == 0U || annotations_.objects == nullptr) {
            return S_OK;
        }
        const auto* object_bytes = reinterpret_cast<const std::byte*>(annotations_.objects);
        const InkpodSnapshotAnnotation* selected{};
        for (std::uint64_t index = 0U; index < annotations_.object_count; ++index) {
            const auto* object = reinterpret_cast<const InkpodSnapshotAnnotation*>(
                object_bytes + static_cast<std::size_t>(
                    index * annotations_.object_stride_bytes));
            if (object->object_id == annotation_selection_id_) {
                selected = object;
                break;
            }
        }
        if (selected == nullptr) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> shadow;
        ComPtr<ID2D1SolidColorBrush> foreground;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.02F, 0.05F, 0.08F, 0.75F), &shadow);
        if (SUCCEEDED(result)) {
            result = d2d_context_->CreateSolidColorBrush(
                D2D1::ColorF(0.1F, 0.85F, 1.0F, 1.0F), &foreground);
        }
        if (FAILED(result)) {
            return result;
        }
        const float stroke_width = static_cast<float>(std::max(1.0, 1.5 / transform_.zoom));
        const float shadow_width = stroke_width * 2.5F;
        const float handle_radius = static_cast<float>(std::max(2.0, 3.0 / transform_.zoom));
        const auto bounds = D2D1::RectF(
            static_cast<float>(selected->bounds.x),
            static_cast<float>(selected->bounds.y),
            static_cast<float>(selected->bounds.x + selected->bounds.width),
            static_cast<float>(selected->bounds.y + selected->bounds.height));
        d2d_context_->DrawRectangle(bounds, shadow.Get(), shadow_width);
        d2d_context_->DrawRectangle(bounds, foreground.Get(), stroke_width);
        const std::array<D2D1_POINT_2F, 4U> corners{
            D2D1::Point2F(bounds.left, bounds.top),
            D2D1::Point2F(bounds.right, bounds.top),
            D2D1::Point2F(bounds.right, bounds.bottom),
            D2D1::Point2F(bounds.left, bounds.bottom)};
        for (const auto corner : corners) {
            d2d_context_->FillEllipse(
                D2D1::Ellipse(corner, handle_radius * 1.5F, handle_radius * 1.5F),
                shadow.Get());
            d2d_context_->FillEllipse(
                D2D1::Ellipse(corner, handle_radius, handle_radius), foreground.Get());
        }
        return S_OK;
    }

    HRESULT DrawFontFallbackWarning() noexcept {
        if (!font_fallback_used_) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> background;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(1.0F, 0.78F, 0.12F, 0.96F), &background);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<ID2D1SolidColorBrush> foreground;
        result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.08F, 0.08F, 0.08F, 1.0F), &foreground);
        if (FAILED(result)) {
            return result;
        }
        constexpr D2D1_RECT_F bounds{8.0F, 8.0F, 260.0F, 34.0F};
        d2d_context_->FillRectangle(bounds, background.Get());
        ComPtr<IDWriteTextFormat> format;
        result = shared_.DwriteFactory()->CreateTextFormat(
            L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 13.0F, L"", &format);
        if (FAILED(result)) {
            return result;
        }
        constexpr wchar_t message[] = L"Font fallback: Segoe UI";
        d2d_context_->DrawTextW(
            message, static_cast<UINT32>(std::size(message) - 1U), format.Get(), bounds,
            foreground.Get(), D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
        return S_OK;
    }

    HRESULT DrawRenderPass(
        const InkpodSnapshotRenderPass& pass,
        const std::unordered_map<std::uint64_t, VectorPathSpan>& paths,
        ID2D1SolidColorBrush* brush,
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
            case INKPOD_RENDER_PASS_VECTOR_FILLS: {
                const auto* fill_bytes = reinterpret_cast<const std::byte*>(vectors_.fills);
                for (std::uint64_t offset = 0; offset < pass.item_count; ++offset) {
                    const auto* fill = reinterpret_cast<const InkpodSnapshotVectorFill*>(
                        fill_bytes + static_cast<std::size_t>(
                            (pass.first_item + offset) * vectors_.fill_stride_bytes));
                    if ((fill->color_rgba & 0xffU) != 0U) {
                        const HRESULT result = DrawFillGeometry(*fill, paths, brush);
                        if (FAILED(result)) {
                            return result;
                        }
                    }
                }
                return S_OK;
            }
            case INKPOD_RENDER_PASS_VECTOR_STROKES: {
                if ((vector_diagnostics_.flags
                        & INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_ONLY)
                    != 0U) {
                    return S_OK;
                }
                const auto* segment_bytes = reinterpret_cast<const std::byte*>(vectors_.segments);
                std::uint64_t index = pass.first_item;
                const std::uint64_t end = pass.first_item + pass.item_count;
                while (index < end) {
                    const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                        segment_bytes
                        + static_cast<std::size_t>(index * vectors_.segment_stride_bytes));
                    const VectorPathSpan path{
                        segment->path_id,
                        segment->z_order,
                        segment->flags,
                        segment,
                        segment->segment_count};
                    if (path.count == 0U || path.count > end - index) {
                        return E_INVALIDARG;
                    }
                    if ((path.flags & INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE) != 0U
                        && (path.first->color_rgba & 0xffU) != 0U) {
                        const HRESULT result = DrawStrokeGeometry(path, brush);
                        if (FAILED(result)) {
                            return result;
                        }
                    }
                    index += path.count;
                }
                return S_OK;
            }
            case INKPOD_RENDER_PASS_ANNOTATIONS:
                return DrawAnnotationPass(pass, brush);
            case INKPOD_RENDER_PASS_ADJUSTMENT:
                return E_UNEXPECTED;
            default:
                return E_INVALIDARG;
        }
    }

    [[nodiscard]] bool HasAdjustmentPass() const noexcept {
        const auto* pass_bytes = reinterpret_cast<const std::byte*>(render_plan_.passes);
        for (std::uint64_t index = 0; index < render_plan_.pass_count; ++index) {
            const auto* pass = reinterpret_cast<const InkpodSnapshotRenderPass*>(
                pass_bytes + static_cast<std::size_t>(index * render_plan_.pass_stride_bytes));
            if (pass->kind == INKPOD_RENDER_PASS_ADJUSTMENT) {
                return true;
            }
        }
        return false;
    }

    HRESULT DrawOrderedContent() noexcept {
        std::unordered_map<std::uint64_t, VectorPathSpan> paths;
        HRESULT result = BuildVectorPathMap(paths);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<ID2D1SolidColorBrush> brush;
        result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(D2D1::ColorF::Black), &brush);
        if (FAILED(result)) {
            return result;
        }
        bool layer_active{};
        const auto* pass_bytes = reinterpret_cast<const std::byte*>(render_plan_.passes);
        for (std::uint64_t index = 0; index < render_plan_.pass_count; ++index) {
            const auto* pass = reinterpret_cast<const InkpodSnapshotRenderPass*>(
                pass_bytes + static_cast<std::size_t>(index * render_plan_.pass_stride_bytes));
            result = DrawRenderPass(*pass, paths, brush.Get(), layer_active);
            if (FAILED(result)) {
                if (layer_active) {
                    d2d_context_->PopLayer();
                }
                return result;
            }
        }
        return layer_active ? E_INVALIDARG : S_OK;
    }

    HRESULT SetAdjustmentEffectTables(
        ID2D1Effect& effect,
        const InkpodSnapshotRenderPass& pass) const noexcept {
        const auto* lut = render_plan_.adjustment_luts_rgb8
            + static_cast<std::size_t>(
                pass.first_item * render_plan_.adjustment_lut_stride_bytes);
        std::array<std::array<float, 256>, 3> tables{};
        for (std::size_t channel = 0; channel < tables.size(); ++channel) {
            for (std::size_t value = 0; value < tables[channel].size(); ++value) {
                tables[channel][value] = static_cast<float>(lut[channel * 256U + value]) / 255.0F;
            }
        }
        constexpr std::array<D2D1_TABLETRANSFER_PROP, 3> properties{
            D2D1_TABLETRANSFER_PROP_RED_TABLE,
            D2D1_TABLETRANSFER_PROP_GREEN_TABLE,
            D2D1_TABLETRANSFER_PROP_BLUE_TABLE};
        for (std::size_t channel = 0; channel < tables.size(); ++channel) {
            const HRESULT result = effect.SetValue(
                properties[channel],
                D2D1_PROPERTY_TYPE_BLOB,
                reinterpret_cast<const BYTE*>(tables[channel].data()),
                static_cast<UINT32>(sizeof(tables[channel])));
            if (FAILED(result)) {
                return result;
            }
        }
        HRESULT result = effect.SetValue(D2D1_TABLETRANSFER_PROP_RED_DISABLE, FALSE);
        if (SUCCEEDED(result)) {
            result = effect.SetValue(D2D1_TABLETRANSFER_PROP_GREEN_DISABLE, FALSE);
        }
        if (SUCCEEDED(result)) {
            result = effect.SetValue(D2D1_TABLETRANSFER_PROP_BLUE_DISABLE, FALSE);
        }
        if (SUCCEEDED(result)) {
            result = effect.SetValue(D2D1_TABLETRANSFER_PROP_ALPHA_DISABLE, TRUE);
        }
        if (SUCCEEDED(result)) {
            result = effect.SetValue(D2D1_TABLETRANSFER_PROP_CLAMP_OUTPUT, TRUE);
        }
        return result;
    }

    HRESULT BuildAdjustedContent(ComPtr<ID2D1Image>& output) noexcept {
        output.Reset();
        ComPtr<ID2D1Image> original_target;
        d2d_context_->GetTarget(&original_target);
        std::unordered_map<std::uint64_t, VectorPathSpan> paths;
        HRESULT result = BuildVectorPathMap(paths);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<ID2D1SolidColorBrush> brush;
        result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(D2D1::ColorF::Black), &brush);
        if (FAILED(result)) {
            return result;
        }
        ComPtr<ID2D1CommandList> current;
        result = d2d_context_->CreateCommandList(&current);
        if (FAILED(result)) {
            return result;
        }
        d2d_context_->SetTarget(current.Get());
        d2d_context_->BeginDraw();
        bool recording = true;
        d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
        if ((snapshot_view_.feature_flags & INKPOD_SNAPSHOT_FEATURE_SOLID_WHITE_BASE) != 0U) {
            brush->SetColor(D2D1::ColorF(D2D1::ColorF::White));
            d2d_context_->FillRectangle(
                D2D1::RectF(
                    0.0F,
                    0.0F,
                    static_cast<float>(transform_.document_width),
                    static_cast<float>(transform_.document_height)),
                brush.Get());
        }
        bool layer_active{};
        const auto* pass_bytes = reinterpret_cast<const std::byte*>(render_plan_.passes);
        for (std::uint64_t index = 0; index < render_plan_.pass_count; ++index) {
            const auto* pass = reinterpret_cast<const InkpodSnapshotRenderPass*>(
                pass_bytes + static_cast<std::size_t>(index * render_plan_.pass_stride_bytes));
            if (pass->kind != INKPOD_RENDER_PASS_ADJUSTMENT) {
                result = DrawRenderPass(*pass, paths, brush.Get(), layer_active);
                if (FAILED(result)) {
                    break;
                }
                continue;
            }
            if (layer_active) {
                result = E_INVALIDARG;
                break;
            }
            result = d2d_context_->EndDraw();
            recording = false;
            if (SUCCEEDED(result)) {
                result = current->Close();
            }
            ComPtr<ID2D1Effect> effect;
            if (SUCCEEDED(result)) {
                result = d2d_context_->CreateEffect(CLSID_D2D1TableTransfer, &effect);
            }
            if (SUCCEEDED(result)) {
                effect->SetInput(0U, current.Get());
                result = SetAdjustmentEffectTables(*effect.Get(), *pass);
            }
            ComPtr<ID2D1CommandList> next;
            if (SUCCEEDED(result)) {
                result = d2d_context_->CreateCommandList(&next);
            }
            if (FAILED(result)) {
                break;
            }
            d2d_context_->SetTarget(next.Get());
            d2d_context_->BeginDraw();
            recording = true;
            d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
            d2d_context_->DrawImage(effect.Get());
            current = next;
        }
        if (SUCCEEDED(result)) {
            if (layer_active) {
                d2d_context_->PopLayer();
                result = E_INVALIDARG;
            }
            const HRESULT end_result = recording ? d2d_context_->EndDraw() : S_OK;
            recording = false;
            if (SUCCEEDED(result)) {
                result = end_result;
            }
            if (SUCCEEDED(result)) {
                result = current->Close();
            }
        } else {
            if (layer_active && recording) {
                d2d_context_->PopLayer();
            }
            if (recording) {
                d2d_context_->EndDraw();
            }
        }
        d2d_context_->SetTarget(original_target.Get());
        d2d_context_->SetTransform(D2D1::Matrix3x2F::Identity());
        if (SUCCEEDED(result)) {
            output = current;
        }
        return result;
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

    HRESULT DrawVectorDiagnostics() noexcept {
        if ((vector_diagnostics_.flags
                & (INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_VISIBLE
                    | INKPOD_VECTOR_DIAGNOSTIC_ENDPOINTS_VISIBLE))
            == 0U) {
            return S_OK;
        }
        ComPtr<ID2D1SolidColorBrush> brush;
        HRESULT result = d2d_context_->CreateSolidColorBrush(
            D2D1::ColorF(0.95F, 0.15F, 0.55F, 0.95F), &brush);
        if (FAILED(result)) {
            return result;
        }
        const D2D1_ANTIALIAS_MODE previous_antialias = d2d_context_->GetAntialiasMode();
        d2d_context_->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        if ((vector_diagnostics_.flags
                & INKPOD_VECTOR_DIAGNOSTIC_CENTERLINE_VISIBLE)
            != 0U) {
            const float width = static_cast<float>(1.0 / transform_.zoom);
            const auto* bytes = reinterpret_cast<const std::byte*>(vectors_.segments);
            for (std::uint64_t index = 0U; index < vectors_.segment_count;) {
                const auto* segment = reinterpret_cast<const InkpodSnapshotVectorSegment*>(
                    bytes + static_cast<std::size_t>(index * vectors_.segment_stride_bytes));
                const VectorPathSpan path{
                    segment->path_id,
                    segment->z_order,
                    segment->flags,
                    segment,
                    segment->segment_count};
                if ((path.flags & INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE) == 0U) {
                    index += path.count;
                    continue;
                }
                ComPtr<ID2D1PathGeometry> geometry;
                result = CreateCenterlineGeometry(path, geometry);
                if (FAILED(result)) {
                    d2d_context_->SetAntialiasMode(previous_antialias);
                    return result;
                }
                d2d_context_->DrawGeometry(geometry.Get(), brush.Get(), width);
                index += path.count;
            }
        }
        if ((vector_diagnostics_.flags & INKPOD_VECTOR_DIAGNOSTIC_ENDPOINTS_VISIBLE)
            != 0U) {
            brush->SetColor(D2D1::ColorF(0.95F, 0.2F, 0.15F, 1.0F));
            const float radius = static_cast<float>(4.0 / transform_.zoom);
            const auto* bytes = reinterpret_cast<const std::byte*>(
                vector_diagnostics_.endpoints);
            for (std::uint64_t index = 0U; index < vector_diagnostics_.endpoint_count;
                 ++index) {
                const auto* endpoint = reinterpret_cast<const InkpodSnapshotVectorEndpoint*>(
                    bytes + static_cast<std::size_t>(
                        index * vector_diagnostics_.endpoint_stride_bytes));
                d2d_context_->FillEllipse(
                    D2D1::Ellipse(
                        D2D1::Point2F(endpoint->point.x, endpoint->point.y),
                        radius,
                        radius),
                    brush.Get());
            }
        }
        d2d_context_->SetAntialiasMode(previous_antialias);
        return S_OK;
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
        while (gpu_tile_bytes_ > tile_budget_bytes_) {
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
            if (victim == tile_cache_.end()) {
                break;
            }
            gpu_tile_bytes_ -= victim->second.byte_count;
            tile_cache_.erase(victim);
        }
    }

    HRESULT RebuildTileCache() noexcept {
        if (!d2d_context_) {
            return E_UNEXPECTED;
        }
        if (snapshot_ == nullptr) {
            tile_cache_.clear();
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
            for (auto& entry : tile_cache_) {
                entry.second.active = false;
            }
            std::uint64_t upload_bytes{};
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
                    upload_bytes = SaturatingAdd(upload_bytes, tile->pixel_bytes);
                }
            }
            while (upload_bytes <= tile_budget_bytes_
                && gpu_tile_bytes_ > tile_budget_bytes_ - upload_bytes) {
                if (!EvictOldestInactive()) {
                    return E_OUTOFMEMORY;
                }
            }
            if (upload_bytes > tile_budget_bytes_) {
                return E_OUTOFMEMORY;
            }
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
            }
            TrimTileCache();
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
            const D2D1_SIZE_U size = target_bitmap_->GetPixelSize();
            surface_width_ = size.width;
            surface_height_ = size.height;
        }
        return result;
    }

    void DiscardSurfaceResources() noexcept {
        if (frame_latency_waitable_ != nullptr) {
            CloseHandle(frame_latency_waitable_);
            frame_latency_waitable_ = nullptr;
        }
        tile_cache_.clear();
        gpu_tile_bytes_ = 0U;
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
    InkpodSnapshotView snapshot_view_{};
    InkpodSnapshotTransform transform_{};
    InkpodSnapshotOverlay overlay_{};
    InkpodSnapshotVectorView vectors_{};
    InkpodSnapshotAnnotationView annotations_{};
    InkpodSnapshotShootingFrameView shooting_frames_{};
    InkpodSnapshotVanishingPointView vanishing_points_{};
    InkpodSnapshotVectorDiagnostics vector_diagnostics_{};
    InkpodSnapshotRenderPlan render_plan_{};
    std::unordered_map<std::wstring, ComPtr<IDWriteTextFormat>> text_format_cache_;
    bool font_fallback_used_{};
    CanvasFloatingPreview floating_preview_{};
    CanvasGeometryPreview geometry_preview_{};
    std::uint64_t annotation_selection_id_{};
    std::unordered_map<std::uint64_t, CachedTile> tile_cache_;
    ComPtr<IDXGISwapChain1> swap_chain_;
    ComPtr<ID2D1DeviceContext> d2d_context_;
    ComPtr<ID2D1Bitmap1> target_bitmap_;
    HANDLE frame_latency_waitable_{};
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
    ValidateClosedVectorStroke,
    ReadPixel,
    GetDocumentBounds,
    GetGeometryPreview,
    SetFloatingPreview,
    SetGeometryPreview,
    SetAnnotationSelection,
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
    std::uint64_t annotation_object_id{};
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
    std::uint64_t retained_snapshot_bytes{};
    std::uint64_t gpu_tile_bytes{};
    std::uint64_t swap_chain_bytes{};
    std::uint64_t cached_tile_count{};
    std::uint64_t active_tile_count{};
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
                in_flight_work_ = 0U;
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
        usage.queued_work_count = static_cast<std::uint64_t>(work_.size());
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
            found->occluded};
        return true;
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
            return stopping_ || (work_.empty() && in_flight_work_ == 0U);
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
            surface.presented_frames,
            surface.surface->RetainedSnapshotBytes(),
            surface.surface->GpuTileBytes(),
            surface.surface->SwapChainBytes(),
            surface.surface->CachedTileCount(),
            surface.surface->ActiveTileCount()};
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
        for (auto& surface : surfaces_) {
            surface.surface->SetTileBudgetBytes(kApplicationGpuTileBudgetBytes);
        }
        for (;;) {
            std::uint64_t total_bytes{};
            for (const auto& surface : surfaces_) {
                total_bytes = SaturatingAdd(
                    total_bytes, surface.surface->GpuTileBytes());
            }
            if (total_bytes <= kApplicationGpuTileBudgetBytes) {
                return;
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
                return;
            }
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
        }
        UpdateTileBudgets();
        for (const auto& surface : surfaces_) {
            PublishSurface(surface);
        }
        device_reset_count_.fetch_add(1U, std::memory_order_relaxed);
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
        HRESULT result = found->surface->SetSnapshot(envelope.snapshot);
        envelope.snapshot = nullptr;
        result = NormalizeResult(*found, result);
        if (SUCCEEDED(result)) {
            UpdateTileBudgets();
            for (const auto& surface : surfaces_) {
                PublishSurface(surface);
            }
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
            case HostControlKind::ReadPixel:
                if (control.out_pixel == nullptr) {
                    result = E_POINTER;
                } else {
                    result = surface.surface->RenderAndReadPixelForSmokeTest(
                        control.pixel_x, control.pixel_y, *control.out_pixel);
                    if (SUCCEEDED(result)) {
                        ++surface.presented_frames;
                    }
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
                render = surface.visible;
                break;
            case HostControlKind::SetGeometryPreview:
                result = surface.surface->SetGeometryPreview(control.geometry_preview);
                render = surface.visible;
                break;
            case HostControlKind::SetAnnotationSelection:
                result = surface.surface->SetAnnotationSelection(
                    control.annotation_object_id);
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
                ++in_flight_work_;
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
            {
                std::lock_guard lock(mutex_);
                --in_flight_work_;
                if (work_.empty() && in_flight_work_ == 0U) {
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
    bool stopping_{true};
    bool running_{};
    bool queue_paused_for_smoke_test_{};
    std::size_t in_flight_work_{};
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

HRESULT RendererHost::SetAnnotationSelection(
    app::CanvasId canvas,
    app::Generation surface_generation,
    std::uint64_t object_id) noexcept {
    if (impl_ == nullptr) {
        return E_UNEXPECTED;
    }
    HostControl control{};
    control.kind = HostControlKind::SetAnnotationSelection;
    control.canvas = canvas;
    control.surface_generation = surface_generation;
    control.annotation_object_id = object_id;
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

bool SetCanvasAnnotationSelection(HWND canvas, std::uint64_t object_id) noexcept {
    auto* host = reinterpret_cast<CanvasHost*>(
        GetWindowLongPtrW(canvas, GWLP_USERDATA));
    return host != nullptr
        && SUCCEEDED(host->Renderer().SetAnnotationSelection(
            host->Canvas(), host->SurfaceGeneration(), object_id));
}

}  // namespace inkpod::renderer
