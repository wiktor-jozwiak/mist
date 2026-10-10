import QtQuick
import ".."

// rounded number + small unit in the same colour, like Fitness / Health
Row {
    id: n
    property string value
    property string unit
    property color c: Theme.t1
    property real size: 22
    spacing: 2
    Txt { id: nv; text: n.value; size: n.size; w: 650; color: n.c; font.family: Theme.fontRounded; font.features: ({ "tnum": 1 }) }
    Txt {
        visible: n.unit !== ""; anchors.baseline: nv.baseline
        text: n.unit; size: Math.round(n.size * 0.55); w: 650; color: n.c; opacity: 0.75
        font.family: Theme.fontRounded; font.capitalization: Font.AllUppercase
    }
}
