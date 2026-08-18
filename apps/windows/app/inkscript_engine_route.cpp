#include "inkscript_engine_route.h"

#include <windows.h>

#include <algorithm>
#include <cstring>
#include <limits>
#include <new>
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

class RouteHost final {
public:
    RouteHost(
        InkpodCore* core,
        const InkScriptEngineRequest& request)
        : filesystem_(), delegate_(filesystem_.HostAdapterRecord()) {
        label_ = request.current_document_label_utf8;
        session_.struct_size = sizeof(session_);
        session_.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        session_.core = core;
        session_.session_id = request.context.document_session.has_value()
            ? request.context.document_session->Value()
            : 0U;
        session_.session_generation = request.context.generation.has_value()
            ? request.context.generation->Value()
            : 0U;
        session_.source_generation = request.source_generation;
        session_.display_label = InkpodInkScriptUtf8Span{
            reinterpret_cast<const std::uint8_t*>(label_.data()),
            static_cast<std::uint64_t>(label_.size())};
        session_.display_number = 1U;
    }

    [[nodiscard]] InkpodInkScriptHostAdapter Record() noexcept {
        return InkpodInkScriptHostAdapter{
            sizeof(InkpodInkScriptHostAdapter),
            INKPOD_INKSCRIPT_RECORD_VERSION,
            INKPOD_FEATURE_NONE,
            this,
            &Call};
    }

    [[nodiscard]] InkScriptFileAuthorityAdapter& Filesystem() noexcept {
        return filesystem_;
    }

    [[nodiscard]] std::uint32_t LastOperation() const noexcept {
        return last_operation_;
    }

    [[nodiscard]] InkpodStatus LastStatus() const noexcept {
        return last_status_;
    }

private:
    static InkpodStatus Call(
        void* context,
        const InkpodInkScriptHostRequest* request,
        InkpodInkScriptHostResponse* response) noexcept {
        if (context == nullptr || request == nullptr || response == nullptr) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        auto& self = *static_cast<RouteHost*>(context);
        self.last_operation_ = request->operation;
        if (request->struct_size < sizeof(InkpodInkScriptHostRequest)
            || request->version != INKPOD_INKSCRIPT_RECORD_VERSION
            || request->feature_flags != INKPOD_FEATURE_NONE
            || response->struct_size < sizeof(InkpodInkScriptHostResponse)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        *response = {};
        response->struct_size = sizeof(*response);
        response->version = INKPOD_INKSCRIPT_RECORD_VERSION;
        switch (request->operation) {
            case INKPOD_INKSCRIPT_HOST_CURRENT_DOCUMENT:
                response->flags = INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT;
                response->session = &self.session_;
                return INKPOD_STATUS_OK;
            case INKPOD_INKSCRIPT_HOST_CURRENT_SEQUENCE:
                return INKPOD_STATUS_NO_DOCUMENT;
            case INKPOD_INKSCRIPT_HOST_CAPTURE_OPEN_SESSION:
                if (self.Matches(*request)) {
                    response->flags = INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT;
                    response->session = &self.session_;
                    return INKPOD_STATUS_OK;
                }
                return INKPOD_STATUS_NO_DOCUMENT;
            case INKPOD_INKSCRIPT_HOST_SESSION_IS_CURRENT:
                response->flags = self.Matches(*request)
                    ? INKPOD_INKSCRIPT_HOST_RESPONSE_PRESENT
                    : 0U;
                return INKPOD_STATUS_OK;
            default: {
                const InkpodStatus status = self.delegate_.call(
                    self.delegate_.context, request, response);
                self.last_status_ = status;
                return status;
            }
        }
    }

    [[nodiscard]] bool Matches(
        const InkpodInkScriptHostRequest& request) const noexcept {
        return request.session_id == session_.session_id
            && request.session_generation == session_.session_generation
            && request.source_generation == session_.source_generation;
    }

    InkScriptFileAuthorityAdapter filesystem_;
    InkpodInkScriptHostAdapter delegate_{};
    std::string label_;
    InkpodInkScriptSessionInput session_{};
    std::uint32_t last_operation_{};
    InkpodStatus last_status_{INKPOD_STATUS_OK};
};

}  // namespace

struct InkScriptEngineTask::Impl final {
    explicit Impl(InkScriptEngineRequest input)
        : request(std::move(input)) {
        result.job_id = request.job_id;
        result.owner_thread_id = GetCurrentThreadId();
    }

    ~Impl() = default;

