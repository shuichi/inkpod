# InkScript traceability

This document is the M00 traceability authority connecting the product requirements in
[`SPEC.md`](../SPEC.md), the normative language contract in
[`INKSCRIPT.md`](../INKSCRIPT.md), the machine-readable registries, milestones, and
planned evidence. It is not a command reference. M23 froze the completed command signatures in
[`catalog-v2.json`](../schemas/inkscript/catalog-v2.json); the derived presentation is
[`inkscript-command-reference.md`](inkscript-command-reference.md).

## Ratified contract

M00's starting contract was explicitly accepted before M01 implementation, and M21's
selector correction was separately approved before implementation:

- InkScript file version 2, procedure catalog version 2, replay epoch 23, `.inkpod`
  top-level version 26, and C ABI version 15 are the exact-current registry values.
- M07's approved exact-current registry schema v2 supersedes registry schema v1 solely to add
  closed catalog-owned enum, record, and constructor definitions; schema v1 is not accepted.
- M21's approved option A changes only the shooting-frame selector from a layer owner to the
  document-owned singleton required by SPEC/Core. It supersedes file/catalog v1 with exact-current
  v2 while leaving registry schema v2, replay epoch 23, `.inkpod` v26, and C ABI v14 unchanged.
- `schemas/inkscript/language-v2.json` freezes the command-independent language core.
  Later discovery of a missing grammar, section, type, selector, assert, asset, or
  resource rule requires an explicit InkScript file-version decision; it is not a silent
  M01+ fix.
- Catalog v2 remained a non-production draft through M22. M23 proved the bijection among all 84
  replayable primitives, exact command entries, owners, implementations, and equivalence IDs,
  removed the draft, and froze `catalog-v2.json` as the production Rust contract. Owner additions
  made before M23 did not bump catalog version; any post-freeze change requires a new versioned
  resource, old-version rejection, and regenerated reference.
- Exact-source equivalence compares canonical state/pixel digests, ID high-watermarks,
  typed result roles and output ordinals, pre/post state digests, stable input/output/asset
  roles, and canonical invocations. Rebound execution guarantees deterministic execution
  only after every external strict selector is explicitly replaced; it does not claim
  source UUID, raw ID, or source-state equality.
- M24's journal exporter is a read-only orchestration query, not a catalog procedure. It maps
  the already frozen typed runtime invocation and retained asset authority back to an
  exact-current semantic fragment, emits exact parent state/ID-allocation assertions and
  strict selectors, and reuses the same canonical executor for equivalence. It changes no
  file grammar, command signature, primitive semantics, replay epoch, native schema, or ABI.
- M25 exposes source parse/diagnostic copy/static compile/journal fragment export through C ABI
  v15. The FFI only translates versioned fixed-width records, bounded spans, and opaque ownership
  into the public Rust APIs; private catalog models and executors remain unreachable, and no
  Windows command, file filter, clipboard, plan execution, report, or product UI route is added.
- Continuous Fill lowers one source seed to one step and therefore produces zero through
  N ordinary Commits. A contiguous, non-semantic `editor_group` is the only lossless 1:N
  link back to one legacy Batch operation; it cannot alter execution or Commit boundaries.
- M13 must present the complete InkScript quick/full workload, semantic counters,
  checksums, samples, environment, and proposed envelope and stop at `[~]`. M14 may change
  the benchmark harness or envelope only after that explicit approval. Existing workload,
  counter, `revision-max`, and envelope contracts are unchanged by M00.
- Existing `.inkbatch` v2, Batch UI, and Batch ABI remain production until M29C shadow
  parity and the separately accepted M34 cutover. M35 removes the test-private legacy
  implementation only after cutover. No production `.inkbatch` importer is added.

## Batch parity matrix

Entries named `INKS-TRACE-*` and `INKS-PARITY-*` are stable planned evidence IDs, not
passing tests in M00. M00's actual registry tests are listed in the next section.

| Existing requirement | InkScript requirements | Owning milestones | Required planned evidence |
| --- | --- | --- | --- |
| `BATCH-001` | `SCRIPT-001`, `SCRIPT-002`, `SCRIPT-003`, `SCRIPT-005` | M04–M05B, M11–M12, M28A–M29C | `INKS-TRACE-BATCH-001`, `INKS-PARITY-BATCH-001`: parse/emit/save/reopen, group/order/enable/set and shadow result parity |
| `BATCH-002` | `SCRIPT-002`, `SCRIPT-005` | M07–M09, M29C | `INKS-TRACE-BATCH-002`, `INKS-PARITY-BATCH-002`: every legacy operation to grouped steps to canonical invocations and direct result parity |
| `BATCH-003` | `SCRIPT-003`, `SCRIPT-005` | M11–M12, M26–M29C | `INKS-TRACE-BATCH-003`, `INKS-PARITY-BATCH-003`: immutable plan, outcome/temp/output/cancel/failure/report and atomic install parity |
| `BATCH-004` | `SCRIPT-002`, `SCRIPT-003`, `SCRIPT-004`, `SCRIPT-005` | M05A–M05B, M08–M09, M28B–M29C | `INKS-TRACE-BATCH-004`, `INKS-PARITY-BATCH-004`: 1:N fill Commit, exact-depth pair/ambiguity, typed destination and transient parameters |

