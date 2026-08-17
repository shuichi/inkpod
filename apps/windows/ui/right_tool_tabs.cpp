#include "right_tool_tabs.h"

#include <algorithm>

namespace inkpod::windows::ui {
namespace {

constexpr std::size_t PaneIndex(DockPaneType type) noexcept {
    return static_cast<std::size_t>(type);
}

void AppendPane(ToolTab& tab, DockPaneType type) noexcept {
    if (tab.pane_count < tab.panes.size()) {
        tab.panes[tab.pane_count++] = type;
    }
}

}  // namespace

RightToolTabsModel::RightToolTabsModel() noexcept {
    Reset();
}

void RightToolTabsModel::Reset() noexcept {
    tabs_ = {};
    tab_count_ = 3U;
    tabs_[0].id = kToolTabColoring;
    tabs_[0].title_id = UiStringId::RightToolTabColoring;
    tabs_[0].predefined = true;
    tabs_[0].visible = true;
    AppendPane(tabs_[0], DockPaneType::Tool);
    AppendPane(tabs_[0], DockPaneType::Color);
    AppendPane(tabs_[0], DockPaneType::Layer);

    tabs_[1].id = kToolTabReference;
    tabs_[1].title_id = UiStringId::RightToolTabReference;
    tabs_[1].predefined = true;
    tabs_[1].visible = true;
    AppendPane(tabs_[1], DockPaneType::Locator);
    AppendPane(tabs_[1], DockPaneType::LightTable);
    AppendPane(tabs_[1], DockPaneType::Reference);

    tabs_[2].id = kToolTabWorkflow;
    tabs_[2].title_id = UiStringId::RightToolTabWorkflow;
    tabs_[2].predefined = true;
    tabs_[2].visible = true;
    AppendPane(tabs_[2], DockPaneType::Sequence);
    AppendPane(tabs_[2], DockPaneType::Batch);
    selected_ = kToolTabColoring;
}

const wchar_t* ToolTabTitle(const ToolTab& tab) noexcept {
    if (tab.custom_title[0] != L'\0') {
        return tab.custom_title.data();
    }
    return tab.title_id == UiStringId::Count ? L"" : UiText(tab.title_id);
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
    const ToolTab* tab = Find(selected_);
    return tab != nullptr && tab->visible ? tab : nullptr;
}

bool RightToolTabsModel::HasVisibleTabs() const noexcept {
    return std::any_of(
        tabs_.begin(),
        tabs_.begin() + static_cast<std::ptrdiff_t>(tab_count_),
        [](const ToolTab& tab) { return tab.visible; });
}

bool RightToolTabsModel::IsVisible(ToolTabId id) const noexcept {
    const ToolTab* tab = Find(id);
    return tab != nullptr && tab->visible;
}

ToolTabId RightToolTabsModel::TabForPane(DockPaneType type) const noexcept {
    if (PaneIndex(type) >= kDockPaneCount) {
        return {};
    }
    for (std::size_t tab_index = 0U; tab_index < tab_count_; ++tab_index) {
        const ToolTab& tab = tabs_[tab_index];
        for (std::size_t pane_index = 0U;
             pane_index < tab.pane_count;
             ++pane_index) {
            if (tab.panes[pane_index] == type) {
                return tab.id;
            }
        }
    }
    return {};
}

ToolTabResult RightToolTabsModel::SetSelected(ToolTabId id) noexcept {
    const ToolTab* tab = Find(id);
    if (tab == nullptr || !tab->visible) {
        return ToolTabResult::InvalidTab;
    }
    if (selected_ == id) {
        return ToolTabResult::NoOp;
    }
    selected_ = id;
    return ToolTabResult::Ok;
}

void RightToolTabsModel::SelectReplacement(
    std::size_t hidden_index) noexcept {
    selected_ = {};
    for (std::size_t offset = 1U; offset <= tab_count_; ++offset) {
        const std::size_t index = (hidden_index + offset) % tab_count_;
        if (tabs_[index].visible) {
            selected_ = tabs_[index].id;
            return;
        }
    }
}

ToolTabResult RightToolTabsModel::SetVisible(
    ToolTabId id, bool visible) noexcept {
    const std::size_t index = IndexOf(id);
    if (index >= tab_count_) {
        return ToolTabResult::InvalidTab;
    }
    ToolTab& tab = tabs_[index];
    if (tab.visible == visible) {
        return ToolTabResult::NoOp;
    }
    tab.visible = visible;
    if (!visible && selected_ == id) {
        SelectReplacement(index);
    } else if (visible && !selected_) {
        selected_ = id;
    }
    return ToolTabResult::Ok;
}

ToolTabResult RightToolTabsModel::MovePane(
    DockPaneType type, ToolTabId destination) noexcept {
    if (PaneIndex(type) >= kDockPaneCount) {
        return ToolTabResult::InvalidPane;
    }
    ToolTab* target = FindMutable(destination);
    if (target == nullptr) {
        return ToolTabResult::InvalidTab;
    }
    if (TabForPane(type) == destination) {
        return ToolTabResult::NoOp;
    }
    if (target->pane_count >= target->panes.size()) {
        return ToolTabResult::CapacityExceeded;
    }
    for (std::size_t tab_index = 0U; tab_index < tab_count_; ++tab_index) {
        ToolTab& tab = tabs_[tab_index];
        for (std::size_t pane_index = 0U;
             pane_index < tab.pane_count;
             ++pane_index) {
            if (tab.panes[pane_index] != type) {
                continue;
            }
            std::move(
                tab.panes.begin() + static_cast<std::ptrdiff_t>(pane_index + 1U),
                tab.panes.begin() + static_cast<std::ptrdiff_t>(tab.pane_count),
                tab.panes.begin() + static_cast<std::ptrdiff_t>(pane_index));
            --tab.pane_count;
            break;
        }
    }
    target->panes[target->pane_count++] = type;
    return ToolTabResult::Ok;
}

ToolTabResult RightToolTabsModel::EnsurePaneAssigned(
    DockPaneType type) noexcept {
    if (PaneIndex(type) >= kDockPaneCount) {
        return ToolTabResult::InvalidPane;
    }
    if (TabForPane(type)) {
        return ToolTabResult::NoOp;
    }
    ToolTabId destination = selected_;
    if (!destination && tab_count_ > 0U) {
        destination = tabs_[0].id;
    }
    return MovePane(type, destination);
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
    if (source_index < target_index) {
        const std::size_t final_index = target_index - (after_target ? 0U : 1U);
        if (final_index == source_index) {
            return ToolTabResult::NoOp;
        }
        std::rotate(
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index),
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index + 1U),
            tabs_.begin() + static_cast<std::ptrdiff_t>(final_index + 1U));
    } else {
        const std::size_t final_index = target_index + (after_target ? 1U : 0U);
        if (final_index == source_index) {
            return ToolTabResult::NoOp;
        }
        std::rotate(
            tabs_.begin() + static_cast<std::ptrdiff_t>(final_index),
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index),
            tabs_.begin() + static_cast<std::ptrdiff_t>(source_index + 1U));
    }
    return ToolTabResult::Ok;
}

}  // namespace inkpod::windows::ui
