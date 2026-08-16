#ifndef INKPOD_MACOS_CORE_C_H
#define INKPOD_MACOS_CORE_C_H

#include <inkpod/core_ffi.h>

/*
 * Clang does not import UINT32_C/UINT64_C-based macros into Swift. Keep these
 * private inline accessors in the bridge module instead of copying the public
 * ABI header or teaching product code the numeric constants.
 */
static inline uint32_t inkpod_bridge_abi_version(void) {
    return INKPOD_ABI_VERSION;
}

static inline uint64_t inkpod_bridge_feature_none(void) {
    return INKPOD_FEATURE_NONE;
}

static inline InkpodStatus inkpod_bridge_status_ok(void) {
    return INKPOD_STATUS_OK;
}

static inline InkpodStatus inkpod_bridge_status_invalid_argument(void) {
    return INKPOD_STATUS_INVALID_ARGUMENT;
}

static inline InkpodStatus inkpod_bridge_status_incompatible_abi(void) {
    return INKPOD_STATUS_INCOMPATIBLE_ABI;
}

static inline InkpodStatus inkpod_bridge_status_buffer_too_small(void) {
    return INKPOD_STATUS_BUFFER_TOO_SMALL;
}

static inline InkpodStatus inkpod_bridge_status_unsupported(void) {
    return INKPOD_STATUS_UNSUPPORTED;
}

static inline InkpodStatus inkpod_bridge_status_panic(void) {
    return INKPOD_STATUS_PANIC;
}

static inline InkpodStatus inkpod_bridge_status_wrong_thread(void) {
    return INKPOD_STATUS_WRONG_THREAD;
}

static inline InkpodStatus inkpod_bridge_status_io_error(void) {
    return INKPOD_STATUS_IO_ERROR;
}

static inline InkpodStatus inkpod_bridge_status_invalid_state(void) {
    return INKPOD_STATUS_INVALID_STATE;
}

static inline InkpodStatus inkpod_bridge_status_no_document(void) {
    return INKPOD_STATUS_NO_DOCUMENT;
}

static inline InkpodStatus inkpod_bridge_status_cancelled(void) {
    return INKPOD_STATUS_CANCELLED;
}

static inline InkpodStatus inkpod_bridge_status_fill_overflow(void) {
    return INKPOD_STATUS_FILL_OVERFLOW;
}

static inline InkpodStatus inkpod_bridge_status_unsaved_changes(void) {
    return INKPOD_STATUS_UNSAVED_CHANGES;
}

static inline InkpodPaintTool inkpod_bridge_tool_pencil(void) {
    return INKPOD_TOOL_PENCIL;
}

static inline InkpodPlaneKind inkpod_bridge_plane_color(void) {
    return INKPOD_PLANE_COLOR;
}

static inline InkpodCoordinateSpace inkpod_bridge_coordinate_document(void) {
    return INKPOD_COORDINATE_SPACE_DOCUMENT;
}

static inline InkpodCoordinateSpace inkpod_bridge_coordinate_device(void) {
    return INKPOD_COORDINATE_SPACE_DEVICE;
}

static inline uint64_t inkpod_bridge_stroke_auto_erase(void) {
    return INKPOD_STROKE_FLAG_AUTO_ERASE;
}

static inline InkpodBrushShape inkpod_bridge_brush_round(void) {
    return INKPOD_BRUSH_ROUND;
}

static inline InkpodStartColorPredicate inkpod_bridge_start_color_any(void) {
    return INKPOD_START_COLOR_ANY;
}

static inline InkpodEditorUpdateKind inkpod_bridge_editor_update_active_tool(void) {
    return INKPOD_EDITOR_UPDATE_ACTIVE_TOOL;
}

static inline InkpodEditorUpdateKind inkpod_bridge_editor_update_tool_color(void) {
    return INKPOD_EDITOR_UPDATE_TOOL_COLOR;
}

static inline InkpodEditorUpdateKind inkpod_bridge_editor_update_tool_diameter(void) {
    return INKPOD_EDITOR_UPDATE_TOOL_DIAMETER;
}

static inline InkpodEditorUpdateKind inkpod_bridge_editor_update_fill_options(void) {
    return INKPOD_EDITOR_UPDATE_FILL_OPTIONS;
}

