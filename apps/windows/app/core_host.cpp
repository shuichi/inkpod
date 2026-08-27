#include "core_host.h"

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <deque>
#include <future>
#include <iterator>
#include <limits>
#include <mutex>
#include <new>
#include <optional>
#include <thread>
#include <utility>
#include <variant>

#include "renderer/canvas.h"

namespace inkpod::app {
namespace {

constexpr std::size_t kMaximumSessions =
    CoreHost::kMaximumDocumentSessions;
constexpr std::size_t kMaximumFrontendViews = kMaximumSessions * 64U;
constexpr std::size_t kMaximumQueuedWork = 4096U;
constexpr std::size_t kReservedStrokeControlWork = 64U;
constexpr std::size_t kMaximumNotifications = 256U;
constexpr std::size_t kMaximumInkScriptJobs = 64U;
constexpr std::size_t kMaximumFileIoJobs = 128U;
constexpr auto kFileIoPollInterval = std::chrono::milliseconds(10);
constexpr std::size_t kMaximumStrokeSamples = 1048576U;
constexpr auto kPreviewFrameInterval = std::chrono::milliseconds(8);

struct SessionBinding {
    DocumentSessionId session{};
    Generation generation{};

    [[nodiscard]] explicit operator bool() const noexcept {
        return static_cast<bool>(session) && static_cast<bool>(generation);
    }

    constexpr auto operator<=>(const SessionBinding&) const noexcept = default;
};

std::wstring ReadCoreErrorOnCurrentThread() {
    std::uint64_t required{};
    if (inkpod_error_message_size(&required) != INKPOD_STATUS_OK || required == 0U
        || required > static_cast<std::uint64_t>(std::numeric_limits<int>::max())) {
        return L"Unknown Core error";
    }
    std::vector<std::uint8_t> utf8(static_cast<std::size_t>(required));
    std::uint64_t written{};
    if (inkpod_error_message_copy(utf8.data(), required, &written) != INKPOD_STATUS_OK
        || written > static_cast<std::uint64_t>(std::numeric_limits<int>::max())) {
        return L"Unknown Core error";
    }
    const int converted = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(utf8.data()),
        static_cast<int>(written),
        nullptr,
        0);
    if (converted <= 0) {
        return L"Invalid UTF-8 Core error";
    }
    std::wstring wide(static_cast<std::size_t>(converted), L'\0');
    if (MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            reinterpret_cast<const char*>(utf8.data()),
            static_cast<int>(written),
            wide.data(),
            converted)
        != converted) {
        return L"Invalid UTF-8 Core error";
    }
    return wide;
}

CommandContext SessionContext(SessionBinding binding) noexcept {
    CommandContext context{};
    context.document_session = binding.session;
    context.generation = binding.generation;
    return context;
}

using AdapterInputToken = std::uint64_t;

struct AdapterWork {
    SessionBinding binding;
    std::uint64_t sequence{};
    CommandContext context;
    AdapterInputToken input_token{};
    bool publish_snapshot{};
    bool refresh_document_info{};
    bool defer_during_active_stroke{};
    std::chrono::steady_clock::time_point queued_at{std::chrono::steady_clock::now()};
};

struct AdapterInput {
    AdapterInputToken token{};
    CoreHost::CoreOperation operation;
    std::optional<std::uint64_t> active_view_update;
    std::shared_ptr<std::promise<InkpodStatus>> completion;
    std::function<void(InkpodStatus)> async_completion;
};

struct OwnerThreadWork {
    std::function<InkpodStatus()> operation;
    std::shared_ptr<std::promise<InkpodStatus>> completion;
    std::function<void(InkpodStatus)> async_completion;
};

struct InkScriptWork {
    SessionBinding binding;
    std::uint64_t sequence{};
    std::uint64_t job_id{};
    CommandContext context;
    std::chrono::steady_clock::time_point not_before{
        std::chrono::steady_clock::now()};
    std::chrono::steady_clock::time_point queued_at{
        std::chrono::steady_clock::now()};
};

struct InkScriptInput {
    explicit InkScriptInput(InkScriptEngineRequest input)
        : job_id(input.job_id),
          context(input.context),
          request(std::move(input)) {}

    std::uint64_t job_id{};
    std::uint64_t sequence{};
    CommandContext context;
    InkScriptEngineRequest request;
    std::unique_ptr<InkScriptEngineTask> task;
    std::atomic<bool> cancel_requested{};
    std::uint32_t confirmation_scope{};
    bool waiting_confirmation{};
    bool work_scheduled{};
};

struct StrokeWork {
    SessionBinding binding;
    std::uint64_t sequence{};
    std::uint64_t pending_units{1U};
    StrokeEvent event;
    std::chrono::steady_clock::time_point queued_at{std::chrono::steady_clock::now()};
};

struct PrimitiveWork {
    SessionBinding binding;
    std::uint64_t sequence{};
    CommandContext context;
    InkpodPrimitiveRequestV3 request{};
    bool publish_snapshot{};
    bool refresh_document_info{};
    bool defer_during_active_stroke{};
    std::shared_ptr<std::promise<InkpodStatus>> completion;
    std::chrono::steady_clock::time_point queued_at{std::chrono::steady_clock::now()};
};

enum class ControlKind : std::uint8_t {
    Create,
    AdoptBatchResult,
    Rebind,
    Close,
};

struct FileIoWork {
    std::uint64_t token{};
};

struct FileIoInput {
    std::uint64_t token{};
    SessionBinding binding;
    CommandContext context;
    std::uint64_t sequence{};
    CoreHost::FileIoOperation operation;
    std::function<void(InkpodStatus)> completion;
    bool publish_snapshot{};
    bool refresh_document_info{};
    bool active{};
    bool installing{};
    std::chrono::steady_clock::time_point next_poll{};
};

struct ControlWork {
    ControlKind kind{ControlKind::Create};
    SessionBinding binding;
    SessionBinding replacement;
    InkpodBatchReport* batch_report{};
    std::uint64_t batch_result_index{};
    std::shared_ptr<std::promise<InkpodStatus>> completion;
};

constexpr bool IsCreationControl(ControlKind kind) noexcept {
    return kind == ControlKind::Create
        || kind == ControlKind::AdoptBatchResult;
}

using WorkItem = std::variant<
    AdapterWork,
    PrimitiveWork,
    StrokeWork,
    FileIoWork,
    InkScriptWork,
    ControlWork,
    OwnerThreadWork>;

struct PublishedSession {
    SessionBinding binding;
    InkpodDocumentInfo document_info{};
    bool has_document_info{};
    InkpodEditorStateInfo editor_state{};
    bool has_editor_state{};
    InkpodHistoryInfo history_info{};
    InkpodHistoryEntryKind undo_kind{};
    InkpodHistoryEntryKind redo_kind{};
    bool has_history_info{};
    InkpodEditTargetCapabilities edit_target_capabilities{};
    bool has_edit_target_presentation{};
    bool has_edit_target_capabilities{};
    std::uint64_t edit_target_count{};
    InkpodSnapshotTransform snapshot_transform{};
    std::uint64_t snapshot_view_id{};
    bool has_snapshot_transform{};
    std::wstring last_error;
    EngineMetrics metrics{};
    CoreSessionState state{};
};

struct CoreEntry {
    SessionBinding binding;
    InkpodCore* core{};
    std::uint64_t active_view_id{};
    std::uint64_t active_sample_count{};
    bool preview_dirty{};
    bool stroke_active{};
    std::uint64_t io_install_fence{};
    std::chrono::steady_clock::time_point next_preview_frame{};
};

struct FrontendViewBinding {
    SessionBinding session;
    DocumentViewId frontend_view{};
    std::uint64_t core_view_id{};
};

}  // namespace

struct CoreHost::Impl final {
    explicit Impl(
        renderer::CanvasSnapshotSink* canvas_window,
        HWND owner_window,
        InkpodIoManager* manager) noexcept
        : initial_canvas(canvas_window), owner(owner_window), io_manager(manager) {}

