# C ABI

The public ABI is `include/inkpod/core_ffi.h`. ABI version 1 covers the M0
lifecycle, the M1 saved-drawing/live-preview slice, the M2 fill/color/recovery
slice, the M3 typed document-editing slice, and the M4 production-workflow
slice, the M5 vector slice, the M6 image-edit slice, and the additional bounded
operations used by the completed M0-M6 Windows GUI workflows.
Numeric fields use fixed-width C types. Every extensible structure begins with
`struct_size`; configuration/span structures also carry feature or reserved
fields.

Every structure pointer must expose a readable `uint32_t struct_size` prefix.
Rust reads or writes the known structure only after that prefix advertises the
complete ABI-v1 size. Record arrays carry an explicit byte stride:
`command_stride_bytes`, `sample_stride_bytes`, or `tile_stride_bytes`. Counts,
strides, alignments, enum values, flags, lengths, and overflow are validated
before a span is traversed.

## M1 through M3 typed operations

- `inkpod_core_new_cell` creates the two-plane `CellDocument` from a nonzero
  128-bit UUID supplied by the platform adapter and returns that UUID, stable
  IDs, frame metadata, dirty/history flags, and plane checksums.
- `inkpod_core_stroke_begin` borrows a complete style plus one or more initial
  samples and creates one Core-owned transient stroke session.
- `inkpod_core_stroke_append` borrows a strided `InkpodStrokeSampleSpan` for one
  call and extends only that preview. Callers may batch adjacent samples; they
  do not build a snapshot or cross FFI for every individual record.
- `inkpod_core_stroke_end` commits the whole session as at most one document and
  history transaction. `inkpod_core_stroke_cancel` discards it. A failed append
  also invalidates the session so partial work cannot later commit.
- `inkpod_core_apply_stroke` remains the whole-span convenience operation for
  non-interactive callers. It is equivalent to begin followed by end and keeps
  the same one-history-entry contract.
- `inkpod_core_undo` and `inkpod_core_redo` each move one transaction.
- `inkpod_core_apply_view` implements pan, anchored zoom, fit, device-pixel 1:1,
  and viewport resize without changing document revision. Values use Canvas
  client device pixels.
- `inkpod_core_save`, `inkpod_core_open`, and `inkpod_core_revert` borrow a
  bounded UTF-8 path only for the call. Paths are not stored as borrowed data.
- `inkpod_core_get_document_info` copies committed IDs, metadata, dirty/history
  flags, active plane, and deterministic plane checksums. Live preview does not
  alter these committed values.
- `inkpod_core_apply_fill` validates one complete `InkpodFillInput`, plans seed,
  closed-region, or extension edits, then commits at most one history unit.
  Overflow returns `INKPOD_STATUS_FILL_OVERFLOW` plus one candidate coordinate
  and commits no pixel, revision, dirty, or history change. Inclusion-color
  arrays are borrowed strided records, limited to six entries, and copied only
  as semantic values for the call. An empty optional span accepts a null pointer
  with zero count and zero stride.
- `InkpodColorValue` carries an explicit 8- or 16-bit depth. Eight-bit channel
  values must fit in the low 8 bits; a 16-bit value is never silently accepted
  into an 8-bit color plane. Fill tolerance is a normalized 16-bit maximum
  per-channel difference.
- `inkpod_core_eyedropper` returns an exact-depth value from selected-plane,
  topmost-nontransparent, or composite sampling. The light-table source is a
  real enum but returns unavailable until light-table content exists in M4.
- `inkpod_core_palette_set` borrows a bounded strided `InkpodColorArray`, copies
  exact 8/16-bit values into one metadata/history transaction, and never takes
  ownership of caller storage. `inkpod_core_palette_get` writes complete color
  records into a caller-owned `InkpodColorBuffer`; a zero-capacity null buffer
  is a successful count query. No palette release function is required.
- `inkpod_core_palette_generate` deterministically quantizes the current
  document composition with validated maximum-color and quantization-bit bounds,
  then replaces the exact-depth document palette as one history transaction.
- `inkpod_core_set_main_line_color` changes the exact-depth base color of an
  opened grayscale main-line plane as one metadata/history transaction;
  `inkpod_core_get_main_line_color` copies it. Binary main-line documents reject
  the setter rather than inventing grayscale semantics.
- `inkpod_core_set_color_check` changes temporary semantic view state only.
  Legacy-white and native-alpha modes affect subsequent render snapshots but
  never document pixels, revision, dirty state, or history. Snapshot feature
  bits identify the check background, and recomposed tiles receive a new render
  revision so the frontend cache uploads the view-only result.
