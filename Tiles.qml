pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// App icons as the Windows Mist dock tiles (one iOS squircle per app): mistd tiles (scripts/mistd) renders
// ~/.cache/mist/tiles/<key>.png (the exact Windows tile from ~/.local/share/mist/wintiles when there is one).
// Shared by the dock and the Start menu; a missing tile is requested once and appears when it is ready.
Singleton {
    id: t
    readonly property string dir: Quickshell.env("HOME") + "/.cache/mist/tiles/"
    property var revs: ({})            // key -> generation count: only regenerated tiles reload
    property var pending: []
    property var running: []
    property var asked: ({})
    function key(name) { return (name || "").replace(/[^A-Za-z0-9._@-]/g, "_") }
    function url(name) { if (!name) return ""; const r = revs[key(name)]; return "file://" + dir + key(name) + ".png" + (r ? "?r=" + r : "") }
    function need(name) {
        if (!name || asked[name]) return
        asked[name] = true
        pending = pending.concat([name]); genT.restart()
    }
    property Timer genT: Timer {
        interval: 250
        onTriggered: if (!t.gen.running && t.pending.length) {
            t.gen.command = [Quickshell.shellDir + "/scripts/bin/mistd", "tiles"].concat(t.pending)
            t.running = t.pending; t.pending = []; t.gen.running = true
        }
    }
    property Process gen: Process {
        onExited: {
            const r = Object.assign({}, t.revs)
            for (const n of t.running) r[t.key(n)] = (r[t.key(n)] || 0) + 1
            t.revs = r; t.running = []
            if (t.pending.length) t.genT.restart()
        }
    }
}
