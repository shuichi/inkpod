#include "ui/localization.h"

#include "dock_layout.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <initializer_list>

#include "app/resource.h"
#include "ui/right_tool_tabs.h"

namespace inkpod::windows::ui {
namespace {

constexpr int kReferenceDpi = 96;
constexpr int kSplitterDip = 4;
constexpr int kToolTabHeightDip = 28;
constexpr int kMinimumEditorWidthDip = 320;
constexpr int kMinimumEditorHeightDip = 240;
constexpr std::uint32_t kMinimumSplitWeight = 100U;
constexpr std::uint32_t kMaximumSplitWeight = 100'000U;

constexpr std::size_t PaneIndex(DockPaneType type) noexcept {
    return static_cast<std::size_t>(type);
}

constexpr std::size_t ZoneIndex(DockZone zone) noexcept {
    return static_cast<std::size_t>(zone);
}

constexpr std::uint32_t DockedAndTransientZones(
    std::initializer_list<DockZone> zones,
    bool auto_hide = false) noexcept {
    std::uint32_t value = DockZoneBit(DockZone::Floating)
        | DockZoneBit(DockZone::Hidden);
    if (auto_hide) {
        value |= DockZoneBit(DockZone::AutoHide);
    }
    for (const DockZone zone : zones) {
        value |= DockZoneBit(zone);
    }
    return value;
}

const std::array<PaneDescriptor, kDockPaneCount> kPaneDescriptors{{
    {DockPaneType::Tool,
     UINT32_C(0x4c4f4f54),
     IDS_DOCK_PANE_TOOL,
     UiText(UiStringId::Text0242),
     DockZone::Left,
     DockedAndTransientZones({DockZone::Left, DockZone::Right}),
      PaneTargetScope::Application,
      1U,
      true,
      true,
      true,
      false,
      false,
     80,
     120,
     80,
     520,
     90U},
    {DockPaneType::ToolOptions,
     UINT32_C(0x54504f54),
     IDS_DOCK_PANE_TOOL_OPTIONS,
     UiText(UiStringId::Text0241),
     DockZone::TopContext,
     DockedAndTransientZones({DockZone::TopContext, DockZone::Bottom}),
      PaneTargetScope::FollowActiveView,
      1U,
      true,
      true,
      true,
      false,
      false,
     320,
     28,
     720,
     40,
     100U},
    {DockPaneType::Color,
     UINT32_C(0x524c4f43),
     IDS_DOCK_PANE_COLOR,
     UiText(UiStringId::Color),
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}),
      PaneTargetScope::Application,
      1U,
      true,
      true,
      true,
      false,
      true,
     240,
     120,
     320,
     220,
     60U},
    {DockPaneType::Layer,
     UINT32_C(0x5259414c),
     IDS_DOCK_PANE_LAYER,
     UiText(UiStringId::LayerPlane),
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}),
      PaneTargetScope::FollowActiveView,
      1U,
      true,
      true,
      true,
      false,
      true,
     240,
     180,
     320,
      420,
      80U},
    {DockPaneType::Locator,
     UINT32_C(0x41434f4c),
     IDS_PANE_LOCATOR,
     UiText(UiStringId::Text0411),
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
     true,
     true,
     true,
     true,
     220,
     220,
     300,
     380,
     45U},
    {DockPaneType::Sequence,
     UINT32_C(0x55514553),
     IDS_PANE_SEQUENCE,
     UiText(UiStringId::Text0204),
     DockZone::Bottom,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
     true,
     true,
     true,
     true,
     260,
     200,
     360,
     420,
     40U},
    {DockPaneType::LightTable,
     UINT32_C(0x544c474c),
     IDS_PANE_LIGHT_TABLE,
     UiText(UiStringId::LightTable),
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
     true,
     true,
     true,
     true,
     280,
     260,
     380,
     500,
     35U},
    {DockPaneType::Reference,
     UINT32_C(0x45464552),
     IDS_PANE_REFERENCE,
     UiText(UiStringId::Text0194),
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
     true,
     true,
     true,
     true,
     300,
     260,
     440,
     560,
     30U},
    {DockPaneType::Batch,
     UINT32_C(0x48435442),
     IDS_PANE_BATCH,
     UiText(UiStringId::Text0255),
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
     true,
     true,
     true,
     true,
     300,
     300,
     420,
     560,
     25U},
    {DockPaneType::JobProgress,
     UINT32_C(0x424f4a50),
     IDS_PANE_JOB_PROGRESS,
     UiText(UiStringId::Text0513),
     DockZone::Bottom,
     DockedAndTransientZones({DockZone::Bottom}),
     PaneTargetScope::Job,
     1U,
     false,
     false,
     false,
     false,
     true,
     320,
     84,
     720,
     112,
     10U},
}};

DockFloatingPlacement DefaultFloatingPlacement(DockPaneType type) noexcept {
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    const int offset = static_cast<int>(PaneIndex(type)) * 28;
    return DockFloatingPlacement{
        120 + offset,
        120 + offset,
        descriptor == nullptr ? 320 : descriptor->preferred_width_dip,
        descriptor == nullptr ? 320 : descriptor->preferred_height_dip};
}

constexpr bool IsDefaultLowerInspectorTab(DockPaneType type) noexcept {
    return type == DockPaneType::Layer || type == DockPaneType::LightTable
        || type == DockPaneType::Reference;
}

DockPanePlacement DefaultPlacement(DockPaneType type) noexcept {
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    const bool lower_inspector_tab = IsDefaultLowerInspectorTab(type);
    DockPanePlacement placement{};
    placement.type = type;
    placement.present = descriptor != nullptr;
    placement.zone = descriptor == nullptr || !descriptor->default_visible
        ? DockZone::Hidden
        : descriptor->default_zone;
    placement.restore_zone = descriptor == nullptr
        ? DockZone::Left
        : descriptor->default_zone;
    placement.order = lower_inspector_tab
        ? 1U
        : (type == DockPaneType::Tool || type == DockPaneType::ToolOptions
                   || type == DockPaneType::Color || type == DockPaneType::Sequence
               ? 0U
               : static_cast<std::uint8_t>(PaneIndex(type)));
    placement.stack = lower_inspector_tab
        ? 1U
        : (type == DockPaneType::Tool || type == DockPaneType::ToolOptions
                   || type == DockPaneType::Color || type == DockPaneType::Sequence
               ? 0U
               : static_cast<std::uint8_t>(PaneIndex(type)));
    placement.tab_order = type == DockPaneType::LightTable
        ? 1U
        : (type == DockPaneType::Reference ? 2U : 0U);
    placement.split_weight = type == DockPaneType::Color
        ? 320U
        : (lower_inspector_tab ? 680U : 1000U);
    placement.floating = DefaultFloatingPlacement(type);
    placement.active_tab = descriptor != nullptr && descriptor->default_visible;
    return placement;
}

