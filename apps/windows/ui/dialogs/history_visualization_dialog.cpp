#include "history_visualization_dialog.h"

#include <commctrl.h>

#include <algorithm>
#include <array>
#include <cstdint>
#include <map>
#include <memory>
#include <mutex>
#include <new>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "app/application_host.h"
#include "app/document_session.h"
#include "app/resource.h"
#include "inkpod/core_ffi.h"
#include "ui/dialogs/effects_dialogs.h"

namespace inkpod::windows::ui {
namespace {

constexpr UINT kHistoryVisualizationStepCompleted = WM_APP + 0x174U;
constexpr UINT kHistoryVisualizationPrefetch = WM_APP + 0x175U;
constexpr std::size_t kMaximumCachedRows = 256U;
constexpr std::size_t kMaximumRowsPerPrefetch = 4U;
constexpr std::size_t kMaximumQueuedRows = 64U;
constexpr std::uint32_t kMaximumEventsPerStep = 1U;

bool IsNativeInkpodPath(std::wstring_view path) noexcept {
    constexpr std::wstring_view extension = L".inkpod";
    return path.size() >= extension.size()
        && CompareStringOrdinal(
               path.data() + path.size() - extension.size(),
               static_cast<int>(extension.size()),
               extension.data(),
               static_cast<int>(extension.size()),
               TRUE) == CSTR_EQUAL;
}

std::wstring LeafName(std::wstring_view path) {
    const std::size_t separator = path.find_last_of(L"\\/");
    return separator == std::wstring_view::npos
        ? std::wstring(path)
        : std::wstring(path.substr(separator + 1U));
}

std::wstring EscapeMenuText(std::wstring_view text) {
    std::wstring escaped;
    escaped.reserve(text.size());
    for (const wchar_t character : text) {
        escaped.push_back(character);
        if (character == L'&') {
            escaped.push_back(L'&');
        }
    }
    return escaped;
}

HMENU FindVisualizationSubmenu(HMENU menu) noexcept {
    if (menu == nullptr) {
        return nullptr;
    }
    const int count = GetMenuItemCount(menu);
    for (int position = 0; position < count; ++position) {
        std::array<wchar_t, 128U> text{};
        MENUITEMINFOW item{};
        item.cbSize = sizeof(item);
        item.fMask = MIIM_SUBMENU | MIIM_STRING;
        item.dwTypeData = text.data();
        item.cch = static_cast<UINT>(text.size() - 1U);
        if (GetMenuItemInfoW(menu, position, TRUE, &item) == FALSE
            || item.hSubMenu == nullptr) {
            continue;
        }
        if (std::wstring_view(text.data()) == L"Inkpodファイルの可視化") {
            return item.hSubMenu;
        }
        if (HMENU nested = FindVisualizationSubmenu(item.hSubMenu);
            nested != nullptr) {
            return nested;
        }
    }
    return nullptr;
}

struct MenuCandidate final {
    app::DocumentSession* document{};
    std::wstring leaf;
};

std::wstring Utf8ToWide(const std::vector<std::uint8_t>& bytes) {
    if (bytes.empty()) {
        return {};
    }
    const int byte_count = static_cast<int>(bytes.size());
    const int length = MultiByteToWideChar(
        CP_UTF8, MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(bytes.data()), byte_count,
        nullptr, 0);
    if (length <= 0) {
        return L"(invalid UTF-8)";
    }
    std::wstring result(static_cast<std::size_t>(length), L'\0');
    if (MultiByteToWideChar(
            CP_UTF8, MB_ERR_INVALID_CHARS,
            reinterpret_cast<const char*>(bytes.data()), byte_count,
            result.data(), length) != length) {
        return L"(invalid UTF-8)";
    }
    return result;
}

struct CachedRow final {
    std::wstring primitive_name;
    std::wstring arguments;
    std::uint32_t width{};
    std::uint32_t height{};
    std::vector<std::uint8_t> bgra;
};

struct VisualizationLoad final {
    ~VisualizationLoad() {
        (void)inkpod_history_visualization_builder_release(&builder, task);
        (void)inkpod_history_visualization_release(&handle);
        (void)inkpod_task_release(&task);
    }

