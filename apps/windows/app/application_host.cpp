#include "application_host.h"

#include <cassert>

#include "application_owner_graph.h"
#include "renderer/canvas.h"

namespace inkpod::app {

bool ApplicationHost::InitializeOwners() noexcept {
    routing.targets.Initialize();
    const Generation generation = routing.targets.CurrentGeneration();
    return InitializeOwnerGraph(
        workspaces_,
        documents_,
        this,
        routing.targets.Workspace(),
        generation);
}

void ApplicationHost::ClearOwners() noexcept {
    ClearOwnerGraph(documents_, workspaces_);
}

WorkspaceWindow& ApplicationHost::Workspace() noexcept {
    assert(workspaces_.Current() != nullptr);
    return *workspaces_.Current();
}

const WorkspaceWindow& ApplicationHost::Workspace() const noexcept {
    assert(workspaces_.Current() != nullptr);
    return *workspaces_.Current();
}

DocumentSession& ApplicationHost::Document() noexcept {
    assert(documents_.Current() != nullptr);
    return *documents_.Current();
}

const DocumentSession& ApplicationHost::Document() const noexcept {
    assert(documents_.Current() != nullptr);
    return *documents_.Current();
}

DocumentView& ApplicationHost::ActiveView() noexcept {
    assert(Document().ActiveView() != nullptr);
    return *Document().ActiveView();
}

const DocumentView& ApplicationHost::ActiveView() const noexcept {
    assert(Document().ActiveView() != nullptr);
    return *Document().ActiveView();
}

bool ApplicationHost::ReplaceDocumentSession(
    DocumentSessionId id,
    Generation generation,
    DocumentViewId initial_view) noexcept {
    if (id != routing.targets.DocumentSession()
        || generation != routing.targets.CurrentGeneration()
        || initial_view != routing.targets.ActiveDocumentView()
        || engine == nullptr) {
        return false;
    }
    DocumentSession& current = Document();
    const DocumentSessionId old_id = current.id;
    const Generation old_generation = current.generation;
    const bool had_core = old_id && old_generation
        && engine->HasSession(old_id, old_generation);
    const InkpodStatus binding_status = had_core
        ? engine->RebindSession(old_id, old_generation, id, generation)
        : engine->CreateSession(id, generation);
    if (binding_status != INKPOD_STATUS_OK) {
        return false;
    }
    if (!documents_.Replace(id, generation, initial_view, engine.get())) {
        if (had_core) {
            (void)engine->RebindSession(id, generation, old_id, old_generation);
        } else {
            (void)engine->CloseSession(id, generation);
        }
        return false;
    }
    if (!engine->SetActiveSession(id, generation)) {
        return false;
    }
    return Workspace().windows.canvas == nullptr
        || renderer::BindCanvasSnapshotSink(
            Workspace().windows.canvas,
            id,
            initial_view,
            generation);
}

void ApplicationHost::DetachCoreSessions() noexcept {
    documents_.ClearCoreBindings();
}

}  // namespace inkpod::app
