#!/usr/bin/env python3
"""Save a REA native-UI observation to disk: screenshot PNG plus a tree text file.

Reads the JSON Evidence emitted by `rea observe-native-ui` (or the
capture-native-ui-scenario output) and writes:

  <out_dir>/<name>.png   the window screenshot, if present
  <out_dir>/<name>.txt   the accessibility tree as indented text
  <out_dir>/<name>.json  the original Evidence, unchanged

Usage:
  save_observation.py <evidence.json> --out-dir DIR --name NAME
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys


def collect(payload: dict) -> list[dict]:
    """Return every capture block in an observation payload."""
    blocks: list[dict] = []
    nr = payload.get("normalized_result", payload)
    if isinstance(nr.get("initial"), dict):
        blocks.append(nr["initial"])
    for step in nr.get("steps", []) or []:
        for phase in ("before", "after"):
            if isinstance(step.get(phase), dict):
                blocks.append(step[phase])
    return blocks


def render_tree(nodes: list[dict]) -> str:
    children: dict[str, list[dict]] = {}
    for node in nodes:
        key = json.dumps(node.get("path", [])[:-1])
        children.setdefault(key, []).append(node)
    lines: list[str] = []

    def walk(path: list, depth: int, seen: frozenset) -> None:
        key = json.dumps(path)
        # The accessibility tree can report a node as its own ancestor.
        # Guard against that so rendering always terminates.
        if key in seen or depth > 64:
            return
        next_seen = seen | {key}
        for child in children.get(key, []):
            parts = [child.get("role") or "?"]
            if child.get("title"):
                parts.append(f'"{child["title"]}"')
            if child.get("value") is not None:
                parts.append(f'={str(child["value"])[:120]}')
            if child.get("actions"):
                parts.append("{" + ",".join(child["actions"]) + "}")
            lines.append("  " * depth + "- " + " ".join(parts))
            walk(child.get("path", []), depth + 1, next_seen)

    walk([], 0, frozenset())
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence")
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--name", required=True)
    args = parser.parse_args()

    with open(args.evidence, encoding="utf-8") as handle:
        payload = json.load(handle)

    os.makedirs(args.out_dir, exist_ok=True)
    blocks = collect(payload)
    written = []

    for index, block in enumerate(blocks):
        suffix = "" if index == 0 else f".{index}"
        shot = block.get("screenshot")
        if shot and shot.get("base64"):
            png_path = os.path.join(args.out_dir, f"{args.name}{suffix}.png")
            with open(png_path, "wb") as handle:
                handle.write(base64.b64decode(shot["base64"]))
            written.append(png_path)

        tree = render_tree(block.get("nodes", []) or [])
        tree_path = os.path.join(args.out_dir, f"{args.name}{suffix}.txt")
        header = [
            f"# window: {json.dumps(block.get('window', {}))}",
            f"# nodes: {len(block.get('nodes', []) or [])} truncated={block.get('truncated')}",
            f"# gaps: {json.dumps(block.get('gaps', []))}",
            "",
        ]
        with open(tree_path, "w", encoding="utf-8") as handle:
            handle.write("\n".join(header) + tree + "\n")
        written.append(tree_path)

    raw_path = os.path.join(args.out_dir, f"{args.name}.evidence.json")
    with open(raw_path, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=1)
    written.append(raw_path)

    for path in written:
        print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
