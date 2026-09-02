#include "document_session.h"

#include <algorithm>
#include <new>
#include <utility>

namespace inkpod::app {

bool ValidRecoveryArtifactProof(
    const InkpodIoRecoveryArtifactProof& proof) noexcept {
    const auto valid_stamp = [](const InkpodIoRecoveryArtifactStamp& stamp) {
        const bool physical = stamp.identity.volume != 0U
            || stamp.identity.object_high != 0U
            || stamp.identity.object_low != 0U;
        return stamp.struct_size == sizeof(InkpodIoRecoveryArtifactStamp)
            && (stamp.flags & ~INKPOD_IO_RECOVERY_ARTIFACT_READONLY) == 0U
            && stamp.identity.struct_size == sizeof(InkpodIoFileIdentity)
            && stamp.identity.kind == 1U && physical;
    };
    return proof.struct_size == sizeof(InkpodIoRecoveryArtifactProof)
        && proof.reserved == 0U
        && valid_stamp(proof.native) && valid_stamp(proof.metadata);
}

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

bool DocumentSession::AdvancePersistenceEpoch() noexcept {
    if (persistence_epoch == UINT64_MAX) {
        return false;
    }
    ++persistence_epoch;
    return true;
}

bool DocumentSession::AdvanceSequenceCatalogIntentEpoch() noexcept {
    if (sequence_catalog_intent_epoch == UINT64_MAX) {
        return false;
    }
    ++sequence_catalog_intent_epoch;
    return true;
}

bool DocumentSession::HasExactPairIdentities(
    const DocumentIdentity& native_identity,
    const DocumentIdentity& raster_identity) const noexcept {
    return native_identity && raster_identity
        && identity == native_identity
        && pair_raster_identity == raster_identity;
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
    const RecoveryMetadata& metadata,
    const InkpodIoRecoveryArtifactProof& artifact_proof) noexcept {
    if ((document_uuid_high == 0U && document_uuid_low == 0U)
        || source_generation == 0U || recovery_path.empty()
        || metadata.document_uuid_high != document_uuid_high
        || metadata.document_uuid_low != document_uuid_low
        || !ValidRecoveryArtifactProof(artifact_proof)) {
        return false;
    }
    try {
        SequenceAutosaveBinding binding{};
        binding.document_uuid_high = document_uuid_high;
        binding.document_uuid_low = document_uuid_low;
        binding.source_generation = source_generation;
        binding.recovery_path = recovery_path;
        binding.metadata = metadata;
        binding.artifact_proof = artifact_proof;
        const auto* existing = FindSequenceAutosave(
            document_uuid_high, document_uuid_low, source_generation);
        const std::uint64_t expected_generation = existing == nullptr
            ? 0U : existing->artifact_generation;
        if (!ReserveSequenceAutosave(
                document_uuid_high, document_uuid_low, source_generation,
                expected_generation)) {
            return false;
        }
        const bool published = PublishReservedSequenceAutosave(
            std::move(binding), expected_generation);
        if (!published) {
            CancelSequenceAutosaveReservation(
                document_uuid_high, document_uuid_low, source_generation,
                expected_generation);
        }
        return published;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DocumentSession::ReserveSequenceAutosave(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation,
    std::uint64_t expected_artifact_generation) noexcept {
    if ((document_uuid_high == 0U && document_uuid_low == 0U)
        || source_generation == 0U
        || sequence_autosave_reservations_.size() >= 64U
        || std::any_of(
            sequence_autosave_reservations_.cbegin(),
            sequence_autosave_reservations_.cend(),
            [document_uuid_high, document_uuid_low, source_generation](
                const SequenceAutosaveReservation& reservation) {
                return reservation.document_uuid_high == document_uuid_high
                    && reservation.document_uuid_low == document_uuid_low
                    && reservation.source_generation == source_generation;
            })) {
        return false;
    }
    if (const auto* existing = FindSequenceAutosave(
            document_uuid_high, document_uuid_low, source_generation);
        existing != nullptr) {
        if (existing->artifact_generation == UINT64_MAX
            || existing->artifact_generation != expected_artifact_generation) {
            return false;
        }
    } else if (expected_artifact_generation != 0U) {
        return false;
    }
    if (sequence_autosaves_.size() >= 10'000U) {
        return false;
    }
    try {
        sequence_autosaves_.reserve(
            sequence_autosaves_.size()
            + sequence_autosave_reservations_.size() + 1U);
        sequence_autosave_reservations_.reserve(
            sequence_autosave_reservations_.size() + 1U);
        sequence_autosave_reservations_.push_back(
            SequenceAutosaveReservation{
                document_uuid_high,
                document_uuid_low,
                source_generation,
                expected_artifact_generation});
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DocumentSession::PublishReservedSequenceAutosave(
    SequenceAutosaveBinding binding,
    std::uint64_t expected_artifact_generation) noexcept {
    if ((binding.document_uuid_high == 0U
            && binding.document_uuid_low == 0U)
        || binding.source_generation == 0U || binding.recovery_path.empty()
        || binding.metadata.document_uuid_high != binding.document_uuid_high
        || binding.metadata.document_uuid_low != binding.document_uuid_low
        || !ValidRecoveryArtifactProof(binding.artifact_proof)) {
        return false;
    }
    const auto reservation = std::find_if(
        sequence_autosave_reservations_.begin(),
        sequence_autosave_reservations_.end(),
        [&binding, expected_artifact_generation](
            const SequenceAutosaveReservation& candidate) {
            return candidate.document_uuid_high == binding.document_uuid_high
                && candidate.document_uuid_low == binding.document_uuid_low
                && candidate.source_generation == binding.source_generation
                && candidate.expected_artifact_generation
                    == expected_artifact_generation;
        });
    if (reservation == sequence_autosave_reservations_.end()) {
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
        if (found->artifact_generation != expected_artifact_generation
            || found->artifact_generation == UINT64_MAX) {
            return false;
        }
        binding.artifact_generation = found->artifact_generation + 1U;
        *found = std::move(binding);
        sequence_autosave_reservations_.erase(reservation);
        return true;
    }
    if (sequence_autosaves_.size() >= sequence_autosaves_.capacity()
        || sequence_autosaves_.size() >= 10'000U
        || expected_artifact_generation != 0U) {
        return false;
    }
    binding.artifact_generation = 1U;
    sequence_autosaves_.push_back(std::move(binding));
    sequence_autosave_reservations_.erase(reservation);
    return true;
}

void DocumentSession::CancelSequenceAutosaveReservation(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation,
    std::uint64_t expected_artifact_generation) noexcept {
    const auto reservation = std::find_if(
        sequence_autosave_reservations_.begin(),
        sequence_autosave_reservations_.end(),
        [document_uuid_high, document_uuid_low, source_generation,
            expected_artifact_generation](
            const SequenceAutosaveReservation& candidate) {
            return candidate.document_uuid_high == document_uuid_high
                && candidate.document_uuid_low == document_uuid_low
                && candidate.source_generation == source_generation
                && candidate.expected_artifact_generation
                    == expected_artifact_generation;
        });
    if (reservation != sequence_autosave_reservations_.end()) {
        sequence_autosave_reservations_.erase(reservation);
    }
}

bool DocumentSession::RemoveSequenceAutosave(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation,
    std::uint64_t artifact_generation) noexcept {
    if (std::any_of(
            sequence_autosave_reservations_.cbegin(),
            sequence_autosave_reservations_.cend(),
            [document_uuid_high, document_uuid_low, source_generation](
                const SequenceAutosaveReservation& reservation) {
                return reservation.document_uuid_high == document_uuid_high
                    && reservation.document_uuid_low == document_uuid_low
                    && reservation.source_generation == source_generation;
            })) {
        return false;
    }
    const auto found = std::find_if(sequence_autosaves_.begin(), sequence_autosaves_.end(),
        [document_uuid_high, document_uuid_low, source_generation,
            artifact_generation](const auto& binding) {
            return binding.document_uuid_high == document_uuid_high
                && binding.document_uuid_low == document_uuid_low
                && binding.source_generation == source_generation
                && binding.artifact_generation == artifact_generation;
        });
    if (found == sequence_autosaves_.end()) {
        return false;
    }
    sequence_autosaves_.erase(found);
    return true;
}

std::optional<SequenceAutosaveBinding> DocumentSession::TakeSequenceAutosave(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation) noexcept {
    if (std::any_of(
            sequence_autosave_reservations_.cbegin(),
            sequence_autosave_reservations_.cend(),
            [document_uuid_high, document_uuid_low, source_generation](
                const SequenceAutosaveReservation& reservation) {
                return reservation.document_uuid_high == document_uuid_high
                    && reservation.document_uuid_low == document_uuid_low
                    && reservation.source_generation == source_generation;
            })) {
        return std::nullopt;
    }
    const auto found = std::find_if(
        sequence_autosaves_.begin(), sequence_autosaves_.end(),
        [document_uuid_high, document_uuid_low, source_generation](
            const SequenceAutosaveBinding& binding) {
            return binding.document_uuid_high == document_uuid_high
                && binding.document_uuid_low == document_uuid_low
                && binding.source_generation == source_generation;
        });
    if (found == sequence_autosaves_.end()) {
        return std::nullopt;
    }
    SequenceAutosaveBinding retired = std::move(*found);
    sequence_autosaves_.erase(found);
    return retired;
}

std::vector<SequenceAutosaveBinding>
DocumentSession::TakeSequenceAutosaves() noexcept {
    sequence_autosave_reservations_.clear();
    return std::move(sequence_autosaves_);
}

void DocumentSession::ClearSequenceAutosaves() noexcept {
    sequence_autosaves_.clear();
    sequence_autosave_reservations_.clear();
}

bool DocumentSession::ReplaceSequenceFileBindings(
    std::vector<SequenceFileBinding> bindings) noexcept {
    return ReplaceSequenceCatalogBindings(std::move(bindings), {});
}

bool DocumentSession::ReplaceSequenceCatalogBindings(
    std::vector<SequenceFileBinding> bindings,
    std::vector<SequenceResidentAuthority> residents) noexcept {
    if (bindings.size() > 10'000U
        || residents.size() > 64U
        || std::any_of(bindings.cbegin(), bindings.cend(), [](const auto& binding) {
               return (binding.document_uuid_high == 0U
                          && binding.document_uuid_low == 0U)
                   || binding.source_generation == 0U
                   || binding.raster_path.empty() || !binding.raster_identity;
           })
        || std::any_of(residents.cbegin(), residents.cend(),
            [&bindings](const SequenceResidentAuthority& resident) {
                const bool native_path = resident.identity.kind
                        == DocumentIdentityKind::WindowsFile
                    ? !resident.shell.current_path.empty()
                        && resident.shell.planned_native_path.empty()
                    : resident.identity.kind
                            == DocumentIdentityKind::NormalizedPath
                        && resident.shell.current_path.empty()
                        && !resident.shell.planned_native_path.empty();
                return (resident.document_uuid_high == 0U
                           && resident.document_uuid_low == 0U)
                    || resident.source_generation == 0U
                    || !resident.identity || !resident.pair_raster_identity
                    || resident.identity == resident.pair_raster_identity
                    || !native_path || resident.shell.source_path.empty()
                    || resident.shell.pair_raster_path.empty()
                    || std::none_of(bindings.cbegin(), bindings.cend(),
                        [&resident](const SequenceFileBinding& binding) {
                            return binding.document_uuid_high
                                    == resident.document_uuid_high
                                && binding.document_uuid_low
                                    == resident.document_uuid_low
                                && binding.source_generation
                                    == resident.source_generation
                                && binding.raster_path
                                    == resident.shell.pair_raster_path
                                && binding.raster_identity
                                    == resident.pair_raster_identity;
                        });
            })) {
        return false;
    }
    for (std::size_t index = 0U; index < residents.size(); ++index) {
        if (std::any_of(residents.cbegin(), residents.cbegin()
                    + static_cast<std::ptrdiff_t>(index),
                [&candidate = residents[index]](
                    const SequenceResidentAuthority& previous) {
                    return previous.document_uuid_high
                            == candidate.document_uuid_high
                        && previous.document_uuid_low
                            == candidate.document_uuid_low
                        && previous.source_generation
                            == candidate.source_generation;
                })) {
            return false;
        }
    }
    sequence_file_bindings_ = std::move(bindings);
    sequence_resident_authorities_ = std::move(residents);
    return true;
}

const SequenceFileBinding* DocumentSession::SequenceFileBindingAt(
    std::size_t index) const noexcept {
    return index < sequence_file_bindings_.size()
        ? &sequence_file_bindings_[index] : nullptr;
}

bool DocumentSession::UpdateSequenceFileBinding(
    std::size_t index,
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation,
    const std::wstring& raster_path,
    const DocumentIdentity& raster_identity) noexcept {
    if (index >= sequence_file_bindings_.size()
        || (document_uuid_high == 0U && document_uuid_low == 0U)
        || source_generation == 0U || raster_path.empty() || !raster_identity) {
        return false;
    }
    try {
        std::wstring path_candidate = raster_path;
        DocumentIdentity identity_candidate = raster_identity;
        auto& binding = sequence_file_bindings_[index];
        binding.document_uuid_high = document_uuid_high;
        binding.document_uuid_low = document_uuid_low;
        binding.source_generation = source_generation;
        binding.raster_path = std::move(path_candidate);
        binding.raster_identity = std::move(identity_candidate);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool DocumentSession::PublishSequenceFileBinding(
    std::size_t index,
    SequenceFileBinding binding) noexcept {
    if (index >= sequence_file_bindings_.size()
        || (binding.document_uuid_high == 0U
            && binding.document_uuid_low == 0U)
        || binding.source_generation == 0U || binding.raster_path.empty()
        || !binding.raster_identity) {
        return false;
    }
    sequence_file_bindings_[index] = std::move(binding);
    return true;
}

bool DocumentSession::RevokeSequenceFileBinding(
    std::size_t index,
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation) noexcept {
    if (index >= sequence_file_bindings_.size()) {
        return false;
    }
    auto& binding = sequence_file_bindings_[index];
    if (binding.document_uuid_high != document_uuid_high
        || binding.document_uuid_low != document_uuid_low
        || binding.source_generation != source_generation) {
        return false;
    }
    binding.raster_path.clear();
    binding.raster_identity = {};
    return true;
}

void DocumentSession::ClearSequenceFileBindings() noexcept {
    sequence_resident_authorities_.clear();
    sequence_file_bindings_.clear();
}

bool DocumentSession::RetainActiveSequenceResidentAuthority(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation) noexcept {
    if ((document_uuid_high == 0U && document_uuid_low == 0U)
        || source_generation == 0U || !identity) {
        return false;
    }
    try {
        SequenceResidentAuthority candidate{};
        candidate.document_uuid_high = document_uuid_high;
        candidate.document_uuid_low = document_uuid_low;
        candidate.source_generation = source_generation;
        candidate.identity = identity;
        candidate.pair_raster_identity = pair_raster_identity;
        candidate.shell = shell;
        const auto found = std::find_if(
            sequence_resident_authorities_.begin(),
            sequence_resident_authorities_.end(),
            [document_uuid_high, document_uuid_low, source_generation](
                const SequenceResidentAuthority& entry) {
                return entry.document_uuid_high == document_uuid_high
                    && entry.document_uuid_low == document_uuid_low
                    && entry.source_generation == source_generation;
            });
        if (found == sequence_resident_authorities_.end()) {
            sequence_resident_authorities_.push_back(std::move(candidate));
        } else {
            *found = std::move(candidate);
        }
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

const SequenceResidentAuthority* DocumentSession::FindSequenceResidentAuthority(
    std::uint64_t document_uuid_high,
    std::uint64_t document_uuid_low,
    std::uint64_t source_generation) const noexcept {
    const auto found = std::find_if(
        sequence_resident_authorities_.cbegin(),
        sequence_resident_authorities_.cend(),
        [document_uuid_high, document_uuid_low, source_generation](
            const SequenceResidentAuthority& entry) {
            return entry.document_uuid_high == document_uuid_high
                && entry.document_uuid_low == document_uuid_low
                && entry.source_generation == source_generation;
        });
    return found == sequence_resident_authorities_.cend() ? nullptr : &*found;
}

void DocumentSession::ClearSequenceResidentAuthorities() noexcept {
    sequence_resident_authorities_.clear();
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
    ClearIdentityReservation(*current);
    current->BindCore(core);
    current->ClearSequenceAutosaves();
    current->ClearSequenceFileBindings();
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
        if (session != nullptr
            && (session->identity == identity
                || session->pair_raster_identity == identity)) {
            return session.get();
        }
    }
    return nullptr;
}

bool DocumentRegistry::AssignIdentity(
    DocumentSessionId id,
    const DocumentIdentity& identity) noexcept {
    return AssignPairIdentities(id, identity, {});
}

bool DocumentRegistry::AssignPairIdentities(
    DocumentSessionId id,
    const DocumentIdentity& identity,
    const DocumentIdentity& pair_raster_identity) noexcept {
    DocumentSession* session = Find(id);
    const DocumentSession* conflict = FindByIdentity(identity);
    const DocumentSession* pair_conflict = pair_raster_identity
        ? FindByIdentity(pair_raster_identity) : nullptr;
    if (session == nullptr || !identity
        || (conflict != nullptr && conflict != session)
        || HasIdentityReservation(identity, {}, id)
        || (pair_raster_identity
            && (pair_raster_identity == identity
                || (pair_conflict != nullptr && pair_conflict != session)
                || HasIdentityReservation(pair_raster_identity, {}, id)))) {
        return false;
    }
    try {
        DocumentIdentity candidate = identity;
        DocumentIdentity pair_candidate = pair_raster_identity;
        session->identity = std::move(candidate);
        session->pair_raster_identity = std::move(pair_candidate);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

IdentityReservationToken DocumentRegistry::ReserveIdentity(
    DocumentSessionId id,
    const DocumentIdentity& identity,
    const std::wstring& original_path,
    const std::wstring& source_path) noexcept {
    return ReserveIdentityPair(id, identity, {}, original_path, source_path);
}

IdentityReservationToken DocumentRegistry::ReserveIdentityPair(
    DocumentSessionId id,
    const DocumentIdentity& identity,
    const DocumentIdentity& pair_raster_identity,
    const std::wstring& original_path,
    const std::wstring& source_path) noexcept {
    DocumentSession* session = Find(id);
    const DocumentSession* conflict = FindByIdentity(identity);
    const DocumentSession* pair_conflict = pair_raster_identity
        ? FindByIdentity(pair_raster_identity) : nullptr;
    if (session == nullptr || !identity || session->identity_reservation_token_
        || (conflict != nullptr && conflict != session)
        || HasIdentityReservation(identity, {}, id)
        || (pair_raster_identity
            && (pair_raster_identity == identity
                || (pair_conflict != nullptr && pair_conflict != session)
                || HasIdentityReservation(pair_raster_identity, {}, id)))) {
        return {};
    }
    try {
        DocumentIdentity candidate = identity;
        DocumentIdentity pair_candidate = pair_raster_identity;
        std::array<std::wstring, 2U> paths;
        const std::array<const std::wstring*, 2U> inputs{&original_path, &source_path};
        for (std::size_t index = 0U; index < paths.size(); ++index) {
            if (!inputs[index]->empty()
                && !NormalizeDocumentFilePath(*inputs[index], paths[index])) {
                return {};
            }
            if (paths[index].empty()) {
                continue;
            }
            if (HasIdentityReservation(identity, paths[index], id)) {
                return {};
            }
            for (const auto& other : sessions_) {
                if (other == nullptr || other.get() == session) {
                    continue;
                }
                for (const auto* path : {&other->shell.current_path,
                         &other->shell.source_path, &other->shell.planned_native_path,
                         &other->shell.pair_raster_path,
                         &other->shell.recovery_original_path}) {
                    if (path->empty()) {
                        continue;
                    }
                    std::wstring normalized;
                    if (!NormalizeDocumentFilePath(*path, normalized)
                        || normalized == paths[index]) {
                        return {};
                    }
                }
            }
        }
        const IdentityReservationToken token = IssueIdentityReservationToken();
        if (!token) {
            return {};
        }
        session->identity_reservation_token_ = token;
        session->reserved_identity_ = std::move(candidate);
        session->reserved_pair_raster_identity_ = std::move(pair_candidate);
        session->reserved_identity_paths_ = std::move(paths);
        return token;
    } catch (const std::bad_alloc&) {
        return {};
    }
}

bool DocumentRegistry::HasIdentityReservation(
    const DocumentIdentity& identity,
    const std::wstring& normalized_path,
    DocumentSessionId except) const noexcept {
    for (const auto& session : sessions_) {
        if (session == nullptr || session->id == except
            || !session->identity_reservation_token_) {
            continue;
        }
        if (identity && session->reserved_identity_ == identity) {
            return true;
        }
        if (identity && session->reserved_pair_raster_identity_ == identity) {
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

bool DocumentRegistry::PublishReservedIdentity(
    DocumentSessionId id,
    IdentityReservationToken token) noexcept {
    DocumentSession* session = Find(id);
    if (session == nullptr || !token
        || session->identity_reservation_token_ != token
        || !session->reserved_identity_) {
        return false;
    }
    session->identity = std::move(session->reserved_identity_);
    session->pair_raster_identity = std::move(session->reserved_pair_raster_identity_);
    ClearIdentityReservation(*session);
    return true;
}

bool DocumentRegistry::PublishRepairedReservedIdentityPair(
    DocumentSessionId id,
    IdentityReservationToken token,
    DocumentIdentity identity,
    DocumentIdentity pair_raster_identity) noexcept {
    DocumentSession* session = Find(id);
    if (session == nullptr || !token
        || session->identity_reservation_token_ != token
        || !session->reserved_identity_
        || !session->reserved_pair_raster_identity_
        || !identity || !pair_raster_identity) {
        return false;
    }
    session->identity = std::move(identity);
    session->pair_raster_identity = std::move(pair_raster_identity);
    ClearIdentityReservation(*session);
    return true;
}

bool DocumentRegistry::PublishPreparedIdentityPair(
    DocumentSessionId id,
    IdentityReservationToken token,
    DocumentIdentity identity,
    DocumentIdentity pair_raster_identity) noexcept {
    DocumentSession* session = Find(id);
    if (session == nullptr || !token
        || session->identity_reservation_token_ != token
        || !session->reserved_identity_
        || !session->reserved_pair_raster_identity_
        || !identity || !pair_raster_identity) {
        return false;
    }
    session->identity = std::move(identity);
    session->pair_raster_identity = std::move(pair_raster_identity);
    ClearIdentityReservation(*session);
    return true;
}

bool DocumentRegistry::PublishReservedIdentityPairWithSequenceBinding(
    DocumentSessionId id,
    IdentityReservationToken token,
    std::size_t sequence_index,
    SequenceFileBinding binding) noexcept {
    DocumentSession* session = Find(id);
    if (session == nullptr || !token
        || session->identity_reservation_token_ != token
        || !session->reserved_identity_
        || !session->reserved_pair_raster_identity_
        || sequence_index >= session->sequence_file_bindings_.size()
        || (binding.document_uuid_high == 0U
            && binding.document_uuid_low == 0U)
        || binding.source_generation == 0U || binding.raster_path.empty()
        || !binding.raster_identity
        || binding.raster_identity != session->reserved_pair_raster_identity_) {
        return false;
    }
    const auto& current = session->sequence_file_bindings_[sequence_index];
    if (current.document_uuid_high != binding.document_uuid_high
        || current.document_uuid_low != binding.document_uuid_low
        || current.source_generation != binding.source_generation) {
        return false;
    }
    session->identity = std::move(session->reserved_identity_);
    session->pair_raster_identity =
        std::move(session->reserved_pair_raster_identity_);
    session->sequence_file_bindings_[sequence_index] = std::move(binding);
    ClearIdentityReservation(*session);
    return true;
}

bool DocumentRegistry::ForceRevokeIdentity(
    DocumentSessionId id,
    DocumentIdentity identity) noexcept {
    DocumentSession* session = Find(id);
    if (session == nullptr || !identity
        || identity.kind != DocumentIdentityKind::Untitled) {
        return false;
    }
    session->identity = std::move(identity);
    session->pair_raster_identity = {};
    ClearIdentityReservation(*session);
    return true;
}

bool DocumentRegistry::CancelIdentityReservation(
    DocumentSessionId id,
    IdentityReservationToken token) noexcept {
    DocumentSession* session = Find(id);
    if (session == nullptr || !token
        || session->identity_reservation_token_ != token) {
        return false;
    }
    ClearIdentityReservation(*session);
    return true;
}

IdentityReservationToken DocumentRegistry::IssueIdentityReservationToken()
    noexcept {
    if (next_identity_reservation_token_ == 0U) {
        return {};
    }
    const IdentityReservationToken token{next_identity_reservation_token_};
    if (next_identity_reservation_token_ == UINT64_MAX) {
        next_identity_reservation_token_ = 0U;
    } else {
        ++next_identity_reservation_token_;
    }
    return token;
}

void DocumentRegistry::ClearIdentityReservation(DocumentSession& session)
    noexcept {
    session.identity_reservation_token_ = {};
    session.reserved_identity_ = {};
    session.reserved_pair_raster_identity_ = {};
    for (auto& path : session.reserved_identity_paths_) {
        path.clear();
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
