#pragma once

#include <windows.h>

namespace inkpod::windows::ui {

// Paints in device pixels using the button HWND's current DPI. The existing
// BUTTON owns input/accessibility; its owner supplies tab selection and hover.
void PaintTabCloseButton(
    const DRAWITEMSTRUCT& draw, bool active, bool hovered) noexcept;

}  // namespace inkpod::windows::ui
