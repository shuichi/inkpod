#include "workspace_layout.h"

#include <shellscalingapi.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kSettingsKey[] = L"Software\\Inkpod";
constexpr wchar_t kWorkspaceWindowCountValue[] = L"WorkspaceWindowCountV1";
constexpr std::uint32_t kMagic = UINT32_C(0x4c574b49);
constexpr std::uint32_t kVersion = 7U;
constexpr std::uint32_t kGroupedPaneMarker = UINT32_C(0x80000000);
constexpr std::uint32_t kGroupedPaneReservedMask = UINT32_C(0x7e000000);
constexpr int kReferenceDpi = 96;
constexpr int kTabsHeightDip = 28;
constexpr std::size_t kMaximumPersistedDockPanes = 16U;
constexpr std::size_t kMaximumPersistedAuxiliaryPanes = 16U;
constexpr std::size_t kLegacyDockPaneCount = 4U;

constexpr std::uint32_t kLegacyToolVisible = UINT32_C(1) << 0U;
constexpr std::uint32_t kLegacyToolOptionsVisible = UINT32_C(1) << 1U;
constexpr std::uint32_t kLegacyColorVisible = UINT32_C(1) << 2U;
constexpr std::uint32_t kLegacyLayerVisible = UINT32_C(1) << 3U;
constexpr std::uint32_t kLegacyMirrored = UINT32_C(1) << 4U;
constexpr std::uint32_t kLegacyKnownFlags = kLegacyToolVisible
    | kLegacyToolOptionsVisible | kLegacyColorVisible | kLegacyLayerVisible
    | kLegacyMirrored;

struct LegacyPersistedWorkspaceLayoutV2 {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t struct_size;
    std::uint32_t flags;
    std::int32_t tool_width_dip;
    std::int32_t inspector_width_dip;
    std::int32_t tool_options_height_dip;
    std::uint32_t color_split_milli;
    std::uint32_t layer_split_milli;
};

struct PersistedDockPaneV3 {
    std::uint32_t type;
    std::uint32_t zone;
    std::uint32_t restore_zone;
    std::uint32_t order;
    std::uint32_t split_weight;
    std::int32_t floating_x_dip;
    std::int32_t floating_y_dip;
    std::int32_t floating_width_dip;
    std::int32_t floating_height_dip;
};

struct PersistedDockZone {
    std::uint32_t mode;
    std::uint32_t active_tab;
    std::int32_t extent_dip;
};

struct PersistedWorkspaceLayoutV3 {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t struct_size;
    std::uint32_t flags;
    std::uint32_t pane_count;
    std::uint32_t zone_count;
    std::uint32_t layer_split_milli;
    std::uint32_t reserved;
    std::array<PersistedDockPaneV3, kLegacyDockPaneCount> panes;
    std::array<PersistedDockZone, kDockedZoneCount> zones;
};

struct PersistedDockPaneV4 {
    std::uint32_t stable_type_id;
    std::uint32_t zone;
    std::uint32_t restore_zone;
    std::uint32_t order;
    std::uint32_t split_weight;
    std::int32_t floating_x_dip;
    std::int32_t floating_y_dip;
    std::int32_t floating_width_dip;
    std::int32_t floating_height_dip;
};

struct PersistedScreenPlacement {
    std::int32_t x_px;
    std::int32_t y_px;
    std::int32_t width_px;
    std::int32_t height_px;
    std::uint32_t capture_dpi;
    std::uint32_t valid;
};

struct PersistedAuxiliaryPaneV4 {
    std::uint32_t stable_type_id;
    std::uint32_t flags;
    std::uint32_t edge;
    std::uint32_t reserved;
    PersistedScreenPlacement floating;
};

struct PersistedWorkspaceLayoutV4 {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t struct_size;
    std::uint32_t flags;
    std::uint32_t pane_count;
    std::uint32_t zone_count;
    std::uint32_t auxiliary_count;
    std::uint32_t selected_preset;
    std::uint32_t density;
    std::uint32_t split_orientation;
    std::uint32_t split_ratio_milli;
    std::uint32_t layer_split_milli;
    std::uint32_t reserved0;
    std::uint32_t window_show_command;
    PersistedScreenPlacement window;
    std::array<wchar_t, kWorkspacePresetNameCapacity> custom_name;
    std::array<PersistedDockPaneV4, kMaximumPersistedDockPanes> panes;
    std::array<PersistedDockZone, kDockedZoneCount> zones;
    std::array<PersistedAuxiliaryPaneV4, kMaximumPersistedAuxiliaryPanes>
        auxiliary;
};

static_assert(
    sizeof(PersistedWorkspaceLayoutV4) <= kMaximumWorkspaceLayoutRecordBytes);

std::uint32_t EncodeGroupedPaneOrder(
    const DockPanePlacement& pane) noexcept {
    return kGroupedPaneMarker
        | (pane.active_tab ? UINT32_C(1) << 24U : 0U)
        | (static_cast<std::uint32_t>(pane.tab_order) << 16U)
        | (static_cast<std::uint32_t>(pane.stack) << 8U)
        | pane.order;
}

bool DecodeGroupedPaneOrder(
    std::uint32_t encoded,
    DockPanePlacement& pane) noexcept {
    if ((encoded & kGroupedPaneMarker) == 0U
        || (encoded & kGroupedPaneReservedMask) != 0U) {
        return false;
    }
    pane.order = static_cast<std::uint8_t>(encoded & UINT32_C(0xff));
    pane.stack = static_cast<std::uint8_t>((encoded >> 8U) & UINT32_C(0xff));
    pane.tab_order = static_cast<std::uint8_t>(
        (encoded >> 16U) & UINT32_C(0xff));
    pane.active_tab = ((encoded >> 24U) & UINT32_C(1)) != 0U;
    return true;
}

RECT ToRect(const DockRect& value) noexcept {
    return RECT{
        value.x,
        value.y,
        value.x + std::max(0, value.width),
        value.y + std::max(0, value.height)};
}

std::size_t PaneIndex(DockPaneType type) noexcept {
    return static_cast<std::size_t>(type);
}

std::size_t AuxiliaryIndex(WorkspaceAuxiliaryPane type) noexcept {
    return static_cast<std::size_t>(type);
}

DockPaneType AuxiliaryDockPaneType(WorkspaceAuxiliaryPane type) noexcept {
    switch (type) {
        case WorkspaceAuxiliaryPane::Locator: return DockPaneType::Locator;
        case WorkspaceAuxiliaryPane::Sequence: return DockPaneType::Sequence;
        case WorkspaceAuxiliaryPane::LightTable: return DockPaneType::LightTable;
        case WorkspaceAuxiliaryPane::Reference: return DockPaneType::Reference;
        case WorkspaceAuxiliaryPane::Batch: return DockPaneType::Batch;
        case WorkspaceAuxiliaryPane::Count: return DockPaneType::Count;
    }
    return DockPaneType::Count;
}

const PaneDescriptor* FindPaneDescriptorByStableId(
    std::uint32_t stable_type_id) noexcept {
    for (const PaneDescriptor& descriptor : PaneDescriptors()) {
        if (descriptor.stable_type_id == stable_type_id) {
            return &descriptor;
        }
    }
    return nullptr;
}

