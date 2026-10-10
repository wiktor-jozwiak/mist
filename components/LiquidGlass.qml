import QtQuick
import QtQuick.Shapes
import QtQuick.Effects
import ".."

// Liquid glass surface (Apple-style, continuous squircle corners): Hyprland blurs what is behind the layer (layer rule "mist",
// ignore_alpha 0.25 -> keep tint >= ~0.28 or the blur switches off), a thin tint on top, light falling
// from the top-left: lens rim bright on the lit and the opposite corner, a thick soft bevel, a specular
// line along the top and a faint reflection at the bottom.
// rimOnly: draw just the rim/bevel/specular (put a second copy above content that covers the glass).
Item {
    id: g
    property real radius: 26
    property real tint: 0.32
    property color tone: Qt.rgba(16/255, 22/255, 36/255, 1)
    property real shine: 1
    property bool shadow: true
    property real shadowOpacity: 0.22
    property bool rimOnly: false
    property bool rims: true          // false: body only (the rims come from a rimOnly copy on top)
    // corners: light only the top-left and the bottom-right corner (radial highlights fading along the edges);
    // the other two corners keep just a faint rim
    property bool corners: false
    readonly property real dk: corners ? 0 : 1          // diagonal-gradient rims
    readonly property real ck: corners ? 1 : 0          // corner highlights
    readonly property real cr: Math.max(60, Math.min(width, height) * 0.56)   // reach of a corner highlight

    readonly property real r: Math.min(radius, width / 2, height / 2)

    // Apple continuous corner ("squircle"): each corner is a quadrant of a superellipse (n = 4) spanning
    // 1.55 x the radius, so the curve eases into the straight edge instead of meeting it with a kink.
    // Returns an SVG subpath (several can be concatenated for odd-even rims).
    function sq(x, y, w, h, rr) {
        if (w <= 0 || h <= 0) return ""
        const e = Math.max(0.01, Math.min(rr * 1.55, w / 2, h / 2)), N = 14
        const cx = x + w / 2, cy = y + h / 2, q = [[1, 1], [-1, 1], [-1, -1], [1, -1]]
        let d = ""
        for (let k = 0; k < 4; k++) {
            const sx = q[k][0], sy = q[k][1]
            const ox = cx + sx * (w / 2 - e), oy = cy + sy * (h / 2 - e)
            for (let i = 0; i <= N; i++) {
                const t = (k % 2 === 0 ? i : N - i) / N * Math.PI / 2
                const px = ox + sx * e * Math.pow(Math.cos(t), 0.5), py = oy + sy * e * Math.pow(Math.sin(t), 0.5)
                d += (d === "" ? "M" : "L") + px.toFixed(2) + " " + py.toFixed(2) + " "
            }
        }
        return d + "Z "
    }

    RectangularShadow {
        visible: g.shadow && !g.rimOnly
        anchors.fill: parent; radius: g.r; offset.y: 14; blur: 30; spread: -6
        color: Qt.rgba(2/255, 5/255, 10/255, g.shadowOpacity)
    }

    // body + light fall-off (lit top, deeper bottom = thickness)
    Shape {
        visible: !g.rimOnly
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: "transparent"
            fillColor: Qt.rgba(g.tone.r, g.tone.g, g.tone.b, g.tint)
            PathSvg { path: g.sq(0, 0, g.width, g.height, g.r) }
        }
        ShapePath {
            strokeColor: "transparent"
            fillGradient: LinearGradient {
                x1: 0; y1: 0; x2: 0; y2: g.height
                GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, 0.085 * g.shine) }
                GradientStop { position: 0.32; color: Qt.rgba(1, 1, 1, 0.025 * g.shine) }
                GradientStop { position: 0.7; color: Qt.rgba(1, 1, 1, 0.0) }
                GradientStop { position: 1.0; color: Qt.rgba(1, 1, 1, 0.045 * g.shine) }
            }
            PathSvg { path: g.sq(0, 0, g.width, g.height, g.r) }
        }
    }

    Shape {
        visible: g.rims
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        // soft bevel: 7 px inner band, diagonal light
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillGradient: LinearGradient {
                x1: 0; y1: 0; x2: g.width; y2: g.height
                GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, 0.075 * g.shine * g.dk) }
                GradientStop { position: 0.35; color: Qt.rgba(1, 1, 1, 0.015 * g.shine * g.dk) }
                GradientStop { position: 0.7; color: Qt.rgba(1, 1, 1, 0.0) }
                GradientStop { position: 1.0; color: Qt.rgba(1, 1, 1, 0.05 * g.shine * g.dk) }
            }
            PathSvg { path: g.sq(1, 1, g.width - 2, g.height - 2, Math.max(0, g.r - 1)) }
            PathSvg { path: g.sq(7, 7, Math.max(0, g.width - 14), Math.max(0, g.height - 14), Math.max(0, g.r - 7)) }
        }
        // lens rim: 1 px, bright top-left, dim middle, lit again bottom-right
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillGradient: LinearGradient {
                x1: 0; y1: 0; x2: g.width; y2: g.height
                GradientStop { position: 0.0; color: Qt.rgba(236/255, 243/255, 1, 0.62 * g.shine * g.dk) }
                GradientStop { position: 0.22; color: Qt.rgba(236/255, 243/255, 1, 0.20 * g.shine * g.dk) }
                GradientStop { position: 0.55; color: Qt.rgba(236/255, 243/255, 1, 0.06 * g.shine * g.dk) }
                GradientStop { position: 0.85; color: Qt.rgba(236/255, 243/255, 1, 0.16 * g.shine * g.dk) }
                GradientStop { position: 1.0; color: Qt.rgba(236/255, 243/255, 1, 0.40 * g.shine * g.dk) }
            }
            PathSvg { path: g.sq(0, 0, g.width, g.height, g.r) }
            PathSvg { path: g.sq(1, 1, g.width - 2, g.height - 2, Math.max(0, g.r - 1)) }
        }
        // specular line along the top inner edge + faint bottom reflection
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillGradient: LinearGradient {
                x1: 0; y1: 0; x2: 0; y2: g.height
                GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, 0.30 * g.shine * g.dk) }
                GradientStop { position: Math.min(0.45, 14 / Math.max(1, g.height)); color: Qt.rgba(1, 1, 1, 0.0) }
                GradientStop { position: Math.max(0.55, 1 - 14 / Math.max(1, g.height)); color: Qt.rgba(1, 1, 1, 0.0) }
                GradientStop { position: 1.0; color: Qt.rgba(1, 1, 1, 0.10 * g.shine * g.dk) }
            }
            PathSvg { path: g.sq(1, 1, g.width - 2, g.height - 2, Math.max(0, g.r - 1)) }
            PathSvg { path: g.sq(2, 2.2, g.width - 4, g.height - 4.2, Math.max(0, g.r - 2)) }
        }

        // ---- corners mode
        // faint rim all around
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillColor: Qt.rgba(236/255, 243/255, 1, 0.06 * g.shine * g.ck)
            PathSvg { path: g.sq(0, 0, g.width, g.height, g.r) }
            PathSvg { path: g.sq(1, 1, g.width - 2, g.height - 2, Math.max(0, g.r - 1)) }
        }
        // top-left: bright lens rim + soft bevel
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillGradient: RadialGradient {
                centerX: g.r * 0.6; centerY: g.r * 0.6; centerRadius: g.cr; focalX: centerX; focalY: centerY
                GradientStop { position: 0.0; color: Qt.rgba(240/255, 246/255, 1, 0.95 * g.shine * g.ck) }
                GradientStop { position: 0.45; color: Qt.rgba(240/255, 246/255, 1, 0.32 * g.shine * g.ck) }
                GradientStop { position: 1.0; color: Qt.rgba(240/255, 246/255, 1, 0.0) }
            }
            PathSvg { path: g.sq(0, 0, g.width, g.height, g.r) }
            PathSvg { path: g.sq(1.2, 1.2, g.width - 2.4, g.height - 2.4, Math.max(0, g.r - 1.2)) }
        }
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillGradient: RadialGradient {
                centerX: g.r * 0.6; centerY: g.r * 0.6; centerRadius: g.cr * 0.9; focalX: centerX; focalY: centerY
                GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, 0.10 * g.shine * g.ck) }
                GradientStop { position: 1.0; color: Qt.rgba(1, 1, 1, 0.0) }
            }
            PathSvg { path: g.sq(1, 1, g.width - 2, g.height - 2, Math.max(0, g.r - 1)) }
            PathSvg { path: g.sq(6, 6, Math.max(0, g.width - 12), Math.max(0, g.height - 12), Math.max(0, g.r - 6)) }
        }
        // bottom-right: the light leaving the glass, a little dimmer
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillGradient: RadialGradient {
                centerX: g.width - g.r * 0.6; centerY: g.height - g.r * 0.6; centerRadius: g.cr; focalX: centerX; focalY: centerY
                GradientStop { position: 0.0; color: Qt.rgba(240/255, 246/255, 1, 0.62 * g.shine * g.ck) }
                GradientStop { position: 0.45; color: Qt.rgba(240/255, 246/255, 1, 0.22 * g.shine * g.ck) }
                GradientStop { position: 1.0; color: Qt.rgba(240/255, 246/255, 1, 0.0) }
            }
            PathSvg { path: g.sq(0, 0, g.width, g.height, g.r) }
            PathSvg { path: g.sq(1.2, 1.2, g.width - 2.4, g.height - 2.4, Math.max(0, g.r - 1.2)) }
        }
        ShapePath {
            strokeColor: "transparent"; fillRule: ShapePath.OddEvenFill
            fillGradient: RadialGradient {
                centerX: g.width - g.r * 0.6; centerY: g.height - g.r * 0.6; centerRadius: g.cr * 0.9; focalX: centerX; focalY: centerY
                GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, 0.07 * g.shine * g.ck) }
                GradientStop { position: 1.0; color: Qt.rgba(1, 1, 1, 0.0) }
            }
            PathSvg { path: g.sq(1, 1, g.width - 2, g.height - 2, Math.max(0, g.r - 1)) }
            PathSvg { path: g.sq(6, 6, Math.max(0, g.width - 12), Math.max(0, g.height - 12), Math.max(0, g.r - 6)) }
        }
    }
}
