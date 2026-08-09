# Native file format

`.inkpod` v9 is the bounded, procedure-authoritative, little-endian native
container. Version 9 and replay epoch 6 are the only accepted native contract.

Until the user explicitly declares a format freeze, Inkpod accepts only the
current version of each application-owned file format. It provides no older-
version reader, writer, migration path, or compatibility shim. Every serialized
schema change before code freeze must increment the format's top-level version;
changing only a section or record version is not a substitute. The current
schema should be replaced whenever a more robust or efficient design is found.

## Current procedure-authoritative contract

This section defines the implemented procedure-authoritative container at
top-level format version 9 and replay epoch 6. It uses a hierarchical document
commitment, an exact-depth target-explicit `ApplyRasterStroke/v2` schema, a
distinct stable Cell ID, and an explicit immutable Genesis base surface:
metadata, raster, and raster-tile commitments are domain-separated so a raster
edit hashes only changed tile payloads instead of every allocated document
pixel. This semantic digest is independent of the renderer's canonical
revision-max cache identity. Asset and procedure-payload digest contracts are
version 1. Every version other than 9 is rejected before Core state replacement;
there is no migration or compatibility reader. Any schema or replay-semantics change
after this contract increments the top-level version before that change is
merged. A replay-result change also increments the replay epoch.

The authoritative sections are `META`, `GENS`, `ASST`, `PROC`, and `EDIT`.
`EXTM`, `CKPT`, and unknown opaque-preserve sections are optional. `CKPT` is a
schema-1 acceleration record in v9. There is no `HIST` section: history moves and
branch cuts are records in `PROC`, while cursor, active branch, savepoints, and
ID high-watermarks are fields in `META`. A materialized document or checkpoint
is never sufficient without Genesis, retained assets, and the procedure/control
event journal.

### Canonical scalar and rounding contract

- All serialized integers are little-endian fixed-width values. Boolean values
  are `u8` 0 or 1; other values are invalid. Reserved bits and bytes are zero.
- Canonical document geometry uses a signed `i64` with 16 fractional bits:
  one document pixel is 65,536 units. The inclusive supported input range is
  -16,777,216 through +16,777,216 document pixels before scaling; individual
  primitives may impose the tighter existing vector or image bounds.
- Pixel indices and half-open raster bounds remain integral. A point exactly on
  the lower/right document edge is outside the raster; a finite point inside a
  pixel cell maps with mathematical floor after view-to-document conversion.
- Finite IEEE-754 input is converted by decoding its sign, exponent, and
  significand, scaling by 65,536, and rounding to nearest with ties to even.
  Negative zero becomes zero. NaN, infinity, and a scaled result outside `i64`
  or the primitive bound are invalid. Canonicalization must not use locale,
  host floating-point rounding mode, or a platform `libm` result.
- Exact colors remain tagged RGBA8 or RGBA16 straight-alpha sRGB. Normalized
  pressure, opacity, and unit-interval parameters use `u16` 0 through 65,535;
  intermediate division uses round-to-nearest, ties-to-even unless a primitive
  schema explicitly fixes another integer rule. DPI remains positive `u32`
  thousandths. Angles use `u32` turns where 2^32 units equal one full turn.

These rules canonicalize procedure arguments. Display-only device coordinates,
OS DPI, monitor state, and renderer antialiasing are not journal fields.

### Stable primitive namespace and replay versioning

`PrimitiveId` is a nonzero `u32`; `PrimitiveSchemaVersion` is a nonzero `u16`.
IDs are never renumbered or reused, and a removed primitive leaves a tombstone
in the primitive catalog. The family allocation is:

| Inclusive ID range | Family |
|---|---|
| `0x0001_0001..0x0001_FFFF` | document, paper, frame, and document metadata |
| `0x0002_0001..0x0002_FFFF` | layer and plane topology |
| `0x0003_0001..0x0003_FFFF` | palette and document color metadata |
| `0x0004_0001..0x0004_FFFF` | guides and grid |
| `0x0005_0001..0x0005_FFFF` | raster paint, fill, cleanup, filters, effects, alpha, and adjustment |
| `0x0006_0001..0x0006_FFFF` | selection and floating-selection commit |
| `0x0007_0001..0x0007_FFFF` | document transforms |
| `0x0008_0001..0x0008_FFFF` | vector document edits |
| `0x0009_0001..0x0009_FFFF` | common-raster import document edits |
| `0x000A_0001..0x000A_FFFF` | Light Table document edits |
| `0x8000_0000..0xFFFF_FFFF` | reserved; never emitted by the built-in catalog |

`0`, family headers `0x0001_0000` through `0x000A_0000`,
`0x000B_0000..0x7FFF_FFFF`, and every otherwise unassigned value are reserved
and invalid in a procedure. Values in
`0x8000_0000..0xFFFF_FFFF` are also invalid in a built-in file; reserving that
range does not define an extension mechanism. A reader accepts only IDs present
in the exact catalog named by the header.

Every production document mutation has a stable ID. The complete source catalog
is the `PrimitiveId` constants in `inkpod-core`; it occupies the family ranges
above without renumbering existing assignments:
`SetMainLineColor` is `0x0003_0001/v1`, `ReplacePalette` is
`0x0003_0002/v1`, `ApplyRasterStroke` is `0x0005_0001/v2`, and
`ImportRasterAsset` is `0x0009_0001/v1`. Other typed invocation records use the
current canonical schema v2: document geometry is signed Q16, normalized pressure
is `u16`, and angles are `u32` turns. `LightTableSwapWithActive` has stable ID
`0x000A_0015`, but replaces the session Genesis and resets history rather than
creating a `HistoryEntry`. Adding a primitive consumes a new ID.
Changing only its canonical argument layout while preserving the exact
semantics increments its schema version. Changing validation, rounding, pixels,
IDs, state digest, or any other replay result increments both `ReplayEpoch` and
the top-level format version. Before format freeze the reader accepts only the
exact current top-level version, replay epoch, primitive catalog digest, and
primitive schema set; no compatibility decoder is retained.

`semantics revision` is a nonzero `u32`, begins at 1, and increments whenever
normative validation, state-transition, pixel, allocation, no-op, or work-charge
semantics change. An argument-layout-only change that provably leaves every
accepted input and replay result identical increments only the primitive schema
version. Any change that can alter acceptance, an output ID, state bytes, or a
digest increments semantics revision, `ReplayEpoch`, and the top-level format
version together. A wording correction that changes no normative rule changes
none of them. Revisions are never decremented or reused.

The primitive catalog digest is computed over ascending entries containing
primitive ID, schema version, canonical name, argument-schema digest, semantics
revision, work-formula ID, and replay-policy byte (`1` journal-replayable, `0`
session Genesis replacement). Query, view, transient, ingestion, export, and
application command IDs are not `PrimitiveId` values.

The four procedures below have fully specified byte schemas. V9 stores bounded
canonical bytes for every typed invocation and retains that typed value as the
runtime replay authority. Its decoder/encoder connects all invocation variants
through the kind-7 canonical-invocation envelope described below. The two
metadata primitives use v1, `ApplyRasterStroke` uses exact-current v2, and
`ImportRasterAsset/v1` supports the inline-or-asset representation for samples:

