#include "ui/ui_resources.h"

#include <windows.h>
#include <commctrl.h>
#include <commdlg.h>
#include <dwmapi.h>
#include <shlobj.h>
#include <shellapi.h>
#include <windowsx.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <climits>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <cwctype>
#include <functional>
#include <memory>
#include <new>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "app/application_host.h"
#include "canvas.h"
#include "app/clipboard_adapter.h"
#include "app/core_host.h"
#include "app/document_shell.h"
#include "app/embedded_manual.h"
#include "inkpod/core_ffi.h"
#include "app/resource.h"
#include "ui/dialogs/about_dialog.h"
#include "ui/dialogs/basic_dialogs.h"
#include "ui/dialogs/batch_dialog.h"
#include "ui/dialogs/effects_dialogs.h"
#include "ui/dialogs/history_visualization_dialog.h"
#include "ui/dialogs/layer_palette.h"
#include "ui/command_state.h"
#include "ui/command_catalog.h"
#include "ui/localization.h"
#include "ui/history_presentation.h"
#include "ui/shortcut_controller.h"
#include "ui/panes/document_panes.h"
#include "ui/panes/color_panes.h"
#include "ui/panes/color_dock_pane.h"
#include "ui/panes/light_table_pane.h"
#include "ui/panes/locator_pane.h"
#include "ui/panes/sequence_pane.h"
#include "ui/panes/subpalette_pane.h"
#include "ui/panes/tool_options_pane.h"
#include "ui/tools/fill_controller.h"
#include "ui/tools/color_replace_controller.h"
#include "ui/tools/floating_paste_controller.h"
#include "ui/tools/selection_controller.h"
#include "ui/tools/tool_state.h"
#include "ui/tools/view_controller.h"
#include "ui/effects_controller.h"
#include "ui/batch_controller.h"
#include "ui/main_window.h"
#include "ui/main_window_runtime.h"
#include "ui/main_window_runtime_internal.h"
#include "ui/main_window_status_presenter.h"
#include "ui/tab_drag.h"

namespace inkpod::windows::ui::runtime {

inline constexpr UINT kLocatorSampleReady = WM_APP + 0x172U;
inline constexpr UINT kSequenceSwitchCompleted = WM_APP + 0x173U;

using inkpod::windows::ui::HistoryDialogState;
using inkpod::windows::ui::CellCreationDialogState;
using inkpod::windows::ui::CutPropertiesDialogState;
using inkpod::windows::ui::EffectEditorState;
using inkpod::windows::ui::ShortcutDialogState;
using inkpod::windows::ui::TextInputDialogState;
using inkpod::windows::ui::ViewOptionsDialogState;
using inkpod::windows::ui::ShootingFrameDialogState;
using inkpod::windows::ui::VanishingPointDialogState;
using inkpod::windows::ui::ShowAboutDialog;
using inkpod::windows::ui::ShowCellCreationOptions;
using inkpod::windows::ui::ShowCutProperties;
using inkpod::windows::ui::ShowHistoryDialog;
using inkpod::windows::ui::ShowEffectEditor;
using inkpod::windows::ui::SetEffectEditorPreviewStatus;
using inkpod::windows::ui::ProgressDialogInfo;
using inkpod::windows::ui::ShowShortcutEditor;
using inkpod::windows::ui::ShowTextInput;
using inkpod::windows::ui::ShowViewOptions;
using inkpod::windows::ui::ShowShootingFrameOptions;
using inkpod::windows::ui::ShowVanishingPointOptions;
using inkpod::app::AnimationUiState;
using inkpod::app::SequenceAutosaveBinding;
using inkpod::app::SequenceCellSwitchPolicy;
using inkpod::app::SequenceEndpointPolicy;
using inkpod::app::SequenceSwitchAsyncResult;
using inkpod::app::ApplicationHost;
using inkpod::app::BatchOperationUi;
using inkpod::app::ColorChartGenerationJob;
using inkpod::app::OutputColorGuardJob;
using inkpod::app::BatchUiState;
using inkpod::app::DocumentShellState;
using inkpod::app::DocumentShellController;
using inkpod::app::DocumentIdentity;
using inkpod::app::EffectsUiState;
using inkpod::app::Generation;
using inkpod::app::AdjustmentLayerUiState;
using inkpod::app::FilterJob;
using inkpod::app::GradientStopValue;
using inkpod::app::CanvasEffectOptions;
using inkpod::app::CanvasId;
using inkpod::app::CommandContext;
using inkpod::app::CommandResolveStatus;
using inkpod::app::CommandTargetScope;
using inkpod::app::CommandTimerKind;
using inkpod::app::CutSession;
using inkpod::app::CutMemberCache;
using inkpod::app::DocumentSessionId;
using inkpod::app::DocumentSession;
using inkpod::app::DocumentView;
using inkpod::app::DocumentViewId;
using inkpod::app::EditorGroupId;
using inkpod::app::EditorGroup;
using inkpod::app::EditorSplitOrientation;
using inkpod::app::EmbeddedHelpDocument;
using inkpod::app::EmbeddedHelpStatus;
using inkpod::app::WorkspaceWindowId;
using inkpod::app::WorkspaceWindow;

std::array<wchar_t, 96U> WorkspaceRegistryValueName(
    std::wstring_view base, std::uint32_t slot) noexcept {
    std::array<wchar_t, 96U> result{};
    _snwprintf_s(
        result.data(),
        result.size(),
        _TRUNCATE,
        L"%.*ls.%u",
        static_cast<int>(base.size()),
        base.data(),
        slot);
    return result;
}
using inkpod::app::LocatorAsyncResult;
using inkpod::app::PaneActionTarget;
using inkpod::app::PaneTargetNotice;
using inkpod::app::PaneTargetPolicy;
using inkpod::app::PaneTargetStatus;
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
using inkpod::app::PrivateRecoveryPath;
using inkpod::app::ReadBoundedFile;
using inkpod::app::RecoveryIsNewer;
using inkpod::app::RecoveryMetadata;
using inkpod::app::BuildRecoveryMetadata;
using inkpod::app::DiscardRecoveryArtifact;
using inkpod::app::ClearPreviousDocumentPaths;
using inkpod::app::SaveRestorePreviousDocumentsSetting;
using inkpod::app::SaveSequenceCellSwitchPolicy;
using inkpod::app::SaveSequenceEndpointPolicy;
using inkpod::app::SaveOutputColorGuardProfileSetting;
using inkpod::app::OutputColorGuardProfileSetting;
using inkpod::app::SequenceRecoveryPath;
using inkpod::app::ResolveDocumentFileIdentity;
using inkpod::app::UntitledDocumentIdentity;
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
using inkpod::windows::ui::CommandStateOwner;
using inkpod::windows::ui::FindCommandState;
using inkpod::windows::ui::IsCommandEnabled;
using inkpod::windows::ui::IsMenuCommand;
using inkpod::windows::ui::WorkspaceAuxiliaryPane;
using inkpod::windows::ui::WorkspaceLayoutState;
using inkpod::windows::ui::WorkspacePreset;
using inkpod::windows::ui::WorkspaceSplitOrientation;
using inkpod::windows::ui::tools::CancelSelectionGeometryPreview;
using inkpod::windows::ui::tools::CancelColorReplaceGeometryPreview;
using inkpod::windows::ui::tools::CancelFillGeometryPreview;
using inkpod::windows::ui::tools::CancelRasterGeometryPreview;
using inkpod::windows::ui::tools::IsGeometryCanvasPlane;
using inkpod::windows::ui::tools::IsGeometryCanvasTool;
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
using inkpod::windows::ui::tools::kInteractionColorReplace;
using inkpod::windows::ui::tools::kInteractionShootingFrame;
using inkpod::windows::ui::tools::kInteractionVanishingPoint;
using inkpod::windows::ui::tools::kInteractionGeometryLine;
using inkpod::windows::ui::tools::kInteractionGeometryCurve;
using inkpod::windows::ui::tools::kInteractionGeometryRectangle;
using inkpod::windows::ui::tools::kInteractionGeometryEllipse;
using inkpod::windows::ui::tools::kInteractionGeometryPolygon;
using inkpod::windows::ui::tools::kInteractionGeometryPolyline;
constexpr UINT kAutosaveIntervalMilliseconds = 60U * 1000U;
constexpr std::array<UINT, inkpod::app::RecentDocumentList::kCapacity>
    kRecentDocumentCommands{
        IDM_FILE_RECENT_1,
        IDM_FILE_RECENT_2,
        IDM_FILE_RECENT_3,
        IDM_FILE_RECENT_4,
        IDM_FILE_RECENT_5,
        IDM_FILE_RECENT_6,
        IDM_FILE_RECENT_7,
        IDM_FILE_RECENT_8};
constexpr UINT kEffectTaskCompleted = WM_APP + 0x170U;
constexpr UINT kBatchTaskCompleted = WM_APP + 0x171U;
constexpr UINT kColorChartGenerationCompleted = WM_APP + 0x174U;
constexpr UINT kShortcutSequenceTimerMilliseconds = 100U;
constexpr UINT kStatusProgressTimerMilliseconds = 100U;
constexpr UINT kContinuousSprayIntervalMilliseconds = 50U;

bool ArmCommandTimer(
    ApplicationHost& state,
    HWND window,
    CommandTimerKind kind,
    UINT interval) noexcept {
    if (const auto current = state.routing.timers.Find(kind)) {
        KillTimer(window, static_cast<UINT_PTR>(current->value));
        (void)state.routing.timers.Disarm(kind);
    }
    const auto token = state.routing.timers.Arm(
        kind, state.routing.targets.Capture());
    if (SetTimer(
            window,
            static_cast<UINT_PTR>(token.value),
            interval,
            nullptr)
        == 0U) {
        (void)state.routing.timers.Disarm(kind);
        return false;
    }
    return true;
}

void DisarmCommandTimer(
    ApplicationHost& state,
    HWND window,
    CommandTimerKind kind) noexcept {
    if (const auto token = state.routing.timers.Find(kind)) {
        KillTimer(window, static_cast<UINT_PTR>(token->value));
    }
    (void)state.routing.timers.Disarm(kind);
}

std::optional<inkpod::app::CommandTimerToken> ResolveCommandTimer(
    ApplicationHost& state,
    HWND window,
    WPARAM timer_id) noexcept {
    const auto token = state.routing.timers.Resolve(
        static_cast<std::uint64_t>(timer_id));
    if (!token.has_value()) {
        return std::nullopt;
    }
    const CommandTargetScope scope =
        token->kind == CommandTimerKind::ShortcutSequence
        ? inkpod::app::kWorkspaceCommandScope
        : inkpod::app::kDocumentViewCommandScope;
    if (state.routing.targets.Resolve(token->context, scope)
        == CommandResolveStatus::Ok) {
        return token;
    }
    KillTimer(window, static_cast<UINT_PTR>(token->value));
    (void)state.routing.timers.Disarm(token->kind);
    return std::nullopt;
}
const std::array<ViewOptionsDialogState::Choice, 7U>& LayerKindChoices() {
    static const std::array<ViewOptionsDialogState::Choice, 7U> choices{{
        {UiText(UiStringId::LayerBinaryColoring), INKPOD_LAYER_BINARY_COLORING},
        {UiText(UiStringId::LayerGrayscaleColoring), INKPOD_LAYER_GRAYSCALE_COLORING},
        {UiText(UiStringId::LayerRasterGeneral), INKPOD_LAYER_RASTER},
        {UiText(UiStringId::LayerSelection), INKPOD_LAYER_SELECTION},
        {UiText(UiStringId::LayerFrame), INKPOD_LAYER_FRAME},
        {UiText(UiStringId::ToolVanishingPoint), INKPOD_LAYER_VANISHING_POINT},
        {UiText(UiStringId::LayerAdjustment), INKPOD_LAYER_ADJUSTMENT},
    }};
    return choices;
}

struct PlaneDialogChoiceStorage {
    std::array<std::wstring, 4U> kind_labels;
    std::array<std::wstring, 5U> format_labels;
    std::array<ViewOptionsDialogState::Choice, 4U> kind_choices;
    std::array<ViewOptionsDialogState::Choice, 5U> format_choices;
    std::wstring validation_error;
};

template <std::size_t Count>
bool LoadViewOptionChoices(
    HINSTANCE instance,
    const std::array<std::pair<UINT, std::int32_t>, Count>& specifications,
    std::array<std::wstring, Count>& labels,
    std::array<ViewOptionsDialogState::Choice, Count>& choices) noexcept {
    try {
        for (std::size_t index = 0U; index < Count; ++index) {
            std::array<wchar_t, 128U> buffer{};
            const int length = LoadLocalizedStringW(
                instance,
                specifications[index].first,
                buffer.data(),
                static_cast<int>(buffer.size()));
            if (length <= 0) {
                return false;
            }
            labels[index].assign(buffer.data(), static_cast<std::size_t>(length));
            choices[index] = {
                labels[index].c_str(), specifications[index].second};
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

bool LoadPlaneDialogChoices(
    HINSTANCE instance, PlaneDialogChoiceStorage& storage) noexcept {
    static constexpr std::array<std::pair<UINT, std::int32_t>, 4U> kind_specs{{
        {IDS_PLANE_KIND_MAIN_LINE, INKPOD_TYPED_PLANE_MAIN_LINE},
        {IDS_PLANE_KIND_COLOR, INKPOD_TYPED_PLANE_COLOR},
        {IDS_PLANE_KIND_RASTER, INKPOD_TYPED_PLANE_RASTER},
        {IDS_PLANE_KIND_SELECTION, INKPOD_TYPED_PLANE_SELECTION},
    }};
    static constexpr std::array<std::pair<UINT, std::int32_t>, 5U> format_specs{{
        {IDS_STORAGE_BINARY8, INKPOD_STORAGE_BINARY8},
        {IDS_STORAGE_GRAYSCALE8, INKPOD_STORAGE_GRAYSCALE8},
        {IDS_STORAGE_GRAYSCALE16, INKPOD_STORAGE_GRAYSCALE16},
        {IDS_STORAGE_RGBA8, INKPOD_STORAGE_RGBA8},
        {IDS_STORAGE_RGBA16, INKPOD_STORAGE_RGBA16},
    }};
    std::array<wchar_t, 160U> error{};
    const int error_length = LoadLocalizedStringW(
        instance,
        IDS_PLANE_CREATION_INVALID,
        error.data(),
        static_cast<int>(error.size()));
    if (error_length <= 0
        || !LoadViewOptionChoices(
            instance, kind_specs, storage.kind_labels, storage.kind_choices)
        || !LoadViewOptionChoices(
            instance, format_specs, storage.format_labels, storage.format_choices)) {
        return false;
    }
    try {
        storage.validation_error.assign(
            error.data(), static_cast<std::size_t>(error_length));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

struct PlaneCreationValidationContext {
    inkpod::app::CoreHost* engine{};
    std::uint64_t layer_id{};
    const wchar_t* error_message{};
};

const wchar_t* ValidatePlaneCreationOptions(
    void* context,
    const std::array<std::int32_t, 4U>& values,
    std::uint32_t value_count) noexcept {
    const auto* validation = static_cast<const PlaneCreationValidationContext*>(context);
    if (validation == nullptr || validation->engine == nullptr
        || validation->layer_id == 0U || validation->error_message == nullptr
        || value_count < 2U) {
        return UiText(UiStringId::Text0324);
    }
    const InkpodStatus status = validation->engine->Invoke(
        [layer_id = validation->layer_id,
         kind = static_cast<InkpodTypedPlaneKind>(values[0]),
         format = static_cast<InkpodStoragePixelFormat>(values[1])](InkpodCore* core) {
            return inkpod_core_validate_plane_creation(core, layer_id, kind, format);
        },
        false,
        false);
    return status == INKPOD_STATUS_OK ? nullptr : validation->error_message;
}

bool QuerySnapshotTransform(
    ApplicationHost& state, InkpodSnapshotTransform& transform) noexcept;
bool QueryDocument(ApplicationHost& state, InkpodDocumentInfo& info) noexcept;
bool QueryShootingFrame(
    ApplicationHost& state,
    bool& present,
    InkpodShootingFrameInfo& frame) noexcept;
InkpodShootingFrameInput ShootingFrameInputFromInfo(
    const InkpodShootingFrameInfo& frame) noexcept;
bool BuildCellCreationDialogPreview(
    void* context,
    const InkpodCellCreationOptions& options,
    InkpodCellCreationPlanItem& preview) noexcept;
InkpodStatus SaveToPath(
    ApplicationHost& state, const std::wstring& path) noexcept;
std::wstring Utf8UserText(const std::string& text);
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
using inkpod::windows::ui::BatchController;
using inkpod::windows::ui::EffectsController;
bool QueryTreeNode(ApplicationHost& state, bool plane, TreePaneNode& output) noexcept;
bool RefreshTreePane(ApplicationHost& state) noexcept;
const InkpodEditorStateInfo* PresentedEditorState(
    const ApplicationHost& state) noexcept;
void SetDrawingColor(ApplicationHost& state, InkpodColorValue color) noexcept;
InkpodStatus SetEditorActiveTool(
    ApplicationHost& state, std::uint32_t tool) noexcept;
void CancelCoreRasterGeometryPreview(ApplicationHost& state) noexcept;
InkpodStatus SetEditorDiameter(ApplicationHost& state, float diameter) noexcept;
InkpodStatus SetEditorFillOptions(
    ApplicationHost& state,
    const inkpod::windows::ui::FillToolOptions& options) noexcept;
InkpodStatus SetEditorBrushOptions(
    ApplicationHost& state,
    const InkpodEditorBrushOptions& options) noexcept;
InkpodStatus SetEditorSelectionOptions(ApplicationHost& state) noexcept;
InkpodStatus SetEditorActiveTarget(
    ApplicationHost& state,
    std::uint64_t layer_id,
    std::uint64_t plane_id) noexcept;
InkpodStatus SetEditorPaletteCursor(
    ApplicationHost& state,
    std::uint32_t group,
    std::uint32_t index,
    bool present) noexcept;
bool RefreshColorPanes(ApplicationHost& state) noexcept;
void RefreshDockPaneViews(ApplicationHost& state) noexcept;
UINT ActiveToolOptionsCommand(const ApplicationHost& state) noexcept;
bool QueryToolOptionsDetail(
    void* context,
    UINT command,
    inkpod::windows::ui::panes::ToolOptionsDetailModel& output) noexcept;
bool ChangeToolOptionsDetail(
    void* context,
    UINT command,
    const inkpod::windows::ui::panes::ToolOptionsDetailModel& value,
    bool execute) noexcept;
void RefreshLocatorPane(ApplicationHost& state) noexcept;
std::wstring LocatorDocumentName(const DocumentSession& document);
void DispatchSequencePaneCommand(void* context, UINT command) noexcept;
void ActivateSequencePaneCell(void* context, std::uint32_t index) noexcept;
void ReorderCutSequenceCell(
    void* context, std::uint32_t from, std::uint32_t to) noexcept;
void DispatchLightTablePaneCommand(void* context, UINT command) noexcept;
void SelectLightTablePaneEntry(
    void* context,
    bool set_selection,
    std::uint32_t index,
    std::uint64_t stable_id) noexcept;
bool RefreshSubpalettePane(ApplicationHost& state) noexcept;
void UpdateBatchTarget(ApplicationHost& state) noexcept;
void RefreshBatchPalette(BatchUiState& batch, HWND palette) noexcept;
void DispatchSubpalettePaneCommand(void* context, UINT command) noexcept;
void PerformSubpalettePaneAction(
    void* context,
    inkpod::windows::ui::panes::SubpalettePaneAction action) noexcept;
void SampleSubpalettePane(void* context, double x, double y) noexcept;
void ApplySubpalettePaneView(
    void* context,
    const inkpod::renderer::CanvasViewGesture& gesture) noexcept;
InkpodStatus ImportSequencePaths(
    ApplicationHost& state,
    const std::vector<std::wstring>& paths,
    DocumentSessionId session,
    Generation generation) noexcept;
void AttachFilePreviewSequence(
    ApplicationHost& state, const std::wstring& opened_path) noexcept;
InkpodStatus ApplyTreeEdit(
    ApplicationHost& state,
    InkpodTreeOperation operation,
    std::uint64_t object_id,
    std::uint32_t destination_index,
    std::uint64_t& out_object_id) noexcept;

ApplicationHost* ActivateWorkspaceContext(void* context) noexcept {
    auto* workspace = static_cast<inkpod::app::WorkspaceWindow*>(context);
    if (workspace == nullptr || workspace->application == nullptr
        || !workspace->application->ActivateWorkspaceWindow(
            workspace->id, true)) {
        return nullptr;
    }
    return workspace->application;
}

bool DispatchEnabledCommand(
    ApplicationHost& state,
    HWND window,
    UINT command,
    std::optional<inkpod::app::PaneInstanceId> pane = std::nullopt) noexcept {
    const auto* workspace = state.WorkspaceForWindow(window);
    if (workspace == nullptr
        || !state.ActivateWorkspaceWindow(workspace->id, false)) {
        return false;
    }
    if (!IsCommandEnabled(state.Workspace().command_states, command)) {
        return false;
    }
    return IssueCommand(&state, window, command, 0, pane).has_value();
}

void DispatchBatchPaletteCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        DispatchEnabledCommand(
            *state,
            state->Workspace().windows.window,
            command,
            state->routing.batch_pane);
    }
}

void UpdateMenuState(ApplicationHost& state) noexcept;

void SelectBatchPaletteOperation(
    void* context, std::uint32_t selected_index) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr) {
        state->batch.selected_stage = selected_index;
        if (selected_index > 0U
            && selected_index <= state->batch.operations.size()) {
            state->batch.selected_operation = selected_index - 1U;
        }
        UpdateBatchParameterEditor(
            state->Workspace().batch_dialog.parameter_host,
            selected_index,
            state->batch.task == nullptr);
        UpdateMenuState(*state);
    }
}

void BatchDraftChanged(void* context) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->batch.task == nullptr) {
        BatchController::ResetDerivedState(state->batch);
        RefreshBatchPalette(
            state->batch, state->Workspace().batch_palette);
        UpdateMenuState(*state);
    }
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
    panes.light_table_selection_session = {};
    panes.light_table_selection_generation = {};
    panes.light_table_move_context.reset();
    panes.light_table_move_samples.clear();
}

void ResetToolDocumentState(ToolUiState& tools) noexcept {
    tools.editor = {};
    tools.procedure = {};
    tools.color_replace_base_revision = 0U;
    tools.color_replace_gesture_samples.clear();
    tools.floating_active = false;
    tools.floating_transform = InkpodFloatingTransform{
        sizeof(InkpodFloatingTransform), INKPOD_TRANSFORM_ANCHOR_CENTER, 0.0, 0.0, 1.0, 1.0, 0.0};
    tools.floating_bounds = {};
    tools.floating_gesture_samples.clear();
    tools.floating_drag_mode = 0U;
    tools.fill_gesture_samples.clear();
    tools.selection_gesture_samples.clear();
    tools.geometry_gesture_samples.clear();
    tools.geometry_base_revision = 0U;
    tools.geometry_view_revision = 0U;
    tools.geometry_preview_active = false;
    tools.geometry_snap_bypass = false;
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
    view.locator_presented_generation = view.locator_generation;
    view.locator_valid = false;
    view.locator = {};
    view.locator_neighborhood_width = 0U;
    view.locator_neighborhood_height = 0U;
    view.locator_neighborhood_origin_x = 0;
    view.locator_neighborhood_origin_y = 0;
    view.locator_neighborhood.fill(0U);
    view.gesture_samples.clear();
    view.guide_drag_active = false;
    view.guide_drag_axis = 0U;
    view.guide_drag_id = 0U;
    view.active_drag.reset();
}

void ResetAnimationDocumentState(AnimationUiState& animation) noexcept {
    animation.active_sequence_index = 0U;
    animation.active_sequence_name.clear();
    animation.motion_active = false;
    animation.motion_paused = false;
    animation.sequence_switch_pending = false;
}

void DispatchToolPaletteCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        DispatchEnabledCommand(
            *state,
            state->Workspace().windows.window,
            command,
            state->routing.tool_pane);
        if (inkpod::windows::ui::ToolPaletteCommandHasOptions(command)
            && inkpod::windows::ui::panes::IsToolOptionsFlyoutVisible(
                state->Workspace().windows.tool_options_flyout)) {
            inkpod::windows::ui::panes::ShowToolOptionsFlyout(
                state->Workspace().windows.tool_options_flyout,
                inkpod::windows::ui::ToolPaletteCheckedOptionsAnchor(
                    state->Workspace().tools.palette),
                command);
        }
    }
}

void RequestToolPaletteOptions(
    void* context, UINT command, HWND anchor) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->Workspace().windows.window == nullptr) {
        return;
    }
    if (command != IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        DispatchEnabledCommand(
            *state,
            state->Workspace().windows.window,
            command,
            state->routing.tool_pane);
    }
    inkpod::windows::ui::panes::ToggleToolOptionsFlyout(
        state->Workspace().windows.tool_options_flyout,
        anchor,
        command);
    UpdateMenuState(*state);
}

void ChangeToolOptionsDiameter(void* context, float diameter) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || !std::isfinite(diameter)
        || diameter < inkpod::windows::ui::panes::kMinimumToolDiameter
        || diameter > inkpod::windows::ui::panes::kMaximumToolDiameter) {
        return;
    }
    if (SetEditorDiameter(*state, diameter) == INKPOD_STATUS_OK) {
        inkpod::windows::ui::panes::UpdateToolOptionsPane(
            state->Workspace().windows.tool_options,
            state->Workspace().tools.active_tool,
            state->Workspace().tools.active_plane,
            state->Workspace().tools.diameter,
            state->Workspace().tools.brush);
        UpdateMenuState(*state);
    }
}

void ChangeToolOptionsBrush(
    void* context, const InkpodEditorBrushOptions& options) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    if (SetEditorBrushOptions(*state, options) == INKPOD_STATUS_OK) {
        inkpod::windows::ui::panes::UpdateToolOptionsPane(
            state->Workspace().windows.tool_options,
            state->Workspace().tools.active_tool,
            state->Workspace().tools.active_plane,
            state->Workspace().tools.diameter,
            state->Workspace().tools.brush);
        UpdateMenuState(*state);
    }
}

void ChangeDockDrawingColor(
    void* context, const InkpodColorValue& color) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    SetDrawingColor(*state, color);
    inkpod::windows::ui::panes::UpdateColorDockPaneDrawingColor(
        state->Workspace().windows.color_pane, state->Workspace().tools.drawing_color);
}

void ChangeDockMainLineColor(
    void* context, const InkpodColorValue& color) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->engine == nullptr) {
        return;
    }
    const PaneActionTarget target = state->routing.pane_targets.CaptureAction(
        state->routing.color_pane,
        state->routing.targets.Capture(),
        state->routing.targets);
    InkpodStatus status = INKPOD_STATUS_INVALID_STATE;
    if (target.status == PaneTargetStatus::Ok
        && target.context.document_session.has_value()
        && target.context.generation.has_value()) {
        InkpodDocumentInfo document{};
        document.struct_size = sizeof(document);
        if (state->engine->GetDocumentInfo(
                target.context.document_session.value(),
                target.context.generation.value(),
                document)) {
            InkpodPrimitiveRequestV3 request{};
            request.struct_size = sizeof(request);
            request.opcode = INKPOD_PRIMITIVE_SET_MAIN_LINE_COLOR;
            request.schema_version = 1U;
            request.base_revision = document.document_revision;
            request.payload_id.struct_size = sizeof(request.payload_id);
            request.color = color;
            status = state->engine->InvokePrimitive(
                target.context.document_session.value(),
                target.context.generation.value(),
                request,
                true,
                true);
        }
    }
    if (status == INKPOD_STATUS_OK) {
        state->Workspace().panes.main_line_color = color;
    } else {
        RefreshColorPanes(*state);
    }
    inkpod::windows::ui::panes::UpdateColorDockPaneMainLineColor(
        state->Workspace().windows.color_pane, state->Workspace().panes.main_line_color);
}

void SelectDockColor(
    void* context, std::uint32_t index, bool chart) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    const auto& colors = chart
        ? state->Workspace().panes.color_chart_colors
        : state->Workspace().panes.palette_colors;
    if (index >= colors.size()) {
        return;
    }
    std::uint32_t group = state->Workspace().panes.palette_group;
    if (chart) {
        state->Workspace().panes.selected_color_chart_index = index;
        state->Workspace().panes.color_chart_page = index / 20U;
    } else {
        group = index / 10U;
        if (SetEditorPaletteCursor(*state, group, index, true)
            != INKPOD_STATUS_OK) {
            return;
        }
    }
    SetDrawingColor(*state, colors[index]);
    UpdateMenuState(*state);
}

void ChangeDockPaletteGroup(void* context, int delta) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    const std::uint32_t groups = std::max<std::uint32_t>(
        1U,
        static_cast<std::uint32_t>(
            (state->Workspace().panes.palette_colors.size() + 9U) / 10U));
    const int current = static_cast<int>(state->Workspace().panes.palette_group % groups);
    const std::uint32_t group = static_cast<std::uint32_t>(
        (current + delta % static_cast<int>(groups) + static_cast<int>(groups))
        % static_cast<int>(groups));
    std::uint32_t index = state->Workspace().panes.selected_palette_index;
    if (!state->Workspace().panes.palette_colors.empty()) {
        index = std::min<std::uint32_t>(
            group * 10U,
            static_cast<std::uint32_t>(
                state->Workspace().panes.palette_colors.size() - 1U));
    }
    if (SetEditorPaletteCursor(*state, group, index, true)
        != INKPOD_STATUS_OK) {
        return;
    }
    RefreshDockPaneViews(*state);
    UpdateMenuState(*state);
}

void NotifyToolPaletteVisibilityChanged(void* context) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        UpdateMenuState(*state);
    }
}

void DispatchLayerPaletteCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        DispatchEnabledCommand(
            *state,
            state->Workspace().windows.window,
            command,
            state->routing.layer_pane);
    }
}

void SelectLayerPaletteLayer(void* context, std::uint64_t layer_id) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->engine == nullptr || layer_id == 0U) {
        return;
    }
    std::vector<TreePaneNode> layers;
    std::vector<TreePaneNode> planes;
    std::uint32_t selected_layer_index{};
    DocumentPanesController controller(*state->engine);
    if (controller.LoadTree(
            layer_id,
            false,
            layers,
            planes,
            selected_layer_index) != INKPOD_STATUS_OK
        || planes.empty()) {
        return;
    }
    const InkpodStatus status = SetEditorActiveTarget(
        *state, layer_id, planes.front().id);
    if (status != INKPOD_STATUS_OK) {
        ShowCoreError(*state, state->Workspace().windows.window, UiText(UiStringId::Text0398));
    } else {
        RefreshTreePane(*state);
    }
    UpdateMenuState(*state);
}

void SelectLayerPalettePlane(void* context, std::uint64_t plane_id) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->engine == nullptr || plane_id == 0U
        || !state->RefreshEditorPresentation(
            state->Document().id, state->Document().generation)) {
        return;
    }
    const InkpodEditorStateInfo* editor = PresentedEditorState(*state);
    if (editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_layer_id == 0U) {
        return;
    }
    const InkpodStatus status = SetEditorActiveTarget(
        *state, editor->active_layer_id, plane_id);
    if (status != INKPOD_STATUS_OK) {
        ShowCoreError(*state, state->Workspace().windows.window, UiText(UiStringId::Text0321));
    } else {
        RefreshTreePane(*state);
    }
    UpdateMenuState(*state);
}

void ToggleLayerPaletteTarget(
    void* context,
    std::uint64_t id,
    bool plane,
    bool range) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->engine == nullptr || id == 0U
        || !state->RefreshEditorPresentation(
            state->Document().id, state->Document().generation)) {
        return;
    }
    const InkpodEditorStateInfo* editor = PresentedEditorState(*state);
    if (editor == nullptr) {
        return;
    }
    std::vector<InkpodEditTarget> targets;
    if (state->engine->GetEditTargets(
            state->Document().id,
            state->Document().generation,
            targets) != INKPOD_STATUS_OK) {
        return;
    }
    const std::uint64_t layer_id = plane
        ? state->Workspace().panes.active_tree_layer_id
        : id;
    const auto same_target = [=](const InkpodEditTarget& target) noexcept {
        return plane
            ? target.kind == INKPOD_EDIT_TARGET_PLANE
                && target.layer_id == layer_id && target.plane_id == id
            : target.kind == INKPOD_EDIT_TARGET_LAYER
                && target.layer_id == id;
    };
    const auto exact = std::find_if(targets.begin(), targets.end(), same_target);
    const auto parent = plane
        ? std::find_if(
              targets.begin(),
              targets.end(),
              [=](const InkpodEditTarget& target) noexcept {
                  return target.kind == INKPOD_EDIT_TARGET_LAYER
                      && target.layer_id == layer_id;
              })
        : targets.end();
    if (parent != targets.end()) {
        targets.erase(parent);
        for (const auto& item : state->Workspace().panes.layer_palette_dialog.plane_items) {
            if (item.id != id) {
                targets.push_back(InkpodEditTarget{
                    sizeof(InkpodEditTarget),
                    INKPOD_EDIT_TARGET_PLANE,
                    layer_id,
                    item.id,
                    0U});
            }
        }
    } else if (exact != targets.end() && !range) {
        targets.erase(exact);
    } else {
        const auto& items = plane
            ? state->Workspace().panes.layer_palette_dialog.plane_items
            : state->Workspace().panes.layer_palette_dialog.items;
        const auto target_item = std::find_if(
            items.begin(), items.end(), [=](const auto& item) { return item.id == id; });
        if (target_item == items.end()) {
            return;
        }
        std::size_t first = static_cast<std::size_t>(
            std::distance(items.begin(), target_item));
        std::size_t last = first;
        if (range) {
            const std::uint64_t anchor = plane
                ? state->Workspace().panes.layer_palette_dialog.selected_plane_id
                : state->Workspace().panes.layer_palette_dialog.selected_layer_id;
            const auto anchor_item = std::find_if(
                items.begin(), items.end(), [=](const auto& item) { return item.id == anchor; });
            if (anchor_item != items.end()) {
                last = static_cast<std::size_t>(
                    std::distance(items.begin(), anchor_item));
                if (first > last) {
                    std::swap(first, last);
                }
            }
        }
        for (std::size_t index = first; index <= last; ++index) {
            const InkpodEditTarget candidate{
                sizeof(InkpodEditTarget),
                plane ? INKPOD_EDIT_TARGET_PLANE : INKPOD_EDIT_TARGET_LAYER,
                plane ? layer_id : items[index].id,
                plane ? items[index].id : 0U,
                0U};
            if (std::none_of(
                    targets.begin(), targets.end(), [&](const InkpodEditTarget& target) {
                        return target.kind == candidate.kind
                            && target.layer_id == candidate.layer_id
                            && target.plane_id == candidate.plane_id;
                    })) {
                targets.push_back(candidate);
            }
        }
    }
    const InkpodStatus status = state->engine->SetEditTargets(
        state->Document().id,
        state->Document().generation,
        editor->editor_revision,
        targets);
    if (status != INKPOD_STATUS_OK) {
        ShowCoreError(*state, state->Workspace().windows.window, UiText(UiStringId::Text0856));
    } else {
        RefreshTreePane(*state);
    }
    UpdateMenuState(*state);
}

InkpodStatus ApplyGroupedEditTargetCommand(
    ApplicationHost& state,
    std::uint32_t operation,
    std::uint64_t flags,
    std::uint32_t kind,
    std::uint32_t pixel_format,
    bool& applied) noexcept {
    applied = false;
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodEditTarget> targets;
    InkpodStatus status = state.engine->GetEditTargets(
        state.Document().id, state.Document().generation, targets);
    if (status != INKPOD_STATUS_OK || targets.empty()) {
        return status;
    }
    applied = true;
    const InkpodEditTargetCommand command{
        sizeof(InkpodEditTargetCommand),
        operation,
        flags,
        kind,
        pixel_format,
        0U};
    InkpodDispatchResult result{};
    std::vector<InkpodEditTarget> outputs;
    return state.engine->ApplyEditTargetCommand(
        state.Document().id,
        state.Document().generation,
        command,
        result,
        outputs);
}

void ReorderLayerPaletteLayer(
    void* context,
    std::uint64_t layer_id,
    std::uint32_t destination_index) noexcept {
    auto* state = ActivateWorkspaceContext(context);
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
        ShowCoreError(*state, state->Workspace().windows.window, UiText(UiStringId::Text0393));
    } else {
        RefreshTreePane(*state);
    }
    UpdateMenuState(*state);
}

void ReorderLayerPalettePlane(
    void* context,
    std::uint64_t plane_id,
    std::uint32_t destination_index) noexcept {
    auto* state = ActivateWorkspaceContext(context);
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
        ShowCoreError(*state, state->Workspace().windows.window, UiText(UiStringId::Text0319));
    } else {
        RefreshTreePane(*state);
    }
    UpdateMenuState(*state);
}

void ChangeLayerPaletteSplit(void* context, std::uint32_t split_milli) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    state->Workspace().windows.workspace.layer_split_milli =
        std::clamp<std::uint32_t>(split_milli, 200U, 800U);
}

void NotifyLayerPaletteVisibilityChanged(void* context) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
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
    effects.gesture_context.reset();
}

std::optional<DocumentViewId> FrontendViewForCoreView(
    const inkpod::app::DocumentSession& document,
    std::uint64_t core_view_id) noexcept {
    const auto* view = document.FindCoreView(core_view_id);
    return view == nullptr ? std::nullopt : std::optional{view->id};
}

void ResetUiForNewActiveDocument(ApplicationHost& state) noexcept {
    CancelFillGeometryPreview(state.Workspace().tools, state.Workspace().windows.canvas);
    CancelSelectionGeometryPreview(state.Workspace().tools, state.Workspace().windows.canvas);
    CancelColorReplaceGeometryPreview(
        state.Workspace().tools, state.Workspace().windows.canvas);
    state.Thumbnails().RemoveDocument(
        state.Document().id, state.Document().generation);
    ResetDocumentShellTransientState(state.Document().shell);
    ResetPaneDocumentState(state.Workspace().panes);
    ResetToolDocumentState(state.Workspace().tools);
    ResetViewDocumentState(state.ActiveView().presentation);
    ResetAnimationDocumentState(state.Workspace().animation);
    ResetEffectsDocumentState(state.effects);
    if (state.Workspace().windows.window != nullptr) {
        DisarmCommandTimer(
            state, state.Workspace().windows.window, CommandTimerKind::ContinuousSpray);
        DisarmCommandTimer(
            state, state.Workspace().windows.window, CommandTimerKind::MotionPlayback);
        DisarmCommandTimer(
            state, state.Workspace().windows.window, CommandTimerKind::ShortcutSequence);
        ArmCommandTimer(
            state,
            state.Workspace().windows.window,
            CommandTimerKind::Autosave,
            kAutosaveIntervalMilliseconds);
    }
}

bool ActivateDocumentTab(
    ApplicationHost& state,
    DocumentViewId view) noexcept {
    if (!view) {
        return false;
    }
    CancelFillGeometryPreview(
        state.Workspace().tools, state.Workspace().windows.canvas);
    CancelSelectionGeometryPreview(
        state.Workspace().tools, state.Workspace().windows.canvas);
    CancelColorReplaceGeometryPreview(
        state.Workspace().tools, state.Workspace().windows.canvas);
    CancelCoreRasterGeometryPreview(state);
    if (!state.ActivateDocumentView(view)) {
        return false;
    }
    ResetPaneDocumentState(state.Workspace().panes);
    InkpodSnapshotTransform transform{};
    if (QuerySnapshotTransform(state, transform)) {
        state.ActiveView().presentation.flip_horizontal =
            (transform.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL) != 0U;
        state.ActiveView().presentation.flip_vertical =
            (transform.flags & INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL) != 0U;
    }
    if (state.Workspace().windows.window != nullptr) {
        DisarmCommandTimer(
            state,
            state.Workspace().windows.window,
            CommandTimerKind::Autosave);
        ArmCommandTimer(
            state,
            state.Workspace().windows.window,
            CommandTimerKind::Autosave,
            kAutosaveIntervalMilliseconds);
    }
    (void)RefreshSubpalettePane(state);
    (void)RefreshColorPanes(state);
    RefreshDockPaneViews(state);
    UpdateBatchTarget(state);
    RefreshBatchPalette(state.batch, state.Workspace().batch_palette);
    UpdateMenuState(state);
    return true;
}

void RelayoutEditorArea(ApplicationHost& state) noexcept {
    RECT client{};
    if (state.Workspace().windows.window != nullptr
        && GetClientRect(state.Workspace().windows.window, &client) != FALSE) {
        inkpod::windows::ui::LayoutMainChrome(
            state.Workspace().windows,
            state.lifetime.smoke_test,
            client.right - client.left,
            client.bottom - client.top);
    }
}

void DispatchColorPaneCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        DispatchEnabledCommand(
            *state,
            state->Workspace().windows.window,
            command,
            state->routing.color_pane);
    }
}

bool ActivateEditorGroup(
    ApplicationHost& state,
    EditorGroupId group_id) noexcept {
    auto* group = state.Workspace().editors.Find(group_id);
    if (group == nullptr) {
        return false;
    }
    auto* previous = state.Workspace().editors.Active();
    const HWND focused = GetFocus();
    const auto owns_focus = [](
                                const inkpod::app::EditorGroup* owner,
                                HWND target) noexcept {
        return owner != nullptr && target != nullptr
            && (target == owner->canvas || target == owner->document_tabs
                || (owner->canvas != nullptr
                    && IsChild(owner->canvas, target) != FALSE)
                || (owner->document_tabs != nullptr
                    && IsChild(owner->document_tabs, target) != FALSE));
    };
    if (previous == group
        && state.routing.targets.ActiveDocumentView() == group->ActiveView()) {
        if (owns_focus(group, focused)) {
            group->focus_history = focused;
        }
        return true;
    }
    if (owns_focus(previous, focused)) {
        previous->focus_history = focused;
    }
    if (group->ActiveView()) {
        const bool activated = ActivateDocumentTab(state, group->ActiveView());
        if (activated && owns_focus(group, focused)) {
            group->focus_history = focused;
        }
        return activated;
    }
    renderer::CancelCanvasStroke(previous == nullptr ? nullptr : previous->canvas);
    const bool activated = state.Workspace().editors.Activate(group_id)
        && state.routing.targets.ActivateEditorGroup(group_id);
    if (activated) {
        if (owns_focus(group, focused)) {
            group->focus_history = focused;
        }
        inkpod::windows::ui::SyncActiveEditorHandles(
            state.Workspace().windows);
        UpdateMenuState(state);
    }
    return activated;
}

bool CreateDocumentViewInGroup(
    ApplicationHost& state,
    EditorGroupId destination,
    HWND error_owner,
    std::optional<std::size_t> insertion_index) noexcept {
    inkpod::app::WorkspaceWindow* destination_workspace = state.FindWorkspace(
        state.routing.targets.WorkspaceForGroup(destination));
    EditorGroup* destination_group = destination_workspace == nullptr
        ? nullptr
        : destination_workspace->editors.Find(destination);
    const std::size_t target_index = insertion_index.value_or(
        destination_group == nullptr ? 0U : destination_group->ViewCount());
    if (state.engine == nullptr || state.Documents().Current() == nullptr
        || destination_group == nullptr
        || target_index > destination_group->ViewCount()) {
        return false;
    }
    const CommandContext previous = state.routing.targets.Capture();
    DocumentSession& document = state.Document();
    const auto frontend_view = state.routing.targets.AddDocumentViewTo(destination);
    if (!frontend_view.has_value()
        || document.ViewCount() >= inkpod::app::DocumentSession::kMaximumViews) {
        if (frontend_view.has_value()) {
            (void)state.routing.targets.RemoveDocumentView(frontend_view.value());
        }
        return false;
    }
    std::uint64_t core_view_id{};
    const InkpodStatus status = state.engine->Invoke(
        document.id,
        document.generation,
        [&core_view_id](InkpodCore* core) {
            return inkpod_core_view_create(core, &core_view_id);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        (void)state.routing.targets.RemoveDocumentView(frontend_view.value());
        if (previous.document_view.has_value()) {
            (void)state.ActivateDocumentView(previous.document_view.value());
        }
        ShowCoreError(state, error_owner, UiText(UiStringId::Text0274));
        return false;
    }
    const bool registered = document.AddView(
            frontend_view.value(),
            state.routing.targets.CurrentGeneration(),
            core_view_id)
        && state.engine->RegisterDocumentView(
            document.id,
            document.generation,
            frontend_view.value(),
            core_view_id)
        && destination_group->InsertView(frontend_view.value(), target_index);
    if (!registered || !state.ActivateDocumentView(frontend_view.value())) {
        (void)state.engine->Invoke(
            document.id,
            document.generation,
            [core_view_id](InkpodCore* core) {
                return inkpod_core_view_close(core, core_view_id);
            },
            false,
            false);
        (void)destination_workspace->editors.RemoveView(frontend_view.value());
        (void)state.engine->UnregisterDocumentView(
            document.id, document.generation, frontend_view.value());
        (void)document.RemoveView(frontend_view.value());
        (void)state.routing.targets.RemoveDocumentView(frontend_view.value());
        if (previous.document_view.has_value()) {
            (void)state.ActivateDocumentView(previous.document_view.value());
        }
        return false;
    }
    state.ActiveView().presentation.secondary_view_id = core_view_id;
    state.ActiveView().presentation.active_view_id = core_view_id;
    state.ActiveView().presentation.flip_horizontal = false;
    state.ActiveView().presentation.flip_vertical = false;
    UpdateMenuState(state);
    return true;
}

bool SplitEditorArea(
    ApplicationHost& state,
    EditorSplitOrientation orientation,
    HWND error_owner) noexcept {
    auto& editors = state.Workspace().editors;
    if (editors.GroupCount() == 2U) {
        const bool changed = editors.SetOrientation(orientation);
        if (changed) {
            RelayoutEditorArea(state);
            UpdateMenuState(state);
        }
        return changed;
    }
    if (state.renderer == nullptr || state.engine == nullptr) {
        return false;
    }
    const EditorGroupId previous_group = state.routing.targets.EditorGroup();
    const auto binding = state.routing.targets.AddEditorGroup();
    const bool model_split = binding.has_value()
        && editors.Split(
            binding->group,
            binding->canvas,
            state.routing.targets.CurrentGeneration(),
            orientation);
    if (!model_split) {
        if (state.lifetime.smoke_test) {
            std::fprintf(
                stderr,
                "editor split model failed binding=%d groups=%zu workspace=%llu\n",
                binding.has_value() ? 1 : 0,
                editors.GroupCount(),
                static_cast<unsigned long long>(state.Workspace().id.Value()));
        }
        if (binding.has_value()) {
            (void)state.routing.targets.RemoveEditorGroup(binding->group);
        }
        return false;
    }
    auto* group = editors.Find(binding->group);
    if (group == nullptr
        || !inkpod::windows::ui::CreateEditorGroupTabs(
            state.Workspace().windows,
            *group,
            state.lifetime.instance,
            state.lifetime.smoke_test)) {
        if (state.lifetime.smoke_test) {
            std::fprintf(
                stderr,
                "editor split tabs failed group=%d tabs=%d workspace=%llu\n",
                group != nullptr ? 1 : 0,
                group != nullptr && group->document_tabs != nullptr ? 1 : 0,
                static_cast<unsigned long long>(state.Workspace().id.Value()));
        }
        EditorGroupId ignored{};
        (void)editors.MergeAndRemove(binding->group, ignored);
        (void)state.routing.targets.RemoveEditorGroup(binding->group);
        return false;
    }
    group->canvas = renderer::CreateCanvasWindow(
        state.lifetime.instance,
        state.Workspace().windows.window,
        *state.renderer,
        binding->canvas,
        state.routing.targets.CurrentGeneration());
    renderer::CanvasSnapshotSink* sink = renderer::GetCanvasSnapshotSink(group->canvas);
    const bool sink_registered = sink != nullptr
        && state.engine->RegisterSnapshotSink(sink);
    const bool view_created = sink_registered
        && CreateDocumentViewInGroup(state, binding->group, error_owner);
    if (group->canvas == nullptr || sink == nullptr || !sink_registered
        || !view_created) {
        if (state.lifetime.smoke_test) {
            std::fprintf(
                stderr,
                "editor split canvas failed canvas=%d sink=%d registered=%d "
                "view=%d workspace=%llu\n",
                group->canvas != nullptr ? 1 : 0,
                sink != nullptr ? 1 : 0,
                sink_registered ? 1 : 0,
                view_created ? 1 : 0,
                static_cast<unsigned long long>(state.Workspace().id.Value()));
        }
        if (sink != nullptr) {
            (void)state.engine->UnregisterSnapshotSink(sink);
        }
        if (group->canvas != nullptr) {
            DestroyWindow(group->canvas);
        }
        if (group->document_tabs != nullptr) {
            DestroyWindow(group->document_tabs);
        }
        EditorGroupId ignored{};
        (void)editors.MergeAndRemove(binding->group, ignored);
        (void)state.routing.targets.RemoveEditorGroup(binding->group);
        (void)ActivateEditorGroup(state, previous_group);
        inkpod::windows::ui::SyncActiveEditorHandles(
            state.Workspace().windows);
        RelayoutEditorArea(state);
        return false;
    }
    inkpod::windows::ui::SyncActiveEditorHandles(state.Workspace().windows);
    RelayoutEditorArea(state);
    return true;
}

bool MoveActiveViewToOtherGroup(ApplicationHost& state) noexcept {
    auto& editors = state.Workspace().editors;
    auto* source = editors.Active();
    auto* target = source == nullptr ? nullptr : editors.Other(source->id);
    const DocumentViewId view = source == nullptr ? DocumentViewId{} : source->ActiveView();
    if (source == nullptr || target == nullptr || !view) {
        return false;
    }
    const EditorGroupId target_id = target->id;
    renderer::CancelCanvasStroke(source->canvas);
    const bool activated = state.MoveDocumentView(
        view,
        state.Workspace().id,
        target_id,
        target->ViewCount());
    if (activated) {
        UpdateMenuState(state);
    }
    return activated;
}

bool MoveActiveTabBy(ApplicationHost& state, int direction) noexcept {
    EditorGroup* group = state.Workspace().editors.Active();
    const DocumentViewId view = group == nullptr
        ? DocumentViewId{}
        : group->ActiveView();
    const auto source_index = group == nullptr
        ? std::nullopt
        : group->ViewIndex(view);
    if (group == nullptr || !source_index.has_value() || direction == 0) {
        return false;
    }
    std::size_t insertion{};
    if (direction < 0) {
        if (source_index.value() == 0U) {
            return false;
        }
        insertion = source_index.value() - 1U;
    } else {
        if (source_index.value() + 1U >= group->ViewCount()) {
            return false;
        }
        insertion = source_index.value() + 2U;
    }
    const bool moved = state.MoveDocumentView(
        view,
        state.Workspace().id,
        group->id,
        insertion);
    if (moved) {
        UpdateMenuState(state);
    }
    return moved;
}

WorkspaceWindow* NextWorkspace(
    ApplicationHost& state, WorkspaceWindowId source) noexcept {
    const std::size_t count = state.Workspaces().Count();
    if (count <= 1U) {
        return nullptr;
    }
    for (std::size_t index = 0U; index < count; ++index) {
        const WorkspaceWindow* candidate = state.Workspaces().At(index);
        if (candidate != nullptr && candidate->id == source) {
            return state.Workspaces().At((index + 1U) % count);
        }
    }
    return nullptr;
}

bool MoveOrDuplicateViewToNextWorkspace(
    ApplicationHost& state,
    const CommandContext& context,
    bool duplicate) noexcept {
    if (!context.workspace.has_value() || !context.document_view.has_value()) {
        return false;
    }
    const WorkspaceWindowId source = context.workspace.value();
    WorkspaceWindow* target = NextWorkspace(state, source);
    EditorGroup* target_group = target == nullptr
        ? nullptr
        : target->editors.Active();
    if (target_group == nullptr) {
        return false;
    }
    const DocumentViewId view = context.document_view.value();
    const bool transferred = duplicate
        ? state.ActivateDocumentView(view)
            && CreateDocumentViewInGroup(
                state, target_group->id, target->windows.window)
        : state.MoveDocumentView(
            view,
            target->id,
            target_group->id,
            target_group->ViewCount());
    if (!transferred) {
        return false;
    }
    if (state.FindWorkspace(source) != nullptr) {
        (void)state.ActivateWorkspaceWindow(source, false);
        UpdateMenuState(state);
    }
    (void)state.ActivateWorkspaceWindow(target->id, true);
    UpdateMenuState(state);
    return true;
}

bool CloseActiveEditorGroup(ApplicationHost& state) noexcept {
    auto& editors = state.Workspace().editors;
    auto* closing = editors.Active();
    if (closing == nullptr || editors.GroupCount() != 2U || state.engine == nullptr) {
        return false;
    }
    const EditorGroupId closing_id = closing->id;
    const HWND closing_canvas = closing->canvas;
    const HWND closing_tabs = closing->document_tabs;
    renderer::CanvasSnapshotSink* sink = renderer::GetCanvasSnapshotSink(closing_canvas);
    renderer::CancelCanvasStroke(closing_canvas);
    if (sink == nullptr || !state.engine->UnregisterSnapshotSink(sink)) {
        return false;
    }
    EditorGroupId survivor{};
    if (!editors.MergeAndRemove(closing_id, survivor)
        || !state.routing.targets.RemoveEditorGroup(closing_id)) {
        (void)state.engine->RegisterSnapshotSink(sink);
        return false;
    }
    DestroyWindow(closing_canvas);
    DestroyWindow(closing_tabs);
    inkpod::windows::ui::SyncActiveEditorHandles(state.Workspace().windows);
    const auto* active = editors.Active();
    const bool activated = active != nullptr && active->ActiveView()
        ? state.ActivateDocumentView(active->ActiveView())
        : ActivateEditorGroup(state, survivor);
    RelayoutEditorArea(state);
    UpdateMenuState(state);
    return activated;
}

bool RefreshTreePane(ApplicationHost& state) noexcept {
    InkpodDocumentInfo document_info{};
    if (state.engine == nullptr || !QueryDocument(state, document_info)) {
        return false;
    }
    const InkpodEditorStateInfo* editor = PresentedEditorState(state);
    if (editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_layer_id == 0U || editor->active_plane_id == 0U) {
        return false;
    }
    std::vector<TreePaneNode> layers;
    std::vector<TreePaneNode> planes;
    const std::uint64_t requested_layer_id = editor->active_layer_id;
    const std::uint64_t requested_plane_id = editor->active_plane_id;
    std::vector<InkpodEditTarget> edit_targets;
    if (state.engine->GetEditTargets(
            state.Document().id,
            state.Document().generation,
            edit_targets) != INKPOD_STATUS_OK) {
        return false;
    }
    std::uint32_t selected_layer_index{};
    DocumentPanesController controller(*state.engine);
    const InkpodStatus status = controller.LoadTree(
        requested_layer_id,
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Layer),
        layers,
        planes,
        selected_layer_index);
    if (status != INKPOD_STATUS_OK || layers.empty()) {
        state.Workspace().panes.tree_layer_count = 0U;
        state.Workspace().panes.tree_plane_count = 0U;
        layers.clear();
        planes.clear();
        UpdateLayerPaletteDialog(
            state.Workspace().panes.layer_palette,
            layers,
            planes,
            edit_targets,
            0U,
            0U,
            state.Workspace().windows.workspace.layer_split_milli);
        return false;
    }
    const DocumentSession& thumbnail_document = state.Document();
    for (auto& layer : layers) {
        if (layer.thumbnail_bgra.empty()) {
            continue;
        }
        const ThumbnailCacheKey key{
            state.Workspace().pane_ids.layer,
            thumbnail_document.id,
            thumbnail_document.generation,
            layer.id,
            layer.thumbnail_revision == 0U
                ? document_info.document_revision
                : layer.thumbnail_revision,
            ThumbnailKind::Layer};
        if (state.Thumbnails().Put(
                key,
                layer.thumbnail_width,
                layer.thumbnail_height,
                layer.thumbnail_stride_bytes,
                ThumbnailPixelLayout::Bgra8,
                std::move(layer.thumbnail_bgra))) {
            layer.thumbnail_key = key;
        } else {
            layer.thumbnail_width = 0U;
            layer.thumbnail_height = 0U;
            layer.thumbnail_stride_bytes = 0U;
            layer.thumbnail_revision = 0U;
        }
    }
    selected_layer_index = std::min<std::uint32_t>(
        selected_layer_index, static_cast<std::uint32_t>(layers.size() - 1U));
    state.Workspace().panes.active_tree_layer_index = selected_layer_index;
    state.Workspace().panes.active_tree_layer_id = layers[selected_layer_index].id;
    state.Workspace().panes.tree_layer_count = static_cast<std::uint32_t>(layers.size());

    std::uint32_t selected_plane_index{};
    for (std::size_t index = 0U; index < planes.size(); ++index) {
        if (planes[index].id == requested_plane_id) {
            selected_plane_index = static_cast<std::uint32_t>(index);
        }
    }
    state.Workspace().panes.tree_plane_count = static_cast<std::uint32_t>(planes.size());
    std::uint32_t active_plane_kind{};
    if (!planes.empty()) {
        selected_plane_index = std::min<std::uint32_t>(
            selected_plane_index, static_cast<std::uint32_t>(planes.size() - 1U));
        state.Workspace().panes.active_tree_plane_index = selected_plane_index;
        state.Workspace().panes.active_tree_plane_id = planes[selected_plane_index].id;
        active_plane_kind = planes[selected_plane_index].kind;
    } else {
        state.Workspace().panes.active_tree_plane_index = 0U;
        state.Workspace().panes.active_tree_plane_id = 0U;
    }
    UpdateLayerPaletteDialog(
        state.Workspace().panes.layer_palette,
        layers,
        planes,
        edit_targets,
        state.Workspace().panes.active_tree_layer_id,
        state.Workspace().panes.active_tree_plane_id,
        state.Workspace().windows.workspace.layer_split_milli);
    if (IsGeometryCanvasTool(state.Workspace().tools.active_tool)
        && !IsGeometryCanvasPlane(active_plane_kind)) {
        (void)SetEditorActiveTool(state, INKPOD_TOOL_PENCIL);
    }
    return true;
}

bool RefreshLightTablePane(ApplicationHost& state) noexcept {
    using inkpod::windows::ui::panes::LightTablePaneItemView;
    using inkpod::windows::ui::panes::LightTablePaneSetView;
    using inkpod::windows::ui::panes::LightTablePaneView;
    using inkpod::windows::ui::panes::UpdateLightTablePaneDialog;

    LightTablePaneView pane{};
    pane.empty_text = UiText(UiStringId::Text0551);
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.light_table_pane);
    pane.pinned = binding != nullptr
        && binding->policy == PaneTargetPolicy::PinnedDocument;
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.light_table_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    auto* document = target.context.document_session.has_value()
        ? state.Documents().Find(target.context.document_session.value())
        : nullptr;
    std::uint64_t notice_sequence{};
    const PaneTargetNotice notice = state.routing.pane_targets.ConsumeNotice(
        state.routing.light_table_pane, notice_sequence);
    if (notice != PaneTargetNotice::None
        && notice_sequence != state.Workspace().light_table_notice_sequence) {
        state.Workspace().light_table_notice_sequence = notice_sequence;
        if (state.Workspace().light_table_palette != nullptr) {
            NotifyWinEvent(
                EVENT_SYSTEM_ALERT,
                state.Workspace().light_table_palette,
                OBJID_CLIENT,
                CHILDID_SELF);
        }
    }
    if (state.engine == nullptr || target.status != PaneTargetStatus::Ok
        || document == nullptr || !target.context.generation.has_value()) {
        pane.target_text = UiText(notice == PaneTargetNotice::PinnedDocumentClosed
            ? UiStringId::PinnedClosedFollowingNoTarget
            : UiStringId::FollowingNoTarget);
        pane.empty_text = UiText(UiStringId::TargetDocumentUnavailable);
        state.Workspace().panes.light_table_set_count = 0U;
        state.Workspace().panes.light_table_item_count = 0U;
        state.Workspace().panes.active_light_table_set_index = 0U;
        state.Workspace().panes.active_light_table_set_id = 0U;
        state.Workspace().panes.active_light_table_item_index = 0U;
        state.Workspace().panes.active_light_table_item_id = 0U;
        state.Workspace().panes.light_table_selection_session = {};
        state.Workspace().panes.light_table_selection_generation = {};
        UpdateLightTablePaneDialog(
            state.Workspace().light_table_palette, std::move(pane));
        return false;
    }

    std::vector<LightTablePaneSet> sets;
    std::vector<LightTablePaneItem> items;
    DocumentPanesController controller(*state.engine);
    const InkpodStatus status = controller.LoadLightTable(
        document->id, document->generation, sets, items);
    if (status != INKPOD_STATUS_OK || sets.empty()) {
        state.Workspace().panes.light_table_set_count = 0U;
        state.Workspace().panes.light_table_item_count = 0U;
        state.Workspace().panes.active_light_table_set_index = 0U;
        state.Workspace().panes.active_light_table_set_id = 0U;
        state.Workspace().panes.active_light_table_item_index = 0U;
        state.Workspace().panes.active_light_table_item_id = 0U;
        state.Workspace().panes.light_table_selection_session = {};
        state.Workspace().panes.light_table_selection_generation = {};
        pane.target_available = true;
        pane.target_text = UiTextWithUserText(
            pane.pinned ? UiStringId::PinnedPrefix : UiStringId::FollowingPrefix,
            LocatorDocumentName(*document));
        pane.empty_text = UiText(UiStringId::Text0364);
        UpdateLightTablePaneDialog(
            state.Workspace().light_table_palette, std::move(pane));
        return false;
    }

    const bool same_selection_namespace =
        state.Workspace().panes.light_table_selection_session == document->id
        && state.Workspace().panes.light_table_selection_generation
            == document->generation;
    std::uint32_t selected_set{};
    for (std::size_t index = 0; index < sets.size(); ++index) {
        const auto& set = sets[index];
        if ((same_selection_namespace
                && set.id == state.Workspace().panes.active_light_table_set_id)
            || (!same_selection_namespace
                && (set.flags & INKPOD_LIGHT_TABLE_SET_ACTIVE) != 0U)) {
            selected_set = static_cast<std::uint32_t>(index);
        }
    }
    state.Workspace().panes.active_light_table_set_index = selected_set;
    state.Workspace().panes.active_light_table_set_id = sets[selected_set].id;
    state.Workspace().panes.light_table_set_count = static_cast<std::uint32_t>(sets.size());

    std::uint32_t selected_item{};
    for (std::size_t index = 0; index < items.size(); ++index) {
        const auto& item = items[index];
        if (same_selection_namespace
            && item.info.id == state.Workspace().panes.active_light_table_item_id) {
            selected_item = static_cast<std::uint32_t>(index);
        }
    }
    state.Workspace().panes.light_table_item_count = static_cast<std::uint32_t>(items.size());
    if (!items.empty()) {
        state.Workspace().panes.active_light_table_item_index = selected_item;
        state.Workspace().panes.active_light_table_item_id = items[selected_item].info.id;
    } else {
        state.Workspace().panes.active_light_table_item_index = 0U;
        state.Workspace().panes.active_light_table_item_id = 0U;
    }
    state.Workspace().panes.light_table_selection_session = document->id;
    state.Workspace().panes.light_table_selection_generation = document->generation;

    const auto to_wide = [](const std::string& value, std::wstring& output) {
        if (value.empty()) {
            output.clear();
            return true;
        }
        if (value.size() > static_cast<std::size_t>(INT_MAX)) {
            return false;
        }
        const int required = MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            value.data(),
            static_cast<int>(value.size()),
            nullptr,
            0);
        if (required <= 0) {
            return false;
        }
        output.resize(static_cast<std::size_t>(required));
        return MultiByteToWideChar(
                   CP_UTF8,
                   MB_ERR_INVALID_CHARS,
                   value.data(),
                   static_cast<int>(value.size()),
                   output.data(),
                   required)
            == required;
    };
    try {
        pane.target_available = true;
        const std::wstring name = LocatorDocumentName(*document);
        pane.target_text = UiTextWithUserText(
            pane.pinned ? UiStringId::PinnedPrefix : UiStringId::FollowingPrefix,
            name);
        if (notice == PaneTargetNotice::PinnedDocumentClosed) {
            pane.target_text = UiTextWithUserText(
                UiStringId::PinnedClosedFollowingPrefix, name);
        }
        pane.sets.reserve(sets.size());
        for (const auto& set : sets) {
            std::wstring name_text;
            if (!to_wide(set.name, name_text)) {
                return false;
            }
            pane.sets.push_back(LightTablePaneSetView{
                set.id,
                set.opacity_milli,
                set.item_count,
                std::move(name_text)});
        }
        pane.items.reserve(items.size());
        for (const auto& item : items) {
            std::wstring name_text;
            if (!to_wide(item.name, name_text)) {
                return false;
            }
            pane.items.push_back(LightTablePaneItemView{
                item.info.id,
                item.info.flags,
                item.info.opacity_milli,
                item.info.display_mode,
                item.info.translate_x_milli,
                item.info.translate_y_milli,
                std::move(name_text)});
        }
        pane.selected_set_index = selected_set;
        pane.selected_item_index = items.empty() ? UINT32_MAX : selected_item;
    } catch (const std::bad_alloc&) {
        pane = {};
        pane.target_text = UiText(UiStringId::Text0378);
        pane.empty_text = UiText(UiStringId::Text0887);
        UpdateLightTablePaneDialog(
            state.Workspace().light_table_palette, std::move(pane));
        return false;
    }
    UpdateLightTablePaneDialog(
        state.Workspace().light_table_palette, std::move(pane));
    return true;
}

bool LoadCutMemberThumbnail(
    ApplicationHost& state,
    const std::wstring& path,
    CutMemberCache& member) noexcept {
    if (member.width != 0U && member.height != 0U
        && member.thumbnail_width != 0U && member.thumbnail_height != 0U
        && member.thumbnail_stride_bytes == member.thumbnail_width * 4U
        && member.thumbnail_checksum != 0U
        && member.thumbnail_rgba.size()
            == static_cast<std::size_t>(member.thumbnail_stride_bytes)
                * member.thumbnail_height) {
        return true;
    }
    if (state.engine == nullptr) {
        return false;
    }
    std::vector<std::uint8_t> encoded_path;
    if (!WidePathToUtf8(path, encoded_path)) {
        return false;
    }
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    InkpodDocumentThumbnailBuffer thumbnail{};
    thumbnail.struct_size = sizeof(thumbnail);
    std::vector<std::uint8_t> pixels;
    const InkpodStatus status = state.engine->Invoke(
        [&encoded_path, &info, &thumbnail, &pixels, &member](InkpodCore*) {
            const InkpodCoreConfig config{
                sizeof(InkpodCoreConfig),
                INKPOD_ABI_VERSION,
                INKPOD_FEATURE_NONE};
            InkpodCore* probe{};
            InkpodStatus result = inkpod_core_create(&config, &probe);
            if (result == INKPOD_STATUS_OK) {
                result = inkpod_core_open(
                    probe, encoded_path.data(), encoded_path.size(), &info);
            }
            if (result == INKPOD_STATUS_OK
                && (info.cell_id != member.cell_id
                    || info.document_uuid_high != member.document_uuid_high
                    || info.document_uuid_low != member.document_uuid_low)) {
                result = INKPOD_STATUS_INVALID_STATE;
            }
            if (result == INKPOD_STATUS_OK) {
                result = inkpod_core_document_thumbnail_get(probe, &thumbnail);
            }
            if (result == INKPOD_STATUS_OK
                && (thumbnail.required_bytes == 0U
                    || thumbnail.required_bytes > 64U * 64U * 4U
                    || thumbnail.stride_bytes != thumbnail.width * 4U
                    || thumbnail.required_bytes
                        != static_cast<std::uint64_t>(thumbnail.stride_bytes)
                            * thumbnail.height)) {
                result = INKPOD_STATUS_INVALID_STATE;
            }
            if (result == INKPOD_STATUS_OK) {
                try {
                    pixels.resize(
                        static_cast<std::size_t>(thumbnail.required_bytes));
                } catch (const std::bad_alloc&) {
                    result = INKPOD_STATUS_INVALID_STATE;
                }
            }
            if (result == INKPOD_STATUS_OK) {
                thumbnail.pixels_rgba8 = pixels.data();
                thumbnail.pixel_capacity = pixels.size();
                result = inkpod_core_document_thumbnail_get(probe, &thumbnail);
            }
            const InkpodStatus destroy_status = inkpod_core_destroy(&probe);
            return result == INKPOD_STATUS_OK ? destroy_status : result;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    member.width = info.width;
    member.height = info.height;
    member.thumbnail_width = thumbnail.width;
    member.thumbnail_height = thumbnail.height;
    member.thumbnail_stride_bytes = thumbnail.stride_bytes;
    member.thumbnail_checksum = thumbnail.checksum;
    member.thumbnail_rgba = std::move(pixels);
    return true;
}

bool RefreshSequencePane(ApplicationHost& state) noexcept {
    using inkpod::windows::ui::panes::SequencePaneCellView;
    using inkpod::windows::ui::panes::SequencePaneView;
    using inkpod::windows::ui::panes::UpdateSequencePaneDialog;
    state.Thumbnails().RemovePane(state.Workspace().pane_ids.sequence);
    SequencePaneView pane{};
    pane.empty_text = UiText(UiStringId::SequenceNoCells);
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.sequence_pane);
    pane.pinned = binding != nullptr
        && binding->policy == PaneTargetPolicy::PinnedDocument;
    if (state.Workspace().cut.handle != nullptr) {
        pane.target_available = true;
        pane.cut_editable = true;
        try {
            pane.target_text = UiTextWithUserText(
                UiStringId::CutPrefix, state.Workspace().cut.cut_name)
                + L" — "
                + std::to_wstring(state.Workspace().cut.members.size())
                + UiText(UiStringId::CellsSuffix);
            const std::wstring& descriptor = state.Workspace().cut.current_path;
            const std::size_t slash = descriptor.find_last_of(L"\\/");
            const std::wstring directory = slash == std::wstring::npos
                ? std::wstring{}
                : descriptor.substr(0U, slash + 1U);
            pane.cells.reserve(state.Workspace().cut.members.size());
            for (std::size_t index = 0U;
                 index < state.Workspace().cut.members.size();
                 ++index) {
                auto& member = state.Workspace().cut.members[index];
                if (!LoadCutMemberThumbnail(
                        state, directory + member.relative_path, member)) {
                    pane.cells.clear();
                    pane.empty_text = UiText(UiStringId::SequenceThumbnailLoadFailed);
                    UpdateSequencePaneDialog(
                        state.Workspace().sequence_palette, std::move(pane));
                    return false;
                }
                const std::wstring& relative = member.relative_path;
                std::wstring name = relative;
                const std::size_t dot = name.find_last_of(L'.');
                if (dot != std::wstring::npos) {
                    name.erase(dot);
                }
                std::uint64_t content_id = member.cell_id
                    ^ member.document_uuid_high ^ member.document_uuid_low;
                if (content_id == 0U) {
                    content_id = member.cell_id;
                }
                const ThumbnailCacheKey thumbnail_key{
                    state.Workspace().pane_ids.sequence,
                    state.Document().id,
                    state.Document().generation,
                    content_id,
                    member.thumbnail_checksum,
                    ThumbnailKind::Sequence};
                std::vector<std::uint8_t> thumbnail_pixels =
                    member.thumbnail_rgba;
                if (!state.Thumbnails().Put(
                        thumbnail_key,
                        member.thumbnail_width,
                        member.thumbnail_height,
                        member.thumbnail_stride_bytes,
                        ThumbnailPixelLayout::Rgba8,
                        std::move(thumbnail_pixels))) {
                    pane.cells.clear();
                    pane.empty_text = UiText(UiStringId::SequenceThumbnailRegisterFailed);
                    UpdateSequencePaneDialog(
                        state.Workspace().sequence_palette, std::move(pane));
                    return false;
                }
                pane.cells.push_back(SequencePaneCellView{
                    static_cast<std::uint32_t>(index),
                    member.display_number,
                    member.width,
                    member.height,
                    member.thumbnail_width,
                    member.thumbnail_height,
                    member.thumbnail_stride_bytes,
                    member.thumbnail_checksum,
                    std::move(name),
                    thumbnail_key});
            }
            InkpodDocumentInfo active_document{};
            active_document.struct_size = sizeof(active_document);
            const bool has_active_document = QueryDocument(state, active_document);
            if (has_active_document) {
                for (std::size_t index = 0U;
                     index < state.Workspace().cut.members.size();
                     ++index) {
                    const auto& member = state.Workspace().cut.members[index];
                    if (active_document.cell_id == member.cell_id
                        && active_document.document_uuid_high
                            == member.document_uuid_high
                        && active_document.document_uuid_low
                            == member.document_uuid_low) {
                        pane.active_index = static_cast<std::uint32_t>(index);
                        break;
                    }
                }
            }
            if (pane.active_index == UINT32_MAX
                && has_active_document
                && !state.Document().shell.current_path.empty()
                && state.Document().shell.current_path.size() > directory.size()
                && _wcsnicmp(
                       state.Document().shell.current_path.c_str(),
                       directory.c_str(),
                       directory.size()) == 0) {
                pane.target_text += L" — ";
                pane.target_text += UiText(UiStringId::CurrentCellOutsideMembers);
            }
        } catch (const std::bad_alloc&) {
            pane.cells.clear();
            pane.empty_text = UiText(UiStringId::CutDisplayOutOfMemory);
            UpdateSequencePaneDialog(
                state.Workspace().sequence_palette, std::move(pane));
            return false;
        }
        UpdateSequencePaneDialog(
            state.Workspace().sequence_palette, std::move(pane));
        return true;
    }
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.sequence_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    auto* document = target.context.document_session.has_value()
        ? state.Documents().Find(target.context.document_session.value())
        : nullptr;
    std::uint64_t notice_sequence{};
    const PaneTargetNotice notice = state.routing.pane_targets.ConsumeNotice(
        state.routing.sequence_pane, notice_sequence);
    if (notice != PaneTargetNotice::None
        && notice_sequence != state.Workspace().sequence_notice_sequence) {
        state.Workspace().sequence_notice_sequence = notice_sequence;
        if (state.Workspace().sequence_palette != nullptr) {
            NotifyWinEvent(
                EVENT_SYSTEM_ALERT,
                state.Workspace().sequence_palette,
                OBJID_CLIENT,
                CHILDID_SELF);
        }
    }
    if (state.engine == nullptr || target.status != PaneTargetStatus::Ok
        || document == nullptr || !target.context.generation.has_value()) {
        pane.target_text = UiText(notice == PaneTargetNotice::PinnedDocumentClosed
            ? UiStringId::PinnedClosedFollowingNoTarget
            : UiStringId::FollowingNoTarget);
        pane.empty_text = UiText(UiStringId::TargetDocumentUnavailable);
        UpdateSequencePaneDialog(
            state.Workspace().sequence_palette, std::move(pane));
        return false;
    }

    std::vector<SequencePaneCell> cells;
    DocumentPanesController controller(*state.engine);
    const InkpodStatus status = controller.LoadSequence(
        document->id, document->generation, cells);
    pane.target_available = true;
    try {
        const std::wstring name = LocatorDocumentName(*document);
        pane.target_text = UiTextWithUserText(
            pane.pinned ? UiStringId::PinnedPrefix : UiStringId::FollowingPrefix,
            name);
        if (notice == PaneTargetNotice::PinnedDocumentClosed) {
            pane.target_text = UiTextWithUserText(
                UiStringId::PinnedClosedFollowingPrefix, name);
        }
        if (status != INKPOD_STATUS_OK) {
            pane.empty_text = UiText(UiStringId::SequenceLoadFailed);
            UpdateSequencePaneDialog(
                state.Workspace().sequence_palette, std::move(pane));
            return false;
        }
        pane.target_text += L" — " + std::to_wstring(cells.size())
            + UiText(UiStringId::CellsSuffix);
        InkpodDocumentInfo info{};
        info.struct_size = sizeof(info);
        if (!state.engine->GetDocumentInfo(
                document->id, document->generation, info)) {
            pane.empty_text = UiText(UiStringId::Text0627);
            UpdateSequencePaneDialog(
                state.Workspace().sequence_palette, std::move(pane));
            return false;
        }
        pane.cells.reserve(cells.size());
        for (auto& cell : cells) {
            std::wstring wide_name;
            if (!cell.name.empty()) {
                if (cell.name.size() > static_cast<std::size_t>(INT_MAX)) {
                    return false;
                }
                const int required = MultiByteToWideChar(
                    CP_UTF8,
                    MB_ERR_INVALID_CHARS,
                    cell.name.data(),
                    static_cast<int>(cell.name.size()),
                    nullptr,
                    0);
                if (required <= 0) {
                    return false;
                }
                wide_name.resize(static_cast<std::size_t>(required));
                if (MultiByteToWideChar(
                        CP_UTF8,
                        MB_ERR_INVALID_CHARS,
                        cell.name.data(),
                        static_cast<int>(cell.name.size()),
                        wide_name.data(),
                        required) != required) {
                    return false;
                }
            }
            if (cell.info.document_uuid_high == info.document_uuid_high
                && cell.info.document_uuid_low == info.document_uuid_low) {
                pane.active_index = static_cast<std::uint32_t>(pane.cells.size());
            }
            if (!cell.thumbnail_rgba.empty()) {
                const ThumbnailCacheKey thumbnail_key{
                    state.Workspace().pane_ids.sequence,
                    document->id,
                    document->generation,
                    static_cast<std::uint64_t>(cell.info.sequence_index) + 1U,
                    cell.info.thumbnail_checksum == 0U
                        ? info.document_revision
                        : cell.info.thumbnail_checksum,
                    ThumbnailKind::Sequence};
                if (state.Thumbnails().Put(
                        thumbnail_key,
                        cell.info.thumbnail_width,
                        cell.info.thumbnail_height,
                        cell.thumbnail_stride_bytes,
                        ThumbnailPixelLayout::Rgba8,
                        std::move(cell.thumbnail_rgba))) {
                    cell.thumbnail_key = thumbnail_key;
                }
            }
            pane.cells.push_back(SequencePaneCellView{
                static_cast<std::uint32_t>(cell.info.sequence_index),
                cell.info.cell_number,
                cell.info.width,
                cell.info.height,
                cell.info.thumbnail_width,
                cell.info.thumbnail_height,
                cell.thumbnail_stride_bytes,
                cell.info.thumbnail_checksum,
                std::move(wide_name),
                cell.thumbnail_key});
        }
        if (document->id == state.routing.targets.DocumentSession()) {
            state.Workspace().panes.sequence_count =
                static_cast<std::uint32_t>(pane.cells.size());
            state.Workspace().animation.active_sequence_index =
                pane.active_index == UINT32_MAX ? 0U : pane.active_index;
            state.Workspace().animation.active_sequence_name =
                pane.active_index == UINT32_MAX
                    ? std::wstring{}
                    : pane.cells[pane.active_index].name;
        }
    } catch (const std::bad_alloc&) {
        pane.cells.clear();
        pane.empty_text = UiText(UiStringId::SequenceDisplayOutOfMemory);
        UpdateSequencePaneDialog(
            state.Workspace().sequence_palette, std::move(pane));
        return false;
    }
    UpdateSequencePaneDialog(
        state.Workspace().sequence_palette, std::move(pane));
    return true;
}

void ResetSubpaletteTarget(ApplicationHost& state) noexcept {
    auto& workspace = state.Workspace();
    if (workspace.subpalette_dialog.canvas != nullptr) {
        renderer::CancelCanvasStroke(workspace.subpalette_dialog.canvas);
        (void)renderer::UnbindCanvasSnapshotSink(
            workspace.subpalette_dialog.canvas);
    }
    if (state.engine != nullptr && workspace.subpalette_core_view_id != 0U
        && workspace.subpalette_session
        && workspace.subpalette_document_generation) {
        const std::uint64_t view_id = workspace.subpalette_core_view_id;
        (void)state.engine->Invoke(
            workspace.subpalette_session,
            workspace.subpalette_document_generation,
            [view_id](InkpodCore* core) {
                return inkpod_core_view_close(core, view_id);
            },
            false,
            false);
    }
    workspace.subpalette_session = {};
    workspace.subpalette_document_view = {};
    workspace.subpalette_document_generation = {};
    workspace.subpalette_core_view_id = 0U;
    workspace.subpalette_snapshot_revision = 0U;
    workspace.subpalette_source_count = 0U;
    workspace.subpalette_active_index = 0U;
}

bool PublishSubpaletteSnapshot(ApplicationHost& state) noexcept {
    auto& workspace = state.Workspace();
    auto* sink = renderer::GetCanvasSnapshotSink(
        workspace.subpalette_dialog.canvas);
    if (state.engine == nullptr || sink == nullptr
        || workspace.subpalette_core_view_id == 0U
        || !workspace.subpalette_session
        || !workspace.subpalette_document_generation) {
        return false;
    }
    InkpodSnapshot* snapshot{};
    InkpodSnapshotView snapshot_view{};
    snapshot_view.struct_size = sizeof(snapshot_view);
    InkpodSnapshotTransform transform{};
    transform.struct_size = sizeof(transform);
    const std::uint64_t view_id = workspace.subpalette_core_view_id;
    const InkpodStatus status = state.engine->Invoke(
        workspace.subpalette_session,
        workspace.subpalette_document_generation,
        [view_id, &snapshot, &snapshot_view, &transform](InkpodCore* core) {
            const InkpodSnapshotOptions options{
                sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodStatus inner = inkpod_core_subpalette_build_snapshot(
                core, view_id, &options, &snapshot);
            if (inner == INKPOD_STATUS_OK) {
                inner = inkpod_snapshot_get_view(snapshot, &snapshot_view);
            }
            if (inner == INKPOD_STATUS_OK) {
                inner = inkpod_snapshot_get_transform(snapshot, &transform);
            }
            if (inner != INKPOD_STATUS_OK) {
                (void)inkpod_snapshot_release(&snapshot);
            }
            return inner;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK || snapshot == nullptr) {
        return false;
    }
    ++workspace.subpalette_snapshot_revision;
    return sink->Submit(renderer::SnapshotEnvelope{
        sink->Route(), snapshot_view.revision, transform.view_revision, snapshot});
}

bool RefreshSubpalettePane(ApplicationHost& state) noexcept {
    using inkpod::windows::ui::panes::SubpalettePaneView;
    using inkpod::windows::ui::panes::UpdateSubpalettePaneDialog;

    auto& workspace = state.Workspace();
    SubpalettePaneView pane{};
    pane.auto_previous = workspace.subpalette_auto_previous;
    pane.scroll_sync = workspace.subpalette_scroll_sync;
    pane.empty_text = UiText(UiStringId::Text0547);
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.subpalette_pane);
    pane.pinned = binding != nullptr
        && binding->policy == PaneTargetPolicy::PinnedDocument;
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.subpalette_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    auto* document = target.context.document_session.has_value()
        ? state.Documents().Find(target.context.document_session.value())
        : nullptr;
    std::uint64_t notice_sequence{};
    const PaneTargetNotice notice = state.routing.pane_targets.ConsumeNotice(
        state.routing.subpalette_pane, notice_sequence);
    if (notice != PaneTargetNotice::None
        && notice_sequence != workspace.subpalette_notice_sequence) {
        workspace.subpalette_notice_sequence = notice_sequence;
        if (workspace.subpalette_palette != nullptr) {
            NotifyWinEvent(
                EVENT_SYSTEM_ALERT,
                workspace.subpalette_palette,
                OBJID_CLIENT,
                CHILDID_SELF);
        }
    }
    if (state.engine == nullptr || target.status != PaneTargetStatus::Ok
        || document == nullptr || !target.context.document_view.has_value()
        || !target.context.generation.has_value()) {
        ResetSubpaletteTarget(state);
        pane.target_text = UiText(notice == PaneTargetNotice::PinnedDocumentClosed
            ? UiStringId::PinnedClosedFollowingNoTarget
            : UiStringId::FollowingNoTarget);
        pane.source_text = UiText(UiStringId::Text0546);
        pane.empty_text = UiText(UiStringId::TargetDocumentUnavailable);
        UpdateSubpalettePaneDialog(
            workspace.subpalette_palette, std::move(pane));
        return false;
    }

    std::vector<SequencePaneCell> cells;
    DocumentPanesController controller(*state.engine);
    const InkpodStatus sequence_status = controller.LoadSequence(
        document->id, document->generation, cells);
    pane.target_available = true;
    const std::wstring document_name = LocatorDocumentName(*document);
    pane.target_text = UiTextWithUserText(
        pane.pinned ? UiStringId::PinnedPrefix : UiStringId::FollowingPrefix,
        document_name);
    if (notice == PaneTargetNotice::PinnedDocumentClosed) {
        pane.target_text = UiTextWithUserText(
            UiStringId::PinnedClosedFollowingPrefix, document_name);
    }
    if (sequence_status != INKPOD_STATUS_OK || cells.empty()) {
        ResetSubpaletteTarget(state);
        pane.source_text = UiText(UiStringId::Text0546);
        pane.empty_text = sequence_status == INKPOD_STATUS_OK
            ? UiText(UiStringId::Text0205)
            : UiText(UiStringId::SequenceLoadFailed);
        UpdateSubpalettePaneDialog(
            workspace.subpalette_palette, std::move(pane));
        return false;
    }

    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    if (!state.engine->GetDocumentInfo(
            document->id, document->generation, info)) {
        pane.source_text = UiText(UiStringId::Text0546);
        pane.empty_text = UiText(UiStringId::Text0627);
        UpdateSubpalettePaneDialog(
            workspace.subpalette_palette, std::move(pane));
        return false;
    }
    std::uint32_t active_index{};
    for (std::size_t index = 0U; index < cells.size(); ++index) {
        if (cells[index].info.document_uuid_high == info.document_uuid_high
            && cells[index].info.document_uuid_low == info.document_uuid_low) {
            active_index = static_cast<std::uint32_t>(index);
            break;
        }
    }
    const bool same_target = workspace.subpalette_session == document->id
        && workspace.subpalette_document_view
            == target.context.document_view.value()
        && workspace.subpalette_document_generation == document->generation;
    if (!same_target) {
        ResetSubpaletteTarget(state);
        workspace.subpalette_source_index =
            workspace.subpalette_auto_previous && active_index != 0U
            ? active_index - 1U
            : active_index;
    }
    workspace.subpalette_source_count = static_cast<std::uint32_t>(cells.size());
    workspace.subpalette_active_index = active_index;
    workspace.subpalette_source_index = std::min<std::uint32_t>(
        workspace.subpalette_source_index,
        workspace.subpalette_source_count - 1U);

    if (workspace.subpalette_core_view_id == 0U) {
        std::uint64_t view_id{};
        const std::uint32_t source_index = workspace.subpalette_source_index;
        const InkpodStatus create_status = state.engine->Invoke(
            document->id,
            document->generation,
            [source_index, &view_id](InkpodCore* core) {
                InkpodStatus inner = inkpod_core_subpalette_set(
                    core, source_index);
                if (inner == INKPOD_STATUS_OK) {
                    inner = inkpod_core_view_create(core, &view_id);
                }
                return inner;
            },
            false,
            false);
        if (create_status != INKPOD_STATUS_OK || view_id == 0U
            || workspace.subpalette_dialog.canvas == nullptr
            || !renderer::BindCanvasSnapshotSink(
                workspace.subpalette_dialog.canvas,
                document->id,
                target.context.document_view.value(),
                document->generation)) {
            if (view_id != 0U) {
                (void)state.engine->Invoke(
                    document->id,
                    document->generation,
                    [view_id](InkpodCore* core) {
                        return inkpod_core_view_close(core, view_id);
                    },
                    false,
                    false);
            }
            pane.source_text = UiText(UiStringId::Text0546);
            pane.empty_text = UiText(UiStringId::Text0548);
            UpdateSubpalettePaneDialog(
                workspace.subpalette_palette, std::move(pane));
            return false;
        }
        workspace.subpalette_session = document->id;
        workspace.subpalette_document_view =
            target.context.document_view.value();
        workspace.subpalette_document_generation = document->generation;
        workspace.subpalette_core_view_id = view_id;
        RECT client{};
        if (GetClientRect(workspace.subpalette_dialog.canvas, &client) != FALSE) {
            const InkpodViewInput resize{
                sizeof(InkpodViewInput),
                INKPOD_VIEW_VIEWPORT_RESIZED,
                0U,
                static_cast<double>(client.right - client.left),
                static_cast<double>(client.bottom - client.top),
                0.0,
                0.0};
            const InkpodViewInput fit{
                sizeof(InkpodViewInput), INKPOD_VIEW_FIT, 0U, 0.0, 0.0, 0.0, 0.0};
            (void)state.engine->Invoke(
                document->id,
                document->generation,
                [view_id, resize, fit](InkpodCore* core) {
                    InkpodStatus inner = inkpod_core_subpalette_view_apply(
                        core, view_id, &resize);
                    return inner == INKPOD_STATUS_OK
                        ? inkpod_core_subpalette_view_apply(
                              core, view_id, &fit)
                        : inner;
                },
                false,
                false);
        }
    } else {
        const std::uint32_t source_index = workspace.subpalette_source_index;
        (void)state.engine->Invoke(
            document->id,
            document->generation,
            [source_index](InkpodCore* core) {
                return inkpod_core_subpalette_set(core, source_index);
            },
            false,
            false);
    }

    pane.source_available = PublishSubpaletteSnapshot(state);
    try {
        const auto& source = cells[workspace.subpalette_source_index];
        std::wstring source_name;
        if (!source.name.empty()) {
            const int required = MultiByteToWideChar(
                CP_UTF8,
                MB_ERR_INVALID_CHARS,
                source.name.data(),
                static_cast<int>(source.name.size()),
                nullptr,
                0);
            if (required > 0) {
                source_name.resize(static_cast<std::size_t>(required));
                (void)MultiByteToWideChar(
                    CP_UTF8,
                    MB_ERR_INVALID_CHARS,
                    source.name.data(),
                    static_cast<int>(source.name.size()),
                    source_name.data(),
                    required);
            }
        }
        pane.source_text = UiText(UiStringId::Text0545)
            + std::to_wstring(source.info.cell_number)
            + (source_name.empty() ? L"" : L" — " + source_name);
        pane.empty_text = pane.source_available
            ? L""
            : UiText(UiStringId::Text0552);
    } catch (const std::bad_alloc&) {
        pane.source_available = false;
        pane.source_text = UiText(UiStringId::Text0546);
        pane.empty_text = UiText(UiStringId::Text0553);
    }
    UpdateSubpalettePaneDialog(
        workspace.subpalette_palette, std::move(pane));
    return workspace.subpalette_dialog.view.source_available;
}

void DispatchSubpalettePaneCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        (void)IssueCommand(
            state,
            state->Workspace().windows.window,
            command,
            0,
            state->routing.subpalette_pane);
    }
}

void ApplySubpaletteViewInput(
    ApplicationHost& state, const InkpodViewInput& input) noexcept {
    auto& workspace = state.Workspace();
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.subpalette_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    if (state.engine == nullptr || target.status != PaneTargetStatus::Ok
        || !target.context.document_session.has_value()
        || !target.context.document_view.has_value()
        || !target.context.generation.has_value()
        || target.context.document_session.value()
            != workspace.subpalette_session
        || target.context.document_view.value()
            != workspace.subpalette_document_view
        || target.context.generation.value()
            != workspace.subpalette_document_generation
        || workspace.subpalette_core_view_id == 0U) {
        (void)RefreshSubpalettePane(state);
        return;
    }
    auto* document = state.Documents().Find(workspace.subpalette_session);
    auto* view = document == nullptr
        ? nullptr
        : document->FindView(workspace.subpalette_document_view);
    const std::uint64_t reference_view_id = workspace.subpalette_core_view_id;
    const std::uint64_t editor_view_id = view == nullptr
        ? 0U
        : view->presentation.active_view_id;
    const bool synchronize = workspace.subpalette_scroll_sync && view != nullptr;
    const InkpodStatus status = state.engine->Invoke(
        workspace.subpalette_session,
        workspace.subpalette_document_generation,
        [reference_view_id, editor_view_id, synchronize, input](InkpodCore* core) {
            InkpodStatus inner = inkpod_core_subpalette_view_apply(
                core, reference_view_id, &input);
            if (inner == INKPOD_STATUS_OK && synchronize) {
                if (editor_view_id == 0U) {
                    InkpodDocumentInfo ignored{};
                    ignored.struct_size = sizeof(ignored);
                    inner = inkpod_core_apply_view(core, &input, &ignored);
                } else {
                    inner = inkpod_core_view_apply(core, editor_view_id, &input);
                }
            }
            return inner;
        },
        synchronize,
        synchronize);
    if (status == INKPOD_STATUS_OK) {
        (void)PublishSubpaletteSnapshot(state);
    }
}

void PerformSubpalettePaneAction(
    void* context,
    inkpod::windows::ui::panes::SubpalettePaneAction action) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    auto& workspace = state->Workspace();
    switch (action) {
        case inkpod::windows::ui::panes::SubpalettePaneAction::Previous:
            if (workspace.subpalette_source_index != 0U) {
                --workspace.subpalette_source_index;
            }
            break;
        case inkpod::windows::ui::panes::SubpalettePaneAction::Next:
            if (workspace.subpalette_source_index + 1U
                < workspace.subpalette_source_count) {
                ++workspace.subpalette_source_index;
            }
            break;
        case inkpod::windows::ui::panes::SubpalettePaneAction::Current:
            workspace.subpalette_source_index = workspace.subpalette_active_index;
            break;
        case inkpod::windows::ui::panes::SubpalettePaneAction::Fit: {
            const InkpodViewInput input{
                sizeof(InkpodViewInput), INKPOD_VIEW_FIT, 0U, 0.0, 0.0, 0.0, 0.0};
            ApplySubpaletteViewInput(*state, input);
            return;
        }
        case inkpod::windows::ui::panes::SubpalettePaneAction::OneToOne: {
            const InkpodViewInput input{
                sizeof(InkpodViewInput),
                INKPOD_VIEW_ONE_TO_ONE,
                0U,
                0.0,
                0.0,
                0.0,
                0.0};
            ApplySubpaletteViewInput(*state, input);
            return;
        }
        case inkpod::windows::ui::panes::SubpalettePaneAction::ToggleAutoPrevious:
            workspace.subpalette_auto_previous =
                !workspace.subpalette_auto_previous;
            if (workspace.subpalette_auto_previous) {
                workspace.subpalette_source_index =
                    workspace.subpalette_active_index == 0U
                    ? 0U
                    : workspace.subpalette_active_index - 1U;
            }
            break;
        case inkpod::windows::ui::panes::SubpalettePaneAction::ToggleScrollSync:
            workspace.subpalette_scroll_sync =
                !workspace.subpalette_scroll_sync;
            break;
    }
    (void)RefreshSubpalettePane(*state);
}

void SampleSubpalettePane(void* context, double x, double y) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->engine == nullptr) {
        return;
    }
    auto& workspace = state->Workspace();
    const PaneActionTarget target = state->routing.pane_targets.CaptureAction(
        state->routing.subpalette_pane,
        state->routing.targets.Capture(),
        state->routing.targets);
    if (target.status != PaneTargetStatus::Ok
        || !target.context.document_session.has_value()
        || !target.context.generation.has_value()
        || target.context.document_session.value()
            != workspace.subpalette_session
        || target.context.generation.value()
            != workspace.subpalette_document_generation
        || workspace.subpalette_core_view_id == 0U) {
        return;
    }
    InkpodColorValue color{};
    color.struct_size = sizeof(color);
    const std::uint64_t view_id = workspace.subpalette_core_view_id;
    const InkpodStatus status = state->engine->Invoke(
        workspace.subpalette_session,
        workspace.subpalette_document_generation,
        [view_id, x, y, &color](InkpodCore* core) {
            return inkpod_core_subpalette_view_sample(
                core, view_id, x, y, &color);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        SetDrawingColor(*state, color);
        (void)RefreshColorPanes(*state);
    }
}

void ApplySubpalettePaneView(
    void* context,
    const inkpod::renderer::CanvasViewGesture& gesture) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    const InkpodViewInput input{
        sizeof(InkpodViewInput),
        gesture.kind,
        0U,
        gesture.value1,
        gesture.value2,
        gesture.value3,
        0.0};
    ApplySubpaletteViewInput(*state, input);
}

void QueueLocatorSample(ApplicationHost& state) noexcept {
    const PaneActionTarget pane_target = state.routing.pane_targets.CaptureAction(
        state.routing.locator_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    auto* document = pane_target.context.document_session.has_value()
        ? state.Documents().Find(pane_target.context.document_session.value())
        : nullptr;
    auto* view = document != nullptr && pane_target.context.document_view.has_value()
        ? document->FindView(pane_target.context.document_view.value())
        : nullptr;
    if (state.engine == nullptr || pane_target.status != PaneTargetStatus::Ok
        || view == nullptr) {
        return;
    }
    if (state.routing.locator_pending_token.load(
            std::memory_order_acquire) != 0U) {
        state.routing.locator_latest_requested = true;
        return;
    }
    state.routing.locator_latest_requested = false;
    std::shared_ptr<LocatorAsyncResult> result;
    try {
        result = std::make_shared<LocatorAsyncResult>();
    } catch (const std::bad_alloc&) {
        return;
    }
    const CommandContext context = pane_target.context;
    if (state.routing.targets.Resolve(
            context, inkpod::app::kDocumentViewCommandScope)
        != CommandResolveStatus::Ok) {
        return;
    }
    result->token = state.routing.tokens.IssueNotification(
        state.routing.targets.CurrentGeneration());
    result->context = context;
    result->sample_generation = view->presentation.locator_generation;
    result->output.struct_size = sizeof(InkpodLocatorOutput);
    result->neighborhood_output.struct_size = sizeof(InkpodLocatorNeighborhoodBuffer);
    result->neighborhood_output.radius = 4U;
    result->neighborhood_output.pixels_rgba8 = result->neighborhood.data();
    result->neighborhood_output.pixel_capacity = result->neighborhood.size();
    const std::uint64_t view_id = view->presentation.active_view_id;
    const double device_x = static_cast<double>(view->presentation.pointer_device_x);
    const double device_y = static_cast<double>(view->presentation.pointer_device_y);
    const HWND window = state.Workspace().windows.window;
    state.routing.locator_pending_token.store(
        result->token.value, std::memory_order_release);
    if (!state.engine->Enqueue(
            context,
            [result, view_id, device_x, device_y](InkpodCore* core) {
                const InkpodStatus sample = inkpod_core_locator_sample(
                    core, view_id, device_x, device_y, &result->output);
                return sample == INKPOD_STATUS_OK
                    ? inkpod_core_locator_neighborhood(
                          core,
                          view_id,
                          device_x,
                          device_y,
                          &result->neighborhood_output)
                    : sample;
            },
            false,
            false,
            false,
            [result, window, routing = &state.routing](InkpodStatus status) {
                result->status = status;
                result->neighborhood_output.pixels_rgba8 = nullptr;
                result->neighborhood_output.pixel_capacity = 0U;
                {
                    std::lock_guard lock(routing->locator_results_mutex);
                    const auto slot = std::find_if(
                        routing->locator_results.begin(),
                        routing->locator_results.end(),
                        [](const auto& pending) { return !pending.has_value(); });
                    if (slot == routing->locator_results.end()) {
                        std::uint64_t expected = result->token.value;
                        (void)routing->locator_pending_token.compare_exchange_strong(
                            expected, 0U, std::memory_order_acq_rel);
                        return;
                    }
                    *slot = *result;
                }
                if (window == nullptr) {
                    return;
                }
                if (PostMessageW(
                        window,
                        kLocatorSampleReady,
                        static_cast<WPARAM>(result->token.value),
                        static_cast<LPARAM>(result->token.generation.Value()))
                    == FALSE) {
                    std::lock_guard lock(routing->locator_results_mutex);
                    const auto found = std::find_if(
                        routing->locator_results.begin(),
                        routing->locator_results.end(),
                        [token = result->token](const auto& pending) {
                            return pending.has_value()
                                && pending->token.value == token.value
                                && pending->token.generation == token.generation;
                        });
                    if (found != routing->locator_results.end()) {
                        found->reset();
                    }
                    std::uint64_t expected = result->token.value;
                    (void)routing->locator_pending_token.compare_exchange_strong(
                        expected, 0U, std::memory_order_acq_rel);
                }
            })) {
        std::uint64_t expected = result->token.value;
        (void)state.routing.locator_pending_token.compare_exchange_strong(
            expected, 0U, std::memory_order_acq_rel);
    }
}

std::optional<std::int32_t> LocatorDeviceCoordinate(float value) noexcept {
    if (!std::isfinite(value)) {
        return std::nullopt;
    }
    const double bounded = std::clamp(
        static_cast<double>(value),
        static_cast<double>(INT32_MIN),
        static_cast<double>(INT32_MAX));
    return static_cast<std::int32_t>(std::round(bounded));
}

void TrackAcceptedStrokePointer(
    ApplicationHost& state,
    const inkpod::app::EditorGroup& source_group,
    inkpod::app::DocumentView& view,
    const inkpod::renderer::CanvasStrokeEvent& input) noexcept {
    const bool interaction_ended =
        input.kind == inkpod::renderer::CanvasStrokeEventKind::End
        || input.kind == inkpod::renderer::CanvasStrokeEventKind::Cancel;
    bool pointer_updated{};
    if (input.samples != nullptr && input.sample_count != 0U) {
        const InkpodStrokeSample& latest = input.samples[
            static_cast<std::size_t>(input.sample_count - 1U)];
        const auto x = LocatorDeviceCoordinate(latest.x);
        const auto y = LocatorDeviceCoordinate(latest.y);
        if (x.has_value() && y.has_value()) {
            view.presentation.pointer_device_x = x.value();
            view.presentation.pointer_device_y = y.value();
            pointer_updated = true;
        }
    }
    if (!pointer_updated && !interaction_ended) {
        return;
    }

    // EnqueueStroke has already accepted the matching Begin/Append/End/Cancel
    // work. Advancing the generation here therefore keeps every locator query
    // behind the preview transition it samples. End/Cancel also invalidate an
    // older in-flight result even when they carry no final pointer sample.
    ++view.presentation.locator_generation;
    if (&source_group == state.Workspace().editors.Active()) {
        QueueLocatorSample(state);
    }
}

std::wstring LocatorDocumentName(const DocumentSession& document) {
    const std::wstring& path = !document.shell.current_path.empty()
        ? document.shell.current_path
        : document.shell.source_path;
    if (!path.empty()) {
        const std::size_t separator = path.find_last_of(L"\\/");
        const std::wstring leaf = separator == std::wstring::npos
            ? path
            : path.substr(separator + 1U);
        if (!leaf.empty()) {
            return leaf;
        }
    }
    const wchar_t* prefix = UiText(UiStringId::Text0777);
    return prefix + std::to_wstring(
        document.untitled_number == 0U ? 1U : document.untitled_number);
}

void RefreshLocatorPane(ApplicationHost& state) noexcept {
    using inkpod::windows::ui::panes::LocatorPaneView;
    using inkpod::windows::ui::panes::UpdateLocatorPaneDialog;
    LocatorPaneView pane{};
    pane.fixed_mode = state.Workspace().locator_fixed_mode;
    pane.auto_scroll = state.Workspace().locator_auto_scroll;

    const auto* binding = state.routing.pane_targets.Find(
        state.routing.locator_pane);
    pane.pinned = binding != nullptr
        && binding->policy == PaneTargetPolicy::PinnedDocument;
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.locator_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    auto* document = target.context.document_session.has_value()
        ? state.Documents().Find(target.context.document_session.value())
        : nullptr;
    auto* view = document != nullptr && target.context.document_view.has_value()
        ? document->FindView(target.context.document_view.value())
        : nullptr;

    std::uint64_t notice_sequence{};
    const PaneTargetNotice notice = state.routing.pane_targets.ConsumeNotice(
        state.routing.locator_pane, notice_sequence);
    if (notice != PaneTargetNotice::None
        && notice_sequence != state.Workspace().locator_notice_sequence) {
        state.Workspace().locator_notice_sequence = notice_sequence;
        if (state.Workspace().locator_palette != nullptr) {
            NotifyWinEvent(
                EVENT_SYSTEM_ALERT,
                state.Workspace().locator_palette,
                OBJID_CLIENT,
                CHILDID_SELF);
        }
    }

    try {
        if (target.status != PaneTargetStatus::Ok || document == nullptr
            || view == nullptr) {
            pane.target_text = UiText(notice == PaneTargetNotice::PinnedDocumentClosed
                ? UiStringId::PinnedClosedFollowingNoTarget
                : UiStringId::FollowingNoTarget);
            pane.coordinate_text = L"X —  Y —";
            pane.selection_text = std::wstring(UiText(UiStringId::LocatorSelectionPrefix))
                + L"H —  V —  L —";
            pane.color_text = L"RGBA —";
            UpdateLocatorPaneDialog(state.Workspace().locator_palette, pane);
            return;
        }
        const std::wstring name = LocatorDocumentName(*document);
        pane.target_text = UiTextWithUserText(
            pane.pinned ? UiStringId::PinnedPrefix : UiStringId::FollowingPrefix,
            name);
        InkpodDocumentInfo target_info{};
        target_info.struct_size = sizeof(target_info);
        if (state.engine != nullptr && state.engine->GetDocumentInfo(
                document->id, document->generation, target_info)) {
            pane.target_text += L" — ";
            pane.target_text += UiText(
                target_info.active_plane == INKPOD_PLANE_COLOR
                    ? UiStringId::Coloring
                    : UiStringId::MainLine);
        }
        if (notice == PaneTargetNotice::PinnedDocumentClosed) {
            pane.target_text = UiTextWithUserText(
                UiStringId::PinnedClosedFollowingPrefix, name);
        }
        pane.valid = view->presentation.locator_valid;
        if (!pane.valid) {
            pane.coordinate_text = L"X —  Y —";
            pane.selection_text = std::wstring(UiText(UiStringId::LocatorSelectionPrefix))
                + L"H —  V —  L —";
            pane.color_text = L"RGBA —";
        } else {
            const InkpodLocatorOutput& locator = view->presentation.locator;
            pane.coordinate_text = L"X " + std::to_wstring(locator.document_x)
                + L"  Y " + std::to_wstring(locator.document_y);
            if ((locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U) {
                const double diagonal = std::hypot(
                    static_cast<double>(locator.selection.width),
                    static_cast<double>(locator.selection.height));
                std::array<wchar_t, 96U> text{};
                _snwprintf_s(
                    text.data(), text.size(), _TRUNCATE,
                    L"%lsH %d  V %d  L %.1f",
                    UiText(UiStringId::LocatorSelectionPrefix),
                    locator.selection.width,
                    locator.selection.height,
                    diagonal);
                pane.selection_text = text.data();
            } else {
                pane.selection_text = UiText(UiStringId::LocatorNoSelection);
            }
            if ((locator.flags & INKPOD_LOCATOR_COLOR_PRESENT) != 0U) {
                pane.color_text = L"RGBA "
                    + std::to_wstring(locator.color.red) + L" / "
                    + std::to_wstring(locator.color.green) + L" / "
                    + std::to_wstring(locator.color.blue) + L" / "
                    + std::to_wstring(locator.color.alpha)
                    + (locator.color.depth == INKPOD_COLOR_DEPTH_16
                           ? L" (16-bit)"
                           : L" (8-bit)");
            } else {
                pane.color_text = L"RGBA —";
            }
            pane.neighborhood_width =
                view->presentation.locator_neighborhood_width;
            pane.neighborhood_height =
                view->presentation.locator_neighborhood_height;
            pane.neighborhood_origin_x =
                view->presentation.locator_neighborhood_origin_x;
            pane.neighborhood_origin_y =
                view->presentation.locator_neighborhood_origin_y;
            pane.neighborhood = view->presentation.locator_neighborhood;
        }
    } catch (const std::bad_alloc&) {
        pane = {};
        pane.target_text = UiText(UiStringId::Text0412);
    }
    UpdateLocatorPaneDialog(state.Workspace().locator_palette, pane);
}

void DispatchLocatorPaneCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        (void)DispatchEnabledCommand(
            *state,
            state->Workspace().windows.window,
            command,
            state->routing.locator_pane);
    }
}

void SelectLocatorPixel(
    void* context,
    std::int32_t document_x,
    std::int32_t document_y) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->engine == nullptr
        || !state->Workspace().locator_fixed_mode) {
        return;
    }
    const PaneActionTarget target = state->routing.pane_targets.CaptureAction(
        state->routing.locator_pane,
        state->routing.targets.Capture(),
        state->routing.targets);
    if (target.status != PaneTargetStatus::Ok) {
        return;
    }
    auto* document = target.context.document_session.has_value()
        ? state->Documents().Find(target.context.document_session.value())
        : nullptr;
    auto* view = document != nullptr && target.context.document_view.has_value()
        ? document->FindView(target.context.document_view.value())
        : nullptr;
    if (view == nullptr) {
        return;
    }
    InkpodDocumentInfo document_info{};
    document_info.struct_size = sizeof(document_info);
    if (!state->engine->GetDocumentInfo(
            document->id, document->generation, document_info)) {
        return;
    }
    double pan_x{};
    double pan_y{};
    if (state->Workspace().locator_auto_scroll
        && view->presentation.locator_neighborhood_width != 0U
        && view->presentation.locator_neighborhood_height != 0U) {
        const std::int32_t left =
            view->presentation.locator_neighborhood_origin_x;
        const std::int32_t top =
            view->presentation.locator_neighborhood_origin_y;
        const std::int32_t right = left + static_cast<std::int32_t>(
            view->presentation.locator_neighborhood_width) - 1;
        const std::int32_t bottom = top + static_cast<std::int32_t>(
            view->presentation.locator_neighborhood_height) - 1;
        pan_x = document_x == left ? 32.0 : (document_x == right ? -32.0 : 0.0);
        pan_y = document_y == top ? 32.0 : (document_y == bottom ? -32.0 : 0.0);
    }
    const std::uint64_t core_view_id = view->core_view_id;
    InkpodStrokeSample sample{
        sizeof(InkpodStrokeSample),
        0U,
        static_cast<float>(document_x) + 0.5F,
        static_cast<float>(document_y) + 0.5F,
        1.0F,
        0U};
    InkpodEditorStrokeInput input{
        sizeof(InkpodEditorStrokeInput),
        INKPOD_COORDINATE_SPACE_DOCUMENT,
        INKPOD_EDITOR_TOOL_PENCIL,
        0U,
        INKPOD_STROKE_FLAG_AUTO_ERASE,
        &sample,
        1U,
        sizeof(InkpodStrokeSample)};
    (void)state->engine->Enqueue(
        target.context,
        [input, sample, core_view_id, pan_x, pan_y](InkpodCore* core) mutable {
            input.samples = &sample;
            const InkpodStatus begin =
                inkpod_core_editor_stroke_begin(core, &input);
            if (begin != INKPOD_STATUS_OK) {
                return begin;
            }
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            const InkpodStatus end = inkpod_core_stroke_end(core, &result);
            if (end != INKPOD_STATUS_OK) {
                (void)inkpod_core_stroke_cancel(core);
            } else if ((pan_x != 0.0 || pan_y != 0.0)
                && core_view_id != 0U) {
                const InkpodViewInput view_input{
                    sizeof(InkpodViewInput),
                    INKPOD_VIEW_PAN_BY,
                    0U,
                    pan_x,
                    pan_y,
                    0.0,
                    0.0};
                (void)inkpod_core_view_apply(core, core_view_id, &view_input);
            }
            return end;
        },
        true,
        true,
        false);
}

bool RefreshColorPanes(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.color_pane);
    const bool pinned = binding != nullptr
        && binding->policy == PaneTargetPolicy::PinnedDocument;
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.color_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    auto* document = target.context.document_session.has_value()
        ? state.Documents().Find(target.context.document_session.value())
        : nullptr;
    std::uint64_t notice_sequence{};
    const PaneTargetNotice notice = state.routing.pane_targets.ConsumeNotice(
        state.routing.color_pane, notice_sequence);
    if (notice != PaneTargetNotice::None
        && notice_sequence != state.Workspace().color_notice_sequence) {
        state.Workspace().color_notice_sequence = notice_sequence;
        if (state.Workspace().windows.color_pane != nullptr) {
            NotifyWinEvent(
                EVENT_SYSTEM_ALERT,
                state.Workspace().windows.color_pane,
                OBJID_CLIENT,
                CHILDID_SELF);
        }
    }
    if (target.status != PaneTargetStatus::Ok || document == nullptr
        || !target.context.generation.has_value()) {
        inkpod::windows::ui::panes::UpdateColorDockPaneTarget(
            state.Workspace().windows.color_pane,
            UiText(notice == PaneTargetNotice::PinnedDocumentClosed
                ? UiStringId::PinnedClosedFollowingNoTarget
                : UiStringId::FollowingNoTarget),
            false,
            false);
        return false;
    }
    ColorPanesController controller(*state.engine);
    const InkpodStatus status = controller.RefreshModel(
        document->id, document->generation, state.Workspace().panes);
    std::wstring target_text = UiTextWithUserText(
        pinned ? UiStringId::PinnedPrefix : UiStringId::FollowingPrefix,
        LocatorDocumentName(*document));
    if (notice == PaneTargetNotice::PinnedDocumentClosed) {
        target_text = UiTextWithUserText(
            UiStringId::PinnedClosedFollowingPrefix,
            LocatorDocumentName(*document));
    }
    inkpod::windows::ui::panes::UpdateColorDockPaneTarget(
        state.Workspace().windows.color_pane,
        std::move(target_text),
        status == INKPOD_STATUS_OK,
        pinned);
    return status == INKPOD_STATUS_OK;
}

void RefreshDockPaneViews(ApplicationHost& state) noexcept {
    inkpod::windows::ui::panes::UpdateToolOptionsPane(
        state.Workspace().windows.tool_options,
        state.Workspace().tools.active_tool,
        state.Workspace().tools.active_plane,
        state.Workspace().tools.diameter,
        state.Workspace().tools.brush);
    if (inkpod::windows::ui::panes::IsToolOptionsFlyoutVisible(
            state.Workspace().windows.tool_options_flyout)) {
        inkpod::windows::ui::panes::RefreshToolOptionsDetail(
            state.Workspace().windows.tool_options,
            state.Workspace().tools.options_flyout.command);
    }
    inkpod::windows::ui::panes::UpdateColorDockPane(
        state.Workspace().windows.color_pane,
        state.Workspace().panes.main_line_color,
        state.Workspace().tools.drawing_color,
        state.Workspace().panes.palette_colors,
        state.Workspace().panes.color_chart_colors,
        state.Workspace().panes.color_chart_names,
        state.Workspace().panes.palette_group,
        state.Workspace().panes.color_chart_page,
        state.Workspace().panes.color_chart_locked);
}

InkpodStatus ReplacePalette(
    ApplicationHost& state,
    const CommandContext& context,
    const std::vector<InkpodColorValue>& colors) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ColorPanesController controller(*state.engine);
    return controller.ReplacePalette(
        context.document_session.value(),
        context.generation.value(),
        colors);
}

void UpdateMotionState(
    AnimationUiState& animation, const InkpodMotionFrame& frame) noexcept {
    animation.motion_paused = (frame.flags & INKPOD_MOTION_FRAME_PAUSED) != 0U;
}

std::wstring Utf8UserText(const std::string& text) {
    if (text.size() > static_cast<std::size_t>(INT_MAX)) {
        return {};
    }
    const int count = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        text.data(),
        static_cast<int>(text.size()),
        nullptr,
        0);
    if (count <= 0) {
        return {};
    }
    std::wstring output(static_cast<std::size_t>(count), L'\0');
    if (MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            text.data(),
            static_cast<int>(text.size()),
            output.data(),
            count) != count) {
        return {};
    }
    return output;
}

bool ConfigureHistoryDialog(
    ApplicationHost& app, bool forward, HistoryDialogState& dialog) noexcept {
    if (app.engine == nullptr) {
        return false;
    }
    InkpodHistoryInfo info{};
    info.struct_size = sizeof(info);
    std::vector<InkpodHistoryEntryKind> kinds;
    const InkpodStatus status = app.engine->Invoke(
        [&info, &kinds](InkpodCore* core) {
            InkpodStatus inner = inkpod_core_history_info(core, &info);
            if (inner != INKPOD_STATUS_OK || info.item_count > UINT64_C(1048576)) {
                return inner == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : inner;
            }
            try {
                kinds.reserve(static_cast<std::size_t>(info.item_count));
                for (std::uint64_t index = 0; index < info.item_count; ++index) {
                    InkpodHistoryItem item{};
                    item.struct_size = sizeof(item);
                    inner = inkpod_core_history_item(core, index, &item);
                    if (inner != INKPOD_STATUS_OK
                        || item.entry_kind < INKPOD_HISTORY_ENTRY_RASTER
                        || item.entry_kind > INKPOD_HISTORY_ENTRY_DOCUMENT) {
                        return inner == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : inner;
                    }
                    kinds.push_back(item.entry_kind);
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
                label = UiText(UiStringId::Text0022);
            } else {
                std::array<wchar_t, 32U> prefix{};
                _snwprintf_s(
                    prefix.data(), prefix.size(), _TRUNCATE, L"%llu: ",
                    static_cast<unsigned long long>(cursor));
                label = prefix.data();
                const auto string_id = HistoryUiStringId(
                    kinds[static_cast<std::size_t>(cursor - 1U)]);
                if (!string_id.has_value()) {
                    return false;
                }
                label += UiText(string_id.value());
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
    ApplicationHost& app,
    InkpodHistoryInfo& info,
    std::wstring& undo_label,
    std::wstring& redo_label) noexcept {
    if (app.engine == nullptr) {
        return false;
    }
    info = {};
    info.struct_size = sizeof(info);
    InkpodHistoryEntryKind undo_kind{};
    InkpodHistoryEntryKind redo_kind{};
    const DocumentSessionId session = app.routing.targets.DocumentSession();
    const DocumentSession* document = app.Documents().Find(session);
    if (document == nullptr
        || !app.engine->GetHistoryPresentation(
            document->id,
            document->generation,
            info,
            undo_kind,
            redo_kind)) {
        return false;
    }
    try {
        const auto undo_string_id = HistoryUiStringId(undo_kind);
        const auto redo_string_id = HistoryUiStringId(redo_kind);
        if ((undo_kind != 0U && !undo_string_id.has_value())
            || (redo_kind != 0U && !redo_string_id.has_value())) {
            return false;
        }
        undo_label = undo_kind == 0U
            ? UiText(UiStringId::Text0486)
            : UiText(UiStringId::Text0487)
                + std::wstring(UiText(undo_string_id.value())) + L"\tCtrl+Z";
        redo_label = redo_kind == 0U
            ? UiText(UiStringId::Text0122)
            : UiText(UiStringId::Text0123)
                + std::wstring(UiText(redo_string_id.value())) + L"\tCtrl+Y";
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

void RefreshBatchPalette(BatchUiState& batch, HWND palette) noexcept {
    BatchController::RefreshPalette(batch, palette);
}

void UpdateBatchTarget(ApplicationHost& state) noexcept {
    const auto* binding = state.routing.pane_targets.Find(
        state.routing.batch_pane);
    state.batch.target_pinned = binding != nullptr
        && binding->policy == PaneTargetPolicy::PinnedDocument;
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.batch_pane,
        state.routing.targets.Capture(),
        state.routing.targets);
    auto* document = target.context.document_session.has_value()
        ? state.Documents().Find(target.context.document_session.value())
        : nullptr;
    std::uint64_t notice_sequence{};
    const PaneTargetNotice notice = state.routing.pane_targets.ConsumeNotice(
        state.routing.batch_pane, notice_sequence);
    if (notice != PaneTargetNotice::None
        && notice_sequence != state.Workspace().batch_notice_sequence) {
        state.Workspace().batch_notice_sequence = notice_sequence;
        if (state.Workspace().batch_palette != nullptr) {
            NotifyWinEvent(
                EVENT_SYSTEM_ALERT,
                state.Workspace().batch_palette,
                OBJID_CLIENT,
                CHILDID_SELF);
        }
    }
    state.batch.target_available = target.status == PaneTargetStatus::Ok
        && document != nullptr;
    if (!state.batch.target_available) {
        state.batch.target_text = UiText(
            notice == PaneTargetNotice::JobClosed
                ? UiStringId::JobClosedNoTarget
                : (notice == PaneTargetNotice::PinnedDocumentClosed
                        ? UiStringId::PinnedClosedFollowingNoTarget
                        : UiStringId::FollowingNoTarget));
        return;
    }
    const std::wstring name = LocatorDocumentName(*document);
    if (binding != nullptr && binding->policy == PaneTargetPolicy::Job
        && target.context.job.has_value()) {
        state.batch.target_text = std::wstring(UiText(UiStringId::JobPrefix))
            + std::to_wstring(target.context.job->Value()) + L": " + name;
    } else {
        state.batch.target_text = UiTextWithUserText(
            state.batch.target_pinned
                ? UiStringId::PinnedPrefix
                : UiStringId::FollowingPrefix,
            name);
        if (notice == PaneTargetNotice::PinnedDocumentClosed) {
            state.batch.target_text = UiTextWithUserText(
                UiStringId::PinnedClosedFollowingPrefix, name);
        }
    }
}

void RefreshBatchPaletteTimer(void* context) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr) {
        UpdateBatchTarget(*state);
        RefreshBatchPalette(
            state->batch, state->Workspace().batch_palette);
    }
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

bool FocusIsWithin(HWND owner, HWND focus) noexcept {
    return owner != nullptr && focus != nullptr
        && (owner == focus || IsChild(owner, focus) != FALSE);
}

HWND FirstVisibleDockFocusTarget(ApplicationHost& state) noexcept {
    constexpr std::array pane_types{
        inkpod::windows::ui::DockPaneType::Tool,
        inkpod::windows::ui::DockPaneType::Color,
        inkpod::windows::ui::DockPaneType::Layer};
    for (const auto pane_type : pane_types) {
        const HWND content = state.Workspace().windows.dock_host.ContentWindow(pane_type);
        if (content == nullptr || IsWindowVisible(content) == FALSE
            || IsWindowEnabled(content) == FALSE) {
            continue;
        }
        const HWND child = GetNextDlgTabItem(content, nullptr, FALSE);
        return child != nullptr ? child : content;
    }
    return nullptr;
}

bool FocusIsInDock(ApplicationHost& state, HWND focus) noexcept {
    constexpr std::array pane_types{
        inkpod::windows::ui::DockPaneType::Tool,
        inkpod::windows::ui::DockPaneType::Color,
        inkpod::windows::ui::DockPaneType::Layer};
    return std::any_of(pane_types.cbegin(), pane_types.cend(), [&](const auto pane_type) {
        return FocusIsWithin(
            state.Workspace().windows.dock_host.ContentWindow(pane_type), focus);
    });
}

bool FocusIsInEditor(ApplicationHost& state, HWND focus) noexcept {
    for (std::size_t index = 0U; index < state.Workspace().editors.GroupCount(); ++index) {
        const auto* group = state.Workspace().editors.GroupAt(index);
        if (group != nullptr
            && (FocusIsWithin(group->canvas, focus)
                || FocusIsWithin(group->document_tabs, focus))) {
            return true;
        }
    }
    return false;
}

bool CycleWorkspaceFocus(ApplicationHost& state, bool reverse) noexcept {
    enum class FocusArea : std::uint8_t { Menu, Dock, Editor, Status };
    const HWND focus = GetFocus();
    FocusArea current = FocusArea::Menu;
    if (FocusIsInDock(state, focus)) {
        current = FocusArea::Dock;
    } else if (FocusIsInEditor(state, focus)) {
        current = FocusArea::Editor;
    } else if (FocusIsWithin(state.Workspace().windows.status_bar, focus)) {
        current = FocusArea::Status;
    }
    const auto next_area = [reverse](FocusArea area) noexcept {
        const auto value = static_cast<std::uint8_t>(area);
        const auto count = static_cast<std::uint8_t>(FocusArea::Status) + 1U;
        return static_cast<FocusArea>(
            reverse ? (value + count - 1U) % count : (value + 1U) % count);
    };
    FocusArea candidate = current;
    for (std::size_t attempt = 0U; attempt < 4U; ++attempt) {
        candidate = next_area(candidate);
        HWND target{};
        switch (candidate) {
            case FocusArea::Menu:
                if (state.Workspace().windows.window != nullptr) {
                    PostMessageW(
                        state.Workspace().windows.window,
                        WM_SYSCOMMAND,
                        SC_KEYMENU,
                        0);
                    return true;
                }
                break;
            case FocusArea::Dock:
                target = FirstVisibleDockFocusTarget(state);
                break;
            case FocusArea::Editor: {
                const auto* group = state.Workspace().editors.Active();
                target = group == nullptr
                    ? nullptr
                    : (group->focus_history != nullptr
                              && IsWindow(group->focus_history) != FALSE
                          ? group->focus_history
                          : group->canvas);
                break;
            }
            case FocusArea::Status:
                target = state.Workspace().windows.status_bar;
                break;
        }
        if (target != nullptr && IsWindowVisible(target) != FALSE
            && IsWindowEnabled(target) != FALSE) {
            SetFocus(target);
            return GetFocus() == target || IsChild(target, GetFocus()) != FALSE;
        }
    }
    return false;
}

bool HandleWorkspaceNavigation(
    ApplicationHost& state,
    HWND window,
    std::uint32_t virtual_key,
    std::uint32_t modifiers) noexcept {
    const std::uint32_t navigation_modifiers = modifiers
        & (INKPOD_SHORTCUT_MODIFIER_CONTROL
            | INKPOD_SHORTCUT_MODIFIER_SHIFT
            | INKPOD_SHORTCUT_MODIFIER_ALT);
    if (virtual_key == VK_F6
        && (navigation_modifiers == 0U
            || navigation_modifiers == INKPOD_SHORTCUT_MODIFIER_SHIFT)) {
        (void)CycleWorkspaceFocus(
            state,
            navigation_modifiers == INKPOD_SHORTCUT_MODIFIER_SHIFT);
        return true;
    }
    if (virtual_key == VK_F6
        && (navigation_modifiers == INKPOD_SHORTCUT_MODIFIER_CONTROL
            || navigation_modifiers
                == (INKPOD_SHORTCUT_MODIFIER_CONTROL
                    | INKPOD_SHORTCUT_MODIFIER_SHIFT))) {
        UpdateMenuState(state);
        DispatchEnabledCommand(state, window, IDM_EDITOR_GROUP_NEXT);
        return true;
    }
    return false;
}

UINT ShortcutMenuCommand(std::uint32_t command_id) noexcept {
    return inkpod::windows::ui::ShortcutMenuCommand(command_id);
}

bool ResolveConfiguredShortcut(
    ApplicationHost& state,
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

void ShowCoreError(const ApplicationHost& state, HWND owner, const wchar_t* operation) noexcept {
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
        UiText(UiStringId::Text0015),
        operation,
        detail.c_str());
    MessageBoxW(owner, message.data(), L"inkpod", MB_OK | MB_ICONERROR);
}

void ShowEmbeddedHelpError(
    const ApplicationHost& state,
    HWND owner,
    UINT message_id) noexcept {
    if (state.lifetime.smoke_test) {
        return;
    }
    std::array<wchar_t, 256U> message{};
    if (LoadLocalizedStringW(
            state.lifetime.instance,
            message_id,
            message.data(),
            static_cast<int>(message.size()))
        <= 0) {
        return;
    }
    MessageBoxW(owner, message.data(), L"inkpod", MB_OK | MB_ICONERROR);
}

void ShowShortcutError(
    const ApplicationHost& state,
    HWND owner,
    const wchar_t* operation,
    InkpodStatus status) noexcept {
    if (state.lifetime.smoke_test) {
        return;
    }
    if (status == INKPOD_STATUS_INVALID_ARGUMENT) {
        MessageBoxW(
            owner,
            UiText(UiStringId::ShortcutPrefixConflict),
            L"inkpod",
            MB_OK | MB_ICONWARNING);
        return;
    }
    if (status == INKPOD_STATUS_IO_ERROR) {
        MessageBoxW(
            owner,
            UiText(UiStringId::Text0543),
            L"inkpod",
            MB_OK | MB_ICONWARNING);
        return;
    }
    ShowCoreError(state, owner, operation);
}

void UpdateFloatingPreview(ApplicationHost& state) noexcept {
    if (state.Workspace().windows.canvas == nullptr) {
        return;
    }
    inkpod::renderer::CanvasFloatingPreview preview{};
    preview.struct_size = sizeof(preview);
    preview.active = state.Workspace().tools.floating_active ? 1U : 0U;
    preview.bounds = state.Workspace().tools.floating_bounds;
    preview.transform = state.Workspace().tools.floating_transform;
    inkpod::renderer::SetCanvasFloatingPreview(
        state.Workspace().windows.canvas, preview);
}

InkpodStatus ActivateFloatingPastePreview(
    ApplicationHost& state, const InkpodClipboard* clipboard) noexcept {
    InkpodClipboardRasterBuffer view{};
    view.struct_size = sizeof(view);
    if (inkpod_clipboard_render_rgba8(clipboard, &view) != INKPOD_STATUS_OK
        || view.width > static_cast<std::uint32_t>(INT_MAX)
        || view.height > static_cast<std::uint32_t>(INT_MAX)) {
        FloatingPasteController controller(*state.engine);
        controller.Finish(false);
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.Workspace().tools.floating_active = true;
    state.Workspace().tools.floating_bounds = {
        view.origin_x,
        view.origin_y,
        static_cast<std::int32_t>(view.width),
        static_cast<std::int32_t>(view.height)};
    state.Workspace().tools.floating_transform = InkpodFloatingTransform{
        sizeof(InkpodFloatingTransform),
        INKPOD_TRANSFORM_ANCHOR_CENTER,
        static_cast<double>(view.origin_x) + static_cast<double>(view.width) / 2.0,
        static_cast<double>(view.origin_y) + static_cast<double>(view.height) / 2.0,
        1.0,
        1.0,
        0.0};
    const InkpodStatus tool_status =
        SetEditorActiveTool(state, kInteractionFloatingTransform);
    if (tool_status != INKPOD_STATUS_OK) {
        return tool_status;
    }
    UpdateFloatingPreview(state);
    return INKPOD_STATUS_OK;
}

InkpodStatus BeginFloatingPaste(ApplicationHost& state, std::uint32_t mode) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    bool imported_standard{};
    if (state.clipboard == nullptr) {
        if (!ImportStandardClipboard(state.Workspace().windows.window, state.clipboard)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        imported_standard = true;
    }
    const InkpodClipboard* clipboard = state.clipboard;
    FloatingPasteController controller(*state.engine);
    InkpodStatus status = controller.Begin(clipboard, mode);
    if (status != INKPOD_STATUS_OK && imported_standard
        && mode == INKPOD_PASTE_COMPATIBLE) {
        status = controller.Begin(clipboard, INKPOD_PASTE_ACTIVE_CONVERTED);
    }
    if (status != INKPOD_STATUS_OK) {
        if (state.lifetime.smoke_test) {
            const std::wstring detail = state.engine->LastError();
            std::fwprintf(
                stderr,
                L"inkpod floating paste begin failed: %u (%ls)\n",
                status,
                detail.c_str());
        }
        return status;
    }
    status = ActivateFloatingPastePreview(state, clipboard);
    if (status != INKPOD_STATUS_OK && state.lifetime.smoke_test) {
        std::fprintf(stderr, "inkpod floating paste preview failed: %u\n", status);
    }
    return status;
}

InkpodStatus BeginFloatingPasteToNewPlane(
    ApplicationHost& state, InkpodTreeEdit edit, const std::string& name) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.clipboard == nullptr
        && !ImportStandardClipboard(state.Workspace().windows.window, state.clipboard)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodClipboard* clipboard = state.clipboard;
    const InkpodStatus status = state.engine->Invoke(
        [clipboard, edit, name](InkpodCore* core) mutable {
            edit.name_utf8 = name.empty()
                ? nullptr
                : reinterpret_cast<const std::uint8_t*>(name.data());
            edit.name_bytes = name.size();
            return inkpod_core_paste_begin_new_plane(core, clipboard, &edit);
        },
        false,
        false);
    return status == INKPOD_STATUS_OK
        ? ActivateFloatingPastePreview(state, clipboard)
        : status;
}

InkpodStatus SetFloatingTransform(
    ApplicationHost& state, const InkpodFloatingTransform& transform) noexcept {
    if (!state.Workspace().tools.floating_active || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    FloatingPasteController controller(*state.engine);
    const InkpodStatus status = controller.Transform(transform);
    if (status == INKPOD_STATUS_OK) {
        state.Workspace().tools.floating_transform = transform;
        UpdateFloatingPreview(state);
    }
    return status;
}

InkpodStatus ShowFloatingTransformDialog(ApplicationHost& state) noexcept {
    if (!state.Workspace().tools.floating_active) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState geometry{};
    geometry.title = UiText(UiStringId::Text0309);
    geometry.labels = {UiText(UiStringId::Text0589), UiText(UiStringId::Text0590), UiText(UiStringId::Text0645), UiText(UiStringId::Text1041)};
    geometry.values = {
        static_cast<std::int32_t>(std::lround(state.Workspace().tools.floating_transform.target_x)),
        static_cast<std::int32_t>(std::lround(state.Workspace().tools.floating_transform.target_y)),
        static_cast<std::int32_t>(std::lround(state.Workspace().tools.floating_transform.scale_x * 100.0)),
        static_cast<std::int32_t>(std::lround(state.Workspace().tools.floating_transform.scale_y * 100.0))};
    geometry.value_count = 4U;
    if (state.lifetime.smoke_test) {
        geometry.values = {2, 1, 125, 75};
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, geometry) != IDOK
        || geometry.values[2] <= 0 || geometry.values[3] <= 0) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState rotation{};
    rotation.title = UiText(UiStringId::Text0578);
    rotation.labels = {UiText(UiStringId::Text0907), UiText(UiStringId::Text0861), UiText(UiStringId::Text0451), nullptr};
    rotation.values = {
        static_cast<std::int32_t>(std::lround(state.Workspace().tools.floating_transform.rotation_degrees)),
        0,
        static_cast<std::int32_t>(state.Workspace().tools.floating_transform.anchor),
        0};
    rotation.value_count = 3U;
    if (state.lifetime.smoke_test) {
        rotation.values[0] = 15;
        rotation.values[2] = INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT;
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, rotation) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    if (rotation.values[2] < static_cast<std::int32_t>(INKPOD_TRANSFORM_ANCHOR_TOP_LEFT)
        || rotation.values[2] > static_cast<std::int32_t>(INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (rotation.values[1] != 0) {
        geometry.values[3] = geometry.values[2];
    }
    const InkpodFloatingTransform transform{
        sizeof(InkpodFloatingTransform),
        static_cast<std::uint32_t>(rotation.values[2]),
        static_cast<double>(geometry.values[0]),
        static_cast<double>(geometry.values[1]),
        static_cast<double>(geometry.values[2]) / 100.0,
        static_cast<double>(geometry.values[3]) / 100.0,
        static_cast<double>(rotation.values[0])};
    return SetFloatingTransform(state, transform);
}

InkpodStatus UpdateFloatingHandleDrag(
    ApplicationHost& state,
    const InkpodStrokeSample& start,
    const InkpodStrokeSample& current,
    bool begin) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds canvas{};
    if (!state.Workspace().tools.floating_active || !QueryDocument(state, info)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, canvas)
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
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return std::pair{x, y};
    };
    const auto to_device = [&](double x, double y) {
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return std::pair{canvas.left + x * zoom, canvas.top + y * zoom};
    };
    const InkpodFloatingTransform& base_transform = begin
        ? state.Workspace().tools.floating_transform
        : state.Workspace().tools.floating_drag_start;
    const double pivot_x = base_transform.target_x;
    const double pivot_y = base_transform.target_y;
    const double left = static_cast<double>(state.Workspace().tools.floating_bounds.x);
    const double top = static_cast<double>(state.Workspace().tools.floating_bounds.y);
    const double right = left + static_cast<double>(state.Workspace().tools.floating_bounds.width);
    const double bottom = top + static_cast<double>(state.Workspace().tools.floating_bounds.height);
    const auto source_anchor = [&]() {
        const double anchor_x = base_transform.anchor == INKPOD_TRANSFORM_ANCHOR_TOP_RIGHT
                || base_transform.anchor == INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT
            ? right
            : base_transform.anchor == INKPOD_TRANSFORM_ANCHOR_CENTER ? (left + right) / 2.0 : left;
        const double anchor_y = base_transform.anchor == INKPOD_TRANSFORM_ANCHOR_BOTTOM_LEFT
                || base_transform.anchor == INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT
            ? bottom
            : base_transform.anchor == INKPOD_TRANSFORM_ANCHOR_CENTER ? (top + bottom) / 2.0 : top;
        return std::pair{anchor_x, anchor_y};
    }();
    const double radians = base_transform.rotation_degrees
        * 3.14159265358979323846 / 180.0;
    const double sine = std::sin(radians);
    const double cosine = std::cos(radians);
    const auto transform_point = [&](double x, double y) {
        const double local_x = (x - source_anchor.first) * base_transform.scale_x;
        const double local_y = (y - source_anchor.second) * base_transform.scale_y;
        return std::pair{
            pivot_x + local_x * cosine - local_y * sine,
            pivot_y + local_x * sine + local_y * cosine};
    };
    if (begin) {
        state.Workspace().tools.floating_drag_start = state.Workspace().tools.floating_transform;
        state.Workspace().tools.floating_drag_mode = 1U;
        const std::array<std::pair<double, double>, 4U> source_corners{
            std::pair{left, top}, std::pair{right, top},
            std::pair{right, bottom}, std::pair{left, bottom}};
        for (const auto& point : source_corners) {
            const auto transformed = transform_point(point.first, point.second);
            const auto device = to_device(transformed.first, transformed.second);
            if (std::hypot(
                    device.first - static_cast<double>(start.x),
                    device.second - static_cast<double>(start.y)) <= 14.0) {
                state.Workspace().tools.floating_drag_mode = 2U;
                break;
            }
        }
        const auto transformed_top_left = transform_point(left, top);
        const auto transformed_top_right = transform_point(right, top);
        const auto rotation_handle = to_device(
            (transformed_top_left.first + transformed_top_right.first) / 2.0
                + sine * 20.0 / zoom,
            (transformed_top_left.second + transformed_top_right.second) / 2.0
                - cosine * 20.0 / zoom);
        if (std::hypot(
                rotation_handle.first - static_cast<double>(start.x),
                rotation_handle.second - static_cast<double>(start.y)) <= 16.0) {
            state.Workspace().tools.floating_drag_mode = 3U;
        }
        return INKPOD_STATUS_OK;
    }
    const auto start_document = to_document(start);
    const auto current_document = to_document(current);
    InkpodFloatingTransform transform = state.Workspace().tools.floating_drag_start;
    if (state.Workspace().tools.floating_drag_mode == 2U) {
        const double start_relative_x = start_document.first - pivot_x;
        const double start_relative_y = start_document.second - pivot_y;
        const double current_relative_x = current_document.first - pivot_x;
        const double current_relative_y = current_document.second - pivot_y;
        const double start_dx = start_relative_x * cosine + start_relative_y * sine;
        const double start_dy = -start_relative_x * sine + start_relative_y * cosine;
        const double current_dx = current_relative_x * cosine + current_relative_y * sine;
        const double current_dy = -current_relative_x * sine + current_relative_y * cosine;
        if (std::abs(start_dx) > 0.01) {
            transform.scale_x *= std::max(0.01, std::abs(current_dx / start_dx));
        }
        if (std::abs(start_dy) > 0.01) {
            transform.scale_y *= std::max(0.01, std::abs(current_dy / start_dy));
        }
    } else if (state.Workspace().tools.floating_drag_mode == 3U) {
        const double start_angle = std::atan2(
            start_document.second - pivot_y, start_document.first - pivot_x);
        const double current_angle = std::atan2(
            current_document.second - pivot_y, current_document.first - pivot_x);
        transform.rotation_degrees +=
            (current_angle - start_angle) * 180.0 / 3.14159265358979323846;
    } else {
        transform.target_x += current_document.first - start_document.first;
        transform.target_y += current_document.second - start_document.second;
    }
    return SetFloatingTransform(state, transform);
}

bool QueryVanishingPoints(
    ApplicationHost& state,
    std::vector<InkpodVanishingPointInfo>& points) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    try {
        points.assign(64U, InkpodVanishingPointInfo{});
    } catch (const std::bad_alloc&) {
        return false;
    }
    std::uint64_t count{};
    const InkpodStatus status = state.engine->Invoke(
        [&points, &count](InkpodCore* core) {
            return inkpod_core_vanishing_points_copy(
                core,
                points.data(),
                static_cast<std::uint64_t>(points.size()),
                sizeof(InkpodVanishingPointInfo),
                &count);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK || count > points.size()) {
        points.clear();
        return false;
    }
    points.resize(static_cast<std::size_t>(count));
    return true;
}

InkpodVanishingPointInput VanishingPointInputFromInfo(
    const InkpodVanishingPointInfo& point) noexcept {
    return InkpodVanishingPointInput{
        sizeof(InkpodVanishingPointInput),
        point.visible,
        0U,
        point.layer_id,
        point.x_milli,
        point.y_milli,
        point.interval_milli_degrees,
        point.angle_milli_degrees,
        point.opacity_milli,
        0U,
        point.color};
}

InkpodStatus HandleVanishingPointCanvasEvent(
    ApplicationHost& state,
    const inkpod::renderer::CanvasStrokeEvent& event) noexcept {
    auto& tools = state.Workspace().tools;
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (event.kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
        tools.vanishing_point_gesture_samples.clear();
        tools.vanishing_point_drag_id = 0U;
        if (!tools.vanishing_point_preview_active) {
            return INKPOD_STATUS_OK;
        }
        tools.vanishing_point_preview_active = false;
        return state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_vanishing_point_preview_cancel(core);
            },
            true,
            false);
    }
    if (event.sample_count == 0U || event.samples == nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodDocumentInfo document{};
    inkpod::renderer::CanvasDocumentBounds canvas{};
    std::vector<InkpodVanishingPointInfo> points;
    if (!QueryDocument(state, document)
        || !QueryVanishingPoints(state, points)
        || !inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, canvas)
        || document.width == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (canvas.right - canvas.left)
        / static_cast<double>(document.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto to_document = [&](const InkpodStrokeSample& sample) {
        double x = (static_cast<double>(sample.x) - canvas.left) / zoom;
        double y = (static_cast<double>(sample.y) - canvas.top) / zoom;
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(document.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(document.height) - y;
        }
        return std::pair{x, y};
    };
    const auto to_device = [&](double x, double y) {
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(document.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(document.height) - y;
        }
        return std::pair{canvas.left + x * zoom, canvas.top + y * zoom};
    };
    const InkpodStrokeSample current =
        event.samples[static_cast<std::size_t>(event.sample_count - 1U)];
    if (event.kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
        tools.vanishing_point_preview_active = false;
        tools.vanishing_point_drag_id = 0U;
        tools.vanishing_point_gesture_samples.clear();
        const InkpodVanishingPointInfo* selected{};
        double best = 14.0;
        for (const auto& point : points) {
            const auto device = to_device(
                static_cast<double>(point.x_milli) / 1000.0,
                static_cast<double>(point.y_milli) / 1000.0);
            const double distance = std::hypot(
                device.first - static_cast<double>(current.x),
                device.second - static_cast<double>(current.y));
            if (distance <= best) {
                best = distance;
                selected = &point;
            }
        }
        if (selected == nullptr) {
            return INKPOD_STATUS_OK;
        }
        try {
            tools.vanishing_point_gesture_samples.push_back(current);
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        tools.vanishing_point_drag_id = selected->point_id;
        tools.vanishing_point_drag_value = VanishingPointInputFromInfo(*selected);
        const InkpodStatus status = state.engine->Invoke(
            [base_revision = document.document_revision,
             point_id = selected->point_id,
             input = tools.vanishing_point_drag_value](InkpodCore* core) {
                return inkpod_core_vanishing_point_preview_begin(
                    core,
                    base_revision,
                    INKPOD_VANISHING_POINT_EDIT_UPDATE,
                    point_id,
                    &input);
            },
            true,
            false);
        tools.vanishing_point_preview_active = status == INKPOD_STATUS_OK;
        return status;
    }
    if (!tools.vanishing_point_preview_active
        || tools.vanishing_point_gesture_samples.empty()) {
        return INKPOD_STATUS_OK;
    }
    const auto start = to_document(tools.vanishing_point_gesture_samples.front());
    const auto position = to_document(current);
    InkpodVanishingPointInput value = tools.vanishing_point_drag_value;
    value.x_milli += static_cast<std::int64_t>(
        std::nearbyint((position.first - start.first) * 1000.0));
    value.y_milli += static_cast<std::int64_t>(
        std::nearbyint((position.second - start.second) * 1000.0));
    InkpodStatus status = state.engine->Invoke(
        [value](InkpodCore* core) {
            return inkpod_core_vanishing_point_preview_update(core, &value);
        },
        true,
        false);
    if (status != INKPOD_STATUS_OK) {
        tools.vanishing_point_preview_active = false;
        tools.vanishing_point_gesture_samples.clear();
        (void)state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_vanishing_point_preview_cancel(core);
            },
            true,
            false);
        return status;
    }
    if (event.kind == inkpod::renderer::CanvasStrokeEventKind::End) {
        status = state.engine->Invoke(
            [](InkpodCore* core) {
                std::uint64_t revision{};
                std::uint64_t point_id{};
                return inkpod_core_vanishing_point_preview_apply(
                    core, &revision, &point_id);
            },
            true,
            true);
        tools.vanishing_point_preview_active = false;
        tools.vanishing_point_gesture_samples.clear();
    }
    return status;
}

InkpodStatus HandleShootingFrameCanvasEvent(
    ApplicationHost& state,
    const inkpod::renderer::CanvasStrokeEvent& event) noexcept {
    auto& tools = state.Workspace().tools;
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (event.kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
        tools.shooting_frame_gesture_samples.clear();
        tools.shooting_frame_drag_handle = 0U;
        if (!tools.shooting_frame_preview_active) {
            return INKPOD_STATUS_OK;
        }
        tools.shooting_frame_preview_active = false;
        return state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_shooting_frame_preview_cancel(core);
            },
            true,
            false);
    }
    if (event.sample_count == 0U || event.samples == nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodDocumentInfo document{};
    InkpodShootingFrameInfo frame{};
    inkpod::renderer::CanvasDocumentBounds canvas{};
    bool present{};
    if (!QueryDocument(state, document)
        || !QueryShootingFrame(state, present, frame)
        || !present
        || !inkpod::renderer::GetCanvasDocumentBounds(
            state.Workspace().windows.canvas, canvas)
        || document.width == 0U || document.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (canvas.right - canvas.left)
        / static_cast<double>(document.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto to_document = [&](const InkpodStrokeSample& sample) {
        double x = (static_cast<double>(sample.x) - canvas.left) / zoom;
        double y = (static_cast<double>(sample.y) - canvas.top) / zoom;
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(document.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(document.height) - y;
        }
        return std::pair{x, y};
    };
    const auto to_device = [&](double x, double y) {
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(document.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(document.height) - y;
        }
        return std::pair{canvas.left + x * zoom, canvas.top + y * zoom};
    };
    const InkpodStrokeSample current =
        event.samples[static_cast<std::size_t>(event.sample_count - 1U)];
    if (event.kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
        tools.shooting_frame_gesture_samples.clear();
        tools.shooting_frame_drag_handle = 0U;
        tools.shooting_frame_preview_active = false;
        try {
            tools.shooting_frame_gesture_samples.push_back(current);
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        tools.shooting_frame_drag_value = ShootingFrameInputFromInfo(frame);
        const auto center = to_device(
            tools.shooting_frame_drag_value.center_x,
            tools.shooting_frame_drag_value.center_y);
        if (std::hypot(
                center.first - static_cast<double>(current.x),
                center.second - static_cast<double>(current.y)) <= 14.0) {
            tools.shooting_frame_drag_handle = 1U;
        }
        std::array<std::pair<double, double>, 4U> corners{};
        for (std::size_t index = 0U; index < corners.size(); ++index) {
            corners[index] = to_device(
                static_cast<double>(frame.corners[index].x_milli) / 1000.0,
                static_cast<double>(frame.corners[index].y_milli) / 1000.0);
            if (std::hypot(
                    corners[index].first - static_cast<double>(current.x),
                    corners[index].second - static_cast<double>(current.y)) <= 14.0) {
                tools.shooting_frame_drag_handle = 2U;
            }
        }
        const double edge_x = corners[1].first - corners[0].first;
        const double edge_y = corners[1].second - corners[0].second;
        const double edge_length = std::hypot(edge_x, edge_y);
        if (edge_length > 0.0) {
            const std::pair rotation_handle{
                (corners[0].first + corners[1].first) / 2.0
                    + edge_y / edge_length * 24.0,
                (corners[0].second + corners[1].second) / 2.0
                    - edge_x / edge_length * 24.0};
            if (std::hypot(
                    rotation_handle.first - static_cast<double>(current.x),
                    rotation_handle.second - static_cast<double>(current.y)) <= 16.0) {
                tools.shooting_frame_drag_handle = 3U;
            }
        }
        if (tools.shooting_frame_drag_handle == 0U) {
            tools.shooting_frame_gesture_samples.clear();
            return INKPOD_STATUS_OK;
        }
        const InkpodStatus status = state.engine->Invoke(
            [base_revision = document.document_revision,
             frame_id = frame.frame_id,
             input = tools.shooting_frame_drag_value](InkpodCore* core) {
                return inkpod_core_shooting_frame_preview_begin(
                    core,
                    base_revision,
                    INKPOD_SHOOTING_FRAME_EDIT_UPDATE,
                    frame_id,
                    &input);
            },
            true,
            false);
        tools.shooting_frame_preview_active = status == INKPOD_STATUS_OK;
        return status;
    }
    if (!tools.shooting_frame_preview_active
        || tools.shooting_frame_gesture_samples.empty()) {
        return INKPOD_STATUS_OK;
    }
    const auto start_document = to_document(
        tools.shooting_frame_gesture_samples.front());
    const auto current_document = to_document(current);
    InkpodShootingFrameInput value = tools.shooting_frame_drag_value;
    if (tools.shooting_frame_drag_handle == 1U) {
        value.center_x += current_document.first - start_document.first;
        value.center_y += current_document.second - start_document.second;
    } else if (tools.shooting_frame_drag_handle == 2U) {
        const double radians = value.rotation_degrees
            * 3.14159265358979323846 / 180.0;
        const double sine = std::sin(radians);
        const double cosine = std::cos(radians);
        const double dx = current_document.first - value.center_x;
        const double dy = current_document.second - value.center_y;
        value.width = std::max(0.001, std::abs(dx * cosine + dy * sine) * 2.0);
        value.height = std::max(0.001, std::abs(-dx * sine + dy * cosine) * 2.0);
    } else if (tools.shooting_frame_drag_handle == 3U) {
        const double start_angle = std::atan2(
            start_document.second - value.center_y,
            start_document.first - value.center_x);
        const double current_angle = std::atan2(
            current_document.second - value.center_y,
            current_document.first - value.center_x);
        value.rotation_degrees += (current_angle - start_angle)
            * 180.0 / 3.14159265358979323846;
    }
    InkpodStatus status = state.engine->Invoke(
        [value](InkpodCore* core) {
            return inkpod_core_shooting_frame_preview_update(core, &value);
        },
        true,
        false);
    if (status != INKPOD_STATUS_OK) {
        tools.shooting_frame_preview_active = false;
        tools.shooting_frame_drag_handle = 0U;
        tools.shooting_frame_gesture_samples.clear();
        (void)state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_shooting_frame_preview_cancel(core);
            },
            true,
            false);
        return status;
    }
    if (event.kind == inkpod::renderer::CanvasStrokeEventKind::End) {
        status = state.engine->Invoke(
            [](InkpodCore* core) {
                std::uint64_t revision{};
                std::uint64_t frame_id{};
                return inkpod_core_shooting_frame_preview_apply(
                    core, &revision, &frame_id);
            },
            true,
            true);
        tools.shooting_frame_preview_active = false;
        tools.shooting_frame_drag_handle = 0U;
        tools.shooting_frame_gesture_samples.clear();
    }
    return status;
}

InkpodStatus EndFloatingPaste(ApplicationHost& state, bool commit) noexcept {
    if (!state.Workspace().tools.floating_active || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    FloatingPasteController controller(*state.engine);
    const InkpodStatus status = controller.Finish(commit);
    if (status == INKPOD_STATUS_OK || !commit) {
        state.Workspace().tools.floating_active = false;
        state.Workspace().tools.floating_bounds = {};
        state.Workspace().tools.floating_gesture_samples.clear();
        UpdateFloatingPreview(state);
        const InkpodStatus tool_status =
            SetEditorActiveTool(state, INKPOD_TOOL_PENCIL);
        if (tool_status != INKPOD_STATUS_OK) {
            return tool_status;
        }
        if (commit) {
            if (!state.RefreshEditorPresentation(
                    state.Document().id, state.Document().generation)) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            RefreshTreePane(state);
        }
    }
    return status;
}

InkpodStatus ResizeDocumentFromDialog(
    ApplicationHost& state, const wchar_t* title, bool resolution_mode) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState dimensions{};
    dimensions.title = title;
    dimensions.labels = {
        UiText(UiStringId::Text0646),
        UiText(UiStringId::Text1042),
        UiText(UiStringId::XDpiThousandthLabel),
        UiText(UiStringId::YDpiThousandthLabel)};
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
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, dimensions) != IDOK
        || dimensions.values[0] <= 0 || dimensions.values[1] <= 0
        || dimensions.values[2] <= 0 || dimensions.values[3] <= 0) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState placement{};
    placement.title = UiText(UiStringId::Text1006);
    placement.labels = {UiText(UiStringId::Text0593), UiText(UiStringId::Text0509), UiText(UiStringId::Text0089), nullptr};
    placement.values = {
        INKPOD_RESIZE_ANCHOR_CENTER,
        resolution_mode ? 0 : 0,
        1,
        0};
    placement.value_count = 3U;
    if (ShowViewOptions(
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, placement) != IDOK
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

InkpodStatus FitPaperToCaptureFrame(ApplicationHost& state) noexcept {
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

bool QueryDocument(ApplicationHost& state, InkpodDocumentInfo& info) noexcept {
    info = EmptyDocumentInfo();
    const DocumentSessionId session = state.routing.targets.DocumentSession();
    DocumentSession* document = state.Documents().Find(session);
    if (state.engine == nullptr || document == nullptr
        || !state.engine->GetDocumentInfo(
            document->id, document->generation, info)) {
        return false;
    }
    InkpodEditorStateInfo cached_editor{};
    cached_editor.struct_size = sizeof(cached_editor);
    const auto& binding = state.Workspace().tools.editor;
    if (state.engine->GetEditorState(
            document->id, document->generation, cached_editor)
        && (!document->has_editor_presentation
            || document->editor_presentation.editor_revision
                != cached_editor.editor_revision
            || !binding.valid || binding.session != document->id
            || binding.generation != document->generation
            || binding.editor_revision != cached_editor.editor_revision)) {
        return state.RefreshEditorPresentation(
            document->id, document->generation);
    }
    return true;
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

bool InitializeEditorUpdate(
    ApplicationHost& state,
    InkpodEditorUpdateKind kind,
    InkpodEditorStateUpdate& update) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    const DocumentSessionId session = state.Document().id;
    const Generation generation = state.Document().generation;
    InkpodEditorStateInfo cached{};
    cached.struct_size = sizeof(cached);
    if (!state.engine->GetEditorState(session, generation, cached)) {
        return false;
    }
    const auto presentation_is_current = [&]() noexcept {
        const auto& binding = state.Workspace().tools.editor;
        return binding.valid && binding.session == session
            && binding.generation == generation
            && binding.editor_revision == cached.editor_revision;
    };
    if (!presentation_is_current()) {
        if (!state.RefreshEditorPresentation(session, generation)
            || !state.engine->GetEditorState(session, generation, cached)
            || !presentation_is_current()) {
            return false;
        }
    }
    const auto& binding = state.Workspace().tools.editor;
    update = {};
    update.struct_size = sizeof(update);
    update.kind = kind;
    update.expected_editor_revision = binding.editor_revision;
    update.color.struct_size = sizeof(update.color);
    update.fill.struct_size = sizeof(update.fill);
    update.selection.struct_size = sizeof(update.selection);
    return true;
}

const InkpodEditorStateInfo* PresentedEditorState(
    const ApplicationHost& state) noexcept {
    const DocumentSession& document = state.Document();
    const auto& binding = state.Workspace().tools.editor;
    return document.has_editor_presentation && binding.valid
            && binding.session == document.id
            && binding.generation == document.generation
            && binding.editor_revision
                == document.editor_presentation.editor_revision
        ? &document.editor_presentation
        : nullptr;
}

bool BeginEditorProcedureCapture(ApplicationHost& state) noexcept {
    const DocumentSession& document = state.Document();
    const InkpodEditorStateInfo* editor = PresentedEditorState(state);
    if (editor == nullptr) {
        state.Workspace().tools.procedure.valid = false;
        return false;
    }
    state.Workspace().tools.procedure.session = document.id;
    state.Workspace().tools.procedure.generation = document.generation;
    state.Workspace().tools.procedure.core_view_id = state.ActiveView().core_view_id;
    state.Workspace().tools.procedure.state = *editor;
    state.Workspace().tools.procedure.valid = true;
    return true;
}

const InkpodEditorStateInfo* CapturedEditorState(
    const ApplicationHost& state) noexcept {
    const auto& capture = state.Workspace().tools.procedure;
    const DocumentSession& document = state.Document();
    return capture.valid && capture.session == document.id
            && capture.generation == document.generation
        ? &capture.state
        : nullptr;
}

void ClearEditorProcedureCapture(ApplicationHost& state) noexcept {
    state.Workspace().tools.procedure.valid = false;
}

std::int64_t FloatToQ16(float value) noexcept {
    return static_cast<std::int64_t>(
        std::llround(static_cast<double>(value) * 65536.0));
}

void CancelCoreRasterGeometryPreview(ApplicationHost& state) noexcept {
    auto& tools = state.Workspace().tools;
    if (tools.geometry_preview_active && state.engine != nullptr) {
        (void)state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_geometry_preview_cancel(core);
            },
            true,
            false);
    }
    CancelRasterGeometryPreview(tools, state.Workspace().windows.canvas);
}

InkpodStatus SetEditorActiveTool(
    ApplicationHost& state, std::uint32_t tool) noexcept {
    InkpodEditorStateUpdate update{};
    if (!InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_ACTIVE_TOOL, update)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint32_t previous = state.Workspace().tools.active_tool;
    if (previous != tool && IsGeometryCanvasTool(previous)) {
        CancelCoreRasterGeometryPreview(state);
    }
    if (previous != tool && previous == kInteractionSelection) {
        CancelSelectionGeometryPreview(
            state.Workspace().tools, state.Workspace().windows.canvas);
    }
    if (previous != tool && previous == kInteractionColorReplace) {
        CancelColorReplaceGeometryPreview(
            state.Workspace().tools, state.Workspace().windows.canvas);
    }
    if (previous != tool && previous == kInteractionFill) {
        CancelFillGeometryPreview(
            state.Workspace().tools, state.Workspace().windows.canvas);
    }
    if (previous != tool && previous == kInteractionShootingFrame) {
        auto& tools = state.Workspace().tools;
        if (tools.shooting_frame_preview_active && state.engine != nullptr) {
            (void)state.engine->Invoke(
                [](InkpodCore* core) {
                    return inkpod_core_shooting_frame_preview_cancel(core);
                },
                true,
                false);
        }
        tools.shooting_frame_preview_active = false;
        tools.shooting_frame_drag_handle = 0U;
        tools.shooting_frame_gesture_samples.clear();
    }
    if (previous != tool && previous == kInteractionVanishingPoint) {
        auto& tools = state.Workspace().tools;
        if (tools.vanishing_point_preview_active && state.engine != nullptr) {
            (void)state.engine->Invoke(
                [](InkpodCore* core) {
                    return inkpod_core_vanishing_point_preview_cancel(core);
                },
                true,
                false);
        }
        tools.vanishing_point_preview_active = false;
        tools.vanishing_point_drag_id = 0U;
        tools.vanishing_point_gesture_samples.clear();
    }
    update.tool = tool;
    return state.UpdateEditorState(update);
}

InkpodStatus SetEditorDiameter(
    ApplicationHost& state, float diameter) noexcept {
    InkpodEditorStateUpdate update{};
    if (!std::isfinite(diameter)
        || !InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_TOOL_DIAMETER, update)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    update.tool = state.Workspace().tools.active_tool;
    update.diameter_q16 = FloatToQ16(diameter);
    return state.UpdateEditorState(update);
}

InkpodStatus SetEditorBrushOptions(
    ApplicationHost& state,
    const InkpodEditorBrushOptions& options) noexcept {
    InkpodEditorStateUpdate update{};
    if (!InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_BRUSH_OPTIONS, update)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    update.brush = options;
    update.brush.struct_size = sizeof(update.brush);
    return state.UpdateEditorState(update);
}

InkpodStatus SetEditorFillOptions(
    ApplicationHost& state,
    const inkpod::windows::ui::FillToolOptions& options) noexcept {
    InkpodEditorStateUpdate update{};
    if (options.inclusion_colors.size() > INKPOD_EDITOR_MAX_INCLUSION_COLORS
        || !InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_FILL_OPTIONS, update)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    update.fill.operation = options.operation;
    update.fill.tolerance = options.tolerance;
    update.fill.gap_close = options.gap_close;
    update.fill.inclusion_mode = options.inclusion_mode;
    update.fill.extension_distance = options.extension_distance;
    update.fill.flags = (options.overflow_abort
            ? INKPOD_EDITOR_FILL_OVERFLOW_ABORT
            : 0U)
        | (options.detached_regions ? INKPOD_EDITOR_FILL_DETACHED_REGIONS : 0U)
        | (options.transparent_only ? INKPOD_EDITOR_FILL_TRANSPARENT_ONLY : 0U)
        | (options.use_document_selection
                ? INKPOD_EDITOR_FILL_DOCUMENT_SELECTION
                : 0U)
        | (options.light_table_boundary
                ? INKPOD_EDITOR_FILL_LIGHT_TABLE_BOUNDARY
                : 0U)
        | (options.light_table_color ? INKPOD_EDITOR_FILL_LIGHT_TABLE_COLOR : 0U);
    update.fill.inclusion_color_count =
        static_cast<std::uint32_t>(options.inclusion_colors.size());
    for (std::size_t index = 0U; index < options.inclusion_colors.size(); ++index) {
        update.fill.inclusion_colors[index] = options.inclusion_colors[index];
        update.fill.inclusion_colors[index].struct_size = sizeof(InkpodColorValue);
    }
    return state.UpdateEditorState(update);
}

InkpodStatus SetEditorSelectionOptions(ApplicationHost& state) noexcept {
    InkpodEditorStateUpdate update{};
    if (!InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_SELECTION_OPTIONS, update)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto& tools = state.Workspace().tools;
    update.selection.shape = tools.selection_shape;
    update.selection.operation = tools.selection_operation;
    update.selection.tolerance = tools.selection_tolerance;
    update.selection.gap_close = tools.selection_gap_close;
    update.selection.diameter_q16 = FloatToQ16(tools.selection_diameter);
    update.selection.interpretation = tools.selection_interpretation;
    update.selection.aspect_ratio_q16 = tools.selection_aspect_ratio_q16;
    update.selection.construction_flags = tools.selection_construction_flags;
    update.selection.rotation_turns = tools.selection_rotation_turns;
    update.selection.trace_shape = tools.selection_trace_shape;
    return state.UpdateEditorState(update);
}

InkpodStatus SetEditorActiveTarget(
    ApplicationHost& state,
    std::uint64_t layer_id,
    std::uint64_t plane_id) noexcept {
    InkpodEditorStateUpdate update{};
    if (!InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_ACTIVE_TARGET, update)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    update.active_layer_id = layer_id;
    update.active_plane_id = plane_id;
    return state.UpdateEditorState(update);
}

InkpodStatus SetEditorPaletteCursor(
    ApplicationHost& state,
    std::uint32_t group,
    std::uint32_t index,
    bool present) noexcept {
    InkpodEditorStateUpdate update{};
    if (!InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_PALETTE_CURSOR, update)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    update.flags = present ? INKPOD_EDITOR_UPDATE_PALETTE_CURSOR_PRESENT : 0U;
    update.palette_group = group;
    update.palette_index = index;
    return state.UpdateEditorState(update);
}

void SetDrawingColor(ApplicationHost& state, InkpodColorValue color) noexcept {
    InkpodEditorStateUpdate update{};
    if (!InitializeEditorUpdate(
            state, INKPOD_EDITOR_UPDATE_TOOL_COLOR, update)) {
        return;
    }
    update.tool = state.Workspace().tools.last_color_consuming_tool != 0U
        ? state.Workspace().tools.last_color_consuming_tool
        : state.Workspace().tools.active_tool;
    update.color = color;
    update.color.struct_size = sizeof(update.color);
    if (state.UpdateEditorState(update) == INKPOD_STATUS_OK) {
        RefreshDockPaneViews(state);
    }
}

InkpodStatus ShowDrawingColorEditor(ApplicationHost& state) noexcept {
    ViewOptionsDialogState format{};
    format.title = UiText(UiStringId::Text0678);
    format.labels = {UiText(UiStringId::Text0774), UiText(UiStringId::Text0857), UiText(UiStringId::Text0040), nullptr};
    format.values = {
        state.Workspace().tools.drawing_color.depth == INKPOD_COLOR_DEPTH_16 ? 16 : 8,
        1,
        1,
        0};
    format.value_count = 3U;
    if (state.lifetime.smoke_test) {
        format.values = {16, 2, 2, 0};
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, format) != IDOK
        || (format.values[0] != 8 && format.values[0] != 16)
        || (format.values[1] != 1 && format.values[1] != 2)) {
        return INKPOD_STATUS_CANCELLED;
    }
    const std::uint32_t maximum = format.values[0] == 16 ? UINT16_MAX : UINT8_MAX;
    ViewOptionsDialogState values{};
    values.title = format.values[1] == 1
        ? UiText(UiStringId::RgbAlphaTitle)
        : UiText(UiStringId::HsvAlphaTitle);
    values.value_count = 4U;
    if (format.values[1] == 1) {
        values.labels = {
            UiText(UiStringId::ChannelR),
            UiText(UiStringId::ChannelG),
            UiText(UiStringId::ChannelB),
            format.values[2] == 2 ? UiText(UiStringId::AlphaPercentLabel)
                                  : UiText(UiStringId::AlphaLabel)};
        values.values = {
            state.Workspace().tools.drawing_color.depth == INKPOD_COLOR_DEPTH_16
                ? state.Workspace().tools.drawing_color.red
                : static_cast<std::int32_t>(state.Workspace().tools.drawing_color.red),
            state.Workspace().tools.drawing_color.green,
            state.Workspace().tools.drawing_color.blue,
            format.values[2] == 2
                ? static_cast<std::int32_t>(
                      (static_cast<std::uint64_t>(state.Workspace().tools.drawing_color.alpha) * 100U
                          + maximum / 2U)
                      / maximum)
                : state.Workspace().tools.drawing_color.alpha};
        if (state.lifetime.smoke_test) {
            values.values = {65535, 32768, 0, 50};
        }
    } else {
        values.labels = {
            UiText(UiStringId::HueRangeLabel),
            UiText(UiStringId::SaturationRangeLabel),
            UiText(UiStringId::ValueRangeLabel),
            format.values[2] == 2 ? UiText(UiStringId::AlphaPercentLabel)
                                  : UiText(UiStringId::AlphaLabel)};
        values.values = {30, 1000, 1000, format.values[2] == 2 ? 100 : static_cast<std::int32_t>(maximum)};
        if (state.lifetime.smoke_test) {
            values.values = {210, 750, 800, 50};
        }
    }
    if (ShowViewOptions(
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, values) != IDOK) {
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

bool FormatOutputColorGuardSummary(
    const InkpodOutputColorGuardResult& result, std::wstring& text) noexcept {
    try {
        text = UiText(UiStringId::Text0519)
            + std::to_wstring(result.selected_pixel_count)
            + UiText(UiStringId::Text0009) + std::to_wstring(result.scanned_pixel_count)
            + UiText(UiStringId::Text0011) + std::to_wstring(result.transparent_pixel_count);
        return true;
    } catch (const std::bad_alloc&) {
        text.clear();
        return false;
    }
}

InkpodStatus StartEffectTask(
    ApplicationHost& state,
    const CommandContext& issued_context,
    bool preview_prompt,
    std::function<InkpodStatus(InkpodCore*, InkpodTask*)> operation) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto job = state.routing.targets.BeginJob();
    if (!job.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CommandContext context = issued_context;
    context.job = job;
    state.effects.job_id = job;
    EffectsController controller(
        state.lifetime,
        state.Workspace().windows,
        state.Workspace().job_progress,
        state.Workspace().job_progress_state,
        state.effects,
        *state.engine);
    const InkpodStatus status = controller.StartTask(
        context,
        preview_prompt,
        std::move(operation),
        kEffectTaskCompleted);
    if (status != INKPOD_STATUS_OK || state.lifetime.smoke_test) {
        (void)state.routing.targets.EndJob(job.value());
        state.effects.job_id.reset();
        state.effects.completion_context = {};
    }
    return status;
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

bool PrepareFilterEditor(
    UINT command, FilterJob& job, EffectEditorState& editor) noexcept {
    job.kind = FilterKindForCommand(command);
    if (job.kind == 0U) {
        return false;
    }
    editor.title = UiText(UiStringId::Text0300);
    editor.parameter_labels = {
        UiText(UiStringId::FilterParameterRadius),
        UiText(UiStringId::FilterParameterAmount),
        UiText(UiStringId::ParameterP2),
        UiText(UiStringId::ParameterP3),
        UiText(UiStringId::ParameterP4)};
    editor.channel_labels = {
        UiText(UiStringId::ColorChannelRgb),
        UiText(UiStringId::ColorChannelRed),
        UiText(UiStringId::ColorChannelGreen),
        UiText(UiStringId::ColorChannelBlue)};
    editor.channel_values = {
        INKPOD_FILTER_CHANNEL_RGB,
        INKPOD_FILTER_CHANNEL_RED,
        INKPOD_FILTER_CHANNEL_GREEN,
        INKPOD_FILTER_CHANNEL_BLUE};
    editor.channel_count = editor.channel_labels.size();
    editor.channel = INKPOD_FILTER_CHANNEL_RGB;
    editor.mode_labels = {
        UiText(UiStringId::CurveBezier),
        UiText(UiStringId::CurveBSpline),
        nullptr,
        nullptr};
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
    return true;
}

InkpodStatus ReplaceColorChart(
    ApplicationHost& state,
    const CommandContext& context,
    const std::vector<InkpodColorValue>& colors,
    const std::vector<std::wstring>& names,
    bool locked) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ColorPanesController controller(*state.engine);
    return controller.ReplaceColorChart(
        context.document_session.value(),
        context.generation.value(),
        colors,
        names,
        locked);
}

bool QueryColorChartGenerationProgress(
    void* context, ProgressDialogInfo& output) noexcept {
    auto* job = static_cast<ColorChartGenerationJob*>(context);
    if (job == nullptr || job->task == nullptr) {
        return false;
    }
    InkpodTaskInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_task_query(job->task, &info) != INKPOD_STATUS_OK) {
        return false;
    }
    output.completed_work = info.completed_work;
    output.total_work = info.total_work;
    return true;
}

void CancelColorChartGenerationProgress(void* context) noexcept {
    auto* job = static_cast<ColorChartGenerationJob*>(context);
    if (job != nullptr && job->task != nullptr) {
        (void)inkpod_task_cancel(job->task);
    }
}

InkpodStatus StartColorChartGeneration(
    ApplicationHost& state,
    const CommandContext& context,
    std::uint32_t maximum_colors,
    std::uint32_t quantization_bits) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto& workspace = state.Workspace();
    auto& panes = workspace.panes;
    if (panes.color_chart_generation != nullptr) {
        CancelColorChartGenerationProgress(
            panes.color_chart_generation.get());
        ClearJobProgress(
            workspace.job_progress,
            workspace.job_progress_state,
            JobProgressSlot::ColorChart);
    }
    if (panes.color_chart_generation_token == UINT64_MAX) {
        return INKPOD_STATUS_INVALID_STATE;
    }

    std::shared_ptr<ColorChartGenerationJob> job;
    try {
        job = std::make_shared<ColorChartGenerationJob>();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodStatus status = inkpod_task_create(&job->task);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    job->context = context;
    job->token = ++panes.color_chart_generation_token;
    job->maximum_colors = maximum_colors;
    job->quantization_bits = quantization_bits;
    panes.color_chart_generation = job;
    const ProgressDialogState progress{
        job.get(),
        QueryColorChartGenerationProgress,
        CancelColorChartGenerationProgress,
        UiText(UiStringId::Text0053),
        UiText(UiStringId::Text0236),
        UiText(UiStringId::Cancelling)};
    if (!BindJobProgress(
            workspace.job_progress,
            workspace.job_progress_state,
            JobProgressSlot::ColorChart,
            progress)) {
        panes.color_chart_generation.reset();
        return INKPOD_STATUS_INVALID_STATE;
    }
    static_cast<void>(
        workspace.windows.dock_host.RestorePane(DockPaneType::JobProgress));
    static_cast<void>(
        workspace.windows.dock_host.ActivatePane(DockPaneType::JobProgress));

    const HWND owner = workspace.windows.window;
    if (!state.engine->Enqueue(
            context,
            [job](InkpodCore* core) {
                return inkpod_core_color_chart_preview_create_task(
                    core,
                    job->maximum_colors,
                    job->quantization_bits,
                    job->task,
                    &job->summary,
                    &job->preview);
            },
            false,
            false,
            true,
            [job, owner](InkpodStatus completion_status) {
                job->status.store(
                    completion_status, std::memory_order_release);
                const LPARAM generation = job->context.generation.has_value()
                    ? static_cast<LPARAM>(job->context.generation->Value())
                    : 0;
                (void)PostMessageW(
                    owner,
                    kColorChartGenerationCompleted,
                    static_cast<WPARAM>(job->token),
                    generation);
            })) {
        ClearJobProgress(
            workspace.job_progress,
            workspace.job_progress_state,
            JobProgressSlot::ColorChart);
        panes.color_chart_generation.reset();
        if (!HasActiveJobProgress(workspace.job_progress_state)) {
            static_cast<void>(
                workspace.windows.dock_host.HidePane(DockPaneType::JobProgress));
        }
        return INKPOD_STATUS_INVALID_STATE;
    }
    return INKPOD_STATUS_OK;
}

bool FilterJobFromEditor(
    std::uint32_t kind,
    std::uint64_t plane_id,
    const EffectEditorState& editor,
    FilterJob& job) noexcept {
    FilterJob candidate{};
    candidate.kind = kind;
    candidate.plane_id = plane_id;
    candidate.channel = editor.channel;
    candidate.interpolation = editor.mode;
    candidate.parameters = editor.parameters;
    candidate.preview = true;
    if (kind == INKPOD_FILTER_TONE_CURVE
        && !ParseCurvePoints(editor.points, candidate.points)) {
        return false;
    }
    try {
        job = std::move(candidate);
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

bool ConfigureFilterEditor(
    ApplicationHost& state, UINT command, FilterJob& job) noexcept {
    EffectEditorState editor{};
    if (!PrepareFilterEditor(command, job, editor)) {
        return false;
    }
    if (ShowEffectEditor(state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, editor) != IDOK) {
        return false;
    }
    if (!FilterJobFromEditor(job.kind, job.plane_id, editor, job)) {
        if (!state.lifetime.smoke_test) {
            MessageBoxW(
                state.Workspace().windows.window,
                UiText(UiStringId::Text0254),
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

void ResetInteractiveFilterPreview(EffectsUiState& effects) noexcept {
    auto& preview = effects.filter_preview;
    preview.context = {};
    preview.kind = 0U;
    preview.plane_id = 0U;
    preview.pending.reset();
    preview.desired_generation = 0U;
    preview.pending_generation = 0U;
    preview.running_generation = 0U;
    preview.work = inkpod::app::FilterPreviewWork::None;
    preview.session_active = false;
    preview.dialog_active = false;
    preview.accept_requested = false;
    preview.cancel_requested = false;
    preview.dialog = nullptr;
}

void RecordSmokeFilterPreview(
    EffectsUiState& effects, const InkpodFilterPreviewInfo& info) noexcept {
    auto& preview = effects.filter_preview;
    ++preview.completed_updates;
    if (preview.smoke_checksum_count < preview.smoke_checksums.size()) {
        preview.smoke_checksums[preview.smoke_checksum_count++] = info.preview_checksum;
    }
}

InkpodStatus QueueInteractiveFilterPreview(ApplicationHost& state) noexcept;
InkpodStatus FinishInteractiveFilterPreview(
    ApplicationHost& state, bool accept) noexcept;

InkpodStatus QueueInteractiveFilterFinalize(
    ApplicationHost& state, bool apply) noexcept {
    auto& preview = state.effects.filter_preview;
    if (!preview.session_active) {
        ResetInteractiveFilterPreview(state.effects);
        return apply ? INKPOD_STATUS_INVALID_STATE : INKPOD_STATUS_OK;
    }
    if (state.engine == nullptr || state.effects.task != nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.lifetime.smoke_test) {
        const InkpodStatus status = state.engine->Invoke(
            [apply](InkpodCore* core) {
                if (apply) {
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
        if (status == INKPOD_STATUS_OK) {
            ResetInteractiveFilterPreview(state.effects);
        }
        return status;
    }
    preview.work = apply
        ? inkpod::app::FilterPreviewWork::Apply
        : inkpod::app::FilterPreviewWork::Cancel;
    const InkpodStatus status = StartEffectTask(
        state,
        preview.context,
        false,
        [apply](InkpodCore* core, InkpodTask*) {
            if (apply) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_filter_preview_apply(core, &result);
            }
            InkpodFilterPreviewInfo info{};
            info.struct_size = sizeof(info);
            return inkpod_core_filter_preview_cancel(core, &info);
        });
    if (status != INKPOD_STATUS_OK) {
        preview.work = inkpod::app::FilterPreviewWork::None;
    }
    return status;
}

InkpodStatus QueueInteractiveFilterPreview(ApplicationHost& state) noexcept {
    auto& preview = state.effects.filter_preview;
    if (!preview.pending.has_value() || state.effects.task != nullptr
        || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    FilterJob job{};
    try {
        job = preview.pending.value();
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t generation = preview.pending_generation;
    const bool update = preview.session_active;
    if (state.lifetime.smoke_test) {
        InkpodFilterPreviewInfo info{};
        info.struct_size = sizeof(info);
        const InkpodStatus status = state.engine->Invoke(
            [&job, update, &info](InkpodCore* core) {
                const InkpodFilterInput input = FilterInputFor(job);
                return update
                    ? inkpod_core_filter_preview_update(core, &input, &info)
                    : inkpod_core_filter_preview_begin(core, &input, &info);
            },
            true,
            true);
        if (status == INKPOD_STATUS_OK) {
            preview.pending.reset();
            preview.pending_generation = 0U;
            preview.session_active = true;
            preview.running_generation = generation;
            RecordSmokeFilterPreview(state.effects, info);
        }
        return status;
    }
    preview.work = update
        ? inkpod::app::FilterPreviewWork::Update
        : inkpod::app::FilterPreviewWork::Begin;
    preview.running_generation = generation;
    const InkpodStatus status = StartEffectTask(
        state,
        preview.context,
        false,
        [job = std::move(job), update](InkpodCore* core, InkpodTask* task) {
            const InkpodFilterInput input = FilterInputFor(job);
            InkpodFilterPreviewInfo info{};
            info.struct_size = sizeof(info);
            return update
                ? inkpod_core_filter_preview_update_task(core, &input, task, &info)
                : inkpod_core_filter_preview_begin_task(core, &input, task, &info);
        });
    if (status == INKPOD_STATUS_OK) {
        preview.pending.reset();
        preview.pending_generation = 0U;
    } else {
        preview.work = inkpod::app::FilterPreviewWork::None;
        preview.running_generation = 0U;
    }
    return status;
}

bool RequestInteractiveFilterPreview(
    ApplicationHost& state, FilterJob job) noexcept {
    auto& preview = state.effects.filter_preview;
    if (!preview.dialog_active || preview.cancel_requested
        || preview.desired_generation == UINT64_MAX) {
        return false;
    }
    try {
        preview.pending = std::move(job);
    } catch (const std::bad_alloc&) {
        return false;
    }
    preview.pending_generation = ++preview.desired_generation;
    if (state.effects.task != nullptr) {
        if (preview.work == inkpod::app::FilterPreviewWork::Begin
            || preview.work == inkpod::app::FilterPreviewWork::Update) {
            inkpod_task_cancel(state.effects.task);
            return true;
        }
        return false;
    }
    return QueueInteractiveFilterPreview(state) == INKPOD_STATUS_OK;
}

bool FilterEditorPreviewChanged(
    void* context, const EffectEditorState& editor) noexcept {
    auto* state = static_cast<ApplicationHost*>(context);
    if (state == nullptr) {
        return false;
    }
    auto& preview = state->effects.filter_preview;
    preview.dialog = editor.dialog;
    FilterJob job{};
    if (!FilterJobFromEditor(
            preview.kind,
            preview.plane_id,
            editor,
            job)) {
        return false;
    }
    return RequestInteractiveFilterPreview(*state, std::move(job));
}

bool FilterEditorPreviewProgress(
    void* context, ProgressDialogInfo& output) noexcept {
    auto* state = static_cast<ApplicationHost*>(context);
    return state != nullptr
        && EffectsController::QueryProgress(&state->effects, output);
}

InkpodStatus FinishInteractiveFilterPreview(
    ApplicationHost& state, bool accept) noexcept {
    auto& preview = state.effects.filter_preview;
    preview.dialog_active = false;
    preview.dialog = nullptr;
    preview.accept_requested = accept;
    preview.cancel_requested = !accept;
    if (!accept) {
        preview.pending.reset();
        preview.pending_generation = 0U;
        if (state.effects.task != nullptr) {
            inkpod_task_cancel(state.effects.task);
            return INKPOD_STATUS_OK;
        }
        return preview.session_active
            ? QueueInteractiveFilterFinalize(state, false)
            : (ResetInteractiveFilterPreview(state.effects), INKPOD_STATUS_OK);
    }
    if (state.effects.task != nullptr) {
        return INKPOD_STATUS_OK;
    }
    if (preview.pending.has_value()) {
        return QueueInteractiveFilterPreview(state);
    }
    return QueueInteractiveFilterFinalize(state, true);
}

InkpodStatus RunInteractiveFilterEditor(
    ApplicationHost& state,
    const CommandContext& context,
    UINT command,
    std::uint64_t plane_id) noexcept {
    if (state.engine == nullptr || state.effects.task != nullptr || plane_id == 0U
        || state.effects.filter_preview.work
            != inkpod::app::FilterPreviewWork::None
        || state.effects.filter_preview.session_active) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    FilterJob defaults{};
    EffectEditorState editor{};
    if (!PrepareFilterEditor(command, defaults, editor)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const bool smoke_cancel = state.effects.filter_preview.smoke_cancel_next;
    ResetInteractiveFilterPreview(state.effects);
    auto& preview = state.effects.filter_preview;
    preview.context = context;
    preview.kind = defaults.kind;
    preview.plane_id = plane_id;
    preview.dialog_active = true;
    preview.smoke_cancel_next = false;

    editor.option1_label = UiText(UiStringId::Text0382);
    editor.option1 = true;
    editor.option1_enabled = false;
    editor.preview_context = &state;
    editor.preview_change = FilterEditorPreviewChanged;
    editor.preview_progress = FilterEditorPreviewProgress;
    editor.smoke_cancel = smoke_cancel;
    const INT_PTR result = ShowEffectEditor(
        state.lifetime.instance,
        state.Workspace().windows.window,
        state.lifetime.smoke_test,
        editor);
    preview.dialog = nullptr;
    return FinishInteractiveFilterPreview(state, result == IDOK);
}

void CompleteInteractiveFilterWork(
    ApplicationHost& state,
    InkpodStatus status,
    bool document_current) noexcept {
    auto& preview = state.effects.filter_preview;
    const inkpod::app::FilterPreviewWork work = preview.work;
    preview.work = inkpod::app::FilterPreviewWork::None;
    preview.running_generation = 0U;

    if (!document_current) {
        if (preview.dialog != nullptr && IsWindow(preview.dialog) != FALSE) {
            PostMessageW(preview.dialog, WM_COMMAND, IDCANCEL, 0);
        }
        ResetInteractiveFilterPreview(state.effects);
        return;
    }
    if (work == inkpod::app::FilterPreviewWork::Apply) {
        if (status == INKPOD_STATUS_OK) {
            ResetInteractiveFilterPreview(state.effects);
            return;
        }
        preview.accept_requested = false;
        preview.cancel_requested = true;
        if (preview.session_active) {
            static_cast<void>(QueueInteractiveFilterFinalize(state, false));
        } else {
            ResetInteractiveFilterPreview(state.effects);
        }
        ShowCoreError(state, state.Workspace().windows.window, UiText(UiStringId::Text0810));
        return;
    }
    if (work == inkpod::app::FilterPreviewWork::Cancel) {
        if (status != INKPOD_STATUS_OK
            && status != INKPOD_STATUS_INVALID_STATE) {
            ShowCoreError(state, state.Workspace().windows.window, UiText(UiStringId::Text0809));
        }
        ResetInteractiveFilterPreview(state.effects);
        return;
    }
    if (work != inkpod::app::FilterPreviewWork::Begin
        && work != inkpod::app::FilterPreviewWork::Update) {
        return;
    }

    if (status == INKPOD_STATUS_OK) {
        preview.session_active = true;
        ++preview.completed_updates;
        SetEffectEditorPreviewStatus(
            preview.dialog,
            preview.pending.has_value()
                ? UiText(UiStringId::Text0124)
                : UiText(UiStringId::Text0049));
    } else if (status != INKPOD_STATUS_CANCELLED) {
        SetEffectEditorPreviewStatus(
            preview.dialog,
            UiText(UiStringId::Text0105));
    }

    if (preview.cancel_requested) {
        preview.pending.reset();
        preview.pending_generation = 0U;
        if (preview.session_active) {
            static_cast<void>(QueueInteractiveFilterFinalize(state, false));
        } else {
            ResetInteractiveFilterPreview(state.effects);
        }
        return;
    }
    if (preview.pending.has_value()) {
        const InkpodStatus queued = QueueInteractiveFilterPreview(state);
        if (queued != INKPOD_STATUS_OK) {
            SetEffectEditorPreviewStatus(
                preview.dialog, UiText(UiStringId::Text0758));
        }
        return;
    }
    if (preview.accept_requested) {
        if (status == INKPOD_STATUS_OK) {
            static_cast<void>(QueueInteractiveFilterFinalize(state, true));
        } else if (preview.session_active) {
            static_cast<void>(QueueInteractiveFilterFinalize(state, false));
        } else {
            ResetInteractiveFilterPreview(state.effects);
        }
    }
}

bool ConfigureAdjustmentEditor(
    ApplicationHost& state, FilterJob& job, bool update) noexcept {
    EffectEditorState editor{};
    editor.title = UiText(UiStringId::Text0933);
    editor.parameter_labels = {
        UiText(UiStringId::Text0067),
        UiText(UiStringId::AdjustmentContrastGamma),
        UiText(UiStringId::AdjustmentHighlight),
        UiText(UiStringId::AdjustmentOutputShadow),
        UiText(UiStringId::AdjustmentOutputHighlight)};
    editor.channel_labels = {
        UiText(UiStringId::Text0725), UiText(UiStringId::Text0251), UiText(UiStringId::Text0407), nullptr, nullptr};
    editor.channel_values = {
        INKPOD_FILTER_BRIGHTNESS_CONTRAST,
        INKPOD_FILTER_TONE_CURVE,
        INKPOD_FILTER_LEVELS,
        0U,
        0U};
    editor.channel_count = 3U;
    editor.channel = INKPOD_FILTER_BRIGHTNESS_CONTRAST;
    editor.mode_labels = {
        UiText(UiStringId::CurveBezier),
        UiText(UiStringId::CurveBSpline),
        nullptr,
        nullptr};
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
    if (ShowEffectEditor(state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, editor) != IDOK) {
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
                state.Workspace().windows.window,
                UiText(UiStringId::Text0253),
                L"inkpod",
                MB_OK | MB_ICONWARNING);
        }
        return false;
    }
    return true;
}

InkpodStatus CreateOrUpdateAdjustment(
    ApplicationHost& state, FilterJob job, bool update) noexcept {
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

InkpodStatus SetAdjustmentVisibility(ApplicationHost& state, bool visible) noexcept {
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

bool SelectAdjustment(ApplicationHost& state, bool next) noexcept {
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

InkpodStatus QueueBoundaryAirbrush(
    ApplicationHost& state,
    const CommandContext& context,
    const CanvasEffectOptions& options) noexcept {
    const InkpodEditorStateInfo* editor = PresentedEditorState(state);
    if (state.engine == nullptr || editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_plane_id == 0U) {
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
    const std::uint64_t plane_id = editor->active_plane_id;
    return state.engine->Enqueue(
               context,
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

bool PrepareCanvasEffectEditor(
    UINT command,
    EffectEditorState& editor,
    std::uint32_t& interaction) noexcept {
    editor = {};
    editor.option1 = false;
    editor.option2 = false;
    editor.channel_labels = {UiText(UiStringId::Text0345), UiText(UiStringId::Text0821), UiText(UiStringId::ToolPolyline), UiText(UiStringId::Text0664), nullptr};
    editor.channel_values = {
        INKPOD_SELECTION_TRACE,
        INKPOD_SELECTION_RECTANGLE,
        INKPOD_SELECTION_POLYLINE,
        INKPOD_SELECTION_LASSO,
        0U};
    editor.channel_count = 4U;
    editor.channel = INKPOD_SELECTION_TRACE;
    editor.points = L"0:00000000;500:80808080;1000:ffffffff";
    interaction = 0U;
    switch (command) {
        case IDM_EFFECT_GRADIENT:
        case IDM_EFFECT_ALPHA_GRADIENT:
            editor.title = command == IDM_EFFECT_GRADIENT
                ? UiText(UiStringId::Text0177)
                : UiText(UiStringId::Text0129);
            editor.parameter_labels = {UiText(UiStringId::Text0746), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746)};
            editor.channel_labels = {UiText(UiStringId::Text0561), UiText(UiStringId::Text0428), nullptr, nullptr, nullptr};
            editor.channel_values = {
                INKPOD_GRADIENT_COMPOSITE, INKPOD_GRADIENT_OVERWRITE, 0U, 0U, 0U};
            editor.channel_count = 2U;
            editor.channel = INKPOD_GRADIENT_OVERWRITE;
            editor.mode_labels = {UiText(UiStringId::Text0850), UiText(UiStringId::Text0693), nullptr, nullptr};
            editor.mode_values = {INKPOD_GRADIENT_LINEAR, INKPOD_GRADIENT_RADIAL, 0U, 0U};
            editor.mode_count = 2U;
            editor.mode = INKPOD_GRADIENT_LINEAR;
            editor.option1_label = UiText(UiStringId::Text0249);
            editor.option2_label = UiText(UiStringId::Text0033);
            interaction = command == IDM_EFFECT_GRADIENT ? kInteractionEffectGradient
                                                         : kInteractionEffectAlphaGradient;
            break;
        case IDM_EFFECT_AIRBRUSH:
            editor.title = UiText(UiStringId::Text0136);
            editor.parameter_labels = {
                UiText(UiStringId::Text0544),
                UiText(UiStringId::Text0822),
                UiText(UiStringId::Text1026),
                UiText(UiStringId::Opacity),
                UiText(UiStringId::FadeLabel)};
            editor.parameters = {8000, 500, 2000, 500, 0};
            editor.channel_count = 0U;
            editor.mode_count = 0U;
            editor.points.clear();
            editor.option1 = true;
            editor.option2 = true;
            editor.option1_label = UiText(UiStringId::Text0836);
            editor.option2_label = UiText(UiStringId::Text0835);
            interaction = kInteractionEffectAirbrush;
            break;
        case IDM_EFFECT_BOUNDARY_AIRBRUSH:
            editor.title = UiText(UiStringId::Text0605);
            editor.parameter_labels = {UiText(UiStringId::Text0648), UiText(UiStringId::Text0649), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746)};
            editor.parameters = {3, 500, 0, 0, 0};
            editor.channel_count = 0U;
            editor.mode_count = 0U;
            editor.points = L"0:ff0000ff;500:00ff00ff;1000:0000ffff";
            editor.option1_enabled = false;
            editor.option2_enabled = false;
            break;
        case IDM_EFFECT_BLUR:
            editor.title = UiText(UiStringId::Text0118);
            editor.parameter_labels = {
                UiText(UiStringId::Text0119), UiText(UiStringId::Text0649), UiText(UiStringId::Text0346), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746)};
            editor.parameters = {3, 750, 24, 0, 0};
            editor.mode_count = 0U;
            editor.points.clear();
            editor.option1 = true;
            editor.option1_label = UiText(UiStringId::Text0348);
            editor.option2_enabled = false;
            interaction = kInteractionEffectBlur;
            break;
        case IDM_EFFECT_STAMP:
            editor.title = UiText(UiStringId::Text0210);
            editor.parameter_labels = {
                UiText(UiStringId::Text0544), UiText(UiStringId::Text0822), UiText(UiStringId::Text1026), UiText(UiStringId::Opacity), UiText(UiStringId::Text0746)};
            editor.parameters = {8000, 750, 2000, 1000, 0};
            editor.channel_count = 0U;
            editor.mode_labels = {UiText(UiStringId::Text0508), UiText(UiStringId::Text0821), nullptr, nullptr};
            editor.mode_values = {INKPOD_STAMP_ROUND, INKPOD_STAMP_SQUARE, 0U, 0U};
            editor.mode_count = 2U;
            editor.mode = INKPOD_STAMP_ROUND;
            editor.points.clear();
            editor.option1 = true;
            editor.option2 = true;
            editor.option1_label = UiText(UiStringId::Text0836);
            editor.option2_label = UiText(UiStringId::Text0835);
            interaction = kInteractionEffectStamp;
            break;
        case IDM_EFFECT_DUST:
            editor.title = UiText(UiStringId::Text0188);
            editor.parameter_labels = {
                UiText(UiStringId::Text0731), UiText(UiStringId::Text0346), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746), UiText(UiStringId::Text0746)};
            editor.parameters = {8, 24, 0, 0, 0};
            editor.channel_labels = {UiText(UiStringId::Text0501), UiText(UiStringId::Text0345), UiText(UiStringId::Text0821), UiText(UiStringId::ToolPolyline), UiText(UiStringId::Text0664)};
            editor.channel_values = {
                0U,
                INKPOD_SELECTION_TRACE,
                INKPOD_SELECTION_RECTANGLE,
                INKPOD_SELECTION_POLYLINE,
                INKPOD_SELECTION_LASSO};
            editor.channel_count = 5U;
            editor.channel = 0U;
            editor.mode_labels = {UiText(UiStringId::Text0542), UiText(UiStringId::Text0954), UiText(UiStringId::Text0871), nullptr};
            editor.mode_values = {
                INKPOD_DUST_REMOVE_FOREGROUND,
                INKPOD_DUST_FILL_TRANSPARENT_HOLES,
                INKPOD_DUST_REPLACE_COLOR_OUTLIERS,
                0U};
            editor.mode_count = 3U;
            editor.mode = INKPOD_DUST_REMOVE_FOREGROUND;
            editor.points.clear();
            editor.option1 = true;
            editor.option1_label = UiText(UiStringId::Text0314);
            editor.option2_enabled = false;
            interaction = kInteractionEffectDust;
            break;
        default:
            return false;
    }
    return true;
}

bool CanvasEffectOptionsFromEditor(
    UINT command,
    const EffectEditorState& editor,
    CanvasEffectOptions& options) noexcept {
    options = {};
    options.parameters = editor.parameters;
    options.shape = editor.channel;
    options.mode = editor.mode;
    options.option = editor.option1;
    options.option2 = editor.option2;
    if (command == IDM_EFFECT_GRADIENT || command == IDM_EFFECT_ALPHA_GRADIENT
        || command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        const std::size_t minimum_stops = command == IDM_EFFECT_BOUNDARY_AIRBRUSH ? 2U : 3U;
        if (!ParseGradientStops(editor.points, options.stops, minimum_stops)) {
            return false;
        }
    }
    if (command == IDM_EFFECT_GRADIENT || command == IDM_EFFECT_ALPHA_GRADIENT) {
        options.parameters[0] = editor.option1 ? 1 : 0;
    }
    return true;
}

void ApplyCanvasEffectOptionsToEditor(
    const CanvasEffectOptions& options,
    EffectEditorState& editor) noexcept {
    editor.parameters = options.parameters;
    editor.channel = options.shape;
    editor.mode = options.mode;
    editor.option1 = options.option;
    editor.option2 = options.option2;
    if (!options.stops.empty()) {
        std::wstring points;
        try {
            for (std::size_t index = 0U; index < options.stops.size(); ++index) {
                if (index != 0U) points.push_back(L';');
                std::array<wchar_t, 32U> entry{};
                _snwprintf_s(
                    entry.data(),
                    entry.size(),
                    _TRUNCATE,
                    L"%u:%08x",
                    options.stops[index].position_milli,
                    options.stops[index].rgba);
                points.append(entry.data());
            }
            editor.points = std::move(points);
        } catch (const std::bad_alloc&) {
        }
    }
}

bool SelectCanvasEffect(ApplicationHost& state, UINT command) noexcept {
    EffectEditorState editor{};
    std::uint32_t interaction{};
    if (!PrepareCanvasEffectEditor(command, editor, interaction)
        || command == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
        return false;
    }
    CanvasEffectOptions options{};
    if (!CanvasEffectOptionsFromEditor(command, editor, options)) {
        return false;
    }
    state.effects.options_command = command;
    state.effects.options = std::move(options);
    if (SetEditorActiveTool(state, interaction) != INKPOD_STATUS_OK) {
        return false;
    }
    state.effects.samples.clear();
    return true;
}

InkpodStatus QueueGradientGesture(
    ApplicationHost& state,
    const CommandContext& context,
    std::vector<InkpodStrokeSample> samples,
    bool alpha_only) noexcept {
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (samples.size() < 2U || state.engine == nullptr || editor == nullptr
        || !state.effects.gesture_options_valid
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodGradientStop> stops;
    try {
        stops = GradientStops(state.effects.gesture_options.stops);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.gesture_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = editor->active_plane_id;
    return state.engine->Enqueue(
               context,
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
    ApplicationHost& state,
    const CommandContext& context,
    std::vector<InkpodStrokeSample> samples) noexcept {
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (samples.empty() || state.engine == nullptr || editor == nullptr
        || !state.effects.gesture_options_valid
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.gesture_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodColorValue color = editor->current_color;
    const std::uint64_t plane_id = editor->active_plane_id;
    return state.engine->Enqueue(
               context,
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
    ApplicationHost& state,
    const CommandContext& context,
    std::vector<InkpodStrokeSample> samples) noexcept {
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (!state.effects.stamp_source_valid || samples.empty() || state.engine == nullptr
        || editor == nullptr || !state.effects.gesture_options_valid
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.gesture_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStrokeSample source = state.effects.stamp_source;
    const std::uint64_t plane_id = editor->active_plane_id;
    return state.engine->Enqueue(
               context,
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
    ApplicationHost& state,
    const CommandContext& context,
    std::vector<InkpodStrokeSample> samples) noexcept {
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (samples.empty() || state.engine == nullptr || editor == nullptr
        || !state.effects.gesture_options_valid
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.gesture_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = editor->active_plane_id;
    return state.engine->Enqueue(
               context,
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
    ApplicationHost& state,
    const CommandContext& context,
    std::vector<InkpodStrokeSample> samples) noexcept {
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (editor == nullptr || !state.effects.gesture_options_valid
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CanvasEffectOptions options{};
    try {
        options = state.effects.gesture_options;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint64_t plane_id = editor->active_plane_id;
    const bool preview = options.option;
    return StartEffectTask(
        state,
        context,
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

InkpodStatus FinishEffectGesture(
    ApplicationHost& state,
    const CommandContext& context) noexcept {
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (editor == nullptr) {
        state.effects.samples.clear();
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodStrokeSample> samples;
    samples.swap(state.effects.samples);
    switch (editor->active_tool) {
        case kInteractionEffectGradient:
            return QueueGradientGesture(state, context, std::move(samples), false);
        case kInteractionEffectAlphaGradient:
            return QueueGradientGesture(state, context, std::move(samples), true);
        case kInteractionEffectAirbrush:
            return QueueAirbrushGesture(state, context, std::move(samples));
        case kInteractionEffectBlur:
            return QueueBlurGesture(state, context, std::move(samples));
        case kInteractionEffectStamp:
            return QueueStampGesture(state, context, std::move(samples));
        case kInteractionEffectDust:
            return QueueDustRemoval(state, context, std::move(samples));
        default:
            return INKPOD_STATUS_INVALID_STATE;
    }
}

CommandStateInputs BuildCommandStateInputs(
    ApplicationHost& state,
    const InkpodDocumentInfo& info,
    bool has_document,
    const InkpodHistoryInfo& history,
    bool has_history) noexcept {
    CommandStateInputs inputs{};
    const inkpod::app::DocumentSession* document = has_document
        ? state.Documents().Find(state.routing.targets.DocumentSession())
        : nullptr;
    const inkpod::app::DocumentView* active_view = document == nullptr
        ? nullptr
        : document->FindView(state.routing.targets.ActiveDocumentView());
    has_document = has_document && document != nullptr && active_view != nullptr;
    inputs.document.has_document = has_document;
    inputs.document.has_saved_path = has_document
        && !document->shell.current_path.empty();
    inputs.document.dirty =
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U;
    InkpodShootingFrameInfo shooting_frame{};
    bool shooting_frame_present{};
    inputs.document.shooting_frame_present = has_document
        && QueryShootingFrame(state, shooting_frame_present, shooting_frame)
        && shooting_frame_present;
    inputs.document.shooting_frame_handle_edit =
        state.Workspace().tools.active_tool == kInteractionShootingFrame;
    std::vector<InkpodVanishingPointInfo> vanishing_points;
    inputs.document.vanishing_point_present = has_document
        && QueryVanishingPoints(state, vanishing_points)
        && !vanishing_points.empty();
    inputs.document.vanishing_point_handle_edit =
        state.Workspace().tools.active_tool == kInteractionVanishingPoint;
    inputs.document.recent_document_count = state.RecentDocumentCount();
    inputs.application.restore_previous_documents =
        state.lifetime.restore_previous_documents;
    inputs.application.sequence_autosave_before_switch =
        state.lifetime.sequence_switch_policy
        == inkpod::app::SequenceCellSwitchPolicy::AutosaveBeforeSwitch;
    inputs.application.sequence_wrap_endpoints =
        state.lifetime.sequence_endpoint_policy
        == inkpod::app::SequenceEndpointPolicy::Wrap;
    inputs.application.ui_language_preference =
        static_cast<std::uint32_t>(CurrentUiLanguagePreference());

    inputs.edit.can_undo =
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_UNDO) != 0U;
    inputs.edit.can_redo =
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_CAN_REDO) != 0U;
    inputs.edit.can_history_back = has_history && history.cursor > 0U;
    inputs.edit.can_history_forward =
        has_history && history.cursor < history.item_count;
    inputs.edit.clipboard_available = state.clipboard != nullptr
        || (InkpodClipboardFormat() != 0U
            && IsClipboardFormatAvailable(InkpodClipboardFormat()) != FALSE);
    inputs.edit.floating_active = state.Workspace().tools.floating_active;

    inputs.effects.color_plane_active =
        state.Workspace().tools.active_plane == INKPOD_PLANE_COLOR;
    inputs.effects.adjustment_available = state.effects.adjustment_id != 0U;
    inputs.effects.multiple_adjustments = state.effects.adjustments.size() > 1U;
    inputs.effects.adjustment_visible = state.effects.adjustment_visible;
    inputs.effects.alpha_view = state.effects.alpha_view;

    inputs.document_pane.removable_layer_available =
        state.Workspace().panes.tree_layer_count > 1U;
    inputs.document_pane.layer_palette_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Layer);

    inputs.animation.motion_fps = state.Workspace().animation.motion_fps;
    inputs.animation.sequence_switch_pending =
        state.Workspace().animation.sequence_switch_pending;

    inputs.selection_view.active_tool = state.Workspace().tools.active_tool;
    inputs.selection_view.selection_shape = state.Workspace().tools.selection_shape;
    inputs.selection_view.selection_operation = state.Workspace().tools.selection_operation;
    inputs.selection_view.flip_horizontal = active_view != nullptr
        && active_view->presentation.flip_horizontal;
    inputs.selection_view.flip_vertical = active_view != nullptr
        && active_view->presentation.flip_vertical;
    inputs.selection_view.ruler_visible = active_view != nullptr
        && active_view->presentation.ruler_visible;
    inputs.selection_view.guides_visible = active_view != nullptr
        && active_view->presentation.guides_visible;
    inputs.selection_view.grid_visible = active_view != nullptr
        && active_view->presentation.grid_visible;
    inputs.selection_view.snap_guides = active_view != nullptr
        && active_view->presentation.snap_guides;
    inputs.selection_view.snap_grid = active_view != nullptr
        && active_view->presentation.snap_grid;
    inputs.selection_view.transparent_visible = active_view != nullptr
        && active_view->presentation.transparent_visible;
    inputs.selection_view.selection_layer_available =
        document != nullptr && document->shell.selection_layer_id != 0U;
    inputs.selection_view.document_count = state.Documents().Count();
    inputs.selection_view.view_count = document == nullptr ? 0U : document->ViewCount();
    inputs.selection_view.editor_group_count =
        state.Workspace().editors.GroupCount();
    const EditorGroup* active_editor_group = state.Workspace().editors.Active();
    inputs.selection_view.active_group_view_count = active_editor_group == nullptr
        ? 0U
        : active_editor_group->ViewCount();
    inputs.selection_view.active_tab_index = active_editor_group == nullptr
            || !active_editor_group->ViewIndex(
                state.routing.targets.ActiveDocumentView()).has_value()
        ? 0U
        : active_editor_group->ViewIndex(
            state.routing.targets.ActiveDocumentView()).value();
    inputs.selection_view.workspace_count = state.Workspaces().Count();

    TreePaneNode active_tool_plane{};
    inputs.tool.geometry_drawable_plane = has_document
        && QueryTreeNode(state, true, active_tool_plane)
        && active_tool_plane.id != 0U
        && IsGeometryCanvasPlane(active_tool_plane.kind);
    inputs.tool.active_tool = state.Workspace().tools.active_tool;
    inputs.tool.active_plane = state.Workspace().tools.active_plane;
    inputs.tool.fill_operation = state.Workspace().tools.fill_options.operation;
    inputs.tool.color_replace_shape = state.Workspace().tools.color_replace_shape;
    inputs.tool.palette_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool);

    inputs.color.eyedropper_source = state.Workspace().tools.eyedropper_source;
    inputs.color.color_check_mode = active_view != nullptr
        && active_view->presentation.color_check_mode;
    inputs.color.chart_locked = state.Workspace().panes.color_chart_locked;

    inputs.batch.idle = state.batch.task == nullptr;
    inputs.batch.has_operations = !state.batch.operations.empty();
    inputs.batch.editable_item = inputs.batch.idle
        && state.batch.selected_stage > 0U
        && state.batch.selected_stage == state.batch.selected_operation + 1U
        && state.batch.selected_operation < state.batch.operations.size();
    inputs.batch.palette_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Batch);
    inputs.batch.output_destination = state.batch.output_destination;
    inputs.batch.failure_policy = state.batch.failure_policy;
    inputs.workspace.tool_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Tool);
    inputs.workspace.tool_options_visible =
        inkpod::windows::ui::panes::IsToolOptionsFlyoutVisible(
            state.Workspace().windows.tool_options_flyout);
    inputs.workspace.color_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Color);
    const auto* color_binding = state.routing.pane_targets.Find(
        state.routing.color_pane);
    inputs.workspace.color_pinned = color_binding != nullptr
        && color_binding->policy == PaneTargetPolicy::PinnedDocument;
    inputs.workspace.color_target_available =
        state.routing.pane_targets.CaptureAction(
            state.routing.color_pane,
            state.routing.targets.Capture(),
            state.routing.targets).status == PaneTargetStatus::Ok;
    inputs.workspace.layer_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Layer);
    inputs.workspace.locator_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Locator);
    const auto* locator_binding = state.routing.pane_targets.Find(
        state.routing.locator_pane);
    inputs.workspace.locator_pinned = locator_binding != nullptr
        && locator_binding->policy == PaneTargetPolicy::PinnedDocument;
    inputs.workspace.locator_fixed = state.Workspace().locator_fixed_mode;
    inputs.workspace.locator_auto_scroll =
        state.Workspace().locator_auto_scroll;
    inputs.workspace.locator_target_available =
        state.routing.pane_targets.CaptureAction(
            state.routing.locator_pane,
            state.routing.targets.Capture(),
            state.routing.targets).status == PaneTargetStatus::Ok;
    inputs.workspace.sequence_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Sequence);
    const auto* sequence_binding = state.routing.pane_targets.Find(
        state.routing.sequence_pane);
    inputs.workspace.sequence_pinned = sequence_binding != nullptr
        && sequence_binding->policy == PaneTargetPolicy::PinnedDocument;
    inputs.workspace.sequence_target_available =
        state.routing.pane_targets.CaptureAction(
            state.routing.sequence_pane,
            state.routing.targets.Capture(),
            state.routing.targets).status == PaneTargetStatus::Ok;
    inputs.workspace.light_table_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::LightTable);
    const auto* light_table_binding = state.routing.pane_targets.Find(
        state.routing.light_table_pane);
    inputs.workspace.light_table_pinned = light_table_binding != nullptr
        && light_table_binding->policy == PaneTargetPolicy::PinnedDocument;
    inputs.workspace.light_table_target_available =
        state.routing.pane_targets.CaptureAction(
            state.routing.light_table_pane,
            state.routing.targets.Capture(),
            state.routing.targets).status == PaneTargetStatus::Ok;
    inputs.workspace.subpalette_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::Reference);
    const auto* subpalette_binding = state.routing.pane_targets.Find(
        state.routing.subpalette_pane);
    inputs.workspace.subpalette_pinned = subpalette_binding != nullptr
        && subpalette_binding->policy == PaneTargetPolicy::PinnedDocument;
    inputs.workspace.subpalette_target_available =
        state.routing.pane_targets.CaptureAction(
            state.routing.subpalette_pane,
            state.routing.targets.Capture(),
            state.routing.targets).status == PaneTargetStatus::Ok;
    const auto* batch_binding = state.routing.pane_targets.Find(
        state.routing.batch_pane);
    inputs.workspace.batch_pinned = batch_binding != nullptr
        && batch_binding->policy == PaneTargetPolicy::PinnedDocument;
    inputs.workspace.batch_target_available =
        inputs.batch.idle && state.routing.pane_targets.CaptureAction(
            state.routing.batch_pane,
            state.routing.targets.Capture(),
            state.routing.targets).status == PaneTargetStatus::Ok;
    inputs.workspace.job_progress_visible =
        state.Workspace().windows.workspace.dock.IsPaneVisible(
            DockPaneType::JobProgress);
    inputs.workspace.mirrored =
        state.Workspace().windows.workspace.dock.Mirrored();
    inputs.workspace.selected_workspace_preset = static_cast<std::uint32_t>(
        state.Workspace().windows.workspace.selected_preset);
    const auto auto_hidden = [&state](WorkspaceAuxiliaryPane type) {
        const DockPanePlacement* pane =
            state.Workspace().windows.workspace.dock.Pane(
                inkpod::windows::ui::DockPaneTypeForAuxiliary(type));
        return pane != nullptr && pane->zone == DockZone::AutoHide;
    };
    inputs.workspace.locator_auto_hidden = auto_hidden(
        WorkspaceAuxiliaryPane::Locator);
    inputs.workspace.sequence_auto_hidden = auto_hidden(
        WorkspaceAuxiliaryPane::Sequence);
    inputs.workspace.light_table_auto_hidden = auto_hidden(
        WorkspaceAuxiliaryPane::LightTable);
    inputs.workspace.reference_auto_hidden = auto_hidden(
        WorkspaceAuxiliaryPane::Reference);
    inputs.workspace.batch_auto_hidden = auto_hidden(
        WorkspaceAuxiliaryPane::Batch);
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

void UpdateLocatorStatus(const ApplicationHost& state) noexcept {
    if (state.Workspace().windows.status_bar == nullptr) {
        return;
    }
    std::array<wchar_t, 128U> coordinate_text{};
    if (state.ActiveView().presentation.locator_valid) {
        _snwprintf_s(
            coordinate_text.data(),
            coordinate_text.size(),
            _TRUNCATE,
            L"X %d  Y %d",
            state.ActiveView().presentation.locator.document_x,
            state.ActiveView().presentation.locator.document_y);
    } else {
        wcscpy_s(coordinate_text.data(), coordinate_text.size(), L"X --  Y --");
    }
    std::array<wchar_t, 160U> sample_text{};
    if (state.ActiveView().presentation.locator_valid
        && (state.ActiveView().presentation.locator.flags & INKPOD_LOCATOR_COLOR_PRESENT) != 0U) {
        const auto& color = state.ActiveView().presentation.locator.color;
        _snwprintf_s(
            sample_text.data(),
            sample_text.size(),
            _TRUNCATE,
            (state.ActiveView().presentation.locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) != 0U
                ? L"RGBA %u,%u,%u,%u | W %d H %d"
                : L"RGBA %u,%u,%u,%u",
            color.red,
            color.green,
            color.blue,
            color.alpha,
            state.ActiveView().presentation.locator.selection.width,
            state.ActiveView().presentation.locator.selection.height);
    } else {
        wcscpy_s(sample_text.data(), sample_text.size(), L"RGBA --");
    }
    StatusBarPresentation presentation{};
    presentation.parts[2] = coordinate_text.data();
    presentation.parts[3] = sample_text.data();
    PresentStatusBar(state.Workspace().windows.status_bar, presentation);
}

bool FormatTaskProgressStatus(
    const ApplicationHost& state,
    std::array<wchar_t, 192U>& text) noexcept {
    InkpodTaskInfo task_info{};
    task_info.struct_size = sizeof(task_info);
    const wchar_t* task_name = UiText(UiStringId::Text0511);
    bool running{};
    if (state.effects.task != nullptr
        && inkpod_task_query(state.effects.task, &task_info) == INKPOD_STATUS_OK
        && task_info.state == INKPOD_TASK_RUNNING) {
        running = true;
        task_name = UiText(UiStringId::Text0806);
    } else if (state.batch.task != nullptr
        && inkpod_batch_task_query(state.batch.task, &task_info) == INKPOD_STATUS_OK
        && task_info.state == INKPOD_TASK_RUNNING) {
        running = true;
        task_name = UiText(UiStringId::Text0255);
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

std::wstring DocumentTabBaseName(
    const ApplicationHost& state,
    const inkpod::app::DocumentSession& document,
    const InkpodDocumentInfo& info,
    bool has_document) {
    if (!has_document) {
        return UiText(UiStringId::NoDocument);
    }
    if (&document == state.Documents().Current()
        && !state.Workspace().animation.active_sequence_name.empty()) {
        return state.Workspace().animation.active_sequence_name;
    }
    const std::wstring& path = !document.shell.current_path.empty()
        ? document.shell.current_path
        : document.shell.source_path;
    if (!path.empty()) {
        const wchar_t* leaf = path.c_str();
        for (const wchar_t* cursor = leaf; *cursor != L'\0'; ++cursor) {
            if (*cursor == L'\\' || *cursor == L'/') {
                leaf = cursor + 1;
            }
        }
        if (*leaf != L'\0') {
            return leaf;
        }
    }
    if ((info.flags & INKPOD_DOCUMENT_FLAG_RECOVERED) != 0U) {
        return UiText(UiStringId::RecoveredCell);
    }
    const wchar_t* prefix = UiText(UiStringId::Text0777);
    return prefix
        + std::to_wstring(document.untitled_number == 0U
            ? 1U
            : document.untitled_number);
}

void UpdateDocumentTabLabels(
    ApplicationHost& state,
    const InkpodDocumentInfo& info,
    bool has_document) noexcept {
    static_assert(sizeof(LPARAM) >= sizeof(std::uint64_t));
    try {
        for (std::size_t group_index = 0U;
             group_index < state.Workspace().editors.GroupCount();
             ++group_index) {
            const auto* group = state.Workspace().editors.GroupAt(group_index);
            if (group == nullptr || group->document_tabs == nullptr) {
                continue;
            }
            int tab_index{};
            int selected_index{-1};
            for (std::size_t placement_index = 0U;
                 placement_index < group->ViewCount();
                 ++placement_index) {
                const DocumentViewId placed_view = group->ViewAt(placement_index);
                const auto* document = state.Documents().FindByView(placed_view);
                const auto* view = document == nullptr
                    ? nullptr
                    : document->FindView(placed_view);
                if (document == nullptr || view == nullptr) {
                    continue;
                }
                InkpodDocumentInfo document_info = EmptyDocumentInfo();
                const bool document_available = document == state.Documents().Current()
                    ? (document_info = info, has_document)
                    : state.engine != nullptr
                        && state.engine->GetDocumentInfo(
                            document->id, document->generation, document_info);
                const std::wstring base_name = DocumentTabBaseName(
                    state, *document, document_info, document_available);
                const wchar_t* dirty = document_available
                        && (document_info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
                    ? L" *"
                    : L"";
                std::size_t document_view_index{};
                for (; document_view_index < document->ViewCount();
                     ++document_view_index) {
                    if (document->ViewAt(document_view_index) == view) {
                        break;
                    }
                }
                std::wstring label = base_name;
                if (document_view_index != 0U) {
                    label += UiText(UiStringId::Text0012);
                    label += std::to_wstring(document_view_index + 1U);
                    label += L"]";
                }
                label += dirty;
                TCITEMW item{};
                item.mask = TCIF_TEXT | TCIF_PARAM;
                item.pszText = label.data();
                item.lParam = static_cast<LPARAM>(view->id.Value());
                const int current_count = TabCtrl_GetItemCount(
                    group->document_tabs);
                const bool updated = tab_index < current_count
                    ? TabCtrl_SetItem(
                        group->document_tabs,
                        tab_index,
                        &item) != FALSE
                    : TabCtrl_InsertItem(
                        group->document_tabs,
                        tab_index,
                        &item) >= 0;
                if (!updated) {
                    return;
                }
                if (view->id == group->ActiveView()) {
                    selected_index = tab_index;
                }
                ++tab_index;
            }
            for (int index = TabCtrl_GetItemCount(group->document_tabs) - 1;
                 index >= tab_index;
                 --index) {
                TabCtrl_DeleteItem(group->document_tabs, index);
            }
            if (selected_index >= 0) {
                TabCtrl_SetCurSel(group->document_tabs, selected_index);
            }
        }
    } catch (const std::bad_alloc&) {
        return;
    }
}

void UpdateMainWindowStatus(
    ApplicationHost& state,
    const InkpodDocumentInfo& info,
    bool has_document) noexcept {
    std::array<wchar_t, 1024> title{};
    const inkpod::app::DocumentSession* document = has_document
        ? state.Documents().Find(state.routing.targets.DocumentSession())
        : nullptr;
    has_document = has_document && document != nullptr;
    const std::wstring active_name = document == nullptr
        ? UiText(UiStringId::NoDocument)
        : DocumentTabBaseName(state, *document, info, true);
    _snwprintf_s(
        title.data(),
        title.size(),
        _TRUNCATE,
        L"%ls%ls - inkpod",
        active_name.c_str(),
        has_document && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U ? L" *" : L"");
    SetWindowTextW(state.Workspace().windows.window, title.data());
    UpdateDocumentTabLabels(state, info, has_document);
    if (state.Workspace().windows.status_bar == nullptr) {
        return;
    }

    InkpodSnapshotTransform transform{};
    const bool has_transform = has_document && QuerySnapshotTransform(state, transform);
    const auto tool_name = [&state]() noexcept -> const wchar_t* {
        switch (state.Workspace().tools.active_tool) {
            case kInteractionBoxZoom: return UiText(UiStringId::ToolBoxZoom);
            case kInteractionGuideMove: return UiText(UiStringId::ToolGuideMove);
            case kInteractionSelection: return UiText(UiStringId::ToolSelection);
            case kInteractionColorReplace: return UiText(UiStringId::ToolColorReplacement);
            case kInteractionFill: return UiText(UiStringId::ToolFill);
            case kInteractionEyedropper: return UiText(UiStringId::ToolEyedropper);
            case kInteractionFloatingTransform: return UiText(UiStringId::ToolFloatingTransform);
            case kInteractionLightTableMove: return UiText(UiStringId::ToolLightTableMove);
            case kInteractionShootingFrame: return UiText(UiStringId::ToolShootingFrame);
            case kInteractionVanishingPoint: return UiText(UiStringId::ToolVanishingPoint);
            case kInteractionEffectGradient: return UiText(UiStringId::ToolGradient);
            case kInteractionEffectAirbrush: return UiText(UiStringId::ToolAirbrush);
            case kInteractionEffectBlur: return UiText(UiStringId::ToolBlur);
            case kInteractionEffectStamp: return UiText(UiStringId::ToolStamp);
            case kInteractionEffectDust: return UiText(UiStringId::ToolDustRemoval);
            case kInteractionEffectAlphaGradient: return UiText(UiStringId::ToolAlphaGradient);
            case INKPOD_TOOL_ERASER: return UiText(UiStringId::ToolEraser);
            case INKPOD_TOOL_BRUSH: return UiText(UiStringId::ToolBrush);
            default: return UiText(UiStringId::ToolPencil);
        }
    };
    std::array<wchar_t, 128U> tool_text{};
    _snwprintf_s(
        tool_text.data(),
        tool_text.size(),
        _TRUNCATE,
        L"%ls | %ls",
        tool_name(),
        UiText(state.Workspace().tools.active_plane == INKPOD_PLANE_COLOR
            ? UiStringId::Coloring
            : UiStringId::MainLine));
    std::array<wchar_t, 160U> zoom_text{};
    if (has_transform) {
        _snwprintf_s(
            zoom_text.data(),
            zoom_text.size(),
            _TRUNCATE,
            L"%ls%.1f%%",
            UiText(UiStringId::ZoomPrefix),
            transform.zoom * 100.0);
        const auto append_indicator = [&zoom_text](bool visible, UiStringId id) noexcept {
            if (visible) {
                wcsncat_s(zoom_text.data(), zoom_text.size(), L" | ", _TRUNCATE);
                wcsncat_s(zoom_text.data(), zoom_text.size(), UiText(id), _TRUNCATE);
            }
        };
        append_indicator(
            state.ActiveView().presentation.flip_horizontal,
            UiStringId::FlipHorizontal);
        append_indicator(
            state.ActiveView().presentation.flip_vertical,
            UiStringId::FlipVertical);
        append_indicator(
            state.ActiveView().presentation.grid_visible,
            UiStringId::Grid);
    } else {
        _snwprintf_s(
            zoom_text.data(), zoom_text.size(), _TRUNCATE,
            L"%ls--", UiText(UiStringId::ZoomPrefix));
    }
    std::array<wchar_t, 160U> document_text{};
    _snwprintf_s(
        document_text.data(),
        document_text.size(),
        _TRUNCATE,
        has_document ? L"%u×%u | %.1f dpi" : UiText(UiStringId::NoDocument),
        has_document ? info.width : 0U,
        has_document ? info.height : 0U,
        has_document ? static_cast<double>(info.dpi_x_milli) / 1000.0 : 0.0);
    std::array<wchar_t, 192U> state_text{};
    const bool has_progress = FormatTaskProgressStatus(state, state_text);
    if (has_progress) {
        ArmCommandTimer(
            state,
            state.Workspace().windows.window,
            CommandTimerKind::StatusProgress,
            kStatusProgressTimerMilliseconds);
    } else if (!state.shortcuts.pending_text.empty()) {
        DisarmCommandTimer(
            state, state.Workspace().windows.window, CommandTimerKind::StatusProgress);
        wcsncpy_s(
            state_text.data(), state_text.size(), state.shortcuts.pending_text.c_str(), _TRUNCATE);
    } else {
        DisarmCommandTimer(
            state, state.Workspace().windows.window, CommandTimerKind::StatusProgress);
        wcscpy_s(
            state_text.data(),
            state_text.size(),
            !has_document
                ? UiText(UiStringId::NoDocument)
                : UiText((info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U
                      ? UiStringId::Modified
                      : UiStringId::Saved));
    }
    if (!has_progress && has_document && state.engine != nullptr) {
        std::uint64_t edit_target_count{};
        InkpodEditTargetCapabilities capabilities{};
        capabilities.struct_size = sizeof(capabilities);
        if (state.engine->GetEditTargetPresentation(
                state.Document().id,
                state.Document().generation,
                edit_target_count,
                capabilities)
            && edit_target_count != 0U) {
            std::array<wchar_t, 48U> target_text{};
            _snwprintf_s(
                target_text.data(),
                target_text.size(),
                _TRUNCATE,
                L" | %ls%zu",
                UiText(UiStringId::EditTargetsPrefix),
                static_cast<std::size_t>(edit_target_count));
            wcsncat_s(
                state_text.data(),
                state_text.size(),
                target_text.data(),
                _TRUNCATE);
        }
    }
    StatusBarPresentation presentation{};
    presentation.parts[0] = tool_text.data();
    presentation.parts[1] = zoom_text.data();
    presentation.parts[4] = document_text.data();
    presentation.parts[5] = state_text.data();
    PresentStatusBar(state.Workspace().windows.status_bar, presentation);
    UpdateLocatorStatus(state);
}

void UpdateMenuState(ApplicationHost& state) noexcept {
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr) {
        return;
    }
    UpdateHistoryVisualizationMenu(state, menu);

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
    state.Workspace().command_states = ComputeCommandStates(inputs);
    const bool has_cut = state.Workspace().cut.handle != nullptr;
    const std::uint32_t cut_flags = state.Workspace().cut.flags;
    for (auto& command_state : state.Workspace().command_states) {
        switch (command_state.command) {
            case IDM_FILE_NEW_CUT:
                command_state.enabled = state.engine != nullptr;
                break;
            case IDM_CUT_PROPERTIES:
            case IDM_CUT_SAVE:
            case IDM_CUT_SEQUENCE_ADD:
                command_state.enabled = has_cut;
                break;
            case IDM_CUT_SEQUENCE_REMOVE:
            case IDM_CUT_SEQUENCE_MOVE_UP:
            case IDM_CUT_SEQUENCE_MOVE_DOWN:
            case IDM_CUT_SEQUENCE_RENUMBER:
                command_state.enabled = has_cut
                    && !state.Workspace().cut.members.empty();
                break;
            case IDM_CUT_UNDO:
                command_state.enabled = has_cut
                    && (cut_flags & INKPOD_CUT_FLAG_CAN_UNDO) != 0U;
                break;
            case IDM_CUT_REDO:
                command_state.enabled = has_cut
                    && (cut_flags & INKPOD_CUT_FLAG_CAN_REDO) != 0U;
                break;
            default:
                break;
        }
    }
    if (has_document && state.engine != nullptr) {
        std::uint64_t edit_target_count{};
        InkpodEditTargetCapabilities capabilities{};
        capabilities.struct_size = sizeof(capabilities);
        if (state.engine->GetEditTargetPresentation(
                state.Document().id,
                state.Document().generation,
                edit_target_count,
                capabilities)
            && edit_target_count != 0U) {
            for (auto& command_state : state.Workspace().command_states) {
                bool capable = true;
                switch (command_state.command) {
                    case IDM_LAYER_DUPLICATE:
                    case IDM_PLANE_DUPLICATE:
                        capable = capabilities.can_duplicate != 0U;
                        break;
                    case IDM_LAYER_DELETE:
                    case IDM_PLANE_DELETE:
                        capable = capabilities.can_delete != 0U;
                        break;
                    case IDM_LAYER_TOGGLE_VISIBLE:
                    case IDM_PLANE_TOGGLE_VISIBLE:
                        capable = capabilities.can_set_visibility != 0U;
                        break;
                    case IDM_LAYER_TOGGLE_EDITABLE:
                    case IDM_PLANE_TOGGLE_EDITABLE:
                        capable = capabilities.can_set_editability != 0U;
                        break;
                    case IDM_LAYER_MERGE:
                    case IDM_PLANE_MERGE:
                        capable = capabilities.can_merge != 0U;
                        break;
                    case IDM_LAYER_CONVERT:
                        capable = capabilities.can_convert_layers != 0U;
                        break;
                    case IDM_PLANE_CONVERT:
                        capable = capabilities.can_convert_planes != 0U;
                        break;
                    default:
                        break;
                }
                command_state.enabled = command_state.enabled && capable;
            }
        }
    }
    const CommandContext command_context = state.routing.targets.Capture();
    state.routing.command_state_context = command_context;
    for (auto& command_state : state.Workspace().command_states) {
        const CommandTargetScope required =
            TargetScopeForOwner(command_state.owner);
        if (state.routing.targets.Resolve(command_context, required)
            != CommandResolveStatus::Ok) {
            command_state.enabled = false;
        }
    }
    UpdateCommandLabels(menu, has_history, undo_label, redo_label);
    for (std::size_t index = 0U;
         index < kRecentDocumentCommands.size();
         ++index) {
        try {
            std::wstring label = L"&" + std::to_wstring(index + 1U) + L"  ";
            if (const auto* recent = state.RecentDocumentAt(index);
                recent != nullptr) {
                for (const wchar_t character : recent->path) {
                    label.push_back(character);
                    if (character == L'&') {
                        label.push_back(L'&');
                    }
                }
            } else {
                label += UiText(UiStringId::NoneParenthesized);
            }
            ModifyMenuW(
                menu,
                kRecentDocumentCommands[index],
                MF_BYCOMMAND | MF_STRING,
                kRecentDocumentCommands[index],
                label.c_str());
        } catch (const std::bad_alloc&) {
        }
    }
    ApplyCommandStates(state.Workspace().command_states, menu);
    UpdateToolPaletteDialog(state.Workspace().tools.palette, state.Workspace().command_states);
    UpdateLayerPaletteCommandState(
        state.Workspace().panes.layer_palette,
        state.Workspace().command_states);
    RefreshDockPaneViews(state);
    RefreshLocatorPane(state);
    ApplyShortcutLabelsToMenu(menu, state.shortcuts.bindings);
    UpdateMainWindowStatus(state, info, has_document);
    DrawMenuBar(state.Workspace().windows.window);
}

bool CommandSurfacesMatchComputedState(const ApplicationHost& state) noexcept {
    const HMENU menu = GetMenu(state.Workspace().windows.window);
    if (menu == nullptr) {
        return false;
    }
    for (const auto& command_state : state.Workspace().command_states) {
        const UINT menu_state = GetMenuState(menu, command_state.command, MF_BYCOMMAND);
        if (menu_state == static_cast<UINT>(-1)) {
            if (IsMenuCommand(command_state.command)) {
                return false;
            }
            continue;
        }
        if (((menu_state & (MF_DISABLED | MF_GRAYED)) == 0U)
                != command_state.enabled
            || ((menu_state & MF_CHECKED) != 0U) != command_state.checked) {
            return false;
        }
    }
    return ToolPaletteMatchesCommandState(
               state.Workspace().tools.palette,
               state.Workspace().command_states)
        && LayerPaletteMatchesCommandState(
               state.Workspace().panes.layer_palette,
               state.Workspace().command_states);
}

InkpodStatus ApplyView(
    ApplicationHost& state,
    InkpodViewCommandKind kind,
    double value1,
    double value2,
    double value3 = 0.0,
    double value4 = 0.0) noexcept {
    const InkpodViewInput input{
        sizeof(InkpodViewInput), kind, 0U, value1, value2, value3, value4};
    const std::uint64_t view_id = state.ActiveView().presentation.active_view_id;
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewController controller(*state.engine);
    return controller.Apply(view_id, input);
}

InkpodStatus ApplyView(
    ApplicationHost& state,
    const CommandContext& context,
    InkpodViewCommandKind kind,
    double value1,
    double value2,
    double value3 = 0.0,
    double value4 = 0.0) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.document_view.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const DocumentSession* document = state.Documents().Find(
        context.document_session.value());
    const auto* view = document == nullptr
        ? nullptr
        : document->FindView(context.document_view.value());
    if (view == nullptr || document->generation != context.generation.value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodViewInput input{
        sizeof(InkpodViewInput), kind, 0U, value1, value2, value3, value4};
    return state.engine->Invoke(
        document->id,
        document->generation,
        [core_view_id = view->core_view_id, input](InkpodCore* core) {
            InkpodDocumentInfo info{};
            info.struct_size = sizeof(info);
            InkpodStatus status = core_view_id == 0U
                ? inkpod_core_apply_view(core, &input, &info)
                : inkpod_core_view_apply(core, core_view_id, &input);
            if (status == INKPOD_STATUS_OK && core_view_id != 0U) {
                status = inkpod_core_get_document_info(core, &info);
            }
            return status;
        },
        true,
        true);
}

bool QuerySnapshotTransform(
    ApplicationHost& state, InkpodSnapshotTransform& transform) noexcept {
    transform = {};
    transform.struct_size = sizeof(transform);
    const DocumentSessionId session = state.routing.targets.DocumentSession();
    const DocumentSession* document = state.Documents().Find(session);
    if (state.engine == nullptr || document == nullptr) {
        return false;
    }
    return state.engine->GetSnapshotTransform(
        document->id,
        document->generation,
        state.ActiveView().presentation.active_view_id,
        transform);
}

InkpodStatus ApplyZoomPercent(ApplicationHost& state, std::uint32_t percent) noexcept {
    if (percent == 0U || percent > 6400U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodSnapshotTransform transform{};
    RECT client{};
    if (!QuerySnapshotTransform(state, transform)
        || GetClientRect(state.Workspace().windows.canvas, &client) == FALSE
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
    ApplicationHost& state,
    const InkpodStrokeSample& start,
    const InkpodStrokeSample& end) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto document_x = [&](double value) {
        double result = (value - bounds.left) / zoom;
        if (state.ActiveView().presentation.flip_horizontal) {
            result = static_cast<double>(info.width) - result;
        }
        return std::clamp(result, 0.0, static_cast<double>(info.width));
    };
    auto document_y = [&](double value) {
        double result = (value - bounds.top) / zoom;
        if (state.ActiveView().presentation.flip_vertical) {
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
    ApplicationHost& state, std::uint32_t axis, std::int32_t position) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t guide_id{};
    ViewController controller(*state.engine);
    return controller.AddGuide(axis, position, guide_id);
}

InkpodStatus DeleteAllGuides(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewController controller(*state.engine);
    return controller.DeleteAllGuides();
}

InkpodStatus SetGrid(ApplicationHost& state, const InkpodGridInput& input) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewController controller(*state.engine);
    return controller.SetGrid(input);
}

bool BeginGuideDrag(
    ApplicationHost& state, const InkpodStrokeSample& sample) noexcept {
    constexpr float ruler_extent = 22.0F;
    if (state.Workspace().tools.active_tool != kInteractionGuideMove && state.ActiveView().presentation.ruler_visible
        && sample.y >= 0.0F && sample.y <= ruler_extent
        && sample.x > ruler_extent) {
        state.ActiveView().presentation.guide_drag_active = true;
        state.ActiveView().presentation.guide_drag_axis = INKPOD_GUIDE_VERTICAL;
        state.ActiveView().presentation.guide_drag_id = 0U;
        return true;
    }
    if (state.Workspace().tools.active_tool != kInteractionGuideMove && state.ActiveView().presentation.ruler_visible
        && sample.x >= 0.0F && sample.x <= ruler_extent
        && sample.y > ruler_extent) {
        state.ActiveView().presentation.guide_drag_active = true;
        state.ActiveView().presentation.guide_drag_axis = INKPOD_GUIDE_HORIZONTAL;
        state.ActiveView().presentation.guide_drag_id = 0U;
        return true;
    }
    if (state.Workspace().tools.active_tool != kInteractionGuideMove || state.engine == nullptr) {
        return false;
    }
    std::uint64_t nearest_id{};
    std::uint32_t nearest_axis{};
    double nearest_distance = 7.0;
    const std::uint64_t view_id = state.ActiveView().presentation.active_view_id;
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
    state.ActiveView().presentation.guide_drag_active = true;
    state.ActiveView().presentation.guide_drag_axis = nearest_axis;
    state.ActiveView().presentation.guide_drag_id = nearest_id;
    return true;
}

InkpodStatus FinishGuideDrag(
    ApplicationHost& state, const InkpodStrokeSample& sample) noexcept {
    if (!state.ActiveView().presentation.guide_drag_active) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const std::uint32_t axis = state.ActiveView().presentation.guide_drag_axis;
    const std::uint64_t guide_id = state.ActiveView().presentation.guide_drag_id;
    state.ActiveView().presentation.guide_drag_active = false;
    state.ActiveView().presentation.guide_drag_axis = 0U;
    state.ActiveView().presentation.guide_drag_id = 0U;
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    RECT client{};
    if (!QueryDocument(state, info)
        || GetClientRect(state.Workspace().windows.canvas, &client) == FALSE
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)) {
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
    if ((axis == INKPOD_GUIDE_VERTICAL && state.ActiveView().presentation.flip_horizontal)
        || (axis == INKPOD_GUIDE_HORIZONTAL && state.ActiveView().presentation.flip_vertical)) {
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

InkpodStatus FitCanvas(ApplicationHost& state, InkpodViewCommandKind kind) noexcept {
    RECT client{};
    GetClientRect(state.Workspace().windows.canvas, &client);
    return ApplyView(
        state,
        kind,
        static_cast<double>(client.right - client.left),
        static_cast<double>(client.bottom - client.top));
}

InkpodStatus ApplyTreeEdit(
    ApplicationHost& state,
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

bool QueryTreeNode(ApplicationHost& state, bool plane, TreePaneNode& output) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    const std::uint32_t layer_index = state.Workspace().panes.active_tree_layer_index;
    const std::uint32_t plane_index = plane ? state.Workspace().panes.active_tree_plane_index : UINT32_MAX;
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
    ApplicationHost& state, InkpodTreeEdit edit, const std::string& name, std::uint64_t& object_id) noexcept {
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
    ApplicationHost& state, bool plane, UINT command) noexcept {
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
        dialog.title = plane ? UiText(UiStringId::Text0318) : UiText(UiStringId::Text0392);
        dialog.labels[0] = UiText(UiStringId::Text0434);
        dialog.values[0] = state.lifetime.smoke_test
            ? 75
            : static_cast<std::int32_t>(node.opacity_milli / 10U);
        if (ShowViewOptions(
                state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, dialog) != IDOK
            || dialog.values[0] < 0 || dialog.values[0] > 100) {
            return INKPOD_STATUS_CANCELLED;
        }
        edit.opacity_milli = static_cast<std::uint32_t>(dialog.values[0]) * 10U;
    }
    std::uint64_t ignored{};
    return ApplyTreeEditRecord(state, edit, node.name, ignored);
}

InkpodStatus EditSelectedTreeNodeProperties(ApplicationHost& state, bool plane) noexcept {
    TreePaneNode node{};
    if (!QueryTreeNode(state, plane, node)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    TextInputDialogState name_dialog{};
    name_dialog.title = plane ? UiText(UiStringId::Text0323) : UiText(UiStringId::Text0404);
    name_dialog.label = UiText(UiStringId::Text0567);
    try {
        name_dialog.value = Utf8UserText(node.name);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.lifetime.smoke_test) {
        name_dialog.value = plane ? L"Smoke Plane" : L"Smoke Layer";
    }
    if (ShowTextInput(
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, name_dialog) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState options{};
    options.title = name_dialog.title;
    options.labels = {UiText(UiStringId::Text0434), UiText(UiStringId::Text0880), UiText(UiStringId::Text0854), UiText(UiStringId::Text0829)};
    options.values = {
        static_cast<std::int32_t>(node.opacity_milli / 10U),
        (node.flags & INKPOD_NODE_VISIBLE) != 0U ? 1 : 0,
        (node.flags & INKPOD_NODE_EDITABLE) != 0U ? 1 : 0,
        static_cast<std::int32_t>(node.kind)};
    options.value_count = 4U;
    if (ShowViewOptions(
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, options) != IDOK
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

bool GestureDocumentPoints(
    ApplicationHost& state,
    const std::vector<InkpodStrokeSample>& samples,
    std::vector<inkpod::renderer::CanvasGeometryPoint>& points) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (samples.empty() || !QueryDocument(state, info) || info.width == 0U || info.height == 0U
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)) {
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
            if (state.ActiveView().presentation.flip_horizontal) {
                x = static_cast<double>(info.width) - x;
            }
            if (state.ActiveView().presentation.flip_vertical) {
                y = static_cast<double>(info.height) - y;
            }
            if (!std::isfinite(x) || !std::isfinite(y)) {
                return false;
            }
            points.push_back(inkpod::renderer::CanvasGeometryPoint{
                static_cast<float>(std::clamp(x, 0.0, static_cast<double>(info.width))),
                static_cast<float>(std::clamp(y, 0.0, static_cast<double>(info.height)))});
        }
        const auto& final_sample = samples.back();
        double final_x = (static_cast<double>(final_sample.x) - bounds.left) / zoom;
        double final_y = (static_cast<double>(final_sample.y) - bounds.top) / zoom;
        if (state.ActiveView().presentation.flip_horizontal) {
            final_x = static_cast<double>(info.width) - final_x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            final_y = static_cast<double>(info.height) - final_y;
        }
        const inkpod::renderer::CanvasGeometryPoint final_point{
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

void UpdateFillGeometryPreview(ApplicationHost& state) noexcept {
    inkpod::renderer::CanvasGeometryPreview preview{};
    preview.struct_size = sizeof(preview);
    const auto publish_preview = [&state, &preview] {
        inkpod::renderer::SetCanvasGeometryPreview(
            state.Workspace().windows.canvas, preview);
    };
    std::vector<inkpod::renderer::CanvasGeometryPoint> points;
    if (!GestureDocumentPoints(
            state, state.Workspace().tools.fill_gesture_samples, points)
        || points.size() < 2U) {
        publish_preview();
        return;
    }
    const float left = std::min(points.front().x, points.back().x);
    const float top = std::min(points.front().y, points.back().y);
    const float right = std::max(points.front().x, points.back().x);
    const float bottom = std::max(points.front().y, points.back().y);
    if (!(right > left) || !(bottom > top)) {
        publish_preview();
        return;
    }
    preview.active = 1U;
    preview.closed = 1U;
    preview.point_count = 4U;
    preview.points[0] = inkpod::renderer::CanvasGeometryPoint{left, top};
    preview.points[1] = inkpod::renderer::CanvasGeometryPoint{right, top};
    preview.points[2] = inkpod::renderer::CanvasGeometryPoint{right, bottom};
    preview.points[3] = inkpod::renderer::CanvasGeometryPoint{left, bottom};
    publish_preview();
}

void UpdateSelectionGeometryPreview(ApplicationHost& state) noexcept {
    inkpod::renderer::CanvasGeometryPreview preview{};
    preview.struct_size = sizeof(preview);
    const auto publish_preview = [&state, &preview] {
        inkpod::renderer::SetCanvasGeometryPreview(
            state.Workspace().windows.canvas, preview);
    };
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (editor == nullptr) {
        publish_preview();
        return;
    }
    const std::uint32_t shape = editor->selection.shape;
    std::vector<inkpod::renderer::CanvasGeometryPoint> points;
    if (!GestureDocumentPoints(
            state, state.Workspace().tools.selection_gesture_samples, points)
        || shape == INKPOD_SELECTION_WAND) {
        publish_preview();
        return;
    }
    preview.active = 1U;
    const auto append_point = [&preview](inkpod::renderer::CanvasGeometryPoint point) {
        if (preview.point_count < inkpod::renderer::kCanvasGeometryPreviewPoints) {
            preview.points[preview.point_count++] = point;
        }
    };
    if (shape == INKPOD_SELECTION_RECTANGLE || shape == INKPOD_SELECTION_ELLIPSE) {
        if (points.size() < 2U) {
            preview.active = 0U;
            publish_preview();
            return;
        }
        const bool from_center = (editor->selection.construction_flags
                & INKPOD_SELECTION_FROM_CENTER)
            != 0U;
        const float delta_x = points.back().x - points.front().x;
        const float delta_y = points.back().y - points.front().y;
        const float center_x = from_center
            ? points.front().x
            : points.front().x + delta_x / 2.0F;
        const float center_y = from_center
            ? points.front().y
            : points.front().y + delta_y / 2.0F;
        float radius_x = std::abs(delta_x) / (from_center ? 1.0F : 2.0F);
        float radius_y = std::abs(delta_y) / (from_center ? 1.0F : 2.0F);
        if (editor->selection.aspect_ratio_q16 != 0U) {
            const float aspect = static_cast<float>(editor->selection.aspect_ratio_q16)
                / 65536.0F;
            const float desired_x = radius_y * aspect;
            if (desired_x >= radius_x) {
                radius_x = desired_x;
            } else {
                radius_y = radius_x / aspect;
            }
        }
        if (!(radius_x > 0.0F) || !(radius_y > 0.0F)) {
            preview.active = 0U;
            publish_preview();
            return;
        }
        std::uint64_t turns = editor->selection.rotation_turns;
        if ((editor->selection.construction_flags
                & INKPOD_SELECTION_CONSTRAIN_ROTATION_45)
            != 0U) {
            constexpr std::uint64_t kTurn = UINT64_C(1) << 32U;
            constexpr std::uint64_t kStep = kTurn / 8U;
            turns = ((turns + kStep / 2U) / kStep * kStep) % kTurn;
        }
        constexpr double kTau = 6.28318530717958647692;
        const double rotation = kTau * static_cast<double>(turns)
            / static_cast<double>(UINT64_C(1) << 32U);
        const auto rotated_point = [&](double local_x, double local_y) {
            return inkpod::renderer::CanvasGeometryPoint{
                center_x + static_cast<float>(
                    local_x * std::cos(rotation) - local_y * std::sin(rotation)),
                center_y + static_cast<float>(
                    local_x * std::sin(rotation) + local_y * std::cos(rotation))};
        };
        if (shape == INKPOD_SELECTION_RECTANGLE) {
            append_point(rotated_point(-radius_x, -radius_y));
            append_point(rotated_point(radius_x, -radius_y));
            append_point(rotated_point(radius_x, radius_y));
            append_point(rotated_point(-radius_x, radius_y));
        } else {
            constexpr std::uint32_t kEllipsePreviewPoints = 48U;
            for (std::uint32_t index = 0U; index < kEllipsePreviewPoints; ++index) {
                const double radians = kTau * static_cast<double>(index)
                    / static_cast<double>(kEllipsePreviewPoints);
                append_point(rotated_point(
                    static_cast<double>(radius_x) * std::cos(radians),
                    static_cast<double>(radius_y) * std::sin(radians)));
            }
        }
        preview.closed = 1U;
    } else {
        for (const inkpod::renderer::CanvasGeometryPoint point : points) {
            append_point(point);
        }
        if (shape == INKPOD_SELECTION_LASSO
            || shape == INKPOD_SELECTION_POLYLINE) {
            if (preview.point_count < 2U) {
                preview.active = 0U;
                publish_preview();
                return;
            }
            preview.closed = 1U;
        } else if (shape == INKPOD_SELECTION_TRACE) {
            preview.stroke_width = std::clamp(
                static_cast<float>(
                    static_cast<double>(editor->selection.diameter_q16)
                    / 65536.0),
                0.001F,
                4096.0F);
            if ((editor->selection.construction_flags
                    & INKPOD_SELECTION_TRACE_PRESSURE_SIZE)
                != 0U) {
                preview.stroke_width *= std::clamp(
                    state.Workspace().tools.selection_gesture_samples.back().pressure,
                    0.0F,
                    1.0F);
            }
            if ((editor->selection.construction_flags
                    & INKPOD_SELECTION_TRACE_SCREEN_SIZE)
                != 0U) {
                InkpodDocumentInfo info{};
                inkpod::renderer::CanvasDocumentBounds bounds{};
                if (QueryDocument(state, info)
                    && inkpod::renderer::GetCanvasDocumentBounds(
                        state.Workspace().windows.canvas, bounds)
                    && info.width != 0U) {
                    const double zoom = (bounds.right - bounds.left)
                        / static_cast<double>(info.width);
                    if (std::isfinite(zoom) && zoom > 0.0) {
                        preview.stroke_width /= static_cast<float>(zoom);
                    }
                }
            }
        }
    }
    if (preview.point_count == 0U) {
        preview.active = 0U;
    }
    publish_preview();
}

void UpdateColorReplaceGeometryPreview(ApplicationHost& state) noexcept {
    inkpod::renderer::CanvasGeometryPreview preview{};
    preview.struct_size = sizeof(preview);
    const auto publish_preview = [&state, &preview] {
        inkpod::renderer::SetCanvasGeometryPreview(
            state.Workspace().windows.canvas, preview);
    };
    std::vector<inkpod::renderer::CanvasGeometryPoint> points;
    const auto& tools = state.Workspace().tools;
    if (!GestureDocumentPoints(
            state, tools.color_replace_gesture_samples, points)) {
        publish_preview();
        return;
    }
    preview.active = 1U;
    const auto append_point = [&preview](inkpod::renderer::CanvasGeometryPoint point) {
        if (preview.point_count < inkpod::renderer::kCanvasGeometryPreviewPoints) {
            preview.points[preview.point_count++] = point;
        }
    };
    if (tools.color_replace_shape == INKPOD_SELECTION_RECTANGLE) {
        if (points.size() < 2U) {
            preview.active = 0U;
            publish_preview();
            return;
        }
        const float left = std::min(points.front().x, points.back().x);
        const float top = std::min(points.front().y, points.back().y);
        const float right = std::max(points.front().x, points.back().x);
        const float bottom = std::max(points.front().y, points.back().y);
        append_point({left, top});
        append_point({right, top});
        append_point({right, bottom});
        append_point({left, bottom});
        preview.closed = 1U;
    } else {
        for (const inkpod::renderer::CanvasGeometryPoint point : points) {
            append_point(point);
        }
        preview.closed = tools.color_replace_shape == INKPOD_SELECTION_LASSO
                || tools.color_replace_shape == INKPOD_SELECTION_POLYLINE
            ? 1U
            : 0U;
        if (tools.color_replace_shape == INKPOD_SELECTION_TRACE) {
            preview.stroke_width = tools.color_replace_diameter;
        }
    }
    if (preview.point_count == 0U) {
        preview.active = 0U;
    }
    publish_preview();
}

InkpodGeometryPrimitive GeometryPrimitiveForTool(std::uint32_t tool) noexcept {
    if (tool == kInteractionGeometryCurve) return INKPOD_GEOMETRY_CURVE;
    if (tool == kInteractionGeometryRectangle) return INKPOD_GEOMETRY_RECTANGLE;
    if (tool == kInteractionGeometryEllipse) return INKPOD_GEOMETRY_ELLIPSE;
    if (tool == kInteractionGeometryPolygon) return INKPOD_GEOMETRY_POLYGON;
    if (tool == kInteractionGeometryPolyline) return INKPOD_GEOMETRY_POLYLINE;
    return INKPOD_GEOMETRY_LINE;
}

InkpodStatus ResolveRasterGeometryPoints(
    ApplicationHost& state,
    const std::vector<InkpodStrokeSample>& samples,
    std::vector<InkpodGeometryPoint>& points) noexcept {
    const auto& capture = state.Workspace().tools.procedure;
    if (state.engine == nullptr || !capture.valid || samples.empty()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        constexpr std::size_t kPointLimit =
            inkpod::renderer::kCanvasGeometryPreviewPoints;
        const std::size_t stride = std::max<std::size_t>(
            1U, (samples.size() + kPointLimit - 1U) / kPointLimit);
        std::vector<InkpodStrokeSample> bounded;
        bounded.reserve(std::min(samples.size() + 1U, kPointLimit));
        for (std::size_t index = 0U; index < samples.size(); index += stride) {
            bounded.push_back(samples[index]);
        }
        if (bounded.back().x != samples.back().x
            || bounded.back().y != samples.back().y) {
            if (bounded.size() == kPointLimit) {
                bounded.back() = samples.back();
            } else {
                bounded.push_back(samples.back());
            }
        }
        std::vector<InkpodGeometryPoint> resolved(
            bounded.size(),
            InkpodGeometryPoint{
                sizeof(InkpodGeometryPoint), 0U, 0.0F, 0.0F});
        auto& tools = state.Workspace().tools;
        const InkpodGeometryPointResolveInput input{
            sizeof(InkpodGeometryPointResolveInput),
            INKPOD_COORDINATE_SPACE_DEVICE,
            tools.geometry_snap_bypass
                ? INKPOD_GEOMETRY_RESOLVE_BYPASS_SNAP
                : INKPOD_GEOMETRY_RESOLVE_USE_VIEW_SNAP,
            capture.core_view_id,
            tools.geometry_view_revision,
            bounded.data(),
            bounded.size(),
            sizeof(InkpodStrokeSample)};
        InkpodGeometryPointResolveResult result{};
        result.struct_size = sizeof(result);
        const InkpodStatus status = state.engine->Invoke(
            [&input, &result, &resolved](InkpodCore* core) {
                return inkpod_core_geometry_points_resolve(
                    core,
                    &input,
                    &result,
                    resolved.data(),
                    resolved.size());
            },
            false,
            false);
        if (status == INKPOD_STATUS_OK) {
            tools.geometry_view_revision = result.view_revision;
            points.swap(resolved);
        }
        return status;
    } catch (const std::bad_alloc&) {
        points.clear();
        return INKPOD_STATUS_INVALID_STATE;
    }
}

InkpodStatus BuildRasterGeometryPoints(
    ApplicationHost& state,
    std::uint32_t tool,
    std::vector<InkpodGeometryPoint>& points) noexcept {
    std::vector<InkpodGeometryPoint> resolved;
    const InkpodStatus status = ResolveRasterGeometryPoints(
        state, state.Workspace().tools.geometry_gesture_samples, resolved);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (resolved.size() < 2U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    try {
        if (tool == kInteractionGeometryPolyline) {
            points.clear();
            points.reserve(resolved.size());
            for (const InkpodGeometryPoint point : resolved) {
                if (points.empty() || points.back().x != point.x
                    || points.back().y != point.y) {
                    points.push_back(point);
                }
            }
            return points.size() >= 2U
                ? INKPOD_STATUS_OK
                : INKPOD_STATUS_INVALID_ARGUMENT;
        }
        points = {resolved.front(), resolved.back()};
        if (tool == kInteractionGeometryCurve) {
            InkpodGeometryPoint control = resolved.size() >= 3U
                ? resolved[resolved.size() / 2U]
                : InkpodGeometryPoint{
                      sizeof(InkpodGeometryPoint),
                      0U,
                      (resolved.front().x + resolved.back().x) * 0.5F,
                      (resolved.front().y + resolved.back().y) * 0.5F};
            control.struct_size = sizeof(InkpodGeometryPoint);
            control.reserved = 0U;
            points.push_back(control);
        }
        return INKPOD_STATUS_OK;
    } catch (const std::bad_alloc&) {
        points.clear();
        return INKPOD_STATUS_INVALID_STATE;
    }
}

bool BuildRasterGeometryInput(
    ApplicationHost& state,
    const InkpodEditorStateInfo& editor,
    const std::vector<InkpodGeometryPoint>& points,
    InkpodGeometryInput& input) noexcept {
    if (points.size() < 2U || editor.active_plane_id == 0U
        || !IsGeometryCanvasTool(editor.active_tool)) {
        return false;
    }
    const InkpodGeometryPrimitive primitive =
        GeometryPrimitiveForTool(editor.active_tool);
    std::uint64_t flags = INKPOD_GEOMETRY_OUTLINE;
    std::uint32_t aspect_ratio_q16{};
    if ((GetKeyState(VK_SHIFT) & 0x8000) != 0) {
        if (primitive == INKPOD_GEOMETRY_RECTANGLE
            || primitive == INKPOD_GEOMETRY_ELLIPSE) {
            aspect_ratio_q16 = UINT32_C(1) << 16U;
        } else {
            flags |= INKPOD_GEOMETRY_CONSTRAIN_45_DEGREES;
        }
    }
    InkpodColorValue color = editor.current_color;
    color.struct_size = sizeof(InkpodColorValue);
    input = InkpodGeometryInput{
        sizeof(InkpodGeometryInput),
        primitive,
        flags,
        editor.active_plane_id,
        state.Workspace().tools.geometry_base_revision,
        color,
        color,
        std::clamp(
            static_cast<float>(
                static_cast<double>(editor.current_diameter_q16) / 65536.0),
            0.001F,
            4096.0F),
        aspect_ratio_q16,
        5U,
        0U,
        points.data(),
        points.size(),
        sizeof(InkpodGeometryPoint)};
    return true;
}

InkpodStatus UpdateRasterGeometryPreview(
    ApplicationHost& state,
    const InkpodEditorStateInfo& editor,
    const std::vector<InkpodGeometryPoint>& points) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodGeometryInput input{};
    if (!BuildRasterGeometryInput(state, editor, points, input)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodGeometryPreviewInfo info{};
    info.struct_size = sizeof(info);
    const bool updating = state.Workspace().tools.geometry_preview_active;
    const InkpodStatus status = state.engine->Invoke(
        [&input, &info, updating](InkpodCore* core) {
            return updating
                ? inkpod_core_geometry_preview_update(core, &input, &info)
                : inkpod_core_geometry_preview_begin(core, &input, &info);
        },
        true,
        false);
    if (status == INKPOD_STATUS_OK) {
        state.Workspace().tools.geometry_preview_active = true;
    }
    return status;
}

InkpodStatus HandleRasterGeometryCanvasEvent(
    ApplicationHost& state,
    const inkpod::renderer::CanvasStrokeEvent& input) noexcept {
    auto& tools = state.Workspace().tools;
    if (input.kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
        CancelCoreRasterGeometryPreview(state);
        return INKPOD_STATUS_OK;
    }
    if (input.kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
        CancelCoreRasterGeometryPreview(state);
        if (!BeginEditorProcedureCapture(state)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const InkpodEditorStateInfo* editor = CapturedEditorState(state);
        InkpodDocumentInfo document{};
        TreePaneNode plane{};
        if (editor == nullptr
            || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
            || !QueryDocument(state, document)
            || !QueryTreeNode(state, true, plane)
            || plane.id != editor->active_plane_id
            || !IsGeometryCanvasPlane(plane.kind)) {
            CancelCoreRasterGeometryPreview(state);
            return INKPOD_STATUS_INVALID_STATE;
        }
        tools.geometry_base_revision = document.document_revision;
        tools.geometry_view_revision = 0U;
        tools.geometry_snap_bypass =
            (GetKeyState(VK_CONTROL) & 0x8000) != 0;
    }
    const InkpodEditorStateInfo* editor = CapturedEditorState(state);
    if (editor == nullptr || !IsGeometryCanvasTool(editor->active_tool)) {
        CancelCoreRasterGeometryPreview(state);
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        if (input.sample_count != 0U) {
            if (tools.geometry_gesture_samples.size()
                > UINT64_C(1048576) - input.sample_count) {
                CancelCoreRasterGeometryPreview(state);
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            tools.geometry_gesture_samples.insert(
                tools.geometry_gesture_samples.end(),
                input.samples,
                input.samples + static_cast<std::size_t>(input.sample_count));
        }
    } catch (const std::bad_alloc&) {
        CancelCoreRasterGeometryPreview(state);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (tools.geometry_gesture_samples.size() < 2U) {
        if (input.kind == inkpod::renderer::CanvasStrokeEventKind::End) {
            CancelCoreRasterGeometryPreview(state);
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        return INKPOD_STATUS_OK;
    }
    std::vector<InkpodGeometryPoint> points;
    InkpodStatus status = BuildRasterGeometryPoints(
        state, editor->active_tool, points);
    if (status != INKPOD_STATUS_OK) {
        CancelCoreRasterGeometryPreview(state);
        return status;
    }
    status = UpdateRasterGeometryPreview(state, *editor, points);
    if (status != INKPOD_STATUS_OK) {
        CancelCoreRasterGeometryPreview(state);
        return status;
    }
    if (input.kind != inkpod::renderer::CanvasStrokeEventKind::End) {
        return INKPOD_STATUS_OK;
    }
    status = state.engine->Invoke(
        [](InkpodCore* core) {
            InkpodDispatchResult result{};
            result.struct_size = sizeof(result);
            return inkpod_core_geometry_preview_commit(core, &result);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK) {
        CancelRasterGeometryPreview(tools, state.Workspace().windows.canvas);
    } else {
        CancelCoreRasterGeometryPreview(state);
    }
    return status;
}

void BuildSelectionToolOptions(
    const ToolUiState& tools,
    inkpod::windows::ui::ViewOptionsDialogState& dialog) noexcept {
    using Choice = inkpod::windows::ui::ViewOptionsDialogState::Choice;
    static const std::array<Choice, 5U> ranges{{
        {UiText(UiStringId::Text0956), INKPOD_RANGE_NORMAL},
        {UiText(UiStringId::Text0682), INKPOD_RANGE_TIGHT},
        {UiText(UiStringId::Text1015), INKPOD_RANGE_ENCLOSED_INTERIOR},
        {UiText(UiStringId::Text0681), INKPOD_RANGE_DRAWING},
        {UiText(UiStringId::Text0601), INKPOD_RANGE_BOUNDARY}}};
    static const std::array<Choice, 4U> aspects{{
        {UiText(UiStringId::Text0864), 0},
        {L"1:1", 1 << 16},
        {L"4:3", (4 << 16) / 3},
        {L"16:9", (16 << 16) / 9}}};
    static const std::array<Choice, 4U> construction{{
        {UiText(UiStringId::Text0956), 0},
        {UiText(UiStringId::Text0441), 1},
        {UiText(UiStringId::Text0033), 2},
        {UiText(UiStringId::Text0438), 3}}};
    dialog = {};
    dialog.title = UiText(UiStringId::Text0976);
    dialog.labels = {
        UiText(UiStringId::Text0507),
        UiText(UiStringId::Text0125),
        UiText(UiStringId::Text0461),
        UiText(UiStringId::Text0580)};
    dialog.values = {
        static_cast<std::int32_t>(tools.selection_interpretation),
        static_cast<std::int32_t>(tools.selection_aspect_ratio_q16),
        static_cast<std::int32_t>(tools.selection_construction_flags & 3U),
        static_cast<std::int32_t>(
            (static_cast<std::uint64_t>(tools.selection_rotation_turns) * 360U
                + (UINT64_C(1) << 31U))
            >> 32U)};
    dialog.choices = {
        ranges.data(), aspects.data(), construction.data(), nullptr};
    dialog.choice_counts = {
        static_cast<std::uint32_t>(ranges.size()),
        static_cast<std::uint32_t>(aspects.size()),
        static_cast<std::uint32_t>(construction.size()),
        0U};
    dialog.value_count = 4U;
}

bool ApplySelectionToolOptions(
    ApplicationHost& state,
    const inkpod::windows::ui::ViewOptionsDialogState& dialog) noexcept {
    if (dialog.values[3] < 0 || dialog.values[3] > 359) {
        return false;
    }
    auto& tools = state.Workspace().tools;
    tools.selection_interpretation =
        static_cast<InkpodRangeInterpretation>(dialog.values[0]);
    tools.selection_aspect_ratio_q16 =
        static_cast<std::uint32_t>(dialog.values[1]);
    tools.selection_construction_flags =
        (tools.selection_construction_flags & ~UINT64_C(3))
        | static_cast<std::uint64_t>(dialog.values[2]);
    tools.selection_rotation_turns = static_cast<std::uint32_t>(
        (static_cast<std::uint64_t>(dialog.values[3]) << 32U) / 360U);
    if (SetEditorSelectionOptions(state) != INKPOD_STATUS_OK) {
        return false;
    }
    CancelSelectionGeometryPreview(tools, state.Workspace().windows.canvas);
    return true;
}

UINT ActiveToolOptionsCommand(const ApplicationHost& state) noexcept {
    const auto& tools = state.Workspace().tools;
    switch (tools.active_tool) {
        case INKPOD_TOOL_PENCIL: return IDM_TOOL_PENCIL;
        case INKPOD_TOOL_BRUSH: return IDM_TOOL_BRUSH;
        case INKPOD_TOOL_ERASER: return IDM_TOOL_ERASER;
        case kInteractionFill:
            return tools.fill_options.operation == INKPOD_FILL_CLOSED_REGION
                ? IDM_TOOL_CLOSED_FILL
                : (tools.fill_options.operation == INKPOD_FILL_EXTENSION
                          ? IDM_TOOL_FILL_EXTENSION
                          : IDM_TOOL_FILL);
        case kInteractionEyedropper: return IDM_TOOL_EYEDROPPER;
        case kInteractionEffectGradient: return IDM_EFFECT_GRADIENT;
        case kInteractionEffectAirbrush: return IDM_EFFECT_AIRBRUSH;
        case kInteractionEffectBlur: return IDM_EFFECT_BLUR;
        case kInteractionEffectStamp: return IDM_EFFECT_STAMP;
        case kInteractionEffectDust: return IDM_EFFECT_DUST;
        case kInteractionEffectAlphaGradient: return IDM_EFFECT_ALPHA_GRADIENT;
        case kInteractionSelection: return IDM_SELECTION_OPTIONS;
        default: return IDM_TOOL_BRUSH;
    }
}

bool QueryToolOptionsDetail(
    void* context,
    UINT command,
    inkpod::windows::ui::panes::ToolOptionsDetailModel& output) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) return false;
    using DetailKind =
        inkpod::windows::ui::panes::ToolOptionsDetailKind;
    try {
        output = {};
        switch (command) {
            case IDM_TOOL_FILL:
            case IDM_TOOL_CLOSED_FILL:
            case IDM_TOOL_FILL_EXTENSION:
            case IDM_TOOL_FILL_OPTIONS:
                output.kind = DetailKind::Fill;
                output.fill = state->Workspace().tools.fill_options;
                return true;
            case IDM_SELECTION_OPTIONS:
                output.kind = DetailKind::View;
                BuildSelectionToolOptions(
                    state->Workspace().tools, output.view);
                return true;
            case IDM_TOOL_EYEDROPPER: {
                using Choice =
                    inkpod::windows::ui::ViewOptionsDialogState::Choice;
                static const std::array<Choice, 4U> sources{{
                    {UiText(UiStringId::Text0728), INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT},
                    {UiText(UiStringId::Text0984), INKPOD_EYEDROPPER_SELECTED_PLANE},
                    {UiText(UiStringId::Text0562), INKPOD_EYEDROPPER_COMPOSITE},
                    {UiText(UiStringId::Text0374), INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST}}};
                output.kind = DetailKind::View;
                output.view.title = UiText(UiStringId::ToolEyedropper);
                output.view.labels = {
                    UiText(UiStringId::Text0213), nullptr, nullptr, nullptr};
                output.view.values[0] = static_cast<std::int32_t>(
                    state->Workspace().tools.eyedropper_source);
                output.view.choices[0] = sources.data();
                output.view.choice_counts[0] =
                    static_cast<std::uint32_t>(sources.size());
                output.view.value_count = 1U;
                return true;
            }
            case IDM_EFFECT_GRADIENT:
            case IDM_EFFECT_AIRBRUSH:
            case IDM_EFFECT_BOUNDARY_AIRBRUSH:
            case IDM_EFFECT_BLUR:
            case IDM_EFFECT_STAMP:
            case IDM_EFFECT_DUST:
            case IDM_EFFECT_ALPHA_GRADIENT: {
                std::uint32_t interaction{};
                output.kind = command == IDM_EFFECT_BOUNDARY_AIRBRUSH
                    ? DetailKind::BoundaryEffect
                    : DetailKind::Effect;
                if (!PrepareCanvasEffectEditor(
                        command, output.effect, interaction)) {
                    return false;
                }
                if (state->effects.options_command == command) {
                    ApplyCanvasEffectOptionsToEditor(
                        state->effects.options, output.effect);
                }
                return true;
            }
            default:
                output.kind = DetailKind::None;
                return true;
        }
    } catch (const std::bad_alloc&) {
        output = {};
        return false;
    }
}

bool ChangeToolOptionsDetail(
    void* context,
    UINT command,
    const inkpod::windows::ui::panes::ToolOptionsDetailModel& value,
    bool execute) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) return false;
    using DetailKind =
        inkpod::windows::ui::panes::ToolOptionsDetailKind;
    bool changed{};
    try {
        if (value.kind == DetailKind::Fill) {
            changed = SetEditorFillOptions(*state, value.fill)
                == INKPOD_STATUS_OK;
            if (changed) {
                state->Workspace().tools.fill_options = value.fill;
            }
        } else if (value.kind == DetailKind::View) {
            if (command == IDM_SELECTION_OPTIONS) {
                changed = ApplySelectionToolOptions(*state, value.view);
            } else if (command == IDM_TOOL_EYEDROPPER) {
                state->Workspace().tools.eyedropper_source =
                    static_cast<InkpodEyedropperSource>(value.view.values[0]);
                changed = true;
            } else {
                return false;
            }
        } else if (value.kind == DetailKind::Effect
            || value.kind == DetailKind::BoundaryEffect) {
            CanvasEffectOptions options{};
            changed = CanvasEffectOptionsFromEditor(
                command, value.effect, options);
            if (changed) {
                state->effects.options_command = command;
                state->effects.options = std::move(options);
                if (value.kind == DetailKind::Effect) {
                    std::uint32_t interaction{};
                    EffectEditorState defaults{};
                    changed = PrepareCanvasEffectEditor(
                                  command, defaults, interaction)
                        && SetEditorActiveTool(*state, interaction)
                            == INKPOD_STATUS_OK;
                } else if (execute) {
                    changed = QueueBoundaryAirbrush(
                                  *state,
                                  state->routing.targets.Capture(),
                                  state->effects.options)
                        == INKPOD_STATUS_OK;
                }
            }
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (changed) {
        UpdateMenuState(*state);
    }
    return changed;
}

InkpodStatus AdjustSelection(
    ApplicationHost& state, std::uint32_t operation, std::uint32_t pixels) noexcept {
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

InkpodStatus EditPaperFrames(ApplicationHost& state, UINT command) noexcept {
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
    input.shooting_frame = info.shooting_frame;
    input.maximum_close_frame = info.maximum_close_frame;
    input.margin_left = info.margin_left;
    input.margin_top = info.margin_top;
    input.margin_right = info.margin_right;
    input.margin_bottom = info.margin_bottom;

    ViewOptionsDialogState dialog{};
    dialog.value_count = 4U;
    InkpodFrameRect* frame{};
    if (command == IDM_CELL_FRAME_HUNDRED) {
        dialog.title = UiText(UiStringId::Text0026);
        frame = &input.hundred_frame;
    } else if (command == IDM_CELL_FRAME_REFERENCE) {
        dialog.title = UiText(UiStringId::Text0591);
        frame = &input.reference_frame;
    } else if (command == IDM_CELL_FRAME_DRAWING) {
        dialog.title = UiText(UiStringId::Text0463);
        frame = &input.drawing_frame;
    } else if (command == IDM_CELL_FRAME_SAFE) {
        dialog.title = UiText(UiStringId::Text0618);
        frame = &input.safe_frame;
    }
    if (frame != nullptr) {
        dialog.labels = {
            UiText(UiStringId::AxisX),
            UiText(UiStringId::AxisY),
            UiText(UiStringId::Text0644),
            UiText(UiStringId::Text1040)};
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
        dialog.title = UiText(UiStringId::Text0456);
        dialog.labels = {UiText(UiStringId::Text0638), UiText(UiStringId::Text0424), UiText(UiStringId::Text0555), UiText(UiStringId::Text0429)};
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
            state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, dialog) != IDOK) {
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

InkpodStatus CreateCellsFromOptions(
    ApplicationHost& state,
    const InkpodCellCreationOptions& options,
    std::optional<std::uint32_t> smoke_failure_index = std::nullopt,
    std::vector<ApplicationHost::DocumentBinding>* created = nullptr) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (smoke_failure_index.has_value() && !state.lifetime.smoke_test) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (created != nullptr) {
        created->clear();
    }
    InkpodCellCreationPlan* plan{};
    InkpodStatus status = inkpod_cell_creation_plan_create(&options, &plan);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    const auto release_plan = [&plan]() noexcept {
        (void)inkpod_cell_creation_plan_release(&plan);
    };
    std::uint32_t count{};
    status = inkpod_cell_creation_plan_count(plan, &count);
    InkpodDocumentInfo current_info{};
    const bool reuse_empty_bootstrap_session = count == 1U
        && state.routing.targets.DocumentSession()
        && !QueryDocument(state, current_info);
    const std::uint32_t added_count = count
        - (reuse_empty_bootstrap_session ? 1U : 0U);
    if (status != INKPOD_STATUS_OK || count == 0U
        || count > INKPOD_MAX_CELL_CREATION_COUNT
        || added_count + state.Documents().Count()
            > INKPOD_MAX_CELL_CREATION_COUNT) {
        release_plan();
        return status == INKPOD_STATUS_OK ? INKPOD_STATUS_INVALID_STATE : status;
    }
    if (smoke_failure_index.has_value()
        && smoke_failure_index.value() >= count) {
        release_plan();
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (created != nullptr) {
        try {
            created->reserve(count);
        } catch (const std::bad_alloc&) {
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    struct PendingIdentity final {
        std::uint64_t high{};
        std::uint64_t low{};
        std::wstring recovery_path;
    };
    std::array<PendingIdentity, INKPOD_MAX_CELL_CREATION_COUNT> identities{};
    for (std::uint32_t index = 0U; index < count; ++index) {
        GUID uuid{};
        if (FAILED(CoCreateGuid(&uuid))) {
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        static_assert(sizeof(uuid) == sizeof(std::uint64_t) * 2U);
        std::memcpy(&identities[index].high, &uuid, sizeof(std::uint64_t));
        std::memcpy(
            &identities[index].low,
            reinterpret_cast<const std::uint8_t*>(&uuid) + sizeof(std::uint64_t),
            sizeof(std::uint64_t));
        if (!PrivateRecoveryPath(
                identities[index].high,
                identities[index].low,
                identities[index].recovery_path)) {
            release_plan();
            return INKPOD_STATUS_IO_ERROR;
        }
    }

    if (reuse_empty_bootstrap_session) {
        if (smoke_failure_index.has_value()) {
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        const std::uint64_t uuid_high = identities[0].high;
        const std::uint64_t uuid_low = identities[0].low;
        status = state.engine->Invoke(
            [plan, uuid_high, uuid_low](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell_from_plan(
                    core, plan, 0U, uuid_high, uuid_low, &info);
            },
            false,
            true);
        const DocumentIdentity identity = UntitledDocumentIdentity(
            identities[0].high, identities[0].low);
        if (status != INKPOD_STATUS_OK
            || !state.Documents().AssignIdentity(state.Document().id, identity)) {
            release_plan();
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
        state.Document().untitled_number = state.IssueUntitledNumber();
        state.Document().shell.current_path.clear();
        state.Document().shell.source_path.clear();
        state.Document().shell.recovery_path =
            std::move(identities[0].recovery_path);
        state.Document().shell.recovery_original_path.clear();
        state.ActiveView().presentation.color_check_mode =
            INKPOD_COLOR_CHECK_OFF;
        if (!state.RefreshEditorPresentation(
                state.Document().id, state.Document().generation)) {
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        ResetUiForNewActiveDocument(state);
        status = FitCanvas(state, INKPOD_VIEW_FIT);
        if (status == INKPOD_STATUS_OK && created != nullptr) {
            created->push_back(ApplicationHost::DocumentBinding{
                state.Document().id,
                state.ActiveView().id,
                state.Document().generation});
        }
        release_plan();
        UpdateMenuState(state);
        return status;
    }

    const DocumentViewId previous_view = state.routing.targets.ActiveDocumentView();
    const EditorGroupId destination_group = state.routing.targets.EditorGroup();
    std::array<ApplicationHost::DocumentBinding, INKPOD_MAX_CELL_CREATION_COUNT> staged{};
    std::size_t staged_count{};
    std::size_t published_count{};
    const auto rollback = [
                              &state,
                              &staged,
                              &staged_count,
                              &published_count,
                              previous_view,
                              created]() noexcept {
        while (staged_count > published_count) {
            --staged_count;
            (void)state.DiscardPreparedDocumentSession(staged[staged_count]);
        }
        while (published_count != 0U) {
            --published_count;
            (void)state.CloseDocumentSession(staged[published_count].session);
        }
        staged_count = 0U;
        if (created != nullptr) {
            created->clear();
        }
        if (previous_view) {
            (void)state.ActivateDocumentView(previous_view);
        }
    };

    for (std::uint32_t index = 0U; index < count; ++index) {
        if (smoke_failure_index.has_value()
            && smoke_failure_index.value() == index) {
            rollback();
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        const auto binding = state.PrepareDocumentSession();
        if (!binding.has_value()) {
            rollback();
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        staged[staged_count++] = binding.value();
        const std::uint64_t uuid_high = identities[index].high;
        const std::uint64_t uuid_low = identities[index].low;
        status = state.engine->Invoke(
            binding->session,
            binding->generation,
            [plan, index, uuid_high, uuid_low](InkpodCore* core) {
                InkpodDocumentInfo info = EmptyDocumentInfo();
                return inkpod_core_new_cell_from_plan(
                    core,
                    plan,
                    index,
                    uuid_high,
                    uuid_low,
                    &info);
            },
            false,
            true);
        if (status != INKPOD_STATUS_OK) {
            rollback();
            release_plan();
            return status;
        }
    }

    for (std::uint32_t index = 0U; index < count; ++index) {
        const auto& binding = staged[index];
        if (!state.PublishPreparedDocumentSession(binding, destination_group)) {
            rollback();
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        ++published_count;
        DocumentSession* document = state.Documents().Find(binding.session);
        DocumentView* view = document == nullptr
            ? nullptr
            : document->FindView(binding.view);
        const DocumentIdentity document_identity = UntitledDocumentIdentity(
            identities[index].high, identities[index].low);
        if (document == nullptr || view == nullptr
            || !state.Documents().AssignIdentity(
                binding.session, document_identity)) {
            rollback();
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        document->untitled_number = state.IssueUntitledNumber();
        document->shell.current_path.clear();
        document->shell.source_path.clear();
        document->shell.recovery_path =
            std::move(identities[index].recovery_path);
        document->shell.recovery_original_path.clear();
        view->presentation.color_check_mode = INKPOD_COLOR_CHECK_OFF;
        if (!state.RefreshEditorPresentation(
                binding.session, binding.generation)) {
            rollback();
            release_plan();
            return INKPOD_STATUS_INVALID_STATE;
        }
        if (created != nullptr) {
            created->push_back(binding);
        }
    }
    if (!state.ActivateDocumentView(staged[staged_count - 1U].view)) {
        rollback();
        release_plan();
        return INKPOD_STATUS_INVALID_STATE;
    }
    ResetUiForNewActiveDocument(state);
    status = FitCanvas(state, INKPOD_VIEW_FIT);
    if (status != INKPOD_STATUS_OK) {
        rollback();
    }
    release_plan();
    UpdateMenuState(state);
    return status;
}

struct CutSnapshot final {
    InkpodCutInfo info{};
    std::array<std::uint8_t, 4096U> work_title{};
    std::array<std::uint8_t, 4096U> episode{};
    std::array<std::uint8_t, 4096U> scene{};
    std::array<std::uint8_t, 4096U> cut_name{};
    std::array<std::uint8_t, 4096U> instruction{};
    std::array<std::array<std::uint8_t, 256U>, INKPOD_MAX_CELL_CREATION_COUNT>
        member_paths{};
    std::array<std::uint64_t, INKPOD_MAX_CELL_CREATION_COUNT> member_path_bytes{};
    std::array<InkpodCutMemberInfo, INKPOD_MAX_CELL_CREATION_COUNT> member_infos{};
};

struct CutCoreTarget final {
    DocumentSessionId session{};
    Generation generation{};
    InkpodCut* cut{};
};

std::optional<CutCoreTarget> CaptureCutCoreTarget(
    ApplicationHost& state, InkpodCut* requested_cut = nullptr) noexcept {
    const CommandContext context = state.routing.targets.Capture();
    InkpodCut* const cut = requested_cut != nullptr
        ? requested_cut
        : state.Workspace().cut.handle;
    if (state.engine == nullptr || cut == nullptr
        || !context.workspace.has_value()
        || context.workspace.value() != state.Workspace().id
        || !context.document_session.has_value()
        || !context.generation.has_value()
        || state.routing.targets.Resolve(
               context, inkpod::app::kDocumentSessionCommandScope)
            != CommandResolveStatus::Ok) {
        return std::nullopt;
    }
    const DocumentSession* document = state.Documents().Find(
        context.document_session.value());
    if (document == nullptr
        || document->generation != context.generation.value()) {
        return std::nullopt;
    }
    return CutCoreTarget{document->id, document->generation, cut};
}

bool Utf8ToWideText(
    const std::uint8_t* bytes,
    std::size_t byte_count,
    std::wstring& output) noexcept {
    if (byte_count == 0U) {
        output.clear();
        return true;
    }
    if (bytes == nullptr || byte_count > static_cast<std::size_t>(INT_MAX)) {
        return false;
    }
    const int required = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        reinterpret_cast<const char*>(bytes),
        static_cast<int>(byte_count),
        nullptr,
        0);
    if (required <= 0) {
        return false;
    }
    try {
        output.resize(static_cast<std::size_t>(required));
    } catch (const std::bad_alloc&) {
        return false;
    }
    return MultiByteToWideChar(
               CP_UTF8,
               MB_ERR_INVALID_CHARS,
               reinterpret_cast<const char*>(bytes),
               static_cast<int>(byte_count),
               output.data(),
               required)
        == required;
}

bool QueryCutSnapshot(
    ApplicationHost& state,
    const CutCoreTarget& target,
    CutSnapshot& snapshot) noexcept {
    snapshot.info = {};
    snapshot.info.struct_size = sizeof(snapshot.info);
    return state.engine->Invoke(
               target.session,
               target.generation,
               [cut = target.cut, &snapshot](InkpodCore*) {
                   InkpodStatus status = inkpod_cut_info(cut, &snapshot.info);
                   if (status != INKPOD_STATUS_OK
                       || snapshot.info.member_count
                           > INKPOD_MAX_CELL_CREATION_COUNT) {
                       return status == INKPOD_STATUS_OK
                           ? INKPOD_STATUS_INVALID_STATE
                           : status;
                   }
                   InkpodCutMetadataBuffer metadata{};
                   metadata.struct_size = sizeof(metadata);
                   metadata.work_title = {
                       snapshot.work_title.data(), snapshot.work_title.size(), 0U};
                   metadata.episode = {
                       snapshot.episode.data(), snapshot.episode.size(), 0U};
                   metadata.scene = {
                       snapshot.scene.data(), snapshot.scene.size(), 0U};
                   metadata.cut_name = {
                       snapshot.cut_name.data(), snapshot.cut_name.size(), 0U};
                   metadata.instruction = {
                       snapshot.instruction.data(), snapshot.instruction.size(), 0U};
                   status = inkpod_cut_metadata_copy(cut, &metadata);
                   if (status != INKPOD_STATUS_OK) {
                       return status;
                   }
                   for (std::uint32_t index = 0U;
                        index < snapshot.info.member_count;
                        ++index) {
                       InkpodCutMemberInfo member{};
                       member.struct_size = sizeof(member);
                       member.relative_path = {
                           snapshot.member_paths[index].data(),
                           snapshot.member_paths[index].size(),
                           0U};
                       status = inkpod_cut_member_get(cut, index, &member);
                       if (status != INKPOD_STATUS_OK) {
                           return status;
                       }
                       snapshot.member_path_bytes[index] =
                           member.relative_path.byte_count;
                       snapshot.member_infos[index] = member;
                   }
                   return INKPOD_STATUS_OK;
               },
               false,
               false)
        == INKPOD_STATUS_OK;
}

bool QueryCutSnapshot(
    ApplicationHost& state,
    CutSnapshot& snapshot,
    InkpodCut* requested_cut = nullptr) noexcept {
    const auto target = CaptureCutCoreTarget(state, requested_cut);
    return target.has_value()
        && QueryCutSnapshot(state, target.value(), snapshot);
}

bool BuildCutSessionCache(
    const CutSnapshot& snapshot, CutSession& destination) noexcept {
    std::wstring cut_name;
    std::vector<CutMemberCache> members;
    if (!Utf8ToWideText(
            snapshot.cut_name.data(),
            static_cast<std::size_t>(snapshot.info.cut_name_bytes),
            cut_name)) {
        return false;
    }
    try {
        members.reserve(snapshot.info.member_count);
        for (std::uint32_t index = 0U;
             index < snapshot.info.member_count;
             ++index) {
            std::wstring path;
            if (!Utf8ToWideText(
                    snapshot.member_paths[index].data(),
                    static_cast<std::size_t>(snapshot.member_path_bytes[index]),
                    path)) {
                return false;
            }
            const auto& info = snapshot.member_infos[index];
            CutMemberCache member{};
            member.display_number = info.display_number;
            member.cell_id = info.cell_id;
            member.document_uuid_high = info.document_uuid_high;
            member.document_uuid_low = info.document_uuid_low;
            member.relative_path = std::move(path);
            const auto cached = std::find_if(
                destination.members.cbegin(),
                destination.members.cend(),
                [&member](const CutMemberCache& candidate) {
                    return candidate.cell_id == member.cell_id
                        && candidate.document_uuid_high
                            == member.document_uuid_high
                        && candidate.document_uuid_low
                            == member.document_uuid_low;
                });
            if (cached != destination.members.cend()) {
                member.width = cached->width;
                member.height = cached->height;
                member.thumbnail_width = cached->thumbnail_width;
                member.thumbnail_height = cached->thumbnail_height;
                member.thumbnail_stride_bytes = cached->thumbnail_stride_bytes;
                member.thumbnail_checksum = cached->thumbnail_checksum;
                member.thumbnail_rgba = cached->thumbnail_rgba;
            }
            members.push_back(std::move(member));
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    destination.cut_name = std::move(cut_name);
    destination.flags = snapshot.info.flags;
    destination.members = std::move(members);
    return true;
}

bool QueryShootingFrame(
    ApplicationHost& state,
    bool& present,
    InkpodShootingFrameInfo& frame) noexcept {
    present = false;
    frame = {};
    frame.struct_size = sizeof(frame);
    const DocumentSessionId session = state.routing.targets.DocumentSession();
    DocumentSession* document = state.Documents().Find(session);
    std::uint32_t raw_present{};
    if (state.engine == nullptr || document == nullptr) {
        return false;
    }
    const InkpodStatus status = state.engine->Invoke(
        document->id,
        document->generation,
        [&raw_present, &frame](InkpodCore* core) {
            return inkpod_core_shooting_frame_get(core, &raw_present, &frame);
        },
        false,
        false);
    present = status == INKPOD_STATUS_OK && raw_present != 0U;
    return status == INKPOD_STATUS_OK;
}

bool RefreshCutSessionCache(
    ApplicationHost& state, const CutSnapshot* existing = nullptr) noexcept {
    CutSnapshot queried{};
    const CutSnapshot* snapshot = existing;
    if (snapshot == nullptr) {
        if (!QueryCutSnapshot(state, queried)) {
            return false;
        }
        snapshot = &queried;
    }
    return BuildCutSessionCache(*snapshot, state.Workspace().cut);
}

bool CutSnapshotToDialog(
    const CutSnapshot& snapshot,
    CutPropertiesDialogState& properties,
    InkpodCellCreationOptions& defaults) noexcept {
    if (!Utf8ToWideText(
            snapshot.work_title.data(), snapshot.info.work_title_bytes,
            properties.work_title)
        || !Utf8ToWideText(
            snapshot.episode.data(), snapshot.info.episode_bytes,
            properties.episode)
        || !Utf8ToWideText(
            snapshot.scene.data(), snapshot.info.scene_bytes,
            properties.scene)
        || !Utf8ToWideText(
            snapshot.cut_name.data(), snapshot.info.cut_name_bytes,
            properties.cut_name)
        || !Utf8ToWideText(
            snapshot.instruction.data(), snapshot.info.instruction_bytes,
            properties.instruction)) {
        return false;
    }
    properties.duration_frames = snapshot.info.duration_frames;
    defaults = InkpodCellCreationOptions{
        sizeof(InkpodCellCreationOptions),
        snapshot.info.sizing_mode,
        INKPOD_FEATURE_NONE,
        snapshot.info.width,
        snapshot.info.height,
        snapshot.info.dpi_x_milli,
        snapshot.info.dpi_y_milli,
        snapshot.info.margin_milli,
        snapshot.info.safe_frame_ratio_milli,
        snapshot.info.maximum_close_ratio_milli,
        snapshot.info.anchor,
        snapshot.info.initial_layer_kind,
        snapshot.info.pixel_format,
        1U,
        0U};
    return true;
}

struct EncodedCutMetadata final {
    std::array<std::vector<std::uint8_t>, 5U> text;
    InkpodCutMetadataInput input{};
};

bool EncodeCutMetadata(
    const CutPropertiesDialogState& source,
    EncodedCutMetadata& encoded) noexcept {
    const std::array<const std::wstring*, 5U> values{
        &source.work_title,
        &source.episode,
        &source.scene,
        &source.cut_name,
        &source.instruction};
    for (std::size_t index = 0U; index < values.size(); ++index) {
        if (values[index]->empty()) {
            encoded.text[index].clear();
        } else if (!WidePathToUtf8(*values[index], encoded.text[index])) {
            return false;
        }
        if (encoded.text[index].size() > 4096U) {
            return false;
        }
    }
    const auto span = [&encoded](std::size_t index) noexcept {
        return InkpodUtf8Span{
            encoded.text[index].empty() ? nullptr : encoded.text[index].data(),
            encoded.text[index].size()};
    };
    encoded.input = InkpodCutMetadataInput{
        sizeof(InkpodCutMetadataInput),
        source.duration_frames,
        span(0U),
        span(1U),
        span(2U),
        span(3U),
        span(4U)};
    return true;
}

InkpodCutDefaultsInput CutDefaultsInput(
    const InkpodCellCreationOptions& source) noexcept {
    return InkpodCutDefaultsInput{
        sizeof(InkpodCutDefaultsInput),
        source.sizing_mode,
        INKPOD_FEATURE_NONE,
        source.width,
        source.height,
        source.dpi_x_milli,
        source.dpi_y_milli,
        source.margin_milli,
        source.safe_frame_ratio_milli,
        source.maximum_close_ratio_milli,
        source.anchor,
        source.initial_layer_kind,
        source.pixel_format,
        0U};
}

InkpodStatus SaveWorkspaceCut(ApplicationHost& state) noexcept {
    auto& cut = state.Workspace().cut;
    if (state.engine == nullptr || cut.handle == nullptr
        || cut.current_path.empty()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<std::uint8_t> path;
    if (!WidePathToUtf8(cut.current_path, path)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodCutInfo info{};
    info.struct_size = sizeof(info);
    InkpodCut* const handle = cut.handle;
    return state.engine->Invoke(
        [handle, &path, &info](InkpodCore*) {
            return inkpod_cut_save(
                handle, path.data(), path.size(), &info);
        },
        false,
        false);
}

bool PrepareWorkspaceCutReplacement(ApplicationHost& state) noexcept {
    if (state.Workspace().cut.handle == nullptr) {
        return true;
    }
    CutSnapshot snapshot{};
    if (!QueryCutSnapshot(state, snapshot)) {
        return false;
    }
    if ((snapshot.info.flags & INKPOD_CUT_FLAG_DIRTY) != 0U) {
        const int choice = state.lifetime.smoke_test
            ? IDNO
            : MessageBoxW(
                  state.Workspace().windows.window,
                  UiText(UiStringId::Text0783),
                  UiText(UiStringId::Text0151),
                  MB_YESNOCANCEL | MB_ICONQUESTION);
        if (choice == IDCANCEL
            || (choice == IDYES
                && SaveWorkspaceCut(state) != INKPOD_STATUS_OK)) {
            return false;
        }
    }
    return true;
}

void ReleaseCutHandle(ApplicationHost& state, InkpodCut*& cut) noexcept {
    if (state.engine == nullptr || cut == nullptr) {
        return;
    }
    (void)state.engine->Invoke(
        [&cut](InkpodCore*) { return inkpod_cut_destroy(&cut); },
        false,
        false);
}

InkpodStatus InstallCutSession(
    ApplicationHost& state,
    InkpodCut*& cut,
    const std::wstring& path,
    bool replacement_prepared) noexcept {
    CutSnapshot snapshot{};
    CutSession candidate{};
    candidate.handle = cut;
    if (!QueryCutSnapshot(state, snapshot, cut)
        || !BuildCutSessionCache(snapshot, candidate)) {
        ReleaseCutHandle(state, cut);
        return INKPOD_STATUS_INVALID_STATE;
    }
    try {
        candidate.current_path = path;
    } catch (const std::bad_alloc&) {
        ReleaseCutHandle(state, cut);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!replacement_prepared && !PrepareWorkspaceCutReplacement(state)) {
        ReleaseCutHandle(state, cut);
        return INKPOD_STATUS_CANCELLED;
    }
    if (!state.DestroyCutSession(state.Workspace())) {
        ReleaseCutHandle(state, cut);
        return INKPOD_STATUS_INVALID_STATE;
    }
    cut = nullptr;
    state.Workspace().cut = std::move(candidate);
    RefreshSequencePane(state);
    UpdateMenuState(state);
    return INKPOD_STATUS_OK;
}

bool IsCutDescriptor(const std::wstring& path) noexcept {
    HANDLE file = CreateFileW(
        path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        return false;
    }
    std::array<std::uint8_t, 8U> magic{};
    DWORD read{};
    const bool ok = ReadFile(
        file, magic.data(), static_cast<DWORD>(magic.size()), &read, nullptr)
        != FALSE;
    CloseHandle(file);
    constexpr std::array<std::uint8_t, 8U> expected{
        'I', 'N', 'K', 'C', 'U', 'T', 0U, 0U};
    return ok && read == magic.size() && magic == expected;
}

InkpodStatus OpenCutDescriptor(
    ApplicationHost& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<std::uint8_t> encoded_path;
    if (!WidePathToUtf8(path, encoded_path)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodCut* opened{};
    InkpodStatus status = state.engine->Invoke(
        [&encoded_path, &opened](InkpodCore*) {
            return inkpod_cut_open(
                encoded_path.data(), encoded_path.size(), &opened);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    return InstallCutSession(state, opened, path, false);
}

bool CutCellPaths(
    const std::wstring& descriptor,
    std::uint32_t count,
    std::vector<std::wstring>& full_paths,
    std::vector<std::wstring>& relative_paths) noexcept {
    const std::size_t slash = descriptor.find_last_of(L"\\/");
    const std::wstring directory = slash == std::wstring::npos
        ? std::wstring{}
        : descriptor.substr(0U, slash + 1U);
    std::wstring stem = slash == std::wstring::npos
        ? descriptor
        : descriptor.substr(slash + 1U);
    const std::size_t dot = stem.find_last_of(L'.');
    if (dot != std::wstring::npos) {
        stem.erase(dot);
    }
    if (stem.empty()) {
        return false;
    }
    try {
        full_paths.reserve(count);
        relative_paths.reserve(count);
        for (std::uint32_t index = 0U; index < count; ++index) {
            std::array<wchar_t, 32U> suffix{};
            _snwprintf_s(
                suffix.data(), suffix.size(), _TRUNCATE,
                L"-%04u.inkpod", index + 1U);
            std::wstring relative = stem + suffix.data();
            full_paths.push_back(directory + relative);
            relative_paths.push_back(std::move(relative));
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    return true;
}

InkpodStatus CreateNewCut(ApplicationHost& state, HWND owner) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodEditorDefaults editor_defaults{};
    const InkpodStatus defaults_status = state.engine->GetEditorDefaults(
        state.Document().id, state.Document().generation, editor_defaults);
    if (defaults_status != INKPOD_STATUS_OK) {
        return defaults_status;
    }
    CutPropertiesDialogState properties{};
    properties.work_title = L"inkpod";
    properties.episode = L"01";
    properties.scene = L"A";
    properties.cut_name = state.lifetime.smoke_test ? L"SmokeCut" : L"C001";
    properties.duration_frames = 24U;
    if (ShowCutProperties(
            state.lifetime.instance, owner, state.lifetime.smoke_test, properties)
        != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    CellCreationDialogState cells{};
    cells.options = InkpodCellCreationOptions{
        sizeof(InkpodCellCreationOptions),
        INKPOD_CELL_SIZING_IMAGE_PIXELS,
        INKPOD_FEATURE_NONE,
        editor_defaults.width,
        editor_defaults.height,
        editor_defaults.dpi_x_milli,
        editor_defaults.dpi_y_milli,
        50U,
        900U,
        500U,
        INKPOD_FRAME_ANCHOR_CENTER,
        INKPOD_LAYER_BINARY_COLORING,
        INKPOD_STORAGE_RGBA8,
        state.lifetime.smoke_test ? 5U : 1U,
        0U};
    cells.layer_choices = LayerKindChoices().data();
    cells.layer_choice_count =
        static_cast<std::uint32_t>(LayerKindChoices().size());
    cells.build_preview = BuildCellCreationDialogPreview;
    if (ShowCellCreationOptions(
            state.lifetime.instance, owner, state.lifetime.smoke_test, cells)
        != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    std::wstring descriptor = state.lifetime.smoke_test
        ? L"inkpod-cut-smoke.inkpod"
        : std::wstring{};
    if (!state.lifetime.smoke_test
        && !ChooseInkpodPath(owner, true, descriptor)) {
        return INKPOD_STATUS_CANCELLED;
    }
    if (!PrepareWorkspaceCutReplacement(state)) {
        return INKPOD_STATUS_CANCELLED;
    }
    std::vector<std::wstring> full_paths;
    std::vector<std::wstring> relative_paths;
    if (!CutCellPaths(
            descriptor, cells.options.count, full_paths, relative_paths)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (!state.lifetime.smoke_test) {
        for (const auto& path : full_paths) {
            if (GetFileAttributesW(path.c_str()) != INVALID_FILE_ATTRIBUTES) {
                state.engine->SetLocalFailure(
                    UiText(UiStringId::Text0142));
                return INKPOD_STATUS_INVALID_STATE;
            }
        }
    }
    std::vector<ApplicationHost::DocumentBinding> created;
    InkpodStatus status = CreateCellsFromOptions(
        state, cells.options, std::nullopt, &created);
    if (status != INKPOD_STATUS_OK || created.size() != full_paths.size()) {
        return status == INKPOD_STATUS_OK
            ? INKPOD_STATUS_INVALID_STATE
            : status;
    }
    std::vector<InkpodDocumentInfo> document_infos(created.size());
    for (std::size_t index = 0U; index < created.size(); ++index) {
        if (!state.ActivateDocumentView(created[index].view)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        status = SaveToPath(state, full_paths[index]);
        if (status != INKPOD_STATUS_OK
            || !state.engine->GetDocumentInfo(
                created[index].session,
                created[index].generation,
                document_infos[index])) {
            state.engine->SetLocalFailure(
                UiText(UiStringId::Text0473));
            return status == INKPOD_STATUS_OK
                ? INKPOD_STATUS_INVALID_STATE
                : status;
        }
    }
    EncodedCutMetadata metadata{};
    if (!EncodeCutMetadata(properties, metadata)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodCutDefaultsInput cut_defaults = CutDefaultsInput(cells.options);
    std::vector<std::vector<std::uint8_t>> encoded_names(relative_paths.size());
    std::vector<InkpodCutMemberInput> members(relative_paths.size());
    for (std::size_t index = 0U; index < relative_paths.size(); ++index) {
        if (!WidePathToUtf8(relative_paths[index], encoded_names[index])) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        members[index] = InkpodCutMemberInput{
            sizeof(InkpodCutMemberInput),
            static_cast<std::uint32_t>(index + 1U),
            document_infos[index].cell_id,
            document_infos[index].document_uuid_high,
            document_infos[index].document_uuid_low,
            InkpodUtf8Span{
                encoded_names[index].data(), encoded_names[index].size()}};
    }
    GUID uuid{};
    if (FAILED(CoCreateGuid(&uuid))) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::uint64_t uuid_high{};
    std::uint64_t uuid_low{};
    std::memcpy(&uuid_high, &uuid, sizeof(uuid_high));
    std::memcpy(
        &uuid_low,
        reinterpret_cast<const std::uint8_t*>(&uuid) + sizeof(uuid_high),
        sizeof(uuid_low));
    const InkpodCutCreateRequest request{
        sizeof(InkpodCutCreateRequest),
        0U,
        INKPOD_FEATURE_NONE,
        uuid_high,
        uuid_low,
        &metadata.input,
        &cut_defaults,
        members.data(),
        members.size(),
        sizeof(InkpodCutMemberInput)};
    std::vector<std::uint8_t> descriptor_utf8;
    if (!WidePathToUtf8(descriptor, descriptor_utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodCut* created_cut{};
    InkpodCutInfo info{};
    info.struct_size = sizeof(info);
    status = state.engine->Invoke(
        [&request, &descriptor_utf8, &created_cut, &info](InkpodCore*) {
            InkpodStatus result = inkpod_cut_create(&request, &created_cut);
            if (result == INKPOD_STATUS_OK) {
                result = inkpod_cut_save(
                    created_cut,
                    descriptor_utf8.data(),
                    descriptor_utf8.size(),
                    &info);
            }
            return result;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        ReleaseCutHandle(state, created_cut);
        return status;
    }
    return InstallCutSession(state, created_cut, descriptor, true);
}

InkpodStatus EditCutProperties(ApplicationHost& state, HWND owner) noexcept {
    CutSnapshot snapshot{};
    CutPropertiesDialogState properties{};
    InkpodCellCreationOptions defaults{};
    if (!QueryCutSnapshot(state, snapshot)
        || !CutSnapshotToDialog(snapshot, properties, defaults)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.lifetime.smoke_test) {
        properties.cut_name += L"-updated";
        properties.duration_frames += 1U;
    }
    if (ShowCutProperties(
            state.lifetime.instance, owner, state.lifetime.smoke_test, properties)
        != IDOK) {
        InkpodDispatchResult result{};
        result.struct_size = sizeof(result);
        return state.engine->Invoke(
            [cut = state.Workspace().cut.handle, &result](InkpodCore*) {
                return inkpod_cut_cancel_update(cut, &result);
            },
            false,
            false);
    }
    CellCreationDialogState cell_defaults{};
    cell_defaults.options = defaults;
    cell_defaults.layer_choices = LayerKindChoices().data();
    cell_defaults.layer_choice_count =
        static_cast<std::uint32_t>(LayerKindChoices().size());
    cell_defaults.build_preview = BuildCellCreationDialogPreview;
    if (ShowCellCreationOptions(
            state.lifetime.instance,
            owner,
            state.lifetime.smoke_test,
            cell_defaults) != IDOK) {
        InkpodDispatchResult result{};
        result.struct_size = sizeof(result);
        return state.engine->Invoke(
            [cut = state.Workspace().cut.handle, &result](InkpodCore*) {
                return inkpod_cut_cancel_update(cut, &result);
            },
            false,
            false);
    }
    EncodedCutMetadata metadata{};
    if (!EncodeCutMetadata(properties, metadata)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodCutDefaultsInput cut_defaults =
        CutDefaultsInput(cell_defaults.options);
    const InkpodCutUpdateRequest request{
        sizeof(InkpodCutUpdateRequest),
        0U,
        INKPOD_FEATURE_NONE,
        snapshot.info.revision,
        &metadata.input,
        &cut_defaults};
    InkpodDispatchResult result{};
    result.struct_size = sizeof(result);
    const InkpodStatus status = state.engine->Invoke(
        [cut = state.Workspace().cut.handle, &request, &result](InkpodCore*) {
            return inkpod_cut_update(cut, &request, &result);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        (void)RefreshCutSessionCache(state);
        RefreshSequencePane(state);
        UpdateMenuState(state);
    }
    return status;
}

std::optional<std::uint32_t> SelectedCutSequenceIndex(
    const ApplicationHost& state) noexcept {
    const HWND pane = state.Workspace().sequence_palette;
    if (pane == nullptr) {
        return std::nullopt;
    }
    const LRESULT selected = SendDlgItemMessageW(
        pane, IDC_SEQUENCE_CELLS, LB_GETCURSEL, 0, 0);
    if (selected == LB_ERR
        || static_cast<std::size_t>(selected)
            >= state.Workspace().cut.members.size()) {
        return std::nullopt;
    }
    return static_cast<std::uint32_t>(selected);
}

InkpodStatus CancelCutSequenceEdit(ApplicationHost& state) noexcept {
    const auto target = CaptureCutCoreTarget(state);
    if (!target.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodCutSequenceEditResult result{};
    result.struct_size = sizeof(result);
    return state.engine->Invoke(
        target->session,
        target->generation,
        [cut = target->cut, &result](InkpodCore*) {
            return inkpod_cut_sequence_cancel(cut, &result);
        },
        false,
        false);
}

InkpodStatus ApplyCutSequenceEdit(
    ApplicationHost& state,
    const std::vector<InkpodCutSequenceEditOperation>& operations) noexcept {
    const auto target = CaptureCutCoreTarget(state);
    CutSnapshot snapshot{};
    if (!target.has_value()
        || !QueryCutSnapshot(state, target.value(), snapshot)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodCutSequenceEditRequest request{
        sizeof(InkpodCutSequenceEditRequest),
        0U,
        INKPOD_FEATURE_NONE,
        snapshot.info.revision,
        operations.empty() ? nullptr : operations.data(),
        operations.size(),
        sizeof(InkpodCutSequenceEditOperation)};
    InkpodCutSequenceEditResult result{};
    result.struct_size = sizeof(result);
    const InkpodStatus status = state.engine->Invoke(
        target->session,
        target->generation,
        [cut = target->cut, &request, &result](InkpodCore*) {
            return inkpod_cut_sequence_edit(cut, &request, &result);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        CutSnapshot committed{};
        if (!QueryCutSnapshot(state, target.value(), committed)
            || committed.info.revision != result.revision
            || committed.info.state_id != result.state_id
            || committed.info.member_count != result.member_count
            || !RefreshCutSessionCache(state, &committed)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        RefreshSequencePane(state);
        UpdateMenuState(state);
    }
    return status;
}

InkpodCutSequenceEditOperation CutSequenceIdentityOperation(
    std::uint32_t kind, const CutMemberCache& member) noexcept {
    InkpodCutSequenceEditOperation operation{};
    operation.struct_size = sizeof(operation);
    operation.kind = kind;
    operation.cell_id = member.cell_id;
    operation.document_uuid_high = member.document_uuid_high;
    operation.document_uuid_low = member.document_uuid_low;
    return operation;
}

InkpodStatus ReorderCutSequence(
    ApplicationHost& state,
    std::uint32_t from,
    std::uint32_t to) noexcept {
    const auto& members = state.Workspace().cut.members;
    if (from >= members.size() || to >= members.size()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (from == to) {
        return CancelCutSequenceEdit(state);
    }
    InkpodCutSequenceEditOperation operation = CutSequenceIdentityOperation(
        from < to ? INKPOD_CUT_SEQUENCE_MOVE_AFTER
                  : INKPOD_CUT_SEQUENCE_MOVE_BEFORE,
        members[from]);
    operation.anchor_cell_id = members[to].cell_id;
    operation.anchor_document_uuid_high = members[to].document_uuid_high;
    operation.anchor_document_uuid_low = members[to].document_uuid_low;
    return ApplyCutSequenceEdit(state, {operation});
}

InkpodStatus AddCutSequenceMember(
    ApplicationHost& state, HWND owner) noexcept {
    std::wstring path;
    if (state.lifetime.smoke_test) {
        const std::wstring& descriptor = state.Workspace().cut.current_path;
        const std::size_t slash = descriptor.find_last_of(L"\\/");
        path = (slash == std::wstring::npos
                ? std::wstring{}
                : descriptor.substr(0U, slash + 1U))
            + L"inkpod-cut-smoke-0002.inkpod";
    } else if (!ChooseInkpodPath(owner, false, path)) {
        (void)CancelCutSequenceEdit(state);
        return INKPOD_STATUS_CANCELLED;
    }
    const std::wstring& descriptor = state.Workspace().cut.current_path;
    const std::size_t descriptor_slash = descriptor.find_last_of(L"\\/");
    const std::size_t member_slash = path.find_last_of(L"\\/");
    const std::wstring descriptor_directory = descriptor_slash == std::wstring::npos
        ? std::wstring{}
        : descriptor.substr(0U, descriptor_slash + 1U);
    const std::wstring member_directory = member_slash == std::wstring::npos
        ? std::wstring{}
        : path.substr(0U, member_slash + 1U);
    if (_wcsicmp(
            descriptor_directory.c_str(), member_directory.c_str()) != 0) {
        state.engine->SetLocalFailure(
            UiText(UiStringId::Text0952));
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const std::wstring relative = member_slash == std::wstring::npos
        ? path
        : path.substr(member_slash + 1U);
    std::vector<std::uint8_t> encoded_path;
    std::vector<std::uint8_t> encoded_relative;
    if (relative.empty() || !WidePathToUtf8(path, encoded_path)
        || !WidePathToUtf8(relative, encoded_relative)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodDocumentInfo info{};
    info.struct_size = sizeof(info);
    const InkpodStatus probe_status = state.engine->Invoke(
        [&encoded_path, &info](InkpodCore*) {
            const InkpodCoreConfig config{
                sizeof(InkpodCoreConfig),
                INKPOD_ABI_VERSION,
                INKPOD_FEATURE_NONE};
            InkpodCore* probe{};
            InkpodStatus status = inkpod_core_create(&config, &probe);
            if (status == INKPOD_STATUS_OK) {
                status = inkpod_core_open(
                    probe, encoded_path.data(), encoded_path.size(), &info);
            }
            const InkpodStatus destroy_status = inkpod_core_destroy(&probe);
            return status == INKPOD_STATUS_OK ? destroy_status : status;
        },
        false,
        false);
    if (probe_status != INKPOD_STATUS_OK) {
        return probe_status;
    }
    std::uint32_t display_number{};
    for (const auto& member : state.Workspace().cut.members) {
        display_number = std::max(display_number, member.display_number);
    }
    if (display_number == UINT32_MAX
        || state.Workspace().cut.members.size() >= UINT32_MAX) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodCutSequenceEditOperation operation{};
    operation.struct_size = sizeof(operation);
    operation.kind = INKPOD_CUT_SEQUENCE_INSERT;
    operation.cell_id = info.cell_id;
    operation.document_uuid_high = info.document_uuid_high;
    operation.document_uuid_low = info.document_uuid_low;
    operation.position = static_cast<std::uint32_t>(
        state.Workspace().cut.members.size());
    operation.display_number = display_number + 1U;
    operation.relative_path = {
        encoded_relative.data(), encoded_relative.size()};
    return ApplyCutSequenceEdit(state, {operation});
}

InkpodStatus RemoveCutSequenceMember(ApplicationHost& state) noexcept {
    const auto selected = SelectedCutSequenceIndex(state);
    if (!selected.has_value()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const auto operation = CutSequenceIdentityOperation(
        INKPOD_CUT_SEQUENCE_REMOVE,
        state.Workspace().cut.members[selected.value()]);
    return ApplyCutSequenceEdit(state, {operation});
}

InkpodStatus MoveCutSequenceMember(
    ApplicationHost& state, bool down) noexcept {
    const auto selected = SelectedCutSequenceIndex(state);
    if (!selected.has_value()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const std::uint32_t index = selected.value();
    if ((!down && index == 0U)
        || (down && index + 1U >= state.Workspace().cut.members.size())) {
        return CancelCutSequenceEdit(state);
    }
    return ReorderCutSequence(state, index, down ? index + 1U : index - 1U);
}

InkpodStatus RenumberCutSequence(
    ApplicationHost& state, HWND owner) noexcept {
    if (state.Workspace().cut.members.empty()) {
        return CancelCutSequenceEdit(state);
    }
    ViewOptionsDialogState dialog{};
    dialog.title = UiText(UiStringId::Text0237);
    dialog.labels[0] = UiText(UiStringId::Text0493);
    dialog.labels[1] = UiText(UiStringId::Text0606);
    dialog.values[0] = 1;
    dialog.values[1] = 1;
    if (ShowViewOptions(
            state.lifetime.instance,
            owner,
            state.lifetime.smoke_test,
            dialog) != IDOK) {
        (void)CancelCutSequenceEdit(state);
        return INKPOD_STATUS_CANCELLED;
    }
    if (dialog.values[0] <= 0 || dialog.values[1] <= 0) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodCutSequenceEditOperation operation{};
    operation.struct_size = sizeof(operation);
    operation.kind = INKPOD_CUT_SEQUENCE_RENUMBER_RANGE;
    operation.position = 0U;
    operation.count = static_cast<std::uint32_t>(
        state.Workspace().cut.members.size());
    operation.first_number = static_cast<std::uint32_t>(dialog.values[0]);
    operation.step = static_cast<std::uint32_t>(dialog.values[1]);
    return ApplyCutSequenceEdit(state, {operation});
}

InkpodStatus MoveCutHistory(ApplicationHost& state, bool redo) noexcept {
    if (state.engine == nullptr || state.Workspace().cut.handle == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodDispatchResult result{};
    result.struct_size = sizeof(result);
    const InkpodStatus status = state.engine->Invoke(
        [cut = state.Workspace().cut.handle, redo, &result](InkpodCore*) {
            return redo ? inkpod_cut_redo(cut, &result)
                        : inkpod_cut_undo(cut, &result);
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        (void)RefreshCutSessionCache(state);
        RefreshSequencePane(state);
        UpdateMenuState(state);
    }
    return status;
}

bool BuildCellCreationDialogPreview(
    void*,
    const InkpodCellCreationOptions& options,
    InkpodCellCreationPlanItem& preview) noexcept {
    InkpodCellCreationOptions preview_options = options;
    preview_options.count = 1U;
    InkpodCellCreationPlan* plan{};
    InkpodStatus status = inkpod_cell_creation_plan_create(&preview_options, &plan);
    if (status != INKPOD_STATUS_OK) {
        return false;
    }
    std::uint32_t written{};
    status = inkpod_cell_creation_plan_copy(
        plan, &preview, 1U, sizeof(preview), &written);
    const InkpodStatus release_status = inkpod_cell_creation_plan_release(&plan);
    return status == INKPOD_STATUS_OK
        && release_status == INKPOD_STATUS_OK && written == 1U;
}

InkpodStatus CreateCellWithLayer(
    ApplicationHost& state,
    std::uint32_t width,
    std::uint32_t height,
    std::uint32_t dpi_milli,
    std::uint32_t initial_layer_kind) noexcept {
    const InkpodCellCreationOptions options{
        sizeof(InkpodCellCreationOptions),
        INKPOD_CELL_SIZING_IMAGE_PIXELS,
        INKPOD_FEATURE_NONE,
        width,
        height,
        dpi_milli,
        dpi_milli,
        0U,
        900U,
        500U,
        INKPOD_FRAME_ANCHOR_CENTER,
        initial_layer_kind,
        INKPOD_STORAGE_RGBA8,
        1U,
        0U};
    return CreateCellsFromOptions(state, options);
}

InkpodStatus CreateCell(
    ApplicationHost& state,
    std::uint32_t width,
    std::uint32_t height,
    std::uint32_t dpi_milli) noexcept {
    return CreateCellWithLayer(
        state,
        width,
        height,
        dpi_milli,
        INKPOD_LAYER_BINARY_COLORING);
}

InkpodStatus CreateDefaultCellImpl(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodEditorDefaults defaults{};
    const InkpodStatus status = state.engine->GetEditorDefaults(
        state.Document().id, state.Document().generation, defaults);
    return status == INKPOD_STATUS_OK
        ? CreateCell(
              state,
              defaults.width,
              defaults.height,
              defaults.dpi_x_milli)
        : status;
}

bool ChoosePalettePath(HWND owner, bool save, std::wstring& path) noexcept {
    std::array<wchar_t, 32768U> buffer{};
    if (!path.empty()) {
        wcsncpy_s(buffer.data(), buffer.size(), path.c_str(), _TRUNCATE);
    }
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = UiText(UiStringId::Text0058);
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
    std::vector<std::uint8_t> utf8_path;
    if (!WidePathToUtf8(path, utf8_path)) {
        return false;
    }
    InkpodColorArray input{};
    input.struct_size = sizeof(input);
    input.colors = colors.empty() ? nullptr : colors.data();
    input.color_count = colors.size();
    input.color_stride_bytes = colors.empty() ? 0U : sizeof(InkpodColorValue);
    return inkpod_palette_file_save(
               utf8_path.data(), utf8_path.size(), &input)
        == INKPOD_STATUS_OK;
}

bool LoadPaletteFile(
    const std::wstring& path, std::vector<InkpodColorValue>& colors) noexcept {
    std::vector<std::uint8_t> utf8_path;
    if (!WidePathToUtf8(path, utf8_path)) {
        return false;
    }
    InkpodColorBuffer buffer{};
    buffer.struct_size = sizeof(buffer);
    if (inkpod_palette_file_load(
            utf8_path.data(), utf8_path.size(), &buffer)
        != INKPOD_STATUS_OK
        || buffer.color_count > 4096U) {
        return false;
    }
    try {
        colors.resize(static_cast<std::size_t>(buffer.color_count));
    } catch (const std::bad_alloc&) {
        return false;
    }
    if (colors.empty()) {
        return true;
    }
    buffer.colors = colors.data();
    buffer.color_capacity = colors.size();
    buffer.color_stride_bytes = sizeof(InkpodColorValue);
    return inkpod_palette_file_load(
               utf8_path.data(), utf8_path.size(), &buffer)
        == INKPOD_STATUS_OK;
}

bool ChooseChartPath(HWND owner, bool save, std::wstring& path) noexcept {
    std::array<wchar_t, 32768U> buffer{};
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = UiText(UiStringId::Text0057);
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
    std::vector<std::uint8_t> utf8_path;
    std::vector<std::vector<std::uint8_t>> encoded_names;
    std::vector<InkpodColorChartEntry> entries;
    try {
        if (!WidePathToUtf8(path, utf8_path)) {
            return false;
        }
        encoded_names.resize(names.size());
        entries.resize(names.size());
        for (std::size_t index = 0U; index < names.size(); ++index) {
            if (!WidePathToUtf8(names[index], encoded_names[index])
                || encoded_names[index].empty() || encoded_names[index].size() > 1024U) {
                return false;
            }
            entries[index].struct_size = sizeof(InkpodColorChartEntry);
            entries[index].color = colors[index];
            entries[index].name_utf8 = encoded_names[index].data();
            entries[index].name_bytes = encoded_names[index].size();
        }
    } catch (const std::bad_alloc&) {
        return false;
    }
    return inkpod_color_chart_file_save(
               utf8_path.data(),
               utf8_path.size(),
               entries.empty() ? nullptr : entries.data(),
               entries.size(),
               entries.empty() ? 0U : sizeof(InkpodColorChartEntry))
        == INKPOD_STATUS_OK;
}

bool LoadColorChartFile(
    const std::wstring& path,
    std::vector<InkpodColorValue>& colors,
    std::vector<std::wstring>& names) noexcept {
    std::vector<std::uint8_t> utf8_path;
    if (!WidePathToUtf8(path, utf8_path)) {
        return false;
    }
    InkpodColorChartFile* chart{};
    if (inkpod_color_chart_file_load(
            utf8_path.data(), utf8_path.size(), &chart)
        != INKPOD_STATUS_OK) {
        return false;
    }
    std::uint64_t count{};
    if (inkpod_color_chart_file_count(chart, &count) != INKPOD_STATUS_OK
        || count > 4096U) {
        (void)inkpod_color_chart_file_release(&chart);
        return false;
    }
    try {
        colors.clear();
        names.clear();
        colors.reserve(static_cast<std::size_t>(count));
        names.reserve(static_cast<std::size_t>(count));
        for (std::uint64_t index = 0U; index < count; ++index) {
            InkpodColorValue color{};
            color.struct_size = sizeof(color);
            std::uint64_t name_bytes{};
            if (inkpod_color_chart_file_get(
                    chart, index, &color, nullptr, 0U, &name_bytes)
                    != INKPOD_STATUS_OK
                || name_bytes == 0U || name_bytes > 1024U) {
                (void)inkpod_color_chart_file_release(&chart);
                return false;
            }
            std::vector<std::uint8_t> utf8_name(static_cast<std::size_t>(name_bytes));
            if (inkpod_color_chart_file_get(
                    chart,
                    index,
                    &color,
                    utf8_name.data(),
                    utf8_name.size(),
                    &name_bytes)
                != INKPOD_STATUS_OK) {
                (void)inkpod_color_chart_file_release(&chart);
                return false;
            }
            const int wide_count = MultiByteToWideChar(
                CP_UTF8,
                MB_ERR_INVALID_CHARS,
                reinterpret_cast<const char*>(utf8_name.data()),
                static_cast<int>(name_bytes),
                nullptr,
                0);
            if (wide_count <= 0) {
                (void)inkpod_color_chart_file_release(&chart);
                return false;
            }
            std::wstring name(static_cast<std::size_t>(wide_count), L'\0');
            if (MultiByteToWideChar(
                    CP_UTF8,
                    MB_ERR_INVALID_CHARS,
                    reinterpret_cast<const char*>(utf8_name.data()),
                    static_cast<int>(name_bytes),
                    name.data(),
                    wide_count) != wide_count) {
                (void)inkpod_color_chart_file_release(&chart);
                return false;
            }
            colors.push_back(color);
            names.push_back(std::move(name));
        }
    } catch (const std::bad_alloc&) {
        colors.clear();
        names.clear();
        (void)inkpod_color_chart_file_release(&chart);
        return false;
    }
    return inkpod_color_chart_file_release(&chart) == INKPOD_STATUS_OK;
}

bool BeginNewDocumentTab(
    ApplicationHost& state,
    DocumentViewId& previous_view,
    std::optional<ApplicationHost::DocumentBinding>& added) noexcept {
    previous_view = state.routing.targets.ActiveDocumentView();
    if (!state.routing.targets.DocumentSession()) {
        added = state.AddDocumentSession();
        return added.has_value();
    }
    InkpodDocumentInfo existing{};
    if (state.Documents().Count() != 0U && !QueryDocument(state, existing)) {
        return true;
    }
    added = state.AddDocumentSession();
    return added.has_value();
}

void RollbackNewDocumentTab(
    ApplicationHost& state,
    const std::optional<ApplicationHost::DocumentBinding>& added,
    DocumentViewId previous_view) noexcept {
    if (!added.has_value()) {
        return;
    }
    (void)state.CloseDocumentSession(added->session);
    if (previous_view) {
        (void)ActivateDocumentTab(state, previous_view);
    }
}

InkpodStatus ImportCommonRasterFromPath(
    ApplicationHost& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentIdentity identity{};
    std::wstring recent_path;
    try {
        recent_path = path;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!ResolveDocumentFileIdentity(path, identity)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (const auto* existing = state.Documents().FindByIdentity(identity);
        existing != nullptr && existing->ActiveView() != nullptr) {
        if (!ActivateDocumentTab(state, existing->ActiveView()->id)
            || !state.RecordRecentDocument(
                std::move(recent_path), std::move(identity))) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        UpdateMenuState(state);
        return INKPOD_STATUS_OK;
    }
    DocumentViewId previous_view{};
    std::optional<ApplicationHost::DocumentBinding> added;
    if (!BeginNewDocumentTab(state, previous_view, added)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(
        state.Document().shell,
        *state.engine,
        state.Document().id,
        state.Document().generation);
    const InkpodStatus status = shell.ImportCommonRaster(path);
    if (status != INKPOD_STATUS_OK) {
        RollbackNewDocumentTab(state, added, previous_view);
        return status;
    }
    if (!state.Documents().AssignIdentity(state.Document().id, identity)) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    AttachFilePreviewSequence(state, path);
    state.Document().untitled_number = 0U;
    state.ActiveView().presentation.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetUiForNewActiveDocument(state);
    if (!state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus view_status = FitCanvas(state, INKPOD_VIEW_FIT);
    if (view_status != INKPOD_STATUS_OK) {
        RollbackNewDocumentTab(state, added, previous_view);
    } else if (!state.RecordRecentDocument(
                   std::move(recent_path), std::move(identity))) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    RefreshSequencePane(state);
    (void)RefreshSubpalettePane(state);
    UpdateMenuState(state);
    return view_status;
}

InkpodStatus ExportCommonRasterToPath(
    ApplicationHost& state, const std::wstring& path, bool composite_white) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(
        state.Document().shell,
        *state.engine,
        state.Document().id,
        state.Document().generation);
    return shell.ExportCommonRaster(path, composite_white);
}

InkpodStatus ExportInstructionCommonRasterToPath(
    ApplicationHost& state, const std::wstring& path, bool composite_white) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(
        state.Document().shell,
        *state.engine,
        state.Document().id,
        state.Document().generation);
    return shell.ExportInstructionCommonRaster(path, composite_white);
}

InkpodStatus ApplyLightTableEdit(
    ApplicationHost& state,
    const CommandContext& context,
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
    return state.engine == nullptr || !context.document_session.has_value()
            || !context.generation.has_value()
        ? INKPOD_STATUS_INVALID_STATE
        : state.engine->Invoke(
              context.document_session.value(),
              context.generation.value(),
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
    ApplicationHost& state,
    const CommandContext& context,
    std::uint32_t index,
    InkpodLightTableItemInfo& output) noexcept {
    output = {};
    output.struct_size = sizeof(output);
    output.display_color.struct_size = sizeof(output.display_color);
    return state.engine != nullptr && context.document_session.has_value()
        && context.generation.has_value()
        && state.engine->Invoke(
               context.document_session.value(),
               context.generation.value(),
               [index, &output](InkpodCore* core) {
                   return inkpod_core_light_table_item_get(core, index, &output);
               },
               false,
               false)
            == INKPOD_STATUS_OK;
}

bool QueryLightTableItem(
    ApplicationHost& state,
    std::uint32_t index,
    InkpodLightTableItemInfo& output) noexcept {
    return QueryLightTableItem(
        state, state.routing.targets.Capture(), index, output);
}

bool PrepareLightTableSelection(
    ApplicationHost& state, const CommandContext& context) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return false;
    }
    if (state.Workspace().panes.light_table_selection_session
            == context.document_session.value()
        && state.Workspace().panes.light_table_selection_generation
            == context.generation.value()) {
        return true;
    }
    std::vector<LightTablePaneSet> sets;
    std::vector<LightTablePaneItem> items;
    DocumentPanesController controller(*state.engine);
    if (controller.LoadLightTable(
            context.document_session.value(),
            context.generation.value(),
            sets,
            items) != INKPOD_STATUS_OK
        || sets.empty()) {
        return false;
    }
    std::uint32_t selected_set{};
    for (std::size_t index = 0U; index < sets.size(); ++index) {
        if ((sets[index].flags & INKPOD_LIGHT_TABLE_SET_ACTIVE) != 0U) {
            selected_set = static_cast<std::uint32_t>(index);
            break;
        }
    }
    auto& panes = state.Workspace().panes;
    panes.light_table_selection_session = context.document_session.value();
    panes.light_table_selection_generation = context.generation.value();
    panes.active_light_table_set_index = selected_set;
    panes.active_light_table_set_id = sets[selected_set].id;
    panes.light_table_set_count = static_cast<std::uint32_t>(sets.size());
    panes.light_table_item_count = static_cast<std::uint32_t>(items.size());
    panes.active_light_table_item_index = 0U;
    panes.active_light_table_item_id = items.empty() ? 0U : items[0].info.id;
    return true;
}

InkpodStatus AddOrReloadLightTableRaster(
    ApplicationHost& state,
    const CommandContext& context,
    const std::wstring& path,
    bool reload) noexcept {
    std::vector<std::uint8_t> bytes;
    const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(path);
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value() || format == 0U
        || !ReadBoundedFile(path, bytes)) {
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
    const std::uint64_t item_id = state.Workspace().panes.active_light_table_item_id;
    std::uint64_t added_item_id{};
    const InkpodStatus status = state.engine->Invoke(
        context.document_session.value(),
        context.generation.value(),
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
        state.Workspace().panes.active_light_table_item_id = added_item_id;
        state.Workspace().panes.active_light_table_item_index = 0U;
    }
    return status;
}

InkpodStatus EditLightTableItemProperties(
    ApplicationHost& state, const CommandContext& context) noexcept {
    InkpodLightTableItemInfo info{};
    if (!QueryLightTableItem(
            state,
            context,
            state.Workspace().panes.active_light_table_item_index,
            info)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState display{};
    display.title = UiText(UiStringId::Text0377);
    display.labels = {UiText(UiStringId::Text0433), UiText(UiStringId::Text0881), UiText(UiStringId::Text0880), UiText(UiStringId::Text0577)};
    display.values = {
        static_cast<std::int32_t>(info.opacity_milli / 10U),
        static_cast<std::int32_t>(info.display_mode),
        (info.flags & INKPOD_LIGHT_TABLE_ITEM_VISIBLE) != 0U ? 1 : 0,
        info.rotation_milli_degrees / 1000};
    display.value_count = 4U;
    if (state.lifetime.smoke_test) {
        display.values = {50, INKPOD_LIGHT_TABLE_MONOTONE, 1, 5};
    }
    if (ShowViewOptions(state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, display) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    ViewOptionsDialogState transform{};
    transform.title = UiText(UiStringId::Text0373);
    transform.labels = {UiText(UiStringId::Text0824), UiText(UiStringId::Text0825), UiText(UiStringId::Text0475), UiText(UiStringId::Text0476)};
    transform.values = {
        info.translate_x_milli,
        info.translate_y_milli,
        static_cast<std::int32_t>(info.scale_x_milli / 10U),
        static_cast<std::int32_t>(info.scale_y_milli / 10U)};
    transform.value_count = 4U;
    if (state.lifetime.smoke_test) {
        transform.values = {1000, -1000, 110, 90};
    }
    if (ShowViewOptions(state.lifetime.instance, state.Workspace().windows.window, state.lifetime.smoke_test, transform) != IDOK
        || display.values[0] < 0 || display.values[0] > 100
        || display.values[1] < INKPOD_LIGHT_TABLE_COLOR
        || display.values[1] > INKPOD_LIGHT_TABLE_HALFTONE
        || transform.values[2] <= 0 || transform.values[3] <= 0) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const InkpodEditorStateInfo* editor = PresentedEditorState(state);
    if (editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_layer_id == 0U || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodLightTableEdit edit{};
    edit.operation = INKPOD_LIGHT_TABLE_UPDATE_ITEM;
    edit.object_id = info.id;
    edit.flags = display.values[2] != 0 ? INKPOD_LIGHT_TABLE_ITEM_VISIBLE : 0U;
    edit.opacity_milli = static_cast<std::uint32_t>(display.values[0]) * 10U;
    edit.display_mode = static_cast<std::uint32_t>(display.values[1]);
    edit.display_color = editor->current_color;
    edit.translate_x_milli = transform.values[0];
    edit.translate_y_milli = transform.values[1];
    edit.scale_x_milli = static_cast<std::uint32_t>(transform.values[2]) * 10U;
    edit.scale_y_milli = static_cast<std::uint32_t>(transform.values[3]) * 10U;
    edit.rotation_milli_degrees = display.values[3] * 1000;
    std::uint64_t ignored{};
    return ApplyLightTableEdit(state, context, edit, {}, ignored);
}

InkpodStatus RegisterSequenceNeighborsInLightTable(
    ApplicationHost& state,
    const CommandContext& context,
    InkpodLightTableBulkDirection direction) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()
        || state.Workspace().panes.active_light_table_set_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ViewOptionsDialogState dialog{};
    dialog.title = UiText(UiStringId::Text0369);
    dialog.labels = {
        UiText(UiStringId::Text1031),
        UiText(UiStringId::Text0946),
        UiText(UiStringId::Text0947),
        nullptr};
    dialog.values = state.lifetime.smoke_test
        ? std::array<std::int32_t, 4U>{1, 80, 20, 0}
        : std::array<std::int32_t, 4U>{2, 80, 20, 0};
    dialog.value_count = 3U;
    if (ShowViewOptions(
            state.lifetime.instance,
            state.Workspace().windows.window,
            state.lifetime.smoke_test,
            dialog) != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }
    if (dialog.values[0] < 0 || dialog.values[0] > 10'000
        || dialog.values[1] < 0 || dialog.values[1] > 100
        || dialog.values[2] < 0 || dialog.values[2] > 100) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }

    InkpodLightTableBulkRequest request{};
    request.struct_size = sizeof(request);
    InkpodLightTableBulkPreviewInfo preview{};
    preview.struct_size = sizeof(preview);
    const std::uint64_t target_set_id =
        state.Workspace().panes.active_light_table_set_id;
    InkpodStatus status = state.engine->Invoke(
        context.document_session.value(),
        context.generation.value(),
        [target_set_id,
         direction,
         count = static_cast<std::uint32_t>(dialog.values[0]),
         opacity = static_cast<std::uint32_t>(dialog.values[1]) * 10U,
         step = static_cast<std::uint32_t>(dialog.values[2]) * 10U,
         &request,
         &preview](InkpodCore* core) {
            InkpodStatus capture = inkpod_core_light_table_bulk_request(
                core,
                target_set_id,
                direction,
                count,
                opacity,
                step,
                &request);
            if (capture != INKPOD_STATUS_OK) {
                return capture;
            }
            return inkpod_core_light_table_bulk_preview(
                core, &request, nullptr, 0U, 0U, &preview);
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }

    std::vector<InkpodLightTableBulkPreviewEntry> entries;
    try {
        entries.resize(static_cast<std::size_t>(preview.entry_count));
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    for (auto& entry : entries) {
        entry.struct_size = sizeof(entry);
    }
    if (!entries.empty()) {
        status = state.engine->Invoke(
            context.document_session.value(),
            context.generation.value(),
            [&request, &preview, &entries](InkpodCore* core) {
                return inkpod_core_light_table_bulk_preview(
                    core,
                    &request,
                    entries.data(),
                    entries.size(),
                    sizeof(InkpodLightTableBulkPreviewEntry),
                    &preview);
            },
            false,
            false);
        if (status != INKPOD_STATUS_OK) {
            return status;
        }
    }

    std::wstring message;
    try {
        std::array<wchar_t, 256U> line{};
        _snwprintf_s(
            line.data(),
            line.size(),
            _TRUNCATE,
            UiText(UiStringId::LightTableBulkPreviewHeaderFormat),
            static_cast<unsigned long long>(preview.entry_count),
            preview.add_count,
            preview.skip_count);
        message = line.data();
        constexpr std::size_t kVisiblePreviewEntries = 32U;
        const std::size_t visible = std::min(entries.size(), kVisiblePreviewEntries);
        for (std::size_t index = 0U; index < visible; ++index) {
            const auto& entry = entries[index];
            if (entry.action == INKPOD_LIGHT_TABLE_BULK_SKIP_EXISTING) {
                _snwprintf_s(
                    line.data(),
                    line.size(),
                    _TRUNCATE,
                    UiText(UiStringId::LightTableKeepExistingLineFormat),
                    index + 1U,
                    entry.cell_number,
                    entry.distance,
                    entry.opacity_milli / 10U,
                    entry.opacity_milli % 10U,
                    static_cast<unsigned long long>(entry.source_generation),
                    static_cast<unsigned long long>(entry.existing_source_revision));
            } else {
                _snwprintf_s(
                    line.data(),
                    line.size(),
                    _TRUNCATE,
                    UiText(UiStringId::Text0017),
                    index + 1U,
                    entry.cell_number,
                    entry.distance,
                    entry.opacity_milli / 10U,
                    entry.opacity_milli % 10U);
            }
            message += line.data();
        }
        if (visible < entries.size()) {
            _snwprintf_s(
                line.data(),
                line.size(),
                _TRUNCATE,
                UiText(UiStringId::Text0021),
                entries.size() - visible);
            message += line.data();
        }
        message += UiText(UiStringId::Text0003);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }

    const int choice = state.lifetime.smoke_test
        ? state.lifetime.smoke_dirty_prompt_choice
        : MessageBoxW(
              state.Workspace().light_table_palette != nullptr
                  ? state.Workspace().light_table_palette
                  : state.Workspace().windows.window,
              message.c_str(),
              UiText(UiStringId::Text0370),
              MB_OKCANCEL | MB_ICONQUESTION);
    if (choice != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }

    std::vector<std::uint64_t> item_ids;
    try {
        item_ids.resize(preview.add_count);
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodDispatchResult result{};
    result.struct_size = sizeof(result);
    InkpodLightTableBulkSummary summary{};
    summary.struct_size = sizeof(summary);
    status = state.engine->Invoke(
        context.document_session.value(),
        context.generation.value(),
        [&request, &result, &summary, &item_ids](InkpodCore* core) {
            return inkpod_core_light_table_bulk_register(
                core,
                &request,
                &result,
                &summary,
                item_ids.empty() ? nullptr : item_ids.data(),
                item_ids.size());
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK && !item_ids.empty()) {
        state.Workspace().panes.active_light_table_item_id = item_ids.front();
        state.Workspace().panes.active_light_table_item_index = 0U;
    }
    return status;
}

InkpodStatus MoveLightTableFromCanvas(
    ApplicationHost& state, const CommandContext& context) noexcept {
    if (state.Workspace().panes.light_table_move_samples.size() < 2U) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (context.document_session != state.routing.targets.DocumentSession()
        || context.document_view != state.routing.targets.ActiveDocumentView()
        || context.generation != state.routing.targets.CurrentGeneration()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodDocumentInfo document{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()
        || !state.engine->GetDocumentInfo(
            context.document_session.value(),
            context.generation.value(),
            document)
        || document.width == 0U
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(document.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    double delta_x = (state.Workspace().panes.light_table_move_samples.back().x
        - state.Workspace().panes.light_table_move_samples.front().x) / zoom;
    double delta_y = (state.Workspace().panes.light_table_move_samples.back().y
        - state.Workspace().panes.light_table_move_samples.front().y) / zoom;
    if (state.ActiveView().presentation.flip_horizontal) {
        delta_x = -delta_x;
    }
    if (state.ActiveView().presentation.flip_vertical) {
        delta_y = -delta_y;
    }
    const std::int64_t delta_x_milli = std::llround(delta_x * 1000.0);
    const std::int64_t delta_y_milli = std::llround(delta_y * 1000.0);
    const bool all_items = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
    InkpodStatus status = INKPOD_STATUS_OK;
    for (std::uint32_t index = 0U; index < 10000U; ++index) {
        if (!all_items && index != state.Workspace().panes.active_light_table_item_index) {
            continue;
        }
        InkpodLightTableItemInfo info{};
        if (!QueryLightTableItem(state, context, index, info)) {
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
        status = ApplyLightTableEdit(state, context, edit, {}, ignored);
        if (status != INKPOD_STATUS_OK || !all_items) {
            break;
        }
    }
    return status;
}

struct SequenceEncodedFile {
    InkpodCommonRasterFormat format{};
    std::vector<std::uint8_t> name;
    std::vector<std::uint8_t> bytes;
};

struct SequenceFilenamePattern final {
    std::size_t digit_start{};
    std::size_t digit_end{};
    std::size_t stem_end{};
};

bool ExtractSequenceFilenamePattern(
    std::wstring_view name, SequenceFilenamePattern& pattern) noexcept {
    pattern = {};
    pattern.stem_end = name.find_last_of(L'.');
    if (pattern.stem_end == std::wstring_view::npos) {
        pattern.stem_end = name.size();
    }
    pattern.digit_end = pattern.stem_end;
    while (pattern.digit_end > 0U
        && (name[pattern.digit_end - 1U] < L'0'
            || name[pattern.digit_end - 1U] > L'9')) {
        --pattern.digit_end;
    }
    if (pattern.digit_end == 0U) {
        return false;
    }
    pattern.digit_start = pattern.digit_end;
    while (pattern.digit_start > 0U
        && name[pattern.digit_start - 1U] >= L'0'
        && name[pattern.digit_start - 1U] <= L'9') {
        --pattern.digit_start;
    }
    return pattern.digit_start < pattern.digit_end;
}

bool MatchesSequenceFilenamePattern(
    std::wstring_view candidate,
    std::wstring_view opened,
    const SequenceFilenamePattern& opened_pattern) noexcept {
    SequenceFilenamePattern candidate_pattern{};
    if (!ExtractSequenceFilenamePattern(candidate, candidate_pattern)
        || candidate_pattern.digit_start != opened_pattern.digit_start
        || candidate_pattern.stem_end - candidate_pattern.digit_end
            != opened_pattern.stem_end - opened_pattern.digit_end) {
        return false;
    }
    return _wcsnicmp(
               candidate.data(), opened.data(), opened_pattern.digit_start)
            == 0
        && _wcsnicmp(
               candidate.data() + candidate_pattern.digit_end,
               opened.data() + opened_pattern.digit_end,
               opened_pattern.stem_end - opened_pattern.digit_end)
            == 0;
}

InkpodStatus ImportSequencePaths(
    ApplicationHost& state,
    const std::vector<std::wstring>& paths,
    DocumentSessionId session,
    Generation generation) noexcept {
    if (state.engine == nullptr || paths.empty()) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    std::vector<SequenceEncodedFile> files;
    try {
        files.reserve(paths.size());
        for (const std::wstring& path : paths) {
            const InkpodCommonRasterFormat format = CommonRasterFormatFromPath(path);
            if (format == 0U) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            std::wstring filename = path;
            const std::size_t slash = filename.find_last_of(L"\\/");
            if (slash != std::wstring::npos) {
                filename.erase(0, slash + 1U);
            }
            SequenceEncodedFile file{};
            file.format = format;
            if (!WidePathToUtf8(filename, file.name)
                || !ReadBoundedFile(path, file.bytes)) {
                return INKPOD_STATUS_IO_ERROR;
            }
            files.push_back(std::move(file));
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        session,
        generation,
        [files = std::move(files)](InkpodCore* core) {
            std::vector<InkpodNamedRasterInput> records;
            try {
                records.reserve(files.size());
                for (const auto& file : files) {
                    records.push_back(InkpodNamedRasterInput{
                        sizeof(InkpodNamedRasterInput),
                        0U,
                        file.format,
                        0U,
                        file.name.data(),
                        file.name.size(),
                        file.bytes.data(),
                        file.bytes.size()});
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            return inkpod_core_sequence_import_mixed_encoded(
                core, records.data(), records.size(), sizeof(InkpodNamedRasterInput));
        },
        false,
        false);
    if (status == INKPOD_STATUS_OK) {
        auto* document = state.Documents().Find(session);
        if (document != nullptr && document->generation == generation) {
            document->ClearSequenceAutosaves();
        }
    }
    return status;
}

void AttachFilePreviewSequence(
    ApplicationHost& state, const std::wstring& opened_path) noexcept {
    if (state.engine == nullptr || opened_path.empty()) {
        return;
    }
    const std::size_t separator = opened_path.find_last_of(L"\\/");
    const std::wstring directory = separator == std::wstring::npos
        ? L".\\"
        : opened_path.substr(0U, separator + 1U);
    const std::wstring opened_name = separator == std::wstring::npos
        ? opened_path
        : opened_path.substr(separator + 1U);
    SequenceFilenamePattern opened_pattern{};
    if (!ExtractSequenceFilenamePattern(opened_name, opened_pattern)) {
        return;
    }
    std::vector<std::wstring> paths;
    WIN32_FIND_DATAW entry{};
    const std::wstring pattern = directory + L"*";
    HANDLE search = FindFirstFileW(pattern.c_str(), &entry);
    if (search == INVALID_HANDLE_VALUE) {
        return;
    }
    bool includes_opened{};
    try {
        do {
            const std::wstring_view name(entry.cFileName);
            if ((entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0U
                || CommonRasterFormatFromPath(std::wstring(name)) == 0U
                || !MatchesSequenceFilenamePattern(
                    name, opened_name, opened_pattern)) {
                continue;
            }
            paths.push_back(directory + std::wstring(name));
            includes_opened = includes_opened
                || _wcsicmp(name.data(), opened_name.c_str()) == 0;
            if (paths.size() >= 10'000U) {
                break;
            }
        } while (FindNextFileW(search, &entry) != FALSE);
    } catch (const std::bad_alloc&) {
        FindClose(search);
        return;
    }
    FindClose(search);
    if (!includes_opened || paths.empty()
        || ImportSequencePaths(
               state,
               paths,
               state.Document().id,
               state.Document().generation)
            != INKPOD_STATUS_OK) {
        return;
    }

    std::vector<SequencePaneCell> cells;
    DocumentPanesController controller(*state.engine);
    if (controller.LoadSequence(
            state.Document().id, state.Document().generation, cells)
        != INKPOD_STATUS_OK) {
        return;
    }
    for (std::uint32_t index = 0U; index < cells.size(); ++index) {
        const std::string& utf8 = cells[index].name;
        if (utf8.empty() || utf8.size() > static_cast<std::size_t>(INT_MAX)) {
            continue;
        }
        const int required = MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            utf8.data(),
            static_cast<int>(utf8.size()),
            nullptr,
            0);
        if (required <= 0) {
            continue;
        }
        std::wstring name;
        try {
            name.resize(static_cast<std::size_t>(required));
        } catch (const std::bad_alloc&) {
            return;
        }
        if (MultiByteToWideChar(
                CP_UTF8,
                MB_ERR_INVALID_CHARS,
                utf8.data(),
                static_cast<int>(utf8.size()),
                name.data(),
                required) != required
            || _wcsicmp(name.c_str(), opened_name.c_str()) != 0) {
            continue;
        }
        InkpodDocumentInfo activated{};
        activated.struct_size = sizeof(activated);
        (void)state.engine->Invoke(
            state.Document().id,
            state.Document().generation,
            [index, &activated](InkpodCore* core) {
                return inkpod_core_sequence_activate(core, index, &activated);
            },
            false,
            false);
        return;
    }
}

InkpodStatus ExportSequenceToPath(
    ApplicationHost& state, const std::wstring& selected_path, bool composite_white) noexcept {
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
                            format,
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

InkpodStatus SaveToPath(ApplicationHost& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::wstring recent_path;
    try {
        recent_path = path;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentIdentity requested_identity{};
    if (!ResolveDocumentFileIdentity(path, requested_identity)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    const auto* conflict = state.Documents().FindByIdentity(requested_identity);
    if (conflict != nullptr && conflict != &state.Document()) {
        state.engine->SetLocalFailure(
            UiText(UiStringId::Text0469));
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(
        state.Document().shell,
        *state.engine,
        state.Document().id,
        state.Document().generation);
    const InkpodStatus status = shell.Save(path);
    if (status == INKPOD_STATUS_OK) {
        DocumentIdentity saved_identity{};
        if (!ResolveDocumentFileIdentity(path, saved_identity)
            || !state.Documents().AssignIdentity(
                state.Document().id, saved_identity)
            || !state.RecordRecentDocument(
                std::move(recent_path), std::move(saved_identity))) {
            state.engine->SetLocalFailure(
                UiText(UiStringId::Text0471));
            return INKPOD_STATUS_INVALID_STATE;
        }
        state.Document().untitled_number = 0U;
        UpdateMenuState(state);
    }
    return status;
}

InkpodStatus SaveDocument(ApplicationHost& state, bool force_dialog) noexcept {
    std::wstring path = state.Document().shell.current_path;
    if (force_dialog || path.empty()) {
        if (!ChooseInkpodPath(state.Workspace().windows.window, true, path)) {
            return INKPOD_STATUS_CANCELLED;
        }
    }
    return SaveToPath(state, path);
}

InkpodStatus WriteCompactedDocumentCopy(
    ApplicationHost& state,
    const CommandContext& context) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodCompactionPlan plan{};
    plan.struct_size = sizeof(plan);
    InkpodStatus status = state.engine->GetCompactionPlan(
        context.document_session.value(), context.generation.value(), plan);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (state.lifetime.smoke_test) {
        return INKPOD_STATUS_CANCELLED;
    }

    std::array<wchar_t, 384U> warning{};
    _snwprintf_s(
        warning.data(),
        warning.size(),
        _TRUNCATE,
        UiText(UiStringId::HistoryCompactionWarningFormat),
        static_cast<unsigned long long>(plan.history_event_count),
        static_cast<unsigned long long>(plan.history_procedure_count));
    if (MessageBoxW(
            state.Workspace().windows.window,
            warning.data(),
            UiText(UiStringId::Text0635),
            MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2)
        != IDOK) {
        return INKPOD_STATUS_CANCELLED;
    }

    std::wstring path;
    if (!ChooseInkpodPath(state.Workspace().windows.window, true, path)) {
        return INKPOD_STATUS_CANCELLED;
    }
    DocumentIdentity target_identity{};
    if (!ResolveDocumentFileIdentity(path, target_identity)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (state.Documents().FindByIdentity(target_identity) != nullptr) {
        state.engine->SetLocalFailure(
            UiText(UiStringId::Text0633));
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<std::uint8_t> path_utf8;
    if (!WidePathToUtf8(path, path_utf8)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    status = state.engine->WriteCompactedCopy(
        context.document_session.value(),
        context.generation.value(),
        std::string_view{
            reinterpret_cast<const char*>(path_utf8.data()),
            path_utf8.size()},
        plan);
    if (status == INKPOD_STATUS_OK) {
        MessageBoxW(
            state.Workspace().windows.window,
            UiText(UiStringId::Text0634),
            UiText(UiStringId::Text0635),
            MB_OK | MB_ICONINFORMATION);
    }
    return status;
}

bool SameSequenceStepPlan(
    const InkpodSequenceStepPlan& left,
    const InkpodSequenceStepPlan& right) noexcept {
    return left.direction == right.direction
        && left.endpoint_policy == right.endpoint_policy
        && left.result_class == right.result_class
        && left.feature_flags == right.feature_flags
        && left.sequence_revision == right.sequence_revision
        && left.source_document_uuid_high == right.source_document_uuid_high
        && left.source_document_uuid_low == right.source_document_uuid_low
        && left.source_generation == right.source_generation
        && left.target_document_uuid_high == right.target_document_uuid_high
        && left.target_document_uuid_low == right.target_document_uuid_low
        && left.target_generation == right.target_generation
        && left.source_index == right.source_index
        && left.target_index == right.target_index
        && left.source_cell_number == right.source_cell_number
        && left.target_cell_number == right.target_cell_number;
}

InkpodStatus SwitchSequenceTarget(
    ApplicationHost& state,
    const CommandContext& context,
    std::optional<std::uint32_t> index,
    InkpodSequenceDirection direction = 0U) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto* document = state.Documents().Find(context.document_session.value());
    if (document == nullptr || document->generation != context.generation.value()
        || document->ActiveView() == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (state.routing.sequence_switch_pending_token.load(
            std::memory_order_acquire) != 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const DocumentViewId previous_view =
        state.routing.targets.ActiveDocumentView();
    const bool target_was_active =
        document->id == state.routing.targets.DocumentSession();
    InkpodDocumentInfo before{};
    before.struct_size = sizeof(before);
    if (!state.engine->GetDocumentInfo(
            document->id, document->generation, before)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodSequenceStepPlan step_plan{};
    const bool has_step_plan = !index.has_value();
    if (has_step_plan) {
        if (direction != INKPOD_SEQUENCE_PREVIOUS
            && direction != INKPOD_SEQUENCE_NEXT) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        step_plan.struct_size = sizeof(step_plan);
        const InkpodSequenceEndpointPolicy endpoint_policy =
            state.lifetime.sequence_endpoint_policy
                == SequenceEndpointPolicy::Wrap
            ? INKPOD_SEQUENCE_ENDPOINT_WRAP
            : INKPOD_SEQUENCE_ENDPOINT_STOP;
        const InkpodStatus resolve_status = state.engine->Invoke(
            document->id,
            document->generation,
            [direction, endpoint_policy, &step_plan](InkpodCore* core) {
                return inkpod_core_sequence_step_resolve(
                    core, direction, endpoint_policy, &step_plan);
            },
            false,
            false);
        if (resolve_status != INKPOD_STATUS_OK) {
            return resolve_status;
        }
        if (step_plan.result_class == INKPOD_SEQUENCE_STEP_EMPTY
            || step_plan.result_class == INKPOD_SEQUENCE_STEP_SINGLE_CELL
            || step_plan.result_class == INKPOD_SEQUENCE_STEP_STOPPED) {
            const wchar_t* message = step_plan.result_class
                    == INKPOD_SEQUENCE_STEP_EMPTY
                ? UiText(UiStringId::Text0961)
                : step_plan.result_class == INKPOD_SEQUENCE_STEP_SINGLE_CELL
                ? UiText(UiStringId::Text0964)
                : UiText(UiStringId::Text0962);
            PresentStatusBarPart(
                state.Workspace().windows.status_bar, 5U, message);
            RefreshSequencePane(state);
            UpdateMenuState(state);
            return INKPOD_STATUS_OK;
        }
        if (step_plan.result_class != INKPOD_SEQUENCE_STEP_ADVANCED
            && step_plan.result_class != INKPOD_SEQUENCE_STEP_WRAPPED) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    if (state.lifetime.sequence_switch_policy
        == SequenceCellSwitchPolicy::AutosaveBeforeSwitch) {
        const std::uint32_t target = index.has_value()
            ? index.value()
            : step_plan.target_index;
        InkpodSequenceSwitchRequest request{};
        request.struct_size = sizeof(request);
        const InkpodStatus request_status = state.engine->Invoke(
            document->id,
            document->generation,
            [target, has_step_plan, step_plan, &request](InkpodCore* core) {
                if (has_step_plan) {
                    InkpodSequenceStepPlan current{};
                    current.struct_size = sizeof(current);
                    const InkpodStatus status = inkpod_core_sequence_step_resolve(
                        core,
                        step_plan.direction,
                        step_plan.endpoint_policy,
                        &current);
                    if (status != INKPOD_STATUS_OK) {
                        return status;
                    }
                    if (!SameSequenceStepPlan(step_plan, current)) {
                        return INKPOD_STATUS_INVALID_STATE;
                    }
                }
                return inkpod_core_sequence_switch_request(
                    core,
                    target,
                    INKPOD_SEQUENCE_SWITCH_AUTOSAVE,
                    &request);
            },
            false,
            false);
        if (request_status != INKPOD_STATUS_OK) {
            return request_status;
        }
        if ((request.flags & INKPOD_SEQUENCE_SWITCH_REQUIRED) == 0U) {
            return INKPOD_STATUS_OK;
        }
        const SequenceAutosaveBinding* target_binding =
            document->FindSequenceAutosave(
                request.target_document_uuid_high,
                request.target_document_uuid_low,
                request.target_source_generation);
        const bool source_dirty =
            (before.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U;
        if (source_dirty || target_binding != nullptr) {
            std::shared_ptr<SequenceSwitchAsyncResult> result;
            try {
                result = std::make_shared<SequenceSwitchAsyncResult>();
                result->context = context;
                result->request = request;
                if (target_binding != nullptr) {
                    result->target_recovery_path = target_binding->recovery_path;
                    result->target_metadata = target_binding->metadata;
                    result->target_restored = true;
                }
                if (source_dirty) {
                    if (!SequenceRecoveryPath(
                            request.source_document_uuid_high,
                            request.source_document_uuid_low,
                            request.source_generation,
                            result->source_recovery_path)
                        || !BuildRecoveryMetadata(
                            document->id,
                            document->generation,
                            document->identity,
                            request.source_document_uuid_high,
                            request.source_document_uuid_low,
                            document->shell.current_path.empty()
                                ? document->shell.recovery_original_path
                                : document->shell.current_path,
                            document->shell.source_path,
                            result->source_metadata)
                        || !document->ReserveSequenceAutosave(
                            request.source_document_uuid_high,
                            request.source_document_uuid_low,
                            request.source_generation)) {
                        return INKPOD_STATUS_INVALID_STATE;
                    }
                }
            } catch (const std::bad_alloc&) {
                return INKPOD_STATUS_INVALID_STATE;
            }
            std::vector<std::uint8_t> source_path_utf8;
            std::vector<std::uint8_t> target_path_utf8;
            if ((source_dirty
                    && !WidePathToUtf8(
                        result->source_recovery_path, source_path_utf8))
                || (result->target_restored
                    && !WidePathToUtf8(
                        result->target_recovery_path, target_path_utf8))) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            result->token = state.routing.tokens.IssueNotification(
                state.routing.targets.CurrentGeneration());
            const HWND completion_window = state.Workspace().windows.window;
            state.routing.sequence_switch_pending_token.store(
                result->token.value, std::memory_order_release);
            state.Workspace().animation.sequence_switch_pending = true;
            PresentStatusBarPart(
                state.Workspace().windows.status_bar,
                5U,
                source_dirty
                    ? UiText(UiStringId::Text0227)
                    : UiText(UiStringId::Text0226));
            UpdateMenuState(state);
            const bool queued = state.engine->Enqueue(
                context,
                [result,
                 source_dirty,
                 source_path_utf8 = std::move(source_path_utf8),
                 target_path_utf8 = std::move(target_path_utf8)](
                    InkpodCore* core) {
                    InkpodDocumentInfo info{};
                    info.struct_size = sizeof(info);
                    if (source_dirty) {
                        InkpodStatus status = inkpod_core_autosave(
                            core,
                            source_path_utf8.data(),
                            source_path_utf8.size(),
                            &info);
                        if (status != INKPOD_STATUS_OK) {
                            return status;
                        }
                        if (!WriteRecoveryMetadata(
                                result->source_recovery_path,
                                result->source_metadata)) {
                            return INKPOD_STATUS_IO_ERROR;
                        }
                        result->source_autosaved = true;
                    }
                    return result->target_restored
                        ? inkpod_core_sequence_restore_autosaved_switch(
                            core,
                            &result->request,
                            target_path_utf8.data(),
                            target_path_utf8.size(),
                            &info)
                        : inkpod_core_sequence_commit_autosaved_switch(
                            core, &result->request, &info);
                },
                true,
                true,
                false,
                [result,
                 completion_window,
                 routing = &state.routing](InkpodStatus status) {
                    result->status = status;
                    {
                        std::lock_guard lock(
                            routing->sequence_switch_results_mutex);
                        routing->sequence_switch_result = result;
                    }
                    if (completion_window == nullptr
                        || PostMessageW(
                            completion_window,
                            kSequenceSwitchCompleted,
                            static_cast<WPARAM>(result->token.value),
                            static_cast<LPARAM>(
                                result->token.generation.Value()))
                            == FALSE) {
                        std::lock_guard lock(
                            routing->sequence_switch_results_mutex);
                        routing->sequence_switch_result.reset();
                        std::uint64_t expected = result->token.value;
                        (void)routing->sequence_switch_pending_token
                            .compare_exchange_strong(
                                expected, 0U, std::memory_order_acq_rel);
                    }
                });
            if (!queued) {
                std::uint64_t expected = result->token.value;
                (void)state.routing.sequence_switch_pending_token
                    .compare_exchange_strong(
                        expected, 0U, std::memory_order_acq_rel);
                state.Workspace().animation.sequence_switch_pending = false;
                UpdateMenuState(state);
                return INKPOD_STATUS_INVALID_STATE;
            }
            return INKPOD_STATUS_OK;
        }
    }
    if ((before.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
        int choice{};
        if (state.lifetime.smoke_test) {
            if (state.lifetime.smoke_dirty_prompt_count != UINT32_MAX) {
                ++state.lifetime.smoke_dirty_prompt_count;
            }
            choice = state.lifetime.smoke_dirty_prompt_choice;
        } else {
            choice = MessageBoxW(
                state.Workspace().sequence_palette != nullptr
                    ? state.Workspace().sequence_palette
                    : state.Workspace().windows.window,
                UiText(UiStringId::Text0784),
                UiText(UiStringId::Text0204),
                MB_OKCANCEL | MB_ICONQUESTION);
        }
        if (choice != IDOK) {
            return INKPOD_STATUS_CANCELLED;
        }
        if (!target_was_active
            && !ActivateDocumentTab(state, document->ActiveView()->id)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const InkpodStatus save_status = SaveDocument(state, false);
        if (save_status != INKPOD_STATUS_OK) {
            if (!target_was_active && previous_view) {
                (void)ActivateDocumentTab(state, previous_view);
            }
            return save_status;
        }
    }
    InkpodDocumentInfo after{};
    after.struct_size = sizeof(after);
    const InkpodStatus status = state.engine->Invoke(
        document->id,
        document->generation,
        [index, step_plan, &after](InkpodCore* core) {
            return index.has_value()
                ? inkpod_core_sequence_activate(core, index.value(), &after)
                : inkpod_core_sequence_step_commit(core, &step_plan, &after);
        },
        true,
        true);
    if (status != INKPOD_STATUS_OK) {
        if (!target_was_active && previous_view) {
            (void)ActivateDocumentTab(state, previous_view);
        }
        RefreshSequencePane(state);
        (void)RefreshSubpalettePane(state);
        UpdateMenuState(state);
        return status;
    }
    if (!target_was_active
        && !ActivateDocumentTab(state, document->ActiveView()->id)) {
        if (previous_view) {
            (void)ActivateDocumentTab(state, previous_view);
        }
        return INKPOD_STATUS_INVALID_STATE;
    }
    const bool changed = before.document_uuid_high != after.document_uuid_high
        || before.document_uuid_low != after.document_uuid_low;
    if (changed) {
        ResetUiForNewActiveDocument(state);
        if (!state.RefreshEditorPresentation(
                document->id, document->generation)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        (void)FitCanvas(state, INKPOD_VIEW_FIT);
    }
    RefreshSequencePane(state);
    (void)RefreshSubpalettePane(state);
    RefreshTreePane(state);
    RefreshLightTablePane(state);
    RefreshColorPanes(state);
    UpdateMenuState(state);
    return INKPOD_STATUS_OK;
}

void DispatchSequencePaneCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        (void)IssueCommand(
            state,
            state->Workspace().windows.window,
            command,
            0,
            state->routing.sequence_pane);
    }
}

void ActivateSequencePaneCell(void* context, std::uint32_t index) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    if (state->Workspace().cut.handle != nullptr) {
        if (index >= state->Workspace().cut.members.size()) {
            return;
        }
        try {
            const std::wstring& descriptor = state->Workspace().cut.current_path;
            const std::size_t slash = descriptor.find_last_of(L"\\/");
            const std::wstring path = (slash == std::wstring::npos
                    ? std::wstring{}
                    : descriptor.substr(0U, slash + 1U))
                + state->Workspace().cut.members[index].relative_path;
            if (OpenDocumentFromPath(*state, path) == INKPOD_STATUS_OK) {
                RefreshSequencePane(*state);
                UpdateMenuState(*state);
            }
        } catch (const std::bad_alloc&) {
        }
        return;
    }
    const PaneActionTarget target = state->routing.pane_targets.CaptureAction(
        state->routing.sequence_pane,
        state->routing.targets.Capture(),
        state->routing.targets);
    if (target.status != PaneTargetStatus::Ok) {
        RefreshSequencePane(*state);
        return;
    }
    const InkpodStatus status = SwitchSequenceTarget(*state, target.context, index);
    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED
        && !state->lifetime.smoke_test) {
        ShowCoreError(
            *state, state->Workspace().sequence_palette, UiText(UiStringId::Text0206));
    }
}

void ReorderCutSequenceCell(
    void* context, std::uint32_t from, std::uint32_t to) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->Workspace().cut.handle == nullptr) {
        return;
    }
    const InkpodStatus status = ReorderCutSequence(*state, from, to);
    if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED
        && !state->lifetime.smoke_test) {
        ShowCoreError(
            *state,
            state->Workspace().sequence_palette,
            UiText(UiStringId::Text0145));
    }
}

void DispatchLightTablePaneCommand(void* context, UINT command) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state != nullptr && state->Workspace().windows.window != nullptr) {
        (void)IssueCommand(
            state,
            state->Workspace().windows.window,
            command,
            0,
            state->routing.light_table_pane);
    }
}

void SelectLightTablePaneEntry(
    void* context,
    bool set_selection,
    std::uint32_t index,
    std::uint64_t stable_id) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr || state->engine == nullptr || stable_id == 0U) {
        return;
    }
    const PaneActionTarget target = state->routing.pane_targets.CaptureAction(
        state->routing.light_table_pane,
        state->routing.targets.Capture(),
        state->routing.targets);
    if (target.status != PaneTargetStatus::Ok
        || !target.context.document_session.has_value()
        || !target.context.generation.has_value()
        || state->Workspace().panes.light_table_selection_session
            != target.context.document_session.value()
        || state->Workspace().panes.light_table_selection_generation
            != target.context.generation.value()) {
        RefreshLightTablePane(*state);
        return;
    }
    if (set_selection) {
        InkpodLightTableEdit edit{};
        edit.struct_size = sizeof(edit);
        edit.operation = INKPOD_LIGHT_TABLE_SET_ACTIVE_OPERATION;
        edit.object_id = stable_id;
        std::uint64_t ignored{};
        const InkpodStatus status = state->engine->Invoke(
            target.context.document_session.value(),
            target.context.generation.value(),
            [edit, &ignored](InkpodCore* core) mutable {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                return inkpod_core_light_table_edit(
                    core, &edit, &result, &ignored);
            },
            true,
            true);
        if (status != INKPOD_STATUS_OK) {
            if (!state->lifetime.smoke_test) {
                ShowCoreError(
                    *state,
                    state->Workspace().light_table_palette,
                    UiText(UiStringId::Text0365));
            }
            RefreshLightTablePane(*state);
            return;
        }
        state->Workspace().panes.active_light_table_set_index = index;
        state->Workspace().panes.active_light_table_set_id = stable_id;
        state->Workspace().panes.active_light_table_item_index = 0U;
        state->Workspace().panes.active_light_table_item_id = 0U;
    } else {
        state->Workspace().panes.active_light_table_item_index = index;
        state->Workspace().panes.active_light_table_item_id = stable_id;
    }
    RefreshLightTablePane(*state);
    UpdateMenuState(*state);
}

InkpodStatus OpenFromPath(ApplicationHost& state, const std::wstring& path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentIdentity identity{};
    std::wstring recent_path;
    try {
        recent_path = path;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!ResolveDocumentFileIdentity(path, identity)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (const auto* existing = state.Documents().FindByIdentity(identity);
        existing != nullptr && existing->ActiveView() != nullptr) {
        if (!ActivateDocumentTab(state, existing->ActiveView()->id)
            || !state.RecordRecentDocument(
                std::move(recent_path), std::move(identity))) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        UpdateMenuState(state);
        return INKPOD_STATUS_OK;
    }
    DocumentViewId previous_view{};
    std::optional<ApplicationHost::DocumentBinding> added;
    if (!BeginNewDocumentTab(state, previous_view, added)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(
        state.Document().shell,
        *state.engine,
        state.Document().id,
        state.Document().generation);
    const InkpodStatus status = shell.Open(path);
    if (status != INKPOD_STATUS_OK) {
        RollbackNewDocumentTab(state, added, previous_view);
        return status;
    }
    if (!state.Documents().AssignIdentity(state.Document().id, identity)) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.Document().untitled_number = 0U;
    state.ActiveView().presentation.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetUiForNewActiveDocument(state);
    if (!state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus view_status = FitCanvas(state, INKPOD_VIEW_FIT);
    if (view_status != INKPOD_STATUS_OK) {
        RollbackNewDocumentTab(state, added, previous_view);
    } else if (!state.RecordRecentDocument(
                   std::move(recent_path), std::move(identity))) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    UpdateMenuState(state);
    return view_status;
}

InkpodStatus OpenRecoveryWithIdentity(
    ApplicationHost& state,
    const std::wstring& path,
    const DocumentIdentity* original_identity,
    const std::wstring* original_path) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentIdentity identity{};
    if (original_identity != nullptr && *original_identity) {
        try {
            identity = *original_identity;
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
    } else if (!ResolveDocumentFileIdentity(path, identity)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (const auto* existing = state.Documents().FindByIdentity(identity);
        existing != nullptr && existing->ActiveView() != nullptr) {
        return ActivateDocumentTab(state, existing->ActiveView()->id)
            ? INKPOD_STATUS_OK
            : INKPOD_STATUS_INVALID_STATE;
    }
    DocumentViewId previous_view{};
    std::optional<ApplicationHost::DocumentBinding> added;
    if (!BeginNewDocumentTab(state, previous_view, added)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    DocumentShellController shell(
        state.Document().shell,
        *state.engine,
        state.Document().id,
        state.Document().generation);
    const InkpodStatus status = shell.OpenRecovery(path);
    if (status != INKPOD_STATUS_OK) {
        RollbackNewDocumentTab(state, added, previous_view);
        return status;
    }
    if (!state.Documents().AssignIdentity(state.Document().id, identity)) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (original_path != nullptr) {
        try {
            state.Document().shell.recovery_original_path = *original_path;
        } catch (const std::bad_alloc&) {
            RollbackNewDocumentTab(state, added, previous_view);
            return INKPOD_STATUS_INVALID_STATE;
        }
    }
    state.Document().untitled_number = 0U;
    state.ActiveView().presentation.color_check_mode = INKPOD_COLOR_CHECK_OFF;
    ResetUiForNewActiveDocument(state);
    if (!state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)) {
        RollbackNewDocumentTab(state, added, previous_view);
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus view_status = FitCanvas(state, INKPOD_VIEW_FIT);
    if (view_status != INKPOD_STATUS_OK) {
        RollbackNewDocumentTab(state, added, previous_view);
    }
    UpdateMenuState(state);
    return view_status;
}

InkpodStatus OpenRecoveryFromPathImpl(
    ApplicationHost& state, const std::wstring& path) noexcept {
    inkpod::app::RecoveryMetadata metadata{};
    if (inkpod::app::ReadRecoveryMetadata(path, metadata)) {
        const DocumentIdentity* identity = metadata.original_identity
            ? &metadata.original_identity
            : nullptr;
        return OpenRecoveryWithIdentity(
            state, path, identity, &metadata.original_path);
    }
    return OpenRecoveryWithIdentity(state, path, nullptr, nullptr);
}

InkpodStatus OpenRecoveryCandidateImpl(
    ApplicationHost& state,
    const inkpod::app::RecoveryCandidate& candidate) noexcept {
    const DocumentIdentity* identity = candidate.has_metadata
            && candidate.metadata.original_identity
        ? &candidate.metadata.original_identity
        : nullptr;
    const std::wstring* original_path = candidate.has_metadata
        ? &candidate.metadata.original_path
        : nullptr;
    if (identity != nullptr
        && state.Documents().FindByIdentity(*identity) != nullptr) {
        if (state.engine != nullptr) {
            state.engine->SetLocalFailure(
                UiText(UiStringId::Text0071));
        }
        return INKPOD_STATUS_INVALID_STATE;
    }
    return OpenRecoveryWithIdentity(
        state, candidate.recovery_path, identity, original_path);
}

InkpodStatus OpenDocumentFromPathImpl(
    ApplicationHost& state, const std::wstring& path) noexcept {
    if (CommonRasterFormatFromPath(path) != 0U) {
        const InkpodStatus status = ImportCommonRasterFromPath(state, path);
        if (status == INKPOD_STATUS_OK) {
            UpdateMenuState(state);
        }
        return status;
    }
    if (IsCutDescriptor(path)) {
        return OpenCutDescriptor(state, path);
    }
    DocumentIdentity identity{};
    std::wstring recent_path;
    try {
        recent_path = path;
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!ResolveDocumentFileIdentity(path, identity)) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    if (const auto* existing = state.Documents().FindByIdentity(identity);
        existing != nullptr && existing->ActiveView() != nullptr) {
        if (!ActivateDocumentTab(state, existing->ActiveView()->id)
            || !state.RecordRecentDocument(
                std::move(recent_path), std::move(identity))) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        UpdateMenuState(state);
        return INKPOD_STATUS_OK;
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
        state.Workspace().windows.window,
        UiText(UiStringId::NewerRecoveryPrompt),
        L"inkpod Recovery",
        MB_YESNOCANCEL | MB_ICONQUESTION);
    if (choice == IDYES) {
        return OpenRecoveryFromPath(state, recovery);
    }
    if (choice == IDNO) {
        (void)DiscardRecoveryArtifact(recovery);
    }
    return OpenFromPath(state, path);
}

bool QueueAutosave(
    ApplicationHost& state,
    const CommandContext& context,
    const std::wstring& path) noexcept {
    if (state.engine == nullptr || !context.document_session.has_value()
        || !context.generation.has_value()) {
        return false;
    }
    DocumentSession* document = state.Documents().Find(
        context.document_session.value());
    InkpodDocumentInfo info = EmptyDocumentInfo();
    RecoveryMetadata metadata{};
    if (document == nullptr
        || document->generation != context.generation.value()
        || !state.engine->GetDocumentInfo(
            document->id, document->generation, info)
        || !BuildRecoveryMetadata(
            document->id,
            document->generation,
            document->identity,
            info.document_uuid_high,
            info.document_uuid_low,
            document->shell.current_path.empty()
                ? document->shell.recovery_original_path
                : document->shell.current_path,
            document->shell.source_path,
            metadata)) {
        return false;
    }
    DocumentShellController shell(
        document->shell,
        *state.engine,
        document->id,
        document->generation);
    return shell.QueueAutosave(context, path, metadata);
}

InkpodStatus ApplyFillAtDeviceRange(
    ApplicationHost& state,
    float device_x,
    float device_y,
    float end_device_x,
    float end_device_y,
    bool has_range,
    const InkpodEditorStateInfo* editor) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)
        || info.width == 0U || info.height == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const double zoom = (bounds.right - bounds.left) / static_cast<double>(info.width);
    if (!std::isfinite(zoom) || zoom <= 0.0) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto to_document_x = [&](double value) {
        double result = (value - bounds.left) / zoom;
        if (state.ActiveView().presentation.flip_horizontal) {
            result = static_cast<double>(info.width) - result;
        }
        return result;
    };
    auto to_document_y = [&](double value) {
        double result = (value - bounds.top) / zoom;
        if (state.ActiveView().presentation.flip_vertical) {
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
    if (editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_layer_id == 0U || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodFillInput input{};
    input.struct_size = sizeof(input);
    input.operation = editor->fill.operation;
    input.flags = ((editor->fill.flags & INKPOD_EDITOR_FILL_OVERFLOW_ABORT) != 0U
            ? INKPOD_FILL_FLAG_OVERFLOW_ABORT
            : 0U)
        | ((editor->fill.flags & INKPOD_EDITOR_FILL_DETACHED_REGIONS) != 0U
                ? INKPOD_FILL_FLAG_DETACHED_REGIONS
                : 0U)
        | ((editor->fill.flags & INKPOD_EDITOR_FILL_TRANSPARENT_ONLY) != 0U
                ? INKPOD_FILL_FLAG_TRANSPARENT_ONLY
                : 0U)
        | ((editor->fill.flags & INKPOD_EDITOR_FILL_DOCUMENT_SELECTION) != 0U
                ? INKPOD_FILL_FLAG_DOCUMENT_SELECTION
                : 0U)
        | ((editor->fill.flags & INKPOD_EDITOR_FILL_LIGHT_TABLE_BOUNDARY) != 0U
                ? INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY
                : 0U)
        | ((editor->fill.flags & INKPOD_EDITOR_FILL_LIGHT_TABLE_COLOR) != 0U
                ? INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR
                : 0U);
    input.seed_x = static_cast<std::uint32_t>(std::floor(document_x));
    input.seed_y = static_cast<std::uint32_t>(std::floor(document_y));
    input.color = editor->current_color;
    input.color.struct_size = sizeof(InkpodColorValue);
    input.tolerance = editor->fill.tolerance;
    input.gap_close = editor->fill.gap_close;
    input.inclusion_mode = editor->fill.inclusion_mode;
    input.extension_distance = editor->fill.extension_distance;
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
        inclusion_colors.assign(
            std::begin(editor->fill.inclusion_colors),
            std::begin(editor->fill.inclusion_colors)
                + std::min<std::uint32_t>(
                    editor->fill.inclusion_color_count,
                    INKPOD_EDITOR_MAX_INCLUSION_COLORS));
        for (InkpodColorValue& color : inclusion_colors) {
            color.struct_size = sizeof(InkpodColorValue);
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
        controller.Apply(
            editor->active_layer_id,
            editor->active_plane_id,
            input,
            inclusion_colors,
            fill_result);
    if (status == INKPOD_STATUS_OK && fill_result.changed_pixel_count != 0U) {
        (void)state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation);
        RefreshTreePane(state);
    }
    if (status == INKPOD_STATUS_FILL_OVERFLOW && !state.lifetime.smoke_test
        && (fill_result.flags & INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE) != 0U) {
        std::array<wchar_t, 160U> message{};
        _snwprintf_s(
            message.data(),
            message.size(),
            _TRUNCATE,
            UiText(UiStringId::Text0597),
            fill_result.leak_x,
            fill_result.leak_y);
        MessageBoxW(state.Workspace().windows.window, message.data(), L"inkpod", MB_OK | MB_ICONWARNING);
    }
    return status;
}

InkpodStatus ApplyFillAtDevicePoint(
    ApplicationHost& state,
    float device_x,
    float device_y,
    const InkpodEditorStateInfo* editor) noexcept {
    return ApplyFillAtDeviceRange(
        state, device_x, device_y, device_x, device_y, false, editor);
}

std::optional<InkpodScopedColorReplaceMode> ColorReplaceModeForPlane(
    std::uint32_t kind) noexcept {
    switch (kind) {
        case INKPOD_TYPED_PLANE_MAIN_LINE:
            return INKPOD_COLOR_REPLACE_RASTER_MAIN_LINE;
        case INKPOD_TYPED_PLANE_COLOR:
        case INKPOD_TYPED_PLANE_RASTER:
            return INKPOD_COLOR_REPLACE_RASTER_COLOR;
        default:
            return std::nullopt;
    }
}

InkpodStatus ApplyColorReplace(
    ApplicationHost& state,
    const InkpodEditorStateInfo* editor,
    bool has_region,
    const std::vector<InkpodStrokeSample>& samples) noexcept {
    InkpodDocumentInfo info{};
    if (state.engine == nullptr || editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_plane_id == 0U || !QueryDocument(state, info)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodScopedColorReplaceInput input{};
    input.struct_size = sizeof(input);
    input.mode = state.Workspace().tools.color_replace_mode;
    input.plane_id = editor->active_plane_id;
    input.base_document_revision = has_region
        ? state.Workspace().tools.color_replace_base_revision
        : info.document_revision;
    input.target_color = state.Workspace().tools.color_replace_target;
    input.target_color.struct_size = sizeof(InkpodColorValue);
    input.replacement_color = editor->current_color;
    input.replacement_color.struct_size = sizeof(InkpodColorValue);

    std::vector<InkpodSelectionPoint> points;
    if (has_region) {
        inkpod::renderer::CanvasDocumentBounds bounds{};
        if (samples.empty() || info.width == 0U || info.height == 0U
            || !inkpod::renderer::GetCanvasDocumentBounds(
                state.Workspace().windows.canvas, bounds)) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const double zoom =
            (bounds.right - bounds.left) / static_cast<double>(info.width);
        if (!std::isfinite(zoom) || zoom <= 0.0) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        const auto document_point = [&](const InkpodStrokeSample& sample) {
            double x = (static_cast<double>(sample.x) - bounds.left) / zoom;
            double y = (static_cast<double>(sample.y) - bounds.top) / zoom;
            if (state.ActiveView().presentation.flip_horizontal) {
                x = static_cast<double>(info.width) - x;
            }
            if (state.ActiveView().presentation.flip_vertical) {
                y = static_cast<double>(info.height) - y;
            }
            return InkpodSelectionPoint{
                sizeof(InkpodSelectionPoint),
                0U,
                static_cast<float>(std::clamp(
                    x, 0.0, static_cast<double>(info.width))),
                static_cast<float>(std::clamp(
                    y, 0.0, static_cast<double>(info.height))),
                std::clamp(sample.pressure, 0.0F, 1.0F),
                0U};
        };
        try {
            if (state.Workspace().tools.color_replace_shape
                == INKPOD_SELECTION_RECTANGLE) {
                if (samples.size() < 2U) {
                    return INKPOD_STATUS_INVALID_ARGUMENT;
                }
                points.reserve(2U);
                points.push_back(document_point(samples.front()));
                points.push_back(document_point(samples.back()));
            } else {
                points.reserve(samples.size());
                for (const auto& sample : samples) {
                    points.push_back(document_point(sample));
                }
            }
        } catch (const std::bad_alloc&) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        input.feature_flags = INKPOD_COLOR_REPLACE_HAS_REGION;
        input.shape = state.Workspace().tools.color_replace_shape;
        if (input.shape == INKPOD_SELECTION_RECTANGLE) {
            const double left = std::floor(std::min(
                static_cast<double>(points.front().x),
                static_cast<double>(points.back().x)));
            const double top = std::floor(std::min(
                static_cast<double>(points.front().y),
                static_cast<double>(points.back().y)));
            const double right = std::ceil(std::max(
                static_cast<double>(points.front().x),
                static_cast<double>(points.back().x)));
            const double bottom = std::ceil(std::max(
                static_cast<double>(points.front().y),
                static_cast<double>(points.back().y)));
            if (!(right > left) || !(bottom > top)) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            input.bounds = InkpodFrameRect{
                static_cast<std::int32_t>(left),
                static_cast<std::int32_t>(top),
                static_cast<std::int32_t>(right - left),
                static_cast<std::int32_t>(bottom - top)};
            points.clear();
        } else {
            if ((input.shape == INKPOD_SELECTION_LASSO
                    || input.shape == INKPOD_SELECTION_POLYLINE)
                && points.size() < 3U) {
                return INKPOD_STATUS_INVALID_ARGUMENT;
            }
            input.point_count = points.size();
            input.point_stride_bytes = sizeof(InkpodSelectionPoint);
            input.diameter = input.shape == INKPOD_SELECTION_TRACE
                ? state.Workspace().tools.color_replace_diameter
                : 0.0F;
        }
    }
    InkpodDispatchResult result{};
    result.struct_size = sizeof(result);
    inkpod::windows::ui::tools::ColorReplaceController controller(*state.engine);
    const InkpodStatus status = controller.Apply(input, points, result);
    if (status == INKPOD_STATUS_OK) {
        (void)state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation);
        RefreshTreePane(state);
    }
    return status;
}

InkpodStatus ApplySelectionGesture(
    ApplicationHost& state,
    const std::vector<InkpodStrokeSample>& samples,
    const InkpodEditorStateInfo* editor) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || editor->active_layer_id == 0U || editor->active_plane_id == 0U
        || samples.empty() || !QueryDocument(state, info)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)
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
        if (state.ActiveView().presentation.flip_horizontal) {
            x = static_cast<double>(info.width) - x;
        }
        if (state.ActiveView().presentation.flip_vertical) {
            y = static_cast<double>(info.height) - y;
        }
        return InkpodSelectionPoint{
            sizeof(InkpodSelectionPoint),
            0U,
            static_cast<float>(std::clamp(x, 0.0, static_cast<double>(info.width))),
            static_cast<float>(std::clamp(y, 0.0, static_cast<double>(info.height))),
            std::clamp(sample.pressure, 0.0F, 1.0F),
            0U};
    };
    std::vector<InkpodSelectionPoint> points;
    try {
        if (editor->selection.shape == INKPOD_SELECTION_RECTANGLE
            || editor->selection.shape == INKPOD_SELECTION_ELLIPSE) {
            if (samples.size() >= 2U) {
                points.reserve(2U);
                points.push_back(document_point(samples.front()));
                points.push_back(document_point(samples.back()));
            }
        } else if (editor->selection.shape == INKPOD_SELECTION_LASSO
            || editor->selection.shape == INKPOD_SELECTION_POLYLINE
            || editor->selection.shape == INKPOD_SELECTION_TRACE) {
            points.reserve(samples.size());
            for (const auto& sample : samples) {
                points.push_back(document_point(sample));
            }
        }
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if ((editor->selection.shape == INKPOD_SELECTION_LASSO
            || editor->selection.shape == INKPOD_SELECTION_POLYLINE)
        && points.size() < 3U) {
        if (state.engine == nullptr) {
            return INKPOD_STATUS_INVALID_STATE;
        }
        SelectionController controller(*state.engine);
        return controller.ApplyEmpty(editor->selection.operation);
    }
    InkpodSelectionInput input{};
    input.struct_size = sizeof(input);
    input.shape = editor->selection.shape;
    input.operation = editor->selection.operation;
    input.tolerance = editor->selection.tolerance;
    input.gap_close = editor->selection.gap_close;
    input.diameter = static_cast<float>(
        static_cast<double>(editor->selection.diameter_q16) / 65536.0);
    input.interpretation = editor->selection.interpretation;
    input.aspect_ratio_q16 = editor->selection.aspect_ratio_q16;
    input.construction_flags = editor->selection.construction_flags;
    input.rotation_turns = editor->selection.rotation_turns;
    input.trace_shape = editor->selection.trace_shape;
    input.view_zoom_q16 = FloatToQ16(static_cast<float>(zoom));
    if (editor->selection.shape == INKPOD_SELECTION_RECTANGLE
        || editor->selection.shape == INKPOD_SELECTION_ELLIPSE) {
        if (points.size() != 2U) {
            return INKPOD_STATUS_INVALID_ARGUMENT;
        }
        input.points = points.data();
        input.point_count = points.size();
        input.point_stride_bytes = sizeof(InkpodSelectionPoint);
    } else if (editor->selection.shape == INKPOD_SELECTION_WAND) {
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
    return controller.Apply(
        editor->active_layer_id, editor->active_plane_id, input, points);
}

InkpodStatus SelectDrawingColor(
    ApplicationHost& state, bool different, InkpodSelectionOperation operation) noexcept {
    if (state.engine == nullptr
        || !state.RefreshEditorPresentation(
            state.Document().id, state.Document().generation)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodEditorStateInfo* editor = PresentedEditorState(state);
    if (editor == nullptr
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
        || (editor->flags & INKPOD_EDITOR_STATE_HAS_CURRENT_COLOR) == 0U
        || editor->active_layer_id == 0U || editor->active_plane_id == 0U) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    InkpodColorValue color{};
    color.struct_size = sizeof(color);
    if (state.Workspace().tools.active_plane == INKPOD_PLANE_MAIN_LINE) {
        color.depth = INKPOD_COLOR_DEPTH_BINARY;
        color.red = editor->current_color.red == 0U
                && editor->current_color.green == 0U
                && editor->current_color.blue == 0U
                && editor->current_color.alpha == 0U
            ? 0U
            : UINT8_MAX;
    } else {
        color = editor->current_color;
        color.struct_size = sizeof(InkpodColorValue);
    }
    SelectionController controller(*state.engine);
    return controller.SelectColor(
        editor->active_layer_id,
        editor->active_plane_id,
        color,
        different,
        operation);
}

InkpodStatus EyedropAtDevicePoint(ApplicationHost& state, float device_x, float device_y) noexcept {
    InkpodDocumentInfo info{};
    inkpod::renderer::CanvasDocumentBounds bounds{};
    if (!QueryDocument(state, info)
        || !inkpod::renderer::GetCanvasDocumentBounds(
               state.Workspace().windows.canvas, bounds)
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
              [x, y, source = state.Workspace().tools.eyedropper_source, &sampled](InkpodCore* core) {
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

bool DiscardCurrentRecovery(ApplicationHost& state) noexcept {
    if (state.Document().shell.recovery_path.empty()) {
        return true;
    }
    if (state.engine != nullptr && state.engine->WaitIdle() != INKPOD_STATUS_OK) {
        return false;
    }
    if (!DiscardRecoveryArtifact(state.Document().shell.recovery_path)) {
        return false;
    }
    state.Document().shell.recovery_path.clear();
    return true;
}

bool ConfirmDiscard(ApplicationHost& state) noexcept {
    InkpodDocumentInfo info{};
    if (!QueryDocument(state, info)) {
        return true;
    }
    if ((info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) == 0U) {
        return true;
    }
    int choice{};
    if (state.lifetime.smoke_test) {
        if (state.lifetime.smoke_dirty_prompt_count != UINT32_MAX) {
            ++state.lifetime.smoke_dirty_prompt_count;
        }
        choice = state.lifetime.smoke_dirty_prompt_choice;
    } else {
        choice = MessageBoxW(
            state.Workspace().windows.window,
            UiText(UiStringId::Text0615),
            L"inkpod",
            MB_YESNOCANCEL | MB_ICONQUESTION);
    }
    if (choice == IDCANCEL) {
        return false;
    }
    if (choice == IDYES) {
        const InkpodStatus status = SaveDocument(state, false);
        if (status != INKPOD_STATUS_OK) {
            if (status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(state, state.Workspace().windows.window, UiText(UiStringId::Save));
            }
            return false;
        }
    } else if (!DiscardCurrentRecovery(state)) {
        ShowCoreError(state, state.Workspace().windows.window, UiText(UiStringId::Text0072));
        return false;
    }
    return true;
}

bool CloseActiveDocument(ApplicationHost& state) noexcept {
    if (!ConfirmDiscard(state)) {
        return false;
    }
    const DocumentSessionId closing = state.Document().id;
    DocumentViewId replacement{};
    const auto* active_group = state.Workspace().editors.Active();
    if (active_group != nullptr) {
        for (std::size_t index = 0U;
             index < active_group->ViewCount();
             ++index) {
            const DocumentViewId view = active_group->ViewAt(index);
            const DocumentSession* candidate = state.Documents().FindByView(view);
            if (candidate != nullptr && candidate->id != closing) {
                replacement = view;
                break;
            }
        }
    }
    if (!replacement) {
        for (std::size_t index = 0U; index < state.Documents().Count(); ++index) {
            const auto* candidate = state.Documents().SessionAt(index);
            if (candidate != nullptr && candidate->id != closing
                && candidate->ActiveView() != nullptr) {
                replacement = candidate->ActiveView()->id;
                break;
            }
        }
    }
    if (!state.CloseDocumentSession(closing)) {
        return false;
    }
    const bool ready = replacement
        ? ActivateDocumentTab(state, replacement)
        : CreateDefaultCellImpl(state) == INKPOD_STATUS_OK;
    UpdateMenuState(state);
    return ready;
}

bool CloseActiveView(ApplicationHost& state) noexcept {
    const DocumentViewId closing = state.routing.targets.ActiveDocumentView();
    if (!closing) {
        return false;
    }
    if (state.Document().ViewCount() == 1U) {
        return CloseActiveDocument(state);
    }
    const bool closed = state.CloseDocumentView(closing);
    if (closed) {
        UpdateMenuState(state);
    }
    return closed;
}

bool ConfirmAllDocuments(ApplicationHost& state) noexcept {
    std::array<DocumentSessionId, inkpod::app::DocumentRegistry::kMaximumSessions>
        sessions{};
    const std::size_t count = state.Documents().Count();
    for (std::size_t index = 0U; index < count; ++index) {
        const auto* document = state.Documents().SessionAt(index);
        if (document != nullptr) {
            sessions[index] = document->id;
        }
    }
    for (std::size_t index = 0U; index < count; ++index) {
        const auto* document = state.Documents().Find(sessions[index]);
        if (document == nullptr || document->ActiveView() == nullptr) {
            continue;
        }
        if (!ActivateDocumentTab(state, document->ActiveView()->id)
            || !ConfirmDiscard(state)) {
            return false;
        }
    }
    return true;
}

bool RegisterWorkspaceSnapshotSinks(
    ApplicationHost& state,
    WorkspaceWindow& workspace) noexcept {
    if (state.engine == nullptr
        || workspace.editors.GroupCount() == 0U) {
        return false;
    }
    std::array<renderer::CanvasSnapshotSink*,
               inkpod::app::EditorArea::kMaximumGroups>
        registered{};
    std::size_t registered_count{};
    for (std::size_t index = 0U;
         index < workspace.editors.GroupCount(); ++index) {
        const EditorGroup* group = workspace.editors.GroupAt(index);
        renderer::CanvasSnapshotSink* sink = group == nullptr
            ? nullptr
            : renderer::GetCanvasSnapshotSink(group->canvas);
        if (sink == nullptr || !state.engine->RegisterSnapshotSink(sink)) {
            while (registered_count > 0U) {
                --registered_count;
                (void)state.engine->UnregisterSnapshotSink(
                    registered[registered_count]);
            }
            return false;
        }
        registered[registered_count++] = sink;
    }
    return true;
}

bool UnregisterWorkspaceSnapshotSinks(
    ApplicationHost& state,
    WorkspaceWindow& workspace) noexcept {
    if (state.engine == nullptr) {
        return true;
    }
    const std::size_t group_count = workspace.editors.GroupCount();
    if (group_count == 0U) {
        return false;
    }
    std::array<renderer::CanvasSnapshotSink*,
               inkpod::app::EditorArea::kMaximumGroups>
        sinks{};
    for (std::size_t index = 0U; index < group_count; ++index) {
        const EditorGroup* group = workspace.editors.GroupAt(index);
        renderer::CanvasSnapshotSink* sink = group == nullptr
            ? nullptr
            : renderer::GetCanvasSnapshotSink(group->canvas);
        if (sink == nullptr
            || std::find(sinks.cbegin(), sinks.cbegin() + index, sink)
                != sinks.cbegin() + index) {
            return false;
        }
        sinks[index] = sink;
    }
    for (std::size_t index = 0U; index < group_count; ++index) {
        const EditorGroup* group = workspace.editors.GroupAt(index);
        renderer::CancelCanvasStroke(group->canvas);
    }
    return state.engine->UnregisterSnapshotSinks(sinks.data(), group_count);
}

bool RetargetCoreNotificationsBeforeWorkspaceClose(
    ApplicationHost& state,
    WorkspaceWindowId closing,
    HWND closing_window) noexcept {
    if (state.engine == nullptr) {
        return false;
    }
    for (std::size_t index = 0U; index < state.Workspaces().Count(); ++index) {
        const WorkspaceWindow* candidate = state.Workspaces().At(index);
        if (candidate != nullptr && candidate->id != closing
            && candidate->windows.window != nullptr) {
            return state.engine->RetargetNotificationOwner(
                closing_window, candidate->windows.window);
        }
    }
    return false;
}

bool CloseWorkspaceWindow(ApplicationHost& state, HWND window) noexcept {
    inkpod::app::WorkspaceWindow* closing = state.WorkspaceForWindow(window);
    if (closing == nullptr || closing->windows.window != window
        || state.Workspaces().Count() <= 1U) {
        return false;
    }
    std::array<DocumentSessionId, inkpod::app::DocumentRegistry::kMaximumSessions>
        sessions_to_close{};
    std::size_t session_count{};
    std::array<DocumentViewId, 128U> views_to_close{};
    std::size_t view_count{};
    for (std::size_t group_index = 0U;
         group_index < closing->editors.GroupCount(); ++group_index) {
        const auto* group = closing->editors.GroupAt(group_index);
        for (std::size_t index = 0U;
             group != nullptr && index < group->ViewCount(); ++index) {
            const DocumentViewId view = group->ViewAt(index);
            if (view_count < views_to_close.size()) {
                views_to_close[view_count++] = view;
            }
            const DocumentSession* document = state.Documents().FindByView(view);
            if (document == nullptr) {
                continue;
            }
            bool outside{};
            for (std::size_t document_view_index = 0U;
                 document_view_index < document->ViewCount();
                 ++document_view_index) {
                const inkpod::app::DocumentView* candidate =
                    document->ViewAt(document_view_index);
                if (candidate != nullptr
                    && state.routing.targets.WorkspaceForView(candidate->id)
                        != closing->id) {
                    outside = true;
                    break;
                }
            }
            if (!outside
                && std::find(
                    sessions_to_close.begin(),
                    sessions_to_close.begin()
                        + static_cast<std::ptrdiff_t>(session_count),
                    document->id)
                    == sessions_to_close.begin()
                        + static_cast<std::ptrdiff_t>(session_count)
                && session_count < sessions_to_close.size()) {
                sessions_to_close[session_count++] = document->id;
            }
        }
    }
    for (std::size_t index = 0U; index < session_count; ++index) {
        DocumentSession* document = state.Documents().Find(sessions_to_close[index]);
        if (document == nullptr || document->ActiveView() == nullptr) {
            continue;
        }
        if (!state.ActivateDocumentView(document->ActiveView()->id)
            || !ConfirmDiscard(state)) {
            (void)state.ActivateWorkspaceWindow(closing->id, true);
            return false;
        }
    }
    if (!RetargetCoreNotificationsBeforeWorkspaceClose(
            state, closing->id, closing->windows.window)
        || !UnregisterWorkspaceSnapshotSinks(state, *closing)) {
        return false;
    }
    for (std::size_t index = 0U; index < session_count; ++index) {
        if (state.Documents().Find(sessions_to_close[index]) != nullptr
            && !state.CloseDocumentSession(sessions_to_close[index])) {
            (void)RegisterWorkspaceSnapshotSinks(state, *closing);
            return false;
        }
    }
    for (std::size_t index = 0U; index < view_count; ++index) {
        DocumentSession* document = state.Documents().FindByView(
            views_to_close[index]);
        if (document != nullptr && document->ViewCount() > 1U
            && !state.CloseDocumentView(views_to_close[index])) {
            (void)RegisterWorkspaceSnapshotSinks(state, *closing);
            return false;
        }
    }

    (void)state.ActivateWorkspaceWindow(closing->id, false);
    CaptureWorkspacePresentation(state);
    if (!state.lifetime.smoke_test) {
        const auto session_name = WorkspaceRegistryValueName(
            L"WorkspaceSessionV5", closing->persistence_slot);
        (void)SaveWorkspaceLayout(
            closing->windows.workspace, session_name.data());
    }
    if (closing->subpalette_dialog.canvas != nullptr) {
        (void)renderer::UnbindCanvasSnapshotSink(
            closing->subpalette_dialog.canvas);
    }
    if (state.engine != nullptr && closing->subpalette_core_view_id != 0U
        && closing->subpalette_session
        && closing->subpalette_document_generation) {
        const std::uint64_t core_view_id = closing->subpalette_core_view_id;
        (void)state.engine->Invoke(
            closing->subpalette_session,
            closing->subpalette_document_generation,
            [core_view_id](InkpodCore* core) {
                return inkpod_core_view_close(core, core_view_id);
            },
            false,
            false);
        closing->subpalette_core_view_id = 0U;
    }
    if (closing->subpalette_canvas_id) {
        (void)state.routing.targets.UnregisterAuxiliaryCanvas(
            closing->subpalette_canvas_id);
        closing->subpalette_canvas_id = {};
    }
    const WorkspaceWindowId closing_id = closing->id;
    if (DestroyWindow(window) == FALSE) {
        (void)RegisterWorkspaceSnapshotSinks(state, *closing);
        return false;
    }
    closing->windows.window = nullptr;
    if (!state.RemoveWorkspaceWindow(closing_id)) {
        return false;
    }
    inkpod::app::WorkspaceWindow* remaining = state.Workspaces().LastFocused();
    if (remaining == nullptr) {
        remaining = state.Workspaces().Current();
    }
    if (remaining != nullptr) {
        (void)state.ActivateWorkspaceWindow(remaining->id, true);
        UpdateMenuState(state);
        SetForegroundWindow(remaining->windows.window);
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

const wchar_t* BatchOperationLabel(UINT command) noexcept {
    for (const auto& entry : inkpod::windows::ui::BatchPaletteEntries()) {
        if (entry.command == command) {
            return entry.label;
        }
    }
    return UiText(UiStringId::Text0269);
}

bool AddBatchOperation(ApplicationHost& state, UINT command) noexcept {
    BatchOperationUi operation{};
    operation.label = BatchOperationLabel(command);
    operation.layer_kind = INKPOD_LAYER_BINARY_COLORING;
    operation.plane_kind = INKPOD_TYPED_PLANE_COLOR;
    try {
        switch (command) {
            case IDM_BATCH_ADD_COLOR_REPLACE: {
                operation.kind = INKPOD_BATCH_OPERATION_COLOR_REPLACE;
                InkpodBatchColorPairInput pair{};
                pair.struct_size = sizeof(pair);
                pair.enabled = 1U;
                pair.old_color = BatchTransparentColor();
                pair.new_color = state.Workspace().tools.drawing_color;
                operation.color_pairs.push_back(pair);
                break;
            }
            case IDM_BATCH_ADD_MOVE_TO_COLOR_PLANE:
                operation.kind = INKPOD_BATCH_OPERATION_MOVE_TO_COLOR_PLANE;
                operation.plane_kind = INKPOD_TYPED_PLANE_MAIN_LINE;
                operation.colors.push_back(state.Workspace().tools.drawing_color);
                break;
            case IDM_BATCH_ADD_MASKING:
                operation.kind = INKPOD_BATCH_OPERATION_MASKING;
                operation.colors.push_back(state.Workspace().tools.drawing_color);
                break;
            case IDM_BATCH_ADD_ERASE:
                operation.kind = INKPOD_BATCH_OPERATION_ERASE;
                operation.colors.push_back(state.Workspace().tools.drawing_color);
                break;
            default:
                return false;
        }
        ResetBatchDerivedState(state.batch);
        state.batch.operations.push_back(std::move(operation));
        state.batch.selected_operation =
            static_cast<std::uint32_t>(state.batch.operations.size() - 1U);
        state.batch.selected_stage = state.batch.selected_operation + 1U;
        RefreshBatchPalette(state.batch, state.Workspace().batch_palette);
        return true;
    } catch (const std::bad_alloc&) {
        return false;
    }
}

bool PrepareBatchRunOperations(ApplicationHost&) noexcept {
    return true;
}

bool SameBatchColor(
    const InkpodColorValue& left, const InkpodColorValue& right) noexcept {
    return left.depth == right.depth && left.red == right.red
        && left.green == right.green && left.blue == right.blue
        && left.alpha == right.alpha;
}

bool ExtractBatchColorPairs(
    ApplicationHost& state,
    const CommandContext& issued_context) noexcept {
    if (state.engine == nullptr
        || state.batch.selected_stage == 0U
        || state.batch.selected_stage != state.batch.selected_operation + 1U
        || state.batch.selected_operation >= state.batch.operations.size()
        || state.batch.operations[state.batch.selected_operation].kind
            != INKPOD_BATCH_OPERATION_COLOR_REPLACE) {
        return false;
    }
    const PaneActionTarget target = state.routing.pane_targets.CaptureAction(
        state.routing.batch_pane, issued_context, state.routing.targets);
    if (target.status != PaneTargetStatus::Ok
        || !target.context.document_session.has_value()
        || !target.context.generation.has_value()) {
        return false;
    }
    ViewOptionsDialogState selector{};
    selector.title = UiText(UiStringId::Text0450);
    selector.labels = {
        UiText(UiStringId::Text0722), UiText(UiStringId::Text0704), nullptr, nullptr};
    selector.values = {1, 2, 0, 0};
    selector.value_count = 2U;
    if (ShowViewOptions(
            state.lifetime.instance,
            state.Workspace().windows.window,
            state.lifetime.smoke_test,
            selector) != IDOK
        || selector.values[0] < 1 || selector.values[1] < 1
        || selector.values[0] == selector.values[1]) {
        return false;
    }
    const std::uint32_t old_index =
        static_cast<std::uint32_t>(selector.values[0] - 1);
    const std::uint32_t new_index =
        static_cast<std::uint32_t>(selector.values[1] - 1);
    InkpodBatchPairPreview* preview{};
    const InkpodStatus status = state.engine->Invoke(
        target.context.document_session.value(),
        target.context.generation.value(),
        [old_index, new_index, &preview](InkpodCore* core) {
            InkpodSequenceSourceIdentity old_identity{};
            old_identity.struct_size = sizeof(old_identity);
            InkpodSequenceSourceIdentity new_identity{};
            new_identity.struct_size = sizeof(new_identity);
            InkpodStatus call = inkpod_core_sequence_source_identity(
                core, old_index, &old_identity);
            if (call == INKPOD_STATUS_OK) {
                call = inkpod_core_sequence_source_identity(
                    core, new_index, &new_identity);
            }
            if (call == INKPOD_STATUS_OK) {
                call = inkpod_core_batch_extract_color_pairs(
                    core, &old_identity, &new_identity, &preview);
            }
            return call;
        },
        false,
        false);
    if (status != INKPOD_STATUS_OK || preview == nullptr) {
        inkpod_batch_pair_preview_release(&preview);
        return false;
    }
    InkpodBatchPairPreviewInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_batch_pair_preview_get_info(preview, &info) != INKPOD_STATUS_OK
        || info.candidate_count == 0U || info.candidate_count > 4096U) {
        inkpod_batch_pair_preview_release(&preview);
        return false;
    }
    std::vector<InkpodBatchPairCandidate> candidates;
    std::vector<InkpodBatchColorPairInput> pairs;
    try {
        candidates.resize(static_cast<std::size_t>(info.candidate_count));
        pairs.reserve(static_cast<std::size_t>(info.candidate_count));
    } catch (const std::bad_alloc&) {
        inkpod_batch_pair_preview_release(&preview);
        return false;
    }
    for (std::uint64_t index = 0U; index < info.candidate_count; ++index) {
        auto& candidate = candidates[static_cast<std::size_t>(index)];
        candidate.struct_size = sizeof(candidate);
        if (inkpod_batch_pair_preview_get_candidate(
                preview, index, &candidate) != INKPOD_STATUS_OK) {
            inkpod_batch_pair_preview_release(&preview);
            return false;
        }
    }
    inkpod_batch_pair_preview_release(&preview);

    for (std::size_t begin = 0U; begin < candidates.size();) {
        std::size_t end = begin + 1U;
        while (end < candidates.size()
               && SameBatchColor(
                   candidates[begin].old_color, candidates[end].old_color)) {
            ++end;
        }
        std::optional<std::size_t> selected;
        if ((candidates[begin].flags
                & INKPOD_BATCH_PAIR_CANDIDATE_AMBIGUOUS)
            == 0U) {
            selected = begin;
        } else if (state.lifetime.smoke_test) {
            selected = begin;
        } else {
            for (std::size_t index = begin; index < end; ++index) {
                const auto& candidate = candidates[index];
                std::array<wchar_t, 384U> message{};
                swprintf_s(
                    message.data(),
                    message.size(),
                    UiText(UiStringId::BatchPairCandidatePromptFormat),
                    candidate.old_color.red,
                    candidate.old_color.green,
                    candidate.old_color.blue,
                    candidate.old_color.alpha,
                    candidate.new_color.red,
                    candidate.new_color.green,
                    candidate.new_color.blue,
                    candidate.new_color.alpha,
                    static_cast<unsigned long long>(candidate.pixel_count),
                    candidate.bounds_x,
                    candidate.bounds_y,
                    candidate.bounds_width,
                    candidate.bounds_height);
                const int decision = MessageBoxW(
                    state.Workspace().windows.window,
                    message.data(),
                    UiText(UiStringId::Text0099),
                    MB_YESNOCANCEL | MB_ICONQUESTION);
                if (decision == IDCANCEL) {
                    return false;
                }
                if (decision == IDYES) {
                    selected = index;
                    break;
                }
            }
        }
        if (selected.has_value()) {
            InkpodBatchColorPairInput pair{};
            pair.struct_size = sizeof(pair);
            pair.enabled = 1U;
            pair.old_color = candidates[selected.value()].old_color;
            pair.new_color = candidates[selected.value()].new_color;
            try {
                pairs.push_back(pair);
            } catch (const std::bad_alloc&) {
                return false;
            }
        }
        begin = end;
    }
    if (pairs.empty()) {
        return false;
    }
    state.batch.operations[state.batch.selected_operation].color_pairs =
        std::move(pairs);
    ResetBatchDerivedState(state.batch);
    try {
        state.batch.last_result = UiText(UiStringId::Text0447)
            + std::to_wstring(info.candidate_count) + L" / ambiguity "
            + std::to_wstring(info.ambiguity_count) + L" / unchanged "
            + std::to_wstring(info.unchanged_pixel_count);
    } catch (const std::bad_alloc&) {
        state.batch.last_result = UiText(UiStringId::Text0449);
    }
    RefreshBatchPalette(state.batch, state.Workspace().batch_palette);
    return true;
}

std::wstring BatchReportSummary(const InkpodBatchReport* report) {
    return BatchController::ReportSummary(report);
}

InkpodStatus InstallBatchNewTabs(
    ApplicationHost& state, InkpodBatchReport* report) noexcept {
    if (state.engine == nullptr || report == nullptr) {
        return INKPOD_STATUS_INVALID_ARGUMENT;
    }
    InkpodBatchReportInfo info{};
    info.struct_size = sizeof(info);
    if (inkpod_batch_report_get_info(report, &info) != INKPOD_STATUS_OK) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (info.staged_result_count == 0U) {
        return INKPOD_STATUS_OK;
    }
    const std::size_t existing = state.engine->SessionCount();
    if (info.staged_result_count
        > inkpod::app::CoreHost::kMaximumDocumentSessions - existing) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<ApplicationHost::DocumentBinding> staged;
    try {
        staged.reserve(static_cast<std::size_t>(info.staged_result_count));
    } catch (const std::bad_alloc&) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto destination_group = state.routing.targets.EditorGroup();
    for (std::uint64_t index = 0U; index < info.staged_result_count; ++index) {
        const auto binding = state.PrepareBatchResultSession(report, index);
        if (!binding.has_value()) {
            for (const auto& prepared : staged) {
                (void)state.DiscardPreparedDocumentSession(prepared);
            }
            return INKPOD_STATUS_INVALID_STATE;
        }
        staged.push_back(binding.value());
    }
    std::size_t published{};
    for (; published < staged.size(); ++published) {
        if (!state.PublishPreparedDocumentSession(
                staged[published], destination_group)) {
            break;
        }
    }
    if (published != staged.size()) {
        for (std::size_t index = 0U; index < published; ++index) {
            (void)state.CloseDocumentSession(staged[index].session);
        }
        for (std::size_t index = published; index < staged.size(); ++index) {
            (void)state.DiscardPreparedDocumentSession(staged[index]);
        }
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!state.ActivateDocumentView(staged.back().view)) {
        for (const auto& binding : staged) {
            (void)state.CloseDocumentSession(binding.session);
        }
        return INKPOD_STATUS_INVALID_STATE;
    }
    ResetUiForNewActiveDocument(state);
    return INKPOD_STATUS_OK;
}

InkpodStatus PreviewBatch(
    ApplicationHost& state,
    const CommandContext& issued_context,
    InkpodBatchRunScope scope) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const PaneActionTarget pane_target =
        state.routing.pane_targets.CaptureAction(
            state.routing.batch_pane,
            issued_context,
            state.routing.targets);
    if (pane_target.status != PaneTargetStatus::Ok) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    BatchController controller(
        state.lifetime,
        state.Workspace().windows,
        state.Workspace().job_progress,
        state.Workspace().job_progress_state,
        state.Workspace().batch_palette,
        state.batch,
        *state.engine);
    return controller.Preview(pane_target.context, scope);
}

InkpodStatus StartBatch(
    ApplicationHost& state,
    const CommandContext& issued_context,
    InkpodBatchRunScope scope,
    bool dry_run) noexcept {
    if (state.engine == nullptr || state.batch.task != nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!PrepareBatchRunOperations(state)) {
        return INKPOD_STATUS_CANCELLED;
    }
    const PaneActionTarget pane_target =
        state.routing.pane_targets.CaptureAction(
            state.routing.batch_pane,
            issued_context,
            state.routing.targets);
    if (pane_target.status != PaneTargetStatus::Ok) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const auto* previous_binding = state.routing.pane_targets.Find(
        state.routing.batch_pane);
    state.batch.return_to_pinned = previous_binding != nullptr
        && previous_binding->policy == PaneTargetPolicy::PinnedDocument;
    state.batch.return_context = pane_target.context;
    const auto job = state.routing.targets.BeginJob();
    if (!job.has_value()) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    CommandContext context = pane_target.context;
    context.pane = state.routing.batch_pane;
    context.job = job;
    state.batch.job_id = job;
    if (state.routing.pane_targets.BindJob(
            state.routing.batch_pane, context, state.routing.targets)
        != PaneTargetStatus::Ok) {
        (void)state.routing.targets.EndJob(job.value());
        state.batch.job_id.reset();
        state.batch.return_context = {};
        return INKPOD_STATUS_INVALID_STATE;
    }
    state.batch.job_text = L"Job " + std::to_wstring(job->Value())
        + UiText(UiStringId::Text0013);
    UpdateBatchTarget(state);
    RefreshBatchPalette(state.batch, state.Workspace().batch_palette);
    BatchController controller(
        state.lifetime,
        state.Workspace().windows,
        state.Workspace().job_progress,
        state.Workspace().job_progress_state,
        state.Workspace().batch_palette,
        state.batch,
        *state.engine);
    InkpodStatus status = controller.Start(
        context, scope, dry_run, kBatchTaskCompleted);
    if (state.lifetime.smoke_test && status == INKPOD_STATUS_OK && !dry_run
        && state.batch.output_destination == INKPOD_BATCH_OUTPUT_NEW_TABS
        && state.batch.report != nullptr) {
        status = InstallBatchNewTabs(state, state.batch.report);
    }
    if (status != INKPOD_STATUS_OK || state.lifetime.smoke_test) {
        (void)state.routing.targets.EndJob(job.value());
        if (state.batch.return_to_pinned) {
            (void)state.routing.pane_targets.PinDocument(
                state.routing.batch_pane,
                state.batch.return_context,
                state.routing.targets);
        } else {
            (void)state.routing.pane_targets.FollowActive(
                state.routing.batch_pane);
        }
        state.batch.job_id.reset();
        state.batch.completion_context = {};
        state.batch.return_context = {};
        state.batch.job_text = status == INKPOD_STATUS_OK
            ? UiText(UiStringId::Text0621)
            : (status == INKPOD_STATUS_CANCELLED ? UiText(UiStringId::Text0171) : UiText(UiStringId::Text1022));
        UpdateBatchTarget(state);
        RefreshBatchPalette(state.batch, state.Workspace().batch_palette);
    }
    return status;
}

bool ChooseBatchSettingsPath(
    HWND owner, bool save, std::wstring& selected_path) noexcept {
    std::array<wchar_t, 32768> path{};
    if (!selected_path.empty()) {
        wcsncpy_s(path.data(), path.size(), selected_path.c_str(), _TRUNCATE);
    }
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = UiText(UiStringId::Text0093);
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

HWND AuxiliaryPaneWindow(
    const ApplicationHost& state, WorkspaceAuxiliaryPane type) noexcept {
    switch (type) {
        case WorkspaceAuxiliaryPane::Locator:
            return state.Workspace().locator_palette;
        case WorkspaceAuxiliaryPane::Sequence:
            return state.Workspace().sequence_palette;
        case WorkspaceAuxiliaryPane::LightTable:
            return state.Workspace().light_table_palette;
        case WorkspaceAuxiliaryPane::Reference:
            return state.Workspace().subpalette_palette;
        case WorkspaceAuxiliaryPane::Batch:
            return state.Workspace().batch_palette;
        case WorkspaceAuxiliaryPane::Count:
            return nullptr;
    }
    return nullptr;
}

WorkspaceSplitOrientation CaptureSplitOrientation(
    const ApplicationHost& state) noexcept {
    switch (state.Workspace().editors.Orientation()) {
        case EditorSplitOrientation::Vertical:
            return WorkspaceSplitOrientation::Vertical;
        case EditorSplitOrientation::Horizontal:
            return WorkspaceSplitOrientation::Horizontal;
        case EditorSplitOrientation::None:
            return WorkspaceSplitOrientation::None;
    }
    return WorkspaceSplitOrientation::None;
}

void CaptureWorkspacePresentation(ApplicationHost& state) noexcept {
    WorkspaceLayoutState& layout = state.Workspace().windows.workspace;
    layout.split_orientation = CaptureSplitOrientation(state);
    layout.split_ratio_milli = state.Workspace().editors.SplitRatioMilli();
    if (state.Workspace().windows.window != nullptr
        && IsWindowVisible(state.Workspace().windows.window) != FALSE) {
        static_cast<void>(inkpod::windows::ui::CaptureWorkspaceWindowPlacement(
            state.Workspace().windows.window, layout));
    }
}

void ApplyWorkspacePresentation(ApplicationHost& state) noexcept {
    WorkspaceLayoutState& layout = state.Workspace().windows.workspace;
    state.Workspace().editors.SetSplitRatioMilli(layout.split_ratio_milli);
    if (state.Workspace().editors.GroupCount() > 1U) {
        const EditorSplitOrientation orientation =
            layout.split_orientation == WorkspaceSplitOrientation::Horizontal
            ? EditorSplitOrientation::Horizontal
            : EditorSplitOrientation::Vertical;
        static_cast<void>(state.Workspace().editors.SetOrientation(orientation));
    }
}

void CollapseAutoHiddenPanes(ApplicationHost& state) noexcept {
    const WorkspaceLayoutState& layout = state.Workspace().windows.workspace;
    for (std::size_t index = 0U;
         index < inkpod::windows::ui::kWorkspaceAuxiliaryPaneCount;
         ++index) {
        const auto type = static_cast<WorkspaceAuxiliaryPane>(index);
        const DockPaneType dock_type =
            inkpod::windows::ui::DockPaneTypeForAuxiliary(type);
        const DockPanePlacement* pane = layout.dock.Pane(dock_type);
        if (pane != nullptr && pane->zone == DockZone::AutoHide) {
            state.Workspace().windows.dock_host.HideAutoHiddenPane(dock_type);
        }
    }
}

bool ToggleAuxiliaryPaneVisibility(
    ApplicationHost& state, WorkspaceAuxiliaryPane type) noexcept {
    WorkspaceLayoutState& layout = state.Workspace().windows.workspace;
    const auto* auxiliary = inkpod::windows::ui::FindWorkspaceAuxiliaryPane(
        layout, type);
    const DockPaneType dock_type =
        inkpod::windows::ui::DockPaneTypeForAuxiliary(type);
    const DockPanePlacement* pane = layout.dock.Pane(dock_type);
    if (auxiliary == nullptr || pane == nullptr
        || AuxiliaryPaneWindow(state, type) == nullptr) {
        return false;
    }
    bool show{};
    bool layout_changed{};
    if (pane->zone == DockZone::AutoHide) {
        show = !state.Workspace().windows.dock_host.AutoHiddenPaneVisible(
            dock_type);
        if (show) {
            show = state.Workspace().windows.dock_host.ShowAutoHiddenPane(
                dock_type,
                inkpod::windows::ui::DockZoneForAutoHideEdge(auxiliary->edge));
        } else {
            state.Workspace().windows.dock_host.HideAutoHiddenPane(dock_type);
        }
    } else {
        show = !layout.dock.IsPaneVisible(dock_type);
        const DockResult result = state.Workspace().windows.dock_host.TogglePane(
            dock_type);
        if (result != DockResult::Ok) {
            return false;
        }
        layout_changed = true;
        if (show) {
            static_cast<void>(
                state.Workspace().windows.dock_host.ActivatePane(dock_type));
        }
    }
    if (layout_changed) {
        layout.selected_preset = WorkspacePreset::Custom;
    }
    return show;
}

void FocusPaneWindow(HWND pane, int control) noexcept {
    if (pane == nullptr) {
        return;
    }
    const HWND root = GetAncestor(pane, GA_ROOT);
    if (root != nullptr) {
        SetForegroundWindow(root);
    }
    const HWND target = control == 0 ? pane : GetDlgItem(pane, control);
    if (target != nullptr) {
        SetFocus(target);
    }
}

void PreserveActiveJobProgressPane(ApplicationHost& state) noexcept {
    if (!HasActiveJobProgress(state.Workspace().job_progress_state)) {
        return;
    }
    static_cast<void>(state.Workspace().windows.dock_host.RestorePane(
        DockPaneType::JobProgress));
    static_cast<void>(state.Workspace().windows.dock_host.ActivatePane(
        DockPaneType::JobProgress));
}

void ClampWorkspaceOwnedWindows(ApplicationHost& state) noexcept {
    WorkspaceLayoutState& layout = state.Workspace().windows.workspace;
    static_cast<void>(inkpod::windows::ui::ApplyWorkspaceWindowPlacement(
        state.Workspace().windows.window, layout));
}

void RelayoutWorkspace(ApplicationHost& state) noexcept {
    RECT client{};
    if (GetClientRect(state.Workspace().windows.window, &client) != FALSE) {
        inkpod::windows::ui::LayoutMainChrome(
            state.Workspace().windows,
            state.lifetime.smoke_test,
            client.right - client.left,
            client.bottom - client.top);
    }
}

bool WorkspaceCanvasOwnsCapture(const ApplicationHost& state) noexcept {
    const HWND capture = GetCapture();
    for (std::size_t index = 0U;
         capture != nullptr && index < state.Workspace().editors.GroupCount();
         ++index) {
        const auto* group = state.Workspace().editors.GroupAt(index);
        if (group != nullptr && group->canvas == capture) {
            return true;
        }
    }
    return false;
}

void ApplyOrDeferWorkspacePresentation(ApplicationHost& state) noexcept {
    if (WorkspaceCanvasOwnsCapture(state)) {
        state.Workspace().workspace_presentation_pending = true;
        UpdateMenuState(state);
        return;
    }
    state.Workspace().workspace_presentation_pending = false;
    ApplyWorkspacePresentation(state);
    RelayoutWorkspace(state);
    RefreshTreePane(state);
    RefreshDockPaneViews(state);
    UpdateMenuState(state);
}

void NotifyDockHostChanged(void* context) noexcept {
    auto* state = ActivateWorkspaceContext(context);
    if (state == nullptr) {
        return;
    }
    state->Workspace().windows.workspace.selected_preset =
        WorkspacePreset::Custom;
    RelayoutWorkspace(*state);
    RefreshColorPanes(*state);
    RefreshDockPaneViews(*state);
    RefreshTreePane(*state);
    UpdateMenuState(*state);
}

bool InitializeMainChrome(ApplicationHost& state) noexcept {
    const auto session_name = WorkspaceRegistryValueName(
        L"WorkspaceSessionV5", state.Workspace().persistence_slot);
    const auto legacy_session_name = WorkspaceRegistryValueName(
        L"WorkspaceSessionV4", state.Workspace().persistence_slot);
    if (!state.lifetime.smoke_test) {
        if (!LoadWorkspaceLayout(
                state.Workspace().windows.workspace, session_name.data())
            && (LoadWorkspaceLayout(
                    state.Workspace().windows.workspace,
                    legacy_session_name.data())
                || (state.Workspace().persistence_slot == 0U
                    && (LoadWorkspaceLayout(
                            state.Workspace().windows.workspace,
                            L"WorkspaceSessionV4")
                        || LoadWorkspaceLayout(
                            state.Workspace().windows.workspace,
                            L"WorkspaceSessionV2"))))) {
            if (SaveWorkspaceLayout(
                    state.Workspace().windows.workspace, session_name.data())) {
                static_cast<void>(DeleteWorkspaceLayout(
                    legacy_session_name.data()));
                static_cast<void>(DeleteWorkspaceLayout(L"WorkspaceSessionV4"));
                static_cast<void>(DeleteWorkspaceLayout(L"WorkspaceSessionV2"));
            }
        }
    }
    if (!inkpod::windows::ui::CreateMainChrome(
            state.Workspace().windows,
            state.Workspace().editors,
            state.lifetime.instance,
            state.lifetime.smoke_test)) {
        return false;
    }
    if (!state.lifetime.smoke_test) {
        static_cast<void>(inkpod::windows::ui::ApplyWorkspaceWindowPlacement(
            state.Workspace().windows.window,
            state.Workspace().windows.workspace));
    }
    state.Workspace().batch_dialog = {};
    state.Workspace().batch_dialog.context = &state.Workspace();
    state.Workspace().batch_dialog.dispatch_command = DispatchBatchPaletteCommand;
    state.Workspace().batch_dialog.select_operation = SelectBatchPaletteOperation;
    state.Workspace().batch_dialog.refresh = RefreshBatchPaletteTimer;
    state.Workspace().batch_dialog.parameter_editor = {
        &state.Workspace(),
        &state.batch,
        &state.Workspace().tools.drawing_color,
        BatchDraftChanged};
    state.Workspace().batch_palette = inkpod::windows::ui::CreateBatchPaletteDialog(
        state.lifetime.instance,
        state.Workspace().windows.window,
        state.Workspace().batch_dialog);
    if (state.Workspace().batch_palette == nullptr) {
        return false;
    }
    UpdateBatchTarget(state);
    RefreshBatchPalette(state.batch, state.Workspace().batch_palette);
    ShowWindow(state.Workspace().batch_palette, SW_HIDE);
    state.Workspace().job_progress_state = {};
    state.Workspace().job_progress = inkpod::windows::ui::CreateJobProgressPane(
        state.lifetime.instance,
        state.Workspace().windows.window,
        state.Workspace().job_progress_state);
    if (state.Workspace().job_progress == nullptr) {
        return false;
    }
    ShowWindow(state.Workspace().job_progress, SW_HIDE);
    state.Workspace().locator_dialog = {};
    state.Workspace().locator_dialog.context = &state.Workspace();
    state.Workspace().locator_dialog.dispatch_command =
        DispatchLocatorPaneCommand;
    state.Workspace().locator_dialog.select_pixel = SelectLocatorPixel;
    state.Workspace().locator_palette =
        inkpod::windows::ui::panes::CreateLocatorPaneDialog(
            state.lifetime.instance,
            state.Workspace().windows.window,
            state.Workspace().locator_dialog);
    if (state.Workspace().locator_palette == nullptr) {
        return false;
    }
    ShowWindow(state.Workspace().locator_palette, SW_HIDE);
    state.Workspace().sequence_dialog = {};
    state.Workspace().sequence_dialog.context = &state.Workspace();
    state.Workspace().sequence_dialog.thumbnail_cache = &state.Thumbnails();
    state.Workspace().sequence_dialog.dispatch_command =
        DispatchSequencePaneCommand;
    state.Workspace().sequence_dialog.activate_cell =
        ActivateSequencePaneCell;
    state.Workspace().sequence_dialog.reorder_cell =
        ReorderCutSequenceCell;
    state.Workspace().sequence_palette =
        inkpod::windows::ui::panes::CreateSequencePaneDialog(
            state.lifetime.instance,
            state.Workspace().windows.window,
            state.Workspace().sequence_dialog);
    if (state.Workspace().sequence_palette == nullptr) {
        return false;
    }
    ShowWindow(state.Workspace().sequence_palette, SW_HIDE);
    state.Workspace().light_table_dialog = {};
    state.Workspace().light_table_dialog.context = &state.Workspace();
    state.Workspace().light_table_dialog.dispatch_command =
        DispatchLightTablePaneCommand;
    state.Workspace().light_table_dialog.select_entry =
        SelectLightTablePaneEntry;
    state.Workspace().light_table_palette =
        inkpod::windows::ui::panes::CreateLightTablePaneDialog(
            state.lifetime.instance,
            state.Workspace().windows.window,
            state.Workspace().light_table_dialog);
    if (state.Workspace().light_table_palette == nullptr) {
        return false;
    }
    ShowWindow(state.Workspace().light_table_palette, SW_HIDE);
    const auto subpalette_canvas =
        state.routing.targets.RegisterAuxiliaryCanvas();
    if (!subpalette_canvas.has_value()) {
        return false;
    }
    state.Workspace().subpalette_canvas_id = subpalette_canvas.value();
    state.Workspace().subpalette_surface_generation =
        state.routing.targets.CurrentGeneration();
    state.Workspace().subpalette_dialog = {};
    state.Workspace().subpalette_dialog.context = &state.Workspace();
    state.Workspace().subpalette_dialog.dispatch_command =
        DispatchSubpalettePaneCommand;
    state.Workspace().subpalette_dialog.perform_action =
        PerformSubpalettePaneAction;
    state.Workspace().subpalette_dialog.sample = SampleSubpalettePane;
    state.Workspace().subpalette_dialog.apply_view = ApplySubpalettePaneView;
    state.Workspace().subpalette_palette =
        inkpod::windows::ui::panes::CreateSubpalettePaneDialog(
            state.lifetime.instance,
            state.Workspace().windows.window,
            *state.renderer,
            state.Workspace().subpalette_canvas_id,
            state.Workspace().subpalette_surface_generation,
            state.Workspace().subpalette_dialog);
    if (state.Workspace().subpalette_palette == nullptr) {
        (void)state.routing.targets.UnregisterAuxiliaryCanvas(
            state.Workspace().subpalette_canvas_id);
        state.Workspace().subpalette_canvas_id = {};
        return false;
    }
    ShowWindow(state.Workspace().subpalette_palette, SW_HIDE);
    state.Workspace().tools.palette_dialog = {};
    state.Workspace().tools.palette_dialog.context = &state.Workspace();
    state.Workspace().tools.palette_dialog.dispatch_command = DispatchToolPaletteCommand;
    state.Workspace().tools.palette_dialog.request_options =
        RequestToolPaletteOptions;
    state.Workspace().tools.palette_dialog.visibility_changed =
        NotifyToolPaletteVisibilityChanged;
    state.Workspace().tools.palette = inkpod::windows::ui::CreateToolPaletteDialog(
        state.lifetime.instance,
        state.Workspace().windows.window,
        state.Workspace().tools.palette_dialog);
    if (state.Workspace().tools.palette == nullptr) {
        return false;
    }
    state.Workspace().windows.tool_palette = state.Workspace().tools.palette;
    state.Workspace().tools.options_pane.context = &state.Workspace();
    state.Workspace().tools.options_pane.dispatch_command = DispatchToolPaletteCommand;
    state.Workspace().tools.options_pane.change_diameter = ChangeToolOptionsDiameter;
    state.Workspace().tools.options_pane.change_brush = ChangeToolOptionsBrush;
    state.Workspace().tools.options_pane.query_detail = QueryToolOptionsDetail;
    state.Workspace().tools.options_pane.change_detail = ChangeToolOptionsDetail;
    state.Workspace().tools.options_pane.active_tool = state.Workspace().tools.active_tool;
    state.Workspace().tools.options_pane.active_plane = state.Workspace().tools.active_plane;
    state.Workspace().tools.options_pane.diameter = state.Workspace().tools.diameter;
    state.Workspace().tools.options_pane.brush = state.Workspace().tools.brush;
    state.Workspace().windows.tool_options_flyout =
        inkpod::windows::ui::panes::CreateToolOptionsFlyout(
            state.lifetime.instance,
            state.Workspace().windows.window,
            state.Workspace().tools.options_flyout,
            state.Workspace().tools.options_pane);
    state.Workspace().windows.tool_options =
        state.Workspace().tools.options_flyout.pane;
    if (state.Workspace().windows.tool_options_flyout == nullptr
        || state.Workspace().windows.tool_options == nullptr) {
        return false;
    }
    state.Workspace().panes.color_pane.context = &state.Workspace();
    state.Workspace().panes.color_pane.dispatch_command = DispatchColorPaneCommand;
    state.Workspace().panes.color_pane.change_color = ChangeDockDrawingColor;
    state.Workspace().panes.color_pane.change_main_line_color = ChangeDockMainLineColor;
    state.Workspace().panes.color_pane.select_color = SelectDockColor;
    state.Workspace().panes.color_pane.change_group = ChangeDockPaletteGroup;
    state.Workspace().windows.color_pane = inkpod::windows::ui::panes::CreateColorDockPane(
        state.lifetime.instance,
        state.Workspace().windows.window,
        state.Workspace().panes.color_pane);
    if (state.Workspace().windows.color_pane == nullptr) {
        return false;
    }
    auto& layer_dialog = state.Workspace().panes.layer_palette_dialog;
    layer_dialog.context = &state.Workspace();
    layer_dialog.thumbnail_cache = &state.Thumbnails();
    layer_dialog.dispatch_command = DispatchLayerPaletteCommand;
    layer_dialog.select_layer = SelectLayerPaletteLayer;
    layer_dialog.select_plane = SelectLayerPalettePlane;
    layer_dialog.reorder_layer = ReorderLayerPaletteLayer;
    layer_dialog.reorder_plane = ReorderLayerPalettePlane;
    layer_dialog.toggle_target = ToggleLayerPaletteTarget;
    layer_dialog.change_split = ChangeLayerPaletteSplit;
    layer_dialog.visibility_changed = NotifyLayerPaletteVisibilityChanged;
    layer_dialog.split_milli = state.Workspace().windows.workspace.layer_split_milli;
    state.Workspace().panes.layer_palette = CreateLayerPaletteDialog(
        state.lifetime.instance,
        state.Workspace().windows.window,
        state.Workspace().panes.layer_palette_dialog);
    if (state.Workspace().panes.layer_palette == nullptr) {
        return false;
    }
    state.Workspace().windows.layer_palette = state.Workspace().panes.layer_palette;
    state.Workspace().windows.dock_host.SetChangedCallback(
        NotifyDockHostChanged, &state.Workspace());
    if (!state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::Tool, state.Workspace().windows.tool_palette)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::Color, state.Workspace().windows.color_pane)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::Layer, state.Workspace().windows.layer_palette)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::Locator, state.Workspace().locator_palette)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::Sequence, state.Workspace().sequence_palette)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::LightTable, state.Workspace().light_table_palette)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::Reference, state.Workspace().subpalette_palette)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::Batch, state.Workspace().batch_palette)
        || !state.Workspace().windows.dock_host.AttachPane(
            DockPaneType::JobProgress, state.Workspace().job_progress)) {
        return false;
    }
    ApplyWorkspacePresentation(state);
    RelayoutWorkspace(state);
    return true;
}

void ShowInitialPalettes(ApplicationHost& state) noexcept {
    RelayoutWorkspace(state);
    RefreshColorPanes(state);
    RefreshDockPaneViews(state);
    RefreshTreePane(state);
    RefreshLocatorPane(state);
    RefreshSequencePane(state);
    RefreshLightTablePane(state);
    RefreshSubpalettePane(state);
    UpdateMenuState(state);
}

inkpod::app::WorkspaceWindow* CreateWorkspaceWindow(
    ApplicationHost& state, bool show) noexcept {
    const WorkspaceWindowId previous = state.Workspace().id;
    inkpod::app::WorkspaceWindow* workspace = state.AddWorkspaceWindow();
    if (workspace == nullptr) {
        return nullptr;
    }
    const WorkspaceWindowId created = workspace->id;
    HMENU menu = LoadLocalizedMenuW(
        state.lifetime.instance, MAKEINTRESOURCEW(IDR_MAIN_MENU));
    if (menu == nullptr) {
        (void)state.RemoveWorkspaceWindow(created);
        (void)state.ActivateWorkspaceWindow(previous, false);
        return nullptr;
    }
    const HWND window = CreateWindowExW(
        0,
        state.lifetime.window_class_name.c_str(),
        state.lifetime.window_title.c_str(),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1024,
        720,
        nullptr,
        menu,
        state.lifetime.instance,
        workspace);
    if (window != nullptr) {
        const BOOL use_dark_mode = TRUE;
        (void)DwmSetWindowAttribute(
            window,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &use_dark_mode,
            sizeof(use_dark_mode));
    }
    if (window == nullptr) {
        DestroyMenu(menu);
    }
    if (window == nullptr
        || !RegisterWorkspaceSnapshotSinks(state, *workspace)) {
        if (workspace->subpalette_canvas_id) {
            (void)state.routing.targets.UnregisterAuxiliaryCanvas(
                workspace->subpalette_canvas_id);
            workspace->subpalette_canvas_id = {};
        }
        if (window != nullptr) {
            DestroyWindow(window);
            workspace->windows.window = nullptr;
        }
        (void)state.RemoveWorkspaceWindow(created);
        (void)state.ActivateWorkspaceWindow(previous, false);
        return nullptr;
    }
    UpdateMenuState(state);
    if (show) {
        ShowWindow(window, state.lifetime.show_command);
        ShowInitialPalettes(state);
        UpdateWindow(window);
    }
    return workspace;
}

void DestroyEmptyWorkspaceWindow(
    ApplicationHost& state,
    WorkspaceWindowId workspace_id,
    WorkspaceWindowId restore) noexcept {
    auto* workspace = state.FindWorkspace(workspace_id);
    if (workspace == nullptr) {
        return;
    }
    (void)state.ActivateWorkspaceWindow(workspace_id, false);
    if (workspace->windows.window != nullptr
        && (!RetargetCoreNotificationsBeforeWorkspaceClose(
                state, workspace->id, workspace->windows.window)
            || !UnregisterWorkspaceSnapshotSinks(state, *workspace))) {
        (void)state.ActivateWorkspaceWindow(restore, false);
        return;
    }
    if (workspace->subpalette_canvas_id) {
        (void)state.routing.targets.UnregisterAuxiliaryCanvas(
            workspace->subpalette_canvas_id);
        workspace->subpalette_canvas_id = {};
    }
    if (workspace->windows.window != nullptr) {
        if (DestroyWindow(workspace->windows.window) == FALSE) {
            (void)RegisterWorkspaceSnapshotSinks(state, *workspace);
            (void)state.ActivateWorkspaceWindow(restore, false);
            return;
        }
        workspace->windows.window = nullptr;
    }
    (void)state.RemoveWorkspaceWindow(workspace_id);
    (void)state.ActivateWorkspaceWindow(restore, false);
}

bool MoveOrDuplicateViewToNewWorkspace(
    ApplicationHost& state,
    const CommandContext& context,
    bool duplicate,
    std::optional<POINT> drop_point) noexcept {
    if (!context.workspace.has_value() || !context.document_view.has_value()) {
        return false;
    }
    const WorkspaceWindowId source_workspace = context.workspace.value();
    const DocumentViewId source_view = context.document_view.value();
    auto* created = CreateWorkspaceWindow(state, false);
    if (created == nullptr) {
        return false;
    }
    const WorkspaceWindowId destination_workspace = created->id;
    const inkpod::app::EditorGroup* destination_group = created->editors.Active();
    bool transferred{};
    if (destination_group != nullptr
        && state.ActivateDocumentView(source_view)) {
        transferred = duplicate
            ? CreateDocumentViewInGroup(
                state,
                destination_group->id,
                created->windows.window)
            : state.MoveDocumentViewToWorkspace(
                source_view, destination_workspace);
    }
    if (!transferred) {
        DestroyEmptyWorkspaceWindow(
            state, destination_workspace, source_workspace);
        return false;
    }
    (void)state.ActivateWorkspaceWindow(source_workspace, false);
    UpdateMenuState(state);
    (void)state.ActivateWorkspaceWindow(destination_workspace, true);
    UpdateMenuState(state);
    if (drop_point.has_value()) {
        RECT bounds{};
        if (GetWindowRect(created->windows.window, &bounds) != FALSE) {
            const HMONITOR monitor = MonitorFromPoint(
                drop_point.value(), MONITOR_DEFAULTTONEAREST);
            MONITORINFO info{};
            info.cbSize = sizeof(info);
            if (GetMonitorInfoW(monitor, &info) != FALSE) {
                const int width = std::min(
                    bounds.right - bounds.left,
                    info.rcWork.right - info.rcWork.left);
                const int height = std::min(
                    bounds.bottom - bounds.top,
                    info.rcWork.bottom - info.rcWork.top);
                const int x = std::clamp(
                    drop_point->x - MulDiv(48, GetDpiForWindow(created->windows.window), 96),
                    info.rcWork.left,
                    info.rcWork.right - width);
                const int y = std::clamp(
                    drop_point->y - MulDiv(16, GetDpiForWindow(created->windows.window), 96),
                    info.rcWork.top,
                    info.rcWork.bottom - height);
                SetWindowPos(
                    created->windows.window,
                    nullptr,
                    x,
                    y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER);
            }
        }
    }
    ShowWindow(created->windows.window, state.lifetime.show_command);
    ShowInitialPalettes(state);
    UpdateWindow(created->windows.window);
    return true;
}

std::optional<LRESULT> RouteBatchCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_BATCH:
            if (state->Workspace().batch_palette != nullptr) {
                const bool shown = ToggleAuxiliaryPaneVisibility(
                    *state, WorkspaceAuxiliaryPane::Batch);
                if (shown) {
                    UpdateBatchTarget(*state);
                    RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
                    FocusPaneWindow(
                        state->Workspace().batch_palette,
                        IDC_BATCH_OPERATIONS);
                }
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_PIN: {
            if (state->batch.task != nullptr) {
                return 0;
            }
            const auto* binding = state->routing.pane_targets.Find(
                state->routing.batch_pane);
            const PaneTargetStatus status = binding != nullptr
                    && binding->policy == PaneTargetPolicy::PinnedDocument
                ? state->routing.pane_targets.FollowActive(
                      state->routing.batch_pane)
                : state->routing.pane_targets.PinDocument(
                      state->routing.batch_pane,
                      context,
                      state->routing.targets);
            if (status == PaneTargetStatus::Ok
                || status == PaneTargetStatus::NoOp) {
                UpdateBatchTarget(*state);
                RefreshBatchPalette(
                    state->batch, state->Workspace().batch_palette);
                UpdateMenuState(*state);
                return status == PaneTargetStatus::Ok ? 1 : 0;
            }
            return 0;
        }
        case IDM_BATCH_INPUT_FILE:
        case IDM_BATCH_INPUT_FOLDER:
        case IDM_BATCH_INPUT_CURRENT: {
            if (state->batch.task != nullptr) {
                return 0;
            }
            std::wstring path;
            InkpodBatchInputKind kind = INKPOD_BATCH_INPUT_ACTIVE_DOCUMENT;
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
            state->batch.inputs = {{kind, std::move(path)}};
            state->batch.selected_stage = 0U;
            RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_BATCH_INPUT_RANGE: {
            if (state->batch.task != nullptr || state->batch.inputs.empty()) {
                return 0;
            }
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0262);
            dialog.labels = {
                UiText(UiStringId::Text1020),
                UiText(UiStringId::Text0842),
                nullptr,
                nullptr};
            dialog.values = {
                static_cast<std::int32_t>(state->batch.inputs.front().first_cell),
                static_cast<std::int32_t>(state->batch.inputs.front().last_cell),
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
            state->batch.inputs.front().first_cell =
                static_cast<std::uint32_t>(dialog.values[0]);
            state->batch.inputs.front().last_cell =
                static_cast<std::uint32_t>(dialog.values[1]);
            RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_BATCH_ADD_COLOR_REPLACE:
        case IDM_BATCH_ADD_MOVE_TO_COLOR_PLANE:
        case IDM_BATCH_ADD_MASKING:
        case IDM_BATCH_ADD_ERASE:
            if (state->batch.task == nullptr
                && AddBatchOperation(*state, LOWORD(wparam))) {
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_OPERATION_DUPLICATE:
            if (state->batch.task == nullptr
                && state->batch.selected_stage > 0U
                && state->batch.selected_stage
                    == state->batch.selected_operation + 1U
                && state->batch.selected_operation
                    < state->batch.operations.size()) {
                try {
                    state->batch.operations.insert(
                        state->batch.operations.begin()
                            + state->batch.selected_operation + 1U,
                        state->batch.operations[state->batch.selected_operation]);
                    ++state->batch.selected_operation;
                    state->batch.selected_stage =
                        state->batch.selected_operation + 1U;
                    ResetBatchDerivedState(state->batch);
                    RefreshBatchPalette(
                        state->batch, state->Workspace().batch_palette);
                    UpdateMenuState(*state);
                    return 1;
                } catch (const std::bad_alloc&) {
                    return 0;
                }
            }
            return 0;
        case IDM_BATCH_REPLACE_SWAP:
            if (state->batch.task == nullptr
                && state->batch.selected_stage > 0U
                && state->batch.selected_stage
                    == state->batch.selected_operation + 1U
                && state->batch.selected_operation < state->batch.operations.size()) {
                BatchOperationUi& operation =
                    state->batch.operations[state->batch.selected_operation];
                if (operation.kind == INKPOD_BATCH_OPERATION_COLOR_REPLACE
                    && !operation.color_pairs.empty()) {
                    for (auto& pair : operation.color_pairs) {
                        std::swap(pair.old_color, pair.new_color);
                    }
                    ResetBatchDerivedState(state->batch);
                    RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
                    UpdateMenuState(*state);
                    return 1;
                }
            }
            return 0;
        case IDM_BATCH_OPERATION_REMOVE:
            if (state->batch.task == nullptr
                && state->batch.selected_stage > 0U
                && state->batch.selected_stage
                    == state->batch.selected_operation + 1U
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
                state->batch.selected_stage = state->batch.operations.empty()
                    ? 0U
                    : state->batch.selected_operation + 1U;
                ResetBatchDerivedState(state->batch);
                RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_OPERATION_UP:
        case IDM_BATCH_OPERATION_DOWN:
            if (state->batch.task == nullptr
                && state->batch.selected_stage > 0U
                && state->batch.selected_stage
                    == state->batch.selected_operation + 1U
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
                    state->batch.selected_stage = target + 1U;
                    ResetBatchDerivedState(state->batch);
                    RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
                    UpdateMenuState(*state);
                    return 1;
                }
            }
            return 0;
        case IDM_BATCH_OUTPUT_FOLDER:
        case IDM_BATCH_OUTPUT_ACTIVE_DOCUMENT:
        case IDM_BATCH_OUTPUT_NEW_TABS:
            if (state->batch.task == nullptr) {
                ResetBatchDerivedState(state->batch);
                state->batch.output_destination =
                    LOWORD(wparam) == IDM_BATCH_OUTPUT_ACTIVE_DOCUMENT
                    ? INKPOD_BATCH_OUTPUT_ACTIVE_DOCUMENT
                    : (LOWORD(wparam) == IDM_BATCH_OUTPUT_NEW_TABS
                           ? INKPOD_BATCH_OUTPUT_NEW_TABS
                           : INKPOD_BATCH_OUTPUT_FOLDER);
                state->batch.selected_stage = static_cast<std::uint32_t>(
                    state->batch.operations.size() + 1U);
                RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_OUTPUT_SETTINGS: {
            if (state->batch.task != nullptr) {
                return 0;
            }
            state->batch.selected_stage = static_cast<std::uint32_t>(
                state->batch.operations.size() + 1U);
            RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
            return 1;
        }
        case IDM_BATCH_FAILURE_CONTINUE:
        case IDM_BATCH_FAILURE_STOP:
            if (state->batch.task == nullptr) {
                ResetBatchDerivedState(state->batch);
                state->batch.failure_policy = LOWORD(wparam) == IDM_BATCH_FAILURE_STOP
                    ? INKPOD_BATCH_FAILURE_STOP
                    : INKPOD_BATCH_FAILURE_CONTINUE;
                RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_EXTRACT_PAIRS:
            if (state->batch.task == nullptr
                && ExtractBatchColorPairs(*state, context)) {
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_BATCH_PREVIEW: {
            const InkpodStatus status = PreviewBatch(
                *state, context, INKPOD_BATCH_SCOPE_ALL);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0259));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_BATCH_DRY_RUN:
        case IDM_BATCH_RUN_CURRENT:
        case IDM_BATCH_RUN_ALL: {
            const UINT command = LOWORD(wparam);
            const InkpodStatus status = StartBatch(
                *state,
                context,
                command == IDM_BATCH_RUN_CURRENT
                    ? INKPOD_BATCH_SCOPE_CURRENT
                    : INKPOD_BATCH_SCOPE_ALL,
                command == IDM_BATCH_DRY_RUN);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0267));
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
                state->Workspace().windows,
                state->Workspace().job_progress,
                state->Workspace().job_progress_state,
                state->Workspace().batch_palette,
                state->batch,
                *state->engine);
            if (save) {
                status = controller.SaveGraph(utf8.data(), utf8.size());
            } else {
                status = controller.LoadGraph(utf8.data(), utf8.size());
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, save ? UiText(UiStringId::Text0257) : UiText(UiStringId::Text0258));
            }
            RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteDocumentCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    const UINT animation_command = LOWORD(wparam);
    if (animation_command >= IDM_LT_SET_NEW
        && animation_command <= IDM_LT_BULK_BOTH
        && !PrepareLightTableSelection(*state, context)) {
        return 0;
    }
    switch (LOWORD(wparam)) {
        case IDM_FILE_RECENT_1:
        case IDM_FILE_RECENT_2:
        case IDM_FILE_RECENT_3:
        case IDM_FILE_RECENT_4:
        case IDM_FILE_RECENT_5:
        case IDM_FILE_RECENT_6:
        case IDM_FILE_RECENT_7:
        case IDM_FILE_RECENT_8: {
            const std::size_t index = static_cast<std::size_t>(
                LOWORD(wparam) - IDM_FILE_RECENT_1);
            const auto* recent = state->RecentDocumentAt(index);
            if (recent == nullptr) {
                return 0;
            }
            std::wstring path;
            try {
                path = recent->path;
            } catch (const std::bad_alloc&) {
                return 0;
            }
            if (GetFileAttributesW(path.c_str()) == INVALID_FILE_ATTRIBUTES) {
                (void)state->RemoveRecentDocument(index);
                UpdateMenuState(*state);
                if (!state->lifetime.smoke_test) {
                    MessageBoxW(
                        window,
                        UiText(UiStringId::Text0738),
                        L"inkpod",
                        MB_OK | MB_ICONINFORMATION);
                }
                return 0;
            }
            const InkpodStatus status = OpenDocumentFromPath(*state, path);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0739));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_FILE_NEW: {
                InkpodEditorDefaults defaults{};
                const InkpodStatus defaults_status = state->engine == nullptr
                    ? INKPOD_STATUS_INVALID_STATE
                    : state->engine->GetEditorDefaults(
                          state->Document().id,
                          state->Document().generation,
                          defaults);
                if (defaults_status != INKPOD_STATUS_OK) {
                    ShowCoreError(*state, window, UiText(UiStringId::Text0712));
                    return 0;
                }
                CellCreationDialogState dialog{};
                dialog.options = InkpodCellCreationOptions{
                    sizeof(InkpodCellCreationOptions),
                    INKPOD_CELL_SIZING_IMAGE_PIXELS,
                    INKPOD_FEATURE_NONE,
                    defaults.width,
                    defaults.height,
                    defaults.dpi_x_milli,
                    defaults.dpi_y_milli,
                    50U,
                    900U,
                    500U,
                    INKPOD_FRAME_ANCHOR_CENTER,
                    INKPOD_LAYER_BINARY_COLORING,
                    INKPOD_STORAGE_RGBA8,
                    state->lifetime.smoke_test ? 3U : 1U,
                    0U};
                dialog.layer_choices = LayerKindChoices().data();
                dialog.layer_choice_count =
                    static_cast<std::uint32_t>(LayerKindChoices().size());
                dialog.build_preview = BuildCellCreationDialogPreview;
                if (ShowCellCreationOptions(
                        state->lifetime.instance,
                        window,
                        state->lifetime.smoke_test,
                        dialog) != IDOK) {
                    return 0;
                }
                const InkpodStatus status = CreateCellsFromOptions(*state, dialog.options);
                if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                    ShowCoreError(*state, window, UiText(UiStringId::Text0711));
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
                    ? UiText(UiStringId::Text0794)
                    : (command == IDM_CELL_IMAGE_SIZE ? UiText(UiStringId::Text0801) : UiText(UiStringId::Text0812)),
                command == IDM_CELL_RESOLUTION);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0793));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CELL_FIT_CAPTURE_FRAME: {
            const InkpodStatus status = FitPaperToCaptureFrame(*state);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0684));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0805));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0305));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_FILE_OPEN: {
                std::wstring path;
                if (ChooseOpenDocumentPath(window, path)) {
                    const InkpodStatus status = OpenDocumentFromPath(*state, path);
                    if (status != INKPOD_STATUS_OK) {
                        ShowCoreError(*state, window, UiText(UiStringId::Text1018));
                    }
                }
            return 0;
        }
        case IDM_FILE_IMPORT_RASTER: {
            std::wstring path = state->lifetime.smoke_test ? state->lifetime.smoke_raster_path : L"";
            if (!state->lifetime.smoke_test && !ChooseCommonRasterPath(window, false, path)) {
                return 0;
            }
            const InkpodStatus status = ImportCommonRasterFromPath(*state, path);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0422));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_FILE_EXPORT_RASTER:
        case IDM_FILE_EXPORT_INSTRUCTION_RASTER: {
            const bool instruction = LOWORD(wparam)
                == IDM_FILE_EXPORT_INSTRUCTION_RASTER;
            std::wstring path = state->lifetime.smoke_test ? state->lifetime.smoke_raster_path : L"";
            if (!state->lifetime.smoke_test && !ChooseCommonRasterPath(window, true, path)) {
                return 0;
            }
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0386);
            dialog.labels[0] = UiText(UiStringId::Text0817);
            dialog.values[0] = state->lifetime.smoke_test ? 1 : 0;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            const InkpodStatus status = instruction
                ? ExportInstructionCommonRasterToPath(
                      *state, path, dialog.values[0] != 0)
                : ExportCommonRasterToPath(
                      *state, path, dialog.values[0] != 0);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0421));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_FILE_SAVE:
        case IDM_FILE_SAVE_AS: {
            const InkpodStatus status = SaveDocument(
                *state, LOWORD(wparam) == IDM_FILE_SAVE_AS);
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Save));
            }
            return 0;
        }
        case IDM_FILE_COMPACT_COPY: {
            const InkpodStatus status =
                WriteCompactedDocumentCopy(*state, context);
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0635));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0659));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0399));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_FILE_AUTOSAVE_NOW: {
            std::wstring path = state->Document().shell.recovery_path;
            if (path.empty() && !ChooseInkpodPath(window, true, path)) {
                return 0;
            }
            if (!QueueAutosave(*state, context, path)) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0076));
            } else {
                try {
                    state->Document().shell.recovery_path = path;
                } catch (const std::bad_alloc&) {
                    ShowCoreError(*state, window, UiText(UiStringId::Text0070));
                }
            }
            return 0;
        }
        case IDM_FILE_OPEN_RECOVERY: {
            std::wstring path = state->Document().shell.recovery_path;
            if (ChooseInkpodPath(window, false, path)
                && OpenRecoveryFromPath(*state, path) != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0074));
            }
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteEditCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0637));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0900));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0183));
            } else {
                inkpod_clipboard_release(&state->clipboard);
                state->clipboard = replacement;
                if (!PublishStandardClipboard(window, state->clipboard)
                    && !state->lifetime.smoke_test) {
                    MessageBoxW(
                        window,
                        UiText(UiStringId::Text0126),
                        L"inkpod",
                        MB_OK | MB_ICONWARNING);
                }
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_EDIT_CUT: {
            (void)RouteEditCommand(
                state, window, IDM_EDIT_COPY, 0, context);
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0139));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0939));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_EDIT_PASTE_CONVERTED: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0609);
            dialog.labels = {UiText(UiStringId::Text0329), UiText(UiStringId::Text0873), UiText(UiStringId::Text0433), UiText(UiStringId::Text0714)};
            dialog.values = {INKPOD_TYPED_PLANE_RASTER, INKPOD_STORAGE_RGBA8, 100, 1};
            dialog.value_count = 4U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            TextInputDialogState name{};
            name.title = UiText(UiStringId::Text0611);
            name.label = UiText(UiStringId::Text0567);
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
            edit.parent_id = state->Workspace().panes.active_tree_layer_id;
            edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
            edit.pixel_format = static_cast<std::uint32_t>(dialog.values[1]);
            edit.opacity_milli = static_cast<std::uint32_t>(
                std::clamp(dialog.values[2], 0, 100)) * 10U;
            InkpodStatus status = INKPOD_STATUS_INVALID_STATE;
            try {
                const std::string plane_name(
                    reinterpret_cast<const char*>(utf8.data()), utf8.size());
                status = BeginFloatingPasteToNewPlane(
                    *state, edit, plane_name);
            } catch (const std::bad_alloc&) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0609));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_EDIT_FLOATING_TRANSFORM: {
            const InkpodStatus status = ShowFloatingTransformDialog(*state);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0307));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_EDIT_FLOATING_COMMIT:
        case IDM_EDIT_FLOATING_CANCEL: {
            const bool commit = LOWORD(wparam) == IDM_EDIT_FLOATING_COMMIT;
            const InkpodStatus status = EndFloatingPaste(*state, commit);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, commit ? UiText(UiStringId::Text0938) : UiText(UiStringId::Text0937));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0797));
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
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_FILTER_LAST: {
            const InkpodEditorStateInfo* editor = PresentedEditorState(*state);
            const InkpodStatus status = state->Workspace().tools.active_plane != INKPOD_PLANE_COLOR
                    || editor == nullptr
                    || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
                    || editor->active_plane_id == 0U
                ? INKPOD_STATUS_INVALID_STATE
                : StartEffectTask(
                      *state,
                      context,
                      false,
                      [plane_id = editor->active_plane_id](
                          InkpodCore* core, InkpodTask* task) {
                          InkpodDispatchResult result{};
                          result.struct_size = sizeof(result);
                          return inkpod_core_filter_apply_last_task(
                              core, plane_id, task, &result);
                      });
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0818));
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
            const InkpodEditorStateInfo* editor = PresentedEditorState(*state);
            if (state->Workspace().tools.active_plane != INKPOD_PLANE_COLOR
                || editor == nullptr
                || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
                || editor->active_plane_id == 0U) {
                return 0;
            }
            const InkpodStatus status = RunInteractiveFilterEditor(
                *state, context, command, editor->active_plane_id);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0285));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0926));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0932));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0931));
            }
            UpdateMenuState(*state);
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}



InkpodShootingFrameInput ShootingFrameInputFromInfo(
    const InkpodShootingFrameInfo& frame) noexcept {
    constexpr double kTurnsToDegrees = 360.0 / 4294967296.0;
    return InkpodShootingFrameInput{
        sizeof(InkpodShootingFrameInput),
        frame.anchor,
        0U,
        static_cast<double>(frame.center_x_milli) / 1000.0,
        static_cast<double>(frame.center_y_milli) / 1000.0,
        static_cast<double>(frame.width_milli) / 1000.0,
        static_cast<double>(frame.height_milli) / 1000.0,
        static_cast<double>(frame.rotation_turns) * kTurnsToDegrees,
        frame.visible,
        frame.include_in_instruction_export};
}

InkpodStatus EditShootingFrameFromDialog(ApplicationHost& state) noexcept {
    InkpodDocumentInfo document{};
    InkpodShootingFrameInfo frame{};
    bool present{};
    if (!QueryDocument(state, document)
        || !QueryShootingFrame(state, present, frame)
        || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    ShootingFrameDialogState dialog{};
    dialog.value = present
        ? ShootingFrameInputFromInfo(frame)
        : InkpodShootingFrameInput{
              sizeof(InkpodShootingFrameInput),
              INKPOD_SHOOTING_FRAME_ANCHOR_CENTER,
              0U,
              static_cast<double>(document.width) / 2.0,
              static_cast<double>(document.height) / 2.0,
              std::max(1.0, static_cast<double>(document.width) * 0.8),
              std::max(1.0, static_cast<double>(document.height) * 0.8),
              0.0,
              1U,
              1U};
    if (state.lifetime.smoke_test) {
        dialog.value.rotation_degrees = 15.0;
    }
    dialog.close_immediately = state.lifetime.smoke_test;
    const InkpodShootingFrameEditKind kind = present
        ? INKPOD_SHOOTING_FRAME_EDIT_UPDATE
        : INKPOD_SHOOTING_FRAME_EDIT_CREATE;
    const std::uint64_t frame_id = present ? frame.frame_id : 0U;
    InkpodStatus status = state.engine->Invoke(
        [base_revision = document.document_revision,
         kind,
         frame_id,
         input = dialog.value](InkpodCore* core) {
            return inkpod_core_shooting_frame_preview_begin(
                core, base_revision, kind, frame_id, &input);
        },
        true,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (ShowShootingFrameOptions(
            state.lifetime.instance, state.Workspace().windows.window, dialog)
        != IDOK) {
        (void)state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_shooting_frame_preview_cancel(core);
            },
            true,
            false);
        return INKPOD_STATUS_CANCELLED;
    }
    status = state.engine->Invoke(
        [input = dialog.value](InkpodCore* core) {
            return inkpod_core_shooting_frame_preview_update(core, &input);
        },
        true,
        false);
    if (status != INKPOD_STATUS_OK) {
        (void)state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_shooting_frame_preview_cancel(core);
            },
            true,
            false);
        return status;
    }
    status = state.engine->Invoke(
        [](InkpodCore* core) {
            std::uint64_t revision{};
            std::uint64_t created_id{};
            return inkpod_core_shooting_frame_preview_apply(
                core, &revision, &created_id);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK) {
        status = SetEditorActiveTool(state, kInteractionShootingFrame);
    }
    return status;
}

InkpodStatus DeleteShootingFrame(ApplicationHost& state) noexcept {
    InkpodDocumentInfo document{};
    InkpodShootingFrameInfo frame{};
    bool present{};
    if (!QueryDocument(state, document)
        || !QueryShootingFrame(state, present, frame)
        || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    if (!present) {
        return INKPOD_STATUS_OK;
    }
    const InkpodStatus status = state.engine->Invoke(
        [expected_revision = document.document_revision, frame_id = frame.frame_id](
            InkpodCore* core) {
            std::uint64_t revision{};
            std::uint64_t ignored_id{};
            return inkpod_core_shooting_frame_edit(
                core,
                expected_revision,
                INKPOD_SHOOTING_FRAME_EDIT_DELETE,
                frame_id,
                nullptr,
                &revision,
                &ignored_id);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK
        && state.Workspace().tools.active_tool == kInteractionShootingFrame) {
        return SetEditorActiveTool(state, INKPOD_TOOL_PENCIL);
    }
    return status;
}

InkpodStatus EnsureVanishingPointLayer(
    ApplicationHost& state, std::uint64_t& layer_id) noexcept {
    TreePaneNode selected{};
    if (QueryTreeNode(state, false, selected)
        && selected.kind == INKPOD_LAYER_VANISHING_POINT) {
        layer_id = selected.id;
        return INKPOD_STATUS_OK;
    }
    InkpodTreeEdit edit{};
    edit.struct_size = sizeof(edit);
    edit.operation = INKPOD_TREE_CREATE_LAYER;
    edit.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
    edit.kind = INKPOD_LAYER_VANISHING_POINT;
    edit.opacity_milli = 1000U;
    const InkpodStatus status = ApplyTreeEditRecord(
        state, edit, "Perspective", layer_id);
    if (status == INKPOD_STATUS_OK) {
        state.Document().shell.smoke_layer_id = layer_id;
        RefreshTreePane(state);
    }
    return status;
}

InkpodStatus EditVanishingPointFromDialog(ApplicationHost& state) noexcept {
    if (state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    std::vector<InkpodVanishingPointInfo> points;
    if (!QueryVanishingPoints(state, points)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    auto& tools = state.Workspace().tools;
    const auto selected = std::find_if(
        points.begin(), points.end(), [&tools](const InkpodVanishingPointInfo& point) {
            return point.point_id == tools.vanishing_point_drag_id;
        });
    const bool updating = selected != points.end();
    std::uint64_t layer_id = updating ? selected->layer_id : 0U;
    InkpodStatus status = updating
        ? INKPOD_STATUS_OK : EnsureVanishingPointLayer(state, layer_id);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document)) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    VanishingPointDialogState dialog{};
    dialog.value = updating
        ? VanishingPointInputFromInfo(*selected)
        : InkpodVanishingPointInput{
              sizeof(InkpodVanishingPointInput),
              1U,
              0U,
              layer_id,
              static_cast<std::int64_t>(document.width) * 500,
              static_cast<std::int64_t>(document.height) * 500,
              15000U,
              0U,
              750U,
              0U,
              InkpodColorValue{
                  sizeof(InkpodColorValue),
                  INKPOD_COLOR_DEPTH_8,
                  48U,
                  128U,
                  240U,
                  255U}};
    if (state.lifetime.smoke_test && !updating) {
        dialog.value.x_milli = -20000;
    }
    dialog.close_immediately = state.lifetime.smoke_test;
    const InkpodVanishingPointEditKind kind = updating
        ? INKPOD_VANISHING_POINT_EDIT_UPDATE
        : INKPOD_VANISHING_POINT_EDIT_CREATE;
    const std::uint64_t point_id = updating ? selected->point_id : 0U;
    status = state.engine->Invoke(
        [base_revision = document.document_revision, kind, point_id,
         input = dialog.value](InkpodCore* core) {
            return inkpod_core_vanishing_point_preview_begin(
                core, base_revision, kind, point_id, &input);
        },
        true,
        false);
    if (status != INKPOD_STATUS_OK) {
        return status;
    }
    if (ShowVanishingPointOptions(
            state.lifetime.instance, state.Workspace().windows.window, dialog)
        != IDOK) {
        (void)state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_vanishing_point_preview_cancel(core);
            },
            true,
            false);
        return INKPOD_STATUS_CANCELLED;
    }
    status = state.engine->Invoke(
        [input = dialog.value](InkpodCore* core) {
            return inkpod_core_vanishing_point_preview_update(core, &input);
        },
        true,
        false);
    if (status != INKPOD_STATUS_OK) {
        (void)state.engine->Invoke(
            [](InkpodCore* core) {
                return inkpod_core_vanishing_point_preview_cancel(core);
            },
            true,
            false);
        return status;
    }
    std::uint64_t committed_id{};
    status = state.engine->Invoke(
        [&committed_id](InkpodCore* core) {
            std::uint64_t revision{};
            return inkpod_core_vanishing_point_preview_apply(
                core, &revision, &committed_id);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK) {
        tools.vanishing_point_drag_id = committed_id;
        status = SetEditorActiveTool(state, kInteractionVanishingPoint);
    }
    return status;
}

InkpodStatus DeleteAllVanishingPoints(ApplicationHost& state) noexcept {
    InkpodDocumentInfo document{};
    if (!QueryDocument(state, document) || state.engine == nullptr) {
        return INKPOD_STATUS_INVALID_STATE;
    }
    const InkpodStatus status = state.engine->Invoke(
        [expected_revision = document.document_revision](InkpodCore* core) {
            std::uint64_t revision{};
            std::uint64_t ignored{};
            return inkpod_core_vanishing_point_edit(
                core,
                expected_revision,
                INKPOD_VANISHING_POINT_EDIT_DELETE_ALL,
                0U,
                nullptr,
                &revision,
                &ignored);
        },
        true,
        true);
    if (status == INKPOD_STATUS_OK) {
        state.Workspace().tools.vanishing_point_drag_id = 0U;
    }
    return status;
}


std::optional<LRESULT> RouteDocumentPaneCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext&) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_LAYER_PALETTE:
            if (state->Workspace().panes.layer_palette != nullptr) {
                const DockResult result =
                    state->Workspace().windows.dock_host.TogglePane(
                        DockPaneType::Layer);
                if (result != DockResult::Ok) {
                    return 0;
                }
                if (state->Workspace().windows.workspace.dock.IsPaneVisible(
                        DockPaneType::Layer)) {
                    RefreshTreePane(*state);
                }
                return 1;
            }
            return 0;
        case IDM_LAYER_NEW: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0715);
            dialog.labels = {UiText(UiStringId::Text0828), UiText(UiStringId::Text0433), nullptr, nullptr};
            dialog.values = {INKPOD_LAYER_RASTER, 100, 0, 0};
            dialog.choices[0] = LayerKindChoices().data();
            dialog.choice_counts[0] =
                static_cast<std::uint32_t>(LayerKindChoices().size());
            dialog.value_count = 2U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.values[0] < INKPOD_LAYER_BINARY_COLORING
                || dialog.values[0] > INKPOD_LAYER_ADJUSTMENT
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
            InkpodStatus status = ApplyTreeEditRecord(
                *state, edit, "Layer", layer_id);
            if (status == INKPOD_STATUS_OK) {
                state->Document().shell.smoke_layer_id = layer_id;
                if (!state->RefreshEditorPresentation(
                        state->Document().id,
                        state->Document().generation)) {
                    status = INKPOD_STATUS_INVALID_STATE;
                }
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0394));
            }
            RefreshTreePane(*state);
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_CELL_SHOOTING_FRAME_PROPERTIES: {
            const InkpodStatus status = EditShootingFrameFromDialog(*state);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0690));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CELL_SHOOTING_FRAME_EDIT_HANDLES: {
            InkpodShootingFrameInfo frame{};
            bool present{};
            const InkpodStatus status = !QueryShootingFrame(*state, present, frame)
                    || !present
                ? INKPOD_STATUS_INVALID_STATE
                : SetEditorActiveTool(
                      *state,
                      state->Workspace().tools.active_tool
                              == kInteractionShootingFrame
                          ? INKPOD_TOOL_PENCIL
                          : kInteractionShootingFrame);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0685));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CELL_SHOOTING_FRAME_DELETE: {
            const InkpodStatus status = DeleteShootingFrame(*state);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0689));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CELL_VANISHING_POINT_PROPERTIES: {
            const InkpodStatus status = EditVanishingPointFromDialog(*state);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0772));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CELL_VANISHING_POINT_EDIT_HANDLES: {
            std::vector<InkpodVanishingPointInfo> points;
            const InkpodStatus status = !QueryVanishingPoints(*state, points)
                    || points.empty()
                ? INKPOD_STATUS_INVALID_STATE
                : SetEditorActiveTool(
                      *state,
                      state->Workspace().tools.active_tool
                              == kInteractionVanishingPoint
                          ? INKPOD_TOOL_PENCIL
                          : kInteractionVanishingPoint);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0769));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CELL_VANISHING_POINT_DELETE_ALL: {
            const InkpodStatus status = DeleteAllVanishingPoints(*state);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0770));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LAYER_DUPLICATE: {
            InkpodDocumentInfo info{};
            std::uint64_t duplicate_id{};
            const std::uint64_t source_id = state->Workspace().panes.active_tree_layer_id != 0U
                ? state->Workspace().panes.active_tree_layer_id
                : (QueryDocument(*state, info) ? info.layer_id : 0U);
            bool grouped{};
            InkpodStatus status = ApplyGroupedEditTargetCommand(
                *state, INKPOD_EDIT_TARGET_DUPLICATE, 0U, 0U, 0U, grouped);
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = source_id != 0U
                    ? ApplyTreeEdit(
                          *state,
                          INKPOD_TREE_DUPLICATE_LAYER,
                          source_id,
                          0U,
                          duplicate_id)
                    : INKPOD_STATUS_INVALID_STATE;
            }
            if (status == INKPOD_STATUS_OK) {
                state->Document().shell.smoke_layer_id = duplicate_id;
                if (!state->RefreshEditorPresentation(
                        state->Document().id,
                        state->Document().generation)) {
                    status = INKPOD_STATUS_INVALID_STATE;
                }
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0397));
            }
            RefreshTreePane(*state);
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_LAYER_DELETE: {
            std::uint64_t ignored{};
            const std::uint64_t target = state->Workspace().panes.active_tree_layer_id != 0U
                ? state->Workspace().panes.active_tree_layer_id
                : state->Document().shell.smoke_layer_id;
            bool grouped{};
            InkpodStatus status = ApplyGroupedEditTargetCommand(
                *state, INKPOD_EDIT_TARGET_DELETE, 0U, 0U, 0U, grouped);
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = target == 0U
                    ? INKPOD_STATUS_INVALID_STATE
                    : ApplyTreeEdit(
                          *state,
                          INKPOD_TREE_DELETE_LAYER,
                          target,
                          0U,
                          ignored);
            }
            if (status == INKPOD_STATUS_OK) {
                if (state->Document().shell.smoke_layer_id == target) {
                    state->Document().shell.smoke_layer_id = 0U;
                }
                if (!state->RefreshEditorPresentation(
                        state->Document().id,
                        state->Document().generation)) {
                    status = INKPOD_STATUS_INVALID_STATE;
                }
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0395));
            }
            RefreshTreePane(*state);
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_LAYER_MOVE_TOP: {
            InkpodDocumentInfo info{};
            const std::uint64_t target = state->Document().shell.smoke_layer_id != 0U
                ? state->Document().shell.smoke_layer_id
                : (state->Workspace().panes.active_tree_layer_id != 0U
                          ? state->Workspace().panes.active_tree_layer_id
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0396));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_LAYER_MOVE_UP:
        case IDM_LAYER_MOVE_DOWN: {
            const int count = static_cast<int>(state->Workspace().panes.tree_layer_count);
            const int current = static_cast<int>(state->Workspace().panes.active_tree_layer_index);
            const int destination = LOWORD(wparam) == IDM_LAYER_MOVE_UP
                ? std::max(0, current - 1)
                : std::min(std::max(0, count - 1), current + 1);
            std::uint64_t ignored{};
            const InkpodStatus status = ApplyTreeEdit(
                *state,
                INKPOD_TREE_REORDER_LAYER,
                state->Workspace().panes.active_tree_layer_id,
                static_cast<std::uint32_t>(destination),
                ignored);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0393));
            }
            RefreshTreePane(*state);
            return 0;
        }
        case IDM_LAYER_TOGGLE_VISIBLE:
        case IDM_LAYER_TOGGLE_EDITABLE:
        case IDM_LAYER_OPACITY: {
            InkpodStatus status{};
            bool grouped{};
            if (LOWORD(wparam) != IDM_LAYER_OPACITY) {
                TreePaneNode node{};
                if (!QueryTreeNode(*state, false, node)) {
                    status = INKPOD_STATUS_INVALID_STATE;
                } else {
                    const bool visible = LOWORD(wparam) == IDM_LAYER_TOGGLE_VISIBLE;
                    const bool value = visible
                        ? (node.flags & INKPOD_NODE_VISIBLE) == 0U
                        : (node.flags & INKPOD_NODE_EDITABLE) == 0U;
                    status = ApplyGroupedEditTargetCommand(
                        *state,
                        visible ? INKPOD_EDIT_TARGET_SET_VISIBILITY
                                : INKPOD_EDIT_TARGET_SET_EDITABILITY,
                        value ? 1U : 0U,
                        0U,
                        0U,
                        grouped);
                }
            }
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = SetSelectedTreeNodeProperties(
                    *state, false, LOWORD(wparam));
            }
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0404));
            }
            RefreshTreePane(*state);
            return 0;
        }
        case IDM_LAYER_PROPERTIES: {
            const InkpodStatus status = EditSelectedTreeNodeProperties(*state, false);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0404));
            }
            RefreshTreePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LAYER_CONVERT: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0405);
            dialog.labels[0] = UiText(UiStringId::Text0613);
            dialog.values[0] = INKPOD_LAYER_RASTER;
            dialog.choices[0] = LayerKindChoices().data();
            dialog.choice_counts[0] =
                static_cast<std::uint32_t>(LayerKindChoices().size());
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CONVERT_LAYER;
            edit.object_id = state->Workspace().panes.active_tree_layer_id;
            edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
            std::uint64_t ignored{};
            bool grouped{};
            InkpodStatus status = ApplyGroupedEditTargetCommand(
                *state,
                INKPOD_EDIT_TARGET_CONVERT_LAYERS,
                0U,
                edit.kind,
                0U,
                grouped);
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = ApplyTreeEditRecord(*state, edit, {}, ignored);
            }
            if (status == INKPOD_STATUS_OK
                && !state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0405));
            }
            RefreshTreePane(*state);
            return 0;
        }
        case IDM_LAYER_MERGE: {
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_MERGE_LAYER;
            edit.object_id = state->Workspace().panes.active_tree_layer_id;
            std::uint64_t ignored{};
            bool grouped{};
            InkpodStatus status = ApplyGroupedEditTargetCommand(
                *state, INKPOD_EDIT_TARGET_MERGE, 0U, 0U, 0U, grouped);
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = ApplyTreeEditRecord(*state, edit, {}, ignored);
            }
            if (status == INKPOD_STATUS_OK
                && !state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0566));
            }
            RefreshTreePane(*state);
            return 0;
        }
        case IDM_LAYER_DELETE_HIDDEN: {
            InkpodStatus status = state->engine == nullptr
                ? INKPOD_STATUS_INVALID_STATE
                : state->engine->Invoke(
                      [](InkpodCore* core) {
                          InkpodTreeEdit edit{};
                          edit.struct_size = sizeof(edit);
                          edit.operation = INKPOD_TREE_DELETE_HIDDEN_LAYERS;
                          InkpodDispatchResult result{};
                          result.struct_size = sizeof(result);
                          std::uint64_t ignored{};
                          return inkpod_core_tree_edit(core, &edit, &result, &ignored);
                      },
                      true,
                      true);
            if (status == INKPOD_STATUS_OK
                && !state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text1036));
            }
            RefreshTreePane(*state);
            return 0;
        }
        case IDM_PLANE_NEW: {
            PlaneDialogChoiceStorage choice_storage{};
            if (!LoadPlaneDialogChoices(state->lifetime.instance, choice_storage)) {
                if (!state->lifetime.smoke_test) {
                    MessageBoxW(
                        window,
                        UiText(UiStringId::Text0322),
                        UiText(UiStringId::Text0713),
                        MB_OK | MB_ICONERROR);
                }
                return 0;
            }
            PlaneCreationValidationContext validation{
                state->engine.get(),
                state->Workspace().panes.active_tree_layer_id,
                choice_storage.validation_error.c_str()};
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0713);
            dialog.labels = {UiText(UiStringId::Text0828), UiText(UiStringId::FormatLabel), UiText(UiStringId::Text0433), nullptr};
            dialog.values = {INKPOD_TYPED_PLANE_RASTER, INKPOD_STORAGE_RGBA8, 100, 0};
            dialog.choices[0] = choice_storage.kind_choices.data();
            dialog.choice_counts[0] =
                static_cast<std::uint32_t>(choice_storage.kind_choices.size());
            dialog.choices[1] = choice_storage.format_choices.data();
            dialog.choice_counts[1] =
                static_cast<std::uint32_t>(choice_storage.format_choices.size());
            dialog.validation_context = &validation;
            dialog.validate = ValidatePlaneCreationOptions;
            dialog.value_count = 3U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_CREATE_PLANE;
            edit.flags = INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
            edit.parent_id = state->Workspace().panes.active_tree_layer_id;
            edit.kind = static_cast<std::uint32_t>(dialog.values[0]);
            edit.pixel_format = static_cast<std::uint32_t>(dialog.values[1]);
            edit.opacity_milli = static_cast<std::uint32_t>(
                std::clamp(dialog.values[2], 0, 100)) * 10U;
            std::uint64_t plane_id{};
            InkpodStatus status = ApplyTreeEditRecord(
                *state, edit, "Plane", plane_id);
            if (status == INKPOD_STATUS_OK
                && !state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0320));
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
            std::uint32_t destination = state->Workspace().panes.active_tree_plane_index;
            if (command == IDM_PLANE_MOVE_UP) {
                destination = destination == 0U ? 0U : destination - 1U;
            } else if (command == IDM_PLANE_MOVE_DOWN) {
                const int count = static_cast<int>(state->Workspace().panes.tree_plane_count);
                destination = static_cast<std::uint32_t>(std::min(
                    std::max(0, count - 1), static_cast<int>(destination) + 1));
            }
            std::uint64_t object_id{};
            bool grouped{};
            InkpodStatus status = INKPOD_STATUS_OK;
            if (command == IDM_PLANE_DUPLICATE || command == IDM_PLANE_DELETE) {
                status = ApplyGroupedEditTargetCommand(
                    *state,
                    command == IDM_PLANE_DUPLICATE
                        ? INKPOD_EDIT_TARGET_DUPLICATE
                        : INKPOD_EDIT_TARGET_DELETE,
                    0U,
                    0U,
                    0U,
                    grouped);
            }
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = ApplyTreeEdit(
                    *state,
                    operation,
                    state->Workspace().panes.active_tree_plane_id,
                    destination,
                    object_id);
            }
            if (status == INKPOD_STATUS_OK
                && !state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0328));
            }
            RefreshTreePane(*state);
            return 0;
        }
        case IDM_PLANE_TOGGLE_VISIBLE:
        case IDM_PLANE_TOGGLE_EDITABLE:
        case IDM_PLANE_OPACITY: {
            InkpodStatus status{};
            bool grouped{};
            if (LOWORD(wparam) != IDM_PLANE_OPACITY) {
                TreePaneNode node{};
                if (!QueryTreeNode(*state, true, node)) {
                    status = INKPOD_STATUS_INVALID_STATE;
                } else {
                    const bool visible = LOWORD(wparam) == IDM_PLANE_TOGGLE_VISIBLE;
                    const bool value = visible
                        ? (node.flags & INKPOD_NODE_VISIBLE) == 0U
                        : (node.flags & INKPOD_NODE_EDITABLE) == 0U;
                    status = ApplyGroupedEditTargetCommand(
                        *state,
                        visible ? INKPOD_EDIT_TARGET_SET_VISIBILITY
                                : INKPOD_EDIT_TARGET_SET_EDITABILITY,
                        value ? 1U : 0U,
                        0U,
                        0U,
                        grouped);
                }
            }
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = SetSelectedTreeNodeProperties(
                    *state, true, LOWORD(wparam));
            }
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0323));
            }
            RefreshTreePane(*state);
            return 0;
        }
        case IDM_PLANE_PROPERTIES: {
            const InkpodStatus status = EditSelectedTreeNodeProperties(*state, true);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0323));
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
            dialog.title = UiText(UiStringId::Text0327);
            dialog.labels = {UiText(UiStringId::Text0614), UiText(UiStringId::Text0612), UiText(UiStringId::Text0683), nullptr};
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
            bool grouped{};
            InkpodStatus status = ApplyGroupedEditTargetCommand(
                *state,
                INKPOD_EDIT_TARGET_CONVERT_PLANES,
                0U,
                edit.kind,
                edit.pixel_format,
                grouped);
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = ApplyTreeEditRecord(*state, edit, {}, ignored);
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0326));
            }
            RefreshTreePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_PLANE_MERGE: {
            InkpodTreeEdit edit{};
            edit.struct_size = sizeof(edit);
            edit.operation = INKPOD_TREE_MERGE_PLANE;
            edit.object_id = state->Workspace().panes.active_tree_plane_id;
            std::uint64_t ignored{};
            bool grouped{};
            InkpodStatus status = ApplyGroupedEditTargetCommand(
                *state, INKPOD_EDIT_TARGET_MERGE, 0U, 0U, 0U, grouped);
            if (status == INKPOD_STATUS_OK && !grouped) {
                status = ApplyTreeEditRecord(*state, edit, {}, ignored);
            }
            if (status == INKPOD_STATUS_OK
                && !state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                status = INKPOD_STATUS_INVALID_STATE;
            }
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0565));
            }
            RefreshTreePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteAnimationCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_LT_SET_NEW:
        case IDM_LT_SET_RENAME: {
            TextInputDialogState dialog{};
            dialog.title = LOWORD(wparam) == IDM_LT_SET_NEW
                ? UiText(UiStringId::Text0366)
                : UiText(UiStringId::Text0367);
            dialog.label = UiText(UiStringId::Text0567);
            dialog.value = state->lifetime.smoke_test ? L"Smoke Set" : L"Light Table";
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            InkpodLightTableEdit edit{};
            edit.operation = LOWORD(wparam) == IDM_LT_SET_NEW
                ? INKPOD_LIGHT_TABLE_CREATE_SET
                : INKPOD_LIGHT_TABLE_RENAME_SET;
            edit.object_id = state->Workspace().panes.active_light_table_set_id;
            std::uint64_t object_id{};
            const InkpodStatus status = ApplyLightTableEdit(
                *state, context, edit, dialog.value, object_id);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::LightTableSetsAccessibleName));
            } else if (object_id != 0U) {
                state->Workspace().panes.active_light_table_set_id = object_id;
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
            edit.object_id = state->Workspace().panes.active_light_table_set_id;
            edit.destination_index = state->Workspace().panes.active_light_table_set_index;
            if (command == IDM_LT_SET_UP && edit.destination_index != 0U) {
                --edit.destination_index;
            } else if (command == IDM_LT_SET_DOWN) {
                ++edit.destination_index;
            }
            std::uint64_t object_id{};
            const InkpodStatus status = ApplyLightTableEdit(
                *state, context, edit, {}, object_id);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0368));
            } else if (object_id != 0U) {
                state->Workspace().panes.active_light_table_set_id = object_id;
            } else {
                state->Workspace().panes.active_light_table_set_id = 0U;
            }
            RefreshLightTablePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LT_GLOBAL_OPACITY: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0372);
            dialog.labels[0] = UiText(UiStringId::Text0434);
            dialog.values[0] = state->lifetime.smoke_test ? 50 : 100;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.values[0] < 0 || dialog.values[0] > 100) {
                return 0;
            }
            const std::uint32_t opacity =
                static_cast<std::uint32_t>(dialog.values[0]) * 10U;
            const InkpodStatus status = state->engine->Invoke(
                context.document_session.value(),
                context.generation.value(),
                [opacity](InkpodCore* core) {
                    InkpodDispatchResult result{};
                    result.struct_size = sizeof(result);
                    return inkpod_core_light_table_set_global_opacity(
                        core, opacity, &result);
                },
                true,
                true);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0372));
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
                *state,
                context,
                path,
                LOWORD(wparam) == IDM_LT_ITEM_RELOAD);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0375));
            }
            RefreshLightTablePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LT_BULK_PREVIOUS:
        case IDM_LT_BULK_NEXT:
        case IDM_LT_BULK_BOTH: {
            const InkpodLightTableBulkDirection direction =
                LOWORD(wparam) == IDM_LT_BULK_PREVIOUS
                ? INKPOD_LIGHT_TABLE_BULK_PREVIOUS
                : (LOWORD(wparam) == IDM_LT_BULK_NEXT
                       ? INKPOD_LIGHT_TABLE_BULK_NEXT
                       : INKPOD_LIGHT_TABLE_BULK_BOTH);
            const InkpodStatus status = RegisterSequenceNeighborsInLightTable(
                *state, context, direction);
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0369));
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
            edit.object_id = state->Workspace().panes.active_light_table_item_id;
            edit.destination_index = state->Workspace().panes.active_light_table_item_index;
            if (LOWORD(wparam) == IDM_LT_ITEM_UP && edit.destination_index != 0U) {
                --edit.destination_index;
            } else if (LOWORD(wparam) == IDM_LT_ITEM_DOWN) {
                ++edit.destination_index;
            }
            std::uint64_t ignored{};
            const InkpodStatus status = ApplyLightTableEdit(
                *state, context, edit, {}, ignored);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0381));
            }
            state->Workspace().panes.active_light_table_item_id = 0U;
            RefreshLightTablePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LT_ITEM_PROPERTIES: {
            const InkpodStatus status = EditLightTableItemProperties(
                *state, context);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0380));
            }
            RefreshLightTablePane(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LT_ITEM_MOVE:
            if (state->Workspace().panes.active_light_table_item_id == 0U) {
                return 0;
            }
            if (context.document_session != state->routing.targets.DocumentSession()
                || context.document_view
                    != state->routing.targets.ActiveDocumentView()
                || context.generation
                    != state->routing.targets.CurrentGeneration()) {
                return 0;
            }
            if (SetEditorActiveTool(*state, kInteractionLightTableMove)
                != INKPOD_STATUS_OK) {
                return 0;
            }
            state->Workspace().panes.light_table_move_context = context;
            state->Workspace().panes.light_table_move_samples.clear();
            UpdateMenuState(*state);
            return 1;
        case IDM_LT_ITEM_SAMPLE: {
            InkpodDocumentInfo document{};
            QueryDocument(*state, document);
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0376);
            dialog.labels = {
                UiText(UiStringId::AxisX),
                UiText(UiStringId::AxisY),
                nullptr,
                nullptr};
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
                context.document_session.value(),
                context.generation.value(),
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0376));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_LT_ITEM_SWAP: {
            const std::uint64_t item_id =
                state->Workspace().panes.active_light_table_item_id;
            if (item_id == 0U) {
                return 0;
            }
            InkpodDocumentInfo before{};
            before.struct_size = sizeof(before);
            if (!state->engine->GetDocumentInfo(
                    context.document_session.value(),
                    context.generation.value(),
                    before)) {
                return 0;
            }
            if ((before.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
                int choice{};
                if (state->lifetime.smoke_test) {
                    if (state->lifetime.smoke_dirty_prompt_count != UINT32_MAX) {
                        ++state->lifetime.smoke_dirty_prompt_count;
                    }
                    choice = state->lifetime.smoke_dirty_prompt_choice;
                } else {
                    choice = MessageBoxW(
                        state->Workspace().light_table_palette != nullptr
                            ? state->Workspace().light_table_palette
                            : window,
                        UiText(UiStringId::Text0787),
                        UiText(UiStringId::LightTable),
                        MB_OKCANCEL | MB_ICONQUESTION);
                }
                if (choice != IDOK || !context.document_view.has_value()
                    || !ActivateDocumentTab(
                        *state, context.document_view.value())) {
                    return 0;
                }
                const InkpodStatus save_status = SaveDocument(*state, false);
                if (save_status != INKPOD_STATUS_OK) {
                    return 0;
                }
            }
            InkpodDocumentInfo info{};
            InkpodStatus status = state->engine->Invoke(
                context.document_session.value(),
                context.generation.value(),
                [item_id, &info](InkpodCore* core) {
                    info = EmptyDocumentInfo();
                    return inkpod_core_light_table_swap(core, item_id, &info);
                },
                false,
                false);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0363));
            } else {
                ResetUiForNewActiveDocument(*state);
                if (!state->RefreshEditorPresentation(
                        context.document_session.value(),
                        context.generation.value())) {
                    status = INKPOD_STATUS_INVALID_STATE;
                    ShowCoreError(
                        *state,
                        window,
                        UiText(UiStringId::Text0371));
                } else {
                    FitCanvas(*state, INKPOD_VIEW_FIT);
                }
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
            const InkpodStatus status = context.document_session.has_value()
                    && context.generation.has_value()
                ? ImportSequencePaths(
                      *state,
                      paths,
                      context.document_session.value(),
                      context.generation.value())
                : INKPOD_STATUS_INVALID_STATE;
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0969));
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
            dialog.title = UiText(UiStringId::Text0968);
            dialog.labels[0] = UiText(UiStringId::Text0817);
            dialog.values[0] = state->lifetime.smoke_test ? 1 : 0;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            const InkpodStatus status = ExportSequenceToPath(
                *state, path, dialog.values[0] != 0);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0968));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SEQ_WRAP_ENDPOINTS: {
            const SequenceEndpointPolicy policy =
                state->lifetime.sequence_endpoint_policy
                    == SequenceEndpointPolicy::Wrap
                ? SequenceEndpointPolicy::Stop
                : SequenceEndpointPolicy::Wrap;
            if (!SaveSequenceEndpointPolicy(policy)) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0963),
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
                return 0;
            }
            state->lifetime.sequence_endpoint_policy = policy;
            PresentStatusBarPart(
                state->Workspace().windows.status_bar,
                5U,
                policy == SequenceEndpointPolicy::Wrap
                    ? UiText(UiStringId::Text0541)
                    : UiText(UiStringId::Text0540));
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_SEQ_PREVIOUS:
        case IDM_SEQ_NEXT: {
            const bool next = LOWORD(wparam) == IDM_SEQ_NEXT;
            const InkpodStatus status = SwitchSequenceTarget(
                *state,
                context,
                std::nullopt,
                next ? INKPOD_SEQUENCE_NEXT : INKPOD_SEQUENCE_PREVIOUS);
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0539));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SEQ_GOTO: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0231);
            dialog.labels[0] = UiText(UiStringId::Text0230);
            dialog.values[0] = state->lifetime.smoke_test ? 3 : 1;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.values[0] < 0) {
                return 0;
            }
            const std::uint32_t number = static_cast<std::uint32_t>(dialog.values[0]);
            std::uint32_t selected{};
            const InkpodStatus query_status = state->engine != nullptr
                    && context.document_session.has_value()
                    && context.generation.has_value()
                ? state->engine->Invoke(
                      context.document_session.value(),
                      context.generation.value(),
                      [number, &selected](InkpodCore* core) {
                          for (std::uint32_t index = 0U; index < 10000U; ++index) {
                              InkpodSequenceCellInfo cell{};
                              cell.struct_size = sizeof(cell);
                              const InkpodStatus query =
                                  inkpod_core_sequence_cell_get(
                                      core, index, &cell);
                              if (query != INKPOD_STATUS_OK) {
                                  return INKPOD_STATUS_INVALID_ARGUMENT;
                              }
                              if (cell.cell_number == number) {
                                  selected = index;
                                  return INKPOD_STATUS_OK;
                              }
                          }
                          return INKPOD_STATUS_INVALID_ARGUMENT;
                      },
                      false,
                      false)
                : INKPOD_STATUS_INVALID_STATE;
            const InkpodStatus status = query_status == INKPOD_STATUS_OK
                ? SwitchSequenceTarget(*state, context, selected)
                : query_status;
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0233));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SUBPALETTE_SET: {
            const std::uint32_t index = state->Workspace().animation.active_sequence_index;
            const InkpodStatus status = !context.document_session.has_value()
                    || !context.generation.has_value()
                ? INKPOD_STATUS_INVALID_STATE
                : state->engine->Invoke(
                context.document_session.value(),
                context.generation.value(),
                [index](InkpodCore* core) {
                    return inkpod_core_subpalette_set(core, index);
                },
                false,
                false);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0190));
            } else {
                (void)RefreshSubpalettePane(*state);
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_SUBPALETTE_SAMPLE: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0191);
            dialog.labels = {
                UiText(UiStringId::AxisX),
                UiText(UiStringId::AxisY),
                nullptr,
                nullptr};
            dialog.values = {0, 0, 0, 0};
            dialog.value_count = 2U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.values[0] < 0 || dialog.values[1] < 0) {
                return 0;
            }
            InkpodColorValue color{};
            color.struct_size = sizeof(color);
            const InkpodStatus status = !context.document_session.has_value()
                    || !context.generation.has_value()
                ? INKPOD_STATUS_INVALID_STATE
                : state->engine->Invoke(
                context.document_session.value(),
                context.generation.value(),
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
                (void)RefreshColorPanes(*state);
                RefreshDockPaneViews(*state);
            } else if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0191));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_MOTION_START: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0353);
            dialog.labels = {
                UiText(UiStringId::MotionFpsValuesLabel),
                UiText(UiStringId::Text0389),
                UiText(UiStringId::Text1002),
                UiText(UiStringId::Text0066)};
            dialog.values = {
                static_cast<std::int32_t>(state->Workspace().animation.motion_fps),
                (state->Workspace().animation.motion_flags & INKPOD_MOTION_FLAG_LOOP) != 0U ? 1 : 0,
                (state->Workspace().animation.motion_flags & INKPOD_MOTION_FLAG_INCLUDE_SELECTION) != 0U ? 1 : 0,
                (state->Workspace().animation.motion_flags & INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE) != 0U ? 1 : 0};
            dialog.value_count = 4U;
            if (ShowViewOptions(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            state->Workspace().animation.motion_fps = static_cast<std::uint32_t>(dialog.values[0]);
            state->Workspace().animation.motion_flags = (dialog.values[1] != 0 ? INKPOD_MOTION_FLAG_LOOP : 0U)
                | (dialog.values[2] != 0
                       ? INKPOD_MOTION_FLAG_INCLUDE_SELECTION
                       : 0U)
                | (dialog.values[3] != 0
                       ? INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE
                       : 0U);
            InkpodMotionCheckInput input{
                sizeof(InkpodMotionCheckInput),
                state->Workspace().animation.motion_fps,
                state->Workspace().animation.motion_flags};
            InkpodMotionFrame frame{};
            frame.struct_size = sizeof(frame);
            const InkpodStatus status = state->engine->Invoke(
                [input, &frame](InkpodCore* core) {
                    return inkpod_core_motion_check_start(core, &input, &frame);
                },
                false,
                false);
            if (status == INKPOD_STATUS_OK) {
                state->Workspace().animation.motion_active = true;
                UpdateMotionState(state->Workspace().animation, frame);
                ArmCommandTimer(
                    *state,
                    window,
                    CommandTimerKind::MotionPlayback,
                    std::max<UINT>(1U, 1000U / state->Workspace().animation.motion_fps));
            } else {
                ShowCoreError(*state, window, UiText(UiStringId::Text0354));
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
                UpdateMotionState(state->Workspace().animation, frame);
                if (state->Workspace().animation.motion_paused) {
                    DisarmCommandTimer(
                        *state, window, CommandTimerKind::MotionPlayback);
                } else {
                    ArmCommandTimer(
                        *state,
                        window,
                        CommandTimerKind::MotionPlayback,
                        std::max<UINT>(1U, 1000U / state->Workspace().animation.motion_fps));
                }
            } else {
                ShowCoreError(*state, window, UiText(UiStringId::Text0356));
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
                UpdateMotionState(state->Workspace().animation, frame);
            } else {
                ShowCoreError(*state, window, UiText(UiStringId::Text0355));
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
                UpdateMotionState(state->Workspace().animation, frame);
            } else {
                ShowCoreError(*state, window, UiText(UiStringId::Text0358));
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
            state->Workspace().animation.motion_fps = command == IDM_MOTION_FPS_30
                ? 30U
                : (command == IDM_MOTION_FPS_25
                          ? 25U
                          : (command == IDM_MOTION_FPS_24
                                    ? 24U
                                    : (command == IDM_MOTION_FPS_12
                                              ? 12U
                                              : (command == IDM_MOTION_FPS_10 ? 10U : 8U))));
            if (!state->Workspace().animation.motion_active) {
                UpdateMenuState(*state);
                return 1;
            }
            const InkpodMotionCheckInput input{
                sizeof(InkpodMotionCheckInput),
                state->Workspace().animation.motion_fps,
                state->Workspace().animation.motion_flags};
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
                UpdateMotionState(state->Workspace().animation, frame);
                ArmCommandTimer(
                    *state,
                    window,
                    CommandTimerKind::MotionPlayback,
                    std::max<UINT>(1U, 1000U / state->Workspace().animation.motion_fps));
            } else {
                ShowCoreError(*state, window, UiText(UiStringId::Text0351));
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
            DisarmCommandTimer(
                *state, window, CommandTimerKind::MotionPlayback);
            state->Workspace().animation.motion_active = false;
            state->Workspace().animation.motion_paused = false;
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0357));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteSelectionViewCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_SELECTION_OPTIONS:
            return inkpod::windows::ui::panes::ShowToolOptionsFlyout(
                       state->Workspace().windows.tool_options_flyout,
                       inkpod::windows::ui::ToolPaletteCheckedOptionsAnchor(
                           state->Workspace().tools.palette),
                       IDM_SELECTION_OPTIONS)
                ? 1
                : 0;
        case IDM_SELECTION_RECTANGLE:
        case IDM_SELECTION_ELLIPSE:
        case IDM_SELECTION_LASSO:
        case IDM_SELECTION_POLYLINE:
        case IDM_SELECTION_TRACE:
        case IDM_SELECTION_WAND: {
            const UINT command = LOWORD(wparam);
            const InkpodSelectionShape shape = command == IDM_SELECTION_ELLIPSE
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
            std::uint16_t tolerance =
                state->Workspace().tools.selection_tolerance;
            std::uint16_t gap_close =
                state->Workspace().tools.selection_gap_close;
            float diameter = state->Workspace().tools.selection_diameter;
            InkpodTraceBrushShape trace_shape =
                state->Workspace().tools.selection_trace_shape;
            std::uint64_t construction_flags =
                state->Workspace().tools.selection_construction_flags;
            if (command == IDM_SELECTION_WAND || command == IDM_SELECTION_TRACE) {
                ViewOptionsDialogState dialog{};
                dialog.title = command == IDM_SELECTION_WAND
                    ? UiText(UiStringId::Text0868)
                    : UiText(UiStringId::Text0981);
                dialog.labels = command == IDM_SELECTION_WAND
                    ? std::array<const wchar_t*, 4U>{
                          UiText(UiStringId::Text0918), UiText(UiStringId::Text1030), nullptr, nullptr}
                    : std::array<const wchar_t*, 4U>{
                          UiText(UiStringId::Text0820), UiText(UiStringId::Text0651), UiText(UiStringId::Text0837), UiText(UiStringId::Text0814)};
                dialog.values[0] = command == IDM_SELECTION_WAND
                    ? state->Workspace().tools.selection_tolerance
                    : static_cast<std::int32_t>(state->Workspace().tools.selection_diameter);
                dialog.values[1] = state->Workspace().tools.selection_gap_close;
                static const std::array<ViewOptionsDialogState::Choice, 2U> kTraceShapes{
                    ViewOptionsDialogState::Choice{UiText(UiStringId::Text0445), INKPOD_TRACE_ROUND},
                    ViewOptionsDialogState::Choice{UiText(UiStringId::Text0906), INKPOD_TRACE_SQUARE}};
                static const std::array<ViewOptionsDialogState::Choice, 2U> kBooleanChoices{
                    ViewOptionsDialogState::Choice{UiText(UiStringId::Text0776), 0},
                    ViewOptionsDialogState::Choice{UiText(UiStringId::Text0740), 1}};
                if (command == IDM_SELECTION_TRACE) {
                    dialog.values[1] = static_cast<std::int32_t>(trace_shape);
                    dialog.values[2] = (construction_flags
                            & INKPOD_SELECTION_TRACE_PRESSURE_SIZE)
                        != 0U;
                    dialog.values[3] = (construction_flags
                            & INKPOD_SELECTION_TRACE_SCREEN_SIZE)
                        != 0U;
                    dialog.choices[1] = kTraceShapes.data();
                    dialog.choice_counts[1] =
                        static_cast<std::uint32_t>(kTraceShapes.size());
                    dialog.choices[2] = kBooleanChoices.data();
                    dialog.choice_counts[2] =
                        static_cast<std::uint32_t>(kBooleanChoices.size());
                    dialog.choices[3] = kBooleanChoices.data();
                    dialog.choice_counts[3] =
                        static_cast<std::uint32_t>(kBooleanChoices.size());
                }
                dialog.value_count = command == IDM_SELECTION_WAND ? 2U : 4U;
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
                    tolerance = static_cast<std::uint16_t>(dialog.values[0]);
                    gap_close = static_cast<std::uint16_t>(dialog.values[1]);
                } else if (dialog.values[0] > 0) {
                    diameter = static_cast<float>(dialog.values[0]);
                    trace_shape = static_cast<InkpodTraceBrushShape>(dialog.values[1]);
                    construction_flags &= ~(INKPOD_SELECTION_TRACE_PRESSURE_SIZE
                        | INKPOD_SELECTION_TRACE_SCREEN_SIZE);
                    if (dialog.values[2] != 0) {
                        construction_flags |= INKPOD_SELECTION_TRACE_PRESSURE_SIZE;
                    }
                    if (dialog.values[3] != 0) {
                        construction_flags |= INKPOD_SELECTION_TRACE_SCREEN_SIZE;
                    }
                }
            }
            state->Workspace().tools.selection_shape = shape;
            state->Workspace().tools.selection_tolerance = tolerance;
            state->Workspace().tools.selection_gap_close = gap_close;
            state->Workspace().tools.selection_diameter = diameter;
            state->Workspace().tools.selection_trace_shape = trace_shape;
            state->Workspace().tools.selection_construction_flags = construction_flags;
            if (SetEditorSelectionOptions(*state) != INKPOD_STATUS_OK
                || SetEditorActiveTool(*state, kInteractionSelection)
                    != INKPOD_STATUS_OK) {
                (void)state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation);
                return 0;
            }
            CancelSelectionGeometryPreview(
                state->Workspace().tools, state->Workspace().windows.canvas);
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_SELECTION_MODE_NEW:
        case IDM_SELECTION_MODE_ADD:
        case IDM_SELECTION_MODE_SUBTRACT:
        case IDM_SELECTION_MODE_INTERSECT:
            state->Workspace().tools.selection_operation = LOWORD(wparam) == IDM_SELECTION_MODE_ADD
                ? INKPOD_SELECTION_ADD
                : (LOWORD(wparam) == IDM_SELECTION_MODE_SUBTRACT
                          ? INKPOD_SELECTION_SUBTRACT
                          : (LOWORD(wparam) == IDM_SELECTION_MODE_INTERSECT
                                    ? INKPOD_SELECTION_INTERSECT
                                    : INKPOD_SELECTION_NEW));
            if (SetEditorSelectionOptions(*state) != INKPOD_STATUS_OK) {
                (void)state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation);
            }
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
                ShowCoreError(*state, window, UiText(UiStringId::Text1003));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0674));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_SELECTION_OUTPUT_COLOR_GUARD: {
            static const std::array<ViewOptionsDialogState::Choice, 1U> kProfiles{{
                {UiText(UiStringId::Text0041),
                 INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR}}};
            static const std::array<ViewOptionsDialogState::Choice, 4U> kOperations{{
                {UiText(UiStringId::Text0706), INKPOD_SELECTION_NEW},
                {UiText(UiStringId::Text0951), INKPOD_SELECTION_ADD},
                {UiText(UiStringId::Delete), INKPOD_SELECTION_SUBTRACT},
                {UiText(UiStringId::Text0452), INKPOD_SELECTION_INTERSECT}}};
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0517);
            dialog.labels = {UiText(UiStringId::Text0331), UiText(UiStringId::Text0989), nullptr, nullptr};
            dialog.values = {
                static_cast<std::int32_t>(state->effects.output_color_guard_profile),
                static_cast<std::int32_t>(state->Workspace().tools.selection_operation),
                0,
                0};
            dialog.choices = {kProfiles.data(), kOperations.data(), nullptr, nullptr};
            dialog.choice_counts = {
                static_cast<std::uint32_t>(kProfiles.size()),
                static_cast<std::uint32_t>(kOperations.size()),
                0U,
                0U};
            dialog.value_count = 2U;
            if (ShowViewOptions(
                    state->lifetime.instance,
                    window,
                    state->lifetime.smoke_test,
                    dialog)
                != IDOK) {
                return 0;
            }
            const auto profile_setting =
                static_cast<OutputColorGuardProfileSetting>(dialog.values[0]);
            if (!state->lifetime.smoke_test
                && !SaveOutputColorGuardProfileSetting(profile_setting)) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0520),
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
                return 0;
            }
            InkpodDocumentInfo document{};
            if (!QueryDocument(*state, document)) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0517));
                return 0;
            }
            std::shared_ptr<OutputColorGuardJob> job;
            try {
                job = std::make_shared<OutputColorGuardJob>();
            } catch (const std::bad_alloc&) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0517));
                return 0;
            }
            state->effects.output_color_guard_profile =
                static_cast<InkpodOutputColorGuardProfile>(dialog.values[0]);
            job->request.profile = state->effects.output_color_guard_profile;
            job->request.operation =
                static_cast<InkpodSelectionOperation>(dialog.values[1]);
            job->request.base_document_revision = document.document_revision;
            state->effects.output_color_guard = job;
            const InkpodStatus status = StartEffectTask(
                *state,
                context,
                false,
                [job](InkpodCore* core, InkpodTask* task) {
                    return inkpod_core_select_output_color_guard(
                        core, &job->request, task, &job->result);
                });
            if (status != INKPOD_STATUS_OK) {
                state->effects.output_color_guard.reset();
                ShowCoreError(*state, window, UiText(UiStringId::Text0517));
            } else if (state->lifetime.smoke_test) {
                (void)FormatOutputColorGuardSummary(
                    job->result, state->effects.last_output_color_guard_summary);
                state->effects.output_color_guard.reset();
            }
            UpdateMenuState(*state);
            if (status == INKPOD_STATUS_OK && state->lifetime.smoke_test
                && !state->effects.last_output_color_guard_summary.empty()) {
                PresentStatusBarPart(
                    state->Workspace().windows.status_bar,
                    5U,
                    state->effects.last_output_color_guard_summary.c_str());
            }
            return 0;
        }
        case IDM_SELECTION_TO_LAYER: {
            // This becomes document data, so keep it language-neutral rather
            // than deriving it from the process UI language.
            static constexpr std::array<std::uint8_t, 11U> name{
                'S', 'e', 'l', 'e', 'c', 't', 'i', 'o', 'n', ' ', '1'};
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0994));
            } else {
                state->Document().shell.selection_layer_id = layer_id;
                state->Document().shell.smoke_layer_id = layer_id;
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
            const std::uint64_t layer_id = state->Document().shell.selection_layer_id;
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0983));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_SELECTION_ALL: {
            InkpodDocumentInfo info{};
            const InkpodEditorStateInfo* editor = PresentedEditorState(*state);
            InkpodSelectionInput input{};
            input.struct_size = sizeof(input);
            input.shape = INKPOD_SELECTION_RECTANGLE;
            input.operation = INKPOD_SELECTION_NEW;
            input.interpretation = INKPOD_RANGE_NORMAL;
            input.trace_shape = INKPOD_TRACE_ROUND;
            input.view_zoom_q16 = INT64_C(1) << 16;
            const bool queried = QueryDocument(*state, info);
            if (queried) {
                input.bounds = {
                    0,
                    0,
                    static_cast<std::int32_t>(info.width),
                    static_cast<std::int32_t>(info.height)};
            }
            const InkpodStatus status = !queried || state->engine == nullptr
                    || editor == nullptr
                    || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U
                    || editor->active_layer_id == 0U
                    || editor->active_plane_id == 0U
                ? INKPOD_STATUS_INVALID_STATE
                : state->engine->Invoke(
                      [input,
                       layer_id = editor->active_layer_id,
                       plane_id = editor->active_plane_id](InkpodCore* core) {
                          InkpodDispatchResult result{};
                          result.struct_size = sizeof(result);
                          return inkpod_core_apply_selection_for_editor_target(
                              core, layer_id, plane_id, &input, &result);
                      },
                      true,
                      true);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0110));
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
                    ? UiText(UiStringId::Text0996)
                    : UiText(UiStringId::Text0998);
                dialog.labels[0] = UiText(UiStringId::Text0646);
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0992));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_VIEW_ZOOM_IN:
        case IDM_VIEW_ZOOM_OUT: {
            RECT client{};
            GetClientRect(state->Workspace().windows.canvas, &client);
            const double factor = LOWORD(wparam) == IDM_VIEW_ZOOM_IN ? 1.2 : 1.0 / 1.2;
            if (ApplyView(
                    *state,
                    INKPOD_VIEW_ZOOM_AT,
                    factor,
                    static_cast<double>(client.right) / 2.0,
                    static_cast<double>(client.bottom) / 2.0)
                != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0892));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0884));
            }
            UpdateMenuState(*state);
            return 0;
        case IDM_VIEW_ZOOM_PERCENT: {
            InkpodSnapshotTransform transform{};
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0890);
            dialog.labels[0] = UiText(UiStringId::Text0474);
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
                    ShowCoreError(*state, window, UiText(UiStringId::Text0695));
                }
                UpdateMenuState(*state);
            }
            return 0;
        }
        case IDM_VIEW_BOX_ZOOM:
            if (SetEditorActiveTool(*state, kInteractionBoxZoom)
                != INKPOD_STATUS_OK) {
                return 0;
            }
            state->ActiveView().presentation.gesture_samples.clear();
            UpdateMenuState(*state);
            return 0;
        case IDM_VIEW_FLIP_HORIZONTAL:
        case IDM_VIEW_FLIP_VERTICAL: {
            const bool horizontal =
                LOWORD(wparam) == IDM_VIEW_FLIP_HORIZONTAL;
            const InkpodStatus status = ApplyView(
                *state,
                context,
                horizontal ? INKPOD_VIEW_FLIP_HORIZONTAL
                           : INKPOD_VIEW_FLIP_VERTICAL,
                0.0,
                0.0);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0894));
            } else if (horizontal) {
                state->ActiveView().presentation.flip_horizontal =
                    !state->ActiveView().presentation.flip_horizontal;
            } else {
                state->ActiveView().presentation.flip_vertical = !state->ActiveView().presentation.flip_vertical;
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
                ? &state->ActiveView().presentation.ruler_visible
                : (command == IDM_VIEW_GUIDES
                          ? &state->ActiveView().presentation.guides_visible
                          : (command == IDM_VIEW_GRID
                                    ? &state->ActiveView().presentation.grid_visible
                                    : (command == IDM_VIEW_SNAP_GUIDES
                                              ? &state->ActiveView().presentation.snap_guides
                                              : (command == IDM_VIEW_SNAP_GRID
                                                        ? &state->ActiveView().presentation.snap_grid
                                                        : &state->ActiveView().presentation.transparent_visible))));
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0896));
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
            dialog.title = vertical ? UiText(UiStringId::Text0586) : UiText(UiStringId::Text0764);
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0167));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_VIEW_GUIDE_MOVE:
            if (SetEditorActiveTool(*state, kInteractionGuideMove)
                != INKPOD_STATUS_OK) {
                return 0;
            }
            UpdateMenuState(*state);
            return 0;
        case IDM_VIEW_GUIDE_DELETE_ALL: {
            const InkpodStatus status = DeleteAllGuides(*state);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0166));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_VIEW_GRID_SETTINGS: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0178);
            dialog.labels = {UiText(UiStringId::Text0086), UiText(UiStringId::Text0087), UiText(UiStringId::Text1025), UiText(UiStringId::Text0521)};
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0178));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_TAB_NEXT:
        case IDM_TAB_PREVIOUS: {
            const HWND tabs = state->Workspace().windows.document_tabs;
            const int count = tabs == nullptr ? 0 : TabCtrl_GetItemCount(tabs);
            const int selected = tabs == nullptr ? -1 : TabCtrl_GetCurSel(tabs);
            if (count <= 1 || selected < 0) {
                return 0;
            }
            const int next = LOWORD(wparam) == IDM_TAB_NEXT
                ? (selected + 1) % count
                : (selected + count - 1) % count;
            TCITEMW item{};
            item.mask = TCIF_PARAM;
            if (TabCtrl_GetItem(tabs, next, &item) != FALSE
                && ActivateDocumentTab(
                    *state,
                    DocumentViewId{
                        static_cast<std::uint64_t>(item.lParam)})) {
                TabCtrl_SetCurSel(tabs, next);
            }
            return 0;
        }
        case IDM_TAB_MOVE_LEFT:
            return MoveActiveTabBy(*state, -1) ? 1 : 0;
        case IDM_TAB_MOVE_RIGHT:
            return MoveActiveTabBy(*state, 1) ? 1 : 0;
        case IDM_VIEW_CLOSE:
            return CloseActiveView(*state) ? 1 : 0;
        case IDM_DOCUMENT_CLOSE:
            return CloseActiveDocument(*state) ? 1 : 0;
        case IDM_VIEW_NEW:
            return CreateDocumentViewInGroup(
                *state, state->routing.targets.EditorGroup(), window)
                ? 1
                : 0;
        case IDM_EDITOR_SPLIT_RIGHT:
            return SplitEditorArea(
                *state, EditorSplitOrientation::Vertical, window)
                ? 1
                : 0;
        case IDM_EDITOR_SPLIT_DOWN:
            return SplitEditorArea(
                *state, EditorSplitOrientation::Horizontal, window)
                ? 1
                : 0;
        case IDM_EDITOR_MOVE_OTHER_GROUP:
            return MoveActiveViewToOtherGroup(*state) ? 1 : 0;
        case IDM_EDITOR_NEW_VIEW_OTHER_GROUP: {
            const auto* active = state->Workspace().editors.Active();
            const auto* other = active == nullptr
                ? nullptr
                : state->Workspace().editors.Other(active->id);
            if (other == nullptr) {
                return SplitEditorArea(
                    *state, EditorSplitOrientation::Vertical, window)
                    ? 1
                    : 0;
            }
            return CreateDocumentViewInGroup(*state, other->id, window) ? 1 : 0;
        }
        case IDM_EDITOR_GROUP_CLOSE:
            return CloseActiveEditorGroup(*state) ? 1 : 0;
        case IDM_EDITOR_GROUP_NEXT: {
            const auto* active = state->Workspace().editors.Active();
            const auto* other = active == nullptr
                ? nullptr
                : state->Workspace().editors.Other(active->id);
            if (other == nullptr || !ActivateEditorGroup(*state, other->id)) {
                return 0;
            }
            const auto* activated = state->Workspace().editors.Active();
            const HWND focus_target = activated == nullptr
                ? nullptr
                : (activated->focus_history != nullptr
                       ? activated->focus_history
                       : activated->canvas);
            if (focus_target != nullptr) {
                SetFocus(focus_target);
            }
            return 1;
        }
        case IDM_VIEW_MOVE_NEW_WINDOW:
            return MoveOrDuplicateViewToNewWorkspace(
                *state, context, false) ? 1 : 0;
        case IDM_VIEW_DUPLICATE_NEW_WINDOW:
            return MoveOrDuplicateViewToNewWorkspace(
                *state, context, true) ? 1 : 0;
        case IDM_VIEW_MOVE_NEXT_WINDOW:
            return MoveOrDuplicateViewToNextWorkspace(
                *state, context, false) ? 1 : 0;
        case IDM_VIEW_DUPLICATE_NEXT_WINDOW:
            return MoveOrDuplicateViewToNextWorkspace(
                *state, context, true) ? 1 : 0;
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteToolCommand(
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext&) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_TOOL_PALETTE:
            if (state->Workspace().tools.palette != nullptr) {
                return state->Workspace().windows.dock_host.TogglePane(
                           DockPaneType::Tool)
                        == DockResult::Ok
                    ? 1
                    : 0;
            }
            return 0;
        case IDM_WINDOW_TOOL_OPTIONS:
        {
            const bool toggled =
                inkpod::windows::ui::panes::ToggleToolOptionsFlyout(
                    state->Workspace().windows.tool_options_flyout,
                    inkpod::windows::ui::ToolPaletteCheckedOptionsAnchor(
                        state->Workspace().tools.palette),
                    ActiveToolOptionsCommand(*state));
            UpdateMenuState(*state);
            return toggled ? 1 : 0;
        }
        case IDM_TOOL_PENCIL:
            (void)SetEditorActiveTool(*state, INKPOD_TOOL_PENCIL);
            UpdateMenuState(*state);
            return 0;
        case IDM_TOOL_BRUSH:
            (void)SetEditorActiveTool(*state, INKPOD_TOOL_BRUSH);
            UpdateMenuState(*state);
            return 0;
        case IDM_TOOL_ERASER:
            (void)SetEditorActiveTool(*state, INKPOD_TOOL_ERASER);
            UpdateMenuState(*state);
            return 0;
        case IDM_TOOL_FILL:
        case IDM_TOOL_CLOSED_FILL:
        case IDM_TOOL_FILL_EXTENSION: {
            if (state->Workspace().tools.active_tool == kInteractionFill) {
                CancelFillGeometryPreview(
                    state->Workspace().tools, state->Workspace().windows.canvas);
            }
            auto options = state->Workspace().tools.fill_options;
            options.operation = LOWORD(wparam) == IDM_TOOL_CLOSED_FILL
                ? INKPOD_FILL_CLOSED_REGION
                : (LOWORD(wparam) == IDM_TOOL_FILL_EXTENSION
                          ? INKPOD_FILL_EXTENSION
                          : INKPOD_FILL_SEED);
            if (SetEditorFillOptions(*state, options) != INKPOD_STATUS_OK
                || SetEditorActiveTool(*state, kInteractionFill)
                    != INKPOD_STATUS_OK) {
                return 0;
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_TOOL_FILL_OPTIONS: {
            return inkpod::windows::ui::panes::ShowToolOptionsFlyout(
                       state->Workspace().windows.tool_options_flyout,
                       inkpod::windows::ui::ToolPaletteCheckedOptionsAnchor(
                           state->Workspace().tools.palette),
                       IDM_TOOL_FILL)
                ? 1
                : 0;
        }
        case IDM_TOOL_EYEDROPPER:
            (void)SetEditorActiveTool(*state, kInteractionEyedropper);
            UpdateMenuState(*state);
            return 0;
        case IDM_GEOMETRY_LINE:
        case IDM_GEOMETRY_CURVE:
        case IDM_GEOMETRY_RECTANGLE:
        case IDM_GEOMETRY_ELLIPSE:
        case IDM_GEOMETRY_POLYGON:
        case IDM_GEOMETRY_POLYLINE: {
            TreePaneNode plane{};
            if (!QueryTreeNode(*state, true, plane)
                || !IsGeometryCanvasPlane(plane.kind)) {
                return 0;
            }
            const UINT command = LOWORD(wparam);
            const std::uint32_t tool = command == IDM_GEOMETRY_LINE
                ? kInteractionGeometryLine
                : (command == IDM_GEOMETRY_CURVE
                          ? kInteractionGeometryCurve
                          : (command == IDM_GEOMETRY_RECTANGLE
                                    ? kInteractionGeometryRectangle
                                    : (command == IDM_GEOMETRY_ELLIPSE
                                              ? kInteractionGeometryEllipse
                                              : (command == IDM_GEOMETRY_POLYGON
                                                        ? kInteractionGeometryPolygon
                                                        : kInteractionGeometryPolyline))));
            CancelCoreRasterGeometryPreview(*state);
            const InkpodStatus status = SetEditorActiveTool(*state, tool);
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_TOOL_COLOR_REPLACE_TARGET: {
            if (!state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                return 0;
            }
            const InkpodEditorStateInfo* editor = PresentedEditorState(*state);
            if (editor == nullptr
                || (editor->flags & INKPOD_EDITOR_STATE_HAS_TARGET) == 0U) {
                return 0;
            }
            state->Workspace().tools.color_replace_target = editor->current_color;
            state->Workspace().tools.color_replace_target.struct_size =
                sizeof(InkpodColorValue);
            return 1;
        }
        case IDM_TOOL_COLOR_REPLACE_PEN:
        case IDM_TOOL_COLOR_REPLACE_RECTANGLE:
        case IDM_TOOL_COLOR_REPLACE_POLYLINE:
        case IDM_TOOL_COLOR_REPLACE_LASSO: {
            TreePaneNode plane{};
            if (!QueryTreeNode(*state, true, plane)) {
                return 0;
            }
            const auto mode = ColorReplaceModeForPlane(plane.kind);
            if (!mode.has_value()) {
                if (state->engine != nullptr) {
                    state->engine->SetLocalFailure(
                        UiText(UiStringId::Text0877));
                }
                return 0;
            }
            const UINT command = LOWORD(wparam);
            state->Workspace().tools.color_replace_shape =
                command == IDM_TOOL_COLOR_REPLACE_RECTANGLE
                ? INKPOD_SELECTION_RECTANGLE
                : (command == IDM_TOOL_COLOR_REPLACE_POLYLINE
                          ? INKPOD_SELECTION_POLYLINE
                          : (command == IDM_TOOL_COLOR_REPLACE_LASSO
                                    ? INKPOD_SELECTION_LASSO
                                    : INKPOD_SELECTION_TRACE));
            state->Workspace().tools.color_replace_mode = mode.value();
            if (SetEditorActiveTool(*state, kInteractionColorReplace)
                != INKPOD_STATUS_OK) {
                return 0;
            }
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_TOOL_COLOR_REPLACE_ALL: {
            if (!state->RefreshEditorPresentation(
                    state->Document().id, state->Document().generation)) {
                return 0;
            }
            const InkpodEditorStateInfo* editor = PresentedEditorState(*state);
            TreePaneNode plane{};
            if (editor == nullptr || !QueryTreeNode(*state, true, plane)) {
                return 0;
            }
            const auto mode = ColorReplaceModeForPlane(plane.kind);
            if (!mode.has_value()) {
                return 0;
            }
            state->Workspace().tools.color_replace_mode = mode.value();
            InkpodLocatorOutput locator{};
            locator.struct_size = sizeof(locator);
            const InkpodStatus locator_status = state->engine == nullptr
                ? INKPOD_STATUS_INVALID_STATE
                : state->engine->Invoke(
                      [&locator](InkpodCore* core) {
                          return inkpod_core_locator_sample(
                              core, 0U, 0.0, 0.0, &locator);
                      },
                      false,
                      false);
            if (locator_status != INKPOD_STATUS_OK) {
                return 0;
            }
            if ((locator.flags & INKPOD_LOCATOR_SELECTION_PRESENT) == 0U
                && !state->lifetime.smoke_test
                && MessageBoxW(
                       window,
                       UiText(UiStringId::Text0990),
                       UiText(UiStringId::ToolColorReplacement),
                       MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2)
                    != IDYES) {
                return 0;
            }
            const InkpodStatus status = ApplyColorReplace(
                *state, editor, false, {});
            if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                ShowCoreError(*state, window, UiText(UiStringId::ToolColorReplacement));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_EFFECT_GRADIENT:
        case IDM_EFFECT_AIRBRUSH:
        case IDM_EFFECT_BOUNDARY_AIRBRUSH:
        case IDM_EFFECT_BLUR:
        case IDM_EFFECT_STAMP:
        case IDM_EFFECT_DUST:
        case IDM_EFFECT_ALPHA_GRADIENT:
            if (LOWORD(wparam) == IDM_EFFECT_BOUNDARY_AIRBRUSH) {
                return inkpod::windows::ui::panes::ShowToolOptionsFlyout(
                           state->Workspace().windows.tool_options_flyout,
                           inkpod::windows::ui::ToolPaletteCheckedOptionsAnchor(
                               state->Workspace().tools.palette),
                           IDM_EFFECT_BOUNDARY_AIRBRUSH)
                    ? 1
                    : 0;
            }
            if (state->Workspace().tools.active_plane == INKPOD_PLANE_COLOR
                && SelectCanvasEffect(*state, LOWORD(wparam))) {
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0130));
            }
            UpdateMenuState(*state);
            return 0;
        }
        case IDM_PLANE_MAIN_LINE:
        case IDM_PLANE_COLOR: {
            const InkpodPlaneKind plane = LOWORD(wparam) == IDM_PLANE_MAIN_LINE
                ? INKPOD_PLANE_MAIN_LINE
                : INKPOD_PLANE_COLOR;
            InkpodDocumentInfo info = EmptyDocumentInfo();
            const InkpodStatus plane_status = QueryDocument(*state, info)
                ? SetEditorActiveTarget(
                      *state,
                      info.layer_id,
                      plane == INKPOD_PLANE_MAIN_LINE
                          ? info.main_plane_id
                          : info.color_plane_id)
                : INKPOD_STATUS_INVALID_STATE;
            if (plane_status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0325));
            } else {
                RefreshTreePane(*state);
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
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_WINDOW_COLOR_PANE:
            if (state->Workspace().windows.dock_host.TogglePane(
                    DockPaneType::Color)
                != DockResult::Ok) {
                return 0;
            }
            if (state->Workspace().windows.workspace.dock.IsPaneVisible(
                    DockPaneType::Color)) {
                RefreshColorPanes(*state);
            }
            return 1;
        case IDM_COLOR_PIN: {
            const auto* binding = state->routing.pane_targets.Find(
                state->routing.color_pane);
            const PaneTargetStatus status = binding != nullptr
                    && binding->policy == PaneTargetPolicy::PinnedDocument
                ? state->routing.pane_targets.FollowActive(
                      state->routing.color_pane)
                : state->routing.pane_targets.PinDocument(
                      state->routing.color_pane,
                      context,
                      state->routing.targets);
            if (status == PaneTargetStatus::Ok
                || status == PaneTargetStatus::NoOp) {
                (void)RefreshColorPanes(*state);
                RefreshDockPaneViews(*state);
                UpdateMenuState(*state);
                return status == PaneTargetStatus::Ok ? 1 : 0;
            }
            return 0;
        }
        case IDM_COLOR_EDITOR: {
            const InkpodStatus status = ShowDrawingColorEditor(*state);
            if (status != INKPOD_STATUS_OK && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0679));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_COLOR_SOURCE_TOPMOST:
        case IDM_COLOR_SOURCE_SELECTED:
        case IDM_COLOR_SOURCE_COMPOSITE:
        case IDM_COLOR_SOURCE_LIGHT_TABLE:
            state->Workspace().tools.eyedropper_source = LOWORD(wparam) == IDM_COLOR_SOURCE_TOPMOST
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
            try {
                colors = state->Workspace().panes.palette_colors;
                if (LOWORD(wparam) == IDM_PALETTE_REGISTER) {
                    if (colors.size() >= 4096U) {
                        return 0;
                    }
                    colors.push_back(state->Workspace().tools.drawing_color);
                    state->Workspace().panes.selected_palette_index =
                        static_cast<std::uint32_t>(colors.size() - 1U);
                    state->Workspace().panes.palette_group =
                        state->Workspace().panes.selected_palette_index / 10U;
                } else if (LOWORD(wparam) == IDM_PALETTE_DELETE) {
                    const std::size_t index = state->Workspace().panes.selected_palette_index;
                    if (index >= colors.size()) {
                        return 0;
                    }
                    colors.erase(colors.begin() + static_cast<std::ptrdiff_t>(index));
                } else {
                    colors.clear();
                    state->Workspace().panes.selected_palette_index = 0U;
                }
            } catch (const std::bad_alloc&) {
                return 0;
            }
            const InkpodStatus status = ReplacePalette(*state, context, colors);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0164));
            }
            RefreshColorPanes(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_PALETTE_NEXT_GROUP:
            ++state->Workspace().panes.palette_group;
            RefreshColorPanes(*state);
            if (!state->Workspace().panes.palette_colors.empty()) {
                state->Workspace().panes.selected_palette_index = std::min<std::uint32_t>(
                    state->Workspace().panes.palette_group * 10U,
                    static_cast<std::uint32_t>(state->Workspace().panes.palette_colors.size() - 1U));
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
                return SavePaletteFile(path, state->Workspace().panes.palette_colors) ? 1 : 0;
            }
            std::vector<InkpodColorValue> colors;
            if (!LoadPaletteFile(path, colors)) {
                return 0;
            }
            const InkpodStatus status = ReplacePalette(*state, context, colors);
            if (status == INKPOD_STATUS_OK) {
                RefreshColorPanes(*state);
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_GENERATE: {
            ViewOptionsDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0222);
            dialog.labels = {UiText(UiStringId::Text0734), UiText(UiStringId::Text1007), nullptr, nullptr};
            dialog.values = {256, 2, 0, 0};
            dialog.value_count = 2U;
            if (state->lifetime.smoke_test) {
                dialog.values = {16, 4, 0, 0};
            }
            if (ShowViewOptions(
                    state->lifetime.instance,
                    window,
                    state->lifetime.smoke_test,
                    dialog) != IDOK
                || dialog.values[0] <= 0 || dialog.values[0] > 4096
                || dialog.values[1] < 0 || dialog.values[1] > 7) {
                return 0;
            }
            const InkpodStatus status = StartColorChartGeneration(
                *state,
                context,
                static_cast<std::uint32_t>(dialog.values[0]),
                static_cast<std::uint32_t>(dialog.values[1]));
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0160));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_SEARCH: {
            TextInputDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0159);
            dialog.label = UiText(UiStringId::Text0568);
            dialog.value = state->lifetime.smoke_test ? L"Smoke" : L"";
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK) {
                return 0;
            }
            wchar_t* end{};
            const unsigned long number = std::wcstoul(dialog.value.c_str(), &end, 10);
            std::size_t index = SIZE_MAX;
            if (end != dialog.value.c_str() && *end == L'\0' && number != 0U
                && number <= state->Workspace().panes.color_chart_colors.size()) {
                index = number - 1U;
            } else {
                std::wstring needle = dialog.value;
                std::transform(
                    needle.begin(), needle.end(), needle.begin(),
                    [](wchar_t value) { return std::towlower(value); });
                for (std::size_t candidate = 0U;
                     candidate < state->Workspace().panes.color_chart_names.size(); ++candidate) {
                    std::wstring name = state->Workspace().panes.color_chart_names[candidate];
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
            state->Workspace().panes.color_chart_page = static_cast<std::uint32_t>(index / 20U);
            state->Workspace().panes.selected_color_chart_index = static_cast<std::uint32_t>(index);
            RefreshColorPanes(*state);
            return 1;
        }
        case IDM_CHART_NEXT: {
            if (state->Workspace().panes.color_chart_colors.empty()) {
                return 0;
            }
            std::size_t index = static_cast<std::size_t>(
                state->Workspace().panes.selected_color_chart_index) + 1U;
            index %= state->Workspace().panes.color_chart_colors.size();
            state->Workspace().panes.color_chart_page = static_cast<std::uint32_t>(index / 20U);
            state->Workspace().panes.selected_color_chart_index = static_cast<std::uint32_t>(index);
            RefreshColorPanes(*state);
            return 1;
        }
        case IDM_CHART_LOCK: {
            const InkpodStatus status = ReplaceColorChart(
                *state,
                context,
                state->Workspace().panes.color_chart_colors,
                state->Workspace().panes.color_chart_names,
                !state->Workspace().panes.color_chart_locked);
            if (status == INKPOD_STATUS_OK) {
                RefreshColorPanes(*state);
            } else {
                ShowCoreError(*state, window, UiText(UiStringId::Text0157));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_NEXT_PAGE:
            ++state->Workspace().panes.color_chart_page;
            RefreshColorPanes(*state);
            if (!state->Workspace().panes.color_chart_colors.empty()) {
                state->Workspace().panes.selected_color_chart_index = std::min<std::uint32_t>(
                    state->Workspace().panes.color_chart_page * 20U,
                    static_cast<std::uint32_t>(state->Workspace().panes.color_chart_colors.size() - 1U));
            }
            return 1;
        case IDM_CHART_RENAME: {
            if (state->Workspace().panes.color_chart_locked) {
                return 0;
            }
            const std::size_t index = state->Workspace().panes.selected_color_chart_index;
            if (index >= state->Workspace().panes.color_chart_names.size()) {
                return 0;
            }
            TextInputDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0158);
            dialog.label = UiText(UiStringId::Text0567);
            dialog.value = state->lifetime.smoke_test ? L"Smoke Color" : state->Workspace().panes.color_chart_names[index];
            if (ShowTextInput(
                    state->lifetime.instance, window, state->lifetime.smoke_test, dialog) != IDOK
                || dialog.value.empty() || dialog.value.size() > 256U) {
                return 0;
            }
            std::vector<std::wstring> names;
            try {
                names = state->Workspace().panes.color_chart_names;
                names[index] = dialog.value;
            } catch (const std::bad_alloc&) {
                return 0;
            }
            const InkpodStatus status = ReplaceColorChart(
                *state,
                context,
                state->Workspace().panes.color_chart_colors,
                names,
                false);
            if (status == INKPOD_STATUS_OK) {
                RefreshColorPanes(*state);
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
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
                           path, state->Workspace().panes.color_chart_colors, state->Workspace().panes.color_chart_names)
                    ? 1
                    : 0;
            }
            std::vector<InkpodColorValue> colors;
            std::vector<std::wstring> names;
            if (!LoadColorChartFile(path, colors, names)) {
                return 0;
            }
            if (state->Workspace().panes.color_chart_locked) {
                return 0;
            }
            const InkpodStatus status = ReplaceColorChart(
                *state, context, colors, names, false);
            if (status == INKPOD_STATUS_OK) {
                state->Workspace().panes.color_chart_page = 0U;
                state->Workspace().panes.selected_color_chart_index = 0U;
                RefreshColorPanes(*state);
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_COPY: {
            const std::size_t index = state->Workspace().panes.selected_color_chart_index;
            if (index >= state->Workspace().panes.color_chart_colors.size()) {
                return 0;
            }
            const auto& selected_color = state->Workspace().panes.color_chart_colors[index];
            std::array<wchar_t, 384U> text{};
            _snwprintf_s(
                text.data(), text.size(), _TRUNCATE,
                selected_color.depth == INKPOD_COLOR_DEPTH_16
                    ? L"%ls\t#%04X%04X%04X%04X"
                    : L"%ls\t#%02X%02X%02X%02X",
                state->Workspace().panes.color_chart_names[index].c_str(),
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
            if (state->Workspace().panes.color_chart_locked
                || RouteColorCommand(
                       state, window, IDM_CHART_COPY, 0, context)
                        .value_or(0)
                    != 1) {
                return 0;
            }
            const std::size_t index = state->Workspace().panes.selected_color_chart_index;
            if (index >= state->Workspace().panes.color_chart_colors.size()) {
                return 0;
            }
            std::vector<InkpodColorValue> colors;
            std::vector<std::wstring> names;
            try {
                colors = state->Workspace().panes.color_chart_colors;
                names = state->Workspace().panes.color_chart_names;
                colors.erase(colors.begin() + static_cast<std::ptrdiff_t>(index));
                names.erase(names.begin() + static_cast<std::ptrdiff_t>(index));
            } catch (const std::bad_alloc&) {
                return 0;
            }
            const InkpodStatus status = ReplaceColorChart(
                *state, context, colors, names, false);
            if (status == INKPOD_STATUS_OK) {
                RefreshColorPanes(*state);
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CHART_PASTE: {
            if (state->Workspace().panes.color_chart_locked || OpenClipboard(window) == FALSE) {
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
            std::vector<InkpodColorValue> colors;
            std::vector<std::wstring> names;
            try {
                colors = state->Workspace().panes.color_chart_colors;
                names = state->Workspace().panes.color_chart_names;
                if (colors.size() >= 4096U) {
                    return 0;
                }
                colors.push_back(color);
                names.push_back(pasted_name.empty() ? L"Pasted" : pasted_name);
            } catch (const std::bad_alloc&) {
                return 0;
            }
            const InkpodStatus status = ReplaceColorChart(
                *state, context, colors, names, false);
            if (status == INKPOD_STATUS_OK) {
                SetDrawingColor(*state, color);
                state->Workspace().panes.color_chart_page = static_cast<std::uint32_t>(
                    (colors.size() - 1U) / 20U);
                state->Workspace().panes.selected_color_chart_index = static_cast<std::uint32_t>(
                    colors.size() - 1U);
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
                (state->Workspace().tools.color_rgba >> 24) & 0xffU,
                (state->Workspace().tools.color_rgba >> 16) & 0xffU,
                (state->Workspace().tools.color_rgba >> 8) & 0xffU);
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
                        static_cast<std::uint16_t>(state->Workspace().tools.color_rgba & 0xffU)});
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0654));
            } else {
                state->ActiveView().presentation.color_check_mode = mode;
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
    ApplicationHost* state,
    HWND window,
    WPARAM wparam,
    LPARAM,
    const CommandContext& context) noexcept {
    if (state == nullptr) {
        return std::nullopt;
    }
    switch (LOWORD(wparam)) {
        case IDM_FILE_NEW_CUT: {
            const InkpodStatus status = CreateNewCut(*state, window);
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0710));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CUT_PROPERTIES: {
            const InkpodStatus status = EditCutProperties(*state, window);
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0146));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CUT_SAVE: {
            const InkpodStatus status = SaveWorkspaceCut(*state);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0149));
            }
            UpdateMenuState(*state);
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CUT_UNDO:
        case IDM_CUT_REDO: {
            const InkpodStatus status = MoveCutHistory(
                *state, LOWORD(wparam) == IDM_CUT_REDO);
            if (status != INKPOD_STATUS_OK) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0154));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_CUT_SEQUENCE_ADD:
        case IDM_CUT_SEQUENCE_REMOVE:
        case IDM_CUT_SEQUENCE_MOVE_UP:
        case IDM_CUT_SEQUENCE_MOVE_DOWN:
        case IDM_CUT_SEQUENCE_RENUMBER: {
            InkpodStatus status = INKPOD_STATUS_INVALID_STATE;
            switch (LOWORD(wparam)) {
                case IDM_CUT_SEQUENCE_ADD:
                    status = AddCutSequenceMember(*state, window);
                    break;
                case IDM_CUT_SEQUENCE_REMOVE:
                    status = RemoveCutSequenceMember(*state);
                    break;
                case IDM_CUT_SEQUENCE_MOVE_UP:
                    status = MoveCutSequenceMember(*state, false);
                    break;
                case IDM_CUT_SEQUENCE_MOVE_DOWN:
                    status = MoveCutSequenceMember(*state, true);
                    break;
                case IDM_CUT_SEQUENCE_RENUMBER:
                    status = RenumberCutSequence(*state, window);
                    break;
                default:
                    break;
            }
            if (status != INKPOD_STATUS_OK
                && status != INKPOD_STATUS_CANCELLED) {
                ShowCoreError(*state, window, UiText(UiStringId::Text0144));
            }
            return status == INKPOD_STATUS_OK ? 1 : 0;
        }
        case IDM_WINDOW_JOB_PROGRESS: {
            const bool show = !state->Workspace().windows.workspace.dock
                                   .IsPaneVisible(DockPaneType::JobProgress);
            if (state->Workspace().windows.dock_host.TogglePane(
                    DockPaneType::JobProgress)
                != DockResult::Ok) {
                return 0;
            }
            if (show) {
                static_cast<void>(state->Workspace().windows.dock_host.ActivatePane(
                    DockPaneType::JobProgress));
            }
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_FILE_RESTORE_PREVIOUS: {
            const bool enabled = !state->lifetime.restore_previous_documents;
            if (!SaveRestorePreviousDocumentsSetting(enabled)
                || (!enabled && !ClearPreviousDocumentPaths())) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0535),
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
                return 0;
            }
            state->lifetime.restore_previous_documents = enabled;
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_FILE_SEQUENCE_AUTOSAVE: {
            const SequenceCellSwitchPolicy policy =
                state->lifetime.sequence_switch_policy
                    == SequenceCellSwitchPolicy::AutosaveBeforeSwitch
                ? SequenceCellSwitchPolicy::Prompt
                : SequenceCellSwitchPolicy::AutosaveBeforeSwitch;
            if (!SaveSequenceCellSwitchPolicy(policy)) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0229),
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
                return 0;
            }
            state->lifetime.sequence_switch_policy = policy;
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_WORKSPACE_NEW_WINDOW:
            return CreateWorkspaceWindow(*state, true) != nullptr ? 1 : 0;
        case IDM_WINDOW_LOCATOR:
            if (state->Workspace().locator_palette != nullptr) {
                const bool shown = ToggleAuxiliaryPaneVisibility(
                    *state, WorkspaceAuxiliaryPane::Locator);
                if (shown) {
                    RefreshLocatorPane(*state);
                    QueueLocatorSample(*state);
                    FocusPaneWindow(
                        state->Workspace().locator_palette,
                        IDC_LOCATOR_PIN);
                }
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_LOCATOR_PIN: {
            const auto* binding = state->routing.pane_targets.Find(
                state->routing.locator_pane);
            const PaneTargetStatus status = binding != nullptr
                    && binding->policy == PaneTargetPolicy::PinnedDocument
                ? state->routing.pane_targets.FollowActive(
                      state->routing.locator_pane)
                : state->routing.pane_targets.PinDocument(
                      state->routing.locator_pane,
                      context,
                      state->routing.targets);
            if (status == PaneTargetStatus::Ok
                || status == PaneTargetStatus::NoOp) {
                RefreshLocatorPane(*state);
                QueueLocatorSample(*state);
                UpdateMenuState(*state);
                return status == PaneTargetStatus::Ok ? 1 : 0;
            }
            return 0;
        }
        case IDM_LOCATOR_FIXED:
            state->Workspace().locator_fixed_mode =
                !state->Workspace().locator_fixed_mode;
            RefreshLocatorPane(*state);
            UpdateMenuState(*state);
            return 1;
        case IDM_LOCATOR_AUTOSCROLL:
            state->Workspace().locator_auto_scroll =
                !state->Workspace().locator_auto_scroll;
            RefreshLocatorPane(*state);
            UpdateMenuState(*state);
            return 1;
        case IDM_WINDOW_SEQUENCE:
            if (state->Workspace().sequence_palette != nullptr) {
                const bool shown = ToggleAuxiliaryPaneVisibility(
                    *state, WorkspaceAuxiliaryPane::Sequence);
                if (shown) {
                    RefreshSequencePane(*state);
                    FocusPaneWindow(
                        state->Workspace().sequence_palette,
                        IDC_SEQUENCE_CELLS);
                }
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_SEQUENCE_PIN: {
            const auto* binding = state->routing.pane_targets.Find(
                state->routing.sequence_pane);
            const PaneTargetStatus status = binding != nullptr
                    && binding->policy == PaneTargetPolicy::PinnedDocument
                ? state->routing.pane_targets.FollowActive(
                      state->routing.sequence_pane)
                : state->routing.pane_targets.PinDocument(
                      state->routing.sequence_pane,
                      context,
                      state->routing.targets);
            if (status == PaneTargetStatus::Ok
                || status == PaneTargetStatus::NoOp) {
                RefreshSequencePane(*state);
                UpdateMenuState(*state);
                return status == PaneTargetStatus::Ok ? 1 : 0;
            }
            return 0;
        }
        case IDM_WINDOW_LIGHT_TABLE:
            if (state->Workspace().light_table_palette != nullptr) {
                const bool shown = ToggleAuxiliaryPaneVisibility(
                    *state, WorkspaceAuxiliaryPane::LightTable);
                if (shown) {
                    RefreshLightTablePane(*state);
                    FocusPaneWindow(
                        state->Workspace().light_table_palette,
                        state->Workspace().panes.light_table_item_count != 0U
                            ? IDC_LIGHT_TABLE_ITEMS
                            : IDC_LIGHT_TABLE_SETS);
                }
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_LIGHT_TABLE_PIN: {
            const auto* binding = state->routing.pane_targets.Find(
                state->routing.light_table_pane);
            const PaneTargetStatus status = binding != nullptr
                    && binding->policy == PaneTargetPolicy::PinnedDocument
                ? state->routing.pane_targets.FollowActive(
                      state->routing.light_table_pane)
                : state->routing.pane_targets.PinDocument(
                      state->routing.light_table_pane,
                      context,
                      state->routing.targets);
            if (status == PaneTargetStatus::Ok
                || status == PaneTargetStatus::NoOp) {
                RefreshLightTablePane(*state);
                UpdateMenuState(*state);
                return status == PaneTargetStatus::Ok ? 1 : 0;
            }
            return 0;
        }
        case IDM_WINDOW_SUBPALETTE:
            if (state->Workspace().subpalette_palette != nullptr) {
                const bool shown = ToggleAuxiliaryPaneVisibility(
                    *state, WorkspaceAuxiliaryPane::Reference);
                if (shown) {
                    (void)RefreshSubpalettePane(*state);
                    FocusPaneWindow(state->Workspace().subpalette_palette, 0);
                    SetFocus(
                        state->Workspace().subpalette_dialog.canvas != nullptr
                        ? state->Workspace().subpalette_dialog.canvas
                        : GetDlgItem(
                              state->Workspace().subpalette_palette,
                              IDC_SUBPALETTE_PIN));
                }
                UpdateMenuState(*state);
                return 1;
            }
            return 0;
        case IDM_SUBPALETTE_PIN: {
            const auto* binding = state->routing.pane_targets.Find(
                state->routing.subpalette_pane);
            const PaneTargetStatus status = binding != nullptr
                    && binding->policy == PaneTargetPolicy::PinnedDocument
                ? state->routing.pane_targets.FollowActive(
                      state->routing.subpalette_pane)
                : state->routing.pane_targets.PinDocument(
                      state->routing.subpalette_pane,
                      context,
                      state->routing.targets);
            if (status == PaneTargetStatus::Ok
                || status == PaneTargetStatus::NoOp) {
                ResetSubpaletteTarget(*state);
                (void)RefreshSubpalettePane(*state);
                UpdateMenuState(*state);
                return status == PaneTargetStatus::Ok ? 1 : 0;
            }
            return 0;
        }
        case IDM_WORKSPACE_RESET:
            static_cast<void>(ApplyWorkspacePreset(
                state->Workspace().windows.workspace,
                WorkspacePreset::Coloring));
            PreserveActiveJobProgressPane(*state);
            ApplyOrDeferWorkspacePresentation(*state);
            return 1;
        case IDM_WORKSPACE_PRESET_COLORING:
        case IDM_WORKSPACE_PRESET_LINE_CLEANUP:
        case IDM_WORKSPACE_PRESET_REFERENCE:
        case IDM_WORKSPACE_PRESET_BATCH:
        case IDM_WORKSPACE_PRESET_FOCUS: {
            WorkspaceLayoutState& layout = state->Workspace().windows.workspace;
            const WorkspaceSplitOrientation orientation =
                CaptureSplitOrientation(*state);
            const std::uint32_t ratio =
                state->Workspace().editors.SplitRatioMilli();
            WorkspacePreset preset = WorkspacePreset::Coloring;
            switch (LOWORD(wparam)) {
                case IDM_WORKSPACE_PRESET_LINE_CLEANUP:
                    preset = WorkspacePreset::LineCleanup;
                    break;
                case IDM_WORKSPACE_PRESET_REFERENCE:
                    preset = WorkspacePreset::ReferenceCheck;
                    break;
                case IDM_WORKSPACE_PRESET_BATCH:
                    preset = WorkspacePreset::Batch;
                    break;
                case IDM_WORKSPACE_PRESET_FOCUS:
                    preset = WorkspacePreset::Focus;
                    break;
                default:
                    break;
            }
            if (!ApplyWorkspacePreset(layout, preset)) {
                return 0;
            }
            layout.split_orientation = orientation;
            layout.split_ratio_milli = ratio;
            PreserveActiveJobProgressPane(*state);
            ApplyOrDeferWorkspacePresentation(*state);
            return 1;
        }
        case IDM_WORKSPACE_SAVE: {
            CaptureWorkspacePresentation(*state);
            const auto saved_name = WorkspaceRegistryValueName(
                L"WorkspaceSavedV5", state->Workspace().persistence_slot);
            const bool saved = SaveWorkspaceLayout(
                state->Workspace().windows.workspace, saved_name.data());
            if (!saved && !state->lifetime.smoke_test) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0785),
                    L"inkpod",
                    MB_OK | MB_ICONWARNING);
            }
            return saved ? 1 : 0;
        }
        case IDM_WORKSPACE_SAVE_AS: {
            TextInputDialogState dialog{};
            dialog.title = UiText(UiStringId::Text0416);
            dialog.label = UiText(UiStringId::Text0567);
            const WorkspaceLayoutState& current =
                state->Workspace().windows.workspace;
            dialog.value = current.custom_name[0] == L'\0'
                ? UiText(UiStringId::Text0362)
                : current.custom_name.data();
            dialog.close_immediately = state->lifetime.smoke_test;
            if (ShowTextInput(
                    state->lifetime.instance,
                    window,
                    state->lifetime.smoke_test,
                    dialog)
                != IDOK) {
                return 0;
            }
            CaptureWorkspacePresentation(*state);
            WorkspaceLayoutState& layout = state->Workspace().windows.workspace;
            if (!SetWorkspaceCustomName(layout, dialog.value)) {
                if (!state->lifetime.smoke_test) {
                    MessageBoxW(
                        window,
                        UiText(UiStringId::Text0418),
                        L"inkpod",
                        MB_OK | MB_ICONWARNING);
                }
                return 0;
            }
            const auto saved_name = WorkspaceRegistryValueName(
                L"WorkspaceSavedV5", state->Workspace().persistence_slot);
            const bool saved = SaveWorkspaceLayout(layout, saved_name.data());
            UpdateMenuState(*state);
            return saved ? 1 : 0;
        }
        case IDM_WORKSPACE_RESTORE: {
            WorkspaceLayoutState restored = state->Workspace().windows.workspace;
            const auto saved_name = WorkspaceRegistryValueName(
                L"WorkspaceSavedV5", state->Workspace().persistence_slot);
            const auto legacy_saved_name = WorkspaceRegistryValueName(
                L"WorkspaceSavedV4", state->Workspace().persistence_slot);
            bool loaded = LoadWorkspaceLayout(restored, saved_name.data());
            if (!loaded
                && (LoadWorkspaceLayout(restored, legacy_saved_name.data())
                    || (state->Workspace().persistence_slot == 0U
                        && LoadWorkspaceLayout(restored, L"WorkspaceSavedV2")))) {
                if (SaveWorkspaceLayout(restored, saved_name.data())) {
                    static_cast<void>(DeleteWorkspaceLayout(
                        legacy_saved_name.data()));
                    static_cast<void>(DeleteWorkspaceLayout(L"WorkspaceSavedV2"));
                }
                loaded = true;
            }
            if (loaded) {
                state->Workspace().windows.workspace = restored;
                PreserveActiveJobProgressPane(*state);
                ApplyOrDeferWorkspacePresentation(*state);
            } else if (!state->lifetime.smoke_test) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0467),
                    L"inkpod",
                    MB_OK | MB_ICONINFORMATION);
            }
            return loaded ? 1 : 0;
        }
        case IDM_WORKSPACE_AUTOHIDE_LOCATOR:
        case IDM_WORKSPACE_AUTOHIDE_SEQUENCE:
        case IDM_WORKSPACE_AUTOHIDE_LIGHT_TABLE:
        case IDM_WORKSPACE_AUTOHIDE_REFERENCE:
        case IDM_WORKSPACE_AUTOHIDE_BATCH: {
            WorkspaceAuxiliaryPane type = WorkspaceAuxiliaryPane::Locator;
            switch (LOWORD(wparam)) {
                case IDM_WORKSPACE_AUTOHIDE_SEQUENCE:
                    type = WorkspaceAuxiliaryPane::Sequence;
                    break;
                case IDM_WORKSPACE_AUTOHIDE_LIGHT_TABLE:
                    type = WorkspaceAuxiliaryPane::LightTable;
                    break;
                case IDM_WORKSPACE_AUTOHIDE_REFERENCE:
                    type = WorkspaceAuxiliaryPane::Reference;
                    break;
                case IDM_WORKSPACE_AUTOHIDE_BATCH:
                    type = WorkspaceAuxiliaryPane::Batch;
                    break;
                default:
                    break;
            }
            auto* pane = inkpod::windows::ui::FindWorkspaceAuxiliaryPane(
                state->Workspace().windows.workspace, type);
            if (pane == nullptr) {
                return 0;
            }
            const DockPaneType dock_type =
                inkpod::windows::ui::DockPaneTypeForAuxiliary(type);
            const DockPanePlacement* placement =
                state->Workspace().windows.workspace.dock.Pane(dock_type);
            const bool enable = placement == nullptr
                || placement->zone != DockZone::AutoHide;
            if (state->Workspace().windows.dock_host.SetPaneAutoHide(
                    dock_type, enable)
                != DockResult::Ok) {
                return 0;
            }
            pane->auto_hide = enable;
            pane->visible = !enable;
            if (!enable) {
                static_cast<void>(
                    state->Workspace().windows.dock_host.ActivatePane(dock_type));
            }
            state->Workspace().windows.workspace.selected_preset =
                WorkspacePreset::Custom;
            RelayoutWorkspace(*state);
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_WORKSPACE_MIRROR:
            state->Workspace().windows.workspace.dock.SetMirrored(
                !state->Workspace().windows.workspace.dock.Mirrored());
            state->Workspace().windows.workspace.selected_preset =
                WorkspacePreset::Custom;
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
                    *state, window, UiText(UiStringId::Text0199), status);
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
                ShowCoreError(*state, window, UiText(UiStringId::Text0201));
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
                ShowShortcutError(*state, window, UiText(UiStringId::Text0202), status);
                if (status == INKPOD_STATUS_IO_ERROR) {
                    UpdateMenuState(*state);
                    return 1;
                }
                return 0;
            }
            UpdateMenuState(*state);
            return 1;
        }
        case IDM_LANGUAGE_SYSTEM:
        case IDM_LANGUAGE_JAPANESE:
        case IDM_LANGUAGE_ENGLISH: {
            UiLanguagePreference preference = UiLanguagePreference::System;
            if (LOWORD(wparam) == IDM_LANGUAGE_JAPANESE) {
                preference = UiLanguagePreference::Japanese;
            } else if (LOWORD(wparam) == IDM_LANGUAGE_ENGLISH) {
                preference = UiLanguagePreference::English;
            }
            if (preference == CurrentUiLanguagePreference()) {
                return 0;
            }
            if (!SaveUiLanguagePreference(preference)) {
                if (!state->lifetime.smoke_test) {
                    MessageBoxW(
                        window,
                        UiText(UiStringId::Text0912),
                        L"inkpod",
                        MB_OK | MB_ICONERROR);
                }
                return 0;
            }
            UpdateMenuState(*state);
            if (!state->lifetime.smoke_test) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0911),
                    L"inkpod",
                    MB_OK | MB_ICONINFORMATION);
            }
            return 1;
        }
        case IDM_HELP_MANUAL:
        case IDM_HELP_FILE_FORMAT:
        case IDM_HELP_ACKNOWLEDGEMENTS: {
            EmbeddedHelpDocument document{};
            UINT error_message{};
            switch (LOWORD(wparam)) {
                case IDM_HELP_MANUAL:
                    document = EmbeddedHelpDocument::Manual;
                    error_message = IDS_HELP_MANUAL_OPEN_FAILED;
                    break;
                case IDM_HELP_FILE_FORMAT:
                    document = EmbeddedHelpDocument::FileFormat;
                    error_message = IDS_HELP_FILE_FORMAT_OPEN_FAILED;
                    break;
                case IDM_HELP_ACKNOWLEDGEMENTS:
                    document = EmbeddedHelpDocument::Acknowledgements;
                    error_message = IDS_HELP_ACKNOWLEDGEMENTS_OPEN_FAILED;
                    break;
                default:
                    return 0;
            }
            std::wstring document_path;
            const EmbeddedHelpStatus status = state->lifetime.smoke_test
                ? inkpod::app::PrepareEmbeddedHelpDocument(
                      state->lifetime.instance, document, document_path)
                : inkpod::app::OpenEmbeddedHelpDocument(
                      state->lifetime.instance, window, document);
            if (status != EmbeddedHelpStatus::Ok) {
                ShowEmbeddedHelpError(*state, window, error_message);
                return 0;
            }
            return 1;
        }
        case IDM_HELP_WEB_PAGE: {
            if (state->lifetime.smoke_test) {
                return 1;
            }
            constexpr wchar_t kInkpodWebPage[] = L"https://shuichi.github.io/inkpod/";
            const HINSTANCE launched = ShellExecuteW(
                window, L"open", kInkpodWebPage, nullptr, nullptr, SW_SHOWNORMAL);
            if (reinterpret_cast<INT_PTR>(launched) <= 32) {
                ShowEmbeddedHelpError(*state, window, IDS_HELP_WEB_PAGE_OPEN_FAILED);
                return 0;
            }
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

std::optional<LRESULT> RouteWindowLifecycleMessage(
    ApplicationHost* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    switch (message) {
        case WM_CREATE:
            if (state == nullptr) {
                return -1;
            }
            if (state->renderer == nullptr) {
                return -1;
            }
            state->Workspace().windows.canvas = inkpod::renderer::CreateCanvasWindow(
                state->lifetime.instance,
                window,
                *state->renderer,
                state->routing.targets.Canvas(),
                state->routing.targets.CurrentGeneration());
            if (state->Workspace().windows.canvas == nullptr) {
                return -1;
            }
            if (auto* group = state->Workspace().editors.Active(); group != nullptr) {
                group->canvas = state->Workspace().windows.canvas;
                group->focus_history = group->canvas;
            }
            if (!InitializeMainChrome(*state)) {
                return -1;
            }
            return 0;
        case WM_SIZE:
            if (state != nullptr) {
                inkpod::windows::ui::LayoutMainChrome(
                    state->Workspace().windows,
                    state->lifetime.smoke_test,
                    LOWORD(lparam),
                    HIWORD(lparam));
            }
            return 0;
        case WM_NOTIFY:
            if (state != nullptr) {
                const auto* notification = reinterpret_cast<const NMHDR*>(lparam);
                for (std::size_t index = 0U;
                     notification != nullptr
                         && index < state->Workspace().editors.GroupCount();
                     ++index) {
                    auto* group = state->Workspace().editors.GroupAt(index);
                    if (group == nullptr
                        || notification->hwndFrom != group->document_tabs
                        || (notification->code != TCN_SELCHANGE
                            && notification->code != NM_SETFOCUS)) {
                        continue;
                    }
                    if (notification->code == NM_SETFOCUS) {
                        group->focus_history = group->document_tabs;
                    }
                    const int selected = TabCtrl_GetCurSel(group->document_tabs);
                    TCITEMW item{};
                    item.mask = TCIF_PARAM;
                    if (selected >= 0
                        && TabCtrl_GetItem(group->document_tabs, selected, &item) != FALSE) {
                        (void)ActivateDocumentTab(
                            *state,
                            DocumentViewId{
                                static_cast<std::uint64_t>(item.lParam)});
                    } else {
                        (void)ActivateEditorGroup(*state, group->id);
                    }
                    return 0;
                }
            }
            break;
        case WM_ACTIVATE:
            if (state != nullptr && LOWORD(wparam) != WA_INACTIVE) {
                CollapseAutoHiddenPanes(*state);
                UpdateMenuState(*state);
            }
            break;
        case WM_DISPLAYCHANGE:
        case WM_SETTINGCHANGE:
            if (state != nullptr) {
                CancelDocumentTabDrag(*state);
                CaptureWorkspacePresentation(*state);
                ClampWorkspaceOwnedWindows(*state);
                RelayoutWorkspace(*state);
            }
            RedrawWindow(
                window,
                nullptr,
                nullptr,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
            break;
        case WM_THEMECHANGED:
        case WM_SYSCOLORCHANGE:
            RedrawWindow(
                window,
                nullptr,
                nullptr,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
            break;
        case WM_DPICHANGED: {
            const auto* bounds = reinterpret_cast<const RECT*>(lparam);
            if (state != nullptr) {
                CancelDocumentTabDrag(*state);
            }
            SetWindowPos(
                window,
                nullptr,
                bounds->left,
                bounds->top,
                bounds->right - bounds->left,
                bounds->bottom - bounds->top,
                SWP_NOACTIVATE | SWP_NOZORDER);
            if (state != nullptr) {
                CaptureWorkspacePresentation(*state);
                ClampWorkspaceOwnedWindows(*state);
                RelayoutWorkspace(*state);
            }
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteKeyboardMessage(
    ApplicationHost* state,
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
                    DisarmCommandTimer(
                        *state, window, CommandTimerKind::ShortcutSequence);
                    UpdateMenuState(*state);
                    return 0;
                }
                if (state->Workspace().tools.floating_active && wparam == VK_RETURN) {
                    DispatchEnabledCommand(*state, window, IDM_EDIT_FLOATING_COMMIT);
                    return 0;
                }
                if (state->Workspace().tools.floating_active && wparam == VK_ESCAPE) {
                    DispatchEnabledCommand(*state, window, IDM_EDIT_FLOATING_CANCEL);
                    return 0;
                }
                if (state->Workspace().animation.motion_active && wparam == VK_ESCAPE) {
                    DispatchEnabledCommand(*state, window, IDM_MOTION_STOP);
                    return 0;
                }
                if (state->Workspace().animation.motion_active && wparam == VK_SPACE) {
                    DispatchEnabledCommand(*state, window, IDM_MOTION_PAUSE);
                    return 0;
                }
                if (state->Workspace().animation.motion_active
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
                if (HandleWorkspaceNavigation(
                        *state,
                        window,
                        static_cast<std::uint32_t>(wparam),
                        modifiers)) {
                    return 0;
                }
                UINT menu_command{};
                const InkpodShortcutMatch shortcut_match = ResolveShortcutStroke(
                    state->shortcuts,
                    InkpodShortcutStroke{static_cast<std::uint32_t>(wparam), modifiers},
                    menu_command);
                if (shortcut_match == INKPOD_SHORTCUT_MATCH_PREFIX) {
                    ArmCommandTimer(
                        *state,
                        window,
                        CommandTimerKind::ShortcutSequence,
                        kShortcutSequenceTimerMilliseconds);
                    UpdateMenuState(*state);
                    return 0;
                }
                DisarmCommandTimer(
                    *state, window, CommandTimerKind::ShortcutSequence);
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
                    const std::size_t index = state->Workspace().panes.palette_group * 10U + digit;
                    if (index < state->Workspace().panes.palette_colors.size()) {
                        state->Workspace().panes.selected_palette_index =
                            static_cast<std::uint32_t>(index);
                        SetDrawingColor(*state, state->Workspace().panes.palette_colors[index]);
                        InvalidateRect(state->Workspace().windows.canvas, nullptr, FALSE);
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
    ApplicationHost* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    switch (message) {
        case inkpod::renderer::kCanvasStrokeReady:
            if (state != nullptr) {
                inkpod::renderer::OwnedCanvasStrokeEvent owned_input{};
                inkpod::app::EditorGroup* source_group{};
                for (std::size_t index = 0U;
                     index < state->Workspace().editors.GroupCount();
                     ++index) {
                    auto* candidate = state->Workspace().editors.GroupAt(index);
                    if (candidate != nullptr
                        && inkpod::renderer::TakeCanvasStrokeEvent(
                            candidate->canvas,
                            static_cast<std::uint64_t>(wparam),
                            Generation(static_cast<std::uint64_t>(lparam)),
                            owned_input)) {
                        source_group = candidate;
                        break;
                    }
                }
                if (source_group == nullptr
                    || !ActivateEditorGroup(*state, source_group->id)) {
                    return 0;
                }
                const inkpod::renderer::CanvasStrokeEvent input_view{
                    owned_input.kind,
                    owned_input.samples.empty() ? nullptr : owned_input.samples.data(),
                    static_cast<std::uint64_t>(owned_input.samples.size())};
                const auto* input = &input_view;
                if (state->engine == nullptr
                    || input->sample_count > UINT64_C(1048576)
                    || (input->sample_count != 0U && input->samples == nullptr)) {
                    return 0;
                }
                if (state->Workspace().tools.active_tool
                    == kInteractionShootingFrame) {
                    const InkpodStatus status = HandleShootingFrameCanvasEvent(
                        *state, *input);
                    if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                        ShowCoreError(*state, window, UiText(UiStringId::Text0685));
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End
                        || input->kind
                            == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        UpdateMenuState(*state);
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                if (state->Workspace().tools.active_tool
                    == kInteractionVanishingPoint) {
                    const InkpodStatus status = HandleVanishingPointCanvasEvent(
                        *state, *input);
                    if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                        ShowCoreError(*state, window, UiText(UiStringId::Text0769));
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End
                        || input->kind
                            == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        UpdateMenuState(*state);
                    }
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                if (state->ActiveView().presentation.guide_drag_active) {
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->ActiveView().presentation.guide_drag_active = false;
                        state->ActiveView().presentation.guide_drag_axis = 0U;
                        state->ActiveView().presentation.guide_drag_id = 0U;
                    } else if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End
                        && input->sample_count != 0U) {
                        const InkpodStatus status = FinishGuideDrag(
                            *state,
                            input->samples[static_cast<std::size_t>(
                                input->sample_count - 1U)]);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::ToolGuideMove));
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
                if (state->Workspace().tools.active_tool == kInteractionGuideMove) {
                    return 1;
                }
                if (state->Workspace().tools.active_tool == kInteractionBoxZoom) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->ActiveView().presentation.gesture_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->ActiveView().presentation.gesture_samples.insert(
                                state->ActiveView().presentation.gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        state->ActiveView().presentation.gesture_samples.clear();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->ActiveView().presentation.gesture_samples.clear();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = state->ActiveView().presentation.gesture_samples.size() < 2U
                            ? INKPOD_STATUS_INVALID_ARGUMENT
                            : ApplyBoxZoomGesture(
                                  *state,
                                  state->ActiveView().presentation.gesture_samples.front(),
                                  state->ActiveView().presentation.gesture_samples.back());
                        state->ActiveView().presentation.gesture_samples.clear();
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::ToolBoxZoom));
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                if (state->Workspace().tools.active_tool == kInteractionEyedropper
                    && input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                    && input->sample_count != 0U) {
                    const InkpodStatus status = EyedropAtDevicePoint(
                        *state, input->samples[0].x, input->samples[0].y);
                    if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                        ShowCoreError(*state, window, UiText(UiStringId::ToolEyedropper));
                    }
                    UpdateMenuState(*state);
                    return 1;
                }
                if (state->Workspace().tools.active_tool == kInteractionEyedropper) {
                    return 1;
                }
                if (state->Workspace().tools.active_tool == kInteractionFloatingTransform) {
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        if (!state->Workspace().tools.floating_gesture_samples.empty()) {
                            SetFloatingTransform(*state, state->Workspace().tools.floating_drag_start);
                        }
                        state->Workspace().tools.floating_gesture_samples.clear();
                        state->Workspace().tools.floating_drag_mode = 0U;
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                        state->Workspace().tools.floating_gesture_samples.clear();
                    }
                    if (input->sample_count != 0U) {
                        try {
                            state->Workspace().tools.floating_gesture_samples.push_back(
                                input->samples[static_cast<std::size_t>(input->sample_count - 1U)]);
                        } catch (const std::bad_alloc&) {
                            state->Workspace().tools.floating_gesture_samples.clear();
                            return 0;
                        }
                    }
                    if (!state->Workspace().tools.floating_gesture_samples.empty()) {
                        const InkpodStatus status = UpdateFloatingHandleDrag(
                            *state,
                            state->Workspace().tools.floating_gesture_samples.front(),
                            state->Workspace().tools.floating_gesture_samples.back(),
                            input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::Text0306));
                        }
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        state->Workspace().tools.floating_gesture_samples.clear();
                        state->Workspace().tools.floating_drag_mode = 0U;
                    }
                    return 1;
                }
                if (state->Workspace().tools.active_tool == kInteractionLightTableMove) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->Workspace().panes.light_table_move_samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->Workspace().panes.light_table_move_samples.push_back(
                                input->samples[static_cast<std::size_t>(input->sample_count - 1U)]);
                        }
                    } catch (const std::bad_alloc&) {
                        state->Workspace().panes.light_table_move_samples.clear();
                        state->Workspace().panes.light_table_move_context.reset();
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->Workspace().panes.light_table_move_samples.clear();
                        state->Workspace().panes.light_table_move_context.reset();
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status =
                            state->Workspace().panes.light_table_move_context.has_value()
                                && state->routing.targets.Resolve(
                                       state->Workspace().panes
                                           .light_table_move_context.value(),
                                       inkpod::app::kDocumentViewCommandScope)
                                    == CommandResolveStatus::Ok
                            ? MoveLightTableFromCanvas(
                                  *state,
                                  state->Workspace().panes
                                      .light_table_move_context.value())
                            : INKPOD_STATUS_INVALID_STATE;
                        state->Workspace().panes.light_table_move_samples.clear();
                        state->Workspace().panes.light_table_move_context.reset();
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::ToolLightTableMove));
                        }
                        RefreshLightTablePane(*state);
                    }
                    return 1;
                }
                const InkpodEditorStateInfo* procedure_editor =
                    CapturedEditorState(*state);
                const std::uint32_t procedure_tool = procedure_editor == nullptr
                    ? state->Workspace().tools.active_tool
                    : procedure_editor->active_tool;
                if (IsGeometryCanvasTool(procedure_tool)) {
                    const InkpodStatus status =
                        HandleRasterGeometryCanvasEvent(*state, *input);
                    if (status != INKPOD_STATUS_OK
                        && status != INKPOD_STATUS_INVALID_ARGUMENT
                        && !state->lifetime.smoke_test) {
                        ShowCoreError(
                            *state,
                            window,
                            UiText(UiStringId::ToolGeometry));
                    }
                    UpdateMenuState(*state);
                    return status == INKPOD_STATUS_OK ? 1 : 0;
                }
                procedure_editor = CapturedEditorState(*state);
                if ((procedure_editor == nullptr
                        ? state->Workspace().tools.active_tool
                        : procedure_editor->active_tool)
                    == kInteractionColorReplace) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            CancelColorReplaceGeometryPreview(
                                state->Workspace().tools,
                                state->Workspace().windows.canvas);
                            if (!BeginEditorProcedureCapture(*state)) {
                                return 0;
                            }
                            procedure_editor = CapturedEditorState(*state);
                            InkpodDocumentInfo info{};
                            TreePaneNode plane{};
                            const auto mode = QueryTreeNode(*state, true, plane)
                                ? ColorReplaceModeForPlane(plane.kind)
                                : std::nullopt;
                            if (procedure_editor == nullptr || !QueryDocument(*state, info)
                                || plane.id != procedure_editor->active_plane_id
                                || !mode.has_value()) {
                                CancelColorReplaceGeometryPreview(
                                    state->Workspace().tools,
                                    state->Workspace().windows.canvas);
                                return 0;
                            }
                            state->Workspace().tools.color_replace_base_revision =
                                info.document_revision;
                            state->Workspace().tools.color_replace_mode = mode.value();
                            state->Workspace().tools.color_replace_diameter =
                                std::clamp(
                                    static_cast<float>(
                                        static_cast<double>(
                                            procedure_editor->current_diameter_q16)
                                        / 65536.0),
                                    0.001F,
                                    4096.0F);
                        }
                        if (input->kind
                                != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->Workspace().tools
                                    .color_replace_gesture_samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                CancelColorReplaceGeometryPreview(
                                    state->Workspace().tools,
                                    state->Workspace().windows.canvas);
                                return 0;
                            }
                            state->Workspace().tools.color_replace_gesture_samples.insert(
                                state->Workspace().tools
                                    .color_replace_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        CancelColorReplaceGeometryPreview(
                            state->Workspace().tools,
                            state->Workspace().windows.canvas);
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        CancelColorReplaceGeometryPreview(
                            state->Workspace().tools,
                            state->Workspace().windows.canvas);
                        return 1;
                    }
                    procedure_editor = CapturedEditorState(*state);
                    if (procedure_editor == nullptr) {
                        CancelColorReplaceGeometryPreview(
                            state->Workspace().tools,
                            state->Workspace().windows.canvas);
                        return 0;
                    }
                    UpdateColorReplaceGeometryPreview(*state);
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = ApplyColorReplace(
                            *state,
                            procedure_editor,
                            true,
                            state->Workspace().tools
                                .color_replace_gesture_samples);
                        CancelColorReplaceGeometryPreview(
                            state->Workspace().tools,
                            state->Workspace().windows.canvas);
                        if (status != INKPOD_STATUS_OK
                            && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::ToolColorReplacement));
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                procedure_editor = CapturedEditorState(*state);
                if ((procedure_editor == nullptr
                        ? state->Workspace().tools.active_tool
                        : procedure_editor->active_tool)
                    == kInteractionSelection) {
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            CancelSelectionGeometryPreview(
                                state->Workspace().tools, state->Workspace().windows.canvas);
                            if (!BeginEditorProcedureCapture(*state)) {
                                return 0;
                            }
                            procedure_editor = CapturedEditorState(*state);
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->Workspace().tools.selection_gesture_samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                CancelSelectionGeometryPreview(
                                    state->Workspace().tools, state->Workspace().windows.canvas);
                                return 0;
                            }
                            state->Workspace().tools.selection_gesture_samples.insert(
                                state->Workspace().tools.selection_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        CancelSelectionGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        CancelSelectionGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        return 1;
                    }
                    if (procedure_editor == nullptr) {
                        CancelSelectionGeometryPreview(
                            state->Workspace().tools,
                            state->Workspace().windows.canvas);
                        return 0;
                    }
                    if (procedure_editor->selection.shape == INKPOD_SELECTION_WAND) {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            const InkpodStatus status = ApplySelectionGesture(
                                *state,
                                state->Workspace().tools.selection_gesture_samples,
                                procedure_editor);
                            state->Workspace().tools.selection_gesture_samples.clear();
                            SendMessageW(
                                state->Workspace().windows.canvas,
                                inkpod::renderer::kCanvasClearGeometryPreview,
                                0,
                                0);
                            if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                                ShowCoreError(*state, window, UiText(UiStringId::Text0868));
                            }
                            UpdateMenuState(*state);
                        } else if (input->kind
                            == inkpod::renderer::CanvasStrokeEventKind::End) {
                            CancelSelectionGeometryPreview(
                                state->Workspace().tools,
                                state->Workspace().windows.canvas);
                        }
                        return 1;
                    }
                    UpdateSelectionGeometryPreview(*state);
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = ApplySelectionGesture(
                            *state,
                            state->Workspace().tools.selection_gesture_samples,
                            procedure_editor);
                        CancelSelectionGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::LayerSelection));
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                procedure_editor = CapturedEditorState(*state);
                if ((procedure_editor == nullptr
                        ? state->Workspace().tools.active_tool
                        : procedure_editor->active_tool)
                    == kInteractionFill) {
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                        CancelFillGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        if (!BeginEditorProcedureCapture(*state)) {
                            return 0;
                        }
                        procedure_editor = CapturedEditorState(*state);
                    }
                    if (procedure_editor == nullptr) {
                        CancelFillGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        return 0;
                    }
                    if (procedure_editor->fill.operation == INKPOD_FILL_SEED) {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                            && input->sample_count != 0U) {
                            const InkpodStatus status = ApplyFillAtDevicePoint(
                                *state,
                                input->samples[0].x,
                                input->samples[0].y,
                                procedure_editor);
                            CancelFillGeometryPreview(
                                state->Workspace().tools, state->Workspace().windows.canvas);
                            if (status != INKPOD_STATUS_OK
                                && status != INKPOD_STATUS_FILL_OVERFLOW
                                && !state->lifetime.smoke_test) {
                                ShowCoreError(*state, window, UiText(UiStringId::Text0283));
                            }
                            UpdateMenuState(*state);
                        }
                        return 1;
                    }
                    try {
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            state->Workspace().tools.fill_gesture_samples.insert(
                                state->Workspace().tools.fill_gesture_samples.end(),
                                input->samples,
                                input->samples
                                    + static_cast<std::size_t>(input->sample_count));
                        }
                    } catch (const std::bad_alloc&) {
                        CancelFillGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        return 0;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        CancelFillGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin
                        || input->kind == inkpod::renderer::CanvasStrokeEventKind::Append) {
                        UpdateFillGeometryPreview(*state);
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        const InkpodStatus status = state->Workspace().tools.fill_gesture_samples.size() < 2U
                            ? INKPOD_STATUS_INVALID_ARGUMENT
                            : ApplyFillAtDeviceRange(
                                  *state,
                                  state->Workspace().tools.fill_gesture_samples.front().x,
                                  state->Workspace().tools.fill_gesture_samples.front().y,
                                  state->Workspace().tools.fill_gesture_samples.back().x,
                                  state->Workspace().tools.fill_gesture_samples.back().y,
                                  true,
                                  procedure_editor);
                        CancelFillGeometryPreview(
                            state->Workspace().tools, state->Workspace().windows.canvas);
                        if (status != INKPOD_STATUS_OK
                            && status != INKPOD_STATUS_FILL_OVERFLOW
                            && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::Text0840));
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                const bool effect_interaction = procedure_tool >= kInteractionEffectGradient
                    && procedure_tool <= kInteractionEffectAlphaGradient;
                if (effect_interaction) {
                    if (procedure_tool == kInteractionEffectStamp
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
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                        const CommandContext context =
                            state->routing.targets.Capture();
                        if (state->routing.targets.Resolve(
                                context,
                                inkpod::app::kDocumentViewCommandScope)
                            != CommandResolveStatus::Ok) {
                            return 0;
                        }
                        state->effects.gesture_context = context;
                    }
                    if (!state->effects.gesture_context.has_value()
                        || state->routing.targets.Resolve(
                               state->effects.gesture_context.value(),
                               inkpod::app::kDocumentViewCommandScope)
                            != CommandResolveStatus::Ok) {
                        state->effects.samples.clear();
                        state->effects.airbrush_active = false;
                        state->effects.gesture_options_valid = false;
                        state->effects.gesture_context.reset();
                        ClearEditorProcedureCapture(*state);
                        DisarmCommandTimer(
                            *state, window, CommandTimerKind::ContinuousSpray);
                        return 0;
                    }
                    try {
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            if (!BeginEditorProcedureCapture(*state)) {
                                state->effects.gesture_context.reset();
                                return 0;
                            }
                            state->effects.gesture_options = state->effects.options;
                            state->effects.gesture_options_valid = true;
                            state->effects.samples.clear();
                        }
                        if (input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                            && input->sample_count != 0U) {
                            if (state->effects.samples.size()
                                > UINT64_C(1048576) - input->sample_count) {
                                state->effects.samples.clear();
                                state->effects.airbrush_active = false;
                                state->effects.gesture_options_valid = false;
                                state->effects.gesture_context.reset();
                                ClearEditorProcedureCapture(*state);
                                DisarmCommandTimer(
                                    *state,
                                    window,
                                    CommandTimerKind::ContinuousSpray);
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
                        state->effects.gesture_options_valid = false;
                        state->effects.gesture_context.reset();
                        ClearEditorProcedureCapture(*state);
                        DisarmCommandTimer(
                            *state, window, CommandTimerKind::ContinuousSpray);
                        return 0;
                    }
                    if (procedure_tool == kInteractionEffectAirbrush
                        && input->kind != inkpod::renderer::CanvasStrokeEventKind::Cancel
                        && input->sample_count != 0U) {
                        state->effects.airbrush_last =
                            input->samples[static_cast<std::size_t>(input->sample_count - 1U)];
                        if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                            state->effects.airbrush_active = true;
                            ArmCommandTimer(
                                *state,
                                window,
                                CommandTimerKind::ContinuousSpray,
                                kContinuousSprayIntervalMilliseconds);
                        }
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                        state->effects.samples.clear();
                        state->effects.airbrush_active = false;
                        state->effects.gesture_options_valid = false;
                        state->effects.gesture_context.reset();
                        ClearEditorProcedureCapture(*state);
                        DisarmCommandTimer(
                            *state, window, CommandTimerKind::ContinuousSpray);
                        return 1;
                    }
                    if (input->kind == inkpod::renderer::CanvasStrokeEventKind::End) {
                        state->effects.airbrush_active = false;
                        DisarmCommandTimer(
                            *state, window, CommandTimerKind::ContinuousSpray);
                        const CommandContext context =
                            state->effects.gesture_context.value();
                        state->effects.gesture_context.reset();
                        const InkpodStatus status = FinishEffectGesture(
                            *state, context);
                        state->effects.gesture_options_valid = false;
                        ClearEditorProcedureCapture(*state);
                        if (status != INKPOD_STATUS_OK && !state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::Text0051));
                        }
                        UpdateMenuState(*state);
                    }
                    return 1;
                }
                inkpod::app::StrokeEvent event{};
                if (input->kind == inkpod::renderer::CanvasStrokeEventKind::Begin) {
                    const CommandContext context =
                        state->routing.targets.Capture();
                    if (state->routing.targets.Resolve(
                            context, inkpod::app::kDocumentViewCommandScope)
                        != CommandResolveStatus::Ok) {
                        return 0;
                    }
                    state->ActiveView().presentation.active_drag =
                        state->routing.tokens.IssueDrag(context);
                }
                if (!state->ActiveView().presentation.active_drag.has_value()
                    || state->routing.targets.Resolve(
                           state->ActiveView().presentation.active_drag->context,
                           inkpod::app::kDocumentViewCommandScope)
                        != CommandResolveStatus::Ok) {
                    state->ActiveView().presentation.active_drag.reset();
                    return 0;
                }
                event.context = state->ActiveView().presentation.active_drag->context;
                if (!event.context.document_view.has_value()
                    || !event.context.generation.has_value()) {
                    state->ActiveView().presentation.active_drag.reset();
                    return 0;
                }
                auto* stroke_document = state->Documents().FindByView(
                    event.context.document_view.value());
                auto* stroke_view = stroke_document == nullptr
                    ? nullptr
                    : stroke_document->FindView(
                          event.context.document_view.value());
                if (stroke_view == nullptr
                    || stroke_document->generation
                        != event.context.generation.value()) {
                    state->ActiveView().presentation.active_drag.reset();
                    return 0;
                }
                event.core_view_id = stroke_view->core_view_id;
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
                    INKPOD_COORDINATE_SPACE_DEVICE,
                    state->Workspace().tools.active_tool == INKPOD_TOOL_PENCIL ? INKPOD_STROKE_FLAG_AUTO_ERASE
                                                      : INKPOD_STROKE_FLAG_PRESSURE_SIZE};
                try {
                    if (input->sample_count != 0U) {
                        event.samples.assign(
                            input->samples,
                            input->samples + static_cast<std::size_t>(input->sample_count));
                    }
                } catch (const std::bad_alloc&) {
                    return 0;
                }
                const bool queued = state->engine->EnqueueStroke(std::move(event));
                if (queued) {
                    TrackAcceptedStrokePointer(
                        *state, *source_group, *stroke_view, *input);
                }
                if (!queued
                    || input->kind == inkpod::renderer::CanvasStrokeEventKind::End
                    || input->kind == inkpod::renderer::CanvasStrokeEventKind::Cancel) {
                    state->ActiveView().presentation.active_drag.reset();
                }
                return queued ? 1 : 0;
            }
            return 0;
        case inkpod::renderer::kCanvasViewGesture:
            if (state != nullptr) {
                inkpod::renderer::CanvasViewGesture gesture{};
                for (std::size_t index = 0U;
                     index < state->Workspace().editors.GroupCount();
                     ++index) {
                    auto* group = state->Workspace().editors.GroupAt(index);
                    if (group != nullptr
                        && inkpod::renderer::TakeCanvasViewGesture(
                            group->canvas,
                            static_cast<std::uint64_t>(wparam),
                            Generation(static_cast<std::uint64_t>(lparam)),
                            gesture)
                        && ActivateEditorGroup(*state, group->id)
                        && ApplyView(
                               *state,
                               gesture.kind,
                               gesture.value1,
                               gesture.value2,
                               gesture.value3) == INKPOD_STATUS_OK) {
                        return 1;
                    }
                }
            }
            return 0;
        case inkpod::renderer::kCanvasActivated:
            if (state != nullptr) {
                const CanvasId canvas{static_cast<std::uint64_t>(wparam)};
                auto* group = state->Workspace().editors.FindByCanvas(canvas);
                if (group != nullptr
                    && group->generation
                        == Generation(static_cast<std::uint64_t>(lparam))) {
                    return ActivateEditorGroup(*state, group->id) ? 1 : 0;
                }
            }
            return 0;
        case inkpod::renderer::kCanvasInteractionEnded:
            if (state != nullptr
                && state->Workspace().workspace_presentation_pending) {
                const CanvasId canvas{static_cast<std::uint64_t>(wparam)};
                const auto* group = state->Workspace().editors.FindByCanvas(canvas);
                if (group != nullptr
                    && group->generation
                        == Generation(static_cast<std::uint64_t>(lparam))) {
                    ApplyOrDeferWorkspacePresentation(*state);
                    return 1;
                }
            }
            return 0;
        case inkpod::renderer::kCanvasViewportChanged:
            if (state != nullptr && state->engine != nullptr && wparam != 0U) {
                const CanvasId canvas{static_cast<std::uint64_t>(wparam)};
                const auto* group = state->Workspace().editors.FindByCanvas(canvas);
                auto* document = group == nullptr
                    ? nullptr
                    : state->Documents().FindByView(group->ActiveView());
                auto* view = document == nullptr
                    ? nullptr
                    : document->FindView(group->ActiveView());
                if (view != nullptr) {
                    const InkpodViewInput input{
                        sizeof(InkpodViewInput),
                        INKPOD_VIEW_VIEWPORT_RESIZED,
                        0U,
                        static_cast<double>(LOWORD(lparam)),
                        static_cast<double>(HIWORD(lparam)),
                        0.0,
                        0.0};
                    (void)state->engine->Invoke(
                        document->id,
                        document->generation,
                        [core_view_id = view->core_view_id, input](InkpodCore* core) {
                            InkpodDocumentInfo ignored{};
                            ignored.struct_size = sizeof(ignored);
                            return core_view_id == 0U
                                ? inkpod_core_apply_view(core, &input, &ignored)
                                : inkpod_core_view_apply(core, core_view_id, &input);
                        },
                        true,
                        false);
                }
            }
            return 0;
        case inkpod::renderer::kCanvasPointerMoved:
            if (state != nullptr) {
                const CanvasId canvas{static_cast<std::uint64_t>(wparam)};
                const auto* group = state->Workspace().editors.FindByCanvas(canvas);
                auto* document = group == nullptr
                    ? nullptr
                    : state->Documents().FindByView(group->ActiveView());
                auto* view = document == nullptr
                    ? nullptr
                    : document->FindView(group->ActiveView());
                if (view != nullptr) {
                    view->presentation.pointer_device_x = GET_X_LPARAM(lparam);
                    view->presentation.pointer_device_y = GET_Y_LPARAM(lparam);
                    ++view->presentation.locator_generation;
                    if (group == state->Workspace().editors.Active()) {
                        QueueLocatorSample(*state);
                    }
                }
            }
            return 1;
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteCoreNotificationMessage(
    ApplicationHost* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    const auto targets_open_document = [state](
                                           const CommandContext& context) noexcept {
        if (state == nullptr || !context.document_session.has_value()
            || !context.generation.has_value()
            || context.generation.value()
                != state->routing.targets.CurrentGeneration()) {
            return false;
        }
        const auto* document = state->Documents().Find(
            context.document_session.value());
        if (document == nullptr
            || document->generation != context.generation.value()) {
            return false;
        }
        CommandTargetScope required = CommandTargetScope::DocumentSession;
        if (context.workspace.has_value()) {
            required = required | CommandTargetScope::Workspace;
        }
        if (context.editor_group.has_value()) {
            required = required | CommandTargetScope::EditorGroup;
        }
        if (context.document_view.has_value()) {
            required = required | CommandTargetScope::DocumentView;
        }
        return state->routing.targets.Resolve(context, required)
            == CommandResolveStatus::Ok;
    };
    switch (message) {
        case kSequenceSwitchCompleted: {
            std::shared_ptr<SequenceSwitchAsyncResult> result;
            if (state != nullptr) {
                std::lock_guard lock(
                    state->routing.sequence_switch_results_mutex);
                if (state->routing.sequence_switch_result != nullptr
                    && state->routing.sequence_switch_result->token.value
                        == static_cast<std::uint64_t>(wparam)
                    && state->routing.sequence_switch_result->token.generation
                            .Value()
                        == static_cast<std::uint64_t>(lparam)) {
                    result = std::move(
                        state->routing.sequence_switch_result);
                }
            }
            if (state == nullptr || result == nullptr) {
                return 0;
            }
            std::uint64_t expected = result->token.value;
            (void)state->routing.sequence_switch_pending_token
                .compare_exchange_strong(
                    expected, 0U, std::memory_order_acq_rel);
            WorkspaceWindow* completion_workspace =
                result->context.workspace.has_value()
                ? state->FindWorkspace(result->context.workspace.value())
                : nullptr;
            if (completion_workspace != nullptr) {
                completion_workspace->animation.sequence_switch_pending = false;
                if (completion_workspace->animation
                        .smoke_sequence_switch_completed
                    != UINT32_MAX) {
                    ++completion_workspace->animation
                        .smoke_sequence_switch_completed;
                }
            }
            const bool target_valid = targets_open_document(result->context);
            auto* document = target_valid
                    && result->context.document_session.has_value()
                ? state->Documents().Find(
                    result->context.document_session.value())
                : nullptr;
            if (result->status == INKPOD_STATUS_OK && document != nullptr) {
                if (result->source_autosaved) {
                    SequenceAutosaveBinding binding{};
                    binding.document_uuid_high =
                        result->request.source_document_uuid_high;
                    binding.document_uuid_low =
                        result->request.source_document_uuid_low;
                    binding.source_generation =
                        result->request.source_generation;
                    binding.recovery_path = std::move(
                        result->source_recovery_path);
                    binding.metadata = std::move(result->source_metadata);
                    if (!document->PublishReservedSequenceAutosave(
                            std::move(binding))) {
                        result->status = INKPOD_STATUS_INVALID_STATE;
                    }
                }
                if (result->status == INKPOD_STATUS_OK) {
                    document->shell.current_path.clear();
                    if (result->target_restored) {
                        document->shell.source_path =
                            result->target_metadata.source_path;
                        document->shell.recovery_path =
                            result->target_recovery_path;
                        document->shell.recovery_original_path =
                            result->target_metadata.original_path;
                    } else {
                        document->shell.source_path.clear();
                        document->shell.recovery_original_path.clear();
                        if (!SequenceRecoveryPath(
                                result->request.target_document_uuid_high,
                                result->request.target_document_uuid_low,
                                result->request.target_source_generation,
                                document->shell.recovery_path)) {
                            document->shell.recovery_path.clear();
                        }
                    }
                    if (completion_workspace != nullptr) {
                        (void)state->ActivateWorkspaceWindow(
                            completion_workspace->id, false);
                    }
                    if (document->ActiveView() != nullptr) {
                        (void)ActivateDocumentTab(
                            *state, document->ActiveView()->id);
                    }
                    ResetUiForNewActiveDocument(*state);
                    (void)state->RefreshEditorPresentation(
                        document->id, document->generation);
                    (void)FitCanvas(*state, INKPOD_VIEW_FIT);
                }
            }
            if (completion_workspace != nullptr) {
                completion_workspace->animation.smoke_sequence_switch_status =
                    result->status;
                (void)state->ActivateWorkspaceWindow(
                    completion_workspace->id, false);
            }
            RefreshSequencePane(*state);
            (void)RefreshSubpalettePane(*state);
            RefreshTreePane(*state);
            RefreshLightTablePane(*state);
            RefreshColorPanes(*state);
            UpdateMenuState(*state);
            return 0;
        }
        case inkpod::app::kCoreStateChanged:
            if (state != nullptr && state->engine != nullptr) {
                inkpod::app::CoreNotification notification{};
                const bool received = state->engine->TakeNotification(
                    static_cast<std::uint64_t>(wparam),
                    Generation(static_cast<std::uint64_t>(lparam)),
                    notification);
                const bool target_valid = received
                    && notification.kind
                        == inkpod::app::CoreNotificationKind::StateChanged
                    && targets_open_document(notification.context);
                if (target_valid && notification.context.workspace.has_value()) {
                    (void)state->ActivateWorkspaceWindow(
                        notification.context.workspace.value(), false);
                }
                const bool target_current = target_valid
                    && notification.context.document_session.has_value()
                    && notification.context.document_session.value()
                        == state->routing.targets.DocumentSession();
                if (target_valid && !target_current) {
                    RefreshSequencePane(*state);
                    (void)RefreshSubpalettePane(*state);
                    UpdateMenuState(*state);
                    return 0;
                }
                if (!target_current) {
                    return 0;
                }
                (void)state->RefreshEditorPresentation(
                    notification.context.document_session.value(),
                    notification.context.generation.value());
                RefreshTreePane(*state);
                RefreshLightTablePane(*state);
                RefreshSequencePane(*state);
                (void)RefreshSubpalettePane(*state);
                RefreshColorPanes(*state);
                UpdateMenuState(*state);
            }
            return 0;
        case kLocatorSampleReady: {
            std::optional<LocatorAsyncResult> result;
            if (state != nullptr) {
                std::lock_guard lock(state->routing.locator_results_mutex);
                const auto found = std::find_if(
                    state->routing.locator_results.begin(),
                    state->routing.locator_results.end(),
                    [wparam, lparam](const auto& pending) {
                        return pending.has_value()
                            && pending->token.value
                                == static_cast<std::uint64_t>(wparam)
                            && pending->token.generation.Value()
                                == static_cast<std::uint64_t>(lparam);
                    });
                if (found != state->routing.locator_results.end()) {
                    result = std::move(found->value());
                    found->reset();
                }
            }
            if (state != nullptr && result.has_value()) {
                std::uint64_t expected = result->token.value;
                const bool was_pending =
                    state->routing.locator_pending_token.compare_exchange_strong(
                        expected, 0U, std::memory_order_acq_rel);
                auto* document = result->context.document_session.has_value()
                    ? state->Documents().Find(result->context.document_session.value())
                    : nullptr;
                auto* view = document != nullptr && result->context.document_view.has_value()
                    ? document->FindView(result->context.document_view.value())
                    : nullptr;
                if (view == nullptr) {
                    if (was_pending && state->routing.locator_latest_requested) {
                        state->routing.locator_latest_requested = false;
                        QueueLocatorSample(*state);
                    }
                    return 0;
                }
                const PaneActionTarget current =
                    state->routing.pane_targets.CaptureAction(
                        state->routing.locator_pane,
                        state->routing.targets.Capture(),
                        state->routing.targets);
                const bool target_current = current.status == PaneTargetStatus::Ok
                    && current.context == result->context;
                const bool presentable = was_pending && target_current
                    && result->status == INKPOD_STATUS_OK
                    && result->sample_generation
                        >= view->presentation.locator_presented_generation
                    && result->sample_generation
                        <= view->presentation.locator_generation;
                if (presentable) {
                    view->presentation.locator = result->output;
                    view->presentation.locator_valid = true;
                    view->presentation.locator_presented_generation =
                        result->sample_generation;
                    view->presentation.locator_neighborhood_width =
                        result->neighborhood_output.width;
                    view->presentation.locator_neighborhood_height =
                        result->neighborhood_output.height;
                    view->presentation.locator_neighborhood_origin_x =
                        result->neighborhood_output.origin_x;
                    view->presentation.locator_neighborhood_origin_y =
                        result->neighborhood_output.origin_y;
                    view->presentation.locator_neighborhood = result->neighborhood;
                    if (view->id == state->routing.targets.ActiveDocumentView()) {
                        UpdateLocatorStatus(*state);
                    }
                    RefreshLocatorPane(*state);
                }
                const bool needs_latest = was_pending
                    && (state->routing.locator_latest_requested
                        || !target_current
                        || result->sample_generation
                            < view->presentation.locator_generation);
                if (needs_latest) {
                    state->routing.locator_latest_requested = false;
                    QueueLocatorSample(*state);
                }
            }
            return 0;
        }
        case kColorChartGenerationCompleted: {
            if (state == nullptr) {
                return 0;
            }
            WorkspaceWindow* completion_workspace{};
            std::shared_ptr<ColorChartGenerationJob> job;
            for (std::size_t index = 0U;
                 index < state->Workspaces().Count(); ++index) {
                WorkspaceWindow* candidate = state->Workspaces().At(index);
                if (candidate == nullptr) {
                    continue;
                }
                const auto pending = candidate->panes.color_chart_generation;
                if (pending != nullptr
                    && pending->token == static_cast<std::uint64_t>(wparam)
                    && pending->context.generation.has_value()
                    && pending->context.generation->Value()
                        == static_cast<std::uint64_t>(lparam)) {
                    completion_workspace = candidate;
                    job = pending;
                    break;
                }
            }
            if (completion_workspace == nullptr || job == nullptr) {
                return 0;
            }
            ClearJobProgress(
                completion_workspace->job_progress,
                completion_workspace->job_progress_state,
                JobProgressSlot::ColorChart);
            if (!HasActiveJobProgress(
                    completion_workspace->job_progress_state)) {
                static_cast<void>(
                    completion_workspace->windows.dock_host.HidePane(
                        DockPaneType::JobProgress));
            }
            completion_workspace->panes.color_chart_generation.reset();

            const InkpodStatus status = static_cast<InkpodStatus>(
                job->status.load(std::memory_order_acquire));
            const bool target_valid = targets_open_document(job->context);
            if (status == INKPOD_STATUS_CANCELLED || !target_valid) {
                return 0;
            }
            if (status != INKPOD_STATUS_OK || job->preview == nullptr) {
                if (!state->lifetime.smoke_test) {
                    MessageBoxW(
                        completion_workspace->windows.window,
                        UiText(UiStringId::Text0052),
                        UiText(UiStringId::Text0053),
                        MB_OK | MB_ICONERROR);
                }
                return 0;
            }

            const auto& summary = job->summary;
            std::wstring comparison =
                UiText(UiStringId::Text0479) + std::to_wstring(summary.entry_count)
                + UiText(UiStringId::Text0008)
                + std::to_wstring(summary.source_unique_color_count)
                + UiText(UiStringId::Text0004) + std::to_wstring(summary.retained_color_count)
                + UiText(UiStringId::Text0006) + std::to_wstring(summary.added_color_count)
                + UiText(UiStringId::Text0005) + std::to_wstring(summary.removed_color_count);
            const std::uint64_t representatives = std::min<std::uint64_t>(
                5U, summary.entry_count);
            for (std::uint64_t index = 0; index < representatives; ++index) {
                InkpodColorValue color{};
                color.struct_size = sizeof(color);
                std::uint64_t name_bytes{};
                std::uint64_t frequency{};
                if (inkpod_color_chart_preview_get(
                        job->preview,
                        index,
                        &color,
                        nullptr,
                        0U,
                        &name_bytes,
                        &frequency) == INKPOD_STATUS_OK) {
                    comparison += L"\n#" + std::to_wstring(index + 1U)
                        + L"  count=" + std::to_wstring(frequency);
                }
            }
            const bool overflow =
                (summary.flags & INKPOD_COLOR_CHART_PREVIEW_EXCEEDS_MAXIMUM)
                != 0U;
            const int decision = state->lifetime.smoke_test
                ? (overflow ? IDCANCEL : IDYES)
                : MessageBoxW(
                      completion_workspace->windows.window,
                      (comparison
                       + (overflow
                              ? UiText(UiStringId::Text0002)
                              : UiText(UiStringId::Text0001)))
                          .c_str(),
                      UiText(UiStringId::Text0054),
                      overflow ? MB_RETRYCANCEL | MB_ICONWARNING
                               : MB_YESNOCANCEL | MB_ICONQUESTION);
            if (!overflow && decision == IDYES) {
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                const InkpodStatus apply_status = state->engine->Invoke(
                    job->context.document_session.value(),
                    job->context.generation.value(),
                    [job, &result](InkpodCore* core) {
                        return inkpod_core_color_chart_preview_apply(
                            core, job->preview, &result);
                    },
                    true,
                    true);
                if (apply_status != INKPOD_STATUS_OK) {
                    ShowCoreError(
                        *state,
                        completion_workspace->windows.window,
                        UiText(UiStringId::Text0161));
                    return 0;
                }
                (void)state->ActivateWorkspaceWindow(
                    completion_workspace->id, false);
                RefreshColorPanes(*state);
                UpdateMenuState(*state);
                return 0;
            }
            if ((!overflow && decision == IDNO)
                || (overflow && decision == IDRETRY)) {
                (void)PostMessageW(
                    completion_workspace->windows.window,
                    WM_COMMAND,
                    IDM_CHART_GENERATE,
                    0);
            }
            return 0;
        }
        case kEffectTaskCompleted:
            if (state != nullptr) {
                const InkpodStatus status = static_cast<InkpodStatus>(wparam);
                const CommandContext completion_context =
                    state->effects.completion_context;
                auto* completion_workspace = completion_context.workspace.has_value()
                    ? state->FindWorkspace(completion_context.workspace.value())
                    : nullptr;
                if (completion_workspace != nullptr) {
                    (void)state->ActivateWorkspaceWindow(
                        completion_workspace->id, false);
                }
                const bool target_current = completion_context.generation.has_value()
                    && completion_context.generation->Value()
                        == static_cast<std::uint64_t>(lparam)
                    && state->routing.targets.Resolve(
                           completion_context,
                           inkpod::app::kDocumentViewCommandScope
                               | CommandTargetScope::Job)
                    == CommandResolveStatus::Ok
                    && completion_context.document_view.has_value()
                    && completion_context.document_view.value()
                        == state->routing.targets.ActiveDocumentView();
                const bool document_current =
                    state->routing.targets.Resolve(
                        completion_context,
                        inkpod::app::kDocumentSessionCommandScope
                            | CommandTargetScope::Job)
                    == CommandResolveStatus::Ok;
                const bool prompt = state->effects.preview_prompt;
                state->effects.preview_prompt = false;
                const bool interactive_filter =
                    state->effects.filter_preview.work
                    != inkpod::app::FilterPreviewWork::None;
                const auto output_color_guard = state->effects.output_color_guard;
                state->effects.output_color_guard.reset();
                if (completion_workspace != nullptr) {
                    ClearJobProgress(
                        completion_workspace->job_progress,
                        completion_workspace->job_progress_state,
                        JobProgressSlot::Effect);
                    if (!HasActiveJobProgress(
                            completion_workspace->job_progress_state)) {
                        static_cast<void>(
                            completion_workspace->windows.dock_host.HidePane(
                                DockPaneType::JobProgress));
                    }
                }
                inkpod_task_release(&state->effects.task);
                if (state->effects.job_id.has_value()) {
                    (void)state->routing.targets.EndJob(
                        state->effects.job_id.value());
                }
                state->effects.job_id.reset();
                state->effects.completion_context = {};
                if (output_color_guard != nullptr) {
                    if (status == INKPOD_STATUS_OK && document_current) {
                        (void)FormatOutputColorGuardSummary(
                            output_color_guard->result,
                            state->effects.last_output_color_guard_summary);
                    } else {
                        state->effects.last_output_color_guard_summary.clear();
                        if (status != INKPOD_STATUS_CANCELLED
                            && completion_workspace != nullptr) {
                            ShowCoreError(
                                *state,
                                completion_workspace->windows.window,
                                UiText(UiStringId::Text0517));
                        }
                    }
                    UpdateMenuState(*state);
                    if (!state->effects.last_output_color_guard_summary.empty()
                        && completion_workspace != nullptr) {
                        PresentStatusBarPart(
                            completion_workspace->windows.status_bar,
                            5U,
                            state->effects.last_output_color_guard_summary.c_str());
                    }
                    return 0;
                }
                if (interactive_filter) {
                    CompleteInteractiveFilterWork(
                        *state, status, document_current);
                    UpdateMenuState(*state);
                    return 0;
                }
                if (status == INKPOD_STATUS_OK && prompt && document_current
                    && state->engine != nullptr) {
                    const int choice = target_current
                        ? MessageBoxW(
                              window,
                              UiText(UiStringId::Text0050),
                              UiText(UiStringId::Text0808),
                              MB_OKCANCEL | MB_ICONQUESTION)
                        : IDCANCEL;
                    const InkpodStatus preview_status = state->engine->Invoke(
                        completion_context.document_session.value(),
                        completion_context.generation.value(),
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
                        ShowCoreError(*state, window, UiText(UiStringId::Text0810));
                    }
                }
                UpdateMenuState(*state);
            }
            return 0;
        case kBatchTaskCompleted:
            if (state != nullptr) {
                InkpodStatus status = static_cast<InkpodStatus>(wparam);
                const CommandContext completion_context =
                    state->batch.completion_context;
                auto* completion_workspace = completion_context.workspace.has_value()
                    ? state->FindWorkspace(completion_context.workspace.value())
                    : nullptr;
                if (completion_workspace != nullptr) {
                    (void)state->ActivateWorkspaceWindow(
                        completion_workspace->id, false);
                }
                const bool target_valid = completion_context.generation.has_value()
                    && completion_context.generation->Value()
                        == static_cast<std::uint64_t>(lparam)
                    && state->routing.targets.Resolve(
                           completion_context,
                           inkpod::app::kDocumentViewCommandScope
                               | CommandTargetScope::Job)
                    == CommandResolveStatus::Ok
                    && completion_context.document_view.has_value();
                const auto* completed_document = target_valid
                        && completion_context.document_session.has_value()
                    ? state->Documents().Find(
                          completion_context.document_session.value())
                    : nullptr;
                const std::wstring completed_target = completed_document == nullptr
                    ? L""
                    : LocatorDocumentName(*completed_document);
                if (completion_workspace != nullptr) {
                    ClearJobProgress(
                        completion_workspace->job_progress,
                        completion_workspace->job_progress_state,
                        JobProgressSlot::Batch);
                    if (!HasActiveJobProgress(
                            completion_workspace->job_progress_state)) {
                        static_cast<void>(
                            completion_workspace->windows.dock_host.HidePane(
                                DockPaneType::JobProgress));
                    }
                }
                if (target_valid && status == INKPOD_STATUS_OK
                    && state->batch.output_destination
                        == INKPOD_BATCH_OUTPUT_NEW_TABS
                    && state->batch.report != nullptr) {
                    status = InstallBatchNewTabs(
                        *state, state->batch.report);
                }
                if (target_valid && state->batch.report != nullptr) {
                    try {
                        state->batch.last_result = BatchReportSummary(state->batch.report);
                    } catch (const std::bad_alloc&) {
                        state->batch.last_result = UiText(UiStringId::Text0410);
                    }
                } else if (!target_valid) {
                    inkpod_batch_report_release(&state->batch.report);
                }
                inkpod_batch_task_release(&state->batch.task);
                if (state->batch.job_id.has_value()) {
                    state->routing.pane_targets.JobClosed(
                        state->batch.job_id.value());
                    (void)state->routing.targets.EndJob(
                        state->batch.job_id.value());
                }
                if (state->batch.return_to_pinned && target_valid) {
                    (void)state->routing.pane_targets.PinDocument(
                        state->routing.batch_pane,
                        state->batch.return_context,
                        state->routing.targets);
                } else {
                    (void)state->routing.pane_targets.FollowActive(
                        state->routing.batch_pane);
                }
                state->batch.job_id.reset();
                state->batch.completion_context = {};
                state->batch.return_context = {};
                state->batch.job_text = status == INKPOD_STATUS_OK
                    ? UiText(UiStringId::Text0621)
                    : (status == INKPOD_STATUS_CANCELLED
                              ? UiText(UiStringId::Text0171)
                              : UiText(UiStringId::Text0616));
                if (!completed_target.empty()) {
                    state->batch.job_text += L": " + completed_target;
                }
                if (!target_valid) {
                    state->batch.job_text += UiText(UiStringId::Text1047);
                }
                UpdateBatchTarget(*state);
                RefreshBatchPalette(state->batch, state->Workspace().batch_palette);
                UpdateMenuState(*state);
                if (target_valid && status != INKPOD_STATUS_OK
                    && status != INKPOD_STATUS_CANCELLED
                    && !state->lifetime.smoke_test) {
                    ShowCoreError(*state, window, UiText(UiStringId::Text0267));
                }
            }
            return 0;
        case inkpod::app::kCoreAsyncFailed:
            if (state != nullptr && state->engine != nullptr) {
                inkpod::app::CoreNotification notification{};
                const bool received = state->engine->TakeNotification(
                    static_cast<std::uint64_t>(wparam),
                    Generation(static_cast<std::uint64_t>(lparam)),
                    notification);
                const bool target_current = received
                    && notification.kind
                        == inkpod::app::CoreNotificationKind::AsyncFailed
                    && targets_open_document(notification.context)
                    && notification.context.document_session.has_value()
                    && notification.context.document_session.value()
                        == state->routing.targets.DocumentSession();
                if (target_current && !state->lifetime.smoke_test) {
                    ShowCoreError(*state, window, UiText(UiStringId::Text1034));
                }
            return 0;
        }
        default:
            break;
    }
    return std::nullopt;
}


std::optional<LRESULT> RouteTimerAndCloseMessage(
    ApplicationHost* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    switch (message) {
        case WM_TIMER: {
            if (state == nullptr) {
                return 0;
            }
            const auto token = ResolveCommandTimer(*state, window, wparam);
            if (!token.has_value()) {
                return 0;
            }
            switch (token->kind) {
            case CommandTimerKind::StatusProgress: {
                std::array<wchar_t, 192U> progress{};
                if (FormatTaskProgressStatus(*state, progress)) {
                    PresentStatusBarPart(
                        state->Workspace().windows.status_bar,
                        5U,
                        progress.data());
                } else {
                    DisarmCommandTimer(
                        *state, window, CommandTimerKind::StatusProgress);
                }
                return 0;
            }
            case CommandTimerKind::ShortcutSequence:
                if (state->shortcuts.pending_deadline == 0U
                    || GetTickCount64() >= state->shortcuts.pending_deadline) {
                    DisarmCommandTimer(
                        *state, window, CommandTimerKind::ShortcutSequence);
                    ClearPendingShortcut(state->shortcuts);
                    UpdateMenuState(*state);
                }
                return 0;
            case CommandTimerKind::MotionPlayback:
                if (state->Workspace().animation.motion_active && !state->Workspace().animation.motion_paused
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
                        UpdateMotionState(state->Workspace().animation, frame);
                    } else {
                        state->Workspace().animation.motion_active = false;
                        DisarmCommandTimer(
                            *state, window, CommandTimerKind::MotionPlayback);
                        if (!state->lifetime.smoke_test) {
                            ShowCoreError(*state, window, UiText(UiStringId::Text0359));
                        }
                    }
                }
                return 0;
            case CommandTimerKind::ContinuousSpray:
                if (state->effects.airbrush_active && state->Workspace().tools.active_tool == kInteractionEffectAirbrush
                    && state->effects.task == nullptr) {
                    try {
                        if (state->effects.samples.size() < UINT64_C(1048576)) {
                            state->effects.samples.push_back(state->effects.airbrush_last);
                        } else {
                            state->effects.airbrush_active = false;
                            DisarmCommandTimer(
                                *state,
                                window,
                                CommandTimerKind::ContinuousSpray);
                        }
                    } catch (const std::bad_alloc&) {
                        state->effects.airbrush_active = false;
                        DisarmCommandTimer(
                            *state, window, CommandTimerKind::ContinuousSpray);
                    }
                }
                return 0;
            case CommandTimerKind::Autosave:
                if (!state->Document().shell.recovery_path.empty()) {
                    InkpodDocumentInfo info{};
                    if (QueryDocument(*state, info)
                        && (info.flags & INKPOD_DOCUMENT_FLAG_DIRTY) != 0U) {
                        QueueAutosave(
                            *state,
                            token->context,
                            state->Document().shell.recovery_path);
                    }
                }
                return 0;
            case CommandTimerKind::EffectProgress:
                return 0;
            }
            return 0;
        }
        case WM_CLOSE:
            if (state != nullptr) {
                CancelDocumentTabDrag(*state);
            }
            if (state != nullptr && state->Workspaces().Count() > 1U) {
                (void)CloseWorkspaceWindow(*state, window);
                return 0;
            }
            if (state != nullptr && !state->lifetime.smoke_test
                && !ConfirmAllDocuments(*state)) {
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
                DisarmCommandTimer(
                    *state, window, CommandTimerKind::ContinuousSpray);
                CaptureWorkspacePresentation(*state);
            }
            ShowWindow(window, SW_HIDE);
            PostQuitMessage(0);
            return 0;
        case inkpod::renderer::kCanvasRenderFailed:
            if (state == nullptr || !state->lifetime.smoke_test) {
                MessageBoxW(
                    window,
                    UiText(UiStringId::Text0043),
                    L"inkpod",
                    MB_OK | MB_ICONERROR);
            }
            if (state == nullptr || !state->lifetime.smoke_test) {
                SendMessageW(window, WM_CLOSE, 0, 0);
            }
            return 0;
        case WM_NCDESTROY:
            if (state != nullptr && state->Workspaces().Count() <= 1U) {
                for (const CommandTimerKind kind : {
                         CommandTimerKind::Autosave,
                         CommandTimerKind::ContinuousSpray,
                         CommandTimerKind::MotionPlayback,
                         CommandTimerKind::ShortcutSequence,
                         CommandTimerKind::StatusProgress}) {
                    DisarmCommandTimer(*state, window, kind);
                }
            }
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            return DefWindowProcW(window, message, wparam, lparam);
        default:
            break;
    }
    return std::nullopt;
}

std::optional<LRESULT> RouteMainWindowMessage(
    ApplicationHost* state,
    HWND window,
    UINT message,
    WPARAM wparam,
    LPARAM lparam) noexcept {
    using MessageRoute = std::optional<LRESULT> (*)(
        ApplicationHost*, HWND, UINT, WPARAM, LPARAM) noexcept;
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

}  // namespace inkpod::windows::ui::runtime
