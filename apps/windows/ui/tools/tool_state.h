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
inline constexpr std::uint32_t kInteractionEffectGradient = 1101U;
inline constexpr std::uint32_t kInteractionEffectAirbrush = 1102U;
inline constexpr std::uint32_t kInteractionEffectBlur = 1103U;
inline constexpr std::uint32_t kInteractionEffectStamp = 1104U;
inline constexpr std::uint32_t kInteractionEffectDust = 1105U;
inline constexpr std::uint32_t kInteractionEffectAlphaGradient = 1106U;
inline constexpr std::uint32_t kInteractionVectorLine = 1201U;
inline constexpr std::uint32_t kInteractionVectorCurve = 1202U;
inline constexpr std::uint32_t kInteractionVectorRectangle = 1203U;
inline constexpr std::uint32_t kInteractionVectorEllipse = 1204U;
inline constexpr std::uint32_t kInteractionVectorPolyline = 1205U;
inline constexpr std::uint32_t kInteractionVectorEraser = 1206U;

bool IsVectorCanvasTool(std::uint32_t tool) noexcept;
bool IsVectorStrokePlane(std::uint32_t kind) noexcept;

// All active-tool changes go through this boundary so leaving a vector,
// selection, or ranged-fill tool cannot retain a geometry preview owned by the
// prior interaction.
void TransitionActiveTool(
    app::ToolUiState& tools, HWND canvas, std::uint32_t next_tool) noexcept;

// Projects a copied Core color into the presentation cache. Runtime command
// routes must update the Core-owned EditorState before calling this helper.
void SetActiveCommandColor(
    app::ToolUiState& tools, InkpodColorValue color) noexcept;

// Called only by an active-plane transition, never by command-state queries.
void HandleActivePlaneTransition(
    app::ToolUiState& tools, HWND canvas, bool vector_stroke_plane) noexcept;

void CancelVectorGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

void CancelSelectionGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

void CancelColorReplaceGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

void CancelFillGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept;

} // namespace inkpod::windows::ui::tools
