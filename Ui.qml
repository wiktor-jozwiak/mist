pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io

// which popup is open and on which monitor
Singleton {
    property string popup: ""          // "", "start", "calendar", "control"
    property string screenName: ""
    property string page: ""           // Control centre page: "" (home), "sound", "network", "bluetooth"
    // diagnostics (ipc call mist perf <key> <value>): sky "auto" | "fast" | "off"
    property string dbgSky: "auto"
    property bool dbgDock: false
    signal dbgGrab(string what, string path)     // ipc call mist perf grab <png>: the weather card as an image
    property var notifHidden: ({})     // Notification centre: group -> newest hidden received_wall_ms (session)
    // night light: Hyprland screen shader shaders/nightlight.glsl (~4000 K), remembered across restarts
    property bool nightLight: false
    function setNightLight(on) {
        nightLight = on
        Quickshell.execDetached(["hyprctl", "eval", "hl.config({ decoration = { screen_shader = \"" + (on ? Quickshell.shellDir + "/shaders/nightlight.glsl" : "") + "\" } })"])
        nlF.setText(on ? "1" : "0")
    }
    FileView {
        id: nlF
        path: Quickshell.env("HOME") + "/.local/state/mist/nightlight"
        printErrors: false
        onLoaded: if (text().trim() === "1") setNightLight(true)
    }
    function focusedScreen() { return Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "DP-3" }
    // popups drawn by mist-ui now (native/mist-ui pane.rs): opened there
    readonly property var native: ["session", "calendar"]
    function toggle(name, scr) {
        if (native.includes(name)) {
            popup = ""
            Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/mist-ui-bin", "msg", "mist", "toggleOn", name, scr || focusedScreen()])
            return
        }
        if (popup === name) { popup = ""; return }
        screenName = scr || focusedScreen()
        popup = name
    }
    function close() { popup = "" }
    // which popup is open, for the native bar (native/mist-ui): $XDG_RUNTIME_DIR/mist/ui.json
    readonly property string barState: JSON.stringify({ popup: popup, screen: screenName, page: page })
    onBarStateChanged: uiF.setText(barState)
    FileView { id: uiF; path: Quickshell.env("XDG_RUNTIME_DIR") + "/mist/ui.json"; printErrors: false }
    // bar icons open the Control centre straight on their page; the same icon again closes it
    function control(pg, scr) {
        if (popup === "control" && page === pg) { popup = ""; return }
        page = pg
        screenName = scr || focusedScreen()
        popup = "control"
    }
}