    InkScriptEngineStep Advance(
        InkpodCore* core,
        bool cancel_requested,
        std::uint32_t confirmation_scope) noexcept {
        if (state == RouteState::Completed) {
            return CompletedStep(result);
        }
        if (core == nullptr) {
            return Finish(nullptr, INKPOD_STATUS_INVALID_ARGUMENT);
        }
        try {
            if (cancel_requested) {
                cancel = true;
            }
            switch (state) {
                case RouteState::Initialize:
                    return Initialize(core);
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

    InkScriptEngineStep Initialize(InkpodCore* core) {
        if (cancel) {
            return Finish(core, INKPOD_STATUS_CANCELLED);
        }
        if (request.job_id == 0U || request.controller_id == 0U
            || request.source_id == 0U || request.source_generation == 0U
            || !request.context.document_session.has_value()
            || !request.context.generation.has_value()
            || request.source_utf8.empty()
            || (request.run_mode != INKPOD_INKSCRIPT_RUN_DRY
                && request.run_mode != INKPOD_INKSCRIPT_RUN_INSTALL)) {
            return Finish(core, INKPOD_STATUS_INVALID_ARGUMENT);
        }
        host = std::make_unique<RouteHost>(core, request);

        InkpodDocumentInfo document{};
        document.struct_size = sizeof(document);
        InkpodStatus status = inkpod_core_get_document_info(core, &document);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
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
        status = AuthorizePaths(core, program_summary.path_intent_count);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }

        std::uint64_t authority_generation{};
        std::uint64_t open_session_generation{};
        status = HostGeneration(
            INKPOD_INKSCRIPT_HOST_AUTHORITY_GENERATION,
            authority_generation);
        if (status == INKPOD_STATUS_OK) {
            status = HostGeneration(
                INKPOD_INKSCRIPT_HOST_OPEN_SESSION_GENERATION,
                open_session_generation);
        }
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        for (auto& grant : grants) {
            grant.authority_generation = authority_generation;
        }

        result.phase = InkScriptEnginePhase::PlanTaskCreate;
        InkpodInkScriptPlanTaskRequest planning{};
        planning.struct_size = sizeof(planning);
        planning.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        planning.controller_id = request.controller_id;
        planning.session_generation = request.context.generation->Value();
        planning.authority_generation = authority_generation;
        planning.open_session_set_generation = open_session_generation;
        planning.grants = grants.empty() ? nullptr : grants.data();
        planning.grant_count = static_cast<std::uint64_t>(grants.size());
        planning.grant_stride_bytes = sizeof(InkpodInkScriptAuthorityGrant);
        planning.host = host->Record();
        status = inkpod_core_inkscript_plan_task_create(
            core, program, &planning, &plan_task);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
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
        InkpodInkScriptFragment* fragment{};
        InkpodStatus status = inkpod_core_inkscript_fragment_export(
            core, &export_request, &fragment);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
        InkpodInkScriptFragmentSummary summary{};
        summary.struct_size = sizeof(summary);
        status = inkpod_core_inkscript_fragment_summary(core, fragment, &summary);
        if (status == INKPOD_STATUS_OK) {
            InkpodInkScriptUtf8Buffer query{};
            query.struct_size = sizeof(query);
            query.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            InkpodStatus query_status =
                inkpod_core_inkscript_fragment_text_copy(core, fragment, &query);
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
                    core, fragment, &query);
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
        const InkpodStatus release =
            inkpod_core_inkscript_fragment_release(core, &fragment);
        return status == INKPOD_STATUS_OK ? release : status;
    }

    InkpodStatus AuthorizePaths(
        InkpodCore* core,
        std::uint64_t path_intent_count) {
        if (path_intent_count == 0U) {
            return INKPOD_STATUS_OK;
        }
        if (path_intent_count > static_cast<std::uint64_t>(
                std::numeric_limits<std::size_t>::max())) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        InkpodInkScriptPathIntentBuffer query{};
        query.struct_size = sizeof(query);
        query.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        InkpodStatus status = inkpod_core_inkscript_program_path_intents_copy(
            core, program, &query);
        if (status != INKPOD_STATUS_BUFFER_TOO_SMALL
            || query.required_records != path_intent_count
            || query.required_utf8_bytes > UINT64_C(16) * 1024U * 1024U) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
        const InkpodInkScriptPathIntent empty_intent{
            sizeof(InkpodInkScriptPathIntent),
            INKPOD_INKSCRIPT_RECORD_VERSION};
        std::vector<InkpodInkScriptPathIntent> intents(
            static_cast<std::size_t>(query.required_records), empty_intent);
        std::vector<std::uint8_t> utf8(
            static_cast<std::size_t>(query.required_utf8_bytes));
        query.records = intents.data();
        query.record_capacity = intents.size();
        query.record_stride_bytes = sizeof(InkpodInkScriptPathIntent);
        query.utf8 = utf8.empty() ? nullptr : utf8.data();
        query.utf8_capacity_bytes = utf8.size();
        status = inkpod_core_inkscript_program_path_intents_copy(
            core, program, &query);
        if (status != INKPOD_STATUS_OK
            || query.records_written != path_intent_count) {
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
        grants.reserve(intents.size());
        for (std::size_t index = 0U; index < intents.size(); ++index) {
            InkpodInkScriptAuthorityGrant grant{};
            status = host->Filesystem().AuthorizePath(
                intents[index].intent_id,
                intents[index].access,
                request.authorized_paths[index],
                grant);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            grants.push_back(grant);
        }
        return INKPOD_STATUS_OK;
    }

    InkpodStatus HostGeneration(
        std::uint32_t operation,
        std::uint64_t& output) noexcept {
        InkpodInkScriptHostRequest host_request{};
        host_request.struct_size = sizeof(host_request);
        host_request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        host_request.operation = operation;
        InkpodInkScriptHostResponse response{};
        response.struct_size = sizeof(response);
        InkpodInkScriptHostAdapter adapter = host->Record();
        const InkpodStatus status = adapter.call(
            adapter.context, &host_request, &response);
        if (status == INKPOD_STATUS_OK) {
            output = response.generation;
        }
        return status;
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
            inkpod_core_inkscript_plan_task_release(core, &plan_task);
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
        run.host = host->Record();
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
        if (status != INKPOD_STATUS_OK) {
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

        InkpodInkScriptReport* report{};
        status = inkpod_core_inkscript_run_task_take_report(
            core, run_task, &report);
        if (status != INKPOD_STATUS_OK) {
            return Finish(core, status);
        }
        status = CopyReport(report);
        const InkpodStatus report_release =
            inkpod_inkscript_report_release(&report);
        if (status == INKPOD_STATUS_OK) {
            status = report_release;
        }
        const InkpodStatus task_release =
            inkpod_core_inkscript_run_task_release(core, &run_task);
        if (status == INKPOD_STATUS_OK) {
            status = task_release;
        }
        const InkpodStatus program_release =
            inkpod_core_inkscript_program_release(core, &program);
        if (status == INKPOD_STATUS_OK) {
            status = program_release;
        }
        result.last_host_operation = host == nullptr ? 0U : host->LastOperation();
        result.last_host_status = host == nullptr
            ? INKPOD_STATUS_OK
            : host->LastStatus();
        host.reset();
        grants.clear();
        state = RouteState::Completed;
        result.phase = InkScriptEnginePhase::Completed;
        result.event_kind = event.kind;
        result.task_state = event.task_state;
        result.completed_items = event.completed_items;
        result.total_items = event.total_items;
        if (status != INKPOD_STATUS_OK) {
            result.status = status;
        } else if (result.outcome == INKPOD_INKSCRIPT_OUTCOME_CANCELLED
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
        const InkpodInkScriptReportItem& final = records.back();
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

    InkScriptEngineStep Finish(
        InkpodCore* core,
        InkpodStatus status) noexcept {
        if (host != nullptr) {
            result.last_host_operation = host->LastOperation();
            result.last_host_status = host->LastStatus();
        }
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
                (void)inkpod_core_inkscript_plan_task_release(core, &plan_task);
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
                        InkpodInkScriptReport* ignored{};
                        (void)inkpod_core_inkscript_run_task_take_report(
                            core, run_task, &ignored);
                        (void)inkpod_inkscript_report_release(&ignored);
                        break;
                    }
                }
                (void)inkpod_core_inkscript_run_task_release(core, &run_task);
            }
        }
        if (core != nullptr) {
            (void)inkpod_core_inkscript_confirmation_release(core, &confirmation);
            (void)inkpod_core_inkscript_plan_release(core, &plan);
            (void)inkpod_core_inkscript_program_release(core, &program);
        }
        (void)inkpod_inkscript_source_release(&source);
        host.reset();
        grants.clear();
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
    std::uint64_t document_uuid_low{};
    std::uint64_t document_uuid_high{};
    std::unique_ptr<RouteHost> host;
    std::vector<InkpodInkScriptAuthorityGrant> grants;
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
    std::uint32_t confirmation_scope) noexcept {
    return impl_ == nullptr
        ? CompletedStep(InkScriptEngineResult{})
        : impl_->Advance(core, cancel_requested, confirmation_scope);
}

}  // namespace inkpod::app
