#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

struct ShortcutDialogEntry {
    std::uint32_t command_id{};
    std::wstring label;
    InkpodShortcutSequence sequence{};
};

struct ShortcutDialogState {
    std::uint32_t command_id{};
    std::uint32_t virtual_key{static_cast<std::uint32_t>('Z')};
    std::uint32_t modifiers{INKPOD_SHORTCUT_MODIFIER_CONTROL};
    InkpodShortcutSequence sequence{};
    std::vector<ShortcutDialogEntry> entries;
    bool close_immediately{};
};

INT_PTR ShowShortcutEditor(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    ShortcutDialogState& state) noexcept;

UINT ShortcutMenuCommand(std::uint32_t command_id) noexcept;

struct ViewOptionsDialogState {
    const wchar_t* title{};
    std::array<const wchar_t*, 4U> labels{};
    std::array<std::int32_t, 4U> values{};
    struct Choice {
        const wchar_t* label{};
        std::int32_t value{};
    };
    using ValidationCallback = const wchar_t* (*)(
        void* context,
        const std::array<std::int32_t, 4U>& values,
        std::uint32_t value_count) noexcept;
    std::array<const Choice*, 4U> choices{};
    std::array<std::uint32_t, 4U> choice_counts{};
    void* validation_context{};
    ValidationCallback validate{};
    std::uint32_t value_count{1U};
    bool close_immediately{};
    bool centered_on_owner{};
};

INT_PTR ShowViewOptions(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    ViewOptionsDialogState& state) noexcept;

struct CellCreationDialogState {
    using PreviewCallback = bool (*)(
        void* context,
        const InkpodCellCreationOptions& options,
        InkpodCellCreationPlanItem& preview) noexcept;
    InkpodCellCreationOptions options{};
    const ViewOptionsDialogState::Choice* layer_choices{};
    std::uint32_t layer_choice_count{};
    void* preview_context{};
    PreviewCallback build_preview{};
    InkpodCellCreationPlanItem preview{};
    bool close_immediately{};
    bool centered_on_owner{};
};

INT_PTR ShowCellCreationOptions(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    CellCreationDialogState& state) noexcept;

struct ShootingFrameDialogState {
    InkpodShootingFrameInput value{sizeof(InkpodShootingFrameInput)};
    bool close_immediately{};
    bool centered_on_owner{};
};

INT_PTR ShowShootingFrameOptions(
    HINSTANCE instance,
    HWND owner,
    ShootingFrameDialogState& state) noexcept;

struct CutPropertiesDialogState {
    std::wstring work_title;
    std::wstring episode;
    std::wstring scene;
    std::wstring cut_name;
    std::wstring instruction;
    std::uint32_t duration_frames{24U};
    bool close_immediately{};
};

INT_PTR ShowCutProperties(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    CutPropertiesDialogState& state) noexcept;

struct TextInputDialogState {
    const wchar_t* title{};
    const wchar_t* label{};
    std::wstring value;
    bool close_immediately{};
};

INT_PTR ShowTextInput(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    TextInputDialogState& state) noexcept;

struct FillToolOptions {
    InkpodFillOperation operation{INKPOD_FILL_SEED};
    std::uint16_t tolerance{};
    std::uint16_t gap_close{};
    std::uint32_t extension_distance{1U};
    InkpodInclusionMode inclusion_mode{INKPOD_INCLUSION_NONE};
    std::vector<InkpodColorValue> inclusion_colors;
    bool overflow_abort{true};
    bool detached_regions{};
    bool transparent_only{};
    bool use_document_selection{};
    bool light_table_boundary{};
    bool light_table_color{};
};

bool ShowFillOptions(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    FillToolOptions& options) noexcept;

struct HistoryDialogState {
    std::vector<std::wstring> labels;
    std::vector<std::uint64_t> cursors;
    std::size_t selected_index{};
    std::uint64_t selected_cursor{};
    bool close_immediately{};
};

INT_PTR ShowHistoryDialog(
    HINSTANCE instance, HWND owner, HistoryDialogState& state) noexcept;

}  // namespace inkpod::windows::ui
