#include "workspace_window.h"

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
        current_ = std::move(candidate);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

void WorkspaceWindowRegistry::Clear() noexcept {
    current_.reset();
}

WorkspaceWindow* WorkspaceWindowRegistry::Current() noexcept {
    return current_.get();
}

const WorkspaceWindow* WorkspaceWindowRegistry::Current() const noexcept {
    return current_.get();
}

}  // namespace inkpod::app
