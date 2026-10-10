import QtQuick
import Quickshell
import Quickshell.Widgets
import ".."

// app icon in a soft squircle tile (uniform sizes like the Windows dock icon pack)
// tiled: the Windows Mist tile for the app (Tiles.qml); the plain icon shows until the tile exists
Item {
    id: a
    property string icon
    property real size: 46
    property bool bare: false        // no tile behind, icon at full size
    property bool tiled: false
    implicitWidth: size; implicitHeight: size
    readonly property bool tileOk: tiled && tile.status === Image.Ready
    Rectangle {
        visible: !a.bare && !a.tiled
        anchors.fill: parent; radius: a.size * 0.28
        color: Qt.rgba(1, 1, 1, 0.07); border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.08)
    }
    Image {
        id: tile
        visible: a.tileOk
        anchors.centerIn: parent
        width: a.size; height: a.size
        source: a.tiled ? Tiles.url(a.icon) : ""
        sourceSize: Qt.size(Math.min(160, a.size * 2), Math.min(160, a.size * 2))
        cache: false; asynchronous: true; smooth: true; mipmap: true
        onStatusChanged: if (status === Image.Error) Tiles.need(a.icon)
    }
    IconImage {
        visible: !a.tileOk
        anchors.centerIn: parent
        implicitSize: a.bare || a.tiled ? a.size : a.size * 0.74
        source: Quickshell.iconPath(a.icon, "application-x-executable")
    }
}
