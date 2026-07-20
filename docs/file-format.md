# Native file format

The native extension is `.inkpod`. No native codec exists in M0, so there is no
claimed on-disk compatibility and no other extension is reused for placeholder
data. `IO-001` remains `Not started` until M1 implements and tests the format.

The M1 format must be a versioned container that separates a manifest from
bounded binary blobs. Before it can be marked verified, the manifest must carry
format version, stable document/layer/plane IDs, pixel dimensions, DPI, color
space, frame metadata, blob lengths, and checksums. Save must complete, flush,
and close a temporary file on the destination volume before replacing the old
file. Round-trip, malformed-input, cancellation, savepoint, and recovery tests
are required.

DGA, CEL, and legacy palette/chart/filter-preset byte layouts are `Unknown`.
No reader or writer will be enabled without rights-cleared fixtures and an
independent expected-result oracle.