int ScaleDip(int value, unsigned int dpi) noexcept {
    constexpr unsigned int kMaximumLayoutDpi = 960U;
    const auto effective_dpi = static_cast<std::int64_t>(
        dpi == 0U ? kReferenceDpi : std::min(dpi, kMaximumLayoutDpi));
    return static_cast<int>(
        (static_cast<std::int64_t>(value) * effective_dpi + kReferenceDpi / 2)
        / kReferenceDpi);
}

bool HasArea(const DockRect& rect) noexcept {
    return rect.width > 0 && rect.height > 0;
}

bool ValidFloatingPlacement(
    const DockFloatingPlacement& value,
    const PaneDescriptor& descriptor) noexcept {
    constexpr int kCoordinateLimit = 1'000'000;
    constexpr int kSizeLimit = 16'384;
    return value.x_dip >= -kCoordinateLimit && value.x_dip <= kCoordinateLimit
        && value.y_dip >= -kCoordinateLimit && value.y_dip <= kCoordinateLimit
        && value.width_dip >= descriptor.minimum_width_dip
        && value.width_dip <= kSizeLimit
        && value.height_dip >= descriptor.minimum_height_dip
        && value.height_dip <= kSizeLimit;
}

bool SameFloatingPlacement(
    const DockFloatingPlacement& left,
    const DockFloatingPlacement& right) noexcept {
    return left.x_dip == right.x_dip && left.y_dip == right.y_dip
        && left.width_dip == right.width_dip
        && left.height_dip == right.height_dip;
}

int MinimumZoneExtent(
    const DockLayoutModel& model, DockZone zone) noexcept {
    int minimum = zone == DockZone::TopContext || zone == DockZone::Bottom
        ? 28
        : 64;
    for (const PaneDescriptor& descriptor : kPaneDescriptors) {
        const DockPanePlacement* pane = model.Pane(descriptor.type);
        if (pane == nullptr || !pane->present || pane->zone != zone) {
            continue;
        }
        minimum = std::max(
            minimum,
            zone == DockZone::TopContext || zone == DockZone::Bottom
                ? descriptor.minimum_height_dip
                : descriptor.minimum_width_dip);
    }
    return minimum;
}

int MinimumZoneExtent(
    const DockLayoutRecord& record, DockZone zone) noexcept {
    int minimum = zone == DockZone::TopContext || zone == DockZone::Bottom
        ? 28
        : 64;
    for (const DockPanePlacement& pane : record.panes) {
        const PaneDescriptor* descriptor = FindPaneDescriptor(pane.type);
        if (!pane.present || pane.zone != zone || descriptor == nullptr) {
            continue;
        }
        minimum = std::max(
            minimum,
            zone == DockZone::TopContext || zone == DockZone::Bottom
                ? descriptor->minimum_height_dip
                : descriptor->minimum_width_dip);
    }
    return minimum;
}

std::array<const DockPanePlacement*, kDockPaneCount> OrderedPanes(
    const DockLayoutModel& model,
    DockZone zone,
    std::size_t& count) noexcept {
    std::array<const DockPanePlacement*, kDockPaneCount> output{};
    count = 0U;
    for (std::size_t index = 0U; index < kDockPaneCount; ++index) {
        const auto type = static_cast<DockPaneType>(index);
        const DockPanePlacement* pane = model.Pane(type);
        if (pane != nullptr && pane->present && pane->zone == zone) {
            output[count++] = pane;
        }
    }
    std::sort(
        output.begin(),
        output.begin() + static_cast<std::ptrdiff_t>(count),
        [](const DockPanePlacement* left, const DockPanePlacement* right) {
            if (left->order != right->order) {
                return left->order < right->order;
            }
            if (left->stack != right->stack) {
                return left->stack < right->stack;
            }
            if (left->tab_order != right->tab_order) {
                return left->tab_order < right->tab_order;
            }
            return PaneIndex(left->type) < PaneIndex(right->type);
        });
    return output;
}

struct OrderedDockStack {
    std::uint8_t id{};
    std::uint8_t order{};
    std::uint32_t split_weight{1000U};
    std::array<const DockPanePlacement*, kDockPaneCount> panes{};
    std::size_t pane_count{};
    DockPaneType active{DockPaneType::Count};
};

std::array<OrderedDockStack, kDockPaneCount> OrderedStacks(
    const DockLayoutModel& model,
    DockZone zone,
    std::size_t& count,
    const RightToolTabsModel* right_tool_tabs = nullptr) noexcept {
    std::array<OrderedDockStack, kDockPaneCount> output{};
    count = 0U;
    if (zone == DockZone::Right && right_tool_tabs != nullptr) {
        const ToolTab* selected = right_tool_tabs->SelectedTab();
        if (selected == nullptr) {
            return output;
        }
        for (std::size_t index = 0U;
             index < selected->pane_count && count < output.size();
             ++index) {
            const DockPaneType type = selected->panes[index];
            const DockPanePlacement* pane = model.Pane(type);
            if (pane == nullptr || !pane->present || pane->zone != zone) {
                continue;
            }
            OrderedDockStack& stack = output[count];
            stack.id = static_cast<std::uint8_t>(PaneIndex(type));
            stack.order = static_cast<std::uint8_t>(count);
            stack.split_weight = pane->split_weight;
            stack.panes[0] = pane;
            stack.pane_count = 1U;
            stack.active = type;
            ++count;
        }
        return output;
    }
    std::size_t pane_count{};
    const auto panes = OrderedPanes(model, zone, pane_count);
    for (std::size_t pane_index = 0U; pane_index < pane_count; ++pane_index) {
        const DockPanePlacement* pane = panes[pane_index];
        std::size_t stack_index{};
        while (stack_index < count && output[stack_index].id != pane->stack) {
            ++stack_index;
        }
        if (stack_index == count) {
            output[count].id = pane->stack;
            output[count].order = pane->order;
            output[count].split_weight = pane->split_weight;
            ++count;
        }
        OrderedDockStack& stack = output[stack_index];
        stack.panes[stack.pane_count++] = pane;
        if (pane->active_tab || stack.active == DockPaneType::Count) {
            stack.active = pane->type;
        }
    }
    std::sort(
        output.begin(),
        output.begin() + static_cast<std::ptrdiff_t>(count),
        [](const OrderedDockStack& left, const OrderedDockStack& right) {
            if (left.order != right.order) {
                return left.order < right.order;
            }
            return left.id < right.id;
        });
    return output;
}

