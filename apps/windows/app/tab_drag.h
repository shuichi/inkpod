#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>

#include "command_context.h"

namespace inkpod::app {

enum class TabDropKind : std::uint8_t {
    None,
    Reorder,
    EditorGroup,
    TearOut,
};

struct TabDropTarget final {
    TabDropKind kind{TabDropKind::None};
    WorkspaceWindowId workspace{};
    EditorGroupId group{};
    std::size_t insertion_index{};

    constexpr auto operator<=>(const TabDropTarget&) const noexcept = default;
};

struct TabDropRequest final {
    DragToken token{};
    CommandContext restore_context{};
    std::size_t source_index{};
    TabDropTarget target{};
};

// Process-wide, UI-thread-owned drag state. It stores only value IDs and never
// changes document/view placement until TakeDrop is called on button release.
class TabDragCoordinator final {
public:
    [[nodiscard]] bool Arm(
        DragToken token,
        CommandContext restore_context,
        std::size_t source_index,
        std::int32_t screen_x,
        std::int32_t screen_y) noexcept;
    [[nodiscard]] bool TryBegin(
        std::int32_t screen_x,
        std::int32_t screen_y,
        std::int32_t threshold_x,
        std::int32_t threshold_y) noexcept;
    [[nodiscard]] bool SetOperation(DragOperation operation) noexcept;
    [[nodiscard]] bool UpdateTarget(TabDropTarget target) noexcept;
    [[nodiscard]] std::optional<TabDropRequest> TakeDrop() noexcept;
    [[nodiscard]] bool Cancel() noexcept;

    [[nodiscard]] bool IsArmed() const noexcept;
    [[nodiscard]] bool IsDragging() const noexcept;
    [[nodiscard]] bool ReferencesWorkspace(WorkspaceWindowId workspace) const noexcept;
    [[nodiscard]] bool ReferencesGroup(EditorGroupId group) const noexcept;
    [[nodiscard]] const DragToken* Token() const noexcept;
    [[nodiscard]] const CommandContext* RestoreContext() const noexcept;
    [[nodiscard]] const TabDropTarget& Target() const noexcept;

private:
    DragToken token_{};
    CommandContext restore_context_{};
    std::size_t source_index_{};
    std::int32_t start_x_{};
    std::int32_t start_y_{};
    TabDropTarget target_{};
    bool armed_{};
    bool dragging_{};
};

}  // namespace inkpod::app
