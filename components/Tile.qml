import QtQuick

// lighter inner glass step inside a card
Rectangle {
    radius: 16
    color: Qt.rgba(1, 1, 1, 0.045)
    border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.07)
    Rectangle { x: parent.radius; width: parent.width - 2 * parent.radius; height: 1; y: 1; color: Qt.rgba(1, 1, 1, 0.08) }
}
