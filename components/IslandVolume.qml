import QtQuick
import ".."

// volume row of a Pipewire node: icon, bar, percent (muted = grey); `mic` = microphone icons
Item {
    id: vb
    property var node: null
    property bool mic: false
    readonly property real vol: node && node.audio ? node.audio.volume : 0
    readonly property bool mute: !node || !node.audio || node.audio.muted
    Icon {
        id: vi
        anchors.verticalCenter: parent.verticalCenter
        width: 20
        name: vb.mic ? (vb.mute || vb.vol === 0 ? "micOff" : "mic") : vb.mute || vb.vol === 0 ? "volumeOff" : vb.vol < 0.4 ? "volLow" : "volume"
        size: 17; color: vb.mute ? Theme.t3 : Theme.t1
    }
    Txt {
        id: vt
        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
        width: 46; horizontalAlignment: Text.AlignRight
        text: vb.mute ? "wycisz." : Math.round(vb.vol * 100) + "%"
        size: 12.5; w: 600; color: vb.mute ? Theme.t3 : vb.vol > 1.001 ? Theme.orange : Theme.t1
        font.features: ({ "tnum": 1 })
    }
    Rectangle {
        anchors.left: vi.right; anchors.right: vt.left; anchors.leftMargin: 10; anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        height: 6; radius: 3; color: Theme.fill2
        Rectangle {
            width: parent.width * Math.min(1, vb.vol); height: parent.height; radius: 3
            color: vb.mute ? Theme.t3 : vb.vol > 1.001 ? Theme.orange : Theme.t1
            Behavior on width { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
        }
    }
}
