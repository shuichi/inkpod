# C ABI

The public ABI is `include/inkpod/core_ffi.h`. ABI version 1 is the M0 contract.
All numeric fields use fixed-width C types; every extensible public structure
starts with `struct_size`, and configuration/view structures also expose an ABI
version or feature flags where applicable.

## Ownership and lifetime

- `inkpod_core_create` allocates `InkpodCore`; the caller owns the returned
  opaque pointer.
- `inkpod_core_destroy` takes `InkpodCore**`, frees the allocation, and writes
  null. Calling it again with the same owner variable is a successful no-op.
- `inkpod_core_build_snapshot` allocates an immutable `InkpodSnapshot`.
- `inkpod_snapshot_get_view` returns borrowed spans that remain valid only
  while that snapshot is live. No per-tile call is needed.
- `inkpod_snapshot_release` takes `InkpodSnapshot**`, frees the allocation, and
  writes null. Calling it again with the same owner variable is a successful
  no-op.
- A copied or aliased stale opaque pointer must never be used after its owner
  variable releases it. The pointer-to-pointer API prevents accidental repeat
  release through the normal ownership path; it cannot make arbitrary dangling
  aliases valid.

Rust-owned storage is always released by Rust. No Rust `Vec`, `String`, enum
layout, reference, trait object, or panic crosses the boundary. The header is
compiled as both C11 and C++20 in the test suite.

## Threading

`InkpodCore` is single-writer. Create, dispatch, snapshot build, and destroy
must occur on its creating thread; a violation returns
`INKPOD_STATUS_WRONG_THREAD` without consuming the handle. Once published, an
immutable snapshot can be read and released on a renderer thread. M0 performs
no callbacks and holds no Core lock while entering C++.

## Validation and failures

Every fallible export catches Rust panics and converts them to
`INKPOD_STATUS_PANIC`. Boundary code checks null, natural alignment, structure
size, ABI version, feature/reserved bits, enum values, bounded counts, and
integer size conversions before accessing an array. As in any C ABI, the caller
must still supply readable/writable storage for a non-null pointer.

The most recent error text is thread-local UTF-8, not shared mutable global
state. Call `inkpod_error_message_size` to get the required byte count including
the trailing NUL, then call `inkpod_error_message_copy`. `out_written_bytes`
excludes the NUL. A too-small buffer returns
`INKPOD_STATUS_BUFFER_TOO_SMALL` without replacing the original diagnostic.

## M0 command and snapshot semantics

`INKPOD_COMMAND_NO_OP` is the sole M0 command. It exercises real batch
validation and produces no document revision. Unknown kinds and nonzero flags
are rejected, rather than silently succeeding. An M0 snapshot has revision zero
and a null/zero tile span. Pixel formats and tile payloads are reserved for M1.

