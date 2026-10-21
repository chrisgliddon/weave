#!/usr/bin/env python3
"""Print the Binder tree of a Scrivener project file, including nested folders.

`scrivx_analyze.py` walks the top-level `Binder` element, which does not
descend into the DraftFolder. This tool walks every `BinderItem` in the
document so a cross-container move can be verified from the persisted file.

Usage:
  scrivx_tree.py <project.scrivx> [--meta]
"""

from __future__ import annotations

import argparse
import sys
import xml.etree.ElementTree as ET


def item_meta(item: ET.Element) -> dict:
    """Return the metadata children of a Binder item."""
    meta = item.find("MetaData")
    if meta is None:
        return {}
    return {child.tag: (child.text or "").strip() for child in meta}


def walk(parent: ET.Element, depth: int, show_meta: bool, lines: list[str]) -> None:
    """Append one line per Binder item, depth first."""
    for child in parent:
        if child.tag != "BinderItem":
            continue
        title = (child.findtext("Title") or "").strip()
        meta = item_meta(child)
        notes = ""
        if show_meta:
            notes = " " + " ".join(f"{k}={v}" for k, v in sorted(meta.items()) if v)
        lines.append(
            f"{'  ' * depth}- {title!r} [{child.get('Type')}] "
            f"id={child.get('UUID', '')[:8]}{notes}"
        )
        children = child.find("Children")
        if children is not None:
            walk(children, depth + 1, show_meta, lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scrivx")
    parser.add_argument("--meta", action="store_true")
    args = parser.parse_args()

    root = ET.parse(args.scrivx).getroot()
    binder = root.find("Binder")
    if binder is None:
        print("no Binder element", file=sys.stderr)
        return 1
    lines: list[str] = []
    walk(binder, 0, args.meta, lines)
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
