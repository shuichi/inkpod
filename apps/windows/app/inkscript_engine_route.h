#pragma once

#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "command_context.h"
#include "inkpod/core_ffi.h"

namespace inkpod::app {

// Immutable input captured by the UI/Input thread before the request enters
// the CoreHost lane. Authorized paths correspond one-for-one with the
// program's stable PathIntent order; the engine obtains access and intent IDs
// from the compiled program instead of trusting frontend copies.
struct InkScriptEngineRequest final {
    std::uint64_t job_id{};
    std::uint64_t controller_id{};
    std::uint64_t source_id{};
    std::uint64_t source_generation{};
    CommandContext context;
    std::string source_utf8;
    std::string current_document_label_utf8{"current.inkpod"};
    std::vector<std::wstring> authorized_paths;
    std::vector<std::uint64_t> export_event_ids;
    std::uint32_t run_mode{INKPOD_INKSCRIPT_RUN_INSTALL};
    std::uint64_t maximum_output_bytes{};
};

enum class InkScriptEngineNotificationKind : std::uint8_t {
    PlanReady,
    Progress,
    Completed,
};

enum class InkScriptEnginePhase : std::uint8_t {
    Initialize,
    ExportFragment,
    Parse,
    Compile,
    Authorize,
    PlanTaskCreate,
    Planning,
    AwaitingConfirmation,
    Running,
    Completed,
};

// Pointer-free value copied into the CoreHost notification queue. It never
// exposes an InkScript/Core/adapter owner to the UI thread.
struct InkScriptEngineResult final {
    std::uint64_t job_id{};
    InkScriptEngineNotificationKind kind{
        InkScriptEngineNotificationKind::Completed};
    InkScriptEnginePhase phase{InkScriptEnginePhase::Initialize};
    InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
    std::uint32_t event_kind{};
    std::uint32_t task_state{};
    std::uint64_t completed_items{};
    std::uint64_t total_items{};
    std::uint32_t wait_milliseconds{};
    std::uint32_t outcome{};
    std::uint32_t failure{};
    std::uint64_t report_flags{};
    std::uint64_t report_item_count{};
    std::uint64_t created_directory_count{};
    std::uint64_t final_revision{};
    std::uint64_t next_stable_id{};
    std::array<std::uint8_t, 32U> final_state_digest{};
    std::uint64_t exported_commit_count{};
    std::uint64_t exported_text_bytes{};
    std::array<std::uint8_t, 512U> diagnostic_utf8{};
    std::uint64_t diagnostic_bytes{};
    std::uint32_t last_host_operation{};
    InkpodStatus last_host_status{INKPOD_STATUS_OK};
    std::uint32_t owner_thread_id{};
};

enum class InkScriptEngineStepKind : std::uint8_t {
    Continue,
    PlanReady,
    Completed,
};

struct InkScriptEngineStep final {
    InkScriptEngineStepKind kind{InkScriptEngineStepKind::Completed};
    std::uint32_t delay_milliseconds{};
    bool has_notification{};
    InkScriptEngineResult notification;
};

// Created, advanced, and destroyed only on the CoreHost owner thread. The
// pimpl owns every Rust handle and the Windows file-authority adapter and
// releases them before the parent InkpodCore.
class InkScriptEngineTask final {
public:
    explicit InkScriptEngineTask(InkScriptEngineRequest request);
    ~InkScriptEngineTask();

    InkScriptEngineTask(const InkScriptEngineTask&) = delete;
    InkScriptEngineTask& operator=(const InkScriptEngineTask&) = delete;

    [[nodiscard]] InkScriptEngineStep Advance(
        InkpodCore* core,
        bool cancel_requested,
        std::uint32_t confirmation_scope) noexcept;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

}  // namespace inkpod::app