| Primitive | Canonical input-ID roles | Canonical asset roles | Canonical arguments | Inline payload | Work formula ID |
|---|---|---|---|---|---:|
| `SetMainLineColor` | none | none | ordinal 1 = tagged exact-depth color | empty | 1 |
| `ReplacePalette` | none | none | ordinal 1 = ordered color sequence (`u64` count, then length-framed tagged colors) | empty | 2 |
| `ApplyRasterStroke` | role 1 = target Plane ID | role 1 = one `CanonicalSampleStream` exactly when the logical sample payload exceeds 4 MiB | ordinals 1 target Plane ID `u64` (must equal input role 1), 2 tool `u32` (1 Pencil, 2 Brush, 3 Eraser), 3 tagged exact-depth color, 4 positive Q16 diameter `i64`, 5 auto-erase boolean, 6 pressure-size boolean | `u64` sample count, then exactly 24 bytes per sample when the logical payload is at most 4 MiB; otherwise empty and asset role 1 carries those exact logical bytes | 3 |
| `ImportRasterAsset` | role 1 = target Plane ID | role 1 = one `CanonicalRaster` | ordinal 1 = target Plane ID `u64` (must equal input role 1) | empty | 4 |

All stroke samples are canonical document coordinates; role-based active plane,
view/device coordinates, pan/zoom/flip, and OS DPI are resolved before the
procedure is formed. Work formulas 1 through 4 are respectively constant 1,
`1 + palette entry count`, sample count plus every clipped dab-bounding-box
pixel tested, and `1 + referenced raster logical element count`. The metadata
primitives retain semantics revision 3;
`ApplyRasterStroke/v2` has semantics revision 4 because it accepts and preserves
RGBA16 in addition to RGBA8 and can therefore change exact state bytes and replay
results. `ImportRasterAsset/v1` begins at semantics revision 1. All four emit no
output IDs. The first two use no assets; stroke uses exactly one inline/asset
representation, and import requires exactly one raster asset.

For pressure canonicalization, the frontend value is an IEEE-754 binary32 or
binary64 value in the closed interval 0 through 1. Its exact rational value is
multiplied by 65,535 and rounded to nearest with ties to even; negative zero
becomes zero. NaN, infinity, or an out-of-range value is invalid. A diameter
source must be finite and greater than 0 through 256 pixels, preserving the
existing public validation range; after the Q16 conversion its canonical value
is `max(1, rounded_q16)`, hence 1 through 16,777,216. This single sub-Q16 clamp
preserves acceptance of an existing positive subquantum diameter, whose dab
radius is already zero. Canonicalization failure publishes no procedure or
state.

`ApplyRasterStroke/v2` fixes the following integer raster semantics. These are
the journal semantics; begin/append preview batching cannot change them.

1. Decode a nonzero stable target Plane ID from argument ordinal 1, require it
   to equal input-ID role 1, then resolve and validate that exact plane before
   execution. MainLine BinaryMask8/Grayscale8/Grayscale16 draw values are the
   format maximum. Color and raster StraightRgba8 planes retain RGBA8 directly
   and reduce RGBA16 by `(channel + 128) / 257`; StraightRgba16 retains RGBA16
   directly and expands RGBA8 by multiplication by 257. Alpha follows the same
   exact conversion. Eraser uses the all-zero value. Other formats, a missing
   or noneditable plane, a role/argument mismatch, or a stale base is invalid.
2. Convert a Q16 center to a raster cell with mathematical floor division by
   65,536, including for negative values. Pencil always has radius zero. For
   Brush/Eraser let `p' = pressure_size ? max(pressure, 655) : 65,535`, let
   `d' = round_ties_even(diameter_q16 * p' / 65,535)`, and let radius be
   `max(0, ceil((d' - 65,536) / 131,072))`. All products use checked signed
   128-bit intermediates.
3. Compute the maximum radius with the greatest canonical sample pressure and
   clip each consecutive Q16 segment to the closed expanded center rectangle
   `[-r*65,536, (width+r)*65,536]` by exact-rational Liang-Barsky clipping.
   Parallel-outside segments are empty. Compare rational clip parameters by
   cross multiplication; interpolate clipped x/y and pressure once, rounding
   to nearest with ties to even. No floating-point or `libm` operation occurs.
4. Rasterize clipped endpoint cells with the signed Bresenham recurrence:
   `dx=abs(x1-x0)`, `sx=(x0<x1?1:-1)`, `dy=-abs(y1-y0)`,
   `sy=(y0<y1?1:-1)`, `error=dx+dy`; visit the current cell, then update x when
   `2*error>=dy` and y when `2*error<=dx`, until the end cell is visited. Let
   `steps=max(dx,-dy,1)` and at visited step `i` use
   `round_ties_even((p0*(steps-i)+p1*i)/steps)`. A one-sample stroke rasterizes
   that sample once. Each later segment includes both endpoints, matching the
   interactive continuation contract.
5. For every dab, enumerate the complete `(2r+1)` square in ascending y offset
   then ascending x offset. Every enumerated candidate consumes one primitive
   work unit. Paint it only when `xoff^2+yoff^2<=r^2` and the cell is within
   `[0,width) x [0,height)`. Checked 128-bit arithmetic is used for squares and
   work. Multiple visits are allowed; the last identical desired value wins.
6. Pencil auto-erase samples the first canonical sample cell before staging. If
   it is in bounds and already equals that stroke's draw value, the whole
   stroke uses the erase value; otherwise it uses the draw value. Auto-erase is
   ignored by Brush/Eraser. A completed stroke that changes no pixel is a
   semantic no-op and emits no Commit, ID, revision, history, or dirty change.

The rational clipping endpoints and pressure are reduced only for overflow-safe
comparison; reduction does not alter their value. A zero denominator, overflow,
resource-limit excess, or inability to represent an interpolated canonical
value rejects the whole staged primitive. The integer rules are the sole pixel
and work-result authority; no floating-point rasterizer is a second owner.

The schema-1 argument `value-kind` catalog is closed: 0 is the absent sentinel,
1 Boolean (one byte, exactly 0 or 1), 2 U32 (four bytes), 3 Q16 document scalar
(signed `i64`), 4 TaggedColor (tag 1 plus four RGBA8 bytes or tag 2 plus four
little-endian RGBA16 channels), and 5 OrderedColorSequence (`u64` count, then
`u64` element length plus one TaggedColor per element), 6 StableObjectId
(one nonzero little-endian `u64` in the document-wide namespace), and 7
CanonicalInvocation (the exact bounded `canonical_arguments` byte string).
Kind 7 is the persistence envelope used by the complete current primitive
catalog: a Commit has either no argument record or exactly ordinal 1/kind
7/present. The selected `PrimitiveId` and schema version determine the typed
decoder. Decode must consume every byte, resolve required assets, and re-encode
to identical canonical bytes before replay. Kind 0 is forbidden in a present
argument; any other code is invalid.
`ApplyRasterStroke/v2` accepts kind-4 tag 1/length 5 and tag 2/length 9 without
reducing its canonical argument depth. The two color metadata primitives also
accept either tag without depth conversion.

Stable object-kind codes are 1 Document, 2 Project, 3 Cut, 4 Cell, 5 Frame, 6
Sequence, 7 Layer, 8 Plane, 9 Guide, 10 LightTableSet, 11 LightTableItem, 12
Adjustment, 13 VectorPath, and 14 VectorFill. Zero and unlisted codes are
invalid. `ApplyRasterStroke/v2` input role 1 is required, has object kind 8,
and names the exact target plane; the other primitives in the table have no ID roles.

### Persistent identity and journal ordering

Every persistent numeric ID is a little-endian nonzero `u64`. Genesis is
`StateId(1)` on root `BranchId(1)`; the next state and branch values begin at 2.
`ProcedureId` and `JournalEventId` begin at 1. `PrimitiveId` is the one stated
`u32` exception, and `AssetId` is the full 32-byte `AssetDigest` rather than a
numeric ID. Optional IDs use an explicit presence field and never encode zero as
`None`.

