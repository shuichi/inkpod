# Architecture

## M0 component boundary

inkpod has one state owner and one platform adapter. The dependency direction
is deliberately one-way:

```text
CMake -> Cargo -> inkpod-ffi -> inkpod-core
   |
   +-> Win32 application -> versioned C ABI
                         -> D3D11/DXGI/Direct2D Canvas
```

`inkpod-core` contains deterministic, platform-independent state transitions
and immutable render snapshots. It forbids unsafe Rust and has an architecture
test that rejects Windows/frontend API tokens anywhere below its `src`
directory. The crate does not depend on `inkpod-ffi`.

`inkpod-ffi` is the only `staticlib`. It translates fixed-layout C structures
to typed Core commands, contains panics at every exported fallible boundary,
and owns all opaque allocations returned through the ABI. C++ does not mirror
document state.

The Windows frontend owns the process entry point, Common Controls and COM
lifetime, window handles, the message loop, and GPU objects. Each Canvas owns a
flip-model DXGI swap chain, D3D11 device, Direct2D device context, and target
bitmap. Hardware device creation falls back to WARP. Resize, occlusion, and
device removal/reset are handled without discarding the Rust Core.

M0 snapshots contain a revision and an empty tile span. M1 will add typed,
immutable tile records without changing the snapshot ownership model.

## Build graph

CMake is the build entry. A custom command declares the Rust source/manifests
as inputs, the profile-specific static library as its output, and Cargo's rlib
as a byproduct. C++ targets depend on an imported static-library target backed
by that output. Therefore an unchanged Rust library is not rebuilt merely
because a C++ target is built.

The checked-in presets use single-configuration Ninja builds with the MSVC x64
developer environment. A single configuration ensures the Cargo `debug` or
`release` artifact always matches the active CMake configuration and the `/MD`
runtime selection used by Rust's MSVC target.

## Initialization and shutdown

The application initializes Common Controls, COM, the hidden/main window and
Canvas renderer, then the Rust Core. Shutdown reverses the last two ownership
steps: Core handles are destroyed before the main window releases renderer and
GPU resources; COM is uninitialized last. Failures unwind only resources that
were successfully initialized.
