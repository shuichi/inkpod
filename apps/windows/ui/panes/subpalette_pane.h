#pragma once

#include <windows.h>

#include <cstdint>
#include <string>

#include "app/identity.h"
#include "inkpod/core_ffi.h"
#include "renderer/canvas.h"

namespace inkpod::renderer {
class RendererHost;
}

namespace inkpod::windows::ui::panes {

enum class SubpalettePaneAction : std::uint32_t {
    OpenFiles,
    OpenFolder,
    Previous,
    Next,
    Fit,
    OneToOne,
    RegisterSample,
};

using SubpalettePaneCommandCallback = void (*)(void*, UINT) noexcept;
using SubpalettePaneActionCallback = void (*)(void*, SubpalettePaneAction) noexcept;
using SubpalettePaneSampleCallback = void (*)(void*, double, double) noexcept;
using SubpalettePaneViewCallback = bool (*)(
    void*, const renderer::CanvasViewGesture&) noexcept;

struct SubpalettePaneView final {
    std::wstring source_text;
    std::wstring empty_text;
    bool source_available{};
    bool loading{};
    bool can_previous{};
    bool can_next{};
    bool sample_available{};
    InkpodColorValue sample_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
};

struct SubpalettePaneDialogState final {
    void* context{};
    SubpalettePaneCommandCallback dispatch_command{};
    SubpalettePaneActionCallback perform_action{};
    SubpalettePaneSampleCallback sample{};
    SubpalettePaneViewCallback apply_view{};
    SubpalettePaneView view;
    HWND canvas{};
    HWND tooltip{};
    HCURSOR eyedropper_cursor{};
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
