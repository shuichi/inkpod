#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>
#include <string_view>

#include "dock_layout.h"
#include "ui/localization.h"

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

inline constexpr ToolTabId kToolTabColoring{UINT32_C(0x524c4f43)};
inline constexpr ToolTabId kToolTabReference{UINT32_C(0x45464552)};
inline constexpr ToolTabId kToolTabWorkflow{UINT32_C(0x574f4c46)};
inline constexpr std::size_t kMaximumToolTabs = 16U;
inline constexpr std::size_t kMaximumToolTabTitleLength = 64U;

enum class ToolTabResult : std::uint8_t {
    Ok,
    NoOp,
    InvalidTab,
    InvalidPane,
    CapacityExceeded,
};

struct ToolTab final {
    ToolTabId id{};
    UiStringId title_id{UiStringId::Count};
    std::array<wchar_t, kMaximumToolTabTitleLength> custom_title{};
    bool predefined{};
    bool visible{};
    std::array<DockPaneType, kDockPaneCount> panes{};
    std::size_t pane_count{};
};

[[nodiscard]] const wchar_t* ToolTabTitle(const ToolTab& tab) noexcept;

// Runtime source of truth for the right-side top-level tabs. Pane HWND
// visibility is a projection of this model plus DockLayoutModel; it never
// determines membership, tab visibility, selection, or ordering.
class RightToolTabsModel final {
public:
    RightToolTabsModel() noexcept;

    void Reset() noexcept;

    [[nodiscard]] std::span<const ToolTab> Tabs() const noexcept;
    [[nodiscard]] const ToolTab* Find(ToolTabId id) const noexcept;
    [[nodiscard]] ToolTabId Selected() const noexcept { return selected_; }
    [[nodiscard]] const ToolTab* SelectedTab() const noexcept;
    [[nodiscard]] bool HasVisibleTabs() const noexcept;
    [[nodiscard]] bool IsVisible(ToolTabId id) const noexcept;
    [[nodiscard]] ToolTabId TabForPane(DockPaneType type) const noexcept;

    [[nodiscard]] ToolTabResult SetSelected(ToolTabId id) noexcept;
    [[nodiscard]] ToolTabResult SetVisible(
        ToolTabId id, bool visible) noexcept;
    [[nodiscard]] ToolTabResult MovePane(
        DockPaneType type, ToolTabId destination) noexcept;
    [[nodiscard]] ToolTabResult EnsurePaneAssigned(
        DockPaneType type) noexcept;
    [[nodiscard]] ToolTabResult Reorder(
        ToolTabId source, ToolTabId target, bool after_target) noexcept;

private:
    [[nodiscard]] ToolTab* FindMutable(ToolTabId id) noexcept;
    [[nodiscard]] std::size_t IndexOf(ToolTabId id) const noexcept;
    void SelectReplacement(std::size_t hidden_index) noexcept;

    std::array<ToolTab, kMaximumToolTabs> tabs_{};
    std::size_t tab_count_{};
    ToolTabId selected_{};
};

}  // namespace inkpod::windows::ui
