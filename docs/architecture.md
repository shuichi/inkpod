# Architecture

This document describes the current component, ownership, thread, and state
boundaries. Product behavior is specified in [`../SPEC.md`](../SPEC.md), and
current gaps are summarized in [`implementation-status.md`](implementation-status.md).
Completed migration steps, superseded designs, and past measurements are
summarized in [`legacy.md`](legacy.md).

## Dependency direction

Inkpod has one platform-independent document-state owner. Crate dependencies are
one-way; owned work and immutable results cross the runtime queues:

```text
CMake -> Cargo -> inkpod-ffi -> inkpod-core -> inkpod-format -> inkpod-image
inkpod-ffi  -> inkpod-io
inkpod-core -> inkpod-io -> inkpod-format

Core engine thread <-> bounded inkpod-io workers
  submit/poll/apply       filesystem/codec/staged Core work

UI/Input thread -> bounded command/sample queue -> Core engine thread
                                                    | versioned C ABI
                                                    v
                                              immutable snapshot
                                                    | ownership queue
                                                    v
                                              Renderer thread -> DXGI Present
```

`inkpod-ffi` is the only Rust `staticlib`. `inkpod-core`, `inkpod-format`, and
`inkpod-image` do not depend on Win32, COM, Direct2D, Direct3D, DXGI, WIC, Windows
DPI types, or frontend thread types. The sole OS-specific filesystem backend is
private to `inkpod-io`, for physical identity, file coordination, and atomic
replacement. No OS type escapes that backend through a public Rust API or the
C ABI. C++ does not implement a second document, image-processing, history, or
native-format model. `inkpod-io` depends on codecs, never on application state;
Core supplies owned detached work to its generic executor.

## Rust responsibilities

| Crate           | Responsibility                                                                                                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `inkpod-image`  | Typed pixel formats, 64 x 64 sparse tiles, `Arc` copy-on-write storage, selection, fill/sampling/palette logic, and deterministic raster/filter/effect operations |
| `inkpod-format` | Bounded procedure-authoritative `.inkpod` v32 Cell/Cut containers and `.inkbatch` v5 models, stream/byte encode/decode/validation, and PNG/TIFF/TGA/BMP codecs; existing synchronous path wrappers remain for Rust callers outside the migrated application routes |
| `inkpod-io`     | Application-owned bounded workers, filesystem paths/identity/locks, encoded and decoded LRU leases, streaming file access, temporary-file publication/cleanup, recoverable native/raster pair installation, recovery artifacts, and polling progress |
| `inkpod-core`   | Stable-ID document/layer/plane state, immutable Genesis/base surfaces, a content-addressed canonical asset registry, StateId savepoints, views, raster clipboard, previews, animation, effects/Batch commands, persistence mapping, immutable render snapshots, and canonical primitive execution plus append-only journal/cache-free replay and semantic document digests for the migrated Core slice |
| `inkpod-ffi`    | ABI v32 fixed records and generation-tagged runtime IDs, opaque I/O manager/job handles and path submission/poll/apply/release, the common raster-pair open kind, issue-time sequence preservation fence, explicit current-document Revert flag, bounded validated-sidecar-target cache control/telemetry, complete sequence-resident target transfer, render preparation telemetry, and immutable prepared-source snapshot accessors, Batch v5 graph/staged-result handles, InkScript source/compiler/fragment plus authority/plan/run/report handles and fixed DTO host callbacks, persistence/compaction diagnostics, validation/conversion, panic containment, and ownership functions |

Binary, grayscale, RGBA8/16, straight-alpha, premultiplied display data, and
selection masks remain distinct types. Win32 may provide a
platform UUID at document creation, but Core persists it without acquiring an OS
dependency.

Each crate root is limited to module declarations and stable public re-exports.
Responsibility-specific modules contain implementation. `inkpod-core` keeps
thumbnail work, Batch v5 model/codec/input-output execution/typed operations, destructive transform orchestration/raster/
frame/numeric helpers, and view commands/coordinates/guides/secondary views/
shortcuts in separate modules; their `mod.rs` files remain declarative indices.
`inkpod-core` keeps
fine-grained tests of private invariants in `#[cfg(test)]` modules beside that
implementation, while public Core workflows run as a separate multi-module
integration-test target below `tests`. Architecture tests enforce small roots,
the responsibility split, `cfg(test)` gating, recursive CMake tracking, ABI
header/export parity, direct contract-test references, and the confinement of
OS filesystem code to the approved private `inkpod-io` backend.

## Shared filesystem service (IO-003)

`ApplicationHost` owns one `FileIoController` and opaque Rust `InkpodIoManager`
shared by every workspace and document. `CoreHost` binds that manager to each
production Core and owns submit, poll, apply, and release on the Core engine
thread. Windows passes owned paths and typed purpose/target metadata; it does
not enumerate, read, decode, write, identify, or replace image files on these
routes. The worker pool owns those filesystem operations and the detached Rust
codec/replay work. Workers hold no live Core handle and call no C++ callback.

The shared boundary covers editable native/recovery and raster open, automatic
and explicit sequences, Subpalette/Reference catalogs, Light Table add/reload,
and Batch file/folder/preview inputs and outputs. Icons, palette/chart files,
shortcuts/settings, clipboard memory images, test fixtures, and Cut member
validation are outside this migration. Raster input is PNG, TIFF, TGA, or BMP;
reference-only loading never creates an editable document or native file.

Each temporary-job lease owns and removes only its exclusively created child
directory. The common `inkpod-file-io` allocation root is retained even when
empty because other workers or test processes may be between creating and
canonicalizing that shared root; one job's explicit or deferred cleanup must
not invalidate another job's allocation attempt.

Editable File Open and Sequence activation use one raster-pair resolver. For a
selected raster `R`, the manager derives the same-directory, same-stem native
candidate `N`, probes both identities/complete stamps, and returns one closed
authority result: `Committed`, `Planned`, or `None`. An existing `N` is decoded,
asset-validated, and replayed before publication; its normal composite must equal
the canonical decode of `R` in dimensions, native depth, straight alpha, pixel
values, and representable DPI. Encoded container layout and optional metadata are
not equality inputs. A valid match returns `Committed`; an absent `N` returns a
clean staged raster Genesis plus a session-only `Planned` pair. Corruption,
non-current native data, format disagreement, or decoded mismatch returns an
explicit conflict and never silently falls back to a new raster document. A
separately invoked explicit raster import returns `None` and does not claim the
existing native path; pair open itself fails atomically.

Companion discovery is a bounded same-directory inventory owned by `inkpod-io`.
Backend-normalized stems and ASCII-insensitive extensions preserve exact existing
case on case-sensitive filesystems. Zero matches select the lowercase canonical
missing path (`.inkpod`, or `.tif` for a newly derived TIFF companion), one match
selects that directory entry, and two matches are ambiguous. One enumeration
collects native and raster candidates together. Up to 32 complete inventories,
each containing at most 20,000 relevant raster/native entries and 16 MiB of
cached path/stem/extension text, are retained by application-wide LRU when the
platform supplies a fail-closed
namespace observer. Windows retains a nonrecursive directory-name change-notification
handle with each entry. Backends without an equally strong observer do not cache
the inventory and continue to enumerate on every proof; mtime alone is not cache
authority. An unchanged observer serves later candidate proofs without `read_dir`.
Entry addition, deletion, rename, observer failure, explicit cache clearing, or
LRU eviction discards the inventory and requires another bounded enumeration. A
directory exceeding the per-inventory entry bound remains correct but uncached.
The observer starts before enumeration and must remain unchanged through
completion before the result is cacheable. Candidate snapshots
are revalidated after pair recovery and after replay/content validation, while
the selected raster/native complete stamps remain independently checked. Thus
cache reuse does not turn a new case/TIFF alias or a changed pair member into
silent authority.

| Image-cache limit | Bound |
|---|---|
| Resident images | 10,000 |
| Encoded bytes per raster file | 512 MiB |
| Encoded total | 8 GiB |
| Decoded pixel total | 8 GiB |
| Validated sidecar targets | 0–1 GiB configurable, 1 GiB default, 64 targets maximum |

Reservations precede reads and pixel allocation. Encoded and decoded LRU
accounting includes queued results, consumer-held leases, old/new replacement
candidates, coexisting decode pixel buffers, and reference-view render pixels.
Reference snapshot and individually cloned tile owners retain their derived
pixel charge. Render tiles expose immutable slices over private shared pixel
storage; moving a completed `Vec` into that storage does not copy its pixels.
Removing a map entry does not uncharge a live lease. Only unpinned entries may
be evicted; an operation that
cannot reserve its bounded working set fails without discarding the current
document/catalog. Document/history assets are separate authoritative data and
are never evicted by this cache. Native/recovery files use uncached bounded
streaming with the existing 1 GiB limit.

Every successfully attached Sequence keeps all source `TileRaster` values and
thumbnails resident. During attachment, each dense codec output is converted
once, its canonical `AssetId` is computed once, then only that exact decoded
cache ownership is discarded. The source's derived-budget lease stays with the
tiles, so eviction or manager shutdown cannot invalidate resident pixels. Thus a
35-frame catalog retains 35 edit-ready tiled sources, not a second dense copy of
the same 35 pixel arrays.

For a sidecar-less Sequence pair target, the manager first validates the
catalog source's originating manager, normalized path, complete stamp, source
generation, format, and raster metadata. A match constructs the target directly
from the catalog's `TileRaster` and precomputed canonical `AssetId`; it does not
read/decode the raster, scan/hash all pixels, allocate a dense payload, or retile
the image. The managed `AssetRecord`, Genesis source, editable MainLine and
sequence catalog share the immutable tile-map backing. `TileRaster` COW detaches
the map and then only a touched tile on the first effective edit. A provenance
mismatch is ordinary ineligibility and uses the unchanged owned import.
Persistence and InkScript export materialize a temporary canonical dense byte
stream only when those operations actually request it. Native v32, replay epoch
27, digests, history, and savepoints are unchanged; ABI v32 adds resident-target
transfer and render-preparation accessors without changing canonical state.
Private test-support counters require
the managed switch to report zero dense-copy, hash, and full-tile-materialization
work.

