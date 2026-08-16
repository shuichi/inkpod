# Architecture

This document describes the current component, ownership, thread, and state
boundaries. Product behavior is specified in [`../SPEC.md`](../SPEC.md), and
current gaps are summarized in [`implementation-status.md`](implementation-status.md).
Completed migration steps, superseded designs, and past measurements are
summarized in [`legacy.md`](legacy.md).

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
| `inkpod-format` | Bounded procedure-authoritative `.inkpod` v26 Cell/Cut containers and `.inkbatch` v2 models, streaming encode/decode/validation, atomic file I/O, and PNG/TIFF/TGA/BMP codecs                     |
| `inkpod-core`   | Stable-ID document/layer/plane state, immutable Genesis/base surfaces, a content-addressed canonical asset registry, StateId savepoints, views, clipboard, previews, animation, vector/effects/Batch commands, persistence mapping, immutable render snapshots, and canonical primitive execution plus append-only journal/cache-free replay and semantic document digests for the migrated Core slice |
| `inkpod-ffi`    | ABI v15 fixed records and generation-tagged runtime IDs, persistence/compaction diagnostics, validation/conversion, panic containment, ownership functions, and feature-specific exports                |

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

## Primitive, route, and journal target contract

The current procedure-history architecture keeps one machine-readable inventory in
[`primitive-route-inventory.md`](primitive-route-inventory.md). It covers every
public `Core` method, every exported C ABI function, and every production
Windows command. Each route has exactly one of these semantic classes:

| Class | Meaning |
|---|---|
| `document-primitive` | Creates Genesis or commits one atomic document meaning change |
| `history-control-event` | Performs an actual Undo, Redo, or history jump and records that move |
| `editor-state-command` | Changes document-session editing state without document history |
| `view-only-command` | Changes a logical view or frontend presentation only |
| `transient-preview-stroke` | Begins, updates, cancels, or owns uncommitted staging state |
| `query-snapshot` | Observes state or builds immutable output without semantic mutation |
| `asset-data-plane` | Ingests, copies, exports, saves, or releases bounded bulk data |
| `os-application-adapter` | Owns path authority, application preferences, windows, jobs, and shell orchestration |

Classification precedence is the committed effect, not the name of an entry
point. A stroke/filter/floating apply endpoint is a document primitive; its
begin/update/cancel calls are transient. A function that ingests bytes and then
commits a document is a document primitive at the commit route and data-plane at
the ingestion route. Query and application wrappers never become primitives
merely because they call Core.

New and raster-import-as-document create Genesis and are document-primitive
routes. Open/recovery/whole revert instead stage and replace an entire Core
generation from the native data plane; they are lifecycle/data-plane routes and
do not append an Import procedure to the document being replaced. Sequence
source ingestion configures bounded external cell data and is data-plane until
an explicit active-cell/editor target or Import primitive is invoked. Export,
copy, and save likewise remain data-plane operations.

`document-primitive` and `history-control-event` routes have `rust-core` as
their semantic owner on the Rust, C ABI, and Windows surfaces. The FFI and
Windows entries are adapters to that owner and may not be catalogued as a C++
feature implementation. The architecture guard compares the inventory with
every public `Core` method, source-derived public non-Core in-place/file
mutation, all C exports, and `app.rc`; a missing route,
duplicate class/owner, unknown class, stale entry, or C++ document/history owner
fails the build. Low-level `inkpod-image` in-place values and `inkpod-format`
atomic file helpers are catalogued separately because they cannot publish a live
Core transaction by themselves.

The target control flow for a document primitive is:

```text
typed frontend request
  -> validate session/generation/base revision/target IDs
  -> copy or resolve bounded Rust-owned data-plane input
  -> canonicalize coordinates, colors, options, payload, and output IDs
  -> execute against private working state
  -> reject invalid/no-op/cancel/stale/overflow/resource failure
  -> atomically publish document + StateId + history + journal + revision
     + dirty/cache invalidation + all high-watermarks
```

The control plane contains only fixed-width values and Rust-owned object/asset
IDs. A borrowed C record is call-by-value in meaning and is not retained after
return. Variable samples, paths, encoded images, and clipboard payloads enter
through bounded data-plane APIs. Every ABI-v2 ingestion route synchronously
validates and copies borrowed data before returning. Raster open/import,
clipboard, and Light Table sources are interned in the canonical asset registry;
stroke samples become an owned inline payload up to 4 MiB and one immutable
sample asset above that cutoff. Sequence sources remain bounded Rust-owned raster
copies, and vector edits retain their typed geometry in the canonical invocation. Neither Core nor a
committed procedure retains the caller's record, buffer, file name, or path. A
committed procedure contains bounded inline canonical bytes or immutable
content-addressed `AssetId` values, never a raw pointer, path, native enum layout,
callback, STL object, temporary object ID, OS DPI, or frontend command ID.

The persistent journal is the closed sequence `Commit`, `HistoryMove`, and
`BranchCut`. `Commit` contains one canonical procedure and its parent/committed
StateIds. `HistoryMove` changes the cursor without creating a history item.
Whenever the cursor is not the active branch tail (whether reached by Undo or a
backward/cross-branch Jump), `BranchCut` precedes the new commit; both records
publish atomically and the inactive branch remains retained. A Jump record names
the post-move active branch, which must contain the destination on its ancestry
path; a commit at that branch's tail needs no cut. Invalid, failure, cancel,
stale, overflow, semantic no-op, query, preview update, and stroke sample calls
emit no journal entry and consume no persistent ID.

Persistent `StateId` names Genesis and committed semantic document states.
Session-local `DocumentRevision` exists only for stale request detection and is
rebased when a Core opens; it is never serialized. Persisted `EditorRevision`
and `EditorStateDigest` are separate from document history. Exact ID start,
ordering, canonical fixed-point, digest framing, and resource-limit rules live
in [`file-format.md`](file-format.md).

Every production document mutation now implements this boundary. The original
value/ID ABI-v3 catalog (`SetMainLineColor`, `ReplacePalette`, bounded
`ApplyRasterStroke`, and `ImportRasterAsset`) continues through
`Core::execute_primitive`; the remaining typed public methods construct a
`CanonicalInvocation`, execute the same method against a private staged Core,
and publish only after it produced exactly one valid document commit.
`Core::replay_procedure` validates the retained procedure and invokes the same
typed route rather than a second semantic implementation;
`Core::document_state_digest` observes the memory-layout-independent BLAKE3-256
semantic state digest. Schema 9 is a domain-separated commitment tree: one
metadata commitment plus stable-Plane-ID-keyed raster roots, whose leaves commit
to logical tile pixels. A revision-matched raster-tile edit reuses every
unchanged leaf and updates only the changed tile, its raster root, and the
document root. Metadata-only edits instead recompute the metadata commitment,
and broader document edits use a cold rebuild; either path produces the same
digest as a cold recomputation, independent of edit count and tile
materialization order. This runtime state-digest cache is not the render cache,
does not supply `RenderTile.source_revision`, and is never consulted while
building a view-only snapshot. Public Rust APIs are wrappers over this boundary
rather than alternate mutation implementations.

For all production history, Core owns an append-only runtime
`JournalEntry::{Commit, HistoryMove, BranchCut}` sequence. A canonical history
entry shares its retained procedure with its `Commit`; an actual Undo, Redo, or
jump appends one `HistoryMove` without
creating a history item. A canonical commit away from the active branch tail
reserves and publishes an adjacent `BranchCut` plus `Commit`, retains the
inactive tail outside the normal redo UI, and advances State, Procedure,
JournalEvent, and Branch IDs only at the common publish boundary. `JournalState`
exposes the current/savepoint StateIds, active branch, visible cursor, and
the canonical status of every current commit. The journal can rebuild its
runtime inverse/COW history cache privately from the
Genesis document and canonical procedures; digest and graph validation precede
cache release, and later history movement reconstructs the cache on demand.

This is deliberately not a generic snapshot- or diff-procedure bridge. Every
production history entry references its route-specific canonical procedure,
and there is no supported incomplete-journal state. The v26 writer serializes
Genesis, retained assets, the complete journal/control-event sequence, editor
state, savepoints, cursor, branch graph, and ID authorities. Open validates and
either fully replays that graph or uses a prefix/state/policy-verified optional
checkpoint in a staged Core before one replacement of the live generation.
Checkpoint mismatch selects full replay; malformed/hash/bound failure rejects.
The journal remains authoritative and every non-v26 version is rejected.

History visualization is a read-only derived view of that journal. Core replays
the complete retained graph through the canonical executor, visits only `Commit`
records in `JournalEventId` order (including inactive branches), formats the
typed invocation as a stable primitive name plus bounded argument text, and
renders each committed document state to a maximum 64-by-64 straight-alpha
RGBA8 thumbnail. `HistoryMove`, `BranchCut`, and Genesis do not produce rows.
The derived rows are returned in one immutable Rust-owned ABI handle and are
never persisted as another history authority. Windows creates that handle on
the Core engine thread, owns it in the modeless-dialog controller, lazily copies
rows into an owner-data list, and cancels an unfinished build when the dialog or
document session closes.

