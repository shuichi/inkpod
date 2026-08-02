#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>
#include <string_view>

#include "dock_layout.h"

namespace inkpod::windows::ui {

enum class WorkspacePreset : std::uint8_t {
    Coloring,
    LineCleanup,
    ReferenceCheck,
    Batch,
    Focus,
    Custom,
    Count,
};

enum class WorkspaceDensity : std::uint8_t {
    Standard,
    Compact,
};

enum class WorkspaceSplitOrientation : std::uint8_t {
    None,
    Vertical,
    Horizontal,
};

enum class WorkspaceAuxiliaryPane : std::uint8_t {
    Locator,
    Sequence,
    LightTable,
    Reference,
    Batch,
    Count,
};

enum class WorkspaceAutoHideEdge : std::uint8_t {
    Left,
    Right,
    Bottom,
};

inline constexpr std::size_t kWorkspaceAuxiliaryPaneCount =
    static_cast<std::size_t>(WorkspaceAuxiliaryPane::Count);
inline constexpr std::size_t kWorkspacePresetNameCapacity = 64U;
inline constexpr std::size_t kMaximumWorkspaceLayoutRecordBytes = 8U * 1024U;
inline constexpr std::uint32_t kMaximumPersistedWorkspaceWindows = 8U;

struct WorkspaceScreenPlacement {
    // Screen coordinates and dimensions are physical device pixels. The
    // capture DPI records their origin and prevents a second DPI conversion.
    int x_px{};
    int y_px{};
    int width_px{};
    int height_px{};
    UINT capture_dpi{96U};
    bool valid{};
};

struct WorkspaceWindowPlacement : WorkspaceScreenPlacement {
    UINT show_command{SW_SHOWNORMAL};
};

struct WorkspaceAuxiliaryPaneState {
    WorkspaceAuxiliaryPane type{};
    std::uint32_t stable_type_id{};
    bool visible{};
    bool auto_hide{};
    WorkspaceAutoHideEdge edge{WorkspaceAutoHideEdge::Right};
    WorkspaceScreenPlacement floating{};
};

struct WorkspaceWorkArea {
    RECT bounds_px{};
    UINT dpi{96U};
    bool primary{};
};

enum class WorkspaceLayoutDecodeResult : std::uint8_t {
    Current,
    Migrated,
    Invalid,
};

struct WorkspaceLayoutState {
    DockLayoutModel dock{};
    // This is the internal layer/plane split inside the Layer pane, not a dock
    // geometry value.
    std::uint32_t layer_split_milli{550U};
    WorkspacePreset selected_preset{WorkspacePreset::Coloring};
    WorkspaceDensity density{WorkspaceDensity::Standard};
    WorkspaceSplitOrientation split_orientation{
        WorkspaceSplitOrientation::None};
    std::uint32_t split_ratio_milli{500U};
    WorkspaceWindowPlacement window{};
    std::array<WorkspaceAuxiliaryPaneState, kWorkspaceAuxiliaryPaneCount>
        auxiliary{{
            {WorkspaceAuxiliaryPane::Locator, UINT32_C(0x41434f4c)},
            {WorkspaceAuxiliaryPane::Sequence, UINT32_C(0x55514553)},
            {WorkspaceAuxiliaryPane::LightTable, UINT32_C(0x544c474c)},
            {WorkspaceAuxiliaryPane::Reference, UINT32_C(0x45464552)},
            {WorkspaceAuxiliaryPane::Batch, UINT32_C(0x48435442)},
        }};
    std::array<wchar_t, kWorkspacePresetNameCapacity> custom_name{};

    // Transient measurement only. These values are never persisted.
    int last_client_width{};
    int last_client_height{};
};

struct WorkspaceLayoutRects {
    DockLayoutGeometry dock{};
    RECT editor{};
    RECT document_tabs{};
    RECT canvas{};
    std::array<RECT, kWorkspaceAuxiliaryPaneCount> auto_hide_buttons{};
};

int ScaleWorkspaceDip(int value, UINT dpi) noexcept;

WorkspaceLayoutRects ComputeWorkspaceLayout(
    int client_width,
    int client_height,
    int status_height,
    UINT dpi,
    const WorkspaceLayoutState& state) noexcept;

void ResetWorkspaceLayout(WorkspaceLayoutState& state) noexcept;

[[nodiscard]] bool ApplyWorkspacePreset(
    WorkspaceLayoutState& state, WorkspacePreset preset) noexcept;
[[nodiscard]] const wchar_t* WorkspacePresetDisplayName(
    WorkspacePreset preset) noexcept;
[[nodiscard]] bool SetWorkspaceCustomName(
    WorkspaceLayoutState& state, std::wstring_view name) noexcept;
[[nodiscard]] WorkspaceAuxiliaryPaneState* FindWorkspaceAuxiliaryPane(
    WorkspaceLayoutState& state, WorkspaceAuxiliaryPane type) noexcept;
[[nodiscard]] const WorkspaceAuxiliaryPaneState* FindWorkspaceAuxiliaryPane(
    const WorkspaceLayoutState& state, WorkspaceAuxiliaryPane type) noexcept;

[[nodiscard]] bool EncodeWorkspaceLayout(
    const WorkspaceLayoutState& state,
    std::span<std::byte> output,
    std::size_t& written) noexcept;
[[nodiscard]] WorkspaceLayoutDecodeResult DecodeWorkspaceLayout(
    WorkspaceLayoutState& state,
    std::span<const std::byte> input) noexcept;

[[nodiscard]] bool ClampWorkspacePlacement(
    WorkspaceScreenPlacement& placement,
    std::span<const WorkspaceWorkArea> work_areas) noexcept;
void ClampWorkspaceFloatingPanes(
    WorkspaceLayoutState& state,
    std::span<const WorkspaceWorkArea> work_areas) noexcept;

[[nodiscard]] bool CaptureWorkspaceWindowPlacement(
    HWND window, WorkspaceLayoutState& state) noexcept;
[[nodiscard]] bool ApplyWorkspaceWindowPlacement(
    HWND window, WorkspaceLayoutState& state) noexcept;
[[nodiscard]] bool CaptureWorkspaceAuxiliaryPlacement(
    HWND window,
    WorkspaceLayoutState& state,
    WorkspaceAuxiliaryPane type) noexcept;
[[nodiscard]] bool ApplyWorkspaceAuxiliaryPlacement(
    HWND window,
    HWND owner,
    WorkspaceLayoutState& state,
    WorkspaceAuxiliaryPane type) noexcept;

bool LoadWorkspaceLayout(
    WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept;

bool SaveWorkspaceLayout(
    const WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept;
bool DeleteWorkspaceLayout(const wchar_t* value_name) noexcept;

[[nodiscard]] bool LoadWorkspaceWindowCount(std::uint32_t& count) noexcept;
[[nodiscard]] bool SaveWorkspaceWindowCount(std::uint32_t count) noexcept;

}  // namespace inkpod::windows::ui
