#!/usr/bin/env python3
"""Verify implemented macOS command English/Japanese String Catalog coverage."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


COMMAND_CASE = re.compile(r'^\s*case \.([A-Za-z0-9]+): "([^"]+)"\s*$', re.MULTILINE)
LITERAL_LOOKUP = re.compile(r'\.text\("([^"\\]+)"\)')


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True, type=Path)
    args = parser.parse_args()
    root = args.repository.resolve()
    command_source = (root / "apps/macos/Application/CommandModel.swift").read_text()
    ui_sources = "\n".join(
        path.read_text()
        for path in (
            root / "apps/macos/Application/ApplicationInfrastructure.swift",
            root / "apps/macos/UI/CommandViews.swift",
            root / "apps/macos/UI/WorkspaceScene.swift",
            root / "apps/macos/UI/WorkspaceM5Views.swift",
            root / "apps/macos/UI/WorkspaceM6Views.swift",
            root / "apps/macos/UI/WorkspaceM9Views.swift",
            root / "apps/macos/UI/BatchWindow.swift",
            root / "apps/macos/Workspace/WorkspaceModel.swift",
        )
    )
    catalog = json.loads(
        (root / "apps/macos/Resources/Localizable.xcstrings").read_text()
    )
    strings = catalog.get("strings", {})

    command_keys = {f"command.{semantic}" for _, semantic in COMMAND_CASE.findall(command_source)}
    if len(command_keys) != 372:
        raise SystemExit(f"expected 372 semantic command localization keys, found {len(command_keys)}")
    required = command_keys | set(LITERAL_LOOKUP.findall(ui_sources)) | {
        "shortcut.result.invalid",
        "shortcut.result.prefixConflict",
        "shortcut.result.protectedStandard",
        "shortcut.result.persistenceFailure",
    }
    required |= {f"m5.layerKind.{value}" for value in range(1, 11)}
    required |= {f"m5.planeKind.{value}" for value in range(1, 8)}
    required |= {f"m5.pixelFormat.{value}" for value in range(1, 6)}
    missing = sorted(required - strings.keys())
    if missing:
        raise SystemExit(f"String Catalog is missing keys: {missing}")

    for key in sorted(required):
        localizations = strings[key].get("localizations", {})
        for language in ("en", "ja"):
            unit = localizations.get(language, {}).get("stringUnit", {})
            if unit.get("state") != "translated" or not str(unit.get("value", "")).strip():
                raise SystemExit(f"{key}: missing translated {language} value")

    print(f"macOS localization: {len(required)} required keys have complete en/ja values")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
