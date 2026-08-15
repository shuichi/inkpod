#!/usr/bin/env python3
"""Verify the source-derived Windows to macOS command parity ledger."""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path


ID_PATTERN = re.compile(r"\b(IDM_[A-Z0-9_]+)\b")
DEFINE_PATTERN = re.compile(r"^#define\s+(IDM_[A-Z0-9_]+)\s+([0-9]+)\s*$", re.MULTILINE)
REQUIREMENT_PATTERN = re.compile(r"^- `([A-Z0-9-]+)`: ", re.MULTILINE)

RANGE_MARKERS = {
    "IDM_TOOL_HISTORY_VISUALIZATION_FIRST",
    "IDM_TOOL_HISTORY_VISUALIZATION_LAST",
}
RESERVED_AGGREGATES = {"IDM_BATCH_OPERATION_ADD"}
DISPOSITIONS = {
    "macEquivalent",
    "macStandardIntegrated",
    "mergedIntoSemanticCommand",
    "notApplicable",
}
IMPLEMENTATION_STATES = {"planned", "implemented"}
REQUIRED_COMMAND_FIELDS = {
    "windowsId",
    "numericId",
    "semanticKey",
    "requirementId",
    "disposition",
    "reason",
    "macSurface",
    "routeOwner",
    "stateOwner",
    "targetScope",
    "milestone",
    "testId",
    "implementation",
}


def fail(message: str) -> None:
    raise ValueError(message)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"cannot read {path}: {error}")


def command_occurrences(path: Path) -> list[str]:
    return ID_PATTERN.findall(read_text(path))


def require_nonempty_string(entry: dict[str, object], field: str, command: str) -> str:
    value = entry[field]
    if not isinstance(value, str) or not value.strip():
        fail(f"{command}: {field} must be a non-empty string")
    return value


