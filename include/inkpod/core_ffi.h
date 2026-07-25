#ifndef INKPOD_CORE_FFI_H
#define INKPOD_CORE_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define INKPOD_ABI_VERSION UINT32_C(1)
#define INKPOD_FEATURE_NONE UINT64_C(0)

typedef uint32_t InkpodStatus;
#define INKPOD_STATUS_OK UINT32_C(0)
#define INKPOD_STATUS_INVALID_ARGUMENT UINT32_C(1)
#define INKPOD_STATUS_INCOMPATIBLE_ABI UINT32_C(2)
#define INKPOD_STATUS_BUFFER_TOO_SMALL UINT32_C(3)
#define INKPOD_STATUS_UNSUPPORTED UINT32_C(4)
#define INKPOD_STATUS_PANIC UINT32_C(5)
#define INKPOD_STATUS_WRONG_THREAD UINT32_C(6)
#define INKPOD_STATUS_IO_ERROR UINT32_C(7)
#define INKPOD_STATUS_INVALID_STATE UINT32_C(8)
#define INKPOD_STATUS_NO_DOCUMENT UINT32_C(9)
#define INKPOD_STATUS_CANCELLED UINT32_C(10)
#define INKPOD_STATUS_FILL_OVERFLOW UINT32_C(11)
#define INKPOD_STATUS_UNSAVED_CHANGES UINT32_C(12)

typedef uint32_t InkpodCommandKind;
#define INKPOD_COMMAND_NO_OP UINT32_C(0)

typedef uint32_t InkpodPixelFormat;
#define INKPOD_PIXEL_FORMAT_INVALID UINT32_C(0)
#define INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8 UINT32_C(1)
#define INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_LEGACY_WHITE (UINT64_C(1) << 0)
#define INKPOD_SNAPSHOT_FEATURE_COLOR_CHECK_NATIVE_ALPHA (UINT64_C(1) << 1)

typedef uint32_t InkpodCommonRasterFormat;
#define INKPOD_COMMON_RASTER_PNG UINT32_C(1)
#define INKPOD_COMMON_RASTER_TIFF UINT32_C(2)
#define INKPOD_COMMON_RASTER_TGA UINT32_C(3)
#define INKPOD_COMMON_RASTER_BMP UINT32_C(4)

typedef uint32_t InkpodPlaneKind;
#define INKPOD_PLANE_MAIN_LINE UINT32_C(1)
#define INKPOD_PLANE_COLOR UINT32_C(2)

typedef uint32_t InkpodPaintTool;
#define INKPOD_TOOL_PENCIL UINT32_C(1)
#define INKPOD_TOOL_BRUSH UINT32_C(2)
#define INKPOD_TOOL_ERASER UINT32_C(3)

typedef uint32_t InkpodCoordinateSpace;
#define INKPOD_COORDINATE_SPACE_DOCUMENT UINT32_C(1)
#define INKPOD_COORDINATE_SPACE_DEVICE UINT32_C(2)

#define INKPOD_STROKE_FLAG_AUTO_ERASE (UINT64_C(1) << 0)
#define INKPOD_STROKE_FLAG_PRESSURE_SIZE (UINT64_C(1) << 1)

#define INKPOD_DOCUMENT_FLAG_DIRTY (UINT32_C(1) << 0)
#define INKPOD_DOCUMENT_FLAG_CAN_UNDO (UINT32_C(1) << 1)
#define INKPOD_DOCUMENT_FLAG_CAN_REDO (UINT32_C(1) << 2)
#define INKPOD_DOCUMENT_FLAG_RECOVERED (UINT32_C(1) << 3)
#define INKPOD_HISTORY_ITEM_APPLIED (UINT32_C(1) << 0)

typedef uint32_t InkpodViewCommandKind;
#define INKPOD_VIEW_PAN_BY UINT32_C(1)
#define INKPOD_VIEW_ZOOM_AT UINT32_C(2)
#define INKPOD_VIEW_FIT UINT32_C(3)
#define INKPOD_VIEW_ONE_TO_ONE UINT32_C(4)
#define INKPOD_VIEW_VIEWPORT_RESIZED UINT32_C(5)
#define INKPOD_VIEW_BOX_ZOOM UINT32_C(6)
#define INKPOD_VIEW_FLIP_HORIZONTAL UINT32_C(7)
#define INKPOD_VIEW_FLIP_VERTICAL UINT32_C(8)
#define INKPOD_VIEW_SET_RULER_VISIBLE UINT32_C(9)
#define INKPOD_VIEW_SET_GUIDES_VISIBLE UINT32_C(10)
#define INKPOD_VIEW_SET_GRID_VISIBLE UINT32_C(11)
#define INKPOD_VIEW_SET_SNAP_ENABLED UINT32_C(12)
#define INKPOD_VIEW_SET_TRANSPARENT_VISIBLE UINT32_C(13)
#define INKPOD_VIEW_SET_ALPHA_VISIBLE UINT32_C(14)
#define INKPOD_VIEW_SET_GUIDE_SNAP_ENABLED UINT32_C(15)
#define INKPOD_VIEW_SET_GRID_SNAP_ENABLED UINT32_C(16)

#define INKPOD_SNAPSHOT_TRANSFORM_FLIP_HORIZONTAL (UINT32_C(1) << 0)
#define INKPOD_SNAPSHOT_TRANSFORM_FLIP_VERTICAL (UINT32_C(1) << 1)
#define INKPOD_SNAPSHOT_OVERLAY_RULER_VISIBLE (UINT32_C(1) << 0)
#define INKPOD_SNAPSHOT_OVERLAY_GUIDES_VISIBLE (UINT32_C(1) << 1)
#define INKPOD_SNAPSHOT_OVERLAY_GRID_VISIBLE (UINT32_C(1) << 2)
#define INKPOD_SNAPSHOT_OVERLAY_SNAP_ENABLED (UINT32_C(1) << 3)
#define INKPOD_SNAPSHOT_OVERLAY_TRANSPARENT_VIEW (UINT32_C(1) << 4)
#define INKPOD_SNAPSHOT_OVERLAY_ALPHA_VIEW (UINT32_C(1) << 5)

#define INKPOD_SHORTCUT_MODIFIER_CONTROL (UINT32_C(1) << 0)
#define INKPOD_SHORTCUT_MODIFIER_SHIFT (UINT32_C(1) << 1)
#define INKPOD_SHORTCUT_MODIFIER_ALT (UINT32_C(1) << 2)
#define INKPOD_SHORTCUT_MODIFIER_EXTENDED (UINT32_C(1) << 3)

typedef uint32_t InkpodColorDepth;
#define INKPOD_COLOR_DEPTH_BINARY UINT32_C(1)
#define INKPOD_COLOR_DEPTH_GRAYSCALE_8 UINT32_C(2)
#define INKPOD_COLOR_DEPTH_GRAYSCALE_16 UINT32_C(3)
#define INKPOD_COLOR_DEPTH_8 UINT32_C(8)
#define INKPOD_COLOR_DEPTH_16 UINT32_C(16)

typedef uint32_t InkpodFillOperation;
#define INKPOD_FILL_SEED UINT32_C(1)
#define INKPOD_FILL_CLOSED_REGION UINT32_C(2)
#define INKPOD_FILL_EXTENSION UINT32_C(3)
#define INKPOD_FILL_FLAG_DETACHED_REGIONS (UINT64_C(1) << 0)
#define INKPOD_FILL_FLAG_OVERFLOW_ABORT (UINT64_C(1) << 1)
#define INKPOD_FILL_FLAG_TRANSPARENT_ONLY (UINT64_C(1) << 2)
#define INKPOD_FILL_FLAG_SELECTION_PRESENT (UINT64_C(1) << 3)
#define INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY (UINT64_C(1) << 4)
#define INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR (UINT64_C(1) << 5)
#define INKPOD_FILL_FLAG_DOCUMENT_SELECTION (UINT64_C(1) << 6)

typedef uint32_t InkpodInclusionMode;
#define INKPOD_INCLUSION_NONE UINT32_C(0)
#define INKPOD_INCLUSION_SPECIFIED UINT32_C(1)
#define INKPOD_INCLUSION_EXCEPT_SPECIFIED UINT32_C(2)

#define INKPOD_FILL_RESULT_FLAG_LEAK_CANDIDATE (UINT32_C(1) << 0)

typedef uint32_t InkpodEyedropperSource;
#define INKPOD_EYEDROPPER_TOPMOST_NONTRANSPARENT UINT32_C(1)
#define INKPOD_EYEDROPPER_SELECTED_PLANE UINT32_C(2)
#define INKPOD_EYEDROPPER_COMPOSITE UINT32_C(3)
#define INKPOD_EYEDROPPER_LIGHT_TABLE_TOPMOST UINT32_C(4)

typedef uint32_t InkpodColorCheckMode;
#define INKPOD_COLOR_CHECK_OFF UINT32_C(0)
#define INKPOD_COLOR_CHECK_LEGACY_WHITE UINT32_C(1)
#define INKPOD_COLOR_CHECK_NATIVE_ALPHA UINT32_C(2)

typedef uint32_t InkpodLayerKind;
#define INKPOD_LAYER_BINARY_COLORING UINT32_C(1)
#define INKPOD_LAYER_GRAYSCALE_COLORING UINT32_C(2)
#define INKPOD_LAYER_RASTER UINT32_C(3)
#define INKPOD_LAYER_SELECTION UINT32_C(4)
#define INKPOD_LAYER_FRAME UINT32_C(5)
#define INKPOD_LAYER_VANISHING_POINT UINT32_C(6)
#define INKPOD_LAYER_ADJUSTMENT UINT32_C(7)
#define INKPOD_LAYER_TEXT UINT32_C(8)
#define INKPOD_LAYER_ANNOTATION UINT32_C(9)
#define INKPOD_LAYER_VECTOR_COLORING UINT32_C(10)

