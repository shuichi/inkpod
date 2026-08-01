#include "application_host.h"

#include <cassert>
#include <utility>

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

DocumentRegistry& ApplicationHost::Documents() noexcept {
    return documents_;
}

const DocumentRegistry& ApplicationHost::Documents() const noexcept {
    return documents_;
}

std::optional<ApplicationHost::DocumentBinding>
ApplicationHost::AddDocumentSession() noexcept {
    if (engine == nullptr) {
        return std::nullopt;
    }
    const CommandContext previous = routing.targets.Capture();
    const auto issued = routing.targets.AddDocument();
    if (!issued.has_value()) {
        return std::nullopt;
    }
    const DocumentBinding binding{
        issued.value(),
        routing.targets.ActiveDocumentView(),
        routing.targets.CurrentGeneration()};
    if (engine->CreateSession(binding.session, binding.generation)
            != INKPOD_STATUS_OK
        || !documents_.Add(
            binding.session,
            binding.generation,
            binding.view,
            engine.get())) {
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)routing.targets.ActivateDocument(
                previous.document_session.value(),
                previous.document_view.value());
        }
        return std::nullopt;
    }
    if (!ActivateDocumentView(binding.view)) {
        (void)documents_.Remove(binding.session);
        (void)engine->CloseSession(binding.session, binding.generation);
        (void)routing.targets.RemoveDocument(binding.session);
        if (previous.document_session.has_value()
            && previous.document_view.has_value()) {
            (void)ActivateDocumentView(previous.document_view.value());
        }
        return std::nullopt;
    }
    return binding;
}

bool ApplicationHost::ActivateDocumentView(DocumentViewId view) noexcept {
    DocumentSession* document = documents_.FindByView(view);
    DocumentView* target = document == nullptr ? nullptr : document->FindView(view);
    if (document == nullptr || target == nullptr || engine == nullptr) {
        return false;
    }
    DocumentSession* previous_document = documents_.Current();
    DocumentView* previous_view = previous_document == nullptr
        ? nullptr
        : previous_document->ActiveView();
    renderer::CancelCanvasStroke(Workspace().windows.canvas);
    if (!engine->SetActiveSession(document->id, document->generation)
        || (Workspace().windows.canvas != nullptr
            && !renderer::BindCanvasSnapshotSink(
                Workspace().windows.canvas,
                document->id,
                target->id,
                document->generation))
        || engine->SetActiveView(target->core_view_id) != INKPOD_STATUS_OK) {
        if (previous_document != nullptr && previous_view != nullptr) {
            (void)engine->SetActiveSession(
                previous_document->id, previous_document->generation);
            if (Workspace().windows.canvas != nullptr) {
                (void)renderer::BindCanvasSnapshotSink(
                    Workspace().windows.canvas,
                    previous_document->id,
                    previous_view->id,
                    previous_document->generation);
            }
            (void)engine->SetActiveView(previous_view->core_view_id);
        }
        return false;
    }
    return documents_.Activate(document->id)
        && document->ActivateView(view)
        && routing.targets.ActivateDocument(document->id, view);
}

bool ApplicationHost::CloseDocumentView(DocumentViewId view) noexcept {
    DocumentSession* document = documents_.FindByView(view);
    if (document == nullptr || document->ViewCount() <= 1U || engine == nullptr) {
        return false;
    }
    const DocumentView* closing = document->FindView(view);
    if (closing == nullptr) {
        return false;
    }
    DocumentViewId replacement{};
    for (std::size_t index = 0U; index < document->ViewCount(); ++index) {
        const DocumentView* candidate = document->ViewAt(index);
        if (candidate != nullptr && candidate->id != view) {
            replacement = candidate->id;
            break;
        }
    }
    const InkpodStatus status = engine->Invoke(
        document->id,
        document->generation,
        [core_view_id = closing->core_view_id](InkpodCore* core) {
            return inkpod_core_view_close(core, core_view_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK
        || !document->RemoveView(view)
        || !routing.targets.RemoveDocumentView(view)) {
        return false;
    }
    return replacement && ActivateDocumentView(replacement);
}

bool ApplicationHost::CloseDocumentSession(DocumentSessionId session) noexcept {
    DocumentSession* document = documents_.Find(session);
    if (document == nullptr || engine == nullptr) {
        return false;
    }
    const Generation generation = document->generation;
    if (documents_.Current() == document) {
        renderer::CancelCanvasStroke(Workspace().windows.canvas);
    }
    if (engine->CloseSession(session, generation) != INKPOD_STATUS_OK) {
        return false;
    }
    return documents_.Remove(session)
        && routing.targets.RemoveDocument(session);
}

std::uint32_t ApplicationHost::IssueUntitledNumber() noexcept {
    const std::uint32_t result = next_untitled_number_++;
    if (next_untitled_number_ == 0U) {
        next_untitled_number_ = 1U;
    }
    return result == 0U ? next_untitled_number_++ : result;
}

bool ApplicationHost::RecordRecentDocument(
    std::wstring path,
    DocumentIdentity identity) noexcept {
    return recent_documents_.Record(
        std::move(path), std::move(identity));
}

bool ApplicationHost::RemoveRecentDocument(std::size_t index) noexcept {
    return recent_documents_.Remove(index);
}

const RecentDocumentEntry* ApplicationHost::RecentDocumentAt(
    std::size_t index) const noexcept {
    return recent_documents_.At(index);
}

std::size_t ApplicationHost::RecentDocumentCount() const noexcept {
    return recent_documents_.Count();
}

void ApplicationHost::DetachCoreSessions() noexcept {
    documents_.ClearCoreBindings();
}

}  // namespace inkpod::app
