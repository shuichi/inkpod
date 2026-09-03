#pragma once

#include "ui/localization.h"
#include "ui/job_progress.h"

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>

namespace inkpod::windows::ui {

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
    std::size_t parameter_count{5U};
    bool line_options{};
    std::array<std::uint32_t, 3U> line_values{1U, 0U, 0U};
    const wchar_t* points_label{};
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

}  // namespace inkpod::windows::ui
