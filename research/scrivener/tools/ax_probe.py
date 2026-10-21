#!/usr/bin/env python3
"""Read and drive a running macOS application through the AX API.

`System Events` can read a menu bar item's own menu, but it cannot open a
submenu of an item that lives inside a menu bar item's menu (`View >
Outliner Options > Columns`). The AX API can, and it also gives direct
access to the check marks on menu items.

Usage:
  ax_probe.py <pid> menus <MenuBarItem> [<MenuItem> [<SubMenu> ...]]
  ax_probe.py <pid> tree <windowName> [roleFilter]

`menus` walks to the requested menu and prints one line per item with its
check mark and enabled state. `tree` prints the accessibility tree of one
window, optionally filtered by role.
"""

from __future__ import annotations

import ctypes
import sys
import time

CF = ctypes.cdll.LoadLibrary(
    "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation"
)
AS = ctypes.cdll.LoadLibrary(
    "/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices"
)

AS.AXUIElementCreateApplication.restype = ctypes.c_void_p
AS.AXUIElementCreateApplication.argtypes = [ctypes.c_int]
AS.AXUIElementCopyAttributeValue.restype = ctypes.c_int
AS.AXUIElementCopyAttributeValue.argtypes = [
    ctypes.c_void_p,
    ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_void_p),
]
AS.AXUIElementPerformAction.restype = ctypes.c_int
AS.AXUIElementPerformAction.argtypes = [ctypes.c_void_p, ctypes.c_void_p]

CF.CFStringCreateWithCString.restype = ctypes.c_void_p
CF.CFStringCreateWithCString.argtypes = [
    ctypes.c_void_p,
    ctypes.c_char_p,
    ctypes.c_uint32,
]
CF.CFArrayGetTypeID.restype = ctypes.c_ulong
CF.CFArrayGetCount.restype = ctypes.c_long
CF.CFArrayGetCount.argtypes = [ctypes.c_void_p]
CF.CFArrayGetValueAtIndex.restype = ctypes.c_void_p
CF.CFArrayGetValueAtIndex.argtypes = [ctypes.c_void_p, ctypes.c_long]
CF.CFBooleanGetTypeID.restype = ctypes.c_ulong
CF.CFBooleanGetValue.restype = ctypes.c_bool
CF.CFBooleanGetValue.argtypes = [ctypes.c_void_p]
CF.CFGetTypeID.restype = ctypes.c_ulong
CF.CFGetTypeID.argtypes = [ctypes.c_void_p]
CF.CFStringGetTypeID.restype = ctypes.c_ulong
CF.CFStringGetCString.restype = ctypes.c_int
CF.CFStringGetCString.argtypes = [
    ctypes.c_void_p,
    ctypes.c_char_p,
    ctypes.c_long,
    ctypes.c_uint32,
]
CF.CFNumberGetTypeID.restype = ctypes.c_ulong
CF.CFNumberGetValue.restype = ctypes.c_int
CF.CFNumberGetValue.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p]

UTF8 = 0x08000100
ARRAY_TYPE_ID = CF.CFArrayGetTypeID()
BOOLEAN_TYPE_ID = CF.CFBooleanGetTypeID()
NUMBER_TYPE_ID = CF.CFNumberGetTypeID()
CF_NUMBER_DOUBLE = 13


def cf_string(text):
    """Create a CFStringRef from a Python string."""
    return ctypes.c_void_p(
        CF.CFStringCreateWithCString(None, text.encode("utf-8"), UTF8)
    )


def cf_string_value(reference):
    """Convert a CFStringRef into a Python string."""
    if not reference:
        return None
    buffer = ctypes.create_string_buffer(4096)
    ok = CF.CFStringGetCString(reference, buffer, ctypes.sizeof(buffer), UTF8)
    return buffer.value.decode("utf-8", "replace") if ok else None


def cf_array_to_list(array):
    """Convert a CFArrayRef into a Python list of opaque element handles."""
    if not array:
        return []
    count = CF.CFArrayGetCount(array)
    return [
        ctypes.c_void_p(CF.CFArrayGetValueAtIndex(array, index))
        for index in range(count)
    ]


