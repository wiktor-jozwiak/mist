import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// Idle: lock the session after `lockAfter` s without input, turn the monitors off after `offAfter` s
// (and back on with the first input). Apps that inhibit idle (video players, browsers playing a video,
// games) hold both off. Times in ~/.config/mist/idle.json {"lock": 600, "off": 900}; 0 = never.
Scope {
    id: root
    property int lockAfter: 600
    property int offAfter: 900

    FileView {
        path: Quickshell.env("HOME") + "/.config/mist/idle.json"
        watchChanges: true; printErrors: false
        onFileChanged: reload()
        onLoaded: {
            try { const c = JSON.parse(text()); root.lockAfter = c.lock ?? 600; root.offAfter = c.off ?? 900 } catch (e) {}
        }
    }
    function dpms(on) { Quickshell.execDetached(["hyprctl", "eval", "hl.dispatch(hl.dsp.dpms({ action = \"" + (on ? "on" : "off") + "\" }))"]) }

    IdleMonitor {
        enabled: root.lockAfter > 0
        timeout: Math.max(30, root.lockAfter)
        respectInhibitors: true
        onIsIdleChanged: if (isIdle) Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/mist-qs", "-p", Quickshell.shellDir, "ipc", "call", "lock", "lock"])
    }
    IdleMonitor {
        enabled: root.offAfter > 0
        timeout: Math.max(30, root.offAfter)
        respectInhibitors: true
        onIsIdleChanged: root.dpms(!isIdle)
    }
}