void MarkTemporaryAutoHide(
    DockLayoutGeometry& output,
    const DockLayoutModel& model,
    DockZone zone) noexcept {
    for (std::size_t index = 0U; index < kDockPaneCount; ++index) {
        const auto type = static_cast<DockPaneType>(index);
        const DockPanePlacement* pane = model.Pane(type);
        if (pane != nullptr && pane->present && pane->zone == zone) {
            output.panes[index].temporarily_auto_hidden = true;
            output.panes[index].shown = false;
        }
    }
}

void AddSplitter(
    DockLayoutGeometry& output,
    DockSplitterKind kind,
    DockZone zone,
    std::uint8_t boundary,
    const DockRect& bounds) noexcept {
    if (!HasArea(bounds) || output.splitter_count >= output.splitters.size()) {
        return;
    }
    output.splitters[output.splitter_count++] =
        DockSplitterGeometry{kind, zone, boundary, bounds};
}

void LayoutZone(
    DockLayoutGeometry& output,
    const DockLayoutModel& model,
    DockZone zone,
    const DockRect& bounds,
    int splitter,
    unsigned int dpi,
    const RightToolTabsModel* right_tool_tabs) noexcept {
    if (!HasArea(bounds)) {
        return;
    }
    std::size_t count{};
    const auto stacks = OrderedStacks(model, zone, count, right_tool_tabs);
    if (count == 0U) {
        return;
    }

    const bool horizontal = zone == DockZone::TopContext || zone == DockZone::Bottom;
    const int extent = horizontal ? bounds.width : bounds.height;
    const int available = std::max(
        0, extent - splitter * static_cast<int>(count - 1U));
    std::array<int, kDockPaneCount> minimums{};
    std::array<int, kDockPaneCount> sizes{};
    int minimum_total{};
    std::uint64_t weight_total{};
    for (std::size_t index = 0U; index < count; ++index) {
        minimums[index] = 1;
        for (std::size_t pane_index = 0U;
             pane_index < stacks[index].pane_count;
             ++pane_index) {
            const PaneDescriptor* descriptor = FindPaneDescriptor(
                stacks[index].panes[pane_index]->type);
            if (descriptor != nullptr) {
                minimums[index] = std::max(
                    minimums[index],
                    ScaleDip(
                        horizontal ? descriptor->minimum_width_dip
                                   : descriptor->minimum_height_dip,
                        dpi));
            }
        }
        minimum_total += minimums[index];
        weight_total += stacks[index].split_weight;
    }
    int remaining = available;
    int remaining_minimum = minimum_total;
    std::uint64_t remaining_weight = std::max<std::uint64_t>(1U, weight_total);
    const bool preserve_positive_right_panes = zone == DockZone::Right
        && right_tool_tabs != nullptr
        && available >= static_cast<int>(count);
    for (std::size_t index = 0U; index < count; ++index) {
        if (index + 1U == count) {
            sizes[index] = std::max(0, remaining);
            break;
        }
        const int raw = static_cast<int>(
            static_cast<std::int64_t>(remaining) * stacks[index].split_weight
            / static_cast<std::int64_t>(remaining_weight));
        const int minimum = available >= minimum_total
            ? minimums[index]
            : preserve_positive_right_panes ? 1 : 0;
        const int remaining_after_minimum = available >= minimum_total
            ? remaining_minimum - minimums[index]
            : preserve_positive_right_panes
                ? static_cast<int>(count - index - 1U)
                : 0;
        const int maximum = std::max(minimum, remaining - remaining_after_minimum);
        sizes[index] = std::clamp(raw, minimum, maximum);
        remaining -= sizes[index];
        remaining_minimum -= minimums[index];
        remaining_weight = remaining_weight > stacks[index].split_weight
            ? remaining_weight - stacks[index].split_weight
            : 1U;
    }

    int cursor = horizontal ? bounds.x : bounds.y;
    for (std::size_t index = 0U; index < count; ++index) {
        DockRect pane_bounds = bounds;
        if (horizontal) {
            pane_bounds.x = cursor;
            pane_bounds.width = sizes[index];
        } else {
            pane_bounds.y = cursor;
            pane_bounds.height = sizes[index];
        }
        for (std::size_t pane_index = 0U;
             pane_index < stacks[index].pane_count;
             ++pane_index) {
            const DockPanePlacement* pane = stacks[index].panes[pane_index];
            DockPaneGeometry& geometry = output.panes[PaneIndex(pane->type)];
            geometry.bounds = pane_bounds;
            geometry.shown = HasArea(pane_bounds)
                && pane->type == stacks[index].active;
        }
        cursor += sizes[index];
        if (index + 1U < count) {
            const DockRect splitter_bounds = horizontal
                ? DockRect{cursor, bounds.y, splitter, bounds.height}
                : DockRect{bounds.x, cursor, bounds.width, splitter};
            AddSplitter(
                output,
                DockSplitterKind::StackBoundary,
                zone,
                static_cast<std::uint8_t>(index),
                splitter_bounds);
            cursor += splitter;
        }
    }
}

}  // namespace

DockLayoutModel::DockLayoutModel() noexcept {
    Reset();
}

void DockLayoutModel::Reset() noexcept {
    mirrored_ = false;
    for (std::size_t index = 0U; index < kDockPaneCount; ++index) {
        panes_[index] = DefaultPlacement(static_cast<DockPaneType>(index));
    }
    zones_[ZoneIndex(DockZone::TopContext)] =
        DockZoneState{DockStackMode::Split, DockPaneType::ToolOptions, 40};
    zones_[ZoneIndex(DockZone::Left)] =
        DockZoneState{DockStackMode::Split, DockPaneType::Tool, 80};
    zones_[ZoneIndex(DockZone::Right)] =
        DockZoneState{DockStackMode::Split, DockPaneType::Color, 320};
    zones_[ZoneIndex(DockZone::Bottom)] =
        DockZoneState{DockStackMode::Split, DockPaneType::Count, 220};
}

