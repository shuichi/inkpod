#include <windows.h>

#include "ui/dock_layout.h"
#include "ui/workspace_layout.h"

namespace {

using inkpod::windows::ui::ComputeWorkspaceLayout;
using inkpod::windows::ui::DockFloatingPlacement;
using inkpod::windows::ui::DockLayoutModel;
using inkpod::windows::ui::DockPaneType;
using inkpod::windows::ui::DockResult;
using inkpod::windows::ui::DockStackMode;
using inkpod::windows::ui::DockZone;
using inkpod::windows::ui::PaneDescriptors;
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
    if (PaneDescriptors().size() != 4U
        || PaneDescriptors()[0].stable_type_id == 0U
        || PaneDescriptors()[0].title_resource_id == 0U
        || PaneDescriptors()[0].fallback_title == nullptr
        || PaneDescriptors()[1].scope
            != inkpod::windows::ui::PaneTargetScope::FollowActiveView
        || PaneDescriptors()[0].can_auto_hide
        || !PaneDescriptors()[0].can_float) {
        return 1;
    }

    DockLayoutModel model{};
    if (model.AddPane(DockPaneType::Tool) != DockResult::DuplicatePane
        || model.MovePane(DockPaneType::Tool, DockZone::TopContext)
            != DockResult::ZoneNotAllowed
        || model.RemovePane(DockPaneType::Count) != DockResult::InvalidPane
        || model.RemovePane(DockPaneType::Tool) != DockResult::Ok
        || model.RemovePane(DockPaneType::Tool) != DockResult::NoOp
        || model.AddPane(DockPaneType::Tool) != DockResult::Ok) {
        return 2;
    }

    if (model.MovePane(DockPaneType::Color, DockZone::Left) != DockResult::Ok
        || model.PaneCount(DockZone::Left) != 2U
        || model.TabPane(DockPaneType::Color, DockPaneType::Tool)
            != DockResult::Ok
        || model.Zone(DockZone::Left) == nullptr
        || model.Zone(DockZone::Left)->mode != DockStackMode::Tabs
        || model.Zone(DockZone::Left)->active_tab != DockPaneType::Color) {
        return 3;
    }

    const DockFloatingPlacement invalid_floating{0, 0, 10, 10};
    const DockFloatingPlacement valid_floating{160, 180, 360, 300};
    const DockFloatingPlacement moved_floating{180, 200, 380, 320};
    const DockFloatingPlacement secondary_monitor{-1'920, 40, 380, 320};
    const DockFloatingPlacement outside_coordinate_limit{
        -1'000'001, 40, 380, 320};
    if (model.FloatPane(DockPaneType::Color, invalid_floating)
            != DockResult::ZoneNotAllowed
        || model.FloatPane(DockPaneType::Color, valid_floating) != DockResult::Ok
        || model.FloatPane(DockPaneType::Color, valid_floating) != DockResult::NoOp
        || model.FloatPane(DockPaneType::Color, moved_floating) != DockResult::Ok
        || model.Pane(DockPaneType::Color)->floating.x_dip != 180
        || model.FloatPane(DockPaneType::Color, secondary_monitor)
            != DockResult::Ok
        || model.Pane(DockPaneType::Color)->floating.x_dip != -1'920
        || model.FloatPane(DockPaneType::Color, outside_coordinate_limit)
            != DockResult::ZoneNotAllowed
        || !model.IsPaneVisible(DockPaneType::Color)
        || model.IsPaneDocked(DockPaneType::Color)
        || model.HidePane(DockPaneType::Color) != DockResult::Ok
        || model.IsPaneVisible(DockPaneType::Color)
        || model.RestorePane(DockPaneType::Color) != DockResult::Ok
        || !model.IsPaneDocked(DockPaneType::Color)
        || model.Pane(DockPaneType::Color)->zone != DockZone::Left
        || model.ResetPane(DockPaneType::Color) != DockResult::Ok
        || model.Pane(DockPaneType::Color)->zone != DockZone::Right) {
        return 4;
    }

    model.Reset();
    const auto* color_before = model.Pane(DockPaneType::Color);
    const auto* layer_before = model.Pane(DockPaneType::Layer);
    if (color_before == nullptr || layer_before == nullptr) {
        return 5;
    }
    const std::uint32_t combined_before =
        color_before->split_weight + layer_before->split_weight;
    if (model.AdjustSplitBoundary(DockZone::Right, 0U, 80)
            != DockResult::Ok
        || model.Pane(DockPaneType::Color)->split_weight != 400U
        || model.Pane(DockPaneType::Layer)->split_weight != 600U
        || model.Pane(DockPaneType::Color)->split_weight
                + model.Pane(DockPaneType::Layer)->split_weight
            != combined_before
        || model.AdjustSplitBoundary(DockZone::Right, 2U, 20)
            != DockResult::InvalidState
        || model.SetZoneExtentDip(DockZone::Left, 1) != DockResult::NoOp
        || model.Zone(DockZone::Left)->extent_dip != 80) {
        return 6;
    }

    const auto record = model.ToRecord();
    DockLayoutModel restored{};
    if (!restored.LoadRecord(record)
        || restored.Pane(DockPaneType::Color)->split_weight != 400U) {
        return 7;
    }
    auto corrupted = record;
    corrupted.panes[0].zone = DockZone::TopContext;
    if (restored.LoadRecord(corrupted)) {
        return 8;
    }
    corrupted = record;
    corrupted.panes[3].order = corrupted.panes[2].order;
    if (restored.LoadRecord(corrupted)) {
        return 18;
    }
    corrupted = record;
    corrupted.zones[static_cast<std::size_t>(DockZone::Right)].active_tab =
        DockPaneType::Tool;
    if (restored.LoadRecord(corrupted)) {
        return 19;
    }
    corrupted = record;
    corrupted.zones[static_cast<std::size_t>(DockZone::TopContext)].extent_dip =
        600;
    if (restored.LoadRecord(corrupted)) {
        return 20;
    }
    DockLayoutModel hidden_record{};
    if (hidden_record.HidePane(DockPaneType::Color) != DockResult::Ok
        || !restored.LoadRecord(hidden_record.ToRecord())
        || restored.Zone(DockZone::Right)->active_tab != DockPaneType::Layer) {
        return 21;
    }

    WorkspaceLayoutState state{};
    const auto normal = ComputeWorkspaceLayout(1'200, 800, 20, 96U, state);
    const auto& tool = normal.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)];
    const auto& options =
        normal.dock.panes[static_cast<std::size_t>(DockPaneType::ToolOptions)];
    const auto& color = normal.dock.panes[static_cast<std::size_t>(DockPaneType::Color)];
    const auto& layer = normal.dock.panes[static_cast<std::size_t>(DockPaneType::Layer)];
    if (!tool.shown || !options.shown || !color.shown || !layer.shown
        || options.bounds.x != 0 || options.bounds.y != 0
        || options.bounds.width != 1'200 || options.bounds.height != 40
        || tool.bounds.x != 0 || tool.bounds.y != 44 || tool.bounds.width != 80
        || normal.editor.left != 84 || normal.editor.right != 876
        || color.bounds.x != 880 || color.bounds.width != 320
        || layer.bounds.x != 880 || layer.bounds.width != 320
        || color.bounds.height != 234 || layer.bounds.height != 498
        || Height(normal.document_tabs) != 28
        || normal.canvas.top != normal.document_tabs.bottom) {
        return 9;
    }

