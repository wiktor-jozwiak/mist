import QtQuick
import ".."

// thin progress bar (4 px)
Rectangle {
    property real value: 0      // 0..1
    property color barColor: Theme.accent
    implicitHeight: 4
    radius: 2
    color: Theme.fill2
    Rectangle {
        width: parent.width * Math.max(0, Math.min(1, parent.value)); height: parent.height; radius: 2
        color: parent.barColor
        Behavior on width { NumberAnimation { duration: 400; easing.type: Easing.OutCubic } }
    }
}
