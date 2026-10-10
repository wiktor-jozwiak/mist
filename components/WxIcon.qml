import QtQuick
import ".."

// small filled weather icon like the forecast icons on Windows: sun / cloud / rain / snow composed from glyphs
Item {
    id: w
    property int code: 3
    property real size: 26
    implicitWidth: size * 1.3; implicitHeight: size
    readonly property bool sunVis: code <= 2
    readonly property bool cloudVis: code >= 2
    readonly property bool rain: (code >= 51 && code <= 67) || (code >= 80 && code <= 82) || code >= 95
    readonly property bool snow: (code >= 71 && code <= 77) || code === 85 || code === 86
    Text {
        visible: w.sunVis
        x: w.code === 2 ? w.size * 0.42 : w.size * 0.15; y: w.code === 2 ? -w.size * 0.12 : 0
        text: ""; font.family: Theme.iconFont; font.pixelSize: w.code === 2 ? w.size * 0.8 : w.size
        color: "#f6d27a"
    }
    Text {
        visible: w.cloudVis
        x: 0; y: w.code === 2 ? w.size * 0.12 : 0
        text: ""; font.family: Theme.iconFont; font.pixelSize: w.size * 1.08
        color: w.rain || w.code === 3 ? "#c9d1de" : "#e9eef6"
        style: Text.Raised; styleColor: Qt.rgba(0, 0, 0, 0.25)
    }
    Row {
        visible: w.rain || w.snow
        x: w.size * 0.3; y: w.size * 0.82; spacing: w.size * 0.14
        Repeater {
            model: 3
            Rectangle { width: w.snow ? 3 : 1.6; height: w.snow ? 3 : w.size * 0.25; radius: 1.5; rotation: w.snow ? 0 : 15; color: w.snow ? "white" : "#8fb8f0" }
        }
    }
    Rectangle {
        visible: w.code === 45 || w.code === 48
        y: w.size * 0.82; x: w.size * 0.1; width: w.size; height: 2; radius: 1; color: "#c9d1de"
    }
}
