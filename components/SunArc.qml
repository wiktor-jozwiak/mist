import QtQuick
import ".."

// Daylight, as quiet as possible: sunrise time · a hairline with a dot for now · sunset time.
// At night the line runs from sunset to the next sunrise.
Item {
    id: arc
    property string rise: ""            // "07:00"
    property string set: ""             // "18:08"
    property date now: new Date()
    implicitWidth: row.implicitWidth; implicitHeight: 20

    function mins(s) { const p = (s || "0:0").split(":"); return +p[0] * 60 + +p[1] }
    readonly property int r0: mins(rise)
    readonly property int s0: mins(set)
    readonly property int n0: now.getHours() * 60 + now.getMinutes()
    readonly property bool day: n0 >= r0 && n0 < s0
    readonly property real p: day ? (n0 - r0) / Math.max(1, s0 - r0)
        : ((n0 - s0 + 1440) % 1440) / Math.max(1, 1440 - (s0 - r0))

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 12
        CTxt { anchors.verticalCenter: parent.verticalCenter; text: (arc.day ? arc.rise : arc.set).replace(/^0/, ""); size: Theme.fsMd; w: 500; color: Theme.t2; font.features: ({ "tnum": 1 }) }
        Item {
            anchors.verticalCenter: parent.verticalCenter
            width: 120; height: 6
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: 1; color: Qt.rgba(1, 1, 1, 0.22) }
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width * arc.p; height: 1; color: Qt.rgba(1, 1, 1, 0.7) }
            Rectangle { x: parent.width * arc.p - 3; anchors.verticalCenter: parent.verticalCenter; width: 6; height: 6; radius: 3; color: "white" }
        }
        CTxt { anchors.verticalCenter: parent.verticalCenter; text: (arc.day ? arc.set : arc.rise).replace(/^0/, ""); size: Theme.fsMd; w: 500; color: Theme.t2; font.features: ({ "tnum": 1 }) }
    }
}