    void Cancel() noexcept {
        if (task != nullptr) {
            (void)inkpod_task_cancel(task);
        }
    }

    std::mutex mutex;
    HWND dialog{};
    InkpodTask* task{};
    InkpodHistoryVisualizationBuilder* builder{};
    InkpodHistoryVisualization* handle{};
    InkpodHistoryVisualizationProgress progress{};
    InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
    bool finished{};
    bool progress_bound{};
};

bool QueryVisualizationProgress(
    void* context, ProgressDialogInfo& output) noexcept {
    const auto* load = static_cast<const VisualizationLoad*>(context);
    if (load == nullptr || load->task == nullptr) {
        return false;
    }
    InkpodTaskInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_task_query(load->task, &info) != INKPOD_STATUS_OK) {
        return false;
    }
    output.completed_work = info.completed_work;
    output.total_work = info.total_work;
    return true;
}

void CancelVisualizationProgress(void* context) noexcept {
    if (auto* load = static_cast<VisualizationLoad*>(context); load != nullptr) {
        load->Cancel();
    }
}

class HistoryVisualizationController final {
public:
    HistoryVisualizationController(
        app::ApplicationHost& application,
        app::DocumentSessionId session,
        app::Generation generation,
        std::wstring display_name) noexcept
        : application_(&application),
          session_(session),
          generation_(generation),
          display_name_(std::move(display_name)) {}

    ~HistoryVisualizationController() {
        if (load_) {
            load_->Cancel();
            ClearProgress();
        }
        if (image_list_ != nullptr) {
            ImageList_Destroy(image_list_);
        }
        (void)inkpod_history_visualization_release(&visualization_);
    }

    void Attach(HWND dialog) noexcept {
        dialog_ = dialog;
        list_ = GetDlgItem(dialog, IDC_HISTORY_VISUALIZATION_LIST);
        if (app::DocumentSession* document = application_->Documents().Find(session_);
            document != nullptr && document->generation == generation_) {
            document->history_visualization_dialog = dialog;
        }
        try {
            const std::wstring title = L"Inkpodファイルの可視化 — " + display_name_;
            SetWindowTextW(dialog, title.c_str());
        } catch (const std::bad_alloc&) {
            SetWindowTextW(dialog, L"Inkpodファイルの可視化");
        }
        InitializeList();
    }

    void StartLoad() noexcept {
        try {
            load_ = std::make_shared<VisualizationLoad>();
        } catch (const std::bad_alloc&) {
            SetLoadFailure(INKPOD_STATUS_INVALID_STATE);
            return;
        }
        load_->dialog = dialog_;
        if (inkpod_task_create(&load_->task) != INKPOD_STATUS_OK) {
            load_.reset();
            SetLoadFailure(INKPOD_STATUS_INVALID_STATE);
            return;
        }
        BindProgress();
        if (!QueueNextStep()) {
            load_->Cancel();
            ClearProgress();
            load_.reset();
            SetLoadFailure(INKPOD_STATUS_INVALID_STATE);
        }
    }

    void StepCompleted() noexcept {
        if (!load_) {
            return;
        }
        InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
        bool finished{};
        {
            std::lock_guard lock(load_->mutex);
            status = load_->status;
            finished = load_->finished;
        }
        UpdateLoadingText();
        InvalidateRect(list_, nullptr, TRUE);
        if (status == INKPOD_STATUS_OK && !finished) {
            if (QueueNextStep()) {
                return;
            }
            status = INKPOD_STATUS_INVALID_STATE;
        }
        if (!finished && status == INKPOD_STATUS_OK) {
            return;
        }
        {
            std::lock_guard lock(load_->mutex);
            if (status == INKPOD_STATUS_OK) {
                visualization_ = std::exchange(load_->handle, nullptr);
            }
        }
        ClearProgress();
        load_.reset();
        loading_ = false;
        if (status != INKPOD_STATUS_OK || visualization_ == nullptr) {
            SetLoadFailure(status);
            return;
        }
        std::uint64_t count{};
        if (inkpod_history_visualization_row_count(
                visualization_, &count) != INKPOD_STATUS_OK
            || count > static_cast<std::uint64_t>(INT_MAX)) {
            SetLoadFailure(INKPOD_STATUS_INVALID_STATE);
            return;
        }
        row_count_ = count;
        ListView_SetItemCountEx(
            list_, static_cast<int>(row_count_), LVSICF_NOINVALIDATEALL);
        QueueVisibleRows();
        InvalidateRect(list_, nullptr, TRUE);
    }

