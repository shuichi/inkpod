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
constexpr std::uint64_t kMaximumSnapshotGuides = 4096U;
constexpr std::uint64_t kMaximumVectorSegments = 262144U;
constexpr std::uint64_t kMaximumVectorFills = 65536U;
constexpr std::uint64_t kMaximumVectorBoundaries = 262144U;
constexpr std::uint64_t kMaximumOverlayLines = 8192U;
constexpr std::size_t kMaximumPointerHistory = 256U;
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
            return CreateDeviceResources();
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
        DiscardDeviceResources();
        return RecreateAfterDeviceLoss();
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
        HRESULT result = d2d_factory_->CreatePathGeometry(&geometry);
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
            HRESULT result = d2d_factory_->CreatePathGeometry(&geometry);
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
    InkpodSnapshotOverlay overlay_{};
    InkpodSnapshotVectorView vectors_{};
    CanvasFloatingPreview floating_preview_{};
    CanvasGeometryPreview geometry_preview_{};
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
    ValidateClosedVectorStroke,
    GetDocumentBounds,
    GetGeometryPreview,
    SetFloatingPreview,
    SetGeometryPreview,
};

struct RenderControl {
    RenderControlKind kind{};
    std::shared_ptr<std::promise<HRESULT>> completion;
    CanvasDocumentBounds* out_bounds{};
    CanvasGeometryPreview* out_geometry_preview{};
    CanvasFloatingPreview floating_preview{};
    CanvasGeometryPreview geometry_preview{};
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
        CanvasDocumentBounds* out_bounds = nullptr,
        const CanvasFloatingPreview* floating_preview = nullptr,
        const CanvasGeometryPreview* geometry_preview = nullptr,
        CanvasGeometryPreview* out_geometry_preview = nullptr) noexcept {
        try {
            auto completion = std::make_shared<std::promise<HRESULT>>();
            auto future = completion->get_future();
            {
                std::lock_guard lock(mutex_);
                if (stopping_) {
                    return E_UNEXPECTED;
                }
                RenderControl control{};
                control.kind = kind;
                control.completion = completion;
                control.out_bounds = out_bounds;
                control.out_geometry_preview = out_geometry_preview;
                if (floating_preview != nullptr) {
                    control.floating_preview = *floating_preview;
                }
                if (geometry_preview != nullptr) {
                    control.geometry_preview = *geometry_preview;
                }
                controls_.push_back(control);
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
                        case RenderControlKind::ValidateClosedVectorStroke:
                            control_result = renderer.ValidateClosedVectorStrokeForSmokeTest();
                            break;
                        case RenderControlKind::GetDocumentBounds:
                            if (control.out_bounds == nullptr) {
                                control_result = E_POINTER;
                            } else {
                                *control.out_bounds = renderer.DocumentBounds();
                            }
                            break;
                        case RenderControlKind::GetGeometryPreview:
                            if (control.out_geometry_preview == nullptr) {
                                control_result = E_POINTER;
                            } else {
                                control_result = renderer.GetGeometryPreviewForSmokeTest(
                                    *control.out_geometry_preview);
                            }
                            break;
                        case RenderControlKind::SetFloatingPreview:
                            control_result = renderer.SetFloatingPreview(
                                control.floating_preview);
                            render = SUCCEEDED(control_result);
                            break;
                        case RenderControlKind::SetGeometryPreview:
                            control_result = renderer.SetGeometryPreview(
                                control.geometry_preview);
                            render = SUCCEEDED(control_result);
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
                    0,
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
                SetFocus(window);
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
        case kCanvasValidateClosedVectorStroke:
            return host != nullptr
                    && SUCCEEDED(host->Renderer().Invoke(
                        RenderControlKind::ValidateClosedVectorStroke))
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
        case kCanvasGetGeometryPreviewForSmokeTest: {
            auto* preview = reinterpret_cast<CanvasGeometryPreview*>(lparam);
            return host != nullptr && preview != nullptr
                    && SUCCEEDED(host->Renderer().Invoke(
                        RenderControlKind::GetGeometryPreview,
                        nullptr,
                        nullptr,
                        nullptr,
                        preview))
                ? 1
                : 0;
        }
        case kCanvasSetFloatingPreview: {
            const auto* preview = reinterpret_cast<const CanvasFloatingPreview*>(lparam);
            return host != nullptr && preview != nullptr
                    && SUCCEEDED(host->Renderer().Invoke(
                        RenderControlKind::SetFloatingPreview, nullptr, preview))
                ? 1
                : 0;
        }
        case kCanvasSetGeometryPreview: {
            const auto* preview = reinterpret_cast<const CanvasGeometryPreview*>(lparam);
            return host != nullptr && preview != nullptr
                    && SUCCEEDED(host->Renderer().Invoke(
                        RenderControlKind::SetGeometryPreview, nullptr, nullptr, preview))
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
