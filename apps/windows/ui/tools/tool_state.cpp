#include "tool_state.h"

#include "app/app_context.h"
#include "canvas.h"
#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui::tools {
namespace {

std::size_t ColorIndex(ColorCommand command) noexcept {
    return static_cast<std::size_t>(command);
}

InkpodColorValue DefaultCommandColor(ColorCommand command) noexcept {
    if (command == ColorCommand::Pencil) {
        return InkpodColorValue{
            sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    }
    return InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 220U, 40U, 30U, 255U};
}

bool ColorCommandForTool(
    std::uint32_t tool, ColorCommand& command) noexcept {
    switch (tool) {
        case INKPOD_TOOL_PENCIL:
            command = ColorCommand::Pencil;
            return true;
        case INKPOD_TOOL_BRUSH:
            command = ColorCommand::Brush;
            return true;
        case kInteractionFill:
            command = ColorCommand::Fill;
            return true;
        case kInteractionSelection:
            command = ColorCommand::Selection;
            return true;
        case kInteractionEffectAirbrush:
            command = ColorCommand::EffectAirbrush;
            return true;
        case kInteractionVectorLine:
            command = ColorCommand::VectorLine;
            return true;
        case kInteractionVectorCurve:
            command = ColorCommand::VectorCurve;
            return true;
        case kInteractionVectorRectangle:
            command = ColorCommand::VectorRectangle;
            return true;
        case kInteractionVectorEllipse:
            command = ColorCommand::VectorEllipse;
            return true;
        case kInteractionVectorPolyline:
            command = ColorCommand::VectorPolyline;
            return true;
        default:
            return false;
    }
}

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

void ActivateColorCommand(
    app::ToolUiState& tools, ColorCommand next_command) noexcept {
    const std::size_t current = ColorIndex(tools.active_color_command);
    tools.command_colors[current] = tools.drawing_color;
    tools.command_color_initialized[current] = true;

    const std::size_t next = ColorIndex(next_command);
    if (!tools.command_color_initialized[next]) {
        tools.command_colors[next] = DefaultCommandColor(next_command);
        tools.command_color_initialized[next] = true;
    }
    tools.active_color_command = next_command;
    ApplyCurrentColor(tools, tools.command_colors[next]);
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

void CancelSelectionGeometryPreview(
    app::ToolUiState& tools, HWND canvas) noexcept {
    tools.selection_gesture_samples.clear();
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
    if (tools.active_tool != next_tool
        && tools.active_tool == kInteractionSelection) {
        CancelSelectionGeometryPreview(tools, canvas);
    }
    ColorCommand next_color_command{};
    if (ColorCommandForTool(next_tool, next_color_command)
        && next_color_command != tools.active_color_command) {
        ActivateColorCommand(tools, next_color_command);
    }
    tools.active_tool = next_tool;
}

void SetActiveCommandColor(
    app::ToolUiState& tools, InkpodColorValue color) noexcept {
    ApplyCurrentColor(tools, color);
    const std::size_t active = ColorIndex(tools.active_color_command);
    tools.command_colors[active] = tools.drawing_color;
    tools.command_color_initialized[active] = true;
}

void HandleActivePlaneTransition(
    app::ToolUiState& tools, HWND canvas, bool vector_stroke_plane) noexcept {
    if (!vector_stroke_plane && IsVectorCanvasTool(tools.active_tool)) {
        TransitionActiveTool(tools, canvas, INKPOD_TOOL_PENCIL);
    }
}

} // namespace inkpod::windows::ui::tools