- `inkpod_core_autosave` writes a bounded recovery container without advancing
  the normal path/savepoint. `inkpod_core_open_recovery` creates a dirty,
  recovered, pathless document. Both borrow bounded UTF-8 path bytes only for
  the call and return copied document metadata.

M3 adds these transactional operations without exposing Rust collections or
native enum layout:

- `inkpod_core_tree_edit` accepts one `InkpodTreeEdit` for create, duplicate,
  delete, reorder, properties, coloring conversion, or compatible merge. Rust
  validates the layer/plane/storage combination before committing and returns a
  stable created ID through caller-owned storage. `inkpod_core_node_get` copies
  one layer or plane descriptor; its optional UTF-8 name buffer remains caller
  owned and supports a zero-capacity size query.
- `inkpod_core_apply_selection` borrows a bounded point span for rectangle,
  ellipse, lasso, polyline, trace, or wand selection and applies new/add/
  subtract/intersect atomically. `inkpod_core_selection_adjust` provides invert,
  expand, and shrink. `inkpod_core_select_color` validates an exact-depth typed
  color and selects equal or different pixels on the active plane.
  Selection-layer conversion uses copied UTF-8 names and stable layer IDs.
- `inkpod_core_clipboard_copy` allocates an opaque Rust-owned typed payload.
  `inkpod_core_paste_begin` clones its semantic payload into transient Core
  state, so the clipboard may then be released. Floating translate/scale/rotate
  remains uncommitted until `inkpod_core_floating_commit`; cancel restores the
  exact base. Payload coordinates remain relative to document origin, not the
  source paper bounds.
- `inkpod_core_paste_begin_mode` selects compatible destination, the active
  plane with validated conversion, or a converted new plane. Copy/cut retain the
  same Rust-owned typed payload contract; clearing selected content is one
  document transaction.
- `inkpod_clipboard_render_rgba8` exports a typed payload into a caller-owned
  size-query buffer for standard Windows clipboard publication. The inverse
  `inkpod_clipboard_create_rgba8` validates and copies a bounded, possibly
  padded, straight-RGBA8 caller raster into a new Rust-owned clipboard handle;
  it never borrows the Windows global-memory block after return.
- `inkpod_core_mirror_document` and `inkpod_core_rotate_document` are
  destructive and each write one history result. `inkpod_core_resize_document`
  validates dimensions/DPI/anchor flags, transforms document metadata and
  content transactionally, and returns no partially resized document on error.
  Horizontal/vertical `inkpod_core_apply_view` flip commands are view-only.
  `InkpodSnapshotTransform.flags` carries only documented
  `INKPOD_SNAPSHOT_TRANSFORM_FLIP_*` bits; consumers must reject/ignore no other
  meaning in ABI v1.
- Guide/grid functions persist document navigation state.
  `inkpod_snapshot_get_overlay` copies the current overlay flags/grid and returns
  a borrowed strided `InkpodSnapshotGuide` span owned by the snapshot. Renderer
  validation bounds the count/stride and rejects unknown flags or guide axes.
  Locator output copies
  per-view document coordinates, optional selection bounds, and optional exact
  color. A `view_id` of zero selects the primary view; a nonzero ID returned by
  `inkpod_core_view_create` selects a secondary logical view.
  `inkpod_core_view_apply` changes only that view's transform/state,
  `inkpod_core_build_snapshot_for_view` captures it, and
  `inkpod_core_view_close` releases its logical state (snapshots remain
  independently owned).
- Shortcut rebind removes any existing key conflict before inserting the new
  bounded binding. `inkpod_core_shortcut_resolve` maps a normalized key chord
  through the current bindings; the Windows key handler uses that result for
  Undo/Redo/Copy/Paste. Reset restores deterministic defaults. Shortcut state
  is application configuration and is not part of `.inkpod` persistence.

Stroke color is `0xRRGGBBAA` straight-alpha sRGB. Fill and eyedropper use the
explicit-depth color record. Snapshot pixels use
`INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8`, suitable for Direct2D; conversion to
that display format is explicit and does not alter persisted 16-bit values.
Binary/grayscale main-line data is composited over color without exposing
mutable planes.

## M4 production workflow

- `InkpodM4RasterInput` borrows a bounded padded-row straight RGBA8/RGBA16
  raster for one call. Rust validates nested sizes, UUID/revision, DPI,
  reference frame, dimensions, stride, byte range, and storage type before any
  allocation, then compacts/copies it. Per-raster and cumulative sequence input
  are capped at 1 GiB; dimensions use the shared raster maximum.
