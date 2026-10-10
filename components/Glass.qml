import QtQuick
import ".."

// Mist popup surface. Default: liquid glass like the Dock / Start (LiquidGlass body under the content,
// its lit rim drawn again above the content; Hyprland blurs behind, layer rule "mist" ignore_alpha 0.25).
// tint keeps the old meaning (how dark the sheet is) and is mapped to a glass tint >= 0.28 so blur stays on.
// liquid: false = the old flat acrylic (Snap keeps it, Windows-like). clipContent clips the children only,
// so the shadow outside the sheet is not cut.
Item {
    id: g
    property real radius: Theme.rPanel
    property real tint: 0.62
    property bool liquid: true
    property real shine: 1.15
    property alias clipContent: inner.clip
    default property alias content: inner.data

    // ---- liquid
    LiquidGlass {
        visible: g.liquid
        anchors.fill: parent; radius: g.radius
        tint: 0.3 + g.tint * 0.2
        shine: g.shine; shadowOpacity: 0.2; rims: false
    }

    // ---- flat (old)
    Rectangle {
        visible: !g.liquid
        anchors.fill: parent; radius: g.radius
        color: Qt.rgba(11/255, 15/255, 24/255, g.tint)
        border.width: 1
        border.color: Theme.rim
        Rectangle {
            anchors.fill: parent; anchors.margins: 1; radius: parent.radius - 1
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.05) }
                GradientStop { position: 0.55; color: Qt.rgba(1, 1, 1, 0.01) }
                GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 0.025) }
            }
        }
    }

    Item { id: inner; anchors.fill: parent }

    LiquidGlass { visible: g.liquid; anchors.fill: parent; radius: g.radius; rimOnly: true; shine: g.shine }
}
