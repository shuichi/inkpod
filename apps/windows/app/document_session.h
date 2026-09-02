#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "document_identity.h"
#include "frontend_state.h"
#include "session_recovery.h"

namespace inkpod::app {

class CoreHost;

[[nodiscard]] bool ValidRecoveryArtifactProof(
    const InkpodIoRecoveryArtifactProof& proof) noexcept;

struct DocumentView final {
    DocumentViewId id{};
    std::uint64_t core_view_id{};
    Generation generation{};
    ViewUiState presentation{};
};

struct SequenceAutosaveBinding final {
    std::uint64_t document_uuid_high{};
    std::uint64_t document_uuid_low{};
    std::uint64_t source_generation{};
    std::uint64_t artifact_generation{};
    std::wstring recovery_path;
    RecoveryMetadata metadata{};
    InkpodIoRecoveryArtifactProof artifact_proof{};
    InkpodCommonRasterFormat raster_format_hint{};
};

// Runtime-only source authority for one Core sequence entry. The vector order
// is the exact natural order published by the same successful I/O result that
// installed the Core catalog; names or pane selection are never used as paths.
struct SequenceFileBinding final {
    std::uint64_t document_uuid_high{};
    std::uint64_t document_uuid_low{};
    std::uint64_t source_generation{};
    std::wstring raster_path;
    DocumentIdentity raster_identity{};
};

// Registry-issued owner capability for one identity reservation. Zero is
// invalid. Values are process-runtime only, monotonically issued by one
// DocumentRegistry, and are never reused during that registry's lifetime.
struct IdentityReservationToken final {
    std::uint64_t value{};

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return value != 0U;
    }

    friend constexpr bool operator==(
        IdentityReservationToken,
        IdentityReservationToken) noexcept = default;
};

class DocumentSession final {
public:
    static constexpr std::size_t kMaximumViews = 64U;

    DocumentSessionId id{};
    Generation generation{};
    DocumentIdentity identity{};
    DocumentIdentity pair_raster_identity{};
    std::uint32_t untitled_number{};
    DocumentShellState shell{};
    InkpodEditorStateInfo editor_presentation{sizeof(InkpodEditorStateInfo)};
    bool has_editor_presentation{};
    HWND history_visualization_dialog{};
    // Nonpersistent result of the latest automatic sequence discovery.
    bool auto_sequence_truncated{};
    // Runtime input fence shared by every view of this session. No new editing
    // command may target a replacement before its correct image is presented.
    bool sequence_activation_pending{};
    std::uint64_t sequence_required_present_revision{};
    // Runtime activation token: recovered documents may reuse an older revision.
    std::uint64_t sequence_required_present_epoch{};
    // Frontend publication fence for recovery artifacts.  It advances after a
    // normal save or a same-session document replacement and is never reset by
    // changing path authority, so a late autosave cannot attach to newer Core
    // contents merely because its artifact generation happens to match again.
    std::uint64_t persistence_epoch{1U};
    // Last-writer-wins fence for automatic discovery versus an explicit
    // sequence import.  A completion may publish file bindings only while the
    // issue-time intent is still current.
    std::uint64_t sequence_catalog_intent_epoch{1U};

    [[nodiscard]] bool AdvancePersistenceEpoch() noexcept;
    [[nodiscard]] bool AdvanceSequenceCatalogIntentEpoch() noexcept;
    // A duplicate open may reuse this session only when both authorities are
    // the same logical native/raster pair.  A one-member overlap is a file
    // conflict, even when that member is the same physical file.
    [[nodiscard]] bool HasExactPairIdentities(
        const DocumentIdentity& native_identity,
        const DocumentIdentity& raster_identity) const noexcept;

    // UI owner only. The caller first validates the complete Canvas route and
    // supplies only the renderer's last successful-Present telemetry. This
    // one-shot acknowledgement survives hiding/rebinding that Canvas, while
    // pending activation, another epoch, or another session/generation closes it.
    [[nodiscard]] bool AcknowledgeSequencePresentation(
        DocumentSessionId presented_session,
        Generation presented_generation,
        DocumentViewId presented_view,
        std::uint64_t presented_document_revision,
        std::uint64_t presented_epoch) noexcept;
    [[nodiscard]] bool HasSequencePresentationAcknowledgement() const noexcept;

    void BindCore(CoreHost* host) noexcept;
    [[nodiscard]] CoreHost* Core() const noexcept;

