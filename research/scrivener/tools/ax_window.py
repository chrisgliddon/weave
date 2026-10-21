#!/usr/bin/env python3
"""Report the screen position of a running application window.

Window screenshots are captured in window-local coordinates, but mouse
events are posted in global screen coordinates, so a click has to be
translated before it is sent.

Usage:
  ax_window.py <pid> [<windowName>]
"""

from __future__ import annotations

import sys

import Quartz


def windows(owner_name: str | None = None):
    """Return every on-screen window, optionally filtered by owner."""
    result = []
    for window in Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    ):
        owner = window.get("kCGWindowOwnerName")
        if owner_name and owner != owner_name:
            continue
        bounds = window.get("kCGWindowBounds") or {}
        result.append(
            {
                "id": window.get("kCGWindowNumber"),
                "owner": owner,
                "name": window.get("kCGWindowName"),
                "x": bounds.get("X", 0),
                "y": bounds.get("Y", 0),
                "width": bounds.get("Width", 0),
                "height": bounds.get("Height", 0),
            }
        )
    return result


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    pid = int(sys.argv[1])
    wanted = sys.argv[2] if len(sys.argv) > 2 else None
    for window in windows():
        if wanted and window["name"] != wanted:
            continue
        print(
            f"id={window['id']} name={window['name']!r} "
            f"origin=({window['x']},{window['y']}) "
            f"size={window['width']}x{window['height']}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
