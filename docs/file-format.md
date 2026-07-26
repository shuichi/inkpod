# Native file format

`.inkpod` v2 is a bounded little-endian container. It does not reuse a legacy
extension and makes no DGA/CEL byte-compatibility claim.

## Container layout

The file separates a manifest from binary tile blobs:

```text
32-byte header
  magic "INKPOD\0\0", format version, required flags,
  manifest byte length, blob count

binary manifest
  128-bit document UUID plus stable document/layer/main-plane/color-plane IDs
  pixel width/height, X/Y DPI in thousandths, sRGB marker
  100/reference/drawing/safe frame rectangles and margins
  required color metadata: exact-depth main-line base color and a bounded
  sequence of exact-depth RGBA8/RGBA16 palette records
  optional document metadata: typed layer tree/properties, active
  IDs, persistent selection plane, guides, and grid
  optional light-table metadata: sets/items, transforms,
  source identity/revision/DPI/reference frame, and source-plane IDs
  optional vector metadata: stable paths/fills, cubic control points,
  endpoint widths, colors, and fill-boundary path IDs
  typed plane descriptors with pixel format and blob ranges
  tile blob descriptors with coordinate, dimensions, format,
  offset, length, and FNV-1a 64-bit checksum

blob area
  compact edge-aware tile bytes in manifest order
```

The current cell DTO retains exactly one main-line plane and one color plane.
DTO. The main-line descriptor accepts binary mask, grayscale 8-bit, or
grayscale 16-bit storage. The color descriptor accepts straight-alpha sRGB
RGBA8 or RGBA16 storage. Header flag bit 0 marks the required color-metadata
section. Each color record carries its own 8/16-bit depth; the main-line base
color and up to 4096 palette entries are serialized little-endian without an
implicit 8-bit conversion. The decoder rejects a missing flag and requires every
tile format to equal its plane descriptor.
Raster storage is sparse; zero tiles are omitted.
Tile revision and GPU cache data are runtime state and are not persisted.

Header flag bit 1 advertises the document-metadata section. It starts with
`"DOCM"` plus section version 1 and contains active layer/plane IDs, the stable
selection-plane ID, bounded layer/guide counts, grid origin/spacing/
subdivisions, and ordered layer descriptors. Each layer stores its stable ID,
typed `LayerKind`, visible/editable flags, opacity in thousandths, bounded UTF-8
name, and an ordered list of plane property descriptors. Plane kind, storage
format, sparse tile blobs, and checksums remain in the common plane descriptor,
so one stable plane ID joins the tree entry to its raster payload. Guide records
store stable ID, horizontal/vertical axis, and signed document position.

The persistent selection is a normal sparse binary-mask plane referenced by
`selection_plane_id`; it is not inferred from a UI rectangle. View flips,
ruler visibility, locator position, secondary-view transforms, floating paste,
and shortcut bindings are transient/application state and are not serialized.

Header flag bit 2 advertises the `"LTBL"` version-1 section. It stores
stable-ID light-table sets, the active set, global opacity, and ordered items.
Each item stores a stable item/source-plane ID, source UUID/revision/DPI/
reference frame, visibility, opacity, display mode/color, translation, scale,
rotation, and bounded UTF-8 name. Source RGBA8/RGBA16 rasters remain in the
checksummed blob area as typed `LightTable` planes and may differ in dimensions
from the editing paper. They are read-only: fill derives a temporary boundary
or sampled color but never writes the source. Files without light-table
metadata open with one empty default set. The decoder rejects
missing/unreferenced source planes, colliding
IDs, invalid transforms/opacities/DPI, and light-table metadata without the
typed document tree.

