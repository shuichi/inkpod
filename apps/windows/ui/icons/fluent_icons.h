#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>

namespace inkpod::windows::ui {

// Windows-only semantic identifiers. Platform icon names never cross the UI
// adapter boundary into Core, the C ABI, history, or persisted state.
enum class ToolIconId : std::uint16_t {
    Pencil,
    Brush,
    Eraser,
    Fill,
    ClosedRegionFill,
    FillExtension,
    Eyedropper,
    Gradient,
    Airbrush,
    BoundaryAirbrush,
    Blur,
    Stamp,
    DustRemoval,
    AlphaGradient,
    Count,
};

enum class PaneIconId : std::uint16_t {
    Visible,
    Hidden,
    Editable,
    Protected,
    PinDocument,
    ReturnToFollowing,
    Previous,
    Next,
    Fit,
    OneToOne,
    Count,
};

inline constexpr std::size_t kToolIconCount =
    static_cast<std::size_t>(ToolIconId::Count);
inline constexpr std::size_t kPaneIconCount =
    static_cast<std::size_t>(PaneIconId::Count);

inline constexpr std::array<ToolIconId, kToolIconCount> kAllToolIconIds{
    ToolIconId::Pencil,
    ToolIconId::Brush,
    ToolIconId::Eraser,
    ToolIconId::Fill,
    ToolIconId::ClosedRegionFill,
    ToolIconId::FillExtension,
    ToolIconId::Eyedropper,
    ToolIconId::Gradient,
    ToolIconId::Airbrush,
    ToolIconId::BoundaryAirbrush,
    ToolIconId::Blur,
    ToolIconId::Stamp,
    ToolIconId::DustRemoval,
    ToolIconId::AlphaGradient,
};

inline constexpr std::array<PaneIconId, kPaneIconCount> kAllPaneIconIds{
    PaneIconId::Visible,
    PaneIconId::Hidden,
    PaneIconId::Editable,
    PaneIconId::Protected,
    PaneIconId::PinDocument,
    PaneIconId::ReturnToFollowing,
    PaneIconId::Previous,
    PaneIconId::Next,
    PaneIconId::Fit,
    PaneIconId::OneToOne,
};

[[nodiscard]] bool FluentIconResourceAvailable(HINSTANCE instance) noexcept;

[[nodiscard]] bool DrawToolIcon(
    HINSTANCE instance,
    HDC destination,
    RECT bounds,
    ToolIconId icon,
    COLORREF foreground) noexcept;

[[nodiscard]] bool DrawPaneIcon(
    HINSTANCE instance,
    HDC destination,
    RECT bounds,
    PaneIconId icon,
    COLORREF foreground) noexcept;

// Keeps the localized window text as the MSAA/UIA name while changing only
// the native button presentation to a DPI-scaled semantic icon. On any atlas
// or GDI resource failure the BS_ICON style is removed and the text is shown.
[[nodiscard]] bool SetPaneIconButton(
    HWND button, PaneIconId icon) noexcept;

// Creates a caller-owned DPI-scaled cursor from the same pinned Fluent atlas.
// The caller must destroy the returned cursor with DestroyCursor.
[[nodiscard]] HCURSOR CreateToolCursor(
    HINSTANCE instance,
    ToolIconId icon,
    UINT dpi) noexcept;

}  // namespace inkpod::windows::ui
