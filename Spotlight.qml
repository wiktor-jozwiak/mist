import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"

// Spotlight (Super+Space / S / R): glass search capsule in the upper third, results drop out of it.
// Apps (ranked by ~/.config/mist/usage.json, shared with Start), calculator, commands, emoji, web.
// Emoji mode (Super+.): Ui.popup "emoji" or a ":" prefix -> emoji grid, Enter copies.
// Keys: ↑↓ (←→ in the emoji grid), Enter, Esc.
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: (Ui.popup === "spotlight" || Ui.popup === "emoji") && Ui.screenName === modelData.name
    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-spotlight"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    color: "transparent"
    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    property string query: ""
    property int sel: 0
    readonly property bool emojiMode: query.startsWith(":")
    onVisibleChanged: if (visible) {
        field.text = Ui.popup === "emoji" ? ":" : ""
        sel = 0; field.forceActiveFocus()
        usageF.reload()
    }
    function close() { Ui.close() }

    // ---------------------------------------------------------------- data
    readonly property var allApps: DesktopEntries.applications.values.filter(a => !a.noDisplay)
    property var usage: ({})
    FileView {
        id: usageF; path: Quickshell.env("HOME") + "/.config/mist/usage.json"; printErrors: false
        onLoaded: { try { win.usage = JSON.parse(text()) } catch (e) {} }
    }
    function launch(entry) {
        const u = Object.assign({}, usage), k = entry.id
        u[k] = { n: ((u[k] && u[k].n) || 0) + 1, t: Date.now() }
        usageF.setText(JSON.stringify(u))
        entry.execute(); close()
    }
    property var emoji: []
    FileView {
        path: "/usr/share/noctalia/assets/emoji.json"; printErrors: false
        onLoaded: { try { win.emoji = JSON.parse(text()) } catch (e) {} }
    }

    function norm(s) { return (s || "").toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/ł/g, "l") }
    function calc(src) {
        let s = src.trim()
        if (!/^[0-9+\-*/().,%^ ]+$/.test(s) || !/[0-9]/.test(s) || !/[+\-*/^%]/.test(s)) return null
        try {
            const v = Function("return (" + s.replace(/,/g, ".").replace(/\^/g, "**") + ")")()
            return isFinite(v) ? ("" + Math.round(v * 1e8) / 1e8).replace(".", ",") : null
        } catch (e) { return null }
    }
    function mist(args) { return [Quickshell.env("HOME") + "/.local/bin/mist-qs", "-p", Quickshell.shellDir, "ipc", "call"].concat(args) }
    readonly property var commands: [
        { k: "zablokuj lock ekran", title: "Zablokuj ekran", icon: "lock", cmd: mist(["lock", "lock"]) },
        { k: "wylacz uruchom ponownie restart reboot shutdown uspij sleep wyloguj logout sesja", title: "Wyłącz / uruchom ponownie…", icon: "power", cmd: mist(["mist", "toggle", "session"]) },
        { k: "tapeta wallpaper tlo", title: "Zmień tapetę", icon: "photo", cmd: mist(["mist", "toggle", "wallpaper"]) },
        { k: "schowek clipboard historia", title: "Historia schowka", icon: "clipboard", cmd: mist(["mist", "toggle", "clipboard"]) },
        { k: "zrzut ekranu screenshot", title: "Zrzut ekranu", icon: "screenshot", cmd: mist(["shot", "toolbar"]) },
        { k: "powiadomienia notifications", title: "Centrum powiadomień", icon: "bell", cmd: mist(["mist", "toggle", "notifications"]) },
        { k: "ustawienia settings centrum sterowania control", title: "Centrum sterowania", icon: "settings", cmd: mist(["mist", "toggle", "control"]) },
        { k: "ustawienia settings wyglad qt motyw theme", title: "Wygląd aplikacji Qt", icon: "settings", cmd: ["qt6ct"] },
        { k: "ustawienia settings wyglad gtk motyw theme", title: "Wygląd aplikacji GTK", icon: "settings", cmd: ["nwg-look"] }
    ]

    readonly property var results: {
        const raw = query.trim()
        if (!raw || emojiMode) return []
        const q = norm(raw), out = []
        const v = calc(raw)
        if (v !== null) out.push({ kind: "calc", title: v, sub: raw + " =", icon: "calc", value: v })
        const scored = []
        for (const a of allApps) {
            const n = norm(a.name), g = norm(a.genericName), k = norm((a.keywords || []).join(" ")), id = norm(a.id)
            const s = n.startsWith(q) ? 100 : n.split(/\s+/).some(w => w.startsWith(q)) ? 80 : n.includes(q) ? 60 : g.includes(q) || id.includes(q) ? 40 : k.includes(q) ? 30 : 0
            const u = usage[a.id]
            if (s) scored.push({ s: s - n.length / 100 + (u ? Math.min(25, u.n * 2.5) : 0), a })
        }
        scored.sort((x, y) => y.s - x.s)
        for (const x of scored.slice(0, 7)) out.push({ kind: "app", title: x.a.name, sub: x.a.genericName || x.a.comment || "Aplikacja", app: x.a.icon, entry: x.a })
        for (const c of commands) if (c.k.split(" ").some(w => w.startsWith(q))) out.push({ kind: "cmd", title: c.title, sub: "Polecenie", icon: c.icon, cmd: c.cmd })
        if (q.length >= 2) {
            let n = 0
            for (const e of emoji) {
                if (n >= 1) break
                if (norm(e.name).startsWith(q) || (e.keywords || []).some(w => norm(w) === q)) { n++; out.push({ kind: "emoji", title: e.emoji + "  " + e.name, sub: "Emoji · Enter kopiuje", glyph: e.emoji }) }
            }
        }
        out.push({ kind: "web", title: "Szukaj w sieci: " + raw, sub: "Google", icon: "world", q: raw })
        return out
    }
    readonly property var emojiResults: {
        if (!emojiMode) return []
        const q = norm(query.slice(1).trim())
        return (q ? emoji.filter(e => norm(e.name).includes(q) || (e.keywords || []).some(w => norm(w).startsWith(q))) : emoji).slice(0, 80)
    }
    readonly property int count: emojiMode ? emojiResults.length : results.length
    onQueryChanged: sel = 0

    function copy(t) { Quickshell.execDetached(["wl-copy", "--", t]) }
    function activate(i) {
        if (emojiMode) { const e = emojiResults[i]; if (e) { copy(e.emoji); close() } return }
        const r = results[i]
        if (!r) return
        if (r.kind === "app") launch(r.entry)
        else if (r.kind === "calc") { copy(r.value); close() }
        else if (r.kind === "emoji") { copy(r.glyph); close() }
        else if (r.kind === "cmd") { close(); Quickshell.execDetached(r.cmd) }
        else if (r.kind === "web") { Quickshell.execDetached(["xdg-open", "https://www.google.com/search?q=" + encodeURIComponent(r.q)]); close() }
    }

    // ---------------------------------------------------------------- view
    MouseArea { anchors.fill: parent; onClicked: win.close() }

    Glass {
        id: sheet
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.round(win.height * 0.31)
        width: 680
        height: 60 + (win.count > 0 ? body.height + 9 : 0)
        radius: 30
        tint: 0.74
        Behavior on height { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        clipContent: true
        MouseArea { anchors.fill: parent }   // swallow clicks inside

        // search capsule
        Item {
            id: bar
            width: parent.width; height: 60
            Icon { id: mag; name: "search"; size: 22; color: Theme.t2; anchors { left: parent.left; leftMargin: 22; verticalCenter: parent.verticalCenter } }
            TextInput {
                id: field
                anchors { left: mag.right; leftMargin: 14; right: parent.right; rightMargin: 22; verticalCenter: parent.verticalCenter }
                color: Theme.t1
                font.family: Theme.font; font.pixelSize: 24; font.weight: 400
                selectionColor: Theme.accentSoft
                clip: true
                onTextChanged: win.query = text
                Txt {
                    visible: field.text === ""
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Szukaj w Spotlight"; size: 24; w: 400; color: Theme.t3
                }
                Keys.onPressed: e => {
                    const cols = win.emojiMode ? 10 : 1
                    if (e.key === Qt.Key_Escape) win.close()
                    else if (e.key === Qt.Key_Down) win.sel = Math.min(win.count - 1, win.sel + cols)
                    else if (e.key === Qt.Key_Up) win.sel = Math.max(0, win.sel - cols)
                    else if (win.emojiMode && e.key === Qt.Key_Right && field.cursorPosition === field.text.length) win.sel = Math.min(win.count - 1, win.sel + 1)
                    else if (win.emojiMode && e.key === Qt.Key_Left && win.sel > 0) win.sel--
                    else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) win.activate(win.sel)
                    else return
                    e.accepted = true
                }
            }
        }
        Rectangle { visible: win.count > 0; y: 60; x: 16; width: parent.width - 32; height: 1; color: Theme.hair }

        Item {
            id: body
            y: 65; x: 8; width: parent.width - 16
            height: win.emojiMode ? Math.min(grid.contentHeight, 340) + 8 : list.contentHeight + 4

            ListView {
                id: list
                visible: !win.emojiMode
                anchors.fill: parent
                interactive: false
                model: win.emojiMode ? [] : win.results
                currentIndex: win.sel
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool on: index === win.sel
                    width: list.width; height: 48; radius: 12
                    color: on ? Theme.accentSoft : rm.containsMouse ? Theme.fill : "transparent"
                    // section caption on the first app / command
                    Item {
                        id: ico
                        x: 10; width: 32; height: 32; anchors.verticalCenter: parent.verticalCenter
                        AppIcon { anchors.fill: parent; visible: row.modelData.kind === "app"; icon: row.modelData.app || ""; size: 32; bare: true }
                        Rectangle {
                            anchors.fill: parent; radius: 9; visible: row.modelData.kind !== "app"
                            color: row.modelData.kind === "calc" ? Qt.rgba(236/255, 212/255, 142/255, 0.18) : Theme.fill2
                            Icon { anchors.centerIn: parent; visible: row.modelData.kind !== "emoji"; name: row.modelData.icon || ""; size: 18; color: row.modelData.kind === "calc" ? Theme.yellow : Theme.t1 }
                            Text { anchors.centerIn: parent; visible: row.modelData.kind === "emoji"; text: row.modelData.glyph || ""; font.pixelSize: 20 }
                        }
                    }
                    Column {
                        anchors { left: ico.right; leftMargin: 12; right: hint.left; rightMargin: 8; verticalCenter: parent.verticalCenter }
                        Txt { width: parent.width; text: row.modelData.title; size: row.modelData.kind === "calc" ? 18 : 15; w: 560 }
                        Txt { width: parent.width; text: row.modelData.sub; size: 12; color: Theme.t3 }
                    }
                    Txt { id: hint; anchors { right: parent.right; rightMargin: 14; verticalCenter: parent.verticalCenter }
                        visible: row.on; text: "↵"; size: 15; color: Theme.t3 }
                    MouseArea { id: rm; anchors.fill: parent; hoverEnabled: true; onPositionChanged: win.sel = row.index; onClicked: win.activate(row.index) }
                }
            }

            GridView {
                id: grid
                visible: win.emojiMode
                anchors.fill: parent; anchors.margins: 4
                cellWidth: Math.floor(width / 10); cellHeight: cellWidth
                clip: true
                model: win.emojiMode ? win.emojiResults : []
                currentIndex: win.sel
                onCurrentIndexChanged: positionViewAtIndex(currentIndex, GridView.Contain)
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    width: grid.cellWidth - 4; height: grid.cellHeight - 4; radius: 12
                    color: index === win.sel ? Theme.accentSoft : em.containsMouse ? Theme.fill : "transparent"
                    Text { anchors.centerIn: parent; text: modelData.emoji; font.pixelSize: 30 }
                    MouseArea { id: em; anchors.fill: parent; hoverEnabled: true; onClicked: win.activate(index) }
                }
            }
        }
    }
    // selected emoji name under the grid
    Txt {
        visible: win.emojiMode && win.emojiResults.length > 0
        anchors.horizontalCenter: sheet.horizontalCenter
        anchors.top: sheet.bottom; anchors.topMargin: 10
        text: win.emojiResults[win.sel] ? win.emojiResults[win.sel].name : ""
        size: 13; color: Theme.t2
    }
}
