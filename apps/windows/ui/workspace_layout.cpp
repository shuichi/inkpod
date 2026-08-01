#include "workspace_layout.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kSettingsKey[] = L"Software\\Inkpod";
constexpr std::uint32_t kMagic = UINT32_C(0x4c574b49);
constexpr std::uint32_t kVersion = 3U;
constexpr int kReferenceDpi = 96;
constexpr int kTabsHeightDip = 28;

constexpr std::uint32_t kLegacyToolVisible = UINT32_C(1) << 0U;
constexpr std::uint32_t kLegacyToolOptionsVisible = UINT32_C(1) << 1U;
constexpr std::uint32_t kLegacyColorVisible = UINT32_C(1) << 2U;
constexpr std::uint32_t kLegacyLayerVisible = UINT32_C(1) << 3U;
constexpr std::uint32_t kLegacyMirrored = UINT32_C(1) << 4U;
constexpr std::uint32_t kLegacyKnownFlags = kLegacyToolVisible
    | kLegacyToolOptionsVisible | kLegacyColorVisible | kLegacyLayerVisible
    | kLegacyMirrored;

struct LegacyPersistedWorkspaceLayoutV2 {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t struct_size;
    std::uint32_t flags;
    std::int32_t tool_width_dip;
    std::int32_t inspector_width_dip;
    std::int32_t tool_options_height_dip;
    std::uint32_t color_split_milli;
    std::uint32_t layer_split_milli;
};

struct PersistedDockPane {
    std::uint32_t type;
    std::uint32_t zone;
    std::uint32_t restore_zone;
    std::uint32_t order;
    std::uint32_t split_weight;
    std::int32_t floating_x_dip;
    std::int32_t floating_y_dip;
    std::int32_t floating_width_dip;
    std::int32_t floating_height_dip;
};

struct PersistedDockZone {
    std::uint32_t mode;
    std::uint32_t active_tab;
    std::int32_t extent_dip;
};

struct PersistedWorkspaceLayoutV3 {
    std::uint32_t magic;
    std::uint32_t version;
    std::uint32_t struct_size;
    std::uint32_t flags;
    std::uint32_t pane_count;
    std::uint32_t zone_count;
    std::uint32_t layer_split_milli;
    std::uint32_t reserved;
    std::array<PersistedDockPane, kDockPaneCount> panes;
    std::array<PersistedDockZone, kDockedZoneCount> zones;
};

RECT ToRect(const DockRect& value) noexcept {
    return RECT{
        value.x,
        value.y,
        value.x + std::max(0, value.width),
        value.y + std::max(0, value.height)};
}

bool ValidLegacyLayout(const LegacyPersistedWorkspaceLayoutV2& value) noexcept {
    return value.magic == kMagic && value.version == 2U
        && value.struct_size == sizeof(value)
        && (value.flags & ~kLegacyKnownFlags) == 0U
        && value.tool_width_dip >= 80 && value.tool_width_dip <= 160
        && value.inspector_width_dip >= 240 && value.inspector_width_dip <= 640
        && value.tool_options_height_dip >= 28
        && value.tool_options_height_dip <= 96
        && value.color_split_milli >= 150U && value.color_split_milli <= 700U
        && value.layer_split_milli >= 200U && value.layer_split_milli <= 800U;
}

bool LoadLegacyLayout(
    WorkspaceLayoutState& state,
    const LegacyPersistedWorkspaceLayoutV2& value) noexcept {
    if (!ValidLegacyLayout(value)) {
        return false;
    }
    WorkspaceLayoutState candidate{};
    static_cast<void>(candidate.dock.SetZoneExtentDip(
        DockZone::Left, value.tool_width_dip));
    static_cast<void>(candidate.dock.SetZoneExtentDip(
        DockZone::Right, value.inspector_width_dip));
    static_cast<void>(candidate.dock.SetZoneExtentDip(
        DockZone::TopContext, value.tool_options_height_dip));
    DockPanePlacement* color = candidate.dock.Pane(DockPaneType::Color);
    DockPanePlacement* layer = candidate.dock.Pane(DockPaneType::Layer);
    if (color == nullptr || layer == nullptr) {
        return false;
    }
    color->split_weight = value.color_split_milli;
    layer->split_weight = 1000U - value.color_split_milli;
    if ((value.flags & kLegacyToolVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::Tool));
    }
    if ((value.flags & kLegacyToolOptionsVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::ToolOptions));
    }
    if ((value.flags & kLegacyColorVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::Color));
    }
    if ((value.flags & kLegacyLayerVisible) == 0U) {
        static_cast<void>(candidate.dock.HidePane(DockPaneType::Layer));
    }
    candidate.dock.SetMirrored((value.flags & kLegacyMirrored) != 0U);
    candidate.layer_split_milli = value.layer_split_milli;
    state = candidate;
    return true;
}

bool DecodeCurrentLayout(
    WorkspaceLayoutState& state,
    const PersistedWorkspaceLayoutV3& value) noexcept {
    if (value.magic != kMagic || value.version != kVersion
        || value.struct_size != sizeof(value) || value.flags > 1U
        || value.pane_count != kDockPaneCount
        || value.zone_count != kDockedZoneCount
        || value.layer_split_milli < 200U
        || value.layer_split_milli > 800U || value.reserved != 0U) {
        return false;
    }
    DockLayoutRecord record{};
    record.mirrored = value.flags;
    for (std::size_t index = 0U; index < value.panes.size(); ++index) {
        const PersistedDockPane& source = value.panes[index];
        record.panes[index] = DockPanePlacement{
            static_cast<DockPaneType>(source.type),
            static_cast<DockZone>(source.zone),
            static_cast<DockZone>(source.restore_zone),
            static_cast<std::uint8_t>(source.order),
            source.split_weight,
            DockFloatingPlacement{
                source.floating_x_dip,
                source.floating_y_dip,
                source.floating_width_dip,
                source.floating_height_dip},
            true};
    }
    for (std::size_t index = 0U; index < value.zones.size(); ++index) {
        const PersistedDockZone& source = value.zones[index];
        record.zones[index] = DockZoneState{
            static_cast<DockStackMode>(source.mode),
            static_cast<DockPaneType>(source.active_tab),
            source.extent_dip};
    }
    WorkspaceLayoutState candidate{};
    if (!candidate.dock.LoadRecord(record)) {
        return false;
    }
    candidate.layer_split_milli = value.layer_split_milli;
    state = candidate;
    return true;
}

