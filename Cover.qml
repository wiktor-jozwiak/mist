pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Hyprland

// Which parts of each monitor are hidden behind windows, so animated surfaces under them (live wallpaper,
// the desktop sky) can slow down: nobody watches a scene through a maximised browser, but the blur of a
// translucent window still shows it, so they slow down instead of freezing.
// Window geometry comes from Hyprland's client list (lastIpcObject), refreshed after window / workspace
// events and every few seconds while windows are shown (moving or resizing a window sends no event).
Singleton {
    id: root
    // monitor name -> [[x, y, w, h], ...] of the windows shown on it (layout coordinates)
    property var wins: ({})

    // share 0..1 of the rect (layout coordinates) hidden behind windows of that monitor
    function frac(mon, x, y, w, h) {
        const r = wins[mon]
        if (!r || !r.length || w <= 0 || h <= 0) return 0
        // 32 x 18 samples: snapped halves and quarters count together
        let n = 0
        for (let i = 0; i < 32; i++) {
            const px = x + (i + 0.5) * w / 32
            for (let j = 0; j < 18; j++) {
                const py = y + (j + 0.5) * h / 18
                if (r.some(b => px >= b[0] && px < b[0] + b[2] && py >= b[1] && py < b[1] + b[3])) n++
            }
        }
        return n / 576
    }
    // gaps around a maximised window leave a thin frame: 85 % hidden counts as hidden
    function hidden(mon, x, y, w, h) { return frac(mon, x, y, w, h) >= 0.85 }
    // the same for a whole monitor, without the area its bars reserve
    function monHidden(scr) {
        const m = Hyprland.monitorFor(scr), r = m && m.lastIpcObject && m.lastIpcObject.reserved || [0, 0, 0, 0]
        return hidden(scr.name, scr.x + r[0], scr.y + r[1], scr.width - r[0] - r[2], scr.height - r[1] - r[3])
    }

    function calc() {
        const out = {}
        let any = false
        for (const m of Hyprland.monitors.values) {
            const mo = m.lastIpcObject || {}
            const ws = m.activeWorkspace ? m.activeWorkspace.name : ""
            const sp = mo.specialWorkspace && mo.specialWorkspace.name ? mo.specialWorkspace.name : "-"
            const r = []
            for (const t of Hyprland.toplevels.values) {
                const o = t.lastIpcObject
                if (!o || !o.at || !o.size || o.hidden || o.mapped === false || !o.workspace) continue
                if (o.workspace.name !== ws && o.workspace.name !== sp) continue
                r.push([o.at[0], o.at[1], o.size[0], o.size[1]])
            }
            out[m.name] = r
            if (r.length) any = true
        }
        if (JSON.stringify(out) !== JSON.stringify(wins)) wins = out
        pollT.running = any
    }
    function refresh() { Hyprland.refreshToplevels(); Hyprland.refreshMonitors(); calcT.restart() }

    Timer { id: calcT; interval: 80; onTriggered: root.calc() }
    Timer { id: evT; interval: 150; onTriggered: root.refresh() }
    Timer { id: pollT; interval: 3000; repeat: true; onTriggered: root.refresh() }
    Component.onCompleted: refresh()

    Connections {
        target: Hyprland
        function onRawEvent(e) {
            switch (e.name) {
            case "openwindow": case "closewindow": case "movewindowv2": case "changefloatingmode":
            case "fullscreen": case "workspacev2": case "focusedmonv2": case "activespecialv2":
            case "activewindowv2": case "moveworkspacev2": case "monitoraddedv2": case "monitorremovedv2":
                evT.restart(); break
            default:
                if (e.name.startsWith("mist")) evT.restart()    // window snapped / drag ended (mistsnap plugin)
            }
        }
    }
}