    void Resize() noexcept {
        if (list_ == nullptr) {
            return;
        }
        RECT client{};
        GetClientRect(dialog_, &client);
        const UINT dpi = GetDpiForWindow(dialog_);
        const int margin = MulDiv(7, static_cast<int>(dpi), 96);
        MoveWindow(
            list_, margin, margin,
            std::max(0, static_cast<int>(client.right) - margin * 2),
            std::max(0, static_cast<int>(client.bottom) - margin * 2), TRUE);
        ResizeColumns();
    }

    void CopyDisplayText(NMLVDISPINFOW& info) noexcept {
        if ((info.item.mask & LVIF_TEXT) == 0U || info.item.pszText == nullptr
            || info.item.cchTextMax <= 0) {
            return;
        }
        const wchar_t* text = L"";
        if (loading_) {
            text = info.item.iSubItem == 0 ? loading_text_.c_str() : L"";
        } else if (load_failed_) {
            text = info.item.iSubItem == 0 ? L"履歴を読み込めませんでした" : L"";
        } else if (const CachedRow* row = Row(
                       static_cast<std::uint64_t>(info.item.iItem));
                   row != nullptr) {
            text = info.item.iSubItem == 0
                ? row->primitive_name.c_str()
                : (info.item.iSubItem == 1 ? row->arguments.c_str() : L"");
        }
        wcsncpy_s(
            info.item.pszText,
            static_cast<std::size_t>(info.item.cchTextMax), text, _TRUNCATE);
    }

    void CacheHint(const NMLVCACHEHINT& hint) noexcept {
        if (loading_ || load_failed_ || hint.iFrom < 0 || hint.iTo < hint.iFrom) {
            return;
        }
        const std::uint64_t first = static_cast<std::uint64_t>(hint.iFrom);
        const std::uint64_t hinted_last =
            static_cast<std::uint64_t>(hint.iTo) + 1U;
        const std::uint64_t last = std::min({
            row_count_, hinted_last, first + kMaximumQueuedRows});
        for (std::uint64_t index = first;
             index < last;
             ++index) {
            RequestPrefetch(index);
        }
    }

    void ProcessPrefetch() noexcept {
        prefetch_posted_ = false;
        std::size_t completed{};
        while (!prefetch_queue_.empty() && completed < kMaximumRowsPerPrefetch) {
            const std::uint64_t index = prefetch_queue_.front();
            prefetch_queue_.erase(prefetch_queue_.begin());
            if (LoadRow(index)) {
                ListView_RedrawItems(
                    list_, static_cast<int>(index), static_cast<int>(index));
            }
            ++completed;
        }
        QueueVisibleRows();
        PostPrefetchIfNeeded();
    }

