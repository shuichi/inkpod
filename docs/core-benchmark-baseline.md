# Rust Core workflow benchmark baseline

This document defines the reproducible comparison contract for
`rust/inkpod-core/benches/core_workflows.rs`. It is a living baseline for the M3
and later refactoring milestones, not a wall-clock performance gate. Update it
only when a reviewed benchmark input or semantic output intentionally changes.

## Commands and profiles

```text
cargo bench --package inkpod-core --bench core_workflows -- --quick
cargo bench --package inkpod-core --bench core_workflows
```

Both commands use the release benchmark profile and the same seven scenarios.
Quick is the bounded Linux CI profile; full increases inputs for local
before/after comparison. Neither profile reads or writes native `.inkpod`
files. Batch uses in-memory sequence cells, and its dry-run asserts that its
absent output directory remains absent.

| Parameter | Quick | Full |
| --- | ---: | ---: |
| sparse document dimensions | `MAX_RASTER_DIMENSION` square | same |
| sparse allocated tiles | 8 | 32 |
| Undo/Redo edits | 12 | 48 |
| light-table document | 128 square, 3 references | 256 square, 6 references |
| vector document | 128 square, 8 closed paths/fills | 256 square, 32 closed paths/fills |
| Batch sequence | 4 cells at 16 square | 16 cells at 32 square |

## Output and assertions

Every scenario prints exactly one line with this stable schema:

```text
inkpod-core-workflows profile=<profile> scenario=<name> iterations=<n> input_items=<n> output_items=<n> reused_items=<n> document_revision=<n> history_entries=<n> successes=<n> failures=<n> checksum=<hex> elapsed_ns=<n>
```

`elapsed_ns` is observational. The benchmark fails on semantic checksum or
counter drift. The scenario assertions cover the following contracts:

| Scenario | Hard assertion |
| --- | --- |
| `sparse_snapshot` | only the deliberately allocated sparse tiles render |
| `dirty_tile_rebuild` | one edited tile rebuilds and all other tile revisions remain reusable |
| `pan_zoom_snapshot` | document revision, history, pixels, and all tile revisions remain unchanged |
| `undo_redo` | every edit is one history entry, Undo reaches the clean savepoint, and Redo restores the exact checksum |
| `light_table_composite` | all references contribute to the expected full tile grid and checksum |
| `vector_snapshot` | segment/fill counts, zero raster snapshot tiles, and rasterized pixels match |
| `batch_preview` | one invalid graph is rejected, all valid inputs dry-run successfully, and no output is generated |

The checksum is local FNV-1a over fixed-width public semantic data. It excludes
wall-clock time, allocator addresses, cache allocation order, and Batch output
paths. The expected values embedded in the benchmark are:

| Scenario | Quick checksum | Full checksum |
| --- | --- | --- |
| `sparse_snapshot` | `517ed7ae78bf0487` | `439040e0244d5773` |
| `dirty_tile_rebuild` | `9e13576def6f539b` | `a33f7534fcdd61e7` |
| `pan_zoom_snapshot` | `517ed7ae78bf0487` | `439040e0244d5773` |
| `undo_redo` | `3f1053b9fde37d35` | `a2c1a74e7f9781a3` |
| `light_table_composite` | `255ab9bad114dfdd` | `77f63d83e130185f` |
| `vector_snapshot` | `aeb93faa005c3a18` | `99c64854735f7960` |
| `batch_preview` | `f31d31fe1bb00fd7` | `6732b8b0a6565d03` |

## M2 reference run

The initial baseline was captured on 2026-07-30 on macOS ARM64 with Rust 1.95.0.
The table reports the median of three warmed release runs. These numbers are a
same-machine comparison reference only.

| Scenario | Quick median ns | Full median ns | Quick semantic counters | Full semantic counters |
| --- | ---: | ---: | --- | --- |
| `sparse_snapshot` | 735,166 | 3,143,417 | 8 tiles, revision/history 9/8 | 32 tiles, revision/history 33/32 |
| `dirty_tile_rebuild` | 82,250 | 90,250 | 1 rebuilt, 7 reused | 1 rebuilt, 31 reused |
| `pan_zoom_snapshot` | 1,417 | 9,625 | 8 reused | 32 reused |
| `undo_redo` | 2,529,584 | 11,250,375 | 12 entries, 36 successful steps | 48 entries, 144 successful steps |
| `light_table_composite` | 1,769,125 | 13,713,916 | 3 references, 4 tiles | 6 references, 16 tiles |
| `vector_snapshot` | 59,910,334 | 301,657,959 | 32 segments, 8 fills | 128 segments, 32 fills |
| `batch_preview` | 224,541 | 1,076,667 | 4 dry-run successes, 1 rejected invalid graph | 16 dry-run successes, 1 rejected invalid graph |

Compare performance changes on the same machine and profile using repeated-run
medians. Correctness is determined by the assertions and fixed counters, never
by an absolute elapsed-time threshold.