    InkpodStatus Start() noexcept {
        try {
            {
                std::lock_guard lock(state_mutex);
                published.reserve(kMaximumSessions);
                frontend_views.reserve(kMaximumFrontendViews);
                snapshot_sinks.reserve(CoreHost::kMaximumSnapshotSinks);
                snapshot_sinks.push_back(initial_canvas);
                notifications.clear();
            }
            entries.reserve(kMaximumSessions);
            adapter_inputs.reserve(kMaximumQueuedWork + kReservedStrokeControlWork);
            inkscript_inputs.reserve(kMaximumInkScriptJobs);
            file_io_inputs.reserve(kMaximumFileIoJobs);
            auto ready = std::make_shared<std::promise<InkpodStatus>>();
            auto future = ready->get_future();
            worker = std::thread([this, ready] { Run(ready); });
            return future.get();
        } catch (const std::system_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::future_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    void Stop() noexcept {
        {
            std::lock_guard state_lock(state_mutex);
            for (auto& session : published) {
                session.state.accepting_work = false;
            }
        }
        {
            std::lock_guard lock(mutex);
            stopping = true;
        }
        wake.notify_one();
        if (worker.joinable()) {
            worker.join();
        }
        {
            std::lock_guard lock(state_mutex);
            published.clear();
            frontend_views.clear();
            snapshot_sinks.clear();
            notifications.clear();
            active = {};
        }
    }

    InkpodStatus Control(
        ControlKind kind,
        SessionBinding binding,
        SessionBinding replacement = {},
        InkpodBatchReport* batch_report = nullptr,
        std::uint64_t batch_result_index = 0U) noexcept {
        try {
            auto completion = std::make_shared<std::promise<InkpodStatus>>();
            auto future = completion->get_future();
            if (!PushControl(ControlWork{
                    kind,
                    binding,
                    replacement,
                    batch_report,
                    batch_result_index,
                    completion})) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return future.get();
        } catch (const std::future_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    InkpodStatus CreateSession(SessionBinding binding) noexcept {
        if (!binding) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        {
            std::lock_guard lock(state_mutex);
            if (published.size() >= kMaximumSessions
                || FindPublishedLocked(binding.session) != published.end()) {
                return INKPOD_STATUS_INVALID_STATE;
            }
        }
        const InkpodStatus status = Control(ControlKind::Create, binding);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        try {
            std::lock_guard lock(state_mutex);
            PublishedSession session{};
            session.binding = binding;
            session.state.generation = binding.generation;
            session.state.accepting_work = true;
            published.push_back(std::move(session));
            if (!active) {
                active = binding;
            }
        } catch (const std::bad_alloc&) {
            (void)Control(ControlKind::Close, binding);
            return INKPOD_STATUS_INVALID_STATE;
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus AdoptBatchResult(
        SessionBinding binding,
        InkpodBatchReport* report,
        std::uint64_t result_index) noexcept {
        if (!binding || report == nullptr) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        {
            std::lock_guard lock(state_mutex);
            if (published.size() >= kMaximumSessions
                || FindPublishedLocked(binding.session) != published.end()) {
                return INKPOD_STATUS_INVALID_STATE;
            }
        }
        const InkpodStatus status = Control(
            ControlKind::AdoptBatchResult,
            binding,
            {},
            report,
            result_index);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        try {
            std::lock_guard lock(state_mutex);
            PublishedSession session{};
            session.binding = binding;
            session.state.generation = binding.generation;
            session.state.accepting_work = true;
            published.push_back(std::move(session));
            if (!active) {
                active = binding;
            }
        } catch (const std::bad_alloc&) {
            (void)Control(ControlKind::Close, binding);
            return INKPOD_STATUS_INVALID_STATE;
        }
        const InkpodStatus refresh_status = Invoke(
            binding,
            [](InkpodCore*) { return INKPOD_STATUS_OK; },
            false,
            true);
        if (refresh_status != INKPOD_STATUS_OK) {
            (void)CloseSession(binding);
            return refresh_status;
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus RebindSession(
        SessionBinding old_binding,
        SessionBinding new_binding) noexcept {
        if (!old_binding || !new_binding) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        if (old_binding == new_binding) {
            return HasSession(old_binding)
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        }
        {
            std::lock_guard lock(state_mutex);
            const auto old = FindPublishedLocked(old_binding);
            if (old == published.end() || !old->state.accepting_work
                || (old_binding.session != new_binding.session
                    && FindPublishedLocked(new_binding.session) != published.end())) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            old->state.accepting_work = false;
        }
        const InkpodStatus status = Control(
            ControlKind::Rebind, old_binding, new_binding);
        std::lock_guard lock(state_mutex);
        const auto old = FindPublishedLocked(old_binding);
        if (status != INKPOD_STATUS_OK) {
            if (old != published.end()) {
                old->state.accepting_work = true;
            }
            return status;
        }
        if (old == published.end()) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        old->binding = new_binding;
        old->document_info = {};
        old->has_document_info = false;
        old->editor_state = {};
        old->has_editor_state = false;
        std::erase_if(frontend_views, [old_binding](const FrontendViewBinding& view) {
            return view.session == old_binding;
        });
        old->state.generation = new_binding.generation;
        old->state.accepting_work = true;
        if (active == old_binding) {
            active = new_binding;
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus CloseSession(SessionBinding binding) noexcept {
        if (!binding) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        {
            std::lock_guard lock(state_mutex);
            const auto found = FindPublishedLocked(binding);
            if (found == published.end() || !found->state.accepting_work) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            found->state.accepting_work = false;
        }
        const InkpodStatus status = Control(ControlKind::Close, binding);
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (status != INKPOD_STATUS_OK) {
            if (found != published.end()) {
                found->state.accepting_work = true;
            }
            return status;
        }
        if (found != published.end()) {
            published.erase(found);
        }
        std::erase_if(frontend_views, [binding](const FrontendViewBinding& view) {
            return view.session == binding;
        });
        if (active == binding) {
            active = published.empty() ? SessionBinding{} : published.front().binding;
        }
        return INKPOD_STATUS_OK;
    }

    bool SetActiveSession(SessionBinding binding) noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end() || !found->state.accepting_work) {
            return false;
        }
        active = binding;
        return true;
    }

    bool RetargetNotificationOwner(
        HWND expected_owner,
        HWND replacement_owner) noexcept {
        if (expected_owner == nullptr || replacement_owner == nullptr
            || expected_owner == replacement_owner
            || IsWindow(replacement_owner) == FALSE) {
            return false;
        }
        std::scoped_lock lock(notification_owner_mutex, state_mutex);
        if (owner != expected_owner) {
            return true;
        }
        for (const CoreNotification& notification : notifications) {
            const UINT message = NotificationMessage(notification.kind);
            if (PostMessageW(
                    replacement_owner,
                    message,
                    static_cast<WPARAM>(notification.token),
                    static_cast<LPARAM>(
                        notification.context.generation->Value()))
                == FALSE) {
                return false;
            }
        }
        owner = replacement_owner;
        return true;
    }

    bool RegisterSnapshotSink(renderer::CanvasSnapshotSink* sink) noexcept {
        if (sink == nullptr) {
            return false;
        }
        std::lock_guard lock(state_mutex);
        if (std::find(snapshot_sinks.cbegin(), snapshot_sinks.cend(), sink)
                != snapshot_sinks.cend()
            || snapshot_sinks.size() >= CoreHost::kMaximumSnapshotSinks) {
            return false;
        }
        try {
            snapshot_sinks.push_back(sink);
            return true;
        } catch (const std::bad_alloc&) {
            return false;
        }
    }

    bool UnregisterSnapshotSinks(
        renderer::CanvasSnapshotSink* const* sinks,
        std::size_t count) noexcept {
        if (sinks == nullptr || count == 0U
            || count > CoreHost::kMaximumSnapshotSinks) {
            return false;
        }
        std::lock_guard lock(state_mutex);
        for (std::size_t index = 0U; index < count; ++index) {
            if (sinks[index] == nullptr
                || std::find(sinks, sinks + index, sinks[index]) != sinks + index
                || std::find(
                    snapshot_sinks.cbegin(),
                    snapshot_sinks.cend(),
                    sinks[index]) == snapshot_sinks.cend()) {
                return false;
            }
        }
        for (std::size_t index = 0U; index < count; ++index) {
            const auto found = std::find(
                snapshot_sinks.begin(), snapshot_sinks.end(), sinks[index]);
            snapshot_sinks.erase(found);
        }
        return true;
    }

    std::size_t SnapshotSinkCount() const noexcept {
        std::lock_guard lock(state_mutex);
        return snapshot_sinks.size();
    }

    bool RegisterDocumentView(
        SessionBinding binding,
        DocumentViewId frontend_view,
        std::uint64_t core_view_id) noexcept {
        if (!binding || !frontend_view) {
            return false;
        }
        std::lock_guard lock(state_mutex);
        const auto session = FindPublishedLocked(binding);
        if (session == published.end() || !session->state.accepting_work
            || frontend_views.size() >= kMaximumFrontendViews) {
            return false;
        }
        const auto duplicate = std::find_if(
            frontend_views.cbegin(),
            frontend_views.cend(),
            [binding, frontend_view, core_view_id](const FrontendViewBinding& view) {
                return view.frontend_view == frontend_view
                    || (view.session == binding && view.core_view_id == core_view_id);
            });
        if (duplicate != frontend_views.cend()) {
            return false;
        }
        try {
            frontend_views.push_back(
                FrontendViewBinding{binding, frontend_view, core_view_id});
            return true;
        } catch (const std::bad_alloc&) {
            return false;
        }
    }

    bool UnregisterDocumentView(
        SessionBinding binding,
        DocumentViewId frontend_view) noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = std::find_if(
            frontend_views.begin(),
            frontend_views.end(),
            [binding, frontend_view](const FrontendViewBinding& view) {
                return view.session == binding && view.frontend_view == frontend_view;
            });
        if (found == frontend_views.end()) {
            return false;
        }
        frontend_views.erase(found);
        return true;
    }

    bool HasSession(SessionBinding binding) const noexcept {
        std::lock_guard lock(state_mutex);
        return FindPublishedLocked(binding) != published.end();
    }

    std::size_t SessionCount() const noexcept {
        std::lock_guard lock(state_mutex);
        return published.size();
    }

    std::optional<SessionBinding> ActiveBinding() const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(active);
        return found == published.end() || !found->state.accepting_work
            ? std::nullopt
            : std::optional{active};
    }

    InkpodStatus Invoke(
        SessionBinding binding,
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept {
        if (!binding || !operation) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            auto completion = std::make_shared<std::promise<InkpodStatus>>();
            auto future = completion->get_future();
            AdapterWork item{
                binding,
                0U,
                SessionContext(binding),
                0U,
                publish_snapshot,
                refresh_document_info,
                false};
            AdapterInput input{
                0U, std::move(operation), std::nullopt, completion, {}};
            if (!PushAdapter(std::move(item), std::move(input))) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return future.get();
        } catch (const std::future_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    InkpodStatus InvokeAll(
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept {
        if (!operation) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        std::vector<SessionBinding> bindings;
        try {
            std::lock_guard lock(state_mutex);
            bindings.reserve(published.size());
            for (const auto& session : published) {
                if (session.state.accepting_work) {
                    bindings.push_back(session.binding);
                }
            }
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        for (const SessionBinding binding : bindings) {
            const InkpodStatus status = Invoke(
                binding, operation, publish_snapshot, refresh_document_info);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus InvokeOwnerThread(
        std::function<InkpodStatus()> operation) noexcept {
        if (!operation) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            auto completion = std::make_shared<std::promise<InkpodStatus>>();
            auto future = completion->get_future();
            {
                std::lock_guard lock(mutex);
                if (stopping || work.size() >= kMaximumQueuedWork) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                work.emplace_back(OwnerThreadWork{
                    std::move(operation), std::move(completion), {}});
            }
            wake.notify_one();
            return future.get();
        } catch (const std::future_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    bool EnqueueOwnerThread(
        std::function<InkpodStatus()> operation,
        std::function<void(InkpodStatus)> completion) noexcept {
        if (!operation || !completion) {
            return false;
        }
        try {
            {
                std::lock_guard lock(mutex);
                if (stopping || work.size() >= kMaximumQueuedWork) {
                    return false;
                }
                work.emplace_back(OwnerThreadWork{
                    std::move(operation), {}, std::move(completion)});
            }
            wake.notify_one();
            return true;
        } catch (const std::bad_alloc&) {
            return false;
        }
    }

    bool Enqueue(
        const CommandContext& context,
        CoreOperation operation,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke,
        std::function<void(InkpodStatus)> completion) noexcept {
        if (!context.document_session.has_value() || !context.generation.has_value()
            || !operation) {
            return false;
        }
        return PushAdapter(
            AdapterWork{
                SessionBinding{
                    context.document_session.value(), context.generation.value()},
                0U,
                context,
                0U,
                publish_snapshot,
                refresh_document_info,
                defer_during_active_stroke},
            AdapterInput{
                0U,
                std::move(operation),
                std::nullopt,
                nullptr,
                std::move(completion)});
    }

    bool EnqueueFileIo(
        const CommandContext& context,
        bool requires_core,
        FileIoOperation operation,
        bool publish_snapshot,
        bool refresh_document_info,
        std::function<void(InkpodStatus)> completion) noexcept {
        if (!operation || (requires_core
                && (!context.document_session.has_value() || !context.generation.has_value()))) {
            return false;
        }
        std::unique_ptr<FileIoInput> input;
        try {
            input = std::make_unique<FileIoInput>();
            input->context = context;
            if (requires_core) {
                input->binding = {
                    context.document_session.value(), context.generation.value()};
            }
            input->operation = std::move(operation);
            input->completion = std::move(completion);
            input->publish_snapshot = publish_snapshot;
            input->refresh_document_info = refresh_document_info;
        } catch (const std::bad_alloc&) {
            return false;
        }
        const SessionBinding binding = input->binding;
        std::lock_guard acceptance_lock(acceptance_mutex);
        if (binding) {
            std::lock_guard state_lock(state_mutex);
            const auto session = FindPublishedLocked(binding);
            if (session == published.end() || !session->state.accepting_work) {
                return false;
            }
            input->sequence = ++session->state.last_accepted_sequence;
            ++session->state.pending_operations;
        }
        const std::uint64_t sequence = input->sequence;
        bool queued{};
        std::uint64_t registered_token{};
        try {
            std::lock_guard lock(mutex);
            if (!stopping && file_io_inputs.size() < kMaximumFileIoJobs
                && work.size() < kMaximumQueuedWork && next_file_io_token != 0U) {
                registered_token = next_file_io_token;
                input->token = registered_token;
                file_io_inputs.push_back(std::move(input));
                try {
                    work.emplace_back(FileIoWork{registered_token});
                    ++next_file_io_token;
                    queued = true;
                } catch (...) {
                    file_io_inputs.pop_back();
                    throw;
                }
            }
        } catch (const std::bad_alloc&) {
            queued = false;
        }
        if (!queued) {
            if (binding) {
                RollbackPending(binding, sequence);
                RecordRejected(binding);
            }
            return false;
        }
        if (binding) {
            RecordAccepted(binding);
        }
        wake.notify_one();
        return true;
    }

    bool EnqueueInkScript(InkScriptEngineRequest request) noexcept {
        if (request.job_id == 0U || !request.context.document_session.has_value()
            || !request.context.generation.has_value()) {
            return false;
        }
        const SessionBinding binding{
            request.context.document_session.value(),
            request.context.generation.value()};
        std::unique_ptr<InkScriptInput> input;
        try {
            input = std::make_unique<InkScriptInput>(std::move(request));
        } catch (const std::bad_alloc&) {
            return false;
        }

        std::lock_guard acceptance_lock(acceptance_mutex);
        {
            std::lock_guard state_lock(state_mutex);
            const auto session = FindPublishedLocked(binding);
            if (session == published.end() || !session->state.accepting_work) {
                if (session != published.end()) {
                    ++session->metrics.rejected_work_items;
                }
                return false;
            }
            input->sequence = ++session->state.last_accepted_sequence;
            ++session->state.pending_operations;
        }
        const std::uint64_t sequence = input->sequence;
        const std::uint64_t job_id = input->job_id;
        const CommandContext context = input->context;
        bool queued{};
        try {
            std::lock_guard lock(mutex);
            const bool duplicate = std::any_of(
                inkscript_inputs.cbegin(),
                inkscript_inputs.cend(),
                [job_id](const auto& candidate) {
                    return candidate->job_id == job_id;
                });
            if (!stopping && !duplicate
                && inkscript_inputs.size() < kMaximumInkScriptJobs
                && work.size() < kMaximumQueuedWork) {
                input->work_scheduled = true;
                inkscript_inputs.push_back(std::move(input));
                try {
                    work.emplace_back(InkScriptWork{
                        binding,
                        sequence,
                        job_id,
                        context});
                    queued = true;
                } catch (...) {
                    inkscript_inputs.pop_back();
                    throw;
                }
            }
        } catch (const std::bad_alloc&) {
            queued = false;
        }
        if (!queued) {
            RollbackPending(binding, sequence);
            RecordRejected(binding);
            return false;
        }
        RecordAccepted(binding);
        wake.notify_one();
        return true;
    }

    bool ConfirmInkScript(
        std::uint64_t job_id,
        const CommandContext& context,
        std::uint32_t scope) noexcept {
        if (job_id == 0U || scope == 0U) {
            return false;
        }
        bool queued{};
        try {
            std::lock_guard lock(mutex);
            const auto found = FindInkScriptInput(job_id);
            if (found == inkscript_inputs.end() || (*found)->context != context
                || !(*found)->waiting_confirmation
                || (*found)->work_scheduled || stopping
                || work.size() >= kMaximumQueuedWork) {
                return false;
            }
            (*found)->confirmation_scope = scope;
            (*found)->waiting_confirmation = false;
            (*found)->work_scheduled = true;
            work.emplace_back(InkScriptWork{
                SessionBinding{
                    context.document_session.value(),
                    context.generation.value()},
                (*found)->sequence,
                job_id,
                context});
            queued = true;
        } catch (const std::bad_alloc&) {
            queued = false;
        }
        if (queued) {
            wake.notify_one();
        }
        return queued;
    }

    bool CancelInkScript(
        std::uint64_t job_id,
        const CommandContext& context) noexcept {
        bool accepted{};
        try {
            std::lock_guard lock(mutex);
            const auto found = FindInkScriptInput(job_id);
            if (found == inkscript_inputs.end() || (*found)->context != context) {
                return false;
            }
            (*found)->cancel_requested.store(true, std::memory_order_release);
            if (!(*found)->work_scheduled) {
                if (work.size() >= kMaximumQueuedWork + kReservedStrokeControlWork) {
                    accepted = true;
                } else {
                    (*found)->waiting_confirmation = false;
                    (*found)->work_scheduled = true;
                    work.emplace_back(InkScriptWork{
                        SessionBinding{
                            context.document_session.value(),
                            context.generation.value()},
                        (*found)->sequence,
                        job_id,
                        context});
                }
            }
            accepted = true;
        } catch (const std::bad_alloc&) {
            accepted = false;
        }
        if (accepted) {
            wake.notify_one();
        }
        return accepted;
    }

    InkpodStatus InvokePrimitive(
        SessionBinding binding,
        const InkpodPrimitiveRequestV3& request,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke) noexcept {
        try {
            auto completion = std::make_shared<std::promise<InkpodStatus>>();
            auto future = completion->get_future();
            PrimitiveWork item{
                binding,
                0U,
                SessionContext(binding),
                request,
                publish_snapshot,
                refresh_document_info,
                defer_during_active_stroke,
                completion};
            if (!PushPrimitive(std::move(item))) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return future.get();
        } catch (const std::future_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    bool EnqueuePrimitive(
        const CommandContext& context,
        const InkpodPrimitiveRequestV3& request,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke) noexcept {
        if (!context.document_session.has_value() || !context.generation.has_value()) {
            return false;
        }
        return PushPrimitive(PrimitiveWork{
            SessionBinding{
                context.document_session.value(), context.generation.value()},
            0U,
            context,
            request,
            publish_snapshot,
            refresh_document_info,
            defer_during_active_stroke,
            nullptr});
    }

    bool PushAdapter(AdapterWork item, AdapterInput input) noexcept {
        std::lock_guard acceptance_lock(acceptance_mutex);
        const SessionBinding binding = item.binding;
        {
            std::lock_guard state_lock(state_mutex);
            const auto session = FindPublishedLocked(binding);
            if (session == published.end() || !session->state.accepting_work) {
                if (session != published.end()) {
                    ++session->metrics.rejected_work_items;
                }
                return false;
            }
            item.sequence = ++session->state.last_accepted_sequence;
            ++session->state.pending_operations;
        }
        const std::uint64_t sequence = item.sequence;
        AdapterInputToken registered_token{};
        bool queued{};
        try {
            std::lock_guard lock(mutex);
            if (!stopping && work.size() < kMaximumQueuedWork
                && next_adapter_input_token != 0U
                && adapter_inputs.size() < kMaximumQueuedWork) {
                item.input_token = next_adapter_input_token;
                input.token = next_adapter_input_token;
                adapter_inputs.push_back(std::move(input));
                registered_token = next_adapter_input_token;
                work.emplace_back(std::move(item));
                ++next_adapter_input_token;
                queued = true;
            }
        } catch (const std::bad_alloc&) {
            if (!adapter_inputs.empty()
                && adapter_inputs.back().token == registered_token) {
                adapter_inputs.pop_back();
            }
            queued = false;
        }
        if (!queued) {
            RollbackPending(binding, sequence);
            RecordRejected(binding);
            return false;
        }
        RecordAccepted(binding);
        wake.notify_one();
        return true;
    }

    bool PushStroke(StrokeEvent event) noexcept {
        std::lock_guard acceptance_lock(acceptance_mutex);
        if (!event.context.document_session.has_value()
            || !event.context.document_view.has_value()
            || !event.context.generation.has_value()
            || event.samples.size() > kMaximumStrokeSamples) {
            return false;
        }
        StrokeWork item{
            SessionBinding{
                event.context.document_session.value(),
                event.context.generation.value()},
            0U,
            1U,
            std::move(event)};
        {
            std::lock_guard state_lock(state_mutex);
            const auto session = FindPublishedLocked(item.binding);
            if (session == published.end() || !session->state.accepting_work) {
                if (session != published.end()) {
                    ++session->metrics.rejected_work_items;
                }
                return false;
            }
            const auto mapped_view = std::find_if(
                frontend_views.cbegin(),
                frontend_views.cend(),
                [&item](const FrontendViewBinding& view) {
                    return view.session == item.binding
                        && view.frontend_view
                            == item.event.context.document_view.value();
                });
            if (mapped_view == frontend_views.cend()
                || mapped_view->core_view_id != item.event.core_view_id) {
                ++session->metrics.rejected_work_items;
                return false;
            }
            item.sequence = ++session->state.last_accepted_sequence;
            ++session->state.pending_operations;
        }

        const std::uint64_t sequence = item.sequence;
        bool queued{};
        try {
            std::lock_guard lock(mutex);
            if (!stopping && item.event.kind == StrokeEventKind::Append
                && !work.empty()) {
                if (auto* pending = std::get_if<StrokeWork>(&work.back());
                    pending != nullptr
                    && pending->event.kind == StrokeEventKind::Append
                    && pending->binding == item.binding
                    && pending->event.core_view_id == item.event.core_view_id
                    && pending->event.context == item.event.context
                    && pending->event.samples.size()
                            <= kMaximumStrokeSamples - std::min(
                                   kMaximumStrokeSamples,
                                   item.event.samples.size())) {
                    pending->event.samples.insert(
                        pending->event.samples.end(),
                        item.event.samples.begin(),
                        item.event.samples.end());
                    pending->sequence = item.sequence;
                    ++pending->pending_units;
                    queued = true;
                }
            }
            const std::size_t limit = item.event.kind == StrokeEventKind::Append
                ? kMaximumQueuedWork
                : kMaximumQueuedWork + kReservedStrokeControlWork;
            if (!queued && !stopping && work.size() < limit) {
                work.emplace_back(std::move(item));
                queued = true;
            }
        } catch (const std::bad_alloc&) {
            queued = false;
        }
        if (!queued) {
            RollbackPending(item.binding, sequence);
            RecordRejected(item.binding);
            return false;
        }
        RecordAccepted(item.binding);
        wake.notify_one();
        return true;
    }

    bool PushControl(ControlWork item) noexcept {
        try {
            std::lock_guard lock(mutex);
            if (stopping || work.size() >= kMaximumQueuedWork + kReservedStrokeControlWork) {
                return false;
            }
            work.emplace_back(std::move(item));
        } catch (const std::bad_alloc&) {
            return false;
        }
        wake.notify_one();
        return true;
    }

    void RollbackPending(SessionBinding binding, std::uint64_t sequence) noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found != published.end() && found->state.pending_operations != 0U) {
            --found->state.pending_operations;
            if (found->state.last_accepted_sequence == sequence) {
                --found->state.last_accepted_sequence;
            }
        }
    }

    void RecordAccepted(SessionBinding binding) noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end()) {
            return;
        }
        ++found->metrics.accepted_work_items;
        found->metrics.peak_pending_operations = std::max(
            found->metrics.peak_pending_operations,
            found->state.pending_operations);
    }

    void RecordRejected(SessionBinding binding) noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found != published.end()) {
            ++found->metrics.rejected_work_items;
        }
    }

    void RecordQueueWait(
        SessionBinding binding,
        std::chrono::steady_clock::time_point queued_at) noexcept {
        const auto elapsed = std::chrono::steady_clock::now() - queued_at;
        const auto raw_microseconds = std::chrono::duration_cast<
            std::chrono::microseconds>(elapsed).count();
        const std::uint64_t microseconds = raw_microseconds <= 0
            ? 0U
            : static_cast<std::uint64_t>(raw_microseconds);
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end()) {
            return;
        }
        ++found->metrics.queue_wait_samples;
        found->metrics.total_queue_wait_microseconds =
            found->metrics.total_queue_wait_microseconds > UINT64_MAX - microseconds
            ? UINT64_MAX
            : found->metrics.total_queue_wait_microseconds + microseconds;
        found->metrics.maximum_queue_wait_microseconds = std::max(
            found->metrics.maximum_queue_wait_microseconds, microseconds);
    }

    void CompletePending(
        SessionBinding binding,
        std::uint64_t sequence,
        std::uint64_t units,
        bool stroke_is_active) noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end()) {
            return;
        }
        found->state.pending_operations = found->state.pending_operations > units
            ? found->state.pending_operations - units
            : 0U;
        found->state.last_completed_sequence = std::max(
            found->state.last_completed_sequence, sequence);
        found->state.stroke_active = stroke_is_active;
    }

    void SetSessionInitializer(CoreOperation next) noexcept {
        std::lock_guard lock(initializer_mutex);
        initializer = std::move(next);
    }

    CoreOperation CopySessionInitializer() noexcept {
        try {
            std::lock_guard lock(initializer_mutex);
            return initializer;
        } catch (const std::bad_alloc&) {
            return {};
        }
    }

    void StoreHostFailure(std::wstring_view message) noexcept {
        try {
            std::lock_guard lock(state_mutex);
            host_error.assign(message.data(), message.size());
        } catch (const std::bad_alloc&) {
            std::lock_guard lock(state_mutex);
            host_error.clear();
        }
    }

    void CaptureFailure(
        CoreEntry& entry,
        InkpodStatus status,
        bool asynchronous,
        const CommandContext& context) noexcept {
        if (status == INKPOD_STATUS_OK
            || (asynchronous && status == INKPOD_STATUS_CANCELLED)) {
            return;
        }
        std::wstring message;
        try {
            message = ReadCoreErrorOnCurrentThread();
        } catch (const std::bad_alloc&) {
            message = L"Core error text allocation failed";
        }
        {
            std::lock_guard lock(state_mutex);
            const auto found = FindPublishedLocked(entry.binding);
            if (found != published.end()) {
                found->last_error = std::move(message);
            }
        }
        if (asynchronous) {
            PostNotification(CoreNotificationKind::AsyncFailed, context, status);
        }
    }

    void RefreshPublishedPresentation(
        SessionBinding binding, InkpodCore* core) noexcept {
        InkpodHistoryInfo history{};
        history.struct_size = sizeof(history);
        InkpodHistoryEntryKind undo_kind{};
        InkpodHistoryEntryKind redo_kind{};
        bool has_history_info{};
        {
            const auto read_history_kind = [core](
                std::uint64_t index, InkpodHistoryEntryKind& output) {
                InkpodHistoryItem item{};
                item.struct_size = sizeof(item);
                const InkpodStatus item_status = inkpod_core_history_item(
                    core, index, &item);
                if (item_status != INKPOD_STATUS_OK) {
                    return item_status;
                }
                output = item.entry_kind;
                return item.entry_kind >= INKPOD_HISTORY_ENTRY_RASTER
                        && item.entry_kind <= INKPOD_HISTORY_ENTRY_DOCUMENT
                    ? INKPOD_STATUS_OK
                    : INKPOD_STATUS_INVALID_STATE;
            };
            InkpodStatus history_status = inkpod_core_history_info(core, &history);
            if (history_status == INKPOD_STATUS_OK && history.cursor != 0U) {
                history_status = read_history_kind(history.cursor - 1U, undo_kind);
            }
            if (history_status == INKPOD_STATUS_OK
                && history.cursor < history.item_count) {
                history_status = read_history_kind(history.cursor, redo_kind);
            }
            has_history_info = history_status == INKPOD_STATUS_OK;
        }

        std::uint64_t edit_target_count{};
        const InkpodStatus edit_target_status = inkpod_core_get_edit_targets(
            core, nullptr, 0U, 0U, &edit_target_count);
        const bool has_edit_target_presentation =
            edit_target_count <= INKPOD_MAX_EDIT_TARGETS
            && (edit_target_status == INKPOD_STATUS_OK
                || edit_target_status == INKPOD_STATUS_BUFFER_TOO_SMALL);
        const bool has_edit_targets = has_edit_target_presentation
            && edit_target_count != 0U;
        InkpodEditTargetCapabilities capabilities{};
        capabilities.struct_size = sizeof(capabilities);
        const bool has_capabilities = has_edit_targets
            && inkpod_core_get_edit_target_capabilities(
                   core, &capabilities) == INKPOD_STATUS_OK;
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found != published.end()) {
            found->history_info = history;
            found->undo_kind = undo_kind;
            found->redo_kind = redo_kind;
            found->has_history_info = has_history_info;
            found->edit_target_capabilities = capabilities;
            found->has_edit_target_presentation = has_edit_target_presentation;
            found->has_edit_target_capabilities = has_capabilities;
            found->edit_target_count = has_edit_target_presentation
                ? edit_target_count : 0U;
        }
    }

    InkpodStatus RefreshDocumentInfo(CoreEntry& entry, const CommandContext& context) noexcept {
        InkpodDocumentInfo info{};
        info.struct_size = sizeof(info);
        InkpodStatus status = inkpod_core_get_document_info(entry.core, &info);
        InkpodEditorStateInfo editor{};
        editor.struct_size = sizeof(editor);
        if (status == INKPOD_STATUS_OK) {
            status = inkpod_core_get_editor_state(entry.core, &editor);
        }
        if (status == INKPOD_STATUS_OK) {
            {
                std::lock_guard lock(state_mutex);
                const auto found = FindPublishedLocked(entry.binding);
                if (found != published.end()) {
                    found->document_info = info;
                    found->has_document_info = true;
                    found->editor_state = editor;
                    found->has_editor_state = true;
                }
            }
            RefreshPublishedPresentation(entry.binding, entry.core);
            PostNotification(CoreNotificationKind::StateChanged, context, status);
        }
        return status;
    }

    bool IsActive(SessionBinding binding) const noexcept {
        std::lock_guard lock(state_mutex);
        return active == binding;
    }

    InkpodStatus PublishSnapshot(CoreEntry& entry, bool preview) noexcept {
        std::lock_guard lock(state_mutex);
        const InkpodSnapshotOptions options{
            sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
        InkpodStatus result = INKPOD_STATUS_OK;
        std::uint64_t published_count{};
        for (renderer::CanvasSnapshotSink* sink : snapshot_sinks) {
            if (sink == nullptr) {
                continue;
            }
            const renderer::SnapshotRoute route = sink->Route();
            if (!route || route.document_session != entry.binding.session
                || route.document_generation != entry.binding.generation
                || !sink->AcceptsSnapshots()) {
                continue;
            }
            const auto mapped = std::find_if(
                frontend_views.cbegin(),
                frontend_views.cend(),
                [&entry, &route](const FrontendViewBinding& view) {
                    return view.session == entry.binding
                        && view.frontend_view == route.document_view;
                });
            if (mapped == frontend_views.cend()) {
                continue;
            }
            const std::uint64_t core_view_id = mapped->core_view_id;
            InkpodSnapshot* snapshot{};
            const InkpodStatus status = core_view_id == 0U
                ? inkpod_core_build_snapshot(entry.core, &options, &snapshot)
                : inkpod_core_build_snapshot_for_view(
                      entry.core, core_view_id, &options, &snapshot);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            InkpodSnapshotView snapshot_view{};
            snapshot_view.struct_size = sizeof(snapshot_view);
            InkpodSnapshotTransform transform{};
            transform.struct_size = sizeof(transform);
            if (inkpod_snapshot_get_view(snapshot, &snapshot_view) != INKPOD_STATUS_OK
                || inkpod_snapshot_get_transform(snapshot, &transform)
                    != INKPOD_STATUS_OK) {
                inkpod_snapshot_release(&snapshot);
                return INKPOD_STATUS_INVALID_STATE;
            }
            if (core_view_id == entry.active_view_id) {
                const auto published_session = FindPublishedLocked(entry.binding);
                if (published_session != published.end()) {
                    published_session->snapshot_transform = transform;
                    published_session->snapshot_view_id = core_view_id;
                    published_session->has_snapshot_transform = true;
                }
            }
            renderer::SnapshotEnvelope envelope{
                route,
                snapshot_view.revision,
                transform.view_revision,
                snapshot};
            if (!sink->Submit(envelope)) {
                result = INKPOD_STATUS_INVALID_STATE;
            } else {
                ++published_count;
            }
        }
        if (preview && published_count != 0U) {
            const auto found = FindPublishedLocked(entry.binding);
            if (found != published.end()) {
                found->metrics.preview_snapshots += published_count;
            }
        }
        return result;
    }

    InkpodStatus AppendSamples(
        CoreEntry& entry,
        const std::vector<InkpodStrokeSample>& samples) noexcept {
        if (samples.empty()) {
            return INKPOD_STATUS_OK;
        }
        const InkpodStrokeSampleSpan span{
            sizeof(InkpodStrokeSampleSpan),
            0U,
            INKPOD_FEATURE_NONE,
            samples.data(),
            static_cast<std::uint64_t>(samples.size()),
            sizeof(InkpodStrokeSample)};
        return inkpod_core_stroke_append(entry.core, &span);
    }

    void ProcessStroke(StrokeWork item) noexcept {
        RecordQueueWait(item.binding, item.queued_at);
        CoreEntry* entry = FindEntry(item.binding);
        if (entry == nullptr || item.event.context.document_session != item.binding.session
            || item.event.context.generation != item.binding.generation) {
            CompletePending(item.binding, item.sequence, item.pending_units, false);
            return;
        }
        InkpodStatus status = INKPOD_STATUS_OK;
        switch (item.event.kind) {
            case StrokeEventKind::Begin: {
                if (item.event.samples.empty() || entry->stroke_active) {
                    status = INKPOD_STATUS_INVALID_ARGUMENT;
                    break;
                }
                const InkpodEditorStrokeInput input{
                    sizeof(InkpodEditorStrokeInput),
                    item.event.style.coordinate_space,
                    0U,
                    0U,
                    item.event.style.flags,
                    item.event.samples.data(),
                    static_cast<std::uint64_t>(item.event.samples.size()),
                    sizeof(InkpodStrokeSample)};
                status = inkpod_core_editor_stroke_begin_for_view(
                    entry->core, item.event.core_view_id, &input);
                if (status == INKPOD_STATUS_OK) {
                    entry->stroke_active = true;
                    entry->active_sample_count = item.event.samples.size();
                    status = PublishSnapshot(*entry, true);
                    entry->preview_dirty = false;
                    entry->next_preview_frame = std::chrono::steady_clock::now()
                        + kPreviewFrameInterval;
                }
                break;
            }
            case StrokeEventKind::Append:
                status = entry->stroke_active
                    ? AppendSamples(*entry, item.event.samples)
                    : INKPOD_STATUS_INVALID_STATE;
                if (status == INKPOD_STATUS_OK) {
                    entry->active_sample_count += item.event.samples.size();
                    entry->preview_dirty = true;
                }
                break;
            case StrokeEventKind::End: {
                status = entry->stroke_active
                    ? AppendSamples(*entry, item.event.samples)
                    : INKPOD_STATUS_INVALID_STATE;
                if (status == INKPOD_STATUS_OK) {
                    entry->active_sample_count += item.event.samples.size();
                    InkpodDispatchResult result{};
                    result.struct_size = sizeof(result);
                    status = inkpod_core_stroke_end(entry->core, &result);
                    entry->stroke_active = false;
                }
                if (status == INKPOD_STATUS_OK) {
                    {
                        std::lock_guard lock(state_mutex);
                        const auto found = FindPublishedLocked(entry->binding);
                        if (found != published.end()) {
                            ++found->metrics.completed_strokes;
                            found->metrics.completed_samples += entry->active_sample_count;
                        }
                    }
                    entry->active_sample_count = 0U;
                    entry->preview_dirty = false;
                    const InkpodStatus info_status = RefreshDocumentInfo(
                        *entry, item.event.context);
                    status = info_status == INKPOD_STATUS_OK
                        ? PublishSnapshot(*entry, false)
                        : info_status;
                }
                break;
            }
            case StrokeEventKind::Cancel:
                status = inkpod_core_stroke_cancel(entry->core);
                entry->stroke_active = false;
                entry->active_sample_count = 0U;
                entry->preview_dirty = false;
                if (status == INKPOD_STATUS_OK) {
                    status = PublishSnapshot(*entry, false);
                }
                break;
        }
        if (status != INKPOD_STATUS_OK) {
            (void)inkpod_core_stroke_cancel(entry->core);
            entry->stroke_active = false;
            entry->active_sample_count = 0U;
            entry->preview_dirty = false;
            (void)PublishSnapshot(*entry, false);
            CaptureFailure(*entry, status, true, item.event.context);
        }
        CompletePending(
            item.binding, item.sequence, item.pending_units, entry->stroke_active);
    }

    std::optional<AdapterInput> TakeAdapterInput(AdapterInputToken token) noexcept {
        std::lock_guard lock(mutex);
        const auto found = std::find_if(
            adapter_inputs.begin(),
            adapter_inputs.end(),
            [token](const AdapterInput& input) { return input.token == token; });
        if (found == adapter_inputs.end()) {
            return std::nullopt;
        }
        AdapterInput input = std::move(*found);
        if (found != std::prev(adapter_inputs.end())) {
            *found = std::move(adapter_inputs.back());
        }
        adapter_inputs.pop_back();
        return input;
    }

    void ProcessAdapter(AdapterWork item) noexcept {
        RecordQueueWait(item.binding, item.queued_at);
        std::optional<AdapterInput> input = TakeAdapterInput(item.input_token);
        CoreEntry* entry = FindEntry(item.binding);
        InkpodStatus status = entry == nullptr
            ? INKPOD_STATUS_CANCELLED
            : input.has_value() ? INKPOD_STATUS_OK : INKPOD_STATUS_INVALID_STATE;
        if (entry != nullptr && input.has_value()) {
            if (input->active_view_update.has_value()) {
                entry->active_view_id = input->active_view_update.value();
            }
            try {
                status = input->operation(entry->core);
            } catch (...) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status == INKPOD_STATUS_OK && item.refresh_document_info) {
                status = RefreshDocumentInfo(*entry, item.context);
            }
            if (status == INKPOD_STATUS_OK && item.publish_snapshot) {
                status = PublishSnapshot(*entry, false);
            }
        }
        const bool asynchronous = !input.has_value() || input->completion == nullptr;
        if (entry != nullptr) {
            CaptureFailure(*entry, status, asynchronous, item.context);
        }
        CompletePending(item.binding, item.sequence, 1U, entry != nullptr && entry->stroke_active);
        if (input.has_value() && input->completion != nullptr) {
            try {
                input->completion->set_value(status);
            } catch (const std::future_error&) {
            }
        }
        if (input.has_value() && input->async_completion) {
            try {
                input->async_completion(status);
            } catch (...) {
                if (entry != nullptr) {
                    CaptureFailure(
                        *entry,
                        INKPOD_STATUS_INVALID_STATE,
                        true,
                        item.context);
                }
            }
        }
    }

    static void ProcessOwnerThread(OwnerThreadWork item) noexcept {
        InkpodStatus status = INKPOD_STATUS_INVALID_STATE;
        try {
            status = item.operation();
        } catch (...) {
            status = INKPOD_STATUS_INVALID_STATE;
        }
        if (item.completion != nullptr) {
            try {
                item.completion->set_value(status);
            } catch (const std::future_error&) {
            }
        }
        if (item.async_completion) {
            try {
                item.async_completion(status);
            } catch (...) {
            }
        }
    }

    void ProcessPrimitive(PrimitiveWork item) noexcept {
        RecordQueueWait(item.binding, item.queued_at);
        CoreEntry* entry = FindEntry(item.binding);
        InkpodStatus status = entry == nullptr
            ? INKPOD_STATUS_CANCELLED
            : INKPOD_STATUS_OK;
        if (entry != nullptr) {
            InkpodPrimitiveResultV3 result{};
            result.struct_size = sizeof(result);
            status = inkpod_core_primitive_execute_v3(
                entry->core, &item.request, &result);
            if (status == INKPOD_STATUS_OK && item.refresh_document_info) {
                status = RefreshDocumentInfo(*entry, item.context);
            }
            if (status == INKPOD_STATUS_OK && item.publish_snapshot) {
                status = PublishSnapshot(*entry, false);
            }
        }
        const bool asynchronous = item.completion == nullptr;
        if (entry != nullptr) {
            CaptureFailure(*entry, status, asynchronous, item.context);
        }
        CompletePending(
            item.binding,
            item.sequence,
            1U,
            entry != nullptr && entry->stroke_active);
        if (item.completion != nullptr) {
            try {
                item.completion->set_value(status);
            } catch (const std::future_error&) {
            }
        }
    }

    void ProcessInkScript(InkScriptWork item) noexcept {
        RecordQueueWait(item.binding, item.queued_at);
        InkScriptInput* input{};
        {
            std::lock_guard lock(mutex);
            const auto found = FindInkScriptInput(item.job_id);
            if (found != inkscript_inputs.end()) {
                input = found->get();
                input->work_scheduled = false;
            }
        }
        CoreEntry* entry = FindEntry(item.binding);
        if (input == nullptr) {
            CompletePending(item.binding, item.sequence, 1U, false);
            return;
        }

        InkScriptEngineStep step{};
        if (entry == nullptr || input->context != item.context) {
            InkScriptEngineResult result{};
            result.job_id = item.job_id;
            result.status = INKPOD_STATUS_CANCELLED;
            step.kind = InkScriptEngineStepKind::Completed;
            step.has_notification = true;
            step.notification = result;
        } else {
            try {
                if (input->task == nullptr) {
                    input->task = std::make_unique<InkScriptEngineTask>(
                        std::move(input->request));
                }
                bool transitioning{};
                {
                    std::lock_guard lock(mutex);
                    transitioning = stopping
                        || HasTransitionControlLocked(item.binding);
                }
                const bool cancel = transitioning
                    || input->cancel_requested.load(std::memory_order_acquire);
                const std::uint32_t scope = std::exchange(
                    input->confirmation_scope, 0U);
                step = input->task->Advance(entry->core, cancel, scope);
            } catch (const std::bad_alloc&) {
                InkScriptEngineResult result{};
                result.job_id = item.job_id;
                result.status = INKPOD_STATUS_INVALID_STATE;
                step.kind = InkScriptEngineStepKind::Completed;
                step.has_notification = true;
                step.notification = result;
            }
        }

        if (step.has_notification) {
            PostInkScriptNotification(item.context, step.notification);
        }
        if (step.kind == InkScriptEngineStepKind::PlanReady) {
            std::lock_guard lock(mutex);
            const auto found = FindInkScriptInput(item.job_id);
            if (found != inkscript_inputs.end()) {
                (*found)->waiting_confirmation = true;
            }
            return;
        }
        if (step.kind == InkScriptEngineStepKind::Continue) {
            item.not_before = std::chrono::steady_clock::now()
                + std::chrono::milliseconds(step.delay_milliseconds);
            item.queued_at = std::chrono::steady_clock::now();
            bool requeued{};
            try {
                std::lock_guard lock(mutex);
                const auto found = FindInkScriptInput(item.job_id);
                if (found != inkscript_inputs.end() && !stopping
                    && work.size() < kMaximumQueuedWork) {
                    (*found)->work_scheduled = true;
                    work.emplace_back(std::move(item));
                    requeued = true;
                }
            } catch (const std::bad_alloc&) {
                requeued = false;
            }
            if (requeued) {
                wake.notify_one();
                return;
            }
            if (entry != nullptr && input->task != nullptr) {
                for (std::uint32_t attempt = 0U; attempt < 8U; ++attempt) {
                    step = input->task->Advance(entry->core, true, 0U);
                    if (step.kind == InkScriptEngineStepKind::Completed) {
                        break;
                    }
                }
            }
            if (step.kind != InkScriptEngineStepKind::Completed) {
                step = {};
                step.kind = InkScriptEngineStepKind::Completed;
                step.has_notification = true;
                step.notification.job_id = input->job_id;
                step.notification.status = INKPOD_STATUS_INVALID_STATE;
            }
            if (step.has_notification) {
                PostInkScriptNotification(input->context, step.notification);
            }
        }

        const InkpodStatus status = step.notification.status;
        if (entry != nullptr) {
            CaptureFailure(*entry, status, true, item.context);
        }
        {
            std::lock_guard lock(mutex);
            const auto found = FindInkScriptInput(input->job_id);
            if (found != inkscript_inputs.end()) {
                inkscript_inputs.erase(found);
            }
        }
        CompletePending(item.binding, item.sequence, 1U, false);
    }

    void ProcessControl(ControlWork item) noexcept {
        InkpodStatus status = INKPOD_STATUS_OK;
        switch (item.kind) {
            case ControlKind::Create: {
                if (FindEntry(item.binding) != nullptr || entries.size() >= kMaximumSessions) {
                    status = INKPOD_STATUS_INVALID_STATE;
                    break;
                }
                InkpodCore* core{};
                const InkpodCoreConfig config{
                    sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
                status = inkpod_core_create(&config, &core);
                if (status == INKPOD_STATUS_OK && io_manager != nullptr) {
                    status = inkpod_core_bind_io_manager(core, io_manager);
                }
                if (status != INKPOD_STATUS_OK) {
                    try {
                        StoreHostFailure(ReadCoreErrorOnCurrentThread());
                    } catch (const std::bad_alloc&) {
                        StoreHostFailure(L"Core creation error text allocation failed");
                    }
                }
                if (status == INKPOD_STATUS_OK) {
                    const CoreOperation initialize = CopySessionInitializer();
                    if (initialize) {
                        try {
                            status = initialize(core);
                            if (status != INKPOD_STATUS_OK) {
                                StoreHostFailure(ReadCoreErrorOnCurrentThread());
                            }
                        } catch (const std::bad_alloc&) {
                            status = INKPOD_STATUS_INVALID_STATE;
                            StoreHostFailure(L"Core initializer error text allocation failed");
                        } catch (...) {
                            status = INKPOD_STATUS_INVALID_STATE;
                            StoreHostFailure(L"Core initializer threw an exception");
                        }
                    }
                }
                if (status == INKPOD_STATUS_OK) {
                    try {
                        auto entry = std::make_unique<CoreEntry>();
                        entry->binding = item.binding;
                        entry->core = core;
                        entries.push_back(std::move(entry));
                    } catch (const std::bad_alloc&) {
                        status = INKPOD_STATUS_INVALID_STATE;
                        StoreHostFailure(L"Core entry allocation failed");
                    }
                }
                if (status != INKPOD_STATUS_OK) {
                    if (core != nullptr) {
                        (void)inkpod_core_destroy(&core);
                    }
                }
                break;
            }
            case ControlKind::AdoptBatchResult: {
                if (FindEntry(item.binding) != nullptr
                    || entries.size() >= kMaximumSessions
                    || item.batch_report == nullptr) {
                    status = INKPOD_STATUS_INVALID_STATE;
                    break;
                }
                std::unique_ptr<CoreEntry> entry;
                try {
                    entry = std::make_unique<CoreEntry>();
                } catch (const std::bad_alloc&) {
                    status = INKPOD_STATUS_INVALID_STATE;
                    break;
                }
                entry->binding = item.binding;
                std::uint64_t staged_generation{};
                status = inkpod_batch_report_take_staged_result(
                    item.batch_report,
                    item.batch_result_index,
                    &staged_generation,
                    &entry->core);
                if (status == INKPOD_STATUS_OK && io_manager != nullptr) {
                    status = inkpod_core_bind_io_manager(entry->core, io_manager);
                }
                if (status == INKPOD_STATUS_OK) {
                    entries.push_back(std::move(entry));
                } else if (entry->core != nullptr) {
                    (void)inkpod_core_destroy(&entry->core);
                }
                break;
            }
            case ControlKind::Rebind: {
                CoreEntry* entry = FindEntry(item.binding);
                if (entry == nullptr
                    || (item.binding != item.replacement
                        && FindEntry(item.replacement) != nullptr)) {
                    status = INKPOD_STATUS_INVALID_STATE;
                    break;
                }
                entry->binding = item.replacement;
                break;
            }
            case ControlKind::Close: {
                const auto found = std::find_if(
                    entries.begin(),
                    entries.end(),
                    [&item](const auto& entry) { return entry->binding == item.binding; });
                if (found == entries.end()) {
                    status = INKPOD_STATUS_INVALID_STATE;
                    break;
                }
                (void)inkpod_core_stroke_cancel((*found)->core);
                status = inkpod_core_destroy(&(*found)->core);
                if (status == INKPOD_STATUS_OK) {
                    entries.erase(found);
                } else {
                    CaptureFailure(
                        **found,
                        status,
                        false,
                        SessionContext(item.binding));
                }
                break;
            }
        }
        if (item.completion != nullptr) {
            try {
                item.completion->set_value(status);
            } catch (const std::future_error&) {
            }
        }
    }

    auto FindInkScriptInput(std::uint64_t job_id) noexcept {
        return std::find_if(
            inkscript_inputs.begin(),
            inkscript_inputs.end(),
            [job_id](const auto& input) { return input->job_id == job_id; });
    }

    auto FindInkScriptInput(std::uint64_t job_id) const noexcept {
        return std::find_if(
            inkscript_inputs.cbegin(),
            inkscript_inputs.cend(),
            [job_id](const auto& input) { return input->job_id == job_id; });
    }

    bool HasInkScriptInputLocked(SessionBinding binding) const noexcept {
        return std::any_of(
            inkscript_inputs.cbegin(),
            inkscript_inputs.cend(),
            [binding](const auto& input) {
                return input->context.document_session == binding.session
                    && input->context.generation == binding.generation;
            });
    }

    bool HasTransitionControlLocked(SessionBinding binding) const noexcept {
        return std::any_of(
            work.cbegin(), work.cend(), [binding](const WorkItem& candidate) {
                const auto* control = std::get_if<ControlWork>(&candidate);
                return control != nullptr && !IsCreationControl(control->kind)
                    && control->binding == binding;
            });
    }

    std::optional<InkScriptWork> ActionableCancellationWorkLocked() const noexcept {
        for (const auto& input : inkscript_inputs) {
            if (input->work_scheduled) {
                continue;
            }
            const SessionBinding binding{
                input->context.document_session.value(),
                input->context.generation.value()};
            if (stopping
                || input->cancel_requested.load(std::memory_order_acquire)
                || HasTransitionControlLocked(binding)) {
                return InkScriptWork{
                    binding,
                    input->sequence,
                    input->job_id,
                    input->context};
            }
        }
        return std::nullopt;
    }

    auto FindFileIoInput(std::uint64_t token) noexcept {
        return std::find_if(file_io_inputs.begin(), file_io_inputs.end(),
            [token](const auto& input) { return input->token == token; });
    }

    auto FindFileIoInput(std::uint64_t token) const noexcept {
        return std::find_if(file_io_inputs.cbegin(), file_io_inputs.cend(),
            [token](const auto& input) { return input->token == token; });
    }

    bool HasFileIoInputLocked(SessionBinding binding) const noexcept {
        return std::any_of(file_io_inputs.cbegin(), file_io_inputs.cend(),
            [binding](const auto& input) { return input->binding == binding; });
    }

    void ProcessFileIo(std::uint64_t token) noexcept {
        FileIoInput* input{};
        bool cancelled{};
        {
            std::lock_guard lock(mutex);
            const auto found = FindFileIoInput(token);
            if (found == file_io_inputs.end()) {
                return;
            }
            input = found->get();
            input->active = true;
            input->next_poll = std::chrono::steady_clock::now() + kFileIoPollInterval;
            cancelled = stopping || (input->binding && HasTransitionControlLocked(input->binding));
        }
        CoreEntry* entry = input->binding ? FindEntry(input->binding) : nullptr;
        if (cancelled && entry != nullptr && entry->stroke_active) {
            CancelActiveStroke(input->binding);
        }
        if (entry != nullptr && (entry->stroke_active
                || (entry->io_install_fence != 0U && entry->io_install_fence != token))) {
            return;
        }
        cancelled = cancelled || (input->binding && entry == nullptr);
        InkpodStatus status = INKPOD_STATUS_INVALID_STATE;
        try {
            status = input->operation(
                entry == nullptr ? nullptr : entry->core, cancelled, input->installing);
        } catch (...) {
            status = INKPOD_STATUS_INVALID_STATE;
        }
        if (entry != nullptr && input->installing) {
            entry->io_install_fence = token;
        }
        if (status == INKPOD_STATUS_PENDING) {
            return;
        }
        if (entry != nullptr && entry->io_install_fence == token) {
            entry->io_install_fence = 0U;
        }
        // The Rust apply result is authoritative. Presentation failure after a
        // durable save or staged open must not undo the UI path transition or
        // cause the successfully adopted document to be discarded.
        const InkpodStatus operation_status = status;
        if (entry != nullptr && status == INKPOD_STATUS_OK && input->refresh_document_info) {
            status = RefreshDocumentInfo(*entry, input->context);
        }
        if (entry != nullptr && status == INKPOD_STATUS_OK && input->publish_snapshot) {
            status = PublishSnapshot(*entry, false);
        }
        if (entry != nullptr) {
            CaptureFailure(*entry, status, false, input->context);
        }
        if (input->binding) {
            CompletePending(input->binding, input->sequence, 1U,
                entry != nullptr && entry->stroke_active);
        }
        if (input->completion) {
            try {
                input->completion(operation_status);
            } catch (...) {
            }
        }
        {
            std::lock_guard lock(mutex);
            const auto found = FindFileIoInput(token);
            if (found != file_io_inputs.end()) {
                file_io_inputs.erase(found);
            }
        }
        wake.notify_one();
    }

    std::chrono::steady_clock::time_point NextFileIoDeadline() const noexcept {
        auto result = std::chrono::steady_clock::time_point::max();
        for (const auto& input : file_io_inputs) {
            if (input->active) {
                result = std::min(result, input->next_poll);
            }
        }
        return result;
    }

    void AdvanceDueFileIo() noexcept {
        std::array<std::uint64_t, kMaximumFileIoJobs> due{};
        std::size_t count{};
        {
            std::lock_guard lock(mutex);
            const auto now = std::chrono::steady_clock::now();
            for (const auto& input : file_io_inputs) {
                if (input->active && now >= input->next_poll) {
                    due[count++] = input->token;
                }
            }
        }
        for (std::size_t index = 0U; index < count; ++index) {
            ProcessFileIo(due[index]);
        }
    }

    bool CanProcess(const WorkItem& item) const noexcept {
        if (const auto* sync = std::get_if<AdapterWork>(&item)) {
            const CoreEntry* entry = FindEntry(sync->binding);
            return entry == nullptr || (entry->io_install_fence == 0U
                && (!entry->stroke_active || !sync->defer_during_active_stroke));
        }
        if (const auto* primitive = std::get_if<PrimitiveWork>(&item)) {
            const CoreEntry* entry = FindEntry(primitive->binding);
            return entry == nullptr || (entry->io_install_fence == 0U
                && (!entry->stroke_active || !primitive->defer_during_active_stroke));
        }
        if (const auto* stroke = std::get_if<StrokeWork>(&item)) {
            const CoreEntry* entry = FindEntry(stroke->binding);
            return entry == nullptr || entry->io_install_fence == 0U;
        }
        if (const auto* io = std::get_if<FileIoWork>(&item)) {
            const auto input = FindFileIoInput(io->token);
            if (input == file_io_inputs.cend()) {
                return true;
            }
            const CoreEntry* entry = (*input)->binding ? FindEntry((*input)->binding) : nullptr;
            return entry == nullptr || (entry->io_install_fence == 0U
                && (stopping || HasTransitionControlLocked((*input)->binding)
                    || !entry->stroke_active));
        }
        if (const auto* script = std::get_if<InkScriptWork>(&item)) {
            const CoreEntry* entry = FindEntry(script->binding);
            if (entry != nullptr && (entry->stroke_active || entry->io_install_fence != 0U)) {
                return false;
            }
            const auto input = FindInkScriptInput(script->job_id);
            const bool cancelled = input != inkscript_inputs.cend()
                && (*input)->cancel_requested.load(std::memory_order_acquire);
            return entry == nullptr || stopping || cancelled
                || HasTransitionControlLocked(script->binding)
                || std::chrono::steady_clock::now() >= script->not_before;
        }
        if (const auto* control = std::get_if<ControlWork>(&item)) {
            if (IsCreationControl(control->kind)) {
                return true;
            }
            const CoreEntry* entry = FindEntry(control->binding);
            return (entry == nullptr || (!entry->stroke_active && entry->io_install_fence == 0U))
                && !HasInkScriptInputLocked(control->binding)
                && !HasFileIoInputLocked(control->binding);
        }
        if (std::holds_alternative<OwnerThreadWork>(item)) {
            return std::none_of(entries.cbegin(), entries.cend(),
                [](const auto& entry) { return entry->io_install_fence != 0U; });
        }
        return true;
    }

    std::chrono::steady_clock::time_point NextPreviewDeadline() const noexcept {
        auto result = std::chrono::steady_clock::time_point::max();
        for (const auto& entry : entries) {
            if (entry->preview_dirty) {
                result = std::min(result, entry->next_preview_frame);
            }
        }
        return result;
    }

    std::chrono::steady_clock::time_point NextInkScriptDeadline() const noexcept {
        auto result = std::chrono::steady_clock::time_point::max();
        for (const WorkItem& item : work) {
            if (const auto* script = std::get_if<InkScriptWork>(&item)) {
                result = std::min(result, script->not_before);
            }
        }
        return result;
    }

    void PublishDuePreviews() noexcept {
        const auto now = std::chrono::steady_clock::now();
        for (auto& entry : entries) {
            if (!entry->preview_dirty || now < entry->next_preview_frame) {
                continue;
            }
            const InkpodStatus status = PublishSnapshot(*entry, true);
            if (status != INKPOD_STATUS_OK) {
                (void)inkpod_core_stroke_cancel(entry->core);
                entry->stroke_active = false;
                entry->preview_dirty = false;
                entry->active_sample_count = 0U;
                CaptureFailure(
                    *entry,
                    status,
                    true,
                    SessionContext(entry->binding));
            } else {
                entry->preview_dirty = false;
                entry->next_preview_frame = now + kPreviewFrameInterval;
            }
            CompletePending(entry->binding, 0U, 0U, entry->stroke_active);
        }
    }

    void CancelAllActiveStrokes() noexcept {
        for (auto& entry : entries) {
            if (entry->stroke_active) {
                (void)inkpod_core_stroke_cancel(entry->core);
                entry->stroke_active = false;
                entry->preview_dirty = false;
                entry->active_sample_count = 0U;
                CompletePending(entry->binding, 0U, 0U, false);
            }
        }
    }

    std::optional<SessionBinding> TransitioningActiveStroke() const noexcept {
        for (const auto& item : work) {
            const auto* control = std::get_if<ControlWork>(&item);
            if (control == nullptr || IsCreationControl(control->kind)) {
                continue;
            }
            const CoreEntry* entry = FindEntry(control->binding);
            if (entry != nullptr && entry->stroke_active) {
                return control->binding;
            }
        }
        return std::nullopt;
    }

    void CancelActiveStroke(SessionBinding binding) noexcept {
        CoreEntry* entry = FindEntry(binding);
        if (entry == nullptr || !entry->stroke_active) {
            return;
        }
        (void)inkpod_core_stroke_cancel(entry->core);
        entry->stroke_active = false;
        entry->preview_dirty = false;
        entry->active_sample_count = 0U;
        CompletePending(binding, 0U, 0U, false);
    }

    void Run(const std::shared_ptr<std::promise<InkpodStatus>>& ready) noexcept {
        thread_id.store(GetCurrentThreadId(), std::memory_order_release);
        ready->set_value(INKPOD_STATUS_OK);

        for (;;) {
            WorkItem item;
            bool has_item{};
            bool cancel_for_shutdown{};
            std::optional<SessionBinding> cancel_for_close;
            std::optional<InkScriptWork> cancel_inkscript;
            {
                std::unique_lock lock(mutex);
                const auto deadline = std::min({
                    NextPreviewDeadline(), NextInkScriptDeadline(), NextFileIoDeadline()});
                wake.wait_until(lock, deadline, [this] {
                    return (stopping && file_io_inputs.empty())
                        || TransitioningActiveStroke().has_value()
                        || ActionableCancellationWorkLocked().has_value()
                        || std::chrono::steady_clock::now() >= NextFileIoDeadline()
                        || std::any_of(work.cbegin(), work.cend(), [this](const WorkItem& candidate) {
                               return CanProcess(candidate);
                           });
                });
                const auto next = std::find_if(
                    work.begin(), work.end(), [this](const WorkItem& candidate) {
                        return CanProcess(candidate);
                    });
                if (next != work.end()) {
                    item = std::move(*next);
                    work.erase(next);
                    has_item = true;
                } else if (const auto closing = TransitioningActiveStroke(); closing.has_value()) {
                    cancel_for_close = closing;
                } else if (const auto pending = ActionableCancellationWorkLocked();
                           pending.has_value()) {
                    cancel_inkscript = pending;
                } else if (stopping) {
                    cancel_for_shutdown = std::any_of(
                        entries.cbegin(), entries.cend(), [](const auto& entry) {
                            return entry->stroke_active;
                        });
                    if (!cancel_for_shutdown && work.empty() && file_io_inputs.empty()) {
                        break;
                    }
                }
            }

            if (cancel_for_shutdown) {
                CancelAllActiveStrokes();
                continue;
            }
            if (cancel_for_close.has_value()) {
                CancelActiveStroke(cancel_for_close.value());
                continue;
            }
            if (cancel_inkscript.has_value()) {
                ProcessInkScript(std::move(cancel_inkscript.value()));
                continue;
            }
            if (has_item) {
                if (auto* sync = std::get_if<AdapterWork>(&item)) {
                    ProcessAdapter(std::move(*sync));
                } else if (auto* primitive = std::get_if<PrimitiveWork>(&item)) {
                    ProcessPrimitive(std::move(*primitive));
                } else if (auto* stroke = std::get_if<StrokeWork>(&item)) {
                    ProcessStroke(std::move(*stroke));
                } else if (auto* io = std::get_if<FileIoWork>(&item)) {
                    ProcessFileIo(io->token);
                } else if (auto* script = std::get_if<InkScriptWork>(&item)) {
                    ProcessInkScript(std::move(*script));
                } else if (auto* owner_work =
                               std::get_if<OwnerThreadWork>(&item)) {
                    ProcessOwnerThread(std::move(*owner_work));
                } else {
                    ProcessControl(std::move(std::get<ControlWork>(item)));
                }
            }
            AdvanceDueFileIo();
            PublishDuePreviews();
        }

        CancelAllActiveStrokes();
        for (auto& entry : entries) {
            (void)inkpod_core_destroy(&entry->core);
        }
        entries.clear();
    }

    CoreEntry* FindEntry(SessionBinding binding) noexcept {
        const auto found = std::find_if(
            entries.begin(), entries.end(), [binding](const auto& entry) {
                return entry->binding == binding;
            });
        return found == entries.end() ? nullptr : found->get();
    }

    const CoreEntry* FindEntry(SessionBinding binding) const noexcept {
        return const_cast<Impl*>(this)->FindEntry(binding);
    }

    auto FindPublishedLocked(SessionBinding binding) noexcept {
        return std::find_if(
            published.begin(), published.end(), [binding](const PublishedSession& session) {
                return session.binding == binding;
            });
    }

    auto FindPublishedLocked(SessionBinding binding) const noexcept {
        return std::find_if(
            published.cbegin(), published.cend(), [binding](const PublishedSession& session) {
                return session.binding == binding;
            });
    }

    auto FindPublishedLocked(DocumentSessionId session) noexcept {
        return std::find_if(
            published.begin(), published.end(), [session](const PublishedSession& candidate) {
                return candidate.binding.session == session;
            });
    }

    auto FindPublishedLocked(DocumentSessionId session) const noexcept {
        return std::find_if(
            published.cbegin(), published.cend(), [session](const PublishedSession& candidate) {
                return candidate.binding.session == session;
            });
    }

    bool CopyDocumentInfo(SessionBinding binding, InkpodDocumentInfo& output) const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end() || !found->has_document_info) {
            return false;
        }
        output = found->document_info;
        return true;
    }

    bool CopyHistoryPresentation(
        SessionBinding binding,
        InkpodHistoryInfo& info,
        InkpodHistoryEntryKind& undo_kind,
        InkpodHistoryEntryKind& redo_kind) const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end() || !found->has_history_info) {
            return false;
        }
        info = found->history_info;
        undo_kind = found->undo_kind;
        redo_kind = found->redo_kind;
        return true;
    }

    bool CopyEditTargetPresentation(
        SessionBinding binding,
        std::uint64_t& target_count,
        InkpodEditTargetCapabilities& capabilities) const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end()) {
            return false;
        }
        target_count = found->edit_target_count;
        capabilities = found->edit_target_capabilities;
        return found->has_edit_target_presentation
            && (target_count == 0U || found->has_edit_target_capabilities);
    }

