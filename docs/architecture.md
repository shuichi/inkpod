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
vector path/selection/rasterization/conversion/thumbnail work, Batch codec
codes/operations/filters/payloads, destructive transform orchestration/raster/
frame/numeric helpers, and view commands/coordinates/guides/secondary views/
shortcuts in separate modules; their `mod.rs` files remain declarative indices.
`inkpod-core` keeps
fine-grained tests of private invariants in `#[cfg(test)]` modules beside that
implementation, while public Core workflows run as a separate multi-module
integration-test target below `tests`. Architecture tests enforce small roots,
the responsibility split, `cfg(test)` gating, recursive CMake tracking, ABI
header/export parity, direct contract-test references, and the absence of
Windows dependencies throughout the Rust workspace.

## Windows frontend ownership

The private dependency direction is:

```text
main -> Application -> MainWindow/controllers -> CoreHost -> C ABI
                         |                         |
                         |                         +-> SnapshotEnvelope queue
                         |                                      |
                         +-> Canvas HWND -> RendererHost -> CanvasSurface
```

- `main.cpp` parses launch mode and invokes `Application`; it contains no
  feature command, dialog, pane, message-loop, or smoke-scenario implementation.
- `Application` performs Common Controls and COM initialization, resource/class
  registration, window construction, recovery/default-cell choice, the message
  loop, and ordered shutdown through its heap-owned `ApplicationHost`.
- MainWindow owns standard chrome, message normalization, presentation adapters,
  and command routing. Each production command ID has exactly one route owner.
- Feature controllers receive typed inputs and only the state/services they own.
  `DocumentShellController` and the clipboard adapter own Windows path, picker,
  recovery, common-raster, and native clipboard coordination while Rust owns
  serialization, codecs, and typed payload conversion.
- Dialog modules receive dialog-specific initial values and results. They do not
  receive the complete application context, call `CoreHost`, or invoke Rust
  directly. Cancel leaves caller state unchanged.
- The renderer is reached only through Canvas and the snapshot sink; controllers
  never call renderer APIs.

`ApplicationHost` is the process-lifetime composition root. It owns global
shortcut and clipboard state, the frontend routing/token registries, job state,
one `CoreHost`, one `RendererHost`, a single-entry workspace registry, and a
bounded multi-entry document registry. The G6 UI exposes those document entries
as tabs in one or two `EditorGroup` values. Each visible group owns one tab
control, active frontend view, focus history, Canvas slot, and Canvas identity;
inactive tabs do not own Canvas surfaces.
`WorkspaceWindow` owns the top-level `HWND`, all child/control handles,
window-local command/menu/status presentation, pane handles, tool presentation,
and layout state. `DocumentSession` owns the file/recovery shell and an explicit
non-owning binding to `CoreHost`; its strong session ID and generation select
exactly one Core entry. Its `DocumentView` values bind strong
frontend view identities to Core-local view IDs and exclusively own view
presentation state such as flip, guide/grid display, pointer/locator state, and
gesture presentation. All views in the session therefore share one Core handle,
history, dirty state, and savepoint while retaining independent presentation.
Cached IDs and metadata support presentation only and are not a C++ document
model.

`DocumentRegistry` also owns the canonical identity index. An existing Windows
file is keyed by `FILE_ID_INFO` volume and file ID when available, otherwise by
its normalized absolute case-insensitive path; an untitled session is keyed by
a generated UUID. Display names and tab positions are never identities. Open
resolves this index before creating a Core entry and selects an existing view on
a duplicate. Save As stages the destination identity, rejects a conflict with a
different live session before writing, and publishes the new shell path,
identity index, title, bounded recent-file entry, and recovery metadata only
after save succeeds. A failed save leaves the old identity and presentation
intact.