typedef uint32_t InkpodTypedPlaneKind;
#define INKPOD_TYPED_PLANE_MAIN_LINE UINT32_C(1)
#define INKPOD_TYPED_PLANE_COLOR UINT32_C(2)
#define INKPOD_TYPED_PLANE_RASTER UINT32_C(3)
#define INKPOD_TYPED_PLANE_SELECTION UINT32_C(4)
#define INKPOD_TYPED_PLANE_VECTOR_MAIN_LINE UINT32_C(5)
#define INKPOD_TYPED_PLANE_COLOR_TRACE UINT32_C(6)
#define INKPOD_TYPED_PLANE_VECTOR_FILL UINT32_C(7)

#define INKPOD_VECTOR_PATH_CLOSED (UINT64_C(1) << 0)
typedef uint32_t InkpodVectorEraseMode;
#define INKPOD_VECTOR_ERASE_PARTIAL UINT32_C(1)
#define INKPOD_VECTOR_ERASE_TO_INTERSECTION UINT32_C(2)
#define INKPOD_VECTOR_ERASE_WHOLE_PATH UINT32_C(3)
typedef uint32_t InkpodVectorWidthMode;
#define INKPOD_VECTOR_WIDTH_ADD UINT32_C(1)
#define INKPOD_VECTOR_WIDTH_SUBTRACT UINT32_C(2)
#define INKPOD_VECTOR_WIDTH_SCALE UINT32_C(3)
#define INKPOD_VECTOR_WIDTH_CONSTANT UINT32_C(4)
typedef uint32_t InkpodVectorSelectionMode;
#define INKPOD_VECTOR_SELECT_CUT_BY_SELECTION UINT32_C(1)
#define INKPOD_VECTOR_SELECT_TOUCHING UINT32_C(2)
#define INKPOD_VECTOR_SELECT_FULLY_CONTAINED UINT32_C(3)
#define INKPOD_VECTOR_SELECT_LINE UINT32_C(4)
#define INKPOD_VECTOR_SELECT_WHOLE_LINE UINT32_C(5)
#define INKPOD_VECTOR_SELECT_TO_INTERSECTION UINT32_C(6)
#define INKPOD_VECTOR_SELECT_FILL_BOUNDARY UINT32_C(7)
#define INKPOD_VECTOR_SELECT_FILL UINT32_C(8)
#define INKPOD_VECTOR_RASTERIZE_ANTIALIAS (UINT64_C(1) << 0)
#define INKPOD_SNAPSHOT_VECTOR_CLOSED (UINT32_C(1) << 0)
#define INKPOD_SNAPSHOT_VECTOR_STROKE_VISIBLE (UINT32_C(1) << 1)

typedef uint32_t InkpodFilterKind;
#define INKPOD_FILTER_SHARPEN_WEAK UINT32_C(1)
#define INKPOD_FILTER_SHARPEN_STRONG UINT32_C(2)
#define INKPOD_FILTER_BLUR_WEAK UINT32_C(3)
#define INKPOD_FILTER_BLUR_STRONG UINT32_C(4)
#define INKPOD_FILTER_GAUSSIAN_BLUR UINT32_C(5)
#define INKPOD_FILTER_INVERT UINT32_C(6)
#define INKPOD_FILTER_AUTO_CONTRAST UINT32_C(7)
#define INKPOD_FILTER_BRIGHTNESS_CONTRAST UINT32_C(8)
#define INKPOD_FILTER_TONE_CURVE UINT32_C(9)
#define INKPOD_FILTER_LEVELS UINT32_C(10)
#define INKPOD_FILTER_HSV UINT32_C(11)
#define INKPOD_FILTER_COLOR_BALANCE UINT32_C(12)
#define INKPOD_FILTER_UNSHARP_MASK UINT32_C(13)

typedef uint32_t InkpodFilterChannel;
#define INKPOD_FILTER_CHANNEL_RGB UINT32_C(1)
#define INKPOD_FILTER_CHANNEL_RED UINT32_C(2)
#define INKPOD_FILTER_CHANNEL_GREEN UINT32_C(3)
#define INKPOD_FILTER_CHANNEL_BLUE UINT32_C(4)

typedef uint32_t InkpodCurveInterpolation;
#define INKPOD_CURVE_BEZIER UINT32_C(1)
#define INKPOD_CURVE_BSPLINE UINT32_C(2)

typedef uint32_t InkpodGradientKind;
#define INKPOD_GRADIENT_LINEAR UINT32_C(1)
#define INKPOD_GRADIENT_RADIAL UINT32_C(2)

typedef uint32_t InkpodGradientMode;
#define INKPOD_GRADIENT_COMPOSITE UINT32_C(1)
#define INKPOD_GRADIENT_OVERWRITE UINT32_C(2)
#define INKPOD_GRADIENT_FLAG_CONSTRAIN_45 (UINT64_C(1) << 0)

#define INKPOD_EFFECT_FLAG_PRESSURE_SIZE (UINT64_C(1) << 0)
#define INKPOD_EFFECT_FLAG_PRESSURE_OPACITY (UINT64_C(1) << 1)

typedef uint32_t InkpodStampShape;
#define INKPOD_STAMP_ROUND UINT32_C(1)
#define INKPOD_STAMP_SQUARE UINT32_C(2)

typedef uint32_t InkpodDustMode;
#define INKPOD_DUST_REMOVE_FOREGROUND UINT32_C(1)
#define INKPOD_DUST_FILL_TRANSPARENT_HOLES UINT32_C(2)
#define INKPOD_DUST_REPLACE_COLOR_OUTLIERS UINT32_C(3)

typedef uint32_t InkpodM6TaskState;
#define INKPOD_M6_TASK_READY UINT32_C(0)
#define INKPOD_M6_TASK_RUNNING UINT32_C(1)
#define INKPOD_M6_TASK_COMPLETED UINT32_C(2)
#define INKPOD_M6_TASK_CANCELLED UINT32_C(3)
#define INKPOD_M6_TASK_FAILED UINT32_C(4)

#define INKPOD_BATCH_GRAPH_VERSION UINT32_C(1)
typedef uint32_t InkpodBatchInputKind;
#define INKPOD_BATCH_INPUT_FILE UINT32_C(1)
#define INKPOD_BATCH_INPUT_FOLDER UINT32_C(2)
#define INKPOD_BATCH_INPUT_CURRENT_SEQUENCE UINT32_C(3)
typedef uint32_t InkpodBatchOutputPolicy;
#define INKPOD_BATCH_OUTPUT_DUPLICATE UINT32_C(1)
#define INKPOD_BATCH_OUTPUT_NEW_SAVE UINT32_C(2)
#define INKPOD_BATCH_OUTPUT_EXPLICIT_OVERWRITE UINT32_C(3)
typedef uint32_t InkpodBatchFailurePolicy;
#define INKPOD_BATCH_FAILURE_CONTINUE UINT32_C(1)
#define INKPOD_BATCH_FAILURE_STOP UINT32_C(2)
typedef uint32_t InkpodBatchMissingPolicy;
#define INKPOD_BATCH_MISSING_SKIP UINT32_C(1)
#define INKPOD_BATCH_MISSING_ERROR UINT32_C(2)
typedef uint32_t InkpodBatchOperationKind;
#define INKPOD_BATCH_OPERATION_COLOR_REPLACE UINT32_C(1)
#define INKPOD_BATCH_OPERATION_CONTINUOUS_FILL UINT32_C(2)
#define INKPOD_BATCH_OPERATION_SEPARATION UINT32_C(3)
#define INKPOD_BATCH_OPERATION_VISIBILITY UINT32_C(4)
#define INKPOD_BATCH_OPERATION_LINE_WIDTH UINT32_C(5)
#define INKPOD_BATCH_OPERATION_FILTER UINT32_C(6)
#define INKPOD_BATCH_OPERATION_BOUNDARY_AIRBRUSH UINT32_C(7)
#define INKPOD_BATCH_OPERATION_DUST_REMOVAL UINT32_C(8)
#define INKPOD_BATCH_OPERATION_MIRROR UINT32_C(9)
#define INKPOD_BATCH_OPERATION_ROTATE_90 UINT32_C(10)
#define INKPOD_BATCH_OPERATION_RESIZE UINT32_C(11)
#define INKPOD_BATCH_OPERATION_CONVERT_PLANE UINT32_C(12)
#define INKPOD_BATCH_OPERATION_ENABLED UINT64_C(1)
#define INKPOD_BATCH_OPERATION_CONFIGURE_EACH_RUN (UINT64_C(1) << 1)
#define INKPOD_BATCH_OUTPUT_CELL_FOLDER UINT64_C(1)
#define INKPOD_BATCH_OUTPUT_DESCENDING (UINT64_C(1) << 1)
#define INKPOD_BATCH_OUTPUT_PREVIEW_BEFORE_SAVE (UINT64_C(1) << 2)
#define INKPOD_BATCH_SEPARATION_INVERT INT64_C(1)
#define INKPOD_BATCH_SEED_HAS_EXPECTED_COLOR UINT32_C(1)
typedef uint32_t InkpodBatchRunScope;
#define INKPOD_BATCH_SCOPE_CURRENT UINT32_C(1)
#define INKPOD_BATCH_SCOPE_ALL UINT32_C(2)
#define INKPOD_BATCH_RUN_DRY UINT64_C(1)
#define INKPOD_BATCH_RUN_PREVIEW_CONFIRMED (UINT64_C(1) << 1)
typedef uint32_t InkpodBatchItemOutcome;
#define INKPOD_BATCH_ITEM_SUCCEEDED UINT32_C(1)
#define INKPOD_BATCH_ITEM_SKIPPED UINT32_C(2)
#define INKPOD_BATCH_ITEM_FAILED UINT32_C(3)
#define INKPOD_BATCH_ITEM_CANCELLED UINT32_C(4)
#define INKPOD_BATCH_ITEM_DRY_RUN UINT32_C(5)
#define INKPOD_BATCH_PREVIEW_HAS_WARNING UINT32_C(1)