    bool CopySnapshotTransform(
        SessionBinding binding,
        std::uint64_t view_id,
        InkpodSnapshotTransform& transform) const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end() || !found->has_snapshot_transform
            || found->snapshot_view_id != view_id) {
            return false;
        }
        transform = found->snapshot_transform;
        return true;
    }

    bool PushPrimitive(PrimitiveWork item) noexcept {
        std::lock_guard acceptance_lock(acceptance_mutex);
        const SessionBinding binding = item.binding;
        {
            std::lock_guard state_lock(state_mutex);
            const auto session = FindPublishedLocked(binding);
            if (session == published.end() || !session->state.accepting_work) {
                if (session != published.end()) {
                    ++session->metrics.rejected_work_items;
                }
                return false;
            }
            item.sequence = ++session->state.last_accepted_sequence;
            ++session->state.pending_operations;
        }
        const std::uint64_t sequence = item.sequence;
        bool queued{};
        try {
            std::lock_guard lock(mutex);
            if (!stopping && work.size() < kMaximumQueuedWork) {
                work.emplace_back(std::move(item));
                queued = true;
            }
        } catch (const std::bad_alloc&) {
            queued = false;
        }
        if (!queued) {
            RollbackPending(binding, sequence);
            RecordRejected(binding);
            return false;
        }
        RecordAccepted(binding);
        wake.notify_one();
        return true;
    }

    bool CopyEditorState(
        SessionBinding binding, InkpodEditorStateInfo& output) const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end() || !found->has_editor_state) {
            return false;
        }
        output = found->editor_state;
        return true;
    }

    bool StoreEditorState(
        SessionBinding binding,
        InkpodCore* core,
        const InkpodEditorStateInfo& state) noexcept {
        {
            std::lock_guard lock(state_mutex);
            const auto found = FindPublishedLocked(binding);
            if (found == published.end() || !found->state.accepting_work) {
                return false;
            }
            found->editor_state = state;
            found->has_editor_state = true;
        }
        RefreshPublishedPresentation(binding, core);
        return true;
    }

    bool StoreDocumentAndEditorState(
        SessionBinding binding,
        InkpodCore* core,
        const InkpodDocumentInfo& document,
        const InkpodEditorStateInfo& editor) noexcept {
        {
            std::lock_guard lock(state_mutex);
            const auto found = FindPublishedLocked(binding);
            if (found == published.end() || !found->state.accepting_work) {
                return false;
            }
            found->document_info = document;
            found->has_document_info = true;
            found->editor_state = editor;
            found->has_editor_state = true;
        }
        RefreshPublishedPresentation(binding, core);
        return true;
    }

    std::wstring CopyLastError(SessionBinding binding) const {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        return found == published.end() ? host_error : found->last_error;
    }

    void StoreLocalFailure(SessionBinding binding, std::wstring_view message) noexcept {
        try {
            std::lock_guard lock(state_mutex);
            const auto found = FindPublishedLocked(binding);
            if (found != published.end()) {
                found->last_error.assign(message.data(), message.size());
            }
        } catch (const std::bad_alloc&) {
            std::lock_guard lock(state_mutex);
            const auto found = FindPublishedLocked(binding);
            if (found != published.end()) {
                found->last_error.clear();
            }
        }
    }

    EngineMetrics CopyMetrics(SessionBinding binding) const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        return found == published.end() ? EngineMetrics{} : found->metrics;
    }

    bool CopySessionState(SessionBinding binding, CoreSessionState& output) const noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = FindPublishedLocked(binding);
        if (found == published.end()) {
            return false;
        }
        output = found->state;
        return true;
    }

    void PostNotification(
        CoreNotificationKind kind,
        const CommandContext& context,
        InkpodStatus status) noexcept {
        if (!context.generation.has_value() || !context.document_session.has_value()) {
            return;
        }
        std::uint64_t token{};
        {
            std::lock_guard lock(state_mutex);
            if (kind == CoreNotificationKind::StateChanged) {
                const auto existing = std::find_if(
                    notifications.begin(), notifications.end(), [&context](const CoreNotification& item) {
                        return item.kind == CoreNotificationKind::StateChanged
                            && item.context.document_session == context.document_session
                            && item.context.generation == context.generation;
                    });
                if (existing != notifications.end()) {
                    return;
                }
            }
            if (notifications.size() >= kMaximumNotifications) {
                const auto state_change = std::find_if(
                    notifications.begin(), notifications.end(), [](const CoreNotification& item) {
                        return item.kind == CoreNotificationKind::StateChanged;
                    });
                if (state_change == notifications.end()) {
                    return;
                }
                notifications.erase(state_change);
            }
            token = next_notification_token++;
            if (next_notification_token == 0U) {
                next_notification_token = 1U;
            }
            if (token == 0U) {
                token = next_notification_token++;
            }
            try {
                notifications.push_back(
                    CoreNotification{token, kind, context, status, {}});
            } catch (const std::bad_alloc&) {
                return;
            }
        }
        const UINT message = NotificationMessage(kind);
        bool posted{};
        {
            std::lock_guard lock(notification_owner_mutex);
            posted = PostMessageW(
                         owner,
                         message,
                         static_cast<WPARAM>(token),
                         static_cast<LPARAM>(context.generation->Value()))
                != FALSE;
        }
        if (!posted) {
            CoreNotification ignored{};
            (void)TakeNotification(token, context.generation.value(), ignored);
        }
    }

    void PostInkScriptNotification(
        const CommandContext& context,
        const InkScriptEngineResult& result) noexcept {
        if (!context.generation.has_value() || !context.document_session.has_value()) {
            return;
        }
        std::uint64_t token{};
        {
            std::lock_guard lock(state_mutex);
            const auto existing = std::find_if(
                notifications.begin(),
                notifications.end(),
                [&result](const CoreNotification& item) {
                    return item.kind == CoreNotificationKind::InkScript
                        && item.inkscript.job_id == result.job_id;
                });
            if (existing != notifications.end()) {
                existing->status = result.status;
                existing->inkscript = result;
                return;
            }
            if (notifications.size() >= kMaximumNotifications) {
                auto replaceable = std::find_if(
                    notifications.begin(),
                    notifications.end(),
                    [](const CoreNotification& item) {
                        return item.kind == CoreNotificationKind::StateChanged;
                    });
                if (replaceable == notifications.end()) {
                    replaceable = std::find_if(
                        notifications.begin(),
                        notifications.end(),
                        [](const CoreNotification& item) {
                            return item.kind == CoreNotificationKind::InkScript
                                && item.inkscript.kind
                                    == InkScriptEngineNotificationKind::Progress;
                        });
                }
                if (replaceable == notifications.end()) {
                    if (result.kind == InkScriptEngineNotificationKind::Progress) {
                        return;
                    }
                    replaceable = notifications.begin();
                }
                notifications.erase(replaceable);
            }
            token = next_notification_token++;
            if (next_notification_token == 0U) {
                next_notification_token = 1U;
            }
            if (token == 0U) {
                token = next_notification_token++;
            }
            try {
                notifications.push_back(CoreNotification{
                    token,
                    CoreNotificationKind::InkScript,
                    context,
                    result.status,
                    result});
            } catch (const std::bad_alloc&) {
                return;
            }
        }
        bool posted{};
        {
            std::lock_guard lock(notification_owner_mutex);
            posted = PostMessageW(
                         owner,
                         kCoreInkScriptNotification,
                         static_cast<WPARAM>(token),
                         static_cast<LPARAM>(context.generation->Value()))
                != FALSE;
        }
        if (!posted) {
            CoreNotification ignored{};
            (void)TakeNotification(token, context.generation.value(), ignored);
        }
    }

    static UINT NotificationMessage(CoreNotificationKind kind) noexcept {
        switch (kind) {
            case CoreNotificationKind::StateChanged:
                return kCoreStateChanged;
            case CoreNotificationKind::AsyncFailed:
                return kCoreAsyncFailed;
            case CoreNotificationKind::InkScript:
                return kCoreInkScriptNotification;
        }
        return kCoreAsyncFailed;
    }

    bool TakeNotification(
        std::uint64_t token,
        Generation generation,
        CoreNotification& output) noexcept {
        std::lock_guard lock(state_mutex);
        const auto found = std::find_if(
            notifications.begin(), notifications.end(), [token, generation](const CoreNotification& item) {
                return item.token == token && item.context.generation == generation;
            });
        if (found == notifications.end()) {
            return false;
        }
        output = std::move(*found);
        notifications.erase(found);
        return true;
    }

    renderer::CanvasSnapshotSink* initial_canvas{};
    mutable std::mutex notification_owner_mutex;
    HWND owner{};
    InkpodIoManager* io_manager{};

    mutable std::mutex acceptance_mutex;
    mutable std::mutex mutex;
    std::condition_variable wake;
    std::deque<WorkItem> work;
    std::vector<AdapterInput> adapter_inputs;
    std::vector<std::unique_ptr<InkScriptInput>> inkscript_inputs;
    std::vector<std::unique_ptr<FileIoInput>> file_io_inputs;
    std::uint64_t next_file_io_token{1U};
    AdapterInputToken next_adapter_input_token{1U};
    bool stopping{};
    std::thread worker;
    std::atomic<DWORD> thread_id{};

    std::vector<std::unique_ptr<CoreEntry>> entries;

    mutable std::mutex state_mutex;
    std::vector<PublishedSession> published;
    std::vector<FrontendViewBinding> frontend_views;
    std::vector<renderer::CanvasSnapshotSink*> snapshot_sinks;
    SessionBinding active{};
    std::wstring host_error{L"CoreHost is not running"};
    std::deque<CoreNotification> notifications;
    std::uint64_t next_notification_token{1U};

    std::mutex initializer_mutex;
    CoreOperation initializer;

    InkpodStatus SetActiveView(SessionBinding binding, std::uint64_t view_id) noexcept {
        try {
            auto completion = std::make_shared<std::promise<InkpodStatus>>();
            auto future = completion->get_future();
            AdapterWork item{
                binding,
                0U,
                SessionContext(binding),
                0U,
                true,
                false,
                false};
            AdapterInput input{
                0U,
                [](InkpodCore*) { return INKPOD_STATUS_OK; },
                view_id,
                completion,
                {}};
            if (!PushAdapter(std::move(item), std::move(input))) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return future.get();
        } catch (const std::future_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
};

CoreHost::CoreHost() = default;

CoreHost::~CoreHost() {
    Stop();
}

InkpodStatus CoreHost::Start(
    renderer::CanvasSnapshotSink* canvas,
    HWND owner,
    InkpodIoManager* io_manager) noexcept {
    if (impl_ != nullptr || canvas == nullptr || owner == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        impl_ = std::make_unique<Impl>(canvas, owner, io_manager);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = impl_->Start();
    if (status != INKPOD_STATUS_OK) {
        impl_->Stop();
        impl_.reset();
    }
    return status;
}

void CoreHost::Stop() noexcept {
    if (impl_ != nullptr) {
        impl_->Stop();
        impl_.reset();
    }
}

InkpodStatus CoreHost::CreateSession(
    DocumentSessionId session,
    Generation generation) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->CreateSession(SessionBinding{session, generation});
}

InkpodIoManager* CoreHost::IoManager() const noexcept {
    return impl_ == nullptr ? nullptr : impl_->io_manager;
}

InkpodStatus CoreHost::AdoptBatchResult(
    DocumentSessionId session,
    Generation generation,
    InkpodBatchReport* report,
    std::uint64_t result_index) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->AdoptBatchResult(
              SessionBinding{session, generation}, report, result_index);
}

