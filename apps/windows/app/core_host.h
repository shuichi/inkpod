#pragma once

#include <windows.h>

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "command_context.h"
#include "inkscript_engine_route.h"
#include "inkpod/core_ffi.h"

namespace inkpod::renderer {
class CanvasSnapshotSink;
}

namespace inkpod::app {

enum class ScrollRangeResetScope : std::uint8_t {
    None,
    TargetView,
    SessionViews,
};

struct ScrollRangeResetRequest final {
    ScrollRangeResetScope scope{ScrollRangeResetScope::None};
    std::uint64_t core_view_id{};
};

inline constexpr UINT kCoreStateChanged = WM_APP + 0x160U;
inline constexpr UINT kCoreAsyncFailed = WM_APP + 0x161U;
inline constexpr UINT kCoreInkScriptNotification = WM_APP + 0x162U;

enum class StrokeEventKind : std::uint32_t {
    Begin,
    Append,
    End,
    Cancel,
};

struct StrokeStyle {
    InkpodCoordinateSpace coordinate_space{INKPOD_COORDINATE_SPACE_DEVICE};
    std::uint64_t flags{};
};

struct StrokeEvent {
    StrokeEventKind kind{StrokeEventKind::Cancel};
    CommandContext context;
    std::uint64_t core_view_id{};
    StrokeStyle style{};
    std::vector<InkpodStrokeSample> samples;
};

struct EngineMetrics {
    std::uint64_t completed_strokes{};
    std::uint64_t completed_samples{};
    std::uint64_t preview_snapshots{};
    std::uint64_t submitted_snapshots{};
    std::uint64_t accepted_work_items{};
    std::uint64_t rejected_work_items{};
    std::uint64_t queue_wait_samples{};
    std::uint64_t total_queue_wait_microseconds{};
    std::uint64_t maximum_queue_wait_microseconds{};
    std::uint64_t peak_pending_operations{};
};

struct CoreSessionState {
    Generation generation{};
    std::uint64_t last_accepted_sequence{};
    std::uint64_t last_completed_sequence{};
    std::uint64_t pending_operations{};
    bool accepting_work{};
    bool stroke_active{};
};

enum class CoreNotificationKind : std::uint8_t {
    StateChanged,
    AsyncFailed,
    InkScript,
};

struct CoreNotification {
    std::uint64_t token{};
    CoreNotificationKind kind{CoreNotificationKind::StateChanged};
    CommandContext context;
    InkpodStatus status{INKPOD_STATUS_OK};
    InkScriptEngineResult inkscript;
};

// Owns every InkpodCore on one engine thread. Public calls capture a
// DocumentSessionId + Generation before queueing and never resolve a later
// active document on the owner thread.
class CoreHost final {
public:
    using CoreOperation = std::function<InkpodStatus(InkpodCore*)>;
    using SubpaletteOperation =
        std::function<InkpodStatus(InkpodSubpalette*)>;
    // Runs one nonblocking Rust I/O submit/poll/apply step on the owner thread.
    // PENDING retains the operation. Installing fences ordinary document work
    // until this same operation finalizes, including during shutdown.
    using FileIoOperation =
        std::function<InkpodStatus(InkpodCore*, bool, bool&, bool&)>;
    // First status is the durable apply result. The second includes subsequent
    // published-state/snapshot failure, so callers can retry presentation without
    // repeating a successful save/open/install. Both run on the owner thread.
    using FileIoCompletion = std::function<void(InkpodStatus, InkpodStatus)>;
    // The application supports up to eight workspace windows with two visible
    // editor groups in each. Only visible editor-group canvases are registered.
    static constexpr std::size_t kMaximumSnapshotSinks = 16U;
    static constexpr std::size_t kMaximumDocumentSessions = 64U;

    CoreHost();
    ~CoreHost();

    CoreHost(const CoreHost&) = delete;
    CoreHost& operator=(const CoreHost&) = delete;

    InkpodStatus Start(
        renderer::CanvasSnapshotSink* canvas,
        HWND owner,
        InkpodIoManager* io_manager = nullptr) noexcept;
    void Stop() noexcept;
    [[nodiscard]] InkpodIoManager* IoManager() const noexcept;

    InkpodStatus CreateSession(
        DocumentSessionId session,
        Generation generation) noexcept;
    InkpodStatus AdoptBatchResult(
        DocumentSessionId session,
        Generation generation,
        InkpodBatchReport* report,
        std::uint64_t result_index) noexcept;
    InkpodStatus RebindSession(
        DocumentSessionId old_session,
        Generation old_generation,
        DocumentSessionId new_session,
        Generation new_generation) noexcept;
    InkpodStatus CloseSession(
        DocumentSessionId session,
        Generation generation) noexcept;
    bool SetActiveSession(
        DocumentSessionId session,
        Generation generation) noexcept;
    void ClearActiveSession() noexcept;
    [[nodiscard]] bool HasSession(
        DocumentSessionId session,
        Generation generation) const noexcept;
    [[nodiscard]] std::size_t SessionCount() const noexcept;

