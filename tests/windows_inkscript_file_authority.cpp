#include <windows.h>

#include <array>
#include <cstdio>
#include <filesystem>
#include <string>
#include <thread>
#include <vector>

#include "app/inkscript_file_authority.h"

namespace {
using inkpod::app::InkScriptFileAuthorityAdapter;

struct Directory final {
    std::wstring path;
    Directory() {
        std::array<wchar_t, MAX_PATH> root{};
        if (GetTempPathW(static_cast<DWORD>(root.size()), root.data()) != 0U) {
            path = root.data();
            path += L"inkpod-shared-authority-" + std::to_wstring(GetCurrentProcessId())
                + L"-" + std::to_wstring(GetTickCount64());
            if (!CreateDirectoryW(path.c_str(), nullptr)) path.clear();
        }
    }
    ~Directory() {
        if (!path.empty()) {
            std::error_code error;
            std::filesystem::remove_all(path, error);
        }
    }
};

InkpodStatus NewCell(InkpodCore* core, std::uint64_t uuid) {
    InkpodCellCreateOptions request{};
    request.struct_size = sizeof(request);
    request.document_uuid_high = UINT64_C(0x415554484f524954);
    request.document_uuid_low = uuid;
    request.width = 16U;
    request.height = 16U;
    request.dpi_x_milli = 96000U;
    request.dpi_y_milli = 96000U;
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return inkpod_core_new_cell(core, &request, &info);
}

bool CreateNative(const std::wstring& path, std::uint64_t uuid) {
    InkpodCore* core{};
    const InkpodCoreConfig config{sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK) return false;
    std::string utf8;
    InkpodStatus status = NewCell(core, uuid);
    if (status == INKPOD_STATUS_OK) status = InkScriptFileAuthorityAdapter::PathUtf8(path, utf8);
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    if (status == INKPOD_STATUS_OK) status = inkpod_core_save(
        core, reinterpret_cast<const std::uint8_t*>(utf8.data()), utf8.size(), &info);
    const InkpodStatus released = inkpod_core_destroy(&core);
    return status == INKPOD_STATUS_OK && released == INKPOD_STATUS_OK;
}

std::vector<std::uint8_t> Read(const std::wstring& path) {
    std::vector<std::uint8_t> result;
    const HANDLE file = CreateFileW(path.c_str(), GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, 0U, nullptr);
    if (file == INVALID_HANDLE_VALUE) return result;
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) && size.QuadPart > 0 && size.QuadPart <= 1024 * 1024) {
        result.resize(static_cast<std::size_t>(size.QuadPart));
        DWORD count{};
        if (!ReadFile(file, result.data(), static_cast<DWORD>(result.size()), &count, nullptr)
            || count != result.size()) result.clear();
    }
    CloseHandle(file);
    return result;
}

std::string Source(bool overwrite) {
    return std::string(R"(inkscript 3;
requires { procedure_catalog = 8; replay_epoch = 29; }
inputs { file "input.inkpod"; }
program { step "Guide" { enabled = true; invoke add_guide { axis = vertical; position = 2; }; } }
)") + (overwrite ? R"(output { policy = explicit_overwrite; format = inkpod; }
)" : R"(output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = "authority"; start_number = 1; direction = ascending; }
)") + "execution { failure = stop; wait_ms = 0; preview_before_save = false; }\n";
}