Existing sidecars still undergo full replay and canonical companion comparison
on the first exact visit. The application-wide `ValidatedTargetCache` then keeps
only clean, non-recovered, committed targets in an LRU keyed by normalized pair
paths and both complete stamps. It is capped by both a conservative logical
weight (0–1 GiB, 1 GiB default) and 64 targets. A hit clones Core metadata and
COW graph/tile owners without repeating native read, replay, raster decode or
full comparison. Directory observer proof, selected-member stamp validation and
final namespace/TOCTOU checks remain mandatory. Changed or missing members,
limit reduction, disable, and LRU pressure evict entries. Live Core/job clones
remain valid after eviction and are not counted as cache ownership.

Pair-target adoption remains independent of that optimization classification.
After the common resolver has proved a staged pair target, the existing restore
path invalidates the outgoing document and re-registers the selected source as
pristine for sidecar replay, recovery, owned fallback, and managed reuse alike.
The existing edit/preview/revision/view-mode invalidation checks remain the sole
subsequent admission fence.

The same manager also enforces the CPU sequence composition cache's separate
64-source / 1-GiB ceiling inside the decoded total. `DecodedLease` reservations
count full-catalog pending preparation, retained compositions, and live snapshot/tile owners
after cache eviction or Core destruction. Manager clones share one budget;
`InkpodIoCacheInfo::sequence_render_allocations` and `sequence_render_bytes`
report that subset without adding it again to `decoded_bytes`. An unsuccessful
cache admission falls back to ordinary foreground composition. It is not a
failure to open or switch the document.

Read/write coordination uses normalized path and physical volume/file identity
keys, including aliases, with a fixed acquisition order for multiple files.
Replacement invalidates affected cache entries. External writes are not locked
by the application: stamp/content validation detects changes and rejects stale
reads or unauthorized overwrite instead of trusting a filename or timestamp
alone.
Paired save additionally proves that each same-stem native/raster candidate set
contains only the selected member after staging, after native publication but
before raster publication, and after both publications before success cleanup.
The middle fence uses mixed-pair recovery; a final-fence conflict first verifies
both replacement and backup proofs, restores or removes both prior members, then
uses ordinary recovery cleanup. An uncoordinated external process can still create
a new alias after the last directory scan because filesystems provide no
directory-wide transaction; the next open/save detects that ambiguity.

Automatic sequence loading is a separate job after primary raster open succeeds.
A Rust worker synchronously enumerates the directory, matches the last ASCII
digit run and case-insensitive surrounding stem, accepts every supported raster
extension, and selects a natural-order neighborhood containing the seed, capped
at 1,000 images. Digit width is not fixed. Truncation is explicit; this cap does
not replace the independent explicit-sequence, reference, or Batch limits.
Late attachment validates sequence/owner identity without reopening Genesis or
discarding edits made after the primary open. Discovery/decode failure leaves
that successful primary open intact.

Jobs expose nonblocking discovery, read, loaded, failure, work, result, and
installation counters through ABI v32. Loaded images include cache hits;
internal Batch output rereads do not count as additional input images. Queued
drop/cancel cannot publish a live candidate. A result is applied only on its
captured owner after generation/revision validation. Normal save, sequence
autosave-switch, and compaction installation return a pending state after
owner authorization and fence document mutation until final apply. Closing or
shutdown cancels/drains this continuation before destroying its Core; releasing
an installation early is not a substitute for finalization.

The Windows-only `CoreHost::FileIoCompletion` runs on the Core owner and returns
the I/O operation/apply status separately from the status after published-state
refresh and snapshot submission. Neither status acknowledges a Renderer Present.
`FileIoController` attempts Rust job release on that owner before exposing its
copied `FileIoResult` to the UI-thread continuation. `document_applied` records a
successful non-reference Core apply, even if later identity refresh, snapshot
publication, or job release fails. The UI reconciles that applied result; a
presentation error must not repeat a successful save, open, or installation.

A sequence activation plan classifies an existing binding as `NOOP`, an identical
initial raster as `BIND`, and a document replacement as `REPLACE`. Core validates
the captured source/target identity, source generation, and revisions again at
commit. `NOOP` and `BIND` preserve the old save authority, including dirty state;
a successful initial `BIND` rekeys the sequence source to the current document
UUID/new owner generation and rebases the active frontend file binding to the
current pair paths/identities without replacing document/history/savepoints;
the initial binding does not use the bound-source-only autosave request. Only
`REPLACE` prepares the next recovery path, invokes the shared raster-pair
resolver, and reserves the target's pair identity before applying the switch. A
bound autosave switch also uses the request's `REQUIRED` flag, including a changed
source generation under the same document UUID. Independently, Core compares the
current document/editor revisions with the runtime-only preservation baseline set
by normal save, ordinary open, immutable source adoption, or exact recovery. A
real switch sets `SOURCE_RECOVERY_REQUIRED` whenever that baseline is stale, even
after Undo returns to a clean savepoint; dirty, recovered, and repair-needed states
also set it. Frontend generation decisions use this issue-time bit rather than
recomputing from presentation flags, and owner validation checks it again before
commit. `NOOP` and `BIND` never set it. An unbound replacement follows
the ordinary save confirmation first. One bounded reservation per registered
session covers both target pair members and the normalized original/source paths,
so another open or write cannot claim them while recovery or target preparation
is pending. The old session identity and paths remain active until success.
Publication moves the prepared target-specific `Committed` or `Planned` result;
it never copies the old pair authority to the new cell. A standalone recovery or
pair-proof-`None` target remains authority `None`: original identity supports
duplicate detection and conflict presentation but grants no normal-save authority.
An exact-pair sequence recovery instead resolves `metadata.source_path` through
the same raster-pair resolver and adopts that target-specific `Committed`,
`Planned`, or repair-needed authority only after capture-time proof, current member
stamps, document UUID, canonical Genesis, raster identity, journal prefix, and
encoded document/editor savepoint baseline all agree.
Failure, cancellation, queue rejection, and session close release the reservation.
True no-ops preserve the old authority, and a snapshot/presentation failure
cannot undo authority publication after Core has committed the switch.
An explicitly supplied target-recovery path is exact replay authority: missing,
malformed, non-current, or wrong-UUID content fails the switch atomically. Only a
switch whose preservation bit is clear and whose target-recovery path is absent
may activate the immutable flattened catalog source. For raster-pair navigation,
the catalog UUID/generation fences
the selection while the resolver validates the current disk pair; catalog pixels
are thumbnails/discovery data and do not override a later successful pair save.

Light Table's explicit swap follows the same save-authority rule. Windows captures
the selected stable item, its source UUID/revision, and the current document/editor
revisions through existing metadata queries, then revalidates them on the owner
before swapping. It reserves the new Untitled identity and prepares its recovery
path before commit. Only a successful swap clears the previous native/source paths;
ordinary save must obtain a destination for the newly editable reference. The
same commit removes the outgoing sequence catalog and changes its runtime owner,
so a held automatic-discovery completion cannot republish file bindings for the
replaced document. Failure, cancellation, and stale validation retain both the
catalog and its bindings.

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
return. Variable samples and clipboard memory images enter through bounded
data-plane APIs that synchronously validate and copy borrowed memory. Migrated
filesystem routes pass paths only and let the I/O worker own decoding. Raster
open/import, clipboard, and Light Table sources are interned in the canonical asset registry;
stroke samples become an owned inline payload up to 4 MiB and one immutable
sample asset above that cutoff. Sequence sources remain bounded Rust-owned raster
copies. Neither Core nor a committed procedure retains the caller's borrowed
record or buffer. Owned filesystem paths remain runtime I/O authority and never
enter a committed procedure. A committed procedure contains bounded inline
canonical bytes or immutable
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
and there is no supported incomplete-journal state. The v32 writer serializes
Genesis, retained assets, the complete journal/control-event sequence, editor
state, savepoints, cursor, branch graph, and ID authorities. Open validates and
either fully replays that graph or uses a prefix/state/policy-verified optional
checkpoint in a staged Core before one replacement of the live generation.
Checkpoint mismatch selects full replay; malformed/hash/bound failure rejects.
The journal remains authoritative and every non-v32 Cell version is rejected.

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
commitment is schema 12/domain 10, the replay contract is epoch 27, and the native
format is version 32. Every image layer has one standard topology: exactly one
MainLine plane, exactly one Color plane, and zero or more Raster planes. Layer kinds
do not exist. Current selection, ordered named saved-selection masks, and the sparse
fill-protection mask are document-owned rasters outside the image tree; only their
materialized tiles allocate. Cut payload schema 3 separates immutable member assets from
ordered membership and records membership before/after states in Cut history, while
retaining Cell-document primitive semantics. Sequence edits stage bounded ordered
insert/remove/move/renumber operations and publish one Cut revision only after final
validation. Removed members are not physically deleted and remain addressable by
stable `(CellId, document UUID)` while retained Cut history can restore them. The
optional angled shooting frame is an independent document object; its canonical edits,
preview, transform rules, snapshot overlay, and output policy are persisted by v32.
Flat normal output excludes the overlay, while explicit instruction export may include
the shooting-frame outline. Vanishing-point and adjustment-layer document objects are
absent from the current model and format. Epoch 19/version 22 added the independent
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
payload length before the digest is accepted. Sample streams use their own closed
canonical descriptors. Equal descriptor-plus-payload inputs deduplicate
regardless of source path or codec, while a different format, alpha meaning,
dimension, stride, or logical payload produces a different identity. Encoded
source bytes, file names, paths, timestamps, and optional provenance do not enter
the asset digest or replay input.

Registry retention is semantic rather than a cache of the current materialized
document. Roots include Genesis, every retained journal branch and redo tail,
known persistent editor/document-metadata references, and live transient owners.
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
stay inline. The Windows path/clipboard adapters retain
OS authority only long enough to obtain input bytes; later replay does not reopen
a path or borrow C++ memory.

Cache-free verification first builds a detached asset archive from every semantic
retention root, deep-copies each logical payload, and re-ingests it into an empty
registry with the expected `AssetId`. Fresh Genesis/journal replay uses only that
detached registry, so passing verification cannot be an artifact of shared
`AssetRecord`, payload, or `TileRaster` ownership. Production v32 persists the
same rooted graph in GENS/ASST.

The present ABI is v30. `InkpodObjectId` separates Core, snapshot, task, color,
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
work variant contains a callable, pointer, path, or STL container. File I/O jobs
have separate manager/job handles and owned bounded path requests; their worker
results return through the engine's polling continuation. Current native normal
save, autosave/recovery, and Batch output all serialize asset-backed Genesis and
every retained asset through the same Core-owned GENS/ASST mapping. Flat common-
raster export remains a separate operation.

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

### Native Windows UI colors

