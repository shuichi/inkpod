#include "editor_area.h"

#include <algorithm>

namespace inkpod::app {

bool EditorGroup::AddView(DocumentViewId view) noexcept {
    if (!view || Contains(view) || view_count_ >= views_.size()) {
        return false;
    }
    views_[view_count_++] = view;
    active_view_ = view;
    return true;
}

bool EditorGroup::RemoveView(DocumentViewId view) noexcept {
    const auto end = views_.begin() + static_cast<std::ptrdiff_t>(view_count_);
    const auto found = std::find(views_.begin(), end, view);
    if (found == end) {
        return false;
    }
    std::move(found + 1, end, found);
    --view_count_;
    views_[view_count_] = {};
    if (active_view_ == view) {
        active_view_ = view_count_ == 0U ? DocumentViewId{} : views_[0];
    }
    return true;
}

bool EditorGroup::ActivateView(DocumentViewId view) noexcept {
    if (!Contains(view)) {
        return false;
    }
    active_view_ = view;
    return true;
}

void EditorGroup::ClearViews() noexcept {
    views_.fill({});
    view_count_ = 0U;
    active_view_ = {};
}

bool EditorGroup::Contains(DocumentViewId view) const noexcept {
    return std::find(views_.cbegin(), views_.cbegin() + view_count_, view)
        != views_.cbegin() + view_count_;
}

DocumentViewId EditorGroup::ActiveView() const noexcept {
    return active_view_;
}

DocumentViewId EditorGroup::ViewAt(std::size_t index) const noexcept {
    return index < view_count_ ? views_[index] : DocumentViewId{};
}

std::size_t EditorGroup::ViewCount() const noexcept {
    return view_count_;
}

bool EditorArea::Initialize(
    EditorGroupId group,
    CanvasId canvas,
    Generation generation) noexcept {
    if (!group || !canvas || !generation) {
        return false;
    }
    Clear();
    groups_[0].id = group;
    groups_[0].canvas_id = canvas;
    groups_[0].generation = generation;
    group_count_ = 1U;
    active_group_ = group;
    return true;
}

bool EditorArea::Split(
    EditorGroupId group,
    CanvasId canvas,
    Generation generation,
    EditorSplitOrientation orientation) noexcept {
    if (group_count_ != 1U || !group || !canvas || !generation
        || orientation == EditorSplitOrientation::None
        || Find(group) != nullptr || FindByCanvas(canvas) != nullptr) {
        return false;
    }
    groups_[1].id = group;
    groups_[1].canvas_id = canvas;
    groups_[1].generation = generation;
    group_count_ = 2U;
    active_group_ = group;
    orientation_ = orientation;
    split_ratio_milli_ = 500U;
    return true;
}

bool EditorArea::SetOrientation(EditorSplitOrientation orientation) noexcept {
    if (group_count_ != 2U || orientation == EditorSplitOrientation::None) {
        return false;
    }
    orientation_ = orientation;
    return true;
}

bool EditorArea::Activate(EditorGroupId group) noexcept {
    if (Find(group) == nullptr) {
        return false;
    }
    active_group_ = group;
    return true;
}

bool EditorArea::AddView(EditorGroupId group, DocumentViewId view) noexcept {
    EditorGroup* target = Find(group);
    return target != nullptr && FindByView(view) == nullptr && target->AddView(view);
}

bool EditorArea::MoveView(
    DocumentViewId view, EditorGroupId destination) noexcept {
    EditorGroup* source = FindByView(view);
    EditorGroup* target = Find(destination);
    if (source == nullptr || target == nullptr || source == target
        || target->ViewCount() >= EditorGroup::kMaximumViews) {
        return false;
    }
    if (!source->RemoveView(view)) {
        return false;
    }
    if (target->AddView(view)) {
        active_group_ = destination;
        return true;
    }
    (void)source->AddView(view);
    return false;
}

bool EditorArea::RemoveView(DocumentViewId view) noexcept {
    EditorGroup* group = FindByView(view);
    return group != nullptr && group->RemoveView(view);
}

bool EditorArea::ResetViews(DocumentViewId view) noexcept {
    EditorGroup* active = Active();
    if (active == nullptr || !view) {
        return false;
    }
    for (std::size_t index = 0U; index < group_count_; ++index) {
        groups_[index].ClearViews();
    }
    return active->AddView(view);
}

bool EditorArea::MergeAndRemove(
    EditorGroupId closing, EditorGroupId& survivor) noexcept {
    if (group_count_ != 2U) {
        return false;
    }
    EditorGroup* source = Find(closing);
    EditorGroup* target = Other(closing);
    if (source == nullptr || target == nullptr
        || target->ViewCount() + source->ViewCount() > EditorGroup::kMaximumViews) {
        return false;
    }
    while (source->ViewCount() != 0U) {
        const DocumentViewId view = source->ViewAt(0U);
        if (!source->RemoveView(view) || !target->AddView(view)) {
            return false;
        }
    }
    survivor = target->id;
    const EditorGroup kept = *target;
    groups_.fill({});
    groups_[0] = kept;
    group_count_ = 1U;
    active_group_ = survivor;
    orientation_ = EditorSplitOrientation::None;
    split_ratio_milli_ = 500U;
    return true;
}

void EditorArea::Clear() noexcept {
    groups_.fill({});
    group_count_ = 0U;
    active_group_ = {};
    orientation_ = EditorSplitOrientation::None;
    split_ratio_milli_ = 500U;
    splitter = nullptr;
    drag_start = {};
    drag_ratio_milli = 500U;
    last_drag_layout_tick = 0U;
}

EditorGroup* EditorArea::Active() noexcept {
    return Find(active_group_);
}

const EditorGroup* EditorArea::Active() const noexcept {
    return Find(active_group_);
}

EditorGroup* EditorArea::Find(EditorGroupId group) noexcept {
    return const_cast<EditorGroup*>(
        static_cast<const EditorArea&>(*this).Find(group));
}

const EditorGroup* EditorArea::Find(EditorGroupId group) const noexcept {
    for (std::size_t index = 0U; index < group_count_; ++index) {
        if (groups_[index].id == group) {
            return &groups_[index];
        }
    }
    return nullptr;
}

EditorGroup* EditorArea::FindByView(DocumentViewId view) noexcept {
    return const_cast<EditorGroup*>(
        static_cast<const EditorArea&>(*this).FindByView(view));
}

const EditorGroup* EditorArea::FindByView(DocumentViewId view) const noexcept {
    for (std::size_t index = 0U; index < group_count_; ++index) {
        if (groups_[index].Contains(view)) {
            return &groups_[index];
        }
    }
    return nullptr;
}

EditorGroup* EditorArea::FindByCanvas(CanvasId canvas) noexcept {
    return const_cast<EditorGroup*>(
        static_cast<const EditorArea&>(*this).FindByCanvas(canvas));
}

const EditorGroup* EditorArea::FindByCanvas(CanvasId canvas) const noexcept {
    for (std::size_t index = 0U; index < group_count_; ++index) {
        if (groups_[index].canvas_id == canvas) {
            return &groups_[index];
        }
    }
    return nullptr;
}

EditorGroup* EditorArea::GroupAt(std::size_t index) noexcept {
    return const_cast<EditorGroup*>(
        static_cast<const EditorArea&>(*this).GroupAt(index));
}

const EditorGroup* EditorArea::GroupAt(std::size_t index) const noexcept {
    return index < group_count_ ? &groups_[index] : nullptr;
}

EditorGroup* EditorArea::Other(EditorGroupId group) noexcept {
    return const_cast<EditorGroup*>(
        static_cast<const EditorArea&>(*this).Other(group));
}

const EditorGroup* EditorArea::Other(EditorGroupId group) const noexcept {
    if (group_count_ != 2U) {
        return nullptr;
    }
    if (groups_[0].id == group) {
        return &groups_[1];
    }
    return groups_[1].id == group ? &groups_[0] : nullptr;
}

std::size_t EditorArea::GroupCount() const noexcept {
    return group_count_;
}

EditorSplitOrientation EditorArea::Orientation() const noexcept {
    return orientation_;
}

std::uint32_t EditorArea::SplitRatioMilli() const noexcept {
    return split_ratio_milli_;
}

void EditorArea::SetSplitRatioMilli(std::uint32_t ratio) noexcept {
    split_ratio_milli_ = std::clamp<std::uint32_t>(ratio, 200U, 800U);
}

}  // namespace inkpod::app