## Immutable Genesis and canonical assets

Each Core document has one immutable Genesis state. Genesis owns a stable
Document ID, a distinct stable Cell ID from the same document namespace, and a
typed base surface. `BaseSurface::SolidWhite` is an allocation-free opaque sRGB
white underlay; it contributes to a flattened canonical composite or export but
is not an editable layer or plane and never enters a layer-only export or the
selection mask. `BaseSurface::Asset` instead names one immutable canonical raster
asset whose dimensions and pixel semantics match the document paper. Replacing
the earlier temporary Document-ID-as-Cell bridge and persisting the shooting and
maximum-close frames change canonical document-state bytes. The current document-state
commitment is schema 9/domain 8, the replay contract is epoch 23, and the native
format is version 26. Text/Annotation layers own bounded stable-ID annotation
objects, and `EditAnnotations/canonical-v2` commits create/update/move/delete or
one completed stroke through the shared executor. Cut payload schema 2 separates immutable member assets from
ordered membership and records membership before/after states in Cut history, while
retaining Cell-document primitive semantics. Sequence edits stage bounded ordered
insert/remove/move/renumber operations and publish one Cut revision only after final
validation. Removed members are not physically deleted and remain addressable by
stable `(CellId, document UUID)` while retained Cut history can restore them. The
optional angled shooting frame and stable-ID vanishing points are independent document
objects. Their canonical edits, previews, transform rules, snapshot overlays, output
inclusion policies, and Core-owned radial snapping are persisted by v26; flat normal
output excludes both overlay families, while explicit instruction export may include
the shooting-frame outline and annotations. Epoch 19/version 22 added the independent
current-only Cut descriptor and Cut metadata/
default history. Epoch
18/version 21 added the canonical floating-transform v3 procedure with
half-open five-point absolute-anchor semantics. Epoch 17/version 20 added the
canonical output-color guard selection procedure
over the committed visible straight-alpha composite. The document-owned Color
chart, canonical whole-chart replacement, and EditorState cursor introduced in
epoch 16/version 19 retain their existing semantics.
The numeric audit, prohibited platform-math list, public golden fixture, and
benchmark gate are specified in [`determinism.md`](determinism.md).

The Core-owned asset registry interns canonical descriptors and logical payload
bytes under a content-addressed `AssetId`. A raster descriptor fixes pixel format,
color space, alpha semantics, dimensions, canonical stride, element count, and
payload length before the digest is accepted. Vector and sample streams use their
own closed canonical descriptors. Equal descriptor-plus-payload inputs deduplicate
regardless of source path or codec, while a different format, alpha meaning,
dimension, stride, or logical payload produces a different identity. Encoded
source bytes, file names, paths, timestamps, and optional provenance do not enter
the asset digest or replay input.

Registry retention is semantic rather than a cache of the current materialized
document. Roots include Genesis, every retained journal branch and redo tail,
known persistent editor/optional-metadata references, and live transient owners.
An asset referenced only by an inactive branch therefore remains available for
cache-free replay. Reference accounting and resource bounds are checked before
publication; failure does not partly install an asset, procedure, document, or
retention edge. Closing the owning Core session releases its registry after live
transient users are drained. A checkpoint or the current materialized state alone
is never an asset-retention authority.

Raster open-as-document canonicalizes the decoded pixels before selecting a
Genesis asset base. Import into an existing document, private clipboard payloads,
and Light Table sources pass through the same bounded asset-ingestion boundary.
Stroke samples are always copied into bounded Rust-owned canonical bytes; payloads
larger than 4 MiB are promoted to a canonical sample asset, while smaller payloads
stay inline. Vector document primitives retain typed canonical geometry and
stable target/output IDs. The Windows path/clipboard adapters retain
OS authority only long enough to obtain input bytes; later replay does not reopen
a path or borrow C++ memory.

Cache-free verification first builds a detached asset archive from every semantic
retention root, deep-copies each logical payload, and re-ingests it into an empty
registry with the expected `AssetId`. Fresh Genesis/journal replay uses only that
detached registry, so passing verification cannot be an artifact of shared
`AssetRecord`, payload, or `TileRaster` ownership. Production v26 persists the
same rooted graph in GENS/ASST.

The present ABI is v15. `InkpodObjectId` separates Core, snapshot, task, color,
sample, raster, thumbnail, and export runtime objects by type and Core generation;
IDs are monotonic within one Core and are never accepted across generation or
after release. Variable input is synchronously copied into bounded Rust-owned
objects before a primitive is queued. The canonical `PrimitiveWork` queue record
contains session/generation, issue-time target context, base revision, fixed-width
opcode/schema values, and object IDs only. It contains no caller pointer, closure,
path, callback, or STL object. The established per-operation FFI wrappers and
`inkpod_core_primitive_execute_v3` delegate to their corresponding canonical
Core boundary, so there is one semantic mutation owner.

Snapshot, thumbnail, and export output uses an ID plus bounded record/byte copy;
the caller's storage is borrowed only for that call. Saturation rolls back an
unaccepted sequence, while accepted primitive work is resolved once through
active-stroke deferral, close, and shutdown drain. `PrimitiveWork` remains the
closed value/ID-only primitive lane. Other operations use a fixed `AdapterWork`
record containing issue-time session/generation/context, flags, sequence, and a
bounded input token; callables, optional view updates, and completions stay in a
CoreHost registry and are removed exactly once on the owner thread. No queued
work variant contains a callable, pointer, path, or STL container. V14 normal
save, autosave/recovery, and Batch output all serialize asset-backed Genesis and
every retained asset through the same Core-owned GENS/ASST mapping. Flat common-
raster export remains a separate operation.

## macOS ABI import, Core ownership, and Metal Canvas

The current macOS slice includes the M0 contract/build/ABI-import foundation,
the M1 Core session host, the M2 SwiftUI/AppKit/Metal product vertical, the
M3 command/menu/shortcut/localization vertical, the M4 Sandbox file, recovery,
clipboard, and Finder-drop vertical, the M5 document/tree/multi-view workspace
vertical, the M6 raster paint/fill/color vertical, the M7 selection/
floating-transform/history vertical, and the M8 filter/effect/adjustment/
vector/annotation/frame/vanishing-point vertical, the M9 Cut/Sequence/
Light Table/Subpalette/Reference/motion vertical, the M10 Batch/job vertical,
and the M11 collapsible workspace-chrome vertical.
CMake/source/CI release gates, archive/sign/notary orchestration, and the
native-evidence checklist form the M12 hardening/distribution vertical; they add
no document, renderer, format, or command semantics.
CMake remains the only root build and invokes Cargo before the checked-in Xcode
project:

```text
CMake -> Cargo -> libinkpod_ffi.a -> Clang module -> Swift bridge
                                              -> WindowGroup product app
                                              -> XCTest / headless integration
```

`apps/macos/CoreBridge/C/include/InkpodCoreC.h` includes the canonical
`include/inkpod/core_ffi.h`; it does not copy ABI declarations. Its private
inline accessors expose only C macro constants that Swift's Clang importer
cannot import directly. C ABI types and `OpaquePointer` stay inside the bridge
implementation. SwiftUI receives only value projections containing typed IDs,
generations, revisions, document dimensions, and capability flags.

The M0 ABI smoke creates, queries, snapshots, and destroys one Core on a dedicated
`Foundation.Thread`. Its snapshot is wrapped immediately in an exactly-once
owner and released from a distinct synchronized thread, which is permitted by
the immutable snapshot contract. Core cleanup remains on the creating thread,
including failure paths, and thread-local diagnostics are copied before a
result crosses that boundary.

M1 adds a value-only `CoreHost` facade and a long-lived `Foundation.Thread`.
The thread closure creates a private `CoreOwnerLoop`; its local registry is the
only owner of live Core pointers, and create/view/stroke/history/snapshot/
destroy calls never leave that OS thread. Requests use separate bounded lanes:
4096 normal entries, 4096 pointer-sample entries plus 64 reserved ordered input
boundaries, and 64 control entries for close/request-cancel/shutdown. Normal
stroke cancel shares the ordered input-boundary lane so a rapid begin/cancel
cannot reorder; only emergency cleanup after failed sample admission uses the
control lane.

Session creation first creates the Core, copies Rust-owned editor defaults,
creates one real default Cell through `inkpod_core_new_cell`, and queries the
replay contract. Only after all steps succeed does it consume and publish the
next session ID/generation. Duplicate document UUID creation is a no-op,
invalid and stale generations never retarget another session, and close cancels
a live Core stroke before owner-thread destroy. Shutdown stops admission,
cancels queued completion exactly once, cancels active transients, and destroys
live sessions in stable ID order before the owner thread exits.

