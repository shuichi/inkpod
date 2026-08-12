#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "ui/thumbnail_cache.h"

namespace inkpod::windows::ui::panes {

using SequencePaneCommandCallback = void (*)(void* context, UINT command) noexcept;
using SequencePaneActivateCallback = void (*)(void* context, std::uint32_t index) noexcept;
using SequencePaneReorderCallback = void (*)(
    void* context, std::uint32_t from, std::uint32_t to) noexcept;

struct SequencePaneCellView final {
    std::uint32_t sequence_index{};
    std::uint32_t cell_number{};
    std::uint32_t width{};
    std::uint32_t height{};
    std::uint32_t thumbnail_width{};
    std::uint32_t thumbnail_height{};
    std::uint32_t thumbnail_stride_bytes{};
    std::uint64_t thumbnail_checksum{};
    std::wstring name;
    ThumbnailCacheKey thumbnail_key{};
};

struct SequencePaneView final {
    std::wstring target_text;
    std::wstring empty_text;
    std::vector<SequencePaneCellView> cells;
    std::uint32_t active_index{UINT32_MAX};
    bool target_available{};
    bool pinned{};
    bool cut_editable{};
};

struct SequencePaneDialogState final {
    void* context{};
    ThumbnailCache* thumbnail_cache{};
    SequencePaneCommandCallback dispatch_command{};
    SequencePaneActivateCallback activate_cell{};
    SequencePaneReorderCallback reorder_cell{};
    SequencePaneView view;
    std::uint32_t drag_index{UINT32_MAX};
};

HWND CreateSequencePaneDialog(
    HINSTANCE instance, HWND owner, SequencePaneDialogState& state) noexcept;

void UpdateSequencePaneDialog(HWND dialog, SequencePaneView view) noexcept;

bool SequencePaneItemHasThumbnail(HWND dialog, std::size_t index) noexcept;

}  // namespace inkpod::windows::ui::panes
