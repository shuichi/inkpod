#include "core_engine.h"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <deque>
#include <future>
#include <limits>
#include <mutex>
#include <thread>
#include <utility>
#include <variant>

#include "canvas.h"

namespace inkpod::app {
namespace {

constexpr std::size_t kMaximumQueuedWork = 4096U;
constexpr std::size_t kMaximumStrokeSamples = 1048576U;
constexpr auto kPreviewFrameInterval = std::chrono::milliseconds(8);

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

struct SyncWork {
    CommandContext context;
    std::function<InkpodStatus(InkpodCore*)> operation;
    bool publish_snapshot{};
    bool refresh_document_info{};
    bool defer_during_active_stroke{};
    std::shared_ptr<std::promise<InkpodStatus>> completion;
    std::function<void(InkpodStatus)> async_completion;
};

using WorkItem = std::variant<SyncWork, StrokeEvent>;

}  // namespace

struct CoreEngine::Impl final {
    explicit Impl(renderer::CanvasSnapshotSink* canvas_window, HWND owner_window) noexcept
        : canvas(canvas_window), owner(owner_window) {}

    InkpodStatus Start() noexcept {
        try {
            auto ready = std::make_shared<std::promise<InkpodStatus>>();
            auto future = ready->get_future();
            worker = std::thread([this, ready] { Run(ready); });
            return future.get();
        } catch (const std::system_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    void Stop() noexcept {
        {
            std::lock_guard lock(mutex);
            stopping = true;
        }
        wake.notify_one();
        if (worker.joinable()) {
            worker.join();
        }
    }

    bool Push(WorkItem item) noexcept {
        try {
            std::lock_guard lock(mutex);
            if (stopping || work.size() >= kMaximumQueuedWork) {
                return false;
            }
            work.push_back(std::move(item));
        } catch (const std::bad_alloc&) {
            return false;
        }
        wake.notify_one();
        return true;
    }

    bool PushStroke(StrokeEvent event) noexcept {
        try {
            std::lock_guard lock(mutex);
            if (stopping) {
                return false;
            }
            if (event.kind == StrokeEventKind::Append && !work.empty()) {
                if (auto* pending = std::get_if<StrokeEvent>(&work.back());
                    pending != nullptr && pending->kind == StrokeEventKind::Append
                    && pending->context == event.context
                    && pending->samples.size()
                            <= kMaximumStrokeSamples - std::min(
                                   kMaximumStrokeSamples, event.samples.size())) {
                    pending->samples.insert(
                        pending->samples.end(), event.samples.begin(), event.samples.end());
                    wake.notify_one();
                    return true;
                }
            }
            if (event.kind == StrokeEventKind::Cancel && work.size() >= kMaximumQueuedWork) {
                while (!work.empty() && work.size() >= kMaximumQueuedWork) {
                    const auto* pending = std::get_if<StrokeEvent>(&work.back());
                    if (pending == nullptr || pending->kind != StrokeEventKind::Append) {
                        break;
                    }
                    work.pop_back();
                }
            }
            if (work.size() >= kMaximumQueuedWork) {
                return false;
            }
            work.emplace_back(std::move(event));
        } catch (const std::bad_alloc&) {
            return false;
        }
        wake.notify_one();
        return true;
    }

    InkpodStatus Invoke(
        std::function<InkpodStatus(InkpodCore*)> operation,
        bool publish_snapshot,
        bool refresh_document_info) noexcept {
        try {
            auto completion = std::make_shared<std::promise<InkpodStatus>>();
            auto future = completion->get_future();
            if (!Push(SyncWork{
                    CurrentContext(),
                    std::move(operation),
                    publish_snapshot,
                    refresh_document_info,
                    false,
                    completion,
                    {}})) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return future.get();
        } catch (const std::future_error&) {
            return INKPOD_STATUS_INVALID_STATE;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }

    bool Enqueue(
        const CommandContext& context,
        std::function<InkpodStatus(InkpodCore*)> operation,
        bool publish_snapshot,
        bool refresh_document_info,
        bool defer_during_active_stroke,
        std::function<void(InkpodStatus)> completion) noexcept {
        return Push(SyncWork{
            context,
            std::move(operation),
            publish_snapshot,
            refresh_document_info,
            defer_during_active_stroke,
            nullptr,
            std::move(completion)});
    }

    bool CopyDocumentInfo(InkpodDocumentInfo& output) const noexcept {
        std::lock_guard lock(state_mutex);
        if (!has_document_info) {
            return false;
        }
        output = document_info;
        return true;
    }

    std::wstring CopyLastError() const {
        std::lock_guard lock(state_mutex);
        return last_error;
    }

    void StoreLocalFailure(std::wstring_view message) noexcept {
        try {
            std::lock_guard lock(state_mutex);
            last_error.assign(message.data(), message.size());
        } catch (const std::bad_alloc&) {
            std::lock_guard lock(state_mutex);
            last_error = L"Local error text allocation failed";
        }
    }

    EngineMetrics CopyMetrics() const noexcept {
        std::lock_guard lock(state_mutex);
        return metrics;
    }

    void CaptureFailure(InkpodStatus status, bool asynchronous) noexcept {
        if (status == INKPOD_STATUS_OK
            || (asynchronous && status == INKPOD_STATUS_CANCELLED)) {
            return;
        }
        try {
            std::lock_guard lock(state_mutex);
            last_error = ReadCoreErrorOnCurrentThread();
        } catch (const std::bad_alloc&) {
            std::lock_guard lock(state_mutex);
            last_error = L"Core error text allocation failed";
        }
        if (asynchronous) {
            PostMessageW(
                owner,
                kCoreAsyncFailed,
                status,
                static_cast<LPARAM>(command_generation.load(
                    std::memory_order_acquire)));
        }
    }

    InkpodStatus RefreshDocumentInfo(InkpodCore* core) noexcept {
        InkpodDocumentInfo info{};
        info.struct_size = sizeof(info);
        const InkpodStatus status = inkpod_core_get_document_info(core, &info);
        if (status == INKPOD_STATUS_OK) {
            {
                std::lock_guard lock(state_mutex);
                document_info = info;
                has_document_info = true;
            }
            PostMessageW(
                owner,
                kCoreStateChanged,
                0,
                static_cast<LPARAM>(command_generation.load(
                    std::memory_order_acquire)));
        }
        return status;
    }

    InkpodStatus PublishSnapshot(InkpodCore* core, bool preview) noexcept {
        const InkpodSnapshotOptions options{
            sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
        InkpodSnapshot* snapshot{};
        const std::uint64_t view_id = active_view_id.load(std::memory_order_acquire);
        const InkpodStatus status = view_id == 0U
            ? inkpod_core_build_snapshot(core, &options, &snapshot)
            : inkpod_core_build_snapshot_for_view(core, view_id, &options, &snapshot);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        if (!canvas->Submit(snapshot)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        if (preview) {
            std::lock_guard lock(state_mutex);
            ++metrics.preview_snapshots;
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus AppendSamples(InkpodCore* core, const std::vector<InkpodStrokeSample>& samples)
        noexcept {
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
        return inkpod_core_stroke_append(core, &span);
    }

    void ProcessStroke(InkpodCore* core, StrokeEvent event) noexcept {
        if (!ContextIsCurrent(event.context)) {
            return;
        }
        InkpodStatus status = INKPOD_STATUS_OK;
        switch (event.kind) {
            case StrokeEventKind::Begin: {
                if (event.samples.empty()) {
                    status = INKPOD_STATUS_INVALID_ARGUMENT;
                    break;
                }
                const InkpodStrokeInput input{
                    sizeof(InkpodStrokeInput),
                    event.style.tool,
                    event.style.plane,
                    event.style.coordinate_space,
                    event.style.flags,
                    event.style.color_rgba,
                    event.style.diameter,
                    event.samples.data(),
                    static_cast<std::uint64_t>(event.samples.size()),
                    sizeof(InkpodStrokeSample)};
                status = inkpod_core_stroke_begin(core, &input);
                if (status == INKPOD_STATUS_OK) {
                    stroke_active = true;
                    active_sample_count = event.samples.size();
                    status = PublishSnapshot(core, true);
                    preview_dirty = false;
                    next_preview_frame = std::chrono::steady_clock::now()
                        + kPreviewFrameInterval;
                }
                break;
            }
            case StrokeEventKind::Append:
                status = AppendSamples(core, event.samples);
                if (status == INKPOD_STATUS_OK) {
                    active_sample_count += event.samples.size();
                    preview_dirty = true;
                }
                break;
            case StrokeEventKind::End: {
                status = AppendSamples(core, event.samples);
                if (status == INKPOD_STATUS_OK) {
                    active_sample_count += event.samples.size();
                    InkpodDispatchResult result{};
                    result.struct_size = sizeof(result);
                    status = inkpod_core_stroke_end(core, &result);
                    stroke_active = false;
                }
                if (status == INKPOD_STATUS_OK) {
                    {
                        std::lock_guard lock(state_mutex);
                        ++metrics.completed_strokes;
                        metrics.completed_samples += active_sample_count;
                    }
                    active_sample_count = 0;
                    preview_dirty = false;
                    const InkpodStatus info_status = RefreshDocumentInfo(core);
                    status = info_status == INKPOD_STATUS_OK
                        ? PublishSnapshot(core, false)
                        : info_status;
                }
                break;
            }
            case StrokeEventKind::Cancel:
                status = inkpod_core_stroke_cancel(core);
                stroke_active = false;
                active_sample_count = 0;
                preview_dirty = false;
                if (status == INKPOD_STATUS_OK) {
                    status = PublishSnapshot(core, false);
                }
                break;
        }
        if (status != INKPOD_STATUS_OK) {
            inkpod_core_stroke_cancel(core);
            stroke_active = false;
            active_sample_count = 0;
            preview_dirty = false;
            PublishSnapshot(core, false);
            CaptureFailure(status, true);
        }
    }

    void ProcessSync(InkpodCore* core, SyncWork item) noexcept {
        InkpodStatus status = ContextIsCurrent(item.context)
            ? item.operation(core)
            : INKPOD_STATUS_CANCELLED;
        if (status == INKPOD_STATUS_OK && item.refresh_document_info) {
            status = RefreshDocumentInfo(core);
        }
        if (status == INKPOD_STATUS_OK && item.publish_snapshot) {
            status = PublishSnapshot(core, false);
        }
        const bool asynchronous = item.completion == nullptr;
        CaptureFailure(status, asynchronous);
        if (item.completion != nullptr) {
            item.completion->set_value(status);
        }
        if (item.async_completion) {
            try {
                item.async_completion(status);
            } catch (...) {
                CaptureFailure(INKPOD_STATUS_INVALID_STATE, true);
            }
        }
    }

    void Run(const std::shared_ptr<std::promise<InkpodStatus>>& ready) noexcept {
        thread_id = GetCurrentThreadId();
        InkpodCore* core{};
        const InkpodCoreConfig config{
            sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
        const InkpodStatus create_status = inkpod_core_create(&config, &core);
        if (create_status != INKPOD_STATUS_OK) {
            CaptureFailure(create_status, false);
            ready->set_value(create_status);
            return;
        }
        ready->set_value(INKPOD_STATUS_OK);

        for (;;) {
            WorkItem item;
            bool has_item = false;
            bool cancel_for_shutdown = false;
            {
                std::unique_lock lock(mutex);
                const auto deadline = preview_dirty
                    ? next_preview_frame
                    : std::chrono::steady_clock::time_point::max();
                const auto can_process = [this](const WorkItem& candidate) noexcept {
                    const auto* sync = std::get_if<SyncWork>(&candidate);
                    return !stroke_active || sync == nullptr
                        || !sync->defer_during_active_stroke;
                };
                wake.wait_until(lock, deadline, [this, &can_process] {
                    return stopping
                        || std::any_of(work.cbegin(), work.cend(), can_process);
                });
                auto next = std::find_if(work.begin(), work.end(), can_process);
                if (next != work.end()) {
                    const bool was_front = next == work.begin();
                    item = std::move(*next);
                    work.erase(next);
                    if (auto* stroke = std::get_if<StrokeEvent>(&item);
                        was_front && stroke != nullptr
                        && stroke->kind == StrokeEventKind::Append) {
                        while (!work.empty()) {
                            auto* pending_append = std::get_if<StrokeEvent>(&work.front());
                            if (pending_append == nullptr
                                || pending_append->kind != StrokeEventKind::Append
                                || pending_append->context != stroke->context
                                || stroke->samples.size()
                                        > kMaximumStrokeSamples - std::min(
                                               kMaximumStrokeSamples,
                                               pending_append->samples.size())) {
                                break;
                            }
                            stroke->samples.insert(
                                stroke->samples.end(),
                                pending_append->samples.begin(),
                                pending_append->samples.end());
                            work.pop_front();
                        }
                    }
                    has_item = true;
                } else if (stopping) {
                    cancel_for_shutdown = stroke_active;
                    if (!cancel_for_shutdown) {
                        break;
                    }
                }
            }

            if (cancel_for_shutdown) {
                inkpod_core_stroke_cancel(core);
                stroke_active = false;
                active_sample_count = 0;
                preview_dirty = false;
                continue;
            }
            if (has_item) {
                if (auto* sync = std::get_if<SyncWork>(&item)) {
                    ProcessSync(core, std::move(*sync));
                } else {
                    ProcessStroke(core, std::move(std::get<StrokeEvent>(item)));
                }
            }
            if (preview_dirty && std::chrono::steady_clock::now() >= next_preview_frame) {
                const InkpodStatus status = PublishSnapshot(core, true);
                if (status != INKPOD_STATUS_OK) {
                    inkpod_core_stroke_cancel(core);
                    stroke_active = false;
                    preview_dirty = false;
                    active_sample_count = 0;
                    CaptureFailure(status, true);
                } else {
                    preview_dirty = false;
                    next_preview_frame = std::chrono::steady_clock::now()
                        + kPreviewFrameInterval;
                }
            }
        }

        inkpod_core_stroke_cancel(core);
        inkpod_core_destroy(&core);
    }

    renderer::CanvasSnapshotSink* canvas{};
    HWND owner{};
    mutable std::mutex mutex;
    std::condition_variable wake;
    std::deque<WorkItem> work;
    bool stopping{};
    std::thread worker;
    DWORD thread_id{};

    mutable std::mutex state_mutex;
    InkpodDocumentInfo document_info{};
    bool has_document_info{};
    std::wstring last_error;
    EngineMetrics metrics{};

    bool preview_dirty{};
    bool stroke_active{};
    std::chrono::steady_clock::time_point next_preview_frame{};
    std::uint64_t active_sample_count{};
    std::atomic<std::uint64_t> active_view_id{};
    std::atomic<std::uint64_t> command_generation{1U};

    [[nodiscard]] CommandContext CurrentContext() const noexcept {
        CommandContext context{};
        context.generation = Generation(
            command_generation.load(std::memory_order_acquire));
        return context;
    }

    [[nodiscard]] bool ContextIsCurrent(
        const CommandContext& context) const noexcept {
        return context.generation.has_value()
            && context.generation->Value()
                == command_generation.load(std::memory_order_acquire);
    }
};

CoreEngine::CoreEngine() = default;

CoreEngine::~CoreEngine() {
    Stop();
}

InkpodStatus CoreEngine::Start(
    renderer::CanvasSnapshotSink* canvas,
    HWND owner) noexcept {
    if (impl_ != nullptr || canvas == nullptr || owner == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        impl_ = std::make_unique<Impl>(canvas, owner);
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

void CoreEngine::Stop() noexcept {
    if (impl_ != nullptr) {
        impl_->Stop();
        impl_.reset();
    }
}

InkpodStatus CoreEngine::Invoke(
    std::function<InkpodStatus(InkpodCore*)> operation,
    bool publish_snapshot,
    bool refresh_document_info) noexcept {
    return impl_ == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : impl_->Invoke(std::move(operation), publish_snapshot, refresh_document_info);
}

bool CoreEngine::Enqueue(
    const CommandContext& context,
    std::function<InkpodStatus(InkpodCore*)> operation,
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

bool CoreEngine::EnqueueStroke(StrokeEvent event) noexcept {
    return impl_ != nullptr && impl_->PushStroke(std::move(event));
}

InkpodStatus CoreEngine::WaitIdle() noexcept {
    return Invoke([](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false);
}

InkpodStatus CoreEngine::FlushPreview() noexcept {
    return Invoke([](InkpodCore*) { return INKPOD_STATUS_OK; }, true, false);
}

InkpodStatus CoreEngine::SetActiveView(std::uint64_t view_id) noexcept {
    if (impl_ == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    impl_->active_view_id.store(view_id, std::memory_order_release);
    return FlushPreview();
}

void CoreEngine::SetCommandGeneration(Generation generation) noexcept {
    if (impl_ != nullptr && generation) {
        impl_->command_generation.store(
            generation.Value(), std::memory_order_release);
    }
}

bool CoreEngine::GetDocumentInfo(InkpodDocumentInfo& info) const noexcept {
    return impl_ != nullptr && impl_->CopyDocumentInfo(info);
}

std::wstring CoreEngine::LastError() const {
    return impl_ == nullptr ? L"Core engine is not running" : impl_->CopyLastError();
}

void CoreEngine::SetLocalFailure(std::wstring_view message) noexcept {
    if (impl_ != nullptr) {
        impl_->StoreLocalFailure(message);
    }
}

EngineMetrics CoreEngine::Metrics() const noexcept {
    return impl_ == nullptr ? EngineMetrics{} : impl_->CopyMetrics();
}

DWORD CoreEngine::ThreadId() const noexcept {
    return impl_ == nullptr ? 0U : impl_->thread_id;
}

}  // namespace inkpod::app
