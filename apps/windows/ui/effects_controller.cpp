#include "effects_controller.h"

#include "app/app_context.h"
#include "app/core_engine.h"
#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

EffectsController::EffectsController(
    app::AppLifetimeState& lifetime,
    app::MainWindowHandles& windows,
    app::EffectsUiState& effects,
    app::CoreEngine& engine) noexcept
    : lifetime_(lifetime), windows_(windows), effects_(effects), engine_(engine) {}

InkpodStatus EffectsController::StartTask(
    const app::CommandContext& context,
    bool preview_prompt,
    Operation operation,
    UINT completion_message) noexcept {
    if (effects_.task != nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodTask* task{};
    InkpodStatus status = inkpod_task_create(&task);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (lifetime_.smoke_test) {
        status = engine_.Invoke(
            [task, operation = std::move(operation)](InkpodCore* core) {
                return operation(core, task);
            },
            true,
            true);
        if (status == INKPOD_STATUS_OK && preview_prompt) {
            status = engine_.Invoke(
                [](InkpodCore* core) {
                    InkpodDispatchResult result{};
                    result.struct_size = sizeof(result);
                    return inkpod_core_filter_preview_apply(core, &result);
                },
                true,
                true);
        }
        inkpod_task_release(&task);
        return status;
    }

    effects_.task = task;
    effects_.completion_context = context;
    effects_.preview_prompt = preview_prompt;
    effects_.progress_dialog = {
        &effects_,
        QueryProgress,
        CancelProgress,
        nullptr,
        L"処理中...",
        L"キャンセル中..."};
    effects_.progress = CreateProgressDialog(
        lifetime_.instance, windows_.window, effects_.progress_dialog);
    if (effects_.progress == nullptr) {
        inkpod_task_release(&effects_.task);
        return INKPOD_STATUS_INVALID_STATE;
    }
    ShowWindow(effects_.progress, SW_SHOW);
    const HWND window = windows_.window;
    if (!engine_.Enqueue(
            context,
            [task, operation = std::move(operation)](InkpodCore* core) {
                return operation(core, task);
            },
            true,
            true,
            true,
            [window, completion_message, context](InkpodStatus completion_status) {
                const LPARAM generation = context.generation.has_value()
                    ? static_cast<LPARAM>(context.generation->Value())
                    : 0;
                PostMessageW(
                    window, completion_message, completion_status, generation);
            })) {
        DestroyWindow(effects_.progress);
        effects_.progress = nullptr;
        inkpod_task_release(&effects_.task);
        effects_.preview_prompt = false;
        effects_.completion_context = {};
        return INKPOD_STATUS_INVALID_STATE;
    }
    return INKPOD_STATUS_OK;
}

bool EffectsController::QueryProgress(
    void* context, ProgressDialogInfo& output) noexcept {
    auto* effects = static_cast<app::EffectsUiState*>(context);
    if (effects == nullptr || effects->task == nullptr) {
        return false;
    }
    InkpodTaskInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_task_query(effects->task, &info) != INKPOD_STATUS_OK) {
        return false;
    }
    output.completed_work = info.completed_work;
    output.total_work = info.total_work;
    return true;
}

void EffectsController::CancelProgress(void* context) noexcept {
    auto* effects = static_cast<app::EffectsUiState*>(context);
    if (effects != nullptr && effects->task != nullptr) {
        inkpod_task_cancel(effects->task);
    }
}

} // namespace inkpod::windows::ui
