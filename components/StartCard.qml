import QtQuick
import ".."

// glass card for the right column of Start: header row + content stacked in a column
Rectangle {
    default property alias content: cc.data
    implicitHeight: cc.implicitHeight + 32
    radius: 22
    color: Qt.rgba(1, 1, 1, 0.03)
    border.width: 1; border.color: Theme.hair
    Rectangle {
        anchors.fill: parent; anchors.margins: 1; radius: 21
        gradient: Gradient { GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.025) } GradientStop { position: 0.4; color: Qt.rgba(1, 1, 1, 0) } }
    }
    Column { id: cc; x: 18; y: 16; width: parent.width - 36; spacing: 14 }
}