A document-wide stable-object cursor begins at 1 and allocates typed document,
project, cut, cell, frame, sequence, layer, plane, guide, Light-Table set/item,
adjustment, vector-path, and vector-fill IDs from one non-reusing namespace.
The object kind is part of every typed reference, so equal numeric values of
different kinds are invalid even though allocation uses one cursor. Each
`next_*` value in `META` is the first unallocated value, not the last allocated
value, and may not exceed `0x7FFF_FFFF_FFFF_FFFF`.

`DocumentRevision` is not serialized. A newly created or successfully opened
Core rebases it to 1 and increments it for each committed document change or
actual history move in that Core generation. `EditorRevision` is a persisted,
nonzero monotonic `u64`, starts at 1, and increments only for a semantic
EditorState change. Persistent preconditions, history, and document savepoints
use `StateId`; EditorState equality/savepoint uses `EditorStateDigest`.

`PROC` is an append-only sequence of the following closed records. Each uses
the common 16-byte record header defined below, then these exact payload bytes:

- `Commit` has a 184-byte fixed prefix: `JournalEventId u64`, `ProcedureId
  u64`, `PrimitiveId u32`, primitive schema `u16`, zero `u16`, replay epoch
  `u32`, zero `u32`, parent `StateId u64`, committed `StateId u64`, post-commit
  `BranchId u64`, argument/asset/input-ID/output-ID counts as four `u32`,
  argument bytes length `u64`, inline payload length `u64`,
  `DocumentStateDigest` before (32 bytes), `DocumentStateDigest` after (32
  bytes), and `ProcedurePayloadDigest` (32 bytes). It is followed, in order,
  by argument records, asset-reference records, input-ID records, output-ID
  records, then inline payload bytes.
- A canonical argument record is `ordinal u32`, `value-kind u16`, `presence
  u8`, zero `u8`, `length u64`, and `length` bytes. Current Commit records use
  zero records for an empty canonical invocation or exactly ordinal 1, kind 7,
  presence 1 for its complete canonical invocation. The concatenated argument
  records must equal the prefix length exactly.
- An asset-reference record is `argument ordinal u32`, zero `u32`, then the
  32-byte `AssetId`. Input/output-ID records are `role ordinal u32`, reserved
  zero `u32`, and stable ID `u64`; the typed canonical invocation is the object-
  kind authority in v9. Each sequence is strictly increasing
  by ordinal with no duplicate role. The primitive schema fixes whether each
  input/output role is required; transient object IDs are forbidden.
- When payload length is zero, `ProcedurePayloadDigest` is 32 zero bytes and no
  hash is compared. For a nonempty payload it is the digest of the one-field
  canonical digest message whose field 1 is the exact inline payload bytes, and
  it must match; a computed digest that happens to be all zero remains valid
  because length, not the digest value, distinguishes absence. The procedure
  schema defines the payload's canonical element framing.
- `HistoryMove` is exactly 40 bytes: `JournalEventId u64`, kind `u8` where
  1/2/3 = Undo/Redo/Jump, seven zero bytes, source `StateId u64`, destination
  `StateId u64`, and the post-move active `BranchId u64`.
- `BranchCut` is exactly 40 bytes: `JournalEventId u64`, fork `StateId u64`,
  old active-tail `StateId u64`, new `BranchId u64`, and deactivated
  `BranchId u64`. The fork belongs to the deactivated branch, the new branch is
  previously unused, and the immediately following Commit uses that new branch
  and names the fork as parent.

Event IDs are strictly increasing by one and equal file order. Commit procedure
and committed-state IDs are also strictly monotonic in their own namespaces.
An Undo/Redo/Jump no-op emits nothing. Undo and Redo move only along the active
branch. Jump may name any existing destination plus a post-move active branch
whose ancestry path contains that destination; all other combinations are
invalid. Whenever the cursor is not that active branch's tail, a new edit stages
one `BranchCut` followed immediately by one `Commit`; their two event IDs are
adjacent, the commit names only the new branch, and the cut is not duplicated in
the commit. A commit at the active tail has no cut. Validation, execution, both
records, state/history publication, and all high-watermark changes form one
atomic publish batch. Failure, cancellation, stale base, overflow, or semantic
no-op consumes no ID and appends no record.

### Digest framing

All semantic and container digests are BLAKE3-256 in derive-key mode. The exact
ASCII context strings are:

| Digest | Derive-key context |
|---|---|
| `DocumentStateDigest` | `org.inkpod.digest.document-state.v4` |
| document metadata commitment | `org.inkpod.digest.document-metadata.v1` |
| document raster commitment | `org.inkpod.digest.document-raster.v1` |
| document raster-tile commitment | `org.inkpod.digest.document-raster-tile.v1` |
| `EditorStateDigest` | `org.inkpod.digest.editor-state.v1` |
| `JournalPrefixDigest` | `org.inkpod.digest.journal-prefix.v1` |
| `AssetDigest` / `AssetId` | `org.inkpod.digest.asset.v1` |
| asset chunk | `org.inkpod.digest.asset-chunk.v1` |
| `ProcedurePayloadDigest` | `org.inkpod.digest.procedure-payload.v1` |
| stored section bytes | `org.inkpod.digest.section-stored.v1` |
| logical section bytes | `org.inkpod.digest.section-logical.v1` |
| `FileRootDigest` | `org.inkpod.digest.file-root.v1` |
| primitive catalog | `org.inkpod.primitive-catalog.v1` |
| primitive argument schema | `org.inkpod.primitive-argument-schema.v1` |
| canonical snapshot composite | `org.inkpod.digest.canonical-composite.v1` |

Every document/container digest message uses the same canonical frame shape.
The primitive catalog and canonical snapshot composite instead use their closed
ordered streams described in their respective sections. A framed message starts with a
digest-schema version `u32` and field count `u32`. `DocumentStateDigest`, the
metadata commitment bytes, raster commitment bytes, and every canonical
document-state frame nested within them use schema version 4. A raster-tile
commitment and every other digest message in the table, including the separately
hashed `AssetDigest` and `ProcedurePayloadDigest`, uses schema version 1. Each field,
in consecutive ordinal order starting at 1, is `ordinal u32`, `presence u8`,
three zero bytes, byte length `u64`, then exactly that many bytes. Presence is
0 or 1. Absent is presence 0 plus length 0; present-empty is presence 1 plus
length 0, so the two never collide. A required field is always present. No
padding occurs between fields.

The canonical snapshot-composite stream begins with schema `u32 = 1`, semantic
snapshot feature flags, document width/height, then the public tile sequence in
`(origin y, origin x, tile ID)` order. Each tile contributes ID, origin, dimensions, stride, byte count,
and exact premultiplied BGRA8 bytes; cache/source revisions are excluded. It then
contains the ordered public vector segments and fills. Segment point/width values
are canonicalized to signed Q16 before hashing; fill boundary path IDs retain
their semantic order. View revision, zoom, pan, flip, guides, grid, renderer
resources, and OS DPI are excluded. Vector segments use
`(z-order, plane ID, path ID, segment index)` order and fills use
`(z-order, plane ID, fill ID)` order. `RenderSnapshot::canonical_composite_digest`
and `inkpod_snapshot_get_canonical_digest` expose this result without a test-only
state accessor.

A sequence field is `element-count u64`, then for every element `element-length
u64` and exact element bytes. A schema-declared ordered sequence retains its
semantic order. A set is sorted by unsigned lexicographic element bytes. A map
is sorted by unsigned lexicographic canonical key bytes; duplicate canonical
keys are invalid. Fixed-width integers are little-endian. UUID is the 16 RFC
4122 network-order octets. UTF-8 is valid, unnormalized, and unterminated; its
field/element length is the byte count. Nested values use this same frame with
their own schema version. Unknown ordinals, missing required ordinals, nonzero
reserved bytes, noncanonical order, and alternative encodings of the same value
are invalid.

The exact digest messages are:

