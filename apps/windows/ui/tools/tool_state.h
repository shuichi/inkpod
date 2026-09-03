#pragma once

#include <windows.h>

#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::app {
struct ToolUiState;
}

namespace inkpod::windows::ui::tools {

inline constexpr std::uint32_t kInteractionFill = 1001U;
inline constexpr std::uint32_t kInteractionEyedropper = 1002U;
inline constexpr std::uint32_t kInteractionBoxZoom = 1003U;
inline constexpr std::uint32_t kInteractionGuideMove = 1004U;
inline constexpr std::uint32_t kInteractionSelection = 1005U;
inline constexpr std::uint32_t kInteractionFloatingTransform = 1006U;
inline constexpr std::uint32_t kInteractionLightTableMove = 1007U;
inline constexpr std::uint32_t kInteractionColorReplace = 1008U;
inline constexpr std::uint32_t kInteractionShootingFrame = 1009U;
inline constexpr std::uint32_t kInteractionEffectGradient = 1101U;
inline constexpr std::uint32_t kInteractionEffectAirbrush = 1102U;
inline constexpr std::uint32_t kInteractionEffectBlur = 1103U;
inline constexpr std::uint32_t kInteractionEffectStamp = 1104U;
inline constexpr std::uint32_t kInteractionEffectDust = 1105U;
inline constexpr std::uint32_t kInteractionEffectAlphaGradient = 1106U;
inline constexpr std::uint32_t kInteractionEffectLineConnect = 1107U;
inline constexpr std::uint32_t kInteractionEffectLineWidth = 1108U;
inline constexpr std::uint32_t kInteractionGeometryLine =
    INKPOD_EDITOR_TOOL_GEOMETRY_LINE;
inline constexpr std::uint32_t kInteractionGeometryCurve =
    INKPOD_EDITOR_TOOL_GEOMETRY_CURVE;
inline constexpr std::uint32_t kInteractionGeometryRectangle =
    INKPOD_EDITOR_TOOL_GEOMETRY_RECTANGLE;
inline constexpr std::uint32_t kInteractionGeometryEllipse =
    INKPOD_EDITOR_TOOL_GEOMETRY_ELLIPSE;
inline constexpr std::uint32_t kInteractionGeometryPolygon =
    INKPOD_EDITOR_TOOL_GEOMETRY_POLYGON;
inline constexpr std::uint32_t kInteractionGeometryPolyline =
    INKPOD_EDITOR_TOOL_GEOMETRY_POLYLINE;

bool IsGeometryCanvasTool(std::uint32_t tool) noexcept;
bool IsGeometryCanvasPlane(std::uint32_t kind) noexcept;

// Resolves the tool that remains meaningful after a plane change. Geometry is
// always reconciled away from unsupported planes. An explicit MainLine choice
// additionally returns Fill to main-line drawing; automatic refreshes do not.
std::uint32_t ActiveToolAfterPlaneTransition(
    std::uint32_t active_tool,
    std::uint32_t plane_kind,
    bool explicit_plane_selection) noexcept;

// All active-tool changes go through this boundary so leaving a geometry,
// selection, or ranged-fill tool cannot retain a preview owned by the prior
// interaction.
void TransitionActiveTool(
    app::ToolUiState& tools, HWND canvas, std::uint32_t next_tool) noexcept;

// Projects a copied Core color into the presentation cache. Runtime command
// routes must update the Core-owned EditorState before calling this helper.
void SetActiveCommandColor(
    app::ToolUiState& tools, InkpodColorValue color) noexcept;

// Called only by an active-plane transition, never by command-state queries.
void HandleActivePlaneTransition(
    app::ToolUiState& tools, HWND canvas, std::uint32_t plane_kind) noexcept;

void CancelRasterGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

void CancelSelectionGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

void CancelColorReplaceGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

void CancelFillGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

} // namespace inkpod::windows::ui::tools
