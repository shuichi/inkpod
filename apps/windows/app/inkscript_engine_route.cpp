#include "inkscript_engine_route.h"

#include <windows.h>

#include <algorithm>
#include <atomic>
#include <cstring>
#include <limits>
#include <new>
#include <mutex>
#include <utility>

#include "inkscript_file_authority.h"

namespace inkpod::app {
namespace {

enum class RouteState : std::uint8_t {
    Initialize,
    Planning,
    AwaitingConfirmation,
    Running,
    Completed,
};

// Detached ABI owners remain on the current Core owner thread, including
// stack unwinding from allocation while copying their bounded output.
struct FragmentOwner final {
    explicit FragmentOwner(InkpodCore* owner) noexcept : core(owner) {}
    ~FragmentOwner() { if (value != nullptr) (void)Release(); }
    FragmentOwner(const FragmentOwner&) = delete;
    FragmentOwner& operator=(const FragmentOwner&) = delete;
    InkpodStatus Release() noexcept {
        return inkpod_core_inkscript_fragment_release(core, &value);
    }
    InkpodCore* core{};
    InkpodInkScriptFragment* value{};
};

struct ReportOwner final {
    ReportOwner() = default;
    ~ReportOwner() { if (value != nullptr) (void)Release(); }
    ReportOwner(const ReportOwner&) = delete;
    ReportOwner& operator=(const ReportOwner&) = delete;
    InkpodStatus Release() noexcept { return inkpod_inkscript_report_release(&value); }
    InkpodInkScriptReport* value{};
};

InkScriptEngineStep CompletedStep(InkScriptEngineResult result) noexcept {
    result.kind = InkScriptEngineNotificationKind::Completed;
    return InkScriptEngineStep{
        InkScriptEngineStepKind::Completed, 0U, true, result};
}

InkScriptEngineStep ContinueStep(
    const InkScriptEngineResult& base,
    const InkpodInkScriptTaskEvent* event = nullptr) noexcept {
    InkScriptEngineStep result{};
    result.kind = InkScriptEngineStepKind::Continue;
    if (event == nullptr) {
        return result;
    }
    result.delay_milliseconds = event->kind == INKPOD_INKSCRIPT_EVENT_WAIT_REQUESTED
        ? event->wait_milliseconds
        : 0U;
    result.has_notification = true;
    result.notification = base;
    result.notification.kind = InkScriptEngineNotificationKind::Progress;
    result.notification.status = INKPOD_STATUS_OK;
    result.notification.event_kind = event->kind;
    result.notification.task_state = event->task_state;
    result.notification.completed_items = event->completed_items;
    result.notification.total_items = event->total_items;
    result.notification.wait_milliseconds = event->wait_milliseconds;
    result.notification.outcome = event->outcome;
    result.notification.failure = event->failure;
    return result;
}

}  // namespace

struct InkScriptEngineTask::Impl final {
    explicit Impl(InkScriptEngineRequest input)
        : request(std::move(input)) {
        result.job_id = request.job_id;
        result.owner_thread_id = GetCurrentThreadId();
    }

    ~Impl() {
        if (state != RouteState::Completed) {
            Cancel();
        }
        for (auto*& value : staged) {
            (void)inkpod_core_inkscript_staged_result_release(owner_core, &value);
        }
        if (run_task != nullptr) {
            (void)ReleaseRunTask(owner_core);
        }
        if (plan_task != nullptr) {
            (void)ReleasePlanTask(owner_core);
        }
        if (confirmation != nullptr) {
            (void)inkpod_core_inkscript_confirmation_release(owner_core, &confirmation);
        }
        if (plan != nullptr) {
            (void)inkpod_core_inkscript_plan_release(owner_core, &plan);
        }
        if (program != nullptr) {
            (void)inkpod_core_inkscript_program_release(owner_core, &program);
        }
        if (source != nullptr) {
            (void)inkpod_inkscript_source_release(&source);
        }
    }