| Digest | Present fields in ordinal order |
|---|---|
| `DocumentStateDigest` | 1 32-byte document metadata commitment; 2 Plane-ID-sorted sequence of frames containing stable Plane ID then 32-byte raster commitment |
| document metadata commitment | 1 document UUID; 2 stable Document ID; 3 paper; 4 frames/margins; 5 base surface; 6 ordered layer/plane tree; 7 persistent selection record; 8 palette; 9 main-line color; 10 guides; 11 grid; 12 Light Table; 13 project/cut/cell/frame/sequence identities and animation metadata; 14 required-extension sequence, empty in schema 4 |
| document raster commitment | 1 width; 2 height; 3 canonical pixel format; 4 tile edge; 5 `(tile_x, tile_y)`-sorted sequence of tile coordinate, valid width/height, and 32-byte raster-tile commitment |
| document raster-tile commitment | 1 canonical pixel format; 2 tile x; 3 tile y; 4 valid width; 5 valid height; 6 exact valid row-major pixel bytes without row padding |
| `EditorStateDigest` | 1 editor-state schema `u32 = 1`; 2 active tool; 3 optional last color-consuming tool; 4 tool-keyed colors; 5 tool-keyed diameters; 6 fill options; 7 selection options; 8 vector options; 9 optional active layer; 10 optional active plane; 11 optional palette cursor; 12 editor-target/option records |
| `JournalPrefixDigest` | 1 last included event count `u64`; 2 ordered sequence of the exact common-header-plus-payload `PROC` record bytes from event 1 through that count |
| `AssetDigest` | 1 asset schema `u32 = 1`; 2 asset kind `u32`; 3 optional pixel format; 4 optional color space; 5 optional alpha semantics; 6 optional width; 7 optional height; 8 optional canonical stride; 9 logical element count `u64`; 10 logical payload length `u64`; 11 exact canonical logical payload bytes |
| asset chunk | 1 `AssetId`; 2 chunk index `u32`; 3 logical offset `u64`; 4 exact chunk bytes |
| `ProcedurePayloadDigest` | 1 exact nonempty inline payload bytes |
| stored section | 1 FourCC bytes; 2 section version `u16`; 3 compression code `u32`; 4 exact on-disk section bytes |
| logical section | 1 FourCC bytes; 2 section version `u16`; 3 exact post-decompression logical bytes |
| `FileRootDigest` | 1 complete 128-byte header with only its FileRootDigest bytes zero; 2 exact directory bytes |
| primitive catalog | 1 ordered sequence of catalog-entry frames |

A primitive catalog entry stream has fields 1 `PrimitiveId u32`, 2 schema
version `u16`, 3 length-prefixed canonical ASCII name, 4 32-byte argument-schema
digest, 5 semantics revision `u32`, 6 work-formula ID `u32`, and 7 replay-policy
`u8`. Entries are strictly ascending by PrimitiveId. In the build gate the
argument-schema digest is BLAKE3 `derive_key` over the exact canonical ASCII
label `<canonical-name>/canonical-v<schema-version>` using the primitive-
argument-schema context above. This label identifies the closed typed schema;
the v9 reader selects its decoder through the same catalog entry and accepts
only a byte-exact canonical re-encoding.

An argument descriptor is a schema-1 frame with fields 1 ordinal `u32`; 2
value-kind `u16`; 3 presence policy `u8` (1 Required, 2 Optional); 4 minimum
encoded value length `u64`; 5 maximum encoded value length `u64`; 6 optional
inclusive lower-bound value encoded in that same value kind; 7 optional
inclusive upper-bound value; 8 element value-kind `u16` (zero for a scalar); 9
minimum element count `u64`; and 10 maximum element count `u64`. Bounds are
absent for nonnumeric values; scalar element counts are both zero. An asset-role
descriptor is a schema-1 frame of role ordinal `u32` and presence policy `u8`.
An ID-role descriptor is a schema-1 frame of role ordinal `u32`, object-kind
`u32`, and presence policy `u8`. All descriptor sequences use consecutive
ordinals from 1.

The payload descriptor is a schema-1 frame with fields 1 payload schema `u16`;
2 fixed element size `u32` (zero when not fixed); 3 minimum element count `u64`;
4 maximum element count `u64`; 5 minimum byte length `u64`; and 6 maximum byte
length `u64`. Payload schema 0 requires every other field to be zero and means
the payload must be empty. For `ApplyRasterStroke/v2`, payload schema is 1,
element size is 24, element count is 1 through 1,048,576, minimum length is 32,
and maximum length is 25,165,832 (`8 + 24 * 1,048,576`). Its count prefix is
outside the fixed-size elements. When the logical payload exceeds 4 MiB, the
procedure's inline bytes are empty and its single sample asset must validate
against this same descriptor and count. `SetMainLineColor`, `ReplacePalette`,
and `ImportRasterAsset` have schema-0 empty payload descriptors.

The exact current first-slice argument descriptors are: `SetMainLineColor`
ordinal 1, kind 4,
Required, length 5..9; `ReplacePalette` ordinal 1, kind 5, Required, length
8..69,640, element kind 4, count 0..4,096; `ApplyRasterStroke/v2` ordinal 1
kind 6 length 8 lower/upper 1/`0x7FFF_FFFF_FFFF_FFFF`, ordinal 2 kind 2 length
4 lower/upper 1/3, ordinal 3 kind 4 length 5..9, ordinal 4 kind 3 length 8
lower/upper 1/16,777,216, and ordinals 5 and 6 kind 1 length 1..1;
`ImportRasterAsset/v1` ordinal 1 kind 6 length 8 lower/upper
1/`0x7FFF_FFFF_FFFF_FFFF`. All are Required; omitted bounds are absent and
scalar element counts are zero. For stroke and import, ordinal 1 must equal
input-ID role 1 in addition to satisfying the descriptor. Stroke has optional
asset role 1 and import has required asset role 1; their permitted kinds and
inline/asset XOR are the catalog semantics above. Catalog names are the exact
ASCII bytes `SetMainLineColor`, `ReplacePalette`, `ApplyRasterStroke`, and
`ImportRasterAsset`. This descriptor frame, rather than a Rust type name or
layout, is the sole argument-schema-digest input.

`DocumentStateDigest` excludes document/editor revisions, history, paths, views,
transient sessions, allocation/tile-cache state, and caches. Its schema-4 root
commits to one metadata digest and every semantic raster digest by stable Plane
ID. The runtime may cache that tree at a matching document revision, but the
revision and cache layout never enter a digest. A changed raster tile replaces
that tile commitment, its raster commitment, and the root; unchanged tile
payloads are neither copied nor hashed. Its nested schema is canonical as
follows:

- Paper is a frame ordering width/height `u32`, DPI x/y thousandths `u32`, and
  color-space code `u32 = 1` for sRGB. Frames/margins order the hundred,
  reference, drawing, and safe rectangles, then left/top/right/bottom margins.
  A rectangle is Q16 x/y/width/height in that order and each margin is signed
  Q16. The base surface is
  discriminant 1 `SolidWhite` or 2 `Asset`, followed by an `AssetId` only for
  discriminant 2.
- Layers and planes retain document stacking order. A layer frame orders stable
  ID, kind, UTF-8 name, visible, editable, normalized `u16` opacity, ordered planes,
  and optional adjustment. A plane frame orders stable ID, kind, pixel format,
  UTF-8 name, visible, editable, normalized `u16` opacity, optional raster, ordered
  vector paths, and ordered vector fills. The closed plane-kind codes are 1
  MainLine, 2 Color, 3 Raster, 4 Selection, 5 VectorMainLine, 6 ColorTrace, and
  7 VectorFill. Layer-kind codes are 1 BinaryColoring, 2 GrayscaleColoring, 3
  Raster, 4 Selection, 5 Frame, 6 VanishingPoint, 7 Adjustment, 8 Text, 9
  Annotation, and 10 VectorColoring.
  Existing thousandth opacity is canonicalized as
  `round_ties_even(opacity_milli * 65,535 / 1,000)` after validating 0..1,000.
