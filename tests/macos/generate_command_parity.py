#!/usr/bin/env python3
"""Generate the checked-in macOS command parity ledger from Windows resources.

The generated JSON is the reviewable per-command ledger. This script keeps the
mechanical ID/value/source extraction deterministic while the mapping functions
below encode the initial planning decisions from MACOS.md.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


DEFINE_PATTERN = re.compile(r"^#define\s+(IDM_[A-Z0-9_]+)\s+([0-9]+)\s*$", re.MULTILINE)
ID_PATTERN = re.compile(r"\b(IDM_[A-Z0-9_]+)\b")
EXCLUDED = {
    "IDM_TOOL_HISTORY_VISUALIZATION_FIRST",
    "IDM_TOOL_HISTORY_VISUALIZATION_LAST",
    "IDM_BATCH_OPERATION_ADD",
}
IMPLEMENTED_MILESTONES = {2, 3, 4, 5, 6, 7, 8, 9, 10}
STANDARD_INTEGRATED = {
    "IDM_APP_EXIT",
    "IDM_EDIT_COPY",
    "IDM_EDIT_CUT",
    "IDM_EDIT_PASTE",
    "IDM_EDIT_REDO",
    "IDM_EDIT_UNDO",
    "IDM_FILE_OPEN",
    "IDM_FILE_SAVE",
    "IDM_FILE_SAVE_AS",
    "IDM_HELP_ABOUT",
    "IDM_VIEW_CLOSE",
}
NOT_APPLICABLE = {
    "IDM_WORKSPACE_AUTOHIDE_BATCH",
    "IDM_WORKSPACE_AUTOHIDE_LIGHT_TABLE",
    "IDM_WORKSPACE_AUTOHIDE_LOCATOR",
    "IDM_WORKSPACE_AUTOHIDE_REFERENCE",
    "IDM_WORKSPACE_AUTOHIDE_SEQUENCE",
}


def symbol_tail(symbol: str) -> str:
    return symbol.removeprefix("IDM_")


def semantic_key(symbol: str) -> str:
    if symbol.startswith("IDM_FILE_RECENT_"):
        return "file.openRecent"
    words = symbol_tail(symbol).lower().split("_")
    return ".".join(words)


def requirement(symbol: str) -> str:
    if symbol == "IDM_FILE_NEW":
        return "CELL-001"
    if symbol == "IDM_FILE_NEW_CUT" or symbol.startswith("IDM_CUT_"):
        if "SEQUENCE" in symbol:
            return "SEQ-STRUCT-001"
        return "CUT-001"
    if symbol in {"IDM_FILE_IMPORT_RASTER", "IDM_FILE_EXPORT_RASTER"}:
        return "IO-002"
    if symbol == "IDM_FILE_EXPORT_INSTRUCTION_RASTER":
        return "ANNOTATION-001"
    if symbol.startswith("IDM_FILE_"):
        return "IO-001"
    if symbol == "IDM_APP_EXIT" or symbol == "IDM_DOCUMENT_CLOSE":
        return "SESSION-001"
    if symbol in {
        "IDM_EDIT_UNDO",
        "IDM_EDIT_REDO",
        "IDM_EDIT_HISTORY_BACK",
        "IDM_EDIT_HISTORY_FORWARD",
        "IDM_TOOL_HISTORY_VISUALIZATION_FIRST",
        "IDM_TOOL_HISTORY_VISUALIZATION_LAST",
    }:
        return "HIST-001"
    if symbol.startswith("IDM_EDIT_FLOATING_") or symbol == "IDM_EDIT_MIRROR_HORIZONTAL":
        return "XFORM-003"
    if symbol.startswith("IDM_EDIT_"):
        return "CLIP-001"
    if symbol.startswith("IDM_VIEW_VECTOR_"):
        return "VIEW-005"
    if any(token in symbol for token in ("GUIDE", "GRID", "SNAP")) and symbol.startswith("IDM_VIEW_"):
        return "SNAP-001"
    if symbol.startswith("IDM_VIEW_"):
        if any(token in symbol for token in ("NEW", "CLOSE", "MOVE", "DUPLICATE")):
            return "VIEW-004"
        return "VIEW-001"
    if symbol.startswith("IDM_TAB_") or symbol.startswith("IDM_EDITOR_"):
        return "WORKSPACE-001"
    if symbol.startswith("IDM_TOOL_COLOR_REPLACE_"):
        return "COLOR-REPLACE-001"
    if symbol.startswith("IDM_TOOL_FILL") or symbol == "IDM_TOOL_CLOSED_FILL":
        return "FILL-001"
    if symbol.startswith("IDM_TOOL_"):
        return "PAINT-001"
    if symbol.startswith("IDM_PLANE_") or symbol.startswith("IDM_LAYER_"):
        return "DOC-003"
    if symbol.startswith("IDM_COLOR_"):
        if symbol == "IDM_COLOR_PIN":
            return "WORKSPACE-002"
        return "COLOR-001"
    if symbol.startswith("IDM_PALETTE_") or symbol.startswith("IDM_CHART_"):
        return "COLOR-002"
    if symbol.startswith("IDM_HELP_") or symbol.startswith("IDM_LANGUAGE_"):
        return "WIN-001"
    if symbol.startswith("IDM_SHORTCUT_"):
        return "SHORT-001"
    if symbol.startswith("IDM_SELECTION_OUTPUT_COLOR_GUARD"):
        return "COLOR-OUTPUT-QA-001"
    if symbol.startswith("IDM_SELECTION_"):
        return "SEL-001"
    if symbol.startswith("IDM_FILTER_"):
        return "FILTER-001"
    if symbol.startswith("IDM_EFFECT_"):
        return "EFFECT-001"
    if symbol.startswith("IDM_ADJUSTMENT_"):
        return "ADJUST-001"
    if symbol.startswith("IDM_CELL_SHOOTING_FRAME_"):
        return "SHOOTING-FRAME-001"
    if symbol.startswith("IDM_CELL_VANISHING_POINT_"):
        return "VANISHING-POINT-001"
    if symbol.startswith("IDM_CELL_"):
        return "CELL-001"
    if symbol.startswith("IDM_ANNOTATION_"):
        return "ANNOTATION-001"
    if symbol.startswith("IDM_LT_"):
        if "BULK" in symbol:
            return "LT-003"
        return "LT-001"
    if symbol == "IDM_SEQ_WRAP_ENDPOINTS":
        return "SEQ-ENDPOINT-001"
    if symbol.startswith("IDM_SEQ_"):
        return "SEQ-001"
    if symbol.startswith("IDM_SUBPALETTE_"):
        return "COLOR-002"
    if symbol.startswith("IDM_MOTION_"):
        return "SEQ-002"
    if symbol.startswith("IDM_VECTOR_"):
        return "VECTOR-001"
    if symbol.startswith("IDM_GEOMETRY_"):
        return "PAINT-002"
    if symbol.startswith("IDM_LOCATOR_"):
        return "VIEW-003"
    if symbol in {"IDM_SEQUENCE_PIN", "IDM_LIGHT_TABLE_PIN", "IDM_SUBPALETTE_PIN", "IDM_BATCH_PIN"}:
        return "WORKSPACE-002"
    if symbol.startswith("IDM_WINDOW_") or symbol.startswith("IDM_WORKSPACE_"):
        return "WORKSPACE-001"
    if symbol == "IDM_BATCH_EXTRACT_PAIRS":
        return "BATCH-004"
    if symbol.startswith("IDM_BATCH_"):
        return "BATCH-001"
    raise ValueError(f"no requirement mapping for {symbol}")


def milestone(symbol: str) -> int:
    if symbol in {
        "IDM_WINDOW_TOOL_PALETTE",
        "IDM_WINDOW_TOOL_OPTIONS",
        "IDM_WINDOW_COLOR_PANE",
        "IDM_WINDOW_LOCATOR",
        "IDM_COLOR_PIN",
        "IDM_WORKSPACE_AUTOHIDE_LOCATOR",
    }:
        return 6
    if symbol in {
        "IDM_SUBPALETTE_SET",
        "IDM_SUBPALETTE_SAMPLE",
        "IDM_WINDOW_SUBPALETTE",
        "IDM_SUBPALETTE_PIN",
        "IDM_WINDOW_SEQUENCE",
        "IDM_SEQUENCE_PIN",
        "IDM_WINDOW_LIGHT_TABLE",
        "IDM_LIGHT_TABLE_PIN",
        "IDM_WORKSPACE_AUTOHIDE_SEQUENCE",
        "IDM_WORKSPACE_AUTOHIDE_LIGHT_TABLE",
        "IDM_WORKSPACE_AUTOHIDE_REFERENCE",
    }:
        return 9
    if symbol in {
        "IDM_WINDOW_BATCH",
        "IDM_BATCH_PIN",
        "IDM_WORKSPACE_AUTOHIDE_BATCH",
        "IDM_WINDOW_JOB_PROGRESS",
    }:
        return 10
    req = requirement(symbol)
    if req == "SNAP-001":
        return 2
    if req in {"VIEW-001", "VIEW-004", "VIEW-005", "SNAP-001", "PAINT-001", "HIST-001"}:
        if symbol.startswith("IDM_VIEW_") and not any(
            token in symbol for token in ("NEW", "CLOSE", "MOVE", "DUPLICATE", "VECTOR")
        ):
            return 2
    if req in {"SHORT-001", "WIN-001"} or symbol == "IDM_APP_EXIT":
        return 3
    if req in {"IO-001", "IO-002", "CLIP-001", "SESSION-001"}:
        return 4
    if req in {"CELL-001", "DOC-003", "VIEW-004", "WORKSPACE-001", "WORKSPACE-002"}:
        return 5
    if req in {
        "PAINT-001",
        "FILL-001",
        "COLOR-REPLACE-001",
        "COLOR-001",
        "COLOR-002",
        "COLOR-OUTPUT-QA-001",
        "VIEW-003",
    }:
        return 6
    if req in {"SEL-001", "XFORM-003", "HIST-001"}:
        return 7
    if req in {
        "FILTER-001",
        "EFFECT-001",
        "ADJUST-001",
        "VECTOR-001",
        "ANNOTATION-001",
        "SHOOTING-FRAME-001",
        "VANISHING-POINT-001",
        "PAINT-002",
        "VIEW-005",
    }:
        return 8
    if req in {"CUT-001", "SEQ-STRUCT-001", "SEQ-001", "SEQ-ENDPOINT-001", "SEQ-002", "LT-001", "LT-003"}:
        return 9
    if req in {"BATCH-001", "BATCH-004"}:
        return 10
    raise ValueError(f"no milestone mapping for {symbol} ({req})")


def owners_and_scope(symbol: str) -> tuple[str, str, str]:
    if symbol.startswith(("IDM_HELP_", "IDM_LANGUAGE_", "IDM_SHORTCUT_", "IDM_APP_")):
        return "ApplicationCommandRouter", "ApplicationStateProvider", "application"
    if symbol == "IDM_FILE_NEW_CUT":
        return "CutCommandRouter", "CutStateProvider", "cutSession"
    if symbol.startswith("IDM_FILE_"):
        return "FileLifecycleRouter", "FileLifecycleStateProvider", "documentSession"
    if symbol.startswith("IDM_CUT_"):
        return "CutCommandRouter", "CutStateProvider", "cutSession"
    if symbol.startswith("IDM_BATCH_"):
        return "BatchCommandRouter", "BatchStateProvider", "job"
    if symbol.startswith(("IDM_WINDOW_", "IDM_WORKSPACE_", "IDM_TAB_", "IDM_EDITOR_")):
        return "WorkspaceCommandRouter", "WorkspaceStateProvider", "workspace"
    if symbol in {"IDM_DOCUMENT_CLOSE"}:
        return "SessionCommandRouter", "SessionStateProvider", "documentSession"
    if symbol.startswith(("IDM_VIEW_", "IDM_LOCATOR_")):
        return "ViewCommandRouter", "ViewStateProvider", "documentView"
    if symbol.endswith("_PIN"):
        return "PaneTargetRouter", "PaneTargetStateProvider", "pane"
    if symbol.startswith(("IDM_SEQ_", "IDM_MOTION_", "IDM_LT_", "IDM_SUBPALETTE_")):
        return "AnimationCommandRouter", "AnimationStateProvider", "cutSession"
    if symbol.startswith(("IDM_COLOR_", "IDM_PALETTE_", "IDM_CHART_")):
        return "ColorCommandRouter", "ColorStateProvider", "documentSession"
    if symbol.startswith(("IDM_SELECTION_", "IDM_EDIT_")):
        return "EditCommandRouter", "EditStateProvider", "documentSession"
    if symbol.startswith(("IDM_FILTER_", "IDM_EFFECT_", "IDM_ADJUSTMENT_")):
        return "ImageCommandRouter", "ImageStateProvider", "documentSession"
    if symbol.startswith(("IDM_TOOL_", "IDM_VECTOR_", "IDM_GEOMETRY_")):
        return "ToolCommandRouter", "ToolStateProvider", "documentView"
    if symbol.startswith(("IDM_CELL_", "IDM_LAYER_", "IDM_PLANE_", "IDM_ANNOTATION_")):
        return "CellCommandRouter", "CellStateProvider", "documentSession"
    raise ValueError(f"no owner mapping for {symbol}")


def surface(symbol: str) -> str:
    if symbol.startswith("IDM_APP_") or symbol.startswith("IDM_LANGUAGE_"):
        return "App menu / Settings"
    if symbol.startswith(("IDM_FILE_", "IDM_CUT_")):
        return "File menu"
    if symbol.startswith("IDM_EDIT_"):
        return "Edit menu"
    if symbol.startswith(("IDM_VIEW_", "IDM_TAB_", "IDM_EDITOR_")):
        return "View menu"
    if symbol.startswith(("IDM_CELL_", "IDM_LAYER_", "IDM_PLANE_", "IDM_ANNOTATION_")):
        return "Cell menu"
    if symbol.startswith("IDM_SELECTION_"):
        return "Selection menu"
    if symbol.startswith(("IDM_FILTER_", "IDM_EFFECT_", "IDM_ADJUSTMENT_")):
        return "Image menu"
    if symbol.startswith(("IDM_TOOL_", "IDM_VECTOR_", "IDM_GEOMETRY_")):
        return "Tools menu"
    if symbol.startswith(("IDM_COLOR_", "IDM_PALETTE_", "IDM_CHART_")):
        return "Color menu"
    if symbol.startswith(("IDM_SEQ_", "IDM_MOTION_", "IDM_LT_", "IDM_SUBPALETTE_")):
        return "Animation menu"
    if symbol.startswith(("IDM_WINDOW_", "IDM_WORKSPACE_", "IDM_LOCATOR_")) or symbol.endswith("_PIN"):
        return "Window menu"
    if symbol.startswith("IDM_BATCH_"):
        return "Batch window"
    if symbol.startswith("IDM_SHORTCUT_"):
        return "Settings"
    if symbol.startswith("IDM_HELP_"):
        return "Help menu"
    if symbol == "IDM_DOCUMENT_CLOSE":
        return "File menu"
    raise ValueError(f"no surface mapping for {symbol}")


def disposition(symbol: str) -> tuple[str, str, bool | None]:
    if symbol.startswith("IDM_FILE_RECENT_"):
        return (
            "mergedIntoSemanticCommand",
            "The eight fixed Windows slots become one native dynamic Open Recent semantic command.",
            None,
        )
    if symbol in NOT_APPLICABLE:
        return (
            "notApplicable",
            "SPEC WORKSPACE-001 visibility remains available through native sidebar, inspector, timeline, or utility-window controls; Windows AutoHide edge behavior is not reproduced.",
            True,
        )
    if symbol in STANDARD_INTEGRATED:
        return (
            "macStandardIntegrated",
            "The shared SPEC behavior is integrated with the corresponding standard macOS command role and shortcut.",
            None,
        )
    return (
        "macEquivalent",
        f"The shared SPEC behavior maps to the native macOS surface scheduled for milestone {milestone(symbol)}.",
        None,
    )


def build_manifest(repository: Path) -> dict[str, object]:
    resource = repository / "apps/windows/app/resource.h"
    ja_resource = repository / "apps/windows/app/app_ui_ja.generated.rc"
    definitions = {
        name: int(value)
        for name, value in DEFINE_PATTERN.findall(resource.read_text(encoding="utf-8"))
    }
    production = set(ID_PATTERN.findall(ja_resource.read_text(encoding="utf-8")))
    if production | EXCLUDED != set(definitions):
        raise ValueError("Windows command inputs do not match the expected production/excluded partition")
    rows: list[dict[str, object]] = []
    for symbol in sorted(production, key=lambda item: (definitions[item], item)):
        route_owner, state_owner, target_scope = owners_and_scope(symbol)
        mapped_disposition, reason, alternative = disposition(symbol)
        target_milestone = milestone(symbol)
        implemented = target_milestone in IMPLEMENTED_MILESTONES
        if implemented and (
            target_milestone in {2, 3} or "scheduled for milestone" in reason
        ):
            reason = (
                "The shared SPEC behavior is connected to the native macOS command "
                "router, state provider, and declared surface."
            )
        row: dict[str, object] = {
            "windowsId": symbol,
            "numericId": definitions[symbol],
            "semanticKey": semantic_key(symbol),
            "requirementId": requirement(symbol),
            "disposition": mapped_disposition,
            "reason": reason,
            "macSurface": surface(symbol),
            "routeOwner": route_owner,
            "stateOwner": state_owner,
            "targetScope": target_scope,
            "milestone": target_milestone,
            "testId": (
                "MAC-BATCH-WORKFLOW-001"
                if target_milestone == 10
                else "MAC-CUT-WORKFLOW-001"
                if target_milestone == 9 and requirement(symbol) == "CUT-001"
                else "MAC-SEQUENCE-STRUCTURE-001"
                if target_milestone == 9 and requirement(symbol) == "SEQ-STRUCT-001"
                else "MAC-LIGHT-TABLE-001"
                if target_milestone == 9 and requirement(symbol) in {"LT-001", "LT-003"}
                else "MAC-SEQUENCE-ENDPOINT-001"
                if target_milestone == 9 and requirement(symbol) == "SEQ-ENDPOINT-001"
                else "MAC-MOTION-SUBPALETTE-001"
                if target_milestone == 9 and requirement(symbol) in {"SEQ-002", "COLOR-002"}
                else "MAC-SEQUENCE-WORKFLOW-001"
                if target_milestone == 9 and requirement(symbol) == "SEQ-001"
                else "MAC-ANIMATION-SURFACE-001"
                if target_milestone == 9
                else "MAC-RENDER-DIAGNOSTICS-001"
                if target_milestone == 8 and requirement(symbol) == "VIEW-005"
                else "MAC-VECTOR-WORKFLOW-001"
                if target_milestone == 8 and requirement(symbol) in {"VECTOR-001", "PAINT-002"}
                else "MAC-ANNOTATION-WORKFLOW-001"
                if target_milestone == 8 and requirement(symbol) == "ANNOTATION-001"
                else "MAC-FRAME-GUIDE-001"
                if target_milestone == 8 and requirement(symbol) in {
                    "SHOOTING-FRAME-001", "VANISHING-POINT-001"
                }
                else "MAC-FILTER-EFFECT-001"
                if target_milestone == 8
                else
                "MAC-SELECTION-HISTORY-001"
                if target_milestone == 7
                else "MAC-PAINT-FILL-001"
                if target_milestone == 6 and requirement(symbol) in {
                    "PAINT-001", "FILL-001", "COLOR-REPLACE-001"
                }
                else "MAC-COLOR-OUTPUT-QA-001"
                if target_milestone == 6 and requirement(symbol) == "COLOR-OUTPUT-QA-001"
                else "MAC-LOCATOR-001"
                if target_milestone == 6 and requirement(symbol) == "VIEW-003"
                else "MAC-COLOR-WORKFLOW-001"
                if target_milestone == 6 and requirement(symbol) in {"COLOR-001", "COLOR-002"}
                else "MAC-PAINT-SURFACE-001"
                if target_milestone == 6
                else
                "MAC-CELL-WORKFLOW-001"
                if target_milestone == 5 and requirement(symbol) in {"CELL-001", "DOC-003"}
                else "MAC-WORKSPACE-001"
                if target_milestone == 5
                else "MAC-CLIPBOARD-001"
                if target_milestone == 4 and requirement(symbol) == "CLIP-001"
                else "MAC-FILE-LIFECYCLE-001"
                if target_milestone == 4
                else "MAC-COMMAND-SURFACE-001"
                if implemented
                else "MAC-PARITY-LEDGER-001"
            ),
            "implementation": "implemented" if implemented else "planned",
        }
        if alternative is not None:
            row["alternativeSurface"] = alternative
        rows.append(row)
    return {
        "schemaVersion": 1,
        "source": {
            "rawSymbolCount": 387,
            "productionCommandCount": 384,
            "reservedAggregateCount": 1,
            "rangeMarkerCount": 2,
            "occurrencesPerLanguage": 391,
        },
        "commands": rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    manifest = build_manifest(arguments.repository.resolve())
    arguments.output.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