    LRESULT CustomDraw(NMLVCUSTOMDRAW& draw) noexcept {
        if (draw.nmcd.dwDrawStage == CDDS_PREPAINT) {
            return CDRF_NOTIFYITEMDRAW;
        }
        if (draw.nmcd.dwDrawStage == CDDS_ITEMPREPAINT) {
            return CDRF_NOTIFYSUBITEMDRAW;
        }
        if (draw.nmcd.dwDrawStage
                != (CDDS_ITEMPREPAINT | CDDS_SUBITEM)
            || draw.iSubItem != 2 || loading_ || load_failed_) {
            return CDRF_DODEFAULT;
        }
        const std::uint64_t index =
            static_cast<std::uint64_t>(draw.nmcd.dwItemSpec);
        const CachedRow* row = Row(index);
        if (row == nullptr || row->bgra.empty()) {
            return CDRF_DODEFAULT;
        }
        RECT cell{};
        if (ListView_GetSubItemRect(
                list_, static_cast<int>(index), 2, LVIR_BOUNDS, &cell) == FALSE) {
            return CDRF_DODEFAULT;
        }
        const bool selected = (ListView_GetItemState(
            list_, static_cast<int>(index), LVIS_SELECTED) & LVIS_SELECTED) != 0U;
        FillRect(
            draw.nmcd.hdc, &cell,
            GetSysColorBrush(selected ? COLOR_HIGHLIGHT : COLOR_WINDOW));

        const UINT dpi = GetDpiForWindow(dialog_);
        const int maximum = MulDiv(64, static_cast<int>(dpi), 96);
        int draw_width = static_cast<int>(row->width);
        int draw_height = static_cast<int>(row->height);
        if (draw_width > maximum || draw_height > maximum) {
            if (draw_width >= draw_height) {
                draw_height = std::max(1, draw_height * maximum / draw_width);
                draw_width = maximum;
            } else {
                draw_width = std::max(1, draw_width * maximum / draw_height);
                draw_height = maximum;
            }
        }
        const int x = static_cast<int>(cell.left) + std::max(
            2, (static_cast<int>(cell.right - cell.left) - draw_width) / 2);
        const int y = static_cast<int>(cell.top) + std::max(
            2, (static_cast<int>(cell.bottom - cell.top) - draw_height) / 2);
        RECT preview{x - 1, y - 1, x + draw_width + 1, y + draw_height + 1};
        FillRect(draw.nmcd.hdc, &preview, GetSysColorBrush(COLOR_WINDOW));
        BITMAPINFO bitmap{};
        bitmap.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
        bitmap.bmiHeader.biWidth = static_cast<LONG>(row->width);
        bitmap.bmiHeader.biHeight = -static_cast<LONG>(row->height);
        bitmap.bmiHeader.biPlanes = 1;
        bitmap.bmiHeader.biBitCount = 32;
        bitmap.bmiHeader.biCompression = BI_RGB;
        SetStretchBltMode(draw.nmcd.hdc, HALFTONE);
        (void)StretchDIBits(
            draw.nmcd.hdc,
            x, y, draw_width, draw_height,
            0, 0, static_cast<int>(row->width), static_cast<int>(row->height),
            row->bgra.data(), &bitmap, DIB_RGB_COLORS, SRCCOPY);
        FrameRect(draw.nmcd.hdc, &preview, GetSysColorBrush(COLOR_WINDOWFRAME));
        return CDRF_SKIPDEFAULT;
    }

    void Detach() noexcept {
        ClearProgress();
        if (app::DocumentSession* document = application_->Documents().Find(session_);
            document != nullptr && document->generation == generation_
            && document->history_visualization_dialog == dialog_) {
            document->history_visualization_dialog = nullptr;
        }
        dialog_ = nullptr;
        list_ = nullptr;
    }

private:
    void BindProgress() noexcept {
        if (!load_) {
            return;
        }
        auto& workspace = application_->Workspace();
        const ProgressDialogState progress{
            load_.get(),
            QueryVisualizationProgress,
            CancelVisualizationProgress,
            L"Inkpodファイルの可視化",
            L"編集履歴を再構築中...",
            L"キャンセル中..."};
        if (!BindJobProgress(
                workspace.job_progress,
                workspace.job_progress_state,
                JobProgressSlot::HistoryVisualization,
                progress)) {
            return;
        }
        load_->progress_bound = true;
        static_cast<void>(workspace.windows.dock_host.RestorePane(
            DockPaneType::JobProgress));
        static_cast<void>(workspace.windows.dock_host.ActivatePane(
            DockPaneType::JobProgress));
    }

    void ClearProgress() noexcept {
        if (!load_ || !load_->progress_bound) {
            return;
        }
        auto& workspace = application_->Workspace();
        ClearJobProgressIfContext(
            workspace.job_progress,
            workspace.job_progress_state,
            JobProgressSlot::HistoryVisualization,
            load_.get());
        load_->progress_bound = false;
        if (!HasActiveJobProgress(workspace.job_progress_state)) {
            static_cast<void>(workspace.windows.dock_host.HidePane(
                DockPaneType::JobProgress));
        }
    }

