#pragma once

#include <windows.h>

#include <array>
#include <cstdint>
#include <string>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui::panes {

using LocatorPaneCommandCallback = void (*)(void* context, UINT command) noexcept;
using LocatorPanePixelCallback = void (*)(
    void* context, std::int32_t document_x, std::int32_t document_y) noexcept;

struct LocatorPaneDialogState final {
    void* context{};
    LocatorPaneCommandCallback dispatch_command{};
    LocatorPanePixelCallback select_pixel{};
    std::uint32_t neighborhood_width{};
    std::uint32_t neighborhood_height{};
    std::int32_t neighborhood_origin_x{};
    std::int32_t neighborhood_origin_y{};
    std::array<std::uint8_t, 9U * 9U * 4U> neighborhood{};
    bool fixed_mode{};
};

struct LocatorPaneView final {
    std::wstring target_text;
    std::wstring coordinate_text;
    std::wstring selection_text;
    std::wstring color_text;
    std::uint32_t neighborhood_width{};
    std::uint32_t neighborhood_height{};
    std::int32_t neighborhood_origin_x{};
    std::int32_t neighborhood_origin_y{};
    std::array<std::uint8_t, 9U * 9U * 4U> neighborhood{};
    bool valid{};
    bool pinned{};
    bool fixed_mode{};
    bool auto_scroll{};
};

HWND CreateLocatorPaneDialog(
    HINSTANCE instance, HWND owner, LocatorPaneDialogState& state) noexcept;

void UpdateLocatorPaneDialog(
    HWND dialog, const LocatorPaneView& view) noexcept;

}  // namespace inkpod::windows::ui::panes
