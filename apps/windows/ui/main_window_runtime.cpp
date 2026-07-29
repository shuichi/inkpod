#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
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
#include <new>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "app/app_context.h"
#include "canvas.h"
#include "app/clipboard_adapter.h"
#include "app/core_engine.h"
#include "app/document_shell.h"
#include "inkpod/core_ffi.h"
#include "app/resource.h"
#include "ui/dialogs/about_dialog.h"
#include "ui/dialogs/basic_dialogs.h"
#include "ui/dialogs/batch_dialog.h"
#include "ui/dialogs/effects_dialogs.h"
#include "ui/dialogs/layer_palette.h"
#include "ui/command_state.h"
#include "ui/command_catalog.h"
#include "ui/shortcut_controller.h"
#include "ui/panes/document_panes.h"
#include "ui/panes/color_panes.h"
#include "ui/panes/color_dock_pane.h"
#include "ui/panes/tool_options_pane.h"
#include "ui/tools/fill_controller.h"
#include "ui/tools/floating_paste_controller.h"
#include "ui/tools/selection_controller.h"
#include "ui/tools/tool_state.h"
#include "ui/tools/view_controller.h"
#include "ui/tools/vector_controller.h"
#include "ui/effects_controller.h"
#include "ui/batch_controller.h"
#include "ui/main_window.h"
#include "ui/main_window_runtime.h"

namespace inkpod::windows::ui::runtime {

inline constexpr UINT kLocatorSampleReady = WM_APP + 0x172U;

using inkpod::windows::ui::HistoryDialogState;
using inkpod::windows::ui::EffectEditorState;
using inkpod::windows::ui::ShortcutDialogState;
using inkpod::windows::ui::TextInputDialogState;
using inkpod::windows::ui::ViewOptionsDialogState;
using inkpod::windows::ui::ShowAboutDialog;
using inkpod::windows::ui::ShowHistoryDialog;
using inkpod::windows::ui::ShowEffectEditor;
using inkpod::windows::ui::ShowShortcutEditor;
using inkpod::windows::ui::ShowTextInput;
using inkpod::windows::ui::ShowViewOptions;
using inkpod::app::AnimationUiState;
using inkpod::app::AppContext;
using inkpod::app::BatchOperationUi;
using inkpod::app::BatchUiState;
using inkpod::app::DocumentShellState;
using inkpod::app::DocumentShellController;
using inkpod::app::EffectsUiState;
using inkpod::app::AdjustmentLayerUiState;
using inkpod::app::FilterJob;
using inkpod::app::GradientStopValue;
using inkpod::app::CanvasEffectOptions;
using inkpod::app::PaneUiState;
using inkpod::app::ToolUiState;
using inkpod::app::ViewUiState;
using inkpod::app::ImportStandardClipboard;
using inkpod::app::InkpodClipboardFormat;
using inkpod::app::PublishStandardClipboard;
using inkpod::app::ChooseCommonRasterPath;
using inkpod::app::ChooseCommonRasterPaths;
using inkpod::app::ChooseInkpodPath;
using inkpod::app::ChooseOpenDocumentPath;
using inkpod::app::CommonRasterFormatFromPath;
using inkpod::app::NewestPrivateRecovery;
using inkpod::app::PrivateRecoveryPath;
using inkpod::app::ReadBoundedFile;
using inkpod::app::RecoveryIsNewer;
using inkpod::app::WidePathToUtf8;
using inkpod::app::WriteFileAtomically;
using inkpod::windows::ui::ApplyCommandStates;
using inkpod::windows::ui::ApplyShortcutLabelsToMenu;
using inkpod::windows::ui::ClearPendingShortcut;
using inkpod::windows::ui::MenuCommandDisplayName;
using inkpod::windows::ui::RebindShortcut;
using inkpod::windows::ui::ResetShortcuts;
using inkpod::windows::ui::ResolveShortcutStroke;
using inkpod::windows::ui::CommandStateInputs;
using inkpod::windows::ui::ComputeCommandStates;
using inkpod::windows::ui::IsCommandEnabled;
using inkpod::windows::ui::tools::CancelVectorGeometryPreview;
using inkpod::windows::ui::tools::CancelSelectionGeometryPreview;
using inkpod::windows::ui::tools::HandleActivePlaneTransition;
using inkpod::windows::ui::tools::IsVectorCanvasTool;
using inkpod::windows::ui::tools::IsVectorStrokePlane;
using inkpod::windows::ui::tools::TransitionActiveTool;
using inkpod::windows::ui::tools::kInteractionBoxZoom;
using inkpod::windows::ui::tools::kInteractionEyedropper;
using inkpod::windows::ui::tools::kInteractionFill;
using inkpod::windows::ui::tools::kInteractionFloatingTransform;
using inkpod::windows::ui::tools::kInteractionGuideMove;
using inkpod::windows::ui::tools::kInteractionLightTableMove;
using inkpod::windows::ui::tools::kInteractionEffectAirbrush;
using inkpod::windows::ui::tools::kInteractionEffectAlphaGradient;
using inkpod::windows::ui::tools::kInteractionEffectBlur;
using inkpod::windows::ui::tools::kInteractionEffectDust;
using inkpod::windows::ui::tools::kInteractionEffectGradient;
using inkpod::windows::ui::tools::kInteractionEffectStamp;
using inkpod::windows::ui::tools::kInteractionSelection;
using inkpod::windows::ui::tools::kInteractionVectorCurve;
using inkpod::windows::ui::tools::kInteractionVectorEllipse;
using inkpod::windows::ui::tools::kInteractionVectorEraser;
using inkpod::windows::ui::tools::kInteractionVectorLine;
using inkpod::windows::ui::tools::kInteractionVectorPolyline;
using inkpod::windows::ui::tools::kInteractionVectorRectangle;
constexpr UINT_PTR kAutosaveTimer = 1U;
constexpr UINT kAutosaveIntervalMilliseconds = 60U * 1000U;
constexpr UINT kEffectTaskCompleted = WM_APP + 0x170U;
constexpr UINT kBatchTaskCompleted = WM_APP + 0x171U;
constexpr UINT_PTR kEffectProgressTimer = 1U;
constexpr UINT_PTR kContinuousSprayTimer = 2U;
constexpr UINT_PTR kMotionPlaybackTimer = 3U;
constexpr UINT_PTR kShortcutSequenceTimer = 4U;
constexpr UINT kShortcutSequenceTimerMilliseconds = 100U;
constexpr UINT_PTR kStatusProgressTimer = 5U;
constexpr UINT kStatusProgressTimerMilliseconds = 100U;
constexpr UINT kContinuousSprayIntervalMilliseconds = 50U;
constexpr wchar_t kVectorStrokePlaneRequired[] =
    L"ベクター描画には、ベクター主線または色トレース線プレーンの選択が必要です。";
constexpr std::array<ViewOptionsDialogState::Choice, 10U> kLayerKindChoices{{
    {L"2値彩色", INKPOD_LAYER_BINARY_COLORING},
    {L"階調彩色", INKPOD_LAYER_GRAYSCALE_COLORING},
    {L"ラスター汎用", INKPOD_LAYER_RASTER},
    {L"選択範囲", INKPOD_LAYER_SELECTION},
    {L"フレーム", INKPOD_LAYER_FRAME},
    {L"消失点", INKPOD_LAYER_VANISHING_POINT},
    {L"調整", INKPOD_LAYER_ADJUSTMENT},
    {L"テキスト", INKPOD_LAYER_TEXT},
    {L"指示", INKPOD_LAYER_ANNOTATION},
    {L"ベクター彩色", INKPOD_LAYER_VECTOR_COLORING},
}};

bool QuerySnapshotTransform(
    AppContext& state, InkpodSnapshotTransform& transform) noexcept;
bool QueryDocument(AppContext& state, InkpodDocumentInfo& info) noexcept;
std::wstring LocalizedHistoryLabel(const std::string& label);
using inkpod::windows::ui::panes::DocumentPanesController;
using inkpod::windows::ui::panes::ColorPanesController;
using inkpod::windows::ui::panes::LightTablePaneItem;
using inkpod::windows::ui::panes::LightTablePaneSet;
using inkpod::windows::ui::panes::SequencePaneCell;
using inkpod::windows::ui::panes::TreePaneNode;
using inkpod::windows::ui::tools::FillController;
using inkpod::windows::ui::tools::FloatingPasteController;
using inkpod::windows::ui::tools::SelectionController;
using inkpod::windows::ui::tools::ViewController;
using inkpod::windows::ui::tools::VectorController;
using inkpod::windows::ui::BatchController;
using inkpod::windows::ui::EffectsController;
bool QueryTreeNode(AppContext& state, bool plane, TreePaneNode& output) noexcept;
bool RefreshTreePane(AppContext& state) noexcept;
void SetDrawingColor(AppContext& state, InkpodColorValue color) noexcept;
void RefreshDockPaneViews(AppContext& state) noexcept;
InkpodStatus ApplyTreeEdit(
    AppContext& state,
    InkpodTreeOperation operation,
    std::uint64_t object_id,
    std::uint32_t destination_index,
    std::uint64_t& out_object_id) noexcept;

bool DispatchEnabledCommand(
    AppContext& state, HWND window, UINT command) noexcept {
    if (!IsCommandEnabled(state.command_states, command)) {
        return false;
    }
    SendMessageW(window, WM_COMMAND, command, 0);
    return true;
}

void DispatchBatchPaletteCommand(void* context, UINT command) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state != nullptr && state->windows.window != nullptr) {
        DispatchEnabledCommand(*state, state->windows.window, command);
    }
}

void SelectBatchPaletteOperation(
    void* context, std::uint32_t selected_index) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state != nullptr && !state->batch.loaded_graph) {
        state->batch.selected_operation = selected_index;
    }
}

void HandleActiveTreePlaneTransition(AppContext& state) noexcept {
    TreePaneNode active_plane{};
    HandleActivePlaneTransition(
        state.tools,
        state.windows.canvas,
        QueryTreeNode(state, true, active_plane)
            && IsVectorStrokePlane(active_plane.kind));
}

void ResetDocumentShellTransientState(DocumentShellState& document) noexcept {
    document.smoke_layer_id = 0U;
    document.selection_layer_id = 0U;
}

void ResetPaneDocumentState(PaneUiState& panes) noexcept {
    panes.active_tree_layer_id = 0U;
    panes.active_tree_plane_id = 0U;
    panes.active_tree_layer_index = 0U;
    panes.active_tree_plane_index = 0U;
    panes.active_light_table_set_id = 0U;
    panes.active_light_table_item_id = 0U;
    panes.active_light_table_set_index = 0U;
    panes.active_light_table_item_index = 0U;
    panes.light_table_move_samples.clear();
}

void ResetToolDocumentState(ToolUiState& tools) noexcept {
    tools.floating_active = false;
    tools.floating_transform = InkpodFloatingTransform{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};
    tools.floating_bounds = {};
    tools.floating_gesture_samples.clear();
    tools.floating_drag_mode = 0U;
    tools.fill_gesture_samples.clear();
    tools.selection_shape = INKPOD_SELECTION_RECTANGLE;
    tools.selection_operation = INKPOD_SELECTION_NEW;
    tools.selection_gesture_samples.clear();
    tools.vector_gesture_samples.clear();
    tools.vector_selected_path_ids.clear();
}

void ResetViewDocumentState(ViewUiState& view) noexcept {
    view.secondary_view_id = 0U;
    view.active_view_id = 0U;
    view.flip_horizontal = false;
    view.flip_vertical = false;
    view.ruler_visible = false;
    view.guides_visible = true;
    view.grid_visible = false;
    view.snap_guides = false;
    view.snap_grid = false;
    view.transparent_visible = true;
    ++view.locator_generation;
    view.locator_valid = false;
    view.locator = {};
    view.gesture_samples.clear();
    view.guide_drag_active = false;
    view.guide_drag_axis = 0U;
    view.guide_drag_id = 0U;
}

void ResetAnimationDocumentState(AnimationUiState& animation) noexcept {
    animation.active_sequence_index = 0U;
    animation.active_sequence_name.clear();
    animation.motion_active = false;
    animation.motion_paused = false;
}

void DispatchToolPaletteCommand(void* context, UINT command) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state != nullptr && state->windows.window != nullptr) {
        DispatchEnabledCommand(*state, state->windows.window, command);
    }
}

void ChangeToolOptionsDiameter(void* context, float diameter) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr || !std::isfinite(diameter)
        || diameter < inkpod::windows::ui::panes::kMinimumToolDiameter
        || diameter > inkpod::windows::ui::panes::kMaximumToolDiameter) {
        return;
    }
    state->tools.diameter = diameter;
    inkpod::windows::ui::panes::UpdateToolOptionsPane(
        state->windows.tool_options,
        state->tools.active_tool,
        state->tools.active_plane,
        state->tools.diameter);
    UpdateMenuState(*state);
}

void ChangeDockDrawingColor(
    void* context, const InkpodColorValue& color) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr) {
        return;
    }
    SetDrawingColor(*state, color);
    UpdateMenuState(*state);
}

void SelectDockColor(
    void* context, std::uint32_t index, bool chart) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr || index >= state->panes.palette_colors.size()) {
        return;
    }
    state->panes.selected_palette_index = index;
    if (chart) {
        state->panes.selected_color_chart_index = index;
        state->panes.color_chart_page = index / 20U;
    } else {
        state->panes.palette_group = index / 10U;
    }
    SetDrawingColor(*state, state->panes.palette_colors[index]);
    UpdateMenuState(*state);
}

void ChangeDockPaletteGroup(void* context, int delta) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr) {
        return;
    }
    const std::uint32_t groups = std::max<std::uint32_t>(
        1U,
        static_cast<std::uint32_t>(
            (state->panes.palette_colors.size() + 9U) / 10U));
    const int current = static_cast<int>(state->panes.palette_group % groups);
    state->panes.palette_group = static_cast<std::uint32_t>(
        (current + delta % static_cast<int>(groups) + static_cast<int>(groups))
        % static_cast<int>(groups));
    if (!state->panes.palette_colors.empty()) {
        state->panes.selected_palette_index = std::min<std::uint32_t>(
            state->panes.palette_group * 10U,
            static_cast<std::uint32_t>(
                state->panes.palette_colors.size() - 1U));
    }
    RefreshDockPaneViews(*state);
    UpdateMenuState(*state);
}

void NotifyToolPaletteVisibilityChanged(void* context) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state != nullptr && state->windows.window != nullptr) {
        UpdateMenuState(*state);
    }
}

void DispatchLayerPaletteCommand(void* context, UINT command) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state != nullptr && state->windows.window != nullptr) {
        DispatchEnabledCommand(*state, state->windows.window, command);
    }
}

void SelectLayerPaletteLayer(void* context, std::uint64_t layer_id) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr || state->engine == nullptr || layer_id == 0U) {
        return;
    }
    const std::uint64_t previous_layer_id = state->panes.active_tree_layer_id;
    const std::uint64_t previous_plane_id = state->panes.active_tree_plane_id;
    state->panes.active_tree_layer_id = layer_id;
    state->panes.active_tree_plane_id = 0U;
    if (!RefreshTreePane(*state) || state->panes.active_tree_plane_id == 0U) {
        state->panes.active_tree_layer_id = previous_layer_id;
        state->panes.active_tree_plane_id = previous_plane_id;
        RefreshTreePane(*state);
        return;
    }
    const std::uint64_t plane_id = state->panes.active_tree_plane_id;
    const InkpodStatus status = state->engine->Invoke(
        [layer_id, plane_id](InkpodCore* core) {
            return inkpod_core_set_active_node(core, layer_id, plane_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        state->panes.active_tree_layer_id = previous_layer_id;
        state->panes.active_tree_plane_id = previous_plane_id;
        RefreshTreePane(*state);
        ShowCoreError(*state, state->windows.window, L"レイヤーの選択");
    } else {
        TreePaneNode active_plane{};
        if (QueryTreeNode(*state, true, active_plane)) {
            state->tools.active_plane =
                active_plane.kind == INKPOD_TYPED_PLANE_MAIN_LINE
                ? INKPOD_PLANE_MAIN_LINE
                : INKPOD_PLANE_COLOR;
        }
    }
    UpdateMenuState(*state);
}

void SelectLayerPalettePlane(void* context, std::uint64_t plane_id) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr || state->engine == nullptr || plane_id == 0U
        || state->panes.active_tree_layer_id == 0U) {
        return;
    }
    const std::uint64_t previous_plane_id = state->panes.active_tree_plane_id;
    const std::uint64_t layer_id = state->panes.active_tree_layer_id;
    state->panes.active_tree_plane_id = plane_id;
    const InkpodStatus status = state->engine->Invoke(
        [layer_id, plane_id](InkpodCore* core) {
            return inkpod_core_set_active_node(core, layer_id, plane_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        state->panes.active_tree_plane_id = previous_plane_id;
        RefreshTreePane(*state);
        ShowCoreError(*state, state->windows.window, L"プレーンの選択");
    } else {
        RefreshTreePane(*state);
        TreePaneNode active_plane{};
        if (QueryTreeNode(*state, true, active_plane)) {
            state->tools.active_plane =
                active_plane.kind == INKPOD_TYPED_PLANE_MAIN_LINE
                ? INKPOD_PLANE_MAIN_LINE
                : INKPOD_PLANE_COLOR;
        }
    }
    UpdateMenuState(*state);
}

void ReorderLayerPaletteLayer(
    void* context,
    std::uint64_t layer_id,
    std::uint32_t destination_index) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr || layer_id == 0U) {
        return;
    }
    std::uint64_t ignored{};
    const InkpodStatus status = ApplyTreeEdit(
        *state,
        INKPOD_TREE_REORDER_LAYER,
        layer_id,
        destination_index,
        ignored);
    if (status != INKPOD_STATUS_OK) {
        ShowCoreError(*state, state->windows.window, L"レイヤーの並べ替え");
    } else {
        state->panes.active_tree_layer_id = layer_id;
        RefreshTreePane(*state);
    }
    UpdateMenuState(*state);
}

void ReorderLayerPalettePlane(
    void* context,
    std::uint64_t plane_id,
    std::uint32_t destination_index) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr || plane_id == 0U) {
        return;
    }
    std::uint64_t ignored{};
    const InkpodStatus status = ApplyTreeEdit(
        *state,
        INKPOD_TREE_REORDER_PLANE,
        plane_id,
        destination_index,
        ignored);
    if (status != INKPOD_STATUS_OK) {
        ShowCoreError(*state, state->windows.window, L"プレーンの並べ替え");
    } else {
        state->panes.active_tree_plane_id = plane_id;
        RefreshTreePane(*state);
    }
    UpdateMenuState(*state);
}

void ChangeLayerPaletteSplit(void* context, std::uint32_t split_milli) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state == nullptr) {
        return;
    }
    state->windows.workspace.layer_split_milli =
        std::clamp<std::uint32_t>(split_milli, 200U, 800U);
}

void NotifyLayerPaletteVisibilityChanged(void* context) noexcept {
    auto* state = static_cast<AppContext*>(context);
    if (state != nullptr && state->windows.window != nullptr) {
        UpdateMenuState(*state);
    }
}

void ResetEffectsDocumentState(EffectsUiState& effects) noexcept {
    effects.adjustment_id = 0U;
    effects.adjustment_visible = true;
    effects.adjustments.clear();
    effects.alpha_view = false;
    effects.stamp_source_valid = false;
    effects.samples.clear();
    effects.airbrush_active = false;
}

void ResetUiForDocumentReplacement(AppContext& state) noexcept {
    CancelSelectionGeometryPreview(state.tools, state.windows.canvas);
    ResetDocumentShellTransientState(state.document);
    ResetPaneDocumentState(state.panes);
    ResetToolDocumentState(state.tools);
    ResetViewDocumentState(state.view);
    ResetAnimationDocumentState(state.animation);
    ResetEffectsDocumentState(state.effects);
    if (state.windows.document_tabs != nullptr) {
        TabCtrl_DeleteAllItems(state.windows.document_tabs);
        TCITEMW item{};
        item.mask = TCIF_TEXT | TCIF_PARAM;
        item.pszText = const_cast<wchar_t*>(L"無題セル 1");
        item.lParam = 0;
        TabCtrl_InsertItem(state.windows.document_tabs, 0, &item);
        TabCtrl_SetCurSel(state.windows.document_tabs, 0);
    }
    if (state.engine != nullptr) {
        state.engine->SetActiveView(0U);
    }
    if (state.windows.window != nullptr) {
        KillTimer(state.windows.window, kContinuousSprayTimer);
        KillTimer(state.windows.window, kMotionPlaybackTimer);
        KillTimer(state.windows.window, kShortcutSequenceTimer);
    }
    HandleActiveTreePlaneTransition(state);
}

bool RefreshTreePane(AppContext& state) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    std::vector<TreePaneNode> layers;
    std::vector<TreePaneNode> planes;
    const std::uint64_t requested_layer_id = state.panes.active_tree_layer_id;
    std::uint32_t selected_layer_index{};
    DocumentPanesController controller(*state.engine);
    const InkpodStatus status = controller.LoadTree(
        requested_layer_id,
        state.windows.workspace.layer_visible,
        layers,
        planes,
        selected_layer_index);
    if (status != INKPOD_STATUS_OK || layers.empty()) {
        state.panes.tree_layer_count = 0U;
        state.panes.tree_plane_count = 0U;
        layers.clear();
        planes.clear();
        UpdateLayerPaletteDialog(
            state.panes.layer_palette,
            layers,
            planes,
            0U,
            0U,
            state.windows.workspace.layer_split_milli);
        return false;
    }
    selected_layer_index = std::min<std::uint32_t>(
        selected_layer_index, static_cast<std::uint32_t>(layers.size() - 1U));
    state.panes.active_tree_layer_index = selected_layer_index;
    state.panes.active_tree_layer_id = layers[selected_layer_index].id;
    state.panes.tree_layer_count = static_cast<std::uint32_t>(layers.size());

    std::uint32_t selected_plane_index{};
    for (std::size_t index = 0U; index < planes.size(); ++index) {
        if (planes[index].id == state.panes.active_tree_plane_id) {
            selected_plane_index = static_cast<std::uint32_t>(index);
        }
    }
    state.panes.tree_plane_count = static_cast<std::uint32_t>(planes.size());
    bool vector_stroke_plane{};
    if (!planes.empty()) {
        selected_plane_index = std::min<std::uint32_t>(
            selected_plane_index, static_cast<std::uint32_t>(planes.size() - 1U));
        state.panes.active_tree_plane_index = selected_plane_index;
        state.panes.active_tree_plane_id = planes[selected_plane_index].id;
        vector_stroke_plane = IsVectorStrokePlane(planes[selected_plane_index].kind);
    } else {
        state.panes.active_tree_plane_index = 0U;
        state.panes.active_tree_plane_id = 0U;
    }
    HandleActivePlaneTransition(
        state.tools, state.windows.canvas, vector_stroke_plane);
    UpdateLayerPaletteDialog(
        state.panes.layer_palette,
        layers,
        planes,
        state.panes.active_tree_layer_id,
        state.panes.active_tree_plane_id,
        state.windows.workspace.layer_split_milli);
    return true;
}

bool RefreshLightTablePane(AppContext& state) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    std::vector<LightTablePaneSet> sets;
    std::vector<LightTablePaneItem> items;
    DocumentPanesController controller(*state.engine);
    const InkpodStatus status = controller.LoadLightTable(sets, items);
    if (status != INKPOD_STATUS_OK || sets.empty()) {
        state.panes.light_table_set_count = 0U;
        state.panes.light_table_item_count = 0U;
        return false;
    }
    std::uint32_t selected_set{};
    for (std::size_t index = 0; index < sets.size(); ++index) {
        const auto& set = sets[index];
        if (set.id == state.panes.active_light_table_set_id
            || (set.flags & INKPOD_LIGHT_TABLE_SET_ACTIVE) != 0U) {
            selected_set = static_cast<std::uint32_t>(index);
        }
    }
    state.panes.active_light_table_set_index = selected_set;
    state.panes.active_light_table_set_id = sets[selected_set].id;
    state.panes.light_table_set_count = static_cast<std::uint32_t>(sets.size());

    std::uint32_t selected_item{};
    for (std::size_t index = 0; index < items.size(); ++index) {
        const auto& item = items[index];
        if (item.info.id == state.panes.active_light_table_item_id) {
            selected_item = static_cast<std::uint32_t>(index);
        }
    }
    state.panes.light_table_item_count = static_cast<std::uint32_t>(items.size());
    if (!items.empty()) {
        state.panes.active_light_table_item_index = selected_item;
        state.panes.active_light_table_item_id = items[selected_item].info.id;
    } else {
        state.panes.active_light_table_item_index = 0U;
        state.panes.active_light_table_item_id = 0U;
    }
    return true;
}

