# inkpod

inkpod is a new, maintainable implementation of an animation-paint workflow.
The project is at milestone M0: a platform-independent Rust Core is connected
through a versioned C ABI to a Windows 11 Win32/Direct2D application shell.

The repository's implementation contract is [AGENTS.md](AGENTS.md), with
feature behavior and milestones defined by [PROMPT.md](PROMPT.md).

## Build and test

Rust validation works anywhere a stable Rust toolchain is available:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

On Windows, open an x64 MSVC developer shell and use CMake as the build entry:

```text
cmake --preset windows-x64-debug
cmake --build --preset windows-x64-debug
ctest --preset windows-x64-debug
```

The Windows smoke test creates a hidden main window and hardware- or
WARP-backed Direct2D Canvas, requests an empty snapshot through the C ABI,
renders once, and shuts down.

