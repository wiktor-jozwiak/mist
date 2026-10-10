pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Bridge: the notification server is native now (native/mist-ui notif.rs, org.freedesktop.Notifications).
// The Notification Centre reads its history (~/.local/state/mist/notifications.json, written on every
// change) and Do Not Disturb from here; changes go back over `mist-ui-bin msg notif ...`.
Singleton {
    id: root
    property var history: []
    property bool dnd: false
    FileView {
        path: Quickshell.env("HOME") + "/.local/state/mist/notifications.json"
        watchChanges: true; printErrors: false
        onFileChanged: reload()
        onLoaded: { try { const d = JSON.parse(text()); root.history = d.entries || []; root.dnd = !!d.dnd } catch (e) {} }
    }
    function msg(args) { Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/mist-ui-bin", "msg"].concat(args)) }
    function setDnd(on) { dnd = on; msg(["notif", "dnd", on ? "true" : "false"]) }
    function clear() { history = []; msg(["notif", "clear"]) }
    function invoke(nid, key) { msg(["notif", "invoke", "" + nid, key || "default"]); return true }
}