- A raster's metadata frame orders width `u32`, height `u32`, canonical pixel
  format `u32`, tile edge `u32 = 64`, and a present-empty tile sequence. Pixel
  content occurs exactly once in the separate raster commitment. That
  commitment uses the same four structural fields and tiles sorted by
  `(tile_x, tile_y)`; each record stores tile coordinates, valid width/height,
  and the domain-separated tile commitment. The tile commitment hashes its
  format, coordinates, valid dimensions, and only valid row-major pixels with
  no row padding. Missing tiles mean all-zero pixels; all-zero stored tiles are
  forbidden. Pixel codes 1/2/3/4/5 are BinaryMask8,
  Grayscale8, Grayscale16 little-endian, straight RGBA8, and straight RGBA16
  little-endian. Premultiplied BGRA is display-only and invalid here.
- The layer/plane tree contains editable document planes only. The persistent
  selection record is a separate frame ordering selection Plane ID (object kind
  8) and one BinaryMask8 raster frame; that ID may not occur in the editable
  plane tree. Thus selection bytes occur exactly once and every selection
  change changes the document digest. The stable Document ID in field 2 and all
  tree/selection IDs must be distinct members of the one document namespace.
- A color is tag `u8` 1 or 2 followed by RGBA8 bytes or four little-endian
  RGBA16 channels. Palette order is semantic and retained. A vector path orders
  path/owner IDs, color, closed boolean, and cubic segments; each segment orders
  Q16 p0/p1/p2/p3 x/y then Q16 start/end width. A vector fill orders fill/owner
  IDs, color, and boundary path IDs in boundary order.
- Guides retain display order and order each stable ID, axis code, and Q16
  position; axis 1/2 is Horizontal/Vertical. The Light-Table frame orders an
  optional active-set ID and ordered sets. A set orders ID, UTF-8 name, normalized
  `u16` global opacity, and ordered items. An item orders ID, canonical AssetId,
  the source reference origin as Q16 x/y, UTF-8 name, visible,
  normalized `u16` opacity, display-mode code 1/2/3 for
  Color/Monotone/Halftone, exact display color, Q16 translate x/y and scale x/y,
  then `u32` turn rotation. The source UUID, source revision, source DPI, and
  reference-frame extent are provenance and are excluded; the source reference
  origin is semantic
  alignment state and therefore is not provenance.
  The grid frame orders Q16 origin x/y, positive Q16 spacing x/y, then nonzero
  subdivisions `u32`; current integer grid values are multiplied by 65,536.
  The hierarchy frame orders optional Project ID, optional Cut ID, required
  Cell ID, ordered animation-frame records, and ordered sequence records. A
  frame record orders Frame ID and zero-based display ordinal `u32`. A sequence
  record orders Sequence ID, UTF-8 name, and an ordered sequence of Frame IDs;
  each referenced frame must occur in the frame-record sequence. Core
  allocates a distinct persistent Cell ID in the document namespace and places
  it in the required Cell slot, with absent Project/Cut IDs and empty frame/
  sequence lists for the current standalone-cell model. Document, Cell, layer,
  plane, selection, and other stable object IDs therefore obey cross-kind
  numeric-ID uniqueness. This document-state frame is schema/domain 4. The
  current build contract is replay epoch 6 and top-level version 9; optional
  checkpoint/streaming records do not change the state-digest schema.
  Collections whose UI order is not semantic are ID-sorted.
- An adjustment frame orders kind, channel, interpolation, six signed `i32`
  parameters, and an ordered point sequence of `(input u16, output u16)`.
  Kinds 1/2/3 are BrightnessContrast, ToneCurve, and Levels; channels 0/1/2/3/4
  are NotApplicable/Rgb/Red/Green/Blue; interpolation 0/1/2 is
  NotApplicable/Bezier/BSpline. Unused parameters are zero. Brightness/contrast
  occupy parameters 1/2; Levels orders input shadow, input gamma thousandths,
  input highlight, output shadow, and output highlight in parameters 1..5.

The canonical EditorState frame has schema 1 and exactly twelve fields in
this order: 1 schema `u32 = 1`; 2 active `ToolId u32`; 3 optional last
color-consuming `ToolId`; 4 tool-color sequence; 5 tool-diameter sequence; 6
fill-options frame; 7 selection-options frame; 8 vector-options frame; 9
optional active Layer ID; 10 optional active Plane ID; 11 optional palette-
cursor frame; and 12 a required present-empty editor-option sequence. Fields 9
and 10 are either both present as nonzero IDs in the same document namespace or
both absent. The palette cursor orders zero-based group and entry `u32` values,
so zero is a valid index rather than an absence sentinel. Field 12 is exactly
the eight-byte zero count in schema 1; a nonempty sequence is unsupported.

The closed `ToolId` catalog is 1 Pencil, 2 Brush, 3 Eraser; 1001 Fill, 1002
Eyedropper, 1003 BoxZoom, 1004 GuideMove, 1005 Selection, 1006
FloatingTransform, 1007 LightTableMove; 1101 EffectGradient, 1102
EffectAirbrush, 1103 EffectBlur, 1104 EffectStamp, 1105 EffectDust, 1106
EffectAlphaGradient; and 1201..1206 VectorLine, VectorCurve, VectorRectangle,
VectorEllipse, VectorPolyline, VectorEraser in that order. Each tool sequence
contains exactly those 22 IDs in ascending order. A color entry frame orders
ToolId and an optional exact color. Color is required only for Pencil, Brush,
Fill, Selection, EffectAirbrush, VectorLine, VectorCurve, VectorRectangle,
VectorEllipse, and VectorPolyline and absent for every other tool. A diameter
entry frame orders ToolId and signed Q16.16 `i64`; every tool has one positive
diameter, bounded to 256 document pixels. Exact color is tag 1 plus four RGBA8
bytes or tag 2 plus four little-endian RGBA16 channels, including alpha; no
packed RGBA8 reduction is permitted.

The fill-options frame orders: 1 operation `u32` (1 Seed, 2 ClosedRegion, 3
Extend); 2 normalized tolerance `u16`; 3 gap-close `u8`; 4 extension distance
`u32`; 5 inclusion mode `u32` (0 None, 1 Specified, 2 ExceptSpecified); 6 an
ordered sequence of zero through six exact colors; then booleans 7
overflow-abort, 8 detached-regions, 9 transparent-only, 10 use-document-
selection, 11 Light-Table boundary, and 12 Light-Table color. Inclusion mode 0
requires an empty color sequence; modes 1/2 require at least one color.
Booleans are exactly one byte 0 or 1.

The selection-options frame orders: 1 shape `u32` (1 Rectangle, 2 Ellipse, 3
Lasso, 4 Polyline, 5 Trace, 6 Wand); 2 operation `u32` (1 New, 2 Add, 3
Subtract, 4 Intersect); 3 tolerance `u16`; 4 gap-close `u8`; and 5 positive
signed Q16.16 diameter `i64`, bounded to 4,096 document pixels. The vector-
options frame orders erase mode `u32` (1 Partial, 2 ToIntersection, 3
WholePath) and selection mode `u32` (1 CutBySelection, 2 Touching, 3
FullyContained, 4 Line, 5 WholeLine, 6 ToIntersection, 7 FillBoundary, 8 Fill).
Unknown codes, wrong fixed lengths, noncanonical booleans, partial targets,
trailing bytes, and lengths/counts that exceed their bounds are invalid.

