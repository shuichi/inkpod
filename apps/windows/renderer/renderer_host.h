#pragma once

#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <memory>

#include "app/identity.h"
#include "inkpod/core_ffi.h"

namespace inkpod::renderer {

inline constexpr std::size_t kCanvasGeometryPreviewPoints = 128U;

struct CanvasDocumentBounds {
    double left;
    double top;
    double right;
    double bottom;
};

struct CanvasPixelRgba8 {
    std::uint8_t red{};
    std::uint8_t green{};
    std::uint8_t blue{};
    std::uint8_t alpha{};
};

struct CanvasFloatingPreview {
    std::uint32_t struct_size;
    std::uint32_t active;
    InkpodFrameRect bounds;
    InkpodFloatingTransform transform;
};

struct CanvasGeometryPoint {
    float x{};
    float y{};
};

struct CanvasGeometryPreview {
    std::uint32_t struct_size;
    std::uint32_t active;
    std::uint32_t point_count;
    std::uint32_t closed;
    float stroke_width;
    std::uint32_t reserved;
    CanvasGeometryPoint points[kCanvasGeometryPreviewPoints];
    // Display-only band preview; zero keeps the existing outline path.
    std::uint32_t brush_shape{};
    float point_diameters[kCanvasGeometryPreviewPoints]{};
};

enum class SnapshotOwnerKind : std::uint8_t {
    Document,
    Auxiliary,
};

enum class CanvasScrollRangeHint : std::uint8_t {
    Preserve,
    ResetToBase,
};

struct SnapshotRoute {
    app::DocumentSessionId document_session;
    app::DocumentViewId document_view;
    app::CanvasId canvas;
    app::Generation document_generation;
    app::Generation surface_generation;
    SnapshotOwnerKind owner_kind{SnapshotOwnerKind::Document};
    app::AuxiliarySourceId auxiliary_source;

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        const bool owner = owner_kind == SnapshotOwnerKind::Auxiliary
            ? static_cast<bool>(auxiliary_source)
            : static_cast<bool>(document_session)
                && static_cast<bool>(document_view);
        return owner && static_cast<bool>(canvas)
            && static_cast<bool>(document_generation)
            && static_cast<bool>(surface_generation);
    }

    constexpr auto operator<=>(const SnapshotRoute&) const noexcept = default;
};

// Owns snapshot on construction. RendererHost::Submit consumes that ownership
// on success, stale rejection, queue replacement/failure, surface close, and
// host shutdown.
struct SnapshotEnvelope {
    SnapshotRoute route;
    // Renderer snapshot revision: previews may advance this independently of
    // the committed document. It continues to validate the immutable payload.
    std::uint64_t document_revision{};
    std::uint64_t view_revision{};
    InkpodSnapshot* snapshot{};
    std::uint64_t estimated_payload_bytes{};
    // Captured on the Core owner thread with the snapshot. Auxiliary sources
    // have no document and use zero. This revision and presentation_epoch,
    // never a preview revision, validate edit fences together.
    std::uint64_t committed_document_revision{};
    // Assigned by RendererHost when accepting the envelope into its queue.
    std::uint64_t submission_qpc{};
    // Windows-only navigation token fixed on the Core owner thread. This
    // distinguishes replacements with equal/lower document revisions without
    // becoming part of source identity or any CPU/GPU cache key.
    std::uint64_t presentation_epoch{};
    // Captured from this exact immutable snapshot. CanvasHost projects these
    // scalar values into native scrollbars only after RendererHost accepts the
    // envelope; they are not renderer cache keys or persistent document state.
    InkpodSnapshotTransform transform{};
    CanvasScrollRangeHint scroll_range_hint{CanvasScrollRangeHint::Preserve};
    // Nonpersistent issue-order identity for a pending ResetToBase cause. Zero
    // accompanies Preserve. CanvasHost uses it only to coalesce UI mail safely.
    std::uint64_t scroll_cause_token{};
};

// Process-wide renderer resource telemetry. Byte counts describe payloads and
// GPU allocations owned by RendererHost; they intentionally exclude driver-
// private allocations that DXGI does not expose portably. The snapshot is a
// value copy and is safe to inspect from the UI/Input thread.
struct RendererResourceUsage {
    std::uint64_t gpu_tile_budget_bytes{};
    std::uint64_t retained_snapshot_bytes{};
    std::uint64_t gpu_tile_bytes{};
    std::uint64_t swap_chain_bytes{};
    std::uint64_t pending_snapshot_bytes{};
    std::uint64_t cached_tile_count{};
    std::uint64_t active_tile_count{};
    std::uint64_t surface_count{};
    std::uint64_t visible_surface_count{};
    std::uint64_t queued_work_count{};
    std::uint64_t queue_rejection_count{};
    std::uint64_t queue_replacement_count{};
    std::uint64_t stale_snapshot_count{};
    std::uint64_t resource_limit_count{};
    std::uint64_t device_reset_count{};
    // Source-cache allocations are a subset of gpu_tile_bytes, including the
    // active pristine source. Upload counters are cumulative for live surfaces.
    std::uint64_t sequence_cache_source_count{};
    std::uint64_t sequence_cache_bytes{};
    std::uint64_t sequence_cache_eviction_count{};
    std::uint64_t uploaded_tile_count{};
    std::uint64_t uploaded_tile_bytes{};
};

