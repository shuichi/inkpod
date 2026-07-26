# Architecture

## M5 component boundary

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
 gap close, exact-depth sampling/palettes, and display-only color checks. M5 also
 places platform-independent fixed-point cubic geometry, deterministic flatten/
 split/intersection calculations, variable-width hit testing, and source-over
 sampling in this crate. It has no stable document IDs, history, or UI state.
The native gap rule is deliberately explicit: a candidate pixel becomes a
virtual boundary only when hard boundaries exist on both opposing horizontal
or both opposing vertical rays and the intervening candidate run is no longer
than `gap_close`. Searches are axis-aligned, four-connected, capped at 64
pixels, and ordered deterministically. This is an inkpod rule, not a claim
about any proprietary legacy implementation.

`inkpod-format` owns the bounded `.inkpod` v1 container and has no application
state dependency. It also owns bounded common-raster codecs; PNG is the only
new third-party codec dependency, while deterministic uncompressed TIFF/TGA/BMP
live in the format crate. `inkpod-core` maps its `CellDocument` to/from the format DTO,
owns a stable-ID typed layer/plane tree, persistent selection mask, guides/grid,
document/view revisions, multi-view transforms, shortcut bindings, stroke and
floating-paste preview, fill transactions, exact-depth main-line base color and
palette metadata, normal savepoint/path, recovery state, history, and immutable
premultiplied-BGRA render snapshots. M4 adds persisted stable-ID light-table
sets/items and source rasters, reference-frame transforms, read-only fill/color
sampling, naturally ordered cut/cell sequences, thumbnails, subpalette
 sampling, dirty-safe switching/item swap, and motion-check state. M5 adds
 stable-ID vector path/fill topology, typed vector layers/planes, transactional
 draw/erase/connect/width/conversion commands, vector selection, and immutable
 flat vector snapshot records. Core stores geometry in milli-document units, so
 zoom/pan never rewrites it. An
architecture test scans Core, image, and format sources/manifests for forbidden
Windows/frontend APIs. All three crates are safe Rust; no `HWND`, COM, D2D,
DXGI, Win32 DPI, or frontend thread type enters them.

`inkpod-ffi` is the only `staticlib`. It validates fixed-layout inputs, exposes
batched stroke operations, M2 fill/color/recovery, M3 tree/selection/
 clipboard/transform/navigation/multi-view operations, copied M4 padded-row
 rasters/strided sequences/light-table/motion operations, and copied M5 vector
 commands plus caller-owned selection/raster conversion buffers. It catches panics and owns
opaque Core/snapshot/clipboard allocations. Win32 supplies a
`CoCreateGuid` document UUID at the create boundary; Core persists it without
acquiring an OS dependency. The Win32 application does not mirror pixels,
history, fill, color-depth, or format rules.

## Rust module structure

Every Rust crate root is intentionally limited to module declarations and
stable public re-exports. All four `lib.rs` files remain below 200 lines, and
`inkpod-core/src/lib.rs` contains neither `Core` implementation blocks nor
document state definitions. CMake uses a configure-aware recursive source list
for each crate's `src/**/*.rs`, so adding or moving a production submodule also
updates the Cargo static-library dependency graph.

| Module | Responsibility |
|---|---|
| `api` / `error` | Stable public commands, DTOs, view values, and Core errors |
| `core` | `Core` state, construction, document lifecycle, revision and transaction helpers |
| `document` | `CellDocument`, stable-ID layer/plane model, validation, and tree operations |
| `history` | History records, Undo/Redo, history navigation, and savepoint-facing state |
| `selection` | Selection algebra, clipboard, floating selection, and mask helpers |
| `stroke` / `paint` | Stroke session/staging and fill, palette, eyedropper, color-check operations |
| `transform` | Destructive mirror, rotate, resize, raster conversion, and frame/guide transforms |
| `view` / `snapshot` | Logical views, guides/grid/locator/shortcuts, composition, and immutable snapshots |
| `persistence` | Native save/open/recovery adapter and raster DTO conversion |
| `animation` / `vector` / `effects` / `batch` | Feature-specific state and commands |

