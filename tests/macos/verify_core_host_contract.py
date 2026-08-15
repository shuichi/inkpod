#!/usr/bin/env python3
"""Verify the source boundary for the macOS M1 Core owner-thread host."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def fail(message: str) -> None:
    raise ValueError(message)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"cannot read {path}: {error}")


def require(text: str, token: str, source: str) -> None:
    if token not in text:
        fail(f"{source} must contain {token!r}")


def reject(text: str, token: str, source: str) -> None:
    if token in text:
        fail(f"{source} must not contain {token!r}")


def verify(repository: Path) -> None:
    bridge = repository / "apps/macos/CoreBridge/Swift"
    host_path = bridge / "CoreHost.swift"
    request_path = bridge / "CoreRequest.swift"
    result_path = bridge / "CoreResult.swift"
    owner_path = bridge / "CoreOwnerThread.swift"
    tests_path = repository / "apps/macos/Tests/Integration/CoreHostIntegrationTests.swift"
    main_path = repository / "apps/macos/Tests/Integration/CoreHostIntegrationMain.swift"
    project_path = repository / "apps/macos/Inkpod.xcodeproj/project.pbxproj"
    parity_path = repository / "tests/macos/macos-command-parity.json"

    host = read(host_path)
    request = read(request_path)
    result = read(result_path)
    owner = read(owner_path)
    tests = read(tests_path)
    main = read(main_path)
    project = read(project_path)

    for source, text in {
        "CoreHost.swift": host,
        "CoreRequest.swift": request,
        "CoreResult.swift": result,
    }.items():
        reject(text, "OpaquePointer", source)
        reject(text, "InkpodCoreC", source)
        reject(text, "@MainActor", source)
        reject(text, "deinit", source)

    require(owner, "private final class CoreOwnerLoop", "CoreOwnerThread.swift")
    require(owner, "let loop = CoreOwnerLoop(", "CoreOwnerThread.swift")
    require(owner, "static let normalCapacity = 4_096", "CoreOwnerThread.swift")
    require(owner, "static let inputSampleCapacity = 4_096", "CoreOwnerThread.swift")
    require(owner, "static let inputBoundaryReserve = 64", "CoreOwnerThread.swift")
    require(owner, "static let controlCapacity = 64", "CoreOwnerThread.swift")
    require(owner, "inkpod_core_create", "CoreOwnerThread.swift")
    require(owner, "inkpod_core_stroke_cancel", "CoreOwnerThread.swift")
    require(owner, "inkpod_core_destroy", "CoreOwnerThread.swift")
    reject(owner, "@MainActor", "CoreOwnerThread.swift")
    reject(owner, "deinit", "CoreOwnerThread.swift")

    owner_thread_start = owner.index("final class CoreOwnerThread")
    entry_start = owner.index("private struct CoreSessionEntry")
    reject(
        owner[owner_thread_start:entry_start],
        "OpaquePointer",
        "CoreOwnerThread stored state",
    )

    require(result, "CheckedContinuation<CoreRequestOutcome, Never>", "CoreResult.swift")
    require(result, "guard self.outcome == nil", "CoreResult.swift")
    require(host, "submit(.shutdown, lane: .control", "CoreHost.swift")
    require(host, "submit(.cancelStroke(target), lane: .inputBoundary)", "CoreHost.swift")
    require(tests, "0..<4_096", "CoreHostIntegrationTests.swift")
    require(tests, "testAsyncContinuationResumesExactlyOnce", "CoreHostIntegrationTests.swift")
    require(tests, "beginTransientForTesting", "CoreHostIntegrationTests.swift")
    require(main, "for index in 1...64", "CoreHostIntegrationMain.swift")
    require(project, "InkpodCoreHostIntegration", "project.pbxproj")

    try:
        parity = json.loads(read(parity_path))
    except json.JSONDecodeError as error:
        fail(f"invalid parity manifest: {error}")
    milestone_one = [
        row["windowsId"] for row in parity["commands"] if row["milestone"] == 1
    ]
    if milestone_one:
        fail(
            "M1 is headless and must not claim command parity rows: "
            + ", ".join(milestone_one)
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        verify(arguments.repository.resolve())
    except ValueError as error:
        print(f"macOS CoreHost contract verification failed: {error}", file=sys.stderr)
        return 1
    print(
        "macOS CoreHost contract: value-only facade, fixed owner thread, "
        "4096 normal + 4096/64 input + 64 control mailbox, zero M1 command rows"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
