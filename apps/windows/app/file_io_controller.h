#pragma once

#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <span>
#include <string>
#include <vector>

#include "command_context.h"
#include "document_identity.h"
#include "session_recovery.h"
#include "inkpod/core_ffi.h"

namespace inkpod::app {

class CoreHost;

inline constexpr UINT_PTR kFileIoPollTimer = 0x494fU;
inline constexpr UINT kFileIoPollMilliseconds = 50U;

struct FileIoRequest final {
    CommandContext context;
    std::uint32_t kind{};
    std::uint64_t flags{};
    std::vector<std::wstring> paths;
    std::uint64_t object_id{};
    std::uint64_t document_uuid_high{};
    std::uint64_t document_uuid_low{};
    std::uint32_t raster_format{};
    InkpodSubpalette* subpalette{};
    const InkpodBatchGraph* batch_graph{};
    std::uint32_t batch_scope{INKPOD_BATCH_SCOPE_ALL};
    std::uint64_t new_tab_capacity{};
    bool publish_snapshot{};
    // Windows-only activation token, published after Core apply and before its frame.
    std::uint64_t presentation_epoch{};
    std::optional<RecoveryMetadata> recovery_metadata;
    std::optional<InkpodIoRecoveryArtifactProof> target_recovery_proof;
    // Present only for application-owned best-effort cleanup.  The backend
    // removes the pair iff both members still match this exact publication.
    std::optional<InkpodIoRecoveryArtifactProof> discard_recovery_proof;
    std::optional<InkpodSequenceSwitchRequest> sequence_switch;
    // The second sequence-switch path is a raster whose same-stem normal pair
    // must be resolved, rather than a private target recovery artifact.
    bool sequence_target_raster_pair{};
    // Non-authoritative cleanup must not replace a completed primary operation's
    // status or publish a stale local failure.
    bool best_effort_cleanup{};
    // Private product-smoke fault: make the fixed repaired-item refresh fail so
    // the production cancel/final-apply/revoke path is exercised end to end.
    bool smoke_fail_repaired_item_refresh{};
    std::optional<InkpodCompactionPlan> compaction_plan;
};

struct FileIoItem final {
    InkpodIoItemInfo info{};
    std::wstring path;
    std::wstring normalized_path;
    std::wstring name;
    DocumentIdentity identity;
};

struct FileIoResult final {
    std::uint64_t request_id{};
    CommandContext context;
    std::uint32_t kind{};
    InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
    // Separate from durable apply: failed presentation does not undo saved data.
    InkpodStatus presentation_status{INKPOD_STATUS_INVALID_STATE};
    InkpodIoJobInfo progress{};
    InkpodDocumentInfo document{};
    // Exact apply outcome, retained even if later snapshot publication/release fails.
    bool document_applied{};
    InkpodSubpaletteInfo subpalette{};
    std::uint64_t object_id{};
    std::vector<FileIoItem> items;
    std::vector<RecoveryCandidate> recovery_candidates;
    bool has_recovery_artifact_proof{};
    InkpodIoRecoveryArtifactProof recovery_artifact_proof{};
    // Core-enriched metadata from the same successful append-only publication.
    // In particular, Sequence recovery carries its exact normal-pair proof.
    std::optional<RecoveryMetadata> recovery_metadata;
    bool authority_repaired{};
    // Owner finalization proved that a published pair can no longer retain a
    // coherent normal-save authority. Frontend aliases must be revoked.
    bool authority_revoked{};
    std::wstring error;
    // The completion may transfer these handles by exchanging them with null.
    // Otherwise the controller releases them after the completion returns.
    InkpodBatchPreview* batch_preview{};
    InkpodBatchReport* batch_report{};
};

// Caller-owned progress only; contains no borrowed job handle or user path.
struct FileIoProgressEntry final {
    std::uint64_t request_id{};
    CommandContext context;
    InkpodIoJobInfo progress{};
    bool cancelling{};
};

// One application owner, shared by every workspace/session. File bytes and
// decoded pixels never enter this adapter. CoreHost polls accepted Rust jobs;
// Poll dispatches their already-complete UI continuations without waiting.
class FileIoController final {
public:
    using Completion = std::function<void(FileIoResult&&)>;
    using Preflight = std::function<InkpodStatus(const FileIoResult&)>;

    FileIoController();
    ~FileIoController();
    FileIoController(const FileIoController&) = delete;
    FileIoController& operator=(const FileIoController&) = delete;

    [[nodiscard]] InkpodStatus Initialize() noexcept;
    [[nodiscard]] InkpodIoManager* Manager() const noexcept;
    [[nodiscard]] bool Queue(
        CoreHost& engine,
        FileIoRequest request,
        Completion completion,
        std::uint64_t* out_request_id = nullptr,
        Preflight preflight = {}) noexcept;
    void Poll() noexcept;
    void Cancel(std::uint64_t request_id) noexcept;
    void CancelSession(DocumentSessionId session, Generation generation) noexcept;
    void CancelWorkspace(WorkspaceWindowId workspace) noexcept;
    void CancelAll() noexcept;
    // Call only after CoreHost has drained accepted continuations.
    void ClearCompleted() noexcept;
    // UI continuation used by close workflows. Empty context waits for all
    // jobs; a session or workspace context narrows the scope. Never blocks.
    [[nodiscard]] bool WhenIdle(CommandContext context, std::function<void()> completion) noexcept;
    [[nodiscard]] bool HasPending() const noexcept;
    [[nodiscard]] bool HasPending(WorkspaceWindowId workspace) const noexcept;
    [[nodiscard]] bool HasPending(DocumentSessionId session, Generation generation) const noexcept;
    [[nodiscard]] bool HasPendingKind(
        DocumentSessionId session,
        Generation generation,
        std::uint32_t kind) const noexcept;
    // Approved write destinations stay reserved until their UI completion.
    [[nodiscard]] bool ConflictsWithPendingWrite(
        const FileIoItem& item, std::uint64_t except_request_id = 0U) const noexcept;
    // Prepared paired opens are not yet published DocumentRegistry sessions.
    // Keep both resolved members authoritative here until their UI completion.
    [[nodiscard]] bool ConflictsWithPendingAuthority(
        const FileIoItem& item, std::uint64_t except_request_id = 0U) const noexcept;
    [[nodiscard]] bool Progress(std::uint64_t request_id, InkpodIoJobInfo& output) const noexcept;
    [[nodiscard]] bool Progress(WorkspaceWindowId workspace, InkpodIoJobInfo& output) const noexcept;
    // UI-thread query of the CoreHost-published cache, with no Rust polling or
    // file I/O. Copies matching issued workspaces in request order, up to the
    // caller's capacity (the controller accepts at most 128 jobs). The return
    // value is the copied count; remaining elements are unchanged. Entries
    // remain present until Poll dispatches their UI continuation.
    [[nodiscard]] std::size_t CopyProgress(
        WorkspaceWindowId workspace, std::span<FileIoProgressEntry> output) const noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::app
