import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Services.Pam
import "lock"

// macOS-style session lock (ext-session-lock, PAM "login"). Visual: lock/MacLock.qml, shared with the greeter.
// mist-qs -p ~/.config/quickshell/mist ipc call lock lock      (Super+L, Control centre)
// mist-qs -p ~/.config/quickshell/mist ipc call lock preview 8 (locks, unlocks itself after N s, max 30: for testing)
Scope {
    id: lk
    property bool busy: false
    property int failTick: 0
    property string message: ""
    property string pending: ""
    property string fullName: ""
    // the photo wallpaper (Mist draws it, LiveCfg.qml), also while a live one plays
    readonly property string wallpaper: LiveCfg.image
    // ~/.face when there is one, else MacLock draws the initials
    property string face: ""
    Process {
        running: true
        command: ["sh", "-c", "for f in \"$HOME/.face\" \"$HOME/.face.icon\" /var/lib/AccountsService/icons/\"$USER\"; do [ -s \"$f\" ] && { echo \"$f\"; exit; }; done"]
        stdout: StdioCollector { onStreamFinished: lk.face = text.trim() }
    }

    function lock() {
        if (sessionLock.locked) return
        message = ""; busy = false; pending = ""
        sessionLock.locked = true
    }
    function unlock() {
        pam.abort()
        pending = ""; busy = false; message = ""
        sessionLock.locked = false
    }
    function tryUnlock(password) {
        if (busy) return
        pending = password; busy = true; message = ""
        if (!pam.start()) { busy = false; pending = ""; message = "Nie można uruchomić PAM" }
    }

    WlSessionLock {
        id: sessionLock
        WlSessionLockSurface {
            id: surf
            color: "black"
            MacLock {
                anchors.fill: parent
                wallpaper: lk.wallpaper ? "file://" + lk.wallpaper : ""
                userName: lk.fullName
                avatar: lk.face ? "file://" + lk.face : ""
                primary: surf.screen && surf.screen.name === "DP-3"
                busy: lk.busy
                failTick: lk.failTick
                message: lk.message
                onSubmit: pw => lk.tryUnlock(pw)
            }
        }
    }

    PamContext {
        id: pam
        config: "login"
        user: Quickshell.env("USER")
        onPamMessage: {
            if (responseRequired) { respond(lk.pending); lk.pending = "" }
            else if (messageIsError || message) lk.message = message
        }
        onCompleted: result => {
            lk.pending = ""; lk.busy = false
            if (result === PamResult.Success) lk.unlock()
            else { lk.message = result === PamResult.MaxTries ? "Zbyt wiele prób, poczekaj chwilę" : "Nieprawidłowe hasło"; lk.failTick++ }
        }
        onError: err => { lk.pending = ""; lk.busy = false; lk.message = "Błąd PAM: " + PamError.toString(err); lk.failTick++ }
    }

    // full name from GECOS, word case normalised
    Process {
        running: true
        command: ["getent", "passwd", Quickshell.env("USER")]
        stdout: StdioCollector {
            onStreamFinished: {
                const g = (text.split(":")[4] || "").split(",")[0].trim()
                lk.fullName = (g || Quickshell.env("USER")).split(" ").map(s => s.charAt(0).toUpperCase() + s.slice(1).toLowerCase()).join(" ")
            }
        }
    }

    // logind: `loginctl lock-session` (Session.Lock on our session) and going to sleep (PrepareForSleep true)
    readonly property string sessionPath: "/org/freedesktop/login1/session/" + (Quickshell.env("XDG_SESSION_ID") || "auto").split("").map(c => /[A-Za-z]/.test(c) ? c : "_" + c.charCodeAt(0).toString(16)).join("")
    Process {
        id: logind
        running: true
        command: ["gdbus", "monitor", "--system", "--dest", "org.freedesktop.login1"]
        stdout: SplitParser {
            onRead: line => {
                if (line.indexOf("PrepareForSleep (true") >= 0 || line.startsWith(lk.sessionPath + ": org.freedesktop.login1.Session.Lock")) lk.lock()
            }
        }
        onExited: logindT.start()
    }
    Timer { id: logindT; interval: 5000; onTriggered: logind.running = true }

    Timer { id: previewT; onTriggered: lk.unlock() }

    IpcHandler {
        target: "lock"
        function lock(): void { lk.lock() }
        function preview(seconds: int): void {
            if (sessionLock.locked) return
            lk.lock()
            previewT.interval = Math.max(2, Math.min(30, seconds)) * 1000
            previewT.restart()
        }
        function isLocked(): bool { return sessionLock.locked }
    }
}
