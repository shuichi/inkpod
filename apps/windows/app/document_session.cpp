#include "document_session.h"

#include <algorithm>
#include <new>
#include <utility>

namespace inkpod::app {
namespace {

void ResetPresentation(ViewUiState& view) noexcept {
    view.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    view.secondary_view_id = 0U;
    view.active_view_id = 0U;
    view.flip_horizontal = false;
    view.flip_vertical = false;
    view.ruler_visible = false;
    view.guides_visible = true;
    view.grid_visible = false;
    view.snap_guides = false;
    view.snap_grid = false;
    view.transparent_visible = true;
    view.pointer_device_x = 0;
    view.pointer_device_y = 0;
    ++view.locator_generation;
    view.locator_presented_generation = view.locator_generation;
    view.locator_valid = false;
    view.locator = {};
    view.locator_neighborhood_width = 0U;
    view.locator_neighborhood_height = 0U;
    view.locator_neighborhood_origin_x = 0;
    view.locator_neighborhood_origin_y = 0;
    view.locator_neighborhood.fill(0U);
    view.gesture_samples.clear();
    view.guide_drag_active = false;
    view.guide_drag_axis = 0U;
    view.guide_drag_id = 0U;
    view.active_drag.reset();
}

void InitializeView(
    DocumentView& view,
    DocumentViewId id,
    Generation generation,
    std::uint64_t core_view_id) noexcept {
    view.id = id;
    view.generation = generation;
    view.core_view_id = core_view_id;
    ResetPresentation(view.presentation);
}

}  // namespace

void DocumentSession::BindCore(CoreHost* host) noexcept {
    core_ = host;
}

CoreHost* DocumentSession::Core() const noexcept {
    return core_;
}

bool DocumentSession::AcknowledgeSequencePresentation(
    DocumentSessionId presented_session,
    Generation presented_generation,
    DocumentViewId presented_view,
    std::uint64_t presented_document_revision,
    std::uint64_t presented_epoch) noexcept {
    if (!id || !generation || sequence_activation_pending
        || presented_session != id || presented_generation != generation
        || FindView(presented_view) == nullptr || presented_epoch == 0U
        || presented_epoch != sequence_required_present_epoch
        || presented_document_revision < sequence_required_present_revision) {
        return false;
    }
    sequence_presented_session_ = presented_session;
    sequence_presented_generation_ = presented_generation;
    sequence_presented_revision_ = presented_document_revision;
    sequence_presented_epoch_ = presented_epoch;
    return true;
}

bool DocumentSession::HasSequencePresentationAcknowledgement() const noexcept {
    return !sequence_activation_pending && sequence_required_present_epoch != 0U
        && sequence_presented_session_ == id
        && sequence_presented_generation_ == generation
        && sequence_presented_epoch_ == sequence_required_present_epoch
        && sequence_presented_revision_ >= sequence_required_present_revision;
}

void DocumentSession::ResetViews(
    DocumentViewId initial_view,
    Generation view_generation,
    std::uint64_t core_view_id) noexcept {
    view_used_.fill(false);
    InitializeView(views_[0], initial_view, view_generation, core_view_id);
    view_used_[0] = true;
    view_count_ = 1U;
    active_view_ = initial_view;
}

bool DocumentSession::AddView(
    DocumentViewId view,
    Generation view_generation,
    std::uint64_t core_view_id) noexcept {
    if (!view || view_count_ >= views_.size() || FindView(view) != nullptr
        || FindCoreView(core_view_id) != nullptr) {
        return false;
    }
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (!view_used_[index]) {
            InitializeView(views_[index], view, view_generation, core_view_id);
            view_used_[index] = true;
            ++view_count_;
            active_view_ = view;
            return true;
        }
    }
    return false;
}

bool DocumentSession::RemoveView(DocumentViewId view) noexcept {
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (!view_used_[index] || views_[index].id != view) {
            continue;
        }
        view_used_[index] = false;
        --view_count_;
        if (active_view_ == view) {
            active_view_ = {};
            for (std::size_t candidate = 0U; candidate < views_.size(); ++candidate) {
                if (view_used_[candidate]) {
                    active_view_ = views_[candidate].id;
                    break;
                }
            }
        }
        return true;
    }
    return false;
}

bool DocumentSession::ActivateView(DocumentViewId view) noexcept {
    if (FindView(view) == nullptr) {
        return false;
    }
    active_view_ = view;
    return true;
}

bool DocumentSession::ActivateCoreView(std::uint64_t core_view_id) noexcept {
    const DocumentView* view = FindCoreView(core_view_id);
    return view != nullptr && ActivateView(view->id);
}

DocumentView* DocumentSession::FindView(DocumentViewId view) noexcept {
    return const_cast<DocumentView*>(
        static_cast<const DocumentSession&>(*this).FindView(view));
}

