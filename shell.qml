//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Services.Mpris

ShellRoot {
    // primary monitor (iiyama PL2770H, DP-3); the second one keeps a bare wallpaper like on Windows
    readonly property var primary: Quickshell.screens.filter(s => s.name === "DP-3")

    // wallpaper: the photo or the animated scene chosen in ~/.config/mist/wallpaper.json
    // wallpaper: native (mist-ui draws the photo, mist-wall the animated scene)
    // top bar: native now (native/mist-ui, drawn on the CPU); TopBar.qml stays for reference / fallback
    // desktop (clock, cards, weather sky): native (mist-ui + mist-wall)
    // Dynamic Island: native now (native/mist-ui island.rs), with the notification server, the phone and MPRIS
    // dock: native now (native/mist-ui dock.rs); Dock.qml stays for reference / fallback
    Snap { id: snapper }
    // lock (mist-ui-bin lock), idle and the polkit agent: native (mist-ui)
    // macOS-style overlays (single instance, follow the focused monitor)

    // popups are created only while open (no idle windows, less RAM)
    Variants {
        model: Quickshell.screens
        delegate: Component {
            Scope {
                id: s
                required property var modelData
                LazyLoader { active: Ui.popup === "start" && Ui.screenName === s.modelData.name; Start { modelData: s.modelData } }
                LazyLoader { active: Ui.popup === "phone" && Ui.screenName === s.modelData.name; PhonePanel { modelData: s.modelData } }
                LazyLoader { active: (Ui.popup === "spotlight" || Ui.popup === "emoji") && Ui.screenName === s.modelData.name; Spotlight { modelData: s.modelData } }
                LazyLoader { active: Ui.popup === "control" && Ui.screenName === s.modelData.name; Control { modelData: s.modelData } }
                LazyLoader { active: Ui.popup === "wallpaper" && Ui.screenName === s.modelData.name; Wallpapers { modelData: s.modelData } }
            }
        }
    }

    // mist-qs -p ~/.config/quickshell/mist ipc call mist toggle start|calendar|phone|spotlight|emoji|control|notifications|session|wallpaper|clipboard
    IpcHandler {
        target: "mist"
        function toggle(name: string): void { if (name === "control") Ui.control("", ""); else Ui.toggle(name, "") }
        // Control centre on a page: "" | sound | network | bluetooth
        function control(page: string): void { Ui.control(page, "") }
        function close(): void { Ui.close() }
        // the native bar (native/mist-ui) opens popups on its own monitor
        function toggleOn(name: string, screen: string): void { Ui.toggle(name, screen) }
        function controlOn(page: string, screen: string): void { Ui.control(page, screen) }
        function dnd(on: bool): void { Notifs.setDnd(on) }
        function phoneRing(): void { Phone.ring() }
        // media keys for scripts and the phone (KDE Connect run-command): toggle | next | prev
        function media(action: string): void {
            const ps = Mpris.players.values, p = ps.find(p => p.isPlaying) || ps.find(p => p.trackTitle) || ps[0]
            if (!p) return
            if (action === "next") p.next(); else if (action === "prev") p.previous(); else p.togglePlaying()
        }
        function stats(): string { return JSON.stringify([Data.cpuPct, Data.memUsed, Data.rxRate, Data.barStats]) }
        function snapPreview(on: bool): void { snapper.screenObj = Quickshell.screens.find(s => s.name === "DP-3"); snapper.preview = on; snapper.hot = on ? [0.5, 0, 0.5, 1] : null }
        function snapEdge(on: bool): void { snapper.screenObj = Quickshell.screens.find(s => s.name === "DP-3"); snapper.preview = on; snapper.dragging = on; snapper.mode = on ? "edge" : ""; snapper.target = on ? [0, 0, 1, 1] : null }
        function snapState(): string { return JSON.stringify([snapper.cx, snapper.cy, snapper.addr, snapper.grab, snapper.mode, snapper.zone, snapper.dragging]) }
        function snapAssist(): string { return JSON.stringify([snapper.assistScreen ? snapper.assistScreen.name : null, snapper.assistZones, snapper.assistGroup, snapper.assistWins.map(t => t.title)]) }
        function snapAssistPick(i: int): void { snapper.assistPick(snapper.assistWins[i]) }
        function snapFly(addr: string): void { snapper.openFly(addr, 900, 80, true) }
        function snapClose(): void { snapper.endAssist(); snapper.closeFly() }
        function debug(): string { return JSON.stringify(Hyprland.monitors.values.map(m => [m.name, m.activeWorkspace ? m.activeWorkspace.id : -1, m.activeWorkspace ? m.activeWorkspace.toplevels.values.length : -1, Hyprland.toplevels.values.filter(t => t.workspace && m.activeWorkspace && t.workspace.id === m.activeWorkspace.id).length])) }
    }
}
