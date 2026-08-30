#include "tool_state.h"

#include "app/frontend_state.h"
#include "canvas.h"
#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui::tools {
namespace {

std::uint32_t ColorToRgba8(const InkpodColorValue& color) noexcept {
    const auto channel = [&](std::uint16_t value) {
        return color.depth == INKPOD_COLOR_DEPTH_16
            ? static_cast<std::uint32_t>(
                  (static_cast<std::uint32_t>(value) + 128U) / 257U)
            : static_cast<std::uint32_t>(value & 0xffU);
    };
    return (channel(color.red) << 24U) | (channel(color.green) << 16U)
        | (channel(color.blue) << 8U) | channel(color.alpha);
}

void ApplyCurrentColor(
    app::ToolUiState& tools, InkpodColorValue color) noexcept {
    color.struct_size = sizeof(InkpodColorValue);
    tools.drawing_color = color;
    tools.color_rgba = ColorToRgba8(color);
}

void ClearGeometryPreviewIfNeeded(
    app::ToolUiState& tools, HWND canvas, bool had_preview) noexcept {
    if (!had_preview && !tools.geometry_preview_clear_pending) {
        return;
    }
    // Cancel still resets the gesture immediately. A rejected Renderer control
    // must not make an old overlay unreachable once its samples are gone.
    tools.geometry_preview_clear_pending = canvas == nullptr
        || SendMessageW(canvas, renderer::kCanvasClearGeometryPreview, 0, 0) != 1;
}

} // namespace

bool IsGeometryCanvasTool(std::uint32_t tool) noexcept {
    return tool >= kInteractionGeometryLine
        && tool <= kInteractionGeometryPolyline;
}

bool IsGeometryCanvasPlane(std::uint32_t kind) noexcept {
    return kind == INKPOD_TYPED_PLANE_MAIN_LINE
        || kind == INKPOD_TYPED_PLANE_COLOR
        || kind == INKPOD_TYPED_PLANE_RASTER;
}

std::uint32_t ActiveToolAfterPlaneTransition(
    std::uint32_t active_tool,
    std::uint32_t plane_kind,
    bool explicit_plane_selection) noexcept {
    if ((IsGeometryCanvasTool(active_tool)
            && !IsGeometryCanvasPlane(plane_kind))
        || (explicit_plane_selection && active_tool == kInteractionFill
            && plane_kind == INKPOD_TYPED_PLANE_MAIN_LINE)) {
        return INKPOD_TOOL_PENCIL;
    }
    return active_tool;
}

void CancelRasterGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    const bool had_preview = tools.geometry_preview_active
        || !tools.geometry_gesture_samples.empty();
    tools.geometry_gesture_samples.clear();
    tools.geometry_base_revision = 0U;
    tools.geometry_view_revision = 0U;
    tools.geometry_preview_active = false;
    tools.geometry_snap_bypass = false;
    tools.procedure.valid = false;
    ClearGeometryPreviewIfNeeded(tools, canvas, had_preview);
}

void CancelSelectionGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    const bool had_preview = !tools.selection_gesture_samples.empty();
    tools.selection_gesture_samples.clear();
    tools.procedure.valid = false;
    ClearGeometryPreviewIfNeeded(tools, canvas, had_preview);
}

void CancelColorReplaceGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    const bool had_preview = !tools.color_replace_gesture_samples.empty();
    tools.color_replace_gesture_samples.clear();
    tools.color_replace_base_revision = 0U;
    tools.procedure.valid = false;
    ClearGeometryPreviewIfNeeded(tools, canvas, had_preview);
}

void CancelFillGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    const bool had_preview = !tools.fill_gesture_samples.empty();
    tools.fill_gesture_samples.clear();
    tools.procedure.valid = false;
    ClearGeometryPreviewIfNeeded(tools, canvas, had_preview);
}

void TransitionActiveTool(
    app::ToolUiState& tools, HWND canvas, std::uint32_t next_tool) noexcept {
    if (tools.active_tool != next_tool
        && IsGeometryCanvasTool(tools.active_tool)) {
        CancelRasterGeometryPreview(tools, canvas);
    }
    if (tools.active_tool != next_tool
        && tools.active_tool == kInteractionSelection) {
        CancelSelectionGeometryPreview(tools, canvas);
    }
    if (tools.active_tool != next_tool
        && tools.active_tool == kInteractionColorReplace) {
        CancelColorReplaceGeometryPreview(tools, canvas);
    }
    if (tools.active_tool != next_tool
        && tools.active_tool == kInteractionFill) {
        CancelFillGeometryPreview(tools, canvas);
    }
    tools.active_tool = next_tool;
}

void SetActiveCommandColor(
    app::ToolUiState& tools, InkpodColorValue color) noexcept {
    ApplyCurrentColor(tools, color);
}

void HandleActivePlaneTransition(
    app::ToolUiState& tools, HWND canvas, std::uint32_t plane_kind) noexcept {
    const std::uint32_t next_tool =
        ActiveToolAfterPlaneTransition(tools.active_tool, plane_kind, true);
    if (next_tool != tools.active_tool) {
        TransitionActiveTool(tools, canvas, next_tool);
    }
}

} // namespace inkpod::windows::ui::tools