This closed catalog, including defaults, is part of the EDIT schema; adding or
changing an option or default changes the top-level version. The
`EditorStateDigest` hashes the exact twelve-field state frame with
`org.inkpod.digest.editor-state.v1`. Editor revision, editor savepoint, and the
digest field itself are excluded.

For raster assets, canonical stride is exactly width times bytes per pixel and
payload is top-to-bottom row-major bytes without padding. Vector/sample assets
use the sequence framing above and Q16 scalar rules. Chunking never changes
`AssetId`: payload is split deterministically at 4 MiB boundaries only for
storage, and chunk descriptors are verified separately against the asset
payload. Encoded source bytes, path, file name, and provenance are never
`AssetDigest` input.

Asset-kind codes 1/2/3 are CanonicalRaster, CanonicalVectorStream, and
CanonicalSampleStream. Raster pixel-format codes are the five canonical codes
above; width, height, stride, and element count are required. Color-space code
1 is sRGB and is present only for color rasters. Alpha-semantics codes 1/2/3
are Opaque, Straight, and CoverageMask. Vector/sample assets omit pixel/color/
alpha/dimension/stride fields and define their element record through their
primitive schema. Unknown codes are invalid, not optional extensions.

The runtime uses this descriptor-and-payload identity in a Core-owned
content-addressed registry. Ingestion validates dimensions, canonical stride,
element count, logical byte length, and work/resource bounds before computing
or accepting an identity. Equal canonical descriptors and logical payloads
deduplicate even when they came from different paths or supported codecs;
encoded bytes, source file names/paths, timestamps, and optional provenance are
not `AssetDigest` input. A caller-supplied identity or descriptor that does not
match the canonical bytes is rejected without publishing a partial registry or
document change.

Runtime retention roots are Genesis, every retained journal branch and redo
tail, known persistent editor/optional-metadata references, and active transient
owners. The current materialized document and checkpoints are not sufficient
roots by themselves. Assets referenced only by an inactive branch remain
available for cache-free replay, and the owning Core session releases its
registry only after transient work has drained. These runtime rules establish
the exact graph serialized by v9 `GENS` and `ASST`.

Cache-free save/reopen-equivalent verification is detached from that live
registry: it walks the same roots, deep-copies every unique payload in
`AssetId` order, re-ingests it into an empty store with the expected identity,
then rebinds Genesis and retained procedures before fresh replay. Descriptor,
payload, identity, and duplicate-root reference counts must match, while the
source and rebuilt `AssetRecord`, payload, and raster allocations must not share
ownership. That detached archive remains test infrastructure; v9 now provides
the production encoder and staged reader.

Self-referential digest fields are present as thirty-two zero bytes during
calculation. The only absent-digest sentinel is paired with an explicit absent
presence bit. The empty inline procedure payload instead stores 32 zero bytes
under the length-zero rule above; no digest comparison is performed. Thus every
zero rule is unambiguous. Directory entries cover section payloads through both
section digests, while independently validated zero padding has no semantic
content.

The approved Rust implementation is the official `blake3` crate pinned as
exact version `=1.8.5` with default features disabled and only `std` enabled, as
recorded in `third-party-notices.md`; its portable/SIMD backend choice does not
change digest output. The Core production dependency computes the
hierarchical schema-4 `DocumentStateDigest` for canonical execution and
fresh-Core replay. Its runtime commitment cache is separate from render
caching: snapshot validation uses only the documented revision-max scalar and
never these digests. The same pinned implementation computes the v9 section,
root, asset-chunk, journal, document, editor, and procedure-payload commitments.

### Header, directory, and record bytes

The v9 header is exactly 128 bytes:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | magic bytes `49 4E 4B 50 4F 44 00 00` |
| 8 | 4 | top-level format version = 9 |
| 12 | 4 | replay epoch = 6 |
| 16 | 4 | header size = 128 |
| 20 | 4 | required flags = 0 |
| 24 | 8 | total file length |
| 32 | 8 | section-directory offset |
| 40 | 4 | section count |
| 44 | 4 | directory-entry size = 128 |
| 48 | 32 | primitive catalog digest |
| 80 | 32 | file-root digest |
| 112 | 16 | zero reserved bytes |

Each directory entry is exactly 128 bytes:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | ASCII section FourCC |
| 4 | 2 | section schema version |
| 6 | 2 | flags: bit 0 critical, bit 1 opaque-preserve; other bits zero |
| 8 | 4 | compression code: 0 = None |
| 12 | 4 | required alignment = 8 |
| 16 | 8 | stored-byte offset |
| 24 | 8 | stored-byte length |
| 32 | 8 | logical-byte length |
| 40 | 8 | record count |
| 48 | 32 | stored-byte digest |
| 80 | 32 | logical-byte digest |
| 112 | 16 | zero reserved bytes |

Directory entries are sorted by unsigned FourCC bytes then section version,
regardless of physical section order. Known sections are singletons. Section and
directory offsets are 8-byte aligned; all gap/padding bytes are zero. Ranges may
not overlap the header, directory, another section, or exceed total length.
There are no trailing bytes beyond total length. Initial readers accept only
compression None, so stored and logical lengths/bytes are identical while both
domain-separated digests remain mandatory. Unknown critical sections are
rejected. Unknown optional sections require the opaque-preserve flag and are
retained byte-for-byte through save.

Logical section bytes are packed records with no implicit alignment or padding.
Every record starts with `u16 kind`, `u16 record schema version`, `u32 flags`,
and `u64 payload length`, followed by that many payload bytes. Record flags are
zero unless the owning section schema defines them. Directory `record_count`
must consume the logical range exactly. Unknown record kinds or record schema
versions in a known section are invalid; only `EXTM` records are opaque.

Required-section identity is closed and exact:

| FourCC | Section version | Directory flags | Cardinality / record kinds |
|---|---:|---:|---|
| `META` | 1 | critical | exactly one kind-1/v1 record |
| `GENS` | 1 | critical | exactly one kind-1/v1 record |
| `ASST` | 1 | critical | one section; zero or more kind-1 descriptor plus kind-2 chunk records |
| `PROC` | 1 | critical | one section; zero or more kind-1 Commit, kind-2 HistoryMove, kind-3 BranchCut records |
| `EDIT` | 1 | critical | exactly one kind-1/v1 record |
| `CKPT` | 1 | no flags | optional; exactly one kind-1/v1 record |
| `EXTM` | 1 | opaque-preserve | zero or one section of opaque records |

All five required sections must occur once even when `ASST` or `PROC` has no
records. Required sections set directory critical bit 0; `EXTM` sets only bit 1.
`CKPT` is optional and is never authoritative. Unknown optional sections must
set only opaque-preserve and are retained as exact stored bytes and their exact
directory descriptor, except that physical offset is reassigned on save.

`META` kind-1 payload is a schema-1 canonical frame with these ordinals:

1. document UUID (16 bytes)
2. replay epoch (`u32`)
3. primitive catalog digest (32 bytes)
4. current `StateId` (`u64`)
5. zero-based history cursor (`u64`; Genesis is 0)
6. active `BranchId` (`u64`)
7. optional document-savepoint `StateId`
8. optional editor-savepoint digest
9. next stable-object ID (`u64`)
10. next `ProcedureId` (`u64`)
11. next `StateId` (`u64`)
12. next `JournalEventId` (`u64`)
13. next `BranchId` (`u64`)
14. Commit/procedure count (`u64`)
15. all-journal-event count (`u64`)
16. asset count (`u64`)
17. editor record count (`u64`, exactly 1)
18. expected current `DocumentStateDigest`
19. expected `EditorStateDigest`
20. expected full `JournalPrefixDigest`