    void Cancel() noexcept {
        cancel_signal.store(true, std::memory_order_release);
        std::lock_guard lock(cancel_mutex);
        if (cancellable_plan != nullptr) {
            (void)inkpod_inkscript_plan_task_cancel(cancellable_plan);
        }
        if (cancellable_run != nullptr) {
            (void)inkpod_inkscript_run_task_cancel(cancellable_run);
        }
    }

    bool QueryProgress(InkpodTaskInfo& output) noexcept {
        std::lock_guard lock(cancel_mutex);
        output.struct_size = sizeof(output);
        if (cancellable_run != nullptr) {
            return inkpod_inkscript_run_task_query(cancellable_run, &output) == INKPOD_STATUS_OK;
        }
        if (cancellable_plan != nullptr) {
            const bool result_ok = inkpod_inkscript_plan_task_query(cancellable_plan, &output) == INKPOD_STATUS_OK;
            output.total_work = 0U;
            return result_ok;
        }
        return false;
    }

    void PublishCancellation() noexcept {
        std::lock_guard lock(cancel_mutex);
        cancellable_plan = plan_task;
        cancellable_run = run_task;
        if (cancel_signal.load(std::memory_order_acquire)) {
            if (cancellable_plan != nullptr) {
                (void)inkpod_inkscript_plan_task_cancel(cancellable_plan);
            }
            if (cancellable_run != nullptr) {
                (void)inkpod_inkscript_run_task_cancel(cancellable_run);
            }
        }
    }

    InkpodStatus ReleasePlanTask(InkpodCore* core) noexcept {
        {
            std::lock_guard lock(cancel_mutex);
            cancellable_plan = nullptr;
        }
        return inkpod_core_inkscript_plan_task_release(core, &plan_task);
    }

    InkpodStatus ReleaseRunTask(InkpodCore* core) noexcept {
        {
            std::lock_guard lock(cancel_mutex);
            cancellable_run = nullptr;
        }
        return inkpod_core_inkscript_run_task_release(core, &run_task);
    }

    InkScriptEngineStep Advance(
        InkpodCore* core,
        bool cancel_requested,
        std::uint32_t confirmation_scope, InkpodIoManager* manager,
        std::span<const InkpodInkScriptIoSession> sessions) noexcept {
        if (state == RouteState::Completed) {
            return CompletedStep(result);
        }
        if (core == nullptr) {
            return Finish(nullptr, INKPOD_STATUS_INVALID_ARGUMENT);
        }
        try {
            if (cancel_requested) {
                Cancel();
            }
            cancel = cancel_signal.load(std::memory_order_acquire);
            if (!cancel && state != RouteState::Initialize && host != nullptr) {
                for (const auto& captured : captured_sessions) {
                    const auto found = std::find_if(sessions.begin(), sessions.end(),
                        [&captured](const auto& session) {
                            return session.session_id == captured.first
                                && session.session_generation == captured.second;
                        });
                    const InkpodStatus valid = found == sessions.end()
                        ? INKPOD_STATUS_INVALID_STATE
                        : inkpod_core_inkscript_io_validate_session(core, host->Handle(),
                            found->session_id, found->session_core);
                    if (valid != INKPOD_STATUS_OK) {
                        return Finish(core, valid);
                    }
                }
            }
            switch (state) {
                case RouteState::Initialize:
                    return Initialize(core, manager, sessions);
                case RouteState::Planning:
                    return AdvancePlan(core);
                case RouteState::AwaitingConfirmation:
                    if (cancel) {
                        return Finish(core, INKPOD_STATUS_CANCELLED);
                    }
                    if (confirmation_scope == 0U) {
                        return InkScriptEngineStep{
                            InkScriptEngineStepKind::PlanReady, 0U, false, {}};
                    }
                    return BeginRun(core, confirmation_scope);
                case RouteState::Running:
                    return AdvanceRun(core);
                case RouteState::Completed:
                    return CompletedStep(result);
            }
        } catch (const std::bad_alloc&) {
            return Finish(core, INKPOD_STATUS_INVALID_STATE);
        } catch (...) {
            return Finish(core, INKPOD_STATUS_INVALID_STATE);
        }
        return Finish(core, INKPOD_STATUS_INVALID_STATE);
    }

