#include "right_tool_tabs.h"

#include <algorithm>
#include <cwchar>
#include <limits>

namespace inkpod::windows::ui {
namespace {

constexpr std::size_t PaneIndex(DockPaneType type) noexcept {
    return static_cast<std::size_t>(type);
}

int ScaleDip(int value, unsigned int dpi) noexcept {
    const unsigned int normalized = dpi == 0U ? 96U : dpi;
    const std::int64_t scaled = static_cast<std::int64_t>(value)
        * static_cast<std::int64_t>(normalized);
    return static_cast<int>(std::clamp<std::int64_t>(
        (scaled + 48) / 96,
        0,
        std::numeric_limits<int>::max()));
}

const wchar_t* PaneTitle(DockPaneType type) noexcept {
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    return descriptor == nullptr || descriptor->fallback_title == nullptr
        ? L""
        : descriptor->fallback_title;
}

constexpr bool RequiresExclusiveTab(DockPaneType type) noexcept {
    return type == DockPaneType::Batch;
}

bool ContainsExclusivePane(const ToolTab& tab) noexcept {
    return std::find(
               tab.panes.begin(),
               tab.panes.begin() + static_cast<std::ptrdiff_t>(tab.pane_count),
               DockPaneType::Batch)
        != tab.panes.begin() + static_cast<std::ptrdiff_t>(tab.pane_count);
}

}  // namespace

RightToolTabsModel::RightToolTabsModel() noexcept {
    Reset();
}

void RightToolTabsModel::Reset() noexcept {
    tabs_ = {};
    tab_count_ = 1U;
    tabs_[0].id = ToolTabId{1U};
    tabs_[0].panes[0] = DockPaneType::Color;
    tabs_[0].panes[1] = DockPaneType::Layer;
    tabs_[0].pane_count = 2U;
    selected_ = tabs_[0].id;
    next_id_ = 2U;
}

const wchar_t* ToolTabTitle(const ToolTab& tab) noexcept {
    return tab.pane_count == 0U ? L"" : PaneTitle(tab.panes[0]);
}

bool ToolTabDescription(
    const ToolTab& tab,
    std::span<wchar_t> output) noexcept {
    if (tab.pane_count == 0U || output.empty()) {
        return false;
    }
    std::size_t written{};
    for (std::size_t index = 0U; index < tab.pane_count; ++index) {
        const wchar_t* title = PaneTitle(tab.panes[index]);
        const std::size_t length = std::wcslen(title);
        const std::size_t separator = index == 0U ? 0U : 2U;
        if (length + separator >= output.size() - written) {
            output[0] = L'\0';
            return false;
        }
        if (separator != 0U) {
            output[written++] = L',';
            output[written++] = L' ';
        }
        std::copy_n(title, length, output.begin() + written);
        written += length;
    }
    output[written] = L'\0';
    return true;
}

std::span<const ToolTab> RightToolTabsModel::Tabs() const noexcept {
    return std::span<const ToolTab>(tabs_.data(), tab_count_);
}

const ToolTab* RightToolTabsModel::Find(ToolTabId id) const noexcept {
    const std::size_t index = IndexOf(id);
    return index < tab_count_ ? &tabs_[index] : nullptr;
}

ToolTab* RightToolTabsModel::FindMutable(ToolTabId id) noexcept {
    const std::size_t index = IndexOf(id);
    return index < tab_count_ ? &tabs_[index] : nullptr;
}

std::size_t RightToolTabsModel::IndexOf(ToolTabId id) const noexcept {
    for (std::size_t index = 0U; index < tab_count_; ++index) {
        if (tabs_[index].id == id) {
            return index;
        }
    }
    return tab_count_;
}

const ToolTab* RightToolTabsModel::SelectedTab() const noexcept {
    return Find(selected_);
}

ToolTabId RightToolTabsModel::TabForPane(DockPaneType type) const noexcept {
    if (!EligiblePane(type)) {
        return {};
    }
    for (std::size_t tab_index = 0U; tab_index < tab_count_; ++tab_index) {
        const ToolTab& tab = tabs_[tab_index];
        const auto end = tab.panes.begin()
            + static_cast<std::ptrdiff_t>(tab.pane_count);
        if (std::find(tab.panes.begin(), end, type) != end) {
            return tab.id;
        }
    }
    return {};
}

ToolTabResult RightToolTabsModel::SetSelected(ToolTabId id) noexcept {
    if (Find(id) == nullptr) {
        return ToolTabResult::InvalidTab;
    }
    if (selected_ == id) {
        return ToolTabResult::NoOp;
    }
    selected_ = id;
    return ToolTabResult::Ok;
}

bool RightToolTabsModel::EligiblePane(DockPaneType type) noexcept {
    if (type == DockPaneType::Tool || type == DockPaneType::ToolOptions
        || type == DockPaneType::JobProgress
        || PaneIndex(type) >= kDockPaneCount) {
        return false;
    }
    const PaneDescriptor* descriptor = FindPaneDescriptor(type);
    return descriptor != nullptr
        && (descriptor->allowed_zones & DockZoneBit(DockZone::Right)) != 0U;
}

bool RightToolTabsModel::FitsSelected(
    DockPaneType type,
    int available_height_px,
    unsigned int dpi,
    int splitter_px) const noexcept {
    const ToolTab* selected = SelectedTab();
    const PaneDescriptor* added = FindPaneDescriptor(type);
    if (selected == nullptr || added == nullptr || available_height_px < 0
        || splitter_px < 0) {
        return false;
    }
    std::int64_t minimum = ScaleDip(added->minimum_height_dip, dpi);
    for (std::size_t index = 0U; index < selected->pane_count; ++index) {
        const PaneDescriptor* descriptor = FindPaneDescriptor(
            selected->panes[index]);
        if (descriptor == nullptr) {
            return false;
        }
        minimum += ScaleDip(descriptor->minimum_height_dip, dpi);
    }
    minimum += static_cast<std::int64_t>(splitter_px)
        * static_cast<std::int64_t>(selected->pane_count);
    return minimum <= available_height_px;
}

ToolTabResult RightToolTabsModel::CreateTab(DockPaneType type) noexcept {
    if (!EligiblePane(type)) {
        return ToolTabResult::InvalidPane;
    }
    if (tab_count_ >= tabs_.size() || next_id_ == 0U
        || next_id_ == std::numeric_limits<std::uint32_t>::max()) {
        return ToolTabResult::CapacityExceeded;
    }
    ToolTab& tab = tabs_[tab_count_++];
    tab = {};
    tab.id = ToolTabId{next_id_++};
    tab.panes[0] = type;
    tab.pane_count = 1U;
    selected_ = tab.id;
    return ToolTabResult::Ok;
}

ToolTabResult RightToolTabsModel::AddPaneToSelected(
    DockPaneType type,
    int available_height_px,
    unsigned int dpi,
    int splitter_px) noexcept {
    if (!EligiblePane(type)) {
        return ToolTabResult::InvalidPane;
    }
    const ToolTabId existing = TabForPane(type);
    if (existing) {
        const ToolTabResult selection = SetSelected(existing);
        return selection == ToolTabResult::InvalidTab
            ? selection
            : ToolTabResult::NoOp;
    }
    const ToolTab* selected_tab = SelectedTab();
    if (RequiresExclusiveTab(type)
        || (selected_tab != nullptr && ContainsExclusivePane(*selected_tab))) {
        return CreateTab(type);
    }
    if (selected_ && FitsSelected(
            type, available_height_px, dpi, splitter_px)) {
        ToolTab* selected_mutable = FindMutable(selected_);
        if (selected_mutable == nullptr
            || selected_mutable->pane_count >= selected_mutable->panes.size()) {
            return ToolTabResult::CapacityExceeded;
        }
        selected_mutable->panes[selected_mutable->pane_count++] = type;
        return ToolTabResult::Ok;
    }
    return CreateTab(type);
}

void RightToolTabsModel::RemoveTab(std::size_t index) noexcept {
    if (index >= tab_count_) {
        return;
    }
    const bool was_selected = tabs_[index].id == selected_;
    std::move(
        tabs_.begin() + static_cast<std::ptrdiff_t>(index + 1U),
        tabs_.begin() + static_cast<std::ptrdiff_t>(tab_count_),
        tabs_.begin() + static_cast<std::ptrdiff_t>(index));
    tabs_[--tab_count_] = {};
    if (!was_selected) {
        return;
    }
    if (tab_count_ == 0U) {
        selected_ = {};
    } else if (index > 0U) {
        selected_ = tabs_[index - 1U].id;
    } else if (index < tab_count_) {
        selected_ = tabs_[index].id;
    } else {
        selected_ = tabs_[0].id;
    }
}

ToolTabResult RightToolTabsModel::RemovePaneInPlace(
    DockPaneType type) noexcept {
    for (std::size_t tab_index = 0U; tab_index < tab_count_; ++tab_index) {
        ToolTab& tab = tabs_[tab_index];
        const auto end = tab.panes.begin()
            + static_cast<std::ptrdiff_t>(tab.pane_count);
        const auto found = std::find(tab.panes.begin(), end, type);
        if (found == end) {
            continue;
        }
        std::move(found + 1, end, found);
        tab.panes[--tab.pane_count] = DockPaneType::Tool;
        if (tab.pane_count == 0U) {
            RemoveTab(tab_index);
        }
        return ToolTabResult::Ok;
    }
    return ToolTabResult::NoOp;
}

ToolTabResult RightToolTabsModel::RemovePane(DockPaneType type) noexcept {
    if (!EligiblePane(type)) {
        return ToolTabResult::InvalidPane;
    }
    return RemovePaneInPlace(type);
}

ToolTabResult RightToolTabsModel::MovePaneInPlace(
    DockPaneType type, ToolTabId destination) noexcept {
    ToolTab* target = FindMutable(destination);
    if (target == nullptr) {
        return ToolTabResult::InvalidTab;
    }
    if (TabForPane(type) == destination) {
        return ToolTabResult::NoOp;
    }
    if (RequiresExclusiveTab(type) || ContainsExclusivePane(*target)) {
        return ToolTabResult::InvalidTab;
    }
    if (target->pane_count >= target->panes.size()) {
        return ToolTabResult::CapacityExceeded;
    }
    static_cast<void>(RemovePaneInPlace(type));
    target = FindMutable(destination);
    if (target == nullptr) {
        return ToolTabResult::InvalidTab;
    }
    target->panes[target->pane_count++] = type;
    selected_ = destination;
    return ToolTabResult::Ok;
}

ToolTabResult RightToolTabsModel::MovePane(
    DockPaneType type, ToolTabId destination) noexcept {
    if (!EligiblePane(type)) {
        return ToolTabResult::InvalidPane;
    }
    RightToolTabsModel candidate = *this;
    const ToolTabResult result = candidate.MovePaneInPlace(type, destination);
    if (result == ToolTabResult::Ok) {
        *this = candidate;
    }
    return result;
}

ToolTabResult RightToolTabsModel::MovePaneToNewTab(
    DockPaneType type) noexcept {
    if (!EligiblePane(type)) {
        return ToolTabResult::InvalidPane;
    }
    const ToolTab* source = Find(TabForPane(type));
    if (source != nullptr && source->pane_count == 1U) {
        return ToolTabResult::NoOp;
    }
    RightToolTabsModel candidate = *this;
    static_cast<void>(candidate.RemovePaneInPlace(type));
    const ToolTabResult result = candidate.CreateTab(type);
    if (result == ToolTabResult::Ok) {
        *this = candidate;
    }
    return result;
}

ToolTabResult RightToolTabsModel::EnsurePaneAssigned(
    DockPaneType type) noexcept {
    if (!EligiblePane(type)) {
        return ToolTabResult::InvalidPane;
    }
    if (const ToolTabId existing = TabForPane(type); existing) {
        const ToolTabResult selection = SetSelected(existing);
        return selection == ToolTabResult::InvalidTab
            ? selection
            : ToolTabResult::NoOp;
    }
    ToolTab* selected = FindMutable(selected_);
    if (RequiresExclusiveTab(type)
        || (selected != nullptr && ContainsExclusivePane(*selected))) {
        return CreateTab(type);
    }
    if (selected != nullptr && selected->pane_count < selected->panes.size()) {
        selected->panes[selected->pane_count++] = type;
        return ToolTabResult::Ok;
    }
    return CreateTab(type);
}

ToolTabResult RightToolTabsModel::ReorderPane(
    DockPaneType type,
    DockPaneType target,
    bool after_target) noexcept {
    const ToolTabId source_tab = TabForPane(type);
    const ToolTabId target_tab = TabForPane(target);
    if (!source_tab || source_tab != target_tab) {
        return ToolTabResult::InvalidPane;
    }
    ToolTab* tab = FindMutable(source_tab);
    const auto end = tab->panes.begin()
        + static_cast<std::ptrdiff_t>(tab->pane_count);
    const auto source_it = std::find(tab->panes.begin(), end, type);
    const auto target_it = std::find(tab->panes.begin(), end, target);
    if (source_it == target_it) {
        return ToolTabResult::NoOp;
    }
    const std::size_t source_index = static_cast<std::size_t>(
        source_it - tab->panes.begin());
    const std::size_t target_index = static_cast<std::size_t>(
        target_it - tab->panes.begin());
    std::size_t final_index = target_index + (after_target ? 1U : 0U);
    if (source_index < final_index) {
        --final_index;
    }
    if (source_index == final_index) {
        return ToolTabResult::NoOp;
    }
    const DockPaneType value = tab->panes[source_index];
    if (source_index < final_index) {
        std::move(
            tab->panes.begin() + static_cast<std::ptrdiff_t>(source_index + 1U),
            tab->panes.begin() + static_cast<std::ptrdiff_t>(final_index + 1U),
            tab->panes.begin() + static_cast<std::ptrdiff_t>(source_index));
    } else {
        std::move_backward(
            tab->panes.begin() + static_cast<std::ptrdiff_t>(final_index),
            tab->panes.begin() + static_cast<std::ptrdiff_t>(source_index),
            tab->panes.begin() + static_cast<std::ptrdiff_t>(source_index + 1U));
    }
    tab->panes[final_index] = value;
    return ToolTabResult::Ok;
}

ToolTabResult RightToolTabsModel::Reorder(
    ToolTabId source, ToolTabId target, bool after_target) noexcept {
    const std::size_t source_index = IndexOf(source);
    const std::size_t target_index = IndexOf(target);
    if (source_index >= tab_count_ || target_index >= tab_count_) {
        return ToolTabResult::InvalidTab;
    }
    if (source_index == target_index) {
        return ToolTabResult::NoOp;
    }
    std::size_t final_index = target_index + (after_target ? 1U : 0U);
    if (source_index < final_index) {
        --final_index;
    }
    if (source_index == final_index) {
        return ToolTabResult::NoOp;
    }
    const ToolTab value = tabs_[source_index];
    if (source_index < final_index) {
        std::move(
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index + 1U),
            tabs_.begin() + static_cast<std::ptrdiff_t>(final_index + 1U),
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index));
    } else {
        std::move_backward(
            tabs_.begin() + static_cast<std::ptrdiff_t>(final_index),
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index),
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index + 1U));
    }
    tabs_[final_index] = value;
    return ToolTabResult::Ok;
}

