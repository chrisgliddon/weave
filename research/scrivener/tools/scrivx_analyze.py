#!/usr/bin/env python3
"""Analyse a Scrivener `.scrivx` project document.

Prints the Binder tree with per-node type, metadata and ordering, and
optionally dumps a single node's detail. Used to correlate runtime
observations (Binder tree, Corkboard card order) with the on-disk
document model that Scrivener persists.

Usage:
  scrivx_analyze.py <project.scrivx> [--depth N] [--node TITLE] [--meta]
"""

from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET


def load(path: str) -> ET.Element:
    return ET.parse(path).getroot()


def item_meta(item: ET.Element) -> dict:
    meta = item.find("MetaData")
    out = {}
    if meta is not None:
        for child in meta:
            out[child.tag] = (child.text or "").strip()
    return out


def item_body(item: ET.Element) -> dict:
    body = item.find("TextSettings")
    out = {}
    if body is not None:
        for child in body:
            out[child.tag] = (child.text or "").strip()
    return out


def walk(parent: ET.Element, depth: int = 0, max_depth: int = 99, lines=None):
    if lines is None:
        lines = []
    for child in parent:
        if child.tag != "BinderItem":
            continue
        title = (child.findtext("Title") or "").strip()
        meta = item_meta(child)
        uuid = child.get("UUID", "")
        kind = child.get("Type", "?")
        note = ""
        if "StatusID" in meta:
            note += f" status={meta['StatusID']}"
        if meta.get("Label"):
            note += f" label={meta['Label']}"
        if meta.get("IncludeInCompile") == "No":
            note += " (excluded)"
        lines.append(
            f"{'  ' * depth}- {title!r} [{kind}] id={uuid[:8]}{note}"
        )
        if depth + 1 < max_depth:
            walk(child, depth + 1, max_depth, lines)
    return lines


def find_node(root: ET.Element, title: str):
    for item in root.iter("BinderItem"):
        if (item.findtext("Title") or "").strip() == title:
            return item
    return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("scrivx")
    parser.add_argument("--depth", type=int, default=99)
    parser.add_argument("--node")
    parser.add_argument("--meta", action="store_true")
    args = parser.parse_args()

    root = load(args.scrivx)

    if args.node:
        item = find_node(root, args.node)
        if item is None:
            print(f"node not found: {args.node}", file=sys.stderr)
            return 1
        print(json.dumps({
            "uuid": item.get("UUID"),
            "type": item.get("Type"),
            "title": (item.findtext("Title") or "").strip(),
            "created": item.get("Created"),
            "modified": item.get("Modified"),
            "metadata": item_meta(item),
            "text_settings": item_body(item),
            "child_titles": [
                (c.findtext("Title") or "").strip()
                for c in item.find("Children") or []
            ] if item.find("Children") is not None else [],
        }, indent=2))
        return 0

    print("\n".join(walk(root.find("Binder"), 0, args.depth)))
    if args.meta:
        print("\n--- project attributes ---")
        print(json.dumps(root.attrib, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