WorkspaceAuxiliaryPaneState* FindAuxiliaryByStableId(
    WorkspaceLayoutState& state, std::uint32_t stable_type_id) noexcept {
    const auto found = std::find_if(
        state.auxiliary.begin(),
        state.auxiliary.end(),
        [stable_type_id](const WorkspaceAuxiliaryPaneState& pane) {
            return pane.stable_type_id == stable_type_id;
        });
    return found == state.auxiliary.end() ? nullptr : &*found;
}

bool ValidScreenPlacement(const WorkspaceScreenPlacement& value) noexcept {
    constexpr int kCoordinateLimit = 2'000'000;
    constexpr int kSizeLimit = 65'536;
    if (!value.valid) {
        return true;
    }
    return value.x_px >= -kCoordinateLimit && value.x_px <= kCoordinateLimit
        && value.y_px >= -kCoordinateLimit && value.y_px <= kCoordinateLimit
        && value.width_px >= 64 && value.width_px <= kSizeLimit
        && value.height_px >= 48 && value.height_px <= kSizeLimit
        && value.capture_dpi >= 48U && value.capture_dpi <= 960U;
}

PersistedScreenPlacement EncodeScreenPlacement(
    const WorkspaceScreenPlacement& value) noexcept {
    return PersistedScreenPlacement{
        value.x_px,
        value.y_px,
        value.width_px,
        value.height_px,
        value.capture_dpi,
        value.valid ? 1U : 0U};
}

bool DecodeScreenPlacement(
    WorkspaceScreenPlacement& output,
    const PersistedScreenPlacement& value) noexcept {
    if (value.valid > 1U) {
        return false;
    }
    const WorkspaceScreenPlacement candidate{
        value.x_px,
        value.y_px,
        value.width_px,
        value.height_px,
        value.capture_dpi,
        value.valid != 0U};
    if (!ValidScreenPlacement(candidate)) {
        return false;
    }
    output = candidate;
    return true;
}

bool ValidLegacyLayout(const LegacyPersistedWorkspaceLayoutV2& value) noexcept {
    return value.magic == kMagic && value.version == 2U
        && value.struct_size == sizeof(value)
        && (value.flags & ~kLegacyKnownFlags) == 0U
        && value.tool_width_dip >= 80 && value.tool_width_dip <= 160
        && value.inspector_width_dip >= 240 && value.inspector_width_dip <= 640
        && value.tool_options_height_dip >= 28
        && value.tool_options_height_dip <= 96
        && value.color_split_milli >= 150U && value.color_split_milli <= 700U
        && value.layer_split_milli >= 200U
        && value.layer_split_milli <= 800U;
}

bool LoadLegacyLayout(
    WorkspaceLayoutState& state,
    const LegacyPersistedWorkspaceLayoutV2& value) noexcept {
    if (!ValidLegacyLayout(value)) {
        return false;
    }
    WorkspaceLayoutState candidate{};
    static_cast<void>(candidate.dock.SetZoneExtentDip(
        DockZone::Left, value.tool_width_dip));
    static_cast<void>(candidate.dock.SetZoneExtentDip(
        DockZone::Right, value.inspector_width_dip));
    static_cast<void>(candidate.dock.SetZoneExtentDip(
        DockZone::TopContext, value.tool_options_height_dip));
    DockPanePlacement* color = candidate.dock.Pane(DockPaneType::Color);
    DockPanePlacement* layer = candidate.dock.Pane(DockPaneType::Layer);
    if (color == nullptr || layer == nullptr) {
        return false;
    }
    color->split_weight = value.color_split_milli;
    layer->split_weight = 1000U - value.color_split_milli;
    if ((value.flags & kLegacyToolVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::Tool));
    }
    if ((value.flags & kLegacyToolOptionsVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::ToolOptions));
    }
    if ((value.flags & kLegacyColorVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::Color));
    }
    if ((value.flags & kLegacyLayerVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::Layer));
    }
    candidate.dock.SetMirrored((value.flags & kLegacyMirrored) != 0U);
    candidate.layer_split_milli = value.layer_split_milli;
    state = candidate;
    return true;
}

void NormalizeRecordOrders(DockLayoutRecord& record) noexcept;

bool DecodeVersion3(
    WorkspaceLayoutState& state,
    const PersistedWorkspaceLayoutV3& value) noexcept {
    if (value.magic != kMagic || value.version != 3U
        || value.struct_size != sizeof(value) || value.flags > 1U
        || value.pane_count != kLegacyDockPaneCount
        || value.zone_count != kDockedZoneCount
        || value.layer_split_milli < 200U
        || value.layer_split_milli > 800U || value.reserved != 0U) {
        return false;
    }
    WorkspaceLayoutState candidate{};
    DockLayoutRecord record = candidate.dock.ToRecord();
    record.mirrored = value.flags;
    for (std::size_t index = 0U; index < value.panes.size(); ++index) {
        const PersistedDockPaneV3& source = value.panes[index];
        DockPanePlacement& pane = record.panes[index];
        pane.type = static_cast<DockPaneType>(source.type);
        pane.zone = static_cast<DockZone>(source.zone);
        pane.restore_zone = static_cast<DockZone>(source.restore_zone);
        pane.order = static_cast<std::uint8_t>(source.order);
        pane.split_weight = source.split_weight;
        pane.floating = DockFloatingPlacement{
            source.floating_x_dip,
            source.floating_y_dip,
            source.floating_width_dip,
            source.floating_height_dip};
        pane.present = true;
    }
    for (std::size_t index = 0U; index < value.zones.size(); ++index) {
        const PersistedDockZone& source = value.zones[index];
        record.zones[index] = DockZoneState{
            static_cast<DockStackMode>(source.mode),
            static_cast<DockPaneType>(source.active_tab),
            source.extent_dip};
    }
    NormalizeRecordOrders(record);
    if (!candidate.dock.LoadRecord(record)) {
        return false;
    }
    candidate.layer_split_milli = value.layer_split_milli;
    state = candidate;
    return true;
}