    bool QueueNextStep() noexcept {
        if (!load_ || application_->engine == nullptr) {
            return false;
        }
        app::CommandContext context{};
        context.document_session = session_;
        context.generation = generation_;
        const std::shared_ptr<VisualizationLoad> load = load_;
        return application_->engine->Enqueue(
            context,
            [load](InkpodCore* core) {
                InkpodStatus status = INKPOD_STATUS_OK;
                if (load->builder == nullptr) {
                    status = inkpod_core_history_visualization_builder_begin(
                        core, load->task, &load->builder);
                }
                InkpodHistoryVisualizationProgress progress{};
                progress.struct_size = sizeof(progress);
                InkpodHistoryVisualization* visualization{};
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_history_visualization_builder_step(
                        load->builder,
                        load->task,
                        kMaximumEventsPerStep,
                        &progress,
                        &visualization);
                }
                {
                    std::lock_guard lock(load->mutex);
                    load->progress = progress;
                    load->handle = visualization;
                    load->status = status;
                    load->finished = status != INKPOD_STATUS_OK
                        || visualization != nullptr;
                }
                return status;
            },
            false,
            false,
            true,
            [load](InkpodStatus status) {
                if (status != INKPOD_STATUS_OK) {
                    std::lock_guard lock(load->mutex);
                    load->status = status;
                    load->finished = true;
                }
                (void)PostMessageW(
                    load->dialog, kHistoryVisualizationStepCompleted, 0, 0);
            });
    }

    void UpdateLoadingText() noexcept {
        if (!load_ || load_->task == nullptr) {
            try {
                loading_text_ = L"履歴を準備中...";
            } catch (const std::bad_alloc&) {
            }
            return;
        }
        InkpodTaskInfo info{};
        info.struct_size = sizeof(info);
        if (inkpod_task_query(load_->task, &info) != INKPOD_STATUS_OK
            || info.total_work == 0U) {
            try {
                loading_text_ = L"履歴を準備中...";
            } catch (const std::bad_alloc&) {
            }
            return;
        }
        std::array<wchar_t, 128U> text{};
        _snwprintf_s(
            text.data(), text.size(), _TRUNCATE,
            L"編集履歴を再構築中... %llu / %llu",
            static_cast<unsigned long long>(info.completed_work),
            static_cast<unsigned long long>(info.total_work));
        try {
            loading_text_ = text.data();
        } catch (const std::bad_alloc&) {
        }
    }

    void QueueVisibleRows() noexcept {
        if (loading_ || load_failed_ || list_ == nullptr || row_count_ == 0U) {
            return;
        }
        const int top = std::max(0, ListView_GetTopIndex(list_));
        const int visible = std::max(1, ListView_GetCountPerPage(list_));
        const std::uint64_t last = std::min<std::uint64_t>(
            row_count_, static_cast<std::uint64_t>(top + visible + 2));
        for (std::uint64_t index = static_cast<std::uint64_t>(top);
             index < last;
             ++index) {
            RequestPrefetch(index);
        }
    }

    void RequestPrefetch(std::uint64_t index) noexcept {
        if (index >= row_count_ || prefetch_queue_.size() >= kMaximumQueuedRows
            || cache_.contains(index)
            || std::find(prefetch_queue_.begin(), prefetch_queue_.end(), index)
                != prefetch_queue_.end()) {
            return;
        }
        try {
            prefetch_queue_.push_back(index);
        } catch (const std::bad_alloc&) {
            return;
        }
        PostPrefetchIfNeeded();
    }

    void PostPrefetchIfNeeded() noexcept {
        if (!prefetch_posted_ && !prefetch_queue_.empty() && dialog_ != nullptr) {
            prefetch_posted_ = PostMessageW(
                dialog_, kHistoryVisualizationPrefetch, 0, 0) != FALSE;
        }
    }

    void InitializeList() noexcept {
        if (list_ == nullptr) {
            return;
        }
        ListView_SetExtendedListViewStyle(
            list_, LVS_EX_FULLROWSELECT | LVS_EX_GRIDLINES | LVS_EX_DOUBLEBUFFER);
        const std::array<const wchar_t*, 3U> labels{
            L"プリミティブ", L"引数", L"結果"};
        for (int index = 0; index < static_cast<int>(labels.size()); ++index) {
            LVCOLUMNW column{};
            column.mask = LVCF_TEXT | LVCF_SUBITEM | LVCF_WIDTH;
            column.iSubItem = index;
            column.pszText = const_cast<wchar_t*>(labels[index]);
            column.cx = 120;
            (void)ListView_InsertColumn(list_, index, &column);
        }
        const UINT dpi = GetDpiForWindow(dialog_);
        const int row_height = MulDiv(70, static_cast<int>(dpi), 96);
        image_list_ = ImageList_Create(
            1, std::max(1, row_height), ILC_COLOR32, 1, 1);
        if (image_list_ != nullptr) {
            ListView_SetImageList(list_, image_list_, LVSIL_SMALL);
        }
        ListView_SetItemCountEx(list_, 1, LVSICF_NOINVALIDATEALL);
        Resize();
    }

