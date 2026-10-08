"""Drives the client in the e2e display through XTest (see e2e.sh).

Commands, run in order (coordinates in window pixels):
  wait                  until the client's window is up
  click:X:Y             left click
  rclick:X:Y            right click
  drag:X1:Y1:X2:Y2      right-drag (sketches a line)
  key:NAME              press a key (X keysym name: 1, q, Escape, F2, Return)
  sleep:SECONDS
  shot:FILE             screenshot of the window, as PNG
"""
import os, subprocess, sys, time
from Xlib import X, XK, display
from Xlib.ext import xtest

def connect(name):
    # A display just started may take a moment to accept clients.
    for _ in range(100):
        try:
            return display.Display(name)
        except Exception:
            time.sleep(0.1)
    sys.exit(f'cannot connect to display {name}')


d = connect(os.environ.get('E2E_DISPLAY', ':77'))
root = d.screen().root


def find(window):
    for child in window.query_tree().children:
        name = child.get_wm_name()
        if name and 'Hibe' in name:
            return child
        found = find(child)
        if found:
            return found


def window():
    for _ in range(300):
        found = find(root)
        if found:
            return found
        time.sleep(0.1)
    sys.exit('the client window did not appear')


win = None


def at(x, y):
    origin = win.translate_coords(root, 0, 0)
    xtest.fake_input(d, X.MotionNotify, x=x - origin.x, y=y - origin.y)
    d.sync()
    time.sleep(0.05)


def button(number, down):
    xtest.fake_input(d, X.ButtonPress if down else X.ButtonRelease, number)
    d.sync()
    time.sleep(0.05)


for command in sys.argv[1:]:
    op, *args = command.split(':')
    if op != 'sleep' and win is None:
        win = window()
    if op == 'wait':
        time.sleep(1.5)  # the first frames
    elif op in ('click', 'rclick'):
        x, y = map(int, args)
        at(x, y)
        number = 1 if op == 'click' else 3
        button(number, True)
        button(number, False)
    elif op == 'drag':
        x1, y1, x2, y2 = map(int, args)
        at(x1, y1)
        button(3, True)
        for i in range(1, 9):
            at(x1 + (x2 - x1) * i // 8, y1 + (y2 - y1) * i // 8)
        button(3, False)
    elif op == 'key':
        win.set_input_focus(X.RevertToParent, X.CurrentTime)
        code = d.keysym_to_keycode(XK.string_to_keysym(args[0]))
        for kind in (X.KeyPress, X.KeyRelease):
            xtest.fake_input(d, kind, code)
            d.sync()
            time.sleep(0.05)
    elif op == 'sleep':
        time.sleep(float(args[0]))
    elif op == 'shot':
        subprocess.run(['import', '-display', d.get_display_name(), '-window',
                        str(win.id), args[0]], check=True)
    else:
        sys.exit(f'unknown command: {command}')