- `INKPOD_FILL_FLAG_LIGHT_TABLE_BOUNDARY` and
  `INKPOD_FILL_FLAG_LIGHT_TABLE_COLOR` connect the read-only M4 reference
  sampling path to the existing all-or-nothing fill ABI. The Core cancellable
  variant also polls while preparing the temporary boundary; cancel leaves both
  edit and reference rasters exact.
- Light-table add/global-opacity/sample/swap operations use reference-frame
  alignment. Swap reports `INKPOD_STATUS_UNSAVED_CHANGES` without mutation
  until the current cell is clean, then retains item transform/opacity and
  replaces its source with the former editing image.
- Color-mode light-table sampling returns the source's exact RGBA8/RGBA16
  depth; opacity changes only alpha at that depth. Snapshot composition is the
  separate explicit BGRA8 display conversion.
- `InkpodSequenceInput` is a bounded strided caller-owned cell array. Core
  copies and naturally sorts it. Previous/next skips absent numbers; dirty
  switching leaves UUID/revision unchanged and returns the same explicit
  unsaved status.
- Motion-check start/step validates 30/25/24/12/10/8 FPS plus loop, selection,
  and light-table flags, returning copied cell/thumbnail metadata. Stop is
  idempotent.

No M4 caller pointer remains borrowed after return. The integrated C11/C++
smoke validates the new layouts and exercises reference alignment, 25%
effective opacity, read-only reference-boundary fill, natural order, motion
stepping, and dirty-switch rejection. Rust negative tests cover a short nested
raster, an excessive dimension, `i32::MIN` rotation, and padded-row copying.

## M5 vector workflow

- `inkpod_core_vector_add_path` copies one bounded strided span of cubic
  segments with variable endpoint widths. `inkpod_core_vector_add_fill` copies
  closed boundary-path IDs. Both validate typed vector planes and commit one
  history transaction.
- Partial, to-intersection, and whole-path erase, deterministic nearest-endpoint
  connect, and add/subtract/scale/constant width correction are transactional.
  Nested spans, finite document coordinates, widths, object counts, editable
  planes, and stable relationships are validated before mutation.
- `inkpod_core_vector_select` writes path ranges normalized to 0..1,000,000 and
  fill IDs into caller-owned buffers. Cut, touching, fully-contained, line,
  whole-line, to-intersection, fill-boundary, and fill modes are supported. A
  null span with zero capacity is a count query; insufficient capacity returns
  `INKPOD_STATUS_BUFFER_TOO_SMALL` after reporting both required counts.
- `inkpod_core_vector_rasterize` returns bounded straight RGBA8 pixels through a
  caller-owned size-query buffer. Scale is 1..16 and the antialias flag selects
  deterministic 4x4 supersampling; non-antialiased samples use pixel centers.
- `inkpod_core_vector_rasterize_to_layer` uses the same validated input but
  rasterizes at document scale into a newly named RGBA8 raster layer, preserves
  the source vector layer, returns its stable ID through caller-owned storage,
  and commits exactly one Undo unit.
  `inkpod_core_raster_vectorize` converts nonzero-alpha, equal-color RGBA8 row
  runs from a raster/color plane into closed path/fill topology as one history
  transaction and reports the fill count. `alpha_threshold` is an inclusive
  minimum for nonzero alpha; zero-alpha pixels are skipped even when the value
  is zero. The target vector layer and its trace/fill planes must be editable,
  and projected path/fill/segment/boundary totals are rejected before commit.
- `inkpod_snapshot_get_vectors` returns borrowed flat segment/fill/boundary-ID
  spans owned by the immutable snapshot. Records carry stable path/plane IDs,
  layer z-order, segment order/count, closed/stroke-visible flags, display RGBA,
  cubic points, and variable widths. The Renderer validates all spans and builds
  Direct2D fill and variable-width outline geometry without mutating Core data.

All M5 input storage is borrowed only for its call and copied where retained.
Selection/raster output storage remains caller-owned. Snapshot vector pointers
remain valid only until `inkpod_snapshot_release`, exactly like tile and guide
pointers.

## M6 image-edit workflow

- `InkpodFilterInput` remains the original 72-byte ABI-v1 record for fixed
  sharpen/blur, Gaussian, unsharp, invert, auto contrast,
  brightness/contrast, tone curve, levels, HSV, and color balance. Optional
  `InkpodCurvePoint` storage is a caller-owned bounded strided span; every point
  record and normalized 16-bit value is validated and copied before return.
  `point_stride_bytes == 0` remains accepted as packed-v1 compatibility for a
  non-empty curve, while new callers pass `sizeof(InkpodCurvePoint)`.
