#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <objbase.h>
#include <shlobj.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <climits>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cwchar>
#include <cwctype>
#include <functional>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "about_icon.h"
#include "canvas.h"
#include "core_engine.h"
#include "inkpod/core_ffi.h"
#include "resource.h"

int InkpodRunAbiSmoke();

namespace {

constexpr std::uint32_t kInteractionFill = 1001U;
constexpr std::uint32_t kInteractionEyedropper = 1002U;
constexpr std::uint32_t kInteractionBoxZoom = 1003U;
constexpr std::uint32_t kInteractionGuideMove = 1004U;
constexpr std::uint32_t kInteractionSelection = 1005U;
constexpr std::uint32_t kInteractionFloatingTransform = 1006U;
constexpr std::uint32_t kInteractionLightTableMove = 1007U;
constexpr std::uint32_t kInteractionM6Gradient = 1101U;
constexpr std::uint32_t kInteractionM6Airbrush = 1102U;
constexpr std::uint32_t kInteractionM6Blur = 1103U;
constexpr std::uint32_t kInteractionM6Stamp = 1104U;
constexpr std::uint32_t kInteractionM6Dust = 1105U;
constexpr std::uint32_t kInteractionM6AlphaGradient = 1106U;
constexpr std::uint32_t kInteractionVectorLine = 1201U;
constexpr std::uint32_t kInteractionVectorCurve = 1202U;
constexpr std::uint32_t kInteractionVectorRectangle = 1203U;
constexpr std::uint32_t kInteractionVectorEllipse = 1204U;
constexpr std::uint32_t kInteractionVectorPolyline = 1205U;
constexpr std::uint32_t kInteractionVectorEraser = 1206U;
constexpr UINT_PTR kAutosaveTimer = 1U;
constexpr UINT kAutosaveIntervalMilliseconds = 60U * 1000U;
constexpr UINT kM6TaskCompleted = WM_APP + 0x170U;
constexpr UINT_PTR kM6ProgressTimer = 1U;
constexpr UINT_PTR kM6ContinuousSprayTimer = 2U;
constexpr UINT_PTR kMotionPlaybackTimer = 3U;
constexpr UINT kM6ContinuousSprayIntervalMilliseconds = 50U;
constexpr wchar_t kVectorStrokePlaneRequired[] =
    L"ベクター描画には、ベクター主線または色トレース線プレーンの選択が必要です。";

struct M6StopValue {
    std::uint32_t position_milli{};
    std::uint32_t rgba{};
};

struct M6ToolOptions {
    std::array<std::int32_t, 5U> parameters{};
    std::uint32_t shape{INKPOD_SELECTION_TRACE};
    std::uint32_t mode{};
    bool option{};
    bool option2{};
    std::vector<M6StopValue> stops;
};

struct M6FilterJob {
    std::uint32_t kind{INKPOD_FILTER_INVERT};
    std::uint32_t channel{INKPOD_FILTER_CHANNEL_RGB};
    std::uint32_t interpolation{INKPOD_CURVE_BEZIER};
    std::array<std::int32_t, 5U> parameters{};
    std::vector<InkpodCurvePoint> points;
    std::uint64_t plane_id{};
    bool preview{};
};

struct M6AdjustmentUiState {
    std::uint64_t id{};
    bool visible{true};
    M6FilterJob job;
    std::string name;
};

struct FillToolOptions {
    InkpodFillOperation operation{INKPOD_FILL_SEED};
    std::uint16_t tolerance{};
    std::uint16_t gap_close{};
    std::uint32_t extension_distance{1U};
    InkpodInclusionMode inclusion_mode{INKPOD_INCLUSION_NONE};
    std::vector<std::uint32_t> inclusion_rgba;
    bool overflow_abort{true};
    bool detached_regions{};
    bool transparent_only{};
    bool use_document_selection{};
    bool light_table_boundary{};
    bool light_table_color{};
};

struct AppState {
    HINSTANCE instance{};
    HWND window{};
    HWND canvas{};
    HWND toolbar{};
    HWND zoom_slider{};
    HWND status_bar{};
    HWND document_tabs{};
    HWND locator_label{};
    HWND layer_list{};
    HWND plane_list{};
    HWND light_table_set_list{};
    HWND light_table_item_list{};
    HWND sequence_list{};
    HWND motion_label{};
    HWND color_palette_list{};
    HWND color_chart_list{};
    std::unique_ptr<inkpod::app::CoreEngine> engine;
    std::uint32_t tool{INKPOD_TOOL_PENCIL};
    InkpodPlaneKind plane{INKPOD_PLANE_MAIN_LINE};
    std::uint32_t color_rgba{UINT32_C(0xdc281eff)};
    InkpodColorValue drawing_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 220U, 40U, 30U, 255U};
    InkpodEyedropperSource eyedropper_source{INKPOD_EYEDROPPER_COMPOSITE};
    std::vector<InkpodColorValue> palette_colors;
    std::uint32_t palette_group{};
    std::vector<std::wstring> color_chart_names;
    std::uint32_t color_chart_page{};
    bool color_chart_locked{};
    float diameter{8.0F};
    std::wstring current_path;
    std::wstring recovery_path;
    std::wstring smoke_raster_path;
    std::vector<std::wstring> smoke_sequence_paths;
    InkpodColorCheckMode color_check_mode{INKPOD_COLOR_CHECK_OFF};
    InkpodClipboard* clipboard{};
    bool floating_active{};
    InkpodFloatingTransform floating_transform{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};
    InkpodFrameRect floating_bounds{};
    std::vector<InkpodStrokeSample> floating_gesture_samples;
    std::uint32_t floating_drag_mode{};
    InkpodFloatingTransform floating_drag_start{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};
    std::uint64_t m3_layer_id{};
    std::uint64_t selection_layer_id{};
    std::uint64_t active_tree_layer_id{};
    std::uint64_t active_tree_plane_id{};
    std::uint32_t active_tree_layer_index{};
    std::uint32_t active_tree_plane_index{};
    std::uint64_t active_light_table_set_id{};
    std::uint64_t active_light_table_item_id{};
    std::uint32_t active_light_table_set_index{};
    std::uint32_t active_light_table_item_index{};
    std::vector<InkpodStrokeSample> light_table_move_samples;
    std::uint32_t active_sequence_index{};
    std::uint32_t motion_fps{24U};
    std::uint64_t motion_flags{INKPOD_MOTION_FLAG_LOOP};
    bool motion_active{};
    bool motion_paused{};
    std::uint64_t secondary_view_id{};
    std::uint64_t active_view_id{};
    std::uint64_t m6_adjustment_id{};
    bool m6_adjustment_visible{true};
    std::vector<M6AdjustmentUiState> m6_adjustments;
    InkpodM6Task* m6_task{};
    HWND m6_progress{};
    bool m6_preview_prompt{};
    bool alpha_view{};
    bool stamp_source_valid{};
    InkpodStrokeSample stamp_source{};
    M6ToolOptions m6_options{};
    FillToolOptions fill_options{};
    std::vector<InkpodStrokeSample> fill_gesture_samples;
    std::vector<InkpodStrokeSample> m6_samples;
    bool m6_airbrush_active{};
    InkpodStrokeSample m6_airbrush_last{};
    bool view_flip_horizontal{};
    bool view_flip_vertical{};
    bool ruler_visible{};
    bool guides_visible{true};
    bool grid_visible{};
    bool snap_guides{};
    bool snap_grid{};
    bool transparent_visible{true};
    std::vector<InkpodStrokeSample> view_gesture_samples;
    bool guide_drag_active{};
    std::uint32_t guide_drag_axis{};
    std::uint64_t guide_drag_id{};
    InkpodSelectionShape selection_shape{INKPOD_SELECTION_RECTANGLE};
    InkpodSelectionOperation selection_operation{INKPOD_SELECTION_NEW};
    std::uint16_t selection_tolerance{};
    std::uint16_t selection_gap_close{};
    float selection_diameter{8.0F};
    std::vector<InkpodStrokeSample> selection_gesture_samples;
    std::vector<InkpodStrokeSample> vector_gesture_samples;
    InkpodVectorEraseMode vector_erase_mode{INKPOD_VECTOR_ERASE_PARTIAL};
    InkpodVectorSelectionMode vector_selection_mode{INKPOD_VECTOR_SELECT_TOUCHING};
    std::vector<std::uint64_t> vector_selected_path_ids;
    bool smoke_test{};
};

bool QuerySnapshotTransform(
    AppState& state, InkpodSnapshotTransform& transform) noexcept;
bool QueryDocument(AppState& state, InkpodDocumentInfo& info) noexcept;
bool WidePathToUtf8(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept;
std::wstring LocalizedHistoryLabel(const std::string& label);
struct TreePaneNode;
bool QueryTreeNode(AppState& state, bool plane, TreePaneNode& output) noexcept;
bool IsVectorCanvasTool(std::uint32_t tool) noexcept;
bool IsVectorStrokePlane(std::uint32_t kind) noexcept;
void ClearVectorGeometryPreview(AppState& state) noexcept;

void ResetM3DocumentUiState(AppState& state) noexcept {
    state.m3_layer_id = 0U;
    state.selection_layer_id = 0U;
    state.active_tree_layer_id = 0U;
    state.active_tree_plane_id = 0U;
    state.active_tree_layer_index = 0U;
    state.active_tree_plane_index = 0U;
    state.active_light_table_set_id = 0U;
    state.active_light_table_item_id = 0U;
    state.active_light_table_set_index = 0U;
    state.active_light_table_item_index = 0U;
    state.light_table_move_samples.clear();
    state.active_sequence_index = 0U;
    state.floating_active = false;
    state.floating_transform = InkpodFloatingTransform{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};
    state.floating_bounds = {};
    state.floating_gesture_samples.clear();
    state.floating_drag_mode = 0U;
    state.motion_active = false;
    state.motion_paused = false;
    state.secondary_view_id = 0U;
    state.active_view_id = 0U;
    state.view_flip_horizontal = false;
    state.view_flip_vertical = false;
    state.ruler_visible = false;
    state.guides_visible = true;
    state.grid_visible = false;
    state.snap_guides = false;
    state.snap_grid = false;
    state.transparent_visible = true;
    state.view_gesture_samples.clear();
    state.fill_gesture_samples.clear();
    state.guide_drag_active = false;
    state.guide_drag_axis = 0U;
    state.guide_drag_id = 0U;
    state.selection_shape = INKPOD_SELECTION_RECTANGLE;
    state.selection_operation = INKPOD_SELECTION_NEW;
    state.selection_gesture_samples.clear();
    state.vector_gesture_samples.clear();
    state.vector_selected_path_ids.clear();
    if (state.document_tabs != nullptr) {
        TabCtrl_DeleteAllItems(state.document_tabs);
        TCITEMW item{};
        item.mask = TCIF_TEXT | TCIF_PARAM;
        item.pszText = const_cast<wchar_t*>(L"セル");
        item.lParam = 0;
        TabCtrl_InsertItem(state.document_tabs, 0, &item);
        TabCtrl_SetCurSel(state.document_tabs, 0);
    }
    if (state.engine != nullptr) {
        state.engine->SetActiveView(0U);
    }
    state.m6_adjustment_id = 0U;
    state.m6_adjustment_visible = true;
    state.m6_adjustments.clear();
    state.alpha_view = false;
    state.stamp_source_valid = false;
    state.m6_samples.clear();
    state.m6_airbrush_active = false;
    if (state.window != nullptr) {
        KillTimer(state.window, kM6ContinuousSprayTimer);
        KillTimer(state.window, kMotionPlaybackTimer);
    }
}

void UpdateLocatorDisplay(AppState& state, int device_x, int device_y) noexcept {
    if (state.engine == nullptr) {
        return;
    }
    InkpodLocatorOutput locator{};
    locator.struct_size = sizeof(locator);
    const std::uint64_t view_id = state.active_view_id;
    const InkpodStatus status = state.engine->Invoke(
        [view_id, device_x, device_y, &locator](InkpodCore* core) {
            return inkpod_core_locator_sample(
                core,
                view_id,
                static_cast<double>(device_x),
                static_cast<double>(device_y),
                &locator);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return;
    }
    const double diagonal = (locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U
        ? std::hypot(
              static_cast<double>(locator.selection.width),
              static_cast<double>(locator.selection.height))
        : 0.0;
    std::array<wchar_t, 256U> text{};
    if ((locator.flags & INKPOD_LOCATOR_COLOR_PRESENT) != 0U) {
        _snwprintf_s(
            text.data(),
            text.size(),
            _TRUNCATE,
            L"カラーロケーター\r\nX: %d  Y: %d\r\nH: %d  V: %d  L: %.2f\r\nRGBA%u: %u, %u, %u, %u",
            locator.document_x,
            locator.document_y,
            locator.selection.width,
            locator.selection.height,
            diagonal,
            locator.color.depth,
            locator.color.red,
            locator.color.green,
            locator.color.blue,
            locator.color.alpha);
    } else {
        _snwprintf_s(
            text.data(),
            text.size(),
            _TRUNCATE,
            L"カラーロケーター\r\nX: %d  Y: %d\r\nH: %d  V: %d  L: %.2f\r\nRGBA: 透明",
            locator.document_x,
            locator.document_y,
            locator.selection.width,
            locator.selection.height,
            diagonal);
    }
    if (state.locator_label != nullptr) {
        SetWindowTextW(state.locator_label, text.data());
    }
    if (state.status_bar != nullptr) {
        std::array<wchar_t, 160U> compact{};
        _snwprintf_s(
            compact.data(),
            compact.size(),
            _TRUNCATE,
            L"X:%d Y:%d H:%d V:%d L:%.1f",
            locator.document_x,
            locator.document_y,
            locator.selection.width,
            locator.selection.height,
            diagonal);
        SendMessageW(
            state.status_bar,
            SB_SETTEXTW,
            1,
            reinterpret_cast<LPARAM>(compact.data()));
    }
}

struct TreePaneNode {
    std::uint64_t id{};
    std::uint64_t parent_id{};
    std::uint32_t index{};
    std::uint32_t kind{};
    std::uint32_t pixel_format{};
    std::uint32_t opacity_milli{};
    std::uint32_t child_count{};
    std::uint32_t flags{};
    std::string name;
};

bool RefreshTreePane(AppState& state) noexcept {
    if (state.engine == nullptr || state.layer_list == nullptr || state.plane_list == nullptr) {
        return false;
    }
    std::vector<TreePaneNode> layers;
    std::vector<TreePaneNode> planes;
    const std::uint64_t requested_layer_id = state.active_tree_layer_id;
    std::uint32_t selected_layer_index{};
    const InkpodStatus status = state.engine->Invoke(
        [&layers, &planes, requested_layer_id, &selected_layer_index](InkpodCore* core) {
            try {
                layers.reserve(64U);
                planes.reserve(64U);
                for (std::uint32_t layer_index = 0U; layer_index < 1024U; ++layer_index) {
                    std::array<std::uint8_t, 256U> name{};
                    InkpodNodeInfo info{};
                    info.struct_size = sizeof(info);
                    info.name_utf8 = name.data();
                    info.name_capacity = name.size();
                    const InkpodStatus node_status = inkpod_core_node_get(
                        core, layer_index, UINT32_MAX, &info);
                    if (node_status != INKPOD_STATUS_OK) {
                        break;
                    }
                    layers.push_back(TreePaneNode{
                        info.id,
                        info.parent_id,
                        info.index,
                        info.kind,
                        info.pixel_format,
                        info.opacity_milli,
                        info.child_count,
                        info.flags,
                        std::string(
                            reinterpret_cast<const char*>(name.data()),
                            static_cast<std::size_t>(info.name_bytes))});
                    if (info.id == requested_layer_id) {
                        selected_layer_index = layer_index;
                    }
                }
                if (layers.empty()) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                if (requested_layer_id == 0U) {
                    selected_layer_index = 0U;
                }
                for (std::uint32_t plane_index = 0U; plane_index < 1024U; ++plane_index) {
                    std::array<std::uint8_t, 256U> name{};
                    InkpodNodeInfo info{};
                    info.struct_size = sizeof(info);
                    info.name_utf8 = name.data();
                    info.name_capacity = name.size();
                    const InkpodStatus node_status = inkpod_core_node_get(
                        core, selected_layer_index, plane_index, &info);
                    if (node_status != INKPOD_STATUS_OK) {
                        break;
                    }
                    planes.push_back(TreePaneNode{
                        info.id,
                        info.parent_id,
                        info.index,
                        info.kind,
                        info.pixel_format,
                        info.opacity_milli,
                        info.child_count,
                        info.flags,
                        std::string(
                            reinterpret_cast<const char*>(name.data()),
                            static_cast<std::size_t>(info.name_bytes))});
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return INKPOD_STATUS_OK;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK || layers.empty()) {
        return false;
    }

    SendMessageW(state.layer_list, LB_RESETCONTENT, 0, 0);
    for (const auto& node : layers) {
        std::wstring name;
        try {
            name = LocalizedHistoryLabel(node.name);
        } catch (const std::bad_alloc&) {
            return false;
        }
        std::array<wchar_t, 320U> label{};
        _snwprintf_s(
            label.data(),
            label.size(),
            _TRUNCATE,
            L"%ls%ls %ls  [種類%u / %u%%]",
            (node.flags & INKPOD_NODE_VISIBLE) != 0U ? L"☑" : L"☐",
            (node.flags & INKPOD_NODE_EDITABLE) != 0U ? L"✎" : L"—",
            name.c_str(),
            node.kind,
            node.opacity_milli / 10U);
        const LRESULT item = SendMessageW(
            state.layer_list,
            LB_ADDSTRING,
            0,
            reinterpret_cast<LPARAM>(label.data()));
        if (item >= 0) {
            SendMessageW(
                state.layer_list,
                LB_SETITEMDATA,
                static_cast<WPARAM>(item),
                static_cast<LPARAM>(node.id));
        }
    }
    selected_layer_index = std::min<std::uint32_t>(
        selected_layer_index, static_cast<std::uint32_t>(layers.size() - 1U));
    SendMessageW(state.layer_list, LB_SETCURSEL, selected_layer_index, 0);
    state.active_tree_layer_index = selected_layer_index;
    state.active_tree_layer_id = layers[selected_layer_index].id;

    SendMessageW(state.plane_list, LB_RESETCONTENT, 0, 0);
    std::uint32_t selected_plane_index{};
    for (std::size_t index = 0U; index < planes.size(); ++index) {
        const auto& node = planes[index];
        std::wstring name;
        try {
            name = LocalizedHistoryLabel(node.name);
        } catch (const std::bad_alloc&) {
            return false;
        }
        std::array<wchar_t, 320U> label{};
        _snwprintf_s(
            label.data(),
            label.size(),
            _TRUNCATE,
            L"%ls%ls %ls  [種類%u / 形式%u / %u%%]",
            (node.flags & INKPOD_NODE_VISIBLE) != 0U ? L"☑" : L"☐",
            (node.flags & INKPOD_NODE_EDITABLE) != 0U ? L"✎" : L"—",
            name.c_str(),
            node.kind,
            node.pixel_format,
            node.opacity_milli / 10U);
        const LRESULT item = SendMessageW(
            state.plane_list,
            LB_ADDSTRING,
            0,
            reinterpret_cast<LPARAM>(label.data()));
        if (item >= 0) {
            SendMessageW(
                state.plane_list,
                LB_SETITEMDATA,
                static_cast<WPARAM>(item),
                static_cast<LPARAM>(node.id));
        }
        if (node.id == state.active_tree_plane_id) {
            selected_plane_index = static_cast<std::uint32_t>(index);
        }
    }
    if (!planes.empty()) {
        selected_plane_index = std::min<std::uint32_t>(
            selected_plane_index, static_cast<std::uint32_t>(planes.size() - 1U));
        SendMessageW(state.plane_list, LB_SETCURSEL, selected_plane_index, 0);
        state.active_tree_plane_index = selected_plane_index;
        state.active_tree_plane_id = planes[selected_plane_index].id;
    } else {
        state.active_tree_plane_index = 0U;
        state.active_tree_plane_id = 0U;
    }
    return true;
}

struct LightTablePaneSet {
    std::uint64_t id{};
    std::uint32_t flags{};
    std::uint32_t opacity_milli{};
    std::uint32_t item_count{};
    std::string name;
};

struct LightTablePaneItem {
    InkpodLightTableItemInfo info{};
    std::string name;
};

bool RefreshLightTablePane(AppState& state) noexcept {
    if (state.engine == nullptr || state.light_table_set_list == nullptr
        || state.light_table_item_list == nullptr) {
        return false;
    }
    std::vector<LightTablePaneSet> sets;
    std::vector<LightTablePaneItem> items;
    const InkpodStatus status = state.engine->Invoke(
        [&sets, &items](InkpodCore* core) {
            try {
                for (std::uint32_t index = 0U; index < 256U; ++index) {
                    std::array<std::uint8_t, 1024U> name{};
                    InkpodLightTableSetInfo info{};
                    info.struct_size = sizeof(info);
                    info.name_utf8 = name.data();
                    info.name_capacity = name.size();
                    if (inkpod_core_light_table_set_get(core, index, &info)
                        != INKPOD_STATUS_OK) {
                        break;
                    }
                    sets.push_back(LightTablePaneSet{
                        info.id,
                        info.flags,
                        info.opacity_milli,
                        info.item_count,
                        std::string(
                            reinterpret_cast<const char*>(name.data()),
                            static_cast<std::size_t>(info.name_bytes))});
                }
                for (std::uint32_t index = 0U; index < 4096U; ++index) {
                    std::array<std::uint8_t, 1024U> name{};
                    LightTablePaneItem item{};
                    item.info.struct_size = sizeof(item.info);
                    item.info.display_color.struct_size = sizeof(item.info.display_color);
                    item.info.name_utf8 = name.data();
                    item.info.name_capacity = name.size();
                    if (inkpod_core_light_table_item_get(core, index, &item.info)
                        != INKPOD_STATUS_OK) {
                        break;
                    }
                    item.name.assign(
                        reinterpret_cast<const char*>(name.data()),
                        static_cast<std::size_t>(item.info.name_bytes));
                    item.info.name_utf8 = nullptr;
                    item.info.name_capacity = 0U;
                    items.push_back(std::move(item));
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return sets.empty() ? INKPOD_STATUS_INVALID_STATE : INKPOD_STATUS_OK;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK || sets.empty()) {
        return false;
    }
    SendMessageW(state.light_table_set_list, LB_RESETCONTENT, 0, 0);
    std::uint32_t selected_set{};
    for (std::size_t index = 0; index < sets.size(); ++index) {
        const auto& set = sets[index];
        const std::wstring name = LocalizedHistoryLabel(set.name);
        std::array<wchar_t, 320U> label{};
        _snwprintf_s(
            label.data(), label.size(), _TRUNCATE, L"LT %ls%ls [%u件/%u%%]",
            (set.flags & INKPOD_LIGHT_TABLE_SET_ACTIVE) != 0U ? L"▶ " : L"",
            name.c_str(), set.item_count, set.opacity_milli / 10U);
        const LRESULT row = SendMessageW(
            state.light_table_set_list, LB_ADDSTRING, 0,
            reinterpret_cast<LPARAM>(label.data()));
        if (row >= 0) {
            SendMessageW(
                state.light_table_set_list, LB_SETITEMDATA, static_cast<WPARAM>(row),
                static_cast<LPARAM>(set.id));
        }
        if (set.id == state.active_light_table_set_id
            || (set.flags & INKPOD_LIGHT_TABLE_SET_ACTIVE) != 0U) {
            selected_set = static_cast<std::uint32_t>(index);
        }
    }
    SendMessageW(state.light_table_set_list, LB_SETCURSEL, selected_set, 0);
    state.active_light_table_set_index = selected_set;
    state.active_light_table_set_id = sets[selected_set].id;

    SendMessageW(state.light_table_item_list, LB_RESETCONTENT, 0, 0);
    std::uint32_t selected_item{};
    for (std::size_t index = 0; index < items.size(); ++index) {
        const auto& item = items[index];
        const std::wstring name = LocalizedHistoryLabel(item.name);
        std::array<wchar_t, 320U> label{};
        _snwprintf_s(
            label.data(), label.size(), _TRUNCATE, L"%ls %ls [%u%%→%u%%]",
            (item.info.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE) != 0U ? L"☑" : L"☐",
            name.c_str(), item.info.opacity_milli / 10U,
            item.info.effective_opacity_milli / 10U);
        const LRESULT row = SendMessageW(
            state.light_table_item_list, LB_ADDSTRING, 0,
            reinterpret_cast<LPARAM>(label.data()));
        if (row >= 0) {
            SendMessageW(
                state.light_table_item_list, LB_SETITEMDATA, static_cast<WPARAM>(row),
                static_cast<LPARAM>(item.info.id));
        }
        if (item.info.id == state.active_light_table_item_id) {
            selected_item = static_cast<std::uint32_t>(index);
        }
    }
    if (!items.empty()) {
        SendMessageW(state.light_table_item_list, LB_SETCURSEL, selected_item, 0);
        state.active_light_table_item_index = selected_item;
        state.active_light_table_item_id = items[selected_item].info.id;
    } else {
        state.active_light_table_item_index = 0U;
        state.active_light_table_item_id = 0U;
    }
    return true;
}

struct SequencePaneCell {
    InkpodSequenceCellInfo info{};
    std::string name;
};

bool RefreshSequencePane(AppState& state) noexcept {
    if (state.engine == nullptr || state.sequence_list == nullptr) {
        return false;
    }
    std::vector<SequencePaneCell> cells;
    const InkpodStatus status = state.engine->Invoke(
        [&cells](InkpodCore* core) {
            try {
                for (std::uint32_t index = 0U; index < 10000U; ++index) {
                    std::array<std::uint8_t, 1024U> name{};
                    SequencePaneCell cell{};
                    cell.info.struct_size = sizeof(cell.info);
                    cell.info.name_utf8 = name.data();
                    cell.info.name_capacity = name.size();
                    const InkpodStatus cell_status =
                        inkpod_core_sequence_cell_get(core, index, &cell.info);
                    if (cell_status == INKPOD_STATUS_INVALID_STATE && index == 0U) {
                        return INKPOD_STATUS_OK;
                    }
                    if (cell_status == INKPOD_STATUS_INVALID_ARGUMENT) {
                        break;
                    }
                    if (cell_status != INKPOD_STATUS_OK) {
                        return cell_status;
                    }
                    cell.name.assign(
                        reinterpret_cast<const char*>(name.data()),
                        static_cast<std::size_t>(cell.info.name_bytes));
                    cell.info.name_utf8 = nullptr;
                    cell.info.name_capacity = 0U;
                    cells.push_back(std::move(cell));
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return INKPOD_STATUS_OK;
        },
        false,
        false);
    SendMessageW(state.sequence_list, LB_RESETCONTENT, 0, 0);
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    if (cells.empty()) {
        state.active_sequence_index = 0U;
        return true;
    }
    InkpodDocumentInfo document{};
    QueryDocument(state, document);
    std::uint32_t selected = std::min<std::uint32_t>(
        state.active_sequence_index, static_cast<std::uint32_t>(cells.size() - 1U));
    for (std::size_t index = 0; index < cells.size(); ++index) {
        const auto& cell = cells[index];
        const bool active = cell.info.document_uuid_high == document.document_uuid_high
            && cell.info.document_uuid_low == document.document_uuid_low;
        const std::wstring name = LocalizedHistoryLabel(cell.name);
        std::array<wchar_t, 360U> label{};
        _snwprintf_s(
            label.data(), label.size(), _TRUNCATE,
            L"%ls%u: %ls [%ux%u / thumb %ux%u / %016llx]",
            active ? L"▶ " : L"", cell.info.cell_number, name.c_str(), cell.info.width,
            cell.info.height, cell.info.thumbnail_width, cell.info.thumbnail_height,
            static_cast<unsigned long long>(cell.info.thumbnail_checksum));
        SendMessageW(
            state.sequence_list, LB_ADDSTRING, 0,
            reinterpret_cast<LPARAM>(label.data()));
        if (active) {
            selected = static_cast<std::uint32_t>(index);
        }
    }
    SendMessageW(state.sequence_list, LB_SETCURSEL, selected, 0);
    state.active_sequence_index = selected;
    return true;
}

bool RefreshColorPanes(AppState& state) noexcept {
    if (state.engine == nullptr || state.color_palette_list == nullptr
        || state.color_chart_list == nullptr) {
        return false;
    }
    std::vector<InkpodColorValue> colors;
    const InkpodStatus status = state.engine->Invoke(
        [&colors](InkpodCore* core) {
            InkpodColorBuffer query{};
            query.struct_size = sizeof(query);
            InkpodStatus inner = inkpod_core_palette_get(core, &query);
            if (inner != INKPOD_STATUS_OK) {
                return inner;
            }
            try {
                colors.resize(static_cast<std::size_t>(query.color_count));
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            for (auto& color : colors) {
                color.struct_size = sizeof(color);
            }
            if (colors.empty()) {
                return INKPOD_STATUS_OK;
            }
            InkpodColorBuffer output{};
            output.struct_size = sizeof(output);
            output.colors = colors.data();
            output.color_capacity = colors.size();
            output.color_stride_bytes = sizeof(InkpodColorValue);
            return inkpod_core_palette_get(core, &output);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    try {
        state.palette_colors = colors;
        const std::size_t previous_names = state.color_chart_names.size();
        state.color_chart_names.resize(colors.size());
        for (std::size_t index = previous_names; index < colors.size(); ++index) {
            state.color_chart_names[index] = L"Color " + std::to_wstring(index + 1U);
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    const std::uint32_t group_count = std::max<std::uint32_t>(
        1U, static_cast<std::uint32_t>((colors.size() + 9U) / 10U));
    state.palette_group %= group_count;
    SendMessageW(state.color_palette_list, LB_RESETCONTENT, 0, 0);
    const std::size_t start = static_cast<std::size_t>(state.palette_group) * 10U;
    for (std::size_t index = start; index < std::min(colors.size(), start + 10U); ++index) {
        const auto& color = colors[index];
        std::array<wchar_t, 160U> label{};
        _snwprintf_s(
            label.data(), label.size(), _TRUNCATE,
            color.depth == INKPOD_COLOR_DEPTH_16
                ? L"%zu  RGBA16 #%04X%04X%04X%04X"
                : L"%zu  RGBA8 #%02X%02X%02X%02X",
            index + 1U, color.red, color.green, color.blue, color.alpha);
        SendMessageW(
            state.color_palette_list, LB_ADDSTRING, 0,
            reinterpret_cast<LPARAM>(label.data()));
    }
    SendMessageW(state.color_chart_list, LB_RESETCONTENT, 0, 0);
    constexpr std::size_t chart_page_size = 20U;
    const std::uint32_t chart_page_count = std::max<std::uint32_t>(
        1U, static_cast<std::uint32_t>((colors.size() + chart_page_size - 1U)
            / chart_page_size));
    state.color_chart_page %= chart_page_count;
    const std::size_t chart_start = static_cast<std::size_t>(state.color_chart_page)
        * chart_page_size;
    for (std::size_t index = chart_start;
         index < std::min(colors.size(), chart_start + chart_page_size);
         ++index) {
        const auto& color = colors[index];
        std::array<wchar_t, 320U> label{};
        _snwprintf_s(
            label.data(), label.size(), _TRUNCATE,
            color.depth == INKPOD_COLOR_DEPTH_16
                ? L"[%u] %ls  #%04X%04X%04X%04X"
                : L"[%u] %ls  #%02X%02X%02X%02X",
            state.color_chart_page + 1U,
            state.color_chart_names[index].c_str(),
            color.red, color.green, color.blue, color.alpha);
        SendMessageW(
            state.color_chart_list, LB_ADDSTRING, 0,
            reinterpret_cast<LPARAM>(label.data()));
    }
    return true;
}

InkpodStatus ReplacePalette(
    AppState& state, const std::vector<InkpodColorValue>& colors) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return state.engine->Invoke(
        [&colors](InkpodCore* core) {
            InkpodColorArray input{};
            input.struct_size = sizeof(input);
            input.colors = colors.empty() ? nullptr : colors.data();
            input.color_count = colors.size();
            input.color_stride_bytes = colors.empty() ? 0U : sizeof(InkpodColorValue);
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_palette_set(core, &input, &result);
        },
        true,
        true);
}

void UpdateMotionLabel(AppState& state, const InkpodMotionFrame& frame) noexcept {
    state.motion_paused = (frame.flags & INKPOD_MOTION_FRAME_PAUSED) != 0U;
    if (state.motion_label == nullptr) {
        return;
    }
    std::array<wchar_t, 256U> text{};
    _snwprintf_s(
        text.data(), text.size(), _TRUNCATE,
        L"モーション %ls  %u fps  セル%u  frame:%llu  thumb:%016llx",
        state.motion_paused ? L"一時停止" : L"再生",
        state.motion_fps,
        frame.cell_number,
        static_cast<unsigned long long>(frame.sequence_index),
        static_cast<unsigned long long>(frame.thumbnail_checksum));
    SetWindowTextW(state.motion_label, text.data());
}

class ComApartment final {
public:
    HRESULT Initialize() noexcept {
        const HRESULT result = CoInitializeEx(
            nullptr, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        initialized_ = SUCCEEDED(result);
        return result;
    }

    ~ComApartment() {
        if (initialized_) {
            CoUninitialize();
        }
    }

    ComApartment(const ComApartment&) = delete;
    ComApartment& operator=(const ComApartment&) = delete;
    ComApartment() = default;

private:
    bool initialized_{};
};

struct AboutDialogState {
    HINSTANCE instance{};
    HICON display_icon{};
    HFONT name_font{};
    HFONT body_font{};
    bool close_immediately{};
    bool layout_valid{};
};

// Layout measurements are device pixels from the 144-DPI reference image.
constexpr UINT kAboutReferenceDpi = 144U;
constexpr int kAboutWindowWidth = 574;
constexpr int kAboutWindowHeight = 544;
constexpr int kAboutIconSize = 88;
constexpr int kAboutIconTop = 68;
constexpr int kAboutNameTop = 194;
constexpr int kAboutNameHeight = 40;
constexpr int kAboutDescriptionTop = 250;
constexpr int kAboutDescriptionHeight = 60;
constexpr int kAboutVersionTop = 324;
constexpr int kAboutVersionHeight = 22;
constexpr int kAboutCopyrightTop = 362;
constexpr int kAboutCopyrightHeight = 22;
constexpr int kAboutFooterHeight = 89;
constexpr int kAboutButtonWidth = 120;
constexpr int kAboutButtonHeight = 48;
constexpr int kAboutButtonRightMargin = 18;
constexpr int kAboutButtonBottomMargin = 23;

int ScaleAboutReferencePixel(int reference_pixels, UINT dpi) noexcept {
    return MulDiv(
        reference_pixels,
        static_cast<int>(dpi == 0U ? USER_DEFAULT_SCREEN_DPI : dpi),
        static_cast<int>(kAboutReferenceDpi));
}

int ClampCoordinate(int value, int minimum, int maximum) noexcept {
    if (maximum < minimum) {
        return minimum;
    }
    if (value < minimum) {
        return minimum;
    }
    return value > maximum ? maximum : value;
}

POINT CenteredAboutOrigin(HWND dialog, int width, int height) noexcept {
    const HWND owner = GetWindow(dialog, GW_OWNER);
    const HWND monitor_window = owner != nullptr ? owner : dialog;
    const HMONITOR monitor = MonitorFromWindow(
        monitor_window, MONITOR_DEFAULTTONEAREST);
    MONITORINFO monitor_info{};
    monitor_info.cbSize = sizeof(monitor_info);
    RECT work_area{};
    if (monitor == nullptr
        || GetMonitorInfoW(monitor, &monitor_info) == FALSE) {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &work_area, 0);
    } else {
        work_area = monitor_info.rcWork;
    }

    RECT anchor = work_area;
    if (owner != nullptr) {
        RECT owner_bounds{};
        if (GetWindowRect(owner, &owner_bounds) != FALSE) {
            anchor = owner_bounds;
        }
    }
    const int centered_x = anchor.left + (anchor.right - anchor.left - width) / 2;
    const int centered_y = anchor.top + (anchor.bottom - anchor.top - height) / 2;
    return POINT{
        ClampCoordinate(centered_x, work_area.left, work_area.right - width),
        ClampCoordinate(centered_y, work_area.top, work_area.bottom - height)};
}

bool MoveAboutControl(
    HWND dialog, int id, int x, int y, int width, int height) noexcept {
    const HWND control = GetDlgItem(dialog, id);
    return control != nullptr
        && MoveWindow(control, x, y, width, height, TRUE) != FALSE;
}

void ReleaseAboutFonts(AboutDialogState& state) noexcept {
    if (state.name_font != nullptr) {
        DeleteObject(state.name_font);
        state.name_font = nullptr;
    }
    if (state.body_font != nullptr) {
        DeleteObject(state.body_font);
        state.body_font = nullptr;
    }
}

bool UpdateAboutFonts(
    HWND dialog, AboutDialogState& state, UINT dpi) noexcept {
    ReleaseAboutFonts(state);
    const auto dialog_font = reinterpret_cast<HFONT>(
        SendMessageW(dialog, WM_GETFONT, 0, 0));
    LOGFONTW base_font{};
    if (dialog_font == nullptr
        || GetObjectW(
               dialog_font,
               static_cast<int>(sizeof(base_font)),
               &base_font) != static_cast<int>(sizeof(base_font))) {
        return false;
    }

    LOGFONTW name_font = base_font;
    name_font.lfHeight = -MulDiv(15, static_cast<int>(dpi), 72);
    name_font.lfWeight = FW_SEMIBOLD;
    state.name_font = CreateFontIndirectW(&name_font);

    LOGFONTW body_font = base_font;
    body_font.lfHeight = -MulDiv(9, static_cast<int>(dpi), 72);
    body_font.lfWeight = FW_NORMAL;
    state.body_font = CreateFontIndirectW(&body_font);
    if (state.name_font == nullptr || state.body_font == nullptr) {
        ReleaseAboutFonts(state);
        return false;
    }

    SendDlgItemMessageW(
        dialog,
        IDC_ABOUT_NAME,
        WM_SETFONT,
        reinterpret_cast<WPARAM>(state.name_font),
        TRUE);
    for (const int id : {
             IDC_ABOUT_DESCRIPTION,
             IDC_ABOUT_VERSION,
             IDC_ABOUT_COPYRIGHT,
             IDOK}) {
        SendDlgItemMessageW(
            dialog,
            id,
            WM_SETFONT,
            reinterpret_cast<WPARAM>(state.body_font),
            TRUE);
    }
    return true;
}

bool UpdateAboutIcon(
    HWND dialog, AboutDialogState& state, int icon_size) noexcept {
    const HWND icon_control = GetDlgItem(dialog, IDC_ABOUT_ICON);
    if (icon_control == nullptr) {
        return false;
    }
    SendMessageW(icon_control, STM_SETIMAGE, IMAGE_ICON, 0);
    if (state.display_icon != nullptr) {
        DestroyIcon(state.display_icon);
        state.display_icon = nullptr;
    }
    const int resource_id = icon_size <= kAboutIconSize
        ? IDR_ABOUT_ICON_88_PNG
        : IDR_ABOUT_ICON_256_PNG;
    state.display_icon = inkpod::windows::LoadPngIconResource(
        state.instance, resource_id, icon_size);
    if (state.display_icon == nullptr) {
        return false;
    }
    SendMessageW(
        icon_control,
        STM_SETIMAGE,
        IMAGE_ICON,
        reinterpret_cast<LPARAM>(state.display_icon));
    return true;
}

bool LayoutAboutDialog(
    HWND dialog, AboutDialogState& state, bool center_on_owner) noexcept {
    UINT dpi = GetDpiForWindow(dialog);
    if (dpi == 0U) {
        dpi = USER_DEFAULT_SCREEN_DPI;
    }
    const int window_width = ScaleAboutReferencePixel(kAboutWindowWidth, dpi);
    const int window_height = ScaleAboutReferencePixel(kAboutWindowHeight, dpi);
    RECT current_bounds{};
    if (GetWindowRect(dialog, &current_bounds) == FALSE) {
        return false;
    }
    const POINT origin = center_on_owner
        ? CenteredAboutOrigin(dialog, window_width, window_height)
        : POINT{current_bounds.left, current_bounds.top};
    if (SetWindowPos(
            dialog,
            nullptr,
            origin.x,
            origin.y,
            window_width,
            window_height,
            SWP_NOACTIVATE | SWP_NOZORDER) == FALSE) {
        return false;
    }

    RECT client{};
    if (GetClientRect(dialog, &client) == FALSE) {
        return false;
    }
    const int icon_size = ScaleAboutReferencePixel(kAboutIconSize, dpi);
    const int content_width = client.right - client.left;
    const int button_width = ScaleAboutReferencePixel(kAboutButtonWidth, dpi);
    const int button_height = ScaleAboutReferencePixel(kAboutButtonHeight, dpi);
    const int description_margin = ScaleAboutReferencePixel(48, dpi);
    const int name_margin = ScaleAboutReferencePixel(20, dpi);
    const int divider_height = ScaleAboutReferencePixel(1, dpi);
    if (!MoveAboutControl(
            dialog,
            IDC_ABOUT_ICON,
            (content_width - icon_size) / 2,
            ScaleAboutReferencePixel(kAboutIconTop, dpi),
            icon_size,
            icon_size)
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_NAME,
            name_margin,
            ScaleAboutReferencePixel(kAboutNameTop, dpi),
            content_width - name_margin * 2,
            ScaleAboutReferencePixel(kAboutNameHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_DESCRIPTION,
            description_margin,
            ScaleAboutReferencePixel(kAboutDescriptionTop, dpi),
            content_width - description_margin * 2,
            ScaleAboutReferencePixel(kAboutDescriptionHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_VERSION,
            name_margin,
            ScaleAboutReferencePixel(kAboutVersionTop, dpi),
            content_width - name_margin * 2,
            ScaleAboutReferencePixel(kAboutVersionHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_COPYRIGHT,
            name_margin,
            ScaleAboutReferencePixel(kAboutCopyrightTop, dpi),
            content_width - name_margin * 2,
            ScaleAboutReferencePixel(kAboutCopyrightHeight, dpi))
        || !MoveAboutControl(
            dialog,
            IDC_ABOUT_SEPARATOR,
            0,
            client.bottom - ScaleAboutReferencePixel(kAboutFooterHeight, dpi),
            content_width,
            divider_height)
        || !MoveAboutControl(
            dialog,
            IDOK,
            content_width
                - ScaleAboutReferencePixel(kAboutButtonRightMargin, dpi)
                - button_width,
            client.bottom
                - ScaleAboutReferencePixel(kAboutButtonBottomMargin, dpi)
                - button_height,
            button_width,
            button_height)
        || !UpdateAboutFonts(dialog, state, dpi)
        || !UpdateAboutIcon(dialog, state, icon_size)) {
        return false;
    }
    return true;
}

bool AboutControlBounds(HWND dialog, int id, RECT& bounds) noexcept {
    const HWND control = GetDlgItem(dialog, id);
    if (control == nullptr || GetWindowRect(control, &bounds) == FALSE) {
        return false;
    }
    MapWindowPoints(
        nullptr,
        dialog,
        reinterpret_cast<POINT*>(&bounds),
        2);
    return true;
}

bool AboutIconSizeMatches(HICON icon, int expected_size) noexcept {
    ICONINFO icon_info{};
    if (icon == nullptr || GetIconInfo(icon, &icon_info) == FALSE) {
        return false;
    }
    BITMAP bitmap{};
    const bool matches = icon_info.hbmColor != nullptr
        && GetObjectW(
               icon_info.hbmColor,
               static_cast<int>(sizeof(bitmap)),
               &bitmap) == static_cast<int>(sizeof(bitmap))
        && bitmap.bmWidth == expected_size
        && bitmap.bmHeight == expected_size;
    if (icon_info.hbmColor != nullptr) {
        DeleteObject(icon_info.hbmColor);
    }
    if (icon_info.hbmMask != nullptr) {
        DeleteObject(icon_info.hbmMask);
    }
    return matches;
}

bool AboutFontHeightMatches(
    HWND dialog, int control_id, int point_size, UINT dpi) noexcept {
    const auto font = reinterpret_cast<HFONT>(
        SendDlgItemMessageW(dialog, control_id, WM_GETFONT, 0, 0));
    LOGFONTW font_info{};
    return font != nullptr
        && GetObjectW(
               font,
               static_cast<int>(sizeof(font_info)),
               &font_info) == static_cast<int>(sizeof(font_info))
        && font_info.lfHeight
            == -MulDiv(point_size, static_cast<int>(dpi), 72);
}

bool ValidateAboutDialog(HWND dialog, HINSTANCE instance) noexcept {
    UINT dpi = GetDpiForWindow(dialog);
    if (dpi == 0U) {
        dpi = USER_DEFAULT_SCREEN_DPI;
    }
    RECT dialog_bounds{};
    RECT icon_bounds{};
    RECT name_bounds{};
    RECT copyright_bounds{};
    RECT separator_bounds{};
    if (GetWindowRect(dialog, &dialog_bounds) == FALSE
        || !AboutControlBounds(dialog, IDC_ABOUT_ICON, icon_bounds)
        || !AboutControlBounds(dialog, IDC_ABOUT_NAME, name_bounds)
        || !AboutControlBounds(
            dialog, IDC_ABOUT_COPYRIGHT, copyright_bounds)
        || !AboutControlBounds(
            dialog, IDC_ABOUT_SEPARATOR, separator_bounds)) {
        return false;
    }
    const int width = dialog_bounds.right - dialog_bounds.left;
    const int height = dialog_bounds.bottom - dialog_bounds.top;
    const POINT expected_origin = CenteredAboutOrigin(dialog, width, height);
    const int expected_icon_size =
        ScaleAboutReferencePixel(kAboutIconSize, dpi);
    const auto displayed_icon = reinterpret_cast<HICON>(
        SendDlgItemMessageW(
            dialog, IDC_ABOUT_ICON, STM_GETIMAGE, IMAGE_ICON, 0));
    if (width != ScaleAboutReferencePixel(kAboutWindowWidth, dpi)
        || height != ScaleAboutReferencePixel(kAboutWindowHeight, dpi)
        || dialog_bounds.left != expected_origin.x
        || dialog_bounds.top != expected_origin.y
        || name_bounds.top - icon_bounds.bottom
            != ScaleAboutReferencePixel(38, dpi)
        || name_bounds.bottom - name_bounds.top
            != ScaleAboutReferencePixel(kAboutNameHeight, dpi)
        || copyright_bounds.bottom >= separator_bounds.top
        || !AboutIconSizeMatches(displayed_icon, expected_icon_size)
        || !AboutFontHeightMatches(dialog, IDC_ABOUT_NAME, 15, dpi)
        || !AboutFontHeightMatches(
            dialog, IDC_ABOUT_DESCRIPTION, 9, dpi)) {
        return false;
    }

    std::array<wchar_t, 32> name{};
    std::array<wchar_t, 64> version{};
    std::array<wchar_t, 96> expected_version{};
    std::array<wchar_t, 512> description{};
    std::array<wchar_t, 512> expected_description{};
    std::array<wchar_t, 64> copyright{};
    std::array<wchar_t, 64> expected_copyright{};
    if (GetDlgItemTextW(
            dialog, IDC_ABOUT_NAME, name.data(), static_cast<int>(name.size())) == 0
        || GetDlgItemTextW(
               dialog,
               IDC_ABOUT_VERSION,
               expected_version.data(),
               static_cast<int>(expected_version.size())) == 0
        || GetDlgItemTextW(
               dialog,
               IDC_ABOUT_DESCRIPTION,
               description.data(),
               static_cast<int>(description.size())) == 0
        || LoadStringW(
               instance,
               IDS_APP_VERSION,
               version.data(),
               static_cast<int>(version.size())) == 0
        || LoadStringW(
               instance,
               IDS_ABOUT_DESCRIPTION,
               expected_description.data(),
               static_cast<int>(expected_description.size())) == 0
        || GetDlgItemTextW(
               dialog,
               IDC_ABOUT_COPYRIGHT,
               copyright.data(),
               static_cast<int>(copyright.size())) == 0
        || LoadStringW(
               instance,
               IDS_ABOUT_COPYRIGHT,
               expected_copyright.data(),
               static_cast<int>(expected_copyright.size())) == 0) {
        return false;
    }
    std::array<wchar_t, 96> version_label{};
    _snwprintf_s(
        version_label.data(),
        version_label.size(),
        _TRUNCATE,
        L"Version %ls",
        version.data());
    return std::wcscmp(name.data(), L"Inkpod") == 0
        && std::wcscmp(expected_version.data(), version_label.data()) == 0
        && std::wcscmp(description.data(), expected_description.data()) == 0
        && std::wcscmp(copyright.data(), expected_copyright.data()) == 0;
}

INT_PTR CALLBACK AboutDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AboutDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<AboutDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));

            std::array<wchar_t, 64> version{};
            std::array<wchar_t, 96> version_label{};
            std::array<wchar_t, 512> description{};
            std::array<wchar_t, 64> copyright{};
            if (LoadStringW(
                    state->instance,
                    IDS_APP_VERSION,
                    version.data(),
                    static_cast<int>(version.size())) == 0
                || LoadStringW(
                       state->instance,
                       IDS_ABOUT_DESCRIPTION,
                       description.data(),
                       static_cast<int>(description.size())) == 0
                || LoadStringW(
                       state->instance,
                       IDS_ABOUT_COPYRIGHT,
                       copyright.data(),
                       static_cast<int>(copyright.size())) == 0) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            _snwprintf_s(
                version_label.data(),
                version_label.size(),
                _TRUNCATE,
                L"Version %ls",
                version.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_VERSION, version_label.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_DESCRIPTION, description.data());
            SetDlgItemTextW(dialog, IDC_ABOUT_COPYRIGHT, copyright.data());
            if (!LayoutAboutDialog(dialog, *state, true)) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }

            const auto caption_icon = LoadIconW(
                state->instance, MAKEINTRESOURCEW(IDI_APP_ICON));
            if (caption_icon != nullptr) {
                SendMessageW(
                    dialog,
                    WM_SETICON,
                    ICON_SMALL,
                    reinterpret_cast<LPARAM>(caption_icon));
            }
            state->layout_valid = ValidateAboutDialog(
                dialog, state->instance);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_DPICHANGED:
            if (state != nullptr) {
                const auto* suggested = reinterpret_cast<const RECT*>(lparam);
                if (suggested != nullptr) {
                    SetWindowPos(
                        dialog,
                        nullptr,
                        suggested->left,
                        suggested->top,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER);
                }
                return LayoutAboutDialog(dialog, *state, false) ? TRUE : FALSE;
            }
            break;
        case WM_COMMAND:
            if (LOWORD(wparam) == IDOK || LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, LOWORD(wparam));
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        case WM_DESTROY:
            if (state != nullptr) {
                SendDlgItemMessageW(
                    dialog, IDC_ABOUT_ICON, STM_SETIMAGE, IMAGE_ICON, 0);
                if (state->display_icon != nullptr) {
                    DestroyIcon(state->display_icon);
                    state->display_icon = nullptr;
                }
                ReleaseAboutFonts(*state);
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowAboutDialog(
    HINSTANCE instance, HWND owner, bool close_immediately) noexcept {
    AboutDialogState state{
        instance, nullptr, nullptr, nullptr, close_immediately, false};
    const INT_PTR result = DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_ABOUT),
        owner,
        AboutDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
    return result == IDOK && (!close_immediately || state.layout_valid)
        ? IDOK
        : IDCANCEL;
}

struct ShortcutDialogState {
    std::uint32_t command_id{1U};
    std::uint32_t virtual_key{static_cast<std::uint32_t>('Z')};
    std::uint32_t modifiers{INKPOD_SHORTCUT_MODIFIER_CONTROL};
    bool close_immediately{};
};

struct ShortcutEntry {
    std::uint32_t command_id;
    UINT menu_command;
    const wchar_t* label;
};

constexpr std::array<ShortcutEntry, 24U> kShortcutEntries{{
    {1U, IDM_EDIT_UNDO, L"[メニュー] 元に戻す"},
    {2U, IDM_EDIT_REDO, L"[メニュー] やり直し"},
    {3U, IDM_EDIT_COPY, L"[メニュー] コピー"},
    {4U, IDM_EDIT_PASTE, L"[メニュー] 貼り付け"},
    {IDM_FILE_NEW, IDM_FILE_NEW, L"[メニュー] 新規セル"},
    {IDM_FILE_OPEN, IDM_FILE_OPEN, L"[メニュー] 開く"},
    {IDM_FILE_SAVE, IDM_FILE_SAVE, L"[メニュー] 保存"},
    {IDM_VIEW_FIT, IDM_VIEW_FIT, L"[メニュー] 全体表示"},
    {IDM_VIEW_ONE_TO_ONE, IDM_VIEW_ONE_TO_ONE, L"[メニュー] ピクセル等倍"},
    {IDM_VIEW_FLIP_HORIZONTAL, IDM_VIEW_FLIP_HORIZONTAL, L"[メニュー] 表示を左右反転"},
    {IDM_VIEW_FLIP_VERTICAL, IDM_VIEW_FLIP_VERTICAL, L"[メニュー] 表示を上下反転"},
    {IDM_SELECTION_ALL, IDM_SELECTION_ALL, L"[メニュー] すべて選択"},
    {IDM_SELECTION_CLEAR, IDM_SELECTION_CLEAR, L"[メニュー] 選択解除"},
    {IDM_TOOL_PENCIL, IDM_TOOL_PENCIL, L"[ツール] 鉛筆"},
    {IDM_TOOL_BRUSH, IDM_TOOL_BRUSH, L"[ツール] ブラシ"},
    {IDM_TOOL_ERASER, IDM_TOOL_ERASER, L"[ツール] 消しゴム"},
    {IDM_TOOL_FILL, IDM_TOOL_FILL, L"[ツール] フィル"},
    {IDM_TOOL_EYEDROPPER, IDM_TOOL_EYEDROPPER, L"[ツール] スポイト"},
    {IDM_SELECTION_RECTANGLE, IDM_SELECTION_RECTANGLE, L"[ツール] 長方形選択"},
    {IDM_SELECTION_WAND, IDM_SELECTION_WAND, L"[ツール] 色の杖"},
    {IDM_VIEW_GRID, IDM_VIEW_GRID, L"[その他] グリッド表示"},
    {IDM_VIEW_GUIDES, IDM_VIEW_GUIDES, L"[その他] ガイド表示"},
    {IDM_VIEW_NEW, IDM_VIEW_NEW, L"[その他] 新規セルビュー"},
    {IDM_COLOR_CHOOSE, IDM_COLOR_CHOOSE, L"[その他] 描画色"},
}};

constexpr WORD DefaultShortcutHotkey(std::uint32_t command_id) noexcept {
    const BYTE key = command_id == 2U
        ? static_cast<BYTE>('Y')
        : (command_id == 3U
                  ? static_cast<BYTE>('C')
                  : (command_id == 4U ? static_cast<BYTE>('V')
                                      : (command_id == 1U ? static_cast<BYTE>('Z') : 0U)));
    return key == 0U ? 0U : MAKEWORD(key, HOTKEYF_CONTROL);
}

std::uint32_t ShortcutModifiers(BYTE hotkey_flags) noexcept {
    std::uint32_t modifiers{};
    if ((hotkey_flags & HOTKEYF_CONTROL) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_CONTROL;
    }
    if ((hotkey_flags & HOTKEYF_SHIFT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_SHIFT;
    }
    if ((hotkey_flags & HOTKEYF_ALT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_ALT;
    }
    if ((hotkey_flags & HOTKEYF_EXT) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    }
    return modifiers;
}

INT_PTR CALLBACK ShortcutDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<ShortcutDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<ShortcutDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND commands = GetDlgItem(dialog, IDC_SHORTCUT_COMMAND);
            if (commands == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            for (const auto& entry : kShortcutEntries) {
                const LRESULT item = SendMessageW(
                    commands,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(entry.label));
                if (item == CB_ERR || item == CB_ERRSPACE) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                SendMessageW(
                    commands,
                    CB_SETITEMDATA,
                    static_cast<WPARAM>(item),
                    static_cast<LPARAM>(entry.command_id));
            }
            SendMessageW(commands, CB_SETCURSEL, 0, 0);
            SendDlgItemMessageW(
                dialog,
                IDC_SHORTCUT_HOTKEY,
                HKM_SETHOTKEY,
                DefaultShortcutHotkey(1U),
                0);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDC_SHORTCUT_COMMAND
                && HIWORD(wparam) == CBN_SELCHANGE) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_COMMAND, CB_GETCURSEL, 0, 0);
                if (selected != CB_ERR) {
                    const auto command_id = static_cast<std::uint32_t>(
                        SendDlgItemMessageW(
                            dialog,
                            IDC_SHORTCUT_COMMAND,
                            CB_GETITEMDATA,
                            static_cast<WPARAM>(selected),
                            0));
                    SendDlgItemMessageW(
                        dialog,
                        IDC_SHORTCUT_HOTKEY,
                        HKM_SETHOTKEY,
                        DefaultShortcutHotkey(command_id),
                        0);
                }
                return TRUE;
            }
            if (LOWORD(wparam) == IDOK) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_COMMAND, CB_GETCURSEL, 0, 0);
                const LRESULT command_id = selected == CB_ERR
                    ? CB_ERR
                    : SendDlgItemMessageW(
                          dialog,
                          IDC_SHORTCUT_COMMAND,
                          CB_GETITEMDATA,
                          static_cast<WPARAM>(selected),
                          0);
                const WORD hotkey = static_cast<WORD>(SendDlgItemMessageW(
                    dialog, IDC_SHORTCUT_HOTKEY, HKM_GETHOTKEY, 0, 0));
                if (command_id == CB_ERR || LOBYTE(hotkey) == 0U) {
                    if (!state->close_immediately) {
                        MessageBoxW(
                            dialog,
                            L"コマンドとキーを指定してください。",
                            L"inkpod",
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                state->command_id = static_cast<std::uint32_t>(command_id);
                state->virtual_key = LOBYTE(hotkey);
                state->modifiers = ShortcutModifiers(HIBYTE(hotkey));
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowShortcutEditor(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    ShortcutDialogState& state) noexcept {
    state.close_immediately = close_immediately;
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_SHORTCUT_EDITOR),
        owner,
        ShortcutDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

struct ViewOptionsDialogState {
    const wchar_t* title{};
    std::array<const wchar_t*, 4U> labels{};
    std::array<std::int32_t, 4U> values{};
    std::uint32_t value_count{1U};
    bool close_immediately{};
};

INT_PTR CALLBACK ViewOptionsDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<ViewOptionsDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    constexpr std::array<int, 4U> label_ids{
        IDC_VIEW_VALUE_LABEL,
        IDC_VIEW_VALUE2_LABEL,
        IDC_VIEW_VALUE3_LABEL,
        IDC_VIEW_VALUE4_LABEL};
    constexpr std::array<int, 4U> edit_ids{
        IDC_VIEW_VALUE,
        IDC_VIEW_VALUE2,
        IDC_VIEW_VALUE3,
        IDC_VIEW_VALUE4};
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<ViewOptionsDialogState*>(lparam);
            if (state == nullptr || state->value_count == 0U
                || state->value_count > label_ids.size()) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            if (state->title != nullptr) {
                SetWindowTextW(dialog, state->title);
            }
            for (std::size_t index = 0; index < label_ids.size(); ++index) {
                const bool visible = index < state->value_count;
                ShowWindow(GetDlgItem(dialog, label_ids[index]), visible ? SW_SHOW : SW_HIDE);
                ShowWindow(GetDlgItem(dialog, edit_ids[index]), visible ? SW_SHOW : SW_HIDE);
                if (!visible) {
                    continue;
                }
                SetDlgItemTextW(
                    dialog,
                    label_ids[index],
                    state->labels[index] == nullptr ? L"値" : state->labels[index]);
                std::array<wchar_t, 32U> value{};
                _snwprintf_s(
                    value.data(), value.size(), _TRUNCATE, L"%d", state->values[index]);
                SetDlgItemTextW(dialog, edit_ids[index], value.data());
            }
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                for (std::size_t index = 0; index < state->value_count; ++index) {
                    std::array<wchar_t, 32U> text{};
                    if (GetDlgItemTextW(
                            dialog,
                            edit_ids[index],
                            text.data(),
                            static_cast<int>(text.size())) <= 0) {
                        return TRUE;
                    }
                    wchar_t* end{};
                    errno = 0;
                    const long value = std::wcstol(text.data(), &end, 10);
                    if (errno == ERANGE || end == text.data() || *end != L'\0'
                        || value < INT_MIN || value > INT_MAX) {
                        if (!state->close_immediately) {
                            MessageBoxW(
                                dialog,
                                L"すべての値を整数で指定してください。",
                                L"inkpod",
                                MB_OK | MB_ICONWARNING);
                        }
                        return TRUE;
                    }
                    state->values[index] = static_cast<std::int32_t>(value);
                }
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowViewOptions(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    ViewOptionsDialogState& state) noexcept {
    state.close_immediately = close_immediately;
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_VIEW_OPTIONS),
        owner,
        ViewOptionsDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

struct TextInputDialogState {
    const wchar_t* title{};
    const wchar_t* label{};
    std::wstring value;
    bool close_immediately{};
};

INT_PTR CALLBACK TextInputDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<TextInputDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<TextInputDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            if (state->title != nullptr) {
                SetWindowTextW(dialog, state->title);
            }
            SetDlgItemTextW(
                dialog, IDC_TEXT_INPUT_LABEL,
                state->label == nullptr ? L"値" : state->label);
            SetDlgItemTextW(dialog, IDC_TEXT_INPUT_VALUE, state->value.c_str());
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                std::array<wchar_t, 1025U> text{};
                const int length = GetDlgItemTextW(
                    dialog, IDC_TEXT_INPUT_VALUE, text.data(), static_cast<int>(text.size()));
                if (length <= 0) {
                    return TRUE;
                }
                try {
                    state->value.assign(text.data(), static_cast<std::size_t>(length));
                } catch (const std::bad_alloc&) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowTextInput(
    HINSTANCE instance,
    HWND owner,
    bool close_immediately,
    TextInputDialogState& state) noexcept {
    state.close_immediately = close_immediately;
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_TEXT_INPUT),
        owner,
        TextInputDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

struct FillOptionsDialogState {
    FillToolOptions options;
    bool close_immediately{};
};

bool ParseFillColors(const wchar_t* text, std::vector<std::uint32_t>& colors) noexcept {
    colors.clear();
    if (text == nullptr) {
        return false;
    }
    try {
        const std::wstring input(text);
        std::size_t start{};
        while (start < input.size()) {
            const std::size_t separator = input.find(L';', start);
            const std::size_t end = separator == std::wstring::npos
                ? input.size()
                : separator;
            std::size_t first = start;
            while (first < end && iswspace(input[first]) != 0) {
                ++first;
            }
            std::size_t last = end;
            while (last > first && iswspace(input[last - 1U]) != 0) {
                --last;
            }
            if (first != last) {
                if (last - first != 8U || colors.size() >= 6U) {
                    return false;
                }
                const std::wstring token = input.substr(first, last - first);
                wchar_t* token_end{};
                errno = 0;
                const unsigned long value = std::wcstoul(
                    token.c_str(), &token_end, 16);
                if (errno == ERANGE || token_end == token.c_str()
                    || *token_end != L'\0') {
                    return false;
                }
                colors.push_back(static_cast<std::uint32_t>(value));
            }
            if (separator == std::wstring::npos) {
                break;
            }
            start = separator + 1U;
        }
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

INT_PTR CALLBACK FillOptionsDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<FillOptionsDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<FillOptionsDialogState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND operation = GetDlgItem(dialog, IDC_FILL_OPERATION);
            for (const wchar_t* label : {L"通常フィル", L"閉領域フィル", L"塗りのばし"}) {
                SendMessageW(
                    operation,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(label));
            }
            SendMessageW(
                operation,
                CB_SETCURSEL,
                state->options.operation == INKPOD_FILL_CLOSED_REGION
                    ? 1
                    : (state->options.operation == INKPOD_FILL_EXTENSION ? 2 : 0),
                0);
            const HWND inclusion = GetDlgItem(dialog, IDC_FILL_INCLUSION_MODE);
            for (const wchar_t* label : {L"なし", L"指定色", L"指定色以外"}) {
                SendMessageW(
                    inclusion,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(label));
            }
            SendMessageW(
                inclusion,
                CB_SETCURSEL,
                state->options.inclusion_mode == INKPOD_INCLUSION_SPECIFIED
                    ? 1
                    : (state->options.inclusion_mode == INKPOD_INCLUSION_EXCEPT_SPECIFIED
                              ? 2
                              : 0),
                0);
            SetDlgItemInt(dialog, IDC_FILL_TOLERANCE, state->options.tolerance, FALSE);
            SetDlgItemInt(dialog, IDC_FILL_GAP, state->options.gap_close, FALSE);
            SetDlgItemInt(
                dialog, IDC_FILL_EXTENSION, state->options.extension_distance, FALSE);
            std::wstring color_text;
            try {
                std::array<wchar_t, 16U> token{};
                for (std::size_t index = 0; index < state->options.inclusion_rgba.size(); ++index) {
                    if (index != 0U) {
                        color_text += L';';
                    }
                    _snwprintf_s(
                        token.data(),
                        token.size(),
                        _TRUNCATE,
                        L"%08X",
                        state->options.inclusion_rgba[index]);
                    color_text += token.data();
                }
            } catch (const std::bad_alloc&) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetDlgItemTextW(dialog, IDC_FILL_COLORS, color_text.c_str());
            for (const auto& [control, checked] : std::array<std::pair<int, bool>, 6U>{
                     std::pair{IDC_FILL_OVERFLOW, state->options.overflow_abort},
                     std::pair{IDC_FILL_DETACHED, state->options.detached_regions},
                     std::pair{IDC_FILL_TRANSPARENT, state->options.transparent_only},
                     std::pair{IDC_FILL_SELECTION, state->options.use_document_selection},
                     std::pair{IDC_FILL_LIGHT_BOUNDARY, state->options.light_table_boundary},
                     std::pair{IDC_FILL_LIGHT_COLOR, state->options.light_table_color}}) {
                CheckDlgButton(dialog, control, checked ? BST_CHECKED : BST_UNCHECKED);
            }
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                BOOL tolerance_ok{};
                BOOL gap_ok{};
                BOOL extension_ok{};
                const UINT tolerance = GetDlgItemInt(
                    dialog, IDC_FILL_TOLERANCE, &tolerance_ok, FALSE);
                const UINT gap = GetDlgItemInt(dialog, IDC_FILL_GAP, &gap_ok, FALSE);
                const UINT extension = GetDlgItemInt(
                    dialog, IDC_FILL_EXTENSION, &extension_ok, FALSE);
                std::array<wchar_t, 256U> color_text{};
                GetDlgItemTextW(
                    dialog,
                    IDC_FILL_COLORS,
                    color_text.data(),
                    static_cast<int>(color_text.size()));
                std::vector<std::uint32_t> colors;
                if (tolerance_ok == FALSE || gap_ok == FALSE || extension_ok == FALSE
                    || tolerance > UINT16_MAX || gap > UINT16_MAX || extension == 0U
                    || !ParseFillColors(color_text.data(), colors)) {
                    if (!state->close_immediately) {
                        MessageBoxW(
                            dialog,
                            L"許容差、隙間、距離、対象色を確認してください。",
                            L"inkpod",
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                const LRESULT operation = SendDlgItemMessageW(
                    dialog, IDC_FILL_OPERATION, CB_GETCURSEL, 0, 0);
                const LRESULT inclusion = SendDlgItemMessageW(
                    dialog, IDC_FILL_INCLUSION_MODE, CB_GETCURSEL, 0, 0);
                if (operation == CB_ERR || inclusion == CB_ERR) {
                    return TRUE;
                }
                state->options.operation = operation == 1
                    ? INKPOD_FILL_CLOSED_REGION
                    : (operation == 2 ? INKPOD_FILL_EXTENSION : INKPOD_FILL_SEED);
                state->options.inclusion_mode = inclusion == 1
                    ? INKPOD_INCLUSION_SPECIFIED
                    : (inclusion == 2 ? INKPOD_INCLUSION_EXCEPT_SPECIFIED
                                      : INKPOD_INCLUSION_NONE);
                if (state->options.inclusion_mode != INKPOD_INCLUSION_NONE
                    && colors.empty()) {
                    if (!state->close_immediately) {
                        MessageBoxW(
                            dialog,
                            L"含み塗りには対象色が必要です。",
                            L"inkpod",
                            MB_OK | MB_ICONWARNING);
                    }
                    return TRUE;
                }
                state->options.tolerance = static_cast<std::uint16_t>(tolerance);
                state->options.gap_close = static_cast<std::uint16_t>(gap);
                state->options.extension_distance = extension;
                state->options.inclusion_rgba = std::move(colors);
                state->options.overflow_abort =
                    IsDlgButtonChecked(dialog, IDC_FILL_OVERFLOW) == BST_CHECKED;
                state->options.detached_regions =
                    IsDlgButtonChecked(dialog, IDC_FILL_DETACHED) == BST_CHECKED;
                state->options.transparent_only =
                    IsDlgButtonChecked(dialog, IDC_FILL_TRANSPARENT) == BST_CHECKED;
                state->options.use_document_selection =
                    IsDlgButtonChecked(dialog, IDC_FILL_SELECTION) == BST_CHECKED;
                state->options.light_table_boundary =
                    IsDlgButtonChecked(dialog, IDC_FILL_LIGHT_BOUNDARY) == BST_CHECKED;
                state->options.light_table_color =
                    IsDlgButtonChecked(dialog, IDC_FILL_LIGHT_COLOR) == BST_CHECKED;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

bool ShowFillOptions(AppState& app) noexcept {
    FillOptionsDialogState state{app.fill_options, app.smoke_test};
    if (DialogBoxParamW(
            app.instance,
            MAKEINTRESOURCEW(IDD_FILL_OPTIONS),
            app.window,
            FillOptionsDialogProcedure,
            reinterpret_cast<LPARAM>(&state)) != IDOK) {
        return false;
    }
    app.fill_options = std::move(state.options);
    return true;
}

struct HistoryDialogState {
    std::vector<std::wstring> labels;
    std::vector<std::uint64_t> cursors;
    std::size_t selected_index{};
    std::uint64_t selected_cursor{};
    bool close_immediately{};
};

INT_PTR CALLBACK HistoryDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<HistoryDialogState*>(
        GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<HistoryDialogState*>(lparam);
            if (state == nullptr || state->labels.empty()
                || state->labels.size() != state->cursors.size()) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(
                dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            const HWND list = GetDlgItem(dialog, IDC_HISTORY_LIST);
            for (std::size_t index = 0; index < state->labels.size(); ++index) {
                const LRESULT item = SendMessageW(
                    list,
                    CB_ADDSTRING,
                    0,
                    reinterpret_cast<LPARAM>(state->labels[index].c_str()));
                if (item == CB_ERR || item == CB_ERRSPACE) {
                    EndDialog(dialog, IDCANCEL);
                    return TRUE;
                }
                SendMessageW(
                    list,
                    CB_SETITEMDATA,
                    static_cast<WPARAM>(item),
                    static_cast<LPARAM>(index));
            }
            const auto selected = static_cast<WPARAM>(
                std::min(state->selected_index, state->labels.size() - 1U));
            SendMessageW(list, CB_SETCURSEL, selected, 0);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                const LRESULT selected = SendDlgItemMessageW(
                    dialog, IDC_HISTORY_LIST, CB_GETCURSEL, 0, 0);
                if (selected == CB_ERR) {
                    return TRUE;
                }
                const auto index = static_cast<std::size_t>(SendDlgItemMessageW(
                    dialog,
                    IDC_HISTORY_LIST,
                    CB_GETITEMDATA,
                    static_cast<WPARAM>(selected),
                    0));
                if (index >= state->cursors.size()) {
                    return TRUE;
                }
                state->selected_cursor = state->cursors[index];
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

std::wstring LocalizedHistoryLabel(const std::string& label) {
    if (label == "Raster edit") {
        return L"画像編集";
    }
    if (label == "Palette edit") {
        return L"パレット編集";
    }
    if (label == "Main-line color") {
        return L"主線色";
    }
    if (label == "Document edit") {
        return L"文書編集";
    }
    const int count = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        label.data(),
        static_cast<int>(label.size()),
        nullptr,
        0);
    if (count <= 0) {
        return L"編集";
    }
    std::wstring output(static_cast<std::size_t>(count), L'\0');
    if (MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            label.data(),
            static_cast<int>(label.size()),
            output.data(),
            count) != count) {
        return L"編集";
    }
    return output;
}

bool ConfigureHistoryDialog(
    AppState& app, bool forward, HistoryDialogState& dialog) noexcept {
    if (app.engine == nullptr) {
        return false;
    }
    InkpodHistoryInfo info{};
    info.struct_size = sizeof(info);
    std::vector<std::string> names;
    const InkpodStatus status = app.engine->Invoke(
        [&info, &names](InkpodCore* core) {
            InkpodStatus inner = inkpod_core_history_info(core, &info);
            if (inner != INKPOD_STATUS_OK || info.item_count > UINT64_C(1048576)) {
                return inner == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : inner;
            }
            try {
                names.reserve(static_cast<std::size_t>(info.item_count));
                for (std::uint64_t index = 0; index < info.item_count; ++index) {
                    InkpodHistoryItem item{};
                    item.struct_size = sizeof(item);
                    inner = inkpod_core_history_item(core, index, &item);
                    if (inner != INKPOD_STATUS_OK
                        || item.name_bytes > UINT64_C(4096)) {
                        return inner == INKPOD_STATUS_OK
                            ? INKPOD_STATUS_INVALID_STATE
                            : inner;
                    }
                    std::string name(static_cast<std::size_t>(item.name_bytes), '\0');
                    item.name_utf8 = reinterpret_cast<std::uint8_t*>(name.data());
                    item.name_capacity = item.name_bytes;
                    inner = inkpod_core_history_item(core, index, &item);
                    if (inner != INKPOD_STATUS_OK) {
                        return inner;
                    }
                    names.push_back(std::move(name));
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return INKPOD_STATUS_OK;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK || info.cursor > info.item_count) {
        return false;
    }
    const std::uint64_t first = forward ? info.cursor + 1U : 0U;
    const std::uint64_t last = forward ? info.item_count : info.cursor - 1U;
    if ((forward && info.cursor == info.item_count)
        || (!forward && info.cursor == 0U)) {
        return false;
    }
    try {
        for (std::uint64_t cursor = first; cursor <= last; ++cursor) {
            std::wstring label;
            if (cursor == 0U) {
                label = L"0: 初期状態";
            } else {
                std::array<wchar_t, 32U> prefix{};
                _snwprintf_s(
                    prefix.data(), prefix.size(), _TRUNCATE, L"%llu: ",
                    static_cast<unsigned long long>(cursor));
                label = prefix.data();
                label += LocalizedHistoryLabel(
                    names[static_cast<std::size_t>(cursor - 1U)]);
            }
            dialog.labels.push_back(std::move(label));
            dialog.cursors.push_back(cursor);
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    dialog.selected_index = app.smoke_test
        ? (forward ? dialog.labels.size() - 1U : 0U)
        : (forward ? 0U : dialog.labels.size() - 1U);
    dialog.close_immediately = app.smoke_test;
    return true;
}

bool QueryHistoryMenuLabels(
    AppState& app,
    InkpodHistoryInfo& info,
    std::wstring& undo_label,
    std::wstring& redo_label) noexcept {
    if (app.engine == nullptr) {
        return false;
    }
    info = {};
    info.struct_size = sizeof(info);
    std::string undo_name;
    std::string redo_name;
    const InkpodStatus status = app.engine->Invoke(
        [&info, &undo_name, &redo_name](InkpodCore* core) {
            InkpodStatus inner = inkpod_core_history_info(core, &info);
            const auto read_name = [core](std::uint64_t index, std::string& output) {
                InkpodHistoryItem item{};
                item.struct_size = sizeof(item);
                InkpodStatus item_status = inkpod_core_history_item(core, index, &item);
                if (item_status != INKPOD_STATUS_OK || item.name_bytes > UINT64_C(4096)) {
                    return item_status == INKPOD_STATUS_OK
                        ? INKPOD_STATUS_INVALID_STATE
                        : item_status;
                }
                try {
                    output.assign(static_cast<std::size_t>(item.name_bytes), '\0');
                } catch (const std::bad_alloc&) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
                item.name_utf8 = reinterpret_cast<std::uint8_t*>(output.data());
                item.name_capacity = item.name_bytes;
                return inkpod_core_history_item(core, index, &item);
            };
            if (inner == INKPOD_STATUS_OK && info.cursor != 0U) {
                inner = read_name(info.cursor - 1U, undo_name);
            }
            if (inner == INKPOD_STATUS_OK && info.cursor < info.item_count) {
                inner = read_name(info.cursor, redo_name);
            }
            return inner;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    try {
        undo_label = undo_name.empty()
            ? L"元に戻す(&U)\tCtrl+Z"
            : L"元に戻す: " + LocalizedHistoryLabel(undo_name) + L"\tCtrl+Z";
        redo_label = redo_name.empty()
            ? L"やり直し(&R)\tCtrl+Y"
            : L"やり直し: " + LocalizedHistoryLabel(redo_name) + L"\tCtrl+Y";
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

INT_PTR ShowHistoryDialog(
    HINSTANCE instance, HWND owner, HistoryDialogState& state) noexcept {
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_HISTORY),
        owner,
        HistoryDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

struct M6EditorState {
    const wchar_t* title{L"M6 エディター"};
    std::array<const wchar_t*, 5U> parameter_labels{L"P0", L"P1", L"P2", L"P3", L"P4"};
    std::array<std::int32_t, 5U> parameters{};
    std::array<const wchar_t*, 5U> channel_labels{};
    std::array<std::uint32_t, 5U> channel_values{};
    std::size_t channel_count{};
    std::uint32_t channel{};
    std::array<const wchar_t*, 4U> mode_labels{};
    std::array<std::uint32_t, 4U> mode_values{};
    std::size_t mode_count{};
    std::uint32_t mode{};
    std::wstring points;
    const wchar_t* option1_label{L"プレビューして確認"};
    const wchar_t* option2_label{L"45度制約 / 筆圧"};
    bool option1{true};
    bool option2{};
    bool option1_enabled{true};
    bool option2_enabled{true};
    bool close_immediately{};
};

bool ReadSignedDialogValue(HWND dialog, int control, std::int32_t& output) noexcept {
    std::array<wchar_t, 64U> text{};
    if (GetDlgItemTextW(dialog, control, text.data(), static_cast<int>(text.size())) <= 0) {
        return false;
    }
    wchar_t* end{};
    errno = 0;
    const long value = wcstol(text.data(), &end, 10);
    if (errno == ERANGE || end == text.data() || *end != L'\0' || value < INT32_MIN
        || value > INT32_MAX) {
        return false;
    }
    output = static_cast<std::int32_t>(value);
    return true;
}

template <std::size_t Count>
void FillM6Combo(
    HWND combo,
    const std::array<const wchar_t*, Count>& labels,
    const std::array<std::uint32_t, Count>& values,
    std::size_t count,
    std::uint32_t selected_value) noexcept {
    int selected = 0;
    for (std::size_t index = 0; index < count; ++index) {
        const LRESULT item = SendMessageW(
            combo, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(labels[index]));
        if (item == CB_ERR || item == CB_ERRSPACE) {
            continue;
        }
        SendMessageW(combo, CB_SETITEMDATA, item, static_cast<LPARAM>(values[index]));
        if (values[index] == selected_value) {
            selected = static_cast<int>(item);
        }
    }
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
    EnableWindow(combo, count != 0U ? TRUE : FALSE);
}

std::uint32_t SelectedM6Combo(HWND dialog, int control, std::uint32_t fallback) noexcept {
    const LRESULT selected = SendDlgItemMessageW(dialog, control, CB_GETCURSEL, 0, 0);
    if (selected == CB_ERR) {
        return fallback;
    }
    const LRESULT value = SendDlgItemMessageW(
        dialog, control, CB_GETITEMDATA, static_cast<WPARAM>(selected), 0);
    return value == CB_ERR ? fallback : static_cast<std::uint32_t>(value);
}

INT_PTR CALLBACK M6EditorDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<M6EditorState*>(GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG: {
            state = reinterpret_cast<M6EditorState*>(lparam);
            if (state == nullptr) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            SetDlgItemTextW(dialog, IDC_M6_TITLE, state->title);
            constexpr std::array<int, 5U> labels{
                IDC_M6_PARAM0_LABEL,
                IDC_M6_PARAM1_LABEL,
                IDC_M6_PARAM2_LABEL,
                IDC_M6_PARAM3_LABEL,
                IDC_M6_PARAM4_LABEL};
            constexpr std::array<int, 5U> edits{
                IDC_M6_PARAM0, IDC_M6_PARAM1, IDC_M6_PARAM2, IDC_M6_PARAM3, IDC_M6_PARAM4};
            for (std::size_t index = 0; index < edits.size(); ++index) {
                SetDlgItemTextW(dialog, labels[index], state->parameter_labels[index]);
                std::array<wchar_t, 32U> value{};
                _snwprintf_s(
                    value.data(), value.size(), _TRUNCATE, L"%d", state->parameters[index]);
                SetDlgItemTextW(dialog, edits[index], value.data());
            }
            FillM6Combo(
                GetDlgItem(dialog, IDC_M6_CHANNEL),
                state->channel_labels,
                state->channel_values,
                state->channel_count,
                state->channel);
            FillM6Combo(
                GetDlgItem(dialog, IDC_M6_MODE),
                state->mode_labels,
                state->mode_values,
                state->mode_count,
                state->mode);
            SetDlgItemTextW(dialog, IDC_M6_POINTS, state->points.c_str());
            SetDlgItemTextW(dialog, IDC_M6_OPTION1, state->option1_label);
            SetDlgItemTextW(dialog, IDC_M6_OPTION2, state->option2_label);
            CheckDlgButton(dialog, IDC_M6_OPTION1, state->option1 ? BST_CHECKED : BST_UNCHECKED);
            CheckDlgButton(dialog, IDC_M6_OPTION2, state->option2 ? BST_CHECKED : BST_UNCHECKED);
            EnableWindow(GetDlgItem(dialog, IDC_M6_OPTION1), state->option1_enabled ? TRUE : FALSE);
            EnableWindow(GetDlgItem(dialog, IDC_M6_OPTION2), state->option2_enabled ? TRUE : FALSE);
            if (state->close_immediately) {
                PostMessageW(dialog, WM_COMMAND, IDOK, 0);
            }
            return TRUE;
        }
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            if (LOWORD(wparam) == IDOK) {
                constexpr std::array<int, 5U> edits{
                    IDC_M6_PARAM0, IDC_M6_PARAM1, IDC_M6_PARAM2, IDC_M6_PARAM3, IDC_M6_PARAM4};
                for (std::size_t index = 0; index < edits.size(); ++index) {
                    if (!ReadSignedDialogValue(dialog, edits[index], state->parameters[index])) {
                        if (!state->close_immediately) {
                            MessageBoxW(
                                dialog,
                                L"数値パラメーターを10進整数で入力してください。",
                                L"inkpod",
                                MB_OK | MB_ICONWARNING);
                        }
                        return TRUE;
                    }
                }
                state->channel = SelectedM6Combo(dialog, IDC_M6_CHANNEL, state->channel);
                state->mode = SelectedM6Combo(dialog, IDC_M6_MODE, state->mode);
                std::array<wchar_t, 1024U> points{};
                GetDlgItemTextW(
                    dialog, IDC_M6_POINTS, points.data(), static_cast<int>(points.size()));
                try {
                    state->points.assign(points.data());
                } catch (const std::bad_alloc&) {
                    return TRUE;
                }
                state->option1 = IsDlgButtonChecked(dialog, IDC_M6_OPTION1) == BST_CHECKED;
                state->option2 = IsDlgButtonChecked(dialog, IDC_M6_OPTION2) == BST_CHECKED;
                EndDialog(dialog, IDOK);
                return TRUE;
            }
            if (LOWORD(wparam) == IDCANCEL) {
                EndDialog(dialog, IDCANCEL);
                return TRUE;
            }
            break;
        case WM_CLOSE:
            EndDialog(dialog, IDCANCEL);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

INT_PTR ShowM6Editor(
    HINSTANCE instance, HWND owner, bool close_immediately, M6EditorState& state) noexcept {
    state.close_immediately = close_immediately;
    return DialogBoxParamW(
        instance,
        MAKEINTRESOURCEW(IDD_M6_EDITOR),
        owner,
        M6EditorDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
}

INT_PTR CALLBACK M6ProgressDialogProcedure(
    HWND dialog, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AppState*>(GetWindowLongPtrW(dialog, GWLP_USERDATA));
    switch (message) {
        case WM_INITDIALOG:
            state = reinterpret_cast<AppState*>(lparam);
            if (state == nullptr) {
                DestroyWindow(dialog);
                return TRUE;
            }
            SetWindowLongPtrW(dialog, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
            SendDlgItemMessageW(dialog, IDC_M6_PROGRESS_BAR, PBM_SETRANGE32, 0, 1000);
            SetTimer(dialog, kM6ProgressTimer, 100U, nullptr);
            return TRUE;
        case WM_TIMER:
            if (state != nullptr && wparam == kM6ProgressTimer && state->m6_task != nullptr) {
                InkpodM6TaskInfo info{};
                info.struct_size = sizeof(info);
                if (inkpod_m6_task_query(state->m6_task, &info) == INKPOD_STATUS_OK) {
                    const std::uint64_t value = info.total_work == 0U
                        ? 0U
                        : std::min<std::uint64_t>(
                              1000U, info.completed_work * 1000U / info.total_work);
                    SendDlgItemMessageW(dialog, IDC_M6_PROGRESS_BAR, PBM_SETPOS, value, 0);
                    std::array<wchar_t, 64U> text{};
                    _snwprintf_s(
                        text.data(),
                        text.size(),
                        _TRUNCATE,
                        L"処理中... %llu / %llu",
                        static_cast<unsigned long long>(info.completed_work),
                        static_cast<unsigned long long>(info.total_work));
                    SetDlgItemTextW(dialog, IDC_M6_PROGRESS_TEXT, text.data());
                }
                return TRUE;
            }
            break;
        case WM_COMMAND:
            if (LOWORD(wparam) == IDCANCEL && state != nullptr && state->m6_task != nullptr) {
                inkpod_m6_task_cancel(state->m6_task);
                EnableWindow(GetDlgItem(dialog, IDCANCEL), FALSE);
                SetDlgItemTextW(dialog, IDC_M6_PROGRESS_TEXT, L"キャンセル中...");
                return TRUE;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr && state->m6_task != nullptr) {
                inkpod_m6_task_cancel(state->m6_task);
            }
            return TRUE;
        case WM_NCDESTROY:
            KillTimer(dialog, kM6ProgressTimer);
            SetWindowLongPtrW(dialog, GWLP_USERDATA, 0);
            return TRUE;
        default:
            break;
    }
    return FALSE;
}

std::uint32_t CurrentShortcutModifiers(LPARAM key_data) noexcept {
    std::uint32_t modifiers{};
    if ((GetKeyState(VK_CONTROL) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_CONTROL;
    }
    if ((GetKeyState(VK_SHIFT) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_SHIFT;
    }
    if ((GetKeyState(VK_MENU) & 0x8000) != 0) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_ALT;
    }
    if ((static_cast<std::uint64_t>(key_data) & (UINT64_C(1) << 24)) != 0U) {
        modifiers |= INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    }
    return modifiers;
}

UINT ShortcutMenuCommand(std::uint32_t command_id) noexcept {
    for (const auto& entry : kShortcutEntries) {
        if (entry.command_id == command_id) {
            return entry.menu_command;
        }
    }
    return 0U;
}

bool ResolveConfiguredShortcut(
    AppState& state,
    std::uint32_t virtual_key,
    std::uint32_t modifiers,
    UINT& menu_command) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    std::uint32_t command_id{};
    const InkpodStatus status = state.engine->Invoke(
        [virtual_key, modifiers, &command_id](InkpodCore* core) {
            return inkpod_core_shortcut_resolve(
                core, virtual_key, modifiers, &command_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    menu_command = ShortcutMenuCommand(command_id);
    return menu_command != 0U;
}

void ShowCoreError(const AppState& state, HWND owner, const wchar_t* operation) noexcept {
    if (state.smoke_test) {
        return;
    }
    const std::wstring detail = state.engine == nullptr
        ? L"Core engine is not running"
        : state.engine->LastError();
    std::array<wchar_t, 768> message{};
    _snwprintf_s(
        message.data(),
        message.size(),
        _TRUNCATE,
        L"%ls に失敗しました。\n\n%ls",
        operation,
        detail.c_str());
    MessageBoxW(owner, message.data(), L"inkpod", MB_OK | MB_ICONERROR);
}

bool WidePathToUtf8(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept {
    if (path.empty() || path.size() > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int required = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        path.data(),
        static_cast<int>(path.size()),
        nullptr,
        0,
        nullptr,
        nullptr);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return WideCharToMultiByte(
               CP_UTF8,
               WC_ERR_INVALID_CHARS,
               path.data(),
               static_cast<int>(path.size()),
               reinterpret_cast<char*>(output.data()),
               required,
               nullptr,
               nullptr)
        == required;
}

struct ClipboardPrivateHeader {
    std::uint32_t magic{UINT32_C(0x504b4e49)};
    std::uint32_t version{1U};
    std::int32_t origin_x{};
    std::int32_t origin_y{};
    std::uint32_t width{};
    std::uint32_t height{};
    std::uint64_t stride{};
    std::uint64_t bytes{};
};

UINT InkpodClipboardFormat() noexcept {
    static const UINT format = RegisterClipboardFormatW(L"inkpod/typed-rgba-v1");
    return format;
}

bool PublishStandardClipboard(HWND owner, const InkpodClipboard* clipboard) noexcept {
    if (clipboard == nullptr) {
        return false;
    }
    InkpodClipboardRasterBuffer view{};
    view.struct_size = sizeof(view);
    if (inkpod_clipboard_render_rgba8(clipboard, &view) != INKPOD_STATUS_OK
        || view.required_bytes == 0U
        || view.required_bytes > static_cast<std::uint64_t>(SIZE_MAX)) {
        return false;
    }
    std::vector<std::uint8_t> rgba;
    try {
        rgba.resize(static_cast<std::size_t>(view.required_bytes));
    } catch (const std::bad_alloc&) {
        return false;
    }
    view.pixels_rgba8 = rgba.data();
    view.pixel_capacity = rgba.size();
    if (inkpod_clipboard_render_rgba8(clipboard, &view) != INKPOD_STATUS_OK) {
        return false;
    }
    const std::uint64_t dib_bytes_u64 = sizeof(BITMAPV5HEADER) + view.required_bytes;
    const std::uint64_t private_bytes_u64 = sizeof(ClipboardPrivateHeader) + view.required_bytes;
    if (dib_bytes_u64 > static_cast<std::uint64_t>(SIZE_MAX)
        || private_bytes_u64 > static_cast<std::uint64_t>(SIZE_MAX)) {
        return false;
    }
    HGLOBAL dib = GlobalAlloc(GMEM_MOVEABLE, static_cast<SIZE_T>(dib_bytes_u64));
    HGLOBAL typed = GlobalAlloc(GMEM_MOVEABLE, static_cast<SIZE_T>(private_bytes_u64));
    if (dib == nullptr || typed == nullptr) {
        if (dib != nullptr) {
            GlobalFree(dib);
        }
        if (typed != nullptr) {
            GlobalFree(typed);
        }
        return false;
    }
    auto* dib_memory = static_cast<std::uint8_t*>(GlobalLock(dib));
    auto* typed_memory = static_cast<std::uint8_t*>(GlobalLock(typed));
    if (dib_memory == nullptr || typed_memory == nullptr) {
        if (dib_memory != nullptr) {
            GlobalUnlock(dib);
        }
        if (typed_memory != nullptr) {
            GlobalUnlock(typed);
        }
        GlobalFree(dib);
        GlobalFree(typed);
        return false;
    }
    BITMAPV5HEADER header{};
    header.bV5Size = sizeof(header);
    header.bV5Width = static_cast<LONG>(view.width);
    header.bV5Height = -static_cast<LONG>(view.height);
    header.bV5Planes = 1U;
    header.bV5BitCount = 32U;
    header.bV5Compression = BI_BITFIELDS;
    header.bV5SizeImage = static_cast<DWORD>(std::min<std::uint64_t>(
        view.required_bytes, static_cast<std::uint64_t>(UINT32_MAX)));
    header.bV5RedMask = UINT32_C(0x00ff0000);
    header.bV5GreenMask = UINT32_C(0x0000ff00);
    header.bV5BlueMask = UINT32_C(0x000000ff);
    header.bV5AlphaMask = UINT32_C(0xff000000);
    header.bV5CSType = LCS_sRGB;
    std::memcpy(dib_memory, &header, sizeof(header));
    auto* bgra = dib_memory + sizeof(header);
    for (std::uint32_t y = 0U; y < view.height; ++y) {
        for (std::uint32_t x = 0U; x < view.width; ++x) {
            const std::size_t offset = static_cast<std::size_t>(
                static_cast<std::uint64_t>(y) * view.row_stride_bytes
                + static_cast<std::uint64_t>(x) * 4U);
            bgra[offset] = rgba[offset + 2U];
            bgra[offset + 1U] = rgba[offset + 1U];
            bgra[offset + 2U] = rgba[offset];
            bgra[offset + 3U] = rgba[offset + 3U];
        }
    }
    const ClipboardPrivateHeader private_header{
        UINT32_C(0x504b4e49),
        1U,
        view.origin_x,
        view.origin_y,
        view.width,
        view.height,
        view.row_stride_bytes,
        view.required_bytes};
    std::memcpy(typed_memory, &private_header, sizeof(private_header));
    std::memcpy(typed_memory + sizeof(private_header), rgba.data(), rgba.size());
    GlobalUnlock(dib);
    GlobalUnlock(typed);
    if (OpenClipboard(owner) == FALSE) {
        GlobalFree(dib);
        GlobalFree(typed);
        return false;
    }
    EmptyClipboard();
    const bool dib_ok = SetClipboardData(CF_DIBV5, dib) != nullptr;
    if (!dib_ok) {
        GlobalFree(dib);
    }
    const UINT typed_format = InkpodClipboardFormat();
    const bool typed_ok = typed_format != 0U
        && SetClipboardData(typed_format, typed) != nullptr;
    if (!typed_ok) {
        GlobalFree(typed);
    }
    CloseClipboard();
    return dib_ok && typed_ok;
}

bool ImportStandardClipboard(HWND owner, InkpodClipboard*& output) noexcept {
    const UINT typed_format = InkpodClipboardFormat();
    if (OpenClipboard(owner) == FALSE) {
        return false;
    }
    InkpodClipboard* replacement{};
    InkpodStatus status = INKPOD_STATUS_INVALID_ARGUMENT;
    if (typed_format != 0U) {
        HANDLE handle = GetClipboardData(typed_format);
        if (handle != nullptr) {
            const SIZE_T bytes = GlobalSize(handle);
            const auto* memory = static_cast<const std::uint8_t*>(GlobalLock(handle));
            if (memory != nullptr && bytes >= sizeof(ClipboardPrivateHeader)) {
                ClipboardPrivateHeader header{};
                std::memcpy(&header, memory, sizeof(header));
                const bool valid = header.magic == UINT32_C(0x504b4e49)
                    && header.version == 1U && header.width != 0U && header.height != 0U
                    && header.stride >= static_cast<std::uint64_t>(header.width) * 4U
                    && header.bytes <= bytes - sizeof(ClipboardPrivateHeader)
                    && header.stride <= header.bytes
                    && header.height <= header.bytes / header.stride;
                if (valid) {
                    const InkpodClipboardRgbaInput input{
                        sizeof(InkpodClipboardRgbaInput),
                        0U,
                        header.origin_x,
                        header.origin_y,
                        header.width,
                        header.height,
                        memory + sizeof(ClipboardPrivateHeader),
                        header.bytes,
                        header.stride};
                    status = inkpod_clipboard_create_rgba8(&input, &replacement);
                }
            }
            if (memory != nullptr) {
                GlobalUnlock(handle);
            }
        }
    }

    // Private data preserves the Inkpod document origin. For images copied by
    // another application, import a conventional DIB at document origin (0, 0).
    for (const UINT format : {CF_DIBV5, CF_DIB}) {
        if (status == INKPOD_STATUS_OK) {
            break;
        }
        HANDLE handle = GetClipboardData(format);
        if (handle == nullptr) {
            continue;
        }
        const SIZE_T bytes = GlobalSize(handle);
        const auto* memory = static_cast<const std::uint8_t*>(GlobalLock(handle));
        if (memory == nullptr || bytes < sizeof(BITMAPINFOHEADER)) {
            if (memory != nullptr) {
                GlobalUnlock(handle);
            }
            continue;
        }
        BITMAPINFOHEADER header{};
        std::memcpy(&header, memory, sizeof(header));
        const std::int64_t height64 = header.biHeight < 0
            ? -static_cast<std::int64_t>(header.biHeight)
            : static_cast<std::int64_t>(header.biHeight);
        const bool bitfields = header.biCompression == BI_BITFIELDS;
        const bool format_valid = header.biSize >= sizeof(BITMAPINFOHEADER)
            && header.biSize <= bytes && header.biWidth > 0 && height64 > 0
            && height64 <= static_cast<std::int64_t>(UINT32_MAX)
            && header.biPlanes == 1U && (header.biBitCount == 24U || header.biBitCount == 32U)
            && (header.biCompression == BI_RGB || (bitfields && header.biBitCount == 32U));
        std::uint64_t pixel_offset = header.biSize;
        std::array<std::uint32_t, 4U> masks{
            UINT32_C(0x00ff0000), UINT32_C(0x0000ff00),
            UINT32_C(0x000000ff), UINT32_C(0xff000000)};
        if (format_valid && bitfields) {
            if (header.biSize >= sizeof(BITMAPV4HEADER)) {
                std::memcpy(masks.data(), memory + sizeof(BITMAPINFOHEADER), sizeof(masks));
            } else {
                if (bytes - pixel_offset < 3U * sizeof(std::uint32_t)) {
                    GlobalUnlock(handle);
                    continue;
                }
                std::memcpy(masks.data(), memory + pixel_offset, 3U * sizeof(std::uint32_t));
                masks[3] = 0U;
                pixel_offset += 3U * sizeof(std::uint32_t);
            }
        }
        const std::uint64_t width = format_valid
            ? static_cast<std::uint64_t>(header.biWidth)
            : 0U;
        const std::uint64_t bits_per_row = width * header.biBitCount;
        const std::uint64_t row_stride = ((bits_per_row + 31U) / 32U) * 4U;
        const std::uint64_t required = row_stride * static_cast<std::uint64_t>(height64);
        const std::uint64_t rgba_bytes = width * static_cast<std::uint64_t>(height64) * 4U;
        if (!format_valid || pixel_offset > bytes || required > bytes - pixel_offset
            || rgba_bytes > UINT64_C(536870912) || rgba_bytes > SIZE_MAX) {
            GlobalUnlock(handle);
            continue;
        }
        std::vector<std::uint8_t> rgba;
        try {
            rgba.resize(static_cast<std::size_t>(rgba_bytes));
        } catch (const std::bad_alloc&) {
            GlobalUnlock(handle);
            continue;
        }
        const auto channel = [](std::uint32_t value, std::uint32_t mask, std::uint8_t fallback) {
            if (mask == 0U) {
                return fallback;
            }
            std::uint32_t shift{};
            while (((mask >> shift) & 1U) == 0U && shift < 31U) {
                ++shift;
            }
            const std::uint32_t maximum = mask >> shift;
            return static_cast<std::uint8_t>(
                ((value & mask) >> shift) * UINT32_C(255) / maximum);
        };
        const std::uint32_t height = static_cast<std::uint32_t>(height64);
        for (std::uint32_t y = 0U; y < height; ++y) {
            const std::uint32_t source_y = header.biHeight < 0
                ? y
                : height - 1U - y;
            const auto* source = memory + pixel_offset
                + static_cast<std::uint64_t>(source_y) * row_stride;
            auto* destination = rgba.data()
                + static_cast<std::uint64_t>(y) * width * 4U;
            for (std::uint32_t x = 0U; x < width; ++x) {
                if (header.biBitCount == 24U) {
                    destination[x * 4U] = source[x * 3U + 2U];
                    destination[x * 4U + 1U] = source[x * 3U + 1U];
                    destination[x * 4U + 2U] = source[x * 3U];
                    destination[x * 4U + 3U] = 255U;
                } else {
                    std::uint32_t value{};
                    std::memcpy(&value, source + x * 4U, sizeof(value));
                    destination[x * 4U] = channel(value, masks[0], 0U);
                    destination[x * 4U + 1U] = channel(value, masks[1], 0U);
                    destination[x * 4U + 2U] = channel(value, masks[2], 0U);
                    destination[x * 4U + 3U] = bitfields
                        ? channel(value, masks[3], 255U)
                        : 255U;
                }
            }
        }
        const InkpodClipboardRgbaInput input{
            sizeof(InkpodClipboardRgbaInput),
            0U,
            0,
            0,
            static_cast<std::uint32_t>(width),
            height,
            rgba.data(),
            rgba.size(),
            width * 4U};
        status = inkpod_clipboard_create_rgba8(&input, &replacement);
        GlobalUnlock(handle);
    }
    CloseClipboard();
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    inkpod_clipboard_release(&output);
    output = replacement;
    return true;
}

void UpdateFloatingPreview(AppState& state) noexcept {
    if (state.canvas == nullptr) {
        return;
    }
    inkpod::renderer::CanvasFloatingPreview preview{};
    preview.struct_size = sizeof(preview);
    preview.active = state.floating_active ? 1U : 0U;
    preview.bounds = state.floating_bounds;
    preview.transform = state.floating_transform;
    SendMessageW(
        state.canvas,
        inkpod::renderer::kCanvasSetFloatingPreview,
        0,
        reinterpret_cast<LPARAM>(&preview));
}

InkpodStatus BeginFloatingPaste(AppState& state, std::uint32_t mode) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    bool imported_standard{};
    if (state.clipboard == nullptr) {
        if (!ImportStandardClipboard(state.window, state.clipboard)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        imported_standard = true;
    }
    const InkpodClipboard* clipboard = state.clipboard;
    InkpodStatus status = state.engine->Invoke(
        [clipboard, mode](InkpodCore* core) {
            return inkpod_core_paste_begin_mode(core, clipboard, mode);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK && imported_standard
        && mode == INKPOD_PASTE_COMPATIBLE) {
        status = state.engine->Invoke(
            [clipboard](InkpodCore* core) {
                return inkpod_core_paste_begin_mode(
                    core, clipboard, INKPOD_PASTE_ACTIVE_CONVERTED);
            },
            false,
            false);
    }
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    InkpodClipboardRasterBuffer view{};
    view.struct_size = sizeof(view);
    if (inkpod_clipboard_render_rgba8(clipboard, &view) != INKPOD_STATUS_OK
        || view.width > static_cast<std::uint32_t>(INT_MAX)
        || view.height > static_cast<std::uint32_t>(INT_MAX)) {
        state.engine->Invoke(
            [](InkpodCore* core) { return inkpod_core_floating_cancel(core); },
            false,
            false);
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.floating_active = true;
    state.floating_bounds = {
        view.origin_x,
        view.origin_y,
        static_cast<std::int32_t>(view.width),
        static_cast<std::int32_t>(view.height)};
    state.floating_transform = InkpodFloatingTransform{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};
    state.tool = kInteractionFloatingTransform;
    UpdateFloatingPreview(state);
    return INKPOD_STATUS_OK;
}

InkpodStatus SetFloatingTransform(
    AppState& state, const InkpodFloatingTransform& transform) noexcept {
    if (!state.floating_active || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [transform](InkpodCore* core) {
            return inkpod_core_floating_transform(core, &transform);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        state.floating_transform = transform;
        UpdateFloatingPreview(state);
    }
    return status;
}

InkpodStatus ShowFloatingTransformDialog(AppState& state) noexcept {
    if (!state.floating_active) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState geometry{};
    geometry.title = L"フローティング選択の変形";
    geometry.labels = {L"X移動 (px)", L"Y移動 (px)", L"幅 (%)", L"高さ (%)"};
    geometry.values = {
        static_cast<std::int32_t>(std::lround(state.floating_transform.translate_x)),
        static_cast<std::int32_t>(std::lround(state.floating_transform.translate_y)),
        static_cast<std::int32_t>(std::lround(state.floating_transform.scale_x * 100.0)),
        static_cast<std::int32_t>(std::lround(state.floating_transform.scale_y * 100.0))};
    geometry.value_count = 4U;
    if (state.smoke_test) {
        geometry.values = {2, 1, 125, 75};
    }
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, geometry) != IDOK
        || geometry.values[2] <= 0 || geometry.values[3] <= 0) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState rotation{};
    rotation.title = L"回転・基準点";
    rotation.labels = {L"角度 (度)", L"縦横比固定 (0/1)", L"五点基準 (1-5)", nullptr};
    rotation.values = {
        static_cast<std::int32_t>(std::lround(state.floating_transform.rotation_degrees)),
        0,
        INKPOD_RESIZE_ANCHOR_CENTER,
        0};
    rotation.value_count = 3U;
    if (state.smoke_test) {
        rotation.values[0] = 15;
    }
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, rotation) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    if (rotation.values[1] != 0) {
        geometry.values[3] = geometry.values[2];
    }
    const InkpodFloatingTransform transform{
        sizeof(InkpodFloatingTransform),
        0U,
        static_cast<double>(geometry.values[0]),
        static_cast<double>(geometry.values[1]),
        static_cast<double>(geometry.values[2]) / 100.0,
        static_cast<double>(geometry.values[3]) / 100.0,
        static_cast<double>(rotation.values[0])};
    return SetFloatingTransform(state, transform);
}

InkpodStatus UpdateFloatingHandleDrag(
    AppState& state,
    const InkpodStrokeSample& start,
    const InkpodStrokeSample& current,
    bool begin) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds canvas{};
    if (!state.floating_active || !QueryDocument(state, info)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&canvas)) != 1
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (canvas.right - canvas.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto to_document = [&](const InkpodStrokeSample& sample) {
        double x = (static_cast<double>(sample.x) - canvas.left) / zoom;
        double y = (static_cast<double>(sample.y) - canvas.top) / zoom;
        if (state.view_flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.view_flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return std::pair{x, y};
    };
    const auto to_device = [&](double x, double y) {
        if (state.view_flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.view_flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return std::pair{canvas.left + x * zoom, canvas.top + y * zoom};
    };
    const double center_x = static_cast<double>(state.floating_bounds.x)
        + static_cast<double>(state.floating_bounds.width - 1) / 2.0
        + state.floating_transform.translate_x;
    const double center_y = static_cast<double>(state.floating_bounds.y)
        + static_cast<double>(state.floating_bounds.height - 1) / 2.0
        + state.floating_transform.translate_y;
    if (begin) {
        state.floating_drag_start = state.floating_transform;
        state.floating_drag_mode = 1U;
        const double radians = state.floating_transform.rotation_degrees
            * 3.14159265358979323846 / 180.0;
        const double sine = std::sin(radians);
        const double cosine = std::cos(radians);
        const double half_width = static_cast<double>(state.floating_bounds.width - 1)
            * state.floating_transform.scale_x / 2.0;
        const double half_height = static_cast<double>(state.floating_bounds.height - 1)
            * state.floating_transform.scale_y / 2.0;
        const std::array<std::pair<double, double>, 4U> local{
            std::pair{-half_width, -half_height},
            std::pair{half_width, -half_height},
            std::pair{half_width, half_height},
            std::pair{-half_width, half_height}};
        for (const auto& point : local) {
            const auto device = to_device(
                center_x + point.first * cosine - point.second * sine,
                center_y + point.first * sine + point.second * cosine);
            if (std::hypot(
                    device.first - static_cast<double>(start.x),
                    device.second - static_cast<double>(start.y)) <= 14.0) {
                state.floating_drag_mode = 2U;
                break;
            }
        }
        const auto rotation_handle = to_device(
            center_x + half_height * sine,
            center_y - half_height * cosine - 20.0 / zoom);
        if (std::hypot(
                rotation_handle.first - static_cast<double>(start.x),
                rotation_handle.second - static_cast<double>(start.y)) <= 16.0) {
            state.floating_drag_mode = 3U;
        }
        return INKPOD_STATUS_OK;
    }
    const auto start_document = to_document(start);
    const auto current_document = to_document(current);
    InkpodFloatingTransform transform = state.floating_drag_start;
    if (state.floating_drag_mode == 2U) {
        const double start_dx = start_document.first - center_x;
        const double start_dy = start_document.second - center_y;
        const double current_dx = current_document.first - center_x;
        const double current_dy = current_document.second - center_y;
        if (std::abs(start_dx) > 0.01) {
            transform.scale_x *= std::max(0.01, std::abs(current_dx / start_dx));
        }
        if (std::abs(start_dy) > 0.01) {
            transform.scale_y *= std::max(0.01, std::abs(current_dy / start_dy));
        }
    } else if (state.floating_drag_mode == 3U) {
        const double start_angle = std::atan2(
            start_document.second - center_y, start_document.first - center_x);
        const double current_angle = std::atan2(
            current_document.second - center_y, current_document.first - center_x);
        transform.rotation_degrees +=
            (current_angle - start_angle) * 180.0 / 3.14159265358979323846;
    } else {
        transform.translate_x += current_document.first - start_document.first;
        transform.translate_y += current_document.second - start_document.second;
    }
    return SetFloatingTransform(state, transform);
}

InkpodStatus EndFloatingPaste(AppState& state, bool commit) noexcept {
    if (!state.floating_active || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [commit](InkpodCore* core) {
            if (!commit) {
                return inkpod_core_floating_cancel(core);
            }
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_floating_commit(core, &result);
        },
        commit,
        commit);
    if (status == INKPOD_STATUS_OK || !commit) {
        state.floating_active = false;
        state.floating_bounds = {};
        state.floating_gesture_samples.clear();
        UpdateFloatingPreview(state);
        state.tool = INKPOD_TOOL_PENCIL;
    }
    return status;
}

InkpodStatus ResizeDocumentFromDialog(
    AppState& state, const wchar_t* title, bool resolution_mode) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState dimensions{};
    dimensions.title = title;
    dimensions.labels = {L"幅 (px)", L"高さ (px)", L"X DPI (1/1000)", L"Y DPI (1/1000)"};
    dimensions.values = {
        static_cast<std::int32_t>(info.width),
        static_cast<std::int32_t>(info.height),
        static_cast<std::int32_t>(info.dpi_x_milli),
        static_cast<std::int32_t>(info.dpi_y_milli)};
    dimensions.value_count = 4U;
    if (state.smoke_test) {
        if (resolution_mode) {
            dimensions.values[2] = 120000;
            dimensions.values[3] = 120000;
        } else {
            dimensions.values[0] = static_cast<std::int32_t>(info.width + 2U);
            dimensions.values[1] = static_cast<std::int32_t>(info.height + 3U);
        }
    }
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, dimensions) != IDOK
        || dimensions.values[0] <= 0 || dimensions.values[1] <= 0
        || dimensions.values[2] <= 0 || dimensions.values[3] <= 0) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState placement{};
    placement.title = L"配置と再サンプル";
    placement.labels = {L"基準位置 (1:左上 2:右上 3:中央 4:左下 5:右下)", L"再サンプル (0/1)", L"crop内容を確認 (1)", nullptr};
    placement.values = {
        INKPOD_RESIZE_ANCHOR_CENTER,
        resolution_mode ? 0 : 0,
        1,
        0};
    placement.value_count = 3U;
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, placement) != IDOK
        || placement.values[2] == 0) {
        return INKPOD_STATUS_CANCELLED;
    }
    const InkpodDocumentResizeInput input{
        sizeof(InkpodDocumentResizeInput),
        static_cast<std::uint32_t>(placement.values[0]),
        placement.values[1] != 0 ? INKPOD_DOCUMENT_RESIZE_RESAMPLE : 0U,
        static_cast<std::uint32_t>(dimensions.values[0]),
        static_cast<std::uint32_t>(dimensions.values[1]),
        static_cast<std::uint32_t>(dimensions.values[2]),
        static_cast<std::uint32_t>(dimensions.values[3])};
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [input](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_resize_document(core, &input, &result);
              },
              true,
              true);
}

InkpodStatus FitPaperToCaptureFrame(AppState& state) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto frame_right = [](const InkpodFrameRect& frame) {
        return std::max<std::int64_t>(0, static_cast<std::int64_t>(frame.x) + frame.width);
    };
    const auto frame_bottom = [](const InkpodFrameRect& frame) {
        return std::max<std::int64_t>(0, static_cast<std::int64_t>(frame.y) + frame.height);
    };
    const std::uint64_t width = static_cast<std::uint64_t>(std::max({
        static_cast<std::int64_t>(info.width),
        frame_right(info.hundred_frame),
        frame_right(info.reference_frame),
        frame_right(info.drawing_frame),
        frame_right(info.safe_frame)}));
    const std::uint64_t height = static_cast<std::uint64_t>(std::max({
        static_cast<std::int64_t>(info.height),
        frame_bottom(info.hundred_frame),
        frame_bottom(info.reference_frame),
        frame_bottom(info.drawing_frame),
        frame_bottom(info.safe_frame)}));
    if (width > UINT32_MAX || height > UINT32_MAX) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodDocumentResizeInput input{
        sizeof(InkpodDocumentResizeInput),
        INKPOD_RESIZE_ANCHOR_TOP_LEFT,
        0U,
        static_cast<std::uint32_t>(width),
        static_cast<std::uint32_t>(height),
        info.dpi_x_milli,
        info.dpi_y_milli};
    return state.engine->Invoke(
        [input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_resize_document(core, &input, &result);
        },
        true,
        true);
}

bool EnsureDirectory(const std::wstring& path) noexcept {
    if (CreateDirectoryW(path.c_str(), nullptr) != FALSE) {
        return true;
    }
    if (GetLastError() != ERROR_ALREADY_EXISTS) {
        return false;
    }
    const DWORD attributes = GetFileAttributesW(path.c_str());
    return attributes != INVALID_FILE_ATTRIBUTES
        && (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0U;
}

bool RecoveryDirectory(std::wstring& output) noexcept {
    PWSTR local_app_data{};
    if (FAILED(SHGetKnownFolderPath(
            FOLDERID_LocalAppData, KF_FLAG_CREATE, nullptr, &local_app_data))) {
        return false;
    }
    try {
        std::wstring root(local_app_data);
        CoTaskMemFree(local_app_data);
        local_app_data = nullptr;
        root += L"\\inkpod";
        if (!EnsureDirectory(root)) {
            return false;
        }
        root += L"\\Recovery";
        if (!EnsureDirectory(root)) {
            return false;
        }
        output = std::move(root);
        return true;
    } catch (const std::bad_alloc&) {
        if (local_app_data != nullptr) {
            CoTaskMemFree(local_app_data);
        }
        return false;
    }
}

bool PrivateRecoveryPath(
    std::uint64_t uuid_high,
    std::uint64_t uuid_low,
    std::wstring& output) noexcept {
    std::wstring directory;
    if (!RecoveryDirectory(directory)) {
        return false;
    }
    std::array<wchar_t, 96> name{};
    _snwprintf_s(
        name.data(),
        name.size(),
        _TRUNCATE,
        L"\\%016llx%016llx.inkpod",
        static_cast<unsigned long long>(uuid_high),
        static_cast<unsigned long long>(uuid_low));
    try {
        output = directory + name.data();
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool NewestPrivateRecovery(std::wstring& output) noexcept {
    std::wstring directory;
    if (!RecoveryDirectory(directory)) {
        return false;
    }
    std::wstring pattern;
    try {
        pattern = directory + L"\\*.inkpod";
    } catch (const std::bad_alloc&) {
        return false;
    }
    WIN32_FIND_DATAW entry{};
    HANDLE search = FindFirstFileW(pattern.c_str(), &entry);
    if (search == INVALID_HANDLE_VALUE) {
        return false;
    }
    FILETIME newest{};
    bool found{};
    do {
        if ((entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) == 0U
            && (!found || CompareFileTime(&entry.ftLastWriteTime, &newest) > 0)) {
            try {
                output = directory + L"\\" + entry.cFileName;
            } catch (const std::bad_alloc&) {
                FindClose(search);
                return false;
            }
            newest = entry.ftLastWriteTime;
            found = true;
        }
    } while (FindNextFileW(search, &entry) != FALSE);
    FindClose(search);
    return found;
}

InkpodDocumentInfo EmptyDocumentInfo() noexcept {
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return info;
}

bool QueryDocument(AppState& state, InkpodDocumentInfo& info) noexcept {
    info = EmptyDocumentInfo();
    return state.engine != nullptr && state.engine->GetDocumentInfo(info);
}

bool ParseCurvePoints(
    const std::wstring& text, std::vector<InkpodCurvePoint>& points) noexcept {
    points.clear();
    const wchar_t* cursor = text.c_str();
    try {
        while (*cursor != L'\0') {
            wchar_t* end{};
            const unsigned long input = wcstoul(cursor, &end, 10);
            if (end == cursor || *end != L':' || input > UINT16_MAX) {
                return false;
            }
            cursor = end + 1;
            const unsigned long output = wcstoul(cursor, &end, 10);
            if (end == cursor || output > UINT16_MAX
                || (*end != L';' && *end != L'\0')) {
                return false;
            }
            points.push_back(InkpodCurvePoint{
                sizeof(InkpodCurvePoint),
                0U,
                static_cast<std::uint32_t>(input),
                static_cast<std::uint32_t>(output)});
            cursor = *end == L';' ? end + 1 : end;
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    return points.size() >= 2U && points.size() <= 64U;
}

bool ParseM6Stops(
    const std::wstring& text,
    std::vector<M6StopValue>& stops,
    std::size_t minimum_stops = 3U) noexcept {
    stops.clear();
    const wchar_t* cursor = text.c_str();
    try {
        while (*cursor != L'\0') {
            wchar_t* end{};
            const unsigned long position = wcstoul(cursor, &end, 10);
            if (end == cursor || *end != L':' || position > 1000U) {
                return false;
            }
            cursor = end + 1;
            const unsigned long color = wcstoul(cursor, &end, 16);
            if (end == cursor || color > UINT32_MAX
                || (*end != L';' && *end != L'\0')) {
                return false;
            }
            stops.push_back(M6StopValue{
                static_cast<std::uint32_t>(position), static_cast<std::uint32_t>(color)});
            cursor = *end == L';' ? end + 1 : end;
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    return stops.size() >= minimum_stops && stops.size() <= 16U
        && std::is_sorted(
            stops.begin(),
            stops.end(),
            [](const M6StopValue& left, const M6StopValue& right) {
                return left.position_milli < right.position_milli;
            })
        && stops.front().position_milli == 0U && stops.back().position_milli == 1000U;
}

InkpodColorValue M6Color(std::uint32_t rgba) noexcept {
    return InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        static_cast<std::uint16_t>((rgba >> 24U) & 0xffU),
        static_cast<std::uint16_t>((rgba >> 16U) & 0xffU),
        static_cast<std::uint16_t>((rgba >> 8U) & 0xffU),
        static_cast<std::uint16_t>(rgba & 0xffU)};
}

std::uint32_t ColorToRgba8(const InkpodColorValue& color) noexcept {
    const auto channel = [&](std::uint16_t value) {
        return color.depth == INKPOD_COLOR_DEPTH_16
            ? static_cast<std::uint32_t>((static_cast<std::uint32_t>(value) + 128U) / 257U)
            : static_cast<std::uint32_t>(value & 0xffU);
    };
    return (channel(color.red) << 24U) | (channel(color.green) << 16U)
        | (channel(color.blue) << 8U) | channel(color.alpha);
}

void SetDrawingColor(AppState& state, InkpodColorValue color) noexcept {
    color.struct_size = sizeof(InkpodColorValue);
    state.drawing_color = color;
    state.color_rgba = ColorToRgba8(color);
}

InkpodStatus ShowDrawingColorEditor(AppState& state) noexcept {
    ViewOptionsDialogState format{};
    format.title = L"描画色形式";
    format.labels = {L"深度 (8/16)", L"編集方式 (1:RGB 2:HSV)", L"Alpha表示 (1:数値 2:%)", nullptr};
    format.values = {
        state.drawing_color.depth == INKPOD_COLOR_DEPTH_16 ? 16 : 8,
        1,
        1,
        0};
    format.value_count = 3U;
    if (state.smoke_test) {
        format.values = {16, 2, 2, 0};
    }
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, format) != IDOK
        || (format.values[0] != 8 && format.values[0] != 16)
        || (format.values[1] != 1 && format.values[1] != 2)) {
        return INKPOD_STATUS_CANCELLED;
    }
    const std::uint32_t maximum = format.values[0] == 16 ? UINT16_MAX : UINT8_MAX;
    ViewOptionsDialogState values{};
    values.title = format.values[1] == 1 ? L"RGB / Alpha" : L"HSV / Alpha";
    values.value_count = 4U;
    if (format.values[1] == 1) {
        values.labels = {L"R", L"G", L"B", format.values[2] == 2 ? L"Alpha (%)" : L"Alpha"};
        values.values = {
            state.drawing_color.depth == INKPOD_COLOR_DEPTH_16
                ? state.drawing_color.red
                : static_cast<std::int32_t>(state.drawing_color.red),
            state.drawing_color.green,
            state.drawing_color.blue,
            format.values[2] == 2
                ? static_cast<std::int32_t>(
                      (static_cast<std::uint64_t>(state.drawing_color.alpha) * 100U
                          + maximum / 2U)
                      / maximum)
                : state.drawing_color.alpha};
        if (state.smoke_test) {
            values.values = {65535, 32768, 0, 50};
        }
    } else {
        values.labels = {L"H (0-359)", L"S (0-1000)", L"V (0-1000)", format.values[2] == 2 ? L"Alpha (%)" : L"Alpha"};
        values.values = {30, 1000, 1000, format.values[2] == 2 ? 100 : static_cast<std::int32_t>(maximum)};
        if (state.smoke_test) {
            values.values = {210, 750, 800, 50};
        }
    }
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, values) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    InkpodColorValue color{};
    color.struct_size = sizeof(color);
    color.depth = format.values[0] == 16 ? INKPOD_COLOR_DEPTH_16 : INKPOD_COLOR_DEPTH_8;
    if (format.values[1] == 1) {
        if (values.values[0] < 0 || values.values[1] < 0 || values.values[2] < 0
            || static_cast<std::uint32_t>(values.values[0]) > maximum
            || static_cast<std::uint32_t>(values.values[1]) > maximum
            || static_cast<std::uint32_t>(values.values[2]) > maximum) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        color.red = static_cast<std::uint16_t>(values.values[0]);
        color.green = static_cast<std::uint16_t>(values.values[1]);
        color.blue = static_cast<std::uint16_t>(values.values[2]);
    } else {
        if (values.values[0] < 0 || values.values[0] >= 360 || values.values[1] < 0
            || values.values[1] > 1000 || values.values[2] < 0 || values.values[2] > 1000) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const double hue = static_cast<double>(values.values[0]);
        const double saturation = static_cast<double>(values.values[1]) / 1000.0;
        const double value = static_cast<double>(values.values[2]) / 1000.0;
        const double chroma = value * saturation;
        const double section = hue / 60.0;
        const double secondary = chroma * (1.0 - std::abs(std::fmod(section, 2.0) - 1.0));
        double red{};
        double green{};
        double blue{};
        if (section < 1.0) {
            red = chroma;
            green = secondary;
        } else if (section < 2.0) {
            red = secondary;
            green = chroma;
        } else if (section < 3.0) {
            green = chroma;
            blue = secondary;
        } else if (section < 4.0) {
            green = secondary;
            blue = chroma;
        } else if (section < 5.0) {
            red = secondary;
            blue = chroma;
        } else {
            red = chroma;
            blue = secondary;
        }
        const double match = value - chroma;
        color.red = static_cast<std::uint16_t>(std::lround((red + match) * maximum));
        color.green = static_cast<std::uint16_t>(std::lround((green + match) * maximum));
        color.blue = static_cast<std::uint16_t>(std::lround((blue + match) * maximum));
    }
    if (format.values[2] == 2) {
        if (values.values[3] < 0 || values.values[3] > 100) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        color.alpha = static_cast<std::uint16_t>(
            (static_cast<std::uint64_t>(values.values[3]) * maximum + 50U) / 100U);
    } else {
        if (values.values[3] < 0 || static_cast<std::uint32_t>(values.values[3]) > maximum) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        color.alpha = static_cast<std::uint16_t>(values.values[3]);
    }
    SetDrawingColor(state, color);
    return INKPOD_STATUS_OK;
}

InkpodFilterInput M6FilterInputFor(const M6FilterJob& job) noexcept {
    InkpodFilterInput input{};
    input.struct_size = sizeof(input);
    input.kind = job.kind;
    input.plane_id = job.plane_id;
    input.channel = job.channel;
    input.interpolation = job.interpolation;
    input.parameter_0 = job.parameters[0];
    input.parameter_1 = job.parameters[1];
    input.parameter_2 = job.parameters[2];
    input.parameter_3 = job.parameters[3];
    input.parameter_4 = job.parameters[4];
    if (!job.points.empty()) {
        input.points = job.points.data();
        input.point_count = job.points.size();
        input.point_stride_bytes = sizeof(InkpodCurvePoint);
    }
    return input;
}

M6AdjustmentUiState* CurrentM6Adjustment(AppState& state) noexcept {
    const auto found = std::find_if(
        state.m6_adjustments.begin(),
        state.m6_adjustments.end(),
        [&state](const M6AdjustmentUiState& adjustment) {
            return adjustment.id == state.m6_adjustment_id;
        });
    return found == state.m6_adjustments.end() ? nullptr : &*found;
}

bool FormatCurvePoints(
    const std::vector<InkpodCurvePoint>& points, std::wstring& text) noexcept {
    try {
        text.clear();
        for (std::size_t index = 0; index < points.size(); ++index) {
            if (index != 0U) {
                text.push_back(L';');
            }
            text.append(std::to_wstring(points[index].input));
            text.push_back(L':');
            text.append(std::to_wstring(points[index].output));
        }
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

InkpodStatus StartM6Task(
    AppState& state,
    bool preview_prompt,
    std::function<InkpodStatus(InkpodCore*, InkpodM6Task*)> operation) noexcept {
    if (state.engine == nullptr || state.m6_task != nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodM6Task* task{};
    InkpodStatus status = inkpod_m6_task_create(&task);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (state.smoke_test) {
        status = state.engine->Invoke(
            [task, operation = std::move(operation)](InkpodCore* core) {
                return operation(core, task);
            },
            true,
            true);
        if (status == INKPOD_STATUS_OK && preview_prompt) {
            status = state.engine->Invoke(
                [](InkpodCore* core) {
                    InkpodDispatchResult result{};
                    result.struct_size = sizeof(result);
                    return inkpod_core_filter_preview_apply(core, &result);
                },
                true,
                true);
        }
        inkpod_m6_task_release(&task);
        return status;
    }
    state.m6_task = task;
    state.m6_preview_prompt = preview_prompt;
    state.m6_progress = CreateDialogParamW(
        state.instance,
        MAKEINTRESOURCEW(IDD_M6_PROGRESS),
        state.window,
        M6ProgressDialogProcedure,
        reinterpret_cast<LPARAM>(&state));
    if (state.m6_progress == nullptr) {
        inkpod_m6_task_release(&state.m6_task);
        return INKPOD_STATUS_INVALID_STATE;
    }
    ShowWindow(state.m6_progress, SW_SHOW);
    const HWND window = state.window;
    if (!state.engine->Enqueue(
            [task, operation = std::move(operation)](InkpodCore* core) {
                return operation(core, task);
            },
            true,
            true,
            true,
            [window](InkpodStatus completion_status) {
                PostMessageW(window, kM6TaskCompleted, completion_status, 0);
            })) {
        DestroyWindow(state.m6_progress);
        state.m6_progress = nullptr;
        inkpod_m6_task_release(&state.m6_task);
        state.m6_preview_prompt = false;
        return INKPOD_STATUS_INVALID_STATE;
    }
    return INKPOD_STATUS_OK;
}

std::uint32_t M6FilterKindForCommand(UINT command) noexcept {
    switch (command) {
        case IDM_FILTER_SHARPEN_WEAK:
            return INKPOD_FILTER_SHARPEN_WEAK;
        case IDM_FILTER_SHARPEN_STRONG:
            return INKPOD_FILTER_SHARPEN_STRONG;
        case IDM_FILTER_BLUR_WEAK:
            return INKPOD_FILTER_BLUR_WEAK;
        case IDM_FILTER_BLUR_STRONG:
            return INKPOD_FILTER_BLUR_STRONG;
        case IDM_FILTER_GAUSSIAN:
            return INKPOD_FILTER_GAUSSIAN_BLUR;
        case IDM_FILTER_INVERT:
            return INKPOD_FILTER_INVERT;
        case IDM_FILTER_AUTO_CONTRAST:
            return INKPOD_FILTER_AUTO_CONTRAST;
        case IDM_FILTER_BRIGHTNESS:
            return INKPOD_FILTER_BRIGHTNESS_CONTRAST;
        case IDM_FILTER_TONE_CURVE:
            return INKPOD_FILTER_TONE_CURVE;
        case IDM_FILTER_LEVELS:
            return INKPOD_FILTER_LEVELS;
        case IDM_FILTER_HSV:
            return INKPOD_FILTER_HSV;
        case IDM_FILTER_COLOR_BALANCE:
            return INKPOD_FILTER_COLOR_BALANCE;
        case IDM_FILTER_UNSHARP:
            return INKPOD_FILTER_UNSHARP_MASK;
        default:
            return 0U;
    }
}

bool ConfigureM6FilterEditor(
    AppState& state, UINT command, M6FilterJob& job) noexcept {
    job.kind = M6FilterKindForCommand(command);
    if (job.kind == 0U) {
        return false;
    }
    M6EditorState editor{};
    editor.title = L"フィルタ（選択範囲があればその内側だけに適用）";
    editor.parameter_labels = {
        L"P0 / radius", L"P1 / amount", L"P2", L"P3", L"P4"};
    editor.channel_labels = {L"RGB", L"Red", L"Green", L"Blue"};
    editor.channel_values = {
        INKPOD_FILTER_CHANNEL_RGB,
        INKPOD_FILTER_CHANNEL_RED,
        INKPOD_FILTER_CHANNEL_GREEN,
        INKPOD_FILTER_CHANNEL_BLUE};
    editor.channel_count = editor.channel_labels.size();
    editor.channel = INKPOD_FILTER_CHANNEL_RGB;
    editor.mode_labels = {L"Bezier", L"B-spline", nullptr, nullptr};
    editor.mode_values = {INKPOD_CURVE_BEZIER, INKPOD_CURVE_BSPLINE, 0U, 0U};
    editor.mode_count = 2U;
    editor.mode = INKPOD_CURVE_BEZIER;
    editor.points = L"0:0;32768:32768;65535:65535";
    editor.option1 = true;
    editor.option2_enabled = false;
    if (job.kind == INKPOD_FILTER_GAUSSIAN_BLUR) {
        editor.parameters = {3, 1000, 0, 0, 0};
    } else if (job.kind == INKPOD_FILTER_LEVELS) {
        editor.parameters = {0, 1000, 65535, 0, 65535};
    } else if (job.kind == INKPOD_FILTER_UNSHARP_MASK) {
        editor.parameters = {2, 1000, 0, 0, 0};
    }
    if (ShowM6Editor(state.instance, state.window, state.smoke_test, editor) != IDOK) {
        return false;
    }
    job.channel = editor.channel;
    job.interpolation = editor.mode;
    job.parameters = editor.parameters;
    job.preview = editor.option1;
    if (job.kind == INKPOD_FILTER_TONE_CURVE
        && !ParseCurvePoints(editor.points, job.points)) {
        if (!state.smoke_test) {
            MessageBoxW(
                state.window,
                L"トーンカーブ点は input:output;...（各0～65535）で2点以上指定してください。",
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

InkpodStatus QueueM6Filter(AppState& state, M6FilterJob job) noexcept {
    return StartM6Task(
        state,
        job.preview,
        [job = std::move(job)](InkpodCore* core, InkpodM6Task* task) {
            const InkpodFilterInput input = M6FilterInputFor(job);
            InkpodFilterPreviewInfo preview{};
            preview.struct_size = sizeof(preview);
            InkpodStatus status = inkpod_core_filter_preview_begin_task(
                core, &input, task, &preview);
            if (status == INKPOD_STATUS_OK && !job.preview) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_filter_preview_apply(core, &result);
                if (status != INKPOD_STATUS_OK) {
                    inkpod_core_filter_preview_cancel(core, &preview);
                }
            }
            return status;
        });
}

bool ConfigureM6AdjustmentEditor(
    AppState& state, M6FilterJob& job, bool update) noexcept {
    M6EditorState editor{};
    editor.title = L"調整レイヤー（作成後も同じ項目から再編集可能）";
    editor.parameter_labels = {
        L"P0 / 明るさ / shadow",
        L"P1 / contrast / gamma",
        L"P2 / highlight",
        L"P3 / output shadow",
        L"P4 / output highlight"};
    editor.channel_labels = {
        L"明るさ・コントラスト", L"トーンカーブ", L"レベル補正", nullptr, nullptr};
    editor.channel_values = {
        INKPOD_FILTER_BRIGHTNESS_CONTRAST,
        INKPOD_FILTER_TONE_CURVE,
        INKPOD_FILTER_LEVELS,
        0U,
        0U};
    editor.channel_count = 3U;
    editor.channel = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
    editor.mode_labels = {L"Bezier", L"B-spline", nullptr, nullptr};
    editor.mode_values = {INKPOD_CURVE_BEZIER, INKPOD_CURVE_BSPLINE, 0U, 0U};
    editor.mode_count = 2U;
    editor.mode = INKPOD_CURVE_BEZIER;
    editor.points = L"0:0;32768:32768;65535:65535";
    editor.option1 = false;
    editor.option2 = false;
    editor.option1_enabled = false;
    editor.option2_enabled = false;
    if (update) {
        const M6AdjustmentUiState* current = CurrentM6Adjustment(state);
        if (current == nullptr) {
            return false;
        }
        try {
            job = current->job;
        } catch (const std::bad_alloc&) {
            return false;
        }
        editor.channel = job.kind;
        editor.mode = job.interpolation;
        editor.parameters = job.parameters;
        if (job.kind == INKPOD_FILTER_TONE_CURVE
            && !FormatCurvePoints(job.points, editor.points)) {
            return false;
        }
    }
    if (ShowM6Editor(state.instance, state.window, state.smoke_test, editor) != IDOK) {
        return false;
    }
    job.kind = editor.channel;
    job.channel = INKPOD_FILTER_CHANNEL_RGB;
    job.interpolation = editor.mode;
    job.parameters = editor.parameters;
    if (job.kind == INKPOD_FILTER_TONE_CURVE
        && !ParseCurvePoints(editor.points, job.points)) {
        if (!state.smoke_test) {
            MessageBoxW(
                state.window,
                L"トーンカーブ点は input:output;... 形式で2点以上指定してください。",
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

InkpodStatus CreateOrUpdateM6Adjustment(
    AppState& state, M6FilterJob job, bool update) noexcept {
    if (state.engine == nullptr || (update && state.m6_adjustment_id == 0U)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t layer_id = state.m6_adjustment_id;
    std::shared_ptr<M6AdjustmentUiState> pending;
    try {
        std::string name;
        if (update) {
            const M6AdjustmentUiState* current = CurrentM6Adjustment(state);
            if (current == nullptr) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            name = current->name;
        } else {
            name = "M6 Adjustment " + std::to_string(state.m6_adjustments.size() + 1U);
            state.m6_adjustments.reserve(state.m6_adjustments.size() + 1U);
        }
        pending = std::make_shared<M6AdjustmentUiState>(
            M6AdjustmentUiState{layer_id, true, std::move(job), std::move(name)});
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t created_id{};
    const InkpodStatus status = state.engine->Invoke(
        [pending, update, layer_id, &created_id](InkpodCore* core) {
            const InkpodFilterInput input = M6FilterInputFor(pending->job);
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            if (update) {
                return inkpod_core_adjustment_update(core, layer_id, &input, &result);
            }
            return inkpod_core_adjustment_create(
                core,
                &input,
                reinterpret_cast<const std::uint8_t*>(pending->name.data()),
                pending->name.size(),
                &result,
                &created_id);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK && !update) {
        pending->id = created_id;
        state.m6_adjustment_id = created_id;
        state.m6_adjustment_visible = true;
        state.m6_adjustments.push_back(std::move(*pending));
    } else if (status == INKPOD_STATUS_OK) {
        M6AdjustmentUiState* current = CurrentM6Adjustment(state);
        if (current != nullptr) {
            current->job = std::move(pending->job);
        }
    }
    return status;
}

InkpodStatus SetM6AdjustmentVisibility(AppState& state, bool visible) noexcept {
    if (state.engine == nullptr || state.m6_adjustment_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t layer_id = state.m6_adjustment_id;
    M6AdjustmentUiState* current = CurrentM6Adjustment(state);
    if (current == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [layer_id, visible, current](InkpodCore* core) {
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_SET_LAYER_PROPERTIES;
            edit.flags = INKPOD_NODE_EDITABLE | (visible ? INKPOD_NODE_VISIBLE : 0U);
            edit.object_id = layer_id;
            edit.opacity_milli = 1000U;
            edit.name_utf8 = reinterpret_cast<const std::uint8_t*>(current->name.data());
            edit.name_bytes = current->name.size();
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t ignored{};
            return inkpod_core_tree_edit(core, &edit, &result, &ignored);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK) {
        current->visible = visible;
    }
    return status;
}

bool SelectM6Adjustment(AppState& state, bool next) noexcept {
    if (state.m6_adjustments.empty()) {
        return false;
    }
    const auto current = std::find_if(
        state.m6_adjustments.begin(),
        state.m6_adjustments.end(),
        [&state](const M6AdjustmentUiState& adjustment) {
            return adjustment.id == state.m6_adjustment_id;
        });
    std::size_t index = current == state.m6_adjustments.end()
        ? 0U
        : static_cast<std::size_t>(current - state.m6_adjustments.begin());
    if (next) {
        index = (index + 1U) % state.m6_adjustments.size();
    } else {
        index = (index + state.m6_adjustments.size() - 1U)
            % state.m6_adjustments.size();
    }
    state.m6_adjustment_id = state.m6_adjustments[index].id;
    state.m6_adjustment_visible = state.m6_adjustments[index].visible;
    return true;
}

std::vector<InkpodGradientStop> M6GradientStops(const std::vector<M6StopValue>& values) {
    std::vector<InkpodGradientStop> stops;
    stops.reserve(values.size());
    for (const M6StopValue& value : values) {
        stops.push_back(InkpodGradientStop{
            sizeof(InkpodGradientStop),
            0U,
            value.position_milli,
            0U,
            M6Color(value.rgba)});
    }
    return stops;
}

InkpodStatus QueueBoundaryAirbrush(AppState& state, const M6ToolOptions& options) noexcept {
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodColorValue> colors;
    try {
        colors.reserve(options.stops.size());
        for (const M6StopValue& stop : options.stops) {
            colors.push_back(M6Color(stop.rgba));
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint32_t width = static_cast<std::uint32_t>(options.parameters[0]);
    const std::uint32_t strength = static_cast<std::uint32_t>(options.parameters[1]);
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [colors = std::move(colors), width, strength, plane_id](InkpodCore* core) {
                   InkpodBoundaryAirbrushInput input{};
                   input.struct_size = sizeof(input);
                   input.plane_id = plane_id;
                   input.width = width;
                   input.strength_milli = strength;
                   input.colors = InkpodColorArray{
                       sizeof(InkpodColorArray),
                       0U,
                       INKPOD_FEATURE_NONE,
                       colors.data(),
                       colors.size(),
                       sizeof(InkpodColorValue)};
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_boundary_airbrush(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

bool ConfigureM6Effect(AppState& state, UINT command) noexcept {
    M6EditorState editor{};
    editor.option1 = false;
    editor.option2 = false;
    editor.channel_labels = {L"ペン", L"矩形", L"折れ線", L"投げ縄", nullptr};
    editor.channel_values = {
        INKPOD_SELECTION_TRACE,
        INKPOD_SELECTION_RECTANGLE,
        INKPOD_SELECTION_POLYLINE,
        INKPOD_SELECTION_LASSO,
        0U};
    editor.channel_count = 4U;
    editor.channel = INKPOD_SELECTION_TRACE;
    editor.points = L"0:00000000;500:80808080;1000:ffffffff";
    std::uint32_t interaction{};
    switch (command) {
        case IDM_EFFECT_GRADIENT:
        case IDM_EFFECT_ALPHA_GRADIENT:
            editor.title = command == IDM_EFFECT_GRADIENT
                ? L"グラデーション（3～16 stops、Canvasをドラッグ）"
                : L"アルファグラデーション（RGBは保持、Canvasをドラッグ）";
            editor.parameter_labels = {L"未使用", L"未使用", L"未使用", L"未使用", L"未使用"};
            editor.channel_labels = {L"合成", L"上書き", nullptr, nullptr, nullptr};
            editor.channel_values = {
                INKPOD_GRADIENT_COMPOSITE, INKPOD_GRADIENT_OVERWRITE, 0U, 0U, 0U};
            editor.channel_count = 2U;
            editor.channel = INKPOD_GRADIENT_OVERWRITE;
            editor.mode_labels = {L"線形", L"放射", nullptr, nullptr};
            editor.mode_values = {INKPOD_GRADIENT_LINEAR, INKPOD_GRADIENT_RADIAL, 0U, 0U};
            editor.mode_count = 2U;
            editor.mode = INKPOD_GRADIENT_LINEAR;
            editor.option1_label = L"ディザー";
            editor.option2_label = L"45度制約";
            interaction = command == IDM_EFFECT_GRADIENT ? kInteractionM6Gradient
                                                         : kInteractionM6AlphaGradient;
            break;
        case IDM_EFFECT_AIRBRUSH:
            editor.title = L"エアブラシ（Canvasをドラッグ）";
            editor.parameter_labels = {
                L"半径 milli", L"硬さ 0-1000", L"間隔 milli", L"不透明度", L"fade"};
            editor.parameters = {8000, 500, 2000, 500, 0};
            editor.channel_count = 0U;
            editor.mode_count = 0U;
            editor.points.clear();
            editor.option1 = true;
            editor.option2 = true;
            editor.option1_label = L"筆圧で不透明度";
            editor.option2_label = L"筆圧でサイズ";
            interaction = kInteractionM6Airbrush;
            break;
        case IDM_EFFECT_BOUNDARY_AIRBRUSH:
            editor.title = L"境界色エアブラシ（現在の選択範囲へ適用）";
            editor.parameter_labels = {L"幅 px", L"強さ 0-1000", L"未使用", L"未使用", L"未使用"};
            editor.parameters = {3, 500, 0, 0, 0};
            editor.channel_count = 0U;
            editor.mode_count = 0U;
            editor.points = L"0:ff0000ff;500:00ff00ff;1000:0000ffff";
            editor.option1_enabled = false;
            editor.option2_enabled = false;
            break;
        case IDM_EFFECT_BLUR:
            editor.title = L"ぼかしツール（領域をCanvasで指定）";
            editor.parameter_labels = {
                L"ぼかし半径", L"強さ 0-1000", L"ペン径 px", L"未使用", L"未使用"};
            editor.parameters = {3, 750, 24, 0, 0};
            editor.mode_count = 0U;
            editor.points.clear();
            editor.option1 = true;
            editor.option1_label = L"ペン範囲を筆圧で細くする";
            editor.option2_enabled = false;
            interaction = kInteractionM6Blur;
            break;
        case IDM_EFFECT_STAMP:
            editor.title = L"スタンプ（Alt+クリックでsource、次にCanvasをドラッグ）";
            editor.parameter_labels = {
                L"半径 milli", L"硬さ 0-1000", L"間隔 milli", L"不透明度", L"未使用"};
            editor.parameters = {8000, 750, 2000, 1000, 0};
            editor.channel_count = 0U;
            editor.mode_labels = {L"円形", L"矩形", nullptr, nullptr};
            editor.mode_values = {INKPOD_STAMP_ROUND, INKPOD_STAMP_SQUARE, 0U, 0U};
            editor.mode_count = 2U;
            editor.mode = INKPOD_STAMP_ROUND;
            editor.points.clear();
            editor.option1 = true;
            editor.option2 = true;
            editor.option1_label = L"筆圧で不透明度";
            editor.option2_label = L"筆圧でサイズ";
            interaction = kInteractionM6Stamp;
            break;
        case IDM_EFFECT_DUST:
            editor.title = L"ゴミ取り（全体または領域を指定、vector planeは拒否）";
            editor.parameter_labels = {
                L"最大pixel数", L"ペン径 px", L"未使用", L"未使用", L"未使用"};
            editor.parameters = {8, 24, 0, 0, 0};
            editor.channel_labels = {L"全体", L"ペン", L"矩形", L"折れ線", L"投げ縄"};
            editor.channel_values = {
                0U,
                INKPOD_SELECTION_TRACE,
                INKPOD_SELECTION_RECTANGLE,
                INKPOD_SELECTION_POLYLINE,
                INKPOD_SELECTION_LASSO};
            editor.channel_count = 5U;
            editor.channel = 0U;
            editor.mode_labels = {L"前景ゴミ除去", L"透明穴埋め", L"色外れ置換", nullptr};
            editor.mode_values = {
                INKPOD_DUST_REMOVE_FOREGROUND,
                INKPOD_DUST_FILL_TRANSPARENT_HOLES,
                INKPOD_DUST_REPLACE_COLOR_OUTLIERS,
                0U};
            editor.mode_count = 3U;
            editor.mode = INKPOD_DUST_REMOVE_FOREGROUND;
            editor.points.clear();
            editor.option1 = true;
            editor.option1_label = L"プレビューして確認";
            editor.option2_enabled = false;
            interaction = kInteractionM6Dust;
            break;
        default:
            return false;
    }
    if (ShowM6Editor(state.instance, state.window, state.smoke_test, editor) != IDOK) {
        return false;
    }
    M6ToolOptions options{};
    options.parameters = editor.parameters;
    options.shape = editor.channel;
    options.mode = editor.mode;
    options.option = editor.option1;
    options.option2 = editor.option2;
    if (command == IDM_EFFECT_GRADIENT || command == IDM_EFFECT_ALPHA_GRADIENT
        || command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        const std::size_t minimum_stops = command == IDM_EFFECT_BOUNDARY_AIRBRUSH ? 2U : 3U;
        if (!ParseM6Stops(editor.points, options.stops, minimum_stops)) {
            if (!state.smoke_test) {
                MessageBoxW(
                    state.window,
                    command == IDM_EFFECT_BOUNDARY_AIRBRUSH
                        ? L"境界色は position:RRGGBBAA;... 形式で2～16個、0から1000まで昇順に指定してください。"
                        : L"stopは position:RRGGBBAA;... 形式で3～16個、0から1000まで昇順に指定してください。",
                    L"inkpod",
                    MB_OK | MB_ICONWARNING);
            }
            return false;
        }
    }
    if (command == IDM_EFFECT_GRADIENT || command == IDM_EFFECT_ALPHA_GRADIENT) {
        options.parameters[0] = editor.option1 ? 1 : 0;
    }
    state.m6_options = std::move(options);
    state.tool = interaction;
    state.m6_samples.clear();
    if (command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        state.tool = INKPOD_TOOL_BRUSH;
        return QueueBoundaryAirbrush(state, state.m6_options) == INKPOD_STATUS_OK;
    }
    return true;
}

InkpodStatus QueueM6GradientGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples, bool alpha_only) noexcept {
    InkpodDocumentInfo document{};
    if (samples.size() < 2U || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodGradientStop> stops;
    try {
        stops = M6GradientStops(state.m6_options.stops);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), stops = std::move(stops), options, plane_id, alpha_only](
                   InkpodCore* core) {
                   InkpodLocatorOutput start{};
                   InkpodLocatorOutput end{};
                   start.struct_size = sizeof(start);
                   end.struct_size = sizeof(end);
                   InkpodStatus status = inkpod_core_locator_sample(
                       core, 0U, samples.front().x, samples.front().y, &start);
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_core_locator_sample(
                           core, 0U, samples.back().x, samples.back().y, &end);
                   }
                   InkpodGradientInput input{};
                   input.struct_size = sizeof(input);
                   input.kind = options.mode;
                   input.feature_flags = options.option2
                       ? INKPOD_GRADIENT_FLAG_CONSTRAIN_45
                       : INKPOD_FEATURE_NONE;
                   input.plane_id = plane_id;
                   input.mode = options.shape;
                   input.dither = options.parameters[0] != 0 ? 1U : 0U;
                   input.start_x_milli = static_cast<std::int64_t>(start.document_x) * 1000;
                   input.start_y_milli = static_cast<std::int64_t>(start.document_y) * 1000;
                   input.end_x_milli = static_cast<std::int64_t>(end.document_x) * 1000;
                   input.end_y_milli = static_cast<std::int64_t>(end.document_y) * 1000;
                   input.stops = stops.data();
                   input.stop_count = stops.size();
                   input.stop_stride_bytes = sizeof(InkpodGradientStop);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   if (status != INKPOD_STATUS_OK) {
                       return status;
                   }
                   return alpha_only ? inkpod_core_alpha_gradient(core, &input, &result)
                                     : inkpod_core_effect_gradient(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6AirbrushGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (samples.empty() || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodColorValue color = M6Color(state.color_rgba);
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), options, color, plane_id](InkpodCore* core) {
                   InkpodAirbrushGestureInput input{};
                   input.struct_size = sizeof(input);
                   input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
                   input.feature_flags = options.option2 ? INKPOD_EFFECT_FLAG_PRESSURE_SIZE : 0U;
                   if (options.option) {
                       input.feature_flags |= INKPOD_EFFECT_FLAG_PRESSURE_OPACITY;
                   }
                   input.plane_id = plane_id;
                   input.radius_milli = static_cast<std::uint32_t>(options.parameters[0]);
                    input.hardness_milli = static_cast<std::uint32_t>(options.parameters[1]);
                    input.spacing_milli = static_cast<std::uint32_t>(options.parameters[2]);
                    input.opacity_milli = static_cast<std::uint32_t>(options.parameters[3]);
                    input.fade_milli = static_cast<std::uint32_t>(options.parameters[4]);
                   input.continuous_dabs = 1U;
                   input.color = color;
                   input.samples = samples.data();
                   input.sample_count = samples.size();
                   input.sample_stride_bytes = sizeof(InkpodStrokeSample);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_airbrush_gesture(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6StampGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (!state.stamp_source_valid || samples.empty() || !QueryDocument(state, document)
        || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStrokeSample source = state.stamp_source;
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), options, source, plane_id](InkpodCore* core) {
                   InkpodStampGestureInput input{};
                   input.struct_size = sizeof(input);
                   input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
                   input.feature_flags = options.option2 ? INKPOD_EFFECT_FLAG_PRESSURE_SIZE : 0U;
                   if (options.option) {
                       input.feature_flags |= INKPOD_EFFECT_FLAG_PRESSURE_OPACITY;
                   }
                   input.plane_id = plane_id;
                   input.source = source;
                   input.radius_milli = static_cast<std::uint32_t>(options.parameters[0]);
                    input.hardness_milli = static_cast<std::uint32_t>(options.parameters[1]);
                    input.spacing_milli = static_cast<std::uint32_t>(options.parameters[2]);
                    input.opacity_milli = static_cast<std::uint32_t>(options.parameters[3]);
                    input.shape = options.mode;
                    input.samples = samples.data();
                   input.sample_count = samples.size();
                   input.sample_stride_bytes = sizeof(InkpodStrokeSample);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_stamp_gesture(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6BlurGesture(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (samples.empty() || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = document.color_plane_id;
    return state.engine->Enqueue(
               [samples = std::move(samples), options, plane_id](InkpodCore* core) {
                   InkpodBlurToolInput input{};
                   input.struct_size = sizeof(input);
                   input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
                   input.feature_flags = options.option ? INKPOD_EFFECT_FLAG_PRESSURE_SIZE : 0U;
                   input.plane_id = plane_id;
                   input.radius = static_cast<std::uint32_t>(options.parameters[0]);
                   input.strength_milli = static_cast<std::uint32_t>(options.parameters[1]);
                   input.shape = options.shape;
                   input.diameter = static_cast<float>(options.parameters[2]);
                   input.samples = samples.data();
                   input.sample_count = samples.size();
                   input.sample_stride_bytes = sizeof(InkpodStrokeSample);
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_effect_blur_tool(core, &input, &result);
               },
               true,
               true,
               true)
        ? INKPOD_STATUS_OK
        : INKPOD_STATUS_INVALID_STATE;
}

InkpodStatus QueueM6Dust(
    AppState& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    M6ToolOptions options{};
    try {
        options = state.m6_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = document.color_plane_id;
    const bool preview = options.option;
    return StartM6Task(
        state,
        preview,
        [samples = std::move(samples), options, plane_id, preview](
            InkpodCore* core, InkpodM6Task* task) {
            InkpodDustInput input{};
            input.struct_size = sizeof(input);
            input.mode = options.mode;
            input.plane_id = plane_id;
            input.coordinate_space = INKPOD_COORDINATE_SPACE_DEVICE;
            input.shape = options.shape;
            input.maximum_pixels = static_cast<std::uint32_t>(options.parameters[0]);
            input.use_region = options.shape == 0U ? 0U : 1U;
            input.diameter = static_cast<float>(options.parameters[1]);
            if (input.use_region != 0U) {
                input.samples = samples.data();
                input.sample_count = samples.size();
                input.sample_stride_bytes = sizeof(InkpodStrokeSample);
            }
            if (preview) {
                InkpodFilterPreviewInfo info{};
                info.struct_size = sizeof(info);
                return inkpod_core_dust_preview_begin(core, &input, task, &info);
            }
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_dust_remove(core, &input, task, &result);
        });
}

InkpodStatus FinishM6CanvasGesture(AppState& state) noexcept {
    std::vector<InkpodStrokeSample> samples;
    samples.swap(state.m6_samples);
    switch (state.tool) {
        case kInteractionM6Gradient:
            return QueueM6GradientGesture(state, std::move(samples), false);
        case kInteractionM6AlphaGradient:
            return QueueM6GradientGesture(state, std::move(samples), true);
        case kInteractionM6Airbrush:
            return QueueM6AirbrushGesture(state, std::move(samples));
        case kInteractionM6Blur:
            return QueueM6BlurGesture(state, std::move(samples));
        case kInteractionM6Stamp:
            return QueueM6StampGesture(state, std::move(samples));
        case kInteractionM6Dust:
            return QueueM6Dust(state, std::move(samples));
        default:
            return INKPOD_STATUS_INVALID_STATE;
    }
}

void UpdateMenuState(AppState& state) noexcept {
    HMENU menu = GetMenu(state.window);
    if (menu == nullptr) {
        return;
    }
    InkpodDocumentInfo info{};
    const bool has_document = QueryDocument(state, info);
    TreePaneNode active_plane{};
    const bool vector_stroke_plane = has_document
        && QueryTreeNode(state, true, active_plane)
        && IsVectorStrokePlane(active_plane.kind);
    if (!vector_stroke_plane && IsVectorCanvasTool(state.tool)) {
        ClearVectorGeometryPreview(state);
        state.tool = INKPOD_TOOL_PENCIL;
    }
    EnableMenuItem(
        menu,
        IDM_FILE_SAVE,
        MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_FILE_SAVE_AS,
        MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_FILE_REVERT,
        MF_BYCOMMAND
            | (has_document && !state.current_path.empty() ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_FILE_REVERT_PARTIAL,
        MF_BYCOMMAND
            | (has_document && !state.current_path.empty() ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_FILE_AUTOSAVE_NOW,
        MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_EDIT_UNDO,
        MF_BYCOMMAND
            | (has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) != 0U
                   ? MF_ENABLED
                   : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_EDIT_REDO,
        MF_BYCOMMAND
            | (has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_REDO) != 0U
                   ? MF_ENABLED
                   : MF_GRAYED));
    InkpodHistoryInfo history_info{};
    std::wstring undo_label;
    std::wstring redo_label;
    const bool has_history = has_document
        && QueryHistoryMenuLabels(
            state, history_info, undo_label, redo_label);
    EnableMenuItem(
        menu,
        IDM_EDIT_HISTORY_BACK,
        MF_BYCOMMAND
            | (has_history && history_info.cursor > 0U ? MF_ENABLED : MF_GRAYED));
    EnableMenuItem(
        menu,
        IDM_EDIT_HISTORY_FORWARD,
        MF_BYCOMMAND
            | (has_history && history_info.cursor < history_info.item_count ? MF_ENABLED
                                                                            : MF_GRAYED));
    if (has_history) {
        ModifyMenuW(
            menu,
            IDM_EDIT_UNDO,
            MF_BYCOMMAND | MF_STRING,
            IDM_EDIT_UNDO,
            undo_label.c_str());
        ModifyMenuW(
            menu,
            IDM_EDIT_REDO,
            MF_BYCOMMAND | MF_STRING,
            IDM_EDIT_REDO,
            redo_label.c_str());
    }
    for (const UINT command : {
             IDM_EDIT_COPY,
             IDM_EDIT_MIRROR_HORIZONTAL,
             IDM_LAYER_DUPLICATE,
             IDM_LAYER_MOVE_TOP,
             IDM_SELECTION_ALL,
             IDM_SELECTION_CLEAR,
             IDM_SELECTION_RECTANGLE,
             IDM_SELECTION_ELLIPSE,
             IDM_SELECTION_LASSO,
             IDM_SELECTION_POLYLINE,
             IDM_SELECTION_TRACE,
             IDM_SELECTION_WAND,
             IDM_SELECTION_MODE_NEW,
             IDM_SELECTION_MODE_ADD,
             IDM_SELECTION_MODE_SUBTRACT,
             IDM_SELECTION_MODE_INTERSECT,
             IDM_SELECTION_COLOR,
             IDM_SELECTION_COLOR_DIFFERENT,
             IDM_SELECTION_COLOR_ADD,
             IDM_SELECTION_TO_LAYER,
             IDM_SELECTION_INVERT,
             IDM_SELECTION_EXPAND,
             IDM_SELECTION_SHRINK,
             IDM_VIEW_FLIP_HORIZONTAL,
             IDM_VIEW_FLIP_VERTICAL,
             IDM_VIEW_ZOOM_PERCENT,
             IDM_VIEW_BOX_ZOOM,
             IDM_VIEW_RULER,
             IDM_VIEW_GUIDES,
             IDM_VIEW_GRID,
             IDM_VIEW_SNAP_GUIDES,
             IDM_VIEW_SNAP_GRID,
             IDM_VIEW_TRANSPARENT,
             IDM_VIEW_GUIDE_VERTICAL,
             IDM_VIEW_GUIDE_HORIZONTAL,
             IDM_VIEW_GUIDE_MOVE,
             IDM_VIEW_GUIDE_DELETE_ALL,
             IDM_VIEW_GRID_SETTINGS,
             IDM_VIEW_NEW}) {
        EnableMenuItem(
            menu, command, MF_BYCOMMAND | (has_document ? MF_ENABLED : MF_GRAYED));
    }
    for (const UINT command : {
             IDM_FILTER_LAST,
             IDM_FILTER_INVERT,
             IDM_FILTER_BLUR_WEAK,
             IDM_FILTER_SHARPEN_WEAK,
             IDM_FILTER_SHARPEN_STRONG,
             IDM_FILTER_BLUR_STRONG,
             IDM_FILTER_GAUSSIAN,
             IDM_FILTER_AUTO_CONTRAST,
             IDM_FILTER_BRIGHTNESS,
             IDM_FILTER_TONE_CURVE,
             IDM_FILTER_LEVELS,
             IDM_FILTER_HSV,
             IDM_FILTER_COLOR_BALANCE,
             IDM_FILTER_UNSHARP,
             IDM_EFFECT_GRADIENT,
             IDM_EFFECT_AIRBRUSH,
             IDM_EFFECT_BOUNDARY_AIRBRUSH,
             IDM_EFFECT_BLUR,
             IDM_EFFECT_STAMP,
             IDM_EFFECT_DUST,
             IDM_EFFECT_ALPHA_GRADIENT,
             IDM_EFFECT_ALPHA_VIEW}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND
                | (has_document && state.plane == INKPOD_PLANE_COLOR ? MF_ENABLED : MF_GRAYED));
    }
    EnableMenuItem(
        menu,
        IDM_ADJUSTMENT_CREATE,
        MF_BYCOMMAND
            | (has_document && state.plane == INKPOD_PLANE_COLOR
                   ? MF_ENABLED
                   : MF_GRAYED));
    for (const UINT command : {
             IDM_ADJUSTMENT_EDIT,
             IDM_ADJUSTMENT_TOGGLE,
             IDM_ADJUSTMENT_MOVE_TOP}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND | (has_document && state.m6_adjustment_id != 0U ? MF_ENABLED
                                                                          : MF_GRAYED));
    }
    for (const UINT command : {IDM_ADJUSTMENT_PREVIOUS, IDM_ADJUSTMENT_NEXT}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND
                | (has_document && state.m6_adjustments.size() > 1U ? MF_ENABLED : MF_GRAYED));
    }
    CheckMenuItem(
        menu,
        IDM_ADJUSTMENT_TOGGLE,
        MF_BYCOMMAND | (state.m6_adjustment_visible ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_EFFECT_ALPHA_VIEW,
        MF_BYCOMMAND | (state.alpha_view ? MF_CHECKED : MF_UNCHECKED));
    const bool paste_available = state.clipboard != nullptr
        || (InkpodClipboardFormat() != 0U
            && IsClipboardFormatAvailable(InkpodClipboardFormat()) != FALSE);
    for (const UINT command : {
             IDM_EDIT_PASTE, IDM_EDIT_PASTE_SELECTED, IDM_EDIT_PASTE_CONVERTED}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND | (has_document && paste_available && !state.floating_active
                                  ? MF_ENABLED
                                  : MF_GRAYED));
    }
    for (const UINT command : {
             IDM_EDIT_FLOATING_TRANSFORM,
             IDM_EDIT_FLOATING_COMMIT,
             IDM_EDIT_FLOATING_CANCEL}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND | (state.floating_active ? MF_ENABLED : MF_GRAYED));
    }
    EnableMenuItem(
        menu,
        IDM_LAYER_DELETE,
        MF_BYCOMMAND
            | (has_document && state.m3_layer_id != 0U ? MF_ENABLED : MF_GRAYED));
    for (const UINT command : {
             IDM_SELECTION_FROM_LAYER,
             IDM_SELECTION_LAYER_ADD,
             IDM_SELECTION_LAYER_SUBTRACT}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND
                | (has_document && state.selection_layer_id != 0U ? MF_ENABLED
                                                                   : MF_GRAYED));
    }
    CheckMenuItem(
        menu,
        IDM_VIEW_FLIP_HORIZONTAL,
        MF_BYCOMMAND | (state.view_flip_horizontal ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_FLIP_VERTICAL,
        MF_BYCOMMAND | (state.view_flip_vertical ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_GRID,
        MF_BYCOMMAND | (state.grid_visible ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_RULER,
        MF_BYCOMMAND | (state.ruler_visible ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_GUIDES,
        MF_BYCOMMAND | (state.guides_visible ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_SNAP_GUIDES,
        MF_BYCOMMAND | (state.snap_guides ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_SNAP_GRID,
        MF_BYCOMMAND | (state.snap_grid ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_TRANSPARENT,
        MF_BYCOMMAND | (state.transparent_visible ? MF_CHECKED : MF_UNCHECKED));
    CheckMenuItem(
        menu,
        IDM_VIEW_BOX_ZOOM,
        MF_BYCOMMAND | (state.tool == kInteractionBoxZoom ? MF_CHECKED : MF_UNCHECKED));
    for (const UINT command : {
             IDM_SELECTION_RECTANGLE,
             IDM_SELECTION_ELLIPSE,
             IDM_SELECTION_LASSO,
             IDM_SELECTION_POLYLINE,
             IDM_SELECTION_TRACE,
             IDM_SELECTION_WAND,
             IDM_SELECTION_MODE_NEW,
             IDM_SELECTION_MODE_ADD,
             IDM_SELECTION_MODE_SUBTRACT,
             IDM_SELECTION_MODE_INTERSECT}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT selection_shape_command = state.selection_shape == INKPOD_SELECTION_ELLIPSE
        ? IDM_SELECTION_ELLIPSE
        : (state.selection_shape == INKPOD_SELECTION_LASSO
                  ? IDM_SELECTION_LASSO
                  : (state.selection_shape == INKPOD_SELECTION_POLYLINE
                            ? IDM_SELECTION_POLYLINE
                            : (state.selection_shape == INKPOD_SELECTION_TRACE
                                      ? IDM_SELECTION_TRACE
                                      : (state.selection_shape == INKPOD_SELECTION_WAND
                                                ? IDM_SELECTION_WAND
                                                : IDM_SELECTION_RECTANGLE))));
    const UINT selection_mode_command = state.selection_operation == INKPOD_SELECTION_ADD
        ? IDM_SELECTION_MODE_ADD
        : (state.selection_operation == INKPOD_SELECTION_SUBTRACT
                  ? IDM_SELECTION_MODE_SUBTRACT
                  : (state.selection_operation == INKPOD_SELECTION_INTERSECT
                            ? IDM_SELECTION_MODE_INTERSECT
                            : IDM_SELECTION_MODE_NEW));
    CheckMenuItem(
        menu, selection_shape_command, MF_BYCOMMAND | MF_CHECKED);
    CheckMenuItem(
        menu, selection_mode_command, MF_BYCOMMAND | MF_CHECKED);
    for (const UINT command : {
             IDM_TOOL_PENCIL, IDM_TOOL_BRUSH, IDM_TOOL_ERASER, IDM_TOOL_FILL,
             IDM_TOOL_CLOSED_FILL, IDM_TOOL_FILL_EXTENSION, IDM_TOOL_EYEDROPPER,
             IDM_PLANE_MAIN_LINE, IDM_PLANE_COLOR}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT tool_command = state.tool == INKPOD_TOOL_PENCIL
        ? IDM_TOOL_PENCIL
        : (state.tool == INKPOD_TOOL_BRUSH
                  ? IDM_TOOL_BRUSH
                  : (state.tool == INKPOD_TOOL_ERASER
                            ? IDM_TOOL_ERASER
                            : (state.tool == kInteractionFill
                                      ? (state.fill_options.operation == INKPOD_FILL_CLOSED_REGION
                                                ? IDM_TOOL_CLOSED_FILL
                                                : (state.fill_options.operation
                                                            == INKPOD_FILL_EXTENSION
                                                      ? IDM_TOOL_FILL_EXTENSION
                                                      : IDM_TOOL_FILL))
                                      : IDM_TOOL_EYEDROPPER)));
    if (state.tool != kInteractionBoxZoom && state.tool != kInteractionGuideMove
        && state.tool != kInteractionSelection && state.tool != kInteractionFloatingTransform
        && state.tool != kInteractionLightTableMove
        && !IsVectorCanvasTool(state.tool)
        && !(state.tool >= kInteractionM6Gradient
            && state.tool <= kInteractionM6AlphaGradient)) {
        CheckMenuItem(menu, tool_command, MF_BYCOMMAND | MF_CHECKED);
    }
    for (const UINT command : {
             IDM_VECTOR_LINE, IDM_VECTOR_CURVE, IDM_VECTOR_RECTANGLE,
             IDM_VECTOR_ELLIPSE, IDM_VECTOR_POLYLINE, IDM_VECTOR_ERASER}) {
        EnableMenuItem(
            menu,
            command,
            MF_BYCOMMAND | (vector_stroke_plane ? MF_ENABLED : MF_GRAYED));
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT vector_tool_command = state.tool == kInteractionVectorLine
        ? IDM_VECTOR_LINE
        : (state.tool == kInteractionVectorCurve
                  ? IDM_VECTOR_CURVE
                  : (state.tool == kInteractionVectorRectangle
                            ? IDM_VECTOR_RECTANGLE
                            : (state.tool == kInteractionVectorEllipse
                                      ? IDM_VECTOR_ELLIPSE
                                      : (state.tool == kInteractionVectorPolyline
                                                ? IDM_VECTOR_POLYLINE
                                                : IDM_VECTOR_ERASER))));
    if (IsVectorCanvasTool(state.tool)) {
        CheckMenuItem(menu, vector_tool_command, MF_BYCOMMAND | MF_CHECKED);
    }
    for (const UINT command : {
             IDM_VECTOR_ERASE_PARTIAL,
             IDM_VECTOR_ERASE_INTERSECTION,
             IDM_VECTOR_ERASE_WHOLE}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    CheckMenuItem(
        menu,
        state.vector_erase_mode == INKPOD_VECTOR_ERASE_TO_INTERSECTION
            ? IDM_VECTOR_ERASE_INTERSECTION
            : (state.vector_erase_mode == INKPOD_VECTOR_ERASE_WHOLE_PATH
                      ? IDM_VECTOR_ERASE_WHOLE
                      : IDM_VECTOR_ERASE_PARTIAL),
        MF_BYCOMMAND | MF_CHECKED);
    CheckMenuItem(
        menu,
        state.plane == INKPOD_PLANE_MAIN_LINE ? IDM_PLANE_MAIN_LINE : IDM_PLANE_COLOR,
        MF_BYCOMMAND | MF_CHECKED);
    for (const UINT command : {
             IDM_COLOR_CHECK_OFF, IDM_COLOR_CHECK_LEGACY, IDM_COLOR_CHECK_NATIVE}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT check_command = state.color_check_mode == INKPOD_COLOR_CHECK_LEGACY_WHITE
        ? IDM_COLOR_CHECK_LEGACY
        : (state.color_check_mode == INKPOD_COLOR_CHECK_NATIVE_ALPHA
                  ? IDM_COLOR_CHECK_NATIVE
                  : IDM_COLOR_CHECK_OFF);
    CheckMenuItem(menu, check_command, MF_BYCOMMAND | MF_CHECKED);
    for (const UINT command : {
             IDM_COLOR_SOURCE_TOPMOST,
             IDM_COLOR_SOURCE_SELECTED,
             IDM_COLOR_SOURCE_COMPOSITE,
             IDM_COLOR_SOURCE_LIGHT_TABLE}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT source_command = state.eyedropper_source
            == INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT
        ? IDM_COLOR_SOURCE_TOPMOST
        : (state.eyedropper_source == INKPOD_EYEDROPPER_SELECTED_PLANE
                  ? IDM_COLOR_SOURCE_SELECTED
                  : (state.eyedropper_source == INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST
                            ? IDM_COLOR_SOURCE_LIGHT_TABLE
                            : IDM_COLOR_SOURCE_COMPOSITE));
    CheckMenuItem(menu, source_command, MF_BYCOMMAND | MF_CHECKED);
    CheckMenuItem(
        menu,
        IDM_CHART_LOCK,
        MF_BYCOMMAND | (state.color_chart_locked ? MF_CHECKED : MF_UNCHECKED));
    for (const UINT command : {
             IDM_MOTION_FPS_30, IDM_MOTION_FPS_25, IDM_MOTION_FPS_24,
             IDM_MOTION_FPS_12, IDM_MOTION_FPS_10, IDM_MOTION_FPS_8}) {
        CheckMenuItem(menu, command, MF_BYCOMMAND | MF_UNCHECKED);
    }
    const UINT fps_command = state.motion_fps == 30U
        ? IDM_MOTION_FPS_30
        : (state.motion_fps == 25U
                  ? IDM_MOTION_FPS_25
                  : (state.motion_fps == 24U
                            ? IDM_MOTION_FPS_24
                            : (state.motion_fps == 12U
                                      ? IDM_MOTION_FPS_12
                                      : (state.motion_fps == 10U
                                                ? IDM_MOTION_FPS_10
                                                : IDM_MOTION_FPS_8))));
    CheckMenuItem(menu, fps_command, MF_BYCOMMAND | MF_CHECKED);

    if (state.toolbar != nullptr) {
        const LPARAM enabled = MAKELPARAM(has_document ? TRUE : FALSE, 0);
        for (const UINT command : {
                 IDM_FILE_SAVE,
                 IDM_VIEW_FIT,
                 IDM_VIEW_ONE_TO_ONE,
                 IDM_VIEW_BOX_ZOOM,
                 IDM_VIEW_FLIP_HORIZONTAL,
                 IDM_VIEW_FLIP_VERTICAL,
                 IDM_VIEW_RULER,
                 IDM_VIEW_GUIDES,
                 IDM_VIEW_GRID,
                 IDM_VIEW_TRANSPARENT}) {
            SendMessageW(state.toolbar, TB_ENABLEBUTTON, command, enabled);
        }
        for (const auto& [command, checked] : std::array<std::pair<UINT, bool>, 7U>{
                 std::pair{IDM_VIEW_BOX_ZOOM, state.tool == kInteractionBoxZoom},
                 std::pair{IDM_VIEW_FLIP_HORIZONTAL, state.view_flip_horizontal},
                 std::pair{IDM_VIEW_FLIP_VERTICAL, state.view_flip_vertical},
                 std::pair{IDM_VIEW_RULER, state.ruler_visible},
                 std::pair{IDM_VIEW_GUIDES, state.guides_visible},
                 std::pair{IDM_VIEW_GRID, state.grid_visible},
                 std::pair{IDM_VIEW_TRANSPARENT, state.transparent_visible}}) {
            SendMessageW(
                state.toolbar,
                TB_CHECKBUTTON,
                command,
                MAKELPARAM(checked ? TRUE : FALSE, 0));
        }
    }

    std::array<wchar_t, 1024> title{};
    const wchar_t* name = state.current_path.empty()
        ? ((info.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U ? L"Recovery" : L"無題")
        : state.current_path.c_str();
    _snwprintf_s(
        title.data(),
        title.size(),
        _TRUNCATE,
        L"%ls%ls - inkpod",
        name,
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U ? L" *" : L"");
    SetWindowTextW(state.window, title.data());
    if (state.status_bar != nullptr) {
        InkpodSnapshotTransform transform{};
        const bool has_transform = has_document && QuerySnapshotTransform(state, transform);
        std::array<wchar_t, 96U> tool_text{};
        const wchar_t* tool_name = state.tool == kInteractionBoxZoom
            ? L"範囲を拡大"
            : (state.tool == kInteractionGuideMove
                      ? L"ガイド移動"
                      : (state.tool == kInteractionSelection
                                ? L"選択"
            : (state.tool == kInteractionFill
                      ? L"フィル"
                      : (state.tool == kInteractionEyedropper
                                ? L"スポイト"
                                : (state.tool == INKPOD_TOOL_ERASER
                                          ? L"消しゴム"
                                          : (state.tool == INKPOD_TOOL_BRUSH ? L"ブラシ"
                                                                              : L"鉛筆"))))));
        _snwprintf_s(
            tool_text.data(), tool_text.size(), _TRUNCATE, L"ツール: %ls", tool_name);
        std::array<wchar_t, 96U> zoom_text{};
        _snwprintf_s(
            zoom_text.data(),
            zoom_text.size(),
            _TRUNCATE,
            has_transform ? L"ズーム: %.1f%%" : L"ズーム: --",
            has_transform ? transform.zoom * 100.0 : 0.0);
        std::array<wchar_t, 96U> document_text{};
        _snwprintf_s(
            document_text.data(),
            document_text.size(),
            _TRUNCATE,
            has_document ? L"%u x %u / %ls" : L"文書なし",
            has_document ? info.width : 0U,
            has_document ? info.height : 0U,
            has_document && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U ? L"変更あり"
                                                                             : L"保存済み");
        SendMessageW(
            state.status_bar,
            SB_SETTEXTW,
            0,
            reinterpret_cast<LPARAM>(tool_text.data()));
        SendMessageW(
            state.status_bar,
            SB_SETTEXTW,
            1,
            reinterpret_cast<LPARAM>(zoom_text.data()));
        SendMessageW(
            state.status_bar,
            SB_SETTEXTW,
            2,
            reinterpret_cast<LPARAM>(document_text.data()));
        if (has_transform && state.zoom_slider != nullptr) {
            const auto slider_value = static_cast<LPARAM>(std::clamp(
                static_cast<int>(std::lround(transform.zoom * 100.0)), 1, 800));
            SendMessageW(state.zoom_slider, TBM_SETPOS, TRUE, slider_value);
        }
    }
    DrawMenuBar(state.window);
}

InkpodStatus ApplyView(
    AppState& state,
    InkpodViewCommandKind kind,
    double value1,
    double value2,
    double value3 = 0.0,
    double value4 = 0.0) noexcept {
    const InkpodViewInput input{
        sizeof(InkpodViewInput), kind, 0U, value1, value2, value3, value4};
    const std::uint64_t view_id = state.active_view_id;
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [input, view_id](InkpodCore* core) {
                  InkpodDocumentInfo info = EmptyDocumentInfo();
                  InkpodStatus status = view_id == 0U
                      ? inkpod_core_apply_view(core, &input, &info)
                      : inkpod_core_view_apply(core, view_id, &input);
                  if (status == INKPOD_STATUS_OK && view_id != 0U) {
                      status = inkpod_core_get_document_info(core, &info);
                  }
                  return status;
              },
              true,
              true);
}

bool QuerySnapshotTransform(
    AppState& state, InkpodSnapshotTransform& transform) noexcept {
    transform = {};
    transform.struct_size = sizeof(transform);
    if (state.engine == nullptr) {
        return false;
    }
    const std::uint64_t view_id = state.active_view_id;
    return state.engine->Invoke(
               [&transform, view_id](InkpodCore* core) {
                   const InkpodSnapshotOptions options{
                       sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
                   InkpodSnapshot* snapshot{};
                   InkpodStatus status = view_id == 0U
                       ? inkpod_core_build_snapshot(core, &options, &snapshot)
                       : inkpod_core_build_snapshot_for_view(
                             core, view_id, &options, &snapshot);
                   if (status == INKPOD_STATUS_OK) {
                       status = inkpod_snapshot_get_transform(snapshot, &transform);
                   }
                   const InkpodStatus release_status =
                       inkpod_snapshot_release(&snapshot);
                   return status == INKPOD_STATUS_OK ? release_status : status;
               },
               false,
               false) == INKPOD_STATUS_OK;
}

InkpodStatus ApplyZoomPercent(AppState& state, std::uint32_t percent) noexcept {
    if (percent == 0U || percent > 6400U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodSnapshotTransform transform{};
    RECT client{};
    if (!QuerySnapshotTransform(state, transform)
        || GetClientRect(state.canvas, &client) == FALSE
        || !std::isfinite(transform.zoom) || transform.zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double target = static_cast<double>(percent) / 100.0;
    return ApplyView(
        state,
        INKPOD_VIEW_ZOOM_AT,
        target / transform.zoom,
        static_cast<double>(client.right - client.left) / 2.0,
        static_cast<double>(client.bottom - client.top) / 2.0);
}

InkpodStatus ApplyBoxZoomGesture(
    AppState& state,
    const InkpodStrokeSample& start,
    const InkpodStrokeSample& end) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto document_x = [&](double value) {
        double result = (value - bounds.left) / zoom;
        if (state.view_flip_horizontal) {
            result = static_cast<double>(info.width) - result;
        }
        return std::clamp(result, 0.0, static_cast<double>(info.width));
    };
    auto document_y = [&](double value) {
        double result = (value - bounds.top) / zoom;
        if (state.view_flip_vertical) {
            result = static_cast<double>(info.height) - result;
        }
        return std::clamp(result, 0.0, static_cast<double>(info.height));
    };
    const double x1 = document_x(start.x);
    const double y1 = document_y(start.y);
    const double x2 = document_x(end.x);
    const double y2 = document_y(end.y);
    const auto left = static_cast<std::int32_t>(std::floor(std::min(x1, x2)));
    const auto top = static_cast<std::int32_t>(std::floor(std::min(y1, y2)));
    const auto right = static_cast<std::int32_t>(std::ceil(std::max(x1, x2)));
    const auto bottom = static_cast<std::int32_t>(std::ceil(std::max(y1, y2)));
    if (right <= left || bottom <= top) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    return ApplyView(
        state,
        INKPOD_VIEW_BOX_ZOOM,
        static_cast<double>(left),
        static_cast<double>(top),
        static_cast<double>(right - left),
        static_cast<double>(bottom - top));
}

InkpodStatus AddGuide(
    AppState& state, std::uint32_t axis, std::int32_t position) noexcept {
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [axis, position](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  std::uint64_t guide_id{};
                  return inkpod_core_guide_add(
                      core, axis, position, &result, &guide_id);
              },
              true,
              true);
}

InkpodStatus DeleteAllGuides(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return state.engine->Invoke(
        [](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &snapshot);
            InkpodSnapshotOverlay overlay{};
            overlay.struct_size = sizeof(overlay);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_overlay(snapshot, &overlay);
            }
            std::vector<std::uint64_t> guide_ids;
            if (status == INKPOD_STATUS_OK) {
                try {
                    guide_ids.reserve(static_cast<std::size_t>(overlay.guide_count));
                    const auto* bytes = reinterpret_cast<const std::uint8_t*>(overlay.guides);
                    for (std::uint64_t index = 0; index < overlay.guide_count; ++index) {
                        const auto* guide = reinterpret_cast<const InkpodSnapshotGuide*>(
                            bytes + static_cast<std::size_t>(
                                        index * overlay.guide_stride_bytes));
                        guide_ids.push_back(guide->id);
                    }
                } catch (const std::bad_alloc&) {
                    status = INKPOD_STATUS_INVALID_STATE;
                }
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            if (release_status != INKPOD_STATUS_OK) {
                return release_status;
            }
            for (const std::uint64_t guide_id : guide_ids) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_guide_delete(core, guide_id, &result);
                if (status != INKPOD_STATUS_OK) {
                    return status;
                }
            }
            return INKPOD_STATUS_OK;
        },
        true,
        true);
}

InkpodStatus SetGrid(AppState& state, const InkpodGridInput& input) noexcept {
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [input](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_grid_set(core, &input, &result);
              },
              true,
              true);
}

bool BeginGuideDrag(
    AppState& state, const InkpodStrokeSample& sample) noexcept {
    constexpr float ruler_extent = 22.0F;
    if (state.tool != kInteractionGuideMove && state.ruler_visible
        && sample.y >= 0.0F && sample.y <= ruler_extent
        && sample.x > ruler_extent) {
        state.guide_drag_active = true;
        state.guide_drag_axis = INKPOD_GUIDE_VERTICAL;
        state.guide_drag_id = 0U;
        return true;
    }
    if (state.tool != kInteractionGuideMove && state.ruler_visible
        && sample.x >= 0.0F && sample.x <= ruler_extent
        && sample.y > ruler_extent) {
        state.guide_drag_active = true;
        state.guide_drag_axis = INKPOD_GUIDE_HORIZONTAL;
        state.guide_drag_id = 0U;
        return true;
    }
    if (state.tool != kInteractionGuideMove || state.engine == nullptr) {
        return false;
    }
    std::uint64_t nearest_id{};
    std::uint32_t nearest_axis{};
    double nearest_distance = 7.0;
    const std::uint64_t view_id = state.active_view_id;
    const InkpodStatus status = state.engine->Invoke(
        [view_id, &sample, &nearest_id, &nearest_axis, &nearest_distance](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus inner = view_id == 0U
                ? inkpod_core_build_snapshot(core, &options, &snapshot)
                : inkpod_core_build_snapshot_for_view(core, view_id, &options, &snapshot);
            InkpodSnapshotOverlay overlay{};
            overlay.struct_size = sizeof(overlay);
            InkpodSnapshotTransform transform{};
            transform.struct_size = sizeof(transform);
            if (inner == INKPOD_STATUS_OK) {
                inner = inkpod_snapshot_get_overlay(snapshot, &overlay);
            }
            if (inner == INKPOD_STATUS_OK) {
                inner = inkpod_snapshot_get_transform(snapshot, &transform);
            }
            if (inner == INKPOD_STATUS_OK) {
                const auto* bytes = reinterpret_cast<const std::uint8_t*>(overlay.guides);
                for (std::uint64_t index = 0; index < overlay.guide_count; ++index) {
                    const auto* guide = reinterpret_cast<const InkpodSnapshotGuide*>(
                        bytes + static_cast<std::size_t>(
                                    index * overlay.guide_stride_bytes));
                    double coordinate{};
                    double pointer{};
                    if (guide->axis == INKPOD_GUIDE_VERTICAL) {
                        const bool flipped = (transform.flags
                            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) != 0U;
                        const double source = flipped
                            ? static_cast<double>(transform.document_width) - guide->position
                            : guide->position;
                        coordinate = transform.pan_x + source * transform.zoom;
                        pointer = sample.x;
                    } else {
                        const bool flipped = (transform.flags
                            & INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL) != 0U;
                        const double source = flipped
                            ? static_cast<double>(transform.document_height) - guide->position
                            : guide->position;
                        coordinate = transform.pan_y + source * transform.zoom;
                        pointer = sample.y;
                    }
                    const double distance = std::abs(coordinate - pointer);
                    if (distance < nearest_distance) {
                        nearest_distance = distance;
                        nearest_id = guide->id;
                        nearest_axis = guide->axis;
                    }
                }
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return inner == INKPOD_STATUS_OK ? release_status : inner;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK || nearest_id == 0U) {
        return false;
    }
    state.guide_drag_active = true;
    state.guide_drag_axis = nearest_axis;
    state.guide_drag_id = nearest_id;
    return true;
}

InkpodStatus FinishGuideDrag(
    AppState& state, const InkpodStrokeSample& sample) noexcept {
    if (!state.guide_drag_active) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint32_t axis = state.guide_drag_axis;
    const std::uint64_t guide_id = state.guide_drag_id;
    state.guide_drag_active = false;
    state.guide_drag_axis = 0U;
    state.guide_drag_id = 0U;
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    RECT client{};
    if (!QueryDocument(state, info)
        || GetClientRect(state.canvas, &client) == FALSE
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const bool outside_canvas = sample.x < 0.0F || sample.y < 0.0F
        || sample.x >= static_cast<float>(client.right)
        || sample.y >= static_cast<float>(client.bottom);
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    double document_position = axis == INKPOD_GUIDE_VERTICAL
        ? (static_cast<double>(sample.x) - bounds.left) / zoom
        : (static_cast<double>(sample.y) - bounds.top) / zoom;
    if ((axis == INKPOD_GUIDE_VERTICAL && state.view_flip_horizontal)
        || (axis == INKPOD_GUIDE_HORIZONTAL && state.view_flip_vertical)) {
        document_position = static_cast<double>(
            axis == INKPOD_GUIDE_VERTICAL ? info.width : info.height)
            - document_position;
    }
    const double maximum = static_cast<double>(
        axis == INKPOD_GUIDE_VERTICAL ? info.width : info.height);
    const bool outside_document = !std::isfinite(document_position)
        || document_position < 0.0 || document_position > maximum;
    if (outside_canvas || outside_document) {
        if (guide_id == 0U) {
            return INKPOD_STATUS_OK;
        }
        return state.engine == nullptr
            ? INKPOD_STATUS_INVALID_STATE
            : state.engine->Invoke(
                  [guide_id](InkpodCore* core) {
                      InkpodDispatchResult result{};
                      result.struct_size = sizeof(result);
                      return inkpod_core_guide_delete(core, guide_id, &result);
                  },
                  true,
                  true);
    }
    const auto position = static_cast<std::int32_t>(std::lround(document_position));
    if (guide_id == 0U) {
        return AddGuide(state, axis, position);
    }
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [guide_id, position](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_guide_move(core, guide_id, position, &result);
              },
              true,
              true);
}

InkpodStatus FitCanvas(AppState& state, InkpodViewCommandKind kind) noexcept {
    RECT client{};
    GetClientRect(state.canvas, &client);
    return ApplyView(
        state,
        kind,
        static_cast<double>(client.right - client.left),
        static_cast<double>(client.bottom - client.top));
}

InkpodStatus ApplyTreeEdit(
    AppState& state,
    InkpodTreeOperation operation,
    std::uint64_t object_id,
    std::uint32_t destination_index,
    std::uint64_t& out_object_id) noexcept {
    InkpodTreeEdit edit{};
    edit.struct_size = sizeof(edit);
    edit.operation = operation;
    edit.object_id = object_id;
    edit.destination_index = destination_index;
    out_object_id = 0U;
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [edit, &out_object_id](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_tree_edit(
                      core, &edit, &result, &out_object_id);
              },
              true,
              true);
}

bool QueryTreeNode(AppState& state, bool plane, TreePaneNode& output) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    const std::uint32_t layer_index = state.active_tree_layer_index;
    const std::uint32_t plane_index = plane ? state.active_tree_plane_index : UINT32_MAX;
    return state.engine->Invoke(
               [layer_index, plane_index, &output](InkpodCore* core) {
                   std::array<std::uint8_t, 256U> name{};
                   InkpodNodeInfo info{};
                   info.struct_size = sizeof(info);
                   info.name_utf8 = name.data();
                   info.name_capacity = name.size();
                   const InkpodStatus status = inkpod_core_node_get(
                       core, layer_index, plane_index, &info);
                   if (status != INKPOD_STATUS_OK) {
                       return status;
                   }
                   try {
                       output = TreePaneNode{
                           info.id,
                           info.parent_id,
                           info.index,
                           info.kind,
                           info.pixel_format,
                           info.opacity_milli,
                           info.child_count,
                           info.flags,
                           std::string(
                               reinterpret_cast<const char*>(name.data()),
                               static_cast<std::size_t>(info.name_bytes))};
                   } catch (const std::bad_alloc&) {
                       return INKPOD_STATUS_INVALID_STATE;
                   }
                   return INKPOD_STATUS_OK;
               },
               false,
               false) == INKPOD_STATUS_OK;
}

InkpodStatus ApplyTreeEditRecord(
    AppState& state, InkpodTreeEdit edit, const std::string& name, std::uint64_t& object_id) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return state.engine->Invoke(
        [edit, name, &object_id](InkpodCore* core) mutable {
            edit.name_utf8 = name.empty()
                ? nullptr
                : reinterpret_cast<const std::uint8_t*>(name.data());
            edit.name_bytes = name.size();
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_tree_edit(core, &edit, &result, &object_id);
        },
        true,
        true);
}

InkpodStatus SetSelectedTreeNodeProperties(
    AppState& state, bool plane, UINT command) noexcept {
    TreePaneNode node{};
    if (!QueryTreeNode(state, plane, node)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodTreeEdit edit{};
    edit.struct_size = sizeof(edit);
    edit.operation = plane ? INKPOD_TREE_SET_PLANE_PROPERTIES
                           : INKPOD_TREE_SET_LAYER_PROPERTIES;
    edit.object_id = node.id;
    edit.flags = node.flags;
    edit.opacity_milli = node.opacity_milli;
    if (command
        == static_cast<UINT>(
            plane ? IDM_PLANE_TOGGLE_VISIBLE : IDM_LAYER_TOGGLE_VISIBLE)) {
        edit.flags ^= INKPOD_NODE_VISIBLE;
    } else if (command
        == static_cast<UINT>(
            plane ? IDM_PLANE_TOGGLE_EDITABLE : IDM_LAYER_TOGGLE_EDITABLE)) {
        edit.flags ^= INKPOD_NODE_EDITABLE;
    } else {
        ViewOptionsDialogState dialog{};
        dialog.title = plane ? L"プレーンの不透明度" : L"レイヤーの不透明度";
        dialog.labels[0] = L"不透明度 (0-100%)";
        dialog.values[0] = state.smoke_test
            ? 75
            : static_cast<std::int32_t>(node.opacity_milli / 10U);
        if (ShowViewOptions(
                state.instance, state.window, state.smoke_test, dialog) != IDOK
            || dialog.values[0] < 0 || dialog.values[0] > 100) {
            return INKPOD_STATUS_CANCELLED;
        }
        edit.opacity_milli = static_cast<std::uint32_t>(dialog.values[0]) * 10U;
    }
    std::uint64_t ignored{};
    return ApplyTreeEditRecord(state, edit, node.name, ignored);
}

InkpodStatus EditSelectedTreeNodeProperties(AppState& state, bool plane) noexcept {
    TreePaneNode node{};
    if (!QueryTreeNode(state, plane, node)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    TextInputDialogState name_dialog{};
    name_dialog.title = plane ? L"プレーンプロパティ" : L"レイヤープロパティ";
    name_dialog.label = L"名前";
    try {
        name_dialog.value = LocalizedHistoryLabel(node.name);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.smoke_test) {
        name_dialog.value = plane ? L"Smoke Plane" : L"Smoke Layer";
    }
    if (ShowTextInput(
            state.instance, state.window, state.smoke_test, name_dialog) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState options{};
    options.title = name_dialog.title;
    options.labels = {L"不透明度 (0-100%)", L"表示 (0/1)", L"編集可 (0/1)", L"種類 (参照)"};
    options.values = {
        static_cast<std::int32_t>(node.opacity_milli / 10U),
        (node.flags & INKPOD_NODE_VISIBLE) != 0U ? 1 : 0,
        (node.flags & INKPOD_NODE_EDITABLE) != 0U ? 1 : 0,
        static_cast<std::int32_t>(node.kind)};
    options.value_count = 4U;
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, options) != IDOK
        || options.values[0] < 0 || options.values[0] > 100) {
        return INKPOD_STATUS_CANCELLED;
    }
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(name_dialog.value, utf8) || utf8.empty()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodTreeEdit edit{};
    edit.struct_size = sizeof(edit);
    edit.operation = plane ? INKPOD_TREE_SET_PLANE_PROPERTIES
                           : INKPOD_TREE_SET_LAYER_PROPERTIES;
    edit.object_id = node.id;
    edit.flags = (options.values[1] != 0 ? INKPOD_NODE_VISIBLE : 0U)
        | (options.values[2] != 0 ? INKPOD_NODE_EDITABLE : 0U);
    edit.opacity_milli = static_cast<std::uint32_t>(options.values[0]) * 10U;
    std::uint64_t ignored{};
    try {
        const std::string name(
            reinterpret_cast<const char*>(utf8.data()), utf8.size());
        return ApplyTreeEditRecord(state, edit, name, ignored);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
}

bool IsVectorCanvasTool(std::uint32_t tool) noexcept {
    return tool >= kInteractionVectorLine && tool <= kInteractionVectorEraser;
}

bool IsVectorStrokePlane(std::uint32_t kind) noexcept {
    return kind == INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE
        || kind == INKPOD_TYPED_PLANE_COLOR_TRACE;
}

void ClearVectorGeometryPreview(AppState& state) noexcept {
    state.vector_gesture_samples.clear();
    if (state.canvas == nullptr) {
        return;
    }
    inkpod::renderer::CanvasGeometryPreview preview{};
    preview.struct_size = sizeof(preview);
    SendMessageW(
        state.canvas,
        inkpod::renderer::kCanvasSetGeometryPreview,
        0,
        reinterpret_cast<LPARAM>(&preview));
}

bool VectorGestureDocumentPoints(
    AppState& state,
    const std::vector<InkpodStrokeSample>& samples,
    std::vector<InkpodVectorPoint>& points) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (samples.empty() || !QueryDocument(state, info) || info.width == 0U || info.height == 0U
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1) {
        return false;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return false;
    }
    const std::size_t stride = std::max<std::size_t>(
        1U, (samples.size() + inkpod::renderer::kCanvasGeometryPreviewPoints - 1U)
            / inkpod::renderer::kCanvasGeometryPreviewPoints);
    try {
        points.clear();
        points.reserve(std::min<std::size_t>(
            samples.size() + 1U, inkpod::renderer::kCanvasGeometryPreviewPoints));
        for (std::size_t index = 0U; index < samples.size(); index += stride) {
            double x = (static_cast<double>(samples[index].x) - bounds.left) / zoom;
            double y = (static_cast<double>(samples[index].y) - bounds.top) / zoom;
            if (state.view_flip_horizontal) {
                x = static_cast<double>(info.width) - x;
            }
            if (state.view_flip_vertical) {
                y = static_cast<double>(info.height) - y;
            }
            if (!std::isfinite(x) || !std::isfinite(y)) {
                return false;
            }
            points.push_back(InkpodVectorPoint{
                static_cast<float>(std::clamp(x, 0.0, static_cast<double>(info.width))),
                static_cast<float>(std::clamp(y, 0.0, static_cast<double>(info.height)))});
        }
        const auto& final_sample = samples.back();
        double final_x = (static_cast<double>(final_sample.x) - bounds.left) / zoom;
        double final_y = (static_cast<double>(final_sample.y) - bounds.top) / zoom;
        if (state.view_flip_horizontal) {
            final_x = static_cast<double>(info.width) - final_x;
        }
        if (state.view_flip_vertical) {
            final_y = static_cast<double>(info.height) - final_y;
        }
        const InkpodVectorPoint final_point{
            static_cast<float>(std::clamp(final_x, 0.0, static_cast<double>(info.width))),
            static_cast<float>(std::clamp(final_y, 0.0, static_cast<double>(info.height)))};
        if (points.empty() || points.back().x != final_point.x || points.back().y != final_point.y) {
            if (points.size() == inkpod::renderer::kCanvasGeometryPreviewPoints) {
                points.back() = final_point;
            } else {
                points.push_back(final_point);
            }
        }
        return !points.empty();
    } catch (const std::bad_alloc&) {
        points.clear();
        return false;
    }
}

InkpodVectorCubicSegment VectorLineSegment(
    InkpodVectorPoint start, InkpodVectorPoint end, float width) noexcept {
    return InkpodVectorCubicSegment{
        sizeof(InkpodVectorCubicSegment),
        0U,
        start,
        InkpodVectorPoint{
            (start.x * 2.0F + end.x) / 3.0F,
            (start.y * 2.0F + end.y) / 3.0F},
        InkpodVectorPoint{
            (start.x + end.x * 2.0F) / 3.0F,
            (start.y + end.y * 2.0F) / 3.0F},
        end,
        width,
        width};
}

bool BuildVectorGestureSegments(
    const AppState& state,
    const std::vector<InkpodVectorPoint>& points,
    std::vector<InkpodVectorCubicSegment>& segments,
    bool& closed) noexcept {
    if (points.size() < 2U) {
        return false;
    }
    const float width = std::clamp(state.diameter, 0.001F, 4096.0F);
    closed = state.tool == kInteractionVectorRectangle
        || state.tool == kInteractionVectorEllipse;
    try {
        segments.clear();
        if (state.tool == kInteractionVectorLine) {
            segments.push_back(VectorLineSegment(points.front(), points.back(), width));
        } else if (state.tool == kInteractionVectorCurve) {
            const InkpodVectorPoint start = points.front();
            const InkpodVectorPoint end = points.back();
            const InkpodVectorPoint control1 = points[points.size() / 3U];
            const InkpodVectorPoint control2 = points[(points.size() * 2U) / 3U];
            segments.push_back(InkpodVectorCubicSegment{
                sizeof(InkpodVectorCubicSegment),
                0U,
                start,
                control1,
                control2,
                end,
                width,
                width});
        } else if (state.tool == kInteractionVectorRectangle) {
            const InkpodVectorPoint start = points.front();
            const InkpodVectorPoint end = points.back();
            const std::array<InkpodVectorPoint, 4U> corners{
                start,
                InkpodVectorPoint{end.x, start.y},
                end,
                InkpodVectorPoint{start.x, end.y}};
            for (std::size_t index = 0U; index < corners.size(); ++index) {
                segments.push_back(VectorLineSegment(
                    corners[index], corners[(index + 1U) % corners.size()], width));
            }
        } else if (state.tool == kInteractionVectorEllipse) {
            const InkpodVectorPoint start = points.front();
            const InkpodVectorPoint end = points.back();
            const float left = std::min(start.x, end.x);
            const float right = std::max(start.x, end.x);
            const float top = std::min(start.y, end.y);
            const float bottom = std::max(start.y, end.y);
            const float center_x = (left + right) / 2.0F;
            const float center_y = (top + bottom) / 2.0F;
            const float radius_x = (right - left) / 2.0F;
            const float radius_y = (bottom - top) / 2.0F;
            constexpr float kappa = 0.55228475F;
            const std::array<InkpodVectorCubicSegment, 4U> ellipse{
                InkpodVectorCubicSegment{sizeof(InkpodVectorCubicSegment), 0U,
                    {center_x + radius_x, center_y},
                    {center_x + radius_x, center_y + radius_y * kappa},
                    {center_x + radius_x * kappa, center_y + radius_y},
                    {center_x, center_y + radius_y}, width, width},
                InkpodVectorCubicSegment{sizeof(InkpodVectorCubicSegment), 0U,
                    {center_x, center_y + radius_y},
                    {center_x - radius_x * kappa, center_y + radius_y},
                    {center_x - radius_x, center_y + radius_y * kappa},
                    {center_x - radius_x, center_y}, width, width},
                InkpodVectorCubicSegment{sizeof(InkpodVectorCubicSegment), 0U,
                    {center_x - radius_x, center_y},
                    {center_x - radius_x, center_y - radius_y * kappa},
                    {center_x - radius_x * kappa, center_y - radius_y},
                    {center_x, center_y - radius_y}, width, width},
                InkpodVectorCubicSegment{sizeof(InkpodVectorCubicSegment), 0U,
                    {center_x, center_y - radius_y},
                    {center_x + radius_x * kappa, center_y - radius_y},
                    {center_x + radius_x, center_y - radius_y * kappa},
                    {center_x + radius_x, center_y}, width, width}};
            segments.assign(ellipse.begin(), ellipse.end());
        } else if (state.tool == kInteractionVectorPolyline) {
            segments.reserve(points.size() - 1U);
            for (std::size_t index = 1U; index < points.size(); ++index) {
                if (points[index - 1U].x != points[index].x
                    || points[index - 1U].y != points[index].y) {
                    segments.push_back(VectorLineSegment(
                        points[index - 1U], points[index], width));
                }
            }
        }
        return !segments.empty();
    } catch (const std::bad_alloc&) {
        segments.clear();
        return false;
    }
}

void UpdateVectorGeometryPreview(AppState& state) noexcept {
    std::vector<InkpodVectorPoint> points;
    std::vector<InkpodVectorCubicSegment> segments;
    bool closed{};
    if (!VectorGestureDocumentPoints(state, state.vector_gesture_samples, points)
        || !BuildVectorGestureSegments(state, points, segments, closed)) {
        return;
    }
    inkpod::renderer::CanvasGeometryPreview preview{};
    preview.struct_size = sizeof(preview);
    preview.active = 1U;
    preview.closed = closed ? 1U : 0U;
    for (const auto& segment : segments) {
        for (std::uint32_t step = 0U; step <= 8U; ++step) {
            if (preview.point_count >= inkpod::renderer::kCanvasGeometryPreviewPoints) {
                break;
            }
            if (step == 0U && preview.point_count != 0U) {
                continue;
            }
            const float t = static_cast<float>(step) / 8.0F;
            const float inverse = 1.0F - t;
            const float a = inverse * inverse * inverse;
            const float b = 3.0F * inverse * inverse * t;
            const float c = 3.0F * inverse * t * t;
            const float d = t * t * t;
            preview.points[preview.point_count++] = InkpodVectorPoint{
                a * segment.p0.x + b * segment.p1.x + c * segment.p2.x + d * segment.p3.x,
                a * segment.p0.y + b * segment.p1.y + c * segment.p2.y + d * segment.p3.y};
        }
    }
    SendMessageW(
        state.canvas,
        inkpod::renderer::kCanvasSetGeometryPreview,
        0,
        reinterpret_cast<LPARAM>(&preview));
}

InkpodStatus FinishVectorCanvasGesture(AppState& state) noexcept {
    TreePaneNode plane{};
    std::vector<InkpodVectorPoint> points;
    std::vector<InkpodVectorCubicSegment> segments;
    bool closed{};
    if (!QueryTreeNode(state, true, plane)) {
        ClearVectorGeometryPreview(state);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!IsVectorStrokePlane(plane.kind)) {
        if (state.engine != nullptr) {
            state.engine->SetLocalFailure(kVectorStrokePlaneRequired);
        }
        ClearVectorGeometryPreview(state);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!VectorGestureDocumentPoints(state, state.vector_gesture_samples, points)) {
        if (state.engine != nullptr) {
            state.engine->SetLocalFailure(
                L"ベクター描画の入力点を文書座標へ変換できませんでした。");
        }
        ClearVectorGeometryPreview(state);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.tool == kInteractionVectorEraser) {
        const InkpodVectorPoint point = points.back();
        const InkpodVectorEraseInput input{
            sizeof(InkpodVectorEraseInput),
            state.vector_erase_mode,
            plane.id,
            point.x,
            point.y,
            std::max(0.5F, state.diameter / 2.0F),
            0U};
        ClearVectorGeometryPreview(state);
        return state.engine->Invoke(
            [input](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_vector_erase(core, &input, &result);
            },
            true,
            true);
    }
    if (!BuildVectorGestureSegments(state, points, segments, closed)) {
        if (state.engine != nullptr) {
            state.engine->SetLocalFailure(
                L"ベクター描画を確定するための入力点が不足しています。");
        }
        ClearVectorGeometryPreview(state);
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodVectorPathInput input{
        sizeof(InkpodVectorPathInput),
        0U,
        closed ? INKPOD_VECTOR_PATH_CLOSED : 0U,
        plane.id,
        state.drawing_color,
        segments.data(),
        segments.size(),
        sizeof(InkpodVectorCubicSegment)};
    const InkpodStatus status = state.engine->Invoke(
        [&input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t path_id{};
            return inkpod_core_vector_add_path(core, &input, &result, &path_id);
        },
        true,
        true);
    ClearVectorGeometryPreview(state);
    return status;
}

InkpodStatus SelectVectorObjects(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodVectorSelectionMode mode = state.vector_selection_mode;
    return state.engine->Invoke(
        [&state, mode](InkpodCore* core) {
            InkpodDocumentInfo document = EmptyDocumentInfo();
            InkpodStatus status = inkpod_core_get_document_info(core, &document);
            InkpodLocatorOutput locator{};
            locator.struct_size = sizeof(locator);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_locator_sample(core, 0U, 0.0, 0.0, &locator);
            }
            InkpodFrameRect bounds = locator.selection;
            if (bounds.width <= 0 || bounds.height <= 0) {
                bounds = InkpodFrameRect{
                    0, 0, static_cast<std::int32_t>(document.width),
                    static_cast<std::int32_t>(document.height)};
            }
            const InkpodVectorSelectionInput input{
                sizeof(InkpodVectorSelectionInput), mode, 0U, bounds};
            InkpodVectorSelectionBuffer output{};
            output.struct_size = sizeof(output);
            status = inkpod_core_vector_select(core, &input, &output);
            if (status == INKPOD_STATUS_BUFFER_TOO_SMALL) {
                status = INKPOD_STATUS_OK;
            }
            std::vector<InkpodVectorSelectionRange> ranges;
            std::vector<std::uint64_t> fill_ids;
            try {
                ranges.resize(static_cast<std::size_t>(output.range_count));
                fill_ids.resize(static_cast<std::size_t>(output.fill_count));
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            for (auto& range : ranges) {
                range.struct_size = sizeof(range);
            }
            output.ranges = ranges.data();
            output.range_capacity = ranges.size();
            output.fill_ids = fill_ids.data();
            output.fill_capacity = fill_ids.size();
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_vector_select(core, &input, &output);
            }
            if (status == INKPOD_STATUS_OK) {
                try {
                    state.vector_selected_path_ids.clear();
                    for (const auto& range : ranges) {
                        if (std::find(
                                state.vector_selected_path_ids.begin(),
                                state.vector_selected_path_ids.end(),
                                range.path_id) == state.vector_selected_path_ids.end()) {
                            state.vector_selected_path_ids.push_back(range.path_id);
                        }
                    }
                } catch (const std::bad_alloc&) {
                    return INKPOD_STATUS_INVALID_STATE;
                }
            }
            return status;
        },
        false,
        false);
}

InkpodStatus ConnectSelectedVectorPlane(AppState& state, float maximum_gap) noexcept {
    TreePaneNode plane{};
    if (!QueryTreeNode(state, true, plane) || maximum_gap <= 0.0F) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    return state.engine->Invoke(
        [plane_id = plane.id, maximum_gap](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t path_id{};
            return inkpod_core_vector_connect(
                core, plane_id, maximum_gap, &result, &path_id);
        },
        true,
        true);
}

InkpodStatus CorrectSelectedVectorWidth(
    AppState& state, InkpodVectorWidthMode mode, float parameter) noexcept {
    if (state.vector_selected_path_ids.empty()) {
        const InkpodStatus select_status = SelectVectorObjects(state);
        if (select_status != INKPOD_STATUS_OK) {
            return select_status;
        }
    }
    if (state.vector_selected_path_ids.empty()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodVectorWidthInput input{
        sizeof(InkpodVectorWidthInput),
        mode,
        0U,
        state.vector_selected_path_ids.data(),
        state.vector_selected_path_ids.size(),
        parameter,
        0U};
    return state.engine->Invoke(
        [&input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_vector_correct_width(core, &input, &result);
        },
        true,
        true);
}

InkpodStatus AdjustSelection(
    AppState& state, std::uint32_t operation, std::uint32_t pixels) noexcept {
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [operation, pixels](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_selection_adjust(
                      core, operation, pixels, &result);
              },
              true,
              true);
}

InkpodStatus EditPaperFrames(AppState& state, UINT command) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodPaperFramesInput input{};
    input.struct_size = sizeof(input);
    input.hundred_frame = info.hundred_frame;
    input.reference_frame = info.reference_frame;
    input.drawing_frame = info.drawing_frame;
    input.safe_frame = info.safe_frame;
    input.margin_left = info.margin_left;
    input.margin_top = info.margin_top;
    input.margin_right = info.margin_right;
    input.margin_bottom = info.margin_bottom;

    ViewOptionsDialogState dialog{};
    dialog.value_count = 4U;
    InkpodFrameRect* frame{};
    if (command == IDM_CELL_FRAME_HUNDRED) {
        dialog.title = L"100フレーム";
        frame = &input.hundred_frame;
    } else if (command == IDM_CELL_FRAME_REFERENCE) {
        dialog.title = L"基準フレーム";
        frame = &input.reference_frame;
    } else if (command == IDM_CELL_FRAME_DRAWING) {
        dialog.title = L"作画フレーム";
        frame = &input.drawing_frame;
    } else if (command == IDM_CELL_FRAME_SAFE) {
        dialog.title = L"安全フレーム";
        frame = &input.safe_frame;
    }
    if (frame != nullptr) {
        dialog.labels = {L"X", L"Y", L"幅", L"高さ"};
        dialog.values = {frame->x, frame->y, frame->width, frame->height};
        if (state.smoke_test && info.width > 8U && info.height > 8U
            && command != IDM_CELL_FRAME_HUNDRED) {
            const std::int32_t inset = command == IDM_CELL_FRAME_REFERENCE
                ? 1
                : (command == IDM_CELL_FRAME_DRAWING ? 2 : 3);
            dialog.values = {
                inset,
                inset,
                static_cast<std::int32_t>(info.width) - inset * 2,
                static_cast<std::int32_t>(info.height) - inset * 2};
        }
    } else if (command == IDM_CELL_MARGINS) {
        dialog.title = L"余白";
        dialog.labels = {L"左", L"上", L"右", L"下"};
        dialog.values = {
            static_cast<std::int32_t>(input.margin_left),
            static_cast<std::int32_t>(input.margin_top),
            static_cast<std::int32_t>(input.margin_right),
            static_cast<std::int32_t>(input.margin_bottom)};
        if (state.smoke_test) {
            dialog.values = {1, 2, 3, 4};
        }
    } else {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (ShowViewOptions(
            state.instance, state.window, state.smoke_test, dialog) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    if (frame != nullptr) {
        frame->x = dialog.values[0];
        frame->y = dialog.values[1];
        frame->width = dialog.values[2];
        frame->height = dialog.values[3];
    } else {
        if (std::any_of(
                dialog.values.cbegin(),
                dialog.values.cend(),
                [](std::int32_t value) { return value < 0; })) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        input.margin_left = static_cast<std::uint32_t>(dialog.values[0]);
        input.margin_top = static_cast<std::uint32_t>(dialog.values[1]);
        input.margin_right = static_cast<std::uint32_t>(dialog.values[2]);
        input.margin_bottom = static_cast<std::uint32_t>(dialog.values[3]);
    }
    return state.engine->Invoke(
        [input](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_update_paper_frames(core, &input, &result);
        },
        true,
        true);
}

InkpodStatus CreateCell(
    AppState& state,
    std::uint32_t width,
    std::uint32_t height,
    std::uint32_t dpi_milli) noexcept {
    if (width == 0U || height == 0U || dpi_milli == 0U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    GUID uuid{};
    if (FAILED(CoCreateGuid(&uuid))) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    static_assert(sizeof(uuid) == sizeof(std::uint64_t) * 2U);
    std::uint64_t uuid_high{};
    std::uint64_t uuid_low{};
    std::memcpy(&uuid_high, &uuid, sizeof(uuid_high));
    std::memcpy(
        &uuid_low,
        reinterpret_cast<const std::uint8_t*>(&uuid) + sizeof(uuid_high),
        sizeof(uuid_low));
    std::wstring private_recovery_path;
    if (!PrivateRecoveryPath(uuid_high, uuid_low, private_recovery_path)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        uuid_high,
        uuid_low,
        width,
        height,
        dpi_milli,
        dpi_milli};
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [options](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_new_cell(core, &options, &info);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.current_path.clear();
    state.recovery_path = std::move(private_recovery_path);
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    state.plane = INKPOD_PLANE_MAIN_LINE;
    ResetM3DocumentUiState(state);
    const InkpodPlaneKind plane = state.plane;
    const InkpodStatus plane_status = state.engine->Invoke(
        [plane](InkpodCore* core) { return inkpod_core_set_active_plane(core, plane); },
        false,
        false);
    if (plane_status != INKPOD_STATUS_OK) {
        return plane_status;
    }
    return FitCanvas(state, INKPOD_VIEW_FIT);
}

InkpodStatus CreateDefaultCell(AppState& state) noexcept {
    return CreateCell(state, 1920U, 1080U, 96000U);
}

bool ChooseInkpodPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    if (!selected_path.empty()) {
        wcsncpy_s(path.data(), path.size(), selected_path.c_str(), _TRUNCATE);
    }
    constexpr wchar_t filter[] = L"inkpod セル (*.inkpod)\0*.inkpod\0すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.lpstrDefExt = L"inkpod";
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    const BOOL accepted = save ? GetSaveFileNameW(&dialog) : GetOpenFileNameW(&dialog);
    if (accepted == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ChooseCommonRasterPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    if (!selected_path.empty()) {
        wcsncpy_s(path.data(), path.size(), selected_path.c_str(), _TRUNCATE);
    }
    constexpr wchar_t filter[] =
        L"対応画像 (*.png;*.tif;*.tiff;*.tga;*.bmp)\0*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"PNG (*.png)\0*.png\0TIFF (*.tif;*.tiff)\0*.tif;*.tiff\0"
        L"TGA (*.tga)\0*.tga\0BMP (*.bmp)\0*.bmp\0すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.lpstrDefExt = L"png";
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    const BOOL accepted = save ? GetSaveFileNameW(&dialog) : GetOpenFileNameW(&dialog);
    if (accepted == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ChooseCommonRasterPaths(
    HWND owner, std::vector<std::wstring>& selected_paths) noexcept {
    std::vector<wchar_t> buffer;
    try {
        buffer.resize(65536U);
    } catch (const std::bad_alloc&) {
        return false;
    }
    constexpr wchar_t filter[] =
        L"対応画像 (*.png;*.tif;*.tiff;*.tga;*.bmp)\0*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
    dialog.lpstrFile = buffer.data();
    dialog.nMaxFile = static_cast<DWORD>(buffer.size());
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | OFN_FILEMUSTEXIST | OFN_ALLOWMULTISELECT;
    if (GetOpenFileNameW(&dialog) == FALSE) {
        return false;
    }
    selected_paths.clear();
    try {
        const std::wstring first(buffer.data());
        const wchar_t* cursor = buffer.data() + first.size() + 1U;
        if (*cursor == L'\0') {
            selected_paths.push_back(first);
            return true;
        }
        while (*cursor != L'\0') {
            const std::wstring name(cursor);
            selected_paths.push_back(first + L"\\" + name);
            cursor += name.size() + 1U;
        }
        return !selected_paths.empty();
    } catch (const std::bad_alloc&) {
        selected_paths.clear();
        return false;
    }
}

bool ChooseOpenDocumentPath(HWND owner, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    constexpr wchar_t filter[] =
        L"inkpod/対応画像 (*.inkpod;*.png;*.tif;*.tiff;*.tga;*.bmp)\0"
        L"*.inkpod;*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"inkpod セル (*.inkpod)\0*.inkpod\0対応画像\0*.png;*.tif;*.tiff;*.tga;*.bmp\0"
        L"すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_FILEMUSTEXIST;
    if (GetOpenFileNameW(&dialog) == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

InkpodCommonRasterFormat CommonRasterFormatFromPath(
    const std::wstring& path) noexcept {
    const std::size_t dot = path.find_last_of(L'.');
    if (dot == std::wstring::npos) {
        return 0U;
    }
    std::wstring extension;
    try {
        extension = path.substr(dot + 1U);
    } catch (const std::bad_alloc&) {
        return 0U;
    }
    std::transform(extension.begin(), extension.end(), extension.begin(), towlower);
    if (extension == L"png") {
        return INKPOD_COMMON_RASTER_PNG;
    }
    if (extension == L"tif" || extension == L"tiff") {
        return INKPOD_COMMON_RASTER_TIFF;
    }
    if (extension == L"tga") {
        return INKPOD_COMMON_RASTER_TGA;
    }
    if (extension == L"bmp") {
        return INKPOD_COMMON_RASTER_BMP;
    }
    return 0U;
}

bool ReadBoundedFile(
    const std::wstring& path, std::vector<std::uint8_t>& output) noexcept {
    constexpr std::uint64_t maximum_bytes = UINT64_C(512) * 1024U * 1024U;
    HANDLE file = CreateFileW(
        path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_SEQUENTIAL_SCAN, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    LARGE_INTEGER size{};
    if (GetFileSizeEx(file, &size) == FALSE || size.QuadPart <= 0
        || static_cast<std::uint64_t>(size.QuadPart) > maximum_bytes) {
        CloseHandle(file);
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(size.QuadPart));
    } catch (const std::bad_alloc&) {
        CloseHandle(file);
        return false;
    }
    std::size_t offset{};
    while (offset < output.size()) {
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(
            output.size() - offset, 1024U * 1024U));
        DWORD read{};
        if (ReadFile(file, output.data() + offset, chunk, &read, nullptr) == FALSE
            || read != chunk) {
            CloseHandle(file);
            output.clear();
            return false;
        }
        offset += read;
    }
    const bool closed = CloseHandle(file) != FALSE;
    return closed;
}

bool WriteFileAtomically(
    const std::wstring& path, const std::vector<std::uint8_t>& bytes) noexcept {
    if (path.empty() || bytes.empty()) {
        return false;
    }
    GUID guid{};
    std::array<wchar_t, 40> guid_text{};
    if (FAILED(CoCreateGuid(&guid))
        || StringFromGUID2(guid, guid_text.data(), static_cast<int>(guid_text.size())) <= 0) {
        return false;
    }
    std::wstring temporary;
    try {
        temporary = path + L"." + guid_text.data() + L".tmp";
    } catch (const std::bad_alloc&) {
        return false;
    }
    HANDLE file = CreateFileW(
        temporary.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW,
        FILE_ATTRIBUTE_TEMPORARY, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    bool success = true;
    std::size_t offset{};
    while (offset < bytes.size()) {
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(
            bytes.size() - offset, 1024U * 1024U));
        DWORD written{};
        if (WriteFile(file, bytes.data() + offset, chunk, &written, nullptr) == FALSE
            || written != chunk) {
            success = false;
            break;
        }
        offset += written;
    }
    success = success && FlushFileBuffers(file) != FALSE;
    success = CloseHandle(file) != FALSE && success;
    if (success) {
        success = MoveFileExW(
                      temporary.c_str(), path.c_str(),
                      MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
            != FALSE;
    }
    if (!success) {
        DeleteFileW(temporary.c_str());
    }
    return success;
}

bool ChoosePalettePath(HWND owner, bool save, std::wstring& path) noexcept {
    std::array<wchar_t, 32768U> buffer{};
    if (!path.empty()) {
        wcsncpy_s(buffer.data(), buffer.size(), path.c_str(), _TRUNCATE);
    }
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = L"Inkpod Palette (*.inkpalette)\0*.inkpalette\0すべてのファイル\0*.*\0\0";
    dialog.lpstrFile = buffer.data();
    dialog.nMaxFile = static_cast<DWORD>(buffer.size());
    dialog.lpstrDefExt = L"inkpalette";
    dialog.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    if ((save ? GetSaveFileNameW(&dialog) : GetOpenFileNameW(&dialog)) == FALSE) {
        return false;
    }
    try {
        path.assign(buffer.data());
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

bool SavePaletteFile(
    const std::wstring& path, const std::vector<InkpodColorValue>& colors) noexcept {
    if (colors.size() > 4096U) {
        return false;
    }
    const std::uint64_t bytes_u64 = 12U
        + static_cast<std::uint64_t>(colors.size()) * sizeof(InkpodColorValue);
    if (bytes_u64 > static_cast<std::uint64_t>(SIZE_MAX)) {
        return false;
    }
    std::vector<std::uint8_t> bytes;
    try {
        bytes.resize(static_cast<std::size_t>(bytes_u64));
    } catch (const std::bad_alloc&) {
        return false;
    }
    const std::array<std::uint8_t, 8U> magic{'I', 'N', 'K', 'P', 'A', 'L', '1', 0};
    std::memcpy(bytes.data(), magic.data(), magic.size());
    const std::uint32_t count = static_cast<std::uint32_t>(colors.size());
    std::memcpy(bytes.data() + 8U, &count, sizeof(count));
    if (!colors.empty()) {
        std::memcpy(
            bytes.data() + 12U,
            colors.data(),
            colors.size() * sizeof(InkpodColorValue));
    }
    return WriteFileAtomically(path, bytes);
}

bool LoadPaletteFile(
    const std::wstring& path, std::vector<InkpodColorValue>& colors) noexcept {
    std::vector<std::uint8_t> bytes;
    if (!ReadBoundedFile(path, bytes) || bytes.size() < 12U
        || std::memcmp(bytes.data(), "INKPAL1\0", 8U) != 0) {
        return false;
    }
    std::uint32_t count{};
    std::memcpy(&count, bytes.data() + 8U, sizeof(count));
    if (count > 4096U
        || bytes.size() != 12U + static_cast<std::size_t>(count) * sizeof(InkpodColorValue)) {
        return false;
    }
    try {
        colors.resize(count);
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (count != 0U) {
        std::memcpy(
            colors.data(), bytes.data() + 12U,
            static_cast<std::size_t>(count) * sizeof(InkpodColorValue));
    }
    return std::all_of(colors.begin(), colors.end(), [](const InkpodColorValue& color) {
        return color.struct_size == sizeof(InkpodColorValue)
            && (color.depth == INKPOD_COLOR_DEPTH_8
                || color.depth == INKPOD_COLOR_DEPTH_16)
            && (color.depth == INKPOD_COLOR_DEPTH_16
                || (color.red <= UINT8_MAX && color.green <= UINT8_MAX
                    && color.blue <= UINT8_MAX && color.alpha <= UINT8_MAX));
    });
}

bool ChooseChartPath(HWND owner, bool save, std::wstring& path) noexcept {
    std::array<wchar_t, 32768U> buffer{};
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = L"Inkpod Color Chart (*.inkchart)\0*.inkchart\0すべてのファイル\0*.*\0\0";
    dialog.lpstrFile = buffer.data();
    dialog.nMaxFile = static_cast<DWORD>(buffer.size());
    dialog.lpstrDefExt = L"inkchart";
    dialog.Flags = OFN_EXPLORER | OFN_NOCHANGEDIR
        | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    if ((save ? GetSaveFileNameW(&dialog) : GetOpenFileNameW(&dialog)) == FALSE) {
        return false;
    }
    try {
        path.assign(buffer.data());
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

bool SaveColorChartFile(
    const std::wstring& path,
    const std::vector<InkpodColorValue>& colors,
    const std::vector<std::wstring>& names) noexcept {
    if (colors.size() > 4096U || names.size() != colors.size()) {
        return false;
    }
    std::vector<std::vector<std::uint8_t>> encoded_names;
    std::uint64_t total = 12U;
    try {
        encoded_names.resize(names.size());
        for (std::size_t index = 0U; index < names.size(); ++index) {
            if (!WidePathToUtf8(names[index], encoded_names[index])
                || encoded_names[index].empty() || encoded_names[index].size() > 1024U) {
                return false;
            }
            total += sizeof(InkpodColorValue) + sizeof(std::uint32_t)
                + encoded_names[index].size();
        }
        if (total > UINT64_C(16777216) || total > SIZE_MAX) {
            return false;
        }
        std::vector<std::uint8_t> bytes(static_cast<std::size_t>(total));
        std::memcpy(bytes.data(), "INKCHT1\0", 8U);
        const std::uint32_t count = static_cast<std::uint32_t>(colors.size());
        std::memcpy(bytes.data() + 8U, &count, sizeof(count));
        std::size_t offset = 12U;
        for (std::size_t index = 0U; index < colors.size(); ++index) {
            std::memcpy(bytes.data() + offset, &colors[index], sizeof(InkpodColorValue));
            offset += sizeof(InkpodColorValue);
            const std::uint32_t name_bytes = static_cast<std::uint32_t>(
                encoded_names[index].size());
            std::memcpy(bytes.data() + offset, &name_bytes, sizeof(name_bytes));
            offset += sizeof(name_bytes);
            std::memcpy(bytes.data() + offset, encoded_names[index].data(), name_bytes);
            offset += name_bytes;
        }
        return WriteFileAtomically(path, bytes);
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool LoadColorChartFile(
    const std::wstring& path,
    std::vector<InkpodColorValue>& colors,
    std::vector<std::wstring>& names) noexcept {
    std::vector<std::uint8_t> bytes;
    if (!ReadBoundedFile(path, bytes) || bytes.size() < 12U
        || std::memcmp(bytes.data(), "INKCHT1\0", 8U) != 0) {
        return false;
    }
    std::uint32_t count{};
    std::memcpy(&count, bytes.data() + 8U, sizeof(count));
    if (count > 4096U) {
        return false;
    }
    try {
        colors.clear();
        names.clear();
        colors.reserve(count);
        names.reserve(count);
        std::size_t offset = 12U;
        for (std::uint32_t index = 0U; index < count; ++index) {
            if (bytes.size() - offset
                < sizeof(InkpodColorValue) + sizeof(std::uint32_t)) {
                return false;
            }
            InkpodColorValue color{};
            std::memcpy(&color, bytes.data() + offset, sizeof(color));
            offset += sizeof(color);
            std::uint32_t name_bytes{};
            std::memcpy(&name_bytes, bytes.data() + offset, sizeof(name_bytes));
            offset += sizeof(name_bytes);
            if (name_bytes == 0U || name_bytes > 1024U
                || name_bytes > bytes.size() - offset
                || color.struct_size != sizeof(InkpodColorValue)
                || (color.depth != INKPOD_COLOR_DEPTH_8
                    && color.depth != INKPOD_COLOR_DEPTH_16)) {
                return false;
            }
            const char* text = reinterpret_cast<const char*>(bytes.data() + offset);
            const int wide_count = MultiByteToWideChar(
                CP_UTF8, MB_ERR_INVALID_CHARS, text, static_cast<int>(name_bytes), nullptr, 0);
            if (wide_count <= 0) {
                return false;
            }
            std::wstring name(static_cast<std::size_t>(wide_count), L'\0');
            if (MultiByteToWideChar(
                    CP_UTF8,
                    MB_ERR_INVALID_CHARS,
                    text,
                    static_cast<int>(name_bytes),
                    name.data(),
                    wide_count) != wide_count) {
                return false;
            }
            colors.push_back(color);
            names.push_back(std::move(name));
            offset += name_bytes;
        }
        return offset == bytes.size();
    } catch (const std::bad_alloc&) {
        colors.clear();
        names.clear();
        return false;
    }
}

InkpodStatus ImportCommonRasterFromPath(
    AppState& state, const std::wstring& path) noexcept {
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(path);
    std::vector<std::uint8_t> bytes;
    if (state.engine == nullptr || format == 0U || !ReadBoundedFile(path, bytes)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    GUID uuid{};
    if (FAILED(CoCreateGuid(&uuid))) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t high{};
    std::uint64_t low{};
    std::memcpy(&high, &uuid, sizeof(high));
    std::memcpy(&low, reinterpret_cast<const std::uint8_t*>(&uuid) + sizeof(high), sizeof(low));
    const InkpodStatus status = state.engine->Invoke(
        [format, bytes = std::move(bytes), high, low](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_import_common_raster(
                core, format, bytes.data(), bytes.size(), high, low, &info);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.current_path.clear();
    state.recovery_path.clear();
    state.plane = INKPOD_PLANE_COLOR;
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetM3DocumentUiState(state);
    const InkpodStatus plane_status = state.engine->Invoke(
        [](InkpodCore* core) { return inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR); },
        false,
        false);
    return plane_status == INKPOD_STATUS_OK
        ? FitCanvas(state, INKPOD_VIEW_FIT)
        : plane_status;
}

InkpodStatus ExportCommonRasterToPath(
    AppState& state, const std::wstring& path, bool composite_white) noexcept {
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(path);
    if (state.engine == nullptr || format == 0U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<std::uint8_t> bytes;
    const InkpodStatus status = state.engine->Invoke(
        [format, composite_white, &bytes](InkpodCore* core) {
            InkpodByteBuffer* buffer{};
            InkpodStatus current = inkpod_core_export_common_raster(
                core, format, composite_white ? 1U : 0U, &buffer);
            const std::uint8_t* data{};
            std::uint64_t length{};
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_byte_buffer_view(buffer, &data, &length);
            }
            if (current == INKPOD_STATUS_OK) {
                try {
                    bytes.assign(data, data + static_cast<std::size_t>(length));
                } catch (const std::bad_alloc&) {
                    current = INKPOD_STATUS_INVALID_STATE;
                }
            }
            const InkpodStatus release_status = inkpod_byte_buffer_release(&buffer);
            return current == INKPOD_STATUS_OK ? release_status : current;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    return WriteFileAtomically(path, bytes) ? INKPOD_STATUS_OK : INKPOD_STATUS_IO_ERROR;
}

InkpodStatus ApplyLightTableEdit(
    AppState& state,
    InkpodLightTableEdit edit,
    const std::wstring& name,
    std::uint64_t& object_id) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!name.empty() && !WidePathToUtf8(name, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    edit.struct_size = sizeof(edit);
    edit.name_utf8 = utf8.empty() ? nullptr : utf8.data();
    edit.name_bytes = utf8.size();
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [edit, utf8, &object_id](InkpodCore* core) mutable {
                  InkpodLightTableEdit input = edit;
                  input.name_utf8 = utf8.empty() ? nullptr : utf8.data();
                  input.name_bytes = utf8.size();
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_light_table_edit(
                      core, &input, &result, &object_id);
              },
              true,
              true);
}

bool QueryLightTableItem(
    AppState& state,
    std::uint32_t index,
    InkpodLightTableItemInfo& output) noexcept {
    output = {};
    output.struct_size = sizeof(output);
    output.display_color.struct_size = sizeof(output.display_color);
    return state.engine != nullptr
        && state.engine->Invoke(
               [index, &output](InkpodCore* core) {
                   return inkpod_core_light_table_item_get(core, index, &output);
               },
               false,
               false)
            == INKPOD_STATUS_OK;
}

InkpodStatus AddOrReloadLightTableRaster(
    AppState& state, const std::wstring& path, bool reload) noexcept {
    std::vector<std::uint8_t> bytes;
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(path);
    if (state.engine == nullptr || format == 0U || !ReadBoundedFile(path, bytes)) {
        return INKPOD_STATUS_IO_ERROR;
    }
    GUID uuid{};
    if (FAILED(CoCreateGuid(&uuid))) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t high{};
    std::uint64_t low{};
    std::memcpy(&high, &uuid, sizeof(high));
    std::memcpy(&low, reinterpret_cast<const std::uint8_t*>(&uuid) + sizeof(high), sizeof(low));
    std::wstring filename = path;
    const std::size_t slash = filename.find_last_of(L"\\/");
    if (slash != std::wstring::npos) {
        filename.erase(0, slash + 1U);
    }
    std::vector<std::uint8_t> name;
    if (!WidePathToUtf8(filename, name)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const std::uint64_t item_id = state.active_light_table_item_id;
    std::uint64_t added_item_id{};
    const InkpodStatus status = state.engine->Invoke(
        [reload,
         item_id,
         format,
         bytes = std::move(bytes),
         name = std::move(name),
         high,
         low,
         &added_item_id](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            if (reload) {
                return inkpod_core_light_table_reload_common_raster(
                    core,
                    item_id,
                    format,
                    bytes.data(),
                    bytes.size(),
                    high,
                    low,
                    1U,
                    &result);
            }
            return inkpod_core_light_table_add_common_raster(
                core,
                format,
                bytes.data(),
                bytes.size(),
                name.data(),
                name.size(),
                high,
                low,
                1U,
                &result,
                &added_item_id);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK && !reload && added_item_id != 0U) {
        state.active_light_table_item_id = added_item_id;
        state.active_light_table_item_index = 0U;
    }
    return status;
}

InkpodStatus EditLightTableItemProperties(AppState& state) noexcept {
    InkpodLightTableItemInfo info{};
    if (!QueryLightTableItem(state, state.active_light_table_item_index, info)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState display{};
    display.title = L"ライトテーブル表示";
    display.labels = {L"不透明度 (%)", L"表示 (1:色 2:単色 3:網点)", L"表示 (0/1)", L"回転 (度)"};
    display.values = {
        static_cast<std::int32_t>(info.opacity_milli / 10U),
        static_cast<std::int32_t>(info.display_mode),
        (info.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE) != 0U ? 1 : 0,
        info.rotation_milli_degrees / 1000};
    display.value_count = 4U;
    if (state.smoke_test) {
        display.values = {50, INKPOD_LIGHT_TABLE_MONOTONE, 1, 5};
    }
    if (ShowViewOptions(state.instance, state.window, state.smoke_test, display) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState transform{};
    transform.title = L"ライトテーブル変形";
    transform.labels = {L"移動X (1/1000px)", L"移動Y (1/1000px)", L"倍率X (%)", L"倍率Y (%)"};
    transform.values = {
        info.translate_x_milli,
        info.translate_y_milli,
        static_cast<std::int32_t>(info.scale_x_milli / 10U),
        static_cast<std::int32_t>(info.scale_y_milli / 10U)};
    transform.value_count = 4U;
    if (state.smoke_test) {
        transform.values = {1000, -1000, 110, 90};
    }
    if (ShowViewOptions(state.instance, state.window, state.smoke_test, transform) != IDOK
        || display.values[0] < 0 || display.values[0] > 100
        || display.values[1] < INKPOD_LIGHT_TABLE_COLOR
        || display.values[1] > INKPOD_LIGHT_TABLE_HALFTONE
        || transform.values[2] <= 0 || transform.values[3] <= 0) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodLightTableEdit edit{};
    edit.operation = INKPOD_LIGHT_TABLE_UPDATE_ITEM;
    edit.object_id = info.id;
    edit.flags = display.values[2] != 0 ? INKPOD_LIGHT_TABLE_ITEM_VISIBLE : 0U;
    edit.opacity_milli = static_cast<std::uint32_t>(display.values[0]) * 10U;
    edit.display_mode = static_cast<std::uint32_t>(display.values[1]);
    edit.display_color = M6Color(state.color_rgba);
    edit.translate_x_milli = transform.values[0];
    edit.translate_y_milli = transform.values[1];
    edit.scale_x_milli = static_cast<std::uint32_t>(transform.values[2]) * 10U;
    edit.scale_y_milli = static_cast<std::uint32_t>(transform.values[3]) * 10U;
    edit.rotation_milli_degrees = display.values[3] * 1000;
    std::uint64_t ignored{};
    return ApplyLightTableEdit(state, edit, {}, ignored);
}

InkpodStatus MoveLightTableFromCanvas(AppState& state) noexcept {
    if (state.light_table_move_samples.size() < 2U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodDocumentInfo document{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, document) || document.width == 0U
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(document.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    double delta_x = (state.light_table_move_samples.back().x
        - state.light_table_move_samples.front().x) / zoom;
    double delta_y = (state.light_table_move_samples.back().y
        - state.light_table_move_samples.front().y) / zoom;
    if (state.view_flip_horizontal) {
        delta_x = -delta_x;
    }
    if (state.view_flip_vertical) {
        delta_y = -delta_y;
    }
    const std::int64_t delta_x_milli = std::llround(delta_x * 1000.0);
    const std::int64_t delta_y_milli = std::llround(delta_y * 1000.0);
    const bool all_items = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
    InkpodStatus status = INKPOD_STATUS_OK;
    for (std::uint32_t index = 0U; index < 10000U; ++index) {
        if (!all_items && index != state.active_light_table_item_index) {
            continue;
        }
        InkpodLightTableItemInfo info{};
        if (!QueryLightTableItem(state, index, info)) {
            break;
        }
        const std::int64_t translated_x = static_cast<std::int64_t>(info.translate_x_milli)
            + delta_x_milli;
        const std::int64_t translated_y = static_cast<std::int64_t>(info.translate_y_milli)
            + delta_y_milli;
        if (translated_x < INT32_MIN || translated_x > INT32_MAX
            || translated_y < INT32_MIN || translated_y > INT32_MAX) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        InkpodLightTableEdit edit{};
        edit.operation = INKPOD_LIGHT_TABLE_UPDATE_ITEM;
        edit.object_id = info.id;
        edit.flags = info.flags;
        edit.opacity_milli = info.opacity_milli;
        edit.display_mode = info.display_mode;
        edit.display_color = info.display_color;
        edit.translate_x_milli = static_cast<std::int32_t>(translated_x);
        edit.translate_y_milli = static_cast<std::int32_t>(translated_y);
        edit.scale_x_milli = info.scale_x_milli;
        edit.scale_y_milli = info.scale_y_milli;
        edit.rotation_milli_degrees = info.rotation_milli_degrees;
        std::uint64_t ignored{};
        status = ApplyLightTableEdit(state, edit, {}, ignored);
        if (status != INKPOD_STATUS_OK || !all_items) {
            break;
        }
    }
    return status;
}

struct SequenceEncodedFile {
    std::vector<std::uint8_t> name;
    std::vector<std::uint8_t> bytes;
};

InkpodStatus ImportSequencePaths(
    AppState& state, const std::vector<std::wstring>& paths) noexcept {
    if (state.engine == nullptr || paths.empty()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(paths.front());
    if (format == 0U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<SequenceEncodedFile> files;
    try {
        files.reserve(paths.size());
        for (const std::wstring& path : paths) {
            if (CommonRasterFormatFromPath(path) != format) {
                return INKPOD_STATUS_UNSUPPORTED;
            }
            std::wstring filename = path;
            const std::size_t slash = filename.find_last_of(L"\\/");
            if (slash != std::wstring::npos) {
                filename.erase(0, slash + 1U);
            }
            SequenceEncodedFile file{};
            if (!WidePathToUtf8(filename, file.name)
                || !ReadBoundedFile(path, file.bytes)) {
                return INKPOD_STATUS_IO_ERROR;
            }
            files.push_back(std::move(file));
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    return state.engine->Invoke(
        [format, files = std::move(files)](InkpodCore* core) {
            std::vector<InkpodNamedBytesInput> records;
            try {
                records.reserve(files.size());
                for (const auto& file : files) {
                    records.push_back(InkpodNamedBytesInput{
                        sizeof(InkpodNamedBytesInput),
                        0U,
                        file.name.data(),
                        file.name.size(),
                        file.bytes.data(),
                        file.bytes.size()});
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return inkpod_core_sequence_import_encoded(
                core, format, records.data(), records.size(), sizeof(InkpodNamedBytesInput));
        },
        false,
        false);
}

InkpodStatus ExportSequenceToPath(
    AppState& state, const std::wstring& selected_path, bool composite_white) noexcept {
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(selected_path);
    if (state.engine == nullptr || format == 0U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<SequenceEncodedFile> files;
    const InkpodStatus status = state.engine->Invoke(
        [format, composite_white, &files](InkpodCore* core) {
            InkpodEncodedSequence* encoded{};
            InkpodStatus current = inkpod_core_sequence_export_encoded(
                core, format, composite_white ? 1U : 0U, &encoded);
            std::uint64_t count{};
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_encoded_sequence_count(encoded, &count);
            }
            try {
                files.reserve(static_cast<std::size_t>(count));
                for (std::uint64_t index = 0U;
                     current == INKPOD_STATUS_OK && index < count;
                     ++index) {
                    const std::uint8_t* name{};
                    std::uint64_t name_bytes{};
                    const std::uint8_t* data{};
                    std::uint64_t data_bytes{};
                    current = inkpod_encoded_sequence_get(
                        encoded,
                        index,
                        &name,
                        &name_bytes,
                        &data,
                        &data_bytes);
                    if (current == INKPOD_STATUS_OK) {
                        files.push_back(SequenceEncodedFile{
                            std::vector<std::uint8_t>(name, name + name_bytes),
                            std::vector<std::uint8_t>(data, data + data_bytes)});
                    }
                }
            } catch (const std::bad_alloc&) {
                current = INKPOD_STATUS_INVALID_STATE;
            }
            const InkpodStatus release = inkpod_encoded_sequence_release(&encoded);
            return current == INKPOD_STATUS_OK ? release : current;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    const std::size_t slash = selected_path.find_last_of(L"\\/");
    const std::size_t dot = selected_path.find_last_of(L'.');
    const std::wstring directory = slash == std::wstring::npos
        ? L""
        : selected_path.substr(0, slash + 1U);
    const std::wstring stem = selected_path.substr(
        slash == std::wstring::npos ? 0U : slash + 1U,
        dot == std::wstring::npos
            ? std::wstring::npos
            : dot - (slash == std::wstring::npos ? 0U : slash + 1U));
    const std::wstring extension = dot == std::wstring::npos
        ? L".png"
        : selected_path.substr(dot);
    for (std::size_t index = 0; index < files.size(); ++index) {
        std::wstring name;
        const int required = MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            reinterpret_cast<const char*>(files[index].name.data()),
            static_cast<int>(files[index].name.size()),
            nullptr,
            0);
        if (required <= 0) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        try {
            name.resize(static_cast<std::size_t>(required));
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            reinterpret_cast<const char*>(files[index].name.data()),
            static_cast<int>(files[index].name.size()),
            name.data(),
            required);
        const std::size_t name_slash = name.find_last_of(L"\\/");
        if (name_slash != std::wstring::npos) {
            name.erase(0, name_slash + 1U);
        }
        const std::size_t name_dot = name.find_last_of(L'.');
        if (name_dot != std::wstring::npos) {
            name.erase(name_dot);
        }
        std::wstring output;
        try {
            output = directory + stem + L"-" + name + extension;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        if (!WriteFileAtomically(output, files[index].bytes)) {
            return INKPOD_STATUS_IO_ERROR;
        }
    }
    return INKPOD_STATUS_OK;
}

InkpodStatus SaveToPath(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::wstring old_recovery_path;
    std::wstring next_current_path;
    std::wstring next_recovery_path;
    try {
        old_recovery_path = state.recovery_path;
        next_current_path = path;
        next_recovery_path = path + L".recovery.inkpod";
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_save(core, utf8.data(), utf8.size(), &info);
        },
        false,
        true);
    if (status == INKPOD_STATUS_OK) {
        state.current_path = std::move(next_current_path);
        state.recovery_path = std::move(next_recovery_path);
        if (!old_recovery_path.empty() && old_recovery_path != path) {
            DeleteFileW(old_recovery_path.c_str());
        }
        UpdateMenuState(state);
    }
    return status;
}

InkpodStatus SaveDocument(AppState& state, bool force_dialog) noexcept {
    std::wstring path = state.current_path;
    if (force_dialog || path.empty()) {
        if (!ChooseInkpodPath(state.window, true, path)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    return SaveToPath(state, path);
}

InkpodStatus OpenFromPath(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_open(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    try {
        state.current_path = path;
        state.recovery_path = path + L".recovery.inkpod";
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.plane = INKPOD_PLANE_MAIN_LINE;
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetM3DocumentUiState(state);
    const InkpodPlaneKind plane = state.plane;
    const InkpodStatus plane_status = state.engine->Invoke(
        [plane](InkpodCore* core) { return inkpod_core_set_active_plane(core, plane); },
        false,
        false);
    if (plane_status != INKPOD_STATUS_OK) {
        return plane_status;
    }
    const InkpodStatus view_status = FitCanvas(state, INKPOD_VIEW_FIT);
    UpdateMenuState(state);
    return view_status;
}

InkpodStatus OpenRecoveryFromPath(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (!WidePathToUtf8(path, utf8) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodStatus status = state.engine->Invoke(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_open_recovery(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.current_path.clear();
    try {
        state.recovery_path = path;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.plane = INKPOD_PLANE_MAIN_LINE;
    state.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetM3DocumentUiState(state);
    const InkpodStatus plane_status = state.engine->Invoke(
        [](InkpodCore* core) {
            return inkpod_core_set_active_plane(core, INKPOD_PLANE_MAIN_LINE);
        },
        false,
        false);
    if (plane_status != INKPOD_STATUS_OK) {
        return plane_status;
    }
    const InkpodStatus view_status = FitCanvas(state, INKPOD_VIEW_FIT);
    UpdateMenuState(state);
    return view_status;
}

bool RecoveryIsNewer(
    const std::wstring& normal_path, const std::wstring& recovery_path) noexcept {
    WIN32_FILE_ATTRIBUTE_DATA recovery{};
    if (GetFileAttributesExW(
            recovery_path.c_str(), GetFileExInfoStandard, &recovery) == FALSE) {
        return false;
    }
    WIN32_FILE_ATTRIBUTE_DATA normal{};
    if (GetFileAttributesExW(normal_path.c_str(), GetFileExInfoStandard, &normal) == FALSE) {
        return true;
    }
    return CompareFileTime(&recovery.ftLastWriteTime, &normal.ftLastWriteTime) > 0;
}

bool QueueAutosave(AppState& state, const std::wstring& path) noexcept {
    std::vector<std::uint8_t> utf8;
    if (state.engine == nullptr || !WidePathToUtf8(path, utf8)) {
        return false;
    }
    return state.engine->Enqueue(
        [utf8](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_autosave(core, utf8.data(), utf8.size(), &info);
        },
        false,
        false,
        true);
}

InkpodStatus ApplyFillAtDeviceRange(
    AppState& state,
    float device_x,
    float device_y,
    float end_device_x,
    float end_device_y,
    bool has_range) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto to_document_x = [&](double value) {
        double result = (value - bounds.left) / zoom;
        if (state.view_flip_horizontal) {
            result = static_cast<double>(info.width) - result;
        }
        return result;
    };
    auto to_document_y = [&](double value) {
        double result = (value - bounds.top) / zoom;
        if (state.view_flip_vertical) {
            result = static_cast<double>(info.height) - result;
        }
        return result;
    };
    const double document_x = to_document_x(device_x);
    const double document_y = to_document_y(device_y);
    if (!std::isfinite(document_x) || !std::isfinite(document_y) || document_x < 0.0
        || document_y < 0.0 || document_x >= static_cast<double>(info.width)
        || document_y >= static_cast<double>(info.height)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodFillInput input{};
    input.struct_size = sizeof(input);
    input.operation = state.fill_options.operation;
    input.flags = (state.fill_options.overflow_abort ? INKPOD_FILL_FLAG_OVERFLOW_ABORT : 0U)
        | (state.fill_options.detached_regions ? INKPOD_FILL_FLAG_DETACHED_REGIONS : 0U)
        | (state.fill_options.transparent_only ? INKPOD_FILL_FLAG_TRANSPARENT_ONLY : 0U)
        | (state.fill_options.use_document_selection
                  ? INKPOD_FILL_FLAG_DOCUMENT_SELECTION
                  : 0U)
        | (state.fill_options.light_table_boundary
                  ? INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY
                  : 0U)
        | (state.fill_options.light_table_color ? INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR : 0U);
    input.seed_x = static_cast<std::uint32_t>(std::floor(document_x));
    input.seed_y = static_cast<std::uint32_t>(std::floor(document_y));
    input.color = state.drawing_color;
    input.color.struct_size = sizeof(InkpodColorValue);
    input.tolerance = state.fill_options.tolerance;
    input.gap_close = state.fill_options.gap_close;
    input.inclusion_mode = state.fill_options.inclusion_mode;
    input.extension_distance = state.fill_options.extension_distance;
    input.inclusion_color_stride_bytes = sizeof(InkpodColorValue);
    if (input.operation != INKPOD_FILL_SEED) {
        if (!has_range) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const double end_x = to_document_x(end_device_x);
        const double end_y = to_document_y(end_device_y);
        const auto left = static_cast<std::int32_t>(std::floor(std::min(document_x, end_x)));
        const auto top = static_cast<std::int32_t>(std::floor(std::min(document_y, end_y)));
        const auto right = static_cast<std::int32_t>(std::ceil(std::max(document_x, end_x)));
        const auto bottom = static_cast<std::int32_t>(std::ceil(std::max(document_y, end_y)));
        if (right <= left || bottom <= top) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        input.flags |= INKPOD_FILL_FLAG_SELECTION_PRESENT;
        input.selection = {left, top, right - left, bottom - top};
        input.seed_x = static_cast<std::uint32_t>(std::clamp(
            (document_x + end_x) / 2.0,
            0.0,
            static_cast<double>(info.width - 1U)));
        input.seed_y = static_cast<std::uint32_t>(std::clamp(
            (document_y + end_y) / 2.0,
            0.0,
            static_cast<double>(info.height - 1U)));
    }
    std::vector<InkpodColorValue> inclusion_colors;
    try {
        inclusion_colors.reserve(state.fill_options.inclusion_rgba.size());
        for (const std::uint32_t rgba : state.fill_options.inclusion_rgba) {
            inclusion_colors.push_back(InkpodColorValue{
                sizeof(InkpodColorValue),
                INKPOD_COLOR_DEPTH_8,
                static_cast<std::uint16_t>((rgba >> 24) & 0xffU),
                static_cast<std::uint16_t>((rgba >> 16) & 0xffU),
                static_cast<std::uint16_t>((rgba >> 8) & 0xffU),
                static_cast<std::uint16_t>(rgba & 0xffU)});
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    input.inclusion_colors = inclusion_colors.empty() ? nullptr : inclusion_colors.data();
    input.inclusion_color_count = inclusion_colors.size();
    InkpodFillResult fill_result{};
    fill_result.struct_size = sizeof(fill_result);
    const InkpodStatus status = state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [input, inclusion_colors, &fill_result](InkpodCore* core) mutable {
                  input.inclusion_colors = inclusion_colors.empty()
                      ? nullptr
                      : inclusion_colors.data();
                  return inkpod_core_apply_fill(core, &input, &fill_result);
              },
              true,
              true);
    if (status == INKPOD_STATUS_OK) {
        state.plane = INKPOD_PLANE_COLOR;
    }
    if (status == INKPOD_STATUS_FILL_OVERFLOW && !state.smoke_test
        && (fill_result.flags & INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE) != 0U) {
        std::array<wchar_t, 160U> message{};
        _snwprintf_s(
            message.data(),
            message.size(),
            _TRUNCATE,
            L"塗りあふれを中断しました。候補座標: (%u, %u)",
            fill_result.leak_x,
            fill_result.leak_y);
        MessageBoxW(state.window, message.data(), L"inkpod", MB_OK | MB_ICONWARNING);
    }
    return status;
}

InkpodStatus ApplyFillAtDevicePoint(
    AppState& state, float device_x, float device_y) noexcept {
    return ApplyFillAtDeviceRange(
        state, device_x, device_y, device_x, device_y, false);
}

InkpodStatus ApplySelectionGesture(
    AppState& state, const std::vector<InkpodStrokeSample>& samples) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (samples.empty() || !QueryDocument(state, info)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto document_point = [&](const InkpodStrokeSample& sample) {
        double x = (static_cast<double>(sample.x) - bounds.left) / zoom;
        double y = (static_cast<double>(sample.y) - bounds.top) / zoom;
        if (state.view_flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.view_flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return InkpodSelectionPoint{
            sizeof(InkpodSelectionPoint),
            0U,
            static_cast<float>(std::clamp(x, 0.0, static_cast<double>(info.width))),
            static_cast<float>(std::clamp(y, 0.0, static_cast<double>(info.height)))};
    };
    std::vector<InkpodSelectionPoint> points;
    try {
        if (state.selection_shape == INKPOD_SELECTION_LASSO
            || state.selection_shape == INKPOD_SELECTION_POLYLINE
            || state.selection_shape == INKPOD_SELECTION_TRACE) {
            points.reserve(samples.size());
            for (const auto& sample : samples) {
                points.push_back(document_point(sample));
            }
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodSelectionInput input{};
    input.struct_size = sizeof(input);
    input.shape = state.selection_shape;
    input.operation = state.selection_operation;
    input.tolerance = state.selection_tolerance;
    input.gap_close = state.selection_gap_close;
    input.diameter = state.selection_diameter;
    if (state.selection_shape == INKPOD_SELECTION_RECTANGLE
        || state.selection_shape == INKPOD_SELECTION_ELLIPSE) {
        if (samples.size() < 2U) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        const auto first = document_point(samples.front());
        const auto last = document_point(samples.back());
        const auto left = static_cast<std::int32_t>(std::floor(std::min(first.x, last.x)));
        const auto top = static_cast<std::int32_t>(std::floor(std::min(first.y, last.y)));
        const auto right = static_cast<std::int32_t>(std::ceil(std::max(first.x, last.x)));
        const auto bottom = static_cast<std::int32_t>(std::ceil(std::max(first.y, last.y)));
        input.bounds = {left, top, right - left, bottom - top};
    } else if (state.selection_shape == INKPOD_SELECTION_WAND) {
        const auto point = document_point(samples.front());
        input.seed_x = static_cast<std::uint32_t>(std::clamp(
            std::floor(static_cast<double>(point.x)),
            0.0,
            static_cast<double>(info.width - 1U)));
        input.seed_y = static_cast<std::uint32_t>(std::clamp(
            std::floor(static_cast<double>(point.y)),
            0.0,
            static_cast<double>(info.height - 1U)));
    } else {
        input.points = points.data();
        input.point_count = points.size();
        input.point_stride_bytes = sizeof(InkpodSelectionPoint);
    }
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [input, points](InkpodCore* core) mutable {
                  if (!points.empty()) {
                      input.points = points.data();
                  }
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_apply_selection(core, &input, &result);
              },
              true,
              true);
}

InkpodStatus SelectDrawingColor(
    AppState& state, bool different, InkpodSelectionOperation operation) noexcept {
    InkpodColorValue color{};
    color.struct_size = sizeof(color);
    if (state.plane == INKPOD_PLANE_MAIN_LINE) {
        color.depth = INKPOD_COLOR_DEPTH_BINARY;
        color.red = state.color_rgba == 0U ? 0U : UINT8_MAX;
    } else {
        color = state.drawing_color;
        color.struct_size = sizeof(InkpodColorValue);
    }
    return state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [color, different, operation](InkpodCore* core) {
                  InkpodDispatchResult result{};
                  result.struct_size = sizeof(result);
                  return inkpod_core_select_color(
                      core,
                      &color,
                      0U,
                      different ? 1U : 0U,
                      operation,
                      &result);
              },
              true,
              true);
}

InkpodStatus EyedropAtDevicePoint(AppState& state, float device_x, float device_y) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    const double document_x = (static_cast<double>(device_x) - bounds.left) / zoom;
    const double document_y = (static_cast<double>(device_y) - bounds.top) / zoom;
    if (!std::isfinite(document_x) || !std::isfinite(document_y) || document_x < 0.0
        || document_y < 0.0 || document_x >= static_cast<double>(info.width)
        || document_y >= static_cast<double>(info.height)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodColorValue sampled{};
    sampled.struct_size = sizeof(sampled);
    const auto x = static_cast<std::uint32_t>(std::floor(document_x));
    const auto y = static_cast<std::uint32_t>(std::floor(document_y));
    const InkpodStatus status = state.engine == nullptr
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              [x, y, source = state.eyedropper_source, &sampled](InkpodCore* core) {
                  return inkpod_core_eyedropper(
                      core, source, x, y, &sampled);
              },
              false,
              false);
    if (status == INKPOD_STATUS_OK) {
        SetDrawingColor(state, sampled);
    }
    return status;
}

bool DiscardCurrentRecovery(AppState& state) noexcept {
    if (state.recovery_path.empty()) {
        return true;
    }
    if (state.engine != nullptr && state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return false;
    }
    if (DeleteFileW(state.recovery_path.c_str()) == FALSE
        && GetLastError() != ERROR_FILE_NOT_FOUND) {
        return false;
    }
    state.recovery_path.clear();
    return true;
}

bool ConfirmDiscard(AppState& state) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)) {
        return true;
    }
    if ((info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        return true;
    }
    const int choice = MessageBoxW(
        state.window,
        L"変更を保存しますか？",
        L"inkpod",
        MB_YESNOCANCEL | MB_ICONQUESTION);
    if (choice == IDCANCEL) {
        return false;
    }
    if (choice == IDYES) {
        const InkpodStatus status = SaveDocument(state, false);
        if (status != INKPOD_STATUS_OK) {
            if (status != INKPOD_STATUS_INVALID_STATE) {
                ShowCoreError(state, state.window, L"保存");
            }
            return false;
        }
    } else if (!DiscardCurrentRecovery(state)) {
        ShowCoreError(state, state.window, L"Recoveryの破棄");
        return false;
    }
    return true;
}

bool SameFrame(const InkpodFrameRect& left, const InkpodFrameRect& right) noexcept {
    return left.x == right.x && left.y == right.y && left.width == right.width
        && left.height == right.height;
}

bool SamePersistentMetadata(
    const InkpodDocumentInfo& left, const InkpodDocumentInfo& right) noexcept {
    return left.document_id == right.document_id && left.layer_id == right.layer_id
        && left.document_uuid_high == right.document_uuid_high
        && left.document_uuid_low == right.document_uuid_low
        && left.main_plane_id == right.main_plane_id
        && left.color_plane_id == right.color_plane_id && left.width == right.width
        && left.height == right.height && left.dpi_x_milli == right.dpi_x_milli
        && left.dpi_y_milli == right.dpi_y_milli
        && SameFrame(left.hundred_frame, right.hundred_frame)
        && SameFrame(left.reference_frame, right.reference_frame)
        && SameFrame(left.drawing_frame, right.drawing_frame)
        && SameFrame(left.safe_frame, right.safe_frame)
        && left.margin_left == right.margin_left && left.margin_top == right.margin_top
        && left.margin_right == right.margin_right && left.margin_bottom == right.margin_bottom
        && left.main_plane_checksum == right.main_plane_checksum
        && left.color_plane_checksum == right.color_plane_checksum;
}

void PumpPendingWindowMessages() noexcept {
    MSG message{};
    while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE) != FALSE) {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

int RunM1Smoke(AppState& state) noexcept {
    const HMENU menu = GetMenu(state.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_HELP_ABOUT, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || SendMessageW(state.window, WM_COMMAND, IDM_HELP_ABOUT, 0) != 1) {
        return 29;
    }
    if (state.engine == nullptr
        || MoveWindow(state.canvas, 0, 0, 640, 480, FALSE) == FALSE
        || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 30;
    }
    PumpPendingWindowMessages();
    UpdateMenuState(state);
    constexpr std::array<UINT, 6U> vector_draw_commands{
        IDM_VECTOR_LINE,
        IDM_VECTOR_CURVE,
        IDM_VECTOR_RECTANGLE,
        IDM_VECTOR_ELLIPSE,
        IDM_VECTOR_POLYLINE,
        IDM_VECTOR_ERASER};
    for (const UINT command : vector_draw_commands) {
        const UINT command_state = GetMenuState(menu, command, MF_BYCOMMAND);
        if (command_state == static_cast<UINT>(-1)
            || (command_state & (MF_DISABLED | MF_GRAYED)) == 0U) {
            return 701;
        }
    }
    if (!RefreshSequencePane(state)
        || SendMessageW(state.sequence_list, LB_GETCOUNT, 0, 0) != 0) {
        return 702;
    }
    InkpodSequenceCellInfo missing_sequence_cell{};
    missing_sequence_cell.struct_size = sizeof(missing_sequence_cell);
    const InkpodStatus missing_sequence_status = state.engine->Invoke(
        [&missing_sequence_cell](InkpodCore* core) {
            return inkpod_core_sequence_cell_get(core, 0U, &missing_sequence_cell);
        },
        false,
        false);
    if (missing_sequence_status != INKPOD_STATUS_INVALID_STATE
        || state.engine->LastError().find(L"no sequence is configured")
            == std::wstring::npos) {
        return 703;
    }
    if (FinishVectorCanvasGesture(state) != INKPOD_STATUS_INVALID_STATE
        || state.engine->LastError() != kVectorStrokePlaneRequired) {
        return 704;
    }
    const std::uint32_t initial_tool = state.tool;
    for (const UINT command : vector_draw_commands) {
        if (SendMessageW(state.window, WM_COMMAND, command, 0) != 0
            || state.tool != initial_tool) {
            return 705;
        }
    }
    const std::wstring initial_recovery_path = state.recovery_path;
    std::wstring discovered_recovery;
    if (initial_recovery_path.empty()
        || !QueueAutosave(state, initial_recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(initial_recovery_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || !NewestPrivateRecovery(discovered_recovery)
        || _wcsicmp(discovered_recovery.c_str(), initial_recovery_path.c_str()) != 0) {
        return 215;
    }
    std::wstring active_stroke_recovery_path;
    try {
        active_stroke_recovery_path = initial_recovery_path + L".active-stroke-test";
    } catch (const std::bad_alloc&) {
        return 217;
    }
    if (DeleteFileW(active_stroke_recovery_path.c_str()) == FALSE
        && GetLastError() != ERROR_FILE_NOT_FOUND) {
        return 218;
    }
    PumpPendingWindowMessages();
    const DWORD ui_thread = GetCurrentThreadId();
    const DWORD core_thread = state.engine->ThreadId();
    const DWORD renderer_thread = static_cast<DWORD>(SendMessageW(
        state.canvas, inkpod::renderer::kCanvasGetRendererThreadId, 0, 0));
    if (core_thread == 0U || renderer_thread == 0U || core_thread == ui_thread
        || renderer_thread == ui_thread || core_thread == renderer_thread) {
        return 31;
    }
    inkpod::renderer::CanvasDocumentBounds document_bounds{};
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasGetDocumentBounds,
            0,
            reinterpret_cast<LPARAM>(&document_bounds))
            != 1
        || std::abs(document_bounds.left - 16.0) > 0.01
        || std::abs(document_bounds.top - 69.0) > 0.01
        || std::abs(document_bounds.right - 624.0) > 0.01
        || std::abs(document_bounds.bottom - 411.0) > 0.01) {
        return 53;
    }

    InkpodDocumentInfo before_line{};
    std::array<wchar_t, 128> initial_title{};
    if (!QueryDocument(state, before_line)
        || (before_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        || GetWindowTextW(
               state.window, initial_title.data(), static_cast<int>(initial_title.size())) == 0
        || std::wcscmp(initial_title.data(), L"無題 - inkpod") != 0) {
        return 32;
    }
    const auto frames_before = static_cast<std::uint64_t>(SendMessageW(
        state.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    SendMessageW(state.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 100));
    for (int x = 90; x <= 240; x += 15) {
        SendMessageW(state.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 120));
    }
    if (state.engine->FlushPreview() != INKPOD_STATUS_OK) {
        return 33;
    }
    if (!QueueAutosave(state, active_stroke_recovery_path)) {
        return 219;
    }
    PumpPendingWindowMessages();
    if (SendMessageW(state.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 34;
    }
    InkpodDocumentInfo during_line{};
    const auto frames_during = static_cast<std::uint64_t>(SendMessageW(
        state.canvas, inkpod::renderer::kCanvasGetPresentedFrameCount, 0, 0));
    if (!QueryDocument(state, during_line)) {
        return 130;
    }
    if (during_line.document_revision != before_line.document_revision) {
        return 131;
    }
    if (during_line.main_plane_checksum != before_line.main_plane_checksum) {
        return 132;
    }
    if ((during_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)
        != (before_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY)) {
        return 133;
    }
    if (frames_during <= frames_before) {
        return 134;
    }
    if (SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(250, 120)) != 1) {
        return 36;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 37;
    }
    if (GetFileAttributesW(active_stroke_recovery_path.c_str())
        == INVALID_FILE_ATTRIBUTES) {
        return 220;
    }
    DeleteFileW(active_stroke_recovery_path.c_str());
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_line{};
    if (!QueryDocument(state, after_line)
        || after_line.document_revision != before_line.document_revision + 1U
        || after_line.main_plane_checksum == before_line.main_plane_checksum
        || (after_line.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U
        || (after_line.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 38;
    }
    const std::uint64_t line_checksum = after_line.main_plane_checksum;

    SendMessageW(state.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(80, 150));
    SendMessageW(state.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(180, 150));
    SendMessageW(state.canvas, WM_CAPTURECHANGED, 0, 0);
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 54;
    }
    InkpodDocumentInfo after_cancel{};
    if (!QueryDocument(state, after_cancel)
        || after_cancel.document_revision != after_line.document_revision
        || after_cancel.main_plane_checksum != after_line.main_plane_checksum) {
        return 55;
    }

    state.plane = INKPOD_PLANE_COLOR;
    if (state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR);
            },
            false,
            true)
        != INKPOD_STATUS_OK) {
        return 39;
    }
    SendMessageW(state.canvas, WM_LBUTTONDOWN, MK_LBUTTON, MAKELPARAM(100, 180));
    for (int x = 115; x <= 260; x += 15) {
        SendMessageW(state.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x, 190));
    }
    if (SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(270, 190)) != 1) {
        return 40;
    }
    if (state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return 41;
    }
    PumpPendingWindowMessages();
    InkpodDocumentInfo after_color{};
    if (!QueryDocument(state, after_color)
        || after_color.main_plane_checksum != line_checksum
        || after_color.color_plane_checksum == after_line.color_plane_checksum) {
        return 42;
    }
    const inkpod::app::EngineMetrics metrics = state.engine->Metrics();
    if (metrics.completed_strokes != 2U || metrics.completed_samples <= 2U
        || metrics.preview_snapshots == 0U) {
        return 43;
    }

    if (state.engine->Invoke(
            [](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_undo(core, &result);
            },
            true,
            true)
            != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [](InkpodCore* core) {
                   InkpodDispatchResult result{};
                   result.struct_size = sizeof(result);
                   return inkpod_core_redo(core, &result);
               },
               true,
               true)
            != INKPOD_STATUS_OK) {
        return 44;
    }
    InkpodDocumentInfo after_redo{};
    if (!QueryDocument(state, after_redo)
        || after_redo.color_plane_checksum != after_color.color_plane_checksum) {
        return 45;
    }

    const std::uint64_t revision_before_view = after_redo.document_revision;
    SendMessageW(state.canvas, WM_MBUTTONDOWN, MK_MBUTTON, MAKELPARAM(300, 220));
    SendMessageW(state.canvas, WM_MOUSEMOVE, MK_MBUTTON, MAKELPARAM(320, 230));
    SendMessageW(state.canvas, WM_MBUTTONUP, 0, MAKELPARAM(320, 230));
    RECT canvas_bounds{};
    GetWindowRect(state.canvas, &canvas_bounds);
    SendMessageW(
        state.canvas,
        WM_MOUSEWHEEL,
        MAKEWPARAM(0, WHEEL_DELTA),
        MAKELPARAM(canvas_bounds.left + 320, canvas_bounds.top + 240));
    InkpodDocumentInfo after_view{};
    if (!QueryDocument(state, after_view)
        || after_view.document_revision != revision_before_view
        || after_view.view_revision == after_redo.view_revision) {
        return 46;
    }

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 47;
    }
    std::array<wchar_t, MAX_PATH> temporary_file{};
    _snwprintf_s(
        temporary_file.data(),
        temporary_file.size(),
        _TRUNCATE,
        L"%lsinkpod-m1-smoke-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(temporary_file.data());
    if (SaveToPath(state, path) != INKPOD_STATUS_OK) {
        return 48;
    }
    if (GetFileAttributesW(initial_recovery_path.c_str()) != INVALID_FILE_ATTRIBUTES
        || GetLastError() != ERROR_FILE_NOT_FOUND) {
        DeleteFileW(path.c_str());
        return 216;
    }
    InkpodDocumentInfo saved{};
    if (!QueryDocument(state, saved)
        || (saved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        DeleteFileW(path.c_str());
        return 49;
    }
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || OpenFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 50;
    }
    InkpodDocumentInfo reopened{};
    const bool round_trip = QueryDocument(state, reopened)
        && SamePersistentMetadata(saved, reopened)
        && (reopened.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U;
    if (!round_trip) {
        DeleteFileW(path.c_str());
        return 51;
    }
    const InkpodStrokeSample history_sample{
        sizeof(InkpodStrokeSample), 0U, 10.0F, 10.0F, 1.0F, 0U};
    const InkpodStrokeInput history_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x14c832ff),
        1.0F,
        &history_sample,
        1U,
        sizeof(history_sample)};
    InkpodSelectionInput history_selection{};
    history_selection.struct_size = sizeof(history_selection);
    history_selection.shape = INKPOD_SELECTION_RECTANGLE;
    history_selection.operation = INKPOD_SELECTION_NEW;
    history_selection.bounds = {10, 10, 1, 1};
    if (state.engine->Invoke(
            [history_stroke, history_selection](InkpodCore* core) {
                InkpodStatus status = inkpod_core_set_active_plane(
                    core, INKPOD_PLANE_COLOR);
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_apply_stroke(core, &history_stroke, &result);
                }
                if (status == INKPOD_STATUS_OK) {
                    status = inkpod_core_apply_selection(
                        core, &history_selection, &result);
                }
                return status;
            },
            true,
            true) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 217;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_FILE_REVERT_PARTIAL, 0);
    InkpodDocumentInfo partially_reverted{};
    if (!QueryDocument(state, partially_reverted)
        || partially_reverted.color_plane_checksum != reopened.color_plane_checksum) {
        DeleteFileW(path.c_str());
        return 241;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_HISTORY_BACK, 0);
    InkpodHistoryInfo smoke_history{};
    smoke_history.struct_size = sizeof(smoke_history);
    if (state.engine->Invoke(
            [&smoke_history](InkpodCore* core) {
                return inkpod_core_history_info(core, &smoke_history);
            },
            false,
            false) != INKPOD_STATUS_OK
        || smoke_history.cursor != 0U || smoke_history.item_count < 3U) {
        DeleteFileW(path.c_str());
        return 219;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_HISTORY_FORWARD, 0);
    if (state.engine->Invoke(
            [&smoke_history](InkpodCore* core) {
                return inkpod_core_history_info(core, &smoke_history);
            },
            false,
            false) != INKPOD_STATUS_OK
        || smoke_history.cursor != smoke_history.item_count
        || state.engine->Invoke(
               [](InkpodCore* core) {
                   return inkpod_core_set_active_plane(
                       core, INKPOD_PLANE_MAIN_LINE);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 220;
    }
    state.plane = INKPOD_PLANE_MAIN_LINE;
    DeleteFileW(path.c_str());

    inkpod::renderer::CanvasDocumentBounds before_dpi_bounds{};
    inkpod::renderer::CanvasDocumentBounds after_dpi_bounds{};
    const bool bounds_before_dpi = SendMessageW(
                                       state.canvas,
                                       inkpod::renderer::kCanvasGetDocumentBounds,
                                       0,
                                       reinterpret_cast<LPARAM>(&before_dpi_bounds)) == 1;
    const bool dpi_changed = SendMessageW(
                                 state.canvas,
                                 WM_DPICHANGED_AFTERPARENT,
                                 0,
                                 0) == 1;
    const bool bounds_after_dpi = SendMessageW(
                                      state.canvas,
                                      inkpod::renderer::kCanvasGetDocumentBounds,
                                      0,
                                      reinterpret_cast<LPARAM>(&after_dpi_bounds)) == 1;
    const bool dpi_transform_stable = bounds_before_dpi && bounds_after_dpi
        && std::abs(before_dpi_bounds.left - after_dpi_bounds.left) <= 0.01
        && std::abs(before_dpi_bounds.top - after_dpi_bounds.top) <= 0.01
        && std::abs(before_dpi_bounds.right - after_dpi_bounds.right) <= 0.01
        && std::abs(before_dpi_bounds.bottom - after_dpi_bounds.bottom) <= 0.01;
    const bool device_recovered = SendMessageW(
                                      state.canvas,
                                      inkpod::renderer::kCanvasSimulateDeviceLoss,
                                      0,
                                      0) == 1;
    const bool rendered = SendMessageW(
                              state.canvas,
                              inkpod::renderer::kCanvasRenderOnce,
                              0,
                              0) == 1;
    return dpi_changed && dpi_transform_stable && device_recovered && rendered ? 0 : 52;
}

int RunM2Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 200;
    }
    const HMENU menu = GetMenu(state.window);
    if (menu == nullptr
        || GetMenuState(menu, IDM_TOOL_FILL, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_TOOL_EYEDROPPER, MF_BYCOMMAND) == static_cast<UINT>(-1)
        || GetMenuState(menu, IDM_COLOR_CHECK_NATIVE, MF_BYCOMMAND)
            == static_cast<UINT>(-1)) {
        return 201;
    }

    std::array<InkpodStrokeSample, 5> boundary_samples{{
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 100.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 200.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 200.0F, 1.0F, 0U},
        {sizeof(InkpodStrokeSample), 0U, 100.0F, 100.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput boundary{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        boundary_samples.data(),
        boundary_samples.size(),
        sizeof(InkpodStrokeSample)};
    if (state.engine->Invoke(
            [boundary](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &boundary, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 202;
    }
    InkpodDocumentInfo before_fill{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, before_fill)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1) {
        return 203;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(before_fill.width);
    const int fill_x = static_cast<int>(std::lround(bounds.left + 150.0 * zoom));
    const int fill_y = static_cast<int>(std::lround(bounds.top + 150.0 * zoom));
    SendMessageW(state.window, WM_COMMAND, IDM_TOOL_FILL, 0);
    if (SendMessageW(
            state.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1) {
        return 204;
    }
    SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));
    InkpodDocumentInfo after_fill{};
    if (!QueryDocument(state, after_fill)
        || after_fill.document_revision != before_fill.document_revision + 1U
        || after_fill.main_plane_checksum != before_fill.main_plane_checksum
        || after_fill.color_plane_checksum == before_fill.color_plane_checksum) {
        return 205;
    }

    const std::uint32_t fill_color = state.color_rgba;
    state.color_rgba = UINT32_C(0x010203ff);
    SendMessageW(state.window, WM_COMMAND, IDM_TOOL_EYEDROPPER, 0);
    if (SendMessageW(
            state.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(fill_x, fill_y)) != 1
        || state.color_rgba != fill_color) {
        return 206;
    }
    SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(fill_x, fill_y));

    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    state.color_rgba = fill_color;
    state.fill_options.operation = INKPOD_FILL_CLOSED_REGION;
    state.fill_options.tolerance = 257U;
    state.fill_options.gap_close = 1U;
    state.fill_options.extension_distance = 2U;
    state.fill_options.detached_regions = true;
    state.fill_options.overflow_abort = true;
    if (SendMessageW(state.window, WM_COMMAND, IDM_TOOL_FILL_OPTIONS, 0) != 0
        || state.fill_options.operation != INKPOD_FILL_CLOSED_REGION) {
        return 221;
    }
    const auto device_x = [&](double document_x) {
        return static_cast<int>(std::lround(bounds.left + document_x * zoom));
    };
    const auto device_y = [&](double document_y) {
        return static_cast<int>(std::lround(bounds.top + document_y * zoom));
    };
    const auto canvas_drag = [&state](int x1, int y1, int x2, int y2) {
        if (SendMessageW(
                state.canvas,
                WM_LBUTTONDOWN,
                MK_LBUTTON,
                MAKELPARAM(x1, y1)) != 1) {
            return false;
        }
        SendMessageW(
            state.canvas, WM_MOUSEMOVE, MK_LBUTTON, MAKELPARAM(x2, y2));
        SendMessageW(state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(x2, y2));
        return true;
    };
    InkpodDocumentInfo before_closed{};
    if (!QueryDocument(state, before_closed)
        || !canvas_drag(
            device_x(90.0),
            device_y(90.0),
            device_x(210.0),
            device_y(210.0))) {
        return 222;
    }
    InkpodDocumentInfo after_closed{};
    if (!QueryDocument(state, after_closed)
        || after_closed.document_revision != before_closed.document_revision + 1U
        || after_closed.main_plane_checksum != before_closed.main_plane_checksum
        || after_closed.color_plane_checksum == before_closed.color_plane_checksum) {
        return 223;
    }

    const int extension_left = device_x(246.0);
    const int extension_top = device_y(246.0);
    const int extension_right = device_x(254.0);
    const int extension_bottom = device_y(254.0);
    const auto extension_seed_x = static_cast<float>(std::floor(
        ((static_cast<double>(extension_left) + static_cast<double>(extension_right)) / 2.0
            - bounds.left)
        / zoom));
    const auto extension_seed_y = static_cast<float>(std::floor(
        ((static_cast<double>(extension_top) + static_cast<double>(extension_bottom)) / 2.0
            - bounds.top)
        / zoom));
    const InkpodStrokeSample extension_source{
        sizeof(InkpodStrokeSample),
        0U,
        extension_seed_x,
        extension_seed_y,
        1.0F,
        0U};
    const InkpodStrokeInput extension_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x28b45aff),
        1.0F,
        &extension_source,
        1U,
        sizeof(extension_source)};
    if (state.engine->Invoke(
            [extension_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &extension_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 224;
    }
    InkpodDocumentInfo before_extension{};
    QueryDocument(state, before_extension);
    state.fill_options.operation = INKPOD_FILL_EXTENSION;
    state.fill_options.extension_distance = 3U;
    state.fill_options.detached_regions = false;
    SendMessageW(state.window, WM_COMMAND, IDM_TOOL_FILL_OPTIONS, 0);
    if (!canvas_drag(
            extension_left,
            extension_top,
            extension_right,
            extension_bottom)) {
        return 225;
    }
    InkpodDocumentInfo after_extension{};
    if (!QueryDocument(state, after_extension)
        || after_extension.color_plane_checksum == before_extension.color_plane_checksum) {
        return 226;
    }

    InkpodSelectionInput persistent_selection{};
    persistent_selection.struct_size = sizeof(persistent_selection);
    persistent_selection.shape = INKPOD_SELECTION_ELLIPSE;
    persistent_selection.operation = INKPOD_SELECTION_NEW;
    persistent_selection.bounds = {300, 300, 8, 8};
    if (state.engine->Invoke(
            [persistent_selection](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_selection(
                    core, &persistent_selection, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 227;
    }
    state.fill_options.operation = INKPOD_FILL_SEED;
    state.fill_options.use_document_selection = true;
    state.fill_options.overflow_abort = false;
    state.fill_options.gap_close = 0U;
    SendMessageW(state.window, WM_COMMAND, IDM_TOOL_FILL_OPTIONS, 0);
    const int selected_x = device_x(304.0);
    const int selected_y = device_y(304.0);
    if (SendMessageW(
            state.canvas,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(selected_x, selected_y)) != 1) {
        return 228;
    }
    SendMessageW(
        state.canvas, WM_LBUTTONUP, 0, MAKELPARAM(selected_x, selected_y));
    InkpodColorValue selected_fill{};
    selected_fill.struct_size = sizeof(selected_fill);
    InkpodColorValue outside_fill{};
    outside_fill.struct_size = sizeof(outside_fill);
    InkpodStatus outside_fill_status = INKPOD_STATUS_OK;
    if (state.engine->Invoke(
            [&selected_fill, &outside_fill, &outside_fill_status](InkpodCore* core) {
                InkpodStatus status = inkpod_core_eyedropper(
                    core,
                    INKPOD_EYEDROPPER_SELECTED_PLANE,
                    304U,
                    304U,
                    &selected_fill);
                if (status == INKPOD_STATUS_OK) {
                    outside_fill_status = inkpod_core_eyedropper(
                        core,
                        INKPOD_EYEDROPPER_SELECTED_PLANE,
                        295U,
                        295U,
                        &outside_fill);
                }
                return status;
            },
            false,
            false) != INKPOD_STATUS_OK
        || selected_fill.alpha == 0U
        || outside_fill_status != INKPOD_STATUS_INVALID_STATE) {
        return 229;
    }
    state.fill_options = {};

    InkpodDocumentInfo before_check{};
    if (!QueryDocument(state, before_check)) {
        return 230;
    }
    const std::uint64_t revision_before_check = before_check.document_revision;
    const std::uint64_t view_before_check = before_check.view_revision;
    SendMessageW(state.window, WM_COMMAND, IDM_COLOR_CHECK_NATIVE, 0);
    InkpodDocumentInfo during_check{};
    std::uint64_t check_features{};
    const InkpodStatus check_snapshot_status = state.engine->Invoke(
        [&check_features](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            InkpodSnapshotView view{};
            view.struct_size = sizeof(view);
            status = inkpod_snapshot_get_view(snapshot, &view);
            if (status == INKPOD_STATUS_OK) {
                check_features = view.feature_flags;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, during_check)
        || check_snapshot_status != INKPOD_STATUS_OK
        || check_features != INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA
        || during_check.document_revision != revision_before_check
        || during_check.view_revision <= view_before_check
        || SendMessageW(state.canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) {
        return 207;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_COLOR_CHECK_OFF, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 208;
    }
    const auto suffix = static_cast<unsigned long long>(GetTickCount64());
    std::array<wchar_t, MAX_PATH> normal_buffer{};
    std::array<wchar_t, MAX_PATH> recovery_buffer{};
    _snwprintf_s(
        normal_buffer.data(),
        normal_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-m2-normal-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    _snwprintf_s(
        recovery_buffer.data(),
        recovery_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-m2-recovery-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        suffix);
    const std::wstring normal_path(normal_buffer.data());
    const std::wstring recovery_path(recovery_buffer.data());
    if (SaveToPath(state, normal_path) != INKPOD_STATUS_OK) {
        return 209;
    }
    InkpodDocumentInfo normally_saved{};
    if (!QueryDocument(state, normally_saved)) {
        return 210;
    }
    std::array<InkpodStrokeSample, 1> edit_sample{{
        {sizeof(InkpodStrokeSample), 0U, 300.0F, 300.0F, 1.0F, 0U},
    }};
    const InkpodStrokeInput edit{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_COLOR,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x010203ff),
        1.0F,
        edit_sample.data(),
        edit_sample.size(),
        sizeof(InkpodStrokeSample)};
    if (state.engine->Invoke(
            [edit](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &edit, &result);
            },
            true,
            true) != INKPOD_STATUS_OK
        || !QueueAutosave(state, recovery_path)
        || state.engine->WaitIdle() != INKPOD_STATUS_OK
        || GetFileAttributesW(normal_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(recovery_path.c_str()) == INVALID_FILE_ATTRIBUTES) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 211;
    }
    InkpodDocumentInfo autosaved{};
    if (!QueryDocument(state, autosaved)
        || (autosaved.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 212;
    }
    if (CreateDefaultCell(state) != INKPOD_STATUS_OK
        || OpenRecoveryFromPath(state, recovery_path) != INKPOD_STATUS_OK) {
        DeleteFileW(normal_path.c_str());
        DeleteFileW(recovery_path.c_str());
        return 213;
    }
    InkpodDocumentInfo recovered{};
    const bool recovery_state = QueryDocument(state, recovered)
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U
        && (recovered.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        && recovered.color_plane_checksum == autosaved.color_plane_checksum
        && state.current_path.empty();
    const InkpodStatus revert_status = state.engine->Invoke(
        [](InkpodCore* core) {
            InkpodDocumentInfo info = EmptyDocumentInfo();
            return inkpod_core_revert(core, &info);
        },
        false,
        false);
    const bool normal_unchanged = OpenFromPath(state, normal_path) == INKPOD_STATUS_OK;
    InkpodDocumentInfo reopened_normal{};
    const bool normal_matches = QueryDocument(state, reopened_normal)
        && reopened_normal.color_plane_checksum == normally_saved.color_plane_checksum
        && reopened_normal.color_plane_checksum != recovered.color_plane_checksum;
    DeleteFileW(normal_path.c_str());
    DeleteFileW(recovery_path.c_str());
    return recovery_state && revert_status == INKPOD_STATUS_INVALID_STATE && normal_unchanged
            && normal_matches
        ? 0
        : 214;
}

int RunM3Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 300;
    }
    const HMENU menu = GetMenu(state.window);
    for (const UINT command : {
             IDM_EDIT_COPY,
             IDM_EDIT_PASTE,
             IDM_EDIT_MIRROR_HORIZONTAL,
             IDM_LAYER_DUPLICATE,
             IDM_LAYER_DELETE,
             IDM_LAYER_MOVE_TOP,
             IDM_SELECTION_RECTANGLE,
             IDM_SELECTION_ELLIPSE,
             IDM_SELECTION_LASSO,
             IDM_SELECTION_POLYLINE,
             IDM_SELECTION_TRACE,
             IDM_SELECTION_WAND,
             IDM_SELECTION_MODE_NEW,
             IDM_SELECTION_MODE_ADD,
             IDM_SELECTION_MODE_SUBTRACT,
             IDM_SELECTION_MODE_INTERSECT,
             IDM_SELECTION_CLEAR,
             IDM_SELECTION_COLOR,
             IDM_SELECTION_COLOR_DIFFERENT,
             IDM_SELECTION_COLOR_ADD,
             IDM_SELECTION_TO_LAYER,
             IDM_SELECTION_FROM_LAYER,
             IDM_SELECTION_LAYER_ADD,
             IDM_SELECTION_LAYER_SUBTRACT,
             IDM_SELECTION_ALL,
             IDM_SELECTION_INVERT,
             IDM_SELECTION_EXPAND,
             IDM_SELECTION_SHRINK,
             IDM_VIEW_FLIP_HORIZONTAL,
             IDM_VIEW_FLIP_VERTICAL,
             IDM_VIEW_GRID,
             IDM_VIEW_NEW,
             IDM_SHORTCUT_EDIT,
             IDM_SHORTCUT_RESET}) {
        if (menu == nullptr
            || GetMenuState(menu, command, MF_BYCOMMAND)
                == static_cast<UINT>(-1)) {
            return 301;
        }
    }

    const InkpodCellCreateOptions source_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000001),
        UINT64_C(0x4d33000000000002),
        8U,
        8U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [source_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &source_options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 302;
    }
    ResetM3DocumentUiState(state);
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 303;
    }

    InkpodDocumentInfo initial{};
    if (!QueryDocument(state, initial)) {
        return 304;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_DUPLICATE, 0);
    const std::uint64_t duplicate_id = state.m3_layer_id;
    InkpodDocumentInfo duplicated{};
    if (duplicate_id == 0U || !QueryDocument(state, duplicated)
        || duplicated.document_revision != initial.document_revision + 1U) {
        return 305;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_MOVE_TOP, 0);
    InkpodNodeInfo top_layer{};
    top_layer.struct_size = sizeof(top_layer);
    const InkpodStatus top_status = state.engine->Invoke(
        [&top_layer](InkpodCore* core) {
            return inkpod_core_node_get(
                core, 0U, UINT32_MAX, &top_layer);
        },
        false,
        false);
    if (top_status != INKPOD_STATUS_OK || top_layer.id != duplicate_id) {
        return 306;
    }

    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_DELETE, 0);
    InkpodDocumentInfo after_delete{};
    if (!QueryDocument(state, after_delete)
        || (after_delete.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 307;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id != duplicate_id) {
        return 308;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_REDO, 0);
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    if (state.engine->Invoke(
            [&top_layer](InkpodCore* core) {
                return inkpod_core_node_get(
                    core, 0U, UINT32_MAX, &top_layer);
            },
            false,
            false) != INKPOD_STATUS_OK
        || top_layer.id == duplicate_id) {
        return 309;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    std::array<wchar_t, MAX_PATH> temporary_directory{};
    if (GetTempPathW(
            static_cast<DWORD>(temporary_directory.size()),
            temporary_directory.data()) == 0U) {
        return 310;
    }
    std::array<wchar_t, MAX_PATH> file_buffer{};
    _snwprintf_s(
        file_buffer.data(),
        file_buffer.size(),
        _TRUNCATE,
        L"%lsinkpod-m3-%lu-%llu.inkpod",
        temporary_directory.data(),
        GetCurrentProcessId(),
        static_cast<unsigned long long>(GetTickCount64()));
    const std::wstring path(file_buffer.data());
    if (SaveToPath(state, path) != INKPOD_STATUS_OK
        || OpenFromPath(state, path) != INKPOD_STATUS_OK) {
        DeleteFileW(path.c_str());
        return 311;
    }
    top_layer = {};
    top_layer.struct_size = sizeof(top_layer);
    const bool reopened_tree = state.engine->Invoke(
                                   [&top_layer](InkpodCore* core) {
                                       return inkpod_core_node_get(
                                           core,
                                           0U,
                                           UINT32_MAX,
                                           &top_layer);
                                   },
                                   false,
                                   false) == INKPOD_STATUS_OK
        && top_layer.id == duplicate_id;
    DeleteFileW(path.c_str());
    if (!reopened_tree) {
        return 312;
    }

    InkpodDocumentInfo before_invalid{};
    if (!QueryDocument(state, before_invalid)) {
        return 313;
    }
    static constexpr std::array<std::uint8_t, 7> invalid_name{
        'I', 'n', 'v', 'a', 'l', 'i', 'd'};
    InkpodTreeEdit invalid_plane{};
    invalid_plane.struct_size = sizeof(invalid_plane);
    invalid_plane.operation = INKPOD_TREE_CREATE_PLANE;
    invalid_plane.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
    invalid_plane.parent_id = duplicate_id;
    invalid_plane.kind = INKPOD_TYPED_PLANE_SELECTION;
    invalid_plane.pixel_format = INKPOD_STORAGE_BINARY8;
    invalid_plane.opacity_milli = 1000U;
    invalid_plane.name_utf8 = invalid_name.data();
    invalid_plane.name_bytes = invalid_name.size();
    InkpodStatus invalid_status = INKPOD_STATUS_OK;
    state.engine->Invoke(
        [&invalid_plane, &invalid_status](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            std::uint64_t object_id{};
            invalid_status = inkpod_core_tree_edit(
                core, &invalid_plane, &result, &object_id);
            return INKPOD_STATUS_OK;
        },
        false,
        false);
    InkpodDocumentInfo after_invalid{};
    if (invalid_status != INKPOD_STATUS_INVALID_ARGUMENT
        || !QueryDocument(state, after_invalid)
        || after_invalid.document_revision != before_invalid.document_revision) {
        return 314;
    }

    inkpod::renderer::CanvasDocumentBounds selection_canvas_bounds{};
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasGetDocumentBounds,
            0,
            reinterpret_cast<LPARAM>(&selection_canvas_bounds)) != 1) {
        return 315;
    }
    const auto selection_sample = [&selection_canvas_bounds](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(selection_canvas_bounds.left
                + (selection_canvas_bounds.right - selection_canvas_bounds.left)
                    * static_cast<double>(x) / 8.0),
            static_cast<float>(selection_canvas_bounds.top
                + (selection_canvas_bounds.bottom - selection_canvas_bounds.top)
                    * static_cast<double>(y) / 8.0),
            1.0F,
            0U};
    };
    const auto send_selection_gesture = [&state](const auto& samples) noexcept {
        if (samples.empty()) {
            return false;
        }
        const inkpod::renderer::CanvasStrokeEvent begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
        if (SendMessageW(
                state.window,
                inkpod::renderer::kCanvasStrokeReady,
                0,
                reinterpret_cast<LPARAM>(&begin)) != 1) {
            return false;
        }
        if (samples.size() > 2U) {
            const inkpod::renderer::CanvasStrokeEvent append{
                inkpod::renderer::CanvasStrokeEventKind::Append,
                samples.data() + 1U,
                samples.size() - 2U};
            if (SendMessageW(
                    state.window,
                    inkpod::renderer::kCanvasStrokeReady,
                    0,
                    reinterpret_cast<LPARAM>(&append)) != 1) {
                return false;
            }
        }
        const inkpod::renderer::CanvasStrokeEvent end{
            inkpod::renderer::CanvasStrokeEventKind::End,
            samples.data() + samples.size() - 1U,
            1U};
        return SendMessageW(
                   state.window,
                   inkpod::renderer::kCanvasStrokeReady,
                   0,
                   reinterpret_cast<LPARAM>(&end)) == 1;
    };
    const auto query_selection = [&state](InkpodLocatorOutput& output) noexcept {
        output = {};
        output.struct_size = sizeof(output);
        return state.engine->Invoke(
                   [&output](InkpodCore* core) {
                       return inkpod_core_locator_sample(core, 0U, 0.0, 0.0, &output);
                   },
                   false,
                   false) == INKPOD_STATUS_OK;
    };
    const auto select_rectangle = [&](UINT mode, float x1, float y1, float x2, float y2) {
        SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_RECTANGLE, 0);
        SendMessageW(state.window, WM_COMMAND, mode, 0);
        const std::array<InkpodStrokeSample, 2U> samples{
            selection_sample(x1, y1), selection_sample(x2, y2)};
        return send_selection_gesture(samples);
    };
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 0.0F, 0.0F, 4.0F, 4.0F)
        || !select_rectangle(IDM_SELECTION_MODE_ADD, 4.0F, 0.0F, 6.0F, 2.0F)
        || !select_rectangle(IDM_SELECTION_MODE_SUBTRACT, 0.0F, 0.0F, 1.0F, 4.0F)
        || !select_rectangle(IDM_SELECTION_MODE_INTERSECT, 2.0F, 0.0F, 4.0F, 4.0F)) {
        return 315;
    }
    InkpodLocatorOutput locator{};
    if (!query_selection(locator)
        || (locator.flags & 1U) == 0U || locator.selection.x != 2
        || locator.selection.y != 0 || locator.selection.width != 2
        || locator.selection.height != 4) {
        return 316;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_ELLIPSE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
    const std::array<InkpodStrokeSample, 2U> ellipse_samples{
        selection_sample(1.0F, 1.0F), selection_sample(7.0F, 7.0F)};
    if (!send_selection_gesture(ellipse_samples)) {
        return 357;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_LASSO, 0);
    const std::array<InkpodStrokeSample, 3U> lasso_samples{
        selection_sample(0.0F, 0.0F),
        selection_sample(7.0F, 0.0F),
        selection_sample(0.0F, 7.0F)};
    if (!send_selection_gesture(lasso_samples)) {
        return 345;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_POLYLINE, 0);
    const std::array<InkpodStrokeSample, 3U> polyline_samples{
        selection_sample(1.0F, 1.0F),
        selection_sample(7.0F, 1.0F),
        selection_sample(7.0F, 7.0F)};
    if (!send_selection_gesture(polyline_samples)) {
        return 346;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_TRACE, 0);
    const std::array<InkpodStrokeSample, 2U> trace_samples{
        selection_sample(0.5F, 7.5F), selection_sample(7.5F, 0.5F)};
    if (!send_selection_gesture(trace_samples)) {
        return 347;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_WAND, 0);
    const std::array<InkpodStrokeSample, 1U> wand_samples{
        selection_sample(4.0F, 4.0F)};
    if (!send_selection_gesture(wand_samples) || !query_selection(locator)
        || (locator.flags & 1U) == 0U) {
        return 348;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_INVERT, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_EXPAND, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_SHRINK, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 349;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_ALL, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.width != 8 || locator.selection.height != 8) {
        return 350;
    }

    const InkpodStrokeSample source_sample{
        sizeof(InkpodStrokeSample), 0U, 6.0F, 6.0F, 1.0F, 0U};
    const InkpodStrokeInput source_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &source_sample,
        1U,
        sizeof(source_sample)};
    if (state.engine->Invoke(
            [source_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &source_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 317;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_COLOR, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.x != 6 || locator.selection.y != 6
        || locator.selection.width != 1 || locator.selection.height != 1) {
        return 351;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_COLOR_DIFFERENT, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U
        || locator.selection.width != 8 || locator.selection.height != 8) {
        return 352;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_COLOR_ADD, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_TO_LAYER, 0);
    if (state.selection_layer_id == 0U) {
        return 353;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_FROM_LAYER, 0);
    if (!query_selection(locator) || (locator.flags & 1U) == 0U) {
        return 354;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_LAYER_ADD, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_SELECTION_LAYER_SUBTRACT, 0);
    if (!query_selection(locator) || (locator.flags & 1U) != 0U) {
        return 355;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_MAIN_LINE, 0);
    if (!select_rectangle(IDM_SELECTION_MODE_NEW, 6.0F, 6.0F, 7.0F, 7.0F)) {
        return 356;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_COPY, 0);
    if (state.clipboard == nullptr
        || IsClipboardFormatAvailable(CF_DIBV5) == FALSE
        || IsClipboardFormatAvailable(InkpodClipboardFormat()) == FALSE
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_CUT, 0) != 1) {
        return 318;
    }
    std::vector<std::uint8_t> external_dib;
    if (OpenClipboard(state.window) == FALSE) {
        return 367;
    }
    HANDLE dib_handle = GetClipboardData(CF_DIBV5);
    const SIZE_T dib_size = dib_handle == nullptr ? 0U : GlobalSize(dib_handle);
    const void* dib_source = dib_handle == nullptr ? nullptr : GlobalLock(dib_handle);
    try {
        if (dib_source != nullptr && dib_size != 0U) {
            external_dib.assign(
                static_cast<const std::uint8_t*>(dib_source),
                static_cast<const std::uint8_t*>(dib_source) + dib_size);
        }
    } catch (const std::bad_alloc&) {
        external_dib.clear();
    }
    if (dib_source != nullptr) {
        GlobalUnlock(dib_handle);
    }
    CloseClipboard();
    HGLOBAL external_handle = external_dib.empty()
        ? nullptr
        : GlobalAlloc(GMEM_MOVEABLE, external_dib.size());
    void* external_destination = external_handle == nullptr
        ? nullptr
        : GlobalLock(external_handle);
    if (external_destination == nullptr) {
        if (external_handle != nullptr) {
            GlobalFree(external_handle);
        }
        return 369;
    }
    std::memcpy(external_destination, external_dib.data(), external_dib.size());
    GlobalUnlock(external_handle);
    if (OpenClipboard(state.window) == FALSE) {
        GlobalFree(external_handle);
        return 370;
    }
    EmptyClipboard();
    const bool external_published = SetClipboardData(CF_DIBV5, external_handle) != nullptr;
    if (!external_published) {
        GlobalFree(external_handle);
    }
    CloseClipboard();
    InkpodClipboard* private_clipboard = state.clipboard;
    state.clipboard = nullptr;
    int external_failure{};
    if (!external_published) {
        external_failure = 371;
    } else if (SendMessageW(state.window, WM_COMMAND, IDM_EDIT_PASTE, 0) != 1) {
        external_failure = 372;
    } else if (!state.floating_active) {
        external_failure = 373;
    } else if (SendMessageW(state.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1) {
        external_failure = 374;
    }
    inkpod_clipboard_release(&state.clipboard);
    state.clipboard = private_clipboard;
    PublishStandardClipboard(state.window, state.clipboard);
    if (external_failure != 0) {
        return external_failure;
    }

    const InkpodCellCreateOptions destination_options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d33000000000003),
        UINT64_C(0x4d33000000000004),
        4U,
        4U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [destination_options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(
                    core, &destination_options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 319;
    }
    ResetM3DocumentUiState(state);
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
        || !RefreshTreePane(state)
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_PASTE_SELECTED, 0) != 1
        || !state.floating_active
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_PASTE_CONVERTED, 0) != 1
        || !state.floating_active
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_PASTE, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_FLOATING_TRANSFORM, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_PASTE, 0) != 1) {
        return 358;
    }
    inkpod::renderer::CanvasDocumentBounds floating_canvas{};
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasGetDocumentBounds,
            0,
            reinterpret_cast<LPARAM>(&floating_canvas)) != 1) {
        return 359;
    }
    const double floating_zoom = (floating_canvas.right - floating_canvas.left) / 4.0;
    const InkpodStrokeSample floating_start{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(floating_canvas.left + 2.0 * floating_zoom),
        static_cast<float>(floating_canvas.top + 2.0 * floating_zoom), 1.0F, 0U};
    const InkpodStrokeSample floating_end{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(floating_canvas.left + 1.0 * floating_zoom),
        static_cast<float>(floating_canvas.top + 1.0 * floating_zoom), 1.0F, 0U};
    const inkpod::renderer::CanvasStrokeEvent floating_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &floating_start, 1U};
    const inkpod::renderer::CanvasStrokeEvent floating_finish{
        inkpod::renderer::CanvasStrokeEventKind::End, &floating_end, 1U};
    if (SendMessageW(
            state.window,
            inkpod::renderer::kCanvasStrokeReady,
            0,
            reinterpret_cast<LPARAM>(&floating_begin)) != 1
        || SendMessageW(
               state.window,
               inkpod::renderer::kCanvasStrokeReady,
               0,
               reinterpret_cast<LPARAM>(&floating_finish)) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0) != 1) {
        return 359;
    }
    const InkpodFloatingTransform floating{
        sizeof(InkpodFloatingTransform),
        0U,
        -4.0,
        -4.0,
        1.0,
        1.0,
        0.0};
    const InkpodStatus paste_status = state.engine->Invoke(
        [&state, floating](InkpodCore* core) {
            InkpodStatus status = inkpod_core_paste_begin(
                core, state.clipboard);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_floating_transform(core, &floating);
            }
            if (status == INKPOD_STATUS_OK) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                status = inkpod_core_floating_commit(core, &result);
            }
            if (status != INKPOD_STATUS_OK) {
                inkpod_core_floating_cancel(core);
            }
            return status;
        },
        true,
        true);
    InkpodColorValue pasted_color{};
    pasted_color.struct_size = sizeof(pasted_color);
    if (paste_status != INKPOD_STATUS_OK
        || state.engine->Invoke(
               [&pasted_color](InkpodCore* core) {
                   return inkpod_core_eyedropper(
                       core,
                       INKPOD_EYEDROPPER_SELECTED_PLANE,
                       2U,
                       2U,
                       &pasted_color);
               },
               false,
               false) != INKPOD_STATUS_OK
        || pasted_color.red != 0U || pasted_color.green != 0U
        || pasted_color.blue != 0U || pasted_color.alpha == 0U) {
        return 320;
    }

    InkpodDocumentInfo before_flip{};
    if (!QueryDocument(state, before_flip)) {
        return 321;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_FLIP_HORIZONTAL, 0);
    InkpodDocumentInfo after_flip{};
    InkpodSnapshotTransform transform{};
    transform.struct_size = sizeof(transform);
    const InkpodStatus transform_status = state.engine->Invoke(
        [&transform](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &snapshot);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(snapshot, &transform);
            }
            const InkpodStatus release_status =
                inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (!QueryDocument(state, after_flip)
        || after_flip.document_revision != before_flip.document_revision
        || after_flip.view_revision <= before_flip.view_revision
        || transform_status != INKPOD_STATUS_OK
        || (transform.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL)
            == 0U) {
        return 322;
    }
    SendMessageW(
        state.window, WM_COMMAND, IDM_EDIT_MIRROR_HORIZONTAL, 0);
    InkpodDocumentInfo after_mirror{};
    if (!QueryDocument(state, after_mirror)
        || after_mirror.document_revision
            != after_flip.document_revision + 1U
        || after_mirror.view_revision != after_flip.view_revision
        || (after_mirror.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) == 0U) {
        return 323;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_EDIT_UNDO, 0);

    const std::array<UINT, 6U> document_transform_commands{
        IDM_CELL_MIRROR_VERTICAL,
        IDM_CELL_ROTATE_LEFT,
        IDM_CELL_ROTATE_RIGHT,
        IDM_CELL_IMAGE_SIZE,
        IDM_CELL_RESOLUTION,
        IDM_CELL_PAPER_SETTINGS};
    for (std::size_t index = 0U; index < document_transform_commands.size(); ++index) {
        if (SendMessageW(
                state.window, WM_COMMAND, document_transform_commands[index], 0) != 1) {
            return 360 + static_cast<int>(index);
        }
    }
    InkpodDocumentInfo transformed_document{};
    if (!QueryDocument(state, transformed_document)
        || transformed_document.width <= 4U || transformed_document.height <= 4U
        || transformed_document.dpi_x_milli != 120000U
        || transformed_document.dpi_y_milli != 120000U) {
        return 366;
    }

    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_NEW, 0);
    if (state.secondary_view_id == 0U || state.active_view_id != state.secondary_view_id
        || state.document_tabs == nullptr
        || TabCtrl_GetItemCount(state.document_tabs) != 2) {
        return 324;
    }
    const InkpodViewInput secondary_pan{
        sizeof(InkpodViewInput),
        INKPOD_VIEW_PAN_BY,
        0U,
        5.0,
        0.0,
        0.0,
        0.0};
    const std::uint64_t secondary_view_id = state.secondary_view_id;
    if (state.engine->Invoke(
            [secondary_view_id, secondary_pan](InkpodCore* core) {
                return inkpod_core_view_apply(
                    core, secondary_view_id, &secondary_pan);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 324;
    }
    const InkpodStrokeSample multi_view_sample{
        sizeof(InkpodStrokeSample), 0U, 0.0F, 0.0F, 1.0F, 0U};
    const InkpodStrokeInput multi_view_stroke{
        sizeof(InkpodStrokeInput),
        INKPOD_TOOL_PENCIL,
        INKPOD_PLANE_MAIN_LINE,
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        0U,
        UINT32_C(0x000000ff),
        1.0F,
        &multi_view_sample,
        1U,
        sizeof(multi_view_sample)};
    if (state.engine->Invoke(
            [multi_view_stroke](InkpodCore* core) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(
                    core, &multi_view_stroke, &result);
            },
            true,
            true) != INKPOD_STATUS_OK) {
        return 325;
    }
    std::uint64_t primary_revision{};
    std::uint64_t secondary_revision{};
    double primary_pan_x{};
    double secondary_pan_x{};
    const InkpodStatus multi_view_status = state.engine->Invoke(
        [secondary_view_id,
         &primary_revision,
         &secondary_revision,
         &primary_pan_x,
         &secondary_pan_x](
            InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* primary{};
            InkpodSnapshot* secondary{};
            InkpodStatus status = inkpod_core_build_snapshot(
                core, &options, &primary);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot_for_view(
                    core, secondary_view_id, &options, &secondary);
            }
            InkpodSnapshotView primary_view{};
            primary_view.struct_size = sizeof(primary_view);
            InkpodSnapshotView secondary_view{};
            secondary_view.struct_size = sizeof(secondary_view);
            InkpodSnapshotTransform primary_transform{};
            primary_transform.struct_size = sizeof(primary_transform);
            InkpodSnapshotTransform secondary_transform{};
            secondary_transform.struct_size = sizeof(secondary_transform);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(primary, &primary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_view(
                    secondary, &secondary_view);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    primary, &primary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_transform(
                    secondary, &secondary_transform);
            }
            if (status == INKPOD_STATUS_OK) {
                primary_revision = primary_view.revision;
                secondary_revision = secondary_view.revision;
                primary_pan_x = primary_transform.pan_x;
                secondary_pan_x = secondary_transform.pan_x;
            }
            const InkpodStatus primary_release =
                inkpod_snapshot_release(&primary);
            const InkpodStatus secondary_release =
                inkpod_snapshot_release(&secondary);
            if (status != INKPOD_STATUS_OK) {
                return status;
            }
            return primary_release == INKPOD_STATUS_OK
                    && secondary_release == INKPOD_STATUS_OK
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_STATE;
        },
        false,
        false);
    if (multi_view_status != INKPOD_STATUS_OK || primary_revision == 0U
        || primary_revision != secondary_revision
        || primary_pan_x == secondary_pan_x) {
        return 326;
    }
    SendMessageW(
        state.window,
        inkpod::renderer::kCanvasPointerMoved,
        0,
        MAKELPARAM(2, 2));
    if (state.locator_label == nullptr || GetWindowTextLengthW(state.locator_label) < 20) {
        return 358;
    }

    const InkpodStatus navigation_status = state.engine->Invoke(
        [](InkpodCore* core) {
            InkpodStatus status = inkpod_core_shortcut_rebind(
                core,
                99U,
                static_cast<std::uint32_t>('Z'),
                INKPOD_SHORTCUT_MODIFIER_CONTROL);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_shortcut_rebind(
                    core,
                    1U,
                    static_cast<std::uint32_t>('U'),
                    INKPOD_SHORTCUT_MODIFIER_CONTROL);
            }
            return status;
        },
        true,
        true);
    if (navigation_status != INKPOD_STATUS_OK) {
        return 327;
    }
    UINT shortcut_menu_command{};
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 328;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_GRID_SETTINGS, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_GUIDE_VERTICAL, 0);
    const auto query_guide_count = [&state]() noexcept {
        std::uint64_t count = UINT64_MAX;
        const std::uint64_t view_id = state.active_view_id;
        const InkpodStatus status = state.engine->Invoke(
            [view_id, &count](InkpodCore* core) {
                const InkpodSnapshotOptions options{
                    sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
                InkpodSnapshot* snapshot{};
                InkpodStatus inner = view_id == 0U
                    ? inkpod_core_build_snapshot(core, &options, &snapshot)
                    : inkpod_core_build_snapshot_for_view(
                          core, view_id, &options, &snapshot);
                InkpodSnapshotOverlay overlay{};
                overlay.struct_size = sizeof(overlay);
                if (inner == INKPOD_STATUS_OK) {
                    inner = inkpod_snapshot_get_overlay(snapshot, &overlay);
                }
                if (inner == INKPOD_STATUS_OK) {
                    count = overlay.guide_count;
                }
                const InkpodStatus released = inkpod_snapshot_release(&snapshot);
                return inner == INKPOD_STATUS_OK ? released : inner;
            },
            false,
            false);
        return status == INKPOD_STATUS_OK ? count : UINT64_MAX;
    };
    if (query_guide_count() != 1U) {
        return 341;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_RULER, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_GRID, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_SNAP_GUIDES, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_SNAP_GRID, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_TRANSPARENT, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_ZOOM_PERCENT, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_BOX_ZOOM, 0);
    inkpod::renderer::CanvasDocumentBounds box_bounds{};
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasGetDocumentBounds,
            0,
            reinterpret_cast<LPARAM>(&box_bounds)) != 1) {
        return 329;
    }
    const std::array<InkpodStrokeSample, 2U> box_samples{
        InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(box_bounds.left
                + (box_bounds.right - box_bounds.left) * 0.25),
            static_cast<float>(box_bounds.top
                + (box_bounds.bottom - box_bounds.top) * 0.25),
            1.0F,
            0U},
        InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(box_bounds.left
                + (box_bounds.right - box_bounds.left) * 0.75),
            static_cast<float>(box_bounds.top
                + (box_bounds.bottom - box_bounds.top) * 0.75),
            1.0F,
            0U}};
    const inkpod::renderer::CanvasStrokeEvent box_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin,
        box_samples.data(),
        1U};
    const inkpod::renderer::CanvasStrokeEvent box_end{
        inkpod::renderer::CanvasStrokeEventKind::End,
        box_samples.data() + 1U,
        1U};
    if (SendMessageW(
            state.window,
            inkpod::renderer::kCanvasStrokeReady,
            0,
            reinterpret_cast<LPARAM>(&box_begin)) != 1
        || SendMessageW(
               state.window,
               inkpod::renderer::kCanvasStrokeReady,
               0,
               reinterpret_cast<LPARAM>(&box_end)) != 1) {
        return 329;
    }
    inkpod::renderer::CanvasDocumentBounds guide_bounds{};
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasGetDocumentBounds,
            0,
            reinterpret_cast<LPARAM>(&guide_bounds)) != 1) {
        return 329;
    }
    const double guide_zoom = (guide_bounds.right - guide_bounds.left) / 4.0;
    const auto send_guide_drag = [&state](
                                     InkpodStrokeSample begin_sample,
                                     InkpodStrokeSample end_sample) noexcept {
        const inkpod::renderer::CanvasStrokeEvent begin_event{
            inkpod::renderer::CanvasStrokeEventKind::Begin, &begin_sample, 1U};
        const inkpod::renderer::CanvasStrokeEvent end_event{
            inkpod::renderer::CanvasStrokeEventKind::End, &end_sample, 1U};
        return SendMessageW(
                   state.window,
                   inkpod::renderer::kCanvasStrokeReady,
                   0,
                   reinterpret_cast<LPARAM>(&begin_event)) == 1
            && SendMessageW(
                   state.window,
                   inkpod::renderer::kCanvasStrokeReady,
                   0,
                   reinterpret_cast<LPARAM>(&end_event)) == 1;
    };
    const auto guide_sample = [](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample), 0U, x, y, 1.0F, 0U};
    };
    const float guide_x1 = static_cast<float>(guide_bounds.left + guide_zoom);
    const float guide_x3 = static_cast<float>(guide_bounds.left + 3.0 * guide_zoom);
    const float guide_y = static_cast<float>((guide_bounds.top + guide_bounds.bottom) / 2.0);
    if (!send_guide_drag(
            guide_sample(30.0F, 10.0F),
            guide_sample(guide_x1, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 2U) {
        return 342;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_VIEW_GUIDE_MOVE, 0);
    if (!send_guide_drag(
            guide_sample(guide_x1, guide_y),
            guide_sample(guide_x3, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 2U) {
        return 343;
    }
    if (!send_guide_drag(
            guide_sample(guide_x3, guide_y),
            guide_sample(-10.0F, guide_y))) {
        return 329;
    }
    if (query_guide_count() != 1U) {
        return 344;
    }
    if (SendMessageW(
            state.window, WM_COMMAND, IDM_SHORTCUT_EDIT, 0) != 1) {
        return 329;
    }
    if (!ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('Z'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)
        || shortcut_menu_command != IDM_EDIT_UNDO
        || ResolveConfiguredShortcut(
            state,
            static_cast<std::uint32_t>('U'),
            INKPOD_SHORTCUT_MODIFIER_CONTROL,
            shortcut_menu_command)) {
        return 330;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_SHORTCUT_RESET, 0);
    locator = {};
    locator.struct_size = sizeof(locator);
    const std::uint64_t active_view_id = state.active_view_id;
    if (!state.grid_visible || !state.ruler_visible || !state.snap_guides
        || !state.snap_grid || state.transparent_visible
        || state.engine->Invoke(
               [active_view_id, &locator](InkpodCore* core) {
                   return inkpod_core_locator_sample(
                       core, active_view_id, 2.0, 2.0, &locator);
               },
               false,
               false) != INKPOD_STATUS_OK) {
        return 331;
    }
    bool overlay_connected{};
    std::uint32_t smoke_overlay_flags{};
    std::uint32_t smoke_grid_spacing{};
    std::uint32_t smoke_grid_subdivisions{};
    std::uint64_t smoke_guide_count{};
    const InkpodStatus overlay_status = state.engine->Invoke(
        [active_view_id,
         &overlay_connected,
         &smoke_overlay_flags,
         &smoke_grid_spacing,
         &smoke_grid_subdivisions,
         &smoke_guide_count](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = active_view_id == 0U
                ? inkpod_core_build_snapshot(core, &options, &snapshot)
                : inkpod_core_build_snapshot_for_view(
                      core, active_view_id, &options, &snapshot);
            InkpodSnapshotOverlay overlay{};
            overlay.struct_size = sizeof(overlay);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_overlay(snapshot, &overlay);
            }
            if (status == INKPOD_STATUS_OK) {
                smoke_overlay_flags = overlay.flags;
                smoke_grid_spacing = overlay.grid_spacing_x;
                smoke_grid_subdivisions = overlay.grid_subdivisions;
                smoke_guide_count = overlay.guide_count;
                overlay_connected =
                    (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED) != 0U
                    && (overlay.flags & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) == 0U
                    && overlay.grid_spacing_x == 8U
                    && overlay.grid_subdivisions == 2U
                    && overlay.guide_count == 1U
                    && overlay.guides != nullptr;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (overlay_status != INKPOD_STATUS_OK || !overlay_connected) {
        if (overlay_status != INKPOD_STATUS_OK) {
            return 332;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE) == 0U) {
            return 334;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE) == 0U) {
            return 335;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED) == 0U) {
            return 336;
        }
        if ((smoke_overlay_flags & INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW) != 0U) {
            return 337;
        }
        if (smoke_grid_spacing != 8U || smoke_grid_subdivisions != 2U) {
            return 338;
        }
        return smoke_guide_count == 1U
            ? 339
            : 340 + static_cast<int>(std::min<std::uint64_t>(smoke_guide_count, 10U));
    }
    return SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 333;
}

int RunM4Smoke(AppState& state) noexcept {
    if (state.engine == nullptr
        || CreateCell(state, 32U, 24U, 120000U) != INKPOD_STATUS_OK) {
        return 400;
    }
    RefreshTreePane(state);
    RefreshLightTablePane(state);
    for (const UINT command : {
             IDM_CELL_FRAME_REFERENCE,
             IDM_CELL_FRAME_DRAWING,
             IDM_CELL_FRAME_SAFE,
             IDM_CELL_MARGINS}) {
        SendMessageW(state.window, WM_COMMAND, command, 0);
    }
    InkpodDocumentInfo paper{};
    if (!QueryDocument(state, paper) || paper.width != 32U || paper.height != 24U
        || paper.dpi_x_milli != 120000U || paper.margin_left != 1U
        || paper.margin_top != 2U || paper.margin_right != 3U
        || paper.margin_bottom != 4U || paper.reference_frame.x != 1
        || paper.drawing_frame.x != 2 || paper.safe_frame.x != 3) {
        return 401;
    }

    if (SendMessageW(state.window, WM_COMMAND, IDM_LAYER_NEW, 0) != 0
        || SendMessageW(state.layer_list, LB_GETCOUNT, 0, 0) < 2) {
        return 402;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_TOGGLE_VISIBLE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_TOGGLE_EDITABLE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_OPACITY, 0);
    if (SendMessageW(state.window, WM_COMMAND, IDM_LAYER_PROPERTIES, 0) != 1) {
        return 402;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_CONVERT, 0);
    if (SendMessageW(state.window, WM_COMMAND, IDM_PLANE_NEW, 0) != 0
        || SendMessageW(state.plane_list, LB_GETCOUNT, 0, 0) < 2) {
        return 403;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_TOGGLE_VISIBLE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_TOGGLE_EDITABLE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_OPACITY, 0);
    if (SendMessageW(state.window, WM_COMMAND, IDM_PLANE_PROPERTIES, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_PLANE_CONVERT, 0) != 1) {
        return 403;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_DUPLICATE, 0);
    SetWindowPos(
        state.plane_list, nullptr, 0, 0, 220, 160,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE);
    const int plane_item_height = static_cast<int>(SendMessageW(
        state.plane_list, LB_GETITEMHEIGHT, 0, 0));
    const std::uint32_t plane_drag_start = state.active_tree_plane_index;
    if (plane_drag_start != 0U) {
        SendMessageW(
            state.plane_list,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(4, plane_item_height * static_cast<int>(plane_drag_start) + 2));
        SendMessageW(
            state.plane_list, WM_LBUTTONUP, 0, MAKELPARAM(4, 2));
        if (state.active_tree_plane_index != 0U) {
            return 468;
        }
    }
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_MOVE_DOWN, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_DELETE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_DUPLICATE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_MOVE_UP, 0);
    if (SendMessageW(state.window, WM_COMMAND, IDM_PLANE_MERGE, 0) != 1) {
        return 403;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_DUPLICATE, 0);
    SetWindowPos(
        state.layer_list, nullptr, 0, 0, 220, 160,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE);
    const int layer_item_height = static_cast<int>(SendMessageW(
        state.layer_list, LB_GETITEMHEIGHT, 0, 0));
    const std::uint32_t layer_drag_start = state.active_tree_layer_index;
    if (layer_drag_start != 0U) {
        SendMessageW(
            state.layer_list,
            WM_LBUTTONDOWN,
            MK_LBUTTON,
            MAKELPARAM(4, layer_item_height * static_cast<int>(layer_drag_start) + 2));
        SendMessageW(
            state.layer_list, WM_LBUTTONUP, 0, MAKELPARAM(4, 2));
        if (state.active_tree_layer_index != 0U) {
            return 469;
        }
    }
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_MOVE_UP, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_MOVE_DOWN, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_LAYER_MERGE, 0);

    if (CreateCell(state, 12U, 10U, 96000U) != INKPOD_STATUS_OK) {
        return 404;
    }
    try {
        state.smoke_raster_path = L"inkpod-io2-smoke.png";
    } catch (const std::bad_alloc&) {
        return 405;
    }
    DeleteFileW(state.smoke_raster_path.c_str());
    if (SendMessageW(state.window, WM_COMMAND, IDM_FILE_EXPORT_RASTER, 0) != 1
        || GetFileAttributesW(state.smoke_raster_path.c_str()) == INVALID_FILE_ATTRIBUTES
        || SendMessageW(state.window, WM_COMMAND, IDM_FILE_IMPORT_RASTER, 0) != 1) {
        DeleteFileW(state.smoke_raster_path.c_str());
        state.smoke_raster_path.clear();
        return 406;
    }
    InkpodDocumentInfo imported{};
    const bool import_ok = QueryDocument(state, imported) && imported.width == 12U
        && imported.height == 10U && imported.dpi_x_milli >= 95000U
        && imported.dpi_x_milli <= 97000U;
    RefreshLightTablePane(state);
    if (!import_ok) {
        DeleteFileW(state.smoke_raster_path.c_str());
        state.smoke_raster_path.clear();
        return 407;
    }
    if (SendMessageW(state.window, WM_COMMAND, IDM_COLOR_EDITOR, 0) != 1
        || state.drawing_color.depth != INKPOD_COLOR_DEPTH_16) {
        return 430;
    }
    const std::array<UINT, 10U> color_commands{
        IDM_COLOR_SOURCE_TOPMOST,
        IDM_COLOR_SOURCE_SELECTED,
        IDM_COLOR_SOURCE_COMPOSITE,
        IDM_COLOR_SOURCE_LIGHT_TABLE,
        IDM_PALETTE_REGISTER,
        IDM_PALETTE_SAVE,
        IDM_PALETTE_CLEAR,
        IDM_PALETTE_LOAD,
        IDM_PALETTE_NEXT_GROUP,
        IDM_CHART_GENERATE};
    for (std::size_t index = 0U; index < color_commands.size(); ++index) {
        if (SendMessageW(state.window, WM_COMMAND, color_commands[index], 0) != 1) {
            return 434 + static_cast<int>(index);
        }
    }
    RefreshColorPanes(state);
    SendMessageW(state.color_chart_list, LB_SETCURSEL, 0, 0);
    const std::array<UINT, 4U> chart_navigation_commands{
        IDM_CHART_RENAME, IDM_CHART_SEARCH, IDM_CHART_NEXT, IDM_CHART_NEXT_PAGE};
    for (std::size_t index = 0U; index < chart_navigation_commands.size(); ++index) {
        if (SendMessageW(
                state.window, WM_COMMAND, chart_navigation_commands[index], 0) != 1) {
            return 450 + static_cast<int>(index);
        }
    }
    state.color_chart_page = 0U;
    RefreshColorPanes(state);
    SendMessageW(state.color_chart_list, LB_SETCURSEL, 0, 0);
    if (SendMessageW(state.window, WM_COMMAND, IDM_CHART_SAVE, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_CHART_COPY, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_CHART_PASTE, 0) != 1) {
        return 432;
    }
    SendMessageW(state.color_chart_list, LB_SETCURSEL, 0, 0);
    if (SendMessageW(state.window, WM_COMMAND, IDM_CHART_CUT, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_CHART_LOAD, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_CHART_LOCK, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_CHART_LOCK, 0) != 1) {
        return 433;
    }
    DeleteFileW(L"inkpod-palette-smoke.inkpalette");
    DeleteFileW(L"inkpod-chart-smoke.inkchart");
    const std::array<UINT, 14U> light_table_commands{
        IDM_LT_SET_NEW,
        IDM_LT_SET_RENAME,
        IDM_LT_SET_DUPLICATE,
        IDM_LT_SET_UP,
        IDM_LT_SET_DOWN,
        IDM_LT_SET_DELETE,
        IDM_LT_ITEM_ADD,
        IDM_LT_ITEM_ADD,
        IDM_LT_ITEM_DOWN,
        IDM_LT_ITEM_UP,
        IDM_LT_GLOBAL_OPACITY,
        IDM_LT_ITEM_SAMPLE,
        IDM_LT_ITEM_PROPERTIES,
        IDM_LT_ITEM_RELOAD};
    for (std::size_t index = 0U; index < light_table_commands.size(); ++index) {
        if (SendMessageW(
                state.window, WM_COMMAND, light_table_commands[index], 0) != 1) {
            DeleteFileW(state.smoke_raster_path.c_str());
            state.smoke_raster_path.clear();
            return 414 + static_cast<int>(index);
        }
    }
    InkpodLightTableItemInfo light_item{};
    if (!QueryLightTableItem(state, state.active_light_table_item_index, light_item)
        || light_item.opacity_milli != 500U
        || light_item.effective_opacity_milli != 250U
        || light_item.translate_x_milli != 1000
        || light_item.translate_y_milli != -1000) {
        DeleteFileW(state.smoke_raster_path.c_str());
        state.smoke_raster_path.clear();
        return 408;
    }
    inkpod::renderer::CanvasDocumentBounds light_canvas{};
    if (SendMessageW(state.window, WM_COMMAND, IDM_LT_ITEM_MOVE, 0) != 1
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&light_canvas)) != 1) {
        return 470;
    }
    const InkpodStrokeSample light_move_start{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(light_canvas.left + 2.0),
        static_cast<float>(light_canvas.top + 2.0), 1.0F, 0U};
    const InkpodStrokeSample light_move_end{
        sizeof(InkpodStrokeSample), 0U,
        static_cast<float>(light_canvas.left + 12.0),
        static_cast<float>(light_canvas.top + 12.0), 1.0F, 0U};
    const inkpod::renderer::CanvasStrokeEvent light_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &light_move_start, 1U};
    const inkpod::renderer::CanvasStrokeEvent light_end{
        inkpod::renderer::CanvasStrokeEventKind::End, &light_move_end, 1U};
    if (SendMessageW(
            state.window,
            inkpod::renderer::kCanvasStrokeReady,
            0,
            reinterpret_cast<LPARAM>(&light_begin)) != 1
        || SendMessageW(
               state.window,
               inkpod::renderer::kCanvasStrokeReady,
               0,
               reinterpret_cast<LPARAM>(&light_end)) != 1
        || !QueryLightTableItem(state, state.active_light_table_item_index, light_item)
        || (light_item.translate_x_milli == 1000
            && light_item.translate_y_milli == -1000)) {
        return 471;
    }
    const std::wstring swap_save = L"inkpod-lt-smoke.inkpod";
    DeleteFileW(swap_save.c_str());
    if (SaveToPath(state, swap_save) != INKPOD_STATUS_OK
        || SendMessageW(state.window, WM_COMMAND, IDM_LT_ITEM_SWAP, 0) != 1) {
        DeleteFileW(swap_save.c_str());
        DeleteFileW((swap_save + L".recovery.inkpod").c_str());
        DeleteFileW(state.smoke_raster_path.c_str());
        state.smoke_raster_path.clear();
        return 409;
    }
    DeleteFileW(swap_save.c_str());
    DeleteFileW((swap_save + L".recovery.inkpod").c_str());
    std::vector<std::uint8_t> sequence_source;
    if (!ReadBoundedFile(state.smoke_raster_path, sequence_source)) {
        DeleteFileW(state.smoke_raster_path.c_str());
        state.smoke_raster_path.clear();
        return 410;
    }
    try {
        state.smoke_sequence_paths = {L"cell10.png", L"cell1.png", L"cell3.png"};
    } catch (const std::bad_alloc&) {
        DeleteFileW(state.smoke_raster_path.c_str());
        state.smoke_raster_path.clear();
        return 410;
    }
    for (const auto& path : state.smoke_sequence_paths) {
        DeleteFileW(path.c_str());
        if (!WriteFileAtomically(path, sequence_source)) {
            return 410;
        }
    }
    if (SendMessageW(state.window, WM_COMMAND, IDM_SEQ_IMPORT, 0) != 1) {
        return 411;
    }
    RefreshSequencePane(state);
    if (SendMessageW(state.sequence_list, LB_GETCOUNT, 0, 0) != 3
        || SendMessageW(state.window, WM_COMMAND, IDM_SUBPALETTE_SET, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_SUBPALETTE_SAMPLE, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_SEQ_GOTO, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_SEQ_PREVIOUS, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_SEQ_NEXT, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_START, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_NEXT, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_PAUSE, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_PAUSE, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_PREVIOUS, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_FIRST, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_LAST, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_FPS_30, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_FPS_25, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_FPS_24, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_FPS_12, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_FPS_10, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_FPS_8, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_MOTION_STOP, 0) != 1) {
        return 412;
    }
    DeleteFileW(state.smoke_raster_path.c_str());
    state.smoke_raster_path = L"inkpod-sequence-export.png";
    DeleteFileW(state.smoke_raster_path.c_str());
    if (SendMessageW(state.window, WM_COMMAND, IDM_SEQ_EXPORT, 0) != 1
        || GetFileAttributesW(L"inkpod-sequence-export-cell1.png")
            == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(L"inkpod-sequence-export-cell3.png")
            == INVALID_FILE_ATTRIBUTES
        || GetFileAttributesW(L"inkpod-sequence-export-cell10.png")
            == INVALID_FILE_ATTRIBUTES) {
        return 413;
    }
    for (const auto& path : state.smoke_sequence_paths) {
        DeleteFileW(path.c_str());
    }
    state.smoke_sequence_paths.clear();
    DeleteFileW(L"inkpod-sequence-export-cell1.png");
    DeleteFileW(L"inkpod-sequence-export-cell3.png");
    DeleteFileW(L"inkpod-sequence-export-cell10.png");
    DeleteFileW(state.smoke_raster_path.c_str());
    state.smoke_raster_path.clear();
    return 0;
}

int RunM5Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 500;
    }
    const InkpodCellCreateOptions options{
        sizeof(InkpodCellCreateOptions),
        0U,
        INKPOD_FEATURE_NONE,
        UINT64_C(0x4d35000000000001),
        UINT64_C(0x4d35000000000002),
        64U,
        64U,
        96000U,
        96000U};
    if (state.engine->Invoke(
            [options](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell(core, &options, &info);
            },
            false,
            false) != INKPOD_STATUS_OK) {
        return 501;
    }
    ResetM3DocumentUiState(state);
    if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
        return 501;
    }

    InkpodSnapshotVectorSegment geometry_before{};
    std::uint64_t vector_layer_id{};
    std::uint64_t vector_trace_plane_id{};
    std::uint64_t vector_path_id{};
    std::uint64_t vector_fill_id{};
    const InkpodStatus vector_status = state.engine->Invoke(
        [&geometry_before,
         &vector_layer_id,
         &vector_trace_plane_id,
         &vector_path_id,
         &vector_fill_id](InkpodCore* core) {
            static constexpr std::array<std::uint8_t, 6U> name{
                'V', 'e', 'c', 't', 'o', 'r'};
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CREATE_LAYER;
            edit.kind = INKPOD_LAYER_VECTOR_COLORING;
            edit.name_utf8 = name.data();
            edit.name_bytes = name.size();
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            InkpodStatus status = inkpod_core_tree_edit(
                core, &edit, &result, &vector_layer_id);
            InkpodNodeInfo trace{};
            trace.struct_size = sizeof(trace);
            InkpodNodeInfo fill{};
            fill.struct_size = sizeof(fill);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 1U, &trace);
            }
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_node_get(core, 1U, 2U, &fill);
            }
            if (status != INKPOD_STATUS_OK || vector_layer_id == 0U
                || trace.kind != INKPOD_TYPED_PLANE_COLOR_TRACE
                || fill.kind != INKPOD_TYPED_PLANE_VECTOR_FILL) {
                return status == INKPOD_STATUS_OK
                    ? INKPOD_STATUS_INVALID_STATE
                    : status;
            }
            vector_trace_plane_id = trace.id;
            constexpr auto point = [](float x, float y) noexcept {
                return InkpodVectorPoint{x, y};
            };
            constexpr auto line = [](InkpodVectorPoint start, InkpodVectorPoint end) noexcept {
                return InkpodVectorCubicSegment{
                    sizeof(InkpodVectorCubicSegment),
                    0U,
                    start,
                    InkpodVectorPoint{
                        (start.x * 2.0F + end.x) / 3.0F,
                        (start.y * 2.0F + end.y) / 3.0F},
                    InkpodVectorPoint{
                        (start.x + end.x * 2.0F) / 3.0F,
                        (start.y + end.y * 2.0F) / 3.0F},
                    end,
                    1.0F,
                    5.0F};
            };
            constexpr std::array<InkpodVectorPoint, 5U> corners{
                point(8.0F, 8.0F),
                point(56.0F, 8.0F),
                point(56.0F, 56.0F),
                point(8.0F, 56.0F),
                point(8.0F, 8.0F)};
            const std::array<InkpodVectorCubicSegment, 4U> segments{
                line(corners[0], corners[1]),
                line(corners[1], corners[2]),
                line(corners[2], corners[3]),
                line(corners[3], corners[4])};
            const InkpodVectorPathInput path{
                sizeof(InkpodVectorPathInput),
                0U,
                INKPOD_VECTOR_PATH_CLOSED,
                trace.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    20U,
                    40U,
                    220U,
                    255U},
                segments.data(),
                segments.size(),
                sizeof(InkpodVectorCubicSegment)};
            status = inkpod_core_vector_add_path(
                core, &path, &result, &vector_path_id);
            const InkpodVectorFillInput topology{
                sizeof(InkpodVectorFillInput),
                0U,
                0U,
                fill.id,
                InkpodColorValue{
                    sizeof(InkpodColorValue),
                    INKPOD_COLOR_DEPTH_8,
                    240U,
                    120U,
                    20U,
                    180U},
                &vector_path_id,
                1U};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_vector_add_fill(
                    core, &topology, &result, &vector_fill_id);
            }
            const InkpodSnapshotOptions snapshot_options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_build_snapshot(
                    core, &snapshot_options, &snapshot);
            }
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK
                && (vectors.segment_count != 4U || vectors.fill_count != 1U
                    || vectors.boundary_path_count != 1U || vectors.segments == nullptr
                    || vectors.fills == nullptr || vectors.boundary_path_ids == nullptr
                    || vectors.segments->path_id != vector_path_id
                    || vectors.fills->fill_id != vector_fill_id
                    || *vectors.boundary_path_ids != vector_path_id)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status == INKPOD_STATUS_OK) {
                geometry_before = *vectors.segments;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        true,
        true);
    if (vector_status != INKPOD_STATUS_OK || vector_layer_id == 0U
        || vector_trace_plane_id == 0U || vector_path_id == 0U || vector_fill_id == 0U) {
        return 502;
    }
    if (ApplyView(state, INKPOD_VIEW_ZOOM_AT, 2.0, 32.0, 32.0)
        != INKPOD_STATUS_OK) {
        return 503;
    }
    bool geometry_unchanged{};
    const InkpodStatus zoom_status = state.engine->Invoke(
        [&geometry_before, &geometry_unchanged](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            InkpodStatus status = inkpod_core_build_snapshot(core, &options, &snapshot);
            InkpodSnapshotVectorView vectors{};
            vectors.struct_size = sizeof(vectors);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_snapshot_get_vectors(snapshot, &vectors);
            }
            if (status == INKPOD_STATUS_OK && vectors.segment_count != 0U
                && vectors.segments != nullptr) {
                const InkpodSnapshotVectorSegment& after = *vectors.segments;
                geometry_unchanged = after.path_id == geometry_before.path_id
                    && after.p0.x == geometry_before.p0.x
                    && after.p0.y == geometry_before.p0.y
                    && after.p1.x == geometry_before.p1.x
                    && after.p1.y == geometry_before.p1.y
                    && after.p2.x == geometry_before.p2.x
                    && after.p2.y == geometry_before.p2.y
                    && after.p3.x == geometry_before.p3.x
                    && after.p3.y == geometry_before.p3.y
                    && after.width_start == geometry_before.width_start
                    && after.width_end == geometry_before.width_end;
            }
            const InkpodStatus release_status = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? release_status : status;
        },
        false,
        false);
    if (zoom_status != INKPOD_STATUS_OK || !geometry_unchanged) {
        return 504;
    }
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasRenderOnce,
            0,
            0) != 1) {
        return 505;
    }
    if (SendMessageW(
            state.canvas,
            inkpod::renderer::kCanvasValidateClosedVectorStroke,
            0,
            0) != 1) {
        return 507;
    }

    state.active_tree_layer_id = vector_layer_id;
    state.active_tree_plane_id = vector_trace_plane_id;
    if (!RefreshTreePane(state)) {
        return 506;
    }
    UpdateMenuState(state);
    for (const UINT command : {
             IDM_VECTOR_LINE, IDM_VECTOR_CURVE, IDM_VECTOR_RECTANGLE,
             IDM_VECTOR_ELLIPSE, IDM_VECTOR_POLYLINE, IDM_VECTOR_ERASER}) {
        const UINT command_state = GetMenuState(GetMenu(state.window), command, MF_BYCOMMAND);
        if (command_state == static_cast<UINT>(-1)
            || (command_state & (MF_DISABLED | MF_GRAYED)) != 0U) {
            return 508;
        }
    }
    inkpod::renderer::CanvasDocumentBounds canvas_bounds{};
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&canvas_bounds)) != 1) {
        return 506;
    }
    const double canvas_zoom = (canvas_bounds.right - canvas_bounds.left)
        / static_cast<double>(document.width);
    const auto sample = [&](float x, float y) {
        return InkpodStrokeSample{
            sizeof(InkpodStrokeSample),
            0U,
            static_cast<float>(canvas_bounds.left + static_cast<double>(x) * canvas_zoom),
            static_cast<float>(canvas_bounds.top + static_cast<double>(y) * canvas_zoom),
            1.0F,
            0U};
    };
    const auto gesture = [&](UINT command, const std::array<InkpodStrokeSample, 4U>& samples) {
        if (SendMessageW(state.window, WM_COMMAND, command, 0) != 1) {
            return false;
        }
        const inkpod::renderer::CanvasStrokeEvent begin{
            inkpod::renderer::CanvasStrokeEventKind::Begin, samples.data(), 1U};
        const inkpod::renderer::CanvasStrokeEvent append{
            inkpod::renderer::CanvasStrokeEventKind::Append, samples.data() + 1U, 2U};
        const inkpod::renderer::CanvasStrokeEvent end{
            inkpod::renderer::CanvasStrokeEventKind::End, samples.data() + 3U, 1U};
        return SendMessageW(
                   state.window,
                   inkpod::renderer::kCanvasStrokeReady,
                   0,
                   reinterpret_cast<LPARAM>(&begin)) == 1
            && SendMessageW(
                   state.window,
                   inkpod::renderer::kCanvasStrokeReady,
                   0,
                   reinterpret_cast<LPARAM>(&append)) == 1
            && SendMessageW(
                   state.window,
                   inkpod::renderer::kCanvasStrokeReady,
                   0,
                   reinterpret_cast<LPARAM>(&end)) == 1;
    };
    const std::array<InkpodStrokeSample, 4U> line_samples{
        sample(4.0F, 8.0F), sample(5.0F, 8.0F),
        sample(6.0F, 8.0F), sample(7.0F, 8.0F)};
    const std::array<InkpodStrokeSample, 4U> curve_samples{
        sample(10.0F, 12.0F), sample(16.0F, 4.0F),
        sample(24.0F, 20.0F), sample(30.0F, 12.0F)};
    const std::array<InkpodStrokeSample, 4U> shape_samples{
        sample(34.0F, 10.0F), sample(38.0F, 16.0F),
        sample(44.0F, 24.0F), sample(50.0F, 30.0F)};
    const std::array<InkpodStrokeSample, 4U> polyline_samples{
        sample(10.0F, 40.0F), sample(20.0F, 45.0F),
        sample(28.0F, 38.0F), sample(36.0F, 48.0F)};
    if (!gesture(IDM_VECTOR_LINE, line_samples)
        || !gesture(IDM_VECTOR_CURVE, curve_samples)
        || !gesture(IDM_VECTOR_RECTANGLE, shape_samples)
        || !gesture(IDM_VECTOR_ELLIPSE, shape_samples)
        || !gesture(IDM_VECTOR_POLYLINE, polyline_samples)) {
        return 507;
    }
    const std::array<UINT, 8U> selection_commands{
        IDM_VECTOR_SELECT_CUT,
        IDM_VECTOR_SELECT_TOUCH,
        IDM_VECTOR_SELECT_CONTAINED,
        IDM_VECTOR_SELECT_LINE,
        IDM_VECTOR_SELECT_WHOLE_LINE,
        IDM_VECTOR_SELECT_INTERSECTION,
        IDM_VECTOR_SELECT_FILL_BOUNDARY,
        IDM_VECTOR_SELECT_FILL};
    for (std::size_t index = 0U; index < selection_commands.size(); ++index) {
        const UINT command = selection_commands[index];
        if (SendMessageW(state.window, WM_COMMAND, command, 0) != 1) {
            return 520 + static_cast<int>(index);
        }
    }
    if (SendMessageW(state.window, WM_COMMAND, IDM_VECTOR_SELECT_TOUCH, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_VECTOR_WIDTH, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_VECTOR_CONNECT, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_VECTOR_ERASE_WHOLE, 0) != 1) {
        return 509;
    }
    const inkpod::renderer::CanvasStrokeEvent erase_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, line_samples.data() + 2U, 1U};
    const inkpod::renderer::CanvasStrokeEvent erase_end{
        inkpod::renderer::CanvasStrokeEventKind::End, line_samples.data() + 2U, 1U};
    if (SendMessageW(
            state.window,
            inkpod::renderer::kCanvasStrokeReady,
            0,
            reinterpret_cast<LPARAM>(&erase_begin)) != 1
        || SendMessageW(
               state.window,
               inkpod::renderer::kCanvasStrokeReady,
               0,
               reinterpret_cast<LPARAM>(&erase_end)) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_VECTOR_RASTERIZE, 0) != 1
        || SendMessageW(state.window, WM_COMMAND, IDM_VECTOR_VECTORIZE, 0) != 1) {
        return 510;
    }
    return 0;
}

int RunM6Smoke(AppState& state) noexcept {
    if (state.engine == nullptr) {
        return 600;
    }
    bool preview_cancelled{};
    bool undo_restored{};
    bool adjustment_preserved_source{};
    bool effect_connected{};
    const InkpodStatus status = state.engine->Invoke(
        [&preview_cancelled, &undo_restored, &adjustment_preserved_source, &effect_connected](
            InkpodCore* core) {
            InkpodDocumentInfo document = EmptyDocumentInfo();
            InkpodStatus current = inkpod_core_get_document_info(core, &document);
            const std::array<InkpodStrokeSample, 1U> samples{
                InkpodStrokeSample{
                    sizeof(InkpodStrokeSample), 0U, 32.0F, 32.0F, 1.0F, 0U}};
            InkpodStrokeInput stroke{
                sizeof(InkpodStrokeInput),
                INKPOD_TOOL_PENCIL,
                INKPOD_PLANE_COLOR,
                INKPOD_COORDINATE_SPACE_DOCUMENT,
                0U,
                UINT32_C(0x204080ff),
                3.0F,
                samples.data(),
                samples.size(),
                sizeof(InkpodStrokeSample)};
            InkpodDispatchResult dispatch{};
            dispatch.struct_size = sizeof(dispatch);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_apply_stroke(core, &stroke, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
            }
            const std::uint64_t original = document.color_plane_checksum;
            InkpodFilterInput filter{};
            filter.struct_size = sizeof(filter);
            filter.kind = INKPOD_FILTER_INVERT;
            filter.plane_id = document.color_plane_id;
            filter.channel = INKPOD_FILTER_CHANNEL_RGB;
            InkpodFilterPreviewInfo preview{};
            preview.struct_size = sizeof(preview);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK
                && (preview.base_checksum != original
                    || preview.preview_checksum == original)) {
                current = INKPOD_STATUS_INVALID_STATE;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_cancel(core, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                preview_cancelled = document.color_plane_checksum == original
                    && preview.preview_checksum == original;
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_begin(core, &filter, &preview);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_filter_preview_apply(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_undo(core, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                undo_restored = document.color_plane_checksum == original;
            }
            filter.kind = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
            filter.parameter_0 = 100;
            filter.parameter_1 = 200;
            static constexpr std::array<std::uint8_t, 13U> name{
                'M', '6', ' ', 'A', 'd', 'j', 'u', 's', 't', 'm', 'e', 'n', 't'};
            std::uint64_t adjustment_id{};
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_adjustment_create(
                    core,
                    &filter,
                    name.data(),
                    name.size(),
                    &dispatch,
                    &adjustment_id);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                adjustment_preserved_source = adjustment_id != 0U
                    && document.color_plane_checksum == original;
            }
            const auto color16 = [](std::uint16_t red,
                                    std::uint16_t green,
                                    std::uint16_t blue,
                                    std::uint16_t alpha) noexcept {
                InkpodColorValue color{};
                color.struct_size = sizeof(color);
                color.depth = INKPOD_COLOR_DEPTH_16;
                color.red = red;
                color.green = green;
                color.blue = blue;
                color.alpha = alpha;
                return color;
            };
            std::array<InkpodGradientStop, 3U> stops{};
            for (auto& stop : stops) {
                stop.struct_size = sizeof(stop);
            }
            stops[0].position_milli = 0U;
            stops[0].color = color16(65535U, 0U, 0U, 65535U);
            stops[1].position_milli = 500U;
            stops[1].color = color16(0U, 65535U, 0U, 32768U);
            stops[2].position_milli = 1000U;
            stops[2].color = color16(0U, 0U, 65535U, 65535U);
            InkpodGradientInput gradient{};
            gradient.struct_size = sizeof(gradient);
            gradient.kind = INKPOD_GRADIENT_LINEAR;
            gradient.plane_id = document.color_plane_id;
            gradient.mode = INKPOD_GRADIENT_OVERWRITE;
            gradient.start_x_milli = 500;
            gradient.start_y_milli = 500;
            gradient.end_x_milli = 63500;
            gradient.end_y_milli = 500;
            gradient.stops = stops.data();
            gradient.stop_count = stops.size();
            gradient.stop_stride_bytes = sizeof(InkpodGradientStop);
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_effect_gradient(core, &gradient, &dispatch);
            }
            if (current == INKPOD_STATUS_OK) {
                current = inkpod_core_get_document_info(core, &document);
                effect_connected = document.color_plane_checksum != original;
            }
            return current;
        },
        true,
        true);
    if (status != INKPOD_STATUS_OK || !preview_cancelled || !undo_restored
        || !adjustment_preserved_source || !effect_connected) {
        return 601;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_PLANE_COLOR, 0);
    InkpodDocumentInfo before_menu = EmptyDocumentInfo();
    InkpodDocumentInfo after_menu = EmptyDocumentInfo();
    HMENU menu = GetMenu(state.window);
    if (menu == nullptr || !QueryDocument(state, before_menu)
        || GetMenuState(menu, IDM_FILTER_LAST, MF_BYCOMMAND) == static_cast<UINT>(-1)) {
        return 602;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_FILTER_LAST, 0);
    if (!QueryDocument(state, after_menu)
        || after_menu.color_plane_checksum == before_menu.color_plane_checksum) {
        return 603;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_CREATE, 0);
    if (state.m6_adjustments.size() != 2U
        || state.m6_adjustments[0].id == state.m6_adjustments[1].id) {
        return 605;
    }
    const std::uint64_t newest_adjustment = state.m6_adjustment_id;
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_PREVIOUS, 0);
    if (state.m6_adjustment_id == newest_adjustment) {
        return 606;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_EDIT, 0);
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_TOGGLE, 0);
    if (state.m6_adjustment_visible) {
        return 607;
    }
    SendMessageW(state.window, WM_COMMAND, IDM_ADJUSTMENT_MOVE_TOP, 0);
    InkpodDocumentInfo before_spray = EmptyDocumentInfo();
    InkpodDocumentInfo after_spray = EmptyDocumentInfo();
    inkpod::renderer::CanvasDocumentBounds spray_bounds{};
    if (!QueryDocument(state, before_spray)
        || SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&spray_bounds)) != 1) {
        return 608;
    }
    state.tool = kInteractionM6Airbrush;
    state.m6_options.parameters = {4000, 1000, 1000, 750, 0};
    state.m6_options.option = true;
    state.m6_options.option2 = true;
    const InkpodStrokeSample spray_sample{
        sizeof(InkpodStrokeSample),
        0U,
        static_cast<float>((spray_bounds.left + spray_bounds.right) / 2.0),
        static_cast<float>((spray_bounds.top + spray_bounds.bottom) / 2.0),
        1.0F,
        0U};
    const inkpod::renderer::CanvasStrokeEvent spray_begin{
        inkpod::renderer::CanvasStrokeEventKind::Begin, &spray_sample, 1U};
    const inkpod::renderer::CanvasStrokeEvent spray_end{
        inkpod::renderer::CanvasStrokeEventKind::End, &spray_sample, 1U};
    if (SendMessageW(
            state.window,
            inkpod::renderer::kCanvasStrokeReady,
            0,
            reinterpret_cast<LPARAM>(&spray_begin)) != 1
        || SendMessageW(state.window, WM_TIMER, kM6ContinuousSprayTimer, 0) != 0
        || state.m6_samples.size() < 2U
        || SendMessageW(
               state.window,
               inkpod::renderer::kCanvasStrokeReady,
               0,
               reinterpret_cast<LPARAM>(&spray_end)) != 1
        || state.engine->Invoke(
               [](InkpodCore*) { return INKPOD_STATUS_OK; }, false, false)
            != INKPOD_STATUS_OK
        || !QueryDocument(state, after_spray)
        || after_spray.color_plane_checksum == before_spray.color_plane_checksum) {
        return 609;
    }
    return SendMessageW(
               state.canvas,
               inkpod::renderer::kCanvasRenderOnce,
               0,
               0) == 1
        ? 0
        : 604;
}

InkpodStatus InitializeCore(AppState& state) noexcept {
    try {
        state.engine = std::make_unique<inkpod::app::CoreEngine>();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Start(
        inkpod::renderer::GetCanvasSnapshotSink(state.canvas), state.window);
    return status;
}

InkpodStatus ShutdownCore(AppState& state) noexcept {
    const InkpodStatus clipboard_status =
        inkpod_clipboard_release(&state.clipboard);
    if (state.m6_task != nullptr) {
        inkpod_m6_task_cancel(state.m6_task);
    }
    if (state.engine != nullptr) {
        state.engine->Stop();
        state.engine.reset();
    }
    if (state.m6_progress != nullptr) {
        DestroyWindow(state.m6_progress);
        state.m6_progress = nullptr;
    }
    const InkpodStatus task_status = inkpod_m6_task_release(&state.m6_task);
    return clipboard_status == INKPOD_STATUS_OK ? task_status : clipboard_status;
}

LRESULT CALLBACK TreeListSubclassProcedure(
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam,
    UINT_PTR subclass_id,
    DWORD_PTR reference_data) noexcept {
    auto* state = reinterpret_cast<AppState*>(reference_data);
    if (state == nullptr) {
        return DefSubclassProc(window, message, wparam, lparam);
    }
    if (message == WM_LBUTTONDOWN) {
        const LRESULT result = DefSubclassProc(window, message, wparam, lparam);
        SetCapture(window);
        return result;
    }
    if (message == WM_LBUTTONUP && GetCapture() == window) {
        const LRESULT hit = SendMessageW(window, LB_ITEMFROMPOINT, 0, lparam);
        const std::uint32_t target = LOWORD(hit);
        const bool outside = HIWORD(hit) != 0U;
        const bool plane = subclass_id == 2U;
        const std::uint32_t start = plane
            ? state->active_tree_plane_index
            : state->active_tree_layer_index;
        ReleaseCapture();
        const LRESULT count = SendMessageW(window, LB_GETCOUNT, 0, 0);
        if (!outside && count > 0 && target < static_cast<std::uint32_t>(count)
            && target != start) {
            const UINT command = target < start
                ? (plane ? IDM_PLANE_MOVE_UP : IDM_LAYER_MOVE_UP)
                : (plane ? IDM_PLANE_MOVE_DOWN : IDM_LAYER_MOVE_DOWN);
            const std::uint32_t steps = target < start ? start - target : target - start;
            for (std::uint32_t step = 0U; step < steps; ++step) {
                SendMessageW(state->window, WM_COMMAND, command, 0);
            }
        }
        return 0;
    }
    if (message == WM_KEYDOWN && wparam == VK_ESCAPE && GetCapture() == window) {
        ReleaseCapture();
        return 0;
    }
    if (message == WM_NCDESTROY) {
        RemoveWindowSubclass(window, TreeListSubclassProcedure, subclass_id);
    }
    return DefSubclassProc(window, message, wparam, lparam);
}

bool CreateMainChrome(AppState& state) noexcept {
    const DWORD visible = state.smoke_test ? 0U : WS_VISIBLE;
    state.toolbar = CreateWindowExW(
        0,
        TOOLBARCLASSNAMEW,
        nullptr,
        WS_CHILD | visible | TBSTYLE_FLAT | TBSTYLE_TOOLTIPS | TBSTYLE_LIST
            | CCS_NODIVIDER,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_TOOLBAR)),
        state.instance,
        nullptr);
    if (state.toolbar == nullptr) {
        return false;
    }
    SendMessageW(state.toolbar, TB_BUTTONSTRUCTSIZE, sizeof(TBBUTTON), 0);
    SendMessageW(state.toolbar, TB_SETEXTENDEDSTYLE, 0, TBSTYLE_EX_MIXEDBUTTONS);
    constexpr wchar_t labels[] =
        L"保存\0全体\0等倍\0範囲\0左右\0上下\0ルーラー\0ガイド\0グリッド\0透明\0";
    const LRESULT first_string = SendMessageW(
        state.toolbar,
        TB_ADDSTRINGW,
        0,
        reinterpret_cast<LPARAM>(labels));
    if (first_string < 0) {
        return false;
    }
    const std::array<UINT, 10U> commands{
        IDM_FILE_SAVE,
        IDM_VIEW_FIT,
        IDM_VIEW_ONE_TO_ONE,
        IDM_VIEW_BOX_ZOOM,
        IDM_VIEW_FLIP_HORIZONTAL,
        IDM_VIEW_FLIP_VERTICAL,
        IDM_VIEW_RULER,
        IDM_VIEW_GUIDES,
        IDM_VIEW_GRID,
        IDM_VIEW_TRANSPARENT};
    std::array<TBBUTTON, commands.size()> buttons{};
    for (std::size_t index = 0; index < buttons.size(); ++index) {
        buttons[index].iBitmap = I_IMAGENONE;
        buttons[index].idCommand = static_cast<int>(commands[index]);
        buttons[index].fsState = TBSTATE_ENABLED;
        buttons[index].fsStyle = static_cast<BYTE>(
            BTNS_BUTTON | BTNS_AUTOSIZE | BTNS_SHOWTEXT
            | (index >= 3U ? BTNS_CHECK : 0U));
        buttons[index].iString = first_string + static_cast<LRESULT>(index);
    }
    if (SendMessageW(
            state.toolbar,
            TB_ADDBUTTONS,
            static_cast<WPARAM>(buttons.size()),
            reinterpret_cast<LPARAM>(buttons.data())) == FALSE) {
        return false;
    }
    state.zoom_slider = CreateWindowExW(
        0,
        TRACKBAR_CLASSW,
        nullptr,
        WS_CHILD | visible | TBS_HORZ | TBS_AUTOTICKS | TBS_TOOLTIPS,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_ZOOM_SLIDER)),
        state.instance,
        nullptr);
    state.status_bar = CreateWindowExW(
        0,
        STATUSCLASSNAMEW,
        nullptr,
        WS_CHILD | visible | SBARS_SIZEGRIP,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_STATUS)),
        state.instance,
        nullptr);
    state.document_tabs = CreateWindowExW(
        0,
        WC_TABCONTROLW,
        nullptr,
        WS_CHILD | visible | WS_CLIPSIBLINGS | WS_TABSTOP,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_DOCUMENT_TABS)),
        state.instance,
        nullptr);
    state.locator_label = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"STATIC",
        L"カラーロケーター\r\nX: —  Y: —\r\nH: —  V: —  L: —\r\nRGBA: —",
        WS_CHILD | visible | SS_LEFT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LOCATOR)),
        state.instance,
        nullptr);
    state.layer_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LAYER_LIST)),
        state.instance,
        nullptr);
    state.plane_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_PLANE_LIST)),
        state.instance,
        nullptr);
    state.light_table_set_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LT_SET_LIST)),
        state.instance,
        nullptr);
    state.light_table_item_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_LT_ITEM_LIST)),
        state.instance,
        nullptr);
    state.sequence_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_SEQUENCE_LIST)),
        state.instance,
        nullptr);
    state.motion_label = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"STATIC",
        L"モーション停止",
        WS_CHILD | visible | SS_LEFT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_MOTION_LABEL)),
        state.instance,
        nullptr);
    state.color_palette_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_COLOR_PALETTE)),
        state.instance,
        nullptr);
    state.color_chart_list = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        L"LISTBOX",
        nullptr,
        WS_CHILD | visible | WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT,
        0,
        0,
        0,
        0,
        state.window,
        reinterpret_cast<HMENU>(static_cast<INT_PTR>(IDC_MAIN_COLOR_CHART)),
        state.instance,
        nullptr);
    if (state.zoom_slider == nullptr || state.status_bar == nullptr
        || state.document_tabs == nullptr || state.locator_label == nullptr
        || state.layer_list == nullptr || state.plane_list == nullptr
        || state.light_table_set_list == nullptr
        || state.light_table_item_list == nullptr || state.sequence_list == nullptr
        || state.motion_label == nullptr || state.color_palette_list == nullptr
        || state.color_chart_list == nullptr) {
        return false;
    }
    if (SetWindowSubclass(
            state.layer_list,
            TreeListSubclassProcedure,
            1U,
            reinterpret_cast<DWORD_PTR>(&state)) == FALSE
        || SetWindowSubclass(
               state.plane_list,
               TreeListSubclassProcedure,
               2U,
               reinterpret_cast<DWORD_PTR>(&state)) == FALSE) {
        return false;
    }
    TCITEMW primary{};
    primary.mask = TCIF_TEXT | TCIF_PARAM;
    primary.pszText = const_cast<wchar_t*>(L"セル");
    primary.lParam = 0;
    if (TabCtrl_InsertItem(state.document_tabs, 0, &primary) < 0) {
        return false;
    }
    SendMessageW(state.zoom_slider, TBM_SETRANGE, TRUE, MAKELPARAM(1, 800));
    SendMessageW(state.zoom_slider, TBM_SETTICFREQ, 100, 0);
    SendMessageW(state.zoom_slider, TBM_SETPOS, TRUE, 100);
    return true;
}

void LayoutMainChrome(AppState& state, int width, int height) noexcept {
    int toolbar_height{};
    int status_height{};
    if (!state.smoke_test && state.toolbar != nullptr) {
        SendMessageW(state.toolbar, TB_AUTOSIZE, 0, 0);
        RECT bounds{};
        if (GetWindowRect(state.toolbar, &bounds) != FALSE) {
            toolbar_height = bounds.bottom - bounds.top;
        }
        MoveWindow(state.zoom_slider, std::max(0, width - 178), 2, 170, 28, TRUE);
    }
    if (!state.smoke_test && state.status_bar != nullptr) {
        SendMessageW(state.status_bar, WM_SIZE, 0, 0);
        RECT bounds{};
        if (GetWindowRect(state.status_bar, &bounds) != FALSE) {
            status_height = bounds.bottom - bounds.top;
        }
        const std::array<int, 3U> parts{
            std::max(160, width / 4),
            std::max(320, width / 2),
            -1};
        SendMessageW(
            state.status_bar,
            SB_SETPARTS,
            static_cast<WPARAM>(parts.size()),
            reinterpret_cast<LPARAM>(parts.data()));
    }
    int content_left{};
    int content_top = toolbar_height;
    int content_width = width;
    int content_height = std::max(0, height - toolbar_height - status_height);
    if (!state.smoke_test) {
        constexpr int right_pane_width = 238;
        constexpr int tabs_height = 28;
        content_width = std::max(0, width - right_pane_width);
        if (state.document_tabs != nullptr) {
            MoveWindow(
                state.document_tabs,
                content_left,
                content_top,
                content_width,
                tabs_height,
                TRUE);
        }
        if (state.locator_label != nullptr) {
            MoveWindow(
                state.locator_label,
                content_width + 6,
                content_top + 6,
                std::max(0, right_pane_width - 12),
                70,
                TRUE);
        }
        if (state.motion_label != nullptr) {
            MoveWindow(
                state.motion_label,
                content_width + 6,
                content_top + 78,
                std::max(0, right_pane_width - 12),
                28,
                TRUE);
        }
        const int pane_x = content_width + 6;
        const int pane_width = std::max(0, right_pane_width - 12);
        const int lists_top = content_top + 110;
        const int lists_height = std::max(0, content_height - 116);
        const int section = std::max(0, (lists_height - 24) / 7);
        if (state.layer_list != nullptr) {
            MoveWindow(
                state.layer_list,
                pane_x,
                lists_top,
                pane_width,
                section,
                TRUE);
        }
        if (state.plane_list != nullptr) {
            MoveWindow(
                state.plane_list,
                pane_x,
                lists_top + section + 4,
                pane_width,
                section,
                TRUE);
        }
        if (state.light_table_set_list != nullptr) {
            MoveWindow(
                state.light_table_set_list,
                pane_x,
                lists_top + (section + 4) * 2,
                pane_width,
                section,
                TRUE);
        }
        if (state.light_table_item_list != nullptr) {
            MoveWindow(
                state.light_table_item_list,
                pane_x,
                lists_top + (section + 4) * 3,
                pane_width,
                section,
                TRUE);
        }
        if (state.sequence_list != nullptr) {
            MoveWindow(
                state.sequence_list,
                pane_x,
                lists_top + (section + 4) * 4,
                pane_width,
                section,
                TRUE);
        }
        if (state.color_palette_list != nullptr) {
            MoveWindow(
                state.color_palette_list,
                pane_x,
                lists_top + (section + 4) * 5,
                pane_width,
                section,
                TRUE);
        }
        if (state.color_chart_list != nullptr) {
            MoveWindow(
                state.color_chart_list,
                pane_x,
                lists_top + (section + 4) * 6,
                pane_width,
                std::max(0, lists_height - (section + 4) * 6),
                TRUE);
        }
        content_top += tabs_height;
        content_height = std::max(0, content_height - tabs_height);
    }
    if (state.canvas != nullptr) {
        MoveWindow(
            state.canvas,
            content_left,
            content_top,
            content_width,
            content_height,
            TRUE);
    }
}

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AppState*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        state = static_cast<AppState*>(create->lpCreateParams);
        state->window = window;
        SetWindowLongPtrW(
            window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
    }

    switch (message) {
        case WM_CREATE:
            if (state == nullptr) {
                return -1;
            }
            state->canvas = inkpod::renderer::CreateCanvasWindow(
                state->instance, window);
            if (state->canvas == nullptr || !CreateMainChrome(*state)
                || SetTimer(
                       window,
                       kAutosaveTimer,
                       kAutosaveIntervalMilliseconds,
                       nullptr) == 0U) {
                return -1;
            }
            return 0;
        case WM_SIZE:
            if (state != nullptr) {
                LayoutMainChrome(*state, LOWORD(lparam), HIWORD(lparam));
            }
            return 0;
        case WM_NOTIFY:
            if (state != nullptr && state->document_tabs != nullptr) {
                const auto* notification = reinterpret_cast<const NMHDR*>(lparam);
                if (notification != nullptr
                    && notification->hwndFrom == state->document_tabs
                    && notification->code == TCN_SELCHANGE) {
                    const int selected = TabCtrl_GetCurSel(state->document_tabs);
                    TCITEMW item{};
                    item.mask = TCIF_PARAM;
                    if (selected >= 0
                        && TabCtrl_GetItem(state->document_tabs, selected, &item) != FALSE) {
                        state->active_view_id = static_cast<std::uint64_t>(item.lParam);
                        if (state->engine != nullptr) {
                            state->engine->SetActiveView(state->active_view_id);
                        }
                        InkpodSnapshotTransform transform{};
                        if (QuerySnapshotTransform(*state, transform)) {
                            state->view_flip_horizontal = (transform.flags
                                & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) != 0U;
                            state->view_flip_vertical = (transform.flags
                                & INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL) != 0U;
                        }
                        UpdateMenuState(*state);
                    }
                    return 0;
                }
            }
            break;
        case WM_DPICHANGED: {
            const auto* bounds = reinterpret_cast<const RECT*>(lparam);
            SetWindowPos(
                window,
                nullptr,
                bounds->left,
                bounds->top,
                bounds->right - bounds->left,
                bounds->bottom - bounds->top,
                SWP_NOACTIVATE | SWP_NOZORDER);
            return 0;
        }
        case WM_HSCROLL:
            if (state != nullptr && state->zoom_slider != nullptr
                && reinterpret_cast<HWND>(lparam) == state->zoom_slider) {
                const int position = static_cast<int>(SendMessageW(
                    state->zoom_slider, TBM_GETPOS, 0, 0));
                if (position > 0
                    && ApplyZoomPercent(*state, static_cast<std::uint32_t>(position))
                        == INKPOD_STATUS_OK) {
                    UpdateMenuState(*state);
                }
                return 0;
            }
            if (state != nullptr && wparam == kMotionPlaybackTimer) {
                if (state->motion_active && !state->motion_paused && state->engine != nullptr) {
                    InkpodMotionFrame frame{};
                    frame.struct_size = sizeof(frame);
                    const InkpodStatus status = state->engine->Invoke(
                        [&frame](InkpodCore* core) {
                            return inkpod_core_motion_check_step(
                                core, INKPOD_SEQUENCE_NEXT, &frame);
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        UpdateMotionLabel(*state, frame);
                    } else {
                        state->motion_active = false;
                        KillTimer(window, kMotionPlaybackTimer);
                        if (!state->smoke_test) {
                            ShowCoreError(*state, window, L"モーション再生");
                        }
                    }
                }
                return 0;
            }
            break;
        case WM_COMMAND:
            if (state == nullptr) {
                break;
            }
            switch (LOWORD(wparam)) {
                case IDC_MAIN_LAYER_LIST:
                    if (HIWORD(wparam) == LBN_SELCHANGE && state->layer_list != nullptr) {
                        const LRESULT selected = SendMessageW(
                            state->layer_list, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR) {
                            const LRESULT id = SendMessageW(
                                state->layer_list,
                                LB_GETITEMDATA,
                                static_cast<WPARAM>(selected),
                                0);
                            if (id != LB_ERR) {
                                state->active_tree_layer_id = static_cast<std::uint64_t>(id);
                                state->active_tree_layer_index = static_cast<std::uint32_t>(selected);
                                state->active_tree_plane_id = 0U;
                                RefreshTreePane(*state);
                                if (state->active_tree_plane_id != 0U && state->engine != nullptr) {
                                    const std::uint64_t layer_id = state->active_tree_layer_id;
                                    const std::uint64_t plane_id = state->active_tree_plane_id;
                                    state->engine->Invoke(
                                        [layer_id, plane_id](InkpodCore* core) {
                                            return inkpod_core_set_active_node(
                                                core, layer_id, plane_id);
                                        },
                                        false,
                                        false);
                                }
                            }
                        }
                    }
                    UpdateMenuState(*state);
                    return 0;
                case IDC_MAIN_PLANE_LIST:
                    if (HIWORD(wparam) == LBN_SELCHANGE && state->plane_list != nullptr) {
                        const LRESULT selected = SendMessageW(
                            state->plane_list, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR) {
                            const LRESULT id = SendMessageW(
                                state->plane_list,
                                LB_GETITEMDATA,
                                static_cast<WPARAM>(selected),
                                0);
                            if (id != LB_ERR && state->engine != nullptr) {
                                state->active_tree_plane_id = static_cast<std::uint64_t>(id);
                                state->active_tree_plane_index =
                                    static_cast<std::uint32_t>(selected);
                                const std::uint64_t layer_id = state->active_tree_layer_id;
                                const std::uint64_t plane_id = state->active_tree_plane_id;
                                const InkpodStatus status = state->engine->Invoke(
                                    [layer_id, plane_id](InkpodCore* core) {
                                        return inkpod_core_set_active_node(
                                            core, layer_id, plane_id);
                                    },
                                    false,
                                    false);
                                if (status != INKPOD_STATUS_OK) {
                                    ShowCoreError(*state, window, L"プレーン選択");
                                }
                            }
                        }
                    }
                    UpdateMenuState(*state);
                    return 0;
                case IDC_MAIN_LT_SET_LIST:
                    if (HIWORD(wparam) == LBN_SELCHANGE
                        && state->light_table_set_list != nullptr) {
                        const LRESULT selected = SendMessageW(
                            state->light_table_set_list, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR) {
                            const LRESULT id = SendMessageW(
                                state->light_table_set_list,
                                LB_GETITEMDATA,
                                static_cast<WPARAM>(selected),
                                0);
                            if (id != LB_ERR) {
                                InkpodLightTableEdit edit{};
                                edit.operation = INKPOD_LIGHT_TABLE_SET_ACTIVE_OPERATION;
                                edit.object_id = static_cast<std::uint64_t>(id);
                                std::uint64_t ignored{};
                                if (ApplyLightTableEdit(*state, edit, {}, ignored)
                                    == INKPOD_STATUS_OK) {
                                    state->active_light_table_set_id = edit.object_id;
                                    state->active_light_table_set_index =
                                        static_cast<std::uint32_t>(selected);
                                    state->active_light_table_item_id = 0U;
                                    RefreshLightTablePane(*state);
                                }
                            }
                        }
                    }
                    return 0;
                case IDC_MAIN_LT_ITEM_LIST:
                    if (HIWORD(wparam) == LBN_SELCHANGE
                        && state->light_table_item_list != nullptr) {
                        const LRESULT selected = SendMessageW(
                            state->light_table_item_list, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR) {
                            const LRESULT id = SendMessageW(
                                state->light_table_item_list,
                                LB_GETITEMDATA,
                                static_cast<WPARAM>(selected),
                                0);
                            if (id != LB_ERR) {
                                state->active_light_table_item_id =
                                    static_cast<std::uint64_t>(id);
                                state->active_light_table_item_index =
                                    static_cast<std::uint32_t>(selected);
                            }
                        }
                    }
                    return 0;
                case IDC_MAIN_SEQUENCE_LIST:
                    if ((HIWORD(wparam) == LBN_SELCHANGE || HIWORD(wparam) == LBN_DBLCLK)
                        && state->sequence_list != nullptr) {
                        const LRESULT selected = SendMessageW(
                            state->sequence_list, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR) {
                            state->active_sequence_index =
                                static_cast<std::uint32_t>(selected);
                            if (HIWORD(wparam) == LBN_DBLCLK && state->engine != nullptr) {
                                InkpodDocumentInfo info{};
                                const std::uint32_t index = state->active_sequence_index;
                                const InkpodStatus status = state->engine->Invoke(
                                    [index, &info](InkpodCore* core) {
                                        info = EmptyDocumentInfo();
                                        return inkpod_core_sequence_activate(core, index, &info);
                                    },
                                    false,
                                    false);
                                if (status == INKPOD_STATUS_OK) {
                                    ResetM3DocumentUiState(*state);
                                    state->active_sequence_index = index;
                                    FitCanvas(*state, INKPOD_VIEW_FIT);
                                    RefreshSequencePane(*state);
                                } else {
                                    ShowCoreError(*state, window, L"連番セル切替");
                                }
                            }
                        }
                    }
                    return 0;
                case IDC_MAIN_COLOR_PALETTE:
                case IDC_MAIN_COLOR_CHART:
                    if (HIWORD(wparam) == LBN_DBLCLK) {
                        const bool chart = LOWORD(wparam) == IDC_MAIN_COLOR_CHART;
                        if (chart && state->color_chart_locked) {
                            return 0;
                        }
                        HWND list = chart ? state->color_chart_list : state->color_palette_list;
                        const LRESULT selected = SendMessageW(list, LB_GETCURSEL, 0, 0);
                        if (selected != LB_ERR) {
                            const std::size_t index = chart
                                ? static_cast<std::size_t>(state->color_chart_page) * 20U
                                    + static_cast<std::size_t>(selected)
                                : static_cast<std::size_t>(state->palette_group) * 10U
                                    + static_cast<std::size_t>(selected);
                            if (index < state->palette_colors.size()) {
                                SetDrawingColor(*state, state->palette_colors[index]);
                            }
                        }
                    }
                    return 0;
                case IDM_FILE_NEW:
                    if (ConfirmDiscard(*state)) {
                        ViewOptionsDialogState dialog{};
                        dialog.title = L"新規セル";
                        dialog.labels = {L"幅 (px)", L"高さ (px)", L"DPI", L"レイヤー種別"};
                        dialog.values = {1920, 1080, 96, INKPOD_LAYER_BINARY_COLORING};
                        dialog.value_count = 4U;
                        if (ShowViewOptions(
                                state->instance,
                                window,
                                state->smoke_test,
                                dialog) != IDOK
                            || dialog.values[0] <= 0 || dialog.values[1] <= 0
                            || dialog.values[2] <= 0) {
                            return 0;
                        }
                        InkpodStatus status = CreateCell(
                            *state,
                            static_cast<std::uint32_t>(dialog.values[0]),
                            static_cast<std::uint32_t>(dialog.values[1]),
                            static_cast<std::uint32_t>(dialog.values[2]) * 1000U);
                        if (status == INKPOD_STATUS_OK
                            && dialog.values[3] != INKPOD_LAYER_BINARY_COLORING) {
                            InkpodDocumentInfo info{};
                            if (QueryDocument(*state, info)) {
                                InkpodTreeEdit edit{};
                                edit.struct_size = sizeof(edit);
                                edit.operation = INKPOD_TREE_CONVERT_LAYER;
                                edit.object_id = info.layer_id;
                                edit.kind = static_cast<std::uint32_t>(dialog.values[3]);
                                status = state->engine->Invoke(
                                    [edit](InkpodCore* core) {
                                        InkpodDispatchResult result{};
                                        result.struct_size = sizeof(result);
                                        std::uint64_t ignored{};
                                        return inkpod_core_tree_edit(
                                            core, &edit, &result, &ignored);
                                    },
                                    true,
                                    true);
                            }
                        }
                        if (status != INKPOD_STATUS_OK) {
                            ShowCoreError(*state, window, L"新規セルの作成");
                        }
                        UpdateMenuState(*state);
                    }
                    return 0;
                case IDM_CELL_PAPER_SETTINGS:
                case IDM_CELL_IMAGE_SIZE:
                case IDM_CELL_RESOLUTION: {
                    const UINT command = LOWORD(wparam);
                    const InkpodStatus status = ResizeDocumentFromDialog(
                        *state,
                        command == IDM_CELL_PAPER_SETTINGS
                            ? L"用紙設定"
                            : (command == IDM_CELL_IMAGE_SIZE ? L"画像サイズ" : L"画像解像度"),
                        command == IDM_CELL_RESOLUTION);
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"用紙・画像サイズ変更");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_CELL_FIT_CAPTURE_FRAME: {
                    const InkpodStatus status = FitPaperToCaptureFrame(*state);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"撮影フレームに用紙を合わせる");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_CELL_MIRROR_VERTICAL:
                case IDM_CELL_ROTATE_LEFT:
                case IDM_CELL_ROTATE_RIGHT: {
                    const UINT command = LOWORD(wparam);
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [command](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  if (command == IDM_CELL_MIRROR_VERTICAL) {
                                      return inkpod_core_mirror_document(
                                          core, INKPOD_MIRROR_VERTICAL, &result);
                                  }
                                  return inkpod_core_rotate_document(
                                      core,
                                      command == IDM_CELL_ROTATE_LEFT
                                          ? INKPOD_ROTATE_LEFT_90
                                          : INKPOD_ROTATE_RIGHT_90,
                                      &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"画像全体の変形");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_CELL_FRAME_HUNDRED:
                case IDM_CELL_FRAME_REFERENCE:
                case IDM_CELL_FRAME_DRAWING:
                case IDM_CELL_FRAME_SAFE:
                case IDM_CELL_MARGINS: {
                    const InkpodStatus status = EditPaperFrames(*state, LOWORD(wparam));
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"フレーム設定");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILE_OPEN:
                    if (ConfirmDiscard(*state)) {
                        std::wstring path;
                        if (ChooseOpenDocumentPath(window, path)) {
                            if (CommonRasterFormatFromPath(path) != 0U) {
                                const InkpodStatus status = ImportCommonRasterFromPath(*state, path);
                                if (status != INKPOD_STATUS_OK) {
                                    ShowCoreError(*state, window, L"一般画像を開く");
                                }
                                UpdateMenuState(*state);
                                return 0;
                            }
                            std::wstring recovery;
                            try {
                                recovery = path + L".recovery.inkpod";
                            } catch (const std::bad_alloc&) {
                                ShowCoreError(*state, window, L"Recovery path の作成");
                                return 0;
                            }
                            InkpodStatus status = INKPOD_STATUS_OK;
                            if (RecoveryIsNewer(path, recovery)) {
                                const int choice = MessageBoxW(
                                    window,
                                    L"通常保存より新しいRecoveryがあります。\n\n"
                                    L"はい: Recoveryを開く\nいいえ: Recoveryを破棄\n"
                                    L"キャンセル: 後で判断して通常保存を開く",
                                    L"inkpod Recovery",
                                    MB_YESNOCANCEL | MB_ICONQUESTION);
                                if (choice == IDYES) {
                                    status = OpenRecoveryFromPath(*state, recovery);
                                } else {
                                    if (choice == IDNO) {
                                        DeleteFileW(recovery.c_str());
                                    }
                                    status = OpenFromPath(*state, path);
                                }
                            } else {
                                status = OpenFromPath(*state, path);
                            }
                            if (status != INKPOD_STATUS_OK) {
                                ShowCoreError(*state, window, L"開く");
                            }
                        }
                    }
                    return 0;
                case IDM_FILE_IMPORT_RASTER: {
                    if (!ConfirmDiscard(*state)) {
                        return 0;
                    }
                    std::wstring path = state->smoke_test ? state->smoke_raster_path : L"";
                    if (!state->smoke_test && !ChooseCommonRasterPath(window, false, path)) {
                        return 0;
                    }
                    const InkpodStatus status = ImportCommonRasterFromPath(*state, path);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"一般画像の読み込み");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_FILE_EXPORT_RASTER: {
                    std::wstring path = state->smoke_test ? state->smoke_raster_path : L"";
                    if (!state->smoke_test && !ChooseCommonRasterPath(window, true, path)) {
                        return 0;
                    }
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"ラスター書き出し";
                    dialog.labels[0] = L"白背景を合成 (0/1)";
                    dialog.values[0] = state->smoke_test ? 1 : 0;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    const InkpodStatus status = ExportCommonRasterToPath(
                        *state, path, dialog.values[0] != 0);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"一般画像の書き出し");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_FILE_SAVE:
                case IDM_FILE_SAVE_AS: {
                    const InkpodStatus status = SaveDocument(
                        *state, LOWORD(wparam) == IDM_FILE_SAVE_AS);
                    if (status != INKPOD_STATUS_OK
                        && status != INKPOD_STATUS_INVALID_STATE) {
                        ShowCoreError(*state, window, L"保存");
                    }
                    return 0;
                }
                case IDM_FILE_REVERT: {
                    const InkpodStatus revert_status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  InkpodDocumentInfo info = EmptyDocumentInfo();
                                  return inkpod_core_revert(core, &info);
                              },
                              false,
                              false);
                    if (revert_status != INKPOD_STATUS_OK
                        || FitCanvas(*state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"復帰");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILE_REVERT_PARTIAL: {
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_revert_active_selection(
                                      core, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの部分復帰");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILE_AUTOSAVE_NOW: {
                    std::wstring path = state->recovery_path;
                    if (path.empty() && !ChooseInkpodPath(window, true, path)) {
                        return 0;
                    }
                    if (!QueueAutosave(*state, path)) {
                        ShowCoreError(*state, window, L"Recovery保存の予約");
                    } else {
                        try {
                            state->recovery_path = path;
                        } catch (const std::bad_alloc&) {
                            ShowCoreError(*state, window, L"Recovery path の保持");
                        }
                    }
                    return 0;
                }
                case IDM_FILE_OPEN_RECOVERY: {
                    if (ConfirmDiscard(*state)) {
                        std::wstring path = state->recovery_path;
                        if (ChooseInkpodPath(window, false, path)
                            && OpenRecoveryFromPath(*state, path) != INKPOD_STATUS_OK) {
                            ShowCoreError(*state, window, L"Recoveryを開く");
                        }
                    }
                    return 0;
                }
                case IDM_EDIT_UNDO:
                case IDM_EDIT_REDO: {
                    const bool redo = LOWORD(wparam) == IDM_EDIT_REDO;
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [redo](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return redo ? inkpod_core_redo(core, &result)
                                              : inkpod_core_undo(core, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"履歴操作");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_EDIT_HISTORY_BACK:
                case IDM_EDIT_HISTORY_FORWARD: {
                    const bool forward = LOWORD(wparam) == IDM_EDIT_HISTORY_FORWARD;
                    HistoryDialogState dialog{};
                    if (!ConfigureHistoryDialog(*state, forward, dialog)
                        || ShowHistoryDialog(state->instance, window, dialog) != IDOK) {
                        return 0;
                    }
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [target = dialog.selected_cursor](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_history_jump(
                                      core, target, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"複数段階の履歴移動");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_EDIT_COPY: {
                    InkpodClipboard* replacement{};
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [&replacement](InkpodCore* core) {
                                  return inkpod_core_clipboard_copy(
                                      core, &replacement);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"コピー");
                    } else {
                        inkpod_clipboard_release(&state->clipboard);
                        state->clipboard = replacement;
                        if (!PublishStandardClipboard(window, state->clipboard)
                            && !state->smoke_test) {
                            MessageBoxW(
                                window,
                                L"アプリ内コピーは完了しましたが、Windowsクリップボードへ公開できませんでした。",
                                L"inkpod",
                                MB_OK | MB_ICONWARNING);
                        }
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_EDIT_CUT: {
                    SendMessageW(window, WM_COMMAND, IDM_EDIT_COPY, 0);
                    const InkpodStatus status = state->clipboard == nullptr
                        || state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_clear_selected_content(core, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"カット");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_EDIT_PASTE:
                case IDM_EDIT_PASTE_SELECTED: {
                    const InkpodStatus status = BeginFloatingPaste(
                        *state,
                        LOWORD(wparam) == IDM_EDIT_PASTE
                            ? INKPOD_PASTE_COMPATIBLE
                            : INKPOD_PASTE_ACTIVE_CONVERTED);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"貼り付け開始");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_EDIT_PASTE_CONVERTED: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"変換してペースト";
                    dialog.labels = {L"プレーン種類 (1-7)", L"色深度/形式 (1-5)", L"不透明度 (%)", L"新規プレーン (1)"};
                    dialog.values = {INKPOD_TYPED_PLANE_RASTER, INKPOD_STORAGE_RGBA8, 100, 1};
                    dialog.value_count = 4U;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    TextInputDialogState name{};
                    name.title = L"変換先プレーン";
                    name.label = L"名前";
                    name.value = L"Pasted Plane";
                    if (ShowTextInput(
                            state->instance, window, state->smoke_test, name) != IDOK) {
                        return 0;
                    }
                    std::vector<std::uint8_t> utf8;
                    if (!WidePathToUtf8(name.value, utf8)) {
                        return 0;
                    }
                    InkpodTreeEdit edit{};
                    edit.struct_size = sizeof(edit);
                    edit.operation = INKPOD_TREE_CREATE_PLANE;
                    edit.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
                    edit.parent_id = state->active_tree_layer_id;
                    edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
                    edit.pixel_format = static_cast<std::uint32_t>(dialog.values[1]);
                    edit.opacity_milli = static_cast<std::uint32_t>(
                        std::clamp(dialog.values[2], 0, 100)) * 10U;
                    std::uint64_t plane_id{};
                    InkpodStatus status = INKPOD_STATUS_INVALID_STATE;
                    try {
                        const std::string plane_name(
                            reinterpret_cast<const char*>(utf8.data()), utf8.size());
                        status = ApplyTreeEditRecord(*state, edit, plane_name, plane_id);
                    } catch (const std::bad_alloc&) {
                        status = INKPOD_STATUS_INVALID_STATE;
                    }
                    if (status == INKPOD_STATUS_OK) {
                        state->active_tree_plane_id = plane_id;
                        RefreshTreePane(*state);
                        status = BeginFloatingPaste(*state, INKPOD_PASTE_ACTIVE_CONVERTED);
                    }
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"変換してペースト");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_EDIT_FLOATING_TRANSFORM: {
                    const InkpodStatus status = ShowFloatingTransformDialog(*state);
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"フローティング変形");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_EDIT_FLOATING_COMMIT:
                case IDM_EDIT_FLOATING_CANCEL: {
                    const bool commit = LOWORD(wparam) == IDM_EDIT_FLOATING_COMMIT;
                    const InkpodStatus status = EndFloatingPaste(*state, commit);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, commit ? L"貼り付け確定" : L"貼り付け取消");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_EDIT_MIRROR_HORIZONTAL: {
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_mirror_document(
                                      core,
                                      INKPOD_MIRROR_HORIZONTAL,
                                      &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"画像の左右反転");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILTER_LAST: {
                    InkpodDocumentInfo document{};
                    const InkpodStatus status = state->plane != INKPOD_PLANE_COLOR
                            || !QueryDocument(*state, document)
                        ? INKPOD_STATUS_INVALID_STATE
                        : StartM6Task(
                              *state,
                              false,
                              [plane_id = document.color_plane_id](
                                  InkpodCore* core, InkpodM6Task* task) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_filter_apply_last_task(
                                      core, plane_id, task, &result);
                              });
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"直前のフィルタ");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_FILTER_INVERT:
                case IDM_FILTER_BLUR_WEAK:
                case IDM_FILTER_SHARPEN_WEAK:
                case IDM_FILTER_SHARPEN_STRONG:
                case IDM_FILTER_BLUR_STRONG:
                case IDM_FILTER_GAUSSIAN:
                case IDM_FILTER_AUTO_CONTRAST:
                case IDM_FILTER_BRIGHTNESS:
                case IDM_FILTER_TONE_CURVE:
                case IDM_FILTER_LEVELS:
                case IDM_FILTER_HSV:
                case IDM_FILTER_COLOR_BALANCE:
                case IDM_FILTER_UNSHARP: {
                    const UINT command = LOWORD(wparam);
                    InkpodDocumentInfo document{};
                    M6FilterJob job{};
                    if (state->plane != INKPOD_PLANE_COLOR
                        || !QueryDocument(*state, document)
                        || !ConfigureM6FilterEditor(*state, command, job)) {
                        return 0;
                    }
                    job.plane_id = document.color_plane_id;
                    const InkpodStatus status = QueueM6Filter(*state, std::move(job));
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"フィルタ");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_ADJUSTMENT_CREATE:
                case IDM_ADJUSTMENT_EDIT: {
                    const bool update = LOWORD(wparam) == IDM_ADJUSTMENT_EDIT;
                    M6FilterJob job{};
                    if (!ConfigureM6AdjustmentEditor(*state, job, update)) {
                        return 0;
                    }
                    const InkpodStatus status = CreateOrUpdateM6Adjustment(
                        *state, std::move(job), update);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"調整レイヤー");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_ADJUSTMENT_PREVIOUS:
                case IDM_ADJUSTMENT_NEXT:
                    SelectM6Adjustment(
                        *state, LOWORD(wparam) == IDM_ADJUSTMENT_NEXT);
                    UpdateMenuState(*state);
                    return 0;
                case IDM_ADJUSTMENT_TOGGLE: {
                    const bool visible = !state->m6_adjustment_visible;
                    const InkpodStatus status = SetM6AdjustmentVisibility(*state, visible);
                    if (status == INKPOD_STATUS_OK) {
                        state->m6_adjustment_visible = visible;
                    } else {
                        ShowCoreError(*state, window, L"調整レイヤー表示");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_ADJUSTMENT_MOVE_TOP: {
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyTreeEdit(
                        *state,
                        INKPOD_TREE_REORDER_LAYER,
                        state->m6_adjustment_id,
                        0U,
                        ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"調整レイヤー移動");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_NEW: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"新規レイヤー";
                    dialog.labels = {L"種類 (1-10)", L"不透明度 (%)", nullptr, nullptr};
                    dialog.values = {INKPOD_LAYER_RASTER, 100, 0, 0};
                    dialog.value_count = 2U;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] < INKPOD_LAYER_BINARY_COLORING
                        || dialog.values[0] > INKPOD_LAYER_VECTOR_COLORING
                        || dialog.values[1] < 0 || dialog.values[1] > 100) {
                        return 0;
                    }
                    InkpodTreeEdit edit{};
                    edit.struct_size = sizeof(edit);
                    edit.operation = INKPOD_TREE_CREATE_LAYER;
                    edit.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
                    edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
                    edit.opacity_milli = static_cast<std::uint32_t>(dialog.values[1]) * 10U;
                    std::uint64_t layer_id{};
                    const InkpodStatus status = ApplyTreeEditRecord(
                        *state, edit, "Layer", layer_id);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの作成");
                    } else {
                        state->active_tree_layer_id = layer_id;
                        state->m3_layer_id = layer_id;
                        RefreshTreePane(*state);
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_DUPLICATE: {
                    InkpodDocumentInfo info{};
                    std::uint64_t duplicate_id{};
                    const std::uint64_t source_id = state->active_tree_layer_id != 0U
                        ? state->active_tree_layer_id
                        : (QueryDocument(*state, info) ? info.layer_id : 0U);
                    const InkpodStatus status = source_id != 0U
                        ? ApplyTreeEdit(
                              *state,
                              INKPOD_TREE_DUPLICATE_LAYER,
                              source_id,
                              0U,
                              duplicate_id)
                        : INKPOD_STATUS_INVALID_STATE;
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの複製");
                    } else {
                        state->m3_layer_id = duplicate_id;
                        state->active_tree_layer_id = duplicate_id;
                        RefreshTreePane(*state);
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_DELETE: {
                    std::uint64_t ignored{};
                    const std::uint64_t target = state->m3_layer_id != 0U
                        ? state->m3_layer_id
                        : state->active_tree_layer_id;
                    const InkpodStatus status = target == 0U
                        ? INKPOD_STATUS_INVALID_STATE
                        : ApplyTreeEdit(
                              *state,
                              INKPOD_TREE_DELETE_LAYER,
                              target,
                              0U,
                              ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの削除");
                    } else {
                        state->m3_layer_id = 0U;
                        state->active_tree_layer_id = 0U;
                        RefreshTreePane(*state);
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_MOVE_TOP: {
                    InkpodDocumentInfo info{};
                    const std::uint64_t target = state->m3_layer_id != 0U
                        ? state->m3_layer_id
                        : (state->active_tree_layer_id != 0U
                                  ? state->active_tree_layer_id
                                  : (QueryDocument(*state, info) ? info.layer_id : 0U));
                    std::uint64_t ignored{};
                    const InkpodStatus status = target == 0U
                        ? INKPOD_STATUS_INVALID_STATE
                        : ApplyTreeEdit(
                              *state,
                              INKPOD_TREE_REORDER_LAYER,
                              target,
                              0U,
                              ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの移動");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_LAYER_MOVE_UP:
                case IDM_LAYER_MOVE_DOWN: {
                    const int count = state->layer_list == nullptr
                        ? 0
                        : static_cast<int>(SendMessageW(state->layer_list, LB_GETCOUNT, 0, 0));
                    const int current = static_cast<int>(state->active_tree_layer_index);
                    const int destination = LOWORD(wparam) == IDM_LAYER_MOVE_UP
                        ? std::max(0, current - 1)
                        : std::min(std::max(0, count - 1), current + 1);
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyTreeEdit(
                        *state,
                        INKPOD_TREE_REORDER_LAYER,
                        state->active_tree_layer_id,
                        static_cast<std::uint32_t>(destination),
                        ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤーの並べ替え");
                    }
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_LAYER_TOGGLE_VISIBLE:
                case IDM_LAYER_TOGGLE_EDITABLE:
                case IDM_LAYER_OPACITY: {
                    const InkpodStatus status = SetSelectedTreeNodeProperties(
                        *state, false, LOWORD(wparam));
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"レイヤープロパティ");
                    }
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_LAYER_PROPERTIES: {
                    const InkpodStatus status = EditSelectedTreeNodeProperties(*state, false);
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"レイヤープロパティ");
                    }
                    RefreshTreePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LAYER_CONVERT: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"レイヤー変換";
                    dialog.labels[0] = L"変換先種類 (1-10)";
                    dialog.values[0] = INKPOD_LAYER_RASTER;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    InkpodTreeEdit edit{};
                    edit.struct_size = sizeof(edit);
                    edit.operation = INKPOD_TREE_CONVERT_LAYER;
                    edit.object_id = state->active_tree_layer_id;
                    edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyTreeEditRecord(*state, edit, {}, ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"レイヤー変換");
                    }
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_LAYER_MERGE: {
                    InkpodTreeEdit edit{};
                    edit.struct_size = sizeof(edit);
                    edit.operation = INKPOD_TREE_MERGE_LAYER;
                    edit.object_id = state->active_tree_layer_id;
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyTreeEditRecord(*state, edit, {}, ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"同種レイヤーの統合");
                    }
                    state->active_tree_layer_id = 0U;
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_LAYER_DELETE_HIDDEN: {
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  std::vector<std::uint64_t> hidden;
                                  try {
                                      for (std::uint32_t index = 0U; index < 1024U; ++index) {
                                          InkpodNodeInfo node{};
                                          node.struct_size = sizeof(node);
                                          if (inkpod_core_node_get(core, index, UINT32_MAX, &node)
                                              != INKPOD_STATUS_OK) {
                                              break;
                                          }
                                          if ((node.flags & INKPOD_NODE_VISIBLE) == 0U) {
                                              hidden.push_back(node.id);
                                          }
                                      }
                                  } catch (const std::bad_alloc&) {
                                      return INKPOD_STATUS_INVALID_STATE;
                                  }
                                  for (const std::uint64_t id : hidden) {
                                      InkpodTreeEdit edit{};
                                      edit.struct_size = sizeof(edit);
                                      edit.operation = INKPOD_TREE_DELETE_LAYER;
                                      edit.object_id = id;
                                      InkpodDispatchResult result{};
                                      result.struct_size = sizeof(result);
                                      std::uint64_t ignored{};
                                      const InkpodStatus item_status = inkpod_core_tree_edit(
                                          core, &edit, &result, &ignored);
                                      if (item_status != INKPOD_STATUS_OK) {
                                          return item_status;
                                      }
                                  }
                                  return INKPOD_STATUS_OK;
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"非表示レイヤーの削除");
                    }
                    state->active_tree_layer_id = 0U;
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_PLANE_NEW: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"新規プレーン";
                    dialog.labels = {L"種類 (1-7)", L"形式 (1-5)", L"不透明度 (%)", nullptr};
                    dialog.values = {INKPOD_TYPED_PLANE_RASTER, INKPOD_STORAGE_RGBA8, 100, 0};
                    dialog.value_count = 3U;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    InkpodTreeEdit edit{};
                    edit.struct_size = sizeof(edit);
                    edit.operation = INKPOD_TREE_CREATE_PLANE;
                    edit.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
                    edit.parent_id = state->active_tree_layer_id;
                    edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
                    edit.pixel_format = static_cast<std::uint32_t>(dialog.values[1]);
                    edit.opacity_milli = static_cast<std::uint32_t>(
                        std::clamp(dialog.values[2], 0, 100)) * 10U;
                    std::uint64_t plane_id{};
                    const InkpodStatus status = ApplyTreeEditRecord(
                        *state, edit, "Plane", plane_id);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"プレーンの作成");
                    } else {
                        state->active_tree_plane_id = plane_id;
                    }
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_PLANE_DUPLICATE:
                case IDM_PLANE_DELETE:
                case IDM_PLANE_MOVE_UP:
                case IDM_PLANE_MOVE_DOWN: {
                    const UINT command = LOWORD(wparam);
                    const InkpodTreeOperation operation = command == IDM_PLANE_DUPLICATE
                        ? INKPOD_TREE_DUPLICATE_PLANE
                        : (command == IDM_PLANE_DELETE ? INKPOD_TREE_DELETE_PLANE
                                                      : INKPOD_TREE_REORDER_PLANE);
                    std::uint32_t destination = state->active_tree_plane_index;
                    if (command == IDM_PLANE_MOVE_UP) {
                        destination = destination == 0U ? 0U : destination - 1U;
                    } else if (command == IDM_PLANE_MOVE_DOWN) {
                        const int count = state->plane_list == nullptr
                            ? 0
                            : static_cast<int>(SendMessageW(
                                  state->plane_list, LB_GETCOUNT, 0, 0));
                        destination = static_cast<std::uint32_t>(std::min(
                            std::max(0, count - 1), static_cast<int>(destination) + 1));
                    }
                    std::uint64_t object_id{};
                    const InkpodStatus status = ApplyTreeEdit(
                        *state,
                        operation,
                        state->active_tree_plane_id,
                        destination,
                        object_id);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"プレーン操作");
                    } else if (command == IDM_PLANE_DUPLICATE) {
                        state->active_tree_plane_id = object_id;
                    } else if (command == IDM_PLANE_DELETE) {
                        state->active_tree_plane_id = 0U;
                    }
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_PLANE_TOGGLE_VISIBLE:
                case IDM_PLANE_TOGGLE_EDITABLE:
                case IDM_PLANE_OPACITY: {
                    const InkpodStatus status = SetSelectedTreeNodeProperties(
                        *state, true, LOWORD(wparam));
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"プレーンプロパティ");
                    }
                    RefreshTreePane(*state);
                    return 0;
                }
                case IDM_PLANE_PROPERTIES: {
                    const InkpodStatus status = EditSelectedTreeNodeProperties(*state, true);
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"プレーンプロパティ");
                    }
                    RefreshTreePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_PLANE_CONVERT: {
                    TreePaneNode node{};
                    if (!QueryTreeNode(*state, true, node)) {
                        return 0;
                    }
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"プレーン変換（変換損失を確認）";
                    dialog.labels = {L"変換先種類 (1-7)", L"変換先形式 (1-5)", L"損失を確認 (1)", nullptr};
                    dialog.values = {
                        static_cast<std::int32_t>(node.kind),
                        static_cast<std::int32_t>(node.pixel_format),
                        1,
                        0};
                    dialog.value_count = 3U;
                    if (state->smoke_test) {
                        dialog.values[0] = INKPOD_TYPED_PLANE_RASTER;
                        dialog.values[1] = INKPOD_STORAGE_RGBA8;
                    }
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[2] == 0) {
                        return 0;
                    }
                    InkpodTreeEdit edit{};
                    edit.struct_size = sizeof(edit);
                    edit.operation = INKPOD_TREE_CONVERT_PLANE;
                    edit.object_id = node.id;
                    edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
                    edit.pixel_format = static_cast<std::uint32_t>(dialog.values[1]);
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyTreeEditRecord(*state, edit, {}, ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"プレーン変換");
                    }
                    RefreshTreePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_PLANE_MERGE: {
                    InkpodTreeEdit edit{};
                    edit.struct_size = sizeof(edit);
                    edit.operation = INKPOD_TREE_MERGE_PLANE;
                    edit.object_id = state->active_tree_plane_id;
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyTreeEditRecord(*state, edit, {}, ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"同種プレーンの統合");
                    }
                    state->active_tree_plane_id = 0U;
                    RefreshTreePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_SET_NEW:
                case IDM_LT_SET_RENAME: {
                    TextInputDialogState dialog{};
                    dialog.title = LOWORD(wparam) == IDM_LT_SET_NEW
                        ? L"ライトテーブルセットを作成"
                        : L"ライトテーブルセット名";
                    dialog.label = L"名前";
                    dialog.value = state->smoke_test ? L"Smoke Set" : L"Light Table";
                    if (ShowTextInput(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    InkpodLightTableEdit edit{};
                    edit.operation = LOWORD(wparam) == IDM_LT_SET_NEW
                        ? INKPOD_LIGHT_TABLE_CREATE_SET
                        : INKPOD_LIGHT_TABLE_RENAME_SET;
                    edit.object_id = state->active_light_table_set_id;
                    std::uint64_t object_id{};
                    const InkpodStatus status = ApplyLightTableEdit(
                        *state, edit, dialog.value, object_id);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ライトテーブルセット");
                    } else if (object_id != 0U) {
                        state->active_light_table_set_id = object_id;
                    }
                    RefreshLightTablePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_SET_DUPLICATE:
                case IDM_LT_SET_DELETE:
                case IDM_LT_SET_UP:
                case IDM_LT_SET_DOWN: {
                    InkpodLightTableEdit edit{};
                    const UINT command = LOWORD(wparam);
                    edit.operation = command == IDM_LT_SET_DUPLICATE
                        ? INKPOD_LIGHT_TABLE_DUPLICATE_SET
                        : (command == IDM_LT_SET_DELETE
                               ? INKPOD_LIGHT_TABLE_DELETE_SET
                               : INKPOD_LIGHT_TABLE_REORDER_SET);
                    edit.object_id = state->active_light_table_set_id;
                    edit.destination_index = state->active_light_table_set_index;
                    if (command == IDM_LT_SET_UP && edit.destination_index != 0U) {
                        --edit.destination_index;
                    } else if (command == IDM_LT_SET_DOWN) {
                        ++edit.destination_index;
                    }
                    std::uint64_t object_id{};
                    const InkpodStatus status = ApplyLightTableEdit(*state, edit, {}, object_id);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ライトテーブルセット操作");
                    } else if (object_id != 0U) {
                        state->active_light_table_set_id = object_id;
                    } else {
                        state->active_light_table_set_id = 0U;
                    }
                    RefreshLightTablePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_GLOBAL_OPACITY: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"ライトテーブル全体不透明度";
                    dialog.labels[0] = L"不透明度 (0-100%)";
                    dialog.values[0] = state->smoke_test ? 50 : 100;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] < 0 || dialog.values[0] > 100) {
                        return 0;
                    }
                    const std::uint32_t opacity =
                        static_cast<std::uint32_t>(dialog.values[0]) * 10U;
                    const InkpodStatus status = state->engine->Invoke(
                        [opacity](InkpodCore* core) {
                            InkpodDispatchResult result{};
                            result.struct_size = sizeof(result);
                            return inkpod_core_light_table_set_global_opacity(
                                core, opacity, &result);
                        },
                        true,
                        true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ライトテーブル全体不透明度");
                    }
                    RefreshLightTablePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_ITEM_ADD:
                case IDM_LT_ITEM_RELOAD: {
                    std::wstring path = state->smoke_test ? state->smoke_raster_path : L"";
                    if (!state->smoke_test && !ChooseCommonRasterPath(window, false, path)) {
                        return 0;
                    }
                    const InkpodStatus status = AddOrReloadLightTableRaster(
                        *state, path, LOWORD(wparam) == IDM_LT_ITEM_RELOAD);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ライトテーブル画像");
                    }
                    RefreshLightTablePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_ITEM_DELETE:
                case IDM_LT_ITEM_UP:
                case IDM_LT_ITEM_DOWN: {
                    InkpodLightTableEdit edit{};
                    edit.operation = LOWORD(wparam) == IDM_LT_ITEM_DELETE
                        ? INKPOD_LIGHT_TABLE_REMOVE_ITEM
                        : INKPOD_LIGHT_TABLE_REORDER_ITEM;
                    edit.object_id = state->active_light_table_item_id;
                    edit.destination_index = state->active_light_table_item_index;
                    if (LOWORD(wparam) == IDM_LT_ITEM_UP && edit.destination_index != 0U) {
                        --edit.destination_index;
                    } else if (LOWORD(wparam) == IDM_LT_ITEM_DOWN) {
                        ++edit.destination_index;
                    }
                    std::uint64_t ignored{};
                    const InkpodStatus status = ApplyLightTableEdit(*state, edit, {}, ignored);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ライトテーブル項目操作");
                    }
                    state->active_light_table_item_id = 0U;
                    RefreshLightTablePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_ITEM_PROPERTIES: {
                    const InkpodStatus status = EditLightTableItemProperties(*state);
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"ライトテーブル項目プロパティ");
                    }
                    RefreshLightTablePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_ITEM_MOVE:
                    if (state->active_light_table_item_id == 0U) {
                        return 0;
                    }
                    state->tool = kInteractionLightTableMove;
                    state->light_table_move_samples.clear();
                    UpdateMenuState(*state);
                    return 1;
                case IDM_LT_ITEM_SAMPLE: {
                    InkpodDocumentInfo document{};
                    QueryDocument(*state, document);
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"ライトテーブル色サンプル";
                    dialog.labels = {L"X", L"Y", nullptr, nullptr};
                    dialog.values = {
                        state->smoke_test ? document.reference_frame.x : 0,
                        state->smoke_test ? document.reference_frame.y : 0,
                        0,
                        0};
                    dialog.value_count = 2U;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] < 0 || dialog.values[1] < 0) {
                        return 0;
                    }
                    InkpodColorValue color{};
                    color.struct_size = sizeof(color);
                    const InkpodStatus status = state->engine->Invoke(
                        [&color, &dialog](InkpodCore* core) {
                            return inkpod_core_light_table_sample(
                                core,
                                static_cast<std::uint32_t>(dialog.values[0]),
                                static_cast<std::uint32_t>(dialog.values[1]),
                                &color);
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        SetDrawingColor(*state, color);
                    } else if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ライトテーブル色サンプル");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_LT_ITEM_SWAP: {
                    InkpodDocumentInfo info{};
                    const std::uint64_t item_id = state->active_light_table_item_id;
                    const InkpodStatus status = state->engine->Invoke(
                        [item_id, &info](InkpodCore* core) {
                            info = EmptyDocumentInfo();
                            return inkpod_core_light_table_swap(core, item_id, &info);
                        },
                        false,
                        false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ライトテーブルと編集画像の入れ替え");
                    } else {
                        ResetM3DocumentUiState(*state);
                        FitCanvas(*state, INKPOD_VIEW_FIT);
                    }
                    RefreshLightTablePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_SEQ_IMPORT: {
                    std::vector<std::wstring> paths;
                    if (state->smoke_test) {
                        try {
                            paths = state->smoke_sequence_paths;
                        } catch (const std::bad_alloc&) {
                            return 0;
                        }
                    } else if (!ChooseCommonRasterPaths(window, paths)) {
                        return 0;
                    }
                    const InkpodStatus status = ImportSequencePaths(*state, paths);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"連番読み込み");
                    }
                    RefreshSequencePane(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_SEQ_EXPORT: {
                    std::wstring path = state->smoke_test ? state->smoke_raster_path : L"";
                    if (!state->smoke_test && !ChooseCommonRasterPath(window, true, path)) {
                        return 0;
                    }
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"連番書き出し";
                    dialog.labels[0] = L"白背景を合成 (0/1)";
                    dialog.values[0] = state->smoke_test ? 1 : 0;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    const InkpodStatus status = ExportSequenceToPath(
                        *state, path, dialog.values[0] != 0);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"連番書き出し");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_SEQ_PREVIOUS:
                case IDM_SEQ_NEXT: {
                    const bool next = LOWORD(wparam) == IDM_SEQ_NEXT;
                    InkpodDocumentInfo info{};
                    const InkpodStatus status = state->engine->Invoke(
                        [next, &info](InkpodCore* core) {
                            info = EmptyDocumentInfo();
                            return inkpod_core_sequence_step(
                                core,
                                next ? INKPOD_SEQUENCE_NEXT : INKPOD_SEQUENCE_PREVIOUS,
                                INKPOD_SEQUENCE_FLAG_LOOP,
                                &info);
                        },
                        false,
                        false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"前後セル切替");
                    } else {
                        ResetM3DocumentUiState(*state);
                        FitCanvas(*state, INKPOD_VIEW_FIT);
                        RefreshSequencePane(*state);
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_SEQ_GOTO: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"セル番号で移動";
                    dialog.labels[0] = L"セル番号";
                    dialog.values[0] = state->smoke_test ? 3 : 1;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] < 0) {
                        return 0;
                    }
                    const std::uint32_t number = static_cast<std::uint32_t>(dialog.values[0]);
                    InkpodDocumentInfo info{};
                    std::uint32_t selected{};
                    const InkpodStatus status = state->engine->Invoke(
                        [number, &info, &selected](InkpodCore* core) {
                            for (std::uint32_t index = 0U; index < 10000U; ++index) {
                                InkpodSequenceCellInfo cell{};
                                cell.struct_size = sizeof(cell);
                                const InkpodStatus query = inkpod_core_sequence_cell_get(
                                    core, index, &cell);
                                if (query != INKPOD_STATUS_OK) {
                                    return INKPOD_STATUS_INVALID_ARGUMENT;
                                }
                                if (cell.cell_number == number) {
                                    selected = index;
                                    info = EmptyDocumentInfo();
                                    return inkpod_core_sequence_activate(core, index, &info);
                                }
                            }
                            return INKPOD_STATUS_INVALID_ARGUMENT;
                        },
                        false,
                        false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"セル番号移動");
                    } else {
                        ResetM3DocumentUiState(*state);
                        state->active_sequence_index = selected;
                        FitCanvas(*state, INKPOD_VIEW_FIT);
                        RefreshSequencePane(*state);
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_SUBPALETTE_SET: {
                    const std::uint32_t index = state->active_sequence_index;
                    const InkpodStatus status = state->engine->Invoke(
                        [index](InkpodCore* core) {
                            return inkpod_core_subpalette_set(core, index);
                        },
                        false,
                        false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"サブパレット登録");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_SUBPALETTE_SAMPLE: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"サブパレット色サンプル";
                    dialog.labels = {L"X", L"Y", nullptr, nullptr};
                    dialog.values = {0, 0, 0, 0};
                    dialog.value_count = 2U;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] < 0 || dialog.values[1] < 0) {
                        return 0;
                    }
                    InkpodColorValue color{};
                    color.struct_size = sizeof(color);
                    const InkpodStatus status = state->engine->Invoke(
                        [&dialog, &color](InkpodCore* core) {
                            return inkpod_core_subpalette_sample(
                                core,
                                static_cast<std::uint32_t>(dialog.values[0]),
                                static_cast<std::uint32_t>(dialog.values[1]),
                                &color);
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        SetDrawingColor(*state, color);
                    } else if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"サブパレット色サンプル");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_MOTION_START: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"モーションチェック設定";
                    dialog.labels = {L"FPS (8/10/12/24/25/30)", L"ループ (0/1)", L"選択表示 (0/1)", L"LT表示 (0/1)"};
                    dialog.values = {
                        static_cast<std::int32_t>(state->motion_fps),
                        (state->motion_flags & INKPOD_MOTION_FLAG_LOOP) != 0U ? 1 : 0,
                        (state->motion_flags & INKPOD_MOTION_FLAG_INCLUDE_SELECTION) != 0U ? 1 : 0,
                        (state->motion_flags & INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE) != 0U ? 1 : 0};
                    dialog.value_count = 4U;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    state->motion_fps = static_cast<std::uint32_t>(dialog.values[0]);
                    state->motion_flags = (dialog.values[1] != 0 ? INKPOD_MOTION_FLAG_LOOP : 0U)
                        | (dialog.values[2] != 0
                               ? INKPOD_MOTION_FLAG_INCLUDE_SELECTION
                               : 0U)
                        | (dialog.values[3] != 0
                               ? INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE
                               : 0U);
                    InkpodMotionCheckInput input{
                        sizeof(InkpodMotionCheckInput),
                        state->motion_fps,
                        state->motion_flags};
                    InkpodMotionFrame frame{};
                    frame.struct_size = sizeof(frame);
                    const InkpodStatus status = state->engine->Invoke(
                        [input, &frame](InkpodCore* core) {
                            return inkpod_core_motion_check_start(core, &input, &frame);
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        state->motion_active = true;
                        UpdateMotionLabel(*state, frame);
                        SetTimer(
                            window,
                            kMotionPlaybackTimer,
                            std::max<UINT>(1U, 1000U / state->motion_fps),
                            nullptr);
                    } else {
                        ShowCoreError(*state, window, L"モーションチェック開始");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_MOTION_PAUSE: {
                    InkpodMotionFrame frame{};
                    frame.struct_size = sizeof(frame);
                    const InkpodStatus status = state->engine->Invoke(
                        [&frame](InkpodCore* core) {
                            return inkpod_core_motion_check_toggle_pause(core, &frame);
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        UpdateMotionLabel(*state, frame);
                        if (state->motion_paused) {
                            KillTimer(window, kMotionPlaybackTimer);
                        } else {
                            SetTimer(
                                window,
                                kMotionPlaybackTimer,
                                std::max<UINT>(1U, 1000U / state->motion_fps),
                                nullptr);
                        }
                    } else {
                        ShowCoreError(*state, window, L"モーション一時停止");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_MOTION_PREVIOUS:
                case IDM_MOTION_NEXT: {
                    InkpodMotionFrame frame{};
                    frame.struct_size = sizeof(frame);
                    const bool next = LOWORD(wparam) == IDM_MOTION_NEXT;
                    const InkpodStatus status = state->engine->Invoke(
                        [next, &frame](InkpodCore* core) {
                            return inkpod_core_motion_check_step(
                                core,
                                next ? INKPOD_SEQUENCE_NEXT : INKPOD_SEQUENCE_PREVIOUS,
                                &frame);
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        UpdateMotionLabel(*state, frame);
                    } else {
                        ShowCoreError(*state, window, L"モーションフレーム移動");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_MOTION_FIRST:
                case IDM_MOTION_LAST: {
                    const bool last = LOWORD(wparam) == IDM_MOTION_LAST;
                    InkpodMotionFrame frame{};
                    frame.struct_size = sizeof(frame);
                    const InkpodStatus status = state->engine->Invoke(
                        [last, &frame](InkpodCore* core) {
                            std::uint32_t count{};
                            for (; count < 10000U; ++count) {
                                InkpodSequenceCellInfo cell{};
                                cell.struct_size = sizeof(cell);
                                if (inkpod_core_sequence_cell_get(core, count, &cell)
                                    != INKPOD_STATUS_OK) {
                                    break;
                                }
                            }
                            if (count == 0U) {
                                return INKPOD_STATUS_INVALID_STATE;
                            }
                            const std::uint64_t target = last ? count - 1U : 0U;
                            for (std::uint32_t step = 0U; step < count; ++step) {
                                const InkpodStatus current = inkpod_core_motion_check_step(
                                    core,
                                    last ? INKPOD_SEQUENCE_NEXT : INKPOD_SEQUENCE_PREVIOUS,
                                    &frame);
                                if (current != INKPOD_STATUS_OK
                                    || frame.sequence_index == target) {
                                    return current;
                                }
                            }
                            return INKPOD_STATUS_INVALID_STATE;
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        UpdateMotionLabel(*state, frame);
                    } else {
                        ShowCoreError(*state, window, L"モーション先頭・末尾移動");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_MOTION_FPS_30:
                case IDM_MOTION_FPS_25:
                case IDM_MOTION_FPS_24:
                case IDM_MOTION_FPS_12:
                case IDM_MOTION_FPS_10:
                case IDM_MOTION_FPS_8: {
                    const UINT command = LOWORD(wparam);
                    state->motion_fps = command == IDM_MOTION_FPS_30
                        ? 30U
                        : (command == IDM_MOTION_FPS_25
                                  ? 25U
                                  : (command == IDM_MOTION_FPS_24
                                            ? 24U
                                            : (command == IDM_MOTION_FPS_12
                                                      ? 12U
                                                      : (command == IDM_MOTION_FPS_10 ? 10U : 8U))));
                    if (!state->motion_active) {
                        UpdateMenuState(*state);
                        return 1;
                    }
                    const InkpodMotionCheckInput input{
                        sizeof(InkpodMotionCheckInput),
                        state->motion_fps,
                        state->motion_flags};
                    InkpodMotionFrame frame{};
                    frame.struct_size = sizeof(frame);
                    const InkpodStatus status = state->engine->Invoke(
                        [input, &frame](InkpodCore* core) {
                            InkpodStatus current = inkpod_core_motion_check_stop(core);
                            if (current == INKPOD_STATUS_OK) {
                                current = inkpod_core_motion_check_start(core, &input, &frame);
                            }
                            return current;
                        },
                        false,
                        false);
                    if (status == INKPOD_STATUS_OK) {
                        UpdateMotionLabel(*state, frame);
                        SetTimer(
                            window,
                            kMotionPlaybackTimer,
                            std::max<UINT>(1U, 1000U / state->motion_fps),
                            nullptr);
                    } else {
                        ShowCoreError(*state, window, L"モーションFPS変更");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_MOTION_STOP: {
                    const InkpodStatus status = state->engine->Invoke(
                        [](InkpodCore* core) {
                            return inkpod_core_motion_check_stop(core);
                        },
                        false,
                        false);
                    KillTimer(window, kMotionPlaybackTimer);
                    state->motion_active = false;
                    state->motion_paused = false;
                    SetWindowTextW(state->motion_label, L"モーション停止");
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"モーション停止");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_SELECTION_RECTANGLE:
                case IDM_SELECTION_ELLIPSE:
                case IDM_SELECTION_LASSO:
                case IDM_SELECTION_POLYLINE:
                case IDM_SELECTION_TRACE:
                case IDM_SELECTION_WAND: {
                    const UINT command = LOWORD(wparam);
                    state->selection_shape = command == IDM_SELECTION_ELLIPSE
                        ? INKPOD_SELECTION_ELLIPSE
                        : (command == IDM_SELECTION_LASSO
                                  ? INKPOD_SELECTION_LASSO
                                  : (command == IDM_SELECTION_POLYLINE
                                            ? INKPOD_SELECTION_POLYLINE
                                            : (command == IDM_SELECTION_TRACE
                                                      ? INKPOD_SELECTION_TRACE
                                                      : (command == IDM_SELECTION_WAND
                                                                ? INKPOD_SELECTION_WAND
                                                                : INKPOD_SELECTION_RECTANGLE))));
                    state->tool = kInteractionSelection;
                    state->selection_gesture_samples.clear();
                    if (command == IDM_SELECTION_WAND || command == IDM_SELECTION_TRACE) {
                        ViewOptionsDialogState dialog{};
                        dialog.title = command == IDM_SELECTION_WAND
                            ? L"色の杖"
                            : L"選択トレース";
                        dialog.labels = command == IDM_SELECTION_WAND
                            ? std::array<const wchar_t*, 4U>{
                                  L"許容差", L"隙間", nullptr, nullptr}
                            : std::array<const wchar_t*, 4U>{
                                  L"直径 (px)", nullptr, nullptr, nullptr};
                        dialog.values[0] = command == IDM_SELECTION_WAND
                            ? state->selection_tolerance
                            : static_cast<std::int32_t>(state->selection_diameter);
                        dialog.values[1] = state->selection_gap_close;
                        dialog.value_count = command == IDM_SELECTION_WAND ? 2U : 1U;
                        if (ShowViewOptions(
                                state->instance,
                                window,
                                state->smoke_test,
                                dialog) != IDOK) {
                            return 0;
                        }
                        if (command == IDM_SELECTION_WAND) {
                            if (dialog.values[0] < 0 || dialog.values[0] > UINT16_MAX
                                || dialog.values[1] < 0
                                || dialog.values[1] > UINT16_MAX) {
                                return 0;
                            }
                            state->selection_tolerance =
                                static_cast<std::uint16_t>(dialog.values[0]);
                            state->selection_gap_close =
                                static_cast<std::uint16_t>(dialog.values[1]);
                        } else if (dialog.values[0] > 0) {
                            state->selection_diameter =
                                static_cast<float>(dialog.values[0]);
                        }
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_MODE_NEW:
                case IDM_SELECTION_MODE_ADD:
                case IDM_SELECTION_MODE_SUBTRACT:
                case IDM_SELECTION_MODE_INTERSECT:
                    state->selection_operation = LOWORD(wparam) == IDM_SELECTION_MODE_ADD
                        ? INKPOD_SELECTION_ADD
                        : (LOWORD(wparam) == IDM_SELECTION_MODE_SUBTRACT
                                  ? INKPOD_SELECTION_SUBTRACT
                                  : (LOWORD(wparam) == IDM_SELECTION_MODE_INTERSECT
                                            ? INKPOD_SELECTION_INTERSECT
                                            : INKPOD_SELECTION_NEW));
                    UpdateMenuState(*state);
                    return 0;
                case IDM_SELECTION_CLEAR: {
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_selection_clear(core, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"選択解除");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_COLOR:
                case IDM_SELECTION_COLOR_DIFFERENT:
                case IDM_SELECTION_COLOR_ADD: {
                    const UINT command = LOWORD(wparam);
                    const InkpodStatus status = SelectDrawingColor(
                        *state,
                        command == IDM_SELECTION_COLOR_DIFFERENT,
                        command == IDM_SELECTION_COLOR_ADD ? INKPOD_SELECTION_ADD
                                                           : INKPOD_SELECTION_NEW);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"描画色の選択");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_TO_LAYER: {
                    static constexpr std::array<std::uint8_t, 13U> name{
                        0xe9, 0x81, 0xb8, 0xe6, 0x8a, 0x9e, 0xe7, 0xaf, 0x84,
                        0xe5, 0x9b, 0xb2, '1'};
                    std::uint64_t layer_id{};
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [&layer_id](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_selection_to_layer(
                                      core,
                                      name.data(),
                                      name.size(),
                                      &result,
                                      &layer_id);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"選択範囲をレイヤーへ変換");
                    } else {
                        state->selection_layer_id = layer_id;
                        state->m3_layer_id = layer_id;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_FROM_LAYER:
                case IDM_SELECTION_LAYER_ADD:
                case IDM_SELECTION_LAYER_SUBTRACT: {
                    const UINT command = LOWORD(wparam);
                    const std::uint32_t operation = command == IDM_SELECTION_LAYER_ADD
                        ? INKPOD_SELECTION_LAYER_ADD
                        : (command == IDM_SELECTION_LAYER_SUBTRACT
                                  ? INKPOD_SELECTION_LAYER_SUBTRACT
                                  : INKPOD_SELECTION_LAYER_REPLACE);
                    const std::uint64_t layer_id = state->selection_layer_id;
                    const InkpodStatus status = state->engine == nullptr || layer_id == 0U
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [layer_id, operation](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_selection_from_layer(
                                      core, layer_id, operation, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"選択レイヤー変換");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_ALL: {
                    InkpodDocumentInfo info{};
                    InkpodSelectionInput input{};
                    input.struct_size = sizeof(input);
                    input.shape = INKPOD_SELECTION_RECTANGLE;
                    input.operation = INKPOD_SELECTION_NEW;
                    const bool queried = QueryDocument(*state, info);
                    if (queried) {
                        input.bounds = {
                            0,
                            0,
                            static_cast<std::int32_t>(info.width),
                            static_cast<std::int32_t>(info.height)};
                    }
                    const InkpodStatus status = !queried || state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [input](InkpodCore* core) {
                                  InkpodDispatchResult result{};
                                  result.struct_size = sizeof(result);
                                  return inkpod_core_apply_selection(
                                      core, &input, &result);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"すべて選択");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SELECTION_INVERT:
                case IDM_SELECTION_EXPAND:
                case IDM_SELECTION_SHRINK: {
                    const std::uint32_t operation =
                        LOWORD(wparam) == IDM_SELECTION_INVERT
                        ? INKPOD_SELECTION_ADJUST_INVERT
                        : (LOWORD(wparam) == IDM_SELECTION_EXPAND
                                  ? INKPOD_SELECTION_ADJUST_EXPAND
                                  : INKPOD_SELECTION_ADJUST_SHRINK);
                    std::uint32_t pixels = 0U;
                    if (operation != INKPOD_SELECTION_ADJUST_INVERT) {
                        ViewOptionsDialogState dialog{};
                        dialog.title = operation == INKPOD_SELECTION_ADJUST_EXPAND
                            ? L"選択範囲を拡張"
                            : L"選択範囲を縮小";
                        dialog.labels[0] = L"幅 (px)";
                        dialog.values[0] = state->smoke_test ? 2 : 1;
                        if (ShowViewOptions(
                                state->instance,
                                window,
                                state->smoke_test,
                                dialog) != IDOK
                            || dialog.values[0] <= 0) {
                            return 0;
                        }
                        pixels = static_cast<std::uint32_t>(dialog.values[0]);
                    }
                    const InkpodStatus status = AdjustSelection(
                        *state,
                        operation,
                        pixels);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"選択範囲の変更");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_ZOOM_IN:
                case IDM_VIEW_ZOOM_OUT: {
                    RECT client{};
                    GetClientRect(state->canvas, &client);
                    const double factor = LOWORD(wparam) == IDM_VIEW_ZOOM_IN ? 1.2 : 1.0 / 1.2;
                    if (ApplyView(
                            *state,
                            INKPOD_VIEW_ZOOM_AT,
                            factor,
                            static_cast<double>(client.right) / 2.0,
                            static_cast<double>(client.bottom) / 2.0)
                        != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"表示倍率の変更");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_FIT:
                case IDM_VIEW_ONE_TO_ONE:
                    if (FitCanvas(
                            *state,
                            LOWORD(wparam) == IDM_VIEW_FIT ? INKPOD_VIEW_FIT
                                                          : INKPOD_VIEW_ONE_TO_ONE)
                        != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"表示の変更");
                    }
                    UpdateMenuState(*state);
                    return 0;
                case IDM_VIEW_ZOOM_PERCENT: {
                    InkpodSnapshotTransform transform{};
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"表示倍率";
                    dialog.labels[0] = L"倍率 (%)";
                    dialog.values[0] = state->smoke_test
                        ? 125
                        : (QuerySnapshotTransform(*state, transform)
                        ? static_cast<std::int32_t>(std::clamp(
                              std::lround(transform.zoom * 100.0), 1L, 6400L))
                        : 100);
                    if (ShowViewOptions(
                            state->instance,
                            window,
                            state->smoke_test,
                            dialog) == IDOK) {
                        const InkpodStatus status = ApplyZoomPercent(
                            *state, static_cast<std::uint32_t>(dialog.values[0]));
                        if (status != INKPOD_STATUS_OK) {
                            ShowCoreError(*state, window, L"数値倍率の変更");
                        }
                        UpdateMenuState(*state);
                    }
                    return 0;
                }
                case IDM_VIEW_BOX_ZOOM:
                    state->tool = kInteractionBoxZoom;
                    state->view_gesture_samples.clear();
                    UpdateMenuState(*state);
                    return 0;
                case IDM_VIEW_FLIP_HORIZONTAL:
                case IDM_VIEW_FLIP_VERTICAL: {
                    const bool horizontal =
                        LOWORD(wparam) == IDM_VIEW_FLIP_HORIZONTAL;
                    const InkpodStatus status = ApplyView(
                        *state,
                        horizontal ? INKPOD_VIEW_FLIP_HORIZONTAL
                                   : INKPOD_VIEW_FLIP_VERTICAL,
                        0.0,
                        0.0);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"表示反転");
                    } else if (horizontal) {
                        state->view_flip_horizontal =
                            !state->view_flip_horizontal;
                    } else {
                        state->view_flip_vertical = !state->view_flip_vertical;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_RULER:
                case IDM_VIEW_GUIDES:
                case IDM_VIEW_GRID:
                case IDM_VIEW_SNAP_GUIDES:
                case IDM_VIEW_SNAP_GRID:
                case IDM_VIEW_TRANSPARENT: {
                    const UINT command = LOWORD(wparam);
                    bool* current = command == IDM_VIEW_RULER
                        ? &state->ruler_visible
                        : (command == IDM_VIEW_GUIDES
                                  ? &state->guides_visible
                                  : (command == IDM_VIEW_GRID
                                            ? &state->grid_visible
                                            : (command == IDM_VIEW_SNAP_GUIDES
                                                      ? &state->snap_guides
                                                      : (command == IDM_VIEW_SNAP_GRID
                                                                ? &state->snap_grid
                                                                : &state->transparent_visible))));
                    const InkpodViewCommandKind kind = command == IDM_VIEW_RULER
                        ? INKPOD_VIEW_SET_RULER_VISIBLE
                        : (command == IDM_VIEW_GUIDES
                                  ? INKPOD_VIEW_SET_GUIDES_VISIBLE
                                  : (command == IDM_VIEW_GRID
                                            ? INKPOD_VIEW_SET_GRID_VISIBLE
                                            : (command == IDM_VIEW_SNAP_GUIDES
                                                      ? INKPOD_VIEW_SET_GUIDE_SNAP_ENABLED
                                                      : (command == IDM_VIEW_SNAP_GRID
                                                                ? INKPOD_VIEW_SET_GRID_SNAP_ENABLED
                                                                : INKPOD_VIEW_SET_TRANSPARENT_VISIBLE))));
                    const bool visible = !*current;
                    const InkpodStatus status = ApplyView(
                        *state,
                        kind,
                        visible ? 1.0 : 0.0,
                        0.0);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"表示補助の切替");
                    } else {
                        *current = visible;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_GUIDE_VERTICAL:
                case IDM_VIEW_GUIDE_HORIZONTAL: {
                    InkpodDocumentInfo info{};
                    if (!QueryDocument(*state, info)) {
                        return 0;
                    }
                    const bool vertical = LOWORD(wparam) == IDM_VIEW_GUIDE_VERTICAL;
                    ViewOptionsDialogState dialog{};
                    dialog.title = vertical ? L"垂直ガイド" : L"水平ガイド";
                    dialog.labels[0] = vertical ? L"X (px)" : L"Y (px)";
                    dialog.values[0] = static_cast<std::int32_t>(
                        vertical ? info.width / 2U : info.height / 2U);
                    if (ShowViewOptions(
                            state->instance,
                            window,
                            state->smoke_test,
                            dialog) != IDOK) {
                        return 0;
                    }
                    const InkpodStatus status = AddGuide(
                        *state,
                        vertical ? INKPOD_GUIDE_VERTICAL : INKPOD_GUIDE_HORIZONTAL,
                        dialog.values[0]);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ガイドの追加");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_GUIDE_MOVE:
                    state->tool = kInteractionGuideMove;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_VIEW_GUIDE_DELETE_ALL: {
                    const InkpodStatus status = DeleteAllGuides(*state);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ガイドの削除");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_GRID_SETTINGS: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"グリッド設定";
                    dialog.labels = {L"X 原点", L"Y 原点", L"間隔 (px)", L"分割数"};
                    dialog.values = {0, 0, 8, 2};
                    dialog.value_count = 4U;
                    if (ShowViewOptions(
                            state->instance,
                            window,
                            state->smoke_test,
                            dialog) != IDOK) {
                        return 0;
                    }
                    InkpodGridInput input{};
                    input.struct_size = sizeof(input);
                    input.origin_x = dialog.values[0];
                    input.origin_y = dialog.values[1];
                    input.spacing_x = static_cast<std::uint32_t>(dialog.values[2]);
                    input.spacing_y = static_cast<std::uint32_t>(dialog.values[2]);
                    input.subdivisions = static_cast<std::uint32_t>(dialog.values[3]);
                    const InkpodStatus status = dialog.values[2] <= 0
                            || dialog.values[3] <= 0
                        ? INKPOD_STATUS_INVALID_ARGUMENT
                        : SetGrid(*state, input);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"グリッド設定");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_VIEW_NEW: {
                    std::uint64_t view_id{};
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [&view_id](InkpodCore* core) {
                                  return inkpod_core_view_create(
                                      core, &view_id);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ビューの作成");
                    } else {
                        state->secondary_view_id = view_id;
                        state->active_view_id = view_id;
                        if (state->document_tabs != nullptr) {
                            TCITEMW item{};
                            item.mask = TCIF_TEXT | TCIF_PARAM;
                            item.pszText = const_cast<wchar_t*>(L"セル [ビュー 2]");
                            item.lParam = static_cast<LPARAM>(view_id);
                            const int index = TabCtrl_GetItemCount(state->document_tabs);
                            if (TabCtrl_InsertItem(state->document_tabs, index, &item) >= 0) {
                                TabCtrl_SetCurSel(state->document_tabs, index);
                            }
                        }
                        state->engine->SetActiveView(view_id);
                        state->view_flip_horizontal = false;
                        state->view_flip_vertical = false;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_TOOL_PENCIL:
                    state->tool = INKPOD_TOOL_PENCIL;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_BRUSH:
                    state->tool = INKPOD_TOOL_BRUSH;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_ERASER:
                    state->tool = INKPOD_TOOL_ERASER;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_FILL:
                case IDM_TOOL_CLOSED_FILL:
                case IDM_TOOL_FILL_EXTENSION:
                    state->tool = kInteractionFill;
                    state->fill_options.operation = LOWORD(wparam) == IDM_TOOL_CLOSED_FILL
                        ? INKPOD_FILL_CLOSED_REGION
                        : (LOWORD(wparam) == IDM_TOOL_FILL_EXTENSION
                                  ? INKPOD_FILL_EXTENSION
                                  : INKPOD_FILL_SEED);
                    state->fill_gesture_samples.clear();
                    UpdateMenuState(*state);
                    return 0;
                case IDM_TOOL_FILL_OPTIONS:
                    if (ShowFillOptions(*state)) {
                        state->tool = kInteractionFill;
                        state->fill_gesture_samples.clear();
                        UpdateMenuState(*state);
                    }
                    return 0;
                case IDM_TOOL_EYEDROPPER:
                    state->tool = kInteractionEyedropper;
                    UpdateMenuState(*state);
                    return 0;
                case IDM_VECTOR_LINE:
                case IDM_VECTOR_CURVE:
                case IDM_VECTOR_RECTANGLE:
                case IDM_VECTOR_ELLIPSE:
                case IDM_VECTOR_POLYLINE:
                case IDM_VECTOR_ERASER: {
                    ClearVectorGeometryPreview(*state);
                    TreePaneNode plane{};
                    if (!QueryTreeNode(*state, true, plane)) {
                        return 0;
                    }
                    if (!IsVectorStrokePlane(plane.kind)) {
                        if (state->engine != nullptr) {
                            state->engine->SetLocalFailure(kVectorStrokePlaneRequired);
                        }
                        UpdateMenuState(*state);
                        return 0;
                    }
                    const UINT command = LOWORD(wparam);
                    state->tool = command == IDM_VECTOR_LINE
                        ? kInteractionVectorLine
                        : (command == IDM_VECTOR_CURVE
                                  ? kInteractionVectorCurve
                                  : (command == IDM_VECTOR_RECTANGLE
                                            ? kInteractionVectorRectangle
                                            : (command == IDM_VECTOR_ELLIPSE
                                                      ? kInteractionVectorEllipse
                                                      : (command == IDM_VECTOR_POLYLINE
                                                                ? kInteractionVectorPolyline
                                                                : kInteractionVectorEraser))));
                    UpdateMenuState(*state);
                    return 1;
                }
                case IDM_VECTOR_ERASE_PARTIAL:
                case IDM_VECTOR_ERASE_INTERSECTION:
                case IDM_VECTOR_ERASE_WHOLE:
                    state->vector_erase_mode = LOWORD(wparam) == IDM_VECTOR_ERASE_INTERSECTION
                        ? INKPOD_VECTOR_ERASE_TO_INTERSECTION
                        : (LOWORD(wparam) == IDM_VECTOR_ERASE_WHOLE
                                  ? INKPOD_VECTOR_ERASE_WHOLE_PATH
                                  : INKPOD_VECTOR_ERASE_PARTIAL);
                    state->tool = kInteractionVectorEraser;
                    UpdateMenuState(*state);
                    return 1;
                case IDM_VECTOR_CONNECT: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"ベクター線つなぎ";
                    dialog.labels[0] = L"最大隙間 (1/1000 px)";
                    dialog.values[0] = state->smoke_test ? 4000 : 2000;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] <= 0) {
                        return 0;
                    }
                    const InkpodStatus status = ConnectSelectedVectorPlane(
                        *state, static_cast<float>(dialog.values[0]) / 1000.0F);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ベクター線つなぎ");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_VECTOR_WIDTH: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"ベクター線幅修正";
                    dialog.labels = {L"モード (1:太く 2:細く 3:倍率 4:一定)",
                        L"値 (1/1000 px または倍率)", nullptr, nullptr};
                    dialog.values = {1, state->smoke_test ? 500 : 1000, 0, 0};
                    dialog.value_count = 2U;
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] < 1 || dialog.values[0] > 4
                        || dialog.values[1] <= 0) {
                        return 0;
                    }
                    const InkpodStatus status = CorrectSelectedVectorWidth(
                        *state,
                        static_cast<InkpodVectorWidthMode>(dialog.values[0]),
                        static_cast<float>(dialog.values[1]) / 1000.0F);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ベクター線幅修正");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_VECTOR_SELECT_CUT:
                case IDM_VECTOR_SELECT_TOUCH:
                case IDM_VECTOR_SELECT_CONTAINED:
                case IDM_VECTOR_SELECT_LINE:
                case IDM_VECTOR_SELECT_WHOLE_LINE:
                case IDM_VECTOR_SELECT_INTERSECTION:
                case IDM_VECTOR_SELECT_FILL_BOUNDARY:
                case IDM_VECTOR_SELECT_FILL: {
                    const UINT command = LOWORD(wparam);
                    state->vector_selection_mode = command == IDM_VECTOR_SELECT_CUT
                        ? INKPOD_VECTOR_SELECT_CUT_BY_SELECTION
                        : (command == IDM_VECTOR_SELECT_CONTAINED
                                  ? INKPOD_VECTOR_SELECT_FULLY_CONTAINED
                                  : (command == IDM_VECTOR_SELECT_LINE
                                            ? INKPOD_VECTOR_SELECT_LINE
                                            : (command == IDM_VECTOR_SELECT_WHOLE_LINE
                                                      ? INKPOD_VECTOR_SELECT_WHOLE_LINE
                                                      : (command == IDM_VECTOR_SELECT_INTERSECTION
                                                                ? INKPOD_VECTOR_SELECT_TO_INTERSECTION
                                                                : (command == IDM_VECTOR_SELECT_FILL_BOUNDARY
                                                                          ? INKPOD_VECTOR_SELECT_FILL_BOUNDARY
                                                                          : (command == IDM_VECTOR_SELECT_FILL
                                                                                    ? INKPOD_VECTOR_SELECT_FILL
                                                                                    : INKPOD_VECTOR_SELECT_TOUCHING))))));
                    const InkpodStatus status = SelectVectorObjects(*state);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ベクター選択");
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_VECTOR_RASTERIZE: {
                    TreePaneNode source{};
                    if (!QueryTreeNode(*state, false, source)
                        || source.kind != INKPOD_LAYER_VECTOR_COLORING) {
                        ShowCoreError(*state, window, L"ベクターをラスタライズ");
                        return 0;
                    }
                    static constexpr std::array<std::uint8_t, 10U> name{
                        'R','a','s','t','e','r','i','z','e','d'};
                    const InkpodVectorRasterizeInput input{
                        sizeof(InkpodVectorRasterizeInput),
                        0U,
                        INKPOD_VECTOR_RASTERIZE_ANTIALIAS,
                        source.id,
                        1U,
                        0U};
                    std::uint64_t layer_id{};
                    const InkpodStatus status = state->engine->Invoke(
                        [input, &layer_id](InkpodCore* core) {
                            InkpodDispatchResult result{};
                            result.struct_size = sizeof(result);
                            return inkpod_core_vector_rasterize_to_layer(
                                core,
                                &input,
                                name.data(),
                                name.size(),
                                &result,
                                &layer_id);
                        },
                        true,
                        true);
                    if (status == INKPOD_STATUS_OK) {
                        state->active_tree_layer_id = layer_id;
                        state->active_tree_plane_id = 0U;
                        RefreshTreePane(*state);
                    } else {
                        ShowCoreError(*state, window, L"ベクターをラスタライズ");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_VECTOR_VECTORIZE: {
                    TreePaneNode source{};
                    if (!QueryTreeNode(*state, true, source)) {
                        return 0;
                    }
                    const std::uint64_t source_plane_id = source.id;
                    std::uint64_t target_layer_id{};
                    const InkpodStatus status = state->engine->Invoke(
                        [source_plane_id, &target_layer_id](InkpodCore* core) {
                            static constexpr std::array<std::uint8_t, 10U> name{
                                'V','e','c','t','o','r','i','z','e','d'};
                            InkpodTreeEdit edit{};
                            edit.struct_size = sizeof(edit);
                            edit.operation = INKPOD_TREE_CREATE_LAYER;
                            edit.kind = INKPOD_LAYER_VECTOR_COLORING;
                            edit.name_utf8 = name.data();
                            edit.name_bytes = name.size();
                            InkpodDispatchResult result{};
                            result.struct_size = sizeof(result);
                            InkpodStatus create_status = inkpod_core_tree_edit(
                                core, &edit, &result, &target_layer_id);
                            if (create_status != INKPOD_STATUS_OK) {
                                return create_status;
                            }
                            const InkpodRasterVectorizeInput input{
                                sizeof(InkpodRasterVectorizeInput), 1U, 0U,
                                source_plane_id, target_layer_id};
                            std::uint64_t fill_count{};
                            return inkpod_core_raster_vectorize(
                                core, &input, &result, &fill_count);
                        },
                        true,
                        true);
                    if (status == INKPOD_STATUS_OK) {
                        state->active_tree_layer_id = target_layer_id;
                        state->active_tree_plane_id = 0U;
                        RefreshTreePane(*state);
                    } else {
                        ShowCoreError(*state, window, L"ラスターをベクター化");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_EFFECT_GRADIENT:
                case IDM_EFFECT_AIRBRUSH:
                case IDM_EFFECT_BOUNDARY_AIRBRUSH:
                case IDM_EFFECT_BLUR:
                case IDM_EFFECT_STAMP:
                case IDM_EFFECT_DUST:
                case IDM_EFFECT_ALPHA_GRADIENT:
                    if (state->plane == INKPOD_PLANE_COLOR
                        && ConfigureM6Effect(*state, LOWORD(wparam))) {
                        UpdateMenuState(*state);
                    }
                    return 0;
                case IDM_EFFECT_ALPHA_VIEW: {
                    const bool enabled = !state->alpha_view;
                    const InkpodStatus status = ApplyView(
                        *state,
                        INKPOD_VIEW_SET_ALPHA_VISIBLE,
                        enabled ? 1.0 : 0.0,
                        0.0);
                    if (status == INKPOD_STATUS_OK) {
                        state->alpha_view = enabled;
                    } else {
                        ShowCoreError(*state, window, L"アルファ表示");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_PLANE_MAIN_LINE:
                case IDM_PLANE_COLOR: {
                    state->plane = LOWORD(wparam) == IDM_PLANE_MAIN_LINE
                        ? INKPOD_PLANE_MAIN_LINE
                        : INKPOD_PLANE_COLOR;
                    const InkpodPlaneKind plane = state->plane;
                    const InkpodStatus plane_status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [plane](InkpodCore* core) {
                                  return inkpod_core_set_active_plane(core, plane);
                              },
                              false,
                              true);
                    if (plane_status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"プレーン切替");
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_COLOR_EDITOR: {
                    const InkpodStatus status = ShowDrawingColorEditor(*state);
                    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                        ShowCoreError(*state, window, L"描画色編集");
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_COLOR_SOURCE_TOPMOST:
                case IDM_COLOR_SOURCE_SELECTED:
                case IDM_COLOR_SOURCE_COMPOSITE:
                case IDM_COLOR_SOURCE_LIGHT_TABLE:
                    state->eyedropper_source = LOWORD(wparam) == IDM_COLOR_SOURCE_TOPMOST
                        ? INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT
                        : (LOWORD(wparam) == IDM_COLOR_SOURCE_SELECTED
                                  ? INKPOD_EYEDROPPER_SELECTED_PLANE
                                  : (LOWORD(wparam) == IDM_COLOR_SOURCE_LIGHT_TABLE
                                            ? INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST
                                            : INKPOD_EYEDROPPER_COMPOSITE));
                    UpdateMenuState(*state);
                    return 1;
                case IDM_PALETTE_REGISTER:
                case IDM_PALETTE_DELETE:
                case IDM_PALETTE_CLEAR: {
                    std::vector<InkpodColorValue> colors;
                    std::vector<std::wstring> names;
                    try {
                        colors = state->palette_colors;
                        names = state->color_chart_names;
                        if (LOWORD(wparam) == IDM_PALETTE_REGISTER) {
                            if (colors.size() >= 4096U) {
                                return 0;
                            }
                            colors.push_back(state->drawing_color);
                            names.push_back(L"Color " + std::to_wstring(colors.size()));
                        } else if (LOWORD(wparam) == IDM_PALETTE_DELETE) {
                            const LRESULT selected = SendMessageW(
                                state->color_palette_list, LB_GETCURSEL, 0, 0);
                            const std::size_t index = static_cast<std::size_t>(
                                state->palette_group) * 10U
                                + (selected == LB_ERR ? 0U : static_cast<std::size_t>(selected));
                            if (selected == LB_ERR || index >= colors.size()) {
                                return 0;
                            }
                            colors.erase(colors.begin() + static_cast<std::ptrdiff_t>(index));
                            names.erase(names.begin() + static_cast<std::ptrdiff_t>(index));
                        } else {
                            colors.clear();
                            names.clear();
                        }
                    } catch (const std::bad_alloc&) {
                        return 0;
                    }
                    const InkpodStatus status = ReplacePalette(*state, colors);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"カラーパレット編集");
                    } else {
                        state->color_chart_names = std::move(names);
                    }
                    RefreshColorPanes(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_PALETTE_NEXT_GROUP:
                    ++state->palette_group;
                    RefreshColorPanes(*state);
                    return 1;
                case IDM_PALETTE_SAVE:
                case IDM_PALETTE_LOAD: {
                    const bool save = LOWORD(wparam) == IDM_PALETTE_SAVE;
                    std::wstring path = state->smoke_test ? L"inkpod-palette-smoke.inkpalette" : L"";
                    if (!state->smoke_test && !ChoosePalettePath(window, save, path)) {
                        return 0;
                    }
                    if (save) {
                        return SavePaletteFile(path, state->palette_colors) ? 1 : 0;
                    }
                    std::vector<InkpodColorValue> colors;
                    if (!LoadPaletteFile(path, colors)) {
                        return 0;
                    }
                    const InkpodStatus status = ReplacePalette(*state, colors);
                    RefreshColorPanes(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_CHART_GENERATE: {
                    ViewOptionsDialogState dialog{};
                    dialog.title = L"セルからカラーチャートを作成";
                    dialog.labels = {L"最大色数", L"量子化で捨てる下位bit (0-7)", L"preview確認 (1)", nullptr};
                    dialog.values = {256, 2, 1, 0};
                    dialog.value_count = 3U;
                    if (state->smoke_test) {
                        dialog.values = {16, 4, 1, 0};
                    }
                    if (ShowViewOptions(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.values[0] <= 0 || dialog.values[1] < 0
                        || dialog.values[1] > 7 || dialog.values[2] == 0) {
                        return 0;
                    }
                    const InkpodStatus status = state->engine->Invoke(
                        [&dialog](InkpodCore* core) {
                            InkpodDispatchResult result{};
                            result.struct_size = sizeof(result);
                            return inkpod_core_palette_generate(
                                core,
                                static_cast<std::uint32_t>(dialog.values[0]),
                                static_cast<std::uint32_t>(dialog.values[1]),
                                &result);
                        },
                        true,
                        true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"カラーチャート生成");
                    }
                    state->color_chart_names.clear();
                    state->color_chart_page = 0U;
                    RefreshColorPanes(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_CHART_SEARCH: {
                    TextInputDialogState dialog{};
                    dialog.title = L"カラーチャート検索";
                    dialog.label = L"名前または番号";
                    dialog.value = state->smoke_test ? L"Smoke" : L"";
                    if (ShowTextInput(
                            state->instance, window, state->smoke_test, dialog) != IDOK) {
                        return 0;
                    }
                    wchar_t* end{};
                    const unsigned long number = std::wcstoul(dialog.value.c_str(), &end, 10);
                    std::size_t index = SIZE_MAX;
                    if (end != dialog.value.c_str() && *end == L'\0' && number != 0U
                        && number <= state->palette_colors.size()) {
                        index = number - 1U;
                    } else {
                        std::wstring needle = dialog.value;
                        std::transform(
                            needle.begin(), needle.end(), needle.begin(),
                            [](wchar_t value) { return std::towlower(value); });
                        for (std::size_t candidate = 0U;
                             candidate < state->color_chart_names.size(); ++candidate) {
                            std::wstring name = state->color_chart_names[candidate];
                            std::transform(
                                name.begin(), name.end(), name.begin(),
                                [](wchar_t value) { return std::towlower(value); });
                            if (name.find(needle) != std::wstring::npos) {
                                index = candidate;
                                break;
                            }
                        }
                    }
                    if (index == SIZE_MAX) {
                        return 0;
                    }
                    state->color_chart_page = static_cast<std::uint32_t>(index / 20U);
                    RefreshColorPanes(*state);
                    SendMessageW(
                        state->color_chart_list, LB_SETCURSEL,
                        static_cast<WPARAM>(index % 20U), 0);
                    return 1;
                }
                case IDM_CHART_NEXT: {
                    if (state->palette_colors.empty()) {
                        return 0;
                    }
                    LRESULT selected = SendMessageW(state->color_chart_list, LB_GETCURSEL, 0, 0);
                    std::size_t index = static_cast<std::size_t>(state->color_chart_page) * 20U
                        + (selected == LB_ERR ? 0U : static_cast<std::size_t>(selected) + 1U);
                    index %= state->palette_colors.size();
                    state->color_chart_page = static_cast<std::uint32_t>(index / 20U);
                    RefreshColorPanes(*state);
                    SendMessageW(
                        state->color_chart_list, LB_SETCURSEL,
                        static_cast<WPARAM>(index % 20U), 0);
                    return 1;
                }
                case IDM_CHART_LOCK:
                    state->color_chart_locked = !state->color_chart_locked;
                    UpdateMenuState(*state);
                    return 1;
                case IDM_CHART_NEXT_PAGE:
                    ++state->color_chart_page;
                    RefreshColorPanes(*state);
                    return 1;
                case IDM_CHART_RENAME: {
                    if (state->color_chart_locked) {
                        return 0;
                    }
                    const LRESULT selected = SendMessageW(
                        state->color_chart_list, LB_GETCURSEL, 0, 0);
                    const std::size_t index = static_cast<std::size_t>(state->color_chart_page)
                        * 20U + (selected == LB_ERR ? 0U : static_cast<std::size_t>(selected));
                    if (selected == LB_ERR || index >= state->color_chart_names.size()) {
                        return 0;
                    }
                    TextInputDialogState dialog{};
                    dialog.title = L"カラーチャート名";
                    dialog.label = L"名前";
                    dialog.value = state->smoke_test ? L"Smoke Color" : state->color_chart_names[index];
                    if (ShowTextInput(
                            state->instance, window, state->smoke_test, dialog) != IDOK
                        || dialog.value.empty() || dialog.value.size() > 256U) {
                        return 0;
                    }
                    try {
                        state->color_chart_names[index] = dialog.value;
                    } catch (const std::bad_alloc&) {
                        return 0;
                    }
                    RefreshColorPanes(*state);
                    SendMessageW(
                        state->color_chart_list, LB_SETCURSEL,
                        static_cast<WPARAM>(index % 20U), 0);
                    return 1;
                }
                case IDM_CHART_SAVE:
                case IDM_CHART_LOAD: {
                    const bool save = LOWORD(wparam) == IDM_CHART_SAVE;
                    std::wstring path = state->smoke_test ? L"inkpod-chart-smoke.inkchart" : L"";
                    if (!state->smoke_test && !ChooseChartPath(window, save, path)) {
                        return 0;
                    }
                    if (save) {
                        return SaveColorChartFile(
                                   path, state->palette_colors, state->color_chart_names)
                            ? 1
                            : 0;
                    }
                    std::vector<InkpodColorValue> colors;
                    std::vector<std::wstring> names;
                    if (!LoadColorChartFile(path, colors, names)) {
                        return 0;
                    }
                    const InkpodStatus status = ReplacePalette(*state, colors);
                    if (status == INKPOD_STATUS_OK) {
                        try {
                            state->color_chart_names = std::move(names);
                        } catch (const std::bad_alloc&) {
                            return 0;
                        }
                        state->color_chart_page = 0U;
                        RefreshColorPanes(*state);
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_CHART_COPY: {
                    const LRESULT selected = SendMessageW(
                        state->color_chart_list, LB_GETCURSEL, 0, 0);
                    const std::size_t index = static_cast<std::size_t>(state->color_chart_page)
                        * 20U + (selected == LB_ERR ? 0U : static_cast<std::size_t>(selected));
                    if (selected == LB_ERR || index >= state->palette_colors.size()) {
                        return 0;
                    }
                    const auto& selected_color = state->palette_colors[index];
                    std::array<wchar_t, 384U> text{};
                    _snwprintf_s(
                        text.data(), text.size(), _TRUNCATE,
                        selected_color.depth == INKPOD_COLOR_DEPTH_16
                            ? L"%ls\t#%04X%04X%04X%04X"
                            : L"%ls\t#%02X%02X%02X%02X",
                        state->color_chart_names[index].c_str(),
                        selected_color.red,
                        selected_color.green,
                        selected_color.blue,
                        selected_color.alpha);
                    const SIZE_T bytes = (wcslen(text.data()) + 1U) * sizeof(wchar_t);
                    HGLOBAL memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
                    if (memory == nullptr) {
                        return 0;
                    }
                    void* destination = GlobalLock(memory);
                    if (destination == nullptr) {
                        GlobalFree(memory);
                        return 0;
                    }
                    std::memcpy(destination, text.data(), bytes);
                    GlobalUnlock(memory);
                    if (OpenClipboard(window) == FALSE) {
                        GlobalFree(memory);
                        return 0;
                    }
                    EmptyClipboard();
                    const bool copied = SetClipboardData(CF_UNICODETEXT, memory) != nullptr;
                    if (!copied) {
                        GlobalFree(memory);
                    }
                    CloseClipboard();
                    return copied ? 1 : 0;
                }
                case IDM_CHART_CUT: {
                    if (state->color_chart_locked
                        || SendMessageW(window, WM_COMMAND, IDM_CHART_COPY, 0) != 1) {
                        return 0;
                    }
                    const LRESULT selected = SendMessageW(
                        state->color_chart_list, LB_GETCURSEL, 0, 0);
                    const std::size_t index = static_cast<std::size_t>(state->color_chart_page)
                        * 20U + (selected == LB_ERR ? 0U : static_cast<std::size_t>(selected));
                    if (selected == LB_ERR || index >= state->palette_colors.size()) {
                        return 0;
                    }
                    std::vector<InkpodColorValue> colors = state->palette_colors;
                    std::vector<std::wstring> names = state->color_chart_names;
                    colors.erase(colors.begin() + static_cast<std::ptrdiff_t>(index));
                    names.erase(names.begin() + static_cast<std::ptrdiff_t>(index));
                    const InkpodStatus status = ReplacePalette(*state, colors);
                    if (status == INKPOD_STATUS_OK) {
                        state->color_chart_names = std::move(names);
                        RefreshColorPanes(*state);
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_CHART_PASTE: {
                    if (state->color_chart_locked || OpenClipboard(window) == FALSE) {
                        return 0;
                    }
                    HANDLE handle = GetClipboardData(CF_UNICODETEXT);
                    const auto* text = handle == nullptr
                        ? nullptr
                        : static_cast<const wchar_t*>(GlobalLock(handle));
                    InkpodColorValue color{};
                    bool parsed{};
                    std::wstring pasted_name = L"Pasted";
                    const wchar_t* marker = text == nullptr ? nullptr : wcsrchr(text, L'#');
                    if (text != nullptr && marker != nullptr) {
                        if (marker > text) {
                            pasted_name.assign(text, marker);
                            while (!pasted_name.empty()
                                && (pasted_name.back() == L'\t'
                                    || iswspace(pasted_name.back()) != 0)) {
                                pasted_name.pop_back();
                            }
                        }
                        const std::size_t length = wcslen(marker + 1U);
                        wchar_t* end{};
                        const unsigned long long value = std::wcstoull(marker + 1U, &end, 16);
                        if (end != marker + 1U && *end == L'\0'
                            && (length == 8U || length == 16U)) {
                            color.struct_size = sizeof(color);
                            color.depth = length == 16U
                                ? INKPOD_COLOR_DEPTH_16
                                : INKPOD_COLOR_DEPTH_8;
                            const std::uint64_t mask = length == 16U ? UINT16_MAX : UINT8_MAX;
                            const unsigned shift = length == 16U ? 16U : 8U;
                            color.alpha = static_cast<std::uint16_t>(value & mask);
                            color.blue = static_cast<std::uint16_t>((value >> shift) & mask);
                            color.green = static_cast<std::uint16_t>((value >> (shift * 2U)) & mask);
                            color.red = static_cast<std::uint16_t>((value >> (shift * 3U)) & mask);
                            parsed = true;
                        }
                    }
                    if (text != nullptr) {
                        GlobalUnlock(handle);
                    }
                    CloseClipboard();
                    if (!parsed) {
                        return 0;
                    }
                    SetDrawingColor(*state, color);
                    std::vector<InkpodColorValue> colors = state->palette_colors;
                    if (colors.size() >= 4096U) {
                        return 0;
                    }
                    colors.push_back(color);
                    const InkpodStatus status = ReplacePalette(*state, colors);
                    if (status == INKPOD_STATUS_OK) {
                        RefreshColorPanes(*state);
                        state->color_chart_names.back() = pasted_name.empty()
                            ? L"Pasted"
                            : pasted_name;
                        state->color_chart_page = static_cast<std::uint32_t>(
                            (state->palette_colors.size() - 1U) / 20U);
                        RefreshColorPanes(*state);
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                case IDM_COLOR_CHOOSE: {
                    static std::array<COLORREF, 16> custom_colors{};
                    CHOOSECOLORW choose{};
                    choose.lStructSize = sizeof(choose);
                    choose.hwndOwner = window;
                    choose.rgbResult = RGB(
                        (state->color_rgba >> 24) & 0xffU,
                        (state->color_rgba >> 16) & 0xffU,
                        (state->color_rgba >> 8) & 0xffU);
                    choose.lpCustColors = custom_colors.data();
                    choose.Flags = CC_FULLOPEN | CC_RGBINIT;
                    if (ChooseColorW(&choose) != FALSE) {
                        SetDrawingColor(
                            *state,
                            InkpodColorValue{
                                sizeof(InkpodColorValue),
                                INKPOD_COLOR_DEPTH_8,
                                GetRValue(choose.rgbResult),
                                GetGValue(choose.rgbResult),
                                GetBValue(choose.rgbResult),
                                static_cast<std::uint16_t>(state->color_rgba & 0xffU)});
                    }
                    return 0;
                }
                case IDM_COLOR_CHECK_OFF:
                case IDM_COLOR_CHECK_LEGACY:
                case IDM_COLOR_CHECK_NATIVE: {
                    const InkpodColorCheckMode mode = LOWORD(wparam) == IDM_COLOR_CHECK_LEGACY
                        ? INKPOD_COLOR_CHECK_LEGACY_WHITE
                        : (LOWORD(wparam) == IDM_COLOR_CHECK_NATIVE
                                  ? INKPOD_COLOR_CHECK_NATIVE_ALPHA
                                  : INKPOD_COLOR_CHECK_OFF);
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [mode](InkpodCore* core) {
                                  return inkpod_core_set_color_check(core, mode);
                              },
                              true,
                              true);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"彩色チェック表示");
                    } else {
                        state->color_check_mode = mode;
                    }
                    UpdateMenuState(*state);
                    return 0;
                }
                case IDM_SHORTCUT_RESET: {
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [](InkpodCore* core) {
                                  return inkpod_core_shortcut_reset(core);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ショートカットの初期化");
                    }
                    return 0;
                }
                case IDM_SHORTCUT_EDIT: {
                    ShortcutDialogState dialog_state{};
                    if (ShowShortcutEditor(
                            state->instance,
                            window,
                            state->smoke_test,
                            dialog_state) != IDOK) {
                        return 0;
                    }
                    const InkpodStatus status = state->engine == nullptr
                        ? INKPOD_STATUS_INVALID_STATE
                        : state->engine->Invoke(
                              [dialog_state](InkpodCore* core) {
                                  return inkpod_core_shortcut_rebind(
                                      core,
                                      dialog_state.command_id,
                                      dialog_state.virtual_key,
                                      dialog_state.modifiers);
                              },
                              false,
                              false);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"ショートカット編集");
                        return 0;
                    }
                    return 1;
                }
                case IDM_HELP_ABOUT:
                    return ShowAboutDialog(
                               state->instance,
                               window,
                               state->smoke_test)
                            == IDOK
                        ? 1
                        : 0;
                case IDM_APP_EXIT:
                    SendMessageW(window, WM_CLOSE, 0, 0);
                    return 0;
                default:
                    break;
            }
            break;
        case WM_KEYDOWN:
        case WM_SYSKEYDOWN:
            if (state != nullptr) {
                if (state->floating_active && wparam == VK_RETURN) {
                    SendMessageW(window, WM_COMMAND, IDM_EDIT_FLOATING_COMMIT, 0);
                    return 0;
                }
                if (state->floating_active && wparam == VK_ESCAPE) {
                    SendMessageW(window, WM_COMMAND, IDM_EDIT_FLOATING_CANCEL, 0);
                    return 0;
                }
                if (state->motion_active && wparam == VK_ESCAPE) {
                    SendMessageW(window, WM_COMMAND, IDM_MOTION_STOP, 0);
                    return 0;
                }
                if (state->motion_active && wparam == VK_SPACE) {
                    SendMessageW(window, WM_COMMAND, IDM_MOTION_PAUSE, 0);
                    return 0;
                }
                if (state->motion_active
                    && (wparam == VK_LEFT || wparam == VK_RIGHT
                        || wparam == VK_HOME || wparam == VK_END)) {
                    const UINT command = wparam == VK_LEFT
                        ? IDM_MOTION_PREVIOUS
                        : (wparam == VK_RIGHT
                                  ? IDM_MOTION_NEXT
                                  : (wparam == VK_HOME ? IDM_MOTION_FIRST : IDM_MOTION_LAST));
                    SendMessageW(window, WM_COMMAND, command, 0);
                    return 0;
                }
                const std::uint32_t modifiers = CurrentShortcutModifiers(lparam);
                if (modifiers
                        == (INKPOD_SHORTCUT_MODIFIER_CONTROL
                            | INKPOD_SHORTCUT_MODIFIER_ALT)
                    && (wparam == '3' || wparam == '2' || wparam == '4'
                        || wparam == '1' || wparam == '0' || wparam == '8')) {
                    const UINT command = wparam == '3'
                        ? IDM_MOTION_FPS_30
                        : (wparam == '2'
                                  ? IDM_MOTION_FPS_25
                                  : (wparam == '4'
                                            ? IDM_MOTION_FPS_24
                                            : (wparam == '1'
                                                      ? IDM_MOTION_FPS_12
                                                      : (wparam == '0'
                                                                ? IDM_MOTION_FPS_10
                                                                : IDM_MOTION_FPS_8))));
                    SendMessageW(window, WM_COMMAND, command, 0);
                    return 0;
                }
                if (modifiers == 0U && wparam == VK_TAB) {
                    SendMessageW(window, WM_COMMAND, IDM_PALETTE_NEXT_GROUP, 0);
                    return 0;
                }
                if (modifiers == 0U && wparam >= '0' && wparam <= '9') {
                    const std::size_t digit = wparam == '0'
                        ? 9U
                        : static_cast<std::size_t>(wparam - '1');
                    const std::size_t index = state->palette_group * 10U + digit;
                    if (index < state->palette_colors.size()) {
                        SetDrawingColor(*state, state->palette_colors[index]);
                        InvalidateRect(state->canvas, nullptr, FALSE);
                    }
                    return 0;
                }
                UINT menu_command{};
                if (ResolveConfiguredShortcut(
                        *state,
                        static_cast<std::uint32_t>(wparam),
                        modifiers,
                        menu_command)) {
                    SendMessageW(window, WM_COMMAND, menu_command, 0);
                    return 0;
                }
                if (modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL && wparam == 'S') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_SAVE, 0);
                    return 0;
                }
                if (modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL && wparam == 'N') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_NEW, 0);
                    return 0;
                }
                if (modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL && wparam == 'O') {
                    SendMessageW(window, WM_COMMAND, IDM_FILE_OPEN, 0);
                    return 0;
                }
            }
            break;
        case inkpod::renderer::kCanvasPointerMoved:
            if (state != nullptr) {
                UpdateLocatorDisplay(
                    *state, GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam));
                return 1;
            }
            return 0;
        case inkpod::renderer::kCanvasStrokeReady:
            if (state != nullptr) {
                const auto* input = reinterpret_cast<
                    const inkpod::renderer::CanvasStrokeEvent*>(lparam);
                if (input == nullptr || state->engine == nullptr
                    || input->sample_count > UINT64_C(1048576)
                    || (input->sample_count != 0U && input->samples == nullptr)) {
                    return 0;
                }
                if (state->guide_drag_active) {
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->guide_drag_active = false;
                        state->guide_drag_axis = 0U;
                        state->guide_drag_id = 0U;
                    } else if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End
                        && input->sample_count != 0U) {
                        const InkpodStatus status = FinishGuideDrag(
                            *state,
                            input->samples[static_cast<std::size_t>(
                                input->sample_count - 1U)]);
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"ガイド移動");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                    && input->sample_count != 0U
                    && BeginGuideDrag(*state, input->samples[0])) {
                    return 1;
                }
                if (state->tool == kInteractionGuideMove) {
                    return 1;
                }
                if (state->tool == kInteractionBoxZoom) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->view_gesture_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->view_gesture_samples.insert(
                                state->view_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->view_gesture_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->view_gesture_samples.clear();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = state->view_gesture_samples.size() < 2U
                            ? INKPOD_STATUS_INVALID_ARGUMENT
                            : ApplyBoxZoomGesture(
                                  *state,
                                  state->view_gesture_samples.front(),
                                  state->view_gesture_samples.back());
                        state->view_gesture_samples.clear();
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"範囲拡大");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (state->tool == kInteractionEyedropper
                    && input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                    && input->sample_count != 0U) {
                    const InkpodStatus status = EyedropAtDevicePoint(
                        *state, input->samples[0].x, input->samples[0].y);
                    if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                        ShowCoreError(*state, window, L"スポイト");
                    }
                    UpdateMenuState(*state);
                    return 1;
                }
                if (state->tool == kInteractionEyedropper) {
                    return 1;
                }
                if (state->tool == kInteractionFloatingTransform) {
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        if (!state->floating_gesture_samples.empty()) {
                            SetFloatingTransform(*state, state->floating_drag_start);
                        }
                        state->floating_gesture_samples.clear();
                        state->floating_drag_mode = 0U;
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                        state->floating_gesture_samples.clear();
                    }
                    if (input->sample_count != 0U) {
                        try {
                            state->floating_gesture_samples.push_back(
                                input->samples[static_cast<std::size_t>(input->sample_count - 1U)]);
                        } catch (const std::bad_alloc&) {
                            state->floating_gesture_samples.clear();
                            return 0;
                        }
                    }
                    if (!state->floating_gesture_samples.empty()) {
                        const InkpodStatus status = UpdateFloatingHandleDrag(
                            *state,
                            state->floating_gesture_samples.front(),
                            state->floating_gesture_samples.back(),
                            input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin);
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"フローティングハンドル変形");
                        }
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        state->floating_gesture_samples.clear();
                        state->floating_drag_mode = 0U;
                    }
                    return 1;
                }
                if (state->tool == kInteractionLightTableMove) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->light_table_move_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->light_table_move_samples.push_back(
                                input->samples[static_cast<std::size_t>(input->sample_count - 1U)]);
                        }
                    } catch (const std::bad_alloc&) {
                        state->light_table_move_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->light_table_move_samples.clear();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = MoveLightTableFromCanvas(*state);
                        state->light_table_move_samples.clear();
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"ライトテーブル移動");
                        }
                        RefreshLightTablePane(*state);
                    }
                    return 1;
                }
                if (IsVectorCanvasTool(state->tool)) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            ClearVectorGeometryPreview(*state);
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->vector_gesture_samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                ClearVectorGeometryPreview(*state);
                                return 0;
                            }
                            state->vector_gesture_samples.insert(
                                state->vector_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        ClearVectorGeometryPreview(*state);
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        ClearVectorGeometryPreview(*state);
                        return 1;
                    }
                    if (state->tool != kInteractionVectorEraser) {
                        UpdateVectorGeometryPreview(*state);
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = FinishVectorCanvasGesture(*state);
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"ベクターCanvas操作");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (state->tool == kInteractionSelection) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->selection_gesture_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->selection_gesture_samples.insert(
                                state->selection_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->selection_gesture_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->selection_gesture_samples.clear();
                        return 1;
                    }
                    if (state->selection_shape == INKPOD_SELECTION_WAND) {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            const InkpodStatus status = ApplySelectionGesture(
                                *state, state->selection_gesture_samples);
                            state->selection_gesture_samples.clear();
                            if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                                ShowCoreError(*state, window, L"色の杖");
                            }
                            UpdateMenuState(*state);
                        }
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = ApplySelectionGesture(
                            *state, state->selection_gesture_samples);
                        state->selection_gesture_samples.clear();
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"選択範囲");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (state->tool == kInteractionFill) {
                    if (state->fill_options.operation == INKPOD_FILL_SEED) {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                            && input->sample_count != 0U) {
                            const InkpodStatus status = ApplyFillAtDevicePoint(
                                *state, input->samples[0].x, input->samples[0].y);
                            if (status != INKPOD_STATUS_OK
                                && status != INKPOD_STATUS_FILL_OVERFLOW
                                && !state->smoke_test) {
                                ShowCoreError(*state, window, L"フィル");
                            }
                            UpdateMenuState(*state);
                        }
                        return 1;
                    }
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->fill_gesture_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->fill_gesture_samples.insert(
                                state->fill_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->fill_gesture_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->fill_gesture_samples.clear();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = state->fill_gesture_samples.size() < 2U
                            ? INKPOD_STATUS_INVALID_ARGUMENT
                            : ApplyFillAtDeviceRange(
                                  *state,
                                  state->fill_gesture_samples.front().x,
                                  state->fill_gesture_samples.front().y,
                                  state->fill_gesture_samples.back().x,
                                  state->fill_gesture_samples.back().y,
                                  true);
                        state->fill_gesture_samples.clear();
                        if (status != INKPOD_STATUS_OK
                            && status != INKPOD_STATUS_FILL_OVERFLOW
                            && !state->smoke_test) {
                            ShowCoreError(*state, window, L"範囲フィル");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                const bool m6_interaction = state->tool >= kInteractionM6Gradient
                    && state->tool <= kInteractionM6AlphaGradient;
                if (m6_interaction) {
                    if (state->tool == kInteractionM6Stamp
                        && input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                        && input->sample_count != 0U
                        && (GetKeyState(VK_MENU) & 0x8000) != 0) {
                        state->stamp_source = input->samples[0];
                        state->stamp_source_valid = true;
                        state->m6_samples.clear();
                        return 1;
                    }
                    if (state->m6_task != nullptr) {
                        return 1;
                    }
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->m6_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->m6_samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                state->m6_samples.clear();
                                state->m6_airbrush_active = false;
                                KillTimer(window, kM6ContinuousSprayTimer);
                                return 0;
                            }
                            state->m6_samples.insert(
                                state->m6_samples.end(),
                                input->samples,
                                input->samples + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->m6_samples.clear();
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                        return 0;
                    }
                    if (state->tool == kInteractionM6Airbrush
                        && input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                        && input->sample_count != 0U) {
                        state->m6_airbrush_last =
                            input->samples[static_cast<std::size_t>(input->sample_count - 1U)];
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->m6_airbrush_active = true;
                            SetTimer(
                                window,
                                kM6ContinuousSprayTimer,
                                kM6ContinuousSprayIntervalMilliseconds,
                                nullptr);
                        }
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->m6_samples.clear();
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                        const InkpodStatus status = FinishM6CanvasGesture(*state);
                        if (status != INKPOD_STATUS_OK && !state->smoke_test) {
                            ShowCoreError(*state, window, L"M6 Canvas効果");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                inkpod::app::StrokeEvent event{};
                switch (input->kind) {
                    case inkpod::renderer::CanvasStrokeEventKind::Begin:
                        event.kind = inkpod::app::StrokeEventKind::Begin;
                        break;
                    case inkpod::renderer::CanvasStrokeEventKind::Append:
                        event.kind = inkpod::app::StrokeEventKind::Append;
                        break;
                    case inkpod::renderer::CanvasStrokeEventKind::End:
                        event.kind = inkpod::app::StrokeEventKind::End;
                        break;
                    case inkpod::renderer::CanvasStrokeEventKind::Cancel:
                        event.kind = inkpod::app::StrokeEventKind::Cancel;
                        break;
                }
                event.style = inkpod::app::StrokeStyle{
                    static_cast<InkpodPaintTool>(state->tool),
                    state->plane,
                    INKPOD_COORDINATE_SPACE_DEVICE,
                    state->tool == INKPOD_TOOL_PENCIL ? INKPOD_STROKE_FLAG_AUTO_ERASE
                                                      : INKPOD_STROKE_FLAG_PRESSURE_SIZE,
                    state->color_rgba,
                    state->tool == INKPOD_TOOL_PENCIL ? 1.0F : state->diameter};
                try {
                    if (input->sample_count != 0U) {
                        event.samples.assign(
                            input->samples,
                            input->samples + static_cast<std::size_t>(input->sample_count));
                    }
                } catch (const std::bad_alloc&) {
                    return 0;
                }
                return state->engine->EnqueueStroke(std::move(event)) ? 1 : 0;
            }
            return 0;
        case inkpod::renderer::kCanvasViewGesture:
            if (state != nullptr) {
                const auto* gesture = reinterpret_cast<
                    const inkpod::renderer::CanvasViewGesture*>(lparam);
                if (gesture != nullptr
                    && ApplyView(
                           *state,
                           gesture->kind,
                           gesture->value1,
                           gesture->value2,
                           gesture->value3) == INKPOD_STATUS_OK) {
                    return 1;
                }
            }
            return 0;
        case inkpod::renderer::kCanvasViewportChanged:
            if (state != nullptr && state->engine != nullptr && wparam != 0U && lparam != 0) {
                ApplyView(
                    *state,
                    INKPOD_VIEW_VIEWPORT_RESIZED,
                    static_cast<double>(wparam),
                    static_cast<double>(lparam));
            }
            return 0;
        case inkpod::app::kCoreStateChanged:
            if (state != nullptr) {
                RefreshTreePane(*state);
                RefreshLightTablePane(*state);
                RefreshSequencePane(*state);
                RefreshColorPanes(*state);
                UpdateMenuState(*state);
            }
            return 0;
        case kM6TaskCompleted:
            if (state != nullptr) {
                const InkpodStatus status = static_cast<InkpodStatus>(wparam);
                const bool prompt = state->m6_preview_prompt;
                state->m6_preview_prompt = false;
                if (state->m6_progress != nullptr) {
                    DestroyWindow(state->m6_progress);
                    state->m6_progress = nullptr;
                }
                inkpod_m6_task_release(&state->m6_task);
                if (status == INKPOD_STATUS_OK && prompt && state->engine != nullptr) {
                    const int choice = MessageBoxW(
                        window,
                        L"Canvasのプレビューを適用しますか？\nキャンセルすると元の状態へ完全に戻ります。",
                        L"M6 プレビュー",
                        MB_OKCANCEL | MB_ICONQUESTION);
                    const InkpodStatus preview_status = state->engine->Invoke(
                        [choice](InkpodCore* core) {
                            if (choice == IDOK) {
                                InkpodDispatchResult result{};
                                result.struct_size = sizeof(result);
                                return inkpod_core_filter_preview_apply(core, &result);
                            }
                            InkpodFilterPreviewInfo info{};
                            info.struct_size = sizeof(info);
                            return inkpod_core_filter_preview_cancel(core, &info);
                        },
                        true,
                        true);
                    if (preview_status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, L"M6プレビューの確定");
                    }
                }
                UpdateMenuState(*state);
            }
            return 0;
        case inkpod::app::kCoreAsyncFailed:
            if (state != nullptr && !state->smoke_test) {
                ShowCoreError(*state, window, L"非同期処理");
            }
            return 0;
        case WM_TIMER:
            if (state != nullptr && wparam == kM6ContinuousSprayTimer) {
                if (state->m6_airbrush_active && state->tool == kInteractionM6Airbrush
                    && state->m6_task == nullptr) {
                    try {
                        if (state->m6_samples.size() < UINT64_C(1048576)) {
                            state->m6_samples.push_back(state->m6_airbrush_last);
                        } else {
                            state->m6_airbrush_active = false;
                            KillTimer(window, kM6ContinuousSprayTimer);
                        }
                    } catch (const std::bad_alloc&) {
                        state->m6_airbrush_active = false;
                        KillTimer(window, kM6ContinuousSprayTimer);
                    }
                }
                return 0;
            }
            if (state != nullptr && wparam == kAutosaveTimer && !state->recovery_path.empty()) {
                InkpodDocumentInfo info{};
                if (QueryDocument(*state, info)
                    && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
                    QueueAutosave(*state, state->recovery_path);
                }
                return 0;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr && !state->smoke_test && !ConfirmDiscard(*state)) {
                return 0;
            }
            if (state != nullptr && state->m6_task != nullptr) {
                inkpod_m6_task_cancel(state->m6_task);
            }
            if (state != nullptr) {
                state->m6_airbrush_active = false;
                KillTimer(window, kM6ContinuousSprayTimer);
            }
            ShowWindow(window, SW_HIDE);
            PostQuitMessage(0);
            return 0;
        case inkpod::renderer::kCanvasRenderFailed:
            if (state == nullptr || !state->smoke_test) {
                MessageBoxW(
                    window,
                    L"Canvas renderer の描画に失敗しました。",
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
            }
            if (state == nullptr || !state->smoke_test) {
                SendMessageW(window, WM_CLOSE, 0, 0);
            }
            return 0;
        case WM_NCDESTROY:
            KillTimer(window, kAutosaveTimer);
            KillTimer(window, kM6ContinuousSprayTimer);
            KillTimer(window, kMotionPlaybackTimer);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

bool RegisterMainWindowClass(
    HINSTANCE instance, const wchar_t* class_name) noexcept {
    const auto app_icon = LoadIconW(instance, MAKEINTRESOURCEW(IDI_APP_ICON));
    const auto small_icon = reinterpret_cast<HICON>(LoadImageW(
        instance,
        MAKEINTRESOURCEW(IDI_APP_ICON),
        IMAGE_ICON,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
        LR_DEFAULTCOLOR | LR_SHARED));
    if (app_icon == nullptr) {
        return false;
    }
    WNDCLASSEXW window_class{};
    window_class.cbSize = sizeof(window_class);
    window_class.style = CS_HREDRAW | CS_VREDRAW;
    window_class.lpfnWndProc = MainWindowProcedure;
    window_class.hInstance = instance;
    window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    window_class.hIcon = app_icon;
    window_class.hbrBackground = nullptr;
    window_class.lpszMenuName = MAKEINTRESOURCEW(IDR_MAIN_MENU);
    window_class.lpszClassName = class_name;
    window_class.hIconSm = small_icon != nullptr ? small_icon : app_icon;
    return RegisterClassExW(&window_class) != 0;
}

}  // namespace

int APIENTRY wWinMain(
    HINSTANCE instance,
    HINSTANCE,
    wchar_t* command_line,
    int show_command) {
    if (command_line != nullptr
        && std::wcsstr(command_line, L"--abi-smoke-test") != nullptr) {
        return InkpodRunAbiSmoke();
    }
    INITCOMMONCONTROLSEX controls{};
    controls.dwSize = sizeof(controls);
    controls.dwICC = ICC_STANDARD_CLASSES | ICC_BAR_CLASSES;
    if (!InitCommonControlsEx(&controls)) {
        MessageBoxW(
            nullptr,
            L"Common Controls の初期化に失敗しました。",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 10;
    }

    ComApartment com;
    if (FAILED(com.Initialize())) {
        MessageBoxW(
            nullptr,
            L"COM の初期化に失敗しました。",
            L"inkpod",
            MB_OK | MB_ICONERROR);
        return 11;
    }

    std::array<wchar_t, 128> title{};
    std::array<wchar_t, 128> class_name{};
    if (LoadStringW(
            instance,
            IDS_APP_TITLE,
            title.data(),
            static_cast<int>(title.size())) == 0
        || LoadStringW(
               instance,
               IDS_MAIN_WINDOW_CLASS,
               class_name.data(),
               static_cast<int>(class_name.size())) == 0) {
        return 12;
    }
    if (!inkpod::renderer::RegisterCanvasClass(instance)
        || !RegisterMainWindowClass(instance, class_name.data())) {
        return 13;
    }

    AppState state{};
    state.instance = instance;
    state.smoke_test = command_line != nullptr
        && std::wcsstr(command_line, L"--smoke-test") != nullptr;
    HWND window = CreateWindowExW(
        0,
        class_name.data(),
        title.data(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1024,
        720,
        nullptr,
        nullptr,
        instance,
        &state);
    if (window == nullptr) {
        return 14;
    }

    InkpodStatus core_status = InitializeCore(state);
    if (core_status != INKPOD_STATUS_OK) {
        if (!state.smoke_test) {
            ShowCoreError(state, window, L"Rust Core の初期化");
        }
        ShutdownCore(state);
        DestroyWindow(window);
        return 15;
    }

    bool document_initialized{};
    if (!state.smoke_test) {
        std::wstring recovery;
        if (NewestPrivateRecovery(recovery)) {
            const int choice = MessageBoxW(
                window,
                L"未処理のRecoveryがあります。\n\n"
                L"はい: Recoveryを開く\nいいえ: Recoveryを破棄\n"
                L"キャンセル: 後で判断して新規セルを開く",
                L"inkpod Recovery",
                MB_YESNOCANCEL | MB_ICONQUESTION);
            if (choice == IDYES) {
                core_status = OpenRecoveryFromPath(state, recovery);
                document_initialized = core_status == INKPOD_STATUS_OK;
                if (!document_initialized) {
                    ShowCoreError(state, window, L"起動時Recoveryを開く");
                    core_status = INKPOD_STATUS_OK;
                }
            } else if (choice == IDNO
                && DeleteFileW(recovery.c_str()) == FALSE
                && GetLastError() != ERROR_FILE_NOT_FOUND) {
                MessageBoxW(
                    window,
                    L"Recoveryを削除できませんでした。ファイルを残して新規セルを開きます。",
                    L"inkpod Recovery",
                    MB_OK | MB_ICONWARNING);
            }
        }
    }
    if (core_status == INKPOD_STATUS_OK && !document_initialized) {
        core_status = CreateDefaultCell(state);
        document_initialized = core_status == INKPOD_STATUS_OK;
    }
    if (core_status != INKPOD_STATUS_OK || !document_initialized) {
        if (!state.smoke_test) {
            ShowCoreError(state, window, L"セルまたはRecoveryの初期化");
        }
        ShutdownCore(state);
        DestroyWindow(window);
        return 16;
    }
    UpdateMenuState(state);

    int exit_code = 0;
    if (state.smoke_test) {
        exit_code = RunM1Smoke(state);
        if (exit_code == 0) {
            exit_code = RunM2Smoke(state);
        }
        if (exit_code == 0) {
            exit_code = RunM3Smoke(state);
        }
        if (exit_code == 0) {
            exit_code = RunM4Smoke(state);
        }
        if (exit_code == 0) {
            exit_code = RunM5Smoke(state);
        }
        if (exit_code == 0) {
            exit_code = RunM6Smoke(state);
        }
    } else {
        ShowWindow(window, show_command);
        UpdateWindow(window);
        MSG message{};
        BOOL result{};
        while ((result = GetMessageW(&message, nullptr, 0, 0)) > 0) {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        exit_code = result == -1 ? 17 : static_cast<int>(message.wParam);
    }

    core_status = ShutdownCore(state);
    DestroyWindow(window);
    if (core_status != INKPOD_STATUS_OK && exit_code == 0) {
        return 18;
    }
    return exit_code;
}
