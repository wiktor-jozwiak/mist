import QtQuick
import ".."

// round icon button with hover fill
Rectangle {
    id: b
    property string icon
    property real iconSize: 16
    property bool active: false
    property color iconColor: active ? Theme.ink : Theme.t1
    readonly property bool hovered: ma.containsMouse
    signal clicked()
    signal rightClicked()
    implicitWidth: 34; implicitHeight: 34; radius: height / 2
    color: active ? Theme.accent : ma.containsMouse ? Theme.fill2 : Theme.fill
    Behavior on color { ColorAnimation { duration: 120 } }
    Icon { anchors.centerIn: parent; name: b.icon; size: b.iconSize; color: b.iconColor }
    MouseArea {
        id: ma; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: m => m.button === Qt.RightButton ? b.rightClicked() : b.clicked()
    }
}
