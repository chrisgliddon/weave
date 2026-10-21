#!/usr/bin/env python3
"""Post real mouse and keyboard events to the macOS window server.

Used to drive Scrivener interactions that the accessibility tree does not
expose as pressable actions (outline row selection, drag-and-drop gestures).

Usage:
  ax_click.py click X Y [--button 0] [--click-count 1]
  ax_click.py drag X1 Y1 X2 Y2 [--steps 24] [--hold 0.35]
  ax_click.py type "text"
  ax_click.py key <keycode> [modifier]
"""

from __future__ import annotations

import argparse
import sys
import time

import Quartz


def post_mouse(event_type, x, y, button=Quartz.kCGMouseButtonLeft, click_count=1):
    event = Quartz.CGEventCreateMouseEvent(None, event_type, (x, y), button)
    if click_count > 1:
        Quartz.CGEventSetIntegerValueField(
            event, Quartz.kCGMouseEventClickState, click_count
        )
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, event)


def click(x, y, button=0, click_count=1):
    mask = Quartz.kCGEventLeftMouseDown
    up = Quartz.kCGEventLeftMouseUp
    if button == 1:
        mask = Quartz.kCGEventRightMouseDown
        up = Quartz.kCGEventRightMouseUp
    elif button == 2:
        mask = Quartz.kCGEventOtherMouseDown
        up = Quartz.kCGEventOtherMouseUp
    post_mouse(mask, x, y, click_count=click_count)
    time.sleep(0.05)
    post_mouse(up, x, y, click_count=click_count)


def drag(x1, y1, x2, y2, steps=24, hold=0.35, pre_hold=0.25):
    """Press at (x1,y1), move in small increments to (x2,y2), then release."""
    post_mouse(Quartz.kCGEventLeftMouseDown, x1, y1)
    time.sleep(pre_hold)
    for step in range(1, steps + 1):
        ratio = step / steps
        # ease so the drag registers like a human drag
        eased = ratio * ratio * (3 - 2 * ratio)
        x = x1 + (x2 - x1) * eased
        y = y1 + (y2 - y1) * eased
        post_mouse(Quartz.kCGEventLeftMouseDragged, x, y)
        time.sleep(0.012)
    time.sleep(hold)
    post_mouse(Quartz.kCGEventLeftMouseUp, x2, y2)


def drag_hold(x1, y1, x2, y2, steps=24, pre_hold=0.25, hold=3.0):
    """Move to (x2,y2) but keep the button pressed, so a screenshot can be taken."""
    post_mouse(Quartz.kCGEventLeftMouseDown, x1, y1)
    time.sleep(pre_hold)
    for step in range(1, steps + 1):
        ratio = step / steps
        eased = ratio * ratio * (3 - 2 * ratio)
        x = x1 + (x2 - x1) * eased
        y = y1 + (y2 - y1) * eased
        post_mouse(Quartz.kCGEventLeftMouseDragged, x, y)
        time.sleep(0.012)
    time.sleep(hold)


def type_text(text: str):
    for char in text:
        code = Quartz.CGEventKeySetUnicodeString
        event = Quartz.CGEventCreateKeyboardEvent(None, 0, True)
        Quartz.CGEventSetIntegerValueField(
            event, Quartz.kCGEventKeyboardType, 0
        )
        data = char.encode("utf-16-le")
        Quartz.CGEventKeyboardSetUnicodeString(event, len(data), data)
        Quartz.CGEventPost(Quartz.kCGHIDEventTap, event)
        up = Quartz.CGEventCreateKeyboardEvent(None, 0, False)
        Quartz.CGEventPost(Quartz.kCGHIDEventTap, up)
        time.sleep(0.01)


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_click = sub.add_parser("click")
    p_click.add_argument("x", type=float)
    p_click.add_argument("y", type=float)
    p_click.add_argument("--button", type=int, default=0)
    p_click.add_argument("--click-count", type=int, default=1)

    p_drag = sub.add_parser("drag")
    p_drag.add_argument("x1", type=float)
    p_drag.add_argument("y1", type=float)
    p_drag.add_argument("x2", type=float)
    p_drag.add_argument("y2", type=float)
    p_drag.add_argument("--steps", type=int, default=24)
    p_drag.add_argument("--hold", type=float, default=0.35)

    p_drag_hold = sub.add_parser("drag-hold")
    p_drag_hold.add_argument("x1", type=float)
    p_drag_hold.add_argument("y1", type=float)
    p_drag_hold.add_argument("x2", type=float)
    p_drag_hold.add_argument("y2", type=float)
    p_drag_hold.add_argument("--steps", type=int, default=24)
    p_drag_hold.add_argument("--pre-hold", type=float, default=0.25)
    p_drag_hold.add_argument("--hold", type=float, default=3.0)

    p_release = sub.add_parser("release")
    p_release.add_argument("x", type=float)
    p_release.add_argument("y", type=float)

    p_type = sub.add_parser("type")
    p_type.add_argument("text")

    args = parser.parse_args()
    if args.cmd == "click":
        click(args.x, args.y, args.button, args.click_count)
    elif args.cmd == "drag":
        drag(args.x1, args.y1, args.x2, args.y2, args.steps, args.hold)
    elif args.cmd == "drag-hold":
        drag_hold(
            args.x1, args.y1, args.x2, args.y2, args.steps, args.pre_hold, args.hold
        )
    elif args.cmd == "release":
        post_mouse(Quartz.kCGEventLeftMouseUp, args.x, args.y)
    elif args.cmd == "type":
        type_text(args.text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
