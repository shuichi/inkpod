#include <windows.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <future>
#include <string>
#include <thread>
#include <type_traits>
#include <vector>

#include "app/core_host.h"
#include "renderer/canvas.h"

namespace {

using inkpod::app::CommandContext;
using inkpod::app::CoreHost;
using inkpod::app::CoreNotification;
using inkpod::app::CoreNotificationKind;
using inkpod::app::DocumentSessionId;
using inkpod::app::Generation;
using inkpod::app::InkScriptEngineNotificationKind;
using inkpod::app::InkScriptEngineRequest;
using inkpod::app::InkScriptEngineResult;

static_assert(std::is_trivially_copyable_v<InkScriptEngineResult>);

class SnapshotSink final : public inkpod::renderer::CanvasSnapshotSink {
public:
    inkpod::renderer::SnapshotRoute Route() const noexcept override { return {}; }
    bool AcceptsSnapshots() const noexcept override { return false; }

    bool Submit(inkpod::renderer::SnapshotEnvelope envelope) noexcept override {
        ++submissions_;
        if (envelope.snapshot != nullptr) {
            (void)inkpod_snapshot_release(&envelope.snapshot);
        }
        return false;
    }

    [[nodiscard]] std::uint64_t Submissions() const noexcept {
        return submissions_.load(std::memory_order_relaxed);
    }

private:
    std::atomic<std::uint64_t> submissions_{};
};

CommandContext Context(DocumentSessionId session, Generation generation) noexcept {
    CommandContext context{};
    context.document_session = session;
    context.generation = generation;
    return context;
}

InkpodStatus NewCell(InkpodCore* core, std::uint64_t uuid) noexcept {
    InkpodCellCreateOptions options{};
    options.struct_size = sizeof(options);
    options.document_uuid_high = UINT64_C(0x4d32374200000000) | uuid;
    options.document_uuid_low = UINT64_C(0x524f555445000000) | uuid;
    options.width = 16U;
    options.height = 16U;
    options.dpi_x_milli = 96000U;
    options.dpi_y_milli = 96000U;
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return inkpod_core_new_cell(core, &options, &info);
}

bool CreateTemporaryDirectory(std::wstring& output) {
    std::array<wchar_t, MAX_PATH> root{};
    const DWORD length = GetTempPathW(static_cast<DWORD>(root.size()), root.data());
    if (length == 0U || length >= root.size()) {
        return false;
    }
    for (std::uint32_t attempt = 0U; attempt < 64U; ++attempt) {
        output.assign(root.data());
        output += L"inkpod-inkscript-engine-route-";
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
    return false;
}

bool WideToUtf8(const std::wstring& input, std::vector<std::uint8_t>& output) {
    if (input.empty() || input.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int required = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        input.data(),
        static_cast<int>(input.size()),
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
               input.data(),
               static_cast<int>(input.size()),
               reinterpret_cast<char*>(output.data()),
               required,
               nullptr,
               nullptr)
        == required;
}

std::string CurrentDocumentSource(
    std::uint32_t wait_milliseconds,
    const char* basename,
    std::uint64_t start_number = 1U) {
    std::string source = R"(inkscript 2;
requires { procedure_catalog = 5; replay_epoch = 27; }
inputs { current_document; }
program {
    step "Add guide" as created {
        enabled = true;
        invoke add_guide { axis = vertical; position = 2; };
    }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = ")";
    source += basename;
    source += R"("; start_number = )";
    source += std::to_string(start_number);
    source += R"(; direction = ascending; }
execution { failure = stop; wait_ms = )";
    source += std::to_string(wait_milliseconds);
    source += "; preview_before_save = false; }\n";
    return source;
}

std::string FolderSource(
    std::uint32_t wait_milliseconds,
    const char* basename,
    std::uint64_t start_number = 1U) {
    std::string source = R"(inkscript 2;
requires { procedure_catalog = 5; replay_epoch = 27; }
inputs { folder "in"; }
program {
    step "Add guide" {
        enabled = true;
        invoke add_guide { axis = vertical; position = 2; };
    }
}
output { policy = duplicate; format = inkpod; folder = "out"; cell_folder = false; basename = ")";
    source += basename;
    source += R"("; start_number = )";
    source += std::to_string(start_number);
    source += R"(; direction = ascending; }
execution { failure = stop; wait_ms = )";
    source += std::to_string(wait_milliseconds);
    source += "; preview_before_save = false; }\n";
    return source;
}

std::string OverwriteFolderSource() {
    return R"(inkscript 2;
requires { procedure_catalog = 5; replay_epoch = 27; }
inputs { file "input.inkpod"; }
program {
    step "Add guide" {
        enabled = true;
        invoke add_guide { axis = vertical; position = 2; };
    }
}
output { policy = explicit_overwrite; format = inkpod; }
execution { failure = stop; wait_ms = 0; preview_before_save = false; }
)";
}

InkScriptEngineRequest Request(
    std::uint64_t job_id,
    const CommandContext& context,
    std::string source,
    std::vector<std::wstring> authorized_paths) {
    InkScriptEngineRequest request{};
    request.job_id = job_id;
    request.controller_id = 2700U + job_id;
    request.source_id = 3700U + job_id;
    request.source_generation = 1U;
    request.context = context;
    request.source_utf8 = std::move(source);
    request.authorized_paths = std::move(authorized_paths);
    return request;
}

void PrintUnexpected(
    InkScriptEngineNotificationKind wanted,
    const CoreNotification& notification) {
    std::fprintf(
        stderr,
        "unexpected InkScript notification: wanted=%u actual=%u phase=%u "
        "status=%u failure=%u host_op=%u host_status=%u\n",
        static_cast<unsigned>(wanted),
        static_cast<unsigned>(notification.inkscript.kind),
        static_cast<unsigned>(notification.inkscript.phase),
        notification.status,
        notification.inkscript.failure,
        notification.inkscript.last_host_operation,
        notification.inkscript.last_host_status);
    if (notification.inkscript.diagnostic_bytes != 0U) {
        std::fwrite(
            notification.inkscript.diagnostic_utf8.data(),
            1U,
            static_cast<std::size_t>(notification.inkscript.diagnostic_bytes),
            stderr);
        std::fputc('\n', stderr);
    }
}

bool TakePostedNotification(
    CoreHost& host,
    const MSG& message,
    CoreNotification& output) {
    return host.TakeNotification(
        static_cast<std::uint64_t>(message.wParam),
        Generation(static_cast<std::uint64_t>(message.lParam)),
        output);
}

bool WaitForNotification(
    HWND owner,
    CoreHost& host,
    std::uint64_t job_id,
    InkScriptEngineNotificationKind kind,
    CoreNotification& output,
    std::chrono::milliseconds timeout = std::chrono::seconds(15)) {
    const auto deadline = std::chrono::steady_clock::now() + timeout;
    do {
        MSG message{};
        while (PeekMessageW(
                   &message,
                   owner,
                   inkpod::app::kCoreInkScriptNotification,
                   inkpod::app::kCoreInkScriptNotification,
                   PM_REMOVE) != FALSE) {
            CoreNotification notification{};
            if (!TakePostedNotification(host, message, notification)) {
                return false;
            }
            if (notification.kind != CoreNotificationKind::InkScript
                || notification.inkscript.job_id != job_id) {
                continue;
            }
            if (notification.inkscript.kind == kind) {
                output = notification;
                return true;
            }
            if (notification.inkscript.kind
                    == InkScriptEngineNotificationKind::Progress
                && kind == InkScriptEngineNotificationKind::Completed) {
                continue;
            }
            PrintUnexpected(kind, notification);
            return false;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    } while (std::chrono::steady_clock::now() < deadline);
    return false;
}

bool WaitForProgressEvent(
    HWND owner,
    CoreHost& host,
    std::uint64_t job_id,
    std::uint32_t event_kind,
    CoreNotification& output,
    std::chrono::milliseconds timeout = std::chrono::seconds(15)) {
    const auto deadline = std::chrono::steady_clock::now() + timeout;
    do {
        MSG message{};
        while (PeekMessageW(
                   &message,
                   owner,
                   inkpod::app::kCoreInkScriptNotification,
                   inkpod::app::kCoreInkScriptNotification,
                   PM_REMOVE) != FALSE) {
            CoreNotification notification{};
            if (!TakePostedNotification(host, message, notification)) {
                return false;
            }
            if (notification.kind != CoreNotificationKind::InkScript
                || notification.inkscript.job_id != job_id) {
                continue;
            }
            if (notification.inkscript.kind
                    == InkScriptEngineNotificationKind::Progress
                && notification.inkscript.event_kind == event_kind) {
                output = notification;
                return true;
            }
            if (notification.inkscript.kind
                == InkScriptEngineNotificationKind::Completed) {
                PrintUnexpected(
                    InkScriptEngineNotificationKind::Progress,
                    notification);
                return false;
            }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    } while (std::chrono::steady_clock::now() < deadline);
    return false;
}

struct DocumentState final {
    InkpodDocumentInfo document{};
    InkpodEditorStateInfo editor{};
    InkpodHistoryInfo history{};
    std::array<std::uint8_t, 32U> digest{};
};

InkpodStatus CaptureCoreState(InkpodCore* core, DocumentState& output) noexcept {
    output = {};
    output.document.struct_size = sizeof(output.document);
    output.editor.struct_size = sizeof(output.editor);
    output.history.struct_size = sizeof(output.history);
    InkpodStatus status = inkpod_core_get_document_info(core, &output.document);
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_get_editor_state(core, &output.editor);
    }
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_history_info(core, &output.history);
    }
    const InkpodSnapshotOptions options{
        sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
    InkpodSnapshot* snapshot{};
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_build_snapshot(core, &options, &snapshot);
    }
    InkpodCanonicalDigest digest{};
    digest.struct_size = sizeof(digest);
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_snapshot_get_canonical_digest(snapshot, &digest);
    }
    if (status == INKPOD_STATUS_OK) {
        std::copy(std::begin(digest.bytes), std::end(digest.bytes), output.digest.begin());
    }
    const InkpodStatus release = inkpod_snapshot_release(&snapshot);
    return status == INKPOD_STATUS_OK ? release : status;
}

