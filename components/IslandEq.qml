import QtQuick
import ".."

// 4-bar equalizer, stepped ~8x/s only while `run`
Item {
    id: eq
    property bool run: false
    implicitWidth: 18; implicitHeight: 14
    width: 18; height: 14
    Repeater {
        id: bars
        model: 4
        Rectangle {
            required property int index
            x: index * 5; width: 3; radius: 1.5; color: Theme.accent
            height: 4; y: eq.height - height
            Behavior on height { NumberAnimation { duration: 120 } }
        }
    }
    Timer {
        interval: 130; running: eq.run && eq.visible; repeat: true
        onTriggered: { for (let i = 0; i < bars.count; i++) bars.itemAt(i).height = 3 + Math.random() * 11 }
    }
}
