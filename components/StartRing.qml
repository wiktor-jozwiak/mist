import QtQuick
import ".."

// one stat for Start's computer card, in the desktop card style: value + unit, a thin load bar, caption.
// The bar turns amber / red only when the load is really high.
Item {
    id: rg
    property string label
    property var value
    property string unit: ""
    property real frac
    readonly property real f: Math.max(0, Math.min(1, frac))
    readonly property color c: f >= 0.9 ? Theme.mBad : f >= 0.75 ? Theme.mWarn : Theme.mAcc
    width: (parent.width - 32) / 3; height: 50
    Row {
        spacing: 3
        Txt { id: v; text: rg.value; size: 18; w: 550; color: Theme.mFg; font.features: ({ "tnum": 1 }) }
        Txt { text: rg.unit; size: Theme.fsXs; w: 500; color: Theme.mMuted; anchors.baseline: v.baseline; visible: text !== "" }
    }
    Rectangle {
        y: 28; width: parent.width; height: 3; radius: 1.5; color: Theme.mTrack
        Rectangle {
            width: Math.max(parent.height, parent.width * rg.f); height: parent.height; radius: 1.5; color: rg.c
            Behavior on width { NumberAnimation { duration: 500; easing.type: Easing.OutCubic } }
        }
    }
    Txt { y: 36; width: parent.width; text: rg.label; size: Theme.fsXs; w: 550; color: Theme.mMuted }
}
