#!/usr/bin/env python3
"""Toggle Outliner columns in a running Scrivener and report their state.

`View > Outliner Options > Columns` is the only place the full column
vocabulary is exposed. The AX API flattens the `Columns` submenu into the
`Outliner Options` menu, so the toggles appear as siblings of the disabled
`Columns` header.

Usage:
  ax_outliner_columns.py <pid> enable-all
  ax_outliner_columns.py <pid> list
"""

from __future__ import annotations

import ctypes
import sys
import time

sys.path.insert(0, __file__.rsplit("/", 1)[0])

import ax_probe as ax  # noqa: E402

# Menu entries that configure the title or keyword column rather than
# selecting a column of their own.
MODIFIERS = {"and Synopsis", "with Icons", "with Numbers", "as Color Chips"}


def outliner_menu(application):
    """Open `View > Outliner Options` and return its menu element."""
    bar = ax.child_named(ax.menu_bar(application), "View")
    menu = ax.open_menu(bar)
    options = ax.child_named(menu, "Outliner Options")
    submenu = ax.submenu_of(options)
    if submenu is None:
        raise SystemExit("Outliner Options has no submenu")
    return submenu


def column_items(submenu):
    """Return the column toggle items of the Outliner Options menu."""
    items = []
    for item in ax.children(submenu):
        name = ax.title(item)
        if not name or name == "Columns":
            continue
        if name in ("Center Content", "Use Fixed Row Height"):
            continue
        items.append((name, item))
    return items


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    pid = int(sys.argv[1])
    command = sys.argv[2]
    application = ctypes.c_void_p(ax.AS.AXUIElementCreateApplication(pid))

    if command == "list":
        for name, item in column_items(outliner_menu(application)):
            mark = ax.attribute(item, "AXMenuItemMarkChar")
            kind = "modifier" if name in MODIFIERS else "column"
            print(f"{kind:9} {name!r} visible={bool(mark)}")
        return 0

    if command == "enable-all":
        for name, item in column_items(outliner_menu(application)):
            if ax.attribute(item, "AXMenuItemMarkChar"):
                continue
            ax.press(item)
            time.sleep(0.2)
            # Re-open the menu: a press closes it.
            for _ in range(4):
                submenu = outliner_menu(application)
                match = dict(column_items(submenu)).get(name)
                if match is not None:
                    item = match
                    break
                time.sleep(0.2)
            mark = ax.attribute(item, "AXMenuItemMarkChar")
            print(f"toggled {name!r} -> visible={bool(mark)}")
        return 0

    raise SystemExit(f"unknown command: {command}")


if __name__ == "__main__":
    sys.exit(main())