Client controls retain their standard Win32 visual styles and drawing colors.
Existing owner-draw UI uses `GetSysColor`/`GetSysColorBrush` foreground,
background and selection pairs. Windows app Light/Dark preferences do not
replace these colors; high-contrast system colors are not overridden.
There is no app-theme monitor, custom client palette, visual-style opt-out,
or theme setting in document/workspace persistence.

Workspace title bars retain the documented
`DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` opt-in, leaving frame
rendering to Windows. Canvas/Light Table composition, Subpalette images and
sampling, color swatches, checker colors and thumbnails remain unchanged.
The native-color source gate rejects reintroducing app-color subscriptions or
client palette overrides.

### Win32 multi-pane resize painting contract

Splitter, docking, floating-window, DPI, and parent-size changes are geometry
updates, not control-lifecycle events. A pane keeps its existing child `HWND`
values, pane-owned Common Controls contents, selection, focus, and a scroll
position that remains inside the resized viewport's valid range. Resize or
reflow may clamp a scroll position only when it falls outside the new limit. DockHost
chrome tab items are a projection of the layout model and may
be resynchronized under redraw suppression for a Structure change. The layout
owner computes the complete final geometry before it changes any child and
skips children whose bounds are already correct, except when a redraw-suspended
control must re-publish its final local visibility. It then applies every
changed position without intermediate painting, using deferred window placement
or `SetWindowPos(..., SWP_NOREDRAW)` as appropriate. It must not alternate one
child move with one child repaint because later moves can expose pixels that the
earlier repaint treated as final. The batch preflights each old bound and local
visibility, verifies the complete final set, and restores the old set if either
the deferred placement or its sequential fallback publishes only a prefix. A
rollback verification failure remains a transaction failure: the candidate
logical model and hit-test state are not committed, and the owner reprojects its
captured model instead of reporting the partial placement as successful.

After all children reach their final bounds, the layout owner performs one
bounded synchronous repaint. A pane parent that owns standard child controls
uses `WS_CLIPCHILDREN`, or equivalent clipping, so parent background erasure
does not overwrite the children at their current positions. Vacated old child
bounds remain part of the parent dirty region and are erased before the final
frame is presented. `RedrawWindow` normally combines `RDW_INVALIDATE` and
`RDW_UPDATENOW`; add `RDW_ERASE` when a standard background or vacated frame
must be cleared, and add `RDW_ALLCHILDREN` when the complete pane subtree must
paint after batched placement. `RDW_FRAME` joins non-client metrics such as a
scrollbar with that final frame. Common Control metric messages that may draw
synchronously (`WM_SETFONT`, column width, item height) disable their own redraw
or run under a local-visibility-preserving `WM_SETREDRAW` guard. The guard wraps
the metric mutation, is restored before visibility-dependent placement is
verified, and leaves any required invalidation to the final bounded redraw. A
fully covering owner-draw surface may omit
erase, but only when its paint contract covers every invalidated pixel.

Canvas non-client scrollbars participate in the same final-frame rule. Main and
Subpalette Canvas layout computes the final client viewport with both permanent
native bars present, derives `SCROLLINFO` only from the last accepted view
transform, and keeps zero-movement bars through `SIF_DISABLENOSCROLL`. Both-axis
`SCROLLINFO` candidates are calculated before either bar is changed, installed
with `SetScrollInfo(..., FALSE)`, and read back exactly; a partial or normalized
mismatch restores the previous accepted pair and leaves the candidate model
uncommitted. Layout does not toggle `WS_HSCROLL`/`WS_VSCROLL`, recreate the
Canvas, or publish an intermediate candidate frame. Its bounded completion
repaint includes `RDW_FRAME` together with the Canvas client region. Ordinary
resize may extend a view's sticky range but does not shrink it, reset pan/focus,
or borrow another view's range.

The dirty region is selected by ownership and overlap:

- for one boundary or a small independent control set, repaint the union of the
  old and new bounds, including the vacated pixels;
- when several child windows move and their old/new bounds can overlap, repaint
  the complete pane subtree after placement;
- for a DockHost Structure change, also include the old and new right-zone and
  right-tool-tab bounds;
- never use a synchronous whole-workspace redraw to hide an incorrectly owned
  dirty region.

The DockHost may repaint its splitter or the narrow strip between the old and
new stack boundary separately, but that does not replace the affected pane's
responsibility for its child layout and vacated background. A geometry-only
path does not rebuild tabs or lists, resend reset-content messages, recreate
controls, or silently change selection and focus.

A Bottom zone containing only the Sequence pane is a measured fixed-extent
exception. The pane measures its current-DPI thumbnail box, three vertical
image/text paddings, one text line, native horizontal-scrollbar and border
metrics, page margins, target/import row, visible Cut action rows, and the
28-DIP DockHost header in device pixels, then rounds the total up to DIPs with
a 168-DIP floor. `DockLayoutRuntimeMetrics` carries this transient value; it is
not persisted. The pure dock projection omits both the Bottom zone-extent
splitter and its four-DIP gap, and `SetZoneExtentDip` is a no-op. Adding another
Bottom pane or moving Sequence out of Bottom restores the ordinary saved zone
extent and splitter. Thumbnail-width changes update ListBox item/column metrics
under redraw suppression without rebuilding item strings, selection, focus,
viewport, or application-wide thumbnail-cache entries.

DockHost and pane-child layout form one nested presentation transaction for
affected pane roots that remain children of DockHost, including a root becoming
Hidden. Before DockHost moves those roots, their tabs, or their splitters, it
starts a sticky inner-layout status and defers synchronous pane completion.
Pane `WM_SIZE` handlers may compute and apply final child geometry during that
interval, but they do not present it. Each inner plan reports placement or
rollback failure to its transaction root. After every sibling and inner plan
reaches its verified final state, DockHost removes the deferral and synchronously
repaints the bounded dirty union. This path covers zone/stack splitters and
same-parent right-pane add/remove and tab Structure changes. Floating, expanded
AutoHide, and other reparenting transitions complete under the destination
parent and are not mixed into this same-parent transaction.

A synchronous DockHost mutation captures the DockLayout model, right-tool-tab
model, pane host flags, physical bounds, and local visibility before mutation.
If outer placement or any inner plan fails, it restores the captured logical
models, attempts and verifies rollback of the registered physical set,
reprojects the old DockHost chrome, and reports failure to the command before
focus or successful command-surface follow-up runs. An OS-level rollback or
reprojection failure is still reported as failure and never commits the candidate
logical model. Ordinary parent
resize has no logical model mutation; its batch still restores and verifies the
registered physical set on failure. DPI-driven tab font and padding changes join
the same final repaint instead of drawing at old tab bounds. A Structure
notification may rebuild DockHost chrome tab items, but is not a pane-data
notification: surviving pane-owned tabs, lists, selections, focus, and valid
scroll state are not refreshed or reconstructed merely because bounds changed;
only a newly out-of-range scroll offset may be clamped to the resized viewport.

Common Controls tab pages are sibling `HWND`s, not children of the tab itself.
After geometry commit and before repaint, the tab is placed at `HWND_BOTTOM` and
visible pages are raised without redraw while preserving their existing sibling
and keyboard order. The complete z-order is verified; failure attempts and
verifies restoration of the original sibling order and remains an inner-
transaction failure even if restoration cannot be verified. Platform-normalized
dimensions, such as the closed height of `CBS_DROPDOWNLIST`, use the same
normalized comparison for unchanged detection and final verification.

Regression coverage combines source-contract and actual-window checks. Static
checks lock the no-intermediate-redraw placement and the final repaint call.
Native smoke resizes representative panes in both axes where applicable,
verifies anchored children move by the complete size delta, and requires no
pending parent or child update region after the synchronous repaint. A hidden
smoke window is not required to receive `WM_PAINT`, because Windows may suppress
paint delivery while hidden; it is checked through geometry and update regions.
At least one visible product or pixel probe must additionally demonstrate that
old frames and background pixels are removed. The English/Japanese visible runs
exercise the environment DPI; model/layout tests cover 96/120/144/192 DPI.
Physical high-DPI repaint remains a separate platform check.
The visible right-pane matrix also adds and removes a pane from a selected tab
with sufficient height, so all affected final geometries and the surviving shrink /
grow round trip are checked together with pane/control identity, no list reset,
list count/selection/top index, valid scroll state, parent/child update regions, and old-frame
sentinel erasure. Layout itself does not redirect focus; the explicit show-pane
command must select its destination and move focus to the newly shown pane's
natural first target after a successful transaction, as required by SPEC 88.

Focused coverage for this slice includes the checked pure range model, permanent
styles and exact accepted `SCROLLINFO` projection on two document-bound Canvas `HWND`s,
one native line gesture without optimistic `nPos` publication, and targeted,
session-wide, rejected, retained and one-shot scroll-reset envelope cases in
`CoreHost`. The remaining product matrix covers disabled no-movement
presentation, page/thumb input and endpoint freeze, two views of one document,
both-axis resize/final-frame regions, complete tab/source switching, unmodified
Subpalette Arrow/Page navigation, Shift-modified pan, exact sampling after
scroll, hidden/occluded and renderer-rejection Subpalette publication,
localized MSAA/UIA range/value exposure, 96/120/144/192-DPI layout, high
contrast, and screen-reader keyboard reachability. The performance gate must
also continue to report zero raster payload access on cache-hit scrollbar
movement.

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
At editor-stroke begin, Core resolves that stable target before color capture:
MainLine uses the document main-line color, while Color and Raster use the
selected tool's independently retained paint color. The resolved exact-depth
value is stored in the canonical stroke, so later color changes cannot affect an
active stroke. On an RGBA MainLine, changing the document main-line color leaves
existing native pixels untouched and controls only later MainLine drawing.
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
authoritative tool, color, fill, selection, or brush defaults. Document, view,
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

`DocumentRegistry` also owns the canonical logical identity index. For migrated
image routes, Rust returns physical volume/file identity or normalized
missing-path identity for each pair member; the C++ registry performs no
filesystem identity query. On Windows, case-insensitive path coordination and
`FILE_ID_INFO` are backend details. A normal raster pair registers the native and
raster identities/missing paths as aliases of one `DocumentSession`, including
while a `Planned` pair is reserved, so opening either member cannot create a
second independent Core. An untitled or authority-`None` session is keyed by a
generated UUID. Display names and tab positions are never identities. Open uses
the resolver result to detect an existing session before publication. Save As
stages both destination members, rejects a conflict with a different live or
planned session before installation, and publishes the new shell path, logical
identity index, title, bounded recent-file entry, and recovery metadata only
after pair save succeeds. A failed save leaves the old identity and presentation
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
sequence-source, thumbnail-cache, and reserved sequence-composition categories
without building a snapshot. ABI v32 retains the latter's byte/source/tile counts;
the aggregate render-cache byte/tile counts already include the same charged
payloads. Shared COW Core clones may report overlapping logical usage; the I/O
manager's shared reservation counters enforce the application-wide CPU ceiling.
Unchanged `TileRaster` clones share a lazy cached value of the existing checksum.
Mutation detaches/invalidates that derived value, so repeated document-info
queries need not rescan the same raster bytes. Neither the checksum algorithm
nor the canonical render-cache formula is changed.
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

