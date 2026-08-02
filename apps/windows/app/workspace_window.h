#pragma once

#include <memory>

#include "editor_area.h"
#include "frontend_state.h"
#include "ui/main_window.h"
#include "ui/panes/light_table_pane.h"
#include "ui/panes/locator_pane.h"
#include "ui/panes/sequence_pane.h"
#include "ui/panes/subpalette_pane.h"

namespace inkpod::app {

class ApplicationHost;

struct WorkspaceWindow final {
    ApplicationHost* application{};
    WorkspaceWindowId id{};
    Generation generation{};
    MainWindowHandles windows{};
    EditorArea editors{};
    ToolUiState tools{};
    PaneUiState panes{};
    AnimationUiState animation{};
    windows::ui::CommandStateSet command_states{};
    HWND effects_progress{};
    HWND batch_progress{};
    HWND batch_palette{};
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
    Generation subpalette_surface_generation{};
    DocumentSessionId subpalette_session{};
    DocumentViewId subpalette_document_view{};
    Generation subpalette_document_generation{};
    std::uint64_t subpalette_core_view_id{};
    std::uint64_t subpalette_snapshot_revision{};
    std::uint32_t subpalette_source_index{};
    std::uint32_t subpalette_source_count{};
    std::uint32_t subpalette_active_index{};
    bool subpalette_auto_previous{true};
    bool subpalette_scroll_sync{};
    std::uint64_t subpalette_notice_sequence{};
};

class WorkspaceWindowRegistry final {
public:
    [[nodiscard]] bool Initialize(
        ApplicationHost* application,
        WorkspaceWindowId id,
        EditorGroupId editor_group,
        CanvasId canvas,
        Generation generation) noexcept;
    void Clear() noexcept;
    [[nodiscard]] WorkspaceWindow* Current() noexcept;
    [[nodiscard]] const WorkspaceWindow* Current() const noexcept;

private:
    std::unique_ptr<WorkspaceWindow> current_;
};

}  // namespace inkpod::app
