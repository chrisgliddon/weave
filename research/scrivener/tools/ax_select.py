#!/usr/bin/env python3
"""Select a Binder row by title in a running Scrivener window.

Usage:
  ax_select.py <pid> <windowName> <rowTitle>
"""

from __future__ import annotations

import ctypes
import sys
import time

sys.path.insert(0, __file__.rsplit("/", 1)[0])

import ax_probe as ax  # noqa: E402


def binder_rows(window):
    """Return every Binder row element in a window."""
    rows = []
    stack = [(window, [])]
    while stack:
        element, path = stack.pop()
        for index, child in enumerate(ax.children(element)):
            child_path = path + [index]
            role = ax.attribute(child, "AXRole")
            if role == "AXRow":
                rows.append((child, child_path))
            stack.append((child, child_path))
    return rows


def row_title(row):
    """Return the title text of a Binder row."""
    for child in ax.children(row):
        if ax.attribute(child, "AXRole") != "AXCell":
            continue
        for cell_child in ax.children(child):
            if ax.attribute(cell_child, "AXRole") == "AXTextField":
                return ax.attribute(cell_child, "AXValue")
    return None


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__)
        return 2
    pid = int(sys.argv[1])
    window_name = sys.argv[2]
    wanted = sys.argv[3]

    application = ctypes.c_void_p(ax.AS.AXUIElementCreateApplication(pid))
    window = ax.find_window(application, window_name)
    for row, path in binder_rows(window):
        if row_title(row) == wanted:
            ax.press(row, "AXPress")
            time.sleep(0.8)
            print(f"selected {wanted!r} at {'.'.join(str(p) for p in path)}")
            return 0
    print(f"row not found: {wanted!r}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
