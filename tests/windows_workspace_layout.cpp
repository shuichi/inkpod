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
using inkpod::windows::ui::DockFloatingPlacement;
using inkpod::windows::ui::DockLayoutModel;
using inkpod::windows::ui::DockPaneType;
using inkpod::windows::ui::DockResult;
using inkpod::windows::ui::DockSplitterGeometry;
using inkpod::windows::ui::DockSplitterKind;
using inkpod::windows::ui::DockStackMode;
using inkpod::windows::ui::DockZone;
using inkpod::windows::ui::PaneDescriptors;
using inkpod::windows::ui::RightToolTabsModel;
using inkpod::windows::ui::ToolTab;
using inkpod::windows::ui::ToolTabId;
using inkpod::windows::ui::ToolTabResult;
using inkpod::windows::ui::ApplyWorkspacePreset;
using inkpod::windows::ui::ClampWorkspaceFloatingPanes;
using inkpod::windows::ui::ClampWorkspacePlacement;
using inkpod::windows::ui::EncodeWorkspaceLayout;
using inkpod::windows::ui::FindWorkspaceAuxiliaryPane;
using inkpod::windows::ui::ResetWorkspaceLayout;
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
    return ReplaceFirst(bytes, UINT32_C(8), UINT32_C(5));
}

bool DowngradeGroupedLayoutToV6(std::span<std::byte> bytes) noexcept {
    return ReplaceFirst(bytes, UINT32_C(8), UINT32_C(6));
}

bool DowngradeGroupedLayoutToV7(std::span<std::byte> bytes) noexcept {
    return ReplaceFirst(bytes, UINT32_C(8), UINT32_C(7));
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

struct PersistedDockPaneForMigration {
    std::uint32_t stable_type_id{};
    std::uint32_t zone{};
    std::uint32_t restore_zone{};
    std::uint32_t order{};
    std::uint32_t split_weight{};
    std::int32_t floating_x_dip{};
    std::int32_t floating_y_dip{};
    std::int32_t floating_width_dip{};
    std::int32_t floating_height_dip{};
};

struct PersistedScreenPlacementForMigration {
    std::int32_t x_px{};
    std::int32_t y_px{};
    std::int32_t width_px{};
    std::int32_t height_px{};
    std::uint32_t capture_dpi{};
    std::uint32_t valid{};
};

struct PersistedAuxiliaryPaneForMigration {
    std::uint32_t stable_type_id{};
    std::uint32_t flags{};
    std::uint32_t edge{};
    std::uint32_t reserved{};
    PersistedScreenPlacementForMigration floating{};
};

struct PersistedWorkspaceForMigration {
    std::uint32_t magic{};
    std::uint32_t version{};
    std::uint32_t struct_size{};
    std::uint32_t flags{};
    std::uint32_t pane_count{};
    std::uint32_t zone_count{};
    std::uint32_t auxiliary_count{};
    std::uint32_t selected_preset{};
    std::uint32_t density{};
    std::uint32_t split_orientation{};
    std::uint32_t split_ratio_milli{};
    std::uint32_t layer_split_milli{};
    std::uint32_t reserved0{};
    std::uint32_t window_show_command{};
    PersistedScreenPlacementForMigration window{};
    std::array<wchar_t, 64U> custom_name{};
    std::array<PersistedDockPaneForMigration, 16U> panes{};
    std::array<LegacyDockZoneV3, kDockedZoneCount> zones{};
    std::array<PersistedAuxiliaryPaneForMigration, 16U> auxiliary{};
};

struct PersistedRightToolTabForMigration {
    std::uint32_t stable_id{};
    std::uint32_t pane_count{};
    std::array<std::uint32_t, kDockPaneCount> pane_stable_type_ids{};
};

struct PersistedWorkspaceV9ForMigration {
    PersistedWorkspaceForMigration workspace{};
    std::uint32_t tab_count{};
    std::uint32_t selected_tab_id{};
    std::uint32_t next_tab_id{};
    std::uint32_t reserved{};
    std::array<PersistedRightToolTabForMigration, kDockPaneCount> tabs{};
};

bool ExtractVersion8Workspace(
    std::span<const std::byte> current,
    std::span<std::byte> legacy,
    std::size_t& written) noexcept {
    written = 0U;
    if (current.size() != sizeof(PersistedWorkspaceV9ForMigration)
        || legacy.size() < sizeof(PersistedWorkspaceForMigration)) {
        return false;
    }
    PersistedWorkspaceV9ForMigration value{};
    std::memcpy(&value, current.data(), sizeof(value));
    value.workspace.version = 8U;
    value.workspace.struct_size = sizeof(PersistedWorkspaceForMigration);
    std::memcpy(legacy.data(), &value.workspace, sizeof(value.workspace));
    written = sizeof(value.workspace);
    return true;
}

bool ValidDynamicRightTabs(const WorkspaceLayoutState& state) noexcept {
    std::array<bool, kDockPaneCount> seen_panes{};
    std::array<std::uint32_t, kDockPaneCount> seen_ids{};
    std::size_t seen_id_count{};
    bool selected_seen = !state.right_tool_tabs.Selected()
        && state.right_tool_tabs.Tabs().empty();
    for (const ToolTab& tab : state.right_tool_tabs.Tabs()) {
        if (!tab.id || tab.pane_count == 0U
            || tab.pane_count > tab.panes.size()) {
            return false;
        }
        for (std::size_t index = 0U; index < seen_id_count; ++index) {
            if (seen_ids[index] == tab.id.Value()) {
                return false;
            }
        }
        seen_ids[seen_id_count++] = tab.id.Value();
        selected_seen = selected_seen || tab.id == state.right_tool_tabs.Selected();
        for (std::size_t index = 0U; index < tab.pane_count; ++index) {
            const std::size_t pane_index = static_cast<std::size_t>(
                tab.panes[index]);
            const auto* placement = state.dock.Pane(tab.panes[index]);
            if (pane_index >= seen_panes.size() || seen_panes[pane_index]
                || placement == nullptr || !placement->present
                || placement->zone != DockZone::Right) {
                return false;
            }
            seen_panes[pane_index] = true;
        }
    }
    if (!selected_seen) {
        return false;
    }
    for (const auto& descriptor : PaneDescriptors()) {
        const auto* placement = state.dock.Pane(descriptor.type);
        if (descriptor.type != DockPaneType::Tool
            && descriptor.type != DockPaneType::ToolOptions
            && descriptor.type != DockPaneType::JobProgress
            && (descriptor.allowed_zones
                & inkpod::windows::ui::DockZoneBit(DockZone::Right)) != 0U
            && placement != nullptr && placement->present
            && placement->zone == DockZone::Right
            && !seen_panes[static_cast<std::size_t>(descriptor.type)]) {
            return false;
        }
    }
    return true;
}

}  // namespace

