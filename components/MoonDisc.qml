import QtQuick

// The Moon's current phase as a small disc: lit part bounded by the terminator ellipse,
// dark part faintly visible (earthshine), soft halo. phase: 0 new, .25 first quarter, .5 full, .75 last.
// Seen from the northern hemisphere: waxing lit on the right.
Canvas {
    id: c
    property real phase: 0
    property real size: 20
    property bool halo: true
    implicitWidth: size + (halo ? size * 0.5 : 0); implicitHeight: implicitWidth
    onPhaseChanged: requestPaint()
    onSizeChanged: requestPaint()
    onPaint: {
        const x = getContext("2d"); x.reset()
        const cx = width / 2, cy = height / 2, r = c.size / 2
        const lit = (1 - Math.cos(2 * Math.PI * c.phase)) / 2          // illuminated fraction
        if (c.halo && lit > 0.05) {
            const g = x.createRadialGradient(cx, cy, r * 0.9, cx, cy, r * 1.5)
            g.addColorStop(0, Qt.rgba(1, 0.97, 0.88, 0.22 * lit)); g.addColorStop(1, Qt.rgba(1, 0.97, 0.88, 0))
            x.fillStyle = g; x.beginPath(); x.arc(cx, cy, r * 1.5, 0, 2 * Math.PI); x.fill()
        }
        // dark disc (earthshine)
        x.fillStyle = Qt.rgba(0.55, 0.62, 0.78, 0.22)
        x.beginPath(); x.arc(cx, cy, r, 0, 2 * Math.PI); x.fill()
        x.strokeStyle = Qt.rgba(0.85, 0.9, 1, 0.18); x.lineWidth = 0.8; x.stroke()
        if (lit < 0.005) return
        // lit region: outer half-circle on the lit side + terminator half-ellipse back
        const waxing = c.phase < 0.5
        const s = waxing ? 1 : -1                                       // lit side: +1 right, -1 left
        const rx = r * Math.cos(2 * Math.PI * c.phase) * s              // terminator bulge (+ = toward the lit side)
        const k = 0.5523
        x.beginPath()
        x.moveTo(cx, cy - r)
        x.bezierCurveTo(cx + s * r * k, cy - r, cx + s * r, cy - r * k, cx + s * r, cy)
        x.bezierCurveTo(cx + s * r, cy + r * k, cx + s * r * k, cy + r, cx, cy + r)
        x.bezierCurveTo(cx + rx * k, cy + r, cx + rx, cy + r * k, cx + rx, cy)
        x.bezierCurveTo(cx + rx, cy - r * k, cx + rx * k, cy - r, cx, cy - r)
        x.closePath()
        const g2 = x.createRadialGradient(cx - r * 0.3, cy - r * 0.35, r * 0.1, cx, cy, r)
        g2.addColorStop(0, "#fbf6e9"); g2.addColorStop(1, "#d9d2bf")
        x.fillStyle = g2; x.fill()
        // a few maria, clipped to the lit part
        x.save(); x.clip()
        x.fillStyle = Qt.rgba(0.45, 0.45, 0.42, 0.13)
        const spots = [[-0.28, -0.30, 0.30], [0.22, -0.30, 0.22], [0.32, 0.02, 0.24], [-0.52, 0.05, 0.30], [-0.12, 0.38, 0.18]]
        for (const p of spots) { x.beginPath(); x.arc(cx + p[0] * r, cy + p[1] * r, p[2] * r, 0, 2 * Math.PI); x.fill() }
        x.restore()
    }
}