    void ResizeColumns() noexcept {
        RECT client{};
        GetClientRect(list_, &client);
        const int width = std::max(
            0, static_cast<int>(client.right - client.left));
        ListView_SetColumnWidth(list_, 0, std::max(120, width * 23 / 100));
        ListView_SetColumnWidth(list_, 1, std::max(260, width * 62 / 100));
        ListView_SetColumnWidth(list_, 2, std::max(96, width * 15 / 100));
    }

    void SetLoadFailure(InkpodStatus) noexcept {
        loading_ = false;
        load_failed_ = true;
        row_count_ = 0;
        ListView_SetItemCountEx(list_, 1, LVSICF_NOINVALIDATEALL);
        InvalidateRect(list_, nullptr, TRUE);
    }

    const CachedRow* Row(std::uint64_t index) noexcept {
        if (index >= row_count_ || visualization_ == nullptr) {
            return nullptr;
        }
        if (const auto found = cache_.find(index); found != cache_.end()) {
            return &found->second;
        }
        RequestPrefetch(index);
        return nullptr;
    }

    bool LoadRow(std::uint64_t index) noexcept {
        if (index >= row_count_ || visualization_ == nullptr
            || cache_.contains(index)) {
            return false;
        }
        if (cache_.size() >= kMaximumCachedRows) {
            cache_.clear();
        }
        InkpodHistoryVisualizationRowBuffer output{};
        output.struct_size = sizeof(output);
        if (inkpod_history_visualization_row_get(
                visualization_, index, &output) != INKPOD_STATUS_OK) {
            return false;
        }
        try {
            std::vector<std::uint8_t> name(
                static_cast<std::size_t>(output.primitive_name_bytes));
            std::vector<std::uint8_t> arguments(
                static_cast<std::size_t>(output.arguments_bytes));
            std::vector<std::uint8_t> rgba(
                static_cast<std::size_t>(output.thumbnail_bytes));
            output.primitive_name_utf8 = name.empty() ? nullptr : name.data();
            output.primitive_name_capacity = name.size();
            output.arguments_utf8 = arguments.empty() ? nullptr : arguments.data();
            output.arguments_capacity = arguments.size();
            output.thumbnail_rgba8 = rgba.empty() ? nullptr : rgba.data();
            output.thumbnail_capacity = rgba.size();
            if (inkpod_history_visualization_row_get(
                    visualization_, index, &output) != INKPOD_STATUS_OK) {
                return false;
            }
            for (std::size_t offset = 0U; offset + 3U < rgba.size(); offset += 4U) {
                std::swap(rgba[offset], rgba[offset + 2U]);
            }
            CachedRow row{};
            row.primitive_name = Utf8ToWide(name);
            row.arguments = Utf8ToWide(arguments);
            row.width = output.thumbnail_width;
            row.height = output.thumbnail_height;
            row.bgra = std::move(rgba);
            cache_.emplace(index, std::move(row));
            return true;
        } catch (const std::bad_alloc&) {
            return false;
        }
    }

    app::ApplicationHost* application_{};
    app::DocumentSessionId session_{};
    app::Generation generation_{};
    std::wstring display_name_;
    HWND dialog_{};
    HWND list_{};
    HIMAGELIST image_list_{};
    InkpodHistoryVisualization* visualization_{};
    std::shared_ptr<VisualizationLoad> load_;
    std::map<std::uint64_t, CachedRow> cache_;
    std::vector<std::uint64_t> prefetch_queue_;
    std::wstring loading_text_{L"履歴を準備中..."};
    std::uint64_t row_count_{};
    bool prefetch_posted_{};
    bool loading_{true};
    bool load_failed_{};
};