The top-level window stores only its `WorkspaceWindow*` in `GWLP_USERDATA`.
The window procedure reaches process services through the workspace's explicit
`ApplicationHost*` link; it does not reinterpret the stored value as an
application-global context. Construction is process host, workspace, document,
view, RendererHost thread, window/Canvas surface registration, CoreHost thread,
then session Core/surface binding. Shutdown rejects new session work, unbinds
the session shells, drains accepted work, cancels live strokes, and destroys
every Core handle on the CoreHost owner thread. It then stops RendererHost,
which releases pending and retained snapshots and destroys every CanvasSurface
and shared GPU resource on the renderer thread, before destroying window-local
controls and the top-level window. Document and workspace owners are cleared
before releasing the application host. Registry initialization
uses candidate ownership so invalid input or allocation failure leaves the
previous owner intact, and a failed later owner creation unwinds earlier owners.

The main-window responsibility entries are physically separated into window
procedure, command router, keyboard/input router, document presenter, and status
presenter translation units. The remaining runtime helpers are internal to
those owner-facing entry points. `CoreHost` owns one long-lived Core engine
thread and a `DocumentSessionId` + `Generation` keyed registry of Core handles.
Create, new/open/import, command, input, snapshot, save, rebind, close, and
destroy work captures that key before queueing. Per-session active stroke,
sequence/pending counts, cached document info, diagnostics, active Core view,
and metrics prevent equal Core-local IDs or revisions from crossing sessions.
Close marks a session non-accepting before its ordered close item, resolves all
previously accepted work, cancels a live stroke, and destroys the handle on the
owner thread. Long operations still share this single lane and may delay other
sessions; worker/revision splitting remains measurement-driven G13 work.

`ApplicationHost` owns one `RendererHost` and starts it before any Canvas or
Core. Its single renderer thread owns a shared D3D11 device/immediate context,
DXGI factory, Direct2D factory/device, device generation, and upload-cache
budget. Each registered `CanvasSurface` owns only its Canvas `HWND` binding,
swap chain, frame-latency handle, Direct2D device context/target, retained
snapshot, overlays, and tile cache. Canvas creation/destruction on the UI thread
performs a synchronous register/unregister handshake; resize, DPI, visibility,
paint, and preview work is queued by strong `CanvasId` plus surface generation.
There is no per-Canvas renderer thread or per-Canvas D3D/D2D device.

CoreHost publishes through a `SnapshotEnvelope` containing document session,
frontend view, Canvas, document generation, surface generation, document
revision, and view revision. RendererHost accepts it only when the complete
route equals the current surface binding and the snapshot accessors confirm both
revisions. Rebind clears the old retained snapshot before accepting the new
route. Stale, hidden, occluded, queue-full, replaced, closed, and shutdown paths
all consume the Rust snapshot owner exactly once. Device loss first discards all
surface GPU resources, recreates the shared device, then reconstructs every
surface cache from its retained immutable snapshot; Core document state is not
involved. G6 retains one Canvas surface per visible editor group, up to two,
rather than per open or inactive tab. Closing a group unregisters its snapshot
sink before destroying its Canvas and moves its views to the surviving group.

The UI/Input thread owns the frontend target registry. Workspace window,
document session, document view, editor group, Canvas, pane, job, and generation
identities are non-interchangeable strong types in one monotonic frontend value
namespace; they are distinct from Core-local IDs. Menu, shortcut, pane-button,
and main-window `WM_COMMAND` entry points converge on `IssueCommand`, which also
serves context-menu commands as those surfaces are added. It captures a
pointer-free `CommandContext` before routing. Command state and execution use
the same owner-to-target-scope mapping. Missing scope, unknown command, stale
generation, closed view/pane/job, and document replacement are rejected without
falling back to the currently active view.