const DocumentView* DocumentSession::FindView(DocumentViewId view) const noexcept {
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (view_used_[index] && views_[index].id == view) {
            return &views_[index];
        }
    }
    return nullptr;
}

DocumentView* DocumentSession::FindCoreView(std::uint64_t core_view_id) noexcept {
    return const_cast<DocumentView*>(
        static_cast<const DocumentSession&>(*this).FindCoreView(core_view_id));
}

const DocumentView* DocumentSession::FindCoreView(
    std::uint64_t core_view_id) const noexcept {
    for (std::size_t index = 0U; index < views_.size(); ++index) {
        if (view_used_[index]
            && views_[index].core_view_id == core_view_id) {
            return &views_[index];
        }
    }
    return nullptr;
}

DocumentView* DocumentSession::ActiveView() noexcept {
    return FindView(active_view_);
}

const DocumentView* DocumentSession::ActiveView() const noexcept {
    return FindView(active_view_);
}

DocumentView* DocumentSession::ViewAt(std::size_t index) noexcept {
    return const_cast<DocumentView*>(
        static_cast<const DocumentSession&>(*this).ViewAt(index));
}

const DocumentView* DocumentSession::ViewAt(std::size_t index) const noexcept {
    std::size_t current{};
    for (std::size_t slot = 0U; slot < views_.size(); ++slot) {
        if (!view_used_[slot]) {
            continue;
        }
        if (current++ == index) {
            return &views_[slot];
        }
    }
    return nullptr;
}

std::size_t DocumentSession::ViewCount() const noexcept {
    return view_count_;
}

const SequenceAutosaveBinding* DocumentSession::FindSequenceAutosave(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation) const noexcept {
    const auto found = std::find_if(
        sequence_autosaves_.cbegin(),
        sequence_autosaves_.cend(),
        [document_uuid_high,
         document_uuid_low,
         source_generation](const SequenceAutosaveBinding& binding) {
            return binding.document_uuid_high == document_uuid_high
                && binding.document_uuid_low == document_uuid_low
                && binding.source_generation == source_generation;
        });
    return found == sequence_autosaves_.cend() ? nullptr : &*found;
}

