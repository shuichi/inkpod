#pragma once

#include <windows.h>

#include <cstdint>
#include <string>

#include "app/identity.h"
#include "renderer/canvas.h"

namespace inkpod::renderer {
class RendererHost;
}

namespace inkpod::windows::ui::panes {

enum class SubpalettePaneAction : std::uint32_t {
    Previous,
    Next,
    Current,
    Fit,
    OneToOne,
    ToggleAutoPrevious,
    ToggleScrollSync,
};

using SubpalettePaneCommandCallback = void (*)(void*, UINT) noexcept;
using SubpalettePaneActionCallback = void (*)(void*, SubpalettePaneAction) noexcept;
using SubpalettePaneSampleCallback = void (*)(void*, double, double) noexcept;
using SubpalettePaneViewCallback = void (*)(
    void*, const renderer::CanvasViewGesture&) noexcept;

struct SubpalettePaneView final {
    std::wstring target_text;
    std::wstring source_text;
    std::wstring empty_text;
    bool target_available{};
    bool source_available{};
    bool pinned{};
    bool auto_previous{true};
    bool scroll_sync{};
};

struct SubpalettePaneDialogState final {
    void* context{};
    SubpalettePaneCommandCallback dispatch_command{};
    SubpalettePaneActionCallback perform_action{};
    SubpalettePaneSampleCallback sample{};
    SubpalettePaneViewCallback apply_view{};
    SubpalettePaneView view;
    HWND canvas{};
    app::Generation surface_generation{};
};

HWND CreateSubpalettePaneDialog(
    HINSTANCE instance,
    HWND owner,
    renderer::RendererHost& renderer,
    app::CanvasId canvas,
    app::Generation surface_generation,
    SubpalettePaneDialogState& state) noexcept;

void UpdateSubpalettePaneDialog(HWND dialog, SubpalettePaneView view) noexcept;
void LayoutSubpalettePaneDialog(HWND dialog) noexcept;

}  // namespace inkpod::windows::ui::panes