Asynchronous Core work receives a copy of the issue-time context. Filter/effect,
Batch, autosave, Canvas effect gesture, and locator completions validate that
copy before changing UI state; a stale preview in the still-current document is
cancelled instead of committed. Timer, drag, and posted-notification tokens are
monotonic values bound to a context/generation. Locator results are copied into
a bounded mutex-protected queue and posted by value token, while an atomic
pending token makes enqueue, allocation, replacement, and `PostMessage` failure
drop only the matching request. Core state/failure notification records likewise
carry session ID, generation, copied context, and status in a bounded host queue;
the window message carries only a value token and generation. No C++ or
Rust-owned object pointer is placed in `WPARAM` or `LPARAM`. Canvas stroke and
view-gesture payloads follow the same rule: `CanvasHost` owns them in bounded
queues until the workspace takes the matching token plus surface generation.
Document-bound and preview queries use typed Canvas APIs rather than output
pointers in custom messages. G6 exposes one workspace with one or two editor
groups and one Canvas per visible group. Focus or explicit group activation
first cancels the prior group's live stroke, then selects the captured frontend
view, session, Canvas route, and Core-local view before refreshing pane, menu,
status, title, and autosave presentation. Mouse hover does not activate a group.
`CoreHost` maps frontend view IDs to Core-local view IDs and builds one immutable
snapshot for each matching visible Canvas; the primary view uses the primary
snapshot path and secondary views retain independent zoom/pan/flip state. All
views of a session still share its document, history, dirty state, and savepoint.
Inactive-session notifications validate their captured session/generation and
update only tab dirty/processing presentation; they do not retarget the active
view or request continuous snapshots.

The fixed command-state catalog assigns all 322 production commands exactly one
state owner. Pure providers compute enabled/checked state without calling Core or
Win32 or mutating tools, previews, or documents. Menus, shortcuts, and palette
entry points consume the same cached result. The main frame deliberately has no
toolbar; every user command remains reachable through a menu leaf.

G8 adds a UI-thread-owned, fixed-capacity `PaneTargetRegistry` beside the target
registry. It stores only `PaneInstanceId`, strong session/view/job IDs,
generation-tagged value contexts, and the four explicit policies `Application`,
`FollowActiveView`, `PinnedDocument`, and `Job`. A pane captures its action target
before routing; a stale pinned or job target never falls back to the active tab.
Closing a pinned document returns that pane to follow mode with a consumable
accessible notice, while closing a job leaves its pane action disabled until it
is explicitly rebound. Locator completion records copy the captured context and
bounded pixel data into application-lifetime storage; `PostMessage` carries only
the token and generation.

The sequence/file-preview palette uses the same pane-target registry. Its header
shows follow/pinned state, and every import or cell activation captures one exact
session/generation before dispatch. Mixed PNG/TIFF/TGA/BMP inputs are copied and
decoded atomically on the Core owner thread; C++ neither decodes pixels nor owns a
second sequence model. The modeless owner-drawn list obtains bounded straight
RGBA8 thumbnails through caller-owned query/copy buffers. Opening a numbered
raster discovers only sibling files whose prefix and suffix around the final
numeric run match, then selects the opened cell in Core natural order. Dirty-cell
cancel, stale target, decode failure, and endpoint no-op leave the current cell
unchanged.

The Light Table palette also uses the pane-target registry. Its set/item
selection is valid only with the captured session/generation namespace, and
every mutation dispatches to that exact Core handle. Canvas movement retains
the issue-time `CommandContext` until commit or cancel; a focus change, close,
or stale generation cannot redirect it. The UI caches only bounded set/item
metadata, while raster storage, snapshots, history, and persistence remain
Core-owned. Modeless pane state is attached on the UI thread after dialog
creation; window messages do not carry C++ object or Rust-owned pointers.

The subpalette/reference palette completes the read-only auxiliary-display
path. It owns one UI-thread Canvas child and one auxiliary `CanvasId`; the
RendererHost still owns that Canvas surface and presents it on the renderer
thread. Its captured document session owns one Core-local secondary view for
independent zoom, pan, flip, and viewport state. Core builds a Rust-owned
immutable snapshot directly from the registered sequence raster without
installing it as the editable document or changing document revision, dirty
state, history, or savepoint. The Canvas consumes pointer strokes and converts
only view gestures and sample coordinates; it never enqueues edit input. A
target rebind or shutdown first unbinds the snapshot sink, closes the secondary
view on the captured Core owner thread, destroys the palette, and unregisters
the auxiliary Canvas ID. Queue rejection retains a single snapshot-release
owner.