typedef uint32_t InkpodStoragePixelFormat;
#define INKPOD_STORAGE_BINARY8 UINT32_C(1)
#define INKPOD_STORAGE_GRAYSCALE8 UINT32_C(2)
#define INKPOD_STORAGE_GRAYSCALE16 UINT32_C(3)
#define INKPOD_STORAGE_RGBA8 UINT32_C(4)
#define INKPOD_STORAGE_RGBA16 UINT32_C(5)

typedef uint32_t InkpodLightTableDisplayMode;
#define INKPOD_LIGHT_TABLE_COLOR UINT32_C(1)
#define INKPOD_LIGHT_TABLE_MONOTONE UINT32_C(2)
#define INKPOD_LIGHT_TABLE_HALFTONE UINT32_C(3)
#define INKPOD_LIGHT_TABLE_ITEM_VISIBLE (UINT32_C(1) << 0)
#define INKPOD_LIGHT_TABLE_SET_ACTIVE (UINT32_C(1) << 1)
#define INKPOD_LIGHT_TABLE_CREATE_SET UINT32_C(1)
#define INKPOD_LIGHT_TABLE_DUPLICATE_SET UINT32_C(2)
#define INKPOD_LIGHT_TABLE_DELETE_SET UINT32_C(3)
#define INKPOD_LIGHT_TABLE_RENAME_SET UINT32_C(4)
#define INKPOD_LIGHT_TABLE_REORDER_SET UINT32_C(5)
#define INKPOD_LIGHT_TABLE_SET_ACTIVE_OPERATION UINT32_C(6)
#define INKPOD_LIGHT_TABLE_REMOVE_ITEM UINT32_C(7)
#define INKPOD_LIGHT_TABLE_REORDER_ITEM UINT32_C(8)
#define INKPOD_LIGHT_TABLE_UPDATE_ITEM UINT32_C(9)

typedef uint32_t InkpodSequenceDirection;
#define INKPOD_SEQUENCE_PREVIOUS UINT32_C(1)
#define INKPOD_SEQUENCE_NEXT UINT32_C(2)
#define INKPOD_SEQUENCE_FLAG_LOOP (UINT32_C(1) << 0)
#define INKPOD_MOTION_FLAG_LOOP (UINT64_C(1) << 0)
#define INKPOD_MOTION_FLAG_INCLUDE_SELECTION (UINT64_C(1) << 1)
#define INKPOD_MOTION_FLAG_INCLUDE_LIGHT_TABLE (UINT64_C(1) << 2)
#define INKPOD_MOTION_FRAME_PAUSED (UINT32_C(1) << 0)
#define INKPOD_MOTION_FRAME_INCLUDE_SELECTION (UINT32_C(1) << 1)
#define INKPOD_MOTION_FRAME_INCLUDE_LIGHT_TABLE (UINT32_C(1) << 2)

typedef uint32_t InkpodTreeOperation;
#define INKPOD_TREE_CREATE_LAYER UINT32_C(1)
#define INKPOD_TREE_DUPLICATE_LAYER UINT32_C(2)
#define INKPOD_TREE_DELETE_LAYER UINT32_C(3)
#define INKPOD_TREE_REORDER_LAYER UINT32_C(4)
#define INKPOD_TREE_SET_LAYER_PROPERTIES UINT32_C(5)
#define INKPOD_TREE_CREATE_PLANE UINT32_C(6)
#define INKPOD_TREE_DUPLICATE_PLANE UINT32_C(7)
#define INKPOD_TREE_DELETE_PLANE UINT32_C(8)
#define INKPOD_TREE_REORDER_PLANE UINT32_C(9)
#define INKPOD_TREE_SET_PLANE_PROPERTIES UINT32_C(10)
#define INKPOD_TREE_CONVERT_LAYER UINT32_C(11)
#define INKPOD_TREE_MERGE_LAYER UINT32_C(12)
#define INKPOD_TREE_CONVERT_PLANE UINT32_C(13)
#define INKPOD_TREE_MERGE_PLANE UINT32_C(14)
#define INKPOD_NODE_VISIBLE (UINT64_C(1) << 0)
#define INKPOD_NODE_EDITABLE (UINT64_C(1) << 1)

typedef uint32_t InkpodSelectionShape;
#define INKPOD_SELECTION_RECTANGLE UINT32_C(1)
#define INKPOD_SELECTION_ELLIPSE UINT32_C(2)
#define INKPOD_SELECTION_LASSO UINT32_C(3)
#define INKPOD_SELECTION_POLYLINE UINT32_C(4)
#define INKPOD_SELECTION_TRACE UINT32_C(5)
#define INKPOD_SELECTION_WAND UINT32_C(6)
typedef uint32_t InkpodSelectionOperation;
#define INKPOD_SELECTION_NEW UINT32_C(1)
#define INKPOD_SELECTION_ADD UINT32_C(2)
#define INKPOD_SELECTION_SUBTRACT UINT32_C(3)
#define INKPOD_SELECTION_INTERSECT UINT32_C(4)
#define INKPOD_SELECTION_ADJUST_INVERT UINT32_C(1)
#define INKPOD_SELECTION_ADJUST_EXPAND UINT32_C(2)
#define INKPOD_SELECTION_ADJUST_SHRINK UINT32_C(3)
#define INKPOD_SELECTION_LAYER_REPLACE UINT32_C(1)
#define INKPOD_SELECTION_LAYER_ADD UINT32_C(2)
#define INKPOD_SELECTION_LAYER_SUBTRACT UINT32_C(3)

#define INKPOD_GUIDE_HORIZONTAL UINT32_C(1)
#define INKPOD_GUIDE_VERTICAL UINT32_C(2)
#define INKPOD_MIRROR_HORIZONTAL UINT32_C(1)
#define INKPOD_MIRROR_VERTICAL UINT32_C(2)
#define INKPOD_ROTATE_LEFT_90 UINT32_C(1)
#define INKPOD_ROTATE_RIGHT_90 UINT32_C(2)
#define INKPOD_RESIZE_ANCHOR_TOP_LEFT UINT32_C(1)
#define INKPOD_RESIZE_ANCHOR_TOP_RIGHT UINT32_C(2)
#define INKPOD_RESIZE_ANCHOR_CENTER UINT32_C(3)
#define INKPOD_RESIZE_ANCHOR_BOTTOM_LEFT UINT32_C(4)
#define INKPOD_RESIZE_ANCHOR_BOTTOM_RIGHT UINT32_C(5)
#define INKPOD_DOCUMENT_RESIZE_RESAMPLE (UINT64_C(1) << 0)
#define INKPOD_PASTE_COMPATIBLE UINT32_C(1)
#define INKPOD_PASTE_ACTIVE_CONVERTED UINT32_C(2)

typedef struct InkpodCore InkpodCore;
typedef struct InkpodSnapshot InkpodSnapshot;
typedef struct InkpodClipboard InkpodClipboard;
typedef struct InkpodByteBuffer InkpodByteBuffer;
typedef struct InkpodEncodedSequence InkpodEncodedSequence;
typedef struct InkpodM6Task InkpodM6Task;
typedef InkpodM6Task InkpodBatchTask;
typedef struct InkpodBatchGraph InkpodBatchGraph;
typedef struct InkpodBatchPreview InkpodBatchPreview;
typedef struct InkpodBatchReport InkpodBatchReport;

typedef struct InkpodCoreConfig {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t feature_flags;
} InkpodCoreConfig;

typedef struct InkpodCommand {
    uint32_t struct_size;
    InkpodCommandKind kind;
    uint64_t flags;
} InkpodCommand;

typedef struct InkpodCommandBatch {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    const InkpodCommand* commands;
    uint64_t command_count;
    uint64_t command_stride_bytes;
} InkpodCommandBatch;

typedef struct InkpodDispatchResult {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t revision;
    uint64_t accepted_command_count;
} InkpodDispatchResult;

typedef struct InkpodCellCreateOptions {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
} InkpodCellCreateOptions;

typedef struct InkpodFrameRect {
    int32_t x;
    int32_t y;
    int32_t width;
    int32_t height;
} InkpodFrameRect;

typedef struct InkpodDocumentInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t document_revision;
    uint64_t view_revision;
    uint64_t document_id;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint64_t layer_id;
    uint64_t main_plane_id;
    uint64_t color_plane_id;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
    InkpodFrameRect hundred_frame;
    InkpodFrameRect reference_frame;
    InkpodFrameRect drawing_frame;
    InkpodFrameRect safe_frame;
    uint32_t margin_left;
    uint32_t margin_top;
    uint32_t margin_right;
    uint32_t margin_bottom;
    InkpodPlaneKind active_plane;
    uint32_t reserved;
    uint64_t main_plane_checksum;
    uint64_t color_plane_checksum;
} InkpodDocumentInfo;

typedef struct InkpodPaperFramesInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    InkpodFrameRect hundred_frame;
    InkpodFrameRect reference_frame;
    InkpodFrameRect drawing_frame;
    InkpodFrameRect safe_frame;
    uint32_t margin_left;
    uint32_t margin_top;
    uint32_t margin_right;
    uint32_t margin_bottom;
} InkpodPaperFramesInput;

typedef struct InkpodHistoryInfo {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t cursor;
    uint64_t item_count;
} InkpodHistoryInfo;

