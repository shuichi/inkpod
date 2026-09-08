#include "inkscript_engine_smoke.h"

#include <windows.h>
#include <commctrl.h>

#include <array>
#include <chrono>
#include <cstdio>
#include <string>
#include <thread>

#include "core_host.h"
#include "application_host.h"
#include "renderer/canvas.h"
#include "ui/inkscript_progress.h"

namespace inkpod::app {
namespace {

class NullSnapshotSink final : public renderer::CanvasSnapshotSink {
public:
    renderer::SnapshotRoute Route() const noexcept override { return {}; }
    bool AcceptsSnapshots() const noexcept override { return false; }
    bool Submit(renderer::SnapshotEnvelope envelope) noexcept override {
        if (envelope.snapshot != nullptr) {
            (void)inkpod_snapshot_release(&envelope.snapshot);
        }
        return false;
    }
};

bool CreateTemporaryDirectory(std::wstring& output) noexcept {
    std::array<wchar_t, MAX_PATH> root{};
    const DWORD length = GetTempPathW(
        static_cast<DWORD>(root.size()), root.data());
    if (length == 0U || length >= root.size()) {
        return false;
    }
    try {
        for (std::uint32_t attempt = 0U; attempt < 64U; ++attempt) {
            output.assign(root.data());
            output += L"inkpod-inkscript-engine-smoke-";
            output += std::to_wstring(GetCurrentProcessId());
            output += L"-";
            output += std::to_wstring(GetTickCount64());
            output += L"-";
            output += std::to_wstring(attempt);
            if (CreateDirectoryW(output.c_str(), nullptr) != FALSE) {
                return true;
            }
            if (GetLastError() != ERROR_ALREADY_EXISTS) {
                return false;
            }
        }
    } catch (...) {
        return false;
    }
    return false;
}

bool WaitFor(
    HWND owner,
    CoreHost& host,
    std::uint64_t job_id,
    InkScriptEngineNotificationKind kind,
    CoreNotification& output,
    windows::ui::InkScriptProgressPresentation& progress) noexcept {
    const auto deadline = std::chrono::steady_clock::now()
        + std::chrono::seconds(15);
    do {
        MSG message{};
        while (PeekMessageW(
                   &message,
                   owner,
                   kCoreInkScriptNotification,
                   kCoreInkScriptNotification,
                   PM_REMOVE) != FALSE) {
            CoreNotification notification{};
            if (!host.TakeNotification(
                    static_cast<std::uint64_t>(message.wParam),
                    Generation(static_cast<std::uint64_t>(message.lParam)),
                    notification)) {
                std::fprintf(stderr, "private InkScript notification token was not found\n");
                return false;
            }
            if (notification.kind != CoreNotificationKind::InkScript
                || notification.inkscript.job_id != job_id) {
                continue;
            }
            if (!progress.Observe(notification)) {
                return false;
            }
            if (notification.inkscript.kind == kind) {
                output = notification;
                return true;
            }
            if (notification.inkscript.kind
                    != InkScriptEngineNotificationKind::Progress
                || kind != InkScriptEngineNotificationKind::Completed) {
                std::fprintf(stderr, "private InkScript unexpected notification: expected=%u actual=%u status=%u\n",
                    static_cast<unsigned>(kind), static_cast<unsigned>(notification.inkscript.kind),
                    static_cast<unsigned>(notification.status));
                return false;
            }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    } while (std::chrono::steady_clock::now() < deadline);
    std::fprintf(stderr, "private InkScript notification timed out: kind=%u\n",
        static_cast<unsigned>(kind));
    return false;
}

}  // namespace

int RunPrivateInkScriptEngineSmoke(
    HWND status_bar, windows::ui::JobProgressState& progress_state) noexcept {
    HWND owner = CreateWindowExW(
        0,
        L"STATIC",
        L"inkpod-private-inkscript-smoke",
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
    std::wstring directory;
    if (!CreateTemporaryDirectory(directory)) {
        DestroyWindow(owner);
        return 2;
    }
    const std::wstring output = directory + L"\\route-smoke_0001.inkpod";

    NullSnapshotSink sink;
    CoreHost host;
    windows::ui::InkScriptProgressPresentation progress(host, status_bar, progress_state);
    constexpr DocumentSessionId session{UINT64_C(27001)};
    constexpr Generation generation{UINT64_C(27)};
    CommandContext context{};
    context.document_session = session;
    context.generation = generation;
    int result{};
    if (host.Start(&sink, owner) != INKPOD_STATUS_OK
        || host.CreateSession(session, generation) != INKPOD_STATUS_OK
        || !host.SetActiveSession(session, generation)) {
        result = 3;
    }
    std::uint64_t event_id{};
    if (result == 0
        && host.Invoke(
               session,
               generation,
               [&event_id](InkpodCore* core) {
                   InkpodCellCreateOptions options{};
                   options.struct_size = sizeof(options);
                   options.document_uuid_high = UINT64_C(0x4d323742534d4f4b);
                   options.document_uuid_low = UINT64_C(0x45524f5554450001);
                   options.width = 16U;
                   options.height = 16U;
                   options.dpi_x_milli = 96000U;
                   options.dpi_y_milli = 96000U;
                   InkpodDocumentInfo info{};
                   info.struct_size = sizeof(info);
                   InkpodStatus status = inkpod_core_new_cell(core, &options, &info);
                   InkpodDispatchResult dispatch{};
                   dispatch.struct_size = sizeof(dispatch);
                   std::uint64_t guide{};
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_core_guide_add(
                           core, INKPOD_GUIDE_VERTICAL, 1, &dispatch, &guide);
                   }
                   InkpodHistoryVisualization* visualization{};
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_core_history_visualization_create(
                           core, &visualization);
                   }
                   std::uint64_t count{};
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_history_visualization_row_count(
                           visualization, &count);
                   }
                   InkpodHistoryVisualizationRowBuffer row{};
                   row.struct_size = sizeof(row);
                   if (status == INKPOD_STATUS_OK && count != 0U) {
                       status = inkpod_history_visualization_row_get(
                           visualization, count - 1U, &row);
                   }
                   if (status == INKPOD_STATUS_OK && count != 0U) {
                       event_id = row.journal_event_id;
                   } else if (status == INKPOD_STATUS_OK) {
                       status = INKPOD_STATUS_INVALID_STATE;
                   }
                   const InkpodStatus release =
                       inkpod_history_visualization_release(&visualization);
                   return status == INKPOD_STATUS_OK ? release : status;
               },
               false,
               true) != INKPOD_STATUS_OK) {
        result = 4;
    }
    if (result == 0) {
        InkScriptEngineRequest request{};
        request.job_id = UINT64_C(27001);
        request.controller_id = UINT64_C(27002);
        request.source_id = UINT64_C(27003);
        request.source_generation = 1U;
        request.context = context;
        request.source_utf8 = R"(inkscript 3;
requires { procedure_catalog = 8; replay_epoch = 29; }
inputs { current_document; }
program {
    step "Add guide" { enabled = true; invoke add_guide { axis = vertical; position = 2; }; }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "route-smoke"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
)";
        request.authorized_paths.push_back(directory);
        request.export_event_ids.push_back(event_id);
        if (!progress.Submit(std::move(request))) {
            result = 5;
        }
        UpdateWindow(status_bar);
    }
    CoreNotification plan{};
    if (result == 0) {
        const bool received = WaitFor(
                owner,
                host,
                UINT64_C(27001),
                InkScriptEngineNotificationKind::PlanReady,
                plan, progress);
        const bool confirmed = received && plan.status == INKPOD_STATUS_OK
            && plan.inkscript.total_items == 1U && host.ConfirmInkScript(
                UINT64_C(27001),
                context,
                INKPOD_INKSCRIPT_SCOPE_CURRENT_DOCUMENT);
        if (!confirmed) {
            std::fprintf(stderr, "private InkScript plan: received=%d status=%u items=%llu\n",
                received ? 1 : 0, static_cast<unsigned>(plan.status),
                static_cast<unsigned long long>(plan.inkscript.total_items));
            result = 6;
        }
    }
    CoreNotification completed{};
    if (result == 0
        && (!WaitFor(
                owner,
                host,
                UINT64_C(27001),
                InkScriptEngineNotificationKind::Completed,
                completed, progress)
            || completed.status != INKPOD_STATUS_OK
            || completed.inkscript.outcome != INKPOD_INKSCRIPT_OUTCOME_INSTALLED
            || completed.inkscript.failure != INKPOD_INKSCRIPT_FAILURE_NONE
            || completed.inkscript.exported_commit_count != 1U
            || completed.inkscript.exported_text_bytes == 0U
            || GetFileAttributesW(output.c_str()) == INVALID_FILE_ATTRIBUTES)) {
        result = 7;
    }
    if (result == 0) {
        InkScriptEngineRequest request{};
        request.job_id = UINT64_C(27004);
        request.controller_id = UINT64_C(27005);
        request.source_id = UINT64_C(27006);
        request.source_generation = 1U;
        request.context = context;
        request.source_utf8 = R"(inkscript 3;
requires { procedure_catalog = 8; replay_epoch = 29; }
inputs { current_document; }
program { step "Guide" { enabled = true; invoke add_guide { axis = vertical; position = 3; }; } }
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "cancelled"; start_number = 1; direction = ascending; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
)";
        request.authorized_paths.push_back(directory);
        if (!progress.Submit(std::move(request))
            || !WaitFor(owner, host, UINT64_C(27004), InkScriptEngineNotificationKind::PlanReady, plan, progress)
            || !windows::ui::CancelJobProgress(status_bar, progress_state, progress_state.selected)
            || !WaitFor(owner, host, UINT64_C(27004), InkScriptEngineNotificationKind::Completed, completed, progress)
            || completed.status != INKPOD_STATUS_CANCELLED
            || GetFileAttributesW((directory + L"\\cancelled_0001.inkpod").c_str()) != INVALID_FILE_ATTRIBUTES) {
            result = 10;
        }
    }
    progress.Clear();
    host.Stop();
    if (DeleteFileW(output.c_str()) == FALSE
        && GetLastError() != ERROR_FILE_NOT_FOUND) {
        result = result == 0 ? 8 : result;
    }
    if (RemoveDirectoryW(directory.c_str()) == FALSE) {
        result = result == 0 ? 9 : result;
    }
    DestroyWindow(owner);
    if (result != 0) {
        std::fprintf(stderr, "private InkScript engine smoke failed: %d\n", result);
    }
    return result;
}

