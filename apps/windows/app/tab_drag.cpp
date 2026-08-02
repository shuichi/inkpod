#include "tab_drag.h"

#include <algorithm>
#include <cstdlib>

namespace inkpod::app {
namespace {

bool IsTabOperation(DragOperation operation) noexcept {
    return operation == DragOperation::TabMove
        || operation == DragOperation::TabCopy;
}

bool IsCompleteTabContext(const CommandContext& context) noexcept {
    return context.workspace.has_value()
        && context.editor_group.has_value()
        && context.document_session.has_value()
        && context.document_view.has_value()
        && context.generation.has_value();
}

bool IsValidTarget(const TabDropTarget& target) noexcept {
    switch (target.kind) {
        case TabDropKind::None:
            return true;
        case TabDropKind::Reorder:
        case TabDropKind::EditorGroup:
            return static_cast<bool>(target.workspace)
                && static_cast<bool>(target.group);
        case TabDropKind::TearOut:
            return !target.workspace && !target.group;
    }
    return false;
}

}  // namespace

bool TabDragCoordinator::Arm(
    DragToken token,
    CommandContext restore_context,
    std::size_t source_index,
    std::int32_t screen_x,
    std::int32_t screen_y) noexcept {
    if (!token || !IsTabOperation(token.operation)
        || !IsCompleteTabContext(token.context)) {
        return false;
    }
    token_ = token;
    restore_context_ = restore_context;
    source_index_ = source_index;
    start_x_ = screen_x;
    start_y_ = screen_y;
    target_ = {};
    armed_ = true;
    dragging_ = false;
    return true;
}

bool TabDragCoordinator::TryBegin(
    std::int32_t screen_x,
    std::int32_t screen_y,
    std::int32_t threshold_x,
    std::int32_t threshold_y) noexcept {
    if (!armed_) {
        return false;
    }
    if (dragging_) {
        return true;
    }
    threshold_x = std::max<std::int32_t>(1, threshold_x);
    threshold_y = std::max<std::int32_t>(1, threshold_y);
    dragging_ = std::abs(screen_x - start_x_) >= threshold_x
        || std::abs(screen_y - start_y_) >= threshold_y;
    return dragging_;
}

bool TabDragCoordinator::SetOperation(DragOperation operation) noexcept {
    if (!armed_ || !IsTabOperation(operation)) {
        return false;
    }
    token_.operation = operation;
    return true;
}

bool TabDragCoordinator::UpdateTarget(TabDropTarget target) noexcept {
    if (!dragging_ || !IsValidTarget(target)) {
        return false;
    }
    target_ = target;
    return true;
}

std::optional<TabDropRequest> TabDragCoordinator::TakeDrop() noexcept {
    if (!dragging_ || target_.kind == TabDropKind::None) {
        (void)Cancel();
        return std::nullopt;
    }
    TabDropRequest request{token_, restore_context_, source_index_, target_};
    (void)Cancel();
    return request;
}

bool TabDragCoordinator::Cancel() noexcept {
    const bool changed = armed_ || dragging_;
    token_ = {};
    restore_context_ = {};
    source_index_ = 0U;
    start_x_ = 0;
    start_y_ = 0;
    target_ = {};
    armed_ = false;
    dragging_ = false;
    return changed;
}

bool TabDragCoordinator::IsArmed() const noexcept {
    return armed_;
}

bool TabDragCoordinator::IsDragging() const noexcept {
    return dragging_;
}

bool TabDragCoordinator::ReferencesWorkspace(
    WorkspaceWindowId workspace) const noexcept {
    return (armed_ && token_.context.workspace == workspace)
        || (dragging_ && target_.workspace == workspace);
}

bool TabDragCoordinator::ReferencesGroup(EditorGroupId group) const noexcept {
    return (armed_ && token_.context.editor_group == group)
        || (dragging_ && target_.group == group);
}

const DragToken* TabDragCoordinator::Token() const noexcept {
    return armed_ ? &token_ : nullptr;
}

const CommandContext* TabDragCoordinator::RestoreContext() const noexcept {
    return armed_ ? &restore_context_ : nullptr;
}

const TabDropTarget& TabDragCoordinator::Target() const noexcept {
    return target_;
}

}  // namespace inkpod::app
