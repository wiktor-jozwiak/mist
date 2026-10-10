import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"

// Clipboard history (Super+V), styled like Spotlight: search capsule, list on the left, preview on the right.
// History: `wl-paste --watch mistd clipstore` -> ~/.cache/mist/clip (text + images, newest 80).
// Enter copies the item back (paste with Ctrl+V), Delete removes it, Esc closes.
Scope {
    id: cb
    readonly property string dir: Quickshell.env("HOME") + "/.cache/mist/clip"
    readonly property bool open: Ui.popup === "clipboard"
    property var items: []          // [{ f, kind: "text"|"image", text, t }]
    property string query: ""
    property int sel: 0

    // the watcher runs all the time; restarted if wl-paste dies
    Process {
        id: watch
        running: true
        command: ["setpriv", "--pdeathsig", "TERM", "wl-paste", "--watch", Quickshell.shellDir + "/scripts/bin/mistd", "clipstore"]
        onExited: restartT.start()
    }
    Timer { id: restartT; interval: 3000; onTriggered: watch.running = true }

    onOpenChanged: if (open) { query = ""; sel = 0; listP.running = true }
    Process {
        id: listP
        command: [Quickshell.shellDir + "/scripts/bin/mistd", "cliplist", cb.dir]
        stdout: StdioCollector { onStreamFinished: { try { cb.items = JSON.parse(text) } catch (e) { cb.items = [] } } }
    }
    readonly property var shown: {
        const q = query.trim().toLowerCase()
        return q ? items.filter(i => i.kind === "text" && i.text.toLowerCase().includes(q) || i.kind === "image" && "obraz image".includes(q)) : items
    }
    readonly property var cur: shown[sel] || null
    onQueryChanged: sel = 0

    function pick(i) {
        const it = shown[i]
        if (!it) return
        const ext = it.f.split(".").pop()
        Quickshell.execDetached(["sh", "-c", 'wl-copy --type "$2" < "$1"', "sh", it.f, it.kind === "image" ? "image/" + ext : "text/plain;charset=utf-8"])
        Ui.close()
    }
    function remove(i) {
        const it = shown[i]
        if (!it) return
        Quickshell.execDetached(["rm", "-f", it.f])
        items = items.filter(x => x.f !== it.f)
        sel = Math.min(sel, shown.length - 1)
    }
    function ago(t) {
        const s = (Date.now() - t) / 1000
        if (s < 60) return "przed chwilą"
        if (s < 3600) return Math.floor(s / 60) + " min temu"
        if (s < 86400) return Math.floor(s / 3600) + " godz. temu"
        return Math.floor(s / 86400) + " dni temu"
    }

    PanelWindow {
        id: win
        visible: cb.open
        screen: Quickshell.screens.find(s => s.name === Ui.screenName) || Quickshell.screens[0]
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-spotlight"
        WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        color: "transparent"
        HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }
        onVisibleChanged: if (visible) { field.text = ""; field.forceActiveFocus() }

        MouseArea { anchors.fill: parent; onClicked: Ui.close() }

        Glass {
            anchors.horizontalCenter: parent.horizontalCenter
            y: Math.round(win.height * 0.24)
            width: 820; height: 500
            radius: 30; tint: 0.76
            MouseArea { anchors.fill: parent }

            Item {
                id: bar
                width: parent.width; height: 60
                Icon { id: ico; name: "clipboard"; size: 22; color: Theme.t2; anchors { left: parent.left; leftMargin: 22; verticalCenter: parent.verticalCenter } }
                TextInput {
                    id: field
                    anchors { left: ico.right; leftMargin: 14; right: count.left; rightMargin: 12; verticalCenter: parent.verticalCenter }
                    color: Theme.t1; font.family: Theme.font; font.pixelSize: 22
                    selectionColor: Theme.accentSoft; clip: true
                    onTextChanged: cb.query = text
                    Txt { visible: field.text === ""; anchors.verticalCenter: parent.verticalCenter; text: "Szukaj w historii schowka"; size: 22; w: 400; color: Theme.t3 }
                    Keys.onPressed: e => {
                        if (e.key === Qt.Key_Escape) Ui.close()
                        else if (e.key === Qt.Key_Down) cb.sel = Math.min(cb.shown.length - 1, cb.sel + 1)
                        else if (e.key === Qt.Key_Up) cb.sel = Math.max(0, cb.sel - 1)
                        else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) cb.pick(cb.sel)
                        else if (e.key === Qt.Key_Delete) cb.remove(cb.sel)
                        else return
                        e.accepted = true
                    }
                }
                Txt { id: count; anchors { right: parent.right; rightMargin: 22; verticalCenter: parent.verticalCenter }
                    text: cb.items.length ? cb.shown.length + " / " + cb.items.length : ""; size: 12; color: Theme.t3 }
            }
            Rectangle { y: 60; x: 16; width: parent.width - 32; height: 1; color: Theme.hair }

            // list
            ListView {
                id: list
                x: 8; y: 68; width: 330; height: parent.height - 76
                clip: true
                model: cb.shown
                currentIndex: cb.sel
                highlightMoveDuration: 0
                onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    required property int index
                    width: list.width; height: 44; radius: 11
                    color: index === cb.sel ? Theme.accentSoft : rm.containsMouse ? Theme.fill : "transparent"
                    Rectangle {
                        id: kind
                        x: 8; width: 28; height: 28; radius: 8; anchors.verticalCenter: parent.verticalCenter
                        color: row.modelData.kind === "image" ? Qt.rgba(187/255, 169/255, 238/255, 0.18) : Theme.fill2
                        Icon { anchors.centerIn: parent; name: row.modelData.kind === "image" ? "photo" : /^\s*https?:\/\//.test(row.modelData.text) ? "link" : "fileText"; size: 15
                            color: row.modelData.kind === "image" ? Theme.violet : Theme.t2 }
                    }
                    Txt {
                        anchors { left: kind.right; leftMargin: 10; right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
                        text: row.modelData.kind === "image" ? "Obraz" : row.modelData.text.trim().replace(/\s+/g, " ")
                        size: 13.5; w: 480
                    }
                    MouseArea { id: rm; anchors.fill: parent; hoverEnabled: true; onClicked: cb.sel = row.index; onDoubleClicked: cb.pick(row.index) }
                }
                Txt { visible: cb.shown.length === 0; anchors.centerIn: parent; text: cb.items.length ? "Brak wyników" : "Historia schowka jest pusta"; size: 13; color: Theme.t3 }
            }
            Rectangle { x: 346; y: 72; width: 1; height: parent.height - 88; color: Theme.hair }

            // preview
            Item {
                x: 362; y: 72; width: parent.width - 362 - 18; height: parent.height - 72 - 18
                visible: !!cb.cur
                Rectangle {
                    anchors { fill: parent; bottomMargin: 54 }
                    radius: 14; color: Theme.inset
                    clip: true
                    Image {
                        anchors.fill: parent; anchors.margins: 10
                        visible: !!cb.cur && cb.cur.kind === "image"
                        source: cb.cur && cb.cur.kind === "image" ? "file://" + cb.cur.f : ""
                        fillMode: Image.PreserveAspectFit; asynchronous: true; cache: false
                    }
                    Flickable {
                        anchors.fill: parent; anchors.margins: 14
                        visible: !!cb.cur && cb.cur.kind === "text"
                        contentHeight: pv.implicitHeight; clip: true
                        TextEdit {
                            id: pv; width: parent.width; readOnly: true; selectByMouse: true
                            text: cb.cur && cb.cur.kind === "text" ? cb.cur.text : ""
                            wrapMode: TextEdit.WrapAnywhere; color: Theme.t1
                            font.family: /\n\s{2,}|[{};]$/m.test(text) ? "monospace" : Theme.font; font.pixelSize: 14
                        }
                    }
                }
                Row {
                    anchors.bottom: parent.bottom; width: parent.width
                    Column {
                        width: parent.width - 236; anchors.verticalCenter: parent.verticalCenter
                        Txt { text: cb.cur ? (cb.cur.kind === "image" ? "Obraz" : cb.cur.text.length + " znaków") : ""; size: 12.5; w: 550; color: Theme.t2 }
                        Txt { text: cb.cur ? "Skopiowano " + cb.ago(cb.cur.t) : ""; size: 11.5; color: Theme.t3 }
                    }
                    Row {
                        spacing: 8
                        Rectangle { width: 100; height: 34; radius: 17; color: dm.containsMouse ? Qt.rgba(238/255, 154/255, 170/255, 0.24) : Theme.fill
                            Txt { anchors.centerIn: parent; text: "Usuń  ⌦"; size: 12.5; w: 550; color: Theme.t2 }
                            MouseArea { id: dm; anchors.fill: parent; hoverEnabled: true; onClicked: cb.remove(cb.sel) } }
                        Rectangle { width: 120; height: 34; radius: 17; color: km.containsMouse ? Theme.accent : Theme.accentStrong
                            Txt { anchors.centerIn: parent; text: "Kopiuj  ↵"; size: 12.5; w: 600; color: Theme.ink }
                            MouseArea { id: km; anchors.fill: parent; hoverEnabled: true; onClicked: cb.pick(cb.sel) } }
                    }
                }
            }
        }
    }
}
