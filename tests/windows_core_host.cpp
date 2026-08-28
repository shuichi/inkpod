#include <windows.h>

#include <array>
#include <atomic>
#include <chrono>
#include <cstdint>
#include <future>
#include <memory>
#include <string>
#include <thread>
#include <vector>

#include "app/core_host.h"
#include "renderer/canvas.h"

namespace {

using inkpod::app::CommandContext;
using inkpod::app::CanvasId;
using inkpod::app::CoreHost;
using inkpod::app::CoreNotification;
using inkpod::app::CoreNotificationKind;
using inkpod::app::CoreSessionState;
using inkpod::app::DocumentSessionId;
using inkpod::app::EngineMetrics;
using inkpod::app::Generation;
using inkpod::app::StrokeEvent;
using inkpod::app::StrokeEventKind;

class SnapshotSink final : public inkpod::renderer::CanvasSnapshotSink {
public:
    void Bind(
        DocumentSessionId session,
        Generation generation,
        inkpod::app::DocumentViewId view = inkpod::app::DocumentViewId{21U},
        CanvasId canvas = CanvasId{31U}) noexcept {
        route_ = inkpod::renderer::SnapshotRoute{
            session,
            view,
            canvas,
            generation,
            Generation(1U)};
    }

    inkpod::renderer::SnapshotRoute Route() const noexcept override {
        return route_;
    }

    bool AcceptsSnapshots() const noexcept override {
        return static_cast<bool>(route_);
    }

    bool Submit(inkpod::renderer::SnapshotEnvelope envelope) noexcept override {
        if (envelope.snapshot == nullptr || envelope.route != route_) {
            if (envelope.snapshot != nullptr) {
                inkpod_snapshot_release(&envelope.snapshot);
            }
            return false;
        }
        InkpodSnapshotView view{};
        view.struct_size = sizeof(view);
        InkpodSnapshotTransform transform{};
        transform.struct_size = sizeof(transform);
        InkpodCanonicalDigest digest{};
        digest.struct_size = sizeof(digest);
        if (inkpod_snapshot_get_view(envelope.snapshot, &view) != INKPOD_STATUS_OK
            || inkpod_snapshot_get_transform(envelope.snapshot, &transform)
                != INKPOD_STATUS_OK
            || inkpod_snapshot_get_canonical_digest(envelope.snapshot, &digest)
                != INKPOD_STATUS_OK
            || digest.algorithm != INKPOD_DIGEST_BLAKE3_256) {
            inkpod_snapshot_release(&envelope.snapshot);
            return false;
        }
        last_revision.store(view.revision, std::memory_order_release);
        last_pan_x.store(transform.pan_x, std::memory_order_release);
        last_digest_byte.store(digest.bytes[0], std::memory_order_release);
        ++submitted;
        return inkpod_snapshot_release(&envelope.snapshot) == INKPOD_STATUS_OK;
    }

    std::atomic<std::uint64_t> submitted{};
    std::atomic<std::uint64_t> last_revision{};
    std::atomic<double> last_pan_x{};
    std::atomic<std::uint8_t> last_digest_byte{};

private:
    inkpod::renderer::SnapshotRoute route_{};
};

InkpodDocumentInfo EmptyDocumentInfo() noexcept {
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return info;
}

InkpodStatus NewCell(
    InkpodCore* core,
    std::uint64_t uuid,
    std::uint32_t width,
    std::uint32_t height) noexcept {
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4755493300000000) | uuid,
        UINT64_C(0x434f524500000000) | uuid,
        width,
        height,
        96000U,
        96000U};
    InkpodDocumentInfo info = EmptyDocumentInfo();
    return inkpod_core_new_cell(core, &options, &info);
}

InkpodStatus ApplyMark(InkpodCore* core, float x, float y) noexcept {
    const InkpodStrokeSample sample{
        sizeof(InkpodStrokeSample), 0U, x, y, 1.0F, 0U};
    const InkpodStrokeInput input{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x204080ff),
        3.0F,
        &sample,
        1U,
        sizeof(InkpodStrokeSample),
        INKPOD_BRUSH_ROUND,
        0U,
        0U,
        INKPOD_START_COLOR_ANY,
        0U};
    InkpodDispatchResult result{};
    result.struct_size = sizeof(result);
    return inkpod_core_apply_stroke(core, &input, &result);
}

bool ToUtf8(const std::wstring& value, std::vector<std::uint8_t>& output) {
    const int required = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        value.data(),
        static_cast<int>(value.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (required <= 0) {
        return false;
    }
    output.resize(static_cast<std::size_t>(required));
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               value.data(),
               static_cast<int>(value.size()),
               reinterpret_cast<char*>(output.data()),
               required,
               nullptr,
               nullptr)
        == required;
}

bool TemporaryPath(std::wstring& output) {
    std::array<wchar_t, MAX_PATH> directory{};
    std::array<wchar_t, MAX_PATH> path{};
    const DWORD length = GetTempPathW(
        static_cast<DWORD>(directory.size()), directory.data());
    if (length == 0U || length >= directory.size()
        || GetTempFileNameW(directory.data(), L"ikp", 0U, path.data()) == 0U) {
        return false;
    }
    output.assign(path.data());
    DeleteFileW(output.c_str());
    return true;
}

CommandContext Context(DocumentSessionId session, Generation generation) noexcept {
    CommandContext context{};
    context.document_session = session;
    context.generation = generation;
    return context;
}

InkpodPrimitiveRequestV3 PrimitiveRequest(
    std::uint32_t opcode,
    std::uint32_t schema_version,
    std::uint64_t base_revision) noexcept {
    InkpodPrimitiveRequestV3 request{};
    request.struct_size = sizeof(request);
    request.opcode = opcode;
    request.schema_version = schema_version;
    request.base_revision = base_revision;
    request.payload_id.struct_size = sizeof(request.payload_id);
    return request;
}

bool WaitForPendingOperations(
    CoreHost& host,
    DocumentSessionId session,
    Generation generation,
    std::uint64_t minimum) noexcept {
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
    do {
        CoreSessionState state{};
        if (host.GetSessionState(session, generation, state)
            && state.pending_operations >= minimum) {
            return true;
        }
        std::this_thread::yield();
    } while (std::chrono::steady_clock::now() < deadline);
    return false;
}

struct FileIoPhaseProbe final {
    std::atomic<std::uint32_t> phase{};
    std::atomic<DWORD> owner{};
    std::atomic<std::uint64_t> polls{};
    std::atomic<std::uint32_t> completions{};
    std::atomic<bool> read_started{};
    std::atomic<bool> install_started{};
    std::atomic<bool> cancellation_seen{};
    std::promise<void> reading;
    std::promise<void> installing;
    std::promise<void> cancelled;
    std::promise<InkpodStatus> completed;

