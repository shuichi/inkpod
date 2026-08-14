#pragma once

#include <windows.h>

#include <array>
#include <cstddef>

#include "dock_layout.h"

namespace inkpod::windows::ui {

using DockHostChangedCallback = void (*)(void* context) noexcept;

class DockHost final {
public:
    DockHost() noexcept;
    ~DockHost() noexcept;
    DockHost(const DockHost&) = delete;
    DockHost& operator=(const DockHost&) = delete;

    [[nodiscard]] bool Initialize(
        HWND owner, HINSTANCE instance, DockLayoutModel& model) noexcept;
    void SetChangedCallback(
        DockHostChangedCallback callback, void* context) noexcept;
    [[nodiscard]] bool AttachPane(DockPaneType type, HWND content) noexcept;
    void ApplyLayout(const DockLayoutGeometry& geometry, UINT dpi) noexcept;

    [[nodiscard]] DockResult TogglePane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult DockPane(
        DockPaneType type, DockZone zone) noexcept;
    [[nodiscard]] DockResult FloatPane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult HidePane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult SetPaneAutoHide(
        DockPaneType type, bool auto_hide) noexcept;
    [[nodiscard]] DockResult RestorePane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult ResetPane(DockPaneType type) noexcept;
    [[nodiscard]] DockResult SetZoneMode(
        DockZone zone, DockStackMode mode) noexcept;
    [[nodiscard]] DockResult ActivatePane(DockPaneType type) noexcept;

    [[nodiscard]] HWND FloatingWindow(DockPaneType type) const noexcept;
    [[nodiscard]] HWND ContentWindow(DockPaneType type) const noexcept;
    [[nodiscard]] HWND HeaderWindow(DockPaneType type) const noexcept;
    [[nodiscard]] HWND TabWindow(DockZone zone) const noexcept;
    [[nodiscard]] HWND SplitterWindow(
        DockZone zone, DockSplitterKind kind) const noexcept;
    [[nodiscard]] bool PreviewVisible() const noexcept;
    [[nodiscard]] bool ShowAutoHiddenPane(
        DockPaneType type, DockZone edge) noexcept;
    [[nodiscard]] bool AutoHiddenPaneVisible(DockPaneType type) const noexcept;
    void HideAutoHiddenPane(DockPaneType type) noexcept;

private:
    struct PaneHostState {
        DockHost* host{};
        DockPaneType type{DockPaneType::Count};
        HWND content{};
        HWND floating_window{};
        DockZone auto_hide_edge{DockZone::Right};
        bool auto_hide_expanded{};
    };

    struct SplitterHostState {
        DockHost* host{};
        DockSplitterGeometry geometry{};
        POINT last_screen{};
        bool accessible_name_set{};
        bool hovered{};
    };

    struct TabHostState {
        DockHost* host{};
        DockZone zone{DockZone::TopContext};
        std::uint8_t stack{};
        HWND control{};
    };

    static LRESULT CALLBACK FloatingWindowProcedure(
        HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept;
    static LRESULT CALLBACK PaneSubclassProcedure(
        HWND window,
        UINT message,
        WPARAM wparam,
        LPARAM lparam,
        UINT_PTR subclass_id,
        DWORD_PTR reference) noexcept;
    static LRESULT CALLBACK SplitterSubclassProcedure(
        HWND window,
        UINT message,
        WPARAM wparam,
        LPARAM lparam,
        UINT_PTR subclass_id,
        DWORD_PTR reference) noexcept;
    static LRESULT CALLBACK TabSubclassProcedure(
        HWND window,
        UINT message,
        WPARAM wparam,
        LPARAM lparam,
        UINT_PTR subclass_id,
        DWORD_PTR reference) noexcept;

    [[nodiscard]] static std::size_t PaneIndex(DockPaneType type) noexcept;
    [[nodiscard]] PaneHostState* PaneState(DockPaneType type) noexcept;
    [[nodiscard]] const PaneHostState* PaneState(DockPaneType type) const noexcept;
    [[nodiscard]] bool EnsureFloatingWindow(PaneHostState& pane) noexcept;
    void LayoutFloatingContent(PaneHostState& pane) noexcept;
    void LayoutAutoHiddenContent(PaneHostState& pane) noexcept;
    [[nodiscard]] bool ShouldShowStackHeader(
        DockZone zone, std::uint8_t stack) const noexcept;
    [[nodiscard]] bool UpdateTabFont(UINT dpi) noexcept;
    void ApplyPaneLayout(PaneHostState& pane) noexcept;
    void ApplyTabLayout(TabHostState& tabs) noexcept;
    void NotifyChanged() noexcept;
    void ShowContextMenu(DockPaneType type, POINT screen) noexcept;
    [[nodiscard]] DockZone PreviewZoneAt(
        DockPaneType type, POINT screen) const noexcept;
    void ShowDockPreview(DockPaneType type, POINT screen) noexcept;
    void HideDockPreview() noexcept;
    void FinishFloatingMove(PaneHostState& pane) noexcept;
    void CaptureFloatingPlacement(PaneHostState& pane) noexcept;
    void UpdateZoneExtentFromPoint(
        const SplitterHostState& splitter, POINT screen) noexcept;
    void UpdateStackBoundaryFromPoint(
        SplitterHostState& splitter, POINT screen) noexcept;
    void ActivateSelectedTab(TabHostState& tabs) noexcept;

    HWND owner_{};
    HINSTANCE instance_{};
    DockLayoutModel* model_{};
    DockHostChangedCallback changed_{};
    void* changed_context_{};
    UINT dpi_{96U};
    DockLayoutGeometry geometry_{};
    std::array<PaneHostState, kDockPaneCount> panes_{};
    std::array<SplitterHostState, kMaximumDockSplitters> splitter_states_{};
    std::array<HWND, kMaximumDockSplitters> splitters_{};
    std::array<TabHostState, kMaximumDockTabStacks> tab_states_{};
    HFONT tab_font_{};
    UINT tab_font_dpi_{};
    HWND preview_{};
    DockZone preview_zone_{DockZone::Count};
    bool initialized_{};
    bool applying_{};
};

}  // namespace inkpod::windows::ui
