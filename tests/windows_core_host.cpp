#include <windows.h>

#include <array>
#include <atomic>
#include <chrono>
#include <cstdint>
#include <future>
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
using inkpod::app::Generation;
using inkpod::app::StrokeEvent;
using inkpod::app::StrokeEventKind;

class SnapshotSink final : public inkpod::renderer::CanvasSnapshotSink {
public:
    void Bind(DocumentSessionId session, Generation generation) noexcept {
        route_ = inkpod::renderer::SnapshotRoute{
            session,
            inkpod::app::DocumentViewId(21U),
            CanvasId(31U),
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
        ++submitted;
        return inkpod_snapshot_release(&envelope.snapshot) == INKPOD_STATUS_OK;
    }

    std::atomic<std::uint64_t> submitted{};

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
        sizeof(InkpodStrokeSample)};
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

}  // namespace

int wmain() {
    SnapshotSink sink;
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

    CoreHost host;
    if (host.Start(&sink, owner) != INKPOD_STATUS_OK || host.ThreadId() == 0U) {
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

    const InkpodStatus no_op_status = host.Invoke(
        second,
        generation,
        [](InkpodCore* core) {
            const InkpodCommandBatch batch{
                sizeof(InkpodCommandBatch),
                0U,
                INKPOD_FEATURE_NONE,
                nullptr,
                0U,
                sizeof(InkpodCommand)};
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_dispatch_batch(core, &batch, &result);
        },
        false,
        true);
    second_info = EmptyDocumentInfo();
    if (no_op_status != INKPOD_STATUS_OK
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
    begin.style.tool = INKPOD_TOOL_PENCIL;
    begin.style.plane = INKPOD_PLANE_COLOR;
    begin.style.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
    begin.style.color_rgba = UINT32_C(0xff0000ff);
    begin.style.diameter = 2.0F;
    begin.samples.push_back(InkpodStrokeSample{
        sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U});
    StrokeEvent cancel{};
    cancel.kind = StrokeEventKind::Cancel;
    cancel.context = Context(first, generation);
    if (!host.EnqueueStroke(std::move(begin))
        || !host.EnqueueStroke(std::move(cancel))
        || host.WaitIdle(first, generation) != INKPOD_STATUS_OK) {
        host.Stop();
        DestroyWindow(owner);
        return 11;
    }
    first_info = EmptyDocumentInfo();
    if (!host.GetDocumentInfo(first, generation, first_info)
        || first_info.color_plane_checksum != marked_checksum) {
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
    release_operation.set_value();
    const InkpodStatus completion_status = completion.get_future().get();
    const InkpodStatus close_status = close_future.get();
    if (!close_started || !stale_rejected || !stale_input_rejected
        || !stale_snapshot_rejected
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

    host.Stop();
    DestroyWindow(owner);
    return 0;
}
