#include <algorithm>
#include <array>
#include <cstdio>
#include <cstdint>
#include <initializer_list>
#include <utility>

#include "app/frontend_state.h"
#include "app/resource.h"
#include "canvas.h"
#include "ui/command_catalog.h"
#include "ui/command_state.h"
#include "ui/tools/tool_state.h"

namespace {

using inkpod::app::ToolUiState;
using inkpod::windows::ui::CommandStateInputs;
using inkpod::windows::ui::CommandStateOwner;
using inkpod::windows::ui::CommandStateSet;
using inkpod::windows::ui::ComputeCommandStates;
using inkpod::windows::ui::BuildDefaultShortcutSequences;
using inkpod::windows::ui::FindShortcutSequence;
using inkpod::windows::ui::FindCommandState;
using inkpod::windows::ui::IsCommandChecked;
using inkpod::windows::ui::IsCommandEnabled;
using inkpod::windows::ui::MenuCommandCatalog;
using inkpod::windows::ui::ShortcutCommandCatalog;
using inkpod::windows::ui::kProductionCommandStateCount;
using inkpod::windows::ui::tools::SetActiveCommandColor;
using inkpod::windows::ui::tools::TransitionActiveTool;
using inkpod::windows::ui::tools::HandleActivePlaneTransition;
using inkpod::windows::ui::tools::ActiveToolAfterPlaneTransition;
using inkpod::windows::ui::tools::kInteractionEyedropper;
using inkpod::windows::ui::tools::kInteractionEffectGradient;
using inkpod::windows::ui::tools::kInteractionFill;
using inkpod::windows::ui::tools::kInteractionSelection;
using inkpod::windows::ui::tools::kInteractionColorReplace;
using inkpod::windows::ui::tools::kInteractionGeometryRectangle;
using inkpod::windows::ui::tools::CancelColorReplaceGeometryPreview;
using inkpod::windows::ui::tools::CancelFillGeometryPreview;
using inkpod::windows::ui::tools::CancelRasterGeometryPreview;
using inkpod::windows::ui::tools::CancelSelectionGeometryPreview;

constexpr UINT kRetiredJobProgressCommand = 41958U;

struct PreviewClearProbe final {
    std::uint32_t calls{};
    bool accept{true};
};

LRESULT CALLBACK PreviewClearWindowProcedure(
    HWND window, UINT message, WPARAM wparam, LPARAM lparam) noexcept {
    if (message == WM_NCCREATE) {
        const auto* create = reinterpret_cast<const CREATESTRUCTW*>(lparam);
        SetWindowLongPtrW(window, GWLP_USERDATA,
            reinterpret_cast<LONG_PTR>(create->lpCreateParams));
    }
    auto* probe = reinterpret_cast<PreviewClearProbe*>(
        GetWindowLongPtrW(window, GWLP_USERDATA));
    if (probe != nullptr && message == inkpod::renderer::kCanvasClearGeometryPreview) {
        ++probe->calls;
        return probe->accept ? 1 : 0;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

bool GeometryPreviewCancellationIsBounded() {
    constexpr wchar_t class_name[] = L"InkpodCommandStatePreviewClearProbe";
    const HINSTANCE instance = GetModuleHandleW(nullptr);
    WNDCLASSW window_class{};
    window_class.hInstance = instance;
    window_class.lpfnWndProc = PreviewClearWindowProcedure;
    window_class.lpszClassName = class_name;
    if (RegisterClassW(&window_class) == 0U) {
        return false;
    }
    PreviewClearProbe probe{};
    struct ProbeWindow final {
        HWND window;
        HINSTANCE instance;
        const wchar_t* class_name;
        ~ProbeWindow() {
            if (window != nullptr) {
                DestroyWindow(window);
            }
            UnregisterClassW(class_name, instance);
        }
    } owned{CreateWindowExW(0U, class_name, L"", 0U, 0, 0, 0, 0,
                HWND_MESSAGE, nullptr, instance, &probe), instance, class_name};
    if (owned.window == nullptr) {
        return false;
    }
    ToolUiState empty{};
    empty.procedure.valid = true;
    // ResetUiForNewActiveDocument performs these three cancellations in order.
    CancelFillGeometryPreview(empty, owned.window);
    CancelSelectionGeometryPreview(empty, owned.window);
    CancelColorReplaceGeometryPreview(empty, owned.window);
    CancelRasterGeometryPreview(empty, owned.window);
    if (probe.calls != 0U || empty.procedure.valid) {
        return false;
    }
    struct CancellationCase final {
        void (*cancel)(ToolUiState&, HWND) noexcept;
        std::vector<InkpodStrokeSample> ToolUiState::* samples;
    };
    const std::array<CancellationCase, 4U> cases{{
        {CancelFillGeometryPreview, &ToolUiState::fill_gesture_samples},
        {CancelSelectionGeometryPreview, &ToolUiState::selection_gesture_samples},
        {CancelColorReplaceGeometryPreview, &ToolUiState::color_replace_gesture_samples},
        {CancelRasterGeometryPreview, &ToolUiState::geometry_gesture_samples}}};
    for (const auto& candidate : cases) {
        ToolUiState active{};
        (active.*candidate.samples).push_back(InkpodStrokeSample{});
        active.procedure.valid = true;
        const std::uint32_t before = probe.calls;
        candidate.cancel(active, owned.window);
        if (probe.calls != before + 1U || !(active.*candidate.samples).empty()
            || active.procedure.valid) {
            return false;
        }
        candidate.cancel(active, owned.window);
        if (probe.calls != before + 1U) {
            return false;
        }
    }
    ToolUiState retry{};
    retry.selection_gesture_samples.push_back(InkpodStrokeSample{});
    const std::uint32_t before_retry = probe.calls;
    probe.accept = false;
    CancelSelectionGeometryPreview(retry, owned.window);
    if (probe.calls != before_retry + 1U || !retry.selection_gesture_samples.empty()) {
        return false;
    }
    probe.accept = true;
    // A failed shared-overlay clear survives even when the next owner is empty.
    CancelFillGeometryPreview(retry, owned.window);
    CancelSelectionGeometryPreview(retry, owned.window);
    if (probe.calls != before_retry + 2U) {
        return false;
    }
    retry.color_replace_gesture_samples.push_back(InkpodStrokeSample{});
    retry.color_replace_base_revision = 9U;
    CancelColorReplaceGeometryPreview(retry, nullptr);
    CancelRasterGeometryPreview(retry, owned.window);
    if (probe.calls != before_retry + 3U || !retry.color_replace_gesture_samples.empty()
        || retry.color_replace_base_revision != 0U) {
        return false;
    }
    retry.geometry_preview_active = true;
    retry.geometry_base_revision = 11U;
    retry.geometry_view_revision = 12U;
    retry.geometry_snap_bypass = true;
    CancelRasterGeometryPreview(retry, owned.window);
    CancelRasterGeometryPreview(retry, owned.window);
    if (probe.calls != before_retry + 4U || retry.geometry_preview_active
        || retry.geometry_base_revision != 0U || retry.geometry_view_revision != 0U
        || retry.geometry_snap_bypass) {
        return false;
    }
    retry.selection_gesture_samples.push_back(InkpodStrokeSample{});
    CancelFillGeometryPreview(retry, owned.window);
    return probe.calls == before_retry + 4U && retry.selection_gesture_samples.size() == 1U;
}

bool SameStates(const CommandStateSet& left, const CommandStateSet& right) noexcept {
    for (std::size_t index = 0; index < left.size(); ++index) {
        if (left[index].command != right[index].command
            || left[index].owner != right[index].owner
            || left[index].enabled != right[index].enabled
            || left[index].checked != right[index].checked) {
            return false;
        }
    }
    return true;
}

bool CatalogHasExactlyOneOwner(const CommandStateSet& states) noexcept {
    std::array<std::size_t, 11U> owner_counts{};
    for (std::size_t left = 0; left < states.size(); ++left) {
        if (states[left].command == 0U) {
            return false;
        }
        const auto owner = static_cast<std::size_t>(states[left].owner);
        if (owner >= owner_counts.size()) {
            return false;
        }
        ++owner_counts[owner];
        for (std::size_t right = left + 1U; right < states.size(); ++right) {
            if (states[left].command == states[right].command) {
                return false;
            }
        }
    }
    for (const std::size_t count : owner_counts) {
        if (count == 0U) {
            return false;
        }
    }
    return states.size() == kProductionCommandStateCount
        && kProductionCommandStateCount == 312U
        && FindCommandState(states, kRetiredJobProgressCommand) == nullptr;
}

bool SameColor(
    const InkpodColorValue& left, const InkpodColorValue& right) noexcept {
    return left.depth == right.depth && left.red == right.red
        && left.green == right.green && left.blue == right.blue
        && left.alpha == right.alpha;
}

bool StartsWith(
    const InkpodShortcutSequence& sequence,
    const InkpodShortcutSequence& prefix) noexcept {
    if (prefix.stroke_count > sequence.stroke_count) {
        return false;
    }
    for (std::uint32_t index = 0; index < prefix.stroke_count; ++index) {
        if (sequence.strokes[index].virtual_key != prefix.strokes[index].virtual_key
            || sequence.strokes[index].modifiers != prefix.strokes[index].modifiers) {
            return false;
        }
    }
    return true;
}

bool ShortcutCatalogIsSparseAndPrefixFree() {
    const auto commands = ShortcutCommandCatalog();
    const auto menu_commands = MenuCommandCatalog();
    const auto shortcuts = BuildDefaultShortcutSequences();
    const auto is_menu_command = [menu_commands](UINT command) noexcept {
        for (const UINT candidate : menu_commands) {
            if (candidate == command) {
                return true;
            }
        }
        return false;
    };
    // Deleted commands must disappear from all projections, including stable
    // shortcut keys. Keep only test-owned former IDs, never product tombstones.
    constexpr std::array retired_commands{
        std::pair{40010U, "file.import.raster"},
        std::pair{40023U, "file.new.cut"},
        std::pair{40024U, "cut.properties"},
        std::pair{40025U, "cut.save"},
        std::pair{40026U, "cut.undo"},
        std::pair{40027U, "cut.redo"},
        std::pair{40028U, "cut.sequence.add"},
        std::pair{40029U, "cut.sequence.remove"},
        std::pair{40030U, "cut.sequence.move.up"},
        std::pair{40031U, "cut.sequence.move.down"},
        std::pair{40032U, "cut.sequence.renumber"},
        std::pair{40033U, "file.export.instruction.raster"}};
    for (const auto& [command, key] : retired_commands) {
        if (is_menu_command(command)
            || std::find(commands.begin(), commands.end(), command) != commands.end()
            || FindShortcutSequence(shortcuts, command) != nullptr
            || !inkpod::windows::ui::CommandStableKey(command).empty()
            || inkpod::windows::ui::CommandFromStableKey(key) != 0U) {
            std::fprintf(stderr, "retired command is still exposed: %s\n", key);
            return false;
        }
    }
    if (menu_commands.size() != 304U
        || shortcuts.size() != 29U
        || commands.size() != kProductionCommandStateCount
        || is_menu_command(IDM_COLOR_PIN)
        || is_menu_command(IDM_BATCH_PIN)
        || !is_menu_command(IDM_WINDOW_BATCH)
        || is_menu_command(kRetiredJobProgressCommand)
        || FindShortcutSequence(shortcuts, kRetiredJobProgressCommand) != nullptr
        || !inkpod::windows::ui::CommandStableKey(kRetiredJobProgressCommand).empty()
        || inkpod::windows::ui::CommandFromStableKey("window.job.progress") != 0U) {
        std::fprintf(
            stderr,
            "catalog count mismatch: menu=%zu shortcuts=%zu states=%zu\n",
            menu_commands.size(),
            shortcuts.size(),
            kProductionCommandStateCount);
        return false;
    }
    for (const auto& sequence : shortcuts) {
        if (std::find(commands.begin(), commands.end(), sequence.command_id)
                == commands.end()
            || sequence.struct_size != sizeof(InkpodShortcutSequence)
            || sequence.stroke_count == 0U
            || sequence.stroke_count > INKPOD_SHORTCUT_MAX_STROKES) {
            std::fprintf(stderr, "invalid shortcut for command=%u\n", sequence.command_id);
            return false;
        }
        for (std::uint32_t index = 0; index < sequence.stroke_count; ++index) {
            if (sequence.strokes[index].virtual_key == 0U
                || sequence.strokes[index].virtual_key == static_cast<std::uint32_t>('Q')) {
                std::fprintf(
                    stderr,
                    "invalid shortcut stroke for command=%u index=%u\n",
                    sequence.command_id,
                    index);
                return false;
            }
        }
    }
    for (std::size_t left = 0; left < shortcuts.size(); ++left) {
        for (std::size_t right = left + 1U; right < shortcuts.size(); ++right) {
            if (shortcuts[left].command_id == shortcuts[right].command_id
                || StartsWith(shortcuts[left], shortcuts[right])
                || StartsWith(shortcuts[right], shortcuts[left])) {
                std::fprintf(
                    stderr,
                    "shortcut conflict: left=%u right=%u\n",
                    shortcuts[left].command_id,
                    shortcuts[right].command_id);
                return false;
            }
        }
    }
    const auto matches = [&shortcuts](
                             UINT command,
                             std::initializer_list<InkpodShortcutStroke> expected) {
        const auto* sequence = FindShortcutSequence(shortcuts, command);
        if (sequence == nullptr
            || sequence->stroke_count
                != static_cast<std::uint32_t>(expected.size())) {
            return false;
        }
        std::size_t index{};
        for (const auto& stroke : expected) {
            if (sequence->strokes[index].virtual_key != stroke.virtual_key
                || sequence->strokes[index].modifiers != stroke.modifiers) {
                return false;
            }
            ++index;
        }
        return true;
    };
    constexpr auto control = INKPOD_SHORTCUT_MODIFIER_CONTROL;
    constexpr auto shift = INKPOD_SHORTCUT_MODIFIER_SHIFT;
    constexpr auto alt = INKPOD_SHORTCUT_MODIFIER_ALT;
    constexpr auto extended = INKPOD_SHORTCUT_MODIFIER_EXTENDED;
    return matches(IDM_FILE_NEW, {{'N', control}})
        && matches(IDM_FILE_OPEN, {{'O', control}})
        && matches(IDM_FILE_SAVE, {{'S', control}})
        && matches(IDM_FILE_SAVE_AS, {{'S', control | shift}})
        && matches(IDM_APP_EXIT, {{VK_F4, alt}})
        && matches(IDM_HELP_MANUAL, {{VK_F1, 0U}})
        && matches(IDM_EDIT_UNDO, {{'Z', control}})
        && matches(IDM_EDIT_REDO, {{'Y', control}})
        && matches(IDM_EDIT_CUT, {{'X', control}})
        && matches(IDM_EDIT_COPY, {{'C', control}})
        && matches(IDM_EDIT_PASTE, {{'V', control}})
        && matches(IDM_SELECTION_ALL, {{'A', control}})
        && matches(IDM_SHORTCUT_EDIT, {{VK_OEM_COMMA, control}})
        && matches(IDM_SHORTCUT_KEYBOARD, {{'K', control}, {'S', control}})
        && matches(IDM_VIEW_ZOOM_IN, {{VK_OEM_PLUS, control}})
        && matches(IDM_VIEW_ZOOM_OUT, {{VK_OEM_MINUS, control}})
        && matches(IDM_TAB_NEXT, {{VK_NEXT, control | extended}})
        && matches(IDM_TAB_PREVIOUS, {{VK_PRIOR, control | extended}})
        && matches(
            IDM_TAB_MOVE_LEFT, {{VK_PRIOR, control | shift | extended}})
        && matches(
            IDM_TAB_MOVE_RIGHT, {{VK_NEXT, control | shift | extended}})
        && matches(IDM_VIEW_CLOSE, {{'W', control}})
        && matches(IDM_EDITOR_SPLIT_RIGHT, {{VK_OEM_5, control}})
        && matches(
            IDM_EDITOR_MOVE_OTHER_GROUP,
            {{VK_RIGHT, control | alt | extended}})
        && matches(IDM_EDITOR_GROUP_FIRST, {{'1', control}})
        && matches(IDM_EDITOR_GROUP_SECOND, {{'2', control}})
        && matches(
            IDM_EDITOR_GROUP_NEXT,
            {{'K', control}, {VK_RIGHT, control | extended}})
        && matches(IDM_EDITOR_GROUP_CLOSE, {{'K', control}, {'W', 0U}})
        && matches(IDM_WORKSPACE_NEW_WINDOW, {{'N', control | shift}})
        && matches(IDM_VIEW_DUPLICATE_NEW_WINDOW, {{'K', control}, {'O', 0U}})
        && FindShortcutSequence(shortcuts, IDM_TOOL_PENCIL) == nullptr
        && FindShortcutSequence(shortcuts, IDM_WINDOW_BATCH) == nullptr
        && FindShortcutSequence(shortcuts, IDM_PALETTE_NEXT_GROUP) == nullptr
        && FindShortcutSequence(shortcuts, IDM_MOTION_FPS_24) == nullptr
        && FindShortcutSequence(shortcuts, IDM_EDITOR_SPLIT_DOWN) == nullptr
        && FindShortcutSequence(shortcuts, IDM_VIEW_MOVE_NEW_WINDOW) == nullptr;
}

} // namespace

int main() {
    if (!GeometryPreviewCancellationIsBounded()) {
        std::fputs("geometry preview cancellation message/retry contract failed\n", stderr);
        return 26;
    }
    CommandStateInputs inputs{};
    CommandStateSet states = ComputeCommandStates(inputs);
    if (!CatalogHasExactlyOneOwner(states)) {
        return 101;
    }
    if (!ShortcutCatalogIsSparseAndPrefixFree()) {
        return 102;
    }
    if (IsCommandEnabled(states, IDM_EDITOR_GROUP_FIRST)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_SECOND)) {
        std::fputs("editor group shortcuts enabled without a document\n", stderr);
        return 30;
    }
    if (FindCommandState(states, IDM_HELP_MANUAL) == nullptr
        || FindCommandState(states, IDM_HELP_FILE_FORMAT) == nullptr
        || FindCommandState(states, IDM_HELP_ACKNOWLEDGEMENTS) == nullptr
        || FindCommandState(states, IDM_HELP_WEB_PAGE) == nullptr
        || FindCommandState(states, IDM_HELP_OPEN_SETTINGS_FILE) == nullptr
        || FindCommandState(states, IDM_HELP_ABOUT) == nullptr
        || IsCommandEnabled(states, IDM_FILE_SAVE)
        || IsCommandEnabled(states, IDM_VIEW_FIT)
        || IsCommandEnabled(states, IDM_VIEW_ONE_TO_ONE)
        || IsCommandEnabled(states, IDM_SELECTION_ALL)
        || IsCommandEnabled(states, IDM_FILTER_INVERT)
        || IsCommandEnabled(states, IDM_BATCH_ADD_COLOR_REPLACE)
        || !IsCommandEnabled(states, IDM_WINDOW_TOOL_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_TOOL_PALETTE)
        || !IsCommandEnabled(states, IDM_WINDOW_LAYER_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_LAYER_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_TOOL_OPTIONS)
        || !IsCommandChecked(states, IDM_WINDOW_COLOR_PANE)
        || !IsCommandEnabled(states, IDM_WINDOW_LOCATOR)
        || IsCommandChecked(states, IDM_WINDOW_LOCATOR)
        || IsCommandEnabled(states, IDM_LOCATOR_PIN)
        || IsCommandEnabled(states, IDM_LOCATOR_FIXED)
        || IsCommandEnabled(states, IDM_LOCATOR_AUTOSCROLL)
        || !IsCommandEnabled(states, IDM_WINDOW_SEQUENCE)
        || IsCommandChecked(states, IDM_WINDOW_SEQUENCE)
        || IsCommandEnabled(states, IDM_SEQUENCE_PIN)
        || !IsCommandEnabled(states, IDM_WINDOW_LIGHT_TABLE)
        || IsCommandChecked(states, IDM_WINDOW_LIGHT_TABLE)
        || IsCommandEnabled(states, IDM_LIGHT_TABLE_PIN)
        || !IsCommandEnabled(states, IDM_WINDOW_SUBPALETTE)
        || IsCommandChecked(states, IDM_WINDOW_SUBPALETTE)
        || IsCommandEnabled(states, IDM_SUBPALETTE_PIN)
        || IsCommandEnabled(states, IDM_COLOR_PIN)
        || IsCommandEnabled(states, IDM_BATCH_PIN)
        || IsCommandChecked(states, IDM_WORKSPACE_MIRROR)
        || IsCommandEnabled(states, IDM_DOCUMENT_CLOSE)
        || IsCommandEnabled(states, IDM_VIEW_CLOSE)
        || IsCommandEnabled(states, IDM_TAB_NEXT)
        || IsCommandEnabled(states, IDM_TAB_PREVIOUS)
        || IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || IsCommandEnabled(states, IDM_EDITOR_SPLIT_RIGHT)
        || IsCommandEnabled(states, IDM_EDITOR_SPLIT_DOWN)
        || IsCommandEnabled(states, IDM_EDITOR_MOVE_OTHER_GROUP)
        || IsCommandEnabled(states, IDM_EDITOR_NEW_VIEW_OTHER_GROUP)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_CLOSE)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_NEXT)
        || !IsCommandEnabled(states, IDM_WORKSPACE_NEW_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEW_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEW_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_FILE_RECENT_1)
        || IsCommandEnabled(states, IDM_TOOL_COLOR_REPLACE_TARGET)
        || IsCommandEnabled(states, IDM_TOOL_COLOR_REPLACE_RECTANGLE)
        || IsCommandEnabled(states, IDM_GEOMETRY_LINE)
        || IsCommandEnabled(states, IDM_GEOMETRY_RECTANGLE)
        || !IsCommandEnabled(states, IDM_FILE_RESTORE_PREVIOUS)
        || IsCommandChecked(states, IDM_FILE_RESTORE_PREVIOUS)
        || !IsCommandEnabled(states, IDM_FILE_SEQUENCE_AUTOSAVE)
        || IsCommandChecked(states, IDM_FILE_SEQUENCE_AUTOSAVE)
        || !IsCommandEnabled(states, IDM_SEQ_WRAP_ENDPOINTS)
        || IsCommandChecked(states, IDM_SEQ_WRAP_ENDPOINTS)
        || !IsCommandEnabled(states, IDM_FILE_NEW)
        || !IsCommandEnabled(states, IDM_FILE_OPEN)
        || !IsCommandEnabled(states, IDM_FILE_OPEN_RECOVERY)
        || FindCommandState(states, IDM_FILE_NEW)->owner != CommandStateOwner::Application
        || FindCommandState(states, IDM_FILE_OPEN)->owner != CommandStateOwner::Application
        || FindCommandState(states, IDM_FILE_OPEN_RECOVERY)->owner != CommandStateOwner::Application) {
        return 1;
    }

    inputs.application.restore_previous_documents = true;
    inputs.application.sequence_autosave_before_switch = true;
    inputs.application.sequence_wrap_endpoints = true;
    inputs.application.ui_language_preference = 3U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandChecked(states, IDM_FILE_RESTORE_PREVIOUS)
        || !IsCommandChecked(states, IDM_FILE_SEQUENCE_AUTOSAVE)
        || !IsCommandChecked(states, IDM_SEQ_WRAP_ENDPOINTS)) {
        return 21;
    }

    inputs.animation.sequence_switch_pending = true;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_SEQ_PREVIOUS)
        || IsCommandEnabled(states, IDM_SEQ_NEXT)
        || IsCommandEnabled(states, IDM_SEQ_GOTO)) {
        return 22;
    }
    inputs.animation.sequence_switch_pending = false;

    inputs.document.has_document = true;
    inputs.document.has_saved_path = true;
    inputs.document.dirty = false;
    inputs.selection_view.document_count = 1U;
    inputs.selection_view.view_count = 1U;
    inputs.selection_view.active_group_view_count = 1U;
    inputs.workspace.locator_target_available = true;
    inputs.workspace.locator_visible = true;
    inputs.workspace.locator_pinned = true;
    inputs.workspace.locator_fixed = true;
    inputs.workspace.locator_auto_scroll = false;
    inputs.workspace.sequence_target_available = true;
    inputs.workspace.sequence_visible = true;
    inputs.workspace.sequence_pinned = true;
    inputs.workspace.light_table_target_available = true;
    inputs.workspace.light_table_visible = true;
    inputs.workspace.light_table_pinned = true;
    inputs.workspace.subpalette_target_available = true;
    inputs.workspace.subpalette_visible = true;
    inputs.workspace.subpalette_pinned = true;
    inputs.workspace.color_target_available = true;
    inputs.workspace.color_pinned = true;
    inputs.workspace.batch_target_available = true;
    inputs.workspace.batch_pinned = true;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_SELECTION_APPLY_SAVED_MASK)
        || IsCommandEnabled(states, IDM_SELECTION_ADD_SAVED_MASK)
        || IsCommandEnabled(states, IDM_SELECTION_SUBTRACT_SAVED_MASK)
        || IsCommandEnabled(states, IDM_SELECTION_RENAME_SAVED_MASK)
        || IsCommandEnabled(states, IDM_SELECTION_DELETE_SAVED_MASK)) {
        return 27;
    }
    inputs.selection_view.saved_selection_available = true;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_SELECTION_APPLY_SAVED_MASK)
        || !IsCommandEnabled(states, IDM_SELECTION_ADD_SAVED_MASK)
        || !IsCommandEnabled(states, IDM_SELECTION_SUBTRACT_SAVED_MASK)
        || !IsCommandEnabled(states, IDM_SELECTION_RENAME_SAVED_MASK)
        || !IsCommandEnabled(states, IDM_SELECTION_DELETE_SAVED_MASK)) {
        return 28;
    }
    inputs.selection_view.saved_selection_available = false;
    states = ComputeCommandStates(inputs);
    CommandStateInputs dirty_inputs = inputs;
    dirty_inputs.document.dirty = true;
    const CommandStateSet dirty_states = ComputeCommandStates(dirty_inputs);
    if (!SameStates(states, dirty_states)
        || !IsCommandEnabled(states, IDM_FILE_SAVE)
        || !IsCommandEnabled(states, IDM_FILE_COMPACT_COPY)
        || !IsCommandEnabled(states, IDM_FILE_REVERT)
        || !IsCommandEnabled(states, IDM_DOCUMENT_CLOSE)
        || !IsCommandEnabled(states, IDM_VIEW_CLOSE)
        || !IsCommandEnabled(states, IDM_EDITOR_SPLIT_RIGHT)
        || !IsCommandEnabled(states, IDM_EDITOR_SPLIT_DOWN)
        || !IsCommandEnabled(states, IDM_EDITOR_NEW_VIEW_OTHER_GROUP)
        || !IsCommandEnabled(states, IDM_WORKSPACE_NEW_WINDOW)
        || !IsCommandEnabled(states, IDM_VIEW_MOVE_NEW_WINDOW)
        || !IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEW_WINDOW)
        || !IsCommandEnabled(states, IDM_TOOL_COLOR_REPLACE_TARGET)
        || !IsCommandEnabled(states, IDM_TOOL_COLOR_REPLACE_RECTANGLE)
        || !IsCommandEnabled(states, IDM_LOCATOR_PIN)
        || !IsCommandEnabled(states, IDM_LOCATOR_FIXED)
        || !IsCommandEnabled(states, IDM_LOCATOR_AUTOSCROLL)
        || !IsCommandChecked(states, IDM_WINDOW_LOCATOR)
        || !IsCommandChecked(states, IDM_LOCATOR_PIN)
        || !IsCommandChecked(states, IDM_LOCATOR_FIXED)
        || IsCommandChecked(states, IDM_LOCATOR_AUTOSCROLL)
        || !IsCommandChecked(states, IDM_WINDOW_SEQUENCE)
        || !IsCommandEnabled(states, IDM_SEQUENCE_PIN)
        || !IsCommandChecked(states, IDM_SEQUENCE_PIN)
        || !IsCommandChecked(states, IDM_WINDOW_LIGHT_TABLE)
        || !IsCommandEnabled(states, IDM_LIGHT_TABLE_PIN)
        || !IsCommandChecked(states, IDM_LIGHT_TABLE_PIN)
        || !IsCommandChecked(states, IDM_WINDOW_SUBPALETTE)
        || !IsCommandEnabled(states, IDM_SUBPALETTE_PIN)
        || !IsCommandChecked(states, IDM_SUBPALETTE_PIN)
        || !IsCommandEnabled(states, IDM_COLOR_PIN)
        || !IsCommandChecked(states, IDM_COLOR_PIN)
        || !IsCommandEnabled(states, IDM_BATCH_PIN)
        || !IsCommandChecked(states, IDM_BATCH_PIN)
        || IsCommandEnabled(states, IDM_EDITOR_MOVE_OTHER_GROUP)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_CLOSE)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_NEXT)
        || !IsCommandEnabled(states, IDM_EDITOR_GROUP_FIRST)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_SECOND)
        || IsCommandEnabled(states, IDM_TAB_NEXT)
        || IsCommandEnabled(states, IDM_TAB_PREVIOUS)
        || IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)) {
        return 2;
    }

    inputs.document.file_io_pending = true;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_FILE_SAVE)
        || IsCommandEnabled(states, IDM_FILE_SAVE_AS)
        || !IsCommandEnabled(states, IDM_FILE_COMPACT_COPY)
        || IsCommandEnabled(states, IDM_FILE_REVERT)
        || IsCommandEnabled(states, IDM_FILE_REVERT_PARTIAL)) {
        return 31;
    }
    inputs.document.file_io_pending = false;

    inputs.document.recent_document_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_FILE_RECENT_1)
        || !IsCommandEnabled(states, IDM_FILE_RECENT_2)
        || IsCommandEnabled(states, IDM_FILE_RECENT_3)) {
        return 17;
    }

    inputs.selection_view.document_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_TAB_NEXT)
        || !IsCommandEnabled(states, IDM_TAB_PREVIOUS)
        || !IsCommandEnabled(states, IDM_VIEW_CLOSE)) {
        return 15;
    }
    inputs.selection_view.document_count = 1U;
    inputs.selection_view.view_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_VIEW_CLOSE)
        || !IsCommandEnabled(states, IDM_TAB_NEXT)
        || !IsCommandEnabled(states, IDM_TAB_PREVIOUS)) {
        return 16;
    }
    inputs.selection_view.view_count = 1U;

    inputs.selection_view.active_group_view_count = 3U;
    inputs.selection_view.active_tab_index = 1U;
    inputs.selection_view.workspace_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || !IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || !IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || !IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)) {
        return 19;
    }
    inputs.selection_view.active_tab_index = 0U;
    inputs.selection_view.workspace_count = 1U;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_TAB_MOVE_LEFT)
        || !IsCommandEnabled(states, IDM_TAB_MOVE_RIGHT)
        || IsCommandEnabled(states, IDM_VIEW_MOVE_NEXT_WINDOW)
        || IsCommandEnabled(states, IDM_VIEW_DUPLICATE_NEXT_WINDOW)) {
        return 20;
    }
    inputs.selection_view.active_group_view_count = 1U;
    inputs.selection_view.active_tab_index = 0U;

    inputs.selection_view.editor_group_count = 2U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_EDITOR_SPLIT_RIGHT)
        || !IsCommandEnabled(states, IDM_EDITOR_SPLIT_DOWN)
        || !IsCommandEnabled(states, IDM_EDITOR_MOVE_OTHER_GROUP)
        || !IsCommandEnabled(states, IDM_EDITOR_NEW_VIEW_OTHER_GROUP)
        || !IsCommandEnabled(states, IDM_EDITOR_GROUP_CLOSE)
        || !IsCommandEnabled(states, IDM_EDITOR_GROUP_NEXT)
        || !IsCommandEnabled(states, IDM_EDITOR_GROUP_FIRST)
        || !IsCommandEnabled(states, IDM_EDITOR_GROUP_SECOND)) {
        return 18;
    }
    inputs.selection_view.editor_group_count = 1U;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_EDITOR_GROUP_FIRST)
        || IsCommandEnabled(states, IDM_EDITOR_GROUP_SECOND)) {
        return 29;
    }

    inputs.edit.can_undo = true;
    inputs.edit.can_redo = false;
    inputs.edit.can_history_back = true;
    inputs.edit.can_history_forward = false;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_EDIT_UNDO)
        || IsCommandEnabled(states, IDM_EDIT_REDO)
        || !IsCommandEnabled(states, IDM_EDIT_HISTORY_BACK)
        || IsCommandEnabled(states, IDM_EDIT_HISTORY_FORWARD)) {
        return 3;
    }

    inputs.tool.active_tool = INKPOD_TOOL_PENCIL;
    inputs.tool.palette_visible = true;
    inputs.document_pane.layer_palette_visible = true;
    inputs.selection_view.active_tool = INKPOD_TOOL_PENCIL;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_TOOL_PENCIL)
        || !IsCommandChecked(states, IDM_TOOL_PENCIL)
        || !IsCommandChecked(states, IDM_WINDOW_TOOL_PALETTE)
        || !IsCommandChecked(states, IDM_WINDOW_LAYER_PALETTE)) {
        return 4;
    }

    inputs.tool.geometry_drawable_plane = true;
    inputs.tool.active_tool = kInteractionGeometryRectangle;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_GEOMETRY_LINE)
        || !IsCommandEnabled(states, IDM_GEOMETRY_CURVE)
        || !IsCommandEnabled(states, IDM_GEOMETRY_RECTANGLE)
        || !IsCommandEnabled(states, IDM_GEOMETRY_ELLIPSE)
        || !IsCommandEnabled(states, IDM_GEOMETRY_POLYGON)
        || !IsCommandEnabled(states, IDM_GEOMETRY_POLYLINE)
        || !IsCommandChecked(states, IDM_GEOMETRY_RECTANGLE)
        || IsCommandChecked(states, IDM_GEOMETRY_LINE)
        || IsCommandChecked(states, IDM_TOOL_PENCIL)) {
        return 24;
    }

    inputs.effects.color_plane_active = true;
    inputs.tool.active_tool = kInteractionEffectGradient;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_EFFECT_GRADIENT)
        || !IsCommandChecked(states, IDM_EFFECT_GRADIENT)) {
        return 10;
    }

    inputs.tool.active_tool = kInteractionColorReplace;
    inputs.tool.color_replace_shape = INKPOD_SELECTION_LASSO;
    states = ComputeCommandStates(inputs);
    if (!IsCommandChecked(states, IDM_TOOL_COLOR_REPLACE_LASSO)
        || IsCommandChecked(states, IDM_TOOL_COLOR_REPLACE_RECTANGLE)) {
        return 22;
    }

    ToolUiState tools{};
    const InkpodColorValue black{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_8, 0U, 0U, 0U, 255U};
    if (tools.editor.valid || tools.active_tool != 0U
        || tools.color_rgba != 0U) {
        return 11;
    }
    SetActiveCommandColor(tools, black);
    TransitionActiveTool(tools, nullptr, kInteractionFill);
    if (!SameColor(tools.drawing_color, black)) {
        return 12;
    }
    const InkpodColorValue sampled_fill_color{
        sizeof(InkpodColorValue), INKPOD_COLOR_DEPTH_16, 1000U, 2000U, 3000U, 65535U};
    SetActiveCommandColor(tools, sampled_fill_color);
    TransitionActiveTool(tools, nullptr, kInteractionEyedropper);
    TransitionActiveTool(tools, nullptr, INKPOD_TOOL_PENCIL);
    if (!SameColor(tools.drawing_color, sampled_fill_color)) {
        return 13;
    }
    TransitionActiveTool(tools, nullptr, kInteractionFill);
    if (!SameColor(tools.drawing_color, sampled_fill_color)) {
        return 14;
    }

    TransitionActiveTool(tools, nullptr, kInteractionSelection);
    tools.selection_gesture_samples.push_back(InkpodStrokeSample{});
    TransitionActiveTool(tools, nullptr, kInteractionFill);
    if (!tools.selection_gesture_samples.empty()) {
        return 13;
    }

    tools.fill_gesture_samples.push_back(InkpodStrokeSample{});
    TransitionActiveTool(tools, nullptr, kInteractionSelection);
    if (!tools.fill_gesture_samples.empty()) {
        return 15;
    }

    TransitionActiveTool(tools, nullptr, kInteractionColorReplace);
    tools.color_replace_gesture_samples.push_back(InkpodStrokeSample{});
    tools.color_replace_base_revision = 9U;
    TransitionActiveTool(tools, nullptr, kInteractionSelection);
    if (!tools.color_replace_gesture_samples.empty()
        || tools.color_replace_base_revision != 0U) {
        return 23;
    }


    TransitionActiveTool(tools, nullptr, kInteractionGeometryRectangle);
    tools.geometry_gesture_samples.push_back(InkpodStrokeSample{});
    tools.geometry_base_revision = 11U;
    tools.geometry_view_revision = 12U;
    tools.geometry_preview_active = true;
    tools.geometry_snap_bypass = true;
    HandleActivePlaneTransition(tools, nullptr, UINT32_MAX);
    if (tools.active_tool != INKPOD_TOOL_PENCIL
        || !tools.geometry_gesture_samples.empty()
        || tools.geometry_base_revision != 0U
        || tools.geometry_view_revision != 0U
        || tools.geometry_preview_active
        || tools.geometry_snap_bypass) {
        return 25;
    }

    TransitionActiveTool(tools, nullptr, kInteractionFill);
    if (ActiveToolAfterPlaneTransition(
            tools.active_tool, INKPOD_TYPED_PLANE_MAIN_LINE, false)
        != kInteractionFill) {
        return 26;
    }
    HandleActivePlaneTransition(tools, nullptr, INKPOD_TYPED_PLANE_COLOR);
    if (tools.active_tool != kInteractionFill) {
        return 26;
    }
    tools.fill_gesture_samples.push_back(InkpodStrokeSample{});
    HandleActivePlaneTransition(tools, nullptr, INKPOD_TYPED_PLANE_MAIN_LINE);
    if (tools.active_tool != INKPOD_TOOL_PENCIL
        || !tools.fill_gesture_samples.empty()) {
        return 27;
    }

    inputs.edit.clipboard_available = true;
    inputs.edit.floating_active = true;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_EDIT_PASTE)
        || !IsCommandEnabled(states, IDM_EDIT_FLOATING_COMMIT)
        || !IsCommandEnabled(states, IDM_EDIT_FLOATING_CANCEL)) {
        return 7;
    }

    inputs.edit.floating_active = false;
    inputs.batch.idle = true;
    inputs.batch.has_operations = true;
    inputs.batch.editable_item = true;
    states = ComputeCommandStates(inputs);
    if (!IsCommandEnabled(states, IDM_BATCH_PREVIEW)
        || !IsCommandEnabled(states, IDM_BATCH_RUN_ALL)
        || IsCommandEnabled(states, IDM_BATCH_CANCEL)
        || !IsCommandEnabled(states, IDM_BATCH_OPERATION_DUPLICATE)) {
        return 8;
    }
    inputs.batch.idle = false;
    inputs.batch.editable_item = false;
    states = ComputeCommandStates(inputs);
    if (IsCommandEnabled(states, IDM_BATCH_PREVIEW)
        || IsCommandEnabled(states, IDM_BATCH_RUN_ALL)
        || !IsCommandEnabled(states, IDM_BATCH_CANCEL)
        || IsCommandEnabled(states, IDM_BATCH_OPERATION_DUPLICATE)) {
        return 9;
    }

    return 0;
}