static inline InkpodEditorUpdateKind inkpod_bridge_editor_update_brush_options(void) {
    return INKPOD_EDITOR_UPDATE_BRUSH_OPTIONS;
}

static inline InkpodEditorUpdateKind inkpod_bridge_editor_update_selection_options(void) {
    return INKPOD_EDITOR_UPDATE_SELECTION_OPTIONS;
}

static inline uint64_t inkpod_bridge_editor_fill_flags(
    uint32_t detached,
    uint32_t overflow_abort,
    uint32_t transparent_only,
    uint32_t document_selection,
    uint32_t light_table_boundary,
    uint32_t light_table_color) {
    return (detached ? INKPOD_EDITOR_FILL_DETACHED_REGIONS : 0)
        | (overflow_abort ? INKPOD_EDITOR_FILL_OVERFLOW_ABORT : 0)
        | (transparent_only ? INKPOD_EDITOR_FILL_TRANSPARENT_ONLY : 0)
        | (document_selection ? INKPOD_EDITOR_FILL_DOCUMENT_SELECTION : 0)
        | (light_table_boundary ? INKPOD_EDITOR_FILL_LIGHT_TABLE_BOUNDARY : 0)
        | (light_table_color ? INKPOD_EDITOR_FILL_LIGHT_TABLE_COLOR : 0);
}

static inline uint64_t inkpod_bridge_fill_flags(
    uint32_t detached,
    uint32_t overflow_abort,
    uint32_t transparent_only,
    uint32_t operation_selection,
    uint32_t document_selection,
    uint32_t light_table_boundary,
    uint32_t light_table_color) {
    return (detached ? INKPOD_FILL_FLAG_DETACHED_REGIONS : 0)
        | (overflow_abort ? INKPOD_FILL_FLAG_OVERFLOW_ABORT : 0)
        | (transparent_only ? INKPOD_FILL_FLAG_TRANSPARENT_ONLY : 0)
        | (operation_selection ? INKPOD_FILL_FLAG_SELECTION_PRESENT : 0)
        | (document_selection ? INKPOD_FILL_FLAG_DOCUMENT_SELECTION : 0)
        | (light_table_boundary ? INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY : 0)
        | (light_table_color ? INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR : 0);
}

static inline uint32_t inkpod_bridge_fill_result_leak_candidate(void) {
    return INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE;
}

static inline uint32_t inkpod_bridge_color_chart_locked(void) {
    return INKPOD_COLOR_CHART_LOCKED;
}

static inline uint32_t inkpod_bridge_color_chart_has_selection(void) {
    return INKPOD_COLOR_CHART_HAS_SELECTION;
}

static inline uint32_t inkpod_bridge_color_chart_preview_exceeds_maximum(void) {
    return INKPOD_COLOR_CHART_PREVIEW_EXCEEDS_MAXIMUM;
}

static inline uint32_t inkpod_bridge_locator_selection_present(void) {
    return INKPOD_LOCATOR_SELECTION_PRESENT;
}

static inline uint32_t inkpod_bridge_locator_color_present(void) {
    return INKPOD_LOCATOR_COLOR_PRESENT;
}

static inline uint64_t inkpod_bridge_color_replace_has_region(void) {
    return INKPOD_COLOR_REPLACE_HAS_REGION;
}

static inline uint32_t inkpod_bridge_color_replace_preview_has_bounds(void) {
    return INKPOD_COLOR_REPLACE_PREVIEW_HAS_BOUNDS;
}

static inline InkpodOutputColorGuardProfile inkpod_bridge_output_guard_profile(void) {
    return INKPOD_OUTPUT_COLOR_GUARD_BT709_CONSERVATIVE_YCBCR;
}

static inline InkpodViewCommandKind inkpod_bridge_view_pan_by(void) {
    return INKPOD_VIEW_PAN_BY;
}

static inline InkpodViewCommandKind inkpod_bridge_view_zoom_at(void) {
    return INKPOD_VIEW_ZOOM_AT;
}

static inline InkpodViewCommandKind inkpod_bridge_view_viewport_resized(void) {
    return INKPOD_VIEW_VIEWPORT_RESIZED;
}

static inline InkpodViewCommandKind inkpod_bridge_view_fit(void) {
    return INKPOD_VIEW_FIT;
}

