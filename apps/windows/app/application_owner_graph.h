#pragma once

#include "document_session.h"
#include "workspace_window.h"

namespace inkpod::app {

inline bool InitializeOwnerGraph(
    WorkspaceWindowRegistry& workspaces,
    DocumentRegistry& documents,
    ApplicationHost* application,
    WorkspaceWindowId workspace,
    EditorGroupId editor_group,
    CanvasId canvas,
    Generation generation) noexcept {
    if (!workspaces.Initialize(
            application, workspace, editor_group, canvas, generation)) {
        return false;
    }
    if (!documents.InitializePlaceholder(generation)) {
        workspaces.Clear();
        return false;
    }
    return true;
}

inline void ClearOwnerGraph(
    DocumentRegistry& documents,
    WorkspaceWindowRegistry& workspaces) noexcept {
    documents.Clear();
    workspaces.Clear();
}

}  // namespace inkpod::app
