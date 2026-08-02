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
using inkpod::windows::ui::DecodeWorkspaceLayout;
using inkpod::windows::ui::DeleteWorkspaceLayout;
using inkpod::windows::ui::DockFloatingPlacement;
using inkpod::windows::ui::DockLayoutModel;
using inkpod::windows::ui::DockPaneType;
using inkpod::windows::ui::DockResult;
using inkpod::windows::ui::DockStackMode;
using inkpod::windows::ui::DockZone;
using inkpod::windows::ui::PaneDescriptors;
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
    std::uint32_t pane_count{static_cast<std::uint32_t>(kDockPaneCount)};
    std::uint32_t zone_count{static_cast<std::uint32_t>(kDockedZoneCount)};
    std::uint32_t layer_split_milli{550U};
    std::uint32_t reserved{};
    std::array<LegacyDockPaneV3, kDockPaneCount> panes{};
    std::array<LegacyDockZoneV3, kDockedZoneCount> zones{};
};

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
            == false) {
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
        || !decoded.window.valid || decoded.window.show_command != SW_SHOWMAXIMIZED) {
        return 27;
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
    const auto legacy_record = serialized.dock.ToRecord();
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
        legacy_v3.zones[index] = LegacyDockZoneV3{
            static_cast<std::uint32_t>(source.mode),
            static_cast<std::uint32_t>(source.active_tab),
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
