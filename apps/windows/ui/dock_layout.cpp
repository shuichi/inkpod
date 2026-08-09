#include "dock_layout.h"

#include <algorithm>
#include <array>
#include <cstdint>
#include <initializer_list>

#include "app/resource.h"

namespace inkpod::windows::ui {
namespace {

constexpr int kReferenceDpi = 96;
constexpr int kSplitterDip = 4;
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

constexpr std::array<PaneDescriptor, kDockPaneCount> kPaneDescriptors{{
    {DockPaneType::Tool,
     UINT32_C(0x4c4f4f54),
     IDS_DOCK_PANE_TOOL,
     L"ツールパレット",
     DockZone::Left,
     DockedAndTransientZones({DockZone::Left, DockZone::Right}),
      PaneTargetScope::Application,
      1U,
      true,
      true,
      true,
      false,
     80,
     120,
     80,
     520,
     90U},
    {DockPaneType::ToolOptions,
     UINT32_C(0x54504f54),
     IDS_DOCK_PANE_TOOL_OPTIONS,
     L"ツールオプション",
     DockZone::TopContext,
     DockedAndTransientZones({DockZone::TopContext, DockZone::Bottom}),
      PaneTargetScope::FollowActiveView,
      1U,
      true,
      true,
      true,
      false,
     320,
     28,
     720,
     40,
     100U},
    {DockPaneType::Color,
     UINT32_C(0x524c4f43),
     IDS_DOCK_PANE_COLOR,
     L"カラー",
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}),
      PaneTargetScope::Application,
      1U,
      true,
      true,
      true,
      false,
     240,
     120,
     320,
     220,
     60U},
    {DockPaneType::Layer,
     UINT32_C(0x5259414c),
     IDS_DOCK_PANE_LAYER,
     L"レイヤー／プレーン",
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}),
      PaneTargetScope::FollowActiveView,
      1U,
      true,
      true,
      true,
      false,
     240,
     180,
     320,
      420,
      80U},
    {DockPaneType::Locator,
     UINT32_C(0x41434f4c),
     IDS_PANE_LOCATOR,
     L"ロケーター",
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
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
     L"シーケンス",
     DockZone::Bottom,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
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
     L"ライトテーブル",
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
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
     L"サブパレット／参照ビュー",
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
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
     L"バッチ",
     DockZone::Right,
     DockedAndTransientZones(
         {DockZone::Left, DockZone::Right, DockZone::Bottom}, true),
     PaneTargetScope::FollowActiveView,
     1U,
     false,
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
     L"処理進捗",
     DockZone::Bottom,
     DockedAndTransientZones({DockZone::Bottom}),
     PaneTargetScope::Job,
     1U,
     false,
     false,
     false,
     false,
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

DockPanePlacement DefaultPlacement(DockPaneType type) noexcept {
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    DockPanePlacement placement{};
    placement.type = type;
    placement.present = descriptor != nullptr;
    placement.zone = descriptor == nullptr || !descriptor->default_visible
        ? DockZone::Hidden
        : descriptor->default_zone;
    placement.restore_zone = descriptor == nullptr
        ? DockZone::Left
        : descriptor->default_zone;
    placement.order = type == DockPaneType::Layer ? 1U : 0U;
    placement.split_weight = type == DockPaneType::Color
        ? 320U
        : (type == DockPaneType::Layer ? 680U : 1000U);
    placement.floating = DefaultFloatingPlacement(type);
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
            return left->order < right->order;
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
    unsigned int dpi) noexcept {
    if (!HasArea(bounds)) {
        return;
    }
    std::size_t count{};
    const auto panes = OrderedPanes(model, zone, count);
    if (count == 0U) {
        return;
    }
    const DockZoneState* zone_state = model.Zone(zone);
    if (zone_state != nullptr && zone_state->mode == DockStackMode::Tabs) {
        DockPaneType active = zone_state->active_tab;
        const bool active_exists = std::any_of(
            panes.begin(),
            panes.begin() + static_cast<std::ptrdiff_t>(count),
            [active](const DockPanePlacement* pane) {
                return pane->type == active;
            });
        if (!active_exists) {
            active = panes[0]->type;
        }
        for (std::size_t index = 0U; index < count; ++index) {
            DockPaneGeometry& geometry = output.panes[PaneIndex(panes[index]->type)];
            geometry.bounds = bounds;
            geometry.shown = panes[index]->type == active;
        }
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
        const PaneDescriptor* descriptor = FindPaneDescriptor(panes[index]->type);
        minimums[index] = descriptor == nullptr
            ? 1
            : ScaleDip(
                  horizontal ? descriptor->minimum_width_dip
                             : descriptor->minimum_height_dip,
                  dpi);
        minimum_total += minimums[index];
        weight_total += panes[index]->split_weight;
    }
    int remaining = available;
    int remaining_minimum = minimum_total;
    std::uint64_t remaining_weight = std::max<std::uint64_t>(1U, weight_total);
    for (std::size_t index = 0U; index < count; ++index) {
        if (index + 1U == count) {
            sizes[index] = std::max(0, remaining);
            break;
        }
        const int raw = static_cast<int>(
            static_cast<std::int64_t>(remaining) * panes[index]->split_weight
            / static_cast<std::int64_t>(remaining_weight));
        const int minimum = available >= minimum_total ? minimums[index] : 0;
        const int remaining_after_minimum = available >= minimum_total
            ? remaining_minimum - minimums[index]
            : 0;
        const int maximum = std::max(minimum, remaining - remaining_after_minimum);
        sizes[index] = std::clamp(raw, minimum, maximum);
        remaining -= sizes[index];
        remaining_minimum -= minimums[index];
        remaining_weight = remaining_weight > panes[index]->split_weight
            ? remaining_weight - panes[index]->split_weight
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
        DockPaneGeometry& geometry = output.panes[PaneIndex(panes[index]->type)];
        geometry.bounds = pane_bounds;
        geometry.shown = HasArea(pane_bounds);
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
    pane->zone = zone;
    pane->restore_zone = zone;
    pane->order = static_cast<std::uint8_t>(PaneCount(zone));
    pane->split_weight = 1000U;
    NormalizeOrders(old_zone);
    NormalizeOrders(zone);
    DockZoneState* state = Zone(zone);
    if (state != nullptr && state->active_tab == DockPaneType::Count) {
        state->active_tab = type;
    }
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
    const DockResult moved = MovePane(type, destination->zone);
    if (moved != DockResult::Ok && moved != DockResult::NoOp) {
        return moved;
    }
    DockZoneState* zone = Zone(destination->zone);
    if (zone == nullptr) {
        return DockResult::InvalidState;
    }
    zone->mode = DockStackMode::Tabs;
    zone->active_tab = type;
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
    pane->zone = target;
    pane->order = static_cast<std::uint8_t>(PaneCount(target));
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
    pane->zone = target;
    pane->order = static_cast<std::uint8_t>(PaneCount(target));
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
    DockZoneState* zone = Zone(pane->zone);
    if (zone != nullptr) {
        zone->mode = DockStackMode::Split;
        zone->active_tab = type;
    }
    return DockResult::Ok;
}

DockResult DockLayoutModel::SetZoneMode(
    DockZone zone, DockStackMode mode) noexcept {
    DockZoneState* state = Zone(zone);
    if (state == nullptr || PaneCount(zone) == 0U) {
        return DockResult::InvalidState;
    }
    if (state->mode == mode) {
        return DockResult::NoOp;
    }
    state->mode = mode;
    if (mode == DockStackMode::Tabs) {
        std::size_t count{};
        const auto panes = OrderedPanes(*this, zone, count);
        state->active_tab = count == 0U ? DockPaneType::Count : panes[0]->type;
    }
    return DockResult::Ok;
}

DockResult DockLayoutModel::SetActiveTab(
    DockZone zone, DockPaneType type) noexcept {
    DockZoneState* state = Zone(zone);
    const DockPanePlacement* pane = Pane(type);
    if (state == nullptr || state->mode != DockStackMode::Tabs
        || pane == nullptr || !pane->present || pane->zone != zone) {
        return DockResult::InvalidState;
    }
    if (state->active_tab == type) {
        return DockResult::NoOp;
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
    const auto panes = OrderedPanes(*this, zone, count);
    if (boundary + 1U >= count || delta_milli == 0) {
        return delta_milli == 0 ? DockResult::NoOp : DockResult::InvalidState;
    }
    DockPanePlacement* first = Pane(panes[boundary]->type);
    DockPanePlacement* second = Pane(panes[boundary + 1U]->type);
    if (first == nullptr || second == nullptr) {
        return DockResult::InvalidState;
    }
    const int combined = static_cast<int>(first->split_weight + second->split_weight);
    const int requested = static_cast<int>(first->split_weight) + delta_milli;
    const int adjusted = std::clamp(
        requested,
        static_cast<int>(kMinimumSplitWeight),
        combined - static_cast<int>(kMinimumSplitWeight));
    if (adjusted == static_cast<int>(first->split_weight)) {
        return DockResult::NoOp;
    }
    first->split_weight = static_cast<std::uint32_t>(adjusted);
    second->split_weight = static_cast<std::uint32_t>(combined - adjusted);
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

DockLayoutRecord DockLayoutModel::ToRecord() const noexcept {
    DockLayoutRecord record{};
    record.mirrored = mirrored_ ? 1U : 0U;
    record.panes = panes_;
    record.zones = zones_;
    return record;
}

bool DockLayoutModel::LoadRecord(const DockLayoutRecord& record) noexcept {
    if (record.version != 1U || record.pane_count != kDockPaneCount
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
            || pane.order >= kDockPaneCount
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
        std::array<bool, kDockPaneCount> orders{};
        std::size_t pane_count{};
        bool active_tab_belongs_to_zone = state.active_tab == DockPaneType::Count;
        for (const DockPanePlacement& pane : record.panes) {
            if (pane.zone != zone) {
                continue;
            }
            ++pane_count;
            if (pane.order >= orders.size() || orders[pane.order]) {
                return false;
            }
            orders[pane.order] = true;
            if (pane.type == state.active_tab) {
                active_tab_belongs_to_zone = true;
            }
        }
        for (std::size_t order = 0U; order < pane_count; ++order) {
            if (!orders[order]) {
                return false;
            }
        }
        const int maximum_extent = zone == DockZone::TopContext
                || zone == DockZone::Bottom
            ? 480
            : 640;
        if ((state.mode != DockStackMode::Split && state.mode != DockStackMode::Tabs)
            || state.extent_dip < MinimumZoneExtent(record, zone)
            || state.extent_dip > maximum_extent
            || (state.active_tab != DockPaneType::Count
                && PaneIndex(state.active_tab) >= kDockPaneCount)
            || !active_tab_belongs_to_zone
            || (state.mode == DockStackMode::Tabs && pane_count > 0U
                && state.active_tab == DockPaneType::Count)) {
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
    std::array<DockPanePlacement*, kDockPaneCount> panes{};
    std::size_t count{};
    for (DockPanePlacement& pane : panes_) {
        if (pane.present && pane.zone == zone) {
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
    for (std::size_t index = 0U; index < count; ++index) {
        panes[index]->order = static_cast<std::uint8_t>(index);
    }
    DockZoneState* state = Zone(zone);
    if (state == nullptr) {
        return;
    }
    const bool active_present = std::any_of(
        panes.begin(),
        panes.begin() + static_cast<std::ptrdiff_t>(count),
        [state](const DockPanePlacement* pane) {
            return pane->type == state->active_tab;
        });
    if (!active_present) {
        state->active_tab = count == 0U ? DockPaneType::Count : panes[0]->type;
    }
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
    unsigned int dpi) noexcept {
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
        active[index] = model.PaneCount(zone) > 0U;
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

    for (std::size_t index = 0U; index < kDockedZoneCount; ++index) {
        const DockZone zone = static_cast<DockZone>(index);
        if (active[index]) {
            LayoutZone(
                output, model, zone, output.zones[index], splitter, dpi);
        }
    }
    return output;
}

}  // namespace inkpod::windows::ui
