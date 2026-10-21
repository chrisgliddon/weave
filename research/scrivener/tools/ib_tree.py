#!/usr/bin/env python3
"""Render the Interface Builder containment tree for one decoded NIB.

Prints the `contains` hierarchy with control class, identifier and label for
each node, plus its target-actions and outlets.

Usage:
  ib_tree.py <nib.json> [--depth N] [--actions] [--outlets]
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict


def load(path: str) -> tuple[list[dict], list[dict]]:
    with open(path, encoding="utf-8") as handle:
        payload = json.load(handle)
    graph = (
        payload.get("normalized_result", payload).get("graph", {})
    )
    return graph.get("nodes", []), graph.get("edges", [])


def node_label(node: dict) -> str:
    attrs = node.get("attributes", {}) or {}
    nib = attrs.get("nibValues", {}) or {}
    ident = nib.get("NSIdentifier") or attrs.get("NSIdentifier") or ""
    label = attrs.get("label")
    if label is None:
        label = nib.get("NSLabel")
    if isinstance(label, dict):
        label = label.get("objectID")
    title = attrs.get("title") or nib.get("NSTitle")
    bits = [node.get("kind", "?")]
    if node.get("name"):
        bits.append(str(node["name"]))
    if ident:
        bits.append(f"id={ident}")
    if label and isinstance(label, str) and label:
        bits.append(f'label="{label}"')
    if title and isinstance(title, str) and title:
        bits.append(f'title="{title}"')
    return " ".join(str(b) for b in bits)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("nib")
    parser.add_argument("--depth", type=int, default=4)
    parser.add_argument("--actions", action="store_true")
    parser.add_argument("--outlets", action="store_true")
    parser.add_argument("--out")
    args = parser.parse_args()

    nodes, edges = load(args.nib)
    by_id = {n["id"]: n for n in nodes}
    children: dict[str, list[str]] = defaultdict(list)
    incoming: set[str] = set()
    for edge in edges:
        if edge.get("relation") == "contains":
            children[edge["from"]].append(edge["to"])
            incoming.add(edge["to"])

    target_actions: dict[str, list[dict]] = defaultdict(list)
    outlet_edges: list[dict] = []
    for edge in edges:
        if edge.get("relation") == "target_action":
            target_actions[edge["from"]].append(edge)
        elif edge.get("relation") == "outlet_to":
            outlet_edges.append(edge)

    roots = [n["id"] for n in nodes if n["id"] not in incoming]

    lines: list[str] = []

    def walk(node_id: str, indent: int) -> None:
        node = by_id.get(node_id)
        if node is None:
            return
        lines.append("  " * indent + "- " + node_label(node))
        if args.actions:
            for edge in target_actions.get(node_id, []):
                to = by_id.get(edge["to"], {})
                lines.append(
                    "  " * (indent + 1)
                    + f"* action -> {to.get('name', edge['to'])}"
                )
        if args.outlets:
            for edge in outlet_edges:
                if edge["from"] == node_id:
                    to = by_id.get(edge["to"], {})
                    lines.append(
                        "  " * (indent + 1)
                        + f"^ outlet -> {to.get('name', edge['to'])}"
                    )
        if indent + 1 >= args.depth:
            return
        for child in children.get(node_id, []):
            walk(child, indent + 1)

    for root in roots:
        walk(root, 0)
        lines.append("")

    text = "\n".join(lines)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(text)
        print(f"wrote {args.out} ({len(lines)} lines)")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
