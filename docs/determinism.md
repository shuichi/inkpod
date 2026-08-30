# Cross-architecture determinism contract

The current runtime replay contract is procedure format 32, replay epoch 27,
canonical numeric version 1, and the digest of the closed 75-entry primitive
catalog. The separate public InkScript catalog/owner manifest is v5 with 73
commands. Production `.inkpod` is exact-current v32; an optional verified
checkpoint preserves this contract and never replaces the authoritative journal.

Epoch 27 and format 31 closed the standard image-tree model. Format 32 changes
only the native Genesis validity rule: a fully opaque imported RGBA MainLine may
use a SolidWhite underlay, while an import with any non-opaque alpha remains Transparent.
The primitive catalog and replay results are unchanged. Every layer has
exactly one MainLine plane, exactly one Color plane, and zero or more Raster
planes; plane conversion preserves that role and changes only pixel format.
Current selection, ordered named saved-selection masks, and sparse fill
protection are document-owned rasters outside the image tree. Layer kinds,
Selection planes, vanishing-point objects, adjustment layers, and their former
procedures are absent; retired primitive codes remain rejected tombstones.
`.inkbatch` v5/operation schema 4 and its private `ApplyBatchOperations/canonical-v3` target Color
or Raster plane roles without a layer-kind selector. Cut payload schema 3 retains
its independent replay epoch 25. Every older top-level, replay, Batch, and Cut
schema is rejected without migration.

The optional angled shooting frame retains its fixed-point CORDIC geometry and
independent document ownership. Floating transforms, output-color guard, Color
chart, Light Table, raster geometry, and the other surviving primitives retain
their established fixed-point semantics; their historical introduction record
is kept in [`file-format.md`](file-format.md) and [`legacy.md`](legacy.md).

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
together with format version 32 and replay epoch 27. A replay-semantic change that updates
the catalog or any replay golden without advancing both version and epoch therefore
fails the public contract review rather than silently accepting a new result.

`RenderSnapshot::canonical_composite_digest` and the C ABI snapshot digest query
observe the canonical output through the production immutable snapshot. The
Windows `CoreHost`, ABI smoke, GUI smoke, and renderer-sink contract exercise the
same query. Cache revisions and view-only state are deliberately excluded.

## Performance fixture

`core_workflows` runs `canonical_replay` in both quick and full profiles with
the same five procedures, six boundary observations, revision/history counters,
and checksum `34f65a7092a87cff`. Wall-clock is diagnostic; the checksum and
semantic counters are hard failures. The established pan/zoom, dirty-rebuild,
native wheel, and drawing ranges are unchanged.
