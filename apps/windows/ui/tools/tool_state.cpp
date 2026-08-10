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

} // namespace

bool IsVectorCanvasTool(std::uint32_t tool) noexcept {
    return tool >= kInteractionVectorLine && tool <= kInteractionVectorEraser;
}

bool IsVectorStrokePlane(std::uint32_t kind) noexcept {
    return kind == INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE
        || kind == INKPOD_TYPED_PLANE_COLOR_TRACE;
}

void CancelVectorGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    tools.vector_gesture_samples.clear();
    tools.procedure.valid = false;
    if (canvas == nullptr) {
        return;
    }
    SendMessageW(canvas, renderer::kCanvasClearGeometryPreview, 0, 0);
}

void CancelSelectionGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    tools.selection_gesture_samples.clear();
    tools.procedure.valid = false;
    if (canvas == nullptr) {
        return;
    }
    SendMessageW(canvas, renderer::kCanvasClearGeometryPreview, 0, 0);
}

void CancelFillGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    tools.fill_gesture_samples.clear();
    tools.procedure.valid = false;
    if (canvas == nullptr) {
        return;
    }
    SendMessageW(canvas, renderer::kCanvasClearGeometryPreview, 0, 0);
}

void TransitionActiveTool(
    app::ToolUiState& tools, HWND canvas, std::uint32_t next_tool) noexcept {
    if (tools.active_tool != next_tool && IsVectorCanvasTool(tools.active_tool)) {
        CancelVectorGeometryPreview(tools, canvas);
    }
    if (tools.active_tool != next_tool
        && tools.active_tool == kInteractionSelection) {
        CancelSelectionGeometryPreview(tools, canvas);
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
    app::ToolUiState& tools, HWND canvas, bool vector_stroke_plane) noexcept {
    if (!vector_stroke_plane && IsVectorCanvasTool(tools.active_tool)) {
        TransitionActiveTool(tools, canvas, INKPOD_TOOL_PENCIL);
    }
}

} // namespace inkpod::windows::ui::tools
