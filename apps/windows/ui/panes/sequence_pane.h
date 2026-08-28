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
    std::uint64_t document_uuid_high{};
    std::uint64_t document_uuid_low{};
};

// Captured from one published Core catalog. An independent session, recreated
// Core owner, or catalog revision cannot reuse the previous pane's metadata.
struct SequencePaneCatalogKey final {
    app::DocumentSessionId session{};
    app::Generation generation{};
    std::uint64_t owner_generation{};
    std::uint64_t revision{};
    std::uint64_t cell_count{};

    [[nodiscard]] explicit constexpr operator bool() const noexcept {
        return static_cast<bool>(session) && static_cast<bool>(generation)
            && owner_generation != 0U && revision != 0U;
    }
    constexpr bool operator==(const SequencePaneCatalogKey&) const noexcept = default;
};

struct SequencePaneView final {
    std::wstring target_text;
    std::wstring empty_text;
    std::vector<SequencePaneCellView> cells;
    std::uint32_t active_index{UINT32_MAX};
    bool target_available{};
    bool pinned{};
    bool cut_editable{};
    bool auto_sequence_truncated{};
    bool wrap_navigation{};
    SequencePaneCatalogKey catalog{};
    // Nonzero only after every referenced thumbnail was present at this generation.
    std::uint64_t thumbnail_generation{};
};

struct SequencePaneSelection final {
    SequencePaneCatalogKey catalog{};
    std::uint32_t active_index{UINT32_MAX};
    std::wstring target_text;
    bool pinned{};
    bool auto_sequence_truncated{};
    bool wrap_navigation{};
};

struct SequencePaneDialogState final {
    void* context{};
    ThumbnailCache* thumbnail_cache{};
    SequencePaneCommandCallback dispatch_command{};
    SequencePaneActivateCallback activate_cell{};
    SequencePaneReorderCallback reorder_cell{};
    SequencePaneView view;
    std::vector<std::wstring> item_labels;
    std::uint32_t drag_index{UINT32_MAX};
    int wheel_remainder{};
};

HWND CreateSequencePaneDialog(
    HINSTANCE instance, HWND owner, SequencePaneDialogState& state) noexcept;

void UpdateSequencePaneDialog(HWND dialog, SequencePaneView view) noexcept;

// Selection/header-only update; preserves cells, labels, geometry, focus, and
// the current viewport unless a changed active cell needs to be revealed.
// Returns false without changing state if the captured catalog or thumbnail
// generation is no longer reusable, so the caller can perform a full refresh.
// A changed global thumbnail generation revalidates this pane's existing keys;
// unrelated pane invalidation must not force every pane to recreate its cache.
[[nodiscard]] bool UpdateSequencePaneSelection(
    HWND dialog, SequencePaneSelection selection) noexcept;

bool SequencePaneItemHasThumbnail(HWND dialog, std::size_t index) noexcept;

}  // namespace inkpod::windows::ui::panes