M2 adds a `WindowGroup` product app and a MainActor `ApplicationCoordinator`.
The coordinator owns a workspace registry, one `CoreHost`, and one process-wide
`MetalRendererHost`; it has no process-global active-document pointer. Each
`WorkspaceModel` captures its session/view/surface ID and generation at issue
time. Stale completions and snapshots are rejected against that immutable route
instead of resolving to whichever window later becomes active.

```text
MainActor SwiftUI / CanvasHostView
  -> top-left client backing-pixel input
  -> ordered CoreHost request lanes
  -> fixed Core owner Thread / private registry / C ABI
  -> immutable snapshot + captured session/view/surface generations
  -> latest-wins SnapshotOwnershipQueue
  -> dedicated Metal renderer Thread
  -> CAMetalLayer drawable
```

`CanvasHostView` is an `NSViewRepresentable` backed by a custom `CAMetalLayer`.
AppKit points are converted to backing coordinates once, then normalized from
the possibly negative flipped-view backing rectangle to the Core's top-left,
half-open device-pixel space. Mouse and tablet events share the same bounded
pencil sample type; middle-button/scroll pan and magnification update only Core
view state. Escape/responder cancellation ends the transient without changing
document history. Swift never reimplements raster, selection, history, or file
semantics.

The renderer thread owns the Metal device, command queue, pipeline, surfaces,
drawables, and a process-wide tile cache keyed by session, tile ID, and tile
revision. Snapshots transfer to the renderer queue exactly once: rejection,
pending replacement, retained-frame replacement, surface close, and shutdown
each have one release owner. A nil drawable retains the latest immutable
snapshot for a later redraw. Hidden surfaces reject new snapshots and perform
no draw. Pan and zoom rebuild only view transforms and reuse unchanged tile
textures. Display/device change clears GPU-only state and redraws the retained
snapshot; memory pressure purges the tile cache without changing the Core
document raster. The Canvas is SDR sRGB with premultiplied-alpha blending; HDR
is not inferred from the host display.

M3 adds a typed semantic command catalog shared by SwiftUI `Commands`, toolbar,
Canvas context menu, and Settings. Focused menu actions and normalized key
events capture workspace/session/view generations plus document/view revisions
at issue time. The fixed Core thread checks those revisions before mutation;
stale input is rejected instead of retargeting the key window or another
document. A window-number registry contains only typed workspace identity and
generation, never a process-global active Core pointer.
The standard Settings scene supplies Command-comma, while the application
command router invokes the environment's real `OpenSettingsAction` for the
semantic shortcut-edit route; it never reports a presentation without opening
that scene.

Shortcut input maps macOS Command to ABI-v15 `PRIMARY`, Option to `ALTERNATE`,
and physical Control to the separate `CONTROL` bit. Unicode-scalar and named-key
strokes cross `InkpodShortcutStrokeV2`; the thread-independent Rust resolver
classifies 1–4 strokes as none, prefix, or exact. Settings persists the full
versioned table only after prefix-free validation; exact conflicts exchange
bindings, prefix conflicts and persistence failure keep the prior table, and a
1.5-second pending sequence or Escape cancels without routing. A local AppKit
monitor stops custom character and multi-stroke routing for `NSTextInputClient`
focus or marked text. `Localizable.xcstrings` is the single en/ja catalog;
language choice is versioned and applies on the next process launch.

M4 keeps file authority in `FileAccessBroker`. A balanced security-scoped lease
and `NSFileCoordinator` reading or `.forReplacing` accessor enclose each ABI-v15
path call; only a borrowed UTF-8 path survives long enough for that call, and no
bookmark or `URL` enters Core or the journal. For normal save, Core prepares a
current-v26 candidate in Foundation's same-volume item-replacement directory,
the platform publishes it with replace/move while the selected-URL authority is
live, and Core commits path plus both savepoints only after publication succeeds.
Recent files use bounded
app-scoped bookmarks with stale regeneration. A volume/inode/canonical-path
identity registry reserves Open and Save As targets at issue time, rejects a
second owning session, and atomically rebinds the reservation to the inode
published by successful replacement. Open, recovery, and common-raster import
decode into a staged Core on the fixed owner thread, then replace the live slot
only after validation succeeds. Normal save publishes the new Core savepoint
only after same-volume temporary-file flush/close/replace succeeds; autosave,
recovery, export, and compacted copy do not advance normal path authority.

Recovery artifacts live in the app-support `Inkpod/Recovery` directory with a
bounded versioned sidecar containing session generation, document UUID,
original path, and write time. Startup offers every candidate in its own
workspace; malformed metadata is disclosed and retained until explicit
discard. Previous-document restoration is versioned, bounded by the recent
bookmark store, and off by default. The app declares only `.inkpod` plus the
four Core-supported common raster types, and the same classifier gates panels
and Finder drops.

`ClipboardBroker` writes one pasteboard item containing a process-private typed
ticket and standard PNG and TIFF representations. The ticket resolves to a
generation-tagged Rust-owned clipboard object held only in the Core owner
registry; replacement, rejected paste, Cancel, close, and shutdown release it
exactly once. External PNG/TIFF bytes are decoded by ImageIO into copied
straight-RGBA data before Core ingestion. Compatible-plane,
active-plane-converted, and new-raster-plane paste all use Core transactions;
Swift does not reproduce selection, plane, or history semantics.

M5 keeps one Rust Core session per logical Cell while `WorkspaceEditorGraph`
owns value-only frontend views, tabs, and at most two nonrecursive editor
groups. Creating or duplicating a view asks the fixed Core owner thread for a
distinct Core view target; every view retains its own view revision and surface
while sharing the session's document revision, history, dirty state, and
savepoint. Cross-window move accepts the destination before removing the source;
copy creates a Core view first, and reject/failure closes it exactly once. The
application coordinator reference-counts workspace claims on a session and
destroys the Core session only after its final window releases it.

New Cell uses an immutable bounded draft and a Rust-owned plan. The owner thread
prepares one or three Cells, validates every option before publication, and
commits all sessions or none; invalid, Cancel, stale, overflow, allocation, and
frontend-publication failure consume neither persistent nor frontend identity.
Cell properties and Layer/Plane edits are typed requests that execute only in
Core. Swift renders value projections and never implements topology, main/color
plane, image, history, or format rules.

The Layer/Plane inspector follows the active view or pins an exact session/view
identity. A stale pinned view may resolve only to another live view of that same
session; after the session disappears it invalidates with an accessibility
notice and never falls back to a different active document. Workspace layout is
a bounded versioned property-list record containing group orientation/ratio,
tab order, inspector state, and named presets. Restore selects the display with
the largest intersection (then nearest center) and clamps to its visible frame;
malformed records restore defaults. This app-scoped layout is not document,
history, journal, or native-format state. The M5 slice did not add paint panes;
M6, M8, and M9 added their inspectors without changing the Core ownership
boundary. M10 adds a value-keyed Batch `WindowGroup` without adding an active-
document global. Its MainActor model captures the issue-time workspace/session/
generation/revision, freezes one value-only graph before enqueue, and hands that
graph plus balanced security-scoped input/output leases to the fixed Core owner
thread. `BatchJobRegistry` retains the task and leases through completion; window
close, workspace close, and application shutdown cancel and then await that exact
job rather than retargeting another document. The renderer remains uninvolved
except for the existing immutable document snapshots.

All raw Batch graph, run-copy, pair-preview, task, and report pointers remain in
the bridge/owner-thread implementation. Loaded `.inkbatch` operations are copied
to Swift values for explicit per-run configuration, then cloned into a distinct
immutable Rust-owned run graph; the stored graph is never modified. Every accept,
reject, replacement, cancellation, close, and shutdown path has one release owner.
SwiftUI receives only bounded operation, progress, preview, and report projections
and never reproduces document, image, selection, history, or format semantics.

M11 replaces the accumulated horizontal pane columns with one
`WorkspaceChromeView`. The MainActor `WorkspaceModel` owns a value-type
`WorkspaceChromePreference`; toolbar, menu, shortcut, preset, mirror, native
`InspectorCommands`, and scene-restoration binding changes all enter its one pure
reducer. The left tool surface is either hidden or a fixed 54-point icon strip.
Each row has a separate tool-selection button and settings-disclosure button; the
latter anchors one standard SwiftUI popover on the Canvas side. Its transient or
pinned presentation is MainActor-only ephemeral state: transient options close
on outside click or active-tool change, while pinned options remain nonmodal and
retarget to the new active tool. Hiding the tool surface closes either form, and
pin state is never persisted. Tool Options menu and shortcut commands use that
same presentation state.

