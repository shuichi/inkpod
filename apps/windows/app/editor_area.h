#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

#include "identity.h"

namespace inkpod::app {

enum class EditorSplitOrientation : std::uint8_t {
    None,
    Vertical,
    Horizontal,
};

struct EditorGroup final {
    static constexpr std::size_t kMaximumViews = 64U;

    EditorGroupId id{};
    CanvasId canvas_id{};
    Generation generation{};
    HWND document_tabs{};
    HWND canvas{};
    HWND focus_history{};

    [[nodiscard]] bool AddView(DocumentViewId view) noexcept;
    [[nodiscard]] bool InsertView(
        DocumentViewId view, std::size_t insertion_index) noexcept;
    [[nodiscard]] bool RemoveView(DocumentViewId view) noexcept;
    [[nodiscard]] bool ReorderView(
        DocumentViewId view, std::size_t insertion_index) noexcept;
    [[nodiscard]] bool ActivateView(DocumentViewId view) noexcept;
    void ClearViews() noexcept;
    [[nodiscard]] bool Contains(DocumentViewId view) const noexcept;
    [[nodiscard]] DocumentViewId ActiveView() const noexcept;
    [[nodiscard]] DocumentViewId ViewAt(std::size_t index) const noexcept;
    [[nodiscard]] std::optional<std::size_t> ViewIndex(
        DocumentViewId view) const noexcept;
    [[nodiscard]] std::size_t ViewCount() const noexcept;

private:
    std::array<DocumentViewId, kMaximumViews> views_{};
    std::size_t view_count_{};
    DocumentViewId active_view_{};
};

// UI-thread-owned editor layout. Document raster/history ownership remains in
// DocumentSession/CoreHost; groups own only view placement and visible HWNDs.
class EditorArea final {
public:
    static constexpr std::size_t kMaximumGroups = 2U;

    [[nodiscard]] bool Initialize(
        EditorGroupId group,
        CanvasId canvas,
        Generation generation) noexcept;
    [[nodiscard]] bool Split(
        EditorGroupId group,
        CanvasId canvas,
        Generation generation,
        EditorSplitOrientation orientation) noexcept;
    [[nodiscard]] bool SetOrientation(EditorSplitOrientation orientation) noexcept;
    [[nodiscard]] bool Activate(EditorGroupId group) noexcept;
    [[nodiscard]] bool AddView(EditorGroupId group, DocumentViewId view) noexcept;
    [[nodiscard]] bool MoveView(
        DocumentViewId view, EditorGroupId destination) noexcept;
    [[nodiscard]] bool MoveView(
        DocumentViewId view,
        EditorGroupId destination,
        std::size_t insertion_index) noexcept;
    [[nodiscard]] bool ReorderView(
        DocumentViewId view, std::size_t insertion_index) noexcept;
    [[nodiscard]] bool RemoveView(DocumentViewId view) noexcept;
    [[nodiscard]] bool ResetViews(DocumentViewId view) noexcept;
    [[nodiscard]] bool MergeAndRemove(
        EditorGroupId closing, EditorGroupId& survivor) noexcept;
    void Clear() noexcept;

    [[nodiscard]] EditorGroup* Active() noexcept;
    [[nodiscard]] const EditorGroup* Active() const noexcept;
    [[nodiscard]] EditorGroup* Find(EditorGroupId group) noexcept;
    [[nodiscard]] const EditorGroup* Find(EditorGroupId group) const noexcept;
    [[nodiscard]] EditorGroup* FindByView(DocumentViewId view) noexcept;
    [[nodiscard]] const EditorGroup* FindByView(DocumentViewId view) const noexcept;
    [[nodiscard]] EditorGroup* FindByCanvas(CanvasId canvas) noexcept;
    [[nodiscard]] const EditorGroup* FindByCanvas(CanvasId canvas) const noexcept;
    [[nodiscard]] EditorGroup* GroupAt(std::size_t index) noexcept;
    [[nodiscard]] const EditorGroup* GroupAt(std::size_t index) const noexcept;
    [[nodiscard]] EditorGroup* Other(EditorGroupId group) noexcept;
    [[nodiscard]] const EditorGroup* Other(EditorGroupId group) const noexcept;
    [[nodiscard]] std::size_t GroupCount() const noexcept;

    [[nodiscard]] EditorSplitOrientation Orientation() const noexcept;
    [[nodiscard]] std::uint32_t SplitRatioMilli() const noexcept;
    void SetSplitRatioMilli(std::uint32_t ratio) noexcept;

    HWND splitter{};
    POINT drag_start{};
    std::uint32_t drag_ratio_milli{500U};
    std::uint64_t last_drag_layout_tick{};

private:
    std::array<EditorGroup, kMaximumGroups> groups_{};
    std::size_t group_count_{};
    EditorGroupId active_group_{};
    EditorSplitOrientation orientation_{EditorSplitOrientation::None};
    std::uint32_t split_ratio_milli_{500U};
};

}  // namespace inkpod::app
