#include "tool_state.h"

#include "app/app_context.h"
#include "canvas.h"
#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui::tools {

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
    if (canvas == nullptr) {
        return;
    }
    renderer::CanvasGeometryPreview preview{};
    preview.struct_size = sizeof(preview);
    SendMessageW(
        canvas,
        renderer::kCanvasSetGeometryPreview,
        0,
        reinterpret_cast<LPARAM>(&preview));
}

void TransitionActiveTool(
    app::ToolUiState& tools, HWND canvas, std::uint32_t next_tool) noexcept {
    if (tools.active_tool != next_tool && IsVectorCanvasTool(tools.active_tool)) {
        CancelVectorGeometryPreview(tools, canvas);
    }
    tools.active_tool = next_tool;
}

void HandleActivePlaneTransition(
    app::ToolUiState& tools, HWND canvas, bool vector_stroke_plane) noexcept {
    if (!vector_stroke_plane && IsVectorCanvasTool(tools.active_tool)) {
        TransitionActiveTool(tools, canvas, INKPOD_TOOL_PENCIL);
    }
}

} // namespace inkpod::windows::ui::tools