Header flag bit 3 advertises the `"VECT"` version-1 section. Geometry
is stored in document coordinates as signed 32-bit thousandths of a pixel,
restricted to -2,000,000,000 through 2,000,000,000 thousandths so reopened
geometry obeys the Core's +/-2,000,000 document-pixel bound;
variable endpoint widths are unsigned thousandths in the range 1 through
4,096,000. Each stable-ID path names a vector main-line or color-trace plane,
an exact RGBA8/RGBA16 color, a closed flag, and one or more continuous cubic
segments. Each stable-ID fill names a vector-fill plane, an exact color, and one
or more unique closed boundary-path IDs. The section is bounded to 65,536 paths,
262,144 total segments, 65,536 fills, and 262,144 total boundary references.

A vector-coloring layer has exactly one vector-main-line plane, one or more
color-trace planes, and exactly one vector-fill plane; optional raster planes are
allowed. Vector plane payload descriptors are empty RGBA8 placeholders because
geometry lives in `VECT`, not in raster blobs. The decoder requires typed document
metadata, rejects vector planes without vector metadata, cross-layer fill boundaries,
open/discontinuous fill boundaries, missing plane/path references, duplicate or
cross-section stable IDs, unsupported flags/reserved values, excessive counts,
and trailing section bytes.

Header flag bit 4 advertises the `"ADJT"` version-1 section. Each record names
one stable-ID adjustment layer and stores exactly one bounded
brightness/contrast, RGB/R/G/B Bezier or B-spline tone curve, or levels
operation. Curve inputs/outputs use the full normalized 0..65535 range and are
strictly ordered from 0 through 65535; levels store input shadow/gamma/highlight
and output shadow/highlight in the same normalized domain. Adjustment layers
have no raster payload, so source plane bytes and checksums remain unchanged.

The decoder requires a one-to-one relationship between `ADJT` records and
zero-plane adjustment layers. It rejects missing or duplicate records,
non-adjustment/wrong layer IDs, invalid channel/interpolation codes, excessive
layer/curve counts, out-of-range parameters, unknown/reserved values, and
trailing bytes.

## Batch settings format

Batch settings use the separate `.inkbatch` extension. Version 1 is a bounded
little-endian file with a 28-byte header: magic `INKBATCH`, graph version, body
length, and FNV-1a 64-bit body checksum. It does not claim compatibility with
an undocumented legacy batch/preset format.

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
available only through the explicit output policy.

The decoder bounds the whole file (1 GiB), including a post-read check against
concurrent file growth, plus manifest (16 MiB), dimensions, plane/blob counts,
offsets, and lengths before allocation. It rejects unknown
required version/flags/types/color space, zero or inconsistent IDs, duplicate
tile coordinates, mismatched dimensions/pixel formats, truncation, overflow,
checksum mismatch, duplicate tree/guide IDs, missing active/selection IDs,
invalid UTF-8/control characters, guide positions outside the document,
 out-of-range opacity/grid values, stable-ID collisions across M3/M4/M5 state,
 and a tree
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

Encoding finishes in memory before a short-named same-directory temporary file is opened.
The temporary file is created with exclusive create, fully written, flushed,
`sync_all`'d, and closed before `rename` replaces the destination on the same
volume. An error or cancellation removes only the exact temporary file and
leaves an existing destination unchanged. Tests cover cancellation before
commit and replacement of an existing Windows destination.

Only a successful normal save advances the Core savepoint and normal-save path.
New documents have no savepoint and are dirty. Open/revert create a clean
savepoint; Undo/Redo compare history-state identity rather than file timestamp.

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
progress and cancellation UI remain incomplete IO-001 work; this does not
affect the M2 recovery acceptance case.

## Unknown formats

DGA, CEL, and legacy palette/chart/filter-preset layouts remain `Unknown`. No
reader or writer is enabled without rights-cleared fixtures and an independent
expected-result oracle.

## M8 corrupted corpus

The checked-in `rust/inkpod-format/tests/corpus/m8` corpus covers forged native
manifest and batch-body lengths plus malformed/oversized PNG, TIFF, TGA, and BMP
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
allocation-bound paths executable on every normal `cargo test` run.
