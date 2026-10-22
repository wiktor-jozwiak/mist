#!/usr/bin/env python3
# X11 window with its own frame: a left press asks the WM to resize it from the bottom-right corner
# (_NET_WM_MOVERESIZE), a middle press to move it
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk
w = Gtk.Window(title="mist-xresize"); w.set_decorated(False); w.set_default_size(400, 300)
w.connect("destroy", Gtk.main_quit)
def press(_w, ev):
    print("press", ev.button, flush=True)
    if ev.button == 1: w.begin_resize_drag(Gdk.WindowEdge.SOUTH_EAST, ev.button, int(ev.x_root), int(ev.y_root), ev.time)
    elif ev.button == 2: w.begin_move_drag(ev.button, int(ev.x_root), int(ev.y_root), ev.time)
    return True
w.add_events(Gdk.EventMask.BUTTON_PRESS_MASK); w.connect("button-press-event", press)
w.show_all(); Gtk.main()
