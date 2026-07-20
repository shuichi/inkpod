#pragma once

#include <windows.h>

#include <cstdint>

namespace inkpod::renderer {

inline constexpr UINT kCanvasSetSnapshotRevision = WM_APP + 0x120U;
inline constexpr UINT kCanvasRenderOnce = WM_APP + 0x121U;
inline constexpr UINT kCanvasRenderFailed = WM_APP + 0x122U;
inline constexpr UINT kCanvasSimulateDeviceLoss = WM_APP + 0x123U;

bool RegisterCanvasClass(HINSTANCE instance) noexcept;
HWND CreateCanvasWindow(HINSTANCE instance, HWND parent) noexcept;

}  // namespace inkpod::renderer