int main() {
    const auto& descriptors = PaneDescriptors();
    const auto& locator_descriptor = descriptors[
        static_cast<std::size_t>(DockPaneType::Locator)];
    const auto& tool_options_descriptor = descriptors[
        static_cast<std::size_t>(DockPaneType::ToolOptions)];
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
        || PaneDescriptors()[0].can_float
        || PaneDescriptors()[0].show_header_when_singleton
        || PaneDescriptors()[1].show_header_when_singleton
        || tool_options_descriptor.default_visible
        || tool_options_descriptor.persist_layout
        || tool_options_descriptor.can_float
        || tool_options_descriptor.allowed_zones
            != (inkpod::windows::ui::DockZoneBit(DockZone::TopContext)
                | inkpod::windows::ui::DockZoneBit(DockZone::Hidden))
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
    const ToolTabId initial_tab{1U};
    if (tool_tabs.Tabs().size() != 1U
        || tool_tabs.Selected() != initial_tab
        || tool_tabs.Tabs()[0].pane_count != 2U
        || tool_tabs.Tabs()[0].panes[0] != DockPaneType::Color
        || tool_tabs.Tabs()[0].panes[1] != DockPaneType::Layer
        || tool_tabs.TabForPane(DockPaneType::Color) != initial_tab
        || tool_tabs.TabForPane(DockPaneType::Locator)) {
        return 120;
    }
    if (tool_tabs.AddPaneToSelected(DockPaneType::Locator, 10'000, 96U, 6)
            != ToolTabResult::Ok
        || tool_tabs.Tabs().size() != 1U
        || tool_tabs.ReorderPane(
               DockPaneType::Locator, DockPaneType::Color, false)
            != ToolTabResult::Ok
        || tool_tabs.Tabs()[0].panes[0] != DockPaneType::Locator
        || tool_tabs.MovePaneToNewTab(DockPaneType::Locator)
            != ToolTabResult::Ok
        || tool_tabs.Tabs().size() != 2U
        || tool_tabs.Selected() != ToolTabId{2U}
        || tool_tabs.AddPaneToSelected(
               DockPaneType::Reference, 10'000, 96U, 6)
            != ToolTabResult::Ok
        || tool_tabs.TabForPane(DockPaneType::Reference) != ToolTabId{2U}
        || tool_tabs.AddPaneToSelected(
               DockPaneType::LightTable, 0, 96U, 6)
            != ToolTabResult::Ok
        || tool_tabs.Tabs().size() != 3U
        || tool_tabs.Selected() != ToolTabId{3U}) {
        return 121;
    }
    if (tool_tabs.Reorder(ToolTabId{3U}, ToolTabId{1U}, false)
            != ToolTabResult::Ok
        || tool_tabs.Tabs()[0].id != ToolTabId{3U}
        || tool_tabs.Tabs()[1].id != ToolTabId{1U}
        || tool_tabs.Tabs()[2].id != ToolTabId{2U}
        || tool_tabs.ReorderPane(
               DockPaneType::Reference, DockPaneType::Locator, false)
            != ToolTabResult::Ok
        || tool_tabs.MovePane(DockPaneType::Color, ToolTabId{2U})
            != ToolTabResult::Ok
        || tool_tabs.MovePane(DockPaneType::Layer, ToolTabId{2U})
            != ToolTabResult::Ok
        || tool_tabs.Tabs().size() != 2U
        || tool_tabs.Find(ToolTabId{1U}) != nullptr
        || tool_tabs.TabForPane(DockPaneType::Layer) != ToolTabId{2U}) {
        return 122;
    }
    const auto stable_count = tool_tabs.Tabs().size();
    const ToolTabId stable_selected = tool_tabs.Selected();
    const std::uint32_t stable_next_id = tool_tabs.NextStableId();
    if (tool_tabs.MovePane(DockPaneType::Tool, ToolTabId{2U})
            != ToolTabResult::InvalidPane
        || tool_tabs.MovePane(DockPaneType::Layer, ToolTabId{99U})
            != ToolTabResult::InvalidTab
        || tool_tabs.Tabs().size() != stable_count
        || tool_tabs.Selected() != stable_selected
        || tool_tabs.NextStableId() != stable_next_id) {
        return 123;
    }

    RightToolTabsModel close_tabs{};
    if (close_tabs.AddPaneToSelected(
            DockPaneType::Locator, 10'000, 96U, 6)
            != ToolTabResult::Ok
        || close_tabs.MovePaneToNewTab(DockPaneType::Locator)
            != ToolTabResult::Ok) {
        return 135;
    }
    const auto close_before = close_tabs;
    std::array<DockPaneType, kDockPaneCount> closed_panes{};
    std::size_t closed_count{};
    std::array<DockPaneType, 1U> short_output{};
    if (close_tabs.CloseTab(ToolTabId{99U}, closed_panes, closed_count)
            != ToolTabResult::InvalidTab
        || closed_count != 0U
        || close_tabs.CloseTab(ToolTabId{1U}, short_output, closed_count)
            != ToolTabResult::CapacityExceeded
        || closed_count != 0U
        || close_tabs.Tabs().size() != close_before.Tabs().size()
        || close_tabs.Selected() != close_before.Selected()
        || close_tabs.CloseTab(ToolTabId{2U}, closed_panes, closed_count)
            != ToolTabResult::Ok
        || closed_count != 1U
        || closed_panes[0] != DockPaneType::Locator
        || close_tabs.Selected() != ToolTabId{1U}
        || close_tabs.CloseTab(ToolTabId{1U}, closed_panes, closed_count)
            != ToolTabResult::Ok
        || closed_count != 2U
        || closed_panes[0] != DockPaneType::Color
        || closed_panes[1] != DockPaneType::Layer
        || close_tabs.HasVisibleTabs()
        || close_tabs.Selected()) {
        return 136;
    }

    RightToolTabsModel exclusive_batch_tabs{};
    if (exclusive_batch_tabs.AddPaneToSelected(
            DockPaneType::Batch, 10'000, 96U, 6)
            != ToolTabResult::Ok
        || exclusive_batch_tabs.Tabs().size() != 2U
        || exclusive_batch_tabs.Selected() != ToolTabId{2U}
        || exclusive_batch_tabs.Tabs()[1].pane_count != 1U
        || exclusive_batch_tabs.Tabs()[1].panes[0] != DockPaneType::Batch
        || exclusive_batch_tabs.AddPaneToSelected(
               DockPaneType::Locator, 10'000, 96U, 6)
            != ToolTabResult::Ok
        || exclusive_batch_tabs.Tabs().size() != 3U
        || exclusive_batch_tabs.Selected() != ToolTabId{3U}
        || exclusive_batch_tabs.MovePane(
               DockPaneType::Batch, ToolTabId{1U})
            != ToolTabResult::InvalidTab
        || exclusive_batch_tabs.MovePane(
               DockPaneType::Color, ToolTabId{2U})
            != ToolTabResult::InvalidTab
        || exclusive_batch_tabs.TabForPane(DockPaneType::Batch)
            != ToolTabId{2U}) {
        return 133;
    }
    const std::array<ToolTab, 1U> invalid_mixed_batch_tab{{
        ToolTab{
            ToolTabId{21U},
            {DockPaneType::Batch, DockPaneType::Reference},
            2U},
    }};
    if (exclusive_batch_tabs.Load(
            invalid_mixed_batch_tab, ToolTabId{21U}, 22U)) {
        return 134;
    }

    const std::array<ToolTab, 3U> replacement_tabs{{
        ToolTab{ToolTabId{11U}, {DockPaneType::Sequence}, 1U},
        ToolTab{ToolTabId{12U}, {DockPaneType::Batch}, 1U},
        ToolTab{ToolTabId{13U}, {DockPaneType::Reference}, 1U},
    }};
    RightToolTabsModel replacement_model{};
    if (!replacement_model.Load(replacement_tabs, ToolTabId{12U}, 14U)
        || replacement_model.RemovePane(DockPaneType::Batch)
            != ToolTabResult::Ok
        || replacement_model.Selected() != ToolTabId{11U}
        || replacement_model.RemovePane(DockPaneType::Sequence)
            != ToolTabResult::Ok
        || replacement_model.Selected() != ToolTabId{13U}
        || replacement_model.RemovePane(DockPaneType::Reference)
            != ToolTabResult::Ok
        || replacement_model.HasVisibleTabs()
        || replacement_model.Selected()
        || replacement_model.AddPaneToSelected(
               DockPaneType::Color, 0, 96U, 6)
            != ToolTabResult::Ok
        || replacement_model.Selected() != ToolTabId{14U}) {
        return 124;
    }

    ToolTab capacity_tab{};
    capacity_tab.id = ToolTabId{41U};
    capacity_tab.panes[0] = DockPaneType::Color;
    capacity_tab.panes[1] = DockPaneType::Layer;
    capacity_tab.pane_count = 2U;
    RightToolTabsModel capacity_model{};
    const std::array<ToolTab, 1U> capacity_tabs{capacity_tab};
    if (!capacity_model.Load(
            capacity_tabs, ToolTabId{41U}, UINT32_MAX - 1U)
        || capacity_model.AddPaneToSelected(
               DockPaneType::Locator, 0, 96U, 6)
            != ToolTabResult::Ok
        || capacity_model.AddPaneToSelected(
               DockPaneType::Reference, 0, 96U, 6)
            != ToolTabResult::CapacityExceeded
        || capacity_model.TabForPane(DockPaneType::Reference)
        || capacity_model.Tabs().size() != 2U
        || capacity_model.NextStableId() != UINT32_MAX
        || capacity_model.Load(capacity_tabs, ToolTabId{41U}, 41U)) {
        return 125;
    }

    RightToolTabsModel selected_tab_model{};
    DockLayoutModel selected_tab_dock{};
    if (selected_tab_model.MovePaneToNewTab(DockPaneType::Layer)
            != ToolTabResult::Ok) {
        return 126;
    }
    const auto layer_tab_geometry = ComputeDockLayout(
        selected_tab_dock, 1'200, 720, 96U, &selected_tab_model);
    if (layer_tab_geometry.right_tool_tabs.height != 28
        || layer_tab_geometry.zones[
               static_cast<std::size_t>(DockZone::Right)].width
            != 320
        || layer_tab_geometry.panes[
               static_cast<std::size_t>(DockPaneType::Color)].shown
        || !layer_tab_geometry.panes[
               static_cast<std::size_t>(DockPaneType::Layer)].shown) {
        return 127;
    }

    if (selected_tab_model.SetSelected(initial_tab) != ToolTabResult::Ok
        || selected_tab_dock.RestorePane(DockPaneType::Locator) != DockResult::Ok
        || selected_tab_dock.RestorePane(DockPaneType::LightTable)
            != DockResult::Ok
        || selected_tab_dock.RestorePane(DockPaneType::Reference)
            != DockResult::Ok
        || selected_tab_model.EnsurePaneAssigned(DockPaneType::Locator)
            != ToolTabResult::Ok
        || selected_tab_model.EnsurePaneAssigned(DockPaneType::LightTable)
            != ToolTabResult::Ok
        || selected_tab_model.EnsurePaneAssigned(DockPaneType::Reference)
            != ToolTabResult::Ok) {
        return 128;
    }
    const auto reference_geometry = ComputeDockLayout(
        selected_tab_dock, 1'200, 720, 96U, &selected_tab_model);
    if (!reference_geometry.panes[
             static_cast<std::size_t>(DockPaneType::Color)].shown
        || reference_geometry.panes[
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
        return 130;
    }
    const std::array<DockPaneType, 4U> constrained_right_panes{
        DockPaneType::Locator,
        DockPaneType::LightTable,
        DockPaneType::Reference,
        DockPaneType::Color};
    for (const DockPaneType type : constrained_right_panes) {
        auto* pane = selected_tab_dock.Pane(type);
        if (pane == nullptr) {
            return 131;
        }
        pane->split_weight = type == DockPaneType::Locator ? 100'000U : 100U;
    }
    const auto constrained_geometry = ComputeDockLayout(
        selected_tab_dock, 1'200, 180, 96U, &selected_tab_model);
    for (const DockPaneType type : constrained_right_panes) {
        const auto& pane = constrained_geometry.panes[
            static_cast<std::size_t>(type)];
        if (!pane.shown || pane.bounds.height <= 0) {
            return 132;
        }
    }

    DockLayoutModel minimum_height_dock{};
    RightToolTabsModel minimum_height_tabs{};
    const auto& minimum_color_descriptor = PaneDescriptors()[
        static_cast<std::size_t>(DockPaneType::Color)];
    const auto& minimum_layer_descriptor = PaneDescriptors()[
        static_cast<std::size_t>(DockPaneType::Layer)];
    const auto minimum_height_geometry = ComputeDockLayout(
        minimum_height_dock, 1'200, 720, 96U, &minimum_height_tabs);
    const auto& minimum_color = minimum_height_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Color)];
    const auto& minimum_layer = minimum_height_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Layer)];
    if (minimum_color_descriptor.minimum_height_dip < 300
        || !minimum_color.shown || !minimum_layer.shown
        || minimum_color.bounds.height
            < minimum_color_descriptor.minimum_height_dip
        || minimum_layer.bounds.height
            < minimum_layer_descriptor.minimum_height_dip) {
        return 139;
    }
    const std::uint32_t minimum_color_weight =
        minimum_height_dock.Pane(DockPaneType::Color)->split_weight;
    const std::uint32_t minimum_layer_weight =
        minimum_height_dock.Pane(DockPaneType::Layer)->split_weight;
    if (minimum_height_dock.AdjustPaneBoundary(
            DockPaneType::Color,
            DockPaneType::Layer,
            -1'000,
            minimum_color.bounds.height + minimum_layer.bounds.height)
            != DockResult::NoOp
        || minimum_height_dock.Pane(DockPaneType::Color)->split_weight
            != minimum_color_weight
        || minimum_height_dock.Pane(DockPaneType::Layer)->split_weight
            != minimum_layer_weight) {
        return 140;
    }
    if (minimum_height_dock.AdjustPaneBoundary(
            DockPaneType::Color,
            DockPaneType::Layer,
            1'000,
            minimum_color.bounds.height + minimum_layer.bounds.height)
        != DockResult::Ok) {
        return 141;
    }
    const auto maximum_color_geometry = ComputeDockLayout(
        minimum_height_dock, 1'200, 720, 96U, &minimum_height_tabs);
    const auto& minimum_layer_after_grow = maximum_color_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Layer)];
    if (minimum_layer_after_grow.bounds.height
        < minimum_layer_descriptor.minimum_height_dip) {
        return 142;
    }
    const std::uint32_t maximum_color_weight =
        minimum_height_dock.Pane(DockPaneType::Color)->split_weight;
    const std::uint32_t minimum_layer_weight_after_grow =
        minimum_height_dock.Pane(DockPaneType::Layer)->split_weight;
    if (minimum_height_dock.AdjustPaneBoundary(
            DockPaneType::Color,
            DockPaneType::Layer,
            1'000,
            minimum_color.bounds.height + minimum_layer.bounds.height)
            != DockResult::NoOp
        || minimum_height_dock.Pane(DockPaneType::Color)->split_weight
            != maximum_color_weight
        || minimum_height_dock.Pane(DockPaneType::Layer)->split_weight
            != minimum_layer_weight_after_grow) {
        return 143;
    }
    DockLayoutModel high_dpi_minimum_dock{};
    RightToolTabsModel high_dpi_minimum_tabs{};
    const auto high_dpi_minimum_geometry = ComputeDockLayout(
        high_dpi_minimum_dock,
        2'400,
        1'440,
        192U,
        &high_dpi_minimum_tabs);
    const auto& high_dpi_color = high_dpi_minimum_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Color)];
    const auto& high_dpi_layer = high_dpi_minimum_geometry.panes[
        static_cast<std::size_t>(DockPaneType::Layer)];
    if (high_dpi_color.bounds.height
            < minimum_color_descriptor.minimum_height_dip * 2
        || high_dpi_layer.bounds.height
            < minimum_layer_descriptor.minimum_height_dip * 2) {
        return 144;
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
        || model.SetZoneExtentDip(DockZone::Left, 200) != DockResult::NoOp
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
    const auto has_zone_extent_splitter = [&normal](DockZone zone) noexcept {
        for (std::size_t index = 0U; index < normal.dock.splitter_count; ++index) {
            const DockSplitterGeometry& splitter = normal.dock.splitters[index];
            if (splitter.kind == DockSplitterKind::ZoneExtent
                && splitter.zone == zone) {
                return true;
            }
        }
        return false;
    };
    if (!tool.shown || options.shown || !color.shown || !layer.shown
        || options.bounds.width != 0 || options.bounds.height != 0
        || tool.bounds.x != 0 || tool.bounds.y != 0 || tool.bounds.width != 80
        || normal.editor.left != 80 || normal.editor.right != 876
        || has_zone_extent_splitter(DockZone::Left)
        || !has_zone_extent_splitter(DockZone::Right)
        || color.bounds.x != 880 || color.bounds.width != 320
        || layer.bounds.x != 880 || layer.bounds.width != 320
        || normal.dock.right_tool_tabs.x != 880
        || normal.dock.right_tool_tabs.y != 0
        || normal.dock.right_tool_tabs.width != 320
        || normal.dock.right_tool_tabs.height != 28
        || color.bounds.y != 28 || layer.bounds.y != 332
        || color.bounds.height != 300 || layer.bounds.height != 448
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
        || mirrored.editor.right != 1'120 || mirrored_tool.bounds.x != 1'120
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
        || narrow.editor.left != 80 || narrow.editor.right != 600) {
        return 11;
    }

    const auto high_dpi = ComputeWorkspaceLayout(1'800, 1'200, 0, 144U, state);
    const auto& high_tool =
        high_dpi.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)];
    const auto& high_options =
        high_dpi.dock.panes[static_cast<std::size_t>(DockPaneType::ToolOptions)];
    const auto& high_color =
        high_dpi.dock.panes[static_cast<std::size_t>(DockPaneType::Color)];
    if (high_options.shown || high_options.bounds.height != 0
        || high_tool.bounds.width != 120
        || high_color.bounds.width != 480 || high_dpi.editor.left != 120
        || high_dpi.editor.right != 1'314) {
        return 12;
    }

    const auto dpi_120 = ComputeWorkspaceLayout(1'500, 1'000, 0, 120U, state);
    const auto dpi_192 = ComputeWorkspaceLayout(2'400, 1'600, 0, 192U, state);
    if (dpi_120.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)]
                .bounds.width
            != 100
        || dpi_120.dock.panes[static_cast<std::size_t>(DockPaneType::ToolOptions)]
                .shown
        || dpi_192.dock.panes[static_cast<std::size_t>(DockPaneType::Tool)]
                .bounds.width
            != 160
        || dpi_192.dock.panes[static_cast<std::size_t>(DockPaneType::ToolOptions)]
                .shown) {
        return 17;
    }

    if (state.dock.RestorePane(DockPaneType::Reference) != DockResult::Ok
        || state.right_tool_tabs.AddPaneToSelected(
               DockPaneType::Reference, 0, 96U, 6)
            != ToolTabResult::Ok) {
        return 13;
    }
    const auto tabbed = ComputeWorkspaceLayout(1'200, 800, 0, 96U, state);
    if (tabbed.dock.panes[static_cast<std::size_t>(DockPaneType::Color)].shown
        || tabbed.dock.panes[static_cast<std::size_t>(DockPaneType::Layer)].shown
        || !tabbed.dock.panes[
               static_cast<std::size_t>(DockPaneType::Reference)].shown
        || tabbed.dock.right_tool_tabs.height != 28
        || tabbed.dock.splitter_count >= normal.dock.splitter_count) {
        return 14;
    }

    for (const DockPaneType type : {
             DockPaneType::Tool,
             DockPaneType::Color,
             DockPaneType::Layer,
             DockPaneType::Reference}) {
        if (state.dock.HidePane(type) != DockResult::Ok) {
            return 15;
        }
    }
    if (state.right_tool_tabs.RemovePane(DockPaneType::Color)
            != ToolTabResult::Ok
        || state.right_tool_tabs.RemovePane(DockPaneType::Layer)
            != ToolTabResult::Ok
        || state.right_tool_tabs.RemovePane(DockPaneType::Reference)
            != ToolTabResult::Ok
        || state.right_tool_tabs.Selected()
        || state.right_tool_tabs.HasVisibleTabs()) {
        return 15;
    }
    const auto canvas_only = ComputeWorkspaceLayout(1'200, 800, 0, 96U, state);
    if (Width(canvas_only.editor) != 1'200 || Height(canvas_only.editor) != 800) {
        return 16;
    }

    WorkspaceLayoutState default_workspace{};
    if (!ValidDynamicRightTabs(default_workspace)) {
        return 137;
    }
    for (const WorkspacePreset workspace_preset : {
             WorkspacePreset::Coloring,
             WorkspacePreset::LineCleanup,
             WorkspacePreset::ReferenceCheck,
             WorkspacePreset::Batch,
             WorkspacePreset::Focus}) {
        WorkspaceLayoutState preset_contract{};
        if (!ApplyWorkspacePreset(preset_contract, workspace_preset)
            || !ValidDynamicRightTabs(preset_contract)) {
            return 138;
        }
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
            != DockPaneType::Reference
        || preset.right_tool_tabs.Selected()
            != preset.right_tool_tabs.TabForPane(DockPaneType::Reference)) {
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

    WorkspaceLayoutState dynamic_tabs{};
    if (dynamic_tabs.right_tool_tabs.ReorderPane(
            DockPaneType::Layer, DockPaneType::Color, false)
            != ToolTabResult::Ok
        || dynamic_tabs.dock.RestorePane(DockPaneType::Reference)
            != DockResult::Ok
        || dynamic_tabs.dock.RestorePane(DockPaneType::Locator)
            != DockResult::Ok
        || dynamic_tabs.right_tool_tabs.AddPaneToSelected(
               DockPaneType::Reference, 0, 96U, 6)
            != ToolTabResult::Ok
        || dynamic_tabs.right_tool_tabs.AddPaneToSelected(
               DockPaneType::Locator, 10'000, 96U, 6)
            != ToolTabResult::Ok) {
        return 133;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes>
        dynamic_tab_bytes{};
    std::size_t dynamic_tab_written{};
    WorkspaceLayoutState dynamic_tab_decoded{};
    if (!EncodeWorkspaceLayout(
            dynamic_tabs, dynamic_tab_bytes, dynamic_tab_written)
        || dynamic_tab_written != sizeof(PersistedWorkspaceV9ForMigration)
        || DecodeWorkspaceLayout(
               dynamic_tab_decoded,
               std::span<const std::byte>(
                   dynamic_tab_bytes.data(), dynamic_tab_written))
            != WorkspaceLayoutDecodeResult::Current
        || dynamic_tab_decoded.right_tool_tabs.Tabs().size() != 2U
        || dynamic_tab_decoded.right_tool_tabs.Selected() != ToolTabId{2U}
        || dynamic_tab_decoded.right_tool_tabs.NextStableId() != 3U
        || dynamic_tab_decoded.right_tool_tabs.Tabs()[0].panes[0]
            != DockPaneType::Layer
        || dynamic_tab_decoded.right_tool_tabs.Tabs()[1].panes[0]
            != DockPaneType::Reference
        || dynamic_tab_decoded.right_tool_tabs.Tabs()[1].panes[1]
            != DockPaneType::Locator) {
        return 134;
    }
    PersistedWorkspaceV9ForMigration dynamic_tab_record{};
    std::memcpy(
        &dynamic_tab_record,
        dynamic_tab_bytes.data(),
        sizeof(dynamic_tab_record));
    const auto rejects_v9 = [](const PersistedWorkspaceV9ForMigration& value) {
        WorkspaceLayoutState candidate{};
        return DecodeWorkspaceLayout(
                   candidate,
                   std::span<const std::byte>(
                       reinterpret_cast<const std::byte*>(&value),
                       sizeof(value)))
                == WorkspaceLayoutDecodeResult::Invalid
            && candidate.selected_preset == WorkspacePreset::Coloring;
    };
    auto duplicate_tab_id = dynamic_tab_record;
    duplicate_tab_id.tabs[1].stable_id = duplicate_tab_id.tabs[0].stable_id;
    auto duplicate_pane = dynamic_tab_record;
    duplicate_pane.tabs[1].pane_stable_type_ids[0] =
        duplicate_pane.tabs[0].pane_stable_type_ids[0];
    auto empty_tab = dynamic_tab_record;
    empty_tab.tabs[0].pane_count = 0U;
    auto invalid_selected_tab = dynamic_tab_record;
    invalid_selected_tab.selected_tab_id = UINT32_C(0xfefefefe);
    auto invalid_next_tab_id = dynamic_tab_record;
    invalid_next_tab_id.next_tab_id = invalid_next_tab_id.tabs[1].stable_id;
    auto tab_count_overflow = dynamic_tab_record;
    tab_count_overflow.tab_count = static_cast<std::uint32_t>(kDockPaneCount + 1U);
    auto pane_count_overflow = dynamic_tab_record;
    pane_count_overflow.tabs[0].pane_count =
        static_cast<std::uint32_t>(kDockPaneCount + 1U);
    auto stable_id_overflow = dynamic_tab_record;
    stable_id_overflow.tabs[1].stable_id = UINT32_MAX;
    stable_id_overflow.next_tab_id = UINT32_MAX;
    auto dirty_unused_tab = dynamic_tab_record;
    dirty_unused_tab.tabs[dynamic_tab_record.tab_count].stable_id = 99U;
    if (!rejects_v9(duplicate_tab_id)
        || !rejects_v9(duplicate_pane)
        || !rejects_v9(empty_tab)
        || !rejects_v9(invalid_selected_tab)
        || !rejects_v9(invalid_next_tab_id)
        || !rejects_v9(tab_count_overflow)
        || !rejects_v9(pane_count_overflow)
        || !rejects_v9(stable_id_overflow)
        || !rejects_v9(dirty_unused_tab)) {
        return 135;
    }
    auto unknown_tab_pane = dynamic_tab_record;
    unknown_tab_pane.tabs[0].pane_stable_type_ids[0] = UINT32_C(0xfefefefe);
    WorkspaceLayoutState ignored_unknown_tab_pane{};
    if (DecodeWorkspaceLayout(
            ignored_unknown_tab_pane,
            std::span<const std::byte>(
                reinterpret_cast<const std::byte*>(&unknown_tab_pane),
                sizeof(unknown_tab_pane)))
            != WorkspaceLayoutDecodeResult::Current
        || !ignored_unknown_tab_pane.right_tool_tabs.TabForPane(
            DockPaneType::Layer)) {
        return 136;
    }

    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> version8_bytes{};
    std::size_t legacy_written{};
    if (!ExtractVersion8Workspace(
            std::span<const std::byte>(bytes.data(), written),
            version8_bytes,
            legacy_written)
        || legacy_written != sizeof(PersistedWorkspaceForMigration)) {
        return 129;
    }
    WorkspaceLayoutState migrated_version8{};
    if (DecodeWorkspaceLayout(
            migrated_version8,
            std::span<const std::byte>(
                version8_bytes.data(), legacy_written))
            != WorkspaceLayoutDecodeResult::Migrated
        || !migrated_version8.right_tool_tabs.HasVisibleTabs()
        || migrated_version8.right_tool_tabs.Tabs()[0].pane_count == 0U) {
        return 129;
    }
    auto version7_bytes = version8_bytes;
    PersistedWorkspaceForMigration version7_record{};
    std::memcpy(
        &version7_record, version7_bytes.data(), sizeof(version7_record));
    if (version7_record.pane_count >= version7_record.panes.size()) {
        return 129;
    }
    const auto& legacy_options_descriptor = PaneDescriptors()[
        static_cast<std::size_t>(DockPaneType::ToolOptions)];
    version7_record.panes[version7_record.pane_count++] =
        PersistedDockPaneForMigration{
            legacy_options_descriptor.stable_type_id,
            static_cast<std::uint32_t>(DockZone::TopContext),
            static_cast<std::uint32_t>(DockZone::TopContext),
            UINT32_C(0x81000000),
            1000U,
            120,
            120,
            720,
            40};
    version7_record.zones[static_cast<std::size_t>(DockZone::TopContext)]
        .active_tab = legacy_options_descriptor.stable_type_id;
    std::memcpy(
        version7_bytes.data(), &version7_record, sizeof(version7_record));
    WorkspaceLayoutState migrated_version7{};
    if (!DowngradeGroupedLayoutToV7(std::span<std::byte>(
            version7_bytes.data(), legacy_written))
        || DecodeWorkspaceLayout(
               migrated_version7,
               std::span<const std::byte>(
                   version7_bytes.data(), legacy_written))
            != WorkspaceLayoutDecodeResult::Migrated
        || migrated_version7.dock.IsPaneVisible(DockPaneType::ToolOptions)
        || migrated_version7.dock.Zone(DockZone::TopContext)->active_tab
            != DockPaneType::Count) {
        return 129;
    }

    WorkspaceLayoutState mixed_serialized{};
    if (mixed_serialized.dock.RestorePane(DockPaneType::LightTable)
            != DockResult::Ok
        || mixed_serialized.dock.RestorePane(DockPaneType::Reference)
            != DockResult::Ok
        || mixed_serialized.right_tool_tabs.EnsurePaneAssigned(
               DockPaneType::LightTable)
            != ToolTabResult::Ok
        || mixed_serialized.right_tool_tabs.EnsurePaneAssigned(
               DockPaneType::Reference)
            != ToolTabResult::Ok
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
            != DockResult::Ok
        || legacy_reference_split.right_tool_tabs.EnsurePaneAssigned(
               DockPaneType::Reference)
            != ToolTabResult::Ok) {
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
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes>
        legacy_reference_v8_bytes{};
    std::size_t legacy_reference_v8_written{};
    if (!EncodeWorkspaceLayout(
            legacy_reference_split,
            legacy_reference_bytes,
            legacy_reference_written)
        || !ExtractVersion8Workspace(
            std::span<const std::byte>(
                legacy_reference_bytes.data(), legacy_reference_written),
            legacy_reference_v8_bytes,
            legacy_reference_v8_written)
        || !DowngradeGroupedLayoutToV6(std::span<std::byte>(
            legacy_reference_v8_bytes.data(),
            legacy_reference_v8_written))) {
        return 60;
    }
    WorkspaceLayoutState migrated_reference_stack{};
    if (DecodeWorkspaceLayout(
            migrated_reference_stack,
            std::span<const std::byte>(
                legacy_reference_v8_bytes.data(),
                legacy_reference_v8_written))
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
            != DockResult::Ok
        || legacy_explicit_reference_split.right_tool_tabs.EnsurePaneAssigned(
               DockPaneType::Reference)
            != ToolTabResult::Ok) {
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
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes>
        explicit_reference_v8_bytes{};
    std::size_t explicit_reference_v8_written{};
    WorkspaceLayoutState preserved_reference_split{};
    if (!EncodeWorkspaceLayout(
            legacy_explicit_reference_split,
            explicit_reference_bytes,
            explicit_reference_written)
        || !ExtractVersion8Workspace(
            std::span<const std::byte>(
                explicit_reference_bytes.data(), explicit_reference_written),
            explicit_reference_v8_bytes,
            explicit_reference_v8_written)
        || !DowngradeGroupedLayoutToV6(std::span<std::byte>(
            explicit_reference_v8_bytes.data(),
            explicit_reference_v8_written))
        || DecodeWorkspaceLayout(
               preserved_reference_split,
               std::span<const std::byte>(
                   explicit_reference_v8_bytes.data(),
                   explicit_reference_v8_written))
            != WorkspaceLayoutDecodeResult::Migrated
        || preserved_reference_split.dock.StackCount(DockZone::Right) != 3U
        || preserved_reference_split.dock.Pane(DockPaneType::Reference)->stack
            == preserved_reference_split.dock.Pane(DockPaneType::Layer)->stack) {
        return 64;
    }

    WorkspaceLayoutState legacy_light_table_tabs{};
    if (legacy_light_table_tabs.dock.RestorePane(DockPaneType::LightTable)
            != DockResult::Ok
        || legacy_light_table_tabs.right_tool_tabs.EnsurePaneAssigned(
               DockPaneType::LightTable)
            != ToolTabResult::Ok
        || legacy_light_table_tabs.dock.SetZoneMode(
               DockZone::Right, DockStackMode::Tabs)
            != DockResult::Ok) {
        return 55;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> legacy_tabs_bytes{};
    std::size_t legacy_tabs_written{};
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes>
        legacy_tabs_v8_bytes{};
    std::size_t legacy_tabs_v8_written{};
    if (!EncodeWorkspaceLayout(
            legacy_light_table_tabs, legacy_tabs_bytes, legacy_tabs_written)
        || !ExtractVersion8Workspace(
            std::span<const std::byte>(
                legacy_tabs_bytes.data(), legacy_tabs_written),
            legacy_tabs_v8_bytes,
            legacy_tabs_v8_written)
        || !DowngradeGroupedLayoutToV5(
            std::span<std::byte>(
                legacy_tabs_v8_bytes.data(), legacy_tabs_v8_written),
            legacy_light_table_tabs.dock.ToRecord())) {
        return 56;
    }
    WorkspaceLayoutState migrated_light_table_tabs{};
    if (DecodeWorkspaceLayout(
            migrated_light_table_tabs,
            std::span<const std::byte>(
                legacy_tabs_v8_bytes.data(), legacy_tabs_v8_written))
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

    auto legacy_v5_bytes = version8_bytes;
    const auto serialized_record = serialized.dock.ToRecord();
    if (!DowngradeGroupedLayoutToV5(
            std::span<std::byte>(legacy_v5_bytes.data(), legacy_written),
            serialized_record)) {
        return 45;
    }
    WorkspaceLayoutState migrated_v5{};
    if (DecodeWorkspaceLayout(
            migrated_v5,
            std::span<const std::byte>(
                legacy_v5_bytes.data(), legacy_written))
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
        || !migrated.dock.IsPaneVisible(DockPaneType::Tool)
        || migrated.dock.IsPaneVisible(DockPaneType::ToolOptions)) {
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

    return 0;
}
