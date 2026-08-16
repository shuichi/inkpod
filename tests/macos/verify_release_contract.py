#!/usr/bin/env python3
"""Verify the macOS M12 hardening and distribution source contract."""

from __future__ import annotations

import argparse
import json
import plistlib
import sys
from pathlib import Path


EXPECTED_ENTITLEMENTS = {
    "com.apple.security.app-sandbox": True,
    "com.apple.security.files.bookmarks.app-scope": True,
    "com.apple.security.files.user-selected.read-write": True,
}


def fail(message: str) -> None:
    raise ValueError(message)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"cannot read {path}: {error}")


def require(source: str, token: str, label: str) -> None:
    if token not in source:
        fail(f"{label} must contain {token!r}")


def verify(repository: Path) -> None:
    entitlements_path = repository / "apps/macos/App/Inkpod.entitlements"
    info_path = repository / "apps/macos/App/Info.plist"
    release_config_path = repository / "apps/macos/Config/Release.xcconfig"
    base_config_path = repository / "apps/macos/Config/Base.xcconfig"
    generated_config_path = repository / "cmake/macos/InkpodGenerated.xcconfig.in"
    archive_path = repository / "cmake/macos/RunArm64Build.cmake"
    release_cli_path = repository / "scripts/macOS.sh"
    parity_path = repository / "tests/macos/macos-command-parity.json"
    ui_tests_path = repository / "apps/macos/Tests/UI/InkpodUITests.swift"
    release_cli_tests_path = repository / "tests/macos/test_macos_release_cli.py"
    checklist_path = repository / "docs/macos-release-checklist.md"
    ci_path = repository / ".github/workflows/ci.yml"
    signed_ci_path = repository / ".github/workflows/macos-release.yml"

    with entitlements_path.open("rb") as source:
        entitlements = plistlib.load(source)
    if entitlements != EXPECTED_ENTITLEMENTS:
        fail(
            "Inkpod.entitlements must contain only App Sandbox, app-scoped "
            "bookmarks, and user-selected read/write access"
        )

    with info_path.open("rb") as source:
        info = plistlib.load(source)
    if info.get("CFBundleShortVersionString") != "$(MARKETING_VERSION)":
        fail("Info.plist marketing version must come from the CMake-generated xcconfig")
    if info.get("CFBundleVersion") != "$(CURRENT_PROJECT_VERSION)":
        fail("Info.plist build number must come from the CMake-generated xcconfig")
    if info.get("LSMinimumSystemVersion") != "$(MACOSX_DEPLOYMENT_TARGET)":
        fail("Info.plist deployment target must use MACOSX_DEPLOYMENT_TARGET")

    base_config = read(base_config_path)
    release_config = read(release_config_path)
    generated_config = read(generated_config_path)
    for token in (
        "MACOSX_DEPLOYMENT_TARGET = 26.0",
        "ARCHS = arm64",
        "SWIFT_STRICT_CONCURRENCY = complete",
        "SWIFT_TREAT_WARNINGS_AS_ERRORS = YES",
    ):
        require(base_config, token, "Base.xcconfig")
    for token in (
        "SWIFT_OPTIMIZATION_LEVEL = -O",
        "ONLY_ACTIVE_ARCH = NO",
        "VALIDATE_PRODUCT = YES",
        "ENABLE_HARDENED_RUNTIME = YES",
    ):
        require(release_config, token, "Release.xcconfig")
    require(generated_config, "MARKETING_VERSION = @PROJECT_VERSION@", "generated xcconfig")
    require(
        generated_config,
        "CURRENT_PROJECT_VERSION = @INKPOD_BUILD_NUMBER@",
        "generated xcconfig",
    )

    archive = read(archive_path)
    for token in (
        "archive",
        "-archivePath",
        "Inkpod.xcarchive",
        "ARCHS=arm64",
        'if(NOT INKPOD_LIPO_ARCHS STREQUAL "arm64")',
    ):
        require(archive, token, "RunArm64Build.cmake")

    release_cli = read(release_cli_path)
    for token in (
        "verify_release_candidate",
        "cargo fmt --all -- --check",
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "cargo test --workspace --all-features",
        "cargo bench --package inkpod-core --bench core_workflows -- --quick",
        'RUSTDOCFLAGS="-D warnings" cargo doc',
        "inkpod_macos_check",
        "inkpod_macos_ui_test",
        "inkpod_macos_metal_check",
        "inkpod_macos_tsan",
        "release cannot skip notarization",
        "notarytool submit",
        "notarytool log",
        "stapler staple",
        "stapler validate",
        "spctl --assess",
        "publish_release_dmg",
        "publish --force",
        "the working tree must be clean before publishing",
        "already contains ${DMG_FILE:t} with different bytes",
        "publish --force is limited to an existing prerelease",
        "--force-with-lease=refs/tags/${RELEASE_TAG}:${REMOTE_TAG_OBJECT}",
        "Concurrent publisher fixed ${RELEASE_TAG} to the same commit",
        "Concurrent publisher uploaded the identical ${DMG_FILE:t}",
        'release create "${RELEASE_TAG}"',
        'gh release upload "${RELEASE_TAG}" "${DMG_FILE}"',
        'gh release download "${RELEASE_TAG}"',
    ):
        require(release_cli, token, "scripts/macOS.sh")
    if release_cli.count("--clobber") != 1:
        fail("scripts/macOS.sh must keep --clobber isolated to publish --force")

    with parity_path.open(encoding="utf-8") as source:
        parity = json.load(source)
    commands = parity.get("commands")
    if not isinstance(commands, list) or len(commands) != 384:
        fail("the M12 parity freeze requires exactly 384 command rows")
    pending = [
        row.get("windowsId", "<unknown>")
        for row in commands
        if row.get("implementation") != "implemented"
    ]
    if pending:
        fail(f"the M12 parity freeze has non-implemented rows: {pending}")

    test_sources = {
        "CoreHostIntegrationTests.swift": read(
            repository / "apps/macos/Tests/Integration/CoreHostIntegrationTests.swift"
        ),
        "CanvasCoreIntegrationTests.swift": read(
            repository / "apps/macos/Tests/Integration/CanvasCoreIntegrationTests.swift"
        ),
        "CoreFileClipboardIntegrationTests.swift": read(
            repository
            / "apps/macos/Tests/Integration/CoreFileClipboardIntegrationTests.swift"
        ),
        "ProductCanvasLifecycleTests.swift": read(
            repository
            / "apps/macos/Tests/Integration/ProductCanvasLifecycleTests.swift"
        ),
        "CommandInfrastructureTests.swift": read(
            repository / "apps/macos/Tests/Unit/CommandInfrastructureTests.swift"
        ),
    }
    required_evidence = {
        "CoreHostIntegrationTests.swift": (
            "testNormalSaturationPreservesControlReserveAndCloseCancelsActiveStroke",
            "testShutdownCancelsQueuedWorkRejectsLateInputAndIsIdempotent",
        ),
        "CanvasCoreIntegrationTests.swift": (
            "testSnapshotQueueReleasesRejectReplaceCloseAndShutdownExactlyOnce",
            "testBackingPixelNormalizationUsesHalfOpenBoundsAndInputFallbacks",
        ),
        "CoreFileClipboardIntegrationTests.swift": (
            "testNativeSaveAutosaveRecoveryRevertAndFailureAreAtomic",
            "testQueuedFileRequestCanBeCancelledWithoutMutation",
        ),
        "ProductCanvasLifecycleTests.swift": (
            "testM11TwoHundredChromeResizesReuseTilesAndReleaseSnapshotsExactlyOnce",
            "testProductSceneCoversInputViewAndMetalLifecycle",
        ),
        "CommandInfrastructureTests.swift": (
            "implemented M2 through M10 commands have one owner, state owner, and real surface",
            "standard shortcuts and IME marked text are not intercepted",
        ),
    }
    for label, tokens in required_evidence.items():
        for token in tokens:
            require(test_sources[label], token, label)

    ui_tests = read(ui_tests_path)
    require(ui_tests, "performAccessibilityAudit", "InkpodUITests.swift")
    require(ui_tests, "isKnownMacOSFrameworkAuditFalsePositive", "InkpodUITests.swift")
    if ui_tests.count("performAccessibilityAudit") != 1:
        fail("the launched-product suite must have one explicit M12 accessibility audit")

    release_cli_tests = read(release_cli_tests_path)
    for token in (
        "test_verify_runs_every_automated_macos_release_profile",
        "test_release_cannot_skip_notarization",
        "test_notarize_rejects_adhoc_identity_before_building",
        "test_publish_adds_dmg_to_an_existing_release_without_rebuilding",
        "test_publish_is_noop_when_the_existing_asset_is_byte_identical",
        "test_publish_rejects_a_different_existing_asset_without_clobbering",
        "test_publish_force_retargets_tag_and_clobbers_prerelease_dmg",
        "test_publish_force_refuses_to_mutate_a_stable_release",
        "test_publish_force_refuses_to_clobber_a_stable_release_asset",
        "test_publish_force_tag_race_restores_the_previous_local_tag",
        "test_publish_force_does_not_assume_an_unreadable_release_is_missing",
        "test_publish_force_reports_a_failed_asset_replacement",
        "test_publish_without_force_still_rejects_a_tag_on_another_commit",
        "test_force_is_accepted_only_by_publish",
        "test_publish_creates_and_pushes_a_tag_then_creates_a_prerelease",
        "test_publish_converges_when_tag_release_and_upload_race",
        "test_publish_rejects_a_racing_tag_that_targets_another_commit",
        "test_publish_rejects_a_dirty_worktree_before_contacting_github",
        "test_publish_rejects_a_renamed_dmg_with_the_wrong_app_version",
        "test_publish_accepts_minified_codesign_entitlements",
        "test_publish_rejects_an_unapproved_mounted_entitlement",
        "test_publish_upload_failure_leaves_the_release_without_deleting_assets",
    ):
        require(release_cli_tests, token, "test_macos_release_cli.py")

    checklist = read(checklist_path)
    for token in (
        "Pass | Fail | Blocked",
        "VoiceOver",
        "Japanese IME",
        "Reduce Transparency",
        "multiple display",
        "sleep/wake",
        "notary log",
        "clean Tahoe",
        "./scripts/macOS.sh publish",
        "./scripts/macOS.sh publish --force",
        "different bytes",
    ):
        require(checklist, token, "macOS release checklist")

    ci = read(ci_path)
    for token in (
        "macos-26",
        "./scripts/macOS.sh verify",
        "windows-11-vs2026-arm",
        "windows-arm-debug",
        "windows-arm-release",
    ):
        require(ci, token, "CI workflow")

    signed_ci = read(signed_ci_path)
    for token in (
        "workflow_dispatch",
        "macos-26",
        "MACOS_DEVELOPER_ID_P12_BASE64",
        "MACOS_NOTARY_KEY_P8_BASE64",
        "scripts/macOS.sh release",
        "upload-artifact",
    ):
        require(signed_ci, token, "signed macOS release workflow")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        verify(arguments.repository.resolve())
    except (OSError, ValueError, plistlib.InvalidFileException, json.JSONDecodeError) as error:
        print(f"macOS M12 release contract verification failed: {error}", file=sys.stderr)
        return 1
    print(
        "macOS M12 release contract: 384 frozen commands, bounded entitlements, "
        "fault/a11y/soak evidence, arm64 archive, non-bypassable notarization, "
        "and race-safe GitHub publication"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
