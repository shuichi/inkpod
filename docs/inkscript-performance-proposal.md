# InkScript performance contract proposal

Status: **quick contract approved and implemented in M14**. The generic
InkScript milestone prompt following M13 approved the recorded quick workload,
counter, checksum, sample policy, reference environment, and 64–107 ms envelope
without modification. The active executable contract and current samples are
maintained in [`core-benchmark-baseline.md`](core-benchmark-baseline.md). The
full contract remains reserved and unimplemented until M36.

## Contract intent

The scenario protects the complete private M09–M12 route: exact-current source
parse, static compile, selector binding, inline asset freeze, multi-item staged
execution, current-native encode/install, deterministic failure and cancellation,
and cache-free reopen. Wall-clock is considered only after every semantic counter
and checksum is exact. Fixture construction is outside the timed interval.

The existing ten `core_workflows` scenarios, their inputs, checksums, counters,
timed intervals, and approved envelopes remain byte-for-byte unchanged. The
InkScript scenario has its own proposed environment range and does not recalibrate
or widen an existing range.

## Deterministic fixture

- Source ID is 913. It requires InkScript file v1, procedure catalog v1, and
  replay epoch 24, reads a naturally ordered folder, writes duplicate current
  `.inkpod` outputs, uses `failure = continue`, and has no wait.
- Every input is an empty 4-by-4 current-v27 Cell with UUIDs beginning at
  `0x1001`. Its exact native image is 6,192 bytes. Inputs are named
  `cell1.inkpod` through `cellN.inkpod`.
- One binding selects the unique color plane. One selection-empty assertion
  precedes the steps.
- Every step invokes the existing `set_plane_properties` catalog entry. Names
  repeat in pairs (`Probe A`, `Probe A`, `Probe B`, `Probe B`, ...), so exactly
  half the invocations commit and half are semantic no-ops. The catalog formula
  is one maximum invocation and one work unit per enabled step, with zero output
  IDs, catalog asset bytes, and output growth.
- One inline straight-sRGB RGBA8 asset is declared and frozen even though this
  catalog slice does not consume an asset role. Payload bytes come from xorshift64
  seed `0x494e4b5343524950`, applying `x ^= x << 13`, `x ^= x >> 7`, then
  `x ^= x << 17` and emitting each state little-endian. The descriptor, Base64,
  canonical `AssetId`, decoded/read/copy counts, and source size are therefore
  deterministic.
- The success run installs every item. A second run selects the first item and
  fails its first temporary write with the exact `Save` outcome. A third run
  selects the first item and returns cancellation immediately before atomic-install
  linearization. Both negative runs execute and encode the staged item before the
  injected boundary; neither publishes an output. Every successful output is then
  decoded into a fresh Core and fully replayed without a checkpoint cache.
- The 64-bit FNV-1a checksum covers the static compile digest; output keys and
  BLAKE3 byte digests; reopened document/editor state, history, and ID authorities;
  ordered reports and statement outcomes; source/token/CST/dependency/work/asset
  counters; installed bytes; and replayed Commit count. Exact failure reason and
  all remaining counters below are separate hard assertions.

## Proposed profiles and semantic gates

Let `S` be enabled steps, `N` be success-run items, `A` be the square asset side,
and `R = N + 2` be attempted items including the failure and cancellation probes.
The work contract is:

```text
asset_bytes             = 4 * A * A
catalog_invocations     = S
catalog_work_units      = S
dependency_edges        = S
binding_resolutions     = R
statement_evaluations   = (S + 1) * R
runner_invocations      = S * R
commits                 = no_ops = (S / 2) * R
installed               = N
failed                  = 1 (Save)
cancelled               = 1 (before install linearization)
cache_free_reopens      = N
replayed_commits        = (S / 2) * N
runner_native_read      = 6,192 * R
```

| Gate | Quick (M14) | Full (M36) |
| --- | ---: | ---: |
| `S` / success-run `N` / asset side `A` | 128 / 4 / 256 | 1,024 / 8 / 2,048 |
| source bytes | 371,176 | 22,537,324 |
| lexer tokens / CST nodes | 7,965 / 2,000 | 61,725 / 15,440 |
| parameters / bindings / asserts / steps | 0 / 1 / 1 / 128 | 0 / 1 / 1 / 1,024 |
| dependency edges | 128 | 1,024 |
| catalog max invocations / work units | 128 / 128 | 1,024 / 1,024 |
| asset declarations / unique assets | 1 / 1 | 1 / 1 |
| logical / unique / inline-decoded / copied asset bytes | 262,144 each | 16,777,216 each |
| authorized asset read bytes | 0 | 0 |
| input native bytes / runner native-read bytes | 24,768 / 37,152 | 49,536 / 61,920 |
| attempted items / binding resolutions | 6 / 6 | 10 / 10 |
| statement evaluations / invocations | 774 / 768 | 10,250 / 10,240 |
| commits / no-ops | 384 / 384 | 5,120 / 5,120 |
| installed / failed / cancelled | 4 / 1 / 1 | 8 / 1 / 1 |
| installed output bytes | 91,584 | 1,127,488 |
| cache-free reopens / replayed Commits | 4 / 256 | 8 / 4,096 |
| checksum | `0f84d2c54cfe1e2c` | `17c636b92b1aebf1` |