class Run final {
public:
    InkpodCore* core{};
    InkScriptFileAuthorityAdapter* adapter{};
    InkpodInkScriptProgram* program{};
    InkpodInkScriptPlanTask* planning{};
    InkpodInkScriptPlan* plan{};
    InkpodInkScriptConfirmation* confirmation{};
    InkpodInkScriptRunTask* running{};
    InkpodInkScriptReport* report{};
    ~Run() {
        if (running != nullptr) {
            (void)inkpod_inkscript_run_task_cancel(running);
            for (unsigned count = 0; count < 4U; ++count) {
                InkpodInkScriptTaskEvent event{};
                event.struct_size = sizeof(event);
                event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
                if (inkpod_core_inkscript_run_task_advance(core, running) != INKPOD_STATUS_OK) break;
                (void)inkpod_core_inkscript_run_task_event_take(core, running, &event);
                if (event.kind == INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE) break;
            }
            (void)inkpod_core_inkscript_run_task_release(core, &running);
        }
        if (planning != nullptr) {
            (void)inkpod_inkscript_plan_task_cancel(planning);
            (void)inkpod_core_inkscript_plan_task_advance(core, planning);
            InkpodInkScriptTaskEvent event{};
            event.struct_size = sizeof(event);
            event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            (void)inkpod_core_inkscript_plan_task_event_take(core, planning, &event);
            (void)inkpod_core_inkscript_plan_task_release(core, &planning);
        }
        (void)inkpod_inkscript_report_release(&report);
        if (core != nullptr) {
            (void)inkpod_core_inkscript_confirmation_release(core, &confirmation);
            (void)inkpod_core_inkscript_plan_release(core, &plan);
            (void)inkpod_core_inkscript_program_release(core, &program);
            delete adapter;
            (void)inkpod_core_destroy(&core);
        }
    }
    InkpodStatus Initialize(const std::string& source, const std::vector<std::wstring>& paths) {
        const InkpodCoreConfig config{sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
        InkpodStatus status = inkpod_core_create(&config, &core);
        if (status == INKPOD_STATUS_OK) status = NewCell(core, 9900U);
        if (status != INKPOD_STATUS_OK) return status;
        InkpodInkScriptSourceInput input{};
        input.struct_size = sizeof(input);
        input.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        input.controller_id = 41U;
        input.session_generation = 2U;
        input.source_id = 3U;
        input.source_utf8 = reinterpret_cast<const std::uint8_t*>(source.data());
        input.source_bytes = source.size();
        InkpodInkScriptSource* parsed{};
        status = inkpod_inkscript_source_parse(&input, &parsed);
        InkpodInkScriptCompileRequest compile{};
        compile.struct_size = sizeof(compile);
        compile.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        compile.controller_id = 41U;
        compile.session_generation = 2U;
        if (status == INKPOD_STATUS_OK) status = inkpod_core_inkscript_compile(core, parsed, &compile, &program);
        (void)inkpod_inkscript_source_release(&parsed);
        if (status != INKPOD_STATUS_OK) return status;
        adapter = new InkScriptFileAuthorityAdapter;
        return adapter->Initialize(core, nullptr, program, paths, 0U);
    }
    InkpodStatus Plan() {
        InkpodInkScriptSharedPlanRequest request{};
        request.struct_size = sizeof(request);
        request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        request.controller_id = 41U;
        request.session_generation = 2U;
        InkpodStatus status = inkpod_core_inkscript_shared_plan_task_create(core, program,
            adapter->Handle(), &request, &planning);
        if (status == INKPOD_STATUS_OK) status = inkpod_core_inkscript_plan_task_advance(core, planning);
        InkpodInkScriptTaskEvent event{};
        event.struct_size = sizeof(event);
        event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        if (status == INKPOD_STATUS_OK) status = inkpod_core_inkscript_plan_task_event_take(core, planning, &event);
        if (status == INKPOD_STATUS_OK) status = inkpod_core_inkscript_plan_task_take_plan(core, planning, &plan);
        if (status == INKPOD_STATUS_OK) status = inkpod_core_inkscript_plan_task_release(core, &planning);
        return status;
    }
    InkpodStatus Start() {
        InkpodInkScriptConfirmationRequest request{};
        request.struct_size = sizeof(request);
        request.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        request.scope = INKPOD_INKSCRIPT_SCOPE_ALL;
        InkpodStatus status = inkpod_core_inkscript_confirmation_create(core, plan, &request, &confirmation);
        InkpodInkScriptRunRequest run{};
        run.struct_size = sizeof(run);
        run.version = INKPOD_INKSCRIPT_RECORD_VERSION;
        run.mode = INKPOD_INKSCRIPT_RUN_INSTALL;
        run.controller_id = 41U;
        run.session_generation = 2U;
        if (status == INKPOD_STATUS_OK) status = inkpod_core_inkscript_run_task_create(core,
            program, &plan, &confirmation, &run, &running);
        return status;
    }
    InkpodStatus Finish(InkpodInkScriptReportItem& result) {
        InkpodStatus status = INKPOD_STATUS_OK;
        for (unsigned count = 0U; count < 8U; ++count) {
            InkpodInkScriptTaskEvent event{};
            event.struct_size = sizeof(event);
            event.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            status = inkpod_core_inkscript_run_task_advance(core, running);
            // A cancelled terminal advance still owns its completion event and
            // report; verify the report's public cancellation outcome below.
            if (status == INKPOD_STATUS_OK || status == INKPOD_STATUS_CANCELLED)
                status = inkpod_core_inkscript_run_task_event_take(core, running, &event);
            if (status != INKPOD_STATUS_OK) return status;
            if (event.kind != INKPOD_INKSCRIPT_EVENT_RUN_COMPLETE) continue;
            status = inkpod_core_inkscript_run_task_take_report(core, running, &report);
            if (status != INKPOD_STATUS_OK) return status;
            (void)inkpod_core_inkscript_run_task_release(core, &running);
            InkpodInkScriptReportBuffer query{};
            query.struct_size = sizeof(query);
            query.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            status = inkpod_inkscript_report_items_copy(report, &query);
            if (status != INKPOD_STATUS_BUFFER_TOO_SMALL || query.required_records != 1U
                || query.required_utf8_bytes > 65536U) return INKPOD_STATUS_INVALID_STATE;
            std::vector<std::uint8_t> text(static_cast<std::size_t>(query.required_utf8_bytes));
            result.struct_size = sizeof(result);
            result.version = INKPOD_INKSCRIPT_RECORD_VERSION;
            query.records = &result;
            query.record_capacity = 1U;
            query.record_stride_bytes = sizeof(result);
            query.utf8 = text.data();
            query.utf8_capacity_bytes = text.size();
            return inkpod_inkscript_report_items_copy(report, &query);
        }
        return INKPOD_STATUS_INVALID_STATE;
    }
};

int TestSharedAuthority() {
    Directory directory;
    if (directory.path.empty()) return 1;
    const auto input = directory.path + L"\\input.inkpod";
    const auto alias = directory.path + L"\\alias.inkpod";
    if (!CreateNative(input, 1U) || !CreateHardLinkW(alias.c_str(), input.c_str(), nullptr)) return 2;
    InkpodIoManager* manager{};
    if (inkpod_io_manager_create(nullptr, &manager) != INKPOD_STATUS_OK) return 3;
    std::string first, second;
    if (InkScriptFileAuthorityAdapter::PathUtf8(input, first) != INKPOD_STATUS_OK
        || InkScriptFileAuthorityAdapter::PathUtf8(alias, second) != INKPOD_STATUS_OK) return 24;
    InkpodIoFileIdentity a{}, b{};
    a.struct_size = sizeof(a);
    b.struct_size = sizeof(b);
    const bool identities = inkpod_io_resolve_identity(manager,
        reinterpret_cast<const std::uint8_t*>(first.data()), first.size(), &a) == INKPOD_STATUS_OK
        && inkpod_io_resolve_identity(manager,
            reinterpret_cast<const std::uint8_t*>(second.data()), second.size(), &b) == INKPOD_STATUS_OK
        && a.volume == b.volume && a.object_high == b.object_high && a.object_low == b.object_low;
    (void)inkpod_io_manager_release(&manager);
    if (!identities) return 4;
    const auto before = Read(input);
    if (before.empty()) return 25;
    {
        Run run;
        if (run.Initialize(Source(true), {input, input}) != INKPOD_STATUS_OK || run.Plan() != INKPOD_STATUS_OK) return 5;
        InkpodStatus cross_thread{};
        std::thread thread([&] { cross_thread = inkpod_core_inkscript_io_invalidate(run.core, run.adapter->Handle(), 0U); });
        thread.join();
        if (cross_thread != INKPOD_STATUS_WRONG_THREAD || run.Start() != INKPOD_STATUS_OK) return 6;
        InkpodInkScriptReportItem result{};
        if (run.Finish(result) != INKPOD_STATUS_OK || result.outcome != INKPOD_INKSCRIPT_OUTCOME_INSTALLED) return 7;
        if (Read(input) == before || Read(alias) != before) return 8;
    }
    // Revoke the shared authority after a plan: no published file or temporary.
    {
        Run run;
        if (run.Initialize(Source(true), {input, input}) != INKPOD_STATUS_OK || run.Plan() != INKPOD_STATUS_OK) return 9;
        const auto original = Read(input);
        if (inkpod_core_inkscript_io_invalidate(run.core, run.adapter->Handle(), 0U) != INKPOD_STATUS_OK) return 10;
        const auto started = run.Start();
        if (started == INKPOD_STATUS_OK) {
            InkpodInkScriptReportItem result{};
            if (run.Finish(result) == INKPOD_STATUS_OK && result.outcome == INKPOD_INKSCRIPT_OUTCOME_INSTALLED) return 11;
        }
        if (Read(input) != original) return 12;
    }
    // A replaced source after planning fails before installation.
    {
        Run run;
        if (run.Initialize(Source(true), {input, input}) != INKPOD_STATUS_OK || run.Plan() != INKPOD_STATUS_OK) return 13;
        const auto replacement = directory.path + L"\\replacement.inkpod";
        if (!CreateNative(replacement, 9U) || !MoveFileExW(replacement.c_str(), input.c_str(), MOVEFILE_REPLACE_EXISTING)) return 14;
        const auto original = Read(input);
        if (run.Start() != INKPOD_STATUS_OK) return 15;
        InkpodInkScriptReportItem result{};
        if (run.Finish(result) != INKPOD_STATUS_OK || result.outcome != INKPOD_INKSCRIPT_OUTCOME_FAILED
            || Read(input) != original) return 16;
    }
    // A real write handle prevents guarded install. Dropping the job releases
    // its exact temporary owners and permits a new writer after completion.
    {
        Run run;
        if (run.Initialize(Source(true), {input, input}) != INKPOD_STATUS_OK || run.Plan() != INKPOD_STATUS_OK
            || run.Start() != INKPOD_STATUS_OK) return 17;
        HANDLE writer = CreateFileW(input.c_str(), GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, 0U, nullptr);
        if (writer == INVALID_HANDLE_VALUE) return 18;
        const auto original = Read(input);
        InkpodInkScriptReportItem result{};
        const auto status = run.Finish(result);
        CloseHandle(writer);
        if (status != INKPOD_STATUS_OK || result.outcome != INKPOD_INKSCRIPT_OUTCOME_FAILED
            || Read(input) != original) return 19;
    }
    {
        Run run;
        if (run.Initialize(Source(true), {input, input}) != INKPOD_STATUS_OK || run.Plan() != INKPOD_STATUS_OK
            || run.Start() != INKPOD_STATUS_OK) return 20;
        const auto original = Read(input);
        if (inkpod_inkscript_run_task_cancel(run.running) != INKPOD_STATUS_OK) return 21;
        InkpodInkScriptReportItem result{};
        if (run.Finish(result) != INKPOD_STATUS_OK || result.outcome != INKPOD_INKSCRIPT_OUTCOME_CANCELLED
            || Read(input) != original) return 22;
    }
    for (const auto& entry : std::filesystem::directory_iterator(directory.path)) {
        if (entry.path().extension() != L".inkpod") return 23;
    }
    return 0;
}
}  // namespace

// The removed callback ABI is no longer the Windows I/O engine. Equivalent
// exact-temp substitution, writer exclusion, parent/reparse guard, collision,
// final-fingerprint, cancellation and cleanup contracts are also tested directly
// at the owning shared manager in rust/inkpod-io/tests/authority.rs. This target
// verifies those guards through the private Windows shared-ABI integration.
int wmain() {
    const int status = TestSharedAuthority();
    if (status != 0) std::fprintf(stderr, "shared InkScript authority failed: %d\n", status);
    return status;
}
