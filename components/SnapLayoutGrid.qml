import QtQuick
import ".."

// Snap Layouts grid (Snap.qml: drag panel and the maximize-button / Win+Z flyout)
Grid {
    id: lg
    property var layouts: []
    property var hot: null
    property int hotLayout: -1
    property bool interactive: false
    signal entered(var zone, int li)
    signal picked(var zone, int li)
    columns: 4; spacing: 12
    // zone under a point in grid coordinates (cells are laid out on a fixed 144 x 88 pitch)
    function hitAt(px, py) {
        for (let i = 0; i < layouts.length; i++) {
            const ox = (i % 4) * 144, oy = Math.floor(i / 4) * 88
            for (const z of layouts[i]) {
                const x = ox + 7 + z[0] * 122, y = oy + 7 + z[1] * 66, w = z[2] * 122 - 4, h = z[3] * 66 - 4
                if (px >= x - 2 && px <= x + w + 2 && py >= y - 2 && py <= y + h + 2) return { zone: z, li: i }
            }
        }
        return null
    }
    Repeater {
        model: lg.layouts
        Rectangle {
            id: cell
            required property var modelData
            required property int index
            width: 132; height: 76; radius: 12
            color: Qt.rgba(1, 1, 1, 0.04); border.width: 1; border.color: Theme.hair
            Repeater {
                model: cell.modelData
                Rectangle {
                    id: zr
                    required property var modelData
                    readonly property bool isHot: lg.hotLayout === cell.index && !!lg.hot && lg.hot.join() === modelData.join()
                    x: 5 + modelData[0] * 122 + 2; y: 5 + modelData[1] * 66 + 2
                    width: modelData[2] * 122 - 4; height: modelData[3] * 66 - 4
                    radius: 6
                    color: isHot ? Theme.accent : Qt.rgba(1, 1, 1, 0.12)
                    border.width: 1; border.color: isHot ? Theme.accentStrong : Qt.rgba(1, 1, 1, 0.08)
                    Behavior on color { ColorAnimation { duration: 90 } }
                    MouseArea {
                        anchors.fill: parent
                        enabled: lg.interactive
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onEntered: lg.entered(zr.modelData, cell.index)
                        onClicked: lg.picked(zr.modelData, cell.index)
                    }
                }
            }
        }
    }
}