The timed interval begins immediately before static compile and ends after plan,
all three runs, cache-free reopen, checksum construction, and counter assertions.
It excludes deterministic source/asset/native-input construction and process
startup. The quick command runs in one Release unit-test process with one test
thread. Routine gating discards one warm-up process and retains at least five
independent measured processes; the reference batch below retains nine quick and
five full processes.

Untimed hard gates retain the existing exact-current tests for malformed/current
version rejection, lowered-resource overflow, cancellation, stale identity,
failure atomicity, save/reopen, Undo/Redo, ID high-watermarks, and document/editor
savepoints. They are not replaced by the performance checksum.

## Candidate-axis exhaustive measurement

M13 used a temporary crate-private Release probe and removed it after measurement.
The following is the complete one-factor candidate search: every listed step,
item, and asset candidate was measured once while the other two axes were fixed.
The selected full compound was then measured separately. No candidate result
changed an existing harness or envelope.

| Step count (`N=4`, 256 KiB asset) | Total (ms) | Checksum |
| ---: | ---: | --- |
| 32 | 25.3212 | `079db44041adae3a` |
| 64 | 44.1625 | `06b43df0abe3aeaa` |
| 128 | 86.2013 | `0f84d2c54cfe1e2c` |
| 256 | 194.8753 | `5b7fe36956216d3b` |
| 512 | 502.9213 | `0f5459b862c65be7` |
| 1,024 | 1,592.3675 | `4b3423584c8062ef` |
| 2,048 | 5,637.5649 | `b210261d1b52a4e2` |

| Item count (`S=128`, 256 KiB asset) | Total (ms) | Checksum |
| ---: | ---: | --- |
| 1 | 62.8315 | `8c71ce6de651980f` |
| 2 | 69.5318 | `07b8037d2310f57d` |
| 4 | 81.8580 | `0f84d2c54cfe1e2c` |
| 8 | 106.8738 | `99bf17cc1989077d` |
| 16 | 157.8051 | `2da5f619c9c97cad` |

| Asset payload (`S=128`, `N=4`) | Total (ms) | Checksum |
| ---: | ---: | --- |
| 16 KiB | 45.6110 | `40b9919b69d80734` |
| 64 KiB | 52.8105 | `127d86765d4505b0` |
| 256 KiB | 81.6319 | `0f84d2c54cfe1e2c` |
| 1 MiB | 201.0423 | `a3765f0860ce3a85` |
| 4 MiB | 675.2434 | `d22aaa13c98a6742` |
| 16 MiB | 2,783.5557 | `8e4085853fdcb990` |

The selected full compound (`S=1,024`, `N=8`, 16 MiB asset) measured
19,976.5163 ms in the candidate run. This deliberately protects the observed
step-count/large-inline-asset interaction rather than extrapolating independent
axis timings.

## Proposed environment envelope

Proposed range ID:
`windows-x64-ryzen-9-9950x3d-release-2026-08-15-inkscript-v1`.
It applies only to Windows build 26200.9168 on the MSI MS-7E26 host with an AMD
Ryzen 9 9950X3D, 127.6 GiB memory, x86_64-pc-windows-msvc, Rust/Cargo 1.97.1,
LLVM 22.1.6, MSVC 19.51.36252.0, Release profile, and the Windows Balanced power
scheme. A materially different host, target, toolchain, or power mode needs its
own explicit range.

Run 1 for each profile was an unmeasured warm-up process. The complete accepted
independent-process samples are:

| Profile | Accepted total samples (ns) | Median |
| --- | --- | ---: |
| quick | 85,230,900; 87,304,700; 84,489,500; 86,339,300; 85,838,200; 85,097,100; 85,237,900; 85,372,200; 86,873,700 | 85,372,200 |
| full | 21,118,546,900; 20,455,099,800; 21,988,093,300; 20,432,645,500; 20,310,961,100 | 20,455,099,800 |

| Protected score | Proposed range | Reference median |
| --- | ---: | ---: |
| quick InkScript pipeline | 64–107 ms | 85.3722 ms |
| full InkScript pipeline | 15.3–25.6 s | 20.4550998 s |

The proposed bounds are the rounded 75–125% band used by the existing x64
output-color-guard contract. The lower edge is diagnostic only when every
semantic gate is exact. An upper-edge breach requires a second independent batch
of at least five processes. Approval never permits automatic widening.

## M14 implementation and reserved M36 changes

M13 left no probe or harness code in the tree. M14 implemented the approved
quick slice as follows:

- M14 added a `#[cfg(test)]` crate-private InkScript benchmark module and one
  ignored Release quick runner. It uses the existing private compiler/planner/
  runner directly, emits the same stable key-value style as `core_workflows`, and
  adds explicit stage reports so failure/cancel work is counted even though a
  terminal item report intentionally omits discarded staged execution details.
- No Rust public re-export, Cargo feature, C ABI symbol, Windows route, product
  command, file-open association, or production asset/catalog entry is added.
- `rust/inkpod-core/benches/core_workflows.rs`, its ten scenarios, both checksum
  arrays, and all approved envelopes receive no code or data change. M14 runs the
  existing quick benchmark separately to prove that invariant.
- M14 records the approved quick range and both the approval and implementation
  sample batches in `core-benchmark-baseline.md`. It does not implement the full
  runner.
- M36 later implements and runs only the approved full fixture above, without
  changing its seed, counters, checksum algorithm, timed interval, or envelope.

Any change to this workload, seed, formulas, counters, checksum, timed interval,
sample policy, reference environment, or proposed bounds requires a new proposal
and explicit approval.