M29C must compare input order, output plan, each-run resolution, canonical procedure
sequence, state/composite digest, history, Undo/Redo, next IDs, report, semantic work
counters, save/reopen, cache-free replay, and failure atomicity before cutover.

## M00 machine-readable ownership

[`owner-manifest-v2.json`](../schemas/inkscript/owner-manifest-v2.json) assigns all 84
current replayable `PrimitiveId` values exactly once. The allocation is deliberately
reviewable before signatures are added:

| Owner | Replayable primitives | Scope |
| --- | ---: | --- |
| M07 | 7 | legacy property/conversion and document transforms |
| M08 | 6 | legacy fill, replacement, separation, filter, boundary-airbrush and dust adapters |
| M15 | 13 | paper/frame and remaining document tree operations |
| M16 | 8 | color metadata and guide/grid operations |
| M17 | 3 | raster stroke, geometry and canonical raster import |
| M18A | 1 | gradient |
| M18B | 11 | gesture effects, alpha, adjustment and scoped color operations |
| M19 | 11 | selection, selected-source restoration and floating selection |
| M20 | 8 | vector operations |
| M21 | 3 | annotation, shooting frame and vanishing point |
| M22 | 13 | replayable Light Table set/item operations |

`LIGHT_TABLE_SWAP_WITH_ACTIVE` is the sole current catalog exclusion because it is
session-only. Query, view, preview, export, save/open, history-control, and frontend
command IDs remain outside the manifest. M24's journal-fragment query therefore remains
outside the 84-entry procedure catalog while exhaustively consuming its typed runtime variants.

## Registry and completeness evidence

The `inkscript_registry` integration target provides these current checks:

- `inkscript_registry_meta_schema_and_production_catalog_are_closed`
- `inkscript_registry_json_rejects_duplicate_malformed_and_overflowing_input`
- `inkscript_language_core_is_closed_and_references_resolve`
- `inkscript_owner_manifest_is_a_bijection_with_replayable_primitives`
- `inkscript_production_catalog_is_bijective_with_runtime_and_equivalence_evidence`
- `inkscript_generated_command_reference_has_no_drift`
- `inkscript_versions_and_traceability_match_repository_contracts`
- `inkscript_removed_draft_is_unreachable_from_production`
- `inkscript_private_typed_models_remain_unreachable_from_core_ffi_and_windows`

PowerShell `Test-Json -SchemaFile` additionally validates the language, production catalog, and owner
documents against the formal Draft 2020-12 meta-schema. The Rust tests validate the closed
meta-schema and JSON syntax, duplicate/malformed/overflow rejection, language type/nonterminal
references, unique field/order/type names, current version constants, the 84-entry
primitive-owner bijection, the one explicit session-only exclusion, requirement/document
drift, immutable catalog fingerprint, generated-reference drift, public Rust ownership, absence of
the removed draft, and absence of private typed catalog models or Windows InkScript routes.

## M24 journal-fragment evidence

Six public exporter contracts plus one inline-asset unit contract cover active and inactive
one/linear Commit selection, Genesis, non-Commit/nonlinear rejection, exact parent assertions,
strict and strict-source-only binding, typed scalar/list result references, retained asset
deduplication, caller-lowered resource bounds, cancellation, source atomicity, and owned
`Send + Sync` output. Seven M17–M22 family fixtures additionally apply the emitted fragment to a
cache-free parent and compare final state/pixel digest, ID high-watermark, pre/post digest,
schema-ordered input/output/asset IDs, and the full canonical procedure sequence. The export
replay path does not materialize history visualization summaries or thumbnails.

## M25 C ABI evidence

Three `inkpod-ffi` contracts plus the C header/export drift, C11 include, C++20 ABI smoke, and
route-inventory gates cover the exact-current ABI v15 source/compiler/export boundary. They prove
bounded source ownership, two-stage original-text and canonical-fragment copy, atomic strided
diagnostic records with packed UTF-8, stored-default/typed-override static compilation through the public Rust compiler, and
journal export through the M24 public exporter. Negative evidence covers v14 rejection, NULL,
misalignment, short records, unknown flags and enums, oversize spans, Cancel, resource limits,
stale controller/session tokens, stale Core generation, wrong-thread access, and idempotent NULL
release. No CST/AST/catalog node, per-token/per-node call, second model/executor, execution/report,
or Windows product route is exposed.
