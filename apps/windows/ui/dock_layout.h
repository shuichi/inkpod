#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

namespace inkpod::windows::ui {

enum class DockPaneType : std::uint8_t {
    Tool,
    ToolOptions,
    Color,
    Layer,
    Locator,
    Sequence,
    LightTable,
    Reference,
    Batch,
    JobProgress,
    Count,
};

enum class DockZone : std::uint8_t {
    TopContext,
    Left,
    Right,
    Bottom,
    Floating,
    Hidden,
    AutoHide,
    Count,
};

enum class PaneTargetScope : std::uint8_t {
    Application,
    FollowActiveView,
    PinnedDocument,
    Job,
};

enum class DockStackMode : std::uint8_t {
    Split,
    Tabs,
    Mixed,
};

enum class DockResult : std::uint8_t {
    Ok,
    NoOp,
    InvalidPane,
    DuplicatePane,
    ZoneNotAllowed,
    InvalidState,
};

inline constexpr std::size_t kDockPaneCount =
    static_cast<std::size_t>(DockPaneType::Count);
inline constexpr std::size_t kDockedZoneCount = 4U;
inline constexpr std::uint8_t kInvalidDockStack = UINT8_MAX;
inline constexpr std::size_t kMaximumDockSplitters =
    kDockedZoneCount + (kDockPaneCount - 1U) * kDockedZoneCount;
inline constexpr std::size_t kMaximumDockTabStacks =
    kDockedZoneCount * kDockPaneCount;

constexpr std::uint32_t DockZoneBit(DockZone zone) noexcept {
    return UINT32_C(1) << static_cast<std::uint8_t>(zone);
}

struct PaneDescriptor {
    DockPaneType type{};
    std::uint32_t stable_type_id{};
    std::uint32_t title_resource_id{};
    const wchar_t* fallback_title{};
    DockZone default_zone{};
    std::uint32_t allowed_zones{};
    PaneTargetScope scope{};
    std::uint8_t maximum_instances{1U};
    bool default_visible{true};
    bool persist_layout{true};
    bool can_float{};
    bool can_auto_hide{};
    int minimum_width_dip{};
    int minimum_height_dip{};
    int preferred_width_dip{};
    int preferred_height_dip{};
    std::uint8_t responsive_priority{};
};

struct DockFloatingPlacement {
    // These are 96-DPI reference pixels. DockHost performs the one and only
    // conversion to or from device pixels at the platform-window boundary.
    int x_dip{120};
    int y_dip{120};
    int width_dip{320};
    int height_dip{420};
};

struct DockPanePlacement {
    DockPaneType type{};
    DockZone zone{DockZone::Hidden};
    DockZone restore_zone{DockZone::Left};
    // A dock zone is a one-direction split of stacks. Panes with the same
    // stack value share one rectangle and appear as tabs within that rectangle.
    // order is the split-stack order; tab_order is local to the stack.
    std::uint8_t order{};
    std::uint8_t stack{};
    std::uint8_t tab_order{};
    std::uint32_t split_weight{1000U};
    DockFloatingPlacement floating{};
    bool present{};
    bool active_tab{true};
};

struct DockZoneState {
    DockStackMode mode{DockStackMode::Split};
    DockPaneType active_tab{DockPaneType::Count};
    int extent_dip{};
};

struct DockLayoutRecord {
    std::uint32_t version{2U};
    std::uint32_t pane_count{static_cast<std::uint32_t>(kDockPaneCount)};
    std::uint32_t mirrored{};
    std::array<DockPanePlacement, kDockPaneCount> panes{};
    std::array<DockZoneState, kDockedZoneCount> zones{};
};

struct DockRect {
    int x{};
    int y{};
    int width{};
    int height{};
};

enum class DockSplitterKind : std::uint8_t {
    ZoneExtent,
    StackBoundary,
};

struct DockPaneGeometry {
    DockPaneType type{};
    DockRect bounds{};
    bool shown{};
    // Narrow-window adaptation never mutates DockLayoutModel. A suppressed
    // pane remains logically visible and can reappear when space returns.
    bool temporarily_auto_hidden{};
};

struct DockSplitterGeometry {
    DockSplitterKind kind{};
    DockZone zone{};
    std::uint8_t boundary{};
    DockRect bounds{};
};

struct DockLayoutGeometry {
    DockRect editor{};
    std::array<DockRect, kDockedZoneCount> zones{};
    std::array<DockPaneGeometry, kDockPaneCount> panes{};
    std::array<DockSplitterGeometry, kMaximumDockSplitters> splitters{};
    std::size_t splitter_count{};
};

class DockLayoutModel final {
public:
    DockLayoutModel() noexcept;

