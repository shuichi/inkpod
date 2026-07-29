# Architecture

This document describes the current component, ownership, thread, and state
boundaries. Product behavior is specified in [`../PROMPT.md`](../PROMPT.md), and
current gaps are summarized in [`implementation-status.md`](implementation-status.md).
Completed migration steps and historical size measurements belong to Git history.

## Dependency direction

Inkpod has one platform-independent document-state owner. Dependencies and data
flow are one-way:

```text
CMake -> Cargo -> inkpod-ffi -> inkpod-core -> inkpod-format -> inkpod-image
                                      |                 ^
                                      +-----------------+

UI/Input thread -> bounded command/sample queue -> Core engine thread
                                                    | versioned C ABI
                                                    v
                                              immutable snapshot
                                                    | ownership queue
                                                    v
                                              Renderer thread -> DXGI Present
```

`inkpod-ffi` is the only Rust `staticlib`. Rust domain crates do not depend on
Win32, COM, Direct2D, Direct3D, DXGI, WIC, Windows DPI types, or frontend thread
types. C++ does not implement a second document, image-processing, history, or
native-format model.

## Rust responsibilities

| Crate           | Responsibility                                                                                                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `inkpod-image`  | Typed pixel formats, 64 x 64 sparse tiles, `Arc` copy-on-write storage, selection, fill/sampling/palette logic, vector geometry, and deterministic raster/filter/effect operations |
| `inkpod-format` | Bounded `.inkpod` v2 and `.inkbatch` models, encode/decode/validation, atomic file I/O, feature metadata, and PNG/TIFF/TGA/BMP codecs                                              |
| `inkpod-core`   | Stable-ID document/layer/plane state, history/savepoint, views, clipboard, previews, animation, vector/effects/Batch commands, persistence mapping, and immutable render snapshots |
| `inkpod-ffi`    | ABI v2 records, validation/conversion, panic containment, opaque handles, ownership functions, and feature-specific exports                                                        |

Binary, grayscale, RGBA8/16, straight-alpha, premultiplied display data, and
selection masks remain distinct types. Core stores vector geometry in
milli-document units, so view transforms never rewrite it. Win32 may provide a
platform UUID at document creation, but Core persists it without acquiring an OS
dependency.

Each crate root is limited to module declarations and stable public re-exports.
Responsibility-specific modules contain implementation. `inkpod-core` keeps
fine-grained tests of private invariants in `#[cfg(test)]` modules beside that
implementation, while public Core workflows run as a separate multi-module
integration-test target below `tests`. Architecture tests enforce small roots,
`cfg(test)` gating, recursive CMake tracking, ABI header/export parity, direct
contract-test references, and the absence of Windows dependencies throughout
the Rust workspace.

## Windows frontend ownership

The private dependency direction is:

```text
main -> Application -> MainWindow/controllers -> CoreEngine -> C ABI
                         |
                         +-> Canvas snapshot sink -> Renderer
```

- `main.cpp` parses launch mode and invokes `Application`; it contains no
  feature command, dialog, pane, message-loop, or smoke-scenario implementation.
- `Application` owns Common Controls and COM initialization, resource/class
  registration, window construction, Core-engine startup, recovery/default-cell
  choice, the message loop, and ordered shutdown.
- MainWindow owns standard chrome, message normalization, presentation adapters,
  and command routing. Each production command ID has exactly one route owner.
- Feature controllers receive typed inputs and only the state/services they own.
  `DocumentShellController` and the clipboard adapter own Windows path, picker,
  recovery, common-raster, and native clipboard coordination while Rust owns
  serialization, codecs, and typed payload conversion.
- Dialog modules receive dialog-specific initial values and results. They do not
  receive the complete application context, call `CoreEngine`, or invoke Rust
  directly. Cancel leaves caller state unchanged.
- The renderer is reached only through Canvas and the snapshot sink; controllers
  never call renderer APIs.

`AppContext` is the private composition root for lifetime, window handles,
document shell, tools, views, panes, animation, effects, Batch, the latest
immutable command-state result, and `CoreEngine`. Owner-specific reset functions
coordinate document replacement; callbacks and helpers receive narrow state,
not the complete context. Cached IDs and metadata support presentation only and
are not a C++ document model.

