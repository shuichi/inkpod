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

typedef uint32_t InkpodCommandKind;
#define INKPOD_COMMAND_NO_OP UINT32_C(0)

typedef uint32_t InkpodPixelFormat;
#define INKPOD_PIXEL_FORMAT_INVALID UINT32_C(0)

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
