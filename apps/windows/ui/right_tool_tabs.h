#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>

#include "dock_layout.h"

namespace inkpod::windows::ui {

class ToolTabId final {
public:
    constexpr ToolTabId() noexcept = default;
    explicit constexpr ToolTabId(std::uint32_t value) noexcept : value_(value) {}

    [[nodiscard]] constexpr std::uint32_t Value() const noexcept {
        return value_;
    }
    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return value_ != 0U;
    }

    friend constexpr bool operator==(
        ToolTabId left, ToolTabId right) noexcept = default;

private:
    std::uint32_t value_{};
};

// A right-side tab can never outnumber the known pane descriptors because each
// tab is non-empty and a pane type belongs to at most one tab.
inline constexpr std::size_t kMaximumToolTabs = kDockPaneCount;
inline constexpr std::size_t kMaximumToolTabDescriptionLength = 512U;

enum class ToolTabResult : std::uint8_t {
    Ok,
    NoOp,
    InvalidTab,
    InvalidPane,
    CapacityExceeded,
};

struct ToolTab final {
    ToolTabId id{};
    std::array<DockPaneType, kDockPaneCount> panes{};
    std::size_t pane_count{};
};

class RightToolTabsModel final {
public:
    RightToolTabsModel() noexcept;

    void Reset() noexcept;

    [[nodiscard]] std::span<const ToolTab> Tabs() const noexcept;
    [[nodiscard]] const ToolTab* Find(ToolTabId id) const noexcept;
    [[nodiscard]] ToolTabId Selected() const noexcept { return selected_; }
    [[nodiscard]] const ToolTab* SelectedTab() const noexcept;
    [[nodiscard]] bool HasVisibleTabs() const noexcept {
        return tab_count_ != 0U;
    }
    [[nodiscard]] bool IsVisible(ToolTabId id) const noexcept {
        return Find(id) != nullptr;
    }
    [[nodiscard]] ToolTabId TabForPane(DockPaneType type) const noexcept;
    [[nodiscard]] std::uint32_t NextStableId() const noexcept {
        return next_id_;
    }

    [[nodiscard]] ToolTabResult SetSelected(ToolTabId id) noexcept;
    [[nodiscard]] ToolTabResult AddPaneToSelected(
        DockPaneType type,
        int available_height_px,
        unsigned int dpi,
        int splitter_px) noexcept;
    [[nodiscard]] ToolTabResult RemovePane(DockPaneType type) noexcept;
    [[nodiscard]] ToolTabResult MovePane(
        DockPaneType type, ToolTabId destination) noexcept;
    [[nodiscard]] ToolTabResult MovePaneToNewTab(DockPaneType type) noexcept;
    // Removes one complete tab and returns its pane membership in vertical
    // order. The caller can stage the corresponding pane visibility changes
    // before publishing both models together.
    [[nodiscard]] ToolTabResult CloseTab(
        ToolTabId id,
        std::span<DockPaneType> closed_panes,
        std::size_t& closed_count) noexcept;
    [[nodiscard]] ToolTabResult EnsurePaneAssigned(
        DockPaneType type) noexcept;
    [[nodiscard]] ToolTabResult ReorderPane(
        DockPaneType type,
        DockPaneType target,
        bool after_target) noexcept;
    [[nodiscard]] ToolTabResult Reorder(
        ToolTabId source, ToolTabId target, bool after_target) noexcept;

    // Used by the V9 decoder after it has validated counts and wire records.
    [[nodiscard]] bool Load(
        std::span<const ToolTab> tabs,
        ToolTabId selected,
        std::uint32_t next_id) noexcept;

private:
    [[nodiscard]] static bool EligiblePane(DockPaneType type) noexcept;
    [[nodiscard]] ToolTab* FindMutable(ToolTabId id) noexcept;
    [[nodiscard]] std::size_t IndexOf(ToolTabId id) const noexcept;
    [[nodiscard]] ToolTabResult CreateTab(DockPaneType type) noexcept;
    [[nodiscard]] ToolTabResult RemovePaneInPlace(DockPaneType type) noexcept;
    [[nodiscard]] ToolTabResult MovePaneInPlace(
        DockPaneType type, ToolTabId destination) noexcept;
    [[nodiscard]] bool FitsSelected(
        DockPaneType type,
        int available_height_px,
        unsigned int dpi,
        int splitter_px) const noexcept;
    void RemoveTab(std::size_t index) noexcept;

    std::array<ToolTab, kMaximumToolTabs> tabs_{};
    std::size_t tab_count_{};
    ToolTabId selected_{};
    std::uint32_t next_id_{1U};
};

}  // namespace inkpod::windows::ui
