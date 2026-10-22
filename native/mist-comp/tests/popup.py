#!/usr/bin/env python3
# GTK3 window with a right-click menu (xdg_popup + grab); prints what happens to the menu
import sys, gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
def log(m): print(m, flush=True)
w = Gtk.Window(title="mist-popup"); w.set_default_size(500, 350); w.connect("destroy", Gtk.main_quit)
menu = Gtk.Menu()
for name in ("first", "second", "third"):
    it = Gtk.MenuItem(label=name); it.connect("activate", lambda _i, n=name: log("activate " + n)); menu.append(it)
menu.show_all()
menu.connect("show", lambda _m: log("menu shown"))
menu.connect("deactivate", lambda _m: log("menu closed"))
def press(_w, ev):
    if ev.button == 3: menu.popup_at_pointer(ev); return True
    log("window click %d" % ev.button); return False
w.add_events(1 << 8); w.connect("button-press-event", press)
w.show_all(); Gtk.main()
