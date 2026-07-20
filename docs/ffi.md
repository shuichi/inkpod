# C ABI

The public ABI is `include/inkpod/core_ffi.h`. ABI version 1 covers the M0
lifecycle, the M1 saved-drawing/live-preview slice, and the M2 fill, color,
eyedropper, color-check, autosave, and recovery slice.
Numeric fields use fixed-width C types. Every extensible structure begins with
`struct_size`; configuration/span structures also carry feature or reserved
fields.

Every structure pointer must expose a readable `uint32_t struct_size` prefix.
Rust reads or writes the known structure only after that prefix advertises the
complete ABI-v1 size. Record arrays carry an explicit byte stride:
`command_stride_bytes`, `sample_stride_bytes`, or `tile_stride_bytes`. Counts,
strides, alignments, enum values, flags, lengths, and overflow are validated
before a span is traversed.

## M1 and M2 typed operations

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
- `inkpod_core_set_color_check` changes temporary semantic view state only.
  Legacy-white and native-alpha modes affect subsequent render snapshots but
  never document pixels, revision, dirty state, or history.
- `inkpod_core_autosave` writes a bounded recovery container without advancing
  the normal path/savepoint. `inkpod_core_open_recovery` creates a dirty,
  recovered, pathless document. Both borrow bounded UTF-8 path bytes only for
  the call and return copied document metadata.

Stroke color is `0xRRGGBBAA` straight-alpha sRGB. Fill and eyedropper use the
explicit-depth color record. Snapshot pixels use
`INKPOD_PIXEL_FORMAT_PREMULTIPLIED_BGRA8`, suitable for Direct2D; conversion to
that display format is explicit and does not alter persisted 16-bit values.
Binary/grayscale main-line data is composited over color without exposing
mutable planes.

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

## Ownership and lifetime

- `inkpod_core_create` allocates `InkpodCore`; the caller owns it.
- `inkpod_core_destroy` takes `InkpodCore**`, releases it, and writes null. A
  repeat call through the same owner variable is a successful no-op. Destroy
  also drops any uncommitted stroke preview.
- `inkpod_core_build_snapshot` allocates an immutable `InkpodSnapshot`. During a
  live stroke it captures preview content; otherwise it captures committed
  content.
- `inkpod_snapshot_get_view` returns a borrowed strided tile span and pixel
  pointers. `inkpod_snapshot_get_transform` copies its view transform.
- All borrowed tile/pixel pointers remain valid only while that snapshot is
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

`InkpodCore` is single-writer and thread-affine: create, document/view/stroke
operations, fill/color/recovery operations, snapshot build, and destroy run on
the creating Core engine thread.
A violation returns `INKPOD_STATUS_WRONG_THREAD` without consuming the handle.
Published snapshots are immutable and `Send + Sync`; view/release must still be
externally synchronized.

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
counts, more than six inclusion colors, invalid color-depth/channel
combinations, cumulative raster/fill work, out-of-range coordinates, and
arithmetic overflow.

Error text is per-thread UTF-8. `inkpod_error_message_size` reports the required
bytes including NUL; `inkpod_error_message_copy` copies it and reports written
bytes excluding NUL. Internal truncation stops at a UTF-8 character boundary;
too-small output leaves the diagnostic available.

## M0 compatibility

`INKPOD_COMMAND_NO_OP` remains a valid real no-op, so create -> empty snapshot
-> release -> destroy and negative ABI tests remain available. Unknown command
kinds and nonzero flags are rejected rather than silently accepted.