    InkScriptEngineStep Initialize(InkpodCore* core, InkpodIoManager* manager,
        std::span<const InkpodInkScriptIoSession> sessions) {
        if (cancel) {
            return Finish(core, INKPOD_STATUS_CANCELLED);
        }
        if (request.job_id == 0U || request.controller_id == 0U
            || request.source_id == 0U || request.source_generation == 0U
            || !request.context.document_session.has_value()
            || !request.context.generation.has_value()
            || request.source_utf8.empty()
            || (request.run_mode != INKPOD_INKSCRIPT_RUN_DRY
                && request.run_mode != INKPOD_INKSCRIPT_RUN_INSTALL
                && request.run_mode != INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW)) {
            return Finish(core, INKPOD_STATUS_INVALID_ARGUMENT);
        }
        owner_core = core;

        InkpodDocumentInfo document{};
        document.struct_size = sizeof(document);
        InkpodStatus status = inkpod_core_get_document_info(core, &document);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        InkpodEditorStateInfo editor{};
        editor.struct_size = sizeof(editor);
        status = inkpod_core_get_editor_state(core, &editor);
        if (status != INKPOD_STATUS_OK
            || document.document_uuid_high != request.expected_document.document_uuid_high
            || document.document_uuid_low != request.expected_document.document_uuid_low
            || document.document_revision != request.expected_document.document_revision
            || editor.editor_revision != request.expected_editor.editor_revision) {
            return Finish(core, status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status);
        }
        document_uuid_low = document.document_uuid_low;
        document_uuid_high = document.document_uuid_high;

        result.phase = InkScriptEnginePhase::ExportFragment;
        status = ExportFragment(core);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }

        result.phase = InkScriptEnginePhase::Parse;
        InkpodInkScriptSourceInput input{};
        input.struct_size = sizeof(input);
        input.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        input.controller_id = request.controller_id;
        input.session_generation = request.context.generation->Value();
        input.source_id = request.source_id;
        input.source_utf8 = reinterpret_cast<const std::uint8_t*>(
            request.source_utf8.data());
        input.source_bytes = static_cast<std::uint64_t>(request.source_utf8.size());
        status = inkpod_inkscript_source_parse(&input, &source);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        InkpodInkScriptSourceSummary source_summary{};
        source_summary.struct_size = sizeof(source_summary);
        status = inkpod_inkscript_source_summary(source, &source_summary);
        if (status != INKPOD_STATUS_OK
            || (source_summary.flags & INKPOD_INKSCRIPT_SOURCE_VALID) == 0U
            || source_summary.diagnostic_count != 0U) {
            return Finish(
                core,
                status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_ARGUMENT
                    : status);
        }

        result.phase = InkScriptEnginePhase::Compile;
        InkpodInkScriptCompileRequest compile{};
        compile.struct_size = sizeof(compile);
        compile.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        compile.controller_id = request.controller_id;
        compile.session_generation = request.context.generation->Value();
        status = inkpod_core_inkscript_compile(core, source, &compile, &program);
        if (status != INKPOD_STATUS_OK) {
            CaptureDiagnostic();
        }
        const InkpodStatus source_release = inkpod_inkscript_source_release(&source);
        if (status == INKPOD_STATUS_OK) {
            status = source_release;
        }
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }

