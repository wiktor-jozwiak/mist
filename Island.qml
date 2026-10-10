import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Widgets
import Quickshell.Services.Mpris
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
import Quickshell.Bluetooth
import "components"

// Dynamic Island, top centre, like the iPhone one: a solid black shape that springs between views:
//   idle    [art | date · time | live activities]; a calendar "live activity" when an event starts within 15 min.
//           Live activities: minutnik (ring + time left), Hyprland submap, Caps Lock, camera (green dot),
//           microphone (orange), screen sharing (purple), music (equalizer)
//   player  hover (when something plays): art, title, seekable progress, shuffle / prev / play / next / loop,
//           player switch when there is more than one
//   glance  hover when nothing plays: weather, next calendar events, minutnik
//   osd     volume / mute of the default output, microphone volume
//   notif   every desktop notification (eavesdropped from the session bus, Noctalia still owns the server):
//           progress hint, replaced / stacked updates, "+N" more waiting, hover = full text,
//           click = focus (or start) the app, right click = dismiss all. Noctalia's own toasts are off
//           (config.toml filter "island_only"), so this is the only popup. Phone (KDE Connect): app · device,
//           sender as title, the phone's app icon; hover = reply in place, the note's actions, dismiss on the phone
//   track   peek on track change and on play / pause from outside the island (media keys, other apps)
//   info    microphone mute / use, camera, screen sharing, screenshot, Bluetooth (adapter, devices, low battery),
//           internet, network, VPN, output device, keyboard layout, Caps / Num Lock, calendar (5 min before,
//           at the start), rain within the hour, charging / low battery, power profile, clipboard (text, link,
//           colour swatch, files, image thumbnail, password), USB devices, drives (click = open), monitors,
//           Hyprland (config reload + errors, urgent window -> click focuses it, scratchpad, pin, submap),
//           minutnik end, anything sent over IPC
// Transient events go through a small priority queue (same key = update in place). Wheel = volume,
// middle click = play/pause, left click on the date = calendar, click on a peek = its action / dismiss,
// right click = dismiss everything (idle: stop the minutnik). Hidden while a window is fullscreen (events still show).
// Test: mist-qs -p ~/.config/quickshell/mist ipc call island demo <kind>    (kinds: island demos)
//       mist-qs -p ~/.config/quickshell/mist ipc call island timer 300 "Herbata"   /  timerStop
//       mist-qs -p ~/.config/quickshell/mist ipc call island show bell "Tytuł" "podpis"
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    anchors.top: true
    exclusionMode: ExclusionMode.Ignore
    implicitWidth: 560; implicitHeight: 240
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-island"
    color: "transparent"
    mask: Region { item: win.hidden ? nothing : shell }
    Item { id: nothing; width: 0; height: 0 }

    // small text button of the notification view (phone actions)
    component ActBtn: Rectangle {
        id: pl
        property string text
        property string icon
        property bool primary: false
        signal clicked()
        implicitWidth: plRow.implicitWidth + 20; implicitHeight: 26; radius: 13
        color: primary ? (plm.containsMouse ? Qt.lighter(Theme.accent, 1.08) : Theme.accent) : plm.containsMouse ? Theme.fill2 : Theme.fill
        Behavior on color { ColorAnimation { duration: 120 } }
        Row {
            id: plRow; anchors.centerIn: parent; spacing: 5
            Icon { visible: !!pl.icon; name: pl.icon || "x"; size: 12; color: pl.primary ? Theme.ink : Theme.t2; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: pl.text; size: 11.5; w: 600; color: pl.primary ? Theme.ink : Theme.t1; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea { id: plm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: pl.clicked() }
    }
    // big round button of the call view (answer / reject)
    component RoundBtn: Rectangle {
        id: rb
        property string icon
        property color bg: Theme.green
        signal clicked()
        width: 46; height: 46; radius: 23
        color: rbm.containsMouse ? Qt.lighter(bg, 1.12) : bg
        scale: rbm.pressed ? 0.92 : 1
        Behavior on scale { NumberAnimation { duration: 90 } }
        Icon { anchors.centerIn: parent; name: rb.icon; size: 21; color: "#ffffff" }
        MouseArea { id: rbm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: rb.clicked() }
    }

    readonly property color orange: "#efb08a"
    readonly property color purple: "#bba9ee"

    // ------------------------------------------------------------ media
    property int pick: -1                                   // player chosen with the switch, -1 = auto
    readonly property var players: Mpris.players.values
    readonly property var player: {
        const ps = players
        if (pick >= 0 && pick < ps.length) return ps[pick]
        return ps.find(p => p.isPlaying) || ps.find(p => p.trackTitle) || null
    }
    readonly property bool playing: !!player && player.isPlaying
    readonly property string art: player ? player.trackArtUrl || "" : ""
    function fmt(s) { s = Math.max(0, Math.floor(s || 0)); return Math.floor(s / 60) + ":" + Data.two(s % 60) }
    function entryIcon(id, fallback) {
        DesktopEntries.applications.values             // re-evaluate once the entries are loaded
        const e = id ? DesktopEntries.byId(id) || DesktopEntries.heuristicLookup(id) : null
        return e ? Quickshell.iconPath(e.icon, fallback) : Quickshell.iconPath(fallback)
    }
    function nextPlayer() { const ps = players; if (ps.length > 1) pick = (Math.max(0, ps.indexOf(player)) + 1) % ps.length }
    // controls used in the island itself: no play / pause peek for them
    property bool selfMedia: false
    Timer { id: selfT; interval: 1200; onTriggered: win.selfMedia = false }
    function mine() { selfMedia = true; selfT.restart() }

    // ------------------------------------------------------------ event queue
    property bool armed: false                              // nothing fires for the state read at start-up
    Timer { running: true; interval: 3000; onTriggered: win.armed = true }

    property var ev: null
    property var queue: []
    property var lastEv: ({})                               // last event of each view (content while fading out)
    readonly property int waiting: queue.filter(q => q.view === "notif").length
    onEvChanged: if (ev) { const o = Object.assign({}, lastEv); o[ev.view] = ev; lastEv = o }
    function push(e) {
        e.prio = e.prio ?? 1; e.dur = e.dur ?? 3000
        if (replying && ev && !(e.key && ev.key === e.key)) { queue = queue.concat([e]).slice(-8); return }   // typing a reply: wait
        if (ev && e.key && ev.key === e.key) {              // update in place (volume, progress, layout...)
            ev = e; evT.interval = e.dur; evT.restart(); return
        }
        if (!ev || e.prio >= ev.prio) {
            if (ev && ev.view === "notif" && e.view !== "notif") queue = [ev].concat(queue.filter(q => q !== ev)).slice(0, 8)
            ev = e; evT.interval = e.dur; evT.restart()
            if (e.pulse) pulse.restart()
            return
        }
        queue = queue.filter(q => !e.key || q.key !== e.key).concat([e]).slice(-8)
    }
    function next() {
        replying = false
        if (queue.length) {
            const e = queue[0]; queue = queue.slice(1); ev = e
            evT.interval = e.view === "notif" ? Math.max(3000, Math.min(e.dur, 4500)) : Math.min(e.dur, 3000); evT.restart()
        } else ev = null
    }
    function clearAll() { replying = false; queue = []; ev = null }
    Timer { id: evT; onTriggered: (hover.hovered || win.replying) && win.ev && (win.ev.view === "notif" || win.ev.hold) ? restart() : win.next() }

    // click on a peek: run its action, then dismiss
    function act(e) {
        if (!e || !e.action) return
        const a = e.action
        if (a.nid && Notifs.invoke(a.nid, "default")) return
        if (a.focus) focusWin(a.focus)
        else if (a.open) Quickshell.execDetached(["xdg-open", a.open])
        else if (a.app) focusApp(a.app, a.entry)
        else if (a.popup) Ui.toggle(a.popup, win.modelData.name)
        else if (a.timerStop) timerStop()
        else if (a.phone) Phone[a.phone]()
    }
    // Hyprland runs a Lua config: dispatchers go through `hyprctl eval`
    function focusWin(addr) { Quickshell.execDetached(["hyprctl", "eval", "hl.dispatch(hl.dsp.focus({ window = \"address:0x" + addr.replace(/^0x/, "") + "\" }))"]) }
    // notification click: the app's window (by class), else start the app
    property var wantApp: null
    function focusApp(names, entryId) {
        wantApp = { names: names.filter(n => n).map(n => n.toLowerCase()), entry: entryId }
        Hyprland.refreshToplevels(); focusT.restart()
    }
    Timer {
        id: focusT; interval: 160
        onTriggered: {
            const w = win.wantApp
            if (!w) return
            const t = Hyprland.toplevels.values.find(t => {
                const c = ((t.lastIpcObject || {}).class || "").toLowerCase()
                return c && w.names.some(n => c === n || c.endsWith("." + n) || n.endsWith("." + c) || n.replace(/ /g, "-") === c)
            })
            if (t) win.focusWin(t.address)
            else if (w.entry) { const e = DesktopEntries.byId(w.entry); if (e) e.execute() }
            win.wantApp = null
        }
    }

    property bool expanded: false
    property bool glanceDemo: false
    // a ringing call takes the island over; a call in progress is its resting state (events still drop in)
    readonly property string mode: Phone.ringing ? "call" : expanded && player && !glanceDemo ? "player" : ev ? ev.view
        : Phone.call ? "call" : expanded ? "glance" : "idle"
    Timer { id: openT; interval: 220; onTriggered: if (hover.hovered && (win.mode === "idle" || win.mode === "track")) { if (win.player) win.ev = null; win.expanded = true } }
    Timer { id: closeT; interval: 320; onTriggered: win.expanded = false }
    Timer { interval: 1000; running: win.mode === "player" && win.playing; repeat: true; onTriggered: if (win.player) win.player.positionChanged() }

    // fullscreen window on this monitor: the idle pill slides away (events still drop in)
    readonly property var hmon: Hyprland.monitorFor(win.screen)
    // (only real fullscreen, mode 2: Win+Up maximises with mode 1 and keeps the island)
    readonly property bool fullscreen: {
        const ws = hmon ? hmon.activeWorkspace : null
        if (!ws || !ws.hasFullscreen) return false
        return ws.toplevels.values.some(t => (t.lastIpcObject || {}).fullscreen === 2)
    }
    readonly property bool hidden: fullscreen && mode === "idle"

    // ------------------------------------------------------------ volume / microphone
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var source: Pipewire.defaultAudioSource
    PwObjectTracker { objects: [win.sink, win.source] }
    property bool devArmed: false                           // a device switch reports its volume: skip that
    Timer { id: devArm; interval: 1500; onTriggered: win.devArmed = true }
    onSinkChanged: {
        if (armed && sink) push({ view: "info", key: "out", prio: 2, dur: 2600, icon: (sink.description || "").match(/head|słuch|buds|airpods/i) ? "headphones" : "volume", color: Theme.accent, title: sink.description || sink.nickname || sink.name, sub: "wyjście audio" })
        devArmed = false; devArm.restart()
    }
    onSourceChanged: {
        if (armed && source) push({ view: "info", key: "in", prio: 2, dur: 2600, icon: "mic", color: Theme.accent, title: source.description || source.nickname || source.name, sub: "wejście audio" })
        devArmed = false; devArm.restart()
    }
    property bool volRecent: false
    Timer { id: volT; interval: 1600; onTriggered: win.volRecent = false }
    function volOsd() {
        if (!armed || !devArmed) return
        volRecent = true; volT.restart()
        if (!expanded || !player) push({ view: "osd", key: "vol", prio: 3, dur: 1600, mic: false })
    }
    Connections {
        target: win.sink ? win.sink.audio : null
        function onVolumeChanged() { win.volOsd() }
        function onMutedChanged() { win.volOsd() }
    }
    Connections {
        target: win.source ? win.source.audio : null
        function onMutedChanged() {
            if (!win.armed || !win.devArmed) return
            const m = win.source.audio.muted
            win.push({ view: "info", key: "mic", prio: 3, dur: 2200, icon: m ? "micOff" : "mic", color: m ? Theme.danger : Theme.green, title: m ? "Mikrofon wyciszony" : "Mikrofon włączony", sub: "" })
        }
        // (not while an app records: WebRTC auto gain moves the capture volume all the time)
        function onVolumeChanged() {
            if (win.armed && win.devArmed && !win.micBusy) win.push({ view: "osd", key: "vol", prio: 3, dur: 1600, mic: true })
        }
    }
    function setVolume(d) {
        const a = sink && sink.audio
        if (!a) return
        if (a.muted && d > 0) a.muted = false
        a.volume = Math.max(0, Math.min(1, Math.round((a.volume + d) * 100) / 100))
    }

    // apps recording from a microphone: a link from a capture node into an application's input stream
    // (Quickshell reports playback streams as isSink, capture streams as !isSink; link state is not
    // exposed, so an open capture stream counts). Sink monitors (visualisers) and the Mist EQ chain do not.
    readonly property var micUsers: {
        const out = []
        for (const g of Pipewire.linkGroups.values) {
            const s = g.source, t = g.target
            if (!s || !t || !t.isStream || t.isSink || !t.audio || s.isSink || s.isStream) continue
            if ((t.name || "").startsWith("mist_eq")) continue
            const n = t.description || t.nickname || t.name
            if (out.indexOf(n) < 0) out.push(n)
        }
        return out
    }
    readonly property bool micBusy: micUsers.length > 0
    onMicBusyChanged: {
        if (armed) push({ view: "info", key: "busy", prio: 2, dur: 2600, icon: micBusy ? "mic" : "micOff", color: orange, title: micBusy ? "Mikrofon w użyciu" : "Mikrofon zwolniony", sub: micBusy ? micUsers.join(", ") : "" })
    }

    // ------------------------------------------------------------ Hyprland
    property string layout: ""
    property bool sharing: false
    property string submap: ""
    property real shotAt: 0                                 // a screencopy that ended quickly = screenshot
    Connections {
        target: Hyprland
        function onRawEvent(e) {
            const n = e.name, d = e.data
            if (n === "activelayout") {
                const l = d.split(",").slice(1).join(",")
                if (l === win.layout) return
                const first = win.layout === ""
                win.layout = l
                if (!first && win.armed) win.push({ view: "info", key: "kbd", prio: 2, dur: 1800, icon: "keyboard", color: Theme.accent, title: l, sub: "układ klawiatury" })
            } else if (n === "screencast") {
                // screenshots (grim, screencopy) fire this too: count it as sharing only after 1.5 s
                win.castOn = d.split(",")[0] === "1"
                if (win.castOn) shareT.restart()
                else if (win.sharing) { win.sharing = false; win.push({ view: "info", key: "share", prio: 2, dur: 2600, icon: "screenshot", color: win.purple, title: "Koniec udostępniania", sub: win.fmtDur((Date.now() - win.shareSince) / 1000) }) }
                else { shareT.stop(); win.shotAt = Date.now() }
            } else if (n === "fullscreen") {
                Hyprland.refreshToplevels()                 // lastIpcObject carries the fullscreen mode
            } else if (n === "submap") {
                win.submap = d
            } else if (n === "urgent") {
                if (!win.armed) return
                const t = Hyprland.toplevels.values.find(t => t.address.replace(/^0x/, "") === d.replace(/^0x/, ""))
                if (!t || (Hyprland.activeToplevel && Hyprland.activeToplevel.address === t.address)) return
                const cls = (t.lastIpcObject || {}).class || ""
                const ent = DesktopEntries.heuristicLookup(cls)
                win.push({ view: "info", key: "urg" + d, prio: 2, dur: 5000, img: win.entryIcon(cls, "application-x-executable"),
                           color: win.orange, title: ent ? ent.name : cls || "Okno", sub: "prosi o uwagę · kliknij", action: { focus: d.replace(/^0x/, "") }, hold: true })
            } else if (n === "activespecial") {
                if (!win.armed) return
                const ws = d.split(",")[0]
                if (ws) win.push({ view: "info", key: "special", prio: 1, dur: 1500, icon: "layers", color: win.purple, title: ws.replace(/^special:?/, "") || "Brudnopis", sub: "obszar specjalny" })
            } else if (n === "configreloaded") {
                if (win.armed) cfgErr.running = true
            } else if (n === "monitoraddedv2" || n === "monitorremovedv2" || n === "monitoradded" || n === "monitorremoved") {
                if (!win.armed) return
                const p = d.split(","), add = n.startsWith("monitoradded")
                win.push({ view: "info", key: "mon" + (p.length > 1 ? p[1] : p[0]), prio: 2, dur: 3000, icon: add ? "screenOn" : "screenOff", color: add ? Theme.accent : Theme.t3,
                           title: (p.length > 2 ? p.slice(2).join(",") : "") || (p.length > 1 ? p[1] : p[0]), sub: add ? "monitor podłączony" : "monitor odłączony" })
            } else if (n === "pin") {
                if (!win.armed) return
                const on = d.split(",")[1] === "1"
                win.push({ view: "info", key: "pin", prio: 1, dur: 1500, icon: "pinned", color: on ? Theme.accent : Theme.t3, title: on ? "Okno przypięte" : "Okno odpięte", sub: "" })
            }
        }
    }
    property bool castOn: false
    property real shareSince: 0
    Timer {
        id: shareT; interval: 1500
        onTriggered: if (win.castOn && !win.sharing) { win.sharing = true; win.shareSince = Date.now(); win.push({ view: "info", key: "share", prio: 2, dur: 2600, icon: "screenshot", color: win.purple, title: "Udostępniasz ekran", sub: "" }) }
    }
    function fmtDur(s) { s = Math.round(s); return s < 60 ? s + " s" : s < 3600 ? Math.floor(s / 60) + " min " + (s % 60) + " s" : Math.floor(s / 3600) + " h " + Math.floor(s % 3600 / 60) + " min" }
    Process {
        id: cfgErr
        command: ["hyprctl", "configerrors", "-j"]
        stdout: StdioCollector {
            onStreamFinished: {
                let errs = []
                try { errs = JSON.parse(this.text).filter(x => x && x.trim()) } catch (e) {}
                if (errs.length) win.push({ view: "info", key: "cfg", prio: 3, dur: 7000, icon: "alert", color: Theme.danger, title: "Błędy konfiguracji Hyprland (" + errs.length + ")", sub: errs[0].replace(/\s+/g, " "), hold: true })
                else win.push({ view: "info", key: "cfg", prio: 1, dur: 1800, icon: "refresh", color: Theme.green, title: "Konfiguracja przeładowana", sub: "Hyprland" })
            }
        }
    }

    // ------------------------------------------------------------ Bluetooth
    property bool btWas: !!Bluetooth.defaultAdapter && Bluetooth.defaultAdapter.enabled
    Connections {
        target: Bluetooth.defaultAdapter
        function onEnabledChanged() {
            const on = Bluetooth.defaultAdapter.enabled
            if (win.armed && on !== win.btWas) win.push({ view: "info", key: "btA", prio: 2, dur: 2000, icon: on ? "bluetooth" : "btOff", color: on ? Theme.accent : Theme.t3, title: on ? "Bluetooth włączony" : "Bluetooth wyłączony", sub: "" })
            win.btWas = on
        }
    }
    property var btLow: ({})
    Variants {
        model: Bluetooth.devices.values
        delegate: Connections {
            required property var modelData
            target: modelData
            function onConnectedChanged() {
                if (!win.armed) return
                const d = modelData
                win.push({ view: "info", key: "bt" + d.address, prio: 2, dur: 3000, icon: d.connected ? "btOn" : "btOff",
                           color: d.connected ? Theme.accent : Theme.t3, title: d.name || d.deviceName,
                           sub: d.connected ? (d.batteryAvailable ? "połączono · " + Math.round(d.battery * 100) + "%" : "połączono") : "rozłączono" })
            }
            function onBatteryChanged() {
                const d = modelData
                if (!d.batteryAvailable || !d.connected) return
                const b = d.battery
                if (b > 0.3) { if (win.btLow[d.address]) { const o = Object.assign({}, win.btLow); delete o[d.address]; win.btLow = o }; return }
                const lvl = b <= 0.1 ? 2 : b <= 0.2 ? 1 : 0
                if (!win.armed || lvl === 0 || (win.btLow[d.address] || 0) >= lvl) return
                const o = Object.assign({}, win.btLow); o[d.address] = lvl; win.btLow = o
                win.push({ view: "info", key: "btLow" + d.address, prio: 3, dur: 5000, icon: "battLow", color: lvl === 2 ? Theme.danger : win.orange,
                           title: d.name || d.deviceName, sub: "słaba bateria · " + Math.round(b * 100) + "%" })
            }
        }
    }

    // ------------------------------------------------------------ phone (KDE Connect, see Phone.qml)
    // its notifications, calls and SMS come as desktop notifications (notifywatch); here: link, pairing, charging, photo
    function phoneEvent(o) {
        const n = o.name || Phone.name || "Telefon"
        const bat = Phone.battery >= 0 ? " · " + Phone.battery + "%" : ""
        const e = {
            conn:     { prio: 2, dur: 2600, icon: "mobile", color: Theme.accent, title: n, sub: "połączono" + bat, action: { popup: "start" } },
            disc:     { prio: 1, dur: 2200, icon: "mobileOff", color: Theme.t3, title: n, sub: "rozłączono" },
            pairreq:  { prio: 3, dur: 15000, hold: true, pulse: true, icon: "mobileShare", color: Theme.yellow, title: n + " chce się sparować", sub: "klucz " + (o.key || "") + " · kliknij, aby zaakceptować", action: { phone: "accept" } },
            paired:   { prio: 2, dur: 3000, icon: "check", color: Theme.green, title: n, sub: "sparowano · telefon i komputer połączone" },
            unpaired: { prio: 2, dur: 2600, icon: "unlink", color: Theme.t3, title: n, sub: "rozparowano" },
            plug:     { prio: 1, dur: 2200, icon: "charging", color: Theme.green, title: n, sub: "ładowanie · " + (o.bat ?? Phone.battery) + "%" },
            full:     { prio: 1, dur: 3000, icon: "battery", color: Theme.green, title: n, sub: "naładowany · 100%" },
            photo:    { prio: 2, dur: 4500, icon: "camera", color: Theme.accent, title: "Zdjęcie z telefonu", sub: "kliknij, aby otworzyć", img: "file://" + o.path, photo: true, action: { open: o.path } },
            shot:     { prio: 2, dur: 4000, icon: "screenshot", color: Theme.accent, title: "Zrzut ekranu telefonu", sub: "w schowku · kliknij, aby otworzyć", img: "file://" + o.path, photo: true, action: { open: o.path } },
            done:     { prio: 1, dur: 2400, icon: o.icon || "check", color: Theme.green, title: n, sub: o.text || "" },
            err:      { prio: 2, dur: 3000, icon: "alert", color: win.orange, title: n, sub: o.text || "błąd" }
        }[o.k]
        if (o.k === "call") {
            const who = o.who || o.number || "Nieznany numer"
            callAt = Date.now()                             // kdeconnectd's own notification of it is dropped (notify)
            // ringing / talking: the call view (Phone.call); here only a missed call
            if (o.what === "missedCall") win.push({ view: "info", key: "phoneCall", prio: 3, dur: 9000, hold: true, icon: "call", color: Theme.danger, title: who, sub: "nieodebrane połączenie" })
            return
        }
        if (e) win.push(Object.assign({ view: "info", key: "phone" + o.k }, e))
    }
    property real callAt: 0
    // calls (Phone.call: Bluetooth hands-free, or a ringing call KDE Connect reports): the "call" view
    property var lastCall: null                         // content while the view fades out
    property bool keypad: false
    function letter(ch) { return !!ch && ch.toLowerCase() !== ch.toUpperCase() }
    property real callNow: Date.now() / 1000
    Timer { interval: 1000; repeat: true; triggeredOnStart: true; running: !!Phone.call && Phone.call.state === "active"; onTriggered: win.callNow = Date.now() / 1000 }
    Connections {
        target: Phone
        function onCallChanged() {
            const c = Phone.call, p = win.lastCall
            if (p && p.state === "active" && p.since && (!c || c.path !== p.path))
                win.push({ view: "info", key: "callEnd", prio: 2, dur: 3200, icon: "callEnd", color: Theme.t2, title: p.who, sub: "rozmowa zakończona · " + win.fmt(Date.now() / 1000 - p.since) })
            if (c) win.lastCall = c
            else win.keypad = false
            if (c && (c.state === "incoming" || c.state === "waiting") && !(p && p.path === c.path && p.state === c.state)) pulse.restart()
        }
    }
    Connections { target: Phone; function onEvent(o) { if (win.armed) win.phoneEvent(o) } }

    // ------------------------------------------------------------ network (NetworkManager, C locale: the lines are matched)
    property string conn: "full"
    property string primary: ""
    function connectivity(c) {
        const prev = conn
        conn = c
        if (!armed || prev === c) return
        if (c === "full") push({ view: "info", key: "net", prio: 2, dur: 2600, icon: "worldOk", color: Theme.green, title: "Połączono z internetem", sub: primary })
        else if (prev === "full") push({ view: "info", key: "net", prio: 2, dur: 4000, icon: "worldOff", color: Theme.danger, title: "Brak internetu", sub: c === "limited" ? "ograniczone" : c === "portal" ? "wymaga logowania (portal)" : "" })
    }
    function nmLine(l) {
        let m = /Connectivity is now '(\w+)'/.exec(l)
        if (m) { connectivity(m[1]); return }
        m = /^'(.+)' is now the primary connection/.exec(l)
        if (m) {
            const was = primary
            primary = m[1]
            if (armed && was && was !== m[1] && conn === "full") push({ view: "info", key: "net", prio: 1, dur: 2400, icon: "network", color: Theme.accent, title: m[1], sub: "główne połączenie" })
            return
        }
        m = /^((?:wg|tun|tap|vpn|proton|nord|mullvad)[\w.-]*): (connected|disconnected|unavailable|unmanaged)/i.exec(l)
        if (m && armed) {
            const on = m[2] === "connected"
            push({ view: "info", key: "vpn" + m[1], prio: 2, dur: 2600, icon: "shield", color: on ? Theme.green : Theme.t3, title: on ? "VPN połączony" : "VPN rozłączony", sub: m[1] })
        }
    }
    Process {
        id: nm; running: true
        environment: ({ LC_ALL: "C", LANG: "C" })
        // setpriv --pdeathsig: the kernel stops the helper if Quickshell dies (a crash skips onDestruction)
        command: ["setpriv", "--pdeathsig", "TERM", "nmcli", "monitor"]
        stdout: SplitParser { onRead: l => win.nmLine(l) }
        onExited: nmRestart.restart()
    }
    Timer { id: nmRestart; interval: 15000; onTriggered: nm.running = true }

    // ------------------------------------------------------------ power: charging, low battery (laptops), power profile
    Connections {
        target: UPower.displayDevice
        function onStateChanged() {
            const d = UPower.displayDevice
            if (!win.armed || !d.isLaptopBattery) return
            if (d.state === UPowerDeviceState.FullyCharged) { win.push({ view: "info", key: "pwr", prio: 2, dur: 3000, icon: "battery", color: Theme.green, title: "Naładowano", sub: "100%" }); return }
            const c = d.state === UPowerDeviceState.Charging
            win.push({ view: "info", key: "pwr", prio: 2, dur: 2600, icon: c ? "charging" : "battery", color: c ? Theme.green : Theme.t2, title: c ? "Ładowanie" : "Na baterii", sub: Math.round(d.percentage * 100) + "%" })
        }
        function onPercentageChanged() {
            const d = UPower.displayDevice
            if (!win.armed || !d.isLaptopBattery || d.state === UPowerDeviceState.Charging) return
            const p = Math.round(d.percentage * 100)
            if (p === 20 || p === 10 || p === 5) win.push({ view: "info", key: "pwrLow", prio: 3, dur: 6000, icon: "battLow", color: p > 10 ? win.orange : Theme.danger, title: "Słaba bateria", sub: p + "% · podłącz zasilacz", pulse: p <= 10 })
        }
    }
    Connections {
        target: PowerProfiles
        function onProfileChanged() {
            if (!win.armed) return
            const p = PowerProfiles.profile
            const v = p === PowerProfile.Performance ? ["rocket", win.orange, "Wydajność"] : p === PowerProfile.PowerSaver ? ["leaf", Theme.green, "Oszczędzanie energii"] : ["gauge", Theme.accent, "Zrównoważony"]
            win.push({ view: "info", key: "profile", prio: 2, dur: 2000, icon: v[0], color: v[1], title: v[2], sub: "profil zasilania" })
        }
    }

    // ------------------------------------------------------------ notifications
    // from Notifs (Mist's notification server) as fields:
    //   app, icon, summary, body, urgency, desktop-entry, replaces-id, progress, sync-key, category, notification id
    property var lastNote: ["", 0]
    function notify(d) {
        if (d.length < 6) return
        const strip = s => s.replace(/<[^>]*>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&#39;|&apos;/g, "'").replace(/&amp;/g, "&").trim()
        const title = strip(d[2]), body = strip(d[3])
        if (!title && !body) return
        const de = d[5]
        const ent = DesktopEntries.byId(de || "") || DesktopEntries.heuristicLookup(de || d[0] || "")
        let img = d[1]
        if (img.startsWith("/")) img = "file://" + img
        else if (img.startsWith("image:") || img.startsWith("data:")) {}
        else if (img && !img.startsWith("file://")) img = Quickshell.iconPath(img, true)
        const urgent = d[4] === "2"
        const rid = +(d[6] || 0), value = d.length > 7 ? +d[7] : -1, sync = d[8] || "", app = ent ? ent.name : (d[0] || "Powiadomienie")
        // the same text twice in 2 s (apps that notify twice): once
        if (Date.now() - callAt < 5000 && /kdeconnect|kde connect/i.test(d[0] + " " + de)) return
        const sig = app + "|" + title + "|" + body
        if (lastNote[0] === sig && Date.now() - lastNote[1] < 2000 && value < 0) return
        lastNote = [sig, Date.now()]
        if (/kdeconnect/i.test(d[0] + " " + de)) { phoneNotify(title, body, img, urgent); return }
        // updates of one notification replace it in place: sync hint, replaces-id of the one on screen, progress
        let key = sync ? "s:" + app + sync : value >= 0 ? "v:" + app + title : ""
        if (!key && rid > 0 && ev && ev.view === "notif" && ev.app === app) key = ev.key || ""
        if (!key && rid > 0) key = "r:" + app + rid
        push({ view: "notif", key: key || undefined, prio: urgent ? 3 : 2, dur: urgent ? 9000 : value >= 0 ? 6000 : 4500, app: app,
               icon: img || (ent ? Quickshell.iconPath(ent.icon, true) : "") || Quickshell.iconPath("dialog-information"),
               title: sharing ? app : title || body, body: sharing ? "treść ukryta · udostępniasz ekran" : title ? body : "", urgent: urgent, value: value, at: Date.now(), pulse: urgent,
               action: { nid: +(d[10] || 0), app: [de, d[0], ent ? ent.id : "", ent ? ent.startupClass : "", app], entry: ent ? ent.id : "" } })
    }
    // KDE Connect: a phone notification comes as summary = phone app, body = "sender: text"; kdeconnectd's own
    // messages (errors, sshfs, permissions) have the device name as summary and become one info line.
    // The phone note itself (icon, reply, actions, dismiss) is looked up in Phone.notes while it is shown:
    // phonewatch reads it a moment after the desktop notification arrives.
    function phoneNotify(title, body, img, urgent) {
        const dev = Phone.name || ""
        if (!body || dev && title === dev) {
            push({ view: "info", key: "phoneMsg", prio: urgent ? 3 : 2, dur: 4000, icon: "mobile", color: urgent ? Theme.danger : Theme.accent,
                   title: dev || title, sub: body || title, action: { popup: "phone" } })
            return
        }
        const m = /^([^:]{1,60}):\s+([\s\S]+)$/.exec(body)
        const who = m ? m[1] : "", text = m ? m[2] : body
        push({ view: "notif", key: "ph:" + title + "|" + who, prio: urgent ? 3 : 2, dur: urgent ? 9000 : 7000,
               app: title + (dev ? " · " + dev : ""), phone: { app: title, raw: body },
               icon: Quickshell.iconPath(title.toLowerCase().replace(/ /g, "-"), true) || img || Quickshell.iconPath("smartphone", true),
               title: sharing ? title : who || title, body: sharing ? "treść ukryta · udostępniasz ekran" : who ? text : body,
               urgent: urgent, value: -1, at: Date.now(), pulse: urgent, action: { popup: "phone" } })
    }
    function phoneNote(p) {
        if (!p) return null
        return Phone.notes.find(n => n.app === p.app && p.raw.indexOf((n.text || n.title).slice(0, 48)) >= 0) || null
    }
    // reply typed in the island: the layer takes the keyboard only meanwhile, the notification stays
    property bool replying: false
    WlrLayershell.keyboardFocus: replying ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
    function phoneDone(msg) {
        replying = false
        if (!msg) { next(); return }
        ev = null
        push({ view: "info", key: "phoneMsg", prio: 2, dur: 2000, icon: "check", color: Theme.green, title: Phone.name || "Telefon", sub: msg })
    }
    Connections {
        target: Notifs
        function onArrived(o) {
            const one = t => ("" + t).replace(/[\t\n\r]+/g, " ").slice(0, 300)
            win.notify([one(o.app), o.icon, one(o.summary), one(o.body), "" + o.urgency, o.entry, "" + o.rid, "" + o.value, o.sync, o.category, "" + o.nid])
        }
        // pictures sent as raw data live only while the notification does: keep a PNG for the history
        function onSaveImage(url, file) { imgSaver.queue = imgSaver.queue.concat([[url, file]]); imgSaver.nextOne() }
    }
    Item {
        id: imgSaver
        property var queue: []
        property var cur: null
        opacity: 0
        width: 1; height: 1
        function nextOne() {
            if (cur || !queue.length) return
            cur = queue[0]; queue = queue.slice(1)
            shot.source = cur[0]
        }
        Image {
            id: shot
            asynchronous: true
            cache: false
            onStatusChanged: {
                if (status === Image.Ready) {
                    const f = imgSaver.cur[1], w = implicitWidth
                    width = implicitWidth; height = implicitHeight
                    shot.grabToImage(r => { r.saveToFile(f); Notifs.imageSaved(f, w); imgSaver.cur = null; shot.source = ""; imgSaver.nextOne() })
                } else if (status === Image.Error) { imgSaver.cur = null; imgSaver.nextOne() }
            }
        }
    }

    // ------------------------------------------------------------ clipboard, USB, drives, camera, Caps / Num Lock
    // one JSON object per line from mistd island
    property bool camBusy: false
    property var camApps: []
    property bool caps: false
    property bool num: true
    function sysEvent(line) {
        let o
        try { o = JSON.parse(line) } catch (e) { return }
        if (o.t === "lock") {
            const c = o.caps !== caps, n = o.num !== num
            caps = o.caps; num = o.num
            if (o.init || !armed) return
            if (c) push({ view: "info", key: "lock", prio: 2, dur: 1400, icon: "capsLock", color: caps ? Theme.accent : Theme.t3, title: caps ? "Caps Lock włączony" : "Caps Lock wyłączony", sub: "" })
            else if (n) push({ view: "info", key: "lock", prio: 2, dur: 1400, icon: "numbers", color: num ? Theme.accent : Theme.t3, title: num ? "Num Lock włączony" : "Num Lock wyłączony", sub: "" })
        } else if (o.t === "cam") {
            camBusy = o.on; camApps = o.apps || []
            if (armed) push({ view: "info", key: "cam", prio: 2, dur: 2600, icon: o.on ? "video" : "videoOff", color: o.on ? Theme.green : Theme.t3, title: o.on ? "Kamera w użyciu" : "Kamera wyłączona", sub: o.on ? camApps.join(", ") : "" })
        } else if (!armed) {
            return
        } else if (o.t === "clip") {
            const shot = o.kind === "image" && Date.now() - shotAt < 4000
            const priv = sharing && (o.kind === "text" || o.kind === "link" || o.kind === "files")
            const k = {
                text:   ["copy", "Skopiowano", "„" + o.text + "”"],
                link:   ["link", "Skopiowano link", o.text],
                color:  ["palette", "Skopiowano kolor", (o.text || "").toUpperCase()],
                files:  ["files", o.n === 1 ? "Skopiowano plik" : "Skopiowano pliki (" + o.n + ")", o.text],
                image:  ["photo", shot ? "Zrzut ekranu w schowku" : "Skopiowano obraz", Math.max(1, Math.round((o.size || 0) / 1024)) + " KB"],
                secret: ["lock", "Skopiowano hasło", "ukryte · schowek"]
            }[o.kind]
            if (!k) return
            push({ view: "info", key: "clip", prio: 1, dur: o.kind === "image" ? 2800 : 2000, icon: k[0], color: o.kind === "secret" ? win.orange : Theme.accent, title: k[1],
                   sub: priv ? "treść ukryta · udostępniasz ekran" : k[2], img: o.kind === "image" && !sharing ? "file://" + o.path : "", photo: o.kind === "image" && !sharing,
                   swatch: o.kind === "color" ? o.text : "" })
        } else if (o.t === "usb") {
            const ic = { keyboard: "keyboard", mouse: "mouse", storage: "usb", audio: "headset", video: "camera", phone: "mobile", pad: "gamepad", printer: "printer", net: "network" }[o.kind] || "usb"
            push({ view: "info", key: "usb" + o.name, prio: 1, dur: 2400, icon: ic, color: o.on ? Theme.accent : Theme.t3, title: o.name, sub: o.on ? "podłączono" : "odłączono" })
        } else if (o.t === "mount") {
            if (o.on) push({ view: "info", key: "mnt" + o.path, prio: 2, dur: 4500, icon: "drive", color: Theme.accent, title: o.name, sub: "zamontowano" + (o.free >= 0 ? " · wolne " + win.gb(o.free) : "") + " · otwórz", action: { open: o.path } })
            else push({ view: "info", key: "mnt" + o.path, prio: 2, dur: 2600, icon: "eject", color: Theme.green, title: o.name, sub: "odmontowano · można odłączyć" })
        }
    }
    function gb(b) { return b >= 1e9 ? (b / 1e9).toFixed(1).replace(".", ",") + " GB" : Math.round(b / 1e6) + " MB" }
    Process {
        id: sysw; running: true
        command: ["setpriv", "--pdeathsig", "TERM", Quickshell.shellDir + "/scripts/bin/mistd", "island"]
        stdout: SplitParser { onRead: l => win.sysEvent(l) }
        onExited: syswRestart.restart()
    }
    Timer { id: syswRestart; interval: 5000; onTriggered: sysw.running = true }
    // a hot reload left busctl / nmcli orphaned (one pair per reload): stop them explicitly
    Component.onDestruction: { nmRestart.stop(); syswRestart.stop(); nm.running = false; sysw.running = false }

    // ------------------------------------------------------------ track change, play / pause from outside
    property string lastTrack: ""
    Connections {
        target: win.player
        function onTrackTitleChanged() { trackT.restart() }
        function onIsPlayingChanged() { stateT.restart() }
    }
    Timer {
        id: trackT; interval: 450
        onTriggered: {
            const p = win.player
            if (!p || !p.trackTitle) return
            const k = p.trackTitle + "|" + p.trackArtist
            if (k === win.lastTrack) return
            win.lastTrack = k
            if (win.armed && p.isPlaying && !win.expanded) win.push({ view: "track", key: "track", prio: 1, dur: 3500, state: "" })
        }
    }
    property bool lastPlaying: false
    Timer {
        id: stateT; interval: 400                         // players flip the state while changing tracks
        onTriggered: {
            const p = win.playing
            if (p === win.lastPlaying) return
            win.lastPlaying = p
            if (win.armed && !win.selfMedia && !win.expanded && win.player && win.player.trackTitle)
                win.push({ view: "track", key: "track", prio: 1, dur: 1800, state: p ? "play" : "pause" })
        }
    }

    // ------------------------------------------------------------ calendar
    readonly property var soon: {
        const t = Data.now.getTime() / 1000
        return (Data.calendar.items || []).find(x => !x.allDay && !x.holiday && x.start > t - 300 && x.start - t <= 900) || null
    }
    function soonText(s) {
        const left = Math.ceil((s.start - Data.now.getTime() / 1000) / 60)
        return left > 0 ? "za " + left + " min" : "teraz"
    }
    property var reminded: ({})
    Connections {
        target: Data
        function onNowChanged() {
            const s = win.soon
            if (!s) return
            const left = s.start - Date.now() / 1000, id = s.title + s.start
            if (left <= 30 && left > -120 && win.reminded[id] !== 2) {
                win.reminded[id] = 2
                win.push({ view: "info", key: "cal", prio: 3, dur: 9000, icon: "calEvent", color: Theme.accent, title: s.title, sub: "zaczyna się teraz" + (s.where ? " · " + s.where : ""), pulse: true, hold: true, action: { popup: "calendar" } })
            } else if (left <= 300 && left > 30 && !win.reminded[id]) {
                win.reminded[id] = 1
                win.push({ view: "info", key: "cal", prio: 2, dur: 7000, icon: "calEvent", color: Theme.accent, title: s.title, sub: win.soonText(s) + (s.where ? " · " + s.where : ""), action: { popup: "calendar" } })
            }
        }
        // rain within the hour (Open-Meteo 15-minute nowcast), at most once per 3 h
        function onWeatherChanged() {
            const w = Data.weather, s = w.soon || []
            if (!win.armed || (w.precip || 0) >= 0.1 || Date.now() - win.rainAt < 3 * 3600000) return
            const i = s.slice(0, 4).findIndex(x => x >= 0.2)
            if (i < 0) return
            win.rainAt = Date.now()
            const snow = (w.snow || 0) > 0
            win.push({ view: "info", key: "rain", prio: 2, dur: 6000, icon: snow ? "snow" : "umbrella", color: Theme.accent,
                       title: i === 0 ? (snow ? "Zaczyna padać śnieg" : "Zaczyna padać") : (snow ? "Śnieg" : "Deszcz") + " za ok. " + i * 15 + " min", sub: w.city || "" })
        }
    }
    property real rainAt: 0

    // ------------------------------------------------------------ minutnik (IPC: timer <s> [label])
    property real timerEnd: 0
    property real timerTotal: 0
    property string timerLabel: ""
    property real nowMs: Date.now()
    readonly property bool timerOn: timerEnd > 0
    readonly property real timerLeft: timerOn ? Math.max(0, (timerEnd - nowMs) / 1000) : 0
    Timer {
        interval: 1000; repeat: true; running: win.timerOn
        onTriggered: {
            win.nowMs = Date.now()
            if (win.nowMs >= win.timerEnd) {
                const l = win.timerLabel
                win.timerEnd = 0
                win.push({ view: "info", key: "timer", prio: 4, dur: 15000, icon: "alarm", color: win.orange, title: l || "Minutnik", sub: "czas minął · " + win.fmtDur(win.timerTotal), pulse: true, hold: true })
                Quickshell.execDetached(["pw-play", "/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga"])
            }
        }
    }
    function timerStart(s, label) {
        if (s <= 0) return
        timerTotal = s; timerLabel = label || ""; nowMs = Date.now(); timerEnd = nowMs + s * 1000
        push({ view: "info", key: "timer", prio: 2, dur: 2000, icon: "hourglass", color: orange, title: timerLabel || "Minutnik", sub: "start · " + fmtDur(s) })
    }
    function timerStop() {
        if (!timerOn) return
        timerEnd = 0
        push({ view: "info", key: "timer", prio: 2, dur: 1500, icon: "stop", color: Theme.t3, title: "Minutnik zatrzymany", sub: timerLabel })
    }

    IpcHandler {
        target: "island"
        function links(): string {
            return JSON.stringify(Pipewire.linkGroups.values.map(g => [g.source ? g.source.name : "?", g.source ? g.source.isSink : null, g.target ? g.target.name : "?", g.target ? [g.target.isStream, g.target.isSink, !!g.target.audio] : null, g.state]))
        }
        function state(): string { return JSON.stringify({ mode: win.mode, ev: win.ev, queue: win.queue.length, caps: win.caps, num: win.num, cam: win.camBusy, submap: win.submap, timer: win.timerLeft, hidden: win.hidden, expanded: win.expanded, glanceDemo: win.glanceDemo }) }
        function timer(seconds: int, label: string): void { win.timerStart(seconds, label) }
        function timerStop(): void { win.timerStop() }
        function dismiss(): void { win.clearAll() }
        function click(): void { if (win.ev) { win.act(win.ev); win.next() } }
        function show(icon: string, title: string, sub: string): void { win.push({ view: "info", prio: 2, dur: 3500, icon: icon, color: Theme.accent, title: title, sub: sub }) }
        function demos(): string { return "volume mic-volume notif notif-long progress urgent track pause mic busy cam share bt bt-low bt-off net vpn kbd caps num cal cal-now rain clip link color files image secret usb mount unmount monitor urgent-window special cfg cfg-err profile timer player glance phone phone-off phone-pair phone-charge phone-call call-in call-bt call-talk call-keys call-dial call-end" }
        function demo(kind: string): void {
            const A = Theme.accent
            const s = {
                "volume": { view: "osd", key: "vol", prio: 3, dur: 2500, mic: false },
                "mic-volume": { view: "osd", key: "vol", prio: 3, dur: 2500, mic: true },
                "notif": { view: "notif", prio: 2, dur: 5000, app: "Zen Browser", icon: Quickshell.iconPath("zen-browser", "dialog-information"), title: "Nowa wiadomość od Kasi", body: "Hej! Widzimy się dziś o 18 na bieganiu? Weź bidon, bo ma być ciepło.", urgent: false, value: -1, at: Date.now() },
                "notif-long": { view: "notif", prio: 2, dur: 8000, app: "Thunderbird", icon: Quickshell.iconPath("thunderbird", "dialog-information"), title: "Plan treningowy na październik", body: "Cześć! W załączniku plan na najbliższe cztery tygodnie: dwa akcenty w tygodniu (interwały i tempo), jedno długie wybieganie w niedzielę, reszta spokojnie w drugiej strefie. Daj znać, jak się czujesz po sobotnim starcie.", urgent: false, value: -1, at: Date.now() },
                "progress": { view: "notif", key: "v:demo", prio: 2, dur: 6000, app: "Pobieranie", icon: Quickshell.iconPath("folder-download", "dialog-information"), title: "archlinux-2026.10.01-x86_64.iso", body: "1,1 GB z 1,4 GB · 12 s", urgent: false, value: 78, at: Date.now() },
                "urgent": { view: "notif", prio: 3, dur: 9000, app: "System", icon: Quickshell.iconPath("dialog-warning"), title: "Mało miejsca na dysku", body: "Na partycji / zostało 2,1 GB.", urgent: true, value: -1, at: Date.now(), pulse: true },
                "track": { view: "track", key: "track", prio: 1, dur: 4000, state: "" },
                "pause": { view: "track", key: "track", prio: 1, dur: 2500, state: "pause" },
                "mic": { view: "info", key: "mic", prio: 3, dur: 2500, icon: "micOff", color: Theme.danger, title: "Mikrofon wyciszony", sub: "" },
                "busy": { view: "info", key: "busy", prio: 2, dur: 2500, icon: "mic", color: win.orange, title: "Mikrofon w użyciu", sub: "Discord" },
                "cam": { view: "info", key: "cam", prio: 2, dur: 2500, icon: "video", color: Theme.green, title: "Kamera w użyciu", sub: "Zen Browser" },
                "share": { view: "info", key: "share", prio: 2, dur: 2500, icon: "screenshot", color: win.purple, title: "Udostępniasz ekran", sub: "" },
                "bt": { view: "info", key: "bt", prio: 2, dur: 3000, icon: "btOn", color: A, title: "Słuchawki", sub: "połączono · 80%" },
                "bt-low": { view: "info", key: "btLow", prio: 3, dur: 4000, icon: "battLow", color: Theme.danger, title: "Słuchawki", sub: "słaba bateria · 9%" },
                "bt-off": { view: "info", key: "btA", prio: 2, dur: 2000, icon: "btOff", color: Theme.t3, title: "Bluetooth wyłączony", sub: "" },
                "net": { view: "info", key: "net", prio: 2, dur: 3000, icon: "worldOff", color: Theme.danger, title: "Brak internetu", sub: "" },
                "vpn": { view: "info", key: "vpn", prio: 2, dur: 2600, icon: "shield", color: Theme.green, title: "VPN połączony", sub: "wg0" },
                "kbd": { view: "info", key: "kbd", prio: 2, dur: 2000, icon: "keyboard", color: A, title: "Polish", sub: "układ klawiatury" },
                "caps": { view: "info", key: "lock", prio: 2, dur: 1600, icon: "capsLock", color: A, title: "Caps Lock włączony", sub: "" },
                "num": { view: "info", key: "lock", prio: 2, dur: 1600, icon: "numbers", color: Theme.t3, title: "Num Lock wyłączony", sub: "" },
                "cal": { view: "info", key: "cal", prio: 2, dur: 5000, icon: "calEvent", color: A, title: "Trening interwałowy", sub: "za 5 min · Stadion", action: { popup: "calendar" } },
                "cal-now": { view: "info", key: "cal", prio: 3, dur: 6000, icon: "calEvent", color: A, title: "Trening interwałowy", sub: "zaczyna się teraz · Stadion", pulse: true, action: { popup: "calendar" } },
                "rain": { view: "info", key: "rain", prio: 2, dur: 5000, icon: "umbrella", color: A, title: "Deszcz za ok. 30 min", sub: "Warszawa" },
                "clip": { view: "info", key: "clip", prio: 2, dur: 2500, icon: "copy", color: A, title: "Skopiowano", sub: "„Rozgrzewka 15 min, 5×800 m, przerwy 400 m truchtem”" },
                "link": { view: "info", key: "clip", prio: 2, dur: 2500, icon: "link", color: A, title: "Skopiowano link", sub: "https://github.com/quickshell-mirror/quickshell" },
                "color": { view: "info", key: "clip", prio: 2, dur: 2500, icon: "palette", color: A, title: "Skopiowano kolor", sub: "#A9C2EE", swatch: "#a9c2ee" },
                "files": { view: "info", key: "clip", prio: 2, dur: 2500, icon: "files", color: A, title: "Skopiowano pliki (3)", sub: "plan.pdf, trasa.gpx, zdjęcie.jpg" },
                "image": { view: "info", key: "clip", prio: 2, dur: 3000, icon: "photo", color: A, title: "Zrzut ekranu w schowku", sub: "412 KB", img: "file:///usr/share/pixmaps/archlinux-logo.png", photo: true },
                "secret": { view: "info", key: "clip", prio: 2, dur: 2000, icon: "lock", color: win.orange, title: "Skopiowano hasło", sub: "ukryte · schowek" },
                "usb": { view: "info", key: "usb", prio: 1, dur: 2400, icon: "usb", color: A, title: "SanDisk Ultra Fit", sub: "podłączono" },
                "mount": { view: "info", key: "mnt", prio: 2, dur: 4000, icon: "drive", color: A, title: "PENDRIVE", sub: "zamontowano · wolne 28,4 GB · otwórz", action: { open: Quickshell.env("HOME") } },
                "unmount": { view: "info", key: "mnt", prio: 2, dur: 2600, icon: "eject", color: Theme.green, title: "PENDRIVE", sub: "odmontowano · można odłączyć" },
                "monitor": { view: "info", key: "mon", prio: 2, dur: 3000, icon: "screenOn", color: A, title: "iiyama PL2770H", sub: "monitor podłączony" },
                "urgent-window": { view: "info", key: "urg", prio: 2, dur: 4000, img: win.entryIcon("kitty", "utilities-terminal"), color: win.orange, title: "kitty", sub: "prosi o uwagę · kliknij" },
                "special": { view: "info", key: "special", prio: 1, dur: 1500, icon: "layers", color: win.purple, title: "magic", sub: "obszar specjalny" },
                "cfg": { view: "info", key: "cfg", prio: 1, dur: 1800, icon: "refresh", color: Theme.green, title: "Konfiguracja przeładowana", sub: "Hyprland" },
                "cfg-err": { view: "info", key: "cfg", prio: 3, dur: 5000, icon: "alert", color: Theme.danger, title: "Błędy konfiguracji Hyprland (2)", sub: "binds.lua:42: unknown dispatcher 'focuswindw'" },
                "profile": { view: "info", key: "profile", prio: 2, dur: 2000, icon: "rocket", color: win.orange, title: "Wydajność", sub: "profil zasilania" }
            }[kind]
            if (kind === "player") { win.ev = null; win.expanded = !win.expanded; return }
            if (kind === "glance") { win.ev = null; win.expanded = !win.expanded; win.glanceDemo = win.expanded; return }
            if (kind === "timer") { win.timerStart(65, "Herbata"); return }
            if (kind === "call-in") { Phone.kdeCall({ what: "ringing", who: "Kasia", number: "+48 600 100 200" }); return }
            if (kind === "call-adb") { Phone.kdeCall({ src: "adb", what: "ringing", id: "TC@1", who: "Kasia", number: "+48 600 100 200" }); return }
            if (kind === "call-adb-talk") { Phone.kdeCall({ src: "adb", what: "talking", id: "TC@1", who: "Kasia", since: Math.floor(Date.now() / 1000) }); return }
            if (kind === "call-adb-out") { Phone.kdeCall({ src: "adb", what: "dialing", id: "TC@2", who: "Kasia", number: "+48600100200" }); return }
            if (kind === "call-adb-end") { Phone.kdeCall({ src: "adb", what: "idle" }); return }
            if (kind === "call-bt") { Phone.btCalls = [{ path: "/demo", state: "incoming", number: "+48600100200", name: "Kasia", since: 0 }]; return }
            if (kind === "call-talk") { Phone.btCalls = [{ path: "/demo", state: "active", number: "+48600100200", name: "Kasia", since: Math.floor(Date.now() / 1000) - 75 }]; return }
            if (kind === "call-keys") { Phone.btCalls = [{ path: "/demo", state: "active", number: "+48600100200", name: "Kasia", since: Math.floor(Date.now() / 1000) - 75 }]; win.keypad = true; return }
            if (kind === "call-dial") { Phone.btCalls = [{ path: "/demo", state: "alerting", number: "+48600100200", name: "", since: 0 }]; return }
            if (kind === "call-end") { Phone.btCalls = []; Phone.kcCall = null; return }
            if (kind.startsWith("phone")) { win.phoneEvent({ k: { "phone": "conn", "phone-off": "disc", "phone-pair": "pairreq", "phone-charge": "plug", "phone-call": "call" }[kind], name: "POCO X5 Pro 5G", key: "A3E7A214", bat: 64, what: "ringing", who: "Kasia" }); return }
            if (s) win.push(s)
        }
    }

    // ================================================================ the pill
    Item {
        id: shell
        anchors.horizontalCenter: parent.horizontalCenter
        y: win.hidden ? -height - 8 : 5                     // same line as the top bar capsules
        Behavior on y { SpringAnimation { spring: 3; damping: 0.32; epsilon: 0.25 } }
        readonly property Item cur: win.mode === "player" ? vPlayer : win.mode === "glance" ? vGlance : win.mode === "osd" ? vOsd : win.mode === "notif" ? vNotif
            : win.mode === "call" ? vCall
            : win.mode === "track" ? vTrack : win.mode === "info" ? vInfo : vIdle
        width: cur.implicitWidth
        height: cur.implicitHeight
        // iOS morph: a spring with a small overshoot, width and height move together
        Behavior on width { SpringAnimation { spring: 3.4; damping: 0.3; epsilon: 0.25 } }
        Behavior on height { SpringAnimation { spring: 3.4; damping: 0.3; epsilon: 0.25 } }

        // attention: a short bounce (urgent notification, calendar start, minutnik end, low battery)
        transform: Scale { id: bump; origin.x: shell.width / 2; origin.y: 0 }
        SequentialAnimation {
            id: pulse
            NumberAnimation { target: bump; properties: "xScale,yScale"; to: 1.07; duration: 140; easing.type: Easing.OutQuad }
            NumberAnimation { target: bump; properties: "xScale,yScale"; to: 0.98; duration: 160; easing.type: Easing.InOutQuad }
            NumberAnimation { target: bump; properties: "xScale,yScale"; to: 1; duration: 260; easing.type: Easing.OutBack }
        }

        // always pure black (no glass, no rim): a pill while compact, a rounded slab when expanded
        readonly property real r: Math.min(height / 2, 30)
        RectangularShadow {
            anchors.fill: parent; radius: shell.r; offset.y: 8; blur: 24; spread: -4
            color: Qt.rgba(0, 0, 0, 0.45)
            opacity: win.mode === "idle" ? 0 : 1
            Behavior on opacity { NumberAnimation { duration: 220 } }
        }
        Rectangle { anchors.fill: parent; radius: shell.r; color: "#000000" }

        HoverHandler {
            id: hover
            onHoveredChanged: if (hovered) { closeT.stop(); openT.restart() } else { openT.stop(); if (win.expanded) closeT.restart() }
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
            cursorShape: win.mode === "idle" || win.mode === "glance" || win.ev && win.ev.action ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: m => {
                if (m.button === Qt.MiddleButton) { if (win.player) { win.mine(); win.player.togglePlaying() } return }
                if (m.button === Qt.RightButton) { if (win.ev || win.queue.length) win.clearAll(); else if (win.timerOn) win.timerStop(); return }
                if (win.mode === "idle" || win.mode === "glance") Ui.toggle("calendar", win.modelData.name)
                else if (win.mode !== "player" && win.mode !== "call") { win.act(win.ev); win.next() }
            }
            onWheel: w => win.setVolume(w.angleDelta.y > 0 ? 0.05 : -0.05)
        }

        Item {
            anchors.fill: parent
            clip: true

            // ---------------- idle
            IslandPane {
                id: vIdle; name: "idle"; mode: win.mode
                implicitWidth: idleRow.implicitWidth + 28; implicitHeight: 30
                RowLayout {
                    id: idleRow
                    anchors.centerIn: parent
                    spacing: 8
                    ClippingRectangle {
                        visible: win.art !== "" && !win.soon
                        implicitWidth: 18; implicitHeight: 18; radius: 5; color: Theme.fill
                        Image { anchors.fill: parent; source: win.art; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(48, 48); asynchronous: true }
                    }
                    Icon { visible: !!win.soon; name: "calEvent"; size: 15; color: Theme.accent }
                    Txt {
                        Layout.maximumWidth: 240
                        text: win.soon ? win.soon.title + " · " + win.soonText(win.soon)
                            : Data.daysShort[Data.now.getDay()] + " " + Data.now.getDate() + " " + Data.monthsGen[Data.now.getMonth()].slice(0, 3)
                        size: 13; w: 600; color: win.soon ? Theme.accent : Theme.t1
                    }
                    Txt { text: "·"; size: 13; color: Theme.t3 }
                    Txt { text: Data.hm(Data.now); size: 13; w: 600 }
                    // minutnik
                    RowLayout {
                        visible: win.timerOn
                        spacing: 5
                        IslandRing { frac: win.timerTotal > 0 ? win.timerLeft / win.timerTotal : 0; c: win.timerLeft < 10 ? Theme.danger : win.orange }
                        Txt { text: win.fmt(Math.ceil(win.timerLeft)); size: 12.5; w: 650; color: win.timerLeft < 10 ? Theme.danger : win.orange; font.features: ({ "tnum": 1 }) }
                    }
                    // Hyprland submap (resize mode, ...)
                    Rectangle {
                        visible: win.submap !== ""
                        implicitHeight: 18; implicitWidth: smRow.implicitWidth + 12; radius: 9
                        color: Qt.rgba(187/255, 169/255, 238/255, 0.2)
                        Row {
                            id: smRow; anchors.centerIn: parent; spacing: 3
                            Icon { name: "command"; size: 11; color: win.purple; anchors.verticalCenter: parent.verticalCenter }
                            Txt { text: win.submap; size: 11; w: 650; color: win.purple; anchors.verticalCenter: parent.verticalCenter }
                        }
                    }
                    Icon { visible: win.caps; name: "capsLock"; size: 14; color: Theme.accent }
                    IslandDot { visible: win.camBusy; c: Theme.green }
                    IslandDot { visible: win.micBusy; c: win.orange }
                    IslandDot { visible: win.sharing; c: win.purple }
                    IslandEq { visible: win.playing; run: vIdle.active; Layout.alignment: Qt.AlignVCenter }
                }
            }

            // ---------------- glance (hover, nothing playing)
            IslandPane {
                id: vGlance; name: "glance"; mode: win.mode
                readonly property var w: Data.weather || ({})
                readonly property var evs: Data.upcoming(2, 2)
                implicitWidth: 420; implicitHeight: Math.max(76, gCol.implicitHeight + 28)
                Row {
                    id: gWx
                    x: 18; anchors.verticalCenter: parent.verticalCenter
                    spacing: 10
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: Theme.wxIcon(vGlance.w.code, vGlance.w.day); size: 30; color: Theme.t1 }
                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        Txt { text: vGlance.w.temp !== undefined ? vGlance.w.temp + "°" : "–"; size: 22; w: 600 }
                        Txt { text: vGlance.w.desc || ""; size: 11; color: Theme.t3; width: Math.min(implicitWidth, 96) }
                        Txt { readonly property var d: (vGlance.w.days || [])[0]; visible: !!d; text: d ? d.max + "° / " + d.min + "°" + (d.pop >= 30 ? " · " + d.pop + "%" : "") : ""; size: 11; color: Theme.t3 }
                    }
                }
                Rectangle { id: gSep; x: 150; width: 1; height: parent.height - 28; anchors.verticalCenter: parent.verticalCenter; color: Theme.hair }
                Column {
                    id: gCol
                    x: gSep.x + 14; anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x - 18
                    spacing: 4
                    Row {
                        visible: win.timerOn
                        spacing: 6
                        IslandRing { anchors.verticalCenter: parent.verticalCenter; frac: win.timerTotal > 0 ? win.timerLeft / win.timerTotal : 0 }
                        Txt { text: (win.timerLabel || "Minutnik") + " · " + win.fmt(Math.ceil(win.timerLeft)); size: 12.5; w: 600; color: win.orange }
                    }
                    Repeater {
                        model: vGlance.evs
                        Column {
                            required property var modelData
                            width: gCol.width
                            Txt { width: parent.width; text: modelData.title; size: 12.5; w: 600 }
                            Txt { width: parent.width; text: Data.rel(modelData.start) + " · " + Data.when(modelData) + (modelData.where ? " · " + modelData.where : ""); size: 11; color: Theme.t3 }
                        }
                    }
                    Txt { visible: vGlance.evs.length === 0 && !win.timerOn; text: "Brak wydarzeń do jutra"; size: 12.5; color: Theme.t3 }
                }
            }

            // ---------------- player
            IslandPane {
                id: vPlayer; name: "player"; mode: win.mode
                implicitWidth: 452; implicitHeight: seek.len > 0 || win.volRecent ? 144 : 110
                readonly property var p: win.player
                ClippingRectangle {
                    id: pArt
                    x: 16; y: 16; width: 78; height: 78; radius: 16; color: Theme.fill
                    Image { anchors.fill: parent; source: win.art; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(180, 180); asynchronous: true }
                    Icon { anchors.centerIn: parent; name: "music"; size: 28; visible: win.art === ""; color: Theme.t3 }
                }
                // player chip (click = next player)
                Rectangle {
                    id: chip
                    anchors.right: parent.right; anchors.rightMargin: 16; y: 18
                    height: 24; radius: 12; width: chipRow.implicitWidth + 14
                    color: chipMa.containsMouse && win.players.length > 1 ? Theme.fill2 : Theme.fill
                    Row {
                        id: chipRow; anchors.centerIn: parent; spacing: 4
                        IconImage { implicitSize: 14; anchors.verticalCenter: parent.verticalCenter; source: vPlayer.p ? win.entryIcon(vPlayer.p.desktopEntry || vPlayer.p.identity, "multimedia-player") : "" }
                        Txt { visible: win.players.length > 1; anchors.verticalCenter: parent.verticalCenter; text: (win.players.indexOf(vPlayer.p) + 1) + "/" + win.players.length; size: 10.5; w: 600; color: Theme.t2 }
                    }
                    MouseArea { id: chipMa; anchors.fill: parent; hoverEnabled: true; cursorShape: win.players.length > 1 ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: win.nextPlayer() }
                }
                Column {
                    x: pArt.x + pArt.width + 14; y: 19
                    width: chip.x - x - 10
                    spacing: 2
                    Txt { width: parent.width; text: vPlayer.p ? vPlayer.p.trackTitle || vPlayer.p.identity : ""; size: 15; w: 650 }
                    Txt { width: parent.width; text: vPlayer.p ? vPlayer.p.trackArtist || vPlayer.p.trackAlbum || vPlayer.p.identity : ""; size: 12.5; color: Theme.t2 }
                }
                Row {
                    x: pArt.x + pArt.width + 8; y: 58
                    spacing: 4
                    IslandCtl { visible: !!vPlayer.p && vPlayer.p.shuffleSupported; icon: "shuffle"; lit: !!vPlayer.p && vPlayer.p.shuffle; onHit: vPlayer.p.shuffle = !vPlayer.p.shuffle }
                    IslandCtl { icon: "prevF"; enabled: !!vPlayer.p && vPlayer.p.canGoPrevious; onHit: { win.mine(); vPlayer.p.previous() } }
                    IslandCtl { icon: win.playing ? "pauseF" : "playF"; big: true; enabled: !!vPlayer.p && vPlayer.p.canTogglePlaying; onHit: { win.mine(); vPlayer.p.togglePlaying() } }
                    IslandCtl { icon: "nextF"; enabled: !!vPlayer.p && vPlayer.p.canGoNext; onHit: { win.mine(); vPlayer.p.next() } }
                    IslandCtl {
                        visible: !!vPlayer.p && vPlayer.p.loopSupported
                        icon: vPlayer.p && vPlayer.p.loopState === MprisLoopState.Track ? "repeatOne" : "repeat"
                        lit: !!vPlayer.p && vPlayer.p.loopState !== MprisLoopState.None
                        onHit: vPlayer.p.loopState = vPlayer.p.loopState === MprisLoopState.None ? MprisLoopState.Playlist : vPlayer.p.loopState === MprisLoopState.Playlist ? MprisLoopState.Track : MprisLoopState.None
                    }
                }
                // progress (seekable) / volume while it changes
                Item {
                    id: seek
                    x: 18; y: 106; width: parent.width - 36; height: 20
                    readonly property real len: vPlayer.p && vPlayer.p.length > 0 ? vPlayer.p.length : 0
                    readonly property real pos: vPlayer.p ? vPlayer.p.position : 0
                    readonly property bool can: !!vPlayer.p && vPlayer.p.canSeek && len > 0
                    property real drag: -1
                    readonly property real frac: drag >= 0 ? drag : len > 0 ? Math.min(1, pos / len) : 0
                    opacity: win.volRecent || len <= 0 ? 0 : 1
                    visible: opacity > 0
                    Behavior on opacity { NumberAnimation { duration: 160 } }
                    Txt { id: el; width: 38; anchors.verticalCenter: parent.verticalCenter; text: seek.len > 0 ? win.fmt(seek.frac * seek.len) : ""; size: 11; w: 500; color: Theme.t3; font.features: ({ "tnum": 1 }) }
                    Txt { id: rem; width: 42; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: "−" + win.fmt(seek.len - seek.frac * seek.len); size: 11; w: 500; color: Theme.t3; font.features: ({ "tnum": 1 }) }
                    Rectangle {
                        id: track
                        anchors.left: el.right; anchors.right: rem.left; anchors.leftMargin: 6; anchors.rightMargin: 6
                        anchors.verticalCenter: parent.verticalCenter
                        height: sma.containsMouse || seek.drag >= 0 ? 7 : 5; radius: height / 2; color: Theme.fill2
                        Behavior on height { NumberAnimation { duration: 120 } }
                        Rectangle { width: Math.max(height, parent.width * seek.frac); height: parent.height; radius: height / 2; color: Theme.t1; visible: seek.len > 0 }
                        MouseArea {
                            id: sma
                            anchors.fill: parent; anchors.topMargin: -7; anchors.bottomMargin: -7
                            enabled: seek.can; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                            onPressed: m => seek.drag = Math.max(0, Math.min(1, m.x / track.width))
                            onPositionChanged: m => { if (pressed) seek.drag = Math.max(0, Math.min(1, m.x / track.width)) }
                            onReleased: { if (vPlayer.p) vPlayer.p.position = seek.drag * seek.len; seek.drag = -1 }
                        }
                    }
                }
                Item {
                    x: 18; y: 106; width: parent.width - 36; height: 20
                    opacity: win.volRecent ? 1 : 0
                    visible: opacity > 0
                    Behavior on opacity { NumberAnimation { duration: 160 } }
                    IslandVolume { node: win.sink;  anchors.fill: parent }
                }
            }

            // ---------------- volume (output or microphone)
            IslandPane {
                id: vOsd; name: "osd"; mode: win.mode
                readonly property bool mic: !!(win.lastEv.osd || {}).mic
                implicitWidth: 300; implicitHeight: 40
                IslandVolume { node: vOsd.mic ? win.source : win.sink; mic: vOsd.mic; x: 16; width: parent.width - 32; height: parent.height }
            }

            // ---------------- phone call: ringing = answer / reject; in a call = a green pill with the time,
            // hover = mute, audio here or on the phone, keypad (tones), hang up
            IslandPane {
                id: vCall; name: "call"; mode: win.mode
                readonly property var c: Phone.call || win.lastCall || ({})
                readonly property bool ring: c.state === "incoming" || c.state === "waiting"
                readonly property bool live: c.state === "active"
                readonly property bool open: active && (ring || hover.hovered || win.keypad || !live && c.state !== "held")
                readonly property var au: Phone.callAudio
                readonly property string status: ring ? (c.state === "waiting" ? "druga rozmowa czeka" : "połączenie przychodzące") + " · " + (Phone.name || "telefon")
                    : live ? win.fmt(win.callNow - c.since) : c.state === "held" ? "wstrzymane" : c.state === "alerting" ? "dzwonię…" : "łączenie…"
                readonly property string audioText: !c.bt ? (ring ? "bez Bluetooth: dźwięk na telefonie" : "")
                    : au.err ? au.err : !au.pc ? "dźwięk na telefonie" : au.on ? "dźwięk przez komputer" + (au.mute ? " · mikrofon wyciszony" : "") : ring ? "" : "łączę dźwięk…"
                implicitWidth: open ? 452 : cMini.implicitWidth + 30
                implicitHeight: !open ? 34 : ring ? 80 : win.keypad ? 196 : 116

                // compact: the call goes on
                Row {
                    id: cMini
                    visible: !vCall.open
                    anchors.centerIn: parent; spacing: 8
                    Icon { name: "call"; size: 14; color: Theme.green; anchors.verticalCenter: parent.verticalCenter }
                    Txt { text: vCall.c.who || ""; size: 13; w: 600; width: Math.min(implicitWidth, 180); anchors.verticalCenter: parent.verticalCenter }
                    Txt { text: vCall.status; size: 13; w: 650; color: vCall.live ? Theme.green : Theme.t3; font.features: ({ "tnum": 1 }); anchors.verticalCenter: parent.verticalCenter }
                    Icon { visible: !!vCall.au.mute; name: "micOff"; size: 13; color: win.orange; anchors.verticalCenter: parent.verticalCenter }
                }

                Item {
                    visible: vCall.open
                    anchors.fill: parent
                    Rectangle {
                        id: cAv
                        x: 14; y: 14; width: 50; height: 50; radius: 25
                        color: Qt.rgba(Theme.green.r, Theme.green.g, Theme.green.b, 0.18)
                        Txt { anchors.centerIn: parent; visible: win.letter((vCall.c.who || "").charAt(0)); text: (vCall.c.who || "?").charAt(0).toUpperCase(); size: 20; w: 650; color: Theme.green }
                        Icon { anchors.centerIn: parent; visible: !win.letter((vCall.c.who || "").charAt(0)); name: vCall.ring ? "callRing" : "call"; size: 20; color: Theme.green }
                    }
                    Column {
                        x: cAv.x + cAv.width + 12; anchors.verticalCenter: cAv.verticalCenter
                        width: parent.width - x - (vCall.ring ? 128 : 16)
                        spacing: 1
                        Txt { width: parent.width; text: vCall.status; size: 11.5; w: vCall.live ? 650 : 400; color: vCall.live ? Theme.green : Theme.t3; font.features: ({ "tnum": 1 }) }
                        Txt { width: parent.width; text: vCall.c.who || ""; size: 16; w: 650; textFormat: Text.PlainText }
                        Txt { width: parent.width; visible: text !== ""; size: 11; color: vCall.au.err ? win.orange : Theme.t3; textFormat: Text.PlainText
                              text: [vCall.c.number && vCall.c.number !== vCall.c.who ? vCall.c.number : "", vCall.audioText].filter(x => x).join(" · ") }
                    }
                    // ringing: reject / answer
                    Row {
                        visible: vCall.ring
                        anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: cAv.verticalCenter
                        spacing: 14
                        RoundBtn { icon: "callEnd"; bg: Theme.danger; onClicked: Phone.hangup() }
                        RoundBtn { icon: "call"; bg: Theme.green; onClicked: Phone.answer() }
                    }
                    // in a call: mute, where the audio goes, keypad, hang up
                    Row {
                        visible: !vCall.ring
                        x: 14; y: 76; spacing: 6
                        ActBtn { visible: !!vCall.c.bt; icon: vCall.au.mute ? "micOff" : "mic"; text: vCall.au.mute ? "Wyciszony" : "Mikrofon"; primary: !!vCall.au.mute; onClicked: Phone.callMute() }
                        ActBtn { visible: !!vCall.c.bt; icon: vCall.au.pc ? "speaker" : "mobile"; text: vCall.au.pc ? "Komputer" : "Telefon"; onClicked: Phone.callAudioToggle() }
                        ActBtn { visible: !!vCall.c.bt; icon: "dialpad"; text: "Klawiatura"; primary: win.keypad; onClicked: win.keypad = !win.keypad }
                    }
                    Rectangle {
                        visible: !vCall.ring
                        anchors.right: parent.right; anchors.rightMargin: 14; y: 74
                        width: endRow.implicitWidth + 24; height: 30; radius: 15
                        color: endM.containsMouse ? Qt.lighter(Theme.danger, 1.1) : Theme.danger
                        Row {
                            id: endRow; anchors.centerIn: parent; spacing: 5
                            Icon { name: "callEnd"; size: 14; color: "#ffffff"; anchors.verticalCenter: parent.verticalCenter }
                            Txt { text: "Zakończ"; size: 12; w: 650; color: "#ffffff"; anchors.verticalCenter: parent.verticalCenter }
                        }
                        MouseArea { id: endM; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Phone.hangup() }
                    }
                    // keypad: tones to the call (menus of hotlines)
                    Grid {
                        visible: win.keypad && !vCall.ring
                        x: 14; y: 116; columns: 6; spacing: 6
                        Repeater {
                            model: ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"]
                            Rectangle {
                                required property string modelData
                                width: (452 - 28 - 30) / 6; height: 32; radius: 10
                                color: kM.pressed ? Theme.fill2 : kM.containsMouse ? Theme.fill2 : Theme.fill
                                Txt { anchors.centerIn: parent; text: parent.modelData; size: 15; w: 600 }
                                MouseArea { id: kM; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Phone.tones(parent.modelData) }
                            }
                        }
                    }
                }
            }

            // ---------------- notification
            IslandPane {
                id: vNotif; name: "notif"; mode: win.mode
                readonly property var e: win.lastEv.notif || ({})
                readonly property bool open: (hover.hovered || win.replying) && active
                readonly property var pn: win.phoneNote(e.phone)          // the phone note behind it (KDE Connect)
                readonly property bool acts: !!pn && (pn.reply || pn.dismiss || (pn.actions || []).length > 0)
                implicitWidth: 452; implicitHeight: Math.max(56, nCol.implicitHeight + 24)
                ClippingRectangle {
                    id: nIcon
                    x: 14; y: vNotif.open ? 14 : (parent.height - height) / 2
                    width: 38; height: 38; radius: 11; color: Theme.fill
                    Image { anchors.fill: parent; anchors.margins: vNotif.pn && vNotif.pn.icon ? 0 : 3; fillMode: Image.PreserveAspectFit; sourceSize: Qt.size(96, 96); asynchronous: true
                            source: vNotif.pn && vNotif.pn.icon ? "file://" + vNotif.pn.icon : vNotif.e.icon || "" }
                }
                // phone notifications: a small phone badge on the icon
                Rectangle {
                    visible: !!vNotif.e.phone
                    x: nIcon.x + nIcon.width - 13; y: nIcon.y + nIcon.height - 13
                    width: 17; height: 17; radius: 8.5; color: "#000000"
                    Icon { anchors.centerIn: parent; name: "mobile"; size: 10; color: Theme.accent }
                }
                Column {
                    id: nCol
                    x: nIcon.x + nIcon.width + 12; y: 12
                    width: parent.width - x - 18
                    spacing: 1
                    Row {
                        width: parent.width; spacing: 6
                        Txt { id: nApp; text: vNotif.e.app || ""; size: 11; w: 600; color: Theme.t3; width: Math.min(implicitWidth, parent.width - 110) }
                        Txt { text: "teraz"; size: 11; color: Theme.t4 }
                        IslandDot { visible: !!vNotif.e.urgent; c: Theme.danger; anchors.verticalCenter: nApp.verticalCenter }
                    }
                    Txt { width: parent.width; text: vNotif.e.title || ""; size: 13.5; w: 600; textFormat: Text.PlainText
                          wrapMode: vNotif.open ? Text.Wrap : Text.NoWrap; maximumLineCount: vNotif.open ? 2 : 1 }
                    Txt { visible: !!vNotif.e.body; width: parent.width; text: vNotif.e.body || ""; size: 12; color: Theme.t2; textFormat: Text.PlainText
                          wrapMode: vNotif.open ? Text.Wrap : Text.NoWrap; maximumLineCount: vNotif.open ? 6 : 1; lineHeight: 1.08 }
                    Item {
                        visible: (vNotif.e.value ?? -1) >= 0
                        width: parent.width; height: 12
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter; anchors.left: parent.left; anchors.right: nPct.left; anchors.rightMargin: 8
                            height: 5; radius: 2.5; color: Theme.fill2
                            Rectangle { width: parent.width * Math.max(0, Math.min(100, vNotif.e.value || 0)) / 100; height: parent.height; radius: 2.5; color: Theme.accent
                                        Behavior on width { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } } }
                        }
                        Txt { id: nPct; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: (vNotif.e.value || 0) + "%"; size: 10.5; w: 600; color: Theme.t3; font.features: ({ "tnum": 1 }) }
                    }
                    // phone (hover): reply in place, the note's own actions, dismiss on the phone
                    Item {
                        visible: vNotif.open && vNotif.acts
                        width: parent.width; height: visible ? nAct.implicitHeight + 9 : 0
                        MouseArea { anchors.fill: parent }          // clicks between the buttons do not open the phone panel
                        Flow {
                            id: nAct
                            visible: !win.replying
                            y: 9; width: parent.width; spacing: 6
                            ActBtn { visible: !!vNotif.pn && vNotif.pn.reply; icon: "message"; text: "Odpowiedz"; primary: true
                                   onClicked: { win.replying = true; nReply.text = ""; nReply.forceActiveFocus() } }
                            Repeater {
                                model: vNotif.pn ? (vNotif.pn.actions || []).slice(0, 3) : []
                                ActBtn { text: modelData
                                       onClicked: { Phone.action(vNotif.pn.id, modelData); win.phoneDone("wysłano · " + modelData) } }
                            }
                            ActBtn { visible: !!vNotif.pn && vNotif.pn.dismiss; icon: "x"; text: "Odrzuć"
                                   onClicked: { Phone.dismiss(vNotif.pn.id); win.phoneDone("") } }
                        }
                        Rectangle {
                            visible: win.replying
                            y: 9; width: parent.width; height: 32; radius: 16
                            color: Theme.fill; border.width: 1; border.color: nReply.activeFocus ? Qt.rgba(169/255, 194/255, 238/255, 0.45) : Theme.rim
                            TextInput {
                                id: nReply
                                x: 13; width: parent.width - 13 - 34; anchors.verticalCenter: parent.verticalCenter
                                color: Theme.t1; font.family: Theme.font; font.pixelSize: 13
                                selectionColor: Theme.accentSoft; clip: true
                                function send() { const t = text.trim(); if (t && vNotif.pn) { Phone.reply(vNotif.pn.id, t); win.phoneDone("odpowiedź wysłana") } }
                                Keys.onReturnPressed: send()
                                Keys.onEscapePressed: win.replying = false
                                Txt { visible: !parent.text; text: "Odpowiedź do " + (vNotif.e.title || ""); size: 12.5; color: Theme.t4; anchors.verticalCenter: parent.verticalCenter; width: parent.width }
                            }
                            Rectangle {
                                anchors.right: parent.right; anchors.rightMargin: 3; anchors.verticalCenter: parent.verticalCenter
                                width: 26; height: 26; radius: 13
                                color: nReply.text.trim() ? Theme.accent : Theme.fill2
                                Icon { anchors.centerIn: parent; name: "send"; size: 13; color: nReply.text.trim() ? Theme.ink : Theme.t3 }
                                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: nReply.send() }
                            }
                        }
                    }
                }
                // more notifications waiting
                Rectangle {
                    visible: win.waiting > 0
                    anchors.right: parent.right; anchors.rightMargin: 14; y: 11
                    height: 17; radius: 8.5; width: nMore.implicitWidth + 12
                    color: Theme.fill2
                    Txt { id: nMore; anchors.centerIn: parent; text: "+" + win.waiting; size: 10.5; w: 650; color: Theme.t2 }
                }
            }

            // ---------------- track peek (track change, play / pause)
            IslandPane {
                id: vTrack; name: "track"; mode: win.mode
                readonly property string st: (win.lastEv.track || {}).state || ""
                implicitWidth: Math.min(420, Math.max(300, tCol.implicitWidth + 112)); implicitHeight: 50
                ClippingRectangle {
                    id: tArt
                    x: 9; anchors.verticalCenter: parent.verticalCenter
                    width: 34; height: 34; radius: 10; color: Theme.fill
                    Image { anchors.fill: parent; source: win.art; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(96, 96); asynchronous: true }
                    Icon { anchors.centerIn: parent; name: "music"; size: 16; visible: win.art === ""; color: Theme.t3 }
                }
                Column {
                    id: tCol
                    x: tArt.x + tArt.width + 11; anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x - 52
                    Txt { width: parent.width; text: win.player ? win.player.trackTitle : ""; size: 13; w: 600 }
                    Txt { width: parent.width; text: win.player ? win.player.trackArtist || win.player.identity : ""; size: 11.5; color: Theme.t3 }
                }
                IslandEq { visible: vTrack.st === ""; anchors.right: parent.right; anchors.rightMargin: 18; anchors.verticalCenter: parent.verticalCenter; run: vTrack.active && win.playing }
                Icon { visible: vTrack.st !== ""; anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter
                       name: vTrack.st === "pause" ? "pauseF" : "playF"; size: 18; color: vTrack.st === "pause" ? Theme.t2 : Theme.accent }
            }

            // ---------------- info (one line; title over a long subtitle / a thumbnail in two)
            IslandPane {
                id: vInfo; name: "info"; mode: win.mode
                readonly property var e: win.lastEv.info || ({})
                readonly property color c: e.color || Theme.accent
                readonly property bool photo: !!e.photo && !!e.img
                readonly property bool stack: photo || (e.title || "").length + (e.sub || "").length > 38
                implicitWidth: Math.min(stack ? 470 : 440, Math.max(230, iRow.implicitWidth + (photo ? 30 : 24))); implicitHeight: photo ? 58 : stack ? 50 : 40
                Row {
                    id: iRow
                    x: vInfo.photo ? 10 : vInfo.stack ? 11 : 7; anchors.verticalCenter: parent.verticalCenter
                    spacing: vInfo.stack ? 11 : 10
                    Item {
                        width: vInfo.photo ? 52 : 28; height: vInfo.photo ? 38 : 28
                        anchors.verticalCenter: parent.verticalCenter
                        // icon badge / app icon / colour swatch / image thumbnail
                        Rectangle {
                            anchors.fill: parent; radius: 14
                            visible: !vInfo.photo && !vInfo.e.img
                            color: vInfo.e.swatch ? vInfo.e.swatch : Qt.rgba(vInfo.c.r, vInfo.c.g, vInfo.c.b, 0.18)
                            border.width: vInfo.e.swatch ? 1 : 0; border.color: Theme.rim
                            Icon { visible: !vInfo.e.swatch; anchors.centerIn: parent; name: vInfo.e.icon || "bell"; size: 15; color: vInfo.c }
                        }
                        IconImage { visible: !vInfo.photo && !!vInfo.e.img; anchors.centerIn: parent; implicitSize: 24; source: !vInfo.photo ? vInfo.e.img || "" : "" }
                        ClippingRectangle {
                            visible: vInfo.photo
                            anchors.fill: parent; radius: 9; color: Theme.fill
                            border.width: 1; border.color: Theme.rim
                            Image { anchors.fill: parent; source: vInfo.photo ? vInfo.e.img : ""; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(160, 120); asynchronous: true; cache: false }
                        }
                    }
                    Column {
                        visible: vInfo.stack
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1
                        Txt { text: vInfo.e.title || ""; size: 13; w: 600; width: Math.min(implicitWidth, 380) }
                        Txt { text: vInfo.e.sub || ""; size: 11.5; color: Theme.t3; width: Math.min(implicitWidth, 380); textFormat: Text.PlainText }
                    }
                    Txt { visible: !vInfo.stack; anchors.verticalCenter: parent.verticalCenter; text: vInfo.e.title || ""; size: 13; w: 600; width: Math.min(implicitWidth, 260) }
                    Txt { visible: !vInfo.stack && !!vInfo.e.sub; anchors.verticalCenter: parent.verticalCenter; text: vInfo.e.sub || ""; size: 12; color: Theme.t3
                          width: Math.min(implicitWidth, 220); textFormat: Text.PlainText }
                }
            }
        }
    }
}