// One CanvasSurface contribution to RendererResourceUsage. The route keeps the
// document/view/Canvas namespace captured when the surface was bound; no GPU
// object or snapshot pointer escapes the renderer owner thread.
struct RendererSurfaceResourceUsage {
    SnapshotRoute route;
    std::uint64_t retained_snapshot_bytes{};
    std::uint64_t gpu_tile_bytes{};
    std::uint64_t swap_chain_bytes{};
    std::uint64_t cached_tile_count{};
    std::uint64_t active_tile_count{};
    bool visible{};
    bool occluded{};
    std::uint64_t sequence_cache_source_count{};
    std::uint64_t sequence_cache_bytes{};
    std::uint64_t sequence_cache_eviction_count{};
    std::uint64_t uploaded_tile_count{};
    std::uint64_t uploaded_tile_bytes{};
    // These identify the last Present that returned S_OK. Snapshot submission,
    // upload, frame-latency timeout, and occlusion never advance them. Binding
    // another route or losing the device invalidates the old presentation.
    std::uint64_t last_presented_document_revision{};
    std::uint64_t last_presented_view_revision{};
    InkpodSnapshotSourceIdentity last_presented_source{};
    // QPC ticks (divide by QueryPerformanceFrequency). Zero means unavailable.
    // Re-presenting the same committed revision and epoch keeps its first time.
    std::uint64_t last_snapshot_submission_qpc{};
    std::uint64_t first_presented_revision_qpc{};
    std::uint64_t last_presented_presentation_epoch{};
    // Cumulative waits that expired and the raw result of the latest render
    // attempt, before occlusion/device recovery normalization. Neither is a
    // successful Present acknowledgement.
    std::uint64_t frame_latency_timeout_count{};
    HRESULT last_render_result{S_OK};
    // Absolute QPC stages of the same first successful committed revision /
    // epoch as first_presented_revision_qpc. Ready follows frame admission and
    // precedes D2D preparation; present-begin immediately precedes Present.
    // Repeated presentations leave all three stages unchanged.
    std::uint64_t first_frame_ready_qpc{};
    std::uint64_t first_present_begin_qpc{};
};

// Process-owned facade for one renderer thread. Every GPU call and every
// CanvasSurface construction/destruction runs on that owner thread.
class RendererHost final {
public:
    RendererHost() noexcept;
    ~RendererHost();

    RendererHost(const RendererHost&) = delete;
    RendererHost& operator=(const RendererHost&) = delete;

    HRESULT Start() noexcept;
    void Stop() noexcept;

    HRESULT RegisterSurface(
        app::CanvasId canvas,
        app::Generation surface_generation,
        HWND window,
        HWND owner_window) noexcept;
    void UnregisterSurface(
        app::CanvasId canvas,
        app::Generation surface_generation) noexcept;
    HRESULT BindSurface(const SnapshotRoute& route) noexcept;
    HRESULT UnbindSurface(
        app::CanvasId canvas,
        app::Generation surface_generation) noexcept;
    [[nodiscard]] bool SurfaceAcceptsSnapshots(const SnapshotRoute& route) const noexcept;

    // Consumes envelope.snapshot on every path.
    bool Submit(SnapshotEnvelope envelope) noexcept;
    void Resize(
        app::CanvasId canvas,
        app::Generation surface_generation,
        UINT width,
        UINT height) noexcept;
    void SetVisible(
        app::CanvasId canvas,
        app::Generation surface_generation,
        bool visible) noexcept;
    void RequestRender(
        app::CanvasId canvas,
        app::Generation surface_generation) noexcept;
    void DpiChanged(
        app::CanvasId canvas,
        app::Generation surface_generation) noexcept;

    HRESULT RenderOnce(
        app::CanvasId canvas,
        app::Generation surface_generation) noexcept;
    HRESULT SimulateDeviceLoss(
        app::CanvasId canvas,
        app::Generation surface_generation) noexcept;
    HRESULT ReadPixelForSmokeTest(
        app::CanvasId canvas,
        app::Generation surface_generation,
        UINT x,
        UINT y,
        CanvasPixelRgba8& pixel) noexcept;
    HRESULT GetDocumentBounds(
        app::CanvasId canvas,
        app::Generation surface_generation,
        CanvasDocumentBounds& bounds) noexcept;
    HRESULT GetGeometryPreview(
        app::CanvasId canvas,
        app::Generation surface_generation,
        CanvasGeometryPreview& preview) noexcept;
    HRESULT SetFloatingPreview(
        app::CanvasId canvas,
        app::Generation surface_generation,
        const CanvasFloatingPreview& preview) noexcept;
    HRESULT SetGeometryPreview(
        app::CanvasId canvas,
        app::Generation surface_generation,
        const CanvasGeometryPreview& preview) noexcept;
    [[nodiscard]] DWORD ThreadId() const noexcept;
    [[nodiscard]] std::uint64_t PresentedFrameCount(
        app::CanvasId canvas,
        app::Generation surface_generation) const noexcept;
    [[nodiscard]] std::size_t SurfaceCount() const noexcept;
    [[nodiscard]] std::uint64_t DeviceGeneration() const noexcept;
    [[nodiscard]] RendererResourceUsage ResourceUsage() const noexcept;
    [[nodiscard]] bool GetSurfaceResourceUsage(
        app::CanvasId canvas,
        app::Generation surface_generation,
        RendererSurfaceResourceUsage& usage) const noexcept;
    void SetQueuePausedForSmokeTest(bool paused) noexcept;
    [[nodiscard]] bool WaitQueueIdleForSmokeTest() noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::renderer
