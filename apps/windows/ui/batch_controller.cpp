#include "ui/localization.h"

#include "batch_controller.h"

#include <shlobj.h>

#include <algorithm>
#include <array>
#include <climits>
#include <cstdint>
#include <new>
#include <utility>
#include <vector>

#include "app/frontend_state.h"
#include "ui/main_window.h"
#include "app/core_host.h"
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

InkpodFilterInput FilterInputFor(const app::FilterJob& job) noexcept {
    InkpodFilterInput input{};
    input.struct_size = sizeof(input);
    input.kind = job.kind;
    input.plane_id = job.plane_id;
    input.channel = job.channel;
    input.interpolation = job.interpolation;
    input.parameter_0 = job.parameters[0];
    input.parameter_1 = job.parameters[1];
    input.parameter_2 = job.parameters[2];
    input.parameter_3 = job.parameters[3];
    input.parameter_4 = job.parameters[4];
    if (!job.points.empty()) {
        input.points = job.points.data();
        input.point_count = job.points.size();
        input.point_stride_bytes = sizeof(InkpodCurvePoint);
    }
    return input;
}

void FillOperationInput(
    const app::BatchOperationUi& source,
    InkpodFilterInput& filter,
    InkpodBatchOperationInput& destination) noexcept {
    destination = {};
    destination.struct_size = sizeof(destination);
    destination.version = INKPOD_BATCH_GRAPH_VERSION;
    destination.kind = source.kind;
    destination.flags = source.flags;
    destination.layer_id = source.layer_id;
    destination.plane_id = source.plane_id;
    destination.layer_kind = source.layer_kind;
    destination.plane_kind = source.plane_kind;
    destination.missing_policy = source.missing_policy;
    std::copy(
        source.parameters.begin(),
        source.parameters.end(),
        std::begin(destination.parameters));
    destination.color_0 = source.color_0;
    destination.color_1 = source.color_1;
    destination.colors.struct_size = sizeof(destination.colors);
    destination.colors.colors = source.colors.empty()
        ? nullptr
        : source.colors.data();
    destination.colors.color_count = source.colors.size();
    destination.colors.color_stride_bytes = sizeof(InkpodColorValue);
    if (source.kind == INKPOD_BATCH_OPERATION_FILTER) {
        filter = FilterInputFor(source.filter);
        destination.filter = &filter;
    }
    destination.color_pairs = source.color_pairs.empty()
        ? nullptr
        : source.color_pairs.data();
    destination.color_pair_count = source.color_pairs.size();
    destination.color_pair_stride_bytes = sizeof(InkpodBatchColorPairInput);
    destination.seeds = source.seeds.empty() ? nullptr : source.seeds.data();
    destination.seed_count = source.seeds.size();
    destination.seed_stride_bytes = sizeof(InkpodBatchSeedInput);
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
        || info.seed_count > 4'096U || info.curve_point_count > 4'096U) {
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
        std::copy(
            std::begin(info.parameters),
            std::end(info.parameters),
            operation.parameters.begin());
        operation.color_0 = info.color_0;
        operation.color_1 = info.color_1;
        operation.filter.kind = info.filter_kind;
        operation.filter.channel = info.filter_channel;
        operation.filter.interpolation = info.filter_interpolation;
        std::copy(
            std::begin(info.filter_parameters),
            std::end(info.filter_parameters),
            operation.filter.parameters.begin());
        operation.colors.resize(static_cast<std::size_t>(info.color_count));
        operation.color_pairs.resize(
            static_cast<std::size_t>(info.color_pair_count));
        operation.seeds.resize(static_cast<std::size_t>(info.seed_count));
        operation.filter.points.resize(
            static_cast<std::size_t>(info.curve_point_count));
        operation.label = UiText(UiStringId::Text0925);
    } catch (const std::bad_alloc&) {
        return false;
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
    for (std::size_t row = 0U; row < operation.seeds.size(); ++row) {
        operation.seeds[row].struct_size = sizeof(InkpodBatchSeedInput);
        if (inkpod_batch_graph_get_operation_seed(
                graph, index, row, &operation.seeds[row])
            != INKPOD_STATUS_OK) {
            return false;
        }
    }
    for (std::size_t row = 0U; row < operation.filter.points.size(); ++row) {
        operation.filter.points[row].struct_size = sizeof(InkpodCurvePoint);
        if (inkpod_batch_graph_get_operation_curve_point(
                graph, index, row, &operation.filter.points[row])
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
    const auto& source_operations = batch_.run_operations.empty()
        ? batch_.operations
        : batch_.run_operations;
    if (source_operations.empty()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        std::vector<InkpodFilterInput> filters(source_operations.size());
        std::vector<InkpodBatchOperationInput> operations(source_operations.size());
        for (std::size_t index = 0; index < source_operations.size(); ++index) {
            FillOperationInput(
                source_operations[index], filters[index], operations[index]);
        }
        if (batch_.loaded_graph && batch_.graph != nullptr) {
            if (batch_.run_operations.empty()) {
                return INKPOD_STATUS_OK;
            }
            InkpodBatchGraph* run_graph{};
            const InkpodStatus status = inkpod_batch_graph_clone_with_operations(
                batch_.graph,
                operations.data(),
                operations.size(),
                sizeof(InkpodBatchOperationInput),
                &run_graph);
            if (status == INKPOD_STATUS_OK) {
                inkpod_batch_graph_release(&batch_.run_graph);
                batch_.run_graph = run_graph;
            }
            return status;
        }
        inkpod_batch_graph_release(&batch_.run_graph);
        std::vector<std::uint8_t> input_path;
        std::vector<std::uint8_t> output_folder;
        std::vector<std::uint8_t> basename;
        if ((!batch_.input_path.empty()
                && !WidePathToUtf8(batch_.input_path, input_path))
            || (!batch_.output_folder.empty()
                && !WidePathToUtf8(batch_.output_folder, output_folder))
            || (!batch_.basename.empty()
                && !WidePathToUtf8(batch_.basename, basename))) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        InkpodBatchInput batch_input{};
        batch_input.struct_size = sizeof(batch_input);
        batch_input.kind = batch_.input_kind;
        batch_input.path_utf8 = input_path.empty() ? nullptr : input_path.data();
        batch_input.path_bytes = input_path.size();
        batch_input.first_cell = batch_.first_cell;
        batch_input.last_cell = batch_.last_cell;

        static constexpr std::array<std::uint8_t, 17U> graph_name{
            'W','i','n','d','o','w','s',' ','B','a','t','c','h',' ','S','e','t'};
        InkpodBatchGraphInput input{};
        input.struct_size = sizeof(input);
        input.version = INKPOD_BATCH_GRAPH_VERSION;
        input.name_utf8 = graph_name.data();
        input.name_bytes = graph_name.size();
        input.inputs = &batch_input;
        input.input_count = 1U;
        input.input_stride_bytes = sizeof(batch_input);
        input.operations = operations.data();
        input.operation_count = operations.size();
        input.operation_stride_bytes = sizeof(InkpodBatchOperationInput);
        input.output_policy = batch_.output_policy;
        input.failure_policy = batch_.failure_policy;
        input.output_flags = (batch_.cell_folder
                                  ? INKPOD_BATCH_OUTPUT_CELL_FOLDER
                                  : 0U)
            | (batch_.descending ? INKPOD_BATCH_OUTPUT_DESCENDING : 0U)
            | (batch_.preview_before_save
                   ? INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE
                   : 0U);
        input.output_folder_utf8 = output_folder.empty()
            ? nullptr
            : output_folder.data();
        input.output_folder_bytes = output_folder.size();
        input.basename_utf8 = basename.empty() ? nullptr : basename.data();
        input.basename_bytes = basename.size();
        input.start_number = batch_.start_number;
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

InkpodStatus BatchController::Preview(
    const app::CommandContext& context,
    InkpodBatchRunScope scope) noexcept {
    if (!context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus graph_status = BuildGraph();
    batch_.run_operations.clear();
    if (graph_status != INKPOD_STATUS_OK) {
        return graph_status;
    }
    inkpod_batch_preview_release(&batch_.preview);
    app::BatchUiState* const batch = &batch_;
    InkpodStatus status = engine_.Invoke(
        *context.document_session,
        *context.generation,
        [batch, scope](InkpodCore* core) {
            const InkpodBatchGraph* graph = batch->run_graph != nullptr
                ? batch->run_graph
                : batch->graph;
            return inkpod_core_batch_preview(
                core, graph, scope, &batch->preview);
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
    batch_.run_operations.clear();
    if (graph_status != INKPOD_STATUS_OK) {
        return graph_status;
    }
    bool preview_confirmed = !batch_.preview_before_save || dry_run;
    if (!preview_confirmed) {
        const InkpodStatus preview_status = Preview(context, scope);
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
                const InkpodBatchGraph* graph = batch->run_graph != nullptr
                    ? batch->run_graph
                    : batch->graph;
                return inkpod_core_batch_execute(
                    core,
                    graph,
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
                const InkpodBatchGraph* graph = batch->run_graph != nullptr
                    ? batch->run_graph
                    : batch->graph;
                return inkpod_core_batch_execute(
                    core,
                    graph,
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
        || info.operation_count == 0U || info.operation_count > 1'024U) {
        inkpod_batch_graph_release(&loaded);
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<app::BatchOperationUi> operations;
    try {
        operations.resize(static_cast<std::size_t>(info.operation_count));
    } catch (const std::bad_alloc&) {
        inkpod_batch_graph_release(&loaded);
        return INKPOD_STATUS_INVALID_STATE;
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
    batch_.operations = std::move(operations);
    batch_.run_operations.clear();
    batch_.selected_operation = 0U;
    batch_.loaded_graph = true;
    batch_.output_policy = info.output_policy;
    batch_.failure_policy = info.failure_policy;
    batch_.cell_folder =
        (info.output_flags & INKPOD_BATCH_OUTPUT_CELL_FOLDER) != 0U;
    batch_.descending =
        (info.output_flags & INKPOD_BATCH_OUTPUT_DESCENDING) != 0U;
    batch_.preview_before_save =
        (info.output_flags & INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE) != 0U;
    return INKPOD_STATUS_OK;
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
        view.target_text = batch.target_text;
        view.job_text = batch.job_text;
        view.target_available = batch.target_available;
        view.pinned = batch.target_pinned;
        if (batch.task != nullptr && batch.job_id.has_value()) {
            ProgressDialogInfo progress{};
            if (QueryProgress(&batch, progress)) {
                view.job_text = L"Job "
                    + std::to_wstring(batch.job_id->Value()) + L" — "
                    + std::to_wstring(progress.completed_work) + L" / "
                    + std::to_wstring(progress.total_work);
            }
        }
        if (batch.input_kind == INKPOD_BATCH_INPUT_CURRENT_SEQUENCE) {
            view.input_label = UiText(UiStringId::Text0790);
        } else if (batch.input_kind == INKPOD_BATCH_INPUT_FOLDER) {
            view.input_label = UiText(UiStringId::Text0303) + batch.input_path;
        } else {
            view.input_label = UiText(UiStringId::Text0281) + batch.input_path;
        }
        if (batch.first_cell != 0U || batch.last_cell != 0U) {
            view.input_label += UiText(UiStringId::Text0010);
            view.input_label += batch.first_cell == 0U
                ? UiText(UiStringId::Text0491)
                : std::to_wstring(batch.first_cell);
            view.input_label += UiText(UiStringId::RangeSeparator);
            view.input_label += batch.last_cell == 0U
                ? UiText(UiStringId::Text0747)
                : std::to_wstring(batch.last_cell);
        }

        view.loaded_graph = batch.loaded_graph;
        if (batch.loaded_graph && batch.graph != nullptr) {
            InkpodBatchGraphInfo info{};
            info.struct_size = sizeof(info);
            if (inkpod_batch_graph_get_info(batch.graph, &info) == INKPOD_STATUS_OK) {
                view.operation_labels.push_back(
                    UiText(UiStringId::Text0924)
                    + std::to_wstring(info.operation_count) + UiText(UiStringId::Text0014));
            }
        } else {
            view.operation_labels.reserve(batch.operations.size());
            for (const auto& operation : batch.operations) {
                std::wstring label = operation.flags & INKPOD_BATCH_OPERATION_ENABLED
                    ? L"✓ "
                    : L"– ";
                label += operation.label;
                if (operation.flags & INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN) {
                    label += UiText(UiStringId::Text1046);
                }
                view.operation_labels.push_back(std::move(label));
            }
            if (!batch.operations.empty()) {
                batch.selected_operation = std::min<std::uint32_t>(
                    batch.selected_operation,
                    static_cast<std::uint32_t>(batch.operations.size() - 1U));
                view.selected_operation = batch.selected_operation;
            }
        }

        const wchar_t* policy = UiText(UiStringId::Text0904);
        if (batch.output_policy == INKPOD_BATCH_OUTPUT_NEW_SAVE) {
            policy = UiText(UiStringId::Text0716);
        } else if (batch.output_policy == INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE) {
            policy = UiText(UiStringId::Text0727);
        }
        view.output_text = UiText(UiStringId::Text0515);
        view.output_text += policy;
        view.output_text += L" / ";
        view.output_text += batch.output_folder.empty()
            ? UiText(UiStringId::Text1045)
            : batch.output_folder;
        if (!batch.last_result.empty()) {
            view.output_text += L"\r\n";
            view.output_text += batch.last_result;
        }
        view.idle = batch.task == nullptr;
        view.runnable = view.idle
            && (batch.graph != nullptr || !batch.operations.empty());
    } catch (const std::bad_alloc&) {
        return;
    }
    UpdateBatchPaletteDialog(palette, view);
}

void BatchController::ResetDerivedState(app::BatchUiState& batch) noexcept {
    inkpod_batch_preview_release(&batch.preview);
    inkpod_batch_report_release(&batch.report);
    inkpod_batch_graph_release(&batch.graph);
    inkpod_batch_graph_release(&batch.run_graph);
    batch.loaded_graph = false;
    batch.last_result.clear();
}

std::wstring BatchController::ReportSummary(const InkpodBatchReport* report) {
    InkpodBatchReportInfo info{};
    info.struct_size = sizeof(info);
    if (report == nullptr
        || inkpod_batch_report_get_info(report, &info) != INKPOD_STATUS_OK) {
        return UiText(UiStringId::Text0409);
    }
    return UiText(UiStringId::Text0844) + std::to_wstring(info.item_count) + UiText(UiStringId::Text0454)
        + std::to_wstring(info.failure_count)
        + (info.cancelled != 0U ? UiText(UiStringId::Text0007) : L"");
}

bool BatchController::ChooseFolder(
    HWND owner, std::wstring& selected_path) noexcept {
    BROWSEINFOW browse{};
    browse.hwndOwner = owner;
    browse.lpszTitle = UiText(UiStringId::Text0261);
    browse.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
    PIDLIST_ABSOLUTE item = SHBrowseForFolderW(&browse);
    if (item == nullptr) {
        return false;
    }
    std::array<wchar_t, MAX_PATH> path{};
    const BOOL resolved = SHGetPathFromIDListW(item, path.data());
    CoTaskMemFree(item);
    if (resolved == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

} // namespace inkpod::windows::ui
