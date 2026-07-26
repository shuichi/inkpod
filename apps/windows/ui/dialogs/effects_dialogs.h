#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>

namespace inkpod::windows::ui {

struct M6EditorState {
    const wchar_t* title{L"M6 エディター"};
    std::array<const wchar_t*, 5U> parameter_labels{L"P0", L"P1", L"P2", L"P3", L"P4"};
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
    const wchar_t* option1_label{L"プレビューして確認"};
    const wchar_t* option2_label{L"45度制約 / 筆圧"};
    bool option1{true};
    bool option2{};
    bool option1_enabled{true};
    bool option2_enabled{true};
    bool close_immediately{};
};

INT_PTR ShowM6Editor(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    M6EditorState& state) noexcept;

struct ProgressDialogInfo {
    std::uint64_t completed_work{};
    std::uint64_t total_work{};
};

using ProgressQueryCallback = bool (*)(
    void* context, ProgressDialogInfo& output) noexcept;
using ProgressCancelCallback = void (*)(void* context) noexcept;

struct ProgressDialogState {
    void* context{};
    ProgressQueryCallback query{};
    ProgressCancelCallback cancel{};
    const wchar_t* title{};
    const wchar_t* progress_prefix{L"処理中..."};
    const wchar_t* cancelling_text{L"キャンセル中..."};
};

HWND CreateProgressDialog(
    HINSTANCE instance, HWND owner, ProgressDialogState& state) noexcept;

}  // namespace inkpod::windows::ui
