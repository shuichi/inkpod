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

struct CanvasGeometryPreview {
    std::uint32_t struct_size;
    std::uint32_t active;
    std::uint32_t point_count;
    std::uint32_t closed;
    float stroke_width;
    std::uint32_t reserved;
    InkpodVectorPoint points[kCanvasGeometryPreviewPoints];
};

struct SnapshotRoute {
    app::DocumentSessionId document_session;
    app::DocumentViewId document_view;
    app::CanvasId canvas;
    app::Generation document_generation;
    app::Generation surface_generation;

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return static_cast<bool>(document_session)
            && static_cast<bool>(document_view)
            && static_cast<bool>(canvas)
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
    std::uint64_t document_revision{};
    std::uint64_t view_revision{};
    InkpodSnapshot* snapshot{};
    std::uint64_t estimated_payload_bytes{};
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
    HRESULT ValidateClosedVectorStroke(
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
    HRESULT SetAnnotationSelection(
        app::CanvasId canvas,
        app::Generation surface_generation,
        std::uint64_t object_id) noexcept;

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