    void ResetViews(
        DocumentViewId initial_view,
        Generation view_generation,
        std::uint64_t core_view_id = 0U) noexcept;
    [[nodiscard]] bool AddView(
        DocumentViewId view,
        Generation view_generation,
        std::uint64_t core_view_id) noexcept;
    [[nodiscard]] bool RemoveView(DocumentViewId view) noexcept;
    [[nodiscard]] bool ActivateView(DocumentViewId view) noexcept;
    [[nodiscard]] bool ActivateCoreView(std::uint64_t core_view_id) noexcept;
    [[nodiscard]] DocumentView* FindView(DocumentViewId view) noexcept;
    [[nodiscard]] const DocumentView* FindView(DocumentViewId view) const noexcept;
    [[nodiscard]] DocumentView* FindCoreView(std::uint64_t core_view_id) noexcept;
    [[nodiscard]] const DocumentView* FindCoreView(
        std::uint64_t core_view_id) const noexcept;
    [[nodiscard]] DocumentView* ActiveView() noexcept;
    [[nodiscard]] const DocumentView* ActiveView() const noexcept;
    [[nodiscard]] DocumentView* ViewAt(std::size_t index) noexcept;
    [[nodiscard]] const DocumentView* ViewAt(std::size_t index) const noexcept;
    [[nodiscard]] std::size_t ViewCount() const noexcept;
    [[nodiscard]] const SequenceAutosaveBinding* FindSequenceAutosave(
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation) const noexcept;
    [[nodiscard]] bool PublishSequenceAutosave(
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation,
        const std::wstring& recovery_path,
        const RecoveryMetadata& metadata,
        const InkpodIoRecoveryArtifactProof& artifact_proof) noexcept;
    [[nodiscard]] bool ReserveSequenceAutosave(
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation,
        std::uint64_t expected_artifact_generation) noexcept;
    [[nodiscard]] bool PublishReservedSequenceAutosave(
        SequenceAutosaveBinding binding,
        std::uint64_t expected_artifact_generation) noexcept;
    void CancelSequenceAutosaveReservation(
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation,
        std::uint64_t expected_artifact_generation) noexcept;
    [[nodiscard]] bool RemoveSequenceAutosave(
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation,
        std::uint64_t artifact_generation) noexcept;
    // Retires exactly one source generation while preserving every inactive
    // cell's recovery association. Used by active-document Revert.
    [[nodiscard]] std::optional<SequenceAutosaveBinding> TakeSequenceAutosave(
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation) noexcept;
    [[nodiscard]] std::vector<SequenceAutosaveBinding>
        TakeSequenceAutosaves() noexcept;
    void ClearSequenceAutosaves() noexcept;
    [[nodiscard]] bool ReplaceSequenceFileBindings(
        std::vector<SequenceFileBinding> bindings) noexcept;
    [[nodiscard]] const SequenceFileBinding* SequenceFileBindingAt(
        std::size_t index) const noexcept;
    [[nodiscard]] bool UpdateSequenceFileBinding(
        std::size_t index,
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation,
        const std::wstring& raster_path,
        const DocumentIdentity& raster_identity) noexcept;
    // Caller prepared every allocating field before the durable save commit.
    [[nodiscard]] bool PublishSequenceFileBinding(
        std::size_t index,
        SequenceFileBinding binding) noexcept;
    // A failed published pair save revoked this active entry's path authority.
    // Preserve its stable catalog key for a pathless recovery association, but
    // clear the stale path/identity alias without allocating.
    [[nodiscard]] bool RevokeSequenceFileBinding(
        std::size_t index,
        std::uint64_t document_uuid_high,
        std::uint64_t document_uuid_low,
        std::uint64_t source_generation) noexcept;
    void ClearSequenceFileBindings() noexcept;

private:
    friend class DocumentRegistry;

    CoreHost* core_{};
    DocumentSessionId sequence_presented_session_{};
    Generation sequence_presented_generation_{};
    std::uint64_t sequence_presented_revision_{};
    std::uint64_t sequence_presented_epoch_{};
    std::array<DocumentView, kMaximumViews> views_{};
    std::array<bool, kMaximumViews> view_used_{};
    std::size_t view_count_{};
    DocumentViewId active_view_{};
    std::vector<SequenceAutosaveBinding> sequence_autosaves_;
    std::uint64_t reserved_sequence_document_uuid_high_{};
    std::uint64_t reserved_sequence_document_uuid_low_{};
    std::uint64_t reserved_sequence_source_generation_{};
    std::uint64_t reserved_sequence_artifact_generation_{};
    bool sequence_autosave_reservation_active_{};
    std::vector<SequenceFileBinding> sequence_file_bindings_;
    IdentityReservationToken identity_reservation_token_{};
    DocumentIdentity reserved_identity_{};
    DocumentIdentity reserved_pair_raster_identity_{};
    std::array<std::wstring, 2U> reserved_identity_paths_{};
};

class DocumentRegistry final {
public:
    static constexpr std::size_t kMaximumSessions = 64U;