void NormalizeRecordOrders(DockLayoutRecord& record) noexcept {
    for (std::size_t zone_index = 0U; zone_index < kDockedZoneCount;
         ++zone_index) {
        const DockZone zone = static_cast<DockZone>(zone_index);
        std::array<DockPanePlacement*, kDockPaneCount> panes{};
        std::size_t count{};
        for (DockPanePlacement& pane : record.panes) {
            if (pane.zone == zone) {
                panes[count++] = &pane;
            }
        }
        std::sort(
            panes.begin(),
            panes.begin() + static_cast<std::ptrdiff_t>(count),
            [](const DockPanePlacement* left, const DockPanePlacement* right) {
                if (left->order != right->order) {
                    return left->order < right->order;
                }
                return PaneIndex(left->type) < PaneIndex(right->type);
            });
        DockZoneState& zone_state = record.zones[zone_index];
        const bool active_present = std::any_of(
            panes.begin(),
            panes.begin() + static_cast<std::ptrdiff_t>(count),
            [&zone_state](const DockPanePlacement* pane) {
                return pane->type == zone_state.active_tab;
            });
        if (!active_present) {
            zone_state.active_tab = count == 0U
                ? DockPaneType::Count
                : panes[0]->type;
        }
        if (zone_state.mode == DockStackMode::Tabs && count > 1U) {
            const std::uint32_t weight = panes[0]->split_weight;
            for (std::size_t index = 0U; index < count; ++index) {
                panes[index]->order = 0U;
                panes[index]->stack = 0U;
                panes[index]->tab_order = static_cast<std::uint8_t>(index);
                panes[index]->split_weight = weight;
                panes[index]->active_tab = panes[index]->type
                    == zone_state.active_tab;
            }
        } else {
            zone_state.mode = DockStackMode::Split;
            for (std::size_t index = 0U; index < count; ++index) {
                panes[index]->order = static_cast<std::uint8_t>(index);
                panes[index]->stack = static_cast<std::uint8_t>(index);
                panes[index]->tab_order = 0U;
                panes[index]->active_tab = true;
            }
        }
    }
}

bool DecodeVersion4Or5(
    WorkspaceLayoutState& state,
    const PersistedWorkspaceLayoutV4& value,
    bool migrate_auxiliary,
    bool grouped) noexcept {
    if (value.magic != kMagic
        || (grouped ? (value.version != kVersion && value.version != 6U)
                    : (value.version != 4U && value.version != 5U))
        || value.struct_size != sizeof(value) || value.flags > 1U
        || value.pane_count > value.panes.size()
        || value.zone_count != kDockedZoneCount
        || value.auxiliary_count > value.auxiliary.size()
        || value.selected_preset
            >= static_cast<std::uint32_t>(WorkspacePreset::Count)
        || value.density > static_cast<std::uint32_t>(WorkspaceDensity::Compact)
        || value.split_orientation
            > static_cast<std::uint32_t>(WorkspaceSplitOrientation::Horizontal)
        || value.split_ratio_milli < 200U || value.split_ratio_milli > 800U
        || value.layer_split_milli < 200U
        || value.layer_split_milli > 800U || value.reserved0 != 0U
        || (value.window_show_command != SW_SHOWNORMAL
            && value.window_show_command != SW_SHOWMAXIMIZED)) {
        return false;
    }
    const auto terminator = std::find(
        value.custom_name.begin(), value.custom_name.end(), L'\0');
    if (terminator == value.custom_name.end()) {
        return false;
    }

    WorkspaceLayoutState candidate{};
    DockLayoutRecord record = candidate.dock.ToRecord();
    record.mirrored = value.flags;
    std::array<bool, kDockPaneCount> seen_panes{};
    std::array<std::array<bool, kDockPaneCount>, kDockedZoneCount> seen_orders{};
    for (std::size_t index = 0U; index < value.pane_count; ++index) {
        const PersistedDockPaneV4& source = value.panes[index];
        const PaneDescriptor* descriptor = FindPaneDescriptorByStableId(
            source.stable_type_id);
        if (descriptor == nullptr) {
            continue;
        }
        const std::size_t type_index = PaneIndex(descriptor->type);
        const DockZone zone = static_cast<DockZone>(source.zone);
        DockPanePlacement pane = record.panes[type_index];
        const bool grouped_order_valid = !grouped
            || DecodeGroupedPaneOrder(source.order, pane);
        const bool legacy_order_valid = grouped || source.order < kDockPaneCount;
        if (type_index >= seen_panes.size() || seen_panes[type_index]
            || !grouped_order_valid || !legacy_order_valid
            || (!grouped && IsDockedZone(zone)
                && seen_orders[static_cast<std::size_t>(zone)][source.order])) {
            return false;
        }
        seen_panes[type_index] = true;
        if (!grouped && IsDockedZone(zone)) {
            seen_orders[static_cast<std::size_t>(zone)][source.order] = true;
        }
        pane.type = descriptor->type;
        pane.zone = zone;
        pane.restore_zone = static_cast<DockZone>(source.restore_zone);
        if (!grouped) {
            pane.order = static_cast<std::uint8_t>(source.order);
        }
        pane.split_weight = source.split_weight;
        pane.floating = DockFloatingPlacement{
            source.floating_x_dip,
            source.floating_y_dip,
            source.floating_width_dip,
            source.floating_height_dip};
        pane.present = true;
        record.panes[type_index] = pane;
    }
    for (std::size_t index = 0U; index < kDockedZoneCount; ++index) {
        const PersistedDockZone& source = value.zones[index];
        DockPaneType active = DockPaneType::Count;
        if (const PaneDescriptor* descriptor = FindPaneDescriptorByStableId(
                source.active_tab);
            descriptor != nullptr) {
            active = descriptor->type;
        }
        record.zones[index] = DockZoneState{
            static_cast<DockStackMode>(source.mode), active, source.extent_dip};
    }
    if (!grouped) {
        const std::size_t right_index = static_cast<std::size_t>(DockZone::Right);
        const DockPanePlacement& legacy_color = record.panes[PaneIndex(
            DockPaneType::Color)];
        const DockPanePlacement& legacy_layer = record.panes[PaneIndex(
            DockPaneType::Layer)];
        const DockPanePlacement& legacy_light_table = record.panes[PaneIndex(
            DockPaneType::LightTable)];
        const std::size_t right_pane_count = static_cast<std::size_t>(
            std::count_if(
                record.panes.begin(),
                record.panes.end(),
                [](const DockPanePlacement& pane) {
                    return pane.present && pane.zone == DockZone::Right;
                }));
        const DockPaneType legacy_active = record.zones[right_index].active_tab;
        const bool migrate_legacy_light_table_tabs = value.version == 5U
            && right_pane_count == 3U
            && record.zones[right_index].mode == DockStackMode::Tabs
            && legacy_color.zone == DockZone::Right
            && legacy_layer.zone == DockZone::Right
            && legacy_light_table.zone == DockZone::Right
            && (legacy_active == DockPaneType::Color
                || legacy_active == DockPaneType::Layer
                || legacy_active == DockPaneType::LightTable);
        const std::uint32_t legacy_color_weight = legacy_color.split_weight;
        const std::uint32_t legacy_layer_weight = legacy_layer.split_weight;
        NormalizeRecordOrders(record);
        if (migrate_legacy_light_table_tabs) {
            DockPanePlacement& color = record.panes[PaneIndex(DockPaneType::Color)];
            DockPanePlacement& layer = record.panes[PaneIndex(DockPaneType::Layer)];
            DockPanePlacement& light_table = record.panes[PaneIndex(
                DockPaneType::LightTable)];
            color.order = 0U;
            color.stack = 0U;
            color.tab_order = 0U;
            color.split_weight = legacy_color_weight;
            color.active_tab = true;
            layer.order = 1U;
            layer.stack = 1U;
            layer.tab_order = 0U;
            layer.split_weight = legacy_layer_weight;
            layer.active_tab = legacy_active != DockPaneType::LightTable;
            light_table.order = 1U;
            light_table.stack = 1U;
            light_table.tab_order = 1U;
            light_table.split_weight = legacy_layer_weight;
            light_table.active_tab = legacy_active == DockPaneType::LightTable;
            record.zones[right_index].mode = DockStackMode::Mixed;
            record.zones[right_index].active_tab = legacy_active;
        }
    } else {
        for (std::size_t index = 0U; index < kDockedZoneCount; ++index) {
            const DockZone zone = static_cast<DockZone>(index);
            DockZoneState& zone_state = record.zones[index];
            const bool active_present = std::any_of(
                record.panes.begin(),
                record.panes.end(),
                [zone, &zone_state](const DockPanePlacement& pane) {
                    return pane.present && pane.zone == zone
                        && pane.type == zone_state.active_tab;
                });
            if (active_present) {
                continue;
            }
            const auto replacement = std::find_if(
                record.panes.begin(),
                record.panes.end(),
                [zone](const DockPanePlacement& pane) {
                    return pane.present && pane.zone == zone && pane.active_tab;
                });
            zone_state.active_tab = replacement == record.panes.end()
                ? DockPaneType::Count
                : replacement->type;
        }
    }
    const DockPanePlacement& reference = record.panes[PaneIndex(
        DockPaneType::Reference)];
    const DockPanePlacement& layer = record.panes[PaneIndex(
        DockPaneType::Layer)];
    const bool migrate_version6_reference_stack = grouped
        && value.version == 6U && reference.present
        && reference.zone == DockZone::Right
        && reference.restore_zone == DockZone::Right
        && reference.stack
            == static_cast<std::uint8_t>(PaneIndex(DockPaneType::Reference))
        && layer.present && layer.zone == DockZone::Right
        && reference.stack != layer.stack;
    if (!candidate.dock.LoadRecord(record)) {
        return false;
    }
    if (migrate_version6_reference_stack
        && candidate.dock.TabPane(DockPaneType::Reference, DockPaneType::Layer)
            != DockResult::Ok) {
        return false;
    }

    candidate.layer_split_milli = value.layer_split_milli;
    candidate.selected_preset = static_cast<WorkspacePreset>(
        value.selected_preset);
    candidate.density = static_cast<WorkspaceDensity>(value.density);
    candidate.split_orientation = static_cast<WorkspaceSplitOrientation>(
        value.split_orientation);
    candidate.split_ratio_milli = value.split_ratio_milli;
    WorkspaceScreenPlacement window{};
    if (!DecodeScreenPlacement(window, value.window)) {
        return false;
    }
    static_cast<WorkspaceScreenPlacement&>(candidate.window) = window;
    candidate.window.show_command = value.window_show_command;
    std::copy(
        value.custom_name.begin(),
        value.custom_name.end(),
        candidate.custom_name.begin());

    std::array<bool, kWorkspaceAuxiliaryPaneCount> seen_auxiliary{};
    for (std::size_t index = 0U; index < value.auxiliary_count; ++index) {
        const PersistedAuxiliaryPaneV4& source = value.auxiliary[index];
        WorkspaceAuxiliaryPaneState* destination = FindAuxiliaryByStableId(
            candidate, source.stable_type_id);
        if (destination == nullptr) {
            continue;
        }
        const std::size_t type_index = AuxiliaryIndex(destination->type);
        if (type_index >= seen_auxiliary.size() || seen_auxiliary[type_index]
            || (source.flags & ~UINT32_C(3)) != 0U
            || source.edge
                > static_cast<std::uint32_t>(WorkspaceAutoHideEdge::Bottom)
            || source.reserved != 0U) {
            return false;
        }
        seen_auxiliary[type_index] = true;
        WorkspaceScreenPlacement floating{};
        if (!DecodeScreenPlacement(floating, source.floating)) {
            return false;
        }
        destination->visible = (source.flags & UINT32_C(1)) != 0U;
        destination->auto_hide = (source.flags & UINT32_C(2)) != 0U;
        destination->edge = static_cast<WorkspaceAutoHideEdge>(source.edge);
        destination->floating = floating;
    }
    if (migrate_auxiliary) {
        for (const WorkspaceAuxiliaryPaneState& pane : candidate.auxiliary) {
            const DockPaneType type = AuxiliaryDockPaneType(pane.type);
            if (type == DockPaneType::Count) {
                continue;
            }
            if (pane.auto_hide) {
                static_cast<void>(candidate.dock.SetPaneAutoHide(type, true));
            } else if (pane.visible) {
                if (pane.floating.valid) {
                    const UINT source_dpi = pane.floating.capture_dpi == 0U
                        ? 96U
                        : pane.floating.capture_dpi;
                    static_cast<void>(candidate.dock.FloatPane(
                        type,
                        DockFloatingPlacement{
                            MulDiv(pane.floating.x_px, 96, source_dpi),
                            MulDiv(pane.floating.y_px, 96, source_dpi),
                            MulDiv(pane.floating.width_px, 96, source_dpi),
                            MulDiv(pane.floating.height_px, 96, source_dpi)}));
                } else {
                    static_cast<void>(candidate.dock.RestorePane(type));
                }
            } else {
                static_cast<void>(candidate.dock.HidePane(type));
            }
        }
    }
    state = candidate;
    return true;
}

