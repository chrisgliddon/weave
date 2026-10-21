#!/usr/bin/env python3
"""Slim a REA native-UI observation down to its reviewable parts.

The full observation carries a base64 PNG and a verbose node list. This keeps
the screenshot (already saved separately as a PNG) out of the JSON, and keeps
only the accessibility nodes that carry a role, title, value, or action.

Usage:
  slim_observations.py <evidence_dir> [--out-dir DIR]
"""

from __future__ import annotations

import argparse
import json
import os


def slim(payload: dict) -> dict:
    nr = payload.get("normalized_result", payload)

    def slim_block(block: dict) -> dict:
        if not isinstance(block, dict):
            return block
        out = {k: v for k, v in block.items() if k != "screenshot"}
        nodes = []
        for node in out.get("nodes", []) or []:
            if (
                node.get("title") is not None
                or node.get("value") is not None
                or node.get("actions")
                or node.get("role") in ("AXRow", "AXOutline", "AXTextArea")
            ):
                nodes.append(node)
        out["nodes"] = nodes
        out["node_count_total"] = len(block.get("nodes", []) or [])
        shot = block.get("screenshot")
        if shot:
            out["screenshot_sha256"] = shot.get("sha256")
            out["screenshot_size"] = [shot.get("width"), shot.get("height")]
        return out

    result = {
        "evidence_id": payload.get("evidence_id"),
        "operation": payload.get("operation"),
        "subject": payload.get("subject"),
        "provider": payload.get("provider"),
        "confidence": payload.get("confidence"),
        "authority": payload.get("authority"),
        "limitations": payload.get("limitations"),
        "normalized_result": {
            "initial": slim_block(nr.get("initial", {})),
            "steps": [
                {
                    "index": s.get("index"),
                    "kind": s.get("kind"),
                    "outcome": s.get("outcome"),
                    "reason": s.get("reason"),
                    "before": slim_block(s.get("before")),
                    "after": slim_block(s.get("after")),
                }
                for s in nr.get("steps", []) or []
            ],
        },
    }
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence_dir")
    parser.add_argument("--out-dir")
    args = parser.parse_args()

    out_dir = args.out_dir or args.evidence_dir
    os.makedirs(out_dir, exist_ok=True)

    for name in sorted(os.listdir(args.evidence_dir)):
        if not name.startswith("obs_") or not name.endswith(".json"):
            continue
        path = os.path.join(args.evidence_dir, name)
        with open(path, encoding="utf-8") as handle:
            payload = json.load(handle)
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(slim(payload), handle, indent=1)
        print(f"slimmed {name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
