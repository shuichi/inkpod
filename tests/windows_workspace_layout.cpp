#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <span>
#include <string>

#include "ui/dock_layout.h"
#include "ui/workspace_layout.h"

namespace {

using inkpod::windows::ui::ComputeWorkspaceLayout;
using inkpod::windows::ui::ComputeDockLayout;
using inkpod::windows::ui::DecodeWorkspaceLayout;
using inkpod::windows::ui::DeleteWorkspaceLayout;
using inkpod::windows::ui::DockFloatingPlacement;
using inkpod::windows::ui::DockLayoutModel;
using inkpod::windows::ui::DockPaneType;
using inkpod::windows::ui::DockResult;
using inkpod::windows::ui::DockStackMode;
using inkpod::windows::ui::DockZone;
using inkpod::windows::ui::PaneDescriptors;
using inkpod::windows::ui::RightToolTabsModel;
using inkpod::windows::ui::ToolTabResult;
using inkpod::windows::ui::ApplyWorkspacePreset;
using inkpod::windows::ui::ClampWorkspaceFloatingPanes;
using inkpod::windows::ui::ClampWorkspacePlacement;
using inkpod::windows::ui::EncodeWorkspaceLayout;
using inkpod::windows::ui::FindWorkspaceAuxiliaryPane;
using inkpod::windows::ui::LoadWorkspaceLayout;
using inkpod::windows::ui::ResetWorkspaceLayout;
using inkpod::windows::ui::SaveWorkspaceLayout;
using inkpod::windows::ui::SetWorkspaceCustomName;
using inkpod::windows::ui::WorkspaceAutoHideEdge;
using inkpod::windows::ui::WorkspaceAuxiliaryPane;
using inkpod::windows::ui::WorkspaceDensity;
using inkpod::windows::ui::WorkspaceLayoutDecodeResult;
using inkpod::windows::ui::WorkspaceLayoutState;
using inkpod::windows::ui::WorkspacePreset;
using inkpod::windows::ui::WorkspaceScreenPlacement;
using inkpod::windows::ui::WorkspaceSplitOrientation;
using inkpod::windows::ui::WorkspaceWorkArea;
using inkpod::windows::ui::kDockedZoneCount;
using inkpod::windows::ui::kDockPaneCount;
using inkpod::windows::ui::kMaximumWorkspaceLayoutRecordBytes;
using inkpod::windows::ui::kToolTabColoring;
using inkpod::windows::ui::kToolTabReference;
using inkpod::windows::ui::kToolTabWorkflow;

int Width(const RECT& value) noexcept {
    return value.right - value.left;
}

int Height(const RECT& value) noexcept {
    return value.bottom - value.top;
}

template <typename T>
bool ReplaceFirst(
    std::span<std::byte> bytes, const T& old_value, const T& new_value) noexcept {
    const auto* old_bytes = reinterpret_cast<const std::byte*>(&old_value);
    for (std::size_t offset = 0U; offset + sizeof(T) <= bytes.size(); ++offset) {
        if (std::memcmp(bytes.data() + offset, old_bytes, sizeof(T)) == 0) {
            std::memcpy(bytes.data() + offset, &new_value, sizeof(T));
            return true;
        }
    }
    return false;
}

std::uint32_t GroupedPaneOrder(
    const inkpod::windows::ui::DockPanePlacement& pane) noexcept {
    return UINT32_C(0x80000000)
        | (pane.active_tab ? UINT32_C(1) << 24U : 0U)
        | (static_cast<std::uint32_t>(pane.tab_order) << 16U)
        | (static_cast<std::uint32_t>(pane.stack) << 8U)
        | pane.order;
}

bool DowngradeGroupedLayoutToV5(
    std::span<std::byte> bytes,
    const inkpod::windows::ui::DockLayoutRecord& record) noexcept {
    std::array<std::uint32_t, kDockedZoneCount> legacy_zone_orders{};
    for (const auto& pane : record.panes) {
        const auto& descriptor = PaneDescriptors()[
            static_cast<std::size_t>(pane.type)];
        if (!descriptor.persist_layout) {
            continue;
        }
        std::uint32_t legacy_order = pane.order;
        if (static_cast<std::size_t>(pane.zone) < kDockedZoneCount) {
            legacy_order = legacy_zone_orders[
                static_cast<std::size_t>(pane.zone)]++;
        }
        if (!ReplaceFirst(
                bytes, GroupedPaneOrder(pane), legacy_order)) {
            return false;
        }
    }
    return ReplaceFirst(bytes, UINT32_C(7), UINT32_C(5));
}

bool DowngradeGroupedLayoutToV6(std::span<std::byte> bytes) noexcept {
    return ReplaceFirst(bytes, UINT32_C(7), UINT32_C(6));
}

struct LegacyWorkspaceV2 {
    std::uint32_t magic{UINT32_C(0x4c574b49)};
    std::uint32_t version{2U};
    std::uint32_t struct_size{sizeof(LegacyWorkspaceV2)};
    std::uint32_t flags{UINT32_C(0x0f)};
    std::int32_t tool_width_dip{80};
    std::int32_t inspector_width_dip{320};
    std::int32_t tool_options_height_dip{40};
    std::uint32_t color_split_milli{320U};
    std::uint32_t layer_split_milli{550U};
};

struct LegacyDockPaneV3 {
    std::uint32_t type{};
    std::uint32_t zone{};
    std::uint32_t restore_zone{};
    std::uint32_t order{};
    std::uint32_t split_weight{};
    std::int32_t floating_x_dip{};
    std::int32_t floating_y_dip{};
    std::int32_t floating_width_dip{};
    std::int32_t floating_height_dip{};
};

struct LegacyDockZoneV3 {
    std::uint32_t mode{};
    std::uint32_t active_tab{};
    std::int32_t extent_dip{};
};

struct LegacyWorkspaceV3 {
    std::uint32_t magic{UINT32_C(0x4c574b49)};
    std::uint32_t version{3U};
    std::uint32_t struct_size{sizeof(LegacyWorkspaceV3)};
    std::uint32_t flags{};
    std::uint32_t pane_count{4U};
    std::uint32_t zone_count{static_cast<std::uint32_t>(kDockedZoneCount)};
    std::uint32_t layer_split_milli{550U};
    std::uint32_t reserved{};
    std::array<LegacyDockPaneV3, 4U> panes{};
    std::array<LegacyDockZoneV3, kDockedZoneCount> zones{};
};

}  // namespace