A separate derived `AdaptiveChromeState` hides the inspector and then the tool
surface when the Canvas would fall below 480 points. Only the explicit chrome
preference is persisted. The app-scoped layout record is version 3; a bounded
version probe selects distinct v1 and v2 migration DTOs or the v3 decoder. A v2
expanded tool surface migrates to the fixed compact strip; malformed and unknown
versions restore the default. This changes neither document persistence nor
replay.

The default trailing edge uses SwiftUI's native single `.inspector`; the mirrored
leading edge hosts the same inspector content and model in a thin
`NSGlassEffectView` fallback. The leading command surface alone uses one
`GlassEffectContainer`. Color, chart, thumbnail, checker, and related judging
content remains in opaque system content wells, and Reduce Transparency replaces
the custom glass background without changing the hierarchy. Hidden inspector
queries retain an issue-time workspace/session/view/generation target, suspend
high-frequency refresh, and reject a stale target on reopening rather than
falling back to the then-active document. Automatic width restoration performs
the same check before publishing visibility; on mismatch it retains the explicit
preference but keeps the runtime inspector hidden until an explicit dismissal or
the captured target becomes current again.

Chrome geometry reduces the actual SwiftUI content region before
`CanvasHostView` lays out, so AppKit bounds, backing-pixel normalization, the Core
viewport, and `CAMetalLayer.drawableSize` continue to describe the same half-open
rectangle. Resize uses the existing surface route. A 200-resize product test
proves bounded tile upload and exactly-once snapshot release without adding Metal
glass composition, snapshot fields, C ABI, or Rust Core semantics.

M6 keeps tool/color state as immutable Swift value projections while the fixed
Core owner thread remains the only owner of editor state, live strokes, fill
transactions, palette/chart codecs, color-replacement previews, locator reads,
and output-color guard tasks. AppKit normalizes mouse/tablet locations to
top-left backing pixels and enqueues begin/sample/end/cancel on the existing
bounded lanes; pressure and tilt are inputs only and no raster algorithm is
implemented in Swift. Each request captures session/view generation plus
document/view/editor revision at issue time. A stale target is rejected and is
never re-resolved to the current tab or another document.

Color controls convert `NSColor` through the device-independent sRGB space and
retain exact RGBA8 or RGBA16 channel values in the bridge projection; display
colors never replace the Core-owned native-depth value. Palette and chart bytes
are bounded caller-owned inputs copied by Core. Rust-owned chart preview and
task objects live only in private owner-thread registries and are released once
on apply, cancel, replacement, session close, or shutdown. Locator follow/pin
uses the same exact pane target policy as M5, and its mouse tracking converts
window/view coordinates through AppKit before the device-to-document Core
query. SwiftUI receives only sampled values and bounded neighborhoods. M6 does
not change public ABI v15, native `.inkpod` v26, replay epoch 23, or procedure
format v26.

M7 routes every selection shape/mode, color operation, adjustment, and
selection-layer conversion through typed requests on the fixed Core owner
thread. Paste retains the Core-owned floating transient while a value-only
Swift draft issues absolute five-anchor transforms; each preview replaces the
same base, Apply performs one Core commit, and Cancel restores the base without
advancing revision or history. Canvas handles use the snapshot transform and
have menu, arrow-key, and `NSAccessibilityCustomAction` alternatives.

Command-Z and Shift-Command-Z replace SwiftUI's standard undo group but invoke
only Core Undo/Redo; no `UndoManager` document history is created. The dynamic
menu uses a bounded value history projection. The History inspector owns a
generation-tagged Core visualization builder/snapshot in the owner-thread
registry, advances it in bounded steps, and copies only 64 visible rows per
request. Replacement, session close, workspace stop, and shutdown each remove
and release that Rust owner exactly once. Branch rows retain Core branch/state/
journal IDs and stale issue-time targets never fall back to another document.
M7 changes neither ABI v15 nor native/replay/procedure versions.

M8 closes every existing filter, effect, adjustment, vector, annotation,
shooting-frame, vanishing-point, and deferred vector-diagnostic route through
value-only requests. A MainActor preview coordinator debounces filter edits for
120 ms, admits at most one running request plus the latest pending request, and
cancels or replaces the Rust-owned task only on the fixed Core owner thread.
Geometry, frame, vanishing-point, and annotation Canvas gestures carry the
issue-time session/view generation and document/view revision; the owner thread
uses Core device-to-document point resolution and rejects stale input instead
of recomputing coordinates or retargeting in Swift.

The immutable snapshot is copied while locked into bounded vector, annotation,
shooting-frame, vanishing-point, radial-guide, diagnostic, and ordered-render
value spans. Raster tile bytes keep their existing borrowed lifetime. The Metal
renderer executes the Core-owned pass order with offscreen ping-pong textures,
premultiplied group opacity, alternate-rule stencil vector fill, vector stroke,
and adjustment LUT passes.
CoreText resolves frontend fonts and fallback only; annotation geometry,
procedure semantics, and normal-versus-instruction export stay in Core. Snapshot
accept, reject, replace, surface close, and renderer shutdown transfer and
release exactly one owner. Renderer allocation, texture, pipeline, or command
encoding failure rejects the frame without publishing partial document state.
M8 changes neither public ABI v15 nor native `.inkpod` v26, replay epoch 23, or
procedure format v26.

The checked-in headless executable proves 64 real Cell sessions use one owner
thread and destroy in order. XCTest fixes success/no-op/invalid/stale/cancel,
ordered input, queue saturation, latest-wins ownership, close/shutdown release,
flipped Retina coordinates, live stroke/Undo, hidden rendering, cache reuse,
device/display/memory recovery, file failure atomicity, duplicate identity,
bookmark regeneration, recovery retention, pasteboard representations, and
clipboard release. The product-scene test runs both normally and with Metal API
Validation; the same Swift suite and headless executable run under Thread
Sanitizer where that extended profile is selected.

CMake fixes the macOS Rust target to `aarch64-apple-darwin` for both Debug and
Release. The Xcode sub-build sets `ARCHS=arm64`, and the Release architecture
gate requires the Rust archive, XCTest executable, headless CoreHost, and
product executable inside a real `Inkpod.xcarchive` to report exactly `arm64`
through `lipo -archs`. Intel Mac is not a supported product target. The
`release` command first runs portable Rust, Swift/ABI/parity, launched-product
accessibility, Metal Validation/ownership soak, Thread Sanitizer, and archive
profiles; it then signs bottom-up with Hardened Runtime and a secure timestamp,
creates and verifies a DMG, waits for an accepted Apple notary result, retrieves
the log, staples and validates the ticket, and runs Gatekeeper assessment. The
release path rejects ad-hoc identity and cannot skip notarization; local ad-hoc
packaging remains a separate `dmg` diagnostic. A source gate keeps the checked-in
entitlement allowlist, evidence tests, CI profiles, and checklist connected.
The separate `publish` command accepts only the already notarized versioned DMG
from a clean release branch synchronized to its remote. It fixes `v<version>` to
the exact HEAD before creating a prerelease, or adds the DMG to an existing
public Release whose tag resolves to that commit. Tag, Release, and upload races
are resolved by re-reading remote state; only the same commit or byte-identical
asset is accepted. Before remote mutation it mounts the DMG read-only and checks
the enclosed app signature, identity, version/build, arm64 executable, and exact
entitlements. Normal publication never deletes or overwrites a same-name
different asset. The explicit development-only `publish --force` route keeps
those preflights but may retarget an existing prerelease tag to exact HEAD with
a remote-object `force-with-lease` and replace only its same-name macOS DMG with
`--clobber`; stable Releases are rejected, a tag race restores the prior local
tag, and the resulting asset bytes and tag are re-read after mutation.

The checked-in parity ledger still classifies all 384 Windows
production command IDs. M3 marks the 19 deferred M2 View rows and 11 M3
application/help/shortcut/language rows implemented; M4 marks exactly 26 file,
clipboard, and close rows; M5 marks exactly 67 document/tree/view/workspace
rows; M6 marks 49 paint/fill/color rows plus one native-surface
`notApplicable` Locator AutoHide row; M7 marks exactly 31 selection/transform/
history rows; M8 marks exactly 70 filter/effect/adjustment/vector/annotation/
frame/guide/diagnostic rows; M9 marks 56 Cut/Sequence/Light Table/Subpalette/
Reference/Motion rows plus three native-surface `notApplicable` AutoHide rows.
M10 marks 50 Batch/job rows as `macEquivalent` and the Windows-only Batch AutoHide
row as native-surface `notApplicable`. All 384 implemented rows have a complete
route/state/surface/test chain; there are no planned rows or placeholder menu,
button, pane, or shortcut surfaces.

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
one `CoreHost`, one `RendererHost`, a bounded multi-entry workspace registry, and a
bounded multi-entry document registry. The UI exposes those document entries
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