`inkpod-image` separates pixel/raster storage, fill/sampling/palette logic, and
alpha/brush/dust/filter/gradient edits. `inkpod-format` separates native model,
encode, decode, validation and atomic I/O from common raster codecs and feature
metadata. `inkpod-ffi` separates ABI constants/records/opaque handles, boundary
validation/conversion, and feature-specific exported functions; its batch ABI
has its own records, parser and export modules.

Public Rust paths are preserved through crate-root re-exports. Internal state
that moved out of the root uses `pub(super)` only where sibling Core modules need
the same access they previously had as descendants of the crate root; it is not
exposed outside `inkpod-core`. Unit and acceptance test source is stored below
each crate's `tests/` directory. Private implementation tests are connected with
test-only `#[path]` modules, which preserves access to internal invariants
without mixing test bodies into production source files. Integration and
malformed-corpus tests remain ordinary Cargo integration targets.

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
   without a UI wait; if it reaches the queue during a live stroke, the adapter
   retains it behind stroke append/end/cancel work and runs it after the stroke
   closes. It may post value-only state/error notifications to the UI queue; it
   never mutates a window and Core never calls a C++ callback.
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

## Windows frontend state ownership

The private `apps/windows/app/app_context.h` defines the frontend composition
root. `AppContext` has no flat feature fields: it composes lifetime, main-window
handles, document-shell, tool, view, pane, animation, effects, and Batch state,
the latest immutable command-state result, plus the owned `CoreEngine`. The
header remains below `apps/windows`; none of these C++/Win32 types enter the
public C API or Rust Core.

`MainWindowHandles` contains only the main frame and child-control `HWND`s.
`DocumentShellState` owns Windows path/clipboard adapter state and the small
stable-ID cache used by existing UI and smoke adapters; it is not a second
document model. Tool, view, pane, and animation members retain only transient UI
choices and gestures. Effects owns its task/progress handles and adjustment UI
records. Batch owns its graph/preview/report/task handles together with its
palette and progress-dialog state, so derived Batch handles can be released
without access to unrelated application state.

Document replacement is coordinated by `ResetUiForDocumentReplacement`, which
calls explicit owner resets for document-shell, panes, tools, view, animation,
and effects before performing the existing tab, active-view, and timer work.
Progress callbacks receive only `EffectsUiState` or `BatchUiState`; color,
motion-label, vector-preview, adjustment lookup, Batch palette refresh, and
Batch derived-handle helpers likewise accept their narrow owner state and any
explicit window handle they need.

R3 adds private feature boundaries without changing that composition root.
`DocumentShellController` owns normal save/open, recovery, autosave, and common
raster shell coordination; `clipboard_adapter` owns Win32 clipboard formats and
memory transfer. Both call the existing Rust serialization, codec, and typed
clipboard APIs, so neither contains a second document or image implementation.
`DocumentPanesController` loads tree, light-table, sequence, and locator models
in batches, while `ColorPanesController` owns palette and color-chart model
synchronization. Native child-control population remains presentation work for
the main-window owner.

View/guide/grid, fill, selection, floating-paste, and vector controllers receive
only `CoreEngine` plus typed command inputs. Effects owns its task/progress and
preview lifecycle. Batch owns graph construction/save/load, preview/run,
progress, report/task lifetimes, and palette presentation state; queued work
captures the long-lived `BatchUiState`, never the temporary routing controller.
All Core calls retain the prior `CoreEngine::Invoke`/`Enqueue` snapshot and
document-info refresh flags. No controller invokes renderer APIs or changes the
Core-engine, renderer-thread, or snapshot queue contracts.

R4 gives that owner an explicit private boundary. `apps/windows/ui/main_window.*`
registers the frame class and creates/layouts only the standard child controls;
its API receives `MainWindowHandles`, the instance, visibility policy, and
geometry rather than `AppContext`. Feature palettes, Core calls, and document
operations stay outside that module. The 24-line top-level window procedure now
normalizes `WM_NCCREATE` and delegates commands and other messages. Eleven
command routes own the document, edit, effects, pane, animation, selection/view,
tool, color, application, and Batch/control groups. Five message handlers own
lifecycle/notifications, keyboard, Canvas input, Core/task completion, and
timer/close behavior. A CMake structural test compares their direct owner cases
with all 273 production command IDs in `app.rc` and rejects missing or duplicate
ownership.

