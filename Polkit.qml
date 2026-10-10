import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Services.Polkit
import "components"

// Polkit authentication agent (was Noctalia's): an app asks for administrator rights (pkexec, GParted,
// systemd units, package managers) -> macOS-style sheet on the focused monitor: what asks, the user,
// the password field. Enter confirms, Esc cancels; a wrong password shakes the card and asks again.
Scope {
    id: root
    PolkitAgent { id: agent }
    readonly property var flow: agent.flow
    readonly property bool asking: agent.isActive && !!flow

    PanelWindow {
        id: win
        screen: Quickshell.screens.find(s => s.name === (Hyprland.focusedMonitor ? Hyprland.focusedMonitor.name : "DP-3")) || Quickshell.screens[0]
        visible: root.asking
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-session"
        WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        color: "transparent"

        property bool busy: false
        onVisibleChanged: if (visible) { pw.text = ""; busy = false; pw.forceActiveFocus() }
        Connections {
            target: root.flow
            function onAuthenticationFailed() { win.busy = false; pw.text = ""; shake.restart(); pw.forceActiveFocus() }
            function onIsResponseRequiredChanged() { if (root.flow && root.flow.isResponseRequired) { win.busy = false; pw.forceActiveFocus() } }
        }
        function submit() {
            if (!root.flow || busy) return
            busy = true
            root.flow.submit(pw.text)
        }
        function cancel() { if (root.flow) root.flow.cancelAuthenticationRequest() }

        Rectangle {
            anchors.fill: parent
            color: Qt.rgba(4/255, 7/255, 14/255, 0.42)
            MouseArea { anchors.fill: parent }
        }

        Glass {
            id: card
            anchors.centerIn: parent
            width: 440; height: col.implicitHeight + 56
            radius: Theme.rSheet; tint: 0.82
            transform: Translate { id: shakeT }
            SequentialAnimation {
                id: shake
                NumberAnimation { target: shakeT; property: "x"; to: -14; duration: 50 }
                NumberAnimation { target: shakeT; property: "x"; to: 12; duration: 70 }
                NumberAnimation { target: shakeT; property: "x"; to: -8; duration: 70 }
                NumberAnimation { target: shakeT; property: "x"; to: 0; duration: 60 }
            }

            Column {
                id: col
                anchors.horizontalCenter: parent.horizontalCenter
                y: 28; width: parent.width - 64
                spacing: 10
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 56; height: 56; radius: 28; color: Theme.fill2
                    Icon { anchors.centerIn: parent; name: "lock"; size: 26; color: Theme.t1 }
                }
                Txt { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: "Wymagane uprawnienia administratora"; size: 15; w: 600 }
                Txt {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; elide: Text.ElideNone
                    text: root.flow ? root.flow.message : ""; size: 12.5; color: Theme.t2
                }
                Txt {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideMiddle
                    text: root.flow ? root.flow.actionId : ""; size: 10.5; color: Theme.t4
                }
                Item { width: 1; height: 4 }
                Txt {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter
                    text: {
                        const i = root.flow ? root.flow.selectedIdentity : null
                        return "Użytkownik: " + (i && i.displayName ? i.displayName : Quickshell.env("USER"))
                    }
                    size: 12.5; color: Theme.t2
                }
                Rectangle {
                    width: parent.width; height: 38; radius: 19
                    color: Theme.fill; border.width: 1; border.color: pw.activeFocus ? Theme.accent : Theme.rim
                    TextInput {
                        id: pw
                        anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16
                        verticalAlignment: TextInput.AlignVCenter
                        echoMode: root.flow && root.flow.responseVisible ? TextInput.Normal : TextInput.Password
                        passwordCharacter: "•"
                        color: Theme.t1; font.family: Theme.font; font.pixelSize: 14
                        selectionColor: Theme.accentSoft
                        enabled: !win.busy
                        Keys.onReturnPressed: win.submit()
                        Keys.onEnterPressed: win.submit()
                        Keys.onEscapePressed: win.cancel()
                        Txt {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: !pw.text
                            text: root.flow && root.flow.inputPrompt ? root.flow.inputPrompt.replace(/:\s*$/, "") : "Hasło"
                            size: 13.5; color: Theme.t4
                        }
                    }
                }
                Txt {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; elide: Text.ElideNone
                    visible: !!text
                    text: root.flow ? root.flow.supplementaryMessage : ""
                    size: 12; color: root.flow && root.flow.supplementaryIsError ? Theme.danger : Theme.t3
                }
                Item { width: 1; height: 4 }
                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 10
                    Rectangle {
                        width: 150; height: 34; radius: 17; color: cm.containsMouse ? Theme.fill2 : Theme.fill
                        Txt { anchors.centerIn: parent; text: "Anuluj"; size: 13.5; w: 550 }
                        MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.cancel() }
                    }
                    Rectangle {
                        width: 150; height: 34; radius: 17
                        color: win.busy ? Theme.fill2 : om.containsMouse ? Theme.accent : Theme.accentStrong
                        Txt { anchors.centerIn: parent; text: win.busy ? "Sprawdzanie…" : "Uwierzytelnij"; size: 13.5; w: 600; color: win.busy ? Theme.t2 : Theme.ink }
                        MouseArea { id: om; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.submit() }
                    }
                }
            }
        }
    }
}
