#include "ui/localization.h"

#include "batch_controller.h"

#include <algorithm>
#include <array>
#include <climits>
#include <cstdint>
#include <new>
#include <string_view>
#include <utility>
#include <vector>

#include "app/frontend_state.h"
#include "ui/main_window.h"
#include "app/core_host.h"
#include "batch_input_picker.h"
#include "batch_set_store.h"
#include "dialogs/batch_dialog.h"

namespace inkpod::windows::ui {

namespace {

bool WidePathToUtf8(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept {
    if (path.empty() || path.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int required = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        path.data(),
        static_cast<int>(path.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               path.data(),
               static_cast<int>(path.size()),
               reinterpret_cast<char*>(output.data()),
               required,
               nullptr,
               nullptr)
        == required;
}

bool Utf8ToWide(
    const std::uint8_t* text,
    std::uint64_t bytes,
    std::wstring& output) noexcept {
    if (bytes == 0U) {
        output.clear();
        return true;
    }
    if (text == nullptr || bytes > static_cast<std::uint64_t>(INT_MAX)) {
        return false;
    }
    const int required = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(text),
        static_cast<int>(bytes),
        nullptr,
        0);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               reinterpret_cast<const char*>(text),
               static_cast<int>(bytes),
               output.data(),
               required)
        == required;
}

constexpr std::uint64_t kMaximumDisplayedBatchFailures = 8U;
constexpr std::size_t kMaximumDisplayedBatchInputCharacters = 128U;
constexpr std::size_t kMaximumDisplayedBatchDiagnosticCharacters = 256U;

std::wstring TruncateBatchReportText(
    std::wstring_view text, std::size_t maximum_characters) {
    if (text.size() <= maximum_characters) {
        return std::wstring(text);
    }
    std::size_t length = maximum_characters;
    if (length > 0U
        && text[length - 1U] >= static_cast<wchar_t>(0xd800U)
        && text[length - 1U] <= static_cast<wchar_t>(0xdbffU)) {
        --length;
    }
    std::wstring truncated(text.substr(0U, length));
    truncated += L"...";
    return truncated;
}

std::wstring BatchFailureReasonText(std::wstring_view diagnostic) {
    if (diagnostic.find(L"batch stable target does not exist")
            != std::wstring_view::npos
        || diagnostic.find(L"batch plane target does not exist")
            != std::wstring_view::npos) {
        return UiText(UiStringId::BatchFailureTargetMissing);
    }
    if (diagnostic.find(L"hidden or non-editable")
        != std::wstring_view::npos) {
        return UiText(UiStringId::BatchFailureTargetUnavailable);
    }
    if (diagnostic.find(L"pixel value does not match the raster pixel format")
            != std::wstring_view::npos
        || diagnostic.find(L"raster contracts do not match")
            != std::wstring_view::npos) {
        return UiText(UiStringId::BatchFailureFormatMismatch);
    }
    if (diagnostic.empty()) {
        return UiText(UiStringId::BatchFailureDetailsUnavailable);
    }
    return UiText(UiStringId::BatchFailureTechnicalDetails)
        + TruncateBatchReportText(
            diagnostic, kMaximumDisplayedBatchDiagnosticCharacters);
}

void AppendBatchFailureLine(
    std::wstring& output,
    std::wstring_view input_name,
    std::wstring_view diagnostic) {
    output += L"\r\n- ";
    output += input_name.empty()
        ? UiText(UiStringId::Text0496)
        : TruncateBatchReportText(
              input_name, kMaximumDisplayedBatchInputCharacters);
    output += L": ";
    output += BatchFailureReasonText(diagnostic);
}

const wchar_t* OperationKindLabel(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_BATCH_OPERATION_COLOR_REPLACE:
            return UiText(UiStringId::ToolColorReplacement);
        case INKPOD_BATCH_OPERATION_MOVE_TO_COLOR_PLANE:
            return UiText(UiStringId::BatchMoveToColorPlane);
        case INKPOD_BATCH_OPERATION_MASKING:
            return UiText(UiStringId::BatchMasking);
        case INKPOD_BATCH_OPERATION_ERASE:
            return UiText(UiStringId::BatchErase);
        default:
            return nullptr;
    }
}

void FillOperationInput(
    const app::BatchOperationUi& source,
    InkpodBatchOperationInput& destination) noexcept {
    destination = {};
    destination.struct_size = sizeof(destination);
    destination.version = INKPOD_BATCH_OPERATION_VERSION;
    destination.kind = source.kind;
    destination.flags = source.flags;
    destination.layer_id = source.layer_id;
    destination.plane_id = source.plane_id;
    destination.layer_kind = source.layer_kind;
    destination.plane_kind = source.plane_kind;
    destination.missing_policy = source.missing_policy;
    destination.colors.struct_size = sizeof(destination.colors);
    destination.colors.reserved = 0U;
    destination.colors.feature_flags = INKPOD_FEATURE_NONE;
    destination.colors.colors = source.colors.empty()
        ? nullptr
        : source.colors.data();
    destination.colors.color_count = source.colors.size();
    destination.colors.color_stride_bytes = sizeof(InkpodColorValue);
    destination.color_pairs = source.color_pairs.empty()
        ? nullptr
        : source.color_pairs.data();
    destination.color_pair_count = source.color_pairs.size();
    destination.color_pair_stride_bytes = sizeof(InkpodBatchColorPairInput);
    destination.additional_targets = source.additional_targets.empty()
        ? nullptr
        : source.additional_targets.data();
    destination.additional_target_count = source.additional_targets.size();
    destination.additional_target_stride_bytes = sizeof(InkpodBatchTargetInput);
}

bool ReadOperation(
    const InkpodBatchGraph* graph,
    std::uint64_t index,
    app::BatchOperationUi& operation) noexcept {
    InkpodBatchOperationInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_batch_graph_get_operation(graph, index, &info)
        != INKPOD_STATUS_OK
        || info.color_count > 4'096U || info.color_pair_count > 4'096U
        || info.target_count == 0U || info.target_count > 64U) {
        return false;
    }
    try {
        operation = {};
        operation.kind = info.kind;
        operation.flags = info.flags;
        operation.layer_id = info.layer_id;
        operation.plane_id = info.plane_id;
        operation.layer_kind = info.layer_kind;
        operation.plane_kind = info.plane_kind;
        operation.missing_policy = info.missing_policy;
        operation.additional_targets.resize(
            static_cast<std::size_t>(info.target_count - 1U));
        operation.colors.resize(static_cast<std::size_t>(info.color_count));
        operation.color_pairs.resize(
            static_cast<std::size_t>(info.color_pair_count));
        const wchar_t* const label = OperationKindLabel(operation.kind);
        if (label == nullptr) {
            return false;
        }
        operation.label = label;
    } catch (const std::bad_alloc&) {
        return false;
    }
    for (std::size_t target_index = 0U;
         target_index < operation.additional_targets.size(); ++target_index) {
        auto& target = operation.additional_targets[target_index];
        target.struct_size = sizeof(target);
        if (inkpod_batch_graph_get_operation_target(
                graph, index, target_index + 1U, &target)
            != INKPOD_STATUS_OK) {
            return false;
        }
    }
    for (std::size_t row = 0U; row < operation.colors.size(); ++row) {
        operation.colors[row].struct_size = sizeof(InkpodColorValue);
        if (inkpod_batch_graph_get_operation_color(
                graph, index, row, &operation.colors[row])
            != INKPOD_STATUS_OK) {
            return false;
        }
    }
    for (std::size_t row = 0U; row < operation.color_pairs.size(); ++row) {
        operation.color_pairs[row].struct_size =
            sizeof(InkpodBatchColorPairInput);
        if (inkpod_batch_graph_get_operation_color_pair(
                graph, index, row, &operation.color_pairs[row])
            != INKPOD_STATUS_OK) {
            return false;
        }
    }
    return true;
}

} // namespace