DockResult DockLayoutModel::AddPane(DockPaneType type) noexcept {
    DockPanePlacement* pane = Pane(type);
    if (pane == nullptr) {
        return DockResult::InvalidPane;
    }
    if (pane->present) {
        return DockResult::DuplicatePane;
    }
    *pane = DefaultPlacement(type);
    NormalizeOrders(pane->zone);
    return DockResult::Ok;
}

DockResult DockLayoutModel::RemovePane(DockPaneType type) noexcept {
    DockPanePlacement* pane = Pane(type);
    if (pane == nullptr) {
        return DockResult::InvalidPane;
    }
    if (!pane->present) {
        return DockResult::NoOp;
    }
    const DockZone old_zone = pane->zone;
    pane->present = false;
    pane->zone = DockZone::Hidden;
    NormalizeOrders(old_zone);
    return DockResult::Ok;
}

DockResult DockLayoutModel::MovePane(
    DockPaneType type, DockZone zone) noexcept {
    DockPanePlacement* pane = Pane(type);
    if (pane == nullptr || !pane->present) {
        return DockResult::InvalidPane;
    }
    if (!IsZoneAllowed(type, zone) || zone == DockZone::AutoHide) {
        return DockResult::ZoneNotAllowed;
    }
    if (zone == DockZone::Floating) {
        return FloatPane(type, pane->floating);
    }
    if (zone == DockZone::Hidden) {
        return HidePane(type);
    }
    if (pane->zone == zone) {
        return DockResult::NoOp;
    }
    const DockZone old_zone = pane->zone;
    const std::size_t destination_stack_count = StackCount(zone);
    std::array<bool, kDockPaneCount> used_stacks{};
    for (const DockPanePlacement& candidate : panes_) {
        if (candidate.type != type && candidate.present && candidate.zone == zone
            && candidate.stack < used_stacks.size()) {
            used_stacks[candidate.stack] = true;
        }
    }
    const auto available = std::find(
        used_stacks.begin(), used_stacks.end(), false);
    pane->stack = available == used_stacks.end()
        ? 0U
        : static_cast<std::uint8_t>(
              std::distance(used_stacks.begin(), available));
    pane->zone = zone;
    pane->restore_zone = zone;
    pane->order = static_cast<std::uint8_t>(destination_stack_count);
    pane->tab_order = 0U;
    pane->split_weight = 1000U;
    pane->active_tab = true;
    NormalizeOrders(old_zone);
    NormalizeOrders(zone);
    return DockResult::Ok;
}

DockResult DockLayoutModel::TabPane(
    DockPaneType type, DockPaneType target) noexcept {
    DockPanePlacement* source = Pane(type);
    const DockPanePlacement* destination = Pane(target);
    if (source == nullptr || destination == nullptr || !source->present
        || !destination->present || type == target
        || !IsDockedZone(destination->zone)) {
        return DockResult::InvalidState;
    }
    const DockZone destination_zone = destination->zone;
    const std::uint8_t destination_stack = destination->stack;
    if (source->zone == destination_zone && source->stack == destination_stack) {
        return SetActiveTab(destination_zone, type);
    }
    const DockZone old_zone = source->zone;
    const std::size_t tab_count = StackPaneCount(
        destination_zone, destination_stack);
    source->zone = destination_zone;
    source->restore_zone = destination_zone;
    source->stack = destination_stack;
    source->order = destination->order;
    source->tab_order = static_cast<std::uint8_t>(tab_count);
    source->split_weight = destination->split_weight;
    source->active_tab = true;
    for (DockPanePlacement& pane : panes_) {
        if (pane.present && pane.zone == destination_zone
            && pane.stack == destination_stack && pane.type != type) {
            pane.active_tab = false;
        }
    }
    NormalizeOrders(old_zone);
    NormalizeOrders(destination_zone);
    if (DockZoneState* zone = Zone(destination_zone); zone != nullptr) {
        zone->active_tab = type;
    }
    return DockResult::Ok;
}

DockResult DockLayoutModel::FloatPane(
    DockPaneType type, const DockFloatingPlacement& placement) noexcept {
    DockPanePlacement* pane = Pane(type);
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    if (pane == nullptr || descriptor == nullptr || !pane->present) {
        return DockResult::InvalidPane;
    }
    if (!descriptor->can_float
        || !ValidFloatingPlacement(placement, *descriptor)) {
        return DockResult::ZoneNotAllowed;
    }
    const bool placement_changed = !SameFloatingPlacement(
        pane->floating, placement);
    if (IsDockedZone(pane->zone)) {
        pane->restore_zone = pane->zone;
    }
    const DockZone old_zone = pane->zone;
    pane->zone = DockZone::Floating;
    pane->floating = placement;
    NormalizeOrders(old_zone);
    return old_zone == DockZone::Floating && !placement_changed
        ? DockResult::NoOp
        : DockResult::Ok;
}

DockResult DockLayoutModel::HidePane(DockPaneType type) noexcept {
    DockPanePlacement* pane = Pane(type);
    if (pane == nullptr || !pane->present) {
        return DockResult::InvalidPane;
    }
    if (pane->zone == DockZone::Hidden) {
        return DockResult::NoOp;
    }
    const DockZone old_zone = pane->zone;
    if (IsDockedZone(old_zone)) {
        pane->restore_zone = old_zone;
    }
    pane->zone = DockZone::Hidden;
    NormalizeOrders(old_zone);
    return DockResult::Ok;
}

DockResult DockLayoutModel::SetPaneAutoHide(
    DockPaneType type, bool auto_hide) noexcept {
    DockPanePlacement* pane = Pane(type);
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    if (pane == nullptr || descriptor == nullptr || !pane->present) {
        return DockResult::InvalidPane;
    }
    if (!descriptor->can_auto_hide) {
        return DockResult::ZoneNotAllowed;
    }
    if (auto_hide) {
        if (pane->zone == DockZone::AutoHide) {
            return DockResult::NoOp;
        }
        const DockZone old_zone = pane->zone;
        if (IsDockedZone(old_zone)) {
            pane->restore_zone = old_zone;
        }
        pane->zone = DockZone::AutoHide;
        NormalizeOrders(old_zone);
        return DockResult::Ok;
    }
    if (pane->zone != DockZone::AutoHide) {
        return DockResult::NoOp;
    }
    DockZone target = pane->restore_zone;
    if (!IsDockedZone(target) || !IsZoneAllowed(type, target)) {
        target = descriptor->default_zone;
    }
    const bool target_was_empty = PaneCount(target) == 0U;
    const std::size_t existing_tabs = StackPaneCount(target, pane->stack);
    pane->zone = target;
    if (existing_tabs == 0U) {
        pane->order = static_cast<std::uint8_t>(StackCount(target));
        pane->tab_order = 0U;
        pane->active_tab = true;
    } else {
        pane->tab_order = static_cast<std::uint8_t>(existing_tabs);
        pane->active_tab = false;
    }
    NormalizeOrders(target);
    if (target_was_empty) {
        static_cast<void>(SetZoneExtentDip(
            target,
            target == DockZone::TopContext || target == DockZone::Bottom
                ? descriptor->preferred_height_dip
                : descriptor->preferred_width_dip));
    }
    return DockResult::Ok;
}