bool RefreshSequencePane(AppContext& state) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    std::vector<SequencePaneCell> cells;
    DocumentPanesController controller(*state.engine);
    const InkpodStatus status = controller.LoadSequence(cells);
    if (status != INKPOD_STATUS_OK) {
        state.panes.sequence_count = 0U;
        state.animation.active_sequence_name.clear();
        return false;
    }
    state.panes.sequence_count = static_cast<std::uint32_t>(cells.size());
    if (cells.empty()) {
        state.animation.active_sequence_index = 0U;
        state.animation.active_sequence_name.clear();
        return true;
    }
    InkpodDocumentInfo document{};
    QueryDocument(state, document);
    std::uint32_t selected = std::min<std::uint32_t>(
        state.animation.active_sequence_index, static_cast<std::uint32_t>(cells.size() - 1U));
    bool has_active_cell{};
    for (std::size_t index = 0; index < cells.size(); ++index) {
        const auto& cell = cells[index];
        const bool active = cell.info.document_uuid_high == document.document_uuid_high
            && cell.info.document_uuid_low == document.document_uuid_low;
        if (active) {
            selected = static_cast<std::uint32_t>(index);
            has_active_cell = true;
        }
    }
    state.animation.active_sequence_index = selected;
    state.animation.active_sequence_name.clear();
    if (has_active_cell) {
        const std::string& name = cells[selected].name;
        if (!name.empty() && name.size() <= static_cast<std::size_t>(INT_MAX)) {
            const int required = MultiByteToWideChar(
                CP_UTF8,
                MB_ERR_INVALID_CHARS,
                name.data(),
                static_cast<int>(name.size()),
                nullptr,
                0);
            if (required > 0) {
                try {
                    state.animation.active_sequence_name.resize(
                        static_cast<std::size_t>(required));
                    if (MultiByteToWideChar(
                            CP_UTF8,
                            MB_ERR_INVALID_CHARS,
                            name.data(),
                            static_cast<int>(name.size()),
                            state.animation.active_sequence_name.data(),
                            required) != required) {
                        state.animation.active_sequence_name.clear();
                    }
                } catch (const std::bad_alloc&) {
                    state.animation.active_sequence_name.clear();
                }
            }
        }
    }
    return true;
}

struct LocatorAsyncResult {
    std::uint64_t generation{};
    InkpodStatus status{INKPOD_STATUS_INVALID_STATE};
    InkpodLocatorOutput output{};
};

void QueueLocatorSample(AppContext& state) noexcept {
    if (state.engine == nullptr || state.view.locator_pending) {
        return;
    }
    std::shared_ptr<LocatorAsyncResult> result;
    try {
        result = std::make_shared<LocatorAsyncResult>();
    } catch (const std::bad_alloc&) {
        return;
    }
    result->generation = state.view.locator_generation;
    result->output.struct_size = sizeof(InkpodLocatorOutput);
    const std::uint64_t view_id = state.view.active_view_id;
    const double device_x = static_cast<double>(state.view.pointer_device_x);
    const double device_y = static_cast<double>(state.view.pointer_device_y);
    const HWND window = state.windows.window;
    state.view.locator_pending = true;
    if (!state.engine->Enqueue(
            [result, view_id, device_x, device_y](InkpodCore* core) {
                return inkpod_core_locator_sample(
                    core, view_id, device_x, device_y, &result->output);
            },
            false,
            false,
            false,
            [result, window](InkpodStatus status) {
                result->status = status;
                auto* delivery = new (std::nothrow) LocatorAsyncResult(*result);
                if (delivery == nullptr
                    || PostMessageW(
                           window,
                           kLocatorSampleReady,
                           0,
                           reinterpret_cast<LPARAM>(delivery)) == FALSE) {
                    delete delivery;
                }
            })) {
        state.view.locator_pending = false;
    }
}

bool RefreshColorPanes(AppContext& state) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    ColorPanesController controller(*state.engine);
    return controller.RefreshModel(state.panes) == INKPOD_STATUS_OK;
}

void RefreshDockPaneViews(AppContext& state) noexcept {
    inkpod::windows::ui::panes::UpdateToolOptionsPane(
        state.windows.tool_options,
        state.tools.active_tool,
        state.tools.active_plane,
        state.tools.diameter);
    inkpod::windows::ui::panes::UpdateColorDockPane(
        state.windows.color_pane,
        state.tools.drawing_color,
        state.panes.palette_colors,
        state.panes.color_chart_names,
        state.panes.palette_group,
        state.panes.color_chart_page,
        state.panes.color_chart_locked);
}

InkpodStatus ReplacePalette(
    AppContext& state, const std::vector<InkpodColorValue>& colors) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ColorPanesController controller(*state.engine);
    return controller.ReplacePalette(colors);
}

void UpdateMotionState(
    AnimationUiState& animation, const InkpodMotionFrame& frame) noexcept {
    animation.motion_paused = (frame.flags & INKPOD_MOTION_FRAME_PAUSED) != 0U;
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
    AppContext& app, bool forward, HistoryDialogState& dialog) noexcept {
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
    dialog.selected_index = app.lifetime.smoke_test
        ? (forward ? dialog.labels.size() - 1U : 0U)
        : (forward ? 0U : dialog.labels.size() - 1U);
    dialog.close_immediately = app.lifetime.smoke_test;
    return true;
}

