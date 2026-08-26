#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>

#include "dock_layout.h"
#include "right_tool_tabs.h"

namespace inkpod::windows::ui {

enum class DockHostChangeKind : std::uint8_t {
    Geometry,
    StackBoundary,
    Structure,
};

using DockHostChangedCallback = void (*)(
    void* context, DockHostChangeKind kind) noexcept;

class DockHost final {
public:
    DockHost() noexcept;
    ~DockHost() noexcept;
    DockHost(const DockHost&) = delete;
    DockHost& operator=(const DockHost&) = delete;

    [[nodiscard]] bool Initialize(
        HWND owner,
        HINSTANCE instance,
        DockLayoutModel& model,
        RightToolTabsModel& right_tool_tabs) noexcept;
    void SetChangedCallback(
        DockHostChangedCallback callback, void* context) noexcept;
    [[nodiscard]] bool AttachPane(DockPaneType type, HWND content) noexcept;
    void ApplyLayout(
        const DockLayoutGeometry& geometry,
        UINT dpi,
        DockHostChangeKind kind = DockHostChangeKind::Structure) noexcept;

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
    [[nodiscard]] HWND ToolTabWindow() const noexcept {
        return right_tool_tab_control_;
    }
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
        bool focused{};
    };

    struct TabHostState {
        DockHost* host{};
        DockZone zone{DockZone::TopContext};
        std::uint8_t stack{};
        HWND control{};
    };

    struct ToolTabCloseButtonSlot {
        DockHost* host{};
        ToolTabId tab{};
        HWND button{};
        bool hovered{};
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
    static LRESULT CALLBACK ToolTabSubclassProcedure(
        HWND window,
        UINT message,
        WPARAM wparam,
        LPARAM lparam,
        UINT_PTR subclass_id,
        DWORD_PTR reference) noexcept;
    static LRESULT CALLBACK ToolTabCloseButtonSubclassProcedure(
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
    [[nodiscard]] bool ShouldShowPaneHeader(
        DockPaneType type) const noexcept;
    [[nodiscard]] bool PaneInSelectedToolTab(
        DockPaneType type) const noexcept;
    void SelectVisibleToolTabForPane(DockPaneType type) noexcept;
    void RemovePaneFromToolTabs(DockPaneType type) noexcept;
    void FocusPane(DockPaneType type) noexcept;
    [[nodiscard]] bool UpdateTabFont(UINT dpi) noexcept;
    void ApplyPaneLayout(PaneHostState& pane) noexcept;
    void ApplyTabLayout(TabHostState& tabs, bool synchronize_items) noexcept;
    void ApplyToolTabLayout(bool synchronize_items) noexcept;
    [[nodiscard]] ToolTabCloseButtonSlot* FindToolTabCloseButton(
        ToolTabId tab) noexcept;
    [[nodiscard]] ToolTabCloseButtonSlot* FindToolTabCloseButton(
        HWND button) noexcept;
    [[nodiscard]] HWND CreateToolTabCloseButton(
        ToolTabCloseButtonSlot& slot) noexcept;
    void DestroyToolTabCloseButton(ToolTabCloseButtonSlot& slot) noexcept;
    [[nodiscard]] bool SynchronizeToolTabCloseButtons() noexcept;
    void LayoutToolTabCloseButtons() noexcept;
    [[nodiscard]] bool DrawToolTabCloseButton(
        const DRAWITEMSTRUCT& draw) noexcept;
    void RepaintChangedStackBoundaries(
        const DockLayoutGeometry& previous) noexcept;
    void NotifyChanged(
        DockHostChangeKind kind = DockHostChangeKind::Structure) noexcept;
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
    [[nodiscard]] DockResult AdjustRightPaneBoundary(
        DockPaneType first,
        DockPaneType second,
        int delta_milli) noexcept;
    void ActivateSelectedTab(TabHostState& tabs) noexcept;
    void ActivateSelectedToolTab() noexcept;
    [[nodiscard]] ToolTabResult MovePaneToToolTab(
        DockPaneType type, ToolTabId destination) noexcept;
    [[nodiscard]] ToolTabResult MovePaneToNewToolTab(
        DockPaneType type) noexcept;
    [[nodiscard]] ToolTabResult CloseToolTab(ToolTabId tab) noexcept;
    [[nodiscard]] std::array<DockPaneType, kDockPaneCount>
    SelectedRightDockedPanes(std::size_t& count) const noexcept;

    HWND owner_{};
    HINSTANCE instance_{};
    DockLayoutModel* model_{};
    RightToolTabsModel* right_tool_tabs_{};
    DockHostChangedCallback changed_{};
    void* changed_context_{};
    UINT dpi_{96U};
    DockLayoutGeometry geometry_{};
    std::array<PaneHostState, kDockPaneCount> panes_{};
    std::array<SplitterHostState, kMaximumDockSplitters> splitter_states_{};
    std::array<HWND, kMaximumDockSplitters> splitters_{};
    std::array<TabHostState, kMaximumDockTabStacks> tab_states_{};
    HWND right_tool_tab_control_{};
    std::array<ToolTabCloseButtonSlot, kMaximumToolTabs>
        tool_tab_close_buttons_{};
    ToolTabId dragging_tool_tab_{};
    POINT tool_tab_drag_origin_{};
    bool tool_tab_drag_active_{};
    HFONT tab_font_{};
    UINT tab_font_dpi_{};
    HWND preview_{};
    DockZone preview_zone_{DockZone::Count};
    bool initialized_{};
    bool applying_{};
};

}  // namespace inkpod::windows::ui