def verify(repository: Path, manifest_path: Path) -> None:
    resource_path = repository / "apps/windows/app/resource.h"
    ja_path = repository / "apps/windows/app/app_ui_ja.generated.rc"
    en_path = repository / "apps/windows/app/app_ui_en.generated.rc"
    spec_path = repository / "SPEC.md"

    definitions = {
        name: int(value)
        for name, value in DEFINE_PATTERN.findall(read_text(resource_path))
    }
    if len(definitions) != 387:
        fail(f"resource.h must define 387 raw IDM_* symbols, found {len(definitions)}")
    if not RANGE_MARKERS.issubset(definitions):
        fail("resource.h is missing a dynamic-history range marker")
    if not RESERVED_AGGREGATES.issubset(definitions):
        fail("resource.h is missing the reserved Batch aggregate")

    ja_occurrences = command_occurrences(ja_path)
    en_occurrences = command_occurrences(en_path)
    ja_commands = set(ja_occurrences)
    en_commands = set(en_occurrences)
    if ja_commands != en_commands:
        fail(
            "Japanese/English production command sets differ: "
            f"ja-only={sorted(ja_commands - en_commands)}, "
            f"en-only={sorted(en_commands - ja_commands)}"
        )
    if len(ja_commands) != 384:
        fail(f"production command set must contain 384 IDs, found {len(ja_commands)}")
    if len(ja_occurrences) != 391 or len(en_occurrences) != 391:
        fail(
            "each generated language resource must contain 391 command occurrences, "
            f"found ja={len(ja_occurrences)}, en={len(en_occurrences)}"
        )

    classified = ja_commands | RANGE_MARKERS | RESERVED_AGGREGATES
    raw_commands = set(definitions)
    if classified != raw_commands:
        fail(
            "raw command partition is incomplete: "
            f"unclassified={sorted(raw_commands - classified)}, "
            f"extra={sorted(classified - raw_commands)}"
        )
    if ja_commands & (RANGE_MARKERS | RESERVED_AGGREGATES):
        fail("range markers or the reserved aggregate leaked into production commands")

    try:
        manifest = json.loads(read_text(manifest_path))
    except json.JSONDecodeError as error:
        fail(f"invalid JSON in {manifest_path}: {error}")
    if manifest.get("schemaVersion") != 1:
        fail("command parity schemaVersion must be 1")
    commands = manifest.get("commands")
    if not isinstance(commands, list):
        fail("manifest commands must be an array")
    if len(commands) != 384:
        fail(f"manifest must contain 384 command rows, found {len(commands)}")

    known_requirements = set(REQUIREMENT_PATTERN.findall(read_text(spec_path)))
    manifest_ids: list[str] = []
    semantics: defaultdict[str, list[dict[str, object]]] = defaultdict(list)
    for index, raw_entry in enumerate(commands):
        if not isinstance(raw_entry, dict):
            fail(f"commands[{index}] must be an object")
        missing = REQUIRED_COMMAND_FIELDS - raw_entry.keys()
        if missing:
            fail(f"commands[{index}] is missing {sorted(missing)}")
        command = require_nonempty_string(raw_entry, "windowsId", f"commands[{index}]")
        manifest_ids.append(command)
        if command not in definitions:
            fail(f"{command}: not declared in resource.h")
        numeric_id = raw_entry["numericId"]
        if not isinstance(numeric_id, int) or isinstance(numeric_id, bool):
            fail(f"{command}: numericId must be an integer")
        if numeric_id != definitions[command]:
            fail(
                f"{command}: numericId {numeric_id} does not match resource.h "
                f"value {definitions[command]}"
            )
        semantic_key = require_nonempty_string(raw_entry, "semanticKey", command)
        semantics[semantic_key].append(raw_entry)
        requirement = require_nonempty_string(raw_entry, "requirementId", command)
        if requirement not in known_requirements:
            fail(f"{command}: unknown SPEC.md requirement {requirement}")
        disposition = require_nonempty_string(raw_entry, "disposition", command)
        if disposition not in DISPOSITIONS:
            fail(f"{command}: invalid disposition {disposition}")
        require_nonempty_string(raw_entry, "reason", command)
        require_nonempty_string(raw_entry, "macSurface", command)
        require_nonempty_string(raw_entry, "routeOwner", command)
        require_nonempty_string(raw_entry, "stateOwner", command)
        require_nonempty_string(raw_entry, "targetScope", command)
        test_id = require_nonempty_string(raw_entry, "testId", command)
        milestone = raw_entry["milestone"]
        if not isinstance(milestone, int) or isinstance(milestone, bool) or not 1 <= milestone <= 11:
            fail(f"{command}: milestone must be an integer from 1 through 11")
        implementation = require_nonempty_string(raw_entry, "implementation", command)
        if implementation not in IMPLEMENTATION_STATES:
            fail(f"{command}: invalid implementation state {implementation}")
        if disposition == "notApplicable":
            if not isinstance(raw_entry.get("alternativeSurface"), bool):
                fail(f"{command}: notApplicable requires boolean alternativeSurface")
            if "SPEC" not in str(raw_entry["reason"]):
                fail(f"{command}: notApplicable reason must identify its SPEC basis")
        if implementation == "implemented":
            for field in ("routeOwner", "stateOwner", "macSurface", "testId"):
                value = str(raw_entry[field]).lower()
                if "planned" in value or "pending" in value or value == "mac-parity-ledger-001":
                    fail(f"{command}: implemented row has placeholder {field}")
            if test_id == "MAC-PARITY-LEDGER-001":
                fail(f"{command}: implemented row needs a feature test, not the ledger test")

    duplicate_ids = sorted(name for name, count in Counter(manifest_ids).items() if count != 1)
    if duplicate_ids:
        fail(f"manifest has duplicate Windows IDs: {duplicate_ids}")
    manifest_set = set(manifest_ids)
    if manifest_set != ja_commands:
        fail(
            "manifest and production command sets differ: "
            f"missing={sorted(ja_commands - manifest_set)}, "
            f"extra={sorted(manifest_set - ja_commands)}"
        )

    expected_implemented = {
        str(entry["windowsId"])
        for entry in commands
        if entry["milestone"] in {2, 3, 4, 5, 6, 7, 8, 9, 10}
    }
    actual_implemented = {
        str(entry["windowsId"])
        for entry in commands
        if entry["implementation"] == "implemented"
    }
    if actual_implemented != expected_implemented:
        fail(
            "M10 must implement exactly the M2 through M10 rows: "
            f"missing={sorted(expected_implemented - actual_implemented)}, "
            f"later={sorted(actual_implemented - expected_implemented)}"
        )

    for semantic_key, entries in semantics.items():
        if len(entries) <= 1:
            continue
        dispositions = {entry["disposition"] for entry in entries}
        if dispositions != {"mergedIntoSemanticCommand"}:
            fail(
                f"semantic key {semantic_key!r} is many-to-one but dispositions are "
                f"{sorted(dispositions)}"
            )

    source = manifest.get("source")
    if source != {
        "rawSymbolCount": 387,
        "productionCommandCount": 384,
        "reservedAggregateCount": 1,
        "rangeMarkerCount": 2,
        "occurrencesPerLanguage": 391,
    }:
        fail("manifest source summary does not match the enforced 387/384/1/2/391 partition")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        verify(arguments.repository.resolve(), arguments.manifest.resolve())
    except ValueError as error:
        print(f"macOS command parity verification failed: {error}", file=sys.stderr)
        return 1
    print("macOS command parity: 384 production commands; raw 387 = 384 + 1 + 2")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