    void Reset() noexcept;
    [[nodiscard]] DockResult AddPane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult RemovePane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult MovePane(DockPaneType type, DockZone zone) noexcept;
    [[nodiscard]] DockResult TabPane(
        DockPaneType type, DockPaneType target) noexcept;
    [[nodiscard]] DockResult FloatPane(
        DockPaneType type, const DockFloatingPlacement& placement) noexcept;
    [[nodiscard]] DockResult HidePane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult SetPaneAutoHide(
        DockPaneType type, bool auto_hide) noexcept;
    [[nodiscard]] DockResult RestorePane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult ResetPane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult SetZoneMode(
        DockZone zone, DockStackMode mode) noexcept;
    [[nodiscard]] DockResult SetActiveTab(
        DockZone zone, DockPaneType type) noexcept;
    [[nodiscard]] DockResult SetZoneExtentDip(
        DockZone zone, int extent_dip) noexcept;
    [[nodiscard]] DockResult AdjustSplitBoundary(
        DockZone zone,
        std::uint8_t boundary,
        int delta_milli) noexcept;

    void SetMirrored(bool mirrored) noexcept { mirrored_ = mirrored; }
    [[nodiscard]] bool Mirrored() const noexcept { return mirrored_; }
    [[nodiscard]] bool IsPaneVisible(DockPaneType type) const noexcept;
    [[nodiscard]] bool IsPaneDocked(DockPaneType type) const noexcept;
    [[nodiscard]] bool IsZoneAllowed(
        DockPaneType type, DockZone zone) const noexcept;
    [[nodiscard]] const DockPanePlacement* Pane(
        DockPaneType type) const noexcept;
    [[nodiscard]] DockPanePlacement* Pane(DockPaneType type) noexcept;
    [[nodiscard]] const DockZoneState* Zone(DockZone zone) const noexcept;
    [[nodiscard]] DockZoneState* Zone(DockZone zone) noexcept;
    [[nodiscard]] std::size_t PaneCount(DockZone zone) const noexcept;
    [[nodiscard]] std::size_t StackCount(DockZone zone) const noexcept;
    [[nodiscard]] std::size_t StackPaneCount(
        DockZone zone, std::uint8_t stack) const noexcept;
    [[nodiscard]] DockLayoutRecord ToRecord() const noexcept;
    [[nodiscard]] bool LoadRecord(const DockLayoutRecord& record) noexcept;

private:
    void NormalizeOrders(DockZone zone) noexcept;

    std::array<DockPanePlacement, kDockPaneCount> panes_{};
    std::array<DockZoneState, kDockedZoneCount> zones_{};
    bool mirrored_{};
};

[[nodiscard]] const std::array<PaneDescriptor, kDockPaneCount>&
PaneDescriptors() noexcept;
[[nodiscard]] const PaneDescriptor* FindPaneDescriptor(
    DockPaneType type) noexcept;
[[nodiscard]] bool IsDockedZone(DockZone zone) noexcept;
[[nodiscard]] DockLayoutGeometry ComputeDockLayout(
    const DockLayoutModel& model,
    int width,
    int height,
    unsigned int dpi) noexcept;

}  // namespace inkpod::windows::ui
