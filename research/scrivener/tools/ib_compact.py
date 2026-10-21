#!/usr/bin/env python3
"""Build a compact, reviewable summary of the decoded Interface Builder graphs.

The full REA decode of every NIB is ~32 MB of JSON. This reduces it to the
structure a UX review actually needs: per document, the hierarchy, the
outlets, the target-actions, and the class list.

Usage:
  ib_compact.py <ib_nibs_dir> --out <summary.json>
"""

from __future__ import annotations

import argparse
import json
import os
from collections import Counter, defaultdict


def load(path: str):
    with open(path, encoding="utf-8") as handle:
        payload = json.load(handle)
    nr = payload.get("normalized_result", payload)
    return nr.get("documents", []), nr.get("graph", {})


def node_doc(node_id: str) -> str:
    if node_id.startswith('ib:"') and '":' in node_id[4:]:
        return node_id[4:].split('":', 1)[0]
    return ""


def compact(path: str) -> dict:
    docs, graph = load(path)
    nodes = graph.get("nodes", [])
    edges = graph.get("edges", [])

    by_doc_nodes: dict[str, list[dict]] = defaultdict(list)
    for node in nodes:
        by_doc_nodes[node_doc(node.get("id", ""))].append(node)
    by_doc_edges: dict[str, list[dict]] = defaultdict(list)
    for edge in edges:
        by_doc_edges[node_doc(edge.get("from", ""))].append(edge)

    out_docs = []
    for doc in docs:
        rel = doc["relative_path"]
        dnodes = by_doc_nodes.get(rel, [])
        dedges = by_doc_edges.get(rel, [])

        actions, outlets, classes, controls = [], [], [], []
        for node in dnodes:
            kind = node.get("kind")
            name = node.get("name", "")
            if kind == "action":
                actions.append(name)
            elif kind == "outlet":
                outlets.append(name)
            elif kind == "objc_class":
                classes.append(name)
            elif kind in ("control", "view", "view_controller"):
                controls.append(name)

        rel_counts = Counter(e.get("relation", "?") for e in dedges)

        out_docs.append(
            {
                "document": rel.split("/")[-1],
                "path": rel,
                "kind": doc.get("document_kind"),
                "object_count": doc.get("object_count"),
                "connection_count": doc.get("connection_count"),
                "hierarchy_complete": doc.get("hierarchy_complete"),
                "node_kinds": dict(
                    Counter(n.get("kind", "unknown") for n in dnodes)
                ),
                "edge_relations": dict(rel_counts),
                "actions": sorted(set(actions)),
                "outlets": sorted(set(outlets)),
                "classes": sorted(set(classes)),
                "controls": sorted(set(controls)),
            }
        )

    return {"documents": out_docs}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("nibs_dir")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    files = sorted(
        os.path.join(args.nibs_dir, name)
        for name in os.listdir(args.nibs_dir)
        if name.endswith(".json")
    )
    documents = []
    for path in files:
        documents.extend(compact(path)["documents"])

    payload = {
        "target": "Scrivener 3.5.2 (build 17486) main bundle Interface Builder resources",
        "document_count": len(documents),
        "documents": documents,
    }
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=1)
    print(f"wrote {args.out} ({len(documents)} documents)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