The cursor counts visible states after Genesis on the active branch and must
resolve to current StateId; the two redundant values are validated. Counts,
high-watermarks, referenced IDs, and the actual sections/journal graph must
agree exactly.

`GENS` kind-1 payload is a schema-1 frame: 1 document UUID; 2 Genesis
`StateId(1)`; 3 root `BranchId(1)`; 4 a Genesis archive; 5 its
`DocumentStateDigest`. The archive starts with base-surface code `u8` (1
SolidWhite, 2 Asset followed by its 32-byte `AssetId`), then nested payload
length `u64` and the exact current schema-1 `DocumentArchive` payload. That nested
payload is not a standalone v2 `.inkpod` container and is not accepted through
the native-file entrypoint; it is the bounded Genesis document DTO owned by the
v9 GENS schema. UUID, base asset, and digest are cross-checked after decode and
before replay. No replay default comes from the build.

`EDIT` kind-1 payload is a schema-1 frame: 1 editor-state schema `u32 = 1`; 2
persisted `EditorRevision u64`; 3 exact canonical EditorState frame; 4 its
`EditorStateDigest`. Revision starts at 1, is excluded from the digest, and the
stored digest must match both the frame and `META`.

The v9 writer emits this bounded canonical EDIT payload and the staged reader
verifies its digest, target IDs, revision, and META savepoint before replacing
the live Core. The decoder rejects an EDIT frame larger than 4 MiB.

An `ASST` kind-1 descriptor payload is the following fixed sequence: `AssetId
[32]`; kind `u32`; pixel format `u32`; color space `u32`; alpha semantics
`u32`; width `u32`; height `u32`; canonical stride `u64`; logical element count
`u64`; logical payload length `u64`; chunk count `u32`; fixed chunk size `u32 =
4,194,304`. Optional numeric descriptor values use zero for absent and are
validated against the selected asset kind. Chunks are a gap-free partition
beginning at offset 0; every nonfinal chunk has the fixed size and the final
chunk is nonempty.

The descriptor is immediately followed by its kind-2 chunk records in ascending
index; asset groups are sorted by unsigned `AssetId`. A kind-2 payload is
`AssetId[32]`, index `u32`, zero `u32`, logical offset `u64`, byte length `u32`,
zero `u32`, asset-chunk digest `[32]`, then exact bytes. Concatenated bytes must
match the descriptor's length and recomputed `AssetId`. A zero-length logical
asset has no chunks. Provenance belongs in `EXTM`, not canonical asset records.

`CKPT` kind-1 is a schema-1 canonical frame with these ordinals: 1 replay epoch;
2 journal-prefix event count; 3 journal-prefix procedure count; 4 32-byte
`JournalPrefixDigest`; 5 current `StateId`; 6 current `DocumentStateDigest`; 7
next stable-object ID; 8 active `BranchId`; 9 history cursor; 10 deterministic
replay-work count; 11 deterministic dirty-byte count; 12 materialized
`DocumentArchive`. The checkpoint is used only when all fields, the archive UUID,
the archive state digest, the current META fields, the complete PROC prefix, and
recomputed policy counters agree. A structurally valid epoch/prefix/state/policy
mismatch falls back to full Genesis/assets/PROC replay. Malformed framing,
out-of-bounds content, or section/root hash failure rejects the file. Removing
the section therefore never changes reconstruction semantics.

A checkpoint is emitted when any closed threshold is reached: 256 procedures,
1,000,000 replay-work units, or 8 MiB dirty bytes. Replay work counts one unit
per procedure plus canonical argument/payload bytes and the logical element
count of each referenced asset. Dirty bytes count canonical argument/payload
bytes plus referenced asset payload bytes. Checked overflow rejects save/open.

### Exact resource limits

Limits are checked with overflow-safe arithmetic before allocation, replay, or
live-Core replacement. Compression None makes stored and logical totals equal;
adding compression requires a new top-level version and explicit decompression-
work limits.

| Resource | Maximum |
|---|---:|
| total `.inkpod` bytes | 1 GiB (`1,073,741,824`) |
| section-directory entries | 64 |
| bytes in one logical section | 768 MiB |
| required plus preserved optional logical bytes | 1 GiB |
| records in one section | 2,097,152 |
| optional `CKPT` logical bytes | 512 MiB |
| procedures / Commit records | 1,048,576 |
| all journal events | 2,097,152 |
| branches | 65,536 |
| any numeric persistent-ID high-watermark | `0x7FFF_FFFF_FFFF_FFFF` |
| canonical argument bytes in one procedure | 1 MiB |
| inline canonical payload in one procedure | 32 MiB |
| one complete `PROC` record | 40 MiB |
| total `PROC` logical bytes | 512 MiB |
| immutable assets | 65,536 |
| one asset logical payload | 512 MiB |
| one asset chunk | 4 MiB |
| total asset logical payload | 768 MiB |
| all live or journal-retained stable objects | 1,048,576 |
| document/project/cut/sequence objects | 1 / 1,024 / 1,024 / 1,024 |
| cells / frames | 65,536 / 100 per cell |
| layers / persisted planes / guides | 4,096 each |
| Light Table sets / items | 256 / 4,096 |
| vector paths / segments / fills / boundary refs | 65,536 / 262,144 / 65,536 / 262,144 |
| palette entries | 4,096 |
| raster width or height | 1,048,576 pixels |
| materialized pixels visited by one image edit | 67,108,864 |
| sparse tiles in one raster / all retained rasters | 262,144 / 1,048,576 |
| canonical raster bytes in one asset / all decoded at open | 512 MiB / 768 MiB |
| UTF-8 node name / general string | 1 KiB / 32 KiB |
| `EDIT` logical bytes | 4 MiB |
| all opaque optional `EXTM` bytes | 16 MiB |
| stroke samples in one canonical procedure | 1,048,576 |

Every Commit is first bounded by its record, argument, payload, asset, object,
and catalog-specific operation limits. Replay then invokes the same canonical
executor used by live editing, including the established per-stroke admission
bound of 16,777,216 formula-3 work units. Oversized image edits, sample streams,
selection/vector collections, asset graphs, event journals, and ID authorities
therefore fail within the staged Core and never partly replace the live Core.

Every primitive catalog entry selects one closed nonzero work-formula ID; there
is no implementation-selected surcharge. A new formula or changed charge
requires a semantics revision, top-level format version, and replay epoch
change. Exceeding a count, byte, object, or work limit rejects the entire staged
open/replay without changing the live Core or existing file.

## Palette and color-chart formats

Application palettes use `.inkpalette`; named color charts use `.inkchart`.
Their codecs live in `inkpod-format`, not the Win32 adapter. Both are bounded,
little-endian, exact-current schema 1 formats and reject any other magic/schema,
truncation, invalid depth, trailing bytes, or oversized input. Before format
freeze, a schema change must change the top-level magic/version; no compatibility
reader is retained implicitly.

An `.inkpalette` file is:

1. 8-byte magic `INKPAL1\0`;
2. `u32` color count;
3. exactly that many 16-byte color records.

An `.inkchart` file is:

1. 8-byte magic `INKCHT1\0`;
2. `u32` entry count;
3. for each entry, one 16-byte color record, a `u32` UTF-8 name length, and the
   name bytes.

Each color record contains `u32 record_size = 16`, `u32 depth` (`8` or `16`),
then straight-alpha red, green, blue, and alpha channels as four `u16` values.
Depth-8 channels must be at most 255; depth-16 values retain all bits. Both
formats allow at most 4,096 colors and 16 MiB total input. Chart names are valid,
non-empty UTF-8 of at most 1,024 bytes; the legacy five-character limit is not
applied. Save validates and encodes first, writes/flushes/synchronizes an
exclusive same-directory temporary file, closes it, and only then renames it to
the destination. Decode and failed save do not mutate a live Core document.