InkpodStatus CoreHost::RebindSession(
    DocumentSessionId old_session,
    Generation old_generation,
    DocumentSessionId new_session,
    Generation new_generation) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->RebindSession(
              SessionBinding{old_session, old_generation},
              SessionBinding{new_session, new_generation});
}

InkpodStatus CoreHost::CloseSession(
    DocumentSessionId session,
    Generation generation) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->CloseSession(SessionBinding{session, generation});
}

bool CoreHost::SetActiveSession(
    DocumentSessionId session,
    Generation generation) noexcept {
    return impl_ != nullptr
        && impl_->SetActiveSession(SessionBinding{session, generation});
}

bool CoreHost::HasSession(
    DocumentSessionId session,
    Generation generation) const noexcept {
    return impl_ != nullptr && impl_->HasSession(SessionBinding{session, generation});
}

std::size_t CoreHost::SessionCount() const noexcept {
    return impl_ == nullptr ? 0U : impl_->SessionCount();
}

InkpodStatus CoreHost::Invoke(
    CoreOperation operation,
    bool publish_snapshot,
    bool refresh_document_info) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto binding = impl_->ActiveBinding();
    return binding.has_value()
        ? impl_->Invoke(
              binding.value(),
              std::move(operation),
              publish_snapshot,
              refresh_document_info)
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus CoreHost::Invoke(
    DocumentSessionId session,
    Generation generation,
    CoreOperation operation,
    bool publish_snapshot,
    bool refresh_document_info) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->Invoke(
              SessionBinding{session, generation},
              std::move(operation),
              publish_snapshot,
              refresh_document_info);
}