Rust Core owns two distinct editor-state layers. Immutable `EditorDefaults`
exists before any document and supplies the built-in initial document spec and
initial editor values; creating a document explicitly copies those values into
its Genesis/session `EditorState`. The defaults are not an application
preference and are never made authoritative by a workspace. Each Core entry
bound to a `DocumentSession` then owns exactly one mutable `EditorState`.
Multiple views of that session share it, while different document sessions
remain isolated even when their views appear in the same workspace. Palette
content, Color chart entries/name/lock, main-line color, and the selection mask
remain document primitives; only the palette cursor, Color chart page/selection,
active layer/plane target, ordered multi-edit-target
set, and selection/tool/brush options belong to `EditorState`. The active target owns
keyboard focus and paint destination; the bounded target set independently owns
grouped copy/tree-command intent. Core normalizes that set to document-tree
order, reconciles it after topology changes, and persists it in EDIT schema 6.
Changing the marker set advances only EditorRevision/editor dirty. A grouped
document command captures the set into one canonical invocation and publishes
one transaction, document revision, history entry, and journal commit.

Raster selection construction is likewise Core-owned. Windows captures the
stable editor target, gesture points/pressure, view zoom, range interpretation,
aspect/center/rotation constraint, and trace options into one ABI request. Core
canonicalizes that request, generates one binary candidate through the shared
geometry/content interpreter, then feeds the existing New/Add/Subtract/Intersect
algebra. The content interpreter is an iterative bounded 4-connected traversal;
the C++ preview draws only the immutable normalized gesture outline and never
creates an alternative document mask or mutation path.

New Cell creation has a separate immutable planning boundary. The runtime
adapter converts the size-prefixed C record once and asks Rust
`plan_cell_creation` for a bounded plan before it creates any Core session or
stable document object. The standard-control dialog receives only typed initial
values plus a preview callback; the callback copies the first item from that
same Rust-owned plan, so the dialog module neither calls `CoreHost` nor
reimplements physical/pixel, rounding, frame, anchor, topology, or depth rules.
The plan contains no document UUID or stable ID and can be released without
changing any Core.

For multiple cells, the UI adapter preflights the focused group/session capacity
and creates all UUID and recovery identities. `ApplicationHost` then reserves
bounded routing bindings while `CoreHost` creates every Core and applies one plan
item to each on the owner thread. These prepared Core sessions are not yet in the
`DocumentRegistry` or the active EditorGroup and therefore have no tab. Only
after all Core work succeeds does `ApplicationHost` publish the complete prefix
to the frontend registries and activate the last view. A failed preparation,
identity, Core commit, frontend publication, presentation, or Fit step discards
unpublished preparations, closes any published prefix, and restores the prior
active view before the command returns; recent files are never touched. Untitled
display numbers remain frontend presentation values and are not Cell IDs or
future sequence numbers.

Cut ownership is deliberately separate from that Cell graph. Each
`WorkspaceWindow` has at most one `CutSession`; it owns one opaque `InkpodCut*`,
the descriptor path, and presentation-only cached Cut name/member file names.
The handle is created, queried, edited, saved, opened, recovered, and destroyed
only through `CoreHost::Invoke` on the Core engine thread. It has its own
revision, canonical metadata/defaults history, dirty state, and savepoint.
`DocumentSession` continues to own each member Cell Core and file/recovery shell,
so a Cut edit cannot enter Cell history and a Cell edit cannot enter Cut history.
Workspace close and CoreHost shutdown destroy every Cut handle before the Core
engine thread stops; no process-global active Cut pointer exists.

The macOS M9 adapter keeps the same split with a MainActor `WorkspaceModel`, a
value-only `CoreCutProjection`, and private Cut/Core registries on the fixed
`Foundation.Thread`. New Cut creates and saves each planned Cell as its own
same-directory `.inkpod`, then saves the relative Cut descriptor. Open and
recovery stage every missing member, validate its exact `CellId` and document
UUID, reserve file identity, and publish the Cut and tabs only after the whole
set succeeds. Failure closes every staged handle and leaves the live workspace
unchanged. MainActor never receives an ABI pointer or mutable Core state.

For Cut playback and thumbnails, the owner thread exports bounded encoded
rasters from the member Cores and imports them through the identified mixed
sequence ABI. Source UUID and generation therefore remain the live Cell's
document identity and document revision without Swift pixel decoding. Direct
selection and endpoint navigation activate the already-open matching member
session; a stale Cut/session/member/sequence revision is rejected instead of
opening a duplicate document or falling back to the active tab. Motion timers
remain MainActor presentation state and issue the same exact-target sequence
commands; they do not mutate Core motion or document state themselves.

The selected persistence topology is an individually referenced descriptor.
The Cut `.inkpod` stores ordered `(CellId, document UUID, display number,
relative file name)` records; every member is an independently saved Cell
`.inkpod` in the descriptor directory. New Cut first uses the existing bounded
Cell creation plan, publishes and saves each independent Cell, then creates and
atomically saves the descriptor from those returned identities. This is an
explicit multi-step boundary, not a fabricated cross-file transaction. Cut
defaults are copied into the creation request only at that boundary; changing
defaults later never mutates an existing Cell. Open/recovery decodes into a
staged Cut, canonicalizes the descriptor directory and every member, opens each
Cell with the current Cell reader, and compares both stable identities before
publishing the Cut handle. Missing, renamed, duplicate, self, traversal, or
directory-escaping members reject the staged Cut without retargeting any live
session.

`CoreHost` queries the selected session/generation on the Core owner thread and
copies an `InkpodEditorStateInfo` presentation record into the matching
`DocumentSession`. `WorkspaceWindow` and tool controls may project that copy,
keyed by session, generation, editor revision, and digest, but do not own
authoritative tool, color, fill, selection, vector, or brush defaults. Document, view,
or workspace switching therefore refreshes from the target Core and never
writes a previous workspace value into it. Commands, strokes, and previews
capture the exact-depth color, diameter/options, brush shape, smoothing,
start-color predicate, and stable target IDs at begin;
their canonical arguments do not consult later `EditorState` changes.

Brush smoothing is performed in Core on canonical document Q16.16 coordinates:
the first sample is unchanged and each later axis is ties-to-even division of
`previous * s + raw * (1001 - s)` by 1001 for `s` in 0..1000. The start-color
predicate samples the immutable pre-stroke target plane at the first raw sample
and compares native channels including alpha exactly. It does not impose
connectivity; selection clipping and the reached round or square footprint are
independent gates. These values are persisted in `ApplyRasterStroke` schema 3,
so live commit, Undo/Redo, reopen, and replay share one executor.

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
The metrics include accepted/rejected work, pending high-water mark, and total
and maximum queue wait measured from enqueue to owner-thread dispatch. A
session/generation value must be supplied when reading them; the UI never
re-resolves the active document. The read-only C ABI resource query reports
logical document tile/history, render-cache, CPU-staging, light-table/reference,
sequence-source, and thumbnail-cache categories without building a snapshot.
`ApplicationHost` owns one UI-thread-only layer/sequence `ThumbnailCache` for
all workspaces. Its 64 MiB default and 256 MiB maximum application budget use a
cross-workspace LRU. Keys contain pane instance, document session/generation,
content ID/revision, and kind; dialogs retain only keys and borrow pixel spans
for the duration of one UI-thread draw. Pane/document/workspace close removes
the corresponding entries. ApplicationHost reports resident bytes/item counts
by captured pane ID and reports color-picker CPU cache bytes separately.
Close marks a session non-accepting before its ordered close item, resolves all
previously accepted work, cancels a live stroke, and destroys the handle on the
owner thread. Long operations still share this single lane and may delay other
sessions. The cross-session delay and fault scenarios retain every
accepted input without partial commit, so current measurements do not justify
relaxing the single-writer owner-thread contract.

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
revision, and view revision. Snapshot fan-out registers every visible
editor-group Canvas, including the primary group of each newly created
workspace, up to 16 sinks matching eight workspaces with two groups each;
inactive tabs and auxiliary panes do not consume that bound. RendererHost accepts a
snapshot only when the complete
route equals the current surface binding and the snapshot accessors confirm both
revisions. Rebind clears the old retained snapshot before accepting the new
route. Stale, hidden, occluded, queue-full, replaced, closed, and shutdown paths
all consume the Rust snapshot owner exactly once. Device loss first discards all
surface GPU resources, recreates the shared device, then reconstructs every
surface cache from its retained immutable snapshot; Core document state is not
involved. Renderer telemetry is published as a value copy and accounts for
retained/pending immutable snapshots, active/cached GPU tiles, swap-chain
payloads, surfaces, queue rejection/replacement, stale frames, resource-limit
rejection, and device reset. A per-surface value copy retains the bound document/
view/Canvas/generation route and byte/count values but no snapshot or GPU
pointer. GPU tile payloads share a 512 MiB application-wide
budget; active tiles are admitted only when the aggregate fits, while inactive
tiles are retained for reuse and evicted in application-wide least-recently-used
order. The frontend retains one Canvas surface per visible editor group, up to two,
rather than per open or inactive tab. Closing a group or non-final workspace
atomically unregisters every affected snapshot sink through the same
`CoreHost` publication lock before destroying its Canvas. The deterministic
application resource snapshot reports one registered sink per visible editor
group, and group close moves its views to the surviving group.

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