DockResult DockLayoutModel::RestorePane(DockPaneType type) noexcept {
    DockPanePlacement* pane = Pane(type);
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    if (pane == nullptr || descriptor == nullptr || !pane->present) {
        return DockResult::InvalidPane;
    }
    if (pane->zone != DockZone::Hidden) {
        return DockResult::NoOp;
    }
    DockZone target = pane->restore_zone;
    if (!IsDockedZone(target) || !IsZoneAllowed(type, target)) {
        target = descriptor->default_zone;
    }
    const bool target_was_empty = PaneCount(target) == 0U;
    const std::size_t existing_tabs = StackPaneCount(target, pane->stack);
    pane->zone = target;
    if (existing_tabs == 0U) {
        pane->order = static_cast<std::uint8_t>(StackCount(target));
        pane->tab_order = 0U;
        pane->active_tab = true;
    } else {
        pane->tab_order = static_cast<std::uint8_t>(existing_tabs);
        pane->active_tab = false;
    }
    NormalizeOrders(target);
    if (target_was_empty) {
        static_cast<void>(SetZoneExtentDip(
            target,
            target == DockZone::TopContext || target == DockZone::Bottom
                ? descriptor->preferred_height_dip
                : descriptor->preferred_width_dip));
    }
    return DockResult::Ok;
}

DockResult DockLayoutModel::ResetPane(DockPaneType type) noexcept {
    DockPanePlacement* pane = Pane(type);
    if (pane == nullptr || !pane->present) {
        return DockResult::InvalidPane;
    }
    const DockZone old_zone = pane->zone;
    *pane = DefaultPlacement(type);
    NormalizeOrders(old_zone);
    NormalizeOrders(pane->zone);
    return DockResult::Ok;
}

DockResult DockLayoutModel::SetZoneMode(
    DockZone zone, DockStackMode mode) noexcept {
    DockZoneState* state = Zone(zone);
    if (state == nullptr || PaneCount(zone) == 0U
        || mode == DockStackMode::Mixed) {
        return DockResult::InvalidState;
    }
    if (state->mode == mode) {
        return DockResult::NoOp;
    }
    std::size_t count{};
    const auto ordered = OrderedPanes(*this, zone, count);
    if (mode == DockStackMode::Tabs) {
        const std::uint8_t stack = ordered[0]->stack;
        const std::uint32_t weight = ordered[0]->split_weight;
        DockPaneType active = state->active_tab;
        const bool active_present = std::any_of(
            ordered.begin(),
            ordered.begin() + static_cast<std::ptrdiff_t>(count),
            [active](const DockPanePlacement* pane) {
                return pane->type == active;
            });
        if (!active_present) {
            active = ordered[0]->type;
        }
        for (std::size_t index = 0U; index < count; ++index) {
            DockPanePlacement* pane = Pane(ordered[index]->type);
            pane->stack = stack;
            pane->order = 0U;
            pane->tab_order = static_cast<std::uint8_t>(index);
            pane->split_weight = weight;
            pane->active_tab = pane->type == active;
        }
    } else {
        for (std::size_t index = 0U; index < count; ++index) {
            DockPanePlacement* pane = Pane(ordered[index]->type);
            pane->stack = static_cast<std::uint8_t>(index);
            pane->order = static_cast<std::uint8_t>(index);
            pane->tab_order = 0U;
            pane->active_tab = true;
        }
    }
    NormalizeOrders(zone);
    return DockResult::Ok;
}

DockResult DockLayoutModel::SetActiveTab(
    DockZone zone, DockPaneType type) noexcept {
    DockZoneState* state = Zone(zone);
    const DockPanePlacement* pane = Pane(type);
    if (state == nullptr || pane == nullptr || !pane->present
        || pane->zone != zone || StackPaneCount(zone, pane->stack) < 2U) {
        return DockResult::InvalidState;
    }
    if (pane->active_tab) {
        return DockResult::NoOp;
    }
    for (DockPanePlacement& candidate : panes_) {
        if (candidate.present && candidate.zone == zone
            && candidate.stack == pane->stack) {
            candidate.active_tab = candidate.type == type;
        }
    }
    state->active_tab = type;
    return DockResult::Ok;
}

DockResult DockLayoutModel::SetZoneExtentDip(
    DockZone zone, int extent_dip) noexcept {
    DockZoneState* state = Zone(zone);
    if (state == nullptr) {
        return DockResult::InvalidState;
    }
    const int minimum = MinimumZoneExtent(*this, zone);
    const int maximum = zone == DockZone::TopContext || zone == DockZone::Bottom
        ? 480
        : 640;
    const int clamped = std::clamp(extent_dip, minimum, maximum);
    if (state->extent_dip == clamped) {
        return DockResult::NoOp;
    }
    state->extent_dip = clamped;
    return DockResult::Ok;
}

DockResult DockLayoutModel::AdjustSplitBoundary(
    DockZone zone,
    std::uint8_t boundary,
    int delta_milli) noexcept {
    std::size_t count{};
    const auto stacks = OrderedStacks(*this, zone, count);
    if (boundary + 1U >= count || delta_milli == 0) {
        return delta_milli == 0 ? DockResult::NoOp : DockResult::InvalidState;
    }
    const int combined = static_cast<int>(
        stacks[boundary].split_weight + stacks[boundary + 1U].split_weight);
    const int requested = static_cast<int>(stacks[boundary].split_weight)
        + delta_milli;
    const int adjusted = std::clamp(
        requested,
        static_cast<int>(kMinimumSplitWeight),
        combined - static_cast<int>(kMinimumSplitWeight));
    if (adjusted == static_cast<int>(stacks[boundary].split_weight)) {
        return DockResult::NoOp;
    }
    for (DockPanePlacement& pane : panes_) {
        if (!pane.present || pane.zone != zone) {
            continue;
        }
        if (pane.stack == stacks[boundary].id) {
            pane.split_weight = static_cast<std::uint32_t>(adjusted);
        } else if (pane.stack == stacks[boundary + 1U].id) {
            pane.split_weight = static_cast<std::uint32_t>(combined - adjusted);
        }
    }
    return DockResult::Ok;
}

