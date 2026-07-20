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

typedef uint32_t InkpodCommandKind;
#define INKPOD_COMMAND_NO_OP UINT32_C(0)

typedef uint32_t InkpodPixelFormat;
#define INKPOD_PIXEL_FORMAT_INVALID UINT32_C(0)
#define INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8 UINT32_C(1)

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

typedef uint32_t InkpodViewCommandKind;
#define INKPOD_VIEW_PAN_BY UINT32_C(1)
#define INKPOD_VIEW_ZOOM_AT UINT32_C(2)
#define INKPOD_VIEW_FIT UINT32_C(3)
#define INKPOD_VIEW_ONE_TO_ONE UINT32_C(4)
#define INKPOD_VIEW_VIEWPORT_RESIZED UINT32_C(5)

typedef uint32_t InkpodColorDepth;
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

typedef struct InkpodCore InkpodCore;
typedef struct InkpodSnapshot InkpodSnapshot;

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
    uint32_t reserved;
    uint64_t view_revision;
    double zoom;
    double pan_x;
    double pan_y;
    uint32_t document_width;
    uint32_t document_height;
} InkpodSnapshotTransform;

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

/* Plane selection is logical UI state and does not change document revision. */
InkpodStatus inkpod_core_set_active_plane(
    InkpodCore* core,
    InkpodPlaneKind plane);

/* Fill planning is bounded and all-or-nothing. Overflow reports one candidate
 * in result and does not change pixels, revision, dirty, or history. */
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
 * FIT(viewport_w,viewport_h), ONE_TO_ONE(viewport_w,viewport_h),
 * VIEWPORT_RESIZED(viewport_w,viewport_h). Resize preserves manual view and
 * recomputes only persistent Fit/OneToOne modes. Values use client device px. */
InkpodStatus inkpod_core_apply_view(
    InkpodCore* core,
    const InkpodViewInput* input,
    InkpodDocumentInfo* out_info);

/* Must run on the Core's creating thread. On success, Rust allocates an
 * immutable snapshot in *out_snapshot. Inputs and output must not overlap. */
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