bool SameState(const DocumentState& left, const DocumentState& right) noexcept {
    return left.document.document_uuid_high == right.document.document_uuid_high
        && left.document.document_uuid_low == right.document.document_uuid_low
        && left.document.document_revision == right.document.document_revision
        && left.document.flags == right.document.flags
        && left.editor.editor_revision == right.editor.editor_revision
        && left.editor.flags == right.editor.flags
        && std::memcmp(
               left.editor.editor_digest,
               right.editor.editor_digest,
               sizeof(left.editor.editor_digest)) == 0
        && left.history.cursor == right.history.cursor
        && left.history.item_count == right.history.item_count
        && left.digest == right.digest;
}

class Fixture final {
public:
    explicit Fixture(std::uint64_t key)
        : session_(DocumentSessionId(270U + key)),
          generation_(Generation(27U + key)),
          context_(Context(session_, generation_)) {
        owner_ = CreateWindowExW(
            0,
            L"STATIC",
            L"inkpod-inkscript-engine-route-test",
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            nullptr,
            GetModuleHandleW(nullptr),
            nullptr);
        if (owner_ == nullptr || !CreateTemporaryDirectory(directory_)) {
            return;
        }
        if (host_.Start(&sink_, owner_) != INKPOD_STATUS_OK
            || host_.CreateSession(session_, generation_) != INKPOD_STATUS_OK
            || !host_.SetActiveSession(session_, generation_)) {
            return;
        }
        if (host_.Invoke(
                session_,
                generation_,
                [key](InkpodCore* core) { return NewCell(core, key); },
                false,
                true) == INKPOD_STATUS_OK) {
            ready_ = true;
        }
    }