DockResult DockLayoutModel::AdjustPaneBoundary(
    DockPaneType first,
    DockPaneType second,
    int delta_milli) noexcept {
    DockPanePlacement* first_pane = Pane(first);
    DockPanePlacement* second_pane = Pane(second);
    if (first_pane == nullptr || second_pane == nullptr
        || !first_pane->present || !second_pane->present
        || first_pane->zone != DockZone::Right
        || second_pane->zone != DockZone::Right) {
        return DockResult::InvalidState;
    }
    if (delta_milli == 0) {
        return DockResult::NoOp;
    }
    const int combined = static_cast<int>(
        first_pane->split_weight + second_pane->split_weight);
    const int adjusted = std::clamp(
        static_cast<int>(first_pane->split_weight) + delta_milli,
        static_cast<int>(kMinimumSplitWeight),
        combined - static_cast<int>(kMinimumSplitWeight));
    if (adjusted == static_cast<int>(first_pane->split_weight)) {
        return DockResult::NoOp;
    }
    first_pane->split_weight = static_cast<std::uint32_t>(adjusted);
    second_pane->split_weight = static_cast<std::uint32_t>(combined - adjusted);
    return DockResult::Ok;
}

bool DockLayoutModel::IsPaneVisible(DockPaneType type) const noexcept {
    const DockPanePlacement* pane = Pane(type);
    return pane != nullptr && pane->present && pane->zone != DockZone::Hidden;
}

bool DockLayoutModel::IsPaneDocked(DockPaneType type) const noexcept {
    const DockPanePlacement* pane = Pane(type);
    return pane != nullptr && pane->present && IsDockedZone(pane->zone);
}

bool DockLayoutModel::IsZoneAllowed(
    DockPaneType type, DockZone zone) const noexcept {
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    if (descriptor == nullptr
        || static_cast<std::size_t>(zone) >= static_cast<std::size_t>(DockZone::Count)) {
        return false;
    }
    if (zone == DockZone::AutoHide && !descriptor->can_auto_hide) {
        return false;
    }
    return (descriptor->allowed_zones & DockZoneBit(zone)) != 0U;
}

const DockPanePlacement* DockLayoutModel::Pane(DockPaneType type) const noexcept {
    const std::size_t index = PaneIndex(type);
    return index < panes_.size() ? &panes_[index] : nullptr;
}

DockPanePlacement* DockLayoutModel::Pane(DockPaneType type) noexcept {
    const std::size_t index = PaneIndex(type);
    return index < panes_.size() ? &panes_[index] : nullptr;
}

const DockZoneState* DockLayoutModel::Zone(DockZone zone) const noexcept {
    const std::size_t index = ZoneIndex(zone);
    return index < zones_.size() ? &zones_[index] : nullptr;
}

DockZoneState* DockLayoutModel::Zone(DockZone zone) noexcept {
    const std::size_t index = ZoneIndex(zone);
    return index < zones_.size() ? &zones_[index] : nullptr;
}

std::size_t DockLayoutModel::PaneCount(DockZone zone) const noexcept {
    return static_cast<std::size_t>(std::count_if(
        panes_.begin(), panes_.end(), [zone](const DockPanePlacement& pane) {
            return pane.present && pane.zone == zone;
        }));
}

std::size_t DockLayoutModel::StackCount(DockZone zone) const noexcept {
    std::array<bool, kDockPaneCount> seen{};
    std::size_t count{};
    for (const DockPanePlacement& pane : panes_) {
        if (!pane.present || pane.zone != zone || pane.stack >= seen.size()
            || seen[pane.stack]) {
            continue;
        }
        seen[pane.stack] = true;
        ++count;
    }
    return count;
}

std::size_t DockLayoutModel::StackPaneCount(
    DockZone zone, std::uint8_t stack) const noexcept {
    return static_cast<std::size_t>(std::count_if(
        panes_.begin(),
        panes_.end(),
        [zone, stack](const DockPanePlacement& pane) {
            return pane.present && pane.zone == zone && pane.stack == stack;
        }));
}

DockLayoutRecord DockLayoutModel::ToRecord() const noexcept {
    DockLayoutRecord record{};
    record.mirrored = mirrored_ ? 1U : 0U;
    record.panes = panes_;
    record.zones = zones_;
    return record;
}

