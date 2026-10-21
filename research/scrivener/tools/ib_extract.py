#!/usr/bin/env python3
"""Extract reader-friendly structure from REA Interface Builder decode Evidence.

Reads the per-NIB JSON files produced by `rea decode-interface-builder` and
emits a condensed structural summary: view/controller hierarchy, outlets,
target-actions, and connection edges.

Usage:
  ib_extract.py <ib_nibs_dir> [--out FILE]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections import defaultdict


def load_docs(path: str) -> list[dict]:
    with open(path, encoding="utf-8") as handle:
        payload = json.load(handle)
    if "normalized_result" in payload:
        return payload["normalized_result"].get("documents", [])
    return payload.get("documents", [])


def load_graph(path: str) -> tuple[list[dict], list[dict]]:
    with open(path, encoding="utf-8") as handle:
        payload = json.load(handle)
    if "normalized_result" in payload:
        graph = payload["normalized_result"].get("graph", {})
    else:
        graph = payload.get("graph", {})
    return graph.get("nodes", []), graph.get("edges", [])


def node_doc(node: dict) -> str:
    ident = node.get("id", "")
    if ident.startswith('ib:"') and '":' in ident[4:]:
        return ident[4:].split('":', 1)[0]
    return ""


def summarise(path: str) -> dict:
    docs = load_docs(path)
    nodes, edges = load_graph(path)

    by_doc_nodes: dict[str, list[dict]] = defaultdict(list)
    for node in nodes:
        by_doc_nodes[node_doc(node)].append(node)

    by_doc_edges: dict[str, list[dict]] = defaultdict(list)
    for edge in edges:
        doc = node_doc({"id": edge.get("from", "")})
        by_doc_edges[doc].append(edge)

    out = []
    for doc in docs:
        rel = doc["relative_path"]
        dnodes = by_doc_nodes.get(rel, [])
        dedges = by_doc_edges.get(rel, [])
        kinds = defaultdict(int)
        for node in dnodes:
            kinds[node.get("kind", "unknown")] += 1

        outlets = []
        actions = []
        controls = []
        objc_classes = []
        for node in dnodes:
            kind = node.get("kind")
            name = node.get("name", "")
            attrs = node.get("attributes", {}) or {}
            if kind == "outlet":
                outlets.append({"name": name, "attributes": attrs})
            elif kind == "action":
                actions.append({"name": name, "attributes": attrs})
            elif kind in ("control", "view", "view_controller", "constraint"):
                controls.append({"kind": kind, "name": name, "attributes": attrs})
            elif kind == "objc_class":
                objc_classes.append(name)

        rel_counts = defaultdict(int)
        for edge in dedges:
            rel_counts[edge.get("relation", "?")] += 1

        out.append(
            {
                "document": rel,
                "document_kind": doc.get("document_kind"),
                "object_count": doc.get("object_count"),
                "connection_count": doc.get("connection_count"),
                "hierarchy_complete": doc.get("hierarchy_complete"),
                "node_kinds": dict(sorted(kinds.items(), key=lambda kv: -kv[1])),
                "outlets": outlets,
                "actions": actions,
                "objc_classes": sorted(set(objc_classes)),
                "edge_relations": dict(sorted(rel_counts.items(), key=lambda kv: -kv[1])),
                "controls": controls,
            }
        )
    return {"documents": out, "nodes": nodes, "edges": edges}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("nibs_dir")
    parser.add_argument("--out")
    parser.add_argument("--full", action="store_true", help="include controls")
    args = parser.parse_args()

    files = sorted(
        os.path.join(args.nibs_dir, name)
        for name in os.listdir(args.nibs_dir)
        if name.endswith(".json")
    )
    combined = {"documents": [], "nodes": [], "edges": []}
    per_file = {}
    for path in files:
        name = os.path.splitext(os.path.basename(path))[0]
        data = summarise(path)
        per_file[name] = data
        combined["documents"].extend(data["documents"])
        combined["nodes"].extend(data["nodes"])
        combined["edges"].extend(data["edges"])

    if not args.full:
        for doc in combined["documents"]:
            doc.pop("controls", None)

    text = json.dumps(combined, indent=1)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(text)
        print(f"wrote {args.out} ({len(combined['documents'])} documents)")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
