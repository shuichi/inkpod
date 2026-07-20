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
  typed plane descriptors with pixel format and blob ranges
  tile blob descriptors with coordinate, dimensions, format,
  offset, length, and FNV-1a 64-bit checksum

blob area
  compact edge-aware tile bytes in manifest order
```

M1 requires exactly one binary-mask main-line plane and one straight-alpha
sRGB RGBA8 color plane. Raster storage is sparse; zero tiles are omitted. Tile
revision and GPU cache data are runtime state and are not persisted.

The decoder bounds the whole file (1 GiB), including a post-read check against
concurrent file growth, plus manifest (16 MiB), dimensions, plane/blob counts,
offsets, and lengths before allocation. It rejects unknown
required version/flags/types/color space, zero or inconsistent IDs, duplicate
tile coordinates, mismatched dimensions/pixel formats, truncation, overflow,
and checksum mismatch.

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
semantics. User-facing background progress, autosave, and recovery files belong
to M2 and are not claimed by M1.

## Unknown formats

DGA, CEL, and legacy palette/chart/filter-preset layouts remain `Unknown`. No
reader or writer is enabled without rights-cleared fixtures and an independent
expected-result oracle.