InkpodStatus CoreHost::InvokeAll(
    CoreOperation operation,
    bool publish_snapshot,
    bool refresh_document_info) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->InvokeAll(
              std::move(operation), publish_snapshot, refresh_document_info);
}

InkpodStatus CoreHost::CreateSubpalette(
    InkpodSubpalette** out_subpalette) noexcept {
    if (impl_ == nullptr || out_subpalette == nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    *out_subpalette = nullptr;
    return impl_->InvokeOwnerThread([out_subpalette] {
        return inkpod_subpalette_create(out_subpalette);
    });
}

InkpodStatus CoreHost::InvokeSubpalette(
    InkpodSubpalette* subpalette,
    SubpaletteOperation operation) noexcept {
    if (impl_ == nullptr || subpalette == nullptr || !operation) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    return impl_->InvokeOwnerThread(
        [subpalette, operation = std::move(operation)] {
            return operation(subpalette);
        });
}

bool CoreHost::EnqueueSubpalette(
    InkpodSubpalette* subpalette,
    SubpaletteOperation operation,
    std::function<void(InkpodStatus)> completion) noexcept {
    if (impl_ == nullptr || subpalette == nullptr || !operation || !completion) {
        return false;
    }
    return impl_->EnqueueOwnerThread(
        [subpalette, operation = std::move(operation)] {
            return operation(subpalette);
        },
        std::move(completion));
}

InkpodStatus CoreHost::ReleaseSubpalette(
    InkpodSubpalette** subpalette) noexcept {
    if (impl_ == nullptr || subpalette == nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (*subpalette == nullptr) {
        return INKPOD_STATUS_OK;
    }
    return impl_->InvokeOwnerThread([subpalette] {
        return inkpod_subpalette_release(subpalette);
    });
}

InkpodStatus CoreHost::InvokePrimitive(
    const InkpodPrimitiveRequestV3& request,
    bool publish_snapshot,
    bool refresh_document_info,
    bool defer_during_active_stroke) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto binding = impl_->ActiveBinding();
    return binding.has_value()
        ? impl_->InvokePrimitive(
              binding.value(),
              request,
              publish_snapshot,
              refresh_document_info,
              defer_during_active_stroke)
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus CoreHost::InvokePrimitive(
    DocumentSessionId session,
    Generation generation,
    const InkpodPrimitiveRequestV3& request,
    bool publish_snapshot,
    bool refresh_document_info,
    bool defer_during_active_stroke) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->InvokePrimitive(
              SessionBinding{session, generation},
              request,
              publish_snapshot,
              refresh_document_info,
              defer_during_active_stroke);
}

bool CoreHost::EnqueuePrimitive(
    const CommandContext& context,
    const InkpodPrimitiveRequestV3& request,
    bool publish_snapshot,
    bool refresh_document_info,
    bool defer_during_active_stroke) noexcept {
    return impl_ != nullptr
        && impl_->EnqueuePrimitive(
            context,
            request,
            publish_snapshot,
            refresh_document_info,
            defer_during_active_stroke);
}

bool CoreHost::Enqueue(
    const CommandContext& context,
    CoreOperation operation,
    bool publish_snapshot,
    bool refresh_document_info,
    bool defer_during_active_stroke,
    std::function<void(InkpodStatus)> completion) noexcept {
    return impl_ != nullptr
        && impl_->Enqueue(
            context,
            std::move(operation),
            publish_snapshot,
            refresh_document_info,
            defer_during_active_stroke,
            std::move(completion));
}

bool CoreHost::EnqueueInkScript(InkScriptEngineRequest request) noexcept {
    return impl_ != nullptr && impl_->EnqueueInkScript(std::move(request));
}

bool CoreHost::EnqueueFileIo(
    const CommandContext& context,
    bool requires_core,
    FileIoOperation operation,
    bool publish_snapshot,
    bool refresh_document_info,
    std::function<void(InkpodStatus)> completion) noexcept {
    return impl_ != nullptr && impl_->EnqueueFileIo(
        context, requires_core, std::move(operation), publish_snapshot,
        refresh_document_info, std::move(completion));
}

bool CoreHost::ConfirmInkScript(
    std::uint64_t job_id,
    const CommandContext& context,
    std::uint32_t scope) noexcept {
    return impl_ != nullptr
        && impl_->ConfirmInkScript(job_id, context, scope);
}

bool CoreHost::CancelInkScript(
    std::uint64_t job_id,
    const CommandContext& context) noexcept {
    return impl_ != nullptr
        && impl_->CancelInkScript(job_id, context);
}

bool CoreHost::EnqueueStroke(StrokeEvent event) noexcept {
    return impl_ != nullptr && impl_->PushStroke(std::move(event));
}

InkpodStatus CoreHost::WaitIdle() noexcept {
    return Invoke([](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false);
}

InkpodStatus CoreHost::WaitIdle(
    DocumentSessionId session,
    Generation generation) noexcept {
    return Invoke(
        session,
        generation,
        [](InkpodCore*) { return INKPOD_STATUS_OK; },
        false,
        false);
}

InkpodStatus CoreHost::FlushPreview() noexcept {
    return Invoke([](InkpodCore*) { return INKPOD_STATUS_OK; }, true, false);
}

InkpodStatus CoreHost::SetActiveView(std::uint64_t view_id) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto binding = impl_->ActiveBinding();
    if (!binding.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return impl_->SetActiveView(binding.value(), view_id);
}

bool CoreHost::RetargetNotificationOwner(
    HWND expected_owner,
    HWND replacement_owner) noexcept {
    return impl_ != nullptr
        && impl_->RetargetNotificationOwner(
            expected_owner, replacement_owner);
}

bool CoreHost::RegisterSnapshotSink(
    renderer::CanvasSnapshotSink* canvas) noexcept {
    return impl_ != nullptr && impl_->RegisterSnapshotSink(canvas);
}

bool CoreHost::UnregisterSnapshotSink(
    renderer::CanvasSnapshotSink* canvas) noexcept {
    return UnregisterSnapshotSinks(&canvas, 1U);
}

bool CoreHost::UnregisterSnapshotSinks(
    renderer::CanvasSnapshotSink* const* canvases,
    std::size_t count) noexcept {
    return impl_ != nullptr
        && impl_->UnregisterSnapshotSinks(canvases, count);
}

std::size_t CoreHost::SnapshotSinkCount() const noexcept {
    return impl_ == nullptr ? 0U : impl_->SnapshotSinkCount();
}

bool CoreHost::RegisterDocumentView(
    DocumentSessionId session,
    Generation generation,
    DocumentViewId frontend_view,
    std::uint64_t core_view_id) noexcept {
    return impl_ != nullptr
        && impl_->RegisterDocumentView(
            SessionBinding{session, generation}, frontend_view, core_view_id);
}

bool CoreHost::UnregisterDocumentView(
    DocumentSessionId session,
    Generation generation,
    DocumentViewId frontend_view) noexcept {
    return impl_ != nullptr
        && impl_->UnregisterDocumentView(
            SessionBinding{session, generation}, frontend_view);
}

bool CoreHost::GetDocumentInfo(InkpodDocumentInfo& info) const noexcept {
    if (impl_ == nullptr) {
        return false;
    }
    const auto binding = impl_->ActiveBinding();
    return binding.has_value() && impl_->CopyDocumentInfo(binding.value(), info);
}

bool CoreHost::GetDocumentInfo(
    DocumentSessionId session,
    Generation generation,
    InkpodDocumentInfo& info) const noexcept {
    return impl_ != nullptr
        && impl_->CopyDocumentInfo(SessionBinding{session, generation}, info);
}

bool CoreHost::GetHistoryPresentation(
    DocumentSessionId session,
    Generation generation,
    InkpodHistoryInfo& info,
    InkpodHistoryEntryKind& undo_kind,
    InkpodHistoryEntryKind& redo_kind) const noexcept {
    return impl_ != nullptr
        && impl_->CopyHistoryPresentation(
            SessionBinding{session, generation}, info, undo_kind, redo_kind);
}

bool CoreHost::GetEditTargetPresentation(
    DocumentSessionId session,
    Generation generation,
    std::uint64_t& target_count,
    InkpodEditTargetCapabilities& capabilities) const noexcept {
    return impl_ != nullptr
        && impl_->CopyEditTargetPresentation(
            SessionBinding{session, generation}, target_count, capabilities);
}

bool CoreHost::GetSnapshotTransform(
    DocumentSessionId session,
    Generation generation,
    std::uint64_t view_id,
    InkpodSnapshotTransform& transform) const noexcept {
    return impl_ != nullptr
        && impl_->CopySnapshotTransform(
            SessionBinding{session, generation}, view_id, transform);
}

InkpodStatus CoreHost::GetReplayContract(
    DocumentSessionId session,
    Generation generation,
    InkpodReplayContract& contract) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    contract = {};
    contract.struct_size = sizeof(contract);
    return impl_->Invoke(
        SessionBinding{session, generation},
        [&contract](InkpodCore* core) {
            return inkpod_core_get_replay_contract(core, &contract);
        },
        false,
        false);
}

InkpodStatus CoreHost::GetPersistenceInfo(
    DocumentSessionId session,
    Generation generation,
    InkpodPersistenceInfo& info) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    info = {};
    info.struct_size = sizeof(info);
    return impl_->Invoke(
        SessionBinding{session, generation},
        [&info](InkpodCore* core) {
            return inkpod_core_get_persistence_info(core, &info);
        },
        false,
        false);
}

