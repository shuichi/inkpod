# Compatibility status

Compatibility means operation semantics, data separation, coordinates, and
saved results—not replication of a legacy UI or assets.

| Requirement | Status | Implementation | Tests | Known difference / next work |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake completion stamp lists all four Rust crates and connects Cargo `inkpod-ffi` staticlib/rlib byproducts to MSVC targets | VS2026 x64 Debug/Release build; unchanged repeat reports no work | VS2022 CI remains configured |
| ARCH-002 | Verified | Safe, OS-independent Core/image/format crates with no frontend dependency | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows API dependency |
| ABI-001 | Verified | ABI v1 opaque handles, sized/strided stroke and M2 fill/color structures, exact 8/16-bit values, leak coordinates, autosave/recovery, checked UTF-8 paths, bounded work, panic containment, per-thread diagnostics | C11 layout object; C++20 lifecycle/thread/negative/M1/M2 smoke; Rust FFI lifecycle, fill, color, recovery, and negative tests | Stale copied opaque aliases remain caller errors |
| ABI-002 | Verified | Immutable snapshot owns batched premultiplied-BGRA tiles, device-pixel view transform, and optional view-only color-check output; ownership moves to a dedicated renderer queue | Empty/preview/raster/color-check snapshot tests; Windows replacement, D2D cache, DPI, device-loss, and color-check smoke | Snapshot display conversion is explicit BGRA8; persisted source values retain their depth |
| IO-001 (native save) | Verified | `.inkpod` v1 bounded UUID/manifest/blob container, checksums, same-directory temp/sync/replace, normal savepoint, open/revert, and grayscale/RGBA 8/16 tile formats | Round-trip, exact RGBA16 bytes, malformed/truncated/checksum, cancellation, replacement, save/discard/reopen smoke | Blob compression is optional and not enabled |
| IO-001 (M2 recovery) | Verified | Atomic autosave preserves the normal path/savepoint; recovery opens dirty and pathless; Windows queues timer/manual autosaves without waiting | Core/FFI recovery tests and Windows normal-file/recovery checksum smoke | Normal user save/open still lacks non-blocking progress UI |
| IO-002 | Not started | — | — | Common raster codecs are M4 |
| DOC-001 (M1 cell) | Verified | 1920 x 1080 CellDocument carries DPI, 100/reference/drawing/safe frames, and margins | Core/FFI/Windows creation and save/reopen metadata equality | M1 uses fixed defaults; editable paper/frame UI is later work |
| DOC-002 (M2 coloring planes) | Verified | Platform-supplied stable IDs with protected binary/grayscale 8/16 main-line semantics and straight RGBA8/16 color storage | Exact format round-trip, grayscale coverage/base-color tests, protected main-line fill/stroke checks | General typed tree operations are DOC-003/M3 |
| HIST-001 (M2 transaction) | Verified | Stroke and fill plan before commit and create at most one Undo transaction; cancel/failure/overflow/no-op restores or retains the exact base; redo/savepoint/revert remain intact | Core/FFI atomicity tests and Windows pre-pointer-up/fill/commit/cancel smoke | Dialog preview remains later scope |
| VIEW-001 (M1 view) | Verified | Anchored zoom, persistent fit, device-pixel 1:1, pan, and viewport resize use a separate view revision | Core revision/mode tests and Windows pan/wheel/exact Fit-bounds DPI smoke | Box zoom and view flips remain M3 scope |
| PAINT-001 | Verified | UI/Input, Core engine, and Renderer are distinct; pencil/brush/eraser/auto-erase/pressure samples stream through a bounded queue and preview at frame cadence | Core tool/resource tests, 256-record FFI test, Windows thread-ID/multi-sample/live-preview smoke | M2 UI retains a single brush diameter and RGBA8 color picker |
| FILL-001 | Verified | Deterministic connected seed fill uses normalized 16-bit per-channel tolerance and an optional selection mask | Closed-region, selection, tolerance, Core transaction, FFI, and Canvas-click tests | Selection construction/editing UI begins in M3 |
| FILL-002 | Verified | Specified/except-specified inclusion (up to six colors), overflow abort with leak coordinate, bounded cancellation, axis gap-close, and detached matching regions | Required gap/inclusion/overflow goldens and invalid/cancel/no-op transaction tests | The native gap rule is documented; proprietary behavior is not claimed |
| FILL-003 | Verified | Closed-region fill rejects edge-connected regions and fill extension expands through a bounded mask/distance | Closed/open-region and extension automation; Core/ABI operation validation | Windows exposes seed fill; separate range-gesture tools are not present |
| COLOR-001 (M2 scope) | Verified | Grayscale 8/16 and straight RGBA8/16 values remain typed; selected/topmost/composite eyedropper sources return exact-depth values | Coverage/base-color and no-implicit-quantization goldens; format/FFI tests; Windows eyedropper smoke | Light-table sampling and full color-editor UI remain later scope |
| COLOR-002 (M2 scope) | Verified | Palette retains exact 8/16-bit entries; temporary legacy-white/native-alpha checks affect only render snapshots | Palette golden plus Core/Windows view-only color-check tests | Chart and subpalette belong to M4 |
| M0 Windows shell (Help/About) | Verified | Japanese Help command opens a native modal About with the generated app icon, product description, and CMake-derived version | Release Windows smoke creates and closes About through the real menu command; EXE icon/version resource inspection | Uses a macOS-inspired centered hierarchy while retaining Windows modal, DPI, and keyboard behavior |
| M8 Windows package preparation | In progress | Windows App Development CLI 0.4.0 manifest, 48 MSIX PNGs, and five-resolution ICO generated from the project SVG | Asset file/dimension/ICO/version CTest; Release RC/link and Windows smoke | No signed MSIX or clean Windows install/uninstall verification yet |

Every M2 requirement and required golden case is `Verified`. M3 remains `Not
started`; no M3 layer-tree, selection-authoring, clipboard, transform, guide,
multi-view, or shortcut-editor behavior is claimed.

## Unknown legacy formats

| Item | Status | Reason |
|---|---|---|
| DGA/CEL binary codec | Unknown | No rights-cleared fixture and independent oracle |
| Legacy palette/chart/filter preset layouts | Unknown | Byte layouts are not defined by the internal specification |

No legacy manual, image, icon, wording, proprietary binary assumption, or
third-party artwork was used for M0 through M2.
