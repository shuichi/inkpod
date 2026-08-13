#include "main_window_runtime_internal.h"

#include <array>

#include "app/application_host.h"
#include "dialogs/history_visualization_dialog.h"

namespace inkpod::windows::ui::runtime {

app::CommandTargetScope TargetScopeForOwner(
    CommandStateOwner owner) noexcept {
    return owner == CommandStateOwner::Workspace
            || owner == CommandStateOwner::Application
        ? app::kWorkspaceCommandScope
        : app::kDocumentViewCommandScope;
}

namespace {

std::optional<LRESULT> RouteMainWindowCommand(
    app::ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM lparam,
    const app::CommandContext& context) noexcept {
    using CommandRoute = std::optional<LRESULT> (*)(
        app::ApplicationHost*, HWND, WPARAM, LPARAM,
        const app::CommandContext&) noexcept;
    constexpr std::array<CommandRoute, 10U> routes{
        RouteBatchCommand,
        RouteDocumentCommand,
        RouteEditCommand,
        RouteEffectsCommand,
        RouteDocumentPaneCommand,
        RouteAnimationCommand,
        RouteSelectionViewCommand,
        RouteToolCommand,
        RouteColorCommand,
        RouteApplicationCommand};
    for (const CommandRoute route : routes) {
        if (const auto result = route(state, window, wparam, lparam, context)) {
            return result;
        }
    }
    return std::nullopt;
}

app::CommandTargetScope TargetScopeForCommand(
    const app::ApplicationHost& state,
    UINT command) noexcept {
    const auto* command_state = FindCommandState(
        state.Workspace().command_states, command);
    if (command_state == nullptr) {
        return app::CommandTargetScope::None;
    }
    return TargetScopeForOwner(command_state->owner);
}

std::optional<app::PaneInstanceId> PaneForCommandSource(
    const app::ApplicationHost& state,
    LPARAM lparam) noexcept {
    const HWND source = reinterpret_cast<HWND>(lparam);
    if (source == nullptr) {
        return std::nullopt;
    }
    const auto belongs_to = [source](HWND pane) noexcept {
        return pane != nullptr
            && (source == pane || IsChild(pane, source) != FALSE);
    };
    if (belongs_to(state.Workspace().windows.tool_palette)) {
        return state.routing.tool_pane;
    }
    if (belongs_to(state.Workspace().windows.tool_options)) {
        return state.routing.tool_options_pane;
    }
    if (belongs_to(state.Workspace().windows.color_pane)) {
        return state.routing.color_pane;
    }
    if (belongs_to(state.Workspace().windows.layer_palette)) {
        return state.routing.layer_pane;
    }
    if (belongs_to(state.Workspace().batch_palette)) {
        return state.routing.batch_pane;
    }
    if (belongs_to(state.Workspace().locator_palette)) {
        return state.routing.locator_pane;
    }
    if (belongs_to(state.Workspace().sequence_palette)) {
        return state.routing.sequence_pane;
    }
    if (belongs_to(state.Workspace().light_table_palette)) {
        return state.routing.light_table_pane;
    }
    if (belongs_to(state.Workspace().subpalette_palette)) {
        return state.routing.subpalette_pane;
    }
    return std::nullopt;
}

}  // namespace

std::optional<LRESULT> IssueCommand(
    app::ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM lparam,
    std::optional<app::PaneInstanceId> pane) noexcept {
    if (state == nullptr) {
        return LRESULT{0};
    }
    const UINT command = LOWORD(wparam);
    if (IsHistoryVisualizationCommand(command)) {
        return IssueHistoryVisualizationCommand(*state, window, command);
    }
    const auto* command_state = FindCommandState(
        state->Workspace().command_states, command);
    if (command_state == nullptr) {
        return LRESULT{0};
    }
    if (!pane.has_value()) {
        pane = PaneForCommandSource(*state, lparam);
    }
    app::CommandContext context = state->routing.targets.Capture(pane);
    if (pane.has_value()) {
        const app::PaneActionTarget target =
            state->routing.pane_targets.CaptureAction(
                pane.value(), context, state->routing.targets);
        if (target.status != app::PaneTargetStatus::Ok) {
            return LRESULT{0};
        }
        context = target.context;
    }
    const app::CommandTargetScope required =
        TargetScopeForCommand(*state, command);
    if (required == app::CommandTargetScope::None
        || state->routing.targets.Resolve(
               app::CommandRequest{command, context}, required)
            != app::CommandResolveStatus::Ok) {
        return LRESULT{0};
    }
    if (context.document_view.has_value()
        && context.document_view.value()
            != state->routing.targets.ActiveDocumentView()) {
        if (!state->ActivateDocumentView(context.document_view.value())) {
            return LRESULT{0};
        }
    } else if (context.workspace.has_value()
        && context.workspace.value() != state->routing.targets.Workspace()
        && !state->ActivateWorkspaceWindow(context.workspace.value(), false)) {
        return LRESULT{0};
    }
    return RouteMainWindowCommand(state, window, wparam, lparam, context)
        .value_or(0);
}

}  // namespace inkpod::windows::ui::runtime