R5 separates command queries from transitions. The private
`apps/windows/ui/command_state.*` module builds a fixed result for all 273
production command IDs from typed, feature-owned input values. Its providers do
not call Core or Win32, mutate tools, or clear previews. One application adapter
caches that result and applies it to the main menu and every matching toolbar
button; keyboard shortcuts and the Batch palette consult the same cache before
dispatch. There is currently no separate production context-menu surface.
`command_state_catalog.inc` assigns every production command exactly one state
owner, and a CMake structural test compares the catalog with `app.rc`.

Active-tool mutation is independently owned by
`apps/windows/ui/tools/tool_state.*`. Leaving a vector tool clears its geometry
preview there, while a non-vector active-plane transition performs the explicit
pencil fallback. Document replacement, tree refresh, and direct plane selection
invoke that transition boundary. A command-state refresh itself cannot change
the active tool, preview samples, Core state, or document state.

R6 finalizes the private bootstrap boundary. `apps/windows/app/main.cpp` is a
45-line launch-mode adapter: it recognizes the existing application, embedded
application-smoke, and ABI-smoke modes, then invokes either `Application` or the
existing ABI runner. `Application` owns Common Controls and COM initialization,
resource/class registration, main-window construction, Core-engine startup,
startup Recovery choice, default-cell creation, the message loop, and ordered
shutdown. It contains no feature command IDs or feature command handlers.

The existing MainWindow owner has two implementation sources. `main_window.cpp`
continues to own standard chrome creation/layout/class registration, while
`main_window_runtime.cpp` owns the already-established presentation adapters,
command routes, and top-level message normalization. Its only bootstrap-facing
private header exposes the window procedure and the existing UI-coordinated
default-cell, Recovery, error, and command-state paths; no declaration enters
the public `include/inkpod` surface. The dependency direction is `main ->
Application`; Application composes MainWindow, application smoke, and
CoreEngine, while MainWindow/application smoke depend only downward on focused
controllers and CoreEngine. The renderer is reached only through the existing
Canvas and snapshot sink. MainWindow and smoke code do not include or call
`Application`, so the graph remains acyclic.

The M1-M7 application smoke bodies are a normal private translation unit linked
directly into `inkpod.exe`, not a second harness or reusable library. This keeps
the Debug, Release, and packaged executable on the same production UI, Core
engine, renderer, and ABI path. A CMake structural test rejects bootstrap,
feature-command, or smoke bodies returning to `main.cpp`, checks all seven smoke
stages and explicit source-list entries, and keeps the existing 273-command
route owner check pointed at the MainWindow runtime source.

## Coordinate and DPI contract

Canvas input and rendering use client device pixels:

```text
device_x = document_x * zoom + pan_x
device_y = document_y * zoom + pan_y
```

Horizontal or vertical view flip is represented by a documented snapshot
transform flag and applied around the document extent before zoom/pan. It never
changes document pixels or history. Destructive mirror instead transforms every
applicable raster/selection/frame/guide value in one Core transaction.

The shared snapshot transform is the only document-to-device transform. The
Direct2D context uses pixel units and a 96-DPI target bitmap so it does not add a
second DIP scale. Per-Monitor DPI v2 still controls native UI sizing and future
physical-size policy, but a DPI notification alone does not translate or shrink
the document. Fit uses the current client size in device pixels and is recomputed
on viewport resize; manual pan/zoom is preserved across resize.

## M5 vector snapshot and renderer

The immutable snapshot owns flattened cubic segment records, fill records, and
one packed boundary-path-ID array in addition to raster tiles and overlays. A
segment repeats path-level stable IDs, z-order, color, closed/visible flags, and
segment count so the renderer can validate path groups in one pass without
per-element FFI calls. No snapshot pointer exposes mutable Core geometry.

The Renderer thread validates bounded record counts/strides/group continuity,
then creates Direct2D path geometry. Closed cubic boundary paths use alternate
fill mode. Variable-width strokes are sampled deterministically into a filled
outline; zoom/pan/flip remain the existing document-to-device D2D transform, so
the outline is generated from unchanged document coordinates. D2D/D3D/DXGI
objects stay on the Renderer thread and are reconstructed after device loss;
the retained immutable snapshot remains the reconstruction source.

## Revision, preview, and transaction model

