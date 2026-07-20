# Architecture

## M2 component boundary

inkpod has one platform-independent state owner. The dependency direction is
one-way:

```text
CMake -> Cargo -> inkpod-ffi -> inkpod-core -> inkpod-format -> inkpod-image
                                      |                ^
                                      +----------------+

UI/Input thread -> bounded command/sample queue -> Core engine thread
                                                    | versioned C ABI
                                                    v
                                              immutable snapshot
                                                    | ownership queue
                                                    v
                                              Renderer thread -> DXGI Present
```

`inkpod-image` owns typed pixels and 64 x 64 sparse raster tiles. Allocated
tiles use `Arc` copy-on-write; an untouched 1920 x 1080 document allocates no
pixel tiles. Binary/grayscale 8/16-bit main-line and straight-alpha sRGB
RGBA8/16 color planes are distinct types. It also owns deterministic, bounded
seed/closed-region/extension planning, selection clipping, inclusion rules,
gap close, exact-depth sampling/palettes, and display-only color checks.
The native gap rule is deliberately explicit: a candidate pixel becomes a
virtual boundary only when hard boundaries exist on both opposing horizontal
or both opposing vertical rays and the intervening candidate run is no longer
than `gap_close`. Searches are axis-aligned, four-connected, capped at 64
pixels, and ordered deterministically. This is an inkpod rule, not a claim
about any proprietary legacy implementation.

`inkpod-format` owns the bounded `.inkpod` v1 container and has no application
state dependency. `inkpod-core` maps its `CellDocument` to/from the format DTO,
owns stable IDs, document/view revisions, stroke preview, fill transactions,
normal savepoint/path, recovery state, history, and immutable
premultiplied-BGRA render snapshots. An
architecture test scans Core, image, and format sources/manifests for forbidden
Windows/frontend APIs. All three crates are safe Rust; no `HWND`, COM, D2D,
DXGI, Win32 DPI, or frontend thread type enters them.

`inkpod-ffi` is the only `staticlib`. It validates fixed-layout inputs, exposes
batched `stroke_begin/append/end/cancel` and M2 fill/color/recovery operations,
catches panics, and owns opaque Core/snapshot allocations. Win32 supplies a
`CoCreateGuid` document UUID at the create boundary; Core persists it without
acquiring an OS dependency. The Win32 application does not mirror pixels,
history, fill, color-depth, or format rules.

## Windows thread model

The Windows frontend has three distinct long-lived threads:

1. The UI/Input thread owns `HWND`, the message loop, Common Controls, file and
   color dialogs, capture, and `WM_POINTER`/mouse normalization. It forwards
   begin/append/end/cancel packets immediately to a bounded Core queue. Pen
   packets include up to 256 coalesced `GetPointerPenInfoHistory` records in
   chronological order. The drawing path does not wait for Core or `Present`.
2. The Core engine thread creates, uses, and destroys `InkpodCore`. It is the
   ABI's only writer, preserves stroke event order, coalesces adjacent append
   packets without dropping samples, builds preview snapshots no faster than
   the configured frame interval, executes fill/color/save/recovery work, and
   caches copied document metadata for UI state. Timer autosave is enqueued
   without a UI wait. It may post value-only state/error notifications to the
   UI queue; it never mutates a window and Core never calls a C++ callback.
3. The Renderer thread creates, uses, and destroys D3D11, DXGI swap-chain,
   Direct2D, tile bitmap, and frame-latency objects. It consumes the newest
   immutable snapshot, uploads only changed tile revisions, waits on the
   frame-latency object, and presents independently of pointer dispatch.

The Core engine transfers each new `InkpodSnapshot*` directly to a thread-safe
C++ snapshot sink, not through an `HWND` message parameter. The sink assumes
release responsibility on both enqueue success and failure. Replacing a pending
snapshot releases the stale frame immediately; the current snapshot is retained
for device recovery. Dropping stale render frames is allowed. Dropping input
samples or stroke boundary/cancel events is not. If input cannot be queued, the
Canvas cancels capture and enqueues cancellation so a partial stroke cannot
commit.

## Coordinate and DPI contract

Canvas input and rendering use client device pixels:

```text
device_x = document_x * zoom + pan_x
device_y = document_y * zoom + pan_y
```