    ~Fixture() {
        host_.Stop();
        std::error_code ignored;
        if (!directory_.empty()) {
            std::filesystem::remove_all(directory_, ignored);
        }
        if (owner_ != nullptr) {
            DestroyWindow(owner_);
        }
    }

    Fixture(const Fixture&) = delete;
    Fixture& operator=(const Fixture&) = delete;

    [[nodiscard]] bool Ready() const noexcept { return ready_; }
    [[nodiscard]] HWND Owner() const noexcept { return owner_; }
    [[nodiscard]] CoreHost& Host() noexcept { return host_; }
    [[nodiscard]] SnapshotSink& Sink() noexcept { return sink_; }
    [[nodiscard]] DocumentSessionId Session() const noexcept { return session_; }
    [[nodiscard]] Generation SessionGeneration() const noexcept { return generation_; }
    [[nodiscard]] const CommandContext& Command() const noexcept { return context_; }
    [[nodiscard]] const std::wstring& Directory() const noexcept { return directory_; }

    bool MakeDirectory(std::wstring_view name, std::wstring& output) const {
        output = directory_ + L"\\" + std::wstring(name);
        return CreateDirectoryW(output.c_str(), nullptr) != FALSE;
    }

    bool Capture(DocumentState& output) {
        return host_.Invoke(
                   session_,
                   generation_,
                   [&output](InkpodCore* core) {
                       return CaptureCoreState(core, output);
                   },
                   false,
                   false) == INKPOD_STATUS_OK;
    }