def attribute(element, name):
    """Return an AX attribute value as a Python value.

    Arrays become lists of element handles, booleans become `bool`, and
    everything else becomes a string.
    """
    if not element:
        return None
    holder = ctypes.c_void_p()
    error = AS.AXUIElementCopyAttributeValue(
        element, cf_string(name), ctypes.byref(holder)
    )
    if error != 0 or not holder:
        return None
    raw = ctypes.cast(holder, ctypes.c_void_p).value
    if not raw:
        return None
    type_id = CF.CFGetTypeID(raw)
    if type_id == ARRAY_TYPE_ID:
        return cf_array_to_list(raw)
    if type_id == BOOLEAN_TYPE_ID:
        return bool(CF.CFBooleanGetValue(raw))
    if type_id == NUMBER_TYPE_ID:
        number = ctypes.c_double()
        CF.CFNumberGetValue(raw, CF_NUMBER_DOUBLE, ctypes.byref(number))
        return number
    if type_id != CF.CFStringGetTypeID():
        # AXValue, AXURL, and other CF types are not strings.
        return f"<{type_id}>"
    return cf_string_value(raw)


def children(element):
    """Return the AX children of an element."""
    return list(attribute(element, "AXChildren") or [])


def title(element):
    return attribute(element, "AXTitle")


def child_named(element, name):
    for child in children(element):
        if title(child) == name:
            return child
    return None


def menu_bar(application):
    """Return the application menu bar element."""
    for child in children(application):
        if attribute(child, "AXRole") == "AXMenuBar":
            return child
    raise SystemExit("no AXMenuBar on the application")


def press(element, action="AXPress"):
    """Perform an AX action and wait for the UI to settle."""
    AS.AXUIElementPerformAction(element, cf_string(action))
    time.sleep(0.45)


def open_menu(item):
    """Open a menu bar item and return its menu element.

    A menu bar item publishes its menu as an `AXMenu` child once the item is
    pressed; the item itself never carries an `AXMenu` attribute.
    """
    press(item)
    time.sleep(0.3)
    for child in children(item):
        if attribute(child, "AXRole") == "AXMenu":
            return child
    return attribute(item, "AXMenu")


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


def walk_menus(application, path):
    """Open the menu bar item, walk to the submenu, and describe its items.

    A menu item publishes its submenu as an `AXMenu` child, so each step
    looks for that child rather than an `AXMenu` attribute on the item.
    """
    bar = child_named(menu_bar(application), path[0])
    if bar is None:
        raise SystemExit(f"menu bar item not found: {path[0]}")
    menu = open_menu(bar)
    if menu is None:
        raise SystemExit(f"no AXMenu on menu bar item: {path[0]}")
    for name in path[1:]:
        item = child_named(menu, name)
        if item is None:
            available = [title(child) for child in children(menu)]
            raise SystemExit(f"menu item not found: {name}; available: {available}")
        submenu = submenu_of(item)
        if submenu is None:
            press(item)
            time.sleep(0.3)
            submenu = submenu_of(item)
        if submenu is None:
            raise SystemExit(f"item has no submenu: {name}")
        menu = submenu
    out: list[str] = []
    for child in children(menu):
        describe(child, 0, out)
    return out


def submenu_of(item):
    """Return the `AXMenu` child of a menu item, if it has one."""
    for child in children(item):
        if attribute(child, "AXRole") == "AXMenu":
            return child
    return None


def find_window(application, name):
    """Return the AX element of a window by its name."""
    for window in children(application):
        if title(window) == name:
            return window
    available = [title(window) for window in children(application)]
    raise SystemExit(f"window not found: {name}; available: {available}")


def walk_tree(element, path, role_filter, out):
    """Record every element whose role matches the filter."""
    for index, child in enumerate(children(element)):
        role = attribute(child, "AXRole")
        if role_filter is None or (role and role_filter in role):
            out.append(
                f"{'.'.join(str(part) for part in path + [index])} {role} "
                f"title={attribute(child, 'AXTitle')!r} "
                f"value={attribute(child, 'AXValue')!r} "
                f"desc={attribute(child, 'AXDescription')!r}"
            )
        walk_tree(child, path + [index], role_filter, out)


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    pid = int(sys.argv[1])
    command = sys.argv[2]
    application = ctypes.c_void_p(AS.AXUIElementCreateApplication(pid))

    if command == "menus":
        print("\n".join(walk_menus(application, sys.argv[3:])))
        return 0

    if command == "tree":
        window = find_window(application, sys.argv[3])
        role_filter = sys.argv[4] if len(sys.argv) > 4 else None
        out: list[str] = []
        walk_tree(window, [], role_filter, out)
        print("\n".join(out))
        return 0

    raise SystemExit(f"unknown command: {command}")


if __name__ == "__main__":
    sys.exit(main())
