#pragma once

#include <windows.h>

#include <cstdint>
#include <string>
#include <vector>

namespace inkpod::windows::ui::panes {

using LightTablePaneCommandCallback = void (*)(void* context, UINT command) noexcept;
using LightTablePaneSelectionCallback = void (*)(
    void* context,
    bool set_selection,
    std::uint32_t index,
    std::uint64_t stable_id) noexcept;

struct LightTablePaneSetView final {
    std::uint64_t id{};
    std::uint32_t opacity_milli{};
    std::uint32_t item_count{};
    std::wstring name;
};

struct LightTablePaneItemView final {
    std::uint64_t id{};
    std::uint32_t flags{};
    std::uint32_t opacity_milli{};
    std::uint32_t display_mode{};
    std::int32_t translate_x_milli{};
    std::int32_t translate_y_milli{};
    std::wstring name;
};

struct LightTablePaneView final {
    std::wstring target_text;
    std::wstring empty_text;
    std::vector<LightTablePaneSetView> sets;
    std::vector<LightTablePaneItemView> items;
    std::uint32_t selected_set_index{UINT32_MAX};
    std::uint32_t selected_item_index{UINT32_MAX};
    bool target_available{};
    bool pinned{};
};

struct LightTablePaneDialogState final {
    void* context{};
    LightTablePaneCommandCallback dispatch_command{};
    LightTablePaneSelectionCallback select_entry{};
    LightTablePaneView view;
};

HWND CreateLightTablePaneDialog(
    HINSTANCE instance, HWND owner, LightTablePaneDialogState& state) noexcept;

void UpdateLightTablePaneDialog(HWND dialog, LightTablePaneView view) noexcept;

}  // namespace inkpod::windows::ui::panes