    bool AddGuideAndCaptureLastEvent(
        std::uint64_t& event_id,
        DocumentState& output) {
        return host_.Invoke(
                   session_,
                   generation_,
                   [&event_id, &output](InkpodCore* core) {
                       InkpodDispatchResult dispatch{};
                       dispatch.struct_size = sizeof(dispatch);
                       std::uint64_t guide{};
                       InkpodStatus status = inkpod_core_guide_add(
                           core, INKPOD_GUIDE_VERTICAL, 1, &dispatch, &guide);
                       InkpodHistoryVisualization* visualization{};
                       if (status == INKPOD_STATUS_OK) {
                           status = inkpod_core_history_visualization_create(
                               core, &visualization);
                       }
                       std::uint64_t row_count{};
                       if (status == INKPOD_STATUS_OK) {
                           status = inkpod_history_visualization_row_count(
                               visualization, &row_count);
                       }
                       InkpodHistoryVisualizationRowBuffer row{};
                       row.struct_size = sizeof(row);
                       if (status == INKPOD_STATUS_OK && row_count != 0U) {
                           status = inkpod_history_visualization_row_get(
                               visualization, row_count - 1U, &row);
                       }
                       if (status == INKPOD_STATUS_OK && row_count != 0U) {
                           event_id = row.journal_event_id;
                           status = CaptureCoreState(core, output);
                       } else if (status == INKPOD_STATUS_OK) {
                           status = INKPOD_STATUS_INVALID_STATE;
                       }
                       const InkpodStatus release =
                           inkpod_history_visualization_release(&visualization);
                       return status == INKPOD_STATUS_OK ? release : status;
                   },
                   false,
                   true) == INKPOD_STATUS_OK;
    }

private:
    HWND owner_{};
    std::wstring directory_;
    SnapshotSink sink_;
    CoreHost host_;
    DocumentSessionId session_{};
    Generation generation_{};
    CommandContext context_;
    bool ready_{};
};

bool CreateNative(const std::wstring& path, std::uint64_t uuid) {
    InkpodCoreConfig config{
        sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
    InkpodCore* core{};
    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK) {
        return false;
    }
    InkpodStatus status = NewCell(core, uuid);
    InkpodDispatchResult dispatch{};
    dispatch.struct_size = sizeof(dispatch);
    std::uint64_t guide{};
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_guide_add(
            core, INKPOD_GUIDE_HORIZONTAL, 1, &dispatch, &guide);
    }
    std::vector<std::uint8_t> utf8;
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    if (status == INKPOD_STATUS_OK && WideToUtf8(path, utf8)) {
        status = inkpod_core_save(core, utf8.data(), utf8.size(), &info);
    } else if (status == INKPOD_STATUS_OK) {
        status = INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodStatus destroy = inkpod_core_destroy(&core);
    return status == INKPOD_STATUS_OK && destroy == INKPOD_STATUS_OK;
}

bool ValidateNativeOutput(
    const std::wstring& path,
    const DocumentState& source,
    const InkScriptEngineResult& result) {
    std::vector<std::uint8_t> utf8;
    if (!WideToUtf8(path, utf8)) {
        return false;
    }
    InkpodCoreConfig config{
        sizeof(InkpodCoreConfig), INKPOD_ABI_VERSION, INKPOD_FEATURE_NONE};
    InkpodCore* core{};
    if (inkpod_core_create(&config, &core) != INKPOD_STATUS_OK) {
        return false;
    }
    DocumentState opened{};
    InkpodPersistenceInfo persistence{};
    persistence.struct_size = sizeof(persistence);
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    InkpodStatus status = inkpod_core_open(core, utf8.data(), utf8.size(), &info);
    if (status == INKPOD_STATUS_OK) {
        status = CaptureCoreState(core, opened);
    }
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_get_persistence_info(core, &persistence);
    }
    const bool clean_open = status == INKPOD_STATUS_OK
        && persistence.format_version == 32U
        && persistence.open_strategy == INKPOD_NATIVE_OPEN_FULL_REPLAY
        && (opened.document.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        && (opened.editor.flags & INKPOD_EDITOR_STATE_DIRTY) == 0U
        && opened.history.item_count == source.history.item_count + 1U
        && std::any_of(
            result.final_state_digest.begin(),
            result.final_state_digest.end(),
            [](std::uint8_t value) { return value != 0U; });
    InkpodDispatchResult dispatch{};
    dispatch.struct_size = sizeof(dispatch);
    DocumentState undone{};
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_undo(core, &dispatch);
    }
    if (status == INKPOD_STATUS_OK) {
        status = CaptureCoreState(core, undone);
    }
    const bool undo_ok = status == INKPOD_STATUS_OK
        && (undone.document.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        && undone.digest == source.digest;
    DocumentState redone{};
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_redo(core, &dispatch);
    }
    if (status == INKPOD_STATUS_OK) {
        status = CaptureCoreState(core, redone);
    }
    const bool redo_ok = status == INKPOD_STATUS_OK
        && (redone.document.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        && (redone.editor.flags & INKPOD_EDITOR_STATE_DIRTY) == 0U
        && redone.digest == opened.digest;
    std::uint64_t next_guide{};
    if (status == INKPOD_STATUS_OK) {
        status = inkpod_core_guide_add(
            core, INKPOD_GUIDE_HORIZONTAL, 3, &dispatch, &next_guide);
    }
    const InkpodStatus destroy = inkpod_core_destroy(&core);
    const bool valid = clean_open && undo_ok && redo_ok
        && status == INKPOD_STATUS_OK
        && next_guide == result.next_stable_id
        && destroy == INKPOD_STATUS_OK;
    if (!valid) {
        std::fprintf(
            stderr,
            "native validation status=%u clean=%u undo=%u redo=%u "
            "format=%u strategy=%u doc_flags=%u editor_flags=%u "
            "revision=%llu expected_revision=%llu history=%llu expected_history=%llu "
            "redone_doc_flags=%u redone_editor_flags=%u redone_digest=%u "
            "next=%llu expected_next=%llu destroy=%u\n",
            status,
            clean_open ? 1U : 0U,
            undo_ok ? 1U : 0U,
            redo_ok ? 1U : 0U,
            persistence.format_version,
            persistence.open_strategy,
            opened.document.flags,
            opened.editor.flags,
            static_cast<unsigned long long>(opened.document.document_revision),
            static_cast<unsigned long long>(result.final_revision),
            static_cast<unsigned long long>(opened.history.item_count),
            static_cast<unsigned long long>(source.history.item_count + 1U),
            redone.document.flags,
            redone.editor.flags,
            std::equal(
                redone.digest.begin(),
                redone.digest.end(),
                opened.digest.begin()) ? 1U : 0U,
            static_cast<unsigned long long>(next_guide),
            static_cast<unsigned long long>(result.next_stable_id),
            destroy);
    }
    return valid;
}