The frontend's repeated-view shortcut also requires the Canvas sink's actual
session, document generation, frontend view, Canvas and surface generation to
match the requested route. Model selection alone is insufficient: creating a
view updates the model's active entries before activation binds the Canvas.
A new or stale route must pass through the normal binding/publication path.
The UI may query the selected view's published transform only after its own
snapshot has been submitted; another view's transform is never substituted.
An editor command rejected by Core refreshes the authoritative editor state
for its captured session/generation before returning the original failure.
It never retries the rejected edit or relies on repeated activation to repair
stale UI state. Successful navigation adds no refresh for this recovery case.

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
frontend view, Canvas, document generation, surface generation, snapshot
revision, view revision, a separately captured committed document revision, and
the Windows-only `presentation_epoch` of the installed navigation result.
The snapshot revision can include a preview and must not stand in for the
committed revision when releasing an editing fence. Recovery can install an
equal or lower document revision, so the epoch must also match. This epoch is
fixed on the Core owner thread after successful installation and before snapshot
publication; it is not a Rust Core revision, C ABI field, or CPU/GPU cache key.
Successful ordinary native, recovery, or raster open explicitly sets the epoch
to zero instead of inheriting a previous sequence token. Core session-generation
rebind also clears it; failed apply does not install the requested epoch.
For these ordinary opens, the UI completion checks `document_applied` and the
issuing session/generation, then clears that session's pending/required
revision/epoch fence and synchronizes its Canvases to zero. This applies even
when the document is no longer active, or later snapshot publication failed;
neither case undoes the successful Core apply or restores an old sequence fence.
Snapshot fan-out registers
every visible editor-group Canvas, including the primary group of each newly created
workspace, up to 16 sinks matching eight workspaces with two groups each;
inactive tabs and auxiliary panes do not consume that bound. RendererHost accepts a
snapshot only when the complete
route equals the current surface binding and the snapshot accessors confirm both
snapshot/view revisions. Rebind clears the old retained snapshot before accepting
the new route. Stale, hidden, occluded, queue-full, replaced, closed, and shutdown paths
all consume the Rust snapshot owner exactly once. Device loss first discards all
surface GPU resources, recreates the shared device, then reconstructs every
surface cache from its retained immutable snapshot; Core document state is not
involved. Renderer telemetry is published as a value copy and accounts for
retained/pending immutable snapshots, active/cached GPU tiles, swap-chain
payloads, surfaces, queue rejection/replacement, stale frames, resource-limit
rejection, and device reset. A per-surface value copy retains the bound document/
view/Canvas/generation route and byte/count values but no snapshot or GPU
pointer. It also records the last successfully presented committed document/view
revision, presentation epoch, and source identity, per-surface upload bytes/counts, and sequence-cache
bytes/source/eviction counts. Submit, upload, frame-latency timeout, and occlusion
do not advance the successful-Present fields; rebind and device loss invalidate
them. The submission QPC and first successful Present QPC for a committed
revision/epoch pair are observations, not a substitute for checking its route,
committed revision, and epoch. Re-presenting that same pair retains its first
successful-Present time.
GPU tile payloads share a 1 GiB application-wide
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
`WM_HSCROLL`, `WM_VSCROLL`, and the native Shift+Arrow/Page routes use the same
bounded, pointer-free Canvas gesture record. `CanvasHost` derives a relative
`PAN_BY` delta from the current accepted absolute `q`; the parent resolves the
exact Main or auxiliary target and invokes the existing view adapter. That
adapter may synchronously execute the Core-host operation and snapshot build on
its established owner lane, but it never waits for Present. The scrollbar does
not move optimistically: only an envelope accepted by the renderer queue returns
its transform through the latest-wins scroll-projection mailbox to the Canvas
owner thread. A relative command remains blocked until that accepted projection
arrives, so a rejection cannot double-apply the prior delta. Line scrolling uses
one 32-DIP step; page scrolling uses the accepted viewport extent minus one line
of overlap, with a one-device-pixel minimum. Close/rebind, invalid operations,
cancellation, and shutdown retain the prior presented snapshot and scrollbar
projection. Renderer-queue rejection can leave an already accepted Core view
awaiting viewport-refresh republication, and relative scrollbar input remains
blocked until that projection is accepted. Messages and mailbox records carry no
Rust-owned pointer. A posted projection wake that encounters the Windows message
quota arms one fixed-ID window timer; if a newer mailbox token arrives during a
reentrant non-client redraw, the completed outer apply re-arms that wake after its
guard is released. Projection-apply retries are bounded, and the final viewport
refresh uses one same-UI-thread parent send only when its ordinary post fails, so
queue saturation neither loses the latest accepted transform nor creates an
unbounded retry loop.
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

The fixed command-state catalog assigns all 324 production commands exactly one
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
session/generation before dispatch. Mixed PNG/TIFF/TGA/BMP inputs are decoded by
the shared Rust I/O service and published atomically on the Core owner thread;
C++ neither decodes pixels nor owns a
second sequence model. The modeless owner-drawn list obtains bounded straight
RGBA8 thumbnails through caller-owned query/copy buffers. Opening a numbered
raster discovers only sibling files whose prefix and suffix around the final
numeric run match, then selects the opened cell in Core natural order. Dirty-cell
cancel, stale target, decode failure, and endpoint no-op leave the current cell
unchanged.

Each immutable source retains one bounded thumbnail generated during ingestion.
The cell metadata and thumbnail-size queries reuse it without resizing or
cloning its pixel vector; only an explicit thumbnail copy transfers bytes to the
caller. `CoreHost` publishes `InkpodSequenceCatalogInfo` per session/generation:
catalog owner generation, sequence revision, count, and active index. With the
same catalog and thumbnail invalidation generation, the UI keeps the list,
labels, and thumbnail keys and updates only selection/header state. Catalog or
pane-target replacement and thumbnail eviction/invalidation take the ordinary
bounded reload path. A zero catalog owner disables reuse.

The native owner-drawn multicolumn ListBox is constrained to one row and scrolls
horizontally. Its native item text retains the complete accessible frame name.
Left/Right navigation is local to list focus, and Cut member activation uses its
stable member identity rather than the ordinary-sequence command. Selection and
thumbnail updates retain the viewport; geometry-only resize does not repopulate
the list. Cut-only controls occupy a compact row only when visible. The pane
descriptor has a 168-DIP minimum and a 184-DIP preferred height. Closing its dock
tab hides the pane through a captured pane identity without closing a document
or deleting the sequence/Cut.

Fresh sequence-cell replacement initializes both the document and editor-state
clean baselines after selecting the imported plane. Merely constructing this
in-memory native representation does not count as an edit and grants no native
save path or source-file overwrite authority. The no-op/bind path, delayed
automatic discovery, and exact-native recovery retain their existing editor
state, history, dirty state, and savepoints; only actual fresh replacement uses
the new baseline.

Replacement stages the final transform for every existing view before commit.
Equal image dimensions preserve zoom, logical pan, flip, viewport, mode, and
view revision. Different dimensions use the existing resize policy: Manual
retains its transform, Fit recomputes fitting, and 1:1 recenters. The Windows
adapter does not issue a second Fit or publish an intermediate reset transform.
This view preparation also applies to the staged autosave/recovery switch;
it does not replace recovered history or editor state with a flattened source.

For a Cut descriptor, the pane keeps a derived thumbnail cache keyed by the
member's stable `(CellId, document UUID)` pair rather than its order or display
number. A cache miss opens that same-directory Cell in a staged temporary Core,
revalidates the identity, and requests one bounded visible-document thumbnail;
the bytes are never persisted in the Cut descriptor. Reorder, renumber, Cut
Undo/Redo, and save/reopen therefore preserve the thumbnail-to-member binding,
while an invalid or mismatched source never substitutes pixels from another Cell.

Normal previous/next navigation has a separate application-level endpoint
policy. It is stored once as the readable `animation.sequenceEndpoint` field in
`%LOCALAPPDATA%\inkpod\Settings\inkpod-settings.json` and is shared by every
workspace window in the process. A missing field falls back to `Stop`; malformed
or future settings reject the staged settings file and use all current defaults.
An exact-format, uniquely versioned older settings object is not decoded: the
loader reopens it with delete access, verifies the same bytes through that handle,
deletes it through that handle, and then uses current defaults.
`Stop` and `Wrap` are closed values and are independent of
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
`Autosave-before-switch` dirty-cell policy. The activation plan first distinguishes
no-op, initial binding, and replacement. A bound replacement's autosave route asks
Core for an immutable value request containing the exact source/target UUIDs,
sequence-source generations, and source document/editor revisions. An unbound
replacement uses normal save confirmation before activation. `DocumentSession` owns a
bounded registry keyed by source UUID plus generation; each published entry owns
one private native recovery path, sidecar metadata, and a monotonic artifact
generation. The UI pre-reserves registry capacity, then Core captures a COW
`SequenceSwitchSnapshot` with the complete sequence revision and source file
authority. Workers prepare the source native recovery and fully validate/replay
an associated target recovery, requiring its UUID to match. After owner
revalidation, the engine fences mutation while the worker installs the source
artifact and metadata; final owner apply swaps the target exactly once. Error,
cancel, or stale preparation keeps the source active. Same-cell requests do no
I/O and advance no document or savepoint state. Current secondary-view IDs,
reference selection, application defaults, and I/O ownership survive the switch.
The flattened `SequenceCellSource` remains a thumbnail/fresh-cell source and is
never used to reconstruct saved layer topology, history, selection, or editor
state.