    InkpodStatus Invoke(
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info,
        ScrollRangeResetRequest scroll_range_reset = {}) noexcept;
    InkpodStatus Invoke(
        DocumentSessionId session,
        Generation generation,
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info,
        ScrollRangeResetRequest scroll_range_reset = {}) noexcept;
    // Dispatch a session-independent operation (for example an application query)
    // on the Core owner thread, including while no document session exists.
    InkpodStatus InvokeOwnerThread(std::function<InkpodStatus()> operation) noexcept;
    InkpodStatus InvokeAll(
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept;
    // These calls dispatch directly to the shared Core engine owner thread.
    // The subpalette handle has no document target and never follows the active
    // document.
    InkpodStatus CreateSubpalette(InkpodSubpalette** out_subpalette) noexcept;
    InkpodStatus InvokeSubpalette(
        InkpodSubpalette* subpalette,
        SubpaletteOperation operation) noexcept;
    // Returns after queueing. completion runs on the Core owner thread, so it
    // must transfer only pointer-free completion state back to the UI thread.
    bool EnqueueSubpalette(
        InkpodSubpalette* subpalette,
        SubpaletteOperation operation,
        std::function<void(InkpodStatus)> completion) noexcept;
    InkpodStatus ReleaseSubpalette(InkpodSubpalette** subpalette) noexcept;
    InkpodStatus InvokePrimitive(
        const InkpodPrimitiveRequestV3& request,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke = true) noexcept;
    InkpodStatus InvokePrimitive(
        DocumentSessionId session,
        Generation generation,
        const InkpodPrimitiveRequestV3& request,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke = true) noexcept;
    bool EnqueuePrimitive(
        const CommandContext& context,
        const InkpodPrimitiveRequestV3& request,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke = true) noexcept;
    bool Enqueue(
        const CommandContext& context,
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke,
        std::function<void(InkpodStatus)> completion = {},
        ScrollRangeResetRequest scroll_range_reset = {}) noexcept;
    bool EnqueueFileIo(
        const CommandContext& context,
        bool requires_core,
        FileIoOperation operation,
        bool publish_snapshot,
        bool refresh_document_info,
        FileIoCompletion completion) noexcept;
    // Captures one private InkScript request without synchronously waiting for
    // parse/compile/plan/run. PlanReady and terminal results are delivered as
    // pointer-free kCoreInkScriptNotification values.
    bool EnqueueInkScript(InkScriptEngineRequest request) noexcept;
    bool ConfirmInkScript(
        std::uint64_t job_id,
        const CommandContext& context,
        std::uint32_t scope) noexcept;
    bool CancelInkScript(
        std::uint64_t job_id,
        const CommandContext& context) noexcept;
    bool EnqueueStroke(StrokeEvent event) noexcept;
    InkpodStatus WaitIdle() noexcept;
    InkpodStatus WaitIdle(
        DocumentSessionId session,
        Generation generation) noexcept;
    InkpodStatus FlushPreview() noexcept;
    // Repeating the applied view avoids queueing and snapshot publication even
    // while unrelated document work is pending. Queued view changes retain order.
    InkpodStatus SetActiveView(std::uint64_t view_id) noexcept;
    // A new Canvas route needs a fresh publication even for the same Core view.
    // This short metadata update never waits for an in-flight snapshot build;
    // an older publication cannot acknowledge a newer invalidation.
    bool InvalidateViewPublication(
        DocumentSessionId session, Generation generation) noexcept;
    // Core-owner-thread only: call after installing the navigation result and
    // before publishing its snapshot. The nonzero UI token is not a Core,
    // render-cache, or persistent revision. Zero clears continuity after an
    // unrelated document replacement. Wrong thread or binding fails.
    bool SetPresentationEpoch(
        DocumentSessionId session, Generation generation, std::uint64_t epoch) noexcept;
    bool RetargetNotificationOwner(
        HWND expected_owner,
        HWND replacement_owner) noexcept;
    // Posts a value-only private WM_APP notification to the current owner,
    // including after a workspace handoff. Failure leaves caller-owned result
    // storage untouched so the UI can poll or clean it up later.
    bool PostCompletionNotification(
        UINT message, std::uint64_t token, Generation generation) noexcept;
    // Borrowed until Unregister returns. Unregistration waits for in-flight
    // publication; ordinary state queries and input acceptance do not wait for it.
    bool RegisterSnapshotSink(renderer::CanvasSnapshotSink* canvas) noexcept;
    bool UnregisterSnapshotSink(renderer::CanvasSnapshotSink* canvas) noexcept;
    bool UnregisterSnapshotSinks(
        renderer::CanvasSnapshotSink* const* canvases,
        std::size_t count) noexcept;
    [[nodiscard]] std::size_t SnapshotSinkCount() const noexcept;
    bool RegisterDocumentView(
        DocumentSessionId session,
        Generation generation,
        DocumentViewId frontend_view,
        std::uint64_t core_view_id) noexcept;
    bool UnregisterDocumentView(
        DocumentSessionId session,
        Generation generation,
        DocumentViewId frontend_view) noexcept;

    bool GetDocumentInfo(InkpodDocumentInfo& info) const noexcept;
    bool GetDocumentInfo(
        DocumentSessionId session,
        Generation generation,
        InkpodDocumentInfo& info) const noexcept;
    // Cached owner-thread metadata; independent of the active/pinned UI pane.
    bool GetSequenceCellName(
        DocumentSessionId session,
        Generation generation,
        std::wstring& name) const noexcept;
    bool GetSequenceCatalog(
        DocumentSessionId session,
        Generation generation,
        InkpodSequenceCatalogInfo& info) const noexcept;
    // Rust defaults remain available after the last document session closes.
    InkpodStatus GetApplicationEditorDefaults(
        InkpodEditorDefaults& defaults) const noexcept;
    bool GetHistoryPresentation(
        DocumentSessionId session,
        Generation generation,
        InkpodHistoryInfo& info,
        InkpodHistoryEntryKind& undo_kind,
        InkpodHistoryEntryKind& redo_kind) const noexcept;
    bool GetEditTargetPresentation(
        DocumentSessionId session,
        Generation generation,
        std::uint64_t& target_count,
        InkpodEditTargetCapabilities& capabilities) const noexcept;
    bool GetSnapshotTransform(
        DocumentSessionId session,
        Generation generation,
        std::uint64_t view_id,
        InkpodSnapshotTransform& transform) const noexcept;
    InkpodStatus GetReplayContract(
        DocumentSessionId session,
        Generation generation,
        InkpodReplayContract& contract) noexcept;
    InkpodStatus GetPersistenceInfo(
        DocumentSessionId session,
        Generation generation,
        InkpodPersistenceInfo& info) noexcept;
    InkpodStatus GetCompactionPlan(
        DocumentSessionId session,
        Generation generation,
        InkpodCompactionPlan& plan) noexcept;
    InkpodStatus WriteCompactedCopy(
        DocumentSessionId session,
        Generation generation,
        std::string_view path_utf8,
        const InkpodCompactionPlan& plan) noexcept;
    InkpodStatus GetEditorDefaults(
        DocumentSessionId session,
        Generation generation,
        InkpodEditorDefaults& defaults) noexcept;
    InkpodStatus RefreshEditorState(
        DocumentSessionId session,
        Generation generation) noexcept;
    InkpodStatus UpdateEditorState(
        DocumentSessionId session,
        Generation generation,
        const InkpodEditorStateUpdate& update) noexcept;
    bool GetEditorState(
        DocumentSessionId session,
        Generation generation,
        InkpodEditorStateInfo& state) const noexcept;
    InkpodStatus GetEditTargets(
        DocumentSessionId session,
        Generation generation,
        std::vector<InkpodEditTarget>& targets) noexcept;
    InkpodStatus GetEditTargetCapabilities(
        DocumentSessionId session,
        Generation generation,
        InkpodEditTargetCapabilities& capabilities) noexcept;
    InkpodStatus SetEditTargets(
        DocumentSessionId session,
        Generation generation,
        std::uint64_t expected_editor_revision,
        const std::vector<InkpodEditTarget>& targets) noexcept;
    InkpodStatus ApplyEditTargetCommand(
        DocumentSessionId session,
        Generation generation,
        const InkpodEditTargetCommand& command,
        InkpodDispatchResult& result,
        std::vector<InkpodEditTarget>& output_targets) noexcept;
    InkpodStatus RegisterColorArray(
        DocumentSessionId session,
        Generation generation,
        const InkpodColorArray& input,
        InkpodObjectId& object_id) noexcept;
    InkpodStatus ReleaseObject(
        DocumentSessionId session,
        Generation generation,
        const InkpodObjectId& object_id) noexcept;
    std::wstring LastError() const;
    std::wstring LastError(
        DocumentSessionId session,
        Generation generation) const;
    void SetLocalFailure(std::wstring_view message) noexcept;
    EngineMetrics Metrics() const noexcept;
    bool GetMetrics(
        DocumentSessionId session,
        Generation generation,
        EngineMetrics& metrics) const noexcept;
    bool GetSessionState(
        DocumentSessionId session,
        Generation generation,
        CoreSessionState& state) const noexcept;
    DWORD ThreadId() const noexcept;

    void SetSessionInitializer(CoreOperation initializer) noexcept;
    bool TakeNotification(
        std::uint64_t token,
        Generation generation,
        CoreNotification& notification) noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::app