int TestSuccessAndNativeContracts() {
    Fixture fixture(1U);
    if (!fixture.Ready()) return 10;
    DocumentState before{};
    std::uint64_t event_id{};
    if (!fixture.AddGuideAndCaptureLastEvent(event_id, before)) return 11;
    InkScriptEngineRequest request = Request(
        1U,
        fixture.Command(),
        CurrentDocumentSource(0U, "success"),
        {fixture.Directory()});
    request.export_event_ids.push_back(event_id);
    const auto enqueue_started = std::chrono::steady_clock::now();
    if (!fixture.Host().EnqueueInkScript(std::move(request))
        || std::chrono::steady_clock::now() - enqueue_started
            >= std::chrono::milliseconds(100)) return 12;
    CoreNotification plan{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 1U,
            InkScriptEngineNotificationKind::PlanReady, plan)
        || plan.status != INKPOD_STATUS_OK
        || plan.inkscript.total_items != 1U) return 13;
    const auto confirm_started = std::chrono::steady_clock::now();
    if (!fixture.Host().ConfirmInkScript(
            1U, fixture.Command(), INKPOD_INKSCRIPT_SCOPE_CURRENT_DOCUMENT)
        || std::chrono::steady_clock::now() - confirm_started
            >= std::chrono::milliseconds(100)) return 14;
    CoreNotification completed{};
    const std::wstring output = fixture.Directory() + L"\\success_0001.inkpod";
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 1U,
            InkScriptEngineNotificationKind::Completed, completed)
        || completed.status != INKPOD_STATUS_OK
        || completed.inkscript.outcome != INKPOD_INKSCRIPT_OUTCOME_INSTALLED
        || completed.inkscript.failure != INKPOD_INKSCRIPT_FAILURE_NONE
        || completed.inkscript.report_item_count != 1U
        || completed.inkscript.exported_commit_count != 1U
        || completed.inkscript.exported_text_bytes == 0U
        || completed.inkscript.owner_thread_id == GetCurrentThreadId()
        || GetFileAttributesW(output.c_str()) == INVALID_FILE_ATTRIBUTES) {
        std::fprintf(
            stderr,
            "success result status=%u outcome=%u failure=%u items=%llu "
            "exports=%llu bytes=%llu owner=%u host_op=%u host_status=%u file=%lu\n",
            completed.status,
            completed.inkscript.outcome,
            completed.inkscript.failure,
            static_cast<unsigned long long>(
                completed.inkscript.report_item_count),
            static_cast<unsigned long long>(
                completed.inkscript.exported_commit_count),
            static_cast<unsigned long long>(
                completed.inkscript.exported_text_bytes),
            completed.inkscript.owner_thread_id,
            completed.inkscript.last_host_operation,
            completed.inkscript.last_host_status,
            GetFileAttributesW(output.c_str()));
        return 15;
    }
    DocumentState after{};
    if (!fixture.Capture(after) || !SameState(before, after)) return 16;
    if (!ValidateNativeOutput(output, before, completed.inkscript)) return 17;
    if (fixture.Host().ConfirmInkScript(
            1U, fixture.Command(), INKPOD_INKSCRIPT_SCOPE_CURRENT_DOCUMENT)
        || fixture.Host().CancelInkScript(1U, fixture.Command())
        || fixture.Sink().Submissions() != 0U) return 18;
    return 0;
}