Only one sequence switch token may be pending application-wide. A resident hit
publishes the outgoing complete editable Core into the sequence resident bank,
takes the exact target Core, validates the captured UUID/source generation, and
queues its final snapshot without native read, replay, raster decode, full graph
reconstruction, or a `SequenceSwitch` file job. Dirty-state recovery capture and
durable autosave publication start after that switch as a separate job; failure
keeps the outgoing resident dirty and reports the autosave error without rolling
the visible target back. During interactive activation, a UI-owned queue accepts at most
256 further navigation intentions. Each captures its `CommandContext`, catalog
owner/revision, explicit target or direction, and endpoint policy. After each
commit, the next intention is revalidated against that same owner and a relative
step is resolved from the newly committed cell. A stale/closed target is never
redirected to the currently active document; saturation rejects new intentions.
Accepted navigation intentions are not frame-coalesced, although the renderer
may replace their older undrawn snapshots. A resident miss or invalidated target
retains the proof-checked asynchronous recovery fallback and its exclusive fence.

The owner continuation publishes the prepared
association, identity, and pathless recovery shell for the captured live session,
even if the originating view has closed. A token/generation-only window message
refreshes surviving pane/menu UI; it never carries object pointers. A resident
hit updates Canvas binding and selection immediately, then coalesces secondary
pane/menu projections for 75 ms; it neither shows `セルを読み込んでいます` nor
restarts the periodic autosave timer. The clean
interactive path uses the same owned completion mailbox and publishes authority
only after Core commit. Core commit success is distinct from snapshot or
presentation success: a rendering failure cannot revert an already published
document. If the originating window is gone, the owned mailbox survives for a
retargeted notification or the existing file-I/O poll fallback. A committed
replacement whose snapshot publication failed is reconciled to the new document
in the UI and its snapshot is retried once; its editing fence remains until
successful Present. Save, metadata, queue, or stale failure before commit leaves the source
active and advances no normal path/savepoint. Close or shutdown cancels and
finalizes accepted installation before destroying that session, without
retargeting completion to another document.

For every sequence replacement, including autosave and native recovery,
`DocumentSession` holds a pending-activation fence, the required committed
document revision, and the accepted switch token as a nonzero presentation
epoch. Every Canvas bind reapplies that session fence. The Core owner fixes the
epoch only after the exact requested replacement commits, before publishing its
snapshot; a failed installation retains the previously required revision/epoch.
Editing is initially blocked until a matching session/generation/Canvas route
reports a successful Present with at least that actual committed revision and
the exact required epoch. An old cell's equal or higher document revision cannot
release a recovery fence. A preview revision, queue acceptance, upload
completion, latency timeout, or occluded frame cannot release it.

`ApplicationHost::SequenceEditReady` is a side-effect-free UI query for the
captured `CommandContext`; it rejects stale targets and pending activation
before considering readiness. The command router, direct pane color/tool/layer
callbacks, and user `UpdateEditorState` entries use this gate before changing
local editor state or dispatching Core work. Automatic presentation refresh and
editor reconciliation are not user edits and do not use that gate.

Before a Canvas route is rebound/unbound or its view is removed, the UI compares
the current sink route with the renderer's complete published route and can
record a successful sequence Present in `DocumentSession`. This acknowledgement
requires the owning session/generation, a still-owned view, the exact nonzero
required epoch, and a sufficient actual document revision. It lets an already
shown document remain editable through a pinned pane after its Canvas becomes
hidden. The query itself does not record an acknowledgement. A new generation
or epoch makes the old record inapplicable, and pending activation always
blocks; if a failed navigation restores the old required fence, its previous
acknowledgement remains usable.

The acknowledgement does not clear the required revision/epoch or the Canvas
fence. New stroke Begin still checks that Canvas's current route and successful
Present, including after rebind. Existing accepted stroke Append/End/Cancel
packets retain their ownership and ordering. This Windows input/presentation
token is separate from the pristine source-cache identity and is absent from
the C ABI, native format, document digest, replay, and both cache keys.

Both policy values are fields of the single versioned settings JSON and are
not part of the `.inkpod` schema.

The Light Table palette also uses the pane-target registry. Its set/item
selection is valid only with the captured session/generation namespace, and
every mutation dispatches to that exact Core handle. Canvas movement retains
the issue-time `CommandContext` until commit or cancel; a focus change, close,
or stale generation cannot redirect it. The UI caches only bounded set/item
metadata, while raster storage, snapshots, history, and persistence remain
Core-owned. Add/reload submits only the source path to the shared manager;
decode completes off-thread before one owner-thread Light Table primitive.
Reload retains display/transform properties and stable item/plane IDs and is
one Undo unit; failed, cancelled, or stale reload retains the previous source.
Explicit edit-image swap revalidates its captured source/target metadata before
commit and publishes its prepared pathless shell only after Core succeeds.
Modeless pane state is attached on the UI thread after dialog
creation; window messages do not carry C++ object or Rust-owned pointers.

The subpalette/reference palette completes the read-only auxiliary-display
path. It owns one UI-thread Canvas child and one auxiliary `CanvasId`; the
RendererHost still owns that Canvas surface and presents it on the renderer
thread. Its standalone Rust `SubpaletteCatalog` owns independent `ViewState`
and immutable decoded image leases, not a hidden Core, Genesis, or editable
document. Shared pure view-command and coordinate code supplies zoom, pan,
flip, fit, and exact native-depth RGBA8/16 sampling. The catalog retains all
accepted source images so navigation needs no I/O/decode, while converted
64-pixel render tiles are cached only for the visible viewport. Conversion
reserves the shared decoded budget before allocating; an exhausted budget
returns no partial snapshot. Leaving the viewport releases only the cache's
ownership, not charges retained by an immutable snapshot or cloned render tile.

The Subpalette Canvas, like each visible editor-group Canvas, permanently owns
native non-client horizontal and vertical scrollbars. They remain present and
use the standard disabled state when their accepted range has no movement. The
Subpalette projection is keyed by its workspace-local auxiliary view and never
borrows the active document view's range or position. Unmodified Arrow and Page
keys continue to navigate sources; Shift+Arrow produces native line scrolling
on the matching axis and Shift+PageUp/PageDown produces vertical page scrolling.
These standard-control keyboard and accessibility routes do not create document
commands or persisted shortcut bindings.

All replacement sources and the first fitted display snapshot validate before a
single catalog publication; a failed candidate leaves the old selection and
resident images available. Catalog replacement keeps the stable workspace-local
auxiliary route and advances a `presentation_epoch`; the renderer detects that
catalog incarnation change and clears its ordinary tile cache before repeated
catalog-local tile IDs can be reused. A visible renderer
queue rejection retains the old catalog, view, snapshot and epoch. When the
Canvas is hidden, a checked same-route bind clears any old retained snapshot
before the new catalog is committed, and the reset cause remains armed for its
later accepted publication. Released catalogs do not invalidate already-owned
immutable snapshots. Cached active-image navigation uses the same stable route,
but strict rollback after a renderer-queue rejection still requires a future
prepare/commit catalog ABI; `SUBPALETTE-001` therefore remains Experimental. The
Canvas consumes pointer strokes and converts only view gestures/sample
coordinates, never edit input. Shutdown unbinds the snapshot sink and releases
the catalog on its owner before unregistering the Canvas. Queue rejection retains
a single snapshot-release owner.

Color and Batch panes use the same target registry. Color registration, clear,
load, save, and main-line changes capture the pane's exact session/generation;
the Color header states follow or pinned policy. Batch exposes no target header
or pin action: its issue-time command context still fixes the exact
session/generation. A Batch run temporarily binds that target to a
generation-tagged `JobSessionId`. Preview, synchronous smoke,
and queued execution all use the captured document session instead of resolving
the later active tab. Completion validates the original target, publishes
progress/result only there, closes the job, and restores the prior follow/pin
policy. Closed/stale targets and queue failure cannot redirect a result.

The Batch pane owns only an editable UI-thread draft. Its stage List-View always
contains one fixed Input row, ordered operation rows, and one fixed Output row;
only operation rows draw a standard checkbox and accept checkbox-hit/Space
toggles. Input and Output never reserve a checkbox cell, and the operation flag
in the draft is the single source of truth rather than a generated state-image
index.
Selection changes only the inline scrollable `batch_parameter_editor`; they do
not mutate Core or an immutable graph. Add/duplicate/remove/move change only the
four-kind draft operation list. Preview, run, and save each validate the draft
and construct one Rust-owned immutable v4 graph. Load performs the inverse query
through `inkpod_batch_graph_get_info`, `get_input`, and operation row queries,
so a loaded set is editable rather than a count-only opaque object.

The parameter editor inherits the pane font, lays out only the controls for its
selected page at natural compact spacing, and gives color columns the full list
client width. Custom-drawn cells reserve separate alpha-aware swatch and native-
depth RGBA text rectangles and query the List-View's actual single-selection
state while painting. A small Windows-only color-editor model owns row/slot
validation: the selected old/new color exposes a native-range alpha edit, the
common RGB chooser preserves alpha, and the drawing-color button chooses old or
new through a localized popup for Color Replace. The Windows-only
`batch_input_picker` provides a
bounded multi-file common dialog and filesystem folder dialog; it accepts only
the Batch-supported formats and copies the returned paths into the draft before
the dialog buffer expires.

The editable set-name combo enumerates extension-free `.inkbatch` filenames in
`%LOCALAPPDATA%\inkpod\batch-sets`. The Windows-only `batch_set_store` creates that
directory, rejects traversal, reserved-device and ambiguous trailing names, and
maps one validated name to one `.inkbatch` path. Core and the file-format crate
continue to receive only the resulting UTF-8 path and own the current v5/operation-schema-4 codec.

Batch submission captures the graph and COW active document without filesystem
access on the issuing engine thread. Rust workers resolve File/Folder selectors
through the shared manager, apply natural order/range/scope once, and freeze the
result as explicit paths before parallel raster prefetch. The processing phase
does not enumerate the folder again; native inputs remain bounded streaming
reads and every temporary/read/write/identity operation uses `inkpod-io`.
All enabled operations for one item lower to the private typed `ApplyBatchOperations`
canonical primitive and share its one transaction, Undo unit, and replay
executor. The dedicated sparse fill-protection mask is document state, not UI
state or a selection-mask alias; all fill routes consume it as a hard boundary.
ActiveDocument output commits only to the captured session/generation, retaining
independent current view identities and application defaults. NewTabs
returns Rust-owned staged Core handles; `CoreHost::AdoptBatchResult` consumes
them on the engine thread, `ApplicationHost` prepares/publishes sessions only
after capacity checks, and rollback destroys any unpublished session. Window
messages carry completion status and generation values, never staged pointers.
Batch retains per-item Stop/Continue and preflights output collisions using
filesystem identity. Native Batch output is one `.inkpod`, never an implicit
normal-save pair; raster output keeps its explicitly chosen export semantics.