PersistedWorkspaceLayoutV4 EncodeCurrent(
    const WorkspaceLayoutState& state) noexcept {
    PersistedWorkspaceLayoutV4 value{};
    value.magic = kMagic;
    value.version = kVersion;
    value.struct_size = sizeof(value);
    value.flags = state.dock.Mirrored() ? 1U : 0U;
    value.pane_count = static_cast<std::uint32_t>(std::count_if(
        PaneDescriptors().begin(),
        PaneDescriptors().end(),
        [](const PaneDescriptor& descriptor) { return descriptor.persist_layout; }));
    value.zone_count = static_cast<std::uint32_t>(kDockedZoneCount);
    value.auxiliary_count = static_cast<std::uint32_t>(
        kWorkspaceAuxiliaryPaneCount);
    value.selected_preset = static_cast<std::uint32_t>(state.selected_preset);
    value.density = static_cast<std::uint32_t>(state.density);
    value.split_orientation = static_cast<std::uint32_t>(
        state.split_orientation);
    value.split_ratio_milli = state.split_ratio_milli;
    value.layer_split_milli = state.layer_split_milli;
    value.window_show_command = state.window.show_command;
    value.window = EncodeScreenPlacement(state.window);
    std::copy(
        state.custom_name.begin(),
        state.custom_name.end(),
        value.custom_name.begin());

    const DockLayoutRecord record = state.dock.ToRecord();
    std::size_t persisted_index{};
    for (std::size_t index = 0U; index < kDockPaneCount; ++index) {
        const DockPanePlacement& source = record.panes[index];
        const PaneDescriptor* descriptor = FindPaneDescriptor(source.type);
        if (descriptor == nullptr || !descriptor->persist_layout) {
            continue;
        }
        value.panes[persisted_index++] = PersistedDockPaneV4{
            descriptor == nullptr ? 0U : descriptor->stable_type_id,
            static_cast<std::uint32_t>(source.zone),
            static_cast<std::uint32_t>(source.restore_zone),
            EncodeGroupedPaneOrder(source),
            source.split_weight,
            source.floating.x_dip,
            source.floating.y_dip,
            source.floating.width_dip,
            source.floating.height_dip};
    }
    for (std::size_t index = 0U; index < kDockedZoneCount; ++index) {
        const DockZoneState& source = record.zones[index];
        const PaneDescriptor* active = FindPaneDescriptor(source.active_tab);
        value.zones[index] = PersistedDockZone{
            static_cast<std::uint32_t>(source.mode),
            active == nullptr ? 0U : active->stable_type_id,
            source.extent_dip};
    }
    for (std::size_t index = 0U; index < state.auxiliary.size(); ++index) {
        const WorkspaceAuxiliaryPaneState& source = state.auxiliary[index];
        const DockPanePlacement* placement = state.dock.Pane(
            AuxiliaryDockPaneType(source.type));
        value.auxiliary[index] = PersistedAuxiliaryPaneV4{
            source.stable_type_id,
            (placement != nullptr && placement->zone != DockZone::Hidden
                    ? UINT32_C(1)
                    : 0U)
                | (placement != nullptr && placement->zone == DockZone::AutoHide
                       ? UINT32_C(2)
                       : 0U),
            static_cast<std::uint32_t>(source.edge),
            0U,
            EncodeScreenPlacement(source.floating)};
    }
    return value;
}

