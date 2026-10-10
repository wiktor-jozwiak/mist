import QtQuick
import QtQuick.Layouts
import QtQuick.Shapes
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import Quickshell.Widgets
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Services.Mpris
import "components"

// Start (Win key), v6: one job first, find and launch. Left: search (prefixes: = licz, > terminal, ? sieć),
// favourites (most launched, then the apps.json groups; max 12) or A–Z, recent files. Right: agenda, now playing.
// Footer: prefix hints + power. Full-screen dim layer: click outside closes. Launches are counted in ~/.config/mist/usage.json and
// boost search ranking. Keys: ↑↓ results, Tab / ←→ (empty field) categories, Enter, Esc.
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: Ui.popup === "start" && Ui.screenName === modelData.name
    anchors { top: true; bottom: true; left: true; right: true }   // full screen: a click outside the sheet closes
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-start"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    property string query: ""
    property int sel: 0
    property string armed: ""
    property string powerTip: ""
    property int tab: 0                                  // 0 favourites, 1 all apps A–Z
    onVisibleChanged: {
        Data.sysActive = visible
        if (visible) { query = ""; search.text = ""; sel = 0; armed = ""; tab = 0; search.forceActiveFocus(); recentF.reload(); dlP.running = true }
    }
    function run(cmd) { Quickshell.execDetached(cmd); Ui.close() }

    // ---------------------------------------------------------------- data
    readonly property var groups: { try { return JSON.parse(appsF.text()).groups } catch (e) { return [] } }
    FileView { id: appsF; path: Quickshell.env("HOME") + "/.config/mist/apps.json"; watchChanges: true; onFileChanged: reload() }
    readonly property var tabs: ["Ulubione", "Wszystkie"]
    readonly property bool allTab: tab === 1
    readonly property var pinned: {
        const seen = {}, out = []
        for (const g of groups) for (const a of g.apps) if (!seen[a]) { seen[a] = true; out.push(a) }
        return out
    }
    // favourites: the most launched apps first, then the curated groups in order
    readonly property var tabApps: {
        const used = Object.keys(usage).sort((a, b) => usage[b].n - usage[a].n || usage[b].t - usage[a].t)
        const out = [], seen = {}
        for (const id of used.concat(pinned)) {
            const e = entryOf(id)
            if (!e || e.noDisplay || seen[e.id]) continue
            seen[e.id] = true; out.push(id)
            if (out.length === 12) break
        }
        return out
    }
    // keyboard focus in the app grid / A–Z list (-1 = none); arrows move it, Enter launches
    property int gsel: -1
    onTabChanged: gsel = -1
    readonly property int gcols: allTab ? 2 : 6
    readonly property int gcount: allTab ? allApps.length : tabApps.length
    function gmove(d) { gsel = gsel < 0 ? 0 : Math.max(0, Math.min(gcount - 1, gsel + d)) }
    function glaunch() { if (gsel >= 0) launch(allTab ? allApps[gsel] : entryOf(tabApps[gsel])) }
    readonly property var allApps: DesktopEntries.applications.values.filter(a => !a.noDisplay).sort((a, b) => a.name.localeCompare(b.name, "pl"))
    function entryOf(id) { DesktopEntries.applications.values; return DesktopEntries.byId(id) || DesktopEntries.heuristicLookup(id) }

    // launch counts: { desktopId: { n, t } }
    property var usage: ({})
    FileView {
        id: usageF; path: Quickshell.env("HOME") + "/.config/mist/usage.json"; printErrors: false
        onLoaded: { try { win.usage = JSON.parse(text()) } catch (e) {} }
    }
    function launch(entry) {
        if (!entry) return
        const u = Object.assign({}, usage), k = entry.id
        u[k] = { n: ((u[k] && u[k].n) || 0) + 1, t: Date.now() }
        usage = u
        usageF.setText(JSON.stringify(u))
        entry.execute(); Ui.close()
    }

    FileView { id: recentF; path: Quickshell.env("HOME") + "/.local/share/recently-used.xbel"; printErrors: false }
    readonly property var files: {
        const t = recentF.text() || ""
        const out = [], re = /<bookmark href="file:\/\/([^"]+)"[^>]*modified="([^"]+)"/g
        let m
        while ((m = re.exec(t)) !== null) out.push({ path: decodeURIComponent(m[1]), t: new Date(m[2]).getTime() })
        out.sort((a, b) => b.t - a.t)
        return out.slice(0, 80)
    }
    // newest files in Pobrane, Obrazy, Dokumenty, Pulpit, Wideo (most never reach recently-used.xbel)
    property var downloads: []
    Process {
        id: dlP
        running: true
        command: ["sh", "-c", "find \"$HOME/Pobrane\" \"$HOME/Downloads\" \"$HOME/Obrazy\" \"$HOME/Dokumenty\" \"$HOME/Pulpit\" \"$HOME/Wideo\" -maxdepth 2 -type f ! -name '.*' ! -name '*.part' ! -name '*.crdownload' -printf '%T@\\t%p\\n' 2>/dev/null | sort -rn | head -16"]
        stdout: StdioCollector {
            onStreamFinished: win.downloads = text.trim().split("\n").filter(l => l.indexOf("\t") > 0).map(l => { const i = l.indexOf("\t"); return { path: l.slice(i + 1), t: parseFloat(l.slice(0, i)) * 1000 } })
        }
    }
    // recently launched apps and recent files (xbel + downloads), newest first; sugFilter 0 all, 1 files, 2 apps
    property int sugFilter: 0
    readonly property var suggest: {
        const out = [], seen = {}
        for (const id in usage) { const e = entryOf(id); if (e && !e.noDisplay) out.push({ kind: "app", entry: e, t: usage[id].t }) }
        // newest first, then one entry per file "family": a.7z, a(1).7z, a (2).7z count as one
        const fl = files.slice(0, 12).concat(downloads).sort((a, b) => b.t - a.t)
        for (const f of fl) {
            const k = base(f.path).toLowerCase().replace(/\s*\(\d+\)(?=\.[^.]+$|$)/, "").replace(/\d+/g, "#")
            if (!seen[k]) { seen[k] = true; out.push({ kind: "file", path: f.path, t: f.t }) }
        }
        out.sort((a, b) => b.t - a.t)
        return out
    }
    readonly property var sugShown: suggest.filter(x => x.kind === "file").slice(0, 6)
    readonly property var agenda: Data.upcoming(3, 21)
    readonly property var player: { const ps = Mpris.players.values; return ps.find(p => p.isPlaying) || ps.find(p => p.trackTitle) || null }
    Timer { running: !!win.player && win.player.isPlaying; interval: 1000; repeat: true; onTriggered: win.player.positionChanged() }

    // ambient: dominant cover color (ColorQuantizer needs a local file, remote covers are cached first)
    readonly property string artUrl: player ? player.trackArtUrl || "" : ""
    property string artLocal: ""
    onArtUrlChanged: fetchArt()
    Component.onCompleted: fetchArt()
    function fetchArt() {
        if (!artUrl) artLocal = ""
        else if (artUrl.startsWith("file://")) artLocal = artUrl
        else { artP.running = false; artP.command = ["sh", "-c", 'f="${XDG_CACHE_HOME:-$HOME/.cache}/mist/art-$(printf %s "$1" | md5sum | cut -c1-16)"; mkdir -p "${f%/*}"; [ -s "$f" ] || curl -sfL --max-time 6 -o "$f" "$1"; [ -s "$f" ] && echo "$f"', "sh", artUrl]; artP.running = true }
    }
    Process { id: artP; stdout: StdioCollector { onStreamFinished: { const f = text.trim(); win.artLocal = f ? "file://" + f : "" } } }
    ColorQuantizer { id: cq; source: win.artLocal; depth: 3; rescaleSize: 64 }
    readonly property color ambient: {
        let best = null, bs = -1
        for (const c of cq.colors || []) {
            if (c.hslHue < 0) continue
            const sc = c.hslSaturation * (1 - Math.abs(c.hslLightness - 0.5) * 1.4)
            if (sc > bs) { bs = sc; best = c }
        }
        return best && bs > 0.12 ? Qt.hsla(best.hslHue, Math.min(0.78, Math.max(0.42, best.hslSaturation)), 0.72, 1)
            : best && bs > 0.03 ? Qt.hsla(best.hslHue, 0.34, 0.74, 1) : win.artLocal ? Qt.rgba(0.9, 0.92, 0.95, 1) : Theme.mAcc   // muted cover: its own soft hue, not the blue accent
    }
    property color ambientShown: ambient
    Behavior on ambientShown { ColorAnimation { duration: 700 } }

    function norm(s) { return (s || "").toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/ł/g, "l") }
    function base(p) { return p.split("/").pop() }
    function ago(t) {
        const s = (Data.now.getTime() - t) / 1000
        if (s < 90) return "przed chwilą"
        if (s < 3600) return Math.round(s / 60) + " min temu"
        if (s < 86400) return Math.round(s / 3600) + " godz. temu"
        const d = Math.round(s / 86400)
        return d === 1 ? "wczoraj" : d + " dni temu"
    }
    function fileKind(p) {
        const e = (base(p).split(".").pop() || "").toLowerCase()
        if (/^(png|jpe?g|webp|gif|svg|avif|heic|bmp)$/.test(e)) return { icon: "photo", c: Theme.mFg2 }
        if (/^(mp4|mkv|webm|mov|avi)$/.test(e)) return { icon: "video", c: Theme.mFg2 }
        if (/^(mp3|flac|ogg|wav|m4a|opus)$/.test(e)) return { icon: "music", c: Theme.mFg2 }
        if (/^(zip|7z|rar|tar|gz|xz|zst|bz2)$/.test(e)) return { icon: "fileZip", c: Theme.mFg2 }
        if (/^(pdf|docx?|odt|txt|md|rtf|xlsx?|ods|pptx?|odp|csv)$/.test(e)) return { icon: "fileText", c: Theme.mFg2 }
        if (/^(qml|js|ts|py|sh|json|c|cpp|h|rs|go|lua|css|html|toml|ya?ml|conf|ini)$/.test(e)) return { icon: "terminal", c: Theme.mFg2 }
        return { icon: "file", c: Theme.mFg2 }
    }
    // strict: plain arithmetic only (auto-detected); loose ("=" prefix): also Math functions, pi, e
    function calc(src, loose) {
        let s = src.trim()
        if (!s || !/[0-9]/.test(s)) return null
        if (!loose && !(/^[0-9+\-*/().,%^ ]+$/.test(s) && /[+\-*/^%]/.test(s))) return null
        if (loose && !/^[0-9a-z+\-*/().,%^ ]+$/i.test(s)) return null
        try {
            s = s.replace(/,/g, ".").replace(/\^/g, "**").replace(/[a-z]+[0-9]*/gi, w => {
                const k = w.toLowerCase()
                if (k === "pi") return "Math.PI"
                if (k === "e") return "Math.E"
                if (typeof Math[k] === "function") return "Math." + k
                throw 0
            })
            const v = Function("return (" + s + ")")()
            return isFinite(v) ? ("" + Math.round(v * 1e8) / 1e8).replace(".", ",") : null
        } catch (e) { return null }
    }
    function webItem(t) { return { kind: "web", cat: "Sieć", title: "Szukaj w sieci: " + t, q: t, sub: "Google", icon: "world", act: "Szukaj" } }
    function calcItem(v) { return { kind: "calc", cat: "Kalkulator", title: "= " + v, value: v, sub: "Enter kopiuje wynik", icon: "calc", act: "Kopiuj" } }

    readonly property string mode: query.startsWith(">") ? "Terminal" : query.startsWith("?") ? "Sieć" : query.startsWith("=") ? "Kalkulator" : ""
    readonly property var results: {
        const raw = query.trim()
        if (!raw) return []
        const out = []
        if (raw[0] === ">") { const c = raw.slice(1).trim(); if (c) out.push({ kind: "sh", cat: "Terminal", title: c, sub: "Uruchom w terminalu (Alacritty)", icon: "terminal", act: "Uruchom" }); return out }
        if (raw[0] === "?") { const t = raw.slice(1).trim(); if (t) out.push(webItem(t)); return out }
        if (raw[0] === "=") { const v = calc(raw.slice(1), true); if (v !== null) out.push(calcItem(v)); return out }
        const q = norm(raw)
        const v = calc(raw, false)
        if (v !== null) out.push(calcItem(v))

        const scored = []
        for (const a of allApps) {
            const n = norm(a.name), g = norm(a.genericName), k = norm((a.keywords || []).join(" ")), id = norm(a.id)
            const s = n.startsWith(q) ? 100 : n.split(/\s+/).some(w => w.startsWith(q)) ? 80 : n.includes(q) ? 60 : g.includes(q) || id.includes(q) ? 40 : k.includes(q) ? 30 : 0
            const u = usage[a.id]
            if (s) scored.push({ s: s - n.length / 100 + (u ? Math.min(25, u.n * 2.5) : 0), a })
        }
        scored.sort((x, y) => y.s - x.s)
        for (const x of scored.slice(0, 6)) out.push({ kind: "app", cat: "Aplikacje", title: x.a.name, sub: x.a.genericName || x.a.comment || "Aplikacja", entry: x.a, act: "Otwórz" })

        if (q.length >= 2) {
            let nf = 0
            for (const f of files) {
                if (nf >= 4) break
                if (norm(base(f.path)).includes(q)) { nf++; out.push({ kind: "file", cat: "Pliki", title: base(f.path), sub: f.path.replace(Quickshell.env("HOME"), "~").split("/").slice(0, -1).join("/") + " · " + ago(f.t), path: f.path, act: "Otwórz" }) }
            }
        }

        const cmds = [
            { k: "zablokuj lock", title: "Zablokuj ekran", cmd: [Quickshell.env("HOME") + "/.local/bin/mist-ui-bin", "lock"], icon: "lock" },
            { k: "ustawienia settings centrum sterowania", title: "Centrum sterowania", cmd: [Quickshell.env("HOME") + "/.local/bin/mist-qs", "-p", Quickshell.shellDir, "ipc", "call", "mist", "toggle", "control"], icon: "settings" },
            { k: "ustawienia settings wyglad qt motyw", title: "Wygląd aplikacji Qt", cmd: ["qt6ct"], icon: "settings" },
            { k: "ustawienia settings wyglad gtk motyw", title: "Wygląd aplikacji GTK", cmd: ["nwg-look"], icon: "settings" },
            { k: "wyloguj logout", title: "Wyloguj", cmd: ["loginctl", "terminate-session", Quickshell.env("XDG_SESSION_ID") || "self"], icon: "logout", risky: true },
            { k: "uruchom ponownie restart reboot", title: "Uruchom ponownie", cmd: ["systemctl", "reboot"], icon: "reload", risky: true },
            { k: "wylacz zamknij shutdown", title: "Wyłącz komputer", cmd: ["systemctl", "poweroff"], icon: "power", risky: true }
        ]
        for (const c of cmds) if (c.k.split(" ").some(w => w.startsWith(q))) out.push({ kind: "cmd", cat: "Polecenia", title: c.title, sub: c.risky ? "Enter dwa razy" : "Polecenie", icon: c.icon, cmd: c.cmd, risky: c.risky, act: "Wykonaj" })
        if (Phone.online) {
            const ph = [
                { k: "znajdz telefon zadzwon ring find", title: "Znajdź telefon", fn: () => Phone.ring(), icon: "bellRing", p: "findmyphone" },
                { k: "telefon schowek wyslij clipboard", title: "Wyślij schowek na telefon", fn: () => Phone.sendClipboard(), icon: "clipboard", p: "clipboard" },
                { k: "telefon wyslij pliki plik send", title: "Wyślij pliki na telefon", fn: () => Phone.pickFiles(), icon: "fileUp", p: "share" },
                { k: "telefon pliki przegladaj files", title: "Pliki telefonu", fn: () => Phone.browse(), icon: "folderOpen", p: "sftp" },
                { k: "telefon zdjecie aparat photo camera", title: "Zrób zdjęcie telefonem", fn: () => Phone.photo(), icon: "camera", p: "photo" },
                { k: "telefon sms wiadomosci message", title: "Wiadomości SMS", fn: () => Phone.sms(), icon: "message", p: "sms" }
            ]
            for (const c of ph) if (Phone.has(c.p) && c.k.split(" ").some(w => w.startsWith(q))) out.push({ kind: "fn", cat: "Telefon", title: c.title, sub: Phone.name, icon: c.icon, fn: c.fn, act: "Wykonaj" })
        }
        // the query itself to the phone: a link opens there, text lands in its clipboard
        if (Phone.has("share")) {
            const url = /^\S+:\/\/\S+$/.test(raw)
            out.push({ kind: "fn", cat: "Telefon", title: "Wyślij na telefon: " + raw, sub: Phone.name + (url ? " · otworzy link" : " · trafi do schowka"), icon: "send", fn: () => url ? Phone.share([raw]) : Phone.shareText(raw), act: "Wyślij" })
        }
        out.push(webItem(raw))
        return out
    }
    function activate(r) {
        if (!r) return
        if (r.kind === "app") launch(r.entry)
        else if (r.kind === "file") run(["xdg-open", r.path])
        else if (r.kind === "fn") { r.fn(); Ui.close() }
        else if (r.kind === "calc") { Quickshell.execDetached(["wl-copy", r.value.replace(",", ".")]); Ui.close() }
        else if (r.kind === "web") run(["xdg-open", "https://www.google.com/search?q=" + encodeURIComponent(r.q)])
        else if (r.kind === "sh") run(["alacritty", "-e", "sh", "-c", r.title + "; printf '\\n[Enter] zamyka'; read _"])
        else if (r.kind === "cmd") { if (r.risky && armed !== r.title) { armed = r.title; return } run(r.cmd) }
    }
    function power(name, cmd) { if (armed === name) run(cmd); else armed = name }

    readonly property string greeting: { const h = Data.now.getHours(); return h < 5 ? "Dobrej nocy" : h < 12 ? "Dzień dobry" : h < 18 ? "Cześć" : "Dobry wieczór" }
    readonly property string userName: Quickshell.env("USER") === "wiktor" ? "Wiktor" : Quickshell.env("USER")

    // ---------------------------------------------------------------- backdrop: dims the screen, click closes
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(3/255, 6/255, 12/255, 0.22)       // < ignore_alpha 0.25: the dim itself is not blurred
        NumberAnimation on opacity { from: 0; to: 1; duration: 180 }
    }
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onClicked: Ui.close() }

    // ---------------------------------------------------------------- sheet
    Item {
        id: sheet
        width: 1000; height: 700
        anchors.centerIn: parent; anchors.verticalCenterOffset: -18
        NumberAnimation on opacity { from: 0; to: 1; duration: 170; easing.type: Easing.OutCubic }
        NumberAnimation on scale { from: 0.965; to: 1; duration: 240; easing.type: Easing.OutCubic }
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }   // clicks on the sheet stay here

        LiquidGlass { anchors.fill: parent; radius: 32; tint: 0.38; shine: 1.25; shadowOpacity: 0.2; rims: false }   // shadow < ignore_alpha 0.25: no blurred box around the sheet
        // inner light, the same two hues as the desktop cards: teal from the wallpaper, a cool blue
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            StartGlow { w: sheet.width; h: sheet.height; rr: 32; cx: 120; cy: 40; r: 560; c: "#8ad6d0"; a: 0.10 }
            StartGlow { w: sheet.width; h: sheet.height; rr: 32; cx: 920; cy: 120; r: 460; c: "#9dbdf0"; a: 0.08 }
        }
        LiquidGlass { anchors.fill: parent; radius: 32; rimOnly: true; shine: 1.25 }

        // ------------------------------------------------ left
        Item {
            id: left
            x: 36; y: 32; width: 588; height: parent.height - 32 - 72

            // search
            Item {
                id: field
                width: parent.width; height: 52
                // focus halo
                Rectangle {
                    anchors.fill: parent; anchors.margins: -4; radius: 24
                    color: "transparent"; border.width: 4
                    border.color: Qt.rgba(138/255, 214/255, 208/255, search.activeFocus ? 0.12 : 0)
                    Behavior on border.color { ColorAnimation { duration: 160 } }
                }
                Rectangle {
                    anchors.fill: parent; radius: 20
                    color: Qt.rgba(4/255, 8/255, 16/255, 0.24)
                    border.width: 1; border.color: search.activeFocus ? Qt.rgba(138/255, 214/255, 208/255, 0.5) : Theme.rim
                    Behavior on border.color { ColorAnimation { duration: 160 } }
                    // inner top shadow = recessed field
                    Rectangle {
                        anchors.fill: parent; anchors.margins: 1; radius: 19
                        gradient: Gradient {
                            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.10) }
                            GradientStop { position: 0.35; color: Qt.rgba(0, 0, 0, 0) }
                        }
                    }
                }
                Icon { x: 20; anchors.verticalCenter: parent.verticalCenter; name: win.mode === "Terminal" ? "terminal" : win.mode === "Kalkulator" ? "calc" : win.mode === "Sieć" ? "world" : "search"; size: 19; color: search.activeFocus || win.mode ? Theme.mAcc : Theme.mMuted }
                TextInput {
                    id: search
                    x: 54; width: parent.width - 54 - hint.width - 28; anchors.verticalCenter: parent.verticalCenter
                    color: Theme.mFg; font.family: Theme.font; font.pixelSize: 17
                    selectionColor: Theme.mAccSoft; clip: true
                    onTextChanged: { win.query = text; win.sel = 0; win.armed = "" }
                    Keys.onEscapePressed: { if (text) text = ""; else Ui.close() }
                    Keys.onDownPressed: { if (text) win.sel = Math.min(win.results.length - 1, win.sel + 1); else win.gmove(win.gsel < 0 ? 0 : win.gcols) }
                    Keys.onUpPressed: { if (text) win.sel = Math.max(0, win.sel - 1); else if (win.gsel >= 0) win.gmove(-win.gcols) }
                    Keys.onReturnPressed: { if (text) win.activate(win.results[win.sel]); else win.glaunch() }
                    Keys.onEnterPressed: { if (text) win.activate(win.results[win.sel]); else win.glaunch() }
                    Keys.onTabPressed: { if (text) win.sel = (win.sel + 1) % Math.max(1, win.results.length); else win.tab = (win.tab + 1) % win.tabs.length }
                    Keys.onBacktabPressed: { if (text) win.sel = Math.max(0, win.sel - 1); else win.tab = (win.tab + win.tabs.length - 1) % win.tabs.length }
                    Keys.onLeftPressed: e => { if (!text) win.gmove(-1); else e.accepted = false }
                    Keys.onRightPressed: e => { if (!text) win.gmove(1); else e.accepted = false }
                    Txt { visible: !parent.text; text: "Szukaj aplikacji, plików i poleceń…"; size: 17; color: Theme.mFaint; anchors.verticalCenter: parent.verticalCenter }
                }
                Row {
                    id: hint
                    anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    Repeater {
                        model: win.mode ? [[win.mode, ""]] : []
                        Rectangle {
                            required property var modelData
                            implicitWidth: hr.implicitWidth + 16; implicitHeight: 26; radius: 9
                            color: win.mode ? Theme.mAccSoft : Theme.fill
                            border.width: 1; border.color: win.mode ? Qt.rgba(138/255, 214/255, 208/255, 0.3) : Theme.hair
                            Row {
                                id: hr; anchors.centerIn: parent; spacing: 5
                                Txt { text: modelData[0]; size: 11.5; w: 700; color: Theme.mAcc }
                                Txt { visible: !!modelData[1]; text: modelData[1]; size: 11; color: Theme.mMuted }
                            }
                        }
                    }
                }
            }

            // ---------------- search results
            ListView {
                id: list
                anchors.top: field.bottom; anchors.topMargin: 14; anchors.bottom: parent.bottom
                width: parent.width; clip: true; spacing: 2
                visible: win.query.trim() !== ""
                model: win.results
                currentIndex: win.sel
                onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
                boundsBehavior: Flickable.StopAtBounds
                section.property: "cat"
                section.delegate: Caps { required property string section; text: section; topPadding: 12; bottomPadding: 6; leftPadding: 6 }
                highlightFollowsCurrentItem: true
                highlightMoveDuration: 140
                highlight: Rectangle {
                    radius: 16
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: Qt.rgba(138/255, 214/255, 208/255, 0.20) }
                        GradientStop { position: 1; color: Qt.rgba(138/255, 214/255, 208/255, 0.05) }
                    }
                    border.width: 1; border.color: Qt.rgba(138/255, 214/255, 208/255, 0.22)
                }
                delegate: Item {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool cur: index === win.sel
                    readonly property bool warn: !!modelData.risky && win.armed === modelData.title
                    width: list.width; height: 58
                    Item {
                        x: 14; width: 36; height: 36; anchors.verticalCenter: parent.verticalCenter
                        AppIcon { anchors.fill: parent; size: 36; bare: true; tiled: true; visible: row.modelData.kind === "app"; icon: row.modelData.entry ? row.modelData.entry.icon : "" }
                        Chip {
                            visible: row.modelData.kind !== "app"; s: 36
                            readonly property var fk: row.modelData.kind === "file" ? win.fileKind(row.modelData.path) : null
                            icon: fk ? fk.icon : row.modelData.icon || "apps"
                            c: row.warn ? Theme.mBad : fk ? fk.c : row.modelData.kind === "calc" ? Theme.mAcc : row.modelData.kind === "sh" ? Theme.mWarn : Theme.mAcc
                        }
                    }
                    Column {
                        x: 64; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 64 - act.width - 26; spacing: 1
                        Txt { width: parent.width; text: row.modelData.title; size: row.modelData.kind === "calc" ? 18 : 14; w: 600; color: row.warn ? Theme.mBad : Theme.mFg }
                        Txt { width: parent.width; text: row.warn ? "Naciśnij jeszcze raz, aby potwierdzić" : row.modelData.sub; size: 11.5; color: Theme.mMuted; elide: Text.ElideMiddle }
                    }
                    Rectangle {
                        id: act
                        opacity: row.cur ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: 120 } }
                        anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                        implicitWidth: ar.implicitWidth + 18; implicitHeight: 26; radius: 9
                        color: row.warn ? Qt.rgba(238/255, 149/255, 163/255, 0.18) : Qt.rgba(1, 1, 1, 0.08)
                        Row {
                            id: ar; anchors.centerIn: parent; spacing: 7
                            Txt { text: row.warn ? "Potwierdź" : row.modelData.act || ""; size: 11.5; w: 500; color: row.warn ? Theme.mBad : Theme.mFg }
                            Txt { text: "↵"; size: 12; w: 600; color: Theme.mMuted }
                        }
                    }
                    MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onEntered: win.sel = row.index; onClicked: win.activate(row.modelData) }
                }
                Column {
                    visible: win.results.length === 0
                    anchors.horizontalCenter: parent.horizontalCenter; y: 60; spacing: 8
                    Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "search"; size: 30; color: Theme.mFaint }
                    Txt { anchors.horizontalCenter: parent.horizontalCenter; text: "Brak wyników"; size: 13; color: Theme.mMuted }
                }
            }

            // ---------------- home: segmented tabs, apps, suggestions
            Item {
                id: home
                anchors.top: field.bottom; anchors.topMargin: 26; anchors.bottom: parent.bottom
                width: parent.width
                visible: win.query.trim() === ""

                // section header: what the grid shows, the other view on the right
                RowLayout {
                    id: tabBar
                    width: parent.width; height: 20
                    Caps { text: win.allTab ? "Wszystkie aplikacje" : "Ulubione"; Layout.fillWidth: true }
                    Txt {
                        text: win.allTab ? "‹ Ulubione" : "Wszystkie ›"; size: Theme.fsSm; w: 600
                        color: swH.hovered ? Theme.mAcc : Theme.mFg2
                        HoverHandler { id: swH; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: win.tab = 1 - win.tab }
                    }
                }

                // pinned / group grid
                Grid {
                    id: grid
                    visible: !win.allTab
                    anchors.top: tabBar.bottom; anchors.topMargin: 12
                    x: -4; columns: 6; rowSpacing: 4; columnSpacing: 0
                    Repeater {
                        model: win.tabApps
                        AppCell { required property var modelData; required property int index; entry: win.entryOf(modelData); focused: index === win.gsel }
                    }
                    Connections { target: win; function onTabChanged() { tabAnim.restart() } }
                    ParallelAnimation {
                        id: tabAnim
                        NumberAnimation { target: grid; property: "opacity"; from: 0.2; to: 1; duration: 200 }
                        NumberAnimation { target: grid; property: "anchors.topMargin"; from: 22; to: 12; duration: 240; easing.type: Easing.OutCubic }
                    }
                }

                // all apps, A–Z
                GridView {
                    visible: win.allTab
                    anchors.top: tabBar.bottom; anchors.topMargin: 10; anchors.bottom: parent.bottom
                    x: -4; width: parent.width + 8; clip: true
                    cellWidth: width / 2; cellHeight: 52
                    boundsBehavior: Flickable.StopAtBounds
                    model: win.allTab ? win.allApps : []
                    currentIndex: win.allTab ? win.gsel : -1
                    onCurrentIndexChanged: if (currentIndex >= 0) positionViewAtIndex(currentIndex, GridView.Contain)
                    delegate: Item {
                        id: ac
                        required property var modelData
                        required property int index
                        width: GridView.view.cellWidth; height: 52
                        Rectangle { anchors.fill: parent; anchors.margins: 2; radius: 14; color: am.containsMouse || ac.index === win.gsel ? Qt.rgba(1, 1, 1, 0.07) : "transparent"; border.width: 1; border.color: ac.index === win.gsel ? Qt.rgba(138/255, 214/255, 208/255, 0.35) : am.containsMouse ? Theme.hair : "transparent" }
                        AppIcon { x: 12; anchors.verticalCenter: parent.verticalCenter; size: 32; bare: true; tiled: true; icon: ac.modelData.icon }
                        Column {
                            x: 56; width: parent.width - 66; anchors.verticalCenter: parent.verticalCenter
                            Txt { width: parent.width; text: ac.modelData.name; size: 13; w: 500 }
                            Txt { width: parent.width; text: ac.modelData.genericName || ac.modelData.comment || ""; size: 11; color: Theme.mMuted; visible: text !== "" }
                        }
                        MouseArea { id: am; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.launch(ac.modelData) }
                    }
                }

                // suggestions: filter chips, rich rows (image thumbnails, file types, app icons), show in folder on hover
                Column {
                    visible: !win.allTab && win.sugShown.length > 0
                    anchors.top: grid.bottom; anchors.topMargin: 26; width: parent.width; spacing: 12
                    RowLayout {
                        width: parent.width
                        Caps { text: "Ostatnie pliki"; Layout.fillWidth: true }
                    }
                    Grid {
                        columns: 2; columnSpacing: 12; rowSpacing: 10
                        Repeater {
                            model: win.sugShown
                            Rectangle {
                                id: sg
                                required property var modelData
                                readonly property var fk: modelData.kind === "file" ? win.fileKind(modelData.path) : null
                                readonly property bool img: !!fk && fk.icon === "photo" && !/\.svg$/i.test(modelData.path)
                                readonly property string ext: fk ? (win.base(modelData.path).split(".").pop() || "").slice(0, 4).toUpperCase() : ""
                                width: (588 - 12) / 2; height: 58; radius: 16
                                color: sgHover.hovered ? Qt.rgba(1, 1, 1, 0.07) : Qt.rgba(1, 1, 1, 0.025)
                                border.width: 1; border.color: sgHover.hovered ? Theme.hair : "transparent"
                                Behavior on color { ColorAnimation { duration: 120 } }
                                // thumbnail / type tile / app icon
                                Item {
                                    id: th; x: 10; width: 40; height: 40; anchors.verticalCenter: parent.verticalCenter
                                    AppIcon { anchors.centerIn: parent; size: 34; bare: true; tiled: true; visible: !sg.fk; icon: sg.modelData.entry ? sg.modelData.entry.icon : "" }
                                    Rectangle {
                                        visible: !!sg.fk && !sg.img
                                        anchors.fill: parent; radius: 11
                                        color: Qt.rgba(1, 1, 1, 0.06)
                                        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.08)
                                        Icon { anchors.centerIn: parent; name: sg.fk ? sg.fk.icon : ""; size: 19; color: sg.fk ? sg.fk.c : Theme.mFg2 }
                                    }
                                    ClippingRectangle {
                                        visible: sg.img
                                        anchors.fill: parent; radius: 11; color: Theme.fill2
                                        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.12)
                                        Image { anchors.fill: parent; source: sg.img ? "file://" + sg.modelData.path : ""; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(96, 96); asynchronous: true }
                                    }
                                }
                                Column {
                                    x: 60; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 60 - side.width - 18; spacing: 2
                                    Txt { width: parent.width; text: sg.fk ? win.base(sg.modelData.path) : sg.modelData.entry.name; size: 12.5; w: 600; elide: sg.fk ? Text.ElideMiddle : Text.ElideRight }
                                    Row {
                                        spacing: 5
                                        Icon { name: sg.fk ? "folder" : "clock"; size: 11; color: Theme.mFaint; anchors.verticalCenter: parent.verticalCenter }
                                        Txt { text: sg.fk ? (sg.modelData.path.split("/").slice(-2, -1)[0] || "/") : "uruchomiono " + win.ago(sg.modelData.t); size: 11; color: Theme.mMuted; anchors.verticalCenter: parent.verticalCenter }
                                    }
                                }
                                // time, or "show in folder" while hovered
                                Item {
                                    id: side
                                    anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                                    width: 64; height: 30
                                    Txt { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; visible: !(sgHover.hovered && sg.fk); text: sg.fk ? win.ago(sg.modelData.t) : ""; size: 10.5; color: Theme.mFaint }
                                    Btn {
                                        visible: sgHover.hovered && !!sg.fk
                                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                        icon: "folderOpen"; implicitWidth: 30; implicitHeight: 30; iconSize: 14; radius: 10
                                        color: hovered ? Theme.fill2 : Theme.fill
                                        onClicked: win.run([Quickshell.env("HOME") + "/.local/bin/mist-files", "--select", sg.modelData.path])
                                    }
                                }
                                HoverHandler { id: sgHover }
                                MouseArea {
                                    anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; z: -1
                                    onClicked: sg.fk ? win.run(["xdg-open", sg.modelData.path]) : win.launch(sg.modelData.entry)
                                }
                            }
                        }
                    }
                }
            }

        }

        // ------------------------------------------------ right column: stacked glass cards
        Rectangle {
            x: 652; y: 32; width: 1; height: parent.height - 32 - 72
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.0) }
                GradientStop { position: 0.5; color: Qt.rgba(1, 1, 1, 0.07) }
                GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0.0) }
            }
        }
        ColumnLayout {
            x: 676; y: 28; width: parent.width - 676 - 28; height: parent.height - 28 - 68
            spacing: 12

            // calendar: week strip (dots = events, click picks a day), that day's events, live countdown
            StartCard {
                id: cal
                Layout.fillWidth: true
                property int pick: 0                                         // days from today
                function dayAt(i) { return new Date(Data.now.getFullYear(), Data.now.getMonth(), Data.now.getDate() + i) }
                readonly property date picked: dayAt(pick)
                readonly property var dayEvents: {
                    const t = Data.now.getTime() / 1000
                    return Data.eventsOn(picked).filter(e => pick > 0 || e.end > t)
                }
                readonly property var soon: Data.upcoming(3, 30)
                function plural(n, one, few, many) { return n === 1 ? one : n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 10 || n % 100 >= 20) ? few : many }
                function dur(s) {
                    const m = Math.max(1, Math.round(s / 60))
                    if (m < 60) return m + " min"
                    const h = Math.floor(m / 60), r = m % 60
                    return h < 24 ? h + " h" + (r ? " " + r + " min" : "") : Data.rel(Data.now.getTime() / 1000 + s)
                }
                function feedColor(e) { return e.holiday ? Theme.mBad : [Theme.mAcc, Theme.mBlue, Theme.mFg2, Theme.mWarn][(e.feed || 0) % 4] }
                // the forecast at the start of an event (5 days of hourly data; all-day events get the day's maximum)
                function wxAt(e) {
                    const W = Data.weather
                    if (e.holiday) return null
                    if (e.allDay) {
                        const s = new Date(e.start * 1000), k = s.getFullYear() + "-" + Data.two(s.getMonth() + 1) + "-" + Data.two(s.getDate())
                        const day = (W.days || []).find(x => x.date === k)
                        return day ? { icon: Theme.wxIcon(day.code, 1), t: day.max, wet: day.pop >= 50 } : null
                    }
                    const h = (W.hourly || []).find(x => x.t <= e.start && x.t + 3600 > e.start)
                    return h ? { icon: Theme.wxIcon(h.code, h.day), t: Math.round(h.temp), wet: (h.pop || 0) >= 50 } : null
                }
                // another timed event the same day overlapping this one
                function clash(e) {
                    if (e.allDay) return null
                    return Data.eventsOn(new Date(e.start * 1000)).find(o => o !== e && !o.allDay && o.start < e.end && o.end > e.start) || null
                }

                RowLayout {
                    width: parent.width; spacing: 8
                    HoverHandler { id: calHead; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: Ui.toggle("calendar", win.modelData.name) }
                    Caps { text: cal.pick === 0 ? "Dziś" : cal.pick === 1 ? "Jutro" : Data.days[cal.picked.getDay()]; color: calHead.hovered ? Theme.mFg : Theme.mFaint; Layout.fillWidth: true }
                    Txt {
                        size: Theme.fsXs; w: 550; color: Theme.mMuted
                        text: cal.dayEvents.length
                            ? cal.dayEvents.length + " " + cal.plural(cal.dayEvents.length, "wydarzenie", "wydarzenia", "wydarzeń") + (cal.pick === 0 ? " przed tobą" : "")
                            : (cal.pick === 0 ? "wolny dzień" : "brak wydarzeń") + (cal.soon.length ? " · następne " + Data.rel(cal.soon[0].start) : "")
                    }
                    Btn { icon: "plus"; implicitWidth: 26; implicitHeight: 26; iconSize: 13; color: hovered ? Theme.fill2 : Qt.rgba(1, 1, 1, 0.05); onClicked: win.run(["xdg-open", "https://calendar.google.com/calendar/r/eventedit"]) }
                }

                // week strip
                Row {
                    width: parent.width
                    Repeater {
                        model: 7
                        Item {
                            id: dc
                            required property int index
                            readonly property date d: cal.dayAt(index)
                            readonly property var evs: Data.eventsOn(d)
                            readonly property bool on: cal.pick === index
                            width: parent.width / 7; height: 54
                            Rectangle {
                                anchors.fill: parent; anchors.margins: 2; radius: 13
                                color: dc.on ? Theme.mAccSoft : dm.containsMouse ? Theme.fill : "transparent"
                                border.width: 1; border.color: dc.on ? Qt.rgba(138/255, 214/255, 208/255, 0.3) : "transparent"
                                Behavior on color { ColorAnimation { duration: 120 } }
                            }
                            Column {
                                anchors.horizontalCenter: parent.horizontalCenter; y: 7; spacing: 2
                                Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Data.daysShort[dc.d.getDay()]; size: 9; w: 700; color: dc.on ? Theme.mAcc : Theme.mFaint; font.capitalization: Font.AllUppercase; font.letterSpacing: 0.5 }
                                Txt { anchors.horizontalCenter: parent.horizontalCenter; text: dc.d.getDate(); size: 15; w: dc.index === 0 || dc.on ? 700 : 500; color: dc.on ? Theme.mFg : dc.index === 0 ? Theme.mAcc : dc.d.getDay() === 0 ? Theme.mBad : Theme.mFg2 }
                                Row {
                                    anchors.horizontalCenter: parent.horizontalCenter; spacing: 3; height: 4
                                    Repeater {
                                        model: dc.evs.slice(0, 3)
                                        Rectangle { required property var modelData; width: 4; height: 4; radius: 2; color: cal.feedColor(modelData) }
                                    }
                                }
                            }
                            MouseArea { id: dm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cal.pick = dc.index }
                        }
                    }
                }

                // the picked day at a glance, 7–23 h: busy blocks, now
                Item {
                    id: dayBar
                    readonly property var timed: Data.eventsOn(cal.picked).filter(e => !e.allDay)
                    readonly property real t0: cal.picked.getTime() / 1000 + 7 * 3600
                    function xOf(ts) { return Math.max(0, Math.min(1, (ts - t0) / (16 * 3600))) * width }
                    visible: timed.length > 0
                    width: parent.width; height: 24
                    Rectangle { y: 3; width: parent.width; height: 6; radius: 3; color: Theme.mTrack }
                    Repeater {
                        model: dayBar.timed
                        Rectangle {
                            required property var modelData
                            x: dayBar.xOf(modelData.start); y: 3; height: 6; radius: 3
                            width: Math.max(6, dayBar.xOf(modelData.end) - x)
                            color: cal.clash(modelData) ? Theme.mWarn : cal.feedColor(modelData)
                        }
                    }
                    Rectangle {
                        visible: cal.pick === 0
                        x: dayBar.xOf(Data.now.getTime() / 1000) - 1; y: 0; width: 2; height: 12; radius: 1; color: Theme.mFg
                    }
                    Repeater {
                        model: [7, 11, 15, 19, 23]
                        Txt {
                            required property int modelData
                            required property int index
                            x: Math.max(0, Math.min(dayBar.width - width, (modelData - 7) / 16 * dayBar.width - width / 2)); y: 12
                            text: modelData; size: 9.5; w: 550; color: Theme.mFaint; font.features: { "tnum": 1 }
                        }
                    }
                }

                // the picked day
                Column {
                    width: parent.width; spacing: 4
                    Repeater {
                        model: cal.dayEvents.slice(0, 3)
                        Rectangle {
                            id: ev
                            required property var modelData
                            required property int index
                            readonly property real t: Data.now.getTime() / 1000
                            readonly property bool live: modelData.start <= t && modelData.end > t
                            readonly property bool first: cal.pick === 0 && index === 0
                            readonly property color tone: cal.feedColor(modelData)
                            width: parent.width; height: 42; radius: 12
                            color: live ? Qt.rgba(138/255, 214/255, 208/255, 0.10) : first ? Qt.rgba(1, 1, 1, 0.04) : "transparent"
                            readonly property var other: cal.clash(modelData)
                            Rectangle { x: 8; width: 3; height: 26; radius: 1.5; anchors.verticalCenter: parent.verticalCenter; color: ev.other ? Theme.mWarn : ev.live ? Theme.mAcc : ev.tone }
                            Column {
                                x: 20; width: 40; anchors.verticalCenter: parent.verticalCenter
                                Txt { text: ev.modelData.allDay ? "cały" : Data.hm(new Date(ev.modelData.start * 1000)); size: 12; w: 600; font.features: { "tnum": 1 } }
                                Txt { text: ev.modelData.allDay ? "dzień" : Data.hm(new Date(ev.modelData.end * 1000)); size: 10; color: Theme.mFaint; font.features: { "tnum": 1 } }
                            }
                            WxChip { id: evWx; w: cal.wxAt(ev.modelData); anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter }
                            Column {
                                x: 66; width: parent.width - 74 - (evWx.visible ? evWx.width + 8 : 0); anchors.verticalCenter: parent.verticalCenter; spacing: 1
                                Txt { width: parent.width; text: ev.modelData.title; size: 12.5; w: 600 }
                                Txt {
                                    width: parent.width; size: 10.5
                                    color: ev.other ? Theme.mWarn : ev.live ? Theme.mAcc : ev.first ? Theme.mAcc : Theme.mMuted
                                    text: ev.other ? "koliduje z: " + ev.other.title
                                        : ev.live ? "trwa · zostało " + cal.dur(ev.modelData.end - ev.t)
                                        : ev.first && !ev.modelData.allDay ? "za " + cal.dur(ev.modelData.start - ev.t) + (ev.modelData.where ? " · " + ev.modelData.where : "")
                                        : ev.modelData.where || (ev.modelData.holiday ? "święto" : Data.when(ev.modelData))
                                }
                            }
                            // progress of a running event
                            Rectangle {
                                visible: ev.live
                                x: 66; anchors.bottom: parent.bottom; anchors.bottomMargin: 4; width: parent.width - 80; height: 2; radius: 1; color: Theme.fill2
                                Rectangle { height: 2; radius: 1; color: Theme.mAcc; width: parent.width * Math.min(1, (ev.t - ev.modelData.start) / Math.max(1, ev.modelData.end - ev.modelData.start)) }
                            }
                        }
                    }
                    // nothing that day: the next events (click jumps to that day in the strip)
                    Caps { visible: cal.dayEvents.length === 0; text: cal.soon.length ? "Najbliższe" : "Nic w kalendarzu na najbliższy miesiąc"; size: 10; leftPadding: 8; topPadding: 2; bottomPadding: 2 }
                    Repeater {
                        model: cal.dayEvents.length === 0 ? cal.soon : []
                        Rectangle {
                            id: se
                            required property var modelData
                            readonly property date d: new Date(modelData.start * 1000)
                            readonly property int off: Math.round((new Date(d.getFullYear(), d.getMonth(), d.getDate()) - cal.dayAt(0)) / 86400000)
                            width: parent.width; height: 40; radius: 12
                            color: seH.hovered ? Qt.rgba(1, 1, 1, 0.05) : "transparent"
                            HoverHandler { id: seH; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: { if (se.off >= 0 && se.off < 7) cal.pick = se.off; else Ui.toggle("calendar", win.modelData.name) } }
                            Rectangle { x: 8; width: 3; height: 24; radius: 1.5; anchors.verticalCenter: parent.verticalCenter; color: cal.feedColor(se.modelData) }
                            Column {
                                x: 20; width: 40; anchors.verticalCenter: parent.verticalCenter
                                Txt { text: Data.daysShort[se.d.getDay()]; size: 9.5; w: 700; color: Theme.mMuted; font.capitalization: Font.AllUppercase }
                                Txt { text: se.d.getDate() + "." + Data.two(se.d.getMonth() + 1); size: 11.5; w: 600; font.features: { "tnum": 1 } }
                            }
                            WxChip { id: seWx; w: cal.wxAt(se.modelData); anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter }
                            Column {
                                x: 66; width: parent.width - 74 - (seWx.visible ? seWx.width + 8 : 0); anchors.verticalCenter: parent.verticalCenter; spacing: 1
                                Txt { width: parent.width; text: se.modelData.title; size: 12.5; w: 600 }
                                Txt { width: parent.width; text: Data.rel(se.modelData.start) + " · " + Data.when(se.modelData); size: 10.5; color: Theme.mMuted }
                            }
                        }
                    }
                    Txt { visible: cal.dayEvents.length > 3; leftPadding: 8; text: "+" + (cal.dayEvents.length - 3) + " więcej"; size: 10.5; color: Theme.mFaint }
                }
            }

            PhoneCard { Layout.fillWidth: true; maxNotes: 1; onDone: Ui.close(); onExpand: Ui.toggle("phone", win.modelData.name) }

            Item { Layout.fillHeight: true }

            // now playing: the cover is the card. Blurred art fills it, the cover color lights the glass,
            // big art with a colored glow, full-width scrubber, centered transport with the play button glowing.
            Item {
                id: np
                visible: !!win.player
                Layout.fillWidth: true; implicitHeight: 146
                readonly property color amb: win.ambientShown
                readonly property bool playing: !!win.player && win.player.isPlaying
                readonly property bool canLen: !!win.player && win.player.lengthSupported && win.player.length > 0
                readonly property real f: canLen ? Math.min(1, win.player.position / win.player.length) : 0
                function mmss(s) { s = Math.max(0, Math.floor(s)); return Math.floor(s / 60) + ":" + Data.two(s % 60) }
                HoverHandler { id: npH }

                // colored halo under the card
                RectangularShadow {
                    anchors.fill: parent; radius: 24; blur: 34; spread: -10; offset.y: 10
                    color: Qt.rgba(np.amb.r, np.amb.g, np.amb.b, np.playing ? 0.22 : 0.08)
                    Behavior on color { ColorAnimation { duration: 400 } }
                }
                // blurred cover, slowly drifting while playing
                ClippingRectangle {
                    anchors.fill: parent; radius: 24; color: Qt.rgba(8/255, 12/255, 20/255, 0.6)
                    Image {
                        id: npImg; visible: false; source: win.artUrl
                        width: parent.width * 1.4; height: width; x: -parent.width * 0.2; y: (parent.height - height) / 2
                        fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(240, 240)
                        SequentialAnimation on rotation {
                            running: np.playing && win.visible; loops: Animation.Infinite
                            NumberAnimation { to: 8; duration: 9000; easing.type: Easing.InOutSine }
                            NumberAnimation { to: -8; duration: 9000; easing.type: Easing.InOutSine }
                        }
                    }
                    MultiEffect {
                        anchors.fill: npImg; source: npImg; rotation: npImg.rotation
                        blurEnabled: true; blur: 1; blurMax: 64; saturation: 0.55; brightness: -0.38
                        autoPaddingEnabled: false
                    }
                    // legibility: dark at the bottom, cover color bleeding from the left
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            GradientStop { position: 0; color: Qt.rgba(6/255, 9/255, 16/255, 0.12) }
                            GradientStop { position: 1; color: Qt.rgba(6/255, 9/255, 16/255, 0.62) }
                        }
                    }
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: Qt.rgba(np.amb.r, np.amb.g, np.amb.b, 0.16) }
                            GradientStop { position: 0.6; color: Qt.rgba(np.amb.r, np.amb.g, np.amb.b, 0) }
                        }
                    }
                }
                // glass rim: lit top edge, cover-tinted hairline
                Rectangle {
                    anchors.fill: parent; radius: 24; color: "transparent"
                    border.width: 1; border.color: Qt.rgba(np.amb.r, np.amb.g, np.amb.b, npH.hovered ? 0.42 : 0.26)
                    Behavior on border.color { ColorAnimation { duration: 160 } }
                }
                Rectangle {
                    x: 24; width: parent.width - 48; height: 1; y: 1
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0) }
                        GradientStop { position: 0.5; color: Qt.rgba(1, 1, 1, 0.28) }
                        GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0) }
                    }
                }

                // cover art with its own glow; click raises the player
                Item {
                    id: art; x: 14; y: 14; width: 60; height: 60
                    scale: artM.pressed ? 0.94 : artM.containsMouse ? 1.04 : 1
                    Behavior on scale { NumberAnimation { duration: 180; easing.type: Easing.OutBack } }
                    RectangularShadow {
                        anchors.fill: parent; radius: 14; blur: 18; spread: -2; offset.y: 6
                        color: Qt.rgba(np.amb.r * 0.5, np.amb.g * 0.5, np.amb.b * 0.5, 0.55)
                    }
                    ClippingRectangle {
                        anchors.fill: parent; radius: 14; color: Theme.fill2
                        Image { anchors.fill: parent; source: win.artUrl; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(160, 160) }
                        Icon { anchors.centerIn: parent; name: "music"; size: 24; color: np.amb; visible: !win.artUrl }
                    }
                    Rectangle { anchors.fill: parent; radius: 14; color: "transparent"; border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.14) }
                    MouseArea { id: artM; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: if (win.player && win.player.canRaise) { win.player.raise(); Ui.close() } }
                }

                Column {
                    anchors.left: art.right; anchors.leftMargin: 14; anchors.right: parent.right; anchors.rightMargin: 16
                    anchors.verticalCenter: art.verticalCenter; spacing: 2
                    Row {
                        spacing: 6
                        // live equalizer next to the source
                        Row {
                            anchors.verticalCenter: parent.verticalCenter; spacing: 2; height: 10
                            Repeater {
                                model: 3
                                Rectangle {
                                    required property int index
                                    anchors.bottom: parent.bottom; width: 2.5; radius: 1.25; color: np.amb
                                    height: 4
                                    SequentialAnimation on height {
                                        running: np.playing && win.visible; loops: Animation.Infinite
                                        NumberAnimation { to: 10; duration: 260 + index * 90; easing.type: Easing.InOutSine }
                                        NumberAnimation { to: 3; duration: 300 + index * 70; easing.type: Easing.InOutSine }
                                    }
                                }
                            }
                        }
                        Txt { text: win.player ? win.player.identity : ""; size: 9.5; w: 700; color: np.amb; font.capitalization: Font.AllUppercase; font.letterSpacing: 1.2; anchors.verticalCenter: parent.verticalCenter }
                    }
                    Txt { width: parent.width; text: win.player ? win.player.trackTitle : ""; size: 16; w: 650; font.letterSpacing: -0.2 }
                    Txt { width: parent.width; text: win.player ? win.player.trackArtist : ""; size: 12.5; color: Theme.mFg2 }
                }

                // scrubber: thin, grows on hover, glowing head
                Item {
                    id: bar
                    x: 16; y: 84; width: parent.width - 32; height: 14
                    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: sk.containsMouse ? 6 : 4; radius: height / 2; color: Qt.rgba(1, 1, 1, 0.14); Behavior on height { NumberAnimation { duration: 120 } } }
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter; width: Math.max(height, parent.width * np.f); height: sk.containsMouse ? 6 : 4; radius: height / 2
                        Behavior on height { NumberAnimation { duration: 120 } }
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: Qt.rgba(np.amb.r, np.amb.g, np.amb.b, 0.55) }
                            GradientStop { position: 1; color: np.amb }
                        }
                    }
                    RectangularShadow {
                        visible: np.canLen
                        x: parent.width * np.f - 7; anchors.verticalCenter: parent.verticalCenter; width: 14; height: 14; radius: 7
                        blur: 10; color: Qt.rgba(np.amb.r, np.amb.g, np.amb.b, 0.8)
                        opacity: sk.containsMouse ? 1 : 0; Behavior on opacity { NumberAnimation { duration: 120 } }
                    }
                    Rectangle {
                        visible: np.canLen
                        x: parent.width * np.f - width / 2; anchors.verticalCenter: parent.verticalCenter
                        width: sk.containsMouse ? 12 : 0; height: width; radius: width / 2; color: "white"
                        Behavior on width { NumberAnimation { duration: 140; easing.type: Easing.OutBack } }
                    }
                    MouseArea {
                        id: sk; anchors.fill: parent; anchors.margins: -4; hoverEnabled: true
                        enabled: np.canLen && win.player.canSeek
                        cursorShape: Qt.PointingHandCursor
                        onClicked: m => win.player.position = Math.max(0, Math.min(1, (m.x - 4) / bar.width)) * win.player.length
                    }
                }

                // times at the ends, transport centered
                Txt { x: 16; anchors.verticalCenter: ctl.verticalCenter; text: np.canLen ? np.mmss(win.player.position) : ""; size: 11; color: Theme.mFg2; font.features: { "tnum": 1 } }
                Txt { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: ctl.verticalCenter; text: np.canLen ? "-" + np.mmss(win.player.length - win.player.position) : ""; size: 11; color: Theme.mMuted; font.features: { "tnum": 1 } }
                Row {
                    id: ctl
                    anchors.horizontalCenter: parent.horizontalCenter; y: 100; spacing: 14
                    Btn { icon: "prev"; anchors.verticalCenter: parent.verticalCenter; implicitWidth: 34; implicitHeight: 34; iconSize: 15; enabled: win.player && win.player.canGoPrevious; opacity: enabled ? 1 : 0.4; color: hovered ? Qt.rgba(1, 1, 1, 0.12) : "transparent"; onClicked: win.player.previous() }
                    Item {
                        width: 42; height: 42; anchors.verticalCenter: parent.verticalCenter
                        RectangularShadow {
                            anchors.fill: parent; radius: 21; blur: 16; offset.y: 3
                            color: Qt.rgba(np.amb.r, np.amb.g, np.amb.b, play.hovered ? 0.7 : 0.45)
                        }
                        Btn {
                            id: play; anchors.fill: parent
                            icon: np.playing ? "pause" : "play"; iconSize: 17
                            color: np.amb; iconColor: Theme.ink
                            scale: hovered ? 1.08 : 1; Behavior on scale { NumberAnimation { duration: 160; easing.type: Easing.OutBack } }
                            onClicked: win.player.togglePlaying()
                        }
                    }
                    Btn { icon: "next"; anchors.verticalCenter: parent.verticalCenter; implicitWidth: 34; implicitHeight: 34; iconSize: 15; enabled: win.player && win.player.canGoNext; opacity: enabled ? 1 : 0.4; color: hovered ? Qt.rgba(1, 1, 1, 0.12) : "transparent"; onClicked: win.player.next() }
                }
            }
        }

        // ------------------------------------------------ footer: search prefixes on the left, power on the right
        Rectangle { x: 28; y: parent.height - 64; width: parent.width - 56; height: 1; color: Theme.hair }
        Item {
            x: 36; y: parent.height - 52; width: parent.width - 64; height: 32
            Row {
                anchors.verticalCenter: parent.verticalCenter; spacing: 16
                visible: !win.armed && !win.powerTip
                Repeater {
                    model: [["=", "licz"], [">", "terminal"], ["?", "sieć"]]
                    Row {
                        required property var modelData
                        spacing: 6
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter; width: 20; height: 20; radius: 6
                            color: Qt.rgba(1, 1, 1, 0.06); border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.08)
                            Txt { anchors.centerIn: parent; text: modelData[0]; size: Theme.fsSm; w: 650; color: Theme.mFg2 }
                        }
                        Txt { anchors.verticalCenter: parent.verticalCenter; text: modelData[1]; size: Theme.fsXs; w: 550; color: Theme.mMuted }
                    }
                }
            }
            Txt {
                visible: !!win.armed || !!win.powerTip; anchors.verticalCenter: parent.verticalCenter
                text: win.armed ? "Kliknij jeszcze raz: " + win.armed.toLowerCase() : win.powerTip
                size: Theme.fsSm; w: 650; color: win.armed ? Theme.mBad : Theme.mFg2
            }
            Row {
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                Repeater {
                    model: [
                        ["lock", "Zablokuj", "", [Quickshell.env("HOME") + "/.local/bin/mist-ui-bin", "lock"]],
                        ["logout", "Wyloguj", "Wyloguj", ["loginctl", "terminate-session", Quickshell.env("XDG_SESSION_ID") || "self"]],
                        ["reload", "Uruchom ponownie", "Uruchom ponownie", ["systemctl", "reboot"]],
                        ["power", "Wyłącz", "Wyłącz", ["systemctl", "poweroff"]]
                    ]
                    Rectangle {
                        id: pb
                        required property var modelData
                        readonly property bool armedHere: !!modelData[2] && win.armed === modelData[2]
                        width: 34; height: 34; radius: 17
                        color: armedHere ? Qt.rgba(238/255, 149/255, 163/255, 0.2) : pbH.hovered ? Qt.rgba(1, 1, 1, 0.09) : "transparent"
                        Behavior on color { ColorAnimation { duration: 120 } }
                        Icon { anchors.centerIn: parent; name: pb.modelData[0]; size: 15; color: pb.armedHere || pb.modelData[0] === "power" && pbH.hovered ? Theme.mBad : pbH.hovered ? Theme.mFg : Theme.mFg2 }
                        // the name shows on the left of the footer, so the buttons never move
                        HoverHandler { id: pbH; cursorShape: Qt.PointingHandCursor; onHoveredChanged: win.powerTip = hovered ? pb.modelData[1] : win.powerTip === pb.modelData[1] ? "" : win.powerTip }
                        TapHandler { onTapped: pb.modelData[2] ? win.power(pb.modelData[2], pb.modelData[3]) : win.run(pb.modelData[3]) }
                    }
                }
            }
        }
    }

    // tracked caps label, the same as the desktop card headers
    component Caps: Txt { size: Theme.fsXs; w: 650; color: Theme.mFaint; font.capitalization: Font.AllUppercase; font.letterSpacing: 1.3 }

    // the forecast for an event: weather icon + temperature, blue when it will likely rain
    component WxChip: Row {
        property var w: null
        visible: !!w
        spacing: 4
        Icon { anchors.verticalCenter: parent.verticalCenter; name: parent.w ? (parent.w.wet ? "umbrella" : parent.w.icon) : ""; size: 13; color: parent.w && parent.w.wet ? Theme.mBlue : Theme.mFg2 }
        Txt { anchors.verticalCenter: parent.verticalCenter; text: parent.w ? parent.w.t + "°" : ""; size: 11.5; w: 600; color: Theme.mFg2; font.features: { "tnum": 1 } }
    }

    // pinned app: big bare icon + up to two lines of name, glass highlight on hover
    component AppCell: Item {
        id: cell
        property var entry
        property bool focused: false
        width: 99; height: 100
        visible: !!entry
        Rectangle {
            anchors.fill: parent; radius: 18
            color: cm.containsMouse || cell.focused ? Qt.rgba(1, 1, 1, 0.06) : "transparent"
            border.width: 1; border.color: cell.focused ? Qt.rgba(138/255, 214/255, 208/255, 0.35) : "transparent"
            Behavior on color { ColorAnimation { duration: 140 } }
        }
        AppIcon {
            y: 12; anchors.horizontalCenter: parent.horizontalCenter
            icon: cell.entry ? cell.entry.icon : ""; size: 48; bare: true; tiled: true
            scale: cm.pressed ? 0.88 : cm.containsMouse ? 1.1 : 1
            Behavior on scale { NumberAnimation { duration: 180; easing.type: Easing.OutBack } }
        }
        Txt {
            x: 6; y: 66; width: parent.width - 12
            horizontalAlignment: Text.AlignHCenter
            text: cell.entry ? cell.entry.name : ""; size: 11; w: 500; color: cm.containsMouse ? Theme.mFg : Theme.mFg2
            wrapMode: Text.Wrap; maximumLineCount: 2; lineHeight: 0.92
        }
        MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.launch(cell.entry) }
    }
}