- `inkpod_core_filter_preview_begin` computes a preview document without
  changing committed revision, dirty state, savepoint, or history.
  `inkpod_core_filter_preview_update` re-runs from the original base rather than
  accumulating the previous result. Both return base/preview checksums and a
  transient preview revision through `InkpodFilterPreviewInfo`.
- `inkpod_core_filter_preview_cancel` drops the preview and reports the original
  checksum. `inkpod_core_filter_preview_apply` commits the preview as exactly
  one Undo unit and records it as the last filter.
  `inkpod_core_filter_apply_last` reuses that copied semantic filter on another
  validated RGBA plane.
- `inkpod_core_filter_preview_begin_task` and `_update_task` provide the same
  transaction semantics while reporting bounded work through `InkpodM6Task`.
  A pre-cancelled or mid-operation cancelled task returns
  `INKPOD_STATUS_CANCELLED` without installing or committing partial output.
  `inkpod_core_filter_apply_last_task` gives last-filter reuse the same atomic
  progress/cancel behavior.
- `inkpod_core_adjustment_create` accepts only brightness/contrast, tone-curve,
  or levels filter records plus copied UTF-8 name storage. It creates one stable
  non-raster adjustment layer; source raster storage is never retained or
  modified. `inkpod_core_adjustment_update` replaces those copied parameters as
  one Undo unit. Adjustment parameters persist in the `M6AD` native section.
- `inkpod_core_effect_gradient`, the primitive effect calls, and the full
  airbrush/stamp/blur gesture calls copy and validate their strided colors,
  stops, source, and sample spans before committing one Core history unit.
  Device-pixel batches are converted once using the selected logical view.
  Airbrush and stamp support pressure; blur's `PRESSURE_SIZE` flag is valid only
  for the screen-fixed pen region. No effect input pointer is retained.
- `inkpod_core_dust_remove` and `inkpod_core_dust_preview_begin` accept a READY
  task and an optional pen/rectangle/polyline/lasso batch. Preview shares the
  filter-preview apply/cancel API; direct removal commits once. All three dust
  modes report progress and poll cancellation before atomic commit.
- `inkpod_core_alpha_edit` copies bounded padded grayscale8/16 rows and changes
  only target alpha. `inkpod_core_alpha_gradient` applies copied gradient stops
  only to alpha. Snapshot overlay `ALPHA_VIEW` renders alpha as grayscale without
  changing pixels. These Core calls remain synchronous on the Core owner thread;
  the Windows adapter schedules them on its Core engine.

At most one stroke or filter-preview transaction may be active. Document,
history, save/open, layer, and competing preview operations return invalid state
while a preview is live; immutable snapshots may still be built and carry the
transient preview revision. All M6 caller pointers are borrowed only for the
duration of one call.

## Stroke session state

At most one live stroke exists per Core. While it is active, other document,
history, fill, save/open, plane, and view mutations return invalid state; immutable
preview snapshots may still be built. Begin/append do not advance document
revision, dirty, savepoint, or Undo/Redo. End advances document revision once
only when pixels changed and creates one Undo unit. Cancel is idempotent when no
session exists and restores the exact committed base.

Every `InkpodStrokeInput`, `InkpodStrokeSampleSpan`, or `InkpodFillInput`, its
record array, and all records are borrowed only until that ABI call returns. The
Core copies the semantic state it needs; callers retain allocation ownership
and may reuse their buffers after return. The transient stroke session and fill
plan are not allocated ABI handles and therefore have no separate release
function.

`InkpodColorArray` and all input records are likewise borrowed only for one
call. `InkpodColorBuffer` and its record storage remain caller-owned before,
during, and after the call. A successful copy initializes each known
`InkpodColorValue` record, including `struct_size`; an insufficient buffer
returns `INKPOD_STATUS_BUFFER_TOO_SMALL` and still reports `color_count`.

## Ownership and lifetime

- `inkpod_core_create` allocates `InkpodCore`; the caller owns it.
- `inkpod_core_destroy` takes `InkpodCore**`, releases it, and writes null. A
  repeat call through the same owner variable is a successful no-op. Destroy
  also drops any uncommitted stroke or filter preview.
- `inkpod_core_build_snapshot` allocates an immutable `InkpodSnapshot`. During a
  live stroke or filter transaction it captures preview content; otherwise it
  captures committed content. Output owner storage must not already contain a
  live handle; callers release or move the previous owner before reusing that
  variable.
