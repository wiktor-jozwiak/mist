import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Widgets
import ".."

// desktop app: bare icon (no label, the card shows the name of the hovered app), lifts and glows softly on hover
Item {
    id: t
    property string appId
    property color glow: Theme.accent
    readonly property var entry: { DesktopEntries.applications.values; return DesktopEntries.byId(appId) || DesktopEntries.heuristicLookup(appId) }
    readonly property bool hovered: ma.containsMouse
    readonly property string name: entry ? entry.name : ""
    visible: !!entry
    implicitHeight: 52
    Rectangle {
        anchors.fill: parent; anchors.margins: 2; radius: 14
        color: ma.containsMouse ? Qt.rgba(1, 1, 1, 0.07) : "transparent"
        Behavior on color { ColorAnimation { duration: 140 } }
    }
    Item {
        anchors.centerIn: parent
        anchors.verticalCenterOffset: ma.containsMouse ? -2 : 0
        width: 38; height: 38
        scale: ma.pressed ? 0.9 : 1
        Behavior on anchors.verticalCenterOffset { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
        Behavior on scale { NumberAnimation { duration: 100 } }
        RectangularShadow {
            anchors.fill: parent; anchors.margins: 6; radius: 14; blur: 22; spread: 0
            color: Qt.rgba(t.glow.r, t.glow.g, t.glow.b, ma.containsMouse ? 0.45 : 0)
            Behavior on color { ColorAnimation { duration: 180 } }
        }
        IconImage {
            anchors.centerIn: parent
            implicitSize: 34
            source: Quickshell.iconPath(t.entry ? t.entry.icon : "", "application-x-executable")
        }
    }
    MouseArea {
        id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
        onClicked: if (t.entry) t.entry.execute()
    }
}
