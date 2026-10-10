import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"

// Screenshots like macOS Cmd+Shift+5: toolbar at the bottom (whole screen / window / selection, delay,
// clipboard, Capture), then a floating thumbnail bottom-right for 6 s: click edits it in Swash,
// the folder button opens ~/Obrazy/Zrzuty ekranu. Capture itself: scripts/shot.sh (grim + slurp).
// mist-qs … ipc call shot toolbar | screen | window | region   (Print, Super+Print, Shift+Print)
Scope {
    id: sh
    readonly property bool open: Ui.popup === "shot"
    property string mode: "region"        // screen | window | region
    property int delay: 0
    property bool copy: true
    property string last: ""
    property bool busy: false
    readonly property var screenObj: Quickshell.screens.find(s => s.name === Ui.focusedScreen()) || Quickshell.screens[0]

    function capture(m) {
        if (busy) return
        if (m) mode = m
        Ui.close()
        busy = true
        shotP.command = [Quickshell.shellDir + "/scripts/shot.sh", mode, "" + delay, copy ? "1" : "0"]
        shotP.running = true
    }
    Process {
        id: shotP
        stdout: StdioCollector { onStreamFinished: { const f = text.trim(); if (f) { sh.last = f; thumb.show() } } }
        onExited: sh.busy = false
    }

    IpcHandler {
        target: "shot"
        function toolbar(): void { Ui.toggle("shot", "") }
        function screen(): void { sh.capture("screen") }
        function window(): void { sh.capture("window") }
        function region(): void { sh.capture("region") }
    }

    // ---------------------------------------------------------------- toolbar
    PanelWindow {
        id: bar
        visible: sh.open
        screen: sh.screenObj
        anchors.bottom: true
        margins.bottom: 120
        exclusionMode: ExclusionMode.Ignore
        implicitWidth: tools.width + 20; implicitHeight: 74
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-popup"
        WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        color: "transparent"
        HyprlandFocusGrab { windows: [bar]; active: bar.visible; onCleared: Ui.close() }
        onVisibleChanged: if (visible) panel.forceActiveFocus()

        component Tool: Rectangle {
            id: t
            property string icon
            property string tip
            property bool on: false
            signal hit()
            width: 46; height: 42; radius: 11
            color: on ? Theme.accentSoft : tm.containsMouse ? Theme.fill2 : "transparent"
            border.width: on ? 1 : 0; border.color: Qt.rgba(169/255, 194/255, 238/255, 0.35)
            Icon { anchors.centerIn: parent; name: t.icon; size: 21; color: t.on ? Theme.accent : Theme.t1 }
            MouseArea { id: tm; anchors.fill: parent; hoverEnabled: true; onClicked: t.hit() }
            Rectangle {
                visible: tm.containsMouse
                anchors { bottom: parent.top; bottomMargin: 8; horizontalCenter: parent.horizontalCenter }
                width: tipT.implicitWidth + 16; height: 24; radius: 8; color: Theme.sheet
                Txt { id: tipT; anchors.centerIn: parent; text: t.tip; size: 11.5 }
            }
        }
        component Sep: Rectangle { width: 1; height: 28; color: Theme.hair; anchors.verticalCenter: parent.verticalCenter }
        component Chip: Rectangle {
            id: c
            property string text
            property bool on: false
            signal hit()
            width: ct.implicitWidth + 22; height: 32; radius: 16
            color: cm.containsMouse ? Theme.fill2 : Theme.fill
            Txt { id: ct; anchors.centerIn: parent; text: c.text; size: 12.5; w: 550; color: c.on ? Theme.t1 : Theme.t3 }
            MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; onClicked: c.hit() }
        }

        Glass {
            id: panel
            anchors.centerIn: parent
            width: tools.width + 16; height: 58
            radius: 18; tint: 0.8
            focus: true
            Keys.onPressed: e => {
                if (e.key === Qt.Key_Escape) Ui.close()
                else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) sh.capture("")
                else if (e.key === Qt.Key_1) sh.mode = "screen"
                else if (e.key === Qt.Key_2) sh.mode = "window"
                else if (e.key === Qt.Key_3) sh.mode = "region"
                else return
                e.accepted = true
            }
            Row {
                id: tools
                anchors.centerIn: parent
                spacing: 6
                Tool { icon: "x"; tip: "Zamknij (Esc)"; onHit: Ui.close() }
                Sep {}
                Tool { icon: "desktop"; tip: "Cały ekran"; on: sh.mode === "screen"; onHit: sh.mode = "screen" }
                Tool { icon: "window"; tip: "Wybrane okno"; on: sh.mode === "window"; onHit: sh.mode = "window" }
                Tool { icon: "screenshot"; tip: "Zaznaczony obszar"; on: sh.mode === "region"; onHit: sh.mode = "region" }
                Sep {}
                Chip { anchors.verticalCenter: parent.verticalCenter; text: sh.delay ? "Opóźnienie " + sh.delay + " s" : "Bez opóźnienia"; on: sh.delay > 0
                    onHit: sh.delay = sh.delay === 0 ? 5 : sh.delay === 5 ? 10 : 0 }
                Chip { anchors.verticalCenter: parent.verticalCenter; text: sh.copy ? "✓ Schowek" : "Schowek"; on: sh.copy; onHit: sh.copy = !sh.copy }
                Sep {}
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: capT.implicitWidth + 30; height: 36; radius: 18
                    color: capM.containsMouse ? Theme.accent : Theme.accentStrong
                    Txt { id: capT; anchors.centerIn: parent; text: "Przechwyć"; size: 13; w: 600; color: Theme.ink }
                    MouseArea { id: capM; anchors.fill: parent; hoverEnabled: true; onClicked: sh.capture("") }
                }
            }
        }
    }

    // ---------------------------------------------------------------- floating thumbnail
    PanelWindow {
        id: thumb
        property bool shown: false
        function show() { shown = true; hideT.restart() }
        visible: shown
        screen: sh.screenObj
        anchors { bottom: true; right: true }
        margins { bottom: 24; right: 24 }
        exclusionMode: ExclusionMode.Ignore
        implicitWidth: 260; implicitHeight: card.height + 20
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-popup"
        color: "transparent"
        Timer { id: hideT; interval: 6000; onTriggered: if (!hov.hovered) thumb.shown = false; else restart() }
        HoverHandler { id: hov }

        Glass {
            id: card
            x: 10; y: 10; width: 240; height: img.height + 52
            radius: 16; tint: 0.78
            Image {
                id: img
                x: 8; y: 8; width: parent.width - 16
                height: status === Image.Ready ? Math.min(150, width * implicitHeight / Math.max(1, implicitWidth)) : 120
                fillMode: Image.PreserveAspectFit
                source: sh.last ? "file://" + sh.last : ""; cache: false; asynchronous: true
                sourceSize.width: 480
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                    onClicked: { thumb.shown = false; Quickshell.execDetached(["swash", sh.last]) } }
            }
            Row {
                anchors { left: parent.left; leftMargin: 10; bottom: parent.bottom; bottomMargin: 8 }
                spacing: 6
                Btn { icon: "palette"; implicitWidth: 30; implicitHeight: 30; iconSize: 14; onClicked: { thumb.shown = false; Quickshell.execDetached(["swash", sh.last]) } }
                Btn { icon: "folderOpen"; implicitWidth: 30; implicitHeight: 30; iconSize: 14; onClicked: { thumb.shown = false; Quickshell.execDetached(["xdg-open", sh.last.substring(0, sh.last.lastIndexOf("/"))]) } }
            }
            Txt { anchors { right: closeB.left; rightMargin: 8; verticalCenter: closeB.verticalCenter }
                text: sh.copy ? "W schowku" : "Zapisano"; size: 11.5; color: Theme.t3 }
            Btn { id: closeB; icon: "x"; implicitWidth: 30; implicitHeight: 30; iconSize: 13
                anchors { right: parent.right; rightMargin: 10; bottom: parent.bottom; bottomMargin: 8 }
                onClicked: thumb.shown = false }
        }
    }
}