    InkpodStatus Step(bool cancel, bool& fence) {
        owner.store(GetCurrentThreadId(), std::memory_order_release);
        polls.fetch_add(1U, std::memory_order_relaxed);
        if (cancel && !cancellation_seen.exchange(true)) {
            cancelled.set_value();
        }
        const auto current = phase.load(std::memory_order_acquire);
        if (current == 0U) {
            fence = false;
            if (!read_started.exchange(true)) {
                reading.set_value();
            }
            return cancel ? INKPOD_STATUS_CANCELLED : INKPOD_STATUS_PENDING;
        }
        if (current == 1U) {
            fence = true;
            if (!install_started.exchange(true)) {
                installing.set_value();
            }
            return INKPOD_STATUS_PENDING;
        }
        fence = false;
        return INKPOD_STATUS_OK;
    }
};

bool FileIoPollingAndInstallFence(HWND owner) {
    auto probe = std::make_shared<FileIoPhaseProbe>();
    auto reading = probe->reading.get_future();
    auto installing = probe->installing.get_future();
    auto completed = probe->completed.get_future();
    SnapshotSink sink;
    CoreHost host;
    constexpr DocumentSessionId first{41U};
    constexpr DocumentSessionId second{42U};
    constexpr Generation generation{1U};
    bool passed = host.Start(&sink, owner) == INKPOD_STATUS_OK
        && host.CreateSession(first, generation) == INKPOD_STATUS_OK
        && host.CreateSession(second, generation) == INKPOD_STATUS_OK;
    if (passed) {
        passed = host.EnqueueFileIo(Context(first, generation), true,
            [probe](InkpodCore*, bool cancel, bool& fence) { return probe->Step(cancel, fence); },
            false, false, [probe](InkpodStatus status) {
                probe->completions.fetch_add(1U);
                probe->completed.set_value(status);
            });
    }
    if (passed) {
        passed = reading.wait_for(std::chrono::seconds(5)) == std::future_status::ready
            && host.Invoke(first, generation, [](InkpodCore*) { return INKPOD_STATUS_OK; },
                false, false) == INKPOD_STATUS_OK;
    }
    if (passed) {
        probe->phase.store(1U, std::memory_order_release);
        passed = installing.wait_for(std::chrono::seconds(5)) == std::future_status::ready;
    }
    std::promise<InkpodStatus> delayed;
    auto delayed_result = delayed.get_future();
    if (passed) {
        passed = host.Enqueue(Context(first, generation),
            [](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false, false,
            [&delayed](InkpodStatus status) { delayed.set_value(status); })
            && host.Invoke(second, generation, [](InkpodCore*) { return INKPOD_STATUS_OK; },
                false, false) == INKPOD_STATUS_OK
            && delayed_result.wait_for(std::chrono::milliseconds(0)) == std::future_status::timeout;
    }
    probe->phase.store(2U, std::memory_order_release);
    if (passed) {
        passed = completed.wait_for(std::chrono::seconds(5)) == std::future_status::ready
            && completed.get() == INKPOD_STATUS_OK
            && delayed_result.wait_for(std::chrono::seconds(5)) == std::future_status::ready
            && delayed_result.get() == INKPOD_STATUS_OK
            && probe->completions.load() == 1U && probe->polls.load() >= 3U
            && probe->owner.load() == host.ThreadId();
    }
    host.Stop();
    return passed;
}

bool FileIoCloseCancellationAndShutdownFinalization(HWND owner) {
    auto read = std::make_shared<FileIoPhaseProbe>();
    auto reading = read->reading.get_future();
    auto completed_read = read->completed.get_future();
    auto install = std::make_shared<FileIoPhaseProbe>();
    install->phase.store(1U);
    auto installing = install->installing.get_future();
    auto cancelled_install = install->cancelled.get_future();
    auto completed_install = install->completed.get_future();
    SnapshotSink sink;
    CoreHost host;
    constexpr DocumentSessionId first{51U};
    constexpr DocumentSessionId second{52U};
    constexpr Generation generation{1U};
    bool passed = host.Start(&sink, owner) == INKPOD_STATUS_OK
        && host.CreateSession(first, generation) == INKPOD_STATUS_OK
        && host.CreateSession(second, generation) == INKPOD_STATUS_OK
        && host.EnqueueFileIo(Context(first, generation), true,
            [read](InkpodCore*, bool cancel, bool& fence) { return read->Step(cancel, fence); },
            false, false, [read](InkpodStatus status) {
                read->completions.fetch_add(1U);
                read->completed.set_value(status);
            });
    if (passed) {
        passed = reading.wait_for(std::chrono::seconds(5)) == std::future_status::ready
            && host.CloseSession(first, generation) == INKPOD_STATUS_OK
            && completed_read.wait_for(std::chrono::seconds(5)) == std::future_status::ready
            && completed_read.get() == INKPOD_STATUS_CANCELLED && read->completions.load() == 1U
            && host.CreateSession(first, Generation{2U}) == INKPOD_STATUS_OK
            && !host.EnqueueFileIo(Context(first, generation), true,
                [](InkpodCore*, bool, bool&) { return INKPOD_STATUS_OK; }, false, false, {});
    }
    if (passed) {
        passed = host.EnqueueFileIo(Context(second, generation), true,
            [install](InkpodCore*, bool cancel, bool& fence) { return install->Step(cancel, fence); },
            false, false, [install](InkpodStatus status) {
                install->completions.fetch_add(1U);
                install->completed.set_value(status);
            }) && installing.wait_for(std::chrono::seconds(5)) == std::future_status::ready;
    }
    if (!passed) {
        install->phase.store(2U);
        host.Stop();
        return false;
    }
    auto stopped = std::async(std::launch::async, [&host] { host.Stop(); });
    passed = cancelled_install.wait_for(std::chrono::seconds(5)) == std::future_status::ready
        && stopped.wait_for(std::chrono::milliseconds(0)) == std::future_status::timeout;
    install->phase.store(2U, std::memory_order_release);
    const bool finished = stopped.wait_for(std::chrono::seconds(5)) == std::future_status::ready;
    stopped.get();
    return passed && finished
        && completed_install.wait_for(std::chrono::seconds(0)) == std::future_status::ready
        && completed_install.get() == INKPOD_STATUS_OK && install->completions.load() == 1U;
}
bool PrimitiveQueueSaturationIsExactlyOnce(
    CoreHost& host,
    DocumentSessionId session,
    Generation generation,
    std::uint64_t base_revision) {
    auto request = PrimitiveRequest(
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR, 1U, base_revision);
    request.color = InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        91U,
        37U,
        143U,
        255U};
    if (host.InvokePrimitive(
            session, generation, request, false, false, true)
        != INKPOD_STATUS_OK) {
        return false;
    }

    InkpodDocumentInfo stable_info = EmptyDocumentInfo();
    if (host.Invoke(
            session,
            generation,
            [&stable_info](InkpodCore* core) {
                return inkpod_core_get_document_info(core, &stable_info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return false;
    }
    request.base_revision = stable_info.document_revision;

    std::promise<void> blocker_started;
    std::promise<void> release_blocker;
    const std::shared_future<void> release = release_blocker.get_future().share();
    if (!host.Enqueue(
            Context(session, generation),
            [&blocker_started, release](InkpodCore*) {
                blocker_started.set_value();
                release.wait();
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false)) {
        return false;
    }
    blocker_started.get_future().wait();

    CoreSessionState before{};
    EngineMetrics metrics_before{};
    if (!host.GetSessionState(session, generation, before)
        || !host.GetMetrics(session, generation, metrics_before)) {
        release_blocker.set_value();
        return false;
    }

    constexpr std::size_t queue_capacity = 4096U;
    for (std::size_t index = 0; index < queue_capacity; ++index) {
        if (!host.EnqueuePrimitive(
                Context(session, generation), request, false, false, true)) {
            release_blocker.set_value();
            return false;
        }
    }
    const bool saturated = !host.EnqueuePrimitive(
        Context(session, generation), request, false, false, true);
    release_blocker.set_value();

    for (int attempt = 0; attempt < 200; ++attempt) {
        CoreSessionState draining{};
        if (host.GetSessionState(session, generation, draining)
            && draining.pending_operations < queue_capacity) {
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    if (host.WaitIdle(session, generation) != INKPOD_STATUS_OK) {
        return false;
    }

    CoreSessionState after{};
    EngineMetrics metrics_after{};
    InkpodDocumentInfo after_info = EmptyDocumentInfo();
    const InkpodStatus info_status = host.Invoke(
        session,
        generation,
        [&after_info](InkpodCore* core) {
            return inkpod_core_get_document_info(core, &after_info);
        },
        false,
        false);
    return saturated && host.GetSessionState(session, generation, after)
        && host.GetMetrics(session, generation, metrics_after)
        && info_status == INKPOD_STATUS_OK
        && after.pending_operations == 0U
        && after.last_completed_sequence == after.last_accepted_sequence
        && after.last_accepted_sequence
            == before.last_accepted_sequence + queue_capacity + 2U
        && metrics_after.accepted_work_items
            == metrics_before.accepted_work_items + queue_capacity + 2U
        && metrics_after.rejected_work_items
            == metrics_before.rejected_work_items + 1U
        && after_info.document_revision == stable_info.document_revision;
}

bool EditorCachePublicationIsOrdered(
    CoreHost& host,
    DocumentSessionId session,
    Generation generation) {
    InkpodEditorStateInfo before{};
    before.struct_size = sizeof(before);
    if (!host.GetEditorState(session, generation, before)) {
        return false;
    }

    std::promise<void> blocker_started;
    std::promise<void> release_blocker;
    const std::shared_future<void> release = release_blocker.get_future().share();
    std::promise<InkpodStatus> blocker_completed;
    if (!host.Enqueue(
            Context(session, generation),
            [&blocker_started, release](InkpodCore*) {
                blocker_started.set_value();
                release.wait();
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false,
            [&blocker_completed](InkpodStatus status) {
                blocker_completed.set_value(status);
            })) {
        return false;
    }
    blocker_started.get_future().wait();

    std::atomic<InkpodStatus> refresh_status{INKPOD_STATUS_INVALID_STATE};
    std::thread refresh([&host, session, generation, &refresh_status] {
        (void)SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
        refresh_status.store(
            host.RefreshEditorState(session, generation),
            std::memory_order_release);
    });
    if (!WaitForPendingOperations(host, session, generation, 2U)) {
        release_blocker.set_value();
        refresh.join();
        (void)blocker_completed.get_future().get();
        return false;
    }

    InkpodEditorStateUpdate update{};
    update.struct_size = sizeof(update);
    update.kind = INKPOD_EDITOR_UPDATE_ACTIVE_TOOL;
    update.expected_editor_revision = before.editor_revision;
    update.tool = before.active_tool == INKPOD_EDITOR_TOOL_BRUSH
        ? INKPOD_EDITOR_TOOL_PENCIL
        : INKPOD_EDITOR_TOOL_BRUSH;
    std::atomic<InkpodStatus> update_status{INKPOD_STATUS_INVALID_STATE};
    std::thread updater([&host, session, generation, update, &update_status] {
        (void)SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        update_status.store(
            host.UpdateEditorState(session, generation, update),
            std::memory_order_release);
    });
    if (!WaitForPendingOperations(host, session, generation, 3U)) {
        release_blocker.set_value();
        refresh.join();
        updater.join();
        (void)blocker_completed.get_future().get();
        return false;
    }

    release_blocker.set_value();
    refresh.join();
    updater.join();
    const InkpodStatus blocker_status = blocker_completed.get_future().get();
    InkpodEditorStateInfo after{};
    after.struct_size = sizeof(after);
    return blocker_status == INKPOD_STATUS_OK
        && refresh_status.load(std::memory_order_acquire) == INKPOD_STATUS_OK
        && update_status.load(std::memory_order_acquire) == INKPOD_STATUS_OK
        && host.GetEditorState(session, generation, after)
        && after.editor_revision == before.editor_revision + 1U
        && after.active_tool == update.tool;
}

bool EditorUpdatePublishesDocumentInfo(
    CoreHost& host,
    DocumentSessionId session,
    Generation generation) noexcept {
    InkpodEditorStateInfo editor_before{};
    editor_before.struct_size = sizeof(editor_before);
    InkpodDocumentInfo document_before = EmptyDocumentInfo();
    if (!host.GetEditorState(session, generation, editor_before)
        || !host.GetDocumentInfo(session, generation, document_before)
        || editor_before.active_layer_id != document_before.layer_id) {
        return false;
    }
    const bool select_color =
        editor_before.active_plane_id != document_before.color_plane_id;
    InkpodEditorStateUpdate update{};
    update.struct_size = sizeof(update);
    update.kind = INKPOD_EDITOR_UPDATE_ACTIVE_TARGET;
    update.expected_editor_revision = editor_before.editor_revision;
    update.active_layer_id = editor_before.active_layer_id;
    update.active_plane_id = select_color
        ? document_before.color_plane_id
        : document_before.main_plane_id;
    if (host.UpdateEditorState(session, generation, update)
        != INKPOD_STATUS_OK) {
        return false;
    }
    InkpodEditorStateInfo editor_after{};
    editor_after.struct_size = sizeof(editor_after);
    InkpodDocumentInfo document_after = EmptyDocumentInfo();
    return host.GetEditorState(session, generation, editor_after)
        && host.GetDocumentInfo(session, generation, document_after)
        && editor_after.editor_revision == editor_before.editor_revision + 1U
        && editor_after.active_layer_id == update.active_layer_id
        && editor_after.active_plane_id == update.active_plane_id
        && document_after.document_revision == document_before.document_revision
        && document_after.active_plane
            == (select_color ? INKPOD_PLANE_COLOR : INKPOD_PLANE_MAIN_LINE)
        && (document_after.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U;
}

bool DrainNotifications(
    HWND owner,
    CoreHost& host,
    DocumentSessionId first,
    DocumentSessionId second) noexcept {
    bool saw_first{};
    bool saw_second{};
    MSG message{};
    while (PeekMessageW(
               &message,
               owner,
               inkpod::app::kCoreStateChanged,
               inkpod::app::kCoreAsyncFailed,
               PM_REMOVE)
        != FALSE) {
        CoreNotification notification{};
        if (!host.TakeNotification(
                static_cast<std::uint64_t>(message.wParam),
                Generation(static_cast<std::uint64_t>(message.lParam)),
                notification)
            || !notification.context.document_session.has_value()
            || !notification.context.generation.has_value()) {
            return false;
        }
        saw_first = saw_first
            || notification.context.document_session.value() == first;
        saw_second = saw_second
            || notification.context.document_session.value() == second;
    }
    return saw_first && saw_second;
}

bool PrimitiveShutdownCompletesExactlyOnce(HWND owner) {
    SnapshotSink sink;
    CoreHost host;
    constexpr DocumentSessionId session{91U};
    constexpr Generation generation{13U};
    sink.Bind(session, generation);
    if (host.Start(&sink, owner) != INKPOD_STATUS_OK
        || host.CreateSession(session, generation) != INKPOD_STATUS_OK
        || !host.SetActiveSession(session, generation)
        || !host.RegisterDocumentView(
            session,
            generation,
            inkpod::app::DocumentViewId{21U},
            0U)
        || host.Invoke(
               session,
               generation,
               [](InkpodCore* core) { return NewCell(core, 91U, 16U, 16U); },
               false,
               true) != INKPOD_STATUS_OK) {
        host.Stop();
        return false;
    }
    InkpodDocumentInfo info = EmptyDocumentInfo();
    if (!host.GetDocumentInfo(session, generation, info)) {
        host.Stop();
        return false;
    }
    auto request = PrimitiveRequest(
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR, 1U, info.document_revision);
    request.color = InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        22U,
        44U,
        66U,
        255U};

    std::promise<void> blocker_started;
    std::promise<void> release_blocker;
    const std::shared_future<void> release = release_blocker.get_future().share();
    if (!host.Enqueue(
            Context(session, generation),
            [&blocker_started, release](InkpodCore*) {
                blocker_started.set_value();
                release.wait();
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false)) {
        host.Stop();
        return false;
    }
    blocker_started.get_future().wait();
    if (!host.EnqueuePrimitive(
            Context(session, generation), request, true, true, true)) {
        release_blocker.set_value();
        host.Stop();
        return false;
    }
    auto stop = std::async(std::launch::async, [&host] { host.Stop(); });
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
    release_blocker.set_value();
    stop.get();
    return sink.submitted.load(std::memory_order_acquire) == 1U
        && sink.last_revision.load(std::memory_order_acquire)
            == request.base_revision + 1U;
}

}  // namespace

int wmain() {
    SnapshotSink sink;
    SnapshotSink second_sink;
    std::array<SnapshotSink, CoreHost::kMaximumSnapshotSinks - 2U> capacity_sinks{};
    SnapshotSink rejected_sink;
    HWND owner = CreateWindowExW(
        0,
        L"STATIC",
        L"inkpod-core-host-test",
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        nullptr,
        GetModuleHandleW(nullptr),
        nullptr);
    if (owner == nullptr) {
        return 1;
    }

    if (!FileIoPollingAndInstallFence(owner)
        || !FileIoCloseCancellationAndShutdownFinalization(owner)) {
        DestroyWindow(owner);
        return 40;
    }
    CoreHost host;
    if (host.Start(&sink, owner) != INKPOD_STATUS_OK || host.ThreadId() == 0U
        || host.SnapshotSinkCount() != 1U) {
        DestroyWindow(owner);
        return 2;
    }

    constexpr DocumentSessionId first{11U};
    constexpr DocumentSessionId second{12U};
    constexpr DocumentSessionId third{13U};
    constexpr Generation generation{7U};
    sink.Bind(first, generation);
    if (host.CreateSession({}, generation) != INKPOD_STATUS_INVALID_ARGUMENT
        || host.CreateSession(first, generation) != INKPOD_STATUS_OK
        || host.CreateSession(second, generation) != INKPOD_STATUS_OK
        || host.CreateSession(first, generation) != INKPOD_STATUS_INVALID_STATE
        || host.SessionCount() != 2U
        || !host.SetActiveSession(first, generation)) {
        host.Stop();
        DestroyWindow(owner);
        return 3;
    }
    InkpodReplayContract replay_contract{};
    if (host.GetReplayContract(first, generation, replay_contract) != INKPOD_STATUS_OK
        || replay_contract.replay_epoch != 25U
        || replay_contract.procedure_format_version != 29U
        || replay_contract.canonical_numeric_version != 1U
        || replay_contract.primitive_count == 0U) {
        host.Stop();
        DestroyWindow(owner);
        return 34;
    }

    std::atomic<DWORD> first_thread{};
    std::atomic<DWORD> second_thread{};
    if (host.Invoke(
            first,
            generation,
            [&first_thread](InkpodCore* core) {
                first_thread.store(GetCurrentThreadId(), std::memory_order_release);
                return NewCell(core, 1U, 32U, 24U);
            },
            false,
            true)
            != INKPOD_STATUS_OK
        || host.Invoke(
               second,
               generation,
               [&second_thread](InkpodCore* core) {
                   second_thread.store(GetCurrentThreadId(), std::memory_order_release);
                   return NewCell(core, 2U, 48U, 16U);
               },
               false,
               true)
            != INKPOD_STATUS_OK
        || first_thread.load(std::memory_order_acquire) != host.ThreadId()
        || second_thread.load(std::memory_order_acquire) != host.ThreadId()
        || first_thread.load(std::memory_order_acquire)
            != second_thread.load(std::memory_order_acquire)) {
        host.Stop();
        DestroyWindow(owner);
        return 4;
    }

    InkpodSubpalette* asynchronous_subpalette{};
    std::atomic<DWORD> subpalette_thread{};
    std::promise<InkpodStatus> subpalette_completion;
    auto subpalette_future = subpalette_completion.get_future();
    if (host.CreateSubpalette(&asynchronous_subpalette) != INKPOD_STATUS_OK
        || asynchronous_subpalette == nullptr
        || !host.EnqueueSubpalette(
            asynchronous_subpalette,
            [&subpalette_thread](InkpodSubpalette* subpalette) {
                subpalette_thread.store(
                    GetCurrentThreadId(), std::memory_order_release);
                InkpodSubpaletteInfo info{};
                info.struct_size = sizeof(info);
                return inkpod_subpalette_get_info(subpalette, &info);
            },
            [&subpalette_completion](InkpodStatus status) {
                subpalette_completion.set_value(status);
            })
        || subpalette_future.get() != INKPOD_STATUS_OK
        || subpalette_thread.load(std::memory_order_acquire) != host.ThreadId()
        || host.ReleaseSubpalette(&asynchronous_subpalette) != INKPOD_STATUS_OK
        || asynchronous_subpalette != nullptr) {
        host.Stop();
        DestroyWindow(owner);
        return 81;
    }

    constexpr inkpod::app::DocumentViewId first_frontend_view{21U};
    constexpr inkpod::app::DocumentViewId second_frontend_view{22U};
    std::uint64_t second_core_view{};
    const InkpodViewInput second_pan{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_PAN_BY,
        0U,
        5.0,
        0.0,
        0.0,
        0.0};
    second_sink.Bind(
        first, generation, second_frontend_view, CanvasId{32U});
    bool registered_capacity = true;
    for (SnapshotSink& capacity_sink : capacity_sinks) {
        registered_capacity = host.RegisterSnapshotSink(&capacity_sink)
            && registered_capacity;
    }
    if (host.Invoke(
            first,
            generation,
            [&second_core_view, &second_pan](InkpodCore* core) {
                const InkpodStatus created =
                    inkpod_core_view_create(core, &second_core_view);
                return created == INKPOD_STATUS_OK
                    ? inkpod_core_view_apply(core, second_core_view, &second_pan)
                    : created;
            },
            false,
            false)
            != INKPOD_STATUS_OK
        || !host.RegisterDocumentView(
            first, generation, first_frontend_view, 0U)
        || !host.RegisterDocumentView(
            first, generation, second_frontend_view, second_core_view)
        || !host.RegisterSnapshotSink(&second_sink)
        || host.RegisterSnapshotSink(&second_sink)
        || !registered_capacity
        || host.RegisterSnapshotSink(&rejected_sink)
        || host.SnapshotSinkCount() != CoreHost::kMaximumSnapshotSinks
        || host.RegisterDocumentView(
            first, generation, second_frontend_view, second_core_view)
        || host.Invoke(
               first,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               true,
               false)
            != INKPOD_STATUS_OK
        || sink.submitted.load(std::memory_order_acquire) == 0U
        || second_sink.submitted.load(std::memory_order_acquire) == 0U
        || sink.last_revision.load(std::memory_order_acquire)
            != second_sink.last_revision.load(std::memory_order_acquire)
        || sink.last_pan_x.load(std::memory_order_acquire)
            == second_sink.last_pan_x.load(std::memory_order_acquire)) {
        host.Stop();
        DestroyWindow(owner);
        return 22;
    }

    InkpodDocumentInfo first_info = EmptyDocumentInfo();
    InkpodDocumentInfo second_info = EmptyDocumentInfo();
    if (!host.GetDocumentInfo(first, generation, first_info)
        || !host.GetDocumentInfo(second, generation, second_info)
        || first_info.width != 32U || first_info.height != 24U
        || second_info.width != 48U || second_info.height != 16U
        || first_info.document_id != second_info.document_id
        || first_info.document_revision != second_info.document_revision
        || (first_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || (second_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || !DrainNotifications(owner, host, first, second)) {
        host.Stop();
        DestroyWindow(owner);
        return 5;
    }

    std::array<InkpodColorValue, 1> queued_colors{{
        {sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 12U, 34U, 56U, 255U}}};
    const InkpodColorArray queued_palette{
        sizeof(InkpodColorArray),
        0U,
        INKPOD_FEATURE_NONE,
        queued_colors.data(),
        queued_colors.size(),
        sizeof(InkpodColorValue)};
    InkpodObjectId palette_id{};
    palette_id.struct_size = sizeof(palette_id);
    if (host.RegisterColorArray(
            first, generation, queued_palette, palette_id)
        != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 31;
    }
    std::promise<void> typed_blocker_started;
    std::promise<void> release_typed_blocker;
    const std::shared_future<void> typed_release =
        release_typed_blocker.get_future().share();
    if (!host.Enqueue(
            Context(first, generation),
            [&typed_blocker_started, typed_release](InkpodCore*) {
                typed_blocker_started.set_value();
                typed_release.wait();
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false)) {
        host.Stop();
        DestroyWindow(owner);
        return 31;
    }
    typed_blocker_started.get_future().wait();
    auto palette_request = PrimitiveRequest(
        INKPOD_PRIMITIVE_REPLACE_PALETTE,
        1U,
        first_info.document_revision);
    palette_request.payload_id = palette_id;
    if (!host.EnqueuePrimitive(
            Context(first, generation),
            palette_request,
            true,
            true,
            true)) {
        release_typed_blocker.set_value();
        host.Stop();
        DestroyWindow(owner);
        return 31;
    }
    queued_colors[0].red = 240U;
    queued_colors[0].green = 241U;
    queued_colors[0].blue = 242U;
    release_typed_blocker.set_value();
    if (host.WaitIdle(first, generation) != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 31;
    }
    std::array<InkpodColorValue, 1> copied_colors{};
    copied_colors[0].struct_size = sizeof(InkpodColorValue);
    InkpodColorBuffer copied_palette{};
    copied_palette.struct_size = sizeof(copied_palette);
    copied_palette.colors = copied_colors.data();
    copied_palette.color_capacity = copied_colors.size();
    copied_palette.color_stride_bytes = sizeof(InkpodColorValue);
    CoreSessionState typed_state{};
    first_info = EmptyDocumentInfo();
    if (host.Invoke(
            first,
            generation,
            [&copied_palette](InkpodCore* core) {
                return inkpod_core_palette_get(core, &copied_palette);
            },
            false,
            false) != INKPOD_STATUS_OK
        || copied_palette.color_count != 1U
        || copied_colors[0].red != 12U
        || copied_colors[0].green != 34U
        || copied_colors[0].blue != 56U
        || !host.GetDocumentInfo(first, generation, first_info)
        || first_info.document_revision != palette_request.base_revision + 1U
        || !host.GetSessionState(first, generation, typed_state)
        || typed_state.last_completed_sequence
            != typed_state.last_accepted_sequence
        || typed_state.pending_operations != 0U
        || host.ReleaseObject(first, generation, palette_id)
            != INKPOD_STATUS_OK
        || host.ReleaseObject(first, generation, palette_id)
            != INKPOD_STATUS_INVALID_STATE
        || host.Invoke(
               second,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               false,
               true) != INKPOD_STATUS_OK
        || !DrainNotifications(owner, host, first, second)) {
        host.Stop();
        DestroyWindow(owner);
        return 31;
    }
    if (!PrimitiveQueueSaturationIsExactlyOnce(
            host, first, generation, first_info.document_revision)
        || host.Invoke(
               first,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               false,
               true) != INKPOD_STATUS_OK
        || host.Invoke(
               second,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               false,
               true) != INKPOD_STATUS_OK
        || !DrainNotifications(owner, host, first, second)) {
        host.Stop();
        DestroyWindow(owner);
        return 32;
    }

    InkpodEditorStateInfo second_editor_before{};
    second_editor_before.struct_size = sizeof(second_editor_before);
    InkpodEditorStateInfo second_editor_after{};
    second_editor_after.struct_size = sizeof(second_editor_after);
    MSG unexpected_editor_notification{};
    if (!host.GetEditorState(second, generation, second_editor_before)
        || !EditorCachePublicationIsOrdered(host, first, generation)
        || !EditorUpdatePublishesDocumentInfo(host, first, generation)
        || !host.GetEditorState(second, generation, second_editor_after)
        || second_editor_after.editor_revision
            != second_editor_before.editor_revision
        || second_editor_after.active_tool != second_editor_before.active_tool
        || PeekMessageW(
               &unexpected_editor_notification,
               owner,
               inkpod::app::kCoreStateChanged,
               inkpod::app::kCoreStateChanged,
               PM_REMOVE) != FALSE) {
        host.Stop();
        DestroyWindow(owner);
        return 28;
    }

    if (host.Invoke(
            first,
            generation,
            [](InkpodCore*) { return INKPOD_STATUS_OK; },
            false,
            true) != INKPOD_STATUS_OK
        || host.Invoke(
               second,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               false,
               true) != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 24;
    }
    HWND replacement_owner = CreateWindowExW(
        0,
        L"STATIC",
        L"inkpod-core-host-replacement-owner",
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        nullptr,
        GetModuleHandleW(nullptr),
        nullptr);
    if (replacement_owner == nullptr
        || host.RetargetNotificationOwner(nullptr, replacement_owner)
        || !host.RetargetNotificationOwner(owner, replacement_owner)) {
        if (replacement_owner != nullptr) {
            DestroyWindow(replacement_owner);
        }
        host.Stop();
        DestroyWindow(owner);
        return 24;
    }
    DestroyWindow(owner);
    owner = replacement_owner;
    if (!DrainNotifications(owner, host, first, second)
        || host.Invoke(
            first,
            generation,
            [](InkpodCore*) { return INKPOD_STATUS_OK; },
            false,
            true) != INKPOD_STATUS_OK
        || host.Invoke(
               second,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               false,
               true) != INKPOD_STATUS_OK
        || !DrainNotifications(owner, host, first, second)) {
        host.Stop();
        DestroyWindow(owner);
        return 24;
    }

    std::wstring second_save_path;
    std::vector<std::uint8_t> second_save_path_utf8;
    if (!TemporaryPath(second_save_path)
        || !ToUtf8(second_save_path, second_save_path_utf8)
        || host.Invoke(
               second,
               generation,
               [&second_save_path_utf8](InkpodCore* core) {
                   InkpodDocumentInfo info = EmptyDocumentInfo();
                   return inkpod_core_save(
                       core,
                       second_save_path_utf8.data(),
                       second_save_path_utf8.size(),
                       &info);
               },
               false,
               true)
            != INKPOD_STATUS_OK
        || !host.GetDocumentInfo(second, generation, second_info)
        || (second_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        DeleteFileW(second_save_path.c_str());
        host.Stop();
        DestroyWindow(owner);
        return 21;
    }
    DeleteFileW(second_save_path.c_str());

    const std::uint64_t before_inactive_publish = sink.submitted.load();
    if (host.Invoke(
            second,
            generation,
            [](InkpodCore*) { return INKPOD_STATUS_OK; },
            true,
            false)
            != INKPOD_STATUS_OK
        || sink.submitted.load() != before_inactive_publish
        || host.FlushPreview() != INKPOD_STATUS_OK
        || sink.submitted.load() <= before_inactive_publish) {
        host.Stop();
        DestroyWindow(owner);
        return 6;
    }

    if (host.Invoke(
            first,
            generation,
            [](InkpodCore* core) { return ApplyMark(core, 4.0F, 5.0F); },
            false,
            true)
            != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 7;
    }
    if (sink.last_revision.load(std::memory_order_acquire)
            != second_sink.last_revision.load(std::memory_order_acquire)) {
        host.Stop();
        DestroyWindow(owner);
        return 23;
    }
    std::array<inkpod::renderer::CanvasSnapshotSink*,
               CoreHost::kMaximumSnapshotSinks - 2U>
        capacity_sink_ptrs{};
    for (std::size_t index = 0U; index < capacity_sinks.size(); ++index) {
        capacity_sink_ptrs[index] = &capacity_sinks[index];
    }
    std::array<inkpod::renderer::CanvasSnapshotSink*, 2U> invalid_sinks{
        capacity_sink_ptrs.front(), &rejected_sink};
    if (host.UnregisterSnapshotSinks(
            invalid_sinks.data(), invalid_sinks.size())
        || host.SnapshotSinkCount() != CoreHost::kMaximumSnapshotSinks
        || !host.UnregisterSnapshotSinks(
            capacity_sink_ptrs.data(), capacity_sink_ptrs.size())
        || host.SnapshotSinkCount() != 2U) {
        host.Stop();
        DestroyWindow(owner);
        return 23;
    }
    const std::uint64_t second_sink_before_unmap =
        second_sink.submitted.load(std::memory_order_acquire);
    if (!host.UnregisterDocumentView(
            first, generation, second_frontend_view)
        || host.UnregisterDocumentView(
            first, generation, second_frontend_view)
        || host.Invoke(
               first,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               true,
               false)
            != INKPOD_STATUS_OK
        || second_sink.submitted.load(std::memory_order_acquire)
            != second_sink_before_unmap
        || !host.UnregisterSnapshotSink(&second_sink)
        || host.UnregisterSnapshotSink(&second_sink)
        || host.SnapshotSinkCount() != 1U
        || host.Invoke(
               first,
               generation,
               [second_core_view](InkpodCore* core) {
                   return inkpod_core_view_close(core, second_core_view);
               },
               false,
               false)
            != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 23;
    }
    first_info = EmptyDocumentInfo();
    second_info = EmptyDocumentInfo();
    if (!host.GetDocumentInfo(first, generation, first_info)
        || !host.GetDocumentInfo(second, generation, second_info)
        || (first_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || (second_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        host.Stop();
        DestroyWindow(owner);
        return 8;
    }
    const std::uint64_t marked_checksum = first_info.color_plane_checksum;
    const std::uint64_t second_revision = second_info.document_revision;

    InkpodPersistenceInfo persistence{};
    const InkpodStatus persistence_status =
        host.GetPersistenceInfo(second, generation, persistence);
    second_info = EmptyDocumentInfo();
    if (persistence_status != INKPOD_STATUS_OK
        || persistence.format_version != 29U
        || persistence.open_strategy != INKPOD_NATIVE_OPEN_NOT_OPENED
        || persistence.feature_flags != INKPOD_FEATURE_NONE
        || !host.GetDocumentInfo(second, generation, second_info)
        || second_info.document_revision != second_revision
        || (second_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        host.Stop();
        DestroyWindow(owner);
        return 9;
    }

    if (host.Invoke(
            first,
            generation,
            [](InkpodCore* core) {
                const InkpodCellCreateOptions invalid{
                    sizeof(InkpodCellCreateOptions),
                    0U,
                    INKPOD_FEATURE_NONE,
                    1U,
                    2U,
                    0U,
                    24U,
                    96000U,
                    96000U};
                InkpodDocumentInfo ignored = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &invalid, &ignored);
            },
            false,
            false)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || !host.GetDocumentInfo(second, generation, second_info)
        || second_info.document_revision != second_revision) {
        host.Stop();
        DestroyWindow(owner);
        return 10;
    }

    StrokeEvent begin{};
    begin.kind = StrokeEventKind::Begin;
    begin.context = Context(first, generation);
    begin.context.document_view = first_frontend_view;
    begin.core_view_id = 0U;
    begin.style.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
    begin.samples.push_back(InkpodStrokeSample{
        sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U});
    StrokeEvent cancel{};
    cancel.kind = StrokeEventKind::Cancel;
    cancel.context = Context(first, generation);
    cancel.context.document_view = first_frontend_view;
    cancel.core_view_id = 0U;
    if (!host.EnqueueStroke(std::move(begin))) {
        host.Stop();
        DestroyWindow(owner);
        return 11;
    }
    bool stroke_became_active{};
    for (int attempt = 0; attempt < 100; ++attempt) {
        CoreSessionState active_state{};
        if (host.GetSessionState(first, generation, active_state)
            && active_state.stroke_active) {
            stroke_became_active = true;
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    auto deferred_request = PrimitiveRequest(
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR,
        1U,
        first_info.document_revision);
    deferred_request.color = InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        91U,
        37U,
        143U,
        255U};
    if (!stroke_became_active
        || !host.EnqueuePrimitive(
            Context(first, generation),
            deferred_request,
            false,
            true,
            true)) {
        (void)host.EnqueueStroke(std::move(cancel));
        host.Stop();
        DestroyWindow(owner);
        return 11;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    CoreSessionState deferred_state{};
    InkpodDocumentInfo deferred_info = EmptyDocumentInfo();
    if (!host.GetSessionState(first, generation, deferred_state)
        || !host.GetDocumentInfo(first, generation, deferred_info)
        || !deferred_state.stroke_active
        || deferred_state.pending_operations == 0U
        || deferred_info.document_revision != first_info.document_revision
        || !host.EnqueueStroke(std::move(cancel))
        || host.WaitIdle(first, generation) != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 11;
    }
    first_info = EmptyDocumentInfo();
    CoreSessionState after_deferred_state{};
    if (!host.GetDocumentInfo(first, generation, first_info)
        || !host.GetSessionState(first, generation, after_deferred_state)
        || first_info.color_plane_checksum != marked_checksum
        || first_info.document_revision != deferred_request.base_revision
        || after_deferred_state.stroke_active
        || after_deferred_state.pending_operations != 0U
        || after_deferred_state.last_completed_sequence
            != after_deferred_state.last_accepted_sequence) {
        host.Stop();
        DestroyWindow(owner);
        return 12;
    }

    InkpodDispatchResult history{};
    history.struct_size = sizeof(history);
    if (host.Invoke(
            first,
            generation,
            [&history](InkpodCore* core) { return inkpod_core_undo(core, &history); },
            false,
            true)
            != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 13;
    }
    first_info = EmptyDocumentInfo();
    if (!host.GetDocumentInfo(first, generation, first_info)
        || first_info.color_plane_checksum == marked_checksum
        || host.Invoke(
               first,
               generation,
               [&history](InkpodCore* core) { return inkpod_core_redo(core, &history); },
               false,
               true)
            != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 14;
    }
    first_info = EmptyDocumentInfo();
    second_info = EmptyDocumentInfo();
    if (!host.GetDocumentInfo(first, generation, first_info)
        || !host.GetDocumentInfo(second, generation, second_info)
        || first_info.color_plane_checksum != marked_checksum
        || second_info.document_revision != second_revision) {
        host.Stop();
        DestroyWindow(owner);
        return 15;
    }

    std::wstring save_path;
    std::vector<std::uint8_t> save_path_utf8;
    if (!TemporaryPath(save_path) || !ToUtf8(save_path, save_path_utf8)
        || host.Invoke(
               first,
               generation,
               [&save_path_utf8](InkpodCore* core) {
                   InkpodDocumentInfo info = EmptyDocumentInfo();
                   return inkpod_core_save(
                       core,
                       save_path_utf8.data(),
                       save_path_utf8.size(),
                       &info);
               },
               false,
               true)
            != INKPOD_STATUS_OK
        || host.Invoke(
               first,
               generation,
               [](InkpodCore* core) { return ApplyMark(core, 14.0F, 8.0F); },
               false,
               true)
            != INKPOD_STATUS_OK
        || host.Invoke(
               first,
               generation,
               [&save_path_utf8](InkpodCore* core) {
                   InkpodDocumentInfo info = EmptyDocumentInfo();
                   return inkpod_core_open(
                       core,
                       save_path_utf8.data(),
                       save_path_utf8.size(),
                       &info);
               },
               false,
               true)
            != INKPOD_STATUS_OK) {
        DeleteFileW(save_path.c_str());
        host.Stop();
        DestroyWindow(owner);
        return 16;
    }
    first_info = EmptyDocumentInfo();
    InkpodCompactionPlan compaction{};
    std::wstring compact_path;
    std::vector<std::uint8_t> compact_path_utf8;
    if (host.GetCompactionPlan(first, generation, compaction) != INKPOD_STATUS_OK
        || compaction.history_procedure_count == 0U
        || !TemporaryPath(compact_path)
        || !ToUtf8(compact_path, compact_path_utf8)
        || host.WriteCompactedCopy(
               first,
               generation,
               std::string_view{
                   reinterpret_cast<const char*>(compact_path_utf8.data()),
                   compact_path_utf8.size()},
               compaction)
            != INKPOD_STATUS_OK) {
        DeleteFileW(compact_path.c_str());
        DeleteFileW(save_path.c_str());
        host.Stop();
        DestroyWindow(owner);
        return 34;
    }
    DeleteFileW(compact_path.c_str());
    const std::wstring invalid_save_path = save_path + L"\\child.inkpod";
    std::vector<std::uint8_t> invalid_save_path_utf8;
    if (!host.GetDocumentInfo(first, generation, first_info)
        || first_info.color_plane_checksum != marked_checksum
        || (first_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || host.Invoke(
               first,
               generation,
               [](InkpodCore* core) {
                   InkpodDocumentInfo info = EmptyDocumentInfo();
                   return inkpod_core_save(core, nullptr, 0U, &info);
               },
               false,
               false)
            != INKPOD_STATUS_INVALID_ARGUMENT
        || !ToUtf8(invalid_save_path, invalid_save_path_utf8)
        || host.Invoke(
               first,
               generation,
               [&invalid_save_path_utf8](InkpodCore* core) {
                   InkpodDocumentInfo info = EmptyDocumentInfo();
                   return inkpod_core_save(
                       core,
                       invalid_save_path_utf8.data(),
                       invalid_save_path_utf8.size(),
                       &info);
               },
               false,
               false)
            != INKPOD_STATUS_IO_ERROR
        || !host.GetDocumentInfo(second, generation, second_info)
        || second_info.document_revision != second_revision) {
        DeleteFileW(save_path.c_str());
        host.Stop();
        DestroyWindow(owner);
        return 17;
    }
    DeleteFileW(save_path.c_str());

    // One long operation retains the single-writer lane, but input already
    // accepted for another document remains queued and observable instead of
    // being dropped or retargeted.
    std::promise<void> latency_operation_started;
    std::promise<void> release_latency_operation;
    std::shared_future<void> latency_release_future =
        release_latency_operation.get_future().share();
    std::promise<InkpodStatus> latency_operation_completion;
    std::promise<InkpodStatus> delayed_input_completion;
    if (!host.Enqueue(
            Context(second, generation),
            [&latency_operation_started, latency_release_future](InkpodCore*) {
                latency_operation_started.set_value();
                latency_release_future.wait();
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false,
            [&latency_operation_completion](InkpodStatus status) {
                latency_operation_completion.set_value(status);
            })) {
        host.Stop();
        DestroyWindow(owner);
        return 25;
    }
    latency_operation_started.get_future().wait();
    InkpodHistoryInfo cached_history{};
    cached_history.struct_size = sizeof(cached_history);
    InkpodHistoryEntryKind cached_undo{};
    InkpodHistoryEntryKind cached_redo{};
    std::uint64_t cached_target_count{};
    InkpodEditTargetCapabilities cached_capabilities{};
    cached_capabilities.struct_size = sizeof(cached_capabilities);
    InkpodSnapshotTransform cached_transform{};
    cached_transform.struct_size = sizeof(cached_transform);
    const auto cache_query_started = std::chrono::steady_clock::now();
    const bool cached_history_available = host.GetHistoryPresentation(
        first, generation, cached_history, cached_undo, cached_redo);
    const bool cached_targets_available = host.GetEditTargetPresentation(
        first,
        generation,
        cached_target_count,
        cached_capabilities);
    const bool cached_transform_available = host.GetSnapshotTransform(
        first, generation, 0U, cached_transform);
    const bool stale_transform_rejected = !host.GetSnapshotTransform(
        first, generation, UINT64_MAX, cached_transform);
    const auto cache_query_elapsed = std::chrono::steady_clock::now()
        - cache_query_started;
    if (!cached_history_available || !cached_targets_available
        || !cached_transform_available || !stale_transform_rejected
        || cache_query_elapsed >= std::chrono::milliseconds(100)) {
        release_latency_operation.set_value();
        (void)latency_operation_completion.get_future().get();
        host.Stop();
        DestroyWindow(owner);
        return !cached_history_available ? 35
            : !cached_targets_available ? 36
            : !cached_transform_available ? 37
            : !stale_transform_rejected ? 38
            : 39;
    }
    if (!host.Enqueue(
            Context(first, generation),
            [](InkpodCore*) { return INKPOD_STATUS_OK; },
            false,
            false,
            false,
            [&delayed_input_completion](InkpodStatus status) {
                delayed_input_completion.set_value(status);
            })) {
        release_latency_operation.set_value();
        host.Stop();
        DestroyWindow(owner);
        return 26;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    release_latency_operation.set_value();
    EngineMetrics first_metrics{};
    if (latency_operation_completion.get_future().get() != INKPOD_STATUS_OK
        || delayed_input_completion.get_future().get() != INKPOD_STATUS_OK
        || !host.GetMetrics(first, generation, first_metrics)
        || first_metrics.accepted_work_items == 0U
        || first_metrics.queue_wait_samples == 0U
        || first_metrics.maximum_queue_wait_microseconds < 1000U
        || first_metrics.peak_pending_operations == 0U) {
        host.Stop();
        DestroyWindow(owner);
        return 27;
    }

    std::promise<void> operation_started;
    std::promise<void> release_operation;
    std::shared_future<void> release_future = release_operation.get_future().share();
    std::promise<InkpodStatus> completion;
    if (!host.Enqueue(
            Context(second, generation),
            [&operation_started, release_future](InkpodCore*) {
                operation_started.set_value();
                release_future.wait();
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false,
            [&completion](InkpodStatus status) { completion.set_value(status); })) {
        host.Stop();
        DestroyWindow(owner);
        return 18;
    }
    operation_started.get_future().wait();
    auto close_future = std::async(std::launch::async, [&host, second, generation] {
        return host.CloseSession(second, generation);
    });
    bool close_started{};
    for (int attempt = 0; attempt < 100; ++attempt) {
        CoreSessionState state{};
        if (host.GetSessionState(second, generation, state)
            && !state.accepting_work) {
            close_started = true;
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    const bool stale_rejected = !host.Enqueue(
        Context(second, generation),
        [](InkpodCore*) { return INKPOD_STATUS_OK; },
        false,
        false,
        false);
    StrokeEvent stale_input{};
    stale_input.kind = StrokeEventKind::Cancel;
    stale_input.context = Context(second, generation);
    const bool stale_input_rejected = !host.EnqueueStroke(std::move(stale_input));
    const bool stale_snapshot_rejected = !host.Enqueue(
        Context(second, generation),
        [](InkpodCore*) { return INKPOD_STATUS_OK; },
        true,
        false,
        false);
    auto stale_primitive_request = PrimitiveRequest(
        INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR,
        1U,
        second_info.document_revision);
    stale_primitive_request.color = InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        1U,
        2U,
        3U,
        255U};
    const bool stale_primitive_rejected = !host.EnqueuePrimitive(
        Context(second, generation),
        stale_primitive_request,
        false,
        true,
        true);
    release_operation.set_value();
    const InkpodStatus completion_status = completion.get_future().get();
    const InkpodStatus close_status = close_future.get();
    if (!close_started || !stale_rejected || !stale_input_rejected
        || !stale_snapshot_rejected || !stale_primitive_rejected
        || completion_status != INKPOD_STATUS_OK
        || close_status != INKPOD_STATUS_OK || host.SessionCount() != 1U
        || host.HasSession(second, generation)
        || host.Invoke(
               second,
               generation,
               [](InkpodCore*) { return INKPOD_STATUS_OK; },
               false,
               false)
            != INKPOD_STATUS_INVALID_STATE) {
        host.Stop();
        DestroyWindow(owner);
        return 19;
    }

    std::atomic<DWORD> initializer_thread{};
    host.SetSessionInitializer([&initializer_thread](InkpodCore*) {
        initializer_thread.store(GetCurrentThreadId(), std::memory_order_release);
        return INKPOD_STATUS_OK;
    });
    if (host.CreateSession(third, generation) != INKPOD_STATUS_OK
        || initializer_thread.load(std::memory_order_acquire) != host.ThreadId()
        || host.RebindSession(third, generation, third, Generation{8U})
            != INKPOD_STATUS_OK
        || host.HasSession(third, generation)
        || !host.HasSession(third, Generation{8U})
        || host.CloseSession(third, Generation{8U}) != INKPOD_STATUS_OK
        || host.CloseSession(first, generation) != INKPOD_STATUS_OK
        || host.SessionCount() != 0U) {
        host.Stop();
        DestroyWindow(owner);
        return 20;
    }

    InkpodEditorDefaults defaults{};
    const DWORD empty_thread = host.ThreadId();
    DWORD empty_owner_thread{};
    if (empty_thread == 0U || host.GetApplicationEditorDefaults(defaults) != INKPOD_STATUS_OK
        || host.InvokeOwnerThread([&empty_owner_thread] {
               empty_owner_thread = GetCurrentThreadId();
               return INKPOD_STATUS_OK;
           }) != INKPOD_STATUS_OK
        || empty_owner_thread != empty_thread
        || defaults.width == 0U || defaults.height == 0U
        || host.CreateSession(third, Generation{9U}) != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 34;
    }
    host.ClearActiveSession();
    if (host.Invoke([](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false)
            != INKPOD_STATUS_INVALID_STATE
        || !host.HasSession(third, Generation{9U})
        || !host.SetActiveSession(third, Generation{9U})
        || host.CloseSession(third, Generation{9U}) != INKPOD_STATUS_OK
        || host.ThreadId() != empty_thread) {
        host.Stop();
        DestroyWindow(owner);
        return 35;
    }
    host.Stop();
    const bool shutdown_completed_once =
        PrimitiveShutdownCompletesExactlyOnce(owner);
    DestroyWindow(owner);
    return shutdown_completed_once ? 0 : 33;
}