Document and view revisions are independent. A successful committed pixel,
typed-tree, selection, paste, guide/grid, or destructive transform transaction,
Undo, Redo, new, or open advances document revision. Pan, zoom, box zoom, fit,
1:1, viewport resize, view flip, and visibility toggles advance only view
revision when semantic view state changes. Plane selection changes neither.

Document-wide M3 edits stage a cloned `CellDocument` whose sparse tile rasters
share allocations through `Arc` copy-on-write. Validation completes before the
candidate replaces committed state. History keeps the before/after document
owners rather than eagerly duplicating every tile, so layer reorder/properties,
selection morphology, guide/grid edits, and mirror remain atomic and Undoable.
Redo after Undo is discarded by any new edit exactly as for pixel history.

The app-private clipboard owns typed pixels plus absolute document coordinates.
Paste first chooses a compatible typed destination and enters transient floating
state. Move/scale/rotate edits only that preview; bounded inverse nearest-neighbor
sampling avoids holes when scaling, commit clips to the destination paper and
creates one history entry, while failure retains the floating state and cancel
drops it with no revision. Pixel edits validate both the target layer and plane
editable flags before staging.
The payload may outlive the source document and is released through Rust.

Immutable snapshots also own their guide array and copy grid/view overlay flags.
The C ABI returns that array as a bounded borrowed span. The renderer validates
the span, draws grid/guides and a transparent-paper checker under the same
document transform as tiles, and releases all borrowed data with the snapshot.
The native key handler resolves Undo/Redo/Copy/Paste through the Core shortcut
map, so the shortcut editor changes the command path used by key events.

Secondary logical views retain independent immutable `ViewState` values but
never clone document state. A snapshot build chooses one view transform at the
last moment, so an edit through either view advances the one shared document
revision and appears in every view's next snapshot.

Pointer-down starts one Core-owned `StrokeSession`. Its preview document shares
sparse tiles through `Arc` copy-on-write and records accumulated before/after
pixel changes plus the last sample. Begin/append mutate only that preview and
can therefore produce immutable snapshots before pointer-up without changing
the committed document revision, dirty state, savepoint, or history. End swaps
the preview into the document and appends exactly one history entry. Cancel,
capture loss, or any failed append drops the session and exactly restores the
base state.

Snapshot tile buffers are `Arc<[u8]>` and the Core render cache reuses unchanged
composited tile buffers between preview frames. A separately generated render
tile revision changes whenever source pixels or view-only color-check
composition changes, so the D2D cache cannot mistake a recolored check tile for
its source tile. Snapshot feature flags select the correct black or magenta
document background for sparse legacy-white/native-alpha checks without
materializing every empty raster tile. History stores before/after pixel values
or bounded palette/base-color metadata instead of a whole image copy. Undo followed by a new edit
truncates the redo branch. A unique history-state token identifies the normal
savepoint, so Undo back to the saved state clears dirty and Redo away restores
dirty.

Stroke coordinates and rasterization work are bounded cumulatively before
commit. Segments are clipped to the document before rasterization, so invalid,
extreme, or resource-limit input leaves pixels, history, and revision unchanged.

Fill follows the same transaction boundary without a live preview session.
Image code first creates an immutable `FillPlan` containing before/after pixel
edits. Selection, tolerance, inclusion, gap, cancellation, overflow, and work
limits are evaluated before Core clones the affected color raster. Oversized
documents are rejected before a selection rectangle is materialized as a mask;
seed, closed-region, and extension planners all poll cancellation. A successful
nonempty plan swaps that clone into the document and appends exactly one history
entry; invalid, cancelled, overflow, and empty plans leave pixels, main-line
checksum, dirty state, revision, and history unchanged. Closed-region and
extension operations use this same path.

Color-check mode is temporary view state. Snapshot composition may replace
display colors according to legacy-white or native-alpha categorization, but it
does not enter the document transaction or persisted file. Grayscale main-line
display coverage is likewise separate from exact base-color eyedropper
sampling. The base color and bounded palette are persisted exactly and exposed
through caller-owned strided ABI buffers. RGBA16 is retained in Core/format and
converted only when building the current BGRA8 renderer snapshot.

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

## M8 portability and performance audit

