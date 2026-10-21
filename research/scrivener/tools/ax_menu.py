#!/usr/bin/env python3
"""Walk a macOS menu hierarchy through the AX API and report item state.

`System Events` cannot open a submenu of an item that lives inside a menu bar
item's own menu, so nested menus (`View > Outliner Options > Columns`) need
the AX API directly. This script opens the menu bar item, walks to the
requested submenu, and reports every item with its check mark and state.

Usage:
  ax_menu.py <pid> <MenuBarItem> [<MenuItem> [<SubMenu> ...]]

  ax_menu.py 29402 View "Outliner Options" Columns
  ax_menu.py 29402 View "Outliner Options"

With no menu item arguments the whole menu bar item is listed.
"""

from __future__ import annotations

import ctypes
import sys
import time

AS = ctypes.cdll.LoadLibrary(
    "/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices"
)

AS.AXUIElementCreateApplication.restype = ctypes.c_void_p
AS.AXUIElementCreateApplication.argtypes = [ctypes.c_int]
AS.AXUIElementCopyAttributeValue.restype = ctypes.c_int
AS.AXUIElementCopyAttributeValue.argtypes = [
    ctypes.c_void_p,
    ctypes.c_char_p,
    ctypes.POINTER(ctypes.c_void_p),
]
AS.AXUIElementPerformAction.restype = ctypes.c_int
AS.AXUIElementPerformAction.argtypes = [ctypes.c_void_p, ctypes.c_char_p]


def attribute(element, name):
    """Return an AX attribute value, or None when the attribute is unset."""
    if not element:
        return None
    holder = ctypes.c_void_p()
    error = AS.AXUIElementCopyAttributeValue(
        element, name.encode("utf-8"), ctypes.byref(holder)
    )
    if error != 0 or not holder:
        return None
    if name == "AXChildren":
        return list(ctypes.cast(holder, ctypes.POINTER(ctypes.py_object)).value or [])
    return ctypes.cast(holder, ctypes.POINTER(ctypes.c_void_p)).contents.value


def children(element):
    """Return the AX children of an element."""
    value = attribute(element, "AXChildren")
    return list(value) if value else []


def title(element):
    return attribute(element, "AXTitle")


def child_named(element, name):
    for child in children(element):
        if title(child) == name:
            return child
    return None


def press(element):
    """Press an element and wait for the UI to settle."""
    AS.AXUIElementPerformAction(element, b"AXPress")
    time.sleep(0.45)


def open_menu_bar_item(application, name):
    """Press a menu bar item and return its menu element."""
    bar = child_named(application, name)
    if bar is None:
        raise SystemExit(f"menu bar item not found: {name}")
    press(bar)
    menu = attribute(bar, "AXMenu")
    if menu is None:
        raise SystemExit(f"no AXMenu on menu bar item: {name}")
    return menu


def open_submenu(menu, name):
    """Return the submenu of a named item, opening it if needed."""
    item = child_named(menu, name)
    if item is None:
        available = [title(child) for child in children(menu)]
        raise SystemExit(f"menu item not found: {name}; available: {available}")
    submenu = attribute(item, "AXMenu")
    if submenu is None:
        # Items without a persistent AXMenu expose children only once opened.
        press(item)
        time.sleep(0.3)
        submenu = attribute(item, "AXMenu")
    if submenu is None:
        raise SystemExit(f"item has no submenu: {name}")
    return submenu


def describe(element, depth, out):
    """Append one line per element, recursing into submenus."""
    mark = attribute(element, "AXMenuItemMarkChar")
    enabled = attribute(element, "AXEnabled")
    role = attribute(element, "AXRole")
    out.append(
        "  " * depth + f"{title(element)!r} role={role} mark={mark!r} enabled={enabled}"
    )
    submenu = attribute(element, "AXMenu")
    if submenu is not None:
        items = children(submenu)
        out.append("  " * depth + f"  [submenu {len(items)} items]")
        for child in items:
            describe(child, depth + 2, out)


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    pid = int(sys.argv[1])
    path = sys.argv[2:]

    application = ctypes.c_void_p(AS.AXUIElementCreateApplication(pid))
    menu = open_menu_bar_item(application, path[0])
    for name in path[1:]:
        menu = open_submenu(menu, name)

    out: list[str] = []
    for child in children(menu):
        describe(child, 0, out)
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