int TestInvalidCancelResourceAndSaveFailure() {
    Fixture fixture(2U);
    if (!fixture.Ready()) return 20;
    DocumentState before{};
    if (!fixture.Capture(before)) return 21;
    std::string invalid = CurrentDocumentSource(0U, "invalid");
    invalid.replace(0U, std::string("inkscript 2").size(), "inkscript 1");
    if (!fixture.Host().EnqueueInkScript(Request(
            20U, fixture.Command(), std::move(invalid), {fixture.Directory()}))) return 22;
    CoreNotification invalid_result{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 20U,
            InkScriptEngineNotificationKind::Completed, invalid_result)
        || invalid_result.status != INKPOD_STATUS_INVALID_ARGUMENT
        || GetFileAttributesW(
               (fixture.Directory() + L"\\invalid_0001.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES) return 23;

    if (!fixture.Host().EnqueueInkScript(Request(
            21U,
            fixture.Command(),
            CurrentDocumentSource(0U, "cancel"),
            {fixture.Directory()}))) return 24;
    CoreNotification cancel_plan{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 21U,
            InkScriptEngineNotificationKind::PlanReady, cancel_plan)) return 25;
    if (fixture.Host().EnqueueInkScript(Request(
            21U,
            fixture.Command(),
            CurrentDocumentSource(0U, "duplicate"),
            {fixture.Directory()}))) return 26;
    const auto cancel_started = std::chrono::steady_clock::now();
    if (!fixture.Host().CancelInkScript(21U, fixture.Command())
        || std::chrono::steady_clock::now() - cancel_started
            >= std::chrono::milliseconds(100)) return 27;
    CoreNotification cancelled{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 21U,
            InkScriptEngineNotificationKind::Completed, cancelled)
        || cancelled.status != INKPOD_STATUS_CANCELLED
        || fixture.Host().CancelInkScript(21U, fixture.Command())
        || GetFileAttributesW(
               (fixture.Directory() + L"\\cancel_0001.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES) return 28;

    InkScriptEngineRequest resource = Request(
        22U,
        fixture.Command(),
        CurrentDocumentSource(0U, "resource"),
        {fixture.Directory()});
    resource.maximum_output_bytes = 1U;
    if (!fixture.Host().EnqueueInkScript(std::move(resource))) return 29;
    CoreNotification resource_plan{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 22U,
            InkScriptEngineNotificationKind::PlanReady, resource_plan)
        || !fixture.Host().ConfirmInkScript(
            22U, fixture.Command(), INKPOD_INKSCRIPT_SCOPE_CURRENT_DOCUMENT)) return 30;
    CoreNotification resource_result{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 22U,
            InkScriptEngineNotificationKind::Completed, resource_result)
        || resource_result.status != INKPOD_STATUS_IO_ERROR
        || resource_result.inkscript.outcome != INKPOD_INKSCRIPT_OUTCOME_FAILED
        || resource_result.inkscript.failure != INKPOD_INKSCRIPT_FAILURE_RESOURCE
        || GetFileAttributesW(
               (fixture.Directory() + L"\\resource_0001.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES) return 31;

    std::wstring overwrite_directory;
    if (!fixture.MakeDirectory(L"save-failure", overwrite_directory)) return 32;
    const auto overwrite_path = overwrite_directory + L"\\input.inkpod";
    if (!CreateNative(overwrite_path, 230U)) return 32;
    if (!fixture.Host().EnqueueInkScript(Request(
            23U,
            fixture.Command(),
            OverwriteFolderSource(),
            {overwrite_path, overwrite_path}))) return 33;
    CoreNotification save_plan{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 23U,
            InkScriptEngineNotificationKind::PlanReady, save_plan)) return 34;
    const HANDLE save_blocker = CreateFileW(
        overwrite_path.c_str(),
        GENERIC_READ,
        FILE_SHARE_READ,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (save_blocker == INVALID_HANDLE_VALUE
        || !fixture.Host().ConfirmInkScript(
            23U, fixture.Command(), INKPOD_INKSCRIPT_SCOPE_ALL)) {
        if (save_blocker != INVALID_HANDLE_VALUE) CloseHandle(save_blocker);
        return 34;
    }
    CoreNotification save_result{};
    const bool received_save_result = WaitForNotification(
            fixture.Owner(), fixture.Host(), 23U,
            InkScriptEngineNotificationKind::Completed, save_result);
    CloseHandle(save_blocker);
    if (!received_save_result || save_result.status != INKPOD_STATUS_IO_ERROR
        || save_result.inkscript.outcome != INKPOD_INKSCRIPT_OUTCOME_FAILED
        || save_result.inkscript.failure != INKPOD_INKSCRIPT_FAILURE_SAVE) {
        std::fprintf(
            stderr,
            "save failure mismatch: status=%u outcome=%u failure=%u operation=%u host=%u diagnostic=%s\n",
            static_cast<unsigned>(save_result.status),
            static_cast<unsigned>(save_result.inkscript.outcome),
            static_cast<unsigned>(save_result.inkscript.failure),
            static_cast<unsigned>(save_result.inkscript.last_host_operation),
            static_cast<unsigned>(save_result.inkscript.last_host_status),
            reinterpret_cast<const char*>(
                save_result.inkscript.diagnostic_utf8.data()));
        return 35;
    }

    CommandContext stale = fixture.Command();
    stale.generation = Generation(fixture.SessionGeneration().Value() + 1U);
    const auto stale_started = std::chrono::steady_clock::now();
    if (fixture.Host().EnqueueInkScript(Request(
            24U,
            stale,
            CurrentDocumentSource(0U, "stale"),
            {fixture.Directory()}))
        || std::chrono::steady_clock::now() - stale_started
            >= std::chrono::milliseconds(100)
        || fixture.Host().ConfirmInkScript(
            999U, fixture.Command(), INKPOD_INKSCRIPT_SCOPE_CURRENT_DOCUMENT)
        || fixture.Host().CancelInkScript(999U, fixture.Command())) return 36;
    DocumentState after{};
    if (!fixture.Capture(after) || !SameState(before, after)
        || fixture.Sink().Submissions() != 0U) return 37;
    return 0;
}

int TestOverflowWaitAndClose() {
    Fixture fixture(3U);
    if (!fixture.Ready()) return 40;
    DocumentState before{};
    if (!fixture.Capture(before)) return 41;
    std::wstring input_directory;
    std::wstring output_directory;
    if (!fixture.MakeDirectory(L"input", input_directory)
        || !fixture.MakeDirectory(L"output", output_directory)
        || !CreateNative(input_directory + L"\\a.inkpod", 301U)
        || !CreateNative(input_directory + L"\\b.inkpod", 302U)) return 42;

    if (!fixture.Host().EnqueueInkScript(Request(
            30U,
            fixture.Command(),
            FolderSource(0U, "overflow", UINT64_C(18446744073709551615)),
            {input_directory, output_directory}))) return 43;
    CoreNotification overflow_result{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 30U,
            InkScriptEngineNotificationKind::Completed, overflow_result)
        || overflow_result.status != INKPOD_STATUS_INVALID_ARGUMENT
        || GetFileAttributesW(
               (output_directory + L"\\overflow_0001.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES) return 44;

    if (!fixture.Host().EnqueueInkScript(Request(
            31U,
            fixture.Command(),
            FolderSource(500U, "wait"),
            {input_directory, output_directory}))) return 45;
    CoreNotification wait_plan{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 31U,
            InkScriptEngineNotificationKind::PlanReady, wait_plan)
        || wait_plan.inkscript.total_items != 2U) return 46;
    const auto confirm_started = std::chrono::steady_clock::now();
    if (!fixture.Host().ConfirmInkScript(
            31U, fixture.Command(), INKPOD_INKSCRIPT_SCOPE_ALL)
        || std::chrono::steady_clock::now() - confirm_started
            >= std::chrono::milliseconds(100)) return 47;
    CoreNotification wait_event{};
    if (!WaitForProgressEvent(
            fixture.Owner(), fixture.Host(), 31U,
            INKPOD_INKSCRIPT_EVENT_WAIT_REQUESTED, wait_event)
        || wait_event.inkscript.wait_milliseconds != 500U) return 48;
    std::promise<std::uint32_t> owner_promise;
    auto owner_future = owner_promise.get_future();
    if (!fixture.Host().Enqueue(
            fixture.Command(),
            [&owner_promise](InkpodCore*) {
                owner_promise.set_value(GetCurrentThreadId());
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false)
        || owner_future.wait_for(std::chrono::milliseconds(200))
            != std::future_status::ready) return 49;
    const std::uint32_t engine_thread = owner_future.get();
    CoreNotification wait_result{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 31U,
            InkScriptEngineNotificationKind::Completed, wait_result)
        || wait_result.status != INKPOD_STATUS_OK
        || wait_result.inkscript.report_item_count != 2U
        || wait_result.inkscript.owner_thread_id != engine_thread
        || GetFileAttributesW(
               (output_directory + L"\\wait_0001.inkpod").c_str())
            == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(
               (output_directory + L"\\wait_0002.inkpod").c_str())
            == INVALID_FILE_ATTRIBUTES) return 50;
    DocumentState after_wait{};
    if (!fixture.Capture(after_wait) || !SameState(before, after_wait)
        || fixture.Sink().Submissions() != 0U) return 51;

    if (!fixture.Host().EnqueueInkScript(Request(
            32U,
            fixture.Command(),
            CurrentDocumentSource(0U, "close"),
            {fixture.Directory()}))) return 52;
    CoreNotification close_plan{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 32U,
            InkScriptEngineNotificationKind::PlanReady, close_plan)) return 53;
    auto close_future = std::async(std::launch::async, [&fixture] {
        return fixture.Host().CloseSession(
            fixture.Session(), fixture.SessionGeneration());
    });
    CoreNotification close_result{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 32U,
            InkScriptEngineNotificationKind::Completed, close_result,
            std::chrono::seconds(2))
        || close_result.status != INKPOD_STATUS_CANCELLED
        || close_future.wait_for(std::chrono::seconds(1))
            != std::future_status::ready
        || close_future.get() != INKPOD_STATUS_OK
        || GetFileAttributesW(
               (fixture.Directory() + L"\\close_0001.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES) return 54;
    return 0;
}

int TestShutdownRace() {
    Fixture fixture(4U);
    if (!fixture.Ready()) return 60;
    if (!fixture.Host().EnqueueInkScript(Request(
            40U,
            fixture.Command(),
            CurrentDocumentSource(0U, "shutdown"),
            {fixture.Directory()}))) return 61;
    CoreNotification plan{};
    if (!WaitForNotification(
            fixture.Owner(), fixture.Host(), 40U,
            InkScriptEngineNotificationKind::PlanReady, plan)) return 62;
    const auto started = std::chrono::steady_clock::now();
    fixture.Host().Stop();
    if (std::chrono::steady_clock::now() - started >= std::chrono::seconds(1)
        || GetFileAttributesW(
               (fixture.Directory() + L"\\shutdown_0001.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES) return 63;
    return 0;
}

int TestQueueSaturation() {
    Fixture fixture(5U);
    if (!fixture.Ready()) return 70;
    std::promise<void> entered_promise;
    auto entered = entered_promise.get_future();
    std::promise<void> release_promise;
    const std::shared_future<void> release = release_promise.get_future().share();
    if (!fixture.Host().Enqueue(
            fixture.Command(),
            [&entered_promise, release](InkpodCore*) {
                entered_promise.set_value();
                release.wait();
                return INKPOD_STATUS_OK;
            },
            false,
            false,
            false)
        || entered.wait_for(std::chrono::seconds(1)) != std::future_status::ready) {
        release_promise.set_value();
        return 71;
    }
    std::uint64_t accepted{};
    for (; accepted < 5000U; ++accepted) {
        if (!fixture.Host().Enqueue(
                fixture.Command(),
                [](InkpodCore*) { return INKPOD_STATUS_OK; },
                false,
                false,
                false)) break;
    }
    const auto route_started = std::chrono::steady_clock::now();
    const bool route_accepted = fixture.Host().EnqueueInkScript(Request(
        50U,
        fixture.Command(),
        CurrentDocumentSource(0U, "saturated"),
        {fixture.Directory()}));
    const auto route_elapsed = std::chrono::steady_clock::now() - route_started;
    release_promise.set_value();
    if (accepted < 4000U || route_accepted
        || route_elapsed >= std::chrono::milliseconds(100)) return 72;
    InkpodStatus idle = INKPOD_STATUS_QUEUE_FULL;
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
    do {
        idle = fixture.Host().WaitIdle(
            fixture.Session(), fixture.SessionGeneration());
        if (idle == INKPOD_STATUS_OK) break;
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    } while (std::chrono::steady_clock::now() < deadline);
    inkpod::app::CoreSessionState state{};
    const auto metrics = fixture.Host().Metrics();
    if (idle != INKPOD_STATUS_OK
        || !fixture.Host().GetSessionState(
            fixture.Session(), fixture.SessionGeneration(), state)
        || state.pending_operations != 0U
        || metrics.rejected_work_items < 2U
        || GetFileAttributesW(
               (fixture.Directory() + L"\\saturated_0001.inkpod").c_str())
            != INVALID_FILE_ATTRIBUTES) return 73;
    return 0;
}

}  // namespace

int wmain() {
    for (const auto test : {
             &TestSuccessAndNativeContracts,
             &TestInvalidCancelResourceAndSaveFailure,
             &TestOverflowWaitAndClose,
             &TestShutdownRace,
             &TestQueueSaturation}) {
        const int result = test();
        if (result != 0) {
            std::fprintf(
                stderr, "InkScript engine route test failed: %d\n", result);
            return result;
        }
    }
    return 0;
}
