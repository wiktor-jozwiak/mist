import QtQuick
import Quickshell
import ".."

// icon + name, click launches the desktop entry
Item {
    id: t
    property string appId
    property real iconSize: 46
    readonly property var entry: { DesktopEntries.applications.values; return DesktopEntries.byId(appId) || DesktopEntries.heuristicLookup(appId) }
    signal launched()
    implicitHeight: col.implicitHeight
    visible: !!entry
    Rectangle {
        anchors.fill: parent; anchors.margins: -4; radius: 12
        color: ma.containsMouse ? Theme.fill : "transparent"
    }
    Column {
        id: col
        width: parent.width; spacing: 6
        AppIcon {
            anchors.horizontalCenter: parent.horizontalCenter
            icon: t.entry ? t.entry.icon : ""; size: t.iconSize
            scale: ma.pressed ? 0.92 : 1
            Behavior on scale { NumberAnimation { duration: 90 } }
        }
        Txt {
            width: parent.width; horizontalAlignment: Text.AlignHCenter
            text: t.entry ? t.entry.name : ""; size: 11; w: 500; color: Theme.t2
        }
    }
    MouseArea {
        id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
        onClicked: { if (t.entry) { t.entry.execute(); t.launched() } }
    }
}
