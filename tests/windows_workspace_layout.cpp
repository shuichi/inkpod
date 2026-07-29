#include <windows.h>

#include "ui/workspace_layout.h"

namespace {

using inkpod::windows::ui::ComputeWorkspaceLayout;
using inkpod::windows::ui::ResetWorkspaceLayout;
using inkpod::windows::ui::WorkspaceLayoutState;

int Width(const RECT& value) noexcept {
    return value.right - value.left;
}

int Height(const RECT& value) noexcept {
    return value.bottom - value.top;
}

}  // namespace

int main() {
    WorkspaceLayoutState state{};
    const auto normal = ComputeWorkspaceLayout(1'200, 800, 20, 96U, state);
    if (normal.tool_options.left != 0 || normal.tool_options.top != 0
        || Width(normal.tool_options) != 1'200
        || Height(normal.tool_options) != 40
        || normal.tool.left != 0 || normal.tool.top != 40
        || Width(normal.tool) != 80 || Width(normal.tool_splitter) != 4
        || normal.document_tabs.left != 84
        || normal.canvas.left != 84 || normal.canvas.right != 876
        || normal.color.left != 880 || Width(normal.color) != 320
        || normal.layer.left != 880 || Width(normal.layer) != 320
        || Height(normal.color_splitter) != 4) {
        return 1;
    }

    state.tool_visible = false;
    const auto without_tool =
        ComputeWorkspaceLayout(1'200, 800, 20, 96U, state);
    if (Width(without_tool.tool) != 0 || Width(without_tool.tool_splitter) != 0
        || without_tool.canvas.left != 0 || without_tool.canvas.right != 876) {
        return 2;
    }

    state.color_visible = false;
    state.layer_visible = false;
    const auto canvas_only =
        ComputeWorkspaceLayout(1'200, 800, 20, 96U, state);
    if (Width(canvas_only.inspector_splitter) != 0
        || Width(canvas_only.color) != 0 || Width(canvas_only.layer) != 0
        || canvas_only.canvas.left != 0 || canvas_only.canvas.right != 1'200) {
        return 3;
    }

    ResetWorkspaceLayout(state);
    state.mirrored = true;
    const auto mirrored = ComputeWorkspaceLayout(1'200, 800, 20, 96U, state);
    if (mirrored.color.left != 0 || mirrored.layer.left != 0
        || mirrored.canvas.left != 324 || mirrored.canvas.right != 1'116
        || mirrored.tool.left != 1'120 || mirrored.tool.right != 1'200) {
        return 4;
    }

    ResetWorkspaceLayout(state);
    const auto narrow = ComputeWorkspaceLayout(600, 600, 0, 96U, state);
    if (Width(narrow.color) != 0 || Width(narrow.layer) != 0
        || narrow.canvas.left != 84 || narrow.canvas.right != 600) {
        return 5;
    }

    const auto high_dpi = ComputeWorkspaceLayout(1'800, 1'200, 0, 144U, state);
    if (Height(high_dpi.tool_options) != 60 || Width(high_dpi.tool) != 120
        || Width(high_dpi.tool_splitter) != 6
        || Width(high_dpi.color) != 480 || Width(high_dpi.color_splitter) != 480
        || high_dpi.canvas.left != 126 || high_dpi.canvas.right != 1'314) {
        return 6;
    }

    return 0;
}