The shared snapshot transform is the only document-to-device transform. The
Direct2D context uses pixel units and a 96-DPI target bitmap so it does not add a
second DIP scale. Per-Monitor DPI v2 still controls native UI sizing and future
physical-size policy, but a DPI notification alone does not translate or shrink
the document. Fit uses the current client size in device pixels and is recomputed
on viewport resize; manual pan/zoom is preserved across resize.

## Revision, preview, and transaction model

Document and view revisions are independent. A successful committed pixel
transaction, Undo, Redo, new, or open advances document revision. Pan, zoom,
fit, 1:1, or viewport resize advances only view revision when the transform
changes. Plane selection changes neither revision.

Pointer-down starts one Core-owned `StrokeSession`. Its preview document shares
sparse tiles through `Arc` copy-on-write and records accumulated before/after
pixel changes plus the last sample. Begin/append mutate only that preview and
can therefore produce immutable snapshots before pointer-up without changing
the committed document revision, dirty state, savepoint, or history. End swaps
the preview into the document and appends exactly one history entry. Cancel,
capture loss, or any failed append drops the session and exactly restores the
base state.

Snapshot tile buffers are `Arc<[u8]>` and the Core render cache reuses unchanged
composited tile buffers between preview frames. History stores before/after
pixel values instead of a whole image copy. Undo followed by a new edit
truncates the redo branch. A unique history-state token identifies the normal
savepoint, so Undo back to the saved state clears dirty and Redo away restores
dirty.

Stroke coordinates and rasterization work are bounded cumulatively before
commit. Segments are clipped to the document before rasterization, so invalid,
extreme, or resource-limit input leaves pixels, history, and revision unchanged.

Fill follows the same transaction boundary without a live preview session.
Image code first creates an immutable `FillPlan` containing before/after pixel
edits. Selection, tolerance, inclusion, gap, cancellation, overflow, and work
limits are evaluated before Core clones the affected color raster. A successful
nonempty plan swaps that clone into the document and appends exactly one history
entry; invalid, cancelled, overflow, and empty plans leave pixels, main-line
checksum, dirty state, revision, and history unchanged. Closed-region and
extension operations use this same path.

Color-check mode is temporary view state. Snapshot composition may replace
display colors according to legacy-white or native-alpha categorization, but it
does not enter the document transaction or persisted file. Grayscale main-line
display coverage is likewise separate from exact base-color eyedropper
sampling. RGBA16 is retained in Core/format and converted only when building the
current BGRA8 renderer snapshot.

## Build graph

CMake is the build entry. Its custom command explicitly lists every library
manifest/source from `inkpod-image`, `inkpod-format`, `inkpod-core`, and
`inkpod-ffi`, produces a profile-specific completion stamp, and declares
Cargo's staticlib/rlib as byproducts. An unchanged repeat build therefore does
not run Cargo.

The checked-in presets use single-configuration Ninja builds with the MSVC x64
developer environment. Cargo `debug`/`release` and CMake `/MD` select matching
profiles and runtimes.

The C11 header probe is compiled as a C object, linked into the application,
and invoked by `inkpod.exe --abi-smoke-test` together with the C++20 ABI tests.
This preserves a real C11 include/layout check while allowing the local Windows
application-control policy to run one approved test binary.

## Initialization and shutdown

The application initializes Common Controls, COM, the main window/Canvas and
Renderer thread, then starts the Core engine thread. Core creation and initial
1920 x 1080 cell/Fit snapshot occur on the Core thread. Shutdown first stops and
joins Core work, then destroys the Canvas and joins its Renderer thread, so no
snapshot sink or window notification target outlives its owner. A renderer-held
immutable snapshot may outlive its Core until Canvas destruction because it owns
all borrowed tile storage and is released by Rust's snapshot release function.

The hidden Windows smoke path uses the normal UI input adapter, Core queue, ABI,
format, snapshot sink, and Renderer. It verifies distinct UI/Core/Renderer
thread IDs, a frame presented before pointer-up while committed state remains
unchanged, one-unit commit/cancel behavior, protected-plane drawing, history,
view operations, save/discard/reopen, exact Fit device bounds across DPI change,
device-loss recovery, render, and normal shutdown. Its M2 phase also dispatches
real Fill/Eyedropper/Color Check menu commands and Canvas clicks, checks a
one-unit fill with an unchanged main-line checksum, queues autosave, opens the
recovery path as dirty/pathless, and proves that reopening the normal file
restores its original checksum.