int RunPrivateInkScriptEngineSmoke() noexcept {
    const INITCOMMONCONTROLSEX controls{sizeof(controls), ICC_BAR_CLASSES | ICC_PROGRESS_CLASS};
    if (!InitCommonControlsEx(&controls)) {
        return 11;
    }
    HWND owner = CreateWindowExW(0U, L"STATIC", L"InkScript private status", 0U,
        0, 0, 700, 50, HWND_MESSAGE, nullptr, GetModuleHandleW(nullptr), nullptr);
    HWND status = owner == nullptr ? nullptr : CreateWindowExW(0U, STATUSCLASSNAMEW, L"", WS_CHILD,
        0, 0, 700, 24, owner, nullptr, GetModuleHandleW(nullptr), nullptr);
    windows::ui::JobProgressState progress{};
    const int result = status != nullptr && windows::ui::InitializeJobProgress(status, progress, nullptr, nullptr)
        ? RunPrivateInkScriptEngineSmoke(status, progress) : 12;
    if (status != nullptr) DestroyWindow(status);
    if (owner != nullptr) DestroyWindow(owner);
    return result;
}

int RunPrivateInkScriptPublicationSmoke(ApplicationHost& application) noexcept {
    if (application.engine == nullptr) return 20;
    const auto original = application.routing.targets.Capture();
    if (!original.document_view.has_value() || !original.editor_group.has_value()) return 21;
    HWND owner = application.Workspace().windows.window;
    HWND status = application.Workspace().windows.status_bar;
    windows::ui::InkScriptProgressPresentation progress(*application.engine,
        status, application.Workspace().job_progress_state);
    for (const auto mode : {INKPOD_INKSCRIPT_RUN_INSTALL, INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW}) {
        const auto target = application.PrepareDocumentSession();
        if (!target.has_value()) return 22;
        auto destination = original;
        destination.document_session = target->session;
        destination.document_view = target->view;
        destination.generation = target->generation;
        const std::uint64_t job = UINT64_C(27100) + mode;
        InkScriptEngineRequest request{};
        request.job_id = job;
        request.controller_id = job + 100U;
        request.source_id = job + 200U;
        request.source_generation = 1U;
        request.context = original;
        request.publication_targets.push_back(destination);
        request.run_mode = mode;
        request.source_utf8 = R"(inkscript 3;
requires { procedure_catalog = 8; replay_epoch = 29; }
inputs { profile = batch; current_document; }
program { step "Guide" { enabled = true; invoke add_guide { axis = vertical; position = 2; }; } }
output { policy = new_tabs; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
)";
        CoreNotification notification{};
        bool published{};
        int failure{};
        if (!progress.Submit(std::move(request))
            || !WaitFor(owner, *application.engine, job, InkScriptEngineNotificationKind::PlanReady, notification, progress)
            || !application.engine->ConfirmInkScript(job, original, INKPOD_INKSCRIPT_SCOPE_ALL)
            || !WaitFor(owner, *application.engine, job, InkScriptEngineNotificationKind::Completed, notification, progress)
            || notification.status != INKPOD_STATUS_OK || notification.context != original
            || notification.inkscript.published_result_count != 1U
            || application.routing.targets.Resolve(original, kDocumentSessionCommandScope) != CommandResolveStatus::Ok) {
            failure = 23;
        } else if (!application.PublishPreparedDocumentSession(*target, *original.editor_group)) {
            failure = 24;
        } else {
            published = true;
            auto* document = application.Documents().Find(target->session);
            if (document == nullptr) {
                failure = 25;
            } else {
                if (mode == INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW) {
                    document->shell.batch_preview_source_context = original;
                }
                if (!application.ActivateDocumentView(target->view)
                    || (mode == INKPOD_INKSCRIPT_RUN_IMAGE_PREVIEW
                        && document->shell.batch_preview_source_context != original)) {
                    failure = 26;
                }
            }
        }
        progress.Clear();
        if (!application.ActivateDocumentView(*original.document_view)) failure = 27;
        const bool removed = published
            ? application.CloseDocumentSession(target->session)
            : application.DiscardPreparedDocumentSession(*target);
        if (!removed) failure = 28;
        if (failure != 0) {
            std::fprintf(stderr, "private InkScript tab publication failed: %d\n", failure);
            return failure;
        }
    }
    return 0;
}

}  // namespace inkpod::app
