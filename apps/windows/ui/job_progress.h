#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>

#include "ui/localization.h"

namespace inkpod::windows::ui {

struct ProgressDialogInfo {
    std::uint64_t completed_work{};
    std::uint64_t total_work{};
};

using ProgressQueryCallback = bool (*)(void*, ProgressDialogInfo&) noexcept;
using ProgressCancelCallback = void (*)(void*) noexcept;

struct ProgressDialogState {
    void* context{};
    ProgressQueryCallback query{};
    ProgressCancelCallback cancel{};
    const wchar_t* title{};
    const wchar_t* progress_prefix{UiText(UiStringId::Text0512)};
    const wchar_t* cancelling_text{UiText(UiStringId::Cancelling)};
};

enum class JobProgressSlot : std::uint8_t {
    Effect,
    Batch,
    ColorChart,
    HistoryVisualization,
    Count,
};

enum class JobProgressSource : std::uint8_t { None, Task, FileIo };
enum class JobProgressPhase : std::uint8_t { Queued, Running, Applying, Cancelling };

struct JobProgressIdentity {
    JobProgressSource source{};
    std::uint64_t id{};
    std::uint64_t generation{};
    friend bool operator==(const JobProgressIdentity&, const JobProgressIdentity&) = default;
};

struct JobProgressItem {
    JobProgressIdentity identity{};
    // Names come from the process-lifetime localization catalog, never a path.
    const wchar_t* name{};
    std::uint64_t completed_work{};
    std::uint64_t total_work{};
    JobProgressPhase phase{JobProgressPhase::Running};
    bool cancellable{true};
};

struct JobProgressEntry {
    ProgressDialogState progress{};
    JobProgressSlot slot{JobProgressSlot::Count};
    std::uint64_t generation{};
    bool active{};
    bool cancelling{};
};

inline constexpr std::size_t kMaximumFileJobProgress = 128U;
inline constexpr std::size_t kMaximumTaskProgress = 128U;
inline constexpr std::size_t kMaximumJobProgress =
    kMaximumFileJobProgress + kMaximumTaskProgress;
using FileJobCancelCallback = void (*)(void*, std::uint64_t) noexcept;

// Workspace-owned presentation only. Task and I/O handles remain with their
// controllers. Registration, refresh, cancellation, and destruction use the UI
// thread; task queries copy atomic progress without waiting for Core work.
struct JobProgressState {
    // Distinct controllers may run the same kind of task concurrently.
    std::array<JobProgressEntry, kMaximumTaskProgress> entries{};
    std::uint64_t next_generation{1U};
    std::array<JobProgressItem, kMaximumFileJobProgress> file_items{};
    std::size_t file_count{};
    std::array<JobProgressItem, kMaximumJobProgress> items{};
    std::size_t item_count{};
    JobProgressIdentity selected{};
    JobProgressIdentity cancel_target{};
    FileJobCancelCallback cancel_file{};
    void* file_context{};
    HWND selector{};
    HWND bar{};
    HWND cancel{};
    std::array<wchar_t, 256U> idle_text{};
    bool visible{};
    bool marquee{};
    bool refreshing{};
    bool cancel_armed{};
};

[[nodiscard]] bool InitializeJobProgress(
    HWND status_bar, JobProgressState& state,
    FileJobCancelCallback cancel_file, void* file_context) noexcept;
[[nodiscard]] bool BindJobProgress(
    HWND status_bar, JobProgressState& state, JobProgressSlot slot,
    const ProgressDialogState& progress) noexcept;
void ClearJobProgressIfContext(
    HWND status_bar, JobProgressState& state, JobProgressSlot slot, const void* context) noexcept;
[[nodiscard]] bool HasActiveJobProgress(const JobProgressState& state) noexcept;
[[nodiscard]] bool SetFileJobProgress(
    HWND status_bar, JobProgressState& state, std::span<const JobProgressItem> items) noexcept;
void RefreshJobProgress(HWND status_bar, JobProgressState& state) noexcept;
[[nodiscard]] bool SelectJobProgress(
    HWND status_bar, JobProgressState& state, JobProgressIdentity identity) noexcept;
[[nodiscard]] bool CancelJobProgress(
    HWND status_bar, JobProgressState& state, JobProgressIdentity identity) noexcept;
void LayoutJobProgress(HWND status_bar) noexcept;
void SetJobProgressIdleText(HWND status_bar, const wchar_t* text) noexcept;

// A running stage never reports completion before owner-thread publication.
[[nodiscard]] unsigned JobProgressPosition(const JobProgressItem& item) noexcept;

}  // namespace inkpod::windows::ui