InkpodStatus CoreHost::GetCompactionPlan(
    DocumentSessionId session,
    Generation generation,
    InkpodCompactionPlan& plan) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    plan = {};
    plan.struct_size = sizeof(plan);
    return impl_->Invoke(
        SessionBinding{session, generation},
        [&plan](InkpodCore* core) {
            return inkpod_core_compaction_plan(core, &plan);
        },
        false,
        false);
}

InkpodStatus CoreHost::WriteCompactedCopy(
    DocumentSessionId session,
    Generation generation,
    std::string_view path_utf8,
    const InkpodCompactionPlan& plan) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        std::string path{path_utf8};
        return impl_->Invoke(
            SessionBinding{session, generation},
            [path = std::move(path), plan](InkpodCore* core) {
                return inkpod_core_write_compacted_copy(
                    core,
                    reinterpret_cast<const std::uint8_t*>(path.data()),
                    static_cast<std::uint64_t>(path.size()),
                    &plan);
            },
            false,
            false);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodStatus CoreHost::GetEditorDefaults(
    DocumentSessionId session,
    Generation generation,
    InkpodEditorDefaults& defaults) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    defaults = {};
    defaults.struct_size = sizeof(defaults);
    defaults.state.struct_size = sizeof(defaults.state);
    return impl_->Invoke(
        SessionBinding{session, generation},
        [&defaults](InkpodCore* core) {
            return inkpod_core_get_editor_defaults(core, &defaults);
        },
        false,
        false);
}