INT_PTR CALLBACK HistoryVisualizationDialogProcedure(
    HWND dialog, UINT message, WPARAM, LPARAM lparam) noexcept {
    auto* controller = reinterpret_cast<HistoryVisualizationController*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            controller = reinterpret_cast<HistoryVisualizationController*>(lparam);
            if (controller == nullptr) {
                return FALSE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(controller));
            controller->Attach(dialog);
            return TRUE;
        case kHistoryVisualizationStepCompleted:
            if (controller != nullptr) {
                controller->StepCompleted();
            }
            return TRUE;
        case kHistoryVisualizationPrefetch:
            if (controller != nullptr) {
                controller->ProcessPrefetch();
            }
            return TRUE;
        case WM_SIZE:
            if (controller != nullptr) {
                controller->Resize();
            }
            return TRUE;
        case WM_NOTIFY:
            if (controller != nullptr) {
                auto* header = reinterpret_cast<NMHDR*>(lparam);
                if (header != nullptr
                    && header->idFrom == IDC_HISTORY_VISUALIZATION_LIST) {
                    if (header->code == LVN_GETDISPINFOW) {
                        controller->CopyDisplayText(
                            *reinterpret_cast<NMLVDISPINFOW*>(lparam));
                        return TRUE;
                    }
                    if (header->code == LVN_ODCACHEHINT) {
                        controller->CacheHint(
                            *reinterpret_cast<NMLVCACHEHINT*>(lparam));
                        return TRUE;
                    }
                    if (header->code == NM_CUSTOMDRAW) {
                        SetWindowLongPtrW(
                            dialog, DWLP_MSGRESULT,
                            controller->CustomDraw(
                                *reinterpret_cast<NMLVCUSTOMDRAW*>(lparam)));
                        return TRUE;
                    }
                }
            }
            break;
        case WM_CLOSE:
            DestroyWindow(dialog);
            return TRUE;
        case WM_NCDESTROY:
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            if (controller != nullptr) {
                controller->Detach();
                delete controller;
            }
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

}  // namespace

bool IsHistoryVisualizationCommand(UINT command) noexcept {
    return command >= IDM_TOOL_HISTORY_VISUALIZATION_FIRST
        && command <= IDM_TOOL_HISTORY_VISUALIZATION_LAST;
}

void UpdateHistoryVisualizationMenu(
    app::ApplicationHost& application, HMENU main_menu) noexcept {
    HMENU submenu = FindVisualizationSubmenu(main_menu);
    if (submenu == nullptr) {
        return;
    }
    while (GetMenuItemCount(submenu) > 0) {
        (void)DeleteMenu(submenu, 0, MF_BYPOSITION);
    }
    app::WorkspaceWindow& workspace = application.Workspace();
    workspace.history_visualization_menu_target_count = 0U;
    try {
        std::vector<MenuCandidate> candidates;
        candidates.reserve(application.Documents().Count());
        for (std::size_t index = 0U; index < application.Documents().Count(); ++index) {
            app::DocumentSession* document = application.Documents().SessionAt(index);
            if (document == nullptr || document->shell.current_path.empty()
                || !IsNativeInkpodPath(document->shell.current_path)) {
                continue;
            }
            candidates.push_back(
                MenuCandidate{document, LeafName(document->shell.current_path)});
        }
        const std::size_t count = std::min(
            candidates.size(), workspace.history_visualization_menu_targets.size());
        for (std::size_t index = 0U; index < count; ++index) {
            const MenuCandidate& candidate = candidates[index];
            const bool duplicate_leaf = std::count_if(
                candidates.cbegin(), candidates.cend(),
                [&candidate](const MenuCandidate& other) {
                    return CompareStringOrdinal(
                               candidate.leaf.c_str(), -1,
                               other.leaf.c_str(), -1, TRUE) == CSTR_EQUAL;
                }) > 1;
            std::wstring label = duplicate_leaf
                ? candidate.leaf + L" — " + candidate.document->shell.current_path
                : candidate.leaf;
            label = EscapeMenuText(label);
            MENUITEMINFOW item{};
            item.cbSize = sizeof(item);
            item.fMask = MIIM_ID | MIIM_STATE | MIIM_STRING;
            item.wID = IDM_TOOL_HISTORY_VISUALIZATION_FIRST
                + static_cast<UINT>(index);
            item.fState = MFS_ENABLED;
            item.dwTypeData = label.data();
            if (InsertMenuItemW(
                    submenu, static_cast<UINT>(index), TRUE, &item) == FALSE) {
                break;
            }
            workspace.history_visualization_menu_targets[index] = {
                candidate.document->id, candidate.document->generation};
            workspace.history_visualization_menu_target_count = index + 1U;
        }
    } catch (const std::bad_alloc&) {
        workspace.history_visualization_menu_target_count = 0U;
    }
    if (workspace.history_visualization_menu_target_count == 0U) {
        MENUITEMINFOW item{};
        item.cbSize = sizeof(item);
        item.fMask = MIIM_ID | MIIM_STATE | MIIM_STRING;
        item.wID = IDM_TOOL_HISTORY_VISUALIZATION_FIRST;
        item.fState = MFS_DISABLED;
        wchar_t label[] = L"(なし)";
        item.dwTypeData = label;
        (void)InsertMenuItemW(submenu, 0U, TRUE, &item);
    }
}