bool QueryHistoryMenuLabels(
    AppContext& app,
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

void RefreshBatchPalette(BatchUiState& batch) noexcept {
    BatchController::RefreshPalette(batch);
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
    return inkpod::windows::ui::ShortcutMenuCommand(command_id);
}

bool ResolveConfiguredShortcut(
    AppContext& state,
    std::uint32_t virtual_key,
    std::uint32_t modifiers,
    UINT& menu_command) noexcept {
    ClearPendingShortcut(state.shortcuts);
    UINT command{};
    const InkpodShortcutMatch match = ResolveShortcutStroke(
        state.shortcuts, InkpodShortcutStroke{virtual_key, modifiers}, command);
    menu_command = ShortcutMenuCommand(command);
    return match == INKPOD_SHORTCUT_MATCH_EXACT && menu_command != 0U;
}

void ShowCoreError(const AppContext& state, HWND owner, const wchar_t* operation) noexcept {
    if (state.lifetime.smoke_test) {
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

void ShowShortcutError(
    const AppContext& state,
    HWND owner,
    const wchar_t* operation,
    InkpodStatus status) noexcept {
    if (state.lifetime.smoke_test) {
        return;
    }
    if (status == INKPOD_STATUS_INVALID_ARGUMENT) {
        MessageBoxW(
            owner,
            L"その割り当ては別のショートカットと前方一致します。\n"
            L"両方を一意に識別できるキー列を指定してください。",
            L"inkpod",
            MB_OK | MB_ICONWARNING);
        return;
    }
    if (status == INKPOD_STATUS_IO_ERROR) {
        MessageBoxW(
            owner,
            L"割り当ては反映されましたが、次回起動用の設定を保存できませんでした。",
            L"inkpod",
            MB_OK | MB_ICONWARNING);
        return;
    }
    ShowCoreError(state, owner, operation);
}

void UpdateFloatingPreview(AppContext& state) noexcept {
    if (state.windows.canvas == nullptr) {
        return;
    }
    inkpod::renderer::CanvasFloatingPreview preview{};
    preview.struct_size = sizeof(preview);
    preview.active = state.tools.floating_active ? 1U : 0U;
    preview.bounds = state.tools.floating_bounds;
    preview.transform = state.tools.floating_transform;
    SendMessageW(
        state.windows.canvas,
        inkpod::renderer::kCanvasSetFloatingPreview,
        0,
        reinterpret_cast<LPARAM>(&preview));
}

InkpodStatus BeginFloatingPaste(AppContext& state, std::uint32_t mode) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    bool imported_standard{};
    if (state.document.clipboard == nullptr) {
        if (!ImportStandardClipboard(state.windows.window, state.document.clipboard)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        imported_standard = true;
    }
    const InkpodClipboard* clipboard = state.document.clipboard;
    FloatingPasteController controller(*state.engine);
    InkpodStatus status = controller.Begin(clipboard, mode);
    if (status != INKPOD_STATUS_OK && imported_standard
        && mode == INKPOD_PASTE_COMPATIBLE) {
        status = controller.Begin(clipboard, INKPOD_PASTE_ACTIVE_CONVERTED);
    }
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    InkpodClipboardRasterBuffer view{};
    view.struct_size = sizeof(view);
    if (inkpod_clipboard_render_rgba8(clipboard, &view) != INKPOD_STATUS_OK
        || view.width > static_cast<std::uint32_t>(INT_MAX)
        || view.height > static_cast<std::uint32_t>(INT_MAX)) {
        controller.Finish(false);
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.tools.floating_active = true;
    state.tools.floating_bounds = {
        view.origin_x,
        view.origin_y,
        static_cast<std::int32_t>(view.width),
        static_cast<std::int32_t>(view.height)};
    state.tools.floating_transform = InkpodFloatingTransform{
        sizeof(InkpodFloatingTransform), 0U, 0.0, 0.0, 1.0, 1.0, 0.0};
    TransitionActiveTool(
        state.tools, state.windows.canvas, kInteractionFloatingTransform);
    UpdateFloatingPreview(state);
    return INKPOD_STATUS_OK;
}

InkpodStatus SetFloatingTransform(
    AppContext& state, const InkpodFloatingTransform& transform) noexcept {
    if (!state.tools.floating_active || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    FloatingPasteController controller(*state.engine);
    const InkpodStatus status = controller.Transform(transform);
    if (status == INKPOD_STATUS_OK) {
        state.tools.floating_transform = transform;
        UpdateFloatingPreview(state);
    }
    return status;
}

InkpodStatus ShowFloatingTransformDialog(AppContext& state) noexcept {
    if (!state.tools.floating_active) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState geometry{};
    geometry.title = L"フローティング選択の変形";
    geometry.labels = {L"X移動 (px)", L"Y移動 (px)", L"幅 (%)", L"高さ (%)"};
    geometry.values = {
        static_cast<std::int32_t>(std::lround(state.tools.floating_transform.translate_x)),
        static_cast<std::int32_t>(std::lround(state.tools.floating_transform.translate_y)),
        static_cast<std::int32_t>(std::lround(state.tools.floating_transform.scale_x * 100.0)),
        static_cast<std::int32_t>(std::lround(state.tools.floating_transform.scale_y * 100.0))};
    geometry.value_count = 4U;
    if (state.lifetime.smoke_test) {
        geometry.values = {2, 1, 125, 75};
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, geometry) != IDOK
        || geometry.values[2] <= 0 || geometry.values[3] <= 0) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState rotation{};
    rotation.title = L"回転・基準点";
    rotation.labels = {L"角度 (度)", L"縦横比固定 (0/1)", L"五点基準 (1-5)", nullptr};
    rotation.values = {
        static_cast<std::int32_t>(std::lround(state.tools.floating_transform.rotation_degrees)),
        0,
        INKPOD_RESIZE_ANCHOR_CENTER,
        0};
    rotation.value_count = 3U;
    if (state.lifetime.smoke_test) {
        rotation.values[0] = 15;
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, rotation) != IDOK) {
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
    AppContext& state,
    const InkpodStrokeSample& start,
    const InkpodStrokeSample& current,
    bool begin) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds canvas{};
    if (!state.tools.floating_active || !QueryDocument(state, info)
        || SendMessageW(
               state.windows.canvas,
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
        if (state.view.flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.view.flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return std::pair{x, y};
    };
    const auto to_device = [&](double x, double y) {
        if (state.view.flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.view.flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return std::pair{canvas.left + x * zoom, canvas.top + y * zoom};
    };
    const double center_x = static_cast<double>(state.tools.floating_bounds.x)
        + static_cast<double>(state.tools.floating_bounds.width - 1) / 2.0
        + state.tools.floating_transform.translate_x;
    const double center_y = static_cast<double>(state.tools.floating_bounds.y)
        + static_cast<double>(state.tools.floating_bounds.height - 1) / 2.0
        + state.tools.floating_transform.translate_y;
    if (begin) {
        state.tools.floating_drag_start = state.tools.floating_transform;
        state.tools.floating_drag_mode = 1U;
        const double radians = state.tools.floating_transform.rotation_degrees
            * 3.14159265358979323846 / 180.0;
        const double sine = std::sin(radians);
        const double cosine = std::cos(radians);
        const double half_width = static_cast<double>(state.tools.floating_bounds.width - 1)
            * state.tools.floating_transform.scale_x / 2.0;
        const double half_height = static_cast<double>(state.tools.floating_bounds.height - 1)
            * state.tools.floating_transform.scale_y / 2.0;
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
                state.tools.floating_drag_mode = 2U;
                break;
            }
        }
        const auto rotation_handle = to_device(
            center_x + half_height * sine,
            center_y - half_height * cosine - 20.0 / zoom);
        if (std::hypot(
                rotation_handle.first - static_cast<double>(start.x),
                rotation_handle.second - static_cast<double>(start.y)) <= 16.0) {
            state.tools.floating_drag_mode = 3U;
        }
        return INKPOD_STATUS_OK;
    }
    const auto start_document = to_document(start);
    const auto current_document = to_document(current);
    InkpodFloatingTransform transform = state.tools.floating_drag_start;
    if (state.tools.floating_drag_mode == 2U) {
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
    } else if (state.tools.floating_drag_mode == 3U) {
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

InkpodStatus EndFloatingPaste(AppContext& state, bool commit) noexcept {
    if (!state.tools.floating_active || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    FloatingPasteController controller(*state.engine);
    const InkpodStatus status = controller.Finish(commit);
    if (status == INKPOD_STATUS_OK || !commit) {
        state.tools.floating_active = false;
        state.tools.floating_bounds = {};
        state.tools.floating_gesture_samples.clear();
        UpdateFloatingPreview(state);
        TransitionActiveTool(
            state.tools, state.windows.canvas, INKPOD_TOOL_PENCIL);
    }
    return status;
}

InkpodStatus ResizeDocumentFromDialog(
    AppContext& state, const wchar_t* title, bool resolution_mode) noexcept {
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
    if (state.lifetime.smoke_test) {
        if (resolution_mode) {
            dimensions.values[2] = 120000;
            dimensions.values[3] = 120000;
        } else {
            dimensions.values[0] = static_cast<std::int32_t>(info.width + 2U);
            dimensions.values[1] = static_cast<std::int32_t>(info.height + 3U);
        }
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, dimensions) != IDOK
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
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, placement) != IDOK
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

InkpodStatus FitPaperToCaptureFrame(AppContext& state) noexcept {
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

InkpodDocumentInfo EmptyDocumentInfo() noexcept {
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    return info;
}

bool QueryDocument(AppContext& state, InkpodDocumentInfo& info) noexcept {
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

bool ParseGradientStops(
    const std::wstring& text,
    std::vector<GradientStopValue>& stops,
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
            stops.push_back(GradientStopValue{
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
            [](const GradientStopValue& left, const GradientStopValue& right) {
                return left.position_milli < right.position_milli;
            })
        && stops.front().position_milli == 0U && stops.back().position_milli == 1000U;
}

InkpodColorValue ColorFromRgba(std::uint32_t rgba) noexcept {
    return InkpodColorValue{
        sizeof(InkpodColorValue),
        INKPOD_COLOR_DEPTH_8,
        static_cast<std::uint16_t>((rgba >> 24U) & 0xffU),
        static_cast<std::uint16_t>((rgba >> 16U) & 0xffU),
        static_cast<std::uint16_t>((rgba >> 8U) & 0xffU),
        static_cast<std::uint16_t>(rgba & 0xffU)};
}

void SetDrawingColor(AppContext& state, InkpodColorValue color) noexcept {
    inkpod::windows::ui::tools::SetActiveCommandColor(state.tools, color);
    RefreshDockPaneViews(state);
}

InkpodStatus ShowDrawingColorEditor(AppContext& state) noexcept {
    ViewOptionsDialogState format{};
    format.title = L"描画色形式";
    format.labels = {L"深度 (8/16)", L"編集方式 (1:RGB 2:HSV)", L"Alpha表示 (1:数値 2:%)", nullptr};
    format.values = {
        state.tools.drawing_color.depth == INKPOD_COLOR_DEPTH_16 ? 16 : 8,
        1,
        1,
        0};
    format.value_count = 3U;
    if (state.lifetime.smoke_test) {
        format.values = {16, 2, 2, 0};
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, format) != IDOK
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
            state.tools.drawing_color.depth == INKPOD_COLOR_DEPTH_16
                ? state.tools.drawing_color.red
                : static_cast<std::int32_t>(state.tools.drawing_color.red),
            state.tools.drawing_color.green,
            state.tools.drawing_color.blue,
            format.values[2] == 2
                ? static_cast<std::int32_t>(
                      (static_cast<std::uint64_t>(state.tools.drawing_color.alpha) * 100U
                          + maximum / 2U)
                      / maximum)
                : state.tools.drawing_color.alpha};
        if (state.lifetime.smoke_test) {
            values.values = {65535, 32768, 0, 50};
        }
    } else {
        values.labels = {L"H (0-359)", L"S (0-1000)", L"V (0-1000)", format.values[2] == 2 ? L"Alpha (%)" : L"Alpha"};
        values.values = {30, 1000, 1000, format.values[2] == 2 ? 100 : static_cast<std::int32_t>(maximum)};
        if (state.lifetime.smoke_test) {
            values.values = {210, 750, 800, 50};
        }
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, values) != IDOK) {
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

InkpodFilterInput FilterInputFor(const FilterJob& job) noexcept {
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

AdjustmentLayerUiState* CurrentAdjustment(EffectsUiState& effects) noexcept {
    const auto found = std::find_if(
        effects.adjustments.begin(),
        effects.adjustments.end(),
        [&effects](const AdjustmentLayerUiState& adjustment) {
            return adjustment.id == effects.adjustment_id;
        });
    return found == effects.adjustments.end() ? nullptr : &*found;
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

InkpodStatus StartEffectTask(
    AppContext& state,
    bool preview_prompt,
    std::function<InkpodStatus(InkpodCore*, InkpodTask*)> operation) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    EffectsController controller(
        state.lifetime, state.windows, state.effects, *state.engine);
    return controller.StartTask(
        preview_prompt, std::move(operation), kEffectTaskCompleted);
}

std::uint32_t FilterKindForCommand(UINT command) noexcept {
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

bool ConfigureFilterEditor(
    AppContext& state, UINT command, FilterJob& job) noexcept {
    job.kind = FilterKindForCommand(command);
    if (job.kind == 0U) {
        return false;
    }
    EffectEditorState editor{};
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
    if (ShowEffectEditor(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, editor) != IDOK) {
        return false;
    }
    job.channel = editor.channel;
    job.interpolation = editor.mode;
    job.parameters = editor.parameters;
    job.preview = editor.option1;
    if (job.kind == INKPOD_FILTER_TONE_CURVE
        && !ParseCurvePoints(editor.points, job.points)) {
        if (!state.lifetime.smoke_test) {
            MessageBoxW(
                state.windows.window,
                L"トーンカーブ点は input:output;...（各0～65535）で2点以上指定してください。",
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

InkpodStatus QueueFilter(AppContext& state, FilterJob job) noexcept {
    return StartEffectTask(
        state,
        job.preview,
        [job = std::move(job)](InkpodCore* core, InkpodTask* task) {
            const InkpodFilterInput input = FilterInputFor(job);
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

bool ConfigureAdjustmentEditor(
    AppContext& state, FilterJob& job, bool update) noexcept {
    EffectEditorState editor{};
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
        const AdjustmentLayerUiState* current = CurrentAdjustment(state.effects);
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
    if (ShowEffectEditor(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, editor) != IDOK) {
        return false;
    }
    job.kind = editor.channel;
    job.channel = INKPOD_FILTER_CHANNEL_RGB;
    job.interpolation = editor.mode;
    job.parameters = editor.parameters;
    if (job.kind == INKPOD_FILTER_TONE_CURVE
        && !ParseCurvePoints(editor.points, job.points)) {
        if (!state.lifetime.smoke_test) {
            MessageBoxW(
                state.windows.window,
                L"トーンカーブ点は input:output;... 形式で2点以上指定してください。",
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

InkpodStatus CreateOrUpdateAdjustment(
    AppContext& state, FilterJob job, bool update) noexcept {
    if (state.engine == nullptr || (update && state.effects.adjustment_id == 0U)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t layer_id = state.effects.adjustment_id;
    std::shared_ptr<AdjustmentLayerUiState> pending;
    try {
        std::string name;
        if (update) {
            const AdjustmentLayerUiState* current = CurrentAdjustment(state.effects);
            if (current == nullptr) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            name = current->name;
        } else {
            name = "Adjustment " + std::to_string(state.effects.adjustments.size() + 1U);
            state.effects.adjustments.reserve(state.effects.adjustments.size() + 1U);
        }
        pending = std::make_shared<AdjustmentLayerUiState>(
            AdjustmentLayerUiState{layer_id, true, std::move(job), std::move(name)});
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t created_id{};
    const InkpodStatus status = state.engine->Invoke(
        [pending, update, layer_id, &created_id](InkpodCore* core) {
            const InkpodFilterInput input = FilterInputFor(pending->job);
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
        state.effects.adjustment_id = created_id;
        state.effects.adjustment_visible = true;
        state.effects.adjustments.push_back(std::move(*pending));
    } else if (status == INKPOD_STATUS_OK) {
        AdjustmentLayerUiState* current = CurrentAdjustment(state.effects);
        if (current != nullptr) {
            current->job = std::move(pending->job);
        }
    }
    return status;
}

InkpodStatus SetAdjustmentVisibility(AppContext& state, bool visible) noexcept {
    if (state.engine == nullptr || state.effects.adjustment_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t layer_id = state.effects.adjustment_id;
    AdjustmentLayerUiState* current = CurrentAdjustment(state.effects);
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

bool SelectAdjustment(AppContext& state, bool next) noexcept {
    if (state.effects.adjustments.empty()) {
        return false;
    }
    const auto current = std::find_if(
        state.effects.adjustments.begin(),
        state.effects.adjustments.end(),
        [&state](const AdjustmentLayerUiState& adjustment) {
            return adjustment.id == state.effects.adjustment_id;
        });
    std::size_t index = current == state.effects.adjustments.end()
        ? 0U
        : static_cast<std::size_t>(current - state.effects.adjustments.begin());
    if (next) {
        index = (index + 1U) % state.effects.adjustments.size();
    } else {
        index = (index + state.effects.adjustments.size() - 1U)
            % state.effects.adjustments.size();
    }
    state.effects.adjustment_id = state.effects.adjustments[index].id;
    state.effects.adjustment_visible = state.effects.adjustments[index].visible;
    return true;
}

std::vector<InkpodGradientStop> GradientStops(const std::vector<GradientStopValue>& values) {
    std::vector<InkpodGradientStop> stops;
    stops.reserve(values.size());
    for (const GradientStopValue& value : values) {
        stops.push_back(InkpodGradientStop{
            sizeof(InkpodGradientStop),
            0U,
            value.position_milli,
            0U,
            ColorFromRgba(value.rgba)});
    }
    return stops;
}

InkpodStatus QueueBoundaryAirbrush(AppContext& state, const CanvasEffectOptions& options) noexcept {
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodColorValue> colors;
    try {
        colors.reserve(options.stops.size());
        for (const GradientStopValue& stop : options.stops) {
            colors.push_back(ColorFromRgba(stop.rgba));
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

bool ConfigureCanvasEffect(AppContext& state, UINT command) noexcept {
    EffectEditorState editor{};
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
            interaction = command == IDM_EFFECT_GRADIENT ? kInteractionEffectGradient
                                                         : kInteractionEffectAlphaGradient;
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
            interaction = kInteractionEffectAirbrush;
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
            interaction = kInteractionEffectBlur;
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
            interaction = kInteractionEffectStamp;
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
            interaction = kInteractionEffectDust;
            break;
        default:
            return false;
    }
    if (ShowEffectEditor(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, editor) != IDOK) {
        return false;
    }
    CanvasEffectOptions options{};
    options.parameters = editor.parameters;
    options.shape = editor.channel;
    options.mode = editor.mode;
    options.option = editor.option1;
    options.option2 = editor.option2;
    if (command == IDM_EFFECT_GRADIENT || command == IDM_EFFECT_ALPHA_GRADIENT
        || command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        const std::size_t minimum_stops = command == IDM_EFFECT_BOUNDARY_AIRBRUSH ? 2U : 3U;
        if (!ParseGradientStops(editor.points, options.stops, minimum_stops)) {
            if (!state.lifetime.smoke_test) {
                MessageBoxW(
                    state.windows.window,
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
    state.effects.options = std::move(options);
    TransitionActiveTool(state.tools, state.windows.canvas, interaction);
    state.effects.samples.clear();
    if (command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        TransitionActiveTool(
            state.tools, state.windows.canvas, INKPOD_TOOL_BRUSH);
        return QueueBoundaryAirbrush(state, state.effects.options) == INKPOD_STATUS_OK;
    }
    return true;
}

InkpodStatus QueueGradientGesture(
    AppContext& state, std::vector<InkpodStrokeSample> samples, bool alpha_only) noexcept {
    InkpodDocumentInfo document{};
    if (samples.size() < 2U || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodGradientStop> stops;
    try {
        stops = GradientStops(state.effects.options.stops);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.options;
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

InkpodStatus QueueAirbrushGesture(
    AppContext& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (samples.empty() || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodColorValue color = ColorFromRgba(state.tools.color_rgba);
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

InkpodStatus QueueStampGesture(
    AppContext& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (!state.effects.stamp_source_valid || samples.empty() || !QueryDocument(state, document)
        || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStrokeSample source = state.effects.stamp_source;
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

InkpodStatus QueueBlurGesture(
    AppContext& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (samples.empty() || !QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.options;
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

InkpodStatus QueueDustRemoval(
    AppContext& state, std::vector<InkpodStrokeSample> samples) noexcept {
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = document.color_plane_id;
    const bool preview = options.option;
    return StartEffectTask(
        state,
        preview,
        [samples = std::move(samples), options, plane_id, preview](
            InkpodCore* core, InkpodTask* task) {
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

InkpodStatus FinishEffectGesture(AppContext& state) noexcept {
    std::vector<InkpodStrokeSample> samples;
    samples.swap(state.effects.samples);
    switch (state.tools.active_tool) {
        case kInteractionEffectGradient:
            return QueueGradientGesture(state, std::move(samples), false);
        case kInteractionEffectAlphaGradient:
            return QueueGradientGesture(state, std::move(samples), true);
        case kInteractionEffectAirbrush:
            return QueueAirbrushGesture(state, std::move(samples));
        case kInteractionEffectBlur:
            return QueueBlurGesture(state, std::move(samples));
        case kInteractionEffectStamp:
            return QueueStampGesture(state, std::move(samples));
        case kInteractionEffectDust:
            return QueueDustRemoval(state, std::move(samples));
        default:
            return INKPOD_STATUS_INVALID_STATE;
    }
}

CommandStateInputs BuildCommandStateInputs(
    AppContext& state,
    const InkpodDocumentInfo& info,
    bool has_document,
    const InkpodHistoryInfo& history,
    bool has_history) noexcept {
    CommandStateInputs inputs{};
    inputs.document.has_document = has_document;
    inputs.document.has_saved_path = !state.document.current_path.empty();
    inputs.document.dirty =
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U;

    inputs.edit.can_undo =
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) != 0U;
    inputs.edit.can_redo =
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_REDO) != 0U;
    inputs.edit.can_history_back = has_history && history.cursor > 0U;
    inputs.edit.can_history_forward =
        has_history && history.cursor < history.item_count;
    inputs.edit.clipboard_available = state.document.clipboard != nullptr
        || (InkpodClipboardFormat() != 0U
            && IsClipboardFormatAvailable(InkpodClipboardFormat()) != FALSE);
    inputs.edit.floating_active = state.tools.floating_active;

    inputs.effects.color_plane_active =
        state.tools.active_plane == INKPOD_PLANE_COLOR;
    inputs.effects.adjustment_available = state.effects.adjustment_id != 0U;
    inputs.effects.multiple_adjustments = state.effects.adjustments.size() > 1U;
    inputs.effects.adjustment_visible = state.effects.adjustment_visible;
    inputs.effects.alpha_view = state.effects.alpha_view;

    inputs.document_pane.removable_layer_available =
        state.panes.tree_layer_count > 1U;
    inputs.document_pane.layer_palette_visible =
        state.windows.workspace.layer_visible;

    inputs.animation.motion_fps = state.animation.motion_fps;

    inputs.selection_view.active_tool = state.tools.active_tool;
    inputs.selection_view.selection_shape = state.tools.selection_shape;
    inputs.selection_view.selection_operation = state.tools.selection_operation;
    inputs.selection_view.flip_horizontal = state.view.flip_horizontal;
    inputs.selection_view.flip_vertical = state.view.flip_vertical;
    inputs.selection_view.ruler_visible = state.view.ruler_visible;
    inputs.selection_view.guides_visible = state.view.guides_visible;
    inputs.selection_view.grid_visible = state.view.grid_visible;
    inputs.selection_view.snap_guides = state.view.snap_guides;
    inputs.selection_view.snap_grid = state.view.snap_grid;
    inputs.selection_view.transparent_visible = state.view.transparent_visible;
    inputs.selection_view.selection_layer_available =
        state.document.selection_layer_id != 0U;

    TreePaneNode active_plane{};
    inputs.tool.vector_stroke_plane = has_document
        && QueryTreeNode(state, true, active_plane)
        && IsVectorStrokePlane(active_plane.kind);
    inputs.tool.active_tool = state.tools.active_tool;
    inputs.tool.active_plane = state.tools.active_plane;
    inputs.tool.fill_operation = state.tools.fill_options.operation;
    inputs.tool.vector_erase_mode = state.tools.vector_erase_mode;
    inputs.tool.vector_selection_mode = state.tools.vector_selection_mode;
    inputs.tool.palette_visible = state.windows.workspace.tool_visible;

    inputs.color.eyedropper_source = state.tools.eyedropper_source;
    inputs.color.color_check_mode = state.view.color_check_mode;
    inputs.color.chart_locked = state.panes.color_chart_locked;

    inputs.batch.idle = state.batch.task == nullptr;
    inputs.batch.has_operations = state.batch.loaded_graph
        ? state.batch.graph != nullptr
        : !state.batch.operations.empty();
    inputs.batch.loaded_graph = state.batch.loaded_graph;
    inputs.batch.editable_item = inputs.batch.idle && !state.batch.loaded_graph
        && state.batch.selected_operation < state.batch.operations.size();
    inputs.batch.palette_visible = state.batch.palette != nullptr
        && IsWindowVisible(state.batch.palette) != FALSE;
    inputs.batch.output_policy = state.batch.output_policy;
    inputs.batch.failure_policy = state.batch.failure_policy;
    inputs.workspace.tool_visible = state.windows.workspace.tool_visible;
    inputs.workspace.tool_options_visible =
        state.windows.workspace.tool_options_visible;
    inputs.workspace.color_visible = state.windows.workspace.color_visible;
    inputs.workspace.layer_visible = state.windows.workspace.layer_visible;
    inputs.workspace.mirrored = state.windows.workspace.mirrored;
    return inputs;
}

void UpdateCommandLabels(
    HMENU menu,
    bool has_history,
    const std::wstring& undo_label,
    const std::wstring& redo_label) noexcept {
    if (!has_history) {
        return;
    }
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

void UpdateLocatorStatus(const AppContext& state) noexcept {
    if (state.windows.status_bar == nullptr) {
        return;
    }
    std::array<wchar_t, 128U> coordinate_text{};
    if (state.view.locator_valid) {
        _snwprintf_s(
            coordinate_text.data(),
            coordinate_text.size(),
            _TRUNCATE,
            L"X %d  Y %d",
            state.view.locator.document_x,
            state.view.locator.document_y);
    } else {
        wcscpy_s(coordinate_text.data(), coordinate_text.size(), L"X --  Y --");
    }
    std::array<wchar_t, 160U> sample_text{};
    if (state.view.locator_valid
        && (state.view.locator.flags & INKPOD_LOCATOR_COLOR_PRESENT) != 0U) {
        const auto& color = state.view.locator.color;
        _snwprintf_s(
            sample_text.data(),
            sample_text.size(),
            _TRUNCATE,
            (state.view.locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U
                ? L"RGBA %u,%u,%u,%u | W %d H %d"
                : L"RGBA %u,%u,%u,%u",
            color.red,
            color.green,
            color.blue,
            color.alpha,
            state.view.locator.selection.width,
            state.view.locator.selection.height);
    } else {
        wcscpy_s(sample_text.data(), sample_text.size(), L"RGBA --");
    }
    SendMessageW(
        state.windows.status_bar,
        SB_SETTEXTW,
        2,
        reinterpret_cast<LPARAM>(coordinate_text.data()));
    SendMessageW(
        state.windows.status_bar,
        SB_SETTEXTW,
        3,
        reinterpret_cast<LPARAM>(sample_text.data()));
}

bool FormatTaskProgressStatus(
    const AppContext& state,
    std::array<wchar_t, 192U>& text) noexcept {
    InkpodTaskInfo task_info{};
    task_info.struct_size = sizeof(task_info);
    const wchar_t* task_name = L"処理";
    bool running{};
    if (state.effects.task != nullptr
        && inkpod_task_query(state.effects.task, &task_info) == INKPOD_STATUS_OK
        && task_info.state == INKPOD_TASK_RUNNING) {
        running = true;
        task_name = L"画像処理";
    } else if (state.batch.task != nullptr
        && inkpod_batch_task_query(state.batch.task, &task_info) == INKPOD_STATUS_OK
        && task_info.state == INKPOD_TASK_RUNNING) {
        running = true;
        task_name = L"バッチ";
    }
    if (!running) {
        return false;
    }
    const std::uint64_t percent = task_info.total_work == 0U
        ? 0U
        : std::min<std::uint64_t>(
              100U,
              static_cast<std::uint64_t>(
                  static_cast<long double>(task_info.completed_work) * 100.0L
                  / static_cast<long double>(task_info.total_work)));
    _snwprintf_s(
        text.data(), text.size(), _TRUNCATE, L"%ls %llu%%", task_name, percent);
    return true;
}

const wchar_t* DocumentTabBaseName(
    const AppContext& state,
    const InkpodDocumentInfo& info,
    bool has_document) noexcept {
    if (!has_document) {
        return L"文書なし";
    }
    if (!state.animation.active_sequence_name.empty()) {
        return state.animation.active_sequence_name.c_str();
    }
    if (!state.document.current_path.empty()) {
        const wchar_t* leaf = state.document.current_path.c_str();
        for (const wchar_t* cursor = leaf; *cursor != L'\0'; ++cursor) {
            if (*cursor == L'\\' || *cursor == L'/') {
                leaf = cursor + 1;
            }
        }
        if (*leaf != L'\0') {
            return leaf;
        }
    }
    return (info.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U
        ? L"復元セル"
        : L"無題セル 1";
}

void UpdateDocumentTabLabels(
    AppContext& state,
    const InkpodDocumentInfo& info,
    bool has_document) noexcept {
    if (state.windows.document_tabs == nullptr) {
        return;
    }
    const int count = TabCtrl_GetItemCount(state.windows.document_tabs);
    const wchar_t* base_name = DocumentTabBaseName(state, info, has_document);
    const wchar_t* dirty = has_document
            && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
        ? L" *"
        : L"";
    for (int index = 0; index < count; ++index) {
        std::array<wchar_t, 1024U> label{};
        if (index == 0) {
            _snwprintf_s(
                label.data(), label.size(), _TRUNCATE, L"%ls%ls", base_name, dirty);
        } else {
            _snwprintf_s(
                label.data(),
                label.size(),
                _TRUNCATE,
                L"%ls [ビュー %d]%ls",
                base_name,
                index + 1,
                dirty);
        }
        TCITEMW item{};
        item.mask = TCIF_TEXT;
        item.pszText = label.data();
        TabCtrl_SetItem(state.windows.document_tabs, index, &item);
    }
}

void UpdateMainWindowStatus(
    AppContext& state,
    const InkpodDocumentInfo& info,
    bool has_document) noexcept {
    std::array<wchar_t, 1024> title{};
    const wchar_t* name = state.document.current_path.empty()
        ? ((info.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U ? L"Recovery" : L"無題")
        : state.document.current_path.c_str();
    _snwprintf_s(
        title.data(),
        title.size(),
        _TRUNCATE,
        L"%ls%ls - inkpod",
        name,
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U ? L" *" : L"");
    SetWindowTextW(state.windows.window, title.data());
    UpdateDocumentTabLabels(state, info, has_document);
    if (state.windows.status_bar == nullptr) {
        return;
    }

    InkpodSnapshotTransform transform{};
    const bool has_transform = has_document && QuerySnapshotTransform(state, transform);
    const auto tool_name = [&state]() noexcept -> const wchar_t* {
        switch (state.tools.active_tool) {
            case kInteractionBoxZoom: return L"範囲拡大";
            case kInteractionGuideMove: return L"ガイド移動";
            case kInteractionSelection: return L"選択";
            case kInteractionFill: return L"フィル";
            case kInteractionEyedropper: return L"スポイト";
            case kInteractionFloatingTransform: return L"変形";
            case kInteractionLightTableMove: return L"ライトテーブル移動";
            case kInteractionVectorLine: return L"直線";
            case kInteractionVectorCurve: return L"曲線";
            case kInteractionVectorRectangle: return L"長方形";
            case kInteractionVectorEllipse: return L"楕円";
            case kInteractionVectorPolyline: return L"折れ線";
            case kInteractionVectorEraser: return L"ベクター消しゴム";
            case kInteractionEffectGradient: return L"グラデーション";
            case kInteractionEffectAirbrush: return L"エアブラシ";
            case kInteractionEffectBlur: return L"ぼかし";
            case kInteractionEffectStamp: return L"スタンプ";
            case kInteractionEffectDust: return L"ゴミ取り";
            case kInteractionEffectAlphaGradient: return L"アルファ";
            case INKPOD_TOOL_ERASER: return L"消しゴム";
            case INKPOD_TOOL_BRUSH: return L"ブラシ";
            default: return L"鉛筆";
        }
    };
    std::array<wchar_t, 128U> tool_text{};
    _snwprintf_s(
        tool_text.data(),
        tool_text.size(),
        _TRUNCATE,
        L"%ls | %ls",
        tool_name(),
        state.tools.active_plane == INKPOD_PLANE_COLOR ? L"彩色" : L"主線");
    std::array<wchar_t, 160U> zoom_text{};
    if (has_transform) {
        _snwprintf_s(
            zoom_text.data(),
            zoom_text.size(),
            _TRUNCATE,
            L"ズーム: %.1f%%%ls%ls%ls",
            transform.zoom * 100.0,
            state.view.flip_horizontal ? L" | 左右" : L"",
            state.view.flip_vertical ? L" | 上下" : L"",
            state.view.grid_visible ? L" | グリッド" : L"");
    } else {
        wcscpy_s(zoom_text.data(), zoom_text.size(), L"ズーム: --");
    }
    std::array<wchar_t, 160U> document_text{};
    _snwprintf_s(
        document_text.data(),
        document_text.size(),
        _TRUNCATE,
        has_document ? L"%u×%u | %.1f dpi" : L"文書なし",
        has_document ? info.width : 0U,
        has_document ? info.height : 0U,
        has_document ? static_cast<double>(info.dpi_x_milli) / 1000.0 : 0.0);
    std::array<wchar_t, 192U> state_text{};
    const bool has_progress = FormatTaskProgressStatus(state, state_text);
    if (has_progress) {
        SetTimer(
            state.windows.window,
            kStatusProgressTimer,
            kStatusProgressTimerMilliseconds,
            nullptr);
    } else if (!state.shortcuts.pending_text.empty()) {
        KillTimer(state.windows.window, kStatusProgressTimer);
        wcsncpy_s(
            state_text.data(), state_text.size(), state.shortcuts.pending_text.c_str(), _TRUNCATE);
    } else {
        KillTimer(state.windows.window, kStatusProgressTimer);
        wcscpy_s(
            state_text.data(),
            state_text.size(),
            !has_document
                ? L"文書なし"
                : ((info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U ? L"変更あり" : L"保存済み"));
    }
    SendMessageW(
        state.windows.status_bar,
        SB_SETTEXTW,
        0,
        reinterpret_cast<LPARAM>(tool_text.data()));
    SendMessageW(
        state.windows.status_bar,
        SB_SETTEXTW,
        1,
        reinterpret_cast<LPARAM>(zoom_text.data()));
    SendMessageW(
        state.windows.status_bar,
        SB_SETTEXTW,
        4,
        reinterpret_cast<LPARAM>(document_text.data()));
    SendMessageW(
        state.windows.status_bar,
        SB_SETTEXTW,
        5,
        reinterpret_cast<LPARAM>(state_text.data()));
    UpdateLocatorStatus(state);
}

void UpdateMenuState(AppContext& state) noexcept {
    const HMENU menu = GetMenu(state.windows.window);
    if (menu == nullptr) {
        return;
    }

    InkpodDocumentInfo info{};
    const bool has_document = QueryDocument(state, info);
    InkpodHistoryInfo history{};
    std::wstring undo_label;
    std::wstring redo_label;
    const bool has_history = has_document
        && QueryHistoryMenuLabels(state, history, undo_label, redo_label);
    const CommandStateInputs inputs =
        BuildCommandStateInputs(state, info, has_document, history, has_history);

    // This is the only state computation. Every interactive surface below reads
    // the same immutable result; no tool or preview transition happens here.
    state.command_states = ComputeCommandStates(inputs);
    UpdateCommandLabels(menu, has_history, undo_label, redo_label);
    ApplyCommandStates(state.command_states, menu);
    UpdateToolPaletteDialog(state.tools.palette, state.command_states);
    UpdateLayerPaletteCommandState(
        state.panes.layer_palette,
        state.command_states);
    RefreshDockPaneViews(state);
    ApplyShortcutLabelsToMenu(menu, state.shortcuts.bindings);
    UpdateMainWindowStatus(state, info, has_document);
    DrawMenuBar(state.windows.window);
}

bool CommandSurfacesMatchComputedState(const AppContext& state) noexcept {
    const HMENU menu = GetMenu(state.windows.window);
    if (menu == nullptr) {
        return false;
    }
    for (const auto& command_state : state.command_states) {
        const UINT menu_state = GetMenuState(menu, command_state.command, MF_BYCOMMAND);
        if (menu_state == static_cast<UINT>(-1)
            || ((menu_state & (MF_DISABLED | MF_GRAYED)) == 0U)
                != command_state.enabled
            || ((menu_state & MF_CHECKED) != 0U) != command_state.checked) {
            return false;
        }
    }
    return ToolPaletteMatchesCommandState(
               state.tools.palette,
               state.command_states)
        && LayerPaletteMatchesCommandState(
               state.panes.layer_palette,
               state.command_states);
}

InkpodStatus ApplyView(
    AppContext& state,
    InkpodViewCommandKind kind,
    double value1,
    double value2,
    double value3 = 0.0,
    double value4 = 0.0) noexcept {
    const InkpodViewInput input{
        sizeof(InkpodViewInput), kind, 0U, value1, value2, value3, value4};
    const std::uint64_t view_id = state.view.active_view_id;
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewController controller(*state.engine);
    return controller.Apply(view_id, input);
}

bool QuerySnapshotTransform(
    AppContext& state, InkpodSnapshotTransform& transform) noexcept {
    transform = {};
    transform.struct_size = sizeof(transform);
    if (state.engine == nullptr) {
        return false;
    }
    const std::uint64_t view_id = state.view.active_view_id;
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

InkpodStatus ApplyZoomPercent(AppContext& state, std::uint32_t percent) noexcept {
    if (percent == 0U || percent > 6400U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodSnapshotTransform transform{};
    RECT client{};
    if (!QuerySnapshotTransform(state, transform)
        || GetClientRect(state.windows.canvas, &client) == FALSE
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
    AppContext& state,
    const InkpodStrokeSample& start,
    const InkpodStrokeSample& end) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.windows.canvas,
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
        if (state.view.flip_horizontal) {
            result = static_cast<double>(info.width) - result;
        }
        return std::clamp(result, 0.0, static_cast<double>(info.width));
    };
    auto document_y = [&](double value) {
        double result = (value - bounds.top) / zoom;
        if (state.view.flip_vertical) {
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
    AppContext& state, std::uint32_t axis, std::int32_t position) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t guide_id{};
    ViewController controller(*state.engine);
    return controller.AddGuide(axis, position, guide_id);
}

InkpodStatus DeleteAllGuides(AppContext& state) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewController controller(*state.engine);
    return controller.DeleteAllGuides();
}

InkpodStatus SetGrid(AppContext& state, const InkpodGridInput& input) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewController controller(*state.engine);
    return controller.SetGrid(input);
}

bool BeginGuideDrag(
    AppContext& state, const InkpodStrokeSample& sample) noexcept {
    constexpr float ruler_extent = 22.0F;
    if (state.tools.active_tool != kInteractionGuideMove && state.view.ruler_visible
        && sample.y >= 0.0F && sample.y <= ruler_extent
        && sample.x > ruler_extent) {
        state.view.guide_drag_active = true;
        state.view.guide_drag_axis = INKPOD_GUIDE_VERTICAL;
        state.view.guide_drag_id = 0U;
        return true;
    }
    if (state.tools.active_tool != kInteractionGuideMove && state.view.ruler_visible
        && sample.x >= 0.0F && sample.x <= ruler_extent
        && sample.y > ruler_extent) {
        state.view.guide_drag_active = true;
        state.view.guide_drag_axis = INKPOD_GUIDE_HORIZONTAL;
        state.view.guide_drag_id = 0U;
        return true;
    }
    if (state.tools.active_tool != kInteractionGuideMove || state.engine == nullptr) {
        return false;
    }
    std::uint64_t nearest_id{};
    std::uint32_t nearest_axis{};
    double nearest_distance = 7.0;
    const std::uint64_t view_id = state.view.active_view_id;
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
    state.view.guide_drag_active = true;
    state.view.guide_drag_axis = nearest_axis;
    state.view.guide_drag_id = nearest_id;
    return true;
}

InkpodStatus FinishGuideDrag(
    AppContext& state, const InkpodStrokeSample& sample) noexcept {
    if (!state.view.guide_drag_active) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint32_t axis = state.view.guide_drag_axis;
    const std::uint64_t guide_id = state.view.guide_drag_id;
    state.view.guide_drag_active = false;
    state.view.guide_drag_axis = 0U;
    state.view.guide_drag_id = 0U;
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    RECT client{};
    if (!QueryDocument(state, info)
        || GetClientRect(state.windows.canvas, &client) == FALSE
        || SendMessageW(
               state.windows.canvas,
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
    if ((axis == INKPOD_GUIDE_VERTICAL && state.view.flip_horizontal)
        || (axis == INKPOD_GUIDE_HORIZONTAL && state.view.flip_vertical)) {
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

InkpodStatus FitCanvas(AppContext& state, InkpodViewCommandKind kind) noexcept {
    RECT client{};
    GetClientRect(state.windows.canvas, &client);
    return ApplyView(
        state,
        kind,
        static_cast<double>(client.right - client.left),
        static_cast<double>(client.bottom - client.top));
}

InkpodStatus ApplyTreeEdit(
    AppContext& state,
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

bool QueryTreeNode(AppContext& state, bool plane, TreePaneNode& output) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    const std::uint32_t layer_index = state.panes.active_tree_layer_index;
    const std::uint32_t plane_index = plane ? state.panes.active_tree_plane_index : UINT32_MAX;
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
    AppContext& state, InkpodTreeEdit edit, const std::string& name, std::uint64_t& object_id) noexcept {
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
    AppContext& state, bool plane, UINT command) noexcept {
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
        dialog.values[0] = state.lifetime.smoke_test
            ? 75
            : static_cast<std::int32_t>(node.opacity_milli / 10U);
        if (ShowViewOptions(
                state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, dialog) != IDOK
            || dialog.values[0] < 0 || dialog.values[0] > 100) {
            return INKPOD_STATUS_CANCELLED;
        }
        edit.opacity_milli = static_cast<std::uint32_t>(dialog.values[0]) * 10U;
    }
    std::uint64_t ignored{};
    return ApplyTreeEditRecord(state, edit, node.name, ignored);
}

InkpodStatus EditSelectedTreeNodeProperties(AppContext& state, bool plane) noexcept {
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
    if (state.lifetime.smoke_test) {
        name_dialog.value = plane ? L"Smoke Plane" : L"Smoke Layer";
    }
    if (ShowTextInput(
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, name_dialog) != IDOK) {
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
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, options) != IDOK
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

bool VectorGestureDocumentPoints(
    AppContext& state,
    const std::vector<InkpodStrokeSample>& samples,
    std::vector<InkpodVectorPoint>& points) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (samples.empty() || !QueryDocument(state, info) || info.width == 0U || info.height == 0U
        || SendMessageW(
               state.windows.canvas,
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
            if (state.view.flip_horizontal) {
                x = static_cast<double>(info.width) - x;
            }
            if (state.view.flip_vertical) {
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
        if (state.view.flip_horizontal) {
            final_x = static_cast<double>(info.width) - final_x;
        }
        if (state.view.flip_vertical) {
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

void UpdateSelectionGeometryPreview(AppContext& state) noexcept {
    inkpod::renderer::CanvasGeometryPreview preview{};
    preview.struct_size = sizeof(preview);
    const auto publish_preview = [&state, &preview] {
        SendMessageW(
            state.windows.canvas,
            inkpod::renderer::kCanvasSetGeometryPreview,
            0,
            reinterpret_cast<LPARAM>(&preview));
    };
    std::vector<InkpodVectorPoint> points;
    if (!VectorGestureDocumentPoints(
            state, state.tools.selection_gesture_samples, points)
        || state.tools.selection_shape == INKPOD_SELECTION_WAND) {
        publish_preview();
        return;
    }
    preview.active = 1U;
    const auto append_point = [&preview](InkpodVectorPoint point) {
        if (preview.point_count < inkpod::renderer::kCanvasGeometryPreviewPoints) {
            preview.points[preview.point_count++] = point;
        }
    };
    if (state.tools.selection_shape == INKPOD_SELECTION_RECTANGLE) {
        if (points.size() < 2U) {
            preview.active = 0U;
            publish_preview();
            return;
        }
        const float left = std::floor(std::min(points.front().x, points.back().x));
        const float top = std::floor(std::min(points.front().y, points.back().y));
        const float right = std::ceil(std::max(points.front().x, points.back().x));
        const float bottom = std::ceil(std::max(points.front().y, points.back().y));
        if (left >= right || top >= bottom) {
            preview.active = 0U;
            publish_preview();
            return;
        }
        append_point(InkpodVectorPoint{left, top});
        append_point(InkpodVectorPoint{right, top});
        append_point(InkpodVectorPoint{right, bottom});
        append_point(InkpodVectorPoint{left, bottom});
        preview.closed = 1U;
    } else if (state.tools.selection_shape == INKPOD_SELECTION_ELLIPSE) {
        if (points.size() < 2U) {
            preview.active = 0U;
            publish_preview();
            return;
        }
        const float left = std::floor(std::min(points.front().x, points.back().x));
        const float top = std::floor(std::min(points.front().y, points.back().y));
        const float right = std::ceil(std::max(points.front().x, points.back().x));
        const float bottom = std::ceil(std::max(points.front().y, points.back().y));
        if (left >= right || top >= bottom) {
            preview.active = 0U;
            publish_preview();
            return;
        }
        const float radius_x = (right - left) / 2.0F;
        const float radius_y = (bottom - top) / 2.0F;
        const float center_x = (left + right) / 2.0F;
        const float center_y = (top + bottom) / 2.0F;
        constexpr std::uint32_t kEllipsePreviewPoints = 48U;
        constexpr double kTau = 6.28318530717958647692;
        for (std::uint32_t index = 0U; index < kEllipsePreviewPoints; ++index) {
            const double radians = kTau * static_cast<double>(index)
                / static_cast<double>(kEllipsePreviewPoints);
            append_point(InkpodVectorPoint{
                center_x + radius_x * static_cast<float>(std::cos(radians)),
                center_y + radius_y * static_cast<float>(std::sin(radians))});
        }
        preview.closed = 1U;
    } else {
        for (const InkpodVectorPoint point : points) {
            append_point(point);
        }
        if (state.tools.selection_shape == INKPOD_SELECTION_LASSO
            || state.tools.selection_shape == INKPOD_SELECTION_POLYLINE) {
            if (preview.point_count < 2U) {
                preview.active = 0U;
                publish_preview();
                return;
            }
            preview.closed = 1U;
        } else if (state.tools.selection_shape == INKPOD_SELECTION_TRACE) {
            preview.stroke_width = std::clamp(
                state.tools.selection_diameter, 0.001F, 4096.0F);
        }
    }
    if (preview.point_count == 0U) {
        preview.active = 0U;
    }
    publish_preview();
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
    const AppContext& state,
    const std::vector<InkpodVectorPoint>& points,
    std::vector<InkpodVectorCubicSegment>& segments,
    bool& closed) noexcept {
    if (points.size() < 2U) {
        return false;
    }
    const float width = std::clamp(state.tools.diameter, 0.001F, 4096.0F);
    closed = state.tools.active_tool == kInteractionVectorRectangle
        || state.tools.active_tool == kInteractionVectorEllipse;
    try {
        segments.clear();
        if (state.tools.active_tool == kInteractionVectorLine) {
            segments.push_back(VectorLineSegment(points.front(), points.back(), width));
        } else if (state.tools.active_tool == kInteractionVectorCurve) {
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
        } else if (state.tools.active_tool == kInteractionVectorRectangle) {
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
        } else if (state.tools.active_tool == kInteractionVectorEllipse) {
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
        } else if (state.tools.active_tool == kInteractionVectorPolyline) {
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

void UpdateVectorGeometryPreview(AppContext& state) noexcept {
    std::vector<InkpodVectorPoint> points;
    std::vector<InkpodVectorCubicSegment> segments;
    bool closed{};
    if (!VectorGestureDocumentPoints(state, state.tools.vector_gesture_samples, points)
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
        state.windows.canvas,
        inkpod::renderer::kCanvasSetGeometryPreview,
        0,
        reinterpret_cast<LPARAM>(&preview));
}

InkpodStatus FinishVectorCanvasGesture(AppContext& state) noexcept {
    TreePaneNode plane{};
    std::vector<InkpodVectorPoint> points;
    std::vector<InkpodVectorCubicSegment> segments;
    bool closed{};
    if (!QueryTreeNode(state, true, plane)) {
        CancelVectorGeometryPreview(state.tools, state.windows.canvas);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!IsVectorStrokePlane(plane.kind)) {
        if (state.engine != nullptr) {
            state.engine->SetLocalFailure(kVectorStrokePlaneRequired);
        }
        CancelVectorGeometryPreview(state.tools, state.windows.canvas);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!VectorGestureDocumentPoints(state, state.tools.vector_gesture_samples, points)) {
        if (state.engine != nullptr) {
            state.engine->SetLocalFailure(
                L"ベクター描画の入力点を文書座標へ変換できませんでした。");
        }
        CancelVectorGeometryPreview(state.tools, state.windows.canvas);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.tools.active_tool == kInteractionVectorEraser) {
        const InkpodVectorPoint point = points.back();
        const InkpodVectorEraseInput input{
            sizeof(InkpodVectorEraseInput),
            state.tools.vector_erase_mode,
            plane.id,
            point.x,
            point.y,
            std::max(0.5F, state.tools.diameter / 2.0F),
            0U};
        CancelVectorGeometryPreview(state.tools, state.windows.canvas);
        VectorController controller(*state.engine);
        return controller.Erase(input);
    }
    if (!BuildVectorGestureSegments(state, points, segments, closed)) {
        if (state.engine != nullptr) {
            state.engine->SetLocalFailure(
                L"ベクター描画を確定するための入力点が不足しています。");
        }
        CancelVectorGeometryPreview(state.tools, state.windows.canvas);
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodVectorPathInput input{
        sizeof(InkpodVectorPathInput),
        0U,
        closed ? INKPOD_VECTOR_PATH_CLOSED : 0U,
        plane.id,
        state.tools.drawing_color,
        segments.data(),
        segments.size(),
        sizeof(InkpodVectorCubicSegment)};
    VectorController controller(*state.engine);
    const InkpodStatus status = controller.AddPath(input);
    CancelVectorGeometryPreview(state.tools, state.windows.canvas);
    return status;
}

InkpodStatus SelectVectorObjects(AppContext& state) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodVectorSelectionMode mode = state.tools.vector_selection_mode;
    VectorController controller(*state.engine);
    return controller.Select(mode, state.tools.vector_selected_path_ids);
}

InkpodStatus ConnectSelectedVectorPlane(AppContext& state, float maximum_gap) noexcept {
    TreePaneNode plane{};
    if (!QueryTreeNode(state, true, plane) || maximum_gap <= 0.0F) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    VectorController controller(*state.engine);
    return controller.Connect(plane.id, maximum_gap);
}

InkpodStatus CorrectSelectedVectorWidth(
    AppContext& state, InkpodVectorWidthMode mode, float parameter) noexcept {
    if (state.tools.vector_selected_path_ids.empty()) {
        const InkpodStatus select_status = SelectVectorObjects(state);
        if (select_status != INKPOD_STATUS_OK) {
            return select_status;
        }
    }
    if (state.tools.vector_selected_path_ids.empty()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodVectorWidthInput input{
        sizeof(InkpodVectorWidthInput),
        mode,
        0U,
        state.tools.vector_selected_path_ids.data(),
        state.tools.vector_selected_path_ids.size(),
        parameter,
        0U};
    VectorController controller(*state.engine);
    return controller.CorrectWidth(input);
}

InkpodStatus AdjustSelection(
    AppContext& state, std::uint32_t operation, std::uint32_t pixels) noexcept {
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

InkpodStatus EditPaperFrames(AppContext& state, UINT command) noexcept {
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
        if (state.lifetime.smoke_test && info.width > 8U && info.height > 8U
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
        if (state.lifetime.smoke_test) {
            dialog.values = {1, 2, 3, 4};
        }
    } else {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, dialog) != IDOK) {
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
    AppContext& state,
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
    state.document.current_path.clear();
    state.document.recovery_path = std::move(private_recovery_path);
    state.view.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    state.tools.active_plane = INKPOD_PLANE_MAIN_LINE;
    ResetUiForDocumentReplacement(state);
    const InkpodPlaneKind plane = state.tools.active_plane;
    const InkpodStatus plane_status = state.engine->Invoke(
        [plane](InkpodCore* core) { return inkpod_core_set_active_plane(core, plane); },
        false,
        false);
    if (plane_status != INKPOD_STATUS_OK) {
        return plane_status;
    }
    return FitCanvas(state, INKPOD_VIEW_FIT);
}

InkpodStatus CreateDefaultCell(AppContext& state) noexcept {
    return CreateCell(state, 1920U, 1080U, 96000U);
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
    AppContext& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(state.document, *state.engine);
    const InkpodStatus status = shell.ImportCommonRaster(path);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.tools.active_plane = INKPOD_PLANE_COLOR;
    state.view.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetUiForDocumentReplacement(state);
    const InkpodStatus plane_status = state.engine->Invoke(
        [](InkpodCore* core) { return inkpod_core_set_active_plane(core, INKPOD_PLANE_COLOR); },
        false,
        false);
    return plane_status == INKPOD_STATUS_OK
        ? FitCanvas(state, INKPOD_VIEW_FIT)
        : plane_status;
}

InkpodStatus ExportCommonRasterToPath(
    AppContext& state, const std::wstring& path, bool composite_white) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(state.document, *state.engine);
    return shell.ExportCommonRaster(path, composite_white);
}

InkpodStatus ApplyLightTableEdit(
    AppContext& state,
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
    AppContext& state,
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
    AppContext& state, const std::wstring& path, bool reload) noexcept {
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
    const std::uint64_t item_id = state.panes.active_light_table_item_id;
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
        state.panes.active_light_table_item_id = added_item_id;
        state.panes.active_light_table_item_index = 0U;
    }
    return status;
}

InkpodStatus EditLightTableItemProperties(AppContext& state) noexcept {
    InkpodLightTableItemInfo info{};
    if (!QueryLightTableItem(state, state.panes.active_light_table_item_index, info)) {
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
    if (state.lifetime.smoke_test) {
        display.values = {50, INKPOD_LIGHT_TABLE_MONOTONE, 1, 5};
    }
    if (ShowViewOptions(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, display) != IDOK) {
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
    if (state.lifetime.smoke_test) {
        transform.values = {1000, -1000, 110, 90};
    }
    if (ShowViewOptions(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, transform) != IDOK
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
    edit.display_color = ColorFromRgba(state.tools.color_rgba);
    edit.translate_x_milli = transform.values[0];
    edit.translate_y_milli = transform.values[1];
    edit.scale_x_milli = static_cast<std::uint32_t>(transform.values[2]) * 10U;
    edit.scale_y_milli = static_cast<std::uint32_t>(transform.values[3]) * 10U;
    edit.rotation_milli_degrees = display.values[3] * 1000;
    std::uint64_t ignored{};
    return ApplyLightTableEdit(state, edit, {}, ignored);
}

InkpodStatus MoveLightTableFromCanvas(AppContext& state) noexcept {
    if (state.panes.light_table_move_samples.size() < 2U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodDocumentInfo document{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, document) || document.width == 0U
        || SendMessageW(
               state.windows.canvas,
               inkpod::renderer::kCanvasGetDocumentBounds,
               0,
               reinterpret_cast<LPARAM>(&bounds)) != 1) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(document.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    double delta_x = (state.panes.light_table_move_samples.back().x
        - state.panes.light_table_move_samples.front().x) / zoom;
    double delta_y = (state.panes.light_table_move_samples.back().y
        - state.panes.light_table_move_samples.front().y) / zoom;
    if (state.view.flip_horizontal) {
        delta_x = -delta_x;
    }
    if (state.view.flip_vertical) {
        delta_y = -delta_y;
    }
    const std::int64_t delta_x_milli = std::llround(delta_x * 1000.0);
    const std::int64_t delta_y_milli = std::llround(delta_y * 1000.0);
    const bool all_items = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
    InkpodStatus status = INKPOD_STATUS_OK;
    for (std::uint32_t index = 0U; index < 10000U; ++index) {
        if (!all_items && index != state.panes.active_light_table_item_index) {
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
    AppContext& state, const std::vector<std::wstring>& paths) noexcept {
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
    AppContext& state, const std::wstring& selected_path, bool composite_white) noexcept {
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

InkpodStatus SaveToPath(AppContext& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(state.document, *state.engine);
    const InkpodStatus status = shell.Save(path);
    if (status == INKPOD_STATUS_OK) {
        UpdateMenuState(state);
    }
    return status;
}

InkpodStatus SaveDocument(AppContext& state, bool force_dialog) noexcept {
    std::wstring path = state.document.current_path;
    if (force_dialog || path.empty()) {
        if (!ChooseInkpodPath(state.windows.window, true, path)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    return SaveToPath(state, path);
}

InkpodStatus OpenFromPath(AppContext& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(state.document, *state.engine);
    const InkpodStatus status = shell.Open(path);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.tools.active_plane = INKPOD_PLANE_MAIN_LINE;
    state.view.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetUiForDocumentReplacement(state);
    const InkpodPlaneKind plane = state.tools.active_plane;
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

InkpodStatus OpenRecoveryFromPath(AppContext& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(state.document, *state.engine);
    const InkpodStatus status = shell.OpenRecovery(path);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    state.tools.active_plane = INKPOD_PLANE_MAIN_LINE;
    state.view.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetUiForDocumentReplacement(state);
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

InkpodStatus OpenDocumentFromPath(
    AppContext& state, const std::wstring& path) noexcept {
    if (CommonRasterFormatFromPath(path) != 0U) {
        const InkpodStatus status = ImportCommonRasterFromPath(state, path);
        if (status == INKPOD_STATUS_OK) {
            UpdateMenuState(state);
        }
        return status;
    }
    std::wstring recovery;
    try {
        recovery = path + L".recovery.inkpod";
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!RecoveryIsNewer(path, recovery)) {
        return OpenFromPath(state, path);
    }

    const int choice = MessageBoxW(
        state.windows.window,
        L"通常保存より新しいRecoveryがあります。\n\n"
        L"はい: Recoveryを開く\nいいえ: Recoveryを破棄\n"
        L"キャンセル: 後で判断して通常保存を開く",
        L"inkpod Recovery",
        MB_YESNOCANCEL | MB_ICONQUESTION);
    if (choice == IDYES) {
        return OpenRecoveryFromPath(state, recovery);
    }
    if (choice == IDNO) {
        DeleteFileW(recovery.c_str());
    }
    return OpenFromPath(state, path);
}

bool QueueAutosave(AppContext& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    DocumentShellController shell(state.document, *state.engine);
    return shell.QueueAutosave(path);
}

InkpodStatus ApplyFillAtDeviceRange(
    AppContext& state,
    float device_x,
    float device_y,
    float end_device_x,
    float end_device_y,
    bool has_range) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.windows.canvas,
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
        if (state.view.flip_horizontal) {
            result = static_cast<double>(info.width) - result;
        }
        return result;
    };
    auto to_document_y = [&](double value) {
        double result = (value - bounds.top) / zoom;
        if (state.view.flip_vertical) {
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
    input.operation = state.tools.fill_options.operation;
    input.flags = (state.tools.fill_options.overflow_abort ? INKPOD_FILL_FLAG_OVERFLOW_ABORT : 0U)
        | (state.tools.fill_options.detached_regions ? INKPOD_FILL_FLAG_DETACHED_REGIONS : 0U)
        | (state.tools.fill_options.transparent_only ? INKPOD_FILL_FLAG_TRANSPARENT_ONLY : 0U)
        | (state.tools.fill_options.use_document_selection
                  ? INKPOD_FILL_FLAG_DOCUMENT_SELECTION
                  : 0U)
        | (state.tools.fill_options.light_table_boundary
                  ? INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY
                  : 0U)
        | (state.tools.fill_options.light_table_color ? INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR : 0U);
    input.seed_x = static_cast<std::uint32_t>(std::floor(document_x));
    input.seed_y = static_cast<std::uint32_t>(std::floor(document_y));
    input.color = state.tools.drawing_color;
    input.color.struct_size = sizeof(InkpodColorValue);
    input.tolerance = state.tools.fill_options.tolerance;
    input.gap_close = state.tools.fill_options.gap_close;
    input.inclusion_mode = state.tools.fill_options.inclusion_mode;
    input.extension_distance = state.tools.fill_options.extension_distance;
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
        inclusion_colors.reserve(state.tools.fill_options.inclusion_rgba.size());
        for (const std::uint32_t rgba : state.tools.fill_options.inclusion_rgba) {
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
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    FillController controller(*state.engine);
    const InkpodStatus status =
        controller.Apply(input, inclusion_colors, fill_result);
    if (status == INKPOD_STATUS_OK) {
        state.tools.active_plane = INKPOD_PLANE_COLOR;
    }
    if (status == INKPOD_STATUS_FILL_OVERFLOW && !state.lifetime.smoke_test
        && (fill_result.flags & INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE) != 0U) {
        std::array<wchar_t, 160U> message{};
        _snwprintf_s(
            message.data(),
            message.size(),
            _TRUNCATE,
            L"塗りあふれを中断しました。候補座標: (%u, %u)",
            fill_result.leak_x,
            fill_result.leak_y);
        MessageBoxW(state.windows.window, message.data(), L"inkpod", MB_OK | MB_ICONWARNING);
    }
    return status;
}

InkpodStatus ApplyFillAtDevicePoint(
    AppContext& state, float device_x, float device_y) noexcept {
    return ApplyFillAtDeviceRange(
        state, device_x, device_y, device_x, device_y, false);
}

InkpodStatus ApplySelectionGesture(
    AppContext& state, const std::vector<InkpodStrokeSample>& samples) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (samples.empty() || !QueryDocument(state, info)
        || SendMessageW(
               state.windows.canvas,
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
        if (state.view.flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.view.flip_vertical) {
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
        if (state.tools.selection_shape == INKPOD_SELECTION_LASSO
            || state.tools.selection_shape == INKPOD_SELECTION_POLYLINE
            || state.tools.selection_shape == INKPOD_SELECTION_TRACE) {
            points.reserve(samples.size());
            for (const auto& sample : samples) {
                points.push_back(document_point(sample));
            }
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if ((state.tools.selection_shape == INKPOD_SELECTION_LASSO
            || state.tools.selection_shape == INKPOD_SELECTION_POLYLINE)
        && points.size() < 3U) {
        if (state.engine == nullptr) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        SelectionController controller(*state.engine);
        return controller.ApplyEmpty(state.tools.selection_operation);
    }
    InkpodSelectionInput input{};
    input.struct_size = sizeof(input);
    input.shape = state.tools.selection_shape;
    input.operation = state.tools.selection_operation;
    input.tolerance = state.tools.selection_tolerance;
    input.gap_close = state.tools.selection_gap_close;
    input.diameter = state.tools.selection_diameter;
    if (state.tools.selection_shape == INKPOD_SELECTION_RECTANGLE
        || state.tools.selection_shape == INKPOD_SELECTION_ELLIPSE) {
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
    } else if (state.tools.selection_shape == INKPOD_SELECTION_WAND) {
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
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    SelectionController controller(*state.engine);
    return controller.Apply(input, points);
}

InkpodStatus SelectDrawingColor(
    AppContext& state, bool different, InkpodSelectionOperation operation) noexcept {
    InkpodColorValue color{};
    color.struct_size = sizeof(color);
    if (state.tools.active_plane == INKPOD_PLANE_MAIN_LINE) {
        color.depth = INKPOD_COLOR_DEPTH_BINARY;
        color.red = state.tools.color_rgba == 0U ? 0U : UINT8_MAX;
    } else {
        color = state.tools.drawing_color;
        color.struct_size = sizeof(InkpodColorValue);
    }
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    SelectionController controller(*state.engine);
    return controller.SelectColor(color, different, operation);
}

InkpodStatus EyedropAtDevicePoint(AppContext& state, float device_x, float device_y) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || SendMessageW(
               state.windows.canvas,
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
              [x, y, source = state.tools.eyedropper_source, &sampled](InkpodCore* core) {
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

bool DiscardCurrentRecovery(AppContext& state) noexcept {
    if (state.document.recovery_path.empty()) {
        return true;
    }
    if (state.engine != nullptr && state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return false;
    }
    if (DeleteFileW(state.document.recovery_path.c_str()) == FALSE
        && GetLastError() != ERROR_FILE_NOT_FOUND) {
        return false;
    }
    state.document.recovery_path.clear();
    return true;
}

bool ConfirmDiscard(AppContext& state) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)) {
        return true;
    }
    if ((info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        return true;
    }
    const int choice = MessageBoxW(
        state.windows.window,
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
                ShowCoreError(state, state.windows.window, L"保存");
            }
            return false;
        }
    } else if (!DiscardCurrentRecovery(state)) {
        ShowCoreError(state, state.windows.window, L"Recoveryの破棄");
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

void ResetBatchDerivedState(BatchUiState& batch) noexcept {
    BatchController::ResetDerivedState(batch);
}

InkpodColorValue BatchTransparentColor() noexcept {
    return InkpodColorValue{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 0U};
}

UINT FilterEditorCommandForBatchCommand(UINT command) noexcept {
    switch (command) {
        case IDM_BATCH_ADD_FILTER_SHARPEN_WEAK:
            return IDM_FILTER_SHARPEN_WEAK;
        case IDM_BATCH_ADD_FILTER_SHARPEN_STRONG:
            return IDM_FILTER_SHARPEN_STRONG;
        case IDM_BATCH_ADD_FILTER_BLUR_WEAK:
            return IDM_FILTER_BLUR_WEAK;
        case IDM_BATCH_ADD_FILTER_BLUR_STRONG:
            return IDM_FILTER_BLUR_STRONG;
        case IDM_BATCH_ADD_FILTER_GAUSSIAN:
            return IDM_FILTER_GAUSSIAN;
        case IDM_BATCH_ADD_FILTER_INVERT:
            return IDM_FILTER_INVERT;
        case IDM_BATCH_ADD_FILTER_AUTO_CONTRAST:
            return IDM_FILTER_AUTO_CONTRAST;
        case IDM_BATCH_ADD_FILTER_BRIGHTNESS:
            return IDM_FILTER_BRIGHTNESS;
        case IDM_BATCH_ADD_FILTER_TONE_CURVE:
            return IDM_FILTER_TONE_CURVE;
        case IDM_BATCH_ADD_FILTER_LEVELS:
            return IDM_FILTER_LEVELS;
        case IDM_BATCH_ADD_FILTER_HSV:
            return IDM_FILTER_HSV;
        case IDM_BATCH_ADD_FILTER_COLOR_BALANCE:
            return IDM_FILTER_COLOR_BALANCE;
        case IDM_BATCH_ADD_FILTER_UNSHARP:
            return IDM_FILTER_UNSHARP;
        default:
            return 0U;
    }
}

const wchar_t* BatchOperationLabel(UINT command) noexcept {
    for (const auto& entry : inkpod::windows::ui::BatchPaletteEntries()) {
        if (entry.command == command) {
            return entry.label;
        }
    }
    return L"バッチ項目";
}

bool AddBatchOperation(AppContext& state, UINT command) noexcept {
    BatchOperationUi operation{};
    operation.label = BatchOperationLabel(command);
    operation.color_0 = state.tools.drawing_color;
    operation.color_1 = state.tools.drawing_color;
    const UINT filter_command = FilterEditorCommandForBatchCommand(command);
    if (filter_command != 0U) {
        operation.kind = INKPOD_BATCH_OPERATION_FILTER;
        if (!ConfigureFilterEditor(state, filter_command, operation.filter)) {
            return false;
        }
    } else {
        switch (command) {
            case IDM_BATCH_ADD_COLOR_REPLACE: {
                operation.kind = INKPOD_BATCH_OPERATION_COLOR_REPLACE;
                InkpodBatchColorPairInput pair{};
                pair.struct_size = sizeof(pair);
                pair.enabled = 1U;
                pair.old_color = BatchTransparentColor();
                pair.new_color = state.tools.drawing_color;
                operation.color_pairs.push_back(pair);
                break;
            }
            case IDM_BATCH_ADD_CONTINUOUS_FILL: {
                operation.kind = INKPOD_BATCH_OPERATION_CONTINUOUS_FILL;
                InkpodBatchSeedInput seed{};
                seed.struct_size = sizeof(seed);
                seed.flags = INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR;
                seed.fill_color = state.tools.drawing_color;
                seed.expected_color = BatchTransparentColor();
                operation.seeds.push_back(seed);
                break;
            }
            case IDM_BATCH_ADD_SEPARATION:
                operation.kind = INKPOD_BATCH_OPERATION_SEPARATION;
                operation.colors.push_back(state.tools.drawing_color);
                operation.color_0 = BatchTransparentColor();
                break;
            case IDM_BATCH_ADD_VISIBILITY:
                operation.kind = INKPOD_BATCH_OPERATION_VISIBILITY;
                operation.plane_kind = 0U;
                operation.parameters[0] = 1;
                break;
            case IDM_BATCH_ADD_LINE_WIDTH:
                operation.kind = INKPOD_BATCH_OPERATION_LINE_WIDTH;
                operation.layer_kind = INKPOD_LAYER_VECTOR_COLORING;
                operation.plane_kind = INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE;
                operation.missing_policy = INKPOD_BATCH_MISSING_SKIP;
                operation.parameters[0] = INKPOD_VECTOR_WIDTH_SCALE;
                operation.parameters[1] = 1000;
                break;
            case IDM_BATCH_ADD_BOUNDARY_AIRBRUSH:
                operation.kind = INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH;
                operation.colors.push_back(BatchTransparentColor());
                operation.colors.push_back(state.tools.drawing_color);
                operation.parameters[0] = 4;
                operation.parameters[1] = 1000;
                break;
            case IDM_BATCH_ADD_DUST:
                operation.kind = INKPOD_BATCH_OPERATION_DUST_REMOVAL;
                operation.parameters[0] = INKPOD_DUST_REMOVE_FOREGROUND;
                operation.parameters[1] = 4;
                break;
            case IDM_BATCH_ADD_MIRROR:
                operation.kind = INKPOD_BATCH_OPERATION_MIRROR;
                operation.layer_kind = 0U;
                operation.plane_kind = 0U;
                operation.missing_policy = 0U;
                operation.parameters[0] = INKPOD_MIRROR_HORIZONTAL;
                break;
            case IDM_BATCH_ADD_ROTATE:
                operation.kind = INKPOD_BATCH_OPERATION_ROTATE_90;
                operation.layer_kind = 0U;
                operation.plane_kind = 0U;
                operation.missing_policy = 0U;
                operation.parameters[0] = INKPOD_ROTATE_RIGHT_90;
                break;
            case IDM_BATCH_ADD_RESIZE: {
                operation.kind = INKPOD_BATCH_OPERATION_RESIZE;
                operation.layer_kind = 0U;
                operation.plane_kind = 0U;
                operation.missing_policy = 0U;
                InkpodDocumentInfo document = EmptyDocumentInfo();
                if (!QueryDocument(state, document)) {
                    return false;
                }
                operation.parameters = {
                    document.width,
                    document.height,
                    document.dpi_x_milli,
                    document.dpi_y_milli,
                    0,
                    INKPOD_RESIZE_ANCHOR_CENTER,
                    0,
                    0};
                break;
            }
            case IDM_BATCH_ADD_CONVERT:
                operation.kind = INKPOD_BATCH_OPERATION_CONVERT_PLANE;
                operation.parameters[0] = INKPOD_TYPED_PLANE_RASTER;
                operation.parameters[1] = INKPOD_STORAGE_RGBA8;
                break;
            default:
                return false;
        }
    }
    try {
        ResetBatchDerivedState(state.batch);
        state.batch.operations.push_back(std::move(operation));
        state.batch.selected_operation =
            static_cast<std::uint32_t>(state.batch.operations.size() - 1U);
        RefreshBatchPalette(state.batch);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

UINT FilterEditorCommandForKind(std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_FILTER_SHARPEN_WEAK:
            return IDM_FILTER_SHARPEN_WEAK;
        case INKPOD_FILTER_SHARPEN_STRONG:
            return IDM_FILTER_SHARPEN_STRONG;
        case INKPOD_FILTER_BLUR_WEAK:
            return IDM_FILTER_BLUR_WEAK;
        case INKPOD_FILTER_BLUR_STRONG:
            return IDM_FILTER_BLUR_STRONG;
        case INKPOD_FILTER_GAUSSIAN_BLUR:
            return IDM_FILTER_GAUSSIAN;
        case INKPOD_FILTER_INVERT:
            return IDM_FILTER_INVERT;
        case INKPOD_FILTER_AUTO_CONTRAST:
            return IDM_FILTER_AUTO_CONTRAST;
        case INKPOD_FILTER_BRIGHTNESS_CONTRAST:
            return IDM_FILTER_BRIGHTNESS;
        case INKPOD_FILTER_TONE_CURVE:
            return IDM_FILTER_TONE_CURVE;
        case INKPOD_FILTER_LEVELS:
            return IDM_FILTER_LEVELS;
        case INKPOD_FILTER_HSV:
            return IDM_FILTER_HSV;
        case INKPOD_FILTER_COLOR_BALANCE:
            return IDM_FILTER_COLOR_BALANCE;
        case INKPOD_FILTER_UNSHARP_MASK:
            return IDM_FILTER_UNSHARP;
        default:
            return 0U;
    }
}

bool EditSelectedBatchOperation(AppContext& state) noexcept {
    if (state.batch.loaded_graph
        || state.batch.selected_operation >= state.batch.operations.size()) {
        return false;
    }
    BatchOperationUi operation{};
    try {
        operation = state.batch.operations[state.batch.selected_operation];
    } catch (const std::bad_alloc&) {
        return false;
    }
    ViewOptionsDialogState metadata{};
    metadata.title = L"バッチ項目の状態";
    metadata.labels = {
        L"有効 (0/1)", L"実行ごとに設定 (0/1)", nullptr, nullptr};
    metadata.values = {
        (operation.flags & INKPOD_BATCH_OPERATION_ENABLED) != 0U ? 1 : 0,
        (operation.flags & INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN) != 0U ? 1 : 0,
        0,
        0};
    metadata.value_count = 2U;
    if (ShowViewOptions(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, metadata) != IDOK
        || metadata.values[0] < 0 || metadata.values[0] > 1
        || metadata.values[1] < 0 || metadata.values[1] > 1) {
        return false;
    }
    operation.flags = (metadata.values[0] != 0 ? INKPOD_BATCH_OPERATION_ENABLED : 0U)
        | (metadata.values[1] != 0
                ? INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN
                : 0U);
    if (operation.missing_policy != 0U) {
        ViewOptionsDialogState target{};
        target.title = L"バッチ対象セレクター";
        target.labels = {
            L"layer kind (0=IDのみ)",
            L"plane kind (0=layer対象)",
            L"欠落時 (1:skip 2:error)",
            nullptr};
        target.values = {
            static_cast<std::int32_t>(operation.layer_kind),
            static_cast<std::int32_t>(operation.plane_kind),
            static_cast<std::int32_t>(operation.missing_policy),
            0};
        target.value_count = 3U;
        if (ShowViewOptions(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, target) != IDOK
            || target.values[0] < 0 || target.values[1] < 0
            || (target.values[2]
                    != static_cast<std::int32_t>(INKPOD_BATCH_MISSING_SKIP)
                && target.values[2]
                    != static_cast<std::int32_t>(INKPOD_BATCH_MISSING_ERROR))
            || (target.values[0] == 0 && target.values[1] == 0
                && operation.layer_id == 0U && operation.plane_id == 0U)) {
            return false;
        }
        operation.layer_kind = static_cast<InkpodLayerKind>(target.values[0]);
        operation.plane_kind = static_cast<InkpodTypedPlaneKind>(target.values[1]);
        operation.missing_policy =
            static_cast<InkpodBatchMissingPolicy>(target.values[2]);
    }
    if (operation.kind == INKPOD_BATCH_OPERATION_FILTER) {
        const UINT command = FilterEditorCommandForKind(operation.filter.kind);
        if (command == 0U || !ConfigureFilterEditor(state, command, operation.filter)) {
            return false;
        }
    } else if (operation.kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        if (operation.color_pairs.empty()) {
            return false;
        }
        operation.color_pairs[0].new_color = state.tools.drawing_color;
    } else {
        ViewOptionsDialogState dialog{};
        dialog.title = L"バッチ項目の編集";
        dialog.labels = {L"P0", L"P1", L"P2", L"P3"};
        dialog.values = {
            static_cast<std::int32_t>(operation.parameters[0]),
            static_cast<std::int32_t>(operation.parameters[1]),
            static_cast<std::int32_t>(operation.parameters[2]),
            static_cast<std::int32_t>(operation.parameters[3])};
        dialog.value_count = 2U;
        if (operation.kind == INKPOD_BATCH_OPERATION_CONTINUOUS_FILL) {
            if (operation.seeds.empty()) {
                return false;
            }
            dialog.labels = {L"seed X", L"seed Y", L"tolerance", L"gap close"};
            dialog.values = {
                static_cast<std::int32_t>(operation.seeds[0].x),
                static_cast<std::int32_t>(operation.seeds[0].y),
                static_cast<std::int32_t>(operation.seeds[0].tolerance),
                static_cast<std::int32_t>(operation.seeds[0].gap_close)};
            dialog.value_count = 4U;
        } else if (operation.kind == INKPOD_BATCH_OPERATION_SEPARATION) {
            dialog.labels = {L"反転 (0/1)", nullptr, nullptr, nullptr};
            dialog.value_count = 1U;
        } else if (operation.kind == INKPOD_BATCH_OPERATION_VISIBILITY) {
            dialog.labels = {L"表示 (0/1)", nullptr, nullptr, nullptr};
            dialog.value_count = 1U;
        } else if (operation.kind == INKPOD_BATCH_OPERATION_LINE_WIDTH) {
            dialog.labels = {L"mode (1-4)", L"value x1000", nullptr, nullptr};
        } else if (operation.kind == INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH) {
            dialog.labels = {L"幅", L"強さ x1000", nullptr, nullptr};
        } else if (operation.kind == INKPOD_BATCH_OPERATION_DUST_REMOVAL) {
            dialog.labels = {L"mode (1-3)", L"最大pixel", nullptr, nullptr};
        } else if (operation.kind == INKPOD_BATCH_OPERATION_MIRROR) {
            dialog.labels = {L"方向 (1:左右 2:上下)", nullptr, nullptr, nullptr};
            dialog.value_count = 1U;
        } else if (operation.kind == INKPOD_BATCH_OPERATION_ROTATE_90) {
            dialog.labels = {L"方向 (1:左 2:右)", nullptr, nullptr, nullptr};
            dialog.value_count = 1U;
        } else if (operation.kind == INKPOD_BATCH_OPERATION_RESIZE) {
            dialog.labels = {L"幅", L"高さ", L"X DPI x1000", L"Y DPI x1000"};
            dialog.value_count = 4U;
        } else if (operation.kind == INKPOD_BATCH_OPERATION_CONVERT_PLANE) {
            dialog.labels = {L"plane kind", L"pixel format", nullptr, nullptr};
        }
        if (ShowViewOptions(state.lifetime.instance, state.windows.window, state.lifetime.smoke_test, dialog) != IDOK) {
            return false;
        }
        if (operation.kind == INKPOD_BATCH_OPERATION_CONTINUOUS_FILL) {
            if (dialog.values[0] < 0 || dialog.values[1] < 0
                || dialog.values[2] < 0 || dialog.values[2] > UINT16_MAX
                || dialog.values[3] < 0 || dialog.values[3] > UINT8_MAX) {
                return false;
            }
            operation.seeds[0].x = static_cast<std::uint32_t>(dialog.values[0]);
            operation.seeds[0].y = static_cast<std::uint32_t>(dialog.values[1]);
            operation.seeds[0].tolerance = static_cast<std::uint32_t>(dialog.values[2]);
            operation.seeds[0].gap_close = static_cast<std::uint32_t>(dialog.values[3]);
            operation.seeds[0].fill_color = state.tools.drawing_color;
        } else {
            for (std::size_t index = 0; index < dialog.value_count; ++index) {
                operation.parameters[index] = dialog.values[index];
            }
        }
    }
    state.batch.operations[state.batch.selected_operation] = std::move(operation);
    ResetBatchDerivedState(state.batch);
    RefreshBatchPalette(state.batch);
    return true;
}

std::wstring BatchReportSummary(const InkpodBatchReport* report) {
    return BatchController::ReportSummary(report);
}

InkpodStatus PreviewBatch(AppContext& state, InkpodBatchRunScope scope) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    BatchController controller(
        state.lifetime, state.windows, state.batch, *state.engine);
    return controller.Preview(scope);
}

InkpodStatus StartBatch(
    AppContext& state,
    InkpodBatchRunScope scope,
    bool dry_run) noexcept {
    if (state.engine == nullptr || state.batch.task != nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    BatchController controller(
        state.lifetime, state.windows, state.batch, *state.engine);
    return controller.Start(scope, dry_run, kBatchTaskCompleted);
}

bool ChooseBatchSettingsPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    if (!selected_path.empty()) {
        wcsncpy_s(path.data(), path.size(), selected_path.c_str(), _TRUNCATE);
    }
    constexpr wchar_t filter[] =
        L"inkpod バッチセット (*.inkbatch)\0*.inkbatch\0すべてのファイル (*.*)\0*.*\0\0";
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter;
    dialog.lpstrFile = path.data();
    dialog.nMaxFile = static_cast<DWORD>(path.size());
    dialog.lpstrDefExt = L"inkbatch";
    dialog.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        | (save ? OFN_OVERWRITEPROMPT : OFN_FILEMUSTEXIST);
    if ((save ? GetSaveFileNameW(&dialog) : GetOpenFileNameW(&dialog)) == FALSE) {
        return false;
    }
    try {
        selected_path.assign(path.data());
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool ChooseBatchFolder(HWND owner, std::wstring& selected_path) noexcept {
    return BatchController::ChooseFolder(owner, selected_path);
}

void RelayoutWorkspace(AppContext& state) noexcept {
    RECT client{};
    if (GetClientRect(state.windows.window, &client) != FALSE) {
        inkpod::windows::ui::LayoutMainChrome(
            state.windows,
            state.lifetime.smoke_test,
            client.right - client.left,
            client.bottom - client.top);
    }
}

bool InitializeMainChrome(AppContext& state) noexcept {
    if (!state.lifetime.smoke_test) {
        LoadWorkspaceLayout(state.windows.workspace, L"WorkspaceSessionV2");
    }
    if (!inkpod::windows::ui::CreateMainChrome(
            state.windows, state.lifetime.instance, state.lifetime.smoke_test)) {
        return false;
    }
    state.batch.palette_dialog = {
        &state,
        DispatchBatchPaletteCommand,
        SelectBatchPaletteOperation,
        state.batch.loaded_graph};
    state.batch.palette = inkpod::windows::ui::CreateBatchPaletteDialog(
        state.lifetime.instance, state.windows.window, state.batch.palette_dialog);
    if (state.batch.palette == nullptr) {
        return false;
    }
    RefreshBatchPalette(state.batch);
    ShowWindow(state.batch.palette, SW_HIDE);
    state.tools.palette_dialog = {};
    state.tools.palette_dialog.context = &state;
    state.tools.palette_dialog.dispatch_command = DispatchToolPaletteCommand;
    state.tools.palette_dialog.visibility_changed =
        NotifyToolPaletteVisibilityChanged;
    state.tools.palette = inkpod::windows::ui::CreateToolPaletteDialog(
        state.lifetime.instance,
        state.windows.window,
        state.tools.palette_dialog);
    if (state.tools.palette == nullptr) {
        return false;
    }
    state.windows.tool_palette = state.tools.palette;
    state.tools.options_pane.context = &state;
    state.tools.options_pane.dispatch_command = DispatchToolPaletteCommand;
    state.tools.options_pane.change_diameter = ChangeToolOptionsDiameter;
    state.tools.options_pane.active_tool = state.tools.active_tool;
    state.tools.options_pane.active_plane = state.tools.active_plane;
    state.tools.options_pane.diameter = state.tools.diameter;
    state.windows.tool_options =
        inkpod::windows::ui::panes::CreateToolOptionsPane(
            state.lifetime.instance,
            state.windows.window,
            state.tools.options_pane);
    if (state.windows.tool_options == nullptr) {
        return false;
    }
    state.panes.color_pane.context = &state;
    state.panes.color_pane.dispatch_command = DispatchToolPaletteCommand;
    state.panes.color_pane.change_color = ChangeDockDrawingColor;
    state.panes.color_pane.select_color = SelectDockColor;
    state.panes.color_pane.change_group = ChangeDockPaletteGroup;
    state.windows.color_pane = inkpod::windows::ui::panes::CreateColorDockPane(
        state.lifetime.instance,
        state.windows.window,
        state.panes.color_pane);
    if (state.windows.color_pane == nullptr) {
        return false;
    }
    auto& layer_dialog = state.panes.layer_palette_dialog;
    layer_dialog.context = &state;
    layer_dialog.dispatch_command = DispatchLayerPaletteCommand;
    layer_dialog.select_layer = SelectLayerPaletteLayer;
    layer_dialog.select_plane = SelectLayerPalettePlane;
    layer_dialog.reorder_layer = ReorderLayerPaletteLayer;
    layer_dialog.reorder_plane = ReorderLayerPalettePlane;
    layer_dialog.change_split = ChangeLayerPaletteSplit;
    layer_dialog.visibility_changed = NotifyLayerPaletteVisibilityChanged;
    layer_dialog.split_milli = state.windows.workspace.layer_split_milli;
    state.panes.layer_palette = CreateLayerPaletteDialog(
        state.lifetime.instance,
        state.windows.window,
        state.panes.layer_palette_dialog);
    if (state.panes.layer_palette == nullptr) {
        return false;
    }
    state.windows.layer_palette = state.panes.layer_palette;
    RelayoutWorkspace(state);
    return true;
}

void ShowInitialPalettes(AppContext& state) noexcept {
    RelayoutWorkspace(state);
    RefreshColorPanes(state);
    RefreshDockPaneViews(state);
    RefreshTreePane(state);
    UpdateMenuState(state);
}

std::optional<LRESULT> RouteBatchCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_BATCH:
            if (state->batch.palette != nullptr) {
                const bool visible = IsWindowVisible(state->batch.palette) != FALSE;
                ShowWindow(state->batch.palette, visible ? SW_HIDE : SW_SHOW);
                if (!visible) {
                    RefreshBatchPalette(state->batch);
                    SetForegroundWindow(state->batch.palette);
                }
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_INPUT_FILE:
        case IDM_BATCH_INPUT_FOLDER:
        case IDM_BATCH_INPUT_CURRENT: {
            if (state->batch.task != nullptr || state->batch.loaded_graph) {
                return 0;
            }
            std::wstring path;
            InkpodBatchInputKind kind = INKPOD_BATCH_INPUT_CURRENT_SEQUENCE;
            bool accepted = true;
            if (LOWORD(wparam) == IDM_BATCH_INPUT_FILE) {
                kind = INKPOD_BATCH_INPUT_FILE;
                accepted = ChooseInkpodPath(window, false, path);
            } else if (LOWORD(wparam) == IDM_BATCH_INPUT_FOLDER) {
                kind = INKPOD_BATCH_INPUT_FOLDER;
                accepted = ChooseBatchFolder(window, path);
            }
            if (!accepted) {
                return 0;
            }
            ResetBatchDerivedState(state->batch);
            state->batch.input_kind = kind;
            state->batch.input_path = std::move(path);
            state->batch.first_cell = 0U;
            state->batch.last_cell = 0U;
            RefreshBatchPalette(state->batch);
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_BATCH_INPUT_RANGE: {
            if (state->batch.task != nullptr || state->batch.loaded_graph) {
                return 0;
            }
            ViewOptionsDialogState dialog{};
            dialog.title = L"バッチ入力範囲";
            dialog.labels = {
                L"開始セル番号 (0=先頭)",
                L"終了セル番号 (0=末尾)",
                nullptr,
                nullptr};
            dialog.values = {
                static_cast<std::int32_t>(state->batch.first_cell),
                static_cast<std::int32_t>(state->batch.last_cell),
                0,
                0};
            dialog.value_count = 2U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.values[0] < 0 || dialog.values[1] < 0
                || (dialog.values[0] != 0 && dialog.values[1] != 0
                    && dialog.values[0] > dialog.values[1])) {
                return 0;
            }
            ResetBatchDerivedState(state->batch);
            state->batch.first_cell =
                static_cast<std::uint32_t>(dialog.values[0]);
            state->batch.last_cell =
                static_cast<std::uint32_t>(dialog.values[1]);
            RefreshBatchPalette(state->batch);
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_BATCH_ADD_COLOR_REPLACE:
        case IDM_BATCH_ADD_CONTINUOUS_FILL:
        case IDM_BATCH_ADD_SEPARATION:
        case IDM_BATCH_ADD_VISIBILITY:
        case IDM_BATCH_ADD_LINE_WIDTH:
        case IDM_BATCH_ADD_BOUNDARY_AIRBRUSH:
        case IDM_BATCH_ADD_DUST:
        case IDM_BATCH_ADD_MIRROR:
        case IDM_BATCH_ADD_ROTATE:
        case IDM_BATCH_ADD_RESIZE:
        case IDM_BATCH_ADD_CONVERT:
        case IDM_BATCH_ADD_FILTER_SHARPEN_WEAK:
        case IDM_BATCH_ADD_FILTER_SHARPEN_STRONG:
        case IDM_BATCH_ADD_FILTER_BLUR_WEAK:
        case IDM_BATCH_ADD_FILTER_BLUR_STRONG:
        case IDM_BATCH_ADD_FILTER_GAUSSIAN:
        case IDM_BATCH_ADD_FILTER_INVERT:
        case IDM_BATCH_ADD_FILTER_AUTO_CONTRAST:
        case IDM_BATCH_ADD_FILTER_BRIGHTNESS:
        case IDM_BATCH_ADD_FILTER_TONE_CURVE:
        case IDM_BATCH_ADD_FILTER_LEVELS:
        case IDM_BATCH_ADD_FILTER_HSV:
        case IDM_BATCH_ADD_FILTER_COLOR_BALANCE:
        case IDM_BATCH_ADD_FILTER_UNSHARP:
            if (state->batch.task == nullptr
                && AddBatchOperation(*state, LOWORD(wparam))) {
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_OPERATION_EDIT:
            if (state->batch.task == nullptr
                && EditSelectedBatchOperation(*state)) {
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_REPLACE_SWAP:
            if (state->batch.task == nullptr && !state->batch.loaded_graph
                && state->batch.selected_operation < state->batch.operations.size()) {
                BatchOperationUi& operation =
                    state->batch.operations[state->batch.selected_operation];
                if (operation.kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE
                    && !operation.color_pairs.empty()) {
                    for (auto& pair : operation.color_pairs) {
                        std::swap(pair.old_color, pair.new_color);
                    }
                    ResetBatchDerivedState(state->batch);
                    RefreshBatchPalette(state->batch);
                    UpdateMenuState(*state);
                    return 1;
                }
            }
            return 0;
        case IDM_BATCH_OPERATION_REMOVE:
            if (state->batch.task == nullptr && !state->batch.loaded_graph
                && state->batch.selected_operation < state->batch.operations.size()) {
                state->batch.operations.erase(
                    state->batch.operations.begin()
                    + state->batch.selected_operation);
                if (!state->batch.operations.empty()) {
                    state->batch.selected_operation = std::min<std::uint32_t>(
                        state->batch.selected_operation,
                        static_cast<std::uint32_t>(
                            state->batch.operations.size() - 1U));
                } else {
                    state->batch.selected_operation = 0U;
                }
                ResetBatchDerivedState(state->batch);
                RefreshBatchPalette(state->batch);
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_OPERATION_UP:
        case IDM_BATCH_OPERATION_DOWN:
            if (state->batch.task == nullptr && !state->batch.loaded_graph
                && state->batch.selected_operation < state->batch.operations.size()) {
                const std::uint32_t current = state->batch.selected_operation;
                const bool up = LOWORD(wparam) == IDM_BATCH_OPERATION_UP;
                if ((up && current > 0U)
                    || (!up && current + 1U < state->batch.operations.size())) {
                    const std::uint32_t target = up ? current - 1U : current + 1U;
                    std::swap(
                        state->batch.operations[current],
                        state->batch.operations[target]);
                    state->batch.selected_operation = target;
                    ResetBatchDerivedState(state->batch);
                    RefreshBatchPalette(state->batch);
                    UpdateMenuState(*state);
                    return 1;
                }
            }
            return 0;
        case IDM_BATCH_OUTPUT_DUPLICATE:
        case IDM_BATCH_OUTPUT_NEW:
        case IDM_BATCH_OUTPUT_OVERWRITE:
            if (state->batch.task == nullptr && !state->batch.loaded_graph) {
                ResetBatchDerivedState(state->batch);
                state->batch.output_policy = LOWORD(wparam) == IDM_BATCH_OUTPUT_NEW
                    ? INKPOD_BATCH_OUTPUT_NEW_SAVE
                    : (LOWORD(wparam) == IDM_BATCH_OUTPUT_OVERWRITE
                              ? INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE
                              : INKPOD_BATCH_OUTPUT_DUPLICATE);
                RefreshBatchPalette(state->batch);
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_OUTPUT_SETTINGS: {
            if (state->batch.task != nullptr || state->batch.loaded_graph) {
                return 0;
            }
            ViewOptionsDialogState dialog{};
            dialog.title = L"バッチ出力設定";
            dialog.labels = {
                L"cell folder (0/1)",
                L"開始番号",
                L"降順 (0/1)",
                L"file間 wait (ms)"};
            dialog.values = {
                state->batch.cell_folder ? 1 : 0,
                static_cast<std::int32_t>(state->batch.start_number),
                state->batch.descending ? 1 : 0,
                static_cast<std::int32_t>(state->batch.wait_milliseconds)};
            dialog.value_count = 4U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.values[0] < 0 || dialog.values[0] > 1
                || dialog.values[1] < 0
                || dialog.values[2] < 0 || dialog.values[2] > 1
                || dialog.values[3] < 0 || dialog.values[3] > 3'600'000) {
                return 0;
            }
            TextInputDialogState folder{};
            folder.title = L"バッチ出力フォルダー";
            folder.label = L"空欄は入力と同じ場所";
            folder.value = state->batch.output_folder;
            TextInputDialogState basename{};
            basename.title = L"バッチ出力basename";
            basename.label = L"空欄は入力名を使用";
            basename.value = state->batch.basename;
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, folder) != IDOK
                || ShowTextInput(
                       state->lifetime.instance, window, state->lifetime.smoke_test, basename) != IDOK) {
                return 0;
            }
            bool preview_before_save = state->batch.preview_before_save;
            if (!state->lifetime.smoke_test) {
                const int preview_choice = MessageBoxW(
                    window,
                    L"保存前に入力・出力と警告のプレビュー確認を必須にしますか？",
                    L"バッチ出力設定",
                    MB_YESNOCANCEL | MB_ICONQUESTION);
                if (preview_choice == IDCANCEL) {
                    return 0;
                }
                preview_before_save = preview_choice == IDYES;
            }
            ResetBatchDerivedState(state->batch);
            state->batch.cell_folder = dialog.values[0] != 0;
            state->batch.start_number =
                static_cast<std::uint32_t>(dialog.values[1]);
            state->batch.descending = dialog.values[2] != 0;
            state->batch.wait_milliseconds =
                static_cast<std::uint32_t>(dialog.values[3]);
            state->batch.output_folder = std::move(folder.value);
            state->batch.basename = std::move(basename.value);
            state->batch.preview_before_save = preview_before_save;
            RefreshBatchPalette(state->batch);
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_BATCH_FAILURE_CONTINUE:
        case IDM_BATCH_FAILURE_STOP:
            if (state->batch.task == nullptr && !state->batch.loaded_graph) {
                ResetBatchDerivedState(state->batch);
                state->batch.failure_policy = LOWORD(wparam) == IDM_BATCH_FAILURE_STOP
                    ? INKPOD_BATCH_FAILURE_STOP
                    : INKPOD_BATCH_FAILURE_CONTINUE;
                RefreshBatchPalette(state->batch);
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_PREVIEW: {
            const InkpodStatus status = PreviewBatch(*state, INKPOD_BATCH_SCOPE_ALL);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"バッチプレビュー");
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_BATCH_DRY_RUN:
        case IDM_BATCH_RUN_CURRENT:
        case IDM_BATCH_RUN_ALL: {
            const UINT command = LOWORD(wparam);
            const InkpodStatus status = StartBatch(
                *state,
                command == IDM_BATCH_RUN_CURRENT
                    ? INKPOD_BATCH_SCOPE_CURRENT
                    : INKPOD_BATCH_SCOPE_ALL,
                command == IDM_BATCH_DRY_RUN);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, L"バッチ実行");
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_BATCH_CANCEL:
            if (state->batch.task != nullptr
                && inkpod_batch_task_cancel(state->batch.task) == INKPOD_STATUS_OK) {
                return 1;
            }
            return 0;
        case IDM_BATCH_SAVE_SET:
        case IDM_BATCH_LOAD_SET: {
            const bool save = LOWORD(wparam) == IDM_BATCH_SAVE_SET;
            std::wstring path = state->lifetime.smoke_test
                ? L"inkpod-batch-ui-smoke.inkbatch"
                : L"";
            if (!state->lifetime.smoke_test
                && !ChooseBatchSettingsPath(window, save, path)) {
                return 0;
            }
            std::vector<std::uint8_t> utf8;
            if (!WidePathToUtf8(path, utf8)) {
                return 0;
            }
            InkpodStatus status = INKPOD_STATUS_OK;
            BatchController controller(
                state->lifetime,
                state->windows,
                state->batch,
                *state->engine);
            if (save) {
                status = controller.SaveGraph(utf8.data(), utf8.size());
            } else {
                status = controller.LoadGraph(utf8.data(), utf8.size());
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, save ? L"バッチセット保存" : L"バッチセット読込");
            }
            RefreshBatchPalette(state->batch);
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteDocumentCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_FILE_NEW:
            if (ConfirmDiscard(*state)) {
                ViewOptionsDialogState dialog{};
                dialog.title = L"新規セル";
                dialog.labels = {L"幅 (px)", L"高さ (px)", L"DPI", L"レイヤー種別"};
                dialog.values = {1920, 1080, 96, INKPOD_LAYER_BINARY_COLORING};
                dialog.value_count = 4U;
                if (ShowViewOptions(
                        state->lifetime.instance,
                        window,
                        state->lifetime.smoke_test,
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
                    const InkpodStatus status = OpenDocumentFromPath(*state, path);
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
            std::wstring path = state->lifetime.smoke_test ? state->lifetime.smoke_raster_path : L"";
            if (!state->lifetime.smoke_test && !ChooseCommonRasterPath(window, false, path)) {
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
            std::wstring path = state->lifetime.smoke_test ? state->lifetime.smoke_raster_path : L"";
            if (!state->lifetime.smoke_test && !ChooseCommonRasterPath(window, true, path)) {
                return 0;
            }
            ViewOptionsDialogState dialog{};
            dialog.title = L"ラスター書き出し";
            dialog.labels[0] = L"白背景を合成 (0/1)";
            dialog.values[0] = state->lifetime.smoke_test ? 1 : 0;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
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
            std::wstring path = state->document.recovery_path;
            if (path.empty() && !ChooseInkpodPath(window, true, path)) {
                return 0;
            }
            if (!QueueAutosave(*state, path)) {
                ShowCoreError(*state, window, L"Recovery保存の予約");
            } else {
                try {
                    state->document.recovery_path = path;
                } catch (const std::bad_alloc&) {
                    ShowCoreError(*state, window, L"Recovery path の保持");
                }
            }
            return 0;
        }
        case IDM_FILE_OPEN_RECOVERY: {
            if (ConfirmDiscard(*state)) {
                std::wstring path = state->document.recovery_path;
                if (ChooseInkpodPath(window, false, path)
                    && OpenRecoveryFromPath(*state, path) != INKPOD_STATUS_OK) {
                    ShowCoreError(*state, window, L"Recoveryを開く");
                }
            }
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteEditCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
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
                || ShowHistoryDialog(state->lifetime.instance, window, dialog) != IDOK) {
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
                inkpod_clipboard_release(&state->document.clipboard);
                state->document.clipboard = replacement;
                if (!PublishStandardClipboard(window, state->document.clipboard)
                    && !state->lifetime.smoke_test) {
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
            const InkpodStatus status = state->document.clipboard == nullptr
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
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            TextInputDialogState name{};
            name.title = L"変換先プレーン";
            name.label = L"名前";
            name.value = L"Pasted Plane";
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, name) != IDOK) {
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
            edit.parent_id = state->panes.active_tree_layer_id;
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
                state->panes.active_tree_plane_id = plane_id;
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
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteEffectsCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_FILTER_LAST: {
            InkpodDocumentInfo document{};
            const InkpodStatus status = state->tools.active_plane != INKPOD_PLANE_COLOR
                    || !QueryDocument(*state, document)
                ? INKPOD_STATUS_INVALID_STATE
                : StartEffectTask(
                      *state,
                      false,
                      [plane_id = document.color_plane_id](
                          InkpodCore* core, InkpodTask* task) {
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
            FilterJob job{};
            if (state->tools.active_plane != INKPOD_PLANE_COLOR
                || !QueryDocument(*state, document)
                || !ConfigureFilterEditor(*state, command, job)) {
                return 0;
            }
            job.plane_id = document.color_plane_id;
            const InkpodStatus status = QueueFilter(*state, std::move(job));
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"フィルタ");
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_ADJUSTMENT_CREATE:
        case IDM_ADJUSTMENT_EDIT: {
            const bool update = LOWORD(wparam) == IDM_ADJUSTMENT_EDIT;
            FilterJob job{};
            if (!ConfigureAdjustmentEditor(*state, job, update)) {
                return 0;
            }
            const InkpodStatus status = CreateOrUpdateAdjustment(
                *state, std::move(job), update);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"調整レイヤー");
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_ADJUSTMENT_PREVIOUS:
        case IDM_ADJUSTMENT_NEXT:
            SelectAdjustment(
                *state, LOWORD(wparam) == IDM_ADJUSTMENT_NEXT);
            UpdateMenuState(*state);
            return 0;
        case IDM_ADJUSTMENT_TOGGLE: {
            const bool visible = !state->effects.adjustment_visible;
            const InkpodStatus status = SetAdjustmentVisibility(*state, visible);
            if (status == INKPOD_STATUS_OK) {
                state->effects.adjustment_visible = visible;
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
                state->effects.adjustment_id,
                0U,
                ignored);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"調整レイヤー移動");
            }
            UpdateMenuState(*state);
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteDocumentPaneCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_LAYER_PALETTE:
            if (state->panes.layer_palette != nullptr) {
                state->windows.workspace.layer_visible =
                    !state->windows.workspace.layer_visible;
                RelayoutWorkspace(*state);
                if (state->windows.workspace.layer_visible) {
                    RefreshTreePane(*state);
                }
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_LAYER_NEW: {
            ViewOptionsDialogState dialog{};
            dialog.title = L"新規レイヤー";
            dialog.labels = {L"種類", L"不透明度 (%)", nullptr, nullptr};
            dialog.values = {INKPOD_LAYER_RASTER, 100, 0, 0};
            dialog.first_value_choices = kLayerKindChoices.data();
            dialog.first_value_choice_count =
                static_cast<std::uint32_t>(kLayerKindChoices.size());
            dialog.value_count = 2U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
                state->panes.active_tree_layer_id = layer_id;
                state->document.smoke_layer_id = layer_id;
                RefreshTreePane(*state);
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_LAYER_DUPLICATE: {
            InkpodDocumentInfo info{};
            std::uint64_t duplicate_id{};
            const std::uint64_t source_id = state->panes.active_tree_layer_id != 0U
                ? state->panes.active_tree_layer_id
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
                state->document.smoke_layer_id = duplicate_id;
                state->panes.active_tree_layer_id = duplicate_id;
                RefreshTreePane(*state);
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_LAYER_DELETE: {
            std::uint64_t ignored{};
            const std::uint64_t target = state->panes.active_tree_layer_id != 0U
                ? state->panes.active_tree_layer_id
                : state->document.smoke_layer_id;
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
                if (state->document.smoke_layer_id == target) {
                    state->document.smoke_layer_id = 0U;
                }
                state->panes.active_tree_layer_id = 0U;
                RefreshTreePane(*state);
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_LAYER_MOVE_TOP: {
            InkpodDocumentInfo info{};
            const std::uint64_t target = state->document.smoke_layer_id != 0U
                ? state->document.smoke_layer_id
                : (state->panes.active_tree_layer_id != 0U
                          ? state->panes.active_tree_layer_id
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
            const int count = static_cast<int>(state->panes.tree_layer_count);
            const int current = static_cast<int>(state->panes.active_tree_layer_index);
            const int destination = LOWORD(wparam) == IDM_LAYER_MOVE_UP
                ? std::max(0, current - 1)
                : std::min(std::max(0, count - 1), current + 1);
            std::uint64_t ignored{};
            const InkpodStatus status = ApplyTreeEdit(
                *state,
                INKPOD_TREE_REORDER_LAYER,
                state->panes.active_tree_layer_id,
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
            dialog.labels[0] = L"変換先種類";
            dialog.values[0] = INKPOD_LAYER_RASTER;
            dialog.first_value_choices = kLayerKindChoices.data();
            dialog.first_value_choice_count =
                static_cast<std::uint32_t>(kLayerKindChoices.size());
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CONVERT_LAYER;
            edit.object_id = state->panes.active_tree_layer_id;
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
            edit.object_id = state->panes.active_tree_layer_id;
            std::uint64_t ignored{};
            const InkpodStatus status = ApplyTreeEditRecord(*state, edit, {}, ignored);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"同種レイヤーの統合");
            }
            state->panes.active_tree_layer_id = 0U;
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
            state->panes.active_tree_layer_id = 0U;
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
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CREATE_PLANE;
            edit.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
            edit.parent_id = state->panes.active_tree_layer_id;
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
                state->panes.active_tree_plane_id = plane_id;
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
            std::uint32_t destination = state->panes.active_tree_plane_index;
            if (command == IDM_PLANE_MOVE_UP) {
                destination = destination == 0U ? 0U : destination - 1U;
            } else if (command == IDM_PLANE_MOVE_DOWN) {
                const int count = static_cast<int>(state->panes.tree_plane_count);
                destination = static_cast<std::uint32_t>(std::min(
                    std::max(0, count - 1), static_cast<int>(destination) + 1));
            }
            std::uint64_t object_id{};
            const InkpodStatus status = ApplyTreeEdit(
                *state,
                operation,
                state->panes.active_tree_plane_id,
                destination,
                object_id);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"プレーン操作");
            } else if (command == IDM_PLANE_DUPLICATE) {
                state->panes.active_tree_plane_id = object_id;
            } else if (command == IDM_PLANE_DELETE) {
                state->panes.active_tree_plane_id = 0U;
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
            if (state->lifetime.smoke_test) {
                dialog.values[0] = INKPOD_TYPED_PLANE_RASTER;
                dialog.values[1] = INKPOD_STORAGE_RGBA8;
            }
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
            edit.object_id = state->panes.active_tree_plane_id;
            std::uint64_t ignored{};
            const InkpodStatus status = ApplyTreeEditRecord(*state, edit, {}, ignored);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"同種プレーンの統合");
            }
            state->panes.active_tree_plane_id = 0U;
            RefreshTreePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteAnimationCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_LT_SET_NEW:
        case IDM_LT_SET_RENAME: {
            TextInputDialogState dialog{};
            dialog.title = LOWORD(wparam) == IDM_LT_SET_NEW
                ? L"ライトテーブルセットを作成"
                : L"ライトテーブルセット名";
            dialog.label = L"名前";
            dialog.value = state->lifetime.smoke_test ? L"Smoke Set" : L"Light Table";
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            InkpodLightTableEdit edit{};
            edit.operation = LOWORD(wparam) == IDM_LT_SET_NEW
                ? INKPOD_LIGHT_TABLE_CREATE_SET
                : INKPOD_LIGHT_TABLE_RENAME_SET;
            edit.object_id = state->panes.active_light_table_set_id;
            std::uint64_t object_id{};
            const InkpodStatus status = ApplyLightTableEdit(
                *state, edit, dialog.value, object_id);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"ライトテーブルセット");
            } else if (object_id != 0U) {
                state->panes.active_light_table_set_id = object_id;
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
            edit.object_id = state->panes.active_light_table_set_id;
            edit.destination_index = state->panes.active_light_table_set_index;
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
                state->panes.active_light_table_set_id = object_id;
            } else {
                state->panes.active_light_table_set_id = 0U;
            }
            RefreshLightTablePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LT_GLOBAL_OPACITY: {
            ViewOptionsDialogState dialog{};
            dialog.title = L"ライトテーブル全体不透明度";
            dialog.labels[0] = L"不透明度 (0-100%)";
            dialog.values[0] = state->lifetime.smoke_test ? 50 : 100;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
            std::wstring path = state->lifetime.smoke_test ? state->lifetime.smoke_raster_path : L"";
            if (!state->lifetime.smoke_test && !ChooseCommonRasterPath(window, false, path)) {
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
            edit.object_id = state->panes.active_light_table_item_id;
            edit.destination_index = state->panes.active_light_table_item_index;
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
            state->panes.active_light_table_item_id = 0U;
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
            if (state->panes.active_light_table_item_id == 0U) {
                return 0;
            }
            TransitionActiveTool(
                state->tools, state->windows.canvas, kInteractionLightTableMove);
            state->panes.light_table_move_samples.clear();
            UpdateMenuState(*state);
            return 1;
        case IDM_LT_ITEM_SAMPLE: {
            InkpodDocumentInfo document{};
            QueryDocument(*state, document);
            ViewOptionsDialogState dialog{};
            dialog.title = L"ライトテーブル色サンプル";
            dialog.labels = {L"X", L"Y", nullptr, nullptr};
            dialog.values = {
                state->lifetime.smoke_test ? document.reference_frame.x : 0,
                state->lifetime.smoke_test ? document.reference_frame.y : 0,
                0,
                0};
            dialog.value_count = 2U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
            const std::uint64_t item_id = state->panes.active_light_table_item_id;
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
                ResetUiForDocumentReplacement(*state);
                FitCanvas(*state, INKPOD_VIEW_FIT);
            }
            RefreshLightTablePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SEQ_IMPORT: {
            std::vector<std::wstring> paths;
            if (state->lifetime.smoke_test) {
                try {
                    paths = state->lifetime.smoke_sequence_paths;
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
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SEQ_EXPORT: {
            std::wstring path = state->lifetime.smoke_test ? state->lifetime.smoke_raster_path : L"";
            if (!state->lifetime.smoke_test && !ChooseCommonRasterPath(window, true, path)) {
                return 0;
            }
            ViewOptionsDialogState dialog{};
            dialog.title = L"連番書き出し";
            dialog.labels[0] = L"白背景を合成 (0/1)";
            dialog.values[0] = state->lifetime.smoke_test ? 1 : 0;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
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
                ResetUiForDocumentReplacement(*state);
                FitCanvas(*state, INKPOD_VIEW_FIT);
                RefreshSequencePane(*state);
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SEQ_GOTO: {
            ViewOptionsDialogState dialog{};
            dialog.title = L"セル番号で移動";
            dialog.labels[0] = L"セル番号";
            dialog.values[0] = state->lifetime.smoke_test ? 3 : 1;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
                ResetUiForDocumentReplacement(*state);
                state->animation.active_sequence_index = selected;
                FitCanvas(*state, INKPOD_VIEW_FIT);
                RefreshSequencePane(*state);
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SUBPALETTE_SET: {
            const std::uint32_t index = state->animation.active_sequence_index;
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
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
                static_cast<std::int32_t>(state->animation.motion_fps),
                (state->animation.motion_flags & INKPOD_MOTION_FLAG_LOOP) != 0U ? 1 : 0,
                (state->animation.motion_flags & INKPOD_MOTION_FLAG_INCLUDE_SELECTION) != 0U ? 1 : 0,
                (state->animation.motion_flags & INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE) != 0U ? 1 : 0};
            dialog.value_count = 4U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            state->animation.motion_fps = static_cast<std::uint32_t>(dialog.values[0]);
            state->animation.motion_flags = (dialog.values[1] != 0 ? INKPOD_MOTION_FLAG_LOOP : 0U)
                | (dialog.values[2] != 0
                       ? INKPOD_MOTION_FLAG_INCLUDE_SELECTION
                       : 0U)
                | (dialog.values[3] != 0
                       ? INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE
                       : 0U);
            InkpodMotionCheckInput input{
                sizeof(InkpodMotionCheckInput),
                state->animation.motion_fps,
                state->animation.motion_flags};
            InkpodMotionFrame frame{};
            frame.struct_size = sizeof(frame);
            const InkpodStatus status = state->engine->Invoke(
                [input, &frame](InkpodCore* core) {
                    return inkpod_core_motion_check_start(core, &input, &frame);
                },
                false,
                false);
            if (status == INKPOD_STATUS_OK) {
                state->animation.motion_active = true;
                UpdateMotionState(state->animation, frame);
                SetTimer(
                    window,
                    kMotionPlaybackTimer,
                    std::max<UINT>(1U, 1000U / state->animation.motion_fps),
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
                UpdateMotionState(state->animation, frame);
                if (state->animation.motion_paused) {
                    KillTimer(window, kMotionPlaybackTimer);
                } else {
                    SetTimer(
                        window,
                        kMotionPlaybackTimer,
                        std::max<UINT>(1U, 1000U / state->animation.motion_fps),
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
                UpdateMotionState(state->animation, frame);
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
                UpdateMotionState(state->animation, frame);
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
            state->animation.motion_fps = command == IDM_MOTION_FPS_30
                ? 30U
                : (command == IDM_MOTION_FPS_25
                          ? 25U
                          : (command == IDM_MOTION_FPS_24
                                    ? 24U
                                    : (command == IDM_MOTION_FPS_12
                                              ? 12U
                                              : (command == IDM_MOTION_FPS_10 ? 10U : 8U))));
            if (!state->animation.motion_active) {
                UpdateMenuState(*state);
                return 1;
            }
            const InkpodMotionCheckInput input{
                sizeof(InkpodMotionCheckInput),
                state->animation.motion_fps,
                state->animation.motion_flags};
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
                UpdateMotionState(state->animation, frame);
                SetTimer(
                    window,
                    kMotionPlaybackTimer,
                    std::max<UINT>(1U, 1000U / state->animation.motion_fps),
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
            state->animation.motion_active = false;
            state->animation.motion_paused = false;
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"モーション停止");
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteSelectionViewCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_SELECTION_RECTANGLE:
        case IDM_SELECTION_ELLIPSE:
        case IDM_SELECTION_LASSO:
        case IDM_SELECTION_POLYLINE:
        case IDM_SELECTION_TRACE:
        case IDM_SELECTION_WAND: {
            const UINT command = LOWORD(wparam);
            state->tools.selection_shape = command == IDM_SELECTION_ELLIPSE
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
            TransitionActiveTool(
                state->tools, state->windows.canvas, kInteractionSelection);
            CancelSelectionGeometryPreview(state->tools, state->windows.canvas);
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
                    ? state->tools.selection_tolerance
                    : static_cast<std::int32_t>(state->tools.selection_diameter);
                dialog.values[1] = state->tools.selection_gap_close;
                dialog.value_count = command == IDM_SELECTION_WAND ? 2U : 1U;
                if (ShowViewOptions(
                        state->lifetime.instance,
                        window,
                        state->lifetime.smoke_test,
                        dialog) != IDOK) {
                    return 0;
                }
                if (command == IDM_SELECTION_WAND) {
                    if (dialog.values[0] < 0 || dialog.values[0] > UINT16_MAX
                        || dialog.values[1] < 0
                        || dialog.values[1] > UINT16_MAX) {
                        return 0;
                    }
                    state->tools.selection_tolerance =
                        static_cast<std::uint16_t>(dialog.values[0]);
                    state->tools.selection_gap_close =
                        static_cast<std::uint16_t>(dialog.values[1]);
                } else if (dialog.values[0] > 0) {
                    state->tools.selection_diameter =
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
            state->tools.selection_operation = LOWORD(wparam) == IDM_SELECTION_MODE_ADD
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
                state->document.selection_layer_id = layer_id;
                state->document.smoke_layer_id = layer_id;
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
            const std::uint64_t layer_id = state->document.selection_layer_id;
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
                dialog.values[0] = state->lifetime.smoke_test ? 2 : 1;
                if (ShowViewOptions(
                        state->lifetime.instance,
                        window,
                        state->lifetime.smoke_test,
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
            GetClientRect(state->windows.canvas, &client);
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
            dialog.values[0] = state->lifetime.smoke_test
                ? 125
                : (QuerySnapshotTransform(*state, transform)
                ? static_cast<std::int32_t>(std::clamp(
                      std::lround(transform.zoom * 100.0), 1L, 6400L))
                : 100);
            if (ShowViewOptions(
                    state->lifetime.instance,
                    window,
                    state->lifetime.smoke_test,
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
            TransitionActiveTool(
                state->tools, state->windows.canvas, kInteractionBoxZoom);
            state->view.gesture_samples.clear();
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
                state->view.flip_horizontal =
                    !state->view.flip_horizontal;
            } else {
                state->view.flip_vertical = !state->view.flip_vertical;
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
                ? &state->view.ruler_visible
                : (command == IDM_VIEW_GUIDES
                          ? &state->view.guides_visible
                          : (command == IDM_VIEW_GRID
                                    ? &state->view.grid_visible
                                    : (command == IDM_VIEW_SNAP_GUIDES
                                              ? &state->view.snap_guides
                                              : (command == IDM_VIEW_SNAP_GRID
                                                        ? &state->view.snap_grid
                                                        : &state->view.transparent_visible))));
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
                    state->lifetime.instance,
                    window,
                    state->lifetime.smoke_test,
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
            TransitionActiveTool(
                state->tools, state->windows.canvas, kInteractionGuideMove);
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
                    state->lifetime.instance,
                    window,
                    state->lifetime.smoke_test,
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
                state->view.secondary_view_id = view_id;
                state->view.active_view_id = view_id;
                if (state->windows.document_tabs != nullptr) {
                    TCITEMW item{};
                    item.mask = TCIF_TEXT | TCIF_PARAM;
                    item.pszText = const_cast<wchar_t*>(L"無題セル 1 [ビュー 2]");
                    item.lParam = static_cast<LPARAM>(view_id);
                    const int index = TabCtrl_GetItemCount(state->windows.document_tabs);
                    if (TabCtrl_InsertItem(state->windows.document_tabs, index, &item) >= 0) {
                        TabCtrl_SetCurSel(state->windows.document_tabs, index);
                    }
                }
                state->engine->SetActiveView(view_id);
                state->view.flip_horizontal = false;
                state->view.flip_vertical = false;
            }
            UpdateMenuState(*state);
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteToolCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_TOOL_PALETTE:
            if (state->tools.palette != nullptr) {
                state->windows.workspace.tool_visible =
                    !state->windows.workspace.tool_visible;
                RelayoutWorkspace(*state);
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_WINDOW_TOOL_OPTIONS:
            state->windows.workspace.tool_options_visible =
                !state->windows.workspace.tool_options_visible;
            RelayoutWorkspace(*state);
            UpdateMenuState(*state);
            return 1;
        case IDM_TOOL_PENCIL:
            TransitionActiveTool(
                state->tools, state->windows.canvas, INKPOD_TOOL_PENCIL);
            UpdateMenuState(*state);
            return 0;
        case IDM_TOOL_BRUSH:
            TransitionActiveTool(
                state->tools, state->windows.canvas, INKPOD_TOOL_BRUSH);
            UpdateMenuState(*state);
            return 0;
        case IDM_TOOL_ERASER:
            TransitionActiveTool(
                state->tools, state->windows.canvas, INKPOD_TOOL_ERASER);
            UpdateMenuState(*state);
            return 0;
        case IDM_TOOL_FILL:
        case IDM_TOOL_CLOSED_FILL:
        case IDM_TOOL_FILL_EXTENSION:
            TransitionActiveTool(
                state->tools, state->windows.canvas, kInteractionFill);
            state->tools.fill_options.operation = LOWORD(wparam) == IDM_TOOL_CLOSED_FILL
                ? INKPOD_FILL_CLOSED_REGION
                : (LOWORD(wparam) == IDM_TOOL_FILL_EXTENSION
                          ? INKPOD_FILL_EXTENSION
                          : INKPOD_FILL_SEED);
            state->tools.fill_gesture_samples.clear();
            UpdateMenuState(*state);
            return 0;
        case IDM_TOOL_FILL_OPTIONS:
            if (inkpod::windows::ui::ShowFillOptions(
                    state->lifetime.instance,
                    state->windows.window,
                    state->lifetime.smoke_test,
                    state->tools.fill_options)) {
                TransitionActiveTool(
                    state->tools, state->windows.canvas, kInteractionFill);
                state->tools.fill_gesture_samples.clear();
                UpdateMenuState(*state);
            }
            return 0;
        case IDM_TOOL_EYEDROPPER:
            TransitionActiveTool(
                state->tools, state->windows.canvas, kInteractionEyedropper);
            UpdateMenuState(*state);
            return 0;
        case IDM_VECTOR_LINE:
        case IDM_VECTOR_CURVE:
        case IDM_VECTOR_RECTANGLE:
        case IDM_VECTOR_ELLIPSE:
        case IDM_VECTOR_POLYLINE:
        case IDM_VECTOR_ERASER: {
            CancelVectorGeometryPreview(state->tools, state->windows.canvas);
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
            const std::uint32_t next_tool = command == IDM_VECTOR_LINE
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
            TransitionActiveTool(
                state->tools, state->windows.canvas, next_tool);
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_VECTOR_ERASE_PARTIAL:
        case IDM_VECTOR_ERASE_INTERSECTION:
        case IDM_VECTOR_ERASE_WHOLE:
            state->tools.vector_erase_mode = LOWORD(wparam) == IDM_VECTOR_ERASE_INTERSECTION
                ? INKPOD_VECTOR_ERASE_TO_INTERSECTION
                : (LOWORD(wparam) == IDM_VECTOR_ERASE_WHOLE
                          ? INKPOD_VECTOR_ERASE_WHOLE_PATH
                          : INKPOD_VECTOR_ERASE_PARTIAL);
            TransitionActiveTool(
                state->tools, state->windows.canvas, kInteractionVectorEraser);
            UpdateMenuState(*state);
            return 1;
        case IDM_VECTOR_CONNECT: {
            ViewOptionsDialogState dialog{};
            dialog.title = L"ベクター線つなぎ";
            dialog.labels[0] = L"最大隙間 (1/1000 px)";
            dialog.values[0] = state->lifetime.smoke_test ? 4000 : 2000;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
            dialog.values = {1, state->lifetime.smoke_test ? 500 : 1000, 0, 0};
            dialog.value_count = 2U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
            state->tools.vector_selection_mode = command == IDM_VECTOR_SELECT_CUT
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
                state->panes.active_tree_layer_id = layer_id;
                state->panes.active_tree_plane_id = 0U;
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
                state->panes.active_tree_layer_id = target_layer_id;
                state->panes.active_tree_plane_id = 0U;
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
            if (state->tools.active_plane == INKPOD_PLANE_COLOR
                && ConfigureCanvasEffect(*state, LOWORD(wparam))) {
                UpdateMenuState(*state);
            }
            return 0;
        case IDM_EFFECT_ALPHA_VIEW: {
            const bool enabled = !state->effects.alpha_view;
            const InkpodStatus status = ApplyView(
                *state,
                INKPOD_VIEW_SET_ALPHA_VISIBLE,
                enabled ? 1.0 : 0.0,
                0.0);
            if (status == INKPOD_STATUS_OK) {
                state->effects.alpha_view = enabled;
            } else {
                ShowCoreError(*state, window, L"アルファ表示");
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_PLANE_MAIN_LINE:
        case IDM_PLANE_COLOR: {
            const InkpodPlaneKind plane = LOWORD(wparam) == IDM_PLANE_MAIN_LINE
                ? INKPOD_PLANE_MAIN_LINE
                : INKPOD_PLANE_COLOR;
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
            } else {
                state->tools.active_plane = plane;
                InkpodDocumentInfo info = EmptyDocumentInfo();
                if (QueryDocument(*state, info)) {
                    state->panes.active_tree_layer_id = info.layer_id;
                    state->panes.active_tree_plane_id =
                        plane == INKPOD_PLANE_MAIN_LINE
                        ? info.main_plane_id
                        : info.color_plane_id;
                    RefreshTreePane(*state);
                }
            }
            UpdateMenuState(*state);
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteColorCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_COLOR_PANE:
            state->windows.workspace.color_visible =
                !state->windows.workspace.color_visible;
            if (state->windows.workspace.color_visible) {
                RefreshColorPanes(*state);
            }
            RelayoutWorkspace(*state);
            RefreshDockPaneViews(*state);
            UpdateMenuState(*state);
            return 1;
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
            state->tools.eyedropper_source = LOWORD(wparam) == IDM_COLOR_SOURCE_TOPMOST
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
                colors = state->panes.palette_colors;
                names = state->panes.color_chart_names;
                if (LOWORD(wparam) == IDM_PALETTE_REGISTER) {
                    if (colors.size() >= 4096U) {
                        return 0;
                    }
                    colors.push_back(state->tools.drawing_color);
                    names.push_back(L"Color " + std::to_wstring(colors.size()));
                    state->panes.selected_palette_index =
                        static_cast<std::uint32_t>(colors.size() - 1U);
                    state->panes.palette_group =
                        state->panes.selected_palette_index / 10U;
                } else if (LOWORD(wparam) == IDM_PALETTE_DELETE) {
                    const std::size_t index = state->panes.selected_palette_index;
                    if (index >= colors.size()) {
                        return 0;
                    }
                    colors.erase(colors.begin() + static_cast<std::ptrdiff_t>(index));
                    names.erase(names.begin() + static_cast<std::ptrdiff_t>(index));
                } else {
                    colors.clear();
                    names.clear();
                    state->panes.selected_palette_index = 0U;
                    state->panes.selected_color_chart_index = 0U;
                }
            } catch (const std::bad_alloc&) {
                return 0;
            }
            const InkpodStatus status = ReplacePalette(*state, colors);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, L"カラーパレット編集");
            } else {
                state->panes.color_chart_names = std::move(names);
            }
            RefreshColorPanes(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_PALETTE_NEXT_GROUP:
            ++state->panes.palette_group;
            RefreshColorPanes(*state);
            if (!state->panes.palette_colors.empty()) {
                state->panes.selected_palette_index = std::min<std::uint32_t>(
                    state->panes.palette_group * 10U,
                    static_cast<std::uint32_t>(state->panes.palette_colors.size() - 1U));
            }
            return 1;
        case IDM_PALETTE_SAVE:
        case IDM_PALETTE_LOAD: {
            const bool save = LOWORD(wparam) == IDM_PALETTE_SAVE;
            std::wstring path = state->lifetime.smoke_test ? L"inkpod-palette-smoke.inkpalette" : L"";
            if (!state->lifetime.smoke_test && !ChoosePalettePath(window, save, path)) {
                return 0;
            }
            if (save) {
                return SavePaletteFile(path, state->panes.palette_colors) ? 1 : 0;
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
            if (state->lifetime.smoke_test) {
                dialog.values = {16, 4, 1, 0};
            }
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
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
            state->panes.color_chart_names.clear();
            state->panes.color_chart_page = 0U;
            state->panes.selected_color_chart_index = 0U;
            RefreshColorPanes(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_SEARCH: {
            TextInputDialogState dialog{};
            dialog.title = L"カラーチャート検索";
            dialog.label = L"名前または番号";
            dialog.value = state->lifetime.smoke_test ? L"Smoke" : L"";
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            wchar_t* end{};
            const unsigned long number = std::wcstoul(dialog.value.c_str(), &end, 10);
            std::size_t index = SIZE_MAX;
            if (end != dialog.value.c_str() && *end == L'\0' && number != 0U
                && number <= state->panes.palette_colors.size()) {
                index = number - 1U;
            } else {
                std::wstring needle = dialog.value;
                std::transform(
                    needle.begin(), needle.end(), needle.begin(),
                    [](wchar_t value) { return std::towlower(value); });
                for (std::size_t candidate = 0U;
                     candidate < state->panes.color_chart_names.size(); ++candidate) {
                    std::wstring name = state->panes.color_chart_names[candidate];
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
            state->panes.color_chart_page = static_cast<std::uint32_t>(index / 20U);
            state->panes.selected_color_chart_index = static_cast<std::uint32_t>(index);
            RefreshColorPanes(*state);
            return 1;
        }
        case IDM_CHART_NEXT: {
            if (state->panes.palette_colors.empty()) {
                return 0;
            }
            std::size_t index = static_cast<std::size_t>(
                state->panes.selected_color_chart_index) + 1U;
            index %= state->panes.palette_colors.size();
            state->panes.color_chart_page = static_cast<std::uint32_t>(index / 20U);
            state->panes.selected_color_chart_index = static_cast<std::uint32_t>(index);
            RefreshColorPanes(*state);
            return 1;
        }
        case IDM_CHART_LOCK:
            state->panes.color_chart_locked = !state->panes.color_chart_locked;
            UpdateMenuState(*state);
            return 1;
        case IDM_CHART_NEXT_PAGE:
            ++state->panes.color_chart_page;
            RefreshColorPanes(*state);
            if (!state->panes.palette_colors.empty()) {
                state->panes.selected_color_chart_index = std::min<std::uint32_t>(
                    state->panes.color_chart_page * 20U,
                    static_cast<std::uint32_t>(state->panes.palette_colors.size() - 1U));
            }
            return 1;
        case IDM_CHART_RENAME: {
            if (state->panes.color_chart_locked) {
                return 0;
            }
            const std::size_t index = state->panes.selected_color_chart_index;
            if (index >= state->panes.color_chart_names.size()) {
                return 0;
            }
            TextInputDialogState dialog{};
            dialog.title = L"カラーチャート名";
            dialog.label = L"名前";
            dialog.value = state->lifetime.smoke_test ? L"Smoke Color" : state->panes.color_chart_names[index];
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.value.empty() || dialog.value.size() > 256U) {
                return 0;
            }
            try {
                state->panes.color_chart_names[index] = dialog.value;
            } catch (const std::bad_alloc&) {
                return 0;
            }
            RefreshColorPanes(*state);
            return 1;
        }
        case IDM_CHART_SAVE:
        case IDM_CHART_LOAD: {
            const bool save = LOWORD(wparam) == IDM_CHART_SAVE;
            std::wstring path = state->lifetime.smoke_test ? L"inkpod-chart-smoke.inkchart" : L"";
            if (!state->lifetime.smoke_test && !ChooseChartPath(window, save, path)) {
                return 0;
            }
            if (save) {
                return SaveColorChartFile(
                           path, state->panes.palette_colors, state->panes.color_chart_names)
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
                    state->panes.color_chart_names = std::move(names);
                } catch (const std::bad_alloc&) {
                    return 0;
                }
                state->panes.color_chart_page = 0U;
                state->panes.selected_color_chart_index = 0U;
                RefreshColorPanes(*state);
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_COPY: {
            const std::size_t index = state->panes.selected_color_chart_index;
            if (index >= state->panes.palette_colors.size()) {
                return 0;
            }
            const auto& selected_color = state->panes.palette_colors[index];
            std::array<wchar_t, 384U> text{};
            _snwprintf_s(
                text.data(), text.size(), _TRUNCATE,
                selected_color.depth == INKPOD_COLOR_DEPTH_16
                    ? L"%ls\t#%04X%04X%04X%04X"
                    : L"%ls\t#%02X%02X%02X%02X",
                state->panes.color_chart_names[index].c_str(),
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
            if (state->panes.color_chart_locked
                || SendMessageW(window, WM_COMMAND, IDM_CHART_COPY, 0) != 1) {
                return 0;
            }
            const std::size_t index = state->panes.selected_color_chart_index;
            if (index >= state->panes.palette_colors.size()) {
                return 0;
            }
            std::vector<InkpodColorValue> colors = state->panes.palette_colors;
            std::vector<std::wstring> names = state->panes.color_chart_names;
            colors.erase(colors.begin() + static_cast<std::ptrdiff_t>(index));
            names.erase(names.begin() + static_cast<std::ptrdiff_t>(index));
            const InkpodStatus status = ReplacePalette(*state, colors);
            if (status == INKPOD_STATUS_OK) {
                state->panes.color_chart_names = std::move(names);
                RefreshColorPanes(*state);
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_PASTE: {
            if (state->panes.color_chart_locked || OpenClipboard(window) == FALSE) {
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
            std::vector<InkpodColorValue> colors = state->panes.palette_colors;
            if (colors.size() >= 4096U) {
                return 0;
            }
            colors.push_back(color);
            const InkpodStatus status = ReplacePalette(*state, colors);
            if (status == INKPOD_STATUS_OK) {
                RefreshColorPanes(*state);
                state->panes.color_chart_names.back() = pasted_name.empty()
                    ? L"Pasted"
                    : pasted_name;
                state->panes.color_chart_page = static_cast<std::uint32_t>(
                    (state->panes.palette_colors.size() - 1U) / 20U);
                state->panes.selected_color_chart_index = static_cast<std::uint32_t>(
                    state->panes.palette_colors.size() - 1U);
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
                (state->tools.color_rgba >> 24) & 0xffU,
                (state->tools.color_rgba >> 16) & 0xffU,
                (state->tools.color_rgba >> 8) & 0xffU);
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
                        static_cast<std::uint16_t>(state->tools.color_rgba & 0xffU)});
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
                state->view.color_check_mode = mode;
            }
            UpdateMenuState(*state);
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteApplicationCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WORKSPACE_RESET:
            ResetWorkspaceLayout(state->windows.workspace);
            RelayoutWorkspace(*state);
            RefreshTreePane(*state);
            RefreshDockPaneViews(*state);
            UpdateMenuState(*state);
            return 1;
        case IDM_WORKSPACE_SAVE: {
            const bool saved = SaveWorkspaceLayout(
                state->windows.workspace, L"WorkspaceSavedV2");
            if (!saved && !state->lifetime.smoke_test) {
                MessageBoxW(
                    window,
                    L"現在のワークスペース配置を保存できませんでした。",
                    L"inkpod",
                    MB_OK | MB_ICONWARNING);
            }
            return saved ? 1 : 0;
        }
        case IDM_WORKSPACE_RESTORE: {
            WorkspaceLayoutState restored = state->windows.workspace;
            const bool loaded = LoadWorkspaceLayout(
                restored, L"WorkspaceSavedV2");
            if (loaded) {
                state->windows.workspace = restored;
                RelayoutWorkspace(*state);
                RefreshTreePane(*state);
                RefreshDockPaneViews(*state);
                UpdateMenuState(*state);
            } else if (!state->lifetime.smoke_test) {
                MessageBoxW(
                    window,
                    L"保存されたワークスペース配置がありません。",
                    L"inkpod",
                    MB_OK | MB_ICONINFORMATION);
            }
            return loaded ? 1 : 0;
        }
        case IDM_WORKSPACE_MIRROR:
            state->windows.workspace.mirrored =
                !state->windows.workspace.mirrored;
            RelayoutWorkspace(*state);
            UpdateMenuState(*state);
            return 1;
        case IDM_SHORTCUT_RESET: {
            const InkpodStatus status = state->engine == nullptr
                ? INKPOD_STATUS_INVALID_STATE
                : ResetShortcuts(
                      *state->engine, state->shortcuts, !state->lifetime.smoke_test);
            if (status != INKPOD_STATUS_OK) {
                ShowShortcutError(
                    *state, window, L"ショートカットの初期化", status);
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_SHORTCUT_EDIT: {
            ShortcutDialogState dialog_state{};
            try {
                dialog_state.entries.reserve(state->shortcuts.bindings.size());
                const HMENU menu = GetMenu(window);
                for (const auto& binding : state->shortcuts.bindings) {
                    dialog_state.entries.push_back({
                        binding.command_id,
                        MenuCommandDisplayName(menu, binding.command_id),
                        binding});
                }
            } catch (const std::bad_alloc&) {
                ShowCoreError(*state, window, L"ショートカット一覧の作成");
                return 0;
            }
            if (ShowShortcutEditor(
                    state->lifetime.instance,
                    window,
                    state->lifetime.smoke_test,
                    dialog_state) != IDOK) {
                return 0;
            }
            const InkpodStatus status = state->engine == nullptr
                ? INKPOD_STATUS_INVALID_STATE
                : RebindShortcut(
                      *state->engine,
                      state->shortcuts,
                      dialog_state.sequence,
                      !state->lifetime.smoke_test);
            if (status != INKPOD_STATUS_OK) {
                ShowShortcutError(*state, window, L"ショートカット編集", status);
                if (status == INKPOD_STATUS_IO_ERROR) {
                    UpdateMenuState(*state);
                    return 1;
                }
                return 0;
            }
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_HELP_ABOUT:
            return ShowAboutDialog(
                       state->lifetime.instance,
                       window,
                       state->lifetime.smoke_test)
                    == IDOK
                ? 1
                : 0;
        case IDM_APP_EXIT:
            SendMessageW(window, WM_CLOSE, 0, 0);
            return 0;
        default:
            break;
    }
    return std::nullopt;
}

std::optional<LRESULT> RouteMainWindowCommand(
    AppContext* state, HWND window, WPARAM wparam, LPARAM lparam) noexcept {
    using CommandRoute = std::optional<LRESULT> (*)(
        AppContext*, HWND, WPARAM, LPARAM) noexcept;
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
        if (const auto result = route(state, window, wparam, lparam)) {
            return result;
        }
    }
    return std::nullopt;
}

std::optional<LRESULT> RouteWindowLifecycleMessage(
    AppContext* state,
    HWND window,
    UINT message,
    WPARAM,
    LPARAM lparam) noexcept {
    switch (message) {
        case WM_CREATE:
            if (state == nullptr) {
                return -1;
            }
            state->windows.canvas = inkpod::renderer::CreateCanvasWindow(
                state->lifetime.instance, window);
            if (state->windows.canvas == nullptr) {
                return -1;
            }
            if (!InitializeMainChrome(*state)) {
                return -1;
            }
            if (SetTimer(
                    window,
                    kAutosaveTimer,
                    kAutosaveIntervalMilliseconds,
                    nullptr) == 0U) {
                return -1;
            }
            return 0;
        case WM_SIZE:
            if (state != nullptr) {
                inkpod::windows::ui::LayoutMainChrome(
                    state->windows,
                    state->lifetime.smoke_test,
                    LOWORD(lparam),
                    HIWORD(lparam));
            }
            return 0;
        case WM_NOTIFY:
            if (state != nullptr && state->windows.document_tabs != nullptr) {
                const auto* notification = reinterpret_cast<const NMHDR*>(lparam);
                if (notification != nullptr
                    && notification->hwndFrom == state->windows.document_tabs
                    && notification->code == TCN_SELCHANGE) {
                    const int selected = TabCtrl_GetCurSel(state->windows.document_tabs);
                    TCITEMW item{};
                    item.mask = TCIF_PARAM;
                    if (selected >= 0
                        && TabCtrl_GetItem(state->windows.document_tabs, selected, &item) != FALSE) {
                        state->view.active_view_id = static_cast<std::uint64_t>(item.lParam);
                        if (state->engine != nullptr) {
                            state->engine->SetActiveView(state->view.active_view_id);
                        }
                        InkpodSnapshotTransform transform{};
                        if (QuerySnapshotTransform(*state, transform)) {
                            state->view.flip_horizontal = (transform.flags
                                & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) != 0U;
                            state->view.flip_vertical = (transform.flags
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
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteKeyboardMessage(
    AppContext* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    switch (message) {
        case WM_KEYDOWN:
        case WM_SYSKEYDOWN:
            if (state != nullptr) {
                if (!state->shortcuts.pending_strokes.empty() && wparam == VK_ESCAPE) {
                    ClearPendingShortcut(state->shortcuts);
                    KillTimer(window, kShortcutSequenceTimer);
                    UpdateMenuState(*state);
                    return 0;
                }
                if (state->tools.floating_active && wparam == VK_RETURN) {
                    DispatchEnabledCommand(*state, window, IDM_EDIT_FLOATING_COMMIT);
                    return 0;
                }
                if (state->tools.floating_active && wparam == VK_ESCAPE) {
                    DispatchEnabledCommand(*state, window, IDM_EDIT_FLOATING_CANCEL);
                    return 0;
                }
                if (state->animation.motion_active && wparam == VK_ESCAPE) {
                    DispatchEnabledCommand(*state, window, IDM_MOTION_STOP);
                    return 0;
                }
                if (state->animation.motion_active && wparam == VK_SPACE) {
                    DispatchEnabledCommand(*state, window, IDM_MOTION_PAUSE);
                    return 0;
                }
                if (state->animation.motion_active
                    && (wparam == VK_LEFT || wparam == VK_RIGHT
                        || wparam == VK_HOME || wparam == VK_END)) {
                    const UINT command = wparam == VK_LEFT
                        ? IDM_MOTION_PREVIOUS
                        : (wparam == VK_RIGHT
                                  ? IDM_MOTION_NEXT
                                  : (wparam == VK_HOME ? IDM_MOTION_FIRST : IDM_MOTION_LAST));
                    DispatchEnabledCommand(*state, window, command);
                    return 0;
                }
                const std::uint32_t modifiers = CurrentShortcutModifiers(lparam);
                UINT menu_command{};
                const InkpodShortcutMatch shortcut_match = ResolveShortcutStroke(
                    state->shortcuts,
                    InkpodShortcutStroke{static_cast<std::uint32_t>(wparam), modifiers},
                    menu_command);
                if (shortcut_match == INKPOD_SHORTCUT_MATCH_PREFIX) {
                    SetTimer(
                        window,
                        kShortcutSequenceTimer,
                        kShortcutSequenceTimerMilliseconds,
                        nullptr);
                    UpdateMenuState(*state);
                    return 0;
                }
                KillTimer(window, kShortcutSequenceTimer);
                if (shortcut_match == INKPOD_SHORTCUT_MATCH_EXACT) {
                    const UINT resolved_command = ShortcutMenuCommand(menu_command);
                    if (resolved_command != 0U) {
                        DispatchEnabledCommand(*state, window, resolved_command);
                    }
                    return 0;
                }
                if (modifiers == 0U && wparam >= '0' && wparam <= '9') {
                    const std::size_t digit = wparam == '0'
                        ? 9U
                        : static_cast<std::size_t>(wparam - '1');
                    const std::size_t index = state->panes.palette_group * 10U + digit;
                    if (index < state->panes.palette_colors.size()) {
                        state->panes.selected_palette_index =
                            static_cast<std::uint32_t>(index);
                        SetDrawingColor(*state, state->panes.palette_colors[index]);
                        InvalidateRect(state->windows.canvas, nullptr, FALSE);
                    }
                    return 0;
                }
            }
            break;
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteCanvasMessage(
    AppContext* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    switch (message) {
        case inkpod::renderer::kCanvasStrokeReady:
            if (state != nullptr) {
                const auto* input = reinterpret_cast<
                    const inkpod::renderer::CanvasStrokeEvent*>(lparam);
                if (input == nullptr || state->engine == nullptr
                    || input->sample_count > UINT64_C(1048576)
                    || (input->sample_count != 0U && input->samples == nullptr)) {
                    return 0;
                }
                if (state->view.guide_drag_active) {
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->view.guide_drag_active = false;
                        state->view.guide_drag_axis = 0U;
                        state->view.guide_drag_id = 0U;
                    } else if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End
                        && input->sample_count != 0U) {
                        const InkpodStatus status = FinishGuideDrag(
                            *state,
                            input->samples[static_cast<std::size_t>(
                                input->sample_count - 1U)]);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
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
                if (state->tools.active_tool == kInteractionGuideMove) {
                    return 1;
                }
                if (state->tools.active_tool == kInteractionBoxZoom) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->view.gesture_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->view.gesture_samples.insert(
                                state->view.gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->view.gesture_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->view.gesture_samples.clear();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = state->view.gesture_samples.size() < 2U
                            ? INKPOD_STATUS_INVALID_ARGUMENT
                            : ApplyBoxZoomGesture(
                                  *state,
                                  state->view.gesture_samples.front(),
                                  state->view.gesture_samples.back());
                        state->view.gesture_samples.clear();
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"範囲拡大");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (state->tools.active_tool == kInteractionEyedropper
                    && input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                    && input->sample_count != 0U) {
                    const InkpodStatus status = EyedropAtDevicePoint(
                        *state, input->samples[0].x, input->samples[0].y);
                    if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                        ShowCoreError(*state, window, L"スポイト");
                    }
                    UpdateMenuState(*state);
                    return 1;
                }
                if (state->tools.active_tool == kInteractionEyedropper) {
                    return 1;
                }
                if (state->tools.active_tool == kInteractionFloatingTransform) {
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        if (!state->tools.floating_gesture_samples.empty()) {
                            SetFloatingTransform(*state, state->tools.floating_drag_start);
                        }
                        state->tools.floating_gesture_samples.clear();
                        state->tools.floating_drag_mode = 0U;
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                        state->tools.floating_gesture_samples.clear();
                    }
                    if (input->sample_count != 0U) {
                        try {
                            state->tools.floating_gesture_samples.push_back(
                                input->samples[static_cast<std::size_t>(input->sample_count - 1U)]);
                        } catch (const std::bad_alloc&) {
                            state->tools.floating_gesture_samples.clear();
                            return 0;
                        }
                    }
                    if (!state->tools.floating_gesture_samples.empty()) {
                        const InkpodStatus status = UpdateFloatingHandleDrag(
                            *state,
                            state->tools.floating_gesture_samples.front(),
                            state->tools.floating_gesture_samples.back(),
                            input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"フローティングハンドル変形");
                        }
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        state->tools.floating_gesture_samples.clear();
                        state->tools.floating_drag_mode = 0U;
                    }
                    return 1;
                }
                if (state->tools.active_tool == kInteractionLightTableMove) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->panes.light_table_move_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->panes.light_table_move_samples.push_back(
                                input->samples[static_cast<std::size_t>(input->sample_count - 1U)]);
                        }
                    } catch (const std::bad_alloc&) {
                        state->panes.light_table_move_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->panes.light_table_move_samples.clear();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = MoveLightTableFromCanvas(*state);
                        state->panes.light_table_move_samples.clear();
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"ライトテーブル移動");
                        }
                        RefreshLightTablePane(*state);
                    }
                    return 1;
                }
                if (IsVectorCanvasTool(state->tools.active_tool)) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            CancelVectorGeometryPreview(state->tools, state->windows.canvas);
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->tools.vector_gesture_samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                CancelVectorGeometryPreview(state->tools, state->windows.canvas);
                                return 0;
                            }
                            state->tools.vector_gesture_samples.insert(
                                state->tools.vector_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        CancelVectorGeometryPreview(state->tools, state->windows.canvas);
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        CancelVectorGeometryPreview(state->tools, state->windows.canvas);
                        return 1;
                    }
                    if (state->tools.active_tool != kInteractionVectorEraser) {
                        UpdateVectorGeometryPreview(*state);
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = FinishVectorCanvasGesture(*state);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"ベクターCanvas操作");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (state->tools.active_tool == kInteractionSelection) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            CancelSelectionGeometryPreview(
                                state->tools, state->windows.canvas);
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->tools.selection_gesture_samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                CancelSelectionGeometryPreview(
                                    state->tools, state->windows.canvas);
                                return 0;
                            }
                            state->tools.selection_gesture_samples.insert(
                                state->tools.selection_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        CancelSelectionGeometryPreview(
                            state->tools, state->windows.canvas);
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        CancelSelectionGeometryPreview(
                            state->tools, state->windows.canvas);
                        return 1;
                    }
                    if (state->tools.selection_shape == INKPOD_SELECTION_WAND) {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            const InkpodStatus status = ApplySelectionGesture(
                                *state, state->tools.selection_gesture_samples);
                            CancelSelectionGeometryPreview(
                                state->tools, state->windows.canvas);
                            if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                                ShowCoreError(*state, window, L"色の杖");
                            }
                            UpdateMenuState(*state);
                        }
                        return 1;
                    }
                    UpdateSelectionGeometryPreview(*state);
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = ApplySelectionGesture(
                            *state, state->tools.selection_gesture_samples);
                        CancelSelectionGeometryPreview(
                            state->tools, state->windows.canvas);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"選択範囲");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (state->tools.active_tool == kInteractionFill) {
                    if (state->tools.fill_options.operation == INKPOD_FILL_SEED) {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                            && input->sample_count != 0U) {
                            const InkpodStatus status = ApplyFillAtDevicePoint(
                                *state, input->samples[0].x, input->samples[0].y);
                            if (status != INKPOD_STATUS_OK
                                && status != INKPOD_STATUS_FILL_OVERFLOW
                                && !state->lifetime.smoke_test) {
                                ShowCoreError(*state, window, L"フィル");
                            }
                            UpdateMenuState(*state);
                        }
                        return 1;
                    }
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->tools.fill_gesture_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->tools.fill_gesture_samples.insert(
                                state->tools.fill_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->tools.fill_gesture_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->tools.fill_gesture_samples.clear();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = state->tools.fill_gesture_samples.size() < 2U
                            ? INKPOD_STATUS_INVALID_ARGUMENT
                            : ApplyFillAtDeviceRange(
                                  *state,
                                  state->tools.fill_gesture_samples.front().x,
                                  state->tools.fill_gesture_samples.front().y,
                                  state->tools.fill_gesture_samples.back().x,
                                  state->tools.fill_gesture_samples.back().y,
                                  true);
                        state->tools.fill_gesture_samples.clear();
                        if (status != INKPOD_STATUS_OK
                            && status != INKPOD_STATUS_FILL_OVERFLOW
                            && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"範囲フィル");
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                const bool effect_interaction = state->tools.active_tool >= kInteractionEffectGradient
                    && state->tools.active_tool <= kInteractionEffectAlphaGradient;
                if (effect_interaction) {
                    if (state->tools.active_tool == kInteractionEffectStamp
                        && input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                        && input->sample_count != 0U
                        && (GetKeyState(VK_MENU) & 0x8000) != 0) {
                        state->effects.stamp_source = input->samples[0];
                        state->effects.stamp_source_valid = true;
                        state->effects.samples.clear();
                        return 1;
                    }
                    if (state->effects.task != nullptr) {
                        return 1;
                    }
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->effects.samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->effects.samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                state->effects.samples.clear();
                                state->effects.airbrush_active = false;
                                KillTimer(window, kContinuousSprayTimer);
                                return 0;
                            }
                            state->effects.samples.insert(
                                state->effects.samples.end(),
                                input->samples,
                                input->samples + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->effects.samples.clear();
                        state->effects.airbrush_active = false;
                        KillTimer(window, kContinuousSprayTimer);
                        return 0;
                    }
                    if (state->tools.active_tool == kInteractionEffectAirbrush
                        && input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                        && input->sample_count != 0U) {
                        state->effects.airbrush_last =
                            input->samples[static_cast<std::size_t>(input->sample_count - 1U)];
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->effects.airbrush_active = true;
                            SetTimer(
                                window,
                                kContinuousSprayTimer,
                                kContinuousSprayIntervalMilliseconds,
                                nullptr);
                        }
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->effects.samples.clear();
                        state->effects.airbrush_active = false;
                        KillTimer(window, kContinuousSprayTimer);
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        state->effects.airbrush_active = false;
                        KillTimer(window, kContinuousSprayTimer);
                        const InkpodStatus status = FinishEffectGesture(*state);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"Canvas効果");
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
                    static_cast<InkpodPaintTool>(state->tools.active_tool),
                    state->tools.active_plane,
                    INKPOD_COORDINATE_SPACE_DEVICE,
                    state->tools.active_tool == INKPOD_TOOL_PENCIL ? INKPOD_STROKE_FLAG_AUTO_ERASE
                                                      : INKPOD_STROKE_FLAG_PRESSURE_SIZE,
                    state->tools.color_rgba,
                    state->tools.active_tool == INKPOD_TOOL_PENCIL ? 1.0F : state->tools.diameter};
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
        case inkpod::renderer::kCanvasPointerMoved:
            if (state != nullptr) {
                state->view.pointer_device_x = GET_X_LPARAM(lparam);
                state->view.pointer_device_y = GET_Y_LPARAM(lparam);
                ++state->view.locator_generation;
                QueueLocatorSample(*state);
            }
            return 1;
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteCoreNotificationMessage(
    AppContext* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    switch (message) {
        case inkpod::app::kCoreStateChanged:
            if (state != nullptr) {
                RefreshTreePane(*state);
                RefreshLightTablePane(*state);
                RefreshSequencePane(*state);
                RefreshColorPanes(*state);
                UpdateMenuState(*state);
            }
            return 0;
        case kLocatorSampleReady: {
            std::unique_ptr<LocatorAsyncResult> result(
                reinterpret_cast<LocatorAsyncResult*>(lparam));
            if (state != nullptr && result != nullptr) {
                state->view.locator_pending = false;
                if (result->status == INKPOD_STATUS_OK
                    && result->generation == state->view.locator_generation) {
                    state->view.locator = result->output;
                    state->view.locator_valid = true;
                    UpdateLocatorStatus(*state);
                } else if (result->generation != state->view.locator_generation) {
                    QueueLocatorSample(*state);
                }
            }
            return 0;
        }
        case kEffectTaskCompleted:
            if (state != nullptr) {
                const InkpodStatus status = static_cast<InkpodStatus>(wparam);
                const bool prompt = state->effects.preview_prompt;
                state->effects.preview_prompt = false;
                if (state->effects.progress != nullptr) {
                    DestroyWindow(state->effects.progress);
                    state->effects.progress = nullptr;
                }
                inkpod_task_release(&state->effects.task);
                if (status == INKPOD_STATUS_OK && prompt && state->engine != nullptr) {
                    const int choice = MessageBoxW(
                        window,
                        L"Canvasのプレビューを適用しますか？\nキャンセルすると元の状態へ完全に戻ります。",
                        L"画像処理プレビュー",
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
                        ShowCoreError(*state, window, L"画像処理プレビューの確定");
                    }
                }
                UpdateMenuState(*state);
            }
            return 0;
        case kBatchTaskCompleted:
            if (state != nullptr) {
                const InkpodStatus status = static_cast<InkpodStatus>(wparam);
                if (state->batch.progress != nullptr) {
                    DestroyWindow(state->batch.progress);
                    state->batch.progress = nullptr;
                }
                if (state->batch.report != nullptr) {
                    try {
                        state->batch.last_result = BatchReportSummary(state->batch.report);
                    } catch (const std::bad_alloc&) {
                        state->batch.last_result = L"レポート表示用メモリが不足しました";
                    }
                }
                inkpod_batch_task_release(&state->batch.task);
                RefreshBatchPalette(state->batch);
                UpdateMenuState(*state);
                if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED
                    && !state->lifetime.smoke_test) {
                    ShowCoreError(*state, window, L"バッチ実行");
                }
            }
            return 0;
        case inkpod::app::kCoreAsyncFailed:
            if (state != nullptr && !state->lifetime.smoke_test) {
                ShowCoreError(*state, window, L"非同期処理");
            }
            return 0;
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteTimerAndCloseMessage(
    AppContext* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    switch (message) {
        case WM_TIMER:
            if (state != nullptr && wparam == kStatusProgressTimer) {
                std::array<wchar_t, 192U> progress{};
                if (FormatTaskProgressStatus(*state, progress)) {
                    SendMessageW(
                        state->windows.status_bar,
                        SB_SETTEXTW,
                        5,
                        reinterpret_cast<LPARAM>(progress.data()));
                } else {
                    KillTimer(window, kStatusProgressTimer);
                }
                return 0;
            }
            if (state != nullptr && wparam == kShortcutSequenceTimer) {
                if (state->shortcuts.pending_deadline == 0U
                    || GetTickCount64() >= state->shortcuts.pending_deadline) {
                    KillTimer(window, kShortcutSequenceTimer);
                    ClearPendingShortcut(state->shortcuts);
                    UpdateMenuState(*state);
                }
                return 0;
            }
            if (state != nullptr && wparam == kMotionPlaybackTimer) {
                if (state->animation.motion_active && !state->animation.motion_paused
                    && state->engine != nullptr) {
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
                        UpdateMotionState(state->animation, frame);
                    } else {
                        state->animation.motion_active = false;
                        KillTimer(window, kMotionPlaybackTimer);
                        if (!state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, L"モーション再生");
                        }
                    }
                }
                return 0;
            }
            if (state != nullptr && wparam == kContinuousSprayTimer) {
                if (state->effects.airbrush_active && state->tools.active_tool == kInteractionEffectAirbrush
                    && state->effects.task == nullptr) {
                    try {
                        if (state->effects.samples.size() < UINT64_C(1048576)) {
                            state->effects.samples.push_back(state->effects.airbrush_last);
                        } else {
                            state->effects.airbrush_active = false;
                            KillTimer(window, kContinuousSprayTimer);
                        }
                    } catch (const std::bad_alloc&) {
                        state->effects.airbrush_active = false;
                        KillTimer(window, kContinuousSprayTimer);
                    }
                }
                return 0;
            }
            if (state != nullptr && wparam == kAutosaveTimer && !state->document.recovery_path.empty()) {
                InkpodDocumentInfo info{};
                if (QueryDocument(*state, info)
                    && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
                    QueueAutosave(*state, state->document.recovery_path);
                }
                return 0;
            }
            break;
        case WM_CLOSE:
            if (state != nullptr && !state->lifetime.smoke_test && !ConfirmDiscard(*state)) {
                return 0;
            }
            if (state != nullptr && state->effects.task != nullptr) {
                inkpod_task_cancel(state->effects.task);
            }
            if (state != nullptr && state->batch.task != nullptr) {
                inkpod_batch_task_cancel(state->batch.task);
            }
            if (state != nullptr) {
                state->effects.airbrush_active = false;
                KillTimer(window, kContinuousSprayTimer);
            }
            ShowWindow(window, SW_HIDE);
            PostQuitMessage(0);
            return 0;
        case inkpod::renderer::kCanvasRenderFailed:
            if (state == nullptr || !state->lifetime.smoke_test) {
                MessageBoxW(
                    window,
                    L"Canvas renderer の描画に失敗しました。",
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
            }
            if (state == nullptr || !state->lifetime.smoke_test) {
                SendMessageW(window, WM_CLOSE, 0, 0);
            }
            return 0;
        case WM_NCDESTROY:
            KillTimer(window, kAutosaveTimer);
            KillTimer(window, kContinuousSprayTimer);
            KillTimer(window, kMotionPlaybackTimer);
            KillTimer(window, kShortcutSequenceTimer);
            KillTimer(window, kStatusProgressTimer);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            break;
    }
    return std::nullopt;
}

std::optional<LRESULT> RouteMainWindowMessage(
    AppContext* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    using MessageRoute = std::optional<LRESULT> (*)(
        AppContext*, HWND, UINT, WPARAM, LPARAM) noexcept;
    constexpr std::array<MessageRoute, 5U> routes{
        RouteWindowLifecycleMessage,
        RouteKeyboardMessage,
        RouteCanvasMessage,
        RouteCoreNotificationMessage,
        RouteTimerAndCloseMessage};
    for (const MessageRoute route : routes) {
        if (const auto result = route(state, window, message, wparam, lparam)) {
            return result;
        }
    }
    return std::nullopt;
}

bool PreTranslateKeyboardMessage(AppContext& state, const MSG& message) noexcept {
    if (message.message != WM_KEYDOWN && message.message != WM_SYSKEYDOWN) {
        return false;
    }
    const HWND focus = GetFocus();
    if (focus != nullptr) {
        wchar_t class_name[64]{};
        if (GetClassNameW(focus, class_name, static_cast<int>(std::size(class_name))) > 0
            && (_wcsicmp(class_name, L"Edit") == 0
                || _wcsnicmp(class_name, L"RichEdit", 8) == 0)) {
            return false;
        }
    }
    const HWND target_root = message.hwnd == nullptr ? nullptr : GetAncestor(message.hwnd, GA_ROOTOWNER);
    if (message.hwnd != state.windows.window
        && !IsChild(state.windows.window, message.hwnd)
        && target_root != state.windows.window) {
        return false;
    }
    return RouteKeyboardMessage(
               &state,
               state.windows.window,
               message.message,
               message.wParam,
               message.lParam)
        .has_value();
}

LRESULT CALLBACK MainWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    auto* state = reinterpret_cast<AppContext*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));

    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        state = static_cast<AppContext*>(create->lpCreateParams);
        state->windows.window = window;
        SetWindowLongPtrW(
            window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(state));
    }

    if (message == WM_COMMAND) {
        if (const auto result = RouteMainWindowCommand(
                state, window, wparam, lparam)) {
            return *result;
        }
    } else if (const auto result = RouteMainWindowMessage(
                   state, window, message, wparam, lparam)) {
        return *result;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

}  // namespace inkpod::windows::ui::runtime
