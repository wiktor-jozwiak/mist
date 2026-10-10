import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Wayland
import "components"

// Window snapping 1:1 like Windows 11.
// Dragging a window (title bar, CSD, Alt+drag):
//  - grabbing a maximized / snapped window restores its previous size under the cursor (hyprbars mistsnap.cpp)
//  - cursor at the top edge                -> full-screen preview, release = maximize
//  - cursor at the top edge in a corner     -> quarter
//  - cursor at the outer left / right edge  -> half; near a corner -> quarter
//  - window near the top                    -> Snap bar pill slides down at the top centre;
//                                              cursor onto it expands Snap Layouts (hover a zone, release)
// Snap Layouts flyout: hover the maximize (green) button of the title bar, or Win+Z.
// Snap Assist: after snapping into part of the screen, the rest shows the other windows; pick one to fill it.
// Events (socket2, hyprbars plugin): "mistdrag>>begin,<addr>,x,y,w,h", "mistdrag>>end,x,y",
// "mistsnap>>snapped,<addr>,<monitor>,fx,fy,fw,fh", "mistsnap>>maxhover,<addr>,x,y", "maxleave,<addr>", "layouts,<addr>,x,y".
// The cursor is polled (j/cursorpos) only while a drag is in progress. MistSnapApply (windows-like.lua) places windows.
Scope {
    id: snap
    readonly property string hyprSock: Quickshell.env("XDG_RUNTIME_DIR") + "/hypr/" + Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE") + "/.socket.sock"
    readonly property int barTop: 40          // Mist top bar height (reserved) when Hyprland does not report it
    readonly property int gap: 8
    readonly property int corner: 120         // corner zone along an edge -> quarter
    readonly property int pillW: 180
    readonly property int pillH: 26
    readonly property int pillShowY: 160      // window dragged this close to the top -> Snap bar appears

    readonly property var layouts: [
        [[0, 0, 0.5, 1], [0.5, 0, 0.5, 1]],
        [[0, 0, 0.667, 1], [0.667, 0, 0.333, 1]],
        [[0, 0, 0.333, 1], [0.333, 0, 0.334, 1], [0.667, 0, 0.333, 1]],
        [[0, 0, 0.5, 0.5], [0.5, 0, 0.5, 0.5], [0, 0.5, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]],
        [[0, 0, 0.5, 1], [0.5, 0, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5]],
        [[0, 0, 0.25, 1], [0.25, 0, 0.5, 1], [0.75, 0, 0.25, 1]],
        [[0, 0, 1, 1]]
    ]
    // every zone of every layout in reading order (keyboard navigation in the flyout)
    readonly property var flatZones: {
        const out = []
        layouts.forEach((l, li) => l.forEach(z => out.push({ zone: z, li: li })))
        return out
    }

    // ---------------------------------------------------------------- drag state
    property bool dragging: false
    property bool preview: false              // ipc snapPreview: layouts panel without tracking
    property string mode: ""                  // "", "layouts", "edge"
    property bool pill: false                 // Snap bar handle shown
    property var screenObj: null
    property real cx: -1
    property real cy: -1
    property var hot: null                    // zone under the cursor in the layouts panel
    property int hotLayout: -1
    property var target: null                 // edge zone (maximize / half / quarter)
    property string addr: ""                  // dragged window
    property var grab: null                   // its geometry when grabbed [x, y, w, h] = restore size after snapping
    property real startedAt: 0
    readonly property var zone: mode === "layouts" ? hot : mode === "edge" ? target : null

    // ---------------------------------------------------------------- Snap Assist state
    property var assistScreen: null
    property var assistZones: []              // zones still to fill, the first one shows the window picker
    property var assistGroup: []              // windows already placed in this snap group
    property var assistWins: []               // candidate HyprlandToplevels
    property int pendingLayout: -1            // layout of the next "snapped" event (-1 = edge / keyboard)
    property bool assistBusy: false           // a picked window is being placed
    readonly property var assistZone: assistZones.length && assistWins.length ? assistZones[0] : null

    // ---------------------------------------------------------------- Snap Layouts flyout state
    property string flyAddr: ""
    property var flyScreen: null
    property real flyX: 0
    property real flyY: 0
    property bool flyOpen: false
    property bool flyPinned: false            // Win+Z: stays until Esc / pick / click away
    property bool btnHover: false
    property int flyIndex: -1                 // index into flatZones (hover / keyboard)

    function norm(a) { return "0x" + String(a).replace(/^0x/, "") }
    function same(a, b) { return !!a && !!b && a.join() === b.join() }
    function screenAt(x, y) { return Quickshell.screens.find(s => x >= s.x && x < s.x + s.width && y >= s.y && y < s.y + s.height) || null }
    // reserved area of a monitor (left, top, right, bottom), same as workarea() in windows-like.lua
    function reservedOf(scr) {
        const m = scr ? Hyprland.monitorFor(scr) : null
        const r = m && m.lastIpcObject ? m.lastIpcObject.reserved : null
        return r && r.length === 4 ? r : [0, barTop, 0, 0]
    }
    // zone -> rectangle in screen-local coordinates
    function zoneRect(scr, z) {
        const r = reservedOf(scr)
        const ax = r[0] + gap, ay = r[1] + gap
        const aw = scr.width - r[0] - r[2] - 2 * gap, ah = scr.height - r[1] - r[3] - 2 * gap
        return { x: Math.floor(ax + z[0] * (aw + gap)), y: Math.floor(ay + z[1] * (ah + gap)),
                 w: Math.floor(z[2] * (aw + gap) - gap), h: Math.floor(z[3] * (ah + gap) - gap) }
    }

    function apply(a, z, li, g, pre) {
        pendingLayout = li
        hypr("eval " + (pre || "") + "MistSnapApply(\"" + norm(a) + "\", " + z.join(", ") + (g ? ", " + g.join(", ") : "") + ")")
    }
    // from a surface with keyboard focus (Snap Assist, Win+Z flyout): Hyprland only focuses the window once that
    // surface is gone, so hide it first and apply a moment later
    property var deferred: null
    function applyLater(a, z, li, pre) { deferred = [a, z, li, pre]; deferT.restart() }
    Timer {
        id: deferT; interval: 90
        onTriggered: {
            const d = snap.deferred
            snap.deferred = null
            if (d) snap.apply(d[0], d[1], d[2], null, d[3])
            snap.assistBusy = false
            if (snap.assistZones.length) { Hyprland.refreshToplevels(); assistT.restart() }
            else snap.endAssist()
        }
    }

    // ---------------------------------------------------------------- events (hyprbars plugin)
    Connections {
        target: Hyprland
        function onRawEvent(event) {
            if (event.name === "mistdrag") {
                const p = event.data.split(",")
                if (p[0] === "begin") snap.begin(p[1], p.slice(2, 6).map(Number))
                else if (p[0] === "end") snap.end(+p[1], +p[2])
            } else if (event.name === "mistsnap") {
                const p = event.data.split(",")
                if (p[0] === "snapped") snap.onSnapped(p[1], p[2], p.slice(3, 7).map(Number))
                else if (p[0] === "maxhover") snap.onMaxHover(p[1], +p[2], +p[3])
                else if (p[0] === "maxleave") { snap.btnHover = false; flyCloseT.restart() }
                else if (p[0] === "layouts") snap.openFly(p[1], +p[2], +p[3], true)
            }
        }
    }

    // ---------------------------------------------------------------- drag
    function begin(a, g) {
        closeFly(); endAssist()
        addr = norm(a); grab = g.length === 4 && g.every(v => !isNaN(v)) ? g : null
        mode = ""; hot = null; hotLayout = -1; target = null; pill = false
        startedAt = Date.now()
        dragging = true
        poll()
    }
    function end(x, y) {
        if (!dragging) return
        if (!isNaN(x) && !isNaN(y)) track(x, y)
        const z = zone
        if (z) apply(addr, z, mode === "layouts" ? hotLayout : -1, grab)
        stop()
    }
    function stop() { dragging = false; mode = ""; hot = null; hotLayout = -1; target = null; pill = false }

    property string buf: ""
    Socket {
        id: q
        path: snap.hyprSock
        onConnectedChanged: {
            if (connected) { write("j/cursorpos"); flush() }
            else snap.onReply(snap.buf)
        }
        parser: SplitParser { splitMarker: ""; onRead: d => snap.buf += d }
    }
    Socket { id: cmd; path: snap.hyprSock; property string pending: ""; onConnectedChanged: if (connected) { write(pending); flush() } }
    function hypr(text) { cmd.connected = false; cmd.pending = text; cmd.connected = true }
    function poll() { if (!q.connected) { buf = ""; q.connected = true } }

    Timer {
        interval: 16; repeat: true
        running: snap.dragging && !snap.preview
        onTriggered: {
            // safety net: a drag end that never arrived (plugin unloaded mid-drag...)
            if (Date.now() - snap.startedAt > 120000) { snap.stop(); return }
            snap.poll()
        }
    }

    function onReply(t) {
        const c = /"x":\s*(-?[\d.]+),\s*"y":\s*(-?[\d.]+)/.exec(t)
        if (c && dragging && !preview) track(+c[1], +c[2])
    }

    function track(x, y) {
        cx = x; cy = y
        const scr = screenAt(x, y)
        if (scr) screenObj = scr
        if (!screenObj) return
        const s = screenObj
        const others = Quickshell.screens.filter(o => o !== s)
        const spanY = o => o.y < s.y + s.height && o.y + o.height > s.y
        const spanX = o => o.x < s.x + s.width && o.x + o.width > s.x
        // edges shared with another monitor do not snap (the cursor crosses over), like Windows
        const outerLeft = !others.some(o => o.x + o.width === s.x && spanY(o))
        const outerRight = !others.some(o => o.x === s.x + s.width && spanY(o))
        const outerTop = !others.some(o => o.y + o.height === s.y && spanX(o))
        const midX = s.x + s.width / 2
        const atTop = outerTop && y <= s.y + 3
        const atLeft = outerLeft && x <= s.x + 3
        const atRight = outerRight && x >= s.x + s.width - 4
        // layouts panel grid origin (global)
        const gx = s.x + (s.width - panel.implicitWidth) / 2 + 18, gy = s.y + 6 + 18

        // Snap Layouts open: stay while the cursor is over the panel (with some slack)
        if (mode === "layouts") {
            const h = dragGrid.hitAt(x - gx, y - gy)
            hot = h ? h.zone : null; hotLayout = h ? h.li : -1
            if (y <= s.y + 6 + panel.implicitHeight + 40 && Math.abs(x - midX) <= panel.implicitWidth / 2 + 40) return
            mode = ""; hot = null; hotLayout = -1
        }

        pill = y < s.y + pillShowY && Math.abs(x - midX) < s.width / 2 - corner
        if (pill && y <= s.y + 6 + pillH + 6 && Math.abs(x - midX) <= pillW / 2 + 6) {
            mode = "layouts"; target = null
            const h = dragGrid.hitAt(x - gx, y - gy)
            hot = h ? h.zone : null; hotLayout = h ? h.li : -1
            return
        }

        const top = y < s.y + corner, bottom = y >= s.y + s.height - corner
        const left = x < s.x + corner, right = x >= s.x + s.width - corner
        let z = null
        if (atLeft) z = top ? [0, 0, 0.5, 0.5] : bottom ? [0, 0.5, 0.5, 0.5] : [0, 0, 0.5, 1]
        else if (atRight) z = top ? [0.5, 0, 0.5, 0.5] : bottom ? [0.5, 0.5, 0.5, 0.5] : [0.5, 0, 0.5, 1]
        else if (atTop) z = left && outerLeft ? [0, 0, 0.5, 0.5] : right && outerRight ? [0.5, 0, 0.5, 0.5] : [0, 0, 1, 1]
        target = z
        mode = z ? "edge" : ""
    }

    // ---------------------------------------------------------------- Snap Assist
    // the rest of the layout the window was snapped into: chosen layout, else halves / quarters
    function remaining(z, li) {
        let l = li >= 0 ? layouts[li] : null
        if (!l || !l.some(o => same(o, z))) {
            if (z[2] === 0.5 && z[3] === 0.5) l = layouts[3]
            else if (z[2] === 0.5 && z[3] === 1) l = layouts[0]
            else l = layouts.find(o => o.length > 1 && o.some(q => same(q, z)))
        }
        return l && l.some(o => same(o, z)) ? l.filter(o => !same(o, z)) : []
    }

    function onSnapped(a, monName, z) {
        const li = pendingLayout
        pendingLayout = -1
        a = norm(a)
        if (assistGroup.includes(a) && assistGroup[0] !== a) return   // a window picked in Snap Assist
        const scr = Quickshell.screens.find(s => s.name === monName)
        if (!scr || z.some(isNaN)) return
        const rest = remaining(z, li)
        if (!rest.length) { endAssist(); return }
        assistScreen = scr
        assistGroup = [a]
        assistZones = rest
        assistWins = []
        Hyprland.refreshToplevels(); Hyprland.refreshMonitors()
        assistT.restart()
    }
    function endAssist() { assistZones = []; assistWins = []; assistGroup = []; assistBusy = false }

    // pick a window for the current zone
    function assistPick(t) {
        const z = assistZones[0]
        if (!z || !t) return
        const a = norm(t.address)
        const o = t.lastIpcObject || {}
        const mon = Hyprland.monitorFor(assistScreen)
        // minimized (special:minimized) -> bring it back to this monitor's workspace first
        const pre = o.workspace && String(o.workspace.name).startsWith("special:") && mon && mon.activeWorkspace
            ? "MistRestore(\"" + a + "\", " + mon.activeWorkspace.id + "); " : ""
        assistGroup = assistGroup.concat([a])
        assistZones = assistZones.slice(1)
        assistWins = []
        assistBusy = true
        applyLater(a, z, -1, pre)
    }

    // after Hyprland reports fresh geometry: drop zones already filled, collect the candidates
    Timer {
        id: assistT
        interval: 160
        onTriggered: {
            const scr = snap.assistScreen
            if (!scr || !snap.assistZones.length || snap.assistBusy) return
            const tops = Hyprland.toplevels.values
            // workspaces on screen: each monitor's active one plus an open special workspace (e.g. "gaming")
            const shown = []
            for (const m of Hyprland.monitors.values) {
                if (m.activeWorkspace) shown.push(m.activeWorkspace.id)
                const sp = m.lastIpcObject ? m.lastIpcObject.specialWorkspace : null
                if (sp && sp.id) shown.push(sp.id)
            }
            const filled = z => {
                const r = snap.zoneRect(scr, z)
                return tops.some(t => {
                    const o = t.lastIpcObject
                    // at/size exclude the title bar drawn above the window: match left, right, bottom; top within a bar
                    if (!o || !o.at || !o.size) return false
                    const x = o.at[0] - scr.x, y = o.at[1] - scr.y
                    return Math.abs(x - r.x) <= 4 && Math.abs(x + o.size[0] - r.x - r.w) <= 4
                        && Math.abs(y + o.size[1] - r.y - r.h) <= 4 && y - r.y >= -4 && y - r.y <= 48
                })
            }
            const zs = snap.assistZones.filter(z => !filled(z))
            const wins = tops.filter(t => {
                const o = t.lastIpcObject
                if (!o || !o.workspace || !o.mapped && o.mapped !== undefined) return false
                if (snap.assistGroup.includes(snap.norm(t.address))) return false
                return shown.includes(o.workspace.id) || o.workspace.name === "special:minimized"
            })
            if (!zs.length || !wins.length) { snap.endAssist(); return }
            snap.assistZones = zs
            snap.assistWins = wins
        }
    }

    // ---------------------------------------------------------------- flyout (maximize-button hover, Win+Z)
    function onMaxHover(a, x, y) {
        btnHover = true
        if (dragging) return
        if (flyOpen && norm(a) === flyAddr) return
        flyAddr = norm(a); flyX = x; flyY = y
        flyHoverT.restart()
    }
    function openFly(a, x, y, pinned) {
        if (dragging) return
        flyAddr = norm(a); flyX = x; flyY = y
        flyScreen = screenAt(x, y) || Quickshell.screens[0]
        flyPinned = pinned
        flyIndex = pinned ? 0 : -1
        flyOpen = true
    }
    function closeFly() { flyOpen = false; flyPinned = false; flyIndex = -1; flyHoverT.stop() }
    function flyPick(i) {
        const f = flatZones[i]
        if (!f) return
        const a = flyAddr
        closeFly()
        applyLater(a, f.zone, f.li, "")
    }
    Timer { id: flyHoverT; interval: 420; onTriggered: if (snap.btnHover && !snap.dragging) snap.openFly(snap.flyAddr, snap.flyX, snap.flyY, false) }
    Timer {
        id: flyCloseT; interval: 320
        onTriggered: if (snap.flyOpen && !snap.flyPinned && !snap.btnHover && !flyHover.hovered) snap.closeFly()
    }

    // ---------------------------------------------------------------- snap preview + Snap bar handle
    PanelWindow {
        id: overlay
        visible: snap.dragging && !!snap.screenObj
        screen: snap.screenObj
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-snap"
        color: "transparent"
        mask: Region {}

        // cursor in overlay coordinates: the preview grows out of it, like Windows
        readonly property real px: snap.screenObj ? snap.cx - snap.screenObj.x : 0
        readonly property real py: snap.screenObj ? snap.cy - snap.screenObj.y : 0

        // translucent preview of where the window will land
        Rectangle {
            readonly property var area: snap.zone && snap.screenObj ? snap.zoneRect(snap.screenObj, snap.zone) : null
            opacity: area ? 1 : 0
            x: area ? area.x : overlay.px
            y: area ? area.y : overlay.py
            width: area ? area.w : 0
            height: area ? area.h : 0
            radius: 12
            color: Qt.rgba(169/255, 194/255, 238/255, 0.18)
            border.width: 1; border.color: Qt.rgba(169/255, 194/255, 238/255, 0.6)
            Behavior on x { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Behavior on y { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Behavior on width { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Behavior on height { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Behavior on opacity { NumberAnimation { duration: 140 } }
        }
        // Snap bar handle (top centre): slides down when the window nears the top
        Rectangle {
            readonly property bool shown: snap.pill && snap.mode !== "layouts"
            anchors.horizontalCenter: parent.horizontalCenter
            y: shown ? 6 : -height - 4
            opacity: shown ? 1 : 0
            width: snap.pillW; height: snap.pillH; radius: 13
            color: Qt.rgba(14/255, 19/255, 29/255, 0.88)
            border.width: 1; border.color: Theme.rim
            Behavior on y { NumberAnimation { duration: 200; easing.type: Easing.OutCubic } }
            Behavior on opacity { NumberAnimation { duration: 140 } }
            Row {
                anchors.centerIn: parent; spacing: 4
                Repeater {
                    model: 4
                    Rectangle { width: 26; height: 10; radius: 3; color: Qt.rgba(1, 1, 1, 0.22) }
                }
            }
        }
    }

    // ---------------------------------------------------------------- Snap Layouts panel (drag onto the pill)
    // mapped for the whole drag (no map latency), content fades / scales in from the pill
    PanelWindow {
        id: panel
        readonly property bool open: snap.mode === "layouts" || snap.preview
        visible: (snap.dragging || snap.preview) && !!snap.screenObj
        screen: snap.screenObj
        anchors.top: true
        margins.top: 6
        exclusionMode: ExclusionMode.Ignore
        implicitWidth: dragGrid.implicitWidth + 36; implicitHeight: dragGrid.implicitHeight + 36
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-popup"
        color: "transparent"
        mask: Region {}

        Glass {
            anchors.fill: parent; radius: 22; tint: 0.9; liquid: false
            opacity: panel.open ? 1 : 0
            scale: panel.open ? 1 : 0.86
            transformOrigin: Item.Top
            Behavior on opacity { NumberAnimation { duration: 140 } }
            Behavior on scale { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            SnapLayoutGrid {
                id: dragGrid
                x: 18; y: 18
                layouts: snap.layouts
                hot: snap.hot
                hotLayout: snap.hotLayout
            }
        }
    }

    // ---------------------------------------------------------------- Snap Layouts flyout (maximize button / Win+Z)
    // Win+Z: a click anywhere else closes it
    PanelWindow {
        visible: snap.flyOpen && snap.flyPinned && !!snap.flyScreen
        screen: snap.flyScreen
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "mist-snap-catcher"
        color: "transparent"
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onPressed: snap.closeFly() }
    }
    PanelWindow {
        id: fly
        visible: snap.flyOpen && !!snap.flyScreen
        screen: snap.flyScreen
        anchors { top: true; left: true }
        margins.left: snap.flyScreen ? Math.max(8, Math.min(snap.flyX - snap.flyScreen.x - 30, snap.flyScreen.width - implicitWidth - 8)) : 0
        margins.top: snap.flyScreen ? Math.max(8, Math.min(snap.flyY - snap.flyScreen.y + 8, snap.flyScreen.height - implicitHeight - 8)) : 0
        exclusionMode: ExclusionMode.Ignore
        implicitWidth: flyGrid.implicitWidth + 28; implicitHeight: flyGrid.implicitHeight + 28
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-popup"
        WlrLayershell.keyboardFocus: snap.flyPinned ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        color: "transparent"

        HoverHandler { id: flyHover; onHoveredChanged: if (!hovered) flyCloseT.restart() }
        Glass {
            id: flyBody
            anchors.fill: parent; radius: 18; tint: 0.9; liquid: false
            opacity: 0; scale: 0.9
            transformOrigin: Item.TopLeft
            states: State { when: fly.visible; PropertyChanges { flyBody.opacity: 1; flyBody.scale: 1 } }
            transitions: Transition { NumberAnimation { properties: "opacity,scale"; duration: 150; easing.type: Easing.OutCubic } }
            focus: true
            Keys.onPressed: e => {
                const n = snap.flatZones.length
                if (e.key === Qt.Key_Escape) snap.closeFly()
                else if (e.key === Qt.Key_Right || e.key === Qt.Key_Tab) snap.flyIndex = (snap.flyIndex + 1) % n
                else if (e.key === Qt.Key_Left || e.key === Qt.Key_Backtab) snap.flyIndex = (snap.flyIndex - 1 + n) % n
                else if (e.key === Qt.Key_Down || e.key === Qt.Key_Up) {
                    // jump to the first zone of the layout in the next / previous row
                    const li = snap.flatZones[Math.max(0, snap.flyIndex)].li
                    const to = Math.max(0, Math.min(snap.layouts.length - 1, li + (e.key === Qt.Key_Down ? 4 : -4)))
                    snap.flyIndex = snap.flatZones.findIndex(f => f.li === to)
                }
                else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) snap.flyPick(snap.flyIndex)
                else if (e.key >= Qt.Key_1 && e.key <= Qt.Key_7) snap.flyIndex = snap.flatZones.findIndex(f => f.li === e.key - Qt.Key_1)
                else return
                e.accepted = true
            }
            SnapLayoutGrid {
                id: flyGrid
                x: 14; y: 14
                layouts: snap.layouts
                interactive: true
                hot: snap.flyIndex >= 0 ? snap.flatZones[snap.flyIndex].zone : null
                hotLayout: snap.flyIndex >= 0 ? snap.flatZones[snap.flyIndex].li : -1
                onEntered: (z, li) => snap.flyIndex = snap.flatZones.findIndex(f => f.li === li && snap.same(f.zone, z))
                onPicked: (z, li) => snap.flyPick(snap.flatZones.findIndex(f => f.li === li && snap.same(f.zone, z)))
            }
        }
    }

    // ---------------------------------------------------------------- Snap Assist (window picker in the free zone)
    PanelWindow {
        id: assist
        visible: !!snap.assistZone && !!snap.assistScreen && !snap.dragging && !snap.assistBusy
        screen: snap.assistScreen
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-snap-assist"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
        color: "transparent"

        property int sel: 0
        onVisibleChanged: sel = 0
        readonly property var area: snap.assistZone && snap.assistScreen ? snap.zoneRect(snap.assistScreen, snap.assistZone) : null

        // a click outside the free zone dismisses Snap Assist (like Windows)
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onPressed: snap.endAssist() }

        Item {
            id: zoneBox
            visible: !!assist.area
            x: assist.area ? assist.area.x : 0
            y: assist.area ? assist.area.y : 0
            width: assist.area ? assist.area.w : 0
            height: assist.area ? assist.area.h : 0
            focus: true
            Keys.onPressed: e => {
                const n = snap.assistWins.length
                if (e.key === Qt.Key_Escape) snap.endAssist()
                else if (e.key === Qt.Key_Right || e.key === Qt.Key_Tab) assist.sel = (assist.sel + 1) % n
                else if (e.key === Qt.Key_Left || e.key === Qt.Key_Backtab) assist.sel = (assist.sel - 1 + n) % n
                else if (e.key === Qt.Key_Down) assist.sel = Math.min(n - 1, assist.sel + cards.cols)
                else if (e.key === Qt.Key_Up) assist.sel = Math.max(0, assist.sel - cards.cols)
                else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) snap.assistPick(snap.assistWins[assist.sel])
                else return
                e.accepted = true
            }

            Behavior on x { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Behavior on y { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Behavior on width { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            Behavior on height { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }

            Glass {
                anchors.fill: parent; radius: 12; tint: 0.55; liquid: false
                MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }   // inside the zone: keep it open
            }

            Flickable {
                id: flick
                anchors.fill: parent; anchors.margins: 20
                contentWidth: width; contentHeight: cards.height
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                interactive: contentHeight > height

                Flow {
                    id: cards
                    readonly property int cardW: Math.max(150, Math.min(280, (flick.width - (cols - 1) * spacing) / cols))
                    readonly property int cols: Math.max(1, Math.min(snap.assistWins.length, Math.floor((flick.width + spacing) / (220 + spacing))))
                    width: cols * cardW + (cols - 1) * spacing
                    x: Math.max(0, (flick.width - width) / 2)
                    y: Math.max(0, (flick.height - height) / 2)
                    spacing: 14

                    Repeater {
                        model: snap.assistWins
                        Rectangle {
                            id: card
                            required property var modelData
                            required property int index
                            readonly property var info: modelData.lastIpcObject || {}
                            readonly property bool sel: assist.sel === index
                            readonly property var entry: {
                                DesktopEntries.applications.values
                                const id = info.class || ""
                                return id ? DesktopEntries.byId(id) || DesktopEntries.heuristicLookup(id) : null
                            }
                            width: cards.cardW
                            height: 34 + thumb.height + 10
                            radius: 10
                            color: sel || ma.containsMouse ? Qt.rgba(1, 1, 1, 0.1) : Qt.rgba(1, 1, 1, 0.03)
                            border.width: sel ? 2 : 1
                            border.color: sel ? Theme.accent : Theme.hair
                            Behavior on color { ColorAnimation { duration: 90 } }

                            Row {
                                x: 10; y: 8; spacing: 8
                                width: parent.width - 20
                                Image {
                                    width: 18; height: 18; sourceSize: Qt.size(36, 36)
                                    source: Quickshell.iconPath(card.entry ? card.entry.icon : (card.info.class || ""), "application-x-executable")
                                }
                                Txt {
                                    width: parent.width - 26
                                    text: card.modelData.title || card.info.title || card.info.class || ""
                                    size: 12.5; w: 500
                                }
                            }
                            Rectangle {
                                id: thumb
                                x: 8; y: 34
                                width: parent.width - 16; height: Math.round(width * 0.6)
                                radius: 6
                                color: Qt.rgba(0, 0, 0, 0.25)
                                clip: true
                                ScreencopyView {
                                    id: shot
                                    anchors.centerIn: parent
                                    captureSource: card.modelData.wayland
                                    live: false
                                    constraintSize: Qt.size(thumb.width, thumb.height)
                                    width: implicitWidth; height: implicitHeight
                                }
                                Image {
                                    visible: !shot.hasContent
                                    anchors.centerIn: parent
                                    width: 48; height: 48; sourceSize: Qt.size(96, 96)
                                    source: Quickshell.iconPath(card.entry ? card.entry.icon : (card.info.class || ""), "application-x-executable")
                                }
                            }
                            MouseArea {
                                id: ma
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onEntered: assist.sel = card.index
                                onClicked: snap.assistPick(card.modelData)
                            }
                        }
                    }
                }
            }
        }
    }
}
