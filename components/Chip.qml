import QtQuick
import ".."

// tinted icon chip (card / folder headers)
Rectangle {
    property string icon
    property color c: Theme.accent
    property real s: 26
    width: s; height: s; radius: s * 0.36
    color: Qt.rgba(c.r, c.g, c.b, 0.16)
    border.width: 1; border.color: Qt.rgba(c.r, c.g, c.b, 0.22)
    Icon { anchors.centerIn: parent; name: parent.icon; size: parent.s * 0.56; color: parent.c }
}
