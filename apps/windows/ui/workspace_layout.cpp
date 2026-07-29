#include "workspace_layout.h"

#include <algorithm>
#include <cstdint>

namespace inkpod::windows::ui {
namespace {

constexpr wchar_t kSettingsKey[] = L"Software\\Inkpod";
constexpr std::uint32_t kMagic = UINT32_C(0x4c574b49);
constexpr std::uint32_t kVersion = 2U;
constexpr int kReferenceDpi = 96;
constexpr int kSplitterDip = 4;
constexpr int kTabsHeightDip = 28;
constexpr int kMinimumCanvasWidthDip = 320;
constexpr int kMinimumToolWidthDip = 80;
constexpr int kMaximumToolWidthDip = 160;
constexpr int kMinimumInspectorWidthDip = 240;
constexpr int kMaximumInspectorWidthDip = 640;
constexpr int kMinimumColorHeightDip = 120;
constexpr int kMinimumLayerHeightDip = 180;

constexpr std::uint32_t kToolVisible = UINT32_C(1) << 0U;
constexpr std::uint32_t kToolOptionsVisible = UINT32_C(1) << 1U;
constexpr std::uint32_t kColorVisible = UINT32_C(1) << 2U;
constexpr std::uint32_t kLayerVisible = UINT32_C(1) << 3U;
constexpr std::uint32_t kMirrored = UINT32_C(1) << 4U;
constexpr std::uint32_t kKnownFlags = kToolVisible | kToolOptionsVisible
    | kColorVisible | kLayerVisible | kMirrored;

struct PersistedWorkspaceLayout {
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

bool ValidPersistedLayout(const PersistedWorkspaceLayout& value) noexcept {
    return value.magic == kMagic && value.version == kVersion
        && value.struct_size == sizeof(value) && (value.flags & ~kKnownFlags) == 0U
        && value.tool_width_dip >= kMinimumToolWidthDip
        && value.tool_width_dip <= kMaximumToolWidthDip
        && value.inspector_width_dip >= kMinimumInspectorWidthDip
        && value.inspector_width_dip <= kMaximumInspectorWidthDip
        && value.tool_options_height_dip >= 28
        && value.tool_options_height_dip <= 96
        && value.color_split_milli >= 150U && value.color_split_milli <= 700U
        && value.layer_split_milli >= 200U && value.layer_split_milli <= 800U;
}

RECT MakeRect(int left, int top, int width, int height) noexcept {
    return RECT{left, top, left + std::max(0, width), top + std::max(0, height)};
}

}  // namespace

int ScaleWorkspaceDip(int value, UINT dpi) noexcept {
    return MulDiv(value, static_cast<int>(dpi == 0U ? kReferenceDpi : dpi), kReferenceDpi);
}

WorkspaceLayoutRects ComputeWorkspaceLayout(
    int client_width,
    int client_height,
    int status_height,
    UINT dpi,
    const WorkspaceLayoutState& state) noexcept {
    WorkspaceLayoutRects output{};
    client_width = std::max(0, client_width);
    client_height = std::max(0, client_height - std::max(0, status_height));
    const int splitter = ScaleWorkspaceDip(kSplitterDip, dpi);
    const int tabs_height = ScaleWorkspaceDip(kTabsHeightDip, dpi);
    const int options_height = state.tool_options_visible
        ? ScaleWorkspaceDip(state.tool_options_height_dip, dpi)
        : 0;
    if (state.tool_options_visible) {
        output.tool_options = MakeRect(0, 0, client_width, options_height);
    }

    const int body_top = options_height;
    const int body_height = std::max(0, client_height - body_top);
    int tool_width = state.tool_visible
        ? ScaleWorkspaceDip(
              std::clamp(
                  state.tool_width_dip,
                  kMinimumToolWidthDip,
                  kMaximumToolWidthDip),
              dpi)
        : 0;
    int inspector_width = (state.color_visible || state.layer_visible)
        ? ScaleWorkspaceDip(
              std::clamp(
                  state.inspector_width_dip,
                  kMinimumInspectorWidthDip,
                  kMaximumInspectorWidthDip),
              dpi)
        : 0;
    const int tool_gap = tool_width > 0 ? splitter : 0;
    const int inspector_gap = inspector_width > 0 ? splitter : 0;
    const int minimum_canvas = ScaleWorkspaceDip(kMinimumCanvasWidthDip, dpi);
    const int available_sides = std::max(0, client_width - minimum_canvas);
    if (tool_width + tool_gap + inspector_width + inspector_gap > available_sides) {
        inspector_width = std::max(
            0,
            std::min(
                inspector_width,
                available_sides - tool_width - tool_gap - inspector_gap));
    }
    if (inspector_width > 0
        && inspector_width < ScaleWorkspaceDip(kMinimumInspectorWidthDip, dpi)) {
        inspector_width = 0;
    }

    int center_left{};
    int center_right = client_width;
    int inspector_left{};
    if (!state.mirrored) {
        if (tool_width > 0) {
            output.tool = MakeRect(0, body_top, tool_width, body_height);
            output.tool_splitter = MakeRect(
                tool_width, body_top, splitter, body_height);
            center_left = tool_width + splitter;
        }
        if (inspector_width > 0) {
            inspector_left = client_width - inspector_width;
            output.inspector_splitter = MakeRect(
                inspector_left - splitter, body_top, splitter, body_height);
            center_right = inspector_left - splitter;
        }
    } else {
        if (inspector_width > 0) {
            inspector_left = 0;
            output.inspector_splitter = MakeRect(
                inspector_width, body_top, splitter, body_height);
            center_left = inspector_width + splitter;
        }
        if (tool_width > 0) {
            const int tool_left = client_width - tool_width;
            output.tool_splitter = MakeRect(
                tool_left - splitter, body_top, splitter, body_height);
            output.tool = MakeRect(tool_left, body_top, tool_width, body_height);
            center_right = tool_left - splitter;
        }
    }

    const int center_width = std::max(0, center_right - center_left);
    output.document_tabs = MakeRect(
        center_left, body_top, center_width, std::min(tabs_height, body_height));
    output.canvas = MakeRect(
        center_left,
        body_top + std::min(tabs_height, body_height),
        center_width,
        std::max(0, body_height - tabs_height));

    if (inspector_width <= 0) {
        return output;
    }
    const int inspector_top = body_top;
    if (state.color_visible && state.layer_visible) {
        const int minimum_color = ScaleWorkspaceDip(kMinimumColorHeightDip, dpi);
        const int minimum_layer = ScaleWorkspaceDip(kMinimumLayerHeightDip, dpi);
        const int available_height = std::max(0, body_height - splitter);
        int color_height = static_cast<int>(
            static_cast<std::int64_t>(available_height)
            * std::clamp<std::uint32_t>(state.color_split_milli, 150U, 700U)
            / 1000);
        if (available_height >= minimum_color + minimum_layer) {
            color_height = std::clamp(
                color_height, minimum_color, available_height - minimum_layer);
        } else {
            color_height = std::min(available_height, minimum_color);
        }
        output.color = MakeRect(
            inspector_left, inspector_top, inspector_width, color_height);
        output.color_splitter = MakeRect(
            inspector_left,
            inspector_top + color_height,
            inspector_width,
            splitter);
        output.layer = MakeRect(
            inspector_left,
            inspector_top + color_height + splitter,
            inspector_width,
            std::max(0, available_height - color_height));
    } else if (state.color_visible) {
        output.color = MakeRect(
            inspector_left, inspector_top, inspector_width, body_height);
    } else if (state.layer_visible) {
        output.layer = MakeRect(
            inspector_left, inspector_top, inspector_width, body_height);
    }
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
    PersistedWorkspaceLayout value{};
    DWORD type{};
    DWORD size = sizeof(value);
    const LSTATUS status = RegGetValueW(
        HKEY_CURRENT_USER,
        kSettingsKey,
        value_name,
        RRF_RT_REG_BINARY,
        &type,
        &value,
        &size);
    if (status != ERROR_SUCCESS || type != REG_BINARY || size != sizeof(value)
        || !ValidPersistedLayout(value)) {
        return false;
    }
    state.tool_visible = (value.flags & kToolVisible) != 0U;
    state.tool_options_visible = (value.flags & kToolOptionsVisible) != 0U;
    state.color_visible = (value.flags & kColorVisible) != 0U;
    state.layer_visible = (value.flags & kLayerVisible) != 0U;
    state.mirrored = (value.flags & kMirrored) != 0U;
    state.tool_width_dip = value.tool_width_dip;
    state.inspector_width_dip = value.inspector_width_dip;
    state.tool_options_height_dip = value.tool_options_height_dip;
    state.color_split_milli = value.color_split_milli;
    state.layer_split_milli = value.layer_split_milli;
    return true;
}

bool SaveWorkspaceLayout(
    const WorkspaceLayoutState& state,
    const wchar_t* value_name) noexcept {
    if (value_name == nullptr || *value_name == L'\0') {
        return false;
    }
    PersistedWorkspaceLayout value{};
    value.magic = kMagic;
    value.version = kVersion;
    value.struct_size = sizeof(value);
    value.flags = (state.tool_visible ? kToolVisible : 0U)
        | (state.tool_options_visible ? kToolOptionsVisible : 0U)
        | (state.color_visible ? kColorVisible : 0U)
        | (state.layer_visible ? kLayerVisible : 0U)
        | (state.mirrored ? kMirrored : 0U);
    value.tool_width_dip = state.tool_width_dip;
    value.inspector_width_dip = state.inspector_width_dip;
    value.tool_options_height_dip = state.tool_options_height_dip;
    value.color_split_milli = state.color_split_milli;
    value.layer_split_milli = state.layer_split_milli;
    if (!ValidPersistedLayout(value)) {
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