bool DockLayoutModel::LoadRecord(const DockLayoutRecord& record) noexcept {
    if (record.version != 2U || record.pane_count != kDockPaneCount
        || record.mirrored > 1U) {
        return false;
    }
    std::array<bool, kDockPaneCount> seen{};
    for (const DockPanePlacement& pane : record.panes) {
        const std::size_t index = PaneIndex(pane.type);
        const PaneDescriptor* descriptor = FindPaneDescriptor(pane.type);
        if (index >= seen.size() || seen[index] || descriptor == nullptr
            || !pane.present || !IsZoneAllowed(pane.type, pane.zone)
            || !IsDockedZone(pane.restore_zone)
            || !IsZoneAllowed(pane.type, pane.restore_zone)
            || pane.order >= kDockPaneCount || pane.stack >= kDockPaneCount
            || pane.tab_order >= kDockPaneCount
            || pane.split_weight < kMinimumSplitWeight
            || pane.split_weight > kMaximumSplitWeight
            || !ValidFloatingPlacement(pane.floating, *descriptor)) {
            return false;
        }
        seen[index] = true;
    }
    for (std::size_t index = 0U; index < record.zones.size(); ++index) {
        const DockZone zone = static_cast<DockZone>(index);
        const DockZoneState& state = record.zones[index];
        std::array<bool, kDockPaneCount> stack_seen{};
        std::array<bool, kDockPaneCount> stack_orders{};
        std::array<std::uint8_t, kDockPaneCount> stack_order{};
        std::array<std::uint32_t, kDockPaneCount> stack_weight{};
        std::array<std::size_t, kDockPaneCount> stack_panes{};
        std::array<std::size_t, kDockPaneCount> stack_active{};
        std::array<std::array<bool, kDockPaneCount>, kDockPaneCount> tab_orders{};
        std::size_t pane_count{};
        bool active_tab_belongs_to_zone = state.active_tab == DockPaneType::Count;
        for (const DockPanePlacement& pane : record.panes) {
            if (pane.zone != zone) {
                continue;
            }
            ++pane_count;
            if (!stack_seen[pane.stack]) {
                if (stack_orders[pane.order]) {
                    return false;
                }
                stack_seen[pane.stack] = true;
                stack_orders[pane.order] = true;
                stack_order[pane.stack] = pane.order;
                stack_weight[pane.stack] = pane.split_weight;
            } else if (stack_order[pane.stack] != pane.order
                       || stack_weight[pane.stack] != pane.split_weight) {
                return false;
            }
            if (tab_orders[pane.stack][pane.tab_order]) {
                return false;
            }
            tab_orders[pane.stack][pane.tab_order] = true;
            ++stack_panes[pane.stack];
            stack_active[pane.stack] += pane.active_tab ? 1U : 0U;
            if (pane.type == state.active_tab) {
                active_tab_belongs_to_zone = true;
            }
        }
        std::size_t stack_count{};
        bool has_tab_stack{};
        for (std::size_t stack = 0U; stack < stack_seen.size(); ++stack) {
            if (!stack_seen[stack]) {
                continue;
            }
            ++stack_count;
            has_tab_stack = has_tab_stack || stack_panes[stack] > 1U;
            if (stack_active[stack] != 1U) {
                return false;
            }
            for (std::size_t tab = 0U; tab < stack_panes[stack]; ++tab) {
                if (!tab_orders[stack][tab]) {
                    return false;
                }
            }
        }
        for (std::size_t order = 0U; order < stack_count; ++order) {
            if (!stack_orders[order]) {
                return false;
            }
        }
        const DockStackMode expected_mode = stack_count == 1U && pane_count > 1U
            ? DockStackMode::Tabs
            : (stack_count > 1U && has_tab_stack ? DockStackMode::Mixed
                                                 : DockStackMode::Split);
        const int maximum_extent = zone == DockZone::TopContext
                || zone == DockZone::Bottom
            ? 480
            : 640;
        if (state.mode != expected_mode
            || state.extent_dip < MinimumZoneExtent(record, zone)
            || state.extent_dip > maximum_extent
            || (state.active_tab != DockPaneType::Count
                && PaneIndex(state.active_tab) >= kDockPaneCount)
            || !active_tab_belongs_to_zone
            || (pane_count > 0U && state.active_tab == DockPaneType::Count)) {
            return false;
        }
    }
    panes_ = record.panes;
    zones_ = record.zones;
    mirrored_ = record.mirrored != 0U;
    for (std::size_t index = 0U; index < kDockedZoneCount; ++index) {
        NormalizeOrders(static_cast<DockZone>(index));
    }
    return true;
}

void DockLayoutModel::NormalizeOrders(DockZone zone) noexcept {
    if (!IsDockedZone(zone)) {
        return;
    }
    struct MutableStack {
        std::uint8_t id{};
        std::uint8_t order{};
        std::array<DockPanePlacement*, kDockPaneCount> panes{};
        std::size_t pane_count{};
    };
    std::array<MutableStack, kDockPaneCount> stacks{};
    std::size_t stack_count{};
    for (DockPanePlacement& pane : panes_) {
        if (!pane.present || pane.zone != zone) {
            continue;
        }
        std::size_t stack_index{};
        while (stack_index < stack_count && stacks[stack_index].id != pane.stack) {
            ++stack_index;
        }
        if (stack_index == stack_count) {
            stacks[stack_count].id = pane.stack;
            stacks[stack_count].order = pane.order;
            ++stack_count;
        } else {
            stacks[stack_index].order = std::min(
                stacks[stack_index].order, pane.order);
        }
        MutableStack& stack = stacks[stack_index];
        stack.panes[stack.pane_count++] = &pane;
    }
    std::sort(
        stacks.begin(),
        stacks.begin() + static_cast<std::ptrdiff_t>(stack_count),
        [](const MutableStack& left, const MutableStack& right) {
            if (left.order != right.order) {
                return left.order < right.order;
            }
            return left.id < right.id;
        });
    DockZoneState* state = Zone(zone);
    if (state == nullptr) {
        return;
    }
    DockPaneType first_active = DockPaneType::Count;
    bool any_tab_stack{};
    bool remembered_active{};
    for (std::size_t stack_index = 0U; stack_index < stack_count; ++stack_index) {
        MutableStack& stack = stacks[stack_index];
        std::sort(
            stack.panes.begin(),
            stack.panes.begin()
                + static_cast<std::ptrdiff_t>(stack.pane_count),
            [](const DockPanePlacement* left, const DockPanePlacement* right) {
                if (left->tab_order != right->tab_order) {
                    return left->tab_order < right->tab_order;
                }
                return PaneIndex(left->type) < PaneIndex(right->type);
            });
        DockPanePlacement* active = nullptr;
        for (std::size_t tab_index = 0U; tab_index < stack.pane_count; ++tab_index) {
            DockPanePlacement* pane = stack.panes[tab_index];
            if (pane->type == state->active_tab) {
                active = pane;
            } else if (active == nullptr && pane->active_tab) {
                active = pane;
            }
        }
        if (active == nullptr) {
            active = stack.panes[0];
        }
        const std::uint32_t weight = stack.panes[0]->split_weight;
        for (std::size_t tab_index = 0U; tab_index < stack.pane_count; ++tab_index) {
            DockPanePlacement* pane = stack.panes[tab_index];
            pane->order = static_cast<std::uint8_t>(stack_index);
            pane->tab_order = static_cast<std::uint8_t>(tab_index);
            pane->split_weight = weight;
            pane->active_tab = pane == active;
        }
        any_tab_stack = any_tab_stack || stack.pane_count > 1U;
        if (first_active == DockPaneType::Count) {
            first_active = active->type;
        }
        remembered_active = remembered_active || active->type == state->active_tab;
    }
    if (!remembered_active) {
        state->active_tab = first_active;
    }
    state->mode = stack_count == 1U && any_tab_stack
        ? DockStackMode::Tabs
        : (stack_count > 1U && any_tab_stack ? DockStackMode::Mixed
                                             : DockStackMode::Split);
}

const std::array<PaneDescriptor, kDockPaneCount>& PaneDescriptors() noexcept {
    return kPaneDescriptors;
}

