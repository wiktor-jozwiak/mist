import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Greetd

// greetd greeter with the macOS look (MacLock.qml). Installed to /usr/share/mist-greeter by install-greeter.sh,
// started by greetd through noctalia-greeter-session (its wlroots compositor) via run-greeter.sh.
// greeter.json next to this file: { user, name, avatar, wallpaper, session: [cmd...], env: [K=V...] }
// Without GREETD_SOCK (nested test) it runs a dry mode: password "test" succeeds and quits.
ShellRoot {
    id: g
    property var cfg: ({})
    property bool busy: false
    property int failTick: 0
    property string message: ""
    property string pending: ""
    readonly property bool dry: !Greetd.available
    readonly property string here: Quickshell.shellDir

    function path(p) { return !p ? "" : "file://" + (p.startsWith("/") ? p : g.here + "/" + p) }

    function submit(pw) {
        if (busy) return
        busy = true; message = ""
        if (dry) { dryT.pw = pw; dryT.start(); return }
        pending = pw
        Greetd.createSession(cfg.user || "")
    }
    function fail(msg) {
        pending = ""; busy = false; message = msg; failTick++
        if (!dry && Greetd.state !== GreetdState.Inactive) Greetd.cancelSession()
    }

    Timer { id: dryT; property string pw; interval: 700
        onTriggered: { if (pw === "test") Qt.quit(); else g.fail("Nieprawidłowe hasło (tryb testowy: test)") } }

    Connections {
        target: Greetd
        enabled: !g.dry
        function onAuthMessage(message, error, responseRequired, echoResponse) {
            if (responseRequired) { Greetd.respond(g.pending); g.pending = "" }
            else { if (error) g.message = message; Greetd.respond("") }
        }
        function onAuthFailure(message) { g.fail("Nieprawidłowe hasło") }
        function onReadyToLaunch() { Greetd.launch(g.cfg.session || ["start-hyprland"], g.cfg.env || [], true) }
        function onError(error) { g.fail("Błąd greetd: " + error) }
    }

    FileView {
        path: g.here + "/greeter.json"
        blockLoading: true
        onLoaded: { try { g.cfg = JSON.parse(text()) } catch (e) { console.warn("greeter.json:", e) } }
    }
    // Segoe UI Variable copied next to the greeter (the greeter user cannot read ~/.local/share/fonts)
    FontLoader { source: g.path("SegUIVar.ttf") }

    Process { id: powerP }

    // the greeter compositor has no layer-shell: one xdg toplevel, it fullscreens it on the greeter output
    FloatingWindow {
        id: win
        title: "mist-greeter"
        color: "black"
        implicitWidth: 1920; implicitHeight: 1080
        MacLock {
            anchors.fill: parent
            login: true
            wallpaper: g.path(g.cfg.wallpaper)
            userName: g.cfg.name || g.cfg.user || ""
            avatar: g.path(g.cfg.avatar)
            busy: g.busy
            failTick: g.failTick
            message: g.message
            onSubmit: pw => g.submit(pw)
            onPower: a => { if (g.dry) { g.message = "tryb testowy: " + a; return } powerP.command = ["systemctl", a]; powerP.running = true }
        }
    }
}
