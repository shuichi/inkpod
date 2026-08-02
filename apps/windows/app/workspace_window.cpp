#include "workspace_window.h"

#include <algorithm>
#include <new>

namespace inkpod::app {

bool WorkspaceWindowRegistry::Initialize(
    ApplicationHost* application,
    WorkspaceWindowId id,
    EditorGroupId editor_group,
    CanvasId canvas,
    Generation generation) noexcept {
    if (application == nullptr || !id || !editor_group || !canvas || !generation) {
        return false;
    }
    try {
        auto candidate = std::make_unique<WorkspaceWindow>();
        candidate->application = application;
        candidate->id = id;
        candidate->generation = generation;
        if (!candidate->editors.Initialize(editor_group, canvas, generation)) {
            return false;
        }
        candidate->persistence_slot = 0U;
        Clear();
        windows_[0] = std::move(candidate);
        count_ = 1U;
        current_ = id;
        last_focused_ = id;
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool WorkspaceWindowRegistry::Add(
    ApplicationHost* application,
    WorkspaceWindowId id,
    EditorGroupId editor_group,
    CanvasId canvas,
    Generation generation,
    std::uint32_t persistence_slot) noexcept {
    if (application == nullptr || !id || !editor_group || !canvas || !generation
        || count_ >= windows_.size() || Find(id) != nullptr) {
        return false;
    }
    try {
        auto candidate = std::make_unique<WorkspaceWindow>();
        candidate->application = application;
        candidate->id = id;
        candidate->generation = generation;
        candidate->persistence_slot = persistence_slot;
        if (!candidate->editors.Initialize(editor_group, canvas, generation)) {
            return false;
        }
        windows_[count_++] = std::move(candidate);
        current_ = id;
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool WorkspaceWindowRegistry::Activate(
    WorkspaceWindowId id, bool record_focus) noexcept {
    if (Find(id) == nullptr) {
        return false;
    }
    current_ = id;
    if (record_focus) {
        last_focused_ = id;
    }
    return true;
}

bool WorkspaceWindowRegistry::Remove(WorkspaceWindowId id) noexcept {
    const auto end = windows_.begin() + static_cast<std::ptrdiff_t>(count_);
    const auto found = std::find_if(
        windows_.begin(), end, [id](const auto& candidate) {
            return candidate != nullptr && candidate->id == id;
        });
    if (found == end) {
        return false;
    }
    std::move(found + 1, end, found);
    --count_;
    windows_[count_].reset();
    if (current_ == id) {
        current_ = count_ == 0U ? WorkspaceWindowId{} : windows_[0]->id;
    }
    if (last_focused_ == id) {
        last_focused_ = current_;
    }
    return true;
}

void WorkspaceWindowRegistry::Clear() noexcept {
    for (auto& window : windows_) {
        window.reset();
    }
    count_ = 0U;
    current_ = {};
    last_focused_ = {};
}

WorkspaceWindow* WorkspaceWindowRegistry::Current() noexcept {
    return Find(current_);
}

const WorkspaceWindow* WorkspaceWindowRegistry::Current() const noexcept {
    return Find(current_);
}

WorkspaceWindow* WorkspaceWindowRegistry::LastFocused() noexcept {
    return Find(last_focused_);
}

const WorkspaceWindow* WorkspaceWindowRegistry::LastFocused() const noexcept {
    return Find(last_focused_);
}

WorkspaceWindow* WorkspaceWindowRegistry::Find(WorkspaceWindowId id) noexcept {
    return const_cast<WorkspaceWindow*>(
        static_cast<const WorkspaceWindowRegistry&>(*this).Find(id));
}

const WorkspaceWindow* WorkspaceWindowRegistry::Find(
    WorkspaceWindowId id) const noexcept {
    for (std::size_t index = 0U; index < count_; ++index) {
        if (windows_[index] != nullptr && windows_[index]->id == id) {
            return windows_[index].get();
        }
    }
    return nullptr;
}

WorkspaceWindow* WorkspaceWindowRegistry::At(std::size_t index) noexcept {
    return const_cast<WorkspaceWindow*>(
        static_cast<const WorkspaceWindowRegistry&>(*this).At(index));
}

const WorkspaceWindow* WorkspaceWindowRegistry::At(
    std::size_t index) const noexcept {
    return index < count_ ? windows_[index].get() : nullptr;
}

std::size_t WorkspaceWindowRegistry::Count() const noexcept {
    return count_;
}

}  // namespace inkpod::app