    state.dock.SetMirrored(true);
    const auto mirrored = ComputeWorkspaceLayout(1'200, 800, 20, 96U, state);
    const auto& mirrored_tool =
        mirrored.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)];
    const auto& mirrored_color =
        mirrored.dock.panes[static_cast<std::size_t>(DockPaneType::Color)];
    if (mirrored_color.bounds.x != 0 || mirrored.editor.left != 324
        || mirrored.editor.right != 1'116 || mirrored_tool.bounds.x != 1'120
        || mirrored_tool.bounds.width != 80) {
        return 10;
    }

    ResetWorkspaceLayout(state);
    const auto narrow = ComputeWorkspaceLayout(600, 600, 0, 96U, state);
    const auto& narrow_color =
        narrow.dock.panes[static_cast<std::size_t>(DockPaneType::Color)];
    const auto& narrow_layer =
        narrow.dock.panes[static_cast<std::size_t>(DockPaneType::Layer)];
    if (narrow_color.shown || narrow_layer.shown
        || !narrow_color.temporarily_auto_hidden
        || !narrow_layer.temporarily_auto_hidden
        || !state.dock.IsPaneVisible(DockPaneType::Color)
        || !state.dock.IsPaneVisible(DockPaneType::Layer)
        || narrow.editor.left != 84 || narrow.editor.right != 600) {
        return 11;
    }

    const auto high_dpi = ComputeWorkspaceLayout(1'800, 1'200, 0, 144U, state);
    const auto& high_tool =
        high_dpi.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)];
    const auto& high_options =
        high_dpi.dock.panes[static_cast<std::size_t>(DockPaneType::ToolOptions)];
    const auto& high_color =
        high_dpi.dock.panes[static_cast<std::size_t>(DockPaneType::Color)];
    if (high_options.bounds.height != 60 || high_tool.bounds.width != 120
        || high_color.bounds.width != 480 || high_dpi.editor.left != 126
        || high_dpi.editor.right != 1'314) {
        return 12;
    }

    const auto dpi_120 = ComputeWorkspaceLayout(1'500, 1'000, 0, 120U, state);
    const auto dpi_192 = ComputeWorkspaceLayout(2'400, 1'600, 0, 192U, state);
    if (dpi_120.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)]
                .bounds.width
            != 100
        || dpi_120.dock.panes[static_cast<std::size_t>(DockPaneType::ToolOptions)]
                .bounds.height
            != 50
        || dpi_192.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)]
                .bounds.width
            != 160
        || dpi_192.dock.panes[static_cast<std::size_t>(DockPaneType::ToolOptions)]
                .bounds.height
            != 80) {
        return 17;
    }

    if (state.dock.SetZoneMode(DockZone::Right, DockStackMode::Tabs)
            != DockResult::Ok
        || state.dock.SetActiveTab(DockZone::Right, DockPaneType::Layer)
            != DockResult::Ok) {
        return 13;
    }
    const auto tabbed = ComputeWorkspaceLayout(1'200, 800, 0, 96U, state);
    if (tabbed.dock.panes[static_cast<std::size_t>(DockPaneType::Color)].shown
        || !tabbed.dock.panes[static_cast<std::size_t>(DockPaneType::Layer)].shown
        || tabbed.dock.splitter_count >= normal.dock.splitter_count) {
        return 14;
    }

    for (const DockPaneType type : {
             DockPaneType::Tool,
             DockPaneType::ToolOptions,
             DockPaneType::Color,
             DockPaneType::Layer}) {
        if (state.dock.HidePane(type) != DockResult::Ok) {
            return 15;
        }
    }
    const auto canvas_only = ComputeWorkspaceLayout(1'200, 800, 0, 96U, state);
    if (Width(canvas_only.editor) != 1'200 || Height(canvas_only.editor) != 800) {
        return 16;
    }

    return 0;
}
