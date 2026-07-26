#include "batch_controller.h"

#include <shlobj.h>

#include <algorithm>
#include <array>
#include <climits>
#include <cstdint>
#include <new>
#include <utility>
#include <vector>

#include "app/app_context.h"
#include "app/core_engine.h"
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

InkpodFilterInput FilterInputFor(const app::M6FilterJob& job) noexcept {
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

} // namespace

BatchController::BatchController(
    app::AppLifetimeState& lifetime,
    app::MainWindowHandles& windows,
    app::BatchUiState& batch,
    app::CoreEngine& engine) noexcept
    : lifetime_(lifetime), windows_(windows), batch_(batch), engine_(engine) {}

InkpodStatus BatchController::BuildGraph() noexcept {
    if (batch_.loaded_graph && batch_.graph != nullptr) {
        return INKPOD_STATUS_OK;
    }
    if (batch_.operations.empty()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
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

        std::vector<InkpodFilterInput> filters(batch_.operations.size());
        std::vector<InkpodBatchOperationInput> operations(batch_.operations.size());
        for (std::size_t index = 0; index < batch_.operations.size(); ++index) {
            const app::BatchOperationUi& source = batch_.operations[index];
            InkpodBatchOperationInput& destination = operations[index];
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
                filters[index] = FilterInputFor(source.filter);
                destination.filter = &filters[index];
            }
            destination.color_pairs = source.color_pairs.empty()
                ? nullptr
                : source.color_pairs.data();
            destination.color_pair_count = source.color_pairs.size();
            destination.color_pair_stride_bytes =
                sizeof(InkpodBatchColorPairInput);
            destination.seeds = source.seeds.empty()
                ? nullptr
                : source.seeds.data();
            destination.seed_count = source.seeds.size();
            destination.seed_stride_bytes = sizeof(InkpodBatchSeedInput);
        }
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

InkpodStatus BatchController::Preview(InkpodBatchRunScope scope) noexcept {
    const InkpodStatus graph_status = BuildGraph();
    if (graph_status != INKPOD_STATUS_OK) {
        return graph_status;
    }
    inkpod_batch_preview_release(&batch_.preview);
    app::BatchUiState* const batch = &batch_;
    InkpodStatus status = engine_.Invoke(
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
                batch_.last_result = L"プレビュー: " + std::to_wstring(count)
                    + L"件 / 警告 " + std::to_wstring(warnings);
            } catch (const std::bad_alloc&) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
        }
    }
    RefreshPalette(batch_);
    return status;
}