bool DocumentSession::PublishSequenceAutosave(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation,
    const std::wstring& recovery_path,
    const RecoveryMetadata& metadata) noexcept {
    if ((document_uuid_high == 0U && document_uuid_low == 0U)
        || source_generation == 0U || recovery_path.empty()
        || metadata.document_uuid_high != document_uuid_high
        || metadata.document_uuid_low != document_uuid_low) {
        return false;
    }
    try {
        SequenceAutosaveBinding binding{};
        binding.document_uuid_high = document_uuid_high;
        binding.document_uuid_low = document_uuid_low;
        binding.source_generation = source_generation;
        binding.recovery_path = recovery_path;
        binding.metadata = metadata;
        return ReserveSequenceAutosave(
                   document_uuid_high, document_uuid_low, source_generation)
            && PublishReservedSequenceAutosave(std::move(binding));
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DocumentSession::ReserveSequenceAutosave(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation) noexcept {
    if ((document_uuid_high == 0U && document_uuid_low == 0U)
        || source_generation == 0U) {
        return false;
    }
    if (const auto* existing = FindSequenceAutosave(
            document_uuid_high, document_uuid_low, source_generation);
        existing != nullptr) {
        return existing->artifact_generation != UINT64_MAX;
    }
    if (sequence_autosaves_.size() >= 10'000U) {
        return false;
    }
    try {
        sequence_autosaves_.reserve(sequence_autosaves_.size() + 1U);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DocumentSession::PublishReservedSequenceAutosave(
    SequenceAutosaveBinding binding) noexcept {
    if ((binding.document_uuid_high == 0U
            && binding.document_uuid_low == 0U)
        || binding.source_generation == 0U || binding.recovery_path.empty()
        || binding.metadata.document_uuid_high != binding.document_uuid_high
        || binding.metadata.document_uuid_low != binding.document_uuid_low) {
        return false;
    }
    auto found = std::find_if(
        sequence_autosaves_.begin(),
        sequence_autosaves_.end(),
        [&binding](const SequenceAutosaveBinding& candidate) {
            return candidate.document_uuid_high == binding.document_uuid_high
                && candidate.document_uuid_low == binding.document_uuid_low
                && candidate.source_generation == binding.source_generation;
        });
    if (found != sequence_autosaves_.end()) {
        if (found->artifact_generation == UINT64_MAX) {
            return false;
        }
        binding.artifact_generation = found->artifact_generation + 1U;
        *found = std::move(binding);
        return true;
    }
    if (sequence_autosaves_.size() >= sequence_autosaves_.capacity()
        || sequence_autosaves_.size() >= 10'000U) {
        return false;
    }
    binding.artifact_generation = 1U;
    sequence_autosaves_.push_back(std::move(binding));
    return true;
}

void DocumentSession::ClearSequenceAutosaves() noexcept {
    sequence_autosaves_.clear();
}

bool DocumentRegistry::InitializePlaceholder(Generation generation) noexcept {
    if (!generation) {
        return false;
    }
    std::unique_ptr<DocumentSession> candidate;
    try {
        candidate = std::make_unique<DocumentSession>();
    } catch (const std::bad_alloc&) {
        return false;
    }
    candidate->generation = generation;
    candidate->ResetViews({}, generation);
    Clear();
    sessions_[0] = std::move(candidate);
    current_index_ = 0U;
    count_ = 1U;
    return true;
}

bool DocumentRegistry::Replace(
    DocumentSessionId id,
    Generation generation,
    DocumentViewId initial_view,
    CoreHost* core) noexcept {
    if (!id || !generation || !initial_view || core == nullptr) {
        return false;
    }
    DocumentSession* current = Current();
    const DocumentSession* duplicate = Find(id);
    if (duplicate != nullptr && duplicate != current) {
        return false;
    }
    if (current == nullptr) {
        return Add(id, generation, initial_view, core);
    }
    current->id = id;
    current->generation = generation;
    CancelIdentityReservation(id);
    current->BindCore(core);
    current->ClearSequenceAutosaves();
    current->auto_sequence_truncated = false;
    current->ResetViews(initial_view, generation);
    return true;
}

bool DocumentRegistry::Add(
    DocumentSessionId id,
    Generation generation,
    DocumentViewId initial_view,
    CoreHost* core) noexcept {
    if (!id || !generation || !initial_view || core == nullptr
        || count_ >= sessions_.size() || Find(id) != nullptr) {
        return false;
    }
    for (std::size_t index = 0U; index < sessions_.size(); ++index) {
        if (sessions_[index] != nullptr) {
            continue;
        }
        try {
            sessions_[index] = std::make_unique<DocumentSession>();
        } catch (const std::bad_alloc&) {
            return false;
        }
        sessions_[index]->id = id;
        sessions_[index]->generation = generation;
        sessions_[index]->BindCore(core);
        sessions_[index]->ResetViews(initial_view, generation);
        current_index_ = index;
        ++count_;
        return true;
    }
    return false;
}

bool DocumentRegistry::Remove(DocumentSessionId id) noexcept {
    for (std::size_t index = 0U; index < sessions_.size(); ++index) {
        if (sessions_[index] == nullptr || sessions_[index]->id != id) {
            continue;
        }
        sessions_[index].reset();
        --count_;
        if (current_index_ == index) {
            current_index_ = kMaximumSessions;
            for (std::size_t candidate = 0U; candidate < sessions_.size(); ++candidate) {
                if (sessions_[candidate] != nullptr) {
                    current_index_ = candidate;
                    break;
                }
            }
        }
        return true;
    }
    return false;
}

void DocumentRegistry::ClearActive() noexcept {
    current_index_ = kMaximumSessions;
}

bool DocumentRegistry::Activate(DocumentSessionId id) noexcept {
    for (std::size_t index = 0U; index < sessions_.size(); ++index) {
        if (sessions_[index] != nullptr && sessions_[index]->id == id) {
            current_index_ = index;
            return true;
        }
    }
    return false;
}

DocumentSession* DocumentRegistry::Find(DocumentSessionId id) noexcept {
    return const_cast<DocumentSession*>(
        static_cast<const DocumentRegistry&>(*this).Find(id));
}

const DocumentSession* DocumentRegistry::Find(DocumentSessionId id) const noexcept {
    for (const auto& session : sessions_) {
        if (session != nullptr && session->id == id) {
            return session.get();
        }
    }
    return nullptr;
}

DocumentSession* DocumentRegistry::FindByView(DocumentViewId view) noexcept {
    return const_cast<DocumentSession*>(
        static_cast<const DocumentRegistry&>(*this).FindByView(view));
}

const DocumentSession* DocumentRegistry::FindByView(
    DocumentViewId view) const noexcept {
    for (const auto& session : sessions_) {
        if (session != nullptr && session->FindView(view) != nullptr) {
            return session.get();
        }
    }
    return nullptr;
}

DocumentSession* DocumentRegistry::FindByIdentity(
    const DocumentIdentity& identity) noexcept {
    return const_cast<DocumentSession*>(
        static_cast<const DocumentRegistry&>(*this).FindByIdentity(identity));
}

const DocumentSession* DocumentRegistry::FindByIdentity(
    const DocumentIdentity& identity) const noexcept {
    if (!identity) {
        return nullptr;
    }
    for (const auto& session : sessions_) {
        if (session != nullptr && session->identity == identity) {
            return session.get();
        }
    }
    return nullptr;
}

bool DocumentRegistry::AssignIdentity(
    DocumentSessionId id,
    const DocumentIdentity& identity) noexcept {
    DocumentSession* session = Find(id);
    const DocumentSession* conflict = FindByIdentity(identity);
    if (session == nullptr || !identity
        || (conflict != nullptr && conflict != session)
        || HasIdentityReservation(identity, {}, id)) {
        return false;
    }
    try {
        DocumentIdentity candidate = identity;
        session->identity = std::move(candidate);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DocumentRegistry::ReserveIdentity(
    DocumentSessionId id,
    const DocumentIdentity& identity,
    const std::wstring& original_path,
    const std::wstring& source_path) noexcept {
    DocumentSession* session = Find(id);
    const DocumentSession* conflict = FindByIdentity(identity);
    if (session == nullptr || !identity || session->reserved_identity_
        || (conflict != nullptr && conflict != session)
        || HasIdentityReservation(identity, {}, id)) {
        return false;
    }
    try {
        DocumentIdentity candidate = identity;
        std::array<std::wstring, 2U> paths;
        const std::array<const std::wstring*, 2U> inputs{&original_path, &source_path};
        for (std::size_t index = 0U; index < paths.size(); ++index) {
            if (!inputs[index]->empty()
                && !NormalizeDocumentFilePath(*inputs[index], paths[index])) {
                return false;
            }
            if (paths[index].empty()) {
                continue;
            }
            if (HasIdentityReservation(identity, paths[index], id)) {
                return false;
            }
            for (const auto& other : sessions_) {
                if (other == nullptr || other.get() == session) {
                    continue;
                }
                for (const auto* path : {&other->shell.current_path,
                         &other->shell.source_path, &other->shell.recovery_original_path}) {
                    if (path->empty()) {
                        continue;
                    }
                    std::wstring normalized;
                    if (!NormalizeDocumentFilePath(*path, normalized)
                        || normalized == paths[index]) {
                        return false;
                    }
                }
            }
        }
        session->reserved_identity_ = std::move(candidate);
        session->reserved_identity_paths_ = std::move(paths);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DocumentRegistry::HasIdentityReservation(
    const DocumentIdentity& identity,
    const std::wstring& normalized_path,
    DocumentSessionId except) const noexcept {
    for (const auto& session : sessions_) {
        if (session == nullptr || session->id == except || !session->reserved_identity_) {
            continue;
        }
        if (identity && session->reserved_identity_ == identity) {
            return true;
        }
        if (!normalized_path.empty()
            && std::find(session->reserved_identity_paths_.begin(),
                   session->reserved_identity_paths_.end(), normalized_path)
                != session->reserved_identity_paths_.end()) {
            return true;
        }
    }
    return false;
}

bool DocumentRegistry::PublishReservedIdentity(DocumentSessionId id) noexcept {
    DocumentSession* session = Find(id);
    if (session == nullptr || !session->reserved_identity_) {
        return false;
    }
    session->identity = std::move(session->reserved_identity_);
    CancelIdentityReservation(id);
    return true;
}

void DocumentRegistry::CancelIdentityReservation(DocumentSessionId id) noexcept {
    if (DocumentSession* session = Find(id); session != nullptr) {
        session->reserved_identity_ = {};
        for (auto& path : session->reserved_identity_paths_) {
            path.clear();
        }
    }
}

void DocumentRegistry::ClearCoreBindings() noexcept {
    for (auto& session : sessions_) {
        if (session != nullptr) {
            session->BindCore(nullptr);
        }
    }
}

void DocumentRegistry::Clear() noexcept {
    for (auto& session : sessions_) {
        session.reset();
    }
    current_index_ = kMaximumSessions;
    count_ = 0U;
}

DocumentSession* DocumentRegistry::Current() noexcept {
    return current_index_ < sessions_.size()
        ? sessions_[current_index_].get()
        : nullptr;
}

const DocumentSession* DocumentRegistry::Current() const noexcept {
    return current_index_ < sessions_.size()
        ? sessions_[current_index_].get()
        : nullptr;
}

DocumentSession* DocumentRegistry::SessionAt(std::size_t index) noexcept {
    return const_cast<DocumentSession*>(
        static_cast<const DocumentRegistry&>(*this).SessionAt(index));
}

const DocumentSession* DocumentRegistry::SessionAt(std::size_t index) const noexcept {
    std::size_t current{};
    for (const auto& session : sessions_) {
        if (session == nullptr) {
            continue;
        }
        if (current++ == index) {
            return session.get();
        }
    }
    return nullptr;
}

std::size_t DocumentRegistry::Count() const noexcept {
    return count_;
}

}  // namespace inkpod::app