typedef struct InkpodHistoryItem {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t index;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodHistoryItem;

typedef struct InkpodStrokeSample {
    uint32_t struct_size;
    uint32_t flags;
    float x;
    float y;
    float pressure;
    uint32_t reserved;
} InkpodStrokeSample;

typedef struct InkpodStrokeInput {
    uint32_t struct_size;
    InkpodPaintTool tool;
    InkpodPlaneKind plane;
    InkpodCoordinateSpace coordinate_space;
    uint64_t flags;
    uint32_t color_rgba; /* 0xRRGGBBAA, straight-alpha sRGB */
    float diameter;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodStrokeInput;

typedef struct InkpodStrokeSampleSpan {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodStrokeSampleSpan;

typedef struct InkpodViewInput {
    uint32_t struct_size;
    InkpodViewCommandKind kind;
    uint64_t flags;
    double value1;
    double value2;
    double value3;
    double value4;
} InkpodViewInput;

typedef struct InkpodColorValue {
    uint32_t struct_size;
    InkpodColorDepth depth;
    uint16_t red;
    uint16_t green;
    uint16_t blue;
    uint16_t alpha;
} InkpodColorValue;

typedef struct InkpodColorArray {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    const InkpodColorValue* colors;
    uint64_t color_count;
    uint64_t color_stride_bytes;
} InkpodColorArray;

typedef struct InkpodColorBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    InkpodColorValue* colors;
    uint64_t color_capacity;
    uint64_t color_stride_bytes;
    uint64_t color_count;
} InkpodColorBuffer;

typedef struct InkpodFillInput {
    uint32_t struct_size;
    InkpodFillOperation operation;
    uint64_t flags;
    uint32_t seed_x;
    uint32_t seed_y;
    InkpodColorValue color;
    uint16_t tolerance; /* normalized 16-bit maximum per-channel difference */
    uint16_t gap_close;
    InkpodInclusionMode inclusion_mode;
    InkpodFrameRect selection;
    const InkpodColorValue* inclusion_colors;
    uint64_t inclusion_color_count;
    uint64_t inclusion_color_stride_bytes; /* may be zero when count is zero */
    uint32_t extension_distance;
    uint32_t reserved;
} InkpodFillInput;

typedef struct InkpodFillResult {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t revision;
    uint64_t changed_pixel_count;
    uint32_t leak_x;
    uint32_t leak_y;
} InkpodFillResult;

typedef struct InkpodSnapshotOptions {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
} InkpodSnapshotOptions;

typedef struct InkpodSnapshotTile {
    uint32_t struct_size;
    InkpodPixelFormat pixel_format;
    uint64_t tile_id;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint32_t reserved;
    const uint8_t* pixels;
    uint64_t pixel_bytes;
    uint64_t tile_revision;
} InkpodSnapshotTile;

typedef struct InkpodSnapshotView {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t feature_flags;
    uint64_t revision;
    const InkpodSnapshotTile* tiles;
    uint64_t tile_count;
    uint64_t tile_stride_bytes;
} InkpodSnapshotView;

typedef struct InkpodSnapshotTransform {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t view_revision;
    double zoom;
    double pan_x;
    double pan_y;
    uint32_t document_width;
    uint32_t document_height;
} InkpodSnapshotTransform;

typedef struct InkpodSnapshotGuide {
    uint32_t struct_size;
    uint32_t axis;
    int32_t position;
    uint32_t reserved;
    uint64_t id;
} InkpodSnapshotGuide;

typedef struct InkpodSnapshotOverlay {
    uint32_t struct_size;
    uint32_t flags;
    int32_t grid_origin_x;
    int32_t grid_origin_y;
    uint32_t grid_spacing_x;
    uint32_t grid_spacing_y;
    uint32_t grid_subdivisions;
    uint32_t reserved;
    const InkpodSnapshotGuide* guides;
    uint64_t guide_count;
    uint64_t guide_stride_bytes;
} InkpodSnapshotOverlay;

typedef struct InkpodVectorPoint {
    float x;
    float y;
} InkpodVectorPoint;

typedef struct InkpodVectorCubicSegment {
    uint32_t struct_size;
    uint32_t reserved;
    InkpodVectorPoint p0;
    InkpodVectorPoint p1;
    InkpodVectorPoint p2;
    InkpodVectorPoint p3;
    float width_start;
    float width_end;
} InkpodVectorCubicSegment;

typedef struct InkpodVectorPathInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t flags;
    uint64_t plane_id;
    InkpodColorValue color;
    const InkpodVectorCubicSegment* segments;
    uint64_t segment_count;
    uint64_t segment_stride_bytes;
} InkpodVectorPathInput;

typedef struct InkpodVectorFillInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    InkpodColorValue color;
    const uint64_t* boundary_path_ids;
    uint64_t boundary_path_count;
} InkpodVectorFillInput;

typedef struct InkpodVectorEraseInput {
    uint32_t struct_size;
    InkpodVectorEraseMode mode;
    uint64_t plane_id;
    float x;
    float y;
    float radius;
    uint32_t reserved;
} InkpodVectorEraseInput;

typedef struct InkpodVectorWidthInput {
    uint32_t struct_size;
    InkpodVectorWidthMode mode;
    uint64_t feature_flags;
    const uint64_t* path_ids;
    uint64_t path_count;
    float parameter;
    uint32_t reserved;
} InkpodVectorWidthInput;

typedef struct InkpodVectorSelectionInput {
    uint32_t struct_size;
    InkpodVectorSelectionMode mode;
    uint64_t feature_flags;
    InkpodFrameRect bounds;
} InkpodVectorSelectionInput;

typedef struct InkpodVectorSelectionRange {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t path_id;
    uint32_t start_million;
    uint32_t end_million;
} InkpodVectorSelectionRange;

typedef struct InkpodVectorSelectionBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    InkpodVectorSelectionRange* ranges;
    uint64_t range_capacity;
    uint64_t range_count;
    uint64_t* fill_ids;
    uint64_t fill_capacity;
    uint64_t fill_count;
} InkpodVectorSelectionBuffer;

typedef struct InkpodVectorRasterizeInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t layer_id;
    uint32_t scale;
    uint32_t reserved_2;
} InkpodVectorRasterizeInput;

typedef struct InkpodVectorRasterBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    uint8_t* pixels;
    uint64_t pixel_capacity;
    uint64_t required_bytes;
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint32_t reserved_2;
} InkpodVectorRasterBuffer;

typedef struct InkpodRasterVectorizeInput {
    uint32_t struct_size;
    uint32_t alpha_threshold;
    uint64_t feature_flags;
    uint64_t source_plane_id;
    uint64_t target_layer_id;
} InkpodRasterVectorizeInput;

typedef struct InkpodCurvePoint {
    uint32_t struct_size;
    uint32_t reserved;
    uint32_t input;
    uint32_t output;
} InkpodCurvePoint;

/* Filter parameters use normalized milli-units. Gaussian uses radius/strength
 * in parameter_0/1; brightness/contrast use parameter_0/1; levels use input
 * shadow/gamma/highlight and output shadow/highlight in parameter_0..4. */
typedef struct InkpodFilterInput {
    uint32_t struct_size;
    InkpodFilterKind kind;
    uint64_t feature_flags;
    uint64_t plane_id;
    InkpodFilterChannel channel;
    InkpodCurveInterpolation interpolation;
    int32_t parameter_0;
    int32_t parameter_1;
    int32_t parameter_2;
    int32_t parameter_3;
    int32_t parameter_4;
    /* Record stride for points. Zero is accepted as packed-v1 compatibility
     * when point_count is nonzero; new callers should pass sizeof(*points).
     * `reserved` retains source compatibility with the original v1 spelling. */
    union {
        uint32_t point_stride_bytes;
        uint32_t reserved;
    };
    const InkpodCurvePoint* points;
    uint64_t point_count;
} InkpodFilterInput;

typedef struct InkpodFilterPreviewInfo {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t plane_id;
    uint64_t base_checksum;
    uint64_t preview_checksum;
    uint64_t preview_revision;
} InkpodFilterPreviewInfo;

typedef struct InkpodGradientStop {
    uint32_t struct_size;
    uint32_t reserved;
    uint32_t position_milli;
    uint32_t reserved_2;
    InkpodColorValue color;
} InkpodGradientStop;

typedef struct InkpodGradientInput {
    uint32_t struct_size;
    InkpodGradientKind kind;
    uint64_t feature_flags;
    uint64_t plane_id;
    InkpodGradientMode mode;
    uint32_t dither;
    int64_t start_x_milli;
    int64_t start_y_milli;
    int64_t end_x_milli;
    int64_t end_y_milli;
    const InkpodGradientStop* stops;
    uint64_t stop_count;
    uint64_t stop_stride_bytes;
} InkpodGradientInput;

typedef struct InkpodAirbrushInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    int64_t center_x_milli;
    int64_t center_y_milli;
    uint32_t radius_milli;
    uint32_t hardness_milli;
    uint32_t opacity_milli;
    uint32_t reserved_2;
    InkpodColorValue color;
} InkpodAirbrushInput;

typedef struct InkpodBoundaryAirbrushInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint32_t width;
    uint32_t strength_milli;
    InkpodColorArray colors;
} InkpodBoundaryAirbrushInput;

typedef struct InkpodBlurEffectInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint32_t radius;
    uint32_t strength_milli;
    uint32_t reserved_2;
    uint32_t reserved_3;
} InkpodBlurEffectInput;

typedef struct InkpodStampInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    uint64_t plane_id;
    int32_t source_x;
    int32_t source_y;
    int32_t destination_x;
    int32_t destination_y;
    uint32_t width;
    uint32_t height;
    uint32_t opacity_milli;
    uint32_t reserved_2;
} InkpodStampInput;

/* Alpha pixels are borrowed only for the call. Rows may be padded and must
 * use GRAYSCALE8 or GRAYSCALE16 storage matching the target dimensions. */
typedef struct InkpodAlphaEditInput {
    uint32_t struct_size;
    InkpodStoragePixelFormat pixel_format;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint32_t width;
    uint32_t height;
    uint32_t reserved;
    uint32_t reserved_2;
    const uint8_t* pixels;
    uint64_t pixel_bytes;
    uint64_t row_stride_bytes;
} InkpodAlphaEditInput;

/* Gesture spans are borrowed and copied during the call. Coordinates may be
 * document logical units or client device pixels. view_id zero selects the
 * primary view. Every completed gesture commits at most one Undo unit. */
