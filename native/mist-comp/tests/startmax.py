#!/usr/bin/env python3
# a window that asks to start maximized (argv[1] = max) or fullscreen (full) before its first frame
import sys, gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
w = Gtk.Window(title="mist-startmax"); w.set_default_size(400, 300); w.connect("destroy", Gtk.main_quit)
w.maximize() if sys.argv[1] == "max" else w.fullscreen()
w.show_all(); Gtk.main()