`ApplicationHost` also owns the single process-wide `TabDragCoordinator` on the
UI/Input thread. The Common Controls tab subclass gives it only strong value IDs,
generation, operation, original index, and a pointer-free restore context; no
`HWND`, C++ object pointer, or Rust owner is posted through a window message. Tab
capture and the native item drag image remain UI-thread resources. Placement is
deferred until button release, when source and target still exist and the
captured route, index, capacity, active-stroke, modal/effect, and capture rules
are revalidated. Reorder, group/window transfer, new-view copy, and tear-out use
the same application operations as menu, context-menu, and keyboard entry
points. Cancellation changes no model state; a failed transfer restores both
`EditorArea` values, target routing, Canvas bindings, and the prior active
context. Only view placement moves: `DocumentSession`, Core handle, document
revision, history, dirty state, and savepoint remain with their existing owners.

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
pointers in custom messages. Each workspace exposes one or two editor
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

The fixed command-state catalog assigns all 384 production commands exactly one
state owner. Pure providers compute enabled/checked state without calling Core or
Win32 or mutating tools, previews, or documents. Menus, shortcuts, and palette
entry points consume the same cached result. The main frame deliberately has no
toolbar; every user command remains reachable through a menu leaf.

The UI thread owns a fixed-capacity `PaneTargetRegistry` beside the target
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

For a Cut descriptor, the pane keeps a derived thumbnail cache keyed by the
member's stable `(CellId, document UUID)` pair rather than its order or display
number. A cache miss opens that same-directory Cell in a staged temporary Core,
revalidates the identity, and requests one bounded visible-document thumbnail;
the bytes are never persisted in the Cut descriptor. Reorder, renumber, Cut
Undo/Redo, and save/reopen therefore preserve the thumbnail-to-member binding,
while an invalid or mismatched source never substitutes pixels from another Cell.

Normal previous/next navigation has a separate application-level endpoint
policy. `SequenceEndpointPolicyV1` is a versioned, bounded HKCU record shared by
every workspace window in the process; missing, malformed, and noncurrent input
falls back to `Stop`. `Stop` and `Wrap` are closed values and are independent of
motion-check loop state. The menu checked state, configurable shortcut, status,
and accessibility presentation all read the same `AppLifetimeState` value, so
changing it never changes document/editor revision, history, dirty state,
savepoint, replay, or native file schema.

Core resolves every relative command into an immutable `SequenceStepPlan` with
an explicit empty, single-cell, stopped, advanced, or wrapped result. Nonempty
plans capture sequence revision and the source/target UUID, source generation,
natural-order index, and parsed cell number. Commit re-resolves the captured
direction and endpoint policy and rejects stale identity or revision atomically.
The Windows adapter uses the Core-provided target for both prompt and autosave
routes; it does not derive endpoint behavior from pane indices.

Sequence navigation also has one application-level, versioned `Prompt` or
`Autosave-before-switch` dirty-cell policy. The autosave route first asks Core for an
immutable value request containing the exact source/target UUIDs, sequence-source
generations, and source document/editor revisions. `DocumentSession` owns a
bounded registry keyed by source UUID plus generation; each published entry owns
one private native recovery path, sidecar metadata, and a monotonic artifact
generation. The UI pre-reserves registry capacity, then the CoreHost owner lane
writes and durably replaces the source artifact before it commits the requested
activation. A target with an existing association is decoded, validated, and
replayed in a staged Core, then swaps the live Core only after its UUID matches.
The flattened `SequenceCellSource` remains a thumbnail/fresh-cell source and is
never used to reconstruct saved layer topology, history, selection, or editor
state.

Only one sequence switch token may be pending application-wide. While it is
pending, previous/next/goto are disabled, progress is visible in the status bar,
and another request is rejected rather than retargeted. Completion carries only
token and frontend generation through `PostMessage`; the UI publishes the
pre-reserved association, active cell, pathless recovery shell state, panes, and
menu state only for the captured live session. Save, metadata, queue, or stale
failure leaves the source cell active and never advances the normal path or
savepoint; close or shutdown invalidates and discards that session without
retargeting completion to another document. Both HKCU policy records are
versioned frontend state, not part of the `.inkpod` schema.

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
`DockLayoutModel`. Its `PaneDescriptor` records give every surface a stable type
ID and localized resource title, default and allowed zones, target scope,
multiplicity, float/auto-hide capability, minimum/preferred sizes, and whether
an inspector header remains visible for a singleton stack. The compact Tool and
Tool Options surfaces opt out of singleton headers. The model permits only
`TopContext`, `Left`, `Right`,
and `Bottom` stacks around the central `EditorArea`, plus floating and hidden
placements. A stack is either tabbed or split in one direction; recursive/free
dock trees cannot be represented. At the 96-DPI reset baseline, active-tool
options occupy the top 40 DIP, the 80-DIP tool strip is left, and the 320-DIP
color/layer stack is right at 32:68. The layer pane retains its internal 55:45
layer/plane split. Its shared action row has a resource-backed label naming the
currently active Layer or Plane target, and each button exposes the same target
in its accessible name. Tool buttons remain 72 x 34 DIP with meaningful
single-word Japanese labels, forward the menu command IDs, and consume the same
cached command state.

`WorkspaceWindow` owns one UI-thread `DockHost`, which applies the pure geometry
to the existing primary pane child windows. Docked content is parented to the
main frame; floating content is reparented into an ordinary main-window-owned
top-level frame and returns to the same child HWND when docked. Drag preview is
limited to descriptor-allowed zones. Resource-titled standard tab controls stay
visible for singleton inspector stacks so the content and action scope remain
identifiable. Mouse/keyboard splitters retain a 4-DIP hit target and paint a
centered system-color rule, highlighted on hover, capture, or keyboard focus;
the internal Layer/Plane splitter follows the same presentation and accessibility
rules. Pane context menus and the Window menu provide tab/split, dock, float,
hide, restore, and reset without retargeting a document command. Floating close
maps to hide, preserving the pane's controller state. All HWND and Common
Controls activity remains on the UI/Input thread; Core and renderer ownership is
unchanged.

The frontend persists a bounded version 7 workspace record in HKCU. It contains only main
window placement, editor split orientation/ratio, dock zones/order/ratios,
primary and secondary pane visibility/size/floating placement, AutoHide edge,
density, and selected or user-named preset; document paths and document/Core
identities are excluded. The decoder validates its exact size, counts, enums,
stable pane IDs, duplicate IDs, placement bounds, and bounded terminated name.
Unknown pane IDs are ignored, absent known panes retain current defaults, and an
invalid or unsupported record restores the default without aborting startup.
Supported version 4, 5, and 6 records migrate once to version 7.

The UI thread owns a fixed-capacity `WorkspaceWindowRegistry`. Each heap-stable
`WorkspaceWindow` owns its top-level
and child HWNDs, menu/status presentation, `DockHost`, `EditorArea`, pane instances,
focus history, and persistence slot. The application continues to share its
`DocumentRegistry`, `CoreHost`, `RendererHost`, clipboard, shortcuts, and job
registries. One `DocumentSession` therefore keeps one Core binding when views are
moved or duplicated across windows; only `DocumentView` presentation and the one
Canvas per visible editor group are distinct. Window procedures resolve their
instance from `GWLP_USERDATA`, and the message loop enumerates the registry for
modeless dialog and keyboard processing.

Window/view commands capture a pointer-free `CommandContext` before routing.
Cross-window move or duplication validates workspace, group, view, session, and
generation namespaces and never re-resolves a later active document. Closing a
window prompts only for dirty sessions whose final view is being removed; shared
sessions and surviving windows remain registered. Before a non-final window is
destroyed, `CoreHost` synchronously retargets its notification owner to a
surviving top-level window and reposts every pending value token; duplicate old
messages are harmless because token consumption is single-shot. Only closing the
final workspace posts quit. Shutdown unbinds every Canvas, rejects stale
notifications, detaches and stops Core on its owner thread, stops the renderer
on its thread, and only then destroys remaining workspace HWND ownership.