static inline InkpodViewCommandKind inkpod_bridge_view_one_to_one(void) {
    return INKPOD_VIEW_ONE_TO_ONE;
}

static inline InkpodViewCommandKind inkpod_bridge_view_box_zoom(void) {
    return INKPOD_VIEW_BOX_ZOOM;
}

static inline InkpodViewCommandKind inkpod_bridge_view_flip_horizontal(void) {
    return INKPOD_VIEW_FLIP_HORIZONTAL;
}

static inline InkpodViewCommandKind inkpod_bridge_view_flip_vertical(void) {
    return INKPOD_VIEW_FLIP_VERTICAL;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_ruler_visible(void) {
    return INKPOD_VIEW_SET_RULER_VISIBLE;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_guides_visible(void) {
    return INKPOD_VIEW_SET_GUIDES_VISIBLE;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_grid_visible(void) {
    return INKPOD_VIEW_SET_GRID_VISIBLE;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_guide_snap_enabled(void) {
    return INKPOD_VIEW_SET_GUIDE_SNAP_ENABLED;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_grid_snap_enabled(void) {
    return INKPOD_VIEW_SET_GRID_SNAP_ENABLED;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_transparent_visible(void) {
    return INKPOD_VIEW_SET_TRANSPARENT_VISIBLE;
}

static inline uint32_t inkpod_bridge_guide_horizontal(void) {
    return INKPOD_GUIDE_HORIZONTAL;
}

static inline uint32_t inkpod_bridge_guide_vertical(void) {
    return INKPOD_GUIDE_VERTICAL;
}

static inline InkpodShortcutKeyKind inkpod_bridge_shortcut_key_unicode_scalar(void) {
    return INKPOD_SHORTCUT_KEY_UNICODE_SCALAR;
}

static inline InkpodShortcutKeyKind inkpod_bridge_shortcut_key_named(void) {
    return INKPOD_SHORTCUT_KEY_NAMED;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_tab(void) {
    return INKPOD_SHORTCUT_NAMED_TAB;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_return(void) {
    return INKPOD_SHORTCUT_NAMED_RETURN;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_escape(void) {
    return INKPOD_SHORTCUT_NAMED_ESCAPE;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_space(void) {
    return INKPOD_SHORTCUT_NAMED_SPACE;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_backspace(void) {
    return INKPOD_SHORTCUT_NAMED_BACKSPACE;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_delete(void) {
    return INKPOD_SHORTCUT_NAMED_DELETE;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_left(void) {
    return INKPOD_SHORTCUT_NAMED_LEFT;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_right(void) {
    return INKPOD_SHORTCUT_NAMED_RIGHT;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_up(void) {
    return INKPOD_SHORTCUT_NAMED_UP;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_down(void) {
    return INKPOD_SHORTCUT_NAMED_DOWN;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_home(void) {
    return INKPOD_SHORTCUT_NAMED_HOME;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_end(void) {
    return INKPOD_SHORTCUT_NAMED_END;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_page_up(void) {
    return INKPOD_SHORTCUT_NAMED_PAGE_UP;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_page_down(void) {
    return INKPOD_SHORTCUT_NAMED_PAGE_DOWN;
}

static inline InkpodShortcutNamedKey inkpod_bridge_shortcut_named_f1(void) {
    return INKPOD_SHORTCUT_NAMED_F1;
}

static inline uint32_t inkpod_bridge_shortcut_modifier_primary(void) {
    return INKPOD_SHORTCUT_MODIFIER_PRIMARY;
}

static inline uint32_t inkpod_bridge_shortcut_modifier_shift(void) {
    return INKPOD_SHORTCUT_MODIFIER_SHIFT;
}

static inline uint32_t inkpod_bridge_shortcut_modifier_alternate(void) {
    return INKPOD_SHORTCUT_MODIFIER_ALTERNATE;
}

static inline uint32_t inkpod_bridge_shortcut_modifier_control(void) {
    return INKPOD_SHORTCUT_MODIFIER_CONTROL;
}

static inline InkpodShortcutMatch inkpod_bridge_shortcut_match_prefix(void) {
    return INKPOD_SHORTCUT_MATCH_PREFIX;
}

static inline InkpodShortcutMatch inkpod_bridge_shortcut_match_exact(void) {
    return INKPOD_SHORTCUT_MATCH_EXACT;
}

static inline void inkpod_bridge_shortcut_sequence_set_stroke(
    InkpodShortcutSequenceV2* sequence,
    uint32_t index,
    InkpodShortcutStrokeV2 stroke) {
    if (sequence != NULL && index < INKPOD_SHORTCUT_MAX_STROKES) {
        sequence->strokes[index] = stroke;
    }
}

static inline uint32_t inkpod_bridge_document_can_undo(void) {
    return INKPOD_DOCUMENT_FLAG_CAN_UNDO;
}

static inline uint32_t inkpod_bridge_document_can_redo(void) {
    return INKPOD_DOCUMENT_FLAG_CAN_REDO;
}

static inline uint32_t inkpod_bridge_document_dirty(void) {
    return INKPOD_DOCUMENT_FLAG_DIRTY;
}

static inline uint32_t inkpod_bridge_document_recovered(void) {
    return INKPOD_DOCUMENT_FLAG_RECOVERED;
}

static inline InkpodCommonRasterFormat inkpod_bridge_common_raster_png(void) {
    return INKPOD_COMMON_RASTER_PNG;
}

static inline InkpodCommonRasterFormat inkpod_bridge_common_raster_tiff(void) {
    return INKPOD_COMMON_RASTER_TIFF;
}

static inline InkpodCommonRasterFormat inkpod_bridge_common_raster_tga(void) {
    return INKPOD_COMMON_RASTER_TGA;
}

static inline InkpodCommonRasterFormat inkpod_bridge_common_raster_bmp(void) {
    return INKPOD_COMMON_RASTER_BMP;
}

static inline uint32_t inkpod_bridge_paste_compatible(void) {
    return INKPOD_PASTE_COMPATIBLE;
}

static inline uint32_t inkpod_bridge_paste_active_converted(void) {
    return INKPOD_PASTE_ACTIVE_CONVERTED;
}

static inline InkpodTreeOperation inkpod_bridge_tree_create_plane(void) {
    return INKPOD_TREE_CREATE_PLANE;
}

static inline uint32_t inkpod_bridge_cell_sizing_image_pixels(void) {
    return INKPOD_CELL_SIZING_IMAGE_PIXELS;
}

static inline uint32_t inkpod_bridge_cell_sizing_frame_micrometres(void) {
    return INKPOD_CELL_SIZING_FRAME_MICROMETRES;
}

static inline InkpodTreeOperation inkpod_bridge_tree_create_layer(void) {
    return INKPOD_TREE_CREATE_LAYER;
}

static inline InkpodTreeOperation inkpod_bridge_tree_duplicate_layer(void) {
    return INKPOD_TREE_DUPLICATE_LAYER;
}

static inline InkpodTreeOperation inkpod_bridge_tree_delete_layer(void) {
    return INKPOD_TREE_DELETE_LAYER;
}

static inline InkpodTreeOperation inkpod_bridge_tree_reorder_layer(void) {
    return INKPOD_TREE_REORDER_LAYER;
}

static inline InkpodTreeOperation inkpod_bridge_tree_set_layer_properties(void) {
    return INKPOD_TREE_SET_LAYER_PROPERTIES;
}

static inline InkpodTreeOperation inkpod_bridge_tree_duplicate_plane(void) {
    return INKPOD_TREE_DUPLICATE_PLANE;
}

static inline InkpodTreeOperation inkpod_bridge_tree_delete_plane(void) {
    return INKPOD_TREE_DELETE_PLANE;
}

static inline InkpodTreeOperation inkpod_bridge_tree_reorder_plane(void) {
    return INKPOD_TREE_REORDER_PLANE;
}

static inline InkpodTreeOperation inkpod_bridge_tree_set_plane_properties(void) {
    return INKPOD_TREE_SET_PLANE_PROPERTIES;
}

static inline InkpodTreeOperation inkpod_bridge_tree_convert_layer(void) {
    return INKPOD_TREE_CONVERT_LAYER;
}

static inline InkpodTreeOperation inkpod_bridge_tree_merge_layer(void) {
    return INKPOD_TREE_MERGE_LAYER;
}

static inline InkpodTreeOperation inkpod_bridge_tree_convert_plane(void) {
    return INKPOD_TREE_CONVERT_PLANE;
}

static inline InkpodTreeOperation inkpod_bridge_tree_merge_plane(void) {
    return INKPOD_TREE_MERGE_PLANE;
}

static inline InkpodTreeOperation inkpod_bridge_tree_delete_hidden_layers(void) {
    return INKPOD_TREE_DELETE_HIDDEN_LAYERS;
}

static inline uint32_t inkpod_bridge_node_visible(void) {
    return INKPOD_NODE_VISIBLE;
}

static inline uint32_t inkpod_bridge_node_editable(void) {
    return INKPOD_NODE_EDITABLE;
}

static inline uint32_t inkpod_bridge_node_visible_editable(void) {
    return INKPOD_NODE_VISIBLE | INKPOD_NODE_EDITABLE;
}

static inline InkpodTypedPlaneKind inkpod_bridge_typed_plane_raster(void) {
    return INKPOD_TYPED_PLANE_RASTER;
}

static inline InkpodStoragePixelFormat inkpod_bridge_storage_rgba8(void) {
    return INKPOD_STORAGE_RGBA8;
}

static inline uint64_t inkpod_bridge_document_resize_resample(void) {
    return INKPOD_DOCUMENT_RESIZE_RESAMPLE;
}

static inline uint32_t inkpod_bridge_mirror_horizontal(void) {
    return INKPOD_MIRROR_HORIZONTAL;
}

static inline uint32_t inkpod_bridge_mirror_vertical(void) {
    return INKPOD_MIRROR_VERTICAL;
}

static inline uint32_t inkpod_bridge_rotate_left(void) {
    return INKPOD_ROTATE_LEFT_90;
}

static inline uint32_t inkpod_bridge_rotate_right(void) {
    return INKPOD_ROTATE_RIGHT_90;
}

static inline InkpodEditorTool inkpod_bridge_editor_tool_pencil(void) {
    return INKPOD_EDITOR_TOOL_PENCIL;
}

static inline InkpodEditorTool inkpod_bridge_editor_tool_selection(void) {
    return INKPOD_EDITOR_TOOL_SELECTION;
}

static inline InkpodEditorTool inkpod_bridge_editor_tool_floating_transform(void) {
    return INKPOD_EDITOR_TOOL_FLOATING_TRANSFORM;
}

static inline InkpodSelectionShape inkpod_bridge_selection_rectangle(void) {
    return INKPOD_SELECTION_RECTANGLE;
}

static inline InkpodSelectionShape inkpod_bridge_selection_ellipse(void) {
    return INKPOD_SELECTION_ELLIPSE;
}

static inline InkpodSelectionShape inkpod_bridge_selection_lasso(void) {
    return INKPOD_SELECTION_LASSO;
}

static inline InkpodSelectionShape inkpod_bridge_selection_polyline(void) {
    return INKPOD_SELECTION_POLYLINE;
}

static inline InkpodSelectionShape inkpod_bridge_selection_trace(void) {
    return INKPOD_SELECTION_TRACE;
}

static inline InkpodSelectionShape inkpod_bridge_selection_wand(void) {
    return INKPOD_SELECTION_WAND;
}

static inline InkpodSelectionOperation inkpod_bridge_selection_new(void) {
    return INKPOD_SELECTION_NEW;
}

static inline InkpodSelectionOperation inkpod_bridge_selection_add(void) {
    return INKPOD_SELECTION_ADD;
}

static inline InkpodSelectionOperation inkpod_bridge_selection_subtract(void) {
    return INKPOD_SELECTION_SUBTRACT;
}

static inline InkpodSelectionOperation inkpod_bridge_selection_intersect(void) {
    return INKPOD_SELECTION_INTERSECT;
}

static inline InkpodRangeInterpretation inkpod_bridge_range_normal(void) {
    return INKPOD_RANGE_NORMAL;
}

static inline InkpodRangeInterpretation inkpod_bridge_range_tight(void) {
    return INKPOD_RANGE_TIGHT;
}

static inline InkpodRangeInterpretation inkpod_bridge_range_enclosed_interior(void) {
    return INKPOD_RANGE_ENCLOSED_INTERIOR;
}

static inline InkpodRangeInterpretation inkpod_bridge_range_drawing(void) {
    return INKPOD_RANGE_DRAWING;
}

static inline InkpodRangeInterpretation inkpod_bridge_range_boundary(void) {
    return INKPOD_RANGE_BOUNDARY;
}

static inline InkpodTraceBrushShape inkpod_bridge_trace_round(void) {
    return INKPOD_TRACE_ROUND;
}

static inline InkpodTraceBrushShape inkpod_bridge_trace_square(void) {
    return INKPOD_TRACE_SQUARE;
}

static inline uint64_t inkpod_bridge_selection_construction_flags(
    uint32_t from_center,
    uint32_t constrain_rotation,
    uint32_t pressure_size,
    uint32_t screen_size) {
    return (from_center ? INKPOD_SELECTION_FROM_CENTER : 0)
        | (constrain_rotation ? INKPOD_SELECTION_CONSTRAIN_ROTATION_45 : 0)
        | (pressure_size ? INKPOD_SELECTION_TRACE_PRESSURE_SIZE : 0)
        | (screen_size ? INKPOD_SELECTION_TRACE_SCREEN_SIZE : 0);
}

static inline uint32_t inkpod_bridge_selection_adjust_invert(void) {
    return INKPOD_SELECTION_ADJUST_INVERT;
}

static inline uint32_t inkpod_bridge_selection_adjust_expand(void) {
    return INKPOD_SELECTION_ADJUST_EXPAND;
}

static inline uint32_t inkpod_bridge_selection_adjust_shrink(void) {
    return INKPOD_SELECTION_ADJUST_SHRINK;
}

static inline uint32_t inkpod_bridge_selection_layer_replace(void) {
    return INKPOD_SELECTION_LAYER_REPLACE;
}

static inline uint32_t inkpod_bridge_selection_layer_add(void) {
    return INKPOD_SELECTION_LAYER_ADD;
}

static inline uint32_t inkpod_bridge_selection_layer_subtract(void) {
    return INKPOD_SELECTION_LAYER_SUBTRACT;
}

static inline uint32_t inkpod_bridge_transform_anchor_top_left(void) {
    return INKPOD_TRANSFORM_ANCHOR_TOP_LEFT;
}

static inline uint32_t inkpod_bridge_transform_anchor_top_right(void) {
    return INKPOD_TRANSFORM_ANCHOR_TOP_RIGHT;
}

static inline uint32_t inkpod_bridge_transform_anchor_center(void) {
    return INKPOD_TRANSFORM_ANCHOR_CENTER;
}

static inline uint32_t inkpod_bridge_transform_anchor_bottom_left(void) {
    return INKPOD_TRANSFORM_ANCHOR_BOTTOM_LEFT;
}

static inline uint32_t inkpod_bridge_transform_anchor_bottom_right(void) {
    return INKPOD_TRANSFORM_ANCHOR_BOTTOM_RIGHT;
}

static inline uint32_t inkpod_bridge_history_item_applied(void) {
    return INKPOD_HISTORY_ITEM_APPLIED;
}

static inline InkpodPixelFormat inkpod_bridge_pixel_premultiplied_bgra8(void) {
    return INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8;
}

static inline uint64_t inkpod_bridge_snapshot_solid_white_base(void) {
    return INKPOD_SNAPSHOT_FEATURE_SOLID_WHITE_BASE;
}

static inline uint64_t inkpod_bridge_snapshot_color_check_legacy_white(void) {
    return INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE;
}

static inline uint64_t inkpod_bridge_snapshot_color_check_native_alpha(void) {
    return INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA;
}

static inline uint32_t inkpod_bridge_snapshot_overlay_transparent_view(void) {
    return INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_alpha_visible(void) {
    return INKPOD_VIEW_SET_ALPHA_VISIBLE;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_vector_antialias(void) {
    return INKPOD_VIEW_SET_VECTOR_ANTIALIAS;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_vector_centerline_mode(void) {
    return INKPOD_VIEW_SET_VECTOR_CENTERLINE_MODE;
}

static inline InkpodViewCommandKind inkpod_bridge_view_set_vector_endpoints_visible(void) {
    return INKPOD_VIEW_SET_VECTOR_ENDPOINTS_VISIBLE;
}

#endif /* INKPOD_MACOS_CORE_C_H */
