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
    function toggle(name, scr) {
        if (popup === name) { popup = ""; return }
        screenName = scr || focusedScreen()
        popup = name
    }
    function close() { popup = "" }
    // bar icons open the Control centre straight on their page; the same icon again closes it
    function control(pg, scr) {
        if (popup === "control" && page === pg) { popup = ""; return }
        page = pg
        screenName = scr || focusedScreen()
        popup = "control"
    }
}