Color and Batch panes use the same target registry. Color registration, clear,
load, save, and main-line changes capture the pane's exact session/generation;
the header states follow or pinned policy. A Batch run replaces that policy
temporarily with a generation-tagged `JobSessionId`. Preview, synchronous smoke,
and queued execution all use the captured document session instead of resolving
the later active tab. Completion validates the original target, publishes
progress/result only there, closes the job, and restores the prior follow/pin
policy. Closed/stale targets and queue failure cannot redirect a result.

The canonical workspace is represented by an HWND-free, fixed-capacity
`DockLayoutModel`. Four primary `PaneDescriptor` records give tool, tool options,
color, and layer stable type IDs and localized resource titles, default and
allowed zones, target scope, multiplicity, float/auto-hide capability, and
minimum/preferred sizes. The model permits only `TopContext`, `Left`, `Right`,
and `Bottom` stacks around the central `EditorArea`, plus floating and hidden
placements. A stack is either tabbed or split in one direction; recursive/free
dock trees cannot be represented. At the 96-DPI reset baseline, active-tool
options occupy the top 40 DIP, the 80-DIP tool strip is left, and the 320-DIP
color/layer stack is right at 32:68. The layer pane retains its internal 55:45
layer/plane split. Tool buttons remain 72 x 34 DIP with meaningful single-word
Japanese labels, forward the menu command IDs, and consume the same cached
command state.

`WorkspaceWindow` owns one UI-thread `DockHost`, which applies the pure geometry
to the existing primary pane child windows. Docked content is parented to the
main frame; floating content is reparented into an ordinary main-window-owned
top-level frame and returns to the same child HWND when docked. Drag preview is
limited to descriptor-allowed zones. Standard tab controls, mouse/keyboard
splitters, pane context menus, and the Window menu provide tab/split, dock,
float, hide, restore, and reset without retargeting a document command. Floating
close maps to hide, preserving the pane's controller state. All HWND and Common
Controls activity remains on the UI/Input thread; Core and renderer ownership is
unchanged.

G9 persists a bounded version 4 workspace record in HKCU. It contains only main
window placement, editor split orientation/ratio, dock zones/order/ratios,
primary and secondary pane visibility/size/floating placement, AutoHide edge,
density, and selected or user-named preset; document paths and document/Core
identities are excluded. The decoder validates its exact size, counts, enums,
stable pane IDs, duplicate IDs, placement bounds, and bounded terminated name.
Unknown pane IDs are ignored, absent known panes retain current defaults, and an
invalid or unsupported record restores the default without aborting startup.
Version 2 fixed and version 3 dock records migrate once to version 4.

The five built-in presets are Coloring, Line Cleanup, Reference Check, Batch,
and Focus. Save, Save As, Restore, and Reset share the normal command/state/
shortcut catalog. Secondary palettes use resource-titled standard-button
AutoHide edge strips, keeping keyboard and accessibility behavior in Common
Controls. Main and floating placements are captured in physical screen pixels
with their source DPI and clamped to current monitor work areas on display, DPI,
and taskbar changes; conversion occurs once. Narrow/compact geometry never
mutates the saved logical layout. If an editor Canvas owns input capture, preset
presentation is deferred until that Canvas sends its `CanvasId` and surface
generation as values after interaction end, so a layout switch cannot cancel or
retarget an active stroke.

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
3. The RendererHost thread owns the shared D3D11/DXGI/Direct2D device graph and
   every registered CanvasSurface's swap chain, target, bitmap cache, and
   frame-latency object. It validates route/revision envelopes, consumes the
   newest immutable snapshot per surface, uploads changed tile revisions, and
   presents independently of input dispatch.