## Batch settings format

Batch settings use the separate `.inkbatch` extension. Version 1 is a bounded
little-endian file with a 28-byte header: magic `INKBATCH`, graph version, body
length, and FNV-1a 64-bit body checksum. The format-freeze policy above applies:
only the current graph version is accepted, and any graph schema change increments
the top-level graph version.

The body stores a bounded UTF-8 graph name; one or more file, folder, or
current-sequence input selectors with optional cell-number ranges; up to 1,024
ordered operations; and one output record. Each operation has its own version,
kind, enabled/configure-each-run flags, conjunctive stable layer/plane ID and type selector,
missing-target policy, and a kind-specific bounded payload. Payloads preserve
exact-depth colors, replacement pairs, continuous-fill seeds and expected
source colors, filter/curve parameters, separation/effect settings, transforms,
resize/DPI, and conversion destination. The output record stores Duplicate,
New Save, or Explicit Overwrite policy, native format code, folder/cell-folder,
basename/numbering direction, continue/stop failure policy, optional wait, and
preview-before-save.

The complete file is limited to 16 MiB, inputs to 16,384, operations to 1,024,
each UTF-8 string to 32 KiB, and each operation payload to 1 MiB. Decode rejects
unknown versions, invalid flags/types/UTF-8/components, empty required lists,
out-of-range counts, checksum mismatch, truncation, overflow, and trailing
bytes before constructing a Core graph. Save encodes and validates in memory,
writes, flushes, and synchronizes a same-directory exclusive temporary file,
and replaces the destination only after completion; cancellation or failure removes only that
exact temporary file.

Batch outputs are ordinary `.inkpod` files. Each input is loaded into a separate
working Core, enabled operations run in order, and only a fully encoded result
is atomically installed. Dry-run creates no output or temporary file. Duplicate
is the default and is forbidden from resolving to its input path; overwrite is
available only through the explicit output policy. A current-document source
retains a copy of its canonical asset store while operations run. Asset-backed
Genesis and every retained journal asset are written through the same v9
GENS/ASST path as an interactive save.

The decoder bounds the whole file (1 GiB), including a post-read check against
concurrent file growth, plus manifest (16 MiB), dimensions, plane/blob counts,
offsets, and lengths before allocation. It rejects unknown
required version/flags/types/color space, zero or inconsistent IDs, duplicate
tile coordinates, mismatched dimensions/pixel formats, truncation, overflow,
checksum mismatch, duplicate tree/guide IDs, missing active/selection IDs,
invalid UTF-8/control characters, guide positions outside the document,
out-of-range opacity/grid values, stable-ID collisions across document,
light-table, and vector state, and a tree
plane ID that does not correspond one-to-one with a persisted plane payload.

## Common raster formats

The Rust format layer exposes a bounded straight-alpha `CommonRaster` DTO and
PNG/TIFF/TGA/BMP codecs. PNG and uncompressed chunky TIFF preserve RGBA8 or
RGBA16. TGA and BMP preserve RGBA8 and reject RGBA16 instead of quantizing it.
PNG stores pixels-per-metre, TIFF rational resolution plus an explicit
unassociated-alpha `ExtraSamples` tag, and BMP
pixels-per-metre; nearest rounding to/from DPI-thousandths is tested within
0.02 DPI. TGA has no standard DPI field and reports DPI unavailable. Tests
round-trip dimensions, bit depth, alpha, and each format's DPI capability.
The white-background option composites at source depth and forces opaque alpha;
disabling it preserves alpha. TIFF/TGA/BMP writers are deterministic and
uncompressed. PNG import expands indexed palettes and transparency; TGA import
honors both image-origin bits and declared alpha depth; BMP import accepts
padded 24-bit RGB rows and the writer's standard 32-bit RGBA bitfields while
rejecting ambiguous masks. Other unsupported compression/layouts are rejected.
Decoded dimensions and byte lengths are rejected before allocating output
storage, and public DTO metadata is revalidated before every conversion.

## Save and savepoint

Section layout and digests are finalized before a short-named same-directory
temporary file is opened. Header, aligned section records, asset chunks,
procedure payloads, and the directory are streamed directly; neither read nor
write first materializes a second complete file buffer. Asset-store payloads are
borrowed while bounded 4 MiB ASST records are prepared, so one full asset clone
is not retained in addition to the chunk records. The temporary file is created
with exclusive create, written in 1 MiB chunks with cancellation checks, flushed,
`sync_all`'d, and closed before `rename` replaces the destination on the same
volume. An error or cancellation removes only the exact temporary file and
leaves an existing destination unchanged. Tests cover cancellation before
commit and replacement of an existing Windows destination.

Only a successful normal save publishes the prospective document StateId and
EditorStateDigest savepoints plus the normal-save path. New documents have no
savepoint and are dirty. A file produced by normal save reopens clean with its
cursor, branches, history, EditorState, and both savepoints restored; Undo/Redo
compare persistent state identity rather than file timestamp. Any encode,
write, flush, cancellation, or replacement failure advances neither savepoint
or path and leaves an existing destination untouched.

The format crate exposes a cancellation hook and tests no-partial-commit
semantics. `save_recovery_atomic` uses the same same-directory temporary-file
protocol but does not advance a Core normal savepoint or attach the recovery
path as a normal document path. `open_recovery` loads the container into a
dirty, recovered, pathless Core document, so a later ordinary Save must choose
a destination and cannot silently overwrite the pre-recovery normal file.

The Windows adapter assigns a per-document recovery path under the user's local
application data directory before a never-saved cell is exposed. A successful
normal save removes that private recovery and switches to an adjacent companion
path. Timer and manual autosaves are queued to the Core engine without blocking
the UI. At startup, the newest private recovery offers explicit open, discard,
or defer choices; ordinary Open offers the same choices for a newer adjacent
recovery. Only explicit discard or a successful normal save removes a recovery.
The format layer also provides bounded modification-time comparison and
idempotent discard helpers. Core, FFI, and Windows tests verify that recovery
never changes the normal file bytes/checksum. Normal user-facing save/open
progress and cancellation UI remain a known `IO-001` difference; recovery
itself retains the contract above.

Explicit compaction is a separate export. Core first returns a confirmation
token containing omitted event/procedure counts and document/editor/journal
digests. Only the exact current token can write a new v9 file whose current
document is Genesis and whose PROC history is empty. The operation never changes
or adopts the live path, journal, savepoints, dirty state, or IDs. There is no
automatic squash.

## Corrupted-input regression corpus

The checked-in `rust/inkpod-format/tests/corpus/corrupted` corpus covers forged
native and batch-body lengths plus malformed/oversized PNG, TIFF, TGA, and BMP
headers. `acceptance_corrupted_file_corpus_is_bounded_and_non_destructive`
passes each case through its public byte decoder and, where available, public
file reader under panic containment. Each corpus entry asserts the intended
bounded rejection path; the PNG IHDR has a valid CRC so dimension validation is
not bypassed by an earlier checksum error. The tests verify unchanged input and
pre-existing output bytes, absence of adjacent temporary output, and preservation
of the current Core document plus its normal file after a failed corrupt open. A second
deterministic mutation harness truncates and bit-flips valid native, batch, and
all four common-raster seeds across every decoder. These regression tests do not
replace coverage-guided fuzzing, but keep the accepted corruption corpus and
allocation-bound paths executable on every normal `cargo test` run. The
`rust/inkpod-format/fuzz` package provides `native_v9` for the current container,
directory, CKPT removal/re-encode path and `native_core_v9` for staged Core
journal/checkpoint/full-replay, retention, and compaction-plan parsing; both call
public production entrypoints.
