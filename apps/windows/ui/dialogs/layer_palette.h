#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

#include "ui/command_state.h"
#include "ui/panes/document_panes.h"

namespace inkpod::windows::ui {

struct LayerPaletteItem {
    std::uint64_t id{};
    std::uint32_t index{};
    std::uint32_t kind{};
    std::uint32_t opacity_milli{};
    std::uint32_t plane_count{};
    std::uint32_t flags{};
    std::wstring name;
    std::uint32_t thumbnail_width{};
    std::uint32_t thumbnail_height{};
    std::uint32_t thumbnail_stride_bytes{};
    std::vector<std::uint8_t> thumbnail_bgra;
};

using LayerPaletteCommandCallback = void (*)(void* context, UINT command) noexcept;
using LayerPaletteSelectionCallback = void (*)(
    void* context, std::uint64_t layer_id) noexcept;
using LayerPaletteReorderCallback = void (*)(
    void* context, std::uint64_t layer_id, std::uint32_t destination_index) noexcept;
using LayerPaletteVisibilityCallback = void (*)(void* context) noexcept;

struct LayerPaletteDialogState {
    void* context{};
    LayerPaletteCommandCallback dispatch_command{};
    LayerPaletteSelectionCallback select_layer{};
    LayerPaletteReorderCallback reorder_layer{};
    LayerPaletteVisibilityCallback visibility_changed{};
    std::vector<LayerPaletteItem> items;
    std::uint64_t selected_layer_id{};
    int drag_source{-1};
    int drop_index{-1};
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
    std::uint64_t selected_layer_id) noexcept;

void UpdateLayerPaletteCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept;

bool LayerPaletteMatchesCommandState(
    HWND dialog,
    const CommandStateSet& states) noexcept;

std::size_t LayerPaletteItemCount(HWND dialog) noexcept;
std::uint64_t LayerPaletteSelectedLayer(HWND dialog) noexcept;
bool LayerPaletteItemHasThumbnail(HWND dialog, std::size_t index) noexcept;

} // namespace inkpod::windows::ui