Contact-sheet preview first copies every file input or materializes the captured
active input into a private temporary job directory. No processing starts until
all input copies are complete. Streaming copies and output writes reserve the
remaining 4 GiB aggregate temporary allowance before publication, with the
native/raster per-file limits. Each processed output is reopened for its
thumbnail. A child progress context shares cancellation but keeps those internal
rereads separate from input-loaded counts. Cleanup must succeed before the
single clean, pathless preview Core is returned; cancellation, stale target,
or cleanup failure never publishes a preview tab or writes the real output folder.

The canonical workspace is represented by an HWND-free, fixed-capacity
`DockLayoutModel`. Its `PaneDescriptor` records give every surface a stable type
ID and localized resource title, default and allowed zones, target scope,
multiplicity, float/auto-hide capability, minimum/preferred sizes, and whether
an inspector header remains visible for a singleton stack. The compact Tool and
Tool Options surfaces opt out of singleton headers. The model permits only
`TopContext`, `Left`, `Right`,
and `Bottom` stacks around the central `EditorArea`, plus floating and hidden
placements. A stack is either tabbed or split in one direction; recursive/free
dock trees cannot be represented. At the 96-DPI reset baseline, the fixed 80-DIP
tool strip is left without a zone-extent splitter, and the 320-DIP color/layer
stack is right at 32:68; active-tool
options are not part of the dock model. The layer pane retains its internal 55:45
layer/plane split. Its shared action row has a resource-backed label naming the
currently active Layer or Plane target, and each button exposes the same target
in its accessible name. Tool buttons remain 64 x 34 DIP with meaningful
single-word Japanese labels, forward the menu command IDs, and consume the same
cached command state.

Each Tool expansion region anchors one UI-thread-owned Tool Options popup. The
popup has a thin system-color border and a captionless 30-DIP client header
rather than a system caption or resize frame. Real child buttons provide the
localized pin/close accessibility names; pin is a session-only presentation
state and never requests system-wide topmost. Deferred activation-loss handling
distinguishes the popup and its owned combo windows from an outside target before
hiding an unpinned popup. The page layout returns its natural content height;
the owner clamps that size to the monitor work area and enables pane-local
vertical scrolling only when the natural content cannot fit.

`WorkspaceWindow` owns one UI-thread `DockHost`, which applies the pure geometry
to the existing primary pane child windows. Docked content is parented to the
main frame; floating content is reparented into an ordinary main-window-owned
top-level frame and returns to the same child HWND when docked. Drag preview is
limited to descriptor-allowed zones. Resource-titled standard tab controls stay
visible for singleton inspector stacks so the content and action scope remain
identifiable. The singleton Tool strip can only be shown or hidden: it cannot
float or AutoHide, exposes only Close in its context menu, and has no adjacent
splitter. Other mouse/keyboard splitters retain a 4-DIP hit target and paint a
centered system-color rule, highlighted on hover, capture, or keyboard focus;
the internal Layer/Plane splitter follows the same presentation and accessibility
rules. Inspector pane context menus provide tab/split, dock, float, hide, restore,
and reset without retargeting a document command. Window-menu pane visibility
entries, including Batch, are direct checked toggles; Color target pin/follow
commands remain pane-local, while Batch exposes neither. Floating close
maps to hide, preserving the pane's controller state. All HWND and Common
Controls activity remains on the UI/Input thread; Core and renderer ownership is
unchanged.

The right zone uses a bounded flat `RightToolTabsModel`: each nonempty tab owns a
stable layout ID and an ordered unique pane-type list, and one selected ID is
stored separately. The first pane supplies the localized tab label; the full
ordered list supplies tooltip and accessibility text. Adding a hidden pane uses
the selected tab only when the DPI-scaled minimum heights plus splitters fit,
otherwise it creates a singleton tab. Batch is always created as a new singleton
tab; adding to a selected Batch tab creates another tab, and add/move/load reject
any mixed Batch membership. Moving and reordering use copy-then-publish model
updates, while removing the last pane deletes the tab and selects previous, next,
then first. Tool and Tool Options never enter this model.

The frontend persists each workspace in the bounded, current-version
`%LOCALAPPDATA%\inkpod\Settings\inkpod-settings.json` file. Pane types, zones,
dynamic tabs, presets, and other enums use stable readable strings rather than
Win32 IDs or binary payloads. The workspace section contains only main window
placement, editor split orientation/ratio, dock zones/order/ratios,
primary and secondary pane visibility/size/floating placement, AutoHide edge,
density, selected or user-named preset, and the dynamic right-tab IDs/order/
selection/pane membership; document paths, active strokes, jobs, and document/Core
identities are excluded. The decoder validates its exact size, counts, enums,
stable pane/tab IDs, duplicate pane/tab IDs, nonempty tabs, selected/next ID,
placement bounds, and bounded names. Unknown fields and pane values are rejected,
and a malformed, foreign, or future settings file restores defaults without aborting
startup or automatically overwriting that invalid file. An unambiguously identified
older `inkpod-settings` version is deleted without migration after same-handle byte
revalidation; deletion failure aborts startup instead of publishing or overwriting
the candidate. Development builds do not migrate old registry workspace records
or decode old settings files. Transient
narrow-window label suppression is never persisted.

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
Controls. The configurable command catalog supplies the standard workspace
defaults: Ctrl+Tab/Ctrl+Shift+Tab select tabs, Ctrl+1/Ctrl+2 select an editor
group, and Ctrl+F4 closes the captured view. Only F6/Shift+F6 focus navigation
is intercepted before that catalog to cycle menu, dock, editor, and status.
Edit controls retain all other text input. Standard
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
RGBA/selection, paper/DPI, and shortcut/dirty state. While background work is
active, its last part hosts a native job-selector button, progress bar, and
Cancel button; the other part boundaries adapt in the same layout operation.
There is no Job Progress pane, window command, or persisted layout member.
The workspace-owned presentation accepts bounded task registrations and copies
up to 128 cached file-job summaries from `FileIoController::CopyProgress`.
The existing 50-ms file-I/O timer polls completions and refreshes that workspace;
the status component refreshes atomic task progress every 100 ms while active.
Main-frame progress timers are handled before workspace/Core activation, using
the addressed HWND's workspace ID and resolving it again after completion
callbacks. Neither progress query path waits for Core execution or copies image
data. Native status-button focus, Tab/Enter/Space input and cancellation do not
issue a document command; the product smoke exercises both progress timers and
native keyboard cancellation while Core installation is deliberately held.
Task identities include a registration generation, file identities use the
controller request ID and captured generation, and multiple controllers of the
same task kind have separate registrations. Selection and cancellation never
retarget after completion, tab changes, or a nested job-selector menu loop.
Unknown totals, queued work, cancellation, and owner-thread publication use an
indeterminate bar; read count, loaded count, and work units are never mixed.
Completed jobs are removed only through their owner completion, and the normal
status text is restored when none remain. Controllers clear their exact context
before releasing task storage; history dialogs retain their initiating workspace.

Document tabs use the session/generation-specific published sequence-cell name,
saved filename, recovery/untitled fallback, dirty marker, and logical view number.
Pane visibility, pinning, and the current workspace's sequence view are not label
sources. Pane target captions use the same published name for their resolved
session, including pinned panes, `Planned` raster sequence cells, and
authority-`None` recovery sessions.
Closing the final view leaves an empty editor area and preserves its
workspace HWND and Core/renderer threads. No replacement document is allocated;
New/Open/Recent routes use workspace context and Rust-provided editor defaults.
The renderer releases the old snapshot/preview and paints its empty background,
and stale Canvas input is rejected while no active document/view exists.
Split creation captures its source workspace/session/view/generation before
creating child HWNDs. Reentrant activation of the new empty group cannot change
that source. Failure releases the partial tabs, Canvas and registered sink, then
restores the captured source only if its context still resolves; it never picks
another active document as a substitute.
Locator sampling is asynchronous and discards stale
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

Renderer preview setters validate their input before reporting an unchanged
state. Repeating an empty geometry/floating preview returns `S_FALSE`; geometry
also returns it when all rendered fields are unchanged. `ProcessControl` renders
these controls only for a state change reported as `S_OK`, so an empty clear
does not wait for a frame or present the old cell again. On the UI thread, the
selection, fill, color-replace, and raster-geometry cancel paths send a shared
Canvas clear only when their own gesture/preview exists or a prior clear is
pending. Cancellation still resets the local gesture immediately. A missing
Canvas or rejected clear preserves `geometry_preview_clear_pending` across tool
document resets, allowing a later cancel to retry; a successful clear removes
the marker. This UI marker is not a Core preview or persisted document state.

`CanvasSurface` acquires its frame-latency permit only after surface resources
and the complete snapshot are ready. The Renderer thread retains an acquired
permit across a failed adjustment, draw, or readback and consumes it only when
`Present` returns `S_OK`. Resizing buffers on the same swap chain preserves the
permit; discarding/recreating the swap chain invalidates it. A wait timeout is
not a successful Present and cannot advance revision/epoch telemetry or release
an input fence.

Asynchronous snapshot, resize, and preview work probes frame readiness without
waiting. If the frame cannot be presented yet, `SurfaceRecord` retains one
`presentation_pending` flag for its latest snapshot. An applied preview setter
can therefore return `S_OK` while its frame is still pending. The Renderer owner
processes newly queued work first, then `WaitForReadySurface` probes pending
surfaces and waits on their DXGI handles together with the queue/stop wake event.
Zero-time probes do not count as latency timeouts. A wait includes at most 63
surface handles and is bounded to 4 ms; larger sets are probed in full and use
rotating 1-ms wait batches. New work or stop interrupts the wait. A visible
surface that transiently reports occlusion is retried for up to 250 ms before it
is treated as durably occluded. Deferred retry
does not make the UI wait for frame readiness or make the Renderer busy-spin.

Explicit render requests retain separate `pending_render_requests` credits,
consuming one only for each successful Present. New snapshots may replace the
pending image without dropping those credits. Deferred work remains included
in the existing 256-entry admission budget (248 noncritical entries),
`queued_work_count`, and queue-idle completion; removing an item from the work
deque alone does not finish it. Hidden, rebound, unbound, unregistered, occluded,
or terminally failed surfaces discard their pending work. Successful device
recovery retains it for retry. Shutdown signals the wake event, stops and joins
the Renderer owner, then closes that event; deferred work owns no extra raw
snapshot pointer outside the surface's existing retained owner. Rendering
failures continue through the existing Canvas failure notification.

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