void SetAuxiliaryPreset(
    WorkspaceLayoutState& state,
    WorkspaceAuxiliaryPane type,
    bool visible,
    bool auto_hide,
    WorkspaceAutoHideEdge edge = WorkspaceAutoHideEdge::Right) noexcept {
    if (WorkspaceAuxiliaryPaneState* pane =
            FindWorkspaceAuxiliaryPane(state, type);
        pane != nullptr) {
        pane->visible = visible;
        pane->auto_hide = auto_hide;
        pane->edge = edge;
        const DockPaneType dock_type = AuxiliaryDockPaneType(type);
        if (auto_hide) {
            static_cast<void>(state.dock.SetPaneAutoHide(dock_type, true));
        } else if (visible) {
            static_cast<void>(state.dock.SetPaneAutoHide(dock_type, false));
            static_cast<void>(state.dock.RestorePane(dock_type));
        } else {
            static_cast<void>(state.dock.HidePane(dock_type));
        }
    }
}

int IntersectionArea(const RECT& left, const RECT& right) noexcept {
    const int width = std::max(
        0L, std::min(left.right, right.right) - std::max(left.left, right.left));
    const int height = std::max(
        0L, std::min(left.bottom, right.bottom) - std::max(left.top, right.top));
    if (width == 0 || height == 0
        || width > std::numeric_limits<int>::max() / height) {
        return 0;
    }
    return width * height;
}

struct MonitorCollection {
    std::array<WorkspaceWorkArea, 16U> work_areas{};
    std::size_t count{};
    UINT fallback_dpi{96U};
};

thread_local MonitorCollection* g_monitor_collection{};

BOOL CALLBACK CollectMonitor(
    HMONITOR monitor, HDC, LPRECT, LPARAM) noexcept {
    MonitorCollection* collection = g_monitor_collection;
    if (collection == nullptr
        || collection->count >= collection->work_areas.size()) {
        return FALSE;
    }
    MONITORINFO info{sizeof(info)};
    if (GetMonitorInfoW(monitor, &info) == FALSE) {
        return TRUE;
    }
    UINT dpi_x = collection->fallback_dpi;
    UINT dpi_y = collection->fallback_dpi;
    if (FAILED(GetDpiForMonitor(
            monitor, MDT_EFFECTIVE_DPI, &dpi_x, &dpi_y))
        || dpi_x == 0U) {
        dpi_x = collection->fallback_dpi;
    }
    collection->work_areas[collection->count++] = WorkspaceWorkArea{
        info.rcWork,
        dpi_x,
        (info.dwFlags & MONITORINFOF_PRIMARY) != 0U};
    return TRUE;
}

std::span<const WorkspaceWorkArea> CurrentWorkAreas(
    HWND window, MonitorCollection& collection) noexcept {
    collection.fallback_dpi = window == nullptr ? 96U : GetDpiForWindow(window);
    if (collection.fallback_dpi == 0U) {
        collection.fallback_dpi = 96U;
    }
    MonitorCollection* const previous_collection = g_monitor_collection;
    g_monitor_collection = &collection;
    EnumDisplayMonitors(
        nullptr,
        nullptr,
        CollectMonitor,
        0);
    g_monitor_collection = previous_collection;
    return std::span<const WorkspaceWorkArea>(
        collection.work_areas.data(), collection.count);
}

}  // namespace

int ScaleWorkspaceDip(int value, UINT dpi) noexcept {
    return MulDiv(
        value,
        static_cast<int>(dpi == 0U ? kReferenceDpi : dpi),
        kReferenceDpi);
}

WorkspaceLayoutRects ComputeWorkspaceLayout(
    int client_width,
    int client_height,
    int status_height,
    UINT dpi,
    const WorkspaceLayoutState& state) noexcept {
    WorkspaceLayoutRects output{};
    const int available_height = std::max(
        0, client_height - std::max(0, status_height));
    output.dock = ComputeDockLayout(
        state.dock,
        std::max(0, client_width),
        available_height,
        dpi);
    output.editor = ToRect(output.dock.editor);

    const int strip_width = ScaleWorkspaceDip(
        state.density == WorkspaceDensity::Compact ? 72 : 92, dpi);
    const int button_height = ScaleWorkspaceDip(
        state.density == WorkspaceDensity::Compact ? 24 : 28, dpi);
    const int gap = std::max(1, ScaleWorkspaceDip(2, dpi));
    const auto is_auto_hidden = [&state](WorkspaceAuxiliaryPane type) {
        const DockPanePlacement* placement = state.dock.Pane(
            AuxiliaryDockPaneType(type));
        return placement != nullptr && placement->zone == DockZone::AutoHide;
    };
    std::array<std::size_t, 3U> edge_counts{};
    for (const WorkspaceAuxiliaryPaneState& pane : state.auxiliary) {
        if (is_auto_hidden(pane.type)) {
            ++edge_counts[static_cast<std::size_t>(pane.edge)];
        }
    }
    if (edge_counts[static_cast<std::size_t>(WorkspaceAutoHideEdge::Left)] > 0U
        && output.editor.right - output.editor.left > strip_width * 2) {
        output.editor.left += strip_width + gap;
    }
    if (edge_counts[static_cast<std::size_t>(WorkspaceAutoHideEdge::Right)] > 0U
        && output.editor.right - output.editor.left > strip_width * 2) {
        output.editor.right -= strip_width + gap;
    }
    if (edge_counts[static_cast<std::size_t>(WorkspaceAutoHideEdge::Bottom)] > 0U
        && output.editor.bottom - output.editor.top > button_height * 2) {
        output.editor.bottom -= button_height + gap;
    }
    std::array<int, 3U> edge_positions{};
    for (std::size_t index = 0U; index < state.auxiliary.size(); ++index) {
        const WorkspaceAuxiliaryPaneState& pane = state.auxiliary[index];
        if (!is_auto_hidden(pane.type)) {
            continue;
        }
        int& position = edge_positions[static_cast<std::size_t>(pane.edge)];
        if (pane.edge == WorkspaceAutoHideEdge::Left) {
            output.auto_hide_buttons[index] = RECT{
                output.dock.editor.x,
                output.dock.editor.y + position,
                output.dock.editor.x + strip_width,
                output.dock.editor.y + position + button_height};
            position += button_height + gap;
        } else if (pane.edge == WorkspaceAutoHideEdge::Right) {
            const int right = output.dock.editor.x + output.dock.editor.width;
            output.auto_hide_buttons[index] = RECT{
                right - strip_width,
                output.dock.editor.y + position,
                right,
                output.dock.editor.y + position + button_height};
            position += button_height + gap;
        } else {
            const int bottom = output.dock.editor.y + output.dock.editor.height;
            output.auto_hide_buttons[index] = RECT{
                output.dock.editor.x + position,
                bottom - button_height,
                output.dock.editor.x + position + strip_width,
                bottom};
            position += strip_width + gap;
        }
    }

    const int tab_height = std::min(
        std::max(
            0,
            static_cast<int>(output.editor.bottom - output.editor.top)),
        ScaleWorkspaceDip(
            state.density == WorkspaceDensity::Compact
                ? kTabsHeightDip - 4
                : kTabsHeightDip,
            dpi));
    output.document_tabs = RECT{
        output.editor.left,
        output.editor.top,
        output.editor.right,
        output.editor.top + tab_height};
    output.canvas = RECT{
        output.editor.left,
        output.editor.top + tab_height,
        output.editor.right,
        output.editor.bottom};
    return output;
}

