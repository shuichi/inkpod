#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "ui/command_state.h"
#include "ui/localization.h"
#include "ui/panes/document_panes.h"

namespace inkpod::windows::ui {

struct LayerPaletteItem {
    std::uint64_t id{};
    std::uint32_t index{};
    std::uint32_t kind{};
    std::uint32_t pixel_format{};
    std::uint32_t opacity_milli{};
    std::uint32_t plane_count{};
    std::uint32_t flags{};
    bool edit_target{};
    UiStringId kind_label_id{UiStringId::LayerUnknown};
    UiStringId format_label_id{UiStringId::NameUnavailable};
    UiStringId visibility_label_id{UiStringId::Hidden};
    UiStringId editability_label_id{UiStringId::Protected};
    std::wstring name;
    std::wstring kind_text;
    std::wstring format_text;
    std::wstring detail_text;
    std::wstring visibility_text;
    std::wstring editability_text;
    std::wstring accessible_text;
    std::uint32_t thumbnail_width{};
    std::uint32_t thumbnail_height{};
    std::uint32_t thumbnail_stride_bytes{};
    ThumbnailCacheKey thumbnail_key{};
};

using LayerPaletteCommandCallback = void (*)(void* context, UINT command) noexcept;
using LayerPaletteSelectionCallback = void (*)(
    void* context, std::uint64_t layer_id) noexcept;
using LayerPaletteReorderCallback = void (*)(
    void* context, std::uint64_t layer_id, std::uint32_t destination_index) noexcept;
using LayerPaletteSplitCallback = void (*)(
    void* context, std::uint32_t split_milli) noexcept;
using LayerPaletteVisibilityCallback = void (*)(void* context) noexcept;
using LayerPaletteTargetCallback = void (*)(
    void* context, std::uint64_t id, bool plane, bool range) noexcept;

struct LayerPaletteDialogState {
    void* context{};
    ThumbnailCache* thumbnail_cache{};
    LayerPaletteCommandCallback dispatch_command{};
    LayerPaletteSelectionCallback select_layer{};
    LayerPaletteSelectionCallback select_plane{};
    LayerPaletteReorderCallback reorder_layer{};
    LayerPaletteReorderCallback reorder_plane{};
    LayerPaletteSplitCallback change_split{};
    LayerPaletteVisibilityCallback visibility_changed{};
    LayerPaletteTargetCallback toggle_target{};
    std::vector<LayerPaletteItem> items;
    std::vector<LayerPaletteItem> plane_items;
    std::uint64_t selected_layer_id{};
    std::uint64_t selected_plane_id{};
    int drag_source{-1};
    int drop_index{-1};
    int drag_list_id{};
    POINT split_drag_start{};
    std::uint32_t split_drag_initial{550U};
    std::uint32_t split_milli{550U};
    CommandStateSet command_states{};
    bool plane_active{};
    bool split_dragging{};
    bool split_hovered{};
    bool has_command_states{};
    bool updating{};
    HFONT font{};
};

HWND CreateLayerPaletteDialog(
    HINSTANCE instance,
    HWND owner,
    LayerPaletteDialogState& state) noexcept;

void UpdateLayerPaletteDialog(
    HWND dialog,
    const std::vector<panes::TreePaneNode>& layers,
    const std::vector<panes::TreePaneNode>& planes,
    const std::vector<InkpodEditTarget>& edit_targets,
    std::uint64_t selected_layer_id,
    std::uint64_t selected_plane_id,
    std::uint32_t split_milli) noexcept;

void UpdateLayerPaletteCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept;

bool LayerPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept;

std::size_t LayerPaletteItemCount(HWND dialog) noexcept;
std::uint64_t LayerPaletteSelectedLayer(HWND dialog) noexcept;
std::size_t LayerPalettePlaneCount(HWND dialog) noexcept;
std::uint64_t LayerPaletteSelectedPlane(HWND dialog) noexcept;
bool LayerPaletteItemHasThumbnail(HWND dialog, std::size_t index) noexcept;

} // namespace inkpod::windows::ui
