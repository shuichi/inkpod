#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import plistlib
import subprocess
import tempfile
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
RELEASE_CLI = REPOSITORY_ROOT / "scripts" / "macOS.sh"

FAKE_TOOL = r"""#!/usr/bin/env python3
import json
import os
import plistlib
import shutil
import sys
from pathlib import Path

tool = Path(sys.argv[0]).name
with Path(os.environ["INKPOD_TEST_TOOL_LOG"]).open("a", encoding="utf-8") as log:
    log.write(tool + " " + " ".join(sys.argv[1:]) + "\n")

remote_state = Path(os.environ["INKPOD_TEST_REMOTE_STATE"])
remote_state.mkdir(parents=True, exist_ok=True)
head_commit = os.environ.get("INKPOD_TEST_HEAD_COMMIT", "1" * 40)
publish_races = set(filter(None, os.environ.get("INKPOD_TEST_PUBLISH_RACES", "").split(",")))
publish_failures = set(filter(None, os.environ.get("INKPOD_TEST_PUBLISH_FAILURES", "").split(",")))


def git_arguments():
    arguments = sys.argv[1:]
    if arguments[:1] == ["-C"]:
        arguments = arguments[2:]
    return arguments


def remote_release_json():
    assets = []
    remote_asset = remote_state / "remote-asset"
    if remote_asset.is_file():
        assets.append(
            {
                "name": os.environ["INKPOD_TEST_ASSET_NAME"],
                "size": remote_asset.stat().st_size,
            }
        )
    return json.dumps(
        {
            "assets": assets,
            "isDraft": False,
            "isPrerelease": os.environ.get(
                "INKPOD_TEST_RELEASE_PRERELEASE", "1"
            )
            == "1",
            "tagName": "v" + os.environ["INKPOD_VERSION"],
            "url": "https://github.com/owner/inkpod/releases/tag/v"
            + os.environ["INKPOD_VERSION"],
        }
    )


if tool == "git":
    arguments = git_arguments()
    local_tag = remote_state / "local-tag"
    remote_tag = remote_state / "remote-tag"
    remote_tag_object = remote_state / "remote-tag-object"
    if arguments == ["status", "--porcelain"]:
        print(os.environ.get("INKPOD_TEST_GIT_STATUS", ""))
    elif arguments == ["branch", "--show-current"]:
        print(os.environ.get("INKPOD_TEST_BRANCH", "main"))
    elif arguments[:1] == ["fetch"]:
        pass
    elif arguments[:1] == ["check-ref-format"]:
        pass
    elif arguments == ["rev-parse", "HEAD"]:
        print(head_commit)
    elif arguments == ["rev-parse", "origin/main"]:
        print(os.environ.get("INKPOD_TEST_REMOTE_BRANCH_COMMIT", head_commit))
    elif arguments[:1] == ["rev-parse"] and arguments[1].startswith("refs/tags/"):
        if not local_tag.is_file():
            sys.exit(1)
        print(local_tag.read_text(encoding="utf-8"))
    elif arguments[:2] == ["remote", "get-url"]:
        print("git@github.com:owner/inkpod.git")
    elif arguments[:1] == ["ls-remote"]:
        if remote_tag.is_file():
            commit = remote_tag.read_text(encoding="utf-8")
            object_id = (
                remote_tag_object.read_text(encoding="utf-8")
                if remote_tag_object.is_file()
                else commit
            )
            print(
                object_id
                + "\trefs/tags/v"
                + os.environ["INKPOD_VERSION"]
            )
            if object_id != commit:
                print(
                    commit
                    + "\trefs/tags/v"
                    + os.environ["INKPOD_VERSION"]
                    + "^{}"
                )
    elif arguments[:2] in (["tag", "-a"], ["tag", "-f"]):
        local_tag.write_text(head_commit, encoding="utf-8")
    elif arguments[:2] == ["tag", "-d"]:
        local_tag.unlink(missing_ok=True)
    elif arguments[:1] == ["update-ref"]:
        local_tag.write_text(arguments[2], encoding="utf-8")
    elif arguments[:1] == ["push"]:
        marker = remote_state / "tag-race-fired"
        if "tag" in publish_races and not marker.exists():
            marker.touch()
            remote_tag.write_text(
                os.environ.get("INKPOD_TEST_RACING_TAG_COMMIT", head_commit),
                encoding="utf-8",
            )
            remote_tag_object.write_text(
                os.environ.get("INKPOD_TEST_RACING_TAG_OBJECT", head_commit),
                encoding="utf-8",
            )
            sys.exit(1)
        force_lease = next(
            (
                argument
                for argument in arguments
                if argument.startswith("--force-with-lease=refs/tags/")
            ),
            None,
        )
        if force_lease is not None:
            expected = force_lease.rsplit(":", 1)[1]
            current_object = (
                remote_tag_object.read_text(encoding="utf-8")
                if remote_tag_object.is_file()
                else remote_tag.read_text(encoding="utf-8")
            )
            if not remote_tag.is_file() or current_object != expected:
                sys.exit(1)
        elif remote_tag.is_file() and remote_tag.read_text(encoding="utf-8") != head_commit:
            sys.exit(1)
        remote_tag.write_text(head_commit, encoding="utf-8")
        remote_tag_object.write_text(head_commit, encoding="utf-8")
    else:
        raise RuntimeError("unexpected fake git invocation: " + " ".join(arguments))
elif tool == "gh":
    arguments = sys.argv[1:]
    release = remote_state / "release"
    remote_asset = remote_state / "remote-asset"
    if arguments[:2] == ["auth", "status"]:
        pass
    elif arguments[:2] == ["api", "repos/owner/inkpod"]:
        pass
    elif arguments[:2] == [
        "api",
        "repos/owner/inkpod/releases/tags/v" + os.environ["INKPOD_VERSION"],
    ]:
        if not release.exists():
            sys.exit(1)
    elif arguments[:2] == ["release", "view"]:
        if not release.exists():
            sys.exit(1)
        if os.environ.get("INKPOD_TEST_RELEASE_VIEW_FAILURE") == "1":
            sys.exit(1)
        print(remote_release_json())
    elif arguments[:2] == ["release", "create"]:
        marker = remote_state / "release-race-fired"
        if "release" in publish_races and not marker.exists():
            marker.touch()
            release.touch()
            sys.exit(1)
        if release.exists():
            sys.exit(1)
        release.touch()
        shutil.copyfile(Path(arguments[3]), remote_asset)
    elif arguments[:2] == ["release", "upload"]:
        source = Path(arguments[3])
        if "upload" in publish_failures:
            sys.exit(1)
        marker = remote_state / "upload-race-fired"
        if "upload" in publish_races and not marker.exists():
            marker.touch()
            shutil.copyfile(source, remote_asset)
            sys.exit(1)
        if remote_asset.exists() and "--clobber" not in arguments:
            sys.exit(1)
        shutil.copyfile(source, remote_asset)
    elif arguments[:2] == ["release", "download"]:
        destination = Path(arguments[arguments.index("--dir") + 1])
        destination.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(
            remote_asset,
            destination / os.environ["INKPOD_TEST_ASSET_NAME"],
        )
    else:
        raise RuntimeError("unexpected fake gh invocation: " + " ".join(arguments))
elif tool == "cmake" and "--build" in sys.argv:
    app = Path(os.environ["INKPOD_SOURCE_APP"])
    executable = app / "Contents" / "MacOS" / "Inkpod"
    executable.parent.mkdir(parents=True, exist_ok=True)
    executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    executable.chmod(0o755)
    with (app / "Contents" / "Info.plist").open("wb") as output:
        plistlib.dump(
            {
                "CFBundleExecutable": "Inkpod",
                "CFBundleIdentifier": "com.inkpod.app",
                "CFBundleShortVersionString": "0.0.0",
                "CFBundleVersion": "0",
            },
            output,
        )
elif tool == "hdiutil":
    if sys.argv[1:2] == ["create"]:
        Path(sys.argv[-1]).write_bytes(b"mock dmg\n")
    elif sys.argv[1:2] == ["attach"]:
        mount_point = Path(sys.argv[sys.argv.index("-mountpoint") + 1])
        app = mount_point / "Inkpod.app"
        executable = app / "Contents" / "MacOS" / "Inkpod"
        executable.parent.mkdir(parents=True, exist_ok=True)
        executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        executable.chmod(0o755)
        with (app / "Contents" / "Info.plist").open("wb") as output:
            plistlib.dump(
                {
                    "CFBundleExecutable": "Inkpod",
                    "CFBundleIdentifier": "com.inkpod.app",
                    "CFBundleShortVersionString": os.environ.get(
                        "INKPOD_TEST_MOUNTED_VERSION", os.environ["INKPOD_VERSION"]
                    ),
                    "CFBundleVersion": os.environ["INKPOD_BUILD_NUMBER"],
                },
                output,
            )
elif tool == "codesign" and "-d" in sys.argv and "--entitlements" in sys.argv:
    entitlements = {
        "com.apple.security.app-sandbox": True,
        "com.apple.security.files.bookmarks.app-scope": True,
        "com.apple.security.files.user-selected.read-write": True,
    }
    if os.environ.get("INKPOD_TEST_EXTRA_ENTITLEMENT") == "1":
        entitlements["com.apple.security.network.client"] = True
    payload = plistlib.dumps(entitlements)
    if os.environ.get("INKPOD_TEST_MINIFIED_ENTITLEMENTS") == "1":
        payload = b"".join(line.strip() for line in payload.splitlines())
    sys.stdout.buffer.write(payload)
elif tool == "lipo":
    print(os.environ.get("INKPOD_TEST_MOUNTED_ARCHITECTURE", "arm64"))
elif tool == "xcrun" and sys.argv[1:3] == ["notarytool", "submit"]:
    print('{"id":"00000000-0000-0000-0000-000000000000","status":"Accepted"}')
elif tool == "xcrun" and sys.argv[1:3] == ["notarytool", "log"]:
    print('{"status":"Accepted","issues":null}')
"""