void ResetWorkspaceLayout(WorkspaceLayoutState& state) noexcept {
    const WorkspaceWindowPlacement window = state.window;
    state = WorkspaceLayoutState{};
    state.window = window;
}

bool ApplyWorkspacePreset(
    WorkspaceLayoutState& state, WorkspacePreset preset) noexcept {
    if (preset >= WorkspacePreset::Count || preset == WorkspacePreset::Custom) {
        return false;
    }
    const WorkspaceWindowPlacement window = state.window;
    state = WorkspaceLayoutState{};
    state.window = window;
    state.selected_preset = preset;
    switch (preset) {
        case WorkspacePreset::Coloring:
            break;
        case WorkspacePreset::LineCleanup:
            static_cast<void>(state.dock.HidePane(DockPaneType::Color));
            static_cast<void>(state.dock.SetZoneExtentDip(DockZone::Right, 360));
            break;
        case WorkspacePreset::ReferenceCheck:
            static_cast<void>(state.dock.HidePane(DockPaneType::Tool));
            static_cast<void>(state.dock.HidePane(DockPaneType::ToolOptions));
            static_cast<void>(state.dock.HidePane(DockPaneType::Layer));
            SetAuxiliaryPreset(
                state,
                WorkspaceAuxiliaryPane::Locator,
                false,
                true,
                WorkspaceAutoHideEdge::Right);
            SetAuxiliaryPreset(
                state,
                WorkspaceAuxiliaryPane::Sequence,
                false,
                true,
                WorkspaceAutoHideEdge::Bottom);
            SetAuxiliaryPreset(
                state,
                WorkspaceAuxiliaryPane::LightTable,
                false,
                true,
                WorkspaceAutoHideEdge::Right);
            SetAuxiliaryPreset(
                state,
                WorkspaceAuxiliaryPane::Reference,
                true,
                false);
            static_cast<void>(state.dock.SetZoneMode(
                DockZone::Right, DockStackMode::Tabs));
            static_cast<void>(state.dock.SetActiveTab(
                DockZone::Right, DockPaneType::Reference));
            break;
        case WorkspacePreset::Batch:
            static_cast<void>(state.dock.HidePane(DockPaneType::Tool));
            static_cast<void>(state.dock.HidePane(DockPaneType::ToolOptions));
            static_cast<void>(state.dock.HidePane(DockPaneType::Color));
            static_cast<void>(state.dock.HidePane(DockPaneType::Layer));
            SetAuxiliaryPreset(
                state, WorkspaceAuxiliaryPane::Sequence, false, true,
                WorkspaceAutoHideEdge::Right);
            SetAuxiliaryPreset(
                state, WorkspaceAuxiliaryPane::Batch, true, false);
            break;
        case WorkspacePreset::Focus:
            for (const DockPaneType type : {
                     DockPaneType::Tool,
                     DockPaneType::ToolOptions,
                     DockPaneType::Color,
                     DockPaneType::Layer}) {
                static_cast<void>(state.dock.HidePane(type));
            }
            state.density = WorkspaceDensity::Compact;
            break;
        case WorkspacePreset::Custom:
        case WorkspacePreset::Count:
            return false;
    }
    return true;
}

const wchar_t* WorkspacePresetDisplayName(WorkspacePreset preset) noexcept {
    switch (preset) {
        case WorkspacePreset::Coloring: return L"彩色";
        case WorkspacePreset::LineCleanup: return L"線整理";
        case WorkspacePreset::ReferenceCheck: return L"参照・チェック";
        case WorkspacePreset::Batch: return L"バッチ";
        case WorkspacePreset::Focus: return L"集中";
        case WorkspacePreset::Custom: return L"ユーザー";
        case WorkspacePreset::Count: return L"";
    }
    return L"";
}

bool SetWorkspaceCustomName(
    WorkspaceLayoutState& state, std::wstring_view name) noexcept {
    if (name.empty() || name.size() >= state.custom_name.size()
        || std::any_of(name.begin(), name.end(), [](wchar_t value) {
               return value < L' ';
           })) {
        return false;
    }
    state.custom_name.fill(L'\0');
    std::copy(name.begin(), name.end(), state.custom_name.begin());
    state.selected_preset = WorkspacePreset::Custom;
    return true;
}

WorkspaceAuxiliaryPaneState* FindWorkspaceAuxiliaryPane(
    WorkspaceLayoutState& state, WorkspaceAuxiliaryPane type) noexcept {
    const std::size_t index = AuxiliaryIndex(type);
    return index < state.auxiliary.size() ? &state.auxiliary[index] : nullptr;
}

const WorkspaceAuxiliaryPaneState* FindWorkspaceAuxiliaryPane(
    const WorkspaceLayoutState& state, WorkspaceAuxiliaryPane type) noexcept {
    const std::size_t index = AuxiliaryIndex(type);
    return index < state.auxiliary.size() ? &state.auxiliary[index] : nullptr;
}

DockPaneType DockPaneTypeForAuxiliary(
    WorkspaceAuxiliaryPane type) noexcept {
    return AuxiliaryDockPaneType(type);
}

DockZone DockZoneForAutoHideEdge(
    WorkspaceAutoHideEdge edge) noexcept {
    switch (edge) {
        case WorkspaceAutoHideEdge::Left: return DockZone::Left;
        case WorkspaceAutoHideEdge::Right: return DockZone::Right;
        case WorkspaceAutoHideEdge::Bottom: return DockZone::Bottom;
    }
    return DockZone::Right;
}