        InkpodInkScriptProgramSummary program_summary{};
        program_summary.struct_size = sizeof(program_summary);
        status = inkpod_core_inkscript_program_summary(
            core, program, &program_summary);
        if (status != INKPOD_STATUS_OK
            || program_summary.path_intent_count
                != static_cast<std::uint64_t>(request.authorized_paths.size())) {
            return Finish(
                core,
                status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_ARGUMENT
                    : status);
        }
        result.phase = InkScriptEnginePhase::Authorize;
        host = std::make_unique<InkScriptFileAuthorityAdapter>();
        status = host->Initialize(core, manager, program, request.authorized_paths,
            request.publication_targets.size());
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        InkpodInkScriptIoSession session{};
        session.struct_size = sizeof(session);
        session.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        session.feature_flags = INKPOD_INKSCRIPT_SESSION_USE_CORE_BACKING;
        session.session_id = request.context.document_session->Value();
        session.session_generation = request.context.generation->Value();
        session.source_generation = request.source_generation;
        session.session_core = core;
        session.label = {reinterpret_cast<const std::uint8_t*>(request.current_document_label_utf8.data()),
            request.current_document_label_utf8.size()};
        session.display_number = 1U;
        status = inkpod_core_inkscript_io_capture_session(core, host->Handle(), &session);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        captured_sessions.emplace_back(session.session_id, session.session_generation);
        for (const auto& other : sessions) {
            if (other.session_id == session.session_id) {
                continue;
            }
            status = inkpod_core_inkscript_io_capture_session(core, host->Handle(), &other);
            if (status != INKPOD_STATUS_OK) {
                return Finish(core, status);
            }
            captured_sessions.emplace_back(other.session_id, other.session_generation);
        }
        result.phase = InkScriptEnginePhase::PlanTaskCreate;
        InkpodInkScriptSharedPlanRequest planning{};
        planning.struct_size = sizeof(planning);
        planning.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        planning.controller_id = request.controller_id;
        planning.session_generation = request.context.generation->Value();
        planning.current_session_id = session.session_id;
        status = inkpod_core_inkscript_shared_plan_task_create(
            core, program, host->Handle(), &planning, &plan_task);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        PublishCancellation();
        state = RouteState::Planning;
        result.phase = InkScriptEnginePhase::Planning;
        return ContinueStep(result);
    }

    InkpodStatus ExportFragment(InkpodCore* core) {
        if (request.export_event_ids.empty()) {
            return INKPOD_STATUS_OK;
        }
        std::vector<InkpodInkScriptJournalEvent> events;
        events.reserve(request.export_event_ids.size());
        for (const std::uint64_t event_id : request.export_event_ids) {
            events.push_back(InkpodInkScriptJournalEvent{
                sizeof(InkpodInkScriptJournalEvent),
                INKPOD_INKSCRIPT_RECORD_VERSION,
                event_id,
                0U});
        }
        InkpodInkScriptExportRequest export_request{};
        export_request.struct_size = sizeof(export_request);
        export_request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        export_request.controller_id = request.controller_id;
        export_request.session_generation = request.context.generation->Value();
        export_request.events = events.data();
        export_request.event_count = static_cast<std::uint64_t>(events.size());
        export_request.event_stride_bytes = sizeof(InkpodInkScriptJournalEvent);
        FragmentOwner fragment(core);
        InkpodStatus status = inkpod_core_inkscript_fragment_export(
            core, &export_request, &fragment.value);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        InkpodInkScriptFragmentSummary summary{};
        summary.struct_size = sizeof(summary);
        status = inkpod_core_inkscript_fragment_summary(core, fragment.value, &summary);
        if (status == INKPOD_STATUS_OK) {
            InkpodInkScriptUtf8Buffer query{};
            query.struct_size = sizeof(query);
            query.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            InkpodStatus query_status =
                inkpod_core_inkscript_fragment_text_copy(core, fragment.value, &query);
            if (query_status != INKPOD_STATUS_BUFFER_TOO_SMALL
                && !(query_status == INKPOD_STATUS_OK
                    && query.required_bytes == 0U)) {
                status = query_status;
            } else if (query.required_bytes != summary.text_bytes) {
                status = INKPOD_STATUS_INVALID_STATE;
            } else if (query.required_bytes != 0U) {
                std::vector<std::uint8_t> text(
                    static_cast<std::size_t>(query.required_bytes));
                query.bytes = text.data();
                query.capacity_bytes = static_cast<std::uint64_t>(text.size());
                query_status = inkpod_core_inkscript_fragment_text_copy(
                    core, fragment.value, &query);
                if (query_status != INKPOD_STATUS_OK
                    || query.written_bytes != query.required_bytes) {
                    status = query_status == INKPOD_STATUS_OK
                        ? INKPOD_STATUS_INVALID_STATE
                        : query_status;
                }
            }
        }
        if (status == INKPOD_STATUS_OK) {
            result.exported_commit_count = summary.commit_count;
            result.exported_text_bytes = summary.text_bytes;
        }
        const InkpodStatus release = fragment.Release();
        return status == INKPOD_STATUS_OK ? release : status;
    }

    InkScriptEngineStep AdvancePlan(InkpodCore* core) {
        if (cancel) {
            (void)inkpod_inkscript_plan_task_cancel(plan_task);
        }
        InkpodStatus status = inkpod_core_inkscript_plan_task_advance(
            core, plan_task);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        InkpodInkScriptTaskEvent event{};
        event.struct_size = sizeof(event);
        event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        status = inkpod_core_inkscript_plan_task_event_take(
            core, plan_task, &event);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        InkpodTaskInfo info{};
        info.struct_size = sizeof(info);
        status = inkpod_inkscript_plan_task_query(plan_task, &info);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        if (cancel || info.state == INKPOD_TASK_CANCELLED) {
            return Finish(core, INKPOD_STATUS_CANCELLED);
        }
        if (info.state != INKPOD_TASK_COMPLETED
            || event.kind != INKPOD_INKSCRIPT_EVENT_PLAN_COMPLETE) {
            return Finish(core, INKPOD_STATUS_INVALID_STATE);
        }
        status = inkpod_core_inkscript_plan_task_take_plan(
            core, plan_task, &plan);
        const InkpodStatus release =
            ReleasePlanTask(core);
        if (status == INKPOD_STATUS_OK) {
            status = release;
        }
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        InkpodInkScriptPlanSummary summary{};
        summary.struct_size = sizeof(summary);
        status = inkpod_core_inkscript_plan_summary(core, plan, &summary);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        result.status = INKPOD_STATUS_OK;
        result.event_kind = event.kind;
        result.task_state = event.task_state;
        result.completed_items = event.completed_items;
        result.total_items = summary.item_count;
        state = RouteState::AwaitingConfirmation;
        result.phase = InkScriptEnginePhase::AwaitingConfirmation;
        InkScriptEngineResult notification = result;
        notification.kind = InkScriptEngineNotificationKind::PlanReady;
        return InkScriptEngineStep{
            InkScriptEngineStepKind::PlanReady,
            0U,
            true,
            notification};
    }

    InkScriptEngineStep BeginRun(
        InkpodCore* core,
        std::uint32_t confirmation_scope) {
        if (confirmation_scope != INKPOD_INKSCRIPT_SCOPE_ALL
            && confirmation_scope != INKPOD_INKSCRIPT_SCOPE_CURRENT_DOCUMENT
            && confirmation_scope != INKPOD_INKSCRIPT_SCOPE_CURRENT_FILE) {
            return Finish(core, INKPOD_STATUS_INVALID_ARGUMENT);
        }
        InkpodInkScriptConfirmationRequest confirmation_request{};
        confirmation_request.struct_size = sizeof(confirmation_request);
        confirmation_request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        confirmation_request.scope = confirmation_scope;
        confirmation_request.document_uuid_low = document_uuid_low;
        confirmation_request.document_uuid_high = document_uuid_high;
        InkpodStatus status = inkpod_core_inkscript_confirmation_create(
            core, plan, &confirmation_request, &confirmation);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        InkpodInkScriptRunRequest run{};
        run.struct_size = sizeof(run);
        run.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        run.mode = request.run_mode;
        run.controller_id = request.controller_id;
        run.session_generation = request.context.generation->Value();
        run.maximum_output_bytes = request.maximum_output_bytes;

        status = inkpod_core_inkscript_run_task_create(
            core,
            program,
            &plan,
            &confirmation,
            &run,
            &run_task);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        PublishCancellation();
        state = RouteState::Running;
        result.phase = InkScriptEnginePhase::Running;
        return ContinueStep(result);
    }

    InkScriptEngineStep AdvanceRun(InkpodCore* core) {
        if (cancel) {
            (void)inkpod_inkscript_run_task_cancel(run_task);
        }
        InkpodStatus status = inkpod_core_inkscript_run_task_advance(
            core, run_task);
        // Normal run cancellation is a terminal result with a queued report
        // and any earlier successful staged items. Preview failure has no
        // report and must continue through the failure cleanup path.
        if (status != INKPOD_STATUS_OK
            && (status != INKPOD_STATUS_CANCELLED
                || request.run_mode == INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW)) {
            return Finish(core, status);
        }
        InkpodInkScriptTaskEvent event{};
        event.struct_size = sizeof(event);
        event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        status = inkpod_core_inkscript_run_task_event_take(
            core, run_task, &event);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        if (event.kind != INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE) {
            return ContinueStep(result, &event);
        }

        status = CaptureStaged(core);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }

        ReportOwner report;
        status = inkpod_core_inkscript_run_task_take_report(
            core, run_task, &report.value);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        status = CopyReport(report.value);
        const InkpodStatus report_release = report.Release();
        if (status == INKPOD_STATUS_OK) {
            status = report_release;
        }
        // Keep the terminal task until staged publication has completed. Its
        // shared atomic cancellation token is still observed by active apply
        // and preview publication, including during final fingerprint work.

        state = RouteState::Completed;
        result.phase = InkScriptEnginePhase::Completed;
        result.event_kind = event.kind;
        result.task_state = event.task_state;
        result.completed_items = event.completed_items;
        result.total_items = event.total_items;
        if (status != INKPOD_STATUS_OK) {
            result.status = status;
        } else if (event.task_state == INKPOD_TASK_CANCELLED
            || result.outcome == INKPOD_INKSCRIPT_OUTCOME_CANCELLED
            || cancel) {
            result.status = INKPOD_STATUS_CANCELLED;
        } else if (result.outcome == INKPOD_INKSCRIPT_OUTCOME_FAILED) {
            result.status = INKPOD_STATUS_IO_ERROR;
        } else {
            result.status = INKPOD_STATUS_OK;
        }
        return CompletedStep(result);
    }

    InkpodStatus CopyReport(const InkpodInkScriptReport* report) {
        InkpodInkScriptReportSummary summary{};
        summary.struct_size = sizeof(summary);
        InkpodStatus status = inkpod_inkscript_report_summary(report, &summary);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        result.report_flags = summary.flags;
        result.report_item_count = summary.item_count;
        result.created_directory_count = summary.created_directory_count;
        if (summary.item_count == 0U) {
            return INKPOD_STATUS_OK;
        }
        InkpodInkScriptReportBuffer query{};
        query.struct_size = sizeof(query);
        query.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        status = inkpod_inkscript_report_items_copy(report, &query);
        if (status != INKPOD_STATUS_BUFFER_TOO_SMALL
            || query.required_records != summary.item_count
            || query.required_records > UINT64_C(65536)
            || query.required_utf8_bytes > UINT64_C(16) * 1024U * 1024U) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
        const InkpodInkScriptReportItem empty_record{
            sizeof(InkpodInkScriptReportItem),
            INKPOD_INKSCRIPT_RECORD_VERSION};
        std::vector<InkpodInkScriptReportItem> records(
            static_cast<std::size_t>(query.required_records), empty_record);
        std::vector<std::uint8_t> utf8(
            static_cast<std::size_t>(query.required_utf8_bytes));
        query.records = records.data();
        query.record_capacity = records.size();
        query.record_stride_bytes = sizeof(InkpodInkScriptReportItem);
        query.utf8 = utf8.empty() ? nullptr : utf8.data();
        query.utf8_capacity_bytes = utf8.size();
        status = inkpod_inkscript_report_items_copy(report, &query);
        if (status != INKPOD_STATUS_OK || query.records_written != records.size()) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
        const auto last_processed = std::find_if(records.rbegin(), records.rend(), [](const auto& item) {
            return item.outcome != INKPOD_INKSCRIPT_OUTCOME_NOT_STARTED;
        });
        const InkpodInkScriptReportItem& final = last_processed == records.rend()
            ? records.back() : *last_processed;
        result.outcome = final.outcome;
        result.failure = final.failure;
        result.final_revision = final.final_revision;
        result.next_stable_id = final.next_stable_id;
        std::copy(
            std::begin(final.final_state_digest),
            std::end(final.final_state_digest),
            result.final_state_digest.begin());
        return INKPOD_STATUS_OK;
    }

    InkpodStatus CaptureStaged(InkpodCore* core) {
        std::uint64_t count{};
        InkpodStatus status = inkpod_core_inkscript_run_task_result_count(core, run_task, &count);
        if (status != INKPOD_STATUS_OK || count > 64U) {
            return status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status;
        }
        staged.resize(static_cast<std::size_t>(count));
        staged_info.resize(static_cast<std::size_t>(count));
        for (std::size_t index = 0U; index < staged.size(); ++index) {
            auto& info = staged_info[index];
            info.struct_size = sizeof(info);
            info.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            status = inkpod_core_inkscript_run_task_take_staged_result(
                core, run_task, index, &staged[index], &info);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            if (info.kind == INKPOD_INKSCRIPT_STAGED_ACTIVE_DOCUMENT
                || info.kind == INKPOD_INKSCRIPT_STAGED_IMAGE_PREVIEW) {
                if (info.session_id != request.context.document_session->Value()
                    || info.session_generation != request.context.generation->Value()
                    || info.source_generation != request.source_generation
                    || staged.size() != 1U) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                result.active_output = info.kind == INKPOD_INKSCRIPT_STAGED_ACTIVE_DOCUMENT;
                result.image_preview = info.kind == INKPOD_INKSCRIPT_STAGED_IMAGE_PREVIEW;
            }
        }
        result.staged_result_count = count;
        return INKPOD_STATUS_OK;
    }

    InkpodStatus TakePublication(InkpodCore* core, std::vector<InkpodCore*>& output) noexcept {
        if (core != owner_core || !output.empty() || state != RouteState::Completed) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        if (staged.empty()) {
            return INKPOD_STATUS_OK;
        }
        if (result.active_output) {
            return inkpod_core_inkscript_staged_result_apply_active(
                core, &staged.front(), core,
                request.context.document_session->Value(),
                request.context.generation->Value(), request.source_generation,
                cancel_signal.load(std::memory_order_acquire) ? 1U : 0U);
        }
        if (staged.size() > request.publication_targets.size()) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        try {
            output.resize(staged.size());
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        InkpodStatus status = INKPOD_STATUS_OK;
        for (std::size_t index = 0U; index < staged.size(); ++index) {
            status = inkpod_core_inkscript_staged_result_take_core(core, &staged[index], &output[index]);
            if (status != INKPOD_STATUS_OK) {
                break;
            }
        }
        if (status != INKPOD_STATUS_OK) {
            for (auto*& value : output) {
                (void)inkpod_core_destroy(&value);
            }
            output.clear();
        }
        return status;
    }

    InkScriptEngineStep Finish(
        InkpodCore* core,
        InkpodStatus status) noexcept {

        if (status != INKPOD_STATUS_OK && result.diagnostic_bytes == 0U) {
            CaptureDiagnostic();
        }
        if (plan_task != nullptr) {
            (void)inkpod_inkscript_plan_task_cancel(plan_task);
            if (core != nullptr) {
                (void)inkpod_core_inkscript_plan_task_advance(core, plan_task);
                InkpodInkScriptTaskEvent event{};
                event.struct_size = sizeof(event);
                event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
                (void)inkpod_core_inkscript_plan_task_event_take(
                    core, plan_task, &event);
                (void)ReleasePlanTask(core);
            }
        }
        if (run_task != nullptr) {
            (void)inkpod_inkscript_run_task_cancel(run_task);
            if (core != nullptr) {
                for (std::uint32_t attempt = 0U;
                     attempt < 4U && run_task != nullptr;
                     ++attempt) {
                    if (inkpod_core_inkscript_run_task_advance(core, run_task)
                        != INKPOD_STATUS_OK) {
                        break;
                    }
                    InkpodInkScriptTaskEvent event{};
                    event.struct_size = sizeof(event);
                    event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
                    if (inkpod_core_inkscript_run_task_event_take(
                            core, run_task, &event) != INKPOD_STATUS_OK) {
                        break;
                    }
                    if (event.kind == INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE) {
                        ReportOwner ignored;
                        (void)inkpod_core_inkscript_run_task_take_report(
                            core, run_task, &ignored.value);
                        break;
                    }
                }
                (void)ReleaseRunTask(core);
            }
        }
        if (core != nullptr) {
            (void)inkpod_core_inkscript_confirmation_release(core, &confirmation);
            (void)inkpod_core_inkscript_plan_release(core, &plan);
            (void)inkpod_core_inkscript_program_release(core, &program);
        }
        (void)inkpod_inkscript_source_release(&source);
        host.reset();

        state = RouteState::Completed;
        result.status = status;
        return CompletedStep(result);
    }

    void CaptureDiagnostic() noexcept {
        std::uint64_t required{};
        if (inkpod_error_message_size(&required) != INKPOD_STATUS_OK
            || required == 0U || required > result.diagnostic_utf8.size()) {
            return;
        }
        std::uint64_t written{};
        if (inkpod_error_message_copy(
                result.diagnostic_utf8.data(),
                result.diagnostic_utf8.size(),
                &written) == INKPOD_STATUS_OK
            && written <= result.diagnostic_utf8.size()) {
            result.diagnostic_bytes = written;
        }
    }

    InkScriptEngineRequest request;
    InkScriptEngineResult result{};
    RouteState state{RouteState::Initialize};
    bool cancel{};
    std::atomic<bool> cancel_signal{};
    std::mutex cancel_mutex;
    InkpodInkScriptPlanTask* cancellable_plan{};
    InkpodInkScriptRunTask* cancellable_run{};
    std::uint64_t document_uuid_low{};
    std::uint64_t document_uuid_high{};
    InkpodCore* owner_core{};
    std::unique_ptr<InkScriptFileAuthorityAdapter> host;
    std::vector<std::pair<std::uint64_t, std::uint64_t>> captured_sessions;
    std::vector<InkpodInkScriptStagedResult*> staged;
    std::vector<InkpodInkScriptStagedInfo> staged_info;
    InkpodInkScriptSource* source{};
    InkpodInkScriptProgram* program{};
    InkpodInkScriptPlanTask* plan_task{};
    InkpodInkScriptPlan* plan{};
    InkpodInkScriptConfirmation* confirmation{};
    InkpodInkScriptRunTask* run_task{};
};

