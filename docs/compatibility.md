# Compatibility status

Compatibility means operation semantics, data separation, coordinates, and
saved results—not replication of a legacy UI or assets.

| Requirement | Status | Implementation | Tests | Known difference / next work |
|---|---|---|---|---|
| ARCH-001 | Verified | CMake completion stamp lists all four Rust crates and connects Cargo `inkpod-ffi` staticlib/rlib byproducts to MSVC targets | VS2026 x64 Debug/Release build; unchanged repeat reports no work | VS2022 CI remains configured |
| ARCH-002 | Verified | Safe, OS-independent Core/image/format crates with no frontend dependency | All three domain crates' source/manifest scan, clippy, workspace tests | No Rust Windows API dependency |
| ABI-001 | Verified | ABI v1 opaque handles, sized structures, live stroke begin/append/end/cancel, strided command/sample/tile records, checked UTF-8 paths, bounded cumulative work, panic containment, per-thread diagnostics | C11 layout object; C++20 lifecycle/thread/negative/M1 smoke; Rust live-session/extended-stride/short-structure/panic/double-release/UTF-8 tests | Stale copied opaque aliases remain caller errors |
| ABI-002 | Verified | Immutable snapshot owns batched premultiplied-BGRA tiles and a device-pixel view transform; ownership moves to a dedicated renderer queue | Empty/preview/raster snapshot tests; Windows replacement, D2D cache, DPI, and device-loss smoke | M1 snapshot contains raster composite only |
| IO-001 (M1 native save) | Verified | `.inkpod` v1 bounded UUID/manifest/blob container, checksums, same-directory temp/sync/replace, normal savepoint, open/revert | Round-trip, malformed/truncated/checksum, cancellation, existing-file replacement, save/discard/reopen smoke | Blob compression is optional and not enabled |
| IO-001 (recovery/background) | Not started | — | Format cancellation hook only | Autosave/recovery is M2; large save/open worker progress is not yet implemented |
| IO-002 | Not started | — | — | Common raster codecs are M4 |
| DOC-001 (M1 cell) | Verified | 1920 x 1080 CellDocument carries DPI, 100/reference/drawing/safe frames, and margins | Core/FFI/Windows creation and save/reopen metadata equality | M1 uses fixed defaults; editable paper/frame UI is later work |
| DOC-002 (M1 typed planes) | Verified | Platform-supplied document UUID and one stable-ID binary-color layer with protected binary main-line and RGBA8 color planes | UUID/ID/pixel/checksum round-trip; main-line protection tests | General typed tree operations are DOC-003/M3 |
| HIST-001 (M1 transaction) | Verified | Begin/append preview leaves committed state unchanged; end creates one Undo transaction; cancel/failure restores the exact base; redo/savepoint/revert remain intact | Core/FFI live-session tests and Windows pre-pointer-up/commit/capture-cancel smoke | Dialog preview remains later scope |
| VIEW-001 (M1 view) | Verified | Anchored zoom, persistent fit, device-pixel 1:1, pan, and viewport resize use a separate view revision | Core revision/mode tests and Windows pan/wheel/exact Fit-bounds DPI smoke | Box zoom and view flips remain M3 scope |
| PAINT-001 | Verified | UI/Input, Core engine, and Renderer are distinct; pencil/brush/eraser/auto-erase/pressure samples stream through a bounded queue and preview at frame cadence | Core tool/resource tests, 256-record FFI test, Windows thread-ID/multi-sample/live-preview smoke | M1 UI exposes a single brush diameter and RGBA8 color picker |
| M0 Windows shell (Help/About) | Verified | Japanese Help command opens a native modal About with the generated app icon, product description, and CMake-derived version | Release Windows smoke creates and closes About through the real menu command; EXE icon/version resource inspection | Uses a macOS-inspired centered hierarchy while retaining Windows modal, DPI, and keyboard behavior |
| M8 Windows package preparation | In progress | Windows App Development CLI 0.4.0 manifest, 48 MSIX PNGs, and five-resolution ICO generated from the project SVG | Asset file/dimension/ICO/version CTest; Release RC/link and Windows smoke | No signed MSIX or clean Windows install/uninstall verification yet |

All M2 requirements (fill/coloring semantics, autosave/recovery, and later
milestones) remain `Not started`; no placeholder API is labeled compatible.

## Unknown legacy formats

| Item | Status | Reason |
|---|---|---|
| DGA/CEL binary codec | Unknown | No rights-cleared fixture and independent oracle |
| Legacy palette/chart/filter preset layouts | Unknown | Byte layouts are not defined by the internal specification |

No legacy manual, image, icon, wording, proprietary binary assumption, or
third-party artwork was used for M0 or M1.
