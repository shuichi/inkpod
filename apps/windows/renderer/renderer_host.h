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
    void SetQueuePausedForSmokeTest(bool paused) noexcept;
    [[nodiscard]] bool WaitQueueIdleForSmokeTest() noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::renderer