bool RightToolTabsModel::Load(
    std::span<const ToolTab> tabs,
    ToolTabId selected,
    std::uint32_t next_id) noexcept {
    if (tabs.size() > tabs_.size() || next_id == 0U
        || (tabs.empty() && selected)
        || (!tabs.empty() && !selected)) {
        return false;
    }
    if (tabs.empty()) {
        tabs_ = {};
        tab_count_ = 0U;
        selected_ = {};
        next_id_ = next_id;
        return true;
    }
    std::array<bool, kDockPaneCount> seen_panes{};
    std::array<std::uint32_t, kMaximumToolTabs> seen_ids{};
    std::size_t seen_id_count{};
    std::uint32_t maximum_id{};
    bool selected_seen{};
    for (const ToolTab& tab : tabs) {
        if (!tab.id || tab.pane_count == 0U
            || tab.pane_count > tab.panes.size()
            || (ContainsExclusivePane(tab) && tab.pane_count != 1U)) {
            return false;
        }
        const auto id_end = seen_ids.begin()
            + static_cast<std::ptrdiff_t>(seen_id_count);
        if (std::find(seen_ids.begin(), id_end, tab.id.Value()) != id_end) {
            return false;
        }
        seen_ids[seen_id_count++] = tab.id.Value();
        maximum_id = std::max(maximum_id, tab.id.Value());
        selected_seen = selected_seen || tab.id == selected;
        for (std::size_t index = 0U; index < tab.pane_count; ++index) {
            const DockPaneType type = tab.panes[index];
            const std::size_t pane_index = PaneIndex(type);
            if (!EligiblePane(type) || pane_index >= seen_panes.size()
                || seen_panes[pane_index]) {
                return false;
            }
            seen_panes[pane_index] = true;
        }
    }
    if (!selected_seen || next_id <= maximum_id) {
        return false;
    }
    tabs_ = {};
    std::copy(tabs.begin(), tabs.end(), tabs_.begin());
    tab_count_ = tabs.size();
    selected_ = selected;
    next_id_ = next_id;
    return true;
}

}  // namespace inkpod::windows::ui
