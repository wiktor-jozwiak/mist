import QtQuick

// one view of the Dynamic Island: centred; like iOS the content comes in a beat after the shape starts
// to morph (fade + scale from 0.92), and leaves at once
Item {
    id: v
    property string name
    property string mode
    readonly property bool active: mode === name
    anchors.centerIn: parent
    width: implicitWidth; height: implicitHeight
    opacity: active ? 1 : 0
    scale: active ? 1 : 0.92
    visible: opacity > 0.01
    Behavior on opacity {
        SequentialAnimation {
            PauseAnimation { duration: v.active ? 110 : 0 }
            NumberAnimation { duration: v.active ? 220 : 80; easing.type: Easing.OutCubic }
        }
    }
    Behavior on scale { SpringAnimation { spring: 3.4; damping: 0.34; epsilon: 0.005 } }
}