PersistedWorkspaceLayoutV3 EncodeCurrentLayout(
    const WorkspaceLayoutState& state) noexcept {
    PersistedWorkspaceLayoutV3 value{};
    value.magic = kMagic;
    value.version = kVersion;
    value.struct_size = sizeof(value);
    value.flags = state.dock.Mirrored() ? 1U : 0U;
    value.pane_count = static_cast<std::uint32_t>(kDockPaneCount);
    value.zone_count = static_cast<std::uint32_t>(kDockedZoneCount);
    value.layer_split_milli = state.layer_split_milli;
    const DockLayoutRecord record = state.dock.ToRecord();
    for (std::size_t index = 0U; index < value.panes.size(); ++index) {
        const DockPanePlacement& source = record.panes[index];
        value.panes[index] = PersistedDockPane{
            static_cast<std::uint32_t>(source.type),
            static_cast<std::uint32_t>(source.zone),
            static_cast<std::uint32_t>(source.restore_zone),
            source.order,
            source.split_weight,
            source.floating.x_dip,
            source.floating.y_dip,
            source.floating.width_dip,
            source.floating.height_dip};
    }
    for (std::size_t index = 0U; index < value.zones.size(); ++index) {
        const DockZoneState& source = record.zones[index];
        value.zones[index] = PersistedDockZone{
            static_cast<std::uint32_t>(source.mode),
            static_cast<std::uint32_t>(source.active_tab),
            source.extent_dip};
    }
    return value;
}

}  // namespace

int ScaleWorkspaceDip(int value, UINT dpi) noexcept {
    return MulDiv(
        value,
        static_cast<int>(dpi == 0U ? kReferenceDpi : dpi),
        kReferenceDpi);
}

WorkspaceLayoutRects ComputeWorkspaceLayout(
    int client_width,
    int client_height,
    int status_height,
    UINT dpi,
    const WorkspaceLayoutState& state) noexcept {
    WorkspaceLayoutRects output{};
    const int available_height = std::max(
        0, client_height - std::max(0, status_height));
    output.dock = ComputeDockLayout(
        state.dock,
        std::max(0, client_width),
        available_height,
        dpi);
    output.editor = ToRect(output.dock.editor);
    const int tab_height = std::min(
        std::max(0, output.dock.editor.height),
        ScaleWorkspaceDip(kTabsHeightDip, dpi));
    output.document_tabs = RECT{
        output.editor.left,
        output.editor.top,
        output.editor.right,
        output.editor.top + tab_height};
    output.canvas = RECT{
        output.editor.left,
        output.editor.top + tab_height,
        output.editor.right,
        output.editor.bottom};
    return output;
}

void ResetWorkspaceLayout(WorkspaceLayoutState& state) noexcept {
    state = WorkspaceLayoutState{};
}

bool LoadWorkspaceLayout(
    WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept {
    if (value_name == nullptr || *value_name == L'\0') {
        return false;
    }
    std::array<BYTE, sizeof(PersistedWorkspaceLayoutV3)> bytes{};
    DWORD type{};
    DWORD size = static_cast<DWORD>(bytes.size());
    const LSTATUS status = RegGetValueW(
        HKEY_CURRENT_USER,
        kSettingsKey,
        value_name,
        RRF_RT_REG_BINARY,
        &type,
        bytes.data(),
        &size);
    if (status != ERROR_SUCCESS || type != REG_BINARY) {
        return false;
    }
    if (size == sizeof(PersistedWorkspaceLayoutV3)) {
        PersistedWorkspaceLayoutV3 value{};
        std::memcpy(&value, bytes.data(), sizeof(value));
        return DecodeCurrentLayout(state, value);
    }
    if (size == sizeof(LegacyPersistedWorkspaceLayoutV2)) {
        LegacyPersistedWorkspaceLayoutV2 value{};
        std::memcpy(&value, bytes.data(), sizeof(value));
        return LoadLegacyLayout(state, value);
    }
    return false;
}

bool SaveWorkspaceLayout(
    const WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept {
    if (value_name == nullptr || *value_name == L'\0'
        || state.layer_split_milli < 200U || state.layer_split_milli > 800U) {
        return false;
    }
    const PersistedWorkspaceLayoutV3 value = EncodeCurrentLayout(state);
    WorkspaceLayoutState validation{};
    if (!DecodeCurrentLayout(validation, value)) {
        return false;
    }
    HKEY key{};
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            kSettingsKey,
            0,
            nullptr,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            nullptr,
            &key,
            nullptr)
        != ERROR_SUCCESS) {
        return false;
    }
    const LSTATUS status = RegSetValueExW(
        key,
        value_name,
        0,
        REG_BINARY,
        reinterpret_cast<const BYTE*>(&value),
        sizeof(value));
    RegCloseKey(key);
    return status == ERROR_SUCCESS;
}

}  // namespace inkpod::windows::ui