One process-lifetime `ActivationService` is owned by `ApplicationHost`. Before
Common Controls, COM, CoreHost, RendererHost, or workspace creation, a
current-user/logon-session named mutex selects the primary. A secondary serializes
only validated bounded UTF-8 paths, version/size, request ID, open mode, and target
preference to a local named pipe whose ACL permits the current SID and SYSTEM and
rejects remote clients. The IPC thread owns no HWND or Core handle; it copies the
request into a bounded queue and posts only a 64-bit value token split across
`WPARAM`/`LPARAM`. The UI thread takes the token once, fixes the last-focused
workspace/active group or explicit new workspace, and calls the normal identity-
aware open route. Duplicate request IDs are idempotent. Queue/post failure removes
the unpublished request, shutdown rejects new requests, and a bounded client
timeout never creates an independent editor. Application shutdown stops activation
before saving optional previous-document paths and stopping CoreHost.

The five built-in presets are Coloring, Line Cleanup, Reference Check, Batch,
and Focus. Save, Save As, Restore, and Reset share the normal command/state/
shortcut catalog. Secondary palettes use resource-titled standard-button
AutoHide edge strips, keeping keyboard and accessibility behavior in Common
Controls. Workspace navigation is intercepted before the configurable command
catalog: Ctrl+Tab/Ctrl+Shift+Tab select tabs, Ctrl+F6/Ctrl+Shift+F6 select an
editor group, F6/Shift+F6 cycle menu, dock, editor, and status, and Ctrl+F4
closes the captured view. Edit controls retain all other text input. Standard
tab, static, dialog, and button controls expose dirty, target, job, pane, and
AutoHide names through the Windows MSAA/UI Automation bridge; captionless
splitters receive explicit accessible names. Main and floating placements are
captured in physical screen pixels
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
generations. Each accepted raster-stroke packet advances the pointer generation
after that stroke work enters the Core queue; pending requests are coalesced,
while End and Cancel always invalidate and resample the final state. Core reads
the same display-document priority as snapshots (active stroke preview, then
filter preview, then committed document), without publishing a document or
history change. Secondary palette presentation that is not yet exposed is
tracked in `implementation-status.md`, while its Core/C ABI models remain owned
here.

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

The viewport background and document paper are separate renderer passes. The
renderer clears the complete backing-pixel viewport to the neutral Canvas color,
then fills only the transformed half-open document rectangle with the snapshot's
paper, color-check, or bounded transparency-checker presentation. Adjustment LUTs
operate on that document rectangle rather than recoloring the surrounding Canvas.
This keeps 1:1 defined as one document pixel per backing pixel on Retina displays
without presenting out-of-document pixels as editable paper.

Core keeps document points/sizes/rectangles, device points/sizes/offsets, and
zoom as distinct private types. Public Rust commands and state accessors retain
their established scalar/record shapes, and the legacy C records retain their
fixed layout beside the additive runtime-ID records; those boundaries validate and convert before calling the typed
`ViewTransform`. Locator sampling, guide/grid snapping, and stroke/effect input
use document points after their single `CoordinateSpace` conversion. Snapshot
raster origins/sizes are likewise typed internally, while raster, vector, and
overlay output coordinates remain document-space public records.

Geometry gestures use the same boundary explicitly: Windows downsamples a
bounded device-pixel span and passes the procedure-captured Core view ID,
expected view revision, and temporary Ctrl-bypass bit to
`Core::resolve_geometry_points_for_view`. Core alone converts, clamps, and
applies grid/guide precedence. A stale or closed view fails the gesture instead
of being rebound to the currently active view. The resolved document points
feed the existing geometry preview/canonical executor, so snapping itself is a
read-only query and does not add a second procedure or history unit.

View flips are snapshot transform flags around the document extent and do not
change document pixels or history. Destructive mirror transforms raster,
selection, frame, and guide state in one Core transaction. Raster hit
quantization uses the same half-open pixel-cell rule as locator sampling and
nearest-neighbor rendering, including magnified lower/right clicks, final edges,
and flipped views.

Immutable snapshots own raster tiles, overlays, flattened cubic segments with
explicit endpoint-connection bits, vector fills, packed boundary path IDs, and
bounded annotation records backed by snapshot-owned UTF-8 and milli-pixel point
pools.
The Rust snapshot adapter derives the target view's bounded disconnected
endpoint records once, and the FFI snapshot handle owns that borrowed span.
Borrowed spans remain valid only for the FFI snapshot lifetime. Endpoint
identity is the Rust-owned explicit stable path/start-or-end topology;
coordinate equality and proximity are not connection evidence. Antialias,
centerline overlay/only, and endpoint visibility remain view-local and advance
only view revision. The renderer validates counts, strides, flag consistency,
endpoint order, and path grouping, creates D2D geometry, sizes centerlines and
markers in device pixels, and reconstructs GPU resources from the retained
snapshot after device loss. Core geometry remains in document coordinates;
zoom, pan, and flip are render transforms only.

Snapshots also own a bottom-to-top render plan. Layer/plane index 0 is the
palette top, so Core walks each tree in reverse and emits closed layer groups,
raster-tile spans, vector-fill spans, vector-stroke spans, annotation spans, and adjustment LUT
references in one semantic order. Layer opacity is applied once by the group;
plane opacity is already resolved into its raster or vector records. The C ABI
publishes only borrowed bounded spans, and the renderer rejects malformed group
structure, kinds, or item ranges before retaining the snapshot. Direct2D draws
the plan sequentially. An adjustment boundary closes the current command list,
applies the Core-resolved RGB lookup tables through TableTransfer, and continues
in a new command list, preserving editable vector geometry and document-scale
quality. DirectWrite font resolution and text-format caching stay on the renderer
thread; a missing annotation family falls back to Segoe UI with a visible Canvas
warning. Device-loss recovery rebuilds the same plan from the retained snapshot.
Thumbnail and flat export use the same bottom-to-top Core semantics and the
Core-owned deterministic portable annotation rasterizer; instruction-output
objects are filtered only from ordinary flat export. Raster-only
documents retain the existing precomposed tile path and its revision-max cache.

### Canonical revision-max render-cache identity

The render-cache source identity uses `revision-max` as its canonical
performance contract. For each document tile coordinate,
Core computes one scalar value as the maximum of all visible plane
`tile_revision` values at that coordinate, the selection tile revision, and the
Light Table source revision. Cache validation compares that value with the
private `RenderTile.source_revision`. A match reuses the composed pixel buffer
and renderer-facing tile revision; a mismatch composes only that coordinate and
publishes a new tile revision. An all-transparent composition is not retained in
the cache and may therefore be recomposed if the coordinate remains in the
snapshot candidate set.

This validation path reads only fixed-width revision scalars. It does not call
`tile_data()`, copy or scan source pixels, compute a payload hash, or maintain a
content digest, clone generation, deletion tombstone, epoch, or negative cache.
Its cost is proportional to the number of visible plane sources rather than the
number of source bytes. Zoom and pan therefore update the view transform and
reuse unchanged composed tiles without making snapshot construction scale with
the raster payload size.

The regression boundary is executable. A normalized source lock covers the
complete primary `build_snapshot` validation body together with the
revision-max helper, so adding a delegated validation helper also requires an
explicit audit. Forbidden-token checks reject direct tile/pixel/hash/digest
access in both bodies. Test-only counters at the sanctioned composition payload
access sites prove that the initial compose reads payload and that 128
subsequent cache-hit wheel-style zoom snapshots perform zero payload accesses.

`source_revision` is private cache bookkeeping. `RenderTile` semantic equality
deliberately excludes it, and it is not exposed through the C ABI or included in
canonical document/procedure digests or persistence. Opacity, visibility, layer
order, main-line color, color-check mode, and other render metadata outside the
revision-max formula rely on the owning edit path's existing atomic whole-cache
invalidation.

The scalar maximum is not a collision-free description of source state. A high
Light Table source revision can mask a later raster edit with a lower numeric
revision. If two visible plane tiles share the maximum revision, deleting one
can leave the maximum unchanged and reuse an obsolete composition. The same
numeric alias can occur between independent plane and selection revision
domains. Because display mode is absent from the formula, primary and secondary
views with different alpha modes can also reuse the shared cache incorrectly
after the first mode switch. Transparent results have no negative cache and can
be recomposed. These are intentional, documented constraints of choosing the
revision-max performance baseline as canonical; they are not described as
correctness fixes. This runtime policy is independent of the native file format,
is not serialized, and is not changed by a format-version update.

#### Rationale and change control

The renderer keeps cache validation separate from document-state commitment,
borrows changed source tiles without copying, and prepares each dirty-tile
composition once. The adoption history and original A/B results are summarized
in [`legacy.md`](legacy.md); the current executable workloads, environment
envelope, and exceptional audit procedure are in
[`core-benchmark-baseline.md`](core-benchmark-baseline.md).