The fixed command-state catalog assigns all 281 production commands exactly one
state owner. Pure providers compute enabled/checked state without calling Core or
Win32 or mutating tools, previews, or documents. Menus, shortcuts, and palette
entry points consume the same cached result. The main frame deliberately has no
toolbar; every user command remains reachable through a menu leaf.

The canonical workspace uses fixed main-frame `WS_CHILD` panes. At the 96-DPI
reset baseline, active-tool options occupy the full-width top 40 DIP; the body
places an 80-DIP, single-column 20-command tool strip on the left, document tabs
and Canvas in the center, and a 320-DIP inspector on the right. The inspector
stacks color/palette/chart above layer/plane at a 32:68 ratio, while the lower
pane stacks layer above plane at 55:45. Four-DIP splitters separate each major
region. The tool buttons are 72 x 34 DIP and use 7-point, meaningful single-word
Japanese labels instead of one-character abbreviations. Buttons forward the same
command IDs as menus, and the cached command-state result drives both surfaces.

Pane widths, vertical ratios, visibility, and mirroring are stored as bounded
96-DPI values in a versioned per-user record; startup restores the last session,
while explicit commands reset, save, restore, or mirror the layout. Mirroring
exchanges the tool and inspector sides without moving the full-width options or
status regions. Mouse and keyboard splitters, DPI/font updates, and dialog
navigation remain on the UI thread. Hiding a pane immediately returns its area
to the Canvas, and narrow windows temporarily suppress the inspector before
reducing the 320-DIP minimum Canvas width. The four primary panes remain docked;
only secondary feature palettes may use separate modeless frames.

The six-part status bar reports tool/plane, zoom/view flags, coordinates,
RGBA/selection, paper/DPI, and task/shortcut/dirty state. Document tabs use the
sequence-cell name, saved filename, recovery/untitled fallback, dirty marker,
and logical view number. Locator sampling is asynchronous and discards stale
generations. Secondary palette presentation that is not yet exposed is tracked
in `implementation-status.md`, while its Core/C ABI models remain owned here.

## Thread and snapshot model

The Windows frontend has three distinct long-lived threads:

1. The UI/Input thread owns `HWND`, the message loop, Common Controls, OS dialogs,
   capture, and `WM_POINTER`/mouse normalization. It sends ordered
   begin/append/end/cancel packets to a bounded queue and never waits for Core or
   `Present` during drawing.
2. The Core engine thread creates, uses, and destroys `InkpodCore`. It is the
   single writer, preserves stroke order, batches samples, creates frame-paced
   preview snapshots, executes Core operations, and posts value-only UI
   notifications. Core never calls a C++ callback while holding its state.
3. The Renderer thread owns D3D11, DXGI, Direct2D, swap-chain, bitmap-cache, and
   frame-latency objects. It consumes the newest immutable snapshot, uploads
   changed tile revisions, and presents independently of input dispatch.

Snapshots move through a thread-safe C++ ownership queue, never as naked Rust
pointers in window-message parameters. The sink assumes release responsibility
on enqueue success and failure. Replaced pending frames are released; the current
snapshot is retained for device recovery. Render frames may be dropped, but input
samples and stroke begin/end/cancel events may not. Queue failure cancels capture
and the Core stroke so partial work cannot commit.

## Coordinate, DPI, and rendering contract

Canvas input and rendering use client device pixels:

```text
device_x = document_x * zoom + pan_x
device_y = document_y * zoom + pan_y
```

The shared snapshot transform is the only document-to-device transform. D2D uses
pixel units and a 96-DPI target, so Per-Monitor DPI scaling applies to native UI,
not a second Canvas transform. DPI notification alone does not move or resize the
document. Fit uses current client-device dimensions; manual pan/zoom survives
viewport resize.

View flips are snapshot transform flags around the document extent and do not
change document pixels or history. Destructive mirror transforms raster,
selection, frame, and guide state in one Core transaction. Raster hit
quantization uses the same half-open pixel-cell rule as locator sampling and
nearest-neighbor rendering, including magnified lower/right clicks, final edges,
and flipped views.

