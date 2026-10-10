import QtQuick
import QtQuick.Shapes

// soft radial light filling a rounded w x h sheet (Start passes its size; default 1080x790)
ShapePath {
    property real cx; property real cy; property real r; property color c; property real a
    property real w: 1080; property real h: 790; property real rr: 30
    strokeColor: "transparent"
    fillGradient: RadialGradient {
        centerX: cx; centerY: cy; focalX: cx; focalY: cy; centerRadius: r; focalRadius: 0
        GradientStop { position: 0; color: Qt.rgba(c.r, c.g, c.b, a) }
        GradientStop { position: 0.55; color: Qt.rgba(c.r, c.g, c.b, a * 0.35) }
        GradientStop { position: 1; color: Qt.rgba(c.r, c.g, c.b, 0) }
    }
    PathRectangle { x: 0; y: 0; width: w; height: h; radius: rr }
}
