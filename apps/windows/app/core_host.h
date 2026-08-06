#pragma once

#include <windows.h>

#include <cstdint>
#include <functional>
#include <memory>
#include <string>
#include <string_view>
#include <vector>

#include "command_context.h"
#include "inkpod/core_ffi.h"

namespace inkpod::renderer {
class CanvasSnapshotSink;
}

namespace inkpod::app {

inline constexpr UINT kCoreStateChanged = WM_APP + 0x160U;
inline constexpr UINT kCoreAsyncFailed = WM_APP + 0x161U;

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
    StrokeStyle style{};
    std::vector<InkpodStrokeSample> samples;
};

struct EngineMetrics {
    std::uint64_t completed_strokes{};
    std::uint64_t completed_samples{};
    std::uint64_t preview_snapshots{};
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
};

struct CoreNotification {
    std::uint64_t token{};
    CoreNotificationKind kind{CoreNotificationKind::StateChanged};
    CommandContext context;
    InkpodStatus status{INKPOD_STATUS_OK};
};

// Owns every InkpodCore on one engine thread. Public calls capture a
// DocumentSessionId + Generation before queueing and never resolve a later
// active document on the owner thread.
class CoreHost final {
public:
    using CoreOperation = std::function<InkpodStatus(InkpodCore*)>;
    // The application supports up to eight workspace windows with two visible
    // editor groups in each. Only visible editor-group canvases are registered.
    static constexpr std::size_t kMaximumSnapshotSinks = 16U;

    CoreHost();
    ~CoreHost();

    CoreHost(const CoreHost&) = delete;
    CoreHost& operator=(const CoreHost&) = delete;

    InkpodStatus Start(renderer::CanvasSnapshotSink* canvas, HWND owner) noexcept;
    void Stop() noexcept;

    InkpodStatus CreateSession(
        DocumentSessionId session,
        Generation generation) noexcept;
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
    [[nodiscard]] bool HasSession(
        DocumentSessionId session,
        Generation generation) const noexcept;
    [[nodiscard]] std::size_t SessionCount() const noexcept;

    InkpodStatus Invoke(
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept;
    InkpodStatus Invoke(
        DocumentSessionId session,
        Generation generation,
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept;
    InkpodStatus InvokeAll(
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept;
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
        std::function<void(InkpodStatus)> completion = {}) noexcept;
    bool EnqueueStroke(StrokeEvent event) noexcept;
    InkpodStatus WaitIdle() noexcept;
    InkpodStatus WaitIdle(
        DocumentSessionId session,
        Generation generation) noexcept;
    InkpodStatus FlushPreview() noexcept;
    InkpodStatus SetActiveView(std::uint64_t view_id) noexcept;
    bool RetargetNotificationOwner(
        HWND expected_owner,
        HWND replacement_owner) noexcept;
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