Every visible editor-group Canvas and the Subpalette Canvas has permanent
`WS_HSCROLL | WS_VSCROLL` non-client bars. Their system metrics reduce the Canvas
client rect before that rect becomes the viewport; they are not overlay pixels
and the renderer never draws or hit-tests them. A zero-movement range is
published with the standard disabled-scrollbar state rather than by changing the
window style or hiding a bar. Consequently Fit and 1:1 use one stable final
client extent and do not enter a show/hide feedback loop.

Scrollbar state is a non-authoritative, view-only projection of an accepted
`ViewTransform`. For either device axis, let `q = -pan`, let `[b0,b1)` be the
accepted zoom/flip image bounds before pan, and let `V` be the final client
viewport extent. The base scroll content range is
`[b0 - V/2, b1 + V/2)`, the page is `V`, and the base legal position range is
`[b0 - V/2, b1 - V/2]`. The frontend performs this calculation in checked wide
arithmetic, then requires the legal position and inclusive page-adjusted maximum
to fit the signed 32-bit `SCROLLINFO` domain; an unrepresentable candidate is
rejected atomically. The Rust view keeps the exact transform, so repeated
projection never round-trips the accepted pan through a quantized thumb
position. Its fractional `q` residual is retained across integer line/page/thumb
movement unless an exact endpoint is requested. Thumb tracking reads
`SIF_TRACKPOS` rather than the 16-bit message payload.

Each stable document view and each workspace-local Subpalette view owns a
sticky dynamic range initialized from that base. When accepted `q` crosses a
base or retained endpoint, the crossed side expands past `q` by one current
viewport extent as guard space. Thumb tracking freezes both gesture-start
endpoints so its own projection cannot move the thumb beneath the pointer; other
pan, scrollbar, zoom and resize processing may extend but does not shrink the
range while that interaction is active. At scroll/pan completion, an axis whose
accepted `q` has returned inside its base range may discard its sticky extension;
an axis still outside retains it. If `SB_ENDSCROLL` or pan completion arrives
before the last renderer-accepted transform reaches the owner-thread mailbox, an
axis-local one-shot shrink latch folds that decision into the same checked
two-axis native commit as the next accepted projection. The latch is consumed on
that successful commit whether the final axis is inside or outside base, retained
on apply failure, and cleared by a new interaction or view binding. A successful
Fit, 1:1, explicit view reset, Canvas bind/rebind,
document/source replacement, or Subpalette active-image change discards only
that view's sticky range and initializes it from the newly accepted transform.
Invalid, non-finite, overflowing, stale or cancelled candidates leave the prior
projection intact. Renderer-queue rejection leaves the displayed `SCROLLINFO`
and snapshot intact but does not roll back a Core view input already accepted on
its owner lane; the Canvas blocks another relative scroll until a later accepted
snapshot reconciles the projection. Other tabs, editor groups, workspaces and
auxiliary views are never used as a fallback range.

Core keeps document points/sizes/rectangles, device points/sizes/offsets, and
zoom as distinct private types. Public Rust commands and state accessors retain
their established scalar/record shapes, and the legacy C records retain their
fixed layout beside the additive runtime-ID records; those boundaries validate and convert before calling the typed
`ViewTransform`. Locator sampling, guide/grid snapping, and stroke/effect input
use document points after their single `CoordinateSpace` conversion. Snapshot
raster origins/sizes are likewise typed internally, while raster and
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

Immutable snapshots own raster tiles plus bounded frame, guide, grid, selection,
and preview overlays. Borrowed spans remain valid
only for the FFI snapshot lifetime. The renderer validates counts, strides,
item order, and group structure, and reconstructs GPU resources from the
retained snapshot after device loss. Core geometry remains in document
coordinates; zoom, pan, and flip are render transforms only.

Snapshots also own a bottom-to-top render plan. Layer/plane index 0 is the
palette top, so Core walks each tree in reverse and emits closed layer groups and
raster-tile spans in one semantic order. Layer opacity is applied once by the
group; plane opacity is already resolved into its raster records. The C ABI
publishes only borrowed bounded spans, and the renderer rejects malformed group
structure, kinds, or item ranges before retaining the snapshot. Direct2D draws
the plan sequentially. Device-loss recovery rebuilds the same plan from the
retained snapshot. Thumbnail and flat
export use the same bottom-to-top Core semantics. Raster documents retain the
existing precomposed tile path and its revision-max cache.

### Bounded sequence source render caches

The CPU and GPU caches retain compositions of pristine immutable sequence
sources under independent application-wide limits:

| Cache | Retained source limit | Pixel limit | Budget owner |
|---|---|---|---|
| CPU composed tiles | 64 source reservations | 1 GiB, within the decoded 8 GiB total | Shared `IoManager` plus the Core's reservation ledger |
| GPU source bitmaps | 64 source allocations across Canvas surfaces | 1 GiB, within the GPU tile 1 GiB total | Process-owned `RendererHost` |

The key is `(document UUID, source generation, owner generation)`.
`owner_generation` is a checked, nonzero, process-local namespace for an
independent Core/catalog lifetime. Successful catalog replacement renews it;
staging preserves it, while an independent Core clone gets a new namespace.
Exhaustion disables source-cache reuse instead of wrapping. The identity is
neither persisted nor included in semantic snapshot equality, document digests,
replay, or revision-max. ABI v32's immutable snapshot accessor copies this
provenance without payload reads and without giving the caller a source owner.

Pristine eligibility is established by fresh source activation and checked
against the exact document/cell identity, document revision, history state, and
source generation. It is not inferred from `dirty == false`, a savepoint, or
initial `NOOP`/`BIND`. Edits, native recovery, stroke/filter/floating previews,
and alpha/color-check output cannot enter the pristine bank. Leaving pristine
state for an edit revokes bank admission without clearing unaffected current
tiles; their existing revision-max checks and metadata invalidation still apply.
Normal-view reentry can select the original pristine source only when all of
its eligibility checks hold. An identity flag describes provenance, not a
guarantee that either cache currently retains the source.

The CPU LRU shares immutable tile pixels and renderer-facing tile revisions
with the current snapshot. A retained-source hit restores those same tiles,
then runs the existing scalar validation; it does not recompose their pixels.
Output byte reservations precede composition and bound allocated tile capacity.
A shared reservation is attached to every exported tile before snapshot clones
are published. Pending preparation and outstanding snapshot/tile clones keep
their source count and byte reservation charged, including after catalog/Core
destruction. The last owner releases the charge; removing an LRU entry alone
does not. Foreground admission may evict unreferenced LRU entries, but reservation
failure or a source exceeding the cache cap uses the ordinary active snapshot
path without retention. Standalone unmanaged Rust sources keep the same local
64-source / 1-GiB limit; production file sources additionally use the shared
manager limit. Up to 64 complete inactive editable Core states live in the same
catalog-owned COW resident bank; immutable graph, asset, and tile backing is
shared until the first effective write.

After sequence attachment, all catalog sources up to the 64-source cap are
prepared on the existing bounded Rust I/O worker pool. Preparation reserves
spare cache capacity first, never evicts a foreground entry to speculate, and
uses an immutable source with detached temporary topology. The worker consumes
no persistent IDs and mutates no live Core. Only the Core owner can adopt a
completed result after checking catalog owner, source UUID/generation, and
captured index. Catalog/source replacement cancels or discards old work.
Foreground selection never waits for unfinished preparation; ordinary snapshot
composition remains the fallback. ABI v32 exposes the prepared source set in
one immutable snapshot so the renderer can pre-upload the same full catalog
within its own independent bounds and rebuild it after device loss.

Each Canvas banks GPU tile maps by the full source key and still checks tile ID,
tile revision, and dimensions before reusing a bitmap. Identical UUIDs and
source generations in different Core owners cannot alias. `RendererHost` sums
source allocations/bytes across surfaces, evicts inactive LRU entries first,
and can stop retaining an active source if all remaining candidates are active.
An unretained active image remains subject to the ordinary GPU tile budget;
cache pressure does not change its pixels or view. Rebind, owner changes, device
loss, and teardown discard the affected GPU entries. The CPU cache remains
independent, and a miss or device rebuild follows the normal upload path.
When an edit or preview leaves a pristine source within the same nonzero Windows
presentation epoch, the current GPU tile map can become the ordinary map and
retain unchanged tiles. It no longer counts as a pristine source allocation;
retained source-bank entries remain immutable. A different presentation epoch
invalidates an ordinary map before reuse, including recovery with equal tile
IDs/revisions. The epoch is an invalidation condition, not part of the source
bank's lookup key.

These are derived display caches only. Native v32, document replay epoch 27,
the canonical revision-max formula below, its payload-access gates, and existing
benchmark workloads/envelopes are unchanged. Source-hit and upload counters
describe which work was reused; they do not replace end-to-end Present checks.

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
planes, light-table sets/items, and secondary views. History
state plus document, view, render-cache, and preview revisions are separate
tokens with their own increment policy. A typed cursor allocates the one
document-wide stable-ID namespace through domain-specific methods; there is no
conversion between identity domains. Public Rust records, C records, runtime IDs, and
`.inkpod` DTOs intentionally retain their established `u64` representation and
convert only at those boundaries. The public `Guide` slice and
`LightTableSource` input value remain raw compatibility boundary objects because
changing their stored field types would break the existing Rust API; Core still
allocates guide identities through `GuideId`, and no private layer/plane
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
and potentially long-running raster conversion paths retain their
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

Normal application Save captures a COW `DocumentSaveSnapshot` and owner token
without filesystem access, replay, or encoding on the engine thread. Rust
workers verify cache-free replay and prepare the native file plus its same-stem
raster companion. The explicit synchronous Rust `Core::save` primitive remains
native-only for its existing callers; Batch native output, temporary files,
autosave/recovery, and explicit export do not implicitly become paired saves.
Timer autosave is queued without blocking the UI and is deferred behind a live
stroke. Long-running tasks expose progress and cancellation. Format limits and
recovery details are specified in [`file-format.md`](file-format.md).