bool EncodeWorkspaceLayout(
    const WorkspaceLayoutState& state,
    std::span<std::byte> output,
    std::size_t& written) noexcept {
    written = 0U;
    if (output.size() < sizeof(PersistedWorkspaceLayoutV4)
        || state.selected_preset >= WorkspacePreset::Count
        || state.density > WorkspaceDensity::Compact
        || state.split_orientation > WorkspaceSplitOrientation::Horizontal
        || state.split_ratio_milli < 200U || state.split_ratio_milli > 800U
        || state.layer_split_milli < 200U || state.layer_split_milli > 800U
        || !ValidScreenPlacement(state.window)
        || (state.window.show_command != SW_SHOWNORMAL
            && state.window.show_command != SW_SHOWMAXIMIZED)
        ) {
        return false;
    }
    DockLayoutModel validation{};
    if (!validation.LoadRecord(state.dock.ToRecord())) {
        return false;
    }
    for (const WorkspaceAuxiliaryPaneState& pane : state.auxiliary) {
        if (pane.type >= WorkspaceAuxiliaryPane::Count
            || pane.stable_type_id == 0U
            || pane.edge > WorkspaceAutoHideEdge::Bottom
            || !ValidScreenPlacement(pane.floating)) {
            return false;
        }
    }
    const PersistedWorkspaceLayoutV4 value = EncodeCurrent(state);
    std::memcpy(output.data(), &value, sizeof(value));
    written = sizeof(value);
    return true;
}

WorkspaceLayoutDecodeResult DecodeWorkspaceLayout(
    WorkspaceLayoutState& state,
    std::span<const std::byte> input) noexcept {
    if (input.size() < sizeof(std::uint32_t) * 3U) {
        ResetWorkspaceLayout(state);
        return WorkspaceLayoutDecodeResult::Invalid;
    }
    std::array<std::uint32_t, 3U> header{};
    std::memcpy(header.data(), input.data(), sizeof(header));
    bool decoded{};
    WorkspaceLayoutDecodeResult result = WorkspaceLayoutDecodeResult::Invalid;
    if (header[0] == kMagic
        && (header[1] == kVersion || header[1] == 6U)
        && header[2] == sizeof(PersistedWorkspaceLayoutV4)
        && input.size() == sizeof(PersistedWorkspaceLayoutV4)) {
        PersistedWorkspaceLayoutV4 value{};
        std::memcpy(&value, input.data(), sizeof(value));
        decoded = DecodeVersion4Or5(state, value, false, true);
        result = header[1] == kVersion ? WorkspaceLayoutDecodeResult::Current
                                      : WorkspaceLayoutDecodeResult::Migrated;
    } else if (header[0] == kMagic
        && (header[1] == 4U || header[1] == 5U)
        && header[2] == sizeof(PersistedWorkspaceLayoutV4)
        && input.size() == sizeof(PersistedWorkspaceLayoutV4)) {
        PersistedWorkspaceLayoutV4 value{};
        std::memcpy(&value, input.data(), sizeof(value));
        decoded = DecodeVersion4Or5(state, value, header[1] == 4U, false);
        result = WorkspaceLayoutDecodeResult::Migrated;
    } else if (header[0] == kMagic && header[1] == 3U
        && header[2] == sizeof(PersistedWorkspaceLayoutV3)
        && input.size() == sizeof(PersistedWorkspaceLayoutV3)) {
        PersistedWorkspaceLayoutV3 value{};
        std::memcpy(&value, input.data(), sizeof(value));
        decoded = DecodeVersion3(state, value);
        result = WorkspaceLayoutDecodeResult::Migrated;
    } else if (header[0] == kMagic && header[1] == 2U
        && header[2] == sizeof(LegacyPersistedWorkspaceLayoutV2)
        && input.size() == sizeof(LegacyPersistedWorkspaceLayoutV2)) {
        LegacyPersistedWorkspaceLayoutV2 value{};
        std::memcpy(&value, input.data(), sizeof(value));
        decoded = LoadLegacyLayout(state, value);
        result = WorkspaceLayoutDecodeResult::Migrated;
    }
    if (!decoded) {
        state = WorkspaceLayoutState{};
        return WorkspaceLayoutDecodeResult::Invalid;
    }
    return result;
}

bool ClampWorkspacePlacement(
    WorkspaceScreenPlacement& placement,
    std::span<const WorkspaceWorkArea> work_areas) noexcept {
    if (!placement.valid || !ValidScreenPlacement(placement)
        || work_areas.empty()) {
        return false;
    }
    const RECT source{
        placement.x_px,
        placement.y_px,
        placement.x_px + placement.width_px,
        placement.y_px + placement.height_px};
    const WorkspaceWorkArea* target = nullptr;
    int best_area{};
    for (const WorkspaceWorkArea& candidate : work_areas) {
        const int area = IntersectionArea(source, candidate.bounds_px);
        if (target == nullptr || area > best_area
            || (area == best_area && candidate.primary && !target->primary)) {
            target = &candidate;
            best_area = area;
        }
    }
    if (best_area == 0) {
        const auto primary = std::find_if(
            work_areas.begin(), work_areas.end(), [](const WorkspaceWorkArea& area) {
                return area.primary;
            });
        target = primary == work_areas.end() ? &work_areas.front() : &*primary;
    }
    const UINT target_dpi = target->dpi == 0U ? 96U : target->dpi;
    const UINT source_dpi = placement.capture_dpi == 0U
        ? target_dpi
        : placement.capture_dpi;
    const int available_width = std::max(
        1L, target->bounds_px.right - target->bounds_px.left);
    const int available_height = std::max(
        1L, target->bounds_px.bottom - target->bounds_px.top);
    const int scaled_width = MulDiv(
        placement.width_px, static_cast<int>(target_dpi),
        static_cast<int>(source_dpi));
    const int scaled_height = MulDiv(
        placement.height_px, static_cast<int>(target_dpi),
        static_cast<int>(source_dpi));
    placement.width_px = std::clamp(
        scaled_width,
        std::min(ScaleWorkspaceDip(96, target_dpi), available_width),
        available_width);
    placement.height_px = std::clamp(
        scaled_height,
        std::min(ScaleWorkspaceDip(64, target_dpi), available_height),
        available_height);
    placement.x_px = std::clamp(
        placement.x_px,
        static_cast<int>(target->bounds_px.left),
        static_cast<int>(target->bounds_px.right) - placement.width_px);
    placement.y_px = std::clamp(
        placement.y_px,
        static_cast<int>(target->bounds_px.top),
        static_cast<int>(target->bounds_px.bottom) - placement.height_px);
    placement.capture_dpi = target_dpi;
    return true;
}

void ClampWorkspaceFloatingPanes(
    WorkspaceLayoutState& state,
    std::span<const WorkspaceWorkArea> work_areas) noexcept {
    static_cast<void>(ClampWorkspacePlacement(state.window, work_areas));
    for (WorkspaceAuxiliaryPaneState& pane : state.auxiliary) {
        static_cast<void>(ClampWorkspacePlacement(pane.floating, work_areas));
    }
}