InkpodStatus CoreHost::RefreshEditorState(
    DocumentSessionId session, Generation generation) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const SessionBinding binding{session, generation};
    Impl* const impl = impl_.get();
    return impl_->Invoke(
        binding,
        [impl, binding](InkpodCore* core) {
            InkpodEditorStateInfo state{};
            state.struct_size = sizeof(state);
            const InkpodStatus status =
                inkpod_core_get_editor_state(core, &state);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            return impl->StoreEditorState(binding, core, state)
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        false,
        false);
}

InkpodStatus CoreHost::UpdateEditorState(
    DocumentSessionId session,
    Generation generation,
    const InkpodEditorStateUpdate& update) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const SessionBinding binding{session, generation};
    Impl* const impl = impl_.get();
    return impl_->Invoke(
        binding,
        [impl, binding, &update](InkpodCore* core) {
            InkpodEditorStateInfo updated{};
            updated.struct_size = sizeof(updated);
            const InkpodStatus update_status =
                inkpod_core_update_editor_state(core, &update, &updated);
            if (update_status != INKPOD_STATUS_OK) {
                return update_status;
            }
            InkpodDocumentInfo document{};
            document.struct_size = sizeof(document);
            const InkpodStatus query_status =
                inkpod_core_get_document_info(core, &document);
            if (query_status != INKPOD_STATUS_OK) {
                return query_status;
            }
            return impl->StoreDocumentAndEditorState(
                       binding, core, document, updated)
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        false,
        false);
}