typedef struct InkpodAirbrushGestureInput {
    uint32_t struct_size;
    InkpodCoordinateSpace coordinate_space;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    uint32_t radius_milli;
    uint32_t hardness_milli;
    uint32_t spacing_milli;
    uint32_t opacity_milli;
    uint32_t fade_milli;
    uint32_t continuous_dabs;
    InkpodColorValue color;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodAirbrushGestureInput;

typedef struct InkpodStampGestureInput {
    uint32_t struct_size;
    InkpodCoordinateSpace coordinate_space;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    InkpodStrokeSample source;
    uint32_t radius_milli;
    uint32_t hardness_milli;
    uint32_t spacing_milli;
    uint32_t opacity_milli;
    InkpodStampShape shape;
    uint32_t reserved;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodStampGestureInput;

typedef struct InkpodM6TaskInfo {
    uint32_t struct_size;
    InkpodM6TaskState state;
    uint64_t completed_work;
    uint64_t total_work;
    uint64_t reserved;
} InkpodM6TaskInfo;

/* M7 graph records are copied into a Rust-owned immutable graph handle.
 * Every nested caller span is borrowed only for inkpod_batch_graph_create. */
typedef struct InkpodBatchInput {
    uint32_t struct_size;
    InkpodBatchInputKind kind;
    uint64_t feature_flags;
    const uint8_t* path_utf8;
    uint64_t path_bytes;
    uint32_t first_cell;
    uint32_t last_cell;
    uint64_t reserved;
} InkpodBatchInput;

typedef struct InkpodBatchColorPairInput {
    uint32_t struct_size;
    uint32_t enabled;
    uint64_t reserved;
    InkpodColorValue old_color;
    InkpodColorValue new_color;
} InkpodBatchColorPairInput;

typedef struct InkpodBatchSeedInput {
    uint32_t struct_size;
    uint32_t flags;
    uint32_t x;
    uint32_t y;
    uint32_t tolerance;
    uint32_t gap_close;
    uint64_t reserved;
    InkpodColorValue fill_color;
    InkpodColorValue expected_color;
} InkpodBatchSeedInput;

/* Operation parameters are kind-specific and versioned:
 * visibility [0]=0/1; line width [0]=mode, [1]=value*1000;
 * separation [0]=INKPOD_BATCH_SEPARATION_INVERT or 0;
 * boundary effect [0]=width, [1]=strength_milli;
 * dust [0]=InkpodDustMode, [1]=maximum_pixels;
 * mirror/rotate [0]=existing axis/direction constants;
 * resize [0..5]=width,height,dpi_x,dpi_y,resample,anchor;
 * convert [0..1]=InkpodTypedPlaneKind,InkpodStoragePixelFormat. */
typedef struct InkpodBatchOperationInput {
    uint32_t struct_size;
    uint32_t version;
    InkpodBatchOperationKind kind;
    uint32_t reserved;
    uint64_t flags;
    uint64_t layer_id;
    uint64_t plane_id;
    InkpodLayerKind layer_kind;
    InkpodTypedPlaneKind plane_kind;
    InkpodBatchMissingPolicy missing_policy;
    uint32_t reserved_2;
    int64_t parameters[8];
    InkpodColorValue color_0;
    InkpodColorValue color_1;
    InkpodColorArray colors;
    const InkpodFilterInput* filter;
    const InkpodBatchColorPairInput* color_pairs;
    uint64_t color_pair_count;
    uint64_t color_pair_stride_bytes;
    const InkpodBatchSeedInput* seeds;
    uint64_t seed_count;
    uint64_t seed_stride_bytes;
    uint64_t reserved_3;
} InkpodBatchOperationInput;

typedef struct InkpodBatchGraphInput {
    uint32_t struct_size;
    uint32_t version;
    uint64_t feature_flags;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    const InkpodBatchInput* inputs;
    uint64_t input_count;
    uint64_t input_stride_bytes;
    const InkpodBatchOperationInput* operations;
    uint64_t operation_count;
    uint64_t operation_stride_bytes;
    InkpodBatchOutputPolicy output_policy;
    InkpodBatchFailurePolicy failure_policy;
    uint64_t output_flags;
    const uint8_t* output_folder_utf8;
    uint64_t output_folder_bytes;
    const uint8_t* basename_utf8;
    uint64_t basename_bytes;
    uint32_t start_number;
    uint32_t wait_milliseconds;
    uint64_t reserved;
} InkpodBatchGraphInput;

typedef struct InkpodBatchGraphInfo {
    uint32_t struct_size;
    uint32_t version;
    uint64_t input_count;
    uint64_t operation_count;
    InkpodBatchOutputPolicy output_policy;
    InkpodBatchFailurePolicy failure_policy;
    uint64_t output_flags;
} InkpodBatchGraphInfo;

/* Returned UTF-8 spans borrow the owning preview/report handle and remain
 * valid until that handle is released. */
typedef struct InkpodBatchPreviewItem {
    uint32_t struct_size;
    uint32_t flags;
    const uint8_t* input_name;
    uint64_t input_name_bytes;
    const uint8_t* output_path;
    uint64_t output_path_bytes;
    const uint8_t* warning;
    uint64_t warning_bytes;
} InkpodBatchPreviewItem;

typedef struct InkpodBatchReportInfo {
    uint32_t struct_size;
    uint32_t cancelled;
    uint64_t item_count;
    uint64_t failure_count;
    uint64_t reserved;
} InkpodBatchReportInfo;

typedef struct InkpodBatchReportItem {
    uint32_t struct_size;
    InkpodBatchItemOutcome outcome;
    const uint8_t* input_name;
    uint64_t input_name_bytes;
    const uint8_t* output_path;
    uint64_t output_path_bytes;
    const uint8_t* message;
    uint64_t message_bytes;
} InkpodBatchReportItem;

typedef struct InkpodSnapshotVectorSegment {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t path_id;
    uint64_t plane_id;
    uint32_t z_order;
    uint32_t segment_index;
    uint32_t segment_count;
    uint32_t color_rgba;
    InkpodVectorPoint p0;
    InkpodVectorPoint p1;
    InkpodVectorPoint p2;
    InkpodVectorPoint p3;
    float width_start;
    float width_end;
} InkpodSnapshotVectorSegment;

typedef struct InkpodSnapshotVectorFill {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t fill_id;
    uint64_t plane_id;
    uint32_t z_order;
    uint32_t color_rgba;
    uint64_t first_boundary_path;
    uint64_t boundary_path_count;
} InkpodSnapshotVectorFill;

typedef struct InkpodSnapshotVectorView {
    uint32_t struct_size;
    uint32_t abi_version;
    uint64_t feature_flags;
    const InkpodSnapshotVectorSegment* segments;
    uint64_t segment_count;
    uint64_t segment_stride_bytes;
    const InkpodSnapshotVectorFill* fills;
    uint64_t fill_count;
    uint64_t fill_stride_bytes;
    const uint64_t* boundary_path_ids;
    uint64_t boundary_path_count;
} InkpodSnapshotVectorView;

typedef struct InkpodTreeEdit {
    uint32_t struct_size;
    InkpodTreeOperation operation;
    uint64_t flags;
    uint64_t object_id;
    uint64_t parent_id;
    uint32_t destination_index;
    uint32_t kind;
    InkpodStoragePixelFormat pixel_format;
    uint32_t opacity_milli;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
} InkpodTreeEdit;

typedef struct InkpodNodeInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t id;
    uint64_t parent_id;
    uint32_t kind;
    InkpodStoragePixelFormat pixel_format;
    uint32_t opacity_milli;
    uint32_t index;
    uint32_t child_count;
    uint32_t reserved;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodNodeInfo;

typedef struct InkpodSelectionPoint {
    uint32_t struct_size;
    uint32_t reserved;
    float x;
    float y;
} InkpodSelectionPoint;

typedef struct InkpodSelectionInput {
    uint32_t struct_size;
    InkpodSelectionShape shape;
    InkpodSelectionOperation operation;
    uint32_t reserved;
    InkpodFrameRect bounds;
    const InkpodSelectionPoint* points;
    uint64_t point_count;
    uint64_t point_stride_bytes;
    float diameter;
    uint16_t tolerance;
    uint16_t gap_close;
    uint32_t seed_x;
    uint32_t seed_y;
} InkpodSelectionInput;

typedef struct InkpodBlurToolInput {
    uint32_t struct_size;
    InkpodCoordinateSpace coordinate_space;
    /* PRESSURE_SIZE varies pen-region diameter; unsupported for other shapes. */
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    uint32_t radius;
    uint32_t strength_milli;
    InkpodSelectionShape shape;
    float diameter;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodBlurToolInput;

typedef struct InkpodDustInput {
    uint32_t struct_size;
    InkpodDustMode mode;
    uint64_t feature_flags;
    uint64_t plane_id;
    uint64_t view_id;
    InkpodCoordinateSpace coordinate_space;
    InkpodSelectionShape shape;
    uint32_t maximum_pixels;
    uint32_t use_region;
    float diameter;
    const InkpodStrokeSample* samples;
    uint64_t sample_count;
    uint64_t sample_stride_bytes;
} InkpodDustInput;

typedef struct InkpodFloatingTransform {
    uint32_t struct_size;
    uint32_t reserved;
    double translate_x;
    double translate_y;
    double scale_x;
    double scale_y;
    double rotation_degrees;
} InkpodFloatingTransform;

typedef struct InkpodDocumentResizeInput {
    uint32_t struct_size;
    uint32_t anchor;
    uint64_t flags;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
} InkpodDocumentResizeInput;

typedef struct InkpodClipboardRasterBuffer {
    uint32_t struct_size;
    uint32_t reserved;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t width;
    uint32_t height;
    uint8_t* pixels_rgba8;
    uint64_t pixel_capacity;
    uint64_t required_bytes;
    uint64_t row_stride_bytes;
} InkpodClipboardRasterBuffer;

typedef struct InkpodClipboardRgbaInput {
    uint32_t struct_size;
    uint32_t reserved;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t width;
    uint32_t height;
    const uint8_t* pixels_rgba8;
    uint64_t pixel_bytes;
    uint64_t row_stride_bytes;
} InkpodClipboardRgbaInput;

typedef struct InkpodGridInput {
    uint32_t struct_size;
    uint32_t reserved;
    int32_t origin_x;
    int32_t origin_y;
    uint32_t spacing_x;
    uint32_t spacing_y;
    uint32_t subdivisions;
    uint32_t flags;
} InkpodGridInput;

typedef struct InkpodLocatorOutput {
    uint32_t struct_size;
    uint32_t flags;
    int32_t document_x;
    int32_t document_y;
    InkpodFrameRect selection;
    InkpodColorValue color;
} InkpodLocatorOutput;
#define INKPOD_LOCATOR_SELECTION_PRESENT (UINT32_C(1) << 0)
#define INKPOD_LOCATOR_COLOR_PRESENT (UINT32_C(1) << 1)

/* M4 raster bytes are borrowed for one call. Rows may be padded, but the
 * advertised byte range must contain every complete row. Only straight RGBA8
 * and RGBA16 storage formats are accepted. */
typedef struct InkpodM4RasterInput {
    uint32_t struct_size;
    InkpodStoragePixelFormat pixel_format;
    uint64_t flags;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint64_t source_revision;
    uint32_t width;
    uint32_t height;
    uint32_t dpi_x_milli;
    uint32_t dpi_y_milli;
    InkpodFrameRect reference_frame;
    const uint8_t* pixels;
    uint64_t pixel_bytes;
    uint64_t row_stride_bytes;
} InkpodM4RasterInput;

typedef struct InkpodLightTableItemInput {
    uint32_t struct_size;
    uint32_t flags;
    uint32_t opacity_milli;
    InkpodLightTableDisplayMode display_mode;
    InkpodColorValue display_color;
    int32_t translate_x_milli;
    int32_t translate_y_milli;
    uint32_t scale_x_milli;
    uint32_t scale_y_milli;
    int32_t rotation_milli_degrees;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    InkpodM4RasterInput source;
} InkpodLightTableItemInput;

typedef struct InkpodLightTableEdit {
    uint32_t struct_size;
    uint32_t operation;
    uint64_t object_id;
    uint32_t destination_index;
    uint32_t flags;
    uint32_t opacity_milli;
    InkpodLightTableDisplayMode display_mode;
    InkpodColorValue display_color;
    int32_t translate_x_milli;
    int32_t translate_y_milli;
    uint32_t scale_x_milli;
    uint32_t scale_y_milli;
    int32_t rotation_milli_degrees;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
} InkpodLightTableEdit;

typedef struct InkpodLightTableSetInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t id;
    uint32_t opacity_milli;
    uint32_t item_count;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodLightTableSetInfo;

typedef struct InkpodLightTableItemInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t id;
    uint64_t source_plane_id;
    uint64_t source_document_uuid_high;
    uint64_t source_document_uuid_low;
    uint64_t source_revision;
    uint32_t opacity_milli;
    uint32_t effective_opacity_milli;
    InkpodLightTableDisplayMode display_mode;
    InkpodColorValue display_color;
    int32_t translate_x_milli;
    int32_t translate_y_milli;
    uint32_t scale_x_milli;
    uint32_t scale_y_milli;
    int32_t rotation_milli_degrees;
    uint32_t reserved;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodLightTableItemInfo;

typedef struct InkpodSequenceCellInput {
    uint32_t struct_size;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    InkpodM4RasterInput source;
} InkpodSequenceCellInput;

typedef struct InkpodSequenceInput {
    uint32_t struct_size;
    uint32_t reserved;
    uint64_t feature_flags;
    const InkpodSequenceCellInput* cells;
    uint64_t cell_count;
    uint64_t cell_stride_bytes;
} InkpodSequenceInput;

typedef struct InkpodNamedBytesInput {
    uint32_t struct_size;
    uint32_t reserved;
    const uint8_t* name_utf8;
    uint64_t name_bytes;
    const uint8_t* bytes;
    uint64_t byte_count;
} InkpodNamedBytesInput;

typedef struct InkpodSequenceCellInfo {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t sequence_index;
    uint64_t document_uuid_high;
    uint64_t document_uuid_low;
    uint32_t cell_number;
    uint32_t width;
    uint32_t height;
    uint32_t thumbnail_width;
    uint32_t thumbnail_height;
    uint32_t reserved;
    uint64_t thumbnail_checksum;
    uint8_t* name_utf8;
    uint64_t name_capacity;
    uint64_t name_bytes;
} InkpodSequenceCellInfo;

typedef struct InkpodMotionCheckInput {
    uint32_t struct_size;
    uint32_t fps;
    uint64_t flags;
} InkpodMotionCheckInput;

typedef struct InkpodMotionFrame {
    uint32_t struct_size;
    uint32_t flags;
    uint64_t sequence_index;
    uint32_t cell_number;
    uint32_t thumbnail_width;
    uint32_t thumbnail_height;
    uint32_t reserved;
    uint64_t thumbnail_checksum;
} InkpodMotionFrame;

uint32_t inkpod_abi_version(void);

/* On success, Rust allocates *out_core and the calling thread becomes its
 * single-writer owner. config and out_core must not overlap. */
InkpodStatus inkpod_core_create(
    const InkpodCoreConfig* config,
    InkpodCore** out_core);

/* Must run on the creating thread. *core == NULL is a successful no-op. The
 * function releases Rust ownership and sets *core to NULL. */
InkpodStatus inkpod_core_destroy(InkpodCore** core);

/* Core mutation is single-writer and must run on the creating thread. Input,
 * output, and Core storage must not overlap. command_stride_bytes is required
 * even for an empty batch and must be at least sizeof(InkpodCommand). */
InkpodStatus inkpod_core_dispatch_batch(
    InkpodCore* core,
    const InkpodCommandBatch* batch,
    InkpodDispatchResult* result);

/* Creates a sparse two-plane coloring CellDocument. IDs are stable/nonzero. */
InkpodStatus inkpod_core_new_cell(
    InkpodCore* core,
    const InkpodCellCreateOptions* options,
    InkpodDocumentInfo* out_info);

InkpodStatus inkpod_core_get_document_info(
    InkpodCore* core,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_update_paper_frames(
    InkpodCore* core,
    const InkpodPaperFramesInput* input,
    InkpodDispatchResult* result);

/* Plane selection is logical UI state and does not change document revision. */
InkpodStatus inkpod_core_set_active_plane(
    InkpodCore* core,
    InkpodPlaneKind plane);
InkpodStatus inkpod_core_set_active_node(
    InkpodCore* core,
    uint64_t layer_id,
    uint64_t plane_id);

/* Fill planning is bounded and all-or-nothing. Overflow reports one candidate
 * in result and does not change pixels, revision, dirty, or history. M4's
 * LIGHT_TABLE_BOUNDARY/COLOR flags sample immutable reference snapshots; they
 * never make a light-table raster a writable fill destination. */
InkpodStatus inkpod_core_apply_fill(
    InkpodCore* core,
    const InkpodFillInput* input,
    InkpodFillResult* result);

InkpodStatus inkpod_core_eyedropper(
    InkpodCore* core,
    InkpodEyedropperSource source,
    uint32_t x,
    uint32_t y,
    InkpodColorValue* out_color);

/* Palette arrays are caller-owned and preserve each entry's explicit depth.
 * Set is one metadata/history transaction. A zero-capacity, null output buffer
 * is a successful count query; otherwise get writes complete color records. */
InkpodStatus inkpod_core_palette_set(
    InkpodCore* core,
    const InkpodColorArray* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_palette_get(
    InkpodCore* core,
    InkpodColorBuffer* buffer);
InkpodStatus inkpod_core_palette_generate(
    InkpodCore* core,
    uint32_t maximum_colors,
    uint32_t quantization_bits,
    InkpodDispatchResult* result);

/* Base color is meaningful for grayscale main-line coverage and is persisted
 * exactly. Set is rejected for a binary main-line plane. */
InkpodStatus inkpod_core_set_main_line_color(
    InkpodCore* core,
    const InkpodColorValue* color,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_get_main_line_color(
    InkpodCore* core,
    InkpodColorValue* out_color);

/* Temporary view state only; it never edits document pixels/history. */
InkpodStatus inkpod_core_set_color_check(
    InkpodCore* core,
    InkpodColorCheckMode mode);

/* One call contains every sample from pointer down through pointer up. Samples
 * are borrowed only for this call and iterated with sample_stride_bytes. */
InkpodStatus inkpod_core_apply_stroke(
    InkpodCore* core,
    const InkpodStrokeInput* input,
    InkpodDispatchResult* result);

/* Live stroke API. begin/append update only a Core-owned transient preview;
 * end commits exactly one document/history transaction and cancel discards the
 * preview. Sample storage is borrowed only for each call. Build an immutable
 * snapshot after begin/append to render the current preview. */
InkpodStatus inkpod_core_stroke_begin(
    InkpodCore* core,
    const InkpodStrokeInput* input);
InkpodStatus inkpod_core_stroke_append(
    InkpodCore* core,
    const InkpodStrokeSampleSpan* span);
InkpodStatus inkpod_core_stroke_end(
    InkpodCore* core,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_stroke_cancel(InkpodCore* core);

InkpodStatus inkpod_core_undo(
    InkpodCore* core,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_redo(
    InkpodCore* core,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_history_info(
    InkpodCore* core,
    InkpodHistoryInfo* out_info);
InkpodStatus inkpod_core_history_item(
    InkpodCore* core,
    uint64_t index,
    InkpodHistoryItem* out_item);
InkpodStatus inkpod_core_history_jump(
    InkpodCore* core,
    uint64_t target_cursor,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_revert_active_selection(
    InkpodCore* core,
    InkpodDispatchResult* result);

/* UTF-8 path bytes are borrowed only for the call. Save writes, flushes, and
 * closes a same-directory temporary file before replacement. */
InkpodStatus inkpod_core_save(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_open(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
/* Autosave does not advance the normal savepoint/path. Recovery opens as a
 * dirty, pathless document and cannot overwrite the former normal file unless
 * the caller explicitly supplies that path to a later normal save. */
InkpodStatus inkpod_core_autosave(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_open_recovery(
    InkpodCore* core,
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_revert(
    InkpodCore* core,
    InkpodDocumentInfo* out_info);

/* value layout: PAN(dx,dy), ZOOM(factor,anchor_x,anchor_y),
 * FIT/ONE_TO_ONE/VIEWPORT_RESIZED(viewport_w,viewport_h),
 * BOX_ZOOM(document_x,document_y,document_w,document_h), and SET_*(enabled).
 * Flip commands ignore values. Resize preserves manual view and recomputes
 * only persistent Fit/OneToOne modes. Values use client device px unless the
 * BOX_ZOOM fields are explicitly document coordinates. */
InkpodStatus inkpod_core_apply_view(
    InkpodCore* core,
    const InkpodViewInput* input,
    InkpodDocumentInfo* out_info);

/* M3 typed tree edits are transactional. Names and point spans are borrowed;
 * node-info buffers and all result storage remain caller-owned. */
InkpodStatus inkpod_core_tree_edit(
    InkpodCore* core,
    const InkpodTreeEdit* input,
    InkpodDispatchResult* result,
    uint64_t* out_object_id);
InkpodStatus inkpod_core_node_get(
    InkpodCore* core,
    uint32_t layer_index,
    /* UINT32_MAX queries the layer record itself. */
    uint32_t plane_index,
    InkpodNodeInfo* out_info);
InkpodStatus inkpod_core_apply_selection(
    InkpodCore* core,
    const InkpodSelectionInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_select_color(
    InkpodCore* core,
    const InkpodColorValue* color,
    uint16_t tolerance,
    uint32_t different,
    InkpodSelectionOperation operation,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_selection_adjust(
    InkpodCore* core,
    uint32_t operation,
    uint32_t pixels,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_selection_clear(
    InkpodCore* core,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_selection_to_layer(
    InkpodCore* core,
    const uint8_t* name_utf8,
    uint64_t name_bytes,
    InkpodDispatchResult* result,
    uint64_t* out_layer_id);
InkpodStatus inkpod_core_selection_from_layer(
    InkpodCore* core,
    uint64_t layer_id,
    uint32_t operation,
    InkpodDispatchResult* result);

/* Private typed clipboard ownership remains in Rust. Coordinate-preserving
 * paste stays floating until commit/cancel; release nulls the owner pointer.
 * out_clipboard storage must not contain a live handle when copy is called. */
InkpodStatus inkpod_core_clipboard_copy(
    InkpodCore* core,
    InkpodClipboard** out_clipboard);
InkpodStatus inkpod_clipboard_release(InkpodClipboard** clipboard);
InkpodStatus inkpod_core_paste_begin(
    InkpodCore* core,
    const InkpodClipboard* clipboard);
InkpodStatus inkpod_core_paste_begin_mode(
    InkpodCore* core,
    const InkpodClipboard* clipboard,
    uint32_t mode);
InkpodStatus inkpod_clipboard_render_rgba8(
    const InkpodClipboard* clipboard,
    InkpodClipboardRasterBuffer* output);
InkpodStatus inkpod_clipboard_create_rgba8(
    const InkpodClipboardRgbaInput* input,
    InkpodClipboard** out_clipboard);
InkpodStatus inkpod_core_floating_transform(
    InkpodCore* core,
    const InkpodFloatingTransform* input);
InkpodStatus inkpod_core_floating_commit(
    InkpodCore* core,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_floating_cancel(InkpodCore* core);
InkpodStatus inkpod_core_clear_selected_content(
    InkpodCore* core,
    InkpodDispatchResult* result);

InkpodStatus inkpod_core_mirror_document(
    InkpodCore* core,
    uint32_t axis,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_rotate_document(
    InkpodCore* core,
    uint32_t direction,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_resize_document(
    InkpodCore* core,
    const InkpodDocumentResizeInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_guide_add(
    InkpodCore* core,
    uint32_t axis,
    int32_t position,
    InkpodDispatchResult* result,
    uint64_t* out_guide_id);
InkpodStatus inkpod_core_guide_move(
    InkpodCore* core,
    uint64_t guide_id,
    int32_t position,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_guide_delete(
    InkpodCore* core,
    uint64_t guide_id,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_grid_set(
    InkpodCore* core,
    const InkpodGridInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_locator_sample(
    InkpodCore* core,
    uint64_t view_id,
    double device_x,
    double device_y,
    InkpodLocatorOutput* out_locator);
InkpodStatus inkpod_core_shortcut_rebind(
    InkpodCore* core,
    uint32_t command_id,
    uint32_t virtual_key,
    uint32_t modifiers);
InkpodStatus inkpod_core_shortcut_resolve(
    InkpodCore* core,
    uint32_t virtual_key,
    uint32_t modifiers,
    uint32_t* out_command_id);
InkpodStatus inkpod_core_shortcut_reset(InkpodCore* core);
InkpodStatus inkpod_core_view_create(
    InkpodCore* core,
    uint64_t* out_view_id);
InkpodStatus inkpod_core_view_apply(
    InkpodCore* core,
    uint64_t view_id,
    const InkpodViewInput* input);
InkpodStatus inkpod_core_view_close(
    InkpodCore* core,
    uint64_t view_id);

/* Common-raster bytes are decoded/encoded by Rust. Exported bytes remain
 * immutable and borrowed until the owning buffer is released. */
InkpodStatus inkpod_core_import_common_raster(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    const uint8_t* bytes,
    uint64_t byte_count,
    uint64_t document_uuid_high,
    uint64_t document_uuid_low,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_export_common_raster(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    uint32_t composite_white,
    InkpodByteBuffer** out_buffer);
InkpodStatus inkpod_byte_buffer_view(
    const InkpodByteBuffer* buffer,
    const uint8_t** out_bytes,
    uint64_t* out_byte_count);
InkpodStatus inkpod_byte_buffer_release(InkpodByteBuffer** buffer);

/* M4 production workflow. Light-table source pixels and sequence-cell spans
 * are copied before return. A dirty cell switch reports UNSAVED_CHANGES and
 * leaves the active document/revision untouched. */
InkpodStatus inkpod_core_light_table_add_item(
    InkpodCore* core,
    const InkpodLightTableItemInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_item_id);
InkpodStatus inkpod_core_light_table_edit(
    InkpodCore* core,
    const InkpodLightTableEdit* input,
    InkpodDispatchResult* result,
    uint64_t* out_object_id);
InkpodStatus inkpod_core_light_table_set_get(
    InkpodCore* core,
    uint32_t index,
    InkpodLightTableSetInfo* output);
InkpodStatus inkpod_core_light_table_item_get(
    InkpodCore* core,
    uint32_t index,
    InkpodLightTableItemInfo* output);
InkpodStatus inkpod_core_light_table_add_common_raster(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    const uint8_t* bytes,
    uint64_t byte_count,
    const uint8_t* name_utf8,
    uint64_t name_bytes,
    uint64_t document_uuid_high,
    uint64_t document_uuid_low,
    uint64_t source_revision,
    InkpodDispatchResult* result,
    uint64_t* out_item_id);
InkpodStatus inkpod_core_light_table_reload_common_raster(
    InkpodCore* core,
    uint64_t item_id,
    InkpodCommonRasterFormat format,
    const uint8_t* bytes,
    uint64_t byte_count,
    uint64_t document_uuid_high,
    uint64_t document_uuid_low,
    uint64_t source_revision,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_light_table_set_global_opacity(
    InkpodCore* core,
    uint32_t opacity_milli,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_light_table_sample(
    InkpodCore* core,
    uint32_t x,
    uint32_t y,
    InkpodColorValue* out_color);
InkpodStatus inkpod_core_light_table_swap(
    InkpodCore* core,
    uint64_t item_id,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_sequence_set(
    InkpodCore* core,
    const InkpodSequenceInput* input);
InkpodStatus inkpod_core_sequence_import_encoded(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    const InkpodNamedBytesInput* files,
    uint64_t file_count,
    uint64_t file_stride_bytes);
InkpodStatus inkpod_core_sequence_export_encoded(
    InkpodCore* core,
    InkpodCommonRasterFormat format,
    uint32_t composite_white,
    InkpodEncodedSequence** out_sequence);
InkpodStatus inkpod_encoded_sequence_count(
    const InkpodEncodedSequence* sequence,
    uint64_t* out_count);
InkpodStatus inkpod_encoded_sequence_get(
    const InkpodEncodedSequence* sequence,
    uint64_t index,
    const uint8_t** out_name,
    uint64_t* out_name_bytes,
    const uint8_t** out_bytes,
    uint64_t* out_byte_count);
InkpodStatus inkpod_encoded_sequence_release(InkpodEncodedSequence** sequence);
InkpodStatus inkpod_core_sequence_cell_get(
    InkpodCore* core,
    uint32_t index,
    InkpodSequenceCellInfo* output);
InkpodStatus inkpod_core_sequence_activate(
    InkpodCore* core,
    uint32_t index,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_sequence_step(
    InkpodCore* core,
    InkpodSequenceDirection direction,
    uint32_t flags,
    InkpodDocumentInfo* out_info);
InkpodStatus inkpod_core_motion_check_start(
    InkpodCore* core,
    const InkpodMotionCheckInput* input,
    InkpodMotionFrame* out_frame);
InkpodStatus inkpod_core_motion_check_step(
    InkpodCore* core,
    InkpodSequenceDirection direction,
    InkpodMotionFrame* out_frame);
InkpodStatus inkpod_core_motion_check_stop(InkpodCore* core);
InkpodStatus inkpod_core_motion_check_toggle_pause(
    InkpodCore* core,
    InkpodMotionFrame* out_frame);
InkpodStatus inkpod_core_subpalette_set(InkpodCore* core, uint32_t index);
InkpodStatus inkpod_core_subpalette_sample(
    InkpodCore* core,
    uint32_t x,
    uint32_t y,
    InkpodColorValue* output);

/* M5 vector inputs are copied before return and commit as one history entry.
 * Geometry uses document coordinates; view zoom never rewrites these values. */
InkpodStatus inkpod_core_vector_add_path(
    InkpodCore* core,
    const InkpodVectorPathInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_path_id);
InkpodStatus inkpod_core_vector_add_fill(
    InkpodCore* core,
    const InkpodVectorFillInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_fill_id);
InkpodStatus inkpod_core_vector_erase(
    InkpodCore* core,
    const InkpodVectorEraseInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_vector_connect(
    InkpodCore* core,
    uint64_t plane_id,
    float maximum_gap,
    InkpodDispatchResult* result,
    uint64_t* out_path_id);
InkpodStatus inkpod_core_vector_correct_width(
    InkpodCore* core,
    const InkpodVectorWidthInput* input,
    InkpodDispatchResult* result);
/* Selection and rasterization use caller-owned buffers. A null pointer with
 * zero capacity performs a count/size query without retaining storage. */
InkpodStatus inkpod_core_vector_select(
    InkpodCore* core,
    const InkpodVectorSelectionInput* input,
    InkpodVectorSelectionBuffer* output);
InkpodStatus inkpod_core_vector_rasterize(
    InkpodCore* core,
    const InkpodVectorRasterizeInput* input,
    InkpodVectorRasterBuffer* output);
/* Rasterizes at document scale into a new RGBA8 raster layer and commits one
 * Undo unit. The source vector layer is preserved. */
InkpodStatus inkpod_core_vector_rasterize_to_layer(
    InkpodCore* core,
    const InkpodVectorRasterizeInput* input,
    const uint8_t* name_utf8,
    uint64_t name_bytes,
    InkpodDispatchResult* result,
    uint64_t* out_layer_id);
InkpodStatus inkpod_core_raster_vectorize(
    InkpodCore* core,
    const InkpodRasterVectorizeInput* input,
    InkpodDispatchResult* result,
    uint64_t* out_fill_count);

/* M6 filter preview never mutates committed tiles until apply. Update always
 * recomputes from the original base. Apply commits one Undo unit; cancel drops
 * the preview and returns the original checksum. Input spans are copied. */
InkpodStatus inkpod_core_filter_preview_begin(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodFilterPreviewInfo* out_info);
InkpodStatus inkpod_core_filter_preview_begin_task(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodM6Task* task,
    InkpodFilterPreviewInfo* out_info);
InkpodStatus inkpod_core_filter_preview_update(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodFilterPreviewInfo* out_info);
InkpodStatus inkpod_core_filter_preview_update_task(
    InkpodCore* core,
    const InkpodFilterInput* input,
    InkpodM6Task* task,
    InkpodFilterPreviewInfo* out_info);
InkpodStatus inkpod_core_filter_preview_cancel(
    InkpodCore* core,
    InkpodFilterPreviewInfo* out_info);
InkpodStatus inkpod_core_filter_preview_apply(
    InkpodCore* core,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_filter_apply_last(
    InkpodCore* core,
    uint64_t plane_id,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_filter_apply_last_task(
    InkpodCore* core,
    uint64_t plane_id,
    InkpodM6Task* task,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_adjustment_create(
    InkpodCore* core,
    const InkpodFilterInput* input,
    const uint8_t* name_utf8,
    uint64_t name_length,
    InkpodDispatchResult* result,
    uint64_t* out_layer_id);
InkpodStatus inkpod_core_adjustment_update(
    InkpodCore* core,
    uint64_t layer_id,
    const InkpodFilterInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_gradient(
    InkpodCore* core,
    const InkpodGradientInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_airbrush(
    InkpodCore* core,
    const InkpodAirbrushInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_boundary_airbrush(
    InkpodCore* core,
    const InkpodBoundaryAirbrushInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_blur(
    InkpodCore* core,
    const InkpodBlurEffectInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_stamp(
    InkpodCore* core,
    const InkpodStampInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_airbrush_gesture(
    InkpodCore* core,
    const InkpodAirbrushGestureInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_stamp_gesture(
    InkpodCore* core,
    const InkpodStampGestureInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_effect_blur_tool(
    InkpodCore* core,
    const InkpodBlurToolInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_dust_remove(
    InkpodCore* core,
    const InkpodDustInput* input,
    InkpodM6Task* task,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_dust_preview_begin(
    InkpodCore* core,
    const InkpodDustInput* input,
    InkpodM6Task* task,
    InkpodFilterPreviewInfo* out_info);
InkpodStatus inkpod_core_alpha_edit(
    InkpodCore* core,
    const InkpodAlphaEditInput* input,
    InkpodDispatchResult* result);
InkpodStatus inkpod_core_alpha_gradient(
    InkpodCore* core,
    const InkpodGradientInput* input,
    InkpodDispatchResult* result);

/* Task handles are Rust-owned and thread-safe. Create/query/cancel/release may
 * run on any thread. The owner must keep the handle alive until the Core call
 * using it has returned; release consumes and nulls exactly one owner slot. */
InkpodStatus inkpod_m6_task_create(InkpodM6Task** out_task);
InkpodStatus inkpod_m6_task_query(
    const InkpodM6Task* task,
    InkpodM6TaskInfo* out_info);
InkpodStatus inkpod_m6_task_cancel(InkpodM6Task* task);
InkpodStatus inkpod_m6_task_release(InkpodM6Task** task);

/* M7 graph/settings, preview, and execution. Graph/preview/report handles are
 * Rust-owned and released idempotently through pointer-to-owner storage.
 * Batch execution runs on the Core owner thread (the Windows Core-engine
 * worker), while task query/cancel are thread-safe. A cancelled call may still
 * return an owned report and INKPOD_STATUS_CANCELLED. */
InkpodStatus inkpod_batch_graph_create(
    const InkpodBatchGraphInput* input,
    InkpodBatchGraph** out_graph);
InkpodStatus inkpod_batch_graph_load(
    const uint8_t* path_utf8,
    uint64_t path_bytes,
    InkpodBatchGraph** out_graph);
InkpodStatus inkpod_batch_graph_save(
    const InkpodBatchGraph* graph,
    const uint8_t* path_utf8,
    uint64_t path_bytes);
InkpodStatus inkpod_batch_graph_get_info(
    const InkpodBatchGraph* graph,
    InkpodBatchGraphInfo* out_info);
InkpodStatus inkpod_batch_graph_release(InkpodBatchGraph** graph);
InkpodStatus inkpod_core_batch_preview(
    InkpodCore* core,
    const InkpodBatchGraph* graph,
    InkpodBatchRunScope scope,
    InkpodBatchPreview** out_preview);
InkpodStatus inkpod_batch_preview_count(
    const InkpodBatchPreview* preview,
    uint64_t* out_count);
InkpodStatus inkpod_batch_preview_get(
    const InkpodBatchPreview* preview,
    uint64_t index,
    InkpodBatchPreviewItem* out_item);
InkpodStatus inkpod_batch_preview_release(InkpodBatchPreview** preview);
InkpodStatus inkpod_core_batch_execute(
    InkpodCore* core,
    const InkpodBatchGraph* graph,
    InkpodBatchRunScope scope,
    uint64_t flags,
    InkpodBatchTask* task,
    InkpodBatchReport** out_report);
InkpodStatus inkpod_batch_report_get_info(
    const InkpodBatchReport* report,
    InkpodBatchReportInfo* out_info);
InkpodStatus inkpod_batch_report_get(
    const InkpodBatchReport* report,
    uint64_t index,
    InkpodBatchReportItem* out_item);
InkpodStatus inkpod_batch_report_release(InkpodBatchReport** report);
InkpodStatus inkpod_batch_task_create(InkpodBatchTask** out_task);
InkpodStatus inkpod_batch_task_query(
    const InkpodBatchTask* task,
    InkpodM6TaskInfo* out_info);
InkpodStatus inkpod_batch_task_cancel(InkpodBatchTask* task);
InkpodStatus inkpod_batch_task_release(InkpodBatchTask** task);

InkpodStatus inkpod_core_build_snapshot_for_view(
    InkpodCore* core,
    uint64_t view_id,
    const InkpodSnapshotOptions* options,
    /* Storage must not contain a live snapshot handle. */
    InkpodSnapshot** out_snapshot);

/* Must run on the Core's creating thread. On success, Rust allocates an
 * immutable snapshot in *out_snapshot. Output storage must not contain a live
 * handle; inputs and output must not overlap. */
InkpodStatus inkpod_core_build_snapshot(
    InkpodCore* core,
    const InkpodSnapshotOptions* options,
    InkpodSnapshot** out_snapshot);

/* May run on any thread. View pointers remain valid only while snapshot remains
 * live and no concurrent release occurs. Iterate tiles using tile_stride_bytes;
 * pixel pointers are borrowed from the same snapshot. */
InkpodStatus inkpod_snapshot_get_view(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotView* out_view);

InkpodStatus inkpod_snapshot_get_transform(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotTransform* out_transform);

InkpodStatus inkpod_snapshot_get_overlay(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotOverlay* out_overlay);

/* Returned spans borrow immutable storage from snapshot. Fill boundary ranges
 * index boundary_path_ids; vector segment records are grouped by path ID. */
InkpodStatus inkpod_snapshot_get_vectors(
    const InkpodSnapshot* snapshot,
    InkpodSnapshotVectorView* out_vectors);

/* May run on any externally synchronized renderer thread. *snapshot == NULL is
 * a successful no-op. The function releases Rust ownership and sets the owner
 * variable to NULL; copied aliases become invalid. */
InkpodStatus inkpod_snapshot_release(InkpodSnapshot** snapshot);

/* Error state is per-thread. Required size includes the trailing NUL;
 * out_written_bytes excludes it and is set to zero on copy failure. */
InkpodStatus inkpod_error_message_size(uint64_t* out_required_bytes);
InkpodStatus inkpod_error_message_copy(
    uint8_t* buffer,
    uint64_t buffer_capacity,
    uint64_t* out_written_bytes);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* INKPOD_CORE_FFI_H */
