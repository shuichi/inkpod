# Native file format

`.inkpod` v1 is a bounded little-endian container. It does not reuse a legacy
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
  optional M2 color metadata: exact-depth main-line base color and a bounded
  sequence of exact-depth RGBA8/RGBA16 palette records
  optional M3 document-editing metadata: typed layer tree/properties, active
  IDs, persistent selection plane, guides, and grid
  optional M4 production-workflow metadata: light-table sets/items, transforms,
  source identity/revision/DPI/reference frame, and source-plane IDs
  optional M5 vector metadata: stable paths/fills, cubic control points,
  endpoint widths, colors, and fill-boundary path IDs
  typed plane descriptors with pixel format and blob ranges
  tile blob descriptors with coordinate, dimensions, format,
  offset, length, and FNV-1a 64-bit checksum

blob area
  compact edge-aware tile bytes in manifest order
```

M2 retains exactly one main-line plane and one color plane in the current cell
DTO. The main-line descriptor accepts binary mask, grayscale 8-bit, or
grayscale 16-bit storage. The color descriptor accepts straight-alpha sRGB
RGBA8 or RGBA16 storage. Header flag bit 0 advertises the M2 color-metadata
section. Each color record carries its own 8/16-bit depth; the main-line base
color and up to 4096 palette entries are serialized little-endian without an
implicit 8-bit conversion. A flag-0 v1 file from M1 remains readable and
defaults to an empty palette plus an opaque black base color at the color-plane
depth. The decoder requires every tile format to equal its plane descriptor.
Raster storage is sparse; zero tiles are omitted.
Tile revision and GPU cache data are runtime state and are not persisted.

Header flag bit 1 advertises the additive M3 metadata section. It starts with
`"M3ED"` plus section version 1 and contains active layer/plane IDs, the stable
selection-plane ID, bounded layer/guide counts, grid origin/spacing/
subdivisions, and ordered layer descriptors. Each layer stores its stable ID,
typed `LayerKind`, visible/editable flags, opacity in thousandths, bounded UTF-8
name, and an ordered list of plane property descriptors. Plane kind, storage
format, sparse tile blobs, and checksums remain in the common plane descriptor,
so one stable plane ID joins the tree entry to its raster payload. Guide records
store stable ID, horizontal/vertical axis, and signed document position.

The persistent selection is a normal sparse binary-mask plane referenced by
`selection_plane_id`; it is not inferred from a UI rectangle. M3 view flips,
ruler visibility, locator position, secondary-view transforms, floating paste,
and shortcut bindings are transient/application state and are not serialized.
A flag-0 or M2-only v1 file remains readable: Core deterministically upgrades
its legacy main/color descriptors into one coloring layer and creates an empty
selection mask without changing the legacy pixel payload.

Header flag bit 2 advertises the additive `"M4WF"` version-1 section. It stores
stable-ID light-table sets, the active set, global opacity, and ordered items.
Each item stores a stable item/source-plane ID, source UUID/revision/DPI/
reference frame, visibility, opacity, display mode/color, translation, scale,
rotation, and bounded UTF-8 name. Source RGBA8/RGBA16 rasters remain in the
checksummed blob area as typed `LightTable` planes and may differ in dimensions
from the editing paper. They are read-only: fill derives a temporary boundary
or sampled color but never writes the source. M1-M3 files open with one empty
default set. The decoder rejects missing/unreferenced source planes, colliding
IDs, invalid transforms/opacities/DPI, and M4 without the M3 typed tree.

Header flag bit 3 advertises the additive `"M5VT"` version-1 section. Geometry
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
geometry lives in `M5VT`, not in raster blobs. The decoder requires M3 typed-tree
metadata, rejects vector planes without M5 metadata, cross-layer fill boundaries,
open/discontinuous fill boundaries, missing plane/path references, duplicate or
cross-M1-M5 stable IDs, unsupported flags/reserved values, excessive counts, and
trailing section bytes. M1-M4 files remain readable and acquire no vector state.

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