The frontend keeps raster-pair authority outside the serialized document as the
closed states `Planned`, `Committed`, and `None`. `Committed` Save targets the
installed pair. `Planned` Save revalidates the raster identity/complete stamp and
the native missing-path proof, then materializes the first pair without opening
Save As. `None` alone asks for an explicit normal destination. For a complete
clean `Committed` pair, an implementation may omit physical rewrites only after
revalidating both identities/complete stamps and external-conflict state; this
optimization is optional, and executing the normal pair transaction remains
conforming. An explicit first Planned Save and a missing-companion repair always
run even when the document is clean. Standalone recovery open and sequence
recovery with pair proof `None` return authority `None`, even when metadata carries
an original/source path, and cannot overwrite that prior pair without an explicit
destination choice. Exact-pair sequence recovery is the narrow exception: after
the shared resolver and all proof/baseline fences agree, it adopts only the
resolved target-specific runtime authority and never trusts the path by itself.
Repair-needed is a missing-raster substate of `Committed`, not a fourth authority;
the recovery metadata's four proof kinds encode missing-member evidence for these
three states.

While any file-I/O operation is pending for the same `DocumentSession`, the
interactive Save and Save As commands are disabled. Their execution path repeats
that check both before showing a destination dialog and after the modal dialog
returns, because a nested message loop can admit new work. A detected conflict is
reported as busy; the explicit request is not silently dropped, retargeted, or
converted into an implicit queued save. Internal Sequence/recovery continuations
may serialize after same-session work only when they retain their captured
session/generation and exact reservation token.

The current `.inkpod` v32 Cell container requires `META`, `GENS`, `ASST`, `PROC`,
and `EDIT`. META section/record schema 2 requires the closed raster-format value:
PNG, TIFF, TGA, or BMP. Raster import records its actual codec; a new cell uses
PNG unless the application supplies a different creation default. Changing that
default does not mutate an existing document, history, or dirty state. Native
reopen, recovery, compaction, and sequence activation preserve the stored format.
The v31 tree simplification advanced replay epoch to 27, document archive schema
to 7, mandatory nested `DOCM` document metadata to schema 8, the document-state
digest to schema 12/domain 10, and canonical snapshot-composite schema to 5.
Batch v5 uses operation schema 4, while the InkScript catalog/owner manifest v5
exposes 73 commands. Normal PNG/TIFF companions retain native 16-bit precision; a 16-bit
document cannot silently become an 8-bit TGA/BMP companion. Explicit display
export retains its separate existing conversion contract.

Native v32 leaves replay epoch 27 and every payload schema unchanged. It admits
an initial-raster source together with `BaseSurface::SolidWhite` only when the
exact imported editable RGBA MainLine is fully opaque. Imports carrying any
non-opaque alpha retain `BaseSurface::Transparent`; in both cases the immutable
source asset materializes Genesis pixels and does not participate in composition.
Lossless raster ingestion means exact canonical decoded dimensions, native 8/16
bit channel depth, straight alpha, and pixel values. It does not promise
byte-for-byte retention or regeneration of the encoded source container,
compression, palette representation, optional metadata, provenance, name, or
path.

Pair paths, filesystem identities, and the `Planned`/`Committed`/`None` state are
runtime authority and are not serialized. The resolver uses existing META field
21, Genesis/assets, and the ordinary composite contract, so this change does not
alter native v32, replay epoch 27, or a section/payload schema. Persisting a pair
path, filesystem identity, or source digest later would require the normal
top-level format-version update.

The codec streams headers, records, asset chunks, procedure payloads, and the
directory without a second complete encoded-file allocation. The I/O manager
owns exclusive same-directory temporary files, flush, sync, close, and
replacement; it never first truncates either destination. Normal native output
contains prospective document/editor savepoints. Both native/raster stages,
verified old-file backups, and a bounded recovery journal are durable before
the owner authorizes installation. The worker revalidates destination proofs
under ordered file locks; owner authorization fences document mutation until
final apply. The pair is one logical normal-save transaction: only successful
installation of both files converts `Planned` to `Committed` and advances the
live path and both savepoints. The physical installation publishes the native
member first and then its raster companion, while neither the intermediate
native-only state nor one installed member is a user-visible success boundary.
Existing or externally changed destinations
require the appropriate overwrite authority; a missing companion can be
regenerated by a normal save without an edit.

The runtime pair journal v2 is published from a flushed same-directory private
stage with the backend's write-through rename before either final changes. Its
phase remains prepared while the two members are installed and verified. Only
after both content/identity proofs, the final bounded alias scan, and directory
durability pass does the worker atomically publish the already-flushed committed
marker. Therefore a crash after both member renames but before that marker rolls
both members back; recovery recognizes completion only from the exact marker.
Cleanup removes the prepared journal before the marker, whose full record also
supports safe orphan-marker cleanup after a crash in that final interval. This
runtime journal revision does not change native Cell format v32.

Installation and rollback do not path-overwrite an existing member after a
check. A durable rollback marker is published first; Windows then verifies and
deletes each expected member through one exclusive handle and publishes its
stage or backup with no-overwrite semantics. A competing external file therefore
wins the path rather than being destroyed, and recovery retains the journal.
The rollback marker also makes a crash between exact deletion and publication
resumable. Non-Windows backends make the final portable stamp recheck immediately
before deletion; eliminating that last external ABA window requires coordination
outside the application process.

Two independent file replacements are not a filesystem-atomic transaction.
Failure recovery uses the recorded proofs to restore the prior pair or recognizes
an already completed pair only from the committed marker. Uncertain recovery
retains its journal and evidence
instead of deleting unverified files or reporting a clean save. Cancellation or
stale owner validation before installation publishes neither destination.

Open validates all container/section/record digests, assets, typed invocation
bytes, references, branch/cursor relationships, high-watermarks, and final
document/editor digests in a staged Core, then swaps once and rebases
`DocumentRevision` to 1. Normal-save output reopens clean with Undo/Redo and
inactive branches intact. Autosave retains the existing normal path/savepoints.
Standalone recovery open and pair-proof-`None` sequence recovery clear both
savepoints, return authority `None`, and mark the restored session dirty;
original/source metadata alone is only a duplicate/conflict hint. Exact-pair
sequence recovery preserves encoded savepoints and adopts target authority only
after the shared resolver baseline and capture-time proof agree; a navigation-only
artifact is clean/non-recovered, while encoded unsaved document/editor differences
remain dirty/recovered. Partial selection revert reconstructs the saved document through
this same current-version reader and commits the selected delta as one new
canonical undo unit.

Whole-document Revert is the explicit ABI v30 `OPEN_NATIVE` request, retained by ABI v32, carrying both
force-reload and `REVERT_CURRENT`; ordinary forced open is not inferred to be a
Revert. Preparation resolves the current native pair and captures its logical
identity. Apply requires the exact live current native path and document UUID,
then replaces the serialized document/history/editor/savepoints while retaining
the runtime sequence catalog, active index, every live stable view ID/logical
view state, the next-view ID, and inactive recovery associations. Render-cache
entries owned by the replaced document revision are invalidated and rebuilt.
After `document_applied`, Windows republishes the
resolved pair identity and shell path, rebases the active `SequenceFileBinding`
to the new owner generation, and refreshes the Sequence projection. A later
snapshot or presentation failure is reported only after that applied authority
has been reconciled; it cannot cause a second Revert or restore the old binding.

Every document replacement revokes the previous native/raster pair proof and
advances its runtime persistence generation at the same commit. Raster import and
real Sequence replacement publish only the target resolver's prepared
`Committed` or `Planned` authority; Light Table swap and pathless Core adoption
publish `None`. Old prepared save tokens can no longer authorize a write for the
new document. Failed replacement retains the existing path and proofs; these
runtime authorities are not part of the native schema or replay digest.

Checkpoint policy is deterministic over procedure count, replay work, and dirty
bytes. A materialized checkpoint is an optimization only: inactive branches and
all retained assets remain rooted by the journal. Explicit compaction first
returns the history-event/procedure counts and document/editor/journal digests
as a confirmation token. Worker preparation builds a detached new-Genesis DTO;
owner validation and an installation fence require that confirmation and the
captured session to remain exact. The worker writes a separate new file without
overwriting an existing destination. Finalization releases the fence without
mutating/adopting the live document, history, path, or savepoints. Compaction
never auto-squashes.
The Windows File menu obtains that token through `CoreHost`, displays both loss
counts before showing the save dialog, rejects a path belonging to any open
session, and reports success without changing current path, dirty state, or
history.

Each successful application autosave is accompanied by a bounded exact-current
version-4 metadata sidecar containing `DocumentSessionId`, generation, document
UUID, original file identity/path, source path, write time, and the capture-time
runtime pair proof kind (`None`, `Committed`, `Planned`, or repair-needed-
`Committed`). Rust owns artifact and
metadata filesystem operations, startup enumeration, and explicit removal.
Startup offers every bounded recovery candidate instead of selecting one newest
file; missing or malformed metadata never causes silent deletion, and
restore/discard/defer is per candidate. A successful append-only write returns an
exact artifact proof binding both the recovery native and sidecar complete stamps;
explicit switch/read/discard validates both members before and after locked access.
Original identity metadata alone describes the source association but never grants
normal-save path authority; only the exact-pair sequence resolver contract above
may re-adopt target-specific runtime authority. The artifact is
retained until explicit discard or successful normal save removes it and its
sidecar. Closing a live document session does not silently delete an inactive
sequence-cell artifact that has not reached either boundary. The in-memory
sequence association ends, while startup enumeration rediscovers the native and
metadata pair as a standalone, pathless authority-`None` recovery candidate with
its exact history and EditorState. Autosave does not advance the normal savepoint.
Workspace layout never contains document paths. A separate bounded current-version binary path record at
`%LOCALAPPDATA%\inkpod\Session\inkpod-session.bin` is read and written only when
the default-off `起動時に前回の文書を復元` setting is enabled; crash recovery remains
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
manifests, and the lockfile. Its narrow file/token allowlist permits filesystem
identity, coordination, and replacement only in the private `inkpod-io` backend;
Core, image, format, and FFI domain code remain OS-independent. Windows-specific
crate dependencies, GUI/COM/renderer access, and OS types in public I/O APIs
remain forbidden. A future frontend uses the same owned path/stream contracts
with an appropriate private filesystem backend; platform UUID creation,
font/GPU resource resolution, clipboard, and picker adapters remain frontend
responsibilities.

The `large_document` image benchmark exercises maximum-dimension sparse
allocation, distributed writes, copy-on-write isolation, and a bounded dense
filter workload. The `core_workflows` benchmark separately covers sparse and
dirty-tile snapshots, view-only cache reuse, Undo/Redo, light-table composition,
and in-memory Batch preview/dry-run. Both expose
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
