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
import os
import plistlib
import sys
from pathlib import Path

tool = Path(sys.argv[0]).name
with Path(os.environ["INKPOD_TEST_TOOL_LOG"]).open("a", encoding="utf-8") as log:
    log.write(tool + " " + " ".join(sys.argv[1:]) + "\n")

if tool == "cmake" and "--build" in sys.argv:
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
    Path(sys.argv[-1]).write_bytes(b"mock dmg\n")
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
        self.dmg = self.output_directory / "Inkpod-test.dmg"

        fake_tool = self.tool_directory / "fake-tool"
        fake_tool.write_text(FAKE_TOOL, encoding="utf-8")
        fake_tool.chmod(0o755)
        for name in (
            "cargo",
            "cmake",
            "codesign",
            "hdiutil",
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
                "INKPOD_VERSION": "1.2.3",
            }
        )

    def run_cli(self, command: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(RELEASE_CLI), command],
            cwd=REPOSITORY_ROOT,
            env=self.environment,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_release_runs_dependencies_in_order_and_versions_the_app(self) -> None:
        result = self.run_cli("release")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(self.dmg.is_file())
        packaged_plist = self.output_directory / "Inkpod.app" / "Contents" / "Info.plist"
        with packaged_plist.open("rb") as source:
            info = plistlib.load(source)
        self.assertEqual(info["CFBundleShortVersionString"], "1.2.3")
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
        self.assertEqual(build_sources.count("ARCHS=arm64"), 3)
        self.assertIn('STREQUAL "arm64"', build_sources)

    def test_help_documents_notarize_subcommand(self) -> None:
        result = self.run_cli("--help")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("notarize", result.stdout)
        self.assertIn("Shuichi Kurabayashi / ETD7LJJGQZ", result.stdout)
        self.assertIn("developer-id-notary", result.stdout)

    def test_notarize_rejects_adhoc_identity_before_building(self) -> None:
        self.environment["INKPOD_CODESIGN_IDENTITY"] = "-"

        result = self.run_cli("notarize")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Developer ID Application", result.stderr)
        self.assertFalse(self.tool_log.exists())


if __name__ == "__main__":
    unittest.main()
