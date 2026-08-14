#pragma once

#include "ui/localization.h"

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>

namespace inkpod::windows::ui {

struct ProgressDialogInfo {
    std::uint64_t completed_work{};
    std::uint64_t total_work{};
};

struct EffectEditorState;
using EffectEditorChangeCallback = bool (*)(
    void* context, const EffectEditorState& state) noexcept;
using EffectEditorProgressCallback = bool (*)(
    void* context, ProgressDialogInfo& output) noexcept;

struct EffectEditorState {
    const wchar_t* title{UiText(UiStringId::Text0811)};
    std::array<const wchar_t*, 5U> parameter_labels{
        UiText(UiStringId::ParameterP0),
        UiText(UiStringId::ParameterP1),
        UiText(UiStringId::ParameterP2),
        UiText(UiStringId::ParameterP3),
        UiText(UiStringId::ParameterP4)};
    std::array<std::int32_t, 5U> parameters{};
    std::array<const wchar_t*, 5U> channel_labels{};
    std::array<std::uint32_t, 5U> channel_values{};
    std::size_t channel_count{};
    std::uint32_t channel{};
    std::array<const wchar_t*, 4U> mode_labels{};
    std::array<std::uint32_t, 4U> mode_values{};
    std::size_t mode_count{};
    std::uint32_t mode{};
    std::wstring points;
    const wchar_t* option1_label{UiText(UiStringId::Text0314)};
    const wchar_t* option2_label{UiText(UiStringId::Text0034)};
    bool option1{true};
    bool option2{};
    bool option1_enabled{true};
    bool option2_enabled{true};
    bool close_immediately{};
    void* preview_context{};
    EffectEditorChangeCallback preview_change{};
    EffectEditorProgressCallback preview_progress{};
    const wchar_t* preview_idle_text{UiText(UiStringId::Text0272)};
    HWND dialog{};
    std::uint32_t smoke_change_step{};
    bool smoke_cancel{};
};

INT_PTR ShowEffectEditor(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    EffectEditorState& state) noexcept;
void SetEffectEditorPreviewStatus(HWND dialog, const wchar_t* text) noexcept;

using ProgressQueryCallback = bool (*)(
    void* context, ProgressDialogInfo& output) noexcept;
using ProgressCancelCallback = void (*)(void* context) noexcept;

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

struct JobProgressEntry {
    ProgressDialogState progress{};
    bool active{};
    bool cancelling{};
};

struct JobProgressPaneState {
    std::array<JobProgressEntry, static_cast<std::size_t>(JobProgressSlot::Count)>
        entries{};
};

HWND CreateJobProgressPane(
    HINSTANCE instance, HWND parent, JobProgressPaneState& state) noexcept;
[[nodiscard]] bool BindJobProgress(
    HWND pane,
    JobProgressPaneState& state,
    JobProgressSlot slot,
    const ProgressDialogState& progress) noexcept;
void ClearJobProgress(
    HWND pane, JobProgressPaneState& state, JobProgressSlot slot) noexcept;
void ClearJobProgressIfContext(
    HWND pane,
    JobProgressPaneState& state,
    JobProgressSlot slot,
    const void* context) noexcept;
[[nodiscard]] bool HasActiveJobProgress(
    const JobProgressPaneState& state) noexcept;

}  // namespace inkpod::windows::ui
