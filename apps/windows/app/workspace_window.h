#pragma once

#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "editor_area.h"
#include "frontend_state.h"
#include "ui/main_window.h"
#include "ui/panes/light_table_pane.h"
#include "ui/panes/locator_pane.h"
#include "ui/panes/sequence_pane.h"
#include "ui/panes/subpalette_pane.h"

namespace inkpod::app {

class ApplicationHost;

struct WorkspacePaneIds final {
    PaneInstanceId tool{};
    PaneInstanceId tool_options{};
    PaneInstanceId color{};
    PaneInstanceId layer{};
    PaneInstanceId batch{};
    PaneInstanceId locator{};
    PaneInstanceId sequence{};
    PaneInstanceId light_table{};
    PaneInstanceId reference{};
    PaneInstanceId subpalette{};
};

struct HistoryVisualizationMenuTarget final {
    DocumentSessionId session{};
    Generation generation{};
};

struct SubpaletteSourceCache final {
    std::uint64_t source_token{};
    std::uint64_t item_id{};
    InkpodCommonRasterFormat format{};
    std::wstring path;
    std::wstring name;
};

struct WorkspaceWindow final {
    ApplicationHost* application{};
    WorkspaceWindowId id{};
    Generation generation{};
    std::uint32_t persistence_slot{};
    WorkspacePaneIds pane_ids{};
    MainWindowHandles windows{};
    EditorArea editors{};
    ToolUiState tools{};
    PaneUiState panes{};
    AnimationUiState animation{};
    windows::ui::CommandStateSet command_states{};
    windows::ui::JobProgressState job_progress_state{};
    HWND batch_palette{};
    windows::ui::BatchPaletteDialogState batch_dialog{};
    bool workspace_presentation_pending{};
    std::uint64_t color_notice_sequence{};
    std::uint64_t batch_notice_sequence{};
    HWND locator_palette{};
    windows::ui::panes::LocatorPaneDialogState locator_dialog{};
    bool locator_fixed_mode{};
    bool locator_auto_scroll{true};
    std::uint64_t locator_notice_sequence{};
    HWND sequence_palette{};
    windows::ui::panes::SequencePaneDialogState sequence_dialog{};
    std::uint64_t sequence_notice_sequence{};
    HWND light_table_palette{};
    windows::ui::panes::LightTablePaneDialogState light_table_dialog{};
    std::uint64_t light_table_notice_sequence{};
    HWND subpalette_palette{};
    windows::ui::panes::SubpalettePaneDialogState subpalette_dialog{};
    CanvasId subpalette_canvas_id{};
    AuxiliarySourceId subpalette_source_id{};
    Generation subpalette_surface_generation{};
    InkpodSubpalette* subpalette{};
    std::vector<SubpaletteSourceCache> subpalette_sources;
    InkpodSubpaletteInfo subpalette_info{};
    std::uint32_t subpalette_navigation_index{};
    std::uint64_t subpalette_snapshot_revision{};
    std::uint64_t subpalette_presentation_epoch{};
    AuxiliarySourceId subpalette_scroll_reset_source{};
    Generation subpalette_scroll_reset_generation{};
    std::uint64_t subpalette_scroll_reset_token{};
    std::uint64_t next_subpalette_scroll_reset_token{};
    std::uint64_t subpalette_load_generation{};
    std::uint64_t subpalette_io_request{};
    InkpodSubpalette* subpalette_candidate{};
    bool subpalette_loading{};
    bool subpalette_sample_available{};
    InkpodColorValue subpalette_sample{};
    std::wstring subpalette_error;
    std::array<HistoryVisualizationMenuTarget, 64U>
        history_visualization_menu_targets{};
    std::size_t history_visualization_menu_target_count{};
};

class WorkspaceWindowRegistry final {
public:
    static constexpr std::size_t kMaximumWindows = 8U;

    [[nodiscard]] bool Initialize(
        ApplicationHost* application,
        WorkspaceWindowId id,
        EditorGroupId editor_group,
        CanvasId canvas,
        Generation generation) noexcept;
    [[nodiscard]] bool Add(
        ApplicationHost* application,
        WorkspaceWindowId id,
        EditorGroupId editor_group,
        CanvasId canvas,
        Generation generation,
        std::uint32_t persistence_slot) noexcept;
    [[nodiscard]] bool Activate(
        WorkspaceWindowId id, bool record_focus) noexcept;
    [[nodiscard]] bool Remove(WorkspaceWindowId id) noexcept;
    void Clear() noexcept;
    [[nodiscard]] WorkspaceWindow* Current() noexcept;
    [[nodiscard]] const WorkspaceWindow* Current() const noexcept;
    [[nodiscard]] WorkspaceWindow* LastFocused() noexcept;
    [[nodiscard]] const WorkspaceWindow* LastFocused() const noexcept;
    [[nodiscard]] WorkspaceWindow* Find(WorkspaceWindowId id) noexcept;
    [[nodiscard]] const WorkspaceWindow* Find(
        WorkspaceWindowId id) const noexcept;
    [[nodiscard]] WorkspaceWindow* At(std::size_t index) noexcept;
    [[nodiscard]] const WorkspaceWindow* At(std::size_t index) const noexcept;
    [[nodiscard]] std::size_t Count() const noexcept;

private:
    std::array<std::unique_ptr<WorkspaceWindow>, kMaximumWindows> windows_{};
    std::size_t count_{};
    WorkspaceWindowId current_{};
    WorkspaceWindowId last_focused_{};
};

}  // namespace inkpod::app
