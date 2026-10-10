pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Notifications

// Mist's own notification server (org.freedesktop.Notifications; it was Noctalia's): the Dynamic Island shows
// each one (Island.qml connects to `arrived`), the Notification Centre (Notifications.qml) reads `history`.
// - history: newest first, at most `keep` entries, ~/.local/state/mist/notifications.json; picture data sent
//   inline (image-data) is saved as PNG by the island window (`saveImage`, a grab needs a window)
// - Do Not Disturb: only critical ones reach the island and make a sound; all go to the history
// - a notification stays tracked (its actions can be invoked) until the app closes it or `hold` ms pass
// - sound: freedesktop "message" (critical: "dialog-warning"), unless the app asks for silence
Singleton {
    id: root
    readonly property string dir: Quickshell.env("HOME") + "/.local/state/mist/"
    readonly property string imgDir: dir + "notif-img/"
    property var history: []
    property bool dnd: false
    readonly property int keep: 300
    readonly property int hold: 600000

    // one notification (new or an update of one already shown) for the island:
    // { nid, app, icon, summary, body, urgency (0..2), entry, rid, value (-1 or 0..100), sync, category, replaced }
    signal arrived(var o)
    // the island window saves `url` (image://qsimage/...) as PNG `file`, then calls imageSaved(file)
    signal saveImage(string url, string file)

    property var live: ({})            // nid -> Notification (tracked)

    NotificationServer {
        id: server
        keepOnReload: true
        persistenceSupported: true
        bodySupported: true
        bodyMarkupSupported: true
        bodyHyperlinksSupported: false
        bodyImagesSupported: false
        actionsSupported: true
        actionIconsSupported: false
        imageSupported: true
        inlineReplySupported: true
        onNotification: n => root.take(n)
    }

    function take(n) {
        n.tracked = true
        const l = Object.assign({}, live); l[n.id] = n; live = l
        n.closed.connect(() => root.forget(n.id))
        const upd = () => root.emit(n, true)
        n.summaryChanged.connect(upd)
        n.bodyChanged.connect(upd)
        n.hintsChanged.connect(upd)
        emit(n, false)
        if (!n.transient) record(n)
        if (!n.resident) Qt.callLater(() => expireLater(n.id))
    }
    function expireLater(nid) { const t = expT.createObject(root, { nid: nid }); t.start() }
    Component {
        id: expT
        Timer {
            property int nid
            interval: root.hold
            onTriggered: { const n = root.live[nid]; if (n) n.expire(); destroy() }
        }
    }
    function forget(nid) { if (live[nid]) { const l = Object.assign({}, live); delete l[nid]; live = l } }

    function hint(n, k) { const h = n.hints || {}; return h[k] === undefined || h[k] === null ? "" : h[k] }
    function emit(n, replaced) {
        const urgent = n.urgency === NotificationUrgency.Critical
        if (dnd && !urgent) return
        const v = hint(n, "value")
        arrived({
            nid: n.id, app: n.appName || "", icon: n.image || n.appIcon || "", summary: n.summary || "", body: n.body || "",
            urgency: urgent ? 2 : n.urgency === NotificationUrgency.Low ? 0 : 1, entry: n.desktopEntry || "",
            rid: replaced ? n.id : 0, value: v === "" ? -1 : +v,
            sync: "" + (hint(n, "x-canonical-private-synchronous") || hint(n, "x-dunst-stack-tag") || hint(n, "synchronous") || ""),
            category: "" + hint(n, "category"), replaced: replaced
        })
        if (!replaced && !hint(n, "suppress-sound") && n.urgency !== NotificationUrgency.Low)
            Quickshell.execDetached(["pw-play", "--volume=0.6", "/usr/share/sounds/freedesktop/stereo/" + (urgent ? "dialog-warning" : "message") + ".oga"])
    }

    // the default action (click), else false: the island then focuses the app
    function invoke(nid, ident) {
        const n = live[nid]
        if (!n) return false
        const a = n.actions.find(x => x.identifier === (ident || "default"))
        if (!a) return false
        a.invoke()
        return true
    }
    function dismiss(nid) { const n = live[nid]; if (n) n.dismiss() }

    // ------------------------------------------------------------ history
    function record(n) {
        let img = "", w = 0
        const u = n.image || ""
        if (u.startsWith("image://qsimage")) {
            img = imgDir + "n" + Date.now() + "_" + n.id + ".png"
            saveImage(u, img)
            img = "file://" + img
            w = 96
        } else if (u.startsWith("/")) { img = "file://" + u; w = 256 }
        else if (u.startsWith("file:")) { img = u; w = 256 }
        const icon = u.startsWith("image://icon/") ? u.slice(13) : n.appIcon || ""
        const e = { id: n.id, app_name: n.appName || "", summary: n.summary || "", body: n.body || "",
                    desktop_entry: n.desktopEntry || "", icon: icon, received_wall_ms: Date.now(),
                    urgency: n.urgency === NotificationUrgency.Critical ? "critical" : n.urgency === NotificationUrgency.Low ? "low" : "normal",
                    image: img, image_w: w }
        const h = [e].concat(history)
        const gone = h.slice(keep).map(x => x.image).filter(x => x && x.indexOf(imgDir) >= 0)
        if (gone.length) Quickshell.execDetached(["rm", "-f"].concat(gone.map(x => x.replace("file://", ""))))
        history = h.slice(0, keep)
        saveT.restart()
    }
    // the island reports the real size of a saved picture (small = avatar, big = thumbnail)
    function imageSaved(file, w) {
        const u = "file://" + file
        history = history.map(e => e.image === u ? Object.assign({}, e, { image_w: w }) : e)
        saveT.restart()
    }
    function clear() {
        Quickshell.execDetached(["sh", "-c", "rm -f \"$0\"/*.png", imgDir])
        history = []
        saveT.restart()
    }
    function setDnd(on) { dnd = on; saveT.restart() }

    property bool loaded: false
    Timer { id: saveT; interval: 400; onTriggered: if (root.loaded) histF.setText(JSON.stringify({ dnd: root.dnd, entries: root.history })) }
    FileView {
        id: histF
        path: root.dir + "notifications.json"
        printErrors: false
        onLoaded: {
            try { const d = JSON.parse(text()); root.history = (d.entries || []).concat(root.history).slice(0, root.keep); root.dnd = !!d.dnd } catch (e) {}
            root.loaded = true
        }
        onLoadFailed: root.loaded = true
    }
    Component.onCompleted: Quickshell.execDetached(["mkdir", "-p", imgDir])
}
