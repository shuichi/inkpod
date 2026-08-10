# Compatibility status

This document tracks only the current status, representative evidence, and
known difference for every requirement ID defined in
[`../SPEC.md`](../SPEC.md). Detailed behavior belongs to `SPEC.md`, ownership
and data flow to [`architecture.md`](architecture.md), exact format bytes to
[`file-format.md`](file-format.md), and current platform verification to
[`implementation-status.md`](implementation-status.md).

Status is one of `Not started`, `In progress`, `Experimental`, `Verified`, or
`Blocked`. A user-facing requirement is `Verified` only when its production UI
route and observable result are tested; infrastructure requirements may be
verified at their natural build, ABI, Core, format, or renderer boundary.

## Requirement matrix

| Requirement | Status | Representative evidence | Known difference / next work |
| --- | --- | --- | --- |
| `ARCH-001`, `ARCH-002` | Verified | CMake/Cargo source tracking, crate-boundary guards, OS-dependency scans, public integration tests, strict Rust/Windows builds | Rust domain crates contain no Windows API dependency |
| `ABI-001`, `ABI-002` | Verified | Header/export/catalog parity for 218 ABI v8 exports; C11/C++20 layout, NULL/short/stale/wrong-thread/ownership tests; brush/selection option enum/range/reserved validation; edit-target, owned Cell plan and ordered render-plan spans; snapshot queue, stale, device-loss and GUI smoke | ABI v1-v7 callers must rebuild against the current header |
| `IO-001` | Verified | V13/runtime epoch-10 stream/layout/digest/current-only tests; v12 rejection; EDIT schema-4 brush/selection/multi-target and Cell/frame round-trip; checkpoint equivalence/fallback/rejection; staged save/open, recovery, failure/cancel, compaction, ABI and Windows smoke | Non-v13 files are rejected without migration; compression code 0 only |
| `IO-002` | Verified | PNG/TIFF/TGA/BMP 8/16-bit, alpha/DPI round-trips; caller/file lifetime, sequence, export, ABI and Windows smoke | Flat common-raster export remains distinct from native save |
| `WIN-001` | In progress | Startup/shutdown, Help/About, owner-centered work-area-clamped modal dialogs, DPI/theme/layout, keyboard, MSAA/UIA, accessibility-name and GUI smoke; reproducible [release checklist](windows-release-checklist.md) | Japanese-only resources; high contrast, 200% DPI, complete screen-reader and Japanese IME work remains |
| `WIN-002` | Verified | Multi-window registry, focus/routing, close/shutdown, sink lifetime, single-instance activation, malformed IPC, x64/ARM64 native smoke | — |
| `WORKSPACE-001` | In progress | All eight modeless surfaces hosted by DockHost; one-direction split stacks with per-stack tabs; Color above the default Layer/Plane + Light Table + Subpalette/Reference tab stack; no duplicate pane-local Close control in tabbed Light Table content while Esc/floating close routes remain; dock/tab/float/hide/AutoHide policy; five presets; V7 round-trip and targeted V4/V5/V6 migration; transient non-persisted Job Progress; malformed HKCU recovery; monitor/DPI clamp; pointer/accessibility and native workspace smoke | Reference Check AutoHide edge buttons are not keyboard-reachable with F6/Tab |
| `WORKSPACE-002` | Verified | Issue-time command context, follow/pin/job policy, stale rejection, cross-window drag/move/copy and target-aware pane smoke | — |
| `SESSION-001` | Verified | File identity, multi-session/view isolation, savepoints, close scopes, Save As conflicts, autosave/recovery, malformed-open and native lifecycle smoke | Long operations serialize on one observable `CoreHost` lane |
| `SAFE-001` | Verified | Corruption/mutation corpus, bounded V13/checkpoint/asset/edit-target/selection-option rejection, staged publication, failure atomicity, fuzz-target declarations | Coverage-guided fuzz execution is not part of the latest local run |
| `PERF-001` | Verified | Fixed quick/full and native workloads; checksum/revision/reuse/rebuild/payload/sample/Present/resource gates; approved five-run envelopes | Canonical `revision-max` limitations remain; `checkpoint_open` has no retrospective timing envelope |
| `PKG-001` | Verified | Static-CRT x64/ARM64 builds, dependency checks, exact four-file ZIP, unsigned MSIX and extracted ABI smoke | ZIP has no file association; administrator MSIX install/uninstall is optional validation |
| `PORT-001` | Verified | Zero-Windows-import acceptance guard and shared Linux/macOS replay/workspace checks | Future sandboxed frontends still need byte/stream and file-authority adapters |
| `DOC-001` | Verified | Metadata, paper/frame/DPI equality, mixed-paper alignment, native round-trip and Windows dialog smoke | — |
| `CELL-001` | Verified | Public image/frame-mode table, every initial layer and RGBA8/16 save/reopen, invalid/overflow/UUID failure atomicity; owned-plan ABI negatives; one-dialog Core-plan summary; three-session Windows production smoke; x64 Release image/frame-mode, 16-bit three-Cell, properties and Cancel manual check | — |
| `DOC-002`, `DOC-003` | Verified | Stable-ID typed topology; tree-ordered bounded multi-target EditorState; grouped duplicate/delete/visibility/editability/compatible merge/convert capability matrix; no-op/invalid/stale/Undo/replay/save-reopen; ABI spans; Windows marker/accessibility/status/menu/smoke; x64 Release grouped main-line/color paste, one-step Undo and active-plane-only brush manual check | — |
| `RENDER-001` | Verified | R/V/R and V/R/V Core pixel/digest goldens; layer/plane reorder, visibility, 0/50/100% opacity, alpha, adjustment, thumbnail/export/save/reopen; ABI plan negatives; Windows offscreen GPU reorder and adjustment pixels plus device-loss smoke | — |
| `VIEW-001`, `VIEW-002` | Verified | Zoom/box/fit/1:1/pan/flip, ruler/guide/grid/snap/transparent-view Core, FFI, coordinate and Windows gesture/render smoke | The main-window zoom slider is intentionally absent |
| `VIEW-003` | Verified | Bounded locator sampling/neighborhood, DPI/flip/half-open coordinates, FFI buffers, target/pin/stale and native pane smoke | — |
| `VIEW-004` | Verified | Multi-view revision/history, per-view device-coordinate stroke placement, split/group/window move/copy, same/different-session isolation, active-stroke and native lifecycle smoke | — |
| `HIST-001` | Verified | Journal state-machine, full/checkpoint replay equivalence, inactive branches/assets, savepoints, cache rebuild, compaction and Windows Undo/Redo/dirty smoke | Compaction intentionally creates a new Genesis with empty history after explicit confirmation |
| `PAINT-001`, `PAINT-002`, `PAINT-003` | Verified | Stable-target stroke/path/shape/cleanup/width primitives; pressure, extreme/per-view device coordinates, preview/cancel, replay, ABI, Canvas and Windows smoke | Pencil is fixed at 1.0 document pixel; brush/eraser share the documented diameter range |
| `PAINT-004` | Experimental | Round/square and pressure footprints; fixed-point smoothing off/on and append batching equality; immutable exact native Binary/Grayscale/RGBA8/16 start-color predicate including alpha, detached same-color hits and selection clip; no-op/invalid/stale/overflow/cancel/Undo/Redo/replay/save-reopen; ABI v8 negatives and Windows Tool Options/Canvas smoke | x64 Release narrow two-color repair, shape/smoothing comparison, Cancel and Undo manual check pending |
| `FILL-001`, `FILL-002`, `FILL-003` | Verified | Connected/inclusion/closed-region goldens, tolerance/selection/gap/overflow/cancel/no-op/Undo, FFI, Windows target smoke, and non-mutating closed-fill drag preview | Gap close follows the documented deterministic native rule |
| `COLOR-001`, `COLOR-002` | Verified | Exact RGBA8/16, eyedropper sources, palette/chart/subpalette, main-line procedures, codecs, history/replay and Color-pane smoke | Display conversion does not replace stored exact-depth color |
| `SEL-001`, `SEL-002`, `SEL-003` | Verified | All shape/mode algebra, color/width operations, layer conversion, vector modes, preview/cancel, ABI and Windows gesture smoke | — |
| `SEL-004` | Verified | Normal/Tight/EnclosedInterior/Drawing/Boundary 8/16-bit goldens; edge/hole/tile/final-pixel bounds; aspect/center/rotation/45-degree and round/square pressure/screen trace; no-op/invalid/stale/Undo/Redo/replay/save-reopen; ABI v8 negatives, Windows production preview smoke, and x64 Release manual mask/geometry/modifier check | — |
| `CLIP-001` | Verified | Ordered multi-plane typed clipboard; exact RGBA8/16, origin and tree order; cross-plane vector fill topology; coordinate preservation, converted-plane transaction, external DIB, ownership, Cancel/Undo/replay and Windows grouped copy/paste smoke | Standard import accepts validated 24/32-bit DIB layouts |
| `XFORM-001`, `XFORM-002` | Verified | Destructive document transforms, frame/guide consistency, floating selection lifecycle, retry/cancel/Undo, FFI and Windows dialog/handle smoke | — |
| `LT-001` | In progress | Set/item administration, ordering, visibility, transform/color/mode/opacity, persistence, follow/pin isolation and Windows palette smoke | Previous/next-N bulk registration and automatic opacity-step controls remain |
| `LT-002` | Verified | Alignment, move/sample/reload/edit-image swap, reference viewer, history isolation, ABI and target-aware Windows smoke | — |
| `SEQ-001` | In progress | Natural-order discovery, thumbnails, direct selection, navigation, dirty-cancel, mixed formats, ABI and Windows palette smoke | Automatic-save-on-switch and endpoint-loop preferences remain; current UI prompts and stops at endpoints |
| `SEQ-002` | Verified | Motion Core/ABI state and Windows FPS/loop/pause/step/shortcut smoke | — |
| `SHORT-001` | Verified | Full command catalog, prefix/conflict/reset/persistence, text-focus guard, FFI resolve and Windows key/menu smoke | Palette `1`-`0` retains its documented no-match fallback |
| `FILTER-001`, `FILTER-002` | Verified | Exact 8/16-bit selection/alpha goldens, catalog, preview/cancel/Undo, shared Job Progress pane, ABI and Windows editor smoke | Filter work runs on the Core worker and reports task progress |
| `EFFECT-001` | Verified | Gradient/airbrush/boundary/blur/stamp acceptance, deterministic pressure/gesture/bounds, ABI and Windows smoke | — |
| `ADJUST-001` | Verified | Non-destructive order/source/opacity/visibility/save-reopen, malformed metadata, alpha RGB preservation, ABI and Windows smoke | — |
| `BATCH-001`, `BATCH-002`, `BATCH-003` | Verified | Current-version graph round-trip, complete operation catalog, dry-run/preview/shared Job Progress/cancel/failure/atomic output, FFI and Batch-pane smoke | Native `.inkpod` is the only output format; boundary airbrush begins with two colors |
| `VECTOR-001`, `VECTOR-002` | Verified | Geometry/render goldens, path editing, width/connect/select/vectorize/rasterize transactions, mixed-order zoom/save/replay, ABI and Windows Canvas smoke | Rasterize preserves its source layer |

## Maintenance rule

Keep one current row for every `SPEC.md` requirement ID. Replace evidence and
known differences instead of appending milestone or dated verification logs.
Move exact samples to the relevant baseline document and current platform
results to [`implementation-status.md`](implementation-status.md).