InkpodStatus BatchController::Start(
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
    bool preview_confirmed = !batch_.preview_before_save || dry_run;
    if (!preview_confirmed) {
        const InkpodStatus preview_status = Preview(scope);
        if (preview_status != INKPOD_STATUS_OK) {
            return preview_status;
        }
        preview_confirmed = lifetime_.smoke_test
            || MessageBoxW(
                   windows_.window,
                   L"表示した入力・出力・警告の内容で保存を続行しますか？",
                   L"バッチ保存前プレビュー",
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
        status = engine_.Invoke(
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
        RefreshPalette(batch_);
        return status;
    }

    batch_.progress_dialog = {
        &batch_,
        QueryProgress,
        CancelProgress,
        L"バッチ実行",
        L"バッチ処理中...",
        L"キャンセル中..."};
    batch_.progress = CreateProgressDialog(
        lifetime_.instance, windows_.window, batch_.progress_dialog);
    if (batch_.progress == nullptr) {
        inkpod_batch_task_release(&batch_.task);
        return INKPOD_STATUS_INVALID_STATE;
    }
    ShowWindow(batch_.progress, SW_SHOW);
    const HWND window = windows_.window;
    if (!engine_.Enqueue(
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
            [window, completion_message](InkpodStatus completion_status) {
                PostMessageW(window, completion_message, completion_status, 0);
            })) {
        DestroyWindow(batch_.progress);
        batch_.progress = nullptr;
        inkpod_batch_task_release(&batch_.task);
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
    inkpod_batch_preview_release(&batch_.preview);
    inkpod_batch_report_release(&batch_.report);
    inkpod_batch_graph_release(&batch_.graph);
    batch_.graph = loaded;
    batch_.loaded_graph = true;
    InkpodBatchGraphInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_batch_graph_get_info(loaded, &info) == INKPOD_STATUS_OK) {
        batch_.output_policy = info.output_policy;
        batch_.failure_policy = info.failure_policy;
        batch_.cell_folder =
            (info.output_flags & INKPOD_BATCH_OUTPUT_CELL_FOLDER) != 0U;
        batch_.descending =
            (info.output_flags & INKPOD_BATCH_OUTPUT_DESCENDING) != 0U;
        batch_.preview_before_save =
            (info.output_flags & INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE) != 0U;
    }
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

void BatchController::RefreshPalette(app::BatchUiState& batch) noexcept {
    if (batch.palette == nullptr) {
        return;
    }
    BatchPaletteView view{};
    try {
        if (batch.input_kind == INKPOD_BATCH_INPUT_CURRENT_SEQUENCE) {
            view.input_label = L"現在セルを含む連番（自然順）";
        } else if (batch.input_kind == INKPOD_BATCH_INPUT_FOLDER) {
            view.input_label = L"フォルダー: " + batch.input_path;
        } else {
            view.input_label = L"ファイル: " + batch.input_path;
        }
        if (batch.first_cell != 0U || batch.last_cell != 0U) {
            view.input_label += L" / 範囲 ";
            view.input_label += batch.first_cell == 0U
                ? L"先頭"
                : std::to_wstring(batch.first_cell);
            view.input_label += L"～";
            view.input_label += batch.last_cell == 0U
                ? L"末尾"
                : std::to_wstring(batch.last_cell);
        }

        view.loaded_graph = batch.loaded_graph;
        if (batch.loaded_graph && batch.graph != nullptr) {
            InkpodBatchGraphInfo info{};
            info.struct_size = sizeof(info);
            if (inkpod_batch_graph_get_info(batch.graph, &info) == INKPOD_STATUS_OK) {
                view.operation_labels.push_back(
                    L"読み込み済みセット: "
                    + std::to_wstring(info.operation_count) + L" 項目");
            }
        } else {
            view.operation_labels.reserve(batch.operations.size());
            for (const auto& operation : batch.operations) {
                std::wstring label = operation.flags & INKPOD_BATCH_OPERATION_ENABLED
                    ? L"✓ "
                    : L"– ";
                label += operation.label;
                if (operation.flags & INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN) {
                    label += L"（実行ごとに設定）";
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

        const wchar_t* policy = L"複製保存";
        if (batch.output_policy == INKPOD_BATCH_OUTPUT_NEW_SAVE) {
            policy = L"新規保存";
        } else if (batch.output_policy == INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE) {
            policy = L"明示上書き";
        }
        view.output_text = L"出力: ";
        view.output_text += policy;
        view.output_text += L" / ";
        view.output_text += batch.output_folder.empty()
            ? L"（入力と同じ場所）"
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
    UpdateBatchPaletteDialog(batch.palette, view);
}

void BatchController::ResetDerivedState(app::BatchUiState& batch) noexcept {
    inkpod_batch_preview_release(&batch.preview);
    inkpod_batch_report_release(&batch.report);
    inkpod_batch_graph_release(&batch.graph);
    batch.loaded_graph = false;
    batch.last_result.clear();
}

std::wstring BatchController::ReportSummary(const InkpodBatchReport* report) {
    InkpodBatchReportInfo info{};
    info.struct_size = sizeof(info);
    if (report == nullptr
        || inkpod_batch_report_get_info(report, &info) != INKPOD_STATUS_OK) {
        return L"レポートを取得できません";
    }
    return L"結果: " + std::to_wstring(info.item_count) + L"件 / 失敗 "
        + std::to_wstring(info.failure_count)
        + (info.cancelled != 0U ? L" / キャンセル" : L"");
}

bool BatchController::ChooseFolder(
    HWND owner, std::wstring& selected_path) noexcept {
    BROWSEINFOW browse{};
    browse.hwndOwner = owner;
    browse.lpszTitle = L"バッチ入力フォルダーを選択";
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