int main() {
    const auto& descriptors = PaneDescriptors();
    const auto& locator_descriptor = descriptors[
        static_cast<std::size_t>(DockPaneType::Locator)];
    const auto& color_descriptor = descriptors[
        static_cast<std::size_t>(DockPaneType::Color)];
    const auto& layer_descriptor = descriptors[
        static_cast<std::size_t>(DockPaneType::Layer)];
    const auto& job_descriptor = descriptors[
        static_cast<std::size_t>(DockPaneType::JobProgress)];
    if (descriptors.size() != kDockPaneCount
        || PaneDescriptors()[0].stable_type_id == 0U
        || PaneDescriptors()[0].title_resource_id == 0U
        || PaneDescriptors()[0].fallback_title == nullptr
        || PaneDescriptors()[1].scope
            != inkpod::windows::ui::PaneTargetScope::FollowActiveView
        || PaneDescriptors()[0].can_auto_hide
        || !PaneDescriptors()[0].can_float
        || PaneDescriptors()[0].show_header_when_singleton
        || PaneDescriptors()[1].show_header_when_singleton
        || !color_descriptor.show_header_when_singleton
        || !layer_descriptor.show_header_when_singleton
        || locator_descriptor.default_visible
        || !locator_descriptor.persist_layout
        || !locator_descriptor.can_float
        || !locator_descriptor.can_auto_hide
        || !locator_descriptor.show_header_when_singleton
        || job_descriptor.default_visible
        || job_descriptor.persist_layout
        || job_descriptor.can_float
        || job_descriptor.can_auto_hide
        || !job_descriptor.show_header_when_singleton) {
        return 1;
    }

    RightToolTabsModel tool_tabs{};
    if (tool_tabs.Tabs().size() != 3U
        || tool_tabs.Selected() != kToolTabColoring
        || !tool_tabs.IsVisible(kToolTabColoring)
        || !tool_tabs.IsVisible(kToolTabReference)
        || !tool_tabs.IsVisible(kToolTabWorkflow)
        || tool_tabs.TabForPane(DockPaneType::Color) != kToolTabColoring
        || tool_tabs.TabForPane(DockPaneType::Locator) != kToolTabReference
        || tool_tabs.TabForPane(DockPaneType::Batch) != kToolTabWorkflow) {
        return 120;
    }
    if (tool_tabs.MovePane(DockPaneType::Color, kToolTabReference)
            != ToolTabResult::Ok
        || tool_tabs.TabForPane(DockPaneType::Color) != kToolTabReference
        || tool_tabs.MovePane(DockPaneType::Color, kToolTabReference)
            != ToolTabResult::NoOp
        || tool_tabs.SetSelected(kToolTabReference) != ToolTabResult::Ok
        || tool_tabs.SetVisible(kToolTabReference, false) != ToolTabResult::Ok
        || tool_tabs.Selected() != kToolTabWorkflow
        || tool_tabs.SetVisible(kToolTabWorkflow, false) != ToolTabResult::Ok
        || tool_tabs.Selected() != kToolTabColoring
        || tool_tabs.SetVisible(kToolTabColoring, false) != ToolTabResult::Ok
        || tool_tabs.Selected()
        || tool_tabs.HasVisibleTabs()
        || tool_tabs.SetVisible(kToolTabReference, true) != ToolTabResult::Ok
        || tool_tabs.Selected() != kToolTabReference) {
        return 121;
    }
    tool_tabs.Reset();
    if (tool_tabs.Reorder(kToolTabColoring, kToolTabWorkflow, true)
            != ToolTabResult::Ok
        || tool_tabs.Tabs()[0].id != kToolTabReference
        || tool_tabs.Tabs()[1].id != kToolTabWorkflow
        || tool_tabs.Tabs()[2].id != kToolTabColoring
        || tool_tabs.Reorder(kToolTabColoring, kToolTabReference, false)
            != ToolTabResult::Ok
        || tool_tabs.Tabs()[0].id != kToolTabColoring
        || tool_tabs.Tabs()[1].id != kToolTabReference
        || tool_tabs.Tabs()[2].id != kToolTabWorkflow) {
        return 122;
    }
    RightToolTabsModel empty_tab_model{};
    DockLayoutModel empty_tab_dock{};
    if (empty_tab_model.MovePane(DockPaneType::Tool, kToolTabReference)
            != ToolTabResult::Ok
        || empty_tab_model.MovePane(DockPaneType::Color, kToolTabReference)
            != ToolTabResult::Ok
        || empty_tab_model.MovePane(DockPaneType::Layer, kToolTabReference)
            != ToolTabResult::Ok) {
        return 123;
    }
    const auto empty_tab_geometry = ComputeDockLayout(
        empty_tab_dock, 1'200, 720, 96U, &empty_tab_model);
    if (empty_tab_geometry.right_tool_tabs.height != 28
        || empty_tab_geometry.zones[
               static_cast<std::size_t>(DockZone::Right)].width
            != 320
        || empty_tab_geometry.panes[
               static_cast<std::size_t>(DockPaneType::Color)].shown
        || empty_tab_geometry.panes[
               static_cast<std::size_t>(DockPaneType::Layer)].shown) {
        return 124;
    }
    if (empty_tab_model.SetSelected(kToolTabReference) != ToolTabResult::Ok
        || empty_tab_dock.RestorePane(DockPaneType::Locator) != DockResult::Ok
        || empty_tab_dock.RestorePane(DockPaneType::LightTable)
            != DockResult::Ok
        || empty_tab_dock.RestorePane(DockPaneType::Reference)
            != DockResult::Ok) {
        return 125;
    }
    const auto reference_geometry = ComputeDockLayout(
        empty_tab_dock, 1'200, 720, 96U, &empty_tab_model);
    if (!reference_geometry.panes[
             static_cast<std::size_t>(DockPaneType::Color)].shown
        || !reference_geometry.panes[
             static_cast<std::size_t>(DockPaneType::Layer)].shown
        || !reference_geometry.panes[
             static_cast<std::size_t>(DockPaneType::Locator)].shown
        || !reference_geometry.panes[
             static_cast<std::size_t>(DockPaneType::LightTable)].shown
        || !reference_geometry.panes[
             static_cast<std::size_t>(DockPaneType::Reference)].shown
        || reference_geometry.panes[
               static_cast<std::size_t>(DockPaneType::Locator)].bounds.height
            <= 0
        || reference_geometry.panes[
               static_cast<std::size_t>(DockPaneType::LightTable)].bounds.height
            <= 0
        || reference_geometry.panes[
               static_cast<std::size_t>(DockPaneType::Reference)].bounds.height
            <= 0) {
        return 126;
    }
    const std::array<DockPaneType, 5U> constrained_right_panes{
        DockPaneType::Locator,
        DockPaneType::LightTable,
        DockPaneType::Reference,
        DockPaneType::Color,
        DockPaneType::Layer};
    for (const DockPaneType type : constrained_right_panes) {
        auto* pane = empty_tab_dock.Pane(type);
        if (pane == nullptr) {
            return 127;
        }
        pane->split_weight = type == DockPaneType::Locator ? 100'000U : 100U;
    }
    const auto constrained_geometry = ComputeDockLayout(
        empty_tab_dock, 1'200, 180, 96U, &empty_tab_model);
    for (const DockPaneType type : constrained_right_panes) {
        const auto& pane = constrained_geometry.panes[
            static_cast<std::size_t>(type)];
        if (!pane.shown || pane.bounds.height <= 0) {
            return 128;
        }
    }

    DockLayoutModel auxiliary_model{};
    if (auxiliary_model.IsPaneVisible(DockPaneType::Locator)
        || auxiliary_model.SetPaneAutoHide(DockPaneType::Locator, true)
            != DockResult::Ok
        || auxiliary_model.Pane(DockPaneType::Locator)->zone
            != DockZone::AutoHide
        || auxiliary_model.SetPaneAutoHide(DockPaneType::Locator, true)
            != DockResult::NoOp
        || auxiliary_model.SetPaneAutoHide(DockPaneType::Locator, false)
            != DockResult::Ok
        || auxiliary_model.Pane(DockPaneType::Locator)->zone
            != DockZone::Right
        || auxiliary_model.SetPaneAutoHide(DockPaneType::JobProgress, true)
            != DockResult::ZoneNotAllowed
        || auxiliary_model.FloatPane(
               DockPaneType::JobProgress,
               DockFloatingPlacement{0, 0, 720, 112})
            != DockResult::ZoneNotAllowed) {
        return 43;
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

    DockLayoutModel grouped_inspector{};
    const std::uint32_t original_layer_weight =
        grouped_inspector.Pane(DockPaneType::Layer)->split_weight;
    if (grouped_inspector.RestorePane(DockPaneType::LightTable)
            != DockResult::Ok
        || grouped_inspector.RestorePane(DockPaneType::Reference)
            != DockResult::Ok
        || grouped_inspector.StackCount(DockZone::Right) != 2U
        || grouped_inspector.StackPaneCount(
               DockZone::Right,
               grouped_inspector.Pane(DockPaneType::Layer)->stack)
            != 3U
        || grouped_inspector.Pane(DockPaneType::Layer)->stack
            != grouped_inspector.Pane(DockPaneType::LightTable)->stack
        || grouped_inspector.Pane(DockPaneType::Layer)->stack
            != grouped_inspector.Pane(DockPaneType::Reference)->stack
        || grouped_inspector.Zone(DockZone::Right)->mode
            != DockStackMode::Mixed
        || grouped_inspector.SetActiveTab(
               DockZone::Right, DockPaneType::Reference)
            != DockResult::Ok) {
        return 47;
    }
    const auto grouped_geometry = ComputeDockLayout(
        grouped_inspector, 1'200, 720, 96U);
    const auto& grouped_color = grouped_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Color)];
    const auto& grouped_layer = grouped_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Layer)];
    const auto& grouped_light_table = grouped_geometry.panes[
        static_cast<std::size_t>(DockPaneType::LightTable)];
    const auto& grouped_reference = grouped_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Reference)];
    if (!grouped_color.shown || grouped_layer.shown || grouped_light_table.shown
        || !grouped_reference.shown) {
        return 48;
    }
    if (grouped_color.bounds.height >= grouped_reference.bounds.height
        || grouped_layer.bounds.x != grouped_reference.bounds.x
        || grouped_layer.bounds.y != grouped_reference.bounds.y
        || grouped_layer.bounds.width != grouped_reference.bounds.width
        || grouped_layer.bounds.height != grouped_reference.bounds.height
        || grouped_light_table.bounds.x != grouped_reference.bounds.x
        || grouped_light_table.bounds.y != grouped_reference.bounds.y
        || grouped_light_table.bounds.width != grouped_reference.bounds.width
        || grouped_light_table.bounds.height != grouped_reference.bounds.height) {
        return 51;
    }
    if (grouped_inspector.Pane(DockPaneType::Layer)->split_weight
            != original_layer_weight
        || grouped_inspector.Pane(DockPaneType::LightTable)->split_weight
            != original_layer_weight
        || grouped_inspector.Pane(DockPaneType::Reference)->split_weight
            != original_layer_weight) {
        return 52;
    }
    if (grouped_inspector.HidePane(DockPaneType::Reference) != DockResult::Ok
        || !grouped_inspector.Pane(DockPaneType::Layer)->active_tab
        || grouped_inspector.Zone(DockZone::Right)->mode
            != DockStackMode::Mixed
        || grouped_inspector.HidePane(DockPaneType::LightTable) != DockResult::Ok
        || grouped_inspector.Zone(DockZone::Right)->mode
            != DockStackMode::Split) {
        return 53;
    }

    const auto record = model.ToRecord();
    DockLayoutModel restored{};
    if (!restored.LoadRecord(record)) {
        return 7;
    }
    if (restored.Pane(DockPaneType::Color)->split_weight != 400U) {
        return 54;
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
        || normal.dock.right_tool_tabs.x != 880
        || normal.dock.right_tool_tabs.y != 44
        || normal.dock.right_tool_tabs.width != 320
        || normal.dock.right_tool_tabs.height != 28
        || color.bounds.y != 72 || layer.bounds.y != 301
        || color.bounds.height != 225 || layer.bounds.height != 479
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

    if (state.right_tool_tabs.SetSelected(kToolTabReference)
            != ToolTabResult::Ok) {
        return 13;
    }
    const auto tabbed = ComputeWorkspaceLayout(1'200, 800, 0, 96U, state);
    if (tabbed.dock.panes[static_cast<std::size_t>(DockPaneType::Color)].shown
        || tabbed.dock.panes[static_cast<std::size_t>(DockPaneType::Layer)].shown
        || tabbed.dock.right_tool_tabs.height != 28
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
    if (state.right_tool_tabs.SetVisible(kToolTabColoring, false)
            != ToolTabResult::Ok
        || state.right_tool_tabs.SetVisible(kToolTabReference, false)
            != ToolTabResult::Ok
        || state.right_tool_tabs.SetVisible(kToolTabWorkflow, false)
            != ToolTabResult::Ok
        || state.right_tool_tabs.Selected()
        || state.right_tool_tabs.HasVisibleTabs()) {
        return 15;
    }
    const auto canvas_only = ComputeWorkspaceLayout(1'200, 800, 0, 96U, state);
    if (Width(canvas_only.editor) != 1'200 || Height(canvas_only.editor) != 800) {
        return 16;
    }

    WorkspaceLayoutState preset{};
    preset.window = inkpod::windows::ui::WorkspaceWindowPlacement{
        WorkspaceScreenPlacement{100, 120, 1'000, 700, 96U, true},
        SW_SHOWMAXIMIZED};
    if (!ApplyWorkspacePreset(preset, WorkspacePreset::ReferenceCheck)
        || preset.selected_preset != WorkspacePreset::ReferenceCheck
        || preset.window.x_px != 100
        || preset.dock.IsPaneVisible(DockPaneType::Tool)
        || !preset.dock.IsPaneVisible(DockPaneType::Color)
        || FindWorkspaceAuxiliaryPane(
               preset, WorkspaceAuxiliaryPane::Locator)
                ->auto_hide
            == false
        || FindWorkspaceAuxiliaryPane(
               preset, WorkspaceAuxiliaryPane::Reference)
                ->visible
            == false
        || preset.dock.Zone(DockZone::Right)->mode != DockStackMode::Tabs
        || preset.dock.Zone(DockZone::Right)->active_tab
            != DockPaneType::Reference) {
        return 22;
    }
    const auto reference_layout = ComputeWorkspaceLayout(
        1'200, 800, 0, 96U, preset);
    const RECT locator_button = reference_layout.auto_hide_buttons[
        static_cast<std::size_t>(WorkspaceAuxiliaryPane::Locator)];
    if (Width(locator_button) <= 0 || Height(locator_button) <= 0
        || reference_layout.editor.right >= reference_layout.dock.editor.x
                + reference_layout.dock.editor.width) {
        return 23;
    }
    preset.density = WorkspaceDensity::Compact;
    const auto compact = ComputeWorkspaceLayout(1'200, 800, 0, 96U, preset);
    if (Height(compact.document_tabs) >= Height(reference_layout.document_tabs)
        || Width(compact.auto_hide_buttons[
               static_cast<std::size_t>(WorkspaceAuxiliaryPane::Locator)])
            >= Width(locator_button)) {
        return 24;
    }

    WorkspaceLayoutState serialized = preset;
    serialized.split_orientation = WorkspaceSplitOrientation::Horizontal;
    serialized.split_ratio_milli = 650U;
    if (serialized.dock.RestorePane(DockPaneType::JobProgress)
            != DockResult::Ok
        || serialized.dock.Zone(DockZone::Bottom)->extent_dip != 112) {
        return 44;
    }
    if (!SetWorkspaceCustomName(serialized, L"仕上げ確認")) {
        return 25;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> bytes{};
    std::size_t written{};
    if (!EncodeWorkspaceLayout(serialized, bytes, written) || written == 0U) {
        return 26;
    }
    WorkspaceLayoutState decoded{};
    if (DecodeWorkspaceLayout(
            decoded, std::span<const std::byte>(bytes.data(), written))
            != WorkspaceLayoutDecodeResult::Current
        || decoded.selected_preset != WorkspacePreset::Custom
        || std::wstring(decoded.custom_name.data()) != L"仕上げ確認"
        || decoded.split_orientation != WorkspaceSplitOrientation::Horizontal
        || decoded.split_ratio_milli != 650U
        || decoded.dock.Pane(DockPaneType::Locator)->zone
            != DockZone::AutoHide
        || !decoded.dock.IsPaneVisible(DockPaneType::Reference)
        || decoded.dock.IsPaneVisible(DockPaneType::JobProgress)
        || !decoded.window.valid || decoded.window.show_command != SW_SHOWMAXIMIZED) {
        return 27;
    }

    WorkspaceLayoutState mixed_serialized{};
    if (mixed_serialized.dock.RestorePane(DockPaneType::LightTable)
            != DockResult::Ok
        || mixed_serialized.dock.RestorePane(DockPaneType::Reference)
            != DockResult::Ok
        || mixed_serialized.dock.SetActiveTab(
               DockZone::Right, DockPaneType::Reference)
            != DockResult::Ok) {
        return 49;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> mixed_bytes{};
    std::size_t mixed_written{};
    WorkspaceLayoutState mixed_decoded{};
    if (!EncodeWorkspaceLayout(mixed_serialized, mixed_bytes, mixed_written)
        || DecodeWorkspaceLayout(
               mixed_decoded,
               std::span<const std::byte>(mixed_bytes.data(), mixed_written))
            != WorkspaceLayoutDecodeResult::Current
        || mixed_decoded.dock.Zone(DockZone::Right)->mode
            != DockStackMode::Mixed
        || mixed_decoded.dock.StackCount(DockZone::Right) != 2U
        || mixed_decoded.dock.Pane(DockPaneType::Layer)->stack
            != mixed_decoded.dock.Pane(DockPaneType::LightTable)->stack
        || mixed_decoded.dock.Pane(DockPaneType::Layer)->stack
            != mixed_decoded.dock.Pane(DockPaneType::Reference)->stack
        || !mixed_decoded.dock.Pane(DockPaneType::Reference)->active_tab) {
        return 50;
    }

    WorkspaceLayoutState legacy_reference_split{};
    if (legacy_reference_split.dock.RestorePane(DockPaneType::Reference)
            != DockResult::Ok) {
        return 58;
    }
    auto legacy_reference_record = legacy_reference_split.dock.ToRecord();
    auto& legacy_reference = legacy_reference_record.panes[
        static_cast<std::size_t>(DockPaneType::Reference)];
    legacy_reference.stack = static_cast<std::uint8_t>(DockPaneType::Reference);
    legacy_reference.order = 2U;
    legacy_reference.tab_order = 0U;
    legacy_reference.split_weight = 1000U;
    legacy_reference.active_tab = true;
    auto& legacy_reference_zone = legacy_reference_record.zones[
        static_cast<std::size_t>(DockZone::Right)];
    legacy_reference_zone.mode = DockStackMode::Split;
    legacy_reference_zone.active_tab = DockPaneType::Reference;
    if (!legacy_reference_split.dock.LoadRecord(legacy_reference_record)) {
        return 59;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes>
        legacy_reference_bytes{};
    std::size_t legacy_reference_written{};
    if (!EncodeWorkspaceLayout(
            legacy_reference_split,
            legacy_reference_bytes,
            legacy_reference_written)
        || !DowngradeGroupedLayoutToV6(std::span<std::byte>(
            legacy_reference_bytes.data(), legacy_reference_written))) {
        return 60;
    }
    WorkspaceLayoutState migrated_reference_stack{};
    if (DecodeWorkspaceLayout(
            migrated_reference_stack,
            std::span<const std::byte>(
                legacy_reference_bytes.data(), legacy_reference_written))
            != WorkspaceLayoutDecodeResult::Migrated
        || migrated_reference_stack.dock.StackCount(DockZone::Right) != 2U
        || migrated_reference_stack.dock.Pane(DockPaneType::Reference)->stack
            != migrated_reference_stack.dock.Pane(DockPaneType::Layer)->stack
        || migrated_reference_stack.dock.Zone(DockZone::Right)->mode
            != DockStackMode::Mixed
        || !migrated_reference_stack.dock.Pane(
                DockPaneType::Reference)->active_tab) {
        return 61;
    }

    WorkspaceLayoutState legacy_explicit_reference_split{};
    if (legacy_explicit_reference_split.dock.RestorePane(
            DockPaneType::Reference)
            != DockResult::Ok) {
        return 62;
    }
    auto explicit_reference_record =
        legacy_explicit_reference_split.dock.ToRecord();
    auto& explicit_reference = explicit_reference_record.panes[
        static_cast<std::size_t>(DockPaneType::Reference)];
    explicit_reference.stack = 2U;
    explicit_reference.order = 2U;
    explicit_reference.tab_order = 0U;
    explicit_reference.split_weight = 1000U;
    explicit_reference.active_tab = true;
    auto& explicit_reference_zone = explicit_reference_record.zones[
        static_cast<std::size_t>(DockZone::Right)];
    explicit_reference_zone.mode = DockStackMode::Split;
    explicit_reference_zone.active_tab = DockPaneType::Reference;
    if (!legacy_explicit_reference_split.dock.LoadRecord(
            explicit_reference_record)) {
        return 63;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes>
        explicit_reference_bytes{};
    std::size_t explicit_reference_written{};
    WorkspaceLayoutState preserved_reference_split{};
    if (!EncodeWorkspaceLayout(
            legacy_explicit_reference_split,
            explicit_reference_bytes,
            explicit_reference_written)
        || !DowngradeGroupedLayoutToV6(std::span<std::byte>(
            explicit_reference_bytes.data(), explicit_reference_written))
        || DecodeWorkspaceLayout(
               preserved_reference_split,
               std::span<const std::byte>(
                   explicit_reference_bytes.data(), explicit_reference_written))
            != WorkspaceLayoutDecodeResult::Migrated
        || preserved_reference_split.dock.StackCount(DockZone::Right) != 3U
        || preserved_reference_split.dock.Pane(DockPaneType::Reference)->stack
            == preserved_reference_split.dock.Pane(DockPaneType::Layer)->stack) {
        return 64;
    }

    WorkspaceLayoutState legacy_light_table_tabs{};
    if (legacy_light_table_tabs.dock.RestorePane(DockPaneType::LightTable)
            != DockResult::Ok
        || legacy_light_table_tabs.dock.SetZoneMode(
               DockZone::Right, DockStackMode::Tabs)
            != DockResult::Ok) {
        return 55;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> legacy_tabs_bytes{};
    std::size_t legacy_tabs_written{};
    if (!EncodeWorkspaceLayout(
            legacy_light_table_tabs, legacy_tabs_bytes, legacy_tabs_written)
        || !DowngradeGroupedLayoutToV5(
            std::span<std::byte>(
                legacy_tabs_bytes.data(), legacy_tabs_written),
            legacy_light_table_tabs.dock.ToRecord())) {
        return 56;
    }
    WorkspaceLayoutState migrated_light_table_tabs{};
    if (DecodeWorkspaceLayout(
            migrated_light_table_tabs,
            std::span<const std::byte>(
                legacy_tabs_bytes.data(), legacy_tabs_written))
            != WorkspaceLayoutDecodeResult::Migrated
        || migrated_light_table_tabs.dock.Zone(DockZone::Right)->mode
            != DockStackMode::Mixed
        || migrated_light_table_tabs.dock.Pane(DockPaneType::Color)->stack
            == migrated_light_table_tabs.dock.Pane(DockPaneType::Layer)->stack
        || migrated_light_table_tabs.dock.Pane(DockPaneType::Layer)->stack
            != migrated_light_table_tabs.dock.Pane(
                   DockPaneType::LightTable)->stack
        || !migrated_light_table_tabs.dock.Pane(DockPaneType::Color)->active_tab
        || !migrated_light_table_tabs.dock.Pane(DockPaneType::Layer)->active_tab) {
        return 57;
    }

    auto legacy_v5_bytes = bytes;
    const auto serialized_record = serialized.dock.ToRecord();
    if (!DowngradeGroupedLayoutToV5(
            std::span<std::byte>(legacy_v5_bytes.data(), written),
            serialized_record)) {
        return 45;
    }
    WorkspaceLayoutState migrated_v5{};
    if (DecodeWorkspaceLayout(
            migrated_v5,
            std::span<const std::byte>(legacy_v5_bytes.data(), written))
            != WorkspaceLayoutDecodeResult::Migrated
        || migrated_v5.dock.Pane(DockPaneType::Locator)->zone
            != DockZone::AutoHide
        || !migrated_v5.dock.IsPaneVisible(DockPaneType::Reference)
        || migrated_v5.dock.IsPaneVisible(DockPaneType::JobProgress)) {
        return 46;
    }
    WorkspaceLayoutState rejected = decoded;
    if (DecodeWorkspaceLayout(
            rejected, std::span<const std::byte>(bytes.data(), written - 1U))
            != WorkspaceLayoutDecodeResult::Invalid
        || rejected.selected_preset != WorkspacePreset::Coloring) {
        return 28;
    }
    rejected = decoded;
    if (DecodeWorkspaceLayout(
            rejected, std::span<const std::byte>(bytes.data(), written + 1U))
        != WorkspaceLayoutDecodeResult::Invalid) {
        return 29;
    }
    auto unknown_pane = bytes;
    const std::uint32_t unknown_id = UINT32_C(0xfefefefe);
    if (!ReplaceFirst(
            std::span<std::byte>(unknown_pane.data(), written),
            PaneDescriptors()[0].stable_type_id,
            unknown_id)) {
        return 30;
    }
    WorkspaceLayoutState supplemented{};
    if (DecodeWorkspaceLayout(
            supplemented,
            std::span<const std::byte>(unknown_pane.data(), written))
            != WorkspaceLayoutDecodeResult::Current
        || supplemented.dock.Pane(DockPaneType::Tool) == nullptr
        || supplemented.dock.Pane(DockPaneType::Tool)->zone != DockZone::Left) {
        return 31;
    }

    const LegacyWorkspaceV2 legacy{};
    WorkspaceLayoutState migrated{};
    if (DecodeWorkspaceLayout(
            migrated,
            std::span<const std::byte>(
                reinterpret_cast<const std::byte*>(&legacy), sizeof(legacy)))
            != WorkspaceLayoutDecodeResult::Migrated
        || migrated.layer_split_milli != 550U
        || !migrated.dock.IsPaneVisible(DockPaneType::Tool)) {
        return 32;
    }
    const auto& legacy_record = serialized_record;
    LegacyWorkspaceV3 legacy_v3{};
    legacy_v3.flags = legacy_record.mirrored;
    legacy_v3.layer_split_milli = serialized.layer_split_milli;
    for (std::size_t index = 0U; index < legacy_v3.panes.size(); ++index) {
        const auto& source = legacy_record.panes[index];
        legacy_v3.panes[index] = LegacyDockPaneV3{
            static_cast<std::uint32_t>(source.type),
            static_cast<std::uint32_t>(source.zone),
            static_cast<std::uint32_t>(source.restore_zone),
            source.order,
            source.split_weight,
            source.floating.x_dip,
            source.floating.y_dip,
            source.floating.width_dip,
            source.floating.height_dip};
    }
    for (std::size_t index = 0U; index < legacy_v3.zones.size(); ++index) {
        const auto& source = legacy_record.zones[index];
        const DockPaneType active_tab = source.active_tab != DockPaneType::Count
                && static_cast<std::size_t>(source.active_tab) >= 4U
            ? (index == static_cast<std::size_t>(DockZone::Right)
                   ? DockPaneType::Color
                   : DockPaneType::Count)
            : source.active_tab;
        legacy_v3.zones[index] = LegacyDockZoneV3{
            static_cast<std::uint32_t>(source.mode),
            static_cast<std::uint32_t>(active_tab),
            source.extent_dip};
    }
    if (DecodeWorkspaceLayout(
            migrated,
            std::span<const std::byte>(
                reinterpret_cast<const std::byte*>(&legacy_v3),
                sizeof(legacy_v3)))
            != WorkspaceLayoutDecodeResult::Migrated
        || migrated.layer_split_milli != serialized.layer_split_milli
        || migrated.dock.IsPaneVisible(DockPaneType::Tool)) {
        return 38;
    }
    const WorkspaceLayoutState before_invalid_preset = migrated;
    if (ApplyWorkspacePreset(migrated, WorkspacePreset::Custom)
        || migrated.layer_split_milli
            != before_invalid_preset.layer_split_milli
        || SetWorkspaceCustomName(migrated, L"")) {
        return 39;
    }

    WorkspaceScreenPlacement missing_monitor{
        -3'000, 100, 1'000, 700, 96U, true};
    const std::array<WorkspaceWorkArea, 2U> work_areas{{
        {RECT{0, 0, 1'920, 1'040}, 96U, true},
        {RECT{1'920, 0, 4'800, 1'760}, 144U, false},
    }};
    if (!ClampWorkspacePlacement(missing_monitor, work_areas)
        || missing_monitor.x_px < 0 || missing_monitor.y_px < 0
        || missing_monitor.x_px + missing_monitor.width_px > 1'920
        || missing_monitor.y_px + missing_monitor.height_px > 1'040) {
        return 33;
    }
    WorkspaceScreenPlacement added_monitor{
        2'100, 200, 1'000, 700, 96U, true};
    if (!ClampWorkspacePlacement(added_monitor, work_areas)
        || added_monitor.capture_dpi != 144U
        || added_monitor.width_px != 1'500
        || added_monitor.x_px < 1'920) {
        return 34;
    }
    WorkspaceScreenPlacement changed_primary{
        -3'000, 100, 1'000, 700, 96U, true};
    const std::array<WorkspaceWorkArea, 2U> primary_changed_work_areas{{
        {RECT{0, 0, 1'920, 1'040}, 96U, false},
        {RECT{1'920, 0, 4'800, 1'760}, 144U, true},
    }};
    if (!ClampWorkspacePlacement(
            changed_primary, primary_changed_work_areas)
        || changed_primary.x_px < 1'920
        || changed_primary.capture_dpi != 144U) {
        return 40;
    }
    decoded.window = inkpod::windows::ui::WorkspaceWindowPlacement{
        WorkspaceScreenPlacement{-4'000, 0, 900, 600, 96U, true},
        SW_SHOWNORMAL};
    auto* floating_locator = FindWorkspaceAuxiliaryPane(
        decoded, WorkspaceAuxiliaryPane::Locator);
    floating_locator->floating = WorkspaceScreenPlacement{
        8'000, 0, 300, 240, 96U, true};
    ClampWorkspaceFloatingPanes(decoded, work_areas);
    if (decoded.window.x_px < 0
        || floating_locator->floating.x_px
            + floating_locator->floating.width_px
            > 1'920) {
        return 35;
    }

    std::array<wchar_t, 96U> registry_name{};
    _snwprintf_s(
        registry_name.data(),
        registry_name.size(),
        _TRUNCATE,
        L"WorkspaceG9Test_%lu",
        static_cast<unsigned long>(GetCurrentProcessId()));
    if (!SaveWorkspaceLayout(serialized, registry_name.data())) {
        return 36;
    }
    WorkspaceLayoutState restarted{};
    const bool restart_loaded = LoadWorkspaceLayout(
        restarted, registry_name.data());
    static_cast<void>(DeleteWorkspaceLayout(registry_name.data()));
    if (!restart_loaded
        || restarted.selected_preset != WorkspacePreset::Custom
        || restarted.split_ratio_milli != 650U) {
        return 37;
    }

    _snwprintf_s(
        registry_name.data(),
        registry_name.size(),
        _TRUNCATE,
        L"WorkspaceG9MigrationTest_%lu",
        static_cast<unsigned long>(GetCurrentProcessId()));
    HKEY settings{};
    DWORD disposition{};
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            L"Software\\Inkpod",
            0,
            nullptr,
            0,
            KEY_SET_VALUE,
            nullptr,
            &settings,
            &disposition)
            != ERROR_SUCCESS
        || RegSetValueExW(
               settings,
               registry_name.data(),
               0,
               REG_BINARY,
               reinterpret_cast<const BYTE*>(&legacy),
               sizeof(legacy))
            != ERROR_SUCCESS) {
        if (settings != nullptr) {
            RegCloseKey(settings);
        }
        return 41;
    }
    RegCloseKey(settings);
    WorkspaceLayoutState registry_migrated{};
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> migrated_bytes{};
    DWORD migrated_type{};
    DWORD migrated_size = static_cast<DWORD>(migrated_bytes.size());
    const bool loaded_legacy = LoadWorkspaceLayout(
        registry_migrated, registry_name.data());
    const LSTATUS read_migrated = RegGetValueW(
        HKEY_CURRENT_USER,
        L"Software\\Inkpod",
        registry_name.data(),
        RRF_RT_REG_BINARY,
        &migrated_type,
        migrated_bytes.data(),
        &migrated_size);
    static_cast<void>(DeleteWorkspaceLayout(registry_name.data()));
    WorkspaceLayoutState migrated_again{};
    if (!loaded_legacy || read_migrated != ERROR_SUCCESS
        || DecodeWorkspaceLayout(
               migrated_again,
               std::span<const std::byte>(
                   migrated_bytes.data(), migrated_size))
            != WorkspaceLayoutDecodeResult::Current) {
        return 42;
    }

    return 0;
}