bool CaptureWorkspaceWindowPlacement(
    HWND window, WorkspaceLayoutState& state) noexcept {
    WINDOWPLACEMENT placement{sizeof(placement)};
    if (window == nullptr || GetWindowPlacement(window, &placement) == FALSE) {
        return false;
    }
    const RECT bounds = placement.rcNormalPosition;
    WorkspaceWindowPlacement candidate{};
    candidate.x_px = bounds.left;
    candidate.y_px = bounds.top;
    candidate.width_px = bounds.right - bounds.left;
    candidate.height_px = bounds.bottom - bounds.top;
    candidate.capture_dpi = GetDpiForWindow(window);
    candidate.valid = true;
    candidate.show_command = placement.showCmd == SW_SHOWMAXIMIZED
        ? SW_SHOWMAXIMIZED
        : SW_SHOWNORMAL;
    if (!ValidScreenPlacement(candidate)) {
        return false;
    }
    state.window = candidate;
    return true;
}

bool ApplyWorkspaceWindowPlacement(
    HWND window, WorkspaceLayoutState& state) noexcept {
    if (window == nullptr || !state.window.valid) {
        return false;
    }
    MonitorCollection monitors{};
    const auto work_areas = CurrentWorkAreas(window, monitors);
    if (!ClampWorkspacePlacement(state.window, work_areas)) {
        return false;
    }
    WINDOWPLACEMENT placement{sizeof(placement)};
    placement.showCmd = state.window.show_command;
    placement.rcNormalPosition = RECT{
        state.window.x_px,
        state.window.y_px,
        state.window.x_px + state.window.width_px,
        state.window.y_px + state.window.height_px};
    return SetWindowPlacement(window, &placement) != FALSE;
}

bool CaptureWorkspaceAuxiliaryPlacement(
    HWND window,
    WorkspaceLayoutState& state,
    WorkspaceAuxiliaryPane type) noexcept {
    WorkspaceAuxiliaryPaneState* pane = FindWorkspaceAuxiliaryPane(state, type);
    RECT bounds{};
    if (window == nullptr || pane == nullptr
        || GetWindowRect(window, &bounds) == FALSE) {
        return false;
    }
    const WorkspaceScreenPlacement candidate{
        bounds.left,
        bounds.top,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        GetDpiForWindow(window),
        true};
    if (!ValidScreenPlacement(candidate)) {
        return false;
    }
    pane->floating = candidate;
    return true;
}

bool ApplyWorkspaceAuxiliaryPlacement(
    HWND window,
    HWND owner,
    WorkspaceLayoutState& state,
    WorkspaceAuxiliaryPane type) noexcept {
    WorkspaceAuxiliaryPaneState* pane = FindWorkspaceAuxiliaryPane(state, type);
    if (window == nullptr || pane == nullptr || !pane->floating.valid) {
        return false;
    }
    MonitorCollection monitors{};
    const auto work_areas = CurrentWorkAreas(owner, monitors);
    if (!ClampWorkspacePlacement(pane->floating, work_areas)) {
        return false;
    }
    return SetWindowPos(
               window,
               nullptr,
               pane->floating.x_px,
               pane->floating.y_px,
               pane->floating.width_px,
               pane->floating.height_px,
               SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER)
        != FALSE;
}

bool LoadWorkspaceLayout(
    WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept {
    if (value_name == nullptr || *value_name == L'\0') {
        return false;
    }
    DWORD type{};
    DWORD size{};
    const LSTATUS query = RegGetValueW(
        HKEY_CURRENT_USER,
        kSettingsKey,
        value_name,
        RRF_RT_REG_BINARY,
        &type,
        nullptr,
        &size);
    if (query != ERROR_SUCCESS || type != REG_BINARY
        || size > kMaximumWorkspaceLayoutRecordBytes) {
        if (query == ERROR_SUCCESS) {
            state = WorkspaceLayoutState{};
        }
        return false;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> bytes{};
    DWORD actual = size;
    if (RegGetValueW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            value_name,
            RRF_RT_REG_BINARY,
            &type,
            bytes.data(),
            &actual)
            != ERROR_SUCCESS
        || actual != size) {
        state = WorkspaceLayoutState{};
        return false;
    }
    const WorkspaceLayoutDecodeResult result = DecodeWorkspaceLayout(
        state, std::span<const std::byte>(bytes.data(), actual));
    if (result == WorkspaceLayoutDecodeResult::Invalid) {
        return false;
    }
    if (result == WorkspaceLayoutDecodeResult::Migrated) {
        static_cast<void>(SaveWorkspaceLayout(state, value_name));
    }
    return true;
}

bool SaveWorkspaceLayout(
    const WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept {
    if (value_name == nullptr || *value_name == L'\0') {
        return false;
    }
    std::array<std::byte, kMaximumWorkspaceLayoutRecordBytes> bytes{};
    std::size_t written{};
    if (!EncodeWorkspaceLayout(state, bytes, written)) {
        return false;
    }
    HKEY key{};
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            0,
            nullptr,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            nullptr,
            &key,
            nullptr)
        != ERROR_SUCCESS) {
        return false;
    }
    const LSTATUS status = RegSetValueExW(
        key,
        value_name,
        0,
        REG_BINARY,
        reinterpret_cast<const BYTE*>(bytes.data()),
        static_cast<DWORD>(written));
    RegCloseKey(key);
    return status == ERROR_SUCCESS;
}

bool DeleteWorkspaceLayout(const wchar_t* value_name) noexcept {
    if (value_name == nullptr || *value_name == L'\0') {
        return false;
    }
    HKEY key{};
    if (RegOpenKeyExW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            0,
            KEY_SET_VALUE,
            &key)
        != ERROR_SUCCESS) {
        return false;
    }
    const LSTATUS status = RegDeleteValueW(key, value_name);
    RegCloseKey(key);
    return status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND;
}

bool LoadWorkspaceWindowCount(std::uint32_t& count) noexcept {
    DWORD value{};
    DWORD size = sizeof(value);
    if (RegGetValueW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            kWorkspaceWindowCountValue,
            RRF_RT_REG_DWORD,
            nullptr,
            &value,
            &size)
            != ERROR_SUCCESS
        || size != sizeof(value)
        || value == 0U
        || value > kMaximumPersistedWorkspaceWindows) {
        count = 1U;
        return false;
    }
    count = value;
    return true;
}

bool SaveWorkspaceWindowCount(std::uint32_t count) noexcept {
    if (count == 0U || count > kMaximumPersistedWorkspaceWindows) {
        return false;
    }
    HKEY key{};
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            0,
            nullptr,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            nullptr,
            &key,
            nullptr)
        != ERROR_SUCCESS) {
        return false;
    }
    const DWORD value = count;
    const LSTATUS status = RegSetValueExW(
        key,
        kWorkspaceWindowCountValue,
        0,
        REG_DWORD,
        reinterpret_cast<const BYTE*>(&value),
        sizeof(value));
    RegCloseKey(key);
    return status == ERROR_SUCCESS;
}

}  // namespace inkpod::windows::ui
