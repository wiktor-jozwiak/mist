import QtQuick
import ".."

// icon (+ text) button of the top bar: no background, dims at rest and lights up on hover; click / middle / right click and wheel
Item {
    id: b
    property string icon
    property string text
    property color tint: Theme.t1
    property color textColor: tint
    property real iconSize: 15
    property real textSize: 13
    signal clicked(var mouse)
    signal scrolled(var wheel)
    implicitWidth: r.implicitWidth + 12
    implicitHeight: 24
    Row {
        id: r
        anchors.centerIn: parent
        opacity: ma.pressed ? 0.6 : ma.containsMouse ? 1 : 0.85
        scale: ma.pressed ? 0.94 : 1
        Behavior on opacity { NumberAnimation { duration: 120 } }
        Behavior on scale { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
        spacing: 5
        Icon { visible: b.icon !== ""; anchors.verticalCenter: parent.verticalCenter; name: b.icon; size: b.iconSize; color: b.tint }
        Txt {
            visible: b.text !== ""; anchors.verticalCenter: parent.verticalCenter
            text: b.text; size: b.textSize; w: 550; color: b.textColor
            font.features: ({ "tnum": 1 })
        }
    }
    MouseArea {
        id: ma
        anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        onClicked: m => b.clicked(m)
        onWheel: w => b.scrolled(w)
    }
}