class MacOSReleaseCliTests(unittest.TestCase):
    def setUp(self) -> None:
        if os.uname().sysname != "Darwin":
            self.skipTest("the release CLI is macOS-only")

        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.root = Path(self.temporary_directory.name)
        self.tool_directory = self.root / "bin"
        self.tool_directory.mkdir()
        self.tool_log = self.root / "tools.log"
        self.source_app = self.root / "products" / "Inkpod.app"
        self.output_directory = self.root / "release"
        self.dmg = self.output_directory / "Inkpod-0.2.3-macOS-arm64.dmg"
        self.remote_state = self.root / "remote"
        self.remote_state.mkdir()

        fake_tool = self.tool_directory / "fake-tool"
        fake_tool.write_text(FAKE_TOOL, encoding="utf-8")
        fake_tool.chmod(0o755)
        for name in (
            "cargo",
            "cmake",
            "codesign",
            "ctest",
            "hdiutil",
            "gh",
            "git",
            "lipo",
            "spctl",
            "xcodebuild",
            "xcrun",
        ):
            (self.tool_directory / name).symlink_to(fake_tool)

        self.environment = os.environ.copy()
        self.environment.update(
            {
                "PATH": f"{self.tool_directory}:{self.environment['PATH']}",
                "INKPOD_BUILD_NUMBER": "42",
                "INKPOD_CODESIGN_IDENTITY": "Developer ID Application: Test",
                "INKPOD_DMG_PATH": str(self.dmg),
                "INKPOD_NOTARY_PROFILE": "test-notary-profile",
                "INKPOD_OUTPUT_DIR": str(self.output_directory),
                "INKPOD_SOURCE_APP": str(self.source_app),
                "INKPOD_TEST_TOOL_LOG": str(self.tool_log),
                "INKPOD_TEST_REMOTE_STATE": str(self.remote_state),
                "INKPOD_TEST_ASSET_NAME": self.dmg.name,
                "INKPOD_VERSION": "0.2.3",
            }
        )

    def run_cli(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(RELEASE_CLI), *arguments],
            cwd=REPOSITORY_ROOT,
            env=self.environment,
            check=False,
            capture_output=True,
            text=True,
        )

    def prepare_publish_candidate(
        self,
        *,
        release_exists: bool = True,
        remote_asset: bytes | None = None,
        remote_tag_commit: str | None = None,
        remote_tag_object: str | None = None,
    ) -> None:
        self.output_directory.mkdir(parents=True, exist_ok=True)
        self.dmg.write_bytes(b"signed and notarized dmg\n")
        head_commit = self.environment.get("INKPOD_TEST_HEAD_COMMIT", "1" * 40)
        (self.remote_state / "remote-tag").write_text(
            remote_tag_commit or head_commit,
            encoding="utf-8",
        )
        (self.remote_state / "remote-tag-object").write_text(
            remote_tag_object or remote_tag_commit or head_commit,
            encoding="utf-8",
        )
        if release_exists:
            (self.remote_state / "release").touch()
        if remote_asset is not None:
            (self.remote_state / "remote-asset").write_bytes(remote_asset)

    def test_release_runs_dependencies_in_order_and_versions_the_app(self) -> None:
        result = self.run_cli("release")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(self.dmg.is_file())
        packaged_plist = self.output_directory / "Inkpod.app" / "Contents" / "Info.plist"
        with packaged_plist.open("rb") as source:
            info = plistlib.load(source)
        self.assertEqual(info["CFBundleShortVersionString"], "0.2.3")
        self.assertEqual(info["CFBundleVersion"], "42")
        self.assertEqual(list(self.output_directory.glob(".inkpod-*")), [])

        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        configure_index = next(
            index for index, call in enumerate(calls) if call.startswith("cmake --preset ")
        )
        build_index = next(
            index for index, call in enumerate(calls) if call.startswith("cmake --build ")
        )
        app_sign_index = next(
            index
            for index, call in enumerate(calls)
            if call.startswith("codesign ") and call.endswith("Inkpod.app")
        )
        dmg_index = next(
            index for index, call in enumerate(calls) if call.startswith("hdiutil create ")
        )
        verify_index = next(
            index for index, call in enumerate(calls) if call.startswith("hdiutil verify ")
        )
        submit_index = next(
            index
            for index, call in enumerate(calls)
            if call.startswith("xcrun notarytool submit ")
        )
        log_index = next(
            index
            for index, call in enumerate(calls)
            if call.startswith("xcrun notarytool log ")
        )
        staple_index = next(
            index
            for index, call in enumerate(calls)
            if call.startswith("xcrun stapler staple ")
        )
        assess_index = next(
            index for index, call in enumerate(calls) if call.startswith("spctl --assess ")
        )
        self.assertLess(
            configure_index,
            build_index,
        )
        self.assertLess(build_index, app_sign_index)
        self.assertLess(app_sign_index, dmg_index)
        self.assertLess(dmg_index, verify_index)
        self.assertLess(verify_index, submit_index)
        self.assertLess(submit_index, log_index)
        self.assertLess(log_index, staple_index)
        self.assertLess(staple_index, assess_index)
        self.assertIn(
            "cmake --preset macos-arm64-release -DINKPOD_BUILD_NUMBER=42",
            calls,
        )
        self.assertIn(
            "cmake --build --preset macos-arm64-release --target inkpod_macos_archive",
            calls,
        )

    def test_verify_runs_every_automated_macos_release_profile(self) -> None:
        result = self.run_cli("verify")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        expected_calls = (
            "cargo fmt --all -- --check",
            "cargo clippy --workspace --all-targets --all-features -- -D warnings",
            "cargo test --workspace --all-features",
            "cargo bench --package inkpod-core --bench core_workflows -- --quick",
            "cargo doc --package inkpod-core --all-features --no-deps",
            "cmake --preset macos-arm64-debug -DINKPOD_BUILD_NUMBER=42",
            "cmake --build --preset macos-arm64-debug --target inkpod_macos_check",
            "ctest --preset macos-arm64-debug --output-on-failure",
            "cmake --build --preset macos-arm64-debug --target inkpod_macos_ui_test",
            "cmake --build --preset macos-arm64-debug --target inkpod_macos_metal_check",
            "cmake --build --preset macos-arm64-debug --target inkpod_macos_tsan",
            "cmake --preset macos-arm64-release -DINKPOD_BUILD_NUMBER=42",
            "cmake --build --preset macos-arm64-release --target inkpod_macos_archive",
        )
        for call in expected_calls:
            self.assertIn(call, calls)
        self.assertEqual(
            [calls.index(call) for call in expected_calls],
            sorted(calls.index(call) for call in expected_calls),
        )

    def test_release_cannot_skip_notarization(self) -> None:
        self.environment["INKPOD_SKIP_NOTARIZE"] = "1"

        result = self.run_cli("release")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot skip notarization", result.stderr)
        self.assertFalse(self.tool_log.exists())

    def test_repository_declares_arm64_only_macos_builds(self) -> None:
        with (REPOSITORY_ROOT / "CMakePresets.json").open(
            encoding="utf-8"
        ) as source:
            presets = json.load(source)

        configure_names = {
            preset["name"] for preset in presets["configurePresets"]
        }
        self.assertIn("macos-arm64-debug", configure_names)
        self.assertIn("macos-arm64-release", configure_names)
        self.assertNotIn("macos-universal-release", configure_names)

        build_sources = "\n".join(
            path.read_text(encoding="utf-8")
            for path in (
                REPOSITORY_ROOT / "CMakeLists.txt",
                REPOSITORY_ROOT / "apps" / "macos" / "Config" / "Base.xcconfig",
                REPOSITORY_ROOT / "cmake" / "macos" / "InkpodMacOS.cmake",
                REPOSITORY_ROOT / "cmake" / "macos" / "RunArm64Build.cmake",
                REPOSITORY_ROOT / "cmake" / "macos" / "RunXcodeTests.cmake",
                REPOSITORY_ROOT / "cmake" / "macos" / "RunXcodeUITests.cmake",
            )
        )
        self.assertNotIn("x86_64-apple-darwin", build_sources)
        self.assertIn("aarch64-apple-darwin", build_sources)
        self.assertIn("MACOSX_DEPLOYMENT_TARGET=26.0", build_sources)
        self.assertIn("ARCHS = arm64", build_sources)
        self.assertEqual(build_sources.count("ARCHS=arm64"), 4)
        self.assertIn('STREQUAL "arm64"', build_sources)
        self.assertIn('-archivePath', build_sources)
        self.assertIn('\n            archive\n', build_sources)

    def test_help_documents_notarize_subcommand(self) -> None:
        result = self.run_cli("--help")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("notarize", result.stdout)
        self.assertIn("publish", result.stdout)
        self.assertIn("identical existing asset is a no-op", result.stdout)
        self.assertIn("INKPOD_GITHUB_PRERELEASE", result.stdout)
        self.assertIn("Shuichi Kurabayashi / ETD7LJJGQZ", result.stdout)
        self.assertIn("developer-id-notary", result.stdout)

    def test_notarize_rejects_adhoc_identity_before_building(self) -> None:
        self.environment["INKPOD_CODESIGN_IDENTITY"] = "-"

        result = self.run_cli("notarize")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Developer ID Application", result.stderr)
        self.assertFalse(self.tool_log.exists())

    def test_publish_adds_dmg_to_an_existing_release_without_rebuilding(self) -> None:
        self.prepare_publish_candidate()

        result = self.run_cli("publish")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            self.dmg.read_bytes(),
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertTrue(any(call.startswith("gh release upload v0.2.3 ") for call in calls))
        self.assertFalse(any(call.startswith("gh release create ") for call in calls))
        self.assertFalse(any(call.startswith("cmake ") for call in calls))
        self.assertFalse(any(call.startswith("cargo ") for call in calls))

    def test_publish_is_noop_when_the_existing_asset_is_byte_identical(self) -> None:
        candidate = b"signed and notarized dmg\n"
        self.prepare_publish_candidate(remote_asset=candidate)

        result = self.run_cli("publish")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("already contains the identical", result.stdout)
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any(call.startswith("gh release upload ") for call in calls))

    def test_publish_rejects_a_different_existing_asset_without_clobbering(self) -> None:
        self.prepare_publish_candidate(remote_asset=b"different artifact\n")

        result = self.run_cli("publish")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("different bytes", result.stderr)
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            b"different artifact\n",
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any(call.startswith("gh release upload ") for call in calls))
        self.assertFalse(any("--clobber" in call for call in calls))

    def test_publish_force_retargets_tag_and_clobbers_prerelease_dmg(self) -> None:
        previous_commit = "2" * 40
        previous_object = "5" * 40
        previous_asset = b"previous development artifact\n"
        self.prepare_publish_candidate(
            remote_asset=previous_asset,
            remote_tag_commit=previous_commit,
            remote_tag_object=previous_object,
        )

        result = self.run_cli("publish", "--force")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("DESTRUCTIVE", result.stdout)
        self.assertEqual(
            (self.remote_state / "remote-tag").read_text(encoding="utf-8"),
            "1" * 40,
        )
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            self.dmg.read_bytes(),
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertTrue(
            any(" tag -f -a v0.2.3 " in call for call in calls),
            calls,
        )
        self.assertTrue(
            any(
                "--force-with-lease=refs/tags/v0.2.3:" + previous_object in call
                for call in calls
            ),
            calls,
        )
        upload = next(call for call in calls if call.startswith("gh release upload "))
        self.assertIn("--clobber", upload)

    def test_publish_force_refuses_to_mutate_a_stable_release(self) -> None:
        previous_commit = "2" * 40
        previous_asset = b"stable artifact\n"
        self.prepare_publish_candidate(
            remote_asset=previous_asset,
            remote_tag_commit=previous_commit,
        )
        self.environment["INKPOD_TEST_RELEASE_PRERELEASE"] = "0"

        result = self.run_cli("publish", "--force")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("limited to an existing prerelease", result.stderr)
        self.assertEqual(
            (self.remote_state / "remote-tag").read_text(encoding="utf-8"),
            previous_commit,
        )
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            previous_asset,
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any(" tag -f " in call for call in calls))
        self.assertFalse(any(call.startswith("gh release upload ") for call in calls))

    def test_publish_force_refuses_to_clobber_a_stable_release_asset(self) -> None:
        previous_asset = b"stable artifact\n"
        self.prepare_publish_candidate(remote_asset=previous_asset)
        self.environment["INKPOD_TEST_RELEASE_PRERELEASE"] = "0"

        result = self.run_cli("publish", "--force")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("limited to an existing prerelease", result.stderr)
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            previous_asset,
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any("--clobber" in call for call in calls))

    def test_publish_force_tag_race_restores_the_previous_local_tag(self) -> None:
        previous_remote_commit = "2" * 40
        previous_local_object = "3" * 40
        racing_commit = "4" * 40
        self.prepare_publish_candidate(remote_tag_commit=previous_remote_commit)
        (self.remote_state / "local-tag").write_text(
            previous_local_object,
            encoding="utf-8",
        )
        self.environment["INKPOD_TEST_PUBLISH_RACES"] = "tag"
        self.environment["INKPOD_TEST_RACING_TAG_COMMIT"] = racing_commit

        result = self.run_cli("publish", "--force")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("local tag was restored", result.stderr)
        self.assertEqual(
            (self.remote_state / "remote-tag").read_text(encoding="utf-8"),
            racing_commit,
        )
        self.assertEqual(
            (self.remote_state / "local-tag").read_text(encoding="utf-8"),
            previous_local_object,
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertTrue(any(" update-ref refs/tags/v0.2.3 " in call for call in calls))
        self.assertFalse(any(call.startswith("gh release upload ") for call in calls))

    def test_publish_force_does_not_assume_an_unreadable_release_is_missing(self) -> None:
        previous_commit = "2" * 40
        previous_asset = b"existing artifact\n"
        self.prepare_publish_candidate(
            remote_asset=previous_asset,
            remote_tag_commit=previous_commit,
        )
        self.environment["INKPOD_TEST_RELEASE_VIEW_FAILURE"] = "1"

        result = self.run_cli("publish", "--force")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exists but its state could not be read", result.stderr)
        self.assertEqual(
            (self.remote_state / "remote-tag").read_text(encoding="utf-8"),
            previous_commit,
        )
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            previous_asset,
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any(" tag -f " in call for call in calls))
        self.assertFalse(any("--clobber" in call for call in calls))

    def test_publish_force_reports_a_failed_asset_replacement(self) -> None:
        previous_asset = b"previous development artifact\n"
        self.prepare_publish_candidate(remote_asset=previous_asset)
        self.environment["INKPOD_TEST_PUBLISH_FAILURES"] = "upload"

        result = self.run_cli("publish", "--force")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "forced GitHub asset replacement failed and the remote asset is not the candidate",
            result.stderr,
        )
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            previous_asset,
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        upload = next(call for call in calls if call.startswith("gh release upload "))
        self.assertIn("--clobber", upload)

    def test_publish_without_force_still_rejects_a_tag_on_another_commit(self) -> None:
        previous_commit = "2" * 40
        self.prepare_publish_candidate(remote_tag_commit=previous_commit)

        result = self.run_cli("publish")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not HEAD", result.stderr)
        self.assertEqual(
            (self.remote_state / "remote-tag").read_text(encoding="utf-8"),
            previous_commit,
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any("--force-with-lease" in call for call in calls))
        self.assertFalse(any("--clobber" in call for call in calls))

    def test_force_is_accepted_only_by_publish(self) -> None:
        result = self.run_cli("release", "--force")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--force is accepted only by publish", result.stderr)
        self.assertFalse(self.tool_log.exists())

    def test_publish_creates_and_pushes_a_tag_then_creates_a_prerelease(self) -> None:
        self.prepare_publish_candidate(release_exists=False)
        (self.remote_state / "remote-tag").unlink()
        (self.remote_state / "remote-tag-object").unlink()

        result = self.run_cli("publish")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertTrue(any(call.startswith("git -C ") and " tag -a v0.2.3 " in call for call in calls))
        self.assertTrue(any(call.startswith("git -C ") and " push origin refs/tags/v0.2.3" in call for call in calls))
        create = next(call for call in calls if call.startswith("gh release create "))
        self.assertIn(str(self.dmg), create)
        self.assertIn("--verify-tag", create)
        self.assertIn("--prerelease", create)
        self.assertFalse(any(call.startswith("gh release upload ") for call in calls))

    def test_publish_converges_when_tag_release_and_upload_race(self) -> None:
        self.prepare_publish_candidate(release_exists=False)
        (self.remote_state / "remote-tag").unlink()
        (self.remote_state / "remote-tag-object").unlink()
        self.environment["INKPOD_TEST_PUBLISH_RACES"] = "tag,release,upload"

        result = self.run_cli("publish")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("concurrent publisher", result.stdout)
        self.assertEqual(
            (self.remote_state / "remote-asset").read_bytes(),
            self.dmg.read_bytes(),
        )

    def test_publish_rejects_a_racing_tag_that_targets_another_commit(self) -> None:
        self.prepare_publish_candidate(release_exists=False)
        (self.remote_state / "remote-tag").unlink()
        (self.remote_state / "remote-tag-object").unlink()
        self.environment["INKPOD_TEST_PUBLISH_RACES"] = "tag"
        self.environment["INKPOD_TEST_RACING_TAG_COMMIT"] = "2" * 40

        result = self.run_cli("publish")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("points to", result.stderr)
        self.assertFalse((self.remote_state / "release").exists())
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertTrue(any(call.startswith("git -C ") and " tag -d v0.2.3" in call for call in calls))

    def test_publish_rejects_a_dirty_worktree_before_contacting_github(self) -> None:
        self.prepare_publish_candidate()
        self.environment["INKPOD_TEST_GIT_STATUS"] = " M scripts/macOS.sh"

        result = self.run_cli("publish")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("working tree must be clean", result.stderr)
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any(call.startswith("gh ") for call in calls))

    def test_publish_rejects_a_renamed_dmg_with_the_wrong_app_version(self) -> None:
        self.prepare_publish_candidate()
        self.environment["INKPOD_TEST_MOUNTED_VERSION"] = "0.2.2"

        result = self.run_cli("publish")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("mounted app version 0.2.2 does not match 0.2.3", result.stderr)
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any(call.startswith("gh release create ") for call in calls))
        self.assertFalse(any(call.startswith("gh release upload ") for call in calls))

    def test_publish_accepts_minified_codesign_entitlements(self) -> None:
        self.prepare_publish_candidate(remote_asset=b"signed and notarized dmg\n")
        self.environment["INKPOD_TEST_MINIFIED_ENTITLEMENTS"] = "1"

        result = self.run_cli("publish")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("already contains the identical", result.stdout)

    def test_publish_rejects_an_unapproved_mounted_entitlement(self) -> None:
        self.prepare_publish_candidate()
        self.environment["INKPOD_TEST_EXTRA_ENTITLEMENT"] = "1"

        result = self.run_cli("publish")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "mounted app must contain exactly the three approved entitlements",
            result.stderr,
        )
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any(call.startswith("gh release create ") for call in calls))
        self.assertFalse(any(call.startswith("gh release upload ") for call in calls))

    def test_publish_upload_failure_leaves_the_release_without_deleting_assets(self) -> None:
        self.prepare_publish_candidate()
        self.environment["INKPOD_TEST_PUBLISH_FAILURES"] = "upload"

        result = self.run_cli("publish")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no identical remote asset exists", result.stderr)
        self.assertFalse((self.remote_state / "remote-asset").exists())
        calls = self.tool_log.read_text(encoding="utf-8").splitlines()
        self.assertFalse(any("--clobber" in call for call in calls))


if __name__ == "__main__":
    unittest.main()