const PaneDescriptor* FindPaneDescriptor(DockPaneType type) noexcept {
    const std::size_t index = PaneIndex(type);
    return index < kPaneDescriptors.size() ? &kPaneDescriptors[index] : nullptr;
}

bool IsDockedZone(DockZone zone) noexcept {
    return ZoneIndex(zone) < kDockedZoneCount;
}

DockLayoutGeometry ComputeDockLayout(
    const DockLayoutModel& model,
    int width,
    int height,
    unsigned int dpi,
    const RightToolTabsModel* right_tool_tabs) noexcept {
    DockLayoutGeometry output{};
    for (std::size_t index = 0U; index < kDockPaneCount; ++index) {
        output.panes[index].type = static_cast<DockPaneType>(index);
    }
    width = std::max(0, width);
    height = std::max(0, height);
    const int splitter = std::max(1, ScaleDip(kSplitterDip, dpi));
    std::array<int, kDockedZoneCount> extents{};
    std::array<bool, kDockedZoneCount> active{};
    for (std::size_t index = 0U; index < kDockedZoneCount; ++index) {
        const DockZone zone = static_cast<DockZone>(index);
        const DockZoneState* state = model.Zone(zone);
        active[index] = zone == DockZone::Right && right_tool_tabs != nullptr
            ? right_tool_tabs->HasVisibleTabs()
            : model.PaneCount(zone) > 0U;
        if (active[index] && state != nullptr) {
            extents[index] = ScaleDip(
                std::max(state->extent_dip, MinimumZoneExtent(model, zone)), dpi);
        }
    }

    const int minimum_editor_height = ScaleDip(kMinimumEditorHeightDip, dpi);
    auto vertical_used = [&]() noexcept {
        int value{};
        for (const DockZone zone : {DockZone::TopContext, DockZone::Bottom}) {
            const std::size_t index = ZoneIndex(zone);
            if (active[index]) {
                value += extents[index] + splitter;
            }
        }
        return value;
    };
    for (const DockZone zone : {DockZone::Bottom, DockZone::TopContext}) {
        if (vertical_used() <= std::max(0, height - minimum_editor_height)) {
            break;
        }
        active[ZoneIndex(zone)] = false;
        MarkTemporaryAutoHide(output, model, zone);
    }

    int top = active[ZoneIndex(DockZone::TopContext)]
        ? extents[ZoneIndex(DockZone::TopContext)]
        : 0;
    int bottom = active[ZoneIndex(DockZone::Bottom)]
        ? extents[ZoneIndex(DockZone::Bottom)]
        : 0;
    const int top_gap = top > 0 ? splitter : 0;
    const int bottom_gap = bottom > 0 ? splitter : 0;
    const int body_y = top + top_gap;
    const int body_height = std::max(0, height - body_y - bottom - bottom_gap);

    if (top > 0) {
        output.zones[ZoneIndex(DockZone::TopContext)] = DockRect{0, 0, width, top};
        AddSplitter(
            output,
            DockSplitterKind::ZoneExtent,
            DockZone::TopContext,
            0U,
            DockRect{0, top, width, splitter});
    }
    if (bottom > 0) {
        const int y = height - bottom;
        output.zones[ZoneIndex(DockZone::Bottom)] = DockRect{0, y, width, bottom};
        AddSplitter(
            output,
            DockSplitterKind::ZoneExtent,
            DockZone::Bottom,
            0U,
            DockRect{0, y - splitter, width, splitter});
    }

    const int minimum_editor_width = ScaleDip(kMinimumEditorWidthDip, dpi);
    auto horizontal_used = [&]() noexcept {
        int value{};
        for (const DockZone zone : {DockZone::Left, DockZone::Right}) {
            const std::size_t index = ZoneIndex(zone);
            if (active[index]) {
                value += extents[index] + splitter;
            }
        }
        return value;
    };
    for (const DockZone zone : {DockZone::Right, DockZone::Left}) {
        if (horizontal_used() <= std::max(0, width - minimum_editor_width)) {
            break;
        }
        active[ZoneIndex(zone)] = false;
        MarkTemporaryAutoHide(output, model, zone);
    }

    const int left = active[ZoneIndex(DockZone::Left)]
        ? extents[ZoneIndex(DockZone::Left)]
        : 0;
    const int right = active[ZoneIndex(DockZone::Right)]
        ? extents[ZoneIndex(DockZone::Right)]
        : 0;
    const bool mirrored = model.Mirrored();
    const int physical_left = mirrored ? right : left;
    const int physical_right = mirrored ? left : right;
    const int physical_left_gap = physical_left > 0 ? splitter : 0;
    const int physical_right_gap = physical_right > 0 ? splitter : 0;
    output.editor = DockRect{
        physical_left + physical_left_gap,
        body_y,
        std::max(
            0,
            width - physical_left - physical_left_gap - physical_right
                - physical_right_gap),
        body_height};

    const auto place_side = [&](DockZone zone, bool on_left, int extent) noexcept {
        if (extent <= 0) {
            return;
        }
        const int x = on_left ? 0 : width - extent;
        output.zones[ZoneIndex(zone)] = DockRect{x, body_y, extent, body_height};
        const int split_x = on_left ? extent : x - splitter;
        AddSplitter(
            output,
            DockSplitterKind::ZoneExtent,
            zone,
            0U,
            DockRect{split_x, body_y, splitter, body_height});
    };
    place_side(DockZone::Left, !mirrored, left);
    place_side(DockZone::Right, mirrored, right);

    if (active[ZoneIndex(DockZone::Right)] && right_tool_tabs != nullptr) {
        DockRect& right_bounds = output.zones[ZoneIndex(DockZone::Right)];
        const int tab_height = std::min(
            right_bounds.height, ScaleDip(kToolTabHeightDip, dpi));
        output.right_tool_tabs = DockRect{
            right_bounds.x,
            right_bounds.y,
            right_bounds.width,
            tab_height};
        right_bounds.y += tab_height;
        right_bounds.height = std::max(0, right_bounds.height - tab_height);
    }

    for (std::size_t index = 0U; index < kDockedZoneCount; ++index) {
        const DockZone zone = static_cast<DockZone>(index);
        if (active[index]) {
            LayoutZone(
                output,
                model,
                zone,
                output.zones[index],
                splitter,
                dpi,
                right_tool_tabs);
        }
    }
    return output;
}

}  // namespace inkpod::windows::ui
