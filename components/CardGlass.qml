import QtQuick
import ".."

// Desktop card: liquid glass (LiquidGlass) under the content, its rim drawn again above the content
// so a full-bleed child (the weather sky) still gets the lit edge. Radius 26, padding 24/22.
// Optional inner light (glow / glowAt): a soft coloured bloom inside the glass that gives a card its identity.
Item {
    id: g
    default property alias content: inner.data
    property real padL: 24
    property real padT: 22
    property real padR: 24
    property real padB: 22
    property real radius: 26
    property real tint: 0.34
    property color glow: "transparent"
    property point glowAt: Qt.point(0.2, 0.25)
    property real glowSize: 0.75
    implicitWidth: 344
    implicitHeight: inner.childrenRect.height + padT + padB

    LiquidGlass { anchors.fill: parent; radius: g.radius; tint: g.tint; rims: false; shadowOpacity: 0.34 }
    Canvas {
        anchors.fill: parent
        visible: g.glow.a > 0
        property color c: g.glow
        onCChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const x = getContext("2d"); x.reset()
            if (c.a <= 0 || width <= 0) return
            const cx = width * g.glowAt.x, cy = height * g.glowAt.y, r = Math.max(width, height) * g.glowSize
            const rgb = Math.round(c.r * 255) + "," + Math.round(c.g * 255) + "," + Math.round(c.b * 255)
            const gr = x.createRadialGradient(cx, cy, 0, cx, cy, r)
            gr.addColorStop(0, "rgba(" + rgb + "," + c.a + ")")
            gr.addColorStop(0.45, "rgba(" + rgb + "," + (c.a * 0.35) + ")")
            gr.addColorStop(1, "rgba(" + rgb + ",0)")
            x.beginPath(); x.roundedRect(1, 1, width - 2, height - 2, g.radius - 1, g.radius - 1); x.clip()
            x.fillStyle = gr; x.fillRect(0, 0, width, height)
        }
    }
    Item {
        id: inner
        x: g.padL; y: g.padT; width: g.width - g.padL - g.padR
        height: childrenRect.height
    }
    LiquidGlass { anchors.fill: parent; radius: g.radius; rimOnly: true; corners: true }
}
