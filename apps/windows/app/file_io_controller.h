#pragma once

#include <windows.h>

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
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
    std::optional<RecoveryMetadata> recovery_metadata;
    std::optional<InkpodSequenceSwitchRequest> sequence_switch;
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
    InkpodIoJobInfo progress{};
    InkpodDocumentInfo document{};
    InkpodSubpaletteInfo subpalette{};
    std::uint64_t object_id{};
    std::vector<FileIoItem> items;
    std::vector<RecoveryCandidate> recovery_candidates;
    std::wstring error;
    // The completion may transfer these handles by exchanging them with null.
    // Otherwise the controller releases them after the completion returns.
    InkpodBatchPreview* batch_preview{};
    InkpodBatchReport* batch_report{};
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
    // Approved write destinations stay reserved until their UI completion.
    [[nodiscard]] bool ConflictsWithPendingWrite(
        const FileIoItem& item, std::uint64_t except_request_id = 0U) const noexcept;
    [[nodiscard]] bool Progress(std::uint64_t request_id, InkpodIoJobInfo& output) const noexcept;
    [[nodiscard]] bool Progress(WorkspaceWindowId workspace, InkpodIoJobInfo& output) const noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::app