Immutable snapshots own raster tiles, overlays, flattened cubic segments, vector
fills, and packed boundary path IDs. Borrowed spans remain valid only for the
snapshot lifetime. The renderer validates counts, strides, and path grouping,
creates D2D geometry, and reconstructs GPU resources from the retained snapshot
after device loss. Core geometry remains in document coordinates; zoom, pan, and
flip are render transforms only.

## Revision, preview, and transaction model

Document and view revisions are independent. Successful document edits, history
movement, new, and open advance document revision. Pan, zoom, fit, viewport
resize, view flip, and other semantic view changes advance only view revision.
Plane selection changes neither.

Document edits stage validated candidate state before replacing committed state.
Sparse tile allocations are shared through copy-on-write, so history can retain
before/after document owners without eager full-image copies. A new edit after
Undo discards the redo branch. A unique history-state token identifies the normal
savepoint and drives dirty state independently of file timestamps.

- Stroke begin/append changes only a preview document. Snapshots may show that
  preview while committed revision, dirty, savepoint, and history remain fixed.
  End commits at most one history entry; cancel, capture loss, or failure restores
  the exact base state.
- Fill/filter/effect/transform operations validate limits, selection, target
  types, cancellation, and stale revision before commit. Invalid, cancelled,
  overflow, empty, or failed work leaves committed state unchanged.
- Floating paste owns typed pixels and absolute document coordinates. Transform
  changes only its preview; commit clips once and creates at most one history
  entry, while failure retains the preview and cancel drops it.
- Secondary logical views own independent `ViewState` values and share one
  document. An edit through any view appears at the same document revision in
  every next snapshot.
- Color-check and grayscale-main-line coverage are display state. Exact stored
  base colors and RGBA16 values are not replaced by BGRA8 snapshot conversion.

## Persistence and background work

Native save encodes and validates before atomically replacing a destination.
Autosave/recovery, normal save, and export retain separate savepoint semantics.
Timer autosave is enqueued without blocking the UI and is deferred behind a live
stroke. Long-running tasks expose progress and cancellation; cancellation,
failure, or stale revision does not partially commit. Format limits and recovery
details are specified in [`file-format.md`](file-format.md).

## Build, portability, and verification boundaries

CMake is the build entry. Configure-aware recursive Rust source tracking feeds a
profile-specific Cargo completion stamp and declares the staticlib/rlib
byproducts, so unchanged builds do not rerun Cargo. Windows presets validate x64
or ARM64 compiler targets, align Cargo/MSVC `/MD` profiles, assemble the matching
MSIX payload, and run strict C11/C++20/Rust tests.

The C11 header probe and C++20 ABI checks run through the real executable's
`--abi-smoke-test`. The private `--smoke-test` path uses the production UI input,
Core queue, ABI, format, snapshot sink, and renderer rather than a second frontend
implementation. Structural tests guard bootstrap boundaries, command route/state
ownership, source lists, and the absence of a toolbar.

The Rust portability guard scans sources, tests, benches, examples, build scripts,
manifests, and the lockfile for Windows-only dependencies or configuration. A
sandboxed future frontend still needs byte/stream I/O, platform UUID/file
authority, font/GPU resource resolution, clipboard, and picker adapters; those
are frontend extensions, not reasons to add Windows dependencies to Core.

The `large_document` benchmark exercises maximum-dimension sparse allocation,
distributed writes, copy-on-write isolation, and a bounded dense filter workload.
It reports resource and timing data without a machine-specific pass threshold.

## Initialization and shutdown

Application initializes Common Controls, COM, the main window/Canvas and Renderer,
then starts the Core engine. Core creation and initial document/snapshot work occur
on the Core thread. Shutdown stops and joins Core work before destroying Canvas
and joining Renderer, so no sink or notification target outlives its owner. A
renderer-held snapshot may outlive Core until Canvas destruction because it owns
its borrowed storage and is released by the Rust snapshot release function.