- `inkpod_core_clipboard_copy` allocates `InkpodClipboard`; the caller owns that
  handle until `inkpod_clipboard_release` receives its owner pointer and writes
  null. A repeat release through the same owner variable is a successful no-op.
  Copy and `inkpod_clipboard_create_rgba8` output storage likewise must not
  already contain a live clipboard owner. `inkpod_clipboard_render_rgba8` writes
  only caller-owned output storage and does not create another owner.
  The opaque payload is immutable and may outlive or cross a document switch;
  the Windows adapter releases it before shutdown.
- `inkpod_core_build_snapshot_for_view` has exactly the same snapshot ownership
  and release contract as `inkpod_core_build_snapshot`.
- `inkpod_m6_task_create` allocates a Rust-owned atomic task. Its single owner
  keeps it live until the Core operation returns. `inkpod_m6_task_release` takes
  `InkpodM6Task**`, frees it, writes null, and is a successful no-op when repeated
  through the same owner variable. The caller must prevent release racing the
  Core call; query and cancel may race safely with that call.
- `inkpod_snapshot_get_view` returns a borrowed strided tile span and pixel
  pointers. `inkpod_snapshot_get_transform` copies its view transform;
  `inkpod_snapshot_get_overlay` returns a borrowed guide span plus copied flags
  and grid values; `inkpod_snapshot_get_vectors` returns borrowed vector spans.
- All borrowed tile/pixel/guide/vector pointers remain valid only while that snapshot is
  live. Tile storage is independently reference-counted, so a snapshot may
  safely outlive the Core that created it. No per-tile accessor is required.
- `inkpod_snapshot_release` takes `InkpodSnapshot**`, releases it, and writes
  null. It may run on an externally synchronized renderer thread.
- In the Windows adapter, Core engine transfers the raw owner exactly once to
  `CanvasSnapshotSink::Submit`. The sink assumes release responsibility whether
  enqueue succeeds or fails. Pending replacement and renderer shutdown release
  through `inkpod_snapshot_release`; a snapshot pointer is never sent as an
  `HWND` message parameter.
- Input/output objects, spans, owner variables, and opaque allocations passed
  to one call must not overlap.

Rust-owned storage is released by Rust. No Rust `Vec`, `String`, enum layout,
reference, trait object, or panic crosses the ABI. Arbitrary copied aliases are
caller errors after the owning pointer has been released.

## Threading

`InkpodCore` is single-writer and thread-affine: create, document/view/stroke,
 tree/selection/paste/navigation/shortcut, fill/color/recovery/vector operations,
snapshot build, and destroy run on the creating Core engine thread.
A violation returns `INKPOD_STATUS_WRONG_THREAD` without consuming the handle.
Published snapshots are immutable and `Send + Sync`; view/release must still be
externally synchronized.

`InkpodM6Task` is the exception to Core affinity: create, query, cancel, and
release may run on any thread, subject to its owner-lifetime rule. State and
completed/total work are atomic. Cancellation is advisory until the Core loop
polls it; the operation then returns cancelled and discards its staged result.

No Core lock is held while calling C++, and the Core never calls a C++ callback.
The Windows UI/Input thread copies pointer packets into a bounded C++ queue. The
Core engine consumes them and calls ABI functions on its own thread. Its
snapshot sink is a C++ ownership queue, not a callback entered from Rust. The
Renderer thread is the only owner of D3D/DXGI/D2D objects and may replace stale
pending snapshots without blocking input or Core execution.

## Validation and failures

Every fallible export catches Rust panics and converts them to
`INKPOD_STATUS_PANIC`. Additional status values distinguish I/O, invalid-state,
no-document, cancellation, and all-or-nothing fill overflow. Boundary code rejects null or
misaligned pointers, short structures, unknown required flags/enums, invalid
UTF-8/embedded NUL paths, invalid floating-point values, excessive sample/path
counts, more than six inclusion colors, palette counts above 4096, invalid
layer/plane/storage combinations, invalid selection spans/morphology, non-finite
 floating transforms, invalid view/guide/grid IDs, invalid color-depth/channel
  combinations, cumulative raster/fill/selection work, out-of-range coordinates,
 vector topology/counts/widths/output capacities, record sizes larger than their
 advertised stride, and arithmetic overflow.

Error text is per-thread UTF-8. `inkpod_error_message_size` reports the required
bytes including NUL; `inkpod_error_message_copy` copies it and reports written
bytes excluding NUL. Internal truncation stops at a UTF-8 character boundary;
too-small output leaves the diagnostic available.

## M0 compatibility

`INKPOD_COMMAND_NO_OP` remains a valid real no-op, so create -> empty snapshot
-> release -> destroy and negative ABI tests remain available. Unknown command
kinds and nonzero flags are rejected rather than silently accepted.
