import QtQuick
import ".."

Text {
    property real size: 13
    property int w: 450
    color: Theme.t1
    font.family: Theme.font
    font.pixelSize: size
    font.weight: w
    font.variableAxes: ({ "wght": w, "opsz": Math.max(8, Math.min(36, size)) })
    renderType: Text.QtRendering
    elide: Text.ElideRight
}