The M8 architecture test scans every Rust workspace crate (`inkpod-core`,
`inkpod-image`, `inkpod-format`, and `inkpod-ffi`), including `src`, `tests`,
`benches`, `examples`, and `build.rs`, plus crate/workspace manifests and the
resolved `Cargo.lock`. It rejects Windows-only imports, `cfg(windows)` branches,
raw-DLL links, renamed Windows crate packages, and Windows packages in the
lockfile. This is stricter than the original `ARCH-002` domain-crate scan and
proves that the whole Rust workspace can build without a Windows SDK; Win32
system libraries remain attached only by CMake to the final Windows
static-library consumer.

The standalone `large_document` benchmark constructs a maximum-dimension
1,048,576 x 1,048,576 sparse RGBA document without allocating empty tiles,
writes bounded distributed samples, checks copy-on-write isolation, and then
filters a dense 1024 x 1024 (`--quick`) or 2048 x 2048 raster. It reports elapsed
times and allocation-relevant tile/byte counts without enforcing a
machine-specific timing threshold.

The next-frontend audit found no platform type gap in document, image,
selection, history, batch, or immutable snapshot APIs. The remaining adapter
work is explicit rather than hidden in Core: a sandboxed frontend needs
byte/stream-based open/save endpoints in addition to the current UTF-8 path ABI,
must inject a platform UUID and file/bookmark authority, must provide its own
font/GPU resource resolution, and must map its native clipboard and picker to
the existing typed payloads. These are frontend/API extensions; they do not
require a Windows dependency or a second document implementation in Rust.

## Initialization and shutdown

The private `Application` initializes Common Controls, COM, the main
window/Canvas and Renderer thread, then starts the Core engine thread. Core
creation and initial 1920 x 1080 cell/Fit snapshot occur on the Core thread.
Shutdown first stops and joins Core work, then destroys the Canvas and joins its
Renderer thread, so no snapshot sink or window notification target outlives its
owner. A renderer-held immutable snapshot may outlive its Core until Canvas
destruction because it owns all borrowed tile storage and is released by Rust's
snapshot release function.

COM apartment lifetime is owned by the private application runtime module. It
remains required by the new-style Shell folder browser used by Batch; About no
longer creates a WIC COM object. Its image reuses the embedded application ICO
through `LoadIconWithScaleDown` at the target-DPI size.
Self-contained modal dialogs are private modules under
`apps/windows/ui/dialogs`: About owns its resource/DPI/font/icon layout;
shortcut, view, text, fill, history, and effects editors accept typed candidate
values and publish results only after `IDOK`. Modeless progress dialogs receive
only a progress query and cancellation callback, while the Batch palette
receives a typed presentation model plus command/selection callbacks. No dialog
module receives `AppContext`, calls `CoreEngine`, or invokes a Rust function.
R3 feature controllers translate those callbacks to their owned UI-thread task
handles and typed Core operations. R4's main-window routes delegate to those
same controllers and preserve synchronous value returns and posted-message
payload lifetimes. R5's command-state providers and tool-transition owner sit
above those routes without changing their Core calls. R6 moves the lifetime and
smoke boundaries without changing the routes themselves. These boundaries do
not change Core/Renderer threads, snapshot ownership, shutdown order, the public
C ABI, or file formats.

The hidden Windows smoke path in `apps/windows/app/app_smoke.cpp` uses the normal
UI input adapter, Core queue, ABI, format, snapshot sink, and Renderer. It
verifies a never-saved private recovery
path can be autosaved and rediscovered before normal save, distinct UI/Core/Renderer
thread IDs, a frame presented before pointer-up while committed state remains
unchanged, one-unit commit/cancel behavior, protected-plane drawing, history,
view operations, save/discard/reopen, exact Fit device bounds across DPI change,
device-loss recovery, render, and normal shutdown. Its M2 phase also dispatches
real Fill/Eyedropper/Color Check menu commands and Canvas clicks, checks a
one-unit fill with an unchanged main-line checksum, queues autosave, opens the
recovery path as dirty/pathless, and proves that reopening the normal file
restores its original checksum.
Its M3 phase drives the real Layer, Selection, Copy, Flip, Mirror, Grid, New
View, Shortcut Editor, and Shortcut Reset commands; verifies tree Undo/Redo/save/reopen and an
invalid typed combination; checks selection boolean bounds and cross-paper
coordinate paste; distinguishes view and document revisions; compares primary
and secondary snapshot revisions; validates rendered overlay data and configured
shortcut resolution; samples the locator; and presents the result.