LRESULT IssueHistoryVisualizationCommand(
    app::ApplicationHost& application, HWND owner, UINT command) noexcept {
    if (!IsHistoryVisualizationCommand(command)) {
        return 0;
    }
    const std::size_t index =
        static_cast<std::size_t>(command - IDM_TOOL_HISTORY_VISUALIZATION_FIRST);
    app::WorkspaceWindow& workspace = application.Workspace();
    if (index >= workspace.history_visualization_menu_target_count) {
        return 0;
    }
    const app::HistoryVisualizationMenuTarget target =
        workspace.history_visualization_menu_targets[index];
    app::DocumentSession* document = application.Documents().Find(target.session);
    if (document == nullptr || document->generation != target.generation
        || !IsNativeInkpodPath(document->shell.current_path)) {
        return 0;
    }
    if (document->history_visualization_dialog != nullptr
        && IsWindow(document->history_visualization_dialog) != FALSE) {
        ShowWindow(document->history_visualization_dialog, SW_SHOWNORMAL);
        SetForegroundWindow(document->history_visualization_dialog);
        return 0;
    }
    HistoryVisualizationController* controller{};
    try {
        controller = new HistoryVisualizationController(
            application, document->id, document->generation,
            LeafName(document->shell.current_path));
    } catch (const std::bad_alloc&) {
        return 0;
    }
    const HWND dialog = CreateDialogParamW(
        GetModuleHandleW(nullptr),
        MAKEINTRESOURCEW(IDD_HISTORY_VISUALIZATION),
        owner,
        HistoryVisualizationDialogProcedure,
        reinterpret_cast<LPARAM>(controller));
    if (dialog == nullptr) {
        delete controller;
        return 0;
    }
    controller->StartLoad();
    ShowWindow(dialog, SW_SHOWNORMAL);
    return 0;
}

bool TranslateHistoryVisualizationDialogMessage(
    const app::ApplicationHost& application, MSG& message) noexcept {
    for (std::size_t index = 0U; index < application.Documents().Count(); ++index) {
        const app::DocumentSession* document = application.Documents().SessionAt(index);
        if (document != nullptr && document->history_visualization_dialog != nullptr
            && IsWindowVisible(document->history_visualization_dialog) != FALSE
            && IsDialogMessageW(
                document->history_visualization_dialog, &message) != FALSE) {
            return true;
        }
    }
    return false;
}

void CloseHistoryVisualizationDialog(app::DocumentSession& document) noexcept {
    if (document.history_visualization_dialog != nullptr) {
        DestroyWindow(document.history_visualization_dialog);
        document.history_visualization_dialog = nullptr;
    }
}

void CloseAllHistoryVisualizationDialogs(
    app::ApplicationHost& application) noexcept {
    for (std::size_t index = 0U; index < application.Documents().Count(); ++index) {
        if (app::DocumentSession* document = application.Documents().SessionAt(index);
            document != nullptr) {
            CloseHistoryVisualizationDialog(*document);
        }
    }
}

}  // namespace inkpod::windows::ui