bool CoreHost::GetEditorState(
    DocumentSessionId session,
    Generation generation,
    InkpodEditorStateInfo& state) const noexcept {
    return impl_ != nullptr
        && impl_->CopyEditorState(SessionBinding{session, generation}, state);
}

InkpodStatus CoreHost::GetEditTargets(
    DocumentSessionId session,
    Generation generation,
    std::vector<InkpodEditTarget>& targets) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    targets.clear();
    return impl_->Invoke(
        SessionBinding{session, generation},
        [&targets](InkpodCore* core) {
            std::uint64_t count{};
            InkpodStatus status = inkpod_core_get_edit_targets(
                core, nullptr, 0U, 0U, &count);
            if (status == INKPOD_STATUS_OK) {
                return status;
            }
            if (status != INKPOD_STATUS_BUFFER_TOO_SMALL
                || count > INKPOD_MAX_EDIT_TARGETS) {
                return status == INKPOD_STATUS_BUFFER_TOO_SMALL
                    ? INKPOD_STATUS_INVALID_STATE
                    : status;
            }
            try {
                targets.resize(static_cast<std::size_t>(count));
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return inkpod_core_get_edit_targets(
                core,
                targets.data(),
                targets.size(),
                sizeof(InkpodEditTarget),
                &count);
        },
        false,
        false);
}

InkpodStatus CoreHost::GetEditTargetCapabilities(
    DocumentSessionId session,
    Generation generation,
    InkpodEditTargetCapabilities& capabilities) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    capabilities = {};
    capabilities.struct_size = sizeof(capabilities);
    return impl_->Invoke(
        SessionBinding{session, generation},
        [&capabilities](InkpodCore* core) {
            return inkpod_core_get_edit_target_capabilities(core, &capabilities);
        },
        false,
        false);
}

InkpodStatus CoreHost::SetEditTargets(
    DocumentSessionId session,
    Generation generation,
    std::uint64_t expected_editor_revision,
    const std::vector<InkpodEditTarget>& targets) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const SessionBinding binding{session, generation};
    Impl* const impl = impl_.get();
    return impl_->Invoke(
        binding,
        [impl, binding, expected_editor_revision, &targets](InkpodCore* core) {
            InkpodEditorStateInfo updated{};
            updated.struct_size = sizeof(updated);
            const InkpodStatus status = inkpod_core_set_edit_targets(
                core,
                expected_editor_revision,
                targets.empty() ? nullptr : targets.data(),
                targets.size(),
                targets.empty() ? 0U : sizeof(InkpodEditTarget),
                &updated);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            InkpodDocumentInfo document{};
            document.struct_size = sizeof(document);
            const InkpodStatus query_status =
                inkpod_core_get_document_info(core, &document);
            return query_status == INKPOD_STATUS_OK
                && impl->StoreDocumentAndEditorState(
                    binding, core, document, updated)
                ? INKPOD_STATUS_OK
                : (query_status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE
                    : query_status);
        },
        false,
        false);
}

InkpodStatus CoreHost::ApplyEditTargetCommand(
    DocumentSessionId session,
    Generation generation,
    const InkpodEditTargetCommand& command,
    InkpodDispatchResult& result,
    std::vector<InkpodEditTarget>& output_targets) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const SessionBinding binding{session, generation};
    Impl* const impl = impl_.get();
    output_targets.clear();
    result = {};
    result.struct_size = sizeof(result);
    return impl_->Invoke(
        binding,
        [impl, binding, &command, &result, &output_targets](InkpodCore* core) {
            std::uint64_t count{};
            InkpodStatus status = inkpod_core_apply_edit_target_command(
                core, &command, &result, nullptr, 0U, 0U, &count);
            if (status == INKPOD_STATUS_BUFFER_TOO_SMALL) {
                if (count > INKPOD_MAX_EDIT_TARGETS) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                try {
                    output_targets.resize(static_cast<std::size_t>(count));
                } catch (const std::bad_alloc&) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                status = inkpod_core_apply_edit_target_command(
                    core,
                    &command,
                    &result,
                    output_targets.data(),
                    output_targets.size(),
                    sizeof(InkpodEditTarget),
                    &count);
            }
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            InkpodDocumentInfo document{};
            document.struct_size = sizeof(document);
            InkpodEditorStateInfo editor{};
            editor.struct_size = sizeof(editor);
            const InkpodStatus document_status =
                inkpod_core_get_document_info(core, &document);
            const InkpodStatus editor_status =
                inkpod_core_get_editor_state(core, &editor);
            return document_status == INKPOD_STATUS_OK
                && editor_status == INKPOD_STATUS_OK
                && impl->StoreDocumentAndEditorState(
                    binding, core, document, editor)
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        false,
        false);
}

InkpodStatus CoreHost::RegisterColorArray(
    DocumentSessionId session,
    Generation generation,
    const InkpodColorArray& input,
    InkpodObjectId& object_id) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    object_id = {};
    object_id.struct_size = sizeof(object_id);
    return impl_->Invoke(
        SessionBinding{session, generation},
        [&input, &object_id](InkpodCore* core) {
            return inkpod_core_register_color_array_v3(
                core, &input, &object_id);
        },
        false,
        false);
}

InkpodStatus CoreHost::ReleaseObject(
    DocumentSessionId session,
    Generation generation,
    const InkpodObjectId& object_id) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->Invoke(
              SessionBinding{session, generation},
              [&object_id](InkpodCore* core) {
                  return inkpod_core_object_release_v3(core, &object_id);
              },
              false,
              false);
}

std::wstring CoreHost::LastError() const {
    if (impl_ == nullptr) {
        return L"CoreHost is not running";
    }
    const auto binding = impl_->ActiveBinding();
    return binding.has_value()
        ? impl_->CopyLastError(binding.value())
        : L"No active Core session";
}

std::wstring CoreHost::LastError(
    DocumentSessionId session,
    Generation generation) const {
    return impl_ == nullptr
        ? L"CoreHost is not running"
        : impl_->CopyLastError(SessionBinding{session, generation});
}

void CoreHost::SetLocalFailure(std::wstring_view message) noexcept {
    if (impl_ == nullptr) {
        return;
    }
    const auto binding = impl_->ActiveBinding();
    if (binding.has_value()) {
        impl_->StoreLocalFailure(binding.value(), message);
    }
}

EngineMetrics CoreHost::Metrics() const noexcept {
    if (impl_ == nullptr) {
        return {};
    }
    const auto binding = impl_->ActiveBinding();
    return binding.has_value() ? impl_->CopyMetrics(binding.value()) : EngineMetrics{};
}

bool CoreHost::GetMetrics(
    DocumentSessionId session,
    Generation generation,
    EngineMetrics& metrics) const noexcept {
    const SessionBinding binding{session, generation};
    if (impl_ == nullptr || !impl_->HasSession(binding)) {
        return false;
    }
    metrics = impl_->CopyMetrics(binding);
    return true;
}

bool CoreHost::GetSessionState(
    DocumentSessionId session,
    Generation generation,
    CoreSessionState& state) const noexcept {
    return impl_ != nullptr
        && impl_->CopySessionState(SessionBinding{session, generation}, state);
}

DWORD CoreHost::ThreadId() const noexcept {
    return impl_ == nullptr ? 0U : impl_->thread_id.load(std::memory_order_acquire);
}

void CoreHost::SetSessionInitializer(CoreOperation initializer) noexcept {
    if (impl_ != nullptr) {
        impl_->SetSessionInitializer(std::move(initializer));
    }
}

bool CoreHost::TakeNotification(
    std::uint64_t token,
    Generation generation,
    CoreNotification& notification) noexcept {
    return impl_ != nullptr
        && impl_->TakeNotification(token, generation, notification);
}

}  // namespace inkpod::app