Snapshots move through a bounded thread-safe C++ ownership queue, never as naked
Rust pointers in window-message parameters. The sink assumes release
responsibility on enqueue success and failure. A newer pending envelope replaces
only the older undrawn envelope for the same Canvas; different surfaces remain
independent. Hidden/minimized/occluded surfaces reject new snapshot builds and
skip Present until visibility/paint requests resume them. The current snapshot
is retained for device recovery. Render frames may be dropped, but input samples
and stroke begin/end/cancel events may not. Queue failure cancels capture and the
Core stroke so partial work cannot commit.

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

Core keeps document points/sizes/rectangles, device points/sizes/offsets, and
zoom as distinct private types. Public Rust commands and state accessors retain
their established scalar/record shapes, and C ABI v2 retains the same fixed
layout; those boundaries validate and convert before calling the typed
`ViewTransform`. Locator sampling, guide/grid snapping, and stroke/effect input
use document points after their single `CoordinateSpace` conversion. Snapshot
raster origins/sizes are likewise typed internally, while raster, vector, and
overlay output coordinates remain document-space public records.

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

Core-owned identities use distinct internal newtypes for documents, layers,
planes, vector paths/fills, light-table sets/items, and secondary views. History
state plus document, view, render-cache, and preview revisions are separate
tokens with their own increment policy. A typed cursor allocates the one
document-wide stable-ID namespace through domain-specific methods; there is no
conversion between identity domains. Public Rust records, C ABI v2 records, and
`.inkpod` DTOs intentionally retain their established `u64` representation and
convert only at those boundaries. The public `Guide` slice and
`LightTableSource` input value remain raw compatibility boundary objects because
changing their stored field types would break the existing Rust API; Core still
allocates guide identities through `GuideId`, and no private layer/plane/vector
lookup accepts either boundary value as another identity kind. Pure raster-
building helpers are the documented revision exception: their raw value is
passed directly to `inkpod-image` tile mutation APIs and may originate from a
committed `DocumentRevision` or an uncommitted `PreviewRevision`; no Core-owned
state stores that value without its semantic newtype.

Immediate synchronous document edits use one internal owning transaction. It
keeps immutable `before` and mutable `working` documents plus base and commit
revisions; only an explicit consuming commit may publish the working document.
Commit rejects a stale base and revision/history overflow before changing live
state, treats an unchanged working document as a no-op, and otherwise updates the
document, revision, one history entry, and render-cache invalidation together.
Palette and main-line-color edits retain their existing history labels and cache
policy through constrained transaction commit modes. Sparse tile allocations are
shared through copy-on-write, so history can retain before/after document owners
without eager full-image copies. A new edit after Undo discards the redo branch.
A unique history-state token identifies the normal savepoint and drives dirty
state independently of file timestamps.

Preview/session, floating-selection, cancellable Batch/effect, external reload,
and potentially long-running raster/vector conversion paths retain their
specialized staging ownership. Their completed candidate state passes through
the same stale-checked atomic publish boundary; cancel or failure drops the
candidate without changing committed document, history, revision, or cache.

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

The `large_document` image benchmark exercises maximum-dimension sparse
allocation, distributed writes, copy-on-write isolation, and a bounded dense
filter workload. The `core_workflows` benchmark separately covers sparse and
dirty-tile snapshots, view-only cache reuse, Undo/Redo, light-table composition,
vector snapshot/rasterization, and in-memory Batch preview/dry-run. Both expose
fixed quick/full inputs and semantic counters/checksums; timing is reported
without a machine-specific pass threshold.

## Initialization and shutdown

Application initializes Common Controls, COM, frontend owners, and RendererHost;
it then creates the main window/Canvas surface and starts CoreHost. Core creation
and initial document/snapshot work occur on the Core thread. Shutdown stops and
joins Core work before stopping and joining RendererHost, then destroys the
Canvas `HWND`; the stopped Canvas unregister is a safe no-op. No sink or
notification target outlives its owner. A renderer-held snapshot may outlive
Core until RendererHost shutdown because it independently owns all borrowed
storage and is released by the Rust snapshot release function.
