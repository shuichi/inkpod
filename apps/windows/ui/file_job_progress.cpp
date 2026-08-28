#include "file_job_progress.h"

#include <array>
#include <span>

#include "app/file_io_controller.h"
#include "ui/job_progress.h"
#include "ui/localization.h"

namespace inkpod::windows::ui {
namespace {

const wchar_t* FileJobName(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_IO_OPEN_NATIVE:
        case INKPOD_IO_OPEN_RASTER:
            return UiText(UiStringId::JobStatusRead);
        case INKPOD_IO_SAVE_PAIR:
        case INKPOD_IO_AUTOSAVE:
            return UiText(UiStringId::JobStatusSave);
        case INKPOD_IO_SEQUENCE_AUTO:
        case INKPOD_IO_SEQUENCE_FILES:
        case INKPOD_IO_SEQUENCE_SWITCH:
            return UiText(UiStringId::JobStatusSequence);
        case INKPOD_IO_EXPORT_RASTER:
        case INKPOD_IO_EXPORT_SEQUENCE:
            return UiText(UiStringId::JobStatusExport);
        case INKPOD_IO_REFERENCE_FILES:
        case INKPOD_IO_REFERENCE_FOLDER:
            return UiText(UiStringId::JobStatusReference);
        case INKPOD_IO_LIGHT_TABLE_ADD:
        case INKPOD_IO_LIGHT_TABLE_RELOAD:
            return UiText(UiStringId::JobStatusLightTable);
        case INKPOD_IO_BATCH_PLAN:
        case INKPOD_IO_BATCH_RUN:
        case INKPOD_IO_BATCH_PREVIEW:
            return UiText(UiStringId::JobStatusBatch);
        case INKPOD_IO_OPEN_RECOVERY:
        case INKPOD_IO_RECOVERY_LIST:
        case INKPOD_IO_RECOVERY_DISCARD:
        case INKPOD_IO_RECOVERY_PROBE:
            return UiText(UiStringId::JobStatusRecovery);
        case INKPOD_IO_COMPACTED_COPY:
            return UiText(UiStringId::JobStatusCompaction);
        default:
            return UiText(UiStringId::FileIoOperation);
    }
}

JobProgressPhase FileJobPhase(const app::FileIoProgressEntry& entry) noexcept {
    if (entry.cancelling || entry.progress.state == INKPOD_IO_CANCELLED) {
        return JobProgressPhase::Cancelling;
    }
    if ((entry.progress.flags & INKPOD_IO_RESULT_INSTALLING) != 0U
        || entry.progress.state == INKPOD_IO_READY
        || entry.progress.state == INKPOD_IO_COMPLETE) {
        return JobProgressPhase::Applying;
    }
    return entry.progress.state == INKPOD_IO_QUEUED
        ? JobProgressPhase::Queued : JobProgressPhase::Running;
}

}  // namespace

void RefreshFileJobProgress(
    app::FileIoController& controller,
    app::WorkspaceWindowId workspace,
    HWND status_bar,
    JobProgressState& state) noexcept {
    std::array<app::FileIoProgressEntry, kMaximumFileJobProgress> progress{};
    const std::size_t count = controller.CopyProgress(workspace, progress);
    std::array<JobProgressItem, kMaximumFileJobProgress> items{};
    for (std::size_t index = 0U; index < count; ++index) {
        const auto& entry = progress[index];
        // The request identity exists before Rust submission and never changes
        // when a queued job receives its Rust job ID.
        items[index] = {
            {JobProgressSource::FileIo, entry.request_id,
             entry.context.generation.value_or(app::Generation{}).Value()},
            FileJobName(entry.progress.kind),
            entry.progress.completed_work,
            entry.progress.total_work,
            FileJobPhase(entry),
            entry.progress.kind != INKPOD_IO_RECOVERY_DISCARD,
        };
    }
    // Unique request IDs and the fixed controller bound satisfy the presenter's
    // identity/capacity contract. Read/loaded counts never replace work units.
    (void)SetFileJobProgress(status_bar, state, std::span{items.data(), count});
}

}  // namespace inkpod::windows::ui
