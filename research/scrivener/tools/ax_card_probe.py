#!/usr/bin/env python3
"""Measure how much of a document body a Scrivener Corkboard card shows.

The card is a projection of the document body, so the boundary can be found
by writing bodies of increasing length and reading the rendered card back
from a screenshot of the card region.

Usage:
  ax_card_probe.py <pid> <windowName> <characterCount>
"""

from __future__ import annotations

import ctypes
import sys
import time

sys.path.insert(0, __file__.rsplit("/", 1)[0])

import ax_probe as ax  # noqa: E402

# The document body used for the measurement. The filler is a repeated
# sentence so the rendered card text can be matched back to the source.
FILLER = "The courier crossed the courtyard at a run and did not look back. "


def find_element(window, role, predicate):
    """Return the first element matching a role and a predicate."""
    stack = [(window, [])]
    while stack:
        element, path = stack.pop()
        for index, child in enumerate(ax.children(element)):
            child_path = path + [index]
            if ax.attribute(child, "AXRole") == role and predicate(child):
                return child_path, child
            stack.append((child, child_path))
    return None, None


def body_of(length: int) -> str:
    """Return a body of roughly the requested character length."""
    text = FILLER * 40
    return text[:length]


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__)
        return 2
    pid = int(sys.argv[1])
    window_name = sys.argv[2]
    length = int(sys.argv[3])

    application = ctypes.c_void_p(ax.AS.AXUIElementCreateApplication(pid))
    window = ax.find_window(application, window_name)
    path, area = find_element(
        window, "AXTextArea", lambda e: bool(ax.attribute(e, "AXValue"))
    )
    if area is None:
        raise SystemExit("no document body text area found")
    text = body_of(length)
    rc = ax.AS.AXUIElementSetAttributeValue(
        area, ax.cf_string("AXValue"), ax.cf_string(text)
    )
    print(f"set body of {len(text)} chars at {'.'.join(map(str, path))}: rc={rc}")
    time.sleep(1.2)
    return 0


if __name__ == "__main__":
    sys.exit(main())
