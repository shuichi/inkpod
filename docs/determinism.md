# Cross-architecture determinism contract

The current runtime replay contract is procedure format 28, replay epoch 25,
canonical numeric version 1, and the digest of the closed 77-entry primitive
catalog. Production `.inkpod` is exact-current v28; an optional verified
checkpoint preserves this contract and never replaces the authoritative journal.
Epoch 25 and format 28 add the dedicated sparse fill-protection mask and private
`ApplyBatchOperations/canonical-v2`; exact-depth color replacement, atomic
move-to-color-plane, mask replacement and erase replay as one ordered transaction.
Epoch 24 and format 27 removed vector and Text/Annotation state, canonical
procedures, persistent IDs, assets, snapshots, and composite inputs from the
accepted replay model. Their former primitive IDs remain reserved tombstones.
Epoch 24/format 27 input is rejected without migration.
Epoch 23 and format 26 add `EditVanishingPoints/canonical-v2`: bounded stable-ID
objects encode owner layer, signed milli-pixel origin, normalized milli-degree
interval/phase, exact RGBA8/16 color, opacity, and visibility. Snapshot radial
segments and snapping are deterministic derived state and never enter the
canonical normal-output composite.
Epoch 22 and format 25 add `EditShootingFrame/canonical-v2`: one optional
stable-ID angled frame uses signed milli-pixel center, positive size, `u32`
binary turns, a closed five-anchor code, and exact display/instruction-export
flags. Deterministic fixed-point CORDIC derives corners and hit targets; no OS
trigonometry enters replay. The epoch 21 annotation procedure and epoch 14 vector
procedures are historical records only and are not accepted by the exact-current
catalog or executor. Epoch 20 and format
23 added ordered atomic Cut membership editing, the separated
immutable member-asset table, and membership-bearing Cut history; Cell-document
procedure semantics remain closed. Epoch 19 and format 22 added the current-only
Cut descriptor and its independent metadata/default canonical history. Epoch 18
and format 21 added `CommitFloating/canonical-v3`: half-open five-point
anchors, canonical absolute target X/Y, local scale, clockwise rotation, and
pixel-centre inverse raster mapping share one replay formula. Epoch 17 and format
20 added `SelectOutputColorGuard/canonical-v1`. Its closed
BT.709 conservative Y'CbCr QA profile scans the committed visible straight-
alpha composite at exact RGBA16 depth, skips alpha zero, and uses fixed rational
half-up conversion before selection algebra. Epoch 16 and format 19 added
`ReplaceColorChart/canonical-v1` and committed the
independent named chart/lock plus EditorState cursor. The prior epoch 15/format
18 added `LightTableBulkRegister/canonical-v2`; its resolved, ordered immutable
source assets and item properties remain the replay input, while
sequence discovery and duplicate preview remain live control-plane queries.
Epoch 14/format 17 introduced resolved raster geometry procedure schema 2; its
fixed-point geometry semantics remain unchanged.

## Canonical numeric authority

`inkpod-image::canonical` is the shared authority for exact IEEE-754 to scaled
integer conversion, signed Q16 document coordinates, normalized `u16` pressure,
`u32` turns, ties-to-even division, mathematical floor/ceiling, integer square
root, fixed CORDIC rotation, straight-alpha source-over, premultiplication, and
fill color distance. Procedure formation canonicalizes public floating inputs
before execution and encodes the fixed values rather than raw IEEE bit patterns.

Image-result Gaussian weights use a bounded integer Pascal kernel. Levels gamma
uses fixed log2/exp2 iterations and a frozen Q48 table. Gradient distance,
airbrush/stamp falloff and spacing, Light Table transforms, floating-selection
rotation, and the ABI 45-degree constraint use integer/fixed-point algorithms.
Brush smoothing is also fixed-point: the first Q16.16 document sample is
unchanged and each later axis is `round_ties_even((previous * s + raw *
(1001 - s)) / 1001)` for `s` in 0..1000. It is causal, so append batching and
worker partitioning cannot change the normalized sample sequence. Exact
start-color restriction compares the immutable pre-stroke native pixel,
including alpha, and never depends on connectivity or mutable dab order.
Production Rust source is guarded against platform `exp`, `powf`, trigonometric,
square-root, hypot, and atan2 calls. Remaining IEEE arithmetic is limited to
validated polynomial geometry whose operation order is explicit; its
stored and replayed inputs are first canonicalized.

Raster traversal is row-major, tiles and stable-object sets are ordered, and no
semantic output depends on hash iteration order, worker count, tile scheduling,
SIMD width, GPU state, locale, clock, or OS entropy. Alpha and color distance
do not have frontend or effect-local alternative formulas.

## Replay and output gates

The public determinism fixture executes a pressure-sensitive stroke, radial dithered
gradient, Gaussian blur, Levels gamma, and airbrush. It locks the semantic digest
at Genesis and after every one of the five procedures, verifies a fresh journal
rebuild at every boundary, and locks the final public snapshot composite digest.
The same test runs in the Rust jobs on Windows, Linux, and macOS; ARM64 Windows
is also covered by the native ARM64 build/test gate. A mismatch is exact—there
is no tolerance or architecture-specific expected value.

The primitive catalog digest covers entries in ascending stable-ID order:
primitive ID, schema version, length-framed canonical name, BLAKE3 argument-
schema digest, semantics revision, work-formula ID, and replay-policy byte.
Tests lock its digest
together with format version 28 and replay epoch 25. A semantic change that updates
the catalog or any golden without advancing both version and epoch therefore
fails the public contract review rather than silently accepting a new result.

`RenderSnapshot::canonical_composite_digest` and the C ABI snapshot digest query
observe the canonical output through the production immutable snapshot. The
Windows `CoreHost`, ABI smoke, GUI smoke, and renderer-sink contract exercise the
same query. Cache revisions and view-only state are deliberately excluded.

## Performance fixture

`core_workflows` runs `canonical_replay` in both quick and full profiles with
the same five procedures, six boundary observations, revision/history counters,
and checksum `264b98028ac92ac6`. Wall-clock is diagnostic; the checksum and
semantic counters are hard failures. The established pan/zoom, dirty-rebuild,
native wheel, and drawing ranges are unchanged.
