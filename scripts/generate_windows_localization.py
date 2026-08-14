#!/usr/bin/env python3
"""Validate and generate the Win32 typed localization artifacts.

The JSON catalog is the only hand-edited source of Japanese and English UI
text.  Generated C++ and RC files are checked in so the normal MSVC/CMake build
does not acquire a Python dependency; CI runs this script with ``--check`` to
reject drift.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sys
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
WINDOWS = ROOT / "apps" / "windows"
UI = WINDOWS / "ui"
APP = WINDOWS / "app"
CATALOG_PATH = UI / "localization_catalog.json"
IDS_PATH = UI / "localization_catalog_ids.generated.inc"
TABLE_PATH = UI / "localization_catalog.generated.inc"
COMMON_RC_PATH = APP / "app_common.rc"
TEMPLATE_RC_PATH = APP / "app_ui.template.rc"
JA_RC_PATH = APP / "app_ui_ja.generated.rc"
EN_RC_PATH = APP / "app_ui_en.generated.rc"
AGGREGATE_RC_PATH = APP / "app.rc"

JAPANESE_RE = re.compile(
    r"[\u3040-\u30ff\u31f0-\u31ff\u3400-\u4dbf\u4e00-\u9fff"
    r"\uf900-\ufaff\uff66-\uff9f]"
)
MARKER_RE = re.compile(r"@INKPOD_UI_TEXT_([A-Za-z][A-Za-z0-9_]*)@")
SOURCE_STRING_RE = re.compile(r'"(?:\\.|[^"\\])*"')


def contains_japanese(value: str) -> bool:
    return JAPANESE_RE.search(value) is not None


def load_catalog() -> tuple[dict[str, Any], list[dict[str, Any]]]:
    document = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
    if document.get("schema") != 1:
        raise RuntimeError("localization catalog schema must be 1")
    if document.get("languages") != ["ja-JP", "en-US"]:
        raise RuntimeError("localization catalog languages must be ja-JP and en-US")
    entries = document.get("entries")
    if not isinstance(entries, list) or not entries:
        raise RuntimeError("localization catalog entries must be a non-empty array")
    return document, entries


def format_signature(value: str) -> tuple[str, ...]:
    signature: list[str] = []
    index = 0
    conversions = "diuoxXfFeEgGaAcspnCSZ"
    while index < len(value):
        if value[index] != "%":
            index += 1
            continue
        index += 1
        if index < len(value) and value[index] == "%":
            index += 1
            continue
        while index < len(value) and value[index] in "-+ #0'":
            index += 1
        if index < len(value) and value[index] == "*":
            signature.append("int:width")
            index += 1
        else:
            while index < len(value) and value[index].isdigit():
                index += 1
        if index < len(value) and value[index] == ".":
            index += 1
            if index < len(value) and value[index] == "*":
                signature.append("int:precision")
                index += 1
            else:
                while index < len(value) and value[index].isdigit():
                    index += 1
        length = ""
        if value.startswith("I64", index) or value.startswith("I32", index):
            length = value[index : index + 3]
            index += 3
        elif value.startswith("hh", index) or value.startswith("ll", index):
            length = value[index : index + 2]
            index += 2
        elif index < len(value) and value[index] in "hljztLw":
            length = value[index]
            index += 1
        if index < len(value) and value[index] in conversions:
            signature.append(f"{length}:{value[index]}")
            index += 1
    return tuple(signature)


def validate(entries: list[dict[str, Any]]) -> None:
    identifiers: set[str] = set()
    errors: list[str] = []
    for index, entry in enumerate(entries):
        identifier = entry.get("id")
        japanese = entry.get("ja")
        english = entry.get("en")
        kind = entry.get("kind")
        resource = entry.get("resource")
        label = identifier if isinstance(identifier, str) else f"entry[{index}]"
        if not isinstance(identifier, str) or re.fullmatch(
            r"[A-Za-z][A-Za-z0-9_]*", identifier
        ) is None:
            errors.append(f"{label}: invalid ID")
        elif identifier in identifiers:
            errors.append(f"{label}: duplicate ID")
        else:
            identifiers.add(identifier)
        if not isinstance(japanese, str) or not japanese:
            errors.append(f"{label}: Japanese text is empty")
        if not isinstance(english, str) or not english:
            errors.append(f"{label}: English text is empty")
        elif contains_japanese(english):
            errors.append(f"{label}: English text contains Japanese characters")
        if kind not in {"text", "format"}:
            errors.append(f"{label}: kind must be text or format")
        if not isinstance(resource, bool):
            errors.append(f"{label}: resource must be boolean")
        if isinstance(japanese, str) and isinstance(english, str):
            ja_signature = format_signature(japanese)
            en_signature = format_signature(english)
            if kind == "format" and ja_signature != en_signature:
                errors.append(
                    f"{label}: format signature differs: "
                    f"ja={ja_signature}, en={en_signature}"
                )
    if errors:
        raise RuntimeError("invalid localization catalog:\n" + "\n".join(errors))


def cpp_quote(value: str) -> str:
    escaped: list[str] = []
    for character in value:
        codepoint = ord(character)
        if character == "\\":
            escaped.append("\\\\")
        elif character == '"':
            escaped.append('\\"')
        elif character == "\n":
            escaped.append("\\n")
        elif character == "\r":
            escaped.append("\\r")
        elif character == "\t":
            escaped.append("\\t")
        elif character == "\0":
            escaped.append("\\0")
        elif codepoint < 0x20:
            escaped.append(f"\\u{codepoint:04x}")
        else:
            escaped.append(character)
    return 'L"' + "".join(escaped) + '"'


def utf16_length(value: str) -> int:
    return len(value.encode("utf-16-le")) // 2


def rc_quote(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def banner(catalog_hash: str, comment: str = "//") -> str:
    return (
        f"{comment} Generated from apps/windows/ui/localization_catalog.json.\n"
        f"{comment} Catalog SHA-256: {catalog_hash}\n"
        f"{comment} Do not edit this file directly.\n"
    )


def catalog_sha256() -> str:
    # Git checkout settings may materialize the catalog with LF or CRLF.
    # Hash its canonical UTF-8/LF representation so generated artifacts do
    # not become stale solely because they were produced on another machine.
    canonical = CATALOG_PATH.read_text(encoding="utf-8").encode("utf-8")
    return hashlib.sha256(canonical).hexdigest()


def generated_artifacts(entries: list[dict[str, Any]]) -> dict[pathlib.Path, str]:
    catalog_hash = catalog_sha256()
    ids = banner(catalog_hash) + "".join(
        f"INKPOD_UI_STRING_ID({entry['id']})\n" for entry in entries
    )
    table = banner(catalog_hash) + "".join(
        "{"
        + cpp_quote(entry["ja"])
        + f", {utf16_length(entry['ja'])}U, "
        + cpp_quote(entry["en"])
        + f", {utf16_length(entry['en'])}U"
        + "},\n"
        for entry in entries
    )
    template = TEMPLATE_RC_PATH.read_text(encoding="utf-8")
    by_id = {entry["id"]: entry for entry in entries}
    markers = set(MARKER_RE.findall(template))
    missing = sorted(markers - set(by_id))
    if missing:
        raise RuntimeError(f"resource template uses unknown catalog IDs: {missing}")
    expected_resource_ids = {entry["id"] for entry in entries if entry["resource"]}
    unused = sorted(expected_resource_ids - markers)
    if unused:
        raise RuntimeError(f"resource catalog rows are unused by template: {unused}")

    def generate_resource(language: str, langid: str) -> str:
        def replace(match: re.Match[str]) -> str:
            return rc_quote(by_id[match.group(1)][language])

        body = MARKER_RE.sub(replace, template)
        return (
            banner(catalog_hash)
            + '#include <windows.h>\n#include <commctrl.h>\n#include "resource.h"\n\n'
            + "#pragma code_page(65001)\n"
            + f"LANGUAGE {langid}\n\n"
            + body
        )

    return {
        IDS_PATH: ids,
        TABLE_PATH: table,
        JA_RC_PATH: generate_resource(
            "ja", "LANG_JAPANESE, SUBLANG_JAPANESE_JAPAN"
        ),
        EN_RC_PATH: generate_resource(
            "en", "LANG_ENGLISH, SUBLANG_ENGLISH_US"
        ),
    }


def bootstrap_resource_template(entries: list[dict[str, Any]]) -> None:
    original = AGGREGATE_RC_PATH.read_text(encoding="utf-8")
    split = original.find("STRINGTABLE")
    if split < 0:
        raise RuntimeError("app.rc does not contain STRINGTABLE")
    common = original[:split].rstrip() + "\n"
    ui_source = original[split:]
    by_japanese: dict[str, dict[str, Any]] = {}
    for entry in entries:
        entry["resource"] = False
        by_japanese.setdefault(entry["ja"], entry)
    used: set[str] = set()

    def replace(match: re.Match[str]) -> str:
        value = json.loads(match.group(0))
        if not contains_japanese(value):
            return match.group(0)
        entry = by_japanese.get(value)
        if entry is None:
            raise RuntimeError(f"resource string is absent from catalog: {value!r}")
        entry["resource"] = True
        used.add(entry["id"])
        return f"@INKPOD_UI_TEXT_{entry['id']}@"

    template = SOURCE_STRING_RE.sub(replace, ui_source)
    residual = JAPANESE_RE.search(template)
    if residual is not None:
        raise RuntimeError("Japanese text remained outside a resource string")
    COMMON_RC_PATH.write_text(common, encoding="utf-8")
    TEMPLATE_RC_PATH.write_text(template, encoding="utf-8")
    AGGREGATE_RC_PATH.write_text(
        '#include "app_common.rc"\n'
        '#include "app_ui_ja.generated.rc"\n'
        '#include "app_ui_en.generated.rc"\n',
        encoding="utf-8",
    )
    document, document_entries = load_catalog()
    resources = {entry["id"] for entry in entries if entry["resource"]}
    for entry in document_entries:
        entry["resource"] = entry["id"] in resources
    CATALOG_PATH.write_text(
        json.dumps(document, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"bootstrapped {len(used)} localized resource strings")


def write_or_check(artifacts: dict[pathlib.Path, str], check: bool) -> None:
    drift: list[str] = []
    for path, expected in artifacts.items():
        if check:
            actual = path.read_text(encoding="utf-8") if path.exists() else None
            if actual != expected:
                drift.append(path.relative_to(ROOT).as_posix())
        else:
            path.write_text(expected, encoding="utf-8")
            print(f"generated {path.relative_to(ROOT)}")
    if drift:
        raise RuntimeError(
            "generated localization artifacts are stale:\n"
            + "\n".join(drift)
            + "\nrun scripts/generate_windows_localization.py --write"
        )


def main() -> None:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write", action="store_true")
    group.add_argument("--check", action="store_true")
    group.add_argument("--bootstrap-resource-template", action="store_true")
    arguments = parser.parse_args()
    _, entries = load_catalog()
    validate(entries)
    if arguments.bootstrap_resource_template:
        bootstrap_resource_template(entries)
        return
    write_or_check(generated_artifacts(entries), arguments.check)


if __name__ == "__main__":
    main()
