#include "app/application_host.h"
#include "app/core_host.h"
#include "app/resource.h"
#include "canvas.h"
#include "ui/main_window_runtime.h"
#include "ui/panes/tool_options_pane.h"
#include "ui/tools/line_correction_options.h"

#include <algorithm>
#include <array>
#include <cstdio>

namespace inkpod::windows::ui::runtime {

bool QueryDocument(app::ApplicationHost&, InkpodDocumentInfo&) noexcept;
bool WaitForSequencePresentation(app::ApplicationHost&, bool, std::optional<std::uint64_t>,
    std::optional<std::uint32_t>, std::optional<std::uint32_t>) noexcept;
InkpodStatus FitCanvas(app::ApplicationHost&, InkpodViewCommandKind) noexcept;

// Called in the audit's disposable 32x32 session. Fixtures use ordinary public
// strokes; every correction goes through actual menus, option controls and Canvas.
int RunLineCorrectionImplementationSmoke(app::ApplicationHost& state) noexcept {
    using Pixels = std::array<std::uint32_t, 32U * 32U>;
    const HWND window = state.Workspace().windows.window;
    const HWND canvas = state.Workspace().windows.canvas;
    if (state.ActiveView().presentation.flip_horizontal) {
        SendMessageW(window, WM_COMMAND, IDM_VIEW_FLIP_HORIZONTAL, 0);
    }
    SendMessageW(window, WM_COMMAND, IDM_SELECTION_CLEAR, 0);
    Pixels main{};
    Pixels color{};
    main[6U * 32U + 6U] = UINT32_C(0x000000ff);
    for (std::uint32_t y = 15U; y <= 17U; ++y) {
        for (std::uint32_t x = 5U; x <= 7U; ++x) {
            if (x != 6U || y != 16U) main[y * 32U + x] = UINT32_C(0x000000ff);
            color[y * 32U + x] = x == 6U && y == 16U ? UINT32_C(0x0000ffff) : UINT32_C(0xff0000ff);
        }
    }
    for (std::uint32_t x = 10U; x <= 26U; ++x) {
        if (x != 18U) main[24U * 32U + x] = UINT32_C(0x000000ff);
    }
    const auto paint = [](InkpodCore* core, const Pixels& pixels, InkpodPlaneKind plane) {
        for (std::uint32_t y = 0U; y < 32U; ++y) {
            for (std::uint32_t x = 0U; x < 32U; ++x) {
                const auto rgba = pixels[y * 32U + x];
                if (rgba == 0U) continue;
                const InkpodStrokeSample sample{sizeof(InkpodStrokeSample), 0U,
                    static_cast<float>(x) + 0.5F, static_cast<float>(y) + 0.5F, 1.0F, 0U};
                InkpodStrokeInput input{};
                input.struct_size = sizeof(input);
                input.tool = INKPOD_TOOL_PENCIL;
                input.plane = plane;
                input.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
                input.color_rgba = rgba;
                input.diameter = 1.0F;
                input.shape = INKPOD_BRUSH_ROUND;
                input.samples = &sample;
                input.sample_count = 1U;
                input.sample_stride_bytes = sizeof(sample);
                InkpodDispatchResult result{};
                result.struct_size = sizeof(result);
                const auto status = inkpod_core_apply_stroke(core, &input, &result);
                if (status != INKPOD_STATUS_OK) return status;
            }
        }
        return INKPOD_STATUS_OK;
    };
    Pixels ring{};
    for (std::uint32_t y = 8U; y <= 23U; ++y) {
        for (std::uint32_t x = 8U; x <= 23U; ++x) {
            if (x == 8U || x == 23U || y == 8U || y == 23U) ring[y * 32U + x] = UINT32_C(0x000000ff);
        }
    }
    if (state.engine->Invoke([&](InkpodCore* core) { return paint(core, ring, INKPOD_PLANE_MAIN_LINE); },
        true, true) != INKPOD_STATUS_OK) return 23300;
    const auto snapshot_pixels = [&state](Pixels& output) {
        output.fill(0U);
        return state.engine->Invoke([&output](InkpodCore* core) {
            const InkpodSnapshotOptions options{sizeof(InkpodSnapshotOptions), 0U, INKPOD_FEATURE_NONE};
            InkpodSnapshot* snapshot{};
            auto status = inkpod_core_build_snapshot(core, &options, &snapshot);
            InkpodSnapshotView view{};
            view.struct_size = sizeof(view);
            if (status == INKPOD_STATUS_OK) status = inkpod_snapshot_get_view(snapshot, &view);
            if (status == INKPOD_STATUS_OK) {
                for (std::uint64_t index = 0U; index < view.tile_count; ++index) {
                    const auto& tile = view.tiles[index];
                    for (std::uint32_t y = 0U; y < tile.height; ++y) {
                        for (std::uint32_t x = 0U; x < tile.width; ++x) {
                            const auto px = tile.origin_x + static_cast<int>(x), py = tile.origin_y + static_cast<int>(y);
                            if (px < 0 || py < 0 || px >= 32 || py >= 32) continue;
                            const auto* pixel = tile.pixels + static_cast<std::size_t>(y) * tile.stride_bytes + x * 4U;
                            output[static_cast<std::size_t>(py * 32 + px)] = (static_cast<std::uint32_t>(pixel[0]) << 24U)
                                | (static_cast<std::uint32_t>(pixel[1]) << 16U)
                                | (static_cast<std::uint32_t>(pixel[2]) << 8U) | pixel[3];
                        }
                    }
                }
            }
            const auto released = inkpod_snapshot_release(&snapshot);
            return status == INKPOD_STATUS_OK ? released : status;
        }, false, false) == INKPOD_STATUS_OK;
    };
    for (const std::uint32_t phase : {0U, 1U, 2U}) {
        if (phase == 1U) {
            if (state.engine->Invoke([](InkpodCore* core) {
                const InkpodStrokeSample point{sizeof(InkpodStrokeSample), 0U, 15.5F, 8.5F, 1.0F, 0U};
                InkpodStrokeInput input{};
                input.struct_size = sizeof(input); input.tool = INKPOD_TOOL_ERASER;
                input.plane = INKPOD_PLANE_MAIN_LINE; input.coordinate_space = INKPOD_COORDINATE_SPACE_DOCUMENT;
                input.diameter = 1.0F; input.shape = INKPOD_BRUSH_ROUND;
                input.samples = &point; input.sample_count = 1U; input.sample_stride_bytes = sizeof(point);
                InkpodDispatchResult result{}; result.struct_size = sizeof(result);
                return inkpod_core_apply_stroke(core, &input, &result);
            }, true, true) != INKPOD_STATUS_OK) return 23301;
            ring[8U * 32U + 15U] = 0U;
        }
        SendMessageW(window, WM_COMMAND, IDM_PLANE_MAIN_LINE, 0);
        SendMessageW(window, WM_COMMAND, IDM_SELECTION_WAND, 0);
        SendMessageW(window, WM_COMMAND, IDM_SELECTION_MODE_NEW, 0);
        InkpodEditorStateInfo editor{}; editor.struct_size = sizeof(editor);
        if (!state.engine->GetEditorState(state.Document().id, state.Document().generation, editor)) return 23302;
        InkpodEditorStateUpdate update{}; update.struct_size = sizeof(update);
        update.kind = INKPOD_EDITOR_UPDATE_SELECTION_OPTIONS;
        update.expected_editor_revision = editor.editor_revision; update.selection = editor.selection;
        update.selection.gap_close = phase == 2U ? 1U : 0U; update.selection.tolerance = 0U;
        if (state.UpdateEditorState(update) != INKPOD_STATUS_OK
            || FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
            || !WaitForSequencePresentation(state, true, std::nullopt, std::nullopt, std::nullopt)
            || SendMessageW(canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) return 23303;
        Pixels before_pixels{}, after_pixels{};
        InkpodDocumentInfo before{}, after{};
        inkpod::renderer::CanvasDocumentBounds bounds{};
        if (!QueryDocument(state, before) || !snapshot_pixels(before_pixels)
            || !inkpod::renderer::GetCanvasDocumentBounds(canvas, bounds)) return 23304;
        const InkpodStrokeSample seed{sizeof(InkpodStrokeSample), 0U,
            static_cast<float>(bounds.left + (bounds.right-bounds.left)*12.5/32.0),
            static_cast<float>(bounds.top + (bounds.bottom-bounds.top)*12.5/32.0), 1.0F, 0U};
        using Kind = inkpod::renderer::CanvasStrokeEventKind;
        if (!inkpod::renderer::SubmitCanvasStrokeEvent(canvas, {Kind::Begin, &seed, 1U})
            || !inkpod::renderer::SubmitCanvasStrokeEvent(canvas, {Kind::End, &seed, 1U})
            || !snapshot_pixels(after_pixels) || !QueryDocument(state, after)) return 23305;
        for (std::uint32_t y = 0U; y < 32U; ++y) {
            for (std::uint32_t x = 0U; x < 32U; ++x) {
                const bool expected = phase == 1U ? ring[y*32U+x] == 0U
                    : (x > 8U && x < 23U && y > 8U && y < 23U);
                if ((before_pixels[y*32U+x] != after_pixels[y*32U+x]) != expected) {
                    std::fprintf(stderr, "wand UI mask mismatch: phase=%u x=%u y=%u\n", phase, x, y);
                    return 23306;
                }
            }
        }
        if (before.main_plane_checksum != after.main_plane_checksum
            || before.color_plane_checksum != after.color_plane_checksum
            || after.document_revision != before.document_revision + 1U) return 23307;
        const Pixels selected_pixels = after_pixels;
        SendMessageW(window, WM_COMMAND, IDM_EDIT_UNDO, 0);
        if (!snapshot_pixels(after_pixels) || before_pixels != after_pixels) return 23308;
        SendMessageW(window, WM_COMMAND, IDM_EDIT_REDO, 0);
        if (!snapshot_pixels(after_pixels) || after_pixels != selected_pixels) return 23309;
        SendMessageW(window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    }
    // Remove the one eraser fixture operation and the 60 one-pixel fixture strokes.
    if (state.engine->Invoke([](InkpodCore* core) {
            for (std::uint32_t index = 0U; index < 61U; ++index) {
                InkpodDispatchResult result{}; result.struct_size = sizeof(result);
                const auto status = inkpod_core_undo(core, &result);
                if (status != INKPOD_STATUS_OK) return status;
            }
            return INKPOD_STATUS_OK;
        }, true, true) != INKPOD_STATUS_OK) return 23310;
    std::fputs("wand UI: closed/gap-disabled/gap-enabled exact 32x32 mask and source invariance passed\n", stderr);
    if (state.engine->Invoke([&](InkpodCore* core) {
            auto status = paint(core, main, INKPOD_PLANE_MAIN_LINE);
            return status == INKPOD_STATUS_OK ? paint(core, color, INKPOD_PLANE_COLOR) : status;
        }, true, true) != INKPOD_STATUS_OK) return 23200;
    // Eyedropper reads the selected plane, independently of the correction and renderer.
    const auto read = [&state](Pixels& output) {
        return state.engine->Invoke([&output](InkpodCore* core) {
            for (std::uint32_t y = 0U; y < 32U; ++y) {
                for (std::uint32_t x = 0U; x < 32U; ++x) {
                    InkpodColorValue pixel{};
                    pixel.struct_size = sizeof(pixel);
                    const auto status = inkpod_core_eyedropper(core, INKPOD_EYEDROPPER_SELECTED_PLANE, x, y, &pixel);
                    if (status == INKPOD_STATUS_INVALID_STATE) output[y * 32U + x] = 0U;
                    else if (status != INKPOD_STATUS_OK) return status;
                    else {
                        const auto divisor = pixel.depth == INKPOD_COLOR_DEPTH_16 ? 257U : 1U;
                        output[y * 32U + x] = (static_cast<std::uint32_t>(pixel.red / divisor) << 24U)
                            | (static_cast<std::uint32_t>(pixel.green / divisor) << 16U)
                            | (static_cast<std::uint32_t>(pixel.blue / divisor) << 8U) | (pixel.alpha / divisor);
                    }
                }
            }
            return INKPOD_STATUS_OK;
        }, false, false) == INKPOD_STATUS_OK;
    };
    for (const std::uint32_t mode : {INKPOD_LINE_REMOVE_DUST, INKPOD_LINE_FILL_HOLES,
            INKPOD_LINE_REPLACE_OUTLIERS, INKPOD_LINE_CONNECT, INKPOD_LINE_THICKEN,
            INKPOD_LINE_THIN, INKPOD_LINE_UNIFORM}) {
        const bool on_color = mode == INKPOD_LINE_REPLACE_OUTLIERS;
        SendMessageW(window, WM_COMMAND, on_color ? IDM_PLANE_COLOR : IDM_PLANE_MAIN_LINE, 0);
        const UINT command = mode <= INKPOD_LINE_REPLACE_OUTLIERS ? IDM_EFFECT_DUST
            : (mode == INKPOD_LINE_CONNECT ? IDM_EFFECT_LINE_CONNECT : IDM_EFFECT_LINE_WIDTH);
        SendMessageW(window, WM_COMMAND, command, 0);
        if (!panes::ShowToolOptionsFlyout(state.Workspace().windows.tool_options_flyout, canvas, command)) return 23201;
        // The flyout owns a separate form state; resolve it from the actual HWND.
        const auto* flyout = reinterpret_cast<const panes::ToolOptionsFlyoutState*>(
            GetWindowLongPtrW(state.Workspace().windows.tool_options_flyout, GWLP_USERDATA));
        if (flyout == nullptr || flyout->pane == nullptr || flyout->pane_state == nullptr) return 23202;
        const HWND pane = flyout->pane;
        const auto select = [pane](int id, std::uint32_t value) {
            const HWND combo = GetDlgItem(pane, id);
            const auto count = SendMessageW(combo, CB_GETCOUNT, 0, 0);
            for (LRESULT index = 0; index < count; ++index) {
                if (static_cast<std::uint32_t>(SendMessageW(combo, CB_GETITEMDATA, index, 0)) == value) {
                    SendMessageW(combo, CB_SETCURSEL, index, 0);
                    SendMessageW(pane, WM_COMMAND, MAKEWPARAM(id, CBN_SELCHANGE), reinterpret_cast<LPARAM>(combo));
                    return true;
                }
            }
            return false;
        };
        if (mode != INKPOD_LINE_CONNECT && !select(IDC_EFFECT_MODE, mode)) return 23203;
        // The shape control and numeric edit lose-focus route commit typed options.
        if (!select(IDC_EFFECT_CHANNEL, INKPOD_SELECTION_RECTANGLE)) return 23204;
        const HWND amount = GetDlgItem(pane, IDC_EFFECT_PARAMETER0);
        SetWindowTextW(amount, mode == INKPOD_LINE_UNIFORM ? L"3" : L"1");
        SendMessageW(pane, WM_COMMAND, MAKEWPARAM(IDC_EFFECT_PARAMETER0, EN_KILLFOCUS), reinterpret_cast<LPARAM>(amount));
        if (state.effects.options.mode != mode || state.effects.options.parameters[0] != (mode == INKPOD_LINE_UNIFORM ? 3 : 1)) return 23205;
        panes::HideToolOptionsFlyout(state.Workspace().windows.tool_options_flyout);
        if (FitCanvas(state, INKPOD_VIEW_FIT) != INKPOD_STATUS_OK
            || !WaitForSequencePresentation(state, true, std::nullopt, std::nullopt, std::nullopt)
            || SendMessageW(canvas, inkpod::renderer::kCanvasRenderOnce, 0, 0) != 1) return 23206;
        InkpodDocumentInfo before{};
        Pixels actual{};
        const auto& original = on_color ? color : main;
        if (!QueryDocument(state, before) || !read(actual) || actual != original) {
            const auto mismatch = std::mismatch(actual.begin(), actual.end(), original.begin());
            const auto index = static_cast<std::size_t>(mismatch.first - actual.begin());
            std::fprintf(stderr, "line fixture: mode=%u first=%zu actual=%08x expected=%08x active=%u\n",
                mode, index, index < actual.size() ? actual[index] : 0U,
                index < original.size() ? original[index] : 0U, state.Workspace().tools.active_plane);
            return 23207;
        }
        Pixels expected = original;
        float left{}, top{}, right{}, bottom{};
        if (mode == INKPOD_LINE_REMOVE_DUST) {
            left = 4.0F; top = 4.0F; right = 9.0F; bottom = 9.0F;
            expected[6U * 32U + 6U] = 0U;
        } else if (mode <= INKPOD_LINE_REPLACE_OUTLIERS) {
            left = 4.0F; top = 14.0F; right = 9.0F; bottom = 19.0F;
            expected[16U * 32U + 6U] = on_color ? UINT32_C(0xff0000ff) : UINT32_C(0x000000ff);
        } else if (mode == INKPOD_LINE_CONNECT) {
            left = 15.0F; top = 22.0F; right = 22.0F; bottom = 27.0F;
            expected[24U * 32U + 18U] = UINT32_C(0x000000ff);
        } else {
            left = 12.0F; top = 22.0F; right = 16.0F; bottom = 27.0F;
            for (std::uint32_t x = 12U; x < 16U; ++x) {
                if (mode == INKPOD_LINE_THIN) expected[24U * 32U + x] = 0U;
                else for (std::uint32_t y = 23U; y <= 25U; ++y) expected[y * 32U + x] = UINT32_C(0x000000ff);
            }
        }
        inkpod::renderer::CanvasDocumentBounds bounds{};
        if (!inkpod::renderer::GetCanvasDocumentBounds(canvas, bounds)) return 23208;
        const auto point = [&bounds](float x, float y) {
            return InkpodStrokeSample{sizeof(InkpodStrokeSample), 0U,
                static_cast<float>(bounds.left + (bounds.right - bounds.left) * x / 32.0),
                static_cast<float>(bounds.top + (bounds.bottom - bounds.top) * y / 32.0), 1.0F, 0U};
        };
        const auto a = point(left, top), b = point(right, bottom);
        using Kind = inkpod::renderer::CanvasStrokeEventKind;
        const auto send = [canvas](Kind kind, const InkpodStrokeSample* sample) {
            return inkpod::renderer::SubmitCanvasStrokeEvent(canvas,
                inkpod::renderer::CanvasStrokeEvent{kind, sample, sample == nullptr ? 0U : 1U});
        };
        if (!send(Kind::Begin, &a) || !send(Kind::Append, &b)) return 23209;
        inkpod::renderer::CanvasGeometryPreview preview{};
        preview.struct_size = sizeof(preview);
        if (!inkpod::renderer::GetCanvasGeometryPreview(canvas, preview) || preview.active != 1U
            || !send(Kind::Cancel, nullptr) || !read(actual) || actual != original) return 23210;
        if (!send(Kind::Begin, &a) || !send(Kind::End, &b) || !read(actual) || actual != expected) {
            std::fprintf(stderr, "line correction UI exact pixels failed: mode=%u\n", mode);
            std::size_t shown{};
            for (std::size_t index = 0U; index < actual.size() && shown < 12U; ++index) {
                if (actual[index] != expected[index]) {
                    ++shown;
                    std::fprintf(stderr, "  (%zu,%zu) actual=%08x expected=%08x\n",
                        index % 32U, index / 32U, actual[index], expected[index]);
                }
            }
            const auto& captured = state.Workspace().tools.procedure;
            std::fprintf(stderr, "  device=(%.9g,%.9g)..(%.9g,%.9g) frame=(%.17g,%.17g) zoom=%.17g\n",
                a.x, a.y, b.x, b.y, captured.canvas_left, captured.canvas_top, captured.zoom);
            return 23211;
        }
        InkpodDocumentInfo after{};
        if (!QueryDocument(state, after) || after.document_revision != before.document_revision + 1U
            || (on_color ? after.main_plane_checksum != before.main_plane_checksum
                         : after.color_plane_checksum != before.color_plane_checksum)) return 23212;
        SendMessageW(window, WM_COMMAND, IDM_EDIT_UNDO, 0);
        if (!read(actual) || actual != original) return 23213;
        SendMessageW(window, WM_COMMAND, IDM_EDIT_REDO, 0);
        if (!read(actual) || actual != expected) return 23214;
        SendMessageW(window, WM_COMMAND, IDM_EDIT_UNDO, 0);
        std::fprintf(stderr, "line correction UI: mode=%u exact pixels, Cancel, one Undo/Redo passed\n", mode);
    }
    SendMessageW(window, WM_COMMAND, IDM_LINE_CONNECT_APPLY, 0);
    const auto* flyout = reinterpret_cast<const panes::ToolOptionsFlyoutState*>(
        GetWindowLongPtrW(state.Workspace().windows.tool_options_flyout, GWLP_USERDATA));
    if (flyout == nullptr || flyout->pane == nullptr) return 23215;
    const HWND apply = GetDlgItem(flyout->pane, IDC_TOOL_OPTIONS_APPLY);
    if (apply == nullptr || IsWindowVisible(apply) == FALSE) return 23216;
    SendMessageW(apply, BM_CLICK, 0, 0);
    Pixels expected = main, actual{};
    expected[24U * 32U + 18U] = UINT32_C(0x000000ff);
    if (!read(actual) || actual != expected) return 23217;
    panes::HideToolOptionsFlyout(state.Workspace().windows.tool_options_flyout);
    SendMessageW(window, WM_COMMAND, IDM_EDIT_UNDO, 0);
    if (!read(actual) || actual != main) return 23218;
    std::fputs("line correction UI: menu Apply to whole plane passed\n", stderr);
    return 0;
}

}  // namespace inkpod::windows::ui::runtime
