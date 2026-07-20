# inkpod

inkpod is a new, maintainable implementation of an animation-paint workflow.
The project is at milestone M0: a platform-independent Rust Core is connected
through a versioned C ABI to a Windows 11 Win32/Direct2D application shell.

The repository's implementation contract is [AGENTS.md](AGENTS.md), with
feature behavior and milestones defined by [PROMPT.md](PROMPT.md).

## Prerequisites

Building the complete Windows application requires:

- Windows 11
- Visual Studio 2022 or 2026 with the **Desktop development with C++** workload, the
  x64 MSVC toolchain, and a Windows SDK
- CMake 3.25 or newer
- Ninja
- Rust 1.85 or newer using the stable MSVC toolchain

Open an **x64 Native Tools Command Prompt** or **Developer PowerShell** for the
installed Visual Studio version, then verify that the required tools are available:

```powershell
cl
cmake --version
ninja --version
rustc --version
cargo --version
```

## Build and test the Windows application

CMake is the build entry point for the complete application. It invokes Cargo
to build the Rust `inkpod-ffi` static library and then links it into the MSVC
targets, so a separate `cargo build` step is not required.

From the repository root, configure, build, and test a Debug build:

```powershell
cmake --preset windows-x64-debug
cmake --build --preset windows-x64-debug
ctest --preset windows-x64-debug
```

Run the Debug application with:

```powershell
.\build\windows-x64-debug\inkpod.exe
```

For a Release build, use the corresponding Release preset:

```powershell
cmake --preset windows-x64-release
cmake --build --preset windows-x64-release
ctest --preset windows-x64-release
.\build\windows-x64-release\inkpod.exe
```

The Windows smoke test creates a hidden main window and hardware- or
WARP-backed Direct2D Canvas, requests an empty snapshot through the C ABI,
exercises resize, DPI-target recreation, and simulated device-resource loss,
renders once, and shuts down. At milestone M0, the executable is an application
shell rather than a complete painting application.

## Validate the Rust workspace only

The platform-independent Rust workspace can be validated anywhere a compatible
stable Rust toolchain is available:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

On systems where enterprise Code Integrity policy blocks a locally built,
unsigned Rust FFI test executable, compile all tests and run the Core tests
separately:

```text
cargo test --workspace --all-features --no-run
cargo test --package inkpod-core --all-features
```