BatchController::BatchController(
    app::AppLifetimeState& lifetime,
    app::MainWindowHandles& windows,
    HWND progress,
    JobProgressPaneState& progress_state,
    HWND& palette,
    app::BatchUiState& batch,
    app::CoreHost& engine) noexcept
    : lifetime_(lifetime),
      windows_(windows),
      progress_(progress),
      progress_state_(progress_state),
      palette_(palette),
      batch_(batch),
      engine_(engine) {}

InkpodStatus BatchController::BuildGraph() noexcept {
    if (batch_.inputs.empty() || batch_.operations.empty()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        std::vector<InkpodBatchOperationInput> operations(batch_.operations.size());
        for (std::size_t index = 0; index < batch_.operations.size(); ++index) {
            FillOperationInput(batch_.operations[index], operations[index]);
        }

        inkpod_batch_graph_release(&batch_.run_graph);
        std::vector<std::vector<std::uint8_t>> input_paths(batch_.inputs.size());
        std::vector<InkpodBatchInput> inputs(batch_.inputs.size());
        std::vector<std::uint8_t> output_folder;
        std::vector<std::uint8_t> naming_template;
        std::vector<std::uint8_t> graph_name;
        for (std::size_t index = 0U; index < batch_.inputs.size(); ++index) {
            const auto& source = batch_.inputs[index];
            if (!source.path.empty()
                && !WidePathToUtf8(source.path, input_paths[index])) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            auto& destination = inputs[index];
            destination.struct_size = sizeof(destination);
            destination.kind = source.kind;
            destination.feature_flags = INKPOD_FEATURE_NONE;
            destination.path_utf8 = input_paths[index].empty()
                ? nullptr
                : input_paths[index].data();
            destination.path_bytes = input_paths[index].size();
            destination.first_cell = source.first_cell;
            destination.last_cell = source.last_cell;
        }
        if ((!batch_.output_folder.empty()
                && !WidePathToUtf8(batch_.output_folder, output_folder))
            || (!batch_.naming_template.empty()
                && !WidePathToUtf8(batch_.naming_template, naming_template))
            || !WidePathToUtf8(batch_.set_name, graph_name)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }

        InkpodBatchGraphInput input{};
        input.struct_size = sizeof(input);
        input.version = INKPOD_BATCH_GRAPH_VERSION;
        input.feature_flags = INKPOD_FEATURE_NONE;
        input.name_utf8 = graph_name.data();
        input.name_bytes = graph_name.size();
        input.inputs = inputs.data();
        input.input_count = inputs.size();
        input.input_stride_bytes = sizeof(InkpodBatchInput);
        input.operations = operations.data();
        input.operation_count = operations.size();
        input.operation_stride_bytes = sizeof(InkpodBatchOperationInput);
        input.output_destination = batch_.output_destination;
        input.failure_policy = batch_.failure_policy;
        input.output_flags = batch_.preview_before_save
            ? INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE
            : 0U;
        input.output_folder_utf8 = output_folder.empty()
            ? nullptr
            : output_folder.data();
        input.output_folder_bytes = output_folder.size();
        input.naming_template_utf8 = naming_template.empty()
            ? nullptr
            : naming_template.data();
        input.naming_template_bytes = naming_template.size();
        input.output_format = batch_.output_format;
        input.wait_milliseconds = batch_.wait_milliseconds;
        InkpodBatchGraph* graph{};
        const InkpodStatus status = inkpod_batch_graph_create(&input, &graph);
        if (status == INKPOD_STATUS_OK) {
            inkpod_batch_graph_release(&batch_.graph);
            batch_.graph = graph;
        }
        return status;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodStatus BatchController::PlanPreview(
    const app::CommandContext& context,
    InkpodBatchRunScope scope) noexcept {
    if (!context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus graph_status = BuildGraph();
    if (graph_status != INKPOD_STATUS_OK) {
        return graph_status;
    }
    inkpod_batch_preview_release(&batch_.preview);
    app::BatchUiState* const batch = &batch_;
    InkpodStatus status = engine_.Invoke(
        *context.document_session,
        *context.generation,
        [batch, scope](InkpodCore* core) {
            return inkpod_core_batch_preview(
                core, batch->graph, scope, &batch->preview);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        std::uint64_t count{};
        std::uint64_t warnings{};
        status = inkpod_batch_preview_count(batch_.preview, &count);
        for (std::uint64_t index = 0U;
             status == INKPOD_STATUS_OK && index < count;
             ++index) {
            InkpodBatchPreviewItem item{};
            item.struct_size = sizeof(item);
            status = inkpod_batch_preview_get(batch_.preview, index, &item);
            if (status == INKPOD_STATUS_OK
                && (item.flags & INKPOD_BATCH_PREVIEW_HAS_WARNING) != 0U) {
                ++warnings;
            }
        }
        if (status == INKPOD_STATUS_OK) {
            try {
                batch_.last_result = UiText(UiStringId::Text0313) + std::to_wstring(count)
                    + UiText(UiStringId::Text0455) + std::to_wstring(warnings);
            } catch (const std::bad_alloc&) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
        }
    }
    RefreshPalette(batch_, palette_);
    return status;
}

InkpodStatus BatchController::Start(
    const app::CommandContext& context,
    InkpodBatchRunScope scope,
    bool dry_run,
    UINT completion_message) noexcept {
    if (batch_.task != nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus graph_status = BuildGraph();
    if (graph_status != INKPOD_STATUS_OK) {
        return graph_status;
    }
    if (!dry_run
        && batch_.output_destination == INKPOD_BATCH_OUTPUT_NEW_TABS) {
        if (!context.document_session.has_value()
            || !context.generation.has_value()) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        InkpodBatchPreview* capacity_preview{};
        InkpodStatus capacity_status = engine_.Invoke(
            *context.document_session,
            *context.generation,
            [this, scope, &capacity_preview](InkpodCore* core) {
                return inkpod_core_batch_preview(
                    core, batch_.graph, scope, &capacity_preview);
            },
            false,
            false);
        std::uint64_t result_count{};
        if (capacity_status == INKPOD_STATUS_OK) {
            capacity_status = inkpod_batch_preview_count(
                capacity_preview, &result_count);
        }
        inkpod_batch_preview_release(&capacity_preview);
        if (capacity_status != INKPOD_STATUS_OK) {
            return capacity_status;
        }
        const std::size_t existing = engine_.SessionCount();
        if (existing > app::CoreHost::kMaximumDocumentSessions
            || result_count
                > app::CoreHost::kMaximumDocumentSessions - existing) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    bool preview_confirmed = !batch_.preview_before_save || dry_run;
    if (!preview_confirmed) {
        const InkpodStatus preview_status = PlanPreview(context, scope);
        if (preview_status != INKPOD_STATUS_OK) {
            return preview_status;
        }
        preview_confirmed = lifetime_.smoke_test
            || MessageBoxW(
                   windows_.window,
                   UiText(UiStringId::Text0883),
                   UiText(UiStringId::Text0260),
                   MB_OKCANCEL | MB_ICONQUESTION) == IDOK;
        if (!preview_confirmed) {
            return INKPOD_STATUS_CANCELLED;
        }
    }

    inkpod_batch_report_release(&batch_.report);
    InkpodStatus status = inkpod_batch_task_create(&batch_.task);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    const std::uint64_t flags = (dry_run ? INKPOD_BATCH_RUN_DRY : 0U)
        | (preview_confirmed ? INKPOD_BATCH_RUN_PREVIEW_CONFIRMED : 0U);
    app::BatchUiState* const batch = &batch_;
    if (lifetime_.smoke_test) {
        if (!context.document_session.has_value()
            || !context.generation.has_value()) {
            inkpod_batch_task_release(&batch_.task);
            return INKPOD_STATUS_INVALID_STATE;
        }
        status = engine_.Invoke(
            *context.document_session,
            *context.generation,
            [batch, scope, flags](InkpodCore* core) {
                return inkpod_core_batch_execute(
                    core,
                    batch->graph,
                    scope,
                    flags,
                    batch->task,
                    &batch->report);
            },
            true,
            true);
        if (batch_.report != nullptr) {
            try {
                batch_.last_result = ReportSummary(batch_.report);
            } catch (const std::bad_alloc&) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
        }
        inkpod_batch_task_release(&batch_.task);
        RefreshPalette(batch_, palette_);
        return status;
    }

    batch_.progress_dialog = {
        &batch_,
        QueryProgress,
        CancelProgress,
        UiText(UiStringId::Text0267),
        UiText(UiStringId::Text0263),
        UiText(UiStringId::Cancelling)};
    if (!BindJobProgress(
            progress_,
            progress_state_,
            JobProgressSlot::Batch,
            batch_.progress_dialog)) {
        inkpod_batch_task_release(&batch_.task);
        return INKPOD_STATUS_INVALID_STATE;
    }
    static_cast<void>(windows_.dock_host.RestorePane(DockPaneType::JobProgress));
    static_cast<void>(windows_.dock_host.ActivatePane(DockPaneType::JobProgress));
    batch_.completion_context = context;
    const HWND window = windows_.window;
    if (!engine_.Enqueue(
            context,
            [batch, scope, flags](InkpodCore* core) {
                return inkpod_core_batch_execute(
                    core,
                    batch->graph,
                    scope,
                    flags,
                    batch->task,
                    &batch->report);
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
        ClearJobProgress(progress_, progress_state_, JobProgressSlot::Batch);
        if (!HasActiveJobProgress(progress_state_)) {
            static_cast<void>(windows_.dock_host.HidePane(
                DockPaneType::JobProgress));
        }
        inkpod_batch_task_release(&batch_.task);
        batch_.completion_context = {};
        return INKPOD_STATUS_INVALID_STATE;
    }
    return INKPOD_STATUS_OK;
}

InkpodStatus BatchController::StartContactSheetPreview(
    const app::CommandContext& context,
    UINT completion_message) noexcept {
    if (batch_.task != nullptr
        || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus graph_status = BuildGraph();
    if (graph_status != INKPOD_STATUS_OK) {
        return graph_status;
    }
    if (engine_.SessionCount() >= app::CoreHost::kMaximumDocumentSessions) {
        return INKPOD_STATUS_INVALID_STATE;
    }

    inkpod_batch_report_release(&batch_.report);
    InkpodStatus status = inkpod_batch_task_create(&batch_.task);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    app::BatchUiState* const batch = &batch_;
    if (lifetime_.smoke_test) {
        status = engine_.Invoke(
            *context.document_session,
            *context.generation,
            [batch](InkpodCore* core) {
                return inkpod_core_batch_contact_sheet_preview(
                    core,
                    batch->graph,
                    batch->task,
                    &batch->report);
            },
            false,
            false);
        if (batch_.report != nullptr) {
            try {
                batch_.last_result = ReportSummary(batch_.report);
            } catch (const std::bad_alloc&) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
        }
        inkpod_batch_task_release(&batch_.task);
        RefreshPalette(batch_, palette_);
        return status;
    }

    batch_.progress_dialog = {
        &batch_,
        QueryProgress,
        CancelProgress,
        UiText(UiStringId::Text0259),
        UiText(UiStringId::Text0263),
        UiText(UiStringId::Cancelling)};
    if (!BindJobProgress(
            progress_,
            progress_state_,
            JobProgressSlot::Batch,
            batch_.progress_dialog)) {
        inkpod_batch_task_release(&batch_.task);
        return INKPOD_STATUS_INVALID_STATE;
    }
    static_cast<void>(windows_.dock_host.RestorePane(DockPaneType::JobProgress));
    static_cast<void>(windows_.dock_host.ActivatePane(DockPaneType::JobProgress));
    batch_.completion_context = context;
    const HWND window = windows_.window;
    if (!engine_.Enqueue(
            context,
            [batch](InkpodCore* core) {
                return inkpod_core_batch_contact_sheet_preview(
                    core,
                    batch->graph,
                    batch->task,
                    &batch->report);
            },
            false,
            false,
            true,
            [window, completion_message, context](InkpodStatus completion_status) {
                const LPARAM generation = context.generation.has_value()
                    ? static_cast<LPARAM>(context.generation->Value())
                    : 0;
                PostMessageW(
                    window, completion_message, completion_status, generation);
            })) {
        ClearJobProgress(progress_, progress_state_, JobProgressSlot::Batch);
        if (!HasActiveJobProgress(progress_state_)) {
            static_cast<void>(windows_.dock_host.HidePane(
                DockPaneType::JobProgress));
        }
        inkpod_batch_task_release(&batch_.task);
        batch_.completion_context = {};
        return INKPOD_STATUS_INVALID_STATE;
    }
    return INKPOD_STATUS_OK;
}

InkpodStatus BatchController::SaveGraph(
    const std::uint8_t* path_utf8, std::size_t path_bytes) noexcept {
    const InkpodStatus graph_status = BuildGraph();
    if (graph_status != INKPOD_STATUS_OK) {
        return graph_status;
    }
    return inkpod_batch_graph_save(batch_.graph, path_utf8, path_bytes);
}

InkpodStatus BatchController::LoadGraph(
    const std::uint8_t* path_utf8, std::size_t path_bytes) noexcept {
    InkpodBatchGraph* loaded{};
    const InkpodStatus status = inkpod_batch_graph_load(
        path_utf8, path_bytes, &loaded);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    InkpodBatchGraphInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_batch_graph_get_info(loaded, &info) != INKPOD_STATUS_OK
        || info.input_count == 0U || info.input_count > 16'384U
        || info.operation_count == 0U || info.operation_count > 1'024U) {
        inkpod_batch_graph_release(&loaded);
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<app::BatchInputUi> inputs;
    std::vector<app::BatchOperationUi> operations;
    std::wstring set_name;
    std::wstring output_folder;
    std::wstring naming_template;
    try {
        inputs.resize(static_cast<std::size_t>(info.input_count));
        operations.resize(static_cast<std::size_t>(info.operation_count));
    } catch (const std::bad_alloc&) {
        inkpod_batch_graph_release(&loaded);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!Utf8ToWide(info.name_utf8, info.name_bytes, set_name)
        || !Utf8ToWide(
            info.output_folder_utf8,
            info.output_folder_bytes,
            output_folder)
        || !Utf8ToWide(
            info.naming_template_utf8,
            info.naming_template_bytes,
            naming_template)) {
        inkpod_batch_graph_release(&loaded);
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    for (std::size_t index = 0U; index < inputs.size(); ++index) {
        InkpodBatchInput input{};
        input.struct_size = sizeof(input);
        if (inkpod_batch_graph_get_input(loaded, index, &input)
                != INKPOD_STATUS_OK
            || !Utf8ToWide(
                input.path_utf8, input.path_bytes, inputs[index].path)) {
            inkpod_batch_graph_release(&loaded);
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        inputs[index].kind = input.kind;
        inputs[index].first_cell = input.first_cell;
        inputs[index].last_cell = input.last_cell;
    }
    for (std::size_t index = 0U; index < operations.size(); ++index) {
        if (!ReadOperation(loaded, index, operations[index])) {
            inkpod_batch_graph_release(&loaded);
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
    }
    inkpod_batch_preview_release(&batch_.preview);
    inkpod_batch_report_release(&batch_.report);
    inkpod_batch_graph_release(&batch_.run_graph);
    inkpod_batch_graph_release(&batch_.graph);
    batch_.graph = loaded;
    batch_.set_name = std::move(set_name);
    batch_.inputs = std::move(inputs);
    batch_.operations = std::move(operations);
    batch_.selected_stage = 0U;
    batch_.selected_operation = 0U;
    batch_.output_destination = info.output_destination;
    batch_.output_format = info.output_format;
    batch_.failure_policy = info.failure_policy;
    batch_.output_folder = std::move(output_folder);
    batch_.naming_template = std::move(naming_template);
    batch_.wait_milliseconds = info.wait_milliseconds;
    batch_.preview_before_save =
        (info.output_flags & INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE) != 0U;
    return INKPOD_STATUS_OK;
}

InkpodStatus BatchController::SaveStoredGraph() noexcept {
    std::wstring path;
    if (lifetime_.smoke_test) {
        path = L"inkpod-batch-ui-smoke.inkbatch";
    } else {
        std::wstring directory;
        std::wstring canonical_name;
        if (!PrepareDefaultBatchSetDirectory(directory)
            || !ResolveBatchSetPath(
                directory, batch_.set_name, path, &canonical_name)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            batch_.set_name = std::move(canonical_name);
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    std::vector<std::uint8_t> path_utf8;
    if (!WidePathToUtf8(path, path_utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodStatus status = SaveGraph(path_utf8.data(), path_utf8.size());
    if (status == INKPOD_STATUS_OK && !lifetime_.smoke_test) {
        static_cast<void>(RefreshSetCatalog(batch_));
    }
    return status;
}

InkpodStatus BatchController::LoadStoredGraph() noexcept {
    std::wstring path;
    if (lifetime_.smoke_test) {
        path = L"inkpod-batch-ui-smoke.inkbatch";
    } else {
        std::wstring directory;
        if (!PrepareDefaultBatchSetDirectory(directory)
            || !ResolveBatchSetPath(directory, batch_.set_name, path)) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
    }
    std::vector<std::uint8_t> path_utf8;
    if (!WidePathToUtf8(path, path_utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodStatus status = LoadGraph(path_utf8.data(), path_utf8.size());
    if (status == INKPOD_STATUS_OK && !lifetime_.smoke_test) {
        static_cast<void>(RefreshSetCatalog(batch_));
    }
    return status;
}

bool BatchController::QueryProgress(
    void* context, ProgressDialogInfo& output) noexcept {
    auto* batch = static_cast<app::BatchUiState*>(context);
    if (batch == nullptr || batch->task == nullptr) {
        return false;
    }
    InkpodTaskInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_batch_task_query(batch->task, &info) != INKPOD_STATUS_OK) {
        return false;
    }
    output.completed_work = info.completed_work;
    output.total_work = info.total_work;
    return true;
}

void BatchController::CancelProgress(void* context) noexcept {
    auto* batch = static_cast<app::BatchUiState*>(context);
    if (batch != nullptr && batch->task != nullptr) {
        inkpod_batch_task_cancel(batch->task);
    }
}

void BatchController::RefreshPalette(
    app::BatchUiState& batch, HWND palette) noexcept {
    if (palette == nullptr) {
        return;
    }
    BatchPaletteView view{};
    try {
        view.job_text = batch.job_text;
        view.set_name = batch.set_name;
        view.set_names = batch.available_set_names;
        if (batch.task != nullptr && batch.job_id.has_value()) {
            ProgressDialogInfo progress{};
            if (QueryProgress(&batch, progress)) {
                view.job_text = L"Job "
                    + std::to_wstring(batch.job_id->Value()) + L" — "
                    + std::to_wstring(progress.completed_work) + L" / "
                    + std::to_wstring(progress.total_work);
            }
        }
        view.stage_labels.reserve(batch.operations.size() + 2U);
        std::array<wchar_t, 96U> input_label{};
        _snwprintf_s(
            input_label.data(),
            input_label.size(),
            _TRUNCATE,
            UiText(UiStringId::BatchInputCountFormat),
            batch.inputs.size());
        view.stage_labels.emplace_back(input_label.data());
        bool any_enabled = false;
        for (auto& operation : batch.operations) {
            const wchar_t* const localized_label =
                OperationKindLabel(operation.kind);
            if (localized_label == nullptr) {
                return;
            }
            operation.label = localized_label;
            const bool enabled =
                (operation.flags & INKPOD_BATCH_OPERATION_ENABLED) != 0U;
            any_enabled = any_enabled || enabled;
            view.stage_labels.push_back(operation.label);
        }
        view.stage_labels.push_back(UiText(UiStringId::BatchOutput));
        batch.selected_stage = std::min<std::uint32_t>(
            batch.selected_stage,
            static_cast<std::uint32_t>(view.stage_labels.size() - 1U));
        view.selected_stage = batch.selected_stage;
        if (batch.selected_stage > 0U
            && batch.selected_stage <= batch.operations.size()) {
            batch.selected_operation = batch.selected_stage - 1U;
        }

        bool valid = true;
        if (batch.inputs.empty()) {
            view.validation_text = UiText(UiStringId::BatchInputRequired);
            valid = false;
        } else if (batch.operations.empty()) {
            view.validation_text = UiText(UiStringId::BatchOperationRequired);
            valid = false;
        } else if (!any_enabled) {
            view.validation_text =
                UiText(UiStringId::BatchEnabledOperationRequired);
            valid = false;
        } else if (batch.output_destination == INKPOD_BATCH_OUTPUT_FOLDER
                   && batch.output_folder.empty()) {
            view.validation_text =
                UiText(UiStringId::BatchOutputFolderRequired);
            valid = false;
        } else {
            view.validation_text = batch.validation_text;
            valid = view.validation_text.empty();
        }
        if (!batch.last_result.empty()) {
            if (!view.validation_text.empty()) {
                view.validation_text += L"\r\n";
            }
            view.validation_text += batch.last_result;
        }
        view.idle = batch.task == nullptr;
        view.runnable = view.idle && valid;
    } catch (const std::bad_alloc&) {
        return;
    }
    UpdateBatchPaletteDialog(palette, view);
}

bool BatchController::RefreshSetCatalog(app::BatchUiState& batch) noexcept {
    std::wstring directory;
    std::vector<std::wstring> names;
    if (!PrepareDefaultBatchSetDirectory(directory)
        || !EnumerateBatchSetNames(directory, names)) {
        return false;
    }
    batch.available_set_names.swap(names);
    return true;
}

void BatchController::ResetDerivedState(app::BatchUiState& batch) noexcept {
    inkpod_batch_preview_release(&batch.preview);
    inkpod_batch_report_release(&batch.report);
    inkpod_batch_graph_release(&batch.graph);
    inkpod_batch_graph_release(&batch.run_graph);
    batch.last_result.clear();
}

std::wstring BatchController::ReportSummary(const InkpodBatchReport* report) {
    InkpodBatchReportInfo info{};
    info.struct_size = sizeof(info);
    if (report == nullptr
        || inkpod_batch_report_get_info(report, &info) != INKPOD_STATUS_OK) {
        return UiText(UiStringId::Text0409);
    }
    std::wstring result = UiText(UiStringId::Text0844)
        + std::to_wstring(info.item_count) + UiText(UiStringId::Text0454)
        + std::to_wstring(info.failure_count)
        + (info.cancelled != 0U ? UiText(UiStringId::Text0007) : L"");
    std::uint64_t displayed_failures{};
    for (std::uint64_t index = 0U;
         index < info.item_count
         && displayed_failures < kMaximumDisplayedBatchFailures;
         ++index) {
        InkpodBatchReportItem item{};
        item.struct_size = sizeof(item);
        if (inkpod_batch_report_get(report, index, &item)
            != INKPOD_STATUS_OK) {
            AppendBatchFailureLine(result, {}, {});
            break;
        }
        if (item.outcome != INKPOD_BATCH_ITEM_FAILED) {
            continue;
        }
        std::wstring input_name;
        std::wstring diagnostic;
        if (!Utf8ToWide(item.input_name, item.input_name_bytes, input_name)
            || !Utf8ToWide(item.message, item.message_bytes, diagnostic)) {
            diagnostic.clear();
        }
        AppendBatchFailureLine(result, input_name, diagnostic);
        ++displayed_failures;
    }
    if (info.failure_count > displayed_failures) {
        std::array<wchar_t, 96U> additional{};
        _snwprintf_s(
            additional.data(),
            additional.size(),
            _TRUNCATE,
            UiText(UiStringId::BatchAdditionalFailuresFormat),
            static_cast<unsigned long long>(
                info.failure_count - displayed_failures));
        result += L"\r\n";
        result += additional.data();
    }
    return result;
}

bool BatchController::ChooseFolder(
    HWND owner, std::wstring& selected_path) noexcept {
    return ChooseBatchFolder(
        owner, UiText(UiStringId::Text0261), selected_path);
}

} // namespace inkpod::windows::ui