InkScriptEngineTask::InkScriptEngineTask(InkScriptEngineRequest request)
    : impl_(std::make_unique<Impl>(std::move(request))) {}

InkScriptEngineTask::~InkScriptEngineTask() = default;

InkScriptEngineStep InkScriptEngineTask::Advance(
    InkpodCore* core,
    bool cancel_requested,
    std::uint32_t confirmation_scope, InkpodIoManager* manager,
    std::span<const InkpodInkScriptIoSession> sessions) noexcept {
    return impl_ == nullptr
        ? CompletedStep(InkScriptEngineResult{})
        : impl_->Advance(core, cancel_requested, confirmation_scope, manager, sessions);
}

const std::vector<CommandContext>& InkScriptEngineTask::PublicationTargets() const noexcept {
    return impl_->request.publication_targets;
}

void InkScriptEngineTask::Cancel() noexcept {
    if (impl_ != nullptr) {
        impl_->Cancel();
    }
}

bool InkScriptEngineTask::QueryProgress(InkpodTaskInfo& output) noexcept {
    return impl_ != nullptr && impl_->QueryProgress(output);
}

void InkScriptEngineTask::InvalidateOpenSessions() noexcept {
    if (impl_ != nullptr && impl_->host != nullptr) {
        (void)inkpod_core_inkscript_io_invalidate(impl_->owner_core, impl_->host->Handle(), 0U);
    }
}

InkpodStatus InkScriptEngineTask::TakePublication(
    InkpodCore* owner, std::vector<InkpodCore*>& output) noexcept {
    return impl_ == nullptr ? INKPOD_STATUS_INVALID_STATE : impl_->TakePublication(owner, output);
}

}  // namespace inkpod::app
