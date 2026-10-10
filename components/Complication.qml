import QtQuick
import ".."

// glass capsule under the desktop clock: icon, value, optional second icon + note
Item {
    id: cp
    property string icon
    property color c: Theme.accent
    property string text
    property string sub
    property string subIcon: ""
    property color subColor: Theme.orange
    implicitWidth: Math.min(320, cRow.implicitWidth + 28); implicitHeight: 38
    LiquidGlass { anchors.fill: parent; radius: height / 2; tint: 0.30; shadowOpacity: 0.14 }
    Row {
        id: cRow
        anchors.centerIn: parent
        spacing: 8
        Icon { anchors.verticalCenter: parent.verticalCenter; name: cp.icon; size: 16; color: cp.c }
        CTxt { anchors.verticalCenter: parent.verticalCenter; text: cp.text; size: 13.5; w: 650; color: Theme.t1; width: Math.min(implicitWidth, 170) }
        Icon { visible: cp.subIcon !== ""; anchors.verticalCenter: parent.verticalCenter; name: cp.subIcon; size: 15; color: cp.subColor }
        CTxt { anchors.verticalCenter: parent.verticalCenter; text: cp.sub; size: 12.5; w: 500; color: Theme.t2 }
    }
}
