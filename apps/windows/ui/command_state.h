#pragma once

#include <windows.h>

#include <array>
#include <cstddef>
#include <cstdint>

#include "inkpod/core_ffi.h"

namespace inkpod::windows::ui {

enum class CommandStateOwner : std::uint8_t {
    Batch,
    Document,
    Edit,
    Effects,
    DocumentPane,
    Animation,
    SelectionView,
    Tool,
    Color,
    Application,
};

struct CommandState {
    UINT command{};
    CommandStateOwner owner{};
    bool enabled{true};
    bool checked{};
};

inline constexpr std::size_t kProductionCommandStateCount = 273U;
using CommandStateSet = std::array<CommandState, kProductionCommandStateCount>;

struct DocumentCommandStateInput {
    bool has_document{};
    bool has_saved_path{};
    bool dirty{};
};

struct EditCommandStateInput {
    bool can_undo{};
    bool can_redo{};
    bool can_history_back{};
    bool can_history_forward{};
    bool clipboard_available{};
    bool floating_active{};
};

struct EffectsCommandStateInput {
    bool color_plane_active{};
    bool adjustment_available{};
    bool multiple_adjustments{};
    bool adjustment_visible{true};
    bool alpha_view{};
};

struct DocumentPaneCommandStateInput {
    bool removable_layer_available{};
};

struct AnimationCommandStateInput {
    std::uint32_t motion_fps{24U};
};

struct SelectionViewCommandStateInput {
    std::uint32_t active_tool{};
    InkpodSelectionShape selection_shape{INKPOD_SELECTION_RECTANGLE};
    InkpodSelectionOperation selection_operation{INKPOD_SELECTION_NEW};
    bool flip_horizontal{};
    bool flip_vertical{};
    bool ruler_visible{};
    bool guides_visible{true};
    bool grid_visible{};
    bool snap_guides{};
    bool snap_grid{};
    bool transparent_visible{true};
    bool selection_layer_available{};
};

struct ToolCommandStateInput {
    std::uint32_t active_tool{};
    InkpodPlaneKind active_plane{INKPOD_PLANE_MAIN_LINE};
    InkpodFillOperation fill_operation{INKPOD_FILL_SEED};
    InkpodVectorEraseMode vector_erase_mode{INKPOD_VECTOR_ERASE_PARTIAL};
    bool vector_stroke_plane{};
};

struct ColorCommandStateInput {
    InkpodEyedropperSource eyedropper_source{INKPOD_EYEDROPPER_COMPOSITE};
    InkpodColorCheckMode color_check_mode{INKPOD_COLOR_CHECK_OFF};
    bool chart_locked{};
};

struct BatchCommandStateInput {
    bool idle{true};
    bool has_operations{};
    bool loaded_graph{};
    bool editable_item{};
    bool palette_visible{};
    InkpodBatchOutputPolicy output_policy{INKPOD_BATCH_OUTPUT_DUPLICATE};
    InkpodBatchFailurePolicy failure_policy{INKPOD_BATCH_FAILURE_CONTINUE};
};

struct CommandStateInputs {
    DocumentCommandStateInput document;
    EditCommandStateInput edit;
    EffectsCommandStateInput effects;
    DocumentPaneCommandStateInput document_pane;
    AnimationCommandStateInput animation;
    SelectionViewCommandStateInput selection_view;
    ToolCommandStateInput tool;
    ColorCommandStateInput color;
    BatchCommandStateInput batch;
};

CommandStateSet ComputeCommandStates(const CommandStateInputs& inputs) noexcept;

const CommandState* FindCommandState(
    const CommandStateSet& states, UINT command) noexcept;

bool IsCommandEnabled(const CommandStateSet& states, UINT command) noexcept;
bool IsCommandChecked(const CommandStateSet& states, UINT command) noexcept;

void ApplyCommandStates(
    const CommandStateSet& states, HMENU menu, HWND toolbar) noexcept;

} // namespace inkpod::windows::ui