The decision favors byte-independent, fixed-scalar validation and minimal cache
bookkeeping over a collision-free source fingerprint. A key containing every
source identity, revision, display mode, generation, deletion tombstone, and
negative-cache state could distinguish more transitions, but would increase key
construction, storage, invalidation, and audit complexity. A pixel or semantic
digest would be stronger still but is explicitly rejected on this hot path
because it can make validation scale with payload or commitment work. The
accepted costs of revision-max are the aliasing and transparent-recomposition
limits above plus strict whole-cache invalidation for metadata outside the
formula. Changing this trade-off requires an explicit product decision and
recalibration; it is not an incidental renderer refactor.

## Revision, preview, and transaction model

Document, editor, and view revisions are independent. Successful document
edits, history movement, new, and open advance document revision. Pan, zoom,
fit, viewport resize, view flip, and other semantic view changes advance only
view revision. A semantic `EditorState` update advances only the nonzero
`EditorRevision` and recomputes the domain-separated canonical
`EditorStateDigest`; the revision, editor savepoint, and digest field itself are
not digest inputs. A semantic no-op changes neither token nor dirty state.
Invalid, stale, failed, or overflowing updates publish no partial state.

Editor updates do not advance document revision, `StateId`, procedure journal,
Undo history, document savepoint, or render content. The editor savepoint is an
`EditorStateDigest`, and Core reports editor dirty by comparing the current
digest with that savepoint. Session dirty is exactly `document_dirty ||
editor_dirty`. A topology-changing document transaction revalidates the active
layer/plane stable-ID pair and resolves an invalid target deterministically in
Core; the frontend does not substitute an active target. Color, diameter, and
target-only changes therefore cannot add an Undo entry.

Core-owned identities use distinct internal newtypes for documents, layers,
planes, vector paths/fills, light-table sets/items, and secondary views. History
state plus document, view, render-cache, and preview revisions are separate
tokens with their own increment policy. A typed cursor allocates the one
document-wide stable-ID namespace through domain-specific methods; there is no
conversion between identity domains. Public Rust records, C records, runtime IDs, and
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
All public document-mutation wrappers construct a typed primitive request or
canonical invocation and delegate validation, canonicalization, working-state
mutation, no-op detection, and commit to the one canonical boundary. Their
canonical procedures retain language-neutral history entry kinds and the existing cache policy, and
replay uses the same route rather than a second pixel, geometry, topology, or
metadata implementation. For every committed edit, document, StateId,
visible history, journal events, document revision, cache invalidation, and all
persistent high-watermarks publish together. A new edit after history movement
removes the old tail from normal Redo while retaining it as an inactive journal
branch. The normal savepoint is a `StateId`, so dirty state follows Undo, Redo,
jump, and branching independently of file timestamps.

Sparse tile allocations remain shared through copy-on-write, and inverse
history changes are an optional runtime cache. Core validates the canonical
journal against the live semantic digest, can release that cache, and
reconstructs it before the next history move. Production has no transition to
an incomplete journal; a history-producing commit without its procedure is an
invalid internal state rather than a supported fallback.

Preview/session, floating-selection, cancellable Batch/effect, external reload,
and potentially long-running raster/vector conversion paths retain their
specialized staging ownership. Their completed candidate state passes through
the same stale-checked atomic publish boundary; cancel or failure drops the
candidate without changing committed document, history, revision, or cache.

- Stroke begin/append changes only a preview document. Snapshots may show that
  preview while committed revision, dirty, savepoint, and history remain fixed.
  Begin and append canonicalize the bounded document-coordinate sample sequence
  into one lifetime-independent inline payload; end submits that owned payload
  to the canonical executor and commits at most one history entry. Cancel,
  capture loss, or failure restores the exact base state.
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

The current `.inkpod` v26 Cell container requires `META`, `GENS`, `ASST`, `PROC`, and
`EDIT`. Save first verifies cache-free journal replay, encodes prospective
document/editor savepoints, and streams the complete validated container to an
exclusive same-directory temporary file. Header, records, asset chunks,
procedure payloads, and directory are streamed without a second complete file
allocation. Only successful flush, sync, close,
and replacement publish the path and both live savepoints. Open validates all
container/section/record digests, assets, typed invocation bytes, references,
branch/cursor relationships, high-watermarks, and final document/editor
digests in a staged Core, then swaps once and rebases `DocumentRevision` to 1.
Normal-save output therefore reopens clean with Undo/Redo and inactive branches
intact. Autosave retains the existing normal path/savepoints; recovery open
clears both savepoints and path authority and marks the restored session dirty.
Partial selection revert reconstructs the saved document through this same v26
reader and commits the selected delta as one new canonical undo unit.

Checkpoint policy is deterministic over procedure count, replay work, and dirty
bytes. A materialized checkpoint is an optimization only: inactive branches and
all retained assets remain rooted by the journal. Explicit compaction first
returns the history-event/procedure counts and document/editor/journal digests
as a confirmation token, then writes a separate new-Genesis file only if that
token is still exact. It never auto-squashes or mutates/adopts the live session.
The Windows File menu obtains that token through `CoreHost`, displays both loss
counts before showing the save dialog, rejects a path belonging to any open
session, and reports success without changing current path, dirty state, or
history.

Each successful autosave is paired with an atomically replaced, current-version,
bounded metadata sidecar containing `DocumentSessionId`, generation, document UUID,
original file identity/path, source path, and write time. Startup enumerates every
bounded recovery artifact instead of selecting one newest file; missing or malformed
metadata never causes silent deletion, and restore/discard/defer is per candidate.
Opening recovery preserves its original identity namespace and retains the artifact
until a normal save explicitly removes the recovery and sidecar. Autosave does not
advance the normal savepoint. Workspace layout never contains document paths. A
separate bounded current-version HKCU path record is read and written only when the
default-off `起動時に前回の文書を復元` setting is enabled; crash recovery remains
independent of that privacy choice.

## Build, portability, and verification boundaries

CMake is the build entry. Configure-aware recursive Rust source tracking feeds a
profile-specific Cargo completion stamp and declares the staticlib/rlib
byproducts, so unchanged builds do not rerun Cargo. Windows presets validate x64
or ARM64 compiler targets, align Cargo `+crt-static` and MSVC `/MT` profiles,
reject dynamic CRT imports in the final executable, assemble the matching MSIX
and four-file portable ZIP payloads, and run strict C11/C++20/Rust tests.

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
fixed quick/full inputs and semantic counters/checksums. Routine wall-clock
acceptance for protected `pan_zoom_snapshot` and `dirty_tile_rebuild` scenarios
uses a matching approved environment envelope from
[`core-benchmark-baseline.md`](core-benchmark-baseline.md): after warm-up, compare at least five-run
medians, and require a second independent five-run median before confirming an
upper-edge regression. Reconstruct the old build only for recalibration or an
explicit audit. Other scenarios use the documented same-machine review rule;
semantic drift and resource-budget failures remain unconditional on every machine.

The private Windows `--performance-smoke-test` is the native companion gate. It
materializes all 256 tiles of a 1024-square raster, sends 256 alternating wheel
pairs through the real Canvas/UI/CoreHost route, and serializes each event to
exactly one renderer-thread GPU update and successful Present. It then sends 16
multi-sample strokes that each traverse 16 tiles; each stroke is committed and
rendered to exactly one final Present, for 544 input samples total. The renderer
pause is a smoke-only synchronization barrier around enqueue, not a production
render mode. It fixes the amount of GPU/Present work so queue coalescing cannot
make one binary appear faster merely by presenting fewer frames. The idle side
of that barrier is satisfied only when the queue is empty, the renderer
in-flight work count is zero, and the last dequeued item's GPU update/Present
path has returned; queue removal alone is not completion. A regression test
requires an idle wait to observe exactly as many new Presents as queued render
requests. Document/view revisions, checksums, completed samples/strokes, tile
bytes, Present counts, queue rejection, and resource-limit counters are hard
assertions. Native routine elapsed medians use the matching approved environment
envelope and the same two-batch upper-edge rule as the protected Core workloads.
Wheel elapsed time is normalized by the recorded display refresh interval; Core
`pan_zoom_snapshot` remains the CPU-sensitive zoom gate. Old/candidate
alternating-order comparison is reserved for recalibration or explicit audit.

## Initialization and shutdown

Application initializes Common Controls, COM, frontend owners, and RendererHost;
it then creates the main window/Canvas surface and starts CoreHost. Core creation
and initial document/snapshot work occur on the Core thread. Shutdown stops and
joins Core work before stopping and joining RendererHost, then destroys the
Canvas `HWND`; the stopped Canvas unregister is a safe no-op. A non-final
workspace first retargets Core notifications and atomically unregisters all of
its snapshot sinks under their respective publication locks, so neither a sink
nor notification target outlives its owner. A renderer-held snapshot may outlive
Core until RendererHost shutdown because it independently owns all borrowed
storage and is released by the Rust snapshot release function.
