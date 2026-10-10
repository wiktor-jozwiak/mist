import QtQuick
import ".."

// media control: round, `big` = white play/pause disc, `lit` = accent (shuffle / loop on)
Item {
    id: c
    property string icon
    property bool big: false
    property bool lit: false
    signal hit()
    width: big ? 38 : 32; height: 38
    opacity: enabled ? 1 : 0.35
    Rectangle {
        anchors.centerIn: parent
        width: c.big ? 36 : 30; height: width; radius: width / 2
        color: c.big ? Theme.t1 : cma.containsMouse ? Theme.fill2 : "transparent"
        scale: cma.pressed ? 0.88 : 1
        Behavior on scale { NumberAnimation { duration: 110 } }
        Behavior on color { ColorAnimation { duration: 120 } }
        Icon { anchors.centerIn: parent; name: c.icon; size: c.big ? 17 : 16; color: c.big ? Theme.ink : c.lit ? Theme.accent : Theme.t1 }
    }
    MouseArea { id: cma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: c.hit() }
}
