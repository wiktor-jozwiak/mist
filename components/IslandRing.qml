import QtQuick
import ".."

// small progress ring (minutnik): `frac` 0..1 of the arc left, track in fill2
Canvas {
    id: r
    property real frac: 0
    property color c: Theme.orange
    property real line: 2.2
    implicitWidth: 15; implicitHeight: 15
    width: implicitWidth; height: implicitHeight
    onFracChanged: requestPaint()
    onCChanged: requestPaint()
    onPaint: {
        const g = getContext("2d"), w = width / 2, rad = w - line / 2
        g.reset()
        g.lineWidth = line
        g.strokeStyle = Theme.fill2
        g.beginPath(); g.arc(w, w, rad, 0, Math.PI * 2); g.stroke()
        g.strokeStyle = c
        g.lineCap = "round"
        g.beginPath(); g.arc(w, w, rad, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0.001, Math.min(1, frac))); g.stroke()
    }
}