    [[nodiscard]] bool InitializePlaceholder(Generation generation) noexcept;
    [[nodiscard]] bool Replace(
        DocumentSessionId id,
        Generation generation,
        DocumentViewId initial_view,
        CoreHost* core) noexcept;
    [[nodiscard]] bool Add(
        DocumentSessionId id,
        Generation generation,
        DocumentViewId initial_view,
        CoreHost* core) noexcept;
    [[nodiscard]] bool Remove(DocumentSessionId id) noexcept;
    [[nodiscard]] bool Activate(DocumentSessionId id) noexcept;
    void ClearActive() noexcept;
    [[nodiscard]] DocumentSession* Find(DocumentSessionId id) noexcept;
    [[nodiscard]] const DocumentSession* Find(DocumentSessionId id) const noexcept;
    [[nodiscard]] DocumentSession* FindByView(DocumentViewId view) noexcept;
    [[nodiscard]] const DocumentSession* FindByView(
        DocumentViewId view) const noexcept;
    [[nodiscard]] DocumentSession* FindByIdentity(
        const DocumentIdentity& identity) noexcept;
    [[nodiscard]] const DocumentSession* FindByIdentity(
        const DocumentIdentity& identity) const noexcept;
    [[nodiscard]] bool AssignIdentity(
        DocumentSessionId id,
        const DocumentIdentity& identity) noexcept;
    [[nodiscard]] bool AssignPairIdentities(
        DocumentSessionId id,
        const DocumentIdentity& identity,
        const DocumentIdentity& pair_raster_identity) noexcept;
    // Reserve a prepared replacement without changing the active identity.
    // Paths cover a physical file whose identity changes after atomic replace.
    [[nodiscard]] IdentityReservationToken ReserveIdentity(
        DocumentSessionId id,
        const DocumentIdentity& identity,
        const std::wstring& original_path = {},
        const std::wstring& source_path = {}) noexcept;
    [[nodiscard]] IdentityReservationToken ReserveIdentityPair(
        DocumentSessionId id,
        const DocumentIdentity& identity,
        const DocumentIdentity& pair_raster_identity,
        const std::wstring& original_path = {},
        const std::wstring& source_path = {}) noexcept;
    [[nodiscard]] bool HasIdentityReservation(
        const DocumentIdentity& identity,
        const std::wstring& normalized_path = {},
        DocumentSessionId except = {}) const noexcept;
    // Publication only moves previously allocated values; no allocation. The
    // exact nonzero token returned by ReserveIdentity[Pair] is mandatory.
    [[nodiscard]] bool PublishReservedIdentity(
        DocumentSessionId id,
        IdentityReservationToken token) noexcept;
    // A failed same-target pair install may restore old bytes under new file
    // IDs. The exact path pair is already reserved; publish the refreshed
    // authorities without another lookup or allocation.
    [[nodiscard]] bool PublishRepairedReservedIdentityPair(
        DocumentSessionId id,
        IdentityReservationToken token,
        DocumentIdentity identity,
        DocumentIdentity pair_raster_identity) noexcept;
    // Publish an already conflict-checked pair by move. The caller prepared
    // both identities before Core apply, so no allocation or lookup is needed
    // after a durable commit; the reservation must still belong to this exact
    // operation token.
    [[nodiscard]] bool PublishPreparedIdentityPair(
        DocumentSessionId id,
        IdentityReservationToken token,
        DocumentIdentity identity,
        DocumentIdentity pair_raster_identity) noexcept;
    // A sequence activation or Revert may preserve the runtime catalog while
    // replacing the live document and its physical pair identities. Validate
    // the exact reservation and prepared target-cell stable key before
    // publishing any member; on success both logical identities and the slot
    // binding are moved together without allocation.
    [[nodiscard]] bool PublishReservedIdentityPairWithSequenceBinding(
        DocumentSessionId id,
        IdentityReservationToken token,
        std::size_t sequence_index,
        SequenceFileBinding binding) noexcept;
    // Owner finalization crossed pair publication but could not retain or
    // repair its authority. Replace both file aliases with the prepared
    // pathless identity without allocating. This deliberately does not depend
    // on a still-visible reservation: a terminal Core revoke is authoritative
    // even if a frontend reservation invariant was already disturbed.
    [[nodiscard]] bool ForceRevokeIdentity(
        DocumentSessionId id,
        DocumentIdentity identity) noexcept;
    [[nodiscard]] bool CancelIdentityReservation(
        DocumentSessionId id,
        IdentityReservationToken token) noexcept;
    void ClearCoreBindings() noexcept;
    void Clear() noexcept;
    [[nodiscard]] DocumentSession* Current() noexcept;
    [[nodiscard]] const DocumentSession* Current() const noexcept;
    [[nodiscard]] DocumentSession* SessionAt(std::size_t index) noexcept;
    [[nodiscard]] const DocumentSession* SessionAt(
        std::size_t index) const noexcept;
    [[nodiscard]] std::size_t Count() const noexcept;

private:
    [[nodiscard]] IdentityReservationToken IssueIdentityReservationToken()
        noexcept;
    static void ClearIdentityReservation(DocumentSession& session) noexcept;

    std::array<std::unique_ptr<DocumentSession>, kMaximumSessions> sessions_{};
    std::size_t current_index_{kMaximumSessions};
    std::size_t count_{};
    // Zero is the permanently exhausted state. Clear/Replace never reset this
    // counter, which prevents an old asynchronous owner capability from
    // becoming valid again through ABA.
    std::uint64_t next_identity_reservation_token_{1U};
};

}  // namespace inkpod::app
