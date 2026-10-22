#!/usr/bin/env python3
# a drag source with a solid red 40x40 drag icon (hotspot at its top-left corner)
import gi, cairo
gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk
w = Gtk.Window(title="mist-dnd"); w.set_default_size(500, 350); w.connect("destroy", Gtk.main_quit)
b = Gtk.EventBox(); b.add(Gtk.Label(label="drag me")); w.add(b)
b.drag_source_set(Gdk.ModifierType.BUTTON1_MASK, [Gtk.TargetEntry.new("text/plain", 0, 0)], Gdk.DragAction.COPY)
def begin(_w, ctx):
    s = cairo.ImageSurface(cairo.FORMAT_ARGB32, 40, 40); c = cairo.Context(s); c.set_source_rgb(1, 0, 0); c.paint()
    Gtk.drag_set_icon_surface(ctx, s); print("drag begin", flush=True)
b.connect("drag-begin", begin)
b.connect("drag-data-get", lambda _w, _c, data, _i, _t: data.set_text("mist", -1))
w.show_all(); Gtk.main()
