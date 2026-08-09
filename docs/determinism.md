# Cross-architecture determinism contract

The current runtime replay contract is procedure format 10, replay epoch 7,
canonical numeric version 1, and the digest of the closed 76-entry primitive
catalog. Production `.inkpod` is exact-current v10; an optional verified
checkpoint preserves this contract and never replaces the authoritative journal.

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
Production Rust source is guarded against platform `exp`, `powf`, trigonometric,
square-root, hypot, and atan2 calls. Remaining IEEE arithmetic is limited to
validated polynomial/vector geometry whose operation order is explicit; its
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
together with format version 10 and replay epoch 7. A semantic change that updates
the catalog or any golden without advancing both version and epoch therefore
fails the public contract review rather than silently accepting a new result.

`RenderSnapshot::canonical_composite_digest` and the C ABI snapshot digest query
observe the canonical output through the production immutable snapshot. The
Windows `CoreHost`, ABI smoke, GUI smoke, and renderer-sink contract exercise the
same query. Cache revisions and view-only state are deliberately excluded.

## Performance fixture

`core_workflows` runs `canonical_replay` in both quick and full profiles with
the same five procedures, six boundary observations, revision/history counters,
and checksum `20de057cc9cc3ca1`. Wall-clock is diagnostic; the checksum and
semantic counters are hard failures. The established pan/zoom, dirty-rebuild,
native wheel, and drawing ranges are unchanged.
