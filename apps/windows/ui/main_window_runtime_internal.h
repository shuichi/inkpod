#pragma once

#include <windows.h>

#include <optional>
#include <cstddef>
#include <string>

#include "app/command_context.h"
#include "ui/command_state.h"

namespace inkpod::app {
class ApplicationHost;
}

namespace inkpod::windows::ui::runtime {

app::CommandTargetScope TargetScopeForOwner(
    CommandStateOwner owner) noexcept;

std::optional<LRESULT> IssueCommand(
    app::ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM lparam,
    std::optional<app::PaneInstanceId> pane = std::nullopt) noexcept;

std::optional<LRESULT> RouteMainWindowMessage(
    app::ApplicationHost* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept;

std::optional<LRESULT> RouteKeyboardMessage(
    app::ApplicationHost* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept;

InkpodStatus CreateDefaultCellImpl(app::ApplicationHost& state) noexcept;
InkpodStatus OpenDocumentFromPathImpl(
    app::ApplicationHost& state, const std::wstring& path) noexcept;
InkpodStatus OpenRecoveryFromPathImpl(
    app::ApplicationHost& state, const std::wstring& path) noexcept;
bool CreateDocumentViewInGroup(
    app::ApplicationHost& state,
    app::EditorGroupId destination,
    HWND error_owner,
    std::optional<std::size_t> insertion_index = std::nullopt) noexcept;
bool MoveOrDuplicateViewToNewWorkspace(
    app::ApplicationHost& state,
    const app::CommandContext& context,
    bool duplicate,
    std::optional<POINT> drop_point = std::nullopt) noexcept;

#define INKPOD_DECLARE_COMMAND_ROUTE(name) \
    std::optional<LRESULT> name( \
        app::ApplicationHost*, HWND, WPARAM, LPARAM, \
        const app::CommandContext&) noexcept

INKPOD_DECLARE_COMMAND_ROUTE(RouteBatchCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteDocumentCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteEditCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteEffectsCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteDocumentPaneCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteAnimationCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteSelectionViewCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteToolCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteColorCommand);
INKPOD_DECLARE_COMMAND_ROUTE(RouteApplicationCommand);

#undef INKPOD_DECLARE_COMMAND_ROUTE

}  // namespace inkpod::windows::ui::runtime
