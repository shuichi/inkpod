# Implementation status

This document is a compact snapshot of the current implementation, active
differences, and latest representative verification. Requirement-by-requirement
status and evidence live in [`compatibility.md`](compatibility.md); detailed
ownership and data-flow contracts live in [`architecture.md`](architecture.md).
Completed phase plans and chronological verification history remain available in
Git history and are not duplicated here.

## Current implementation

| Area                   | Current state                                                                                                                                                                                                                                                                                                                                                                                                           |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Product surface        | The native Windows application connects the maintained drawing, fill, selection, layer/plane, transform, light-table, sequence, vector, filter/effect, adjustment, import/export, and Batch operations from production commands through the C ABI to Rust Core. All 275 production commands are menu-accessible and have configurable displayed shortcuts.                                                              |
| Rust Core              | `inkpod-core`, `inkpod-image`, and `inkpod-format` are OS-independent. Documents use stable IDs, sparse tiled rasters, copy-on-write snapshots/history, typed layer/plane rules, deterministic transactions, savepoint-aware Undo/Redo, and bounded aspect-preserving per-layer RGBA thumbnails. Crate roots are small module/re-export indices with responsibility-specific modules and tests outside `src`.                                                                |
| Windows frontend       | UI/Input, Core engine, and Renderer are separate long-lived threads. `Application` owns startup/message-loop/shutdown, MainWindow owns message normalization and command routing, feature controllers own typed UI state, and dialog modules use typed inputs/results without receiving the complete application state or Core. The compact owned floating Tools palette groups all 37 Tools-menu leaves into tabs. The owned floating Layer palette shows vertically ordered tiles with thumbnails, name, type, plane count, opacity, visibility/editability, selection, actions, and drag reordering. Both palettes share command state with menus, restore on-screen placement, and hide on close. Command routing and command-state catalogs cover every production command exactly once. |
| Rendering and input    | Canvas uses client device pixels with one document-to-device transform. Immutable snapshots move through an ownership queue to the D3D11/DXGI/Direct2D renderer. Raster hit quantization uses the same half-open pixel-cell rule as locator sampling and nearest-neighbor display, including magnified lower/right clicks, final-row/final-column clipping, and flipped views.                                          |
| ABI                    | Public C ABI v2 exposes 160 versioned fixed-layout functions, including a caller-owned size-query buffer for bounded layer thumbnails. Header/export parity and a direct contract-test reference for every function are enforced. Opaque Core/snapshot/task ownership, copied/buffer-query records, panic containment, validation, and double-release behavior are covered at the boundary.                                                                                                      |
| Persistence and codecs | `.inkpod` v2 uses bounded semantic `DOCM`, `LTBL`, `VECT`, and `ADJT` sections; `.inkbatch` is checksummed and versioned. Atomic save/recovery and PNG/TIFF/TGA/BMP import/export preserve supported depth, alpha, dimensions, and DPI. Pre-v2 project files are intentionally unsupported.                                                                                                                             |
| Windows packaging      | CMake is the build entry point and drives the Rust static library, strict MSVC C++20 build, resources, tests, and unsigned x64/ARM64 MSIX assembly with architecture-matched app-local MSVC runtime payloads.                                                                                                                                                                                                           |
| Reliability            | Public readers reject bounded malformed dimensions, lengths, checksums, IDs, references, and unsupported layouts without partially replacing the current document or output. Deterministic mutation tests cover native, Batch, PNG, TIFF, TGA, and BMP readers; large sparse/COW and bounded dense-image paths have benchmark coverage.                                                                                 |

## Active user-facing gaps

The following requirements remain `In progress`. Their underlying Core/C ABI
models and menu commands are retained. The layer list now has a modeless tile
palette; the remaining work is listed per requirement.

