import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"

// Session menu (Super+Alt+C, Control centre power button): dimmed screen, glass card with the user and
// five round actions. Restart / shut down / log out ask like macOS ("…automatically in 60 s").
// Keys 1–5, ←→ + Enter, Esc.
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: Ui.popup === "session" && Ui.screenName === modelData.name
    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-session"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    color: "transparent"
    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    property int sel: 4
    property var ask: null          // action waiting for confirmation
    property int left: 60
    property string fullName: ""
    onVisibleChanged: if (visible) { sel = 4; ask = null; keys.forceActiveFocus() }

    function mist(args) { return [Quickshell.env("HOME") + "/.local/bin/mist-qs", "-p", Quickshell.shellDir, "ipc", "call"].concat(args) }
    readonly property var actions: [
        { id: "lock", icon: "lock", t: "Zablokuj", cmd: mist(["lock", "lock"]) },
        { id: "logout", icon: "logout", t: "Wyloguj", cmd: ["loginctl", "terminate-session", Quickshell.env("XDG_SESSION_ID") || "self"],
          q: "Czy na pewno chcesz zamknąć wszystkie aplikacje i się wylogować?", n: "wylogujesz się" },
        { id: "suspend", icon: "zzz", t: "Uśpij", cmd: ["sh", "-c", "\"$0\" -p \"$1\" ipc call lock lock; sleep 0.6; systemctl suspend", Quickshell.env("HOME") + "/.local/bin/mist-qs", Quickshell.shellDir] },
        { id: "reboot", icon: "reload", t: "Uruchom ponownie", cmd: ["systemctl", "reboot"],
          q: "Czy na pewno chcesz teraz ponownie uruchomić komputer?", n: "komputer uruchomi się ponownie" },
        { id: "poweroff", icon: "power", t: "Wyłącz", cmd: ["systemctl", "poweroff"],
          q: "Czy na pewno chcesz teraz wyłączyć komputer?", n: "komputer wyłączy się" }
    ]
    function pick(i) {
        const a = actions[i]
        if (!a) return
        if (a.q && ask !== a) { ask = a; left = 60; return }
        run(a)
    }
    function run(a) { ask = null; Ui.close(); Quickshell.execDetached(a.cmd) }
    Timer { running: !!win.ask && win.visible; interval: 1000; repeat: true
        onTriggered: { win.left--; if (win.left <= 0) win.run(win.ask) } }

    Process {
        running: true
        command: ["getent", "passwd", Quickshell.env("USER")]
        stdout: StdioCollector { onStreamFinished: {
            const g = (text.split(":")[4] || "").split(",")[0].trim()
            win.fullName = (g || Quickshell.env("USER")).split(" ").map(s => s.charAt(0).toUpperCase() + s.slice(1).toLowerCase()).join(" ")
        } }
    }

    // dim + click outside closes
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(4/255, 7/255, 14/255, 0.42)
        MouseArea { anchors.fill: parent; onClicked: Ui.close() }
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onPressed: e => {
            if (e.key === Qt.Key_Escape) { if (win.ask) win.ask = null; else Ui.close() }
            else if (e.key >= Qt.Key_1 && e.key <= Qt.Key_5) win.pick(e.key - Qt.Key_1)
            else if (win.ask && (e.key === Qt.Key_Return || e.key === Qt.Key_Enter)) win.run(win.ask)
            else if (e.key === Qt.Key_Right || e.key === Qt.Key_Tab) win.sel = (win.sel + 1) % 5
            else if (e.key === Qt.Key_Left || e.key === Qt.Key_Backtab) win.sel = (win.sel + 4) % 5
            else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter || e.key === Qt.Key_Space) win.pick(win.sel)
            else return
            e.accepted = true
        }
    }

    Glass {
        id: card
        anchors.centerIn: parent
        width: 600; height: win.ask ? 250 : 292
        radius: Theme.rSheet; tint: 0.8
        Behavior on height { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        MouseArea { anchors.fill: parent }

        // ---------------- actions
        Column {
            visible: !win.ask
            anchors.horizontalCenter: parent.horizontalCenter
            y: 26
            spacing: 0
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 64; height: 64; radius: 32
                gradient: Gradient { GradientStop { position: 0; color: "#a9b1be" } GradientStop { position: 1; color: "#6c7482" } }
                Txt { anchors.centerIn: parent; text: win.fullName.split(" ").filter(s => s).slice(0, 2).map(s => s[0]).join(""); size: 24; w: 600; color: "white" }
            }
            Item { width: 1; height: 10 }
            Txt { anchors.horizontalCenter: parent.horizontalCenter; text: win.fullName; size: 17; w: 600 }
            Item { width: 1; height: 26 }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 14
                Repeater {
                    model: win.actions
                    delegate: Column {
                        id: act
                        required property var modelData
                        required property int index
                        readonly property bool on: index === win.sel
                        width: 100; spacing: 8
                        Rectangle {
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: 56; height: 56; radius: 28
                            color: act.on ? (act.modelData.id === "poweroff" ? Qt.rgba(238/255, 154/255, 170/255, 0.28) : Theme.accentSoft)
                                          : am.containsMouse ? Theme.fill2 : Theme.fill
                            border.width: act.on ? 1 : 0; border.color: Theme.rim
                            Behavior on color { ColorAnimation { duration: 120 } }
                            Icon { anchors.centerIn: parent; name: act.modelData.icon; size: 24; color: act.modelData.id === "poweroff" && act.on ? Theme.danger : Theme.t1 }
                            MouseArea { id: am; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                                onPositionChanged: win.sel = act.index; onClicked: win.pick(act.index) }
                        }
                        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: act.modelData.t; size: 12.5; w: 500; color: act.on ? Theme.t1 : Theme.t2 }
                        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: act.index + 1; size: 10.5; color: Theme.t4 }
                    }
                }
            }
        }

        // ---------------- macOS confirmation
        Column {
            visible: !!win.ask
            anchors.horizontalCenter: parent.horizontalCenter
            y: 30; width: parent.width - 80
            spacing: 10
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 52; height: 52; radius: 26; color: Theme.fill2
                Icon { anchors.centerIn: parent; name: win.ask ? win.ask.icon : ""; size: 24; color: Theme.t1 }
            }
            Txt { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; elide: Text.ElideNone
                text: win.ask ? win.ask.q : ""; size: 15; w: 600 }
            Txt { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; elide: Text.ElideNone
                text: win.ask ? "Jeśli nic nie zrobisz, " + win.ask.n + " automatycznie za " + win.left + " s." : ""; size: 12.5; color: Theme.t2 }
            Item { width: 1; height: 6 }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 10
                Rectangle {
                    width: 150; height: 34; radius: 17; color: cm.containsMouse ? Theme.fill2 : Theme.fill
                    Txt { anchors.centerIn: parent; text: "Anuluj"; size: 13.5; w: 550 }
                    MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; onClicked: win.ask = null }
                }
                Rectangle {
                    width: 150; height: 34; radius: 17; color: om.containsMouse ? Theme.accent : Theme.accentStrong
                    Txt { anchors.centerIn: parent; text: win.ask ? win.ask.t : ""; size: 13.5; w: 600; color: Theme.ink }
                    MouseArea { id: om; anchors.fill: parent; hoverEnabled: true; onClicked: win.run(win.ask) }
                }
            }
        }
    }
}