| Requirement          | Available now                                                                                                                                               | Remaining UI                                                        |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `DOC-002`, `DOC-003` | Typed stable-ID layer/plane topology and transactional commands; a floating layer palette exposes vertically ordered thumbnail tiles, layer selection, visibility/editability, opacity/type/plane-count metadata, shared actions, and direct drag reordering | Floating plane selection/actions and direct plane reordering; multi-target editing presentation |
| `VIEW-003`           | Same-document views, descriptive/dirty tab labels, and asynchronous X/Y/RGBA/selection sampling in the status bar                                           | Magnified neighborhood locator display and editing                  |
| `COLOR-002`          | Palette, chart, subpalette, color-check models, file/menu commands, and number/Tab shortcuts                                                                | Floating palette and chart presentation                             |
| `LT-001`, `LT-002`   | Light-table sets/items, transform/color/mode/opacity, reference alignment, Canvas movement, sampling, reload, and dirty-safe edit-image swap                | Floating light-table set/item controls                              |
| `SEQ-001`            | Natural-order import/export and first/previous/next/last/goto commands with a synchronized sequence model                                                   | Floating thumbnail list and direct cell selection                   |

See [`compatibility.md`](compatibility.md) for the authoritative status, evidence,
and exact known difference of every requirement.

## Known differences and unknowns

- Normal user-initiated `.inkpod` save/open still waits synchronously for the
  Core-engine work item. Autosave and image-processing tasks use asynchronous
  queue/task paths.
- Vector layers preserve their mutual z-order, but the current snapshot renderer
  draws vector content after precomposited raster tiles instead of supporting
  arbitrary raster/vector interleaving.
- Gap close, filters, effects, broadcast-color checks, and other algorithms whose
  proprietary legacy details are not independently specified use documented,
  deterministic Inkpod semantics. They are not claims of undocumented kernel
  compatibility.
- Batch output currently writes native `.inkpod` cells. The graph is extensible,
  but no undocumented legacy Batch or preset format is accepted or emitted.
- Native v2 blobs are separated but not compressed; compression remains an
  optional future container feature.
- DGA/CEL and legacy palette/chart/filter preset layouts remain `Unknown` with
  zero rights-cleared fixtures and zero measured read/write/round-trip variants.
- The generated MSIX is intentionally unsigned. Public distribution requires a
  protected production publisher credential and timestamp policy that must not
  be committed. Administrator install/installed-ABI/uninstall smoke remains
  optional release validation rather than an implementation gap.
- A future sandboxed frontend still needs byte/stream I/O and platform file-
  authority adapters; no Windows dependency is allowed in Rust domain crates.

## Recent verification

These rows intentionally keep only the latest representative results for each
currently relevant platform boundary.

| Date       | Platform                                             | Verification                                                                                                                                | Result                                                                                                                                                                                                                                        |
| ---------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-07-29 | Windows 11 x64, MSVC 19.51, stable Rust              | Rust format, zero-warning clippy, all-feature workspace tests; x64 Debug/Release CMake configure/build; non-install CTest; whitespace check | 138 Rust tests passed; strict Debug/Release build and unsigned MSIX assembly passed; both CTest presets passed 9/9. Native smoke covers the owned Tools palette frame/owner, all 37 command controls, checked state, focused dispatch, hide/show, monitor-safe placement, and DPI handling. |
| 2026-07-29 | macOS ARM64, CMake 4.4.0, AppleClang 21, Rust 1.95.0 | Rust format, zero-warning clippy, all-feature workspace tests; CMake configure/build and platform-independent CTest; C11/C++20 public-header syntax; whitespace check | 139 Rust tests plus doc-tests and 5/5 CTests passed. All 275 production commands have one route/state owner. The Windows-only palette build and native smoke remain for Windows CI. |
| 2026-07-29 | Windows 11 ARM64, MSVC 19.51, stable Rust            | Rust format, zero-warning clippy, all-feature workspace tests; fresh ARM64 Debug configure/build and MSIX assembly; CTest; normal-launch Tools palette capture | 140 Rust tests, strict build, MSIX assembly, and all 9 non-administrator CTests passed. Native smoke verifies the 3 Tools tabs, 8/21/8 controls, per-page visibility, z-order, bounds, scrolling, command state, and stable-ID Raster reselection followed by plane create/properties/convert/duplicate/reorder/delete/merge. Core negative coverage confirms singleton main-line duplication and incompatible merge are rejected without mutation. MSIX install/uninstall smoke requires elevation. Normal launch confirmed that every page renders its buttons; ARM64 Release was not rerun. |

## Maintenance rule

Update this file only when the current implementation summary, an active gap, or
a representative latest verification result changes. Replace superseded entries
instead of appending a chronological log. Update `compatibility.md` when a
requirement's status, evidence, or known difference changes.
